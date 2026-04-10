use eframe::egui;
use std::sync::mpsc;

pub struct LogApp {
    pub rx: mpsc::Receiver<String>,
    pub lines: Vec<String>,
    pub selected_index: Option<usize>,
    pub file_path: String,
    pub is_connected: bool,
    pub tx: Option<mpsc::Sender<String>>,
}

impl LogApp {
    pub fn new(rx: mpsc::Receiver<String>) -> Self {
        Self {
            rx,
            lines: vec![],
            selected_index: None,
            file_path: "/tmp/running.INFO".to_string(),
            is_connected: false,
            tx: None,
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
            self.lines.push(line);
        }

        egui::TopBottomPanel::top("input_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("File Path:");
                ui.text_edit_singleline(&mut self.file_path);

                if ui.button("Start").clicked() {
                    // 发送文件路径给后台线程
                    if let Some(ref tx) = self.tx {
                        tx.send(self.file_path.clone()).ok();
                        self.is_connected = true;
                        self.lines.clear(); // 清空旧日志
                    }
                }

                if self.is_connected {
                    ui.label("✓ 已连接");
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // 滚动区域，类似 QListWidget
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .stick_to_bottom(true) // 自动滚到最新一行
                .show(ui, |ui| {
                    for (index, line) in self.lines.iter().enumerate() {
                        let is_selected = self.selected_index == Some(index);

                        // 创建可点击的行
                        let response = ui.selectable_label(is_selected, line);

                        // 点击时更新选中状态
                        if response.clicked() {
                            self.selected_index = Some(index);
                        }
                    }
                });
        });

        // 持续刷新，不然新数据不会触发重绘
        ctx.request_repaint();
    }
}
