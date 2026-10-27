//! Directory watching. One thread ("dirwatch") owns every `ReadDirectoryChangesW` watch in the
//! app; the UI thread only sends it commands and drains its events.
//!
//! Each watched folder is a directory handle (`FILE_LIST_DIRECTORY`, opened with
//! `FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED`) associated with one I/O completion port.
//! The thread waits on `GetQueuedCompletionStatus`; commands from the UI thread travel over a
//! channel, announced by a `PostQueuedCompletionStatus` with the completion key `KEY_CMD`, and the
//! wait's timeout is the coalescing timer.
//!
//! Watches are keyed by tab (the UI decides which tabs to watch; at most `MAX_WATCHES` live).
//! A plain watch parses its `FILE_NOTIFY_INFORMATION` records and delivers the net change per
//! name over its coalescing window (`net_changes`); a buffer overflow (`ERROR_NOTIFY_ENUM_DIR`, or
//! an empty completion) is delivered as such and the UI re-enumerates. A subtree watch (a tab
//! with folder sizes, `fs::sizes`) tells a change in the folder itself (the listing changes) from
//! one below one of its subfolders (only that subfolder's size changes). Changes are coalesced per
//! watch: the first change after a quiet spell is delivered `COALESCE` later (the trailing edge, so
//! a burst becomes one event); a folder that keeps changing gets longer windows (`Backoff`).
//!
//! A watch asked for with `parent` has a second handle on its folder's parent, which notices the
//! folder itself being renamed (`Event::Moved`, the watch follows it) or deleted (`Event::Lost`).
//!
//! Network folders are opened, cancelled and closed on helper threads, so a dead share never
//! blocks this thread. A watch on a removable drive registers for the drive's device events
//! (`device_event`, called from `crate::devices`), closes its handle when Windows asks to remove
//! the drive, and reopens it when the removal fails.
//!
//! A watch's state is a `Box<Watch>` whose first field is its `OVERLAPPED`, so the `OVERLAPPED`
//! pointer a completion carries is the watch's identity. A replaced or removed watch is cancelled
//! (`CancelIoEx`, `CloseHandle`) and kept alive until its cancelled read completes
//! (`ERROR_OPERATION_ABORTED`), and for a network folder until the helper thread that cancelled it
//! has reported back: the kernel writes into its buffer until then, and the completion must find
//! the old watch, never a new one at a recycled address.

use crossbeam_channel::{Receiver, Sender, bounded, unbounded};
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf, Prefix};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_NOTIFY_ENUM_DIR, GetLastError, HANDLE, INVALID_HANDLE_VALUE, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED, FILE_LIST_DIRECTORY, FILE_NOTIFY_CHANGE_ATTRIBUTES,
    FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDriveTypeW, GetFinalPathNameByHandleW,
    GetVolumeNameForVolumeMountPointW, OPEN_EXISTING, ReadDirectoryChangesW,
};
use windows_sys::Win32::System::Diagnostics::Debug::{SEM_FAILCRITICALERRORS, SetThreadErrorMode};
use windows_sys::Win32::System::IO::{
    CancelIoEx, CreateIoCompletionPort, GetQueuedCompletionStatus, OVERLAPPED, PostQueuedCompletionStatus,
};
use windows_sys::Win32::System::Threading::INFINITE;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DBT_DEVICEQUERYREMOVE, DBT_DEVICEQUERYREMOVEFAILED, DBT_DEVICEREMOVECOMPLETE, DBT_DEVTYP_HANDLE, DEV_BROADCAST_HANDLE,
    DEVICE_NOTIFY_WINDOW_HANDLE, RegisterDeviceNotificationW, UnregisterDeviceNotification,
};

/// The coalescing window of a watch after a quiet spell.
pub const COALESCE: Duration = Duration::from_millis(250);

/// The longest coalescing window, for a folder that changes continuously.
pub const COALESCE_MAX: Duration = Duration::from_secs(2);

/// A change this long after the last delivery starts again at `COALESCE`.
pub const QUIET: Duration = Duration::from_secs(1);

/// At most this many live watches (parent handles not counted).
pub const MAX_WATCHES: usize = 8;

/// How long the watcher waits for a network folder to open.
pub const OPEN_TIMEOUT: Duration = Duration::from_secs(3);

/// The notification buffer per watch, in bytes (DWORD-aligned: it is a `Vec<u32>`).
const BUF_BYTES: usize = 64 * 1024;

/// Past this many records in one window a plain watch stops gathering them and reports an
/// overflow (the UI lists the folder again).
const MAX_RECORDS: usize = 1000;

/// The buffer of a parent watch (folder names only).
const PARENT_BUF_BYTES: usize = 4 * 1024;

/// Completion key of a command post. Watches use their own (heap) address, never 1 or 2.
const KEY_CMD: usize = 1;

/// Completion key of a network folder's open, posted by its helper thread; the packet's
/// `OVERLAPPED` pointer is a `Box<Opened>`.
const KEY_OPENED: usize = 3;

/// A network folder whose open stalled is refused at once for this long.
const STALL_COOLDOWN: Duration = Duration::from_secs(30);

/// Completion key of a helper thread's report that it has cancelled and closed a network watch;
/// the packet's `OVERLAPPED` pointer is the watch.
const KEY_HELPER: usize = 2;

const FILTER: u32 = FILE_NOTIFY_CHANGE_FILE_NAME
    | FILE_NOTIFY_CHANGE_DIR_NAME
    | FILE_NOTIFY_CHANGE_ATTRIBUTES
    | FILE_NOTIFY_CHANGE_SIZE
    | FILE_NOTIFY_CHANGE_LAST_WRITE;

/// Helper threads that may still post to a port; `Watcher::stop` leaves the port open while any
/// runs.
static HELPERS: AtomicUsize = AtomicUsize::new(0);

/// The running watcher's command sender and port, for `device_event` (which runs in the main
/// window's subclass and touches no app state).
static DEVICE: Mutex<Option<(Sender<Cmd>, usize)>> = Mutex::new(None);

/// The net change of one name over a coalescing window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Added(String),
    Removed(String),
    Modified(String),
    Renamed { from: String, to: String },
}

/// What the watcher tells the UI thread. `tab_id` is the tab the watch was started for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The folder changed (coalesced): the net `changes` per name, from `records` raw records.
    /// `overflow`: what changed is unknown (`changes` is empty); the listing needs a refresh.
    Changed { pane: usize, tab_id: u64, changes: Vec<Change>, records: usize, overflow: bool },
    /// What a subtree watch saw (coalesced). `top`: something in the folder itself changed (an
    /// entry added, removed, renamed or modified), and `edits` its net changes from `records` raw
    /// records in the folder itself, as a plain watch's (P7-M4: row edits), or None when past
    /// `MAX_RECORDS` or on an overflow (the listing needs a refresh). `changes`: each
    /// child of the folder that a record lay below or added, removed or renamed (not one that only
    /// modified the child's own entry), once, with the first such record ("added sub\x.txt"). `overflow`: the buffer overflowed, so what changed is unknown.
    Subtree { pane: usize, tab_id: u64, top: bool, changes: Vec<(String, String)>, overflow: bool, edits: Option<Vec<Change>>, records: usize },
    /// The watched folder itself was renamed; the watch now follows `to`.
    Moved { pane: usize, tab_id: u64, from: PathBuf, to: PathBuf },
    /// No watch for `path` (sent every time it is asked for; logged once).
    Refused { pane: usize, tab_id: u64, path: PathBuf, why: String },
    /// The watch failed after it started (folder deleted, drive gone, access revoked) and is
    /// closed.
    Lost { pane: usize, tab_id: u64, error: String },
}

enum Cmd {
    Watch { pane: usize, tab_id: u64, path: PathBuf, subtree: bool, parent: bool },
    Unwatch(u64),
    UnwatchPane(usize),
    SetHwnd(isize),
    /// Windows asks to remove the drive of the watch registered as this HDEVNOTIFY: close it and
    /// reply whether it was ours.
    Suspend(isize, Sender<bool>),
    Resume(isize),
    Removed(isize),
    /// Reply once every earlier command has been handled (tests).
    #[cfg(test)]
    Sync(Sender<()>),
    Quit,
}

/// The UI thread's handle on the watcher thread. Dropping it stops the thread (and waits up to
/// 1 s for it).
pub struct Watcher {
    /// The completion port (0 if it could not be created: no watching at all).
    port: isize,
    tx: Sender<Cmd>,
    /// Signalled (or disconnected) when the thread has finished. Not a `JoinHandle`: if the
    /// process is already exiting, Windows has terminated the thread, and `join` would panic.
    exited: Option<Receiver<()>>,
}

