//! Scripted driver for unattended gate runs. Set TWINFM_SCRIPT to a ';'-separated list
//! (or to @<file> to read the list from a file; Windows caps an env var at 32 KB):
//!   sleep <ms>                       wait
//!   split                            toggle the second pane (same as F3)
//!   wheel <pane> <ticks> <ms>        wheel ticks (negative = down) <ms> apart over the pane's list
//!   drag <pane> <dy> <ms>            press the pane's scrollbar thumb and drag it <dy> px in 16 ms steps
//!   nav <pane> <path>                navigate a pane (a path inside a zip archive, `C:\dir\x.zip\sub`,
//!                                    lists that level of the archive)
//!   up <pane>                        go to the parent directory
//!   scrollpos <pane>                 log the pane's list scroll (content-y) and cursor row
//!   sort <pane> <0|1|2|3>            click a column header (name/size/modified/type)
//!   key <name>                       press a key through the Slint key path; names: F3, F5,
//!                                    Backspace, Return, Space, AltUp, CtrlShiftRight, CtrlH
//!                                    (hidden files), CtrlR (refresh), CtrlA, Down, ShiftUp, ShiftDown,
//!                                    ShiftF5, ShiftF6, Delete, ShiftDelete, F2, F7, CtrlC, CtrlX,
//!                                    CtrlV, CtrlZ, CtrlShiftN, Escape, ShiftF10, Menu (the last two open
//!                                    the context menu and take in-menu steps like rclick); CtrlT,
//!                                    CtrlW, CtrlTab, CtrlShiftTab, CtrlL, Ctrl1 to Ctrl9, CtrlD, F1, F9,
//!                                    CtrlShiftPageUp, CtrlShiftPageDown, AltLeft, AltRight (back
//!                                    and forward in the tab's history), CtrlComma (settings),
//!                                    CtrlF (the filter box), CtrlShiftF (search), AltReturn (size
//!                                    every folder in the listing), and any single character, typed
//!                                    as itself (`key a`)
//!   copy | move                      Shift+F5 / Shift+F6: the active pane's selection to the other pane
//!   delete | sdelete                 Delete (Recycle Bin) / Shift+Delete (permanent)
//!   cc | cx | cv                     Ctrl+C / Ctrl+X / Ctrl+V through the real clipboard
//!   undo                             Ctrl+Z: undo the newest recorded operation (fileops::undo)
//!   newfolder <name>                 F7, type the name into the editor, Return
//!   rename <name>                    F2 on the cursor row, type the new name, Return
//!   waitops <ms>                     wait until no file operation runs, no pane refreshes and no
//!                                    virtual-file drop writes (at most <ms>), then 300 ms more for
//!                                    the pass to land
//!   click <pane> <row>               click a visible row (selects only it, moves the cursor); in the
//!                                    large-icon grid, the tile of visible entry <row> (row / cols
//!                                    down, row % cols across), as for every step that takes a row
//!   ctrlclick <pane> <row>           Ctrl+click a visible row (toggles it)
//!   shiftclick <pane> <row>          Shift+click a visible row (selects anchor..row)
//!   dblclick <pane> <row>            double-click a visible row (enters the directory if it is one)
//!   midclick <pane> <row>            middle-click a visible row (a folder opens in a new tab)
//!   backclick <pane>                 the mouse's Back button in the middle of the pane's list (a row
//!                                    or the empty space), as posted WM_MOUSEMOVE, WM_XBUTTONDOWN and
//!                                    WM_XBUTTONUP (XBUTTON1) messages, so it takes winit's real path:
//!                                    the pane becomes active and goes back in its tab's history
//!   forwardclick <pane>              the same with the Forward button (XBUTTON2): forward in the history
//!   history <back|forward>           Alt+Left / Alt+Right through the Slint key path, on the active pane
//!   view <details|icons>             Ctrl+Shift+1 / Ctrl+Shift+2 (key names CtrlShift1, CtrlShift2): the
//!                                    active pane's view, the details list or the large-icon grid
//!                                    (P4-M1); `[view]` lines on stderr. Key names Up, Down, Left,
//!                                    Right, PageUp, PageDown, Home, End (and ShiftLeft, ShiftRight)
//!                                    move the cursor in the focused list
//!   rclick <pane> <row>              right-click a visible row: press, 60 ms for a frame, release;
//!                                    the release opens the real shell context menu
//!   rclickempty <pane>               right-click the empty space below the last row (the folder's
//!                                    background menu); when the rows fill the list there is none,
//!                                    and the step calls the pane's context-menu callback directly
//!   menuopen <text>                  inside the menu: open the submenu whose item reads <text>
//!                                    ("Send to"), with Down presses then Right; never Return
//!   menukey <Down|Up|Right|Left|Escape>  inside the menu: one key
//!   menupick <text>                  inside the menu: choose one of TwinFM's own items ("Copy path",
//!                                    "Open in other pane", "Open in terminal", and the drop menu's
//!                                    "Move here", "Copy here", "Cancel"); refuses shell items
//!   menuclose                        inside the menu: close it (EndMenu)
//!   dragrow <pane> <row> <dx> <dy> [right]
//!                                    drag a visible row: press (the right button with `right`),
//!                                    60 ms, move <dx>,<dy> px, which starts the OLE drag
//!                                    (DoDragDrop) if past the threshold; takes in-drag steps
//!                                    (`sleep`, then `dragcancel`) like rclick takes in-menu steps.
//!                                    A scripted drag can only be cancelled: it never drops
//!                                    (dnd::script_begin says why)
//!   dragcancel                       inside the drag: Escape (a WM_KEYDOWN posted to the window)
//!   dropfiles <pane> <row> <copy|move|auto|menu <copy|move|cancel>> <path|path|...>
//!                                    drop files without OLE's drag loop: our drop target's
//!                                    DragEnter then Drop, called directly with a data object
//!                                    serving the paths as CF_HDROP, at the centre of a visible row
//!                                    (or, with row -1, the pane's header: its folder). In place of
//!                                    `<pane> <row>`, `side <row>` aims at sidebar row <row> (headers
//!                                    count; 0 is "Drives"; -1 the last row, the newest bookmark) and
//!                                    `tab <pane> <index>` at tab <index>
//!                                    of the pane's strip (shown with two tabs or more). copy holds
//!                                    Ctrl, move Shift, auto neither; menu holds the right button,
//!                                    so Drop opens the drop menu, and picks "Copy here", "Move
//!                                    here" or "Cancel" from the in-menu timer. Paths are separated
//!                                    by `|` because `;` separates steps
//!   dropvirtual <pane> <row> [async] <name=size|name=size|...>
//!                                    drop virtual files (FileGroupDescriptorW + FileContents, no
//!                                    CF_HDROP) the same way: `name=size` is a file of generated
//!                                    bytes (`sub\file.txt=100` makes `sub`), `name\` a folder;
//!                                    with `async` the source asks for asynchronous extraction
//!                                    (IDataObjectAsyncCapability). A copy, written by TwinFM
//!   dropvirtual <pane> <row> zip <path|path|...>
//!                                    the same with the shell zip folder's own data object for
//!                                    paths inside one folder of an archive (what a drag out of a
//!                                    zip pane carries). `waitops` waits for an async drop to end.
//!                                    `ownzip` in place of `zip` marks the drop as our own drag
//!                                    out of the archive (the paths are its sources), which the
//!                                    target turns into the extraction job; `idlist` drops the
//!                                    shell's generic data object over those items instead
//!                                    (`SHCreateDataObject`: a Shell IDList Array only)
//!   dragover <pane> <top|bottom> <ms> <path|path|...>
//!                                    a drag of the paths enters the window 6 px inside the pane
//!                                    list's top or bottom edge and hovers there for <ms> (our drop
//!                                    target's DragEnter, DragOver every 20 ms from a Slint timer,
//!                                    DragLeave): the auto-scroll near the edges. It never drops
//!   tab <new|close|next|prev|N>      Ctrl+T, Ctrl+W, Ctrl+Tab, Ctrl+Shift+Tab or Ctrl+N (1..9) through the
//!                                    Slint key path, on the active pane
//!   tab <moveleft|moveright>         Ctrl+Shift+PageUp / Ctrl+Shift+PageDown: move the active tab
//!   tabdrag <pane> <index> <dx>      press the left button on tab <index> (0-based), move <dx> px
//!                                    in 8 px steps, release: a tab drag through Slint's pointer path
//!   crumb <pane> <index>             breadcrumb button <index> (0 is This PC), through its callback
//!   crumbmenu <pane> <index>         the chevron after crumb <index>: its subfolders in a dropdown
//!                                    (listed on a worker; sleep before crumbpick)
//!   crumbpick <name>                 pick the dropdown's folder <name> (the pane navigates there)
//!   crumbclose                       close the dropdown (as Escape does)
//!   pathmode <pane> <entry|crumb>    the path bar as text entry (as Ctrl+L) or back to the breadcrumb
//!   sidebar toggle                   F9: show or hide the sidebar
//!   sidebar click <row>              sidebar row <row> (headers count; 0 is "Drives"), through its callback
//!   bookmark add                     Ctrl+D: bookmark the active tab's folder
//!   bookmark remove <index>          the bookmark menu's "Remove bookmark" for bookmark <index>
//!   bookmark rename <index> <title>  what the bookmark row's title editor commits (an empty title
//!                                    goes back to the folder's name)
//!   bookmark move <index> <up|down>  the bookmark menu's "Move up" / "Move down"
//!   about open | about close         F1 opens the About dialog; close closes it
//!   settings open                    Ctrl+, opens the settings dialog (key name CtrlComma)
//!   settings set <field> <value>     stage a value in the open dialog through the function its control
//!                                    calls: fields start_left, start_right (a path; the rest of the
//!                                    step, may be empty), hidden_left, hidden_right, type_column,
//!                                    thumbnails (32 px rows with thumbnails; applied on OK, it lists
//!                                    both panes again), sidebar (true|false), path_bar (entry|crumb)
//!   settings ok | settings close     the OK button (apply the staged changes) / the Cancel button
//!                                    (discard them); `key Escape` cancels through the key path
//!   touch <pane> <name>              create an empty file in the pane's folder, or set an existing
//!                                    one's modified time to now (a change for the directory watch)
//!   rmfile <pane> <name>             delete that file from the pane's folder. Both refuse paths
//!                                    outside the soak folders under TWINFM_SOAK=1, as file
//!                                    operations do; otherwise use them on %TEMP% scratch folders only
//!   waitwatch <ms>                   wait until the next refresh started by the directory watch has
//!                                    listed its folder (at most <ms>), then 300 ms more for the pass
//!   filter <pane> <text>             put <text> (the rest of the step, may be empty) in the pane's
//!                                    filter box, opening it without the focus, and call the box's
//!                                    `edited` callback, as typing does (applied 150 ms later)
//!   search <pane>                    Ctrl+Shift+F's callback: a recursive name search for the
//!                                    filter box's text in a new tab (an empty box opens, focused)
//!   waitsearch <ms>                  wait until no search tab is still walking (at most <ms>), then
//!                                    300 ms more for the pass
//!   sizeall <pane>                   Alt+Return in the pane's list (focused first): size every folder
//!                                    of the listing (at most 500, nearest the cursor first)
//!   waitsize <ms>                    wait until no folder size is queued or walking and the last
//!                                    reply has landed (at most <ms>), then 300 ms more
//!   window <max|restore|close>       maximise or restore the window (ShowWindow), or close it as
//!                                    the close button does (WM_CLOSE: settings saved, app exits)
//!   quit                             exit the event loop
//! Events are dispatched with slint::Window::dispatch_event on the UI thread, so Slint's full
//! input -> layout -> render path runs; only the OS WndProc hop is skipped.
//!
//! In-menu steps. The shell menu's `TrackPopupMenuEx` runs a modal loop inside the release
//! event, and while it runs no Slint timer fires: winit buffers every event that reaches it
//! while its handler is busy (and these steps run inside it), so a step scheduled with
//! `slint::Timer` would only run after the menu has closed. So `rclick`/`rclickempty` take the
//! steps that follow them, as long as they are `sleep`, `menuopen`, `menukey` or `menuclose`,
//! up to and including the first `menuclose`, and run those from a Win32 thread timer
//! (`SetTimer` with a `TIMERPROC`), which the menu's own loop dispatches. Each in-menu tick logs
//! whether the app state and the driver are free (no RefCell borrow held across the loop). The
//! script resumes with the step after `menuclose` once the menu has closed. Without a
//! `menuclose` in that run of steps the menu stays open for a person to close. Example:
//!   rclick 0 3; sleep 300; menuopen Send to; sleep 300; menuclose; sleep 300; quit
//!
//! In-drag steps: `DoDragDrop` is a modal loop inside the pointer move that crossed the
//! threshold, so `dragrow` takes the `sleep` and `dragcancel` steps after it, up to
//! the first `dragcancel`. Unlike the menu's loop, OLE's drag loop runs no Win32 timer callback,
//! so these steps run on a helper thread (`spawn_drag_pump`), which also posts mouse moves to
//! keep the loop turning. While a scripted drag runs, the drop source counts the left button as
//! held (the real one is up), so only Escape ends it. Example:
//!   dragrow 0 2 0 40; sleep 300; dragcancel; sleep 300; quit

