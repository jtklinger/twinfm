//! File operations through `IFileOperation`, so progress, conflicts, undo, elevation and the
//! Recycle Bin behave as they do in Explorer.
//!
//! One worker thread owns every `IFileOperation`. It is a single-threaded apartment
//! (`COINIT_APARTMENTTHREADED`) running a real `GetMessage`/`DispatchMessage` loop, because the
//! shell's progress, conflict and confirmation dialogs are created on the calling thread and
//! need a pump there. Jobs arrive over a channel; each send is followed by a `PostThreadMessage`
//! nudge (`WM_RUN`). A job runs synchronously on the worker (`PerformOperations` pumps its own
//! dialogs), then the result goes back through the `done` callback, which the app turns into an
//! `invoke_from_event_loop`. The UI thread never waits on a job.
//!
//! A nudge posted while a job's dialog is up is eaten by that dialog's modal loop (it dispatches
//! thread messages to no window). That loses no job: the send happened before the post, and the
//! worker drains the channel until it is empty after every job, so the request is found then.
//!
//! Each job advises an `IFileOperationProgressSink` of ours (`Sink`, made on the worker and
//! released there) for its `PerformOperations`. Its Post* methods record, per item, the source,
//! the item the shell actually created (the real "New folder (2)", the renamed or moved item's
//! new path, a copy's path) and the HRESULT; they come back in `Done::results`. Its
//! `UpdateProgress` feeds the `progress` callback, through `ProgressGate`.
//!
//! A restore from the Recycle Bin (`Job::Restore`, the undo of a recycle) is not an
//! `IFileOperation` job: each item is restored by the bin's own "undelete" verb, which keeps the
//! bin's bookkeeping, and moved back from `$Recycle.Bin` only when the verb cannot be reached.

pub mod clipboard;
pub mod archive;
pub mod undo;

use crossbeam_channel::{Receiver, Sender, unbounded};
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{E_FAIL, E_INVALIDARG, ERROR_ALREADY_EXISTS, ERROR_CANCELLED, ERROR_TIMEOUT, HWND};
use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL};
use windows_sys::Win32::System::Com::{CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Shell::{
    BHID_EnumItems, BHID_SFUIObject, CMF_NORMAL, CMINVOKECOMMANDINFOEX, COPYENGINE_E_USER_CANCELLED, FOF_ALLOWUNDO,
    FOF_NOCONFIRMATION, FOF_NOCONFIRMMKDIR, FOF_RENAMEONCOLLISION, FOF_WANTNUKEWARNING, FOFX_ADDUNDORECORD,
    FOFX_SHOWELEVATIONPROMPT, FOLDERID_RecycleBinFolder, FileOperation, KF_FLAG_DEFAULT, SHCreateItemFromParsingName,
    SHGetKnownFolderItem,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, DestroyMenu, DispatchMessageW, GetMessageW, HMENU, MSG, PM_NOREMOVE, PM_REMOVE, PeekMessageW,
    PostThreadMessageW, SW_SHOWNORMAL, TranslateMessage, WM_APP, WM_QUIT,
};
use windows_sys::core::{BOOL, GUID, HRESULT};

/// Thread message that tells the worker the channel has requests.
const WM_RUN: u32 = WM_APP + 1;

/// HRESULT_FROM_WIN32(ERROR_CANCELLED): what some shell paths return when the user cancels.
pub const HR_CANCELLED: HRESULT = (0x8007_0000u32 | ERROR_CANCELLED) as HRESULT;

/// HRESULT_FROM_WIN32(ERROR_TIMEOUT): the "undelete" verb returned but the item never came back.
const HR_TIMEOUT: HRESULT = (0x8007_0000u32 | ERROR_TIMEOUT) as HRESULT;

/// What New file names the file it makes (Explorer's "New > Text Document").
pub const NEW_FILE_NAME: &str = "New Text Document.txt";

/// HRESULT_FROM_WIN32(ERROR_ALREADY_EXISTS): a restore whose original path is taken.
const HR_IN_THE_WAY: HRESULT = (0x8007_0000u32 | ERROR_ALREADY_EXISTS) as HRESULT;

/// How long a restore waits for the "undelete" verb to put an item back.
const UNDELETE_WAIT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Job {
    Copy { sources: Vec<PathBuf>, dest_dir: PathBuf },
    Move { sources: Vec<PathBuf>, dest_dir: PathBuf },
    /// Move each item into `dest_dir` under the given name (`MoveItem` with `pszNewName`).
    MoveAs { items: Vec<(PathBuf, String)>, dest_dir: PathBuf },
    Delete { sources: Vec<PathBuf>, permanent: bool },
    Rename { source: PathBuf, new_name: String },
    NewFolder { parent: PathBuf, name: String },
    /// An empty file (P6-M3: the toolbar's New file and Ctrl+Alt+N, "New Text Document.txt").
    NewFile { parent: PathBuf, name: String },
    /// Restore recycled items: (the `$R` path in the bin, the original full path).
    Restore { items: Vec<(PathBuf, PathBuf)> },
    /// A new 7z or TAR archive of the items in `dest_dir` (Compress to, P7-M3), named after the
    /// first item by the worker.
    Compress { sources: Vec<PathBuf>, dest_dir: PathBuf, format: crate::fs::archivewrite::Format },
}

impl Job {
    /// Items the job acts on.
    pub fn count(&self) -> usize {
        match self {
            Job::Copy { sources, .. } | Job::Move { sources, .. } | Job::Delete { sources, .. } | Job::Compress { sources, .. } => sources.len(),
            Job::MoveAs { items, .. } => items.len(),
            Job::Restore { items } => items.len(),
            Job::Rename { .. } | Job::NewFolder { .. } | Job::NewFile { .. } => 1,
        }
    }

    /// Every path the job reads or writes, for the soak guard. A restore names the originals
    /// only: the `$R` paths are the bin's.
    pub fn paths(&self) -> Vec<&Path> {
        match self {
            Job::Copy { sources, dest_dir } | Job::Move { sources, dest_dir } | Job::Compress { sources, dest_dir, .. } => {
                sources.iter().map(PathBuf::as_path).chain([dest_dir.as_path()]).collect()
            }
            Job::MoveAs { items, dest_dir } => items.iter().map(|(p, _)| p.as_path()).chain([dest_dir.as_path()]).collect(),
            Job::Delete { sources, .. } => sources.iter().map(PathBuf::as_path).collect(),
            Job::Restore { items } => items.iter().map(|(_, original)| original.as_path()).collect(),
            Job::Rename { source, .. } => vec![source.as_path()],
            Job::NewFolder { parent, .. } | Job::NewFile { parent, .. } => vec![parent.as_path()],
        }
    }
}

pub struct Request {
    pub id: u64,
    pub job: Job,
    /// Owner window for the shell's dialogs (0: none). An HWND is an integer; it crosses threads.
    pub hwnd: isize,
    /// Add `FOF_NOCONFIRMATION` (the soak run and the integration tests; never the default).
    pub no_confirm: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Done,
    /// The user cancelled a dialog, or skipped an item (`GetAnyOperationsAborted`).
    Cancelled,
    Failed(HRESULT),
    /// TwinFM's own archive writers failed, or the job was refused on the worker (P7-M3).
    Error(String),
}

/// One item as the progress sink saw it finish.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemResult {
    /// The item acted on (for a new folder: the folder it was made in).
    pub source: PathBuf,
    /// The item the shell created or renamed to (the new name, the copy, the moved item), if it
    /// reported one with a file system path.
    pub dest: Option<PathBuf>,
    pub hr: HRESULT,
}

impl ItemResult {
    pub fn ok(&self) -> bool {
        self.hr >= 0
    }
}

pub struct Done {
    pub id: u64,
    pub job: Job,
    pub outcome: Outcome,
    /// The job's own items as the sink reported them, in the order the shell finished them.
    pub results: Vec<ItemResult>,
    /// A copy's or move's destination paths that were there before it ran (P11-M3): an item that
    /// landed on one replaced that file (`undo::forbid_if_replaced`).
    pub existed: Vec<PathBuf>,
}

/// The destination paths a copy or move would land on that are there already (P11-M3), looked at
/// on the worker just before the job runs.
fn existing_dests(job: &Job) -> Vec<PathBuf> {
    let (dest_dir, names): (&Path, Vec<std::ffi::OsString>) = match job {
        Job::Copy { sources, dest_dir } | Job::Move { sources, dest_dir } => (dest_dir, sources.iter().filter_map(|s| s.file_name().map(|n| n.to_os_string())).collect()),
        Job::MoveAs { items, dest_dir } => (dest_dir, items.iter().map(|(_, n)| n.into()).collect()),
        _ => return Vec::new(),
    };
    // In their long form, as the sink names the items it made: a folder reached through an 8.3 name
    // (`C:\Users\RUNNER~1`) would otherwise never match.
    names.into_iter().map(|n| dest_dir.join(n)).filter(|p| std::fs::symlink_metadata(p).is_ok()).map(|p| long_path(&p)).collect()
}

