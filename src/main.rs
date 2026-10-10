// twinfm: Slint bootstrap, wires model <-> UI, owns the two panes.
slint::include_modules!();

mod fs {
    pub mod enumerate;
}
mod auto;
mod dnd;
mod fileops;
mod icons;
mod model;
mod perf;
mod shellmenu;

use crossbeam_channel::{Receiver, Sender, unbounded};
use fileops::{Job, Outcome, clipboard, same_dir};
use fs::enumerate::{Msg, THIS_PC, is_this_pc};
use icons::IconTable;
use model::{Kind, PaneModel, SelOp, SortCol, filter_sort};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::{ComponentHandle, Image, ModelRc, RenderingState, SharedString, Weak};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_LBUTTON};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXDRAG, SM_CYDRAG};

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
    /// The file operation worker (STA thread, owns every IFileOperation).
    ops: fileops::Worker,
    /// The main window's HWND once it exists (0 before): owner of the shell's dialogs and of
    /// the clipboard.
    hwnd: isize,
    next_job: u64,
    /// Jobs in flight: id to source pane.
    jobs: HashMap<u64, usize>,
    /// The paste-of-a-cut job, if one is in flight: on success the clipboard is emptied.
    paste_move: Option<u64>,
    /// Per pane, the last file operation's message, shown before the item count.
    notes: [String; 2],
    /// Per pane, a name for the next pass to put the cursor and selection on (after a rename or
    /// a new folder).
    focus_name: [Option<SharedString>; 2],
    /// TWINFM_SOAK=1: no confirmations, and every path must lie inside one of these scratch
    /// folders (`%TEMP%\twinfm-m3` for the M3 soak, `%TEMP%\twinfm-m5` for the M5 one).
    soak: Option<Vec<PathBuf>>,
    /// What each pane's icon strip was last composed from (None: hidden), and whether its
    /// rows changed since, so the next resolve recomposes it.
    strip: [Option<icons::Composed>; 2],
    strip_dirty: [bool; 2],
    /// Each pane's left-button gesture on a row (click, drag, double-click), dnd::Gesture.
    gesture: [dnd::Gesture; 2],
    /// Retries the drop target registration until the window exists, then stops.
    dnd_timer: slint::Timer,
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
        st.notes[pane].clear();
        st.focus_name[pane] = None;
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