use crate::App;
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, SharedString, Timer, TimerMode};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::SystemServices::{MK_CONTROL, MK_RBUTTON, MK_SHIFT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT, VK_UP};
use windows_sys::Win32::UI::WindowsAndMessaging::{EndMenu, KillTimer, PostMessageW, SetTimer, WM_KEYDOWN, WM_MOUSEMOVE};

/// Where a `dropfiles` lands: a pane's row (-1: the pane's folder), a sidebar row, or a tab.
#[derive(Debug, Clone, Copy)]
enum DropAt {
    Row(usize, i32),
    Side(i32),
    Tab(usize, usize),
}

#[derive(Debug, Clone)]
enum Step {
    Sleep(u64),
    Split,
    Wheel { pane: usize, ticks: i32, ms: u64 },
    Drag { pane: usize, dy: f32, ms: u64 },
    Nav { pane: usize, path: String },
    Up(usize),
    ScrollPos(usize),
    Sort { pane: usize, col: i32 },
    Key(String),
    Click { pane: usize, row: usize, modifier: Option<Key> },
    DblClick { pane: usize, row: usize },
    MidClick { pane: usize, row: usize },
    /// Post the mouse's Back (or, `forward`, Forward) button's press and release to the window, in
    /// the middle of the pane's list area.
    XButton { pane: usize, forward: bool },
    /// Press `lead`, type `text` into whatever then has focus, press Return.
    Type { lead: String, text: String },
    WaitOps(u64),
    /// Right-click a row, or with `row: None` the empty space below the rows.
    RClick { pane: usize, row: Option<usize> },
    MenuOpen(String),
    MenuKey(String),
    MenuPick(String),
    MenuClose,
    /// Press a row (the right button with `right`), move `dx`,`dy`: an OLE drag past the threshold.
    DragRow { pane: usize, row: usize, dx: f32, dy: f32, right: bool },
    DragCancel,
    /// Drop `paths` on a row (or, row -1, the pane's folder) through our drop target directly.
    /// Mode `menu` is a right-button drop whose menu gets `pick` (copy, move or cancel).
    DropFiles { at: DropAt, mode: String, pick: String, paths: Vec<String> },
    /// `dropvirtual`: virtual files (`name=size`, a folder `name\`; with `async` from a source that
    /// asks for asynchronous extraction), or with `zip` the shell's data object for paths inside an
    /// archive.
    DropVirtual { pane: usize, row: i32, zip: bool, own: bool, idlist: bool, asynced: bool, items: Vec<String> },
    /// Hover a drag of `paths` near the pane list's top or bottom edge for `ms`, then leave.
    DragOver { pane: usize, top: bool, ms: u64, paths: Vec<String> },
    Crumb { pane: usize, index: i32 },
    CrumbMenu { pane: usize, index: i32 },
    CrumbPick(String),
    CrumbClose,
    /// Press the left button on tab `index`'s centre, move `dx` px in 8 px steps, release.
    TabDrag { pane: usize, index: usize, dx: f32 },
    PathMode { pane: usize, entry: bool },
    SideClick(i32),
    BookmarkRemove(i32),
    BookmarkRename(i32, String),
    /// Bookmark index, and up (true) or down.
    BookmarkMove(i32, bool),
    AboutClose,
    /// The settings dialog's OK (true) or Cancel button.
    SettingsClose(bool),
    /// Stage `value` for the dialog's `field`.
    SettingsSet(String, String),
    /// `touch` (create or update) or `rmfile` (delete) a file in the pane's folder.
    ScriptFile { pane: usize, name: String, remove: bool },
    WaitWatch(u64),
    /// Set the pane's filter box to `text` (opening it, without the focus) and call its `edited`
    /// callback, as typing does.
    Filter { pane: usize, text: String },
    /// Ctrl+Shift+F's callback on the pane.
    Search(usize),
    /// Wait until no search tab is still walking (at most `ms`), then 300 ms for the pass.
    WaitSearch(u64),
    /// Alt+Return in the pane's list.
    SizeAll(usize),
    /// Wait until no folder size is queued or walking (at most `ms`), then 300 ms.
    WaitSize(u64),
    /// `window max` maximises the window, `window restore` restores it (ShowWindow), `window
    /// close` posts WM_CLOSE (the close button's path: settings saved, then the app exits).
    Window(String),
    Quit,
}

