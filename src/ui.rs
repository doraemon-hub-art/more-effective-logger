use eframe::egui;
use std::sync::mpsc;
use crate::parser::{parse_log_line, LogEntry};
use crate::filter::{FilterManager, FilterRule, FilterField, FilterOperator};
use crate::config::{Config, SessionConfig};

#[derive(PartialEq)]
enum Page {
    Logs,
    Session,
    Settings,
    Terminal,
}

pub struct LogApp {
    pub rx: mpsc::Receiver<String>,
    pub lines: Vec<String>,
    pub parsed_logs: Vec<LogEntry>,
    pub selected_index: Option<usize>,
    pub file_path: String,
    pub is_connected: bool,
    pub tx: Option<mpsc::Sender<String>>,
    current_page: Page,
    // Session settings
    ssh_host: String,
    ssh_username: String,
    ssh_password: String,
    session_connected: bool,
    // Auto scroll lock
    auto_scroll_locked: bool,
    // Config
    config: Config,
    // Filter
    filter_manager: FilterManager,
    new_filter_field: FilterField,
    new_filter_operator: FilterOperator,
    new_filter_value: String,
    // Terminal
    terminal_input: String,
    terminal_output: Vec<String>,
    terminal_cmd_tx: Option<mpsc::Sender<String>>,
    terminal_output_rx: Option<mpsc::Receiver<String>>,
}

impl LogApp {
    pub fn new(rx: mpsc::Receiver<String>) -> Self {
        let config = Config::load();
        
        let (ssh_host, ssh_username, ssh_password) = if let Some(session) = config.get_session() {
            (session.host.clone(), session.username.clone(), session.password.clone())
        } else {
            ("0.0.0.0:22".to_string(), "root".to_string(), String::new())
        };
        
        Self {
            rx,
            lines: vec![],
            parsed_logs: vec![],
            selected_index: None,
            file_path: "/tmp/running.INFO".to_string(),
            is_connected: false,
            tx: None,
            current_page: Page::Logs,
            ssh_host,
            ssh_username,
            ssh_password,
            session_connected: false,
            auto_scroll_locked: true,
            config,
            filter_manager: FilterManager::new(),
            new_filter_field: FilterField::Message,
            new_filter_operator: FilterOperator::Contains,
            new_filter_value: String::new(),
            terminal_input: String::new(),
            terminal_output: vec![],
            terminal_cmd_tx: None,
            terminal_output_rx: None,
        }
    }

    pub fn set_tx(&mut self, tx: mpsc::Sender<String>) {
        self.tx = Some(tx);
    }

    pub fn set_terminal_channels(
        &mut self,
        cmd_tx: mpsc::Sender<String>,
        output_rx: mpsc::Receiver<String>,
    ) {
        self.terminal_cmd_tx = Some(cmd_tx);
        self.terminal_output_rx = Some(output_rx);
    }
}

impl eframe::App for LogApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 每帧把 channel 里积压的新行全部收进来
        while let Ok(line) = self.rx.try_recv() {
            self.lines.push(line.clone());
            
            // 尝试解析日志
            if let Some(entry) = parse_log_line(&line) {
                self.parsed_logs.push(entry);
            }
        }

        // 左侧导航栏
        egui::SidePanel::left("nav_panel")
            .resizable(false)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.heading("Menu");
                    ui.separator();

                    if ui
                        .selectable_label(self.current_page == Page::Logs, "Logs")
                        .clicked()
                    {
                        self.current_page = Page::Logs;
                    }

                    if ui
                        .selectable_label(self.current_page == Page::Session, "Session")
                        .clicked()
                    {
                        self.current_page = Page::Session;
                    }

                    if ui
                        .selectable_label(self.current_page == Page::Settings, "Settings")
                        .clicked()
                    {
                        self.current_page = Page::Settings;
                    }

                    if ui
                        .selectable_label(self.current_page == Page::Terminal, "Terminal")
                        .clicked()
                    {
                        self.current_page = Page::Terminal;
                    }
                });
            });

        // 主内容区
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.current_page {
                Page::Logs => self.show_logs_page(ui),
                Page::Session => self.show_session_page(ui),
                Page::Settings => self.show_settings_page(ui),
                Page::Terminal => self.show_terminal_page(ui),
            }
        });

        // 持续刷新
        ctx.request_repaint();
    }
}

