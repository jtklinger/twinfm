//! OLE drag and drop: TwinFM as a drop target (`IDropTarget` on the main window) and as a drag
//! source (`SHCreateDataObject` + `DoDragDrop`), so files move between the panes and to and from
//! Explorer and other programs.
//!
//! Everything here runs on the UI thread, which winit made a single-threaded apartment with
//! `OleInitialize`. Every COM object made here is a `#[repr(C)]` struct with a hand-written
//! vtable and an atomic reference count, as in `fileops` and `shellmenu`.
//!
//! **Registration.** winit registers its own `IDropTarget` on the window when it creates it, so
//! `RegisterDragDrop` answers `DRAGDROP_E_ALREADYREGISTERED` until `RevokeDragDrop` has taken
//! winit's away. `register` does both, once, when the window handle is first known. winit revokes
//! whatever is registered when the window is destroyed, which releases ours.
//!
//! **When the target methods run.** For a drag that another program started, OLE calls our
//! `DragEnter`/`DragOver`/`DragLeave`/`Drop` from messages dispatched by winit's ordinary message
//! loop, outside any winit handler, so Slint renders the highlight normally. A drag we start
//! ourselves is asked for inside a winit handler (the pointer move that crossed the threshold),
//! where nothing of Slint could render (see `shellmenu`'s modal-loop notes), so it is not run
//! there: `crate::drag_start` posts `WM_RUN_DRAG`, and `DoDragDrop` runs when winit's message loop
//! dispatches it, outside any handler. OLE's loop then dispatches WM_PAINT to winit, which hands
//! Slint a redraw; `repaint_during_own_drag` asks for one whenever the highlight or a list's
//! scroll changes, because Slint's own redraw request waits on a Slint timer, and no Slint timer
//! runs inside OLE's loop.
//!
//! **Modal-loop rule.** `drag` must be called with no `RefCell` borrow of app state held. The
//! target methods take what they need from the app state under short, non-reentrant borrows
//! (`crate::drop_spot`), set the two highlight properties, and never submit a job themselves:
//! `Drop` hands the job to `slint::invoke_from_event_loop`, which runs after the drag loop has
//! returned (our own drag) or at the next turn of the event loop (another program's drag).
//!
//! **Virtual files** (P3-M4). A data object with `FileGroupDescriptorW` and `FileContents` but no
//! `CF_HDROP` (a zip folder's items, a mail attachment) is a copy that TwinFM writes itself: inside
//! `Drop` for a synchronous source (it touches the file system and the data object only, never the
//! app state; its note goes through the event loop), or on a worker for a source that asks for
//! asynchronous extraction. Our own drags out of a zip folder carry the zip folder's data object.
//!
//! **Moves are optimized moves.** A move dropped on TwinFM is performed by TwinFM's file
//! operation worker, later. So `Drop` reports `DROPEFFECT_NONE` to the source, with "Performed
//! DropEffect" `DROPEFFECT_NONE` and "Logical Performed DropEffect" `DROPEFFECT_MOVE` on the data
//! object, which is how Explorer tells a source not to delete the originals (it does the same when
//! it is the target). Reporting `DROPEFFECT_MOVE` could make a source delete the files before the
//! worker has moved them.

use crate::fileops::clipboard;
use crate::model::SelOp;
use crate::shellmenu::{self, Com};
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use windows_sys::Win32::Foundation::{
    DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, DV_E_FORMATETC, E_NOINTERFACE, E_NOTIMPL,
    GlobalFree, HWND, LPARAM, LRESULT, OLE_E_ADVISENOTSUPPORTED, POINT, POINTL, RECT, S_OK, SIZE, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, CreateFontW, DIB_RGB_COLORS, DT_CENTER,
    DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject, DrawTextW, GdiFlush, HBITMAP, ScreenToClient, SelectObject,
    RDW_INTERNALPAINT, RedrawWindow, SetBkMode, SetTextColor, TRANSPARENT,
};
use std::os::windows::ffi::OsStrExt;
use windows_sys::Win32::Foundation::MAX_PATH;
use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, GetVolumePathNameW};
use windows_sys::Win32::System::Com::Marshal::CoMarshalInterThreadInterfaceInStream;
use windows_sys::Win32::System::Com::StructuredStorage::CoGetInterfaceAndReleaseStream;
use windows_sys::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize, DVASPECT_CONTENT, FORMATETC, STGMEDIUM,
    TYMED_HGLOBAL, TYMED_ISTREAM,
};
use windows_sys::Win32::System::DataExchange::RegisterClipboardFormatW;
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows_sys::Win32::System::Ole::{
    CF_HDROP, DROPEFFECT_COPY, DROPEFFECT_MOVE, DROPEFFECT_NONE, DoDragDrop, MK_ALT, RegisterDragDrop, ReleaseStgMedium,
    RevokeDragDrop,
};
use windows_sys::Win32::System::SystemServices::{MK_CONTROL, MK_LBUTTON, MK_RBUTTON, MK_SHIFT};
use windows_sys::Win32::Globalization::{CP_ACP, MultiByteToWideChar};
use windows_sys::Win32::UI::Shell::Common::ITEMIDLIST;
use windows_sys::Win32::UI::Shell::{
    CFSTR_DROPDESCRIPTION, CFSTR_FILECONTENTS, CFSTR_FILEDESCRIPTORA, CFSTR_FILEDESCRIPTORW, CFSTR_SHELLIDLIST, DROPIMAGE_COPY,
    DROPIMAGE_INVALID, DROPIMAGE_MOVE, DROPIMAGE_NONE, ILCombine, ILFree, SHGetNameFromIDList,
    SIGDN_DESKTOPABSOLUTEPARSING, CFSTR_LOGICALPERFORMEDDROPEFFECT, CFSTR_PERFORMEDDROPEFFECT, DefSubclassProc,
    FD_ATTRIBUTES, FD_FILESIZE, FD_PROGRESSUI, RemoveWindowSubclass, SHCreateDataObject, SHCreateMemStream, SHDRAGIMAGE,
    SetWindowSubclass,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, MF_GRAYED, MF_SEPARATOR, MF_STRING, SetForegroundWindow, SetMenuDefaultItem,
    TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, TPM_TOPALIGN, TrackPopupMenuEx, WM_ENTERIDLE,
};
use windows_sys::core::{BOOL, GUID, HRESULT, PCWSTR};

pub const NONE: u32 = DROPEFFECT_NONE;
pub const COPY: u32 = DROPEFFECT_COPY;
pub const MOVE: u32 = DROPEFFECT_MOVE;

/// Posted to the main window by `crate::drag_start`; the permanent subclass (`devices`) runs the
/// drag (`crate::run_pending_drag`) when it is dispatched, outside winit's event handler.
pub const WM_RUN_DRAG: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 0x44;

// ---------------------------------------------------------------------------------------------
// Pure decisions (unit-tested)
// ---------------------------------------------------------------------------------------------

/// `IDropSource::QueryContinueDrag`. A left-button drag: Escape or the right button cancels,
/// releasing the left button drops. A right-button drag (`right`), as in Explorer: Escape or the
/// left button cancels, releasing the right button drops (the target then asks what to do).
/// Anything else keeps dragging.
pub fn continue_drag(escape: bool, lbutton: bool, rbutton: bool, right: bool) -> HRESULT {
    let (held, other) = if right { (rbutton, lbutton) } else { (lbutton, rbutton) };
    if escape || other {
        DRAGDROP_S_CANCEL
    } else if !held {
        DRAGDROP_S_DROP
    } else {
        S_OK
    }
}

/// The right-button drop menu's items (command ids 1, 2, 3; 0 is "dismissed").
pub const MENU_MOVE: u32 = 1;
pub const MENU_COPY: u32 = 2;
pub const MENU_CANCEL: u32 = 3;
pub const MENU_LABELS: [&str; 3] = ["Move here", "Copy here", "Cancel"];

/// The menu item drawn bold: the one a left-button drop would have done.
pub fn menu_default(effect: u32) -> u32 {
    if effect == MOVE { MENU_MOVE } else { MENU_COPY }
}

/// The effect a right-button drop menu's choice means: "Move here" and "Copy here" when the
/// source allows them (the menu greys out the others), else, and for "Cancel" or a dismissed
/// menu (Escape, a click elsewhere), none.
pub fn menu_effect(cmd: u32, allowed: u32) -> u32 {
    match cmd {
        MENU_MOVE if allowed & MOVE != 0 => MOVE,
        MENU_COPY if allowed & COPY != 0 => COPY,
        _ => NONE,
    }
}

/// The volume a path lives on, as a comparable key: for a local drive path the mount point
/// `GetVolumePathNameW` gives (lowercase, no trailing separator: `c:`, or `c:\mnt\data` for a
/// volume mounted in that folder), else `path_volume_key`'s string rule: always for UNC paths and
/// mapped network drives (a mount point lies on a local volume, and the call could wait on a dead
/// server), and when the call fails. The path need not exist: the call walks up to the nearest
/// mount point. One file-system call, so the drop target caches it (`VolumeCache`).
pub fn volume_key(p: &Path) -> String {
    let fallback = path_volume_key(p);
    if fallback.starts_with(r"\\") || crate::fs::watch::is_remote(p) {
        return fallback;
    }
    let wide: Vec<u16> = p.as_os_str().encode_wide().chain([0]).collect();
    let mut buf = vec![0u16; wide.len().max(MAX_PATH as usize + 1)];
    // SAFETY: `wide` is NUL-terminated and `buf` is writable for the length passed.
    let ok = unsafe { GetVolumePathNameW(wide.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
    let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
    if ok == 0 || n == 0 {
        return fallback;
    }
    let s = String::from_utf16_lossy(&buf[..n]).to_lowercase();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    s.trim_end_matches('\\').to_string()
}

/// A one-entry cache of a folder's volume key: DragOver runs about every 50 ms while a drag
/// hovers, and the destination folder changes only when the pointer crosses into another row,
/// tab or pane, so `volume_key` runs once per distinct destination.
#[derive(Default)]
pub struct VolumeCache {
    last: Option<(PathBuf, String)>,
}

impl VolumeCache {
    /// The key of `dest`, from the cache or from `lookup`, and whether `lookup` ran for it.
    pub fn key(&mut self, dest: &Path, lookup: impl FnOnce(&Path) -> String) -> (String, bool) {
        if let Some((p, k)) = &self.last {
            if p == dest {
                return (k.clone(), false);
            }
        }
        let k = lookup(dest);
        self.last = Some((dest.to_path_buf(), k.clone()));
        (k, true)
    }
}

/// The string rule: `c:` for a drive path, `\\server\share` for a UNC path (lowercase, `\\?\` and
/// `\\?\UNC\` prefixes removed). A volume mounted in a folder counts as the drive that holds it.
pub fn path_volume_key(p: &Path) -> String {
    let s = p.to_string_lossy().to_lowercase();
    let s = if let Some(rest) = s.strip_prefix(r"\\?\unc\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        s
    };
    if let Some(rest) = s.strip_prefix(r"\\") {
        let mut parts = rest.split('\\').filter(|p| !p.is_empty());
        let server = parts.next().unwrap_or("");
        let share = parts.next().unwrap_or("");
        return format!(r"\\{server}\{share}");
    }
    s.split('\\').next().unwrap_or("").to_string()
}

/// Lowercase, no trailing separator: for comparing folders.
fn norm(p: &Path) -> String {
    p.to_string_lossy().trim_end_matches('\\').to_lowercase()
}

/// A drop of `sources` into `dest` that makes no sense: into a source's own folder, onto one of
/// the dragged items, or into a folder inside a dragged folder.
pub fn refused(sources: &[PathBuf], dest: &Path) -> bool {
    let d = norm(dest);
    sources.iter().any(|s| {
        let s = norm(s);
        let parent = s.rfind('\\').map(|i| &s[..i]).unwrap_or("");
        parent == d || s == d || d.strip_prefix(&s).is_some_and(|rest| rest.starts_with('\\'))
    })
}

/// The effect for a drop, from the modifier keys (`grfKeyState`), the effects the source allows,
/// and whether source and destination share a volume. Explorer's rules: move on the same volume,
/// copy across volumes; Ctrl forces a copy and Shift a move. Ctrl+Shift and Alt ask Explorer for
/// a shortcut; TwinFM makes no shortcuts and copies instead. A wish the source does not allow
/// falls back to the other effect it allows (copy first).
pub fn decide_effect(keys: u32, allowed: u32, same_volume: bool) -> u32 {
    let want = if keys & (MK_CONTROL | MK_ALT) != 0 {
        COPY
    } else if keys & MK_SHIFT != 0 {
        MOVE
    } else if same_volume {
        MOVE
    } else {
        COPY
    };
    if allowed & want != 0 {
        want
    } else if allowed & COPY != 0 {
        COPY
    } else if allowed & MOVE != 0 {
        MOVE
    } else {
        NONE
    }
}

/// The whole decision for dropping `sources` into `dest`: none for an empty list or a refused
/// drop, else `decide_effect`. `same_volume`: the first source and `dest` share a volume (the
/// drop target decides it from its cached `volume_key`s).
pub fn effect_for(sources: &[PathBuf], dest: &Path, keys: u32, allowed: u32, same_volume: bool) -> u32 {
    if sources.is_empty() || refused(sources, dest) { NONE } else { decide_effect(keys, allowed, same_volume) }
}

/// What the text under the drag image says (`DROPDESCRIPTION`): the image type, the message and
/// the text the shell puts in place of its `%1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Description {
    pub kind: i32,
    pub message: &'static str,
    pub insert: String,
}

impl Description {
    /// `DROPIMAGE_INVALID` with no text: the shell goes back to the source's default (set when a
    /// drag leaves us or drops, so Explorer as the source shows its own text again).
    pub fn cleared() -> Description {
        Description { kind: DROPIMAGE_INVALID, message: "", insert: String::new() }
    }
}

/// The description for a drop with `effect` into `dest`: "Move to %1" or "Copy to %1" with the
/// folder's name; for none (or no folder), `DROPIMAGE_NONE` with no text, so the shell shows its
/// own "not allowed" image.
pub fn description(effect: u32, dest: Option<&Path>) -> Description {
    let (kind, message) = match effect {
        MOVE => (DROPIMAGE_MOVE, "Move to %1"),
        COPY => (DROPIMAGE_COPY, "Copy to %1"),
        _ => (DROPIMAGE_NONE, ""),
    };
    match dest {
        Some(d) if kind != DROPIMAGE_NONE => Description { kind, message, insert: folder_label(d) },
        _ => Description { kind: DROPIMAGE_NONE, message: "", insert: String::new() },
    }
}

/// A folder's name as the description shows it: its last component, `C:` for a drive root, the
/// share's name for a UNC share root.
pub fn folder_label(dest: &Path) -> String {
    if let Some(n) = dest.file_name() {
        return n.to_string_lossy().into_owned();
    }
    let s = dest.to_string_lossy();
    let s = s.trim_end_matches('\\');
    s.rsplit('\\').next().unwrap_or(s).to_string()
}

/// The `DROPDESCRIPTION` struct's bytes (shlobj_core.h): the type (4 bytes), then the message and
/// the insert, 260 wide characters each, NUL-terminated (cut at 259).
pub fn description_block(d: &Description) -> Vec<u8> {
    let mut b = d.kind.to_le_bytes().to_vec();
    for s in [d.message, d.insert.as_str()] {
        let mut w = [0u16; 260];
        for (k, u) in s.encode_utf16().take(259).enumerate() {
            w[k] = u;
        }
        b.extend(w.iter().flat_map(|u| u.to_le_bytes()));
    }
    b
}

/// What the effect reported back to the source is: a move is an optimized move (module docs).
pub fn reported_effect(performed: u32) -> u32 {
    if performed == MOVE { NONE } else { performed }
}

/// Window geometry, logical px, as `app.slint` lays it out.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub width: f32,
    pub height: f32,
    pub split: bool,
    pub ratio: f32,
    /// Whether each pane shows its tab strip (it has more than one tab), which moves its list down.
    pub tabs: [bool; 2],
    /// Where the panes start: right of the sidebar and its 4 px resize edge, or of the 12 px
    /// strip of a collapsed sidebar.
    pub left: f32,
    /// The lists' row height (`icons::ListGeom`: 22 px, or 32 px with thumbnails on).
    pub row_px: f32,
    /// Each pane showing the icon grid: its tiles to a grid row; None for a pane showing the
    /// details list.
    pub grid: [Option<usize>; 2],
    /// Each pane's tiles in the grid (large or medium, `icons::TileGeom`; P6-M2).
    pub tile: [crate::icons::TileGeom; 2],
    /// Where the sidebar and the panes start: under the toolbar across the window (`TOP_BAR`) or
    /// at 0 (P6-M3).
    pub top: f32,
    /// The toolbar at the top of each pane, above its tab strip and path bar, and its spacing
    /// (`PANE_BAR`), or 0.
    pub bar: f32,
    /// The width the preview column and its 6 px handle take from the window's right edge
    /// (P8-M1: its share of the space right of `left`, plus 3 px), or 0 while it is hidden.
    pub preview: f32,
}

impl Layout {
    /// The height of one line of the pane's list: a row, or a grid row of tiles.
    pub fn row_px_of(&self, pane: usize) -> f32 {
        if self.grid[pane].is_some() { self.tile[pane].h } else { self.row_px }
    }
}

/// The list's top inside a pane (pane.slint): 3 px padding, 36 px path entry, 2 px spacing,
/// 32 px header, 2 px spacing (P10-M1 task 5; 63 px before it). Its bottom is 33 px above the
/// pane's (2 px spacing, 28 px status line, 3 px padding), its sides 3 px in.
pub const LIST_TOP: f32 = 75.0;
pub const LIST_BOTTOM_GAP: f32 = 33.0;
pub const PANE_PAD: f32 = 3.0;
/// The tab strip above the path bar (30 px) and the layout spacing under it (2 px), present only
/// when the pane has more than one tab.
pub const TAB_STRIP: f32 = 32.0;

/// The toolbar across the window (app.slint, `toolbar = "top"`): 44 px over the sidebar and the
/// panes (36 before P10-M1 task 5). The toolbar in each pane (`toolbar = "pane"`): 28 px at the pane's top, above the tab
/// strip and the path bar, and the 2 px layout spacing under it (P6-M3).
pub const TOP_BAR: f32 = 44.0;
pub const PANE_BAR: f32 = 30.0;

/// The top of the pane's list, in window px.
pub fn list_top(l: &Layout, pane: usize) -> f32 {
    l.top + l.bar + if l.tabs[pane] { LIST_TOP + TAB_STRIP } else { LIST_TOP }
}

/// The top of the panes' tab strips, in window px (under the in-pane toolbar when it shows).
pub fn tab_top(l: &Layout) -> f32 {
    l.top + l.bar + TAB_TOP
}

/// The top of the sidebar's list, in window px.
pub fn side_top(l: &Layout) -> f32 {
    l.top + SIDE_TOP
}

/// Where a window point lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spot {
    /// Outside both panes (the splitter, or off the window), or on a sidebar header or the
    /// sidebar's footer.
    Outside,
    /// In the pane but not over a row (path bar, header, empty space, status line, the tab
    /// strip past the last tab).
    Pane(usize),
    /// Over row `.1` of pane `.0`.
    Row(usize, usize),
    /// Over sidebar row `.0` (a drive, a place or a bookmark).
    Side(usize),
    /// Over tab `.1` in pane `.0`'s tab strip.
    Tab(usize, usize),
    /// Over breadcrumb button `.1` of pane `.0`'s path bar.
    Crumb(usize, usize),
}

/// The sidebar's list starts under its 3 px top padding and ends above the 32 px footer (the
/// "About TwinFM" row); a pane's tab strip is 30 px tall under the pane's 3 px padding.
pub const SIDE_TOP: f32 = 3.0;
pub const SIDE_FOOTER: f32 = 32.0;
pub const TAB_TOP: f32 = 3.0;
pub const TAB_H: f32 = 30.0;
/// The breadcrumb buttons (pane.slint `crumb-row`): 3 px padding inside the 36 px path bar, which
/// sits under the pane's 3 px padding and, with two tabs or more, the tab strip (`TAB_STRIP`), so
/// the buttons are 30 px tall from 6 px (or 38 px) below the pane's top.
pub const CRUMB_PAD: f32 = 3.0;
pub const CRUMB_H: f32 = 30.0;

/// The top of pane `pane`'s breadcrumb buttons, in window px.
pub fn crumb_top(l: &Layout, pane: usize) -> f32 {
    l.top + PANE_PAD + if l.tabs[pane] { TAB_STRIP } else { 0.0 } + l.bar + CRUMB_PAD
}

/// What `hit_all` needs beyond the panes: the sidebar, the tab strips and the breadcrumbs
/// (logical px).
#[derive(Clone, Copy, Debug)]
pub struct Strips<'a> {
    /// The sidebar's width (0 when it is collapsed: no targets), its list's scroll offset (zero
    /// or negative), and each of its rows' height and whether it is a drop target (headers are not).
    pub side_w: f32,
    pub side_scroll: f32,
    pub side_rows: &'a [(f32, bool)],
    /// Each pane's tab strip: tab count, tab width and the strip's scroll offset. The strip shows
    /// only when the pane has more than one tab (`Layout::tabs`).
    pub tabs: [(usize, f32, f32); 2],
    /// Each pane's breadcrumb as Slint reported it: the crumb row's offset in the bar (zero or
    /// negative: right-aligned when it is wider) and the width the bar shows of it, then each
    /// crumb's x in the row and its label's width. No crumbs while the text entry shows.
    pub crumb_row: [(f32, f32); 2],
    pub crumbs: [&'a [(f32, f32)]; 2],
}

/// The top of sidebar row `i` in the sidebar list's content (before its scroll), from the rows'
/// heights.
pub fn side_row_top(rows: &[(f32, bool)], i: usize) -> f32 {
    rows.iter().take(i).map(|r| r.0).sum()
}

/// `hit`, plus the sidebar's rows, the panes' tab strips and their breadcrumbs.
pub fn hit_all(l: &Layout, s: &Strips, x: f32, y: f32, content_y: [f32; 2], len: [usize; 2]) -> Spot {
    if y < l.top || y >= l.height || x < 0.0 {
        return Spot::Outside;
    }
    if x < s.side_w {
        if y < side_top(l) || y >= l.height - SIDE_FOOTER {
            return Spot::Outside;
        }
        let ly = y - side_top(l) - s.side_scroll;
        let mut top = 0.0;
        for (i, &(h, target)) in s.side_rows.iter().enumerate() {
            if ly >= top && ly < top + h {
                return if target { Spot::Side(i) } else { Spot::Outside };
            }
            top += h;
        }
        return Spot::Outside;
    }
    let panes = if l.split { 2 } else { 1 };
    for pane in 0..panes {
        let (a, b) = pane_x(l, pane);
        if l.tabs[pane] && x >= a + PANE_PAD && x < b - PANE_PAD && y >= tab_top(l) && y < tab_top(l) + TAB_H {
            let (n, w, off) = s.tabs[pane];
            let i = ((x - a - PANE_PAD - off) / w).floor();
            return if w > 0.0 && i >= 0.0 && (i as usize) < n { Spot::Tab(pane, i as usize) } else { Spot::Pane(pane) };
        }
        // A crumb's label (its chevron, the bar past the crumbs and a crumb clipped off the bar's
        // left end are the pane).
        let top = crumb_top(l, pane);
        let (row_x, shown_w) = s.crumb_row[pane];
        let bx = x - a - PANE_PAD;
        if x < b - PANE_PAD && bx >= 0.0 && bx < shown_w && y >= top && y < top + CRUMB_H {
            let cx = bx - row_x;
            if let Some(i) = s.crumbs[pane].iter().position(|&(cx0, w)| cx >= cx0 && cx < cx0 + w) {
                return Spot::Crumb(pane, i);
            }
        }
    }
    hit(l, x, y, content_y, len)
}

/// The pane's x range: the left pane is the whole panes area without the split, else up to 3 px
/// left of the split line; the right pane starts 3 px right of it (the splitter is the 6 px
/// between). The ratio applies to the panes area, right of `left` and left of the preview column.
pub fn pane_x(l: &Layout, pane: usize) -> (f32, f32) {
    let right = l.width - l.preview;
    let line = l.left + (right - l.left) * l.ratio;
    match (l.split, pane) {
        (false, _) => (l.left, right),
        (true, 0) => (l.left, line - 3.0),
        (true, _) => (line + 3.0, right),
    }
}

/// The width the preview column takes from the window's right edge (`Layout::preview`) at share
/// `ratio` of the space right of `left` (app.slint: the panes narrow to that space times 1 - ratio,
/// less 3 px; the handle's 6 px and the column take the rest).
pub fn preview_w(width: f32, left: f32, ratio: f32) -> f32 {
    (width - left) * ratio + 3.0
}

/// The name cell of pane `pane`'s rows (P9-M1): its left edge in window px and its width, the
/// list's width less the rows' left padding (`ListGeom::name_x`), their 6 px right padding and the
/// width the columns shown take (`cols::Widths::shown`); zero when the pane is too narrow for them,
/// as the row clips them. `hit` keeps the whole row: a drop on the Type cell lands on the row.
pub fn name_cell(l: &Layout, pane: usize, shown: f32) -> (f32, f32) {
    let (a, b) = pane_x(l, pane);
    let x = a + PANE_PAD + crate::icons::ListGeom::new(l.row_px > 22.0).name_x();
    (x, (b - PANE_PAD - 6.0 - shown - x).max(0.0))
}

/// Hit-test a window point (logical px) against the panes and their rows. `content_y` is each
/// list's scroll offset (zero or negative), `len` its row count.
pub fn hit(l: &Layout, x: f32, y: f32, content_y: [f32; 2], len: [usize; 2]) -> Spot {
    if y < l.top || y >= l.height {
        return Spot::Outside;
    }
    let panes = if l.split { 2 } else { 1 };
    let Some(pane) = (0..panes).find(|&p| {
        let (a, b) = pane_x(l, p);
        x >= a && x < b
    }) else {
        return Spot::Outside;
    };
    let (a, b) = pane_x(l, pane);
    let in_list = x >= a + PANE_PAD && x < b - PANE_PAD && y >= list_top(l, pane) && y < l.height - LIST_BOTTOM_GAP;
    if !in_list {
        return Spot::Pane(pane);
    }
    let r = ((y - list_top(l, pane) - content_y[pane]) / l.row_px_of(pane)).floor();
    if r < 0.0 {
        return Spot::Pane(pane);
    }
    // The grid: the tile's column from x; past the last column of a grid row (the space right
    // of the tiles) or past the last entry is the pane.
    let i = match l.grid[pane] {
        None => r as usize,
        Some(cols) => {
            let c = ((x - a - PANE_PAD) / l.tile[pane].w).floor() as usize;
            if c >= cols {
                return Spot::Pane(pane);
            }
            r as usize * cols + c
        }
    };
    if i < len[pane] { Spot::Row(pane, i) } else { Spot::Pane(pane) }
}

/// How close to a list's top or bottom edge a hovering drag scrolls it, logical px.
pub const EDGE_PX: f32 = 24.0;
/// How often a hovering drag scrolls a list by one row.
pub const SCROLL_EVERY: std::time::Duration = std::time::Duration::from_millis(50);

