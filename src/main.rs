mod ssh;
mod parser;
mod filter;
mod config;
mod session;
mod window;

use config::Config;
use filter::FilterManager;
use parser::parse_log_line;
use session::Session;
use ssh::SshClient;
use slint::VecModel;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::sync::mpsc;
use std::thread;
use window::WindowFactory;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let window = WindowFactory::create_main_window()?;
    let ui = window.as_inner();

    let (log_tx, log_rx) = mpsc::channel::<String>();
    let (cmd_out_tx, cmd_out_rx) = mpsc::channel::<String>();
    let (tail_status_tx, tail_status_rx) = mpsc::channel::<bool>();

    let config = Config::load();
    let current_session = Arc::new(Mutex::new(config.get_session().cloned()));
    let tail_active = Arc::new(AtomicBool::new(false));
    let filters = Arc::new(Mutex::new(FilterManager::new()));

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
    let ui_handle_for_timer = ui.as_weak();
    let log_model_clone = log_model.clone();
    let terminal_model_clone = terminal_model.clone();
    let tail_active_for_timer = tail_active.clone();
    let filters_for_timer = filters.clone();
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(50),
        move || {
            while let Ok(line) = log_rx.try_recv() {
                if tail_active_for_timer.load(Ordering::Relaxed) {
                    if let Some(entry) = parse_log_line(&line) {
                        let is_matched = filters_for_timer
                            .lock()
                            .map(|manager| manager.matches(&entry))
                            .unwrap_or(true);

                        if is_matched {
                            log_model_clone.push(LogEntry {
                                level: entry.level.into(),
                                timestamp: entry.timestamp.into(),
                                file_line: entry.file_line.into(),
                                message: entry.message.into(),
                            });
                        }
                    }
                }
            }

            while let Ok(line) = cmd_out_rx.try_recv() {
                terminal_model_clone.push(line.into());
            }

            while let Ok(started) = tail_status_rx.try_recv() {
                if let Some(ui) = ui_handle_for_timer.upgrade() {
                    ui.set_is_connected(started);
                }
                tail_active_for_timer.store(started, Ordering::Relaxed);
            }
        },
    );

    // 回调：连接 session
    let current_session_for_connect = current_session.clone();
    let ui_handle = ui.as_weak();
    ui.on_connect_session(move |host, username, password| {
        let session = Session::new(host.to_string(), username.to_string(), password.to_string());
        if let Ok(mut current) = current_session_for_connect.lock() {
            *current = Some(session.clone());
        }

        let mut config = Config::load();
        let _ = config.save_session(session);

        ui_handle.unwrap().set_session_status("Connected".into());
    });

    // 回调：开始 tail
    let session_for_tail = current_session.clone();
    let log_tx_for_tail = log_tx.clone();
    let tail_status_tx_for_tail = tail_status_tx.clone();
    ui.on_start_tail(move |file_path| {
        let Some(session) = session_for_tail.lock().ok().and_then(|s| s.clone()) else {
            return;
        };

        let file_path = file_path.to_string();
        let log_tx_for_worker = log_tx_for_tail.clone();
        let tail_status_tx_for_worker = tail_status_tx_for_tail.clone();
        thread::spawn(move || {
            let client = SshClient::new((&session).into());
            match client.file_exists(&file_path) {
                Ok(true) => {
                    tail_status_tx_for_worker.send(true).ok();
                    if let Err(e) = client.tail_file(file_path, log_tx_for_worker) {
                        eprintln!("SSH error: {}", e);
                        tail_status_tx_for_worker.send(false).ok();
                    }
                }
                Ok(false) => {
                    tail_status_tx_for_worker.send(false).ok();
                }
                Err(e) => {
                    eprintln!("Check file error: {}", e);
                    tail_status_tx_for_worker.send(false).ok();
                }
            }
        });
    });

    // 回调：停止 tail
    let tail_active_for_stop = tail_active.clone();
    let ui_handle = ui.as_weak();
    ui.on_stop_tail(move || {
        tail_active_for_stop.store(false, Ordering::Relaxed);
        ui_handle.unwrap().set_is_connected(false);
    });

    // 回调：清空日志
    let log_model_clone = log_model.clone();
    ui.on_clear_logs(move || {
        log_model_clone.set_vec(vec![]);
    });

    // 回调：执行命令
    let session_for_terminal = current_session.clone();
    let cmd_out_tx_for_terminal = cmd_out_tx.clone();
    let terminal_model_clone = terminal_model.clone();
    ui.on_execute_command(move |cmd| {
        terminal_model_clone.push(format!("> {}", cmd).into());
        let Some(session) = session_for_terminal.lock().ok().and_then(|s| s.clone()) else {
            return;
        };

        let cmd = cmd.to_string();
        let cmd_out_tx_worker = cmd_out_tx_for_terminal.clone();
        thread::spawn(move || {
            let client = SshClient::new((&session).into());
            if let Err(e) = client.execute_single_command(&cmd, cmd_out_tx_worker) {
                eprintln!("Terminal error: {}", e);
            }
        });
    });

    // 回调：添加过滤器
    let filters_for_add = filters.clone();
    ui.on_add_filter(move |field, operator, value| {
        if let Some(rule) = filter::FilterRule::from_ui(&field, &operator, &value) {
            if let Ok(mut manager) = filters_for_add.lock() {
                manager.add_rule(rule);
            }
        }
    });

    let filters_for_remove = filters.clone();
    ui.on_remove_filter(move |index| {
        if index >= 0 {
            if let Ok(mut manager) = filters_for_remove.lock() {
                manager.remove_rule(index as usize);
            }
        }
    });

    window.run()
}
