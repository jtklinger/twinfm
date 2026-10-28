//! The sizes view (P13-M3): a tab titled `Sizes: <folder>` that shows where the space under a
//! folder goes, biggest first, each folder opened in place one level down (Jeremy's option A, for
//! freeing space).
//!
//! The walk (`walk`, on a thread of its own per view, as a search's) reads every folder below the
//! root through the directory information (`fs::dirinfo`: one call per 64 KB of entries, each with
//! its size on disk) and keeps **every folder** in a `Tree` with its totals, and no file entries; a
//! level's files are read again when the level is opened (`level`). Sizes are sizes on disk: a
//! compressed or sparse file counts what it holds, a cloud-only file 0, a hard-linked file once per
//! walk (by its file ID, `dirinfo::HardLinks`), a reparse point (a junction, a link) 0 and is not
//! entered, as folder sizes do. Folders deeper than `SIZE_MAX_DEPTH` are counted and not entered.
//!
//! `Tree` and `rows` are pure: the ordering, the opening in place, the share bars' arithmetic and a
//! removal's totals are unit-tested on a tree built by hand.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;
use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT};

use crate::fs::dirinfo;
use crate::icons::IconKeys;
use crate::model::{Entry, Kind, LocalZone, PaneModel, format_size};
use slint::{Model, ModelTracker, SharedString};
use std::cell::RefCell;
use std::rc::Rc;
use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};

/// A node's index in `Tree::nodes`; the root is 0.
pub type Id = u32;
const NONE: Id = Id::MAX;

/// What a folder (with everything below it) holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    /// Size on disk.
    pub disk: u64,
    /// Length (`EndOfFile`), for the done line.
    pub logical: u64,
    pub files: u64,
    pub folders: u64,
    /// Folders that could not be read.
    pub skipped: u64,
}

impl Totals {
    fn add(&mut self, t: &Totals) {
        self.disk += t.disk;
        self.logical += t.logical;
        self.files += t.files;
        self.folders += t.folders;
        self.skipped += t.skipped;
    }

    fn sub(&mut self, t: &Totals) {
        self.disk = self.disk.saturating_sub(t.disk);
        self.logical = self.logical.saturating_sub(t.logical);
        self.files = self.files.saturating_sub(t.files);
        self.folders = self.folders.saturating_sub(t.folders);
        self.skipped = self.skipped.saturating_sub(t.skipped);
    }
}

/// One folder of the walk. Children are a linked list (no Vec per folder: a walk of `C:\` keeps a
/// few hundred thousand of these).
pub struct Node {
    /// The folder's own name (for This PC's drives, the drive's root, `C:\`).
    pub name: Box<str>,
    pub parent: Id,
    first_child: Id,
    next: Id,
    pub depth: u16,
    /// Taken off the tree (deleted or moved out from the view): the walk's later additions below
    /// it are dropped.
    gone: bool,
    /// Its listing has been read (its totals below may still grow while the walk runs).
    pub read: bool,
    pub totals: Totals,
}

/// Every folder under the root, with its totals.
pub struct Tree {
    pub nodes: Vec<Node>,
    /// The root is This PC: its children are drives, named by their roots.
    pub this_pc: bool,
}

impl Tree {
    pub fn new(root_name: &str, this_pc: bool) -> Tree {
        Tree { nodes: vec![node(root_name, NONE, 0)], this_pc }
    }

    /// A new folder under `parent`, counted as a folder of every node above it.
    pub fn add_folder(&mut self, parent: Id, name: &str) -> Id {
        let id = self.nodes.len() as Id;
        let depth = self.nodes[parent as usize].depth + 1;
        let mut n = node(name, parent, depth);
        n.next = self.nodes[parent as usize].first_child;
        self.nodes.push(n);
        self.nodes[parent as usize].first_child = id;
        self.add(parent, &Totals { folders: 1, ..Default::default() });
        id
    }

    /// Add `t` to `id` and every node above it; nothing when it or a node above it is gone.
    pub fn add(&mut self, id: Id, t: &Totals) {
        let mut chain = Vec::new();
        let mut at = id;
        while at != NONE {
            let n = &self.nodes[at as usize];
            if n.gone {
                return;
            }
            chain.push(at);
            at = n.parent;
        }
        for at in chain {
            self.nodes[at as usize].totals.add(t);
        }
    }