/// The status line for a pane: loading, complete, or failed, after the pane's note (the last
/// file operation's result) when there is one.
fn status_text(model: &PaneModel, t: &PaneTiming, note: &str) -> String {
    if !note.is_empty() {
        return format!("{note} | {}", status_text(model, t, ""));
    }
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
                    set_status(&app, pane, status_text(model, t, &st.notes[pane]));
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
                            set_status(&app, pane, status_text(model, &st.timing[pane], &st.notes[pane]));
                        }
                        None => {
                            eprintln!(
                                "[enum] pane={pane} done: {total} entries, worker {:.1} ms, request-to-done {wall:.1} ms",
                                elapsed.as_secs_f64() * 1e3
                            );
                            st.timing[pane].done_ms = Some(wall);
                            set_status(&app, pane, status_text(model, &st.timing[pane], &st.notes[pane]));
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
    let mut restore = model.capture(current_row(&app, pane));
    if let Some(name) = st.focus_name[pane].take() {
        restore.cursor = Some(name.clone());
        restore.selected = vec![name];
    }
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
                set_status(&app, pane, status_text(model, &st.timing[pane], &st.notes[pane]));
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

/// The main window's HWND, fetched once the window exists.
fn main_hwnd(st: &mut State, app: &App) -> isize {
    if st.hwnd == 0 {
        let handle = app.window().window_handle();
        if let Ok(h) = handle.window_handle() {
            if let RawWindowHandle::Win32(w) = h.as_raw() {
                st.hwnd = w.hwnd.get();
            }
        }
    }
    st.hwnd
}

/// Show `text` as the pane's note now.
fn note(st: &mut State, app: &App, pane: usize, text: String) {
    eprintln!("[fileops] pane={pane} note: {text}");
    st.notes[pane] = text;
    set_status(app, pane, status_text(&st.panes[pane], &st.timing[pane], &st.notes[pane]));
}

/// `p` is `root` or lies under it (case-insensitive, whole components).
fn inside(p: &Path, root: &Path) -> bool {
    let (p, r) = (p.to_string_lossy().to_lowercase(), root.to_string_lossy().trim_end_matches('\\').to_lowercase());
    p == r || p.strip_prefix(&r).is_some_and(|rest| rest.starts_with('\\'))
}

/// Full paths of the selection (or the cursor row) in a pane, or a note saying why there are none.
fn sources(st: &mut State, app: &App, pane: usize) -> Option<(Vec<PathBuf>, Vec<SharedString>)> {
    let targets = st.panes[pane].targets(current_row(app, pane));
    if targets.is_empty() {
        note(st, app, pane, "nothing selected".into());
        return None;
    }
    if targets.iter().any(|e| e.kind == Kind::Drive) {
        note(st, app, pane, "drives cannot be copied, moved, renamed or deleted".into());
        return None;
    }
    let folder = &st.paths[pane];
    Some((targets.iter().map(|e| folder.join(e.name.as_str())).collect(), targets.into_iter().map(|e| e.name).collect()))
}

/// Queue a job on the worker; the source pane shows its progress note. Returns the job id.
fn submit(st: &mut State, app: &App, pane: usize, job: Job) -> Option<u64> {
    if let Some(roots) = &st.soak {
        if let Some(p) = job.paths().into_iter().find(|p| !roots.iter().any(|r| inside(p, r))) {
            let shown: Vec<String> = roots.iter().map(|r| r.display().to_string()).collect();
            let text = format!("soak: refused {} (outside {})", p.display(), shown.join(" and "));
            note(st, app, pane, text);
            return None;
        }
    }
    st.next_job += 1;
    let id = st.next_job;
    let hwnd = main_hwnd(st, app);
    let running = fileops::running_text(&job);
    eprintln!("[fileops] submit id={id} pane={pane} hwnd={hwnd:#x} {job:?}");
    if !st.ops.submit(fileops::Request { id, job, hwnd, no_confirm: st.soak.is_some() }) {
        note(st, app, pane, "the file operation worker has stopped".into());
        return None;
    }
    st.jobs.insert(id, pane);
    note(st, app, pane, running);
    Some(id)
}

/// A file operation key in a pane's list. Copy and move go to the other pane's folder; rename
/// and new folder open the inline name editor, which commits through `edit_commit`.
fn command(pane: usize, cmd: &str) {
    match cmd {
        "cc" | "cx" => return clipboard_put(pane, cmd == "cx"),
        "cv" => return clipboard_paste(pane),
        _ => {}
    }
    let edit = with_state(|st| {
        let app = st.app.upgrade()?;
        let folder = st.paths[pane].clone();
        let at_drives = is_this_pc(&folder.to_string_lossy());
        match cmd {
            "newfolder" if at_drives => note(st, &app, pane, "This PC lists drives; open a folder first".into()),
            "newfolder" => return Some((app, 2, SharedString::from("New folder"))),
            "rename" => match usize::try_from(current_row(&app, pane)).ok().and_then(|r| st.panes[pane].entry(r)) {
                None => note(st, &app, pane, "nothing to rename".into()),
                Some(e) if e.kind == Kind::Drive => note(st, &app, pane, "drives cannot be renamed here".into()),
                Some(e) => return Some((app, 1, e.name)),
            },
            "copy" | "move" => {
                if !app.get_split() {
                    note(st, &app, pane, "open the second pane (F3) first".into());
                    return None;
                }
                let dest = st.paths[1 - pane].clone();
                if is_this_pc(&dest.to_string_lossy()) {
                    note(st, &app, pane, "the other pane lists drives; open a folder there first".into());
                    return None;
                }
                if cmd == "move" && same_dir(&folder, &dest) {
                    note(st, &app, pane, "source and destination are the same folder".into());
                    return None;
                }
                let (sources, _) = sources(st, &app, pane)?;
                let job = if cmd == "move" { Job::Move { sources, dest_dir: dest } } else { Job::Copy { sources, dest_dir: dest } };
                submit(st, &app, pane, job);
            }
            "delete" | "sdelete" => {
                let (sources, _) = sources(st, &app, pane)?;
                submit(st, &app, pane, Job::Delete { sources, permanent: cmd == "sdelete" });
            }
            _ => {}
        }
        None
    });
    // Outside the state borrow: opening the editor moves keyboard focus.
    if let Some((app, mode, text)) = edit {
        app.invoke_begin_edit(pane as i32, mode, text);
    }
}

/// The inline name editor's Return: rename the row or create the folder.
fn edit_commit(pane: usize, mode: i32, original: &str, typed: &str) {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let name = typed.trim();
        if let Some(why) = fileops::invalid_name(name) {
            note(st, &app, pane, why.into());
            return;
        }
        let folder = st.paths[pane].clone();
        let job = match mode {
            1 if name == original => return,
            1 => Job::Rename { source: folder.join(original), new_name: name.into() },
            2 => Job::NewFolder { parent: folder, name: name.into() },
            _ => return,
        };
        submit(st, &app, pane, job);
    });
}