fn parse(script: &str) -> Vec<Step> {
    script
        .split(';')
        .filter_map(|s| {
            let a: Vec<&str> = s.split_whitespace().collect();
            let num = |i: usize| a.get(i).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            Some(match *a.first()? {
                "sleep" => Step::Sleep(num(1) as u64),
                "split" => Step::Split,
                "wheel" => Step::Wheel { pane: num(1) as usize, ticks: num(2) as i32, ms: num(3).max(1) as u64 },
                "drag" => Step::Drag { pane: num(1) as usize, dy: num(2) as f32, ms: num(3).max(1) as u64 },
                "nav" => Step::Nav { pane: num(1) as usize, path: a[2..].join(" ") },
                "up" => Step::Up(num(1) as usize),
                "scrollpos" => Step::ScrollPos(num(1) as usize),
                "sort" => Step::Sort { pane: num(1) as usize, col: num(2) as i32 },
                "key" => Step::Key(a.get(1).unwrap_or(&"").to_string()),
                "click" => Step::Click { pane: num(1) as usize, row: num(2) as usize, modifier: None },
                "ctrlclick" => Step::Click { pane: num(1) as usize, row: num(2) as usize, modifier: Some(Key::Control) },
                "shiftclick" => Step::Click { pane: num(1) as usize, row: num(2) as usize, modifier: Some(Key::Shift) },
                "dblclick" => Step::DblClick { pane: num(1) as usize, row: num(2) as usize },
                "midclick" => Step::MidClick { pane: num(1) as usize, row: num(2) as usize },
                "backclick" => Step::XButton { pane: num(1) as usize, forward: false },
                "forwardclick" => Step::XButton { pane: num(1) as usize, forward: true },
                "history" => Step::Key(if a.get(1) == Some(&"forward") { "AltRight" } else { "AltLeft" }.into()),
                "view" => Step::Key(if a.get(1) == Some(&"icons") { "CtrlShift2" } else { "CtrlShift1" }.into()),
                "copy" => Step::Key("ShiftF5".into()),
                "move" => Step::Key("ShiftF6".into()),
                "delete" => Step::Key("Delete".into()),
                "sdelete" => Step::Key("ShiftDelete".into()),
                "cc" => Step::Key("CtrlC".into()),
                "cx" => Step::Key("CtrlX".into()),
                "cv" => Step::Key("CtrlV".into()),
                "undo" => Step::Key("CtrlZ".into()),
                "newfolder" => Step::Type { lead: "F7".into(), text: a[1..].join(" ") },
                "rename" => Step::Type { lead: "F2".into(), text: a[1..].join(" ") },
                "waitops" => Step::WaitOps(num(1).max(1) as u64),
                "rclick" => Step::RClick { pane: num(1) as usize, row: Some(num(2) as usize) },
                "rclickempty" => Step::RClick { pane: num(1) as usize, row: None },
                "menuopen" => Step::MenuOpen(a[1..].join(" ")),
                "menukey" => Step::MenuKey(a.get(1).unwrap_or(&"").to_string()),
                "menupick" => Step::MenuPick(a[1..].join(" ")),
                "menuclose" => Step::MenuClose,
                "dragrow" => Step::DragRow {
                    pane: num(1) as usize,
                    row: num(2) as usize,
                    dx: num(3) as f32,
                    dy: num(4) as f32,
                    right: a.get(5) == Some(&"right"),
                },
                "dragcancel" => Step::DragCancel,
                "dropfiles" => {
                    // `tab <pane> <index>` takes one word more than `<pane> <row>` and `side <row>`.
                    let m = if a.get(1) == Some(&"tab") { 4 } else { 3 };
                    let mode = a.get(m).unwrap_or(&"auto").to_string();
                    // `menu <copy|move|cancel>` takes one more word before the paths.
                    let (pick, first) = if mode == "menu" { (a.get(m + 1).unwrap_or(&"cancel").to_string(), m + 2) } else { (String::new(), m + 1) };
                    // The spot: `<pane> <row>`, `side <row>` or `tab <pane> <index>`.
                    let at = match a.get(1).copied() {
                        Some("side") => DropAt::Side(num(2) as i32),
                        Some("tab") => DropAt::Tab(num(2) as usize, num(3) as usize),
                        _ => DropAt::Row(num(1) as usize, num(2) as i32),
                    };
                    Step::DropFiles {
                        at,
                        mode,
                        pick,
                        paths: a.get(first..).unwrap_or(&[]).join(" ").split('|').map(str::trim).filter(|p| !p.is_empty()).map(String::from).collect(),
                    }
                }
                "dropvirtual" => {
                    let (own, idlist) = (a.get(3) == Some(&"ownzip"), a.get(3) == Some(&"idlist"));
                    let (zip, asynced) = (own || idlist || a.get(3) == Some(&"zip"), a.get(3) == Some(&"async"));
                    let first = if zip || asynced { 4 } else { 3 };
                    Step::DropVirtual {
                        pane: num(1) as usize,
                        row: num(2) as i32,
                        zip,
                        own,
                        idlist,
                        asynced,
                        items: a.get(first..).unwrap_or(&[]).join(" ").split('|').map(str::trim).filter(|p| !p.is_empty()).map(String::from).collect(),
                    }
                }
                "dragover" => Step::DragOver {
                    pane: num(1) as usize,
                    top: a.get(2) == Some(&"top"),
                    ms: num(3).max(1) as u64,
                    paths: a.get(4..).unwrap_or(&[]).join(" ").split('|').map(str::trim).filter(|p| !p.is_empty()).map(String::from).collect(),
                },
                "tab" => Step::Key(match a.get(1).copied().unwrap_or("") {
                    "new" => "CtrlT".into(),
                    "close" => "CtrlW".into(),
                    "next" => "CtrlTab".into(),
                    "prev" => "CtrlShiftTab".into(),
                    "moveleft" => "CtrlShiftPageUp".into(),
                    "moveright" => "CtrlShiftPageDown".into(),
                    n => format!("Ctrl{n}"),
                }),
                "tabdrag" => Step::TabDrag { pane: num(1) as usize, index: num(2) as usize, dx: num(3) as f32 },
                "crumb" => Step::Crumb { pane: num(1) as usize, index: num(2) as i32 },
                "crumbmenu" => Step::CrumbMenu { pane: num(1) as usize, index: num(2) as i32 },
                "crumbpick" => Step::CrumbPick(a[1..].join(" ")),
                "crumbclose" => Step::CrumbClose,
                "pathmode" => Step::PathMode { pane: num(1) as usize, entry: a.get(2) == Some(&"entry") },
                "sidebar" if a.get(1) == Some(&"click") => Step::SideClick(num(2) as i32),
                "sidebar" => Step::Key("F9".into()),
                "bookmark" if a.get(1) == Some(&"remove") => Step::BookmarkRemove(num(2) as i32),
                "bookmark" if a.get(1) == Some(&"rename") => Step::BookmarkRename(num(2) as i32, a.get(3..).unwrap_or(&[]).join(" ")),
                "bookmark" if a.get(1) == Some(&"move") => Step::BookmarkMove(num(2) as i32, a.get(3) == Some(&"up")),
                "bookmark" => Step::Key("CtrlD".into()),
                "about" if a.get(1) == Some(&"close") => Step::AboutClose,
                "about" => Step::Key("F1".into()),
                "settings" if a.get(1) == Some(&"ok") => Step::SettingsClose(true),
                "settings" if a.get(1) == Some(&"close") => Step::SettingsClose(false),
                "settings" if a.get(1) == Some(&"set") => {
                    Step::SettingsSet(a.get(2).unwrap_or(&"").to_string(), a.get(3..).unwrap_or(&[]).join(" "))
                }
                "settings" => Step::Key("CtrlComma".into()),
                "touch" => Step::ScriptFile { pane: num(1) as usize, name: a.get(2..).unwrap_or(&[]).join(" "), remove: false },
                "rmfile" => Step::ScriptFile { pane: num(1) as usize, name: a.get(2..).unwrap_or(&[]).join(" "), remove: true },
                "waitwatch" => Step::WaitWatch(num(1).max(1) as u64),
                // The text is the rest of the step after the pane, trimmed (may be empty).
                "filter" => Step::Filter {
                    pane: num(1) as usize,
                    text: s.trim().splitn(3, char::is_whitespace).nth(2).unwrap_or("").to_string(),
                },
                "search" => Step::Search(num(1) as usize),
                "waitsearch" => Step::WaitSearch(num(1).max(1) as u64),
                "sizeall" => Step::SizeAll(num(1) as usize),
                "waitsize" => Step::WaitSize(num(1).max(1) as u64),
                "window" => Step::Window(a.get(1).copied().unwrap_or("").to_string()),
                "quit" => Step::Quit,
                other => {
                    eprintln!("[auto] unknown step: {other}");
                    return None;
                }
            })
        })
        .collect()
}

struct Runner {
    app: slint::Weak<App>,
    steps: Vec<Step>,
    idx: usize,
    timer: Timer,
    sub: Timer,
}

thread_local! {
    static RUNNER: RefCell<Option<Rc<RefCell<Runner>>>> = const { RefCell::new(None) };
    /// The steps to run inside the open menu, from the Win32 timer. Borrowed only briefly, by
    /// the runner before and after the menu and by the timer callback inside it.
    static IN_MENU: RefCell<VecDeque<Step>> = const { RefCell::new(VecDeque::new()) };
    /// The in-menu Win32 timer (0: none), when the next in-menu step is due, and the window the
    /// in-menu keys go to.
    static MENU_TIMER: Cell<usize> = const { Cell::new(0) };
    static MENU_DUE: Cell<Option<std::time::Instant>> = const { Cell::new(None) };
    static MENU_HWND: Cell<isize> = const { Cell::new(0) };
}

/// Time origin for the in-menu tick log.
static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

