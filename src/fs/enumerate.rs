//! Worker-thread directory enumeration. FindFirstFileExW/FindNextFileW -> Vec<Entry> batches ->
//! channel.
//! Never runs on the UI thread. The UI consumes batches via slint::invoke_from_event_loop.
//! The pseudo-folder "This PC" (or an empty path) lists the drives instead.

use crate::model::{Entry, Kind};
use crossbeam_channel::Sender;
use slint::SharedString;
use std::cell::Cell;
use std::collections::HashMap;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, FILETIME, GetLastError, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SPARSE_FILE,
    FILE_ATTRIBUTE_UNPINNED, FIND_FIRST_EX_LARGE_FETCH, FindClose, FindExInfoBasic,
    FindExSearchNameMatch, FindFirstFileExW, FindNextFileW, GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives,
    GetVolumeInformationW, WIN32_FIND_DATAW,
};
use windows_sys::Win32::System::Diagnostics::Debug::{SEM_FAILCRITICALERRORS, SetThreadErrorMode};
use windows_sys::Win32::UI::Shell::{ASSOCF_NOTRUNCATE, ASSOCSTR_FRIENDLYDOCNAME, AssocQueryStringW};

pub const BATCH_SIZE: usize = 2000;

pub enum Msg {
    Batch { pane: usize, generation: u64, entries: Vec<Entry> },
    Done {
        pane: usize,
        generation: u64,
        total: usize,
        elapsed: Duration,
        error: Option<String>,
        free: Option<(u64, u64)>,
        reparse: ReparseCounts,
        /// A level of a zip archive: what its central directory read cost (P3-M4).
        archive: Option<crate::fs::archive::LevelInfo>,
    },
}

/// Reparse points in a listing by kind, for the `[enum]` line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReparseCounts {
    pub junction: usize,
    pub symlink: usize,
    pub cloud: usize,
    pub alias: usize,
    pub other: usize,
}

impl ReparseCounts {
    fn add(&mut self, kind: Kind) {
        match kind {
            Kind::Junction => self.junction += 1,
            Kind::Symlink => self.symlink += 1,
            Kind::Cloud => self.cloud += 1,
            Kind::AppAlias => self.alias += 1,
            Kind::Reparse => self.other += 1,
            Kind::Directory | Kind::File | Kind::Drive => {}
        }
    }

    pub fn total(&self) -> usize {
        self.junction + self.symlink + self.cloud + self.alias + self.other
    }
}

/// List `path` on a worker thread; `thumbnails`: thumbnail files get their own icon keys
/// (`icons::IconKeys`); `cloud`: the folder's cloud location, which marks the keys.
pub fn spawn(path: PathBuf, pane: usize, generation: u64, thumbnails: bool, cloud: crate::icons::Cloud, tx: Sender<Msg>, wake: impl Fn() + Send + 'static) {
    std::thread::Builder::new()
        .name(format!("enum-{pane}-{generation}"))
        .spawn(move || run(path, pane, generation, thumbnails, cloud, tx, wake))
        .expect("spawn enumeration thread");
}

/// The drive list's pseudo-path, shown in the path bar.
pub const THIS_PC: &str = "This PC";

/// An empty path or "This PC" (any case) means the drive list.
pub fn is_this_pc(path: &str) -> bool {
    let t = path.trim();
    t.is_empty() || t.eq_ignore_ascii_case(THIS_PC)
}

fn run(path: PathBuf, pane: usize, generation: u64, thumbnails: bool, cloud: crate::icons::Cloud, tx: Sender<Msg>, wake: impl Fn()) {
    let t0 = Instant::now();
    if is_this_pc(&path.to_string_lossy()) {
        let entries = drive_entries();
        let total = entries.len();
        let _ = tx.send(Msg::Batch { pane, generation, entries });
        let reparse = ReparseCounts::default();
        let _ = tx.send(Msg::Done { pane, generation, total, elapsed: t0.elapsed(), error: None, free: None, reparse, archive: None });
        wake();
        return;
    }
    if let Some((zip, inner)) = crate::fs::archive::split(&path) {
        return run_archive(&zip, &inner, pane, generation, tx, wake, t0);
    }
    // A sync root is listed with its placeholders shown, for each file's own state (P12-M7).
    let sync_root = cloud == crate::icons::Cloud::SyncRoot;
    if sync_root {
        expose_placeholders("listing");
    }
    let mut types = TypeNames::default();
    let mut icon_keys = crate::icons::IconKeys::new(path.clone(), thumbnails).with_cloud(cloud);
    let zone = crate::model::LocalZone::current();
    let mut batch: Vec<Entry> = Vec::with_capacity(BATCH_SIZE);
    let mut total = 0usize;
    let mut reparse = ReparseCounts::default();
    let (mut files, mut unsynced) = (0usize, 0usize);
    let listed = list_dir(&path, &mut types, &mut icon_keys, &zone, |e| {
        reparse.add(e.kind);
        if sync_root && e.attrs & FILE_ATTRIBUTE_DIRECTORY == 0 {
            files += 1;
            unsynced += usize::from(e.icon_key.starts_with("look:"));
        }
        batch.push(e);
        if batch.len() >= BATCH_SIZE {
            total += batch.len();
            let full = std::mem::replace(&mut batch, Vec::with_capacity(BATCH_SIZE));
            if tx.send(Msg::Batch { pane, generation, entries: full }).is_err() {
                return false;
            }
            wake();
        }
        true
    });
    match listed {
        Err(code) => {
            let error = Some(std::io::Error::from_raw_os_error(code as i32).to_string());
            let reparse = ReparseCounts::default();
            let _ = tx.send(Msg::Done { pane, generation, total: 0, elapsed: t0.elapsed(), error, free: None, reparse, archive: None });
            wake();
            return;
        }
        Ok(false) => return,
        Ok(true) => {}
    }
    if sync_root {
        eprintln!("[cloud] pane={pane} not in sync: {unsynced} of {files} files");
    }
    if !batch.is_empty() {
        total += batch.len();
        let _ = tx.send(Msg::Batch { pane, generation, entries: batch });
    }
    // Timed before the free-space query, so the [enum] worker time is the listing alone.
    let elapsed = t0.elapsed();
    let free = crate::statusbar::disk_free(&path);
    let _ = tx.send(Msg::Done { pane, generation, total, elapsed, error: None, free, reparse, archive: None });
    wake();
}

/// A level of a zip archive, streamed in batches as a folder's listing is; no free space.
fn run_archive(zip: &Path, inner: &str, pane: usize, generation: u64, tx: Sender<Msg>, wake: impl Fn(), t0: Instant) {
    let reparse = ReparseCounts::default();
    let zone = crate::model::LocalZone::current();
    let (entries, info) = match crate::fs::archive::list(zip, inner, &mut TypeNames::default(), &zone) {
        Ok(l) => l,
        Err(e) => {
            let error = Some(e);
            let _ = tx.send(Msg::Done { pane, generation, total: 0, elapsed: t0.elapsed(), error, free: None, reparse, archive: None });
            return wake();
        }
    };
    let total = entries.len();
    let mut entries = entries.into_iter();
    loop {
        let batch: Vec<Entry> = entries.by_ref().take(BATCH_SIZE).collect();
        if batch.is_empty() {
            break;
        }
        if tx.send(Msg::Batch { pane, generation, entries: batch }).is_err() {
            return;
        }
        wake();
    }
    let elapsed = t0.elapsed();
    let _ = tx.send(Msg::Done { pane, generation, total, elapsed, error: None, free: None, reparse, archive: Some(info) });
    wake();
}

