//! Folder sizes. One worker thread ("sizes") walks folders with the listing loop
//! (`enumerate::list_dir`) and adds up what is in them: every file, hidden and system files too
//! (disk usage, not what the list shows). It never enters a reparse point (junction, symbolic
//! link, cloud placeholder, app alias: counted, 0 bytes, not entered) and does not enter folders
//! deeper than `SIZE_MAX_DEPTH` (the result is then marked truncated).
//!
//! Requests go into one shared queue, served newest first like the icon workers'. Each carries a
//! cancel flag, read before every folder, and the tab's size generation, which the UI checks on
//! the reply (a reply for an older generation is dropped). While a walk runs, partial totals are
//! sent every `PARTIAL_EVERY`, so a big folder's number grows on screen.
//!
//! `TabSizes` is the per-tab bookkeeping on the UI thread (the cache, the requests in flight) and
//! is pure, so the selection's queueing rules are unit-tested.

use crossbeam_channel::Sender;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY;
use windows_sys::Win32::System::Diagnostics::Debug::{SEM_FAILCRITICALERRORS, SetThreadErrorMode};

use crate::model::Kind;

/// Folders deeper than this under the sized folder are not entered (the result is truncated).
pub const SIZE_MAX_DEPTH: usize = 64;
/// Partial totals at most this often per walk.
pub const PARTIAL_EVERY: Duration = Duration::from_millis(250);
/// At most this many selected folders are sized per selection; the rest stay unsized.
pub const SELECTION_CAP: usize = 100;
/// Alt+Shift+Return sizes at most this many folders of the listing.
pub const ALL_CAP: usize = 500;
/// A sized folder that changed is walked again no sooner than this, or twice its last walk's
/// duration if that is longer; further changes meanwhile wait for that one re-walk.
pub const REWALK_MIN: Duration = Duration::from_secs(2);

/// What a walk found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub bytes: u64,
    pub files: u64,
    pub folders: u64,
    /// A folder deeper than `SIZE_MAX_DEPTH` was not entered.
    pub truncated: bool,
    /// Folders that could not be listed (access denied, gone meanwhile).
    pub skipped: usize,
}

/// Add up `root`: every file's size, hidden and system files included. Reparse points are counted
/// (as folders or files by their attributes) at 0 bytes and never entered; folders deeper than
/// `max_depth` below `root` are counted and not entered. `cancel` is read before each folder;
/// `each_dir` is called after each folder is listed with the totals so far. None when cancelled.
/// A root that cannot be listed gives totals with `skipped` 1.
pub fn measure(root: &Path, max_depth: usize, cancel: &AtomicBool, mut each_dir: impl FnMut(&Totals)) -> Option<Totals> {
    let mut types = super::enumerate::TypeNames::default();
    let zone = crate::model::LocalZone::current();
    let mut t = Totals::default();
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let mut icon_keys = crate::icons::IconKeys::new(dir.clone(), false);
        let listed = super::enumerate::list_dir(&dir, &mut types, &mut icon_keys, &zone, |e| {
            if e.attrs & FILE_ATTRIBUTE_DIRECTORY != 0 {
                t.folders += 1;
                if e.kind == Kind::Directory {
                    if depth < max_depth {
                        stack.push((dir.join(e.name.as_str()), depth + 1));
                    } else {
                        t.truncated = true;
                    }
                }
            } else {
                t.files += 1;
                if e.kind == Kind::File {
                    t.bytes += e.size;
                }
            }
            true
        });
        if listed.is_err() {
            t.skipped += 1;
        }
        each_dir(&t);
    }
    Some(t)
}

/// A folder to size: the tab's folder joined with `name`.
pub struct Request {
    pub pane: usize,
    pub tab: u64,
    /// The tab's size generation when it was asked for.
    pub generation: u64,
    pub name: String,
    pub path: PathBuf,
    pub cancel: Arc<AtomicBool>,
}

/// Partial (`done` false) or final totals for a request.
pub struct Reply {
    pub pane: usize,
    pub tab: u64,
    pub generation: u64,
    pub name: String,
    pub cancel: Arc<AtomicBool>,
    pub totals: Totals,
    pub done: bool,
    /// The walk's duration so far, in ms.
    pub ms: u64,
}

