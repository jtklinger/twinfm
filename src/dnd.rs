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
//! loop, outside any winit handler, so Slint renders the highlight normally. For a drag we started
//! ourselves, `DoDragDrop` runs a modal loop inside a winit handler (the pointer move that crossed
//! the threshold), and OLE calls our target directly from that loop. Nothing of Slint runs there
//! (see `shellmenu`'s modal-loop notes): property sets land, but the window does not repaint
//! until `DoDragDrop` returns.
//!
//! **Modal-loop rule.** `drag` must be called with no `RefCell` borrow of app state held. The
//! target methods take what they need from the app state under short, non-reentrant borrows
//! (`crate::drop_spot`), set the two highlight properties, and never submit a job themselves:
//! `Drop` hands the job to `slint::invoke_from_event_loop`, which runs after the drag loop has
//! returned (our own drag) or at the next turn of the event loop (another program's drag).
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
    GlobalFree, HWND, OLE_E_ADVISENOTSUPPORTED, POINT, POINTL, S_OK,
};
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
use windows_sys::Win32::System::Com::{DVASPECT_CONTENT, FORMATETC, STGMEDIUM, TYMED_HGLOBAL};
use windows_sys::Win32::System::DataExchange::RegisterClipboardFormatW;
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows_sys::Win32::System::Ole::{
    CF_HDROP, DROPEFFECT_COPY, DROPEFFECT_MOVE, DROPEFFECT_NONE, DoDragDrop, MK_ALT, RegisterDragDrop, ReleaseStgMedium,
    RevokeDragDrop,
};
use windows_sys::Win32::System::SystemServices::{MK_CONTROL, MK_LBUTTON, MK_RBUTTON, MK_SHIFT};
use windows_sys::Win32::UI::Shell::{CFSTR_LOGICALPERFORMEDDROPEFFECT, CFSTR_PERFORMEDDROPEFFECT, SHCreateDataObject};
use windows_sys::core::{BOOL, GUID, HRESULT, PCWSTR};

pub const NONE: u32 = DROPEFFECT_NONE;
pub const COPY: u32 = DROPEFFECT_COPY;
pub const MOVE: u32 = DROPEFFECT_MOVE;

// ---------------------------------------------------------------------------------------------
// Pure decisions (unit-tested)
// ---------------------------------------------------------------------------------------------

/// `IDropSource::QueryContinueDrag`: Escape or the right button cancels, releasing the left
/// button drops, anything else keeps dragging.
pub fn continue_drag(escape: bool, lbutton: bool, rbutton: bool) -> HRESULT {
    if escape || rbutton {
        DRAGDROP_S_CANCEL
    } else if !lbutton {
        DRAGDROP_S_DROP
    } else {
        S_OK
    }
}

/// The volume a path lives on, as a comparable key: `c:` for a drive path, `\\server\share` for a
/// UNC path (lowercase, `\\?\` and `\\?\UNC\` prefixes removed). A volume mounted in a folder
/// counts as the drive that holds the folder.
pub fn volume_key(p: &Path) -> String {
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
/// drop, else `decide_effect` with the volume of the first source.
pub fn effect_for(sources: &[PathBuf], dest: &Path, keys: u32, allowed: u32) -> u32 {
    match sources.first() {
        None => NONE,
        Some(_) if refused(sources, dest) => NONE,
        Some(first) => decide_effect(keys, allowed, volume_key(first) == volume_key(dest)),
    }
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
}

/// The list's top inside a pane (pane.slint): 3 px padding, 32 px path entry, 2 px spacing,
/// 24 px header, 2 px spacing. Its bottom is 23 px above the pane's (2 px spacing, 18 px status
/// line, 3 px padding), its sides 3 px in.
pub const LIST_TOP: f32 = 63.0;
pub const LIST_BOTTOM_GAP: f32 = 23.0;
pub const PANE_PAD: f32 = 3.0;
pub const ROW_PX: f32 = crate::icons::ROW_PX;
/// The tab strip above the path bar (26 px) and the layout spacing under it (2 px), present only
/// when the pane has more than one tab.
pub const TAB_STRIP: f32 = 28.0;

/// The top of the pane's list, in window px.
pub fn list_top(l: &Layout, pane: usize) -> f32 {
    if l.tabs[pane] { LIST_TOP + TAB_STRIP } else { LIST_TOP }
}

/// Where a window point lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spot {
    /// Outside both panes (the splitter, or off the window).
    Outside,
    /// In the pane but not over a row (path bar, header, empty space, status line).
    Pane(usize),
    /// Over row `.1` of pane `.0`.
    Row(usize, usize),
}

