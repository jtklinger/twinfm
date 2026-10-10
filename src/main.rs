// twinfm: Slint bootstrap, wires model <-> UI, owns the two panes.
slint::include_modules!();

mod fs {
    pub mod enumerate;
}
mod auto;
mod model;
mod perf;

use crossbeam_channel::{Receiver, Sender, unbounded};
use fs::enumerate::Msg;
use model::{PaneModel, SelOp, SortCol, filter_sort};
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
    /// Request-to-done wall time once the listing is complete.
    done_ms: Option<f64>,
    error: Option<String>,
}

struct State {
    app: Weak<App>,
    panes: [Rc<PaneModel>; 2],
    rx: Receiver<Msg>,
    tx: Sender<Msg>,
    timing: [PaneTiming; 2],
    /// The folder each pane last navigated to (the path bar may hold unsubmitted text).
    paths: [PathBuf; 2],
    /// Restore the list's scroll offset at the next pass swap (set when a refresh completes).
    keep_scroll: [bool; 2],
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
        let Some(app) = st.app.upgrade() else { return };
        let model = &st.panes[pane];
        let generation = model.generation.get() + 1;
        model.generation.set(generation);
        model.clear();
        st.timing[pane] = PaneTiming { requested: Some(Instant::now()), ..Default::default() };
        st.paths[pane] = path.clone();
        st.keep_scroll[pane] = false;
        let shown = path.to_string_lossy().to_string();
        if pane == 0 {
            app.set_left_path(shown.as_str().into());
        } else {
            app.set_right_path(shown.as_str().into());
        }
        set_status(&app, pane, "loading...".into());
        app.invoke_reset_scroll(pane as i32);
        eprintln!("[nav] pane={pane} gen={generation} path={shown}");
        let tx = st.tx.clone();
        fs::enumerate::spawn(path, pane, generation, tx, wake);
    });
}

/// Re-enumerate the pane's folder in place (F5, Ctrl+R). The old rows stay on screen until the
/// new listing has been filtered and sorted; cursor, selection and scroll come back by name.
fn refresh(pane: usize) {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let model = &st.panes[pane];
        let generation = model.generation.get() + 1;
        model.generation.set(generation);
        model.begin_refresh();
        st.timing[pane] = PaneTiming { requested: Some(Instant::now()), ..Default::default() };
        set_status(&app, pane, "refreshing...".into());
        let path = st.paths[pane].clone();
        eprintln!("[refresh] pane={pane} gen={generation} path={}", path.display());
        fs::enumerate::spawn(path, pane, generation, st.tx.clone(), wake);
    });
}

fn set_status(app: &App, pane: usize, s: String) {
    if pane == 0 { app.set_left_status(s.into()) } else { app.set_right_status(s.into()) }
}

/// The status line for a pane: loading, complete, or failed.
fn status_text(model: &PaneModel, t: &PaneTiming) -> String {
    if let Some(e) = &t.error {
        return format!("error: {e}");
    }
    let hidden = match model.hidden_count() {
        0 => String::new(),
        n => format!(" ({n} hidden)"),
    };
    match t.done_ms {
        Some(ms) => format!("{} items{hidden}, enumerated in {ms:.0} ms", model.len()),
        None => format!("{} items{hidden} (loading...)", model.len()),
    }
}

fn drain() {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        while let Ok(msg) = st.rx.try_recv() {
            match msg {
                Msg::Batch { pane, generation, entries } => {
                    let model = &st.panes[pane];
                    if model.generation.get() != generation {
                        continue;
                    }
                    model.append(entries);
                    if model.is_refreshing() {
                        continue; // staged; the old rows and status stay until the pass
                    }
                    let t = &mut st.timing[pane];
                    if t.first_batch.is_none() {
                        t.first_batch = Some(Instant::now());
                        if let Some(r) = t.requested {
                            eprintln!("[enum] pane={pane} first batch after {:.1} ms", perf::elapsed_ms(r));
                        }
                    }
                    set_status(&app, pane, status_text(model, t));
                }
                Msg::Done { pane, generation, total, elapsed, error } => {
                    let model = &st.panes[pane];
                    if model.generation.get() != generation {
                        continue;
                    }
                    let wall = st.timing[pane].requested.map(perf::elapsed_ms).unwrap_or(0.0);
                    if model.is_refreshing() {
                        model.finish_refresh();
                        st.keep_scroll[pane] = true;
                    }
                    match error {
                        Some(e) => {
                            st.timing[pane].error = Some(e);
                            set_status(&app, pane, status_text(model, &st.timing[pane]));
                        }
                        None => {
                            eprintln!(
                                "[enum] pane={pane} done: {total} entries, worker {:.1} ms, request-to-done {wall:.1} ms",
                                elapsed.as_secs_f64() * 1e3
                            );
                            st.timing[pane].done_ms = Some(wall);
                            set_status(&app, pane, status_text(model, &st.timing[pane]));
                            // Apply the pane's current filter and sort once the listing is complete.
                            request_pass(st, pane);
                        }
                    }
                }
            }
        }
    });
}