/// The shared queue: the newest request is served first; cancelled ones are dropped when met.
#[derive(Default)]
pub struct Queue {
    items: Vec<Request>,
    closed: bool,
}

impl Queue {
    pub fn push(&mut self, r: Request) {
        self.items.push(r);
    }

    /// The newest request whose flag is not set.
    pub fn pop(&mut self) -> Option<Request> {
        while let Some(r) = self.items.pop() {
            if !r.cancel.load(Ordering::Relaxed) {
                return Some(r);
            }
        }
        None
    }

    /// Requests waiting, cancelled ones included until they are met.
    pub fn len(&self) -> usize {
        self.items.len()
    }
}

/// The UI thread's handle on the size worker. Dropping it lets the worker exit once its walk ends.
pub struct Sizer {
    queue: Arc<(Mutex<Queue>, Condvar)>,
}

impl Sizer {
    /// Start the worker. Replies go to `tx`, followed by `wake()`.
    pub fn spawn(tx: Sender<Reply>, wake: impl Fn() + Send + 'static) -> Sizer {
        let queue: Arc<(Mutex<Queue>, Condvar)> = Arc::default();
        let q = queue.clone();
        std::thread::Builder::new()
            .name("sizes".into())
            .spawn(move || {
                // No "insert a disk" dialog for an empty drive met on the way.
                // SAFETY: plain flag call on this (worker) thread; the old mode is not needed.
                unsafe { SetThreadErrorMode(SEM_FAILCRITICALERRORS, std::ptr::null_mut()) };
                while let Some(r) = next(&q) {
                    serve(r, &tx, &wake);
                }
            })
            .expect("spawn size thread");
        Sizer { queue }
    }

    /// Queue a request; returns the queue's length after it.
    pub fn push(&self, r: Request) -> usize {
        let (m, cv) = &*self.queue;
        let mut q = m.lock().unwrap_or_else(PoisonError::into_inner);
        q.push(r);
        cv.notify_one();
        q.len()
    }
}

impl Drop for Sizer {
    fn drop(&mut self) {
        let (m, cv) = &*self.queue;
        m.lock().unwrap_or_else(PoisonError::into_inner).closed = true;
        cv.notify_one();
    }
}

/// Wait for the next live request; None once the queue is closed.
fn next(q: &(Mutex<Queue>, Condvar)) -> Option<Request> {
    let (m, cv) = q;
    let mut g = m.lock().unwrap_or_else(PoisonError::into_inner);
    loop {
        if g.closed {
            return None;
        }
        if let Some(r) = g.pop() {
            return Some(r);
        }
        g = cv.wait(g).unwrap_or_else(PoisonError::into_inner);
    }
}

fn serve(r: Request, tx: &Sender<Reply>, wake: &impl Fn()) {
    let t0 = Instant::now();
    let mut last = Instant::now();
    let reply = |totals: Totals, done: bool| Reply {
        ms: t0.elapsed().as_millis() as u64,
        pane: r.pane,
        tab: r.tab,
        generation: r.generation,
        name: r.name.clone(),
        cancel: r.cancel.clone(),
        totals,
        done,
    };
    let res = measure(&r.path, SIZE_MAX_DEPTH, &r.cancel, |t| {
        if last.elapsed() >= PARTIAL_EVERY {
            last = Instant::now();
            eprintln!("[size] partial {}: {} bytes, {} files, {} folders", r.path.display(), t.bytes, t.files, t.folders);
            if tx.send(reply(*t, false)).is_ok() {
                wake();
            }
        }
    });
    // A cancelled walk sends nothing: the UI logged the cancel and forgot the request.
    let Some(t) = res else { return };
    eprintln!(
        "[size] done {}: {} bytes, {} files, {} folders in {:.0} ms ({} skipped){}",
        r.path.display(),
        t.bytes,
        t.files,
        t.folders,
        t0.elapsed().as_secs_f64() * 1e3,
        t.skipped,
        if t.truncated { ", truncated at depth 64" } else { "" }
    );
    if tx.send(reply(t, true)).is_ok() {
        wake();
    }
}