/// `path` with every 8.3 component in its long form (`GetLongPathNameW`, which follows no link);
/// as it is when that fails.
fn long_path(path: &Path) -> PathBuf {
    use std::os::windows::ffi::OsStringExt;
    let w = wide(path.as_os_str());
    let mut buf = vec![0u16; 1024];
    // SAFETY: NUL-terminated input; `buf` holds its length in u16s.
    let n = unsafe { windows_sys::Win32::Storage::FileSystem::GetLongPathNameW(w.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
    if n == 0 || n >= buf.len() {
        return path.to_path_buf();
    }
    PathBuf::from(std::ffi::OsString::from_wide(&buf[..n]))
}

/// The name the shell really gave a renamed item, a new folder or a new file ("New folder (2)"),
/// if the sink reported it; None for the other jobs.
pub fn real_name(job: &Job, results: &[ItemResult]) -> Option<String> {
    match job {
        Job::Rename { .. } | Job::NewFolder { .. } | Job::NewFile { .. } | Job::Compress { .. } => results
            .iter()
            .filter(|r| r.ok())
            .find_map(|r| r.dest.as_ref()?.file_name().map(|n| n.to_string_lossy().into_owned())),
        _ => None,
    }
}

/// Same folder, ignoring case and a trailing separator.
pub fn same_dir(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().trim_end_matches('\\').to_lowercase();
    norm(a) == norm(b)
}

/// `SetOperationFlags` for a job. A permanent delete drops `FOF_ALLOWUNDO` (which is what
/// recycles) and asks for the shell's own "permanently delete?" confirmation; `no_confirm`
/// suppresses every confirmation, that one included. A copy into the sources' own folder and a
/// new folder over an existing name get `FOF_RENAMEONCOLLISION` ("a - Copy.txt",
/// "New folder (2)", as Explorer does); without it the shell stops at an "Interrupted Action"
/// window, even with `FOF_NOCONFIRMATION`.
pub fn flags(job: &Job, no_confirm: bool) -> u32 {
    let mut f = FOF_ALLOWUNDO | FOFX_ADDUNDORECORD | FOF_NOCONFIRMMKDIR | FOFX_SHOWELEVATIONPROMPT;
    match job {
        Job::Delete { permanent: true, .. } => {
            f &= !FOF_ALLOWUNDO;
            if !no_confirm {
                f |= FOF_WANTNUKEWARNING;
            }
        }
        Job::Copy { sources, dest_dir }
            if !sources.is_empty() && sources.iter().all(|s| s.parent().is_some_and(|d| same_dir(d, dest_dir))) =>
        {
            f |= FOF_RENAMEONCOLLISION;
        }
        Job::NewFolder { .. } | Job::NewFile { .. } => f |= FOF_RENAMEONCOLLISION,
        _ => {}
    }
    if no_confirm {
        f |= FOF_NOCONFIRMATION;
    }
    f
}

fn items(n: usize) -> String {
    if n == 1 { "1 item".into() } else { format!("{n} items") }
}

/// The status line after a job. A rename or a new folder names what the shell really made
/// (`real_name`), or the name asked for when the sink reported none.
pub fn status_text(job: &Job, outcome: &Outcome, results: &[ItemResult]) -> String {
    match outcome {
        Outcome::Cancelled => "Cancelled".into(),
        Outcome::Failed(hr) => format!("Failed: 0x{:08X}", *hr as u32),
        Outcome::Error(e) => format!("Failed: {e}"),
        Outcome::Done => match job {
            Job::Copy { .. } => format!("Copied {}", items(job.count())),
            Job::Move { .. } | Job::MoveAs { .. } => format!("Moved {}", items(job.count())),
            Job::Restore { .. } => format!("Restored {} from the Recycle Bin", items(job.count())),
            // Inside a zip nothing goes to the Recycle Bin (P7-M3): its results name no bin item.
            Job::Delete { sources, .. } if sources.first().is_some_and(|s| crate::fs::archive::within(s)) => {
                format!("Deleted {} from the archive", items(job.count()))
            }
            Job::Delete { permanent: false, .. } => format!("Deleted {} to the Recycle Bin", items(job.count())),
            Job::Delete { permanent: true, .. } => format!("Deleted {} permanently", items(job.count())),
            Job::Rename { new_name, .. } => format!("Renamed to {}", real_name(job, results).unwrap_or_else(|| new_name.clone())),
            Job::NewFolder { name, .. } | Job::NewFile { name, .. } => {
                format!("Created {}", real_name(job, results).unwrap_or_else(|| name.clone()))
            }
            Job::Compress { .. } => format!("Compressed {} to {}", items(job.count()), real_name(job, results).unwrap_or_default()),
        },
    }
}

/// The status line while a job runs.
pub fn running_text(job: &Job) -> String {
    match job {
        Job::Copy { .. } => format!("copying {}...", items(job.count())),
        Job::Move { .. } | Job::MoveAs { .. } => format!("moving {}...", items(job.count())),
        Job::Restore { .. } => format!("restoring {}...", items(job.count())),
        Job::Delete { .. } => format!("deleting {}...", items(job.count())),
        Job::Rename { .. } => "renaming...".into(),
        Job::NewFolder { .. } => "creating folder...".into(),
        Job::NewFile { .. } => "creating file...".into(),
        Job::Compress { format, .. } => format!("compressing {} to {}...", items(job.count()), format.ext()),
    }
}

/// Why a typed name cannot be a file name, or None. `<>:"/\|?*` and control characters are
/// refused by the file system; a name of dots or with trailing dots or spaces is mangled.
pub fn invalid_name(name: &str) -> Option<&'static str> {
    if name.trim().is_empty() {
        Some("the name is empty")
    } else if name.chars().any(|c| "<>:\"/\\|?*".contains(c) || (c as u32) < 32) {
        Some("a name cannot contain < > : \" / \\ | ? *")
    } else if name.ends_with('.') || name.ends_with(' ') {
        Some("a name cannot end with a dot or a space")
    } else {
        None
    }
}

/// A running job's progress, as the sink's `UpdateProgress` reported it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub id: u64,
    pub percent: u32,
}

/// `sofar` of `total` as a percentage, None for a zero total; clamped to 100.
pub fn percent(total: u32, sofar: u32) -> Option<u32> {
    (total != 0).then(|| (sofar as u64 * 100 / total as u64).min(100) as u32)
}

/// The least time between two progress reports.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

/// The worker-side throttle: at most one report per 250 ms, and only when the percent changed;
/// a change to 100 always goes through.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProgressGate {
    last: Option<(Instant, u32)>,
}

impl ProgressGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `percent` at `now` is reported (and, if so, remembered as the last report).
    pub fn due(&mut self, now: Instant, percent: u32) -> bool {
        let due = match self.last {
            None => true,
            Some((_, p)) if p == percent => false,
            Some((t, _)) => percent == 100 || now.duration_since(t) >= PROGRESS_EVERY,
        };
        if due {
            self.last = Some((now, percent));
        }
        due
    }
}

/// The shell's dialogs of this process (class `#32770`: a password prompt, a conflict, an error)
/// that are visible but not the foreground window: (window, title). The shell makes them on threads
/// of its own, owned by its unowned progress window, and does not bring them forward, so behind
/// TwinFM's window a job waits on a dialog nobody sees (P7-M3's console check: an encrypted zip's
/// "Password needed").
pub fn hidden_dialogs() -> Vec<(isize, String)> {
    use windows_sys::Win32::Foundation::LPARAM;
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible};
    unsafe extern "system" fn each(h: HWND, l: LPARAM) -> BOOL {
        // SAFETY: `l` is the Vec passed below, alive for EnumWindows; plain queries on `h`.
        unsafe {
            let out = &mut *(l as *mut Vec<(isize, String)>);
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, &mut pid);
            if pid != GetCurrentProcessId() || IsWindowVisible(h) == 0 || h == GetForegroundWindow() {
                return 1;
            }
            let mut class = [0u16; 16];
            let n = GetClassNameW(h, class.as_mut_ptr(), class.len() as i32).max(0) as usize;
            if String::from_utf16_lossy(&class[..n]) == "#32770" {
                let mut title = [0u16; 128];
                let n = GetWindowTextW(h, title.as_mut_ptr(), title.len() as i32).max(0) as usize;
                out.push((h as isize, String::from_utf16_lossy(&title[..n])));
            }
            1
        }
    }
    let mut out: Vec<(isize, String)> = Vec::new();
    // SAFETY: the callback only reads windows and pushes to `out`, which outlives the call.
    unsafe { EnumWindows(Some(each), &mut out as *mut _ as LPARAM) };
    out
}

/// Handle to the worker. Dropping it ends the worker's message loop.
pub struct Worker {
    tid: u32,
    tx: Sender<Request>,
    /// Jobs with an id up to this one that have not started yet are cancelled unrun (Escape).
    cancel_upto: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl Worker {
    /// Start the worker. `done` is called on the worker thread after each job, `progress` on the
    /// worker thread while one runs (throttled by `ProgressGate`).
    pub fn spawn(done: impl Fn(Done) + Send + 'static, progress: impl Fn(Progress) + Send + 'static) -> Worker {
        let (tx, rx) = unbounded::<Request>();
        let (tid_tx, tid_rx) = crossbeam_channel::bounded(1);
        let cancel_upto = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let cancel = cancel_upto.clone();
        std::thread::Builder::new()
            .name("fileops".into())
            .spawn(move || {
                // SAFETY: plain Win32 calls on this thread. PeekMessage creates the thread's
                // message queue before its id is published, so no nudge can be lost to a
                // missing queue.
                let tid = unsafe {
                    let mut msg: MSG = std::mem::zeroed();
                    PeekMessageW(&mut msg, std::ptr::null_mut(), WM_APP, WM_APP, PM_NOREMOVE);
                    GetCurrentThreadId()
                };
                let _ = tid_tx.send(tid);
                run_loop(rx, &cancel, done, Rc::new(progress));
            })
            .expect("spawn fileops thread");
        let tid = tid_rx.recv().unwrap_or(0);
        Worker { tid, tx, cancel_upto }
    }

    /// Queue a job. False if the worker is gone.
    pub fn submit(&self, r: Request) -> bool {
        if self.tx.send(r).is_err() {
            return false;
        }
        // SAFETY: posting to a thread id; fails harmlessly if the thread has exited.
        unsafe { PostThreadMessageW(self.tid, WM_RUN, 0, 0) != 0 }
    }

    /// Cancel every job with an id up to `id` that has not started: the worker reports each as
    /// `Cancelled` without running it. A job already running is not touched (its own dialog
    /// cancels it).
    pub fn cancel_queued(&self, id: u64) {
        self.cancel_upto.fetch_max(id, std::sync::atomic::Ordering::AcqRel);
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // SAFETY: as in `submit`.
        unsafe { PostThreadMessageW(self.tid, WM_QUIT, 0, 0) };
    }
}

fn run_loop(rx: Receiver<Request>, cancel_upto: &std::sync::atomic::AtomicU64, done: impl Fn(Done), progress: Rc<dyn Fn(Progress)>) {
    // SAFETY: initialises COM for this thread as an STA; balanced by CoUninitialize below.
    let com = unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
    // SAFETY: a zeroed MSG is a valid out-buffer for GetMessageW.
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    loop {
        // SAFETY: standard message loop on this thread's queue; 0 is WM_QUIT, -1 an error.
        let r = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
        if r == 0 || r == -1 {
            break;
        }
        if msg.hwnd.is_null() && msg.message == WM_RUN {
            while let Ok(req) = rx.try_recv() {
                let existed = existing_dests(&req.job);
                let (outcome, results) = if req.id <= cancel_upto.load(std::sync::atomic::Ordering::Acquire) {
                    (Outcome::Cancelled, Vec::new()) // cancelled while queued: never started
                } else if com >= 0 {
                    perform(&req, &progress)
                } else {
                    (Outcome::Failed(com), Vec::new())
                };
                done(Done { id: req.id, job: req.job, outcome, results, existed });
            }
            continue;
        }
        // SAFETY: `msg` came from GetMessageW.
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    if com >= 0 {
        // SAFETY: pairs the successful CoInitializeEx above, on the same thread.
        unsafe { CoUninitialize() };
    }
}

const IID_IFILEOPERATION: GUID = GUID::from_u128(0x947aab5f_0a5c_4c13_b4d6_4bf7836fc9f8);
const IID_ISHELLITEM: GUID = GUID::from_u128(0x43826d1e_e718_42ee_bc55_a1e261c37bfe);

type Slot = *const c_void;

#[repr(C)]
struct UnknownVtbl {
    _query_interface: Slot,
    _add_ref: Slot,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
}

/// IFileOperation's vtable in declaration order (shobjidl_core.h); unused slots are opaque.
#[repr(C)]
struct FileOperationVtbl {
    base: UnknownVtbl,
    advise: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut u32) -> HRESULT,
    unadvise: unsafe extern "system" fn(*mut c_void, u32) -> HRESULT,
    set_operation_flags: unsafe extern "system" fn(*mut c_void, u32) -> HRESULT,
    _set_progress_message: Slot,
    _set_progress_dialog: Slot,
    _set_properties: Slot,
    set_owner_window: unsafe extern "system" fn(*mut c_void, HWND) -> HRESULT,
    _apply_properties_to_item: Slot,
    _apply_properties_to_items: Slot,
    rename_item: unsafe extern "system" fn(*mut c_void, *mut c_void, *const u16, *mut c_void) -> HRESULT,
    _rename_items: Slot,
    move_item: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void, *const u16, *mut c_void) -> HRESULT,
    _move_items: Slot,
    copy_item: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void, *const u16, *mut c_void) -> HRESULT,
    _copy_items: Slot,
    delete_item: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void) -> HRESULT,
    _delete_items: Slot,
    new_item: unsafe extern "system" fn(*mut c_void, *mut c_void, u32, *const u16, *const u16, *mut c_void) -> HRESULT,
    perform_operations: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    get_any_operations_aborted: unsafe extern "system" fn(*mut c_void, *mut BOOL) -> HRESULT,
}

