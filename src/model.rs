//! PaneModel: a flat Vec of plain entries exposed to Slint as a virtualized model.
//! Only the UI thread touches it. Workers hand over whole Vec<Entry> batches.

use crate::Row;
use slint::{Model, ModelNotify, ModelTracker, SharedString};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashSet};
use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SYSTEM,
};
use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime, TIME_ZONE_INFORMATION};

/// What a row is, for display. A reparse point (junction, symlink, cloud placeholder) shows as
/// `Reparse` even when it is a directory; it still opens like one (see `Entry::is_dir`).
/// The discriminants are the `kind` int the Slint `Row` carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Directory = 0,
    File = 1,
    Drive = 2,
    Reparse = 3,
}

impl Kind {
    pub fn from_attrs(attrs: u32) -> Kind {
        if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            Kind::Reparse
        } else if attrs & FILE_ATTRIBUTE_DIRECTORY != 0 {
            Kind::Directory
        } else {
            Kind::File
        }
    }
}

/// One directory entry. No handles, no callbacks, no per-row heap objects besides the
/// refcounted strings (SharedString is a thin Arc-like handle, cheap to clone). The display
/// strings are formatted once here, on the enumeration worker, so `row_data` only clones.
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: SharedString,
    pub size: u64,
    pub modified: i64, // unix seconds
    /// Win32 FILE_ATTRIBUTE_* bits as FindNextFile reported them (reparse points not followed).
    pub attrs: u32,
    pub kind: Kind,
    pub size_text: SharedString,
    pub modified_text: SharedString,
    pub type_text: SharedString,
    /// Mirror of selection membership, kept true only in the pane's `shown` Vec so that
    /// `row_data` stays a plain clone (no set lookup per row).
    pub selected: bool,
}

impl Entry {
    pub fn new(name: SharedString, size: u64, modified: i64, attrs: u32, type_text: SharedString) -> Entry {
        let is_dir = attrs & FILE_ATTRIBUTE_DIRECTORY != 0;
        Entry {
            size_text: if is_dir { SharedString::default() } else { format_size(size).into() },
            modified_text: format_time(modified).into(),
            type_text,
            name,
            size,
            modified,
            attrs,
            kind: Kind::from_attrs(attrs),
            selected: false,
        }
    }

    /// Opens by navigating into it: directories, reparse directories and drives.
    pub fn is_dir(&self) -> bool {
        self.kind == Kind::Drive || self.attrs & FILE_ATTRIBUTE_DIRECTORY != 0
    }

    pub fn is_hidden(&self) -> bool {
        self.attrs & FILE_ATTRIBUTE_HIDDEN != 0
    }

