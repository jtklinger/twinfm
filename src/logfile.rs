//! Logging as a setting (P11-M7). The exe is a windowed program, so its stderr lines (`[perf]`,
//! `[enum]` and the rest) go nowhere unless the launcher redirects them. With the `log` setting on,
//! TwinFM points its own standard error at a file (`SetStdHandle`): Rust's stderr looks the handle
//! up on every write, so every existing `eprintln!` lands there unchanged. Off (the default), nothing
//! is touched and a launcher's redirect (the gate's, the soak's) gets the lines as before.
//!
//! One file per run, `twinfm.log` in the settings folder; at start the last run's becomes
//! `twinfm.previous.log`. A run past `CUT_BYTES` starts a fresh `twinfm.log` the same way (checked
//! once a minute, `cut_if_over`). A file the log stops writing to is never closed while TwinFM runs:
//! a write racing the switch must not reach a handle Windows has given to something else.

use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const NAME: &str = "twinfm.log";
pub const PREVIOUS: &str = "twinfm.previous.log";
/// A run's log is cut past this many bytes.
pub const CUT_BYTES: u64 = 20 << 20;

// kernel32's standard handle calls, declared here rather than enabling windows-sys's Console feature
// for two functions.
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetStdHandle(id: u32) -> *mut c_void;
    fn SetStdHandle(id: u32, handle: *mut c_void) -> i32;
}
const STD_ERROR_HANDLE: u32 = -12i32 as u32;

struct Log {
    /// Standard error as the process started (null for a windowed program nobody redirected).
    original: usize,
    current: Option<(File, PathBuf)>,
    /// Files the log no longer writes to, kept open (module docs).
    retired: Vec<File>,
}

static LOG: Mutex<Option<Log>> = Mutex::new(None);

fn open(path: &Path) -> std::io::Result<File> {
    // Shared for delete too, so the file can be renamed to `PREVIOUS` while it is open (a cut).
    OpenOptions::new().create(true).write(true).truncate(true).share_mode(7).open(path)
}

/// Point standard error at `file`.
fn redirect(file: &File) {
    // SAFETY: a live file handle, kept open as long as it is standard error (module docs).
    unsafe { SetStdHandle(STD_ERROR_HANDLE, file.as_raw_handle() as *mut c_void) };
}

/// Start writing the log into `dir` (the settings folder), `first` its first line (the version and
/// the start time): the last run's `twinfm.log` becomes `twinfm.previous.log`. The log's path; when
/// it is already on, the file it writes to.
pub fn start(dir: &Path, first: &str) -> Result<PathBuf, String> {
    let mut g = LOG.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: a plain query of this process's standard error handle.
    let log = g.get_or_insert_with(|| Log { original: unsafe { GetStdHandle(STD_ERROR_HANDLE) } as usize, current: None, retired: Vec::new() });
    if let Some((_, p)) = &log.current {
        return Ok(p.clone());
    }
    let path = dir.join(NAME);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    if path.exists() {
        let _ = std::fs::rename(&path, dir.join(PREVIOUS));
    }
    let mut f = open(&path).map_err(|e| e.to_string())?;
    let _ = writeln!(f, "{first}");
    redirect(&f);
    log.current = Some((f, path.clone()));
    Ok(path)
}

/// Stop writing the log: standard error goes back to what it was at start. The file it stopped,
/// if it was on.
pub fn stop() -> Option<PathBuf> {
    let mut g = LOG.lock().unwrap_or_else(|e| e.into_inner());
    let log = g.as_mut()?;
    let (mut f, path) = log.current.take()?;
    let _ = writeln!(f, "[log] stopped");
    // SAFETY: the handle the process started with (or null), as it was.
    unsafe { SetStdHandle(STD_ERROR_HANDLE, log.original as *mut c_void) };
    log.retired.push(f);
    Some(path)
}

/// A run's log past `limit` bytes is cut: it becomes `twinfm.previous.log` and a fresh `twinfm.log`
/// takes the lines (`first` its first line). True when it cut.
pub fn cut_if_over(limit: u64, first: &str) -> bool {
    let mut g = LOG.lock().unwrap_or_else(|e| e.into_inner());
    let Some(log) = g.as_mut() else { return false };
    let Some((f, path)) = &log.current else { return false };
    if f.metadata().map_or(true, |m| m.len() <= limit) {
        return false;
    }
    let path = path.clone();
    let Some(dir) = path.parent() else { return false };
    if std::fs::rename(&path, dir.join(PREVIOUS)).is_err() {
        return false;
    }
    let Ok(mut fresh) = open(&path) else { return false };
    let _ = writeln!(fresh, "{first}");
    let _ = writeln!(fresh, "[log] cut at {} MB: the lines before are in {PREVIOUS}", limit >> 20);
    redirect(&fresh);
    if let Some((old, _)) = log.current.replace((fresh, path)) {
        log.retired.push(old);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_takes_stderr_rotates_and_cuts() {
        let dir = std::env::temp_dir().join(format!("twinfm-p11m7-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // A run before this one left its log.
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(NAME), "an earlier run\n").unwrap();
        let line = |s: &str| writeln!(std::io::stderr(), "{s}").unwrap();
        let path = start(&dir, "TwinFM test log").unwrap();
        assert_eq!(start(&dir, "again").unwrap(), path, "on already: the same file");
        line("[test] into the log");
        // Over a 10-byte limit: cut, the lines so far in the previous file.
        assert!(cut_if_over(10, "TwinFM test log, part 2"));
        line("[test] after the cut");
        assert!(!cut_if_over(CUT_BYTES, "x"), "under the limit: no cut");
        assert_eq!(stop(), Some(path.clone()));
        assert_eq!(stop(), None, "off already");
        line("PROBE after stop: not in the log");
        let now = std::fs::read_to_string(&path).unwrap();
        let prev = std::fs::read_to_string(dir.join(PREVIOUS)).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(prev.starts_with("TwinFM test log\n") && prev.contains("[test] into the log"), "{prev:?}");
        assert!(now.starts_with("TwinFM test log, part 2\n[log] cut at 0 MB"), "{now:?}");
        assert!(now.contains("[test] after the cut") && now.ends_with("[log] stopped\n") && !now.contains("PROBE"), "{now:?}");
        assert!(!prev.contains("an earlier run"), "the earlier run's file was the previous one, then replaced by the cut");
    }
}
