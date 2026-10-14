//! PaneModel: a flat Vec of plain entries exposed to Slint as a virtualized model.
//! Only the UI thread touches it. Workers hand over whole Vec<Entry> batches.

use crate::Row;
use crate::icons::{IconTable, KeyState, PLACEHOLDER_DRIVE, PLACEHOLDER_FILE, PLACEHOLDER_FOLDER, PLACEHOLDERS};
use slint::{Model, ModelNotify, ModelTracker, SharedString};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::collections::{BTreeSet, HashSet};
use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SYSTEM,
};
use windows_sys::Win32::System::Time::{
    FileTimeToSystemTime, GetTimeZoneInformation, SystemTimeToTzSpecificLocalTime, TIME_ZONE_INFORMATION,
};

/// What a row is, for display. A reparse point shows as its own kind even when it is a
/// directory; a junction or directory link still opens like one (see `Entry::is_dir`).
/// The discriminants are the `kind` int the Slint `Row` carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Directory = 0,
    File = 1,
    Drive = 2,
    /// A reparse point of a tag not told apart below.
    Reparse = 3,
    Junction = 4,
    Symlink = 5,
    /// A cloud sync engine's placeholder (OneDrive and others), file or directory.
    Cloud = 6,
    /// An app execution alias (the Store's `WindowsApps` shims).
    AppAlias = 7,
}

// Reparse tags (winnt.h).
pub const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
pub const IO_REPARSE_TAG_SYMLINK: u32 = 0xA000_000C;
pub const IO_REPARSE_TAG_CLOUD: u32 = 0x9000_001A;
pub const IO_REPARSE_TAG_APPEXECLINK: u32 = 0x8000_001B;

