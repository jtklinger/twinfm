//! Shell icons. A pool of worker threads (`ICON_WORKERS`), each initialised for COM (MTA), owns
//! every shell call: a worker turns a path into a 32-bit HBITMAP through
//! `IShellItemImageFactory::GetImage` and the bitmap into a premultiplied RGBA pixel buffer, with
//! the shell's overlay (a link's arrow) drawn over it for links and reparse points. The UI thread keeps one buffer per cache key in an
//! `IconTable` that both panes share, and composes the icons of the rows on screen into one
//! strip image per pane (`compose`), so the list rows carry no image at all.
//!
//! Cache keys (`icon_key`): ordinary files share one key per lowercase extension (`ext:.txt`),
//! plain directories share `dir`; files whose icon is their own (`.exe`, `.lnk`, `.ico`, `.url`,
//! `.cur`, no extension), customised folders (a `desktop.ini` that the shell honours), reparse
//! points and drives are keyed by full path. A cloud placeholder (OneDrive) is keyed as if it
//! were no reparse point: its icon is its type's, or the folder's. With thumbnails on (settings
//! `thumbnails`), a plain file whose extension is in `THUMB_EXTS` is keyed by itself
//! (`thumb:<full path>|<modified>|<size>`) and rendered from the shell's thumbnail cache
//! (`IThumbnailCache`), letterboxed into the icon square; without a thumbnail it gets its icon.
//! The table and the workers keep each key per icon size in px, so the 16 px and 32 px lists'
//! icons and the grid's 96 px ones (tiles take thumbnails at that size, P4-M2) never stand in for
//! each other.

use crate::model::Kind;
use crossbeam_channel::{Receiver, Sender};
use slint::{Rgba8Pixel, SharedPixelBuffer, SharedString};
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::cell::Cell;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::SIZE;
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject,
    GdiFlush, GetDC, GetDIBits, GetObjectW, HBITMAP, ReleaseDC, SelectObject,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_PINNED, FILE_ATTRIBUTE_READONLY,
    FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SYSTEM, FILE_ATTRIBUTE_UNPINNED,
};
use windows_sys::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};
use windows_sys::Win32::UI::Shell::{
    SHCreateItemFromParsingName, SHDefExtractIconW, SHFILEINFOW, SHGFI_ADDOVERLAYS, SHGFI_ICON, SHGFI_OVERLAYINDEX, SHGFI_SMALLICON, SHGetFileInfoW,
    SHGetImageList, SHIL_EXTRALARGE, SHIL_JUMBO, SHIL_LARGE, SHIL_SMALL, SIIGBF, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY, SIIGBF_THUMBNAILONLY,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{DI_NORMAL, DestroyIcon, DrawIconEx, HICON};
use windows_sys::core::{GUID, HRESULT};

/// Icon table slots that exist before any shell icon arrives, one per kind of row.
pub const PLACEHOLDER_FILE: u16 = 0;
pub const PLACEHOLDER_FOLDER: u16 = 1;
pub const PLACEHOLDER_DRIVE: u16 = 2;
pub const PLACEHOLDERS: u16 = 3;

/// The details list's row height and icon size in logical px (pane.slint): 22 px rows with 16 px
/// icons, or with thumbnails on (settings `thumbnails`) 32 px rows with 32 px icons. The icon
/// buffer is the icon size times the window scale factor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListGeom {
    pub row_px: f32,
    pub icon_px: f32,
}

impl ListGeom {
    pub const fn new(thumbnails: bool) -> ListGeom {
        if thumbnails { ListGeom { row_px: 32.0, icon_px: 32.0 } } else { ListGeom { row_px: 22.0, icon_px: 16.0 } }
    }

    /// The name column's left edge in a row: 6 px padding plus the icon column, one row high,
    /// plus a 4 px gap after a thumbnail (the rows' `padding-left`, 28 or 42 px).
    pub fn name_x(&self) -> f32 {
        6.0 + self.row_px + if self.row_px > 22.0 { 4.0 } else { 0.0 }
    }
}

/// A grid's tiles (P4-M1; the medium size P6-M2), logical px: `w` x `h`, N to a grid row (N =
/// floor(list width / w), at least 1), each with an `icon_px` icon centred horizontally `icon_top`
/// px below the tile's top and the name under it (pane.slint FileGrid, FileGridMedium). Chosen per
/// pane (settings `tile_left` / `tile_right`) and read at layout, never bound in a tile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileGeom {
    pub w: f32,
    pub h: f32,
    pub icon_px: f32,
    pub icon_top: f32,
}

impl TileGeom {
    /// Large icons: the shell's 96 px icon filling the tile's width, the name under it on up to two
    /// lines (P7-M4; one line and 120 px before): 2 + 96 + 38 px tall.
    pub const LARGE: TileGeom = TileGeom { w: 96.0, h: 136.0, icon_px: 96.0, icon_top: 2.0 };
    /// Medium icons: the shell's 48 px icon (SHIL_EXTRALARGE, a thumbnail cache size), so nothing
    /// is scaled. Explorer's medium tile at 100% is 74 px wide on an 82 x 99 pitch with names on up
    /// to three lines (measured with UI Automation, 2026-10-17); ours too since P8-L1 (one line and
    /// 72 px until P7-M4, two lines and 88 px until P8-L1): 2 + 48 + 57 px tall, 80 px wide (a 76
    /// px name).
    pub const MEDIUM: TileGeom = TileGeom { w: 80.0, h: 107.0, icon_px: 48.0, icon_top: 2.0 };

    pub fn of(medium: bool) -> TileGeom {
        if medium { TileGeom::MEDIUM } else { TileGeom::LARGE }
    }
}

/// Grid rows composed into the strip above and below the visible ones: one 136 px row, about the
/// details strip's 88 px (4 rows of 22) of scroll before a recompose, at a fraction of the size.
pub const GRID_MARGIN_ROWS: usize = 1;

/// Extensions whose icon lives in the file itself, so they are keyed by full path.
const OWN_ICON_EXTS: [&str; 5] = ["exe", "lnk", "ico", "url", "cur"];

/// Extensions a plain file gets a thumbnail for when thumbnails are on: images, video, documents.
/// Whether the shell has a thumbnail handler for one is its business; without one the row shows
/// the type's icon.
pub const THUMB_EXTS: &[&str] = &[
    "jpg", "jpeg", "jpe", "png", "gif", "bmp", "webp", "tif", "tiff", "heic", "heif", "avif", // images
    "mp4", "m4v", "mov", "mkv", "avi", "wmv", "webm", // video
    "pdf", "docx", "doc", "pptx", "ppt", "xlsx", "xls", // documents
];

/// One icon fetch. `generation` is the requesting pane's navigation generation; the worker
/// skips the request when the pane has moved on.
pub struct Request {
    pub generation: u64,
    pub pane: usize,
    pub key: SharedString,
    pub path: PathBuf,
    pub size_px: u32,
    /// Draw the shell's overlay (a link's arrow) over the icon (`wants_overlay`).
    pub overlay: bool,
    /// The shell's thumbnail, letterboxed into the `size_px` square (a `thumb:` key), with the
    /// icon as the fallback.
    pub thumbnail: bool,
    /// For a thumbnail: the row it was asked for and the pane's resolve it was asked in
    /// (`Views`), so a worker can drop it once the list has scrolled away; for a lookup (a
    /// `look:` key) the row, handed back in the reply. 0 otherwise.
    pub row: usize,
    pub wanted: u64,
    /// The cloud mark drawn over the icon (`key_mark` of the key; P6-M1), which also keeps a
    /// cloud-only file's thumbnail to the shell's cache (`Mark::cache_only`).
    pub mark: Mark,
}

/// The key of a drag image's icon (P5-M3): asked for when a row is pressed, at the drag image's
/// icon size, so a drag that follows need not wait for the shell on the UI thread. Its reply goes
/// to the pending drag icon, never to the icon table, and the workers render it for every press
/// (no `taken` entry): one shell call per press, on a worker.
pub fn drag_key(path: &Path) -> SharedString {
    format!("{DRAG_PREFIX}{}", path.display()).into()
}

const DRAG_PREFIX: &str = "drag:";

pub fn is_drag_key(key: &str) -> bool {
    key.starts_with(DRAG_PREFIX)
}

/// The rows each pane asks icons for (the rows on screen and the UI's margin), published by the
/// UI once per resolve while thumbnails are on, so the workers can drop thumbnail requests for
/// rows a scroll has passed (`passed`). Plain icon requests (shared keys, cheap) are never dropped.
#[derive(Default)]
pub struct Views {
    /// Each pane's last resolve number.
    seq: [AtomicU64; 2],
    /// The rows each pane asked for then, first << 32 | last.
    range: [AtomicU64; 2],
}

impl Views {
    /// The pane's resolve `seq` asks for rows `first..=last`.
    pub fn publish(&self, pane: usize, seq: u64, (first, last): (usize, usize)) {
        self.range[pane].store((first as u64) << 32 | (last as u64 & 0xFFFF_FFFF), Ordering::Relaxed);
        self.seq[pane].store(seq, Ordering::Release);
    }

    /// A thumbnail for `row`, asked for in resolve `wanted`, is not wanted any more: the pane has
    /// resolved since, and the row lies more than `STRIP_MARGIN_ROWS` outside the rows it asks for.
    /// The UI asks again if the row comes back (the reply is `Skipped`).
    fn passed(&self, pane: usize, row: usize, wanted: u64) -> bool {
        if self.seq[pane].load(Ordering::Acquire) == wanted {
            return false;
        }
        let r = self.range[pane].load(Ordering::Relaxed);
        let (first, last) = ((r >> 32) as usize, (r & 0xFFFF_FFFF) as usize);
        row + STRIP_MARGIN_ROWS < first || row > last + STRIP_MARGIN_ROWS
    }
}

pub enum Outcome {
    Pixels(SharedPixelBuffer<Rgba8Pixel>),
    /// A thumbnail: its pixels and its picture's rectangle (x, y, w, h) in the square, which the
    /// tiles frame (P6-M2). Interned as `Pixels` are.
    Picture(SharedPixelBuffer<Rgba8Pixel>, [u32; 4]),
    /// The shell had no icon for this path; the rows keep their placeholder.
    Failed,
    /// Not rendered (the pane navigated away, or the key was delivered before); the UI may ask again.
    Skipped,
    /// A `look:` key's mark (None when the shell gave none): the entry takes `looked`'s key.
    Looked(Mark),
    /// A thumbnail the cache has not got yet (`LATER_HRESULTS`): the type icon, shown meanwhile;
    /// the table asks for the thumbnail again (`SizeKeys::later`).
    Later(SharedPixelBuffer<Rgba8Pixel>),
}

pub struct Reply {
    pub key: SharedString,
    /// The size asked for: the table keeps each key per size.
    pub size_px: u32,
    pub outcome: Outcome,
    /// The request's pane and row (a lookup's entry is found by them, then by its key).
    pub pane: usize,
    pub row: usize,
}

/// Worker-side counters, read by the UI's once-a-second `[icons]` line.
#[derive(Default)]
pub struct Stats {
    pub rendered: AtomicU64,
    pub render_ns: AtomicU64,
    /// Icons that got an overlay drawn over them.
    pub overlays: AtomicU64,
    /// Renders (overlay included) over `SLOW_MS`.
    pub slow: AtomicU64,
    /// Thumbnail requests rendered (from a thumbnail or, without one, the icon) and their time;
    /// not counted in `rendered`.
    pub thumbs: AtomicU64,
    pub thumb_ns: AtomicU64,
    /// Thumbnail requests dropped because the list scrolled away from their row (`Views`).
    pub dropped: AtomicU64,
    /// Cloud mark lookups made (`look:` keys: one shell call each; P6-M1).
    pub cloud: AtomicU64,
}

/// Lowercase extension after the last dot, as Explorer and `TypeNames` read it, or None.
fn extension(name: &str) -> Option<&str> {
    match name.rfind('.') {
        Some(i) if i + 1 < name.len() => Some(&name[i + 1..]),
        _ => None,
    }
}

/// `attrs` as the key rules read them: a cloud placeholder's icon is its type's (a file) or
/// the plain folder's, so it is keyed as if it were no reparse point. Sharing those keys is
/// what spares one shell call per placeholder (one took 568 ms in M2).
fn key_attrs(attrs: u32, kind: Kind) -> u32 {
    if kind == Kind::Cloud { attrs & !FILE_ATTRIBUTE_REPARSE_POINT } else { attrs }
}

/// The cache key for a directory entry (drives are keyed by their root, see `Entry::drive`).
/// `custom_folder` says the directory carries a `desktop.ini` the shell honours (`custom_folder`).
pub fn icon_key(folder: &Path, name: &str, attrs: u32, kind: Kind, custom_folder: bool) -> String {
    let full = || folder.join(name).to_string_lossy().into_owned();
    let attrs = key_attrs(attrs, kind);
    if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return full();
    }
    if attrs & FILE_ATTRIBUTE_DIRECTORY != 0 {
        return if custom_folder { full() } else { "dir".to_string() };
    }
    match extension(name) {
        Some(ext) if !OWN_ICON_EXTS.iter().any(|o| o.eq_ignore_ascii_case(ext)) => {
            format!("ext:.{}", ext.to_ascii_lowercase())
        }
        _ => full(),
    }
}

/// Whether an entry's icon should carry the shell's overlay: links (`.lnk`, `.url`), junctions,
/// symbolic links, app execution aliases and other reparse points. Each of them is keyed by full
/// path, so the overlay never lands on a key other files share; a folder named `x.lnk` (keyed
/// `dir`) does not qualify.
pub fn wants_overlay(kind: Kind, attrs: u32, name: &str) -> bool {
    match kind {
        Kind::Junction | Kind::Symlink | Kind::AppAlias | Kind::Reparse => true,
        Kind::Drive => false,
        _ => {
            attrs & FILE_ATTRIBUTE_DIRECTORY == 0
                && extension(name).is_some_and(|e| e.eq_ignore_ascii_case("lnk") || e.eq_ignore_ascii_case("url"))
        }
    }
}

/// Whether a directory has its own icon through `desktop.ini`. The shell reads `desktop.ini`
/// only in folders marked read-only or system, so only those cost a file probe: unfiltered, the
/// probe cost 25 to 155 us per directory (1.1 to 4.2 s for the 26,867 folders of
/// `C:\Windows\WinSxS`); filtered, 0.26 ms for System32 and nothing for WinSxS.
pub fn custom_folder(folder: &Path, name: &str, attrs: u32) -> bool {
    attrs & FILE_ATTRIBUTE_DIRECTORY != 0
        && attrs & FILE_ATTRIBUTE_REPARSE_POINT == 0
        && attrs & (FILE_ATTRIBUTE_READONLY | FILE_ATTRIBUTE_SYSTEM) != 0
        && folder.join(name).join("desktop.ini").exists()
}

/// Whether a file of this name gets a thumbnail when thumbnails are on (its extension is in
/// `THUMB_EXTS`; `x.jpg.lnk` is a shortcut, not an image).
pub fn is_thumbnail(name: &str) -> bool {
    extension(name).is_some_and(|e| THUMB_EXTS.iter().any(|t| t.eq_ignore_ascii_case(e)))
}

/// A file's thumbnail key: its full path, modified time (unix seconds) and size, so a changed file
/// gets a new key (rendered again after the watch's refresh) and the old entry is simply unused.
pub fn thumb_key(full: &Path, modified: i64, size: u64) -> String {
    format!("thumb:{}|{modified}|{size}", full.display())
}

/// Where a listing's entries carry cloud marks (P6-M1), decided once per folder on the UI thread
/// (`cloud::Locations::of`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cloud {
    /// Today's keys and today's cost.
    #[default]
    None,
    /// Under a registered sync root (OneDrive): a file's state comes from its attributes, a
    /// folder's from the shell's `System.StorageProviderState` (a lookup on a worker).
    SyncRoot,
    /// On Google Drive's volume: every entry's mark is the shell's overlay (a lookup on a worker).
    Overlays,
}

/// A cloud mark over an icon (P6-M1). `Overlay` is the shell's overlay at that index (Google
/// Drive's handlers answer `SHGFI_OVERLAYINDEX`); `State` is a sync root's
/// `System.StorageProviderState` value, drawn with the glyph Explorer's Status column draws for it
/// (`STATE_GLYPHS`, from imageres.dll). OneDrive's own overlay handlers mark nothing for any
/// process but Explorer's status code reads (the P6-M1 record), hence the second kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Mark {
    #[default]
    None,
    Overlay(u8),
    State(u8),
}

impl Mark {
    /// The mark's part of a key: `o12`, `s1`; None for no mark.
    fn tag(self) -> Option<String> {
        match self {
            Mark::None => None,
            Mark::Overlay(i) => Some(format!("o{i}")),
            Mark::State(s) => Some(format!("s{s}")),
        }
    }

    /// Whether a thumbnail for a file with this mark may only come from the shell's cache: a
    /// sync root's cloud-only file (state 1, "Available when online"), and every Google Drive
    /// file, whose overlay index does not say which of its handlers set it. An extraction would
    /// download the file.
    pub fn cache_only(self) -> bool {
        matches!(self, Mark::Overlay(_) | Mark::State(STATE_CLOUD))
    }
}

/// `System.StorageProviderState`'s "Available when online": the file is only in the cloud.
pub const STATE_CLOUD: u8 = 1;

/// `System.StorageProviderState` values and the imageres.dll icon Explorer's Status column draws
/// for each (the property's enumeration, read through `IPropertyDescription` on 2026-10-16): 1
/// "Available when online" (the cloud), 2 "Available on this device" (the outlined tick), 3
/// "Always available on this device" (the solid tick), 4, 5 and 10 "Sync pending" and 6 "Syncing"
/// (the arrows), 7 "Error", 8 "Warning". 9, "Excluded (not synced)", has no image.
const STATE_GLYPHS: [(u8, u16); 9] = [(1, 1404), (2, 1400), (3, 1405), (4, 1401), (5, 1401), (6, 1401), (7, 1402), (8, 1403), (10, 1401)];

/// A sync root file's state from its attributes, as the shell's property gives it (checked on
/// 987 cloud-only and 6 local files, P6-M1): recall on data access (the cloud filter keeps that
/// one visible to a process that does not expose placeholders) or offline, the cloud; pinned and
/// unpinned both (OneDrive's own files, "Excluded"), no mark; pinned, always available; else on
/// this device. Folders carry none of this and are looked up instead.
pub fn state_from_attrs(attrs: u32) -> Mark {
    if attrs & (FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_OFFLINE) != 0 {
        Mark::State(STATE_CLOUD)
    } else if attrs & FILE_ATTRIBUTE_PINNED != 0 && attrs & FILE_ATTRIBUTE_UNPINNED != 0 {
        Mark::None
    } else if attrs & FILE_ATTRIBUTE_PINNED != 0 {
        Mark::State(3)
    } else {
        Mark::State(2)
    }
}

/// `o<digits>:` or `s<digits>:` at the start of `s`: the mark and what follows it.
fn parse_tag(s: &str) -> Option<(Mark, &str)> {
    let (tag, rest) = s.split_once(':')?;
    let (kind, digits) = tag.split_at_checked(1)?;
    let n: u8 = if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) { return None } else { digits.parse().ok()? };
    match kind {
        "o" => Some((Mark::Overlay(n), rest)),
        "s" => Some((Mark::State(n), rest)),
        _ => None,
    }
}

/// The key of an entry whose plain key is `plain` with `mark` over its icon: `ext:<mark>:.txt`,
/// `folder:<mark>`, `thumb:<mark>:<path>|<modified>|<size>`, and `cloud:<mark>:<path>` for a key
/// that is a full path (its own icon). Type keys stay shared by every file of that type and mark;
/// no mark gives `plain` back.
pub fn marked(plain: &str, mark: Mark) -> String {
    let Some(tag) = mark.tag() else { return plain.to_string() };
    if plain == "dir" {
        format!("folder:{tag}")
    } else if let Some(rest) = plain.strip_prefix("ext:") {
        format!("ext:{tag}:{rest}")
    } else if let Some(rest) = plain.strip_prefix("thumb:") {
        format!("thumb:{tag}:{rest}")
    } else {
        format!("cloud:{tag}:{plain}")
    }
}

/// The mark a key carries (`marked`), or None.
pub fn key_mark(key: &str) -> Mark {
    if let Some(rest) = key.strip_prefix("folder:") {
        return parse_tag(&format!("{rest}:")).map_or(Mark::None, |(m, _)| m);
    }
    ["ext:", "thumb:", "cloud:"]
        .iter()
        .find_map(|p| key.strip_prefix(p))
        .and_then(parse_tag)
        .map_or(Mark::None, |(m, _)| m)
}

/// The Cloud Files placeholder state from a listed entry's attributes and reparse tag
/// (`CfGetPlaceholderStateFromAttributeTag`: no file is opened, nothing is read from the provider).
pub fn placeholder_state(attrs: u32, tag: u32) -> u32 {
    // SAFETY: a pure function of its two arguments in cldapi.dll.
    unsafe { windows_sys::Win32::Storage::CloudFilters::CfGetPlaceholderStateFromAttributeTag(attrs, tag) }
}

