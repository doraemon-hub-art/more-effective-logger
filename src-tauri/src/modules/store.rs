//! Small JSON files in the app's config directory
//!
//! Two of them: the settings (`settings.json`, small, meant to be read and edited by
//! hand) and the session (`session.json`, machine-written, the layout to come back to).
//! This module only moves bytes: the shape of a file is the frontend's business, so
//! adding a field never means touching the backend.

/*
 * @file store.rs
 * @brief Read and write the app's JSON files under its config directory
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-04
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 */

use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// The settings the app runs with.
pub const SETTINGS: &str = "settings.json";
/// The layout and working directories to restore on the next start.
pub const SESSION: &str = "session.json";

/// Where a config file lives; the directory itself is created by the first write.
fn path(app: &AppHandle, file: &str) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(dir.join(file))
}

/// The file as the frontend left it; `None` when it does not exist yet. A file that does
/// not parse is an error, so a broken one is never half-applied.
pub fn load(app: &AppHandle, file: &str) -> Result<Option<Value>, String> {
    let path = path(app, file)?;
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Write the file out, creating the config directory on the way. Pretty-printed and
/// newline-terminated, because this is a file people read by hand.
pub fn save(app: &AppHandle, file: &str, value: &Value) -> Result<(), String> {
    let path = path(app, file)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(&path, format!("{text}\n")).map_err(|e| format!("{}: {e}", path.display()))
}
