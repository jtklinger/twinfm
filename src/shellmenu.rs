//! The Windows shell context menu (`IContextMenu`) for a pane's selection or its folder
//! background, with TwinFM's own items above the shell's.
//!
//! Everything here runs on the UI thread except `run_verb`, which the verb worker calls
//! (`crate::verbs`, P7-M1): a picked shell command runs there, from a menu built again for the
//! same items, so a verb that waits on a slow file system never holds the UI thread.
//! `IContextMenu` wants the owner window's thread, and
//! `TrackPopupMenuEx` runs a modal message loop on it. winit has already made the UI thread a
//! single-threaded apartment (`OleInitialize`, for drag and drop); `ensure_com` checks that once
//! and never uninitialises. Every COM object made here is created, used and released inside one
//! `show` call.
//!
//! **Modal-loop rule.** `show` must be called with no `RefCell` borrow of app state held (the
//! main `STATE`, a pane model, the driver's `RUNNER`): take the paths and names out, drop the
//! borrow, call `show`, then borrow again to act on the result. While the menu (or an invoked
//! command's dialog) is up, the loop dispatches messages for this thread. With winit 0.30 those
//! do not reach Slint: winit buffers every event that arrives while its handler is running
//! (`EventLoopRunner::should_buffer`), including Slint timers and `invoke_from_event_loop`, and
//! answers `WM_PAINT` without rendering. What does run inside the loop is plain Win32: the
//! window subclass below (forwarding menu messages to the shell), Win32 timer callbacks (the
//! scripted driver's `menuclose`), and whatever the shell's own code does. None of those may
//! touch app state; the driver's in-menu callback checks that the state is free and says so.
//!
//! **IContextMenu2/3.** "Send to", "Open with" and "New" fill their submenus, and draw their
//! icons, in response to `WM_INITMENUPOPUP`, `WM_MEASUREITEM`, `WM_DRAWITEM` and `WM_MENUCHAR`
//! sent to the owner window. A comctl32 subclass (`SetWindowSubclass`) on the main window,
//! installed only while the menu is up, forwards those to `HandleMenuMsg2` (or `HandleMenuMsg`).

use crate::icons::ListGeom;
use std::cell::Cell;
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, POINT, RPC_E_CHANGED_MODE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree};
use windows_sys::Win32::System::Threading::{
    CREATE_NEW_CONSOLE, CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW,
};
use windows_sys::Win32::UI::Shell::Common::ITEMIDLIST;
use windows_sys::Win32::UI::Shell::{
    CMF_EXPLORE, CMF_EXTENDEDVERBS, CMF_NORMAL, CMIC_MASK_PTINVOKE, CMINVOKECOMMANDINFOEX,
    DefSubclassProc, GCS_VERBW, RemoveWindowSubclass, SHBindToObject, SHParseDisplayName, SetWindowSubclass,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, DestroyMenu, GetMenuItemCount, GetMenuItemInfoW, GetMenuStringW, GetSubMenu, HMENU, InsertMenuW,
    MENUITEMINFOW, MF_BYCOMMAND, MF_BYPOSITION, MF_SEPARATOR, MF_STRING, MFT_SEPARATOR, MIIM_FTYPE, MIIM_ID, MIIM_SUBMENU, SW_SHOWNORMAL,
    TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, TPM_TOPALIGN, TrackPopupMenuEx, WM_DRAWITEM, WM_INITMENUPOPUP,
    WM_MEASUREITEM, WM_MENUCHAR,
};
use windows_sys::core::{GUID, HRESULT};

/// `CMIC_MASK_UNICODE` (shobjidl_core.h defines it as `SEE_MASK_UNICODE`); windows-sys 0.60
/// has no constant of that name.
const CMIC_MASK_UNICODE: u32 = windows_sys::Win32::UI::Shell::SEE_MASK_UNICODE;

/// Command ids. TwinFM's own items use 1..=20 and the shell gets 100..=0x7FFF, so the two
/// ranges never meet; 0 is "nothing chosen".
pub const OWN_FIRST: u32 = 1;
pub const OWN_LAST: u32 = 20;
pub const SHELL_FIRST: u32 = 100;
pub const SHELL_LAST: u32 = 0x7FFF;

/// TwinFM's own menu items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnItem {
    /// Navigate the other pane to the directory (turning the split on if needed).
    OpenOther = 1,
    /// Windows Terminal in the folder, or PowerShell when wt.exe cannot be started.
    Terminal = 2,
    /// The full path of each target, one per line, as text on the clipboard.
    CopyPath = 3,
}

impl OwnItem {
    pub const ALL: [OwnItem; 3] = [OwnItem::OpenOther, OwnItem::Terminal, OwnItem::CopyPath];

    pub fn id(self) -> u32 {
        self as u32
    }

    pub fn label(self) -> &'static str {
        match self {
            OwnItem::OpenOther => "Open in other pane",
            OwnItem::Terminal => "Open in terminal",
            OwnItem::CopyPath => "Copy path",
        }
    }

    fn from_id(id: u32) -> Option<OwnItem> {
        OwnItem::ALL.into_iter().find(|o| o.id() == id)
    }
}

/// What a command id returned by `TrackPopupMenuEx` means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// Dismissed, or an id outside both ranges.
    Nothing,
    Own(OwnItem),
    /// A shell command, as its offset from `SHELL_FIRST` (the `lpVerb` of `InvokeCommand`).
    Shell(u32),
}

pub fn classify(cmd: u32) -> Pick {
    match cmd {
        OWN_FIRST..=OWN_LAST => OwnItem::from_id(cmd).map_or(Pick::Nothing, Pick::Own),
        SHELL_FIRST..=SHELL_LAST => Pick::Shell(cmd - SHELL_FIRST),
        _ => Pick::Nothing,
    }
}

/// What happened, for the caller to act on once it holds the state again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Picked {
    Nothing,
    Own(OwnItem),
    /// The shell's "rename" verb: it needs an `IShellView` site to start an in-place edit and
    /// does nothing without one, so TwinFM opens its own name editor instead.
    Rename,
    /// A shell verb TwinFM runs as its own command (`route`): the command's name for
    /// `main::command` ("delete", "cx", "cc", "cv").
    Route(&'static str),
    /// A shell command for the verb worker: its canonical verb (empty when it has none), its
    /// offset from `SHELL_FIRST` and its label in the menu that was shown.
    Shell { verb: String, offset: u32, label: String },
}

pub struct Request {
    /// The owner window (the main window); the menu and any dialog it opens belong to it.
    pub hwnd: isize,
    /// The pane's folder (`This PC` means the drive list).
    pub folder: PathBuf,
    /// The selected names in that folder; empty for the background menu.
    pub names: Vec<String>,
    /// Own items to put above the shell's, in order.
    pub own: Vec<OwnItem>,
    /// Screen position, physical pixels.
    pub x: i32,
    pub y: i32,
    /// Shift was held: ask for the extended verbs ("Copy as path", "Open PowerShell window here").
    pub extended: bool,
}

thread_local! {
    /// The open menu's root HMENU (0 while none is open), for the driver's in-menu steps and
    /// the subclass's submenu log. A Cell: read inside the modal loop without any borrow.
    static CURRENT_MENU: Cell<isize> = const { Cell::new(0) };
    /// The CoInitializeEx result on the UI thread, once checked.
    static COM_INIT: Cell<Option<HRESULT>> = const { Cell::new(None) };
}

/// The open menu's root HMENU, or 0.
pub fn current_menu() -> isize {
    CURRENT_MENU.with(Cell::get)
}

/// The drop target's right-button menu (`dnd::drop_menu`) is up (or closed, 0): the driver's
/// in-menu steps and the second-right-click guard see it as they see the shell menu.
pub(crate) fn set_current_menu(hmenu: isize) {
    CURRENT_MENU.with(|c| c.set(hmenu));
}

/// A shell menu is up on this thread (a second right-click arriving inside the loop is dropped).
pub fn is_open() -> bool {
    current_menu() != 0
}

