use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use ssh2::Session;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

mod config;
use config::AppConfig;

struct PtySession {
    writer: Box<dyn std::io::Write + Send>,
    master: Box<dyn portable_pty::MasterPty + Send>,
}

struct LogSession {
    stop_flag: Arc<AtomicBool>,
}

struct AppState {
    sessions: Mutex<HashMap<u32, PtySession>>,
    log_sessions: Mutex<HashMap<u32, LogSession>>,
    config: AppConfig,
}

#[derive(Clone, serde::Serialize)]
struct PtyPayload {
    #[serde(rename = "termId")] term_id: u32,
    data: String,
}

#[derive(Clone, serde::Serialize)]
struct LogPayload {
    #[serde(rename = "logId")] log_id: u32,
    line: String,
}

#[derive(Clone, serde::Serialize)]
struct LogStatusPayload {
    #[serde(rename = "logId")] log_id: u32,
    status: String, error: Option<String>,
}

#[tauri::command]
async fn spawn_terminal(
    app: AppHandle, state: State<'_, AppState>,
    term_id: u32, cols: u16, rows: u16,
) -> Result<(), String> {
    let pty_system = native_pty_system();
    let pty_pair = pty_system
        .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| format!("PTY open failed: {e}"))?;
    let mut cmd = CommandBuilder::new(std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into()));
    cmd.env("TERM", "xterm-256color"); cmd.env("COLORTERM", "truecolor");
    let mut child = pty_pair.slave.spawn_command(cmd).map_err(|e| format!("Shell spawn: {e}"))?;
    let writer = pty_pair.master.take_writer().map_err(|e| format!("{e}"))?;
    let mut reader = pty_pair.master.try_clone_reader().map_err(|e| format!("{e}"))?;
    state.sessions.lock().unwrap().insert(term_id, PtySession { writer, master: pty_pair.master });

    // PTY 输出：读到数据立即发送，不批量等待
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    let _ = app.emit("pty-output", PtyPayload { term_id, data });
                }
                Err(_) => break,
            }
        }
    });
    std::thread::spawn(move || { let _ = child.wait(); });
    Ok(())
}

#[tauri::command]
async fn pty_input(state: State<'_, AppState>, term_id: u32, data: String) -> Result<(), String> {
    let mut s = state.sessions.lock().unwrap();
    if let Some(p) = s.get_mut(&term_id) {
        p.writer.write_all(data.as_bytes()).map_err(|e| format!("{e}"))?;
        p.writer.flush().map_err(|e| format!("{e}"))?;
    }
    Ok(())
}

#[tauri::command]
async fn pty_resize(state: State<'_, AppState>, term_id: u32, cols: u16, rows: u16) -> Result<(), String> {
    if let Some(s) = state.sessions.lock().unwrap().get(&term_id) {
        s.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| format!("{e}"))?;
    }
    Ok(())
}

#[tauri::command]
async fn log_connect(
    app: AppHandle, state: State<'_, AppState>,
    log_id: u32, host: String, port: u16, username: String, password: String, file_path: String,
) -> Result<(), String> {
    let stop = Arc::new(AtomicBool::new(false));
    state.log_sessions.lock().unwrap().insert(log_id, LogSession { stop_flag: stop.clone() });
    let a = app.clone();
    std::thread::spawn(move || {
        let status = |s: &str, e: Option<&str>| {
            let _ = a.emit("log-status", LogStatusPayload { log_id, status: s.to_string(), error: e.map(|x| x.to_string()) });
        };
        let addr = format!("{host}:{port}");
        let tcp = match TcpStream::connect(&addr) { Ok(t) => t, Err(e) => { status("error", Some(&e.to_string())); return; } };
        let mut sess = match Session::new() { Ok(s) => s, Err(e) => { status("error", Some(&e.to_string())); return; } };
        sess.set_tcp_stream(tcp);
        if let Err(e) = sess.handshake() { status("error", Some(&e.to_string())); return; }
        if let Err(e) = sess.userauth_password(&username, &password) { status("error", Some(&format!("Auth: {e}"))); return; }
        if !sess.authenticated() { status("error", Some("Auth failed")); return; }
        let mut ch = match sess.channel_session() { Ok(c) => c, Err(e) => { status("error", Some(&e.to_string())); return; } };
        let ck = format!("test -f '{}' && echo OK", file_path.replace('\'', "'\\''"));
        if let Err(e) = ch.exec(&ck) { status("error", Some(&e.to_string())); return; }
        let mut out = String::new(); let _ = ch.read_to_string(&mut out); ch.wait_close().ok();
        if !out.contains("OK") { status("error", Some("File not found")); return; }
        let tail = format!("tail -f '{}'", file_path.replace('\'', "'\\''"));
        let mut ch = match sess.channel_session() { Ok(c) => c, Err(e) => { status("error", Some(&e.to_string())); return; } };
        if let Err(e) = ch.exec(&tail) { status("error", Some(&e.to_string())); return; }
        status("connected", None);
        use std::io::BufRead;
        for line in std::io::BufReader::new(ch.stream(0)).lines() {
            if stop.load(Ordering::Relaxed) { break; }
            match line { Ok(t) => { let _ = a.emit("log-line", LogPayload { log_id, line: t }); }, Err(e) => { status("error", Some(&e.to_string())); break; } }
        }
        status("stopped", None);
    });
    Ok(())
}

#[tauri::command]
async fn log_stop(state: State<'_, AppState>, log_id: u32) -> Result<(), String> {
    if let Some(s) = state.log_sessions.lock().unwrap().get(&log_id) {
        s.stop_flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
fn get_config(state: State<'_, AppState>) -> Result<AppConfig, String> { Ok(state.config.clone()) }

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cfg = AppConfig::load();
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().level(log::LevelFilter::Info).build())
        .manage(AppState { sessions: Mutex::new(HashMap::new()), log_sessions: Mutex::new(HashMap::new()), config: cfg })
        .invoke_handler(tauri::generate_handler![spawn_terminal, pty_input, pty_resize, log_connect, log_stop, get_config])
        .setup(|_| { log::info!("More Effective Logger starting..."); Ok(()) })
        .run(tauri::generate_context!()).expect("run failed");
}
