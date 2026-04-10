use ssh2::Session;
use std::io::BufRead;
use std::io::BufReader;
use std::net::TcpStream;
use std::sync::mpsc;

pub struct SshConfig {
    pub host: String,
    pub username: String,
    pub password: String,
}

pub struct SshClient {
    config: SshConfig,
}

impl SshClient {
    pub fn new(config: SshConfig) -> Self {
        Self { config }
    }

    pub fn tail_logs(
        &self,
        rx: mpsc::Receiver<String>,
        tx: mpsc::Sender<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 等待接收文件路径
        let log_file = rx.recv()?;
        println!("开始监听文件: {}", log_file);

        // 1. 建立 TCP 连接
        let tcp = TcpStream::connect(&self.config.host)?;

        // 2. 创建 SSH Session 并握手
        let mut session = Session::new()?;
        session.set_tcp_stream(tcp);
        session.handshake()?;

        // 3. 认证
        session.userauth_password(&self.config.username, &self.config.password)?;

        println!("SSH 连接成功！");

        // 4. 打开 channel 执行 tail -f
        let mut channel = session.channel_session()?;
        channel.exec(&format!("tail -f {}", log_file))?;

        // 5. 逐行读取并通过 channel 发送给 UI
        let reader = BufReader::new(channel.stream(0));
        for line in reader.lines() {
            match line {
                Ok(text) => {
                    tx.send(text).ok();
                }
                Err(e) => {
                    eprintln!("读取错误: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }
}