/// A drag hovering at window point (x, y) near a list's edge: that pane and the direction to
/// scroll (-1 up, toward the top edge; 1 down). None outside the lists or away from their edges.
pub fn edge_scroll(l: &Layout, x: f32, y: f32) -> Option<(usize, i32)> {
    let panes = if l.split { 2 } else { 1 };
    let pane = (0..panes).find(|&p| {
        let (a, b) = pane_x(l, p);
        x >= a + PANE_PAD && x < b - PANE_PAD
    })?;
    let (top, bottom) = (list_top(l, pane), l.height - LIST_BOTTOM_GAP);
    if y < top || y >= bottom {
        None
    } else if y < top + EDGE_PX {
        Some((pane, -1))
    } else if y >= bottom - EDGE_PX {
        Some((pane, 1))
    } else {
        None
    }
}

/// The list's scroll offset after one row's scroll in `dir` from `content_y` (zero or negative),
/// kept in range for `len` rows of `row_px` in a viewport `visible_h` tall.
pub fn scrolled(content_y: f32, dir: i32, len: usize, visible_h: f32, row_px: f32) -> f32 {
    scroll_step(content_y, dir, row_px, len as f32 * row_px, visible_h)
}

/// An offset (zero or negative) after one `step` in `dir`, kept in range for `content` px shown
/// in `visible` px: what `scrolled` does for a list, for the sidebar's rows of mixed heights and
/// for a tab strip (whose "rows" run left to right).
pub fn scroll_step(offset: f32, dir: i32, step: f32, content: f32, visible: f32) -> f32 {
    (offset - dir as f32 * step).clamp((visible - content).min(0.0), 0.0)
}

/// Where a hovering drag scrolls something (direction -1 toward the start: up, or left in a tab
/// strip; 1 toward the end).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// A pane's list (`edge_scroll`).
    List(usize, i32),
    /// The sidebar's list: within `EDGE_PX` of its top or of its footer.
    Side(i32),
    /// A pane's tab strip, while it shows: within `EDGE_PX` of its left or right end.
    Strip(usize, i32),
}

/// `edge_scroll`, plus the sidebar's list and the panes' tab strips. Whether there is anything
/// to scroll (an overflowing sidebar or strip) is the caller's: `scroll_step` leaves an offset
/// that cannot move where it is.
pub fn edge_scroll_all(l: &Layout, s: &Strips, x: f32, y: f32) -> Option<Edge> {
    if x >= 0.0 && x < s.side_w {
        let (top, bottom) = (side_top(l), l.height - SIDE_FOOTER);
        return if y >= top && y < top + EDGE_PX {
            Some(Edge::Side(-1))
        } else if y >= bottom - EDGE_PX && y < bottom {
            Some(Edge::Side(1))
        } else {
            None
        };
    }
    let panes = if l.split { 2 } else { 1 };
    for pane in 0..panes {
        let (a, b) = pane_x(l, pane);
        let (left, right) = (a + PANE_PAD, b - PANE_PAD);
        if l.tabs[pane] && x >= left && x < right && y >= tab_top(l) && y < tab_top(l) + TAB_H {
            return if x < left + EDGE_PX {
                Some(Edge::Strip(pane, -1))
            } else if x >= right - EDGE_PX {
                Some(Edge::Strip(pane, 1))
            } else {
                None
            };
        }
    }
    edge_scroll(l, x, y).map(|(pane, dir)| Edge::List(pane, dir))
}

/// The folder a drop goes to: the row's directory when the row is one (a drive row's name is its
/// root, so `join` gives the drive), else the pane's folder; the drive list itself is not a
/// folder. `row` is the row's name and whether it is a directory. Returns the folder and whether
/// it is the row's (true) or the pane's (false).
pub fn drop_folder(pane_folder: &Path, this_pc: bool, row: Option<(&str, bool)>) -> Option<(PathBuf, bool)> {
    match row {
        Some((name, true)) => Some((pane_folder.join(name), true)),
        _ if this_pc => None,
        _ => Some((pane_folder.to_path_buf(), false)),
    }
}

/// A left-button gesture on a row, per pane: press, then either a release (a click) or a move
/// past the drag threshold (a drag), then the release Slint delivers afterwards. Explorer's
/// rules: pressing an unselected row selects it at once, so the drag carries it; pressing a row
/// that is already selected changes nothing until the release, so a drag carries the whole
/// selection, and the release applies the click only if no drag happened. After a drag, neither
/// the release nor a double-click Slint derives from it may select or open anything.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Gesture {
    #[default]
    Idle,
    /// The left button is down on `row`; `deferred` is the selection to apply on release.
    Pressed { row: usize, deferred: Option<SelOp> },
    /// `DoDragDrop` is running for a drag that started on `row`, with the right button (`right`)
    /// or the left.
    Dragging { row: usize, right: bool },
    /// The drag ended; waiting for the release Slint will deliver.
    Dragged,
}

impl Gesture {
    /// Left press on `row` (`selected`: it was selected before the press). Returns the
    /// selection to apply now.
    pub fn press(&mut self, row: usize, selected: bool, ctrl: bool, shift: bool) -> Option<SelOp> {
        // Ctrl wins over Shift, as the M1 click handler had it.
        let (now, deferred) = if ctrl {
            if selected { (None, Some(SelOp::Toggle)) } else { (Some(SelOp::Toggle), None) }
        } else if shift {
            (Some(SelOp::Range), None)
        } else if selected {
            (None, Some(SelOp::Only))
        } else {
            (Some(SelOp::Only), None)
        };
        *self = Gesture::Pressed { row, deferred };
        now
    }

    /// The pointer moved past the threshold with the button down: start a drag of `row`'s
    /// selection? Only once per press.
    pub fn drag(&mut self, row: usize) -> bool {
        match *self {
            Gesture::Pressed { row: r, .. } if r == row => {
                *self = Gesture::Dragging { row, right: false };
                true
            }
            _ => false,
        }
    }

    /// The pointer moved past the threshold with the right button down on `row` (its press
    /// selected the row, as a right-click does): start a right-button drag? Not while a drag
    /// runs.
    pub fn drag_right(&mut self, row: usize) -> bool {
        if matches!(self, Gesture::Dragging { .. }) {
            return false;
        }
        *self = Gesture::Dragging { row, right: true };
        true
    }

    /// `DoDragDrop` returned (dropped, cancelled or failed). A left-button drag waits for the
    /// release the app hands Slint (`Dragged`); a right-button drag gets no release (the app ends
    /// Slint's grab without one, so no context menu opens), so it is over.
    pub fn drag_ended(&mut self) {
        match self {
            Gesture::Dragging { right: true, .. } => *self = Gesture::Idle,
            Gesture::Dragging { .. } => *self = Gesture::Dragged,
            _ => {}
        }
    }

    /// Left release. Returns the deferred selection to apply (row, op), or None.
    pub fn release(&mut self) -> Option<(usize, SelOp)> {
        let out = match *self {
            Gesture::Pressed { row, deferred } => deferred.map(|op| (row, op)),
            _ => None,
        };
        *self = Gesture::Idle;
        out
    }

    /// Slint's double-click (it fires before the release's pointer event). Open the row?
    pub fn double_click(&mut self) -> bool {
        match self {
            Gesture::Dragging { .. } | Gesture::Dragged => false,
            _ => {
                // Opening navigates; a deferred selection must not land on the new listing.
                *self = Gesture::Idle;
                true
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// COM plumbing
// ---------------------------------------------------------------------------------------------

const IID_IUNKNOWN: GUID = GUID::from_u128(0x00000000_0000_0000_c000_000000000046);
const IID_IDATAOBJECT: GUID = GUID::from_u128(0x0000010e_0000_0000_c000_000000000046);
const IID_IDROPSOURCE: GUID = GUID::from_u128(0x00000121_0000_0000_c000_000000000046);
const IID_IDROPTARGET: GUID = GUID::from_u128(0x00000122_0000_0000_c000_000000000046);

fn same_guid(a: &GUID, b: &GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}

/// IDataObject in declaration order (objidl.h).
#[repr(C)]
struct DataObjectVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    get_data: unsafe extern "system" fn(*mut c_void, *const FORMATETC, *mut STGMEDIUM) -> HRESULT,
    get_data_here: unsafe extern "system" fn(*mut c_void, *const FORMATETC, *mut STGMEDIUM) -> HRESULT,
    query_get_data: unsafe extern "system" fn(*mut c_void, *const FORMATETC) -> HRESULT,
    get_canonical_format_etc: unsafe extern "system" fn(*mut c_void, *const FORMATETC, *mut FORMATETC) -> HRESULT,
    set_data: unsafe extern "system" fn(*mut c_void, *const FORMATETC, *const STGMEDIUM, BOOL) -> HRESULT,
    enum_format_etc: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> HRESULT,
    d_advise: unsafe extern "system" fn(*mut c_void, *const FORMATETC, u32, *mut c_void, *mut u32) -> HRESULT,
    d_unadvise: unsafe extern "system" fn(*mut c_void, u32) -> HRESULT,
    enum_d_advise: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
}

/// IDropTarget (oleidl.h). `POINTL` is passed by value.
#[repr(C)]
struct DropTargetVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    drag_enter: unsafe extern "system" fn(*mut c_void, *mut c_void, u32, POINTL, *mut u32) -> HRESULT,
    drag_over: unsafe extern "system" fn(*mut c_void, u32, POINTL, *mut u32) -> HRESULT,
    drag_leave: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    drop: unsafe extern "system" fn(*mut c_void, *mut c_void, u32, POINTL, *mut u32) -> HRESULT,
}

/// IDropSource (oleidl.h).
#[repr(C)]
struct DropSourceVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    query_continue_drag: unsafe extern "system" fn(*mut c_void, BOOL, u32) -> HRESULT,
    give_feedback: unsafe extern "system" fn(*mut c_void, u32) -> HRESULT,
}

/// Shared QueryInterface/AddRef/Release for our objects: each starts with its vtable pointer,
/// then the reference count, and is a `Box` freed by `free` when the count reaches zero.
#[repr(C)]
struct Header {
    vtbl: *const c_void,
    refs: AtomicU32,
}

unsafe fn add_ref_impl(this: *mut c_void) -> u32 {
    // SAFETY: `this` points at one of our objects, which all start with a Header.
    unsafe { (*(this as *const Header)).refs.fetch_add(1, Ordering::Relaxed) + 1 }
}

unsafe fn release_impl(this: *mut c_void, free: unsafe fn(*mut c_void)) -> u32 {
    // SAFETY: as in `add_ref_impl`; `free` reconstructs and drops the Box at zero.
    unsafe {
        let n = (*(this as *const Header)).refs.fetch_sub(1, Ordering::AcqRel) - 1;
        if n == 0 {
            free(this);
        }
        n
    }
}

unsafe fn query_impl(this: *mut c_void, iid: *const GUID, own: &GUID, out: *mut *mut c_void) -> HRESULT {
    // SAFETY: COM contract: `iid` and `out` are valid pointers.
    unsafe {
        if same_guid(&*iid, &IID_IUNKNOWN) || same_guid(&*iid, own) {
            add_ref_impl(this);
            *out = this;
            S_OK
        } else {
            *out = std::ptr::null_mut();
            E_NOINTERFACE
        }
    }
}

/// An `IDataObject` interface we were handed (not owned): its vtable.
unsafe fn data_vtbl<'a>(data: *mut c_void) -> &'a DataObjectVtbl {
    // SAFETY: the caller passes a live IDataObject.
    unsafe { &**(data as *const *const DataObjectVtbl) }
}

fn hdrop_format() -> FORMATETC {
    FORMATETC { cfFormat: CF_HDROP, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT, lindex: -1, tymed: TYMED_HGLOBAL as u32 }
}

fn registered_format(name: PCWSTR) -> u16 {
    // SAFETY: a NUL-terminated wide constant; the same id comes back for the session.
    unsafe { RegisterClipboardFormatW(name) as u16 }
}

/// The paths a data object offers as `CF_HDROP`, or None when it offers no files.
pub fn read_hdrop(data: *mut c_void) -> Option<Vec<PathBuf>> {
    clipboard::parse_dropfiles(&read_hglobal(data, CF_HDROP)?)
}

/// The bytes a data object offers in format `cf` in an HGLOBAL, or None.
fn read_hglobal(data: *mut c_void, cf: u16) -> Option<Vec<u8>> {
    if data.is_null() {
        return None;
    }
    let fmt = FORMATETC { cfFormat: cf, ..hdrop_format() };
    // SAFETY: a live IDataObject; the medium is released with ReleaseStgMedium after copying.
    unsafe {
        let v = data_vtbl(data);
        if (v.query_get_data)(data, &fmt) != S_OK {
            return None;
        }
        let mut medium: STGMEDIUM = std::mem::zeroed();
        if (v.get_data)(data, &fmt, &mut medium) < 0 {
            return None;
        }
        let mut bytes = None;
        if medium.tymed == TYMED_HGLOBAL as u32 && !medium.u.hGlobal.is_null() {
            let h = medium.u.hGlobal;
            let p = GlobalLock(h) as *const u8;
            if !p.is_null() {
                bytes = Some(std::slice::from_raw_parts(p, GlobalSize(h)).to_vec());
                GlobalUnlock(h);
            }
        }
        ReleaseStgMedium(&mut medium);
        bytes
    }
}

/// A DWORD in a registered format on a data object ("Logical Performed DropEffect"), if set.
fn read_dword(data: *mut c_void, name: PCWSTR) -> Option<u32> {
    let fmt = FORMATETC { cfFormat: registered_format(name), ..hdrop_format() };
    // SAFETY: as in `read_hdrop`.
    unsafe {
        let v = data_vtbl(data);
        let mut medium: STGMEDIUM = std::mem::zeroed();
        if (v.get_data)(data, &fmt, &mut medium) < 0 {
            return None;
        }
        let mut out = None;
        if medium.tymed == TYMED_HGLOBAL as u32 && !medium.u.hGlobal.is_null() {
            let p = GlobalLock(medium.u.hGlobal) as *const u8;
            if !p.is_null() {
                if GlobalSize(medium.u.hGlobal) >= 4 {
                    out = Some(u32::from_le_bytes(std::slice::from_raw_parts(p, 4).try_into().unwrap_or([0; 4])));
                }
                GlobalUnlock(medium.u.hGlobal);
            }
        }
        ReleaseStgMedium(&mut medium);
        out
    }
}

/// Tell the data object what the drop did (`SetData` with a DWORD); best effort, a source that
/// does not take it is not an error.
fn set_dword(data: *mut c_void, name: PCWSTR, value: u32) {
    set_block(data, name, &value.to_le_bytes());
}

/// `SetData` of `bytes` in an HGLOBAL in the registered format `name`; its HRESULT.
fn set_block(data: *mut c_void, name: PCWSTR, bytes: &[u8]) -> HRESULT {
    let fmt = FORMATETC { cfFormat: registered_format(name), ..hdrop_format() };
    // SAFETY: a moveable block of `bytes.len()` handed to SetData with fRelease = TRUE (the data
    // object owns it on success); freed here if SetData refuses it.
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
        if h.is_null() {
            return windows_sys::Win32::Foundation::E_OUTOFMEMORY;
        }
        let p = GlobalLock(h) as *mut u8;
        if p.is_null() {
            GlobalFree(h);
            return windows_sys::Win32::Foundation::E_OUTOFMEMORY;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        GlobalUnlock(h);
        let mut medium: STGMEDIUM = std::mem::zeroed();
        medium.tymed = TYMED_HGLOBAL as u32;
        medium.u.hGlobal = h;
        let hr = (data_vtbl(data).set_data)(data, &fmt, &medium, 1);
        if hr < 0 {
            GlobalFree(h);
        }
        hr
    }
}

// --- A minimal IDataObject: CF_HDROP and/or virtual files (the driver's `dropfiles` and
// `dropvirtual`, and the tests) ------------------------------------------------------------------

#[repr(C)]
struct MiniData {
    header: Header,
    /// The vtable of its `IDataObjectAsyncCapability` face: QueryInterface hands out a pointer to
    /// this field when `can_async` (a source that asks for asynchronous extraction).
    async_vtbl: *const AsyncVtbl,
    can_async: bool,
    /// StartOperation was called and EndOperation not yet; what EndOperation reported.
    in_op: AtomicBool,
    ended: std::sync::Mutex<Option<(HRESULT, u32)>>,
    /// The `CF_HDROP` block (`clipboard::dropfiles_block`), if it offers files.
    hdrop: Option<Vec<u8>>,
    /// The `FileGroupDescriptorW` block (`descriptor_block`), if it offers virtual files, and
    /// each item's `FileContents`.
    descriptor: Option<Vec<u8>>,
    contents: Vec<Vec<u8>>,
    /// Serve `FileContents` as an `IStream` (`SHCreateMemStream`) rather than an HGLOBAL.
    stream: bool,
}

static MINI_DATA_VTBL: DataObjectVtbl = DataObjectVtbl {
    query_interface: hd_query,
    add_ref: hd_add_ref,
    release: hd_release,
    get_data: hd_get_data,
    get_data_here: hd_get_data_here,
    query_get_data: hd_query_get_data,
    get_canonical_format_etc: hd_canonical,
    set_data: hd_set_data,
    enum_format_etc: hd_enum_format_etc,
    d_advise: hd_d_advise,
    d_unadvise: hd_d_unadvise,
    enum_d_advise: hd_enum_d_advise,
};

unsafe fn hd_free(this: *mut c_void) {
    // SAFETY: created by `mini_data` with Box::into_raw.
    drop(unsafe { Box::from_raw(this as *mut MiniData) });
}

unsafe extern "system" fn hd_query(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    // SAFETY: `this` is a MiniData; the async face is a field of it, kept alive by its count.
    unsafe {
        let d = this as *mut MiniData;
        if (*d).can_async && same_guid(&*iid, &IID_IDATAOBJECTASYNCCAPABILITY) {
            add_ref_impl(this);
            *out = std::ptr::addr_of_mut!((*d).async_vtbl) as *mut c_void;
            return S_OK;
        }
        query_impl(this, iid, &IID_IDATAOBJECT, out)
    }
}

// The async face: `this` points at `MiniData::async_vtbl`.
unsafe fn face_base(this: *mut c_void) -> *mut MiniData {
    // SAFETY: the caller passes the async face of a MiniData.
    unsafe { (this as *mut u8).sub(std::mem::offset_of!(MiniData, async_vtbl)) as *mut MiniData }
}
unsafe extern "system" fn ha_query(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    unsafe { hd_query(face_base(this) as *mut c_void, iid, out) }
}
unsafe extern "system" fn ha_add_ref(this: *mut c_void) -> u32 {
    unsafe { add_ref_impl(face_base(this) as *mut c_void) }
}
unsafe extern "system" fn ha_release(this: *mut c_void) -> u32 {
    unsafe { release_impl(face_base(this) as *mut c_void, hd_free) }
}
unsafe extern "system" fn ha_set_async_mode(_: *mut c_void, _: BOOL) -> HRESULT {
    S_OK
}
unsafe extern "system" fn ha_get_async_mode(_: *mut c_void, on: *mut BOOL) -> HRESULT {
    // SAFETY: COM contract: a valid out-pointer (checked for null).
    if !on.is_null() {
        unsafe { *on = 1 };
    }
    S_OK
}
unsafe extern "system" fn ha_start_operation(this: *mut c_void, _: *mut c_void) -> HRESULT {
    // SAFETY: `this` is a MiniData's async face.
    unsafe { (*face_base(this)).in_op.store(true, Ordering::Release) };
    S_OK
}
unsafe extern "system" fn ha_in_operation(this: *mut c_void, on: *mut BOOL) -> HRESULT {
    // SAFETY: as above; a valid out-pointer (checked for null).
    if !on.is_null() {
        unsafe { *on = (*face_base(this)).in_op.load(Ordering::Acquire) as BOOL };
    }
    S_OK
}
unsafe extern "system" fn ha_end_operation(this: *mut c_void, hr: HRESULT, _: *mut c_void, effect: u32) -> HRESULT {
    // SAFETY: as above.
    let d = unsafe { &*face_base(this) };
    d.in_op.store(false, Ordering::Release);
    *d.ended.lock().unwrap_or_else(|e| e.into_inner()) = Some((hr, effect));
    S_OK
}

static MINI_ASYNC_VTBL: AsyncVtbl = AsyncVtbl {
    query_interface: ha_query,
    add_ref: ha_add_ref,
    release: ha_release,
    set_async_mode: ha_set_async_mode,
    get_async_mode: ha_get_async_mode,
    start_operation: ha_start_operation,
    in_operation: ha_in_operation,
    end_operation: ha_end_operation,
};
unsafe extern "system" fn hd_add_ref(this: *mut c_void) -> u32 {
    unsafe { add_ref_impl(this) }
}
unsafe extern "system" fn hd_release(this: *mut c_void) -> u32 {
    unsafe { release_impl(this, hd_free) }
}

/// What a MiniData serves for `fmt`: the bytes, and whether as an IStream. None: not offered.
fn mini_serves(d: &MiniData, fmt: *const FORMATETC) -> Option<(&[u8], bool)> {
    if fmt.is_null() {
        return None;
    }
    // SAFETY: COM contract: a valid FORMATETC pointer (checked for null).
    let f = unsafe { &*fmt };
    let hglobal = f.tymed & TYMED_HGLOBAL as u32 != 0;
    if f.cfFormat == CF_HDROP {
        return d.hdrop.as_deref().filter(|_| hglobal).map(|b| (b, false));
    }
    if f.cfFormat == registered_format(CFSTR_FILEDESCRIPTORW) {
        return d.descriptor.as_deref().filter(|_| hglobal).map(|b| (b, false));
    }
    if f.cfFormat == registered_format(CFSTR_FILECONTENTS) && d.descriptor.is_some() {
        let bytes = d.contents.get(usize::try_from(f.lindex).ok()?)?;
        let stream = d.stream && f.tymed & TYMED_ISTREAM as u32 != 0;
        return (stream || hglobal).then_some((bytes.as_slice(), stream));
    }
    None
}

unsafe extern "system" fn hd_get_data(this: *mut c_void, fmt: *const FORMATETC, medium: *mut STGMEDIUM) -> HRESULT {
    // SAFETY: `this` is a MiniData.
    let d = unsafe { &*(this as *const MiniData) };
    let Some((block, stream)) = mini_serves(d, fmt) else { return DV_E_FORMATETC };
    if medium.is_null() {
        return DV_E_FORMATETC;
    }
    // SAFETY: a fresh moveable block (or memory stream) the caller frees with ReleaseStgMedium
    // (pUnkForRelease null).
    unsafe {
        let mut m: STGMEDIUM = std::mem::zeroed();
        if stream {
            let s = SHCreateMemStream(block.as_ptr(), block.len() as u32);
            if s.is_null() {
                return windows_sys::Win32::Foundation::E_OUTOFMEMORY;
            }
            m.tymed = TYMED_ISTREAM as u32;
            m.u.pstm = s;
        } else {
            let h = GlobalAlloc(GMEM_MOVEABLE, block.len().max(1));
            if h.is_null() {
                return windows_sys::Win32::Foundation::E_OUTOFMEMORY;
            }
            let p = GlobalLock(h) as *mut u8;
            if p.is_null() {
                GlobalFree(h);
                return windows_sys::Win32::Foundation::E_OUTOFMEMORY;
            }
            std::ptr::copy_nonoverlapping(block.as_ptr(), p, block.len());
            GlobalUnlock(h);
            m.tymed = TYMED_HGLOBAL as u32;
            m.u.hGlobal = h;
        }
        *medium = m;
    }
    S_OK
}
unsafe extern "system" fn hd_get_data_here(_: *mut c_void, _: *const FORMATETC, _: *mut STGMEDIUM) -> HRESULT {
    E_NOTIMPL
}
unsafe extern "system" fn hd_query_get_data(this: *mut c_void, fmt: *const FORMATETC) -> HRESULT {
    // SAFETY: `this` is a MiniData.
    if mini_serves(unsafe { &*(this as *const MiniData) }, fmt).is_some() { S_OK } else { DV_E_FORMATETC }
}
unsafe extern "system" fn hd_canonical(_: *mut c_void, _: *const FORMATETC, out: *mut FORMATETC) -> HRESULT {
    if !out.is_null() {
        // SAFETY: COM contract: a valid out-pointer.
        unsafe { (*out).ptd = std::ptr::null_mut() };
    }
    E_NOTIMPL
}
unsafe extern "system" fn hd_set_data(_: *mut c_void, _: *const FORMATETC, _: *const STGMEDIUM, _: BOOL) -> HRESULT {
    E_NOTIMPL
}
unsafe extern "system" fn hd_enum_format_etc(_: *mut c_void, _: u32, out: *mut *mut c_void) -> HRESULT {
    if !out.is_null() {
        // SAFETY: COM contract: a valid out-pointer.
        unsafe { *out = std::ptr::null_mut() };
    }
    E_NOTIMPL
}
unsafe extern "system" fn hd_d_advise(_: *mut c_void, _: *const FORMATETC, _: u32, _: *mut c_void, _: *mut u32) -> HRESULT {
    OLE_E_ADVISENOTSUPPORTED
}
unsafe extern "system" fn hd_d_unadvise(_: *mut c_void, _: u32) -> HRESULT {
    OLE_E_ADVISENOTSUPPORTED
}
unsafe extern "system" fn hd_enum_d_advise(_: *mut c_void, _: *mut *mut c_void) -> HRESULT {
    OLE_E_ADVISENOTSUPPORTED
}

fn mini_data(hdrop: Option<Vec<u8>>, descriptor: Option<Vec<u8>>, contents: Vec<Vec<u8>>, stream: bool, can_async: bool) -> Com {
    let header = Header { vtbl: &MINI_DATA_VTBL as *const DataObjectVtbl as *const c_void, refs: AtomicU32::new(1) };
    let d = MiniData {
        header,
        async_vtbl: &MINI_ASYNC_VTBL,
        can_async,
        in_op: AtomicBool::new(false),
        ended: std::sync::Mutex::new(None),
        hdrop,
        descriptor,
        contents,
        stream,
    };
    Com(Box::into_raw(Box::new(d)) as *mut c_void)
}

/// What a data object made by `virtual_data` with `can_async` was told by EndOperation.
#[cfg(test)]
pub fn mini_ended(data: &Com) -> Option<(HRESULT, u32)> {
    // SAFETY: the caller passes a MiniData.
    *unsafe { &*(data.0 as *const MiniData) }.ended.lock().unwrap_or_else(|e| e.into_inner())
}

/// A data object that serves `paths` as `CF_HDROP` and nothing else.
pub fn hdrop_data(paths: &[PathBuf]) -> Com {
    mini_data(Some(clipboard::dropfiles_block(paths)), None, Vec::new(), false, false)
}

/// A data object that serves virtual files, as a zip view in Explorer or a mail attachment does:
/// `FileGroupDescriptorW` for `items` (a relative name, and the contents of a file or None for a
/// folder) and each file's `FileContents`, as an `IStream` with `stream` or else an HGLOBAL; with
/// `hdrop`, `CF_HDROP` too; with `can_async`, `IDataObjectAsyncCapability` in async mode.
pub fn virtual_data(items: &[(String, Option<Vec<u8>>)], stream: bool, hdrop: Option<&[PathBuf]>, can_async: bool) -> Com {
    let listed: Vec<(&str, Option<u64>)> = items.iter().map(|(n, c)| (n.as_str(), c.as_ref().map(|c| c.len() as u64))).collect();
    let contents = items.iter().map(|(_, c)| c.clone().unwrap_or_default()).collect();
    mini_data(hdrop.map(clipboard::dropfiles_block), Some(descriptor_block(&listed)), contents, stream, can_async)
}

// --- Virtual files: FileGroupDescriptorW + FileContents ------------------------------------------
//
// A source with no files on disk (a zip folder's items, a mail attachment) offers a descriptor per
// item and the contents of each by index. On Drop TwinFM writes them into the target folder itself
// (copies only, no shell dialog). A source that supports `IDataObjectAsyncCapability` and asks for
// it is read on a worker thread after Drop has returned; any other is read inside Drop, as the
// source requires, up to `SYNC_CAP` bytes per drop.

/// Bytes a synchronous virtual drop may write; a larger one is refused before anything is written.
pub const SYNC_CAP: u64 = 256 * 1024 * 1024;

/// One `FILEDESCRIPTORW` (shlobj_core.h, packed): flags at 0, attributes at 36, size (high, low) at
/// 64 and 68, the name (260 wide chars) at 72.
const FD_LEN: usize = 592;

/// One virtual item: its index in the descriptor array (the `lindex` of its contents), its name
/// relative to the drop folder (`\` separated, `.`, `..` and empty parts removed), whether it is a
/// folder, and its size when the source gave one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VirtualItem {
    pub index: usize,
    pub name: String,
    pub dir: bool,
    pub size: Option<u64>,
}

