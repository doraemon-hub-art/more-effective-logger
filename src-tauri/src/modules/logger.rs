//! Log sources: a local file read once, or a remote file followed over SSH.
//!
//! Two ways into the same pane, and the difference between them is the whole point: the
//! local read is a snapshot (what the file held at that moment, and then nothing more),
//! the remote read is a stream (only the bytes written after the connection came up).
//! Both hand the frontend whole lines — splitting happens here, so the UI never has to
//! deal with a half line, and a line that is still being written simply waits for its
//! newline.
//!
//! A remote connection that drops is picked up again with a growing pause, because a log
//! that stops after one hiccup is worse than a log that lags; the bytes written while the
//! link was down are not lost, the follow resumes where it left off. A refused password is
//! not retried, on the other hand: it would be refused again.
//!
//! One thing about following a growing file over SFTP is worth knowing before touching the
//! read loop: an SFTP handle that has once read to the end of a file is done for good —
//! libssh2 marks it and hands back 0 from then on, however much the file grows afterwards.
//! Reading on therefore means opening the file again at the offset this side has reached
//! (see `next_step` and the read loop); a file that turned up shorter was replaced, and is
//! read from the top.

/*
 * @file logger.rs
 * @brief Log sources: local file snapshot, remote file followed over SSH
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-06
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 */

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde::Serialize;
use ssh2::{Session, Sftp};
use tauri::{AppHandle, Emitter, Manager};

use crate::modules::logfile;
use crate::state::AppState;

/// Lines of one pane, already split.
pub const EVT_DATA: &str = "log-data";
/// Connection state of one pane: connected, or retrying after a drop.
pub const EVT_STATE: &str = "log-state";
/// The source is gone for good: closed on purpose, or failed in a way retrying cannot fix.
pub const EVT_EXIT: &str = "log-exit";

/// How long one connect attempt may take before it counts as failed.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Pause between two reads once the file has been read up to its end.
const POLL_INTERVAL: Duration = Duration::from_millis(200);
/// Pause after the first dropped connection, and the ceiling the pause grows to.
const RETRY_FIRST: Duration = Duration::from_secs(1);
const RETRY_MAX: Duration = Duration::from_secs(30);
/// How much of the remote file one SFTP read asks for.
const CHUNK: usize = 64 * 1024;

/// Lines for one pane.
#[derive(Clone, Serialize)]
pub struct DataPayload {
    pub id: String,
    pub lines: Vec<String>,
}

/// Where one pane's connection stands.
#[derive(Clone, Serialize)]
pub struct StatePayload {
    pub id: String,
    /// True while the file is being followed.
    pub connected: bool,
    /// What is going on, in one line; empty when there is nothing to report.
    pub note: String,
}

/// One pane's source has stopped.
#[derive(Clone, Serialize)]
pub struct ExitPayload {
    pub id: String,
    pub reason: String,
}

/// A local file as it was at the moment it was opened.
#[derive(Serialize)]
pub struct Snapshot {
    pub lines: Vec<String>,
    /// Size of the file on disk, so the pane can report it.
    pub bytes: u64,
}

/// One completion candidate for the path the user has typed so far.
#[derive(Serialize)]
pub struct Candidate {
    pub path: String,
    /// True for a directory: picking it keeps the user typing inside it.
    pub dir: bool,
}

/// A running follow. Stopping it means asking its thread; the flag is how.
pub struct LogSession {
    pub stop: Arc<AtomicBool>,
}

/// What one remote follow needs; kept together because its thread outlives the call.
struct Target {
    host: String,
    port: u16,
    user: String,
    password: String,
    path: String,
}

/// Why a follow stopped.
enum Stopped {
    /// The pane asked for it.
    Asked,
    /// Retrying cannot help; the reason goes to the pane.
    Fatal(String),
    /// The connection dropped; try again after a pause.
    Retry(String),
}

