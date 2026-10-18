//! The verb worker (P7-M1): a shell command picked in the context menu runs here, on its own
//! single-threaded apartment with a message loop, not on the UI thread. The UI thread shows the
//! menu and reads the picked command's verb and label (`shellmenu::show`); the worker builds the
//! menu again for the same items and invokes the command there (`shellmenu::run_verb`), so a verb
//! that waits on a slow file system (Google Drive's "Create shortcut" on `G:`) leaves the window
//! painting. The shell's menus are apartment-threaded, so the UI thread's menu object cannot be
//! handed over: a call through a proxy would run on the UI thread again.
//!
//! One job at a time, in order. `done` is called on the worker after each; the caller wakes the
//! UI thread with `slint::invoke_from_event_loop`. Nothing here touches app state.

use crate::shellmenu::{Invoke, VerbRequest};
use crossbeam_channel::{Receiver, Sender, unbounded};
use std::sync::atomic::{AtomicUsize, Ordering};
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, TranslateMessage, WM_APP, WM_QUIT,
};
use windows_sys::core::HRESULT;

/// Posted to the worker's thread when a job is queued.
const WM_RUN: u32 = WM_APP + 0x31;

/// Jobs queued or running, for the driver's `waitops`.
static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

/// A verb job is queued or running.
pub fn busy() -> bool {
    IN_FLIGHT.load(Ordering::Acquire) > 0
}

pub struct Job {
    pub id: u64,
    /// The pane whose menu it came from (the status bar's note).
    pub pane: usize,
    pub req: VerbRequest,
    /// The driver's test verb (`menuverb <pane> sleep:<ms>`): sleep on the worker this long, run
    /// nothing.
    pub sleep_ms: Option<u64>,
}

pub struct Done {
    pub id: u64,
    pub pane: usize,
    pub verb: String,
    pub label: String,
    /// How it was invoked; None for the test verb.
    pub how: Option<Invoke>,
    /// What `InvokeCommand` returned (0 when refused or slept).
    pub hr: HRESULT,
    pub ms: f64,
    /// The menu could not be built again (`run_verb`'s error).
    pub error: Option<String>,
}

/// Handle to the worker. Dropping it ends the worker's message loop.
pub struct Worker {
    tid: u32,
    tx: Sender<Job>,
}

impl Worker {
    pub fn spawn(done: impl Fn(Done) + Send + 'static) -> Worker {
        let (tx, rx) = unbounded::<Job>();
        let (tid_tx, tid_rx) = crossbeam_channel::bounded(1);
        std::thread::Builder::new()
            .name("verbs".into())
            .spawn(move || {
                // SAFETY: PeekMessage creates this thread's queue before its id is published, so
                // no WM_RUN can be lost to a missing queue.
                let tid = unsafe {
                    let mut msg: MSG = std::mem::zeroed();
                    PeekMessageW(&mut msg, std::ptr::null_mut(), WM_APP, WM_APP, PM_NOREMOVE);
                    GetCurrentThreadId()
                };
                let _ = tid_tx.send(tid);
                run_loop(rx, done);
            })
            .expect("spawn verbs thread");
        let tid = tid_rx.recv().unwrap_or(0);
        Worker { tid, tx }
    }

    /// Queue a job. False if the worker is gone.
    pub fn submit(&self, job: Job) -> bool {
        IN_FLIGHT.fetch_add(1, Ordering::AcqRel);
        if self.tx.send(job).is_err() {
            IN_FLIGHT.fetch_sub(1, Ordering::AcqRel);
            return false;
        }
        // SAFETY: posting to a thread id; fails harmlessly if the thread has exited.
        unsafe { PostThreadMessageW(self.tid, WM_RUN, 0, 0) != 0 }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let n = IN_FLIGHT.load(Ordering::Acquire);
        if n > 0 {
            // A verb's own dialog may still be up; the process exit ends it (no ASYNCOK hand-off
            // is asked for, so the shell holds no reference that needs the worker alive).
            eprintln!("[menu] quit while {n} verb job(s) queued or running: not waited for");
        }
        // SAFETY: as in `submit`.
        unsafe { PostThreadMessageW(self.tid, WM_QUIT, 0, 0) };
    }
}