impl Watcher {
    /// Start the watcher thread. Events go to `tx`, followed by `wake()` (as `fs::enumerate`).
    pub fn spawn(tx: Sender<Event>, wake: impl Fn() + Send + 'static) -> Watcher {
        // SAFETY: creates a new completion port; no file handle yet.
        let port = unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, std::ptr::null_mut(), 0, 1) } as isize;
        let (cmd_tx, cmd_rx) = unbounded();
        if port == 0 {
            eprintln!("[watch] no completion port ({}): directory watching is off", last_error());
            return Watcher { port: 0, tx: cmd_tx, exited: None };
        }
        let (exit_tx, exit_rx) = unbounded();
        std::thread::Builder::new()
            .name("dirwatch".into())
            .spawn(move || {
                Dirwatch {
                    port: port as HANDLE,
                    slots: HashMap::new(),
                    dying: Vec::new(),
                    refused: HashMap::new(),
                    hwnd: 0,
                    opening: HashMap::new(),
                    next_req: 0,
                    stalled: HashMap::new(),
                    no_notify: HashSet::new(),
                    cmds: cmd_rx,
                    events: tx,
                    wake,
                }
                .run();
                let _ = exit_tx.send(());
            })
            .expect("spawn dirwatch thread");
        if let Ok(mut d) = DEVICE.lock() {
            *d = Some((cmd_tx.clone(), port as usize));
        }
        Watcher { port, tx: cmd_tx, exited: Some(exit_rx) }
    }

    fn send(&self, cmd: Cmd) {
        if self.port == 0 || self.tx.send(cmd).is_err() {
            return;
        }
        // SAFETY: posts a packet with no OVERLAPPED to our own port.
        unsafe { PostQueuedCompletionStatus(self.port as HANDLE, 0, KEY_CMD, std::ptr::null()) };
    }

    /// Watch `path` for tab `tab_id` of `pane` (with every folder below it when `subtree`, and the
    /// folder's own rename or delete when `parent`), replacing whatever that tab watched before.
    /// A no-op when the tab already watches that folder the same way.
    pub fn watch(&self, pane: usize, tab_id: u64, path: PathBuf, subtree: bool, parent: bool) {
        self.send(Cmd::Watch { pane, tab_id, path, subtree, parent });
    }

    /// Stop watching for tab `tab_id`.
    pub fn unwatch(&self, tab_id: u64) {
        self.send(Cmd::Unwatch(tab_id));
    }

    /// Stop every watch of `pane` (the split going off).
    pub fn unwatch_pane(&self, pane: usize) {
        self.send(Cmd::UnwatchPane(pane));
    }

    /// The main window, which receives the device events of watches on removable drives.
    pub fn set_hwnd(&self, hwnd: isize) {
        self.send(Cmd::SetHwnd(hwnd));
    }

    /// Wait until the thread has handled every command sent so far.
    #[cfg(test)]
    fn sync(&self) {
        let (tx, rx) = unbounded();
        self.send(Cmd::Sync(tx));
        let _ = rx.recv_timeout(Duration::from_secs(5));
    }

    /// Stop the thread (it cancels every watch first) and wait up to 1 s for it; later calls do
    /// nothing. Called at the end of `main`, while the thread is still alive, and by `drop`.
    pub fn stop(&mut self) {
        self.send(Cmd::Quit);
        let Some(exited) = self.exited.take() else { return };
        let port = std::mem::replace(&mut self.port, 0);
        if let Ok(mut d) = DEVICE.lock() {
            if d.as_ref().is_some_and(|(_, p)| *p == port as usize) {
                *d = None;
            }
        }
        if let Err(crossbeam_channel::RecvTimeoutError::Timeout) = exited.recv_timeout(Duration::from_secs(1)) {
            // Still running (or terminated by process exit): leave the port open for it.
            eprintln!("[watch] the watcher thread did not stop within 1 s");
            return;
        }
        if HELPERS.load(Ordering::SeqCst) > 0 {
            // A helper stuck on a dead share may still post to the port: leave it open.
            return;
        }
        // SAFETY: the port is ours and the thread that waited on it has finished.
        unsafe { CloseHandle(port as HANDLE) };
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A device event for a handle registration (`DBT_DEVTYP_HANDLE`), from the main window's
/// subclass (`crate::devices`): `hdevnotify` names the registration. Talks to the running
/// watcher only; true when the event was one the watcher handles and the watcher runs. For a
/// query remove it waits (at most 1 s) until the watch has closed its handle.
pub fn device_event(event: u32, hdevnotify: isize) -> bool {
    let Some((tx, port)) = DEVICE.lock().ok().and_then(|d| d.clone()) else { return false };
    let post = |cmd: Cmd| {
        if tx.send(cmd).is_ok() {
            // SAFETY: posts a packet with no OVERLAPPED to the watcher's port.
            unsafe { PostQueuedCompletionStatus(port as HANDLE, 0, KEY_CMD, std::ptr::null()) };
        }
    };
    match event {
        DBT_DEVICEQUERYREMOVE => {
            let (reply_tx, reply_rx) = bounded(1);
            post(Cmd::Suspend(hdevnotify, reply_tx));
            match reply_rx.recv_timeout(Duration::from_secs(1)) {
                Ok(true) => eprintln!("[devices] query remove for a watched handle: suspended"),
                Ok(false) => {}
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    eprintln!("[devices] query remove for a watched handle: no reply within 1 s")
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    eprintln!("[devices] query remove for a watched handle: the watcher has stopped")
                }
            }
        }
        DBT_DEVICEQUERYREMOVEFAILED => post(Cmd::Resume(hdevnotify)),
        DBT_DEVICEREMOVECOMPLETE => post(Cmd::Removed(hdevnotify)),
        _ => return false,
    }
    true
}

/// The coalescing window of a watch: `COALESCE` after a quiet spell, doubled (up to
/// `COALESCE_MAX`) for each change that arrives within `QUIET` of the last delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    pub window: Duration,
    pub last_delivery: Option<Instant>,
}

impl Default for Backoff {
    fn default() -> Self {
        Backoff { window: COALESCE, last_delivery: None }
    }
}

impl Backoff {
    /// The window for a change that arrives at `now` and starts one.
    pub fn next_window(&mut self, now: Instant) -> Duration {
        self.window = match self.last_delivery {
            Some(last) if now.saturating_duration_since(last) <= QUIET => (self.window * 2).min(COALESCE_MAX),
            _ => COALESCE,
        };
        self.window
    }

    /// A window was delivered at `now`.
    pub fn delivered(&mut self, now: Instant) {
        self.last_delivery = Some(now);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// The watched folder.
    Main,
    /// The watched folder's parent, for the folder's own rename or delete.
    Parent,
}

/// One watch. `ov` must stay the first field: a completion's `OVERLAPPED` pointer is the watch.
#[repr(C)]
struct Watch {
    ov: OVERLAPPED,
    /// Null while suspended for a drive removal.
    handle: HANDLE,
    role: Role,
    pane: usize,
    tab_id: u64,
    /// The watched folder (for a parent watch: the parent folder).
    path: PathBuf,
    buf: Vec<u32>,
    /// A read is issued and its completion has not been dequeued yet.
    pending: bool,
    /// Replaced or removed: free it when its read completes and no helper holds it.
    cancelled: bool,
    /// A network watch's helper thread is cancelling and closing it and has not reported yet.
    helper: bool,
    /// On a network share: opened and closed on helper threads.
    remote: bool,
    /// A change is waiting to be delivered at this time.
    due: Option<Instant>,
    backoff: Backoff,
    /// Watching every folder below too; the records are parsed into the three fields after it,
    /// gathered until the change is delivered.
    subtree: bool,
    top: bool,
    changes: Vec<(String, String)>,
    overflow: bool,
    /// A plain watch's records of the current window, and how many there were (a subtree watch's:
    /// those in the folder itself). `raw_over`: a subtree watch's went past `MAX_RECORDS`.
    raw: Vec<(u32, String)>,
    records: usize,
    raw_over: bool,
    /// A main watch's parent watch (null if none) and whether one was asked for.
    parent: *mut Watch,
    wants_parent: bool,
    /// A parent watch's last read ended with the folder's old name and no new one.
    cut_old: bool,
    /// On a removable drive; its device notification registration (0 if none).
    removable: bool,
    notify: isize,
    /// The handle is closed for a drive removal; `resume`: reopen once the cancelled read is in.
    suspended: bool,
    resume: bool,
}

impl Watch {
    fn new(role: Role, handle: HANDLE, pane: usize, tab_id: u64, path: PathBuf, subtree: bool) -> Watch {
        let bytes = if role == Role::Parent { PARENT_BUF_BYTES } else { BUF_BYTES };
        Watch {
            ov: OVERLAPPED::default(),
            handle,
            role,
            pane,
            tab_id,
            remote: is_remote(&path),
            removable: role == Role::Main && removable(&path),
            path,
            buf: vec![0u32; bytes / 4],
            pending: false,
            cancelled: false,
            helper: false,
            due: None,
            backoff: Backoff::default(),
            subtree,
            top: false,
            changes: Vec::new(),
            overflow: false,
            raw: Vec::new(),
            records: 0,
            raw_over: false,
            parent: std::ptr::null_mut(),
            wants_parent: false,
            cut_old: false,
            notify: 0,
            suspended: false,
            resume: false,
        }
    }
}

/// The result of a network folder's open, from its helper thread.
struct Opened {
    req: u64,
    result: Result<usize, String>,
}

/// A network folder's open in flight: what to do with the handle when it arrives.
struct PendingOpen {
    tab_id: u64,
    pane: usize,
    role: Role,
    path: PathBuf,
    subtree: bool,
    parent: bool,
    deadline: Instant,
}

/// The drive or share a path is on (lowercased), for the stall cool-down.
fn root_of(path: &Path) -> std::ffi::OsString {
    path.components().next().map(|c| c.as_os_str().to_ascii_lowercase()).unwrap_or_default()
}

struct Dirwatch<W: Fn()> {
    port: HANDLE,
    /// The live (main) watch of each tab.
    slots: HashMap<u64, *mut Watch>,
    /// Cancelled watches whose read or helper has not completed yet.
    dying: Vec<*mut Watch>,
    /// The pane and path each tab last failed to watch, so the refusal is logged once.
    refused: HashMap<u64, (usize, PathBuf)>,
    /// The main window (0 until the UI sends it).
    hwnd: isize,
    /// Network folders being opened on helper threads, by request number.
    opening: HashMap<u64, PendingOpen>,
    next_req: u64,
    /// Network roots whose open stalled, and when.
    stalled: HashMap<std::ffi::OsString, Instant>,
    /// Volumes that refused a device notification (`volume_of`), not asked again (P12-M6).
    no_notify: HashSet<String>,
    cmds: Receiver<Cmd>,
    events: Sender<Event>,
    wake: W,
}

fn last_error() -> String {
    // SAFETY: reads this thread's last-error value.
    std::io::Error::from_raw_os_error(unsafe { GetLastError() } as i32).to_string()
}

/// `GetDriveTypeW` of a drive-letter path's root; None for any other path.
/// A watch on this drive registers for its device events, so "Safely remove" can close it: a
/// removable drive, and since P11-M3 a fixed one whose disk says it is hot-pluggable or has
/// removable media (a USB hard disk reports `DRIVE_FIXED`). Logged once per drive.
fn removable(path: &Path) -> bool {
    match drive_type(path) {
        Some(crate::fs::enumerate::DRIVE_REMOVABLE) => true,
        Some(DRIVE_FIXED) => {
            let Some(Component::Prefix(p)) = path.components().next() else { return false };
            let (Prefix::Disk(letter) | Prefix::VerbatimDisk(letter)) = p.kind() else { return false };
            let hot = hotplug(letter as char);
            thread_local!(static LOGGED: std::cell::RefCell<std::collections::HashSet<char>> = Default::default());
            if LOGGED.with(|l| l.borrow_mut().insert(letter as char)) {
                eprintln!("[devices] {}: fixed, {}", letter as char, if hot { "removable media: registered" } else { "not removable" });
            }
            hot
        }
        _ => false,
    }
}

/// `GetDriveTypeW`'s fixed drive (WinBase.h).
const DRIVE_FIXED: u32 = 3;

/// The volume's disk says it is hot-pluggable or has removable media
/// (`IOCTL_STORAGE_GET_HOTPLUG_INFO` on `\\.\X:`, which needs no access rights).
fn hotplug(letter: char) -> bool {
    use windows_sys::Win32::System::IO::DeviceIoControl;
    const IOCTL_STORAGE_GET_HOTPLUG_INFO: u32 = 0x002D_0C14;
    let name: Vec<u16> = format!(r"\\.\{letter}:").encode_utf16().chain([0]).collect();
    // SAFETY: NUL-terminated name; no access asked, so no admin right is needed.
    let h = unsafe { CreateFileW(name.as_ptr(), 0, FILE_SHARE_READ | FILE_SHARE_WRITE, std::ptr::null(), OPEN_EXISTING, 0, std::ptr::null_mut()) };
    if h == INVALID_HANDLE_VALUE {
        return false;
    }
    // STORAGE_HOTPLUG_INFO: Size, then MediaRemovable, MediaHotplug, DeviceHotplug, WriteCacheEnableOverride.
    let mut info = [0u8; 8];
    let mut got = 0u32;
    // SAFETY: a live handle; `info` is the out buffer of its size.
    let ok = unsafe { DeviceIoControl(h, IOCTL_STORAGE_GET_HOTPLUG_INFO, std::ptr::null(), 0, info.as_mut_ptr().cast(), info.len() as u32, &mut got, std::ptr::null_mut()) } != 0;
    // SAFETY: the handle opened above, closed once.
    unsafe { CloseHandle(h) };
    ok && got >= 7 && (info[4] != 0 || info[6] != 0)
}

fn drive_type(path: &Path) -> Option<u32> {
    let Some(Component::Prefix(p)) = path.components().next() else { return None };
    match p.kind() {
        Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
            let root: Vec<u16> = [letter as u16, b':' as u16, b'\\' as u16, 0].to_vec();
            // SAFETY: `root` is a NUL-terminated wide string for the duration of the call.
            Some(unsafe { GetDriveTypeW(root.as_ptr()) })
        }
        _ => None,
    }
}