/// COM on the UI thread: S_FALSE is the expected answer (winit's `OleInitialize` came first).
/// Checked once; never uninitialised, so shell extensions that outlive a menu keep their
/// apartment.
pub(crate) fn ensure_com() -> Result<(), String> {
    let hr = COM_INIT.with(|c| {
        c.get().unwrap_or_else(|| {
            // SAFETY: plain COM initialisation of the calling (UI) thread.
            let hr = unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
            let what = match hr {
                0 => "S_OK (COM was not initialised yet; now an STA for the process lifetime)",
                1 => "S_FALSE (already an STA, winit's OleInitialize)",
                RPC_E_CHANGED_MODE => "RPC_E_CHANGED_MODE (the thread is multithreaded)",
                _ => "failed",
            };
            eprintln!("[menu] UI thread CoInitializeEx(APARTMENTTHREADED) = 0x{:08X}: {what}", hr as u32);
            c.set(Some(hr));
            hr
        })
    });
    if hr == RPC_E_CHANGED_MODE {
        Err("the UI thread is not a single-threaded apartment".into())
    } else if hr < 0 {
        Err(format!("CoInitializeEx failed: 0x{:08X}", hr as u32))
    } else {
        Ok(())
    }
}

/// Where the keyboard menu (Shift+F10, the Menu key) opens for `row`, in window logical px:
/// at the name column's left edge (28 px into the list, 42 with thumbnails on) under the row,
/// kept inside the list's viewport. `list_x`/`list_y` are the list's window position, `content_y`
/// its scroll offset (zero or negative), `visible_h` its viewport height, `g` its row geometry.
pub fn row_menu_point(list_x: f32, list_y: f32, content_y: f32, visible_h: f32, row: usize, g: ListGeom) -> (f32, f32) {
    let bottom = list_y + content_y + (row as f32 + 1.0) * g.row_px;
    (list_x + g.name_x(), bottom.clamp(list_y, list_y + visible_h.max(0.0)))
}

/// `row_menu_point` for the icon grid: under the tile of entry `row` in a grid of `cols` tiles of
/// `tile` to a grid row, 2 px into the tile (the name's left edge), kept inside the viewport.
pub fn tile_menu_point(list_x: f32, list_y: f32, content_y: f32, visible_h: f32, row: usize, cols: usize, tile: crate::icons::TileGeom) -> (f32, f32) {
    let (r, c) = crate::model::grid_cell(row, cols);
    let bottom = list_y + content_y + (r as f32 + 1.0) * tile.h;
    (list_x + c as f32 * tile.w + 2.0, bottom.clamp(list_y, list_y + visible_h.max(0.0)))
}

/// Window logical px to screen physical px.
pub fn to_screen(hwnd: isize, x: f32, y: f32, scale: f32) -> (i32, i32) {
    let mut pt = POINT { x: (x * scale).round() as i32, y: (y * scale).round() as i32 };
    // SAFETY: plain Win32 call with a valid out-pointer; leaves `pt` unchanged on failure.
    unsafe { ClientToScreen(hwnd as HWND, &mut pt) };
    (pt.x, pt.y)
}

/// The clipboard text for "Copy path": one full path per line, CRLF between lines.
pub fn path_text(paths: &[PathBuf]) -> String {
    paths.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>().join("\r\n")
}

/// The command lines "Open in terminal" tries, in order: Windows Terminal (`wt.exe -d`), then
/// PowerShell for a system without it. wt treats `;` as its own command separator, so it is
/// escaped; a trailing backslash is doubled so it does not escape the closing quote. PowerShell
/// gets the folder as a single-quoted literal (quotes doubled; `$` and backticks stay literal).
pub fn terminal_commands(folder: &str) -> [String; 2] {
    let mut wt_dir = folder.replace(';', "\\;");
    if wt_dir.ends_with('\\') {
        wt_dir.push('\\');
    }
    [
        format!("wt.exe -d \"{wt_dir}\""),
        format!("powershell.exe -NoExit -Command \"Set-Location -LiteralPath '{}'\"", folder.replace('\'', "''")),
    ]
}

/// Start the first command of `terminal_commands` that launches, in its own console, with the
/// folder as the working directory and none of our handles inherited (stderr may be a pipe).
/// Returns the command line that worked.
pub fn open_terminal(folder: &Path) -> Result<String, String> {
    let dir = wide(folder.as_os_str());
    let mut last = 0;
    for cmd in terminal_commands(&folder.to_string_lossy()) {
        let mut line: Vec<u16> = cmd.encode_utf16().chain([0]).collect();
        // SAFETY: zeroed STARTUPINFOW/PROCESS_INFORMATION are valid inputs/outputs; `line` is a
        // mutable NUL-terminated buffer as CreateProcessW requires, `dir` is NUL-terminated.
        unsafe {
            let mut si: STARTUPINFOW = std::mem::zeroed();
            si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
            let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
            let ok = CreateProcessW(
                std::ptr::null(),
                line.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                CREATE_NEW_CONSOLE,
                std::ptr::null(),
                dir.as_ptr(),
                &si,
                &mut pi,
            );
            if ok != 0 {
                CloseHandle(pi.hThread);
                CloseHandle(pi.hProcess);
                return Ok(cmd);
            }
            last = windows_sys::Win32::Foundation::GetLastError();
        }
        eprintln!("[menu] terminal: {cmd} did not start (error {last})");
    }
    Err(format!("no terminal could be started (error {last})"))
}

/// Open a file the way Explorer's double-click does: `ShellExecuteExW` with no verb (the
/// default one) and `SEE_MASK_INVOKEIDLIST`, so the shell's own association and context-menu
/// handlers decide; `SEE_MASK_NOASYNC` so the call does not return before a DDE conversation the
/// handler starts has finished. On the UI thread, with no app-state borrow held: the shell can
/// show its "Open with" dialog from inside the call. Err is the Win32 error code;
/// `ERROR_CANCELLED` (that dialog dismissed) counts as Ok.
pub fn open_item(hwnd: isize, file: &Path, dir: &Path) -> Result<(), u32> {
    use windows_sys::Win32::UI::Shell::{SEE_MASK_INVOKEIDLIST, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW};
    let (file_w, dir_w) = (wide(file.as_os_str()), wide(dir.as_os_str()));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC | SEE_MASK_INVOKEIDLIST,
        hwnd: hwnd as HWND,
        lpFile: file_w.as_ptr(),
        lpDirectory: dir_w.as_ptr(),
        nShow: windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        ..Default::default()
    };
    // SAFETY: `info` is a sized, zero-initialised SHELLEXECUTEINFOW whose strings are
    // NUL-terminated buffers that outlive the call; no process handle is requested.
    let ok = unsafe { ShellExecuteExW(&mut info) };
    if ok != 0 {
        return Ok(());
    }
    // SAFETY: plain Win32 query.
    match unsafe { windows_sys::Win32::Foundation::GetLastError() } {
        windows_sys::Win32::Foundation::ERROR_CANCELLED => Ok(()),
        e => Err(e),
    }
}

/// Open a web or mail link from the Markdown preview (P8-M2) in the default browser or mail
/// program: `ShellExecuteExW` with the `open` verb and without `SEE_MASK_INVOKEIDLIST`, so the
/// URL goes to its protocol's handler and no shell item is made of it. The caller passes only
/// `http`, `https` and `mailto` links. On the UI thread with no app-state borrow held, as
/// `open_item`. Err is the Win32 error code.
pub fn open_url(hwnd: isize, url: &str) -> Result<(), u32> {
    use windows_sys::Win32::UI::Shell::{SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW};
    let (verb, url_w) = (wide(std::ffi::OsStr::new("open")), wide(std::ffi::OsStr::new(url)));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC,
        hwnd: hwnd as HWND,
        lpVerb: verb.as_ptr(),
        lpFile: url_w.as_ptr(),
        nShow: windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        ..Default::default()
    };
    // SAFETY: as in `open_item`: a zero-initialised SHELLEXECUTEINFOW whose strings outlive the call.
    if unsafe { ShellExecuteExW(&mut info) } != 0 {
        return Ok(());
    }
    // SAFETY: plain Win32 query.
    Err(unsafe { windows_sys::Win32::Foundation::GetLastError() })
}