/// One folder's entries through FindFirstFileExW/FindNextFileW (`.` and `..` left out), each
/// handed to `f`; `f` returning false stops the listing (Ok(false)). Err with the Win32 error code
/// when the folder cannot be listed; ERROR_FILE_NOT_FOUND from the first call (a drive root with
/// nothing in it has no "." entry either) is an empty listing, as read_dir gave. The one listing
/// loop: the folder enumeration, the recursive search and the folder sizes (`fs::sizes`) use it.
pub(crate) fn list_dir(
    path: &Path,
    types: &mut TypeNames,
    icon_keys: &mut crate::icons::IconKeys,
    zone: &crate::model::LocalZone,
    mut f: impl FnMut(Entry) -> bool,
) -> Result<bool, u32> {
    let mut data = WIN32_FIND_DATAW::default();
    let pattern = find_pattern(path);
    // SAFETY: `pattern` is a NUL-terminated wide string and `data` a WIN32_FIND_DATAW, both alive
    // for the call. Basic info (no 8.3 name) and large fetch.
    let find = unsafe {
        FindFirstFileExW(
            pattern.as_ptr(),
            FindExInfoBasic,
            &mut data as *mut WIN32_FIND_DATAW as *mut std::ffi::c_void,
            FindExSearchNameMatch,
            std::ptr::null(),
            FIND_FIRST_EX_LARGE_FETCH,
        )
    };
    if find == INVALID_HANDLE_VALUE {
        // SAFETY: no arguments; read right after the failing call on this thread.
        let code = unsafe { GetLastError() };
        return if code == ERROR_FILE_NOT_FOUND { Ok(true) } else { Err(code) };
    }
    let mut complete = true;
    loop {
        if let Some(e) = entry_of(&data, types, icon_keys, zone) {
            if !f(e) {
                complete = false;
                break;
            }
        }
        // SAFETY: `find` is a live search handle and `data` is writable. Any failure ends the
        // listing (ERROR_NO_MORE_FILES at the end; read_dir's iterator skipped other errors).
        if unsafe { FindNextFileW(find, &mut data) } == 0 {
            break;
        }
    }
    // SAFETY: `find` is the live search handle from FindFirstFileExW, closed exactly once.
    unsafe { FindClose(find) };
    Ok(complete)
}

/// One entry of `dir` by name, as `list_dir` would have built it: `FindFirstFileExW` on the exact
/// path (no wildcard), so a name with wildcard characters is looked up as itself. None when the
/// file is gone or cannot be read. The name returned is the on-disk casing (`cFileName`). A name
/// that is empty, `.`, `..`, or holds a separator, a colon (a stream or a drive) or a character
/// the find call reads as a wildcard (`* ? < > "`; no Win32 file name holds any of these) gives
/// None: it names no entry of `dir`.
pub fn stat_entry(
    dir: &Path,
    name: &str,
    types: &mut TypeNames,
    icon_keys: &mut crate::icons::IconKeys,
    zone: &crate::model::LocalZone,
) -> Option<Entry> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['\\', '/', ':', '*', '?', '<', '>', '"']) {
        return None;
    }
    // Always verbatim (`\\?\`): without it a name ending in a dot or a space is normalised to
    // another name. So Win32 normalises nothing in `dir` either: it must be absolute, with
    // backslashes and no `.` or `..` (the edit worker's `stat_dir`); `\\?\C:/x\name` names nothing.
    let path = long_wide(dir.join(name).as_os_str().encode_wide().collect(), true);
    let mut data = WIN32_FIND_DATAW::default();
    // SAFETY: `path` is a NUL-terminated wide string and `data` a WIN32_FIND_DATAW, both alive for
    // the call. Basic info (no 8.3 name), no large fetch: one entry.
    let find = unsafe {
        FindFirstFileExW(
            path.as_ptr(),
            FindExInfoBasic,
            &mut data as *mut WIN32_FIND_DATAW as *mut std::ffi::c_void,
            FindExSearchNameMatch,
            std::ptr::null(),
            0,
        )
    };
    if find == INVALID_HANDLE_VALUE {
        return None;
    }
    // SAFETY: `find` is the live search handle from FindFirstFileExW, closed exactly once.
    unsafe { FindClose(find) };
    entry_of(&data, types, icon_keys, zone)
}

/// The recursive name search's batches: at most this many entries, or whatever matched in the last
/// `SEARCH_FLUSH`, whichever comes first, so results stream in.
pub const SEARCH_BATCH: usize = 256;
pub const SEARCH_FLUSH: Duration = Duration::from_millis(100);
/// Folders deeper than this under the search's root are not entered.
pub const SEARCH_MAX_DEPTH: usize = 64;

/// A search's cancel flag: 0 while it runs; the UI thread stores why it stopped it.
pub const SEARCH_RUNNING: u8 = 0;
pub const SEARCH_CLOSED: u8 = 1;
pub const SEARCH_NAVIGATED: u8 = 2;
pub const SEARCH_REPLACED: u8 = 3;
pub const SEARCH_ESCAPE: u8 = 4;
pub const SEARCH_REFRESH: u8 = 5;

pub fn cancel_reason(code: u8) -> &'static str {
    match code {
        SEARCH_CLOSED => "tab closed",
        SEARCH_NAVIGATED => "navigated away",
        SEARCH_REPLACED => "new search",
        SEARCH_ESCAPE => "Escape",
        SEARCH_REFRESH => "F5",
        _ => "stopped",
    }
}

/// What a walk did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WalkStats {
    /// Entries that matched and were handed out.
    pub found: usize,
    /// Folders listed, the root included, and folders that could not be listed (skipped).
    pub walked: usize,
    pub skipped: usize,
    /// The cancel flag's value when the walk stopped early (None: it ran to the end).
    pub cancelled: Option<u8>,
    /// A content search (P11-M1): files opened and searched, files left unread (over the cap, binary
    /// by their first bytes, cloud-only by their attributes, unreadable), and folders under a sync
    /// root that were not entered.
    pub read: usize,
    pub over_cap: usize,
    pub binary: usize,
    pub cloud_only: usize,
    pub unreadable: usize,
    pub cloud_folders: usize,
}

impl WalkStats {
    /// Files a content search left unread.
    pub fn files_skipped(&self) -> usize {
        self.over_cap + self.binary + self.cloud_only + self.unreadable
    }
}

/// What a content search looks for (P11-M1): the text, `content::fold`ed, and the sync roots
/// (`cloud::normalise`d) whose folders the walk never enters, since reading a file there could
/// download it.
#[derive(Clone, Debug, Default)]
pub struct Content {
    pub needle: String,
    pub cloud_roots: Vec<String>,
}

impl Content {
    fn in_cloud(&self, dir: &Path) -> bool {
        let d = crate::cloud::normalise(&dir.to_string_lossy());
        self.cloud_roots.iter().any(|r| crate::cloud::under(&d, r))
    }

    /// Read file `e` at `path`: true when it holds the text (its row's line set in `found`).
    fn holds(&self, e: &mut Entry, path: &Path, stats: &mut WalkStats) -> bool {
        use crate::fs::content::{Outcome, cloud_only, search_file};
        if e.is_dir() {
            return false;
        }
        if e.kind == Kind::Cloud || cloud_only(e.attrs) {
            stats.cloud_only += 1;
            return false;
        }
        // Links, app aliases and other reparse points are not followed, as the name walk's folders.
        if e.kind != Kind::File {
            return false;
        }
        match search_file(path, e.size, &self.needle) {
            Outcome::Found(line) => {
                stats.read += 1;
                e.found = line.into();
                true
            }
            Outcome::Missed => {
                stats.read += 1;
                false
            }
            Outcome::OverCap => {
                stats.over_cap += 1;
                false
            }
            Outcome::Binary => {
                stats.binary += 1;
                false
            }
            Outcome::Unreadable => {
                stats.unreadable += 1;
                false
            }
        }
    }
}