/// Ctrl+C / Ctrl+X: the selection onto the real clipboard, as Explorer writes it. A cut dims
/// its rows until the paste (or the next copy or cut).
fn clipboard_put(pane: usize, cut: bool) {
    let Some((hwnd, paths, names)) = with_state(|st| {
        let app = st.app.upgrade()?;
        let (paths, names) = sources(st, &app, pane)?;
        Some((main_hwnd(st, &app), paths, names))
    }) else {
        return;
    };
    let effect = if cut { clipboard::EFFECT_MOVE } else { clipboard::EFFECT_COPY };
    // No state borrow across the clipboard calls (see fileops::clipboard).
    let res = clipboard::write(hwnd, &paths, effect);
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        match res {
            Ok(()) => {
                let n = paths.len();
                let what = if cut { "Cut" } else { "Copied" };
                note(st, &app, pane, format!("{what} {n} item{} to the clipboard", if n == 1 { "" } else { "s" }));
                for p in 0..2 {
                    st.panes[p].set_cut(if cut && p == pane { names.clone() } else { Vec::new() });
                }
            }
            Err(e) => note(st, &app, pane, format!("clipboard: {e}")),
        }
    });
}

/// Ctrl+V: copy or move the clipboard's files into the pane's folder ("Preferred DropEffect"
/// decides; copy when absent).
fn clipboard_paste(pane: usize) {
    let Some(hwnd) = with_state(|st| {
        let app = st.app.upgrade()?;
        if is_this_pc(&st.paths[pane].to_string_lossy()) {
            note(st, &app, pane, "This PC lists drives; open a folder first".into());
            return None;
        }
        Some(main_hwnd(st, &app))
    }) else {
        return;
    };
    let res = clipboard::read(hwnd);
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let (sources, effect) = match res {
            Ok(Some((s, e))) if !s.is_empty() => (s, e),
            Ok(_) => return note(st, &app, pane, "no files on the clipboard".into()),
            Err(e) => return note(st, &app, pane, format!("clipboard: {e}")),
        };
        let dest = st.paths[pane].clone();
        if effect == clipboard::EFFECT_MOVE {
            if sources.iter().all(|s| s.parent().is_some_and(|d| same_dir(d, &dest))) {
                return note(st, &app, pane, "source and destination are the same folder".into());
            }
            st.paste_move = submit(st, &app, pane, Job::Move { sources, dest_dir: dest });
        } else {
            submit(st, &app, pane, Job::Copy { sources, dest_dir: dest });
        }
    });
}