/// Open an item inside a zip archive as Explorer does: `ShellExecuteExW` with the item's PIDL
/// (`SEE_MASK_IDLIST`), so the shell's zip folder extracts it to a temporary folder and opens it
/// there. On the UI thread with no app-state borrow held, as `open_item`. Err is the HRESULT of a
/// failed parse or the Win32 error code of a failed open.
pub fn open_in_archive(hwnd: isize, path: &Path) -> Result<(), u32> {
    use windows_sys::Win32::UI::Shell::{SEE_MASK_IDLIST, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW};
    let w = wide(path.as_os_str());
    let mut raw: *mut ITEMIDLIST = std::ptr::null_mut();
    // SAFETY: NUL-terminated name; `raw` receives a PIDL we own or stays null.
    let hr = unsafe { SHParseDisplayName(w.as_ptr(), std::ptr::null_mut(), &mut raw, 0, std::ptr::null_mut()) };
    if hr < 0 || raw.is_null() {
        eprintln!("[archive] parse \"{}\" -> item failed 0x{:08X}", path.display(), hr as u32);
        return Err(hr as u32);
    }
    eprintln!("[archive] parse \"{}\" -> item ok", path.display());
    let pidl = Pidl(raw);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC | SEE_MASK_IDLIST,
        hwnd: hwnd as HWND,
        lpIDList: pidl.0 as *mut c_void,
        nShow: SW_SHOWNORMAL,
        ..Default::default()
    };
    // SAFETY: a sized, zero-initialised SHELLEXECUTEINFOW whose PIDL outlives the call.
    if unsafe { ShellExecuteExW(&mut info) } != 0 {
        return Ok(());
    }
    // SAFETY: plain Win32 query.
    match unsafe { windows_sys::Win32::Foundation::GetLastError() } {
        windows_sys::Win32::Foundation::ERROR_CANCELLED => Ok(()),
        e => Err(e),
    }
}

/// The folder's own data object for its children (`GetUIObjectOf(IID_IDataObject)`). For a folder
/// inside a zip archive it serves `FileGroupDescriptorW` and `FileContents`, from which Explorer
/// and other targets extract.
pub(crate) fn ui_data_object(hwnd: HWND, folder: &Com, children: &[Pidl]) -> Result<Com, String> {
    const IID_IDATAOBJECT: GUID = GUID::from_u128(0x0000010e_0000_0000_c000_000000000046);
    let apidl: Vec<*const ITEMIDLIST> = children.iter().map(|c| c.0 as *const ITEMIDLIST).collect();
    let mut p: *mut c_void = std::ptr::null_mut();
    // SAFETY: `folder` is an IShellFolder; child PIDLs relative to it, alive for the call.
    let hr = unsafe {
        (folder.vtbl::<ShellFolderVtbl>().get_ui_object_of)(folder.0, hwnd, apidl.len() as u32, apidl.as_ptr(), &IID_IDATAOBJECT, std::ptr::null_mut(), &mut p)
    };
    check(hr, "GetUIObjectOf(IDataObject)")?;
    if p.is_null() {
        return Err("the shell returned no data object".into());
    }
    Ok(Com(p))
}

pub(crate) fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain([0]).collect()
}

const IID_ISHELLFOLDER: GUID = GUID::from_u128(0x000214e6_0000_0000_c000_000000000046);
const IID_ICONTEXTMENU: GUID = GUID::from_u128(0x000214e4_0000_0000_c000_000000000046);
const IID_ICONTEXTMENU2: GUID = GUID::from_u128(0x000214f4_0000_0000_c000_000000000046);
const IID_ICONTEXTMENU3: GUID = GUID::from_u128(0xbcfce0a0_ec17_11d0_8d10_00a0c90f2719);
/// The This PC folder, for the drive list's menus.
const THIS_PC_PARSING_NAME: &str = "::{20D04FE0-3AEA-1069-A2D8-08002B30309D}";
/// Our subclass id on the main window.
const SUBCLASS_ID: usize = 0x7477_666d; // "twfm"

type Slot = *const c_void;

#[repr(C)]
struct UnknownVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    _add_ref: Slot,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
}

/// IShellFolder in declaration order (shobjidl_core.h); unused slots are opaque.
#[repr(C)]
struct ShellFolderVtbl {
    base: UnknownVtbl,
    parse_display_name: unsafe extern "system" fn(
        *mut c_void,
        HWND,
        *mut c_void,
        *const u16,
        *mut u32,
        *mut *mut ITEMIDLIST,
        *mut u32,
    ) -> HRESULT,
    _enum_objects: Slot,
    _bind_to_object: Slot,
    _bind_to_storage: Slot,
    _compare_ids: Slot,
    create_view_object: unsafe extern "system" fn(*mut c_void, HWND, *const GUID, *mut *mut c_void) -> HRESULT,
    _get_attributes_of: Slot,
    get_ui_object_of: unsafe extern "system" fn(
        *mut c_void,
        HWND,
        u32,
        *const *const ITEMIDLIST,
        *const GUID,
        *mut u32,
        *mut *mut c_void,
    ) -> HRESULT,
}

/// IContextMenu, then the methods IContextMenu2 and IContextMenu3 append.
#[repr(C)]
struct ContextMenuVtbl {
    base: UnknownVtbl,
    query_context_menu: unsafe extern "system" fn(*mut c_void, HMENU, u32, u32, u32, u32) -> HRESULT,
    invoke_command: unsafe extern "system" fn(*mut c_void, *const CMINVOKECOMMANDINFOEX) -> HRESULT,
    get_command_string: unsafe extern "system" fn(*mut c_void, usize, u32, *mut u32, *mut u8, u32) -> HRESULT,
}

#[repr(C)]
struct ContextMenu2Vtbl {
    cm: ContextMenuVtbl,
    handle_menu_msg: unsafe extern "system" fn(*mut c_void, u32, WPARAM, LPARAM) -> HRESULT,
}

#[repr(C)]
struct ContextMenu3Vtbl {
    cm2: ContextMenu2Vtbl,
    handle_menu_msg2: unsafe extern "system" fn(*mut c_void, u32, WPARAM, LPARAM, *mut LRESULT) -> HRESULT,
}

/// An owned COM interface pointer, released on drop.
pub(crate) struct Com(pub(crate) *mut c_void);

impl Com {
    /// The object's vtable, viewed as `T` (the caller picks the interface it asked for).
    pub(crate) unsafe fn vtbl<T>(&self) -> &T {
        // SAFETY: the caller guarantees the pointer is that interface.
        unsafe { &**(self.0 as *const *const T) }
    }

    pub(crate) fn query(&self, iid: &GUID) -> Option<Com> {
        let mut p: *mut c_void = std::ptr::null_mut();
        // SAFETY: a live interface; every COM vtable starts with IUnknown.
        let hr = unsafe { (self.vtbl::<UnknownVtbl>().query_interface)(self.0, iid, &mut p) };
        (hr >= 0 && !p.is_null()).then_some(Com(p))
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        // SAFETY: a live interface pointer we own one reference to.
        unsafe { (self.vtbl::<UnknownVtbl>().release)(self.0) };
    }
}

/// A PIDL the shell allocated for us, freed with CoTaskMemFree.
pub(crate) struct Pidl(pub(crate) *mut ITEMIDLIST);

impl Drop for Pidl {
    fn drop(&mut self) {
        // SAFETY: allocated by the shell with the COM task allocator, owned by us.
        unsafe { CoTaskMemFree(self.0 as *const c_void) };
    }
}

/// A popup menu, destroyed on drop.
pub(crate) struct Menu(pub(crate) HMENU);

impl Menu {
    fn new() -> Result<Menu, String> {
        // SAFETY: a new empty popup menu, destroyed by `Menu`'s drop.
        let m = unsafe { CreatePopupMenu() };
        if m.is_null() { Err("CreatePopupMenu failed".into()) } else { Ok(Menu(m)) }
    }
}