/// The recursive name search: list `root` and every folder under it with `list_dir`, depth first,
/// and hand each entry whose own name `matcher` accepts to `emit`, in batches of up to
/// `SEARCH_BATCH` or every `SEARCH_FLUSH`. An entry's name is its path relative to `root`
/// (`sub\dir\file.txt`). Never enters a reparse point (junction, symbolic link, cloud
/// placeholder, app alias: only `Kind::Directory` is entered, so a loop through a link cannot
/// happen), a folder the tab's hidden-file switches keep out (`model::shows`), or a folder deeper than
/// `SEARCH_MAX_DEPTH`. Hidden matches in the folders it enters are handed out too (the tab's
/// hidden-file switch shows or hides them, as in a folder). `cancel` is read before each folder
/// and after each batch. `emit` returning false (the receiver is gone) ends the walk. Err with
/// the error text when `root` itself cannot be listed; a folder below it that cannot be listed is
/// counted as skipped. `thumbnails`: a result in `THUMB_EXTS` gets its `thumb:` key, as in a folder
/// (P9-M4; the caller passes false for a cloud location's root, where a thumbnail could download).
/// `content` (P11-M1): a content search, whose results are the plain files `matcher` accepts that
/// hold its text, each read once (`Content::holds`), and which enters no folder under a sync root;
/// `cancel` is read before each file it opens too.
pub fn walk(
    root: &Path,
    matcher: &crate::model::NameMatch,
    show: crate::model::Show,
    thumbnails: bool,
    content: Option<&Content>,
    cancel: &std::sync::atomic::AtomicU8,
    mut emit: impl FnMut(Vec<Entry>) -> bool,
) -> Result<WalkStats, String> {
    use std::sync::atomic::Ordering;
    // Inside a zip archive the walk is a pass over its cached levels.
    if let Some((zip, inner)) = crate::fs::archive::split(root) {
        let (found, walked) = crate::fs::archive::walk(&zip, &inner, matcher, emit)?;
        return Ok(WalkStats { found, walked, ..Default::default() });
    }
    let mut types = TypeNames::default();
    let zone = crate::model::LocalZone::current();
    let mut stats = WalkStats::default();
    let mut batch: Vec<Entry> = Vec::new();
    let mut flushed = Instant::now();
    // (folder, its path relative to the root with a trailing `\`, depth): depth first.
    let mut stack: Vec<(PathBuf, String, usize)> = vec![(root.to_path_buf(), String::new(), 0)];
    let mut gone = false;
    while let Some((dir, rel, depth)) = stack.pop() {
        let code = cancel.load(Ordering::Relaxed);
        if code != SEARCH_RUNNING {
            stats.cancelled = Some(code);
            break;
        }
        let mut icon_keys = crate::icons::IconKeys::new(dir.clone(), thumbnails);
        let listed = list_dir(&dir, &mut types, &mut icon_keys, &zone, |mut e| {
            let leaf = e.name.clone();
            let enter = e.kind == Kind::Directory && depth < SEARCH_MAX_DEPTH && crate::model::shows(&e, show);
            if enter {
                let sub = dir.join(leaf.as_str());
                if content.is_some_and(|c| c.in_cloud(&sub)) {
                    stats.cloud_folders += 1;
                } else {
                    stack.push((sub, format!("{rel}{leaf}\\"), depth + 1));
                }
            }
            let hit = match content {
                None => matcher.matches(&leaf),
                Some(c) => {
                    if !matcher.matches(&leaf) || e.is_dir() {
                        false
                    } else if cancel.load(Ordering::Relaxed) != SEARCH_RUNNING {
                        return false;
                    } else {
                        c.holds(&mut e, &dir.join(leaf.as_str()), &mut stats)
                    }
                }
            };
            if hit {
                e.name = format!("{rel}{leaf}").into();
                batch.push(e);
                stats.found += 1;
            }
            if batch.len() >= SEARCH_BATCH || (!batch.is_empty() && flushed.elapsed() >= SEARCH_FLUSH) {
                flushed = Instant::now();
                if !emit(std::mem::take(&mut batch)) {
                    gone = true;
                    return false;
                }
                // Stop within the folder too: a single folder can hold tens of thousands of entries.
                if cancel.load(Ordering::Relaxed) != SEARCH_RUNNING {
                    return false;
                }
            }
            true
        });
        match listed {
            Err(code) if depth == 0 => return Err(std::io::Error::from_raw_os_error(code as i32).to_string()),
            Err(_) => stats.skipped += 1,
            Ok(_) => stats.walked += 1,
        }
        if gone {
            return Ok(stats);
        }
        if !batch.is_empty() && flushed.elapsed() >= SEARCH_FLUSH {
            flushed = Instant::now();
            if !emit(std::mem::take(&mut batch)) {
                return Ok(stats);
            }
        }
    }
    // A flag set during the last folder's listing stopped it part way.
    let code = cancel.load(Ordering::Relaxed);
    if stats.cancelled.is_none() && code != SEARCH_RUNNING {
        stats.cancelled = Some(code);
    }
    if !batch.is_empty() {
        emit(batch);
    }
    Ok(stats)
}

/// Run `walk` on a worker thread for a search tab: its batches go to the enumeration channel as
/// `Msg::Batch` with the tab's generation (so the UI handles them as a listing's), then one
/// `Msg::Done` with the count, the time and the error if the root could not be listed. Logs the
/// `[search]` batch (at most ten a second), done and cancelled lines.
pub fn spawn_search(
    root: PathBuf,
    matcher: crate::model::NameMatch,
    show: crate::model::Show,
    thumbnails: bool,
    content: Option<Content>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicU8>,
    pane: usize,
    generation: u64,
    tx: Sender<Msg>,
    wake: impl Fn() + Send + 'static,
) {
    std::thread::Builder::new()
        .name(format!("search-{pane}-{generation}"))
        .spawn(move || {
            // No "insert a disk" dialog for an empty drive met on the way.
            // SAFETY: plain flag call on this (worker) thread; the old mode is not needed.
            unsafe { SetThreadErrorMode(SEM_FAILCRITICALERRORS, std::ptr::null_mut()) };
            let t0 = Instant::now();
            let (mut batches, mut sent) = (0usize, 0usize);
            let mut logged: Option<Instant> = None;
            let res = walk(&root, &matcher, show, thumbnails, content.as_ref(), &cancel, |entries| {
                batches += 1;
                sent += entries.len();
                if logged.is_none_or(|t| t.elapsed() >= SEARCH_FLUSH) {
                    logged = Some(Instant::now());
                    eprintln!("[search] pane={pane} gen={generation} batch {batches} ({sent} so far)");
                }
                let ok = tx.send(Msg::Batch { pane, generation, entries }).is_ok();
                wake();
                ok
            });
            let elapsed = t0.elapsed();
            let (total, error) = match res {
                Ok(s) => {
                    if let Some(code) = s.cancelled {
                        eprintln!("[search] pane={pane} gen={generation} cancelled after {} folders ({})", s.walked, cancel_reason(code));
                    } else if content.is_some() {
                        eprintln!(
                            "[search] pane={pane} gen={generation} done: {} found in {} files read, {} skipped ({} over the cap, {} binary, {} cloud-only, {} unreadable), {} folders walked, {} cloud, {} not listed, in {:.0} ms",
                            s.found,
                            s.read,
                            s.files_skipped(),
                            s.over_cap,
                            s.binary,
                            s.cloud_only,
                            s.unreadable,
                            s.walked,
                            s.cloud_folders,
                            s.skipped,
                            elapsed.as_secs_f64() * 1e3
                        );
                    } else {
                        eprintln!(
                            "[search] pane={pane} gen={generation} done: {} found, {} folders walked, {} skipped, in {:.0} ms",
                            s.found,
                            s.walked,
                            s.skipped,
                            elapsed.as_secs_f64() * 1e3
                        );
                    }
                    if s.skipped > 0 {
                        eprintln!("[search] pane={pane} gen={generation} skipped {} folders", s.skipped);
                    }
                    (s.found, None)
                }
                Err(e) => {
                    eprintln!("[search] pane={pane} gen={generation} cannot list {}: {e}", root.display());
                    (0, Some(e))
                }
            };
            // No free-space figure inside an archive.
            let free = if error.is_none() && !crate::fs::archive::inside(&root) { crate::statusbar::disk_free(&root) } else { None };
            let reparse = ReparseCounts::default();
            let _ = tx.send(Msg::Done { pane, generation, total, elapsed, error, free, reparse, archive: None });
            wake();
        })
        .expect("spawn search thread");
}

