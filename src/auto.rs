//! Scripted driver for unattended gate runs. Set TWINFM_SCRIPT to a ';'-separated list
//! (or to @<file> to read the list from a file; Windows caps an env var at 32 KB):
//!   sleep <ms>                       wait
//!   split                            toggle the second pane (same as F3)
//!   wheel <pane> <ticks> <ms>        wheel ticks (negative = down) <ms> apart over the pane's list
//!   drag <pane> <dy> <ms>            press the pane's scrollbar thumb and drag it <dy> px in 16 ms steps
//!   nav <pane> <path>                navigate a pane
//!   up <pane>                        go to the parent directory
//!   sort <pane> <0|1|2|3>            click a column header (name/size/modified/type)
//!   key <name>                       press a key through the Slint key path; names: F3, F5,
//!                                    Backspace, Return, Space, AltUp, CtrlShiftRight, CtrlH
//!                                    (hidden files), CtrlR (refresh), CtrlA, ShiftUp, ShiftDown,
//!                                    ShiftF5, ShiftF6, Delete, ShiftDelete, F2, F7, CtrlC, CtrlX,
//!                                    CtrlV, CtrlShiftN, Escape, ShiftF10, Menu (the last two open
//!                                    the context menu and take in-menu steps like rclick)
//!   copy | move                      Shift+F5 / Shift+F6: the active pane's selection to the other pane
//!   delete | sdelete                 Delete (Recycle Bin) / Shift+Delete (permanent)
//!   cc | cx | cv                     Ctrl+C / Ctrl+X / Ctrl+V through the real clipboard
//!   newfolder <name>                 F7, type the name into the editor, Return
//!   rename <name>                    F2 on the cursor row, type the new name, Return
//!   waitops <ms>                     wait until no file operation runs and no pane refreshes
//!                                    (at most <ms>), then 300 ms more for the pass to land
//!   click <pane> <row>               click a visible row (selects only it, moves the cursor)
//!   ctrlclick <pane> <row>           Ctrl+click a visible row (toggles it)
//!   shiftclick <pane> <row>          Shift+click a visible row (selects anchor..row)
//!   dblclick <pane> <row>            double-click a visible row (enters the directory if it is one)
//!   rclick <pane> <row>              right-click a visible row: press, 60 ms for a frame, release;
//!                                    the release opens the real shell context menu
//!   rclickempty <pane>               right-click the empty space below the last row (the folder's
//!                                    background menu); when the rows fill the list there is none,
//!                                    and the step calls the pane's context-menu callback directly
//!   menuopen <text>                  inside the menu: open the submenu whose item reads <text>
//!                                    ("Send to"), with Down presses then Right; never Return
//!   menukey <Down|Up|Right|Left|Escape>  inside the menu: one key
//!   menupick <text>                  inside the menu: choose one of TwinFM's own items ("Copy path",
//!                                    "Open in other pane", "Open in terminal"); refuses shell items
//!   menuclose                        inside the menu: close it (EndMenu)
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

use crate::App;
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, SharedString, Timer, TimerMode};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT, VK_UP};
use windows_sys::Win32::UI::WindowsAndMessaging::{EndMenu, KillTimer, PostMessageW, SetTimer, WM_KEYDOWN};

#[derive(Debug, Clone)]
enum Step {
    Sleep(u64),
    Split,
    Wheel { pane: usize, ticks: i32, ms: u64 },
    Drag { pane: usize, dy: f32, ms: u64 },
    Nav { pane: usize, path: String },
    Up(usize),
    Sort { pane: usize, col: i32 },
    Key(String),
    Click { pane: usize, row: usize, modifier: Option<Key> },
    DblClick { pane: usize, row: usize },
    /// Press `lead`, type `text` into whatever then has focus, press Return.
    Type { lead: String, text: String },
    WaitOps(u64),
    /// Right-click a row, or with `row: None` the empty space below the rows.
    RClick { pane: usize, row: Option<usize> },
    MenuOpen(String),
    MenuKey(String),
    MenuPick(String),
    MenuClose,
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
                "sort" => Step::Sort { pane: num(1) as usize, col: num(2) as i32 },
                "key" => Step::Key(a.get(1).unwrap_or(&"").to_string()),
                "click" => Step::Click { pane: num(1) as usize, row: num(2) as usize, modifier: None },
                "ctrlclick" => Step::Click { pane: num(1) as usize, row: num(2) as usize, modifier: Some(Key::Control) },
                "shiftclick" => Step::Click { pane: num(1) as usize, row: num(2) as usize, modifier: Some(Key::Shift) },
                "dblclick" => Step::DblClick { pane: num(1) as usize, row: num(2) as usize },
                "copy" => Step::Key("ShiftF5".into()),
                "move" => Step::Key("ShiftF6".into()),
                "delete" => Step::Key("Delete".into()),
                "sdelete" => Step::Key("ShiftDelete".into()),
                "cc" => Step::Key("CtrlC".into()),
                "cx" => Step::Key("CtrlX".into()),
                "cv" => Step::Key("CtrlV".into()),
                "newfolder" => Step::Type { lead: "F7".into(), text: a[1..].join(" ") },
                "rename" => Step::Type { lead: "F2".into(), text: a[1..].join(" ") },
                "waitops" => Step::WaitOps(num(1).max(1) as u64),
                "rclick" => Step::RClick { pane: num(1) as usize, row: Some(num(2) as usize) },
                "rclickempty" => Step::RClick { pane: num(1) as usize, row: None },
                "menuopen" => Step::MenuOpen(a[1..].join(" ")),
                "menukey" => Step::MenuKey(a.get(1).unwrap_or(&"").to_string()),
                "menupick" => Step::MenuPick(a[1..].join(" ")),
                "menuclose" => Step::MenuClose,
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
    let size = app.window().size().to_logical(app.window().scale_factor());
    let (w, h) = (size.width, size.height);
    let split = app.get_split();
    let ratio = app.get_split_ratio();
    let x = if !split {
        w * 0.5
    } else if pane == 0 {
        w * ratio * 0.5
    } else {
        w * ratio + (w - w * ratio) * 0.5
    };
    (w, h, LogicalPosition::new(x, h * 0.5))
}