/// Take every whole line out of `pending`, leaving a partial line behind.
fn take_lines(pending: &mut Vec<u8>) -> Vec<String> {
    let mut lines = Vec::new();
    let mut start = 0;
    for at in 0..pending.len() {
        if pending[at] != b'\n' {
            continue;
        }
        // A file written on Windows ends its lines with CRLF; the CR is not part of it.
        let mut end = at;
        if end > start && pending[end - 1] == b'\r' {
            end -= 1;
        }
        lines.push(String::from_utf8_lossy(&pending[start..end]).into_owned());
        start = at + 1;
    }
    pending.drain(..start);
    lines
}

/// A typed path with a leading `~` resolved: only a shell expands a tilde, and this side
/// has no shell.
fn expand(path: &str) -> String {
    let home = std::env::var("HOME").ok().filter(|home| !home.is_empty());
    match (home, path) {
        (Some(home), "~") => home,
        (Some(home), rest) if rest.starts_with("~/") => format!("{home}/{}", &rest[2..]),
        _ => path.to_string(),
    }
}

/// Read a local file once and split it into lines.
///
/// The whole file is read in one go: what is in a log at this moment is the point, and
/// there is nothing to follow afterwards.
pub fn read_snapshot(path: &str) -> Result<Snapshot, String> {
    let path = expand(path);
    let bytes = fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    let size = bytes.len() as u64;
    let mut pending = bytes;
    let mut lines = take_lines(&mut pending);
    // A file whose last line has no newline yet still has that line.
    if !pending.is_empty() {
        lines.push(String::from_utf8_lossy(&pending).into_owned());
    }
    Ok(Snapshot { lines, bytes: size })
}