fn run_loop(rx: Receiver<Job>, done: impl Fn(Done)) {
    // SAFETY: initialises COM for this thread as an STA; balanced by CoUninitialize below.
    let com = unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
    eprintln!("[menu] verb worker CoInitializeEx(APARTMENTTHREADED) = 0x{:08X}", com as u32);
    // SAFETY: a zeroed MSG is a valid out-buffer for GetMessageW.
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    loop {
        // SAFETY: standard message loop on this thread's queue; 0 is WM_QUIT, -1 an error. Shell
        // verbs post messages to this thread and some show their own windows on it.
        let r = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
        if r == 0 || r == -1 {
            break;
        }
        if msg.hwnd.is_null() && msg.message == WM_RUN {
            while let Ok(job) = rx.try_recv() {
                let d = run(job, com);
                IN_FLIGHT.fetch_sub(1, Ordering::AcqRel);
                done(d);
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

fn run(job: Job, com: HRESULT) -> Done {
    let t0 = std::time::Instant::now();
    let mut d = Done {
        id: job.id,
        pane: job.pane,
        verb: job.req.verb.clone(),
        label: job.req.label.clone(),
        how: None,
        hr: 0,
        ms: 0.0,
        error: None,
    };
    if let Some(ms) = job.sleep_ms {
        eprintln!("[menu] verb worker: test verb sleeps {ms} ms (id {})", job.id);
        std::thread::sleep(std::time::Duration::from_millis(ms));
    } else if com < 0 {
        d.error = Some(format!("CoInitializeEx failed: 0x{:08X}", com as u32));
    } else {
        match crate::shellmenu::run_verb(&job.req) {
            Ok((how, hr)) => {
                d.how = Some(how);
                d.hr = hr;
            }
            Err(e) => d.error = Some(e),
        }
    }
    d.ms = crate::perf::elapsed_ms(t0);
    let what = match (&d.how, &d.error) {
        (_, Some(e)) => format!("failed: {e}"),
        (Some(Invoke::Verb), _) => format!("by verb = 0x{:08X}", d.hr as u32),
        (Some(Invoke::Offset), _) => format!("by offset = 0x{:08X}", d.hr as u32),
        (Some(Invoke::Refuse(why)), _) => format!("refused: {why}"),
        (None, _) => "slept".into(),
    };
    eprintln!(
        "[menu] InvokeCommand verb=\"{}\" offset={} label=\"{}\" {what} in {:.0} ms (verb worker, id {})",
        d.verb, job.req.offset, d.label, d.ms, d.id
    );
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real verb end to end through the worker: "Create shortcut" (`link`) on a scratch file
    /// makes its shortcut beside it, by verb, with no dialog; the test verb only sleeps.
    #[test]
    fn worker_runs_a_real_verb_and_the_test_verb() {
        let dir = std::env::temp_dir().join(format!("twinfm-p7m1-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), b"x").unwrap();
        let (tx, rx) = crossbeam_channel::unbounded();
        let w = Worker::spawn(move |d| {
            let _ = tx.send(d);
        });
        let req = VerbRequest {
            folder: dir.clone(),
            names: vec!["a.txt".into()],
            verb: "link".into(),
            offset: 0,
            label: "Create shortcut".into(),
            x: 0,
            y: 0,
            extended: false,
        };
        assert!(w.submit(Job { id: 1, pane: 0, req: req.clone(), sleep_ms: Some(50) }));
        assert!(w.submit(Job { id: 2, pane: 0, req, sleep_ms: None }));
        let slept = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        assert_eq!((slept.id, slept.how.clone(), slept.hr), (1, None, 0));
        assert!(slept.ms >= 45.0, "slept {} ms", slept.ms);
        let done = rx.recv_timeout(std::time::Duration::from_secs(30)).unwrap();
        assert_eq!(done.id, 2);
        assert_eq!(done.error, None);
        assert_eq!(done.how, Some(Invoke::Verb));
        assert!(done.hr >= 0, "InvokeCommand = 0x{:08X}", done.hr as u32);
        let links: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")))
            .collect();
        assert_eq!(links.len(), 1, "one shortcut beside a.txt");
        assert!(!busy(), "nothing left in flight");
        drop(w);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