impl LogApp {
    fn show_logs_page(&mut self, ui: &mut egui::Ui) {
        // 输入框和确定按钮 - 自适应宽度
        ui.horizontal(|ui| {
            ui.label("File Path:");
            
            // 文件路径输入框 - 填充剩余空间
            ui.text_edit_singleline(&mut self.file_path);

            if ui.button("OK").clicked() {
                if let Some(ref tx) = self.tx {
                    tx.send(self.file_path.clone()).ok();
                    self.is_connected = true;
                    self.lines.clear();
                    self.parsed_logs.clear();
                }
            }

            if ui.button("Clear").clicked() {
                self.lines.clear();
                self.parsed_logs.clear();
                self.selected_index = None;
            }

            if self.is_connected {
                if ui.button("Stop").clicked() {
                    self.is_connected = false;
                }
            } else {
                if ui.button("Start").clicked() {
                    if let Some(ref tx) = self.tx {
                        tx.send(self.file_path.clone()).ok();
                        self.is_connected = true;
                    }
                }
            }

            if self.is_connected {
                ui.label("Connected");
            }

            // Auto scroll lock button
            let lock_text = if self.auto_scroll_locked { "Locked" } else { "Unlocked" };
            if ui.button(lock_text).clicked() {
                self.auto_scroll_locked = !self.auto_scroll_locked;
            }
        });

        ui.separator();

        // 过滤器 UI
        ui.heading("Filters");
        
        // 显示已有的过滤规则
        let mut to_remove = None;
        for (idx, rule) in self.filter_manager.rules.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{:?} {:?} {}", rule.field, rule.operator, rule.value));
                if ui.button("Remove").clicked() {
                    to_remove = Some(idx);
                }
            });
        }
        if let Some(idx) = to_remove {
            self.filter_manager.remove_rule(idx);
        }

        // 添加新过滤规则
        ui.horizontal(|ui| {
            ui.label("Add:");
            
            // 字段选择
            ui.menu_button(format!("{:?}", self.new_filter_field), |ui| {
                if ui.button("Level").clicked() {
                    self.new_filter_field = FilterField::Level;
                }
                if ui.button("Timestamp").clicked() {
                    self.new_filter_field = FilterField::Timestamp;
                }
                if ui.button("FileLine").clicked() {
                    self.new_filter_field = FilterField::FileLine;
                }
                if ui.button("Message").clicked() {
                    self.new_filter_field = FilterField::Message;
                }
            });

            // 操作符选择
            ui.menu_button(format!("{:?}", self.new_filter_operator), |ui| {
                if ui.button("Contains").clicked() {
                    self.new_filter_operator = FilterOperator::Contains;
                }
                if ui.button("NotContains").clicked() {
                    self.new_filter_operator = FilterOperator::NotContains;
                }
                if ui.button("Equals").clicked() {
                    self.new_filter_operator = FilterOperator::Equals;
                }
                if ui.button("NotEquals").clicked() {
                    self.new_filter_operator = FilterOperator::NotEquals;
                }
            });

            // 值输入 - 自适应宽度
            ui.text_edit_singleline(&mut self.new_filter_value);

            // 添加按钮
            if ui.button("Add").clicked() {
                let rule = FilterRule {
                    field: self.new_filter_field.clone(),
                    operator: self.new_filter_operator.clone(),
                    value: self.new_filter_value.clone(),
                };
                self.filter_manager.add_rule(rule);
                self.new_filter_value.clear();
            }
        });

        ui.separator();

        // 表格显示解析后的日志 - 自适应大小
        let scroll_area = egui::ScrollArea::vertical()
            .auto_shrink(false)
            .stick_to_bottom(true);

        scroll_area.show(ui, |ui| {
            egui::Grid::new("logs_grid")
                .striped(true)
                .spacing([3.0, 3.0])
                .show(ui, |ui| {
                    // 表头
                    ui.label(egui::RichText::new("Level").strong());
                    ui.label(egui::RichText::new("Timestamp").strong());
                    ui.label(egui::RichText::new("File:Line").strong());
                    ui.label(egui::RichText::new("Message").strong());
                    ui.end_row();

                    // 日志行 - 只显示符合过滤条件的
                    for (index, entry) in self.parsed_logs.iter().enumerate() {
                        if !self.filter_manager.matches(entry) {
                            continue;
                        }
                        
                        let is_selected = self.selected_index == Some(index);
                        
                        if ui.selectable_label(is_selected, &entry.level).clicked() {
                            self.selected_index = Some(index);
                        }
                        
                        if ui.selectable_label(is_selected, &entry.timestamp).clicked() {
                            self.selected_index = Some(index);
                        }
                        
                        if ui.selectable_label(is_selected, &entry.file_line).clicked() {
                            self.selected_index = Some(index);
                        }
                        
                        if ui.selectable_label(is_selected, &entry.message).clicked() {
                            self.selected_index = Some(index);
                        }
                        ui.end_row();
                    }
                });
        });

        // 禁止用户滚动（当锁定时）
        if self.auto_scroll_locked {
            ui.input(|i| {
                if i.raw_scroll_delta.y != 0.0 {
                    // 用户尝试滚动，但我们忽略它
                }
            });
        }
    }

    fn show_settings_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.separator();

        ui.label("Font Settings");
        ui.label("(Font size and type options coming soon)");

        ui.separator();

        ui.label("Other Settings");
        ui.label("(More options coming soon)");
    }

    fn show_session_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("Session Settings");
        ui.separator();

        ui.label("SSH Connection");
        ui.horizontal(|ui| {
            ui.label("Host:");
            ui.text_edit_singleline(&mut self.ssh_host);
        });

        ui.horizontal(|ui| {
            ui.label("Username:");
            ui.text_edit_singleline(&mut self.ssh_username);
        });

        ui.horizontal(|ui| {
            ui.label("Password:");
            let password_input = ui.text_edit_singleline(&mut self.ssh_password);
            // 显示为 * 号
            if password_input.changed() {
                // 密码字段已更新
            }
        });

        ui.separator();

        if ui.button("Connect").clicked() {
            if let Some(ref tx) = self.tx {
                // Send session config to backend
                let config = format!("{}|{}|{}", self.ssh_host, self.ssh_username, self.ssh_password);
                tx.send(config).ok();
                self.session_connected = true;
                
                // Save session config
                let session = SessionConfig {
                    host: self.ssh_host.clone(),
                    username: self.ssh_username.clone(),
                    password: self.ssh_password.clone(),
                };
                if let Err(e) = self.config.save_session(session) {
                    eprintln!("Failed to save config: {}", e);
                }
            }
        }

        ui.separator();

        if self.session_connected {
            ui.label("Status: Connected");
        } else {
            ui.label("Status: Disconnected");
        }
    }

    fn show_terminal_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("Terminal");
        ui.separator();

        // 接收终端输出
        if let Some(ref rx) = self.terminal_output_rx {
            while let Ok(line) = rx.try_recv() {
                self.terminal_output.push(line);
            }
        }

        // 输出区域 - 自适应大小
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &self.terminal_output {
                    ui.label(line);
                }
            });

        ui.separator();

        // 输入区域 - 自适应宽度
        ui.horizontal(|ui| {
            let response = ui.text_edit_singleline(&mut self.terminal_input);
            
            // 回车或点击按钮执行
            let should_execute = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
                || ui.button("Execute").clicked();

            if should_execute {
                if !self.terminal_input.is_empty() {
                    if let Some(ref tx) = self.terminal_cmd_tx {
                        self.terminal_output.push(format!("> {}", self.terminal_input));
                        tx.send(self.terminal_input.clone()).ok();
                        self.terminal_input.clear();
                    }
                }
            }
        });
    }
}