/// Where the folder open as `handle` is now (`GetFinalPathNameByHandleW`, without the `\\?\`
/// prefix it gives: `\\?\C:\x` is `C:\x`, `\\?\UNC\server\share` is `\\server\share`).
fn final_path(handle: HANDLE) -> Option<PathBuf> {
    let mut buf = vec![0u16; 1024];
    // SAFETY: a live handle; `buf` holds its length in u16s.
    let n = unsafe { GetFinalPathNameByHandleW(handle, buf.as_mut_ptr(), buf.len() as u32, 0) } as usize;
    if n == 0 || n >= buf.len() {
        return None;
    }
    let s = String::from_utf16_lossy(&buf[..n]);
    let s = match s.strip_prefix(r"\\?\UNC\") {
        Some(rest) => format!(r"\\{rest}"),
        None => s.strip_prefix(r"\\?\").unwrap_or(&s).to_string(),
    };
    Some(PathBuf::from(s))
}

/// A network folder: a UNC path or a mapped network drive.
pub fn is_remote(path: &Path) -> bool {
    match path.components().next() {
        Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::UNC(..) | Prefix::VerbatimUNC(..)) => true,
        _ => drive_type(path) == Some(crate::fs::enumerate::DRIVE_REMOTE),
    }
}

/// Why `path` gets no watch, before any handle is opened: the drive list; a folder inside an
/// archive; a disc (it cannot change).
fn unwatchable(path: &Path) -> Option<&'static str> {
    if crate::fs::enumerate::is_this_pc(&path.to_string_lossy()) {
        return Some("the drive list is not a folder");
    }
    if crate::fs::archive::inside(path) {
        return Some("inside an archive");
    }
    let Some(Component::Prefix(_)) = path.components().next() else { return Some("not an absolute path") };
    if drive_type(path) == Some(crate::fs::enumerate::DRIVE_CDROM) {
        return Some("a disc cannot change");
    }
    None
}

/// `CreateFileW` of a folder for watching, on the calling thread.
fn open_now(path: &Path) -> Result<usize, String> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: `wide` is NUL-terminated; no security attributes or template.
    let h = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_LIST_DIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
            std::ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE { Err(last_error()) } else { Ok(h as usize) }
}

/// Open a local `path` for watching on this thread: the directory handle, or why not. (A network
/// folder goes through `Dirwatch::request_open` instead.)
fn open_dir(path: &Path) -> Result<HANDLE, String> {
    if let Some(why) = unwatchable(path) {
        return Err(why.into());
    }
    open_now(path).map(|h| h as HANDLE)
}

/// Register for the device events of the drive under `handle`: the HDEVNOTIFY, or 0. A volume that
/// refused once is noted in `refused` and not asked again (P12-M6: Google Drive's `G:` refuses
/// every watch with os error 1066).
fn register(hwnd: isize, handle: HANDLE, w: &Watch, refused: &mut HashSet<String>) -> isize {
    let volume = volume_of(&w.path);
    if refused.contains(&volume) {
        return 0;
    }
    let filter = DEV_BROADCAST_HANDLE {
        dbch_size: std::mem::size_of::<DEV_BROADCAST_HANDLE>() as u32,
        dbch_devicetype: DBT_DEVTYP_HANDLE,
        dbch_handle: handle,
        ..Default::default()
    };
    // SAFETY: `filter` is a valid DEV_BROADCAST_HANDLE for the call; `hwnd` is the main window.
    let n = unsafe {
        RegisterDeviceNotificationW(hwnd as HANDLE, &filter as *const _ as *const c_void, DEVICE_NOTIFY_WINDOW_HANDLE)
    } as isize;
    if n == 0 {
        eprintln!("[watch] pane={} tab={} no device notification for {}: {}: not asked again on {volume}", w.pane, w.tab_id, w.path.display(), last_error());
        refused.insert(volume);
    }
    n
}

/// The volume under `path` by its GUID path (`\\?\Volume{...}\`), so a disk given the same letter
/// later is asked again; its mount point (`dnd::volume_key`) when it has none.
fn volume_of(path: &Path) -> String {
    let mount = crate::dnd::volume_key(path);
    let mut buf = [0u16; 64];
    let root: Vec<u16> = format!("{mount}\\").encode_utf16().chain([0]).collect();
    // SAFETY: a NUL-terminated mount point; `buf` is writable for its length (a GUID path is 49
    // characters and its NUL).
    let ok = unsafe { GetVolumeNameForVolumeMountPointW(root.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) } != 0;
    let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
    if ok && n > 0 { String::from_utf16_lossy(&buf[..n]) } else { mount }
}

fn unregister(notify: isize) {
    if notify != 0 {
        // SAFETY: a registration of ours, unregistered once.
        unsafe { UnregisterDeviceNotification(notify as *mut c_void) };
    }
}

/// Issue the watch's next read.
///
/// # Safety
/// `w` is a live, boxed watch whose handle is associated with the port and has no read pending.
unsafe fn arm(w: &mut Watch) -> Result<(), String> {
    w.ov = OVERLAPPED::default();
    let (filter, subtree) = match w.role {
        Role::Main => (FILTER, w.subtree),
        Role::Parent => (FILE_NOTIFY_CHANGE_DIR_NAME, false),
    };
    // SAFETY: the buffer and OVERLAPPED live in the Box until this read's completion is dequeued.
    let ok = unsafe {
        ReadDirectoryChangesW(
            w.handle,
            w.buf.as_mut_ptr() as *mut c_void,
            (w.buf.len() * 4) as u32,
            subtree as i32,
            filter,
            std::ptr::null_mut(),
            &mut w.ov,
            None,
        )
    };
    if ok == 0 {
        return Err(last_error());
    }
    w.pending = true;
    Ok(())
}

impl<W: Fn()> Dirwatch<W> {
    fn run(mut self) {
        // No "insert a disk" dialog from a removable drive that went away.
        // SAFETY: plain flag call on this thread.
        unsafe { SetThreadErrorMode(SEM_FAILCRITICALERRORS, std::ptr::null_mut()) };
        loop {
            let timeout = self.next_due().map_or(INFINITE, |due| {
                let left = due.saturating_duration_since(Instant::now());
                left.as_micros().div_ceil(1000).min(u32::MAX as u128 - 1) as u32
            });
            let (mut bytes, mut key, mut ov) = (0u32, 0usize, std::ptr::null_mut::<OVERLAPPED>());
            // SAFETY: valid out pointers; the port lives until the Watcher is dropped after us.
            let ok = unsafe { GetQueuedCompletionStatus(self.port, &mut bytes, &mut key, &mut ov, timeout) } != 0;
            // SAFETY: reads this thread's last-error value, set by the failed call just above.
            let err = if ok { 0 } else { unsafe { GetLastError() } };
            if key == KEY_OPENED && !ov.is_null() {
                self.opened(ov as *mut Opened);
            } else if !ov.is_null() {
                self.completed(ov as *mut Watch, key, ok, err, bytes);
            } else if ok && key == KEY_CMD {
                if !self.commands() {
                    return self.shutdown();
                }
            } else if !ok && err != WAIT_TIMEOUT {
                eprintln!("[watch] completion port failed: {}; the watcher stops", std::io::Error::from_raw_os_error(err as i32));
                return self.shutdown();
            }
            self.expire_opens();
            self.deliver();
        }
    }

    fn next_due(&self) -> Option<Instant> {
        // SAFETY: every slot is a live watch owned by this thread.
        let due = self.slots.values().filter_map(|&p| unsafe { (*p).due }).min();
        let open = self.opening.values().map(|o| o.deadline).min();
        due.into_iter().chain(open).min()
    }

    fn send(&self, ev: Event) {
        if self.events.send(ev).is_ok() {
            (self.wake)();
        }
    }

    /// The live watch registered as `notify`.
    fn by_notify(&self, notify: isize) -> Option<*mut Watch> {
        // SAFETY: every slot is a live watch owned by this thread.
        self.slots.values().copied().find(|&p| notify != 0 && unsafe { (*p).notify } == notify)
    }

    /// Handle every queued command; false on Quit.
    fn commands(&mut self) -> bool {
        while let Ok(cmd) = self.cmds.try_recv() {
            match cmd {
                Cmd::Watch { pane, tab_id, path, subtree, parent } => self.start(pane, tab_id, path, subtree, parent),
                Cmd::Unwatch(tab_id) => {
                    self.refused.remove(&tab_id);
                    self.opening.retain(|_, o| o.tab_id != tab_id);
                    if let Some(p) = self.slots.remove(&tab_id) {
                        self.retire(p);
                    }
                }
                Cmd::UnwatchPane(pane) => {
                    self.refused.retain(|_, (p, _)| *p != pane);
                    self.opening.retain(|_, o| o.pane != pane);
                    // SAFETY: every slot is a live watch owned by this thread.
                    let tabs: Vec<u64> = self.slots.iter().filter(|&(_, &p)| unsafe { (*p).pane } == pane).map(|(&t, _)| t).collect();
                    for t in tabs {
                        if let Some(p) = self.slots.remove(&t) {
                            self.retire(p);
                        }
                    }
                }
                Cmd::SetHwnd(hwnd) => {
                    self.hwnd = hwnd;
                    for &p in self.slots.values() {
                        // SAFETY: a live watch owned by this thread.
                        let w = unsafe { &mut *p };
                        if w.removable && w.notify == 0 && !w.suspended {
                            w.notify = register(hwnd, w.handle, w, &mut self.no_notify);
                        }
                    }
                }
                Cmd::Suspend(notify, reply) => {
                    let found = self.by_notify(notify);
                    if let Some(p) = found {
                        self.suspend(p);
                    }
                    let _ = reply.send(found.is_some());
                }
                Cmd::Resume(notify) => {
                    if let Some(p) = self.by_notify(notify) {
                        // SAFETY: a live watch owned by this thread.
                        let w = unsafe { &mut *p };
                        if w.suspended {
                            // The old registration ends here; the reopened handle gets a new one.
                            unregister(std::mem::take(&mut w.notify));
                            if w.pending {
                                w.resume = true;
                            } else {
                                self.resume(p);
                            }
                        }
                    }
                }
                Cmd::Removed(notify) => {
                    if let Some(p) = self.by_notify(notify) {
                        self.lose(p, "the drive was removed".into());
                    }
                }
                #[cfg(test)]
                Cmd::Sync(tx) => {
                    let _ = tx.send(());
                }
                Cmd::Quit => return false,
            }
        }
        true
    }

    /// No watch for `path`: say why once per tab and path (the UI asks again when the listing
    /// completes, and on every refresh), and tell the UI every time.
    fn refuse(&mut self, pane: usize, tab_id: u64, path: PathBuf, why: String) {
        if self.refused.get(&tab_id).map(|(_, p)| p) != Some(&path) {
            eprintln!("[watch] pane={pane} tab={tab_id} no watch for {}: {why}", path.display());
            self.refused.insert(tab_id, (pane, path.clone()));
        }
        self.send(Event::Refused { pane, tab_id, path, why });
    }

