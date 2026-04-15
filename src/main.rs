mod ssh;
mod parser;
mod filter;
mod config;

use std::sync::mpsc;
use std::thread;
use std::rc::Rc;
use ssh::{SshClient, SshConfig};
use parser::parse_log_line;
use config::{Config, SessionConfig};
use slint::VecModel;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;

    let (config_tx, config_rx) = mpsc::channel::<String>();
    let (log_tx, log_rx) = mpsc::channel::<String>();
    let (cmd_tx, cmd_rx) = mpsc::channel::<String>();
    let (cmd_out_tx, cmd_out_rx) = mpsc::channel::<String>();

    // SSH 日志线程
    thread::spawn(move || {
        if let Ok(config_str) = config_rx.recv() {
            let parts: Vec<&str> = config_str.split('|').collect();
            if parts.len() == 3 {
                let ssh_config = SshConfig {
                    host: parts[0].to_string(),
                    username: parts[1].to_string(),
                    password: parts[2].to_string(),
                };

                let client = SshClient::new(ssh_config);
                let (file_tx, file_rx) = mpsc::channel::<String>();
                let log_tx_clone = log_tx.clone();

                thread::spawn(move || {
                    if let Err(e) = client.tail_logs(file_rx, log_tx_clone) {
                        eprintln!("SSH error: {}", e);
                    }
                });

                while let Ok(file_path) = config_rx.recv() {
                    file_tx.send(file_path).ok();
                }
            }
        }
    });

    // 终端命令线程
    {
        let config = Config::load();
        if let Some(session) = config.get_session() {
            let ssh_config = SshConfig {
                host: session.host.clone(),
                username: session.username.clone(),
                password: session.password.clone(),
            };
            let client = SshClient::new(ssh_config);
            thread::spawn(move || {
                if let Err(e) = client.execute_shell_command(cmd_rx, cmd_out_tx) {
                    eprintln!("Terminal error: {}", e);
                }
            });
        }
    }

    // 加载已保存的 session 配置
    let config = Config::load();
    if let Some(session) = config.get_session() {
        ui.set_ssh_host(session.host.clone().into());
        ui.set_ssh_username(session.username.clone().into());
    }

    // 初始化 model
    let log_model = Rc::new(VecModel::<LogEntry>::default());
    let terminal_model = Rc::new(VecModel::<slint::SharedString>::default());
    ui.set_log_entries(log_model.clone().into());
    ui.set_terminal_output(terminal_model.clone().into());

    // 定时器：从 channel 读取日志更新 UI
    let ui_handle = ui.as_weak();
    let log_model_clone = log_model.clone();
    let terminal_model_clone = terminal_model.clone();
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(50),
        move || {
            while let Ok(line) = log_rx.try_recv() {
                if let Some(entry) = parse_log_line(&line) {
                    log_model_clone.push(LogEntry {
                        level: entry.level.into(),
                        timestamp: entry.timestamp.into(),
                        file_line: entry.file_line.into(),
                        message: entry.message.into(),
                    });
                }
            }

            while let Ok(line) = cmd_out_rx.try_recv() {
                terminal_model_clone.push(line.into());
            }
        },
    );

    // 回调：连接 session
    let config_tx_clone = config_tx.clone();
    let ui_handle = ui.as_weak();
    ui.on_connect_session(move |host, username, password| {
        let config_str = format!("{}|{}|{}", host, username, password);
        config_tx_clone.send(config_str).ok();

        let mut config = Config::load();
        let _ = config.save_session(SessionConfig {
            host: host.to_string(),
            username: username.to_string(),
            password: password.to_string(),
        });

        ui_handle.unwrap().set_session_status("Connected".into());
    });

    // 回调：开始 tail
    let config_tx_clone = config_tx.clone();
    let ui_handle = ui.as_weak();
    ui.on_start_tail(move |file_path| {
        config_tx_clone.send(file_path.to_string()).ok();
        ui_handle.unwrap().set_is_connected(true);
    });

    // 回调：停止 tail
    let ui_handle = ui.as_weak();
    ui.on_stop_tail(move || {
        ui_handle.unwrap().set_is_connected(false);
    });

    // 回调：清空日志
    let log_model_clone = log_model.clone();
    ui.on_clear_logs(move || {
        log_model_clone.set_vec(vec![]);
    });

    // 回调：执行命令
    let cmd_tx_clone = cmd_tx.clone();
    let terminal_model_clone = terminal_model.clone();
    ui.on_execute_command(move |cmd| {
        terminal_model_clone.push(format!("> {}", cmd).into());
        cmd_tx_clone.send(cmd.to_string()).ok();
    });

    // 回调：添加过滤器
    ui.on_add_filter(move |field, operator, value| {
        println!("Add filter: {} {} {}", field, operator, value);
    });

    ui.run()
}