/// Whether a placeholder state calls for the provider's own state (P9-M4): a placeholder that is
/// not in sync. On Jeremy's OneDrive every placeholder read in sync (0x9 or 0xD), so the property
/// read (about 23 ms) stays off the rows of a synced folder. Also a file freed up but not released
/// yet (unpinned, its data still here: `attrs`): OneDrive can leave it so for minutes, its state
/// "sync pending", while its attributes alone would read as on this device (the console check).
/// TwinFM's own process is not shown placeholders (Windows hides them from most programs: no
/// reparse point, state 0), so the second test is the one that works there. `shown` (P12-M7): the
/// entry was listed on a thread shown placeholders (`fs::enumerate::expose_placeholders`), where a
/// file that is no placeholder is one the provider has not taken in yet (copied there and uploading,
/// Explorer's arrows: the P12-M7 probe's file was a plain file for 5.7 s, then an in-sync
/// placeholder), unless it is pinned and unpinned both, OneDrive's own "excluded" files
/// (`desktop.ini`), which keep no mark as before.
pub fn needs_state_read(state: u32, attrs: u32, shown: bool) -> bool {
    use windows_sys::Win32::Storage::CloudFilters::{CF_PLACEHOLDER_STATE_IN_SYNC, CF_PLACEHOLDER_STATE_PLACEHOLDER};
    let releasing = attrs & FILE_ATTRIBUTE_UNPINNED != 0
        && attrs & (FILE_ATTRIBUTE_PINNED | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_OFFLINE) == 0;
    let placeholder = state & CF_PLACEHOLDER_STATE_PLACEHOLDER != 0;
    let excluded = attrs & (FILE_ATTRIBUTE_PINNED | FILE_ATTRIBUTE_UNPINNED) == FILE_ATTRIBUTE_PINNED | FILE_ATTRIBUTE_UNPINNED;
    releasing
        || (placeholder && state & CF_PLACEHOLDER_STATE_IN_SYNC == 0)
        || (shown && !placeholder && !excluded && attrs & FILE_ATTRIBUTE_DIRECTORY == 0)
}

const LOOK_PREFIX: &str = "look:";

/// The key of an entry whose mark is not known yet: `look:<o|s>:<plain key>|<full path>`. A row
/// with it asks the workers for the mark (a lookup, `Outcome::Looked`) instead of an icon, and
/// takes `looked`'s key when the reply lands. One per file (a path has no `|`), never cached.
fn look_key(kind: char, plain: &str, full: &Path) -> String {
    format!("{LOOK_PREFIX}{kind}:{plain}|{}", full.display())
}

pub fn is_look_key(key: &str) -> bool {
    key.starts_with(LOOK_PREFIX)
}

/// A looked-up entry's key: its plain key with the mark found.
pub fn looked(look: &str, mark: Mark) -> Option<String> {
    Some(marked(look_plain(look)?, mark))
}

/// A `look:` key's plain key.
fn look_plain(look: &str) -> Option<&str> {
    let rest = look.strip_prefix(LOOK_PREFIX)?;
    Some(rest.get(2..)?.rsplit_once('|')?.0)
}

/// Whether `old`, an entry's key from before a refresh, is the key a lookup of `look` (the new
/// listing's key for the same name) gives with `old`'s mark: the same type, the same thumbnail
/// (path, time and size), whatever the mark (P11-M2: the row may show `old`'s icon until the
/// lookup lands).
pub fn same_but_mark(old: &str, look: &str) -> bool {
    look_plain(look).is_some_and(|plain| marked(plain, key_mark(old)) == old)
}

/// Per-enumeration key builder: extension keys and `dir` are shared SharedStrings, so most
/// entries get a refcount bump rather than an allocation. Lives on the enumeration worker.
/// `thumbnails`: plain files in `THUMB_EXTS` get their own `thumb:` key (`thumb_key`). `cloud`
/// (`with_cloud`): the folder's cloud location, which marks every key but those of links, reparse
/// points and drives (they keep the shell's own overlay, the link arrow).
pub struct IconKeys {
    folder: PathBuf,
    dir: SharedString,
    ext: HashMap<String, SharedString>,
    thumbnails: bool,
    cloud: Cloud,
    /// Marked type keys, shared like `ext`.
    marked: HashMap<(SharedString, Mark), SharedString>,
}

impl IconKeys {
    pub fn new(folder: PathBuf, thumbnails: bool) -> IconKeys {
        IconKeys { folder, dir: "dir".into(), ext: HashMap::new(), thumbnails, cloud: Cloud::None, marked: HashMap::new() }
    }

    pub fn with_cloud(mut self, cloud: Cloud) -> IconKeys {
        self.cloud = cloud;
        self
    }

    /// `modified` (unix seconds) and `size` go into a thumbnail's key only. An entry in sync: the
    /// tests' form of `for_entry_synced`.
    #[cfg(test)]
    pub fn for_entry(&mut self, name: &str, attrs: u32, kind: Kind, modified: i64, size: u64) -> SharedString {
        self.for_entry_synced(name, attrs, kind, modified, size, true)
    }

    /// `for_entry` for a listed entry, `synced` false when it is a cloud placeholder out of sync
    /// (`needs_state_read`, P9-M4): under a sync root such a file is looked up as a folder is, so
    /// its mark is the provider's own state (syncing, pending, error) rather than its attributes'.
    pub fn for_entry_synced(&mut self, name: &str, attrs: u32, kind: Kind, modified: i64, size: u64, synced: bool) -> SharedString {
        let plain = self.plain(name, attrs, kind, modified, size);
        if self.cloud == Cloud::None || kind == Kind::Drive || wants_overlay(kind, attrs, name) {
            return plain;
        }
        if self.cloud == Cloud::Overlays || attrs & FILE_ATTRIBUTE_DIRECTORY != 0 || !synced {
            let kind = if self.cloud == Cloud::Overlays { 'o' } else { 's' };
            return look_key(kind, &plain, &self.folder.join(name)).into();
        }
        let mark = state_from_attrs(attrs);
        if plain.starts_with("thumb:") || mark == Mark::None {
            return marked(&plain, mark).into();
        }
        self.marked.entry((plain, mark)).or_insert_with_key(|(p, m)| marked(p, *m).into()).clone()
    }

    fn plain(&mut self, name: &str, attrs: u32, kind: Kind, modified: i64, size: u64) -> SharedString {
        // Out of a cloud location (a search tab walking into OneDrive), a file not on this disk is
        // never keyed for its thumbnail: nothing would keep the request to the cache (P9-M4).
        let away = self.cloud == Cloud::None && attrs & (FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_OFFLINE) != 0;
        if self.thumbnails && kind == Kind::File && is_thumbnail(name) && !away {
            return thumb_key(&self.folder.join(name), modified, size).into();
        }
        let attrs = key_attrs(attrs, kind);
        let custom = custom_folder(&self.folder, name, attrs);
        if attrs & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) == FILE_ATTRIBUTE_DIRECTORY && !custom {
            return self.dir.clone();
        }
        if attrs & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) == 0 {
            if let Some(ext) = extension(name) {
                if let Some(k) = self.ext.get(ext) {
                    return k.clone();
                }
            }
        }
        let key = icon_key(&self.folder, name, attrs, kind, custom);
        if key.starts_with("ext:") {
            let k: SharedString = key.as_str().into();
            // Cached under the extension as spelled; another spelling misses once and caches too.
            if let Some(ext) = extension(name) {
                self.ext.insert(ext.to_string(), k.clone());
            }
            return k;
        }
        key.into()
    }
}

/// First and last row (inclusive) whose `row_px` band intersects the viewport, for a list of
/// `len` rows scrolled to `content_y` (zero or negative, as Slint reports it) with `height`
/// visible. None for an empty list.
pub fn visible_range(content_y: f32, height: f32, len: usize, row_px: f32) -> Option<(usize, usize)> {
    if len == 0 {
        return None;
    }
    let top = (-content_y).max(0.0);
    let first = ((top / row_px).floor() as usize).min(len - 1);
    let last = (((top + height.max(0.0)) / row_px).ceil() as usize).clamp(first, len - 1);
    Some((first, last))
}

/// Rows composed into the strip above and below the visible range, so a scroll of less than
/// this many rows moves the strip image without recomposing it.
pub const STRIP_MARGIN_ROWS: usize = 4;

/// Physical geometry of an icon strip at one window scale factor. Sizes are rounded once, so
/// the strip is composed at integer pixels whatever the scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StripGeom {
    /// Icon edge in physical pixels (the list's logical icon size times the scale factor, rounded).
    pub icon_px: u32,
    pub scale: f32,
    pub list: ListGeom,
}

impl StripGeom {
    /// The strip for the details list with thumbnails off (22 px rows, 16 px icons) or on (32 px
    /// rows, 32 px icons) at window scale `scale`.
    pub fn new(scale: f32, thumbnails: bool) -> StripGeom {
        let list = ListGeom::new(thumbnails);
        StripGeom { icon_px: ((list.icon_px * scale).round() as u32).max(1), scale, list }
    }

    /// Physical y of the icon of the k-th row of the strip: the row's top plus the centering
    /// offset of the icon in its row (3 px of a 22 px row, none in a 32 px one).
    pub fn icon_y(&self, k: usize) -> u32 {
        let ListGeom { row_px, icon_px } = self.list;
        ((k as f32 * row_px + (row_px - icon_px) / 2.0) * self.scale).round() as u32
    }

    /// Physical height of a strip of `rows` rows.
    pub fn height(&self, rows: usize) -> u32 {
        (rows as f32 * self.list.row_px * self.scale).round() as u32
    }
}

/// The rows (inclusive) a strip covers for the visible range `(first, last)` of a list of `len`
/// rows: the visible rows plus `STRIP_MARGIN_ROWS` each side, clamped to the list.
pub fn strip_range((first, last): (usize, usize), len: usize) -> (usize, usize) {
    (first.saturating_sub(STRIP_MARGIN_ROWS), (last + STRIP_MARGIN_ROWS).min(len.saturating_sub(1)))
}

/// What a pane's strip was last composed from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Composed {
    pub first: usize,
    pub last: usize,
    pub icon_px: u32,
}

/// Whether a pane's strip must be recomposed: nothing composed yet, its content went stale
/// (`dirty`: an icon landed for a row in it, or the rows changed), the scale changed, the
/// visible range left the composed rows, or the list shrank under them. A scroll that keeps
/// the visible rows inside the composed ones only moves the image.
pub fn needs_recompose(c: Option<Composed>, visible: (usize, usize), len: usize, icon_px: u32, dirty: bool) -> bool {
    match c {
        None => true,
        Some(c) => dirty || c.icon_px != icon_px || visible.0 < c.first || visible.1 > c.last || c.last >= len,
    }
}

/// Scale every premultiplied channel of `line` to 45%, rounded (255 becomes 115, 0 stays 0): the
/// icon of a row on the clipboard as a cut, matching its text's `transparentize(55%)`.
pub fn dim_line(line: &mut [Rgba8Pixel]) {
    let f = |v: u8| ((v as u32 * 45 + 50) / 100) as u8;
    for p in line {
        *p = Rgba8Pixel { r: f(p.r), g: f(p.g), b: f(p.b), a: f(p.a) };
    }
}

/// Compose a strip: `icons[k]` (each `g.icon_px` square, premultiplied) is copied, one scanline
/// at a time, to `y = g.icon_y(k)` of a buffer `g.icon_px` wide and `g.height(icons.len())`
/// tall, dimmed (`dim_line`) when `dim[k]` is true (entry k is cut; a `dim` shorter than `icons`
/// leaves the rest undimmed). An icon of any other size is left out (the table hands out fitted
/// buffers).
pub fn compose(icons: &[SharedPixelBuffer<Rgba8Pixel>], dim: &[bool], g: &StripGeom) -> SharedPixelBuffer<Rgba8Pixel> {
    let w = g.icon_px as usize;
    let h = g.height(icons.len()) as usize;
    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(w as u32, h as u32);
    let dst = buf.make_mut_slice();
    for (k, icon) in icons.iter().enumerate() {
        if icon.width() != g.icon_px || icon.height() != g.icon_px {
            continue;
        }
        let dimmed = dim.get(k) == Some(&true);
        let y0 = g.icon_y(k) as usize;
        for (r, line) in icon.as_slice().chunks_exact(w).enumerate() {
            let at = (y0 + r) * w;
            if at + w > dst.len() {
                break;
            }
            dst[at..at + w].copy_from_slice(line);
            if dimmed {
                dim_line(&mut dst[at..at + w]);
            }
        }
    }
    buf
}

/// The grid rows (inclusive) a grid strip covers for the visible grid rows `(first, last)` of a
/// grid of `rows` rows: the visible ones plus `GRID_MARGIN_ROWS` each side, clamped.
pub fn grid_strip_range((first, last): (usize, usize), rows: usize) -> (usize, usize) {
    (first.saturating_sub(GRID_MARGIN_ROWS), (last + GRID_MARGIN_ROWS).min(rows.saturating_sub(1)))
}

/// Physical geometry of a grid strip: `cols` tiles of `tile` to a grid row, at one scale factor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridGeom {
    /// Icon edge in physical pixels (the tile's icon size times the scale factor, rounded).
    pub icon_px: u32,
    pub cols: usize,
    pub scale: f32,
    pub tile: TileGeom,
}

impl GridGeom {
    pub fn new(scale: f32, cols: usize, tile: TileGeom) -> GridGeom {
        GridGeom { icon_px: ((tile.icon_px * scale).round() as u32).max(1), cols: cols.max(1), scale, tile }
    }

    /// Physical (x, y) of the icon of the k-th entry of the strip (grid row k / cols, column
    /// k % cols): its tile's corner plus the centring offset and the top margin.
    pub fn icon_pos(&self, k: usize) -> (u32, u32) {
        let (r, c) = (k / self.cols, k % self.cols);
        let t = self.tile;
        let x = ((c as f32 * t.w + (t.w - t.icon_px) / 2.0) * self.scale).round() as u32;
        let y = ((r as f32 * t.h + t.icon_top) * self.scale).round() as u32;
        (x, y)
    }

    /// Physical width of the strip: `cols` tiles.
    pub fn width(&self) -> u32 {
        (self.cols as f32 * self.tile.w * self.scale).round() as u32
    }

    /// Physical height of a strip of `rows` grid rows.
    pub fn height(&self, rows: usize) -> u32 {
        (rows as f32 * self.tile.h * self.scale).round() as u32
    }
}

/// The frame drawn around a thumbnail's picture in a tile (P6-M2): a neutral grey, one logical
/// pixel wide (at least one physical pixel).
pub const FRAME: Rgba8Pixel = Rgba8Pixel { r: 190, g: 190, b: 190, a: 255 };

/// Compose a grid strip of `rows` grid rows: `icons[k]` (each `g.icon_px` square,
/// premultiplied; entry k of the strip) is copied scanline by scanline to `g.icon_pos(k)`, dimmed
/// when `dim[k]` is true, as `compose` does. `frames[k]`, when given, is the picture's rectangle
/// (x, y, w, h) inside a thumbnail's square (`IconTable::picture`): its edge pixels are drawn in
/// `FRAME`, dimmed with the icon. A type icon has none. An icon of any other size is left out;
/// anything past the buffer's edge is clipped.
pub fn compose_grid(icons: &[SharedPixelBuffer<Rgba8Pixel>], dim: &[bool], frames: &[Option<[u32; 4]>], g: &GridGeom, rows: usize) -> SharedPixelBuffer<Rgba8Pixel> {
    let (w, h) = (g.width() as usize, g.height(rows) as usize);
    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(w as u32, h as u32);
    let dst = buf.make_mut_slice();
    let n = g.icon_px as usize;
    for (k, icon) in icons.iter().enumerate() {
        if icon.width() != g.icon_px || icon.height() != g.icon_px {
            continue;
        }
        let (x0, y0) = g.icon_pos(k);
        let (x0, y0) = (x0 as usize, y0 as usize);
        if x0 >= w {
            continue;
        }
        let span = n.min(w - x0);
        let dimmed = dim.get(k) == Some(&true);
        for (r, line) in icon.as_slice().chunks_exact(n).enumerate() {
            if y0 + r >= h {
                break;
            }
            let at = (y0 + r) * w + x0;
            dst[at..at + span].copy_from_slice(&line[..span]);
            if dimmed {
                dim_line(&mut dst[at..at + span]);
            }
        }
        if let Some(&Some([fx, fy, fw, fh])) = frames.get(k) {
            let t = (g.scale.round() as usize).max(1);
            let (fx, fy, fw, fh) = (fx as usize, fy as usize, fw as usize, fh as usize);
            for y in fy..(fy + fh).min(n) {
                for x in fx..(fx + fw).min(n) {
                    let edge = x < fx + t || x + t >= fx + fw || y < fy + t || y + t >= fy + fh;
                    if edge && x0 + x < w && y0 + y < h {
                        let at = (y0 + y) * w + x0 + x;
                        dst[at] = FRAME;
                        if dimmed {
                            dim_line(&mut dst[at..at + 1]);
                        }
                    }
                }
            }
        }
    }
    buf
}

/// Box-filter resample of a premultiplied buffer to `px` square: each target pixel averages
/// the source pixels its area covers (at least one). Used once per icon and size.
pub fn fit(src: &SharedPixelBuffer<Rgba8Pixel>, px: u32) -> SharedPixelBuffer<Rgba8Pixel> {
    resample(src, px, px)
}

/// `fit` to `w` x `h`.
fn resample(src: &SharedPixelBuffer<Rgba8Pixel>, w: u32, h: u32) -> SharedPixelBuffer<Rgba8Pixel> {
    let (sw, sh) = (src.width() as usize, src.height() as usize);
    if sw == 0 || sh == 0 {
        return SharedPixelBuffer::new(w, h);
    }
    if (sw, sh) == (w as usize, h as usize) {
        return src.clone();
    }
    let (nw, nh) = (w as usize, h as usize);
    let s = src.as_slice();
    let mut out = SharedPixelBuffer::<Rgba8Pixel>::new(w, h);
    let span = |i: usize, len: usize, n: usize| {
        let a = i * len / n;
        (a, ((i + 1) * len / n).max(a + 1).min(len))
    };
    for (y, row) in out.make_mut_slice().chunks_exact_mut(nw).enumerate() {
        let (y0, y1) = span(y, sh, nh);
        for (x, d) in row.iter_mut().enumerate() {
            let (x0, x1) = span(x, sw, nw);
            let mut acc = [0u32; 4];
            for yy in y0..y1 {
                for p in &s[yy * sw + x0..yy * sw + x1] {
                    acc[0] += p.r as u32;
                    acc[1] += p.g as u32;
                    acc[2] += p.b as u32;
                    acc[3] += p.a as u32;
                }
            }
            let c = ((y1 - y0) * (x1 - x0)) as u32;
            *d = Rgba8Pixel { r: (acc[0] / c) as u8, g: (acc[1] / c) as u8, b: (acc[2] / c) as u8, a: (acc[3] / c) as u8 };
        }
    }
    out
}

/// Where `letterbox` puts a `sw` x `sh` picture in a `px` square: (x, y, w, h), its longer side
/// `px`, centred. None for an empty picture.
pub fn letterbox_rect(sw: u32, sh: u32, px: u32) -> Option<[u32; 4]> {
    let (sw, sh) = (sw as u64, sh as u64);
    if sw == 0 || sh == 0 {
        return None;
    }
    let long = sw.max(sh);
    let side = |d: u64| ((d * px as u64 + long / 2) / long).clamp(1, px as u64) as u32;
    let (w, h) = (side(sw), side(sh));
    Some([(px - w) / 2, (px - h) / 2, w, h])
}

/// A thumbnail in a `px` square: its longer side fitted to `px` (box filter, aspect kept), centred,
/// with transparent bands, so it composes into the strip like any icon.
pub fn letterbox(src: &SharedPixelBuffer<Rgba8Pixel>, px: u32) -> SharedPixelBuffer<Rgba8Pixel> {
    let Some([x0, y0, w, h]) = letterbox_rect(src.width(), src.height(), px) else {
        return SharedPixelBuffer::new(px, px);
    };
    let scaled = resample(src, w, h);
    let mut out = SharedPixelBuffer::<Rgba8Pixel>::new(px, px);
    let (x0, y0, n) = (x0 as usize, y0 as usize, px as usize);
    let dst = out.make_mut_slice();
    for (r, line) in scaled.as_slice().chunks_exact(w as usize).enumerate() {
        let at = (y0 + r) * n + x0;
        dst[at..at + w as usize].copy_from_slice(line);
    }
    out
}

/// Convert GetDIBits output (top-down, 32-bit BGRA, premultiplied) to RGBA. Bitmaps from icons
/// with no alpha channel come back with every alpha byte zero; those are opaque. A channel above
/// its alpha is clamped to it: the shell's 96 px folder icon has an edge row of straight (not
/// premultiplied) colour at alpha 64, which the renderer, blending it as premultiplied, saturated
/// into a thin pale lime fringe along the folder's top (P4-M2).
pub fn bgra_to_rgba(bgra: &[u8], width: u32, height: u32) -> SharedPixelBuffer<Rgba8Pixel> {
    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
    let opaque = bgra.chunks_exact(4).all(|p| p[3] == 0);
    for (dst, src) in buf.make_mut_slice().iter_mut().zip(bgra.chunks_exact(4)) {
        let a = if opaque { 255 } else { src[3] };
        *dst = Rgba8Pixel { r: src[2].min(a), g: src[1].min(a), b: src[0].min(a), a };
    }
    buf
}