/// The `FileGroupDescriptorW` block for `items` (name, size; None is a folder).
pub fn descriptor_block(items: &[(&str, Option<u64>)]) -> Vec<u8> {
    let mut b = (items.len() as u32).to_le_bytes().to_vec();
    for (name, size) in items {
        let mut d = vec![0u8; FD_LEN];
        let flags = (FD_ATTRIBUTES | FD_FILESIZE | FD_PROGRESSUI) as u32;
        d[0..4].copy_from_slice(&flags.to_le_bytes());
        let attrs = if size.is_none() { FILE_ATTRIBUTE_DIRECTORY } else { FILE_ATTRIBUTE_NORMAL };
        d[36..40].copy_from_slice(&attrs.to_le_bytes());
        let n = size.unwrap_or(0);
        d[64..68].copy_from_slice(&((n >> 32) as u32).to_le_bytes());
        d[68..72].copy_from_slice(&(n as u32).to_le_bytes());
        for (k, u) in name.encode_utf16().take(259).enumerate() {
            d[72 + 2 * k..74 + 2 * k].copy_from_slice(&u.to_le_bytes());
        }
        b.extend(d);
    }
    b
}

/// The items of a `FileGroupDescriptorW` block, None when it is cut short. An item whose name is
/// empty or not a valid file name once cleaned (a drive, `:` for a stream, a wildcard) is left out.
#[cfg(test)]
pub fn parse_descriptors(b: &[u8]) -> Option<Vec<VirtualItem>> {
    parse_descriptors_as(b, true)
}

/// `parse_descriptors` for `FileGroupDescriptorW` (`wide`) or the ANSI `FileGroupDescriptor`, whose
/// records are 332 bytes with a 260-byte name in the ANSI code page.
fn parse_descriptors_as(b: &[u8], wide_names: bool) -> Option<Vec<VirtualItem>> {
    let len = if wide_names { FD_LEN } else { FD_LEN_A };
    let n = u32::from_le_bytes(b.get(0..4)?.try_into().ok()?) as usize;
    if b.len() < 4 + n.checked_mul(len)? {
        return None;
    }
    let u32_at = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    let mut out = Vec::with_capacity(n);
    for index in 0..n {
        let o = 4 + index * len;
        let flags = u32_at(o) as i32;
        let raw = if wide_names {
            let wide: Vec<u16> = (0..260).map(|k| u16::from_le_bytes([b[o + 72 + 2 * k], b[o + 73 + 2 * k]])).take_while(|&u| u != 0).collect();
            String::from_utf16_lossy(&wide)
        } else {
            ansi_to_string(&b[o + 72..o + 332])
        };
        let parts: Vec<&str> = raw.split(['\\', '/']).filter(|p| !p.is_empty() && *p != "." && *p != "..").collect();
        if parts.is_empty() || parts.iter().any(|p| crate::fileops::invalid_name(p).is_some()) {
            eprintln!("[dnd] virtual drop: item {index} \"{raw}\" left out (not a file name)");
            continue;
        }
        let dir = flags & FD_ATTRIBUTES != 0 && u32_at(o + 36) & FILE_ATTRIBUTE_DIRECTORY != 0;
        let size = (flags & FD_FILESIZE != 0).then(|| (u32_at(o + 64) as u64) << 32 | u32_at(o + 68) as u64);
        out.push(VirtualItem { index, name: parts.join("\\"), dir, size });
    }
    Some(out)
}

/// The ANSI `FILEDESCRIPTORA`'s length: the same fields up to the name, then 260 bytes.
const FD_LEN_A: usize = 332;

/// A NUL-terminated name in the ANSI code page.
fn ansi_to_string(b: &[u8]) -> String {
    let b = &b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())];
    let mut w = vec![0u16; b.len().max(1)];
    // SAFETY: input and output buffers of the given lengths.
    let n = unsafe { MultiByteToWideChar(CP_ACP, 0, b.as_ptr(), b.len() as i32, w.as_mut_ptr(), w.len() as i32) };
    String::from_utf16_lossy(&w[..n.max(0) as usize])
}

/// The virtual items a data object offers (`FileGroupDescriptorW`, else the ANSI
/// `FileGroupDescriptor`), or None when it offers neither.
pub fn read_descriptors(data: *mut c_void) -> Option<Vec<VirtualItem>> {
    if let Some(b) = read_hglobal(data, registered_format(CFSTR_FILEDESCRIPTORW)) {
        return parse_descriptors_as(&b, true);
    }
    parse_descriptors_as(&read_hglobal(data, registered_format(CFSTR_FILEDESCRIPTORA))?, false)
}

/// The items of a `Shell IDList Array` (a CIDA: the folder's PIDL, then each item's relative to
/// it) when every one of them lies inside a zip archive: their parsing paths, which the extraction
/// job takes. None otherwise. A shell data object made over items of a zip folder
/// (`SHCreateDataObject`) offers nothing else.
fn read_archive_idlist(data: *mut c_void) -> Option<Vec<PathBuf>> {
    let b = read_hglobal(data, registered_format(CFSTR_SHELLIDLIST))?;
    let u32_at = |o: usize| -> Option<usize> { Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?) as usize) };
    let n = u32_at(0)?;
    let at = |i: usize| -> Option<*const ITEMIDLIST> {
        let off = u32_at(4 + 4 * i)?;
        (off < b.len()).then(|| b[off..].as_ptr() as *const ITEMIDLIST)
    };
    let folder = at(0)?;
    let mut paths = Vec::with_capacity(n);
    for i in 1..=n {
        // SAFETY: PIDLs inside the CIDA block (bounds-checked offsets; the shell reads each up to
        // its terminating zero-length id); the combined PIDL and the name are freed here.
        let path = unsafe {
            let abs = ILCombine(folder, at(i)?);
            if abs.is_null() {
                return None;
            }
            let mut s: *mut u16 = std::ptr::null_mut();
            let hr = SHGetNameFromIDList(abs, SIGDN_DESKTOPABSOLUTEPARSING, &mut s);
            ILFree(abs);
            if hr < 0 || s.is_null() {
                return None;
            }
            let len = (0..).take_while(|&k| *s.add(k) != 0).count();
            let p = PathBuf::from(String::from_utf16_lossy(std::slice::from_raw_parts(s, len)));
            windows_sys::Win32::System::Com::CoTaskMemFree(s as *const c_void);
            p
        };
        if !crate::fs::archive::within(&path) {
            return None;
        }
        paths.push(path);
    }
    (!paths.is_empty()).then_some(paths)
}

/// What a drop offers. Files on disk (`CF_HDROP`) win over virtual files when both are there.
#[derive(Debug, PartialEq, Eq)]
pub enum Offer {
    Files(Vec<PathBuf>),
    Virtual(Vec<VirtualItem>),
    /// Items inside a zip archive, by their in-archive paths, copied by the extraction job rather
    /// than read here: our own drag out of an archive, or a source offering only their PIDLs.
    Archive(Vec<PathBuf>),
    Nothing,
}

pub fn offer(data: *mut c_void) -> Offer {
    if let Some(paths) = read_hdrop(data) {
        return Offer::Files(paths);
    }
    match read_descriptors(data) {
        Some(items) if !items.is_empty() => Offer::Virtual(items),
        // Items of a zip folder offered as PIDLs only: the extraction job, as for our own drags.
        _ => read_archive_idlist(data).map_or(Offer::Nothing, Offer::Archive),
    }
}

thread_local! {
    /// While our own drag out of an archive runs: the in-archive paths its data object was made
    /// from. Any drag reaching our target meanwhile is that drag (OLE's loop is ours).
    static OWN_ARCHIVE_SOURCES: RefCell<Option<Vec<PathBuf>>> = const { RefCell::new(None) };
}

/// What the drop target makes of a data object: `offer`, except that a drag out of an archive is
/// its in-archive paths, for the extraction job (the shell's progress dialog, off the UI thread,
/// with an undo record) instead of the synchronous virtual-file reader: our own drag (its paths
/// recorded), or another program's whose data object carries the items' PIDLs beside the virtual
/// files, as a shell folder's own does (P7-M3: Explorer's zip and 7z views).
fn target_offer(data: *mut c_void) -> Offer {
    match offer(data) {
        Offer::Virtual(v) => match OWN_ARCHIVE_SOURCES.with(|s| s.borrow().clone()) {
            Some(own) => Offer::Archive(own),
            None => read_archive_idlist(data).map_or(Offer::Virtual(v), Offer::Archive),
        },
        o => o,
    }
}

/// A virtual drop is a copy whatever the source allows (a move is done, and reported, as a copy).
pub fn virtual_effect(allowed: u32) -> u32 {
    if allowed & (COPY | MOVE) != 0 { COPY } else { NONE }
}

/// The known bytes of a drop, when they exceed `SYNC_CAP`.
pub fn over_cap(items: &[VirtualItem]) -> Option<u64> {
    let total: u64 = items.iter().filter_map(|i| i.size).sum();
    (total > SYNC_CAP).then_some(total)
}

/// `leaf` in `dir`, or `leaf (2)`, `leaf (3)`... (before a file's extension) when it is taken.
pub fn free_name(dir: &Path, leaf: &str, is_dir: bool) -> PathBuf {
    let first = dir.join(leaf);
    if std::fs::symlink_metadata(&first).is_err() {
        return first;
    }
    let (stem, ext) = match leaf.rfind('.') {
        Some(i) if !is_dir && i > 0 => (&leaf[..i], &leaf[i..]),
        _ => (leaf, ""),
    };
    (2..).map(|k| dir.join(format!("{stem} ({k}){ext}"))).find(|p| std::fs::symlink_metadata(p).is_err()).unwrap_or(first)
}

/// Write the items into `dest`: folders made as needed, a name already taken getting " (2)" (a
/// folder renamed so takes its contents with it), each file's contents read from the data object
/// (`FileContents`, an IStream or an HGLOBAL). `progress(k, n, path)` after each file. Stops at the
/// first failure, or when the bytes written pass `cap`. Ok((files, bytes)).
pub fn extract(
    data: *mut c_void,
    items: &[VirtualItem],
    dest: &Path,
    cap: Option<u64>,
    mut progress: impl FnMut(usize, usize, &Path),
) -> Result<(usize, u64), String> {
    let files = items.iter().filter(|i| !i.dir).count();
    // A folder of the drop (its lowercase relative name) and where it went.
    let mut made: std::collections::HashMap<String, PathBuf> = std::collections::HashMap::new();
    let (mut written, mut bytes) = (0usize, 0u64);
    for it in items {
        let (parent, leaf) = it.name.rsplit_once('\\').unwrap_or(("", &it.name));
        let dir = match made.get(&parent.to_lowercase()) {
            _ if parent.is_empty() => dest.to_path_buf(),
            Some(d) => d.clone(),
            None => {
                // A folder the descriptors imply but do not list: below the nearest one they do
                // list (which may have been renamed), else below `dest`.
                let parts: Vec<&str> = parent.split('\\').collect();
                let d = (1..parts.len())
                    .rev()
                    .find_map(|k| Some(made.get(&parts[..k].join("\\").to_lowercase())?.join(parts[k..].join("\\"))))
                    .unwrap_or_else(|| dest.join(parent));
                std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
                made.insert(parent.to_lowercase(), d.clone());
                d
            }
        };
        let target = free_name(&dir, leaf, it.dir);
        if it.dir {
            std::fs::create_dir(&target).map_err(|e| format!("{}: {e}", target.display()))?;
            made.insert(it.name.to_lowercase(), target);
            continue;
        }
        let mut out = std::fs::File::create(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        let limit = cap.map(|c| c.saturating_sub(bytes));
        bytes += read_contents(data, it, &mut out, limit).map_err(|e| format!("{}: {e}", target.display()))?;
        written += 1;
        progress(written, files, &target);
    }
    Ok((written, bytes))
}

/// One item's `FileContents` into `out`; the bytes written. `limit`: refuse past that many.
fn read_contents(data: *mut c_void, it: &VirtualItem, out: &mut std::fs::File, limit: Option<u64>) -> Result<u64, String> {
    use std::io::Write;
    let fmt = FORMATETC {
        cfFormat: registered_format(CFSTR_FILECONTENTS),
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT,
        lindex: it.index as i32,
        tymed: (TYMED_ISTREAM | TYMED_HGLOBAL) as u32,
    };
    let over = |n: u64| limit.is_some_and(|l| n > l);
    // SAFETY: a live IDataObject; the medium is released with ReleaseStgMedium on every path.
    unsafe {
        let mut medium: STGMEDIUM = std::mem::zeroed();
        let hr = (data_vtbl(data).get_data)(data, &fmt, &mut medium);
        if hr < 0 {
            return Err(format!("FileContents {} failed: 0x{:08X}", it.index, hr as u32));
        }
        let mut n = 0u64;
        let res = if medium.tymed == TYMED_ISTREAM as u32 && !medium.u.pstm.is_null() {
            let s = medium.u.pstm;
            let read = (**(s as *const *const StreamVtbl)).read;
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let mut got = 0u32;
                let hr = read(s, buf.as_mut_ptr() as *mut c_void, buf.len() as u32, &mut got);
                if hr < 0 {
                    break Err(format!("reading the contents failed: 0x{:08X}", hr as u32));
                }
                if got == 0 {
                    break Ok(());
                }
                n += got as u64;
                if over(n) {
                    break Err("over the drop's size limit".to_string());
                }
                if let Err(e) = out.write_all(&buf[..got as usize]) {
                    break Err(e.to_string());
                }
            }
        } else if medium.tymed == TYMED_HGLOBAL as u32 && !medium.u.hGlobal.is_null() {
            let h = medium.u.hGlobal;
            let p = GlobalLock(h) as *const u8;
            // The block may be larger than the file: the descriptor's size, when given, is the file's.
            let len = it.size.map_or(GlobalSize(h), |s| (s as usize).min(GlobalSize(h)));
            n = len as u64;
            let r = if p.is_null() {
                Err("GlobalLock failed".to_string())
            } else if over(n) {
                Err("over the drop's size limit".to_string())
            } else {
                out.write_all(std::slice::from_raw_parts(p, len)).map_err(|e| e.to_string())
            };
            if !p.is_null() {
                GlobalUnlock(h);
            }
            r
        } else {
            Err(format!("FileContents {} came in an unsupported medium ({})", it.index, medium.tymed))
        };
        ReleaseStgMedium(&mut medium);
        res.map(|()| n)
    }
}

/// ISequentialStream, up to Read (every IStream starts with it).
#[repr(C)]
struct StreamVtbl {
    _query_interface: *const c_void,
    _add_ref: *const c_void,
    _release: *const c_void,
    read: unsafe extern "system" fn(*mut c_void, *mut c_void, u32, *mut u32) -> HRESULT,
}

const IID_IDATAOBJECTASYNCCAPABILITY: GUID = GUID::from_u128(0x3d8b0590_f691_11d2_8ea9_006097df5bd4);

/// IDataObjectAsyncCapability (shldisp.h).
#[repr(C)]
struct AsyncVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    set_async_mode: unsafe extern "system" fn(*mut c_void, BOOL) -> HRESULT,
    get_async_mode: unsafe extern "system" fn(*mut c_void, *mut BOOL) -> HRESULT,
    start_operation: unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT,
    in_operation: unsafe extern "system" fn(*mut c_void, *mut BOOL) -> HRESULT,
    end_operation: unsafe extern "system" fn(*mut c_void, HRESULT, *mut c_void, u32) -> HRESULT,
}

/// Virtual drops still writing on a worker (the driver's `waitops` waits for them).
static VIRTUAL_RUNNING: AtomicU32 = AtomicU32::new(0);

pub fn virtual_drops_running() -> bool {
    VIRTUAL_RUNNING.load(Ordering::Acquire) > 0
}

/// "Extracted N files" or the failure, for the pane's note.
fn virtual_result(res: &Result<(usize, u64), String>) -> String {
    match res {
        Ok((n, _)) => format!("Extracted {n} file{}", if *n == 1 { "" } else { "s" }),
        Err(e) => format!("Extracting failed: {e}"),
    }
}

/// The drop of virtual files into `dest` (the spot's folder, noted in `pane`). Async when the source
/// asks for it: `StartOperation`, the data object marshalled to a worker that writes the files,
/// reports "Extracting N of M..." through the event loop, and ends with `EndOperation`. Otherwise
/// inside this call (the source's requirement), refused over `SYNC_CAP`. **Called from Drop** with
/// no app-state borrow held; it touches no app state (notes go through the event loop).
fn virtual_drop(data: *mut c_void, items: Vec<VirtualItem>, pane: usize, dest: PathBuf) {
    let files = items.iter().filter(|i| !i.dir).count();
    let known: u64 = items.iter().filter_map(|i| i.size).sum();
    let note = move |text: String| {
        let _ = slint::invoke_from_event_loop(move || crate::drop_note(pane, text));
    };
    // SAFETY: a live IDataObject for the duration of Drop; the async interface is released here.
    let asynced = unsafe {
        let mut p: *mut c_void = std::ptr::null_mut();
        if (data_vtbl(data).query_interface)(data, &IID_IDATAOBJECTASYNCCAPABILITY, &mut p) < 0 || p.is_null() {
            false
        } else {
            let a = Com(p);
            let v = a.vtbl::<AsyncVtbl>();
            let mut on: BOOL = 0;
            let got = (v.get_async_mode)(a.0, &mut on);
            let started = if got >= 0 && on != 0 { (v.start_operation)(a.0, std::ptr::null_mut()) } else { -1 };
            eprintln!("[dnd] virtual drop: GetAsyncMode = 0x{:08X} ({on}), StartOperation = 0x{:08X}", got as u32, started as u32);
            started >= 0
        }
    };
    eprintln!("[dnd] virtual drop: {files} file(s), {known} bytes, {}", if asynced { "async" } else { "sync" });
    if !asynced {
        if let Some(total) = over_cap(&items) {
            eprintln!("[dnd] virtual drop refused: {total} bytes is over the {SYNC_CAP}-byte limit for a synchronous source");
            return note(format!("Not extracted: {} is over the 256 MB limit for this source", crate::model::format_size(total)));
        }
        let t0 = std::time::Instant::now();
        let res = extract(data, &items, &dest, Some(SYNC_CAP), |k, n, p| eprintln!("[dnd] virtual drop wrote {} ({k} of {n})", p.display()));
        eprintln!("[dnd] virtual drop done in {:.0} ms: {res:?}", crate::perf::elapsed_ms(t0));
        return note(virtual_result(&res));
    }
    let mut stream: *mut c_void = std::ptr::null_mut();
    // SAFETY: marshals the live IDataObject for one unmarshal on the worker.
    let hr = unsafe { CoMarshalInterThreadInterfaceInStream(&IID_IDATAOBJECT, data, &mut stream) };
    if hr < 0 {
        eprintln!("[dnd] virtual drop refused: CoMarshalInterThreadInterfaceInStream 0x{:08X}", hr as u32);
        // SAFETY: ends the operation StartOperation began, on the object Drop was handed.
        unsafe { end_async(data, hr, NONE) };
        return note(format!("Extracting failed: 0x{:08X}", hr as u32));
    }
    let stream = stream as usize;
    VIRTUAL_RUNNING.fetch_add(1, Ordering::AcqRel);
    std::thread::Builder::new()
        .name("virtual-drop".into())
        .spawn(move || {
            // SAFETY: COM for this worker (multithreaded: the proxy needs no message pump here); the
            // stream is unmarshalled once (which releases it) and the proxy released before
            // CoUninitialize.
            unsafe {
                let init = CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32);
                let mut p: *mut c_void = std::ptr::null_mut();
                let hr = CoGetInterfaceAndReleaseStream(stream as *mut c_void, &IID_IDATAOBJECT, &mut p);
                let res = if hr < 0 || p.is_null() {
                    Err(format!("CoGetInterfaceAndReleaseStream 0x{:08X}", hr as u32))
                } else {
                    let proxy = Com(p);
                    let t0 = std::time::Instant::now();
                    let res = extract(proxy.0, &items, &dest, None, |k, n, path| {
                        eprintln!("[dnd] virtual drop wrote {} ({k} of {n})", path.display());
                        note(format!("Extracting {k} of {n}..."));
                    });
                    eprintln!("[dnd] virtual drop done in {:.0} ms: {res:?}", crate::perf::elapsed_ms(t0));
                    let (hr, effect) = if res.is_ok() { (S_OK, COPY) } else { (windows_sys::Win32::Foundation::E_FAIL, NONE) };
                    end_async(proxy.0, hr, effect);
                    res
                };
                note(virtual_result(&res));
                if init >= 0 {
                    CoUninitialize();
                }
            }
            VIRTUAL_RUNNING.fetch_sub(1, Ordering::AcqRel);
        })
        .expect("spawn virtual drop thread");
}

/// As a drag source: tell targets they may extract from the data object asynchronously
/// (`SetAsyncMode(TRUE)`; the zip folder's object supports it but starts out synchronous).
fn allow_async(data: *mut c_void) {
    // SAFETY: a live IDataObject; the interface is released by `Com`.
    unsafe {
        let mut p: *mut c_void = std::ptr::null_mut();
        if (data_vtbl(data).query_interface)(data, &IID_IDATAOBJECTASYNCCAPABILITY, &mut p) >= 0 && !p.is_null() {
            let a = Com(p);
            let hr = (a.vtbl::<AsyncVtbl>().set_async_mode)(a.0, 1);
            eprintln!("[dnd] the zip folder's data object: SetAsyncMode(TRUE) = 0x{:08X}", hr as u32);
        }
    }
}

/// `IDataObjectAsyncCapability::EndOperation` on a data object (or its proxy).
unsafe fn end_async(data: *mut c_void, hr: HRESULT, effect: u32) {
    // SAFETY: the caller passes a live IDataObject; the interface is released by `Com`.
    unsafe {
        let mut p: *mut c_void = std::ptr::null_mut();
        if (data_vtbl(data).query_interface)(data, &IID_IDATAOBJECTASYNCCAPABILITY, &mut p) >= 0 && !p.is_null() {
            let a = Com(p);
            let r = (a.vtbl::<AsyncVtbl>().end_operation)(a.0, hr, std::ptr::null_mut(), effect);
            eprintln!("[dnd] virtual drop: EndOperation(0x{:08X}, {}) = 0x{:08X}", hr as u32, effect_name(effect), r as u32);
        }
    }
}

// --- The drop target ----------------------------------------------------------------------------

/// What a drop spot highlights: a row of the pane's list (-1: the whole list, the drop goes to
/// the pane's folder), a tab of its strip, a sidebar row, or a crumb of its path bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Row(i32),
    Tab(usize),
    Side(usize),
    Crumb(usize),
}

/// Where a drop at a window point goes, as the app decides it (`crate::drop_spot`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DropSpot {
    /// The pane the drop is noted in (for a sidebar row, the active pane).
    pub pane: usize,
    pub mark: Mark,
    pub dest: PathBuf,
    /// A drag held here opens it (`Spring`): a folder row or tile (`Mark::Row` with a row), a tab
    /// that is not the pane's active one, and since P11-M3 a sidebar row (in the active pane) and a
    /// crumb other than the folder shown.
    pub springs: bool,
}

/// How long a drag must hover a folder row, a tile or a tab before it opens there (Explorer's is
/// about a second; the open question in phase-5-plan.md says the number is set at the console).
pub const SPRING_MS: u64 = 1000;

/// Spring-loaded folders: the drop target arms it when the spot changes to something that
/// opens, and asks it on every DragOver (OLE calls DragOver on a timer even while the mouse rests,
/// so that call is the clock: no timer of our own). It fires once per target: the same target
/// does not fire again until the pointer has left it.
#[derive(Debug)]
pub struct Spring<T> {
    armed: Option<(T, std::time::Instant)>,
    fired: Option<T>,
}

impl<T> Default for Spring<T> {
    fn default() -> Self {
        Spring { armed: None, fired: None }
    }
}

impl<T: Clone + PartialEq> Spring<T> {
    /// The pointer is over `target`, which opens. True when that armed the spring (a new target),
    /// false while it was armed for it already or has fired for it.
    pub fn arm(&mut self, target: T, now: std::time::Instant) -> bool {
        if self.armed.as_ref().is_some_and(|(t, _)| *t == target) || self.fired.as_ref() == Some(&target) {
            return false;
        }
        self.armed = Some((target, now));
        self.fired = None;
        true
    }

    /// The pointer left the target (to anything that does not open, or out of the window). True
    /// when the spring was armed (and had not fired yet).
    pub fn disarm(&mut self) -> bool {
        self.fired = None;
        self.armed.take().is_some()
    }

    /// The target and how long it was hovered, once the hover has lasted `SPRING_MS`.
    pub fn check(&mut self, now: std::time::Instant) -> Option<(T, std::time::Duration)> {
        let waited = now.saturating_duration_since(self.armed.as_ref()?.1);
        if waited < std::time::Duration::from_millis(SPRING_MS) {
            return None;
        }
        let (target, _) = self.armed.take()?;
        self.fired = Some(target.clone());
        Some((target, waited))
    }
}

#[repr(C)]
struct DropTarget {
    header: Header,
    hwnd: isize,
    /// What the data object being dragged over us offers, read at DragEnter: files (`CF_HDROP`),
    /// virtual files (`FileGroupDescriptorW`), or nothing, which every point refuses.
    offered: RefCell<Offer>,
    /// The last (spot, effect) logged by DragOver, so it logs changes only.
    last: RefCell<Option<(Option<DropSpot>, u32)>>,
    /// The last DragEnter or DragOver had the right button down: a right-button drag, whose
    /// Drop asks what to do (the button is already up when Drop is called).
    right: Cell<bool>,
    /// When DragOver last scrolled a list (auto-scroll near its edges, `SCROLL_EVERY`).
    scrolled_at: Cell<Option<std::time::Instant>>,
    /// The spring-loaded folder or tab under the drag (P5-M3).
    spring: RefCell<Spring<DropSpot>>,
    // P5-M3, drop descriptions and volumes (per drag, reset at DragEnter).
    /// The data object DragEnter was handed, with a reference of ours (released in DragLeave and
    /// Drop): DragOver is not handed it, and the drop description is set on it. Null between drags.
    data: Cell<*mut c_void>,
    /// The drop description last set on it (type, insert), so it is set on change only, and
    /// whether a SetData failure was logged this drag.
    described: RefCell<Option<(i32, String)>>,
    describe_failed: Cell<bool>,
    /// The sources' volume (`volume_key` of the first file), looked up at DragEnter, and the
    /// last destination's, looked up once per distinct folder.
    src_volume: RefCell<Option<String>>,
    dest_volume: RefCell<VolumeCache>,
}