/// The pane's x range: the left pane is the whole window without the split, else up to 3 px left
/// of the split line; the right pane starts 3 px right of it (the splitter is the 6 px between).
pub fn pane_x(l: &Layout, pane: usize) -> (f32, f32) {
    match (l.split, pane) {
        (false, _) => (0.0, l.width),
        (true, 0) => (0.0, l.width * l.ratio - 3.0),
        (true, _) => (l.width * l.ratio + 3.0, l.width),
    }
}

/// Hit-test a window point (logical px) against the panes and their rows. `content_y` is each
/// list's scroll offset (zero or negative), `len` its row count.
pub fn hit(l: &Layout, x: f32, y: f32, content_y: [f32; 2], len: [usize; 2]) -> Spot {
    if y < 0.0 || y >= l.height {
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
    let r = ((y - list_top(l, pane) - content_y[pane]) / ROW_PX).floor();
    if r >= 0.0 && (r as usize) < len[pane] { Spot::Row(pane, r as usize) } else { Spot::Pane(pane) }
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
    /// `DoDragDrop` is running for a drag that started on `row`.
    Dragging { row: usize },
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
                *self = Gesture::Dragging { row };
                true
            }
            _ => false,
        }
    }

    /// `DoDragDrop` returned (dropped, cancelled or failed).
    pub fn drag_ended(&mut self) {
        if matches!(self, Gesture::Dragging { .. }) {
            *self = Gesture::Dragged;
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
    if data.is_null() {
        return None;
    }
    let fmt = hdrop_format();
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
        bytes.and_then(|b| clipboard::parse_dropfiles(&b))
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
    let fmt = FORMATETC { cfFormat: registered_format(name), ..hdrop_format() };
    // SAFETY: a moveable 4-byte block handed to SetData with fRelease = TRUE (the data object
    // owns it on success); freed here if SetData refuses it.
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE, 4);
        if h.is_null() {
            return;
        }
        let p = GlobalLock(h) as *mut u8;
        if p.is_null() {
            GlobalFree(h);
            return;
        }
        std::ptr::copy_nonoverlapping(value.to_le_bytes().as_ptr(), p, 4);
        GlobalUnlock(h);
        let mut medium: STGMEDIUM = std::mem::zeroed();
        medium.tymed = TYMED_HGLOBAL as u32;
        medium.u.hGlobal = h;
        if (data_vtbl(data).set_data)(data, &fmt, &medium, 1) < 0 {
            GlobalFree(h);
        }
    }
}

// --- A minimal IDataObject serving CF_HDROP (the driver's `dropfiles`, and the tests) ---------

#[repr(C)]
struct HdropData {
    header: Header,
    /// The `CF_HDROP` block (`clipboard::dropfiles_block`).
    block: Vec<u8>,
}

static HDROP_DATA_VTBL: DataObjectVtbl = DataObjectVtbl {
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
    // SAFETY: created by `hdrop_data` with Box::into_raw.
    drop(unsafe { Box::from_raw(this as *mut HdropData) });
}

unsafe extern "system" fn hd_query(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    unsafe { query_impl(this, iid, &IID_IDATAOBJECT, out) }
}
unsafe extern "system" fn hd_add_ref(this: *mut c_void) -> u32 {
    unsafe { add_ref_impl(this) }
}
unsafe extern "system" fn hd_release(this: *mut c_void) -> u32 {
    unsafe { release_impl(this, hd_free) }
}

fn serves_hdrop(fmt: *const FORMATETC) -> bool {
    // SAFETY: COM contract: a valid FORMATETC pointer (checked for null).
    !fmt.is_null() && unsafe { (*fmt).cfFormat == CF_HDROP && (*fmt).tymed & TYMED_HGLOBAL as u32 != 0 }
}

