mod ssh;
mod ui;

use eframe::egui;
use std::sync::mpsc;
use std::thread;
use ui::LogApp;
use ssh::{SshClient, SshConfig};

fn main() -> eframe::Result<()> {
    // SSH 配置在这里初始化
    let ssh_config = SshConfig {
        host: "192.168.1.98:22".to_string(),
        username: "root".to_string(),
        password: "laimou8981".to_string(),
    };

    // 两个 channel：
    // - file_path_tx: UI 发送文件路径给后台线程
    // - log_rx: 后台线程发送日志给 UI
    let (file_path_tx, file_path_rx) = mpsc::channel::<String>();
    let (log_tx, log_rx) = mpsc::channel::<String>();

    // SSH 连接在后台线程
    thread::spawn(move || {
        let client = SshClient::new(ssh_config);
        if let Err(e) = client.tail_logs(file_path_rx, log_tx) {
            eprintln!("SSH 错误: {}", e);
        }
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "More Effective Logger",
        options,
        Box::new(|_cc| {
            let mut app = LogApp::new(log_rx);
            app.set_tx(file_path_tx);
            Ok(Box::new(app))
        }),
    )
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 尝试使用系统字体，Linux 上通常有 DejaVu 或其他字体
    // 如果需要中文支持，可以手动放一个 .ttf 文件到 assets/ 目录

    // 这里先用默认字体，支持 ASCII 和大部分符号
    // 如果要支持中文，需要手动下载字体文件放到 assets/NotoSansCJK-Regular.ttf

    ctx.set_fonts(fonts);
}