pub fn start(app: &App) {
    let Ok(mut script) = std::env::var("TWINFM_SCRIPT") else { return };
    if let Some(path) = script.strip_prefix('@') {
        script = std::fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("[auto] cannot read {path}: {e}");
            String::new()
        });
    }
    let steps = parse(&script);
    T0.get_or_init(std::time::Instant::now);
    eprintln!("[auto] {} steps", steps.len());
    let runner = Rc::new(RefCell::new(Runner {
        app: app.as_weak(),
        steps,
        idx: 0,
        timer: Timer::default(),
        sub: Timer::default(),
    }));
    RUNNER.with(|r| *r.borrow_mut() = Some(runner.clone()));
    schedule(runner, 500);
}

fn schedule(runner: Rc<RefCell<Runner>>, delay_ms: u64) {
    let r2 = runner.clone();
    runner
        .borrow()
        .timer
        .start(TimerMode::SingleShot, Duration::from_millis(delay_ms), move || run_step(r2.clone()));
}

/// Logical window size and a point inside the given pane's list.
fn geometry(app: &App, pane: usize) -> (f32, f32, LogicalPosition) {
    let l = crate::window_layout(app);
    let (a, b) = crate::dnd::pane_x(&l, pane);
    (l.width, l.height, LogicalPosition::new((a + b) * 0.5, l.height * 0.5))
}

/// The top of the pane's list (63 px, or 91 px under a tab strip).
fn list_top(app: &App, pane: usize) -> f32 {
    crate::dnd::list_top(&crate::window_layout(app), pane)
}

/// Where the pane's vertical scrollbar thumb sits with content-y = 0 (right edge of the list,
/// just under the path bar and column header).
fn thumb(app: &App, pane: usize) -> LogicalPosition {
    let (_, right_edge) = crate::dnd::pane_x(&crate::window_layout(app), pane);
    LogicalPosition::new(right_edge - 10.0, list_top(app, pane) + 17.0)
}

/// Centre of a visible row: rows start under the 3 px padding, the tab strip when there is one,
/// 32 px path bar, 2 px spacing, 24 px header, 2 px spacing; 22 px each (32 px with thumbnails on).
/// In the large-icon grid, the centre of visible entry `row`'s tile (`row / cols` grid rows down,
/// `row % cols` tiles across, 96 x 120 px each).
fn row_pos(app: &App, pane: usize, row: usize) -> LogicalPosition {
    let (_, _, mid) = geometry(app, pane);
    let l = crate::window_layout(app);
    match l.grid[pane] {
        None => LogicalPosition::new(mid.x, list_top(app, pane) + row as f32 * l.row_px + l.row_px / 2.0),
        Some(cols) => {
            let (r, c) = crate::model::grid_cell(row, cols);
            let (left, _) = crate::dnd::pane_x(&l, pane);
            let x = left + crate::dnd::PANE_PAD + (c as f32 + 0.5) * crate::icons::TILE_W;
            LogicalPosition::new(x, list_top(app, pane) + (r as f32 + 0.5) * crate::icons::TILE_H)
        }
    }
}

/// Where an `rclickempty` lands: halfway down the empty space below the last row (the last grid
/// row of tiles), or None when the rows fill the list (rows start at `list_top`; the list ends 23
/// px above the window bottom, over the 18 px status line, 2 px spacing and 3 px padding).
fn empty_pos(app: &App, pane: usize) -> Option<LogicalPosition> {
    use slint::Model;
    let (_, h, mid) = geometry(app, pane);
    let l = crate::window_layout(app);
    let rows = if pane == 0 { app.get_left_rows().row_count() } else { app.get_right_rows().row_count() };
    let lines = match l.grid[pane] {
        None => rows,
        Some(cols) => crate::model::grid_rows(rows, cols),
    };
    let end = list_top(app, pane) + app.invoke_get_scroll(pane as i32) + lines as f32 * l.row_px_of(pane);
    let bottom = h - 23.0;
    (end + 4.0 < bottom).then(|| LogicalPosition::new(mid.x, (end + bottom) * 0.5))
}

/// Take the in-loop steps that follow an `rclick` (or a `dragrow`): the leading run of sleep/
/// menuopen/menukey/menupick/menuclose/dragcancel steps up to and including the first one that
/// ends the loop (menuclose, menupick, dragcancel). None if that run has none.
fn take_menu_steps(r: &mut Runner) -> Vec<Step> {
    let rest = &r.steps[r.idx..];
    let run = rest
        .iter()
        .take_while(|s| {
            matches!(
                s,
                Step::Sleep(_)
                    | Step::MenuOpen(_)
                    | Step::MenuKey(_)
                    | Step::MenuPick(_)
                    | Step::MenuClose
                    | Step::DragCancel
            )
        })
        .count();
    let Some(close) =
        rest[..run].iter().position(|s| matches!(s, Step::MenuClose | Step::MenuPick(_) | Step::DragCancel))
    else {
        eprintln!("[auto] no menuclose/dragcancel after the step: the menu waits for a person (a scripted drag for its 20 s limit)");
        return Vec::new();
    };
    let taken = rest[..=close].to_vec();
    r.idx += close + 1;
    taken
}

/// Queue the in-menu steps and start the first tick. Called just before the event that opens
/// the menu; the ticks run from the menu's own message loop.
fn arm_menu_steps(app: &App, steps: Vec<Step>) {
    MENU_HWND.with(|c| c.set(hwnd_of(app)));
    let any = !steps.is_empty();
    IN_MENU.with(|q| *q.borrow_mut() = steps.into());
    if any {
        MENU_DUE.with(|c| c.set(Some(std::time::Instant::now())));
        // One repeating thread timer (no window) for the whole menu, killed in
        // `disarm_menu_steps`. A one-shot timer re-set from its own callback does not work here:
        // Windows hands the new timer the old id, and the menu loop dispatches the WM_TIMER it
        // already generated for that id again at once, so a `sleep` ended after a few ms.
        // SAFETY: a plain Win32 timer on this thread; `menu_tick` touches no app state.
        let id = unsafe { SetTimer(std::ptr::null_mut(), 0, 10, Some(menu_tick)) };
        MENU_TIMER.with(|c| c.set(id));
    }
}

/// Drive a scripted drag's in-drag steps from a helper thread while `DoDragDrop` runs on the UI
/// thread. OLE's drag loop waits for mouse or keyboard input and dispatches nothing else: no
/// Win32 timer callback (thread or window timer) ran inside it, and with the mouse untouched it
/// never even called the drop source, so a scripted drag sat in `DoDragDrop` until killed. The
/// pump posts a `WM_MOUSEMOVE` (at the real cursor position) to the main window every 20 ms so
/// the loop turns and calls `QueryContinueDrag`, and posts Escape for `dragcancel`. After 20 s
/// without `done` it posts Escape itself.
fn spawn_drag_pump(hwnd: isize, steps: Vec<Step>, done: std::sync::Arc<std::sync::atomic::AtomicBool>) -> std::thread::JoinHandle<()> {
    use std::sync::atomic::Ordering;
    std::thread::spawn(move || {
        let t0 = std::time::Instant::now();
        let mut plan: VecDeque<Step> = steps.into();
        let mut due = Duration::ZERO;
        let mut gave_up = false;
        while !done.load(Ordering::Acquire) {
            let elapsed = t0.elapsed();
            while elapsed >= due {
                let Some(step) = plan.pop_front() else { break };
                eprintln!("[auto] in drag (pump thread, t={} ms): {step:?}", elapsed.as_millis());
                match step {
                    Step::Sleep(ms) => due += Duration::from_millis(ms),
                    // SAFETY: posting to our own window; the message carries no pointers.
                    Step::DragCancel => unsafe {
                        PostMessageW(hwnd as HWND, WM_KEYDOWN, VK_ESCAPE as usize, 1);
                    },
                    _ => {}
                }
            }
            if elapsed > Duration::from_secs(20) && !gave_up {
                gave_up = true;
                eprintln!("[auto] the scripted drag is still running after 20 s: posting Escape");
                // SAFETY: as above.
                unsafe { PostMessageW(hwnd as HWND, WM_KEYDOWN, VK_ESCAPE as usize, 1) };
            }
            let mut p = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
            // SAFETY: plain Win32 queries and a post to our own window (thread-safe calls).
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut p);
                windows_sys::Win32::Graphics::Gdi::ScreenToClient(hwnd as HWND, &mut p);
                let lp = ((p.y as u16 as u32) << 16 | p.x as u16 as u32) as isize;
                PostMessageW(hwnd as HWND, WM_MOUSEMOVE, 0, lp);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if !plan.is_empty() {
            eprintln!("[auto] the drag ended before {} in-drag step(s) ran: {plan:?}", plan.len());
        }
    })
}

/// Run `f` once the drag a `dragrow` asked for is over (it runs from a posted message after the
/// step's dispatch; no Slint timer fires while its loop runs, so this polls after it).
fn when_drag_over(f: Box<dyn FnOnce()>) {
    if crate::drag_pending() {
        Timer::single_shot(Duration::from_millis(20), move || when_drag_over(f));
    } else {
        f();
    }
}

