//! Business sub-module directory
//!
//! Features live here as separate modules, one domain per file:
//! - pty.rs: local terminal sessions (portable-pty)
//! - serial.rs: serial port sessions (/sys scan, raw tty)
//! - sysstat.rs: system statistics for the status bar (/proc, /sys)
//! - logfile.rs: the app's own runtime log (the file this run is written to)
//! - ssh.rs: remote SSH connections (ssh2)
//! - logger.rs: log sources (local file snapshot, remote file followed over SSH)

/*
 * @file mod.rs
 * @brief Business sub-module directory (pty/serial/sysstat/logfile/logger)
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-08-23
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 */

pub mod fonts;
pub mod logfile;
pub mod logger;
pub mod pty;
pub mod serial;
pub mod store;
pub mod sysstat;