/// Where the pane's vertical scrollbar thumb sits with content-y = 0 (right edge of the list,
/// just under the path entry and column header).
fn thumb(app: &App, pane: usize) -> LogicalPosition {
    let (w, _h, _) = geometry(app, pane);
    let split = app.get_split();
    let ratio = app.get_split_ratio();
    let right_edge = if !split { w } else if pane == 0 { w * ratio - 3.0 } else { w };
    LogicalPosition::new(right_edge - 10.0, 80.0)
}

/// Centre of a visible row: rows start under the 3 px padding, 32 px path entry, 2 px spacing,
/// 24 px header, 2 px spacing.
fn row_pos(app: &App, pane: usize, row: usize) -> LogicalPosition {
    let (_, _, mid) = geometry(app, pane);
    LogicalPosition::new(mid.x, 63.0 + row as f32 * 22.0 + 11.0)
}

/// Where an `rclickempty` lands: halfway down the empty space below the last row, or None when
/// the rows fill the list (rows start at y 63; the list ends 23 px above the window bottom,
/// over the 18 px status line, 2 px spacing and 3 px padding).
fn empty_pos(app: &App, pane: usize) -> Option<LogicalPosition> {
    use slint::Model;
    let (_, h, mid) = geometry(app, pane);
    let rows = if pane == 0 { app.get_left_rows().row_count() } else { app.get_right_rows().row_count() };
    let end = 63.0 + app.invoke_get_scroll(pane as i32) + rows as f32 * 22.0;
    let bottom = h - 23.0;
    (end + 4.0 < bottom).then(|| LogicalPosition::new(mid.x, (end + bottom) * 0.5))
}

/// Take the in-menu steps that follow an `rclick`: the leading run of sleep/menuopen/menukey/
/// menuclose steps up to and including the first menuclose (or menupick, which also closes the
/// menu). None if that run has no menuclose.
fn take_menu_steps(r: &mut Runner) -> Vec<Step> {
    let rest = &r.steps[r.idx..];
    let run = rest
        .iter()
        .take_while(|s| matches!(s, Step::Sleep(_) | Step::MenuOpen(_) | Step::MenuKey(_) | Step::MenuPick(_) | Step::MenuClose))
        .count();
    let Some(close) = rest[..run].iter().position(|s| matches!(s, Step::MenuClose | Step::MenuPick(_))) else {
        eprintln!("[auto] no menuclose after the right-click: the menu stays open until closed by hand");
        return Vec::new();
    };
    let taken = rest[..=close].to_vec();
    r.idx += close + 1;
    taken
}

/// Queue the in-menu steps and start the first tick. Called just before the event that opens
/// the menu; the ticks run from the menu's own message loop.
fn arm_menu_steps(app: &App, steps: Vec<Step>) {
    if let Ok(h) = app.window().window_handle().window_handle() {
        if let RawWindowHandle::Win32(w) = h.as_raw() {
            MENU_HWND.with(|c| c.set(w.hwnd.get()));
        }
    }
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
                let own = crate::shellmenu::OwnItem::ALL.iter().any(|o| o.label().eq_ignore_ascii_case(&text));
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
        "CtrlShiftRight" => vec![k(Key::Control), k(Key::Shift), k(Key::RightArrow)],
        "CtrlH" => vec![k(Key::Control), "h".into()],
        "CtrlR" => vec![k(Key::Control), "r".into()],
        "CtrlA" => vec![k(Key::Control), "a".into()],
        "CtrlC" => vec![k(Key::Control), "c".into()],
        "CtrlX" => vec![k(Key::Control), "x".into()],
        "CtrlV" => vec![k(Key::Control), "v".into()],
        "CtrlShiftN" => vec![k(Key::Control), k(Key::Shift), "N".into()],
        "ShiftUp" => vec![k(Key::Shift), k(Key::UpArrow)],
        "ShiftDown" => vec![k(Key::Shift), k(Key::DownArrow)],
        "ShiftF5" => vec![k(Key::Shift), k(Key::F5)],
        "ShiftF6" => vec![k(Key::Shift), k(Key::F6)],
        "ShiftDelete" => vec![k(Key::Shift), k(Key::Delete)],
        "ShiftF10" => vec![k(Key::Shift), k(Key::F10)],
        "Menu" => vec![k(Key::Menu)],
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
        Step::Quit => {
            let _ = slint::quit_event_loop();
        }
        Step::MenuOpen(_) | Step::MenuKey(_) | Step::MenuPick(_) | Step::MenuClose => {
            eprintln!("[auto] {step:?} outside a menu (it belongs after rclick or rclickempty); skipped");
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
                arm_menu_steps(&app, menu_steps);
                app.invoke_context_menu(pane as i32, -1, mid.x, 63.0 + 11.0, false);
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