static DROP_TARGET_VTBL: DropTargetVtbl = DropTargetVtbl {
    query_interface: dt_query,
    add_ref: dt_add_ref,
    release: dt_release,
    drag_enter: dt_drag_enter,
    drag_over: dt_drag_over,
    drag_leave: dt_drag_leave,
    drop: dt_drop,
};

unsafe fn dt_free(this: *mut c_void) {
    // SAFETY: created by `register` with Box::into_raw.
    drop(unsafe { Box::from_raw(this as *mut DropTarget) });
}

unsafe extern "system" fn dt_query(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    unsafe { query_impl(this, iid, &IID_IDROPTARGET, out) }
}
unsafe extern "system" fn dt_add_ref(this: *mut c_void) -> u32 {
    unsafe { add_ref_impl(this) }
}
unsafe extern "system" fn dt_release(this: *mut c_void) -> u32 {
    unsafe { release_impl(this, dt_free) }
}

impl DropTarget {
    /// Screen point to client px.
    fn client(&self, pt: POINTL) -> POINT {
        let mut p = POINT { x: pt.x, y: pt.y };
        // SAFETY: plain Win32 call on our own window with a valid in/out pointer.
        unsafe { ScreenToClient(self.hwnd as HWND, &mut p) };
        p
    }

    /// The spot and effect at screen point `pt` for the cached sources.
    fn evaluate(&self, keys: u32, pt: POINTL, allowed: u32) -> (Option<DropSpot>, u32) {
        let p = self.client(pt);
        let spot = crate::drop_spot(p.x, p.y);
        // Into a zip (the only archive a spot can be in, P7-M3): files are copied unless Shift
        // asks for a move, as onto another volume; virtual files and items of another archive
        // are refused.
        let into_archive = spot.as_ref().is_some_and(|s| crate::fs::archive::inside(&s.dest));
        let effect = match (&*self.offered.borrow(), &spot) {
            (Offer::Files(s), Some(spot)) => effect_for(s, &spot.dest, keys, allowed, !into_archive && self.same_volume(&spot.dest)),
            (Offer::Virtual(_) | Offer::Archive(_), Some(_)) if !into_archive => virtual_effect(allowed),
            _ => NONE,
        };
        (spot, effect)
    }

    /// The sources (looked up at DragEnter) and `dest` share a volume; `dest`'s key is looked up
    /// once per distinct folder.
    fn same_volume(&self, dest: &Path) -> bool {
        let Some(src) = self.src_volume.borrow().clone() else { return false };
        let (key, looked_up) = self.dest_volume.borrow_mut().key(dest, volume_key);
        if looked_up {
            eprintln!("[dnd] volumes: dest {} on {key}", dest.display());
        }
        key == src
    }

    /// Set the drop description on the drag's data object when it differs from the last one set.
    /// A data object that refuses SetData (some sources' do) is logged once per drag and is no
    /// error: the image then shows no text.
    fn describe(&self, d: Description) {
        let data = self.data.get();
        if data.is_null() || self.described.borrow().as_ref().is_some_and(|(k, i)| *k == d.kind && *i == d.insert) {
            return;
        }
        *self.described.borrow_mut() = Some((d.kind, d.insert.clone()));
        let hr = set_block(data, CFSTR_DROPDESCRIPTION, &description_block(&d));
        if hr < 0 {
            if !self.describe_failed.replace(true) {
                eprintln!("[dnd] drop description: SetData failed 0x{:08X}", hr as u32);
            }
            return;
        }
        match d.kind {
            DROPIMAGE_MOVE => eprintln!("[dnd] drop description: Move to \"{}\"", d.insert),
            DROPIMAGE_COPY => eprintln!("[dnd] drop description: Copy to \"{}\"", d.insert),
            DROPIMAGE_INVALID => eprintln!("[dnd] drop description: cleared"),
            _ => eprintln!("[dnd] drop description: none"),
        }
    }

    /// The drag left or dropped: give the description back to the source (when one was set) and
    /// release our reference to the data object.
    fn end_drag(&self) {
        if self.described.borrow().is_some() {
            self.describe(Description::cleared());
        }
        let data = self.data.replace(std::ptr::null_mut());
        if !data.is_null() {
            // SAFETY: the reference DragEnter took.
            unsafe { (data_vtbl(data).release)(data) };
        }
    }

    fn over(&self, keys: u32, pt: POINTL, effect: *mut u32) {
        // A right-button drag: the button is still down in DragOver; Drop, after its release,
        // goes by this.
        self.right.set(keys & MK_RBUTTON != 0);
        // SAFETY: COM contract: `effect` is a valid in/out pointer (checked for null).
        let allowed = if effect.is_null() { NONE } else { unsafe { *effect } };
        let (spot, e) = self.evaluate(keys, pt, allowed);
        let highlight = spot.as_ref().filter(|_| e != NONE).map(|s| (s.pane, s.mark));
        crate::drop_highlight(highlight);
        // Before the target helper's DragEnter/DragOver, which read it.
        self.describe(description(e, spot.as_ref().map(|s| s.dest.as_path())));
        let changed = self.last.borrow().as_ref() != Some(&(spot.clone(), e));
        if changed {
            eprintln!("[dnd] over {} keys=0x{keys:x} allowed=0x{allowed:x} -> effect {}", describe(&spot), effect_name(e));
        }
        self.spring_over(spot.as_ref().filter(|s| s.springs && e != NONE));
        if changed {
            *self.last.borrow_mut() = Some((spot, e));
        }
        if !effect.is_null() {
            // SAFETY: as above.
            unsafe { *effect = e };
        }
    }

    /// The spring-loaded folder: arm it over a spot that opens (one a drop would land on: a
    /// refused spot neither lights up nor opens), open it once the hover has lasted `SPRING_MS`.
    /// The open runs from the event loop, as the closures the drag loop already runs do, so
    /// nothing here touches the app state; the next DragOver evaluates the spot against whatever
    /// the pane then shows.
    fn spring_over(&self, target: Option<&DropSpot>) {
        let now = std::time::Instant::now();
        let mut spring = self.spring.borrow_mut();
        match target {
            Some(t) => {
                if spring.arm(t.clone(), now) {
                    eprintln!("[dnd] spring armed: {}", describe(&Some(t.clone())));
                }
            }
            None => {
                if spring.disarm() {
                    eprintln!("[dnd] spring disarmed (moved away)");
                }
            }
        }
        let Some((t, waited)) = spring.check(now) else { return };
        drop(spring);
        let ms = waited.as_millis();
        match t.mark {
            Mark::Tab(i) => {
                eprintln!("[dnd] spring-loaded: pane {} shows tab {i} after {ms} ms", t.pane);
                crate::spring_tab(t.pane, i);
            }
            _ => {
                eprintln!("[dnd] spring-loaded: pane {} opens {} after {ms} ms", t.pane, t.dest.display());
                crate::spring_open(t.pane, t.dest);
            }
        }
    }

    /// The drag left or dropped: an armed spring goes, saying why.
    fn unspring(&self, why: &str) {
        if self.spring.borrow_mut().disarm() {
            eprintln!("[dnd] spring disarmed ({why})");
        }
    }
}

fn describe(spot: &Option<DropSpot>) -> String {
    match spot {
        None => "nowhere".into(),
        Some(s) => match s.mark {
            Mark::Row(r) if r >= 0 => format!("pane {} row {r} ({})", s.pane, s.dest.display()),
            Mark::Row(_) => format!("pane {} ({})", s.pane, s.dest.display()),
            Mark::Tab(i) => format!("pane {} tab {i} ({})", s.pane, s.dest.display()),
            Mark::Side(i) => format!("sidebar row {i} ({}), noted in pane {}", s.dest.display(), s.pane),
            Mark::Crumb(i) => format!("pane {} crumb {i} ({})", s.pane, s.dest.display()),
        },
    }
}

pub fn effect_name(e: u32) -> &'static str {
    match e {
        NONE => "none",
        COPY => "copy",
        MOVE => "move",
        _ => "other",
    }
}

unsafe extern "system" fn dt_drag_enter(this: *mut c_void, data: *mut c_void, keys: u32, pt: POINTL, effect: *mut u32) -> HRESULT {
    // SAFETY: `this` is our DropTarget (OLE calls us through the vtable we registered).
    let t = unsafe { &*(this as *const DropTarget) };
    let offered = target_offer(data);
    eprintln!(
        "[dnd] DragEnter: {}",
        match &offered {
            Offer::Files(s) => format!("{} file(s), first {}", s.len(), s.first().map(|p| p.display().to_string()).unwrap_or_default()),
            Offer::Virtual(v) => format!("{} virtual item(s) (FileGroupDescriptorW, no CF_HDROP), first {}", v.len(), v[0].name),
            Offer::Archive(p) => format!("{} item(s) inside an archive (extraction job), first {}", p.len(), p[0].display()),
            Offer::Nothing => "no CF_HDROP and no virtual files, refused".into(),
        }
    );
    let nothing = matches!(offered, Offer::Nothing);
    // The sources' volume, once per drag (a drag whose first source changes is a new DragEnter).
    let src = match &offered {
        Offer::Files(s) => s.first().map(|f| volume_key(f)),
        _ => None,
    };
    if let Some(k) = &src {
        eprintln!("[dnd] volumes: sources on {k}");
    }
    *t.src_volume.borrow_mut() = src;
    *t.offered.borrow_mut() = offered;
    *t.last.borrow_mut() = None;
    // Keep the data object for DragOver's drop descriptions (a DragEnter without a DragLeave
    // before it gives the old one back first).
    t.end_drag();
    if !data.is_null() {
        // SAFETY: a live IDataObject; the reference is released in `end_drag`.
        unsafe { (data_vtbl(data).add_ref)(data) };
    }
    t.data.set(data);
    *t.described.borrow_mut() = None;
    t.describe_failed.set(false);
    // SAFETY: COM contract: a valid in/out pointer (checked for null).
    let allowed = if effect.is_null() { NONE } else { unsafe { *effect } };
    t.over(keys, pt, effect);
    // A drag whose data object offers nothing we take (no files, no virtual files, no items inside
    // an archive): say what it does offer, once per DragEnter. A spot that refuses is not this.
    if nothing {
        eprintln!("[dnd] DragEnter refused: allowed=0x{allowed:x} keys=0x{keys:x}; {}", format_report(data));
    }
    // The shell paints the source's drag image over our window (after our own work for the call).
    target_helper_call(HelperCall::Enter(t.hwnd, data), pt, out_effect(effect));
    S_OK
}

/// Every format a data object enumerates (name or standard id, tymed mask, lindex, aspect) and
/// `QueryGetData` for the formats a file drop can come in, for the refusal log.
fn format_report(data: *mut c_void) -> String {
    let name = |cf: u16| -> String {
        let mut buf = [0u16; 128];
        // SAFETY: a writable buffer of the advertised length.
        let n = unsafe { windows_sys::Win32::System::DataExchange::GetClipboardFormatNameW(cf as u32, buf.as_mut_ptr(), buf.len() as i32) };
        if n > 0 { String::from_utf16_lossy(&buf[..n as usize]) } else { format!("CF {cf}") }
    };
    let mut listed = Vec::new();
    // SAFETY: a live IDataObject; the enumerator is released by `Com`; each FORMATETC's `ptd` (a
    // target device, never asked for here) is freed with CoTaskMemFree.
    unsafe {
        let mut e: *mut c_void = std::ptr::null_mut();
        let hr = (data_vtbl(data).enum_format_etc)(data, 1, &mut e); // DATADIR_GET
        if hr < 0 || e.is_null() {
            listed.push(format!("EnumFormatEtc failed 0x{:08X}", hr as u32));
        } else {
            let en = Com(e);
            let next: unsafe extern "system" fn(*mut c_void, u32, *mut FORMATETC, *mut u32) -> HRESULT =
                std::mem::transmute((**(en.0 as *const *const [*const c_void; 4]))[3]);
            loop {
                let mut f: FORMATETC = std::mem::zeroed();
                let mut got = 0u32;
                if next(en.0, 1, &mut f, &mut got) != S_OK || got == 0 {
                    break;
                }
                if !f.ptd.is_null() {
                    windows_sys::Win32::System::Com::CoTaskMemFree(f.ptd as *const c_void);
                }
                listed.push(format!("{} tymed=0x{:x} lindex={} aspect={}", name(f.cfFormat), f.tymed, f.lindex, f.dwAspect));
            }
        }
    }
    let both = (TYMED_ISTREAM | TYMED_HGLOBAL) as u32;
    let asks = [
        (registered_format(CFSTR_FILEDESCRIPTORW), -1, TYMED_HGLOBAL as u32),
        (registered_format(CFSTR_FILEDESCRIPTORA), -1, TYMED_HGLOBAL as u32),
        (registered_format(CFSTR_FILECONTENTS), 0, both),
        (CF_HDROP, -1, TYMED_HGLOBAL as u32),
        (registered_format(CFSTR_SHELLIDLIST), -1, TYMED_HGLOBAL as u32),
    ];
    let queried: Vec<String> = asks
        .iter()
        .map(|&(cf, lindex, tymed)| {
            let f = FORMATETC { cfFormat: cf, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT, lindex, tymed };
            // SAFETY: a live IDataObject and a valid FORMATETC.
            let hr = unsafe { (data_vtbl(data).query_get_data)(data, &f) };
            format!("{}={:#010x}", name(cf), hr as u32)
        })
        .collect();
    format!("formats [{}]; QueryGetData {}", listed.join(", "), queried.join(" "))
}

unsafe extern "system" fn dt_drag_over(this: *mut c_void, keys: u32, pt: POINTL, effect: *mut u32) -> HRESULT {
    // SAFETY: as in `dt_drag_enter`.
    let t = unsafe { &*(this as *const DropTarget) };
    // Near a list's top or bottom edge: scroll it a row, at most once per SCROLL_EVERY (OLE calls
    // DragOver on a timer even while the mouse rests), then place the highlight on the scrolled
    // list. The scroll is a property set; no app-state borrow is held while it runs.
    if t.scrolled_at.get().is_none_or(|at| at.elapsed() >= SCROLL_EVERY) {
        let p = t.client(pt);
        if crate::drop_autoscroll(p.x, p.y) {
            t.scrolled_at.set(Some(std::time::Instant::now()));
        }
    }
    t.over(keys, pt, effect);
    target_helper_call(HelperCall::Over, pt, out_effect(effect));
    S_OK
}

unsafe extern "system" fn dt_drag_leave(this: *mut c_void) -> HRESULT {
    // SAFETY: as in `dt_drag_enter`.
    let t = unsafe { &*(this as *const DropTarget) };
    *t.offered.borrow_mut() = Offer::Nothing;
    *t.last.borrow_mut() = None;
    crate::drop_highlight(None);
    eprintln!("[dnd] DragLeave");
    t.unspring("DragLeave");
    t.end_drag();
    target_helper_call(HelperCall::Leave, POINTL { x: 0, y: 0 }, NONE);
    S_OK
}

/// Our temporary subclass id on the main window while the drop menu is up ("twdm").
const DROP_MENU_SUBCLASS: usize = 0x7477_646d;

/// While the drop menu is up: log, once, whether the app state is free when the menu's modal
/// loop first goes idle (`WM_ENTERIDLE` goes to the owner). Touches no app state itself.
unsafe extern "system" fn drop_menu_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, data: usize) -> LRESULT {
    if msg == WM_ENTERIDLE && data != 0 {
        // SAFETY: `data` is the Cell that `drop_menu` keeps alive until after the subclass is removed.
        let seen = unsafe { &*(data as *const Cell<bool>) };
        if !seen.replace(true) {
            eprintln!("[dnd] drop menu loop idle: app state free={}", crate::state_is_free());
        }
    }
    // SAFETY: everything goes on down the subclass chain.
    unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
}

/// The right-button drop menu at screen point `pt`, owned by `hwnd`: "Move here", "Copy here", a
/// separator and "Cancel", `default` in bold, an effect the source does not allow greyed out.
/// Returns the command id (0 when dismissed). A modal loop: **call with no borrow held**.
fn drop_menu(hwnd: isize, pt: POINTL, default: u32, allowed: u32) -> u32 {
    // SAFETY: a popup menu we create, show on our own window's thread and destroy; the subclass
    // and its refdata live until after RemoveWindowSubclass.
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return 0;
        }
        for (i, (label, effect)) in [(MENU_LABELS[0], MOVE), (MENU_LABELS[1], COPY)].into_iter().enumerate() {
            let w: Vec<u16> = label.encode_utf16().chain([0]).collect();
            let grey = if allowed & effect != 0 { 0 } else { MF_GRAYED };
            AppendMenuW(menu, MF_STRING | grey, i + 1, w.as_ptr());
        }
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        let w: Vec<u16> = MENU_LABELS[2].encode_utf16().chain([0]).collect();
        AppendMenuW(menu, MF_STRING, MENU_CANCEL as usize, w.as_ptr());
        SetMenuDefaultItem(menu, default, 0);
        let seen = Box::new(Cell::new(false));
        let hwnd = hwnd as HWND;
        SetWindowSubclass(hwnd, Some(drop_menu_proc), DROP_MENU_SUBCLASS, &*seen as *const Cell<bool> as usize);
        crate::shellmenu::set_current_menu(menu as isize);
        // A menu whose owner is not in the foreground does not close on a click elsewhere
        // (TrackPopupMenu's documented caveat); a drop from Explorer leaves Explorer in front.
        SetForegroundWindow(hwnd);
        eprintln!("[dnd] drop menu starts at ({}, {})", pt.x, pt.y);
        // MODAL LOOP. No RefCell borrow of app state is held (see dt_drop).
        let cmd = TrackPopupMenuEx(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_LEFTALIGN | TPM_TOPALIGN, pt.x, pt.y, hwnd, std::ptr::null()) as u32;
        crate::shellmenu::set_current_menu(0);
        RemoveWindowSubclass(hwnd, Some(drop_menu_proc), DROP_MENU_SUBCLASS);
        DestroyMenu(menu);
        cmd
    }
}

/// The effect a target method is about to return (NONE when OLE passed no pointer).
fn out_effect(effect: *mut u32) -> u32 {
    // SAFETY: COM contract: a valid in/out pointer (checked for null).
    if effect.is_null() { NONE } else { unsafe { *effect } }
}

unsafe extern "system" fn dt_drop(this: *mut c_void, data: *mut c_void, keys: u32, pt: POINTL, effect: *mut u32) -> HRESULT {
    // SAFETY: as in `dt_drag_enter`.
    let t = unsafe { &*(this as *const DropTarget) };
    // Read the files again from the data object OLE hands to Drop: the authoritative list.
    *t.offered.borrow_mut() = target_offer(data);
    // SAFETY: COM contract: a valid in/out pointer (checked for null).
    let allowed = if effect.is_null() { NONE } else { unsafe { *effect } };
    let (spot, mut e) = t.evaluate(keys, pt, allowed);
    let right = keys & MK_RBUTTON != 0 || t.right.replace(false);
    let fresh = std::mem::replace(&mut *t.offered.borrow_mut(), Offer::Nothing);
    // Virtual files are only ever copied: the menu greys "Move here".
    let menu_allowed = if matches!(fresh, Offer::Virtual(_) | Offer::Archive(_)) { allowed & COPY } else { allowed };
    *t.last.borrow_mut() = None;
    crate::drop_highlight(None);
    t.unspring("Drop");
    // The source's own description back (on the data object DragEnter kept, which is Drop's).
    t.end_drag();
    // The shell takes its drag image down (before the menu, as Explorer does).
    target_helper_call(HelperCall::Drop(data), pt, e);
    if right && e != NONE && spot.is_some() {
        // A right-button drop asks: Move here, Copy here or Cancel, the default in bold. A modal
        // loop inside Drop (itself inside the source's DoDragDrop): nothing is borrowed here,
        // `drop_spot`'s borrow ended inside `evaluate` and the target's own cells are free.
        let cmd = drop_menu(t.hwnd, pt, menu_default(e), menu_allowed);
        let chosen = menu_effect(cmd, menu_allowed);
        eprintln!(
            "[dnd] drop menu: default {}, chose {} -> effect {}",
            MENU_LABELS[menu_default(e) as usize - 1],
            if cmd == 0 { "nothing (dismissed)" } else { MENU_LABELS.get(cmd as usize - 1).copied().unwrap_or("?") },
            effect_name(chosen)
        );
        e = chosen;
    }
    let reported = reported_effect(e);
    eprintln!(
        "[dnd] Drop at {} keys=0x{keys:x} allowed=0x{allowed:x}: {}, effect {} (reported to the source: {})",
        describe(&spot),
        match &fresh {
            Offer::Files(f) => format!("{} file(s)", f.len()),
            Offer::Virtual(v) => format!("{} virtual item(s)", v.len()),
            Offer::Archive(p) => format!("{} in-archive item(s)", p.len()),
            Offer::Nothing => "0 file(s)".into(),
        },
        effect_name(e),
        effect_name(reported)
    );
    if let (Some(spot), true) = (spot, e != NONE) {
        match fresh {
            // The job (and every app-state change) runs from the event loop: after our own
            // DoDragDrop has returned, or at the next loop turn for another program's drag.
            Offer::Files(sources) => {
                let _ = slint::invoke_from_event_loop(move || crate::drop_files(spot.pane, sources, spot.dest, e));
            }
            // Written here (or, for an async source, on a worker once Drop has returned).
            Offer::Virtual(items) => virtual_drop(data, items, spot.pane, spot.dest),
            // Items inside an archive: the extraction job, queued as a files drop is.
            Offer::Archive(sources) => {
                let _ = slint::invoke_from_event_loop(move || crate::drop_files(spot.pane, sources, spot.dest, COPY));
            }
            Offer::Nothing => {}
        }
        set_dword(data, CFSTR_PERFORMEDDROPEFFECT, reported);
        set_dword(data, CFSTR_LOGICALPERFORMEDDROPEFFECT, e);
    }
    if !effect.is_null() {
        // SAFETY: as above.
        unsafe { *effect = reported };
    }
    S_OK
}

thread_local! {
    /// Our registered drop target (one reference held for the life of the thread), for the
    /// driver's `dropfiles`, which calls it directly. Null until `register`.
    static TARGET: Cell<*mut c_void> = const { Cell::new(std::ptr::null_mut()) };
    /// Whether this drag's first QueryContinueDrag was logged.
    static FIRST_QCD_SEEN: Cell<bool> = const { Cell::new(false) };

    /// Drags started (`drag` calls that reached DoDragDrop), for the driver.
    static DRAGS: Cell<u64> = const { Cell::new(0) };

    /// Our own drag is inside DoDragDrop: its window, the frame count when it started, the frame
    /// count when the highlight first changed inside it, and whether the paint was logged.
    static OWN_DRAG: Cell<Option<(isize, u64)>> = const { Cell::new(None) };
    static HIGHLIGHT_AT: Cell<Option<u64>> = const { Cell::new(None) };
    static PAINT_LOGGED: Cell<bool> = const { Cell::new(false) };
}

/// How many drags reached `DoDragDrop` so far.
pub fn drags_started() -> u64 {
    DRAGS.with(Cell::get)
}

/// Our own drag's loop is running.
pub fn own_drag_running() -> bool {
    OWN_DRAG.with(Cell::get).is_some()
}

/// The highlight or a list's scroll changed. Inside our own drag nothing asks Slint for a frame
/// (its redraw throttle is a Slint timer, and Slint timers do not run inside OLE's loop), so ask
/// Windows for a paint, as winit's `request_redraw` does; the drag runs outside winit's handler
/// (`crate::run_pending_drag`), so the WM_PAINT reaches Slint and renders. Outside our own drag,
/// nothing.
pub fn repaint_during_own_drag() {
    let Some((hwnd, _)) = OWN_DRAG.with(Cell::get) else { return };
    HIGHLIGHT_AT.with(|h| {
        if h.get().is_none() {
            h.set(Some(crate::perf::frames()));
        }
    });
    // SAFETY: our own window; RDW_INTERNALPAINT only queues a WM_PAINT.
    unsafe { RedrawWindow(hwnd as HWND, std::ptr::null(), std::ptr::null_mut(), RDW_INTERNALPAINT) };
}

/// From QueryContinueDrag (on every turn of the drag loop): once a frame was rendered after the
/// highlight changed, say so, once per drag.
fn log_paint_inside_drag() {
    let Some(at) = HIGHLIGHT_AT.with(Cell::get) else { return };
    if !PAINT_LOGGED.with(Cell::get) && crate::perf::frames() > at {
        PAINT_LOGGED.with(|p| p.set(true));
        eprintln!("[dnd] highlight painted inside our own drag loop (frame {} after the change at frame {at})", crate::perf::frames());
    }
}

/// The shell's generic data object (`SHCreateDataObject`) for `names` in `folder`: for items of a
/// zip folder it offers only a Shell IDList Array (the driver's `dropvirtual ... idlist`).
pub fn idlist_data(hwnd: isize, folder: &Path, names: &[String]) -> Result<Com, String> {
    shellmenu::ensure_com()?;
    let (folder_pidl, _folder, children) = shellmenu::parse_items(hwnd as HWND, folder, names)?;
    let apidl: Vec<*const _> = children.iter().map(|c| c.0 as *const _).collect();
    let mut p: *mut c_void = std::ptr::null_mut();
    // SAFETY: as in `drag`.
    let hr = unsafe { SHCreateDataObject(folder_pidl.0, apidl.len() as u32, apidl.as_ptr(), std::ptr::null_mut(), &IID_IDATAOBJECT, &mut p) };
    if hr < 0 || p.is_null() {
        return Err(format!("SHCreateDataObject failed: 0x{:08X}", hr as u32));
    }
    Ok(Com(p))
}

/// The driver's `dropvirtual ... ownzip`: `drop_data` with `paths` standing in for our own drag
/// out of an archive, as `drag` records them for its DoDragDrop.
pub fn drop_as_own_archive_drag(data: &Com, paths: Vec<PathBuf>, keys: u32, x: i32, y: i32) -> Option<(u32, u32)> {
    OWN_ARCHIVE_SOURCES.with(|s| *s.borrow_mut() = Some(paths));
    let r = drop_data(data, keys, x, y);
    OWN_ARCHIVE_SOURCES.with(|s| s.borrow_mut().take());
    r
}

/// The shell's own data object for `names` in `folder` (a folder inside a zip archive): what a
/// drag out of that folder carries.
pub fn archive_data(hwnd: isize, folder: &Path, names: &[String]) -> Result<Com, String> {
    shellmenu::ensure_com()?;
    let (_folder_pidl, folder, children) = shellmenu::parse_items(hwnd as HWND, folder, names)?;
    let d = shellmenu::ui_data_object(hwnd as HWND, &folder, &children)?;
    allow_async(d.0);
    Ok(d)
}

