//! Local terminal sessions backed by a pseudo-terminal.

/*
 * @file pty.rs
 * @brief Local PTY sessions: spawn a shell, pump bytes both ways
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-01
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 */

use std::io::{Read, Write};
use std::path::PathBuf;
use std::thread;

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::modules::logfile;
use crate::state::AppState;

/// Event carrying shell output; the payload carries the id of the pane it belongs to.
pub const EVT_OUTPUT: &str = "pty-output";
/// Event emitted once the shell has exited and its session is gone.
pub const EVT_EXIT: &str = "pty-exit";

/// Shell output for one pane.
#[derive(Clone, Serialize)]
pub struct OutputPayload {
    pub id: String,
    pub data: String,
}

/// The shell behind one pane has gone away.
#[derive(Clone, Serialize)]
pub struct ExitPayload {
    pub id: String,
}

/// What the frontend needs to draw the pane head right after spawning.
#[derive(Clone, Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub pid: Option<u32>,
    pub cols: u16,
    pub rows: u16,
    /// User part of the prompt
    pub user: String,
    /// Host part of the prompt
    pub host: String,
}

/// One live shell attached to a pty.
pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

impl PtySession {
    /// Push bytes typed in the frontend into the shell.
    pub fn write(&mut self, data: &[u8]) -> Result<(), String> {
        self.writer.write_all(data).map_err(|e| e.to_string())?;
        self.writer.flush().map_err(|e| e.to_string())
    }

    /// Tell the kernel (and through it the shell) about a new window size.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string())
    }

    /// Terminate the shell. The reader thread sees EOF and cleans the session up.
    pub fn kill(&mut self) -> Result<(), String> {
        self.child.kill().map_err(|e| e.to_string())
    }

    /// Where the shell currently is, with the home directory shortened to `~`.
    ///
    /// Read from /proc rather than tracked: `cd` happens inside the shell, so the
    /// kernel's view of the child's working directory is the only one that is right.
    #[cfg(unix)]
    pub fn cwd(&self) -> Option<String> {
        let pid = self.child.process_id()?;
        let path = std::fs::read_link(format!("/proc/{pid}/cwd")).ok()?;
        let path = path.to_string_lossy().to_string();
        let home = std::env::var("HOME").ok().filter(|home| !home.is_empty());
        Some(match home {
            // Only a path boundary shortens: `/home/xuanother` is not under `/home/xuan`.
            Some(home) if path == home => "~".to_string(),
            Some(home) => match path.strip_prefix(&format!("{home}/")) {
                Some(rest) => format!("~/{rest}"),
                None => path,
            },
            None => path,
        })
    }
}

/// The directory a restored shell should start in. The session file holds the shortened
/// form the pane head shows (`~`), and only a shell expands a tilde, so it is resolved
/// here. A directory that is gone by now is not an error — the shell starts where it
/// otherwise would, which is what `None` means.
fn restored_dir(dir: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").ok().filter(|home| !home.is_empty());
    let path = match (home, dir) {
        (Some(home), "~") => PathBuf::from(home),
        (Some(home), dir) if dir.starts_with("~/") => PathBuf::from(home).join(&dir[2..]),
        _ => PathBuf::from(dir),
    };
    path.is_dir().then_some(path)
}

/// Spawn a shell in a fresh pty and start its reader thread.
///
/// The session is registered under `id` before this returns, so the frontend can
/// send input for that id immediately.
pub fn spawn(
    app: &AppHandle,
    id: &str,
    cols: u16,
    rows: u16,
    cwd: Option<&str>,
) -> Result<SessionInfo, String> {
    if cols == 0 || rows == 0 {
        return Err("terminal size must be non-zero".into());
    }
    if app
        .state::<AppState>()
        .ptys
        .lock()
        .map_err(|e| e.to_string())?
        .contains_key(id)
    {
        return Err(format!("terminal {id} already exists"));
    }

    let size = PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    };
    let pair = native_pty_system()
        .openpty(size)
        .map_err(|e| e.to_string())?;

    // new_default_prog() runs the user's shell ($SHELL); TERM has to be set here
    // because a pty has no other way to tell the shell what it is talking to.
    let mut cmd = CommandBuilder::new_default_prog();
    cmd.env("TERM", "xterm-256color");
    // A restored pane's shell starts where it was left.
    let start_dir = cwd.and_then(restored_dir);
    if let Some(dir) = &start_dir {
        cmd.cwd(dir);
    }
    let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    // The slave end must be closed here, otherwise the reader below never sees EOF.
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let pid = child.process_id();

    app.state::<AppState>()
        .ptys
        .lock()
        .map_err(|e| e.to_string())?
        .insert(
            id.to_string(),
            PtySession {
                master: pair.master,
                writer,
                child,
            },
        );

    let app_handle = app.clone();
    let session_id = id.to_string();
    let pid_label = pid.map_or_else(|| "-".to_string(), |pid| pid.to_string());
    let dir_label = start_dir
        .as_ref()
        .map_or_else(|| "-".to_string(), |dir| dir.display().to_string());
    logfile::info(
        "pty",
        format!("spawned {id} {cols}x{rows} pid={pid_label} dir={dir_label}"),
    );
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut pending: Vec<u8> = Vec::new();
        loop {
            match reader.read(&mut buf) {
                // EOF means the shell is gone on its own.
                Ok(0) => break,
                Err(err) => {
                    logfile::warn("pty", format!("{session_id} read: {err}"));
                    break;
                }
                Ok(n) => {
                    pending.extend_from_slice(&buf[..n]);
                    // Forward whole UTF-8 sequences only: one multi-byte character can be
                    // split across two reads, and its first half must wait for the rest.
                    let mut complete = match std::str::from_utf8(&pending) {
                        Ok(_) => pending.len(),
                        Err(e) => e.valid_up_to(),
                    };
                    if complete == 0 && pending.len() >= 4 {
                        // Not UTF-8 at all (binary output): do not stall on it forever.
                        complete = pending.len();
                    }
                    if complete > 0 {
                        let data = String::from_utf8_lossy(&pending[..complete]).to_string();
                        pending.drain(..complete);
                        let _ = app_handle.emit(
                            EVT_OUTPUT,
                            OutputPayload {
                                id: session_id.clone(),
                                data,
                            },
                        );
                    }
                }
            }
        }

        // The shell is gone: reap it so it does not linger as a zombie, drop the
        // session (closing the pty), and let the frontend mark the pane as exited.
        let state = app_handle.state::<AppState>();
        if let Ok(mut sessions) = state.ptys.lock() {
            if let Some(mut session) = sessions.remove(&session_id) {
                let _ = session.child.wait();
            }
        }
        logfile::info("pty", format!("{session_id} shell exited"));
        let _ = app_handle.emit(EVT_EXIT, ExitPayload { id: session_id });
    });

    Ok(SessionInfo {
        id: id.to_string(),
        pid,
        cols,
        rows,
        user: env_user(),
        host: env_host(),
    })
}

/// User name for the pane head, taken from the environment.
fn env_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "user".into())
}

/// Host name for the pane head.
fn env_host() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|s| s.trim().to_string())
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "localhost".into())
}