/// `over` drawn over `base`, source-over on premultiplied pixels: `dst = src + dst * (1 - src.a)`
/// per channel. The shell's overlay images are full icon size with the glyph in the bottom-left
/// corner, so the two buffers line up at (0, 0). Buffers of different sizes give `base` back.
pub fn overlay(base: &SharedPixelBuffer<Rgba8Pixel>, over: &SharedPixelBuffer<Rgba8Pixel>) -> SharedPixelBuffer<Rgba8Pixel> {
    let mut out = base.clone();
    if (base.width(), base.height()) != (over.width(), over.height()) {
        return out;
    }
    for (d, s) in out.make_mut_slice().iter_mut().zip(over.as_slice()) {
        let keep = 255 - s.a as u32;
        let mix = |s: u8, d: u8| (s as u32 + (d as u32 * keep + 127) / 255).min(255) as u8;
        *d = Rgba8Pixel { r: mix(s.r, d.r), g: mix(s.g, d.g), b: mix(s.b, d.b), a: mix(s.a, d.a) };
    }
    out
}

#[repr(C)]
struct ImageFactoryVtbl {
    _query_interface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    _add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    get_image: unsafe extern "system" fn(*mut c_void, SIZE, SIIGBF, *mut HBITMAP) -> HRESULT,
}

const IID_ISHELLITEMIMAGEFACTORY: GUID = GUID::from_u128(0xbcc18b79_ba16_442f_80c4_8a59c30c463b);

/// The shell's icon for `path` at `size_px` square, or None. Needs COM: the worker, and the UI
/// thread once per drag for the drag image (`dnd::attach_drag_image`).
pub(crate) fn render(path: &Path, size_px: u32) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    render_with(path, size_px, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK)
}

/// `render` with the image factory's flags given.
fn render_with(path: &Path, size_px: u32, flags: SIIGBF) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut factory: *mut c_void = std::ptr::null_mut();
    // SAFETY: `wide` is NUL-terminated and outlives the call; `factory` receives an interface
    // pointer of the IID asked for, or stays null on failure.
    let hr = unsafe { SHCreateItemFromParsingName(wide.as_ptr(), std::ptr::null_mut(), &IID_ISHELLITEMIMAGEFACTORY, &mut factory) };
    if hr < 0 || factory.is_null() {
        return None;
    }
    let mut hbm: HBITMAP = std::ptr::null_mut();
    let size = SIZE { cx: size_px as i32, cy: size_px as i32 };
    // SAFETY: `factory` is a live IShellItemImageFactory, whose vtable starts with IUnknown's
    // three methods followed by GetImage; it is released exactly once here.
    let hr = unsafe {
        let vtbl = *(factory as *const *const ImageFactoryVtbl);
        let hr = ((*vtbl).get_image)(factory, size, flags, &mut hbm);
        ((*vtbl).release)(factory);
        hr
    };
    if hr < 0 || hbm.is_null() {
        return None;
    }
    let pixels = hbitmap_pixels(hbm);
    // SAFETY: GetImage handed us ownership of the bitmap.
    unsafe { DeleteObject(hbm) };
    pixels
}

/// Read a bitmap's pixels as top-down 32-bit BGRA and convert them.
fn hbitmap_pixels(hbm: HBITMAP) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    // SAFETY: `bm` is a writable BITMAP of the size passed.
    let mut bm: BITMAP = unsafe { std::mem::zeroed() };
    if unsafe { GetObjectW(hbm, std::mem::size_of::<BITMAP>() as i32, &mut bm as *mut BITMAP as *mut c_void) } == 0 {
        return None;
    }
    let (w, h) = (bm.bmWidth, bm.bmHeight.abs());
    if w <= 0 || h <= 0 || w > 1024 || h > 1024 {
        return None;
    }
    // SAFETY: zeroed BITMAPINFO is a valid starting point; the header fields are set below.
    let mut bmi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w,
        biHeight: -h, // negative: top-down rows
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..bmi.bmiHeader
    };
    let mut bgra = vec![0u8; (w * h * 4) as usize];
    // SAFETY: `bgra` holds h rows of w 32-bit pixels, as `bmi` describes; the screen DC is
    // released before returning.
    let lines = unsafe {
        let dc = GetDC(std::ptr::null_mut());
        let n = GetDIBits(dc, hbm, 0, h as u32, bgra.as_mut_ptr() as *mut c_void, &mut bmi, DIB_RGB_COLORS);
        ReleaseDC(std::ptr::null_mut(), dc);
        n
    };
    (lines == h).then(|| bgra_to_rgba(&bgra, w as u32, h as u32))
}

#[repr(C)]
struct UnknownVtbl {
    _query_interface: usize,
    _add_ref: usize,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
}

/// IThumbnailCache (thumbcache.h): IUnknown, then GetThumbnail (GetThumbnailByID unused).
#[repr(C)]
struct ThumbnailCacheVtbl {
    _query_interface: usize,
    _add_ref: usize,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    get_thumbnail: unsafe extern "system" fn(*mut c_void, *mut c_void, u32, u32, *mut *mut c_void, *mut u32, *mut c_void) -> HRESULT,
}

/// ISharedBitmap (thumbcache.h): IUnknown, then GetSharedBitmap (the rest unused).
#[repr(C)]
struct SharedBitmapVtbl {
    _query_interface: usize,
    _add_ref: usize,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    get_shared_bitmap: unsafe extern "system" fn(*mut c_void, *mut HBITMAP) -> HRESULT,
}

const CLSID_LOCAL_THUMBNAIL_CACHE: GUID = GUID::from_u128(0x50ef4544_ac9f_4a8e_b21b_8a26180db13f);
const IID_ITHUMBNAILCACHE: GUID = GUID::from_u128(0xf676c15d_596a_4ce2_8234_33996f445db1);
const IID_ISHELLITEM: GUID = GUID::from_u128(0x43826d1e_e718_42ee_bc55_a1e261c37bfe);
/// WTS_EXTRACT (thumbcache.h): from the cache, or extracted (and cached) when it is not there.
const WTS_EXTRACT: u32 = 0;
/// WTS_INCACHEONLY: from the cache only, never extracted (a cloud-only file's content would be
/// downloaded by an extraction; P6-M1).
const WTS_INCACHEONLY: u32 = 1;
/// WTS_E_FAILEDEXTRACTION (thumbcache.h): the handler could not make a picture of this file.
const WTS_E_FAILEDEXTRACTION: HRESULT = 0x8004B200_u32 as HRESULT;
/// STG_E_FILENOTFOUND: under WTS_INCACHEONLY, not in the cache.
const STG_E_FILENOTFOUND: HRESULT = 0x80030002_u32 as HRESULT;
/// The answers that say "not now" rather than "no picture": WTS_E_EXTRACTIONTIMEDOUT,
/// WTS_E_SURROGATEUNAVAILABLE, WTS_E_EXTRACTIONPENDING (thumbcache.h) and E_PENDING. The type icon
/// shows meanwhile and the thumbnail is asked for again (`SizeKeys::later`).
const LATER_HRESULTS: [HRESULT; 4] = [0x8004B201_u32 as HRESULT, 0x8004B202_u32 as HRESULT, 0x8004B205_u32 as HRESULT, 0x8000000A_u32 as HRESULT];

/// A thumbnail's answer: the picture, none (the type icon stands), or none yet (`LATER_HRESULTS`).
enum Thumbnail {
    Got(Rendered),
    None,
    Later,
}

/// Whether a request is for a thumbnail, and where the thumbnail may come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Thumb {
    No,
    Extract,
    CacheOnly,
}

thread_local! {
    /// This worker thread's IThumbnailCache, made on its first thumbnail and released before the
    /// thread leaves COM (`release_thumbnail_cache`): a COM object stays on the thread that made it.
    static THUMB_CACHE: Cell<*mut c_void> = const { Cell::new(std::ptr::null_mut()) };
}

/// The shell's thumbnail for `path` from the system thumbnail cache (extracted and cached on a
/// miss, unless `wts` is `WTS_INCACHEONLY`), letterboxed into a `size_px` square; None when there is
/// no handler, the extraction failed or it is not in the cache, Later when the cache says not yet.
/// The cache hands back its own stored size (32, 96, 256 px ...), fitted here. Measured against
/// `IShellItemImageFactory::GetImage(SIIGBF_THUMBNAILONLY)` on 200 JPEGs (P3-M5 decision 1): 7.3 ms
/// against 8.9 warm and 36 against 38 cold per thumbnail. Worker threads only (COM, MTA).
fn thumbnail(path: &Path, size_px: u32, wts: u32) -> Thumbnail {
    let cache = THUMB_CACHE.with(|c| {
        if c.get().is_null() {
            let mut p: *mut c_void = std::ptr::null_mut();
            // SAFETY: `p` receives an IThumbnailCache pointer or stays null.
            let hr = unsafe { CoCreateInstance(&CLSID_LOCAL_THUMBNAIL_CACHE, std::ptr::null_mut(), CLSCTX_INPROC_SERVER, &IID_ITHUMBNAILCACHE, &mut p) };
            if hr >= 0 {
                c.set(p);
            }
        }
        c.get()
    });
    if cache.is_null() {
        return Thumbnail::None;
    }
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut item: *mut c_void = std::ptr::null_mut();
    // SAFETY: `wide` is NUL-terminated and outlives the call; `item` receives an IShellItem or stays null.
    if unsafe { SHCreateItemFromParsingName(wide.as_ptr(), std::ptr::null_mut(), &IID_ISHELLITEM, &mut item) } < 0 || item.is_null() {
        return Thumbnail::None;
    }
    let mut shared: *mut c_void = std::ptr::null_mut();
    let mut flags = 0u32;
    // SAFETY: `cache` is this thread's live IThumbnailCache and `item` a live IShellItem, released
    // once here; `shared` receives an ISharedBitmap or stays null.
    let hr = unsafe {
        let hr = ((**(cache as *const *const ThumbnailCacheVtbl)).get_thumbnail)(cache, item, size_px, wts, &mut shared, &mut flags, std::ptr::null_mut());
        ((**(item as *const *const UnknownVtbl)).release)(item);
        hr
    };
    if hr < 0 || shared.is_null() {
        // WTS_E_FAILEDEXTRACTION is every file with no handler or no picture (inside an archive, a
        // text file named .jpg), STG_E_FILENOTFOUND under WTS_INCACHEONLY every cloud-only file
        // not in the cache: the type icon, as before, without a line.
        let later = wts == WTS_EXTRACT && LATER_HRESULTS.contains(&hr);
        if hr != WTS_E_FAILEDEXTRACTION && !(wts == WTS_INCACHEONLY && hr == STG_E_FILENOTFOUND) {
            let then = if later { " (asked again later)" } else { "" };
            eprintln!("[icons] thumbnail {}: GetThumbnail = 0x{:08X}{then}", path.display(), hr as u32);
        }
        return if later { Thumbnail::Later } else { Thumbnail::None };
    }
    let mut hbm: HBITMAP = std::ptr::null_mut();
    // SAFETY: `shared` is a live ISharedBitmap; the bitmap it lends stays its own (read, never
    // deleted here) until the release, which happens exactly once.
    let pixels = unsafe {
        let vt = &**(shared as *const *const SharedBitmapVtbl);
        let p = if (vt.get_shared_bitmap)(shared, &mut hbm) >= 0 && !hbm.is_null() { hbitmap_pixels(hbm) } else { None };
        (vt.release)(shared);
        p
    };
    pixels.map_or(Thumbnail::None, |p| Thumbnail::Got((letterbox(&p, size_px), letterbox_rect(p.width(), p.height(), size_px))))
}

/// Release this thread's IThumbnailCache, if it made one.
fn release_thumbnail_cache() {
    let cache = THUMB_CACHE.with(|c| c.replace(std::ptr::null_mut()));
    if !cache.is_null() {
        // SAFETY: made on this thread by `thumbnail`, released exactly once.
        unsafe { ((**(cache as *const *const UnknownVtbl)).release)(cache) };
    }
}

/// What a worker renders for a request: for a thumbnail the shell's thumbnail (`thumbnail`;
/// from its cache only for `Thumb::CacheOnly`), else, or when there is none, the icon (`render`);
/// and whether the thumbnail is to be asked for again (the cache said not yet).
fn render_request(path: &Path, size_px: u32, thumb: Thumb) -> (Option<Rendered>, bool) {
    let flags = if thumb == Thumb::CacheOnly { WTS_INCACHEONLY } else { WTS_EXTRACT };
    let mut later = false;
    if thumb != Thumb::No {
        match thumbnail(path, size_px, flags) {
            Thumbnail::Got(t) => return (Some(t), false),
            Thumbnail::Later => later = true,
            Thumbnail::None => {}
        }
    }
    (render(path, size_px).map(|p| (p, None)), later)
}

/// A render's pixels and, for a thumbnail, where its picture lies in the square (`letterbox_rect`;
/// the tiles frame it, P6-M2); None for an icon.
type Rendered = (SharedPixelBuffer<Rgba8Pixel>, Option<[u32; 4]>);

/// Threads that render icons. Two, so one slow shell call (a network path, a dead share, a
/// placeholder the sync engine must answer) does not hold the icons queued behind it: a
/// synchronous shell call cannot be abandoned, so the other thread serves the queue meanwhile.
pub const ICON_WORKERS: usize = 2;

/// A render (overlay included) at least this long is logged as `[icons] slow icon`.
const SLOW_MS: u64 = 100;

/// How a worker turns a path (a size in px, thumbnail or not) into pixels: `render_request`, or a
/// stand-in in the tests.
type RenderFn = fn(&Path, u32, Thumb) -> (Option<Rendered>, bool);

/// What the worker threads share.
#[derive(Default)]
struct Pool {
    /// Requests not yet taken, served newest first, and whether the request channel has closed.
    queue: Mutex<(Vec<Request>, bool)>,
    ready: Condvar,
    /// Keys rendered or being rendered, per size in px: a key is taken when a thread pops it, so
    /// the two threads never render one key twice at one size. A failed key is given back (the
    /// UI's `failed` set is what stops it being asked for again).
    taken: Mutex<HashSet<(SharedString, u32)>>,
    /// Overlay images by (mark, size in px), fetched once per process (`mark_image`): the shell's
    /// overlays by index, and the sync roots' status glyphs.
    overlays: Mutex<HashMap<(Mark, u32), Option<SharedPixelBuffer<Rgba8Pixel>>>>,
}

/// A lock that a panic elsewhere cannot poison for good: the data is plain sets and queues.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Start the icon workers (`ICON_WORKERS` threads sharing one queue). They serve the queue
/// newest first, skip requests whose pane has navigated since (`generations`), render each key
/// at most once per process, draw the shell's overlay when asked, and wake the UI thread after
/// every reply.
pub fn spawn(
    rx: Receiver<Request>,
    tx: Sender<Reply>,
    generations: Arc<[AtomicU64; 2]>,
    views: Arc<Views>,
    stats: Arc<Stats>,
    wake: impl Fn() + Send + Sync + 'static,
) {
    spawn_with(rx, tx, generations, views, stats, wake, render_request);
}

/// `spawn` with the render function as a parameter, so a test can make one render slow.
fn spawn_with(
    rx: Receiver<Request>,
    tx: Sender<Reply>,
    generations: Arc<[AtomicU64; 2]>,
    views: Arc<Views>,
    stats: Arc<Stats>,
    wake: impl Fn() + Send + Sync + 'static,
    render: RenderFn,
) {
    let pool = Arc::new(Pool::default());
    // The feeder moves requests from the UI's channel into the shared queue and wakes one idle
    // worker. Workers that drained the channel themselves would leave the requests they drained
    // waiting behind their own slow render while the other thread slept in `recv`.
    let feed = pool.clone();
    std::thread::Builder::new()
        .name("icons-feed".into())
        .spawn(move || {
            for r in rx.iter() {
                lock(&feed.queue).0.push(r);
                feed.ready.notify_one();
            }
            lock(&feed.queue).1 = true;
            feed.ready.notify_all();
        })
        .expect("spawn icon feeder thread");
    let wake = Arc::new(wake);
    for i in 0..ICON_WORKERS {
        let (pool, tx, generations, views, stats, wake) =
            (pool.clone(), tx.clone(), generations.clone(), views.clone(), stats.clone(), wake.clone());
        std::thread::Builder::new()
            .name(format!("icons-{i}"))
            .spawn(move || work(&pool, &tx, &generations, &views, &stats, &*wake, render))
            .expect("spawn icon thread");
    }
}

/// The newest request, and whether the queue is empty after it; None once the channel closed.
fn next(pool: &Pool) -> Option<(Request, bool)> {
    let mut q = lock(&pool.queue);
    loop {
        if let Some(r) = q.0.pop() {
            let idle = q.0.is_empty();
            return Some((r, idle));
        }
        if q.1 {
            return None;
        }
        q = pool.ready.wait(q).unwrap_or_else(PoisonError::into_inner);
    }
}

/// One worker thread: COM (MTA) for itself, then requests until the channel closes.
fn work(pool: &Pool, tx: &Sender<Reply>, generations: &[AtomicU64; 2], views: &Views, stats: &Stats, wake: &dyn Fn(), render: RenderFn) {
    // SAFETY: initialises COM for this thread only; balanced by CoUninitialize below.
    let com = unsafe { CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32) };
    // Newest first: the latest requests are the rows on screen now, while older ones may be
    // rows a fast scroll passed over (still rendered, later, for the cache; a thumbnail for a
    // row the list has left is dropped).
    while let Some((r, idle)) = next(pool) {
        let stale = generations[r.pane].load(Ordering::Relaxed) != r.generation;
        // A drag image's thumbnail (P9-M4) is for the pressed row, wherever the list is.
        let passed = !stale && r.thumbnail && !is_drag_key(&r.key) && views.passed(r.pane, r.row, r.wanted);
        if passed {
            stats.dropped.fetch_add(1, Ordering::Relaxed);
        }
        let taken = (r.key.clone(), r.size_px);
        // A drag image's icon (`drag_key`) is rendered for every press that asks, never cached;
        // a lookup (`look:`) is made for every row that asks (a refresh asks again).
        let drag = is_drag_key(&r.key);
        let look = is_look_key(&r.key);
        if stale || passed || (!drag && !look && !lock(&pool.taken).insert(taken.clone())) {
            if tx.send(Reply { key: r.key, size_px: r.size_px, outcome: Outcome::Skipped, pane: r.pane, row: r.row }).is_err() {
                break;
            }
            if idle {
                wake(); // otherwise the next rendered reply wakes the UI for both
            }
            continue;
        }
        let t0 = Instant::now();
        if look {
            let mark = if com < 0 {
                Mark::None
            } else if r.key[LOOK_PREFIX.len()..].starts_with('s') {
                let state = storage_state(&r.path);
                // A file out of sync (P9-M4); a plain folder (`dir`) is looked up at every listing.
                if !r.key[LOOK_PREFIX.len()..].starts_with("s:dir|") {
                    eprintln!("[cloud] state read {}: {} in {} ms", r.path.display(), state.unwrap_or(0), t0.elapsed().as_millis());
                }
                state.map_or(Mark::None, Mark::State)
            } else {
                overlay_index(&r.path).map_or(Mark::None, |i| Mark::Overlay(i as u8))
            };
            stats.cloud.fetch_add(1, Ordering::Relaxed);
            let took = t0.elapsed().as_millis() as u64;
            if took >= SLOW_MS {
                stats.slow.fetch_add(1, Ordering::Relaxed);
                eprintln!("[icons] slow cloud lookup {took} ms: {}", r.path.display());
            }
            if tx.send(Reply { key: r.key, size_px: r.size_px, outcome: Outcome::Looked(mark), pane: r.pane, row: r.row }).is_err() {
                break;
            }
            wake();
            continue;
        }
        let thumb = match (r.thumbnail, r.mark.cache_only()) {
            (false, _) => Thumb::No,
            (true, false) => Thumb::Extract,
            (true, true) => Thumb::CacheOnly,
        };
        let (got, later) = if com >= 0 { render(&r.path, r.size_px, thumb) } else { (None, false) };
        let (mut pixels, picture) = match got {
            Some((p, picture)) => (Some(p), picture),
            None => (None, None),
        };
        let over = match (&pixels, r.mark) {
            (None, _) => None,
            (Some(_), Mark::None) if r.overlay => overlay_image(pool, &r.path, r.size_px),
            (Some(_), Mark::None) => None,
            (Some(_), m) => mark_image(pool, m, r.size_px),
        };
        if let (Some(p), Some(over)) = (&pixels, over) {
            pixels = Some(overlay(&fit(p, r.size_px), &over));
            stats.overlays.fetch_add(1, Ordering::Relaxed);
        }
        let took = t0.elapsed();
        let (n, ns) = if r.thumbnail { (&stats.thumbs, &stats.thumb_ns) } else { (&stats.rendered, &stats.render_ns) };
        n.fetch_add(1, Ordering::Relaxed);
        ns.fetch_add(took.as_nanos() as u64, Ordering::Relaxed);
        if took.as_millis() as u64 >= SLOW_MS {
            stats.slow.fetch_add(1, Ordering::Relaxed);
            let what = if r.thumbnail { "thumbnail" } else { "slow icon" };
            eprintln!("[icons] {what} {} ms: {}", took.as_millis(), r.path.display());
        }
        let outcome = match (pixels, picture) {
            (Some(p), Some(rect)) => Outcome::Picture(p, rect),
            // The type icon for now; the key is given back, so the UI's next ask renders it again.
            (Some(p), None) if later && !drag => {
                lock(&pool.taken).remove(&taken);
                Outcome::Later(p)
            }
            (Some(p), None) => Outcome::Pixels(p),
            (None, _) => {
                if !drag {
                    lock(&pool.taken).remove(&taken);
                }
                Outcome::Failed
            }
        };
        if tx.send(Reply { key: r.key, size_px: r.size_px, outcome, pane: r.pane, row: r.row }).is_err() {
            break;
        }
        wake();
    }
    release_thumbnail_cache();
    if com >= 0 {
        // SAFETY: pairs the successful CoInitializeEx above, on the same thread.
        unsafe { CoUninitialize() };
    }
}