    fn start(&mut self, pane: usize, tab_id: u64, path: PathBuf, subtree: bool, parent: bool) {
        // A network open in flight for this tab: the same request waits for it; another one
        // supersedes it (its late handle is closed when it arrives).
        if let Some((&req, o)) = self.opening.iter_mut().find(|(_, o)| o.tab_id == tab_id && o.role == Role::Main) {
            if o.path == path && o.subtree == subtree {
                (o.pane, o.parent) = (pane, parent);
                return;
            }
            self.opening.remove(&req);
        }
        if let Some(&p) = self.slots.get(&tab_id) {
            // SAFETY: a live watch owned by this thread.
            let w = unsafe { &mut *p };
            if w.path == path && w.subtree == subtree {
                w.pane = pane;
                if parent != w.wants_parent {
                    // A suspended watch only remembers it: `resume` opens the parent if wanted.
                    w.wants_parent = parent;
                    if w.suspended {
                    } else if parent {
                        self.open_parent(p);
                    } else if !w.parent.is_null() {
                        let pp = std::mem::replace(&mut w.parent, std::ptr::null_mut());
                        self.retire(pp);
                    }
                }
                return;
            }
            if w.path == path && !subtree {
                eprintln!("[watch] pane={pane} subtree off {}", path.display());
            }
            if w.path == path && w.due.is_some() {
                // The same folder, watched another way (the subtree flag flipped): the window
                // gathered so far is delivered now, not lost with the old watch.
                if self.events.send(window_event(w, Instant::now())).is_ok() {
                    (self.wake)();
                }
            }
            self.slots.remove(&tab_id);
            self.opening.retain(|_, o| o.tab_id != tab_id);
            self.retire(p);
        }
        let opening = self.opening.values().filter(|o| o.role == Role::Main).count();
        if self.slots.len() + opening >= MAX_WATCHES {
            return self.refuse(pane, tab_id, path, format!("watch cap reached ({MAX_WATCHES})"));
        }
        if let Some(why) = unwatchable(&path) {
            return self.refuse(pane, tab_id, path, why.into());
        }
        if is_remote(&path) {
            if let Err(why) = self.request_open(tab_id, pane, Role::Main, path.clone(), subtree, parent) {
                self.refuse(pane, tab_id, path, why);
            }
            return;
        }
        match open_dir(&path) {
            Ok(h) => self.install(pane, tab_id, path, subtree, parent, h),
            Err(why) => self.refuse(pane, tab_id, path, why),
        }
    }

    /// Open a network folder on a helper thread, which posts the result (`KEY_OPENED`); refused
    /// at once while its root is in the stall cool-down.
    fn request_open(&mut self, tab_id: u64, pane: usize, role: Role, path: PathBuf, subtree: bool, parent: bool) -> Result<(), String> {
        if self.stalled.get(&root_of(&path)).is_some_and(|t| t.elapsed() < STALL_COOLDOWN) {
            return Err("stalled: no watch".into());
        }
        let req = self.next_req;
        self.next_req += 1;
        let (port, p) = (self.port as usize, path.clone());
        HELPERS.fetch_add(1, Ordering::SeqCst);
        let spawned = std::thread::Builder::new().name("dirwatch-open".into()).spawn(move || {
            let packet = Box::into_raw(Box::new(Opened { req, result: open_now(&p) }));
            // SAFETY: the watcher frees the Box when it dequeues the packet; the port stays open
            // while any helper runs (`HELPERS`).
            unsafe { PostQueuedCompletionStatus(port as HANDLE, 0, KEY_OPENED, packet as *const OVERLAPPED) };
            HELPERS.fetch_sub(1, Ordering::SeqCst);
        });
        if let Err(e) = spawned {
            HELPERS.fetch_sub(1, Ordering::SeqCst);
            return Err(format!("no open thread: {e}"));
        }
        self.opening.insert(req, PendingOpen { tab_id, pane, role, path, subtree, parent, deadline: Instant::now() + OPEN_TIMEOUT });
        Ok(())
    }

    /// A network folder's open finished: install the watch (or parent watch) it was for, or close
    /// the handle when that request was superseded or gave up.
    fn opened(&mut self, packet: *mut Opened) {
        // SAFETY: the helper's Box, posted once and dequeued once.
        let o = unsafe { Box::from_raw(packet) };
        let close = |h: usize| {
            // SAFETY: the handle the helper opened; nobody else has it.
            unsafe { CloseHandle(h as HANDLE) };
        };
        let Some(po) = self.opening.remove(&o.req) else {
            if let Ok(h) = o.result {
                close(h);
            }
            return;
        };
        match (po.role, o.result) {
            (Role::Main, Ok(h)) => self.install(po.pane, po.tab_id, po.path, po.subtree, po.parent, h as HANDLE),
            (Role::Main, Err(why)) => self.refuse(po.pane, po.tab_id, po.path, why),
            (Role::Parent, result) => {
                // SAFETY: slots hold live watches.
                let owner = self.slots.get(&po.tab_id).copied().filter(|&m| unsafe {
                    (*m).parent.is_null() && (*m).wants_parent && !(*m).suspended && (*m).path.parent() == Some(po.path.as_path())
                });
                match (owner, result) {
                    (Some(m), Ok(h)) => self.attach_parent(m, po.path, h as HANDLE),
                    (None, Ok(h)) => close(h),
                    (Some(m), Err(why)) => {
                        // SAFETY: a live main watch.
                        let w = unsafe { &*m };
                        eprintln!("[watch] pane={} tab={} no parent watch for {}: {why}", w.pane, w.tab_id, w.path.display());
                    }
                    (None, Err(_)) => {}
                }
            }
        }
    }

    /// Network opens that have not finished within `OPEN_TIMEOUT`: refused as stalled, and their
    /// root refused at once for `STALL_COOLDOWN`. The helper's late handle is closed on arrival.
    fn expire_opens(&mut self) {
        let now = Instant::now();
        let expired: Vec<u64> = self.opening.iter().filter(|(_, o)| o.deadline <= now).map(|(&r, _)| r).collect();
        for req in expired {
            let Some(po) = self.opening.remove(&req) else { continue };
            self.stalled.insert(root_of(&po.path), now);
            match po.role {
                Role::Main => self.refuse(po.pane, po.tab_id, po.path, "stalled: no watch".into()),
                Role::Parent => {
                    eprintln!("[watch] pane={} tab={} no parent watch for {}: stalled: no watch", po.pane, po.tab_id, po.path.display())
                }
            }
        }
    }

    /// Start the main watch of `tab_id` on the open `handle`.
    fn install(&mut self, pane: usize, tab_id: u64, path: PathBuf, subtree: bool, parent: bool, handle: HANDLE) {
        let p = Box::into_raw(Box::new(Watch::new(Role::Main, handle, pane, tab_id, path, subtree)));
        // SAFETY: `p` is the Box just leaked; the handle is open; the key is the watch's address.
        let res = unsafe {
            if CreateIoCompletionPort(handle, self.port, p as usize, 0).is_null() { Err(last_error()) } else { arm(&mut *p) }
        };
        // SAFETY: as above.
        let w = unsafe { &mut *p };
        if let Err(why) = res {
            // SAFETY: no read is pending (arm failed or never ran), so nothing refers to it.
            let w = unsafe {
                CloseHandle(handle);
                Box::from_raw(p)
            };
            return self.refuse(pane, tab_id, w.path, why);
        }
        eprintln!("[watch] pane={pane} tab={tab_id} watching {}", w.path.display());
        if subtree {
            eprintln!("[watch] pane={pane} subtree on {}", w.path.display());
        }
        if w.removable && self.hwnd != 0 {
            w.notify = register(self.hwnd, handle, w, &mut self.no_notify);
        }
        w.wants_parent = parent;
        self.slots.insert(tab_id, p);
        self.refused.remove(&tab_id);
        if parent {
            self.open_parent(p);
        }
    }

    /// Open the parent watch of the main watch at `p`, or log why there is none.
    fn open_parent(&mut self, p: *mut Watch) {
        // SAFETY: a live main watch owned by this thread.
        let w = unsafe { &mut *p };
        let no_parent = |why: &str| eprintln!("[watch] pane={} tab={} no parent watch for {}: {why}", w.pane, w.tab_id, w.path.display());
        let Some(dir) = w.path.parent().filter(|d| w.path.file_name().is_some() && !d.as_os_str().is_empty()) else {
            return no_parent("a drive root");
        };
        if is_remote(dir) {
            let (tab_id, pane, dir) = (w.tab_id, w.pane, dir.to_path_buf());
            if let Err(why) = self.request_open(tab_id, pane, Role::Parent, dir, false, false) {
                // SAFETY: as above.
                let w = unsafe { &*p };
                eprintln!("[watch] pane={} tab={} no parent watch for {}: {why}", w.pane, w.tab_id, w.path.display());
            }
            return;
        }
        match open_dir(dir) {
            Ok(h) => self.attach_parent(p, dir.to_path_buf(), h),
            Err(why) => no_parent(&why),
        }
    }

    /// Give the main watch at `p` its parent watch on `dir`, opened as `handle`.
    fn attach_parent(&mut self, p: *mut Watch, dir: PathBuf, handle: HANDLE) {
        // SAFETY: a live main watch owned by this thread.
        let w = unsafe { &mut *p };
        let no_parent = |why: &str| eprintln!("[watch] pane={} tab={} no parent watch for {}: {why}", w.pane, w.tab_id, w.path.display());
        let pp = Box::into_raw(Box::new(Watch::new(Role::Parent, handle, w.pane, w.tab_id, dir.clone(), false)));
        // SAFETY: `pp` is the Box just leaked; the handle is open; the key is the watch's address.
        let res = unsafe {
            if CreateIoCompletionPort(handle, self.port, pp as usize, 0).is_null() { Err(last_error()) } else { arm(&mut *pp) }
        };
        if let Err(why) = res {
            // SAFETY: no read is pending, so nothing refers to it.
            unsafe {
                CloseHandle(handle);
                drop(Box::from_raw(pp));
            }
            return no_parent(&why);
        }
        eprintln!("[watch] pane={} tab={} parent watch on {}", w.pane, w.tab_id, dir.display());
        w.parent = pp;
    }