/// Where `dropfiles` aims: the centre of a visible row, or with row -1 the pane's header (in the
/// pane, over no row, so the drop goes to the pane's folder); the centre of a sidebar row; the
/// centre of a tab. None for a sidebar row that does not exist, or a tab when the pane shows no
/// strip.
fn drop_point(app: &App, at: DropAt) -> Option<LogicalPosition> {
    match at {
        DropAt::Row(pane, row) => {
            let (_, _, mid) = geometry(app, pane);
            Some(match usize::try_from(row) {
                Ok(r) => row_pos(app, pane, r),
                Err(_) => LogicalPosition::new(mid.x, list_top(app, pane) - 14.0),
            })
        }
        DropAt::Side(row) => Some(LogicalPosition::new(app.get_sidebar_width() / 2.0, crate::side_row_centre(row)?)),
        DropAt::Tab(pane, index) => {
            let l = crate::window_layout(app);
            if !l.tabs[pane] {
                return None;
            }
            let (left, _) = crate::dnd::pane_x(&l, pane);
            let w = app.invoke_tab_width(pane as i32);
            let x = left + crate::dnd::PANE_PAD + app.invoke_tab_offset(pane as i32) + (index as f32 + 0.5) * w;
            Some(LogicalPosition::new(x, crate::dnd::TAB_TOP + crate::dnd::TAB_H / 2.0))
        }
    }
}

/// The main window's HWND (0 before the window exists).
fn hwnd_of(app: &App) -> isize {
    match app.window().window_handle().window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(w)) => w.hwnd.get(),
        _ => 0,
    }
}

/// The menu closed: stop the timer and report steps that never ran.
fn disarm_menu_steps() {
    let id = MENU_TIMER.with(|c| c.replace(0));
    MENU_DUE.with(|c| c.set(None));
    if id != 0 {
        // SAFETY: a thread timer this thread set.
        unsafe { KillTimer(std::ptr::null_mut(), id) };
    }
    let left = IN_MENU.with(|q| std::mem::take(&mut *q.borrow_mut()));
    if !left.is_empty() {
        eprintln!("[auto] the menu closed before {} in-menu step(s) ran: {left:?}", left.len());
    }
}

/// Post one key to the main window; the menu's loop reads keyboard input from the queue.
fn post_key(vk: u16) {
    let hwnd = MENU_HWND.with(Cell::get) as HWND;
    // SAFETY: posting to our own window; the message carries no pointers.
    unsafe { PostMessageW(hwnd, WM_KEYDOWN, vk as usize, 1) };
}

/// One in-menu tick, every 10 ms from the Win32 timer inside the menu's modal loop: when a step
/// is due, run steps until a `sleep` (which sets the next due time) or the end. Touches no app
/// state; it checks and logs that no borrow of it is held across the loop.
unsafe extern "system" fn menu_tick(_hwnd: HWND, _msg: u32, id: usize, _time: u32) {
    let now = std::time::Instant::now();
    if MENU_DUE.with(Cell::get).is_none_or(|due| now < due) {
        return;
    }
    MENU_DUE.with(|c| c.set(None));
    let runner_free = RUNNER.with(|r| match r.try_borrow() {
        Ok(g) => g.as_ref().is_none_or(|rr| rr.try_borrow_mut().is_ok()),
        Err(_) => false,
    });
    eprintln!(
        "[auto] in-menu tick (timer {id}, t={:.0} ms): menu open={} app state free={} driver free={runner_free}",
        crate::perf::elapsed_ms(*T0.get_or_init(std::time::Instant::now)),
        crate::shellmenu::is_open(),
        crate::state_is_free()
    );
    loop {
        let Some(step) = IN_MENU.with(|q| q.borrow_mut().pop_front()) else { return };
        eprintln!("[auto] in menu: {step:?}");
        match step {
            Step::Sleep(ms) => return MENU_DUE.with(|c| c.set(Some(now + Duration::from_millis(ms)))),
            Step::MenuClose => {
                // SAFETY: ends this thread's active menu; a no-op when none is open.
                unsafe { EndMenu() };
            }
            Step::MenuKey(k) => match k.as_str() {
                "Down" => post_key(VK_DOWN),
                "Up" => post_key(VK_UP),
                "Right" => post_key(VK_RIGHT),
                "Left" => post_key(VK_LEFT),
                "Escape" => post_key(VK_ESCAPE),
                other => eprintln!("[auto] menukey {other}: only Down, Up, Right, Left and Escape are allowed"),
            },
            Step::MenuPick(text) => {
                let labels = crate::shellmenu::item_labels(crate::shellmenu::current_menu());
                // TwinFM's own items, and the drop target's right-button menu (all its items are ours).
                let own = crate::shellmenu::OwnItem::ALL.iter().any(|o| o.label().eq_ignore_ascii_case(&text))
                    || crate::dnd::MENU_LABELS.iter().any(|l| l.eq_ignore_ascii_case(&text));
                match labels.iter().position(|l| l.eq_ignore_ascii_case(&text)) {
                    Some(i) if own => {
                        let downs = labels[..i].iter().filter(|l| *l != "---").count() + 1;
                        eprintln!("[auto] menupick \"{text}\": item {i}, Down x{downs} then Return");
                        for _ in 0..downs {
                            post_key(VK_DOWN);
                        }
                        post_key(VK_RETURN);
                    }
                    _ => {
                        eprintln!("[auto] menupick \"{text}\": not one of TwinFM's own items in {labels:?}; closing instead");
                        // SAFETY: as for MenuClose.
                        unsafe { EndMenu() };
                    }
                }
            }
            Step::MenuOpen(text) => {
                let labels = crate::shellmenu::item_labels(crate::shellmenu::current_menu());
                let found = labels.iter().position(|l| l.trim_end_matches(" >").eq_ignore_ascii_case(&text));
                match found {
                    Some(i) if labels[i].ends_with(" >") => {
                        // Keyboard navigation skips separators only; Down from no selection lands on item 0.
                        let downs = labels[..i].iter().filter(|l| *l != "---").count() + 1;
                        eprintln!("[auto] menuopen \"{text}\": item {i}, Down x{downs} then Right");
                        for _ in 0..downs {
                            post_key(VK_DOWN);
                        }
                        post_key(VK_RIGHT);
                    }
                    Some(_) => eprintln!("[auto] menuopen \"{text}\": that item has no submenu; nothing pressed"),
                    None => eprintln!("[auto] menuopen \"{text}\": no such item in {labels:?}"),
                }
            }
            _ => {}
        }
    }
}

/// The key sequence for a `key` step name (modifiers first).
fn chord(name: &str) -> Vec<SharedString> {
    let k = |k: Key| SharedString::from(k);
    match name {
        "F2" => vec![k(Key::F2)],
        "F3" => vec![k(Key::F3)],
        "F5" => vec![k(Key::F5)],
        "F7" => vec![k(Key::F7)],
        "Backspace" => vec![k(Key::Backspace)],
        "Return" => vec![k(Key::Return)],
        "Escape" => vec![k(Key::Escape)],
        "Delete" => vec![k(Key::Delete)],
        "Space" => vec![" ".into()],
        "AltUp" => vec![k(Key::Alt), k(Key::UpArrow)],
        "AltLeft" => vec![k(Key::Alt), k(Key::LeftArrow)],
        "AltRight" => vec![k(Key::Alt), k(Key::RightArrow)],
        "AltReturn" => vec![k(Key::Alt), k(Key::Return)],
        "CtrlComma" => vec![k(Key::Control), ",".into()],
        "CtrlShiftRight" => vec![k(Key::Control), k(Key::Shift), k(Key::RightArrow)],
        "CtrlH" => vec![k(Key::Control), "h".into()],
        "CtrlR" => vec![k(Key::Control), "r".into()],
        "CtrlA" => vec![k(Key::Control), "a".into()],
        "CtrlC" => vec![k(Key::Control), "c".into()],
        "CtrlX" => vec![k(Key::Control), "x".into()],
        "CtrlV" => vec![k(Key::Control), "v".into()],
        "CtrlZ" => vec![k(Key::Control), "z".into()],
        "CtrlShiftN" => vec![k(Key::Control), k(Key::Shift), "N".into()],
        "CtrlF" => vec![k(Key::Control), "f".into()],
        "CtrlShiftF" => vec![k(Key::Control), k(Key::Shift), "F".into()],
        "Down" => vec![k(Key::DownArrow)],
        "Up" => vec![k(Key::UpArrow)],
        "Left" => vec![k(Key::LeftArrow)],
        "Right" => vec![k(Key::RightArrow)],
        "PageUp" => vec![k(Key::PageUp)],
        "PageDown" => vec![k(Key::PageDown)],
        "Home" => vec![k(Key::Home)],
        "End" => vec![k(Key::End)],
        "ShiftUp" => vec![k(Key::Shift), k(Key::UpArrow)],
        "ShiftDown" => vec![k(Key::Shift), k(Key::DownArrow)],
        "ShiftLeft" => vec![k(Key::Shift), k(Key::LeftArrow)],
        "ShiftRight" => vec![k(Key::Shift), k(Key::RightArrow)],
        "CtrlShift1" => vec![k(Key::Control), k(Key::Shift), "1".into()],
        "CtrlShift2" => vec![k(Key::Control), k(Key::Shift), "2".into()],
        "ShiftF5" => vec![k(Key::Shift), k(Key::F5)],
        "ShiftF6" => vec![k(Key::Shift), k(Key::F6)],
        "ShiftDelete" => vec![k(Key::Shift), k(Key::Delete)],
        "ShiftF10" => vec![k(Key::Shift), k(Key::F10)],
        "Menu" => vec![k(Key::Menu)],
        "CtrlT" => vec![k(Key::Control), "t".into()],
        "CtrlD" => vec![k(Key::Control), "d".into()],
        "F1" => vec![k(Key::F1)],
        "F9" => vec![k(Key::F9)],
        "CtrlW" => vec![k(Key::Control), "w".into()],
        "CtrlL" => vec![k(Key::Control), "l".into()],
        "CtrlTab" => vec![k(Key::Control), k(Key::Tab)],
        "CtrlShiftTab" => vec![k(Key::Control), k(Key::Shift), k(Key::Tab)],
        "CtrlShiftPageUp" => vec![k(Key::Control), k(Key::Shift), k(Key::PageUp)],
        "CtrlShiftPageDown" => vec![k(Key::Control), k(Key::Shift), k(Key::PageDown)],
        n if n.len() == 5 && n.starts_with("Ctrl") && n.as_bytes()[4].is_ascii_digit() && n.as_bytes()[4] != b'0' => {
            vec![k(Key::Control), n[4..].into()]
        }
        // One character typed as itself (into the filter box, say).
        n if n.chars().count() == 1 => vec![n.into()],
        other => {
            eprintln!("[auto] unknown key {other}");
            vec![]
        }
    }
}

