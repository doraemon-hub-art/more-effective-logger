use iced::{
    widget::{column, container, row, text, text_input, button, scrollable, pick_list},
    Element, Length, Task, Subscription,
};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use crate::parser::LogEntry;
use crate::filter::{FilterManager, FilterRule, FilterField, FilterOperator};
use crate::config::{Config, SessionConfig};
use crate::ssh::{SshClient, SshConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Logs,
    Session,
    Settings,
    Terminal,
}

#[derive(Debug, Clone)]
pub enum Message {
    PageChanged(Page),
    FilePathChanged(String),
    OkPressed,
    ClearPressed,
    StartPressed,
    StopPressed,
    LockToggled,
    FilterFieldChanged(FilterField),
    FilterOperatorChanged(FilterOperator),
    FilterValueChanged(String),
    AddFilterPressed,
    RemoveFilter(usize),
    SshHostChanged(String),
    SshUsernameChanged(String),
    SshPasswordChanged(String),
    ConnectPressed,
    TerminalInputChanged(String),
    ExecutePressed,
    LogSelected(usize),
    LogLineReceived(String),
    TerminalLineReceived(String),
}

pub struct LogApp {
    current_page: Page,
    
    // Logs page
    file_path: String,
    is_connected: bool,
    parsed_logs: Vec<LogEntry>,
    selected_index: Option<usize>,
    auto_scroll_locked: bool,
    
    // Filter
    filter_manager: FilterManager,
    new_filter_field: FilterField,
    new_filter_operator: FilterOperator,
    new_filter_value: String,
    
    // Session page
    ssh_host: String,
    ssh_username: String,
    ssh_password: String,
    session_connected: bool,
    config: Config,
    
    // Terminal page
    terminal_input: String,
    terminal_output: Vec<String>,
    
    // Channels
    log_tx: Sender<String>,
    terminal_tx: Sender<String>,
    ui_rx: Receiver<Message>,
}

