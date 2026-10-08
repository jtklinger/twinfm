// rustfm-proto: Slint bootstrap, wires model <-> UI, owns the two panes.
slint::include_modules!();

mod fs {
    pub mod enumerate;
}
mod auto;
mod model;
mod perf;

use crossbeam_channel::{Receiver, Sender, unbounded};
use fs::enumerate::Msg;
use model::{PaneModel, SortCol, sort_entries};
use slint::{ComponentHandle, ModelRc, RenderingState, Weak};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

/// Per-pane timing bookkeeping for the gate measurements.
#[derive(Default)]
struct PaneTiming {
    requested: Option<Instant>,
    first_batch: Option<Instant>,
    first_frame_reported: bool,
}

struct State {
    app: Weak<App>,
    panes: [Rc<PaneModel>; 2],
    rx: Receiver<Msg>,
    tx: Sender<Msg>,
    timing: [PaneTiming; 2],
    split_requested: Option<Instant>,
    busy_timer: slint::Timer,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(s.borrow_mut().as_mut().expect("state initialised")))
}

/// Wake the UI thread so it drains the channel. Called from worker threads.
fn wake() {
    let _ = slint::invoke_from_event_loop(drain);
}

fn navigate(pane: usize, path: PathBuf) {
    with_state(|st| {
        let app = st.app.unwrap();
        let model = &st.panes[pane];
        let generation = model.generation.get() + 1;
        model.generation.set(generation);
        model.clear();
        st.timing[pane] = PaneTiming { requested: Some(Instant::now()), ..Default::default() };
        let shown = path.to_string_lossy().to_string();
        if pane == 0 {
            app.set_left_path(shown.as_str().into());
            app.set_left_status("loading...".into());
        } else {
            app.set_right_path(shown.as_str().into());
            app.set_right_status("loading...".into());
        }
        app.invoke_reset_scroll(pane as i32);
        eprintln!("[nav] pane={pane} gen={generation} path={shown}");
        let tx = st.tx.clone();
        fs::enumerate::spawn(path, pane, generation, tx, wake);
    });
}

fn set_status(app: &App, pane: usize, s: String) {
    if pane == 0 { app.set_left_status(s.into()) } else { app.set_right_status(s.into()) }
}

fn drain() {
    with_state(|st| {
        let app = st.app.unwrap();
        while let Ok(msg) = st.rx.try_recv() {
            match msg {
                Msg::Batch { pane, generation, entries } => {
                    let model = &st.panes[pane];
                    if model.generation.get() != generation {
                        continue;
                    }
                    model.append(entries);
                    let t = &mut st.timing[pane];
                    if t.first_batch.is_none() {
                        t.first_batch = Some(Instant::now());
                        if let Some(r) = t.requested {
                            eprintln!("[enum] pane={pane} first batch after {:.1} ms", perf::elapsed_ms(r));
                        }
                    }
                    set_status(&app, pane, format!("{} items (loading...)", model.len()));
                }
                Msg::Done { pane, generation, total, elapsed, error } => {
                    let model = &st.panes[pane];
                    if model.generation.get() != generation {
                        continue;
                    }
                    let wall = st.timing[pane].requested.map(perf::elapsed_ms).unwrap_or(0.0);
                    match error {
                        Some(e) => set_status(&app, pane, format!("error: {e}")),
                        None => {
                            eprintln!(
                                "[enum] pane={pane} done: {total} entries, worker {:.1} ms, request-to-done {wall:.1} ms",
                                elapsed.as_secs_f64() * 1e3
                            );
                            set_status(&app, pane, format!("{total} items, enumerated in {:.0} ms", wall));
                            // Apply the pane's current sort once the listing is complete.
                            request_sort(st, pane, model.sort_col.get(), model.sort_asc.get());
                        }
                    }
                }
            }
        }
    });
}

/// Sort on a worker over a snapshot; swap the Vec in one step when it returns.
fn request_sort(st: &mut State, pane: usize, col: SortCol, asc: bool) {
    let model = &st.panes[pane];
    model.sort_col.set(col);
    model.sort_asc.set(asc);
    let app = st.app.unwrap();
    if pane == 0 {
        app.set_left_sort_col(col as i32);
        app.set_left_sort_asc(asc);
    } else {
        app.set_right_sort_col(col as i32);
        app.set_right_sort_asc(asc);
    }
    let generation = model.generation.get();
    let mut snapshot = model.snapshot();
    let t0 = Instant::now();
    std::thread::spawn(move || {
        sort_entries(&mut snapshot, col, asc);
        let sort_ms = perf::elapsed_ms(t0);
        let _ = slint::invoke_from_event_loop(move || {
            with_state(|st| {
                let model = &st.panes[pane];
                if model.generation.get() != generation || model.len() != snapshot.len() {
                    return; // listing changed underneath us; a newer sort or Done will follow
                }
                eprintln!("[sort] pane={pane} {} entries by {col:?} asc={asc} in {sort_ms:.1} ms", snapshot.len());
                model.replace(snapshot);
            });
        });
    });
}

