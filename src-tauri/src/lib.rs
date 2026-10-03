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

/// App entry: mounts global state, starts the status-bar sampler, registers IPC commands
pub fn run() {
    tauri::Builder::default()
        .manage(state::AppState::default())
        .setup(|app| {
            // Pushes one sys-stats event per second for the status bar.
            modules::sysstat::start_loop(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_ping,
            commands::spawn_terminal,
            commands::pty_input,
            commands::pty_resize,
            commands::pty_kill,
            commands::terminal_cwd,
            commands::serial_list,
            commands::serial_open,
            commands::serial_write,
            commands::serial_close
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