/// A request in flight, as the tab remembers it.
pub struct Pending {
    pub cancel: Arc<AtomicBool>,
    /// The latest partial totals.
    pub partial: Totals,
    /// Asked for by Alt+Shift+Return (not cancelled by a selection change).
    pub all: bool,
}

/// What a selection change asks of the worker.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// Folders to queue, in display order (pushed last first, so the first is served first).
    pub queue: Vec<String>,
    /// Requests from an earlier selection whose folder is no longer selected.
    pub cancel: Vec<String>,
    /// (sized, selected): more folders are selected than `SELECTION_CAP`.
    pub capped: Option<(usize, usize)>,
}

/// The selected folders' part of the status bar's middle text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirTotals {
    /// Selected folders (plain directories; links and drives count no bytes, as files do not).
    pub folders: usize,
    /// Their known bytes: finished sizes plus the partial totals of those still walking.
    pub bytes: u64,
    pub pending: usize,
    /// Selected folders that are neither sized nor being sized (past the cap).
    pub not_sized: usize,
    /// Selected folders whose last size is shown while it is out of date (a change below them:
    /// waiting for the re-walk, or being walked again).
    pub updating: usize,
}

/// One tab's folder sizes (UI thread only), keyed by the folder's name in the tab's folder.
#[derive(Default)]
pub struct TabSizes {
    /// Bumped whenever the cache is cleared; replies for an older one are dropped.
    pub generation: u64,
    pub cache: HashMap<String, Totals>,
    pub pending: HashMap<String, Pending>,
    /// Subtree watch overflows in a row; at two the tab's watch stays shallow.
    pub overflows: u8,
    /// Cached sizes a change has made out of date, with when they are walked again. The cached
    /// value stays (and is shown) until the re-walk lands.
    pub stale: HashMap<String, Instant>,
    /// Each sized folder's last walk duration, for the re-walk delay.
    pub walk_ms: HashMap<String, u64>,
    /// Folders Alt+Shift+Return sized: re-walked after a change even when not selected.
    pub from_all: HashSet<String>,
}

impl TabSizes {
    /// Sizes cached or in flight: the tab's folder is then watched with every folder below it.
    pub fn any(&self) -> bool {
        !self.cache.is_empty() || !self.pending.is_empty()
    }

    /// The first `cap` selected folders (display order) are sized; those neither cached nor in
    /// flight are queued. Requests a selection made (not Alt+Shift+Return's) for folders no longer
    /// selected are cancelled.
    pub fn plan(&self, selected: &[String], cap: usize) -> Plan {
        let set: HashSet<&str> = selected.iter().map(String::as_str).collect();
        let mut cancel: Vec<String> =
            self.pending.iter().filter(|(n, p)| !p.all && !set.contains(n.as_str())).map(|(n, _)| n.clone()).collect();
        cancel.sort();
        let first = &selected[..selected.len().min(cap)];
        let queue = first.iter().filter(|n| !self.cache.contains_key(*n) && !self.pending.contains_key(*n)).cloned().collect();
        let capped = (selected.len() > cap).then_some((cap, selected.len()));
        Plan { queue, cancel, capped }
    }

    pub fn summary(&self, selected: &[String]) -> DirTotals {
        let mut d = DirTotals { folders: selected.len(), ..Default::default() };
        for n in selected {
            if let Some(t) = self.cache.get(n) {
                d.bytes += t.bytes;
                if self.stale.contains_key(n) || self.pending.contains_key(n) {
                    d.updating += 1;
                }
            } else if let Some(p) = self.pending.get(n) {
                d.bytes += p.partial.bytes;
                d.pending += 1;
            } else {
                d.not_sized += 1;
            }
        }
        d
    }

    /// A change somewhere below the tab's folder, `child` being the first component of its path
    /// (the folder's own name for `sub\dir\file.txt`): mark that folder's cached size stale, to be
    /// walked again after `rewalk_delay`. Returns the cached name and the delay when it was not
    /// stale yet; None when it has no size or is already waiting (the change joins that re-walk).
    /// Names compare ignoring ASCII case, as the file system does.
    pub fn invalidate(&mut self, child: &str, now: Instant) -> Option<(String, Duration)> {
        let key = self.cache.keys().find(|k| k.eq_ignore_ascii_case(child))?.clone();
        if self.stale.contains_key(&key) {
            return None;
        }
        let delay = rewalk_delay(self.walk_ms.get(&key).copied().unwrap_or(0));
        self.stale.insert(key.clone(), now + delay);
        Some((key, delay))
    }

