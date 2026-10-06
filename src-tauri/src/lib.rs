//! melogger backend assembly entry
//!
//! Mounts global state and registers IPC commands.

/*
 * @file lib.rs
 * @brief Backend assembly entry: mounts AppState, registers IPC commands
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-08-23
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 */
mod commands;
mod modules;
mod state;

/// App entry: mounts global state, opens the run's log, starts the status-bar sampler,
/// registers IPC commands
pub fn run() {
    tauri::Builder::default()
        .manage(state::AppState::default())
        .setup(|app| {
            // First thing in the process: everything after this line can be logged. A log
            // that cannot be opened is not fatal, stdout still carries the lines.
            match modules::logfile::init(app.handle()) {
                Ok(path) => modules::logfile::info(
                    "app",
                    format!(
                        "melogger {} started, log file {}",
                        env!("CARGO_PKG_VERSION"),
                        path.display()
                    ),
                ),
                Err(err) => modules::logfile::error("app", format!("no log file: {err}")),
            }
            modules::logfile::hook_panics();
            // Pushes one sys-stats event per second for the status bar.
            modules::logfile::info("app", "status-bar sampler started");
            modules::sysstat::start_loop(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_ping,
            commands::log_write,
            commands::set_zoom,
            commands::spawn_terminal,
            commands::pty_input,
            commands::pty_resize,
            commands::pty_kill,
            commands::terminal_cwd,
            commands::serial_list,
            commands::font_list,
            commands::settings_load,
            commands::settings_save,
            commands::session_load,
            commands::session_save,
            commands::serial_open,
            commands::serial_write,
            commands::serial_close,
            commands::log_open_file,
            commands::log_complete,
            commands::log_connect,
            commands::log_close
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