/// IImageList (commoncontrols.h), every slot in declaration order; only the ones called are
/// typed. GetIcon is slot 10 and GetOverlayImage slot 31, counting IUnknown's three.
#[repr(C)]
struct ImageListVtbl {
    _query_interface: usize,
    _add_ref: usize,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    _add: usize,
    _replace_icon: usize,
    _set_overlay_image: usize,
    _replace: usize,
    _add_masked: usize,
    _draw: usize,
    _remove: usize,
    get_icon: unsafe extern "system" fn(*mut c_void, i32, u32, *mut HICON) -> HRESULT,
    _get_image_info: usize,
    _copy: usize,
    _merge: usize,
    _clone: usize,
    _get_image_rect: usize,
    get_icon_size: unsafe extern "system" fn(*mut c_void, *mut i32, *mut i32) -> HRESULT,
    _set_icon_size: usize,
    _get_image_count: usize,
    _set_image_count: usize,
    _set_bk_color: usize,
    _get_bk_color: usize,
    _begin_drag: usize,
    _end_drag: usize,
    _drag_enter: usize,
    _drag_leave: usize,
    _drag_move: usize,
    _set_drag_cursor_image: usize,
    _drag_show_nolock: usize,
    _get_drag_image: usize,
    _get_item_flags: usize,
    get_overlay_image: unsafe extern "system" fn(*mut c_void, i32, *mut i32) -> HRESULT,
}

const IID_IIMAGELIST: GUID = GUID::from_u128(0x46eb5926_582e_4017_9fdf_e8998daa0950);
/// ILD_TRANSPARENT (commctrl.h), declared here rather than enabling windows-sys's Win32_UI_Controls.
const ILD_TRANSPARENT: u32 = 1;

/// The shell's overlay for `path` drawn at `size_px`, or None when it has none. The overlay's
/// index costs one `SHGetFileInfoW` per call (the shell looks at the file); its image is fetched
/// once per (index, size) per process and shared by the worker threads.
fn overlay_image(pool: &Pool, path: &Path, size_px: u32) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let index = overlay_index(path)?;
    mark_image(pool, Mark::Overlay(index as u8), size_px)
}

/// A mark's image drawn at `size_px`, fetched once per (mark, size) per process and shared by the
/// worker threads: the shell's overlay at its index, or a sync root state's glyph.
fn mark_image(pool: &Pool, mark: Mark, size_px: u32) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let mut cache = lock(&pool.overlays);
    if let Some(o) = cache.get(&(mark, size_px)) {
        return o.clone();
    }
    let none = |o: &Option<SharedPixelBuffer<Rgba8Pixel>>| if o.is_some() { "" } else { ": no image" };
    let o = match mark {
        Mark::None => None,
        Mark::Overlay(index) => {
            let side = overlay_side(size_px);
            let o = overlay_pixels(index as i32, side).map(|o| if side == size_px { o } else { lower_left(&o, size_px) });
            let over = if side == size_px { String::new() } else { format!(" over {size_px} px") };
            eprintln!("[icons] overlay index {index} fetched at {side} px{over}{}", none(&o));
            o
        }
        Mark::State(state) => {
            let res = STATE_GLYPHS.iter().find(|(s, _)| *s == state).map(|&(_, r)| r);
            let o = res.and_then(|r| state_glyph(r, size_px));
            let g = glyph_px(size_px);
            eprintln!("[icons] status glyph state {state} (imageres -{}) fetched at {g} px over {size_px} px{}", res.unwrap_or(0), none(&o));
            o
        }
    };
    cache.insert((mark, size_px), o.clone());
    o
}

/// The square the shell's overlay image (Google Drive's mark, the link arrow) is drawn at over an
/// icon of `size_px` (P9-M4): the icon's own size up to 48 px, as Explorer draws it in its lists;
/// two thirds of it above, so a tile's mark covers less of the picture (Google Drive's glyph fills
/// 57% of its image's width, the arrow 39%).
pub fn overlay_side(size_px: u32) -> u32 {
    if size_px > 48 { size_px * 2 / 3 } else { size_px }
}

/// `p` at the lower left of a transparent `size` square (an overlay drawn smaller than its icon).
pub fn lower_left(p: &SharedPixelBuffer<Rgba8Pixel>, size: u32) -> SharedPixelBuffer<Rgba8Pixel> {
    let mut out = SharedPixelBuffer::<Rgba8Pixel>::new(size, size);
    let (w, h) = (p.width().min(size) as usize, p.height().min(size) as usize);
    let top = size as usize - h;
    let dst = out.make_mut_slice();
    for y in 0..h {
        let row = (top + y) * size as usize;
        dst[row..row + w].copy_from_slice(&p.as_slice()[y * p.width() as usize..y * p.width() as usize + w]);
    }
    out
}

/// A status glyph's size over an icon of `size_px`: about half a list icon (9 px over 16, 18 over
/// 32, as the shell's link arrow sits), a quarter of a tile's (24 px over 96).
pub fn glyph_px(size_px: u32) -> u32 {
    if size_px >= 64 { size_px / 4 } else { (size_px * 9).div_ceil(16) }
}

/// imageres.dll's icon `res` drawn `glyph_px` square in the bottom-left corner of a transparent
/// `size_px` square, where the shell's overlays sit; None when it cannot be loaded.
fn state_glyph(res: u16, size_px: u32) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let file: Vec<u16> = Path::new(&root).join(r"System32\imageres.dll").as_os_str().encode_wide().chain([0]).collect();
    let g = glyph_px(size_px);
    let mut hicon: HICON = std::ptr::null_mut();
    // SAFETY: `file` is NUL-terminated; a negative index names the resource id; the icon, when
    // made, is ours to destroy.
    let hr = unsafe { SHDefExtractIconW(file.as_ptr(), -(res as i32), 0, &mut hicon, std::ptr::null_mut(), g) };
    if hr != 0 || hicon.is_null() {
        return None;
    }
    let pixels = icon_pixels_at(hicon, size_px, 0, size_px - g, g);
    // SAFETY: made for us above.
    unsafe { DestroyIcon(hicon) };
    pixels
}

/// An icon location as the packaged menu commands' `GetIcon` and the Open with handlers give it:
/// `C:\x\app.exe,-101` (a negative index names a resource id), `C:\x\app.ico` (index 0), with `/`
/// allowed in the path. None for an empty one or an indirect `@` string.
pub fn parse_icon_location(s: &str) -> Option<(String, i32)> {
    let s = s.trim().trim_matches('"');
    if s.is_empty() || s.starts_with('@') {
        return None;
    }
    let (path, index) = match s.rsplit_once(',') {
        Some((p, i)) if i.trim().parse::<i32>().is_ok() => (p, i.trim().parse().unwrap_or(0)),
        _ => (s, 0),
    };
    let path = path.trim().trim_matches('"').replace('/', "\\");
    (!path.is_empty()).then_some((path, index))
}

/// The icon at an icon location (`parse_icon_location`) drawn `px` square; None when it cannot be
/// loaded. For the Windows 11 menu's packaged commands and Open with apps, on the verb worker.
pub(crate) fn location_pixels(location: &str, px: u32) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    // A packaged app's indirect string (`@{Package?ms-resource://...}`) names a picture in the
    // package, resolved for the display's scale; the shell draws it as its own thumbnail.
    if let Some(r) = location.trim().strip_prefix("@{").and_then(|r| r.split_once('}')).map(|(r, _)| format!("@{{{r}}}")) {
        use windows_sys::Win32::UI::Shell::SHLoadIndirectString;
        let src: Vec<u16> = std::ffi::OsStr::new(&r).encode_wide().chain([0]).collect();
        let mut buf = [0u16; 1024];
        // SAFETY: `src` is NUL-terminated; `buf` is writable for its length.
        if unsafe { SHLoadIndirectString(src.as_ptr(), buf.as_mut_ptr(), buf.len() as u32, std::ptr::null_mut()) } < 0 {
            return None;
        }
        let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
        let picture = render_with(Path::new(&String::from_utf16_lossy(&buf[..n])), px, SIIGBF_THUMBNAILONLY | SIIGBF_BIGGERSIZEOK)?;
        return Some(if picture.width() == px && picture.height() == px { picture } else { fit(&picture, px) });
    }
    let (path, index) = parse_icon_location(location)?;
    let file: Vec<u16> = std::ffi::OsStr::new(&path).encode_wide().chain([0]).collect();
    let mut hicon: HICON = std::ptr::null_mut();
    // SAFETY: `file` is NUL-terminated; the icon, when made, is ours to destroy.
    let hr = unsafe { SHDefExtractIconW(file.as_ptr(), index, 0, &mut hicon, std::ptr::null_mut(), px) };
    if hr != 0 || hicon.is_null() {
        return None;
    }
    let pixels = icon_pixels(hicon, px);
    // SAFETY: made for us above.
    unsafe { DestroyIcon(hicon) };
    pixels
}

#[repr(C)]
struct PropertyKey {
    fmtid: GUID,
    pid: u32,
}

/// PROPVARIANT's layout as far as a VT_UI4 needs it (24 bytes on x64).
#[repr(C)]
struct PropVariant {
    vt: u16,
    _reserved: [u16; 3],
    value: u64,
    _more: u64,
}

/// IPropertyStore (propsys.h): IUnknown, GetCount, GetAt, GetValue (the rest unused).
#[repr(C)]
struct PropertyStoreVtbl {
    _query_interface: usize,
    _add_ref: usize,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    _get_count: usize,
    _get_at: usize,
    get_value: unsafe extern "system" fn(*mut c_void, *const PropertyKey, *mut PropVariant) -> HRESULT,
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn SHGetPropertyStoreFromParsingName(path: *const u16, bc: *mut c_void, flags: i32, riid: *const GUID, out: *mut *mut c_void) -> HRESULT;
}

#[link(name = "ole32")]
unsafe extern "system" {
    fn PropVariantClear(v: *mut PropVariant) -> HRESULT;
}

const IID_IPROPERTYSTORE: GUID = GUID::from_u128(0x886d8eeb_8cf2_4446_8d02_cdba1dbdcf99);
/// PKEY_StorageProviderState: {E77E90DF-6271-4F5B-834F-2DD1F245DDA4}, 3.
const PKEY_STORAGE_PROVIDER_STATE: PropertyKey = PropertyKey { fmtid: GUID::from_u128(0xe77e90df_6271_4f5b_834f_2dd1f245dda4), pid: 3 };
const VT_UI4: u16 = 19;

/// A sync root entry's `System.StorageProviderState` (the value Explorer's Status column draws),
/// or None (no value, or 0). One property store read through the shell, about 23 ms each on
/// OneDrive (P6-M1 record), so only folders ask: a file's state is in its attributes.
fn storage_state(path: &Path) -> Option<u8> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut store: *mut c_void = std::ptr::null_mut();
    // SAFETY: `wide` is NUL-terminated and outlives the call; `store` receives an IPropertyStore
    // or stays null (GPS_DEFAULT: GPS_FASTPROPERTIESONLY fails for this property).
    let hr = unsafe { SHGetPropertyStoreFromParsingName(wide.as_ptr(), std::ptr::null_mut(), 0, &IID_IPROPERTYSTORE, &mut store) };
    if hr < 0 || store.is_null() {
        return None;
    }
    // SAFETY: `store` is a live IPropertyStore, released once; `v` is a writable PROPVARIANT,
    // cleared once.
    let state = unsafe {
        let vt = &**(store as *const *const PropertyStoreVtbl);
        let mut v: PropVariant = std::mem::zeroed();
        let ok = (vt.get_value)(store, &PKEY_STORAGE_PROVIDER_STATE, &mut v) >= 0 && v.vt == VT_UI4;
        let state = ok.then_some(v.value as u32);
        PropVariantClear(&mut v);
        (vt.release)(store);
        state
    };
    state.filter(|&s| s > 0 && s < 256).map(|s| s as u8)
}

/// The overlay index the shell gives `path` (the top eight bits of `iIcon` with
/// SHGFI_OVERLAYINDEX), or None for no overlay. The icon handle that comes with it is destroyed.
fn overlay_index(path: &Path) -> Option<i32> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: a zeroed SHFILEINFOW is valid; `wide` is NUL-terminated; the HICON, when one is
    // returned, is ours to destroy.
    let (ok, shfi) = unsafe {
        let mut shfi: SHFILEINFOW = std::mem::zeroed();
        let flags = SHGFI_ICON | SHGFI_SMALLICON | SHGFI_ADDOVERLAYS | SHGFI_OVERLAYINDEX;
        let ok = SHGetFileInfoW(wide.as_ptr(), 0, &mut shfi, std::mem::size_of::<SHFILEINFOW>() as u32, flags);
        if !shfi.hIcon.is_null() {
            DestroyIcon(shfi.hIcon);
        }
        (ok, shfi)
    };
    let index = (shfi.iIcon >> 24) & 0xFF;
    (ok != 0 && index != 0).then_some(index)
}

/// Overlay `index`'s image from the system image list, drawn at `size_px` square: from the
/// smallest list whose icons are at least that big (`DrawIconEx` scales).
fn overlay_pixels(index: i32, size_px: u32) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let list_of = |which: u32| {
        let mut list: *mut c_void = std::ptr::null_mut();
        // SAFETY: `list` receives an IImageList pointer or stays null.
        let hr = unsafe { SHGetImageList(which as i32, &IID_IIMAGELIST, &mut list) };
        (hr >= 0 && !list.is_null()).then_some(list)
    };
    // SAFETY: `list` is a live IImageList from SHGetImageList, whose vtable is `ImageListVtbl`.
    let vt = |list: *mut c_void| unsafe { &**(list as *const *const ImageListVtbl) };
    // The smallest of the small, large, extra-large and jumbo lists whose icons are at least
    // `size_px` (the grid asks for 96 px and more), else the biggest there is.
    let mut list = list_of(SHIL_SMALL)?;
    for which in [SHIL_LARGE, SHIL_EXTRALARGE, SHIL_JUMBO] {
        let (mut cx, mut cy) = (0, 0);
        // SAFETY: a live list; `cx` and `cy` are writable.
        if unsafe { (vt(list).get_icon_size)(list, &mut cx, &mut cy) } >= 0 && (cx as u32) >= size_px {
            break;
        }
        let Some(bigger) = list_of(which) else { break };
        // SAFETY: the smaller list is released once and replaced by the bigger one.
        unsafe { (vt(list).release)(list) };
        list = bigger;
    }
    let mut image = -1;
    let mut hicon: HICON = std::ptr::null_mut();
    // SAFETY: a live list; `image` and `hicon` are writable; the icon, if made, is ours.
    let ok = unsafe {
        (vt(list).get_overlay_image)(list, index, &mut image) >= 0
            && image >= 0
            && (vt(list).get_icon)(list, image, ILD_TRANSPARENT, &mut hicon) >= 0
    };
    // SAFETY: released exactly once; nothing uses it after this.
    unsafe { (vt(list).release)(list) };
    if !ok || hicon.is_null() {
        return None;
    }
    let pixels = icon_pixels(hicon, size_px);
    // SAFETY: GetIcon made this icon for us.
    unsafe { DestroyIcon(hicon) };
    pixels
}

/// An icon drawn at `px` square into a transparent 32-bit DIB: `DrawIconEx` alpha-blends an
/// alpha icon, which leaves premultiplied BGRA. None when nothing visible was drawn.
fn icon_pixels(hicon: HICON, px: u32) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    icon_pixels_at(hicon, px, 0, 0, px)
}

/// `icon_pixels` with the icon drawn `w` square at (`x`, `y`) of the `px` square.
fn icon_pixels_at(hicon: HICON, px: u32, x: u32, y: u32, w: u32) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    // SAFETY: zeroed BITMAPINFO is a valid starting point; the header fields are set below.
    let mut bmi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: px as i32,
        biHeight: -(px as i32), // negative: top-down rows
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..bmi.bmiHeader
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    // SAFETY: a memory DC and a DIB section of px * px 32-bit pixels, both freed before
    // returning; `bits` covers px * px * 4 bytes while the section lives.
    let bgra = unsafe {
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        let bmp = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        let mut out = None;
        if !bmp.is_null() && !bits.is_null() {
            let old = SelectObject(dc, bmp);
            if DrawIconEx(dc, x as i32, y as i32, hicon, w as i32, w as i32, 0, std::ptr::null_mut(), DI_NORMAL) != 0 {
                GdiFlush();
                out = Some(std::slice::from_raw_parts(bits as *const u8, (px * px * 4) as usize).to_vec());
            }
            SelectObject(dc, old);
        }
        if !bmp.is_null() {
            DeleteObject(bmp);
        }
        DeleteDC(dc);
        out
    }?;
    // All alpha zero would read as an opaque square in `bgra_to_rgba`: no overlay instead.
    if bgra.chunks_exact(4).all(|p| p[3] == 0) {
        return None;
    }
    Some(bgra_to_rgba(&bgra, px, px))
}

/// Thumbnails the table holds at most (each 32 x scale px square, 4 bytes a pixel: 4 KB at 100%,
/// 16 KB at 200%, so 80 to 320 MB at the cap); past it a file's thumbnail key falls back to its
/// extension's icon (`thumb_fallback`).
pub const THUMB_TABLE_CAP: usize = 20_000;

/// The extension key a thumbnail key falls back to past `THUMB_TABLE_CAP`. Thumbnail
/// extensions are never keyed by full path, so this is the key the file has with thumbnails off.
pub fn thumb_fallback(key: &str) -> SharedString {
    let rest = key.trim_start_matches("thumb:");
    let (mark, rest) = parse_tag(rest).unwrap_or((Mark::None, rest));
    let path = rest.rsplitn(3, '|').last().unwrap_or_default();
    match extension(path) {
        Some(e) => marked(&format!("ext:.{}", e.to_ascii_lowercase()), mark).into(),
        None => path.into(),
    }
}

/// What the table knows about a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyState {
    Ready(u16),
    Pending,
    Failed,
    Missing,
}

/// UI-thread icon table shared by both panes: pixel buffers by index (the first
/// `PLACEHOLDERS` are the placeholders), the key of every interned buffer, and the keys in
/// flight or failed, so each key is requested once however many rows show it. Keys are kept per
/// icon size in px (16 px rows' icons and 32 px rows' are different buffers): one `SizeKeys` per
/// size, looked up once per resolve, so a row's lookup borrows its key as before. `fitted` caches
/// each buffer resampled to the strip's icon size, for the size last asked for.
pub struct IconTable {
    bufs: Vec<SharedPixelBuffer<Rgba8Pixel>>,
    fitted: Vec<Option<SharedPixelBuffer<Rgba8Pixel>>>,
    fitted_px: u32,
    /// One entry per icon size asked for (one or two in practice), searched linearly.
    sizes: Vec<(u32, SizeKeys)>,
    cap: Cap,
    /// The picture's rectangle in each thumbnail's buffer (`Outcome::Picture`), for the tiles' frame.
    pictures: HashMap<u16, [u32; 4]>,
}

/// The keys the table knows at one icon size: interned (to their buffer's index), in flight, failed.
#[derive(Default)]
pub struct SizeKeys {
    index: HashMap<SharedString, u16>,
    pending: HashSet<SharedString>,
    failed: HashSet<SharedString>,
    /// Thumbnails answered with the type icon for now (`Outcome::Later`): when each is asked for
    /// again (None once it has been taken out of `index` and is asked), and the answers so far.
    later: HashMap<SharedString, (Option<Instant>, usize)>,
    /// The icons those keys showed when taken out of `index`: rows showing one are looked at by
    /// the resolve as placeholder rows are, so they ask again and keep the icon meanwhile.
    stale: HashSet<u16>,
}

/// After a thumbnail's first, second and third `Later` answer, the wait before it is asked again;
/// after the fourth the type icon stays for the session.
const LATER_WAITS: [Duration; 3] = [Duration::from_secs(2), Duration::from_secs(5), Duration::from_secs(10)];

impl SizeKeys {
    pub fn state(&self, key: &SharedString) -> KeyState {
        if let Some(&i) = self.index.get(key) {
            KeyState::Ready(i)
        } else if self.pending.contains(key) {
            KeyState::Pending
        } else if self.failed.contains(key) {
            KeyState::Failed
        } else {
            KeyState::Missing
        }
    }

    /// Mark a key as requested.
    pub fn begin(&mut self, key: &SharedString) {
        self.pending.insert(key.clone());
    }