    /// The stale folders whose re-walk is due at `now`, taken out of `stale`.
    pub fn due(&mut self, now: Instant) -> Vec<String> {
        let mut due: Vec<String> = self.stale.iter().filter(|(_, t)| **t <= now).map(|(n, _)| n.clone()).collect();
        due.sort();
        for n in &due {
            self.stale.remove(n);
        }
        due
    }

    /// When the next re-walk is due.
    pub fn next_due(&self) -> Option<Instant> {
        self.stale.values().min().copied()
    }

    /// Forget one folder's size (it is no longer wanted, or out of date and not walked again).
    pub fn forget(&mut self, name: &str) -> bool {
        self.stale.remove(name);
        self.walk_ms.remove(name);
        self.from_all.remove(name);
        self.cache.remove(name).is_some()
    }

    /// The tab goes to the background (P6-M3): its finished sizes stay (its watch there is a subtree
    /// watch while it has any), its walks in flight are cancelled and its out-of-date sizes, which
    /// would be walked again, are forgotten. Returns both lists of names, for the log.
    pub fn to_background(&mut self) -> (Vec<String>, Vec<String>) {
        let mut cancelled: Vec<String> = self
            .pending
            .drain()
            .map(|(n, p)| {
                p.cancel.store(true, Ordering::Relaxed);
                n
            })
            .collect();
        cancelled.sort();
        let mut stale: Vec<String> = self.stale.keys().cloned().collect();
        stale.sort();
        for n in &stale {
            self.forget(n);
        }
        (cancelled, stale)
    }

    /// Forget everything: cancel every request (returned, for the log) and empty the cache
    /// (returns how many sizes it held). Bumps the generation.
    pub fn clear(&mut self) -> (usize, Vec<String>) {
        let mut cancelled: Vec<String> = self
            .pending
            .drain()
            .map(|(n, p)| {
                p.cancel.store(true, Ordering::Relaxed);
                n
            })
            .collect();
        cancelled.sort();
        let cached = self.cache.len();
        self.cache.clear();
        self.stale.clear();
        self.walk_ms.clear();
        self.from_all.clear();
        self.generation += 1;
        (cached, cancelled)
    }
}

/// How long a changed folder waits before it is walked again: `REWALK_MIN`, or twice its last
/// walk's duration if that is longer.
pub fn rewalk_delay(last_walk_ms: u64) -> Duration {
    REWALK_MIN.max(Duration::from_millis(last_walk_ms.saturating_mul(2)))
}

