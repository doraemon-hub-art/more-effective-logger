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

/// App entry: mounts global state and registers IPC commands
pub fn run() {
    tauri::Builder::default()
        .manage(state::AppState::default())
        .invoke_handler(tauri::generate_handler![commands::app_ping])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
