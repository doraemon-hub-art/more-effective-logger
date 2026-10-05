//! The app's own log file.
//!
//! One file holds a whole run: the backend writes to it directly, the frontend hands its
//! lines over through the `log_write` command, and both land in the same file in the order
//! they happened. Every start truncates it, so what is in there is always the current
//! session and never a pile of older runs.
//!
//! Nothing here is fatal: a log that cannot be opened costs the file, not the app.

/*
 * @file logfile.rs
 * @brief Runtime log: one line per event, into a file each start rewrites
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-05
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 */

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use tauri::{AppHandle, Manager};

/// Name of the log inside the app's config directory.
pub const FILE: &str = "melogger.log";

/// Severity of one line, as the word in its column.
#[derive(Clone, Copy)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    /// Five characters, so the level column lines up whatever the level is.
    fn label(self) -> &'static str {
        match self {
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }

    /// The level the frontend named; anything unrecognized is information.
    fn of(name: &str) -> Level {
        match name.to_ascii_lowercase().as_str() {
            "debug" => Level::Debug,
            "warn" => Level::Warn,
            "error" => Level::Error,
            _ => Level::Info,
        }
    }
}

/// One line the frontend logged.
#[derive(Deserialize)]
pub struct Entry {
    pub level: String,
    pub target: String,
    pub message: String,
}

/// The open log file; unset when it could not be created, and then only stdout is left.
static SINK: OnceLock<Mutex<File>> = OnceLock::new();

/// Where the log is written.
pub fn path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(dir.join(FILE))
}

/// Open the log for this run, dropping what the run before left behind. Called once, ahead
/// of everything else in `setup`; a failure is reported and the app carries on.
pub fn init(app: &AppHandle) -> Result<PathBuf, String> {
    let path = path(app)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let file = File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let _ = SINK.set(Mutex::new(file));
    Ok(path)
}

/// Send panics into the file as well: a crash is the one thing the log must not miss. The
/// default hook stays in place, its stderr output is still wanted.
pub fn hook_panics() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        error("panic", info.to_string());
        default(info);
    }));
}

/// One line at debug level.
pub fn debug(target: &str, message: impl AsRef<str>) {
    write(Level::Debug, target, message);
}

/// One line at information level: the things a normal run is made of.
pub fn info(target: &str, message: impl AsRef<str>) {
    write(Level::Info, target, message);
}

/// One line at warning level: something went wrong and was worked around.
pub fn warn(target: &str, message: impl AsRef<str>) {
    write(Level::Warn, target, message);
}

/// One line at error level: something failed.
pub fn error(target: &str, message: impl AsRef<str>) {
    write(Level::Error, target, message);
}

/// Write one line: into the file, and onto stdout, so a dev build still shows it in the
/// terminal it was started from.
fn write(level: Level, target: &str, message: impl AsRef<str>) {
    let line = line(level, target, message.as_ref());
    println!("{line}");
    let Some(sink) = SINK.get() else {
        return;
    };
    // A panic that left the lock poisoned must not cost the rest of the log.
    let mut file = sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ = writeln!(file, "{line}");
}

/// One formatted line: when, how bad, where from, what happened. The level keeps five
/// characters and the target eight, so the columns line up down the file.
fn line(level: Level, target: &str, message: &str) -> String {
    format!(
        "{} {:<5} {:<8} {message}",
        timestamp(),
        level.label(),
        target
    )
}

/// Lines handed over by the frontend, written in the order it logged them.
pub fn write_batch(entries: &[Entry]) {
    for entry in entries {
        write(Level::of(&entry.level), &entry.target, &entry.message);
    }
}

/// Local wall clock down to the millisecond: `2026-10-05 18:40:12.123`.
///
/// The conversion goes through libc rather than a date crate: libc is already a dependency
/// here, and the only thing needed from it is the local time offset.
fn timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let secs = now.as_secs() as libc::time_t;
    unsafe { libc::localtime_r(&secs, &mut tm) };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec,
        now.subsec_millis()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 23 characters, digits where digits go and separators where they go.
    #[test]
    fn timestamp_shape() {
        let stamp = timestamp();
        assert_eq!(stamp.len(), 23, "{stamp}");
        let wrong = stamp.char_indices().find(|(at, ch)| match at {
            4 | 7 => *ch != '-',
            10 => *ch != ' ',
            13 | 16 => *ch != ':',
            19 => *ch != '.',
            _ => !ch.is_ascii_digit(),
        });
        assert!(wrong.is_none(), "{stamp}");
    }

    /// The level keeps its width and the target keeps its own, so the columns stay put.
    #[test]
    fn columns() {
        let text = line(Level::Warn, "serial", "write pane-1 failed");
        assert!(text.starts_with(&timestamp()[..10]), "{text}");
        assert!(
            text.ends_with("WARN  serial   write pane-1 failed"),
            "{text}"
        );
    }

    /// The frontend names the level in a string; anything it does not name is information.
    #[test]
    fn level_names() {
        assert_eq!(Level::of("debug").label(), "DEBUG");
        assert_eq!(Level::of("WARN").label(), "WARN");
        assert_eq!(Level::of("error").label(), "ERROR");
        assert_eq!(Level::of("").label(), "INFO");
        assert_eq!(Level::of("chatty").label(), "INFO");
    }
}