    /// Whether a row showing icon `i` is to ask again: its key's thumbnail is due (`retry_due`).
    pub fn is_stale(&self, i: u16) -> bool {
        !self.stale.is_empty() && self.stale.contains(&i)
    }
}

/// Thumbnails interned (`thumb:` keys), against `THUMB_TABLE_CAP`, and whether the cap was logged.
#[derive(Default)]
pub struct Cap {
    thumbs: usize,
    logged: bool,
}

impl Cap {
    /// Past `THUMB_TABLE_CAP`, a thumbnail key not yet held becomes its extension's key (logged
    /// once); any other key is returned as it is.
    pub fn capped(&mut self, key: &SharedString) -> Option<SharedString> {
        if self.thumbs < THUMB_TABLE_CAP || !key.starts_with("thumb:") {
            return None;
        }
        if !self.logged {
            self.logged = true;
            eprintln!("[icons] thumbnail cap reached: {} thumbnails held, new ones get their type's icon", self.thumbs);
        }
        Some(thumb_fallback(key))
    }
}

impl Default for IconTable {
    fn default() -> Self {
        IconTable {
            bufs: vec![placeholder_file(16), placeholder_folder(16), placeholder_drive(16)],
            fitted: Vec::new(),
            fitted_px: 0,
            sizes: Vec::new(),
            cap: Cap::default(),
            pictures: HashMap::new(),
        }
    }
}

impl IconTable {
    /// The buffer at `i` as delivered (an out-of-range index reads as the file placeholder): the
    /// tests, and the drag image when the shell has no large icon for the item.
    pub fn buffer(&self, i: u16) -> &SharedPixelBuffer<Rgba8Pixel> {
        self.bufs.get(i as usize).unwrap_or(&self.bufs[PLACEHOLDER_FILE as usize])
    }

    /// The buffer at `i` resampled to `px` square (a placeholder is drawn at `px` instead),
    /// computed once per index and size (a refcount bump after that). A new size drops the cache.
    /// An out-of-range index reads as the file placeholder.
    pub fn fitted(&mut self, i: u16, px: u32) -> SharedPixelBuffer<Rgba8Pixel> {
        if self.fitted_px != px {
            self.fitted.clear();
            self.fitted_px = px;
        }
        let i = if (i as usize) < self.bufs.len() { i as usize } else { PLACEHOLDER_FILE as usize };
        if self.fitted.len() <= i {
            self.fitted.resize(self.bufs.len(), None);
        }
        self.fitted[i]
            .get_or_insert_with(|| if i < PLACEHOLDERS as usize { placeholder_at(i, px) } else { fit(&self.bufs[i], px) })
            .clone()
    }

    /// For a thumbnail at `i`, its picture's rectangle in the buffer resampled to `px` (`fitted`),
    /// which the tiles frame; None for an icon or a placeholder.
    pub fn picture(&self, i: u16, px: u32) -> Option<[u32; 4]> {
        let r = *self.pictures.get(&i)?;
        let from = self.bufs.get(i as usize)?.width();
        if from == px || from == 0 {
            return Some(r);
        }
        let s = |v: u32| ((v as u64 * px as u64 + from as u64 / 2) / from as u64) as u32;
        let (x, y) = (s(r[0]).min(px.saturating_sub(1)), s(r[1]).min(px.saturating_sub(1)));
        Some([x, y, s(r[2]).clamp(1, (px - x).max(1)), s(r[3]).clamp(1, (px - y).max(1))])
    }

    /// Buffers held, placeholders included.
    pub fn len(&self) -> usize {
        self.bufs.len()
    }

    /// The keys known at `px` and the thumbnail cap: what a resolve needs, taken once per call.
    pub fn keys(&mut self, px: u32) -> (&mut SizeKeys, &mut Cap) {
        (Self::at(&mut self.sizes, px), &mut self.cap)
    }

    fn at(sizes: &mut Vec<(u32, SizeKeys)>, px: u32) -> &mut SizeKeys {
        let i = match sizes.iter().position(|(p, _)| *p == px) {
            Some(i) => i,
            None => {
                sizes.push((px, SizeKeys::default()));
                sizes.len() - 1
            }
        };
        &mut sizes[i].1
    }

    /// Take every thumbnail whose wait has run out (`LATER_WAITS`) out of the keys, its icon marked
    /// stale, so the next resolve asks for it again from the rows that show that icon. Returns how
    /// many; the caller resolves the panes when any.
    pub fn retry_due(&mut self, now: Instant) -> usize {
        let mut n = 0;
        for (_, keys) in &mut self.sizes {
            let SizeKeys { index, later, stale, .. } = keys;
            for (key, (due, _)) in later.iter_mut() {
                if due.is_some_and(|d| d <= now) {
                    *due = None;
                    if let Some(i) = index.remove(key) {
                        stale.insert(i);
                        n += 1;
                    }
                }
            }
        }
        n
    }

    /// Apply a worker reply for `key` at `px`. Returns true when a new image was interned.
    pub fn complete(&mut self, key: SharedString, px: u32, outcome: Outcome) -> bool {
        let keys = Self::at(&mut self.sizes, px);
        keys.pending.remove(&key);
        let (p, picture) = match outcome {
            Outcome::Pixels(p) => {
                keys.later.remove(&key);
                (p, None)
            }
            Outcome::Picture(p, rect) => {
                keys.later.remove(&key);
                (p, Some(rect))
            }
            Outcome::Later(p) => {
                let tries = keys.later.get(&key).map_or(0, |l| l.1);
                match LATER_WAITS.get(tries) {
                    Some(wait) => {
                        keys.later.insert(key.clone(), (Some(Instant::now() + *wait), tries + 1));
                    }
                    None => {
                        keys.later.remove(&key);
                        eprintln!("[icons] thumbnail not ready after {} answers: {key} keeps its type icon", tries + 1);
                    }
                }
                (p, None)
            }
            Outcome::Failed => {
                keys.failed.insert(key);
                return false;
            }
            Outcome::Skipped | Outcome::Looked(_) => return false,
        };
        if keys.index.contains_key(&key) || self.bufs.len() >= u16::MAX as usize {
            return false;
        }
        self.cap.thumbs += usize::from(key.starts_with("thumb:"));
        let i = self.bufs.len() as u16;
        keys.index.insert(key, i);
        self.bufs.push(p);
        if let Some(rect) = picture {
            self.pictures.insert(i, rect);
        }
        true
    }
}

/// A `px` square opaque-on-transparent placeholder, drawn at that size rather than stretched from
/// 16 px (P4-M2: a 96 px tile showed six-pixel blocks before its icon landed). `rects` are (x0, y0,
/// x1, y1, inset, rgb) on a 16-unit grid, filled in order; `inset` pixels (at any size) are taken
/// off every edge, so a one-pixel outline at 16 px stays one pixel at 96.
fn placeholder(px: u32, rects: &[(u32, u32, u32, u32, u32, [u8; 3])]) -> SharedPixelBuffer<Rgba8Pixel> {
    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(px, px);
    let out = buf.make_mut_slice();
    let unit = |v: u32| (v as f32 * px as f32 / 16.0).round() as u32;
    for &(x0, y0, x1, y1, inset, [r, g, b]) in rects {
        let (x0, x1) = ((unit(x0) + inset).min(px), unit(x1).saturating_sub(inset).min(px));
        let (y0, y1) = ((unit(y0) + inset).min(px), unit(y1).saturating_sub(inset).min(px));
        for y in y0..y1 {
            for x in x0..x1 {
                out[(y * px + x) as usize] = Rgba8Pixel { r, g, b, a: 255 };
            }
        }
    }
    buf
}

fn placeholder_file(px: u32) -> SharedPixelBuffer<Rgba8Pixel> {
    placeholder(px, &[(3, 1, 13, 15, 0, [150, 150, 150]), (3, 1, 13, 15, 1, [245, 245, 245])])
}

fn placeholder_folder(px: u32) -> SharedPixelBuffer<Rgba8Pixel> {
    placeholder(px, &[(1, 3, 7, 5, 0, [214, 170, 60]), (1, 5, 15, 14, 0, [232, 190, 80])])
}

fn placeholder_drive(px: u32) -> SharedPixelBuffer<Rgba8Pixel> {
    placeholder(px, &[(1, 5, 15, 12, 0, [120, 120, 120]), (1, 5, 15, 12, 1, [200, 200, 200]), (11, 8, 13, 10, 0, [60, 160, 60])])
}