/// An owned COM interface pointer, released on drop (on the thread that made it).
struct Com(*mut c_void);

impl Drop for Com {
    fn drop(&mut self) {
        // SAFETY: a live interface pointer we own one reference to; every COM vtable starts
        // with IUnknown.
        unsafe { ((**(self.0 as *const *const UnknownVtbl)).release)(self.0) };
    }
}

fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain([0]).collect()
}

/// Copy the item at `source` into `dest_dir` through the shell with no progress window and no
/// questions (P8-L3: an archive inside an archive of a format the shell lists, extracted to its
/// copy on the archive worker). On an STA thread; blocks until done.
pub(crate) fn copy_quietly(source: &Path, dest_dir: &Path) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::{FOF_NOERRORUI, FOF_SILENT};
    let mut p: *mut c_void = std::ptr::null_mut();
    // SAFETY: standard in-process COM activation; `p` receives an IFileOperation or stays null.
    let hr = unsafe { CoCreateInstance(&FileOperation, std::ptr::null_mut(), CLSCTX_ALL, &IID_IFILEOPERATION, &mut p) };
    if hr < 0 || p.is_null() {
        return Err(format!("the shell's copy engine is not available (0x{:08X})", hr as u32));
    }
    let op = Com(p);
    // SAFETY: `op` is a live IFileOperation; its vtable matches `FileOperationVtbl`.
    let v = unsafe { &**(op.0 as *const *const FileOperationVtbl) };
    let fail = |what: &str, hr: HRESULT| format!("{what} failed (0x{:08X})", hr as u32);
    let item = shell_item(source).map_err(|hr| fail("opening the archive inside", hr))?;
    let dest = shell_item(dest_dir).map_err(|hr| fail("opening the copy's folder", hr))?;
    // SAFETY (each call): live interfaces owned by this frame.
    let hr = unsafe { (v.set_operation_flags)(op.0, FOF_SILENT | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_NOCONFIRMMKDIR) };
    if hr < 0 {
        return Err(fail("SetOperationFlags", hr));
    }
    let hr = unsafe { (v.copy_item)(op.0, item.0, dest.0, std::ptr::null(), std::ptr::null_mut()) };
    if hr < 0 {
        return Err(fail("CopyItem", hr));
    }
    let hr = unsafe { (v.perform_operations)(op.0) };
    drop((item, dest));
    if hr < 0 { Err(fail("extracting the archive inside", hr)) } else { Ok(()) }
}

/// The shell item for a path. A path inside a zip archive (`C:\dir\x.zip\sub\file.txt`) is parsed
/// by the shell's zip folder, so a copy out of it is an extraction.
fn shell_item(path: &Path) -> Result<Com, HRESULT> {
    // Inside an archive inside an archive: the inner archive's copy (P8-L3).
    let path = &crate::fs::archive::real(path).unwrap_or_else(|| path.to_path_buf());
    let w = wide(path.as_os_str());
    let mut p: *mut c_void = std::ptr::null_mut();
    // SAFETY: `w` is NUL-terminated and outlives the call; `p` receives an IShellItem or stays null.
    let hr = unsafe { SHCreateItemFromParsingName(w.as_ptr(), std::ptr::null_mut(), &IID_ISHELLITEM, &mut p) };
    let res = if hr < 0 || p.is_null() { Err(if hr < 0 { hr } else { -1 }) } else { Ok(Com(p)) };
    if crate::fs::archive::inside(path) {
        match &res {
            Ok(_) => eprintln!("[archive] parse \"{}\" -> item ok", path.display()),
            Err(hr) => eprintln!("[archive] parse \"{}\" -> item failed 0x{:08X}", path.display(), *hr as u32),
        }
    }
    res
}

fn check(hr: HRESULT) -> Result<(), HRESULT> {
    if hr < 0 { Err(hr) } else { Ok(()) }
}

// --- The progress sink ------------------------------------------------------------------------

const IID_IUNKNOWN: GUID = GUID::from_u128(0x00000000_0000_0000_c000_000000000046);
const IID_IFILEOPERATIONPROGRESSSINK: GUID = GUID::from_u128(0x04b0f1a7_9490_44bc_96e1_4296a31252e2);

/// SIGDN_FILESYSPATH, and SIGDN_DESKTOPABSOLUTEPARSING for an item that has no file system path
/// (inside a zip archive: `C:\dir\x.zip\sub\file.txt`).
const SIGDN_FILESYSPATH: i32 = 0x8005_8000u32 as i32;
const SIGDN_DESKTOPABSOLUTEPARSING: i32 = 0x8002_8000u32 as i32;

type Item = *mut c_void;
type Wstr = *const u16;

/// IFileOperationProgressSink in declaration order (shobjidl_core.h).
#[repr(C)]
struct SinkVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    start_operations: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    finish_operations: unsafe extern "system" fn(*mut c_void, HRESULT) -> HRESULT,
    pre_rename_item: unsafe extern "system" fn(*mut c_void, u32, Item, Wstr) -> HRESULT,
    post_rename_item: unsafe extern "system" fn(*mut c_void, u32, Item, Wstr, HRESULT, Item) -> HRESULT,
    pre_move_item: unsafe extern "system" fn(*mut c_void, u32, Item, Item, Wstr) -> HRESULT,
    post_move_item: unsafe extern "system" fn(*mut c_void, u32, Item, Item, Wstr, HRESULT, Item) -> HRESULT,
    pre_copy_item: unsafe extern "system" fn(*mut c_void, u32, Item, Item, Wstr) -> HRESULT,
    post_copy_item: unsafe extern "system" fn(*mut c_void, u32, Item, Item, Wstr, HRESULT, Item) -> HRESULT,
    pre_delete_item: unsafe extern "system" fn(*mut c_void, u32, Item) -> HRESULT,
    post_delete_item: unsafe extern "system" fn(*mut c_void, u32, Item, HRESULT, Item) -> HRESULT,
    pre_new_item: unsafe extern "system" fn(*mut c_void, u32, Item, Wstr) -> HRESULT,
    post_new_item: unsafe extern "system" fn(*mut c_void, u32, Item, Wstr, Wstr, u32, HRESULT, Item) -> HRESULT,
    update_progress: unsafe extern "system" fn(*mut c_void, u32, u32) -> HRESULT,
    reset_timer: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    pause_timer: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    resume_timer: unsafe extern "system" fn(*mut c_void) -> HRESULT,
}

/// IShellItem, up to the methods used.
#[repr(C)]
struct ShellItemVtbl {
    base: UnknownVtbl,
    bind_to_handler: unsafe extern "system" fn(*mut c_void, *mut c_void, *const GUID, *const GUID, *mut *mut c_void) -> HRESULT,
    _get_parent: Slot,
    get_display_name: unsafe extern "system" fn(*mut c_void, i32, *mut *mut u16) -> HRESULT,
}

/// Our sink: a `Box` with the vtable pointer first, a reference count, the results, and the
/// job's id with the progress callback and its gate. It is made, advised, called and released on
/// the worker thread only (an STA), so nothing needs a lock; the count is atomic only because
/// that is what COM objects conventionally do.
#[repr(C)]
struct Sink {
    vtbl: *const SinkVtbl,
    refs: std::sync::atomic::AtomicU32,
    results: std::cell::RefCell<Vec<ItemResult>>,
    id: u64,
    progress: Rc<dyn Fn(Progress)>,
    gate: std::cell::Cell<ProgressGate>,
}

static SINK_VTBL: SinkVtbl = SinkVtbl {
    query_interface: sink_query,
    add_ref: sink_add_ref,
    release: sink_release,
    start_operations: sink_ok0,
    finish_operations: sink_finish,
    pre_rename_item: sink_pre_rename,
    post_rename_item: sink_post_rename,
    pre_move_item: sink_pre_move_copy,
    post_move_item: sink_post_move_copy,
    pre_copy_item: sink_pre_move_copy,
    post_copy_item: sink_post_move_copy,
    pre_delete_item: sink_pre_delete,
    post_delete_item: sink_post_delete,
    pre_new_item: sink_pre_rename,
    post_new_item: sink_post_new,
    update_progress: sink_progress,
    reset_timer: sink_ok0,
    pause_timer: sink_ok0,
    resume_timer: sink_ok0,
};

fn same_guid(a: &GUID, b: &GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}

/// The file system path of a shell item we were handed (not owned), or its parsing path when it
/// has none (an item inside a zip archive), if either.
fn item_path(item: Item) -> Option<PathBuf> {
    if item.is_null() {
        return None;
    }
    let mut s: *mut u16 = std::ptr::null_mut();
    // SAFETY: `item` is a live IShellItem for the duration of the sink call; GetDisplayName
    // allocates `s` with the COM allocator, freed below.
    unsafe {
        let v = &**(item as *const *const ShellItemVtbl);
        if ((v.get_display_name)(item, SIGDN_FILESYSPATH, &mut s) < 0 || s.is_null())
            && ((v.get_display_name)(item, SIGDN_DESKTOPABSOLUTEPARSING, &mut s) < 0 || s.is_null())
        {
            return None;
        }
        let len = (0..).take_while(|&i| *s.add(i) != 0).count();
        let path = PathBuf::from(<std::ffi::OsString as std::os::windows::ffi::OsStringExt>::from_wide(std::slice::from_raw_parts(s, len)));
        windows_sys::Win32::System::Com::CoTaskMemFree(s as *const c_void);
        Some(path)
    }
}

