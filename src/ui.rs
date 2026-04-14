use eframe::egui;
use std::sync::mpsc;
use crate::parser::{parse_log_line, LogEntry};

#[derive(PartialEq)]
enum Page {
    Logs,
    Session,
    Settings,
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
}

impl LogApp {
    pub fn new(rx: mpsc::Receiver<String>) -> Self {
        Self {
            rx,
            lines: vec![],
            parsed_logs: vec![],
            selected_index: None,
            file_path: "/tmp/running.INFO".to_string(),
            is_connected: false,
            tx: None,
            current_page: Page::Logs,
            ssh_host: "0.0.0.0:22".to_string(),
            ssh_username: "root".to_string(),
            ssh_password: String::new(),
            session_connected: false,
            auto_scroll_locked: true,
        }
    }

    pub fn set_tx(&mut self, tx: mpsc::Sender<String>) {
        self.tx = Some(tx);
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
                });
            });

        // 主内容区
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.current_page {
                Page::Logs => self.show_logs_page(ui),
                Page::Session => self.show_session_page(ui),
                Page::Settings => self.show_settings_page(ui),
            }
        });

        // 持续刷新
        ctx.request_repaint();
    }
}

impl LogApp {
    fn show_logs_page(&mut self, ui: &mut egui::Ui) {
        // 输入框和确定按钮
        ui.horizontal(|ui| {
            ui.label("File Path:");
            
            // 文件路径输入框
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

        // 表格显示解析后的日志
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

                    // 日志行
                    for (index, entry) in self.parsed_logs.iter().enumerate() {
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
            }
        }

        ui.separator();

        if self.session_connected {
            ui.label("Status: Connected");
        } else {
            ui.label("Status: Disconnected");
        }
    }
}