/// Press the keys in order and release them in reverse.
fn press(app: &App, keys: &[SharedString]) {
    let win = app.window();
    for t in keys {
        win.dispatch_event_with_result(WindowEvent::KeyPressed { text: t.clone() }).ok();
    }
    for t in keys.iter().rev() {
        win.dispatch_event_with_result(WindowEvent::KeyReleased { text: t.clone() }).ok();
    }
}

fn run_step(runner: Rc<RefCell<Runner>>) {
    let (step, app) = {
        let r = runner.borrow();
        let Some(step) = r.steps.get(r.idx).cloned() else {
            eprintln!("[auto] script finished");
            return;
        };
        let Some(app) = r.app.upgrade() else { return };
        (step, app)
    };
    runner.borrow_mut().idx += 1;
    eprintln!("[auto] {step:?}");
    match step {
        Step::Sleep(ms) => schedule(runner, ms),
        Step::Split => {
            app.invoke_toggle_split();
            schedule(runner, 1);
        }
        Step::Nav { pane, path } => {
            app.invoke_navigate(pane as i32, path.as_str().into());
            schedule(runner, 1);
        }
        Step::Up(pane) => {
            app.invoke_go_up(pane as i32);
            schedule(runner, 1);
        }
        Step::ScrollPos(pane) => {
            let row = if pane == 0 { app.get_left_current_row() } else { app.get_right_current_row() };
            eprintln!("[auto] scrollpos pane={pane} content-y {:.1} current-row {row}", app.invoke_get_scroll(pane as i32));
            schedule(runner, 1);
        }
        Step::Sort { pane, col } => {
            app.invoke_sort_by(pane as i32, col);
            schedule(runner, 1);
        }
        Step::Key(name) if matches!(name.as_str(), "ShiftF10" | "Menu") => {
            // The keyboard menu opens inside the key press, like rclick's inside the release.
            let steps = take_menu_steps(&mut runner.borrow_mut());
            arm_menu_steps(&app, steps);
            press(&app, &chord(&name));
            disarm_menu_steps();
            schedule(runner, 1);
        }
        Step::Key(name) => {
            press(&app, &chord(&name));
            schedule(runner, 1);
        }
        Step::Type { lead, text } => {
            press(&app, &chord(&lead));
            for c in text.chars() {
                press(&app, &[c.to_string().into()]);
            }
            press(&app, &chord("Return"));
            schedule(runner, 1);
        }
        Step::WaitOps(max_ms) => {
            let deadline = std::time::Instant::now() + Duration::from_millis(max_ms);
            let r2 = runner.clone();
            runner.borrow().sub.start(TimerMode::Repeated, Duration::from_millis(50), move || {
                let busy = crate::ops_busy();
                if !busy || std::time::Instant::now() >= deadline {
                    if busy {
                        eprintln!("[auto] waitops: still busy after {max_ms} ms");
                    }
                    r2.borrow().sub.stop();
                    schedule(r2.clone(), 300);
                }
            });
        }
        Step::Click { pane, row, modifier } => {
            let pos = row_pos(&app, pane, row);
            let win = app.window();
            win.dispatch_event_with_result(WindowEvent::PointerMoved { position: pos }).ok();
            if let Some(m) = modifier {
                win.dispatch_event_with_result(WindowEvent::KeyPressed { text: m.into() }).ok();
            }
            win.dispatch_event_with_result(WindowEvent::PointerPressed { position: pos, button: PointerEventButton::Left }).ok();
            win.dispatch_event_with_result(WindowEvent::PointerReleased { position: pos, button: PointerEventButton::Left }).ok();
            if let Some(m) = modifier {
                win.dispatch_event_with_result(WindowEvent::KeyReleased { text: m.into() }).ok();
            }
            // Space consecutive clicks out so the next one is not taken as a double-click.
            schedule(runner, 600);
        }
        Step::DblClick { pane, row } => {
            let pos = row_pos(&app, pane, row);
            let win = app.window();
            win.dispatch_event_with_result(WindowEvent::PointerMoved { position: pos }).ok();
            for _ in 0..2 {
                win.dispatch_event_with_result(WindowEvent::PointerPressed { position: pos, button: PointerEventButton::Left }).ok();
                win.dispatch_event_with_result(WindowEvent::PointerReleased { position: pos, button: PointerEventButton::Left }).ok();
            }
            schedule(runner, 1);
        }
        Step::MidClick { pane, row } => {
            let pos = row_pos(&app, pane, row);
            let win = app.window();
            win.dispatch_event_with_result(WindowEvent::PointerMoved { position: pos }).ok();
            win.dispatch_event_with_result(WindowEvent::PointerPressed { position: pos, button: PointerEventButton::Middle }).ok();
            win.dispatch_event_with_result(WindowEvent::PointerReleased { position: pos, button: PointerEventButton::Middle }).ok();
            schedule(runner, 1);
        }
        Step::XButton { pane, forward } => {
            use windows_sys::Win32::System::SystemServices::{MK_XBUTTON1, MK_XBUTTON2};
            use windows_sys::Win32::UI::WindowsAndMessaging::{WM_XBUTTONDOWN, WM_XBUTTONUP, XBUTTON1, XBUTTON2};
            // Posted, not dispatched: winit's window procedure turns them into the side button's
            // press and release (after the move that places the pointer), as for a real mouse.
            let (_, _, pos) = geometry(&app, pane);
            let s = app.window().scale_factor();
            let (x, y) = ((pos.x * s).round() as i32, (pos.y * s).round() as i32);
            let lp = (((y as u16 as u32) << 16) | x as u16 as u32) as isize;
            let (which, held) = if forward { (XBUTTON2, MK_XBUTTON2) } else { (XBUTTON1, MK_XBUTTON1) };
            let hwnd = hwnd_of(&app) as HWND;
            // SAFETY: posting to our own window; the messages carry no pointers.
            unsafe {
                PostMessageW(hwnd, WM_MOUSEMOVE, 0, lp);
                PostMessageW(hwnd, WM_XBUTTONDOWN, ((which as usize) << 16) | held as usize, lp);
                PostMessageW(hwnd, WM_XBUTTONUP, (which as usize) << 16, lp);
            }
            let name = if forward { "forwardclick" } else { "backclick" };
            eprintln!("[auto] {name} pane={pane}: posted at ({:.0}, {:.0}) logical, ({x}, {y}) physical", pos.x, pos.y);
            // The posted messages are handled when this timer callback returns.
            schedule(runner, 150);
        }
        Step::Crumb { pane, index } => {
            app.invoke_crumb_clicked(pane as i32, index);
            schedule(runner, 1);
        }
        Step::CrumbMenu { pane, index } => {
            // Under the path bar, near the pane's left edge.
            let (left, _) = crate::dnd::pane_x(&crate::window_layout(&app), pane);
            app.invoke_crumb_menu(pane as i32, index, left + 40.0, list_top(&app, pane) - 26.0);
            schedule(runner, 1);
        }
        Step::CrumbPick(name) => {
            match crate::crumb_menu_index(&name) {
                Some(i) => {
                    app.invoke_close_crumb_menu();
                    app.invoke_crumb_pick(i as i32);
                }
                None => eprintln!("[auto] crumbpick: no {name:?} in the dropdown (is it open and listed yet?)"),
            }
            schedule(runner, 1);
        }
        Step::CrumbClose => {
            app.invoke_close_crumb_menu();
            schedule(runner, 1);
        }
        Step::TabDrag { pane, index, dx } => {
            // The strip is the pane's first row: 3 px padding, 26 px tall, every tab tab-width
            // wide from the strip's offset.
            let (left, _) = crate::dnd::pane_x(&crate::window_layout(&app), pane);
            let w = app.invoke_tab_width(pane as i32);
            let x0 = left + 3.0 + app.invoke_tab_offset(pane as i32) + (index as f32 + 0.5) * w;
            let at = |x: f32| LogicalPosition::new(x, 16.0);
            let win = app.window();
            win.dispatch_event_with_result(WindowEvent::PointerMoved { position: at(x0) }).ok();
            win.dispatch_event_with_result(WindowEvent::PointerPressed { position: at(x0), button: PointerEventButton::Left }).ok();
            let steps = (dx.abs() / 8.0).ceil().max(1.0) as i32;
            for k in 1..=steps {
                win.dispatch_event_with_result(WindowEvent::PointerMoved { position: at(x0 + dx * k as f32 / steps as f32) }).ok();
            }
            win.dispatch_event_with_result(WindowEvent::PointerReleased { position: at(x0 + dx), button: PointerEventButton::Left }).ok();
            eprintln!("[auto] tabdrag pane={pane} tab {index} from x {x0:.0} by {dx:.0} (tab width {w:.0})");
            schedule(runner, 1);
        }
        Step::PathMode { pane, entry } => {
            app.invoke_set_path_mode(pane as i32, entry);
            schedule(runner, 1);
        }
        Step::SideClick(row) => {
            app.invoke_side_clicked(row, false);
            schedule(runner, 1);
        }
        Step::BookmarkRemove(i) => {
            app.invoke_bookmark_remove(i);
            schedule(runner, 1);
        }
        Step::BookmarkRename(i, title) => {
            app.invoke_bookmark_rename(i, title.as_str().into());
            schedule(runner, 1);
        }
        Step::BookmarkMove(i, up) => {
            app.invoke_bookmark_move(i, up);
            schedule(runner, 1);
        }
        Step::AboutClose => {
            app.invoke_close_about();
            schedule(runner, 1);
        }
        Step::SettingsClose(ok) => {
            if ok { app.invoke_settings_ok() } else { app.invoke_settings_cancel() }
            schedule(runner, 1);
        }
        Step::SettingsSet(field, value) => {
            app.invoke_settings_stage(field.as_str().into(), value.as_str().into());
            schedule(runner, 1);
        }
        Step::ScriptFile { pane, name, remove } => {
            if name.is_empty() || name.contains(['\\', '/']) {
                eprintln!("[auto] touch/rmfile take a plain file name, not {name:?}");
            } else {
                crate::script_file(pane, &name, remove);
            }
            schedule(runner, 1);
        }
        Step::WaitWatch(max_ms) => {
            let start = crate::watch_refreshes();
            let deadline = std::time::Instant::now() + Duration::from_millis(max_ms);
            let r2 = runner.clone();
            runner.borrow().sub.start(TimerMode::Repeated, Duration::from_millis(50), move || {
                let seen = crate::watch_refreshes() > start;
                if seen || std::time::Instant::now() >= deadline {
                    if !seen {
                        eprintln!("[auto] waitwatch: no watch-driven refresh within {max_ms} ms");
                    }
                    r2.borrow().sub.stop();
                    schedule(r2.clone(), 300);
                }
            });
        }
        Step::Filter { pane, text } => {
            if pane == 0 {
                app.set_left_filter_open(true);
                app.set_left_filter_text(text.as_str().into());
            } else {
                app.set_right_filter_open(true);
                app.set_right_filter_text(text.as_str().into());
            }
            app.invoke_filter_edited(pane as i32, text.as_str().into());
            schedule(runner, 1);
        }
        Step::Search(pane) => {
            app.invoke_search(pane as i32);
            schedule(runner, 1);
        }
        Step::WaitSearch(max_ms) => {
            let deadline = std::time::Instant::now() + Duration::from_millis(max_ms);
            let r2 = runner.clone();
            runner.borrow().sub.start(TimerMode::Repeated, Duration::from_millis(50), move || {
                let running = crate::search_running();
                if !running || std::time::Instant::now() >= deadline {
                    if running {
                        eprintln!("[auto] waitsearch: still searching after {max_ms} ms");
                    }
                    r2.borrow().sub.stop();
                    schedule(r2.clone(), 300);
                }
            });
        }
        Step::SizeAll(pane) => {
            app.invoke_focus_pane(pane as i32);
            press(&app, &chord("AltReturn"));
            schedule(runner, 1);
        }
        Step::WaitSize(max_ms) => {
            let deadline = std::time::Instant::now() + Duration::from_millis(max_ms);
            let r2 = runner.clone();
            runner.borrow().sub.start(TimerMode::Repeated, Duration::from_millis(50), move || {
                let busy = crate::sizes_busy();
                if !busy || std::time::Instant::now() >= deadline {
                    if busy {
                        eprintln!("[auto] waitsize: still sizing after {max_ms} ms");
                    }
                    r2.borrow().sub.stop();
                    schedule(r2.clone(), 300);
                }
            });
        }
        Step::Window(what) => {
            use windows_sys::Win32::UI::WindowsAndMessaging::{SW_MAXIMIZE, SW_RESTORE, ShowWindow, WM_CLOSE};
            let hwnd = hwnd_of(&app) as HWND;
            // SAFETY: our own top-level window, on its thread; WM_CLOSE carries no pointers.
            match what.as_str() {
                "max" => unsafe { ShowWindow(hwnd, SW_MAXIMIZE) },
                "restore" => unsafe { ShowWindow(hwnd, SW_RESTORE) },
                "close" => unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) },
                other => {
                    eprintln!("[auto] window {other}: only max, restore and close");
                    0
                }
            };
            schedule(runner, 1);
        }
        Step::Quit => {
            let _ = slint::quit_event_loop();
        }
        Step::MenuOpen(_)
        | Step::MenuKey(_)
        | Step::MenuPick(_)
        | Step::MenuClose
        | Step::DragCancel => {
            eprintln!("[auto] {step:?} outside a menu or drag (it belongs after rclick, rclickempty or dragrow); skipped");
            schedule(runner, 1);
        }
        Step::DragRow { pane, row, dx, dy, right } => {
            let hwnd = hwnd_of(&app);
            let mut drag_steps = take_menu_steps(&mut runner.borrow_mut());
            let pos = row_pos(&app, pane, row);
            let to = LogicalPosition::new(pos.x + dx, pos.y + dy);
            let button = if right { PointerEventButton::Right } else { PointerEventButton::Left };
            let win = app.window();
            win.dispatch_event_with_result(WindowEvent::PointerMoved { position: pos }).ok();
            win.dispatch_event_with_result(WindowEvent::PointerPressed { position: pos, button }).ok();
            // Move after a frame, so the pressed row is drawn selected before the drag loop.
            let r2 = runner.clone();
            let w = app.as_weak();
            runner.borrow().sub.start(TimerMode::SingleShot, Duration::from_millis(60), move || {
                let Some(app) = w.upgrade() else { return };
                let before = crate::dnd::drags_started();
                let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                let pump = spawn_drag_pump(hwnd, std::mem::take(&mut drag_steps), done.clone());
                crate::dnd::script_begin();
                // The move asks for the drag; it runs from a posted message right after this
                // dispatch (main.rs `run_pending_drag`), so wait until it is over.
                app.window().dispatch_event_with_result(WindowEvent::PointerMoved { position: to }).ok();
                let w = app.as_weak();
                let r2 = r2.clone();
                when_drag_over(Box::new(move || {
                    crate::dnd::script_end();
                    done.store(true, std::sync::atomic::Ordering::Release);
                    let _ = pump.join();
                    let Some(app) = w.upgrade() else { return };
                    if crate::dnd::drags_started() == before {
                        if right {
                            // A right release would open the row's context menu and wait for a
                            // person; leaving the window ends Slint's grab instead.
                            eprintln!("[auto] dragrow: no right-button drag started (under the threshold?); pointer leaves the window");
                            app.window().dispatch_event_with_result(WindowEvent::PointerExited).ok();
                        } else {
                            eprintln!("[auto] dragrow: no drag started (under the threshold?); releasing");
                            app.window().dispatch_event_with_result(WindowEvent::PointerReleased { position: to, button }).ok();
                        }
                    }
                    // After a drag the app gives Slint the release it missed (main.rs).
                    schedule(r2, 1);
                }));
            });
        }
        Step::DragOver { pane, top, ms, paths } => {
            // A drag hovering 6 px inside the list's top or bottom edge: DragEnter, a DragOver every
            // 20 ms from a Slint timer (OLE calls DragOver on its own timer for another program's
            // drag, outside any modal loop of ours), then DragLeave. It never drops.
            let l = crate::window_layout(&app);
            let (_, _, mid) = geometry(&app, pane);
            let y = if top { crate::dnd::list_top(&l, pane) + 6.0 } else { l.height - crate::dnd::LIST_BOTTOM_GAP - 6.0 };
            let (sx, sy) = crate::shellmenu::to_screen(hwnd_of(&app), mid.x, y, app.window().scale_factor());
            let paths: Vec<std::path::PathBuf> = paths.iter().map(std::path::PathBuf::from).collect();
            if !crate::dnd::hover_begin(&paths, sx, sy) {
                eprintln!("[auto] dragover: no drop target registered yet");
                schedule(runner, 1);
                return;
            }
            let deadline = std::time::Instant::now() + Duration::from_millis(ms);
            let r2 = runner.clone();
            runner.borrow().sub.start(TimerMode::Repeated, Duration::from_millis(20), move || {
                if std::time::Instant::now() >= deadline {
                    crate::dnd::hover_end();
                    r2.borrow().sub.stop();
                    schedule(r2.clone(), 1);
                } else {
                    crate::dnd::hover_over(sx, sy);
                }
            });
        }
        Step::DropFiles { at, mode, pick, paths } => {
            let Some(pos) = drop_point(&app, at) else {
                eprintln!("[auto] dropfiles {at:?}: no such sidebar row or tab on screen");
                schedule(runner, 1);
                return;
            };
            let (sx, sy) = crate::shellmenu::to_screen(hwnd_of(&app), pos.x, pos.y, app.window().scale_factor());
            let keys = match mode.as_str() {
                "copy" => MK_CONTROL,
                "move" => MK_SHIFT,
                "menu" => MK_RBUTTON,
                _ => 0,
            };
            // A right-button drop opens the drop menu inside Drop; its item is picked from the
            // in-menu timer, as `menupick` does in a context menu.
            let label = match pick.as_str() {
                "move" => crate::dnd::MENU_LABELS[0],
                "copy" => crate::dnd::MENU_LABELS[1],
                _ => crate::dnd::MENU_LABELS[2],
            };
            if mode == "menu" {
                arm_menu_steps(&app, vec![Step::Sleep(300), Step::MenuPick(label.into())]);
            }
            let paths: Vec<std::path::PathBuf> = paths.iter().map(std::path::PathBuf::from).collect();
            let result = crate::dnd::drop_direct(&paths, keys, sx, sy);
            if mode == "menu" {
                disarm_menu_steps();
            }
            match result {
                Some((enter, dropped)) => eprintln!(
                    "[auto] dropfiles {at:?} {mode}{}: {} path(s), DragEnter effect {}, Drop reported {}",
                    if mode == "menu" { format!(" {pick}") } else { String::new() },
                    paths.len(),
                    crate::dnd::effect_name(enter),
                    crate::dnd::effect_name(dropped)
                ),
                None => eprintln!("[auto] dropfiles: no drop target registered yet"),
            }
            schedule(runner, 1);
        }
        Step::DropVirtual { pane, row, zip, own, idlist, asynced, items } => {
            let Some(pos) = drop_point(&app, DropAt::Row(pane, row)) else {
                schedule(runner, 1);
                return;
            };
            let (sx, sy) = crate::shellmenu::to_screen(hwnd_of(&app), pos.x, pos.y, app.window().scale_factor());
            let data = if zip {
                // Paths inside one folder of an archive: the zip folder's own data object, as a
                // drag out of a zip pane carries it.
                let paths: Vec<std::path::PathBuf> = items.iter().map(std::path::PathBuf::from).collect();
                let folder = paths.first().and_then(|p| p.parent()).map(std::path::Path::to_path_buf).unwrap_or_default();
                let names: Vec<String> = paths.iter().filter_map(|p| Some(p.file_name()?.to_string_lossy().into_owned())).collect();
                if idlist { crate::dnd::idlist_data(hwnd_of(&app), &folder, &names) } else { crate::dnd::archive_data(hwnd_of(&app), &folder, &names) }
            } else {
                // Generated contents: `name=size` bytes of a pattern; `name\` a folder.
                let listed: Vec<(String, Option<Vec<u8>>)> = items
                    .iter()
                    .enumerate()
                    .map(|(k, it)| match it.rsplit_once('=') {
                        Some((n, size)) => (n.to_string(), Some((0..size.parse::<usize>().unwrap_or(0)).map(|i| (i * 7 + k) as u8).collect())),
                        None => (it.trim_end_matches('\\').to_string(), None),
                    })
                    .collect();
                Ok(crate::dnd::virtual_data(&listed, false, None, asynced))
            };
            let own_paths: Vec<std::path::PathBuf> = items.iter().map(std::path::PathBuf::from).collect();
            let dropped = data.map(|d| if own { crate::dnd::drop_as_own_archive_drag(&d, own_paths, 0, sx, sy) } else { crate::dnd::drop_data(&d, 0, sx, sy) });
            match dropped {
                Ok(Some((enter, dropped))) => eprintln!(
                    "[auto] dropvirtual pane {pane} row {row}{}: {} item(s), DragEnter effect {}, Drop reported {}",
                    if own {
                        " (the zip folder's data object, as our own drag)"
                    } else if idlist {
                        " (SHCreateDataObject: a Shell IDList Array only)"
                    } else if zip { " (the zip folder's data object)" } else if asynced { " (async source)" } else { "" },
                    items.len(),
                    crate::dnd::effect_name(enter),
                    crate::dnd::effect_name(dropped)
                ),
                Ok(None) => eprintln!("[auto] dropvirtual: no drop target registered yet"),
                Err(e) => eprintln!("[auto] dropvirtual: {e}"),
            }
            schedule(runner, 1);
        }
        Step::RClick { pane, row } => {
            let mut menu_steps = take_menu_steps(&mut runner.borrow_mut());
            let target = match row {
                Some(r) => Some(row_pos(&app, pane, r)),
                None => empty_pos(&app, pane),
            };
            let Some(pos) = target else {
                // The rows fill the list: no empty space to click. Same callback the empty
                // space's TouchArea fires, called directly.
                eprintln!("[auto] rclickempty: no empty space below the last row; calling context-menu(-1) directly");
                let (_, _, mid) = geometry(&app, pane);
                // What the empty space's right press does first: clear the selection.
                app.invoke_select(pane as i32, 5, -1);
                arm_menu_steps(&app, menu_steps);
                app.invoke_context_menu(pane as i32, -1, mid.x, list_top(&app, pane) + crate::window_layout(&app).row_px_of(pane) / 2.0, false);
                disarm_menu_steps();
                schedule(runner, 1);
                return;
            };
            let win = app.window();
            win.dispatch_event_with_result(WindowEvent::PointerMoved { position: pos }).ok();
            win.dispatch_event_with_result(WindowEvent::PointerPressed { position: pos, button: PointerEventButton::Right }).ok();
            // Release after a frame, so the right-pressed row is drawn selected before the menu's
            // modal loop starts (nothing renders while it runs).
            let r2 = runner.clone();
            let w = app.as_weak();
            runner.borrow().sub.start(TimerMode::SingleShot, Duration::from_millis(60), move || {
                let Some(app) = w.upgrade() else { return };
                arm_menu_steps(&app, std::mem::take(&mut menu_steps));
                // The menu opens and runs inside this dispatch; it returns when the menu closes.
                app.window()
                    .dispatch_event_with_result(WindowEvent::PointerReleased { position: pos, button: PointerEventButton::Right })
                    .ok();
                disarm_menu_steps();
                schedule(r2.clone(), 1);
            });
        }
        Step::Wheel { pane, ticks, ms } => {
            let (_, _, pos) = geometry(&app, pane);
            app.window().dispatch_event_with_result(WindowEvent::PointerMoved { position: pos }).ok();
            let remaining = Rc::new(RefCell::new(ticks.abs()));
            let delta = if ticks < 0 { -120.0 } else { 120.0 };
            let r2 = runner.clone();
            let w = app.as_weak();
            runner.borrow().sub.start(TimerMode::Repeated, Duration::from_millis(ms), move || {
                let Some(app) = w.upgrade() else { return };
                let mut n = remaining.borrow_mut();
                if *n == 0 {
                    r2.borrow().sub.stop();
                    schedule(r2.clone(), 1);
                    return;
                }
                *n -= 1;
                app.window()
                    .dispatch_event_with_result(WindowEvent::PointerScrolled { position: pos, delta_x: 0.0, delta_y: delta })
                    .ok();
            });
        }
        Step::Drag { pane, dy, ms } => {
            let start = thumb(&app, pane);
            let win = app.window();
            win.dispatch_event_with_result(WindowEvent::PointerMoved { position: start }).ok();
            win.dispatch_event_with_result(WindowEvent::PointerPressed { position: start, button: PointerEventButton::Left }).ok();
            let steps = (ms / 16).max(1) as i32;
            let i = Rc::new(RefCell::new(0i32));
            let r2 = runner.clone();
            let w = app.as_weak();
            runner.borrow().sub.start(TimerMode::Repeated, Duration::from_millis(16), move || {
                let Some(app) = w.upgrade() else { return };
                let mut k = i.borrow_mut();
                *k += 1;
                let pos = LogicalPosition::new(start.x, start.y + dy * (*k as f32) / steps as f32);
                if *k >= steps {
                    app.window().dispatch_event_with_result(WindowEvent::PointerMoved { position: pos }).ok();
                    app.window()
                        .dispatch_event_with_result(WindowEvent::PointerReleased { position: pos, button: PointerEventButton::Left })
                        .ok();
                    r2.borrow().sub.stop();
                    schedule(r2.clone(), 1);
                } else {
                    app.window().dispatch_event_with_result(WindowEvent::PointerMoved { position: pos }).ok();
                }
            });
        }
    }
}
