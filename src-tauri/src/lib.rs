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
        .plugin(tauri_plugin_clipboard_manager::init())
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
            start_dev_bridge(app.handle());
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
            commands::app_info,
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

/// Events a browser client has to receive; the window client gets them from Tauri itself.
#[cfg(debug_assertions)]
const BRIDGED_EVENTS: [&str; 8] = [
    modules::pty::EVT_OUTPUT,
    modules::pty::EVT_EXIT,
    modules::serial::EVT_DATA,
    modules::serial::EVT_EXIT,
    modules::logger::EVT_DATA,
    modules::logger::EVT_STATE,
    modules::logger::EVT_EXIT,
    modules::sysstat::EVT_STATS,
];

/// Serve commands and events to a browser on loopback, so a page loaded from the dev server
/// can reach the same backend the window does. Development only: a release build has no
/// business answering commands over a socket.
#[cfg(debug_assertions)]
fn start_dev_bridge(app: &tauri::AppHandle) {
    use tauri::Listener;

    let handle = app.clone();
    let dispatch: modules::devbridge::Dispatch = std::sync::Arc::new(move |cmd, args| {
        tauri::async_runtime::block_on(commands::devbridge_call(handle.clone(), cmd, args))
    });
    let hub = match modules::devbridge::start(modules::devbridge::ADDR, dispatch) {
        Ok(hub) => hub,
        Err(error) => {
            modules::logfile::error("app", format!("dev bridge not started: {error}"));
            return;
        }
    };
    for name in BRIDGED_EVENTS {
        let hub = hub.clone();
        let _ = app.listen_any(name, move |event| {
            hub.publish(modules::devbridge::Event {
                name,
                payload: event.payload().to_string(),
            });
        });
    }
    modules::logfile::info("app", format!("dev bridge on {}", modules::devbridge::ADDR));
}

/// Release builds ship without the browser bridge.
#[cfg(not(debug_assertions))]
fn start_dev_bridge(_app: &tauri::AppHandle) {}
