//! Global application state.

/*
 * @file state.rs
 * @brief Global app state: AppState (pty session registry + placeholder)
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-08-23
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 */
use std::collections::HashMap;
use std::sync::Mutex;

use crate::modules::pty::PtySession;

/// Global application state
///
/// Shared state for the business modules (PTY, SSH, logging, etc.) is mounted here.
#[derive(Default)]
pub struct AppState {
    /// Placeholder field: verifies Mutex state mounting; replace with real business state later.
    pub placeholder: Mutex<Option<String>>,
    /// Live terminal sessions, keyed by the pane id the frontend uses.
    pub ptys: Mutex<HashMap<String, PtySession>>,
}