/// Record one finished item on the sink.
unsafe fn record(this: *mut c_void, source: Option<PathBuf>, dest: Item, hr: HRESULT) {
    // SAFETY: `this` is our Sink (the shell only calls us through the pointer we advised).
    let sink = unsafe { &*(this as *const Sink) };
    sink.results.borrow_mut().push(ItemResult { source: source.unwrap_or_default(), dest: item_path(dest), hr });
}

unsafe extern "system" fn sink_query(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    // SAFETY: COM contract: `iid` and `out` are valid pointers.
    unsafe {
        if same_guid(&*iid, &IID_IUNKNOWN) || same_guid(&*iid, &IID_IFILEOPERATIONPROGRESSSINK) {
            sink_add_ref(this);
            *out = this;
            0
        } else {
            *out = std::ptr::null_mut();
            windows_sys::Win32::Foundation::E_NOINTERFACE
        }
    }
}
unsafe extern "system" fn sink_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is our Sink.
    unsafe { (*(this as *const Sink)).refs.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1 }
}
unsafe extern "system" fn sink_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is our Sink, a Box made in `perform_inner`; freed when the count reaches 0.
    unsafe {
        let n = (*(this as *const Sink)).refs.fetch_sub(1, std::sync::atomic::Ordering::AcqRel) - 1;
        if n == 0 {
            drop(Box::from_raw(this as *mut Sink));
        }
        n
    }
}
unsafe extern "system" fn sink_ok0(_: *mut c_void) -> HRESULT {
    0
}
unsafe extern "system" fn sink_finish(_: *mut c_void, _: HRESULT) -> HRESULT {
    0
}
unsafe extern "system" fn sink_pre_rename(_: *mut c_void, _: u32, _: Item, _: Wstr) -> HRESULT {
    0
}
unsafe extern "system" fn sink_pre_move_copy(_: *mut c_void, _: u32, _: Item, _: Item, _: Wstr) -> HRESULT {
    0
}
unsafe extern "system" fn sink_pre_delete(_: *mut c_void, _: u32, _: Item) -> HRESULT {
    0
}
unsafe extern "system" fn sink_progress(this: *mut c_void, total: u32, sofar: u32) -> HRESULT {
    // SAFETY: `this` is our Sink, called on the worker.
    let sink = unsafe { &*(this as *const Sink) };
    if let Some(percent) = percent(total, sofar) {
        let mut gate = sink.gate.get();
        let due = gate.due(Instant::now(), percent);
        sink.gate.set(gate);
        if due {
            (sink.progress)(Progress { id: sink.id, percent });
        }
    }
    0
}
unsafe extern "system" fn sink_post_rename(this: *mut c_void, _: u32, item: Item, _: Wstr, hr: HRESULT, created: Item) -> HRESULT {
    // SAFETY: called by the shell with live items, on the worker.
    unsafe { record(this, item_path(item), created, hr) };
    0
}
unsafe extern "system" fn sink_post_move_copy(
    this: *mut c_void,
    _: u32,
    item: Item,
    _dest_folder: Item,
    _: Wstr,
    hr: HRESULT,
    created: Item,
) -> HRESULT {
    // SAFETY: as above.
    unsafe { record(this, item_path(item), created, hr) };
    0
}
unsafe extern "system" fn sink_post_delete(this: *mut c_void, _: u32, item: Item, hr: HRESULT, created: Item) -> HRESULT {
    // SAFETY: as above. `created` is the item in the Recycle Bin for a recycle, null otherwise.
    unsafe { record(this, item_path(item), created, hr) };
    0
}
unsafe extern "system" fn sink_post_new(
    this: *mut c_void,
    _: u32,
    folder: Item,
    _: Wstr,
    _: Wstr,
    _: u32,
    hr: HRESULT,
    created: Item,
) -> HRESULT {
    // SAFETY: as above.
    unsafe { record(this, item_path(folder), created, hr) };
    0
}

/// `a` names the same file as `b`: equal ignoring case, or equal once both parents are
/// canonicalised (the shell hands back long paths; `%TEMP%` can be an 8.3 path).
fn same_path(a: &Path, b: &Path) -> bool {
    let lower = |p: &Path| p.to_string_lossy().trim_end_matches('\\').to_lowercase();
    if lower(a) == lower(b) {
        return true;
    }
    let canon = |p: &Path| -> Option<String> {
        let name = p.file_name()?.to_string_lossy().to_lowercase();
        let dir = std::fs::canonicalize(p.parent()?).ok()?;
        Some(format!("{}\\{name}", dir.to_string_lossy().trim_end_matches('\\').to_lowercase()))
    };
    matches!((canon(a), canon(b)), (Some(x), Some(y)) if x == y)
}

/// Keep the results for the job's own items: the shell also reports the contents of a copied,
/// moved or deleted folder one by one.
fn own_results(job: &Job, all: Vec<ItemResult>) -> Vec<ItemResult> {
    let mine: Vec<&Path> = match job {
        Job::Copy { sources, .. } | Job::Move { sources, .. } | Job::Delete { sources, .. } => {
            sources.iter().map(PathBuf::as_path).collect()
        }
        Job::MoveAs { items, .. } => items.iter().map(|(p, _)| p.as_path()).collect(),
        Job::Rename { source, .. } => vec![source.as_path()],
        Job::NewFolder { parent, .. } | Job::NewFile { parent, .. } => vec![parent.as_path()],
        Job::Restore { .. } | Job::Compress { .. } => return all, // one result per item, made by `restore` or `archive`
    };
    // Top-level items share a parent with a source; checking the parent first keeps the
    // canonicalising comparison off a folder's contents.
    let parents: Vec<&Path> = mine.iter().filter_map(|p| p.parent()).collect();
    let is_top = |r: &ItemResult| match job {
        Job::NewFolder { .. } | Job::NewFile { .. } => true,
        _ => r.source.parent().is_some_and(|rp| parents.iter().any(|p| same_dir(p, rp) || same_path(p, rp))),
    };
    all.into_iter().filter(|r| is_top(r) && mine.iter().any(|m| same_path(m, &r.source))).collect()
}

/// Run one job on this (STA, pumping) thread: TwinFM's own archive writers for a Compress job and
/// a zip's writes (`archive`), `IFileOperation` (or the restore) for the rest.
fn perform(req: &Request, progress: &Rc<dyn Fn(Progress)>) -> (Outcome, Vec<ItemResult>) {
    if let Job::Compress { sources, dest_dir, format } = &req.job {
        return archive::compress(req, sources, dest_dir, *format, progress);
    }
    match archive::classify(&req.job) {
        Err(why) => {
            eprintln!("[archive] refused on the worker: {why}");
            return (Outcome::Error(why), Vec::new());
        }
        Ok(Some(plan)) => return archive::run(req, plan, progress),
        Ok(None) => {}
    }
    let mut results = Vec::new();
    let outcome = match perform_inner(req, progress, &mut results) {
        Ok(false) => Outcome::Done,
        Ok(true) => Outcome::Cancelled,
        Err(hr) if hr == COPYENGINE_E_USER_CANCELLED || hr == HR_CANCELLED => Outcome::Cancelled,
        Err(hr) => Outcome::Failed(hr),
    };
    (outcome, own_results(&req.job, results))
}

/// Ok(aborted) or the first failing HRESULT; the sink's results go to `results` either way.
fn perform_inner(req: &Request, progress: &Rc<dyn Fn(Progress)>, results: &mut Vec<ItemResult>) -> Result<bool, HRESULT> {
    if let Job::Restore { items } = &req.job {
        return restore(req, items, progress, results);
    }
    let mut p: *mut c_void = std::ptr::null_mut();
    // SAFETY: standard in-process COM activation; `p` receives an IFileOperation or stays null.
    check(unsafe { CoCreateInstance(&FileOperation, std::ptr::null_mut(), CLSCTX_ALL, &IID_IFILEOPERATION, &mut p) })?;
    if p.is_null() {
        return Err(-1);
    }
    let op = Com(p);
    // SAFETY: `op` is a live IFileOperation; its vtable matches `FileOperationVtbl`.
    let v = unsafe { &**(op.0 as *const *const FileOperationVtbl) };
    // SAFETY (every call below): `op.0` and the item pointers are live interfaces owned by this
    // frame, strings are NUL-terminated wide buffers that outlive the call.
    check(unsafe { (v.set_operation_flags)(op.0, flags(&req.job, req.no_confirm)) })?;
    if req.hwnd != 0 {
        check(unsafe { (v.set_owner_window)(op.0, req.hwnd as HWND) })?;
    }
    // Items stay alive until after PerformOperations (the operation also holds its own references).
    let mut keep: Vec<Com> = Vec::new();
    match &req.job {
        Job::Copy { sources, dest_dir } | Job::Move { sources, dest_dir } => {
            let dest = shell_item(dest_dir)?;
            let is_move = matches!(req.job, Job::Move { .. });
            for s in sources {
                let item = shell_item(s)?;
                let f = if is_move { v.move_item } else { v.copy_item };
                check(unsafe { f(op.0, item.0, dest.0, std::ptr::null(), std::ptr::null_mut()) })?;
                keep.push(item);
            }
            keep.push(dest);
        }
        Job::MoveAs { items, dest_dir } => {
            let dest = shell_item(dest_dir)?;
            for (s, name) in items {
                let item = shell_item(s)?;
                let name = wide(std::ffi::OsStr::new(name));
                check(unsafe { (v.move_item)(op.0, item.0, dest.0, name.as_ptr(), std::ptr::null_mut()) })?;
                keep.push(item);
            }
            keep.push(dest);
        }
        Job::Delete { sources, .. } => {
            for s in sources {
                let item = shell_item(s)?;
                check(unsafe { (v.delete_item)(op.0, item.0, std::ptr::null_mut()) })?;
                keep.push(item);
            }
        }
        Job::Rename { source, new_name } => {
            let item = shell_item(source)?;
            let name = wide(std::ffi::OsStr::new(new_name));
            check(unsafe { (v.rename_item)(op.0, item.0, name.as_ptr(), std::ptr::null_mut()) })?;
            keep.push(item);
        }
        Job::NewFolder { parent, name } | Job::NewFile { parent, name } => {
            let dir = shell_item(parent)?;
            let attrs = if matches!(req.job, Job::NewFolder { .. }) { FILE_ATTRIBUTE_DIRECTORY } else { FILE_ATTRIBUTE_NORMAL };
            let name = wide(std::ffi::OsStr::new(name));
            // No template: the new file is empty.
            check(unsafe { (v.new_item)(op.0, dir.0, attrs, name.as_ptr(), std::ptr::null(), std::ptr::null_mut()) })?;
            keep.push(dir);
        }
        Job::Restore { .. } => unreachable!("restored above"),
        Job::Compress { .. } => unreachable!("compressed in `perform`"),
    }
    // The sink, advised for this one PerformOperations and unadvised right after. `sink` holds
    // our own reference; the operation takes its own in Advise and drops it in Unadvise (or
    // when it is released).
    let sink = Com(Box::into_raw(Box::new(Sink {
        vtbl: &SINK_VTBL,
        refs: std::sync::atomic::AtomicU32::new(1),
        results: std::cell::RefCell::new(Vec::new()),
        id: req.id,
        progress: progress.clone(),
        gate: std::cell::Cell::new(ProgressGate::new()),
    })) as *mut c_void);
    let mut cookie = 0u32;
    let advised = unsafe { (v.advise)(op.0, sink.0, &mut cookie) } >= 0;
    let performed = unsafe { (v.perform_operations)(op.0) };
    let mut aborted: BOOL = 0;
    let got = unsafe { (v.get_any_operations_aborted)(op.0, &mut aborted) };
    if advised {
        unsafe { (v.unadvise)(op.0, cookie) };
    }
    // SAFETY: `sink.0` is our live Sink (we still hold a reference).
    *results = std::mem::take(&mut *unsafe { &*(sink.0 as *const Sink) }.results.borrow_mut());
    drop(sink);
    drop(keep);
    if performed < 0 {
        return Err(performed);
    }
    Ok(got >= 0 && aborted != 0)
}

