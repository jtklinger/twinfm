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
use std::collections::HashMap;
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
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDriveTypeW, OPEN_EXISTING, ReadDirectoryChangesW,
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

/// The buffer of a parent watch (folder names only).
const PARENT_BUF_BYTES: usize = 4 * 1024;

/// Completion key of a command post. Watches use their own (heap) address, never 1 or 2.
const KEY_CMD: usize = 1;

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
    /// entry added, removed, renamed or modified; the listing needs a refresh). `changes`: each
    /// child of the folder that a record lay below or added, removed or renamed (not one that only
    /// modified the child's own entry), once, with the first such record ("added sub\x.txt"). `overflow`: the buffer overflowed, so what changed is unknown.
    Subtree { pane: usize, tab_id: u64, top: bool, changes: Vec<(String, String)>, overflow: bool },
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
                Err(_) => eprintln!("[devices] query remove for a watched handle: no reply within 1 s"),
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
    /// A plain watch's records of the current window, and how many there were.
    raw: Vec<(u32, String)>,
    records: usize,
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
            removable: role == Role::Main && drive_type(&path) == Some(crate::fs::enumerate::DRIVE_REMOVABLE),
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
            parent: std::ptr::null_mut(),
            wants_parent: false,
            cut_old: false,
            notify: 0,
            suspended: false,
            resume: false,
        }
    }
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
    cmds: Receiver<Cmd>,
    events: Sender<Event>,
    wake: W,
}

fn last_error() -> String {
    // SAFETY: reads this thread's last-error value.
    std::io::Error::from_raw_os_error(unsafe { GetLastError() } as i32).to_string()
}

/// `GetDriveTypeW` of a drive-letter path's root; None for any other path.
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

/// Open `path` for watching: the directory handle, or why not. A network folder is opened on a
/// helper thread, waited for at most `OPEN_TIMEOUT`; a late open is closed by the helper.
fn open_dir(path: &Path) -> Result<HANDLE, String> {
    if let Some(why) = unwatchable(path) {
        return Err(why.into());
    }
    if !is_remote(path) {
        return open_now(path).map(|h| h as HANDLE);
    }
    // A rendezvous channel: the handle is either received here or handed back to the helper.
    let (tx, rx) = bounded::<Result<usize, String>>(0);
    let p = path.to_path_buf();
    let spawned = std::thread::Builder::new().name("dirwatch-open".into()).spawn(move || {
        if let Err(crossbeam_channel::SendError(Ok(h))) = tx.send(open_now(&p)) {
            // SAFETY: the open completed after the watcher gave up; nobody else has the handle.
            unsafe { CloseHandle(h as HANDLE) };
        }
    });
    if let Err(e) = spawned {
        return Err(format!("no open thread: {e}"));
    }
    match rx.recv_timeout(OPEN_TIMEOUT) {
        Ok(r) => r.map(|h| h as HANDLE),
        Err(_) => Err("stalled: no watch".into()),
    }
}

