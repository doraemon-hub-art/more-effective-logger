use ssh2::Session;
use std::io::BufRead;
use std::io::BufReader;
use std::net::TcpStream;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use crate::session::Session as AppSession;

pub struct SshConfig {
    pub host: String,
    pub username: String,
    pub password: String,
}

impl From<&AppSession> for SshConfig {
    fn from(session: &AppSession) -> Self {
        Self {
            host: session.host.clone(),
            username: session.username.clone(),
            password: session.password.clone(),
        }
    }
}

pub trait SshCommand: Send {
    fn execute(&self, session: &mut Session) -> Result<Box<dyn std::io::Read + Send>, Box<dyn std::error::Error>>;
    fn name(&self) -> &str;
}

pub struct TailFileCommand {
    pub file_path: String,
}

impl SshCommand for TailFileCommand {
    fn execute(&self, session: &mut Session) -> Result<Box<dyn std::io::Read + Send>, Box<dyn std::error::Error>> {
        let mut channel = session.channel_session()?;
        channel.exec(&format!("tail -f {}", self.file_path))?;
        Ok(Box::new(channel.stream(0)))
    }

    fn name(&self) -> &str {
        "tail"
    }
}

pub struct SshClient {
    config: SshConfig,
    is_connected: Arc<Mutex<bool>>,
}

impl SshClient {
    fn open_session(&self) -> Result<Session, Box<dyn std::error::Error>> {
        let tcp = TcpStream::connect(&self.config.host)?;
        tcp.set_read_timeout(Some(Duration::from_secs(30)))?;

        let mut session = Session::new()?;
        session.set_tcp_stream(tcp);
        session.handshake()?;
        session.userauth_password(&self.config.username, &self.config.password)?;

        Ok(session)
    }

    pub fn new(config: SshConfig) -> Self {
        Self {
            config,
            is_connected: Arc::new(Mutex::new(false)),
        }
    }

    pub fn execute_command(
        &self,
        command: Box<dyn SshCommand>,
        tx: mpsc::Sender<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let is_connected = Arc::clone(&self.is_connected);

        // 启动监控线程，检测连接状态
        let is_connected_clone = Arc::clone(&is_connected);
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(5));
                let connected = *is_connected_clone.lock().unwrap();
                if !connected {
                    println!("连接已断开，准备重连...");
                }
            }
        });

        // 主循环：连接 -> 执行命令 -> 断开重连
        loop {
            match self.connect_and_execute(&command, &tx) {
                Ok(_) => {
                    println!("连接正常关闭");
                }
                Err(e) => {
                    eprintln!("连接错误: {}", e);
                    *is_connected.lock().unwrap() = false;
                }
            }

            // 连接断开，等待后重试
            println!("5 秒后重新连接...");
            thread::sleep(Duration::from_secs(5));
        }
    }

    fn connect_and_execute(
        &self,
        command: &Box<dyn SshCommand>,
        tx: &mpsc::Sender<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("Connecting to {}...", self.config.host);
        // 1. 创建 SSH Session 并认证
        let mut session = self.open_session()?;

        // 2. 认证状态打印
        println!("Authenticating as {}...", self.config.username);
        println!("Authentication successful!");

        *self.is_connected.lock().unwrap() = true;
        println!("SSH connected successfully!");

        // 3. 执行命令
        let output = command.execute(&mut session)?;

        // 4. 逐行读取并通过 channel 发送给 UI
        let reader = BufReader::new(output);
        for line in reader.lines() {
            match line {
                Ok(text) => {
                    tx.send(text).ok();
                }
                Err(e) => {
                    eprintln!("Read error: {}", e);
                    *self.is_connected.lock().unwrap() = false;
                    return Err(Box::new(e));
                }
            }
        }

        Ok(())
    }

    pub fn tail_logs(
        &self,
        rx: mpsc::Receiver<String>,
        tx: mpsc::Sender<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 等待接收文件路径
        let log_file = rx.recv()?;
        println!("开始监听文件: {}", log_file);

        let command = Box::new(TailFileCommand {
            file_path: log_file,
        });

        self.execute_command(command, tx)
    }

    pub fn tail_file(
        &self,
        file_path: String,
        tx: mpsc::Sender<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let command = Box::new(TailFileCommand { file_path });
        self.execute_command(command, tx)
    }

    pub fn file_exists(&self, file_path: &str) -> Result<bool, Box<dyn std::error::Error>> {
        let session = self.open_session()?;
        let mut channel = session.channel_session()?;
        channel.exec(&format!("test -f '{}' && echo __MELOGGER_EXISTS__", file_path.replace('\'', "'\\''")))?;

        let mut output = String::new();
        std::io::Read::read_to_string(&mut channel.stream(0), &mut output)?;
        channel.wait_close().ok();

        Ok(output.contains("__MELOGGER_EXISTS__"))
    }

    pub fn execute_single_command(
        &self,
        cmd: &str,
        tx: mpsc::Sender<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let session = self.open_session()?;
        let mut channel = session.channel_session()?;
        channel.exec(cmd)?;

        let reader = BufReader::new(channel.stream(0));
        for line in reader.lines() {
            if let Ok(text) = line {
                tx.send(text).ok();
            }
        }
        channel.wait_close().ok();
        Ok(())
    }

    pub fn execute_shell_command(
        &self,
        rx: mpsc::Receiver<String>,
        tx: mpsc::Sender<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 等待接收命令
        while let Ok(cmd) = rx.recv() {
            if cmd.is_empty() {
                continue;
            }

            // 建立 TCP 连接
            let tcp = TcpStream::connect(&self.config.host)?;
            tcp.set_read_timeout(Some(Duration::from_secs(30)))?;

            // 创建 SSH Session 并握手
            let mut session = Session::new()?;
            session.set_tcp_stream(tcp);
            session.handshake()?;

            // 认证
            session.userauth_password(&self.config.username, &self.config.password)?;

            // 执行命令
            let mut channel = session.channel_session()?;
            channel.exec(&cmd)?;

            // 读取输出
            let reader = BufReader::new(channel.stream(0));
            for line in reader.lines() {
                if let Ok(text) = line {
                    tx.send(text).ok();
                }
            }

            // 等待命令完成
            channel.wait_close().ok();
        }

        Ok(())
    }
}