/// Replace winit's drop target with ours on `hwnd`. Returns (RevokeDragDrop, RegisterDragDrop)
/// results. Call once, on the UI thread, when the window exists.
pub fn register(hwnd: isize) -> (HRESULT, HRESULT) {
    let obj = Box::new(DropTarget {
        header: Header { vtbl: &DROP_TARGET_VTBL as *const DropTargetVtbl as *const c_void, refs: AtomicU32::new(1) },
        hwnd,
        offered: RefCell::new(Offer::Nothing),
        last: RefCell::new(None),
        right: Cell::new(false),
        scrolled_at: Cell::new(None),
        spring: RefCell::new(Spring::default()),
        data: Cell::new(std::ptr::null_mut()),
        described: RefCell::new(None),
        describe_failed: Cell::new(false),
        src_volume: RefCell::new(None),
        dest_volume: RefCell::new(VolumeCache::default()),
    });
    let ptr = Box::into_raw(obj) as *mut c_void;
    // SAFETY: plain OLE calls on our own window, on its thread. RegisterDragDrop takes its own
    // reference; ours stays in TARGET for the life of the thread.
    let (revoked, registered) = unsafe { (RevokeDragDrop(hwnd as HWND), RegisterDragDrop(hwnd as HWND, ptr)) };
    if registered == S_OK {
        TARGET.with(|t| t.set(ptr));
        // Both drag image helpers, made now on the UI thread and kept.
        source_helper();
        target_helper();
    } else {
        // SAFETY: drops our only reference; frees the object.
        unsafe { dt_release(ptr) };
    }
    (revoked, registered)
}

/// The driver's `dropfiles`: call our registered target's DragEnter then Drop with a data object
/// serving `paths`, at screen point (x, y), as OLE would. Returns (DragEnter effect, Drop
/// effect), or None when no target is registered.
pub fn drop_direct(paths: &[PathBuf], keys: u32, x: i32, y: i32) -> Option<(u32, u32)> {
    drop_data(&hdrop_data(paths), keys, x, y)
}

/// The driver's `dropvirtual`: as `drop_direct` with a data object of virtual files (ours from
/// `virtual_data`, or the shell's zip folder's from `archive_data`).
pub fn drop_data(data: &Com, keys: u32, x: i32, y: i32) -> Option<(u32, u32)> {
    let t = TARGET.with(Cell::get);
    if t.is_null() {
        return None;
    }
    let pt = POINTL { x, y };
    // SAFETY: `t` is our live DropTarget (TARGET holds a reference); `data` is a live
    // IDataObject for both calls.
    unsafe {
        let v = &*(*(t as *const *const DropTargetVtbl));
        let mut enter = COPY | MOVE;
        (v.drag_enter)(t, data.0, keys, pt, &mut enter);
        let mut dropped = COPY | MOVE;
        (v.drop)(t, data.0, keys, pt, &mut dropped);
        Some((enter, dropped))
    }
}

thread_local! {
    /// The data object of the driver's `dragover` hover, between `hover_begin` and `hover_end`.
    static HOVER: RefCell<Option<Com>> = const { RefCell::new(None) };
}

/// The driver's `dragover`: a drag of `paths` that enters our window at screen point (x, y) and
/// hovers there (`hover_over`, called from a Slint timer, as OLE calls DragOver on its own timer
/// for another program's drag) until `hover_end` leaves. It never drops. False when no target
/// is registered.
pub fn hover_begin(paths: &[PathBuf], x: i32, y: i32) -> bool {
    let t = TARGET.with(Cell::get);
    if t.is_null() {
        return false;
    }
    let data = hdrop_data(paths);
    // SAFETY: as in `drop_direct`.
    unsafe {
        let v = &*(*(t as *const *const DropTargetVtbl));
        let mut e = COPY | MOVE;
        (v.drag_enter)(t, data.0, 0, POINTL { x, y }, &mut e);
    }
    HOVER.with(|h| *h.borrow_mut() = Some(data));
    true
}

pub fn hover_over(x: i32, y: i32) {
    let t = TARGET.with(Cell::get);
    if t.is_null() || HOVER.with(|h| h.borrow().is_none()) {
        return;
    }
    // SAFETY: as in `drop_direct`; the hover's data object is alive in HOVER.
    unsafe {
        let v = &*(*(t as *const *const DropTargetVtbl));
        let mut e = COPY | MOVE;
        (v.drag_over)(t, 0, POINTL { x, y }, &mut e);
    }
}

pub fn hover_end() {
    let t = TARGET.with(Cell::get);
    let data = HOVER.with(|h| h.borrow_mut().take());
    if t.is_null() || data.is_none() {
        return;
    }
    // SAFETY: as in `drop_direct`.
    unsafe { ((*(*(t as *const *const DropTargetVtbl))).drag_leave)(t) };
    drop(data);
}

// --- Drag images: the shell's drag-drop helper --------------------------------------------------
//
// One `CLSID_DragDropHelper` object per interface, created on the UI thread the first time it is
// needed and kept for the life of the process (never released: it belongs to the UI thread's
// apartment, and a release from a thread-local destructor at exit could run after COM is gone).
// `IDragSourceHelper::InitializeFromBitmap` stores our drag image on the data object (`SetData`
// with the shell's private formats); `IDropTargetHelper` paints whatever image a source stored
// (Explorer's, or ours) over our window while a drag hovers it.

const CLSID_DRAG_DROP_HELPER: GUID = GUID::from_u128(0x4657278a_411b_11d2_839a_00c04fd918d0);
const IID_IDROPTARGETHELPER: GUID = GUID::from_u128(0x4657278b_411b_11d2_839a_00c04fd918d0);
const IID_IDRAGSOURCEHELPER: GUID = GUID::from_u128(0xde5bf786_477a_11d2_839d_00c04fd918d0);

/// IDragSourceHelper (shobjidl_core.h).
#[repr(C)]
struct DragSourceHelperVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    initialize_from_bitmap: unsafe extern "system" fn(*mut c_void, *const SHDRAGIMAGE, *mut c_void) -> HRESULT,
    initialize_from_window: unsafe extern "system" fn(*mut c_void, HWND, *const POINT, *mut c_void) -> HRESULT,
}

/// IDropTargetHelper (shobjidl_core.h). Points are screen coordinates, by pointer.
#[repr(C)]
struct DropTargetHelperVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    drag_enter: unsafe extern "system" fn(*mut c_void, HWND, *mut c_void, *const POINT, u32) -> HRESULT,
    drag_leave: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    drag_over: unsafe extern "system" fn(*mut c_void, *const POINT, u32) -> HRESULT,
    drop: unsafe extern "system" fn(*mut c_void, *mut c_void, *const POINT, u32) -> HRESULT,
    show: unsafe extern "system" fn(*mut c_void, BOOL) -> HRESULT,
}

/// A new drag-drop helper object, asked for `iid`, on the calling thread.
fn create_helper(iid: &GUID) -> Result<Com, HRESULT> {
    let mut p: *mut c_void = std::ptr::null_mut();
    // SAFETY: a valid CLSID/IID pair and out-pointer; the object is in-process.
    let hr = unsafe { CoCreateInstance(&CLSID_DRAG_DROP_HELPER, std::ptr::null_mut(), CLSCTX_INPROC_SERVER, iid, &mut p) };
    if hr < 0 || p.is_null() { Err(hr) } else { Ok(Com(p)) }
}

thread_local! {
    /// The UI thread's two helpers (null: not created, or creation failed), and whether each was
    /// tried, so a failure is logged once.
    static SOURCE_HELPER: Cell<(*mut c_void, bool)> = const { Cell::new((std::ptr::null_mut(), false)) };
    static TARGET_HELPER: Cell<(*mut c_void, bool)> = const { Cell::new((std::ptr::null_mut(), false)) };
}

/// The thread's helper for `iid` from `slot`, created on first use and kept (never released).
fn helper(slot: &'static std::thread::LocalKey<Cell<(*mut c_void, bool)>>, iid: &GUID, what: &str) -> *mut c_void {
    let (p, tried) = slot.with(Cell::get);
    if tried {
        return p;
    }
    let p = match create_helper(iid) {
        Ok(c) => {
            let raw = c.0;
            std::mem::forget(c);
            eprintln!("[dnd] {what} created");
            raw
        }
        Err(hr) => {
            eprintln!("[dnd] {what}: CoCreateInstance failed 0x{:08X}; no drag images", hr as u32);
            std::ptr::null_mut()
        }
    };
    slot.with(|c| c.set((p, true)));
    p
}

fn source_helper() -> *mut c_void {
    helper(&SOURCE_HELPER, &IID_IDRAGSOURCEHELPER, "IDragSourceHelper")
}

const IID_IDRAGSOURCEHELPER2: GUID = GUID::from_u128(0x83e07d0d_0c5f_4163_bf1a_60b274051e40);
/// `IDragSourceHelper2::SetFlags`: show the target's `DROPDESCRIPTION` text on our drag image.
const DSH_ALLOWDROPDESCRIPTIONTEXT: u32 = 1;

/// IDragSourceHelper2 (shobjidl.h): IDragSourceHelper's methods, then SetFlags.
#[repr(C)]
struct DragSourceHelper2Vtbl {
    base: DragSourceHelperVtbl,
    set_flags: unsafe extern "system" fn(*mut c_void, u32) -> HRESULT,
}

thread_local! {
    /// The source helper's IDragSourceHelper2 face (null: not asked yet, or not offered), kept
    /// like the helper itself, and whether it was asked for.
    static SOURCE_HELPER2: Cell<(*mut c_void, bool)> = const { Cell::new((std::ptr::null_mut(), false)) };
}

/// Let the drop target's description show on our drag image: `SetFlags(DSH_ALLOWDROPDESCRIPTIONTEXT)`
/// on the source helper, before every `InitializeFromBitmap` (the helper applies its flags when it
/// stores an image on a data object; the documentation states no order, and every sample sets the
/// flags first). Logged once per process; without IDragSourceHelper2 the image shows no text.
fn allow_description_text(helper: *mut c_void) {
    let (mut h2, asked) = SOURCE_HELPER2.with(Cell::get);
    if !asked {
        let mut p: *mut c_void = std::ptr::null_mut();
        // SAFETY: `helper` is the live IDragSourceHelper of this thread; the interface obtained is
        // kept for the life of the process, as the helper is.
        let hr = unsafe { ((**(helper as *const *const DragSourceHelperVtbl)).query_interface)(helper, &IID_IDRAGSOURCEHELPER2, &mut p) };
        h2 = if hr >= 0 { p } else { std::ptr::null_mut() };
        if !h2.is_null() {
            // SAFETY: as below.
            let hr = unsafe { ((**(h2 as *const *const DragSourceHelper2Vtbl)).set_flags)(h2, DSH_ALLOWDROPDESCRIPTIONTEXT) };
            eprintln!("[dnd] IDragSourceHelper2::SetFlags(DSH_ALLOWDROPDESCRIPTIONTEXT) = 0x{:08X}", hr as u32);
        }
        SOURCE_HELPER2.with(|c| c.set((h2, true)));
        return;
    }
    if !h2.is_null() {
        // SAFETY: the live IDragSourceHelper2 kept for this thread.
        unsafe { ((**(h2 as *const *const DragSourceHelper2Vtbl)).set_flags)(h2, DSH_ALLOWDROPDESCRIPTIONTEXT) };
    }
}

fn target_helper() -> *mut c_void {
    helper(&TARGET_HELPER, &IID_IDROPTARGETHELPER, "IDropTargetHelper")
}

/// The target helper's calls from our drop target, after our own work for the call is done (and
/// `drop_spot`'s borrow released). A no-op when the helper could not be created.
enum HelperCall {
    Enter(isize, *mut c_void),
    Over,
    Leave,
    Drop(*mut c_void),
}

fn target_helper_call(call: HelperCall, pt: POINTL, effect: u32) {
    let h = target_helper();
    if h.is_null() {
        return;
    }
    let p = POINT { x: pt.x, y: pt.y };
    // SAFETY: `h` is the live IDropTargetHelper kept for the thread; the data objects are the
    // live ones OLE handed to the target method that calls us.
    unsafe {
        let v = &**(h as *const *const DropTargetHelperVtbl);
        match call {
            HelperCall::Enter(hwnd, data) => (v.drag_enter)(h, hwnd as HWND, data, &p, effect),
            HelperCall::Over => (v.drag_over)(h, &p, effect),
            HelperCall::Leave => (v.drag_leave)(h),
            HelperCall::Drop(data) => (v.drop)(h, data, &p, effect),
        };
    }
}

/// The drag image's geometry for a window scale factor: the whole image (square), the icon in
/// it and its inset, the count badge's radius, and where the cursor sits (near the top-left).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageGeom {
    pub size: u32,
    pub icon: u32,
    pub inset: u32,
    pub badge_r: f32,
    pub cursor: (i32, i32),
}

impl ImageGeom {
    pub fn new(scale: f32) -> ImageGeom {
        Self::sized(scale, 96.0, 64.0, 8.0)
    }

    /// Out of the grid (P4-M3): the tile's picture at the tile's icon size (96 px at 100%, 48 px
    /// out of medium tiles, P6-M2), with the inset doubled so the badge still fits inside the
    /// square: 128 px, or 80 px.
    pub fn tile(scale: f32, tile: crate::icons::TileGeom) -> ImageGeom {
        Self::sized(scale, tile.icon_px + 32.0, tile.icon_px, 16.0)
    }

    fn sized(scale: f32, size: f32, icon: f32, inset: f32) -> ImageGeom {
        let s = |v: f32| (v * scale).round().max(1.0);
        ImageGeom { size: s(size) as u32, icon: s(icon) as u32, inset: s(inset) as u32, badge_r: s(14.0), cursor: (s(16.0) as i32, s(16.0) as i32) }
    }

    /// The badge's centre: over the icon's bottom-right corner.
    pub fn badge_centre(&self) -> (f32, f32) {
        let c = (self.inset + self.icon) as f32 - self.badge_r * 0.6;
        (c, c)
    }
}

/// The count shown on the badge.
pub fn badge_text(count: usize) -> String {
    if count > 99 { "99+".into() } else { count.to_string() }
}

/// The drag image as top-down BGRA, premultiplied (what a 32-bit `SHDRAGIMAGE` bitmap holds):
/// the icon (premultiplied RGBA, `g.icon` square) at the inset, and for several items a round
/// accent badge with the count over its bottom-right corner. `text` is the count's coverage
/// (0..255), `2 * badge_r` square, centred on the badge; None draws the badge without it.
pub fn compose_image(g: &ImageGeom, icon: Option<&[slint::Rgba8Pixel]>, count: usize, text: Option<(&[u8], u32)>) -> Vec<u8> {
    let n = g.size as usize;
    let mut out = vec![0u8; n * n * 4];
    if let Some(px) = icon.filter(|p| p.len() == (g.icon * g.icon) as usize) {
        for y in 0..g.icon as usize {
            for x in 0..g.icon as usize {
                let (dx, dy) = (x + g.inset as usize, y + g.inset as usize);
                if dx >= n || dy >= n {
                    continue;
                }
                let p = px[y * g.icon as usize + x];
                let o = (dy * n + dx) * 4;
                out[o..o + 4].copy_from_slice(&[p.b, p.g, p.r, p.a]);
            }
        }
    }
    if count > 1 {
        let (cx, cy) = g.badge_centre();
        let r = g.badge_r;
        // Accent blue (Windows' default accent, 0 120 215), premultiplied BGRA at full alpha.
        let badge = [215.0f32, 120.0, 0.0, 255.0];
        let x0 = (cx - r - 1.0).floor().max(0.0) as usize;
        let y0 = (cy - r - 1.0).floor().max(0.0) as usize;
        let x1 = ((cx + r + 1.0).ceil() as usize).min(n);
        let y1 = ((cy + r + 1.0).ceil() as usize).min(n);
        let tw = text.map_or(0, |(_, w)| w as usize);
        let (tx0, ty0) = ((cx - tw as f32 / 2.0).round() as i64, (cy - tw as f32 / 2.0).round() as i64);
        for y in y0..y1 {
            for x in x0..x1 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let a = (r + 0.5 - d).clamp(0.0, 1.0);
                if a <= 0.0 {
                    continue;
                }
                let o = (y * n + x) * 4;
                for k in 0..4 {
                    out[o + k] = (badge[k] * a + out[o + k] as f32 * (1.0 - a)).round() as u8;
                }
                if let Some((cov, w)) = text {
                    let (tx, ty) = (x as i64 - tx0, y as i64 - ty0);
                    if tx >= 0 && ty >= 0 && (tx as u32) < w && (ty as u32) < w {
                        let c = cov[ty as usize * w as usize + tx as usize] as f32 / 255.0 * a;
                        for k in 0..4 {
                            out[o + k] = (255.0 * c + out[o + k] as f32 * (1.0 - c)).round() as u8;
                        }
                    }
                }
            }
        }
    }
    out
}

/// The count's text as coverage, white on black drawn by GDI into a `w` square DIB (GDI text
/// leaves alpha alone, so the green channel is the coverage). None if any GDI call fails.
fn text_coverage(text: &str, w: u32, px: i32) -> Option<Vec<u8>> {
    let wide: Vec<u16> = text.encode_utf16().collect();
    let face: Vec<u16> = "Segoe UI".encode_utf16().chain([0]).collect();
    // SAFETY: a memory DC and a DIB section we create, select, read and free here; the bits
    // pointer is valid while the bitmap lives.
    unsafe {
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        if dc.is_null() {
            return None;
        }
        let mut bits: *mut c_void = std::ptr::null_mut();
        let bmi = dib_info(w, w);
        let bmp = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        let font = CreateFontW(-px, 0, 0, 0, 700, 0, 0, 0, 0, 0, 0, 5 /* CLEARTYPE_QUALITY */, 0, face.as_ptr());
        let mut out = None;
        if !bmp.is_null() && !bits.is_null() {
            let old_bmp = SelectObject(dc, bmp);
            let old_font = if font.is_null() { std::ptr::null_mut() } else { SelectObject(dc, font) };
            SetTextColor(dc, 0x00FF_FFFF);
            SetBkMode(dc, TRANSPARENT as i32);
            let mut rc = RECT { left: 0, top: 0, right: w as i32, bottom: w as i32 };
            DrawTextW(dc, wide.as_ptr(), wide.len() as i32, &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
            GdiFlush();
            let px = std::slice::from_raw_parts(bits as *const u8, (w * w * 4) as usize);
            out = Some(px.chunks_exact(4).map(|p| p[1]).collect());
            if !old_font.is_null() {
                SelectObject(dc, old_font);
            }
            SelectObject(dc, old_bmp);
        }
        if !font.is_null() {
            DeleteObject(font);
        }
        if !bmp.is_null() {
            DeleteObject(bmp);
        }
        DeleteDC(dc);
        out
    }
}

/// A top-down 32-bit BITMAPINFO.
fn dib_info(w: u32, h: u32) -> BITMAPINFO {
    // SAFETY: an all-zero BITMAPINFO is valid; the header is filled in below.
    let mut bmi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w as i32,
        biHeight: -(h as i32),
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..bmi.bmiHeader
    };
    bmi
}

/// A 32-bit top-down DIB section holding `bgra` (`w` x `h`), or None.
fn dib_from(bgra: &[u8], w: u32, h: u32) -> Option<HBITMAP> {
    if bgra.len() != (w * h * 4) as usize {
        return None;
    }
    let bmi = dib_info(w, h);
    let mut bits: *mut c_void = std::ptr::null_mut();
    // SAFETY: CreateDIBSection with no DC for DIB_RGB_COLORS; the bits pointer covers w*h*4 bytes.
    unsafe {
        let bmp = CreateDIBSection(std::ptr::null_mut(), &bmi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        if bmp.is_null() || bits.is_null() {
            return None;
        }
        std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits as *mut u8, bgra.len());
        Some(bmp)
    }
}

/// Hand `bgra` to the source helper as the drag image of `data`. The helper owns the bitmap on
/// success; on failure it is deleted here. Returns InitializeFromBitmap's HRESULT (or the
/// failure before it).
fn init_drag_image(helper: *mut c_void, data: *mut c_void, bgra: &[u8], size: u32, cursor: (i32, i32)) -> HRESULT {
    if helper.is_null() {
        return E_NOINTERFACE;
    }
    let Some(bmp) = dib_from(bgra, size, size) else { return windows_sys::Win32::Foundation::E_OUTOFMEMORY };
    allow_description_text(helper);
    let img = SHDRAGIMAGE {
        sizeDragImage: SIZE { cx: size as i32, cy: size as i32 },
        ptOffset: POINT { x: cursor.0, y: cursor.1 },
        hbmpDragImage: bmp,
        crColorKey: 0xFFFF_FFFF, // CLR_NONE: the bitmap's alpha decides
    };
    // SAFETY: `helper` is a live IDragSourceHelper of this thread, `data` a live IDataObject.
    let hr = unsafe { ((**(helper as *const *const DragSourceHelperVtbl)).initialize_from_bitmap)(helper, &img, data) };
    if hr < 0 {
        // SAFETY: the helper did not take the bitmap.
        unsafe { DeleteObject(bmp) };
    }
    hr
}

/// What a drag shows under the cursor: the first item's icon, how many items, the scale.
pub struct DragImage {
    /// The item whose icon is drawn (asked of the shell at drag start, one call per drag).
    pub path: PathBuf,
    /// The pane's cached picture for it (the pane's icon size), used when the shell call fails;
    /// out of the grid (`tile`), what the tile shows when it is a real picture (`tile_picture`:
    /// the thumbnail or the type icon, not a placeholder), drawn in place of the shell's icon.
    pub cached: Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>>,
    pub tile: Option<crate::icons::TileGeom>,
    pub tile_picture: bool,
    /// The shell's icon for `path` at the image's icon size, rendered by the icon worker when the
    /// row was pressed (P5-M3), so the drag start need not wait for the shell; None when the reply
    /// had not landed yet (the shell is then asked here, as before).
    pub prefetched: Option<slint::SharedPixelBuffer<slint::Rgba8Pixel>>,
    /// The prefetched picture is the file's thumbnail, asked for a row that shows one (P9-M4).
    pub prefetched_thumbnail: bool,
    pub count: usize,
    pub scale: f32,
}

/// Where a drag image's icon is taken from, first that applies: out of the grid the tile's own
/// picture (`tile_picture`), else what the icon worker answered at the press (`prefetched`: Some, a
/// thumbnail when true, P9-M4), else the shell, asked at the drag's start (which falls back to the
/// pane's cache, then nothing).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconFrom {
    Tile,
    Thumbnail,
    Worker,
    Shell,
}

impl IconFrom {
    pub fn pick(tile_picture: bool, prefetched: Option<bool>) -> IconFrom {
        match (tile_picture, prefetched) {
            (true, _) => IconFrom::Tile,
            (false, Some(true)) => IconFrom::Thumbnail,
            (false, Some(false)) => IconFrom::Worker,
            (false, None) => IconFrom::Shell,
        }
    }

    /// The `[dnd] drag image ... icon from <label>` wording.
    pub fn label(self) -> &'static str {
        match self {
            IconFrom::Tile => "the tile",
            IconFrom::Thumbnail => "the thumbnail",
            IconFrom::Worker => "the worker",
            IconFrom::Shell => "the shell",
        }
    }
}

/// Compose the image and store it on `data` through the source helper; logs the result.
fn attach_drag_image(data: *mut c_void, img: &DragImage) {
    let g = match img.tile {
        Some(tile) => ImageGeom::tile(img.scale, tile),
        None => ImageGeom::new(img.scale),
    };
    let t0 = std::time::Instant::now();
    let tile = img.tile.is_some() && img.tile_picture && img.cached.is_some();
    let from = IconFrom::pick(tile, img.prefetched.as_ref().map(|_| img.prefetched_thumbnail));
    let (icon, from) = match (from, &img.cached, &img.prefetched) {
        (IconFrom::Tile, Some(c), _) => (Some(crate::icons::fit(c, g.icon)), from.label()),
        (IconFrom::Thumbnail | IconFrom::Worker, _, Some(p)) => (Some(crate::icons::fit(p, g.icon)), from.label()),
        _ => match crate::icons::render(&img.path, g.icon) {
            Some(p) => (Some(crate::icons::fit(&p, g.icon)), "the shell"),
            None => match &img.cached {
                Some(c) => (Some(crate::icons::fit(c, g.icon)), "the pane's cache"),
                None => (None, "nowhere"),
            },
        },
    };
    let tw = (g.badge_r * 2.0) as u32;
    let cov = (img.count > 1).then(|| text_coverage(&badge_text(img.count), tw, (g.badge_r * 1.1) as i32)).flatten();
    let bgra = compose_image(&g, icon.as_ref().map(|p| p.as_slice()), img.count, cov.as_deref().map(|c| (c, tw)));
    let hr = init_drag_image(source_helper(), data, &bgra, g.size, g.cursor);
    eprintln!(
        "[dnd] drag image {0}x{0} px, {4} px icon, {1} item(s), icon from {from}, composed in {2:.1} ms: InitializeFromBitmap = 0x{3:08X}",
        g.size,
        img.count,
        crate::perf::elapsed_ms(t0),
        hr as u32,
        g.icon
    );
}

// --- The drag source ----------------------------------------------------------------------------

#[repr(C)]
struct DropSource {
    header: Header,
    /// A right-button drag (`continue_drag`).
    right: bool,
    /// A scripted drag, fixed when the drag was asked for (`DragRequest::scripted`).
    scripted: bool,
    /// The drag's data object, not owned: `drag` holds it for the whole of `DoDragDrop`, the only
    /// time `GiveFeedback` is called.
    data: *mut c_void,
}

static DROP_SOURCE_VTBL: DropSourceVtbl = DropSourceVtbl {
    query_interface: ds_query,
    add_ref: ds_add_ref,
    release: ds_release,
    query_continue_drag: ds_query_continue_drag,
    give_feedback: ds_give_feedback,
};

unsafe fn ds_free(this: *mut c_void) {
    // SAFETY: created by `drag` with Box::into_raw.
    drop(unsafe { Box::from_raw(this as *mut DropSource) });
}

unsafe extern "system" fn ds_query(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    unsafe { query_impl(this, iid, &IID_IDROPSOURCE, out) }
}
unsafe extern "system" fn ds_add_ref(this: *mut c_void) -> u32 {
    unsafe { add_ref_impl(this) }
}
unsafe extern "system" fn ds_release(this: *mut c_void) -> u32 {
    unsafe { release_impl(this, ds_free) }
}

unsafe extern "system" fn ds_query_continue_drag(this: *mut c_void, escape: BOOL, keys: u32) -> HRESULT {
    if !FIRST_QCD_SEEN.with(|c| c.replace(true)) {
        // The re-entrancy check: inside DoDragDrop's loop no borrow of the app state is held.
        eprintln!("[dnd] first QueryContinueDrag inside the drag loop: app state free={}", crate::state_is_free());
    }
    // SAFETY: `this` is our DropSource.
    let right = unsafe { (*(this as *const DropSource)).right };
    // A scripted drag holds its button (the left, or the right for a right-button drag) until
    // Escape cancels it: it never drops (see `script_begin`).
    // SAFETY: as above.
    let scripted = unsafe { (*(this as *const DropSource)).scripted };
    log_paint_inside_drag();
    let lbutton = (scripted && !right) || keys & MK_LBUTTON != 0;
    let rbutton = (scripted && right) || keys & MK_RBUTTON != 0;
    let hr = continue_drag(escape != 0, lbutton, rbutton, right);
    if hr != S_OK {
        eprintln!(
            "[dnd] QueryContinueDrag escape={} keys=0x{keys:x}{}: {}",
            escape != 0,
            if scripted { " (scripted)" } else { "" },
            if hr == DRAGDROP_S_DROP { "drop" } else { "cancel" }
        );
    }
    hr
}

