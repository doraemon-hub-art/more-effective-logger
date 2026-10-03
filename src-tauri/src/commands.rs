//! IPC command layer
//!
//! All `#[tauri::command]` handlers are defined here, grouped by domain:
//! - pty.rs: terminal sessions
//! - ssh.rs: remote connections
//! - logger.rs: log panels

/*
 * @file commands.rs
 * @brief IPC command layer: app_ping test handler + terminal (pty) commands
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-08-23
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 */
use crate::modules::pty;
use crate::modules::serial;
use crate::state::AppState;
use tauri::{AppHandle, State};

/// Minimal test handler: verifies frontend-backend IPC connectivity
#[tauri::command]
pub async fn app_ping(state: State<'_, AppState>) -> Result<String, String> {
    // Also verify global state is mounted (lock the placeholder field)
    let _guard = state.placeholder.lock().map_err(|e| e.to_string())?;
    Ok("Pong from Rust!".into())
}

/// Spawn a shell in a new pty under the given pane id
#[tauri::command]
pub async fn spawn_terminal(
    app: AppHandle,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<pty::SessionInfo, String> {
    pty::spawn(&app, &id, cols, rows)
}

/// Send keystrokes from the frontend to the shell
#[tauri::command]
pub async fn pty_input(state: State<'_, AppState>, id: String, data: String) -> Result<(), String> {
    let mut sessions = state.ptys.lock().map_err(|e| e.to_string())?;
    let session = sessions
        .get_mut(&id)
        .ok_or_else(|| format!("unknown terminal: {id}"))?;
    session.write(data.as_bytes())
}

/// Resize the pty after the pane changed size
#[tauri::command]
pub async fn pty_resize(
    state: State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let sessions = state.ptys.lock().map_err(|e| e.to_string())?;
    let session = sessions
        .get(&id)
        .ok_or_else(|| format!("unknown terminal: {id}"))?;
    session.resize(cols, rows)
}

/// Terminate the shell behind a pane (the reader thread then drops the session)
#[tauri::command]
pub async fn pty_kill(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut sessions = state.ptys.lock().map_err(|e| e.to_string())?;
    match sessions.get_mut(&id) {
        Some(session) => session.kill(),
        None => Ok(()),
    }
}

/// Working directory of a session's shell, for the pane mirror in the top bar
#[tauri::command]
pub async fn terminal_cwd(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<String>, String> {
    let sessions = state.ptys.lock().map_err(|e| e.to_string())?;
    let session = sessions
        .get(&id)
        .ok_or_else(|| format!("unknown terminal: {id}"))?;
    Ok(session.cwd())
}

/// Scan the machine's serial ports for the picker (reads /sys and /proc only)
#[tauri::command]
pub async fn serial_list() -> Result<Vec<serial::PortInfo>, String> {
    Ok(serial::list())
}

/// Open a serial port under the given pane id
#[tauri::command]
pub async fn serial_open(
    app: AppHandle,
    id: String,
    device: String,
    baud: u32,
) -> Result<serial::OpenInfo, String> {
    serial::open(&app, &id, &device, baud)
}

/// Send raw bytes out of an open port
#[tauri::command]
pub async fn serial_write(
    state: State<'_, AppState>,
    id: String,
    data: Vec<u8>,
) -> Result<(), String> {
    serial::write(&state, &id, &data)
}

/// Stop the reader thread of a port; it closes the fd and emits the exit event
#[tauri::command]
pub async fn serial_close(state: State<'_, AppState>, id: String) -> Result<(), String> {
    serial::close(&state, &id)
}
