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
use crate::modules::fonts;
use crate::modules::logfile;
use crate::modules::logger;
use crate::modules::pty;
use crate::modules::serial;
use crate::modules::store;
use crate::state::AppState;
use serde_json::Value;
use tauri::{AppHandle, Manager, State};

/// Minimal test handler: verifies frontend-backend IPC connectivity
#[tauri::command]
pub async fn app_ping(state: State<'_, AppState>) -> Result<String, String> {
    // Also verify global state is mounted (lock the placeholder field)
    let _guard = state.placeholder.lock().map_err(|e| e.to_string())?;
    Ok("Pong from Rust!".into())
}

/// Lines the frontend logged (its console output and uncaught errors), landing in the same
/// file as the backend's own lines
#[tauri::command]
pub async fn log_write(lines: Vec<logfile::Entry>) {
    logfile::write_batch(&lines);
}

/// Spawn a shell in a new pty under the given pane id; `cwd` is where a restored pane
/// left its shell, absent for a fresh one
#[tauri::command]
pub async fn spawn_terminal(
    app: AppHandle,
    id: String,
    cols: u16,
    rows: u16,
    cwd: Option<String>,
) -> Result<pty::SessionInfo, String> {
    pty::spawn(&app, &id, cols, rows, cwd.as_deref())
}

/// Apply the whole-interface zoom level from the settings page (browser-style page zoom)
#[tauri::command]
pub async fn set_zoom(app: AppHandle, scale: f64) -> Result<(), String> {
    logfile::debug("ui", format!("zoom {scale}"));
    let webview = app
        .get_webview_window("main")
        .ok_or_else(|| "no main window".to_string())?;
    webview.set_zoom(scale).map_err(|e| e.to_string())
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
        Some(session) => {
            logfile::info("pty", format!("killing {id}"));
            session.kill()
        }
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

/// List installed monospace families for the settings font picker (fontconfig scan)
#[tauri::command]
pub async fn font_list() -> Result<Vec<String>, String> {
    Ok(fonts::list_mono())
}

/// Read the settings file; null when it does not exist yet
#[tauri::command]
pub async fn settings_load(app: AppHandle) -> Result<Option<Value>, String> {
    store::load(&app, store::SETTINGS)
}

/// Write the settings file (creating it, and its directory, on the first call)
#[tauri::command]
pub async fn settings_save(app: AppHandle, settings: Value) -> Result<(), String> {
    store::save(&app, store::SETTINGS, &settings)
}

/// Read the session file (the layout to come back to); null when there is none
#[tauri::command]
pub async fn session_load(app: AppHandle) -> Result<Option<Value>, String> {
    store::load(&app, store::SESSION)
}

/// Write the session file
#[tauri::command]
pub async fn session_save(app: AppHandle, session: Value) -> Result<(), String> {
    store::save(&app, store::SESSION, &session)
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

/// Read a local log file once: the pane freezes on what the file held at this moment
#[tauri::command]
pub async fn log_open_file(path: String) -> Result<logger::Snapshot, String> {
    logger::read_snapshot(&path)
}

/// Completion candidates for the path typed so far (the pane's local file field)
#[tauri::command]
pub async fn log_complete(prefix: String) -> Result<Vec<logger::Candidate>, String> {
    Ok(logger::complete(&prefix))
}

/// Follow a remote file over SSH under the given pane id: whole lines arrive as
/// `log-data` events, the connection state as `log-state`, the end as `log-exit`
#[tauri::command]
pub async fn log_connect(
    app: AppHandle,
    id: String,
    host: String,
    user: String,
    password: String,
    port: u16,
    path: String,
) -> Result<(), String> {
    logger::connect(&app, &id, &host, port, &user, &password, &path)
}

/// Stop the follow behind a pane; its thread closes the session and emits `log-exit`
#[tauri::command]
pub async fn log_close(state: State<'_, AppState>, id: String) -> Result<(), String> {
    logger::close(&state, &id)
}

/// One argument of a bridged call, under the name the frontend sent.
fn arg<'a>(args: &'a Value, key: &str) -> Result<&'a Value, String> {
    args.get(key)
        .ok_or_else(|| format!("missing argument: {key}"))
}