    pub fn children(&self, id: Id) -> Children<'_> {
        Children { tree: self, at: self.nodes[id as usize].first_child }
    }

    /// The node's path relative to the root (`sub\dir`; under This PC, from the drive: `C:\sub`).
    pub fn rel(&self, id: Id) -> String {
        let mut parts = Vec::new();
        let mut at = id;
        while at != 0 && at != NONE {
            let n = &self.nodes[at as usize];
            parts.push(&*n.name);
            at = n.parent;
        }
        parts.reverse();
        join(&parts)
    }

    /// The child folder of `parent` named `name` (ASCII case ignored, as the file system does).
    pub fn child(&self, parent: Id, name: &str) -> Option<Id> {
        self.children(parent).find(|&c| self.nodes[c as usize].name.eq_ignore_ascii_case(name))
    }

    /// The folder at `rel` (as `rel` gives it).
    pub fn find(&self, rel: &str) -> Option<Id> {
        let mut at = 0;
        for part in split(rel, self.this_pc) {
            at = self.child(at, part)?;
        }
        Some(at)
    }

    /// Take folder `id` off the tree: its totals come off every node above it. Returns how many
    /// nodes' totals changed (0 when it is the root or already gone).
    pub fn remove_folder(&mut self, id: Id) -> usize {
        if id == 0 || self.nodes[id as usize].gone {
            return 0;
        }
        let mut t = self.nodes[id as usize].totals;
        // The folder itself is counted as a folder of those above it.
        t.folders += 1;
        let parent = self.nodes[id as usize].parent;
        let k = self.take(parent, &t);
        // Out of the parent's list.
        let next = self.nodes[id as usize].next;
        if self.nodes[parent as usize].first_child == id {
            self.nodes[parent as usize].first_child = next;
        } else {
            let mut at = self.nodes[parent as usize].first_child;
            while at != NONE {
                if self.nodes[at as usize].next == id {
                    self.nodes[at as usize].next = next;
                    break;
                }
                at = self.nodes[at as usize].next;
            }
        }
        self.nodes[id as usize].gone = true;
        k
    }

    /// Take `t` off `id` and every node above it; returns how many.
    pub fn take(&mut self, id: Id, t: &Totals) -> usize {
        let mut k = 0;
        let mut at = id;
        while at != NONE {
            let n = &mut self.nodes[at as usize];
            n.totals.sub(t);
            k += 1;
            at = n.parent;
        }
        k
    }

    pub fn root(&self) -> &Totals {
        &self.nodes[0].totals
    }
}

impl Default for Tree {
    fn default() -> Tree {
        Tree::new("", false)
    }
}

/// `path` relative to a sizes view's `root` (`sub\dir`, as `Tree::rel` gives a node's), ASCII case
/// ignored; under This PC the whole path. None for the root itself and for a path outside it.
pub fn rel_under(root: &Path, path: &Path) -> Option<String> {
    let p = path.to_string_lossy();
    if crate::fs::enumerate::is_this_pc(&root.to_string_lossy()) {
        return (p.len() >= 3).then(|| p.to_string());
    }
    let r = root.to_string_lossy();
    let r = r.trim_end_matches('\\');
    let head = p.get(..r.len())?;
    let rest = p[r.len()..].strip_prefix('\\')?;
    (head.eq_ignore_ascii_case(r) && !rest.is_empty()).then(|| rest.trim_end_matches('\\').to_string())
}

fn node(name: &str, parent: Id, depth: u16) -> Node {
    Node { name: name.into(), parent, first_child: NONE, next: NONE, depth, gone: false, read: false, totals: Totals::default() }
}

pub struct Children<'a> {
    tree: &'a Tree,
    at: Id,
}

impl Iterator for Children<'_> {
    type Item = Id;
    fn next(&mut self) -> Option<Id> {
        if self.at == NONE {
            return None;
        }
        let id = self.at;
        self.at = self.tree.nodes[id as usize].next;
        Some(id)
    }
}

