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

pub mod clipboard;

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
const HR_CANCELLED: HRESULT = (0x8007_0000u32 | ERROR_CANCELLED) as HRESULT;

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

pub struct Done {
    pub id: u64,
    pub job: Job,
    pub outcome: Outcome,
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

/// The status line after a job.
pub fn status_text(job: &Job, outcome: &Outcome) -> String {
    match outcome {
        Outcome::Cancelled => "Cancelled".into(),
        Outcome::Failed(hr) => format!("Failed: 0x{:08X}", *hr as u32),
        Outcome::Done => match job {
            Job::Copy { .. } => format!("Copied {}", items(job.count())),
            Job::Move { .. } => format!("Moved {}", items(job.count())),
            Job::Delete { permanent: false, .. } => format!("Deleted {} to the Recycle Bin", items(job.count())),
            Job::Delete { permanent: true, .. } => format!("Deleted {} permanently", items(job.count())),
            Job::Rename { new_name, .. } => format!("Renamed to {new_name}"),
            Job::NewFolder { name, .. } => format!("Created {name}"),
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
                let outcome = if com >= 0 { perform(&req) } else { Outcome::Failed(com) };
                done(Done { id: req.id, job: req.job, outcome });
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
    _advise: Slot,
    _unadvise: Slot,
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

/// Run one job on this (STA, pumping) thread.
fn perform(req: &Request) -> Outcome {
    match perform_inner(req) {
        Ok(false) => Outcome::Done,
        Ok(true) => Outcome::Cancelled,
        Err(hr) if hr == COPYENGINE_E_USER_CANCELLED || hr == HR_CANCELLED => Outcome::Cancelled,
        Err(hr) => Outcome::Failed(hr),
    }
}

/// Ok(aborted) or the first failing HRESULT.
fn perform_inner(req: &Request) -> Result<bool, HRESULT> {
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
    let performed = unsafe { (v.perform_operations)(op.0) };
    let mut aborted: BOOL = 0;
    let got = unsafe { (v.get_any_operations_aborted)(op.0, &mut aborted) };
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
        assert_eq!(status_text(&copy, &Outcome::Done), "Copied 3 items");
        let one = Job::Move { sources: vec![PathBuf::from("a")], dest_dir: PathBuf::new() };
        assert_eq!(status_text(&one, &Outcome::Done), "Moved 1 item");
        let del = Job::Delete { sources: src.clone(), permanent: false };
        assert_eq!(status_text(&del, &Outcome::Done), "Deleted 3 items to the Recycle Bin");
        let nuke = Job::Delete { sources: src, permanent: true };
        assert_eq!(status_text(&nuke, &Outcome::Done), "Deleted 3 items permanently");
        let ren = Job::Rename { source: PathBuf::from("a"), new_name: "b.txt".into() };
        assert_eq!(status_text(&ren, &Outcome::Done), "Renamed to b.txt");
        let nf = Job::NewFolder { parent: PathBuf::new(), name: "New folder".into() };
        assert_eq!(status_text(&nf, &Outcome::Done), "Created New folder");
        assert_eq!(status_text(&copy, &Outcome::Cancelled), "Cancelled");
        assert_eq!(status_text(&copy, &Outcome::Failed(0x80070005u32 as i32)), "Failed: 0x80070005");
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
    /// `FOF_NOCONFIRMATION` (through the job, not the env var), checking the disk after each.
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
        let mut run = |job: Job| {
            id += 1;
            assert!(worker.submit(Request { id, job, hwnd: 0, no_confirm: true }));
            let d = rx.recv_timeout(Duration::from_secs(30)).expect("job finishes within 30 s");
            assert_eq!(d.id, id);
            assert_eq!(d.outcome, Outcome::Done, "{:?}", d.job);
        };

        run(Job::NewFolder { parent: root.clone(), name: "made".into() });
        assert!(root.join("made").is_dir());

        run(Job::Copy { sources: vec![root.join("a.txt")], dest_dir: root.join("made") });
        assert_eq!(std::fs::read(root.join("made").join("a.txt")).unwrap(), b"hello");
        assert!(root.join("a.txt").exists(), "a copy keeps the source");

        run(Job::Rename { source: root.join("made").join("a.txt"), new_name: "b.txt".into() });
        assert!(root.join("made").join("b.txt").exists());
        assert!(!root.join("made").join("a.txt").exists());

        run(Job::Move { sources: vec![root.join("made").join("b.txt")], dest_dir: root.clone() });
        assert!(root.join("b.txt").exists());
        assert!(!root.join("made").join("b.txt").exists());

        run(Job::Copy { sources: vec![root.join("a.txt")], dest_dir: root.clone() });
        assert!(root.join("a - Copy.txt").exists(), "a copy into its own folder is renamed, not stopped");
        run(Job::NewFolder { parent: root.clone(), name: "made".into() });
        assert!(root.join("made (2)").is_dir(), "a new folder over an existing name is numbered");

        run(Job::Delete {
            sources: vec![root.join("b.txt"), root.join("made"), root.join("made (2)"), root.join("a - Copy.txt")],
            permanent: true,
        });
        assert!(!root.join("b.txt").exists());
        assert!(!root.join("made").exists());
        assert!(!root.join("made (2)").exists());
        assert!(root.join("a.txt").exists());

        drop(run);
        drop(worker);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