impl Drop for Menu {
    fn drop(&mut self) {
        // SAFETY: a menu we created and nobody else destroys.
        unsafe { DestroyMenu(self.0) };
    }
}

/// The subclass's reference data: the menu interface that takes the forwarded messages.
struct Routing {
    ptr: *mut c_void,
    /// IContextMenu3 (HandleMenuMsg2, also takes WM_MENUCHAR) rather than IContextMenu2.
    v3: bool,
}

/// While installed, forwards the owner-draw and popup-init messages to the shell menu.
/// Runs inside the menu's modal loop: COM and logging only, no app state.
unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    if matches!(msg, WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR) && data != 0 {
        // SAFETY: `data` is the Routing that `show` keeps alive until after RemoveWindowSubclass.
        let r = unsafe { &*(data as *const Routing) };
        let handled = if r.v3 {
            let mut res: LRESULT = 0;
            // SAFETY: `r.ptr` is a live IContextMenu3 owned by `show`.
            let hr = unsafe { ((*(*(r.ptr as *const *const ContextMenu3Vtbl))).handle_menu_msg2)(r.ptr, msg, wp, lp, &mut res) };
            (hr >= 0).then_some(res)
        } else if msg != WM_MENUCHAR {
            // SAFETY: `r.ptr` is a live IContextMenu2 owned by `show`.
            let hr = unsafe { ((*(*(r.ptr as *const *const ContextMenu2Vtbl))).handle_menu_msg)(r.ptr, msg, wp, lp) };
            (hr >= 0).then_some(0)
        } else {
            None
        };
        if msg == WM_INITMENUPOPUP {
            log_popup(wp as HMENU, handled.is_some());
        }
        if let Some(res) = handled {
            return res;
        }
    }
    // SAFETY: pass everything else down the subclass chain (winit's window procedure).
    unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
}

/// The items of a menu as text: "---" for a separator, " >" after a submenu's name.
pub(crate) fn menu_items(hmenu: HMENU) -> Vec<String> {
    // SAFETY: read-only queries on a live menu with valid buffers.
    unsafe {
        let n = GetMenuItemCount(hmenu).max(0) as u32;
        (0..n)
            .map(|i| {
                let mut mii: MENUITEMINFOW = std::mem::zeroed();
                mii.cbSize = std::mem::size_of::<MENUITEMINFOW>() as u32;
                mii.fMask = MIIM_FTYPE | MIIM_SUBMENU;
                GetMenuItemInfoW(hmenu, i, 1, &mut mii);
                if mii.fType & MFT_SEPARATOR != 0 {
                    return "---".to_string();
                }
                let mut buf = [0u16; 128];
                let len = GetMenuStringW(hmenu, i, buf.as_mut_ptr(), buf.len() as i32, MF_BYPOSITION).max(0) as usize;
                let text = String::from_utf16_lossy(&buf[..len]).replace('&', "");
                let text = if text.is_empty() { "(owner-drawn)".to_string() } else { text };
                if mii.hSubMenu.is_null() { text } else { format!("{text} >") }
            })
            .collect()
    }
}

/// The items of the menu `hmenu` (an HMENU as an integer), as `menu_items` reads them.
pub fn item_labels(hmenu: isize) -> Vec<String> {
    if hmenu == 0 { Vec::new() } else { menu_items(hmenu as HMENU) }
}

/// Which item of the open root menu owns `sub`, by its text.
fn submenu_name(sub: HMENU) -> Option<String> {
    let root = current_menu() as HMENU;
    if root.is_null() {
        return None;
    }
    if root == sub {
        return Some("(root)".into());
    }
    // SAFETY: read-only queries on the live root menu.
    let n = unsafe { GetMenuItemCount(root) }.max(0);
    (0..n).find(|&i| unsafe { GetSubMenu(root, i) } == sub).map(|i| {
        let mut buf = [0u16; 128];
        // SAFETY: as above.
        let len = unsafe { GetMenuStringW(root, i as u32, buf.as_mut_ptr(), buf.len() as i32, MF_BYPOSITION) }.max(0);
        String::from_utf16_lossy(&buf[..len as usize]).replace('&', "")
    })
}

/// A submenu just opened: log its name and what it holds after the shell filled it.
fn log_popup(sub: HMENU, handled: bool) {
    let name = submenu_name(sub).unwrap_or_else(|| "(nested)".into());
    let items = menu_items(sub);
    let shown: Vec<&str> = items.iter().take(12).map(String::as_str).collect();
    eprintln!(
        "[menu] popup \"{name}\" after HandleMenuMsg{}: {} items: {}",
        if handled { "" } else { " (not handled)" },
        items.len(),
        shown.join(" | ")
    );
}

pub(crate) fn check(hr: HRESULT, what: &str) -> Result<(), String> {
    if hr < 0 { Err(format!("{what} failed: 0x{:08X}", hr as u32)) } else { Ok(()) }
}

/// `names` relative to `folder`, which may hold a `\` (a search tab's results, `sub\file.txt`), as
/// the one folder they are in and their own names. Err when they are in different folders.
pub(crate) fn common_parent(folder: &Path, names: &[String]) -> Result<(PathBuf, Vec<String>), String> {
    // A drive on This PC is named by its root (`C:\`): a name, not a path to split.
    fn split(n: &str) -> (&str, &str) {
        if n.ends_with('\\') { ("", n) } else { n.rsplit_once('\\').unwrap_or(("", n)) }
    }
    let parent = names.first().map_or("", |n| split(n).0);
    if names.iter().any(|n| !split(n).0.eq_ignore_ascii_case(parent)) {
        return Err("the items are in different folders; select items from one folder".into());
    }
    let dir = if parent.is_empty() { folder.to_path_buf() } else { folder.join(parent) };
    Ok((dir, names.iter().map(|n| split(n).1.to_string()).collect()))
}

/// The folder's absolute PIDL, its `IShellFolder`, and one child PIDL per name, as
/// `GetUIObjectOf` and `SHCreateDataObject` take them. `This PC` means the drive list (This PC's
/// shell folder, so drives get their own menus and data objects). Drop order: declare the result
/// in this order so the children go first, then the folder, then its PIDL.
pub(crate) fn parse_items(hwnd: HWND, folder: &Path, names: &[String]) -> Result<(Pidl, Com, Vec<Pidl>), String> {
    // A search tab's names are paths relative to its root: the items' own folder and their own
    // names, since a child PIDL must be one level deep.
    let (folder, names) = &common_parent(folder, names)?;
    let folder_name = if crate::fs::enumerate::is_this_pc(&folder.to_string_lossy()) {
        wide(std::ffi::OsStr::new(THIS_PC_PARSING_NAME))
    } else {
        wide(folder.as_os_str())
    };
    let mut raw: *mut ITEMIDLIST = std::ptr::null_mut();
    // SAFETY: NUL-terminated name; `raw` receives a PIDL we own or stays null.
    check(unsafe { SHParseDisplayName(folder_name.as_ptr(), std::ptr::null_mut(), &mut raw, 0, std::ptr::null_mut()) }, "SHParseDisplayName")?;
    let folder_pidl = Pidl(raw);
    let mut p: *mut c_void = std::ptr::null_mut();
    // SAFETY: binds the absolute PIDL from the desktop to an IShellFolder.
    check(unsafe { SHBindToObject(std::ptr::null_mut(), folder_pidl.0, std::ptr::null_mut(), &IID_ISHELLFOLDER, &mut p) }, "SHBindToObject")?;
    let shell_folder = Com(p);
    // SAFETY: `shell_folder` is an IShellFolder.
    let fv = unsafe { shell_folder.vtbl::<ShellFolderVtbl>() };
    let mut children: Vec<Pidl> = Vec::with_capacity(names.len());
    for name in names {
        let w = wide(std::ffi::OsStr::new(name));
        let mut child: *mut ITEMIDLIST = std::ptr::null_mut();
        // SAFETY: parses a child name relative to the folder; `child` is ours on success.
        let hr = unsafe {
            (fv.parse_display_name)(shell_folder.0, hwnd, std::ptr::null_mut(), w.as_ptr(), std::ptr::null_mut(), &mut child, std::ptr::null_mut())
        };
        check(hr, &format!("parsing \"{name}\""))?;
        children.push(Pidl(child));
    }
    Ok((folder_pidl, shell_folder, children))
}

