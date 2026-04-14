mod ssh;
mod ui;
mod parser;
mod filter;
mod config;

use eframe::egui;
use std::sync::mpsc;
use std::thread;
use ui::LogApp;
use ssh::{SshClient, SshConfig};

fn main() -> eframe::Result<()> {
    // 两个 channel：
    // - config_tx: UI 发送 SSH 配置给后台线程
    // - log_rx: 后台线程发送日志给 UI
    let (config_tx, config_rx) = mpsc::channel::<String>();
    let (log_tx, log_rx) = mpsc::channel::<String>();

    // SSH 连接在后台线程
    thread::spawn(move || {
        // 等待接收 SSH 配置（格式: "host|username|password"）
        if let Ok(config_str) = config_rx.recv() {
            let parts: Vec<&str> = config_str.split('|').collect();
            if parts.len() == 3 {
                let ssh_config = SshConfig {
                    host: parts[0].to_string(),
                    username: parts[1].to_string(),
                    password: parts[2].to_string(),
                };

                println!("SSH Config: host={}, username={}", parts[0], parts[1]);

                let client = SshClient::new(ssh_config);
                
                // 创建一个新的 channel 用于接收文件路径
                let (file_tx, file_rx) = mpsc::channel::<String>();
                
                // 在另一个线程中等待文件路径
                let log_tx_clone = log_tx.clone();
                thread::spawn(move || {
                    if let Err(e) = client.tail_logs(file_rx, log_tx_clone) {
                        eprintln!("SSH error: {}", e);
                    }
                });
                
                // 主线程继续接收文件路径并转发
                while let Ok(file_path) = config_rx.recv() {
                    file_tx.send(file_path).ok();
                }
            }
        }
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "More Effective Logger",
        options,
        Box::new(|cc| {
            setup_fonts(&cc.egui_ctx);
            let mut app = LogApp::new(log_rx);
            app.set_tx(config_tx);
            Ok(Box::new(app))
        }),
    )
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 尝试加载系统中文字体
    // Linux 上常见的中文字体路径
    let font_paths = vec![
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.otf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    ];

    for path in font_paths {
        if std::path::Path::new(path).exists() {
            if let Ok(font_data) = std::fs::read(path) {
                fonts.font_data.insert(
                    "custom_font".to_owned(),
                    egui::FontData::from_owned(font_data),
                );

                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_insert_with(Vec::new)
                    .insert(0, "custom_font".to_owned());

                println!("加载字体: {}", path);
                break;
            }
        }
    }

    ctx.set_fonts(fonts);

    // 设置字体大小
    let mut style = (*ctx.style()).clone();
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(14.0, egui::FontFamily::Proportional),
    );

    style.text_styles.insert(
        egui::TextStyle::Monospace,
        egui::FontId::new(13.0, egui::FontFamily::Monospace),
    );

    ctx.set_style(style);
}
