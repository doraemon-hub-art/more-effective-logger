//! System statistics for the status bar (Linux: /proc and /sys).

/*
 * @file sysstat.rs
 * @brief System statistics for the status bar: CPU, memory, temperature, uptime
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-01
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 *
 * Everything here is read straight from /proc and /sys, so it works without any
 * extra dependency and without root. Nothing is fatal: a value that cannot be
 * read is reported as 0 / None instead of failing the whole sample.
 */

use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Event carrying one sample; pushed once per second.
pub const EVT_STATS: &str = "sys-stats";

/// Sampling interval.
const INTERVAL: Duration = Duration::from_secs(1);

/// One sample of the machine's state, as shown in the status bar.
#[derive(Clone, Serialize)]
pub struct SysStats {
    /// CPU usage over the last interval, percent
    pub cpu_percent: f32,
    /// Package temperature in Celsius; None when the machine exposes no sensor
    pub temp_c: Option<f32>,
    /// Memory in use (total - available), bytes
    pub mem_used_bytes: u64,
    /// Installed memory, bytes
    pub mem_total_bytes: u64,
    /// Memory in use, percent
    pub mem_percent: f32,
    /// Machine uptime, seconds
    pub uptime_secs: u64,
    /// How long this app has been running, seconds
    pub app_uptime_secs: u64,
}

/// Sample the machine and push it to the frontend, once a second, until exit.
pub fn start_loop(app: AppHandle) {
    thread::spawn(move || {
        // CPU usage needs two samples, so take the baseline right away.
        let mut previous = cpu_sample();
        loop {
            thread::sleep(INTERVAL);
            let current = cpu_sample();
            let (mem_used_bytes, mem_total_bytes) = memory_usage();
            let stats = SysStats {
                cpu_percent: cpu_percent(previous, current),
                temp_c: read_temp_c(),
                mem_used_bytes,
                mem_total_bytes,
                mem_percent: percent(mem_used_bytes, mem_total_bytes),
                uptime_secs: read_uptime_secs(),
                app_uptime_secs: app_uptime().as_secs(),
            };
            previous = current;
            let _ = app.emit(EVT_STATS, stats);
        }
    });
}

/// How long this process has been alive.
fn app_uptime() -> Duration {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed()
}

/// Memory in use and installed memory, in bytes: exactly what the status bar shows.
fn memory_usage() -> (u64, u64) {
    let (total, available) = memory().unwrap_or((0, 0));
    (total.saturating_sub(available), total)
}

/// Total and available memory in bytes, from /proc/meminfo.
fn memory() -> Option<(u64, u64)> {
    let info = std::fs::read_to_string("/proc/meminfo").ok()?;
    let mut total_kb = None;
    let mut available_kb = None;
    for line in info.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            total_kb = parse_first_number(rest);
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            available_kb = parse_first_number(rest);
        }
        if total_kb.is_some() && available_kb.is_some() {
            break;
        }
    }
    Some((total_kb? * 1024, available_kb? * 1024))
}

/// Cumulative CPU counters from /proc/stat, as (idle+jobs-waiting, total).
fn cpu_sample() -> Option<(u64, u64)> {
    let stat = std::fs::read_to_string("/proc/stat").ok()?;
    let line = stat.lines().next()?;
    let mut fields = line.split_whitespace();
    if fields.next()? != "cpu" {
        return None;
    }
    let values: Vec<u64> = fields.filter_map(|value| value.parse().ok()).collect();
    // user nice system idle iowait ...
    if values.len() < 4 {
        return None;
    }
    let idle = values[3] + values.get(4).copied().unwrap_or(0);
    Some((idle, values.iter().sum()))
}

/// CPU usage between two samples, percent.
fn cpu_percent(previous: Option<(u64, u64)>, current: Option<(u64, u64)>) -> f32 {
    let (Some((prev_idle, prev_total)), Some((idle, total))) = (previous, current) else {
        return 0.0;
    };
    let total_delta = total.saturating_sub(prev_total);
    if total_delta == 0 {
        return 0.0;
    }
    let idle_delta = idle.saturating_sub(prev_idle);
    let busy = 100.0 * (1.0 - idle_delta as f64 / total_delta as f64);
    busy.clamp(0.0, 100.0) as f32
}

