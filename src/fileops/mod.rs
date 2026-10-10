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
//! new path, a copy's path) and the HRESULT; they come back in `Done::results`.

pub mod clipboard;
pub mod undo;

use crossbeam_channel::{Receiver, Sender, unbounded};
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use windows_sys::Win32::Foundation::{ERROR_CANCELLED, HWND};
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY;
use windows_sys::Win32::System::Com::{CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Shell::{
    COPYENGINE_E_USER_CANCELLED, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOCONFIRMMKDIR, FOF_RENAMEONCOLLISION,
    FOF_WANTNUKEWARNING,
    FOFX_ADDUNDORECORD, FOFX_SHOWELEVATIONPROMPT, FileOperation, SHCreateItemFromParsingName,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, TranslateMessage, WM_APP,
    WM_QUIT,
};
use windows_sys::core::{BOOL, GUID, HRESULT};

/// Thread message that tells the worker the channel has requests.
const WM_RUN: u32 = WM_APP + 1;

/// HRESULT_FROM_WIN32(ERROR_CANCELLED): what some shell paths return when the user cancels.
pub const HR_CANCELLED: HRESULT = (0x8007_0000u32 | ERROR_CANCELLED) as HRESULT;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Job {
    Copy { sources: Vec<PathBuf>, dest_dir: PathBuf },
    Move { sources: Vec<PathBuf>, dest_dir: PathBuf },
    Delete { sources: Vec<PathBuf>, permanent: bool },
    Rename { source: PathBuf, new_name: String },
    NewFolder { parent: PathBuf, name: String },
}

impl Job {
    /// Items the job acts on.
    pub fn count(&self) -> usize {
        match self {
            Job::Copy { sources, .. } | Job::Move { sources, .. } | Job::Delete { sources, .. } => sources.len(),
            Job::Rename { .. } | Job::NewFolder { .. } => 1,
        }
    }

    /// Every path the job reads or writes, for the soak guard.
    pub fn paths(&self) -> Vec<&Path> {
        match self {
            Job::Copy { sources, dest_dir } | Job::Move { sources, dest_dir } => {
                sources.iter().map(PathBuf::as_path).chain([dest_dir.as_path()]).collect()
            }
            Job::Delete { sources, .. } => sources.iter().map(PathBuf::as_path).collect(),
            Job::Rename { source, .. } => vec![source.as_path()],
            Job::NewFolder { parent, .. } => vec![parent.as_path()],
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
}

/// The name the shell really gave a renamed item or a new folder ("New folder (2)"), if the
/// sink reported it; None for the other jobs.
pub fn real_name(job: &Job, results: &[ItemResult]) -> Option<String> {
    match job {
        Job::Rename { .. } | Job::NewFolder { .. } => results
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
        Job::NewFolder { .. } => f |= FOF_RENAMEONCOLLISION,
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
        Outcome::Done => match job {
            Job::Copy { .. } => format!("Copied {}", items(job.count())),
            Job::Move { .. } => format!("Moved {}", items(job.count())),
            Job::Delete { permanent: false, .. } => format!("Deleted {} to the Recycle Bin", items(job.count())),
            Job::Delete { permanent: true, .. } => format!("Deleted {} permanently", items(job.count())),
            Job::Rename { new_name, .. } => format!("Renamed to {}", real_name(job, results).unwrap_or_else(|| new_name.clone())),
            Job::NewFolder { name, .. } => format!("Created {}", real_name(job, results).unwrap_or_else(|| name.clone())),
        },
    }
}

/// The status line while a job runs.
pub fn running_text(job: &Job) -> String {
    match job {
        Job::Copy { .. } => format!("copying {}...", items(job.count())),
        Job::Move { .. } => format!("moving {}...", items(job.count())),
        Job::Delete { .. } => format!("deleting {}...", items(job.count())),
        Job::Rename { .. } => "renaming...".into(),
        Job::NewFolder { .. } => "creating folder...".into(),
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

/// Handle to the worker. Dropping it ends the worker's message loop.
pub struct Worker {
    tid: u32,
    tx: Sender<Request>,
}

impl Worker {
    /// Start the worker. `done` is called on the worker thread after each job.
    pub fn spawn(done: impl Fn(Done) + Send + 'static) -> Worker {
        let (tx, rx) = unbounded::<Request>();
        let (tid_tx, tid_rx) = crossbeam_channel::bounded(1);
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
                run_loop(rx, done);
            })
            .expect("spawn fileops thread");
        let tid = tid_rx.recv().unwrap_or(0);
        Worker { tid, tx }
    }

    /// Queue a job. False if the worker is gone.
    pub fn submit(&self, r: Request) -> bool {
        if self.tx.send(r).is_err() {
            return false;
        }
        // SAFETY: posting to a thread id; fails harmlessly if the thread has exited.
        unsafe { PostThreadMessageW(self.tid, WM_RUN, 0, 0) != 0 }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // SAFETY: as in `submit`.
        unsafe { PostThreadMessageW(self.tid, WM_QUIT, 0, 0) };
    }
}

fn run_loop(rx: Receiver<Request>, done: impl Fn(Done)) {
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
                let (outcome, results) = if com >= 0 { perform(&req) } else { (Outcome::Failed(com), Vec::new()) };
                done(Done { id: req.id, job: req.job, outcome, results });
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

fn shell_item(path: &Path) -> Result<Com, HRESULT> {
    let w = wide(path.as_os_str());
    let mut p: *mut c_void = std::ptr::null_mut();
    // SAFETY: `w` is NUL-terminated and outlives the call; `p` receives an IShellItem or stays null.
    let hr = unsafe { SHCreateItemFromParsingName(w.as_ptr(), std::ptr::null_mut(), &IID_ISHELLITEM, &mut p) };
    if hr < 0 || p.is_null() { Err(if hr < 0 { hr } else { -1 }) } else { Ok(Com(p)) }
}

fn check(hr: HRESULT) -> Result<(), HRESULT> {
    if hr < 0 { Err(hr) } else { Ok(()) }
}

// --- The progress sink ------------------------------------------------------------------------

const IID_IUNKNOWN: GUID = GUID::from_u128(0x00000000_0000_0000_c000_000000000046);
const IID_IFILEOPERATIONPROGRESSSINK: GUID = GUID::from_u128(0x04b0f1a7_9490_44bc_96e1_4296a31252e2);

/// SIGDN_FILESYSPATH.
const SIGDN_FILESYSPATH: i32 = 0x8005_8000u32 as i32;

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

/// IShellItem, up to the one method used.
#[repr(C)]
struct ShellItemVtbl {
    base: UnknownVtbl,
    _bind_to_handler: Slot,
    _get_parent: Slot,
    get_display_name: unsafe extern "system" fn(*mut c_void, i32, *mut *mut u16) -> HRESULT,
}

/// Our sink: a `Box` with the vtable pointer first, a reference count, and the results. It is
/// made, advised, called and released on the worker thread only (an STA), so the results need
/// no lock; the count is atomic only because that is what COM objects conventionally do.
#[repr(C)]
struct Sink {
    vtbl: *const SinkVtbl,
    refs: std::sync::atomic::AtomicU32,
    results: std::cell::RefCell<Vec<ItemResult>>,
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

/// The file system path of a shell item we were handed (not owned), if it has one.
fn item_path(item: Item) -> Option<PathBuf> {
    if item.is_null() {
        return None;
    }
    let mut s: *mut u16 = std::ptr::null_mut();
    // SAFETY: `item` is a live IShellItem for the duration of the sink call; GetDisplayName
    // allocates `s` with the COM allocator, freed below.
    unsafe {
        let v = &**(item as *const *const ShellItemVtbl);
        if (v.get_display_name)(item, SIGDN_FILESYSPATH, &mut s) < 0 || s.is_null() {
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
unsafe extern "system" fn sink_progress(_: *mut c_void, _: u32, _: u32) -> HRESULT {
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
        Job::Rename { source, .. } => vec![source.as_path()],
        Job::NewFolder { parent, .. } => vec![parent.as_path()],
    };
    // Top-level items share a parent with a source; checking the parent first keeps the
    // canonicalising comparison off a folder's contents.
    let parents: Vec<&Path> = mine.iter().filter_map(|p| p.parent()).collect();
    let is_top = |r: &ItemResult| match job {
        Job::NewFolder { .. } => true,
        _ => r.source.parent().is_some_and(|rp| parents.iter().any(|p| same_dir(p, rp) || same_path(p, rp))),
    };
    all.into_iter().filter(|r| is_top(r) && mine.iter().any(|m| same_path(m, &r.source))).collect()
}

/// Run one job on this (STA, pumping) thread.
fn perform(req: &Request) -> (Outcome, Vec<ItemResult>) {
    let mut results = Vec::new();
    let outcome = match perform_inner(req, &mut results) {
        Ok(false) => Outcome::Done,
        Ok(true) => Outcome::Cancelled,
        Err(hr) if hr == COPYENGINE_E_USER_CANCELLED || hr == HR_CANCELLED => Outcome::Cancelled,
        Err(hr) => Outcome::Failed(hr),
    };
    (outcome, own_results(&req.job, results))
}

/// Ok(aborted) or the first failing HRESULT; the sink's results go to `results` either way.
fn perform_inner(req: &Request, results: &mut Vec<ItemResult>) -> Result<bool, HRESULT> {
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
        Job::NewFolder { parent, name } => {
            let dir = shell_item(parent)?;
            let name = wide(std::ffi::OsStr::new(name));
            check(unsafe {
                (v.new_item)(op.0, dir.0, FILE_ATTRIBUTE_DIRECTORY, name.as_ptr(), std::ptr::null(), std::ptr::null_mut())
            })?;
            keep.push(dir);
        }
    }
    // The sink, advised for this one PerformOperations and unadvised right after. `sink` holds
    // our own reference; the operation takes its own in Advise and drops it in Unadvise (or
    // when it is released).
    let sink = Com(Box::into_raw(Box::new(Sink {
        vtbl: &SINK_VTBL,
        refs: std::sync::atomic::AtomicU32::new(1),
        results: std::cell::RefCell::new(Vec::new()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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
        assert_eq!(real_name(&copy, &made), None, "only renames and new folders have a real name");
        assert_eq!(status_text(&copy, &Outcome::Cancelled, &[]), "Cancelled");
        assert_eq!(status_text(&copy, &Outcome::Failed(0x80070005u32 as i32), &[]), "Failed: 0x80070005");
        assert_eq!(running_text(&copy), "copying 3 items...");
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
        });
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
}
