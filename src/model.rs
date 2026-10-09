//! PaneModel: a flat Vec of plain entries exposed to Slint as a virtualized model.
//! Only the UI thread touches it. Workers hand over whole Vec<Entry> batches.

use crate::Row;
use slint::{Model, ModelNotify, ModelTracker, SharedString};
use std::cell::{Cell, RefCell};
use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime, TIME_ZONE_INFORMATION};

/// One directory entry. No handles, no callbacks, no per-row heap objects besides the
/// refcounted strings (SharedString is a thin Arc-like handle, cheap to clone). The display
/// strings are formatted once here, on the enumeration worker, so `row_data` only clones.
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: SharedString,
    pub size: u64,
    pub modified: i64, // unix seconds
    pub is_dir: bool,
    pub size_text: SharedString,
    pub modified_text: SharedString,
}

impl Entry {
    pub fn new(name: SharedString, size: u64, modified: i64, is_dir: bool) -> Entry {
        Entry {
            size_text: if is_dir { SharedString::default() } else { format_size(size).into() },
            modified_text: format_time(modified).into(),
            name,
            size,
            modified,
            is_dir,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortCol {
    Name = 0,
    Size = 1,
    Modified = 2,
}

impl SortCol {
    pub fn from_i32(c: i32) -> SortCol {
        match c {
            1 => SortCol::Size,
            2 => SortCol::Modified,
            _ => SortCol::Name,
        }
    }
}

pub struct PaneModel {
    entries: RefCell<Vec<Entry>>,
    notify: ModelNotify,
    /// Generation of the enumeration whose batches we accept; stale workers are ignored.
    pub generation: Cell<u64>,
    pub sort_col: Cell<SortCol>,
    pub sort_asc: Cell<bool>,
}

impl Default for PaneModel {
    fn default() -> Self {
        PaneModel {
            entries: RefCell::new(Vec::new()),
            notify: ModelNotify::default(),
            generation: Cell::new(0),
            sort_col: Cell::new(SortCol::Name),
            sort_asc: Cell::new(true),
        }
    }
}

impl PaneModel {
    pub fn clear(&self) {
        self.entries.borrow_mut().clear();
        self.notify.reset();
    }

    /// Append a batch; one row_added notification per batch, never per entry.
    pub fn append(&self, mut batch: Vec<Entry>) {
        let start = self.entries.borrow().len();
        let n = batch.len();
        self.entries.borrow_mut().append(&mut batch);
        self.notify.row_added(start, n);
    }

    /// Swap the whole Vec in one step (used after a worker-side sort).
    pub fn replace(&self, entries: Vec<Entry>) {
        *self.entries.borrow_mut() = entries;
        self.notify.reset();
    }

    pub fn snapshot(&self) -> Vec<Entry> {
        self.entries.borrow().clone()
    }

    pub fn len(&self) -> usize {
        self.entries.borrow().len()
    }

    pub fn entry(&self, i: usize) -> Option<Entry> {
        self.entries.borrow().get(i).cloned()
    }
}

impl Model for PaneModel {
    type Data = Row;

    fn row_count(&self) -> usize {
        self.entries.borrow().len()
    }

    fn row_data(&self, row: usize) -> Option<Row> {
        let entries = self.entries.borrow();
        let e = entries.get(row)?;
        Some(Row { name: e.name.clone(), size: e.size_text.clone(), modified: e.modified_text.clone(), is_dir: e.is_dir })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Sort in place: directories first, then by the chosen column. Runs on a worker.
pub fn sort_entries(entries: &mut [Entry], col: SortCol, asc: bool) {
    entries.sort_unstable_by(|a, b| {
        let dir_order = b.is_dir.cmp(&a.is_dir);
        if dir_order != std::cmp::Ordering::Equal {
            return dir_order;
        }
        let o = match col {
            SortCol::Name => natural_cmp(&a.name, &b.name),
            SortCol::Size => a.size.cmp(&b.size).then_with(|| natural_cmp(&a.name, &b.name)),
            SortCol::Modified => a.modified.cmp(&b.modified).then_with(|| natural_cmp(&a.name, &b.name)),
        };
        if asc { o } else { o.reverse() }
    });
}

/// Case-insensitive compare with embedded numbers compared numerically (file_2 < file_10).
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut ia, mut ib) = (a.bytes().peekable(), b.bytes().peekable());
    loop {
        match (ia.peek().copied(), ib.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(ca), Some(cb)) if ca.is_ascii_digit() && cb.is_ascii_digit() => {
                let mut na: u64 = 0;
                let mut nb: u64 = 0;
                while let Some(&c) = ia.peek().filter(|c| c.is_ascii_digit()) {
                    na = na.saturating_mul(10).saturating_add((c - b'0') as u64);
                    ia.next();
                }
                while let Some(&c) = ib.peek().filter(|c| c.is_ascii_digit()) {
                    nb = nb.saturating_mul(10).saturating_add((c - b'0') as u64);
                    ib.next();
                }
                if na != nb {
                    return na.cmp(&nb);
                }
            }
            (Some(ca), Some(cb)) => {
                let (la, lb) = (ca.to_ascii_lowercase(), cb.to_ascii_lowercase());
                if la != lb {
                    return la.cmp(&lb);
                }
                ia.next();
                ib.next();
            }
        }
    }
}

pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Local date and time from Unix seconds, through the Win32 time-zone conversion
/// (`FileTimeToSystemTime` + `SystemTimeToTzSpecificLocalTime`); no chrono dependency.
pub fn format_time(secs: i64) -> String {
    format_time_in(secs, None)
}

/// `tz = None` uses the current time zone; tests pass a fixed one.
fn format_time_in(secs: i64, tz: Option<&TIME_ZONE_INFORMATION>) -> String {
    // FILETIME counts 100 ns ticks since 1601-01-01; the Unix epoch is 11,644,473,600 s later.
    let ticks = (secs + 11_644_473_600).saturating_mul(10_000_000);
    let ft = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    let mut utc: SYSTEMTIME = unsafe { std::mem::zeroed() };
    let mut local: SYSTEMTIME = unsafe { std::mem::zeroed() };
    let tz_ptr = tz.map_or(std::ptr::null(), |t| t as *const TIME_ZONE_INFORMATION);
    // SAFETY: every pointer refers to a live stack value of the exact type the call expects.
    let ok = unsafe {
        FileTimeToSystemTime(&ft, &mut utc) != 0 && SystemTimeToTzSpecificLocalTime(tz_ptr, &utc, &mut local) != 0
    };
    if !ok {
        return String::new();
    }
    format!("{:04}-{:02}-{:02} {:02}:{:02}", local.wYear, local.wMonth, local.wDay, local.wHour, local.wMinute)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str, size: u64, is_dir: bool) -> Entry {
        Entry::new(name.into(), size, 0, is_dir)
    }

    /// A fixed UTC-5 zone with no daylight rule, so the test does not depend on the machine.
    fn utc_minus_5() -> TIME_ZONE_INFORMATION {
        let mut tz: TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
        tz.Bias = 300;
        tz
    }

    #[test]
    fn natural_order_and_dirs_first() {
        let mut v = vec![e("file_10", 0, false), e("file_2", 0, false), e("zdir", 0, true), e("File_1", 0, false)];
        sort_entries(&mut v, SortCol::Name, true);
        let names: Vec<&str> = v.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["zdir", "File_1", "file_2", "file_10"]);
        sort_entries(&mut v, SortCol::Name, false);
        let names: Vec<&str> = v.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["zdir", "file_10", "file_2", "File_1"]);
    }

    #[test]
    fn dates_and_sizes() {
        assert_eq!(format_time_in(0, Some(&utc_minus_5())), "1969-12-31 19:00");
        assert_eq!(format_time_in(1_790_000_000, Some(&utc_minus_5())), "2026-09-21 09:13");
        assert_eq!(format_time(1_790_000_000).len(), 16); // current zone: shape only
        assert_eq!(e("f", 1536, false).size_text.as_str(), "1.5 KB");
        assert_eq!(e("d", 1536, true).size_text.as_str(), "");
        assert_eq!(format_size(1), "1 B");
        assert_eq!(format_size(1536), "1.5 KB");
    }

    #[test]
    fn append_then_replace() {
        let m = PaneModel::default();
        m.append(vec![e("a", 1, false), e("b", 2, false)]);
        assert_eq!(m.row_count(), 2);
        assert_eq!(m.row_data(1).unwrap().size.as_str(), "2 B");
        m.replace(vec![e("c", 3, true)]);
        assert_eq!(m.row_count(), 1);
        assert!(m.row_data(0).unwrap().is_dir);
        assert_eq!(m.row_data(0).unwrap().size.as_str(), "");
    }
}