    /// A watch leaves its slot (or a main watch drops its parent): cancel its read and close it;
    /// free it now if nothing refers to it, else when its cancelled read completes and (for a
    /// network folder) its helper thread has reported.
    fn retire(&mut self, p: *mut Watch) {
        // SAFETY: a live watch owned by this thread, just taken out of its slot.
        let w = unsafe { &mut *p };
        w.cancelled = true;
        w.due = None;
        if !w.parent.is_null() {
            let pp = std::mem::replace(&mut w.parent, std::ptr::null_mut());
            self.retire(pp);
        }
        unregister(std::mem::take(&mut w.notify));
        let handle = std::mem::replace(&mut w.handle, std::ptr::null_mut());
        if !handle.is_null() && w.remote {
            // A dead share can hang CancelIoEx and CloseHandle: a helper does both, then posts
            // KEY_HELPER with the watch, which stays alive until then (the cancel names its
            // OVERLAPPED).
            w.helper = true;
            let (h, ov, port, pending) = (handle as usize, p as usize, self.port as usize, w.pending);
            HELPERS.fetch_add(1, Ordering::SeqCst);
            let work = move || {
                // SAFETY: the watch is kept alive until the KEY_HELPER packet below is dequeued;
                // the handle is ours and closed once.
                unsafe {
                    if pending {
                        CancelIoEx(h as HANDLE, ov as *const OVERLAPPED);
                    }
                    CloseHandle(h as HANDLE);
                    PostQueuedCompletionStatus(port as HANDLE, 0, KEY_HELPER, ov as *const OVERLAPPED);
                }
                HELPERS.fetch_sub(1, Ordering::SeqCst);
            };
            // The closure captures only integers, so it is Copy: run it here if no thread starts.
            if std::thread::Builder::new().name("dirwatch-close".into()).spawn(work).is_err() {
                work();
            }
            self.dying.push(p);
            return;
        }
        if !handle.is_null() {
            // SAFETY: the handle is open; cancelling and closing are both fine with a read pending
            // (its completion is still queued to the port, with ERROR_OPERATION_ABORTED).
            unsafe {
                if w.pending {
                    CancelIoEx(handle, &w.ov);
                }
                CloseHandle(handle);
            }
        }
        if w.pending {
            self.dying.push(p);
        } else {
            // SAFETY: no read refers to it.
            unsafe { drop(Box::from_raw(p)) };
        }
    }

    /// Free a cancelled watch once neither its read nor its helper refers to it.
    fn free_if_done(&mut self, p: *mut Watch) {
        // SAFETY: a cancelled watch owned by this thread.
        let w = unsafe { &*p };
        if !w.pending && !w.helper {
            self.dying.retain(|&d| d != p);
            // SAFETY: nothing refers to it any more.
            unsafe { drop(Box::from_raw(p)) };
        }
    }

    /// A packet for the watch at `p`: a read completed (or failed) with `bytes` of records in its
    /// buffer, or (key `KEY_HELPER`) a helper thread has closed it.
    fn completed(&mut self, p: *mut Watch, key: usize, ok: bool, err: u32, bytes: u32) {
        // SAFETY: only reads of live or dying watches are ever issued, and neither is freed while
        // its read is pending or its helper runs, so `p` is a live Box.
        let w = unsafe { &mut *p };
        if key == KEY_HELPER {
            w.helper = false;
            return self.free_if_done(p);
        }
        w.pending = false;
        if w.cancelled {
            return self.free_if_done(p);
        }
        if w.suspended {
            if std::mem::take(&mut w.resume) {
                self.resume(p);
            }
            return;
        }
        if w.role == Role::Parent {
            return self.parent_completed(p, ok, err, bytes);
        }
        if !ok && err != ERROR_NOTIFY_ENUM_DIR {
            return self.lose(p, std::io::Error::from_raw_os_error(err as i32).to_string());
        }
        // Success (records, or zero bytes for an overflow) or an overflow error: changed.
        if w.due.is_none() {
            let now = Instant::now();
            let before = w.backoff.window;
            let window = w.backoff.next_window(now);
            if window != before {
                eprintln!("[watch] pane={} tab={} back-off: window {} ms", w.pane, w.tab_id, window.as_millis());
            }
            w.due = Some(now + window);
        }
        if !ok || bytes == 0 {
            w.overflow = true;
            w.raw.clear();
        } else {
            // SAFETY: the kernel wrote `bytes` (at most the buffer's size) into the buffer, whose
            // read has completed; it is not written again until `arm` below.
            let raw = unsafe { std::slice::from_raw_parts(w.buf.as_ptr() as *const u8, (bytes as usize).min(w.buf.len() * 4)) };
            let records = parse_records(raw);
            if w.subtree {
                for (action, name) in records {
                    let child = crate::fs::sizes::child_of(&name);
                    let top = child.len() == name.len();
                    w.top |= top;
                    if top {
                        w.records += 1;
                        if !w.raw_over {
                            w.raw.push((action, name.clone()));
                            if w.raw.len() > MAX_RECORDS {
                                eprintln!("[watch] pane={} tab={} {} records in the folder itself: refresh", w.pane, w.tab_id, w.raw.len());
                                w.raw_over = true;
                                w.raw = Vec::new();
                            }
                        }
                    }
                    // A subfolder's own entry modified (its times, which NTFS also updates lazily
                    // after a walk has opened it) is not a change of its contents.
                    if top && action == FILE_ACTION_MODIFIED {
                        continue;
                    }
                    if !w.changes.iter().any(|(c, _)| c.eq_ignore_ascii_case(child)) {
                        w.changes.push((child.to_string(), format!("{} {name}", action_name(action))));
                    }
                }
            } else {
                w.records += records.len();
                if !w.overflow {
                    w.raw.extend(records.into_iter().filter(|(_, n)| !n.contains('\\')));
                    if w.raw.len() > MAX_RECORDS {
                        // Folding this many costs more than listing the folder again.
                        eprintln!("[watch] pane={} tab={} {} records in the window: overflow", w.pane, w.tab_id, w.raw.len());
                        w.overflow = true;
                        w.raw = Vec::new();
                    }
                }
            }
        }
        // SAFETY: the read just completed, so none is pending.
        if let Err(why) = unsafe { arm(w) } {
            self.lose(p, why);
        }
    }

    /// A parent watch's read completed: the watched folder renamed (the watch follows it) or
    /// deleted (the watch is lost). Its own overflow is ignored; its failure drops it.
    fn parent_completed(&mut self, pp: *mut Watch, ok: bool, err: u32, bytes: u32) {
        // SAFETY: a live parent watch owned by this thread.
        let pw = unsafe { &mut *pp };
        let owner = self.slots.get(&pw.tab_id).copied();
        // SAFETY: slots hold live watches.
        let Some(m) = owner.filter(|&m| unsafe { (*m).parent } == pp) else {
            return self.retire(pp);
        };
        // SAFETY: a live main watch, distinct from `pp`.
        let mw = unsafe { &mut *m };
        if !ok && err != ERROR_NOTIFY_ENUM_DIR {
            let why = std::io::Error::from_raw_os_error(err as i32);
            eprintln!("[watch] pane={} tab={} no parent watch for {}: {why}", mw.pane, mw.tab_id, mw.path.display());
            mw.parent = std::ptr::null_mut();
            return self.retire(pp);
        }
        if ok && bytes > 0 {
            // SAFETY: as in `completed`.
            let raw = unsafe { std::slice::from_raw_parts(pw.buf.as_ptr() as *const u8, (bytes as usize).min(pw.buf.len() * 4)) };
            let records = parse_records(raw);
            let mut i = 0;
            while i < records.len() {
                let name = mw.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let (action, n) = (&records[i].0, &records[i].1);
                // An old name of our folder with no new name after it: the folder left this
                // parent (moved to another folder on the volume).
                let renamed_to = if i == 0 && std::mem::take(&mut pw.cut_old) {
                    if *action != FILE_ACTION_RENAMED_NEW_NAME {
                        return self.moved_away(m, pp, "the folder was moved away");
                    }
                    Some(n.clone())
                } else if *action == FILE_ACTION_RENAMED_OLD_NAME && n.eq_ignore_ascii_case(&name) {
                    match records.get(i + 1) {
                        Some((FILE_ACTION_RENAMED_NEW_NAME, new)) => {
                            i += 1;
                            Some(new.clone())
                        }
                        Some(_) => return self.moved_away(m, pp, "the folder was moved away"),
                        None => {
                            pw.cut_old = true;
                            None
                        }
                    }
                } else {
                    None
                };
                if let Some(new) = renamed_to {
                    let from = mw.path.clone();
                    let to = pw.path.join(&new);
                    eprintln!("[watch] pane={} tab={} folder renamed: {} -> {}", mw.pane, mw.tab_id, from.display(), to.display());
                    mw.path = to.clone();
                    self.send(Event::Moved { pane: mw.pane, tab_id: mw.tab_id, from, to });
                } else if *action == FILE_ACTION_REMOVED && n.eq_ignore_ascii_case(&name) {
                    return self.moved_away(m, pp, "the folder was deleted");
                }
                i += 1;
            }
        }
        // SAFETY: the read just completed, so none is pending.
        if let Err(why) = unsafe { arm(pw) } {
            eprintln!("[watch] pane={} tab={} no parent watch for {}: {why}", mw.pane, mw.tab_id, mw.path.display());
            mw.parent = std::ptr::null_mut();
            self.retire(pp);
        }
    }

    /// Windows asks to remove the drive of the main watch at `p`: close its handles (its read is
    /// cancelled; the watch stays in its slot, suspended) so the removal can go ahead.
    fn suspend(&mut self, p: *mut Watch) {
        // SAFETY: a live main watch owned by this thread.
        let w = unsafe { &mut *p };
        if w.suspended {
            return;
        }
        w.suspended = true;
        w.due = None;
        if !w.parent.is_null() {
            let pp = std::mem::replace(&mut w.parent, std::ptr::null_mut());
            self.retire(pp);
        }
        let handle = std::mem::replace(&mut w.handle, std::ptr::null_mut());
        // SAFETY: the handle is open (a watch is suspended once); its cancelled read's completion
        // finds the watch suspended and leaves it alone.
        unsafe {
            if w.pending {
                CancelIoEx(handle, &w.ov);
            }
            CloseHandle(handle);
        }
        // The registration stays until the removal completes or fails: those events name it.
        eprintln!("[watch] pane={} tab={} suspended for removal: {}", w.pane, w.tab_id, w.path.display());
    }

    /// The removal failed: reopen the suspended main watch at `p` (no read pending), re-arm and
    /// re-register it, and tell the UI to refresh (changes may have been missed).
    fn resume(&mut self, p: *mut Watch) {
        // SAFETY: a live, suspended main watch owned by this thread.
        let w = unsafe { &mut *p };
        let handle = match open_dir(&w.path) {
            Ok(h) => h,
            Err(why) => return self.lose(p, why),
        };
        w.handle = handle;
        w.suspended = false;
        // SAFETY: the reopened handle is associated with the port under the watch's address.
        let res = unsafe {
            if CreateIoCompletionPort(handle, self.port, p as usize, 0).is_null() { Err(last_error()) } else { arm(w) }
        };
        if let Err(why) = res {
            return self.lose(p, why);
        }
        if self.hwnd != 0 {
            w.notify = register(self.hwnd, handle, w, &mut self.no_notify);
        }
        eprintln!("[watch] pane={} tab={} resumed after a failed removal: {}", w.pane, w.tab_id, w.path.display());
        let (pane, tab_id) = (w.pane, w.tab_id);
        if w.wants_parent {
            self.open_parent(p);
        }
        self.send(Event::Changed { pane, tab_id, changes: Vec::new(), records: 0, overflow: true });
    }

    /// The watched folder at `m` left its parent (watched by `pp`): a move to another folder comes
    /// as a removal record, or an old name with no new one (P11-M3). Its own handle, still open,
    /// names where it is now, so the watch follows it there (`Moved`, a new parent watch on its new
    /// parent); `Lost` with `lost` when the handle names no other folder (it was deleted).
    fn moved_away(&mut self, m: *mut Watch, pp: *mut Watch, lost: &str) {
        // SAFETY: a live main watch owned by this thread.
        let mw = unsafe { &mut *m };
        let Some(to) = final_path(mw.handle).filter(|to| !to.as_os_str().eq_ignore_ascii_case(mw.path.as_os_str()) && to.is_dir()) else {
            return self.lose(m, lost.into());
        };
        let from = std::mem::replace(&mut mw.path, to.clone());
        eprintln!("[watch] pane={} tab={} folder moved: {} -> {}", mw.pane, mw.tab_id, from.display(), to.display());
        mw.parent = std::ptr::null_mut();
        self.retire(pp);
        self.open_parent(m);
        // SAFETY: as above; `open_parent` keeps the watch.
        let mw = unsafe { &*m };
        self.send(Event::Moved { pane: mw.pane, tab_id: mw.tab_id, from, to });
    }