/// Completion candidates for what the user has typed: the directory part is listed, the
/// entries starting with the name part come back, directories with a trailing slash so
/// picking one keeps the typing inside it.
pub fn complete(prefix: &str) -> Vec<Candidate> {
    let typed = expand(prefix);
    let (dir, name) = match typed.rfind('/') {
        Some(at) => (typed[..=at].to_string(), typed[at + 1..].to_string()),
        None => (String::new(), typed),
    };
    let listing = if dir.is_empty() { "." } else { dir.as_str() };
    let Ok(entries) = fs::read_dir(listing) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        // A dotfile shows up once the user has typed the dot, not before.
        if !file.starts_with(&name) || (file.starts_with('.') && !name.starts_with('.')) {
            continue;
        }
        let dir_entry = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        out.push(Candidate {
            path: if dir_entry {
                format!("{dir}{file}/")
            } else {
                format!("{dir}{file}")
            },
            dir: dir_entry,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Start following a remote file under the given pane id.
///
/// The connection itself happens in the thread, so an unreachable host or a refused
/// password reaches the pane as its state instead of as an error from this call.
pub fn connect(
    app: &AppHandle,
    id: &str,
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    path: &str,
) -> Result<(), String> {
    if host.trim().is_empty() {
        return Err("没填 IP".into());
    }
    if path.trim().is_empty() {
        return Err("没填文件路径".into());
    }
    let stop = Arc::new(AtomicBool::new(false));
    {
        let state = app.state::<AppState>();
        let mut sessions = state.logs.lock().map_err(|e| e.to_string())?;
        // Connecting again while a follow runs is a restart, not a mistake: the running
        // thread is asked to stop and the new session takes the id over.
        if let Some(running) = sessions.insert(id.to_string(), LogSession { stop: stop.clone() }) {
            running.stop.store(true, Ordering::SeqCst);
            logfile::info("log", format!("{id} replaced while running"));
        }
    }
    let target = Target {
        host: host.trim().to_string(),
        port,
        user: user.to_string(),
        password: password.to_string(),
        path: path.trim().to_string(),
    };
    logfile::info(
        "log",
        format!(
            "{id} following {}@{}:{}/{}",
            target.user, target.host, target.port, target.path
        ),
    );
    let app = app.clone();
    let id = id.to_string();
    thread::spawn(move || follow(&app, &id, &target, &stop));
    Ok(())
}

/// Stop the follow behind a pane. The thread notices, tidies up and says so.
pub fn close(state: &AppState, id: &str) -> Result<(), String> {
    let mut sessions = state.logs.lock().map_err(|e| e.to_string())?;
    match sessions.remove(id) {
        Some(session) => {
            session.stop.store(true, Ordering::SeqCst);
            logfile::info("log", format!("closing {id}"));
            Ok(())
        }
        None => Ok(()),
    }
}

/// Follow until the pane is closed, reconnecting for as long as it takes.
///
/// The stop flag is held as the `Arc` the session table has, not as a plain reference: it is
/// also what tells the tail end of this thread whether the entry under this id is still its
/// own (see `owns`).
fn follow(app: &AppHandle, id: &str, target: &Target, stop: &Arc<AtomicBool>) {
    let mut pending: Vec<u8> = Vec::new();
    // Where the next byte is expected, counted from the start of the file; unset until the
    // first connection, whose offset is the end of the file at that moment.
    let mut offset: Option<u64> = None;
    let mut misses = 0u32;
    loop {
        if stop.load(Ordering::SeqCst) {
            finish(app, id, stop, "");
            return;
        }
        match follow_once(
            app,
            id,
            target,
            stop.as_ref(),
            &mut pending,
            &mut offset,
            &mut misses,
        ) {
            Stopped::Asked => {
                finish(app, id, stop, "");
                return;
            }
            Stopped::Fatal(reason) => {
                finish(app, id, stop, &reason);
                return;
            }
            Stopped::Retry(reason) => {
                misses += 1;
                let wait = backoff(misses);
                logfile::warn(
                    "log",
                    format!(
                        "{id}: {reason}; retrying in {}s (#{misses})",
                        wait.as_secs()
                    ),
                );
                emit_state(app, id, false, &reason);
                if !sleep_until(stop.as_ref(), wait) {
                    finish(app, id, stop, "");
                    return;
                }
            }
        }
    }
}

/// One connection: dial, authenticate, follow the file, come back with why it ended.
fn follow_once(
    app: &AppHandle,
    id: &str,
    target: &Target,
    stop: &AtomicBool,
    pending: &mut Vec<u8>,
    offset: &mut Option<u64>,
    misses: &mut u32,
) -> Stopped {
    let stream = match dial(target) {
        Ok(stream) => stream,
        Err(err) => return Stopped::Retry(err),
    };
    let mut session = match Session::new() {
        Ok(session) => session,
        Err(err) => return Stopped::Retry(err.to_string()),
    };
    session.set_timeout(CONNECT_TIMEOUT.as_millis() as u32);
    session.set_tcp_stream(stream);
    if let Err(err) = session.handshake() {
        return Stopped::Retry(format!("握手失败：{err}"));
    }
    if let Err(err) = session.userauth_password(&target.user, &target.password) {
        // A password that was refused will be refused again: stop, do not hammer.
        return Stopped::Fatal(format!("登录失败（{}）：{err}", target.user));
    }
    if !session.authenticated() {
        return Stopped::Fatal(format!("登录被拒：{}", target.user));
    }
    let sftp = match session.sftp() {
        Ok(sftp) => sftp,
        Err(err) => return Stopped::Fatal(format!("打不开 SFTP：{err}")),
    };

    // The first connection starts at the end of the file: what was written before the pane
    // connected is not its business. A later connection keeps the offset it had, so the
    // lines written while the link was down still arrive; a file that shrank was replaced
    // or truncated in the meantime, and is read from its start.
    let size = match remote_size(&sftp, &target.path) {
        Ok(size) => size,
        Err(err) => return Stopped::Retry(err),
    };
    let from = match *offset {
        None => size,
        Some(at) if size < at => 0,
        Some(at) => at,
    };
    let mut file = match open_at(&sftp, &target.path, from) {
        Ok(file) => file,
        Err(err) => return Stopped::Retry(err),
    };
    *offset = Some(from);
    *misses = 0;
    logfile::info("log", format!("{id} connected, from offset {from}"));
    emit_state(app, id, true, "");

    let mut buf = vec![0u8; CHUNK];
    loop {
        if stop.load(Ordering::SeqCst) {
            return Stopped::Asked;
        }
        match file.read(&mut buf) {
            // A read that came back empty means the handle has reached the end of the file
            // and libssh2 has marked it so; from then on that handle answers 0 forever, no
            // matter how much the file grows (`if(filep->eof) return 0;` in libssh2's
            // sftp_read, and `libssh2_sftp_seek64` will not clear it when the offset is
            // already right). So the file is asked how big it is, and a fresh handle picks
            // up from there — one open per poll while lines keep coming, nothing while the
            // file sits still.
            Ok(0) => {
                thread::sleep(POLL_INTERVAL);
                let size = match remote_size(&sftp, &target.path) {
                    Ok(size) => size,
                    Err(err) => return Stopped::Retry(err),
                };
                match next_step(size, offset.unwrap_or(0)) {
                    Next::Wait => {}
                    Next::Reopen(at) => {
                        if at == 0 {
                            logfile::info(
                                "log",
                                format!("{id}: {} rotated, reading the new file", target.path),
                            );
                        }
                        match open_at(&sftp, &target.path, at) {
                            Ok(fresh) => {
                                file = fresh;
                                *offset = Some(at);
                            }
                            Err(err) => return Stopped::Retry(err),
                        }
                    }
                }
            }
            Ok(read) => {
                *offset = Some(offset.unwrap_or(0) + read as u64);
                pending.extend_from_slice(&buf[..read]);
                let lines = take_lines(pending);
                if !lines.is_empty() {
                    let _ = app.emit(
                        EVT_DATA,
                        DataPayload {
                            id: id.to_string(),
                            lines,
                        },
                    );
                }
            }
            Err(err) => return Stopped::Retry(format!("读失败：{err}")),
        }
    }
}

/// What to do after a read came back empty.
enum Next {
    /// The file is where it was: wait and ask again.
    Wait,
    /// Re-open the file at this offset and read on from there.
    Reopen(u64),
}

/// A file that got shorter was replaced or truncated and starts over; a file that grew is
/// picked up where this side left off; a file that did neither is waited on.
fn next_step(size: u64, at: u64) -> Next {
    if size < at {
        Next::Reopen(0)
    } else if size > at {
        Next::Reopen(at)
    } else {
        Next::Wait
    }
}

/// Whether the entry in the table is still the session that owns `stop`.
///
/// A connect that replaced a running follow leaves the old thread to wind down on its own,
/// and that thread must not clear the entry the new one put there.
fn owns(session: Option<&LogSession>, stop: &Arc<AtomicBool>) -> bool {
    session
        .map(|session| Arc::ptr_eq(&session.stop, stop))
        .unwrap_or(false)
}

/// The pane's source is gone: let the id be used again, and say so.
///
/// A session that was replaced stays quiet: the pane is already following the new one, and
/// a "stopped" from the old thread would land on top of the new connection's state.
fn finish(app: &AppHandle, id: &str, stop: &Arc<AtomicBool>, reason: &str) {
    let ours = match app.state::<AppState>().logs.lock() {
        Ok(mut sessions) => {
            if owns(sessions.get(id), stop) {
                sessions.remove(id);
                true
            } else {
                false
            }
        }
        Err(_) => false,
    };
    logfile::info(
        "log",
        match reason {
            "" => format!("{id} stopped"),
            reason => format!("{id} stopped: {reason}"),
        },
    );
    if !ours {
        return;
    }
    let _ = app.emit(
        EVT_EXIT,
        ExitPayload {
            id: id.to_string(),
            reason: reason.to_string(),
        },
    );
}

/// Hand one pane's connection state to the frontend.
fn emit_state(app: &AppHandle, id: &str, connected: bool, note: &str) {
    let _ = app.emit(
        EVT_STATE,
        StatePayload {
            id: id.to_string(),
            connected,
            note: note.to_string(),
        },
    );
}

/// Open the TCP connection, with a timeout: a host that swallows packets must not hold the
/// thread forever.
fn dial(target: &Target) -> Result<TcpStream, String> {
    let where_to = format!("{}:{}", target.host, target.port);
    let addr = (target.host.as_str(), target.port)
        .to_socket_addrs()
        .map_err(|e| format!("连不上 {where_to}：{e}"))?
        .next()
        .ok_or_else(|| format!("连不上 {where_to}：解析不出地址"))?;
    TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
        .map_err(|e| format!("连不上 {where_to}：{e}"))
}

/// Size of the remote file.
fn remote_size(sftp: &Sftp, path: &str) -> Result<u64, String> {
    sftp.stat(Path::new(path))
        .map(|stat| stat.size.unwrap_or(0))
        .map_err(|e| format!("取 {path} 的大小失败：{e}"))
}

/// Open the remote file with the read offset put where the next byte is expected.
fn open_at(sftp: &Sftp, path: &str, at: u64) -> Result<ssh2::File, String> {
    let mut file = sftp
        .open(Path::new(path))
        .map_err(|e| format!("打不开 {path}：{e}"))?;
    if at > 0 {
        file.seek(SeekFrom::Start(at))
            .map_err(|e| format!("定位 {path} 到第 {at} 字节失败：{e}"))?;
    }
    Ok(file)
}

/// Pause after `misses` failures in a row: from a second, doubling up to a ceiling, so a
/// host that is down is not hammered and a host that blinked is picked up quickly.
fn backoff(misses: u32) -> Duration {
    RETRY_FIRST
        .saturating_mul(1u32 << misses.saturating_sub(1).min(5))
        .min(RETRY_MAX)
}

/// Sleep, waking early when the pane is closed meanwhile. False once closing was asked for,
/// which is the caller's signal to stop.
fn sleep_until(stop: &AtomicBool, total: Duration) -> bool {
    let mut left = total;
    while left > Duration::ZERO {
        thread::sleep(POLL_INTERVAL.min(left));
        left = left.saturating_sub(POLL_INTERVAL);
        if stop.load(Ordering::SeqCst) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines_of(text: &str) -> Vec<String> {
        let mut pending = text.as_bytes().to_vec();
        take_lines(&mut pending)
    }

    /// A line ends at the newline, and the newline goes with it.
    #[test]
    fn whole_lines() {
        assert_eq!(lines_of("a\nb\n"), ["a", "b"]);
        assert_eq!(lines_of(""), Vec::<String>::new());
        assert_eq!(lines_of("\n"), [""]);
    }

    /// A partial line at the end of a chunk stays behind until its newline arrives.
    #[test]
    fn partial_line_waits() {
        let mut pending = b"first\nsec".to_vec();
        assert_eq!(take_lines(&mut pending), ["first"]);
        assert_eq!(pending, b"sec");
        pending.extend_from_slice(b"ond\n");
        assert_eq!(take_lines(&mut pending), ["second"]);
        assert!(pending.is_empty());
    }

    /// A CRLF file keeps no carriage return at the end of its lines; a lone CR is content.
    #[test]
    fn carriage_returns() {
        assert_eq!(lines_of("a\r\nb\r\n"), ["a", "b"]);
        assert_eq!(lines_of("a\rb\n"), ["a\rb"]);
    }

    /// Text that is not UTF-8 at all still shows, with the broken bytes replaced.
    #[test]
    fn broken_bytes_do_not_stop_the_read() {
        let mut pending = vec![b'a', 0xff, b'b', b'\n'];
        assert_eq!(take_lines(&mut pending), ["a\u{fffd}b"]);
    }

    /// A file whose last line has no newline still has that line.
    #[test]
    fn snapshot_of_a_file_without_a_final_newline() {
        let dir = std::env::temp_dir().join(format!("melogger-logger-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tail.log");
        fs::write(&path, "one\ntwo").unwrap();
        let snapshot = read_snapshot(path.to_str().unwrap()).unwrap();
        assert_eq!(snapshot.lines, ["one", "two"]);
        assert_eq!(snapshot.bytes, 7);
        fs::write(&path, "one\ntwo\nthree\n").unwrap();
        assert_eq!(
            read_snapshot(path.to_str().unwrap()).unwrap().lines,
            ["one", "two", "three"]
        );
        // A file that is not there is an error, and it names the file.
        let missing = read_snapshot(dir.join("nope.log").to_str().unwrap())
            .err()
            .expect("a file that is not there must not read");
        assert!(missing.contains("nope.log"), "{missing}");
        let _ = fs::remove_dir_all(&dir);
    }

    /// The pause grows from a second to the ceiling, and stays there.
    #[test]
    fn backoff_grows_and_stops() {
        assert_eq!(backoff(1), Duration::from_secs(1));
        assert_eq!(backoff(2), Duration::from_secs(2));
        assert_eq!(backoff(3), Duration::from_secs(4));
        assert_eq!(backoff(6), RETRY_MAX);
        assert_eq!(backoff(99), RETRY_MAX);
    }

    /// Completions follow the typed prefix, and only offer what is under it.
    #[test]
    fn completion_of_a_prefix() {
        let dir =
            std::env::temp_dir().join(format!("melogger-complete-test-{}", std::process::id()));
        fs::create_dir_all(dir.join("logs")).unwrap();
        fs::write(dir.join("app.log"), "").unwrap();
        fs::write(dir.join("app.log.1"), "").unwrap();
        fs::write(dir.join("other.log"), "").unwrap();
        fs::write(dir.join(".hidden"), "").unwrap();
        let prefix = format!("{}/app", dir.display());
        let found: Vec<String> = complete(&prefix).into_iter().map(|c| c.path).collect();
        assert_eq!(
            found,
            [
                format!("{}/app.log", dir.display()),
                format!("{}/app.log.1", dir.display())
            ]
        );
        // A directory comes back with its slash, and an unknown one has nothing to offer.
        let dirs: Vec<String> = complete(&format!("{}/lo", dir.display()))
            .into_iter()
            .map(|c| c.path)
            .collect();
        assert_eq!(dirs, [format!("{}/logs/", dir.display())]);
        assert!(complete(&format!("{}/zzz", dir.display())).is_empty());
        // A dotfile only shows up once the dot is typed.
        let dots: Vec<String> = complete(&format!("{}/.", dir.display()))
            .into_iter()
            .map(|c| c.path)
            .collect();
        assert_eq!(dots, [format!("{}/.hidden", dir.display())]);
        let _ = fs::remove_dir_all(&dir);
    }

    /// Growth is read on from where this side stopped; a file that shrank starts over.
    #[test]
    fn an_empty_read_waits_grows_or_starts_over() {
        assert!(matches!(next_step(1000, 1000), Next::Wait));
        assert!(matches!(next_step(1500, 1000), Next::Reopen(1000)));
        assert!(matches!(next_step(20, 1000), Next::Reopen(0)));
    }

    /// A session only clears its own entry: the one a later connect replaced it with stays.
    #[test]
    fn a_replaced_session_does_not_clear_the_new_one() {
        let old = Arc::new(AtomicBool::new(false));
        let fresh = Arc::new(AtomicBool::new(false));
        let held = LogSession {
            stop: fresh.clone(),
        };
        assert!(owns(Some(&held), &fresh));
        assert!(!owns(Some(&held), &old));
        assert!(!owns(None, &old));
    }

    /// The tilde is this side's job: nothing here goes through a shell.
    #[test]
    fn tilde_is_expanded() {
        let home = std::env::var("HOME").unwrap_or_default();
        if home.is_empty() {
            return;
        }
        assert_eq!(expand("~"), home);
        assert_eq!(expand("~/x"), format!("{home}/x"));
        assert_eq!(expand("/var/log"), "/var/log");
    }
}
