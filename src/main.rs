// twinfm: Slint bootstrap, wires model <-> UI, owns the two panes.
slint::include_modules!();

mod fs {
    pub mod enumerate;
}
mod auto;
mod icons;
mod model;
mod perf;

use crossbeam_channel::{Receiver, Sender, unbounded};
use fs::enumerate::{Msg, THIS_PC, is_this_pc};
use icons::IconTable;
use model::{Kind, PaneModel, SelOp, SortCol, filter_sort};
use slint::{ComponentHandle, Image, ModelRc, RenderingState, Weak};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Rows above and below the viewport whose icons are fetched ahead of a scroll.
const ICON_MARGIN_ROWS: usize = 10;

/// UI-side icon counters for the `[icons]` line.
#[derive(Default)]
struct IconCounts {
    requests: u64,
    replies: u64,
    resolves: u64,
    resolve_ns: u64,
    /// Icon strips composed, and the time spent composing them (fitted buffers plus copies).
    composes: u64,
    compose_ns: u64,
    /// (requests, replies, worker renders, worker ns, composes) at the last report.
    reported: (u64, u64, u64, u64, u64),
}

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
    icons: Rc<RefCell<IconTable>>,
    icon_tx: Sender<icons::Request>,
    icon_rx: Receiver<icons::Reply>,
    /// Each pane's navigation generation, read by the icon worker to skip stale requests.
    icon_gens: Arc<[AtomicU64; 2]>,
    icon_stats: Arc<icons::Stats>,
    icon_counts: IconCounts,
    /// The (content-y, visible-height) each list last reported, in logical px.
    view: [(f32, f32); 2],
    /// What each pane's icon strip was last composed from (None: hidden), and whether its
    /// rows changed since, so the next resolve recomposes it.
    strip: [Option<icons::Composed>; 2],
    strip_dirty: [bool; 2],
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
    let path = if is_this_pc(&path.to_string_lossy()) { PathBuf::from(THIS_PC) } else { path };
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let model = &st.panes[pane];
        let generation = model.generation.get() + 1;
        model.generation.set(generation);
        model.clear();
        st.icon_gens[pane].store(generation, Ordering::Relaxed);
        st.timing[pane] = PaneTiming { requested: Some(Instant::now()), ..Default::default() };
        st.paths[pane] = path.clone();
        st.keep_scroll[pane] = false;
        hide_strip(st, &app, pane);
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
        st.icon_gens[pane].store(generation, Ordering::Relaxed);
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
                st.strip_dirty[pane] = true;
                resolve_view(st, &app, pane);
            });
        });
    });
}

/// Fetch or apply icons for the rows on screen (plus a margin) in one pane, then recompose
/// its icon strip if the visible rows left it or an icon in it changed.
fn resolve_view(st: &mut State, app: &App, pane: usize) {
    if pane == 1 && !app.get_split() {
        return;
    }
    let model = &st.panes[pane];
    let (y, h) = st.view[pane];
    let Some(visible) = icons::visible_range(y, h, model.len()) else {
        hide_strip(st, app, pane);
        return;
    };
    let (first, last) = visible;
    let t0 = Instant::now();
    let geom = icons::StripGeom::new(app.window().scale_factor());
    let size_px = geom.icon_px;
    let generation = model.generation.get();
    let (folder, tx) = (&st.paths[pane], &st.icon_tx);
    let mut sent = 0;
    let changed = model.resolve_icons(first.saturating_sub(ICON_MARGIN_ROWS), last + ICON_MARGIN_ROWS, |e| {
        let path = if e.kind == Kind::Drive { PathBuf::from(e.name.as_str()) } else { folder.join(e.name.as_str()) };
        let _ = tx.send(icons::Request { generation, pane, key: e.icon_key.clone(), path, size_px });
        sent += 1;
    });
    if let Some(c) = st.strip[pane] {
        if changed.iter().any(|&i| (c.first..=c.last).contains(&i)) {
            st.strip_dirty[pane] = true;
        }
    }
    let c = &mut st.icon_counts;
    c.requests += sent;
    c.resolves += 1;
    c.resolve_ns += t0.elapsed().as_nanos() as u64;
    if icons::needs_recompose(st.strip[pane], visible, model.len(), geom.icon_px, st.strip_dirty[pane]) {
        compose_strip(st, app, pane, visible, &geom);
    }
}

/// Compose the pane's icon strip for the visible rows plus the strip margin and hand it to
/// the pane, which places it at its first row.
fn compose_strip(st: &mut State, app: &App, pane: usize, visible: (usize, usize), geom: &icons::StripGeom) {
    let model = &st.panes[pane];
    let (first, last) = icons::strip_range(visible, model.len());
    let t0 = Instant::now();
    let bufs: Vec<_> = {
        let mut table = st.icons.borrow_mut();
        model.icon_indices(first, last).into_iter().map(|i| table.fitted(i, geom.icon_px)).collect()
    };
    let strip = icons::compose(&bufs, geom);
    st.icon_counts.composes += 1;
    st.icon_counts.compose_ns += t0.elapsed().as_nanos() as u64;
    let (w, h) = (strip.width() as f32 / geom.scale, strip.height() as f32 / geom.scale);
    app.invoke_set_strip(pane as i32, Image::from_rgba8_premultiplied(strip), first as i32, w, h, true);
    st.strip[pane] = Some(icons::Composed { first, last, icon_px: geom.icon_px });
    st.strip_dirty[pane] = false;
}