impl LogApp {
    pub fn new() -> (Self, Task<Message>) {
        let config = Config::load();
        let (ssh_host, ssh_username, ssh_password) = if let Some(session) = config.get_session() {
            (session.host.clone(), session.username.clone(), session.password.clone())
        } else {
            ("0.0.0.0:22".to_string(), "root".to_string(), String::new())
        };

        let (log_tx, log_rx) = channel();
        let (terminal_tx, terminal_rx) = channel();
        let (ui_tx, ui_rx) = channel();

        // 启动日志监听线程
        let ui_tx_clone = ui_tx.clone();
        thread::spawn(move || {
            eprintln!("[DEBUG] Log listener thread started");
            while let Ok(line) = log_rx.recv() {
                eprintln!("[DEBUG] Log thread received: {}", line);
                if let Err(e) = ui_tx_clone.send(Message::LogLineReceived(line)) {
                    eprintln!("[DEBUG] Failed to send LogLineReceived: {}", e);
                    break;
                }
            }
            eprintln!("[DEBUG] Log listener thread ended");
        });

        // 启动终端监听线程
        let ui_tx_clone = ui_tx.clone();
        thread::spawn(move || {
            eprintln!("[DEBUG] Terminal listener thread started");
            while let Ok(line) = terminal_rx.recv() {
                eprintln!("[DEBUG] Terminal thread received: {}", line);
                if let Err(e) = ui_tx_clone.send(Message::TerminalLineReceived(line)) {
                    eprintln!("[DEBUG] Failed to send TerminalLineReceived: {}", e);
                    break;
                }
            }
            eprintln!("[DEBUG] Terminal listener thread ended");
        });

        let app = Self {
            current_page: Page::Logs,
            file_path: "/tmp/running.INFO".to_string(),
            is_connected: false,
            parsed_logs: vec![],
            selected_index: None,
            auto_scroll_locked: true,
            filter_manager: FilterManager::new(),
            new_filter_field: FilterField::Message,
            new_filter_operator: FilterOperator::Contains,
            new_filter_value: String::new(),
            ssh_host,
            ssh_username,
            ssh_password,
            session_connected: false,
            config,
            terminal_input: String::new(),
            terminal_output: vec![],
            log_tx,
            terminal_tx,
            ui_rx,
        };

        (app, Task::none())
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PageChanged(page) => self.current_page = page,
            Message::FilePathChanged(path) => self.file_path = path,
            Message::OkPressed => {
                self.is_connected = true;
                self.parsed_logs.clear();
                
                let ssh_config = SshConfig {
                    host: self.ssh_host.clone(),
                    username: self.ssh_username.clone(),
                    password: self.ssh_password.clone(),
                };
                let ssh_client = SshClient::new(ssh_config);
                let file_path = self.file_path.clone();
                let log_tx = self.log_tx.clone();
                
                thread::spawn(move || {
                    let (cmd_tx, cmd_rx) = channel();
                    cmd_tx.send(file_path).ok();
                    let _ = ssh_client.tail_logs(cmd_rx, log_tx);
                });
            }
            Message::ClearPressed => {
                self.parsed_logs.clear();
                self.selected_index = None;
            }
            Message::StartPressed => {
                self.is_connected = true;
                
                let ssh_config = SshConfig {
                    host: self.ssh_host.clone(),
                    username: self.ssh_username.clone(),
                    password: self.ssh_password.clone(),
                };
                let ssh_client = SshClient::new(ssh_config);
                let file_path = self.file_path.clone();
                let log_tx = self.log_tx.clone();
                
                thread::spawn(move || {
                    let (cmd_tx, cmd_rx) = channel();
                    cmd_tx.send(file_path).ok();
                    let _ = ssh_client.tail_logs(cmd_rx, log_tx);
                });
            }
            Message::StopPressed => {
                self.is_connected = false;
            }
            Message::LockToggled => {
                self.auto_scroll_locked = !self.auto_scroll_locked;
            }
            Message::FilterFieldChanged(field) => {
                self.new_filter_field = field;
            }
            Message::FilterOperatorChanged(op) => {
                self.new_filter_operator = op;
            }
            Message::FilterValueChanged(value) => {
                self.new_filter_value = value;
            }
            Message::AddFilterPressed => {
                let rule = FilterRule {
                    field: self.new_filter_field.clone(),
                    operator: self.new_filter_operator.clone(),
                    value: self.new_filter_value.clone(),
                };
                self.filter_manager.add_rule(rule);
                self.new_filter_value.clear();
            }
            Message::RemoveFilter(idx) => {
                self.filter_manager.remove_rule(idx);
            }
            Message::SshHostChanged(host) => self.ssh_host = host,
            Message::SshUsernameChanged(username) => self.ssh_username = username,
            Message::SshPasswordChanged(password) => self.ssh_password = password,
            Message::ConnectPressed => {
                self.session_connected = true;
                let session = SessionConfig {
                    host: self.ssh_host.clone(),
                    username: self.ssh_username.clone(),
                    password: self.ssh_password.clone(),
                };
                let _ = self.config.save_session(session);
            }
            Message::TerminalInputChanged(input) => self.terminal_input = input,
            Message::ExecutePressed => {
                if !self.terminal_input.is_empty() {
                    self.terminal_output.push(format!("> {}", self.terminal_input));
                    
                    let ssh_config = SshConfig {
                        host: self.ssh_host.clone(),
                        username: self.ssh_username.clone(),
                        password: self.ssh_password.clone(),
                    };
                    let ssh_client = SshClient::new(ssh_config);
                    let cmd = self.terminal_input.clone();
                    let terminal_tx = self.terminal_tx.clone();
                    
                    thread::spawn(move || {
                        let (cmd_tx, cmd_rx) = channel();
                        cmd_tx.send(cmd).ok();
                        let _ = ssh_client.execute_shell_command(cmd_rx, terminal_tx);
                    });
                    
                    self.terminal_input.clear();
                }
            }
            Message::LogSelected(idx) => {
                self.selected_index = Some(idx);
            }
            Message::LogLineReceived(line) => {
                eprintln!("[DEBUG] Received log line: {}", line);
                if let Some(entry) = crate::parser::parse_log_line(&line) {
                    eprintln!("[DEBUG] Parsed log entry: {:?}", entry);
                    self.parsed_logs.push(entry);
                } else {
                    eprintln!("[DEBUG] Failed to parse log line");
                }
            }
            Message::TerminalLineReceived(line) => {
                let output = if line.starts_with("[TERMINAL]") {
                    line.strip_prefix("[TERMINAL]").unwrap_or(&line).to_string()
                } else {
                    line
                };
                self.terminal_output.push(output);
            }
        }
        
