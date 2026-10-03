//! Serial port sessions: a raw byte stream over a tty device.

/*
 * @file serial.rs
 * @brief Serial port panels: scan the machine's ports, open one exclusively, pump raw bytes
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-02
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 */

use std::ffi::CString;
use std::fs;
use std::os::fd::RawFd;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

/// Event carrying received bytes; the payload names the pane it belongs to.
pub const EVT_DATA: &str = "serial-data";
/// Event emitted once the port is gone (closed by us, unplugged, or a read error).
pub const EVT_EXIT: &str = "serial-exit";

/// Raw bytes read from a port.
#[derive(Clone, Serialize)]
pub struct DataPayload {
    pub id: String,
    pub data: Vec<u8>,
}

/// The port behind one pane is no longer open.
#[derive(Clone, Serialize)]
pub struct ExitPayload {
    pub id: String,
    pub reason: String,
}

/// One scanned port, as the picker lists it.
#[derive(Clone, Serialize)]
pub struct PortInfo {
    /// Device node the frontend will open, e.g. /dev/ttyUSB0
    pub device: String,
    /// Bound kernel driver, e.g. ch341-uart / cdc_acm / 8250
    pub driver: String,
    /// Bridge name for the common USB adapters, empty for on-board UARTs
    pub chip: String,
    /// VID:PID of the USB device behind the port, empty for on-board UARTs
    pub vid_pid: String,
    /// Device serial number; clone bridges often ship without one
    pub serial: String,
    /// USB topology path, e.g. 1-3.2
    pub usb_path: String,
    /// Pid holding the node open right now, if another process got there first
    pub busy_pid: Option<u32>,
}

/// What the frontend needs right after a successful open.
#[derive(Clone, Serialize)]
pub struct OpenInfo {
    pub id: String,
    pub device: String,
    pub baud: u32,
}

/// One live port. The fd is copied out for writes; only the reader thread closes it.
pub struct SerialSession {
    fd: RawFd,
    stop: Arc<AtomicBool>,
}

