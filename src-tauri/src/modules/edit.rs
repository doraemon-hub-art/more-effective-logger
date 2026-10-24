//! Files for the edit pane: a directory on this machine, or one on a host reached over SFTP.
//!
//! The two kinds differ only in where the bytes come from, so they hand back the same shapes
//! and the pane never learns which one it is looking at. Local paths are taken literally
//! (with the usual tilde); remote ones start at the login directory when the caller asks for
//! `.`, and every reply carries the path it actually read, so the pane can show where it is.
//!
//! A remote link keeps only the session: an SFTP channel borrows it, so a channel is opened
//! per call instead of being stored next to the session that owns it.

/*
 * @file edit.rs
 * @brief File tree and file contents for the edit pane: local filesystem or SFTP
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-24
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 */

use super::paths::expand_home;
use super::ssh;
use serde::Serialize;
use ssh2::Session;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// One row of a directory listing.
#[derive(Serialize)]
pub struct Entry {
    /// Name only, no path: the pane joins it with the directory it asked about.
    pub name: String,
    /// A directory can be opened; anything else is a file.
    pub dir: bool,
    /// Size in bytes; zero for directories.
    pub size: u64,
}

/// What one directory held when it was read.
#[derive(Serialize)]
pub struct Listing {
    /// The directory that was read, as the pane should show it.
    pub path: String,
    pub entries: Vec<Entry>,
}

/// Where one edit pane reads and writes.
pub enum Link {
    /// A directory on this machine; every call carries the path it works on.
    Local,
    /// A logged-in host; the channel is opened per call, so only the session is kept.
    Remote { session: Arc<Mutex<Session>> },
}

/// The open links, keyed by the pane id the frontend uses.
pub type Links = Mutex<HashMap<String, Link>>;

/// Read a local directory and make it a link for this pane.
pub fn open_local(links: &Links, id: &str, dir: &str) -> Result<Listing, String> {
    let path = PathBuf::from(expand_home(dir));
    let listing = list_local(&path)?;
    let mut open = links.lock().map_err(|e| e.to_string())?;
    open.insert(id.to_string(), Link::Local);
    Ok(listing)
}

/// Log in and start at the login directory, or at the one asked for.
pub fn open_remote(
    links: &Links,
    id: &str,
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    dir: &str,
) -> Result<Listing, String> {
    let session = ssh::connect(host, port, user, password).map_err(|refused| refused.text())?;
    session.set_timeout(ssh::SESSION_TIMEOUT.as_millis() as u32);
    let listing = {
        let sftp = session.sftp().map_err(|e| format!("SFTP: {e}"))?;
        list_remote(&sftp, if dir.trim().is_empty() { "." } else { dir })?
    };
    let mut open = links.lock().map_err(|e| e.to_string())?;
    open.insert(
        id.to_string(),
        Link::Remote {
            session: Arc::new(Mutex::new(session)),
        },
    );
    Ok(listing)
}

/// List one directory of the pane's link.
pub fn list(links: &Links, id: &str, path: &str) -> Result<Listing, String> {
    let open = links.lock().map_err(|e| e.to_string())?;
    match open.get(id) {
        Some(Link::Local) => list_local(Path::new(&expand_home(path))),
        Some(Link::Remote { session }) => {
            let session = session.lock().map_err(|e| e.to_string())?;
            let sftp = session.sftp().map_err(|e| format!("SFTP: {e}"))?;
            list_remote(&sftp, path)
        }
        None => Err(format!("not linked: {id}")),
    }
}

/// The whole file as text.
pub fn read(links: &Links, id: &str, path: &str) -> Result<String, String> {
    let open = links.lock().map_err(|e| e.to_string())?;
    let bytes = match open.get(id) {
        Some(Link::Local) => {
            let path = expand_home(path);
            fs::read(&path).map_err(|e| local_error(&path, &e))?
        }
        Some(Link::Remote { session }) => {
            let session = session.lock().map_err(|e| e.to_string())?;
            let sftp = session.sftp().map_err(|e| format!("SFTP: {e}"))?;
            let mut file = sftp
                .open(Path::new(path))
                .map_err(|e| sftp_error(path, e))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .map_err(|e| sftp_error(path, e))?;
            bytes
        }
        None => return Err(format!("not linked: {id}")),
    };
    String::from_utf8(bytes).map_err(|_| format!("EILSEQ: invalid UTF-8 — {path}"))
}