/// A job finished (on the UI thread, from the worker's wake-up): show the result on the source
/// pane and refresh both panes. Moved and deleted rows drop out of the selection because the
/// refresh restores it by name; a copy keeps it.
fn job_done(d: fileops::Done) {
    let mut clear_clipboard = None;
    let panes = with_state(|st| {
        let pane = st.jobs.remove(&d.id).unwrap_or(0);
        let text = fileops::status_text(&d.job, &d.outcome);
        eprintln!("[fileops] done id={} pane={pane} {:?}: {text}", d.id, d.outcome);
        if d.outcome == Outcome::Done {
            match &d.job {
                Job::Rename { new_name: n, .. } | Job::NewFolder { name: n, .. } => st.focus_name[pane] = Some(n.as_str().into()),
                _ => {}
            }
        }
        if st.paste_move == Some(d.id) {
            st.paste_move = None;
            if d.outcome == Outcome::Done {
                clear_clipboard = Some(st.hwnd);
                for m in &st.panes {
                    m.set_cut(Vec::new());
                }
            }
        }
        st.notes[pane] = text;
        let split = st.app.upgrade().is_some_and(|a| a.get_split());
        if split { vec![0, 1] } else { vec![0] }
    });
    if let Some(hwnd) = clear_clipboard {
        if let Err(e) = clipboard::clear(hwnd) {
            eprintln!("[fileops] clearing the clipboard after a cut-paste failed: {e}");
        }
    }
    for p in panes {
        refresh(p);
    }
}

/// What `context_menu` needs after the menu closes, taken out of the state before it opens.
struct MenuContext {
    hwnd: isize,
    /// The directory "Open in other pane" and "Open in terminal" use.
    dir: PathBuf,
    /// The full paths "Copy path" puts on the clipboard.
    paths: Vec<PathBuf>,
}

/// The shell context menu: for the selection (`row` >= 0; a right press has already applied
/// the right-click selection rule) or the folder background (`row` -1, or nothing to act on),
/// at window position (`x`, `y`) in logical px. `extended`: Shift was held.
fn context_menu(pane: usize, row: i32, x: f32, y: f32, extended: bool) {
    if shellmenu::is_open() {
        eprintln!("[menu] pane={pane} a menu is already open; ignored");
        return;
    }
    // Take everything the menu needs out of the state, then drop the borrow: the menu runs a
    // modal message loop (see shellmenu), and no RefCell borrow may be held across it.
    let taken = with_state(|st| {
        let app = st.app.upgrade()?;
        let hwnd = main_hwnd(st, &app);
        if hwnd == 0 {
            note(st, &app, pane, "context menu: the window has no handle yet".into());
            return None;
        }
        let folder = st.paths[pane].clone();
        let this_pc = is_this_pc(&folder.to_string_lossy());
        let targets = if row >= 0 { st.panes[pane].targets(current_row(&app, pane)) } else { Vec::new() };
        let names: Vec<String> = targets.iter().map(|e| e.name.to_string()).collect();
        let (own, dir, paths) = match targets.as_slice() {
            [] if this_pc => (Vec::new(), folder.clone(), Vec::new()),
            [] => (vec![shellmenu::OwnItem::OpenOther, shellmenu::OwnItem::Terminal, shellmenu::OwnItem::CopyPath], folder.clone(), vec![folder.clone()]),
            [one] if one.is_dir() => {
                let dir = folder.join(one.name.as_str());
                (vec![shellmenu::OwnItem::OpenOther, shellmenu::OwnItem::Terminal, shellmenu::OwnItem::CopyPath], dir.clone(), vec![dir])
            }
            many => (
                vec![shellmenu::OwnItem::Terminal, shellmenu::OwnItem::CopyPath],
                folder.clone(),
                many.iter().map(|e| folder.join(e.name.as_str())).collect(),
            ),
        };
        let (sx, sy) = shellmenu::to_screen(hwnd, x, y, app.window().scale_factor());
        eprintln!("[menu] pane={pane} row={row} {} target(s) at ({x:.0}, {y:.0}) -> screen ({sx}, {sy})", names.len());
        let req = shellmenu::Request { hwnd, folder, names, own, x: sx, y: sy, extended };
        Some((req, MenuContext { hwnd, dir, paths }))
    });
    let Some((req, ctx)) = taken else { return };
    let picked = shellmenu::show(&req);
    match picked {
        Err(e) => with_state(|st| {
            if let Some(app) = st.app.upgrade() {
                note(st, &app, pane, format!("context menu: {e}"));
            }
        }),
        Ok(shellmenu::Picked::Nothing) => {}
        Ok(shellmenu::Picked::Rename) => command(pane, "rename"),
        Ok(shellmenu::Picked::Own(item)) => own_item(pane, item, ctx),
        Ok(shellmenu::Picked::Invoked { verb, hr }) => {
            if hr < 0 && hr != fileops::HR_CANCELLED {
                with_state(|st| {
                    if let Some(app) = st.app.upgrade() {
                        note(st, &app, pane, format!("\"{verb}\" failed: 0x{:08X}", hr as u32));
                    }
                });
            }
            // The shell may have created, renamed or deleted entries in either folder.
            let split = with_state(|st| st.app.upgrade().is_some_and(|a| a.get_split()));
            refresh(0);
            if split {
                refresh(1);
            }
        }
    }
}