/// Path parts joined with `\`, a drive's root (`C:\`) taking no second separator.
fn join(parts: &[&str]) -> String {
    let mut s = String::new();
    for p in parts {
        if !s.is_empty() && !s.ends_with('\\') {
            s.push('\\');
        }
        s.push_str(p);
    }
    s
}

/// `rel` back into its parts (a drive's root first under This PC).
fn split(rel: &str, this_pc: bool) -> Vec<&str> {
    if rel.is_empty() {
        return Vec::new();
    }
    if this_pc && rel.len() >= 3 && &rel[1..3] == ":\\" {
        let mut v = vec![&rel[..3]];
        v.extend(rel[3..].split('\\').filter(|p| !p.is_empty()));
        return v;
    }
    rel.split('\\').filter(|p| !p.is_empty()).collect()
}

/// A file (or a link the walk does not enter) of an opened level, from its listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    pub name: String,
    pub disk: u64,
    pub logical: u64,
    pub attrs: u32,
    pub tag: u32,
    /// Unix seconds.
    pub modified: i64,
}

/// What a row of the view is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Folder(Id),
    /// The k-th file of a level's listing.
    File(Id, usize),
}

/// One row of the view, in the order shown.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub item: Item,
    /// 0 for the root's children.
    pub depth: u16,
    /// Its path relative to the root (the entry's name; as search results', so every command
    /// joins it to the root).
    pub rel: String,
    pub disk: u64,
    pub files: u64,
    /// Its share of the root's size on disk, 0 to 1.
    pub share: f32,
    /// A folder opened in place.
    pub open: bool,
}

/// The rows: the root's children (folders from the tree, files from the root's listing once it
/// has landed), each level biggest first (by size on disk, then by name), an open folder's own
/// level right under it.
pub fn rows(tree: &Tree, open: &HashSet<Id>, files: &HashMap<Id, Vec<File>>) -> Vec<Row> {
    let mut out = Vec::new();
    let total = tree.root().disk;
    level(tree, 0, open, files, total, &mut out);
    out
}

fn level(tree: &Tree, id: Id, open: &HashSet<Id>, files: &HashMap<Id, Vec<File>>, total: u64, out: &mut Vec<Row>) {
    let base = if id == 0 { String::new() } else { tree.rel(id) };
    let depth = tree.nodes[id as usize].depth;
    let mut items: Vec<(u64, &str, Item)> =
        tree.children(id).map(|c| (tree.nodes[c as usize].totals.disk, &*tree.nodes[c as usize].name, Item::Folder(c))).collect();
    if let Some(fs) = files.get(&id) {
        items.extend(fs.iter().enumerate().map(|(k, f)| (f.disk, f.name.as_str(), Item::File(id, k))));
    }
    items.sort_by_cached_key(|x| (std::cmp::Reverse(x.0), x.1.to_lowercase()));
    for (disk, name, item) in items {
        let rel = join(&[&base, name]);
        let (files_n, is_open) = match item {
            Item::Folder(c) => (tree.nodes[c as usize].totals.files, open.contains(&c)),
            Item::File(..) => (1, false),
        };
        out.push(Row { item, depth, rel, disk, files: files_n, share: share(disk, total), open: is_open });
        if let (Item::Folder(c), true) = (item, is_open) {
            level(tree, c, open, files, total, out);
        }
    }
}

/// `part` against `whole`, 0 to 1 (0 for an empty whole).
pub fn share(part: u64, whole: u64) -> f32 {
    if whole == 0 { 0.0 } else { (part as f64 / whole as f64).clamp(0.0, 1.0) as f32 }
}

/// Take a file of an opened level off the view: its size comes off every total above it. Returns
/// (its size on disk, the totals changed) when it was there.
pub fn remove_file(tree: &mut Tree, files: &mut HashMap<Id, Vec<File>>, parent: Id, name: &str) -> Option<(u64, usize)> {
    let list = files.get_mut(&parent)?;
    let k = list.iter().position(|f| f.name.eq_ignore_ascii_case(name))?;
    let f = list.remove(k);
    let counted = f.attrs & FILE_ATTRIBUTE_DIRECTORY == 0;
    let t = Totals { disk: f.disk, logical: f.logical, files: u64::from(counted), folders: u64::from(!counted), skipped: 0 };
    Some((f.disk, tree.take(parent, &t)))
}

