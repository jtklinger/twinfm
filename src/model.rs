//! PaneModel: a flat Vec of plain entries exposed to Slint as a virtualized model.
//! Only the UI thread touches it. Workers hand over whole Vec<Entry> batches.

use crate::{GridRow, Row, Tile};
use crate::icons::{IconTable, KeyState, PLACEHOLDER_DRIVE, PLACEHOLDER_FILE, PLACEHOLDER_FOLDER, PLACEHOLDERS};
use slint::{Model, ModelNotify, ModelRc, ModelTracker, SharedString, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::collections::{BTreeSet, HashMap, HashSet};
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
    /// What the list shows: `all` without the hidden entries `show` keeps out, sorted
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
    /// The large-icon grid over `shown` (P4-M1): `grid_n` entries to a grid row (set from the
    /// list's width), the cursor's entry index (for the tile's `current`), and the grid model's
    /// own notifications (`GridModel`), sent beside every `notify` the grid shows. With the pane in
    /// the details view nothing listens to `grid_notify`, so its calls cost a peer-list check.
    grid_n: Cell<usize>,
    grid_cursor: Cell<i32>,
    grid_notify: ModelNotify,
    /// Generation of the enumeration whose batches we accept; stale workers are ignored.
    pub generation: Cell<u64>,
    /// Bumped by every filter/sort request; only the newest pass may swap `shown`.
    pub pass: Cell<u64>,
    /// The pass that last landed (`set_landed_pass`); equal to `pass` once the newest one has.
    pub landed_pass: Cell<u64>,
    pub sort_col: Cell<SortCol>,
    pub sort_asc: Cell<bool>,
    pub show_hidden: Cell<bool>,
    /// The pane's protected-files setting (P6-M3; `show_protected_*`, the settings dialog): with
    /// `show_hidden` too, entries both hidden and system are shown.
    pub show_protected: Cell<bool>,
    /// The tab's name filter (Ctrl+F), applied by the same pass as `show_hidden`; empty: none.
    pub filter: RefCell<NameMatch>,
    /// The names are paths under a search's root (`sub\file.txt`): the rows show them file name
    /// first (`search_label`, P6-M3). The entries, the sort and every command keep the path.
    pub search_names: Cell<bool>,
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
            grid_n: Cell::new(1),
            grid_cursor: Cell::new(-1),
            grid_notify: ModelNotify::default(),
            generation: Cell::new(0),
            pass: Cell::new(0),
            landed_pass: Cell::new(0),
            sort_col: Cell::new(SortCol::Name),
            sort_asc: Cell::new(true),
            show_hidden: Cell::new(false),
            show_protected: Cell::new(false),
            filter: RefCell::new(NameMatch::default()),
            search_names: Cell::new(false),
        }
    }

    /// The tab's hidden-file switches, as the passes apply them.
    pub fn show(&self) -> Show {
        Show { hidden: self.show_hidden.get(), protected: self.show_protected.get() }
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

    /// Every entry back to its placeholder, its key kept, so the next resolve asks for its icon at
    /// the size the list now draws: a view switch changes the icon size, and an entry resolved at
    /// the old size kept that buffer, fitted to the new size (the first screenful of a details
    /// list showed its 16 px icons blown up to 96 px in the grid, P4-M1 to P6-M1). A key already
    /// held at the new size is taken on that resolve, with no request.
    pub fn forget_icons(&self) {
        let placeholder = |e: &Entry| match e.kind {
            Kind::Drive => PLACEHOLDER_DRIVE,
            _ if e.is_dir() => PLACEHOLDER_FOLDER,
            _ => PLACEHOLDER_FILE,
        };
        for e in self.all.borrow_mut().iter_mut() {
            e.icon = placeholder(e);
        }
        for e in self.shown.borrow_mut().iter_mut() {
            e.icon = placeholder(e);
        }
    }

    /// Give the entry keyed `from` (a cloud lookup's `look:` key, at `row` unless a pass moved it)
    /// the key `to`, in the listing and in its row; its icon stays the placeholder, so the next
    /// resolve asks for `to`. False when no entry has `from` any more (a refresh replaced it).
    pub fn relabel(&self, row: usize, from: &SharedString, to: SharedString) -> bool {
        if let Some(e) = self.all.borrow_mut().iter_mut().find(|e| &e.icon_key == from) {
            e.icon_key = to.clone();
        }
        let mut shown = self.shown.borrow_mut();
        let i = if shown.get(row).is_some_and(|e| &e.icon_key == from) { Some(row) } else { shown.iter().position(|e| &e.icon_key == from) };
        match i {
            Some(i) => {
                shown[i].icon_key = to;
                true
            }
            None => false,
        }
    }

    /// The icon table index of every row in `first..=last` (clamped to the list), in order.
    pub fn icon_indices(&self, first: usize, last: usize) -> Vec<u16> {
        let shown = self.shown.borrow();
        let end = (last + 1).min(shown.len());
        shown.get(first..end).map_or_else(Vec::new, |rows| rows.iter().map(|e| e.icon).collect())
    }

    /// The `cut` flag of every row in `first..=last` (clamped to the list), in order: which icons
    /// of the strip are composed dimmed.
    pub fn cut_flags(&self, first: usize, last: usize) -> Vec<bool> {
        let shown = self.shown.borrow();
        let end = (last + 1).min(shown.len());
        shown.get(first..end).map_or_else(Vec::new, |rows| rows.iter().map(|e| e.cut).collect())
    }

    pub fn clear(&self) {
        self.all.borrow_mut().clear();
        self.shown.borrow_mut().clear();
        self.hidden.set(0);
        self.filtered.set(0);
        self.incoming.borrow_mut().clear();
        self.refreshing.set(false);
        self.landed_pass.set(0);
        *self.sel.borrow_mut() = Selection::default();
        self.sel_version.set(self.sel_version.get() + 1);
        self.cut.borrow_mut().clear();
        self.notify.reset();
        self.grid_notify.reset();
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
            self.grid_changed(i);
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
        self.selection_changed(&changed);
    }

    /// Rows whose selection membership changed: mirror the set into the rows and notify them.
    fn selection_changed(&self, changed: &[usize]) {
        if changed.is_empty() {
            return;
        }
        {
            let sel = self.sel.borrow();
            let mut shown = self.shown.borrow_mut();
            for &i in changed {
                shown[i].selected = sel.set.contains(&i);
            }
        }
        for &i in changed {
            self.notify.row_changed(i);
            self.grid_changed(i);
        }
        self.sel_version.set(self.sel_version.get() + 1);
    }

    /// Replace the selection (the rubber band); only rows whose membership changed are notified.
    pub fn set_selection(&self, new: BTreeSet<usize>) {
        let len = self.shown.borrow().len();
        let changed = self.sel.borrow_mut().replace(new, len);
        self.selection_changed(&changed);
    }

    /// Anchor Shift's range at `row` (the cursor the band left, P6-M2).
    pub fn set_anchor(&self, row: usize) {
        self.sel.borrow_mut().anchor = Some(row);
    }

    /// The selected rows' indices.
    pub fn selected_set(&self) -> BTreeSet<usize> {
        self.sel.borrow().set.clone()
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
        let show_hidden = self.show();
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
            // The grid: the partial grid row the batch fills out changed, the rest are new.
            let n = self.grid_n.get();
            let (old, new) = (grid_rows(start, n), grid_rows(start + added, n));
            if start % n.max(1) != 0 {
                self.grid_notify.row_changed(old - 1);
            }
            if new > old {
                self.grid_notify.row_added(old, new - old);
            }
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
        self.grid_notify.reset();
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

    /// Entries to a grid row; a change re-maps every grid row (a reset). Returns whether it changed.
    pub fn set_grid_n(&self, n: usize) -> bool {
        let n = n.max(1);
        if self.grid_n.get() == n {
            return false;
        }
        self.grid_n.set(n);
        self.grid_notify.reset();
        true
    }

    pub fn grid_n(&self) -> usize {
        self.grid_n.get()
    }

    /// Grid rows shown.
    pub fn grid_len(&self) -> usize {
        grid_rows(self.len(), self.grid_n.get())
    }

    /// The cursor's entry index (-1: none); the grid rows holding the old and the new one are
    /// notified, nothing else.
    pub fn set_grid_cursor(&self, cursor: i32) {
        let old = self.grid_cursor.replace(cursor);
        if old == cursor {
            return;
        }
        for c in [old, cursor] {
            if let Ok(i) = usize::try_from(c) {
                self.grid_changed(i);
            }
        }
    }

    /// Entry `i` changed: notify its grid row, if it exists.
    fn grid_changed(&self, i: usize) {
        if i < self.len() {
            self.grid_notify.row_changed(grid_cell(i, self.grid_n.get()).0);
        }
    }

    pub fn entry(&self, i: usize) -> Option<Entry> {
        self.shown.borrow().get(i).cloned()
    }

    /// The pass number that last landed (`land_pass` calls this after `replace`).
    pub fn set_landed_pass(&self, pass: u64) {
        self.landed_pass.set(pass);
    }

    /// No refresh in flight, a pass has landed since the listing began (passes count from 1;
    /// `clear` resets `landed_pass` to 0, so a first enumeration's unsorted batches are not settled)
    /// and it is the newest requested one: `shown` is `all` filtered and sorted by the current
    /// settings, so `apply_edits` may edit it.
    pub fn settled(&self) -> bool {
        !self.refreshing.get() && self.landed_pass.get() != 0 && self.landed_pass.get() == self.pass.get()
    }

    /// Whether the listing has an entry of this name: exactly, or else ASCII-case-insensitively.
    #[cfg(test)]
    pub fn has_name(&self, name: &str) -> bool {
        let all = self.all.borrow();
        all.iter().any(|e| e.name == name) || all.iter().any(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// Apply a window of changes to the listing in place, keeping `shown` sorted by the current sort
    /// and the selection, cut marks and cursor on the rows they were on. `cursor` is the cursor's
    /// row before (or -1); the returned one is where that row is now (-1 when its row was removed
    /// and nothing follows it; the row after it when it was removed: as a delete's fallback cursor).
    /// All-or-nothing: Err(why) and no change when a `Remove` or `Rename.from` names an entry the
    /// listing does not have (the caller refreshes the folder instead), or the model is not
    /// `settled` (a refresh or a pass in flight, or no pass landed yet). A pass in flight would
    /// carry the listing from before the edits and undo them when it lands (`land_pass` drops a
    /// stale pass only when `total()` changed). The edits leave `pass` and `landed_pass` as they
    /// are, so the model stays settled. The caller must not request a pass between its `settled`
    /// check and this call (both run on the UI thread in one handler).
    ///
    /// Notifications, sent after every edit is applied, in order: `row_added(i, 1)`,
    /// `row_removed(i, 1)` or `row_changed(i)` per step on the details list (an entry that moves is
    /// a removal and an addition); on the grid one `grid_changed` per in-place change, or a single
    /// `reset` when any row was added or removed. Never a reset of the details list.
    pub fn apply_edits(&self, edits: Vec<Edit>, cursor: i32) -> Result<Applied, String> {
        if self.refreshing.get() {
            return Err("a refresh is in flight".into());
        }
        if self.landed_pass.get() != self.pass.get() {
            return Err("a pass is in flight".into());
        }
        if self.landed_pass.get() == 0 {
            return Err("no pass has landed since the listing began".into());
        }
        let mut all = self.all.borrow_mut();
        // Exact names to indices into `all`, kept up to date through the call.
        let mut idx: HashMap<SharedString, usize> = all.iter().enumerate().map(|(i, e)| (e.name.clone(), i)).collect();

        // Phase 1: resolve every name against the listing as the earlier edits leave it, without
        // touching the model. `overlay` (names removed or added so far) overlays `idx`.
        //
        // Names are matched exactly, except where the record's casing may differ from the listing's:
        // a `Remove` or a `Rename.from` that misses exactly falls back to an ASCII-case-insensitive
        // match through `lower` (ASCII lowercase to the names that ever had it), built on the first
        // such miss only (about 33 ms over 40k names). A rename's target is matched exactly, and
        // through `lower` only if that is built already (an overwrite under another casing; else
        // the overwritten entry stays until the next listing). An `Upsert` is
        // matched exactly, by contract: a record carries the on-disk casing, so an Upsert whose
        // name differs from an entry's only in case is a new entry (an insert); a case-only rename
        // arrives as a `Rename`.
        let mut plan: Vec<Step> = Vec::with_capacity(edits.len());
        {
            let mut overlay: HashMap<SharedString, bool> = HashMap::new();
            let mut lower: Option<HashMap<String, Vec<SharedString>>> = None;
            let present = |overlay: &HashMap<SharedString, bool>, n: &SharedString| overlay.get(n).copied().unwrap_or_else(|| idx.contains_key(n));
            let exact = |overlay: &HashMap<SharedString, bool>, n: &SharedString| present(overlay, n).then(|| n.clone());
            // Only through `lower` when an earlier miss built it: never a scan of every name.
            let scan = |overlay: &HashMap<SharedString, bool>, lower: &Option<HashMap<String, Vec<SharedString>>>, n: &SharedString| {
                lower.as_ref()?.get(&n.to_ascii_lowercase())?.iter().find(|k| present(overlay, k)).cloned()
            };
            let resolve = |overlay: &HashMap<SharedString, bool>, lower: &mut Option<HashMap<String, Vec<SharedString>>>, n: &SharedString| {
                if present(overlay, n) {
                    return Some(n.clone());
                }
                let map = lower.get_or_insert_with(|| {
                    let mut m: HashMap<String, Vec<SharedString>> = HashMap::new();
                    for k in idx.keys().chain(overlay.keys()) {
                        m.entry(k.to_ascii_lowercase()).or_default().push(k.clone());
                    }
                    m
                });
                map.get(&n.to_ascii_lowercase())?.iter().find(|k| present(overlay, k)).cloned()
            };
            let mark = |overlay: &mut HashMap<SharedString, bool>, lower: &mut Option<HashMap<String, Vec<SharedString>>>, n: &SharedString, on: bool| {
                overlay.insert(n.clone(), on);
                if let (true, Some(m)) = (on, lower.as_mut()) {
                    let v = m.entry(n.to_ascii_lowercase()).or_default();
                    if !v.contains(n) {
                        v.push(n.clone());
                    }
                }
            };
            for edit in edits {
                match edit {
                    Edit::Upsert(e) => match exact(&overlay, &e.name) {
                        Some(old) => {
                            mark(&mut overlay, &mut lower, &old, false);
                            mark(&mut overlay, &mut lower, &e.name, true);
                            plan.push(Step::Replace { old, entry: e, rename: false });
                        }
                        None => {
                            mark(&mut overlay, &mut lower, &e.name, true);
                            plan.push(Step::Insert(e));
                        }
                    },
                    Edit::Remove(n) => {
                        let old = resolve(&overlay, &mut lower, &n).ok_or_else(|| format!("remove of \"{n}\": not in the listing"))?;
                        mark(&mut overlay, &mut lower, &old, false);
                        plan.push(Step::Remove(old));
                    }
                    Edit::Rename { from, entry } => {
                        let old = resolve(&overlay, &mut lower, &from)
                            .ok_or_else(|| format!("rename of \"{from}\" to \"{}\": not in the listing", entry.name))?;
                        mark(&mut overlay, &mut lower, &old, false);
                        // Onto an existing name (overwriting it): that entry leaves first.
                        if let Some(target) = exact(&overlay, &entry.name).or_else(|| scan(&overlay, &lower, &entry.name)) {
                            mark(&mut overlay, &mut lower, &target, false);
                            plan.push(Step::Remove(target));
                        }
                        mark(&mut overlay, &mut lower, &entry.name, true);
                        plan.push(Step::Replace { old, entry, rename: true });
                    }
                }
            }
        }

        // Phase 2: apply the plan; nothing below can fail.
        let (col, asc, show_hidden) = (self.sort_col.get(), self.sort_asc.get(), self.show());
        let filter = self.filter.borrow();
        let mut shown = self.shown.borrow_mut();
        let mut sel = self.sel.borrow_mut();
        let sel_before = (sel.set.clone(), sel.anchor);
        let mut cur = usize::try_from(cursor).ok().filter(|&c| c < shown.len());
        let (mut hidden, mut filtered) = (self.hidden.get(), self.filtered.get());
        let mut notes: Vec<Note> = Vec::new();
        let mut gone_names: HashSet<SharedString> = HashSet::new();
        let mut out = Applied { added: 0, removed: 0, changed: 0, cursor: -1 };
        let cut_names: HashSet<SharedString> = self.cut.borrow().iter().cloned().collect();

        // Counts for an entry that is in `all` but not in `shown`, joining (+1) or leaving (-1).
        let mut count_out = |e: &Entry, join: bool| {
            let c = if shows(e, show_hidden) { &mut filtered } else { &mut hidden };
            *c = if join { *c + 1 } else { c.saturating_sub(1) };
        };
        let pos_of = |shown: &[Entry], e: &Entry| locate(shown, e, col, asc);
        let insert_at = |shown: &[Entry], e: &Entry| shown.partition_point(|x| cmp_entries(x, e, col, asc) != std::cmp::Ordering::Greater);

        for step in plan {
            match step {
                Step::Insert(mut e) => {
                    out.added += 1;
                    e.selected = false;
                    e.cut = false;
                    idx.insert(e.name.clone(), all.len());
                    all.push(e.clone());
                    if visible(&e, show_hidden, &filter) {
                        let q = insert_at(&shown, &e);
                        row_insert(&mut shown, &mut sel, &mut cur, &mut notes, q, e);
                    } else {
                        count_out(&e, true);
                    }
                }
                Step::Remove(name) => {
                    out.removed += 1;
                    let ai = idx.remove(&name).expect("resolved in phase 1");
                    let old = all.swap_remove(ai);
                    if ai < all.len() {
                        idx.insert(all[ai].name.clone(), ai);
                    }
                    gone_names.insert(old.name.clone());
                    let pos = if visible(&old, show_hidden, &filter) { pos_of(&shown, &old) } else { None };
                    match pos {
                        Some(p) => {
                            row_remove(&mut shown, &mut sel, &mut cur, &mut notes, p);
                        }
                        None => count_out(&old, false),
                    }
                }
                Step::Replace { old, mut entry, rename } => {
                    out.changed += 1;
                    let ai = idx.remove(&old).expect("resolved in phase 1");
                    let prev = all[ai].clone();
                    if rename {
                        gone_names.insert(prev.name.clone());
                    }
                    // What the fresh stat cannot know: a folder's computed size (and, below, the
                    // row's resolved icon).
                    // Not across a rename: the size was walked under the old name, and the caller
                    // drops it.
                    if !rename && entry.is_dir() && prev.is_dir() && !prev.size_text.is_empty() {
                        entry.size = prev.size;
                        entry.size_text = prev.size_text.clone();
                    }
                    entry.selected = false;
                    entry.cut = false;
                    idx.insert(entry.name.clone(), ai);
                    all[ai] = entry.clone();
                    let vis = visible(&entry, show_hidden, &filter);
                    let pos = if visible(&prev, show_hidden, &filter) { pos_of(&shown, &prev) } else { None };
                    match pos {
                        Some(p) => {
                            // The row keeps its icon (same key: `resolve_icons` writes icons into
                            // `shown` only) and its selection; its cut mark too unless renamed away.
                            if entry.icon_key == shown[p].icon_key {
                                entry.icon = shown[p].icon;
                            }
                            entry.selected = shown[p].selected;
                            entry.cut = if rename { cut_names.contains(&entry.name) } else { shown[p].cut };
                            if !vis {
                                row_remove(&mut shown, &mut sel, &mut cur, &mut notes, p);
                                count_out(&entry, true);
                            } else if fits(&shown, p, &entry, col, asc) {
                                shown[p] = entry;
                                notes.push(Note::Changed(p));
                            } else {
                                let (was_sel, was_cur, was_anchor) = (sel.set.contains(&p), cur == Some(p), sel.anchor == Some(p));
                                row_remove(&mut shown, &mut sel, &mut cur, &mut notes, p);
                                let q = insert_at(&shown, &entry);
                                row_insert(&mut shown, &mut sel, &mut cur, &mut notes, q, entry);
                                if was_sel {
                                    sel.set.insert(q);
                                }
                                if was_cur {
                                    cur = Some(q);
                                }
                                if was_anchor {
                                    sel.anchor = Some(q);
                                }
                            }
                        }
                        None => {
                            count_out(&prev, false);
                            if vis {
                                entry.cut = !rename && cut_names.contains(&prev.name);
                                let q = insert_at(&shown, &entry);
                                row_insert(&mut shown, &mut sel, &mut cur, &mut notes, q, entry);
                            } else {
                                count_out(&entry, true);
                            }
                        }
                    }
                }
            }
        }
        drop(filter);
        self.hidden.set(hidden);
        self.filtered.set(filtered);
        if !gone_names.is_empty() {
            // Names that left (removed, renamed away) leave the cut.
            self.cut.borrow_mut().retain(|n| !gone_names.contains(n));
        }
        let sel_changed = (&sel.set, sel.anchor) != (&sel_before.0, sel_before.1);
        if let Some(c) = cur {
            if c >= shown.len() {
                cur = shown.len().checked_sub(1);
            }
        }
        out.cursor = cur.map_or(-1, |c| c as i32);
        drop((all, shown, sel));
        if sel_changed {
            self.sel_version.set(self.sel_version.get() + 1);
        }
        let structural = notes.iter().any(|n| !matches!(n, Note::Changed(_)));
        for n in &notes {
            match *n {
                Note::Added(i) => self.notify.row_added(i, 1),
                Note::Removed(i) => self.notify.row_removed(i, 1),
                Note::Changed(i) => {
                    self.notify.row_changed(i);
                    if !structural {
                        self.grid_changed(i);
                    }
                }
            }
        }
        if structural {
            self.grid_notify.reset();
        }
        Ok(out)
    }
}

/// One change of a watch window, for `PaneModel::apply_edits`.
#[derive(Clone, Debug)]
pub enum Edit {
    /// An entry added or modified: its fresh stat. A known name is replaced in place, an unknown
    /// one inserted at its sorted position.
    Upsert(Entry),
    /// An entry removed.
    Remove(SharedString),
    /// An entry renamed: `from` leaves, `entry` (its fresh stat under the new name) arrives.
    Rename { from: SharedString, entry: Entry },
}

/// What `apply_edits` did: entries of the listing added, removed and changed (a rename counts as
/// changed; one onto an existing name also as removed), and the cursor's row now (-1: none).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Applied {
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
    pub cursor: i32,
}

/// An edit resolved against the listing (phase 1 of `apply_edits`): names are the stored ones.
enum Step {
    Insert(Entry),
    Remove(SharedString),
    Replace { old: SharedString, entry: Entry, rename: bool },
}

/// A row notification `apply_edits` sends once its borrows are released.
enum Note {
    Added(usize),
    Removed(usize),
    Changed(usize),
}

/// Where `e` (as stored, so with the sort key its row has) sits in `shown`: by binary search on
/// the sort, then among the equal ones by name; a linear scan if the list is not in that order
/// (a folder size that landed after the pass). None when it is not shown.
fn locate(shown: &[Entry], e: &Entry, col: SortCol, asc: bool) -> Option<usize> {
    let lo = shown.partition_point(|x| cmp_entries(x, e, col, asc) == std::cmp::Ordering::Less);
    shown[lo..]
        .iter()
        .take_while(|x| cmp_entries(x, e, col, asc) == std::cmp::Ordering::Equal)
        .position(|x| x.name == e.name)
        .map(|k| lo + k)
        .or_else(|| shown.iter().position(|x| x.name == e.name))
}

/// Whether `e` can take row `p`'s place without breaking the order.
fn fits(shown: &[Entry], p: usize, e: &Entry, col: SortCol, asc: bool) -> bool {
    use std::cmp::Ordering::Greater;
    (p == 0 || cmp_entries(&shown[p - 1], e, col, asc) != Greater) && shown.get(p + 1).is_none_or(|n| cmp_entries(e, n, col, asc) != Greater)
}

/// Insert a row at `q`, shifting the selection, anchor and cursor at or after it.
fn row_insert(shown: &mut Vec<Entry>, sel: &mut Selection, cur: &mut Option<usize>, notes: &mut Vec<Note>, q: usize, e: Entry) {
    shown.insert(q, e);
    if sel.set.range(q..).next().is_some() {
        let hi = sel.set.split_off(&q);
        sel.set.extend(hi.into_iter().map(|j| j + 1));
    }
    if let Some(a) = sel.anchor.filter(|&a| a >= q) {
        sel.anchor = Some(a + 1);
    }
    if let Some(c) = cur.filter(|&c| c >= q) {
        *cur = Some(c + 1);
    }
    notes.push(Note::Added(q));
}

/// Remove row `p`: it leaves the selection, later indices shift down, the anchor on it is
/// dropped, and a cursor on it falls to the row that followed it (the same index).
fn row_remove(shown: &mut Vec<Entry>, sel: &mut Selection, cur: &mut Option<usize>, notes: &mut Vec<Note>, p: usize) {
    shown.remove(p);
    if sel.set.range(p..).next().is_some() {
        let mut hi = sel.set.split_off(&p);
        hi.remove(&p);
        sel.set.extend(hi.into_iter().map(|j| j - 1));
    }
    sel.anchor = match sel.anchor {
        Some(a) if a == p => None,
        Some(a) if a > p => Some(a - 1),
        a => a,
    };
    if let Some(c) = *cur {
        if c > p {
            *cur = Some(c - 1);
        } else if c == p {
            *cur = if p < shown.len() { Some(p) } else { shown.len().checked_sub(1) };
        }
    }
    notes.push(Note::Removed(p));
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
            name: if self.search_names.get() { search_label(&e.name) } else { e.name.clone() },
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

/// A search result's name as its row shows it (P6-M3): the file's own name first, then the folder
/// it is in under the search's root, so the name is never the part that is cut short
/// (`sub\dir\file.txt` reads `file.txt  (sub\dir)`). A result in the root itself is its name.
pub fn search_label(name: &str) -> SharedString {
    match name.rsplit_once('\\') {
        Some((dir, leaf)) => format!("{leaf}  ({dir})").into(),
        None => name.into(),
    }
}

/// Grid rows for `len` entries at `n` to a row (n = 0 reads as 1).
pub fn grid_rows(len: usize, n: usize) -> usize {
    len.div_ceil(n.max(1))
}

/// (grid row, column) of entry `i` at `n` to a row (n = 0 reads as 1).
pub fn grid_cell(i: usize, n: usize) -> (usize, usize) {
    let n = n.max(1);
    (i / n, i % n)
}

/// The entries of grid row `r` at `n` to a row, for a list of `len` (empty past the end).
pub fn grid_span(r: usize, n: usize, len: usize) -> std::ops::Range<usize> {
    let n = n.max(1);
    (r * n).min(len)..((r + 1) * n).min(len)
}

/// The pane's entries as grid rows of `grid_n` tiles (the large-icon view's ListView model).
/// Notifications come from the PaneModel (`grid_notify`), which knows when entries change.
pub struct GridModel(pub Rc<PaneModel>);

impl Model for GridModel {
    type Data = GridRow;

    fn row_count(&self) -> usize {
        self.0.grid_len()
    }

    fn row_data(&self, row: usize) -> Option<GridRow> {
        let m = &self.0;
        let shown = m.shown.borrow();
        let span = grid_span(row, m.grid_n.get(), shown.len());
        if span.is_empty() {
            return None;
        }
        let cursor = m.grid_cursor.get();
        let first = span.start;
        let tiles: Vec<Tile> = shown[span]
            .iter()
            .enumerate()
            .map(|(k, e)| Tile { name: if m.search_names.get() { search_label(&e.name) } else { e.name.clone() }, selected: e.selected, cut: e.cut, current: (first + k) as i32 == cursor })
            .collect();
        Some(GridRow { first: first as i32, tiles: ModelRc::new(VecModel::from(tiles)) })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.0.grid_notify
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// The hidden-file switches (P6-M3, Explorer's two): `hidden` (Ctrl+H, per tab) shows entries
/// with the hidden attribute; entries both hidden and system ("protected operating system files")
/// need `protected` (a per-pane setting) as well. An entry with the system attribute alone is
/// always shown, as Explorer shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Show {
    pub hidden: bool,
    pub protected: bool,
}

#[cfg(test)]
impl Show {
    /// Neither switch: what a new pane shows by default.
    pub const OFF: Show = Show { hidden: false, protected: false };
    /// Ctrl+H on, protected files still kept out.
    pub const HIDDEN: Show = Show { hidden: true, protected: false };
}

/// Whether the hidden-file switches and the name filter let an entry through.
pub fn visible(e: &Entry, show: Show, filter: &NameMatch) -> bool {
    shows(e, show) && filter.matches(&e.name)
}

/// Whether the hidden-file switches let an entry through.
pub fn shows(e: &Entry, show: Show) -> bool {
    match (e.is_hidden(), e.is_system()) {
        (false, _) => true,
        (true, false) => show.hidden,
        (true, true) => show.hidden && show.protected,
    }
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

/// The worker pass: filter a snapshot of `all` (hidden-file switches, then name filter), sort it,
/// and carry the cursor and selection over by name. Pure, runs off the UI thread.
pub fn filter_sort(mut all: Vec<Entry>, show_hidden: Show, filter: &NameMatch, col: SortCol, asc: bool, r: &Restore) -> PassOutput {
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

impl Selection {
    /// Replace the selection with `new` (rows past `len` dropped), keeping the anchor; returns
    /// the rows whose membership changed. The rubber band's bulk update.
    pub fn replace(&mut self, new: BTreeSet<usize>, len: usize) -> Vec<usize> {
        let new: BTreeSet<usize> = new.into_iter().filter(|&i| i < len).collect();
        let changed = self.set.symmetric_difference(&new).copied().collect();
        self.set = new;
        changed
    }
}

/// Sort in place: directories first, then by the chosen column. Runs on a worker.
pub fn sort_entries(entries: &mut [Entry], col: SortCol, asc: bool) {
    entries.sort_unstable_by(|a, b| cmp_entries(a, b, col, asc));
}

/// The sort's order: directories first, then the chosen column with the name as the tie-break,
/// reversed (directories still first) when descending. `sort_entries` and `apply_edits` share it.
pub fn cmp_entries(a: &Entry, b: &Entry, col: SortCol, asc: bool) -> std::cmp::Ordering {
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
    fn filter_hides_hidden_and_protected_unless_shown() {
        let a = |name: &str, attrs: u32| Entry::new(name.into(), 0, 0, attrs, "".into());
        let all = vec![
            a("b", 0),
            a("hid", FILE_ATTRIBUTE_HIDDEN),
            a("sys", FILE_ATTRIBUTE_SYSTEM),
            a("a", 0),
            a("both", FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM),
        ];
        let names = |o: &PassOutput| o.shown.iter().map(|x| x.name.to_string()).collect::<Vec<_>>();
        // A system file alone is always shown (Explorer); hidden ones need Ctrl+H, hidden and
        // system ones the protected-files setting as well (P6-M3).
        let off = filter_sort(all.clone(), Show::OFF, &NameMatch::default(), SortCol::Name, true, &Restore::default());
        assert_eq!(names(&off), ["a", "b", "sys"]);
        assert_eq!(off.hidden, 2);
        let on = filter_sort(all.clone(), Show::HIDDEN, &NameMatch::default(), SortCol::Name, true, &Restore::default());
        assert_eq!(names(&on), ["a", "b", "hid", "sys"]);
        assert_eq!(on.hidden, 1);
        let protected_only = Show { hidden: false, protected: true };
        let p = filter_sort(all.clone(), protected_only, &NameMatch::default(), SortCol::Name, true, &Restore::default());
        assert_eq!(names(&p), names(&off), "protected files need Ctrl+H too");
        let both = Show { hidden: true, protected: true };
        let all_on = filter_sort(all.clone(), both, &NameMatch::default(), SortCol::Name, true, &Restore::default());
        assert_eq!((all_on.shown.len(), all_on.hidden), (5, 0));

        // Batches during enumeration: `all` keeps everything, `shown` only what passes.
        let m = PaneModel::default();
        m.append(all[..3].to_vec());
        m.append(all[3..].to_vec());
        assert_eq!((m.total(), m.row_count(), m.hidden_count()), (5, 3, 2));
        assert_eq!(m.row_data(2).unwrap().name.as_str(), "a");
        m.show_hidden.set(true);
        m.show_protected.set(true);
        assert_eq!(m.show(), both);
        m.replace(filter_sort(m.snapshot_all(), m.show(), &NameMatch::default(), SortCol::Name, true, &Restore::default()));
        assert_eq!((m.row_count(), m.hidden_count()), (5, 0));
    }

    #[test]
    fn search_rows_show_the_file_name_first() {
        assert_eq!(search_label(r"Windows\WinSxS\amd64_x\kernel32.dll").as_str(), r"kernel32.dll  (Windows\WinSxS\amd64_x)");
        assert_eq!(search_label("top.txt").as_str(), "top.txt", "a result in the root is its name");
        let m = PaneModel::default();
        m.append(vec![Entry::new(r"sub\a.txt".into(), 0, 0, 0, "".into())]);
        assert_eq!(m.row_data(0).unwrap().name.as_str(), r"sub\a.txt", "a folder tab shows names as they are");
        m.search_names.set(true);
        assert_eq!(m.row_data(0).unwrap().name.as_str(), "a.txt  (sub)");
        assert_eq!(m.entry(0).unwrap().name.as_str(), r"sub\a.txt", "the entry keeps its path");
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
        let all = vec![a("one.txt", 0), a("two.log", 0), a("three.txt", FILE_ATTRIBUTE_HIDDEN), a("four.TXT", 0), a("sys.txt", FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM)];
        let txt = NameMatch::new("*.txt");
        assert!(visible(&all[0], Show::OFF, &txt) && !visible(&all[1], Show::OFF, &txt) && !visible(&all[2], Show::OFF, &txt));
        assert!(visible(&all[2], Show::HIDDEN, &txt), "the filter never reveals a hidden file, the switch does");
        let off = filter_sort(all.clone(), Show::OFF, &txt, SortCol::Name, true, &Restore::default());
        let names: Vec<&str> = off.shown.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["four.TXT", "one.txt"]);
        assert_eq!((off.hidden, off.filtered), (2, 1));
        let on = filter_sort(all.clone(), Show { hidden: true, protected: true }, &txt, SortCol::Name, true, &Restore::default());
        assert_eq!((on.shown.len(), on.hidden, on.filtered), (4, 0, 1));

        // Batches during enumeration count the two apart as the pass does.
        let m = PaneModel::default();
        *m.filter.borrow_mut() = txt.clone();
        m.append(all[..2].to_vec());
        m.append(all[2..].to_vec());
        assert_eq!((m.total(), m.row_count(), m.hidden_count(), m.filtered_count()), (5, 2, 2, 1));
        // Cleared: the next pass shows every entry the hidden switch lets through.
        m.replace(filter_sort(m.snapshot_all(), Show::OFF, &NameMatch::default(), SortCol::Name, true, &Restore::default()));
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
    fn selection_replace_reports_the_rows_that_changed() {
        let mut s = Selection::default();
        s.apply(SelOp::Range, 2, 10);
        s.apply(SelOp::Range, 4, 10); // 2..=4
        let mut changed = s.replace([3, 4, 5, 12].into_iter().collect(), 10);
        changed.sort_unstable();
        assert_eq!(changed, vec![2, 5], "2 leaves, 5 joins, 3 and 4 stay, 12 is past the end");
        assert_eq!(s.set.iter().copied().collect::<Vec<_>>(), vec![3, 4, 5]);
        assert_eq!(s.anchor, Some(2), "the anchor is not the band's to move");
        assert!(s.replace([3, 4, 5].into_iter().collect(), 10).is_empty(), "nothing changed");
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
        let out = filter_sort(m.snapshot_all(), Show::OFF, &NameMatch::default(), SortCol::Name, false, &r);
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
    fn grid_maps_entries_to_rows_and_columns() {
        assert_eq!(grid_rows(0, 7), 0);
        assert_eq!(grid_rows(14, 7), 2);
        assert_eq!(grid_rows(15, 7), 3, "a last partial row");
        assert_eq!(grid_rows(5, 1), 5);
        assert_eq!(grid_rows(5, 0), 5, "n = 0 (no width yet) reads as one column");
        assert_eq!(grid_cell(0, 7), (0, 0));
        assert_eq!(grid_cell(6, 7), (0, 6));
        assert_eq!(grid_cell(7, 7), (1, 0));
        assert_eq!(grid_cell(15, 7), (2, 1));
        assert_eq!(grid_cell(3, 1), (3, 0));
        assert_eq!(grid_cell(3, 0), (3, 0));
        assert_eq!(grid_span(0, 7, 15), 0..7);
        assert_eq!(grid_span(2, 7, 15), 14..15, "the last partial row");
        assert_eq!(grid_span(3, 7, 15), 15..15, "past the end");
        assert_eq!(grid_span(4, 1, 15), 4..5);
        assert_eq!(grid_span(4, 0, 15), 4..5);
        // Round trip: every entry lies in the span of its own grid row, at its column.
        for n in [1, 2, 7] {
            for i in 0..30 {
                let (r, c) = grid_cell(i, n);
                assert_eq!(grid_span(r, n, 30).start + c, i);
            }
        }
        // The adapter over a pane model: rows, tiles, the cursor, and a width change.
        let m = Rc::new(PaneModel::default());
        m.append((0..15).map(|i| Entry::new(format!("f{i:02}").into(), 1, 0, 0, "File".into())).collect());
        assert!(m.set_grid_n(7));
        assert!(!m.set_grid_n(7));
        m.set_grid_cursor(8);
        let g = GridModel(m.clone());
        assert_eq!(g.row_count(), 3);
        let r1 = g.row_data(1).unwrap();
        assert_eq!(r1.first, 7);
        assert_eq!(r1.tiles.row_count(), 7);
        assert_eq!(r1.tiles.row_data(0).unwrap().name, "f07");
        assert!(r1.tiles.row_data(1).unwrap().current, "entry 8 is row 1, column 1");
        assert!(!r1.tiles.row_data(0).unwrap().current);
        assert_eq!(g.row_data(2).unwrap().tiles.row_count(), 1);
        assert!(g.row_data(3).is_none());
        m.select(SelOp::Only, 14);
        assert!(g.row_data(2).unwrap().tiles.row_data(0).unwrap().selected);
        m.set_grid_n(4);
        assert_eq!(g.row_count(), 4);
        assert_eq!(g.row_data(3).unwrap().first, 12);
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
        let cursor = m.replace(filter_sort(m.snapshot_all(), Show::OFF, &NameMatch::default(), SortCol::Name, true, &r));
        assert_eq!(cursor, Some(1)); // new order "0", "b", "c"
        assert_eq!(m.selected_names(), ["b"]);
        assert_eq!(m.row_count(), 3);
    }

    /// Records the notifications a model sends, as the views' peers receive them.
    #[derive(Default)]
    struct Rec(RefCell<Vec<String>>);

    impl slint::private_unstable_api::re_exports::ModelChangeListener for Rec {
        fn row_changed(self: std::pin::Pin<&Self>, row: usize) {
            self.0.borrow_mut().push(format!("chg {row}"));
        }
        fn row_added(self: std::pin::Pin<&Self>, index: usize, count: usize) {
            self.0.borrow_mut().push(format!("add {index}{}", if count == 1 { String::new() } else { format!("+{count}") }));
        }
        fn row_removed(self: std::pin::Pin<&Self>, index: usize, count: usize) {
            self.0.borrow_mut().push(format!("rem {index}{}", if count == 1 { String::new() } else { format!("+{count}") }));
        }
        fn reset(self: std::pin::Pin<&Self>) {
            self.0.borrow_mut().push("reset".into());
        }
    }

    type Peer = std::pin::Pin<Box<slint::private_unstable_api::re_exports::ModelChangeListenerContainer<Rec>>>;

    /// A model of the ten files a1 to a10 (sizes 10 to 100) listed in reverse, sorted by `col` and
    /// `asc` through a landed pass, with a recorder on the details list and one on the grid (one
    /// tile to a grid row, so a grid row is an entry).
    fn edited(col: SortCol, asc: bool, filter: &str) -> (Rc<PaneModel>, Peer, Peer) {
        use slint::private_unstable_api::re_exports::ModelChangeListenerContainer;
        let m = Rc::new(PaneModel::default());
        *m.filter.borrow_mut() = NameMatch::new(filter);
        m.append((1..=10).rev().map(|i| e(&format!("a{i}"), i * 10, false)).collect());
        m.sort_col.set(col);
        m.sort_asc.set(asc);
        m.pass.set(1);
        m.replace(filter_sort(m.snapshot_all(), Show::OFF, &m.filter.borrow().clone(), col, asc, &Restore::default()));
        m.set_landed_pass(1);
        assert!(m.settled());
        let rows: Peer = Box::pin(ModelChangeListenerContainer::new(Rec::default()));
        let grid: Peer = Box::pin(ModelChangeListenerContainer::new(Rec::default()));
        m.model_tracker().attach_peer(rows.as_ref().model_peer());
        GridModel(m.clone()).model_tracker().attach_peer(grid.as_ref().model_peer());
        (m, rows, grid)
    }

    fn rows(m: &PaneModel) -> Vec<String> {
        (0..m.row_count()).map(|i| m.row_data(i).unwrap().name.to_string()).collect()
    }

    fn log(p: &Peer) -> Vec<String> {
        std::mem::take(&mut *p.0.borrow_mut())
    }

    #[test]
    fn apply_edits_inserts_at_the_sorted_position() {
        let (m, r, g) = edited(SortCol::Name, true, "");
        // "a01" compares equal to "a1" (natural order): it goes after it.
        let a = m.apply_edits(vec![Edit::Upsert(e("a01", 5, false))], -1).unwrap();
        assert_eq!(a, Applied { added: 1, removed: 0, changed: 0, cursor: -1 });
        assert_eq!(log(&r), ["add 1"]);
        assert_eq!(log(&g), ["reset"]);
        m.apply_edits(vec![Edit::Upsert(e("zdir", 0, true)), Edit::Upsert(e("a5x", 1, false))], -1).unwrap();
        assert_eq!(log(&r), ["add 0", "add 7"], "the folder before every file; a5x after a5");
        assert_eq!(rows(&m), ["zdir", "a1", "a01", "a2", "a3", "a4", "a5", "a5x", "a6", "a7", "a8", "a9", "a10"]);
        assert_eq!((m.total(), m.len()), (13, 13));

        let (m, r, _g) = edited(SortCol::Name, false, "");
        m.apply_edits(vec![Edit::Upsert(e("a01", 5, false)), Edit::Upsert(e("zdir", 0, true)), Edit::Upsert(e("a5x", 1, false))], -1).unwrap();
        assert_eq!(log(&r), ["add 10", "add 0", "add 6"]);
        assert_eq!(rows(&m), ["zdir", "a10", "a9", "a8", "a7", "a6", "a5x", "a5", "a4", "a3", "a2", "a1", "a01"]);
        assert!(m.snapshot_all().iter().any(|x| x.name == "a01"), "the listing has it too");
    }

    #[test]
    fn apply_edits_replaces_in_place_and_moves_when_the_key_changes() {
        // Size sort: a3 grows from 30 to 95 and moves between a9 (90) and a10 (100).
        let (m, r, g) = edited(SortCol::Size, true, "");
        let a = m.apply_edits(vec![Edit::Upsert(e("a3", 95, false))], -1).unwrap();
        assert_eq!(a, Applied { added: 0, removed: 0, changed: 1, cursor: -1 });
        assert_eq!(log(&r), ["rem 2", "add 8"]);
        assert_eq!(log(&g), ["reset"]);
        assert_eq!(rows(&m), ["a1", "a2", "a4", "a5", "a6", "a7", "a8", "a9", "a3", "a10"]);
        assert_eq!(m.row_data(8).unwrap().size, "95 B");
        // A change that keeps its place: one row_changed, and the grid row alone.
        m.apply_edits(vec![Edit::Upsert(e("a3", 96, false))], -1).unwrap();
        assert_eq!((log(&r), log(&g)), (vec!["chg 8".to_string()], vec!["chg 8".to_string()]));

        // Name sort: a size change never moves a row.
        let (m, r, g) = edited(SortCol::Name, true, "");
        m.apply_edits(vec![Edit::Upsert(e("a3", 95, false))], -1).unwrap();
        assert_eq!((log(&r), log(&g)), (vec!["chg 2".to_string()], vec!["chg 2".to_string()]));
        assert_eq!(m.row_data(2).unwrap().size, "95 B");
        assert_eq!(m.snapshot_all().iter().find(|x| x.name == "a3").unwrap().size, 95);
        assert_eq!((m.total(), m.len()), (10, 10));
    }

    #[test]
    fn apply_edits_keeps_selection_cut_and_cursor() {
        let (m, r, _g) = edited(SortCol::Name, true, "");
        m.select(SelOp::Only, 2); // a3
        m.select(SelOp::Toggle, 5); // a6
        m.set_cut(vec!["a8".into()]); // row 7
        log(&r);
        let v = m.sel_version.get();
        // An insert before them, then a3 removed; the cursor is on a6 (row 5).
        let a = m.apply_edits(vec![Edit::Upsert(e("a01", 1, false)), Edit::Remove("a3".into())], 5).unwrap();
        assert_eq!(a, Applied { added: 1, removed: 1, changed: 0, cursor: 5 });
        assert_eq!(log(&r), ["add 1", "rem 3"]);
        assert_eq!(rows(&m), ["a1", "a01", "a2", "a4", "a5", "a6", "a7", "a8", "a9", "a10"]);
        assert_eq!(m.selected_names(), ["a6"], "the selection follows its rows; the removed one leaves");
        assert_eq!(m.selected_set(), set(&[5]));
        assert!(m.row_data(5).unwrap().selected && !m.row_data(2).unwrap().selected);
        assert!(m.sel_version.get() > v, "a selected row left");
        assert!(m.row_data(7).unwrap().cut, "a8 keeps its cut mark");
        // A replace keeps the cut mark and the selection.
        m.select(SelOp::Toggle, 7);
        m.apply_edits(vec![Edit::Upsert(e("a8", 1234, false))], 5).unwrap();
        let a8 = m.row_data(7).unwrap();
        assert!(a8.cut && a8.selected && a8.size == "1.2 KB");
        assert_eq!(m.selected_names(), ["a6", "a8"]);
        // The cursor's row removed: the row that followed it (a7) is the cursor, at the same index.
        let a = m.apply_edits(vec![Edit::Remove("a6".into())], 5).unwrap();
        assert_eq!((a.cursor, m.row_data(5).unwrap().name.as_str()), (5, "a7"));
        assert_eq!(m.selected_names(), ["a8"]);
        // The last row removed under the cursor: the new last row; a cut name removed leaves the cut.
        let a = m.apply_edits(vec![Edit::Remove("a10".into()), Edit::Remove("a8".into())], 8).unwrap();
        assert_eq!(rows(&m), ["a1", "a01", "a2", "a4", "a5", "a7", "a9"]);
        assert_eq!(a.cursor, 6);
        assert!(m.selected_names().is_empty());
        assert!(m.capture(-1).cut.is_empty(), "a8 left the cut");
        // A cursor below an insert moves down with its row; everything removed: no cursor.
        assert_eq!(m.apply_edits(vec![Edit::Upsert(e("a0", 1, false))], 3).unwrap().cursor, 4);
        let all: Vec<Edit> = rows(&m).into_iter().map(|n| Edit::Remove(n.into())).collect();
        assert_eq!(m.apply_edits(all, 4).unwrap().cursor, -1);
        assert_eq!((m.len(), m.total()), (0, 0));
    }

    #[test]
    fn apply_edits_counts_hidden_and_filtered() {
        let consistent = |m: &PaneModel| assert_eq!(m.len() + m.hidden_count() + m.filtered_count(), m.total());
        let (m, r, _g) = edited(SortCol::Name, true, "");
        let hidden = |name: &str| Entry::new(name.into(), 30, 0, FILE_ATTRIBUTE_HIDDEN, SharedString::default());
        m.apply_edits(vec![Edit::Upsert(hidden("a3"))], -1).unwrap();
        assert_eq!(log(&r), ["rem 2"], "now hidden: it leaves the list");
        assert_eq!((m.len(), m.total(), m.hidden_count(), m.filtered_count()), (9, 10, 1, 0));
        consistent(&m);
        m.apply_edits(vec![Edit::Upsert(e("a3", 30, false))], -1).unwrap();
        assert_eq!(log(&r), ["add 2"], "shown again");
        assert_eq!((m.len(), m.hidden_count()), (10, 0));
        // A hidden newcomer and the removal of a hidden entry only count.
        m.apply_edits(vec![Edit::Upsert(hidden("h"))], -1).unwrap();
        assert_eq!((m.len(), m.total(), m.hidden_count()), (10, 11, 1));
        m.apply_edits(vec![Edit::Remove("h".into())], -1).unwrap();
        assert_eq!((m.len(), m.total(), m.hidden_count()), (10, 10, 0));
        assert!(log(&r).is_empty());
        consistent(&m);

        // A filter on: "a1*" shows a1 and a10.
        let (m, r, _g) = edited(SortCol::Name, true, "a1*");
        assert_eq!((m.len(), m.filtered_count()), (2, 8));
        m.apply_edits(vec![Edit::Upsert(e("zzz", 1, false)), Edit::Upsert(e("a11", 1, false))], -1).unwrap();
        assert_eq!(log(&r), ["add 2"], "a11 matches, zzz does not");
        assert_eq!((m.len(), m.total(), m.filtered_count()), (3, 12, 9));
        consistent(&m);
        // A rename out of the filter and one into it.
        m.apply_edits(vec![Edit::Rename { from: "a10".into(), entry: e("b10", 100, false) }, Edit::Rename { from: "a5".into(), entry: e("a15", 50, false) }], -1).unwrap();
        assert_eq!(log(&r), ["rem 1", "add 2"]);
        assert_eq!(rows(&m), ["a1", "a11", "a15"]);
        assert_eq!((m.len(), m.total(), m.filtered_count()), (3, 12, 9));
        m.apply_edits(vec![Edit::Remove("zzz".into()), Edit::Remove("a11".into())], -1).unwrap();
        assert_eq!((m.len(), m.total(), m.filtered_count()), (2, 10, 8));
        consistent(&m);
    }

    #[test]
    fn apply_edits_refuses_unknown_names_without_changing_anything() {
        let (m, r, g) = edited(SortCol::Name, true, "");
        m.select(SelOp::Only, 4);
        log(&r);
        log(&g);
        let (before, v) = (rows(&m), m.sel_version.get());
        let err = m.apply_edits(vec![Edit::Upsert(e("new", 1, false)), Edit::Remove("a2".into()), Edit::Remove("stranger".into())], 4);
        assert!(err.unwrap_err().contains("stranger"));
        let err = m.apply_edits(vec![Edit::Remove("a2".into()), Edit::Remove("a2".into())], 4);
        assert!(err.is_err(), "removed twice: the second names an entry that is gone");
        let err = m.apply_edits(vec![Edit::Rename { from: "ghost".into(), entry: e("x", 1, false) }], 4);
        assert!(err.is_err());
        assert_eq!((rows(&m), m.total(), m.selected_names(), m.sel_version.get()), (before, 10, vec![SharedString::from("a5")], v));
        assert!(log(&r).is_empty() && log(&g).is_empty(), "no notification");
        // Within one window a name added earlier can be removed later.
        assert!(m.apply_edits(vec![Edit::Upsert(e("tmp", 1, false)), Edit::Remove("tmp".into())], 4).is_ok());
        assert_eq!(log(&r), ["add 10", "rem 10"]);
        // No edits while a refresh is in flight.
        m.begin_refresh();
        assert!(m.apply_edits(vec![Edit::Upsert(e("x", 1, false))], 4).is_err());
    }

    #[test]
    fn apply_edits_renames() {
        let (m, r, _g) = edited(SortCol::Name, true, "");
        m.select(SelOp::Only, 2); // a3
        log(&r);
        let a = m.apply_edits(vec![Edit::Rename { from: "a3".into(), entry: e("z9", 30, false) }], 2).unwrap();
        assert_eq!(a, Applied { added: 0, removed: 0, changed: 1, cursor: 9 }, "the cursor follows the row");
        assert_eq!(log(&r), ["rem 2", "add 9"]);
        assert_eq!(rows(&m), ["a1", "a2", "a4", "a5", "a6", "a7", "a8", "a9", "a10", "z9"]);
        assert_eq!(m.selected_names(), ["z9"]);
        // A case-only rename stays in place; a record's casing that differs from the listing's
        // still finds the entry.
        m.apply_edits(vec![Edit::Rename { from: "A4".into(), entry: e("A4", 40, false) }], -1).unwrap();
        assert_eq!(log(&r), ["chg 2"]);
        assert!(m.has_name("A4") && m.has_name("a4") && !m.has_name("a3"));
        assert_eq!(m.snapshot_all().iter().filter(|x| x.name.eq_ignore_ascii_case("a4")).count(), 1);
        // Onto an existing name: the target leaves, the renamed row takes its name and keeps its
        // own selection and cursor.
        m.select(SelOp::Only, 0); // a1
        log(&r);
        let a = m.apply_edits(vec![Edit::Rename { from: "a1".into(), entry: e("a5", 10, false) }], 0).unwrap();
        assert_eq!(a, Applied { added: 0, removed: 1, changed: 1, cursor: 2 });
        assert_eq!(log(&r), ["rem 3", "rem 0", "add 2"]);
        assert_eq!(rows(&m), ["a2", "A4", "a5", "a6", "a7", "a8", "a9", "a10", "z9"]);
        assert_eq!(m.selected_names(), ["a5"]);
        assert_eq!(m.row_data(2).unwrap().size, "10 B");
        assert_eq!((m.total(), m.len()), (9, 9));
        // A record's casing on a removal too.
        m.apply_edits(vec![Edit::Remove("Z9".into())], -1).unwrap();
        assert!(!m.has_name("z9"));
        // A sized folder keeps its size across a replace, not across a rename.
        m.apply_edits(vec![Edit::Upsert(e("d1", 0, true))], -1).unwrap();
        m.set_size("d1", Some(2048));
        m.apply_edits(vec![Edit::Upsert(e("d1", 0, true))], -1).unwrap();
        assert_eq!(m.snapshot_all().iter().find(|x| x.name == "d1").unwrap().size_text.as_str(), "2.0 KB");
        m.apply_edits(vec![Edit::Rename { from: "d1".into(), entry: e("d2", 0, true) }], -1).unwrap();
        assert_eq!(m.snapshot_all().iter().find(|x| x.name == "d2").unwrap().size_text.as_str(), "");
    }

    #[test]
    fn apply_edits_notifies_rows_not_resets() {
        let (m, r, g) = edited(SortCol::Modified, false, "");
        m.set_grid_n(4);
        log(&g);
        let mut many: Vec<Edit> = (0..50).map(|i| Edit::Upsert(e(&format!("n{i}"), i, false))).collect();
        many.push(Edit::Remove("a4".into()));
        many.push(Edit::Rename { from: "a5".into(), entry: e("b5", 50, false) });
        many.push(Edit::Upsert(e("a6", 7, false)));
        let a = m.apply_edits(many, 3).unwrap();
        assert_eq!((a.added, a.removed, a.changed), (50, 1, 2));
        let details = log(&r);
        assert_eq!(details.len(), 50 + 1 + 1 + 1 + 1, "one per step: 50 adds, a removal, a moved rename, a replace");
        assert!(details.iter().all(|n| n != "reset"), "the details list never resets: {details:?}");
        assert_eq!(log(&g), ["reset"], "the grid resets once");
        // The list is still in the sort's order and agrees with a fresh pass over the listing.
        let fresh = filter_sort(m.snapshot_all(), Show::OFF, &NameMatch::default(), SortCol::Modified, false, &Restore::default());
        assert_eq!(rows(&m), fresh.shown.iter().map(|x| x.name.to_string()).collect::<Vec<_>>());
        assert_eq!((m.len(), m.total()), (59, 59));
        assert_eq!(m.grid_len(), 15);
        // An empty window does nothing.
        assert_eq!(m.apply_edits(Vec::new(), 2).unwrap(), Applied { added: 0, removed: 0, changed: 0, cursor: 2 });
        assert!(log(&r).is_empty() && log(&g).is_empty());
    }

    #[test]
    fn landed_pass_and_settled() {
        let m = PaneModel::default();
        m.append(vec![e("a", 0, false)]);
        assert!(!m.settled(), "a first enumeration's batches: no pass has landed");
        assert!(m.apply_edits(vec![Edit::Upsert(e("b", 0, false))], -1).is_err());
        m.pass.set(3);
        assert!(!m.settled(), "a pass in flight");
        assert_eq!(m.apply_edits(vec![Edit::Upsert(e("b", 0, false))], -1).unwrap_err(), "a pass is in flight");
        m.set_landed_pass(3);
        assert!(m.settled());
        m.apply_edits(vec![Edit::Upsert(e("b", 0, false))], -1).unwrap();
        assert!(m.settled() && m.pass.get() == 3, "the edits keep the model settled");
        m.begin_refresh();
        assert!(!m.settled(), "a refresh in flight");
        m.finish_refresh();
        assert!(m.settled());
        m.clear(); // navigation: the next listing starts unsettled
        assert!(!m.settled() && m.landed_pass.get() == 0);
    }

    #[test]
    fn apply_edits_upserts_exactly_and_keeps_resolved_icons() {
        let (m, r, _g) = edited(SortCol::Name, true, "");
        m.set_cut(vec!["a3".into()]);
        log(&r);
        // The contract: an Upsert is matched by its exact name (a record carries the on-disk
        // casing), so one that differs from an entry only in case is a new entry, inserted after
        // its equal; the entry it resembles, its row and its cut mark are untouched.
        let a = m.apply_edits(vec![Edit::Upsert(e("A3", 31, false))], -1).unwrap();
        assert_eq!((a.added, a.changed), (1, 0));
        assert_eq!(log(&r), ["add 3"]);
        assert_eq!((m.row_data(2).unwrap().name.as_str(), m.row_data(3).unwrap().name.as_str()), ("a3", "A3"));
        assert!(m.row_data(2).unwrap().cut && !m.row_data(3).unwrap().cut);
        assert_eq!(m.capture(-1).cut, [SharedString::from("a3")]);
        m.apply_edits(vec![Edit::Remove("A3".into())], -1).unwrap();
        assert_eq!(rows(&m)[2..4], ["a3", "a4"], "the exact name is removed, not its case twin");
        // The icon `resolve_icons` gave the row survives a replace with the same key; a new key
        // goes back to the placeholder.
        let keyed = |name: &str, key: &str| Entry { icon_key: key.into(), ..e(name, 5, false) };
        m.apply_edits(vec![Edit::Upsert(keyed("a5", "ext:.txt"))], -1).unwrap();
        m.icons.borrow_mut().complete("ext:.txt".into(), 16, crate::icons::Outcome::Pixels(slint::SharedPixelBuffer::new(1, 1)));
        assert_eq!(m.resolve_icons(0, 99, 16, |_, _| {}), [4]);
        let resolved = m.icon_indices(4, 4)[0];
        assert!(resolved >= PLACEHOLDERS);
        m.apply_edits(vec![Edit::Upsert(keyed("a5", "ext:.txt"))], -1).unwrap();
        assert_eq!(m.icon_indices(4, 4), [resolved]);
        m.apply_edits(vec![Edit::Upsert(keyed("a5", "ext:.log"))], -1).unwrap();
        assert_eq!(m.icon_indices(4, 4), [PLACEHOLDER_FILE]);
        // A rename onto a name that exists under another casing overwrites it when an earlier
        // case-insensitive lookup in the window built the lowercase map (here the Remove of "A8")...
        let total = m.total();
        let a = m.apply_edits(vec![Edit::Remove("A8".into()), Edit::Rename { from: "a6".into(), entry: e("A7", 6, false) }], -1).unwrap();
        assert_eq!((a.removed, a.changed, m.total()), (2, 1, total - 2));
        assert_eq!(m.snapshot_all().iter().filter(|x| x.name.eq_ignore_ascii_case("a7")).map(|x| x.name.to_string()).collect::<Vec<_>>(), ["A7"]);
        // ... and is not looked for otherwise (no scan of every name per rename): the twin stays
        // until the next listing.
        let a = m.apply_edits(vec![Edit::Rename { from: "a9".into(), entry: e("A10", 9, false) }], -1).unwrap();
        assert_eq!((a.removed, a.changed), (0, 1));
        assert_eq!(m.snapshot_all().iter().filter(|x| x.name.eq_ignore_ascii_case("a10")).count(), 2);
    }

    #[test]
    fn cmp_entries_matches_sort_entries() {
        // A shuffled mix of folders and files with equal sizes, times and types among them.
        let mk = |i: u64| {
            let attrs = if i % 7 == 0 { FILE_ATTRIBUTE_DIRECTORY } else { 0 };
            let ty = ["Text Document", "PNG File", "File"][(i % 3) as usize];
            Entry::new(format!("f_{}", (i * 37) % 101).into(), (i * 13) % 5, ((i * 11) % 4) as i64, attrs, ty.into())
        };
        let v: Vec<Entry> = (0..101).map(mk).collect();
        for col in [SortCol::Name, SortCol::Size, SortCol::Modified, SortCol::Type] {
            for asc in [true, false] {
                let mut a = v.clone();
                sort_entries(&mut a, col, asc);
                let mut b = v.clone();
                b.sort_by(|x, y| cmp_entries(x, y, col, asc));
                let names = |s: &[Entry]| s.iter().map(|e| e.name.to_string()).collect::<Vec<_>>();
                assert_eq!(names(&a), names(&b), "{col:?} asc={asc}");
                assert!(a.windows(2).all(|w| cmp_entries(&w[0], &w[1], col, asc) != std::cmp::Ordering::Greater));
            }
        }
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
        // A cloud lookup's key is replaced in the listing and its row (found by key when a pass
        // moved it), and the row then asks for the marked key.
        {
            let m = PaneModel::default();
            m.append(vec![keyed("a", "look:o:ext:.txt|G:\\a"), keyed("b", "look:o:dir|G:\\b")]);
            assert!(m.relabel(1, &"look:o:ext:.txt|G:\\a".into(), "ext:o12:.txt".into()), "found by key, not by the stale row");
            assert!(!m.relabel(0, &"look:o:ext:.txt|G:\\a".into(), "x".into()), "already relabelled");
            assert_eq!(m.entry(0).unwrap().icon_key, "ext:o12:.txt");
            assert_eq!(m.snapshot_all()[0].icon_key, "ext:o12:.txt");
            let mut asked = Vec::new();
            m.resolve_icons(0, 1, 16, |_, e| asked.push(e.icon_key.to_string()));
            assert_eq!(asked, ["ext:o12:.txt", "look:o:dir|G:\\b"]);
        }
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
        // A view switch to the grid (P6-M2): every entry back to its placeholder, so the rows
        // resolved at 16 px ask again at 96 px; a key held at 96 px is taken with no request.
        m.forget_icons();
        assert_eq!(m.icon_indices(0, 99), [PLACEHOLDER_FILE; 4]);
        assert_eq!(m.snapshot_all().iter().map(|e| e.icon).collect::<Vec<_>>(), [PLACEHOLDER_FILE; 4]);
        let mut asked = Vec::new();
        m.resolve_icons(0, 3, 96, |_, e| asked.push(e.icon_key.to_string()));
        assert_eq!(asked, ["ext:.txt", "ext:.png"]);
        m.icons.borrow_mut().complete("ext:.txt".into(), 96, Outcome::Pixels(slint::SharedPixelBuffer::new(1, 1)));
        assert_eq!(m.resolve_icons(0, 3, 96, |_, _| panic!("held at 96 px")), [0, 1, 3]);
        assert_eq!(m.icon_indices(0, 0), [PLACEHOLDERS + 1], "the 96 px buffer, not the 16 px one");
        // Back to the details list: the 16 px buffer again, with no request.
        m.forget_icons();
        assert_eq!(m.resolve_icons(0, 3, 16, |_, _| panic!("held at 16 px")), [0, 1, 3]);
        assert_eq!(m.icon_indices(0, 0), [PLACEHOLDERS]);
        let d = PaneModel::default();
        d.append(vec![e("sub", 0, true)]);
        d.forget_icons();
        assert_eq!(d.icon_indices(0, 0), [PLACEHOLDER_FOLDER], "a folder's placeholder");
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
        let out = filter_sort(m.snapshot_all(), Show::OFF, &NameMatch::default(), SortCol::Name, false, &m.capture(0));
        m.replace(out);
        let cut: Vec<(String, bool)> = (0..3).map(|i| m.row_data(i).unwrap()).map(|r| (r.name.to_string(), r.cut)).collect();
        assert_eq!(cut, [("c".into(), true), ("b".into(), true), ("a".into(), false)]);
        // Clearing the cut while a pass is out: the swap re-derives the marks (none).
        let mut out = filter_sort(m.snapshot_all(), Show::OFF, &NameMatch::default(), SortCol::Name, true, &m.capture(0));
        m.set_cut(vec![]);
        out.restore(&m.capture(0));
        m.replace(out);
        assert!((0..3).all(|i| !m.row_data(i).unwrap().cut));
        m.set_cut(vec!["a".into()]);
        m.clear();
        m.append(vec![e("a", 0, false)]);
        assert!(!m.row_data(0).unwrap().cut, "navigating away forgets the cut");
    }

    #[test]
    fn cut_flags_follow_the_shown_rows() {
        let m = PaneModel::default();
        m.append(vec![e("a", 0, false), e("b", 0, false), e("c", 0, false), e("d", 0, false)]);
        assert_eq!(m.cut_flags(0, 99), [false; 4]);
        m.set_cut(vec!["b".into(), "d".into()]);
        assert_eq!(m.cut_flags(0, 99), [false, true, false, true], "clamped to the list");
        assert_eq!(m.cut_flags(1, 2), [true, false]);
        assert!(m.cut_flags(4, 9).is_empty(), "past the end");
    }
}