/// Shift+F10 or the Menu key in a pane's list: the menu for the cursor row's selection, under
/// that row (already scrolled into view), or the background menu when there is no cursor row.
fn context_key(pane: usize, row: i32, list_x: f32, list_y: f32, content_y: f32, visible_h: f32) {
    let len = with_state(|st| st.panes[pane].len());
    match usize::try_from(row).ok().filter(|&r| r < len) {
        Some(r) => {
            let (x, y) = shellmenu::row_menu_point(list_x, list_y, content_y, visible_h, r);
            context_menu(pane, row, x, y, false);
        }
        None => context_menu(pane, -1, list_x + 28.0, list_y, false),
    }
}

/// One of TwinFM's own menu items, after the menu closed.
fn own_item(pane: usize, item: shellmenu::OwnItem, ctx: MenuContext) {
    match item {
        shellmenu::OwnItem::OpenOther => {
            let Some(app) = with_state(|st| st.app.upgrade()) else { return };
            if !app.get_split() {
                app.invoke_toggle_split();
            }
            eprintln!("[menu] open in other pane: {}", ctx.dir.display());
            navigate(1 - pane, ctx.dir);
        }
        shellmenu::OwnItem::Terminal => {
            // CreateProcessW outside the state borrow, like every other call that can block.
            let res = shellmenu::open_terminal(&ctx.dir);
            with_state(|st| {
                let Some(app) = st.app.upgrade() else { return };
                match res {
                    Ok(cmd) => note(st, &app, pane, format!("Opened a terminal: {cmd}")),
                    Err(e) => note(st, &app, pane, format!("terminal: {e}")),
                }
            });
        }
        shellmenu::OwnItem::CopyPath => {
            // No state borrow across the clipboard calls (see fileops::clipboard).
            let res = clipboard::write_text(ctx.hwnd, &shellmenu::path_text(&ctx.paths));
            with_state(|st| {
                let Some(app) = st.app.upgrade() else { return };
                match res {
                    Ok(()) => {
                        // The text replaced any cut on the clipboard.
                        for m in &st.panes {
                            m.set_cut(Vec::new());
                        }
                        let n = ctx.paths.len();
                        note(st, &app, pane, format!("Copied {n} path{}", if n == 1 { "" } else { "s" }));
                    }
                    Err(e) => note(st, &app, pane, format!("clipboard: {e}")),
                }
            });
        }
    }
}

