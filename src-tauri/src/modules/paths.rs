//! Path helpers used by more than one module
//!
//! A path typed into a pane is taken literally, except for a leading tilde: only a shell
//! expands that, and no pane here goes through a shell.

/*
 * @file paths.rs
 * @brief Path helpers shared by the panes: tilde expansion
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-24
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 */

/// A typed path with a leading `~` resolved against the home directory.
pub fn expand_home(path: &str) -> String {
    let home = std::env::var("HOME").ok().filter(|home| !home.is_empty());
    match (home, path) {
        (Some(home), "~") => home,
        (Some(home), rest) if rest.starts_with("~/") => format!("{home}/{}", &rest[2..]),
        _ => path.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tilde is this side's job: nothing here goes through a shell.
    #[test]
    fn tilde_is_expanded() {
        let home = std::env::var("HOME").unwrap_or_default();
        if home.is_empty() {
            return;
        }
        assert_eq!(expand_home("~"), home);
        assert_eq!(expand_home("~/x"), format!("{home}/x"));
        assert_eq!(expand_home("/var/log"), "/var/log");
    }
}