impl SerialSession {
    /// Ask the reader thread to finish; it unregisters the session and closes the fd.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Bridge names for the USB-serial chips that turn up most often; anything else
/// falls back to the product string the device reports.
const KNOWN_CHIPS: &[(&str, &str)] = &[
    ("1a86:7523", "CH340"),
    ("1a86:55d4", "CH9102"),
    ("10c4:ea60", "CP2102"),
    ("0403:6001", "FT232R"),
    ("0403:6015", "FT231X"),
    ("067b:2303", "PL2303"),
    ("303a:1001", "USB Serial/JTAG"),
    ("0483:5740", "CDC ACM"),
];

/// What sysfs knows about the USB device behind a port.
struct UsbInfo {
    vid_pid: String,
    serial: String,
    usb_path: String,
    product: String,
}

fn read_trim(path: &Path) -> String {
    fs::read_to_string(path)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Walk up from a tty device directory until the USB node (the one carrying idVendor).
fn usb_info(device_dir: &Path) -> Option<UsbInfo> {
    let mut dir = device_dir.to_path_buf();
    for _ in 0..4 {
        if dir.join("idVendor").exists() {
            let vid = read_trim(&dir.join("idVendor"));
            let pid = read_trim(&dir.join("idProduct"));
            return Some(UsbInfo {
                vid_pid: format!("{vid}:{pid}"),
                serial: read_trim(&dir.join("serial")),
                usb_path: dir.file_name()?.to_string_lossy().to_string(),
                product: read_trim(&dir.join("product")),
            });
        }
        dir = dir.parent()?.to_path_buf();
    }
    None
}

/// Driver bound to the port, taken from uevent and falling back to the driver symlink.
fn driver_of(device_dir: &Path) -> String {
    if let Ok(uevent) = fs::read_to_string(device_dir.join("uevent")) {
        for line in uevent.lines() {
            if let Some(name) = line.strip_prefix("DRIVER=") {
                return name.to_string();
            }
        }
    }
    fs::read_link(device_dir.join("driver"))
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_default()
}

/// Pid that currently holds the device node open, excluding this process.
///
/// Only processes of the same user are visible: another user's (or root's) open file
/// descriptors cannot be read without privileges, and then the port just looks free.
fn busy_pid(device: &str) -> Option<u32> {
    let me = std::process::id();
    let wanted = Path::new(device);
    for entry in fs::read_dir("/proc").ok()?.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        if pid == me {
            continue;
        }
        let Ok(fds) = fs::read_dir(entry.path().join("fd")) else {
            continue;
        };
        for fd in fds.flatten() {
            if fs::read_link(fd.path()).is_ok_and(|p| p.as_path() == wanted) {
                return Some(pid);
            }
        }
    }
    None
}

/// Scan every tty the kernel exposes and describe the ones that are real ports.
///
/// Reads /sys and /proc only: nothing here opens a port, so scanning cannot take a
/// device away from another program (or reset a board through the DTR line).
pub fn list() -> Vec<PortInfo> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/tty") else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let device = format!("/dev/{name}");
        if !Path::new(&device).exists() {
            continue;
        }
        // Virtual terminals (tty0, pts/*) have no `device` link; ports do.
        let device_link = entry.path().join("device");
        if !device_link.exists() {
            continue;
        }
        let device_dir = fs::canonicalize(&device_link).unwrap_or(device_link);
        // The legacy 8250 driver publishes 32 placeholder lines that are not wired to
        // anything; they all sit under /sys/devices/platform/serial8250. A real UART
        // hangs off its own device (pnp/isa/platform node), so this drops only the ghosts.
        if device_dir.to_string_lossy().contains("serial8250") {
            continue;
        }
        let usb = usb_info(&device_dir);
        let chip = usb
            .as_ref()
            .map(|u| {
                KNOWN_CHIPS
                    .iter()
                    .find(|(id, _)| *id == u.vid_pid)
                    .map(|(_, name)| name.to_string())
                    .unwrap_or_else(|| u.product.clone())
            })
            .unwrap_or_default();
        out.push(PortInfo {
            driver: driver_of(&device_dir),
            chip,
            vid_pid: usb.as_ref().map(|u| u.vid_pid.clone()).unwrap_or_default(),
            serial: usb.as_ref().map(|u| u.serial.clone()).unwrap_or_default(),
            usb_path: usb.as_ref().map(|u| u.usb_path.clone()).unwrap_or_default(),
            busy_pid: busy_pid(&device),
            device,
        });
    }
    out.sort_by(|a, b| a.device.cmp(&b.device));
    out
}

/// Standard rates we accept, mapped to the kernel's encoding.
///
/// Anything else needs the Linux-specific BOTHER path, which is not worth it until
/// someone actually has a device on an odd rate.
fn speed_const(baud: u32) -> Option<libc::speed_t> {
    Some(match baud {
        9600 => libc::B9600,
        19200 => libc::B19200,
        38400 => libc::B38400,
        57600 => libc::B57600,
        115200 => libc::B115200,
        230400 => libc::B230400,
        460800 => libc::B460800,
        921600 => libc::B921600,
        _ => return None,
    })
}

/// Put the fd into raw mode at the requested rate, 8N1, no flow control.
///
/// The raw part is not cosmetic: a tty defaults to ICRNL, which rewrites a received
/// 0x0D into 0x0A, and OPOST, which rewrites what we send. Either one corrupts a
/// binary payload, so both are switched off here.
fn configure(fd: RawFd, speed: libc::speed_t) -> Result<(), String> {
    let mut tty: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(fd, &mut tty) } != 0 {
        return Err(format!("tcgetattr: {}", std::io::Error::last_os_error()));
    }
    unsafe { libc::cfmakeraw(&mut tty) };
    tty.c_cflag |= libc::CLOCAL | libc::CREAD;
    tty.c_cflag &= !(libc::CSIZE | libc::PARENB | libc::PARODD | libc::CSTOPB | libc::CRTSCTS);
    tty.c_cflag |= libc::CS8;
    // The reader polls instead of blocking, so it must never wait inside read().
    tty.c_cc[libc::VMIN] = 0;
    tty.c_cc[libc::VTIME] = 0;
    let in_speed = unsafe { libc::cfsetispeed(&mut tty, speed) };
    let out_speed = unsafe { libc::cfsetospeed(&mut tty, speed) };
    if in_speed != 0 || out_speed != 0 {
        return Err("cfsetispeed/cfsetospeed failed".into());
    }
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &tty) } != 0 {
        return Err(format!("tcsetattr: {}", std::io::Error::last_os_error()));
    }
    Ok(())
}