/// Enter the row's directory (Return, or a double-click that was not the tail of a drag).
fn open_row(pane: usize, row: i32) {
    let target = with_state(|st| {
        let e = st.panes[pane].entry(usize::try_from(row).ok()?)?;
        if !e.is_dir() {
            return None;
        }
        let app = st.app.upgrade()?;
        Some(Path::new(pane_path(&app, pane).as_str()).join(e.name.as_str()))
    });
    if let Some(p) = target {
        navigate(pane, p);
    }
}

fn log_select(model: &PaneModel, pane: usize, op: SelOp, row: usize) {
    eprintln!("[select] pane={pane} {op:?} row={row} selected={}", model.selected_count());
}

/// The left button on a row: press (0), release (1), moved past the drag threshold with the
/// button down (2), double-click (3). `dnd::Gesture` decides what selects, drags and opens.
fn row_gesture(pane: usize, row: i32, what: i32, ctrl: bool, shift: bool) {
    let Ok(row) = usize::try_from(row) else { return };
    match what {
        0 => with_state(|st| {
            let model = &st.panes[pane];
            let selected = model.entry(row).is_some_and(|e| e.selected);
            if let Some(op) = st.gesture[pane].press(row, selected, ctrl, shift) {
                model.select(op, row);
                log_select(model, pane, op, row);
            }
        }),
        1 => with_state(|st| {
            if let Some((r, op)) = st.gesture[pane].release() {
                st.panes[pane].select(op, r);
                log_select(&st.panes[pane], pane, op, r);
            }
        }),
        2 => drag_start(pane, row),
        3 => {
            if with_state(|st| st.gesture[pane].double_click()) {
                open_row(pane, row as i32);
            } else {
                eprintln!("[dnd] pane={pane} double-click at the end of a drag: not opened");
            }
        }
        _ => {}
    }
}

/// The pointer left the drag threshold with the left button down on `row`: drag the selection
/// (the press made sure it includes the row) with OLE. `DoDragDrop` runs a modal loop, so the
/// state borrow is dropped first and taken again after it returns.
fn drag_start(pane: usize, row: usize) {
    // SAFETY: plain Win32 query of this thread's key state.
    let held = dnd::scripted() || unsafe { GetKeyState(VK_LBUTTON as i32) } < 0;
    let taken = with_state(|st| {
        let app = st.app.upgrade()?;
        if !held {
            // Slint thinks the button is down but it is not (a release it never saw): no drag,
            // and the gesture ends like one, so the release given below selects nothing.
            eprintln!("[dnd] pane={pane} drag threshold crossed with the left button up: no drag");
            st.gesture[pane] = dnd::Gesture::Dragged;
            return Some(None);
        }
        if !st.gesture[pane].drag(row) {
            return None;
        }
        let folder = st.paths[pane].clone();
        let targets = st.panes[pane].targets(current_row(&app, pane));
        if targets.is_empty() || targets.iter().any(|e| e.kind == Kind::Drive) {
            note(st, &app, pane, "drives cannot be dragged".into());
            st.gesture[pane].drag_ended();
            return None;
        }
        let hwnd = main_hwnd(st, &app);
        if hwnd == 0 {
            st.gesture[pane].drag_ended();
            return None;
        }
        let names: Vec<String> = targets.iter().map(|e| e.name.to_string()).collect();
        eprintln!("[dnd] pane={pane} row={row}: dragging {} item(s)", names.len());
        Some(Some(dnd::DragRequest { hwnd, folder, names }))
    });
    let req = match taken {
        None => return,
        Some(None) => return slint::Timer::single_shot(Duration::ZERO, release_after_drag),
        Some(Some(req)) => req,
    };
    // MODAL LOOP: no borrow held (see dnd). A drop on our own other pane comes back through our
    // drop target from inside this call, and its job is posted to the event loop.
    let res = dnd::drag(&req);
    let dropped = res == Ok(true);
    with_state(|st| {
        st.gesture[pane].drag_ended();
        if let (Err(e), Some(app)) = (&res, st.app.upgrade()) {
            note(st, &app, pane, format!("drag: {e}"));
        }
    });
    if dropped {
        // A target that moved the files (Explorer does it itself, sometimes after this returns)
        // leaves this pane stale; a drop on our own pane refreshes both when its job ends.
        refresh(pane);
    }
    // Slint never saw the button come up (DoDragDrop's loop consumed it) and still holds the
    // row's grab. Give it a release, after this pointer event has finished dispatching.
    slint::Timer::single_shot(Duration::ZERO, release_after_drag);
}

