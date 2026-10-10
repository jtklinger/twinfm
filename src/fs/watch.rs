//! Directory watching. One thread ("dirwatch") owns every `ReadDirectoryChangesW` watch in the
//! app; the UI thread only sends it commands and drains its events.
//!
//! Each watched folder is a directory handle (`FILE_LIST_DIRECTORY`, opened with
//! `FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED`) associated with one I/O completion port.
//! The thread waits on `GetQueuedCompletionStatus`; commands from the UI thread travel over a
//! channel, announced by a `PostQueuedCompletionStatus` with the completion key `KEY_CMD`, and the
//! wait's timeout is the coalescing timer.
//!
//! Records are not parsed: any completion, a buffer overflow (`ERROR_NOTIFY_ENUM_DIR`) included,
//! means "the folder changed", and the UI re-enumerates it through its refresh path. Changes are
//! coalesced per watch: the first change after a quiet spell is delivered 250 ms later (the
//! trailing edge, so a burst becomes one event), and a continuous stream delivers one event per
//! 250 ms.
//!
//! A watch's state is a `Box<Watch>` whose first field is its `OVERLAPPED`, so the `OVERLAPPED`
//! pointer a completion carries is the watch's identity. A replaced or removed watch is cancelled
//! (`CancelIoEx`, `CloseHandle`) and kept alive until its cancelled read completes
//! (`ERROR_OPERATION_ABORTED`): the kernel writes into its buffer until then, and the completion
//! must find the old watch, never a new one at a recycled address.

use crossbeam_channel::{Receiver, Sender, unbounded};
use std::collections::HashMap;
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf, Prefix};
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

/// At most one `Changed` per watch per this interval.
pub const COALESCE: Duration = Duration::from_millis(250);

/// The notification buffer per watch, in bytes (DWORD-aligned: it is a `Vec<u32>`).
const BUF_BYTES: usize = 64 * 1024;

/// Completion key of a command post. Watches use their own (heap) address, never 1.
const KEY_CMD: usize = 1;

const FILTER: u32 = FILE_NOTIFY_CHANGE_FILE_NAME
    | FILE_NOTIFY_CHANGE_DIR_NAME
    | FILE_NOTIFY_CHANGE_ATTRIBUTES
    | FILE_NOTIFY_CHANGE_SIZE
    | FILE_NOTIFY_CHANGE_LAST_WRITE;

/// What the watcher tells the UI thread. `tab_id` is the tab the watch was started for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The folder changed (coalesced).
    Changed { pane: usize, tab_id: u64 },
    /// The watch failed after it started (folder deleted, drive gone, access revoked) and is
    /// closed.
    Lost { pane: usize, tab_id: u64, error: String },
}

