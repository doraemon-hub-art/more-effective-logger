//! SSH links to a remote host (ssh2 / libssh2)
//!
//! One function, because the callers disagree about what to do with a failure: a log pane
//! retries a link that never came up but stops on a refused password, while an edit pane
//! simply reports it. So the failure says which of the two it is, and each caller decides.
//!
//! The SFTP channel is opened by the caller: a channel borrows the session, so both have to
//! live in the same scope — a struct holding one and the other cannot be built.

/*
 * @file ssh.rs
 * @brief SSH handshake and password login shared by the remote panes
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-24
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 */

use ssh2::Session;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Budget for the TCP connect and the handshake: a host that swallows packets must not hold
/// the caller forever.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Budget once the link is up: opening the SFTP subsystem, stat, read and write all get the
/// wider one, because a slow server is not a broken one.
pub const SESSION_TIMEOUT: Duration = Duration::from_secs(30);

/// Why a link did not come up: retrying makes sense for the first, never for the second.
#[derive(Debug)]
pub enum Refused {
    /// The host was not reachable, or the handshake broke off.
    Retry(String),
    /// The credentials were refused; they will be refused again.
    Fatal(String),
}

impl Refused {
    /// The text the panes show, whichever kind it is.
    pub fn text(&self) -> String {
        match self {
            Refused::Retry(text) | Refused::Fatal(text) => text.clone(),
        }
    }
}

/// Dial, shake hands, log in. The session comes back with the connect budget still in
/// place; a caller that keeps it around should widen it to [`SESSION_TIMEOUT`].
pub fn connect(host: &str, port: u16, user: &str, password: &str) -> Result<Session, Refused> {
    let where_to = format!("{host}:{port}");
    let addr = (host, port)
        .to_socket_addrs()
        .map_err(|e| Refused::Retry(format!("连接失败 {where_to}：{e}")))?
        .next()
        .ok_or_else(|| Refused::Retry(format!("连接失败 {where_to}：解析不出地址")))?;
    let stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
        .map_err(|e| Refused::Retry(format!("连接失败 {where_to}：{e}")))?;

    let mut session = Session::new().map_err(|e| Refused::Retry(e.to_string()))?;
    session.set_timeout(CONNECT_TIMEOUT.as_millis() as u32);
    session.set_tcp_stream(stream);
    session
        .handshake()
        .map_err(|e| Refused::Retry(format!("握手失败：{e}")))?;
    session
        .userauth_password(user, password)
        .map_err(|e| Refused::Fatal(format!("登录失败（{user}）：{e}")))?;
    if !session.authenticated() {
        return Err(Refused::Fatal(format!("登录被拒：{user}")));
    }
    Ok(session)
}
