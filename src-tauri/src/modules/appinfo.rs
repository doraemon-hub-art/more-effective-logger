//! What the settings page reports about this build
//!
//! Three facts and nothing else: the version from the manifest, the platform the binary
//! runs on, and where the app keeps its files. None of them can be set, so they live
//! apart from the settings the app owns and writes.

/*
 * @file appinfo.rs
 * @brief Build and platform facts reported by the settings page
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-24
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 */
use serde::Serialize;
use tauri::{AppHandle, Manager};

/// The facts the settings page draws, all as ready-to-show strings.
#[derive(Serialize)]
pub struct Info {
    /// Version from `src-tauri/Cargo.toml`, compiled into the binary.
    pub version: String,
    /// `os · arch · libc`, e.g. `linux · x86_64 · glibc 2.35`.
    pub platform: String,
    /// Where settings.json, session.json and melogger.log live, home shortened to `~`.
    pub config_dir: String,
}

/// Collect the facts; only the directory can fail, and it comes back empty then.
pub fn info(app: &AppHandle) -> Info {
    Info {
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: platform(),
        config_dir: app
            .path()
            .app_config_dir()
            .map(|dir| shorten_home(&dir))
            .unwrap_or_default(),
    }
}

/// `os · arch · libc`. The libc half only exists under glibc; other targets report no more.
fn platform() -> String {
    let text = format!("{} · {}", std::env::consts::OS, std::env::consts::ARCH);
    #[cfg(target_env = "gnu")]
    let text = format!("{text} · glibc {}", glibc_version());
    text
}

/// Version of the running C library, from the library itself.
#[cfg(target_env = "gnu")]
fn glibc_version() -> String {
    // The pointer is a static string owned by libc, never freed.
    let raw = unsafe { libc::gnu_get_libc_version() };
    if raw.is_null() {
        return "?".to_string();
    }
    unsafe { std::ffi::CStr::from_ptr(raw) }
        .to_string_lossy()
        .into_owned()
}

/// A path under the home directory reads as `~/…`, so the row stays short.
fn shorten_home(path: &std::path::Path) -> String {
    let shown = path.display().to_string();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() => match shown.strip_prefix(&home) {
            Some(rest) => format!("~{rest}"),
            None => shown,
        },
        _ => shown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The row reads `os · arch`, with the libc part added where the target has one.
    #[test]
    fn platform_names_os_and_arch() {
        let text = platform();
        assert!(text.starts_with(std::env::consts::OS));
        assert!(text.contains(std::env::consts::ARCH));
        assert_eq!(
            text.matches(" · ").count(),
            if cfg!(target_env = "gnu") { 2 } else { 1 }
        );
    }

    /// Only a path inside the home directory loses its prefix.
    #[test]
    fn home_prefix_becomes_a_tilde() {
        let home = std::env::var("HOME").expect("HOME is set while testing");
        let inside = std::path::Path::new(&home).join(".config/com.melogger.app");
        assert_eq!(shorten_home(&inside), "~/.config/com.melogger.app");
        let outside = std::path::Path::new("/etc/melogger");
        assert_eq!(shorten_home(outside), "/etc/melogger");
    }
}
