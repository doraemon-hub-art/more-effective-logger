//! Installed font enumeration
//!
//! The fonts are the system's, not ours: the picker offers what is actually
//! installed, which only fontconfig can answer.

/*
 * @file fonts.rs
 * @brief Installed monospace families for the settings font picker (fontconfig scan)
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-04
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 */

use std::collections::BTreeSet;
use std::process::Command;

use crate::modules::logfile;

/// What makes a family usable as a terminal font: fixed width, and plain ASCII so
/// icon and emoji faces (fixed width, but no text) stay out of the list.
const MONO_ASCII: &str = ":spacing=mono:charset=0020-007e";

/// Installed monospace families, sorted and deduplicated. Empty when fontconfig is
/// not installed, which leaves the picker its built-in fallback.
pub fn list_mono() -> Vec<String> {
    let output = Command::new("fc-list")
        .arg(MONO_ASCII)
        .arg("--format=%{family[0]}\n")
        // A fixed locale keeps the same family names on every machine.
        .env("LC_ALL", "C")
        .output();
    let Ok(output) = output else {
        logfile::warn("font", "fc-list is not available; the picker falls back");
        return Vec::new();
    };
    if !output.status.success() {
        logfile::warn("font", "fc-list refused the query; the picker falls back");
        return Vec::new();
    }
    let families: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if families.is_empty() {
        logfile::warn(
            "font",
            "fc-list knows no monospace family; the picker falls back",
        );
    } else {
        logfile::debug("font", format!("{} monospace families", families.len()));
    }
    families
}