/// Build, show and run the menu. Blocks in `TrackPopupMenuEx` (and in `InvokeCommand`) until the
/// user is done. **Call with no app-state borrow held** (module docs).
pub fn show(req: &Request) -> Result<Picked, String> {
    ensure_com()?;
    let hwnd = req.hwnd as HWND;
    // Objects are declared in creation order so they drop in reverse: the menu interfaces
    // first, then the HMENU, the child PIDLs, the folder and its PIDL.
    let (_folder_pidl, folder, children) = parse_items(hwnd, &req.folder, &req.names)?;
    let cm = context_menu_of(hwnd, &folder, &children)?;
    // SAFETY: `cm` is an IContextMenu.
    let cv = unsafe { cm.vtbl::<ContextMenuVtbl>() };
    let menu = Menu::new()?;
    for (i, item) in req.own.iter().enumerate() {
        let label = wide(std::ffi::OsStr::new(item.label()));
        // SAFETY: inserting a string item into our own menu.
        unsafe { InsertMenuW(menu.0, i as u32, MF_BYPOSITION | MF_STRING, item.id() as usize, label.as_ptr()) };
    }
    if !req.own.is_empty() {
        // SAFETY: as above.
        unsafe { InsertMenuW(menu.0, req.own.len() as u32, MF_BYPOSITION | MF_SEPARATOR, 0, std::ptr::null()) };
    }
    // The shell's items go after ours (indexMenu = our count), so ours stay on top.
    // SAFETY: a live menu and IContextMenu.
    let base = unsafe { GetMenuItemCount(menu.0) }.max(0) as u32;
    let flags = CMF_NORMAL | CMF_EXPLORE | if req.extended { CMF_EXTENDEDVERBS } else { 0 };
    check(unsafe { (cv.query_context_menu)(cm.0, menu.0, base, SHELL_FIRST, SHELL_LAST, flags) }, "QueryContextMenu")?;
    eprintln!(
        "[menu] {} for {} in {}: {}",
        if req.names.is_empty() { "background menu" } else { "item menu" },
        if req.names.is_empty() { "the folder".to_string() } else { format!("{} item(s)", req.names.len()) },
        req.folder.display(),
        menu_items(menu.0).join(" | ")
    );

    // The zip folder's menus (items and background) crash inside their own QueryInterface for
    // IContextMenu2 and IContextMenu3 (P3-M4); they have no owner-drawn submenus to route anyway.
    let (routed, v3) = if crate::fs::archive::inside(&req.folder) {
        eprintln!("[menu] inside an archive: no IContextMenu2/3 routing");
        (None, false)
    } else {
        match cm.query(&IID_ICONTEXTMENU3) {
            Some(c) => (Some(c), true),
            None => (cm.query(&IID_ICONTEXTMENU2), false),
        }
    };
    let routing = routed.as_ref().map(|c| Box::new(Routing { ptr: c.0, v3 }));
    if let Some(r) = &routing {
        let data = &**r as *const Routing as usize;
        // SAFETY: subclassing our own window on its thread; removed below before `routing`
        // (the refdata) and the menu interface are released.
        unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, data) };
    }
    CURRENT_MENU.with(|c| c.set(menu.0 as isize));
    eprintln!("[menu] TrackPopupMenuEx starts");
    let cpu0 = crate::perf::thread_cpu_secs();
    let t0 = std::time::Instant::now();
    // MODAL LOOP. No RefCell borrow of app state is held here (the caller dropped its borrow
    // before calling `show`). TrackPopupMenuEx dispatches this thread's messages until the
    // menu closes; only Win32-level code (the subclass, Win32 timers, shell code) runs inside.
    // SAFETY: a live menu owned by `hwnd`'s thread (this thread).
    let cmd = unsafe {
        TrackPopupMenuEx(menu.0, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_LEFTALIGN | TPM_TOPALIGN, req.x, req.y, hwnd, std::ptr::null())
    } as u32;
    let cpu_ms = (crate::perf::thread_cpu_secs() - cpu0) * 1e3;
    CURRENT_MENU.with(|c| c.set(0));
    if routing.is_some() {
        // SAFETY: removes the subclass installed above.
        unsafe { RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID) };
    }
    drop(routing);
    drop(routed);
    let pick = classify(cmd);
    eprintln!(
        "[menu] closed after {:.0} ms (UI thread cpu {cpu_ms:.0} ms in the loop): id {cmd} = {pick:?}",
        crate::perf::elapsed_ms(t0)
    );

    match pick {
        Pick::Nothing => Ok(Picked::Nothing),
        Pick::Own(o) => Ok(Picked::Own(o)),
        Pick::Shell(offset) => {
            // The verb and the label are read here, from the menu that was shown; the verb itself
            // runs on the verb worker (`crate::verbs`), which builds the menu again (P7-M1).
            let verb = verb_of(&cm, offset);
            let label = command_label(menu.0, SHELL_FIRST + offset);
            if let Some(cmd) = route(&verb) {
                eprintln!("[menu] shell verb \"{verb}\" routed to TwinFM's {cmd}");
                return Ok(if cmd == "rename" { Picked::Rename } else { Picked::Route(cmd) });
            }
            Ok(Picked::Shell { verb, offset, label })
        }
    }
}

/// The shell verbs TwinFM runs as its own commands (P7-M1), by canonical verb: the command the
/// key and the toolbar button call (`main::command`). Every other verb goes to the verb worker.
pub fn route(verb: &str) -> Option<&'static str> {
    match verb.to_ascii_lowercase().as_str() {
        "delete" => Some("delete"),
        "cut" => Some("cx"),
        "copy" => Some("cc"),
        "paste" => Some("cv"),
        "rename" => Some("rename"),
        _ => None,
    }
}

/// The context menu for `children` of `folder`, or the folder's background menu when there are
/// none.
fn context_menu_of(hwnd: HWND, folder: &Com, children: &[Pidl]) -> Result<Com, String> {
    // SAFETY: `folder` is an IShellFolder.
    let fv = unsafe { folder.vtbl::<ShellFolderVtbl>() };
    let mut p: *mut c_void = std::ptr::null_mut();
    let hr = if children.is_empty() {
        // SAFETY: the folder's background menu ("New", "Paste", "Properties", handlers).
        unsafe { (fv.create_view_object)(folder.0, hwnd, &IID_ICONTEXTMENU, &mut p) }
    } else {
        let apidl: Vec<*const ITEMIDLIST> = children.iter().map(|c| c.0 as *const ITEMIDLIST).collect();
        // SAFETY: child PIDLs relative to `folder`, alive for the call.
        unsafe {
            (fv.get_ui_object_of)(folder.0, hwnd, apidl.len() as u32, apidl.as_ptr(), &IID_ICONTEXTMENU, std::ptr::null_mut(), &mut p)
        }
    };
    check(hr, if children.is_empty() { "CreateViewObject" } else { "GetUIObjectOf" })?;
    if p.is_null() {
        return Err("the shell returned no context menu".into());
    }
    Ok(Com(p))
}

/// The canonical verb of the command at `offset` (`GCS_VERBW`), or empty when it has none.
fn verb_of(cm: &Com, offset: u32) -> String {
    let mut verb = [0u16; 64];
    // SAFETY: `cm` is an IContextMenu; GCS_VERBW writes at most `verb.len()` wide chars, and a
    // failure leaves the buffer as it was.
    let hr = unsafe {
        (cm.vtbl::<ContextMenuVtbl>().get_command_string)(cm.0, offset as usize, GCS_VERBW, std::ptr::null_mut(), verb.as_mut_ptr() as *mut u8, verb.len() as u32)
    };
    if hr < 0 {
        return String::new();
    }
    String::from_utf16_lossy(&verb[..verb.iter().position(|&u| u == 0).unwrap_or(verb.len())])
}