/// Replace the file with `text`.
pub fn save(links: &Links, id: &str, path: &str, text: &str) -> Result<(), String> {
    let open = links.lock().map_err(|e| e.to_string())?;
    match open.get(id) {
        Some(Link::Local) => {
            let path = expand_home(path);
            fs::write(&path, text).map_err(|e| local_error(&path, &e))
        }
        Some(Link::Remote { session }) => {
            let session = session.lock().map_err(|e| e.to_string())?;
            let sftp = session.sftp().map_err(|e| format!("SFTP: {e}"))?;
            let mut file = sftp
                .create(Path::new(path))
                .map_err(|e| sftp_error(path, e))?;
            file.write_all(text.as_bytes())
                .map_err(|e| sftp_error(path, e))
        }
        None => Err(format!("not linked: {id}")),
    }
}

/// Drop the link; a remote session is closed by being dropped.
pub fn close(links: &Links, id: &str) {
    if let Ok(mut open) = links.lock() {
        open.remove(id);
    }
}

/// A failed local file operation, worded the way the tools this replaces word it: an errno
/// name, the system's own sentence, then the path.
fn local_error(path: &str, error: &std::io::Error) -> String {
    let code = match error.kind() {
        std::io::ErrorKind::NotFound => "ENOENT",
        std::io::ErrorKind::PermissionDenied => "EACCES",
        std::io::ErrorKind::IsADirectory => "EISDIR",
        std::io::ErrorKind::NotADirectory => "ENOTDIR",
        std::io::ErrorKind::ReadOnlyFilesystem => "EROFS",
        _ => "EIO",
    };
    format!("{code}: {} — {path}", os_text(error))
}

/// The system's sentence without Rust's `(os error N)` tail; the errno name already says it.
fn os_text(error: &std::io::Error) -> String {
    let text = error.to_string();
    match text.find(" (os error") {
        Some(at) => text[..at].to_string(),
        None => text,
    }
}

/// A failed SFTP round trip, in the same shape.
fn sftp_error(path: &str, error: impl std::fmt::Display) -> String {
    format!("SFTP: {error} — {path}")
}

/// One completion candidate: the path typed so far with the rest filled in. Directories
/// carry a trailing slash, so taking one keeps the typing inside it.
#[derive(Serialize)]
pub struct Candidate {
    pub path: String,
    pub dir: bool,
}

/// Completion candidates for a path typed into the pane, local or remote. A pane that has
/// not linked yet still completes: the card is asking about this machine.
pub fn complete(links: &Links, id: &str, prefix: &str) -> Result<Vec<Candidate>, String> {
    let remote = {
        let open = links.lock().map_err(|e| e.to_string())?;
        match open.get(id) {
            Some(Link::Remote { session }) => Some(session.clone()),
            _ => None,
        }
    };
    match remote {
        Some(session) => {
            let session = session.lock().map_err(|e| e.to_string())?;
            let sftp = session.sftp().map_err(|e| format!("SFTP: {e}"))?;
            Ok(complete_remote(&sftp, prefix))
        }
        None => Ok(super::logger::complete(prefix)
            .into_iter()
            .map(|candidate| Candidate {
                path: candidate.path,
                dir: candidate.dir,
            })
            .collect()),
    }
}

/// Read a directory on this machine, directories first, then names.
fn list_local(path: &Path) -> Result<Listing, String> {
    let read = fs::read_dir(path).map_err(|e| local_error(&path.display().to_string(), &e))?;
    let mut entries = Vec::new();
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let meta = entry.metadata().ok();
        let dir = meta.as_ref().map(|meta| meta.is_dir()).unwrap_or(false);
        entries.push(Entry {
            name,
            dir,
            size: if dir {
                0
            } else {
                meta.map(|meta| meta.len()).unwrap_or(0)
            },
        });
    }
    Ok(Listing {
        path: fs::canonicalize(path)
            .map(|full| full.display().to_string())
            .unwrap_or_else(|_| path.display().to_string()),
        entries: sorted(entries),
    })
}