    /// The live main watch at `p` failed: take it out of its slot, retire it (and its parent),
    /// and say so.
    fn lose(&mut self, p: *mut Watch, error: String) {
        // SAFETY: a live main watch owned by this thread.
        let w = unsafe { &*p };
        let (pane, tab_id) = (w.pane, w.tab_id);
        if self.slots.get(&tab_id) == Some(&p) {
            self.slots.remove(&tab_id);
        }
        eprintln!("[watch] pane={pane} tab={tab_id} lost {}: {error}", w.path.display());
        self.retire(p);
        self.send(Event::Lost { pane, tab_id, error });
    }

    /// Send every change whose time has come.
    fn deliver(&mut self) {
        let now = Instant::now();
        let mut sent = false;
        for &p in self.slots.values() {
            // SAFETY: a live watch owned by this thread.
            let w = unsafe { &mut *p };
            if w.due.is_some_and(|d| d <= now) {
                sent |= self.events.send(window_event(w, now)).is_ok();
            }
        }
        if sent {
            (self.wake)();
        }
    }

    /// Quit: cancel every watch and wait (bounded) for the cancelled reads and helpers, so no
    /// buffer is freed while the kernel may still write into it.
    fn shutdown(mut self) {
        self.opening.clear();
        let live: Vec<_> = self.slots.drain().map(|(_, p)| p).collect();
        for p in live {
            self.retire(p);
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while !self.dying.is_empty() {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                eprintln!("[watch] {} cancelled read(s) never completed; their buffers are leaked", self.dying.len());
                return;
            }
            let (mut bytes, mut key, mut ov) = (0u32, 0usize, std::ptr::null_mut::<OVERLAPPED>());
            // SAFETY: as in `run`.
            let ok = unsafe { GetQueuedCompletionStatus(self.port, &mut bytes, &mut key, &mut ov, left.as_millis() as u32) } != 0;
            if key == KEY_OPENED && !ov.is_null() {
                self.opened(ov as *mut Opened);
            } else if !ov.is_null() {
                self.completed(ov as *mut Watch, key, ok, 0, 0);
            }
        }
    }
}

/// A watch's gathered window as its event, now (`deliver`, or `start` replacing the watch): the
/// subtree watch's `Subtree`, a plain watch's net changes as `Changed` (with its line). Clears the
/// window and tells the back-off.
fn window_event(w: &mut Watch, now: Instant) -> Event {
    w.due = None;
    w.backoff.delivered(now);
    if w.subtree {
        let (top, changes, overflow) = (std::mem::take(&mut w.top), std::mem::take(&mut w.changes), std::mem::take(&mut w.overflow));
        let (raw, records, raw_over) = (std::mem::take(&mut w.raw), std::mem::take(&mut w.records), std::mem::take(&mut w.raw_over));
        let edits = (!overflow && !raw_over).then(|| net_changes(raw));
        if top && !overflow {
            eprintln!(
                "[watch] pane={} tab={} subtree records {records} in the folder, changes {}, window {} ms",
                w.pane,
                w.tab_id,
                edits.as_ref().map_or("over the cap".to_string(), |e| e.len().to_string()),
                w.backoff.window.as_millis()
            );
        }
        return Event::Subtree { pane: w.pane, tab_id: w.tab_id, top, changes, overflow, edits, records };
    }
    let (raw, records, overflow) = (std::mem::take(&mut w.raw), std::mem::take(&mut w.records), std::mem::take(&mut w.overflow));
    let changes = if overflow { Vec::new() } else { net_changes(raw) };
    eprintln!(
        "[watch] pane={} tab={} records {records}, changes {}, window {} ms{}",
        w.pane,
        w.tab_id,
        changes.len(),
        w.backoff.window.as_millis(),
        if overflow { ", overflow" } else { "" }
    );
    Event::Changed { pane: w.pane, tab_id: w.tab_id, changes, records, overflow }
}

/// The records of a completed read: (action, name relative to the watched folder) for each
/// `FILE_NOTIFY_INFORMATION` (NextEntryOffset, Action, FileNameLength in bytes, then the UTF-16
/// name). Stops at a record that does not fit in `buf`.
fn parse_records(buf: &[u8]) -> Vec<(u32, String)> {
    let u32_at = |i: usize| buf.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let mut out = Vec::new();
    let mut at = 0usize;
    while let (Some(next), Some(action), Some(len)) = (u32_at(at), u32_at(at + 4), u32_at(at + 8)) {
        let Some(name) = buf.get(at + 12..at + 12 + len as usize) else { break };
        let wide: Vec<u16> = name.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        out.push((action, String::from_utf16_lossy(&wide)));
        if next == 0 {
            break;
        }
        at += next as usize;
    }
    out
}

const FILE_ACTION_ADDED: u32 = 1;
const FILE_ACTION_REMOVED: u32 = 2;
const FILE_ACTION_MODIFIED: u32 = 3;
const FILE_ACTION_RENAMED_OLD_NAME: u32 = 4;
const FILE_ACTION_RENAMED_NEW_NAME: u32 = 5;

/// FILE_ACTION_* as a word for the `[size] invalidated` line.
fn action_name(action: u32) -> &'static str {
    match action {
        1 => "added",
        2 => "removed",
        3 => "modified",
        4 => "renamed from",
        5 => "renamed to",
        _ => "changed",
    }
}