fn arg_str(args: &Value, key: &str) -> Result<String, String> {
    arg(args, key)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("argument {key} is not a string"))
}

fn arg_optional_str(args: &Value, key: &str) -> Result<Option<String>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(|text| Some(text.to_string()))
            .ok_or_else(|| format!("argument {key} is not a string")),
    }
}

fn arg_number<T: TryFrom<u64>>(args: &Value, key: &str) -> Result<T, String> {
    arg(args, key)?
        .as_u64()
        .and_then(|number| T::try_from(number).ok())
        .ok_or_else(|| format!("argument {key} is not a valid number"))
}

fn arg_scale(args: &Value, key: &str) -> Result<f64, String> {
    arg(args, key)?
        .as_f64()
        .ok_or_else(|| format!("argument {key} is not a number"))
}

fn arg_value<T: serde::de::DeserializeOwned>(args: &Value, key: &str) -> Result<T, String> {
    serde_json::from_value(arg(args, key)?.clone())
        .map_err(|error| format!("argument {key}: {error}"))
}

/// A command's result in the shape the bridge returns; `()` becomes null.
fn answer<T: serde::Serialize>(result: Result<T, String>) -> Result<Value, String> {
    result.and_then(|value| serde_json::to_value(value).map_err(|error| error.to_string()))
}

/// Run one command by name for a browser client. This mirrors the `generate_handler!` list
/// in lib.rs: every command registered there has an arm here.
#[cfg(debug_assertions)]
pub async fn devbridge_call(app: AppHandle, cmd: String, args: Value) -> Result<Value, String> {
    let state = app.state::<AppState>();
    match cmd.as_str() {
        "app_ping" => answer(app_ping(state).await),
        "log_write" => {
            log_write(arg_value(&args, "lines")?).await;
            Ok(Value::Null)
        }
        "set_zoom" => answer(set_zoom(app.clone(), arg_scale(&args, "scale")?).await),
        "spawn_terminal" => answer(
            spawn_terminal(
                app.clone(),
                arg_str(&args, "id")?,
                arg_number(&args, "cols")?,
                arg_number(&args, "rows")?,
                arg_optional_str(&args, "cwd")?,
            )
            .await,
        ),
        "pty_input" => {
            answer(pty_input(state, arg_str(&args, "id")?, arg_str(&args, "data")?).await)
        }
        "pty_resize" => answer(
            pty_resize(
                state,
                arg_str(&args, "id")?,
                arg_number(&args, "cols")?,
                arg_number(&args, "rows")?,
            )
            .await,
        ),
        "pty_kill" => answer(pty_kill(state, arg_str(&args, "id")?).await),
        "terminal_cwd" => answer(terminal_cwd(state, arg_str(&args, "id")?).await),
        "serial_list" => answer(serial_list().await),
        "font_list" => answer(font_list().await),
        "settings_load" => answer(settings_load(app.clone()).await),
        "settings_save" => answer(settings_save(app.clone(), arg_value(&args, "settings")?).await),
        "session_load" => answer(session_load(app.clone()).await),
        "session_save" => answer(session_save(app.clone(), arg_value(&args, "session")?).await),
        "serial_open" => answer(
            serial_open(
                app.clone(),
                arg_str(&args, "id")?,
                arg_str(&args, "device")?,
                arg_number(&args, "baud")?,
            )
            .await,
        ),
        "serial_write" => {
            answer(serial_write(state, arg_str(&args, "id")?, arg_value(&args, "data")?).await)
        }
        "serial_close" => answer(serial_close(state, arg_str(&args, "id")?).await),
        "log_open_file" => answer(log_open_file(arg_str(&args, "path")?).await),
        "log_complete" => answer(log_complete(arg_str(&args, "prefix")?).await),
        "log_connect" => answer(
            log_connect(
                app.clone(),
                arg_str(&args, "id")?,
                arg_str(&args, "host")?,
                arg_str(&args, "user")?,
                arg_str(&args, "password")?,
                arg_number(&args, "port")?,
                arg_str(&args, "path")?,
            )
            .await,
        ),
        "log_close" => answer(log_close(state, arg_str(&args, "id")?).await),
        other => Err(format!("unknown command: {other}")),
    }
}
