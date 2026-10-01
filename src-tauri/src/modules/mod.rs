//! Business sub-module directory
//!
//! Features live here as separate modules, one domain per file:
//! - pty.rs: local terminal sessions (portable-pty)
//! - sysstat.rs: system statistics for the status bar (/proc, /sys)
//! - ssh.rs: remote SSH connections (ssh2)
//! - logger.rs: remote log tail panels

/*
 * @file mod.rs
 * @brief Business sub-module directory (pty/sysstat/ssh/logger)
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-08-23
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 */

pub mod pty;
pub mod sysstat;