/// Read a directory over SFTP; `.` is the login directory, and the reply carries the real
/// absolute path so the pane shows `/home/whoever` rather than a dot.
fn list_remote(sftp: &ssh2::Sftp, path: &str) -> Result<Listing, String> {
    let asked = if path.trim().is_empty() { "." } else { path };
    let real = sftp
        .realpath(Path::new(asked))
        .map_err(|e| sftp_error(asked, e))?;
    let read = sftp
        .readdir(&real)
        .map_err(|e| sftp_error(&real.display().to_string(), e))?;
    let mut entries = Vec::new();
    for (entry, stat) in read {
        let name = match entry.file_name() {
            Some(name) => name.to_string_lossy().into_owned(),
            None => continue,
        };
        if name == "." || name == ".." {
            continue;
        }
        let dir = stat.is_dir();
        entries.push(Entry {
            name,
            dir,
            size: if dir { 0 } else { stat.size.unwrap_or(0) },
        });
    }
    Ok(Listing {
        path: real.display().to_string(),
        entries: sorted(entries),
    })
}

/// Candidates over SFTP: the directory part is listed, the entries starting with the name
/// part come back, directories with a trailing slash.
fn complete_remote(sftp: &ssh2::Sftp, prefix: &str) -> Vec<Candidate> {
    let (dir, name) = match prefix.rfind('/') {
        Some(at) => (prefix[..=at].to_string(), prefix[at + 1..].to_string()),
        None => (String::new(), prefix.to_string()),
    };
    let listing = if dir.is_empty() { "." } else { dir.as_str() };
    let read = match sftp.readdir(Path::new(listing)) {
        Ok(read) => read,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for (entry, stat) in read {
        let file = match entry.file_name() {
            Some(file) => file.to_string_lossy().into_owned(),
            None => continue,
        };
        // A dotfile shows up once the user has typed the dot, not before.
        if file == "."
            || file == ".."
            || !file.starts_with(&name)
            || (file.starts_with('.') && !name.starts_with('.'))
        {
            continue;
        }
        let is_dir = stat.is_dir();
        out.push(Candidate {
            path: if is_dir {
                format!("{dir}{file}/")
            } else {
                format!("{dir}{file}")
            },
            dir: is_dir,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Directories first, then names, so the tree reads the same whichever side it came from.
fn sorted(mut entries: Vec<Entry>) -> Vec<Entry> {
    entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.cmp(&b.name)));
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Directories come first, then names in order.
    #[test]
    fn directories_lead_the_listing() {
        let entries = sorted(vec![
            Entry {
                name: "b.txt".into(),
                dir: false,
                size: 3,
            },
            Entry {
                name: "z-dir".into(),
                dir: true,
                size: 0,
            },
            Entry {
                name: "a.txt".into(),
                dir: false,
                size: 1,
            },
            Entry {
                name: "a-dir".into(),
                dir: true,
                size: 0,
            },
        ]);
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["a-dir", "z-dir", "a.txt", "b.txt"]);
    }

    /// A local link reads, writes and lists under the pane id it was opened with.
    #[test]
    fn a_local_link_round_trips_a_file() {
        let dir = std::env::temp_dir().join(format!("melogger-edit-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("temp dir");
        let links: Links = Mutex::new(HashMap::new());
        let listing = open_local(&links, "pane-1", &dir.display().to_string()).expect("open");
        assert!(listing.entries.is_empty());

        let file = dir.join("note.txt");
        save(&links, "pane-1", &file.display().to_string(), "hello\n").expect("save");
        assert_eq!(
            read(&links, "pane-1", &file.display().to_string()).expect("read"),
            "hello\n"
        );
        let listing = list(&links, "pane-1", &dir.display().to_string()).expect("list");
        assert_eq!(listing.entries.len(), 1);
        assert_eq!(listing.entries[0].name, "note.txt");
        assert!(!listing.entries[0].dir);

        close(&links, "pane-1");
        assert!(read(&links, "pane-1", &file.display().to_string()).is_err());
        fs::remove_dir_all(&dir).ok();
    }

    /// A pane that never connected says so instead of guessing a path.
    #[test]
    fn an_unopened_pane_is_an_error() {
        let links: Links = Mutex::new(HashMap::new());
        assert!(list(&links, "pane-x", "/").is_err());
        assert!(read(&links, "pane-x", "/etc/hostname").is_err());
    }
}