/// Take whatever `rel` names off the view (a folder of the tree, or a file of an opened level):
/// (its size on disk, the totals changed), or None when the view does not show it.
pub fn remove(tree: &mut Tree, files: &mut HashMap<Id, Vec<File>>, rel: &str) -> Option<(u64, usize)> {
    if let Some(id) = tree.find(rel).filter(|&id| id != 0) {
        let disk = tree.nodes[id as usize].totals.disk;
        files.remove(&id);
        let k = tree.remove_folder(id);
        return (k > 0).then_some((disk, k));
    }
    let (dir, leaf) = match rel.rsplit_once('\\') {
        // `C:\x` under This PC: the parent is the drive's root, `C:\`.
        Some((d, l)) if d.len() == 2 && d.ends_with(':') => (&rel[..3], l),
        Some((d, l)) => (d, l),
        None => ("", rel),
    };
    let parent = tree.find(dir)?;
    remove_file(tree, files, parent, leaf)
}

/// Windows' compact files (`compact /exe`, CompactOS) are reparse points with the WOF tag; they are
/// files with their data here, counted at what they hold.
const IO_REPARSE_TAG_WOF: u32 = 0x8000_0017;

/// The size on disk a listed file counts: none for a cloud-only file (it takes no space here),
/// none for a reparse point other than a compact file (a link: what it points at is counted where
/// it is).
pub fn file_disk(attrs: u32, tag: u32, disk: u64) -> u64 {
    if crate::fs::content::cloud_only(attrs) || (attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 && tag != IO_REPARSE_TAG_WOF) {
        0
    } else {
        disk
    }
}

/// The walk's progress, sent to the UI.
pub enum Progress {
    /// Partial totals (at most every `sizes::PARTIAL_EVERY`).
    Partial,
    /// The walk ended, in `ms`, having met `ids` files (each hard-linked file once).
    Done { ms: u64, ids: usize },
    Cancelled,
}

/// Walk `roots` (one root, or This PC's drives under the tree's root) into `tree`, folder by
/// folder, locking it for each folder's additions only. `progress` is called every
/// `PARTIAL_EVERY` and at the end.
pub fn walk(tree: &Arc<Mutex<Tree>>, roots: &[(Id, PathBuf)], cancel: &AtomicBool, mut progress: impl FnMut(Progress)) {
    let t0 = Instant::now();
    let mut last = Instant::now();
    let mut links = dirinfo::HardLinks::default();
    let mut stack: Vec<(Id, PathBuf, usize)> = roots.iter().rev().map(|(id, p)| (*id, p.clone(), 0)).collect();
    let mut subs: Vec<String> = Vec::new();
    while let Some((id, dir, depth)) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            return progress(Progress::Cancelled);
        }
        subs.clear();
        let mut t = Totals::default();
        let res = dirinfo::read(&dir, true, |i| {
            if i.is_dot() {
                return;
            }
            if i.attrs & FILE_ATTRIBUTE_DIRECTORY != 0 {
                if i.attrs & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
                    subs.push(String::from_utf16_lossy(i.name));
                } else {
                    // A junction or a directory link: a folder, not entered.
                    t.folders += 1;
                }
            } else {
                t.files += 1;
                if links.first(i.id) {
                    t.disk += file_disk(i.attrs, i.tag, i.disk);
                    t.logical += i.logical;
                }
            }
        });
        if res.is_err() {
            t.skipped = 1;
        }
        {
            let mut tr = tree.lock().unwrap_or_else(PoisonError::into_inner);
            tr.add(id, &t);
            if let Some(n) = tr.nodes.get_mut(id as usize) {
                n.read = true;
            }
            for name in subs.drain(..) {
                let c = tr.add_folder(id, &name);
                if depth < crate::fs::sizes::SIZE_MAX_DEPTH {
                    stack.push((c, dir.join(&name), depth + 1));
                }
            }
        }
        if last.elapsed() >= crate::fs::sizes::PARTIAL_EVERY {
            last = Instant::now();
            progress(Progress::Partial);
        }
    }
    progress(Progress::Done { ms: t0.elapsed().as_millis() as u64, ids: links.len() });
}

