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

// ── PTY 会话 ──────────────────────────────────

struct PtySession {
    writer: Box<dyn std::io::Write + Send>,
    master: Box<dyn portable_pty::MasterPty + Send>,
}

// ── SSH Log 会话 ──────────────────────────────

struct LogSession {
    stop_flag: Arc<AtomicBool>,
}

// ── 全局状态 ──────────────────────────────────

struct AppState {
    sessions: Mutex<HashMap<u32, PtySession>>,
    log_sessions: Mutex<HashMap<u32, LogSession>>,
    config: AppConfig,
}

#[derive(Clone, serde::Serialize)]
struct PtyPayload {
    #[serde(rename = "termId")]
    term_id: u32,
    data: String,
}

#[derive(Clone, serde::Serialize)]
struct LogPayload {
    #[serde(rename = "logId")]
    log_id: u32,
    line: String,
}

#[derive(Clone, serde::Serialize)]
struct LogStatusPayload {
    #[serde(rename = "logId")]
    log_id: u32,
    status: String,
    error: Option<String>,
}

// ══════════════════════════════════════════════
// PTY 终端 commands
// ══════════════════════════════════════════════

#[tauri::command]
async fn spawn_terminal(
    app: AppHandle,
    state: State<'_, AppState>,
    term_id: u32,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let pty_system = native_pty_system();
    let pty_pair = pty_system
        .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| format!("PTY open failed: {e}"))?;

    let mut cmd = CommandBuilder::new(
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into()),
    );
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    let mut child = pty_pair.slave.spawn_command(cmd)
        .map_err(|e| format!("Shell spawn failed: {e}"))?;

    let writer = pty_pair.master.take_writer().map_err(|e| format!("{e}"))?;
    let mut reader = pty_pair.master.try_clone_reader().map_err(|e| format!("{e}"))?;

    state.sessions.lock().unwrap().insert(term_id, PtySession { writer, master: pty_pair.master });

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
    let mut sessions = state.sessions.lock().unwrap();
    if let Some(s) = sessions.get_mut(&term_id) {
        s.writer.write_all(data.as_bytes()).map_err(|e| format!("{e}"))?;
        s.writer.flush().map_err(|e| format!("{e}"))?;
    }
    Ok(())
}

#[tauri::command]
async fn pty_resize(state: State<'_, AppState>, term_id: u32, cols: u16, rows: u16) -> Result<(), String> {
    let sessions = state.sessions.lock().unwrap();
    if let Some(s) = sessions.get(&term_id) {
        s.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| format!("{e}"))?;
    }
    Ok(())
}

// ══════════════════════════════════════════════
// SSH Log commands
// ══════════════════════════════════════════════

#[tauri::command]
async fn log_connect(
    app: AppHandle,
    state: State<'_, AppState>,
    log_id: u32,
    host: String,
    port: u16,
    username: String,
    password: String,
    file_path: String,
) -> Result<(), String> {
    let stop_flag = Arc::new(AtomicBool::new(false));
    state.log_sessions.lock().unwrap().insert(log_id, LogSession { stop_flag: stop_flag.clone() });

    let app_clone = app.clone();
    let log_id_clone = log_id;

    std::thread::spawn(move || {
        let status = |s: &str, e: Option<&str>| {
            let _ = app_clone.emit("log-status", LogStatusPayload {
                log_id: log_id_clone,
                status: s.to_string(),
                error: e.map(|x| x.to_string()),
            });
        };
        // 连接 SSH
        let addr = format!("{host}:{port}");
        let tcp = match TcpStream::connect(&addr) {
            Ok(t) => t,
            Err(e) => { status("error", Some(&e.to_string())); return; }
        };
        let mut session = match Session::new() {
            Ok(s) => s,
            Err(e) => { status("error", Some(&e.to_string())); return; }
        };
        session.set_tcp_stream(tcp);
        if let Err(e) = session.handshake() {
            status("error", Some(&e.to_string())); return;
        }
        if let Err(e) = session.userauth_password(&username, &password) {
            status("error", Some(&format!("Auth failed: {e}"))); return;
        }
        if !session.authenticated() {
            status("error", Some("Authentication failed")); return;
        }

        // 检查文件是否存在
        let check_cmd = format!("test -f '{}' && echo __EXISTS__", file_path.replace('\'', "'\\''"));
        let mut ch = match session.channel_session() {
            Ok(c) => c,
            Err(e) => { status("error", Some(&e.to_string())); return; }
        };
        if let Err(e) = ch.exec(&check_cmd) {
            status("error", Some(&e.to_string())); return;
        }
        let mut out = String::new();
        let _ = ch.read_to_string(&mut out);
        ch.wait_close().ok();
        if !out.contains("__EXISTS__") {
            status("error", Some("File not found on remote"));
            return;
        }

        // tail -f 监听文件
        let tail_cmd = format!("tail -f '{}'", file_path.replace('\'', "'\\''"));
        let mut ch = match session.channel_session() {
            Ok(c) => c,
            Err(e) => { status("error", Some(&e.to_string())); return; }
        };
        if let Err(e) = ch.exec(&tail_cmd) {
            status("error", Some(&e.to_string())); return;
        }

        status("connected", None);

        // 逐行读取
        use std::io::BufRead;
        let reader = std::io::BufReader::new(ch.stream(0));
        for line in reader.lines() {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }
            match line {
                Ok(text) => {
                    let _ = app.emit("log-line", LogPayload { log_id, line: text });
                }
                Err(e) => {
                    status("error", Some(&e.to_string()));
                    break;
                }
            }
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

// ══════════════════════════════════════════════
// Config
// ══════════════════════════════════════════════

#[tauri::command]
fn get_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    Ok(state.config.clone())
}

// ══════════════════════════════════════════════
// Entry
// ══════════════════════════════════════════════

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = AppConfig::load();
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().level(log::LevelFilter::Info).build())
        .manage(AppState {
            sessions: Mutex::new(HashMap::new()),
            log_sessions: Mutex::new(HashMap::new()),
            config,
        })
        .invoke_handler(tauri::generate_handler![
            spawn_terminal, pty_input, pty_resize,
            log_connect, log_stop,
            get_config,
        ])
        .setup(|_app| {
            log::info!("More Effective Logger starting...");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