/// Hide the pane's icon strip (empty list, or a navigation under way).
fn hide_strip(st: &mut State, app: &App, pane: usize) {
    if st.strip[pane].take().is_some() {
        app.invoke_set_strip(pane as i32, Image::default(), 0, 0.0, 0.0, false);
    }
}

/// A list scrolled, resized or changed length. Called from Slint `changed` handlers; if the
/// state is borrowed at that moment, retry from the event loop rather than drop the update.
fn on_viewport(pane: usize, y: f32, h: f32) {
    let busy = STATE.with(|s| {
        let Ok(mut guard) = s.try_borrow_mut() else { return true };
        let Some(st) = guard.as_mut() else { return false };
        st.view[pane] = (y, h);
        if let Some(app) = st.app.upgrade() {
            resolve_view(st, &app, pane);
        }
        false
    });
    if busy {
        slint::Timer::single_shot(Duration::ZERO, move || on_viewport(pane, y, h));
    }
}

/// Take every icon reply the worker has sent, intern the images, then re-resolve both panes'
/// visible rows once (one walk over a screenful per pane, however many replies arrived).
fn drain_icons() {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let mut any = false;
        {
            let mut table = st.icons.borrow_mut();
            while let Ok(r) = st.icon_rx.try_recv() {
                table.complete(r.key, r.outcome);
                st.icon_counts.replies += 1;
                any = true;
            }
        }
        if any {
            resolve_view(st, &app, 0);
            resolve_view(st, &app, 1);
        }
    });
}

/// Once a second, when icon work happened: requests sent, replies received, table size, the
/// worker's average time per icon, the UI's average resolve time, and the strips composed
/// (in total) with their average compose time.
fn report_icons() {
    with_state(|st| {
        let c = &mut st.icon_counts;
        let rendered = st.icon_stats.rendered.load(Ordering::Relaxed);
        let render_ns = st.icon_stats.render_ns.load(Ordering::Relaxed);
        let (req0, rep0, ren0, ns0, comp0) = c.reported;
        if c.requests == req0 && c.replies == rep0 && c.composes == comp0 {
            return;
        }
        let n = rendered - ren0;
        eprintln!(
            "[icons] +{} requests +{} replies (total {} / {}), table {} images, worker {n} rendered avg {:.2} ms/icon, resolve {} calls avg {:.1} us, compose {} strips avg {:.1} us",
            c.requests - req0,
            c.replies - rep0,
            c.requests,
            c.replies,
            st.icons.borrow().len(),
            if n > 0 { (render_ns - ns0) as f64 / n as f64 / 1e6 } else { 0.0 },
            c.resolves,
            if c.resolves > 0 { c.resolve_ns as f64 / c.resolves as f64 / 1e3 } else { 0.0 },
            c.composes,
            if c.composes > 0 { c.compose_ns as f64 / c.composes as f64 / 1e3 } else { 0.0 },
        );
        c.reported = (c.requests, c.replies, rendered, render_ns, c.composes);
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
    let icons = Rc::new(RefCell::new(IconTable::default()));
    let panes: [Rc<PaneModel>; 2] =
        [Rc::new(PaneModel::with_icons(icons.clone())), Rc::new(PaneModel::with_icons(icons.clone()))];
    app.set_left_rows(ModelRc::from(panes[0].clone()));
    app.set_right_rows(ModelRc::from(panes[1].clone()));
    let (tx, rx) = unbounded();
    let (icon_tx, icon_requests) = unbounded();
    let (icon_replies, icon_rx) = unbounded();
    let icon_gens = Arc::new([AtomicU64::new(0), AtomicU64::new(0)]);
    let icon_stats = Arc::new(icons::Stats::default());
    icons::spawn(icon_requests, icon_replies, icon_gens.clone(), icon_stats.clone(), || {
        let _ = slint::invoke_from_event_loop(drain_icons);
    });
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
            icons,
            icon_tx,
            icon_rx,
            icon_gens,
            icon_stats,
            icon_counts: IconCounts::default(),
            view: [(0.0, 0.0); 2],
            strip: [None; 2],
            strip_dirty: [false; 2],
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
    let icon_timer = slint::Timer::default();
    icon_timer.start(slint::TimerMode::Repeated, Duration::from_secs(1), report_icons);
    app.on_viewport_changed(|pane, y, h| on_viewport(pane as usize, y, h));

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
            let base = pane_path(&app, pane as usize);
            if is_this_pc(&base) {
                return None;
            }
            // A drive root (or UNC share root) has no parent: go up to the drive list.
            Some(Path::new(base.as_str()).parent().map_or_else(|| PathBuf::from(THIS_PC), |p| p.to_path_buf()))
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
    // Start with keyboard focus in the list, so arrows, Space and Ctrl+A work before a click.
    app.invoke_focus_pane(0);
    auto::start(&app);

    app.run()
}
