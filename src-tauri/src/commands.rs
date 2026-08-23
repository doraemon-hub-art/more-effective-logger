//! IPC command layer
//!
//! All `#[tauri::command]` handlers are defined here, grouped by domain:
//! - pty.rs: terminal sessions
//! - ssh.rs: remote connections
//! - logger.rs: log panels

/*
 * @file commands.rs
 * @brief IPC command layer: minimal app_ping test handler
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-08-23
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 */
use crate::state::AppState;
use tauri::State;

/// Minimal test handler: verifies frontend-backend IPC connectivity
#[tauri::command]
pub async fn app_ping(state: State<'_, AppState>) -> Result<String, String> {
    // Also verify global state is mounted (lock the placeholder field)
    let _guard = state.placeholder.lock().map_err(|e| e.to_string())?;
    Ok("Pong from Rust!".into())
}