impl Kind {
    /// `tag` is the reparse tag FindFirstFileExW reports (`dwReserved0`), read only when
    /// `attrs` has FILE_ATTRIBUTE_REPARSE_POINT.
    pub fn from_attrs(attrs: u32, tag: u32) -> Kind {
        if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            Kind::from_tag(tag)
        } else if attrs & FILE_ATTRIBUTE_DIRECTORY != 0 {
            Kind::Directory
        } else {
            Kind::File
        }
    }

    /// The kind of a reparse point by its tag. The cloud tags are IO_REPARSE_TAG_CLOUD and its
    /// sixteen numbered variants, which differ only in bits 12 to 15 (`IsReparseTagCloud`'s mask).
    pub fn from_tag(tag: u32) -> Kind {
        match tag {
            IO_REPARSE_TAG_MOUNT_POINT => Kind::Junction,
            IO_REPARSE_TAG_SYMLINK => Kind::Symlink,
            IO_REPARSE_TAG_APPEXECLINK => Kind::AppAlias,
            t if t & 0xFFFF_0FFF == IO_REPARSE_TAG_CLOUD => Kind::Cloud,
            _ => Kind::Reparse,
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
    /// On the clipboard as a cut (drawn dimmed); mirrored like `selected`, from `PaneModel::cut`.
    pub cut: bool,
    /// Index into the shared `IconTable`: a placeholder until the shell icon for `icon_key`
    /// has arrived and the row has been on screen (`PaneModel::resolve_icons`).
    pub icon: u16,
    /// Icon cache key (`icons::icon_key`), set on the enumeration worker.
    pub icon_key: SharedString,
}

impl Entry {
    #[cfg(test)]
    pub fn new(name: SharedString, size: u64, modified: i64, attrs: u32, type_text: SharedString) -> Entry {
        let kind = Kind::from_attrs(attrs, 0);
        Entry::with_zone(name, size, modified, attrs, kind, type_text, SharedString::default(), &LocalZone::current())
    }

    /// `zone` is built once per enumeration; per-entry Win32 time conversion cost about 60 ms
    /// per 40k entries (see `LocalZone`). `kind` comes from `Kind::from_attrs`.
    pub fn with_zone(
        name: SharedString,
        size: u64,
        modified: i64,
        attrs: u32,
        kind: Kind,
        type_text: SharedString,
        icon_key: SharedString,
        zone: &LocalZone,
    ) -> Entry {
        let is_dir = attrs & FILE_ATTRIBUTE_DIRECTORY != 0;
        Entry {
            icon: if is_dir { PLACEHOLDER_FOLDER } else { PLACEHOLDER_FILE },
            icon_key,
            size_text: if is_dir { SharedString::default() } else { format_size(size).into() },
            modified_text: zone.format(modified),
            type_text,
            name,
            size,
            modified,
            attrs,
            kind,
            selected: false,
            cut: false,
        }
    }

    /// A drive in the "This PC" listing: the size column carries the free space.
    pub fn drive(name: SharedString, free: Option<u64>, type_text: SharedString) -> Entry {
        Entry {
            size: free.unwrap_or(0),
            size_text: free.map(|f| format!("{} free", format_size(f)).into()).unwrap_or_default(),
            modified: 0,
            modified_text: SharedString::default(),
            type_text,
            icon: PLACEHOLDER_DRIVE,
            icon_key: name.clone(),
            name,
            attrs: FILE_ATTRIBUTE_DIRECTORY,
            kind: Kind::Drive,
            selected: false,
            cut: false,
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
    /// How many entries of `all` the hidden-file switch currently keeps out of `shown`.
    hidden: Cell<usize>,
    /// How many more the name filter keeps out (entries the hidden switch lets through).
    filtered: Cell<usize>,
    /// While a refresh enumerates, its batches collect here and the old listing stays on
    /// screen; `finish_refresh` swaps them into `all` and the next pass replaces `shown`.
    incoming: RefCell<Vec<Entry>>,
    refreshing: Cell<bool>,
    /// Selected rows, as indices into `shown`. UI thread only.
    sel: RefCell<Selection>,
    /// Bumped on every selection change, so a worker pass can tell whether the selection it
    /// carried over is still the current one when it lands.
    pub sel_version: Cell<u64>,
    /// Names on the clipboard as a cut from this pane's folder; carried across passes by name.
    cut: RefCell<Vec<SharedString>>,
    notify: ModelNotify,
    /// Generation of the enumeration whose batches we accept; stale workers are ignored.
    pub generation: Cell<u64>,
    /// Bumped by every filter/sort request; only the newest pass may swap `shown`.
    pub pass: Cell<u64>,
    pub sort_col: Cell<SortCol>,
    pub sort_asc: Cell<bool>,
    pub show_hidden: Cell<bool>,
    /// The tab's name filter (Ctrl+F), applied by the same pass as `show_hidden`; empty: none.
    pub filter: RefCell<NameMatch>,
    /// Shell icons, shared by both panes; `Entry::icon` indexes it and the pane's icon strip
    /// is composed from it (`icons::compose`).
    icons: Rc<RefCell<IconTable>>,
}

impl Default for PaneModel {
    fn default() -> Self {
        PaneModel::with_icons(Rc::new(RefCell::new(IconTable::default())))
    }
}

impl PaneModel {
    pub fn with_icons(icons: Rc<RefCell<IconTable>>) -> Self {
        PaneModel {
            icons,
            all: RefCell::new(Vec::new()),
            shown: RefCell::new(Vec::new()),
            hidden: Cell::new(0),
            filtered: Cell::new(0),
            incoming: RefCell::new(Vec::new()),
            refreshing: Cell::new(false),
            sel: RefCell::new(Selection::default()),
            sel_version: Cell::new(0),
            cut: RefCell::new(Vec::new()),
            notify: ModelNotify::default(),
            generation: Cell::new(0),
            pass: Cell::new(0),
            sort_col: Cell::new(SortCol::Name),
            sort_asc: Cell::new(true),
            show_hidden: Cell::new(false),
            filter: RefCell::new(NameMatch::default()),
        }
    }

    /// Give the rows in `first..=last` (clamped to the list) their shell icon at `px` where the
    /// table has it, and call `request` once for each key it has not seen, with its row. Only
    /// placeholder rows are looked at, so a screenful that is already resolved costs a few dozen
    /// integer compares. A thumbnail key past the table's cap becomes its extension's key here.
    /// Returns the rows that changed (no row notification: rows carry no icon, the pane's
    /// strip does).
    pub fn resolve_icons(&self, first: usize, last: usize, px: u32, mut request: impl FnMut(usize, &Entry)) -> Vec<usize> {
        let mut changed = Vec::new();
        let mut table = self.icons.borrow_mut();
        let (keys, cap) = table.keys(px);
        let mut shown = self.shown.borrow_mut();
        let Some(end) = shown.len().checked_sub(1).map(|n| n.min(last)) else { return changed };
        for i in first..=end {
            let e = &mut shown[i];
            if e.icon >= PLACEHOLDERS || e.icon_key.is_empty() {
                continue;
            }
            let mut state = keys.state(&e.icon_key);
            if state == KeyState::Missing {
                if let Some(k) = cap.capped(&e.icon_key) {
                    e.icon_key = k;
                    state = keys.state(&e.icon_key);
                }
            }
            match state {
                KeyState::Ready(idx) => {
                    e.icon = idx;
                    changed.push(i);
                }
                KeyState::Missing => {
                    keys.begin(&e.icon_key);
                    request(i, e);
                }
                KeyState::Pending | KeyState::Failed => {}
            }
        }
        changed
    }

    /// The icon table index of every row in `first..=last` (clamped to the list), in order.
    pub fn icon_indices(&self, first: usize, last: usize) -> Vec<u16> {
        let shown = self.shown.borrow();
        let end = (last + 1).min(shown.len());
        shown.get(first..end).map_or_else(Vec::new, |rows| rows.iter().map(|e| e.icon).collect())
    }

    pub fn clear(&self) {
        self.all.borrow_mut().clear();
        self.shown.borrow_mut().clear();
        self.hidden.set(0);
        self.filtered.set(0);
        self.incoming.borrow_mut().clear();
        self.refreshing.set(false);
        *self.sel.borrow_mut() = Selection::default();
        self.sel_version.set(self.sel_version.get() + 1);
        self.cut.borrow_mut().clear();
        self.notify.reset();
    }

    /// Mark `names` as cut (empty: none); only rows whose mark changed are notified.
    pub fn set_cut(&self, names: Vec<SharedString>) {
        let set: HashSet<&str> = names.iter().map(|n| n.as_str()).collect();
        let mut changed = Vec::new();
        {
            let mut shown = self.shown.borrow_mut();
            for (i, e) in shown.iter_mut().enumerate() {
                let cut = set.contains(e.name.as_str());
                if e.cut != cut {
                    e.cut = cut;
                    changed.push(i);
                }
            }
        }
        drop(set);
        *self.cut.borrow_mut() = names;
        // A pass already on the worker carries the old marks; this makes the swap re-derive them.
        self.sel_version.set(self.sel_version.get() + 1);
        for i in changed {
            self.notify.row_changed(i);
        }
    }

    /// The entries a file operation acts on: the selection, or the cursor row when nothing is
    /// selected.
    pub fn targets(&self, cursor: i32) -> Vec<Entry> {
        let shown = self.shown.borrow();
        let sel = self.sel.borrow();
        if sel.set.is_empty() {
            return usize::try_from(cursor).ok().and_then(|i| shown.get(i)).cloned().into_iter().collect();
        }
        sel.set.iter().filter_map(|&i| shown.get(i).cloned()).collect()
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

    pub fn selected_count(&self) -> usize {
        self.sel.borrow().set.len()
    }

    /// For the status bar: the selected row count, the total size of the selected files
    /// (folders and drives count no bytes), and whether any file is selected.
    pub fn selection_summary(&self) -> (usize, u64, bool) {
        let shown = self.shown.borrow();
        let sel = self.sel.borrow();
        let (mut bytes, mut any_file) = (0u64, false);
        for e in sel.set.iter().filter_map(|&i| shown.get(i)) {
            if !e.is_dir() {
                bytes = bytes.saturating_add(e.size);
                any_file = true;
            }
        }
        (sel.set.len(), bytes, any_file)
    }

    /// Names of the selected plain folders (not links, placeholders or drives), in display order:
    /// the ones whose size the size worker computes.
    pub fn selected_folders(&self) -> Vec<String> {
        let shown = self.shown.borrow();
        let sel = self.sel.borrow();
        sel.set.iter().filter_map(|&i| shown.get(i)).filter(|e| e.kind == Kind::Directory).map(|e| e.name.to_string()).collect()
    }

    /// The plain folders shown, with their rows: what Alt+Return sizes.
    pub fn folder_rows(&self) -> Vec<(usize, String)> {
        let shown = self.shown.borrow();
        shown.iter().enumerate().filter(|(_, e)| e.kind == Kind::Directory).map(|(i, e)| (i, e.name.to_string())).collect()
    }

    /// A folder's computed size (None: not known, an empty cell) into its entry in the listing
    /// and its row, which alone is notified. The next pass sorts by it.
    pub fn set_size(&self, name: &str, bytes: Option<u64>) {
        let text: SharedString = bytes.map(format_size).unwrap_or_default().into();
        if let Some(e) = self.all.borrow_mut().iter_mut().find(|e| e.name == name) {
            e.size = bytes.unwrap_or(0);
            e.size_text = text.clone();
        }
        let row = {
            let mut shown = self.shown.borrow_mut();
            let row = shown.iter().position(|e| e.name == name);
            if let Some(e) = row.map(|i| &mut shown[i]) {
                e.size = bytes.unwrap_or(0);
                e.size_text = text;
            }
            row
        };
        if let Some(i) = row {
            self.notify.row_changed(i);
        }
    }

    /// Put cached folder sizes back into a fresh listing (after a refresh that kept the cache), in
    /// one pass over it; the pass that follows shows and sorts them.
    pub fn apply_sizes(&self, size_of: impl Fn(&str) -> Option<u64>) {
        for e in self.all.borrow_mut().iter_mut() {
            if let Some(b) = size_of(e.name.as_str()) {
                e.size = b;
                e.size_text = format_size(b).into();
            }
        }
    }

    /// The cursor and selection by name, to carry across a pass or a refresh.
    pub fn capture(&self, cursor: i32) -> Restore {
        let shown = self.shown.borrow();
        let cursor = usize::try_from(cursor).ok().and_then(|i| shown.get(i)).map(|e| e.name.clone());
        drop(shown);
        Restore { cursor, selected: self.selected_names(), cut: self.cut.borrow().clone(), fallback_index: None }
    }

    /// Before a job that removes `removed` from this folder (delete, move out): the index the
    /// cursor should fall back to if its row goes, which is its row less the removed rows above
    /// it (the row that followed it once they are gone). None without a cursor.
    pub fn cursor_after_removal(&self, cursor: i32, removed: &[SharedString]) -> Option<usize> {
        let c = usize::try_from(cursor).ok()?;
        let shown = self.shown.borrow();
        if c >= shown.len() {
            return None;
        }
        let names: HashSet<&str> = removed.iter().map(|s| s.as_str()).collect();
        Some(c - shown[..c].iter().filter(|e| names.contains(e.name.as_str())).count())
    }

    /// Append a batch; one row_added notification per batch, never per entry. The batch moves
    /// into `all`; `shown` gets clones of the entries the filter keeps (refcount bumps only).
    pub fn append(&self, mut batch: Vec<Entry>) {
        if self.refreshing.get() {
            self.incoming.borrow_mut().append(&mut batch);
            return;
        }
        let show_hidden = self.show_hidden.get();
        let filter = self.filter.borrow();
        let start = self.shown.borrow().len();
        {
            let mut shown = self.shown.borrow_mut();
            shown.extend(batch.iter().filter(|e| visible(e, show_hidden, &filter)).cloned());
        }
        let added = self.shown.borrow().len() - start;
        let hidden = batch.iter().filter(|e| !shows(e, show_hidden)).count();
        self.hidden.set(self.hidden.get() + hidden);
        self.filtered.set(self.filtered.get() + batch.len() - added - hidden);
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
        self.filtered.set(out.filtered);
        *self.sel.borrow_mut() = Selection { set: out.selection, anchor: out.cursor };
        self.sel_version.set(self.sel_version.get() + 1);
        self.notify.reset();
        out.cursor
    }

    /// Start re-enumerating in place: batches are staged, the current rows stay shown.
    pub fn begin_refresh(&self) {
        self.incoming.borrow_mut().clear();
        self.refreshing.set(true);
    }

    pub fn is_refreshing(&self) -> bool {
        self.refreshing.get()
    }

    /// The refresh listing is complete: it becomes `all`. `shown` (and the selection indices
    /// into it) stay valid until the following pass swaps in the new rows.
    pub fn finish_refresh(&self) {
        *self.all.borrow_mut() = std::mem::take(&mut *self.incoming.borrow_mut());
        self.refreshing.set(false);
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

    /// Entries the hidden-file switch keeps out of view.
    pub fn hidden_count(&self) -> usize {
        self.hidden.get()
    }

    /// Entries the name filter keeps out of view (besides the hidden ones).
    pub fn filtered_count(&self) -> usize {
        self.filtered.get()
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
            cut: e.cut,
        })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Whether the hidden-file switch and the name filter let an entry through.
pub fn visible(e: &Entry, show_hidden: bool, filter: &NameMatch) -> bool {
    shows(e, show_hidden) && filter.matches(&e.name)
}

/// Whether the hidden-file switch lets an entry through.
fn shows(e: &Entry, show_hidden: bool) -> bool {
    show_hidden || !(e.is_hidden() || e.is_system())
}

/// The name filter's pattern, prepared once per pass: case-insensitive (Unicode lowercase on both
/// sides), a substring of the name, or with `*` or `?` in it a glob over the whole name. Only the
/// part after the last `\` is tested, so a search tab's relative names (`sub\file.txt`) are
/// filtered by the file's own name. Shared by the filter and the recursive search.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NameMatch {
    /// What was typed (for the log and the box).
    pub pattern: String,
    /// Its lowercase form, and the same as characters for a glob.
    lower: String,
    glob: Option<Vec<char>>,
}

impl NameMatch {
    pub fn new(pattern: &str) -> NameMatch {
        let lower = pattern.to_lowercase();
        let glob = lower.contains(['*', '?']).then(|| lower.chars().collect());
        NameMatch { pattern: pattern.to_string(), lower, glob }
    }

    pub fn is_empty(&self) -> bool {
        self.lower.is_empty()
    }

    pub fn is_glob(&self) -> bool {
        self.glob.is_some()
    }

    /// An empty pattern matches every name, without lowercasing anything.
    pub fn matches(&self, name: &str) -> bool {
        if self.lower.is_empty() {
            return true;
        }
        let leaf = name.rsplit('\\').next().unwrap_or(name).to_lowercase();
        match &self.glob {
            Some(p) => glob_match(p, &leaf.chars().collect::<Vec<char>>()),
            None => leaf.contains(self.lower.as_str()),
        }
    }
}

/// `*` matches any run of characters (none included), `?` exactly one; anchored at both ends.
/// Iterative, backtracking only to the last `*`.
fn glob_match(p: &[char], s: &[char]) -> bool {
    let (mut pi, mut si) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while si < s.len() {
        if pi < p.len() && (p[pi] == '?' || (p[pi] != '*' && p[pi] == s[si])) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, si));
            pi += 1;
        } else if let Some((sp, ss)) = star {
            pi = sp + 1;
            si = ss + 1;
            star = Some((sp, ss + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

/// What a worker pass hands back to the UI thread.
pub struct PassOutput {
    pub shown: Vec<Entry>,
    pub hidden: usize,
    /// Entries the name filter left out (besides the hidden ones).
    pub filtered: usize,
    pub cursor: Option<usize>,
    pub selection: BTreeSet<usize>,
    /// Whether `restore` marked any entry as cut (so a later restore knows to clear them).
    pub cut_marked: bool,
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
        if !r.cut.is_empty() || self.cut_marked {
            let names: HashSet<&str> = r.cut.iter().map(|s| s.as_str()).collect();
            for e in self.shown.iter_mut() {
                e.cut = names.contains(e.name.as_str());
            }
            self.cut_marked = !r.cut.is_empty();
        }
    }
}

/// The worker pass: filter a snapshot of `all` (hidden-file switch, then name filter), sort it,
/// and carry the cursor and selection over by name. Pure, runs off the UI thread.
pub fn filter_sort(mut all: Vec<Entry>, show_hidden: bool, filter: &NameMatch, col: SortCol, asc: bool, r: &Restore) -> PassOutput {
    let before = all.len();
    all.retain(|e| shows(e, show_hidden));
    let hidden = before - all.len();
    let unfiltered = all.len();
    if !filter.is_empty() {
        all.retain(|e| filter.matches(&e.name));
    }
    let filtered = unfiltered - all.len();
    sort_entries(&mut all, col, asc);
    for e in all.iter_mut() {
        e.selected = false;
    }
    let mut out = PassOutput { shown: all, hidden, filtered, cursor: None, selection: BTreeSet::new(), cut_marked: false };
    out.restore(r);
    out
}

/// Cursor and selection remembered by name across a re-sort, a filter change or a refresh.
#[derive(Clone, Debug, Default)]
pub struct Restore {
    pub cursor: Option<SharedString>,
    pub selected: Vec<SharedString>,
    /// Names marked as cut (rarely non-empty; the pass marks them on the worker).
    pub cut: Vec<SharedString>,
    /// Where the cursor goes when its row is gone (after a delete or a move out of the folder):
    /// the old row's index, less the removed rows above it, so the cursor lands on the row that
    /// followed it, as in Explorer. Clamped to the last row. Set once, by the refresh after a job.
    pub fallback_index: Option<usize>,
}

/// Where the remembered names sit in a new listing; names that no longer exist are dropped.
/// A cursor whose name is gone (or that had none) goes to `fallback_index`, clamped to the list.
pub fn restore_by_name(r: &Restore, entries: &[Entry]) -> (Option<usize>, BTreeSet<usize>) {
    let cursor = r
        .cursor
        .as_ref()
        .and_then(|c| entries.iter().position(|e| &e.name == c))
        .or_else(|| Some(r.fallback_index?.min(entries.len().checked_sub(1)?)));
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

/// The list's content-y (0 or negative, as Slint's ListView keeps it) that brings `row` fully
/// into a viewport `visible_h` high with the least movement, or None when it is already
/// visible (or the viewport has no height yet). The same arithmetic as the pane's
/// `bring-into-view`.
pub fn reveal_row(content_y: f32, visible_h: f32, row: usize, row_px: f32) -> Option<f32> {
    if visible_h <= 0.0 {
        return None;
    }
    let top = row as f32 * row_px;
    let target = if content_y + top < 0.0 {
        -top
    } else if content_y + top + row_px > visible_h {
        // Bottom-aligned, but never past the row's top (a row taller than the viewport).
        (visible_h - top - row_px).max(-top).min(0.0)
    } else {
        return None;
    };
    (target != content_y).then_some(target)
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
    /// Right-click: an unselected row becomes the only selection; a selected row keeps the
    /// selection as it is (Explorer behaviour, so the menu acts on everything selected).
    Context = 4,
    /// Right-click in the empty space below the rows: nothing stays selected (Explorer). The
    /// row is ignored.
    Clear = 5,
}

impl SelOp {
    pub fn from_i32(op: i32) -> SelOp {
        match op {
            1 => SelOp::Toggle,
            2 => SelOp::Range,
            3 => SelOp::All,
            4 => SelOp::Context,
            5 => SelOp::Clear,
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
        if op != SelOp::All && op != SelOp::Clear && row >= len {
            return Vec::new();
        }
        match op {
            SelOp::Clear => {
                self.anchor = None;
                std::mem::take(&mut self.set).into_iter().collect()
            }
            SelOp::Context if self.set.contains(&row) => {
                self.anchor = Some(row);
                Vec::new()
            }
            SelOp::Context => self.apply(SelOp::Only, row, len),
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
pub(crate) fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
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
#[cfg(test)]
pub fn format_time(secs: i64) -> String {
    format_time_in(secs, None)
}

/// Smallest and largest Unix seconds a FILETIME can hold (0 to i64::MAX ticks since 1601), the
/// range outside which the Win32 reference path returns an empty string.
const MIN_SECS: i64 = -11_644_473_600;
const MAX_SECS: i64 = i64::MAX / 10_000_000 - 11_644_473_600;

/// A time zone's rules, read once per enumeration from a `TIME_ZONE_INFORMATION`, so the local
/// time of each entry is pure arithmetic. Two Win32 calls plus a `format!` per entry cost about
/// 1.5 us, 60 ms on a 40k folder. Matches `SystemTimeToTzSpecificLocalTime` (the reference in
/// the tests): transitions are in the local time of the period being left, offsets in minutes
/// with UTC = local + bias.
pub struct LocalZone {
    /// UTC minus local, in seconds, outside and inside daylight time.
    std_off: i64,
    dst_off: i64,
    rules: Rules,
    /// The last year looked up, with the UTC milliseconds at which its daylight time starts
    /// and ends. A folder's dates cluster in a few years and recomputing a year is a few
    /// dozen integer operations, so one entry beats a map.
    cache: Cell<(i64, i64, i64)>,
}

enum Rules {
    NoDst,
    /// The usual "week N of month" rules (`wYear == 0`): daylight start, standard start.
    Annual(SYSTEMTIME, SYSTEMTIME),
    /// Absolute-date rules (`wYear != 0`), which `GetTimeZoneInformation` does not return in
    /// practice: left to the Win32 path.
    Win32(TIME_ZONE_INFORMATION),
}

impl LocalZone {
    pub fn current() -> LocalZone {
        LocalZone::from_tzi(&current_tz())
    }

    pub fn from_tzi(tzi: &TIME_ZONE_INFORMATION) -> LocalZone {
        let (sd, dd) = (tzi.StandardDate, tzi.DaylightDate);
        let rules = if sd.wMonth == 0 || dd.wMonth == 0 {
            Rules::NoDst
        } else if sd.wYear == 0 && dd.wYear == 0 {
            Rules::Annual(dd, sd)
        } else {
            Rules::Win32(*tzi)
        };
        LocalZone {
            std_off: (tzi.Bias as i64 + tzi.StandardBias as i64) * 60,
            dst_off: (tzi.Bias as i64 + tzi.DaylightBias as i64) * 60,
            rules,
            cache: Cell::new((i64::MIN, 0, 0)),
        }
    }

    /// UTC minus local, in seconds, at the instant `secs` (Unix seconds, within the FILETIME range).
    fn offset(&self, secs: i64) -> i64 {
        let Rules::Annual(dst_rule, std_rule) = &self.rules else { return self.std_off };
        let year = civil_from_days((secs - self.std_off).div_euclid(86_400)).0;
        let (mut y, mut start, mut end) = self.cache.get();
        if y != year {
            y = year;
            start = transition_ms(year, dst_rule) + self.std_off * 1000;
            end = transition_ms(year, std_rule) + self.dst_off * 1000;
            self.cache.set((y, start, end));
        }
        let t = secs * 1000;
        let dst = if start < end { start <= t && t < end } else { t >= start || t < end };
        if dst { self.dst_off } else { self.std_off }
    }

    /// Unix seconds of a wall-clock date and time in this zone (a zip entry's MS-DOS time is local).
    pub fn from_local(&self, y: i64, m: u32, d: u32, secs_of_day: i64) -> i64 {
        let local = days_from_civil(y, m, d) * 86_400 + secs_of_day;
        local + self.offset(local + self.std_off)
    }

    /// `YYYY-MM-DD HH:MM` in this zone, or an empty string where the Win32 path also fails.
    /// Written straight into the SharedString, with no intermediate String.
    pub fn format(&self, secs: i64) -> SharedString {
        if let Rules::Win32(tzi) = &self.rules {
            return format_time_in(secs, Some(tzi)).into();
        }
        if !(MIN_SECS..=MAX_SECS).contains(&secs) {
            return SharedString::default();
        }
        let local = secs - self.offset(secs);
        let (y, mo, d) = civil_from_days(local.div_euclid(86_400));
        if !(1601..=30827).contains(&y) {
            return SharedString::default();
        }
        let tod = local.rem_euclid(86_400);
        // A year past 9999 takes five digits, as the reference `{:04}` does.
        let yl = if y >= 10_000 { 5 } else { 4 };
        let mut buf = [0u8; 17];
        let mut v = y;
        for i in (0..yl).rev() {
            buf[i] = b'0' + (v % 10) as u8;
            v /= 10;
        }
        // Separators and the remaining fields sit at fixed positions after the year.
        buf[yl] = b'-';
        buf[yl + 3] = b'-';
        buf[yl + 6] = b' ';
        buf[yl + 9] = b':';
        for (at, v) in [(yl + 1, mo as i64), (yl + 4, d as i64), (yl + 7, tod / 3600), (yl + 10, tod / 60 % 60)] {
            buf[at] = b'0' + (v / 10) as u8;
            buf[at + 1] = b'0' + (v % 10) as u8;
        }
        // Only ASCII digits and separators were written, so this never falls back to "".
        std::str::from_utf8(&buf[..yl + 12]).unwrap_or_default().into()
    }
}

/// Local milliseconds since the Unix epoch (as if UTC) of a "week N of month" rule in `year`.
fn transition_ms(year: i64, r: &SYSTEMTIME) -> i64 {
    let month = r.wMonth.clamp(1, 12) as u32;
    let first = days_from_civil(year, month, 1);
    let first_dow = (first + 4).rem_euclid(7); // 1970-01-01 was a Thursday; Sunday = 0
    let mut day = 1 + (r.wDayOfWeek as i64 - first_dow).rem_euclid(7) + (r.wDay.clamp(1, 5) as i64 - 1) * 7;
    let next = if month == 12 { days_from_civil(year + 1, 1, 1) } else { days_from_civil(year, month + 1, 1) };
    while day > next - first {
        day -= 7; // week 5 means the last such weekday
    }
    let secs = (first + day - 1) * 86_400 + r.wHour as i64 * 3600 + r.wMinute as i64 * 60 + r.wSecond as i64;
    secs * 1000 + r.wMilliseconds as i64
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Year, month and day of a day count since 1970-01-01 (the inverse of `days_from_civil`).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + (m <= 2) as i64, m, d)
}

/// The current zone, with its daylight rules, so the conversion stays correct across a DST
/// boundary even though it is fetched once per enumeration rather than per entry.
pub fn current_tz() -> TIME_ZONE_INFORMATION {
    let mut tz: TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: tz is a live, writable TIME_ZONE_INFORMATION. The return code only tells whether
    // DST is in effect; on failure the zeroed struct means UTC, which is a visible but harmless result.
    unsafe { GetTimeZoneInformation(&mut tz) };
    tz
}

/// `tz = None` uses the current time zone; tests pass a fixed one.
fn format_time_in(secs: i64, tz: Option<&TIME_ZONE_INFORMATION>) -> String {
    // FILETIME counts 100 ns ticks since 1601-01-01; the Unix epoch is 11,644,473,600 s later.
    let ticks = secs.saturating_add(11_644_473_600).saturating_mul(10_000_000);
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
        let off = filter_sort(all.clone(), false, &NameMatch::default(), SortCol::Name, true, &Restore::default());
        let names: Vec<&str> = off.shown.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["a", "b"]);
        assert_eq!(off.hidden, 3);
        let on = filter_sort(all.clone(), true, &NameMatch::default(), SortCol::Name, true, &Restore::default());
        assert_eq!(on.shown.len(), 5);
        assert_eq!(on.hidden, 0);

        // Batches during enumeration: `all` keeps everything, `shown` only what passes.
        let m = PaneModel::default();
        m.append(all[..3].to_vec());
        m.append(all[3..].to_vec());
        assert_eq!((m.total(), m.row_count(), m.hidden_count()), (5, 2, 3));
        assert_eq!(m.row_data(1).unwrap().name.as_str(), "a");
        m.show_hidden.set(true);
        m.replace(filter_sort(m.snapshot_all(), true, &NameMatch::default(), SortCol::Name, true, &Restore::default()));
        assert_eq!((m.row_count(), m.hidden_count()), (5, 0));
    }

    #[test]
    fn name_match_substring_glob_and_case() {
        let m = |p: &str, n: &str| NameMatch::new(p).matches(n);
        // Empty: everything; substring anywhere in the name, either case.
        assert!(NameMatch::new("").is_empty() && m("", "anything.txt"));
        assert!(m("port", "Report.DOC") && m("REPORT", "my report.doc") && !m("x", "report.doc"));
        assert!(!NameMatch::new("port").is_glob() && NameMatch::new("*.txt").is_glob() && NameMatch::new("a?").is_glob());
        // Unicode lowercase on both sides.
        assert!(m("ÉTÉ", "Photos été 2024.jpg") && m("straße", "STRASSE-straße.txt") && !m("é", "ete.txt"));
        // Globs are anchored over the whole name.
        assert!(m("*.txt", "notes.TXT") && !m("*.txt", "notes.txt.bak") && !m("*.txt", "txt"));
        assert!(m("report?.doc*", "report1.docx") && m("report?.doc*", "Report2.doc") && !m("report?.doc*", "report12.doc"));
        assert!(m("*7*", "file00007") && !m("*7*", "file00001") && m("*", "") && !m("?", ""));
        assert!(m("a*b*c", "aXbYc") && !m("a*b*c", "aXbYcd") && m("*a", "banana"));
        // A search tab's relative name is matched by its own name only.
        assert!(m("dll", r"sub\kernel32.dll") && !m("sub", r"sub\kernel32.dll") && m("k*.dll", r"a\b\kernel32.dll"));
    }

    #[test]
    fn filter_and_hidden_combine_in_the_pass_and_the_batches() {
        let a = |name: &str, attrs: u32| Entry::new(name.into(), 0, 0, attrs, "".into());
        let all = vec![a("one.txt", 0), a("two.log", 0), a("three.txt", FILE_ATTRIBUTE_HIDDEN), a("four.TXT", 0), a("sys.txt", FILE_ATTRIBUTE_SYSTEM)];
        let txt = NameMatch::new("*.txt");
        assert!(visible(&all[0], false, &txt) && !visible(&all[1], false, &txt) && !visible(&all[2], false, &txt));
        assert!(visible(&all[2], true, &txt), "the filter never reveals a hidden file, the switch does");
        let off = filter_sort(all.clone(), false, &txt, SortCol::Name, true, &Restore::default());
        let names: Vec<&str> = off.shown.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["four.TXT", "one.txt"]);
        assert_eq!((off.hidden, off.filtered), (2, 1));
        let on = filter_sort(all.clone(), true, &txt, SortCol::Name, true, &Restore::default());
        assert_eq!((on.shown.len(), on.hidden, on.filtered), (4, 0, 1));

        // Batches during enumeration count the two apart as the pass does.
        let m = PaneModel::default();
        *m.filter.borrow_mut() = txt.clone();
        m.append(all[..2].to_vec());
        m.append(all[2..].to_vec());
        assert_eq!((m.total(), m.row_count(), m.hidden_count(), m.filtered_count()), (5, 2, 2, 1));
        // Cleared: the next pass shows every entry the hidden switch lets through.
        m.replace(filter_sort(m.snapshot_all(), false, &NameMatch::default(), SortCol::Name, true, &Restore::default()));
        assert_eq!((m.row_count(), m.hidden_count(), m.filtered_count()), (3, 2, 0));
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
    fn right_click_selects_an_unselected_row_and_keeps_a_selected_one() {
        let mut s = Selection::default();
        s.apply(SelOp::Only, 2, 10);
        s.apply(SelOp::Toggle, 4, 10);
        s.apply(SelOp::Toggle, 6, 10);
        assert!(s.apply(SelOp::Context, 4, 10).is_empty(), "a selected row keeps the selection");
        assert_eq!(s.set, set(&[2, 4, 6]));
        let mut changed = s.apply(SelOp::Context, 5, 10);
        changed.sort();
        assert_eq!(changed, vec![2, 4, 5, 6], "an unselected row becomes the only selection");
        assert_eq!((s.set.clone(), s.anchor), (set(&[5]), Some(5)));
        assert!(s.apply(SelOp::Context, 10, 10).is_empty(), "out of range is ignored");
        assert_eq!(SelOp::from_i32(4), SelOp::Context);
        // Right-click in the empty space clears it, whatever the row says.
        s.apply(SelOp::Toggle, 7, 10);
        let mut changed = s.apply(SelOp::Clear, usize::MAX, 10);
        changed.sort();
        assert_eq!((changed, s.set.is_empty(), s.anchor), (vec![5, 7], true, None));
        assert!(s.apply(SelOp::Clear, 0, 10).is_empty(), "nothing selected: nothing changes");
        assert_eq!(SelOp::from_i32(5), SelOp::Clear);
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
        let gone = Restore { cursor: Some("zz".into()), selected: vec!["zz".into()], cut: vec![], fallback_index: None };
        assert_eq!(restore_by_name(&gone, &new), (None, BTreeSet::new()));
        // A pass carries the selection over a re-sort, and marks the rows.
        let out = filter_sort(m.snapshot_all(), false, &NameMatch::default(), SortCol::Name, false, &r);
        let names: Vec<&str> = out.shown.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["d", "c", "b", "a"]);
        assert_eq!((out.cursor, out.selection.clone()), (Some(0), set(&[0, 2])));
        let cursor = m.replace(out);
        assert_eq!(cursor, Some(0));
        assert_eq!(m.selected_names(), ["d", "b"]);
        assert!(m.row_data(0).unwrap().selected && !m.row_data(1).unwrap().selected);
    }

    #[test]
    fn cursor_falls_back_to_the_next_row() {
        let m = PaneModel::default();
        m.append(["a", "b", "c", "d", "e"].iter().map(|n| e(n, 0, false)).collect());
        // Cursor on "c", deleting "b" and "c": "d" follows, at index 1 once both are gone.
        let fb = m.cursor_after_removal(2, &["b".into(), "c".into()]);
        assert_eq!(fb, Some(1));
        assert_eq!(m.cursor_after_removal(-1, &["a".into()]), None, "no cursor, no fallback");
        assert_eq!(m.cursor_after_removal(9, &["a".into()]), None);
        let r = Restore { cursor: Some("c".into()), fallback_index: fb, ..Default::default() };
        let after = vec![e("a", 0, false), e("d", 0, false), e("e", 0, false)];
        assert_eq!(restore_by_name(&r, &after).0, Some(1), "the cursor row is gone: the next row");
        // The cursor row still there (a delete of other rows, cancelled): by name wins.
        let kept = vec![e("a", 0, false), e("c", 0, false), e("d", 0, false), e("e", 0, false)];
        assert_eq!(restore_by_name(&r, &kept).0, Some(1));
        let r2 = Restore { fallback_index: Some(3), ..r.clone() };
        assert_eq!(restore_by_name(&r2, &kept).0, Some(1), "by name wins over the fallback");
        // The last rows went: clamped to the new last row.
        let r3 = Restore { cursor: Some("e".into()), fallback_index: Some(4), ..Default::default() };
        assert_eq!(restore_by_name(&r3, &after[..2]).0, Some(1));
        // Everything went: no cursor.
        assert_eq!(restore_by_name(&r, &[]).0, None);
        // No fallback: a gone cursor goes nowhere, as before.
        let r4 = Restore { cursor: Some("zz".into()), ..Default::default() };
        assert_eq!(restore_by_name(&r4, &after).0, None);
    }

    #[test]
    fn reveal_row_moves_the_least() {
        // 10 rows of 22 px visible (220 px), scrolled to the top.
        assert_eq!(reveal_row(0.0, 220.0, 3, 22.0), None, "already visible");
        assert_eq!(reveal_row(0.0, 220.0, 9, 22.0), None, "last fully visible row");
        assert_eq!(reveal_row(0.0, 220.0, 10, 22.0), Some(-22.0), "one row below: scroll one row");
        assert_eq!(reveal_row(0.0, 220.0, 50, 22.0), Some(220.0 - 51.0 * 22.0), "far below: lands at the bottom");
        assert_eq!(reveal_row(-440.0, 220.0, 5, 22.0), Some(-110.0), "above: lands at the top");
        assert_eq!(reveal_row(-440.0, 220.0, 20, 22.0), None);
        assert_eq!(reveal_row(0.0, 0.0, 5, 22.0), None, "no viewport yet");
        assert_eq!(reveal_row(0.0, 10.0, 0, 22.0), None, "a row taller than the viewport at the top stays put");
    }

    #[test]
    fn refresh_keeps_old_rows_until_the_pass() {
        let m = PaneModel::default();
        m.append(vec![e("a", 0, false), e("b", 0, false)]);
        m.select(SelOp::Only, 1);
        let r = m.capture(1);
        m.begin_refresh();
        m.append(vec![e("b", 0, false), e("c", 0, false), e("0", 0, false)]);
        assert_eq!((m.row_count(), m.total()), (2, 2), "old rows stay until the swap");
        m.finish_refresh();
        assert!(!m.is_refreshing());
        assert_eq!((m.row_count(), m.total()), (2, 3));
        let cursor = m.replace(filter_sort(m.snapshot_all(), false, &NameMatch::default(), SortCol::Name, true, &r));
        assert_eq!(cursor, Some(1)); // new order "0", "b", "c"
        assert_eq!(m.selected_names(), ["b"]);
        assert_eq!(m.row_count(), 3);
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
        assert_eq!(Kind::from_attrs(0, 0), Kind::File);
        assert_eq!(Kind::from_attrs(FILE_ATTRIBUTE_DIRECTORY, 0), Kind::Directory);
        let junction = FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT;
        assert_eq!(Kind::from_attrs(junction, 0), Kind::Reparse);
        assert_eq!(Kind::from_attrs(junction, IO_REPARSE_TAG_MOUNT_POINT), Kind::Junction);
        // The tag is read only for a reparse point.
        assert_eq!(Kind::from_attrs(FILE_ATTRIBUTE_DIRECTORY, IO_REPARSE_TAG_MOUNT_POINT), Kind::Directory);
        assert_eq!(Kind::from_attrs(0, IO_REPARSE_TAG_SYMLINK), Kind::File);
        let j = Entry::new("j".into(), 0, 0, junction, "File folder".into());
        assert!(j.is_dir(), "a reparse directory still opens like a directory");
        let h = Entry::new("h".into(), 0, 0, FILE_ATTRIBUTE_HIDDEN, "".into());
        assert!(h.is_hidden() && !h.is_system());
    }

    #[test]
    fn reparse_tags_map_to_kinds() {
        assert_eq!(Kind::from_tag(0xA000_0003), Kind::Junction);
        assert_eq!(Kind::from_tag(0xA000_000C), Kind::Symlink);
        assert_eq!(Kind::from_tag(0x8000_001B), Kind::AppAlias);
        // IO_REPARSE_TAG_CLOUD and CLOUD_1 to CLOUD_F (OneDrive uses several of them).
        assert_eq!(Kind::from_tag(0x9000_001A), Kind::Cloud);
        for n in 1..=0xFu32 {
            assert_eq!(Kind::from_tag(0x9000_001A | (n << 12)), Kind::Cloud, "CLOUD_{n:X}");
        }
        // Others: dedup, WIM, WOF (compressed system files), AF_UNIX sockets, an unknown tag.
        for t in [0x8000_0013u32, 0x8000_0008, 0x8000_0017, 0x8000_0023, 0x9000_001B, 0x1234_5678] {
            assert_eq!(Kind::from_tag(t), Kind::Reparse, "{t:#x}");
        }
        // The discriminants the Slint Row's `kind` comment documents.
        let kinds = [Kind::Directory, Kind::File, Kind::Drive, Kind::Reparse, Kind::Junction, Kind::Symlink, Kind::Cloud, Kind::AppAlias];
        assert_eq!(kinds.map(|k| k as i32), [0, 1, 2, 3, 4, 5, 6, 7]);
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

    fn rule(month: u16, dow: u16, week: u16, hour: u16) -> SYSTEMTIME {
        SYSTEMTIME { wYear: 0, wMonth: month, wDayOfWeek: dow, wDay: week, wHour: hour, wMinute: 0, wSecond: 0, wMilliseconds: 0 }
    }

    /// US Eastern: DST from the 2nd Sunday of March 02:00 to the 1st Sunday of November 02:00.
    fn us_eastern() -> TIME_ZONE_INFORMATION {
        let mut tz = utc_minus_5();
        tz.DaylightBias = -60;
        tz.DaylightDate = rule(3, 0, 2, 2);
        tz.StandardDate = rule(11, 0, 1, 2);
        tz
    }

    /// UTC+10 with DST spanning the new year: 1st Sunday of October 02:00 to 1st Sunday of April 03:00.
    fn southern() -> TIME_ZONE_INFORMATION {
        let mut tz: TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
        tz.Bias = -600;
        tz.DaylightBias = -60;
        tz.DaylightDate = rule(10, 0, 1, 2);
        tz.StandardDate = rule(4, 0, 1, 3);
        tz
    }

    /// The arithmetic path against the Win32 reference: every 7 minutes over 2024 to 2027, and
    /// every 10 seconds within two hours of each transition. Returns how many points compared.
    fn sweep(tzi: &TIME_ZONE_INFORMATION) -> usize {
        let zone = LocalZone::from_tzi(tzi);
        let (from, to) = (1_704_067_200i64 - 86_400, 1_830_297_600 + 86_400); // 2024-01-01 .. 2028-01-01 UTC
        let mut points: Vec<i64> = (from..to).step_by(420).collect();
        for year in 2024..=2027 {
            if let Rules::Annual(d, s) = &zone.rules {
                for (r, off) in [(d, zone.std_off), (s, zone.dst_off)] {
                    let at = transition_ms(year, r) / 1000 + off;
                    points.extend((at - 7200..=at + 7200).step_by(10));
                }
            }
        }
        let mut compared = 0;
        for secs in points {
            let slow = format_time_in(secs, Some(tzi));
            if !slow.is_empty() {
                assert_eq!(zone.format(secs).as_str(), slow, "secs {secs}");
                compared += 1;
            }
        }
        compared
    }

    #[test]
    fn local_zone_matches_win32_us_eastern() {
        assert!(sweep(&us_eastern()) > 300_000);
        let z = LocalZone::from_tzi(&us_eastern());
        assert_eq!(z.format(1_710_053_940), "2024-03-10 01:59"); // 06:59 UTC, last standard minute
        assert_eq!(z.format(1_710_054_000), "2024-03-10 03:00"); // 07:00 UTC, DST starts
    }

    #[test]
    fn local_zone_matches_win32_no_dst_and_southern() {
        assert!(sweep(&utc_minus_5()) > 300_000);
        assert!(sweep(&southern()) > 300_000);
    }

    #[test]
    fn local_zone_edges_do_not_panic() {
        let zones = [us_eastern(), southern(), utc_minus_5()];
        let current = current_tz();
        for tzi in zones.iter().chain([&current]) {
            let z = LocalZone::from_tzi(tzi);
            for secs in [i64::MIN, i64::MIN + 1, MIN_SECS - 1, MIN_SECS, MIN_SECS + 86_400, -1, 0, 1, MAX_SECS - 86_400, MAX_SECS, MAX_SECS + 1, i64::MAX] {
                assert_eq!(z.format(secs).as_str(), format_time_in(secs, Some(tzi)), "secs {secs}");
            }
        }
        assert_eq!(LocalZone::from_tzi(&utc_minus_5()).format(0), "1969-12-31 19:00");
        assert_eq!(LocalZone::from_tzi(&utc_minus_5()).format(1_790_000_000), "2026-09-21 09:13");
    }

    #[test]
    fn resolve_icons_requests_each_key_once_and_applies_replies() {
        use crate::icons::Outcome;
        let keyed = |name: &str, key: &str| Entry { icon_key: key.into(), ..e(name, 0, false) };
        let m = PaneModel::default();
        m.append(vec![keyed("a.txt", "ext:.txt"), keyed("b.txt", "ext:.txt"), keyed("c.png", "ext:.png"), keyed("d.txt", "ext:.txt")]);
        let mut asked = Vec::new();
        // Rows 0..=2 are on screen: two keys, each requested once.
        assert!(m.resolve_icons(0, 2, 16, |_, e| asked.push(e.icon_key.to_string())).is_empty());
        assert_eq!(asked, ["ext:.txt", "ext:.png"]);
        // Scrolling over a pending key asks nothing more.
        m.resolve_icons(0, 3, 16, |_, e| asked.push(e.icon_key.to_string()));
        assert_eq!(asked.len(), 2);
        m.icons.borrow_mut().complete("ext:.txt".into(), 16, Outcome::Pixels(slint::SharedPixelBuffer::new(1, 1)));
        // The reply lands: the visible .txt rows switch; row 3 is off screen and keeps its placeholder.
        assert_eq!(m.resolve_icons(0, 2, 16, |_, _| panic!("nothing new to request")), [0, 1]);
        assert_eq!(m.icon_indices(0, 99), [PLACEHOLDERS, PLACEHOLDERS, PLACEHOLDER_FILE, PLACEHOLDER_FILE]);
        assert_eq!(m.icon_indices(1, 2), [PLACEHOLDERS, PLACEHOLDER_FILE]);
        // A failed key is not asked for again and keeps its placeholder.
        m.icons.borrow_mut().complete("ext:.png".into(), 16, Outcome::Failed);
        assert_eq!(m.resolve_icons(0, 99, 16, |_, _| panic!("failed keys are not retried")), [3]);
        assert_eq!(m.icon_indices(2, 2), [PLACEHOLDER_FILE]);
        assert!(m.resolve_icons(5, 9, 16, |_, _| panic!("out of range")).is_empty());
        assert!(m.icon_indices(5, 9).is_empty());
    }

    #[test]
    fn selection_summary_counts_file_bytes_only() {
        let m = PaneModel::default();
        m.append(vec![e("dir", 4096, true), e("a", 1000, false), e("b", 536, false)]);
        assert_eq!(m.selection_summary(), (0, 0, false));
        m.select(SelOp::Only, 0);
        assert_eq!(m.selection_summary(), (1, 0, false), "a folder counts no bytes");
        m.select(SelOp::All, 0);
        assert_eq!(m.selection_summary(), (3, 1536, true));
    }

    #[test]
    fn append_then_replace() {
        let m = PaneModel::default();
        m.append(vec![e("a", 1, false), e("b", 2, false)]);
        assert_eq!(m.row_count(), 2);
        assert_eq!(m.row_data(1).unwrap().size.as_str(), "2 B");
        m.replace(PassOutput { shown: vec![e("c", 3, true)], hidden: 0, filtered: 0, cursor: None, selection: BTreeSet::new(), cut_marked: false });
        assert_eq!(m.row_count(), 1);
        assert_eq!(m.row_data(0).unwrap().kind, Kind::Directory as i32);
        assert_eq!(m.row_data(0).unwrap().size.as_str(), "");
    }

    #[test]
    fn cut_marks_follow_names_and_targets_fall_back_to_the_cursor() {
        let m = PaneModel::default();
        m.append(vec![e("a", 0, false), e("b", 0, false), e("c", 0, false)]);
        // Nothing selected: the cursor row is the target; no cursor, no target.
        assert_eq!(m.targets(2).iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["c"]);
        assert!(m.targets(-1).is_empty());
        m.select(SelOp::Only, 0);
        m.select(SelOp::Toggle, 1);
        assert_eq!(m.targets(2).iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["a", "b"], "selection wins");
        m.set_cut(vec!["b".into(), "c".into()]);
        let cut: Vec<bool> = (0..3).map(|i| m.row_data(i).unwrap().cut).collect();
        assert_eq!(cut, [false, true, true]);
        // A re-sort pass carries the marks by name.
        let out = filter_sort(m.snapshot_all(), false, &NameMatch::default(), SortCol::Name, false, &m.capture(0));
        m.replace(out);
        let cut: Vec<(String, bool)> = (0..3).map(|i| m.row_data(i).unwrap()).map(|r| (r.name.to_string(), r.cut)).collect();
        assert_eq!(cut, [("c".into(), true), ("b".into(), true), ("a".into(), false)]);
        // Clearing the cut while a pass is out: the swap re-derives the marks (none).
        let mut out = filter_sort(m.snapshot_all(), false, &NameMatch::default(), SortCol::Name, true, &m.capture(0));
        m.set_cut(vec![]);
        out.restore(&m.capture(0));
        m.replace(out);
        assert!((0..3).all(|i| !m.row_data(i).unwrap().cut));
        m.set_cut(vec!["a".into()]);
        m.clear();
        m.append(vec![e("a", 0, false)]);
        assert!(!m.row_data(0).unwrap().cut, "navigating away forgets the cut");
    }
}