/// `<path>\*` as a NUL-terminated wide string. A long path gets the `\\?\` prefix (`\\?\UNC\`
/// for a share), as std's read_dir adds it, so folders past MAX_PATH still list.
fn find_pattern(path: &Path) -> Vec<u16> {
    long_wide(path.join("*").as_os_str().encode_wide().collect(), false)
}

/// A wide path, NUL-terminated, with the long-path prefix when it needs one (`find_pattern`), or
/// always with `verbatim` (`stat_entry`: no normalisation, so a trailing dot or space is kept).
pub(crate) fn long_wide(star: Vec<u16>, verbatim: bool) -> Vec<u16> {
    let (bs, q) = (b'\\' as u16, b'?' as u16);
    let (prefix, rest) = if (!verbatim && star.len() < 248) || star.starts_with(&[bs, bs, q, bs]) {
        ("", &star[..])
    } else if star.starts_with(&[bs, bs]) {
        (r"\\?\UNC", &star[1..]) // \\server\share -> \\?\UNC\server\share
    } else {
        (r"\\?\", &star[..])
    };
    prefix.encode_utf16().chain(rest.iter().copied()).chain([0]).collect()
}

/// FILETIME (100 ns ticks since 1601) to unix seconds, truncated; a time before 1970 reads 0,
/// as `metadata().modified()` through `duration_since(UNIX_EPOCH)` gave.
fn unix_secs(ft: FILETIME) -> i64 {
    const EPOCH_1970: i64 = 116_444_736_000_000_000;
    let ticks = ((ft.dwHighDateTime as u64) << 32 | ft.dwLowDateTime as u64) as i64;
    if ticks < EPOCH_1970 { 0 } else { (ticks - EPOCH_1970) / 10_000_000 }
}

/// One listing entry from FindFirstFileExW/FindNextFileW data; None for "." and "..". The
/// attributes are the entry's own (a reparse point is not followed) and its tag is in
/// `dwReserved0` when FILE_ATTRIBUTE_REPARSE_POINT is set.
fn entry_of(
    d: &WIN32_FIND_DATAW,
    types: &mut TypeNames,
    icon_keys: &mut crate::icons::IconKeys,
    zone: &crate::model::LocalZone,
) -> Option<Entry> {
    let len = d.cFileName.iter().position(|&c| c == 0).unwrap_or(d.cFileName.len());
    let wname = &d.cFileName[..len];
    let dot = b'.' as u16;
    if wname == [dot] || wname == [dot, dot] {
        return None;
    }
    let name = String::from_utf16_lossy(wname);
    let attrs = d.dwFileAttributes;
    let tag = if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 { d.dwReserved0 } else { 0 };
    // Only an entry that is a reparse point or unpinned can ask (no cldapi call for a plain file).
    let state = if attrs & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_UNPINNED) != 0 { crate::icons::placeholder_state(attrs, tag) } else { 0 };
    let shown = EXPOSED.with(Cell::get);
    let (attrs, tag) = as_listed(attrs, tag, shown);
    let kind = Kind::from_attrs(attrs, tag);
    let size = (d.nFileSizeHigh as u64) << 32 | d.nFileSizeLow as u64;
    // A cloud placeholder keeps its type's text (Explorer shows "Microsoft Word Document").
    let type_text = match kind {
        Kind::Junction => SharedString::from("Junction"),
        Kind::Symlink => SharedString::from("Symbolic link"),
        Kind::AppAlias => SharedString::from("Application"),
        _ => types.for_entry(&name, attrs & FILE_ATTRIBUTE_DIRECTORY != 0),
    };
    let modified = unix_secs(d.ftLastWriteTime);
    let synced = !crate::icons::needs_state_read(state, attrs, shown);
    let icon_key = icon_keys.for_entry_synced(&name, attrs, kind, modified, size, synced);
    Some(Entry::with_zone(name.into(), size, modified, attrs, kind, type_text, icon_key, zone))
}

thread_local! {
    /// This thread lists with the cloud files' placeholders shown (`expose_placeholders`).
    static EXPOSED: Cell<bool> = const { Cell::new(false) };
}

/// `PHCM_EXPOSE_PLACEHOLDERS` (ntifs.h; windows-sys declares the call, not its modes).
const PHCM_EXPOSE_PLACEHOLDERS: i8 = 2;

/// Show this thread the cloud files' placeholders (P12-M7). Windows hides them from TwinFM's
/// process: a OneDrive file lists as a plain file whose placeholder state reads 0, so a file
/// syncing, waiting or in error could not be told from one in sync (P9-M4). Shown, each lists as a
/// reparse point with its cloud tag and its real state, at no cost to the listing (40,023 OneDrive
/// entries, 6.1 s against 6.2 s), and `entry_of` takes the state and lists it as before
/// (`as_listed`). For the threads that build a sync root's entries: a listing's own and the watch's
/// edit worker. `who` names the thread in the line logged when Windows refuses.
pub fn expose_placeholders(who: &str) {
    // SAFETY: sets this thread's own placeholder mode; no pointers.
    let previous = unsafe { windows_sys::Wdk::Storage::FileSystem::RtlSetThreadPlaceholderCompatibilityMode(PHCM_EXPOSE_PLACEHOLDERS) };
    // A negative answer is an error (PHCM_ERROR_INVALID_PARAMETER, PHCM_ERROR_NO_TEB): the mode stays.
    EXPOSED.with(|e| e.set(previous >= 0));
    if previous < 0 {
        eprintln!("[cloud] {who}: placeholders not shown (RtlSetThreadPlaceholderCompatibilityMode = {previous})");
    }
}

/// An entry's attributes and reparse tag as a thread in Windows' default mode sees them (P12-M7):
/// with placeholders shown (`exposed`), a cloud placeholder is a reparse point with a cloud tag,
/// and one whose data is not all here is also offline and sparse (OneDrive's 34,401 such files:
/// `0x00400020` by default, `0x00401620` shown); listed as the plain file or folder it is by
/// default, its kind, keys, thumbnail, counts and the walkers' rules stay as they were. Any other
/// entry, and every entry on another thread, as given.
fn as_listed(attrs: u32, tag: u32, exposed: bool) -> (u32, u32) {
    if exposed && attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 && Kind::from_tag(tag) == Kind::Cloud {
        (attrs & !(FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_OFFLINE | FILE_ATTRIBUTE_SPARSE_FILE), 0)
    } else {
        (attrs, tag)
    }
}

// GetDriveTypeW results (WinBase.h). Declared here rather than enabling windows-sys's large
// Win32_System_WindowsProgramming feature for five constants.
pub(crate) const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_FIXED: u32 = 3;
pub(crate) const DRIVE_REMOTE: u32 = 4;
pub(crate) const DRIVE_CDROM: u32 = 5;
const DRIVE_RAMDISK: u32 = 6;