/// Open a port exclusively and start pumping its bytes to the frontend.
pub fn open(app: &AppHandle, id: &str, device: &str, baud: u32) -> Result<OpenInfo, String> {
    if !device.starts_with("/dev/") {
        return Err(format!("not a device path: {device}"));
    }
    if !Path::new(device).exists() {
        return Err(format!("{device} is not there"));
    }
    if let Some(pid) = busy_pid(device) {
        return Err(format!("{device} 正被 pid {pid} 占用"));
    }
    if app
        .state::<AppState>()
        .serials
        .lock()
        .map_err(|e| e.to_string())?
        .contains_key(id)
    {
        return Err(format!("serial {id} already exists"));
    }
    let speed = speed_const(baud).ok_or_else(|| format!("unsupported baud rate: {baud}"))?;

    let path = CString::new(device).map_err(|e| e.to_string())?;
    let fd = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_RDWR | libc::O_NOCTTY | libc::O_NONBLOCK,
        )
    };
    if fd < 0 {
        return Err(format!(
            "open {device} failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    // Exclusive: without it a second program opens the same port happily and the two
    // readers split the byte stream between them.
    unsafe { libc::ioctl(fd, libc::TIOCEXCL) };

    if let Err(err) = configure(fd, speed) {
        unsafe { libc::close(fd) };
        return Err(err);
    }
    // Drop whatever piled up in the tty buffer before we attached.
    unsafe { libc::tcflush(fd, libc::TCIOFLUSH) };

    let stop = Arc::new(AtomicBool::new(false));
    app.state::<AppState>()
        .serials
        .lock()
        .map_err(|e| e.to_string())?
        .insert(
            id.to_string(),
            SerialSession {
                fd,
                stop: stop.clone(),
            },
        );

    let app_handle = app.clone();
    let session_id = id.to_string();
    thread::spawn(move || {
        let reason = reader_loop(&app_handle, &session_id, fd, &stop);
        // Unregister before closing: a write command either finds the session (fd still
        // valid, and it holds the lock so the close below waits) or does not find it.
        if let Ok(mut sessions) = app_handle.state::<AppState>().serials.lock() {
            sessions.remove(&session_id);
        }
        unsafe { libc::close(fd) };
        let _ = app_handle.emit(
            EVT_EXIT,
            ExitPayload {
                id: session_id,
                reason,
            },
        );
    });

    Ok(OpenInfo {
        id: id.to_string(),
        device: device.to_string(),
        baud,
    })
}

/// Pump bytes until asked to stop or the device goes away. Returns why it ended.
fn reader_loop(app: &AppHandle, id: &str, fd: RawFd, stop: &AtomicBool) -> String {
    let mut buf = [0u8; 4096];
    loop {
        if stop.load(Ordering::Relaxed) {
            return String::new();
        }
        let mut poll_fd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // A timeout instead of a blocking read: the loop has to notice `stop` even when
        // the line stays silent, and read() on a blocking tty would never come back.
        let ready = unsafe { libc::poll(&mut poll_fd, 1, 200) };
        if ready < 0 {
            return format!("poll: {}", std::io::Error::last_os_error());
        }
        if ready == 0 {
            continue;
        }
        if poll_fd.revents & libc::POLLIN == 0 {
            return "设备已断开".into();
        }
        let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        if n > 0 {
            let _ = app.emit(
                EVT_DATA,
                DataPayload {
                    id: id.to_string(),
                    data: buf[..n as usize].to_vec(),
                },
            );
        } else if n < 0 {
            let err = std::io::Error::last_os_error();
            match err.raw_os_error() {
                Some(libc::EAGAIN) | Some(libc::EINTR) => continue,
                _ => return err.to_string(),
            }
        }
    }
}

/// Push bytes out of an open port.
pub fn write(state: &AppState, id: &str, data: &[u8]) -> Result<(), String> {
    let sessions = state.serials.lock().map_err(|e| e.to_string())?;
    let session = sessions
        .get(id)
        .ok_or_else(|| format!("unknown serial: {id}"))?;
    let mut sent = 0;
    while sent < data.len() {
        let n = unsafe { libc::write(session.fd, data[sent..].as_ptr().cast(), data.len() - sent) };
        if n > 0 {
            sent += n as usize;
            continue;
        }
        let err = std::io::Error::last_os_error();
        match err.raw_os_error() {
            Some(libc::EINTR) => continue,
            Some(libc::EAGAIN) => {
                let mut poll_fd = libc::pollfd {
                    fd: session.fd,
                    events: libc::POLLOUT,
                    revents: 0,
                };
                if unsafe { libc::poll(&mut poll_fd, 1, 500) } <= 0 {
                    return Err("write timed out".into());
                }
            }
            _ => return Err(err.to_string()),
        }
    }
    Ok(())
}

/// Ask the reader thread of a port to finish.
pub fn close(state: &AppState, id: &str) -> Result<(), String> {
    let sessions = state.serials.lock().map_err(|e| e.to_string())?;
    if let Some(session) = sessions.get(id) {
        session.stop();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The scan must never come back empty-handed on a machine with a bridged port,
    /// and must never list virtual terminals.
    #[test]
    fn scan_skips_virtual_terminals() {
        let ports = list();
        for port in &ports {
            assert!(port.device.starts_with("/dev/"), "{:?}", port.device);
            assert!(
                !port.device.contains("/pts/"),
                "virtual terminal listed: {}",
                port.device
            );
        }
    }

    /// Rates come from a table; an unsupported one has to be refused, not rounded.
    #[test]
    fn rate_table() {
        assert!(speed_const(115200).is_some());
        assert!(speed_const(9600).is_some());
        assert!(speed_const(123456).is_none());
    }

    /// A node nobody holds open reports no owner instead of guessing one.
    #[test]
    fn free_node_has_no_owner() {
        assert_eq!(busy_pid("/dev/definitely-not-a-port"), None);
    }

    /// The scan has to agree with /dev: every bridged port node shows up, with the
    /// driver the kernel bound to it. Prints what it saw, for the record.
    #[test]
    fn scan_matches_dev_nodes() {
        let ports = list();
        for port in &ports {
            println!(
                "{}  driver={} chip={} id={} usb={} busy={:?}",
                port.device, port.driver, port.chip, port.vid_pid, port.usb_path, port.busy_pid
            );
        }
        let listed: Vec<String> = ports.iter().map(|p| p.device.clone()).collect();
        let mut bridged = 0;
        for entry in fs::read_dir("/dev").into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !(name.starts_with("ttyUSB") || name.starts_with("ttyACM")) {
                continue;
            }
            let device = format!("/dev/{name}");
            bridged += 1;
            assert!(listed.contains(&device), "{device} missing from scan");
            let port = ports.iter().find(|p| p.device == device).unwrap();
            assert!(!port.driver.is_empty(), "{device} reported no driver");
        }
        println!("bridged ports in /dev: {bridged}, listed: {}", ports.len());
        for port in &ports {
            let name = port.device.trim_start_matches("/dev/");
            let dir = fs::canonicalize(format!("/sys/class/tty/{name}/device")).unwrap_or_default();
            assert!(
                !dir.to_string_lossy().contains("serial8250"),
                "ghost 8250 placeholder listed: {}",
                port.device
            );
        }
    }
}
