use lazy_static::lazy_static;
use ssh2::Session;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

lazy_static! {
    static ref TERMINAL_CALLBACK: Mutex<Option<Box<dyn Fn(String) + Send + Sync>>> = Mutex::new(None);
}

pub fn set_terminal_callback<F>(callback: F)
where
    F: Fn(String) + Send + Sync + 'static,
{
    *TERMINAL_CALLBACK.lock().unwrap() = Some(Box::new(callback));
}

pub fn clear_terminal_callback() {
    *TERMINAL_CALLBACK.lock().unwrap() = None;
}

pub fn invoke_callback(data: String) {
    if let Some(ref callback) = *TERMINAL_CALLBACK.lock().unwrap() {
        callback(data);
    }
}

pub struct InteractiveTerminal {
    session: Session,
    is_running: Arc<Mutex<bool>>,
}

impl InteractiveTerminal {
    pub fn new(host: &str, user: &str, password: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let tcp = TcpStream::connect(host)?;
        tcp.set_read_timeout(Some(Duration::from_secs(30)))?;

        let mut session = Session::new()?;
        session.set_tcp_stream(tcp);
        session.handshake()?;
        session.userauth_password(user, password)?;

        if !session.authenticated() {
            return Err("SSH authentication failed".into());
        }

        Ok(Self {
            session,
            is_running: Arc::new(Mutex::new(true)),
        })
    }

    pub fn start_shell(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut channel = self.session.channel_session()?;
        channel.request_pty("xterm", None, None)?;

        channel.shell()?;

        let is_running = Arc::clone(&self.is_running);
        let mut reader = channel.stream(0);

        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while *is_running.lock().unwrap() {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = String::from_utf8_lossy(&buf[..n]).to_string();
                        invoke_callback(data);
                    }
                    Err(e) => {
                        eprintln!("Shell read error: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    pub fn write(&mut self, data: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Ok(mut channel) = self.session.channel_session() {
            channel.write_all(data.as_bytes())?;
            channel.flush()?;
        }
        Ok(())
    }

    pub fn resize(&mut self, _cols: u16, _rows: u16) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        *self.is_running.lock().unwrap()
    }

    pub fn stop(&mut self) {
        *self.is_running.lock().unwrap() = false;
    }

    pub fn close(&mut self) {
        self.stop();
        self.session.disconnect(None, "Connection closed", None).ok();
    }
}

impl Drop for InteractiveTerminal {
    fn drop(&mut self) {
        self.close();
    }
}

pub struct TerminalManager {
    terminal: Option<InteractiveTerminal>,
    host: Option<String>,
    user: Option<String>,
    password: Option<String>,
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            terminal: None,
            host: None,
            user: None,
            password: None,
        }
    }

    pub fn set_session(&mut self, host: String, user: String, password: String) {
        self.host = Some(host);
        self.user = Some(user);
        self.password = Some(password);
    }

    pub fn create_terminal(&mut self) -> Result<&mut InteractiveTerminal, Box<dyn std::error::Error + Send + Sync>> {
        if self.terminal.is_none() {
            let host = self.host.as_ref().ok_or("No host configured")?;
            let user = self.user.as_ref().ok_or("No user configured")?;
            let password = self.password.as_ref().ok_or("No password configured")?;

            let mut terminal = InteractiveTerminal::new(host, user, password)?;
            terminal.start_shell()?;
            self.terminal = Some(terminal);
        }
        Ok(self.terminal.as_mut().unwrap())
    }

    pub fn write_to_terminal(&mut self, data: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let terminal = self.create_terminal()?;
        terminal.write(data)
    }

    pub fn close_terminal(&mut self) {
        if let Some(ref mut terminal) = self.terminal {
            terminal.close();
        }
        self.terminal = None;
    }
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new()
    }
}