enum Cmd {
    Watch { pane: usize, tab_id: u64, path: PathBuf },
    Unwatch(usize),
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
                Dirwatch { port: port as HANDLE, slots: HashMap::new(), dying: Vec::new(), refused: HashMap::new(), cmds: cmd_rx, events: tx, wake }.run();
                let _ = exit_tx.send(());
            })
            .expect("spawn dirwatch thread");
        Watcher { port, tx: cmd_tx, exited: Some(exit_rx) }
    }

    fn send(&self, cmd: Cmd) {
        if self.port == 0 || self.tx.send(cmd).is_err() {
            return;
        }
        // SAFETY: posts a packet with no OVERLAPPED to our own port.
        unsafe { PostQueuedCompletionStatus(self.port as HANDLE, 0, KEY_CMD, std::ptr::null()) };
    }

    /// Watch `path` for `pane`, replacing whatever the pane watched before. A no-op when the pane
    /// already watches that folder (only the tab id is updated).
    pub fn watch(&self, pane: usize, tab_id: u64, path: PathBuf) {
        self.send(Cmd::Watch { pane, tab_id, path });
    }

    /// Stop watching for `pane`.
    pub fn unwatch(&self, pane: usize) {
        self.send(Cmd::Unwatch(pane));
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
        if let Err(crossbeam_channel::RecvTimeoutError::Timeout) = exited.recv_timeout(Duration::from_secs(1)) {
            // Still running (or terminated by process exit): leave the port open for it.
            eprintln!("[watch] the watcher thread did not stop within 1 s");
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

/// One watch. `ov` must stay the first field: a completion's `OVERLAPPED` pointer is the watch.
#[repr(C)]
struct Watch {
    ov: OVERLAPPED,
    handle: HANDLE,
    pane: usize,
    tab_id: u64,
    path: PathBuf,
    buf: Vec<u32>,
    /// A read is issued and its completion has not been dequeued yet.
    pending: bool,
    /// Replaced or removed: free it when its read completes.
    cancelled: bool,
    /// A change is waiting to be delivered at this time.
    due: Option<Instant>,
}

struct Dirwatch<W: Fn()> {
    port: HANDLE,
    /// The live watch of each pane.
    slots: HashMap<usize, *mut Watch>,
    /// Cancelled watches whose read has not completed yet.
    dying: Vec<*mut Watch>,
    /// The path each pane last failed to watch, so the refusal is logged once.
    refused: HashMap<usize, PathBuf>,
    cmds: Receiver<Cmd>,
    events: Sender<Event>,
    wake: W,
}

fn last_error() -> String {
    // SAFETY: reads this thread's last-error value.
    std::io::Error::from_raw_os_error(unsafe { GetLastError() } as i32).to_string()
}

/// Why `path` gets no watch, before any handle is opened: the drive list; a network drive or
/// share (ReadDirectoryChangesW over SMB is unreliable and can hang on a dead share); a removable
/// drive or disc (an open directory handle makes Windows refuse to eject it; F5 still refreshes).
fn unwatchable(path: &Path) -> Option<&'static str> {
    if crate::fs::enumerate::is_this_pc(&path.to_string_lossy()) {
        return Some("the drive list is not a folder");
    }
    let Some(Component::Prefix(p)) = path.components().next() else { return Some("not an absolute path") };
    match p.kind() {
        Prefix::UNC(..) | Prefix::VerbatimUNC(..) => Some("network share"),
        Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
            let root: Vec<u16> = [letter as u16, b':' as u16, b'\\' as u16, 0].to_vec();
            // SAFETY: `root` is a NUL-terminated wide string for the duration of the call.
            let kind = unsafe { GetDriveTypeW(root.as_ptr()) };
            use crate::fs::enumerate::{DRIVE_CDROM, DRIVE_REMOTE, DRIVE_REMOVABLE};
            match kind {
                DRIVE_REMOTE => Some("network drive"),
                DRIVE_REMOVABLE | DRIVE_CDROM => Some("removable drive"),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Open `path` for watching: the directory handle, or why not.
fn open_dir(path: &Path) -> Result<HANDLE, String> {
    if let Some(why) = unwatchable(path) {
        return Err(why.into());
    }
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
    if h == INVALID_HANDLE_VALUE { Err(last_error()) } else { Ok(h) }
}

/// Issue the watch's next read.
///
/// # Safety
/// `w` is a live, boxed watch whose handle is associated with the port and has no read pending.
unsafe fn arm(w: &mut Watch) -> Result<(), String> {
    w.ov = OVERLAPPED::default();
    // SAFETY: the buffer and OVERLAPPED live in the Box until this read's completion is dequeued.
    let ok = unsafe {
        ReadDirectoryChangesW(
            w.handle,
            w.buf.as_mut_ptr() as *mut c_void,
            BUF_BYTES as u32,
            0,
            FILTER,
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
                self.completed(ov as *mut Watch, ok, err);
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

    /// Handle every queued command; false on Quit.
    fn commands(&mut self) -> bool {
        while let Ok(cmd) = self.cmds.try_recv() {
            match cmd {
                Cmd::Watch { pane, tab_id, path } => self.start(pane, tab_id, path),
                Cmd::Unwatch(pane) => {
                    self.refused.remove(&pane);
                    if let Some(p) = self.slots.remove(&pane) {
                        self.retire(p);
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

    /// No watch for `path`: say why once (the UI asks again when the listing completes, and on
    /// every refresh; the same refusal is not repeated until the pane watches something else).
    fn refuse(&mut self, pane: usize, path: PathBuf, why: String) {
        if self.refused.get(&pane) != Some(&path) {
            eprintln!("[watch] pane={pane} no watch for {}: {why}", path.display());
            self.refused.insert(pane, path);
        }
    }

    fn start(&mut self, pane: usize, tab_id: u64, path: PathBuf) {
        if let Some(&p) = self.slots.get(&pane) {
            // SAFETY: a live watch owned by this thread.
            let w = unsafe { &mut *p };
            if w.path == path {
                w.tab_id = tab_id;
                return;
            }
            self.slots.remove(&pane);
            self.retire(p);
        }
        let handle = match open_dir(&path) {
            Ok(h) => h,
            Err(why) => return self.refuse(pane, path, why),
        };
        let p = Box::into_raw(Box::new(Watch {
            ov: OVERLAPPED::default(),
            handle,
            pane,
            tab_id,
            path,
            buf: vec![0u32; BUF_BYTES / 4],
            pending: false,
            cancelled: false,
            due: None,
        }));
        // SAFETY: `p` is the Box just leaked; the handle is open; the key is the watch's address.
        let res = unsafe {
            if CreateIoCompletionPort(handle, self.port, p as usize, 0).is_null() { Err(last_error()) } else { arm(&mut *p) }
        };
        // SAFETY: as above.
        let w = unsafe { &mut *p };
        match res {
            Ok(()) => {
                eprintln!("[watch] pane={pane} tab={tab_id} watching {}", w.path.display());
                self.slots.insert(pane, p);
                self.refused.remove(&pane);
            }
            Err(why) => {
                // SAFETY: no read is pending (arm failed or never ran), so nothing refers to it.
                let w = unsafe {
                    CloseHandle(handle);
                    Box::from_raw(p)
                };
                self.refuse(pane, w.path, why);
            }
        }
    }

    /// A watch leaves its slot: cancel its read and close it; free it now if no read is pending,
    /// else when the cancelled read completes.
    fn retire(&mut self, p: *mut Watch) {
        // SAFETY: a live watch owned by this thread, just taken out of its slot.
        let w = unsafe { &mut *p };
        w.cancelled = true;
        w.due = None;
        // SAFETY: the handle is open; cancelling and closing are both fine with a read pending
        // (its completion is still queued to the port, with ERROR_OPERATION_ABORTED).
        unsafe {
            if w.pending {
                CancelIoEx(w.handle, &w.ov);
            }
            CloseHandle(w.handle);
        }
        if w.pending {
            self.dying.push(p);
        } else {
            // SAFETY: no read refers to it.
            unsafe { drop(Box::from_raw(p)) };
        }
    }

    /// A read completed (or failed) for the watch at `p`.
    fn completed(&mut self, p: *mut Watch, ok: bool, err: u32) {
        // SAFETY: only reads of live or dying watches are ever issued, and neither is freed while
        // its read is pending, so `p` is a live Box.
        let w = unsafe { &mut *p };
        w.pending = false;
        if w.cancelled {
            self.dying.retain(|&d| d != p);
            // SAFETY: its last read has completed; nothing refers to it any more.
            unsafe { drop(Box::from_raw(p)) };
            return;
        }
        if !ok && err != ERROR_NOTIFY_ENUM_DIR {
            return self.lose(p, std::io::Error::from_raw_os_error(err as i32).to_string());
        }
        // Success (records, or zero bytes for an overflow) or an overflow error: changed.
        if w.due.is_none() {
            w.due = Some(Instant::now() + COALESCE);
        }
        // SAFETY: the read just completed, so none is pending.
        if let Err(why) = unsafe { arm(w) } {
            self.lose(p, why);
        }
    }

    /// The live watch at `p` failed: close it, free it (no read is pending) and say so.
    fn lose(&mut self, p: *mut Watch, error: String) {
        // SAFETY: a live watch with no read pending; taken out of its slot and freed here.
        let w = unsafe { Box::from_raw(p) };
        if self.slots.get(&w.pane) == Some(&p) {
            self.slots.remove(&w.pane);
        }
        // SAFETY: the handle is open and nothing else uses it.
        unsafe { CloseHandle(w.handle) };
        eprintln!("[watch] pane={} tab={} lost {}: {error}", w.pane, w.tab_id, w.path.display());
        if self.events.send(Event::Lost { pane: w.pane, tab_id: w.tab_id, error }).is_ok() {
            (self.wake)();
        }
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
                sent |= self.events.send(Event::Changed { pane: w.pane, tab_id: w.tab_id }).is_ok();
            }
        }
        if sent {
            (self.wake)();
        }
    }

    /// Quit: cancel every watch and wait (bounded) for the cancelled reads, so no buffer is freed
    /// while the kernel may still write into it.
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
                self.completed(ov as *mut Watch, ok, 0);
            }
        }
    }
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

    #[test]
    fn create_file_gives_one_changed() {
        let dir = Scratch::new("create");
        let (w, rx) = watcher();
        w.watch(0, 7, dir.0.clone());
        w.sync();
        std::fs::write(dir.0.join("a.txt"), b"x").unwrap();
        let first = rx.recv_timeout(Duration::from_secs(2)).expect("a Changed within 2 s");
        assert_eq!(first, Event::Changed { pane: 0, tab_id: 7 });
        assert!(collect(&rx, Duration::from_millis(800)).is_empty(), "one create, one event");
    }

    #[test]
    fn bursts_are_coalesced() {
        let dir = Scratch::new("burst");
        let (w, rx) = watcher();
        w.watch(1, 3, dir.0.clone());
        w.sync();
        for i in 0..50 {
            let f = dir.0.join(format!("f{i}.txt"));
            std::fs::write(&f, b"x").unwrap();
            std::fs::remove_file(&f).unwrap();
        }
        let events = collect(&rx, Duration::from_secs(2));
        assert!(!events.is_empty() && events.len() < 5, "50 creates and deletes gave {} events", events.len());
        assert!(events.iter().all(|e| *e == Event::Changed { pane: 1, tab_id: 3 }));
    }

    #[test]
    fn rename_and_attribute_change_are_changes() {
        let dir = Scratch::new("rename");
        let f = dir.0.join("a.txt");
        std::fs::write(&f, b"x").unwrap();
        let (w, rx) = watcher();
        w.watch(0, 1, dir.0.clone());
        w.sync();
        std::fs::rename(&f, dir.0.join("b.txt")).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)), Ok(Event::Changed { pane: 0, tab_id: 1 }), "rename");
        assert!(collect(&rx, Duration::from_millis(400)).is_empty());
        let mut perms = std::fs::metadata(dir.0.join("b.txt")).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(dir.0.join("b.txt"), perms.clone()).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)), Ok(Event::Changed { pane: 0, tab_id: 1 }), "attributes");
        perms.set_readonly(false);
        std::fs::set_permissions(dir.0.join("b.txt"), perms).unwrap();
    }

    #[test]
    fn unwatch_stops_events() {
        let dir = Scratch::new("unwatch");
        let (w, rx) = watcher();
        w.watch(0, 1, dir.0.clone());
        w.sync();
        w.unwatch(0);
        w.sync();
        std::fs::write(dir.0.join("a.txt"), b"x").unwrap();
        assert!(collect(&rx, Duration::from_millis(500)).is_empty());
    }

    #[test]
    fn replacing_a_watch_moves_it() {
        let (a, b) = (Scratch::new("replace-a"), Scratch::new("replace-b"));
        let (w, rx) = watcher();
        w.watch(0, 1, a.0.clone());
        w.sync();
        w.watch(0, 2, b.0.clone());
        w.sync();
        std::fs::write(a.0.join("old.txt"), b"x").unwrap();
        assert!(collect(&rx, Duration::from_millis(500)).is_empty(), "the old folder is no longer watched");
        std::fs::write(b.0.join("new.txt"), b"x").unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)), Ok(Event::Changed { pane: 0, tab_id: 2 }));
    }

    #[test]
    fn missing_folder_gets_no_watch() {
        let dir = Scratch::new("missing");
        let gone = dir.0.join("not-there");
        assert!(open_dir(&gone).is_err());
        assert_eq!(unwatchable(Path::new("This PC")), Some("the drive list is not a folder"));
        assert_eq!(unwatchable(Path::new("")), Some("the drive list is not a folder"));
        assert_eq!(unwatchable(Path::new(r"\\server\share\dir")), Some("network share"));
        assert_eq!(unwatchable(&dir.0), None);
        let (w, rx) = watcher();
        w.watch(0, 1, gone);
        w.sync();
        std::fs::write(dir.0.join("a.txt"), b"x").unwrap();
        assert!(collect(&rx, Duration::from_millis(500)).is_empty());
        // The watcher still works afterwards.
        w.watch(0, 2, dir.0.clone());
        w.sync();
        std::fs::write(dir.0.join("b.txt"), b"x").unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)), Ok(Event::Changed { pane: 0, tab_id: 2 }));
    }

    #[test]
    fn deleting_the_folder_loses_the_watch() {
        let dir = Scratch::new("deleted");
        let inner = dir.0.join("watched");
        std::fs::create_dir(&inner).unwrap();
        std::fs::write(inner.join("a.txt"), b"x").unwrap();
        let (w, rx) = watcher();
        w.watch(1, 9, inner.clone());
        w.sync();
        std::fs::remove_dir_all(&inner).unwrap();
        let events = collect(&rx, Duration::from_secs(2));
        assert!(
            events.iter().any(|e| matches!(e, Event::Lost { pane: 1, tab_id: 9, .. })),
            "expected Lost, got {events:?}"
        );
    }

    #[test]
    fn drop_stops_the_thread() {
        let dir = Scratch::new("drop");
        let (w, _rx) = watcher();
        w.watch(0, 1, dir.0.clone());
        w.watch(1, 2, dir.0.clone());
        w.sync();
        let (done_tx, done_rx) = unbounded();
        std::thread::spawn(move || {
            drop(w); // returns once the dirwatch thread has finished
            let _ = done_tx.send(());
        });
        assert!(done_rx.recv_timeout(Duration::from_secs(3)).is_ok(), "the watcher thread did not exit");
    }
}