/// Machine uptime in seconds, from /proc/uptime.
fn read_uptime_secs() -> u64 {
    std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|raw| parse_first_float(&raw))
        .map_or(0, |secs| secs as u64)
}

/// Temperature in Celsius, from the first CPU-ish thermal zone that reads cleanly.
fn read_temp_c() -> Option<f32> {
    let mut fallback = None;
    for zone in 0..16 {
        let dir = format!("/sys/class/thermal/thermal_zone{zone}");
        let Ok(raw) = std::fs::read_to_string(format!("{dir}/temp")) else {
            continue;
        };
        let Ok(milli) = raw.trim().parse::<i64>() else {
            continue;
        };
        let celsius = milli as f32 / 1000.0;
        if !(-50.0..=150.0).contains(&celsius) {
            continue;
        }
        let zone_type = std::fs::read_to_string(format!("{dir}/type")).unwrap_or_default();
        let zone_type = zone_type.trim().to_ascii_lowercase();
        if zone_type.contains("pkg")
            || zone_type.contains("core")
            || zone_type.contains("cpu")
            || zone_type.contains("x86")
        {
            return Some(celsius);
        }
        fallback.get_or_insert(celsius);
    }
    fallback
}

/// Percent of `total` taken by `part`; 0 when total is unknown.
fn percent(part: u64, total: u64) -> f32 {
    if total == 0 {
        return 0.0;
    }
    (part as f64 * 100.0 / total as f64) as f32
}

/// First whitespace-separated number in the text.
fn parse_first_number(text: &str) -> Option<u64> {
    text.split_whitespace().next()?.parse().ok()
}

/// First whitespace-separated float in the text.
fn parse_first_float(text: &str) -> Option<f64> {
    text.split_whitespace().next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_is_plausible() {
        let (total, available) = memory().expect("/proc/meminfo must be readable on linux");
        assert!(
            total > 512 * 1024 * 1024,
            "installed memory looks too small: {total} bytes"
        );
        assert!(
            available <= total,
            "available memory {available} exceeds total {total}"
        );
    }

    #[test]
    fn used_memory_is_between_zero_and_total() {
        let (used, total) = memory_usage();
        assert!(
            total > 512 * 1024 * 1024,
            "installed memory looks too small: {total} bytes"
        );
        assert!(
            used > 0,
            "used memory must not be zero on a running machine"
        );
        assert!(
            used < total,
            "used memory {used} must stay below total {total}"
        );
    }

    #[test]
    fn memory_percent_is_in_range() {
        let (total, available) = memory().expect("/proc/meminfo must be readable on linux");
        let used = total - available;
        let used_percent = percent(used, total);
        assert!(
            (0.0..=100.0).contains(&used_percent),
            "memory usage out of range: {used_percent}"
        );
    }

    #[test]
    fn cpu_usage_is_in_range() {
        let first = cpu_sample().expect("/proc/stat must be readable on linux");
        thread::sleep(Duration::from_millis(150));
        let second = cpu_sample().expect("/proc/stat must be readable on linux");
        let usage = cpu_percent(Some(first), Some(second));
        assert!(
            (0.0..=100.0).contains(&usage),
            "cpu usage out of range: {usage}"
        );
    }

    #[test]
    fn missing_cpu_samples_report_zero() {
        assert_eq!(cpu_percent(None, None), 0.0);
    }

    #[test]
    fn uptime_is_positive() {
        assert!(read_uptime_secs() > 0);
    }

    #[test]
    fn temperature_is_sane_when_present() {
        if let Some(celsius) = read_temp_c() {
            assert!(
                (-50.0..=150.0).contains(&celsius),
                "temperature out of range: {celsius}"
            );
        }
    }
}