/// One entry per logical drive: name "C:\", type "Local Disk" or "Label (C:)", free space.
/// Network drives are not queried for label or free space: a disconnected share can stall
/// those calls for many seconds.
pub fn drive_entries() -> Vec<Entry> {
    // No "insert a disk" dialog for an empty card reader or optical drive.
    // SAFETY: plain flag call on the current (worker) thread; the old mode is not needed.
    unsafe { SetThreadErrorMode(SEM_FAILCRITICALERRORS, std::ptr::null_mut()) };
    // SAFETY: no arguments.
    let mask = unsafe { GetLogicalDrives() };
    let mut out = Vec::new();
    for i in 0..26u8 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i) as char;
        let root = format!("{letter}:\\");
        let wroot: Vec<u16> = root.encode_utf16().chain([0]).collect();
        // SAFETY: `wroot` is a NUL-terminated wide string for the duration of the call.
        let dtype = unsafe { GetDriveTypeW(wroot.as_ptr()) };
        let type_name = match dtype {
            DRIVE_REMOVABLE => "Removable Disk",
            DRIVE_FIXED => "Local Disk",
            DRIVE_REMOTE => "Network Drive",
            DRIVE_CDROM => "CD Drive",
            DRIVE_RAMDISK => "RAM Disk",
            _ => "Drive",
        };
        let (label, free) = if dtype == DRIVE_REMOTE { (String::new(), None) } else { (volume_label(&wroot), free_space(&wroot)) };
        let type_text = if label.is_empty() { type_name.to_string() } else { format!("{label} ({letter}:)") };
        out.push(Entry::drive(root.into(), free, type_text.into()));
    }
    out
}

fn volume_label(wroot: &[u16]) -> String {
    let mut buf = [0u16; 261];
    // SAFETY: `wroot` is NUL-terminated; `buf` holds the advertised length; other outputs are null.
    let ok = unsafe {
        GetVolumeInformationW(
            wroot.as_ptr(),
            buf.as_mut_ptr(),
            buf.len() as u32,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        )
    };
    if ok == 0 {
        return String::new();
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..n])
}

fn free_space(wroot: &[u16]) -> Option<u64> {
    let mut free = 0u64;
    // SAFETY: `wroot` is NUL-terminated; `free` is a live u64; the other outputs are optional.
    let ok = unsafe { GetDiskFreeSpaceExW(wroot.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) };
    (ok != 0).then_some(free)
}

/// Friendly type names ("Text Document") by extension, looked up once per extension per
/// enumeration through the shell's association registry. Lives on the worker only.
#[derive(Default)]
pub struct TypeNames {
    cache: HashMap<String, SharedString>,
    key: String,
}

impl TypeNames {
    pub fn for_entry(&mut self, name: &str, is_dir: bool) -> SharedString {
        if is_dir {
            return SharedString::from("File folder");
        }
        // Text after the last dot, as Explorer does (".gitignore" has extension "gitignore").
        let ext = match name.rfind('.') {
            Some(i) if i + 1 < name.len() => &name[i + 1..],
            _ => return SharedString::from("File"),
        };
        self.key.clear();
        self.key.extend(ext.chars().map(|c| c.to_ascii_lowercase()));
        if let Some(t) = self.cache.get(&self.key) {
            return t.clone();
        }
        let t: SharedString =
            assoc_friendly_name(&self.key).unwrap_or_else(|| format!("{} File", ext.to_uppercase())).into();
        self.cache.insert(self.key.clone(), t.clone());
        t
    }
}

