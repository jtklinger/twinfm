//! Scripted driver for unattended gate runs. Set RUSTFM_SCRIPT to a ';'-separated list:
//!   sleep <ms>                       wait
//!   split                            toggle the second pane (same as F3)
//!   wheel <pane> <ticks> <ms>        wheel ticks (negative = down) <ms> apart over the pane's list
//!   drag <pane> <dy> <ms>            press the pane's scrollbar thumb and drag it <dy> px in 16 ms steps
//!   nav <pane> <path>                navigate a pane
//!   up <pane>                        go to the parent directory
//!   sort <pane> <0|1|2>              click a column header (name/size/modified)
//!   key <F3|Backspace|AltUp|CtrlShiftRight|Return>   press a key through the Slint key path
//!   dblclick <pane> <row>            double-click a visible row (enters the directory if it is one)
//!   quit                             exit the event loop
//! Events are dispatched with slint::Window::dispatch_event on the UI thread, so Slint's full
//! input -> layout -> render path runs; only the OS WndProc hop is skipped.

use crate::App;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, Timer, TimerMode};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

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
    DblClick { pane: usize, row: usize },
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
                "dblclick" => Step::DblClick { pane: num(1) as usize, row: num(2) as usize },
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
}

pub fn start(app: &App) {
    let Ok(script) = std::env::var("RUSTFM_SCRIPT") else { return };
    let steps = parse(&script);
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

fn run_step(runner: Rc<RefCell<Runner>>) {
    let (step, app) = {
        let r = runner.borrow();
        let Some(step) = r.steps.get(r.idx).cloned() else {
            eprintln!("[auto] script finished");
            return;
        };
        (step, r.app.unwrap())
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
        Step::Key(name) => {
            use slint::platform::Key;
            let chord: Vec<Key> = match name.as_str() {
                "F3" => vec![Key::F3],
                "Backspace" => vec![Key::Backspace],
                "Return" => vec![Key::Return],
                "AltUp" => vec![Key::Alt, Key::UpArrow],
                "CtrlShiftRight" => vec![Key::Control, Key::Shift, Key::RightArrow],
                other => {
                    eprintln!("[auto] unknown key {other}");
                    vec![]
                }
            };
            let win = app.window();
            for k in &chord {
                win.dispatch_event_with_result(WindowEvent::KeyPressed { text: (*k).into() }).ok();
            }
            for k in chord.iter().rev() {
                win.dispatch_event_with_result(WindowEvent::KeyReleased { text: (*k).into() }).ok();
            }
            schedule(runner, 1);
        }
        Step::DblClick { pane, row } => {
            // rows start under the 3 px padding, 32 px path entry, 2 px spacing, 24 px header, 2 px spacing
            let (_, _, mid) = geometry(&app, pane);
            let pos = LogicalPosition::new(mid.x, 63.0 + row as f32 * 22.0 + 11.0);
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
        Step::Wheel { pane, ticks, ms } => {
            let (_, _, pos) = geometry(&app, pane);
            app.window().dispatch_event_with_result(WindowEvent::PointerMoved { position: pos }).ok();
            let remaining = Rc::new(RefCell::new(ticks.abs()));
            let delta = if ticks < 0 { -120.0 } else { 120.0 };
            let r2 = runner.clone();
            let w = app.as_weak();
            runner.borrow().sub.start(TimerMode::Repeated, Duration::from_millis(ms), move || {
                let app = w.unwrap();
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
                let app = w.unwrap();
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