        Task::none()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::none()
    }

    pub fn view(&self) -> Element<Message> {
        let content = match self.current_page {
            Page::Logs => self.view_logs_page(),
            Page::Session => self.view_session_page(),
            Page::Settings => self.view_settings_page(),
            Page::Terminal => self.view_terminal_page(),
        };

        let sidebar = column![
            text("Menu").size(24),
            button(text("Logs")).on_press(Message::PageChanged(Page::Logs)),
            button(text("Session")).on_press(Message::PageChanged(Page::Session)),
            button(text("Settings")).on_press(Message::PageChanged(Page::Settings)),
            button(text("Terminal")).on_press(Message::PageChanged(Page::Terminal)),
        ]
        .spacing(10)
        .padding(10);

        let layout = row![
            container(sidebar)
                .width(Length::Fixed(150.0))
                .height(Length::Fill),
            container(content)
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(10),
        ]
        .spacing(0);

        container(layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn view_logs_page(&self) -> Element<Message> {
        let file_input = row![
            text("File Path:"),
            text_input("Enter file path", &self.file_path)
                .on_input(Message::FilePathChanged)
                .width(Length::Fill),
            button("OK").on_press(Message::OkPressed),
            button("Clear").on_press(Message::ClearPressed),
            if self.is_connected {
                button("Stop").on_press(Message::StopPressed)
            } else {
                button("Start").on_press(Message::StartPressed)
            },
            if self.is_connected {
                text("Connected")
            } else {
                text("Disconnected")
            },
            button(if self.auto_scroll_locked { "Locked" } else { "Unlocked" })
                .on_press(Message::LockToggled),
        ]
        .spacing(5);

        let filter_section = self.view_filters();
        let logs_table = self.view_logs_table();

        column![
            file_input,
            filter_section,
            logs_table,
        ]
        .spacing(10)
        .padding(10)
        .into()
    }

    fn view_filters(&self) -> Element<Message> {
        let mut filter_list = column![text("Filters").size(18)].spacing(5);

        for (idx, rule) in self.filter_manager.rules.iter().enumerate() {
            let filter_row = row![
                text(format!("{:?} {:?} {}", rule.field, rule.operator, rule.value)),
                button("Remove").on_press(Message::RemoveFilter(idx)),
            ]
            .spacing(5);
            filter_list = filter_list.push(filter_row);
        }

        let add_filter = row![
            text("Add:"),
            pick_list(
                &[FilterField::Level, FilterField::Timestamp, FilterField::FileLine, FilterField::Message][..],
                Some(self.new_filter_field),
                Message::FilterFieldChanged,
            ),
            pick_list(
                &[FilterOperator::Contains, FilterOperator::NotContains, FilterOperator::Equals, FilterOperator::NotEquals][..],
                Some(self.new_filter_operator),
                Message::FilterOperatorChanged,
            ),
            text_input("Filter value", &self.new_filter_value)
                .on_input(Message::FilterValueChanged)
                .width(Length::Fill),
            button("Add").on_press(Message::AddFilterPressed),
        ]
        .spacing(5);

        filter_list = filter_list.push(add_filter);

        container(filter_list)
            .padding(10)
            .into()
    }

    fn view_logs_table(&self) -> Element<Message> {
        let mut logs_col = column![
            row![
                text("Level").width(Length::Fixed(60.0)),
                text("Timestamp").width(Length::Fixed(180.0)),
                text("File:Line").width(Length::Fixed(150.0)),
                text("Message").width(Length::Fill),
            ]
            .spacing(5),
        ]
        .spacing(5);

        for (index, entry) in self.parsed_logs.iter().enumerate() {
            if !self.filter_manager.matches(entry) {
                continue;
            }

            let _is_selected = self.selected_index == Some(index);

            let log_row = row![
                text(&entry.level).width(Length::Fixed(60.0)),
                text(&entry.timestamp).width(Length::Fixed(180.0)),
                text(&entry.file_line).width(Length::Fixed(150.0)),
                text(&entry.message).width(Length::Fill),
            ]
            .spacing(5)
            .padding(5);

            let log_button = button(log_row)
                .on_press(Message::LogSelected(index))
                .width(Length::Fill);

            logs_col = logs_col.push(log_button);
        }

        scrollable(logs_col)
            .height(Length::Fill)
            .into()
    }

    fn view_session_page(&self) -> Element<Message> {
        column![
            text("Session Settings").size(24),
            text("SSH Connection"),
            row![
                text("Host:").width(Length::Fixed(100.0)),
                text_input("Host", &self.ssh_host)
                    .on_input(Message::SshHostChanged)
                    .width(Length::Fill),
            ]
            .spacing(10),
            row![
                text("Username:").width(Length::Fixed(100.0)),
                text_input("Username", &self.ssh_username)
                    .on_input(Message::SshUsernameChanged)
                    .width(Length::Fill),
            ]
            .spacing(10),
            row![
                text("Password:").width(Length::Fixed(100.0)),
                text_input("Password", &self.ssh_password)
                    .on_input(Message::SshPasswordChanged)
                    .width(Length::Fill),
            ]
            .spacing(10),
            button("Connect").on_press(Message::ConnectPressed),
            if self.session_connected {
                text("Status: Connected")
            } else {
                text("Status: Disconnected")
            },
        ]
        .spacing(10)
        .padding(10)
        .into()
    }

    fn view_settings_page(&self) -> Element<Message> {
        column![
            text("Settings").size(24),
            text("Font Settings"),
            text("(Font size and type options coming soon)"),
            text("Other Settings"),
            text("(More options coming soon)"),
        ]
        .spacing(10)
        .padding(10)
        .into()
    }

    fn view_terminal_page(&self) -> Element<Message> {
        let output_items: Vec<Element<Message>> = self.terminal_output
            .iter()
            .map(|line| text(line).into())
            .collect();
        
        let output = column(output_items).spacing(5);

        column![
            text("Terminal").size(24),
            scrollable(output).height(Length::Fill),
            row![
                text_input("Enter command", &self.terminal_input)
                    .on_input(Message::TerminalInputChanged)
                    .width(Length::Fill),
                button("Execute").on_press(Message::ExecutePressed),
            ]
            .spacing(5),
        ]
        .spacing(10)
        .padding(10)
        .into()
    }
}