/// The placeholder at index `i` (one of `PLACEHOLDERS`) drawn at `px` square.
fn placeholder_at(i: usize, px: u32) -> SharedPixelBuffer<Rgba8Pixel> {
    match i as u16 {
        PLACEHOLDER_FOLDER => placeholder_folder(px),
        PLACEHOLDER_DRIVE => placeholder_drive(px),
        _ => placeholder_file(px),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    use std::time::Duration;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_ARCHIVE;

    #[test]
    fn key_rules() {
        let f = Path::new(r"C:\some\dir");
        let (file, dir) = (Kind::File, Kind::Directory);
        assert_eq!(icon_key(f, "a.TXT", 0, file, false), "ext:.txt");
        assert_eq!(icon_key(f, "archive.tar.gz", 0, file, false), "ext:.gz");
        assert_eq!(icon_key(f, ".gitignore", 0, file, false), "ext:.gitignore");
        for own in ["setup.exe", "Link.LNK", "app.ico", "site.url", "arrow.cur", "README", "trailing."] {
            assert_eq!(icon_key(f, own, 0, file, false), format!(r"C:\some\dir\{own}"), "{own}");
        }
        assert_eq!(icon_key(f, "sub", FILE_ATTRIBUTE_DIRECTORY, dir, false), "dir");
        assert_eq!(icon_key(f, "Docs", FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_READONLY, dir, true), r"C:\some\dir\Docs");
        let rdir = FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT;
        let rfile = FILE_ATTRIBUTE_REPARSE_POINT;
        assert_eq!(icon_key(f, "j", rdir, Kind::Junction, false), r"C:\some\dir\j");
        assert_eq!(icon_key(f, "l", rdir, Kind::Symlink, false), r"C:\some\dir\l");
        assert_eq!(icon_key(f, "l.txt", rfile, Kind::Symlink, false), r"C:\some\dir\l.txt");
        assert_eq!(icon_key(f, "x.txt", rfile, Kind::Reparse, false), r"C:\some\dir\x.txt");
        assert_eq!(icon_key(f, "wt.exe", rfile, Kind::AppAlias, false), r"C:\some\dir\wt.exe");
        // Cloud placeholders share their type's key, or `dir`, unless the icon is their own.
        assert_eq!(icon_key(f, "cloud.TXT", rfile, Kind::Cloud, false), "ext:.txt");
        assert_eq!(icon_key(f, "cloud.lnk", rfile, Kind::Cloud, false), r"C:\some\dir\cloud.lnk");
        assert_eq!(icon_key(f, "csub", rdir, Kind::Cloud, false), "dir");
        assert_eq!(icon_key(f, "Pictures", rdir | FILE_ATTRIBUTE_READONLY, Kind::Cloud, true), r"C:\some\dir\Pictures");
        assert_eq!(icon_key(Path::new(r"C:\"), "x.exe", 0, file, false), r"C:\x.exe");

        // The cached builder agrees with the pure rules and shares extension keys.
        let mut keys = IconKeys::new(f.to_path_buf(), false);
        let a = keys.for_entry("a.txt", 0, file, 0, 0);
        let b = keys.for_entry("b.txt", 0, file, 0, 0);
        assert_eq!((a.as_str(), b.as_str()), ("ext:.txt", "ext:.txt"));
        assert_eq!(keys.for_entry("B.TXT", 0, file, 0, 0), "ext:.txt");
        assert_eq!(keys.for_entry("sub", FILE_ATTRIBUTE_DIRECTORY, dir, 0, 0), "dir");
        assert_eq!(keys.for_entry("x.exe", 0, file, 0, 0), r"C:\some\dir\x.exe");
        // A read-only folder with no desktop.ini (this path does not exist) stays `dir`.
        assert_eq!(keys.for_entry("ro", FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_READONLY, dir, 0, 0), "dir");
        assert_eq!(keys.for_entry("cloud.txt", rfile, Kind::Cloud, 0, 0), "ext:.txt");
        assert_eq!(keys.for_entry("csub", rdir | FILE_ATTRIBUTE_READONLY, Kind::Cloud, 0, 0), "dir");
        assert_eq!(keys.for_entry("j", rdir, Kind::Junction, 0, 0), r"C:\some\dir\j");
        assert_eq!(keys.for_entry("l.txt", rfile, Kind::Symlink, 0, 0), r"C:\some\dir\l.txt");
        assert_eq!(keys.for_entry("Photo.JPG", 0, file, 1, 2), "ext:.jpg", "thumbnails off: the extension's key");

        // Thumbnails on: a plain file in THUMB_EXTS is keyed by its path, modified time and size.
        let mut thumbs = IconKeys::new(f.to_path_buf(), true);
        assert_eq!(thumbs.for_entry("Photo.JPG", 0, file, 1_700_000_000, 12345), r"thumb:C:\some\dir\Photo.JPG|1700000000|12345");
        assert_eq!(thumbs.for_entry("clip.mp4", 0, file, 5, 6), r"thumb:C:\some\dir\clip.mp4|5|6");
        assert_eq!(thumbs.for_entry("report.pdf", FILE_ATTRIBUTE_READONLY, file, 5, 6), r"thumb:C:\some\dir\report.pdf|5|6");
        let older = thumbs.for_entry("Photo.JPG", 0, file, 1_699_999_999, 12345);
        assert_ne!(older, thumbs.for_entry("Photo.JPG", 0, file, 1_700_000_000, 12345), "a changed file gets a new key");
        assert_eq!(thumbs.for_entry("a.txt", 0, file, 5, 6), "ext:.txt", "not a thumbnail type");
        assert_eq!(thumbs.for_entry("x.exe", 0, file, 5, 6), r"C:\some\dir\x.exe");
        assert_eq!(thumbs.for_entry("pics.jpg", FILE_ATTRIBUTE_DIRECTORY, dir, 5, 0), "dir", "a folder named .jpg");
        assert_eq!(thumbs.for_entry("x.jpg.lnk", 0, file, 5, 6), r"C:\some\dir\x.jpg.lnk", "a shortcut, not an image");
        // Never a thumbnail for a link or a cloud placeholder (that would download the file).
        assert_eq!(thumbs.for_entry("l.png", rfile, Kind::Symlink, 5, 6), r"C:\some\dir\l.png");
        assert_eq!(thumbs.for_entry("cloud.jpg", rfile, Kind::Cloud, 5, 6), "ext:.jpg");
        assert!(is_thumbnail("a.HEIC") && is_thumbnail("b.webm") && is_thumbnail("c.xlsx") && !is_thumbnail("jpg"));
    }

    #[test]
    fn custom_folder_needs_desktop_ini_and_ro_or_system() {
        let dir = std::env::temp_dir().join(format!("twinfm-icon-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("custom")).unwrap();
        std::fs::create_dir_all(dir.join("plain")).unwrap();
        std::fs::write(dir.join("custom").join("desktop.ini"), b"[.ShellClassInfo]\r\n").unwrap();
        let ro = FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_READONLY;
        assert!(custom_folder(&dir, "custom", ro));
        assert!(!custom_folder(&dir, "custom", FILE_ATTRIBUTE_DIRECTORY), "the shell ignores desktop.ini without RO/system");
        assert!(!custom_folder(&dir, "plain", ro));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn bgra_conversion_keeps_premultiplied_alpha() {
        // 2 x 2: opaque blue, half-transparent premultiplied red, transparent, opaque white.
        let bgra = [255, 0, 0, 255, 0, 0, 128, 128, 0, 0, 0, 0, 255, 255, 255, 255];
        let buf = bgra_to_rgba(&bgra, 2, 2);
        let px = buf.as_slice();
        assert_eq!((buf.width(), buf.height()), (2, 2));
        assert_eq!(px[0], Rgba8Pixel { r: 0, g: 0, b: 255, a: 255 });
        assert_eq!(px[1], Rgba8Pixel { r: 128, g: 0, b: 0, a: 128 });
        assert_eq!(px[2], Rgba8Pixel { r: 0, g: 0, b: 0, a: 0 });
        assert_eq!(px[3], Rgba8Pixel { r: 255, g: 255, b: 255, a: 255 });
        // No alpha channel at all (every alpha zero): treated as opaque.
        let flat = bgra_to_rgba(&[10, 20, 30, 0, 40, 50, 60, 0, 0, 0, 0, 0, 1, 2, 3, 0], 2, 2);
        assert_eq!(flat.as_slice()[0], Rgba8Pixel { r: 30, g: 20, b: 10, a: 255 });
        assert!(flat.as_slice().iter().all(|p| p.a == 255));
        // A channel above its alpha (the shell's 96 px folder icon has an edge row of
        // (251, 187, 23) at alpha 64, P4-M2) is clamped to it: blended as premultiplied, such a
        // pixel saturated into a pale lime fringe along the folder's top.
        let edge = bgra_to_rgba(&[23, 187, 251, 64, 0, 127, 255, 2], 2, 1);
        assert_eq!(edge.as_slice()[0], Rgba8Pixel { r: 64, g: 64, b: 23, a: 64 });
        assert_eq!(edge.as_slice()[1], Rgba8Pixel { r: 2, g: 2, b: 0, a: 2 });
    }

    #[test]
    fn visible_range_arithmetic() {
        assert_eq!(visible_range(0.0, 220.0, 0, 22.0), None);
        assert_eq!(visible_range(0.0, 220.0, 1000, 22.0), Some((0, 10)));
        assert_eq!(visible_range(-22.0, 220.0, 1000, 22.0), Some((1, 11)));
        assert_eq!(visible_range(-30.0, 100.0, 1000, 22.0), Some((1, 6))); // 30..130 px: rows 1..=5, ceil to 6
        assert_eq!(visible_range(0.0, 220.0, 5, 22.0), Some((0, 4)), "clamped to the last row");
        assert_eq!(visible_range(-100_000.0, 220.0, 5, 22.0), Some((4, 4)), "scrolled past the end");
        assert_eq!(visible_range(10.0, 44.0, 100, 22.0), Some((0, 2)), "overscroll at the top");
        // 32 px rows (thumbnails on).
        assert_eq!(visible_range(0.0, 220.0, 1000, 32.0), Some((0, 7)));
        assert_eq!(visible_range(-32.0, 220.0, 1000, 32.0), Some((1, 8)));
        assert_eq!(visible_range(-40.0, 100.0, 1000, 32.0), Some((1, 5))); // 40..140 px: rows 1..=4, ceil to 5
    }

    fn px() -> Outcome {
        Outcome::Pixels(SharedPixelBuffer::new(2, 2))
    }

    #[test]
    fn placeholders_are_drawn_at_the_requested_size() {
        // At 16 px the three placeholders are the pictures they have always been.
        let at = |b: &SharedPixelBuffer<Rgba8Pixel>, x: u32, y: u32| {
            let p = b.as_slice()[(y * b.width() + x) as usize];
            (p.r, p.g, p.b, p.a)
        };
        let f16 = placeholder_file(16);
        assert_eq!((f16.width(), f16.height()), (16, 16));
        assert_eq!(at(&f16, 0, 0), (0, 0, 0, 0), "transparent outside");
        assert_eq!(at(&f16, 3, 1), (150, 150, 150, 255), "the grey outline");
        assert_eq!(at(&f16, 4, 2), (245, 245, 245, 255), "the light paper, one px in");
        assert_eq!(at(&placeholder_folder(16), 1, 3), (214, 170, 60, 255));
        assert_eq!(at(&placeholder_drive(16), 11, 8), (60, 160, 60, 255));
        // At 96 px (the grid's tiles) the outline is still one pixel, not a six-pixel block, and the
        // shapes sit at the same proportions.
        let f96 = placeholder_file(96);
        assert_eq!((f96.width(), f96.height()), (96, 96));
        assert_eq!(at(&f96, 17, 5), (0, 0, 0, 0), "outside the paper");
        assert_eq!(at(&f96, 18, 6), (150, 150, 150, 255), "the outline's corner");
        assert_eq!(at(&f96, 19, 7), (245, 245, 245, 255), "paper one px inside the outline");
        assert_eq!(at(&f96, 77, 89), (150, 150, 150, 255), "the far corner of the outline");
        assert_eq!(at(&f96, 78, 90), (0, 0, 0, 0));
        let d96 = placeholder_drive(96);
        assert_eq!(at(&d96, 6, 30), (120, 120, 120, 255), "the drive's case");
        assert_eq!(at(&d96, 7, 31), (200, 200, 200, 255), "its face one px in");
        assert_eq!(at(&d96, 70, 50), (60, 160, 60, 255), "the light");
        // The table hands a placeholder out at the size asked for, and a real icon fitted.
        let mut t = IconTable::default();
        assert_eq!(t.fitted(PLACEHOLDER_FOLDER, 96).as_slice(), placeholder_folder(96).as_slice());
        assert_eq!(t.fitted(PLACEHOLDER_FILE, 32).as_slice(), placeholder_file(32).as_slice());
        assert_eq!(t.buffer(PLACEHOLDER_FILE).width(), 16, "the raw buffer stays 16 px (drag images)");
    }

    #[test]
    fn table_interns_once_and_tracks_pending_and_failed() {
        let mut t = IconTable::default();
        assert_eq!(t.len(), PLACEHOLDERS as usize);
        let txt: SharedString = "ext:.txt".into();
        assert_eq!(t.keys(16).0.state(&txt), KeyState::Missing);
        t.keys(16).0.begin(&txt);
        assert_eq!(t.keys(16).0.state(&txt), KeyState::Pending);
        assert_eq!(t.keys(32).0.state(&txt), KeyState::Missing, "pending at 16 px only");
        assert!(t.complete(txt.clone(), 16, px()));
        assert_eq!(t.keys(16).0.state(&txt), KeyState::Ready(PLACEHOLDERS));
        assert!(!t.complete(txt.clone(), 16, px()), "a duplicate reply does not intern twice");
        assert_eq!(t.keys(32).0.state(&txt), KeyState::Missing, "each size is its own entry");
        // The same key at 32 px: its own buffer; failing at one size leaves the other alone.
        t.keys(32).0.begin(&txt);
        assert!(t.complete(txt.clone(), 32, px()));
        assert_eq!(t.keys(32).0.state(&txt), KeyState::Ready(PLACEHOLDERS + 1));
        assert_eq!(t.keys(16).0.state(&txt), KeyState::Ready(PLACEHOLDERS));
        let half: SharedString = "ext:.half".into();
        assert!(!t.complete(half.clone(), 32, Outcome::Failed));
        assert_eq!((t.keys(32).0.state(&half), t.keys(16).0.state(&half)), (KeyState::Failed, KeyState::Missing));
        assert_eq!(t.len(), PLACEHOLDERS as usize + 2, "one buffer per size");
        let gone: SharedString = r"C:\gone.exe".into();
        t.keys(16).0.begin(&gone);
        assert!(!t.complete(gone.clone(), 16, Outcome::Failed));
        assert_eq!(t.keys(16).0.state(&gone), KeyState::Failed);
        let skip: SharedString = "dir".into();
        t.keys(16).0.begin(&skip);
        t.complete(skip.clone(), 16, Outcome::Skipped);
        assert_eq!(t.keys(16).0.state(&skip), KeyState::Missing, "a skipped key may be requested again");
        assert_eq!(t.buffer(PLACEHOLDERS).width(), 2);
        assert_eq!(t.buffer(PLACEHOLDER_FOLDER).width(), 16);
        // Fitted to the strip's icon size, once per size.
        assert_eq!(t.fitted(PLACEHOLDERS, 4).width(), 4);
        assert_eq!(t.fitted(PLACEHOLDER_FOLDER, 24).height(), 24);
        assert_eq!(t.fitted(PLACEHOLDER_FOLDER, 16).as_slice(), t.buffer(PLACEHOLDER_FOLDER).as_slice());
        assert_eq!(t.fitted(999, 16).as_slice(), t.buffer(PLACEHOLDER_FILE).as_slice(), "out of range reads as the file placeholder");
    }

    #[test]
    fn strip_layout_arithmetic() {
        let g1 = StripGeom::new(1.0, false);
        assert_eq!(g1.icon_px, 16);
        assert_eq!((g1.icon_y(0), g1.icon_y(1), g1.icon_y(10)), (3, 25, 223), "row top plus (22 - 16) / 2");
        assert_eq!((g1.height(0), g1.height(1), g1.height(50)), (0, 22, 1100));
        let g15 = StripGeom::new(1.5, false);
        assert_eq!(g15.icon_px, 24);
        assert_eq!((g15.icon_y(0), g15.icon_y(1), g15.icon_y(3)), (5, 38, 104)); // 4.5 rounds to 5
        assert_eq!(g15.height(3), 99);
        let g125 = StripGeom::new(1.25, false);
        assert_eq!(g125.icon_px, 20);
        assert_eq!(g125.height(3), 83); // 82.5, rounded once for the whole strip
        // Thumbnails on: 32 px icons filling 32 px rows.
        let t1 = StripGeom::new(1.0, true);
        assert_eq!(t1.icon_px, 32);
        assert_eq!((t1.icon_y(0), t1.icon_y(1), t1.icon_y(10)), (0, 32, 320), "row top plus (32 - 32) / 2");
        assert_eq!((t1.height(0), t1.height(1), t1.height(50)), (0, 32, 1600));
        let t15 = StripGeom::new(1.5, true);
        assert_eq!(t15.icon_px, 48);
        assert_eq!((t15.icon_y(0), t15.icon_y(1), t15.icon_y(3)), (0, 48, 144));
        assert_eq!(t15.height(3), 144);
        let t125 = StripGeom::new(1.25, true);
        assert_eq!(t125.icon_px, 40);
        assert_eq!((t125.icon_y(1), t125.height(3)), (40, 120));
        let t175 = StripGeom::new(1.75, true);
        assert_eq!((t175.icon_px, t175.icon_y(1), t175.height(3)), (56, 56, 168));
        assert_eq!(StripGeom::new(2.0, true).icon_px, 64);
        // Every icon lies inside its own row band at any of these scales, in both geometries.
        for thumbs in [false, true] {
            for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
                let g = StripGeom::new(scale, thumbs);
                for k in 0..200 {
                    assert!(g.icon_y(k) >= g.height(k), "scale {scale} thumbnails {thumbs} row {k}");
                    assert!(g.icon_y(k) + g.icon_px <= g.height(k + 1), "scale {scale} thumbnails {thumbs} row {k}");
                }
            }
        }
        assert_eq!((ListGeom::new(false).name_x(), ListGeom::new(true).name_x()), (28.0, 42.0), "the rows' padding-left");
        // Range: the visible rows plus the margin, clamped to the list.
        let m = STRIP_MARGIN_ROWS;
        assert_eq!(strip_range((10, 40), 1000), (10 - m, 40 + m));
        assert_eq!(strip_range((0, 30), 1000), (0, 30 + m));
        assert_eq!(strip_range((0, 4), 5), (0, 4), "list shorter than the viewport");
        assert_eq!(strip_range((990, 999), 1000), (990 - m, 999), "range at the end");
    }

    #[test]
    fn recompose_decision() {
        let c = Some(Composed { first: 6, last: 44, icon_px: 16 });
        assert!(needs_recompose(None, (0, 30), 100, 16, false), "nothing composed yet");
        assert!(!needs_recompose(c, (10, 40), 100, 16, false), "same range, no new key");
        assert!(!needs_recompose(c, (6, 36), 100, 16, false), "scrolled inside the margin");
        assert!(!needs_recompose(c, (14, 44), 100, 16, false), "scrolled inside the margin");
        assert!(needs_recompose(c, (5, 35), 100, 16, false), "one row above the strip");
        assert!(needs_recompose(c, (15, 45), 100, 16, false), "one row below the strip");
        assert!(needs_recompose(c, (10, 40), 100, 16, true), "an icon landed or the rows changed");
        assert!(needs_recompose(c, (10, 40), 100, 24, false), "scale changed");
        assert!(needs_recompose(c, (10, 40), 44, 16, false), "list shrank under the strip");
        assert!(needs_recompose(c, (10, 50), 100, 16, false), "viewport grew");
        assert!(!needs_recompose(c, (10, 20), 100, 16, false), "viewport shrank");
    }

    #[test]
    fn compose_places_icons_at_row_offsets() {
        let solid = |v: u8| {
            let mut b = SharedPixelBuffer::<Rgba8Pixel>::new(2, 2);
            for (i, p) in b.make_mut_slice().iter_mut().enumerate() {
                *p = Rgba8Pixel { r: v, g: i as u8, b: 0, a: 255 };
            }
            b
        };
        let mut t = IconTable::default();
        t.complete("a".into(), 2, Outcome::Pixels(solid(10)));
        t.complete("b".into(), 2, Outcome::Pixels(solid(20)));
        // Three rows: key a, a row with no icon yet (the file placeholder), key b.
        let g = StripGeom { icon_px: 2, scale: 1.0, list: ListGeom::new(false) };
        let icons: Vec<_> = [PLACEHOLDERS, PLACEHOLDER_FILE, PLACEHOLDERS + 1].iter().map(|&i| t.fitted(i, 2)).collect();
        let strip = compose(&icons, &[], &g);
        assert_eq!((strip.width(), strip.height()), (2, 66));
        let px = strip.as_slice();
        let at = |x: usize, y: usize| px[y * 2 + x];
        // Row 0 at y 3..5 and row 2 at y 47..49, scanlines in order.
        assert_eq!(at(0, 3), Rgba8Pixel { r: 10, g: 0, b: 0, a: 255 });
        assert_eq!(at(1, 3), Rgba8Pixel { r: 10, g: 1, b: 0, a: 255 });
        assert_eq!(at(0, 4), Rgba8Pixel { r: 10, g: 2, b: 0, a: 255 });
        assert_eq!(at(1, 4), Rgba8Pixel { r: 10, g: 3, b: 0, a: 255 });
        assert_eq!(at(0, 47), Rgba8Pixel { r: 20, g: 0, b: 0, a: 255 });
        assert_eq!(at(1, 48), Rgba8Pixel { r: 20, g: 3, b: 0, a: 255 });
        // Row 1 at y 25..27 holds the file placeholder fitted to 2 x 2.
        assert_eq!(&px[25 * 2..27 * 2], t.fitted(PLACEHOLDER_FILE, 2).as_slice());
        assert!(at(0, 25).a > 0, "the placeholder is visible");
        // Everything else is transparent.
        let filled: Vec<usize> = (0..66).filter(|&y| at(0, y).a != 0 || at(1, y).a != 0).collect();
        assert_eq!(filled, [3, 4, 25, 26, 47, 48]);
        // An empty strip, and an icon of the wrong size left out.
        assert_eq!(compose(&[], &[], &g).height(), 0);
        assert!(compose(&[solid(1)], &[], &StripGeom { icon_px: 4, scale: 1.0, ..g }).as_slice().iter().all(|p| p.a == 0));
        // 32 px rows with 32 px icons: each icon at its row's top, rows 32 px apart.
        let g32 = StripGeom { icon_px: 2, scale: 1.0, list: ListGeom::new(true) };
        let strip = compose(&icons, &[], &g32);
        assert_eq!((strip.width(), strip.height()), (2, 96));
        let px = strip.as_slice();
        let at = |x: usize, y: usize| px[y * 2 + x];
        assert_eq!(at(0, 0), Rgba8Pixel { r: 10, g: 0, b: 0, a: 255 });
        assert_eq!(at(1, 1), Rgba8Pixel { r: 10, g: 3, b: 0, a: 255 });
        assert_eq!(&px[32 * 2..34 * 2], t.fitted(PLACEHOLDER_FILE, 2).as_slice());
        assert_eq!(at(0, 64), Rgba8Pixel { r: 20, g: 0, b: 0, a: 255 });
        let filled: Vec<usize> = (0..96).filter(|&y| at(0, y).a != 0 || at(1, y).a != 0).collect();
        assert_eq!(filled, [0, 1, 32, 33, 64, 65]);
    }

    #[test]
    fn grid_compose_places_icons_in_their_tiles() {
        for (scale, tile) in [1.0f32, 1.25, 1.5, 2.0].into_iter().flat_map(|s| [(s, TileGeom::LARGE), (s, TileGeom::MEDIUM)]) {
            let g = GridGeom::new(scale, 3, tile);
            assert_eq!(g.icon_px, (tile.icon_px * scale).round() as u32);
            assert_eq!(g.width(), (3.0 * tile.w * scale).round() as u32);
            // Every icon lies inside its own tile: column band in x, grid-row band in y.
            for k in 0..30 {
                let (r, c) = (k / 3, k % 3);
                let (x, y) = g.icon_pos(k);
                let tile_x0 = (c as f32 * tile.w * scale).round() as u32;
                let tile_x1 = ((c + 1) as f32 * tile.w * scale).round() as u32;
                assert!(x >= tile_x0 && x + g.icon_px <= tile_x1 + 1, "scale {scale} entry {k} x {x}");
                assert!(y >= g.height(r) && y + g.icon_px <= g.height(r + 1), "scale {scale} entry {k} y {y}");
            }
            // Compose five entries over two grid rows (the last partial): each icon's pixels land
            // at its position, nothing else is drawn.
            let n = g.icon_px;
            let solid = |v: u8| {
                let mut b = SharedPixelBuffer::<Rgba8Pixel>::new(n, n);
                b.make_mut_slice().iter_mut().for_each(|p| *p = Rgba8Pixel { r: v, g: 0, b: 0, a: 255 });
                b
            };
            let icons: Vec<_> = (1..=5).map(|v| solid(v * 10)).collect();
            let strip = compose_grid(&icons, &[], &[], &g, 2);
            assert_eq!((strip.width(), strip.height()), (g.width(), g.height(2)));
            let w = strip.width() as usize;
            let px = strip.as_slice();
            let mut drawn = 0;
            for (k, v) in (1..=5u8).enumerate() {
                let (x, y) = g.icon_pos(k);
                for (dx, dy) in [(0, 0), (n - 1, 0), (0, n - 1), (n - 1, n - 1)] {
                    let p = px[(y + dy) as usize * w + (x + dx) as usize];
                    assert_eq!(p.r, v * 10, "scale {scale} entry {k} corner ({dx}, {dy})");
                }
                drawn += (n * n) as usize;
            }
            assert_eq!(px.iter().filter(|p| p.a != 0).count(), drawn, "scale {scale}: only the icons are drawn");
            // A wrong-sized icon is left out.
            assert!(compose_grid(&[solid(1), SharedPixelBuffer::new(2, 2)], &[], &[], &g, 1).as_slice().iter().filter(|p| p.a != 0).count() == (n * n) as usize);
        }
        // Grid strip range: visible grid rows plus the margin, clamped.
        let m = GRID_MARGIN_ROWS;
        assert_eq!(grid_strip_range((10, 18), 1000), (10 - m, 18 + m));
        assert_eq!(grid_strip_range((0, 3), 4), (0, 3));
        assert_eq!(visible_range(-136.0, 600.0, 1000, TileGeom::LARGE.h), Some((1, 6)));
        assert_eq!(visible_range(-214.0, 600.0, 1000, TileGeom::MEDIUM.h), Some((2, 8)));
        assert_eq!(GridGeom::new(1.0, 0, TileGeom::LARGE).cols, 1, "no width yet reads as one column");
        // The medium tile: a 48 px icon centred in an 80 px tile, 2 px from its top.
        let m = GridGeom::new(1.0, 4, TileGeom::MEDIUM);
        assert_eq!((m.icon_px, m.icon_pos(0), m.icon_pos(5), m.width(), m.height(2)), (48, (16, 2), (96, 109), 320, 214));
        assert_eq!(TileGeom::of(true), TileGeom::MEDIUM);
        assert_eq!(TileGeom::of(false), TileGeom::LARGE);
    }

    #[test]
    fn tiles_frame_thumbnails_and_never_icons() {
        let g = GridGeom::new(1.0, 2, TileGeom::MEDIUM);
        let n = g.icon_px;
        let solid = |v: u8| {
            let mut b = SharedPixelBuffer::<Rgba8Pixel>::new(n, n);
            b.make_mut_slice().iter_mut().for_each(|p| *p = Rgba8Pixel { r: v, g: v, b: v, a: 255 });
            b
        };
        // Entry 0 a wide picture letterboxed in its square (48 x 24 at y 12), entry 1 an icon.
        let rect = letterbox_rect(200, 100, n).unwrap();
        assert_eq!(rect, [0, 12, 48, 24]);
        let strip = compose_grid(&[solid(10), solid(20)], &[], &[Some(rect), None], &g, 1);
        let w = strip.width() as usize;
        let px = strip.as_slice();
        let at = |k: usize, x: u32, y: u32| {
            let (x0, y0) = g.icon_pos(k);
            px[(y0 + y) as usize * w + (x0 + x) as usize]
        };
        for (x, y) in [(0, 12), (47, 12), (0, 35), (47, 35), (20, 12), (20, 35), (0, 20), (47, 20)] {
            assert_eq!(at(0, x, y), FRAME, "the picture's edge at ({x}, {y})");
        }
        for (x, y) in [(1, 13), (46, 34), (20, 20), (0, 11), (0, 36)] {
            assert_eq!(at(0, x, y).r, 10, "inside the frame or outside the picture ({x}, {y})");
        }
        assert!((0..n).all(|y| (0..n).all(|x| at(1, x, y).r == 20)), "an icon gets no frame");
        // At 200% the frame is two physical pixels; a cut thumbnail's frame is dimmed with it.
        let g2 = GridGeom::new(2.0, 1, TileGeom::MEDIUM);
        let big = SharedPixelBuffer::<Rgba8Pixel>::new(g2.icon_px, g2.icon_px);
        let r2 = [0, 0, g2.icon_px, g2.icon_px];
        let s2 = compose_grid(&[big], &[true], &[Some(r2)], &g2, 1);
        let (x0, y0) = g2.icon_pos(0);
        let w2 = s2.width() as usize;
        let p2 = |x: u32, y: u32| s2.as_slice()[(y0 + y) as usize * w2 + (x0 + x) as usize];
        let mut dimmed = [FRAME];
        dim_line(&mut dimmed);
        assert_eq!((p2(0, 50), p2(1, 50), p2(2, 50)), (dimmed[0], dimmed[0], Rgba8Pixel::default()));
        // The rectangle follows a buffer resampled to another size (`IconTable::picture`).
        let mut t = IconTable::default();
        assert!(t.complete("thumb:a".into(), 96, Outcome::Picture(SharedPixelBuffer::new(96, 96), [0, 24, 96, 48])));
        assert!(t.complete("ext:.txt".into(), 96, Outcome::Pixels(SharedPixelBuffer::new(96, 96))));
        let (a, b) = (PLACEHOLDERS, PLACEHOLDERS + 1);
        assert_eq!(t.picture(a, 96), Some([0, 24, 96, 48]));
        assert_eq!(t.picture(a, 48), Some([0, 12, 48, 24]));
        assert_eq!(t.picture(b, 96), None, "an icon has no picture");
        assert_eq!(t.picture(PLACEHOLDER_FILE, 48), None);
    }

    #[test]
    fn dim_line_scales_every_channel() {
        let p = |r, g, b, a| Rgba8Pixel { r, g, b, a };
        let mut line = [p(255, 255, 255, 255), p(0, 0, 0, 0), p(100, 20, 1, 200), p(2, 3, 128, 255)];
        dim_line(&mut line);
        // 45% rounded half up: 255 -> 115, 100 -> 45, 20 -> 9, 1 -> 0, 200 -> 90, 2 -> 1, 3 -> 1, 128 -> 58.
        assert_eq!(line, [p(115, 115, 115, 115), p(0, 0, 0, 0), p(45, 9, 0, 90), p(1, 1, 58, 115)]);
    }

    #[test]
    fn compose_dims_cut_entries_only() {
        let solid = |n: u32, v: u8| {
            let mut b = SharedPixelBuffer::<Rgba8Pixel>::new(n, n);
            b.make_mut_slice().iter_mut().for_each(|p| *p = Rgba8Pixel { r: v, g: v, b: v, a: 255 });
            b
        };
        let full = Rgba8Pixel { r: 200, g: 200, b: 200, a: 255 };
        let dimmed = Rgba8Pixel { r: 90, g: 90, b: 90, a: 115 };
        // The strip: entry 1 of three dimmed; a `dim` shorter than the icons leaves entry 2 alone.
        let g = StripGeom { icon_px: 2, scale: 1.0, list: ListGeom::new(false) };
        let icons = vec![solid(2, 200); 3];
        let strip = compose(&icons, &[false, true], &g);
        let px = strip.as_slice();
        let rows = |k: usize| {
            let y = g.icon_y(k) as usize;
            &px[y * 2..(y + 2) * 2]
        };
        assert!(rows(0).iter().all(|&p| p == full));
        assert!(rows(1).iter().all(|&p| p == dimmed));
        assert!(rows(2).iter().all(|&p| p == full));
        assert_eq!(px.iter().filter(|p| p.a != 0).count(), 12, "nothing else drawn");
        // The grid: entry 0 of two dimmed, entry 1 not.
        let g = GridGeom::new(1.0, 2, TileGeom::LARGE);
        let n = g.icon_px;
        let icons = vec![solid(n, 200); 2];
        let strip = compose_grid(&icons, &[true, false], &[], &g, 1);
        let w = strip.width() as usize;
        let px = strip.as_slice();
        for (k, want) in [(0, dimmed), (1, full)] {
            let (x, y) = g.icon_pos(k);
            for dy in 0..n {
                let at = (y + dy) as usize * w + x as usize;
                assert!(px[at..at + n as usize].iter().all(|&p| p == want), "entry {k} line {dy}");
            }
        }
        assert_eq!(px.iter().filter(|p| p.a != 0).count(), 2 * (n * n) as usize);
    }

    #[test]
    fn fit_box_filters_premultiplied() {
        // 4 x 4 of two colours in 2 x 2 blocks, down to 2 x 2: one pixel per block.
        let red = Rgba8Pixel { r: 200, g: 0, b: 0, a: 255 };
        let blue = Rgba8Pixel { r: 0, g: 0, b: 100, a: 100 };
        let mut b = SharedPixelBuffer::<Rgba8Pixel>::new(4, 4);
        for (i, p) in b.make_mut_slice().iter_mut().enumerate() {
            *p = if (i % 4 < 2) == (i / 4 < 2) { red } else { blue };
        }
        let f = fit(&b, 2);
        assert_eq!(f.as_slice(), [red, blue, blue, red]);
        // Down to 1 x 1: the average of all sixteen.
        assert_eq!(fit(&b, 1).as_slice()[0], Rgba8Pixel { r: 100, g: 0, b: 50, a: 177 });
        // Up 2 to 3: every target pixel takes at least one source pixel.
        let up = fit(&f, 3);
        assert_eq!(up.as_slice().len(), 9);
        assert!(up.as_slice().iter().all(|p| p.a != 0));
    }

    #[test]
    fn letterbox_keeps_the_aspect_with_transparent_bands() {
        let p = |r: u8| Rgba8Pixel { r, g: 0, b: 0, a: 255 };
        let buf = |w: u32, h: u32, px: &[Rgba8Pixel]| {
            let mut b = SharedPixelBuffer::<Rgba8Pixel>::new(w, h);
            b.make_mut_slice().copy_from_slice(px);
            b
        };
        let alpha = |b: &SharedPixelBuffer<Rgba8Pixel>| b.as_slice().iter().map(|q| q.a).collect::<Vec<u8>>();
        // 2 x 1 into 4 x 4: rows 1 and 2, each source pixel two wide; rows 0 and 3 transparent.
        let wide = letterbox(&buf(2, 1, &[p(10), p(20)]), 4);
        assert_eq!((wide.width(), wide.height()), (4, 4));
        assert_eq!(alpha(&wide), [0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0]);
        assert_eq!(&wide.as_slice()[4..8], [p(10), p(10), p(20), p(20)]);
        assert_eq!(&wide.as_slice()[8..12], [p(10), p(10), p(20), p(20)]);
        // 1 x 2 into 4 x 4: columns 1 and 2, the top half from the first pixel.
        let tall = letterbox(&buf(1, 2, &[p(10), p(20)]), 4);
        assert_eq!(alpha(&tall), [0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0]);
        assert_eq!((tall.as_slice()[1], tall.as_slice()[5], tall.as_slice()[9], tall.as_slice()[14]), (p(10), p(10), p(20), p(20)));
        // A square of the size already: unchanged.
        assert_eq!(letterbox(&wide, 4).as_slice(), wide.as_slice());
        // 400 x 300 down to 32: 32 x 24, rows 4 to 27.
        let big = letterbox(&buf(400, 300, &vec![p(50); 120_000]), 32);
        let rows: Vec<usize> = (0..32).filter(|&y| big.as_slice()[y * 32 + 16].a != 0).collect();
        assert_eq!(rows, (4..28).collect::<Vec<_>>());
        assert!(big.as_slice()[4 * 32..28 * 32].iter().all(|&q| q == p(50)));
    }

    #[test]
    fn overlay_blends_source_over_premultiplied() {
        let buf = |px: [Rgba8Pixel; 4]| {
            let mut b = SharedPixelBuffer::<Rgba8Pixel>::new(2, 2);
            b.make_mut_slice().copy_from_slice(&px);
            b
        };
        let p = |r, g, b, a| Rgba8Pixel { r, g, b, a };
        let clear = p(0, 0, 0, 0);
        // Base: opaque red, opaque red, half-transparent green (premultiplied), transparent.
        let base = buf([p(255, 0, 0, 255), p(255, 0, 0, 255), p(0, 128, 0, 128), clear]);
        // Overlay: nothing, opaque white, half-transparent blue (premultiplied), opaque black.
        let over = buf([clear, p(255, 255, 255, 255), p(0, 0, 128, 128), p(0, 0, 0, 255)]);
        let out = overlay(&base, &over);
        assert_eq!(out.as_slice()[0], p(255, 0, 0, 255), "a transparent overlay pixel keeps the base");
        assert_eq!(out.as_slice()[1], p(255, 255, 255, 255), "an opaque overlay pixel replaces it");
        // 128 + 128 * 127 / 255 = 128 + 64 (rounded): half blue over half green.
        assert_eq!(out.as_slice()[2], p(0, 64, 128, 192));
        assert_eq!(out.as_slice()[3], p(0, 0, 0, 255), "over a transparent base the overlay shows as is");
        assert_eq!(base.as_slice()[1], p(255, 0, 0, 255), "the base buffer is not changed");
        // Different sizes: the base comes back unchanged.
        assert_eq!(overlay(&base, &SharedPixelBuffer::new(4, 4)).as_slice(), base.as_slice());
    }

    #[test]
    fn overlays_over_tiles_are_two_thirds_at_the_lower_left() {
        // P9-M4: up to 48 px (the lists, and medium tiles at 100%) the shell's overlay as Explorer
        // draws it; above, two thirds of the icon (64 over a 96 px tile, 96 over 144 at 150%).
        assert_eq!((overlay_side(16), overlay_side(32), overlay_side(48)), (16, 32, 48));
        assert_eq!((overlay_side(72), overlay_side(96), overlay_side(144)), (48, 64, 96));
        let p = |v| Rgba8Pixel { r: v, g: v, b: v, a: v };
        let mut small = SharedPixelBuffer::<Rgba8Pixel>::new(2, 2);
        small.make_mut_slice().copy_from_slice(&[p(1), p(2), p(3), p(4)]);
        let placed = lower_left(&small, 3);
        let clear = Rgba8Pixel { r: 0, g: 0, b: 0, a: 0 };
        assert_eq!((placed.width(), placed.height()), (3, 3));
        assert_eq!(placed.as_slice(), [clear, clear, clear, p(1), p(2), clear, p(3), p(4), clear]);
    }

    #[test]
    fn overlay_only_for_links_and_reparse_points() {
        let rdir = FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT;
        assert!(wants_overlay(Kind::File, 0, "Notes.LNK"));
        assert!(wants_overlay(Kind::File, 0, "site.url"));
        assert!(wants_overlay(Kind::Cloud, FILE_ATTRIBUTE_REPARSE_POINT, "synced.lnk"));
        assert!(wants_overlay(Kind::Junction, rdir, "Application Data"));
        assert!(wants_overlay(Kind::Symlink, FILE_ATTRIBUTE_REPARSE_POINT, "link.txt"));
        assert!(wants_overlay(Kind::AppAlias, FILE_ATTRIBUTE_REPARSE_POINT, "wt.exe"));
        assert!(wants_overlay(Kind::Reparse, FILE_ATTRIBUTE_REPARSE_POINT, "x.dat"));
        assert!(!wants_overlay(Kind::File, 0, "a.txt"));
        assert!(!wants_overlay(Kind::File, 0, "setup.exe"));
        assert!(!wants_overlay(Kind::Directory, FILE_ATTRIBUTE_DIRECTORY, "odd.lnk"), "keyed `dir`, shared");
        assert!(!wants_overlay(Kind::Cloud, rdir, "csub"));
        assert!(!wants_overlay(Kind::Cloud, FILE_ATTRIBUTE_REPARSE_POINT, "doc.docx"));
        assert!(!wants_overlay(Kind::Drive, FILE_ATTRIBUTE_DIRECTORY, r"C:\"));
    }

    fn req(generation: u64, key: &str, path: &str) -> Request {
        Request { generation, pane: 0, key: key.into(), path: PathBuf::from(path), size_px: 1, overlay: false, thumbnail: false, row: 0, wanted: 0, mark: Mark::None }
    }

    /// A stand-in render: "slow" takes 2 s, "busy" 50 ms (counted in BUSY_RENDERS), others none.
    fn fake_render(path: &Path, _size_px: u32, thumb: Thumb) -> (Option<Rendered>, bool) {
        if path.to_str().is_some_and(|p| p.starts_with("thumbmode")) {
            lock(&THUMB_MODES).push((path.to_string_lossy().into_owned(), thumb));
        }
        match path.to_str() {
            Some("slow") => std::thread::sleep(Duration::from_secs(2)),
            Some("busy") => {
                BUSY_RENDERS.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(50));
            }
            Some("fail") => return (None, false),
            Some("picture") => return (Some((SharedPixelBuffer::new(1, 1), Some([0, 0, 1, 1]))), false),
            Some("later") => return (Some((SharedPixelBuffer::new(1, 1), None)), true),
            _ => {}
        }
        (Some((SharedPixelBuffer::new(1, 1), None)), false)
    }

    static BUSY_RENDERS: AtomicU64 = AtomicU64::new(0);
    static THUMB_MODES: Mutex<Vec<(String, Thumb)>> = Mutex::new(Vec::new());

    #[test]
    fn pool_serves_others_while_one_icon_is_slow() {
        let (req_tx, req_rx) = unbounded();
        let (rep_tx, rep_rx) = unbounded();
        let gens = Arc::new([AtomicU64::new(1), AtomicU64::new(0)]);
        let stats = Arc::new(Stats::default());
        spawn_with(req_rx, rep_tx, gens.clone(), Arc::default(), stats.clone(), || {}, fake_render);

        // One slow render takes a thread; three requests sent after it go to the other one.
        req_tx.send(req(1, "slow", "slow")).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let t0 = Instant::now();
        for k in ["a", "b", "c"] {
            req_tx.send(req(1, k, k)).unwrap();
        }
        let mut fast = HashSet::new();
        while fast.len() < 3 {
            let r = rep_rx.recv_timeout(Duration::from_millis(500)).expect("the others reply within 500 ms");
            assert!(matches!(r.outcome, Outcome::Pixels(_)), "{} rendered", r.key);
            fast.insert(r.key.to_string());
        }
        assert!(t0.elapsed() < Duration::from_millis(500), "{:?}", t0.elapsed());
        assert_eq!(fast, HashSet::from(["a".to_string(), "b".to_string(), "c".to_string()]));

        // A stale generation is skipped, not rendered, even while a thread is busy.
        req_tx.send(req(9, "stale", "x")).unwrap();
        let r = rep_rx.recv_timeout(Duration::from_millis(500)).expect("stale reply");
        assert_eq!(r.key, "stale");
        assert!(matches!(r.outcome, Outcome::Skipped));

        // A failed key is given back, so a later request for it renders (and fails) again.
        req_tx.send(req(1, "f", "fail")).unwrap();
        assert!(matches!(rep_rx.recv_timeout(Duration::from_millis(500)).unwrap().outcome, Outcome::Failed));
        req_tx.send(req(1, "f", "fail")).unwrap();
        assert!(matches!(rep_rx.recv_timeout(Duration::from_millis(500)).unwrap().outcome, Outcome::Failed));

        // A thumbnail's picture rectangle comes back with its pixels (P6-M2, the tiles' frame).
        req_tx.send(req(1, "p", "picture")).unwrap();
        assert!(matches!(rep_rx.recv_timeout(Duration::from_millis(500)).unwrap().outcome, Outcome::Picture(_, [0, 0, 1, 1])));

        // A thumbnail the cache has not got yet comes back as the icon for now, and its key is
        // given back: asked again, it renders again rather than being skipped as delivered.
        for _ in 0..2 {
            req_tx.send(req(1, "l", "later")).unwrap();
            assert!(matches!(rep_rx.recv_timeout(Duration::from_millis(500)).unwrap().outcome, Outcome::Later(_)));
        }

        let r = rep_rx.recv_timeout(Duration::from_secs(3)).expect("the slow icon arrives");
        assert_eq!(r.key, "slow");
        assert_eq!(stats.slow.load(Ordering::Relaxed), 1, "only the 2 s render is slow");
    }

    #[test]
    fn pool_renders_each_key_once() {
        let (req_tx, req_rx) = unbounded();
        let (rep_tx, rep_rx) = unbounded();
        let gens = Arc::new([AtomicU64::new(1), AtomicU64::new(0)]);
        spawn_with(req_rx, rep_tx, gens, Arc::default(), Arc::new(Stats::default()), || {}, fake_render);
        for _ in 0..20 {
            req_tx.send(req(1, "same", "busy")).unwrap();
        }
        let mut pixels = 0;
        for _ in 0..20 {
            match rep_rx.recv_timeout(Duration::from_secs(2)).expect("20 replies").outcome {
                Outcome::Pixels(_) => pixels += 1,
                Outcome::Skipped => {}
                Outcome::Failed | Outcome::Looked(_) | Outcome::Picture(..) | Outcome::Later(_) => panic!("the fake render does not fail here"),
            }
        }
        assert_eq!(pixels, 1, "one reply carries the icon, the other 19 are skipped");
        assert_eq!(BUSY_RENDERS.load(Ordering::SeqCst), 1, "rendered once across the pool");
        // Delivered keys stay taken.
        req_tx.send(req(1, "same", "busy")).unwrap();
        assert!(matches!(rep_rx.recv_timeout(Duration::from_secs(2)).unwrap().outcome, Outcome::Skipped));
        drop(req_tx);
        assert!(rep_rx.recv_timeout(Duration::from_secs(2)).is_err(), "the workers exit when the channel closes");
    }

    /// A `side` x `side` opaque PNG of one colour: RGBA, unfiltered rows in one stored deflate block.
    fn solid_png(side: u32, rgb: [u8; 3]) -> Vec<u8> {
        fn crc32(bytes: &[u8]) -> u32 {
            let mut c = !0u32;
            for &b in bytes {
                c ^= b as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
                }
            }
            !c
        }
        fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
            out.extend((data.len() as u32).to_be_bytes());
            let body: Vec<u8> = kind.iter().chain(data).copied().collect();
            out.extend(&body);
            out.extend(crc32(&body).to_be_bytes());
        }
        let mut raw = Vec::new();
        for _ in 0..side {
            raw.push(0); // filter: none
            for _ in 0..side {
                raw.extend([rgb[0], rgb[1], rgb[2], 255]);
            }
        }
        let (mut a, mut b) = (1u32, 0u32);
        for &x in &raw {
            a = (a + x as u32) % 65521;
            b = (b + a) % 65521;
        }
        let len = raw.len() as u16;
        let mut z = vec![0x78, 0x01, 0x01];
        z.extend(len.to_le_bytes());
        z.extend((!len).to_le_bytes());
        z.extend(&raw);
        z.extend(((b << 16) | a).to_be_bytes());
        let mut ihdr = Vec::new();
        ihdr.extend(side.to_be_bytes());
        ihdr.extend(side.to_be_bytes());
        ihdr.extend([8, 6, 0, 0, 0]);
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        chunk(&mut png, b"IHDR", &ihdr);
        chunk(&mut png, b"IDAT", &z);
        chunk(&mut png, b"IEND", &[]);
        png
    }

    #[test]
    fn pool_drops_thumbnails_scrolled_past() {
        let (req_tx, req_rx) = unbounded();
        let (rep_tx, rep_rx) = unbounded();
        let gens = Arc::new([AtomicU64::new(1), AtomicU64::new(0)]);
        let views = Arc::new(Views::default());
        let stats = Arc::new(Stats::default());
        // The list resolved again (resolve 2) and asks for rows 150..=190 by the time the workers
        // take the 200 thumbnails asked for in resolve 1, rows 0..200.
        views.publish(0, 2, (150, 190));
        spawn_with(req_rx, rep_tx, gens, views.clone(), stats.clone(), || {}, fake_render);
        let thumb = |row: usize, wanted: u64| Request {
            generation: 1,
            pane: 0,
            key: format!("thumb:{row}").into(),
            path: PathBuf::from("thumb"),
            size_px: 1,
            overlay: false,
            thumbnail: true,
            row,
            wanted,
            mark: Mark::None,
        };
        for row in 0..200 {
            req_tx.send(thumb(row, 1)).unwrap();
        }
        let mut rendered = Vec::new();
        for _ in 0..200 {
            let r = rep_rx.recv_timeout(Duration::from_secs(2)).expect("200 replies");
            if matches!(r.outcome, Outcome::Pixels(_)) {
                rendered.push(r.key[6..].parse::<usize>().unwrap());
            }
        }
        rendered.sort();
        // The rows asked for plus STRIP_MARGIN_ROWS each side are rendered, the rest dropped.
        assert_eq!(rendered, (146..=194).collect::<Vec<_>>());
        assert_eq!((stats.thumbs.load(Ordering::Relaxed), stats.dropped.load(Ordering::Relaxed)), (49, 151));
        // Scrolled back to the top (resolve 3): row 10's thumbnail, dropped before, renders now.
        views.publish(0, 3, (0, 40));
        req_tx.send(thumb(10, 3)).unwrap();
        assert!(matches!(rep_rx.recv_timeout(Duration::from_secs(2)).unwrap().outcome, Outcome::Pixels(_)));
        // A request of the current resolve renders even outside the range (the UI's icon margin).
        req_tx.send(thumb(60, 3)).unwrap();
        assert!(matches!(rep_rx.recv_timeout(Duration::from_secs(2)).unwrap().outcome, Outcome::Pixels(_)));
        // A plain icon request is never dropped, wherever its row is.
        req_tx.send(Request { thumbnail: false, key: "ext:.txt".into(), ..thumb(199, 1) }).unwrap();
        assert!(matches!(rep_rx.recv_timeout(Duration::from_secs(2)).unwrap().outcome, Outcome::Pixels(_)));
        assert_eq!(stats.dropped.load(Ordering::Relaxed), 151);
    }

    #[test]
    fn thumbnail_cap_falls_back_to_the_extension() {
        assert_eq!(thumb_fallback(r"thumb:C:\p\Photo.JPG|1700000000|12345"), "ext:.jpg");
        assert_eq!(thumb_fallback(r"thumb:C:\a|b\x.mp4|5|6"), "ext:.mp4", "the last two fields are the time and size");
        let mut t = IconTable::default();
        let key: SharedString = r"thumb:C:\p\new.png|1|2".into();
        assert_eq!(t.cap.capped(&key), None);
        for i in 0..THUMB_TABLE_CAP {
            assert!(t.complete(format!("thumb:{i}|0|0").into(), 32, px()));
        }
        assert!(t.complete("ext:.txt".into(), 32, px()), "icons are not counted against the cap");
        assert_eq!(t.cap.thumbs, THUMB_TABLE_CAP);
        assert_eq!(t.cap.capped(&key).as_deref(), Some("ext:.png"));
        assert_eq!(t.cap.capped(&"ext:.png".into()), None);
        assert_eq!(t.keys(32).0.state(&"thumb:7|0|0".into()), KeyState::Ready(PLACEHOLDERS + 7), "held thumbnails stay");
    }

    #[test]
    fn icon_locations_as_the_menu_commands_give_them() {
        let l = |s: &str| parse_icon_location(s);
        assert_eq!(l(r"C:\Program Files\WindowsApps\T\WindowsTerminal.exe,-101"), Some((r"C:\Program Files\WindowsApps\T\WindowsTerminal.exe".into(), -101)));
        assert_eq!(l(r"C:\Apps\N\Notepad/Notepad.exe,-2"), Some((r"C:\Apps\N\Notepad\Notepad.exe".into(), -2)), "a / in the path");
        assert_eq!(l(r"C:\PowerToys\Assets\PowerRenameUI.ico"), Some((r"C:\PowerToys\Assets\PowerRenameUI.ico".into(), 0)));
        assert_eq!(l(r"C:\Program Files\Google\NearbyShare\nearby_share.exe"), Some((r"C:\Program Files\Google\NearbyShare\nearby_share.exe".into(), 0)));
        assert_eq!(l(r#""C:\a b\x.dll",3"#), Some((r"C:\a b\x.dll".into(), 3)), "quotes go");
        assert_eq!(l(r"C:\odd,name\x.ico"), Some((r"C:\odd,name\x.ico".into(), 0)), "a comma not followed by a number is the path's");
        assert_eq!(l("@{Microsoft.WindowsNotepad_1_x64__8wekyb3d8bbwe?ms-resource://Microsoft.WindowsNotepad/Files/Assets/A.png},0"), None, "an indirect string");
        assert_eq!(l(""), None);
        assert_eq!(l(",0"), None);
    }

    #[test]
    fn cloud_mark_keys() {
        // The key format: type keys stay shared per type and mark, a full-path key carries the mark
        // as a prefix, no mark is today's key.
        let (cloud, local, overlay) = (Mark::State(1), Mark::State(2), Mark::Overlay(12));
        assert_eq!(marked("ext:.txt", cloud), "ext:s1:.txt");
        assert_eq!(marked("ext:.txt", overlay), "ext:o12:.txt");
        assert_eq!(marked("dir", local), "folder:s2");
        assert_eq!(marked("dir", overlay), "folder:o12");
        assert_eq!(marked(r"thumb:C:\a\p.jpg|5|6", cloud), r"thumb:s1:C:\a\p.jpg|5|6");
        assert_eq!(marked(r"C:\a\setup.exe", local), r"cloud:s2:C:\a\setup.exe");
        assert_eq!(marked("ext:.txt", Mark::None), "ext:.txt", "index 0 shares today's key");
        assert_eq!(marked("dir", Mark::None), "dir");
        for (key, m) in [
            ("ext:s1:.txt", cloud),
            ("ext:o12:.txt", overlay),
            ("folder:s2", local),
            (r"thumb:s1:C:\a\p.jpg|5|6", cloud),
            (r"cloud:s2:C:\a\setup.exe", local),
            ("ext:.txt", Mark::None),
            ("dir", Mark::None),
            (r"thumb:C:\a\p.jpg|5|6", Mark::None),
            (r"thumb:S:\a\p.jpg|5|6", Mark::None),
            (r"thumb:o:\a\p.jpg|5|6", Mark::None),
            (r"C:\a\setup.exe", Mark::None),
            ("folder:", Mark::None),
            ("ext:x9:.txt", Mark::None),
        ] {
            assert_eq!(key_mark(key), m, "{key}");
        }
        // A lookup's key names the entry's plain key and its path; the answer marks the plain key.
        let full = Path::new(r"G:\My Drive\a.txt");
        let look = look_key('o', "ext:.txt", full);
        assert_eq!(look, r"look:o:ext:.txt|G:\My Drive\a.txt");
        assert!(is_look_key(&look) && !is_look_key("ext:.txt"));
        assert_eq!(key_mark(&look), Mark::None, "a lookup draws nothing");
        assert_eq!(looked(&look, overlay).as_deref(), Some("ext:o12:.txt"));
        assert_eq!(looked(&look, Mark::None).as_deref(), Some("ext:.txt"));
        let thumb = look_key('o', r"thumb:G:\My Drive\p.jpg|5|6", Path::new(r"G:\My Drive\p.jpg"));
        assert_eq!(looked(&thumb, overlay).as_deref(), Some(r"thumb:o12:G:\My Drive\p.jpg|5|6"));
        assert_eq!(looked(&look_key('s', "dir", Path::new(r"C:\OneDrive\sub")), cloud).as_deref(), Some("folder:s1"));
        assert_eq!(looked("ext:.txt", cloud), None);
        // The thumbnail cap's fallback keeps the mark.
        assert_eq!(thumb_fallback(r"thumb:s1:C:\a\p.JPG|5|6"), "ext:s1:.jpg");
        assert_eq!(thumb_fallback(r"thumb:o12:G:\a\clip.mp4|5|6"), "ext:o12:.mp4");
    }

    #[test]
    fn sync_root_state_from_attributes() {
        let recall = FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS;
        assert_eq!(state_from_attrs(recall | FILE_ATTRIBUTE_ARCHIVE), Mark::State(STATE_CLOUD), "a cloud-only file, as seen without exposed placeholders");
        assert_eq!(state_from_attrs(recall | FILE_ATTRIBUTE_UNPINNED), Mark::State(STATE_CLOUD), "freed up by the user");
        assert_eq!(state_from_attrs(FILE_ATTRIBUTE_OFFLINE | FILE_ATTRIBUTE_REPARSE_POINT), Mark::State(STATE_CLOUD), "as seen with exposed placeholders");
        assert_eq!(state_from_attrs(FILE_ATTRIBUTE_ARCHIVE), Mark::State(2), "on this device");
        assert_eq!(state_from_attrs(0), Mark::State(2));
        assert_eq!(state_from_attrs(FILE_ATTRIBUTE_PINNED), Mark::State(3), "always available");
        assert_eq!(state_from_attrs(FILE_ATTRIBUTE_PINNED | FILE_ATTRIBUTE_UNPINNED), Mark::None, "OneDrive's own files: excluded");
        // Only a cloud-only file, and any Google Drive file, keep their thumbnail to the cache.
        assert!(Mark::State(1).cache_only() && Mark::Overlay(12).cache_only() && Mark::Overlay(2).cache_only());
        assert!(!Mark::State(2).cache_only() && !Mark::State(3).cache_only() && !Mark::State(6).cache_only() && !Mark::None.cache_only());
        assert_eq!((glyph_px(16), glyph_px(24), glyph_px(32), glyph_px(96), glyph_px(192)), (9, 14, 18, 24, 48));
    }

    #[test]
    fn cloud_location_keys() {
        let f = Path::new(r"C:\OneDrive\dir");
        let (file, dir) = (Kind::File, Kind::Directory);
        let recall = FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS;
        // Not a cloud location: today's keys, untouched.
        let mut plain = IconKeys::new(f.to_path_buf(), false);
        assert_eq!(plain.for_entry("a.txt", recall, file, 0, 0), "ext:.txt");
        assert_eq!(plain.for_entry("sub", FILE_ATTRIBUTE_DIRECTORY, dir, 0, 0), "dir");
        // Under a sync root: files by their attributes, shared per type and state; folders looked up.
        let mut sync = IconKeys::new(f.to_path_buf(), false).with_cloud(Cloud::SyncRoot);
        let a = sync.for_entry("a.txt", recall, file, 0, 0);
        assert_eq!(a, "ext:s1:.txt");
        assert_eq!(sync.for_entry("b.TXT", recall, file, 0, 0), a);
        assert_eq!(sync.for_entry("c.txt", 0, file, 0, 0), "ext:s2:.txt");
        assert_eq!(sync.for_entry("d.txt", FILE_ATTRIBUTE_PINNED, file, 0, 0), "ext:s3:.txt");
        assert_eq!(sync.for_entry("e.txt", FILE_ATTRIBUTE_PINNED | FILE_ATTRIBUTE_UNPINNED, file, 0, 0), "ext:.txt");
        assert_eq!(sync.for_entry("setup.exe", recall, file, 0, 0), r"cloud:s1:C:\OneDrive\dir\setup.exe");
        assert_eq!(sync.for_entry("sub", FILE_ATTRIBUTE_DIRECTORY, dir, 0, 0), r"look:s:dir|C:\OneDrive\dir\sub");
        // Links and reparse points keep the shell's own overlay (the arrow) and today's key.
        assert_eq!(sync.for_entry("Vault.lnk", FILE_ATTRIBUTE_PINNED | FILE_ATTRIBUTE_UNPINNED, file, 0, 0), r"C:\OneDrive\dir\Vault.lnk");
        let rdir = FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT;
        assert_eq!(sync.for_entry("j", rdir, Kind::Junction, 0, 0), r"C:\OneDrive\dir\j");
        let mut thumbs = IconKeys::new(f.to_path_buf(), true).with_cloud(Cloud::SyncRoot);
        assert_eq!(thumbs.for_entry("p.jpg", recall, file, 5, 6), r"thumb:s1:C:\OneDrive\dir\p.jpg|5|6");
        assert_eq!(thumbs.for_entry("q.jpg", 0, file, 5, 6), r"thumb:s2:C:\OneDrive\dir\q.jpg|5|6");
        // On Google Drive's volume: every entry is looked up.
        let g = Path::new(r"G:\My Drive");
        let mut google = IconKeys::new(g.to_path_buf(), true).with_cloud(Cloud::Overlays);
        assert_eq!(google.for_entry("a.txt", 0, file, 0, 0), r"look:o:ext:.txt|G:\My Drive\a.txt");
        assert_eq!(google.for_entry("Wedding", FILE_ATTRIBUTE_DIRECTORY, dir, 0, 0), r"look:o:dir|G:\My Drive\Wedding");
        assert_eq!(google.for_entry("p.png", 0, file, 5, 6), r"look:o:thumb:G:\My Drive\p.png|5|6|G:\My Drive\p.png");
        assert_eq!(google.for_entry("x.url", 0, file, 0, 0), r"G:\My Drive\x.url");
    }

    #[test]
    fn placeholders_out_of_sync_are_looked_up() {
        // The Cloud Files placeholder state (P9-M4): only a placeholder that is not in sync asks
        // for the provider's own state; in sync (OneDrive's 0x9 and 0xD) and not a placeholder (0x0,
        // desktop.ini) keep their attribute mark.
        let local = FILE_ATTRIBUTE_ARCHIVE | FILE_ATTRIBUTE_REPARSE_POINT;
        assert!(!needs_state_read(0x9, local, false) && !needs_state_read(0xD, local, false) && !needs_state_read(0x0, local, false));
        assert!(needs_state_read(0x1, local, false) && needs_state_read(0x1 | 0x10, local, false), "a placeholder, partial or not, out of sync");
        // Freed up but not released yet (Jeremy's console check: 0x00100420, the provider's state
        // 10, sync pending, for minutes): looked up, so the row shows what Explorer shows.
        assert!(needs_state_read(0x9, local | FILE_ATTRIBUTE_UNPINNED, false));
        // Released (recall on access) or always kept (pinned): the attributes tell it.
        assert!(!needs_state_read(0x39, local | FILE_ATTRIBUTE_UNPINNED | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_OFFLINE, false));
        assert!(!needs_state_read(0x9, local | FILE_ATTRIBUTE_PINNED, false));
        // TwinFM's own process does not see placeholders (no reparse point, state 0): the
        // attributes alone tell a file being freed (AMD.pdf read 0x00100020 there).
        assert!(needs_state_read(0, FILE_ATTRIBUTE_ARCHIVE | FILE_ATTRIBUTE_UNPINNED, false));
        assert!(!needs_state_read(0, FILE_ATTRIBUTE_ARCHIVE, false));
        assert!(!needs_state_read(0, FILE_ATTRIBUTE_UNPINNED | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_OFFLINE, false));
        // P12-M7, placeholders shown: in sync keeps its attribute mark; a file that is no placeholder
        // yet (just copied in, uploading) is looked up, OneDrive's excluded desktop.ini (pinned and
        // unpinned, hidden, system: 0x00180026 on Jeremy's OneDrive) is not; a folder never by this
        // rule (folders are looked up anyway).
        assert!(!needs_state_read(0, 0x0018_0026, true), "excluded: no mark, as before");
        assert!(!needs_state_read(0x9, FILE_ATTRIBUTE_ARCHIVE, true) && !needs_state_read(0x39, FILE_ATTRIBUTE_ARCHIVE | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, true));
        assert!(needs_state_read(0x1, FILE_ATTRIBUTE_ARCHIVE, true), "a placeholder out of sync");
        assert!(needs_state_read(0, FILE_ATTRIBUTE_ARCHIVE, true), "no placeholder yet: uploading");
        assert!(!needs_state_read(0, FILE_ATTRIBUTE_DIRECTORY, true));
        let f = Path::new(r"C:\OneDrive\dir");
        let mut sync = IconKeys::new(f.to_path_buf(), true).with_cloud(Cloud::SyncRoot);
        let (file, cloud) = (Kind::File, Kind::Cloud);
        assert_eq!(sync.for_entry_synced("a.txt", 0, cloud, 0, 0, true), "ext:s2:.txt");
        assert_eq!(sync.for_entry_synced("a.txt", 0, cloud, 0, 0, false), r"look:s:ext:.txt|C:\OneDrive\dir\a.txt");
        assert_eq!(sync.for_entry_synced("p.jpg", 0, file, 5, 6, false), r"look:s:thumb:C:\OneDrive\dir\p.jpg|5|6|C:\OneDrive\dir\p.jpg");
        // Out of a sync root the state is never read.
        let mut plain = IconKeys::new(f.to_path_buf(), false);
        assert_eq!(plain.for_entry_synced("a.txt", 0, cloud, 0, 0, false), "ext:.txt");
        // A link keeps its own key and arrow either way.
        assert_eq!(sync.for_entry_synced("v.lnk", 0, file, 0, 0, false), r"C:\OneDrive\dir\v.lnk");
        // A search tab carries no cloud location: a file not on this disk keeps its type's key, so
        // its thumbnail is never asked (it could start a download); one on this disk gets its own.
        let mut search = IconKeys::new(f.to_path_buf(), true);
        assert_eq!(search.for_entry("away.jpg", FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, file, 5, 6), "ext:.jpg");
        assert_eq!(search.for_entry("old.jpg", FILE_ATTRIBUTE_OFFLINE, file, 5, 6), "ext:.jpg");
        assert_eq!(search.for_entry("here.jpg", FILE_ATTRIBUTE_ARCHIVE, file, 5, 6), r"thumb:C:\OneDrive\dir\here.jpg|5|6");
        // Under a sync root the mark keeps a cloud-only file's thumbnail to the cache instead.
        assert_eq!(sync.for_entry_synced("away.jpg", FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, file, 5, 6, true), r"thumb:s1:C:\OneDrive\dir\away.jpg|5|6");
    }

    #[test]
    fn pool_keeps_cloud_thumbnails_to_the_cache_and_looks_up_every_time() {
        let (req_tx, req_rx) = unbounded();
        let (rep_tx, rep_rx) = unbounded();
        let gens = Arc::new([AtomicU64::new(1), AtomicU64::new(0)]);
        let stats = Arc::new(Stats::default());
        spawn_with(req_rx, rep_tx, gens, Arc::default(), stats.clone(), || {}, fake_render);
        let ask = |key: &str, path: &str, thumbnail: bool, mark: Mark| Request { thumbnail, mark, ..req(1, key, path) };
        // The marks' images come from the shell, which a stand-in render does not need: Overlay(0)
        // and State(0) have none, so these replies are the stand-in's pixels.
        req_tx.send(ask("t1", "thumbmode-cloud", true, Mark::State(STATE_CLOUD))).unwrap();
        req_tx.send(ask("t2", "thumbmode-local", true, Mark::State(0))).unwrap();
        req_tx.send(ask("t3", "thumbmode-google", true, Mark::Overlay(0))).unwrap();
        req_tx.send(ask("t4", "thumbmode-plain", true, Mark::None)).unwrap();
        req_tx.send(ask("t5", "thumbmode-icon", false, Mark::State(STATE_CLOUD))).unwrap();
        for _ in 0..5 {
            rep_rx.recv_timeout(Duration::from_secs(2)).expect("five replies");
        }
        let mut modes = lock(&THUMB_MODES).clone();
        modes.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            modes,
            vec![
                ("thumbmode-cloud".into(), Thumb::CacheOnly),
                ("thumbmode-google".into(), Thumb::CacheOnly),
                ("thumbmode-icon".into(), Thumb::No),
                ("thumbmode-local".into(), Thumb::Extract),
                ("thumbmode-plain".into(), Thumb::Extract),
            ]
        );
        // A lookup is made every time it is asked (never `taken`), and replies with the row.
        let missing = std::env::temp_dir().join("twinfm-p6m1-no-such-file.txt");
        let look = look_key('o', "ext:.txt", &missing);
        for row in [7, 9] {
            req_tx.send(Request { row, path: missing.clone(), ..req(1, &look, "") }).unwrap();
            let r = rep_rx.recv_timeout(Duration::from_secs(5)).expect("a lookup reply");
            assert_eq!((r.key.as_str(), r.row), (look.as_str(), row));
            assert!(matches!(r.outcome, Outcome::Looked(Mark::None)), "no file, no overlay");
        }
        assert_eq!(stats.cloud.load(Ordering::Relaxed), 2);
        assert_eq!(stats.rendered.load(Ordering::Relaxed), 1, "lookups are not renders");
        // The table treats a lookup's reply as nothing to intern.
        let mut t = IconTable::default();
        t.keys(16).0.begin(&look.as_str().into());
        assert!(!t.complete(look.as_str().into(), 16, Outcome::Looked(Mark::Overlay(12))));
        assert_eq!(t.keys(16).0.state(&look.as_str().into()), KeyState::Missing);
    }

    #[test]
    fn status_glyphs_come_from_imageres() {
        // Reads imageres.dll only: each state's glyph sits in the bottom-left corner of the square.
        for (state, res) in STATE_GLYPHS {
            for size in [16, 32, 96] {
                let g = state_glyph(res, size).unwrap_or_else(|| panic!("state {state} at {size} px: no glyph"));
                assert_eq!((g.width(), g.height()), (size, size));
                let (gp, px) = (glyph_px(size), g.as_slice());
                let inside = |x: u32, y: u32| x < gp && y >= size - gp;
                assert!(px.iter().enumerate().any(|(i, p)| p.a != 0 && inside(i as u32 % size, i as u32 / size)), "state {state}: visible");
                assert!(px.iter().enumerate().all(|(i, p)| p.a == 0 || inside(i as u32 % size, i as u32 / size)), "state {state}: only in its corner");
            }
        }
        // The cloud and the tick differ.
        assert_ne!(state_glyph(1404, 16).unwrap().as_slice(), state_glyph(1400, 16).unwrap().as_slice());
        // P12-M7: a file waiting (4, 5, 10) or syncing (6) shows the arrows; in error (7), the red
        // cross; with a warning (8), the warning; each unlike the others and unlike the tick.
        let res = |state: u8| STATE_GLYPHS.iter().find(|(s, _)| *s == state).map(|&(_, r)| r).unwrap();
        assert!([4, 5, 10].iter().all(|&s| res(s) == res(6)), "pending states draw the syncing arrows");
        let px = |state: u8| state_glyph(res(state), 16).unwrap().as_slice().to_vec();
        for (a, b) in [(6, 7), (6, 8), (7, 8), (6, 2), (7, 2), (8, 2)] {
            assert_ne!(px(a), px(b), "states {a} and {b} share a glyph");
        }
    }

    #[test]
    fn worker_renders_exe_drive_and_extension() {
        let (req_tx, req_rx) = unbounded();
        let (rep_tx, rep_rx) = unbounded();
        let gens = Arc::new([AtomicU64::new(1), AtomicU64::new(0)]);
        spawn(req_rx, rep_tx, gens.clone(), Arc::default(), Arc::new(Stats::default()), || {});
        let txt = std::env::temp_dir().join(format!("twinfm-icon-{}.txt", std::process::id()));
        std::fs::write(&txt, b"x").unwrap();
        // A junction, asked for twice: plain and with the shell's overlay (the link arrow).
        let dir = std::env::temp_dir().join(format!("twinfm-p2m5-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("target")).unwrap();
        let junction = dir.join("junc");
        let made = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&junction)
            .arg(dir.join("target"))
            .output()
            .is_ok_and(|o| o.status.success());
        assert!(made, "mklink /J failed");
        // A red PNG, asked for as a thumbnail at 32 px.
        let png = dir.join("red.png");
        std::fs::write(&png, solid_png(16, [220, 20, 20])).unwrap();
        let jobs = [
            (r"C:\Windows\notepad.exe".to_string(), PathBuf::from(r"C:\Windows\notepad.exe")),
            (r"C:\".to_string(), PathBuf::from(r"C:\")),
            ("ext:.txt".to_string(), txt.clone()),
            ("junction".to_string(), junction.clone()),
        ];
        for (key, path) in &jobs {
            req_tx.send(Request { generation: 1, pane: 0, key: key.as_str().into(), path: path.clone(), size_px: 16, overlay: false, thumbnail: false, row: 0, wanted: 0, mark: Mark::None }).unwrap();
        }
        let key = "junction+overlay";
        req_tx.send(Request { generation: 1, pane: 0, key: key.into(), path: junction.clone(), size_px: 16, overlay: true, thumbnail: false, row: 0, wanted: 0, mark: Mark::None }).unwrap();
        let thumb: SharedString = thumb_key(&png, 0, 0).into();
        req_tx.send(Request { generation: 1, pane: 0, key: thumb.clone(), path: png.clone(), size_px: 32, overlay: false, thumbnail: true, row: 0, wanted: 0, mark: Mark::None }).unwrap();
        // A stale request (pane 1 is at generation 0) is skipped, not rendered.
        req_tx.send(Request { generation: 5, pane: 1, key: "stale".into(), path: txt.clone(), size_px: 16, overlay: false, thumbnail: false, row: 0, wanted: 0, mark: Mark::None }).unwrap();
        let mut got = HashMap::new();
        while got.len() < 7 {
            let r = rep_rx.recv_timeout(Duration::from_secs(5)).expect("reply within 5 s");
            got.insert(r.key.to_string(), r.outcome);
        }
        std::fs::remove_file(&txt).unwrap();
        std::fs::remove_dir_all(&dir).unwrap(); // removes the junction, not its target's contents
        assert!(matches!(got.get("stale"), Some(Outcome::Skipped)));
        if jobs.iter().all(|(k, _)| matches!(got.get(k), Some(Outcome::Failed))) {
            eprintln!("skipping: the shell returned no icons at all (COM unavailable in this environment?)");
            return;
        }
        for (key, _) in &jobs {
            match got.get(key) {
                Some(Outcome::Pixels(p)) => {
                    assert!(p.width() >= 16 && p.height() >= 16, "{key}: {}x{}", p.width(), p.height());
                    assert!(p.as_slice().iter().any(|q| q.a != 0), "{key}: some visible pixels");
                }
                Some(Outcome::Failed) => panic!("{key}: the shell returned no icon"),
                _ => panic!("{key}: no reply"),
            }
        }
        match (got.get("junction"), got.get(key)) {
            (Some(Outcome::Pixels(plain)), Some(Outcome::Pixels(with))) => {
                assert_eq!((with.width(), with.height()), (16, 16), "fitted to the size asked for, then overlaid");
                assert_ne!(fit(plain, 16).as_slice(), with.as_slice(), "the overlay changed the junction's icon");
            }
            _ => panic!("the junction's icons did not both arrive"),
        }
        match got.get(thumb.as_str()) {
            Some(Outcome::Picture(p, rect)) => {
                assert_eq!((p.width(), p.height()), (32, 32), "letterboxed into the size asked for");
                let centre = p.as_slice()[16 * 32 + 16];
                assert_eq!((centre.r, centre.g, centre.b, centre.a), (220, 20, 20, 255), "the PNG's own pixels");
                assert!(p.as_slice().iter().all(|q| q.a == 255), "a square image fills the square");
                assert_eq!(*rect, [0, 0, 32, 32], "the picture's rectangle, for the tiles' frame");
            }
            Some(Outcome::Pixels(p)) => {
                // The shell gave the type's icon (no thumbnail handler or cache here): no picture.
                eprintln!("skipping the thumbnail check: the shell returned no thumbnail");
                assert!(p.as_slice().iter().any(|q| q.a != 0));
            }
            _ => panic!("the PNG got no thumbnail and no icon"),
        }
    }
}

