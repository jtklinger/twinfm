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
//!                                    CtrlV, CtrlZ, CtrlY, CtrlShiftN, Escape, ShiftF10, Menu (the last two open
//!                                    the Windows 11 menu under the cursor row, as rclick does); CtrlT,
//!                                    CtrlW, Tab, ShiftTab, CtrlTab, CtrlShiftTab, CtrlL, Ctrl1 to Ctrl9, CtrlD, F1, F9,
//!                                    CtrlShiftPageUp, CtrlShiftPageDown, AltLeft, AltRight (back
//!                                    and forward in the tab's history), CtrlComma (settings),
//!                                    CtrlF (the filter box), CtrlShiftF (search), AltReturn
//!                                    (Properties, P7-M2), AltShiftReturn (size every folder in the
//!                                    listing), and any single character, typed as itself (`key a`)
//!   copy | move                      Shift+F5 / Shift+F6: the active pane's selection to the other pane
//!   delete | sdelete                 Delete (Recycle Bin) / Shift+Delete (permanent)
//!   cc | cx | cv                     Ctrl+C / Ctrl+X / Ctrl+V through the real clipboard
//!   undo                             Ctrl+Z: undo the newest recorded operation (fileops::undo)
//!   redo                             Ctrl+Y: redo the newest undone operation
//!   newfolder <name>                 F7, type the name into the editor, Return
//!   newfile <name>                   Ctrl+Alt+N (key name CtrlAltN), type the name, Return: an
//!                                    empty file (P6-M3)
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
//!   backclick <pane> path|edit       the Back button over the path bar's middle (with `pathmode
//!                                    <pane> entry`, the text entry) or over row 0's name cell (with F2,
//!                                    the rename editor): ignored there (P6-M3)
//!   backclick <pane> tool            the Back button over the toolbar's first button (P7-M4): the bar
//!                                    across the window (the active pane goes back) or, with
//!                                    `toolbar = "pane"`, the pane's own bar; `forwardclick` the same
//!   hover <x> <y>                    move the pointer to (x, y), logical px of the window, through
//!                                    Slint's pointer path (a toolbar button's tooltip shows after
//!                                    500 ms: `[toolbar] top|pane=N tooltip "<name> (<key>)"`)
//!   clickat <x> <y> [right]          press and release the left (or right) button at (x, y), logical
//!                                    px of the window, through Slint's pointer path (P12-M5: TwinFM's
//!                                    own popups, such as the sidebar headers' menus)
//!   history <back|forward>           Alt+Left / Alt+Right through the Slint key path, on the active pane
//!   view <details|icons>             Ctrl+Shift+1 / Ctrl+Shift+2 (key names CtrlShift1, CtrlShift2): the
//!                                    active pane's view, the details list or the large-icon grid
//!                                    (P4-M1); `[view]` lines on stderr. Key names Up, Down, Left,
//!                                    Right, PageUp, PageDown, Home, End (and ShiftLeft, ShiftRight)
//!                                    move the cursor in the focused list
//!   rclick <pane> <row> [shift]      right-click a visible row: press, 60 ms for a frame, release;
//!                                    the release opens the Windows 11 style menu (P7-M2), which
//!                                    shows when the verb worker answers or after 250 ms (`waitmenu`);
//!                                    with `shift` (Shift held over the release) the classic shell
//!                                    menu opens at once, inside the release, and the step takes the
//!                                    in-menu steps after it (below)
//!   rclickempty <pane> [shift]       right-click the empty space below the last row (the folder's
//!                                    background menu); when the rows fill the list there is none,
//!                                    and the step calls the pane's context-menu callback directly
//!   waitmenu <ms>                    wait until the Windows 11 menu is shown and the verb worker has
//!                                    answered (at most <ms>), then 100 ms
//!   menuopen <text>                  in the Windows 11 menu: open the submenu of the item reading
//!                                    <text> ("Open with", "Compress to"); "Show more options" opens
//!                                    the classic menu and takes the in-menu steps after it. Inside
//!                                    the classic menu: open the submenu whose item reads <text>
//!                                    ("Send to"), with Down presses then Right; never Return
//!   menukey <Down|Up|Right|Left|Escape|Return|Home|End|ShiftF10>
//!                                    one key in the menu (Return, Home, End and ShiftF10 in the
//!                                    Windows 11 menu only)
//!   menupick <text>                  in the Windows 11 menu: pick the item (or icon-row button)
//!                                    reading <text>; one that runs on the verb worker (Share,
//!                                    Properties, Open with's apps, the packaged apps' commands) is
//!                                    refused and the menu closes, except Compress to > "ZIP file"
//!                                    inside the TWINFM_SOAK fence. Inside the classic menu: one of
//!                                    TwinFM's own items ("Copy path", "Open in other pane", "Open in
//!                                    terminal", and the drop menu's "Move here", "Copy here",
//!                                    "Cancel"); refuses shell items
//!   menuclose                        close the Windows 11 menu, or inside the classic menu: EndMenu
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
//!                                    `tab <pane> <index>` at tab <index> of the pane's strip, `crumb
//!                                    <pane> <index>` at the label of crumb <index> of the pane's
//!                                    breadcrumb (0 is This PC; -1 the folder itself, -2 its
//!                                    parent); `sidegap <index>` (P12-M5) at the line before bookmark
//!                                    <index> (the count: below the last), the Bookmarks gap, where a
//!                                    drop bookmarks the folders; `point <x> <y>` at that window point
//!                                    (logical px), whatever the hit test finds there. copy holds
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
//!                                    DragLeave): the auto-scroll near the edges. It never drops.
//!                                    `side <top|bottom>` hovers 6 px inside the sidebar list's top
//!                                    or bottom and `strip <pane> <left|right>` 6 px inside a tab
//!                                    strip's end (their auto-scroll); `row <pane> <row>`, `tab
//!                                    <pane> <index>` and `crumb <pane> <index>` hover the centre of
//!                                    that spot, as `dropfiles` aims at it (held 1000 ms or more
//!                                    over a folder row, a tile or a tab, the spring-load opens it)
//!   tab <new|close|next|prev|N>      Ctrl+T, Ctrl+W, Ctrl+Tab, Ctrl+Shift+Tab or Ctrl+N (1..9) through the
//!                                    Slint key path, on the active pane
//!   tab <moveleft|moveright>         Ctrl+Shift+PageUp / Ctrl+Shift+PageDown: move the active tab
//!   tabdrag <pane> <index> <dx>      press the left button on tab <index> (0-based), move <dx> px
//!                                    in 8 px steps, release: a tab drag through Slint's pointer path
//!   colwidth <pane> <type|size|modified> <px>
//!                                    set a column's width as a grip drag's end does (P9-M1): the
//!                                    boundary at the column's left edge moves, its left neighbour
//!                                    (Name for type) taking the difference
//!   coldrag <pane> <type|size|modified> <dx>
//!                                    press on that grip's centre in the header (the boundary at the
//!                                    column's left edge), move <dx> px in 8 px steps, release,
//!                                    through Slint's pointer path
//!   colfit <pane> <type|size|modified>
//!                                    double-click that grip: the column fitted to its cells on screen
//!   band <pane> <x0> <y0> <x1> <y1> [ctrl|shift] [hold <ms>]
//!                                    press the left button at (x0, y0) of the pane's list (px from
//!                                    its top-left corner; aim at empty grid space), move to (x1, y1)
//!                                    in 8 px steps, release: a rubber band through Slint's pointer
//!                                    path (P4-M3), with Ctrl or Shift held through it; `hold`
//!                                    keeps the button down at (x1, y1) for <ms> before the release
//!                                    and runs the next steps meanwhile (the edge auto-scroll runs,
//!                                    a `key Escape` cancels the band; no waitops/waitwatch inside)
//!   crumb <pane> <index>             breadcrumb button <index> (0 is This PC), through its callback
//!   toolbar <pane> <button>          a toolbar button (P6-M3: newfile, newfolder, cut, copy, paste,
//!                                    rename, delete) through the callback a press calls, which makes
//!                                    the pane active (`[toolbar]` lines);
//!                                    pressed with the toolbar hidden too, since a button only calls
//!                                    its key's command; a disabled button is not pressed (logged)
//!   menuverb <pane> <verb>           as if the shell command with canonical verb <verb> had been
//!                                    picked in the context menu for the cursor row's selection (the
//!                                    background with no cursor row; P7-M1): delete, cut, copy, paste
//!                                    and rename go to TwinFM's own commands, any other verb to the
//!                                    verb worker by its verb (`copy` never does: use a handler verb
//!                                    such as `link`, Create shortcut); `sleep:<ms>` is a test verb that sleeps
//!                                    on the worker; `try:<label path>` (P12-M6) is a dry test verb: the
//!                                    classic menu's command at that path (`Offline access>Available
//!                                    offline`), handed to the worker as a pick would be, resolved there
//!                                    and never invoked; `waitops` waits for the worker too
//!   zippassword <text>               the answer to the next password question of a job writing
//!                                    into a password-protected zip (P8-L2), in place of the
//!                                    dialog; a scripted job with none is refused, never shown it
//!   crumbmenu <pane> <index>         the chevron after crumb <index>: its subfolders in a dropdown
//!                                    (listed on a worker; sleep before crumbpick)
//!   crumbpick <name>                 pick the dropdown's folder <name> (the pane navigates there)
//!   crumbclose                       close the dropdown (as Escape does)
//!   pathmode <pane> <entry|crumb>    the path bar as text entry (as Ctrl+L) or back to the breadcrumb
//!   sidebar toggle                   F9: show or hide the sidebar
//!   sidebar click <row>              sidebar row <row> (headers count; 0 is "Drives"; -1 the last row), as a left click: the
//!                                    sidebar takes the keys (P6-M3: `key F2` then opens a bookmark's
//!                                    title editor, `key Escape` gives the keys back to the list)
//!   bookmark add                     Ctrl+D: bookmark the active tab's folder
//!   bookmark remove <index>          the bookmark menu's "Remove bookmark" for bookmark <index>
//!   bookmark rename <index> <title>  what the bookmark row's title editor commits (an empty title
//!                                    goes back to the folder's name)
//!   bookmark move <index> <up|down>  the bookmark menu's "Move up" / "Move down"
//!   bookmark drag <from> <to>        drag bookmark <from> so it lands at index <to> (P12-M5): a press
//!                                    on its row, moves in 4 px steps to the line where it lands and a
//!                                    release, through Slint's pointer path (the sidebar scrolled to its
//!                                    end first, where the bookmarks are)
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
//!   mvfile <pane> <old> <new>        rename a file in the pane's folder (plain names without
//!                                    spaces; the same fence as touch and rmfile)
//!   waitwatch <ms>                   wait until the next refresh started by the directory watch has
//!                                    listed its folder, or the next watch window has been applied
//!                                    as row edits (at most <ms>), then 300 ms more for the pass
//!   waitedit <ms>                    wait until the next watch window has been applied as row edits
//!                                    (not a refresh: a script asserts the edit path), at most <ms>,
//!                                    then 300 ms more
//!   filter <pane> <text>             put <text> (the rest of the step, may be empty) in the pane's
//!                                    filter box, opening it without the focus, and call the box's
//!                                    `edited` callback, as typing does (applied 150 ms later)
//!   search <pane>                    Ctrl+Shift+F's callback: a recursive name search for the
//!                                    filter box's text in a new tab (an empty box opens, focused)
//!   waitsearch <ms>                  wait until no search tab is still walking (at most <ms>), then
//!                                    300 ms more for the pass
//!   sizeall <pane>                   Alt+Shift+Return in the pane's list (focused first): size every folder
//!                                    of the listing (at most 500, nearest the cursor first)
//!   waitsize <ms>                    wait until no folder size is queued or walking and the last
//!                                    reply has landed (at most <ms>), then 300 ms more
//!   preview toggle                   Alt+P (key name AltP): show or hide the preview column (P8-M1)
//!   preview width <ratio>            set the column's share of the space right of the sidebar (0.2
//!                                    to 0.6, clamped), as its handle's drag does
//!   previewscroll <px>               scroll the column to <px> from its top
//!   wheelat <x> <y> <dy> [shift]     turn the wheel by <dy> logical px (-60 is one notch down) at
//!                                    (x, y), logical px of the window, through Slint's pointer
//!                                    path, with Shift held (a Shift key event either side) given
//!                                    `shift`; logs the preview's scroll top before and 300 ms after
//!   waitpreview <ms>                 wait until the cursor file's preview has settled (no settle,
//!                                    width or anchor timer runs, and a message shows, or every page
//!                                    kept holds its image at the current width, or a Markdown file's
//!                                    blocks and every picture have come), at most <ms>, then 100 ms
//!   previewlink <dest>               the Markdown preview's link to <dest> (the rest of the step), as
//!                                    a click on it: logged; an anchor, a folder and a program run
//!                                    (scroll, navigate, show), a browser or document link is never
//!                                    launched in a scripted run (P8-M2)
//!   previewzoom <in|out|reset>       a PDF's zoom one step larger or smaller, or back to 100%, as
//!                                    Ctrl+wheel over the column does (P11-M4)
//!   pdfpassword <text>               the answer to the next PDF password question, then the column's
//!                                    "Enter password…" button pressed (P11-M6); a scripted run never
//!                                    shows the dialog, so a question without it is refused
//!   waitquestion <ms>                wait until one of TwinFM's own questions shows (P12-M3: a zip's
//!                                    replace or delete question, "Open N items at once?"), at most
//!                                    <ms>, then 100 ms; a scripted run's question left 30 s is
//!                                    answered Cancel
//!   answer <button>                  press the shown question's button reading <button> (any case)
//!   previewcopy <block|all>          the "Copy text" menu's pick: block <block> (0-based, a code
//!                                    block or table whole) or every block onto the clipboard (P11-M4)
//!   window <max|restore|close>       maximise or restore the window (ShowWindow), or close it as
//!                                    the close button does (WM_CLOSE: settings saved, app exits)
//!   quit                             exit the event loop
//! Events are dispatched with slint::Window::dispatch_event on the UI thread, so Slint's full
//! input -> layout -> render path runs; only the OS WndProc hop is skipped.
//!
//! In-menu steps. The Windows 11 style menu (P7-M2) is a Slint popup with no modal loop: its steps
//! run as any other step. The classic shell menu's `TrackPopupMenuEx` (Shift+right-click, `rclick
//! ... shift`, or "Show more options") runs a modal loop inside the event that opens it, and while
//! it runs no Slint timer fires: winit buffers every event that reaches it
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