/// Filter and sort on a worker over a snapshot of the full listing; swap the shown Vec in one
/// step when it returns. Uses the pane's current sort column, direction and hidden filter.
fn request_pass(st: &mut State, pane: usize) {
    let Some(app) = st.app.upgrade() else { return };
    let model = &st.panes[pane];
    let (col, asc, show_hidden) = (model.sort_col.get(), model.sort_asc.get(), model.show_hidden.get());
    if pane == 0 {
        app.set_left_sort_col(col as i32);
        app.set_left_sort_asc(asc);
    } else {
        app.set_right_sort_col(col as i32);
        app.set_right_sort_asc(asc);
    }
    let generation = model.generation.get();
    let pass = model.pass.get() + 1;
    model.pass.set(pass);
    let snapshot = model.snapshot_all();
    let restore = model.capture(current_row(&app, pane));
    let sel_version = model.sel_version.get();
    let t0 = Instant::now();
    std::thread::spawn(move || {
        let total = snapshot.len();
        let mut out = filter_sort(snapshot, show_hidden, col, asc, &restore);
        let pass_ms = perf::elapsed_ms(t0);
        let _ = slint::invoke_from_event_loop(move || {
            with_state(|st| {
                let Some(app) = st.app.upgrade() else { return };
                let model = &st.panes[pane];
                if model.generation.get() != generation || model.pass.get() != pass || model.total() != total {
                    return; // listing or request changed underneath us; a newer pass or Done follows
                }
                if model.sel_version.get() != sel_version {
                    // The user changed the selection while the pass ran; carry the current one.
                    out.restore(&model.capture(current_row(&app, pane)));
                }
                eprintln!(
                    "[sort] pane={pane} {} of {total} entries by {col:?} asc={asc} hidden={} cursor={:?} selected={} in {pass_ms:.1} ms",
                    out.shown.len(),
                    out.hidden,
                    out.cursor,
                    out.selection.len()
                );
                let scroll = std::mem::take(&mut st.keep_scroll[pane]).then(|| app.invoke_get_scroll(pane as i32));
                let cursor = model.replace(out);
                set_current_row(&app, pane, cursor);
                if let (Some(y), Some(_)) = (scroll, cursor) {
                    app.invoke_set_scroll(pane as i32, y);
                    eprintln!("[refresh] pane={pane} scroll restored to {y:.0} (now {:.0})", app.invoke_get_scroll(pane as i32));
                }
                set_status(&app, pane, status_text(model, &st.timing[pane]));
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

fn current_row(app: &App, pane: usize) -> i32 {
    if pane == 0 { app.get_left_current_row() } else { app.get_right_current_row() }
}

fn set_current_row(app: &App, pane: usize, row: Option<usize>) {
    let row = row.map_or(-1, |r| r as i32);
    if pane == 0 { app.set_left_current_row(row) } else { app.set_right_current_row(row) }
}

/// The path shown in a pane's path bar.
fn pane_path(app: &App, pane: usize) -> slint::SharedString {
    if pane == 0 { app.get_left_path() } else { app.get_right_path() }
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
            paths: Default::default(),
            keep_scroll: [false; 2],
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
            if !e.is_dir() {
                return None;
            }
            let app = st.app.upgrade()?;
            Some(Path::new(pane_path(&app, pane as usize).as_str()).join(e.name.as_str()))
        });
        if let Some(p) = target {
            navigate(pane as usize, p);
        }
    });
    app.on_go_up(|pane| {
        let target = with_state(|st| {
            let app = st.app.upgrade()?;
            Path::new(pane_path(&app, pane as usize).as_str()).parent().map(|p| p.to_path_buf())
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
            model.sort_col.set(col);
            model.sort_asc.set(asc);
            request_pass(st, pane as usize);
        });
    });
    app.on_select(|pane, op, row| {
        let op = SelOp::from_i32(op);
        if row < 0 && op != SelOp::All {
            return;
        }
        with_state(|st| {
            let model = &st.panes[pane as usize];
            model.select(op, row.max(0) as usize);
            eprintln!("[select] pane={pane} {op:?} row={row} selected={}", model.selected_count());
        });
    });
    app.on_refresh(|pane| refresh(pane as usize));
    app.on_toggle_hidden(|pane| {
        with_state(|st| {
            let model = &st.panes[pane as usize];
            model.show_hidden.set(!model.show_hidden.get());
            eprintln!("[hidden] pane={pane} show_hidden={}", model.show_hidden.get());
            request_pass(st, pane as usize);
        });
    });
    app.on_toggle_split({
        let w = app.as_weak();
        move || {
            let Some(app) = w.upgrade() else { return };
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