/// AssocQueryStringW(ASSOCSTR_FRIENDLYDOCNAME) for ".ext"; None when nothing is registered.
fn assoc_friendly_name(ext: &str) -> Option<String> {
    let assoc: Vec<u16> = ".".encode_utf16().chain(ext.encode_utf16()).chain([0]).collect();
    let mut buf = [0u16; 256];
    let mut len = buf.len() as u32;
    // SAFETY: `assoc` is NUL-terminated, `buf` holds `len` u16s, `len` is a live u32.
    let hr = unsafe {
        AssocQueryStringW(
            ASSOCF_NOTRUNCATE,
            ASSOCSTR_FRIENDLYDOCNAME,
            assoc.as_ptr(),
            std::ptr::null(),
            buf.as_mut_ptr(),
            &mut len,
        )
    };
    if hr != 0 {
        return None; // includes truncation: a type name over 255 chars is not worth a retry
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let s = String::from_utf16_lossy(&buf[..n]);
    if s.is_empty() { None } else { Some(s) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Kind;
    use crossbeam_channel::unbounded;
    use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM, SetFileAttributesW};

    #[test]
    fn a_shown_placeholder_lists_as_before_with_its_own_state() {
        // P12-M7: on a thread with placeholders shown, a OneDrive file is a reparse point with a
        // cloud tag; it lists as the plain file it is in Windows' default mode, and only its state is
        // new. Reads nothing: cldapi's state is a function of the attributes and the tag.
        use crate::icons::{Cloud, IconKeys, placeholder_state};
        use windows_sys::Win32::Storage::CloudFilters::{CF_PLACEHOLDER_STATE_IN_SYNC, CF_PLACEHOLDER_STATE_PLACEHOLDER};
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_ARCHIVE;
        let data = |name: &str, attrs: u32, tag: u32| {
            let mut d = WIN32_FIND_DATAW { dwFileAttributes: attrs, dwReserved0: tag, ..Default::default() };
            let w: Vec<u16> = name.encode_utf16().collect();
            d.cFileName[..w.len()].copy_from_slice(&w);
            d
        };
        let (zone, mut types) = (crate::model::LocalZone::current(), TypeNames::default());
        let mut keys = IconKeys::new(PathBuf::from(r"C:\OneDrive"), true).with_cloud(Cloud::SyncRoot);
        let reparse = FILE_ATTRIBUTE_REPARSE_POINT;
        let (file, dir) = (FILE_ATTRIBUTE_ARCHIVE | reparse, FILE_ATTRIBUTE_DIRECTORY | reparse);
        let in_sync = 0x9000_601A; // state 0x9 on Jeremy's OneDrive (the P12-M7 probe)
        let state = placeholder_state(file, in_sync);
        assert_eq!(state & (CF_PLACEHOLDER_STATE_PLACEHOLDER | CF_PLACEHOLDER_STATE_IN_SYNC), CF_PLACEHOLDER_STATE_PLACEHOLDER | CF_PLACEHOLDER_STATE_IN_SYNC);
        let out = (0..16u32)
            .map(|x| 0x9000_001A | x << 12)
            .find(|&t| placeholder_state(file, t) & (CF_PLACEHOLDER_STATE_PLACEHOLDER | CF_PLACEHOLDER_STATE_IN_SYNC) == CF_PLACEHOLDER_STATE_PLACEHOLDER)
            .expect("a cloud tag cldapi reads as a placeholder not in sync");
        EXPOSED.with(|e| e.set(true));
        let a = entry_of(&data("a.jpg", file, in_sync), &mut types, &mut keys, &zone).unwrap();
        let b = entry_of(&data("b.txt", file, out), &mut types, &mut keys, &zone).unwrap();
        let d = entry_of(&data("sub", dir, in_sync), &mut types, &mut keys, &zone).unwrap();
        let new = entry_of(&data("new.txt", FILE_ATTRIBUTE_ARCHIVE, 0), &mut types, &mut keys, &zone).unwrap();
        EXPOSED.with(|e| e.set(false));
        let old = entry_of(&data("old.txt", FILE_ATTRIBUTE_ARCHIVE, 0), &mut types, &mut keys, &zone).unwrap();
        assert_eq!(new.icon_key.as_str(), r"look:s:ext:.txt|C:\OneDrive\new.txt", "no placeholder yet (uploading): its own state is looked up");
        assert_eq!(old.icon_key.as_str(), "ext:s2:.txt", "a thread not shown placeholders keeps the attribute mark, as before");
        assert_eq!((a.kind, a.attrs), (Kind::File, FILE_ATTRIBUTE_ARCHIVE), "listed as the plain file");
        assert!(a.icon_key.starts_with(r"thumb:s2:C:\OneDrive\a.jpg|"), "in sync: its thumbnail and the attribute mark, as before: {}", a.icon_key);
        assert_eq!(b.kind, Kind::File);
        assert_eq!(b.icon_key.as_str(), r"look:s:ext:.txt|C:\OneDrive\b.txt", "not in sync: its own state is looked up");
        assert_eq!((d.kind, d.attrs), (Kind::Directory, FILE_ATTRIBUTE_DIRECTORY));
        // A thread that never showed placeholders lists a cloud reparse point as it always did.
        let c = entry_of(&data("c.txt", file, in_sync), &mut types, &mut keys, &zone).unwrap();
        assert_eq!(c.kind, Kind::Cloud);
        assert_eq!(as_listed(file, in_sync, true), (FILE_ATTRIBUTE_ARCHIVE, 0));
        assert_eq!(as_listed(file, in_sync, false), (file, in_sync));
        assert_eq!(as_listed(dir, 0xA000_0003, true), (dir, 0xA000_0003), "a junction is no placeholder");
        assert_eq!(as_listed(FILE_ATTRIBUTE_ARCHIVE, 0, true), (FILE_ATTRIBUTE_ARCHIVE, 0));
        // Not all its data here: offline and sparse as well when shown, neither by default.
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS;
        let away = FILE_ATTRIBUTE_ARCHIVE | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS;
        assert_eq!(as_listed(away | FILE_ATTRIBUTE_OFFLINE | FILE_ATTRIBUTE_SPARSE_FILE | reparse, in_sync, true), (away, 0));
    }

    fn set_attrs(path: &std::path::Path, attrs: u32) {
        let w: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().chain([0]).collect();
        // SAFETY: `w` is a NUL-terminated wide path that outlives the call.
        assert_ne!(unsafe { SetFileAttributesW(w.as_ptr(), attrs) }, 0, "SetFileAttributesW {path:?}");
    }

    fn entries_of(msgs: &[Msg]) -> Vec<Entry> {
        msgs.iter()
            .filter_map(|m| match m {
                Msg::Batch { entries, .. } => Some(entries.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    #[test]
    fn reads_hidden_and_system_attributes() {
        let dir = std::env::temp_dir().join(format!("twinfm-attr-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for n in ["plain.txt", "hidden.txt", "system.txt"] {
            std::fs::write(dir.join(n), b"x").unwrap();
        }
        set_attrs(&dir.join("hidden.txt"), FILE_ATTRIBUTE_HIDDEN);
        set_attrs(&dir.join("system.txt"), FILE_ATTRIBUTE_SYSTEM);
        let entries = entries_of(&collect(dir.clone()));
        for n in ["hidden.txt", "system.txt"] {
            set_attrs(&dir.join(n), 0x80); // FILE_ATTRIBUTE_NORMAL, so cleanup can delete them
        }
        std::fs::remove_dir_all(&dir).unwrap();

        let by = |n: &str| entries.iter().find(|e| e.name == n).unwrap().clone();
        assert!(!by("plain.txt").is_hidden() && !by("plain.txt").is_system());
        assert!(by("hidden.txt").is_hidden() && !by("hidden.txt").is_system());
        assert!(by("system.txt").is_system() && !by("system.txt").is_hidden());
    }

    #[test]
    fn this_pc_lists_drives() {
        assert!(is_this_pc("") && is_this_pc("  this pc ") && is_this_pc("This PC"));
        assert!(!is_this_pc("C:\\"));
        for path in ["This PC", ""] {
            let msgs = collect(PathBuf::from(path));
            let entries = entries_of(&msgs);
            let c = entries.iter().find(|e| e.name == "C:\\").expect("C:\\ is listed");
            assert_eq!(c.kind, Kind::Drive);
            assert!(c.is_dir());
            assert!(!c.type_text.is_empty());
            assert!(c.size > 0 && c.size_text.ends_with(" free"), "free space on C: ({})", c.size_text);
            assert_eq!(c.modified_text, "");
            match msgs.last().unwrap() {
                Msg::Done { total, error: None, .. } => assert_eq!(*total, entries.len()),
                other => panic!("unexpected final message: {}", describe(other)),
            }
        }
    }

    #[test]
    fn type_names_by_extension() {
        let mut t = TypeNames::default();
        assert_eq!(t.for_entry("sub", true), "File folder");
        assert_eq!(t.for_entry("README", false), "File");
        assert_eq!(t.for_entry("trailing.", false), "File");
        // Nothing registers this extension, so either the shell's or our fallback applies.
        assert_eq!(t.for_entry("a.Zq7xw", false), "ZQ7XW File");
        let txt = t.for_entry("a.txt", false);
        assert!(!txt.is_empty());
        assert_eq!(t.for_entry("B.TXT", false), txt, "cache is keyed by lowercase extension");
    }

    fn collect(path: PathBuf) -> Vec<Msg> {
        let (tx, rx) = unbounded();
        spawn(path, 1, 7, false, crate::icons::Cloud::None, tx, || {});
        let mut out = Vec::new();
        loop {
            let m = rx.recv().expect("worker sends Done before exiting");
            let done = matches!(m, Msg::Done { .. });
            out.push(m);
            if done {
                return out;
            }
        }
    }

    fn describe(m: &Msg) -> String {
        match m {
            Msg::Batch { entries, .. } => format!("Batch of {}", entries.len()),
            Msg::Done { total, error, .. } => format!("Done total={total} error={error:?}"),
        }
    }

    #[test]
    fn enumerates_files_and_a_subdirectory() {
        let dir = std::env::temp_dir().join(format!("twinfm-enum-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        for i in 0..3 {
            std::fs::write(dir.join(format!("f{i}.txt")), vec![b'x'; i * 100]).unwrap();
        }
        let msgs = collect(dir.clone());
        std::fs::remove_dir_all(&dir).unwrap();

        let batches: Vec<&Vec<Entry>> = msgs
            .iter()
            .filter_map(|m| match m {
                Msg::Batch { entries, pane: 1, generation: 7 } => Some(entries),
                _ => None,
            })
            .collect();
        assert_eq!(batches.len(), 1, "4 entries fit in one batch of {BATCH_SIZE}");
        let entries = batches[0];
        assert_eq!(entries.len(), 4);
        let sub = entries.iter().find(|e| e.name == "sub").expect("subdirectory listed");
        assert!(sub.is_dir());
        assert_eq!(sub.kind, Kind::Directory);
        assert_eq!(sub.type_text, "File folder");
        assert_eq!(sub.size_text, "");
        let f2 = entries.iter().find(|e| e.name == "f2.txt").unwrap();
        assert!(!f2.is_dir());
        assert_eq!(f2.kind, Kind::File);
        assert!(!f2.type_text.is_empty());
        assert_eq!(f2.size, 200);
        assert_eq!(f2.size_text, "200 B");
        assert_eq!(f2.modified_text.len(), 16);
        match msgs.last().unwrap() {
            Msg::Done { pane: 1, generation: 7, total: 4, error: None, .. } => {}
            other => panic!("unexpected final message: {}", describe(other)),
        }
    }

    #[test]
    fn tells_reparse_points_apart() {
        let dir = std::env::temp_dir().join(format!("twinfm-enum-reparse-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join("plain.txt"), b"hello").unwrap();
        // A junction needs no privilege; mklink /J is the shortest way to make one.
        let made = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(dir.join("junc"))
            .arg(dir.join("target"))
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(made, "mklink /J failed");
        // A symbolic link needs the privilege or Developer Mode.
        let symlink = std::os::windows::fs::symlink_file(dir.join("plain.txt"), dir.join("link.txt")).is_ok();
        let plain_modified = std::fs::metadata(dir.join("plain.txt"))
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let msgs = collect(dir.clone());
        std::fs::remove_dir_all(&dir).unwrap(); // removes the junction, not its target's contents

        let entries = entries_of(&msgs);
        let by = |n: &str| entries.iter().find(|e| e.name == n).unwrap_or_else(|| panic!("{n} listed")).clone();
        let junc = by("junc");
        assert_eq!(junc.kind, Kind::Junction);
        assert_eq!(junc.type_text, "Junction");
        assert!(junc.is_dir(), "a junction opens by navigating");
        assert_eq!(junc.icon_key.as_str(), dir.join("junc").to_string_lossy(), "keyed by full path for its overlay");
        let plain = by("plain.txt");
        assert_eq!(plain.kind, Kind::File);
        assert_eq!(plain.size, 5);
        assert_eq!(plain.modified, plain_modified, "the same second metadata().modified() gives");
        assert_eq!(plain.icon_key, "ext:.txt");
        assert_eq!(by("target").kind, Kind::Directory);
        let expected = ReparseCounts { junction: 1, symlink: symlink as usize, ..Default::default() };
        match msgs.last().unwrap() {
            Msg::Done { reparse, error: None, .. } => assert_eq!(*reparse, expected),
            other => panic!("unexpected final message: {}", describe(other)),
        }
        if !symlink {
            eprintln!("skipping the symbolic link: creating one needs the privilege or Developer Mode");
            return;
        }
        let link = by("link.txt");
        assert_eq!(link.kind, Kind::Symlink);
        assert_eq!(link.type_text, "Symbolic link");
        assert!(!link.is_dir());
        assert_eq!(link.icon_key.as_str(), dir.join("link.txt").to_string_lossy());
    }

    #[test]
    fn search_walks_the_tree_without_links_or_hidden_folders() {
        use crate::model::NameMatch;
        use std::sync::atomic::AtomicU8;
        let dir = std::env::temp_dir().join(format!("twinfm-p3m2-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("root");
        std::fs::create_dir_all(root.join(r"d1\d2\d3")).unwrap();
        std::fs::create_dir_all(root.join(r"d1\hit-dir")).unwrap();
        std::fs::create_dir_all(root.join("secret")).unwrap();
        for f in ["hit-top.txt", "miss.txt", r"d1\d2\d3\HIT-deep.txt", r"d1\d2\miss.log", r"secret\hit-hidden.txt"] {
            std::fs::write(root.join(f), b"x").unwrap();
        }
        set_attrs(&root.join("secret"), FILE_ATTRIBUTE_HIDDEN);
        // A junction to an ancestor: entering it would loop until the depth limit.
        let made = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(root.join(r"d1\hit-loop"))
            .arg(&root)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        // Cancellation: a folder of 600 matches and 20 subfolders with one each.
        let bulk = dir.join("bulk");
        for i in 0..20 {
            std::fs::create_dir_all(bulk.join(format!("sub{i:02}"))).unwrap();
            std::fs::write(bulk.join(format!(r"sub{i:02}\hit.txt")), b"x").unwrap();
        }
        for i in 0..600 {
            std::fs::write(bulk.join(format!("hit{i:03}.txt")), b"x").unwrap();
        }

        let hit = NameMatch::new("hit");
        let run = |root: &Path, show_hidden: bool, cancel_after_first: bool| {
            let show_hidden = if show_hidden { crate::model::Show::HIDDEN } else { crate::model::Show::OFF };
            let cancel = AtomicU8::new(SEARCH_RUNNING);
            let mut batches: Vec<Vec<Entry>> = Vec::new();
            let stats = walk(root, &hit, show_hidden, false, None, &cancel, |b| {
                if cancel_after_first {
                    cancel.store(SEARCH_ESCAPE, std::sync::atomic::Ordering::Relaxed);
                }
                batches.push(b);
                true
            });
            (stats, batches)
        };
        let (plain, plain_batches) = run(&root, false, false);
        let (hidden, hidden_batches) = run(&root, true, false);
        let (cancelled, cancelled_batches) = run(&bulk, false, true);
        let (bulk_all, _) = run(&bulk, false, false);
        let missing = walk(&dir.join("nope"), &hit, crate::model::Show::OFF, false, None, &AtomicU8::new(0), |_| true);
        // P9-M4: with thumbnails on a result in THUMB_EXTS is keyed by its full path, as a listing's is.
        let pics = dir.join("pics");
        std::fs::create_dir_all(pics.join("sub")).unwrap();
        std::fs::write(pics.join(r"sub\hit.jpg"), b"x").unwrap();
        std::fs::write(pics.join("hit.txt"), b"x").unwrap();
        let mut thumbed: Vec<Entry> = Vec::new();
        walk(&pics, &hit, crate::model::Show::OFF, true, None, &AtomicU8::new(SEARCH_RUNNING), |b| {
            thumbed.extend(b);
            true
        })
        .unwrap();
        let mut unthumbed: Vec<Entry> = Vec::new();
        walk(&pics, &hit, crate::model::Show::OFF, false, None, &AtomicU8::new(SEARCH_RUNNING), |b| {
            unthumbed.extend(b);
            true
        })
        .unwrap();
        set_attrs(&root.join("secret"), FILE_ATTRIBUTE_DIRECTORY);
        std::fs::remove_dir_all(&dir).unwrap(); // removes the junction, not the tree twice

        assert!(made, "mklink /J failed");
        let names = |b: &[Vec<Entry>]| {
            let mut v: Vec<String> = b.iter().flatten().map(|e| e.name.to_string()).collect();
            v.sort();
            v
        };
        assert_eq!(names(&plain_batches), [r"d1\d2\d3\HIT-deep.txt", r"d1\hit-dir", r"d1\hit-loop", "hit-top.txt"]);
        let plain = plain.unwrap();
        // root, d1, d2, d3, hit-dir: not the hidden folder, not the junction.
        assert_eq!((plain.found, plain.walked, plain.skipped, plain.cancelled), (4, 5, 0, None));
        let all = plain_batches.concat();
        let by = |n: &str| all.iter().find(|e| e.name == n).unwrap().clone();
        assert_eq!(by(r"d1\hit-loop").kind, Kind::Junction);
        assert_eq!(by(r"d1\hit-loop").icon_key.as_str(), root.join(r"d1\hit-loop").to_string_lossy(), "keyed by its real full path");
        assert_eq!(by(r"d1\d2\d3\HIT-deep.txt").icon_key, "ext:.txt");
        let key = |v: &[Entry], n: &str| v.iter().find(|e| e.name == n).unwrap().icon_key.to_string();
        let jpg = &thumbed.iter().find(|e| e.name == r"sub\hit.jpg").unwrap();
        assert_eq!(key(&thumbed, r"sub\hit.jpg"), crate::icons::thumb_key(&pics.join(r"sub\hit.jpg"), jpg.modified, jpg.size));
        assert_eq!(key(&thumbed, "hit.txt"), "ext:.txt");
        assert_eq!(key(&unthumbed, r"sub\hit.jpg"), "ext:.jpg", "thumbnails off: the type's key");
        assert!(by(r"d1\hit-dir").is_dir());
        let hidden = hidden.unwrap();
        assert_eq!((hidden.found, hidden.walked), (5, 6), "with hidden files shown the hidden folder is entered");
        assert!(names(&hidden_batches).contains(&r"secret\hit-hidden.txt".to_string()));

        let all_bulk = bulk_all.unwrap();
        assert_eq!((all_bulk.found, all_bulk.walked), (620, 21));
        let cancelled = cancelled.unwrap();
        assert_eq!(cancelled.cancelled, Some(SEARCH_ESCAPE));
        // Stopped inside the first folder, after its first batch (SEARCH_BATCH entries, or fewer if
        // listing them took longer than SEARCH_FLUSH).
        assert_eq!(cancelled.walked, 1);
        assert!(cancelled.found > 0 && cancelled.found <= SEARCH_BATCH, "found {}", cancelled.found);
        assert_eq!((cancelled_batches.len(), cancelled_batches.concat().len()), (1, cancelled.found));
        assert!(missing.is_err(), "a root that cannot be listed is an error");
    }

    #[test]
    fn content_search_reads_text_files_and_stays_out_of_links_and_clouds() {
        use crate::model::NameMatch;
        use std::sync::atomic::AtomicU8;
        use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_OFFLINE};
        let dir = std::env::temp_dir().join(format!("twinfm-p11m1-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(r"sub\needle-folder")).unwrap();
        std::fs::create_dir_all(dir.join("cloudy")).unwrap();
        std::fs::write(dir.join("a.txt"), "first\nthe NEEDLE is here\nneedle again\n").unwrap();
        std::fs::write(dir.join("miss.txt"), "nothing to see").unwrap();
        std::fs::write(dir.join("needle-in-name.txt"), "only the name").unwrap();
        let wide: Vec<u8> = [0xFF, 0xFE].into_iter().chain("one\r\nwide needle\r\n".encode_utf16().flat_map(u16::to_le_bytes)).collect();
        std::fs::write(dir.join(r"sub\w.txt"), wide).unwrap();
        std::fs::write(dir.join("b.bin"), b"needle\x00needle").unwrap();
        std::fs::write(dir.join("offline.txt"), "needle").unwrap();
        set_attrs(&dir.join("offline.txt"), FILE_ATTRIBUTE_OFFLINE | FILE_ATTRIBUTE_ARCHIVE);
        std::fs::write(dir.join(r"cloudy\c.txt"), "needle").unwrap();
        let made = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(dir.join(r"sub\loop"))
            .arg(&dir)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        let content = Content { needle: crate::fs::content::fold("Needle"), cloud_roots: vec![crate::cloud::normalise(&dir.join("cloudy").to_string_lossy())] };
        let mut found: Vec<Entry> = Vec::new();
        let stats = walk(&dir, &NameMatch::default(), crate::model::Show::OFF, false, Some(&content), &AtomicU8::new(SEARCH_RUNNING), |b| {
            found.extend(b);
            true
        });
        // Cancelled before the first file: nothing is read.
        let mut none: Vec<Entry> = Vec::new();
        let stopped = walk(&dir, &NameMatch::default(), crate::model::Show::OFF, false, Some(&content), &AtomicU8::new(SEARCH_ESCAPE), |b| {
            none.extend(b);
            true
        });
        set_attrs(&dir.join("offline.txt"), FILE_ATTRIBUTE_ARCHIVE);
        std::fs::remove_dir_all(&dir).unwrap();

        assert!(made, "mklink /J failed");
        let mut rows: Vec<(String, String)> = found.iter().map(|e| (e.name.to_string(), e.found.to_string())).collect();
        rows.sort();
        assert_eq!(rows, [("a.txt".into(), "the NEEDLE is here".into()), (r"sub\w.txt".into(), "wide needle".into())], "files holding the text, each with its first line; no folder, no name match");
        let s = stats.unwrap();
        // a.txt, miss.txt, needle-in-name.txt and w.txt read; the binary, the offline file and the
        // cloud folder's file not; the junction not entered (root, sub, needle-folder walked).
        assert_eq!((s.found, s.read, s.binary, s.cloud_only, s.over_cap, s.unreadable), (2, 4, 1, 1, 0, 0));
        assert_eq!((s.walked, s.cloud_folders, s.skipped, s.files_skipped()), (3, 1, 0, 2));
        let stopped = stopped.unwrap();
        assert_eq!((stopped.read, stopped.cancelled, none.len()), (0, Some(SEARCH_ESCAPE), 0));
    }

    #[test]
    fn filetime_and_long_paths() {
        let ft = |t: i64| FILETIME { dwLowDateTime: t as u32, dwHighDateTime: (t >> 32) as u32 };
        assert_eq!(unix_secs(ft(116_444_736_000_000_000)), 0);
        assert_eq!(unix_secs(ft(116_444_736_000_000_000 + 15_999_999)), 1, "truncated");
        assert_eq!(unix_secs(ft(0)), 0, "before 1970 reads 0");
        assert_eq!(unix_secs(ft(133_000_000_000_000_000)), 1_655_526_400);
        let s = |p: &str| String::from_utf16(&find_pattern(Path::new(p))).unwrap();
        assert_eq!(s(r"C:\Windows"), r"C:\Windows\*".to_string() + "\0");
        assert_eq!(s(r"C:\"), r"C:\*".to_string() + "\0");
        let long = format!(r"C:\{}", "a".repeat(300));
        assert_eq!(s(&long), format!(r"\\?\{long}\*") + "\0");
        let unc = format!(r"\\server\share\{}", "b".repeat(300));
        assert_eq!(s(&unc), format!(r"\\?\UNC\server\share\{}\*", "b".repeat(300)) + "\0");
        let verbatim = format!(r"\\?\C:\{}", "c".repeat(300));
        assert_eq!(s(&verbatim), format!(r"{verbatim}\*") + "\0");
    }

    #[test]
    fn stat_entry_reads_one_entry() {
        let dir = std::env::temp_dir().join(format!("twinfm-p5m1-stat-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Sub")).unwrap();
        std::fs::write(dir.join("Report.TXT"), b"hello").unwrap();
        std::fs::write(dir.join("a[1].txt"), b"xy").unwrap();
        std::fs::write(dir.join("a1.txt"), b"other").unwrap();
        // A name ending in a dot exists only through a verbatim path.
        let verbatim = |n: &str| PathBuf::from(format!(r"\\?\{}", dir.join(n).display()));
        std::fs::write(verbatim("trail."), b"dot").unwrap();
        let (mut types, zone) = (TypeNames::default(), crate::model::LocalZone::current());
        let mut keys = crate::icons::IconKeys::new(dir.clone(), false);
        let mut stat = |name: &str| stat_entry(&dir, name, &mut types, &mut keys, &zone);
        let report = stat("report.txt");
        let bracket = stat("a[1].txt");
        let sub = stat("Sub");
        let missing = stat("nope.txt");
        let trail = stat("trail.");
        let no_dot = stat("trail");
        let refused: Vec<bool> = ["", ".", "..", "a*.txt", "a?.txt", r"Sub\x", "Report.TXT:stream", "C:x"].iter().map(|n| stat(n).is_none()).collect();
        // The listing's own entry for the same file, to compare field by field.
        let mut listed = Vec::new();
        let mut keys2 = crate::icons::IconKeys::new(dir.clone(), false);
        list_dir(&dir, &mut TypeNames::default(), &mut keys2, &zone, |e| {
            listed.push(e);
            true
        })
        .unwrap();
        std::fs::remove_file(verbatim("trail.")).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();

        let trail = trail.expect("a name ending in a dot is looked up as itself");
        assert_eq!((trail.name.as_str(), trail.size), ("trail.", 3));
        assert!(no_dot.is_none(), "and not as the name without its dot");
        let report = report.expect("found with another casing");
        assert_eq!(report.name, "Report.TXT", "the on-disk casing");
        let from_list = listed.iter().find(|e| e.name == "Report.TXT").unwrap();
        assert_eq!(
            (report.size, report.modified, report.attrs, report.kind, &report.type_text, &report.icon_key, &report.modified_text),
            (from_list.size, from_list.modified, from_list.attrs, from_list.kind, &from_list.type_text, &from_list.icon_key, &from_list.modified_text)
        );
        let bracket = bracket.expect("a name with [ is found as itself");
        assert_eq!((bracket.name.as_str(), bracket.size), ("a[1].txt", 2));
        let sub = sub.unwrap();
        assert!(sub.is_dir() && sub.kind == Kind::Directory);
        assert!(missing.is_none(), "a missing name gives None");
        assert!(refused.iter().all(|&r| r), "names that cannot be one entry give None: {refused:?}");
    }

    #[test]
    fn bad_path_yields_done_with_error() {
        let msgs = collect(PathBuf::from(r"C:\twinfm-enum-test-does-not-exist\nope"));
        assert_eq!(msgs.len(), 1);
        match &msgs[0] {
            Msg::Done { total: 0, error: Some(e), .. } => assert!(!e.is_empty()),
            other => panic!("unexpected message: {}", describe(other)),
        }
    }
}