fn on_frame_rendered() {
    with_state(|st| {
        for pane in 0..2 {
            let t = &mut st.timing[pane];
            if t.first_batch.is_some() && !t.first_frame_reported {
                t.first_frame_reported = true;
                if let Some(r) = t.requested {
                    eprintln!("[enum] pane={pane} first screenful rendered {:.1} ms after request", perf::elapsed_ms(r));
                }
            }
        }
        if let Some(s) = st.split_requested {
            if st.panes[1].len() > 0 {
                st.split_requested = None;
                eprintln!("[split] second pane rendered with rows {:.1} ms after F3", perf::elapsed_ms(s));
            }
        }
    });
}

fn main() -> Result<(), slint::PlatformError> {
    let app = App::new()?;
    let panes: [Rc<PaneModel>; 2] = [Rc::new(PaneModel::default()), Rc::new(PaneModel::default())];
    app.set_left_rows(ModelRc::from(panes[0].clone()));
    app.set_right_rows(ModelRc::from(panes[1].clone()));
    let (tx, rx) = unbounded();
    STATE.with(|s| {
        *s.borrow_mut() = Some(State {
            app: app.as_weak(),
            panes,
            rx,
            tx,
            timing: Default::default(),
            split_requested: None,
            busy_timer: slint::Timer::default(),
        })
    });

    match app.window().set_rendering_notifier(|state, _| match state {
        RenderingState::BeforeRendering => perf::before_rendering(),
        RenderingState::AfterRendering => {
            perf::after_rendering();
            on_frame_rendered();
        }
        _ => {}
    }) {
        Ok(()) => {}
        Err(e) => eprintln!("[perf] rendering notifier unavailable: {e:?} (frame stats disabled)"),
    }
    let report_timer = slint::Timer::default();
    report_timer.start(slint::TimerMode::Repeated, std::time::Duration::from_secs(5), perf::report);

    app.on_navigate(|pane, path| navigate(pane as usize, PathBuf::from(path.as_str())));
    app.on_open_row(|pane, row| {
        let target = with_state(|st| {
            let e = st.panes[pane as usize].entry(row as usize)?;
            if !e.is_dir {
                return None;
            }
            let app = st.app.unwrap();
            let base = if pane == 0 { app.get_left_path() } else { app.get_right_path() };
            Some(Path::new(base.as_str()).join(e.name.as_str()))
        });
        if let Some(p) = target {
            navigate(pane as usize, p);
        }
    });
    app.on_go_up(|pane| {
        let target = with_state(|st| {
            let app = st.app.unwrap();
            let base = if pane == 0 { app.get_left_path() } else { app.get_right_path() };
            Path::new(base.as_str()).parent().map(|p| p.to_path_buf())
        });
        if let Some(p) = target {
            navigate(pane as usize, p);
        }
    });
    app.on_sort_by(|pane, col| {
        with_state(|st| {
            let model = &st.panes[pane as usize];
            let col = SortCol::from_i32(col);
            let asc = if model.sort_col.get() == col { !model.sort_asc.get() } else { true };
            request_sort(st, pane as usize, col, asc);
        });
    });
    app.on_toggle_split({
        let w = app.as_weak();
        move || {
            let app = w.unwrap();
            let now = !app.get_split();
            app.set_split(now);
            if now {
                with_state(|st| st.split_requested = Some(perf::mark("F3: split requested")));
                let busy = perf::BusyWindow::start("10 s after split");
                with_state(|st| {
                    st.busy_timer.start(slint::TimerMode::SingleShot, std::time::Duration::from_secs(10), move || {
                        busy.finish();
                    })
                });
                navigate(1, PathBuf::from(app.get_left_path().as_str()));
                app.invoke_focus_pane(1);
            } else {
                app.invoke_focus_pane(0);
            }
        }
    });

    let start = std::env::args().nth(1).unwrap_or_else(|| "C:\\".to_string());
    app.set_right_path(start.as_str().into());
    navigate(0, PathBuf::from(start));
    auto::start(&app);

    app.run()
}