    pub fn is_system(&self) -> bool {
        self.attrs & FILE_ATTRIBUTE_SYSTEM != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortCol {
    Name = 0,
    Size = 1,
    Modified = 2,
    Type = 3,
}

impl SortCol {
    pub fn from_i32(c: i32) -> SortCol {
        match c {
            1 => SortCol::Size,
            2 => SortCol::Modified,
            3 => SortCol::Type,
            _ => SortCol::Name,
        }
    }
}

pub struct PaneModel {
    /// Every entry of the listing, unfiltered, in enumeration order.
    all: RefCell<Vec<Entry>>,
    /// What the list shows: `all` without hidden/system entries (unless `show_hidden`), sorted
    /// by the last worker pass. `row_count` and `row_data` read only this.
    shown: RefCell<Vec<Entry>>,
    /// How many entries of `all` the filter currently keeps out of `shown`.
    hidden: Cell<usize>,
    /// Selected rows, as indices into `shown`. UI thread only.
    sel: RefCell<Selection>,
    /// Bumped on every selection change, so a worker pass can tell whether the selection it
    /// carried over is still the current one when it lands.
    pub sel_version: Cell<u64>,
    notify: ModelNotify,
    /// Generation of the enumeration whose batches we accept; stale workers are ignored.
    pub generation: Cell<u64>,
    /// Bumped by every filter/sort request; only the newest pass may swap `shown`.
    pub pass: Cell<u64>,
    pub sort_col: Cell<SortCol>,
    pub sort_asc: Cell<bool>,
    pub show_hidden: Cell<bool>,
}

impl Default for PaneModel {
    fn default() -> Self {
        PaneModel {
            all: RefCell::new(Vec::new()),
            shown: RefCell::new(Vec::new()),
            hidden: Cell::new(0),
            sel: RefCell::new(Selection::default()),
            sel_version: Cell::new(0),
            notify: ModelNotify::default(),
            generation: Cell::new(0),
            pass: Cell::new(0),
            sort_col: Cell::new(SortCol::Name),
            sort_asc: Cell::new(true),
            show_hidden: Cell::new(false),
        }
    }
}

impl PaneModel {
    pub fn clear(&self) {
        self.all.borrow_mut().clear();
        self.shown.borrow_mut().clear();
        self.hidden.set(0);
        *self.sel.borrow_mut() = Selection::default();
        self.sel_version.set(self.sel_version.get() + 1);
        self.notify.reset();
    }

    /// Apply a selection gesture; only rows whose membership changed are notified.
    pub fn select(&self, op: SelOp, row: usize) {
        let len = self.shown.borrow().len();
        let changed = self.sel.borrow_mut().apply(op, row, len);
        if changed.is_empty() {
            return;
        }
        {
            let sel = self.sel.borrow();
            let mut shown = self.shown.borrow_mut();
            for &i in &changed {
                shown[i].selected = sel.set.contains(&i);
            }
        }
        for &i in &changed {
            self.notify.row_changed(i);
        }
        self.sel_version.set(self.sel_version.get() + 1);
    }

    /// Names of the selected rows, in display order.
    pub fn selected_names(&self) -> Vec<SharedString> {
        let shown = self.shown.borrow();
        self.sel.borrow().set.iter().filter_map(|&i| shown.get(i).map(|e| e.name.clone())).collect()
    }

    /// The cursor and selection by name, to carry across a pass or a refresh.
    pub fn capture(&self, cursor: i32) -> Restore {
        let shown = self.shown.borrow();
        let cursor = usize::try_from(cursor).ok().and_then(|i| shown.get(i)).map(|e| e.name.clone());
        drop(shown);
        Restore { cursor, selected: self.selected_names() }
    }

    /// Append a batch; one row_added notification per batch, never per entry. The batch moves
    /// into `all`; `shown` gets clones of the entries the filter keeps (refcount bumps only).
    pub fn append(&self, mut batch: Vec<Entry>) {
        let show_hidden = self.show_hidden.get();
        let start = self.shown.borrow().len();
        {
            let mut shown = self.shown.borrow_mut();
            shown.extend(batch.iter().filter(|e| visible(e, show_hidden)).cloned());
        }
        let added = self.shown.borrow().len() - start;
        self.hidden.set(self.hidden.get() + batch.len() - added);
        self.all.borrow_mut().append(&mut batch);
        if added > 0 {
            self.notify.row_added(start, added);
        }
    }

    /// Swap the shown Vec in one step (the result of a worker-side filter and sort pass) and
    /// adopt the selection the pass restored. Returns the restored cursor row.
    pub fn replace(&self, out: PassOutput) -> Option<usize> {
        *self.shown.borrow_mut() = out.shown;
        self.hidden.set(out.hidden);
        *self.sel.borrow_mut() = Selection { set: out.selection, anchor: out.cursor };
        self.sel_version.set(self.sel_version.get() + 1);
        self.notify.reset();
        out.cursor
    }

    /// Copy of the full listing for a worker pass.
    pub fn snapshot_all(&self) -> Vec<Entry> {
        self.all.borrow().clone()
    }

    /// Rows shown.
    pub fn len(&self) -> usize {
        self.shown.borrow().len()
    }

    /// Entries enumerated, shown or not.
    pub fn total(&self) -> usize {
        self.all.borrow().len()
    }

    /// Entries the filter keeps out of view.
    pub fn hidden_count(&self) -> usize {
        self.hidden.get()
    }

    pub fn entry(&self, i: usize) -> Option<Entry> {
        self.shown.borrow().get(i).cloned()
    }
}

impl Model for PaneModel {
    type Data = Row;

    fn row_count(&self) -> usize {
        self.shown.borrow().len()
    }

    fn row_data(&self, row: usize) -> Option<Row> {
        let shown = self.shown.borrow();
        let e = shown.get(row)?;
        Some(Row {
            name: e.name.clone(),
            type_text: e.type_text.clone(),
            size: e.size_text.clone(),
            modified: e.modified_text.clone(),
            kind: e.kind as i32,
            selected: e.selected,
        })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Whether the hidden/system filter lets an entry through.
pub fn visible(e: &Entry, show_hidden: bool) -> bool {
    show_hidden || !(e.is_hidden() || e.is_system())
}

/// What a worker pass hands back to the UI thread.
pub struct PassOutput {
    pub shown: Vec<Entry>,
    pub hidden: usize,
    pub cursor: Option<usize>,
    pub selection: BTreeSet<usize>,
}

impl PassOutput {
    /// Re-derive cursor and selection from `r` (the UI thread does this when the selection
    /// changed while the pass was on the worker).
    pub fn restore(&mut self, r: &Restore) {
        for &i in &self.selection {
            self.shown[i].selected = false;
        }
        let (cursor, selection) = restore_by_name(r, &self.shown);
        for &i in &selection {
            self.shown[i].selected = true;
        }
        self.cursor = cursor;
        self.selection = selection;
    }
}

/// The worker pass: filter a snapshot of `all`, sort it, and carry the cursor and selection
/// over by name. Pure, runs off the UI thread.
pub fn filter_sort(mut all: Vec<Entry>, show_hidden: bool, col: SortCol, asc: bool, r: &Restore) -> PassOutput {
    let before = all.len();
    all.retain(|e| visible(e, show_hidden));
    let hidden = before - all.len();
    sort_entries(&mut all, col, asc);
    for e in all.iter_mut() {
        e.selected = false;
    }
    let mut out = PassOutput { shown: all, hidden, cursor: None, selection: BTreeSet::new() };
    out.restore(r);
    out
}

/// Cursor and selection remembered by name across a re-sort, a filter change or a refresh.
#[derive(Clone, Debug, Default)]
pub struct Restore {
    pub cursor: Option<SharedString>,
    pub selected: Vec<SharedString>,
}

/// Where the remembered names sit in a new listing; names that no longer exist are dropped.
pub fn restore_by_name(r: &Restore, entries: &[Entry]) -> (Option<usize>, BTreeSet<usize>) {
    let cursor = r.cursor.as_ref().and_then(|c| entries.iter().position(|e| &e.name == c));
    let mut selection = BTreeSet::new();
    if !r.selected.is_empty() {
        let names: HashSet<&str> = r.selected.iter().map(|s| s.as_str()).collect();
        for (i, e) in entries.iter().enumerate() {
            if names.contains(e.name.as_str()) {
                selection.insert(i);
            }
        }
    }
    (cursor, selection)
}

/// A selection gesture. The discriminants are the `op` int the Slint `select` callback passes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelOp {
    /// Click or plain arrow: select only this row.
    Only = 0,
    /// Ctrl+click or Space: toggle this row.
    Toggle = 1,
    /// Shift+click or Shift+arrow: select from the anchor to this row.
    Range = 2,
    /// Ctrl+A.
    All = 3,
}

impl SelOp {
    pub fn from_i32(op: i32) -> SelOp {
        match op {
            1 => SelOp::Toggle,
            2 => SelOp::Range,
            3 => SelOp::All,
            _ => SelOp::Only,
        }
    }
}

/// The selected rows (indices into the shown list) and the anchor a range extends from.
#[derive(Clone, Debug, Default)]
pub struct Selection {
    pub set: BTreeSet<usize>,
    pub anchor: Option<usize>,
}

impl Selection {
    /// Apply a gesture over a list of `len` rows; returns the rows whose membership changed.
    pub fn apply(&mut self, op: SelOp, row: usize, len: usize) -> Vec<usize> {
        if op != SelOp::All && row >= len {
            return Vec::new();
        }
        match op {
            SelOp::Toggle => {
                if !self.set.remove(&row) {
                    self.set.insert(row);
                }
                self.anchor = Some(row);
                vec![row]
            }
            SelOp::Only => {
                self.anchor = Some(row);
                let mut changed: Vec<usize> = self.set.iter().copied().filter(|&i| i != row).collect();
                if !self.set.contains(&row) {
                    changed.push(row);
                }
                self.set = BTreeSet::from([row]);
                changed
            }
            SelOp::Range | SelOp::All => {
                let new: BTreeSet<usize> = if op == SelOp::All {
                    (0..len).collect()
                } else {
                    let a = *self.anchor.get_or_insert(row);
                    (a.min(row)..=a.max(row)).collect()
                };
                let changed = self.set.symmetric_difference(&new).copied().collect();
                self.set = new;
                changed
            }
        }
    }
}

/// Sort in place: directories first, then by the chosen column. Runs on a worker.
pub fn sort_entries(entries: &mut [Entry], col: SortCol, asc: bool) {
    entries.sort_unstable_by(|a, b| {
        let dir_order = b.is_dir().cmp(&a.is_dir());
        if dir_order != std::cmp::Ordering::Equal {
            return dir_order;
        }
        let o = match col {
            SortCol::Name => natural_cmp(&a.name, &b.name),
            SortCol::Size => a.size.cmp(&b.size).then_with(|| natural_cmp(&a.name, &b.name)),
            SortCol::Modified => a.modified.cmp(&b.modified).then_with(|| natural_cmp(&a.name, &b.name)),
            SortCol::Type => natural_cmp(&a.type_text, &b.type_text).then_with(|| natural_cmp(&a.name, &b.name)),
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
        let attrs = if is_dir { FILE_ATTRIBUTE_DIRECTORY } else { 0 };
        Entry::new(name.into(), size, 0, attrs, SharedString::default())
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
    fn filter_hides_hidden_and_system_unless_shown() {
        let a = |name: &str, attrs: u32| Entry::new(name.into(), 0, 0, attrs, "".into());
        let all = vec![
            a("b", 0),
            a("hid", FILE_ATTRIBUTE_HIDDEN),
            a("sys", FILE_ATTRIBUTE_SYSTEM),
            a("a", 0),
            a("both", FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM),
        ];
        let off = filter_sort(all.clone(), false, SortCol::Name, true, &Restore::default());
        let names: Vec<&str> = off.shown.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["a", "b"]);
        assert_eq!(off.hidden, 3);
        let on = filter_sort(all.clone(), true, SortCol::Name, true, &Restore::default());
        assert_eq!(on.shown.len(), 5);
        assert_eq!(on.hidden, 0);

        // Batches during enumeration: `all` keeps everything, `shown` only what passes.
        let m = PaneModel::default();
        m.append(all[..3].to_vec());
        m.append(all[3..].to_vec());
        assert_eq!((m.total(), m.row_count(), m.hidden_count()), (5, 2, 3));
        assert_eq!(m.row_data(1).unwrap().name.as_str(), "a");
        m.show_hidden.set(true);
        m.replace(filter_sort(m.snapshot_all(), true, SortCol::Name, true, &Restore::default()));
        assert_eq!((m.row_count(), m.hidden_count()), (5, 0));
    }

    fn set(v: &[usize]) -> BTreeSet<usize> {
        v.iter().copied().collect()
    }

    #[test]
    fn selection_gestures() {
        let mut s = Selection::default();
        assert_eq!(s.apply(SelOp::Only, 3, 10), vec![3]);
        assert_eq!(s.set, set(&[3]));
        assert_eq!(s.apply(SelOp::Toggle, 5, 10), vec![5]);
        assert_eq!(s.set, set(&[3, 5]));
        assert_eq!(s.apply(SelOp::Toggle, 3, 10), vec![3]);
        assert_eq!(s.set, set(&[5]));
        // Range runs from the anchor (3, set by the last toggle) and replaces the set.
        let mut changed = s.apply(SelOp::Range, 2, 10);
        changed.sort();
        assert_eq!(changed, vec![2, 3, 5]);
        assert_eq!(s.set, set(&[2, 3]));
        assert_eq!(s.apply(SelOp::Range, 7, 10).len(), 5); // 2 leaves, 4..=7 join
        assert_eq!(s.set, set(&[3, 4, 5, 6, 7]));
        assert_eq!(s.apply(SelOp::All, 0, 10).len(), 5);
        assert_eq!(s.set.len(), 10);
        let mut changed = s.apply(SelOp::Only, 9, 10);
        changed.sort();
        assert_eq!(changed, (0..9).collect::<Vec<_>>());
        assert_eq!(s.set, set(&[9]));
        assert!(s.apply(SelOp::Toggle, 10, 10).is_empty(), "out of range is ignored");
        // Range with no anchor selects just the row and anchors there.
        let mut fresh = Selection::default();
        fresh.apply(SelOp::Range, 4, 10);
        assert_eq!((fresh.set.clone(), fresh.anchor), (set(&[4]), Some(4)));
    }

    #[test]
    fn model_selection_mirrors_rows_and_clears_on_navigate() {
        let m = PaneModel::default();
        m.append(vec![e("a", 0, false), e("b", 0, false), e("c", 0, false)]);
        m.select(SelOp::Only, 1);
        assert!(m.row_data(1).unwrap().selected);
        m.select(SelOp::Toggle, 2);
        assert_eq!(m.selected_names(), ["b", "c"]);
        m.select(SelOp::All, 0);
        assert!((0..3).all(|i| m.row_data(i).unwrap().selected));
        let v = m.sel_version.get();
        m.clear(); // what navigate does
        assert!(m.selected_names().is_empty());
        assert!(m.sel_version.get() > v);
        m.append(vec![e("a", 0, false)]);
        assert!(!m.row_data(0).unwrap().selected);
    }

    #[test]
    fn restore_by_name_after_relisting() {
        let old = vec![e("a", 0, false), e("b", 0, false), e("c", 0, false), e("d", 0, false)];
        let m = PaneModel::default();
        m.append(old);
        m.select(SelOp::Only, 1);
        m.select(SelOp::Toggle, 3);
        let r = m.capture(3); // cursor on "d", selection {b, d}
        assert_eq!(r.cursor.as_deref(), Some("d"));
        // New listing: "b" is gone, "0" arrived first, "d" moved.
        let new = vec![e("0", 0, false), e("a", 0, false), e("c", 0, false), e("d", 0, false)];
        assert_eq!(restore_by_name(&r, &new), (Some(3), set(&[3])));
        let gone = Restore { cursor: Some("zz".into()), selected: vec!["zz".into()] };
        assert_eq!(restore_by_name(&gone, &new), (None, BTreeSet::new()));
        // A pass carries the selection over a re-sort, and marks the rows.
        let out = filter_sort(m.snapshot_all(), false, SortCol::Name, false, &r);
        let names: Vec<&str> = out.shown.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["d", "c", "b", "a"]);
        assert_eq!((out.cursor, out.selection.clone()), (Some(0), set(&[0, 2])));
        let cursor = m.replace(out);
        assert_eq!(cursor, Some(0));
        assert_eq!(m.selected_names(), ["d", "b"]);
        assert!(m.row_data(0).unwrap().selected && !m.row_data(1).unwrap().selected);
    }

    #[test]
    fn sort_by_type_then_name() {
        let t = |name: &str, ty: &str| Entry::new(name.into(), 0, 0, 0, ty.into());
        let mut v = vec![t("b.txt", "Text Document"), t("z.png", "PNG File"), t("a.txt", "Text Document"), e("dir", 0, true)];
        sort_entries(&mut v, SortCol::Type, true);
        let names: Vec<&str> = v.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["dir", "z.png", "a.txt", "b.txt"]);
        sort_entries(&mut v, SortCol::Type, false);
        let names: Vec<&str> = v.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["dir", "b.txt", "a.txt", "z.png"]);
        assert_eq!(SortCol::from_i32(3), SortCol::Type);
    }

    #[test]
    fn kind_from_attributes() {
        assert_eq!(Kind::from_attrs(0), Kind::File);
        assert_eq!(Kind::from_attrs(FILE_ATTRIBUTE_DIRECTORY), Kind::Directory);
        let junction = FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT;
        assert_eq!(Kind::from_attrs(junction), Kind::Reparse);
        let j = Entry::new("j".into(), 0, 0, junction, "File folder".into());
        assert!(j.is_dir(), "a reparse directory still opens like a directory");
        let h = Entry::new("h".into(), 0, 0, FILE_ATTRIBUTE_HIDDEN, "".into());
        assert!(h.is_hidden() && !h.is_system());
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
        m.replace(PassOutput { shown: vec![e("c", 3, true)], hidden: 0, cursor: None, selection: BTreeSet::new() });
        assert_eq!(m.row_count(), 1);
        assert_eq!(m.row_data(0).unwrap().kind, Kind::Directory as i32);
        assert_eq!(m.row_data(0).unwrap().size.as_str(), "");
    }
}