/// Where a `dropfiles` lands: a pane's row (-1: the pane's folder), a sidebar row, a tab, or a
/// crumb of the pane's breadcrumb (a negative index counts from the end).
#[derive(Debug, Clone, Copy)]
enum DropAt {
    Row(usize, i32),
    Side(i32),
    /// The Bookmarks gap before bookmark `.0` (the count: after the last), P12-M5.
    SideGap(usize),
    Tab(usize, usize),
    Crumb(usize, i32),
    /// A window point, logical px: whatever the hit test finds there.
    Point(f32, f32),
}

/// Where a `dragover` hovers: 6 px inside a pane list's top (true) or bottom edge, a `dropfiles`
/// spot's centre, 6 px inside the sidebar list's top or bottom, or 6 px inside a tab strip's left
/// (false) or right end.
#[derive(Debug, Clone, Copy)]
enum HoverAt {
    Edge(usize, bool),
    At(DropAt),
    Side(bool),
    Strip(usize, bool),
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
    /// `at`: None the list's middle; "path" the path bar's middle; "edit" row 0's name cell.
    XButton { pane: usize, forward: bool, at: Option<String> },
    Hover { x: f32, y: f32 },
    /// A press and release of the left (or right) button at (x, y), logical px of the window.
    ClickAt { x: f32, y: f32, right: bool },
    WheelAt { x: f32, y: f32, dy: f32, shift: bool },
    /// Press `lead`, type `text` into whatever then has focus, press Return.
    Type { lead: String, text: String },
    WaitOps(u64),
    /// Right-click a row, or with `row: None` the empty space below the rows.
    RClick { pane: usize, row: Option<usize>, shift: bool },
    /// Wait until the Windows 11 menu is shown and settled (at most `ms`), then 100 ms.
    WaitMenu(u64),
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
    /// Hover a drag of `paths` at `at` for `ms`, then leave.
    DragOver { at: HoverAt, ms: u64, paths: Vec<String> },
    Crumb { pane: usize, index: i32 },
    Toolbar { pane: usize, button: String },
    MenuVerb { pane: usize, verb: String },
    ZipPassword(String),
    CrumbMenu { pane: usize, index: i32 },
    CrumbPick(String),
    CrumbClose,
    /// Press the left button on tab `index`'s centre, move `dx` px in 8 px steps, release.
    TabDrag { pane: usize, index: usize, dx: f32 },
    /// `colwidth`, `coldrag` and `colfit` (P9-M1): `what` is the width, the drag's dx, or None for
    /// the fit's double-click.
    Col { pane: usize, col: String, what: Option<f32>, drag: bool },
    Band { pane: usize, from: (f32, f32), to: (f32, f32), modifier: Option<Key>, hold_ms: u64 },
    PathMode { pane: usize, entry: bool },
    SideClick(i32),
    BookmarkRemove(i32),
    BookmarkRename(i32, String),
    /// Bookmark index, and up (true) or down.
    BookmarkMove(i32, bool),
    /// Drag bookmark `from` so it lands at index `to` (P12-M5).
    BookmarkDrag(usize, usize),
    AboutClose,
    /// The settings dialog's OK (true) or Cancel button.
    SettingsClose(bool),
    /// Stage `value` for the dialog's `field`.
    SettingsSet(String, String),
    /// `touch` (create or update) or `rmfile` (delete) a file in the pane's folder.
    ScriptFile { pane: usize, name: String, remove: bool },
    /// `mvfile`: rename a file in the pane's folder.
    ScriptMove { pane: usize, old: String, new: String },
    /// `waitwatch` (a watch-driven refresh or an applied window) or `waitedit` (`edits_only`).
    WaitWatch(u64, bool),
    /// Set the pane's filter box to `text` (opening it, without the focus) and call its `edited`
    /// callback, as typing does.
    Filter { pane: usize, text: String },
    /// The pane's history menu, as Alt+Down opens it (P11-M3).
    HistMenu(usize),
    /// The open history menu's entry at that position (0: the top), as a click picks it.
    HistPick(usize),
    /// Ctrl+Shift+F's callback on the pane.
    Search(usize),
    /// Return in the pane's search box, with the box's text (P10-M2).
    SearchReturn(usize),
    /// The pane's search button (P10-M2).
    SearchButton(usize),
    /// The search box's content toggle set on or off, as its click does (P11-M1).
    ContentMode { pane: usize, on: bool },
    /// The content toggle on, `text` in the box, then Ctrl+Shift+F's callback (P11-M1).
    ContentSearch { pane: usize, text: String },
    /// Wait until no search tab is still walking (at most `ms`), then 300 ms for the pass.
    WaitSearch(u64),
    /// Alt+Shift+Return in the pane's list.
    SizeAll(usize),
    /// Wait until no folder size is queued or walking (at most `ms`), then 300 ms.
    WaitSize(u64),
    /// The preview column's share (`preview width`).
    PreviewWidth(f32),
    /// Scroll the preview column to this many px from its top.
    PreviewScroll(f32),
    /// Wait until the preview has settled (at most `ms`), then 100 ms.
    WaitPreview(u64),
    /// A link in the Markdown preview, as clicked.
    PreviewLink(String),
    /// A PDF's zoom step: 1 in, -1 out, 0 back to 100%.
    PreviewZoom(i32),
    /// A PDF's password, given and the button pressed.
    PdfPassword(String),
    /// "Copy text": a block, or None for all.
    PreviewCopy(Option<usize>),
    /// Wait until a question shows (at most `ms`), then 100 ms (P12-M3).
    WaitQuestion(u64),
    /// The "+" after the pane's tab strip, as its click (P12-M5).
    TabPlus(usize),
    /// Press the shown question's button with this label (P12-M3).
    Answer(String),
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
                "hover" => Step::Hover { x: a.get(1).and_then(|v| v.parse().ok()).unwrap_or(0.0), y: a.get(2).and_then(|v| v.parse().ok()).unwrap_or(0.0) },
                "clickat" => Step::ClickAt {
                    x: a.get(1).and_then(|v| v.parse().ok()).unwrap_or(0.0),
                    y: a.get(2).and_then(|v| v.parse().ok()).unwrap_or(0.0),
                    right: a.get(3) == Some(&"right"),
                },
                "wheelat" => Step::WheelAt {
                    x: a.get(1).and_then(|v| v.parse().ok()).unwrap_or(0.0),
                    y: a.get(2).and_then(|v| v.parse().ok()).unwrap_or(0.0),
                    dy: a.get(3).and_then(|v| v.parse().ok()).unwrap_or(0.0),
                    shift: a.get(4) == Some(&"shift"),
                },
                "backclick" => Step::XButton { pane: num(1) as usize, forward: false, at: a.get(2).map(|s| s.to_string()) },
                "forwardclick" => Step::XButton { pane: num(1) as usize, forward: true, at: a.get(2).map(|s| s.to_string()) },
                "history" => Step::Key(if a.get(1) == Some(&"forward") { "AltRight" } else { "AltLeft" }.into()),
                "view" => Step::Key(
                    match a.get(1).copied() {
                        Some("icons") => "CtrlShift2",
                        Some("medium") => "CtrlShift3",
                        _ => "CtrlShift1",
                    }
                    .into(),
                ),
                "copy" => Step::Key("ShiftF5".into()),
                "move" => Step::Key("ShiftF6".into()),
                "delete" => Step::Key("Delete".into()),
                "sdelete" => Step::Key("ShiftDelete".into()),
                "cc" => Step::Key("CtrlC".into()),
                "cx" => Step::Key("CtrlX".into()),
                "cv" => Step::Key("CtrlV".into()),
                "undo" => Step::Key("CtrlZ".into()),
                "redo" => Step::Key("CtrlY".into()),
                "newfolder" => Step::Type { lead: "F7".into(), text: a[1..].join(" ") },
                "newfile" => Step::Type { lead: "CtrlAltN".into(), text: a[1..].join(" ") },
                "rename" => Step::Type { lead: "F2".into(), text: a[1..].join(" ") },
                "waitops" => Step::WaitOps(num(1).max(1) as u64),
                "rclick" => Step::RClick { pane: num(1) as usize, row: Some(num(2) as usize), shift: a.get(3) == Some(&"shift") },
                "rclickempty" => Step::RClick { pane: num(1) as usize, row: None, shift: a.get(2) == Some(&"shift") },
                "waitmenu" => Step::WaitMenu(num(1).max(1) as u64),
                "menuopen" => Step::MenuOpen(a[1..].join(" ")),
                "menukey" => Step::MenuKey(a.get(1).unwrap_or(&"").to_string()),
                "menupick" => Step::MenuPick(a[1..].join(" ")),
                "histmenu" => Step::HistMenu(num(1) as usize),
                "histpick" => Step::HistPick(num(1) as usize),
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
                    // `tab <pane> <index>` and `crumb <pane> <index>` take one word more than
                    // `<pane> <row>` and `side <row>`.
                    let m = if matches!(a.get(1), Some(&"tab" | &"crumb" | &"point")) { 4 } else { 3 };
                    // `sidegap <index>` (P12-M5) takes the side row's place.
                    let mode = a.get(m).unwrap_or(&"auto").to_string();
                    // `menu <copy|move|cancel>` takes one more word before the paths.
                    let (pick, first) = if mode == "menu" { (a.get(m + 1).unwrap_or(&"cancel").to_string(), m + 2) } else { (String::new(), m + 1) };
                    // The spot: `<pane> <row>`, `side <row>`, `tab <pane> <index>` or `crumb <pane> <index>`.
                    let at = match a.get(1).copied() {
                        Some("side") => DropAt::Side(num(2) as i32),
                        Some("sidegap") => DropAt::SideGap(num(2) as usize),
                        Some("tab") => DropAt::Tab(num(2) as usize, num(3) as usize),
                        Some("crumb") => DropAt::Crumb(num(2) as usize, num(3) as i32),
                        Some("point") => DropAt::Point(num(2) as f32, num(3) as f32),
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
                "dragover" => {
                    // `row|tab|crumb|strip <pane> <n|end>` take one word more than `<pane>
                    // <top|bottom>` and `side <top|bottom>`.
                    let (at, m) = match a.get(1).copied() {
                        Some("row") => (HoverAt::At(DropAt::Row(num(2) as usize, num(3) as i32)), 4),
                        Some("tab") => (HoverAt::At(DropAt::Tab(num(2) as usize, num(3) as usize)), 4),
                        Some("crumb") => (HoverAt::At(DropAt::Crumb(num(2) as usize, num(3) as i32)), 4),
                        Some("strip") => (HoverAt::Strip(num(2) as usize, a.get(3) == Some(&"right")), 4),
                        // `side <row>` (P11-M3): a sidebar row's centre, as `dropfiles side <row>`.
                        Some("side") if a.get(2).is_some_and(|v| v.parse::<i32>().is_ok()) => (HoverAt::At(DropAt::Side(num(2) as i32)), 3),
                        Some("side") => (HoverAt::Side(a.get(2) == Some(&"top")), 3),
                        _ => (HoverAt::Edge(num(1) as usize, a.get(2) == Some(&"top")), 3),
                    };
                    Step::DragOver {
                        at,
                        ms: num(m).max(1) as u64,
                        paths: a.get(m + 1..).unwrap_or(&[]).join(" ").split('|').map(str::trim).filter(|p| !p.is_empty()).map(String::from).collect(),
                    }
                }
                "tab" if a.get(1) == Some(&"plus") => Step::TabPlus(num(2) as usize),
                "tab" => Step::Key(match a.get(1).copied().unwrap_or("") {
                    "new" => "CtrlT".into(),
                    "close" => "CtrlW".into(),
                    "next" => "CtrlTab".into(),
                    "prev" => "CtrlShiftTab".into(),
                    "moveleft" => "CtrlShiftPageUp".into(),
                    "moveright" => "CtrlShiftPageDown".into(),
                    n => format!("Ctrl{n}"),
                }),
                "colwidth" | "coldrag" | "colfit" => Step::Col {
                    pane: num(1) as usize,
                    col: a.get(2).unwrap_or(&"").to_string(),
                    what: (a[0] != "colfit").then(|| a.get(3).and_then(|v| v.parse().ok()).unwrap_or(0.0)),
                    drag: a[0] == "coldrag",
                },
                "tabdrag" => Step::TabDrag { pane: num(1) as usize, index: num(2) as usize, dx: num(3) as f32 },
                "band" => Step::Band {
                    pane: num(1) as usize,
                    from: (num(2) as f32, num(3) as f32),
                    to: (num(4) as f32, num(5) as f32),
                    modifier: match a.get(6).map(|m| &**m) {
                        Some("ctrl") => Some(Key::Control),
                        Some("shift") => Some(Key::Shift),
                        _ => None,
                    },
                    hold_ms: a.iter().position(|w| &**w == "hold").and_then(|i| a.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(0),
                },
                "crumb" => Step::Crumb { pane: num(1) as usize, index: num(2) as i32 },
                "toolbar" => Step::Toolbar { pane: num(1) as usize, button: a.get(2).unwrap_or(&"").to_string() },
                "menuverb" => Step::MenuVerb { pane: num(1) as usize, verb: a.get(2..).unwrap_or(&[]).join(" ") },
                "zippassword" => Step::ZipPassword(a.get(1..).unwrap_or(&[]).join(" ")),
                "crumbmenu" => Step::CrumbMenu { pane: num(1) as usize, index: num(2) as i32 },
                "crumbpick" => Step::CrumbPick(a[1..].join(" ")),
                "crumbclose" => Step::CrumbClose,
                "pathmode" => Step::PathMode { pane: num(1) as usize, entry: a.get(2) == Some(&"entry") },
                "sidebar" if a.get(1) == Some(&"click") => Step::SideClick(num(2) as i32),
                "sidebar" => Step::Key("F9".into()),
                "bookmark" if a.get(1) == Some(&"remove") => Step::BookmarkRemove(num(2) as i32),
                "bookmark" if a.get(1) == Some(&"rename") => Step::BookmarkRename(num(2) as i32, a.get(3..).unwrap_or(&[]).join(" ")),
                "bookmark" if a.get(1) == Some(&"move") => Step::BookmarkMove(num(2) as i32, a.get(3) == Some(&"up")),
                "bookmark" if a.get(1) == Some(&"drag") => Step::BookmarkDrag(num(2) as usize, num(3) as usize),
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
                "mvfile" => Step::ScriptMove {
                    pane: num(1) as usize,
                    old: a.get(2).unwrap_or(&"").to_string(),
                    new: a.get(3).unwrap_or(&"").to_string(),
                },
                "waitwatch" => Step::WaitWatch(num(1).max(1) as u64, false),
                "waitedit" => Step::WaitWatch(num(1).max(1) as u64, true),
                // The text is the rest of the step after the pane, trimmed (may be empty).
                "filter" => Step::Filter {
                    pane: num(1) as usize,
                    text: s.trim().splitn(3, char::is_whitespace).nth(2).unwrap_or("").to_string(),
                },
                "search" => Step::Search(num(1) as usize),
                "searchreturn" => Step::SearchReturn(num(1) as usize),
                "searchbutton" => Step::SearchButton(num(1) as usize),
                "contentmode" => Step::ContentMode { pane: num(1) as usize, on: a.get(2) != Some(&"off") },
                "csearch" => Step::ContentSearch {
                    pane: num(1) as usize,
                    text: s.trim().splitn(3, char::is_whitespace).nth(2).unwrap_or("").to_string(),
                },
                "waitsearch" => Step::WaitSearch(num(1).max(1) as u64),
                "sizeall" => Step::SizeAll(num(1) as usize),
                "waitsize" => Step::WaitSize(num(1).max(1) as u64),
                "preview" if a.get(1) == Some(&"width") => Step::PreviewWidth(a.get(2).and_then(|v| v.parse().ok()).unwrap_or(crate::preview::RATIO_DEFAULT)),
                "preview" => Step::Key("AltP".into()),
                "previewscroll" => Step::PreviewScroll(num(1) as f32),
                "waitpreview" => Step::WaitPreview(num(1).max(1) as u64),
                "previewlink" => Step::PreviewLink(s.trim_start()["previewlink".len()..].trim().to_string()),
                "previewzoom" => Step::PreviewZoom(match a.get(1).copied() {
                    Some("in") => 1,
                    Some("out") => -1,
                    _ => 0,
                }),
                "pdfpassword" => Step::PdfPassword(s.trim_start()["pdfpassword".len()..].trim().to_string()),
                "previewcopy" => Step::PreviewCopy(a.get(1).and_then(|v| v.parse().ok())),
                "waitquestion" => Step::WaitQuestion(num(1).max(1) as u64),
                "answer" => Step::Answer(a.get(1..).unwrap_or(&[]).join(" ")),
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

/// The search box's content toggle (P11-M1) set as its click sets it: the property, then
/// `content-toggled` when it changed.
fn set_content_mode(app: &App, pane: usize, on: bool) {
    let was = if pane == 0 { app.get_left_content_mode() } else { app.get_right_content_mode() };
    eprintln!("[auto] contentmode pane={pane} {}", if on { "on" } else { "off" });
    if was == on {
        return;
    }
    if pane == 0 { app.set_left_content_mode(on) } else { app.set_right_content_mode(on) }
    app.invoke_content_toggled(pane as i32, on);
}

/// Logical window size and a point inside the given pane's list.
fn geometry(app: &App, pane: usize) -> (f32, f32, LogicalPosition) {
    let l = crate::window_layout(app);
    let (a, b) = crate::dnd::pane_x(&l, pane);
    (l.width, l.height, LogicalPosition::new((a + b) * 0.5, l.height * 0.5))
}

/// The top of the pane's list (75 px, or 107 px under a tab strip).
fn list_top(app: &App, pane: usize) -> f32 {
    crate::dnd::list_top(&crate::window_layout(app), pane)
}

/// Where the pane's vertical scrollbar thumb sits with content-y = 0 (right edge of the list,
/// just under the path bar and column header). Since P12-M2 the thumb is TwinFM's own
/// (`VScrollBar`, 2 px below the list's top, at least 24 px long): the same aim lands on it.
fn thumb(app: &App, pane: usize) -> LogicalPosition {
    let (_, right_edge) = crate::dnd::pane_x(&crate::window_layout(app), pane);
    LogicalPosition::new(right_edge - 10.0, list_top(app, pane) + 17.0)
}

/// Centre of a visible row: rows start under the 3 px padding, the tab strip when there is one,
/// 32 px path bar, 2 px spacing, 24 px header, 2 px spacing; 22 px each (32 px with thumbnails on).
/// In the icon grid, the centre of visible entry `row`'s tile (`row / cols` grid rows down,
/// `row % cols` tiles across, 96 x 136 px each (P7-M4), 80 x 107 px medium (P8-L1)).
fn row_pos(app: &App, pane: usize, row: usize) -> LogicalPosition {
    let (_, _, mid) = geometry(app, pane);
    let l = crate::window_layout(app);
    match l.grid[pane] {
        None => LogicalPosition::new(mid.x, list_top(app, pane) + row as f32 * l.row_px + l.row_px / 2.0),
        Some(cols) => {
            let (r, c) = crate::model::grid_cell(row, cols);
            let (left, _) = crate::dnd::pane_x(&l, pane);
            let t = l.tile[pane];
            let x = left + crate::dnd::PANE_PAD + (c as f32 + 0.5) * t.w;
            LogicalPosition::new(x, list_top(app, pane) + (r as f32 + 0.5) * t.h)
        }
    }
}

/// Where an `rclickempty` lands: halfway down the empty space below the last row (the last grid
/// row of tiles), or None when the rows fill the list (rows start at `list_top`; the list ends 33
/// px above the window bottom, over the 28 px status line, 2 px spacing and 3 px padding).
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
/// centre of a tab; the centre of a crumb's label. None for a sidebar row that does not exist, a
/// tab when the pane shows no strip, or a crumb that is not on screen.
fn drop_point(app: &App, at: DropAt) -> Option<LogicalPosition> {
    match at {
        DropAt::Row(pane, row) => {
            let (_, _, mid) = geometry(app, pane);
            Some(match usize::try_from(row) {
                Ok(r) => row_pos(app, pane, r),
                Err(_) => LogicalPosition::new(mid.x, list_top(app, pane) - 14.0),
            })
        }
        DropAt::Side(row) => Some(LogicalPosition::new(side_x(app), crate::side_row_centre(row)?)),
        DropAt::SideGap(k) => {
            // The bookmarks are the sidebar's last rows: scrolled into view first.
            app.invoke_side_scroll_end();
            Some(LogicalPosition::new(side_x(app), crate::side_gap_y(k)?))
        }
        DropAt::Tab(pane, index) => {
            let l = crate::window_layout(app);
            if !l.tabs[pane] {
                return None;
            }
            let (left, _) = crate::dnd::pane_x(&l, pane);
            let w = app.invoke_tab_width(pane as i32);
            let x = left + crate::dnd::PANE_PAD + app.invoke_tab_offset(pane as i32) + (index as f32 + 0.5) * w;
            Some(LogicalPosition::new(x, crate::dnd::tab_top(&l) + crate::dnd::TAB_H / 2.0))
        }
        DropAt::Crumb(pane, index) => crate::crumb_centre(app, pane, index),
        DropAt::Point(x, y) => Some(LogicalPosition::new(x, y)),
    }
}

/// Where `dragover` hovers (see `HoverAt`), in window px.
fn hover_point(app: &App, at: HoverAt) -> Option<LogicalPosition> {
    let l = crate::window_layout(app);
    match at {
        HoverAt::Edge(pane, top) => {
            let (_, _, mid) = geometry(app, pane);
            let y = if top { crate::dnd::list_top(&l, pane) + 6.0 } else { l.height - crate::dnd::LIST_BOTTOM_GAP - 6.0 };
            Some(LogicalPosition::new(mid.x, y))
        }
        HoverAt::At(spot) => drop_point(app, spot),
        HoverAt::Side(top) => {
            let y = if top { crate::dnd::side_top(&l) + 6.0 } else { l.height - crate::dnd::SIDE_FOOTER - 6.0 };
            Some(LogicalPosition::new(side_x(app), y))
        }
        HoverAt::Strip(pane, right) => {
            let (a, b) = crate::dnd::pane_x(&l, pane);
            let x = if right { b - crate::dnd::PANE_PAD - crate::dnd::TAB_PLUS - 6.0 } else { a + crate::dnd::PANE_PAD + 6.0 };
            l.tabs[pane].then(|| LogicalPosition::new(x, crate::dnd::tab_top(&l) + crate::dnd::TAB_H / 2.0))
        }
    }
}

/// The middle of the sidebar, or of the icons-only rail when it is collapsed (P12-M5).
fn side_x(app: &App) -> f32 {
    if app.get_sidebar_visible() { app.get_sidebar_width() / 2.0 } else { crate::dnd::RAIL_W / 2.0 }
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
        "AltP" => vec![k(Key::Alt), "p".into()],
        "AltShiftReturn" => vec![k(Key::Alt), k(Key::Shift), k(Key::Return)],
        "CtrlComma" => vec![k(Key::Control), ",".into()],
        "CtrlShiftRight" => vec![k(Key::Control), k(Key::Shift), k(Key::RightArrow)],
        "CtrlH" => vec![k(Key::Control), "h".into()],
        "CtrlR" => vec![k(Key::Control), "r".into()],
        "CtrlA" => vec![k(Key::Control), "a".into()],
        "CtrlC" => vec![k(Key::Control), "c".into()],
        "CtrlX" => vec![k(Key::Control), "x".into()],
        "CtrlV" => vec![k(Key::Control), "v".into()],
        "CtrlZ" => vec![k(Key::Control), "z".into()],
        "CtrlY" => vec![k(Key::Control), "y".into()],
        "CtrlShiftN" => vec![k(Key::Control), k(Key::Shift), "N".into()],
        "CtrlAltN" => vec![k(Key::Control), k(Key::Alt), "n".into()],
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
        "ShiftPageUp" => vec![k(Key::Shift), k(Key::PageUp)],
        "ShiftPageDown" => vec![k(Key::Shift), k(Key::PageDown)],
        "ShiftLeft" => vec![k(Key::Shift), k(Key::LeftArrow)],
        "ShiftRight" => vec![k(Key::Shift), k(Key::RightArrow)],
        "CtrlShift1" => vec![k(Key::Control), k(Key::Shift), "1".into()],
        "CtrlShift2" => vec![k(Key::Control), k(Key::Shift), "2".into()],
        "CtrlShift3" => vec![k(Key::Control), k(Key::Shift), "3".into()],
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
        "Tab" => vec![k(Key::Tab)],
        "ShiftTab" => vec![k(Key::Shift), k(Key::Tab)],
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
        Step::XButton { pane, forward, at } => {
            use windows_sys::Win32::System::SystemServices::{MK_XBUTTON1, MK_XBUTTON2};
            use windows_sys::Win32::UI::WindowsAndMessaging::{WM_XBUTTONDOWN, WM_XBUTTONUP, XBUTTON1, XBUTTON2};
            // Posted, not dispatched: winit's window procedure turns them into the side button's
            // press and release (after the move that places the pointer), as for a real mouse.
            let (_, _, mut pos) = geometry(&app, pane);
            let l = crate::window_layout(&app);
            let (left, _) = crate::dnd::pane_x(&l, pane);
            match at.as_deref() {
                Some("path") => pos.y = crate::dnd::crumb_top(&l, pane) + crate::dnd::CRUMB_H / 2.0,
                Some("edit") => {
                    pos.x = left + crate::dnd::PANE_PAD + l.row_px + 60.0;
                    pos.y = list_top(&app, pane) + l.row_px / 2.0;
                }
                // Over the toolbar's first button (P7-M4): the bar across the window when there is
                // one (its Back goes to the active pane), else the pane's own bar.
                Some("tool") if l.top > 0.0 => pos = slint::LogicalPosition::new(20.0, l.top / 2.0),
                Some("tool") => pos = slint::LogicalPosition::new(left + crate::dnd::PANE_PAD + 16.0, l.top + crate::dnd::PANE_PAD + 16.0),
                _ => {}
            }
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
        Step::Hover { x, y } => {
            // Through Slint's pointer path: winit drops a posted WM_MOUSEMOVE while the real cursor
            // is outside the window.
            let pos = slint::LogicalPosition::new(x, y);
            app.window().dispatch_event_with_result(slint::platform::WindowEvent::PointerMoved { position: pos }).ok();
            eprintln!("[auto] hover at ({x:.0}, {y:.0})");
            schedule(runner, 50);
        }
        Step::ClickAt { x, y, right } => {
            let pos = slint::LogicalPosition::new(x, y);
            let button = if right { PointerEventButton::Right } else { PointerEventButton::Left };
            let win = app.window();
            win.dispatch_event_with_result(WindowEvent::PointerMoved { position: pos }).ok();
            win.dispatch_event_with_result(WindowEvent::PointerPressed { position: pos, button }).ok();
            win.dispatch_event_with_result(WindowEvent::PointerReleased { position: pos, button }).ok();
            eprintln!("[auto] clickat ({x:.0}, {y:.0}){}", if right { " right" } else { "" });
            schedule(runner, 50);
        }
        Step::WheelAt { x, y, dy, shift } => {
            // Slint keeps the modifiers from key events, so Shift is pressed and released around it.
            let pos = slint::LogicalPosition::new(x, y);
            let win = app.window();
            let before = app.invoke_preview_scroll_top();
            win.dispatch_event_with_result(WindowEvent::PointerMoved { position: pos }).ok();
            if shift {
                win.dispatch_event_with_result(WindowEvent::KeyPressed { text: Key::Shift.into() }).ok();
            }
            win.dispatch_event_with_result(WindowEvent::PointerScrolled { position: pos, delta_x: 0.0, delta_y: dy }).ok();
            if shift {
                win.dispatch_event_with_result(WindowEvent::KeyReleased { text: Key::Shift.into() }).ok();
            }
            let r2 = runner.clone();
            let weak = app.as_weak();
            runner.borrow().sub.start(TimerMode::SingleShot, Duration::from_millis(300), move || {
                if let Some(app) = weak.upgrade() {
                    eprintln!(
                        "[auto] wheelat ({x:.0}, {y:.0}) dy {dy}{}: preview scroll {before:.0} -> {:.0}",
                        if shift { " shift" } else { "" },
                        app.invoke_preview_scroll_top()
                    );
                }
                schedule(r2.clone(), 1);
            });
        }
        Step::Crumb { pane, index } => {
            app.invoke_crumb_clicked(pane as i32, index);
            schedule(runner, 1);
        }
        Step::Toolbar { pane, button } => {
            let t = if pane == 0 { app.get_left_tools() } else { app.get_right_tools() };
            let enabled = match button.as_str() {
                "newfile" | "newfolder" => t.new,
                "cut" => t.cut,
                "copy" => t.copy,
                "paste" => t.paste,
                "rename" => t.rename,
                "delete" => t.delete,
                _ => true,
            };
            if enabled {
                app.invoke_toolbar_clicked(pane as i32, button.as_str().into());
            } else {
                eprintln!("[auto] toolbar pane={pane} {button}: disabled, not pressed");
            }
            schedule(runner, 1);
        }
        Step::MenuVerb { pane, verb } => {
            crate::script_menu_verb(pane, &verb);
            schedule(runner, 1);
        }
        Step::ZipPassword(password) => {
            eprintln!("[auto] zippassword: the next password question gets the script's answer");
            crate::fileops::archive::script_password(&password);
            schedule(runner, 1);
        }
        Step::CrumbMenu { pane, index } => {
            // Under the path bar, near the pane's left edge.
            let (left, _) = crate::dnd::pane_x(&crate::window_layout(&app), pane);
            app.invoke_crumb_menu(pane as i32, index, left + 40.0, list_top(&app, pane) - 36.0);
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
        Step::Col { pane, col, what, drag } => {
            let l = crate::window_layout(&app);
            match crate::cols::Col::from_name(&col) {
                None => eprintln!("[auto] col: no column {col:?} (type, size or modified)"),
                Some(c) if l.grid[pane].is_some() || !crate::cols::grips(app.get_show_type()).contains(&c) => {
                    eprintln!("[auto] col pane={pane} {col}: no such grip in this pane's header")
                }
                Some(c) if !drag && what.is_some() => crate::col_set(&app, pane, c, what.unwrap_or(0.0), "script"),
                Some(c) => {
                    // The grip's centre: the header's cells end at the list's right edge, and the
                    // header is the 32 px above the list's top less the layout's 2 px spacing.
                    let (_, right) = crate::dnd::pane_x(&l, pane);
                    let x0 = crate::cols::grip_x(right - crate::dnd::PANE_PAD, crate::cols_of(&app, pane), c);
                    let y = crate::dnd::list_top(&l, pane) - 18.0;
                    let at = |x: f32| LogicalPosition::new(x, y);
                    let win = app.window();
                    let press = |x: f32| {
                        win.dispatch_event_with_result(WindowEvent::PointerPressed { position: at(x), button: PointerEventButton::Left }).ok();
                    };
                    let release = |x: f32| {
                        win.dispatch_event_with_result(WindowEvent::PointerReleased { position: at(x), button: PointerEventButton::Left }).ok();
                    };
                    win.dispatch_event_with_result(WindowEvent::PointerMoved { position: at(x0) }).ok();
                    press(x0);
                    if drag {
                        let dx = what.unwrap_or(0.0);
                        let steps = (dx.abs() / 8.0).ceil().max(1.0) as i32;
                        for k in 1..=steps {
                            win.dispatch_event_with_result(WindowEvent::PointerMoved { position: at(x0 + dx * k as f32 / steps as f32) }).ok();
                        }
                        release(x0 + dx);
                        eprintln!("[auto] coldrag pane={pane} {col} from x {x0:.0} by {dx:.0}");
                    } else {
                        release(x0);
                        press(x0);
                        release(x0);
                        eprintln!("[auto] colfit pane={pane} {col}: double-click at x {x0:.0}");
                    }
                    // Move away so the next step does not start over the grip.
                    win.dispatch_event_with_result(WindowEvent::PointerExited).ok();
                }
            }
            schedule(runner, 1);
        }
        Step::Band { pane, from, to, modifier, hold_ms } => {
            let layout = crate::window_layout(&app);
            let (left, right) = crate::dnd::pane_x(&layout, pane);
            let origin = (left + crate::dnd::PANE_PAD, crate::dnd::list_top(&layout, pane));
            let width = right - left - 2.0 * crate::dnd::PANE_PAD;
            let at = |p: (f32, f32)| LogicalPosition::new(origin.0 + p.0, origin.1 + p.1);
            let win = app.window();
            win.dispatch_event_with_result(WindowEvent::PointerMoved { position: at(from) }).ok();
            if let Some(m) = modifier {
                win.dispatch_event_with_result(WindowEvent::KeyPressed { text: m.into() }).ok();
            }
            win.dispatch_event_with_result(WindowEvent::PointerPressed { position: at(from), button: PointerEventButton::Left }).ok();
            let (dx, dy) = (to.0 - from.0, to.1 - from.1);
            let steps = (dx.abs().max(dy.abs()) / 8.0).ceil().max(1.0) as i32;
            for k in 1..=steps {
                let f = k as f32 / steps as f32;
                win.dispatch_event_with_result(WindowEvent::PointerMoved { position: at((from.0 + dx * f, from.1 + dy * f)) }).ok();
            }
            eprintln!(
                "[auto] band pane={pane} from ({}, {}) to ({}, {}){} (list {width:.0} px wide, {} tiles to a row){}",
                from.0,
                from.1,
                to.0,
                to.1,
                modifier.map_or("", |m| if m == Key::Control { " with Ctrl" } else { " with Shift" }),
                layout.grid[pane].map_or(0, |c| c),
                if hold_ms > 0 { format!(", held {hold_ms} ms") } else { String::new() }
            );
            let release = at(to);
            let w = app.as_weak();
            let r2 = runner.clone();
            let end = move || {
                let Some(app) = w.upgrade() else { return };
                let win = app.window();
                win.dispatch_event_with_result(WindowEvent::PointerReleased { position: release, button: PointerEventButton::Left }).ok();
                if let Some(m) = modifier {
                    win.dispatch_event_with_result(WindowEvent::KeyReleased { text: m.into() }).ok();
                }
                if hold_ms == 0 {
                    schedule(r2.clone(), 300);
                }
            };
            if hold_ms > 0 {
                runner.borrow().sub.start(TimerMode::SingleShot, Duration::from_millis(hold_ms), end);
                schedule(runner, 1);
            } else {
                end();
            }
        }
        Step::PathMode { pane, entry } => {
            app.invoke_set_path_mode(pane as i32, entry);
            schedule(runner, 1);
        }
        Step::SideClick(row) => {
            // A negative row counts from the end (-1: the last row, the newest bookmark).
            let row = if row < 0 { slint::Model::row_count(&app.get_side_items()) as i32 + row } else { row };
            app.invoke_side_click(row);
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
        Step::BookmarkDrag(from, to) => {
            // The bookmarks are the sidebar's last rows: the list scrolled to its end, then rows
            // aimed with its scroll.
            app.invoke_side_scroll_end();
            let n = app.get_bookmark_count().max(0) as usize;
            let first = slint::Model::row_count(&app.get_side_items()).saturating_sub(n);
            let top = crate::dnd::side_top(&crate::window_layout(&app)) + app.invoke_side_scroll();
            let row_y = |r: usize| top + app.invoke_side_row_top(r as i32);
            let x = side_x(&app);
            let gap = if to > from { to + 1 } else { to };
            let (y0, y1) = (row_y(first + from) + 16.0, row_y(first + gap.min(n)));
            let win = app.window();
            win.dispatch_event_with_result(WindowEvent::PointerMoved { position: LogicalPosition::new(x, y0) }).ok();
            win.dispatch_event_with_result(WindowEvent::PointerPressed { position: LogicalPosition::new(x, y0), button: PointerEventButton::Left }).ok();
            let steps = ((y1 - y0).abs() / 4.0).ceil().max(2.0) as i32;
            for k in 1..=steps {
                let y = y0 + (y1 - y0) * k as f32 / steps as f32;
                win.dispatch_event_with_result(WindowEvent::PointerMoved { position: LogicalPosition::new(x, y) }).ok();
            }
            win.dispatch_event_with_result(WindowEvent::PointerReleased { position: LogicalPosition::new(x, y1), button: PointerEventButton::Left }).ok();
            eprintln!("[auto] bookmark drag {from} -> {to}: pressed at y {y0:.0}, released at y {y1:.0}");
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
        Step::ScriptMove { pane, old, new } => {
            if [&old, &new].iter().any(|n| n.is_empty() || n.contains(['\\', '/'])) {
                eprintln!("[auto] mvfile takes two plain file names, not {old:?} {new:?}");
            } else {
                crate::script_rename(pane, &old, &new);
            }
            schedule(runner, 1);
        }
        Step::WaitWatch(max_ms, edits_only) => {
            let counts = move || (if edits_only { 0 } else { crate::watch_refreshes() }, crate::watch_edits());
            let start = counts();
            let deadline = std::time::Instant::now() + Duration::from_millis(max_ms);
            let r2 = runner.clone();
            runner.borrow().sub.start(TimerMode::Repeated, Duration::from_millis(50), move || {
                let seen = counts() != start;
                if seen || std::time::Instant::now() >= deadline {
                    if !seen && edits_only {
                        eprintln!("[auto] waitedit: no watch window applied within {max_ms} ms");
                    } else if !seen {
                        eprintln!("[auto] waitwatch: no watch-driven refresh or applied window within {max_ms} ms");
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
        Step::SearchReturn(pane) => {
            let text = if pane == 0 { app.get_left_filter_text() } else { app.get_right_filter_text() };
            app.invoke_filter_return(pane as i32, text);
            schedule(runner, 1);
        }
        Step::HistMenu(pane) => {
            app.invoke_command(pane as i32, "histmenu".into());
            schedule(runner, 1);
        }
        Step::HistPick(i) => {
            eprintln!("[auto] histpick {i}");
            crate::winmenu_picked(crate::winmenu::Cmd::History(i).id());
            schedule(runner, 1);
        }
        Step::SearchButton(pane) => {
            app.invoke_search_pressed(pane as i32);
            schedule(runner, 1);
        }
        Step::ContentMode { pane, on } => {
            set_content_mode(&app, pane, on);
            schedule(runner, 1);
        }
        Step::ContentSearch { pane, text } => {
            set_content_mode(&app, pane, true);
            if pane == 0 {
                app.set_left_filter_text(text.as_str().into());
            } else {
                app.set_right_filter_text(text.as_str().into());
            }
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
            press(&app, &chord("AltShiftReturn"));
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
        Step::PreviewWidth(r) => {
            let r = crate::preview::clamp_ratio(r);
            app.set_preview_width(r);
            app.invoke_settings_changed();
            eprintln!("[auto] preview width {r:.2}");
            schedule(runner, 1);
        }
        Step::PreviewScroll(px) => {
            app.invoke_preview_set_scroll(px);
            schedule(runner, 1);
        }
        Step::PreviewLink(dest) => {
            crate::preview::link(dest.into());
            schedule(runner, 100);
        }
        Step::PreviewZoom(step) => {
            if !crate::preview::zoom(step) {
                eprintln!("[auto] previewzoom: no PDF shown");
            }
            schedule(runner, 1);
        }
        Step::PdfPassword(p) => {
            eprintln!("[auto] pdfpassword: the next password question gets the script's answer");
            crate::preview::script_password(&p);
            crate::preview::password();
            schedule(runner, 1);
        }
        Step::PreviewCopy(one) => {
            crate::preview::copy(one);
            schedule(runner, 1);
        }
        Step::WaitQuestion(max_ms) => {
            let deadline = std::time::Instant::now() + Duration::from_millis(max_ms);
            let r2 = runner.clone();
            runner.borrow().sub.start(TimerMode::Repeated, Duration::from_millis(50), move || {
                let shown = crate::ask::shown();
                if shown || std::time::Instant::now() >= deadline {
                    if !shown {
                        eprintln!("[auto] waitquestion: no question after {max_ms} ms");
                    }
                    r2.borrow().sub.stop();
                    schedule(r2.clone(), 100);
                }
            });
        }
        Step::TabPlus(pane) => {
            app.invoke_tab_key(pane as i32, "plus".into());
            schedule(runner, 1);
        }
        Step::Answer(label) => {
            if !crate::ask::answer_label(&label) {
                eprintln!("[auto] answer {label}: no question shows a button reading so");
            }
            schedule(runner, 1);
        }
        Step::WaitPreview(max_ms) => {
            let deadline = std::time::Instant::now() + Duration::from_millis(max_ms);
            let r2 = runner.clone();
            // The first look after 50 ms, so a cursor move just made has restarted the settle.
            runner.borrow().sub.start(TimerMode::Repeated, Duration::from_millis(50), move || {
                let busy = !crate::preview::settled();
                if !busy || std::time::Instant::now() >= deadline {
                    if busy {
                        eprintln!("[auto] waitpreview: not settled after {max_ms} ms");
                    }
                    r2.borrow().sub.stop();
                    schedule(r2.clone(), 100);
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
        Step::MenuOpen(t) | Step::MenuPick(t) | Step::MenuKey(t)
            if (t.eq_ignore_ascii_case("Show more options") || t == "ShiftF10") && crate::winmenu_shown(false) =>
        {
            // The classic menu opens inside this call: its in-menu steps run from the Win32 timer.
            let steps = take_menu_steps(&mut runner.borrow_mut());
            arm_menu_steps(&app, steps);
            crate::script_winmenu("open", "Show more options");
            disarm_menu_steps();
            schedule(runner, 1);
        }
        Step::MenuOpen(t) if crate::script_winmenu("open", &t) => schedule(runner, 1),
        Step::MenuPick(t) if crate::script_winmenu("pick", &t) => schedule(runner, 1),
        Step::MenuKey(k) if crate::script_winmenu("key", &if k == "ShiftF10" { "more".to_string() } else { k.to_ascii_lowercase() }) => schedule(runner, 1),
        Step::MenuClose if crate::script_winmenu("close", "") => schedule(runner, 1),
        Step::WaitMenu(max_ms) => {
            let deadline = std::time::Instant::now() + Duration::from_millis(max_ms);
            let r2 = runner.clone();
            runner.borrow().sub.start(TimerMode::Repeated, Duration::from_millis(20), move || {
                let shown = crate::winmenu_shown(true);
                if shown || std::time::Instant::now() >= deadline {
                    if !shown {
                        eprintln!("[auto] waitmenu: no settled Windows 11 menu after {max_ms} ms");
                    }
                    r2.borrow().sub.stop();
                    schedule(r2.clone(), 100);
                }
            });
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
        Step::DragOver { at, ms, paths } => {
            // A drag hovering at one point (an edge that scrolls, a spot that springs open):
            // DragEnter, a DragOver every 20 ms from a Slint timer (OLE calls DragOver on its own
            // timer for another program's drag, outside any modal loop of ours), then DragLeave.
            // It never drops.
            let Some(pos) = hover_point(&app, at) else {
                eprintln!("[auto] dragover {at:?}: no such spot on screen");
                schedule(runner, 1);
                return;
            };
            let (sx, sy) = crate::shellmenu::to_screen(hwnd_of(&app), pos.x, pos.y, app.window().scale_factor());
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
        Step::RClick { pane, row, shift } => {
            // Only the classic menu (Shift) runs a modal loop that takes in-menu steps.
            let mut menu_steps = if shift { take_menu_steps(&mut runner.borrow_mut()) } else { Vec::new() };
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
                app.invoke_context_menu(pane as i32, -1, mid.x, list_top(&app, pane) + crate::window_layout(&app).row_px_of(pane) / 2.0, shift);
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
                // Shift held over the release: the classic menu opens and runs inside this
                // dispatch, which returns when it closes. Without it the Windows 11 menu opens.
                let shift_key = [SharedString::from(Key::Shift)];
                if shift {
                    app.window().dispatch_event_with_result(WindowEvent::KeyPressed { text: shift_key[0].clone() }).ok();
                }
                app.window()
                    .dispatch_event_with_result(WindowEvent::PointerReleased { position: pos, button: PointerEventButton::Right })
                    .ok();
                if shift {
                    app.window().dispatch_event_with_result(WindowEvent::KeyReleased { text: shift_key[0].clone() }).ok();
                }
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
