//! Dev bridge: the app's commands and events over loopback HTTP, for a browser client.
//!
//! The Tauri window reaches the backend through `__TAURI_INTERNALS__`, which a plain browser
//! does not have. This module opens the same two directions over HTTP instead: `POST /rpc`
//! runs one command and returns its value, `GET /events` streams events as server-sent
//! events. It is transport only — it holds no application state and knows no command names;
//! the app hands it a dispatcher and publishes events into it.

/*
 * @file devbridge.rs
 * @brief Loopback HTTP bridge for a browser client: commands in, events out
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-23
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 */
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Where the bridge listens. Loopback only: the port is expected to arrive through an ssh
/// forward, never to be exposed.
pub const ADDR: &str = "127.0.0.1:1421";

/// Silence longer than this puts a comment line on the stream, so a dead peer is noticed.
const PING: Duration = Duration::from_secs(15);

/// Runs one command by name. Provided by the app; the error is the text the frontend would
/// have received from `invoke`.
pub type Dispatch = Arc<dyn Fn(String, Value) -> Result<Value, String> + Send + Sync>;

/// One event on its way out: its name and its payload, already JSON.
#[derive(Clone)]
pub struct Event {
    pub name: &'static str,
    pub payload: String,
}

/// The open event streams, so the app can publish to all of them.
#[derive(Clone, Default)]
pub struct Hub {
    clients: Arc<Mutex<Vec<Sender<Arc<Event>>>>>,
}

impl Hub {
    /// Hand the event to every stream still open; the closed ones drop out here.
    pub fn publish(&self, event: Event) {
        let event = Arc::new(event);
        let mut clients = match self.clients.lock() {
            Ok(clients) => clients,
            Err(poisoned) => poisoned.into_inner(),
        };
        clients.retain(|client| client.send(event.clone()).is_ok());
    }
}

/// Bind `addr` and serve in the background until the process ends.
pub fn start(addr: &str, dispatch: Dispatch) -> std::io::Result<Hub> {
    let listener = TcpListener::bind(addr)?;
    let hub = Hub::default();
    let accepting = hub.clone();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let dispatch = dispatch.clone();
            let hub = accepting.clone();
            thread::spawn(move || serve(stream, dispatch, hub));
        }
    });
    Ok(hub)
}

/// What `POST /rpc` carries.
#[derive(serde::Deserialize)]
struct Request {
    cmd: String,
    #[serde(default)]
    args: Value,
}

/// One connection: read the head, then answer by route.
fn serve(mut stream: TcpStream, dispatch: Dispatch, hub: Hub) {
    let reading = match stream.try_clone() {
        Ok(clone) => clone,
        Err(_) => return,
    };
    let mut reader = BufReader::new(reading);

    let mut request = String::new();
    if reader.read_line(&mut request).is_err() {
        return;
    }
    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();

    let mut length = 0usize;
    loop {
        let mut header = String::new();
        match reader.read_line(&mut header) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let header = header.trim_end();
                if header.is_empty() {
                    break;
                }
                if let Some((name, value)) = header.split_once(':') {
                    if name.eq_ignore_ascii_case("content-length") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
            }
        }
    }

    match (method.as_str(), path.as_str()) {
        ("OPTIONS", _) => {
            let _ = stream.write_all(b"HTTP/1.1 204 No Content\r\n");
            let _ = write_cors(&mut stream);
            let _ = stream.write_all(b"\r\n");
        }
        ("POST", "/rpc") => {
            let mut body = vec![0u8; length];
            if reader.read_exact(&mut body).is_err() {
                return;
            }
            let answer = match serde_json::from_slice::<Request>(&body) {
                Ok(request) => match (dispatch)(request.cmd, request.args) {
                    Ok(value) => format!(r#"{{"ok":true,"value":{value}}}"#),
                    Err(error) => error_body(&error),
                },
                Err(error) => error_body(&error.to_string()),
            };
            let _ = write_response(&mut stream, "200 OK", "application/json", answer.as_bytes());
        }
        ("GET", "/events") => stream_events(stream, hub),
        _ => {
            let _ = write_response(&mut stream, "404 Not Found", "text/plain", b"not found");
        }
    }
}

/// Keep writing `event: <name>` / `data: <json>` frames until the client goes away.
fn stream_events(mut stream: TcpStream, hub: Hub) {
    let (sender, receiver) = mpsc::channel();
    {
        let mut clients = match hub.clients.lock() {
            Ok(clients) => clients,
            Err(poisoned) => poisoned.into_inner(),
        };
        clients.push(sender);
    }

    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\n\
                Connection: keep-alive\r\nAccess-Control-Allow-Origin: *\r\n\r\n";
    if stream.write_all(head.as_bytes()).is_err() || stream.flush().is_err() {
        return;
    }

    loop {
        match receiver.recv_timeout(PING) {
            Ok(event) => {
                let frame = format!("event: {}\ndata: {}\n\n", event.name, event.payload);
                if stream.write_all(frame.as_bytes()).is_err() || stream.flush().is_err() {
                    return;
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if stream.write_all(b": ping\n\n").is_err() || stream.flush().is_err() {
                    return;
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// The failure shape of `POST /rpc`: the message alone, which the frontend throws as is.
fn error_body(message: &str) -> String {
    let quoted = serde_json::to_string(message).unwrap_or_else(|_| "\"\"".to_string());
    format!(r#"{{"ok":false,"error":{quoted}}}"#)
}

fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    write!(stream, "HTTP/1.1 {status}\r\n")?;
    write!(stream, "Content-Type: {content_type}\r\n")?;
    write!(stream, "Content-Length: {}\r\n", body.len())?;
    write!(stream, "Connection: close\r\n")?;
    write_cors(stream)?;
    stream.write_all(b"\r\n")?;
    stream.write_all(body)?;
    stream.flush()
}

/// The page comes from the dev server on another port, so every answer has to allow it.
fn write_cors(stream: &mut TcpStream) -> std::io::Result<()> {
    stream.write_all(
        b"Access-Control-Allow-Origin: *\r\n\
          Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
          Access-Control-Allow-Headers: content-type\r\n",
    )
}
