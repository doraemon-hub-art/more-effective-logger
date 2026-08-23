//! Global application state placeholder.

/*
 * @file state.rs
 * @brief Global app state: AppState (Mutex placeholder)
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-08-23
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 */
use std::sync::Mutex;

/// Global application state placeholder
///
/// Shared state for future business modules (PTY, SSH, logging, etc.) is mounted here.
#[derive(Default)]
pub struct AppState {
    /// Placeholder field: verifies Mutex state mounting; replace with real business state later.
    pub placeholder: Mutex<Option<String>>,
}