/// While the drag image shows a target's drop description, the source sets the cursor itself to
/// the plain arrow, as Explorer's sources do (P7-M4): OLE's default cursors would add their own
/// arrow-and-box glyph beside the description's icon (Jeremy's console check, 2026-10-19). A
/// description shows when the drag-drop helper draws a layered image (`IsShowingLayered`) and the
/// target has set a `DropDescription` other than `DROPIMAGE_INVALID`.
pub fn own_cursor(description: Option<i32>, layered: bool) -> bool {
    layered && description.is_some_and(|k| k != DROPIMAGE_INVALID)
}

/// The `DropDescription` type on a data object (its first four bytes), if it carries one.
fn description_kind(data: *mut c_void) -> Option<i32> {
    let b = read_hglobal(data, registered_format(CFSTR_DROPDESCRIPTION))?;
    Some(i32::from_le_bytes(b.get(..4)?.try_into().ok()?))
}

/// The cursor `GiveFeedback` chose last (0 none yet, 1 the default cursors, 2 our arrow), so the
/// `[dnd] cursor` line is logged on a change only.
static CURSOR_MODE: AtomicU32 = AtomicU32::new(0);

unsafe extern "system" fn ds_give_feedback(this: *mut c_void, _: u32) -> HRESULT {
    // SAFETY: `this` is our DropSource, alive for the drag.
    let data = unsafe { (*(this as *const DropSource)).data };
    let kind = description_kind(data);
    let own = own_cursor(kind, read_dword(data, windows_sys::core::w!("IsShowingLayered")).is_some_and(|v| v != 0));
    let mode = if own { 2 } else { 1 };
    if CURSOR_MODE.swap(mode, Ordering::Relaxed) != mode {
        if own {
            eprintln!("[dnd] cursor: the arrow (drop description type {})", kind.unwrap_or(DROPIMAGE_INVALID));
        } else {
            eprintln!("[dnd] cursor: OLE's default cursors");
        }
    }
    if !own {
        return DRAGDROP_S_USEDEFAULTCURSORS;
    }
    use windows_sys::Win32::UI::WindowsAndMessaging::{IDC_ARROW, LoadCursorW, PostMessageW, SetCursor, WM_USER};
    // SAFETY: a shared system cursor; SetCursor takes any cursor handle.
    unsafe { SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_ARROW)) };
    // With the cursors ours, OLE no longer tells the drag image's window to repaint, so the
    // description never shows (Jeremy's console check, 2026-10-19): ask it, as a source that sets
    // its own cursor must (`DDWM_UPDATEWINDOW`, WM_USER + 3, to the helper's "DragWindow").
    if let Some(w) = read_dword(data, windows_sys::core::w!("DragWindow")).filter(|&w| w != 0) {
        // SAFETY: a posted message with no pointers; a stale handle only fails the post.
        unsafe { PostMessageW(w as usize as HWND, WM_USER + 3, 0, 0) };
    }
    S_OK
}

/// The driver's scripted drag is running (the real left button is up).
static SCRIPTED: AtomicBool = AtomicBool::new(false);

/// From `script_begin` until `script_end` the left button counts as held whatever the mouse
/// says, so a scripted drag ends only by Escape (or the right button): a cancel. A scripted
/// release was tried and removed: OLE drops on whatever window it last hit-tested under the real
/// cursor, and in a remote session that cursor moves under the script, so one test drop landed on
/// another program's window (M5 record in docs/phase-1-plan.md).
pub fn script_begin() {
    SCRIPTED.store(true, Ordering::Release);
}
pub fn script_end() {
    SCRIPTED.store(false, Ordering::Release);
}
pub fn scripted() -> bool {
    SCRIPTED.load(Ordering::Acquire)
}

pub struct DragRequest {
    pub hwnd: isize,
    /// The pane's folder.
    pub folder: PathBuf,
    /// The dragged names in it.
    pub names: Vec<String>,
    /// The drag image to show (None: the cursor alone).
    pub image: Option<DragImage>,
    /// A right-button drag: the right button held, its release drops, the target asks.
    pub right: bool,
    /// A scripted drag (`script_begin`): its button counts as held until Escape, whatever the
    /// driver does meanwhile, so it can never drop.
    pub scripted: bool,
}