/// The text of the item with command id `cmd` anywhere in `hmenu` (submenus included), without
/// its `&`; empty when there is none.
fn command_label(hmenu: HMENU, cmd: u32) -> String {
    let mut buf = [0u16; 128];
    // SAFETY: a read-only query on a live menu; MF_BYCOMMAND searches the submenus too.
    let len = unsafe { GetMenuStringW(hmenu, cmd, buf.as_mut_ptr(), buf.len() as i32, MF_BYCOMMAND) }.max(0) as usize;
    String::from_utf16_lossy(&buf[..len]).replace('&', "")
}

/// What the verb worker runs: the command picked in a menu the UI thread showed, for the same
/// items (P7-M1).
#[derive(Clone, Debug)]
pub struct VerbRequest {
    pub folder: PathBuf,
    pub names: Vec<String>,
    /// The canonical verb (`GCS_VERBW`), empty when the command has none.
    pub verb: String,
    /// The command's offset from `SHELL_FIRST` in the menu that was shown.
    pub offset: u32,
    /// Its label there, for the check when it has no verb.
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub extended: bool,
    /// No menu was shown (the Windows 11 menu's Share and Compress to, P7-M2): the worker finds
    /// the command in the menu it builds, by `verb` when given, else by `label` as a path of item
    /// labels joined by `>` ("Send to>Compressed (zipped) folder"); `offset` is unused.
    pub find: bool,
}

/// How the verb worker invoked a command (`choose`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Invoke {
    /// By its canonical verb: a rebuilt menu can number its items differently.
    Verb,
    /// By its offset: it has no verb, and the rebuilt menu's item there carries the same label.
    Offset,
    /// Not invoked, and why.
    Refuse(String),
    /// A packaged command (`IExplorerCommand`) or an Open with app (`IAssocHandler`), P7-M2.
    Command,
}

/// How to invoke a command picked in the shown menu from the menu the worker rebuilt: by `verb`
/// when it has one, else by its offset only when `rebuilt` (the label at the same id in the
/// rebuilt menu, None when there is no item there) matches `label`.
pub fn choose(verb: &str, label: &str, rebuilt: Option<&str>) -> Invoke {
    if !verb.is_empty() {
        return Invoke::Verb;
    }
    let norm = |s: &str| s.replace('&', "").trim().to_string();
    match rebuilt {
        Some(r) if !norm(label).is_empty() && norm(r) == norm(label) => Invoke::Offset,
        Some(r) => Invoke::Refuse(format!("the rebuilt menu has \"{}\" at that id, not \"{}\"", norm(r), norm(label))),
        None => Invoke::Refuse(format!("the rebuilt menu has no item at that id (\"{}\")", norm(label))),
    }
}

/// The command in a built menu that `verb` names (when not empty), else the one at the label path
/// `label` (labels joined by `>`, compared without `&` and case): its id. `items` holds each
/// command's label path, id and canonical verb.
pub fn find_command(items: &[(Vec<String>, u32, String)], verb: &str, label: &str) -> Option<u32> {
    if !verb.is_empty() {
        return items.iter().find(|(_, _, v)| v.eq_ignore_ascii_case(verb)).map(|(_, id, _)| *id);
    }
    let norm = |s: &str| s.replace('&', "").trim().to_lowercase();
    let want: Vec<String> = label.split('>').map(norm).collect();
    items.iter().find(|(path, _, _)| path.len() == want.len() && path.iter().zip(&want).all(|(a, b)| norm(a) == *b)).map(|(_, id, _)| *id)
}

/// Every command of a built menu with its label path, id and canonical verb, two levels deep.
fn command_tree(cm: &Com, hmenu: HMENU) -> Vec<(Vec<String>, u32, String)> {
    fn walk(cm: &Com, hmenu: HMENU, path: &[String], depth: u32, out: &mut Vec<(Vec<String>, u32, String)>) {
        // SAFETY: read-only queries on a live menu with valid buffers.
        let n = unsafe { GetMenuItemCount(hmenu) }.max(0) as u32;
        for i in 0..n {
            // SAFETY: as above.
            let mut mii: MENUITEMINFOW = unsafe { std::mem::zeroed() };
            mii.cbSize = std::mem::size_of::<MENUITEMINFOW>() as u32;
            mii.fMask = MIIM_FTYPE | MIIM_SUBMENU | MIIM_ID;
            // SAFETY: as above.
            unsafe { GetMenuItemInfoW(hmenu, i, 1, &mut mii) };
            if mii.fType & MFT_SEPARATOR != 0 {
                continue;
            }
            let mut buf = [0u16; 128];
            // SAFETY: as above.
            let len = unsafe { GetMenuStringW(hmenu, i, buf.as_mut_ptr(), buf.len() as i32, MF_BYPOSITION) }.max(0) as usize;
            let mut p = path.to_vec();
            p.push(String::from_utf16_lossy(&buf[..len]).replace('&', ""));
            if !mii.hSubMenu.is_null() {
                if depth > 1 {
                    walk(cm, mii.hSubMenu, &p, depth - 1, out);
                }
            } else if (SHELL_FIRST..=SHELL_LAST).contains(&mii.wID) {
                out.push((p, mii.wID, verb_of(cm, mii.wID - SHELL_FIRST)));
            }
        }
    }
    let mut out = Vec::new();
    walk(cm, hmenu, &[], 2, &mut out);
    out
}

/// Fill the dynamic submenus ("Send to", "Open with") of a rebuilt menu as opening them would, so
/// a command without a verb can be found by its id: `WM_INITMENUPOPUP` to the handler for every
/// submenu, two levels deep.
fn fill_submenus(cm: &Com, hmenu: HMENU, depth: u32) {
    let (routed, v3) = match cm.query(&IID_ICONTEXTMENU3) {
        Some(c) => (Some(c), true),
        None => (cm.query(&IID_ICONTEXTMENU2), false),
    };
    let Some(r) = routed else { return };
    fn walk(r: &Com, v3: bool, hmenu: HMENU, depth: u32) {
        // SAFETY: read-only queries on a live menu.
        let n = unsafe { GetMenuItemCount(hmenu) }.max(0);
        for i in 0..n {
            // SAFETY: as above.
            let sub = unsafe { GetSubMenu(hmenu, i) };
            if sub.is_null() {
                continue;
            }
            let (wp, lp) = (sub as WPARAM, i as LPARAM);
            // SAFETY: `r` is the IContextMenu3 or IContextMenu2 its flag says, alive for the call.
            unsafe {
                if v3 {
                    let mut res: LRESULT = 0;
                    (r.vtbl::<ContextMenu3Vtbl>().handle_menu_msg2)(r.0, WM_INITMENUPOPUP, wp, lp, &mut res);
                } else {
                    (r.vtbl::<ContextMenu2Vtbl>().handle_menu_msg)(r.0, WM_INITMENUPOPUP, wp, lp);
                }
            }
            if depth > 1 {
                walk(r, v3, sub, depth - 1);
            }
        }
    }
    walk(&r, v3, hmenu, depth);
}

/// How long the verb worker waits for a dynamic submenu to fill: Windows fills "Send to" in the
/// background, about 450 ms after it is first asked here (P7-M3's console check: "Compress to >
/// ZIP file" was refused as "no ... in the menu" when the submenu was cold).
const SUBMENU_WAIT_MS: u64 = 3000;