/// The net change per name of a window's raw records (FILE_ACTION_*, name). A rename's old name
/// followed by its new name is one rename; an old name with no new name after it is a removal, a
/// new name with no old name an addition. Names compare ASCII-case-insensitively (NTFS).
///
/// The changes come out so that applying them in order is right: departures first (removals, then
/// renames, each rename after the one that vacates its target), then additions, then
/// modifications; within a kind, in the order each entry first appeared. Renames that form a
/// cycle (a swap through a temporary name) become a modification of each name.
pub fn net_changes(raw: Vec<(u32, String)>) -> Vec<Change> {
    /// One entry: its name at the window's start (None: created in the window), its name now
    /// (None: gone), and whether its contents changed.
    struct Entry {
        start: Option<String>,
        now: Option<String>,
        modified: bool,
    }
    let key = |n: &str| n.to_ascii_lowercase();
    let mut entries: Vec<Entry> = Vec::new();
    // Lowercased current name -> live entry; lowercased start name -> entry that is gone.
    let mut live: HashMap<String, usize> = HashMap::new();
    let mut gone: HashMap<String, usize> = HashMap::new();
    let mut it = raw.into_iter().peekable();
    while let Some((action, name)) = it.next() {
        let k = key(&name);
        match action {
            FILE_ACTION_RENAMED_OLD_NAME if matches!(it.peek(), Some((FILE_ACTION_RENAMED_NEW_NAME, _))) => {
                let Some((_, to)) = it.next() else { unreachable!() };
                let i = live.remove(&k).unwrap_or_else(|| {
                    entries.push(Entry { start: Some(name), now: None, modified: false });
                    entries.len() - 1
                });
                live.insert(key(&to), i);
                entries[i].now = Some(to);
            }
            FILE_ACTION_REMOVED | FILE_ACTION_RENAMED_OLD_NAME => match live.remove(&k) {
                Some(i) => {
                    entries[i].now = None;
                    if let Some(s) = &entries[i].start {
                        gone.insert(key(s), i);
                    }
                }
                None => {
                    entries.push(Entry { start: Some(name), now: None, modified: false });
                    gone.insert(k, entries.len() - 1);
                }
            },
            FILE_ACTION_ADDED | FILE_ACTION_RENAMED_NEW_NAME => {
                if let Some(&i) = live.get(&k) {
                    entries[i].modified = true;
                } else if let Some(i) = gone.remove(&k) {
                    // Removed, then here again: still in the listing, its entry re-read.
                    entries[i].now = Some(name);
                    entries[i].modified = true;
                    live.insert(k, i);
                } else {
                    entries.push(Entry { start: None, now: Some(name), modified: false });
                    live.insert(k, entries.len() - 1);
                }
            }
            _ => match live.get(&k) {
                Some(&i) => entries[i].modified = true,
                None => {
                    entries.push(Entry { start: Some(name.clone()), now: Some(name), modified: true });
                    live.insert(k, entries.len() - 1);
                }
            },
        }
    }
    let (mut removed, mut renames, mut added, mut modified) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for e in entries {
        match (e.start, e.now) {
            (None, None) => {}
            (None, Some(n)) => added.push(Change::Added(n)),
            (Some(s), None) => removed.push(Change::Removed(s)),
            (Some(s), Some(n)) => {
                if s != n {
                    renames.push((s, n.clone()));
                }
                if e.modified {
                    modified.push(Change::Modified(n));
                }
            }
        }
    }
    // Each rename waits for the rename whose source is its target (a case-only rename waits for
    // nobody); when every one left waits, they contain a cycle: its names are modified instead.
    let mut ordered = Vec::new();
    let mut cycles = Vec::new();
    while !renames.is_empty() {
        let froms: HashMap<String, usize> = renames.iter().enumerate().map(|(i, (f, _))| (key(f), i)).collect();
        let waits = |i: usize| froms.get(&key(&renames[i].1)).is_some_and(|&j| j != i);
        if let Some(i) = (0..renames.len()).find(|&i| !waits(i)) {
            let (from, to) = renames.remove(i);
            ordered.push(Change::Renamed { from, to });
            continue;
        }
        let mut path = vec![0usize];
        let cycle = loop {
            let next = froms[&key(&renames[*path.last().unwrap()].1)];
            if let Some(at) = path.iter().position(|&x| x == next) {
                break path.split_off(at);
            }
            path.push(next);
        };
        let mut cycle = cycle;
        cycle.sort_unstable();
        for &i in cycle.iter().rev() {
            let (_, to) = renames.remove(i);
            cycles.push(to);
        }
    }
    for to in cycles.into_iter().rev() {
        if !modified.iter().any(|m| matches!(m, Change::Modified(n) if n.eq_ignore_ascii_case(&to))) {
            modified.push(Change::Modified(to));
        }
    }
    removed.into_iter().chain(ordered).chain(added).chain(modified).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch folder `%TEMP%\twinfm-p2m1-test-<pid>\<name>`, removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let p = std::env::temp_dir().join(format!("twinfm-p2m1-test-{}", std::process::id())).join(name);
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).expect("create scratch folder");
            Scratch(p)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
            if let Some(parent) = self.0.parent() {
                let _ = std::fs::remove_dir(parent); // only once the last test's folder is gone
            }
        }
    }

    fn watcher() -> (Watcher, Receiver<Event>) {
        let (tx, rx) = unbounded();
        (Watcher::spawn(tx, || {}), rx)
    }

    /// Every event that arrives within `window`.
    fn collect(rx: &Receiver<Event>, window: Duration) -> Vec<Event> {
        let end = Instant::now() + window;
        let mut out = Vec::new();
        while let Ok(e) = rx.recv_timeout(end.saturating_duration_since(Instant::now())) {
            out.push(e);
        }
        out
    }

    /// The next `Changed` within 2 s (other events skipped): (pane, tab, changes, overflow).
    fn next_changed(rx: &Receiver<Event>) -> (usize, u64, Vec<Change>, bool) {
        let end = Instant::now() + Duration::from_secs(2);
        while let Ok(e) = rx.recv_timeout(end.saturating_duration_since(Instant::now())) {
            if let Event::Changed { pane, tab_id, changes, overflow, .. } = e {
                return (pane, tab_id, changes, overflow);
            }
        }
        panic!("no Changed within 2 s");
    }

    fn s(n: &str) -> String {
        n.to_string()
    }

    #[test]
    fn create_file_gives_one_changed() {
        let dir = Scratch::new("create");
        let (w, rx) = watcher();
        w.watch(0, 7, dir.0.clone(), false, false);
        w.sync();
        std::fs::write(dir.0.join("a.txt"), b"x").unwrap();
        let (pane, tab, changes, overflow) = next_changed(&rx);
        assert_eq!((pane, tab, overflow), (0, 7, false));
        assert_eq!(changes, vec![Change::Added(s("a.txt"))], "a create and its write fold into Added");
        assert!(collect(&rx, Duration::from_millis(800)).is_empty(), "one create, one event");
    }

    #[test]
    fn bursts_are_coalesced() {
        let dir = Scratch::new("burst");
        let (w, rx) = watcher();
        w.watch(1, 3, dir.0.clone(), false, false);
        w.sync();
        for i in 0..50 {
            let f = dir.0.join(format!("f{i}.txt"));
            std::fs::write(&f, b"x").unwrap();
            std::fs::remove_file(&f).unwrap();
        }
        let events = collect(&rx, Duration::from_secs(2));
        assert!(!events.is_empty() && events.len() < 5, "50 creates and deletes gave {} events", events.len());
        assert!(events.iter().all(|e| matches!(e, Event::Changed { pane: 1, tab_id: 3, .. })));
        // Every file came and went inside the windows: nothing is left to apply (unless the
        // buffer overflowed).
        for e in &events {
            if let Event::Changed { changes, overflow: false, .. } = e {
                assert!(changes.is_empty(), "{changes:?}");
            }
        }
    }

    #[test]
    fn rename_and_attribute_change_are_changes() {
        let dir = Scratch::new("rename");
        let f = dir.0.join("a.txt");
        std::fs::write(&f, b"x").unwrap();
        let (w, rx) = watcher();
        w.watch(0, 1, dir.0.clone(), false, false);
        w.sync();
        std::fs::rename(&f, dir.0.join("b.txt")).unwrap();
        let (pane, tab, changes, _) = next_changed(&rx);
        assert_eq!((pane, tab), (0, 1));
        assert_eq!(changes, vec![Change::Renamed { from: s("a.txt"), to: s("b.txt") }], "rename");
        assert!(collect(&rx, Duration::from_millis(400)).is_empty());
        let mut perms = std::fs::metadata(dir.0.join("b.txt")).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(dir.0.join("b.txt"), perms.clone()).unwrap();
        let (_, tab, changes, _) = next_changed(&rx);
        assert_eq!(tab, 1);
        assert_eq!(changes, vec![Change::Modified(s("b.txt"))], "attributes");
        perms.set_readonly(false);
        std::fs::set_permissions(dir.0.join("b.txt"), perms).unwrap();
    }

    #[test]
    fn unwatch_stops_events() {
        let dir = Scratch::new("unwatch");
        let (w, rx) = watcher();
        w.watch(0, 1, dir.0.clone(), false, true);
        w.sync();
        w.unwatch(1);
        w.sync();
        std::fs::write(dir.0.join("a.txt"), b"x").unwrap();
        assert!(collect(&rx, Duration::from_millis(500)).is_empty());
    }

    #[test]
    fn replacing_a_watch_moves_it() {
        let (a, b) = (Scratch::new("replace-a"), Scratch::new("replace-b"));
        let (w, rx) = watcher();
        w.watch(0, 1, a.0.clone(), false, false);
        w.sync();
        w.watch(0, 1, b.0.clone(), false, false);
        w.sync();
        std::fs::write(a.0.join("old.txt"), b"x").unwrap();
        assert!(collect(&rx, Duration::from_millis(500)).is_empty(), "the old folder is no longer watched");
        std::fs::write(b.0.join("new.txt"), b"x").unwrap();
        assert_eq!(next_changed(&rx).1, 1);
    }

    #[test]
    fn rewatching_the_same_folder_delivers_the_gathered_window() {
        let dir = Scratch::new("rewatch");
        let (w, rx) = watcher();
        w.watch(0, 1, dir.0.clone(), false, false);
        w.sync();
        std::fs::write(dir.0.join("a.txt"), b"x").unwrap();
        std::thread::sleep(Duration::from_millis(50));
        // The subtree flag flips within the window: the plain watch's window comes at once.
        w.watch(0, 1, dir.0.clone(), true, false);
        w.sync();
        let got = collect(&rx, Duration::from_millis(100));
        assert!(
            matches!(&got[..], [Event::Changed { tab_id: 1, changes, overflow: false, .. }] if *changes == vec![Change::Added(s("a.txt"))]),
            "{got:?}"
        );
        std::fs::write(dir.0.join("b.txt"), b"x").unwrap();
        std::thread::sleep(Duration::from_millis(50));
        w.watch(0, 1, dir.0.clone(), false, false);
        w.sync();
        let got = collect(&rx, Duration::from_millis(100));
        assert!(matches!(&got[..], [Event::Subtree { tab_id: 1, top: true, overflow: false, .. }]), "{got:?}");
    }

    #[test]
    fn missing_folder_gets_no_watch() {
        let dir = Scratch::new("missing");
        let gone = dir.0.join("not-there");
        assert!(open_dir(&gone).is_err());
        assert_eq!(unwatchable(Path::new("This PC")), Some("the drive list is not a folder"));
        assert_eq!(unwatchable(Path::new("")), Some("the drive list is not a folder"));
        assert_eq!(unwatchable(Path::new(r"\\server\share\dir")), None, "network shares are watched");
        assert_eq!(unwatchable(&dir.0), None);
        let (w, rx) = watcher();
        w.watch(0, 1, gone.clone(), false, false);
        w.sync();
        assert!(matches!(rx.try_recv(), Ok(Event::Refused { pane: 0, tab_id: 1, ref path, .. }) if *path == gone));
        std::fs::write(dir.0.join("a.txt"), b"x").unwrap();
        assert!(collect(&rx, Duration::from_millis(500)).is_empty());
        // The watcher still works afterwards.
        w.watch(0, 2, dir.0.clone(), false, false);
        w.sync();
        std::fs::write(dir.0.join("b.txt"), b"x").unwrap();
        assert_eq!(next_changed(&rx).1, 2);
    }

    #[test]
    fn remote_paths_are_told_apart() {
        assert!(is_remote(Path::new(r"\\server\share\x")));
        assert!(is_remote(Path::new(r"\\?\UNC\server\share\x")));
        assert!(!is_remote(Path::new(r"C:\")));
    }

    #[test]
    fn deleting_the_folder_loses_the_watch() {
        let dir = Scratch::new("deleted");
        let inner = dir.0.join("watched");
        std::fs::create_dir(&inner).unwrap();
        std::fs::write(inner.join("a.txt"), b"x").unwrap();
        let (w, rx) = watcher();
        w.watch(1, 9, inner.clone(), false, true);
        w.sync();
        std::fs::remove_dir_all(&inner).unwrap();
        let events = collect(&rx, Duration::from_secs(2));
        assert!(
            events.iter().any(|e| matches!(e, Event::Lost { pane: 1, tab_id: 9, .. })),
            "expected Lost, got {events:?}"
        );
    }

    #[test]
    fn renaming_the_watched_folder_is_noticed() {
        let dir = Scratch::new("moved");
        let (before, after) = (dir.0.join("before"), dir.0.join("after"));
        std::fs::create_dir(&before).unwrap();
        let (w, rx) = watcher();
        w.watch(0, 5, before.clone(), false, true);
        w.sync();
        std::fs::rename(&before, &after).unwrap();
        let end = Instant::now() + Duration::from_secs(2);
        let mut moved = None;
        while let Ok(e) = rx.recv_timeout(end.saturating_duration_since(Instant::now())) {
            if let Event::Moved { pane: 0, tab_id: 5, from, to } = e {
                moved = Some((from, to));
                break;
            }
        }
        assert_eq!(moved, Some((before, after.clone())));
        std::fs::write(after.join("x.txt"), b"x").unwrap();
        let (pane, tab, changes, _) = next_changed(&rx);
        assert_eq!((pane, tab), (0, 5));
        assert_eq!(changes, vec![Change::Added(s("x.txt"))]);
    }

    #[test]
    fn moving_the_watched_folder_to_another_parent_follows_it() {
        // P11-M3: before it the watch was lost; the folder's own handle names its new place.
        let dir = Scratch::new("moved-away");
        let (from, to) = (dir.0.join("a"), dir.0.join("b"));
        std::fs::create_dir_all(from.join("watched")).unwrap();
        std::fs::create_dir(&to).unwrap();
        let (w, rx) = watcher();
        w.watch(0, 6, from.join("watched"), false, true);
        w.sync();
        std::fs::rename(from.join("watched"), to.join("watched")).unwrap();
        let events = collect(&rx, Duration::from_secs(1));
        let moved: Vec<_> = events.iter().filter_map(|e| if let Event::Moved { tab_id: 6, to, .. } = e { Some(to.clone()) } else { None }).collect();
        assert_eq!(moved.len(), 1, "{events:?}");
        let canon = |p: &Path| std::fs::canonicalize(p).unwrap();
        assert_eq!(canon(&moved[0]), canon(&to.join("watched")));
        assert!(!events.iter().any(|e| matches!(e, Event::Lost { tab_id: 6, .. })), "{events:?}");
        // The watch goes on in the new place: a file made there is a change of this tab.
        std::fs::write(to.join(r"watched\x.txt"), b"x").unwrap();
        let (pane, tab, changes, _) = next_changed(&rx);
        assert_eq!((pane, tab), (0, 6));
        assert_eq!(changes, vec![Change::Added(s("x.txt"))]);
    }

    #[test]
    fn watches_are_keyed_by_tab_and_capped() {
        let dirs: Vec<Scratch> = (0..10).map(|i| Scratch::new(&format!("cap{i}"))).collect();
        let (w, rx) = watcher();
        for (i, d) in dirs.iter().enumerate().take(9) {
            w.watch(0, i as u64, d.0.clone(), false, false);
        }
        w.sync();
        let events = collect(&rx, Duration::from_millis(100));
        assert_eq!(
            events,
            vec![Event::Refused { pane: 0, tab_id: 8, path: dirs[8].0.clone(), why: s("watch cap reached (8)") }]
        );
        w.unwatch(3);
        w.watch(0, 9, dirs[9].0.clone(), false, false);
        w.sync();
        assert!(rx.try_recv().is_err(), "the tenth fits once one is gone");
        std::fs::write(dirs[9].0.join("a.txt"), b"x").unwrap();
        assert_eq!(next_changed(&rx).1, 9);
        std::fs::write(dirs[0].0.join("a.txt"), b"x").unwrap();
        assert_eq!(next_changed(&rx).1, 0, "a pane's other tabs keep their watches");
    }

    #[test]
    fn continuous_changes_back_off() {
        let dir = Scratch::new("churn");
        let (w, rx) = watcher();
        w.watch(0, 2, dir.0.clone(), false, false);
        w.sync();
        let start = Instant::now();
        let (mut i, mut times) = (0u32, Vec::new());
        while start.elapsed() < Duration::from_secs(3) {
            std::fs::write(dir.0.join("churn.txt"), i.to_le_bytes()).unwrap();
            i += 1;
            std::thread::sleep(Duration::from_millis(20));
            while let Ok(e) = rx.try_recv() {
                assert!(matches!(e, Event::Changed { pane: 0, tab_id: 2, .. }), "{e:?}");
                times.push(Instant::now());
            }
        }
        let end = start + Duration::from_millis(5500);
        while let Ok(e) = rx.recv_timeout(end.saturating_duration_since(Instant::now())) {
            assert!(matches!(e, Event::Changed { pane: 0, tab_id: 2, .. }), "{e:?}");
            times.push(Instant::now());
        }
        assert!(times.len() >= 2 && times.len() < 12, "{} events in 3 s of churn", times.len());
        let widest = times.windows(2).map(|t| t[1] - t[0]).max().unwrap();
        assert!(widest > Duration::from_millis(600), "the window never grew: widest gap {widest:?}");
    }

    #[test]
    fn drop_stops_the_thread() {
        let dir = Scratch::new("drop");
        let (w, _rx) = watcher();
        w.watch(0, 1, dir.0.clone(), false, true);
        w.watch(1, 2, dir.0.clone(), false, false);
        w.sync();
        let (done_tx, done_rx) = unbounded();
        std::thread::spawn(move || {
            drop(w); // returns once the dirwatch thread has finished
            let _ = done_tx.send(());
        });
        assert!(done_rx.recv_timeout(Duration::from_secs(3)).is_ok(), "the watcher thread did not exit");
    }

    #[test]
    fn records_parse() {
        let mut buf = Vec::new();
        for (i, (action, name)) in [(1u32, "a.txt"), (3, r"sub\deep")].iter().enumerate() {
            let wide: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
            let len = 12 + wide.len();
            let next = if i == 0 { len.div_ceil(4) * 4 } else { 0 };
            buf.extend((next as u32).to_le_bytes());
            buf.extend(action.to_le_bytes());
            buf.extend((wide.len() as u32).to_le_bytes());
            buf.extend(&wide);
            buf.resize(buf.len() + next.saturating_sub(len), 0);
        }
        assert_eq!(parse_records(&buf), vec![(1, "a.txt".to_string()), (3, r"sub\deep".to_string())]);
        assert_eq!(parse_records(&buf[..10]), vec![], "a cut record is dropped");
    }

    #[test]
    fn net_changes_fold_a_window() {
        let n = |v: &[(u32, &str)]| net_changes(v.iter().map(|&(a, n)| (a, n.to_string())).collect());
        use Change::*;
        assert_eq!(n(&[(1, "a"), (2, "a")]), vec![], "added then removed");
        assert_eq!(n(&[(1, "a"), (3, "a")]), vec![Added(s("a"))], "added then modified");
        assert_eq!(n(&[(3, "a"), (3, "a")]), vec![Modified(s("a"))], "modified twice");
        assert_eq!(n(&[(3, "a"), (2, "a")]), vec![Removed(s("a"))], "modified then removed");
        assert_eq!(n(&[(2, "a"), (1, "a")]), vec![Modified(s("a"))], "removed then added");
        assert_eq!(n(&[(4, "a"), (5, "b"), (2, "b")]), vec![Removed(s("a"))], "renamed then removed");
        assert_eq!(n(&[(4, "a"), (5, "b"), (4, "b"), (5, "c")]), vec![Renamed { from: s("a"), to: s("c") }], "renamed twice");
        assert_eq!(n(&[(1, "a"), (4, "a"), (5, "b")]), vec![Added(s("b"))], "added then renamed");
        assert_eq!(n(&[(1, "a"), (3, "a"), (3, "a")]), vec![Added(s("a"))], "an added name stays added");
        assert_eq!(n(&[(4, "a")]), vec![Removed(s("a"))], "a cut rename is a removal");
        assert_eq!(n(&[(4, "a"), (1, "x")]), vec![Removed(s("a")), Added(s("x"))], "an old name not followed by its new one");
        assert_eq!(n(&[(5, "b")]), vec![Added(s("b"))], "a new name alone is an addition");
        assert_eq!(n(&[(1, "A.txt"), (3, "a.TXT"), (2, "a.txt")]), vec![], "names compare case-insensitively");
        assert_eq!(n(&[(4, "a"), (5, "b"), (4, "b"), (5, "a")]), vec![], "renamed and back");
        assert_eq!(
            n(&[(3, "z"), (1, "a"), (2, "y"), (3, "z"), (2, "x")]),
            vec![Removed(s("y")), Removed(s("x")), Added(s("a")), Modified(s("z"))],
            "departures, then arrivals, then modifications; each in order of first appearance"
        );
        assert_eq!(
            n(&[(1, "doc.tmp"), (4, "doc"), (5, "doc.bak"), (4, "doc.tmp"), (5, "doc")]),
            vec![Renamed { from: s("doc"), to: s("doc.bak") }, Added(s("doc"))],
            "a safe-save: the old file is renamed away before the new one takes its name"
        );
        assert_eq!(
            n(&[(4, "a"), (5, "t"), (4, "c"), (5, "a"), (4, "t"), (5, "c")]),
            vec![Modified(s("c")), Modified(s("a"))],
            "a swap through a temporary name modifies both names"
        );
        assert_eq!(
            n(&[(4, "b"), (5, "c"), (4, "a"), (5, "b")]),
            vec![Renamed { from: s("b"), to: s("c") }, Renamed { from: s("a"), to: s("b") }],
            "a shift: b leaves before a takes its name"
        );
        assert_eq!(
            n(&[(4, "a"), (5, "b"), (4, "c"), (5, "a")]),
            vec![Renamed { from: s("a"), to: s("b") }, Renamed { from: s("c"), to: s("a") }],
            "a rename into a name another rename vacated comes after it"
        );
        assert_eq!(n(&[(4, "a"), (5, "A")]), vec![Renamed { from: s("a"), to: s("A") }], "a case-only rename");
    }

    #[test]
    fn backoff_grows_under_churn_and_resets_after_quiet() {
        let t0 = Instant::now();
        let ms = |m: u64| t0 + Duration::from_millis(m);
        let mut b = Backoff::default();
        assert_eq!(b.next_window(ms(0)), COALESCE, "the first change");
        b.delivered(ms(250));
        assert_eq!(b.next_window(ms(300)), Duration::from_millis(500));
        b.delivered(ms(800));
        assert_eq!(b.next_window(ms(900)), Duration::from_millis(1000));
        b.delivered(ms(1900));
        assert_eq!(b.next_window(ms(2000)), COALESCE_MAX);
        b.delivered(ms(4000));
        assert_eq!(b.next_window(ms(4100)), COALESCE_MAX, "capped");
        b.delivered(ms(6100));
        assert_eq!(b.next_window(ms(6100) + QUIET + Duration::from_millis(1)), COALESCE, "a quiet spell resets it");
        b.delivered(ms(8000));
        assert_eq!(b.next_window(ms(8000) + QUIET), Duration::from_millis(500), "within QUIET still grows");
    }

    #[test]
    fn subtree_watch_tells_deep_changes_from_top_ones() {
        let dir = Scratch::new("subtree");
        std::fs::create_dir_all(dir.0.join(r"sub\deep")).unwrap();
        let (w, rx) = watcher();
        w.watch(0, 4, dir.0.clone(), true, false);
        w.sync();
        std::fs::write(dir.0.join(r"sub\deep\x.txt"), b"x").unwrap();
        let ev = rx.recv_timeout(Duration::from_secs(2)).expect("an event within 2 s");
        let Event::Subtree { pane: 0, tab_id: 4, top, changes, overflow, edits, .. } = ev else { panic!("expected Subtree, got {ev:?}") };
        assert!(!top && !overflow, "a change two levels down is not in the folder itself: {changes:?}");
        // NTFS may report "sub" itself modified (lazily): that is the folder's own record.
        assert!(edits.as_ref().is_some_and(|e| e.iter().all(|c| *c == Change::Modified(s("sub")))), "{edits:?}");
        assert_eq!(changes.first().map(|(c, _)| c.as_str()), Some("sub"));
        assert!(changes.iter().all(|(c, _)| c == "sub"), "{changes:?}");
        assert!(collect(&rx, Duration::from_millis(400)).is_empty());
        std::fs::write(dir.0.join("top.txt"), b"x").unwrap();
        let ev = rx.recv_timeout(Duration::from_secs(2)).expect("an event within 2 s");
        assert!(matches!(ev, Event::Subtree { top: true, .. }), "{ev:?}");
        // The folder's own records come as the plain watch's net changes (P7-M4: row edits).
        let Event::Subtree { edits: Some(edits), records, .. } = ev else { panic!("expected edits") };
        assert!(edits.contains(&Change::Added(s("top.txt"))), "{edits:?}");
        assert!(records >= 1);
        // Back to shallow: a deep change is no longer seen, a top-level one is a plain Changed.
        w.watch(0, 4, dir.0.clone(), false, false);
        w.sync();
        std::fs::write(dir.0.join(r"sub\deep\y.txt"), b"x").unwrap();
        assert!(collect(&rx, Duration::from_millis(600)).is_empty());
        std::fs::write(dir.0.join("top2.txt"), b"x").unwrap();
        let (pane, tab, changes, _) = next_changed(&rx);
        assert_eq!((pane, tab), (0, 4));
        // NTFS may also report "sub" modified, lazily, after the deep write.
        assert!(changes.contains(&Change::Added(s("top2.txt"))), "{changes:?}");
    }

    #[test]
    fn a_subtree_watch_past_the_record_cap_gives_no_edits() {
        // P11-M3: a burst of more than `MAX_RECORDS` records in the folder itself under a subtree
        // watch is a window with no edits (`raw_over`), so the UI lists the folder again instead of
        // folding them. (The buffer's own overflow, `overflow: true`, could not be forced here: the
        // watcher kept up with 3,200 records of over 400 bytes from eight threads.)
        let dir = Scratch::new("subtree-overflow");
        std::fs::create_dir_all(dir.0.join("sub")).unwrap();
        let (w, rx) = watcher();
        w.watch(0, 7, dir.0.clone(), true, false);
        w.sync();
        for i in 0..(2 * MAX_RECORDS) {
            std::fs::write(dir.0.join(format!("burst-{i:05}.txt")), b"x").unwrap();
        }
        let events = collect(&rx, Duration::from_secs(4));
        let capped = events.iter().filter(|e| matches!(e, Event::Subtree { tab_id: 7, top: true, overflow: false, edits: None, records, .. } if *records > MAX_RECORDS)).count();
        assert!(capped >= 1, "expected a window past the cap with no edits, got {events:?}");
    }
}
