mod ssh;
mod ui;
mod parser;
mod filter;
mod config;

use ui::LogApp;

#[tokio::main]
async fn main() -> iced::Result {
    iced::application("More Effective Logger", LogApp::update, LogApp::view)
        .subscription(LogApp::subscription)
        .run_with(LogApp::new)
}