// --- Restore from the Recycle Bin ---------------------------------------------------------------

const IID_IENUMSHELLITEMS: GUID = GUID::from_u128(0x70629033_e363_4a28_a567_0db78006e6d7);
const IID_ICONTEXTMENU: GUID = GUID::from_u128(0x000214e4_0000_0000_c000_000000000046);

/// CMIC_MASK_FLAG_NO_UI (SEE_MASK_FLAG_NO_UI): no dialogs from the verb.
const CMIC_MASK_FLAG_NO_UI: u32 = 0x0000_0400;
/// CMIC_MASK_NOASYNC (SEE_MASK_NOASYNC): the handler finishes before InvokeCommand returns.
const CMIC_MASK_NOASYNC: u32 = 0x0000_0100;

/// IEnumShellItems, up to `Next`.
#[repr(C)]
struct EnumShellItemsVtbl {
    base: UnknownVtbl,
    next: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void, *mut u32) -> HRESULT,
}

/// IContextMenu, up to `InvokeCommand`.
#[repr(C)]
struct ContextMenuVtbl {
    base: UnknownVtbl,
    query_context_menu: unsafe extern "system" fn(*mut c_void, HMENU, u32, u32, u32, u32) -> HRESULT,
    invoke_command: unsafe extern "system" fn(*mut c_void, *const CMINVOKECOMMANDINFOEX) -> HRESULT,
}

/// Restore each item: the bin's "undelete" verb first, a move back from `$Recycle.Bin` when the
/// verb cannot be reached, fails or times out. One result per item reached (source: the `$R`
/// path, dest: the original); Ok(aborted) when every item came back, else the first failing
/// HRESULT. A `$R` path outside a `$Recycle.Bin` folder is refused, an original path already
/// taken is never moved onto, a cancelled verb is not followed by the move, and a `WM_QUIT` met
/// while waiting ends the job as cancelled.
fn restore(req: &Request, items: &[(PathBuf, PathBuf)], progress: &Rc<dyn Fn(Progress)>, results: &mut Vec<ItemResult>) -> Result<bool, HRESULT> {
    let found = bin_items(items);
    let mut aborted = false;
    let mut first_err = None;
    for (k, (r, original)) in items.iter().enumerate() {
        let t = Instant::now();
        let (rs, os) = (r.display(), original.display());
        let hr = if !in_recycle_bin(r) {
            eprintln!("[fileops] undelete {rs} -> {os}: not a Recycle Bin path");
            E_INVALIDARG
        } else {
            let verb = match &found {
                Err(why) => Err(NotRestored::Fallback(Err(why.clone()))),
                Ok(found) => match &found[k] {
                    Some(item) => undelete(req, item, r, original),
                    None => Err(NotRestored::Fallback(Err("not in the Recycle Bin".into()))),
                },
            };
            match verb {
                Ok(()) => {
                    eprintln!("[fileops] undelete {rs} -> {os}: verb ok ({} ms)", t.elapsed().as_millis());
                    0
                }
                Err(NotRestored::Quit) => {
                    eprintln!("[fileops] undelete {rs} -> {os}: quit while waiting: stopped");
                    return Ok(true);
                }
                Err(NotRestored::InTheWay(why)) => {
                    eprintln!("[fileops] undelete {rs} -> {os}: refused ({why})");
                    HR_IN_THE_WAY
                }
                Err(NotRestored::Fallback(Ok(hr))) if hr == HR_CANCELLED || hr == COPYENGINE_E_USER_CANCELLED => {
                    eprintln!("[fileops] undelete {rs} -> {os}: verb cancelled 0x{:08X}", hr as u32);
                    aborted = true;
                    hr
                }
                Err(NotRestored::Fallback(e)) => {
                    match e {
                        Ok(hr) => eprintln!("[fileops] undelete {rs} -> {os}: verb failed 0x{:08X}: moved back", hr as u32),
                        Err(why) => eprintln!("[fileops] undelete {rs} -> {os}: verb not reachable ({why}): moved back"),
                    }
                    let (hr, a) = move_back(req, r, original, progress);
                    if hr < 0 {
                        eprintln!("[fileops] undelete {rs} -> {os}: moved back failed 0x{:08X}", hr as u32);
                    }
                    aborted |= a;
                    hr
                }
            }
        };
        results.push(ItemResult { source: r.clone(), dest: Some(original.clone()), hr });
        if hr < 0 && first_err.is_none() {
            first_err = Some(hr);
        }
    }
    first_err.map_or(Ok(aborted), Err)
}

/// A `$R` path lies in a `$Recycle.Bin` folder: the one guard on what a restore moves.
fn in_recycle_bin(r: &Path) -> bool {
    r.to_string_lossy().to_lowercase().contains(r"\$recycle.bin\")
}

/// The `$I` file beside a `$R` item (its original path and deletion time).
fn info_file(r: &Path) -> Option<PathBuf> {
    let name = r.file_name()?.to_str()?;
    let rest = name.strip_prefix("$R").or_else(|| name.strip_prefix("$r"))?;
    Some(r.with_file_name(format!("$I{rest}")))
}

/// Why the "undelete" verb did not put an item back.
enum NotRestored {
    /// The verb failed or timed out (Ok(hr)) or could not be reached (Err(why)): the move back
    /// runs, unless the verb was cancelled.
    Fallback(Result<HRESULT, String>),
    /// The original path is taken: nothing is moved (`undo::check` refuses this first).
    InTheWay(String),
    /// A `WM_QUIT` came while waiting: the worker is stopping.
    Quit,
}

/// The bin's own items for the `$R` paths, in their order (None: not in the bin), from one pass
/// over the bin; Err when the bin cannot be listed.
fn bin_items(items: &[(PathBuf, PathBuf)]) -> Result<Vec<Option<Com>>, String> {
    let lower = |p: &Path| p.to_string_lossy().to_lowercase();
    let want: Vec<String> = items.iter().map(|(r, _)| lower(r)).collect();
    let mut found: Vec<Option<Com>> = items.iter().map(|_| None).collect();
    let mut p: *mut c_void = std::ptr::null_mut();
    // SAFETY: a known folder id and an out-pointer that receives an IShellItem or stays null.
    let hr = unsafe { SHGetKnownFolderItem(&FOLDERID_RecycleBinFolder, KF_FLAG_DEFAULT, std::ptr::null_mut(), &IID_ISHELLITEM, &mut p) };
    if hr < 0 || p.is_null() {
        return Err(format!("the Recycle Bin 0x{:08X}", hr as u32));
    }
    let bin = Com(p);
    let mut e: *mut c_void = std::ptr::null_mut();
    // SAFETY: `bin` is a live IShellItem; `e` receives an IEnumShellItems or stays null.
    let hr = unsafe { ((**(bin.0 as *const *const ShellItemVtbl)).bind_to_handler)(bin.0, std::ptr::null_mut(), &BHID_EnumItems, &IID_IENUMSHELLITEMS, &mut e) };
    if hr < 0 || e.is_null() {
        return Err(format!("listing the Recycle Bin 0x{:08X}", hr as u32));
    }
    let en = Com(e);
    while found.iter().any(Option::is_none) {
        let mut item: *mut c_void = std::ptr::null_mut();
        let mut n = 0u32;
        // SAFETY: `en` is a live IEnumShellItems; one item out, owned by us when fetched.
        let hr = unsafe { ((**(en.0 as *const *const EnumShellItemsVtbl)).next)(en.0, 1, &mut item, &mut n) };
        if hr != 0 || n == 0 || item.is_null() {
            break; // S_FALSE: the end
        }
        let item = Com(item);
        if let Some(k) = item_path(item.0).map(|p| lower(&p)).and_then(|p| want.iter().zip(&found).position(|(w, f)| f.is_none() && *w == p)) {
            found[k] = Some(item);
        }
    }
    Ok(found)
}

/// The bin item's "undelete" verb, then a wait (pumping) until `r` is gone and `original` is
/// there; refused before the verb when `original` already exists.
fn undelete(req: &Request, item: &Com, r: &Path, original: &Path) -> Result<(), NotRestored> {
    if std::fs::symlink_metadata(original).is_ok() {
        let name = original.file_name().map_or_else(|| original.display().to_string(), |n| n.to_string_lossy().into_owned());
        return Err(NotRestored::InTheWay(format!("{name} is in the way")));
    }
    let mut p: *mut c_void = std::ptr::null_mut();
    // SAFETY: `item` is a live IShellItem; `p` receives an IContextMenu or stays null.
    let hr = unsafe { ((**(item.0 as *const *const ShellItemVtbl)).bind_to_handler)(item.0, std::ptr::null_mut(), &BHID_SFUIObject, &IID_ICONTEXTMENU, &mut p) };
    if hr < 0 || p.is_null() {
        return Err(NotRestored::Fallback(Err(format!("no context menu 0x{:08X}", hr as u32))));
    }
    let cm = Com(p);
    // SAFETY: `cm` is a live IContextMenu.
    let cv = unsafe { &**(cm.0 as *const *const ContextMenuVtbl) };
    // SAFETY: a new empty menu, destroyed below.
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return Err(NotRestored::Fallback(Err("CreatePopupMenu failed".into())));
    }
    // SAFETY: a live IContextMenu and our menu.
    let hr = unsafe { (cv.query_context_menu)(cm.0, menu, 0, 1, 0x7FFF, CMF_NORMAL) };
    let hr = if hr < 0 {
        Err(format!("QueryContextMenu 0x{:08X}", hr as u32))
    } else {
        let mut ici = CMINVOKECOMMANDINFOEX::default();
        ici.cbSize = std::mem::size_of::<CMINVOKECOMMANDINFOEX>() as u32;
        ici.fMask = CMIC_MASK_NOASYNC | if req.no_confirm { CMIC_MASK_FLAG_NO_UI } else { 0 };
        ici.hwnd = req.hwnd as HWND;
        ici.lpVerb = c"undelete".as_ptr() as *const u8;
        ici.nShow = SW_SHOWNORMAL;
        // SAFETY: a live IContextMenu and a fully initialised CMINVOKECOMMANDINFOEX whose verb
        // string outlives the call.
        Ok(unsafe { (cv.invoke_command)(cm.0, &ici) })
    };
    // SAFETY: the menu made above.
    unsafe { DestroyMenu(menu) };
    match hr {
        Err(why) => Err(NotRestored::Fallback(Err(why))),
        Ok(hr) if hr < 0 => Err(NotRestored::Fallback(Ok(hr))),
        Ok(_) => match wait_restored(r, original) {
            Wait::Restored => Ok(()),
            Wait::TimedOut => Err(NotRestored::Fallback(Ok(HR_TIMEOUT))),
            Wait::Quit => Err(NotRestored::Quit),
        },
    }
}