/// Drag the named items: a shell data object for them, our drop source, and `DoDragDrop`, which
/// returns when the drop or cancel is done. True when something was dropped (whatever the
/// effect: an optimized move reports none). The effect, and the "Logical Performed DropEffect"
/// the target may have set, go to the log. **Call with no app-state borrow held** (module docs).
pub fn drag(req: &DragRequest) -> Result<bool, String> {
    shellmenu::ensure_com()?;
    let hwnd = req.hwnd as HWND;
    let (folder_pidl, folder, children) = shellmenu::parse_items(hwnd, &req.folder, &req.names)?;
    let archive = crate::fs::archive::inside(&req.folder);
    let data = if archive {
        // The zip folder's own data object: virtual files (FileGroupDescriptorW + FileContents),
        // which Explorer and our own drop target extract, asynchronously where they can.
        let d = shellmenu::ui_data_object(hwnd, &folder, &children)?;
        allow_async(d.0);
        d
    } else {
        let apidl: Vec<*const _> = children.iter().map(|c| c.0 as *const _).collect();
        let mut p: *mut c_void = std::ptr::null_mut();
        // SAFETY: an absolute folder PIDL and child PIDLs relative to it, alive for the call.
        let hr = unsafe { SHCreateDataObject(folder_pidl.0, apidl.len() as u32, apidl.as_ptr(), std::ptr::null_mut(), &IID_IDATAOBJECT, &mut p) };
        if hr < 0 || p.is_null() {
            return Err(format!("SHCreateDataObject failed: 0x{:08X}", hr as u32));
        }
        Com(p)
    };
    if let Some(img) = &req.image {
        attach_drag_image(data.0, img);
    }
    let source = Com(Box::into_raw(Box::new(DropSource {
        header: Header { vtbl: &DROP_SOURCE_VTBL as *const DropSourceVtbl as *const c_void, refs: AtomicU32::new(1) },
        right: req.right,
        scripted: req.scripted,
        data: data.0,
    })) as *mut c_void);
    CURSOR_MODE.store(0, Ordering::Relaxed);
    let mut effect = NONE;
    DRAGS.with(|d| d.set(d.get() + 1));
    FIRST_QCD_SEEN.with(|c| c.set(false));
    eprintln!(
        "[dnd] DoDragDrop starts ({} button): {} item(s) from {}",
        if req.right { "right" } else { "left" },
        req.names.len(),
        req.folder.display()
    );
    let t0 = std::time::Instant::now();
    if archive {
        OWN_ARCHIVE_SOURCES.with(|s| *s.borrow_mut() = Some(req.names.iter().map(|n| req.folder.join(n)).collect()));
    }
    OWN_DRAG.with(|d| d.set(Some((req.hwnd, crate::perf::frames()))));
    HIGHLIGHT_AT.with(|h| h.set(None));
    PAINT_LOGGED.with(|p| p.set(false));
    // MODAL LOOP. No RefCell borrow of app state is held here (the caller dropped its borrow).
    // OLE runs the drag until the button is released or Escape is pressed; our own drop target
    // is called from inside this loop when the cursor is over our window.
    // SAFETY: live IDataObject and IDropSource; `effect` is a valid out-pointer.
    let hr = unsafe { DoDragDrop(data.0, source.0, COPY | MOVE, &mut effect) };
    OWN_ARCHIVE_SOURCES.with(|s| s.borrow_mut().take());
    let logical = read_dword(data.0, CFSTR_LOGICALPERFORMEDDROPEFFECT);
    eprintln!(
        "[dnd] DoDragDrop returned 0x{:08X} ({}) after {:.0} ms: effect {}, logical performed effect {}",
        hr as u32,
        match hr {
            DRAGDROP_S_DROP => "dropped",
            DRAGDROP_S_CANCEL => "cancelled",
            _ => "failed",
        },
        crate::perf::elapsed_ms(t0),
        effect_name(effect),
        logical.map_or("not set", effect_name)
    );
    let frames = crate::perf::frames() - OWN_DRAG.with(|d| d.take()).map_or(0, |(_, f)| f);
    eprintln!("[dnd] {frames} frame(s) rendered inside the drag loop");
    Ok(hr == DRAGDROP_S_DROP)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    /// `effect_for` with the string rule's volumes (`path_volume_key`), no file-system call.
    fn eff(sources: &[PathBuf], dest: &Path, keys: u32, allowed: u32) -> u32 {
        let same = sources.first().is_some_and(|f| path_volume_key(f) == path_volume_key(dest));
        effect_for(sources, dest, keys, allowed, same)
    }

    #[test]
    fn query_continue_drag_states() {
        // (escape, lbutton, rbutton, right-button drag)
        assert_eq!(continue_drag(false, true, false, false), S_OK, "button held: keep dragging");
        assert_eq!(continue_drag(false, false, false, false), DRAGDROP_S_DROP, "left released: drop");
        assert_eq!(continue_drag(true, true, false, false), DRAGDROP_S_CANCEL, "Escape cancels");
        assert_eq!(continue_drag(true, false, false, false), DRAGDROP_S_CANCEL, "Escape wins over a release");
        assert_eq!(continue_drag(false, true, true, false), DRAGDROP_S_CANCEL, "right button cancels");
        assert_eq!(continue_drag(false, false, true, false), DRAGDROP_S_CANCEL);
        assert_eq!(continue_drag(true, false, true, false), DRAGDROP_S_CANCEL);
    }

    #[test]
    fn right_button_drag() {
        // A right-button drag continues while the right button is held, drops on its release,
        // and is cancelled by Escape or the left button.
        assert_eq!(continue_drag(false, false, true, true), S_OK, "right held: keep dragging");
        assert_eq!(continue_drag(false, false, false, true), DRAGDROP_S_DROP, "right released: drop");
        assert_eq!(continue_drag(true, false, true, true), DRAGDROP_S_CANCEL, "Escape cancels");
        assert_eq!(continue_drag(true, false, false, true), DRAGDROP_S_CANCEL, "Escape wins over the release");
        assert_eq!(continue_drag(false, true, true, true), DRAGDROP_S_CANCEL, "the left button cancels");

        // The gesture: a right-button drag from any state but a running drag; no release follows
        // it (the app ends Slint's grab without one), so it is over when DoDragDrop returns.
        let mut g = Gesture::default();
        assert!(g.drag_right(4));
        assert_eq!(g, Gesture::Dragging { row: 4, right: true });
        assert!(!g.drag_right(4), "one drag at a time");
        assert!(!g.drag(4), "no left drag inside it");
        assert!(!g.double_click(), "nothing opens while it runs");
        g.drag_ended();
        assert_eq!(g, Gesture::Idle, "over: the next press starts afresh");
        // A right-button drag after a left press on another row (the press is forgotten).
        g.press(1, false, false, false);
        assert!(g.drag_right(2));
        assert_eq!(g.release(), None, "a stray left release selects nothing");
        // The left gesture is unchanged: a left drag ends as Dragged, waiting for its release.
        g.press(3, false, false, false);
        assert!(g.drag(3));
        assert_eq!(g, Gesture::Dragging { row: 3, right: false });
        g.drag_ended();
        assert_eq!(g, Gesture::Dragged);

        // The drop menu's choice.
        let both = COPY | MOVE;
        assert_eq!(menu_effect(MENU_MOVE, both), MOVE);
        assert_eq!(menu_effect(MENU_COPY, both), COPY);
        assert_eq!(menu_effect(MENU_CANCEL, both), NONE, "Cancel");
        assert_eq!(menu_effect(0, both), NONE, "dismissed (Escape, a click elsewhere)");
        assert_eq!(menu_effect(MENU_MOVE, COPY), NONE, "a greyed-out item cannot be chosen");
        assert_eq!(menu_effect(MENU_COPY, MOVE), NONE);
        assert_eq!(menu_effect(99, both), NONE);
        assert_eq!(menu_default(MOVE), MENU_MOVE, "bold: what a left drop would do");
        assert_eq!(menu_default(COPY), MENU_COPY);
        assert_eq!(MENU_LABELS[MENU_MOVE as usize - 1], "Move here");
        assert_eq!(MENU_LABELS[MENU_COPY as usize - 1], "Copy here");
        assert_eq!(MENU_LABELS[MENU_CANCEL as usize - 1], "Cancel");
        // The effect the menu starts from is decide_effect's, unchanged by the right button.
        assert_eq!(eff(&[p(r"C:\src\a")], &p(r"C:\dst"), MK_RBUTTON, both), MOVE);
        assert_eq!(eff(&[p(r"C:\src\a")], &p(r"D:\dst"), MK_RBUTTON, both), COPY);
    }

    #[test]
    fn volumes() {
        // The string rule (UNC paths, and the fallback).
        assert_eq!(path_volume_key(&p(r"C:\a\b.txt")), "c:");
        assert_eq!(path_volume_key(&p(r"c:\")), "c:");
        assert_eq!(path_volume_key(&p(r"D:\x")), "d:");
        assert_eq!(path_volume_key(&p(r"\\?\C:\long\path")), "c:");
        assert_eq!(path_volume_key(&p(r"\\Server\Share\dir\f")), r"\\server\share");
        assert_eq!(path_volume_key(&p(r"\\?\UNC\server\share\f")), r"\\server\share");
        assert_ne!(path_volume_key(&p(r"\\server\a\f")), path_volume_key(&p(r"\\server\b\f")));
        // GetVolumePathNameW on the system drive: a folder, its root, a path that does not exist
        // and a verbatim one all lie on the root's volume.
        let windows = std::env::var("SystemRoot").map(PathBuf::from).unwrap_or_else(|_| p(r"C:\Windows"));
        let drive = path_volume_key(&windows);
        let key = volume_key(&PathBuf::from(format!(r"{}\", drive.to_uppercase())));
        assert_eq!(key, drive, "a drive root's key is the drive");
        assert_eq!(volume_key(&windows), key);
        assert_eq!(volume_key(&windows.join("no-such-folder-p5m3").join("x.txt")), key, "the path need not exist");
        assert_eq!(volume_key(&PathBuf::from(format!(r"\\?\{}", windows.display()))), key);
        // UNC paths keep the string rule (no call that could wait on a server).
        assert_eq!(volume_key(&p(r"\\no-such-server-p5m3\Share\dir")), r"\\no-such-server-p5m3\share");
    }

    #[test]
    fn volume_cache_looks_each_destination_up_once() {
        let calls = Cell::new(0);
        let fake = |d: &Path| {
            calls.set(calls.get() + 1);
            if d.starts_with(r"C:\mnt\data") { r"c:\mnt\data".to_string() } else { "c:".to_string() }
        };
        let mut c = VolumeCache::default();
        assert_eq!(c.key(&p(r"C:\dst"), fake), ("c:".to_string(), true), "first: looked up");
        assert_eq!(c.key(&p(r"C:\dst"), fake), ("c:".to_string(), false), "the same folder: cached");
        assert_eq!(c.key(&p(r"C:\dst"), fake), ("c:".to_string(), false));
        assert_eq!(calls.get(), 1, "DragOver after DragOver over one folder: one lookup");
        assert_eq!(c.key(&p(r"C:\mnt\data\sub"), fake), (r"c:\mnt\data".to_string(), true), "another folder");
        assert_eq!(c.key(&p(r"C:\dst"), fake), ("c:".to_string(), true), "one entry: back again is a lookup");
        assert_eq!(calls.get(), 3);
        // A mount point decides the effect: the same drive letter, another volume, copies.
        let both = COPY | MOVE;
        let src = fake(&p(r"C:\src\a.txt"));
        let (dest, _) = c.key(&p(r"C:\mnt\data\sub"), fake);
        assert_eq!(effect_for(&[p(r"C:\src\a.txt")], &p(r"C:\mnt\data\sub"), 0, both, src == dest), COPY);
        let (dest, _) = c.key(&p(r"C:\dst"), fake);
        assert_eq!(effect_for(&[p(r"C:\src\a.txt")], &p(r"C:\dst"), 0, both, src == dest), MOVE);
    }

    #[test]
    fn our_arrow_only_while_a_description_shows() {
        assert!(own_cursor(Some(DROPIMAGE_MOVE), true));
        assert!(own_cursor(Some(DROPIMAGE_COPY), true));
        assert!(own_cursor(Some(DROPIMAGE_NONE), true), "the description's own no-drop image");
        assert!(!own_cursor(Some(DROPIMAGE_INVALID), true), "cleared: OLE's cursors again");
        assert!(!own_cursor(None, true), "no target has set one");
        assert!(!own_cursor(Some(DROPIMAGE_MOVE), false), "no layered drag image: nothing shows it");
    }

    #[test]
    fn drop_descriptions() {
        assert_eq!(
            description(MOVE, Some(&p(r"C:\Users\me\Documents"))),
            Description { kind: DROPIMAGE_MOVE, message: "Move to %1", insert: "Documents".into() }
        );
        assert_eq!(description(COPY, Some(&p(r"D:\"))), Description { kind: DROPIMAGE_COPY, message: "Copy to %1", insert: "D:".into() });
        assert_eq!(description(COPY, Some(&p(r"C:\dir\"))).insert, "dir", "a trailing separator");
        assert_eq!(description(MOVE, Some(&p(r"\\server\share"))).insert, "share", "a UNC share root");
        assert_eq!(description(MOVE, Some(&p(r"\\server\share\"))).insert, "share");
        assert_eq!(description(MOVE, Some(&p(r"\\server\share\dir"))).insert, "dir");
        assert_eq!(description(COPY, Some(&p(r"\\?\C:\"))).insert, "C:");
        let none = Description { kind: DROPIMAGE_NONE, message: "", insert: String::new() };
        assert_eq!(description(NONE, Some(&p(r"C:\dst"))), none, "refused: the shell's own image, no text");
        assert_eq!(description(MOVE, None), none, "nowhere to drop");
        assert_eq!(Description::cleared().kind, DROPIMAGE_INVALID);

        // The DROPDESCRIPTION block: the type, then 260 wide characters each for message and insert.
        let b = description_block(&description(COPY, Some(&p(r"C:\dst"))));
        assert_eq!(b.len(), 1044);
        assert_eq!(b.len(), std::mem::size_of::<windows_sys::Win32::UI::Shell::DROPDESCRIPTION>());
        assert_eq!(i32::from_le_bytes(b[0..4].try_into().unwrap()), DROPIMAGE_COPY);
        let wide = |o: usize| -> String {
            let w: Vec<u16> = (0..260).map(|k| u16::from_le_bytes([b[o + 2 * k], b[o + 2 * k + 1]])).take_while(|&u| u != 0).collect();
            String::from_utf16_lossy(&w)
        };
        assert_eq!(wide(4), "Copy to %1");
        assert_eq!(wide(4 + 520), "dst");
        assert_eq!(&b[4 + 2 * 10..4 + 2 * 11], &[0, 0], "the message's NUL");
        assert_eq!(&b[524 + 2 * 3..524 + 2 * 4], &[0, 0], "the insert's NUL");
        // A name past 259 characters is cut so the NUL still fits.
        let long = description_block(&Description { kind: DROPIMAGE_MOVE, message: "Move to %1", insert: "x".repeat(300) });
        assert_eq!(long.len(), 1044);
        assert_eq!(&long[524 + 2 * 258..524 + 2 * 259], &[b'x', 0]);
        assert_eq!(&long[524 + 2 * 259..524 + 2 * 260], &[0, 0]);
        let cleared = description_block(&Description::cleared());
        assert_eq!(i32::from_le_bytes(cleared[0..4].try_into().unwrap()), -1);
        assert!(cleared[4..].iter().all(|&c| c == 0));
    }

    #[test]
    fn effect_decision() {
        let both = COPY | MOVE;
        // Same volume moves, across volumes copies.
        assert_eq!(eff(&[p(r"C:\src\a.txt")], &p(r"C:\dst"), 0, both), MOVE);
        assert_eq!(eff(&[p(r"C:\src\a.txt")], &p(r"D:\dst"), 0, both), COPY);
        assert_eq!(eff(&[p(r"\\s\x\a")], &p(r"\\S\X\dst"), 0, both), MOVE);
        assert_eq!(eff(&[p(r"\\s\x\a")], &p(r"C:\dst"), 0, both), COPY);
        // Modifiers.
        assert_eq!(eff(&[p(r"C:\src\a")], &p(r"C:\dst"), MK_CONTROL, both), COPY, "Ctrl forces copy");
        assert_eq!(eff(&[p(r"C:\src\a")], &p(r"D:\dst"), MK_SHIFT, both), MOVE, "Shift forces move");
        assert_eq!(eff(&[p(r"C:\src\a")], &p(r"C:\dst"), MK_CONTROL | MK_SHIFT, both), COPY, "no links: copy");
        assert_eq!(eff(&[p(r"C:\src\a")], &p(r"C:\dst"), MK_ALT, both), COPY, "Alt (link in Explorer): copy");
        assert_eq!(eff(&[p(r"C:\src\a")], &p(r"C:\dst"), MK_LBUTTON, both), MOVE, "the button bit is no modifier");
        // What the source allows.
        assert_eq!(eff(&[p(r"C:\src\a")], &p(r"C:\dst"), 0, COPY), COPY, "move not allowed: copy");
        assert_eq!(eff(&[p(r"C:\src\a")], &p(r"D:\dst"), 0, MOVE), MOVE, "copy not allowed: move");
        assert_eq!(eff(&[p(r"C:\src\a")], &p(r"C:\dst"), 0, NONE), NONE);
        // Refused drops.
        assert_eq!(eff(&[], &p(r"C:\dst"), 0, both), NONE, "no files");
        assert_eq!(eff(&[p(r"C:\src\a.txt")], &p(r"C:\src"), MK_CONTROL, both), NONE, "into its own folder");
        assert_eq!(eff(&[p(r"C:\src\a.txt")], &p(r"c:\SRC\"), 0, both), NONE, "case and trailing separator");
        assert_eq!(eff(&[p(r"C:\src\d")], &p(r"C:\src\d"), 0, both), NONE, "onto a dragged folder");
        assert_eq!(eff(&[p(r"C:\src\d")], &p(r"C:\src\d\sub"), 0, both), NONE, "into a dragged folder's subfolder");
        assert_eq!(eff(&[p(r"C:\src\d")], &p(r"C:\src\dd"), 0, both), MOVE, "a sibling with a longer name is fine");
        assert_eq!(eff(&[p(r"C:\a\x"), p(r"C:\b\y")], &p(r"C:\b"), 0, both), NONE, "any source's own folder");
        // An optimized move reports none to the source.
        assert_eq!(reported_effect(MOVE), NONE);
        assert_eq!(reported_effect(COPY), COPY);
        assert_eq!(reported_effect(NONE), NONE);
    }

    #[test]
    fn name_cell_follows_the_column_widths() {
        use crate::cols::{Widths, DEFAULT};
        let one = Layout { width: 1000.0, height: 600.0, split: false, ratio: 0.5, tabs: [false; 2], left: 0.0, row_px: 22.0, grid: [None; 2], tile: [crate::icons::TileGeom::LARGE; 2], top: 0.0, bar: 0.0, preview: 0.0 };
        // 3 px pane padding, 28 px row padding; 1000 - 3 - 6 - 350 = 641 on the right.
        assert_eq!(name_cell(&one, 0, DEFAULT.shown(true)), (31.0, 610.0));
        assert_eq!(name_cell(&one, 0, DEFAULT.shown(false)), (31.0, 730.0));
        let wide = Widths { ty: 200.0, size: 110.0, modified: 180.0 };
        assert_eq!(name_cell(&one, 0, wide.shown(true)), (31.0, 470.0));
        // 32 px rows: 42 px of row padding.
        let tall = Layout { row_px: 32.0, ..one };
        assert_eq!(name_cell(&tall, 0, DEFAULT.shown(true)), (45.0, 596.0));
        // The right pane of a split, narrower than its columns: clamped at zero.
        let two = Layout { split: true, width: 600.0, ..one };
        let (x, w) = name_cell(&two, 1, wide.shown(true));
        assert_eq!((x, w), (303.0 + 3.0 + 28.0, 0.0));
    }

    #[test]
    fn hit_testing_and_drop_folder() {
        let one = Layout { width: 1000.0, height: 600.0, split: false, ratio: 0.5, tabs: [false; 2], left: 0.0, row_px: 22.0, grid: [None; 2], tile: [crate::icons::TileGeom::LARGE; 2], top: 0.0, bar: 0.0, preview: 0.0 };
        let two = Layout { split: true, ..one };
        let none = [0.0, 0.0];
        // Rows start at y 75, 22 px each; the list ends 33 px above the bottom (y 567).
        assert_eq!(hit(&one, 500.0, 75.0, none, [10, 0]), Spot::Row(0, 0));
        assert_eq!(hit(&one, 500.0, 96.9, none, [10, 0]), Spot::Row(0, 0));
        assert_eq!(hit(&one, 500.0, 97.0, none, [10, 0]), Spot::Row(0, 1));
        assert_eq!(hit(&one, 500.0, 75.0 + 9.5 * 22.0, none, [10, 0]), Spot::Row(0, 9));
        assert_eq!(hit(&one, 500.0, 75.0 + 10.5 * 22.0, none, [10, 0]), Spot::Pane(0), "below the last row");
        assert_eq!(hit(&one, 500.0, 50.0, none, [10, 0]), Spot::Pane(0), "the header");
        assert_eq!(hit(&one, 500.0, 590.0, none, [1000, 0]), Spot::Pane(0), "the status line");
        assert_eq!(hit(&one, 1.0, 100.0, none, [10, 0]), Spot::Pane(0), "the pane's padding");
        assert_eq!(hit(&one, 500.0, -1.0, none, [10, 0]), Spot::Outside);
        assert_eq!(hit(&one, 1000.0, 100.0, none, [10, 0]), Spot::Outside);
        // Scrolled by 100 rows: the first visible row is row 100.
        assert_eq!(hit(&one, 500.0, 80.0, [-2200.0, 0.0], [1000, 0]), Spot::Row(0, 100));
        assert_eq!(hit(&one, 500.0, 80.0 + 22.0 * 3.0, [-2200.0, 0.0], [1000, 0]), Spot::Row(0, 103));
        // 32 px rows (thumbnails on): row 0 ends at 107, and 100 rows are 3200 px.
        let tall = Layout { row_px: 32.0, ..one };
        assert_eq!(hit(&tall, 500.0, 106.9, none, [10, 0]), Spot::Row(0, 0));
        assert_eq!(hit(&tall, 500.0, 107.0, none, [10, 0]), Spot::Row(0, 1));
        assert_eq!(hit(&tall, 500.0, 80.0 + 32.0 * 3.0, [-3200.0, 0.0], [1000, 0]), Spot::Row(0, 103));
        // The large-icon grid, 5 tiles of 96 x 136 px to a grid row (120 px before P7-M4; the list
        // is 994 px wide, so 10 would fit; the count is the pane's, read from Slint): tiles start
        // at x 3, y 75.
        let grid = Layout { grid: [Some(5), None], ..one };
        assert_eq!(grid.row_px_of(0), 136.0);
        assert_eq!(grid.row_px_of(1), 22.0);
        assert_eq!(hit(&grid, 3.0, 75.0, none, [12, 0]), Spot::Row(0, 0));
        assert_eq!(hit(&grid, 98.9, 210.9, none, [12, 0]), Spot::Row(0, 0), "the tile's far corner");
        assert_eq!(hit(&grid, 99.0, 100.0, none, [12, 0]), Spot::Row(0, 1));
        assert_eq!(hit(&grid, 3.0 + 4.0 * 96.0 + 10.0, 100.0, none, [12, 0]), Spot::Row(0, 4), "the last column");
        assert_eq!(hit(&grid, 3.0 + 5.0 * 96.0 + 10.0, 100.0, none, [12, 0]), Spot::Pane(0), "right of the tiles");
        assert_eq!(hit(&grid, 50.0, 220.0, none, [12, 0]), Spot::Row(0, 5), "the second grid row");
        assert_eq!(hit(&grid, 3.0 + 96.0 + 10.0, 220.0 + 136.0, none, [12, 0]), Spot::Row(0, 11), "the last entry");
        assert_eq!(hit(&grid, 3.0 + 2.0 * 96.0 + 10.0, 220.0 + 136.0, none, [12, 0]), Spot::Pane(0), "past the last entry");
        assert_eq!(hit(&grid, 50.0, 50.0, none, [12, 0]), Spot::Pane(0), "the header");
        // Scrolled by ten grid rows: the first visible tile is entry 50.
        assert_eq!(hit(&grid, 50.0, 80.0, [-1360.0, 0.0], [1000, 0]), Spot::Row(0, 50));
        assert_eq!(hit(&grid, 3.0 + 3.0 * 96.0 + 1.0, 80.0 + 136.0 * 2.0, [-1360.0, 0.0], [1000, 0]), Spot::Row(0, 63));
        // Medium tiles (P6-M2), 12 of 80 x 107 px to a grid row: the column and line from the tile.
        let medium = Layout { grid: [Some(12), None], tile: [crate::icons::TileGeom::MEDIUM, crate::icons::TileGeom::LARGE], ..one };
        assert_eq!(medium.row_px_of(0), 107.0);
        assert_eq!(hit(&medium, 3.0 + 80.0, 75.0 + 107.0, none, [100, 0]), Spot::Row(0, 13));
        assert_eq!(hit(&medium, 3.0 + 79.9, 75.0 + 106.9, none, [100, 0]), Spot::Row(0, 0), "the tile's far corner");
        assert_eq!(hit(&medium, 3.0 + 12.0 * 80.0 + 1.0, 80.0, none, [100, 0]), Spot::Pane(0), "right of the tiles");
        // With the split, each pane has its own view: the right pane stays a 22 px list.
        let both = Layout { split: true, grid: [Some(5), None], ..one };
        assert_eq!(hit(&both, 50.0, 220.0, none, [12, 7]), Spot::Row(0, 5));
        assert_eq!(hit(&both, 700.0, 80.0 + 22.0 * 6.0, none, [12, 7]), Spot::Row(1, 6));
        // Split at 0.5: left pane to x 497, splitter 497..503, right pane from 503.
        assert_eq!(hit(&two, 100.0, 80.0, none, [5, 7]), Spot::Row(0, 0));
        assert_eq!(hit(&two, 496.0, 80.0, none, [5, 7]), Spot::Pane(0), "the left pane's padding");
        assert_eq!(hit(&two, 497.0, 80.0, none, [5, 7]), Spot::Outside, "the splitter");
        assert_eq!(hit(&two, 502.9, 80.0, none, [5, 7]), Spot::Outside);
        assert_eq!(hit(&two, 503.0, 80.0, none, [5, 7]), Spot::Pane(1), "the right pane's padding");
        assert_eq!(hit(&two, 700.0, 80.0 + 22.0 * 6.0, none, [5, 7]), Spot::Row(1, 6));
        assert_eq!(hit(&two, 700.0, 80.0 + 22.0 * 7.0, none, [5, 7]), Spot::Pane(1));
        assert_eq!(hit(&two, 700.0, 80.0, [-44.0, -22.0], [5, 7]), Spot::Row(1, 1), "each pane its own scroll");
        let ratio = Layout { ratio: 0.25, ..two };
        assert_eq!(pane_x(&ratio, 0), (0.0, 247.0));
        assert_eq!(pane_x(&ratio, 1), (253.0, 1000.0));
        // A tab strip moves that pane's rows down by 32 px; the other pane's stay.
        let tabbed = Layout { tabs: [true, false], ..two };
        assert_eq!(hit(&tabbed, 100.0, 80.0, none, [5, 7]), Spot::Pane(0), "the tab strip pushed row 0 down");
        assert_eq!(hit(&tabbed, 100.0, 107.0, none, [5, 7]), Spot::Row(0, 0));
        assert_eq!(hit(&tabbed, 100.0, 135.0, none, [5, 7]), Spot::Row(0, 1));
        assert_eq!(hit(&tabbed, 700.0, 80.0, none, [5, 7]), Spot::Row(1, 0));
        assert_eq!((list_top(&tabbed, 0), list_top(&tabbed, 1)), (107.0, 75.0));
        // The toolbar across the window moves everything down by 44 px and is no target itself;
        // the one at the top of each pane moves the tab strip, the path bar and the list by 30 px
        // (P6-M3).
        let top = Layout { top: TOP_BAR, ..tabbed };
        assert_eq!(hit(&top, 100.0, 20.0, none, [5, 7]), Spot::Outside, "the toolbar");
        assert_eq!((list_top(&top, 0), list_top(&top, 1)), (151.0, 119.0));
        assert_eq!(hit(&top, 700.0, 119.0, none, [5, 7]), Spot::Row(1, 0));
        assert_eq!((tab_top(&top), side_top(&top), crumb_top(&top, 1)), (47.0, 47.0, 50.0));
        let bar = Layout { bar: PANE_BAR, ..tabbed };
        assert_eq!((list_top(&bar, 0), list_top(&bar, 1)), (137.0, 105.0));
        assert_eq!(hit(&bar, 700.0, 104.0, none, [5, 7]), Spot::Pane(1), "the header, under the pane's toolbar");
        assert_eq!(hit(&bar, 700.0, 105.0, none, [5, 7]), Spot::Row(1, 0));
        assert_eq!((tab_top(&bar), crumb_top(&bar, 0), crumb_top(&bar, 1)), (33.0, 68.0, 36.0));
        // A sidebar 180 px wide (plus its 4 px edge): the panes start at 184 and split their own width.
        let side = Layout { left: 184.0, ..two };
        assert_eq!(pane_x(&side, 0), (184.0, 589.0));
        assert_eq!(pane_x(&side, 1), (595.0, 1000.0));
        assert_eq!(hit(&side, 100.0, 80.0, none, [5, 7]), Spot::Outside, "the sidebar");
        assert_eq!(hit(&side, 200.0, 80.0, none, [5, 7]), Spot::Row(0, 0));
        assert_eq!(hit(&side, 600.0, 80.0, none, [5, 7]), Spot::Row(1, 0));
        let single = Layout { left: 12.0, ..one };
        assert_eq!(pane_x(&single, 0), (12.0, 1000.0), "a collapsed sidebar leaves a 12 px strip");
        // The preview column (P8-M1) at 0.35 of the 816 px right of the sidebar: 288.6 px with its
        // handle's left half; the split's ratio applies to what is left.
        let shown = Layout { preview: preview_w(1000.0, 184.0, 0.35), ..side };
        assert!((shown.preview - 288.6).abs() < 1e-3, "{}", shown.preview);
        let (right, line) = (1000.0 - shown.preview, 184.0 + (1000.0 - shown.preview - 184.0) * 0.5);
        assert_eq!(pane_x(&shown, 0), (184.0, line - 3.0));
        assert_eq!(pane_x(&shown, 1), (line + 3.0, right));
        assert_eq!(hit(&shown, 720.0, 80.0, none, [5, 7]), Spot::Outside, "the column");
        assert_eq!(hit(&shown, 700.0, 80.0, none, [5, 7]), Spot::Row(1, 0));

        // Drop folders.
        let f = p(r"C:\work");
        assert_eq!(drop_folder(&f, false, Some(("sub", true))), Some((p(r"C:\work\sub"), true)), "a directory row");
        assert_eq!(drop_folder(&f, false, Some(("a.txt", false))), Some((f.clone(), false)), "a file row: the pane's folder");
        assert_eq!(drop_folder(&f, false, None), Some((f.clone(), false)), "empty space");
        let pc = p("This PC");
        assert_eq!(drop_folder(&pc, true, Some((r"D:\", true))), Some((p(r"D:\"), true)), "a drive row");
        assert_eq!(drop_folder(&pc, true, None), None, "the drive list is no folder");
    }

    #[test]
    fn hit_sidebar_rows_and_tabs() {
        // A 180 px sidebar (panes from 184), split at 0.5: left pane 184..589, right 595..1000.
        let l = Layout { width: 1000.0, height: 600.0, split: true, ratio: 0.5, tabs: [false, true], left: 184.0, row_px: 22.0, grid: [None; 2], tile: [crate::icons::TileGeom::LARGE; 2], top: 0.0, bar: 0.0, preview: 0.0 };
        // Header 26, two drives 24, header 26, a place 24, header 26, a bookmark being renamed 30.
        let rows = [(26.0, false), (24.0, true), (24.0, true), (26.0, false), (24.0, true), (26.0, false), (30.0, true)];
        let s = Strips {
            side_w: 180.0,
            side_scroll: 0.0,
            side_rows: &rows,
            tabs: [(1, 160.0, 0.0), (3, 120.0, 0.0)],
            crumb_row: [(0.0, 0.0); 2],
            crumbs: [&[], &[]],
        };
        let none = [0.0, 0.0];
        let hit = |x: f32, y: f32| hit_all(&l, &s, x, y, none, [5, 7]);
        // Sidebar rows from y 3 (its padding).
        assert_eq!(hit(90.0, 2.0), Spot::Outside, "above the list: its padding");
        assert_eq!(hit(90.0, 3.0), Spot::Outside, "the Drives header");
        assert_eq!(hit(90.0, 29.0), Spot::Side(1), "the first drive");
        assert_eq!(hit(90.0, 52.9), Spot::Side(1));
        assert_eq!(hit(90.0, 53.0), Spot::Side(2));
        assert_eq!(hit(90.0, 80.0), Spot::Outside, "the Places header");
        assert_eq!(hit(90.0, 103.0), Spot::Side(4), "a place");
        assert_eq!(hit(90.0, 130.0), Spot::Outside, "the Bookmarks header");
        assert_eq!(hit(90.0, 160.0), Spot::Side(6), "a bookmark, 30 px while renamed");
        assert_eq!(hit(90.0, 184.0), Spot::Outside, "below the last row");
        assert_eq!(hit(90.0, 580.0), Spot::Outside, "the About row (footer)");
        assert_eq!(hit(181.0, 29.0), Spot::Outside, "the sidebar's resize edge");
        assert_eq!(side_row_top(&rows, 0), 0.0);
        assert_eq!(side_row_top(&rows, 4), 100.0);
        assert_eq!(side_row_top(&rows, 6), 150.0);
        // A scrolled sidebar list (content-y -50): row 4 now sits at 3 + 100 - 50.
        let scrolled = Strips { side_scroll: -50.0, ..s };
        assert_eq!(hit_all(&l, &scrolled, 90.0, 60.0, none, [5, 7]), Spot::Side(4));
        // A collapsed sidebar: no targets (and the 12 px strip is outside the panes).
        let collapsed = Strips { side_w: 0.0, ..s };
        let lc = Layout { left: 12.0, ..l };
        assert_eq!(hit_all(&lc, &collapsed, 5.0, 29.0, none, [5, 7]), Spot::Outside);
        // Tabs: only the right pane shows a strip (3 tabs of 120 px from x 598, y 3..33).
        assert_eq!(hit(600.0, 10.0), Spot::Tab(1, 0));
        assert_eq!(hit(717.9, 10.0), Spot::Tab(1, 0));
        assert_eq!(hit(718.0, 10.0), Spot::Tab(1, 1));
        assert_eq!(hit(598.0 + 2.5 * 120.0, 32.9), Spot::Tab(1, 2));
        assert_eq!(hit(970.0, 10.0), Spot::Pane(1), "past the last tab: the pane");
        assert_eq!(hit(700.0, 2.0), Spot::Pane(1), "above the strip: the pane's padding");
        assert_eq!(hit(700.0, 33.0), Spot::Pane(1), "below the strip: the path bar");
        assert_eq!(hit(300.0, 10.0), Spot::Pane(0), "the left pane has no strip: its path bar");
        // A strip scrolled left by 130 px: x 600 is 132 px into the content, tab 1.
        let s2 = Strips { tabs: [(1, 160.0, 0.0), (4, 120.0, -130.0)], ..s };
        assert_eq!(hit_all(&l, &s2, 600.0, 10.0, none, [5, 7]), Spot::Tab(1, 1));
        // The lists still hit as before.
        assert_eq!(hit(300.0, 80.0), Spot::Row(0, 0));
        assert_eq!(hit(700.0, 110.0), Spot::Row(1, 0));
    }

    #[test]
    fn auto_scroll_near_the_list_edges() {
        let one = Layout { width: 1000.0, height: 600.0, split: false, ratio: 0.5, tabs: [false; 2], left: 0.0, row_px: 22.0, grid: [None; 2], tile: [crate::icons::TileGeom::LARGE; 2], top: 0.0, bar: 0.0, preview: 0.0 };
        // The list runs from y 75 to 567; the edge zones are 24 px inside it.
        assert_eq!(edge_scroll(&one, 500.0, 75.0), Some((0, -1)), "the top edge scrolls up");
        assert_eq!(edge_scroll(&one, 500.0, 98.9), Some((0, -1)));
        assert_eq!(edge_scroll(&one, 500.0, 99.0), None, "inside the list, away from the edges");
        assert_eq!(edge_scroll(&one, 500.0, 300.0), None);
        assert_eq!(edge_scroll(&one, 500.0, 543.0), Some((0, 1)), "the bottom edge scrolls down");
        assert_eq!(edge_scroll(&one, 500.0, 566.9), Some((0, 1)));
        assert_eq!(edge_scroll(&one, 500.0, 567.0), None, "the status line is no edge");
        assert_eq!(edge_scroll(&one, 500.0, 50.0), None, "the header is no edge");
        assert_eq!(edge_scroll(&one, 1.0, 70.0), None, "the pane's padding");
        // Each pane its own edges; a tab strip moves the top edge down.
        let two = Layout { split: true, tabs: [false, true], ..one };
        assert_eq!(edge_scroll(&two, 100.0, 80.0), Some((0, -1)));
        assert_eq!(edge_scroll(&two, 700.0, 70.0), None, "above the tabbed pane's list");
        assert_eq!(edge_scroll(&two, 700.0, 110.0), Some((1, -1)));
        assert_eq!(edge_scroll(&two, 700.0, 560.0), Some((1, 1)));
        assert_eq!(edge_scroll(&two, 500.0, 560.0), None, "the splitter");
        // One row per step, kept in range.
        assert_eq!(scrolled(0.0, 1, 100, 440.0, 22.0), -22.0);
        assert_eq!(scrolled(-22.0, -1, 100, 440.0, 22.0), 0.0);
        assert_eq!(scrolled(0.0, -1, 100, 440.0, 22.0), 0.0, "already at the top");
        assert_eq!(scrolled(-1760.0, 1, 100, 440.0, 22.0), -1760.0, "already at the bottom (2200 - 440)");
        assert_eq!(scrolled(-1750.0, 1, 100, 440.0, 22.0), -1760.0, "clamped to the bottom");
        assert_eq!(scrolled(0.0, 1, 10, 440.0, 22.0), 0.0, "rows that fit do not scroll");
        // 32 px rows (thumbnails on): one 32 px row per step, the bottom at 3200 - 440.
        assert_eq!(scrolled(0.0, 1, 100, 440.0, 32.0), -32.0);
        assert_eq!(scrolled(-2750.0, 1, 100, 440.0, 32.0), -2760.0, "clamped to the bottom");
    }

    #[test]
    fn auto_scroll_sidebar_and_tab_strip() {
        // A 180 px sidebar (panes from 184), split at 0.5: left pane 184..589, right 595..1000;
        // the right pane shows a strip of 6 tabs of 120 px in its 399 px (720 px of tabs).
        let l = Layout { width: 1000.0, height: 600.0, split: true, ratio: 0.5, tabs: [false, true], left: 184.0, row_px: 22.0, grid: [None; 2], tile: [crate::icons::TileGeom::LARGE; 2], top: 0.0, bar: 0.0, preview: 0.0 };
        let rows = [(26.0, false), (24.0, true)];
        let s = Strips {
            side_w: 180.0,
            side_scroll: 0.0,
            side_rows: &rows,
            tabs: [(1, 160.0, 0.0), (6, 120.0, 0.0)],
            crumb_row: [(0.0, 0.0); 2],
            crumbs: [&[], &[]],
        };
        // The sidebar list runs from y 3 to 568 (above the 32 px footer).
        assert_eq!(edge_scroll_all(&l, &s, 90.0, 3.0), Some(Edge::Side(-1)), "the sidebar's top");
        assert_eq!(edge_scroll_all(&l, &s, 90.0, 26.9), Some(Edge::Side(-1)));
        assert_eq!(edge_scroll_all(&l, &s, 90.0, 27.0), None, "inside the sidebar list");
        assert_eq!(edge_scroll_all(&l, &s, 90.0, 550.0), Some(Edge::Side(1)), "the sidebar's bottom");
        assert_eq!(edge_scroll_all(&l, &s, 90.0, 567.9), Some(Edge::Side(1)));
        assert_eq!(edge_scroll_all(&l, &s, 90.0, 568.0), None, "the footer");
        assert_eq!(edge_scroll_all(&l, &s, 90.0, 2.0), None, "above the list");
        // The right pane's strip runs from x 598 to 997, y 3 to 33: its ends scroll it.
        assert_eq!(edge_scroll_all(&l, &s, 598.0, 10.0), Some(Edge::Strip(1, -1)), "the strip's left end");
        assert_eq!(edge_scroll_all(&l, &s, 621.9, 32.9), Some(Edge::Strip(1, -1)));
        assert_eq!(edge_scroll_all(&l, &s, 622.0, 10.0), None, "inside the strip");
        assert_eq!(edge_scroll_all(&l, &s, 973.0, 10.0), Some(Edge::Strip(1, 1)), "the strip's right end");
        assert_eq!(edge_scroll_all(&l, &s, 996.9, 10.0), Some(Edge::Strip(1, 1)));
        assert_eq!(edge_scroll_all(&l, &s, 997.0, 10.0), None, "the pane's padding");
        assert_eq!(edge_scroll_all(&l, &s, 598.0, 33.0), None, "under the strip: the path bar");
        assert_eq!(edge_scroll_all(&l, &s, 190.0, 10.0), None, "the left pane shows no strip");
        // The lists' edges as before (the right list starts under the strip, at 107).
        assert_eq!(edge_scroll_all(&l, &s, 300.0, 80.0), Some(Edge::List(0, -1)));
        assert_eq!(edge_scroll_all(&l, &s, 700.0, 110.0), Some(Edge::List(1, -1)));
        assert_eq!(edge_scroll_all(&l, &s, 700.0, 560.0), Some(Edge::List(1, 1)));
        assert_eq!(edge_scroll_all(&Layout { tabs: [false; 2], ..l }, &s, 700.0, 10.0), None, "no strip: the path bar");
        // A collapsed sidebar: nothing to scroll there.
        let collapsed = Strips { side_w: 0.0, ..s };
        assert_eq!(edge_scroll_all(&Layout { left: 12.0, ..l }, &collapsed, 5.0, 10.0), None);
        // The sidebar's rows of mixed heights, one 24 px row per step: 600 px of rows in 571.
        assert_eq!(scroll_step(0.0, 1, 24.0, 600.0, 571.0), -24.0);
        assert_eq!(scroll_step(-24.0, 1, 24.0, 600.0, 571.0), -29.0, "clamped to the bottom");
        assert_eq!(scroll_step(-29.0, -1, 24.0, 600.0, 571.0), -5.0);
        assert_eq!(scroll_step(0.0, 1, 24.0, 200.0, 571.0), 0.0, "rows that fit do not scroll");
        // The strip, one tab per step: 720 px of tabs in 399 px, the end at -321.
        assert_eq!(scroll_step(0.0, 1, 120.0, 720.0, 399.0), -120.0);
        assert_eq!(scroll_step(-240.0, 1, 120.0, 720.0, 399.0), -321.0, "clamped to the last tab");
        assert_eq!(scroll_step(-321.0, -1, 120.0, 720.0, 399.0), -201.0);
        assert_eq!(scroll_step(-100.0, -1, 120.0, 720.0, 399.0), 0.0, "clamped to the first tab");
        assert_eq!(scroll_step(0.0, 1, 120.0, 360.0, 399.0), 0.0, "tabs that fit do not scroll");
    }

    #[test]
    fn hit_breadcrumb_crumbs() {
        // One pane from x 0 (no sidebar), 600 px tall; the bar's crumb row reported at offset 0,
        // 900 px of it shown; crumbs "This PC", "C:", "Users" at 0, 70 and 110 in the row.
        let l = Layout { width: 1000.0, height: 600.0, split: false, ratio: 0.5, tabs: [false; 2], left: 0.0, row_px: 22.0, grid: [None; 2], tile: [crate::icons::TileGeom::LARGE; 2], top: 0.0, bar: 0.0, preview: 0.0 };
        let crumbs = [(0.0, 54.0), (70.0, 24.0), (110.0, 60.0)];
        let s = Strips { side_w: 0.0, side_scroll: 0.0, side_rows: &[], tabs: [(1, 160.0, 0.0); 2], crumb_row: [(0.0, 900.0), (0.0, 0.0)], crumbs: [&crumbs, &[]] };
        let none = [0.0, 0.0];
        let hit = |s: &Strips, x: f32, y: f32| hit_all(&l, s, x, y, none, [5, 0]);
        // The labels run from y 6 to 32, from x 3 (the pane's padding) plus their x in the row.
        assert_eq!(crumb_top(&l, 0), 6.0);
        assert_eq!(hit(&s, 3.0, 6.0), Spot::Crumb(0, 0));
        assert_eq!(hit(&s, 56.9, 35.9), Spot::Crumb(0, 0));
        assert_eq!(hit(&s, 57.0, 20.0), Spot::Pane(0), "the chevron after This PC");
        assert_eq!(hit(&s, 73.0, 20.0), Spot::Crumb(0, 1));
        assert_eq!(hit(&s, 113.0 + 59.9, 20.0), Spot::Crumb(0, 2));
        assert_eq!(hit(&s, 400.0, 20.0), Spot::Pane(0), "the bar past the crumbs");
        assert_eq!(hit(&s, 80.0, 5.0), Spot::Pane(0), "above the crumbs: the padding");
        assert_eq!(hit(&s, 80.0, 36.0), Spot::Pane(0), "below them: the bar's padding");
        assert_eq!(hit(&s, 80.0, 80.0), Spot::Row(0, 0), "the list as before");
        // A row right-aligned in a narrow bar (offset -60, 100 px shown): This PC is clipped off,
        // "C:" sits at 13..37 and "Users" (53..113) is cut at the bar's end (x 103).
        let narrow = Strips { crumb_row: [(-60.0, 100.0), (0.0, 0.0)], ..s };
        assert_eq!(hit(&narrow, 5.0, 20.0), Spot::Pane(0), "This PC, off the bar's left end");
        assert_eq!(hit(&narrow, 13.0, 20.0), Spot::Crumb(0, 1));
        assert_eq!(hit(&narrow, 102.9, 20.0), Spot::Crumb(0, 2));
        assert_eq!(hit(&narrow, 103.0, 20.0), Spot::Pane(0), "past the bar: the mode glyph");
        // A tab strip moves the bar down by 28 px.
        let lt = Layout { tabs: [true, false], ..l };
        assert_eq!(crumb_top(&lt, 0), 38.0);
        assert_eq!(hit_all(&lt, &s, 80.0, 20.0, none, [5, 0]), Spot::Tab(0, 0), "the strip, not a crumb");
        assert_eq!(hit_all(&lt, &s, 80.0, 40.0, none, [5, 0]), Spot::Crumb(0, 1));
        // No crumbs (the text entry shows): the bar is the pane.
        let entry = Strips { crumbs: [&[], &[]], ..s };
        assert_eq!(hit(&entry, 80.0, 20.0), Spot::Pane(0));
        // Split: each pane its own crumbs; the right pane starts at 503.
        let two = Layout { split: true, ..l };
        let right = [(0.0, 54.0)];
        let both = Strips { crumb_row: [(0.0, 400.0), (0.0, 400.0)], crumbs: [&crumbs, &right], ..s };
        assert_eq!(hit_all(&two, &both, 73.0, 20.0, none, [5, 0]), Spot::Crumb(0, 1));
        assert_eq!(hit_all(&two, &both, 510.0, 20.0, none, [5, 0]), Spot::Crumb(1, 0));
        assert_eq!(hit_all(&two, &both, 580.0, 20.0, none, [5, 0]), Spot::Pane(1));
    }

    #[test]
    fn spring_loaded_folders() {
        let t0 = std::time::Instant::now();
        let at = |ms: u64| t0 + std::time::Duration::from_millis(ms);
        let mut s: Spring<u32> = Spring::default();
        // Nothing armed: nothing fires, and a disarm says so.
        assert_eq!(s.check(at(5000)), None);
        assert!(!s.disarm());
        // Armed over target 1; DragOver's checks before SPRING_MS do nothing.
        assert!(s.arm(1, at(0)));
        assert!(!s.arm(1, at(20)), "the same target keeps its start");
        assert_eq!(s.check(at(980)), None);
        // It fires once at SPRING_MS, and not again while the pointer stays on it.
        assert_eq!(s.check(at(1000)), Some((1, std::time::Duration::from_millis(1000))));
        assert_eq!(s.check(at(3000)), None);
        assert!(!s.arm(1, at(3020)), "still over the opened target: no new arm");
        assert_eq!(s.check(at(5000)), None);
        // Leaving it after it fired: no armed spring to report, and the target may fire again.
        assert!(!s.disarm());
        assert!(s.arm(1, at(6000)));
        assert_eq!(s.check(at(7100)).map(|(t, _)| t), Some(1));
        // Moving to another target restarts the clock.
        assert!(s.arm(2, at(8000)));
        assert!(s.arm(3, at(8500)));
        assert_eq!(s.check(at(9000)), None, "target 3 was armed only 500 ms ago");
        assert_eq!(s.check(at(9600)).map(|(t, _)| t), Some(3));
        // Moving away before the time: disarmed (reported), nothing fires later.
        assert!(s.arm(4, at(10000)));
        assert!(s.disarm());
        assert_eq!(s.check(at(12000)), None);
        // A fired target followed by another one: the new one arms at once.
        assert!(s.arm(5, at(13000)));
        assert_eq!(s.check(at(14000)).map(|(t, _)| t), Some(5));
        assert!(s.arm(6, at(14020)));
        assert_eq!(s.check(at(15020)).map(|(t, _)| t), Some(6));
    }

    #[test]
    fn release_after_drag_never_opens_or_reselects() {
        let mut g = Gesture::default();
        // A click on an unselected row selects on the press; the release does nothing more.
        assert_eq!(g.press(3, false, false, false), Some(SelOp::Only));
        assert_eq!(g.release(), None);
        assert_eq!(g, Gesture::Idle);
        // A click on a selected row (part of a multi-selection) selects only it on the release.
        assert_eq!(g.press(3, true, false, false), None);
        assert_eq!(g.release(), Some((3, SelOp::Only)));
        // Ctrl on a selected row toggles on the release; on an unselected row at once.
        assert_eq!(g.press(4, true, true, false), None);
        assert_eq!(g.release(), Some((4, SelOp::Toggle)));
        assert_eq!(g.press(5, false, true, false), Some(SelOp::Toggle));
        assert_eq!(g.release(), None);
        // Shift selects the range at once.
        assert_eq!(g.press(6, true, false, true), Some(SelOp::Range));
        assert_eq!(g.release(), None);

        // A drag of a selected row keeps the selection: the deferred click never applies.
        assert_eq!(g.press(3, true, false, false), None);
        assert!(g.drag(3));
        assert!(!g.drag(3), "one drag per press");
        g.drag_ended();
        assert_eq!(g, Gesture::Dragged);
        assert!(!g.double_click(), "a double-click Slint derives from the release does not open");
        assert_eq!(g.release(), None, "the release after a drag does not re-select");
        assert_eq!(g, Gesture::Idle);
        // The same while DoDragDrop still runs (a release delivered from inside the loop).
        g.press(2, false, false, false);
        assert!(g.drag(2));
        assert!(!g.double_click());
        assert_eq!(g.release(), None);
        // A drag needs a press on that row.
        assert!(!g.drag(2), "no press");
        g.press(1, false, false, false);
        assert!(!g.drag(2), "another row");

        // A real double-click opens and drops the deferred selection of its second press.
        let mut g = Gesture::default();
        g.press(7, false, false, false);
        g.release();
        assert_eq!(g.press(7, true, false, false), None);
        assert!(g.double_click());
        assert_eq!(g.release(), None, "nothing lands on the folder just opened");
        // drag_ended outside a drag changes nothing.
        g.drag_ended();
        assert_eq!(g, Gesture::Idle);
    }

    #[test]
    fn synthetic_hdrop_data_object_round_trips() {
        let paths = vec![p(r"C:\one.txt"), p(r"C:\dir\two words \u{e9}.bin"), p(r"\\server\share\x")];
        let data = hdrop_data(&paths);
        assert_eq!(read_hdrop(data.0), Some(paths.clone()), "GetData serves the paths");
        assert_eq!(read_hdrop(data.0), Some(paths), "each GetData hands out a fresh block");
        // SAFETY: `data` is a live IDataObject.
        unsafe {
            let v = data_vtbl(data.0);
            let other = FORMATETC { cfFormat: 1, ..hdrop_format() };
            assert_eq!((v.query_get_data)(data.0, &other), DV_E_FORMATETC, "only CF_HDROP");
            let stream = FORMATETC { tymed: 4, ..hdrop_format() };
            assert_eq!((v.query_get_data)(data.0, &stream), DV_E_FORMATETC, "only in an HGLOBAL");
            let mut q: *mut c_void = std::ptr::null_mut();
            assert_eq!((v.query_interface)(data.0, &IID_IDROPTARGET, &mut q), E_NOINTERFACE);
            assert_eq!((v.query_interface)(data.0, &IID_IDATAOBJECT, &mut q), S_OK);
            assert_eq!((v.release)(q), 1, "QueryInterface took a reference; ours remains");
        }
        assert_eq!(read_hdrop(hdrop_data(&[]).0), Some(Vec::new()));
        assert_eq!(read_dword(hdrop_data(&[]).0, CFSTR_LOGICALPERFORMEDDROPEFFECT), None);
    }

    /// The shell's own data object for two real files offers CF_HDROP with their paths (what our
    /// target reads when a pane drags into the other).
    #[test]
    fn shell_data_object_offers_hdrop() {
        let root = std::env::temp_dir().join(format!("twinfm-m5-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.txt"), b"a").unwrap();
        std::fs::write(root.join("b c.txt"), b"b").unwrap();
        // SAFETY: an STA for this test thread, balanced below.
        let com = unsafe {
            windows_sys::Win32::System::Com::CoInitializeEx(std::ptr::null(), windows_sys::Win32::System::Com::COINIT_APARTMENTTHREADED as u32)
        };
        {
            let names = vec!["a.txt".to_string(), "b c.txt".to_string()];
            let (folder_pidl, _folder, children) = shellmenu::parse_items(std::ptr::null_mut(), &root, &names).unwrap();
            let apidl: Vec<*const _> = children.iter().map(|c| c.0 as *const _).collect();
            let mut q: *mut c_void = std::ptr::null_mut();
            // SAFETY: as in `drag`.
            let hr = unsafe { SHCreateDataObject(folder_pidl.0, 2, apidl.as_ptr(), std::ptr::null_mut(), &IID_IDATAOBJECT, &mut q) };
            assert!(hr >= 0 && !q.is_null(), "SHCreateDataObject 0x{:08X}", hr as u32);
            let data = Com(q);
            let got = read_hdrop(data.0).expect("CF_HDROP offered");
            // The shell returns long names; a runner's %TEMP% can be an 8.3 path (RUNNER~1).
            let canon = |p: &std::path::Path| std::fs::canonicalize(p).unwrap();
            let got: Vec<_> = got.iter().map(|p| canon(p)).collect();
            assert_eq!(got, vec![canon(&root.join("a.txt")), canon(&root.join("b c.txt"))]);
            // The optimized-move notes are stored and read back.
            set_dword(data.0, CFSTR_LOGICALPERFORMEDDROPEFFECT, MOVE);
            assert_eq!(read_dword(data.0, CFSTR_LOGICALPERFORMEDDROPEFFECT), Some(MOVE));
            // So is a drop description (P5-M3): the shell's data object takes the 1044-byte block.
            let d = description(COPY, Some(&root));
            assert_eq!(set_block(data.0, CFSTR_DROPDESCRIPTION, &description_block(&d)), S_OK);
            let back = read_hglobal(data.0, registered_format(CFSTR_DROPDESCRIPTION)).expect("DropDescription stored");
            assert_eq!(&back[..1044], description_block(&d).as_slice());
        }
        // P11-M2: a search tab's results from two folders, as the desktop's absolute PIDLs.
        std::fs::create_dir_all(root.join("s1")).unwrap();
        std::fs::create_dir_all(root.join("s2")).unwrap();
        std::fs::write(root.join(r"s1\x.txt"), b"x").unwrap();
        std::fs::write(root.join(r"s2\y.txt"), b"y").unwrap();
        {
            let names = vec![r"s1\x.txt".to_string(), r"s2\y.txt".to_string()];
            let (folder_pidl, _folder, children) = shellmenu::parse_items(std::ptr::null_mut(), &root, &names).unwrap();
            let apidl: Vec<*const _> = children.iter().map(|c| c.0 as *const _).collect();
            let mut q: *mut c_void = std::ptr::null_mut();
            // SAFETY: as in `drag`.
            let hr = unsafe { SHCreateDataObject(folder_pidl.0, 2, apidl.as_ptr(), std::ptr::null_mut(), &IID_IDATAOBJECT, &mut q) };
            assert!(hr >= 0 && !q.is_null(), "SHCreateDataObject over two folders 0x{:08X}", hr as u32);
            let data = Com(q);
            let canon = |p: &std::path::Path| std::fs::canonicalize(p).unwrap();
            let got: Vec<_> = read_hdrop(data.0).expect("CF_HDROP offered").iter().map(|p| canon(p)).collect();
            assert_eq!(got, vec![canon(&root.join(r"s1\x.txt")), canon(&root.join(r"s2\y.txt"))]);
        }
        if com >= 0 {
            // SAFETY: pairs the CoInitializeEx above.
            unsafe { windows_sys::Win32::System::Com::CoUninitialize() };
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn drag_image_composition() {
        let g = ImageGeom::new(1.0);
        assert_eq!((g.size, g.icon, g.inset, g.cursor), (96, 64, 8, (16, 16)));
        let g2 = ImageGeom::new(1.5);
        assert_eq!((g2.size, g2.icon, g2.inset), (144, 96, 12));
        assert_eq!(badge_text(3), "3");
        assert_eq!(badge_text(150), "99+");
        // An opaque red icon: copied to the inset as BGRA.
        let red = slint::Rgba8Pixel { r: 255, g: 0, b: 0, a: 255 };
        let icon = vec![red; 64 * 64];
        let one = compose_image(&g, Some(&icon), 1, None);
        let at = |img: &[u8], x: usize, y: usize| img[(y * 96 + x) * 4..(y * 96 + x) * 4 + 4].to_vec();
        assert_eq!(one.len(), 96 * 96 * 4);
        assert_eq!(at(&one, 8, 8), vec![0, 0, 255, 255], "the icon's first pixel, BGRA");
        assert_eq!(at(&one, 7, 8), vec![0, 0, 0, 0], "left of the icon: transparent");
        assert_eq!(at(&one, 72, 72), vec![0, 0, 0, 0], "past the icon, no badge for one item");
        // Several items: the badge's centre is accent blue, opaque; white text where covered.
        let (cx, cy) = g.badge_centre();
        let many = compose_image(&g, Some(&icon), 5, None);
        assert_eq!(at(&many, cx as usize, cy as usize), vec![215, 120, 0, 255]);
        let tw = (g.badge_r * 2.0) as u32;
        let cov = vec![255u8; (tw * tw) as usize];
        let text = compose_image(&g, Some(&icon), 5, Some((&cov, tw)));
        assert_eq!(at(&text, cx as usize, cy as usize), vec![255, 255, 255, 255]);
        // A wrong-sized icon is ignored rather than misread.
        assert_eq!(at(&compose_image(&g, Some(&icon[..10]), 1, None), 8, 8), vec![0, 0, 0, 0]);
    }

    #[test]
    fn drag_image_icon_source_order() {
        // The grid's real picture first; then what the worker answered at the press, a thumbnail
        // (P9-M4) or an icon; else the shell, asked at the drag's start.
        assert_eq!(IconFrom::pick(true, Some(true)), IconFrom::Tile);
        assert_eq!(IconFrom::pick(true, None), IconFrom::Tile);
        assert_eq!(IconFrom::pick(false, Some(true)), IconFrom::Thumbnail);
        assert_eq!(IconFrom::pick(false, Some(false)), IconFrom::Worker);
        assert_eq!(IconFrom::pick(false, None), IconFrom::Shell);
        assert_eq!(IconFrom::Thumbnail.label(), "the thumbnail");
        assert_eq!(IconFrom::Worker.label(), "the worker");
    }

    #[test]
    fn tile_drag_image_holds_the_tile_picture() {
        // Out of the grid (P4-M3): the tile's 96 px picture with room for the badge.
        let g = ImageGeom::tile(1.0, crate::icons::TileGeom::LARGE);
        assert_eq!((g.size, g.icon, g.inset, g.cursor), (128, 96, 16, (16, 16)));
        let (cx, cy) = g.badge_centre();
        assert!(cx + g.badge_r < g.size as f32 && cy + g.badge_r < g.size as f32, "the badge fits");
        let g2 = ImageGeom::tile(1.5, crate::icons::TileGeom::LARGE);
        assert_eq!((g2.size, g2.icon, g2.inset), (192, 144, 24));
        // Out of medium tiles (P6-M2): the 48 px picture in an 80 px image, the badge inside.
        let m = ImageGeom::tile(1.0, crate::icons::TileGeom::MEDIUM);
        assert_eq!((m.size, m.icon, m.inset), (80, 48, 16));
        let (mx, my) = m.badge_centre();
        assert!(mx + m.badge_r < m.size as f32 && my + m.badge_r < m.size as f32, "the badge fits");
        let blue = slint::Rgba8Pixel { r: 0, g: 0, b: 255, a: 255 };
        let pic = vec![blue; 96 * 96];
        let one = compose_image(&g, Some(&pic), 1, None);
        let at = |img: &[u8], x: usize, y: usize| img[(y * 128 + x) * 4..(y * 128 + x) * 4 + 4].to_vec();
        assert_eq!(one.len(), 128 * 128 * 4);
        assert_eq!(at(&one, 16, 16), vec![255, 0, 0, 255], "the picture's first pixel, BGRA");
        assert_eq!(at(&one, 111, 111), vec![255, 0, 0, 255], "its last");
        assert_eq!(at(&one, 112, 112), vec![0, 0, 0, 0], "past it");
    }

    /// The shell's drag-drop helper takes a drag image for the shell's data object over a real
    /// file (the data object `drag` hands to DoDragDrop), and the image lands on it.
    #[test]
    fn drag_source_helper_takes_a_bitmap() {
        let root = std::env::temp_dir().join(format!("twinfm-m5-test-img-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.txt"), b"a").unwrap();
        // SAFETY: an STA for this test thread, balanced below.
        let com = unsafe {
            windows_sys::Win32::System::Com::CoInitializeEx(std::ptr::null(), windows_sys::Win32::System::Com::COINIT_APARTMENTTHREADED as u32)
        };
        {
            let names = vec!["a.txt".to_string()];
            let (folder_pidl, _folder, children) = shellmenu::parse_items(std::ptr::null_mut(), &root, &names).unwrap();
            let apidl: Vec<*const _> = children.iter().map(|c| c.0 as *const _).collect();
            let mut q: *mut c_void = std::ptr::null_mut();
            // SAFETY: as in `drag`.
            let hr = unsafe { SHCreateDataObject(folder_pidl.0, 1, apidl.as_ptr(), std::ptr::null_mut(), &IID_IDATAOBJECT, &mut q) };
            assert!(hr >= 0 && !q.is_null());
            let data = Com(q);
            let helper = create_helper(&IID_IDRAGSOURCEHELPER).expect("CoCreateInstance(CLSID_DragDropHelper)");
            let g = ImageGeom { size: 16, icon: 8, inset: 2, badge_r: 3.0, cursor: (2, 2) };
            let bgra = compose_image(&g, Some(&vec![slint::Rgba8Pixel { r: 0, g: 128, b: 0, a: 255 }; 64]), 2, None);
            assert_eq!(init_drag_image(helper.0, data.0, &bgra, g.size, g.cursor), S_OK, "InitializeFromBitmap");
            // The helper stored the image on the data object in the shell's own format.
            let fmt = FORMATETC { cfFormat: registered_format(windows_sys::core::w!("DragImageBits")), ..hdrop_format() };
            // SAFETY: a live IDataObject.
            assert_eq!(unsafe { (data_vtbl(data.0).query_get_data)(data.0, &fmt) }, S_OK, "DragImageBits offered");
            // The target helper exists too.
            assert!(create_helper(&IID_IDROPTARGETHELPER).is_ok(), "IDropTargetHelper");
            // A bitmap of the wrong size is refused before the helper sees it.
            assert!(init_drag_image(helper.0, data.0, &bgra[..8], g.size, g.cursor) < 0);
        }
        if com >= 0 {
            // SAFETY: pairs the CoInitializeEx above.
            unsafe { windows_sys::Win32::System::Com::CoUninitialize() };
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    fn tree(root: &Path) -> Vec<(String, Option<Vec<u8>>)> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let rel = e.path().strip_prefix(root).unwrap().to_string_lossy().into_owned();
                if e.path().is_dir() {
                    stack.push(e.path());
                    out.push((rel, None));
                } else {
                    out.push((rel, Some(std::fs::read(e.path()).unwrap())));
                }
            }
        }
        out.sort();
        out
    }

    /// Virtual files through our minimal data object, served as HGLOBALs and as IStreams: the
    /// descriptors round-trip, a nested path makes its folders, a taken name gets " (2)" (a folder
    /// so renamed takes its contents with it), names that are no file names are left out.
    #[test]
    fn virtual_files_round_trip() {
        let root = std::env::temp_dir().join(format!("twinfm-p3m4-virtual-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let items: Vec<(String, Option<Vec<u8>>)> = vec![
            ("top.txt".into(), Some(b"hello".to_vec())),
            ("dir".into(), None),
            (r"dir\nested\deep.bin".into(), Some((0..70_000u32).map(|i| i as u8).collect())),
            (r"..\escape.txt".into(), Some(b"stays inside".to_vec())),
            (r"C:\abs.txt".into(), Some(b"left out".to_vec())),
            ("empty.dat".into(), Some(Vec::new())),
        ];
        for stream in [false, true] {
            let dest = root.join(if stream { "stream" } else { "hglobal" });
            std::fs::create_dir_all(&dest).unwrap();
            let data = virtual_data(&items, stream, None, false);
            let Offer::Virtual(got) = offer(data.0) else { panic!("virtual files offered") };
            assert_eq!(
                got.iter().map(|i| (i.index, i.name.as_str(), i.dir, i.size)).collect::<Vec<_>>(),
                vec![
                    (0, "top.txt", false, Some(5)),
                    (1, "dir", true, Some(0)),
                    (2, r"dir\nested\deep.bin", false, Some(70_000)),
                    (3, "escape.txt", false, Some(12)),
                    (5, "empty.dat", false, Some(0)),
                ],
                "`..` dropped, a drive-qualified name left out, the indexes kept"
            );
            let mut seen = Vec::new();
            let r = extract(data.0, &got, &dest, Some(SYNC_CAP), |k, n, p| seen.push((k, n, p.strip_prefix(&dest).unwrap().to_path_buf())));
            assert_eq!(r, Ok((4, 70_017)), "stream={stream}");
            assert_eq!(seen.last().map(|s| (s.0, s.1)), Some((4, 4)));
            let want: Vec<(String, Option<Vec<u8>>)> = vec![
                ("dir".into(), None),
                (r"dir\nested".into(), None),
                (r"dir\nested\deep.bin".into(), items[2].1.clone()),
                ("empty.dat".into(), Some(Vec::new())),
                ("escape.txt".into(), Some(b"stays inside".to_vec())),
                ("top.txt".into(), Some(b"hello".to_vec())),
            ];
            assert_eq!(tree(&dest), want);
            // The same drop again: every top-level name is taken.
            assert_eq!(extract(data.0, &got, &dest, Some(SYNC_CAP), |_, _, _| {}), Ok((4, 70_017)));
            let names: Vec<String> = tree(&dest).into_iter().map(|(n, _)| n).collect();
            for n in ["top (2).txt", "dir (2)", r"dir (2)\nested\deep.bin", "escape (2).txt", "empty (2).dat"] {
                assert!(names.contains(&n.to_string()), "{n} in {names:?}");
            }
            // Past the cap: refused while reading.
            let small = root.join(format!("cap-{stream}"));
            std::fs::create_dir_all(&small).unwrap();
            let e = extract(data.0, &got, &small, Some(100), |_, _, _| {}).unwrap_err();
            assert!(e.contains("over the drop's size limit"), "{e}");
        }
        let big = [
            VirtualItem { index: 0, name: "a".into(), dir: false, size: Some(SYNC_CAP) },
            VirtualItem { index: 1, name: "b".into(), dir: false, size: Some(1) },
        ];
        assert_eq!(over_cap(&big), Some(SYNC_CAP + 1));
        assert_eq!(over_cap(&big[..1]), None, "exactly the cap is allowed");
        assert_eq!(parse_descriptors(&descriptor_block(&[("x", Some(1))])[..100]), None, "a cut block");
        // The ANSI FileGroupDescriptor: 332-byte records, the name in the ANSI code page.
        let mut a = 1u32.to_le_bytes().to_vec();
        let mut rec = vec![0u8; FD_LEN_A];
        rec[0..4].copy_from_slice(&((FD_FILESIZE) as u32).to_le_bytes());
        rec[68..72].copy_from_slice(&7u32.to_le_bytes());
        rec[72..80].copy_from_slice(b"ansi.txt");
        a.extend(rec);
        assert_eq!(parse_descriptors_as(&a, false), Some(vec![VirtualItem { index: 0, name: "ansi.txt".into(), dir: false, size: Some(7) }]));
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// A data object with both files and virtual files is a drop of the files; the virtual effect
    /// is a copy whatever the source allows.
    #[test]
    fn hdrop_wins_over_virtual_files() {
        let paths = vec![p(r"C:\real\one.txt")];
        let items = vec![("one.txt".to_string(), Some(b"1".to_vec()))];
        assert_eq!(offer(virtual_data(&items, false, Some(&paths), false).0), Offer::Files(paths.clone()));
        assert!(matches!(offer(virtual_data(&items, true, None, false).0), Offer::Virtual(v) if v.len() == 1));
        assert_eq!(offer(virtual_data(&[], false, None, false).0), Offer::Nothing, "no items, nothing to drop");
        assert_eq!(offer(hdrop_data(&paths).0), Offer::Files(paths.clone()));
        // During our own drag out of an archive its virtual files are its in-archive paths.
        let own = vec![PathBuf::from("C:/d/x.zip/sub/one.txt")];
        OWN_ARCHIVE_SOURCES.with(|s| *s.borrow_mut() = Some(own.clone()));
        assert_eq!(target_offer(virtual_data(&items, false, None, false).0), Offer::Archive(own));
        assert_eq!(target_offer(hdrop_data(&paths).0), Offer::Files(paths.clone()), "files stay files");
        OWN_ARCHIVE_SOURCES.with(|s| s.borrow_mut().take());
        assert!(matches!(target_offer(virtual_data(&items, false, None, false).0), Offer::Virtual(_)), "a foreign source");
        assert_eq!(
            (virtual_effect(COPY | MOVE), virtual_effect(MOVE), virtual_effect(COPY), virtual_effect(NONE)),
            (COPY, COPY, COPY, NONE)
        );
    }

    /// `p` with its 8.3 components expanded (`canonicalize` without the `\\?\` prefix), so paths
    /// built from it compare equal to what the shell hands back for items inside a zip, which
    /// cannot be canonicalised themselves. The CI runner's %TEMP% is an 8.3 path (RUNNER~1).
    fn long_path(p: &std::path::Path) -> PathBuf {
        let c = std::fs::canonicalize(p).unwrap();
        let s = c.to_string_lossy();
        PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s))
    }

    /// The shell's zip folder's own data object (what a drag out of a zip pane carries) offers
    /// virtual files and no CF_HDROP, can be read asynchronously, and extracts through `extract`.
    #[test]
    fn zip_folder_data_object_offers_virtual_files() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!("twinfm-p3m4-dnd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("out")).unwrap();
        let root = long_path(&root);
        let zip = root.join("t.zip");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&zip).unwrap());
        for (name, data) in [("sub/a.txt", &b"alpha"[..]), ("sub/inner/b.txt", b"beta beta")] {
            w.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
        // SAFETY: an STA for this test thread, balanced below.
        let com = unsafe {
            windows_sys::Win32::System::Com::CoInitializeEx(std::ptr::null(), windows_sys::Win32::System::Com::COINIT_APARTMENTTHREADED as u32)
        };
        {
            let data = archive_data(0, &zip.join("sub"), &["a.txt".to_string(), "inner".to_string()]).unwrap();
            assert_eq!(read_hdrop(data.0), None, "no CF_HDROP from a zip folder");
            let Offer::Virtual(items) = offer(data.0) else { panic!("virtual files offered") };
            let listed: Vec<(&str, bool)> = items.iter().map(|i| (i.name.as_str(), i.dir)).collect();
            assert!(
                listed.contains(&("a.txt", false)) && listed.contains(&("inner", true)) && listed.contains(&(r"inner\b.txt", false)),
                "{listed:?}"
            );
            assert!(data.query(&IID_IDATAOBJECTASYNCCAPABILITY).is_some(), "the zip folder supports async drops");
            // Its items' PIDLs ride along: a drop of it (from Explorer's zip view too) goes to the
            // extraction job (P7-M3), not the synchronous reader.
            match target_offer(data.0) {
                Offer::Archive(mut p) => {
                    p.sort();
                    let want = [zip.join("sub").join("a.txt"), zip.join("sub").join("inner")];
                    assert!(p.len() == 2 && p.iter().zip(&want).all(|(a, b)| a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())), "{p:?}");
                }
                other => panic!("the zip folder's data object as a drop: {other:?}"),
            }
            // A shell data object over the same items offers only a Shell IDList Array: their
            // in-archive paths, for the extraction job.
            let (fp, _folder, children) = shellmenu::parse_items(std::ptr::null_mut(), &zip.join("sub"), &["a.txt".to_string()]).unwrap();
            let apidl: Vec<*const _> = children.iter().map(|c| c.0 as *const _).collect();
            let mut q: *mut c_void = std::ptr::null_mut();
            // SAFETY: as in `drag`.
            let hr = unsafe { SHCreateDataObject(fp.0, 1, apidl.as_ptr(), std::ptr::null_mut(), &IID_IDATAOBJECT, &mut q) };
            assert!(hr >= 0 && !q.is_null());
            let idl = Com(q);
            assert!(read_descriptors(idl.0).is_none(), "no descriptors");
            match offer(idl.0) {
                Offer::Archive(p) => assert!(p.len() == 1 && p[0].to_string_lossy().eq_ignore_ascii_case(&zip.join("sub").join("a.txt").to_string_lossy()), "{p:?}"),
                other => panic!("{other:?}"),
            }
            assert_eq!(extract(data.0, &items, &root.join("out"), Some(SYNC_CAP), |_, _, _| {}), Ok((2, 14)));
            assert_eq!(std::fs::read(root.join("out").join("a.txt")).unwrap(), b"alpha");
            assert_eq!(std::fs::read(root.join("out").join("inner").join("b.txt")).unwrap(), b"beta beta");
        }
        if com >= 0 {
            // SAFETY: pairs the CoInitializeEx above.
            unsafe { windows_sys::Win32::System::Com::CoUninitialize() };
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// A source that asks for asynchronous extraction: Drop starts the operation, a worker reads
    /// the contents through the marshalled data object (each call comes back to this STA, which
    /// pumps), writes the files and ends the operation with a copy.
    #[test]
    fn async_virtual_drop_on_a_worker() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage};
        let root = std::env::temp_dir().join(format!("twinfm-p3m4-async-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        // SAFETY: an STA for this test thread, balanced below.
        let com = unsafe {
            windows_sys::Win32::System::Com::CoInitializeEx(std::ptr::null(), windows_sys::Win32::System::Com::COINIT_APARTMENTTHREADED as u32)
        };
        {
            let items: Vec<(String, Option<Vec<u8>>)> =
                (0..20).map(|k| (format!(r"sub\f{k}.bin"), Some(vec![k as u8; 1000 + k]))).collect();
            let data = virtual_data(&items, true, None, true);
            let Offer::Virtual(listed) = offer(data.0) else { panic!("virtual files offered") };
            virtual_drop(data.0, listed, 0, root.clone());
            let t0 = std::time::Instant::now();
            while virtual_drops_running() && t0.elapsed() < std::time::Duration::from_secs(30) {
                let mut msg: MSG = MSG::default();
                // SAFETY: plain message pump on this thread.
                while unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
                    unsafe {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert!(!virtual_drops_running(), "the worker finished within 30 s");
            assert_eq!(mini_ended(&data), Some((S_OK, COPY)), "EndOperation reported the copy");
            for k in 0..20 {
                assert_eq!(std::fs::read(root.join("sub").join(format!("f{k}.bin"))).unwrap(), vec![k as u8; 1000 + k]);
            }
        }
        if com >= 0 {
            // SAFETY: pairs the CoInitializeEx above.
            unsafe { windows_sys::Win32::System::Com::CoUninitialize() };
        }
        std::fs::remove_dir_all(&root).unwrap();
    }
}

