use ssh2::Session;
use std::io::BufRead;
use std::io::BufReader;
use std::net::TcpStream;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub struct SshConfig {
    pub host: String,
    pub username: String,
    pub password: String,
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
        // 1. 建立 TCP 连接
        println!("Connecting to {}...", self.config.host);
        let tcp = TcpStream::connect(&self.config.host)?;
        tcp.set_read_timeout(Some(Duration::from_secs(30)))?;

        // 2. 创建 SSH Session 并握手
        let mut session = Session::new()?;
        session.set_tcp_stream(tcp);
        session.handshake()?;

        // 3. 认证
        println!("Authenticating as {}...", self.config.username);
        match session.userauth_password(&self.config.username, &self.config.password) {
            Ok(_) => {
                println!("Authentication successful!");
            }
            Err(e) => {
                eprintln!("Authentication failed: {}", e);
                *self.is_connected.lock().unwrap() = false;
                return Err(Box::new(e));
            }
        }

        *self.is_connected.lock().unwrap() = true;
        println!("SSH connected successfully!");

        // 4. 执行命令
        let output = command.execute(&mut session)?;

        // 5. 逐行读取并通过 channel 发送给 UI
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