/// The files of one level (`dir`), each with its size on disk, and the links and junctions the walk
/// counts as folders and does not enter (shown at 0). Err with the Win32 error.
pub fn level_files(dir: &Path) -> Result<Vec<File>, u32> {
    let mut out = Vec::new();
    dirinfo::read(dir, false, |i| {
        if i.is_dot() || (i.attrs & FILE_ATTRIBUTE_DIRECTORY != 0 && i.attrs & FILE_ATTRIBUTE_REPARSE_POINT == 0) {
            return;
        }
        out.push(File {
            name: String::from_utf16_lossy(i.name),
            disk: file_disk(i.attrs, i.tag, i.disk),
            logical: i.logical,
            attrs: i.attrs,
            tag: i.tag,
            modified: unix_secs(i.modified),
        });
    })?;
    Ok(out)
}

/// FILETIME ticks to unix seconds (0 before 1970), as the listing's.
fn unix_secs(ticks: i64) -> i64 {
    const EPOCH_1970: i64 = 116_444_736_000_000_000;
    if ticks < EPOCH_1970 { 0 } else { (ticks - EPOCH_1970) / 10_000_000 }
}


/// What a sizes row draws besides its entry (`SizeModel`), kept beside the pane's rows in the same
/// order.
#[derive(Clone, Debug, Default)]
pub struct Extra {
    pub depth: u16,
    pub share: f32,
    pub files: SharedString,
    pub folder: bool,
    pub open: bool,
}

/// The rows as the pane's entries and their extras: a folder's entry is a folder (a drive under
/// This PC) whose size is its size on disk; a file's is the file with its size on disk as its size.
/// Every name is the path under the root (`Row::rel`).
pub fn entries(tree: &Tree, files: &HashMap<Id, Vec<File>>, root: &Path, rows: &[Row], zone: &LocalZone) -> (Vec<Entry>, Vec<Extra>) {
    let mut keys: HashMap<Id, IconKeys> = HashMap::new();
    let mut out = Vec::with_capacity(rows.len());
    let mut extra = Vec::with_capacity(rows.len());
    for r in rows {
        let (e, folder) = match r.item {
            Item::Folder(_) => {
                let mut e = if tree.this_pc && r.depth == 0 {
                    Entry::drive(r.rel.as_str().into(), None, "Local Disk".into())
                } else {
                    Entry::with_zone(r.rel.as_str().into(), r.disk, 0, FILE_ATTRIBUTE_DIRECTORY, Kind::Directory, "File folder".into(), "dir".into(), zone)
                };
                e.size = r.disk;
                e.size_text = format_size(r.disk).into();
                (e, true)
            }
            Item::File(parent, k) => {
                let f = &files[&parent][k];
                let dir = if parent == 0 { root.to_path_buf() } else { root.join(tree.rel(parent)) };
                let kind = Kind::from_attrs(f.attrs, f.tag);
                let key = keys.entry(parent).or_insert_with(|| IconKeys::new(dir, false)).for_entry_synced(&f.name, f.attrs, kind, f.modified, f.logical, true);
                let mut e = Entry::with_zone(r.rel.as_str().into(), r.disk, f.modified, f.attrs, kind, SharedString::default(), key, zone);
                e.size_text = format_size(r.disk).into();
                (e, false)
            }
        };
        out.push(e);
        extra.push(Extra { depth: r.depth, share: r.share, files: if folder { count(r.files).into() } else { SharedString::default() }, folder, open: r.open });
    }
    (out, extra)
}