unsafe extern "system" fn hd_get_data(this: *mut c_void, fmt: *const FORMATETC, medium: *mut STGMEDIUM) -> HRESULT {
    if !serves_hdrop(fmt) || medium.is_null() {
        return DV_E_FORMATETC;
    }
    // SAFETY: `this` is an HdropData; a fresh moveable block the caller frees with
    // ReleaseStgMedium (pUnkForRelease null).
    unsafe {
        let block = &(*(this as *const HdropData)).block;
        let h = GlobalAlloc(GMEM_MOVEABLE, block.len());
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
        let mut m: STGMEDIUM = std::mem::zeroed();
        m.tymed = TYMED_HGLOBAL as u32;
        m.u.hGlobal = h;
        *medium = m;
    }
    S_OK
}
unsafe extern "system" fn hd_get_data_here(_: *mut c_void, _: *const FORMATETC, _: *mut STGMEDIUM) -> HRESULT {
    E_NOTIMPL
}
unsafe extern "system" fn hd_query_get_data(_: *mut c_void, fmt: *const FORMATETC) -> HRESULT {
    if serves_hdrop(fmt) { S_OK } else { DV_E_FORMATETC }
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

/// A data object that serves `paths` as `CF_HDROP` and nothing else.
pub fn hdrop_data(paths: &[PathBuf]) -> Com {
    let obj = Box::new(HdropData {
        header: Header { vtbl: &HDROP_DATA_VTBL as *const DataObjectVtbl as *const c_void, refs: AtomicU32::new(1) },
        block: clipboard::dropfiles_block(paths),
    });
    Com(Box::into_raw(obj) as *mut c_void)
}

// --- The drop target ----------------------------------------------------------------------------

/// Where a drop at a window point goes, as the app decides it (`crate::drop_spot`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DropSpot {
    pub pane: usize,
    /// The row highlighted (-1: the whole pane, the drop goes to the pane's folder).
    pub row: i32,
    pub dest: PathBuf,
}

#[repr(C)]
struct DropTarget {
    header: Header,
    hwnd: isize,
    /// The files of the data object being dragged over us, read at DragEnter (None: it offers
    /// no `CF_HDROP`, so every point refuses it).
    sources: RefCell<Option<Vec<PathBuf>>>,
    /// The last (spot, effect) logged by DragOver, so it logs changes only.
    last: RefCell<Option<(Option<DropSpot>, u32)>>,
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
    /// The spot and effect at screen point `pt` for the cached sources.
    fn evaluate(&self, keys: u32, pt: POINTL, allowed: u32) -> (Option<DropSpot>, u32) {
        let mut p = POINT { x: pt.x, y: pt.y };
        // SAFETY: plain Win32 call on our own window with a valid in/out pointer.
        unsafe { ScreenToClient(self.hwnd as HWND, &mut p) };
        let spot = crate::drop_spot(p.x, p.y);
        let sources = self.sources.borrow();
        let effect = match (&*sources, &spot) {
            (Some(s), Some(spot)) => effect_for(s, &spot.dest, keys, allowed),
            _ => NONE,
        };
        (spot, effect)
    }

    fn over(&self, keys: u32, pt: POINTL, effect: *mut u32) {
        // SAFETY: COM contract: `effect` is a valid in/out pointer (checked for null).
        let allowed = if effect.is_null() { NONE } else { unsafe { *effect } };
        let (spot, e) = self.evaluate(keys, pt, allowed);
        let highlight = spot.as_ref().filter(|_| e != NONE).map(|s| (s.pane, s.row));
        crate::drop_highlight(highlight);
        let changed = self.last.borrow().as_ref() != Some(&(spot.clone(), e));
        if changed {
            eprintln!("[dnd] over {} keys=0x{keys:x} allowed=0x{allowed:x} -> effect {}", describe(&spot), effect_name(e));
            *self.last.borrow_mut() = Some((spot, e));
        }
        if !effect.is_null() {
            // SAFETY: as above.
            unsafe { *effect = e };
        }
    }
}