/// Register for the device events of the drive under `handle`: the HDEVNOTIFY, or 0.
fn register(hwnd: isize, handle: HANDLE, w: &Watch) -> isize {
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
        eprintln!("[watch] pane={} tab={} no device notification for {}: {}", w.pane, w.tab_id, w.path.display(), last_error());
    }
    n
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
            if !ov.is_null() {
                self.completed(ov as *mut Watch, key, ok, err, bytes);
            } else if ok && key == KEY_CMD {
                if !self.commands() {
                    return self.shutdown();
                }
            } else if !ok && err != WAIT_TIMEOUT {
                eprintln!("[watch] completion port failed: {}; the watcher stops", std::io::Error::from_raw_os_error(err as i32));
                return self.shutdown();
            }
            self.deliver();
        }
    }

    fn next_due(&self) -> Option<Instant> {
        // SAFETY: every slot is a live watch owned by this thread.
        self.slots.values().filter_map(|&p| unsafe { (*p).due }).min()
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
                    if let Some(p) = self.slots.remove(&tab_id) {
                        self.retire(p);
                    }
                }
                Cmd::UnwatchPane(pane) => {
                    self.refused.retain(|_, (p, _)| *p != pane);
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
                            w.notify = register(hwnd, w.handle, w);
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
        if let Some(&p) = self.slots.get(&tab_id) {
            // SAFETY: a live watch owned by this thread.
            let w = unsafe { &mut *p };
            if w.path == path && w.subtree == subtree {
                w.pane = pane;
                if parent != w.wants_parent && !w.suspended {
                    w.wants_parent = parent;
                    if parent {
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
            self.slots.remove(&tab_id);
            self.retire(p);
        }
        if self.slots.len() >= MAX_WATCHES {
            return self.refuse(pane, tab_id, path, format!("watch cap reached ({MAX_WATCHES})"));
        }
        let handle = match open_dir(&path) {
            Ok(h) => h,
            Err(why) => return self.refuse(pane, tab_id, path, why),
        };
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
            w.notify = register(self.hwnd, handle, w);
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
        let handle = match open_dir(dir) {
            Ok(h) => h,
            Err(why) => return no_parent(&why),
        };
        let pp = Box::into_raw(Box::new(Watch::new(Role::Parent, handle, w.pane, w.tab_id, dir.to_path_buf(), false)));
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
                let renamed_to = if std::mem::take(&mut pw.cut_old) && *action == FILE_ACTION_RENAMED_NEW_NAME {
                    Some(n.clone())
                } else if *action == FILE_ACTION_RENAMED_OLD_NAME && n.eq_ignore_ascii_case(&name) {
                    match records.get(i + 1) {
                        Some((FILE_ACTION_RENAMED_NEW_NAME, new)) => {
                            i += 1;
                            Some(new.clone())
                        }
                        Some(_) => None,
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
                    return self.lose(m, "the folder was deleted".into());
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
            w.notify = register(self.hwnd, handle, w);
        }
        eprintln!("[watch] pane={} tab={} resumed after a failed removal: {}", w.pane, w.tab_id, w.path.display());
        let (pane, tab_id) = (w.pane, w.tab_id);
        if w.wants_parent {
            self.open_parent(p);
        }
        self.send(Event::Changed { pane, tab_id, changes: Vec::new(), records: 0, overflow: true });
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
                w.due = None;
                w.backoff.delivered(now);
                let ev = if w.subtree {
                    let (top, changes, overflow) = (std::mem::take(&mut w.top), std::mem::take(&mut w.changes), std::mem::take(&mut w.overflow));
                    Event::Subtree { pane: w.pane, tab_id: w.tab_id, top, changes, overflow }
                } else {
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
                };
                sent |= self.events.send(ev).is_ok();
            }
        }
        if sent {
            (self.wake)();
        }
    }

    /// Quit: cancel every watch and wait (bounded) for the cancelled reads and helpers, so no
    /// buffer is freed while the kernel may still write into it.
    fn shutdown(mut self) {
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
            if !ov.is_null() {
                self.completed(ov as *mut Watch, key, ok, 0, 0);
            }
        }
    }
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

/// The net change per name of a window's raw records (FILE_ACTION_*, name), in the order each
/// entry first appeared. A rename's old name followed by its new name is one rename; an old name
/// with no new name after it is a removal, a new name with no old name an addition. Names compare
/// ASCII-case-insensitively (NTFS).
pub fn net_changes(raw: Vec<(u32, String)>) -> Vec<Change> {
    /// One entry: its name at the window's start (None: created in the window), its name now
    /// (None: gone), and whether its contents changed.
    struct Entry {
        start: Option<String>,
        now: Option<String>,
        modified: bool,
    }
    let same = |a: &Option<String>, b: &str| a.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(b));
    let mut entries: Vec<Entry> = Vec::new();
    let mut it = raw.into_iter().peekable();
    while let Some((action, name)) = it.next() {
        let live = entries.iter().position(|e| same(&e.now, &name));
        match action {
            FILE_ACTION_RENAMED_OLD_NAME if matches!(it.peek(), Some((FILE_ACTION_RENAMED_NEW_NAME, _))) => {
                let Some((_, to)) = it.next() else { unreachable!() };
                match live {
                    Some(i) => entries[i].now = Some(to),
                    None => entries.push(Entry { start: Some(name), now: Some(to), modified: false }),
                }
            }
            FILE_ACTION_REMOVED | FILE_ACTION_RENAMED_OLD_NAME => match live {
                Some(i) => entries[i].now = None,
                None => entries.push(Entry { start: Some(name), now: None, modified: false }),
            },
            FILE_ACTION_ADDED | FILE_ACTION_RENAMED_NEW_NAME => {
                if let Some(i) = live {
                    entries[i].modified = true;
                } else if let Some(i) = entries.iter().position(|e| e.now.is_none() && same(&e.start, &name)) {
                    // Removed, then here again: still in the listing, its entry re-read.
                    entries[i].now = Some(name);
                    entries[i].modified = true;
                } else {
                    entries.push(Entry { start: None, now: Some(name), modified: false });
                }
            }
            _ => match live {
                Some(i) => entries[i].modified = true,
                None => entries.push(Entry { start: Some(name.clone()), now: Some(name), modified: true }),
            },
        }
    }
    let mut out = Vec::new();
    for e in entries {
        match (e.start, e.now) {
            (None, None) => {}
            (None, Some(n)) => out.push(Change::Added(n)),
            (Some(s), None) => out.push(Change::Removed(s)),
            (Some(s), Some(n)) => {
                if s != n {
                    out.push(Change::Renamed { from: s, to: n.clone() });
                }
                if e.modified {
                    out.push(Change::Modified(n));
                }
            }
        }
    }
    out
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
            n(&[(3, "z"), (1, "a"), (2, "y"), (3, "z")]),
            vec![Modified(s("z")), Added(s("a")), Removed(s("y"))],
            "order of first appearance"
        );
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
        let Event::Subtree { pane: 0, tab_id: 4, top, changes, overflow } = ev else { panic!("expected Subtree, got {ev:?}") };
        assert!(!top && !overflow, "a change two levels down is not in the folder itself: {changes:?}");
        assert_eq!(changes.first().map(|(c, _)| c.as_str()), Some("sub"));
        assert!(changes.iter().all(|(c, _)| c == "sub"), "{changes:?}");
        assert!(collect(&rx, Duration::from_millis(400)).is_empty());
        std::fs::write(dir.0.join("top.txt"), b"x").unwrap();
        let ev = rx.recv_timeout(Duration::from_secs(2)).expect("an event within 2 s");
        assert!(matches!(ev, Event::Subtree { top: true, .. }), "{ev:?}");
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
}