/// The release Slint missed during `DoDragDrop`, at the cursor's position. The gesture is
/// `Dragged`, so it neither selects nor (as a double-click) opens.
fn release_after_drag() {
    let Some((app, hwnd)) = with_state(|st| Some((st.app.upgrade()?, st.hwnd))) else { return };
    let mut p = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
    // SAFETY: plain Win32 queries with a valid in/out pointer.
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut p);
        windows_sys::Win32::Graphics::Gdi::ScreenToClient(hwnd as windows_sys::Win32::Foundation::HWND, &mut p);
    }
    let scale = app.window().scale_factor();
    let position = slint::LogicalPosition::new(p.x as f32 / scale, p.y as f32 / scale);
    eprintln!("[dnd] release after the drag at ({:.0}, {:.0})", position.x, position.y);
    app.window()
        .dispatch_event_with_result(slint::platform::WindowEvent::PointerReleased {
            position,
            button: slint::platform::PointerEventButton::Left,
        })
        .ok();
}

/// Register the drop target once the window exists (retried by `dnd_timer` until then), and
/// set the drag threshold from `SM_CXDRAG`/`SM_CYDRAG`.
fn register_dnd() {
    let Some((app, hwnd)) = with_state(|st| {
        let app = st.app.upgrade()?;
        let hwnd = main_hwnd(st, &app);
        (hwnd != 0).then_some((app, hwnd))
    }) else {
        return;
    };
    with_state(|st| st.dnd_timer.stop());
    let (revoked, registered) = dnd::register(hwnd);
    eprintln!(
        "[dnd] registered: RevokeDragDrop (winit's target) = 0x{:08X}, RegisterDragDrop (ours) = 0x{:08X}",
        revoked as u32, registered as u32
    );
    let scale = app.window().scale_factor();
    // SAFETY: plain Win32 queries.
    let (cx, cy) = unsafe { (GetSystemMetrics(SM_CXDRAG), GetSystemMetrics(SM_CYDRAG)) };
    app.set_drag_dx(cx as f32 / scale);
    app.set_drag_dy(cy as f32 / scale);
    eprintln!("[dnd] drag threshold {cx} x {cy} px (scale {scale})");
}

/// For the drop target: where a drop at client point (`px`, `py`, physical px) goes. A short
/// borrow of the state, never held past this function; None off the panes, over the drive
/// list's empty space, or if the state is busy (not expected: no borrow is held across a drag
/// loop or a dispatched message).
pub fn drop_spot(px: i32, py: i32) -> Option<dnd::DropSpot> {
    STATE.with(|s| {
        let Ok(guard) = s.try_borrow() else {
            eprintln!("[dnd] app state busy during a drop target call: refused");
            return None;
        };
        let st = guard.as_ref()?;
        let app = st.app.upgrade()?;
        let scale = app.window().scale_factor();
        let size = app.window().size().to_logical(scale);
        let layout = dnd::Layout { width: size.width, height: size.height, split: app.get_split(), ratio: app.get_split_ratio() };
        let content_y = [app.invoke_get_scroll(0), app.invoke_get_scroll(1)];
        let len = [st.panes[0].len(), st.panes[1].len()];
        let (pane, row) = match dnd::hit(&layout, px as f32 / scale, py as f32 / scale, content_y, len) {
            dnd::Spot::Outside => return None,
            dnd::Spot::Pane(p) => (p, None),
            dnd::Spot::Row(p, r) => (p, Some(r)),
        };
        let folder = &st.paths[pane];
        let entry = row.and_then(|r| st.panes[pane].entry(r));
        let this_pc = is_this_pc(&folder.to_string_lossy());
        let (dest, on_row) = dnd::drop_folder(folder, this_pc, entry.as_ref().map(|e| (e.name.as_str(), e.is_dir())))?;
        let row = if on_row { row.map_or(-1, |r| r as i32) } else { -1 };
        Some(dnd::DropSpot { pane, row, dest })
    })
}