/// The first component of a path relative to the watched folder (`sub` for `sub\dir\x.txt`).
pub fn child_of(relative: &str) -> &str {
    relative.split('\\').next().unwrap_or(relative)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM, SetFileAttributesW};

    fn set_attrs(p: &Path, attrs: u32) {
        let w: Vec<u16> = p.as_os_str().encode_wide().chain([0]).collect();
        // SAFETY: NUL-terminated path for the duration of the call.
        assert_ne!(unsafe { SetFileAttributesW(w.as_ptr(), attrs) }, 0);
    }

    fn write(p: &Path, n: usize) {
        std::fs::write(p, vec![b'x'; n]).unwrap();
    }

    #[test]
    fn a_background_tab_keeps_its_finished_sizes() {
        let mut s = TabSizes::default();
        let t = |bytes| Totals { bytes, ..Default::default() };
        s.cache.insert("done".into(), t(10));
        s.cache.insert("old".into(), t(20));
        let flag = Arc::new(AtomicBool::new(false));
        s.pending.insert("walking".into(), Pending { cancel: flag.clone(), partial: t(5), all: false });
        assert!(s.invalidate("OLD", Instant::now()).is_some());
        let generation = s.generation;
        let (cancelled, stale) = s.to_background();
        assert_eq!((cancelled, stale), (vec!["walking".to_string()], vec!["old".to_string()]));
        assert!(flag.load(Ordering::Relaxed), "the walk in flight is told to stop");
        assert_eq!(s.cache.keys().collect::<Vec<_>>(), ["done"], "the finished size stays");
        assert!(s.any(), "so the background watch stays a subtree watch");
        assert_eq!(s.generation, generation, "nothing cleared: the cached sizes are still current");
    }

    #[test]
    fn walker_counts_hidden_files_and_skips_links() {
        let dir = std::env::temp_dir().join(format!("twinfm-p3m3-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("root");
        std::fs::create_dir_all(root.join(r"sub\deep\deeper")).unwrap();
        write(&root.join("a.txt"), 100);
        write(&root.join("h.txt"), 50);
        write(&root.join("s.txt"), 25);
        write(&root.join(r"sub\b.bin"), 1000);
        write(&root.join(r"sub\deep\c"), 10);
        write(&root.join(r"sub\deep\deeper\d"), 7);
        set_attrs(&root.join("h.txt"), FILE_ATTRIBUTE_HIDDEN);
        set_attrs(&root.join("s.txt"), FILE_ATTRIBUTE_SYSTEM);
        // A junction to an ancestor: entering it would loop until the depth limit.
        let made = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(root.join(r"sub\loop"))
            .arg(&root)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        // An unreadable folder: deny Everyone the listing (the owner can still change the ACL).
        let locked = root.join("locked");
        std::fs::create_dir(&locked).unwrap();
        write(&locked.join("x"), 3);
        let denied = std::process::Command::new("icacls")
            .arg(&locked)
            .args(["/deny", "*S-1-1-0:(RD)"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        let never = AtomicBool::new(false);
        let mut dirs = 0;
        let all = measure(&root, SIZE_MAX_DEPTH, &never, |_| dirs += 1).expect("not cancelled");
        // Two levels only: `deeper` is counted, not entered.
        let shallow = measure(&root, 2, &never, |_| {}).expect("not cancelled");
        // Cancelled after the first folder.
        let stop = AtomicBool::new(false);
        let mut seen = 0;
        let cancelled = measure(&root, SIZE_MAX_DEPTH, &stop, |_| {
            seen += 1;
            stop.store(true, Ordering::Relaxed);
        });
        let missing = measure(&dir.join("not-there"), SIZE_MAX_DEPTH, &never, |_| {});

        if denied {
            let _ = std::process::Command::new("icacls").arg(&locked).args(["/remove:d", "*S-1-1-0"]).output();
        }
        std::fs::remove_dir_all(&dir).unwrap(); // removes the junction, not its target's contents

        assert!(made, "mklink /J failed");
        // a, h (hidden), s (system), b, c, d, and x when the locked folder could be read.
        let x = if denied { 0 } else { 3 };
        assert_eq!(all.bytes, 100 + 50 + 25 + 1000 + 10 + 7 + x);
        assert_eq!(all.files, 6 + u64::from(!denied));
        // sub, deep, deeper, loop (the junction, not entered), locked.
        assert_eq!(all.folders, 5);
        assert!(!all.truncated);
        // root, sub, deep, deeper, locked: the junction is never listed.
        assert_eq!(dirs, 5);
        if denied {
            assert_eq!(all.skipped, 1, "the locked folder is skipped");
        } else {
            eprintln!("note: icacls could not deny the listing; the skipped count was not checked");
        }
        assert!(shallow.truncated);
        assert_eq!(shallow.bytes, all.bytes - 7);
        assert_eq!(cancelled, None);
        assert_eq!(seen, 1);
        assert_eq!(missing, Some(Totals { skipped: 1, ..Default::default() }));
    }

    fn req(name: &str) -> Request {
        Request { pane: 0, tab: 1, generation: 1, name: name.into(), path: name.into(), cancel: Arc::default() }
    }

    #[test]
    fn queue_serves_newest_first_and_drops_cancelled() {
        let mut q = Queue::default();
        let b = req("b");
        let flag = b.cancel.clone();
        q.push(req("a"));
        q.push(b);
        q.push(req("c"));
        assert_eq!(q.len(), 3);
        flag.store(true, Ordering::Relaxed);
        assert_eq!(q.pop().map(|r| r.name), Some("c".into()));
        assert_eq!(q.pop().map(|r| r.name), Some("a".into()), "the cancelled b is dropped");
        assert!(q.pop().is_none());
    }

    #[test]
    fn selection_plan_caps_and_cancels() {
        let names: Vec<String> = (0..150).map(|i| format!("d{i:03}")).collect();
        let mut s = TabSizes::default();
        let p = s.plan(&names, SELECTION_CAP);
        assert_eq!(p.queue.len(), 100);
        assert_eq!(p.queue.first().map(String::as_str), Some("d000"));
        assert_eq!(p.queue.last().map(String::as_str), Some("d099"));
        assert_eq!(p.capped, Some((100, 150)));
        // Two sized, one walking, one asked for by Alt+Shift+Return.
        s.cache.insert("d000".into(), Totals { bytes: 10, ..Default::default() });
        s.cache.insert("d001".into(), Totals { bytes: 5, ..Default::default() });
        let partial = Totals { bytes: 7, ..Default::default() };
        s.pending.insert("d002".into(), Pending { cancel: Arc::default(), partial, all: false });
        s.pending.insert("zz".into(), Pending { cancel: Arc::default(), partial: Totals::default(), all: true });
        let d = s.summary(&names);
        assert_eq!(d, DirTotals { folders: 150, bytes: 22, pending: 1, not_sized: 147, updating: 0 });
        // d000 changed: its old size still counts, as updating.
        s.stale.insert("d000".into(), Instant::now());
        assert_eq!(s.summary(&names[..2]), DirTotals { folders: 2, bytes: 15, pending: 0, not_sized: 0, updating: 1 });
        // A new selection: d001 and d003; d002 (selection) is cancelled, zz (Alt+Shift+Return) is not.
        let p = s.plan(&["d001".into(), "d003".into()], SELECTION_CAP);
        assert_eq!(p, Plan { queue: vec!["d003".into()], cancel: vec!["d002".into()], capped: None });
    }

    #[test]
    fn cache_invalidates_by_first_component() {
        let mut s = TabSizes::default();
        s.cache.insert("Sub".into(), Totals::default());
        s.cache.insert("sub2".into(), Totals::default());
        assert_eq!(child_of(r"sub\deep\x.txt"), "sub");
        assert_eq!(child_of("top.txt"), "top.txt");
        let t0 = Instant::now();
        s.walk_ms.insert("Sub".into(), 15_000);
        assert_eq!(s.invalidate(child_of(r"sub\deep\x.txt"), t0), Some(("Sub".into(), Duration::from_secs(30))), "case-insensitive; twice the walk");
        assert_eq!(s.invalidate(child_of(r"sub\deep\y.txt"), t0), None, "joins the pending re-walk");
        assert!(s.cache.contains_key("Sub"), "the old size stays until the re-walk lands");
        assert!(!s.stale.contains_key("sub2"), "a longer name is not under sub");
        assert_eq!(s.invalidate(child_of(r"sub2\y"), t0), Some(("sub2".into(), REWALK_MIN)), "at least 2 s");
        assert_eq!(s.invalidate("nothing", t0), None);
        assert_eq!(s.next_due(), Some(t0 + REWALK_MIN));
        assert!(s.due(t0 + Duration::from_secs(1)).is_empty());
        assert_eq!(s.due(t0 + REWALK_MIN), vec!["sub2".to_string()]);
        assert_eq!(s.due(t0 + Duration::from_secs(30)), vec!["Sub".to_string()]);
        assert_eq!(s.next_due(), None);
        assert_eq!(rewalk_delay(500), REWALK_MIN);
        assert_eq!(rewalk_delay(1500), Duration::from_secs(3));
        let flag: Arc<AtomicBool> = Arc::default();
        s.pending.insert("p".into(), Pending { cancel: flag.clone(), partial: Totals::default(), all: false });
        s.cache.insert("c".into(), Totals::default());
        let g = s.generation;
        assert_eq!(s.clear(), (3, vec!["p".into()]), "Sub, sub2 (their old sizes stayed) and c");
        assert!(flag.load(Ordering::Relaxed) && s.cache.is_empty() && s.pending.is_empty() && s.walk_ms.is_empty());
        assert_eq!(s.generation, g + 1);
    }
}