/// `fill_submenus`, then, until `found` says yes or `SUBMENU_WAIT_MS` passes, this thread's
/// messages pumped and the submenus asked again every 50 ms. True when found; logs a late find.
/// On the verb worker only (its own messages; a `WM_QUIT` met here is posted again and ends the wait).
fn fill_until(cm: &Com, hmenu: HMENU, what: &str, found: impl Fn() -> bool) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, PostQuitMessage, TranslateMessage, WM_QUIT};
    let t0 = std::time::Instant::now();
    loop {
        fill_submenus(cm, hmenu, 2);
        if found() {
            if t0.elapsed().as_millis() >= 50 {
                eprintln!("[menu] \"{what}\" found after {} ms (a submenu filled late)", t0.elapsed().as_millis());
            }
            return true;
        }
        if t0.elapsed().as_millis() as u64 >= SUBMENU_WAIT_MS {
            eprintln!("[menu] \"{what}\" not in the menu after {} ms", t0.elapsed().as_millis());
            return false;
        }
        // SAFETY: this thread's own message queue; a zeroed MSG is a valid out-buffer.
        unsafe {
            let mut m: MSG = std::mem::zeroed();
            while PeekMessageW(&mut m, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                if m.message == WM_QUIT {
                    PostQuitMessage(m.wParam as i32);
                    return false;
                }
                TranslateMessage(&m);
                DispatchMessageW(&m);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// On the verb worker (an STA with a message loop): build the menu for the request's items again,
/// pick the command by `choose`, and invoke it with no owner window. Every COM object is made,
/// used and released here. Returns how it was invoked and what `InvokeCommand` returned (0 when
/// refused).
pub(crate) fn run_verb(req: &VerbRequest) -> Result<(Invoke, HRESULT), String> {
    // No owner window: a dialog owned by the main window from this thread would attach the two
    // threads' input queues while it is up (P7-M1 Decisions).
    let hwnd: HWND = std::ptr::null_mut();
    let (_folder_pidl, folder, children) = parse_items(hwnd, &req.folder, &req.names)?;
    let cm = context_menu_of(hwnd, &folder, &children)?;
    // SAFETY: `cm` is an IContextMenu.
    let cv = unsafe { cm.vtbl::<ContextMenuVtbl>() };
    let menu = Menu::new()?;
    let flags = CMF_NORMAL | CMF_EXPLORE | if req.extended { CMF_EXTENDEDVERBS } else { 0 };
    // SAFETY: a live menu and IContextMenu.
    check(unsafe { (cv.query_context_menu)(cm.0, menu.0, 0, SHELL_FIRST, SHELL_LAST, flags) }, "QueryContextMenu")?;
    let cmd = SHELL_FIRST + req.offset;
    let rebuilt = |cm: &Com| {
        if req.verb.is_empty() && !crate::fs::archive::inside(&req.folder) {
            // The zip folder's menus crash in their own QueryInterface for IContextMenu2/3 (P3-M4).
            fill_until(cm, menu.0, &req.label, || command_label(menu.0, cmd).replace('&', "").trim().eq_ignore_ascii_case(req.label.rsplit('>').next().unwrap_or("").replace('&', "").trim()));
        }
        let l = command_label(menu.0, cmd);
        (!l.is_empty()).then_some(l)
    };
    let (how, offset) = if req.find {
        if !crate::fs::archive::inside(&req.folder) {
            fill_until(&cm, menu.0, &req.label, || find_command(&command_tree(&cm, menu.0), &req.verb, &req.label).is_some());
        }
        match find_command(&command_tree(&cm, menu.0), &req.verb, &req.label) {
            Some(id) => (Invoke::Offset, id - SHELL_FIRST),
            None => (Invoke::Refuse(format!("no \"{}\" in the menu", if req.verb.is_empty() { &req.label } else { &req.verb })), 0),
        }
    } else {
        (choose(&req.verb, &req.label, if req.verb.is_empty() { rebuilt(&cm) } else { None }.as_deref()), req.offset)
    };
    let dir = wide(req.folder.as_os_str());
    let verb_w = wide(std::ffi::OsStr::new(&req.verb));
    let verb_a: Vec<u8> = req.verb.bytes().map(|b| if b.is_ascii() { b } else { b'?' }).chain([0]).collect();
    let invoke = |by_verb: bool| {
        let mut ici: CMINVOKECOMMANDINFOEX = CMINVOKECOMMANDINFOEX::default();
        ici.cbSize = std::mem::size_of::<CMINVOKECOMMANDINFOEX>() as u32;
        ici.fMask = CMIC_MASK_UNICODE | CMIC_MASK_PTINVOKE;
        ici.hwnd = hwnd;
        if by_verb {
            ici.lpVerb = verb_a.as_ptr();
            ici.lpVerbW = verb_w.as_ptr();
        } else {
            // MAKEINTRESOURCE(offset): the command's offset in place of a verb string.
            ici.lpVerb = offset as usize as *const u8;
            ici.lpVerbW = offset as usize as *const u16;
        }
        if !children.is_empty() {
            ici.lpDirectoryW = dir.as_ptr();
        }
        ici.nShow = SW_SHOWNORMAL;
        ici.ptInvoke = POINT { x: req.x, y: req.y };
        // MODAL LOOP: a command may show a dialog on this thread; no app state lives here.
        // SAFETY: a live IContextMenu and a fully initialised CMINVOKECOMMANDINFOEX whose
        // strings outlive the call.
        unsafe { (cv.invoke_command)(cm.0, &ici) }
    };
    match how {
        Invoke::Refuse(_) | Invoke::Command => Ok((how, 0)),
        Invoke::Offset => Ok((how, invoke(false))),
        Invoke::Verb => {
            let hr = invoke(true);
            if hr >= 0 || hr == crate::fileops::HR_CANCELLED {
                return Ok((Invoke::Verb, hr));
            }
            // A handler that names its verb but takes only its id: by offset, when the rebuilt
            // menu's item there is the one that was picked.
            match choose("", &req.label, rebuilt(&cm).as_deref()) {
                Invoke::Offset => {
                    eprintln!("[menu] InvokeCommand by verb \"{}\" = 0x{:08X}: by offset instead", req.verb, hr as u32);
                    Ok((Invoke::Offset, invoke(false)))
                }
                _ => Ok((Invoke::Verb, hr)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_names_resolve_to_their_own_folder() {
        let s = |v: &[&str]| v.iter().map(|n| n.to_string()).collect::<Vec<String>>();
        let root = Path::new(r"C:\Windows");
        assert_eq!(common_parent(root, &s(&["a.txt", "b"])), Ok((root.to_path_buf(), s(&["a.txt", "b"]))));
        assert_eq!(common_parent(root, &[]), Ok((root.to_path_buf(), vec![])));
        assert_eq!(
            common_parent(root, &s(&[r"System32\kernel32.dll", r"system32\ntdll.dll"])),
            Ok((PathBuf::from(r"C:\Windows\System32"), s(&["kernel32.dll", "ntdll.dll"])))
        );
        assert_eq!(common_parent(root, &s(&[r"a\b\c.txt"])), Ok((PathBuf::from(r"C:\Windows\a\b"), s(&["c.txt"]))));
        assert!(common_parent(root, &s(&[r"System32\kernel32.dll", r"SysWOW64\kernel32.dll"])).is_err());
        assert!(common_parent(root, &s(&["top.txt", r"sub\x.txt"])).is_err());
        // This PC's drives are named by their roots (P7-M2 found the split cutting them).
        let pc = Path::new("This PC");
        assert_eq!(common_parent(pc, &s(&[r"C:\", r"G:\"])), Ok((pc.to_path_buf(), s(&[r"C:\", r"G:\"]))));
    }

    #[test]
    fn verb_worker_invokes_by_verb_or_by_a_matching_label() {
        assert_eq!(choose("link", "Create shortcut", None), Invoke::Verb, "a verb is enough, whatever the rebuilt menu holds");
        assert_eq!(choose("link", "Create shortcut", Some("Delete")), Invoke::Verb);
        assert_eq!(choose("", "&Desktop (create shortcut)", Some("Desktop (create shortcut)")), Invoke::Offset, "the & is ignored");
        assert_eq!(choose("", " Documents ", Some("Documents")), Invoke::Offset);
        assert!(matches!(choose("", "Documents", Some("Desktop")), Invoke::Refuse(w) if w.contains("Desktop")));
        assert!(matches!(choose("", "Documents", None), Invoke::Refuse(w) if w.contains("no item")));
        assert!(matches!(choose("", "", Some("")), Invoke::Refuse(_)), "no verb and no label: nothing to match");
    }

    #[test]
    fn commands_are_found_by_verb_or_label_path() {
        let s = |v: &[&str]| v.iter().map(|n| n.to_string()).collect::<Vec<String>>();
        let items = vec![
            (s(&["Open"]), 100, "open".to_string()),
            (s(&["Share"]), 117, String::new()),
            (s(&["Send to", "Compressed (zipped) folder"]), 131, String::new()),
            (s(&["Send to", "Documents"]), 132, String::new()),
            (s(&["Properties"]), 140, "properties".to_string()),
        ];
        assert_eq!(find_command(&items, "properties", "ignored"), Some(140), "a verb wins over the label");
        assert_eq!(find_command(&items, "Open", ""), Some(100), "verbs without case");
        assert_eq!(find_command(&items, "", "Share"), Some(117));
        assert_eq!(find_command(&items, "", "Send to>Compressed (zipped) folder"), Some(131));
        assert_eq!(find_command(&items, "", "send TO > &Compressed (zipped) folder"), Some(131), "& and case ignored");
        assert_eq!(find_command(&items, "", "Compressed (zipped) folder"), None, "the path must be whole");
        assert_eq!(find_command(&items, "openas", ""), None);
        assert_eq!(find_command(&items, "", "Send to"), None, "a submenu is no command");
    }

    #[test]
    fn canonical_verbs_route_to_twinfm_commands() {
        assert_eq!(route("delete"), Some("delete"));
        assert_eq!(route("Delete"), Some("delete"), "verbs are matched without case");
        assert_eq!(route("cut"), Some("cx"));
        assert_eq!(route("copy"), Some("cc"));
        assert_eq!(route("paste"), Some("cv"));
        assert_eq!(route("rename"), Some("rename"));
        for other in ["", "link", "properties", "open", "copyaspath", "pastelink", "undelete"] {
            assert_eq!(route(other), None, "{other} goes to the verb worker");
        }
    }

    #[test]
    fn menu_ids_partition_into_own_and_shell_ranges() {
        assert_eq!(classify(0), Pick::Nothing, "dismissed");
        assert_eq!(classify(1), Pick::Own(OwnItem::OpenOther));
        assert_eq!(classify(2), Pick::Own(OwnItem::Terminal));
        assert_eq!(classify(3), Pick::Own(OwnItem::CopyPath));
        assert_eq!(classify(4), Pick::Nothing, "an own id with no item");
        assert_eq!(classify(20), Pick::Nothing);
        assert_eq!(classify(21), Pick::Nothing, "the gap between the ranges");
        assert_eq!(classify(99), Pick::Nothing);
        assert_eq!(classify(100), Pick::Shell(0), "the shell's first id is verb offset 0");
        assert_eq!(classify(137), Pick::Shell(37));
        assert_eq!(classify(0x7FFF), Pick::Shell(0x7FFF - 100));
        assert_eq!(classify(0x8000), Pick::Nothing);
        for o in OwnItem::ALL {
            assert!((OWN_FIRST..=OWN_LAST).contains(&o.id()));
            assert!(o.id() < SHELL_FIRST);
        }
    }

    #[test]
    fn terminal_commands_and_fallback() {
        let [wt, ps] = terminal_commands(r"C:\Users\me\My Docs");
        assert_eq!(wt, r#"wt.exe -d "C:\Users\me\My Docs""#);
        assert_eq!(ps, r#"powershell.exe -NoExit -Command "Set-Location -LiteralPath 'C:\Users\me\My Docs'""#);
        let [wt, ps] = terminal_commands(r"C:\");
        assert_eq!(wt, r#"wt.exe -d "C:\\""#, "a trailing backslash must not escape the quote");
        assert_eq!(ps, r#"powershell.exe -NoExit -Command "Set-Location -LiteralPath 'C:\'""#);
        let [wt, ps] = terminal_commands(r"D:\a;b\it's $x");
        assert_eq!(wt, r#"wt.exe -d "D:\a\;b\it's $x""#, "wt's command separator is escaped");
        assert_eq!(ps, r#"powershell.exe -NoExit -Command "Set-Location -LiteralPath 'D:\a;b\it''s $x'""#);
    }

    #[test]
    fn copy_path_text_is_one_path_per_line_crlf() {
        let paths = [PathBuf::from(r"C:\a.txt"), PathBuf::from(r"C:\dir\b c"), PathBuf::from(r"D:\")];
        assert_eq!(path_text(&paths), "C:\\a.txt\r\nC:\\dir\\b c\r\nD:\\");
        assert_eq!(path_text(&paths[..1]), r"C:\a.txt", "no trailing newline");
        assert_eq!(path_text(&[]), "");
    }

    #[test]
    fn keyboard_menu_point_sits_under_the_row_inside_the_viewport() {
        let g = ListGeom::new(false);
        // List at (10, 63), not scrolled, 400 px tall: row 0's bottom is 22 px down.
        assert_eq!(row_menu_point(10.0, 63.0, 0.0, 400.0, 0, g), (38.0, 85.0));
        assert_eq!(row_menu_point(10.0, 63.0, 0.0, 400.0, 3, g), (38.0, 63.0 + 88.0));
        // Scrolled by 100 rows: row 105 is the sixth on screen.
        assert_eq!(row_menu_point(10.0, 63.0, -2200.0, 400.0, 105, g), (38.0, 63.0 + 6.0 * 22.0));
        // Rows above or below the viewport clamp to its edges.
        assert_eq!(row_menu_point(10.0, 63.0, -2200.0, 400.0, 0, g).1, 63.0);
        assert_eq!(row_menu_point(10.0, 63.0, 0.0, 400.0, 500, g).1, 463.0);
        assert_eq!(row_menu_point(10.0, 63.0, 0.0, -5.0, 2, g).1, 63.0, "a collapsed list");
        // 32 px rows (thumbnails on): the name column starts 42 px in (a 4 px gap after the thumbnail).
        let t = ListGeom::new(true);
        assert_eq!(row_menu_point(10.0, 63.0, 0.0, 400.0, 0, t), (52.0, 95.0));
        assert_eq!(row_menu_point(10.0, 63.0, -3200.0, 400.0, 105, t), (52.0, 63.0 + 6.0 * 32.0));
    }

    #[test]
    fn tile_menu_point_sits_under_the_tile() {
        const L: crate::icons::TileGeom = crate::icons::TileGeom::LARGE;
        // Five tiles to a grid row, 96 x 136 px: entry 7 is grid row 1, column 2.
        assert_eq!(tile_menu_point(10.0, 63.0, 0.0, 600.0, 0, 5, L), (12.0, 199.0));
        assert_eq!(tile_menu_point(10.0, 63.0, 0.0, 600.0, 7, 5, L), (10.0 + 2.0 * 96.0 + 2.0, 63.0 + 272.0));
        // Scrolled by ten grid rows: entry 52 (grid row 10, column 2) is the first row on screen.
        assert_eq!(tile_menu_point(10.0, 63.0, -1360.0, 600.0, 52, 5, L), (204.0, 199.0));
        // Tiles above or below the viewport clamp to its edges; one column reads every entry as its own row.
        assert_eq!(tile_menu_point(10.0, 63.0, -1360.0, 600.0, 0, 5, L).1, 63.0);
        assert_eq!(tile_menu_point(10.0, 63.0, 0.0, 600.0, 500, 5, L).1, 663.0);
        assert_eq!(tile_menu_point(10.0, 63.0, 0.0, 600.0, 3, 1, L), (12.0, 63.0 + 4.0 * 136.0));
        // Medium tiles, 80 x 88 px (P6-M2, 72 before P7-M4): entry 7 of five to a row is grid row 1, column 2.
        let m = crate::icons::TileGeom::MEDIUM;
        assert_eq!(tile_menu_point(10.0, 63.0, 0.0, 600.0, 7, 5, m), (10.0 + 2.0 * 80.0 + 2.0, 63.0 + 176.0));
    }
}