fn describe(spot: &Option<DropSpot>) -> String {
    match spot {
        None => "nowhere".into(),
        Some(s) if s.row >= 0 => format!("pane {} row {} ({})", s.pane, s.row, s.dest.display()),
        Some(s) => format!("pane {} ({})", s.pane, s.dest.display()),
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
    let sources = read_hdrop(data);
    eprintln!(
        "[dnd] DragEnter: {}",
        match &sources {
            Some(s) => format!("{} file(s), first {}", s.len(), s.first().map(|p| p.display().to_string()).unwrap_or_default()),
            None => "no CF_HDROP, refused".into(),
        }
    );
    *t.sources.borrow_mut() = sources;
    *t.last.borrow_mut() = None;
    t.over(keys, pt, effect);
    S_OK
}

unsafe extern "system" fn dt_drag_over(this: *mut c_void, keys: u32, pt: POINTL, effect: *mut u32) -> HRESULT {
    // SAFETY: as in `dt_drag_enter`.
    let t = unsafe { &*(this as *const DropTarget) };
    t.over(keys, pt, effect);
    S_OK
}

unsafe extern "system" fn dt_drag_leave(this: *mut c_void) -> HRESULT {
    // SAFETY: as in `dt_drag_enter`.
    let t = unsafe { &*(this as *const DropTarget) };
    *t.sources.borrow_mut() = None;
    *t.last.borrow_mut() = None;
    crate::drop_highlight(None);
    eprintln!("[dnd] DragLeave");
    S_OK
}

unsafe extern "system" fn dt_drop(this: *mut c_void, data: *mut c_void, keys: u32, pt: POINTL, effect: *mut u32) -> HRESULT {
    // SAFETY: as in `dt_drag_enter`.
    let t = unsafe { &*(this as *const DropTarget) };
    // Read the files again from the data object OLE hands to Drop: the authoritative list.
    let fresh = read_hdrop(data);
    *t.sources.borrow_mut() = fresh.clone();
    // SAFETY: COM contract: a valid in/out pointer (checked for null).
    let allowed = if effect.is_null() { NONE } else { unsafe { *effect } };
    let (spot, e) = t.evaluate(keys, pt, allowed);
    *t.sources.borrow_mut() = None;
    *t.last.borrow_mut() = None;
    crate::drop_highlight(None);
    let reported = reported_effect(e);
    eprintln!(
        "[dnd] Drop at {} keys=0x{keys:x} allowed=0x{allowed:x}: {} file(s), effect {} (reported to the source: {})",
        describe(&spot),
        fresh.as_ref().map_or(0, Vec::len),
        effect_name(e),
        effect_name(reported)
    );
    if let (Some(sources), Some(spot)) = (fresh, spot) {
        if e != NONE {
            // The job (and every app-state change) runs from the event loop: after our own
            // DoDragDrop has returned, or at the next loop turn for another program's drag.
            let _ = slint::invoke_from_event_loop(move || crate::drop_files(spot.pane, sources, spot.dest, e));
            set_dword(data, CFSTR_PERFORMEDDROPEFFECT, reported);
            set_dword(data, CFSTR_LOGICALPERFORMEDDROPEFFECT, e);
        }
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
}

/// How many drags reached `DoDragDrop` so far.
pub fn drags_started() -> u64 {
    DRAGS.with(Cell::get)
}

/// Replace winit's drop target with ours on `hwnd`. Returns (RevokeDragDrop, RegisterDragDrop)
/// results. Call once, on the UI thread, when the window exists.
pub fn register(hwnd: isize) -> (HRESULT, HRESULT) {
    let obj = Box::new(DropTarget {
        header: Header { vtbl: &DROP_TARGET_VTBL as *const DropTargetVtbl as *const c_void, refs: AtomicU32::new(1) },
        hwnd,
        sources: RefCell::new(None),
        last: RefCell::new(None),
    });
    let ptr = Box::into_raw(obj) as *mut c_void;
    // SAFETY: plain OLE calls on our own window, on its thread. RegisterDragDrop takes its own
    // reference; ours stays in TARGET for the life of the thread.
    let (revoked, registered) = unsafe { (RevokeDragDrop(hwnd as HWND), RegisterDragDrop(hwnd as HWND, ptr)) };
    if registered == S_OK {
        TARGET.with(|t| t.set(ptr));
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
    let t = TARGET.with(Cell::get);
    if t.is_null() {
        return None;
    }
    let data = hdrop_data(paths);
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

// --- The drag source ----------------------------------------------------------------------------

#[repr(C)]
struct DropSource {
    header: Header,
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

unsafe extern "system" fn ds_query_continue_drag(_this: *mut c_void, escape: BOOL, keys: u32) -> HRESULT {
    if !FIRST_QCD_SEEN.with(|c| c.replace(true)) {
        // The re-entrancy check: inside DoDragDrop's loop no borrow of the app state is held.
        eprintln!("[dnd] first QueryContinueDrag inside the drag loop: app state free={}", crate::state_is_free());
    }
    // A scripted drag holds the left button until Escape cancels it: it never drops (see
    // `script_begin`).
    let scripted = SCRIPTED.load(Ordering::Acquire);
    let lbutton = scripted || keys & MK_LBUTTON != 0;
    let hr = continue_drag(escape != 0, lbutton, keys & MK_RBUTTON != 0);
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

unsafe extern "system" fn ds_give_feedback(_: *mut c_void, _: u32) -> HRESULT {
    DRAGDROP_S_USEDEFAULTCURSORS
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
}

/// Drag the named items: a shell data object for them, our drop source, and `DoDragDrop`, which
/// returns when the drop or cancel is done. True when something was dropped (whatever the
/// effect: an optimized move reports none). The effect, and the "Logical Performed DropEffect"
/// the target may have set, go to the log. **Call with no app-state borrow held** (module docs).
pub fn drag(req: &DragRequest) -> Result<bool, String> {
    shellmenu::ensure_com()?;
    let hwnd = req.hwnd as HWND;
    let (folder_pidl, _folder, children) = shellmenu::parse_items(hwnd, &req.folder, &req.names)?;
    let apidl: Vec<*const _> = children.iter().map(|c| c.0 as *const _).collect();
    let mut p: *mut c_void = std::ptr::null_mut();
    // SAFETY: an absolute folder PIDL and child PIDLs relative to it, alive for the call.
    let hr = unsafe { SHCreateDataObject(folder_pidl.0, apidl.len() as u32, apidl.as_ptr(), std::ptr::null_mut(), &IID_IDATAOBJECT, &mut p) };
    if hr < 0 || p.is_null() {
        return Err(format!("SHCreateDataObject failed: 0x{:08X}", hr as u32));
    }
    let data = Com(p);
    let source = Com(Box::into_raw(Box::new(DropSource {
        header: Header { vtbl: &DROP_SOURCE_VTBL as *const DropSourceVtbl as *const c_void, refs: AtomicU32::new(1) },
    })) as *mut c_void);
    let mut effect = NONE;
    DRAGS.with(|d| d.set(d.get() + 1));
    FIRST_QCD_SEEN.with(|c| c.set(false));
    eprintln!("[dnd] DoDragDrop starts: {} item(s) from {}", req.names.len(), req.folder.display());
    let t0 = std::time::Instant::now();
    // MODAL LOOP. No RefCell borrow of app state is held here (the caller dropped its borrow).
    // OLE runs the drag until the button is released or Escape is pressed; our own drop target
    // is called from inside this loop when the cursor is over our window.
    // SAFETY: live IDataObject and IDropSource; `effect` is a valid out-pointer.
    let hr = unsafe { DoDragDrop(data.0, source.0, COPY | MOVE, &mut effect) };
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
    Ok(hr == DRAGDROP_S_DROP)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn query_continue_drag_states() {
        // (escape, lbutton, rbutton)
        assert_eq!(continue_drag(false, true, false), S_OK, "button held: keep dragging");
        assert_eq!(continue_drag(false, false, false), DRAGDROP_S_DROP, "left released: drop");
        assert_eq!(continue_drag(true, true, false), DRAGDROP_S_CANCEL, "Escape cancels");
        assert_eq!(continue_drag(true, false, false), DRAGDROP_S_CANCEL, "Escape wins over a release");
        assert_eq!(continue_drag(false, true, true), DRAGDROP_S_CANCEL, "right button cancels");
        assert_eq!(continue_drag(false, false, true), DRAGDROP_S_CANCEL);
        assert_eq!(continue_drag(true, false, true), DRAGDROP_S_CANCEL);
    }

    #[test]
    fn volumes() {
        assert_eq!(volume_key(&p(r"C:\a\b.txt")), "c:");
        assert_eq!(volume_key(&p(r"c:\")), "c:");
        assert_eq!(volume_key(&p(r"D:\x")), "d:");
        assert_eq!(volume_key(&p(r"\\?\C:\long\path")), "c:");
        assert_eq!(volume_key(&p(r"\\Server\Share\dir\f")), r"\\server\share");
        assert_eq!(volume_key(&p(r"\\?\UNC\server\share\f")), r"\\server\share");
        assert_ne!(volume_key(&p(r"\\server\a\f")), volume_key(&p(r"\\server\b\f")));
    }

    #[test]
    fn effect_decision() {
        let both = COPY | MOVE;
        // Same volume moves, across volumes copies.
        assert_eq!(effect_for(&[p(r"C:\src\a.txt")], &p(r"C:\dst"), 0, both), MOVE);
        assert_eq!(effect_for(&[p(r"C:\src\a.txt")], &p(r"D:\dst"), 0, both), COPY);
        assert_eq!(effect_for(&[p(r"\\s\x\a")], &p(r"\\S\X\dst"), 0, both), MOVE);
        assert_eq!(effect_for(&[p(r"\\s\x\a")], &p(r"C:\dst"), 0, both), COPY);
        // Modifiers.
        assert_eq!(effect_for(&[p(r"C:\src\a")], &p(r"C:\dst"), MK_CONTROL, both), COPY, "Ctrl forces copy");
        assert_eq!(effect_for(&[p(r"C:\src\a")], &p(r"D:\dst"), MK_SHIFT, both), MOVE, "Shift forces move");
        assert_eq!(effect_for(&[p(r"C:\src\a")], &p(r"C:\dst"), MK_CONTROL | MK_SHIFT, both), COPY, "no links: copy");
        assert_eq!(effect_for(&[p(r"C:\src\a")], &p(r"C:\dst"), MK_ALT, both), COPY, "Alt (link in Explorer): copy");
        assert_eq!(effect_for(&[p(r"C:\src\a")], &p(r"C:\dst"), MK_LBUTTON, both), MOVE, "the button bit is no modifier");
        // What the source allows.
        assert_eq!(effect_for(&[p(r"C:\src\a")], &p(r"C:\dst"), 0, COPY), COPY, "move not allowed: copy");
        assert_eq!(effect_for(&[p(r"C:\src\a")], &p(r"D:\dst"), 0, MOVE), MOVE, "copy not allowed: move");
        assert_eq!(effect_for(&[p(r"C:\src\a")], &p(r"C:\dst"), 0, NONE), NONE);
        // Refused drops.
        assert_eq!(effect_for(&[], &p(r"C:\dst"), 0, both), NONE, "no files");
        assert_eq!(effect_for(&[p(r"C:\src\a.txt")], &p(r"C:\src"), MK_CONTROL, both), NONE, "into its own folder");
        assert_eq!(effect_for(&[p(r"C:\src\a.txt")], &p(r"c:\SRC\"), 0, both), NONE, "case and trailing separator");
        assert_eq!(effect_for(&[p(r"C:\src\d")], &p(r"C:\src\d"), 0, both), NONE, "onto a dragged folder");
        assert_eq!(effect_for(&[p(r"C:\src\d")], &p(r"C:\src\d\sub"), 0, both), NONE, "into a dragged folder's subfolder");
        assert_eq!(effect_for(&[p(r"C:\src\d")], &p(r"C:\src\dd"), 0, both), MOVE, "a sibling with a longer name is fine");
        assert_eq!(effect_for(&[p(r"C:\a\x"), p(r"C:\b\y")], &p(r"C:\b"), 0, both), NONE, "any source's own folder");
        // An optimized move reports none to the source.
        assert_eq!(reported_effect(MOVE), NONE);
        assert_eq!(reported_effect(COPY), COPY);
        assert_eq!(reported_effect(NONE), NONE);
    }

    #[test]
    fn hit_testing_and_drop_folder() {
        let one = Layout { width: 1000.0, height: 600.0, split: false, ratio: 0.5, tabs: [false; 2] };
        let two = Layout { split: true, ..one };
        let none = [0.0, 0.0];
        // Rows start at y 63, 22 px each; the list ends 23 px above the bottom (y 577).
        assert_eq!(hit(&one, 500.0, 63.0, none, [10, 0]), Spot::Row(0, 0));
        assert_eq!(hit(&one, 500.0, 84.9, none, [10, 0]), Spot::Row(0, 0));
        assert_eq!(hit(&one, 500.0, 85.0, none, [10, 0]), Spot::Row(0, 1));
        assert_eq!(hit(&one, 500.0, 63.0 + 9.5 * 22.0, none, [10, 0]), Spot::Row(0, 9));
        assert_eq!(hit(&one, 500.0, 63.0 + 10.5 * 22.0, none, [10, 0]), Spot::Pane(0), "below the last row");
        assert_eq!(hit(&one, 500.0, 50.0, none, [10, 0]), Spot::Pane(0), "the header");
        assert_eq!(hit(&one, 500.0, 590.0, none, [1000, 0]), Spot::Pane(0), "the status line");
        assert_eq!(hit(&one, 1.0, 100.0, none, [10, 0]), Spot::Pane(0), "the pane's padding");
        assert_eq!(hit(&one, 500.0, -1.0, none, [10, 0]), Spot::Outside);
        assert_eq!(hit(&one, 1000.0, 100.0, none, [10, 0]), Spot::Outside);
        // Scrolled by 100 rows: the first visible row is row 100.
        assert_eq!(hit(&one, 500.0, 70.0, [-2200.0, 0.0], [1000, 0]), Spot::Row(0, 100));
        assert_eq!(hit(&one, 500.0, 70.0 + 22.0 * 3.0, [-2200.0, 0.0], [1000, 0]), Spot::Row(0, 103));
        // Split at 0.5: left pane to x 497, splitter 497..503, right pane from 503.
        assert_eq!(hit(&two, 100.0, 70.0, none, [5, 7]), Spot::Row(0, 0));
        assert_eq!(hit(&two, 496.0, 70.0, none, [5, 7]), Spot::Pane(0), "the left pane's padding");
        assert_eq!(hit(&two, 497.0, 70.0, none, [5, 7]), Spot::Outside, "the splitter");
        assert_eq!(hit(&two, 502.9, 70.0, none, [5, 7]), Spot::Outside);
        assert_eq!(hit(&two, 503.0, 70.0, none, [5, 7]), Spot::Pane(1), "the right pane's padding");
        assert_eq!(hit(&two, 700.0, 70.0 + 22.0 * 6.0, none, [5, 7]), Spot::Row(1, 6));
        assert_eq!(hit(&two, 700.0, 70.0 + 22.0 * 7.0, none, [5, 7]), Spot::Pane(1));
        assert_eq!(hit(&two, 700.0, 70.0, [-44.0, -22.0], [5, 7]), Spot::Row(1, 1), "each pane its own scroll");
        let ratio = Layout { ratio: 0.25, ..two };
        assert_eq!(pane_x(&ratio, 0), (0.0, 247.0));
        assert_eq!(pane_x(&ratio, 1), (253.0, 1000.0));
        // A tab strip moves that pane's rows down by 28 px; the other pane's stay.
        let tabbed = Layout { tabs: [true, false], ..two };
        assert_eq!(hit(&tabbed, 100.0, 70.0, none, [5, 7]), Spot::Pane(0), "the tab strip pushed row 0 down");
        assert_eq!(hit(&tabbed, 100.0, 91.0, none, [5, 7]), Spot::Row(0, 0));
        assert_eq!(hit(&tabbed, 100.0, 113.0, none, [5, 7]), Spot::Row(0, 1));
        assert_eq!(hit(&tabbed, 700.0, 70.0, none, [5, 7]), Spot::Row(1, 0));
        assert_eq!((list_top(&tabbed, 0), list_top(&tabbed, 1)), (91.0, 63.0));

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
        }
        if com >= 0 {
            // SAFETY: pairs the CoInitializeEx above.
            unsafe { windows_sys::Win32::System::Com::CoUninitialize() };
        }
        std::fs::remove_dir_all(&root).unwrap();
    }
}