/// A count with thousands separated (`12,345`).
pub fn count(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The sizes view's rows for Slint: the pane's entries with their extras, the pane model's own
/// notifications (a selection change notifies its row here too).
pub struct SizeModel {
    model: Rc<PaneModel>,
    extra: Rc<RefCell<Vec<Extra>>>,
}

impl SizeModel {
    pub fn new(model: Rc<PaneModel>, extra: Rc<RefCell<Vec<Extra>>>) -> SizeModel {
        SizeModel { model, extra }
    }
}

impl Model for SizeModel {
    type Data = crate::SizeRow;

    fn row_count(&self) -> usize {
        self.model.len()
    }

    fn row_data(&self, row: usize) -> Option<crate::SizeRow> {
        let x = self.extra.borrow().get(row).cloned().unwrap_or_default();
        self.model.with_entry(row, |e| crate::SizeRow {
            name: leaf(&e.name).into(),
            kind: e.kind as i32,
            selected: e.selected,
            cut: e.cut,
            depth: i32::from(x.depth),
            share: x.share,
            size: e.size_text.clone(),
            files: x.files.clone(),
            folder: x.folder,
            open: x.open,
        })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        self.model.tracker()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// A row's own name: the last part of its path (a drive's root, `C:\`, whole).
pub fn leaf(rel: &str) -> &str {
    match rel.trim_end_matches('\\').rsplit_once('\\') {
        Some((_, l)) => l,
        None => rel,
    }
}

/// The fixed drives' roots (`C:\`), for a sizes view of This PC.
pub fn fixed_drives() -> Vec<PathBuf> {
    // SAFETY: no arguments.
    let mask = unsafe { GetLogicalDrives() };
    (0..26u8)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| format!("{}:\\", (b'A' + i) as char))
        .filter(|r| {
            let w: Vec<u16> = r.encode_utf16().chain([0]).collect();
            // SAFETY: a NUL-terminated root alive for the call.
            unsafe { GetDriveTypeW(w.as_ptr()) == DRIVE_FIXED }
        })
        .map(PathBuf::from)
        .collect()
}

/// GetDriveTypeW's fixed drive (WinBase.h).
const DRIVE_FIXED: u32 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(disk: u64, files: u64) -> Totals {
        Totals { disk, logical: disk, files, ..Default::default() }
    }

    fn file(name: &str, disk: u64) -> File {
        File { name: name.into(), disk, logical: disk, attrs: 0x20, tag: 0, modified: 0 }
    }

    /// root: a (100 in 2 files, with a\deep: 300 in 3), b (50), files top.txt 200 and z.txt 0.
    fn sample() -> (Tree, HashMap<Id, Vec<File>>) {
        let mut t = Tree::new(r"C:\root", false);
        let a = t.add_folder(0, "a");
        let b = t.add_folder(0, "b");
        let deep = t.add_folder(a, "deep");
        t.add(a, &bytes(100, 2));
        t.add(deep, &bytes(300, 3));
        t.add(b, &bytes(50, 1));
        t.add(0, &bytes(200, 2));
        let mut files = HashMap::new();
        files.insert(0, vec![file("z.txt", 0), file("top.txt", 200)]);
        files.insert(a, vec![file("one", 60), file("two", 40)]);
        (t, files)
    }

    fn shown(r: &[Row]) -> Vec<(&str, u16, u64)> {
        r.iter().map(|r| (r.rel.as_str(), r.depth, r.disk)).collect()
    }

    #[test]
    fn levels_sort_biggest_first_and_open_in_place() {
        let (t, files) = sample();
        assert_eq!(*t.root(), Totals { disk: 650, logical: 650, files: 8, folders: 3, skipped: 0 });
        assert_eq!(t.nodes[1].totals, Totals { disk: 400, logical: 400, files: 5, folders: 1, skipped: 0 }, "a holds deep");
        let closed = rows(&t, &HashSet::new(), &files);
        assert_eq!(shown(&closed), [("a", 0, 400), ("top.txt", 0, 200), ("b", 0, 50), ("z.txt", 0, 0)]);
        assert!(closed.iter().all(|r| !r.open));
        assert_eq!(closed[0].files, 5);
        assert_eq!(closed[1].files, 1);
        // a opened: its folder and its listed files, sorted among themselves, right under it.
        let a = t.find("a").unwrap();
        let open: HashSet<Id> = [a].into();
        let r = rows(&t, &open, &files);
        assert_eq!(
            shown(&r),
            [("a", 0, 400), (r"a\deep", 1, 300), (r"a\one", 1, 60), (r"a\two", 1, 40), ("top.txt", 0, 200), ("b", 0, 50), ("z.txt", 0, 0)]
        );
        assert!(r[0].open && !r[1].open);
        assert_eq!(r[2].item, Item::File(a, 0));
        // deep opened too, with no listing yet: its subfolders only (none).
        let deep = t.find(r"A\DEEP").expect("found ignoring case");
        let r = rows(&t, &[a, deep].into(), &files);
        assert_eq!(r.len(), 7);
        assert!(r[1].open);
        // A tie in size goes by name.
        let mut t2 = Tree::new("x", false);
        t2.add_folder(0, "b");
        t2.add_folder(0, "A");
        assert_eq!(shown(&rows(&t2, &HashSet::new(), &HashMap::new())), [("A", 0, 0), ("b", 0, 0)]);
    }

    #[test]
    fn share_bars_are_parts_of_the_root() {
        let (t, files) = sample();
        let r = rows(&t, &HashSet::new(), &files);
        assert!((r[0].share - 400.0 / 650.0).abs() < 1e-6);
        assert!((r[1].share - 200.0 / 650.0).abs() < 1e-6);
        assert_eq!(r[3].share, 0.0);
        assert_eq!(share(5, 0), 0.0, "an empty root draws no bar");
        assert_eq!(share(10, 10), 1.0);
        assert_eq!(share(20, 10), 1.0, "never past the whole");
    }

    #[test]
    fn a_removal_comes_off_every_total_above_it() {
        let (mut t, mut files) = sample();
        // A file of an opened level: off a and the root.
        assert_eq!(remove(&mut t, &mut files, r"a\one"), Some((60, 2)));
        assert_eq!(t.nodes[1].totals.disk, 340);
        assert_eq!(t.nodes[1].totals.files, 4);
        assert_eq!(t.root().disk, 590);
        assert_eq!(files[&1].len(), 1);
        // A folder: deep off a and the root, with its files and itself as a folder.
        assert_eq!(remove(&mut t, &mut files, r"a\deep"), Some((300, 2)));
        assert_eq!(t.nodes[1].totals, Totals { disk: 40, logical: 40, files: 1, folders: 0, skipped: 0 });
        assert_eq!(*t.root(), Totals { disk: 290, logical: 290, files: 4, folders: 2, skipped: 0 });
        assert!(t.find(r"a\deep").is_none(), "out of the tree");
        // The walk's later additions below a removed folder are dropped.
        t.add(3, &bytes(1000, 1));
        assert_eq!(t.root().disk, 290);
        // A root file; and what the view does not show.
        assert_eq!(remove(&mut t, &mut files, "top.txt"), Some((200, 1)));
        assert_eq!(t.root().disk, 90);
        assert_eq!(remove(&mut t, &mut files, "nothing"), None);
        assert_eq!(remove(&mut t, &mut files, r"b\unlisted"), None, "b's files were never listed");
        let r = rows(&t, &[1].into(), &files);
        assert_eq!(shown(&r), [("b", 0, 50), ("a", 0, 40), (r"a\two", 1, 40), ("z.txt", 0, 0)]);
    }

    #[test]
    fn this_pc_names_rows_from_the_drive() {
        let mut t = Tree::new("This PC", true);
        let c = t.add_folder(0, r"C:\");
        let d = t.add_folder(0, r"D:\");
        let users = t.add_folder(c, "Users");
        t.add(users, &bytes(10, 1));
        t.add(d, &bytes(5, 1));
        let mut files = HashMap::new();
        files.insert(c, vec![file("pagefile.sys", 7)]);
        let r = rows(&t, &[c].into(), &files);
        assert_eq!(shown(&r), [(r"C:\", 0, 10), (r"C:\Users", 1, 10), (r"C:\pagefile.sys", 1, 7), (r"D:\", 0, 5)]);
        assert_eq!(t.find(r"C:\Users"), Some(users));
        assert_eq!(remove(&mut t, &mut files, r"C:\pagefile.sys"), Some((7, 2)));
        assert_eq!(t.root().disk, 8);
    }

    #[test]
    fn paths_under_the_root() {
        let root = Path::new(r"C:\Data");
        assert_eq!(rel_under(root, Path::new(r"c:\data\Sub\x.txt")).as_deref(), Some(r"Sub\x.txt"));
        assert_eq!(rel_under(root, Path::new(r"C:\Data")), None, "the root itself");
        assert_eq!(rel_under(root, Path::new(r"C:\Database\x")), None, "a longer name is not under it");
        assert_eq!(rel_under(root, Path::new(r"D:\x")), None);
        assert_eq!(rel_under(Path::new(r"C:\"), Path::new(r"C:\Users")).as_deref(), Some("Users"));
        assert_eq!(rel_under(Path::new("This PC"), Path::new(r"C:\Users")).as_deref(), Some(r"C:\Users"));
        assert_eq!(leaf(r"C:\Users\me"), "me");
        assert_eq!(leaf(r"C:\"), r"C:\");
        assert_eq!(leaf("top.txt"), "top.txt");
        assert_eq!(count(1_234_567), "1,234,567");
        assert_eq!(count(999), "999");
    }

    #[test]
    fn files_count_what_they_hold_here() {
        assert_eq!(file_disk(0x20, 0, 4096), 4096);
        assert_eq!(file_disk(0x0040_0020, 0, 4096), 0, "cloud-only (recall on data access)");
        assert_eq!(file_disk(0x1000, 0, 4096), 0, "offline");
        assert_eq!(file_disk(0x420, 0xA000_000C, 4096), 0, "a symbolic link");
        assert_eq!(file_disk(0x420, IO_REPARSE_TAG_WOF, 4096), 4096, "a compact file");
        assert_eq!(unix_secs(116_444_736_000_000_000 + 10_000_000), 1);
        assert_eq!(unix_secs(0), 0);
    }

    #[test]
    fn a_walk_reads_folders_into_the_tree() {
        let dir = std::env::temp_dir().join(format!("twinfm-p13m3-walk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(r"a\deep")).unwrap();
        std::fs::create_dir_all(dir.join("b")).unwrap();
        std::fs::write(dir.join("top.txt"), vec![1u8; 10_000]).unwrap();
        std::fs::write(dir.join(r"a\deep\x.bin"), vec![1u8; 100_000]).unwrap();
        std::fs::write(dir.join(r"b\y.bin"), vec![1u8; 5_000]).unwrap();
        // A hard link to x.bin: the walk counts the file once.
        std::fs::hard_link(dir.join(r"a\deep\x.bin"), dir.join(r"b\x-link.bin")).unwrap();
        let tree = Arc::new(Mutex::new(Tree::new(&dir.to_string_lossy(), false)));
        let never = AtomicBool::new(false);
        let mut ends = Vec::new();
        walk(&tree, &[(0, dir.clone())], &never, |p| ends.push(matches!(p, Progress::Done { .. })));
        let stop = AtomicBool::new(true);
        let mut cancelled = false;
        walk(&Arc::new(Mutex::new(Tree::new("x", false))), &[(0, dir.clone())], &stop, |p| cancelled = matches!(p, Progress::Cancelled));
        let level = level_files(&dir.join("b")).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(ends.last(), Some(&true));
        assert!(cancelled);
        let t = tree.lock().unwrap();
        let root = *t.root();
        assert_eq!((root.files, root.folders, root.skipped), (4, 3, 0));
        assert_eq!(root.logical, 115_000, "x.bin counted once of its two names");
        let deep = t.find(r"a\deep").unwrap();
        assert!(t.nodes[deep as usize].read);
        assert_eq!(t.nodes[deep as usize].totals.files, 1);
        // x.bin's clusters count under whichever of its two names the walk met first, once.
        let b = t.find("b").unwrap();
        let (dd, bd) = (t.nodes[deep as usize].totals.disk, t.nodes[b as usize].totals.disk);
        assert!(dd + bd >= 105_000, "deep {dd}, b {bd}");
        assert!(root.disk >= 115_000);
        let mut names: Vec<&str> = level.iter().map(|f| f.name.as_str()).collect();
        names.sort();
        assert_eq!(names, ["x-link.bin", "y.bin"], "a level's listing has every name");
    }
}