/// For the drop target: highlight the pane (row -1) or row a drop would land on, or nothing.
pub fn drop_highlight(at: Option<(usize, i32)>) {
    let app = STATE.with(|s| s.try_borrow().ok().and_then(|g| g.as_ref().and_then(|st| st.app.upgrade())));
    let Some(app) = app else { return };
    let (pane, row) = at.map_or((-1, -1), |(p, r)| (p as i32, r));
    if app.get_drop_pane() != pane {
        app.set_drop_pane(pane);
    }
    if app.get_drop_row() != row {
        app.set_drop_row(row);
    }
}

/// A drop landed (posted by the drop target through the event loop): copy or move the files
/// into `dest` with the file operation worker, noted on the pane they were dropped on. Both
/// panes refresh when the job ends.
pub fn drop_files(pane: usize, sources: Vec<PathBuf>, dest: PathBuf, effect: u32) {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        eprintln!("[dnd] drop into pane {pane}: {} {} item(s) to {}", dnd::effect_name(effect), sources.len(), dest.display());
        let job = if effect == dnd::MOVE { Job::Move { sources, dest_dir: dest } } else { Job::Copy { sources, dest_dir: dest } };
        submit(st, &app, pane, job);
    });
}

/// For the scripted driver's in-menu steps: no borrow of the app state is held right now.
pub fn state_is_free() -> bool {
    STATE.with(|s| s.try_borrow_mut().is_ok())
}

/// For the scripted driver: a job is in flight or a pane is still re-enumerating.
pub fn ops_busy() -> bool {
    STATE.with(|s| match s.try_borrow() {
        Ok(g) => g.as_ref().is_some_and(|st| !st.jobs.is_empty() || st.panes.iter().any(|m| m.is_refreshing())),
        Err(_) => true,
    })
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
    let ops = fileops::Worker::spawn(|d| {
        let _ = slint::invoke_from_event_loop(move || job_done(d));
    });
    let soak = (std::env::var("TWINFM_SOAK").as_deref() == Ok("1"))
        .then(|| vec![std::env::temp_dir().join("twinfm-m3"), std::env::temp_dir().join("twinfm-m5")]);
    if let Some(roots) = &soak {
        let roots: Vec<String> = roots.iter().map(|r| r.display().to_string()).collect();
        eprintln!("[fileops] TWINFM_SOAK=1: no confirmations, paths confined to {}", roots.join(" or "));
    }
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
            ops,
            hwnd: 0,
            next_job: 0,
            jobs: HashMap::new(),
            paste_move: None,
            notes: Default::default(),
            focus_name: Default::default(),
            soak,
            strip: [None; 2],
            strip_dirty: [false; 2],
            gesture: Default::default(),
            dnd_timer: slint::Timer::default(),
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
    app.on_open_row(|pane, row| open_row(pane as usize, row));
    app.on_row_gesture(|pane, row, what, ctrl, shift| row_gesture(pane as usize, row, what, ctrl, shift));
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
    app.on_context_menu(|pane, row, x, y, extended| context_menu(pane as usize, row, x, y, extended));
    app.on_context_key(|pane, row, lx, ly, cy, vh| context_key(pane as usize, row, lx, ly, cy, vh));
    app.on_command(|pane, cmd| command(pane as usize, cmd.as_str()));
    app.on_edit_commit(|pane, mode, original, typed| edit_commit(pane as usize, mode, original.as_str(), typed.as_str()));
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
    // The drop target goes on the window once it exists (winit creates it lazily, in `run`).
    with_state(|st| st.dnd_timer.start(slint::TimerMode::Repeated, Duration::from_millis(50), register_dnd));

    app.run()
}