/// How `wait_restored` ended.
enum Wait {
    Restored,
    TimedOut,
    /// A `WM_QUIT` came first (posted again for the worker's loop).
    Quit,
}

/// Wait, pumping this thread's messages, until `r` is gone and `original` exists (at most
/// `UNDELETE_WAIT`). A `WM_QUIT` met here is posted again for the worker's loop and ends the wait.
fn wait_restored(r: &Path, original: &Path) -> Wait {
    let end = Instant::now() + UNDELETE_WAIT;
    let done = || std::fs::symlink_metadata(r).is_err() && std::fs::symlink_metadata(original).is_ok();
    loop {
        if done() {
            return Wait::Restored;
        }
        if Instant::now() >= end {
            return Wait::TimedOut;
        }
        // SAFETY: a standard PeekMessage pump on this thread's queue; `msg` is a valid out-buffer.
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_QUIT {
                    PostThreadMessageW(GetCurrentThreadId(), WM_QUIT, msg.wParam, msg.lParam);
                    return if done() { Wait::Restored } else { Wait::Quit };
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The fallback: move `r` into the original's folder under the original's name (an ordinary
/// `MoveAs` job, through the sink), then its `$I` file removed. The item's HRESULT as the sink
/// reported it (when it reported none: cancelled if aborted, 0 if the original is there, else
/// E_FAIL), and whether the operation was aborted. An item the verb did put back after all (`r`
/// gone, `original` there) is not moved: hr 0.
fn move_back(req: &Request, r: &Path, original: &Path, progress: &Rc<dyn Fn(Progress)>) -> (HRESULT, bool) {
    let exists = |p: &Path| std::fs::symlink_metadata(p).is_ok();
    if !exists(r) && exists(original) {
        eprintln!("[fileops] undelete {} -> {}: already back, nothing moved", r.display(), original.display());
        return (0, false);
    }
    let (Some(dir), Some(name)) = (original.parent(), original.file_name()) else {
        return (E_INVALIDARG, false);
    };
    let job = Job::MoveAs { items: vec![(r.to_path_buf(), name.to_string_lossy().into_owned())], dest_dir: dir.to_path_buf() };
    let sub = Request { id: req.id, job, hwnd: req.hwnd, no_confirm: req.no_confirm };
    let mut got = Vec::new();
    match perform_inner(&sub, progress, &mut got) {
        Err(hr) => (hr, false),
        Ok(aborted) => {
            let none = if aborted {
                HR_CANCELLED
            } else if exists(original) {
                0
            } else {
                E_FAIL
            };
            let hr = got.iter().find(|x| same_path(&x.source, r)).map_or(none, |x| x.hr);
            if hr >= 0 && !exists(r) {
                // Windows leaves the bin's `$I` record behind when the item is moved out.
                if let Some(i) = info_file(r) {
                    let _ = std::fs::remove_file(i);
                }
            }
            (hr, aborted)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn restore_moves_only_out_of_the_bin() {
        assert!(in_recycle_bin(Path::new(r"C:\$Recycle.Bin\S-1-5-21-1\$RAB12CD.txt")));
        assert!(in_recycle_bin(Path::new(r"D:\$RECYCLE.BIN\S-1-5-21-1\$RAB12CD")), "any case, any drive");
        assert!(!in_recycle_bin(Path::new(r"C:\Users\x\$RAB12CD.txt")));
        assert!(!in_recycle_bin(Path::new(r"C:\Users\x\$Recycle.Bin.txt")), "a name, not the folder");
        assert!(!in_recycle_bin(Path::new(r"C:\$Recycle.Bin")), "the bin itself");
        // The guard runs before any shell call, item by item.
        let req = Request { id: 1, job: Job::Restore { items: vec![] }, hwnd: 0, no_confirm: true };
        let progress: Rc<dyn Fn(Progress)> = Rc::new(|_| {});
        let items = [(PathBuf::from(r"C:\Users\x\$RAB12CD.txt"), PathBuf::from(r"C:\Users\x\a.txt"))];
        let mut results = Vec::new();
        assert_eq!(restore(&req, &items, &progress, &mut results), Err(E_INVALIDARG));
        assert_eq!(results, [ItemResult { source: items[0].0.clone(), dest: Some(items[0].1.clone()), hr: E_INVALIDARG }]);

        assert_eq!(info_file(Path::new(r"C:\$Recycle.Bin\S-1\$RAB12CD.txt")), Some(PathBuf::from(r"C:\$Recycle.Bin\S-1\$IAB12CD.txt")));
        assert_eq!(info_file(Path::new(r"C:\$Recycle.Bin\S-1\$RXY9")), Some(PathBuf::from(r"C:\$Recycle.Bin\S-1\$IXY9")), "a folder");
        assert_eq!(info_file(Path::new(r"C:\$Recycle.Bin\S-1\a.txt")), None);
    }

    #[test]
    fn job_flags() {
        let base = FOF_ALLOWUNDO | FOFX_ADDUNDORECORD | FOF_NOCONFIRMMKDIR | FOFX_SHOWELEVATIONPROMPT;
        let copy = Job::Copy { sources: vec![], dest_dir: PathBuf::new() };
        assert_eq!(flags(&copy, false), base);
        assert_eq!(flags(&copy, true), base | FOF_NOCONFIRMATION);
        let recycle = Job::Delete { sources: vec![], permanent: false };
        assert_eq!(flags(&recycle, false), base, "a recycle keeps FOF_ALLOWUNDO");
        let nuke = Job::Delete { sources: vec![], permanent: true };
        let f = flags(&nuke, false);
        assert_eq!(f & FOF_ALLOWUNDO, 0, "a permanent delete drops FOF_ALLOWUNDO");
        assert_ne!(f & FOF_WANTNUKEWARNING, 0);
        assert_eq!(f & FOF_NOCONFIRMATION, 0, "the shell confirms a permanent delete");
        let quiet = flags(&nuke, true);
        assert_eq!(quiet & (FOF_ALLOWUNDO | FOF_WANTNUKEWARNING), 0);
        assert_ne!(quiet & FOF_NOCONFIRMATION, 0);
        let rename = Job::Rename { source: PathBuf::new(), new_name: "x".into() };
        assert_eq!(flags(&rename, false), base, "a rename onto an existing name asks, as in Explorer");
        let here = Job::Copy { sources: vec![PathBuf::from(r"C:\d\a.txt")], dest_dir: PathBuf::from(r"c:\D\") };
        assert_eq!(flags(&here, false), base | FOF_RENAMEONCOLLISION, "a copy into its own folder makes a copy");
        let there = Job::Copy { sources: vec![PathBuf::from(r"C:\d\a.txt")], dest_dir: PathBuf::from(r"C:\e") };
        assert_eq!(flags(&there, false), base, "a conflict elsewhere asks");
        let nf = Job::NewFolder { parent: PathBuf::from(r"C:\d"), name: "New folder".into() };
        assert_eq!(flags(&nf, false), base | FOF_RENAMEONCOLLISION);
        let file = Job::NewFile { parent: PathBuf::from(r"C:\d"), name: NEW_FILE_NAME.into() };
        assert_eq!(flags(&file, false), base | FOF_RENAMEONCOLLISION, "a new file is numbered as a new folder is");
    }

    #[test]
    fn status_texts() {
        let src = vec![PathBuf::from("a"), PathBuf::from("b"), PathBuf::from("c")];
        let copy = Job::Copy { sources: src.clone(), dest_dir: PathBuf::new() };
        assert_eq!(status_text(&copy, &Outcome::Done, &[]), "Copied 3 items");
        let one = Job::Move { sources: vec![PathBuf::from("a")], dest_dir: PathBuf::new() };
        assert_eq!(status_text(&one, &Outcome::Done, &[]), "Moved 1 item");
        let del = Job::Delete { sources: src.clone(), permanent: false };
        assert_eq!(status_text(&del, &Outcome::Done, &[]), "Deleted 3 items to the Recycle Bin");
        let nuke = Job::Delete { sources: src, permanent: true };
        assert_eq!(status_text(&nuke, &Outcome::Done, &[]), "Deleted 3 items permanently");
        let ren = Job::Rename { source: PathBuf::from("a"), new_name: "b.txt".into() };
        assert_eq!(status_text(&ren, &Outcome::Done, &[]), "Renamed to b.txt");
        let nf = Job::NewFolder { parent: PathBuf::new(), name: "New folder".into() };
        assert_eq!(status_text(&nf, &Outcome::Done, &[]), "Created New folder");
        let made = [ItemResult { source: PathBuf::from(r"C:\d"), dest: Some(PathBuf::from(r"C:\d\New folder (2)")), hr: 0 }];
        assert_eq!(status_text(&nf, &Outcome::Done, &made), "Created New folder (2)", "the real name wins");
        assert_eq!(real_name(&nf, &made).as_deref(), Some("New folder (2)"));
        let failed = [ItemResult { hr: -1, ..made[0].clone() }];
        assert_eq!(real_name(&nf, &failed), None, "a failed item names nothing");
        assert_eq!(real_name(&copy, &made), None, "only renames and new folders and files have a real name");
        let file = Job::NewFile { parent: PathBuf::new(), name: NEW_FILE_NAME.into() };
        assert_eq!(running_text(&file), "creating file...");
        let made = [ItemResult { source: PathBuf::from(r"C:\d"), dest: Some(PathBuf::from(r"C:\d\New Text Document (2).txt")), hr: 0 }];
        assert_eq!(status_text(&file, &Outcome::Done, &made), "Created New Text Document (2).txt");
        assert_eq!(status_text(&copy, &Outcome::Cancelled, &[]), "Cancelled");
        assert_eq!(status_text(&copy, &Outcome::Failed(0x80070005u32 as i32), &[]), "Failed: 0x80070005");
        assert_eq!(running_text(&copy), "copying 3 items...");
        let mv_as = Job::MoveAs { items: vec![(PathBuf::from(r"C:\b\a (2).txt"), "a.txt".into())], dest_dir: PathBuf::from(r"C:\a") };
        assert_eq!(status_text(&mv_as, &Outcome::Done, &[]), "Moved 1 item");
        assert_eq!(running_text(&mv_as), "moving 1 item...");
        assert_eq!(mv_as.paths(), [Path::new(r"C:\b\a (2).txt"), Path::new(r"C:\a")]);
        let restore = Job::Restore { items: vec![(PathBuf::from(r"C:\$Recycle.Bin\S\$RX.txt"), PathBuf::from(r"C:\a\x.txt")); 2] };
        assert_eq!(status_text(&restore, &Outcome::Done, &[]), "Restored 2 items from the Recycle Bin");
        assert_eq!(running_text(&restore), "restoring 2 items...");
        assert_eq!(restore.paths(), [Path::new(r"C:\a\x.txt"); 2], "the originals only: the soak fence");
    }

    #[test]
    fn progress_percent_and_gate() {
        assert_eq!(percent(0, 0), None);
        assert_eq!(percent(0, 5), None);
        assert_eq!(percent(200, 0), Some(0));
        assert_eq!(percent(200, 101), Some(50));
        assert_eq!(percent(200, 300), Some(100), "clamped");
        assert_eq!(percent(u32::MAX, u32::MAX - 1), Some(99), "no overflow");

        let t0 = Instant::now();
        let ms = |n: u64| t0 + Duration::from_millis(n);
        let mut g = ProgressGate::new();
        assert!(g.due(ms(0), 0), "the first report goes through");
        assert!(!g.due(ms(10), 5), "within 250 ms");
        assert!(!g.due(ms(300), 0), "unchanged");
        assert!(g.due(ms(300), 7));
        assert!(!g.due(ms(549), 8));
        assert!(g.due(ms(550), 8), "250 ms after the last report");
        assert!(g.due(ms(560), 100), "100 goes through when it changes");
        assert!(!g.due(ms(900), 100), "but only once");
    }

    #[test]
    fn name_validation() {
        assert_eq!(invalid_name("report.txt"), None);
        assert_eq!(invalid_name("New folder"), None);
        assert!(invalid_name("").is_some());
        assert!(invalid_name("   ").is_some());
        for bad in ["a/b", "a\\b", "a:b", "a*b", "a?b", "a\"b", "a<b", "a>b", "a|b", "a\tb"] {
            assert!(invalid_name(bad).is_some(), "{bad}");
        }
        assert!(invalid_name("trailing.").is_some());
        assert!(invalid_name("trailing ").is_some());
    }

    /// Jobs cancelled while queued come back `Cancelled` and never touch the disk; later ones run.
    #[test]
    fn cancelled_queued_jobs_never_run() {
        let root = std::env::temp_dir().join(format!("twinfm-m3-test-queue-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (tx, rx) = unbounded();
        let worker = Worker::spawn(move |d| {
            let _ = tx.send(d);
        }, |_| {});
        worker.cancel_queued(2);
        for (id, name) in [(1, "one"), (2, "two"), (3, "three")] {
            let job = Job::NewFolder { parent: root.clone(), name: name.into() };
            assert!(worker.submit(Request { id, job, hwnd: 0, no_confirm: true }));
        }
        let mut got: Vec<(u64, Outcome)> =
            (0..3).map(|_| rx.recv_timeout(Duration::from_secs(30)).map(|d| (d.id, d.outcome)).expect("each job reports")).collect();
        got.sort_by_key(|g| g.0);
        assert_eq!(got, [(1, Outcome::Cancelled), (2, Outcome::Cancelled), (3, Outcome::Done)]);
        assert!(!root.join("one").exists() && !root.join("two").exists());
        assert!(root.join("three").is_dir());
        drop(worker);
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// The real shell, end to end, on a scratch folder: start the STA worker with no owner
    /// window, then new folder, copy, rename, move and a permanent delete with
    /// `FOF_NOCONFIRMATION` (through the job, not the env var), checking the disk after each, and
    /// what the progress sink reported (real names and paths, one result per top-level item).
    #[test]
    fn worker_runs_real_shell_operations() {
        let root = std::env::temp_dir().join(format!("twinfm-m3-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.txt"), b"hello").unwrap();
        let (tx, rx) = unbounded();
        let worker = Worker::spawn(move |d| {
            let _ = tx.send(d);
        }, |_| {});
        let mut id = 0;
        let mut run = |job: Job| -> Vec<ItemResult> {
            id += 1;
            assert!(worker.submit(Request { id, job, hwnd: 0, no_confirm: true }));
            let d = rx.recv_timeout(Duration::from_secs(30)).expect("job finishes within 30 s");
            assert_eq!(d.id, id);
            assert_eq!(d.outcome, Outcome::Done, "{:?}", d.job);
            assert!(d.results.iter().all(ItemResult::ok), "{:?}", d.results);
            d.results
        };
        // The shell hands back long paths; %TEMP% may be an 8.3 path (the CI runner's is).
        let canon = |p: &Path| std::fs::canonicalize(p).unwrap();
        let dest = |r: &[ItemResult]| -> Vec<PathBuf> { r.iter().map(|x| canon(x.dest.as_ref().expect("a dest path"))).collect() };

        let r = run(Job::NewFolder { parent: root.clone(), name: "made".into() });
        assert!(root.join("made").is_dir());
        assert_eq!(dest(&r), [canon(&root.join("made"))]);

        let r = run(Job::Copy { sources: vec![root.join("a.txt")], dest_dir: root.join("made") });
        assert_eq!(std::fs::read(root.join("made").join("a.txt")).unwrap(), b"hello");
        assert!(root.join("a.txt").exists(), "a copy keeps the source");
        assert_eq!(dest(&r), [canon(&root.join("made").join("a.txt"))]);
        // P11-M3: the same copy again replaces it (no confirmation asked): the worker saw the path
        // before the job, the sink reports the item landing on it, so the record cannot be undone.
        // Through the folder's 8.3 name when the volume has one (the CI runner's %TEMP% is one): the
        // sink names the item by its long path all the same.
        let short = {
            let w = wide(root.join("made").as_os_str());
            let mut buf = vec![0u16; 1024];
            // SAFETY: NUL-terminated input; `buf` holds its length in u16s.
            let n = unsafe { windows_sys::Win32::Storage::FileSystem::GetShortPathNameW(w.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
            if n == 0 || n >= buf.len() { root.join("made") } else { PathBuf::from(String::from_utf16_lossy(&buf[..n])) }
        };
        assert!(worker.submit(Request { id: 900, job: Job::Copy { sources: vec![root.join("a.txt")], dest_dir: short }, hwnd: 0, no_confirm: true }));
        let r = rx.recv_timeout(Duration::from_secs(30)).expect("job finishes within 30 s");
        assert_eq!((r.id, r.outcome.clone(), r.existed.len()), (900, Outcome::Done, 1));
        assert_eq!(canon(&r.existed[0]), canon(&root.join("made").join("a.txt")));
        let rec = undo::forbid_if_replaced(undo::record_for(&r.job, &r.outcome, &r.results, "Copied"), &r.existed, &r.results).unwrap();
        assert_eq!((rec.undo, rec.cannot), (None, undo::CANNOT_UNDO_REPLACED), "{:?}", r.results);

        let r = run(Job::Rename { source: root.join("made").join("a.txt"), new_name: "b.txt".into() });
        assert!(root.join("made").join("b.txt").exists());
        assert!(!root.join("made").join("a.txt").exists());
        assert_eq!(dest(&r), [canon(&root.join("made").join("b.txt"))], "a rename reports the new path");

        let r = run(Job::Move { sources: vec![root.join("made").join("b.txt")], dest_dir: root.clone() });
        assert!(root.join("b.txt").exists());
        assert!(!root.join("made").join("b.txt").exists());
        assert_eq!(dest(&r), [canon(&root.join("b.txt"))], "a move reports the destination path");

        let r = run(Job::Copy { sources: vec![root.join("a.txt")], dest_dir: root.clone() });
        assert!(root.join("a - Copy.txt").exists(), "a copy into its own folder is renamed, not stopped");
        assert_eq!(dest(&r), [canon(&root.join("a - Copy.txt"))], "the copy's real name");
        let job = Job::NewFolder { parent: root.clone(), name: "made".into() };
        let r = run(job.clone());
        assert!(root.join("made (2)").is_dir(), "a new folder over an existing name is numbered");
        assert_eq!(dest(&r), [canon(&root.join("made (2)"))], "the real name of the new folder");
        assert_eq!(real_name(&job, &r).as_deref(), Some("made (2)"));
        assert_eq!(status_text(&job, &Outcome::Done, &r), "Created made (2)");
        // A new file (P6-M3): empty, numbered over an existing name as the shell numbers it.
        let job = Job::NewFile { parent: root.clone(), name: NEW_FILE_NAME.into() };
        let r = run(job.clone());
        assert_eq!(std::fs::metadata(root.join(NEW_FILE_NAME)).unwrap().len(), 0, "the new file is empty");
        assert_eq!(dest(&r), [canon(&root.join(NEW_FILE_NAME))]);
        let r = run(job.clone());
        assert_eq!(real_name(&job, &r).as_deref(), Some("New Text Document (2).txt"));

        // A folder with contents: only the folder itself is reported.
        std::fs::write(root.join("made (2)").join("inner.txt"), b"x").unwrap();
        let r = run(Job::Copy { sources: vec![root.join("made (2)")], dest_dir: root.join("made") });
        assert!(root.join("made").join("made (2)").join("inner.txt").exists());
        assert_eq!(dest(&r), [canon(&root.join("made").join("made (2)"))], "a folder's contents are not the job's items");

        // Undo round trip: move a file, build the undo from the sink's results, run it.
        std::fs::write(root.join("c.txt"), b"undo me").unwrap();
        let job = Job::Move { sources: vec![root.join("c.txt")], dest_dir: root.join("made") };
        let r = run(job.clone());
        assert!(root.join("made").join("c.txt").exists() && !root.join("c.txt").exists());
        let u = undo::undo_for(&job, &r).expect("a move is undoable");
        assert_eq!(undo::check(&u), Ok(()));
        run(u.job);
        assert_eq!(std::fs::read(root.join("c.txt")).unwrap(), b"undo me", "the file is back");
        assert!(!root.join("made").join("c.txt").exists());

        let sources = vec![root.join("b.txt"), root.join("made"), root.join("made (2)"), root.join("a - Copy.txt"), root.join("c.txt")];
        let r = run(Job::Delete { sources: sources.clone(), permanent: true });
        assert!(!root.join("b.txt").exists());
        assert!(!root.join("made").exists());
        assert!(!root.join("made (2)").exists());
        assert!(root.join("a.txt").exists());
        assert_eq!(r.len(), sources.len(), "one result per source: {r:?}");
        for s in &sources {
            assert!(r.iter().any(|x| same_path(&x.source, s)), "{} reported", s.display());
        }

        drop(run);
        drop(worker);
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// The real shell and the real Recycle Bin, on a scratch folder (`twinfm-p5m2-test-<pid>`):
    /// a file recycled and restored through the undo record (the bin's "undelete" verb, which
    /// must finish in under 25 s, well inside the verb's 30 s wait before the move back), the redo
    /// material of that restore, a `MoveAs` round trip, and progress reports from a folder copy.
    /// It adds one Recycle Bin entry and restores it, so a passing run leaves the bin's listing as
    /// it found it (an orphan `$I` file, which Windows's own restore can leave, may stay on disk);
    /// the scratch folder is then deleted permanently.
    #[test]
    fn recycle_restore_move_as_and_progress() {
        let root = std::env::temp_dir().join(format!("twinfm-p5m2-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("made")).unwrap();
        let root = long_path(&root);
        let (tx, rx) = unbounded();
        let (ptx, prx) = unbounded();
        let worker = Worker::spawn(
            move |d| {
                let _ = tx.send(d);
            },
            move |p| {
                let _ = ptx.send(p);
            },
        );
        let mut id = 0;
        let mut run = |job: Job| -> (u64, Vec<ItemResult>) {
            id += 1;
            assert!(worker.submit(Request { id, job, hwnd: 0, no_confirm: true }));
            let d = rx.recv_timeout(Duration::from_secs(60)).expect("job finishes within 60 s");
            assert_eq!(d.id, id);
            assert_eq!(d.outcome, Outcome::Done, "{:?}: {:?}", d.job, d.results);
            assert!(d.results.iter().all(ItemResult::ok), "{:?}", d.results);
            (id, d.results)
        };
        let lower = |p: &Path| p.to_string_lossy().to_lowercase();

        // Recycle, then undo through the record: a restore from the bin.
        let file = root.join("recycled.txt");
        std::fs::write(&file, b"from the bin").unwrap();
        let del = Job::Delete { sources: vec![file.clone()], permanent: false };
        let (_, r) = run(del.clone());
        assert!(!file.exists());
        let r_path = r[0].dest.clone().expect("the sink reports the item in the bin");
        assert!(lower(&r_path).contains(r"\$recycle.bin\"), "{}", r_path.display());
        let rec = undo::record_for(&del, &Outcome::Done, &r, "Deleted 1 item to the Recycle Bin").unwrap();
        let u = rec.undo.expect("a recycle is undoable");
        assert_eq!(u.job, Job::Restore { items: vec![(r_path.clone(), file.clone())] });
        assert_eq!(undo::check(&u), Ok(()));
        let restore = u.job;
        let t = std::time::Instant::now();
        let (_, r) = run(restore.clone());
        assert!(t.elapsed() < Duration::from_secs(25), "the verb restored it (took {:?})", t.elapsed());
        assert_eq!(std::fs::read(&file).unwrap(), b"from the bin", "the file is back");
        assert!(!r_path.exists(), "and gone from the bin");
        assert_eq!(r.len(), 1);
        assert_eq!((&r[0].source, r[0].dest.as_ref()), (&r_path, Some(&file)), "source: the `$R` path, dest: the original");
        // The redo material: the restore's own record recycles the file again.
        let redo = undo::record_for(&restore, &Outcome::Done, &r, "Restored 1 item from the Recycle Bin").unwrap();
        assert_eq!(redo.undo.unwrap().job, Job::Delete { sources: vec![file.clone()], permanent: false });

        // MoveAs: made\b.txt into the root as c.txt, and back.
        std::fs::write(root.join("made").join("b.txt"), b"bee").unwrap();
        let job = Job::MoveAs { items: vec![(root.join("made").join("b.txt"), "c.txt".into())], dest_dir: root.clone() };
        let (_, r) = run(job.clone());
        assert_eq!(std::fs::read(root.join("c.txt")).unwrap(), b"bee");
        assert_eq!(r.iter().map(|x| lower(x.dest.as_ref().unwrap())).collect::<Vec<_>>(), [lower(&root.join("c.txt"))]);
        let u = undo::undo_for(&job, &r).expect("a MoveAs is undoable");
        match &u.job {
            Job::MoveAs { items, dest_dir } => {
                assert_eq!(items.len(), 1);
                assert_eq!(lower(&items[0].0), lower(&root.join("c.txt")));
                assert_eq!(items[0].1, "b.txt");
                assert!(same_dir(dest_dir, &root.join("made")), "{}", dest_dir.display());
            }
            other => panic!("undo of a MoveAs: {other:?}"),
        }
        run(u.job);
        assert_eq!(std::fs::read(root.join("made").join("b.txt")).unwrap(), b"bee", "moved back under its old name");

        // Progress: a copy of a folder of a few files.
        for k in 0..8 {
            std::fs::write(root.join("made").join(format!("f{k}.bin")), vec![k as u8; 256 * 1024]).unwrap();
        }
        std::fs::create_dir(root.join("out")).unwrap();
        while prx.try_recv().is_ok() {}
        let (copy_id, _) = run(Job::Copy { sources: vec![root.join("made")], dest_dir: root.join("out") });
        let got: Vec<Progress> = prx.try_iter().collect();
        assert!(!got.is_empty(), "UpdateProgress reported nothing for the copy");
        assert!(got.iter().all(|p| p.id == copy_id && p.percent <= 100), "{got:?}");
        eprintln!("[test] progress for the copy: {got:?}");

        drop(run);
        drop(worker);
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// `p` with its 8.3 components expanded (`canonicalize` without the `\\?\` prefix): the CI
    /// runner's %TEMP% is an 8.3 path (RUNNER~1) and the shell hands back long paths, which a
    /// path inside a zip cannot be canonicalised to afterwards.
    fn long_path(p: &Path) -> PathBuf {
        let c = std::fs::canonicalize(p).unwrap();
        let s = c.to_string_lossy();
        PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s))
    }

    /// The shell's zip folder parses paths inside an archive into shell items, and a copy of them
    /// is an extraction whose sink results name the in-archive sources, so the undo record
    /// recycles the extracted copies.
    #[test]
    fn shell_items_inside_a_zip_extract() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!("twinfm-p3m4-shell-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("out")).unwrap();
        let root = long_path(&root);
        let zip = root.join("t.zip");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&zip).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        for (name, data) in [("sub/file.txt", &b"from the zip"[..]), ("sub/deep/two.bin", b"22")] {
            w.start_file(name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
        let child = zip.join("sub").join("file.txt");
        // SAFETY: an STA for this test thread, balanced below; the item is released before.
        let com = unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
        {
            let item = shell_item(&child).expect("the zip folder parses a path inside the archive");
            let mut s: *mut u16 = std::ptr::null_mut();
            // SAFETY: a live IShellItem; the string is freed with CoTaskMemFree.
            let name = unsafe {
                let v = &**(item.0 as *const *const ShellItemVtbl);
                assert!((v.get_display_name)(item.0, SIGDN_FILESYSPATH, &mut s) < 0, "no file system path inside a zip");
                assert!((v.get_display_name)(item.0, SIGDN_DESKTOPABSOLUTEPARSING, &mut s) >= 0);
                let len = (0..).take_while(|&i| *s.add(i) != 0).count();
                let n = String::from_utf16_lossy(std::slice::from_raw_parts(s, len));
                windows_sys::Win32::System::Com::CoTaskMemFree(s as *const c_void);
                n
            };
            assert!(name.eq_ignore_ascii_case(&child.to_string_lossy()), "{name}");
            assert_eq!(item_path(item.0).as_deref(), Some(child.as_path()), "the sink's path for it");
        }
        if com >= 0 {
            // SAFETY: pairs the CoInitializeEx above.
            unsafe { windows_sys::Win32::System::Com::CoUninitialize() };
        }
        let (tx, rx) = unbounded();
        let worker = Worker::spawn(move |d| {
            let _ = tx.send(d);
        }, |_| {});
        let job = Job::Copy { sources: vec![child.clone(), zip.join("sub").join("deep")], dest_dir: root.join("out") };
        assert!(worker.submit(Request { id: 1, job: job.clone(), hwnd: 0, no_confirm: true }));
        let d = rx.recv_timeout(Duration::from_secs(30)).expect("the extraction finishes within 30 s");
        assert_eq!(d.outcome, Outcome::Done);
        assert_eq!(std::fs::read(root.join("out").join("file.txt")).unwrap(), b"from the zip");
        assert_eq!(std::fs::read(root.join("out").join("deep").join("two.bin")).unwrap(), b"22");
        let sources: Vec<&Path> = d.results.iter().map(|r| r.source.as_path()).collect();
        assert_eq!(sources.len(), 2, "one result per source: {:?}", d.results);
        let u = undo::undo_for(&job, &d.results).expect("an extraction is undoable");
        let canon = |p: &Path| std::fs::canonicalize(p).unwrap();
        match &u.job {
            Job::Delete { sources, permanent: false } => {
                let mut got: Vec<PathBuf> = sources.iter().map(|p| canon(p)).collect();
                got.sort();
                assert_eq!(got, vec![canon(&root.join("out").join("deep")), canon(&root.join("out").join("file.txt"))]);
            }
            other => panic!("undo of an extraction: {other:?}"),
        }
        drop(worker);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
