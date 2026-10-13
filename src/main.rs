// twinfm: Slint bootstrap, wires model <-> UI, owns the two panes and their tabs.
slint::include_modules!();

mod fs {
    pub mod enumerate;
    pub mod sizes;
    pub mod watch;
}
mod auto;
mod devices;
mod dnd;
mod fileops;
mod icons;
mod model;
mod pathbar;
mod perf;
mod settings;
mod shellmenu;
mod sidebar;
mod statusbar;
mod tabs;

use crossbeam_channel::{Receiver, Sender, unbounded};
use fileops::{Job, Outcome, clipboard, same_dir};
use fs::enumerate::{Msg, THIS_PC, is_this_pc};
use icons::IconTable;
use model::{Kind, NameMatch, PaneModel, SelOp, SortCol, filter_sort};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::{ComponentHandle, Image, Model, ModelRc, RenderingState, SharedString, VecModel, Weak};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tabs::{Place, Tabs};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_LBUTTON, VK_RBUTTON};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXDRAG, SM_CYDRAG, SW_MAXIMIZE, ShowWindow};

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
    /// (requests, replies, worker renders, worker ns, composes, overlays, slow renders) at the
    /// last report.
    reported: (u64, u64, u64, u64, u64, u64, u64),
}

/// Per-tab timing bookkeeping for the gate measurements.
#[derive(Default)]
struct PaneTiming {
    requested: Option<Instant>,
    first_batch: Option<Instant>,
    first_frame_reported: bool,
    /// Request-to-done wall time once the listing is complete.
    done_ms: Option<f64>,
    error: Option<String>,
}

/// One tab: its own listing (model, with its sort column, direction and hidden filter), folder,
/// note, selection, cursor and scroll. A pane shows its active tab; the others keep their state.
struct Tab {
    /// Unique for the life of the process; workers' replies name their tab by it (passes) or by
    /// the model's generation, which is unique across tabs too (`State::next_gen`).
    id: u64,
    model: Rc<PaneModel>,
    /// The folder the tab last navigated to (the path bar may hold unsubmitted text).
    path: PathBuf,
    timing: PaneTiming,
    /// Restore the list's scroll offset at the next pass swap (set when a refresh completes).
    keep_scroll: bool,
    /// The last file operation's message, shown before the item count.
    note: String,
    /// A name for the next pass to put the cursor and selection on (after a rename or a new
    /// folder).
    focus_name: Option<SharedString>,
    /// Where the next pass puts the cursor if its row is gone (after a delete or a move out of
    /// the folder): consumed by that pass, never set by a watch-driven refresh.
    fallback_cursor: Option<usize>,
    /// Free and total bytes of the folder's volume, read by the enumeration worker.
    free: Option<(u64, u64)>,
    /// Saved when the tab goes to the background, restored when it comes back.
    cursor: i32,
    scroll: f32,
    /// A file operation or shell command finished while the tab was in the background, or the
    /// tab left the foreground (its directory watch went with it): it refreshes when it comes to
    /// the front.
    stale: bool,
    /// The directory watch reported a change while the listing was still in flight: refresh again
    /// when it is done.
    refresh_pending: bool,
    /// The refresh in flight was started by the directory watch (counted for `waitwatch`).
    watch_refresh: bool,
    /// Back/forward (Alt+Left, Alt+Right, the mouse's side buttons); empty for a new or restored
    /// tab, never saved.
    history: tabs::History,
    /// A search tab (Ctrl+Shift+F): its listing is a recursive name search under `path`, its
    /// root. None for a folder tab.
    search: Option<Search>,
    /// Folder sizes computed for this tab (selected folders, Alt+Return), and those in flight.
    sizes: fs::sizes::TabSizes,
}

/// What a search tab searches for, and its worker's cancel flag (`fs::enumerate::SEARCH_*`).
struct Search {
    pattern: NameMatch,
    cancel: Arc<AtomicU8>,
}

impl Tab {
    /// Where the tab is, as its history records it: its folder, or its search and root.
    fn place(&self) -> Place {
        Place { path: self.path.clone(), search: self.search.as_ref().map(|s| s.pattern.pattern.clone()) }
    }

    /// Folder sizes are computed in folder tabs only (not for search results, not on This PC).
    fn sizable(&self) -> bool {
        self.search.is_none() && !is_this_pc(&self.path.to_string_lossy())
    }

    /// Forget the tab's folder sizes: cancel the walks in flight and empty the cache.
    fn drop_sizes(&mut self, pane: usize, why: &str) {
        let (cached, cancelled) = self.sizes.clear();
        for n in &cancelled {
            eprintln!("[size] cancelled {} ({why})", self.path.join(n).display());
        }
        if cached > 0 || !cancelled.is_empty() {
            eprintln!("[size] cache cleared pane={pane} ({why})");
        }
    }
}

impl Search {
    /// Stop the walk (within one folder or one batch), saying why; a no-op once stopped.
    fn cancel(&self, why: u8) {
        let _ = self.cancel.compare_exchange(fs::enumerate::SEARCH_RUNNING, why, Ordering::Relaxed, Ordering::Relaxed);
    }
}

/// A job in flight, as the UI side remembers it.
struct JobInfo {
    /// The pane and tab that submitted it (its note goes there).
    pane: usize,
    tab: u64,
    /// For a job that removes rows from a folder (delete, move out, cut-paste): the pane and tab
    /// those rows were in and where its cursor falls back to if its row goes
    /// (`PaneModel::cursor_after_removal`).
    fallback: Option<(usize, u64, usize)>,
    /// An undo job: the status text of the job it undoes. Its own completion is not recorded.
    undo_of: Option<String>,
}

struct State {
    app: Weak<App>,
    panes: [Tabs<Tab>; 2],
    /// Last tab id and last enumeration generation handed out.
    next_tab: u64,
    next_gen: u64,
    rx: Receiver<Msg>,
    tx: Sender<Msg>,
    split_requested: Option<Instant>,
    busy_timer: slint::Timer,
    icons: Rc<RefCell<IconTable>>,
    icon_tx: Sender<icons::Request>,
    icon_rx: Receiver<icons::Reply>,
    /// Each pane's active-tab generation, read by the icon worker to skip stale requests.
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
    /// Jobs in flight.
    jobs: HashMap<u64, JobInfo>,
    /// The pane and tab whose rows carry the cut marks (None: no cut on the clipboard).
    cut_source: Option<(usize, u64)>,
    /// The clipboard's sequence number right after TwinFM's last copy or cut.
    clip_seq: u32,
    /// Ctrl+Z's record, newest last, shared by both panes (`fileops::undo`).
    undo: Vec<fileops::undo::Record>,
    /// The paste-of-a-cut job, if one is in flight: on success the clipboard is emptied.
    paste_move: Option<u64>,
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
    /// Each pane's tab strip and breadcrumb, rebuilt on change only.
    tab_models: [Rc<VecModel<TabInfo>>; 2],
    crumb_models: [Rc<VecModel<Crumb>>; 2],
    /// TWINFM_PERF_STATUS=1: the status bar also shows the enumeration time.
    perf_status: bool,
    /// The settings as loaded and as changed since, and where they are saved (None: a scripted
    /// run, nothing is saved). `save_timer` is the one debounce timer: restarted on every change,
    /// it writes the file once, 500 ms after the last one.
    config: settings::Config,
    config_path: Option<PathBuf>,
    save_timer: slint::Timer,
    /// The sidebar's sections and the rows composed from them (the Slint model mirrors `side`).
    drives: Vec<(String, PathBuf)>,
    places: Vec<(String, PathBuf)>,
    side: Vec<sidebar::Row>,
    side_model: Rc<VecModel<SideItem>>,
    /// The directory watcher (one thread, at most one watch per pane: its active tab's folder)
    /// and its events, drained with the enumeration messages.
    watch: fs::watch::Watcher,
    watch_rx: Receiver<fs::watch::Event>,
    /// Watch-driven refreshes whose listing completed (the driver's `waitwatch` waits on it).
    watch_refreshes: u64,
    /// Debounces volume arrivals and removals (WM_DEVICECHANGE comes in bursts).
    device_timer: slint::Timer,
    /// The breadcrumb dropdown: requests handed out (a listing that comes back for an older one
    /// is dropped), and the one on screen (its pane, folder and the folder names it lists).
    crumb_seq: u64,
    crumb_menu: Option<(usize, PathBuf, Vec<String>)>,
    /// The settings dialog's values when it opened (None: closed); OK applies what differs.
    dialog: Option<settings::Dialog>,
    /// Each pane's filter debounce: restarted on every edit in the box, it applies the text 150 ms
    /// after the last one.
    filter_timer: [slint::Timer; 2],
    /// Ctrl+Shift+F with an empty filter box opened the box: Return there starts the search.
    search_armed: [bool; 2],
    /// The search tab whose "no watch" line each pane logged last (logged once per tab).
    no_watch_logged: [Option<u64>; 2],
    /// The folder size worker and its replies.
    sizer: fs::sizes::Sizer,
    size_rx: Receiver<fs::sizes::Reply>,
    /// Each pane's re-walk timer: fires once when the earliest stale size of its active tab is due.
    size_timer: [slint::Timer; 2],
}

/// A tab with no folder yet, its model sharing the icon table.
fn blank_tab(id: u64, icons: &Rc<RefCell<IconTable>>) -> Tab {
    Tab {
        id,
        model: Rc::new(PaneModel::with_icons(icons.clone())),
        path: PathBuf::new(),
        timing: PaneTiming::default(),
        keep_scroll: false,
        note: String::new(),
        focus_name: None,
        fallback_cursor: None,
        free: None,
        cursor: -1,
        scroll: 0.0,
        stale: false,
        refresh_pending: false,
        watch_refresh: false,
        history: tabs::History::default(),
        search: None,
        sizes: fs::sizes::TabSizes::default(),
    }
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

impl State {
    /// A new enumeration generation, unique across every tab of both panes.
    fn generation(&mut self) -> u64 {
        self.next_gen += 1;
        self.next_gen
    }

    /// A fresh tab on `path` (not yet enumerated) whose model shares the icon table.
    fn new_tab(&mut self, path: PathBuf) -> Tab {
        self.next_tab += 1;
        Tab { path, ..blank_tab(self.next_tab, &self.icons) }
    }

    fn tab(&self, pane: usize) -> &Tab {
        self.panes[pane].active()
    }

    fn tab_mut(&mut self, pane: usize) -> &mut Tab {
        self.panes[pane].active_mut()
    }

    /// The tab with this id in the pane, active or not (None once it is closed).
    fn find(&mut self, pane: usize, id: u64) -> Option<&mut Tab> {
        self.panes[pane].iter_mut().find(|t| t.id == id)
    }

    /// The tab whose model is enumerating this generation.
    fn by_generation(&mut self, pane: usize, generation: u64) -> Option<&mut Tab> {
        self.panes[pane].iter_mut().find(|t| t.model.generation.get() == generation)
    }

    fn is_active(&self, pane: usize, id: u64) -> bool {
        self.panes[pane].active().id == id
    }

    fn model(&self, pane: usize) -> &PaneModel {
        &self.panes[pane].active().model
    }

    /// Point the pane's directory watch at its active tab's folder (a no-op when it already
    /// watches it). The hidden second pane watches nothing.
    fn watch_active(&mut self, app: &App, pane: usize) {
        if pane == 1 && !app.get_split() {
            return self.watch.unwatch(1);
        }
        let t = self.panes[pane].active();
        if t.search.is_some() {
            // A search's results come from many folders; F5 runs it again.
            self.watch.unwatch(pane);
            if self.no_watch_logged[pane] != Some(t.id) {
                eprintln!("[watch] pane={pane} no watch for {}: search results", t.path.display());
                self.no_watch_logged[pane] = Some(t.id);
            }
            return;
        }
        // With folder sizes cached or in flight, every folder below is watched too (until two
        // overflows in a row), so a change inside a sized folder drops its size.
        let subtree = t.sizes.any() && t.sizes.overflows < 2;
        self.watch.watch(pane, t.id, t.path.clone(), subtree);
    }

    /// The pane's active tab is a search tab.
    fn is_search(&self, pane: usize) -> bool {
        self.tab(pane).search.is_some()
    }
}

/// An ordinary navigation (a click, Return, a crumb, the sidebar, the path entry, up a folder):
/// the folder being left goes on the active tab's back stack.
fn navigate(pane: usize, path: PathBuf) {
    navigate_to(pane, path, true);
}

/// The one place folders change. `push`: record the folder being left in the tab's history (an
/// ordinary navigation); false for a move through the history itself (`history_move`).
fn navigate_to(pane: usize, path: PathBuf, push: bool) {
    let path = if is_this_pc(&path.to_string_lossy()) { PathBuf::from(THIS_PC) } else { path };
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let generation = st.generation();
        let t = st.tab_mut(pane);
        // A folder that then fails to list still counts as visited, as in Explorer.
        // From a search tab the search goes on the back stack (Back runs it again).
        let from = t.place();
        if push && t.history.push(&from, &Place::folder(path.clone())) {
            eprintln!("[nav] pane={pane} history push {from} ({} back)", t.history.back_len());
        }
        t.drop_sizes(pane, "navigation");
        t.sizes.overflows = 0;
        t.model.generation.set(generation);
        t.model.clear();
        t.timing = PaneTiming { requested: Some(Instant::now()), ..Default::default() };
        t.path = path.clone();
        t.keep_scroll = false;
        t.note.clear();
        t.focus_name = None;
        t.fallback_cursor = None;
        t.free = None;
        t.stale = false;
        t.refresh_pending = false;
        t.watch_refresh = false;
        // A search tab becomes a folder tab again (its root goes on the back stack, the search
        // does not).
        if let Some(s) = t.search.take() {
            s.cancel(fs::enumerate::SEARCH_NAVIGATED);
        }
        // Another folder starts unfiltered (Explorer's behaviour).
        let filtered = !t.model.filter.borrow().is_empty();
        *t.model.filter.borrow_mut() = NameMatch::default();
        let id = t.id;
        if filtered {
            eprintln!("[filter] pane={pane} cleared");
        }
        st.filter_timer[pane].stop();
        st.search_armed[pane] = false;
        set_filter_box(&app, pane, "");
        // `model.clear` dropped the tab's cut marks.
        if st.cut_source == Some((pane, id)) {
            st.cut_source = None;
        }
        st.icon_gens[pane].store(generation, Ordering::Relaxed);
        // Watched from the start, so a change during the listing is not missed; dropped again if
        // the listing fails (Msg::Done).
        st.watch_active(&app, pane);
        hide_strip(st, &app, pane);
        let shown = path.to_string_lossy().to_string();
        set_path(st, &app, pane);
        update_tabs(st, pane);
        update_status(st, &app, pane);
        app.invoke_reset_scroll(pane as i32);
        eprintln!("[nav] pane={pane} gen={generation} path={shown}");
        let tx = st.tx.clone();
        fs::enumerate::spawn(path, pane, generation, tx, wake);
        schedule_save(st);
    });
}

/// Re-enumerate the active tab's folder in place (F5, Ctrl+R). The old rows stay on screen until
/// the new listing has been filtered and sorted; cursor, selection and scroll come back by name.
fn refresh(pane: usize) {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let generation = st.generation();
        let t = st.tab_mut(pane);
        t.model.generation.set(generation);
        t.model.begin_refresh();
        t.timing = PaneTiming { requested: Some(Instant::now()), ..Default::default() };
        t.stale = false;
        let path = t.path.clone();
        st.icon_gens[pane].store(generation, Ordering::Relaxed);
        update_status(st, &app, pane);
        eprintln!("[refresh] pane={pane} gen={generation} path={}", path.display());
        // A search tab runs its search again (F5, after a file operation, when it comes back to
        // the front stale).
        let t = st.tab_mut(pane);
        if let Some(s) = &mut t.search {
            s.cancel(fs::enumerate::SEARCH_REFRESH);
            s.cancel = Arc::new(AtomicU8::new(fs::enumerate::SEARCH_RUNNING));
            let (pattern, cancel, hidden) = (s.pattern.clone(), s.cancel.clone(), t.model.show_hidden.get());
            eprintln!("[search] pane={pane} gen={generation} start \"{}\" in {}", pattern.pattern, path.display());
            fs::enumerate::spawn_search(path, pattern, hidden, cancel, pane, generation, st.tx.clone(), wake);
            return;
        }
        fs::enumerate::spawn(path, pane, generation, st.tx.clone(), wake);
    });
}

/// The pane's status bar from its active tab: items, hidden, note and error on the left, the
/// selection in the middle, free space on the right. Called on navigation, enumeration
/// progress, pass swaps, selection changes and notes; never from a scroll or render path.
fn update_status(st: &State, app: &App, pane: usize) {
    let t = st.tab(pane);
    let m = &t.model;
    let loading = t.timing.done_ms.is_none() && t.timing.error.is_none();
    let enum_ms = if st.perf_status { t.timing.done_ms } else { None };
    // Jobs waiting behind the running one, shown on both panes.
    let note = statusbar::with_queue(&t.note, st.jobs.len().saturating_sub(1));
    let left = match &t.search {
        Some(s) => {
            let secs = (!loading).then(|| t.timing.done_ms.unwrap_or(0.0) / 1e3);
            let stopped = s.cancel.load(Ordering::Relaxed) != fs::enumerate::SEARCH_RUNNING;
            statusbar::search_text(&note, m.len(), m.hidden_count(), secs, stopped, t.timing.error.as_deref())
        }
        None => {
            // "N of M items" while a name filter is on.
            let of = (!m.filter.borrow().is_empty()).then(|| m.len() + m.filtered_count());
            statusbar::left_text(&note, m.len(), of, m.hidden_count(), loading, enum_ms, t.timing.error.as_deref())
        }
    };
    let (n, bytes, any_file) = m.selection_summary();
    let dirs = if t.sizable() { t.sizes.summary(&m.selected_folders()) } else { fs::sizes::DirTotals::default() };
    let s = StatusInfo {
        left: left.into(),
        mid: statusbar::mid_text(n, bytes, any_file, dirs).into(),
        right: statusbar::free_text(t.free).into(),
    };
    if pane == 0 { app.set_left_status(s) } else { app.set_right_status(s) }
}

/// The pane's tab strip: one entry per tab, titled by its folder. Hidden by the UI with one tab.
fn update_tabs(st: &State, pane: usize) {
    let tabs = &st.panes[pane];
    let active = tabs.active_index();
    let v: Vec<TabInfo> = tabs
        .iter()
        .enumerate()
        .map(|(i, t)| TabInfo { title: tab_title(t).into(), active: i == active })
        .collect();
    // Same count: change the rows that differ, so the strip keeps its tab items (a reset would
    // recreate them and drop the pointer grab of a tab being pressed or dragged).
    let m = &st.tab_models[pane];
    if m.row_count() == v.len() {
        for (i, info) in v.into_iter().enumerate() {
            if m.row_data(i).as_ref() != Some(&info) {
                m.set_row_data(i, info);
            }
        }
    } else {
        m.set_vec(v);
    }
    // The active tab scrolled into view in an overflowing strip (on change only, like the rest).
    if let (Some(app), true) = (st.app.upgrade(), tabs.len() > 1) {
        let x = app.invoke_reveal_tab(pane as i32, active as i32);
        if x <= -0.5 {
            eprintln!("[tab] pane={pane} strip scrolled to {x:.0} (tab {} of {} in view)", active + 1, tabs.len());
        }
    }
}

/// A tab's title: its folder's name, or `Search: <pattern>` for a search tab.
fn tab_title(t: &Tab) -> String {
    match &t.search {
        Some(s) => format!("Search: {}", s.pattern.pattern),
        None => pathbar::title(&t.path.to_string_lossy()),
    }
}

/// The path bar (text and breadcrumb) from the active tab's folder. A search tab's breadcrumb is
/// its root's, navigable as ever, then one more crumb naming the search (a click on it does
/// nothing: it is past the root's crumbs).
fn set_path(st: &State, app: &App, pane: usize) {
    let t = st.tab(pane);
    let path = t.path.to_string_lossy().to_string();
    let mut crumbs: Vec<Crumb> = pathbar::crumbs(&path).into_iter().map(|c| Crumb { label: c.label.into() }).collect();
    if let Some(s) = &t.search {
        crumbs.push(Crumb { label: format!("Search: \"{}\"", s.pattern.pattern).into() });
    }
    st.crumb_models[pane].set_vec(crumbs);
    // The window title reads the active pane's one ("TwinFM - <folder>", app.slint).
    let title: SharedString = tab_title(t).into();
    if pane == 0 {
        app.set_left_path(path.into());
        app.set_left_title(title);
    } else {
        app.set_right_path(path.into());
        app.set_right_title(title);
    }
}

/// A setting changed: write the file 500 ms after the last change (restarting the one timer),
/// so a splitter or sidebar drag writes once when it stops.
fn schedule_save(st: &State) {
    if st.config_path.is_some() {
        st.save_timer.start(slint::TimerMode::SingleShot, Duration::from_millis(500), save_now);
    }
}

/// Gather the settings from the window and the panes and write them (atomically).
fn save_now() {
    let Some((config, path)) = with_state(|st| {
        st.save_timer.stop();
        let app = st.app.upgrade()?;
        // Each pane's tabs; a pane whose tabs never had a folder (the second pane before the
        // split was first opened) keeps the saved list.
        let tab_paths = |tabs: &Tabs<Tab>| {
            let v: Vec<String> = tabs.iter().map(|t| t.path.to_string_lossy().to_string()).collect();
            v.iter().all(|p| !p.is_empty()).then_some((v, tabs.active_index()))
        };
        let (left_tabs, right_tabs) = (tab_paths(&st.panes[0]), tab_paths(&st.panes[1]));
        let c = &mut st.config;
        if let Some((v, a)) = left_tabs {
            (c.left_tabs, c.left_active) = (v, a);
        }
        if let Some((v, a)) = right_tabs {
            (c.right_tabs, c.right_active) = (v, a);
        }
        // The start folders are the settings dialog's to change (the tab lists hold the folders
        // last open).
        c.split = app.get_split();
        c.split_ratio = app.get_split_ratio();
        c.sidebar_visible = app.get_sidebar_visible();
        c.sidebar_width = app.get_sidebar_width();
        c.path_bar = if app.get_path_entry_sticky() { "entry" } else { "breadcrumb" }.into();
        let w = app.window();
        // Maximised: the Win32 placement's normal rectangle, so un-maximising after a restart
        // gives the right size. Otherwise the window as it is; minimised: left as last saved.
        match settings::placement(st.hwnd, w.scale_factor()) {
            Some((true, normal)) => st.config.window = Some(normal),
            Some((false, _)) if !w.is_minimized() && w.is_visible() => {
                let size = w.size().to_logical(w.scale_factor());
                let pos = w.position();
                if size.width > 0.0 && size.height > 0.0 {
                    st.config.window =
                        Some(settings::WindowGeom { x: pos.x, y: pos.y, width: size.width, height: size.height, maximized: false });
                }
            }
            _ => {}
        }
        Some((st.config.clone(), st.config_path.clone()?))
    }) else {
        return;
    };
    match config.save(&path) {
        Ok(()) => eprintln!("[settings] saved {}", path.display()),
        Err(e) => eprintln!("[settings] could not save: {e}"),
    }
}

/// Rebuild the sidebar's rows from its sections (on a change only).
fn rebuild_sidebar(st: &mut State) {
    st.side = sidebar::compose(&st.drives, &st.places, &st.config.bookmarks);
    let items: Vec<SideItem> =
        st.side.iter().map(|r| SideItem { label: r.label.as_str().into(), kind: r.kind as i32, index: r.index as i32 }).collect();
    st.side_model.set_vec(items);
    if let Some(app) = st.app.upgrade() {
        app.set_bookmark_count(st.config.bookmarks.len() as i32);
    }
}

/// Re-read the drive list for the sidebar on a worker (labels and free space can stall on a slow
/// drive); the rows are rebuilt when it arrives. When the sidebar is shown and on F5.
fn refresh_drives() {
    std::thread::spawn(|| {
        let drives: Vec<(String, PathBuf)> =
            fs::enumerate::drive_entries().iter().map(|e| (sidebar::drive_label(e), PathBuf::from(e.name.as_str()))).collect();
        let _ = slint::invoke_from_event_loop(move || {
            with_state(|st| {
                eprintln!("[sidebar] {} drive(s)", drives.len());
                st.drives = drives;
                rebuild_sidebar(st);
            })
        });
    });
}

/// The pane keyboard actions go to: the right one only while the split is on and it is active.
fn active_pane(app: &App) -> usize {
    if app.get_split() && app.get_active_pane() == 1 { 1 } else { 0 }
}

/// A sidebar row: open its folder in the active pane, or with the middle button in a new tab there.
fn side_clicked(i: usize, middle: bool) {
    let Some((app, path)) = with_state(|st| Some((st.app.upgrade()?, st.side.get(i)?.path.clone()?))) else { return };
    let pane = active_pane(&app);
    eprintln!("[sidebar] row {i} {}: {}", if middle { "middle-click, new tab" } else { "click" }, path.display());
    if middle { open_tab(pane, Some(path)) } else { navigate(pane, path) }
}

/// Ctrl+D: bookmark the active tab's folder.
fn bookmark_add() {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let pane = active_pane(&app);
        let folder = st.tab(pane).path.to_string_lossy().to_string();
        let text = if is_this_pc(&folder) {
            "This PC cannot be bookmarked".to_string()
        } else if sidebar::add_bookmark(&mut st.config.bookmarks, &folder) {
            rebuild_sidebar(st);
            schedule_save(st);
            format!("Bookmarked {folder}")
        } else {
            format!("{folder} is bookmarked already")
        };
        eprintln!("[sidebar] {text}");
        note(st, &app, pane, text);
    });
}

/// The bookmark menu's "Remove bookmark".
fn bookmark_remove(i: usize) {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let Some(gone) = sidebar::remove_bookmark(&mut st.config.bookmarks, i) else { return };
        rebuild_sidebar(st);
        schedule_save(st);
        eprintln!("[sidebar] removed bookmark {i}: {}", gone.path);
        note(st, &app, active_pane(&app), format!("Removed the bookmark {}", gone.path));
    });
}

/// The bookmark row's inline editor (from "Rename bookmark"): the title the sidebar shows; an
/// empty one goes back to the folder's name.
fn bookmark_rename(i: usize, title: &str) {
    with_state(|st| {
        if !sidebar::rename_bookmark(&mut st.config.bookmarks, i, title) {
            return;
        }
        rebuild_sidebar(st);
        schedule_save(st);
        let b = &st.config.bookmarks[i];
        eprintln!("[sidebar] renamed bookmark {i} ({}): {:?}", b.path, b.title);
    });
}

/// The bookmark menu's "Move up" and "Move down".
fn bookmark_move(i: usize, up: bool) {
    with_state(|st| {
        let Some(to) = sidebar::move_bookmark(&mut st.config.bookmarks, i, up) else {
            return eprintln!("[sidebar] bookmark {i} cannot move {}", if up { "up" } else { "down" });
        };
        rebuild_sidebar(st);
        schedule_save(st);
        eprintln!("[sidebar] moved bookmark {i} to {to}: {}", st.config.bookmarks[to].path);
    });
}

fn drain() {
    // Panes whose active tab refreshes once the state borrow is released.
    let again = with_state(|st| {
        let mut again: Vec<usize> = Vec::new();
        let Some(app) = st.app.upgrade() else { return again };
        while let Ok(msg) = st.rx.try_recv() {
            match msg {
                Msg::Batch { pane, generation, entries } => {
                    let Some(t) = st.by_generation(pane, generation) else { continue };
                    t.model.append(entries);
                    if t.model.is_refreshing() {
                        continue; // staged; the old rows and status stay until the pass
                    }
                    let id = t.id;
                    if t.timing.first_batch.is_none() {
                        t.timing.first_batch = Some(Instant::now());
                        if let Some(r) = t.timing.requested {
                            eprintln!("[enum] pane={pane} first batch after {:.1} ms", perf::elapsed_ms(r));
                        }
                    }
                    if st.is_active(pane, id) {
                        update_status(st, &app, pane);
                    }
                }
                Msg::Done { pane, generation, total, elapsed, error, free, reparse } => {
                    let Some(t) = st.by_generation(pane, generation) else { continue };
                    let wall = t.timing.requested.map(perf::elapsed_ms).unwrap_or(0.0);
                    if t.model.is_refreshing() {
                        t.model.finish_refresh();
                        t.keep_scroll = true;
                        // A refresh that kept the folder sizes (the watch's, an operation's).
                        if !t.sizes.cache.is_empty() {
                            t.model.apply_sizes(|n| t.sizes.cache.get(n).map(|s| s.bytes));
                        }
                    }
                    t.free = free;
                    let id = t.id;
                    match error {
                        Some(e) => t.timing.error = Some(e),
                        None => {
                            let split = if reparse.total() == 0 {
                                String::new()
                            } else {
                                format!(
                                    ", reparse: {} junction, {} symlink, {} cloud, {} app alias, {} other",
                                    reparse.junction, reparse.symlink, reparse.cloud, reparse.alias, reparse.other
                                )
                            };
                            eprintln!(
                                "[enum] pane={pane} done: {total} entries, worker {:.1} ms, request-to-done {wall:.1} ms{split}",
                                elapsed.as_secs_f64() * 1e3
                            );
                            t.timing.done_ms = Some(wall);
                        }
                    }
                    let watched = std::mem::take(&mut t.watch_refresh);
                    let ok = t.timing.error.is_none();
                    let pending = std::mem::take(&mut t.refresh_pending) && ok;
                    if watched {
                        st.watch_refreshes += 1;
                    }
                    if st.is_active(pane, id) {
                        update_status(st, &app, pane);
                        // A folder that cannot be listed is not watched.
                        if ok { st.watch_active(&app, pane) } else { st.watch.unwatch(pane) }
                        if pending {
                            st.tab_mut(pane).watch_refresh = true;
                            again.push(pane);
                        }
                    }
                    if ok {
                        // Apply the tab's current filter and sort once the listing is complete.
                        request_pass(st, pane, id);
                    }
                }
            }
        }
        while let Ok(ev) = st.watch_rx.try_recv() {
            watch_event(st, ev, &mut again);
        }
        again
    });
    for pane in again {
        refresh(pane);
    }
}

/// A directory watch event (on the UI thread, inside `drain`). A change to the active tab
/// refreshes it (after the listing in flight, if there is one); a tab that is no longer active
/// goes stale. A lost watch refreshes the active tab once, so its error shows. A subtree watch's
/// change below the folder only drops the sizes of the subfolders it lay under (and sizes them
/// again if they are selected); only a change in the folder itself, or an overflow, refreshes.
fn watch_event(st: &mut State, ev: fs::watch::Event, again: &mut Vec<usize>) {
    let (pane, tab_id) = match &ev {
        fs::watch::Event::Changed { pane, tab_id }
        | fs::watch::Event::Lost { pane, tab_id, .. }
        | fs::watch::Event::Subtree { pane, tab_id, .. } => (*pane, *tab_id),
    };
    let active = st.is_active(pane, tab_id);
    let Some(t) = st.find(pane, tab_id) else { return };
    let (mut resize, mut stale) = (false, false);
    match ev {
        fs::watch::Event::Lost { error, .. } => {
            eprintln!("[watch] pane={pane} tab={tab_id} watch lost ({error}){}", if active { ": refresh" } else { "" });
            if active && !again.contains(&pane) {
                again.push(pane);
            }
            return;
        }
        fs::watch::Event::Changed { .. } => {}
        fs::watch::Event::Subtree { top, changes, overflow, .. } => {
            if overflow {
                t.sizes.overflows += 1;
                let names: Vec<String> = t.sizes.cache.keys().cloned().collect();
                for n in &names {
                    t.sizes.forget(n);
                    t.model.set_size(n, None);
                }
                eprintln!("[watch] pane={pane} tab={tab_id} subtree overflow {}: sizes dropped", t.path.display());
                if t.sizes.overflows >= 2 {
                    eprintln!("[watch] pane={pane} tab={tab_id} subtree disabled for this tab after two overflows");
                }
                resize = true;
            } else {
                t.sizes.overflows = 0;
                // A sized folder that changed keeps showing its size and is walked again after a
                // delay; changes meanwhile join that one re-walk (no line for them).
                let now = Instant::now();
                for (child, change) in &changes {
                    if let Some((name, delay)) = t.sizes.invalidate(child, now) {
                        let path = t.path.join(&name);
                        eprintln!("[size] invalidated {} ({change})", path.display());
                        eprintln!("[size] stale {}: re-walk in {:.1} s", path.display(), delay.as_secs_f64());
                        stale = true;
                    }
                }
            }
            if stale {
                arm_rewalk(st, pane);
            }
            if !(top || overflow || !active) {
                eprintln!("[watch] pane={pane} tab={tab_id} changed below the folder: no refresh");
                if resize {
                    resize_after_change(st, pane);
                }
                return;
            }
        }
    }
    let Some(t) = st.find(pane, tab_id) else { return };
    if !active {
        t.stale = true;
        eprintln!("[watch] pane={pane} tab={tab_id} changed in the background: stale");
    } else if t.timing.done_ms.is_none() && t.timing.error.is_none() {
        t.refresh_pending = true;
        eprintln!("[watch] pane={pane} tab={tab_id} changed while listing: refresh after it");
    } else {
        t.watch_refresh = true;
        eprintln!("[watch] pane={pane} tab={tab_id} changed: refresh");
        if !again.contains(&pane) {
            again.push(pane);
        }
    }
    if resize {
        resize_after_change(st, pane);
    }
}

/// Start (or move) the pane's re-walk timer to the earliest stale size of its active tab, and show
/// the folders as updating. One single-shot timer per pane, restarted on each new stale size.
fn arm_rewalk(st: &mut State, pane: usize) {
    let Some(app) = st.app.upgrade() else { return };
    if let Some(due) = st.tab(pane).sizes.next_due() {
        let wait = due.saturating_duration_since(Instant::now());
        st.size_timer[pane].start(slint::TimerMode::SingleShot, wait, move || rewalk_due(pane));
    }
    update_status(st, &app, pane);
}

/// The re-walk timer fired: walk the due stale folders again if they are still wanted (selected,
/// or sized by Alt+Return in this listing); forget the others. Their old size stays shown until
/// the new one lands.
fn rewalk_due(pane: usize) {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let State { panes, sizer, .. } = &mut *st;
        let t = panes[pane].active_mut();
        let selected: std::collections::HashSet<String> = t.model.selected_folders().into_iter().collect();
        for n in t.sizes.due(Instant::now()) {
            if t.sizes.pending.contains_key(&n) {
                continue; // being walked already (Alt+Return, a selection)
            }
            let all = t.sizes.from_all.contains(&n);
            if all || selected.contains(&n) {
                queue_size(sizer, pane, t, &n, all);
            } else if t.sizes.forget(&n) {
                eprintln!("[size] stale {}: not selected any more, size dropped", t.path.join(&n).display());
                t.model.set_size(&n, None);
            }
        }
        st.watch_active(&app, pane);
        arm_rewalk(st, pane);
    });
}

/// After sizes were dropped by the watch: size the selected folders again, point the watch at the
/// folder the new way (shallow once the cache is empty) and show the new totals.
fn resize_after_change(st: &mut State, pane: usize) {
    let Some(app) = st.app.upgrade() else { return };
    size_selection(st, pane);
    st.watch_active(&app, pane);
    update_status(st, &app, pane);
}

/// Queue one folder of the pane's active tab for the size worker.
fn queue_size(sizer: &fs::sizes::Sizer, pane: usize, t: &mut Tab, name: &str, all: bool) {
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let path = t.path.join(name);
    let shown = path.display().to_string();
    let generation = t.sizes.generation;
    let r = fs::sizes::Request { pane, tab: t.id, generation, name: name.to_string(), path, cancel: cancel.clone() };
    let n = sizer.push(r);
    eprintln!("[size] queued {shown} ({n} in queue)");
    t.sizes.pending.insert(name.to_string(), fs::sizes::Pending { cancel, partial: Default::default(), all });
}

/// The selection changed (or a pass restored it): size the selected folders that have no size
/// yet (at most `SELECTION_CAP`), and cancel the walks for folders no longer selected. Does
/// nothing, and logs nothing, when nothing changes.
fn size_selection(st: &mut State, pane: usize) {
    let State { panes, sizer, .. } = st;
    let t = panes[pane].active_mut();
    if !t.sizable() {
        return;
    }
    let before = t.sizes.any();
    let selected = t.model.selected_folders();
    let plan = t.sizes.plan(&selected, fs::sizes::SELECTION_CAP);
    for n in &plan.cancel {
        if let Some(p) = t.sizes.pending.remove(n) {
            p.cancel.store(true, Ordering::Relaxed);
            eprintln!("[size] cancelled {} (deselected)", t.path.join(n).display());
            // A cancelled re-walk leaves an out-of-date size: drop it.
            if t.sizes.forget(n) {
                t.model.set_size(n, None);
            }
        }
    }
    // Newest first: the first selected folder goes in last and is served first.
    for n in plan.queue.iter().rev() {
        queue_size(sizer, pane, t, n, false);
    }
    if let (Some((k, m)), false) = (plan.capped, plan.queue.is_empty()) {
        eprintln!("[size] capped: {k} of {m} folders sized");
    }
    // The first request makes the watch a subtree watch; the last one cancelled with nothing
    // cached makes it shallow again.
    if t.sizes.any() != before {
        if let Some(app) = st.app.upgrade() {
            st.watch_active(&app, pane);
        }
    }
}

/// Alt+Return in the list: size every folder of the listing (at most `ALL_CAP`), the ones nearest
/// the cursor first.
fn size_all(st: &mut State, app: &App, pane: usize) {
    let cursor = usize::try_from(current_row(app, pane)).unwrap_or(0);
    if !st.tab(pane).sizable() {
        let why = if st.is_search(pane) { "search results have no folder sizes" } else { "drives show their free space" };
        return note(st, app, pane, why.into());
    }
    let State { panes, sizer, .. } = st;
    let t = panes[pane].active_mut();
    let mut rows: Vec<(usize, String)> = t
        .model
        .folder_rows()
        .into_iter()
        .filter(|(_, n)| !t.sizes.cache.contains_key(n) && !t.sizes.pending.contains_key(n))
        .collect();
    let m = rows.len();
    rows.sort_by_key(|(i, _)| i.abs_diff(cursor));
    rows.truncate(fs::sizes::ALL_CAP);
    eprintln!("[size] pane={pane} Alt+Return: {} of {} folder(s) to size", rows.len(), m);
    if m > fs::sizes::ALL_CAP {
        eprintln!("[size] capped: {} of {m} folders sized", fs::sizes::ALL_CAP);
    }
    // The farthest go in first, so the nearest are the newest and served first.
    for (_, n) in rows.iter().rev() {
        queue_size(sizer, pane, t, n, true);
    }
    st.watch_active(app, pane);
    update_status(st, app, pane);
}

/// Take every size reply: a partial total updates the status bar, a final one goes into the tab's
/// cache and its row (that one row). Replies for an older generation, or for a request since
/// cancelled, are dropped.
fn drain_sizes() {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let mut touched = [false; 2];
        while let Ok(r) = st.size_rx.try_recv() {
            let Some(t) = st.find(r.pane, r.tab) else { continue };
            if t.sizes.generation != r.generation {
                continue;
            }
            let Some(p) = t.sizes.pending.get_mut(&r.name).filter(|p| Arc::ptr_eq(&p.cancel, &r.cancel)) else { continue };
            touched[r.pane] = true;
            if !r.done {
                p.partial = r.totals;
                continue;
            }
            if p.all {
                t.sizes.from_all.insert(r.name.clone());
            }
            t.sizes.pending.remove(&r.name);
            t.sizes.walk_ms.insert(r.name.clone(), r.ms);
            t.sizes.cache.insert(r.name.clone(), r.totals);
            t.model.set_size(&r.name, Some(r.totals.bytes));
        }
        for pane in 0..2 {
            if touched[pane] {
                update_status(st, &app, pane);
            }
        }
    });
}

/// Filter and sort on a worker over a snapshot of the tab's full listing; swap the shown Vec in
/// one step when it returns. Uses the tab's current sort column, direction and hidden filter.
/// The tab may be in the background by then: the result lands in its model all the same.
fn request_pass(st: &mut State, pane: usize, id: u64) {
    let Some(app) = st.app.upgrade() else { return };
    let active = st.is_active(pane, id);
    let Some(t) = st.find(pane, id) else { return };
    let model = t.model.clone();
    let (col, asc, show_hidden) = (model.sort_col.get(), model.sort_asc.get(), model.show_hidden.get());
    let filter = model.filter.borrow().clone();
    if active {
        if pane == 0 {
            app.set_left_sort_col(col as i32);
            app.set_left_sort_asc(asc);
        } else {
            app.set_right_sort_col(col as i32);
            app.set_right_sort_asc(asc);
        }
    }
    let generation = model.generation.get();
    let pass = model.pass.get() + 1;
    model.pass.set(pass);
    let snapshot = model.snapshot_all();
    let cursor = if active { current_row(&app, pane) } else { t.cursor };
    let mut restore = model.capture(cursor);
    let focus = t.focus_name.take();
    restore.fallback_index = t.fallback_cursor.take();
    // A cursor placed by an operation (a new or renamed row, the row after a removed one) is
    // scrolled into view when the pass lands.
    let reveal = focus.is_some() || restore.fallback_index.is_some();
    if let Some(name) = focus {
        restore.cursor = Some(name.clone());
        restore.selected = vec![name];
    }
    let sel_version = model.sel_version.get();
    let t0 = Instant::now();
    std::thread::spawn(move || {
        let total = snapshot.len();
        let out = filter_sort(snapshot, show_hidden, &filter, col, asc, &restore);
        let pass_ms = perf::elapsed_ms(t0);
        let _ = slint::invoke_from_event_loop(move || {
            with_state(|st| land_pass(st, pane, id, (generation, pass, total, sel_version), out, (col, asc, pass_ms), reveal, &filter));
        });
    });
}

/// A pass came back: swap it into its tab's model if nothing changed underneath it.
fn land_pass(
    st: &mut State,
    pane: usize,
    id: u64,
    (generation, pass, total, sel_version): (u64, u64, usize, u64),
    mut out: model::PassOutput,
    (col, asc, pass_ms): (SortCol, bool, f64),
    reveal: bool,
    filter: &NameMatch,
) {
    let Some(app) = st.app.upgrade() else { return };
    let active = st.is_active(pane, id);
    let Some(t) = st.find(pane, id) else { return }; // the tab was closed
    let model = t.model.clone();
    if model.generation.get() != generation || model.pass.get() != pass || model.total() != total {
        return; // listing or request changed underneath us; a newer pass or Done follows
    }
    let cursor_now = if active { current_row(&app, pane) } else { t.cursor };
    if model.sel_version.get() != sel_version {
        // The user changed the selection while the pass ran; carry the current one.
        out.restore(&model.capture(cursor_now));
    }
    eprintln!(
        "[sort] pane={pane} {} of {total} entries by {col:?} asc={asc} hidden={} cursor={:?} selected={} in {pass_ms:.1} ms",
        out.shown.len(),
        out.hidden,
        out.cursor,
        out.selection.len()
    );
    if !filter.is_empty() {
        eprintln!(
            "[filter] pane={pane} \"{}\" ({}): {} of {} shown, pass {pass_ms:.1} ms",
            filter.pattern,
            if filter.is_glob() { "glob" } else { "substring" },
            out.shown.len(),
            out.shown.len() + out.filtered
        );
    }
    let keep = std::mem::take(&mut t.keep_scroll);
    let scroll = (keep && active).then(|| app.invoke_get_scroll(pane as i32));
    let cursor = model.replace(out);
    if !active {
        t.cursor = cursor.map_or(-1, |c| c as i32);
        return;
    }
    set_current_row(&app, pane, cursor);
    if let (Some(y), Some(_)) = (scroll, cursor) {
        app.invoke_set_scroll(pane as i32, y);
        eprintln!("[refresh] pane={pane} scroll restored to {y:.0} (now {:.0})", app.invoke_get_scroll(pane as i32));
    }
    // Once per pass swap, never per frame: bring an operation's cursor row into view.
    if let (true, Some(row)) = (reveal, cursor) {
        let y = app.invoke_get_scroll(pane as i32);
        if let Some(to) = model::reveal_row(y, st.view[pane].1, row, icons::ROW_PX) {
            app.invoke_set_scroll(pane as i32, to);
            eprintln!("[refresh] pane={pane} cursor row {row} scrolled into view: {y:.0} -> {to:.0}");
        }
    }
    size_selection(st, pane);
    update_status(st, &app, pane);
    st.strip_dirty[pane] = true;
    resolve_view(st, &app, pane);
}

/// Fetch or apply icons for the rows on screen (plus a margin) in one pane, then recompose
/// its icon strip if the visible rows left it or an icon in it changed.
fn resolve_view(st: &mut State, app: &App, pane: usize) {
    if pane == 1 && !app.get_split() {
        return;
    }
    let t = st.panes[pane].active();
    let model = t.model.clone();
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
    let (folder, tx) = (&t.path, &st.icon_tx);
    let mut sent = 0;
    let changed = model.resolve_icons(first.saturating_sub(ICON_MARGIN_ROWS), last + ICON_MARGIN_ROWS, |e| {
        let path = if e.kind == Kind::Drive { PathBuf::from(e.name.as_str()) } else { folder.join(e.name.as_str()) };
        let overlay = icons::wants_overlay(e.kind, e.attrs, &e.name);
        let _ = tx.send(icons::Request { generation, pane, key: e.icon_key.clone(), path, size_px, overlay });
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
    let model = st.model(pane);
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
        let overlays = st.icon_stats.overlays.load(Ordering::Relaxed);
        let slow = st.icon_stats.slow.load(Ordering::Relaxed);
        let (req0, rep0, ren0, ns0, comp0, ov0, slow0) = c.reported;
        if c.requests == req0 && c.replies == rep0 && c.composes == comp0 {
            return;
        }
        let n = rendered - ren0;
        eprintln!(
            "[icons] +{} requests +{} replies (total {} / {}), table {} images, worker {n} rendered avg {:.2} ms/icon, overlays {}, slow {}, resolve {} calls avg {:.1} us, compose {} strips avg {:.1} us",
            c.requests - req0,
            c.replies - rep0,
            c.requests,
            c.replies,
            st.icons.borrow().len(),
            if n > 0 { (render_ns - ns0) as f64 / n as f64 / 1e6 } else { 0.0 },
            overlays - ov0,
            slow - slow0,
            c.resolves,
            if c.resolves > 0 { c.resolve_ns as f64 / c.resolves as f64 / 1e3 } else { 0.0 },
            c.composes,
            if c.composes > 0 { c.compose_ns as f64 / c.composes as f64 / 1e3 } else { 0.0 },
        );
        c.reported = (c.requests, c.replies, rendered, render_ns, c.composes, overlays, slow);
    });
}

/// The main window's HWND, fetched once the window exists.
fn main_hwnd(st: &mut State, app: &App) -> isize {
    if st.hwnd == 0 {
        let handle = app.window().window_handle();
        if let Ok(h) = handle.window_handle() {
            if let RawWindowHandle::Win32(w) = h.as_raw() {
                st.hwnd = w.hwnd.get();
                devices::install(st.hwnd);
            }
        }
    }
    st.hwnd
}

/// Show `text` as the active tab's note now.
fn note(st: &mut State, app: &App, pane: usize, text: String) {
    eprintln!("[fileops] pane={pane} note: {text}");
    st.tab_mut(pane).note = text;
    update_status(st, app, pane);
}

/// `p` is `root` or lies under it (case-insensitive, whole components).
fn inside(p: &Path, root: &Path) -> bool {
    let (p, r) = (p.to_string_lossy().to_lowercase(), root.to_string_lossy().trim_end_matches('\\').to_lowercase());
    p == r || p.strip_prefix(&r).is_some_and(|rest| rest.starts_with('\\'))
}

/// Full paths of the selection (or the cursor row) in a pane, or a note saying why there are none.
fn sources(st: &mut State, app: &App, pane: usize) -> Option<(Vec<PathBuf>, Vec<SharedString>)> {
    let targets = st.model(pane).targets(current_row(app, pane));
    if targets.is_empty() {
        note(st, app, pane, "nothing selected".into());
        return None;
    }
    if targets.iter().any(|e| e.kind == Kind::Drive) {
        note(st, app, pane, "drives cannot be copied, moved, renamed or deleted".into());
        return None;
    }
    let folder = &st.tab(pane).path;
    Some((targets.iter().map(|e| folder.join(e.name.as_str())).collect(), targets.into_iter().map(|e| e.name).collect()))
}

/// Queue a job on the worker; the source pane's active tab shows its progress note. Returns the
/// job id.
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
    let tab = st.tab(pane).id;
    st.jobs.insert(id, JobInfo { pane, tab, fallback: None, undo_of: None });
    note(st, app, pane, running);
    if st.jobs.len() > 1 {
        eprintln!("[fileops] {} operation(s) queued", st.jobs.len() - 1);
        update_status(st, app, 1 - pane);
    }
    Some(id)
}

/// Before a job removes `names` from a pane's active tab: remember where its cursor falls back to
/// (the row after it) if the cursor row goes. Attached to the job by `submit_removing`.
fn removal_fallback(st: &State, app: &App, pane: usize, names: &[SharedString]) -> Option<(usize, u64, usize)> {
    let idx = st.model(pane).cursor_after_removal(current_row(app, pane), names)?;
    Some((pane, st.tab(pane).id, idx))
}

/// `submit` for a job that removes rows from a folder shown in a tab, with that tab's cursor
/// fallback.
fn submit_removing(st: &mut State, app: &App, pane: usize, job: Job, fallback: Option<(usize, u64, usize)>) -> Option<u64> {
    let id = submit(st, app, pane, job)?;
    if let Some(j) = st.jobs.get_mut(&id) {
        j.fallback = fallback;
    }
    Some(id)
}

/// A file operation key in a pane's list. Copy and move go to the other pane's folder; rename
/// and new folder open the inline name editor, which commits through `edit_commit`.
fn command(pane: usize, cmd: &str) {
    match cmd {
        "cc" | "cx" => return clipboard_put(pane, cmd == "cx"),
        "cv" => return clipboard_paste(pane),
        "undo" => return undo(pane),
        "cancelqueued" => return cancel_queued(pane),
        _ => {}
    }
    let edit = with_state(|st| {
        let app = st.app.upgrade()?;
        let folder = st.tab(pane).path.clone();
        let at_drives = is_this_pc(&folder.to_string_lossy());
        match cmd {
            "sizeall" => size_all(st, &app, pane),
            "newfolder" if at_drives => note(st, &app, pane, "This PC lists drives; open a folder first".into()),
            "newfolder" if st.is_search(pane) => note(st, &app, pane, "search results: open a folder to make a folder in it".into()),
            "newfolder" => return Some((app, 2, SharedString::from("New folder"))),
            "rename" => match usize::try_from(current_row(&app, pane)).ok().and_then(|r| st.model(pane).entry(r)) {
                None => note(st, &app, pane, "nothing to rename".into()),
                Some(e) if e.kind == Kind::Drive => note(st, &app, pane, "drives cannot be renamed here".into()),
                Some(e) => return Some((app, 1, e.name)),
            },
            "copy" | "move" => {
                if !app.get_split() {
                    note(st, &app, pane, "open the second pane (F3) first".into());
                    return None;
                }
                let dest = st.tab(1 - pane).path.clone();
                if st.is_search(1 - pane) {
                    note(st, &app, pane, "the other pane shows search results; open a folder there first".into());
                    return None;
                }
                if is_this_pc(&dest.to_string_lossy()) {
                    note(st, &app, pane, "the other pane lists drives; open a folder there first".into());
                    return None;
                }
                if cmd == "move" && same_dir(&folder, &dest) {
                    note(st, &app, pane, "source and destination are the same folder".into());
                    return None;
                }
                let (sources, names) = sources(st, &app, pane)?;
                if cmd == "move" {
                    let fb = removal_fallback(st, &app, pane, &names);
                    submit_removing(st, &app, pane, Job::Move { sources, dest_dir: dest }, fb);
                } else {
                    submit(st, &app, pane, Job::Copy { sources, dest_dir: dest });
                }
            }
            "delete" | "sdelete" => {
                let (sources, names) = sources(st, &app, pane)?;
                let fb = removal_fallback(st, &app, pane, &names);
                submit_removing(st, &app, pane, Job::Delete { sources, permanent: cmd == "sdelete" }, fb);
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
        // A search result's name is its path under the root (`sub\file.txt`); the editor shows it
        // whole, and a rename keeps it in its folder.
        let parent = original.rsplit_once('\\').map_or("", |(p, _)| p);
        let typed = typed.trim();
        let name = typed.strip_prefix(parent).and_then(|t| t.strip_prefix('\\')).filter(|_| mode == 1 && !parent.is_empty()).unwrap_or(typed);
        if let Some(why) = fileops::invalid_name(name) {
            note(st, &app, pane, why.into());
            return;
        }
        let folder = st.tab(pane).path.clone();
        let job = match mode {
            1 if name == original || (!parent.is_empty() && original.ends_with(&format!("\\{name}"))) => return,
            1 => Job::Rename { source: folder.join(original), new_name: name.into() },
            2 => Job::NewFolder { parent: folder, name: name.into() },
            _ => return,
        };
        submit(st, &app, pane, job);
    });
}

/// For a paste of a cut: the cut tab's cursor fallback, its cut names being what the move removes.
fn cut_fallback(st: &mut State, app: &App) -> Option<(usize, u64, usize)> {
    let (pane, tab) = st.cut_source?;
    let active = st.is_active(pane, tab);
    let row = if active { current_row(app, pane) } else { st.find(pane, tab)?.cursor };
    let t = st.find(pane, tab)?;
    let names = t.model.capture(-1).cut;
    Some((pane, tab, t.model.cursor_after_removal(row, &names)?))
}

/// Ctrl+Z in a pane's list: undo the newest recorded operation, whichever pane it was done in.
/// Refused with a note when the items are no longer where it left them; the undo job runs
/// through the worker like any other (the soak fence included) and is not recorded itself.
fn undo(pane: usize) {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let Some(rec) = st.undo.pop() else {
            eprintln!("[undo] nothing to undo");
            return note(st, &app, pane, "Nothing to undo".into());
        };
        let Some(u) = rec.undo else {
            eprintln!("[undo] \"{}\" cannot be undone", rec.text);
            return note(st, &app, pane, fileops::undo::CANNOT_UNDO_DELETE.into());
        };
        if let Err(why) = fileops::undo::check(&u) {
            eprintln!("[undo] \"{}\" refused: {why}", rec.text);
            return note(st, &app, pane, why);
        }
        eprintln!("[undo] \"{}\" with {:?}", rec.text, u.job);
        // Undoing a copy or a new folder in this pane's folder removes rows from it.
        let fallback = match &u.job {
            Job::Delete { sources, .. } => {
                let folder = st.tab(pane).path.clone();
                let names: Vec<SharedString> = sources
                    .iter()
                    .filter(|s| s.parent().is_some_and(|p| same_dir(p, &folder)))
                    .filter_map(|s| Some(s.file_name()?.to_string_lossy().as_ref().into()))
                    .collect();
                removal_fallback(st, &app, pane, &names)
            }
            _ => None,
        };
        if let Some(id) = submit_removing(st, &app, pane, u.job, fallback) {
            if let Some(j) = st.jobs.get_mut(&id) {
                j.undo_of = Some(rec.text);
            }
        }
    });
}

/// Escape in a pane's list: cancel every queued job that has not started (each reports
/// `Cancelled` through `job_done`); the running one goes on.
fn cancel_queued(pane: usize) {
    with_state(|st| {
        // Escape in a search tab's list also stops its walk; the results so far stay.
        let t = st.tab(pane);
        if let Some(s) = t.search.as_ref().filter(|_| t.timing.done_ms.is_none() && t.timing.error.is_none()) {
            s.cancel(fs::enumerate::SEARCH_ESCAPE);
            eprintln!("[search] pane={pane} Escape: stopping the search");
        }
        let queued = st.jobs.len().saturating_sub(1);
        if queued == 0 {
            return;
        }
        st.ops.cancel_queued(st.next_job);
        eprintln!("[fileops] Escape: cancelling {queued} queued operation(s) up to id {}", st.next_job);
        if let Some(app) = st.app.upgrade() {
            note(st, &app, pane, format!("Cancelling {queued} queued operation{}", if queued == 1 { "" } else { "s" }));
        }
    });
}

/// Clear the cut marks in every tab (the clipboard no longer holds a cut).
fn clear_cuts(st: &mut State) {
    st.cut_source = None;
    for tabs in &st.panes {
        for t in tabs.iter() {
            t.model.set_cut(Vec::new());
        }
    }
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
    let seq = clipboard::sequence();
    with_state(|st| {
        st.clip_seq = seq;
        let Some(app) = st.app.upgrade() else { return };
        match res {
            Ok(()) => {
                let n = paths.len();
                let what = if cut { "Cut" } else { "Copied" };
                note(st, &app, pane, format!("{what} {n} item{} to the clipboard", if n == 1 { "" } else { "s" }));
                clear_cuts(st);
                if cut {
                    st.model(pane).set_cut(names);
                    st.cut_source = Some((pane, st.tab(pane).id));
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
        if is_this_pc(&st.tab(pane).path.to_string_lossy()) {
            note(st, &app, pane, "This PC lists drives; open a folder first".into());
            return None;
        }
        if st.is_search(pane) {
            note(st, &app, pane, "search results: paste into a folder".into());
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
        let dest = st.tab(pane).path.clone();
        if effect == clipboard::EFFECT_MOVE {
            if sources.iter().all(|s| s.parent().is_some_and(|d| same_dir(d, &dest))) {
                return note(st, &app, pane, "source and destination are the same folder".into());
            }
            let fb = cut_fallback(st, &app);
            st.paste_move = submit_removing(st, &app, pane, Job::Move { sources, dest_dir: dest }, fb);
        } else {
            submit(st, &app, pane, Job::Copy { sources, dest_dir: dest });
        }
    });
}

/// Every tab not on screen goes stale: it refreshes when it comes to the front. After a file
/// operation or shell command, any folder may have changed.
fn mark_background_stale(st: &mut State) {
    for tabs in st.panes.iter_mut() {
        let active = tabs.active_index();
        for (i, t) in tabs.iter_mut().enumerate() {
            if i != active {
                t.stale = true;
            }
        }
    }
}

/// A job finished (on the UI thread, from the worker's wake-up): show the result on the source
/// tab and refresh both panes' active tabs (the others refresh when they come to the front).
/// Moved and deleted rows drop out of the selection because the refresh restores it by name; a
/// copy keeps it.
fn job_done(d: fileops::Done) {
    let mut clear_clipboard = None;
    let panes = with_state(|st| {
        let info = st.jobs.remove(&d.id).unwrap_or(JobInfo { pane: 0, tab: 0, fallback: None, undo_of: None });
        let (pane, tab) = (info.pane, info.tab);
        let text = fileops::status_text(&d.job, &d.outcome, &d.results);
        let text = match &info.undo_of {
            // An undo job is never recorded itself (no redo).
            Some(orig) => {
                let shown = if d.outcome == Outcome::Done { format!("Undid: {orig}") } else { format!("Undo: {text}") };
                eprintln!("[undo] done id={} {:?}: {shown}", d.id, d.outcome);
                shown
            }
            None => {
                if let Some(r) = fileops::undo::record_for(&d.job, &d.outcome, &d.results, &text) {
                    eprintln!("[undo] recorded \"{}\": {:?}", r.text, r.undo.as_ref().map(|u| &u.job));
                    fileops::undo::push(&mut st.undo, r);
                }
                text
            }
        };
        // Whatever the outcome (a cancel can come after some items went), the source tab's next
        // pass puts a cursor whose row is gone on the row after it.
        if let Some((p, id, idx)) = info.fallback {
            if let Some(t) = st.find(p, id) {
                t.fallback_cursor = Some(idx);
            }
        }
        eprintln!("[fileops] done id={} pane={pane} tab={tab} {:?}: {text}", d.id, d.outcome);
        for r in &d.results {
            let to = r.dest.as_ref().map_or_else(|| "-".to_string(), |p| p.display().to_string());
            eprintln!("[fileops]   item 0x{:08X} {} -> {to}", r.hr as u32, r.source.display());
        }
        if let Some(t) = st.find(pane, tab) {
            if d.outcome == Outcome::Done {
                // The name the shell really used ("New folder (2)"), else the one asked for.
                match &d.job {
                    Job::Rename { new_name: n, .. } | Job::NewFolder { name: n, .. } => {
                        let real = fileops::real_name(&d.job, &d.results).unwrap_or_else(|| n.clone());
                        t.focus_name = Some(real.as_str().into());
                    }
                    _ => {}
                }
            }
            t.note = text;
        }
        if st.paste_move == Some(d.id) {
            st.paste_move = None;
            if d.outcome == Outcome::Done {
                clear_clipboard = Some(st.hwnd);
                clear_cuts(st);
            }
        }
        mark_background_stale(st);
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
        let folder = st.tab(pane).path.clone();
        let this_pc = is_this_pc(&folder.to_string_lossy());
        let targets = if row >= 0 { st.model(pane).targets(current_row(&app, pane)) } else { Vec::new() };
        let names: Vec<String> = targets.iter().map(|e| e.name.to_string()).collect();
        if targets.is_empty() && st.is_search(pane) {
            // Its "Paste" and "New" would act on the root, which is not what the list shows.
            note(st, &app, pane, "search results have no folder menu; right-click a result".into());
            return None;
        }
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
            let split = with_state(|st| {
                mark_background_stale(st);
                st.app.upgrade().is_some_and(|a| a.get_split())
            });
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
    let len = with_state(|st| st.model(pane).len());
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
                        clear_cuts(st);
                        let n = ctx.paths.len();
                        note(st, &app, pane, format!("Copied {n} path{}", if n == 1 { "" } else { "s" }));
                    }
                    Err(e) => note(st, &app, pane, format!("clipboard: {e}")),
                }
            });
        }
    }
}

/// Return, or a double-click that was not the tail of a drag: enter the row's directory (also a
/// reparse directory or a drive), or open the file with its default program.
fn open_row(pane: usize, row: i32) {
    let target = with_state(|st| {
        let e = st.model(pane).entry(usize::try_from(row).ok()?)?;
        let app = st.app.upgrade()?;
        let folder = st.tab(pane).path.clone();
        let hwnd = main_hwnd(st, &app);
        let path = folder.join(e.name.as_str());
        // The file's own folder (a search result's may lie below the tab's).
        let dir = path.parent().map_or(folder, Path::to_path_buf);
        Some((e.is_dir(), path, dir, hwnd))
    });
    match target {
        Some((true, p, _, _)) => navigate(pane, p),
        Some((false, file, folder, hwnd)) => {
            // No state borrow across ShellExecuteExW: the shell can show a dialog from inside it.
            let res = shellmenu::open_item(hwnd, &file, &folder);
            eprintln!("[open] pane={pane} {}: {res:?}", file.display());
            if let Err(code) = res {
                with_state(|st| {
                    if let Some(app) = st.app.upgrade() {
                        let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        note(st, &app, pane, format!("could not open {name}: {}", std::io::Error::from_raw_os_error(code as i32)));
                    }
                });
            }
        }
        None => {}
    }
}

fn log_select(model: &PaneModel, pane: usize, op: SelOp, row: usize) {
    eprintln!("[select] pane={pane} {op:?} row={row} selected={}", model.selected_count());
}

/// Backspace and Alt+Up: the active tab's parent folder (a drive or share root goes up to This
/// PC). False on This PC, which has no parent.
fn go_up(pane: usize) -> bool {
    let target = with_state(|st| {
        let base = st.tab(pane).path.clone();
        if is_this_pc(&base.to_string_lossy()) {
            return None;
        }
        // A drive root (or UNC share root) has no parent: go up to the drive list.
        Some(base.parent().map_or_else(|| PathBuf::from(THIS_PC), |p| p.to_path_buf()))
    });
    match target {
        Some(p) => {
            navigate(pane, p);
            true
        }
        None => false,
    }
}

/// Alt+Left (back) and Alt+Right (`forward`), and the side buttons through `history_button`:
/// move through the active tab's history without recording the move. With nothing to go to it
/// logs one line and does nothing (no fallback to up a folder).
fn history_move(pane: usize, forward: bool) {
    let dir = if forward { "forward" } else { "back" };
    let target = with_state(|st| {
        let t = st.tab_mut(pane);
        let here = t.place();
        let to = if forward { t.history.forward(&here) } else { t.history.back(&here) };
        match &to {
            Some(to) => eprintln!(
                "[nav] pane={pane} history {dir}: {here} -> {to} ({} back, {} forward left)",
                t.history.back_len(),
                t.history.forward_len()
            ),
            None => eprintln!("[nav] pane={pane} history {dir}: nothing to go {dir} to"),
        }
        to
    });
    match target {
        // A search in the history runs again, in this tab, under the same root.
        Some(Place { path, search: Some(pattern) }) => start_search(pane, &pattern, Some(path)),
        Some(Place { path, search: None }) => navigate_to(pane, path, false),
        None => {}
    }
}

/// The mouse's Back or Forward button released over a pane (any part of it but a text entry, the
/// tab strip and the scrollbar): that pane becomes active, as a click there would make it, and
/// goes back or forward in its active tab's history.
fn history_button(pane: usize, forward: bool) {
    let Some(app) = with_state(|st| st.app.upgrade()) else { return };
    if pane == 1 && !app.get_split() {
        return;
    }
    let was = app.get_active_pane();
    app.invoke_focus_pane(pane as i32);
    let name = if forward { "Forward" } else { "Back" };
    eprintln!("[nav] pane={pane} {name} button (active pane was {was}, now {pane})");
    history_move(pane, forward);
}

/// The middle button on a row: a folder (or a drive, or a reparse directory) opens in a new tab
/// in the same pane, as a middle-click in the sidebar does; a file does nothing.
fn middle_click_row(pane: usize, row: usize) {
    let target = with_state(|st| {
        let e = st.model(pane).entry(row)?;
        Some((e.is_dir(), st.tab(pane).path.join(e.name.as_str())))
    });
    match target {
        Some((true, path)) => {
            eprintln!("[tab] pane={pane} middle-click row {row}: new tab on {}", path.display());
            open_tab(pane, Some(path));
        }
        Some((false, path)) => eprintln!("[tab] pane={pane} middle-click row {row}: {} is a file, nothing to do", path.display()),
        None => {}
    }
}

/// The left button on a row: press (0), release (1), moved past the drag threshold with the
/// button down (2), double-click (3); the middle button's release (4), the Back button's (5) and
/// the Forward button's (7).
/// `dnd::Gesture` decides what selects, drags and opens.
fn row_gesture(pane: usize, row: i32, what: i32, ctrl: bool, shift: bool) {
    let Ok(row) = usize::try_from(row) else { return };
    match what {
        0 => with_state(|st| {
            let model = st.panes[pane].active().model.clone();
            let selected = model.entry(row).is_some_and(|e| e.selected);
            if let Some(op) = st.gesture[pane].press(row, selected, ctrl, shift) {
                model.select(op, row);
                log_select(&model, pane, op, row);
                size_selection(st, pane);
                if let Some(app) = st.app.upgrade() {
                    update_status(st, &app, pane);
                }
            }
        }),
        1 => with_state(|st| {
            if let Some((r, op)) = st.gesture[pane].release() {
                st.model(pane).select(op, r);
                log_select(st.model(pane), pane, op, r);
                size_selection(st, pane);
                if let Some(app) = st.app.upgrade() {
                    update_status(st, &app, pane);
                }
            }
        }),
        2 => drag_start(pane, row, false),
        6 => drag_start(pane, row, true),
        3 => {
            if with_state(|st| st.gesture[pane].double_click()) {
                open_row(pane, row as i32);
            } else {
                eprintln!("[dnd] pane={pane} double-click at the end of a drag: not opened");
            }
        }
        4 => middle_click_row(pane, row),
        5 => history_button(pane, false),
        7 => history_button(pane, true),
        _ => {}
    }
}

/// The pointer left the drag threshold with the left button (or, `right`, the right button) down
/// on `row`: drag the selection (the press made sure it includes the row) with OLE. The request
/// is taken out of the state here and run by `run_pending_drag` from a posted message.
fn drag_start(pane: usize, row: usize, right: bool) {
    let button = if right { VK_RBUTTON } else { VK_LBUTTON };
    // SAFETY: plain Win32 query of this thread's key state.
    let held = dnd::scripted() || unsafe { GetKeyState(button as i32) } < 0;
    let taken = with_state(|st| {
        let app = st.app.upgrade()?;
        if !held {
            // Slint thinks the button is down but it is not (a release it never saw): no drag,
            // and the gesture ends like one, so the release given below selects nothing.
            eprintln!("[dnd] pane={pane} drag threshold crossed with the {} button up: no drag", if right { "right" } else { "left" });
            st.gesture[pane] = if right { dnd::Gesture::Idle } else { dnd::Gesture::Dragged };
            return Some(None);
        }
        if !(if right { st.gesture[pane].drag_right(row) } else { st.gesture[pane].drag(row) }) {
            return None;
        }
        let folder = st.tab(pane).path.clone();
        let targets = st.model(pane).targets(current_row(&app, pane));
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
        eprintln!("[dnd] pane={pane} row={row}: dragging {} item(s){}", names.len(), if right { " with the right button" } else { "" });
        // The image shows the pressed row's icon (or the first item's), with the count.
        let pressed = st.model(pane).entry(row).filter(|e| targets.iter().any(|t| t.name == e.name));
        let shown = pressed.as_ref().unwrap_or(&targets[0]);
        let image = dnd::DragImage {
            path: folder.join(shown.name.as_str()),
            cached: Some(st.icons.borrow().buffer(shown.icon).clone()),
            count: names.len(),
            scale: app.window().scale_factor(),
        };
        Some(Some(dnd::DragRequest { hwnd, folder, names, image: Some(image), right, scripted: dnd::scripted() }))
    });
    let req = match taken {
        None => return,
        Some(None) => return slint::Timer::single_shot(Duration::ZERO, move || release_after_drag(right)),
        Some(Some(req)) => req,
    };
    // The drag runs from a posted message, not inside this pointer event: OLE's loop then runs
    // outside winit's event handler, so the WM_PAINT it dispatches reaches Slint and the
    // highlight (and auto-scroll) render during our own drag. The button is still down when the
    // message arrives (posted messages come before input); a drag that finds it released drops
    // where it started, as a real one would.
    let hwnd = req.hwnd;
    PENDING_DRAG.with(|p| *p.borrow_mut() = Some((pane, req)));
    // SAFETY: posting a message without pointers to our own window.
    unsafe { windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(hwnd as windows_sys::Win32::Foundation::HWND, dnd::WM_RUN_DRAG, 0, 0) };
}

thread_local! {
    /// The drag `drag_start` asked for, until the posted `WM_RUN_DRAG` runs it.
    static PENDING_DRAG: RefCell<Option<(usize, dnd::DragRequest)>> = const { RefCell::new(None) };
}

/// A drag was asked for and has not finished (for the driver, which waits for it).
pub fn drag_pending() -> bool {
    PENDING_DRAG.with(|p| p.borrow().is_some()) || dnd::own_drag_running()
}

/// `WM_RUN_DRAG` arrived (the permanent subclass, dispatched by winit's message loop outside its
/// event handler): run the pending drag. `DoDragDrop` runs a modal loop; no borrow is held.
pub fn run_pending_drag() {
    let Some((pane, req)) = PENDING_DRAG.with(|p| p.borrow_mut().take()) else { return };
    let right = req.right;
    if !state_is_free() {
        // Not expected: the message is dispatched by an event loop, never inside a borrow.
        eprintln!("[dnd] pane={pane} pending drag dropped: app state busy");
        with_state(|st| st.gesture[pane].drag_ended());
        return slint::Timer::single_shot(Duration::ZERO, move || release_after_drag(right));
    }
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
    // row's grab. Give it a release from the event loop.
    slint::Timer::single_shot(Duration::ZERO, move || release_after_drag(right));
}

/// The release Slint missed during `DoDragDrop`, at the cursor's position. The gesture is
/// `Dragged`, so it neither selects nor (as a double-click) opens. After a right-button drag a
/// right release would open a context menu wherever the cursor is, so Slint gets the pointer
/// leaving the window instead, which ends the row's grab (and cancels the list's right press).
fn release_after_drag(right: bool) {
    let Some((app, hwnd)) = with_state(|st| Some((st.app.upgrade()?, st.hwnd))) else { return };
    if right {
        eprintln!("[dnd] right-button drag over: pointer grab ended without a release (no context menu)");
        app.window().dispatch_event_with_result(slint::platform::WindowEvent::PointerExited).ok();
        return;
    }
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
    // The window exists now: a window saved maximised is maximised again (its normal placement
    // was restored before it was created, so un-maximising gives that size).
    if with_state(|st| {
        st.dnd_timer.stop();
        // The panes have their width now: scroll restored tab strips to their active tabs.
        update_tabs(st, 0);
        update_tabs(st, 1);
        st.config.window.is_some_and(|g| g.maximized)
    }) {
        // SAFETY: our own top-level window, on its thread.
        unsafe { ShowWindow(hwnd as windows_sys::Win32::Foundation::HWND, SW_MAXIMIZE) };
        eprintln!("[settings] window restored maximised");
    }
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

/// The window's layout as `dnd::hit` and the driver's geometry need it (logical px), read from
/// the window and its properties, so it needs no app-state borrow.
pub fn window_layout(app: &App) -> dnd::Layout {
    let size = app.window().size().to_logical(app.window().scale_factor());
    dnd::Layout {
        width: size.width,
        height: size.height,
        split: app.get_split(),
        ratio: app.get_split_ratio(),
        tabs: [app.get_left_tabs().row_count() > 1, app.get_right_tabs().row_count() > 1],
        left: if app.get_sidebar_visible() { app.get_sidebar_width() + 4.0 } else { 12.0 },
    }
}

/// For the drop target: where a drop at client point (`px`, `py`, physical px) goes. A short
/// borrow of the state, never held past this function; None off the panes, over the drive
/// list's empty space, or if the state is busy (not expected: no borrow is held across a drag
/// loop or a dispatched message).
pub fn drop_spot(px: i32, py: i32) -> Option<dnd::DropSpot> {
    let spot = drop_spot_at(px, py);
    if spot.is_some() {
        DROP_REFUSED.with(|c| c.set(None));
    }
    spot
}

thread_local! {
    /// The search tab (pane, tab id) a drag over it was last refused for, so the refusal is
    /// logged once per hover rather than on every DragOver.
    static DROP_REFUSED: std::cell::Cell<Option<(usize, u64)>> = const { std::cell::Cell::new(None) };
}

/// A drop on a search tab (its list, or its tab in the strip) goes nowhere: its folder is the
/// search's root, not what the list shows. Logged once per hover.
fn refuse_search_drop(pane: usize, id: u64) {
    if DROP_REFUSED.with(|c| c.replace(Some((pane, id)))) != Some((pane, id)) {
        eprintln!("[dnd] pane {pane} tab {id} shows search results: no drop there (it would land in the search's root)");
    }
}

fn drop_spot_at(px: i32, py: i32) -> Option<dnd::DropSpot> {
    STATE.with(|s| {
        let Ok(guard) = s.try_borrow() else {
            eprintln!("[dnd] app state busy during a drop target call: refused");
            return None;
        };
        let st = guard.as_ref()?;
        let app = st.app.upgrade()?;
        let scale = app.window().scale_factor();
        let layout = window_layout(&app);
        let content_y = [app.invoke_get_scroll(0), app.invoke_get_scroll(1)];
        let len = [st.model(0).len(), st.model(1).len()];
        let side_rows = side_rows(st, &app);
        let strips = strips(st, &app, &side_rows);
        let (pane, row) = match dnd::hit_all(&layout, &strips, px as f32 / scale, py as f32 / scale, content_y, len) {
            dnd::Spot::Outside => return None,
            dnd::Spot::Pane(p) => (p, None),
            dnd::Spot::Row(p, r) => (p, Some(r)),
            // A sidebar row's folder (a drive's root, a place, a bookmark), noted in the active pane.
            dnd::Spot::Side(i) => {
                let dest = st.side.get(i)?.path.clone()?;
                return Some(dnd::DropSpot { pane: active_pane(&app), mark: dnd::Mark::Side(i), dest });
            }
            // A tab's folder; a tab on This PC is no folder.
            dnd::Spot::Tab(p, i) => {
                let tab = st.panes[p].iter().nth(i)?;
                if tab.search.is_some() {
                    refuse_search_drop(p, tab.id);
                    return None;
                }
                let dest = tab.path.clone();
                if dest.as_os_str().is_empty() || is_this_pc(&dest.to_string_lossy()) {
                    return None;
                }
                return Some(dnd::DropSpot { pane: p, mark: dnd::Mark::Tab(i), dest });
            }
        };
        if st.is_search(pane) {
            refuse_search_drop(pane, st.tab(pane).id);
            return None;
        }
        let folder = &st.tab(pane).path;
        let entry = row.and_then(|r| st.model(pane).entry(r));
        let this_pc = is_this_pc(&folder.to_string_lossy());
        let (dest, on_row) = dnd::drop_folder(folder, this_pc, entry.as_ref().map(|e| (e.name.as_str(), e.is_dir())))?;
        let row = if on_row { row.map_or(-1, |r| r as i32) } else { -1 };
        Some(dnd::DropSpot { pane, mark: dnd::Mark::Row(row), dest })
    })
}

/// The sidebar's rows as the drop target's hit test sees them: each row's height (sidebar.slint
/// `SideRow`: a header 26 px, a row 24 px, the row being renamed 30 px) and whether it is a drop
/// target (every row but a header).
fn side_rows(st: &State, app: &App) -> Vec<(f32, bool)> {
    let editing = app.get_side_edit();
    st.side
        .iter()
        .enumerate()
        .map(|(i, r)| match r.kind {
            sidebar::RowKind::Header => (26.0, false),
            _ if i as i32 == editing => (30.0, true),
            _ => (24.0, true),
        })
        .collect()
}

/// The sidebar and the tab strips for `dnd::hit_all`.
fn strips<'a>(st: &State, app: &App, side_rows: &'a [(f32, bool)]) -> dnd::Strips<'a> {
    let tab = |p: usize| (st.panes[p].len(), app.invoke_tab_width(p as i32), app.invoke_tab_offset(p as i32));
    dnd::Strips {
        side_w: if app.get_sidebar_visible() { app.get_sidebar_width() } else { 0.0 },
        side_scroll: app.invoke_side_scroll(),
        side_rows,
        tabs: [tab(0), tab(1)],
    }
}

/// For the driver's `dropfiles side <row>`: sidebar row `i`'s vertical centre in the window
/// (logical px), if the row exists; a negative `i` counts from the end (-1: the last row, the
/// newest bookmark).
pub fn side_row_centre(i: i32) -> Option<f32> {
    with_state(|st| {
        let app = st.app.upgrade()?;
        let rows = side_rows(st, &app);
        let i = if i < 0 { rows.len().checked_sub(i.unsigned_abs() as usize)? } else { i as usize };
        let h = rows.get(i)?.0;
        Some(dnd::SIDE_TOP + app.invoke_side_scroll() + dnd::side_row_top(&rows, i) + h / 2.0)
    })
}

/// For the drop target's DragOver: a drag hovering near a list's top or bottom edge (client
/// point, physical px) scrolls that list by one row. True when it scrolled. The state is read
/// under a short borrow, released before the scroll is set (`set-scroll` is a property set).
pub fn drop_autoscroll(px: i32, py: i32) -> bool {
    let found = STATE.with(|s| {
        let guard = s.try_borrow().ok()?;
        let st = guard.as_ref()?;
        let app = st.app.upgrade()?;
        let scale = app.window().scale_factor();
        let layout = window_layout(&app);
        let (pane, dir) = dnd::edge_scroll(&layout, px as f32 / scale, py as f32 / scale)?;
        let visible = layout.height - dnd::LIST_BOTTOM_GAP - dnd::list_top(&layout, pane);
        Some((app, pane, dir, st.model(pane).len(), visible))
    });
    let Some((app, pane, dir, len, visible)) = found else { return false };
    let from = app.invoke_get_scroll(pane as i32);
    let to = dnd::scrolled(from, dir, len, visible);
    if to == from {
        return false;
    }
    app.invoke_set_scroll(pane as i32, to);
    dnd::repaint_during_own_drag();
    eprintln!("[dnd] auto-scroll pane {pane} {}: content-y {from:.0} -> {to:.0}", if dir < 0 { "up" } else { "down" });
    true
}

/// For the drop target: highlight what a drop would land on (a pane's row, or with row -1 its
/// whole list; a tab; a sidebar row), or nothing. Sets the properties only when they change.
pub fn drop_highlight(at: Option<(usize, dnd::Mark)>) {
    // The sidebar row's place, under a short borrow (only for a sidebar mark).
    let found = STATE.with(|s| {
        let guard = s.try_borrow().ok()?;
        let st = guard.as_ref()?;
        let app = st.app.upgrade()?;
        let side = match at {
            Some((_, dnd::Mark::Side(i))) => {
                let rows = side_rows(st, &app);
                rows.get(i).map(|r| (i as i32, dnd::side_row_top(&rows, i), r.0))
            }
            _ => None,
        };
        Some((app, side))
    });
    let Some((app, side)) = found else { return };
    let (pane, row, tab_pane, tab) = match at {
        Some((p, dnd::Mark::Row(r))) => (p as i32, r, -1, -1),
        Some((p, dnd::Mark::Tab(i))) => (-1, -1, p as i32, i as i32),
        _ => (-1, -1, -1, -1),
    };
    let (side_row, side_y, side_h) = side.unwrap_or((-1, 0.0, 0.0));
    let now = (app.get_drop_pane(), app.get_drop_row(), app.get_drop_tab_pane(), app.get_drop_tab(), app.get_drop_side_row());
    if now == (pane, row, tab_pane, tab, side_row) {
        return;
    }
    app.set_drop_pane(pane);
    app.set_drop_row(row);
    app.set_drop_tab_pane(tab_pane);
    app.set_drop_tab(tab);
    app.set_drop_side_y(side_y);
    app.set_drop_side_h(side_h);
    app.set_drop_side_row(side_row);
    dnd::repaint_during_own_drag();
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

/// For the scripted driver: a job is in flight or a tab is still re-enumerating.
pub fn ops_busy() -> bool {
    STATE.with(|s| match s.try_borrow() {
        Ok(g) => g.as_ref().is_some_and(|st| !st.jobs.is_empty() || st.panes.iter().any(|p| p.iter().any(|t| t.model.is_refreshing()))),
        Err(_) => true,
    })
}

fn on_frame_rendered() {
    with_state(|st| {
        for pane in 0..2 {
            let t = &mut st.panes[pane].active_mut().timing;
            if t.first_batch.is_some() && !t.first_frame_reported {
                t.first_frame_reported = true;
                if let Some(r) = t.requested {
                    eprintln!("[enum] pane={pane} first screenful rendered {:.1} ms after request", perf::elapsed_ms(r));
                }
            }
        }
        if let Some(s) = st.split_requested {
            if st.model(1).len() > 0 {
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

/// Put the pane's active tab on screen: its rows (the list re-points at its model), folder,
/// breadcrumb, sort indicators, cursor, scroll, status and icons.
fn show_tab(st: &mut State, app: &App, pane: usize) {
    let t = st.tab(pane);
    let (model, cursor, scroll) = (t.model.clone(), t.cursor, t.scroll);
    let rows = ModelRc::from(model.clone());
    if pane == 0 { app.set_left_rows(rows) } else { app.set_right_rows(rows) }
    let (col, asc) = (model.sort_col.get() as i32, model.sort_asc.get());
    if pane == 0 {
        app.set_left_sort_col(col);
        app.set_left_sort_asc(asc);
    } else {
        app.set_right_sort_col(col);
        app.set_right_sort_asc(asc);
    }
    set_path(st, app, pane);
    set_filter_box(app, pane, &model.filter.borrow().pattern);
    set_current_row(app, pane, usize::try_from(cursor).ok());
    app.invoke_set_scroll(pane as i32, scroll);
    st.view[pane].0 = app.invoke_get_scroll(pane as i32);
    st.icon_gens[pane].store(model.generation.get(), Ordering::Relaxed);
    st.gesture[pane] = dnd::Gesture::Idle;
    hide_strip(st, app, pane);
    st.strip_dirty[pane] = true;
    update_tabs(st, pane);
    update_status(st, app, pane);
    resolve_view(st, app, pane);
    st.watch_active(app, pane);
}

/// Remember the active tab's cursor and scroll before another tab takes the pane.
fn save_view(st: &mut State, app: &App, pane: usize) {
    let (cursor, scroll) = (current_row(app, pane), app.invoke_get_scroll(pane as i32));
    let t = st.tab_mut(pane);
    t.cursor = cursor;
    t.scroll = scroll;
}

/// Switch the pane to another tab (`pick` changes the active index); a stale tab refreshes.
fn switch_tab(pane: usize, pick: impl FnOnce(&mut Tabs<Tab>)) {
    let stale = with_state(|st| {
        let app = st.app.upgrade()?;
        let before = st.panes[pane].active_index();
        save_view(st, &app, pane);
        pick(&mut st.panes[pane]);
        if st.panes[pane].active_index() == before {
            return None;
        }
        // Its watch goes to the new tab, so what happens to its folder from now on is unknown. A
        // search tab has no watch to lose (and going stale would run its search again).
        if let Some(t) = st.panes[pane].iter_mut().nth(before).filter(|t| t.search.is_none()) {
            t.stale = true;
            t.drop_sizes(pane, "tab in the background");
        }
        show_tab(st, &app, pane);
        schedule_save(st);
        let t = st.tab(pane);
        eprintln!(
            "[tab] pane={pane} switched to {} of {} (id {}, {}), cursor {} scroll {:.0}",
            st.panes[pane].active_index() + 1,
            st.panes[pane].len(),
            t.id,
            t.path.display(),
            current_row(&app, pane),
            app.invoke_get_scroll(pane as i32)
        );
        Some(t.stale)
    });
    if stale == Some(true) {
        refresh(pane);
    }
}

/// Ctrl+T: a new tab after the active one, on the same folder (or `path`), sorted and filtered
/// like it.
fn open_tab(pane: usize, path: Option<PathBuf>) {
    let path = with_state(|st| {
        let app = st.app.upgrade()?;
        save_view(st, &app, pane);
        // Leaves the foreground and loses its watch (see switch_tab); a search tab has none.
        if !st.is_search(pane) {
            st.tab_mut(pane).stale = true;
            st.tab_mut(pane).drop_sizes(pane, "tab in the background");
        }
        let cur = st.tab(pane);
        let (col, asc, hidden) = (cur.model.sort_col.get(), cur.model.sort_asc.get(), cur.model.show_hidden.get());
        let path = path.unwrap_or_else(|| cur.path.clone());
        let t = st.new_tab(path.clone());
        t.model.sort_col.set(col);
        t.model.sort_asc.set(asc);
        t.model.show_hidden.set(hidden);
        let id = t.id;
        st.panes[pane].open(t);
        show_tab(st, &app, pane);
        eprintln!("[tab] pane={pane} opened tab {} of {} (id {id}) on {}", st.panes[pane].active_index() + 1, st.panes[pane].len(), path.display());
        Some(path)
    });
    if let Some(p) = path {
        navigate(pane, p);
    }
}

/// Ctrl+W, a tab's close glyph or a middle-click: close tab `i`; never the pane's last one.
fn close_tab(pane: usize, i: usize) {
    let stale = with_state(|st| {
        let app = st.app.upgrade()?;
        let was_active = st.panes[pane].active_index() == i;
        if was_active {
            save_view(st, &app, pane);
        }
        let Some(mut closed) = st.panes[pane].close(i) else {
            eprintln!("[tab] pane={pane} close tab {}: the last tab stays", i + 1);
            return None;
        };
        // Late batches and passes for it find no tab with its generation or id, and are dropped.
        closed.model.generation.set(0);
        if let Some(s) = &closed.search {
            s.cancel(fs::enumerate::SEARCH_CLOSED);
        }
        closed.drop_sizes(pane, "tab closed");
        eprintln!("[tab] pane={pane} closed tab {} (id {}), {} left", i + 1, closed.id, st.panes[pane].len());
        schedule_save(st);
        if was_active {
            show_tab(st, &app, pane);
            Some(st.tab(pane).stale)
        } else {
            update_tabs(st, pane);
            Some(false)
        }
    });
    if stale == Some(true) {
        refresh(pane);
    }
}

/// A tab dragged in its strip, or Ctrl+Shift+PageUp/PageDown: move tab `from` to `to`. The active
/// tab stays active, so only the strip changes.
fn move_tab(pane: usize, from: usize, to: usize) {
    with_state(|st| {
        if !st.panes[pane].move_tab(from, to) {
            return eprintln!("[tab] pane={pane} cannot move tab {} to {}", from + 1, to + 1);
        }
        let tabs = &st.panes[pane];
        eprintln!("[tab] pane={pane} moved tab {} to {}, tab {} of {} active", from + 1, to + 1, tabs.active_index() + 1, tabs.len());
        update_tabs(st, pane);
        schedule_save(st);
    });
}

/// Ctrl+T, Ctrl+W, Ctrl+Tab, Ctrl+Shift+Tab, Ctrl+1..9 and Ctrl+Shift+PageUp/PageDown on the
/// active pane.
fn tab_key(pane: usize, what: &str) {
    match what {
        "new" => open_tab(pane, None),
        "close" => {
            let i = with_state(|st| st.panes[pane].active_index());
            close_tab(pane, i);
        }
        "next" => switch_tab(pane, |t| t.next()),
        "prev" => switch_tab(pane, |t| t.prev()),
        "moveleft" | "moveright" => {
            let i = with_state(|st| st.panes[pane].active_index());
            let to = if what == "moveleft" { i.checked_sub(1) } else { Some(i + 1) };
            match to {
                Some(to) => move_tab(pane, i, to),
                None => eprintln!("[tab] pane={pane} tab 1 is first already"),
            }
        }
        n => {
            if let Ok(n @ 1..=9) = n.parse::<usize>() {
                switch_tab(pane, |t| {
                    t.select(n - 1);
                });
            }
        }
    }
}

/// A breadcrumb button: navigate the active tab to the path up to that component.
fn crumb_clicked(pane: usize, i: usize) {
    let target = with_state(|st| pathbar::crumbs(&st.tab(pane).path.to_string_lossy()).into_iter().nth(i).map(|c| c.target));
    if let Some(p) = target {
        eprintln!("[crumb] pane={pane} index {i}: {}", p.display());
        navigate(pane, p);
    }
}

/// The chevron after crumb `i`: list that crumb's subfolders on a worker (the pane's hidden-file
/// setting applies), then show the dropdown at window point (`x`, `y`).
fn crumb_menu(pane: usize, i: usize, x: f32, y: f32) {
    let Some((dir, hidden, seq)) = with_state(|st| {
        let dir = pathbar::crumbs(&st.tab(pane).path.to_string_lossy()).into_iter().nth(i)?.target;
        st.crumb_seq += 1;
        st.crumb_menu = None;
        Some((dir, st.model(pane).show_hidden.get(), st.crumb_seq))
    }) else {
        return;
    };
    eprintln!("[crumb] pane={pane} menu {i}: listing the folders in {}", dir.display());
    std::thread::spawn(move || {
        let t0 = Instant::now();
        let res = pathbar::subfolders(&dir, hidden, pathbar::MENU_CAP);
        let ms = perf::elapsed_ms(t0);
        let _ = slint::invoke_from_event_loop(move || crumb_menu_ready(pane, seq, dir, res, (x, y), ms));
    });
}

/// The dropdown's listing came back (UI thread): fill the popup and show it, unless a newer
/// request replaced this one.
fn crumb_menu_ready(pane: usize, seq: u64, dir: PathBuf, res: Result<(Vec<String>, usize), String>, (x, y): (f32, f32), ms: f64) {
    let Some(app) = with_state(|st| {
        let app = st.app.upgrade()?;
        if st.crumb_seq != seq {
            return None;
        }
        let (names, note) = match res {
            Ok((names, total)) if names.is_empty() => (names, if total == 0 { "No folders".to_string() } else { String::new() }),
            Ok((names, total)) if total > names.len() => {
                let note = format!("... and {} more", total - names.len());
                (names, note)
            }
            Ok((names, _)) => (names, String::new()),
            Err(e) => (Vec::new(), e),
        };
        let shown = if note.is_empty() { String::new() } else { format!(" ({note})") };
        eprintln!("[crumb] pane={pane} menu: {} folder(s) in {}{shown}, listed in {ms:.1} ms", names.len(), dir.display());
        let items: Vec<SharedString> = names.iter().map(|n| n.as_str().into()).collect();
        app.set_crumb_menu_items(ModelRc::from(Rc::new(VecModel::from(items))));
        app.set_crumb_menu_note(note.into());
        app.set_crumb_menu_x(x);
        app.set_crumb_menu_y(y);
        st.crumb_menu = Some((pane, dir, names));
        Some(app)
    }) else {
        return;
    };
    // Outside the state borrow: showing the popup moves keyboard focus.
    app.invoke_show_crumb_menu();
}

/// Item `i` of the breadcrumb dropdown: navigate its pane there.
fn crumb_pick(i: usize) {
    let Some((pane, target)) = with_state(|st| {
        let (pane, dir, names) = st.crumb_menu.take()?;
        let name = names.get(i)?;
        let target = if is_this_pc(&dir.to_string_lossy()) { PathBuf::from(name) } else { dir.join(name) };
        Some((pane, target))
    }) else {
        return;
    };
    eprintln!("[crumb] pane={pane} picked {}", target.display());
    navigate(pane, target);
    if let Some(app) = with_state(|st| st.app.upgrade()) {
        app.invoke_focus_pane(pane as i32);
    }
}

/// For the scripted driver's `crumbpick`: the dropdown's item named `name` (case-insensitive).
pub fn crumb_menu_index(name: &str) -> Option<usize> {
    with_state(|st| st.crumb_menu.as_ref()?.2.iter().position(|n| n.eq_ignore_ascii_case(name)))
}

/// The clipboard changed (`WM_CLIPBOARDUPDATE`, posted by the `devices` subclass). If it was not
/// TwinFM's own write and rows show as cut, the cut is no longer on the clipboard: clear them.
pub fn clipboard_changed() {
    with_state(|st| {
        if clipboard::taken_from_cut(clipboard::sequence(), st.clip_seq, st.cut_source.is_some()) {
            clear_cuts(st);
            eprintln!("[clipboard] another program took the clipboard: cut marks cleared");
        }
    });
}

/// A volume arrived or went (posted by the `devices` subclass): wait 500 ms for the burst to end.
pub fn volumes_changed() {
    with_state(|st| st.device_timer.start(slint::TimerMode::SingleShot, Duration::from_millis(500), devices_changed));
}

/// The drives changed: re-read the sidebar's drive list and refresh every visible tab on This PC.
fn devices_changed() {
    let panes = with_state(|st| {
        let split = st.app.upgrade().is_some_and(|a| a.get_split());
        (0..2).filter(|&p| (p == 0 || split) && is_this_pc(&st.tab(p).path.to_string_lossy())).collect::<Vec<_>>()
    });
    eprintln!("[devices] drives changed: drive list re-read, This PC refreshed in {} pane(s)", panes.len());
    refresh_drives();
    for p in panes {
        refresh(p);
    }
}

/// Ctrl+H, and the settings dialog's hidden-file boxes: show or hide hidden and system files in the
/// pane's active tab.
fn toggle_hidden(pane: usize) {
    with_state(|st| {
        let model = st.model(pane);
        let shown = !model.show_hidden.get();
        model.show_hidden.set(shown);
        eprintln!("[hidden] pane={pane} show_hidden={shown}");
        // The pane's new tabs follow its last Ctrl+H.
        if pane == 0 { st.config.show_hidden_left = shown } else { st.config.show_hidden_right = shown }
        schedule_save(st);
        let id = st.tab(pane).id;
        request_pass(st, pane, id);
    });
}

/// The pane's filter box: its text, shown while the text is not empty (a tab switch shows the new
/// tab's filter; a navigation clears it). Closing an open box in the active pane gives its list
/// the focus: the box may have held it, and a destroyed box leaves the focus nowhere (no key would
/// reach the window). `focus-pane` only bumps a counter, so this is safe inside the state borrow.
fn set_filter_box(app: &App, pane: usize, text: &str) {
    let was_open = if pane == 0 { app.get_left_filter_open() } else { app.get_right_filter_open() };
    if pane == 0 {
        app.set_left_filter_text(text.into());
        app.set_left_filter_open(!text.is_empty());
    } else {
        app.set_right_filter_text(text.into());
        app.set_right_filter_open(!text.is_empty());
    }
    if was_open && text.is_empty() && active_pane(app) == pane {
        app.invoke_focus_pane(pane as i32);
    }
}

/// An edit in the filter box: apply it to the active tab 150 ms after the last edit, so a word
/// typed is one pass, not one per keystroke.
fn filter_edited(pane: usize, text: SharedString) {
    with_state(|st| {
        let id = st.tab(pane).id;
        st.filter_timer[pane].start(slint::TimerMode::SingleShot, Duration::from_millis(150), move || apply_filter(pane, id, &text));
    });
}

/// Return or Down in the filter box: apply the text at once (an empty one closes the box). With a
/// search waiting for its pattern (Ctrl+Shift+F on an empty box), start the search instead.
fn filter_apply(pane: usize, text: SharedString) {
    let Some((app, id, armed)) = with_state(|st| {
        st.filter_timer[pane].stop();
        Some((st.app.upgrade()?, st.tab(pane).id, std::mem::take(&mut st.search_armed[pane])))
    }) else {
        return;
    };
    if armed && !text.trim().is_empty() {
        return start_search(pane, text.trim(), None);
    }
    apply_filter(pane, id, &text);
    if text.is_empty() {
        set_filter_box(&app, pane, "");
    }
}

/// Escape in the filter box or its clear glyph (the box has closed itself): no filter, at once.
fn filter_clear(pane: usize) {
    let id = with_state(|st| {
        st.filter_timer[pane].stop();
        st.search_armed[pane] = false;
        st.tab(pane).id
    });
    apply_filter(pane, id, "");
}

/// Set tab `id`'s name filter and run its pass (the hidden-file switch's pass: the snapshot, the
/// worker, cursor and selection restored by name); nothing when the filter is unchanged.
fn apply_filter(pane: usize, id: u64, text: &str) {
    with_state(|st| {
        let Some(t) = st.find(pane, id) else { return };
        let new = NameMatch::new(text);
        if *t.model.filter.borrow() == new {
            return;
        }
        if new.is_empty() {
            eprintln!("[filter] pane={pane} cleared");
        }
        *t.model.filter.borrow_mut() = new;
        request_pass(st, pane, id);
    });
}

/// Ctrl+Shift+F (and the driver's `search`): search the active tab's folder and everything under
/// it for the filter box's text. With an empty box, open it: Return there starts the search.
fn search_key(pane: usize) {
    let Some(app) = with_state(|st| st.app.upgrade()) else { return };
    let text = if pane == 0 { app.get_left_filter_text() } else { app.get_right_filter_text() };
    if text.trim().is_empty() {
        with_state(|st| st.search_armed[pane] = true);
        eprintln!("[search] pane={pane} no pattern yet: the filter box opens, Return searches");
        return app.invoke_open_filter(pane as i32);
    }
    start_search(pane, text.trim(), None);
}

/// Start a recursive name search for `pattern` under the active tab's folder: in a new tab after
/// it, or in place when the active tab is a search tab already (its old search is cancelled). The
/// search worker sends its matches through the enumeration channel with the tab's generation, so
/// they stream in as a listing's batches do.
///
/// `history_root`: a search taken from the tab's history (Back or Forward): it runs again in this
/// tab under that root, and nothing is pushed. Otherwise the search is a navigation: a new search
/// tab starts with its root folder on its back stack (it was searched from there), and a new search
/// in a search tab pushes the old one.
fn start_search(pane: usize, pattern: &str, history_root: Option<PathBuf>) {
    let started = with_state(|st| {
        let app = st.app.upgrade()?;
        st.search_armed[pane] = false;
        st.filter_timer[pane].stop();
        if is_this_pc(&st.tab(pane).path.to_string_lossy()) {
            note(st, &app, pane, "This PC lists drives; open a folder to search in it".into());
            return None;
        }
        let to = |root: &Path| Place::search(root, pattern);
        if let Some(root) = history_root {
            st.tab_mut(pane).path = root;
        } else if st.is_search(pane) {
            let t = st.tab_mut(pane);
            let from = t.place();
            if t.history.push(&from, &to(&t.path)) {
                eprintln!("[nav] pane={pane} history push {from} ({} back)", t.history.back_len());
            }
        } else {
            save_view(st, &app, pane);
            // Leaves the foreground and loses its watch (see switch_tab).
            st.tab_mut(pane).stale = true;
            let cur = st.tab(pane);
            let (col, asc, hidden, root) = (cur.model.sort_col.get(), cur.model.sort_asc.get(), cur.model.show_hidden.get(), cur.path.clone());
            let mut t = st.new_tab(root.clone());
            t.model.sort_col.set(col);
            t.model.sort_asc.set(asc);
            t.model.show_hidden.set(hidden);
            if t.history.push(&Place::folder(root.clone()), &to(&root)) {
                eprintln!("[nav] pane={pane} history push {} ({} back)", root.display(), t.history.back_len());
            }
            let id = t.id;
            st.panes[pane].open(t);
            let tabs = &st.panes[pane];
            eprintln!("[tab] pane={pane} opened tab {} of {} (id {id}) on {} for a search", tabs.active_index() + 1, tabs.len(), root.display());
        }
        let generation = st.generation();
        let matcher = NameMatch::new(pattern);
        let cancel = Arc::new(AtomicU8::new(fs::enumerate::SEARCH_RUNNING));
        let t = st.tab_mut(pane);
        if let Some(old) = t.search.take() {
            old.cancel(fs::enumerate::SEARCH_REPLACED);
        }
        t.search = Some(Search { pattern: matcher.clone(), cancel: cancel.clone() });
        *t.model.filter.borrow_mut() = NameMatch::default();
        t.model.generation.set(generation);
        t.model.clear();
        t.timing = PaneTiming { requested: Some(Instant::now()), ..Default::default() };
        t.keep_scroll = false;
        t.note.clear();
        t.focus_name = None;
        t.fallback_cursor = None;
        t.free = None;
        t.stale = false;
        t.refresh_pending = false;
        t.watch_refresh = false;
        t.cursor = -1;
        t.scroll = 0.0;
        let (id, root, hidden) = (t.id, t.path.clone(), t.model.show_hidden.get());
        if st.cut_source == Some((pane, id)) {
            st.cut_source = None;
        }
        st.icon_gens[pane].store(generation, Ordering::Relaxed);
        show_tab(st, &app, pane);
        schedule_save(st);
        eprintln!("[search] pane={pane} gen={generation} start \"{pattern}\" in {}", root.display());
        fs::enumerate::spawn_search(root, matcher, hidden, cancel, pane, generation, st.tx.clone(), wake);
        Some(app)
    });
    // The search started from the filter box (Ctrl+Shift+F there, or Return with a search waiting)
    // or from the list: either way the keys go to the results list now. Outside the state borrow:
    // moving the focus runs the pane's handlers.
    if let Some(app) = started {
        app.invoke_focus_pane(pane as i32);
    }
}

/// For the scripted driver's `waitsearch`: a search tab is still walking.
pub fn search_running() -> bool {
    STATE.with(|s| match s.try_borrow() {
        Ok(g) => g.as_ref().is_some_and(|st| {
            st.panes.iter().any(|p| p.iter().any(|t| t.search.is_some() && t.timing.done_ms.is_none() && t.timing.error.is_none()))
        }),
        Err(_) => true,
    })
}

/// The Type column on or off in both panes. Off, every tab sorted by Type is sorted by name
/// instead (a header click can no longer reach Type).
fn set_show_type(st: &mut State, app: &App, on: bool) {
    app.set_show_type(on);
    st.config.show_type = on;
    eprintln!("[settings] Type column {}", if on { "shown" } else { "removed from the header and the rows" });
    if on {
        return;
    }
    if settings::sort_from_name(&st.config.sort_column) == SortCol::Type {
        st.config.sort_column = settings::sort_name(SortCol::Name).into();
    }
    let mut resort: Vec<(usize, u64)> = Vec::new();
    for (pane, tabs) in st.panes.iter().enumerate() {
        for t in tabs.iter().filter(|t| t.model.sort_col.get() == SortCol::Type) {
            t.model.sort_col.set(SortCol::Name);
            resort.push((pane, t.id));
        }
    }
    for (pane, id) in resort {
        eprintln!("[settings] pane={pane} tab {id} was sorted by Type: sorted by name instead");
        request_pass(st, pane, id);
    }
}

/// Ctrl+, or "Settings": copy the live values into the dialog and show it.
fn settings_open() {
    let Some((app, d, note)) = with_state(|st| {
        let app = st.app.upgrade()?;
        if st.dialog.is_some() {
            return None;
        }
        let d = settings::Dialog {
            start_left: st.config.start_left.clone(),
            start_right: st.config.start_right.clone(),
            show_hidden_left: st.model(0).show_hidden.get(),
            show_hidden_right: st.model(1).show_hidden.get(),
            show_type: st.config.show_type,
            sidebar_visible: app.get_sidebar_visible(),
            path_bar: if app.get_path_entry_sticky() { "entry" } else { "breadcrumb" }.into(),
        };
        st.dialog = Some(d.clone());
        let note = match &st.config_path {
            Some(p) => format!("Saved to {}", p.display()),
            None => "Not saved in a scripted run".to_string(),
        };
        Some((app, d, note))
    }) else {
        return;
    };
    app.set_dlg_start_left(d.start_left.into());
    app.set_dlg_start_right(d.start_right.into());
    app.set_dlg_hidden_left(d.show_hidden_left);
    app.set_dlg_hidden_right(d.show_hidden_right);
    app.set_dlg_type_column(d.show_type);
    app.set_dlg_sidebar(d.sidebar_visible);
    app.set_dlg_crumbs(d.path_bar != "entry");
    app.set_settings_note(note.into());
    eprintln!("[settings] dialog opened");
    // Outside the state borrow: showing the popup moves keyboard focus.
    app.invoke_show_settings();
}

/// The dialog closed: OK applies every field whose staged value differs from the value it opened
/// with, through the path the rest of the app uses for it (Ctrl+H, F9, the path bar's glyph);
/// Cancel and Escape drop them.
fn settings_closed(ok: bool) {
    let Some((app, opened)) = with_state(|st| Some((st.app.upgrade()?, st.dialog.take()?))) else { return };
    let staged = settings::Dialog {
        start_left: app.get_dlg_start_left().to_string(),
        start_right: app.get_dlg_start_right().to_string(),
        show_hidden_left: app.get_dlg_hidden_left(),
        show_hidden_right: app.get_dlg_hidden_right(),
        show_type: app.get_dlg_type_column(),
        sidebar_visible: app.get_dlg_sidebar(),
        path_bar: if app.get_dlg_crumbs() { "breadcrumb" } else { "entry" }.into(),
    }
    .normalized();
    let changes = opened.changes(&staged);
    if !ok {
        return eprintln!("[settings] dialog closed (cancel), {} change(s) discarded", changes.len());
    }
    for (key, old, new) in &changes {
        eprintln!("[settings] dialog: {key} {old} -> {new}");
        match *key {
            "start_left" => with_state(|st| st.config.start_left = staged.start_left.clone()),
            "start_right" => with_state(|st| st.config.start_right = staged.start_right.clone()),
            "show_hidden_left" | "show_hidden_right" => {
                let pane = usize::from(*key == "show_hidden_right");
                let want = if pane == 0 { staged.show_hidden_left } else { staged.show_hidden_right };
                if with_state(|st| st.model(pane).show_hidden.get()) != want {
                    toggle_hidden(pane);
                }
            }
            "show_type" => with_state(|st| set_show_type(st, &app, staged.show_type)),
            "sidebar_visible" => {
                if app.get_sidebar_visible() != staged.sidebar_visible {
                    app.invoke_toggle_sidebar();
                }
            }
            "path_bar" => app.set_path_entry_sticky(staged.path_bar == "entry"),
            _ => {}
        }
    }
    if !changes.is_empty() {
        with_state(|st| schedule_save(st));
    }
    eprintln!("[settings] dialog closed (ok), {} change(s)", changes.len());
}

/// For the scripted driver's `waitsize`: a folder size is still queued or walking.
pub fn sizes_busy() -> bool {
    STATE.with(|s| match s.try_borrow() {
        Ok(g) => g.as_ref().is_some_and(|st| st.panes.iter().any(|p| p.iter().any(|t| !t.sizes.pending.is_empty()))),
        Err(_) => true,
    })
}

/// For the scripted driver: watch-driven refreshes completed so far.
pub fn watch_refreshes() -> u64 {
    STATE.with(|s| s.try_borrow().ok().and_then(|g| g.as_ref().map(|st| st.watch_refreshes)).unwrap_or(0))
}

/// For the scripted driver's `touch` and `rmfile`: create (or update the time of) an empty file
/// in the pane's folder, or delete one. Under TWINFM_SOAK=1 the path must lie in the soak fence.
pub fn script_file(pane: usize, name: &str, remove: bool) {
    let Some((folder, soak)) = with_state(|st| Some((st.panes.get(pane)?.active().path.clone(), st.soak.clone()))) else {
        return eprintln!("[auto] no pane {pane}");
    };
    if is_this_pc(&folder.to_string_lossy()) {
        return eprintln!("[auto] pane {pane} lists drives; no file there");
    }
    let path = folder.join(name);
    if let Some(roots) = soak {
        if !roots.iter().any(|r| inside(&path, r)) {
            return eprintln!("[auto] soak: refused {} (outside the soak folders)", path.display());
        }
    }
    let res = if remove {
        std::fs::remove_file(&path)
    } else {
        std::fs::OpenOptions::new().create(true).append(true).open(&path).and_then(|f| f.set_modified(std::time::SystemTime::now()))
    };
    eprintln!("[auto] {} {}: {res:?}", if remove { "rmfile" } else { "touch" }, path.display());
}

fn main() -> Result<(), slint::PlatformError> {
    let app = App::new()?;
    let icons = Rc::new(RefCell::new(IconTable::default()));
    let (tx, rx) = unbounded();
    let (icon_tx, icon_requests) = unbounded();
    let (icon_replies, icon_rx) = unbounded();
    let icon_gens = Arc::new([AtomicU64::new(0), AtomicU64::new(0)]);
    let icon_stats = Arc::new(icons::Stats::default());
    icons::spawn(icon_requests, icon_replies, icon_gens.clone(), icon_stats.clone(), || {
        let _ = slint::invoke_from_event_loop(drain_icons);
    });
    let (watch_tx, watch_rx) = unbounded();
    let watch = fs::watch::Watcher::spawn(watch_tx, wake);
    let (size_tx, size_rx) = unbounded();
    let sizer = fs::sizes::Sizer::spawn(size_tx, || {
        let _ = slint::invoke_from_event_loop(drain_sizes);
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
    let tab_models = [Rc::new(VecModel::default()), Rc::new(VecModel::default())];
    let crumb_models = [Rc::new(VecModel::default()), Rc::new(VecModel::default())];
    app.set_left_tabs(ModelRc::from(tab_models[0].clone()));
    app.set_right_tabs(ModelRc::from(tab_models[1].clone()));
    app.set_left_crumbs(ModelRc::from(crumb_models[0].clone()));
    app.set_right_crumbs(ModelRc::from(crumb_models[1].clone()));
    // Settings before the first navigation: they decide the start folders, sort and hidden files,
    // the split, the sidebar, the path bar and the window's placement.
    let config_path = settings::config_path();
    let mut config = match &config_path {
        Some(p) => {
            let c = settings::Config::load(p);
            eprintln!("[settings] file {} ({})", p.display(), if p.exists() { "read" } else { "not there yet: defaults" });
            c
        }
        None => {
            eprintln!("[settings] scripted run: defaults, nothing saved (set TWINFM_CONFIG to use a file)");
            settings::Config::default()
        }
    };
    // With the Type column off, a header click cannot reach a Type sort, so none is restored.
    if !config.show_type && settings::sort_from_name(&config.sort_column) == SortCol::Type {
        eprintln!("[settings] the Type column is off: sorting by name instead of the saved type sort");
        config.sort_column = settings::sort_name(SortCol::Name).into();
    }
    // Each pane's tabs: the saved list, or one tab on its start folder. A command-line folder
    // replaces the left pane's tabs; the right pane's are restored only with the split (otherwise
    // F3 opens it on the left pane's folder, as before). Only the active tab navigates below;
    // the others are stale and list their folder when first shown.
    let arg = std::env::args().nth(1);
    let (left_paths, left_active) = match &arg {
        Some(a) => (vec![a.clone()], 0),
        None => settings::start_tabs(&config.left_tabs, config.left_active, &config.start_left),
    };
    let right_start = if config.start_right.is_empty() { left_paths[left_active].clone() } else { config.start_right.clone() };
    let (right_paths, right_active) = if config.split {
        settings::start_tabs(&config.right_tabs, config.right_active, &right_start)
    } else {
        (vec![String::new()], 0)
    };
    let mut next_tab = 0;
    let mut restore = |paths: &[String], active: usize, hidden: bool| {
        let mut tabs: Vec<Tab> = paths
            .iter()
            .enumerate()
            .map(|(i, p)| {
                next_tab += 1;
                let t = Tab { path: PathBuf::from(p), stale: i != active, ..blank_tab(next_tab, &icons) };
                t.model.sort_col.set(settings::sort_from_name(&config.sort_column));
                t.model.sort_asc.set(config.sort_ascending);
                t.model.show_hidden.set(hidden);
                t
            })
            .collect();
        let first = tabs.remove(0);
        Tabs::restored(first, tabs, active)
    };
    let panes = [
        restore(&left_paths, left_active, config.show_hidden_left),
        restore(&right_paths, right_active, config.show_hidden_right),
    ];
    for (pane, tabs) in panes.iter().enumerate() {
        if tabs.len() > 1 {
            eprintln!("[tab] pane={pane} restored {} tabs, tab {} active", tabs.len(), tabs.active_index() + 1);
        }
    }
    app.set_left_rows(ModelRc::from(panes[0].active().model.clone()));
    app.set_right_rows(ModelRc::from(panes[1].active().model.clone()));
    app.set_sidebar_visible(config.sidebar_visible);
    app.set_sidebar_width(config.sidebar_width);
    app.set_split_ratio(config.split_ratio);
    app.set_path_entry_sticky(config.path_bar == "entry");
    app.set_show_type(config.show_type);
    app.set_version(env!("CARGO_PKG_VERSION").into());
    if let Some(g) = config.window {
        app.window().set_size(slint::LogicalSize::new(g.width, g.height));
        if settings::on_screen(g.x, g.y, g.width as i32, &settings::monitors()) {
            app.window().set_position(slint::PhysicalPosition::new(g.x, g.y));
        } else {
            eprintln!("[settings] saved window position ({}, {}) is off screen; left to Windows", g.x, g.y);
        }
    }
    let side_model = Rc::new(VecModel::default());
    app.set_side_items(ModelRc::from(side_model.clone()));
    STATE.with(|s| {
        *s.borrow_mut() = Some(State {
            app: app.as_weak(),
            panes,
            next_tab,
            next_gen: 0,
            rx,
            tx,
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
            cut_source: None,
            clip_seq: 0,
            undo: Vec::new(),
            paste_move: None,
            soak,
            strip: [None; 2],
            strip_dirty: [false; 2],
            gesture: Default::default(),
            dnd_timer: slint::Timer::default(),
            tab_models,
            crumb_models,
            perf_status: std::env::var("TWINFM_PERF_STATUS").as_deref() == Ok("1"),
            config: config.clone(),
            config_path,
            save_timer: slint::Timer::default(),
            drives: Vec::new(),
            places: sidebar::known_folders(),
            side: Vec::new(),
            side_model,
            watch,
            watch_rx,
            watch_refreshes: 0,
            device_timer: slint::Timer::default(),
            crumb_seq: 0,
            crumb_menu: None,
            dialog: None,
            filter_timer: Default::default(),
            search_armed: [false; 2],
            no_watch_logged: [None; 2],
            sizer,
            size_rx,
            size_timer: Default::default(),
        })
    });
    with_state(rebuild_sidebar);
    if config.sidebar_visible {
        refresh_drives();
    }

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
        go_up(pane as usize);
    });
    app.on_pane_back(|pane| history_button(pane as usize, false));
    app.on_pane_forward(|pane| history_button(pane as usize, true));
    app.on_pane_history(|pane, forward| history_move(pane as usize, forward));
    app.on_sort_by(|pane, col| {
        with_state(|st| {
            let model = st.model(pane as usize);
            let col = SortCol::from_i32(col);
            let asc = if model.sort_col.get() == col { !model.sort_asc.get() } else { true };
            model.sort_col.set(col);
            model.sort_asc.set(asc);
            // The last sort chosen is what new tabs start with.
            st.config.sort_column = settings::sort_name(col).into();
            st.config.sort_ascending = asc;
            schedule_save(st);
            let id = st.tab(pane as usize).id;
            request_pass(st, pane as usize, id);
        });
    });
    app.on_select(|pane, op, row| {
        let op = SelOp::from_i32(op);
        if row < 0 && op != SelOp::All && op != SelOp::Clear {
            return;
        }
        with_state(|st| {
            let model = st.model(pane as usize);
            model.select(op, row.max(0) as usize);
            eprintln!("[select] pane={pane} {op:?} row={row} selected={}", model.selected_count());
            size_selection(st, pane as usize);
            if let Some(app) = st.app.upgrade() {
                update_status(st, &app, pane as usize);
            }
        });
    });
    app.on_refresh(|pane| {
        // F5 sizes folders afresh; a refresh by the watch or after an operation keeps the sizes.
        with_state(|st| st.tab_mut(pane as usize).drop_sizes(pane as usize, "F5"));
        refresh(pane as usize);
        if with_state(|st| st.app.upgrade().is_some_and(|a| a.get_sidebar_visible())) {
            refresh_drives();
        }
    });
    app.on_side_clicked(|i, middle| {
        if let Ok(i) = usize::try_from(i) {
            side_clicked(i, middle);
        }
    });
    app.on_bookmark_add(bookmark_add);
    app.on_bookmark_remove(|i| {
        if let Ok(i) = usize::try_from(i) {
            bookmark_remove(i);
        }
    });
    app.on_bookmark_rename(|i, title| {
        if let Ok(i) = usize::try_from(i) {
            bookmark_rename(i, title.as_str());
        }
    });
    app.on_bookmark_move(|i, up| {
        if let Ok(i) = usize::try_from(i) {
            bookmark_move(i, up);
        }
    });
    app.on_sidebar_toggled(|| {
        let shown = with_state(|st| {
            schedule_save(st);
            st.app.upgrade().is_some_and(|a| a.get_sidebar_visible())
        });
        eprintln!("[sidebar] {}", if shown { "shown" } else { "hidden" });
        if shown {
            refresh_drives();
        }
    });
    app.on_settings_changed(|| with_state(|st| schedule_save(st)));
    // Closing the window writes the settings at once (the window's placement included).
    app.window().on_close_requested(|| {
        save_now();
        slint::CloseRequestResponse::HideWindow
    });
    app.on_context_menu(|pane, row, x, y, extended| context_menu(pane as usize, row, x, y, extended));
    app.on_context_key(|pane, row, lx, ly, cy, vh| context_key(pane as usize, row, lx, ly, cy, vh));
    app.on_command(|pane, cmd| command(pane as usize, cmd.as_str()));
    app.on_edit_commit(|pane, mode, original, typed| edit_commit(pane as usize, mode, original.as_str(), typed.as_str()));
    app.on_tab_select(|pane, i| {
        if let Ok(i) = usize::try_from(i) {
            switch_tab(pane as usize, |t| {
                t.select(i);
            });
        }
    });
    app.on_tab_close(|pane, i| {
        if let Ok(i) = usize::try_from(i) {
            close_tab(pane as usize, i);
        }
    });
    app.on_tab_move(|pane, from, to| {
        if let (Ok(from), Ok(to)) = (usize::try_from(from), usize::try_from(to)) {
            move_tab(pane as usize, from, to);
        }
    });
    app.on_tab_key(|pane, what| tab_key(pane as usize, what.as_str()));
    app.on_crumb_clicked(|pane, i| {
        if let Ok(i) = usize::try_from(i) {
            crumb_clicked(pane as usize, i);
        }
    });
    app.on_crumb_menu(|pane, i, x, y| {
        if let Ok(i) = usize::try_from(i) {
            crumb_menu(pane as usize, i, x, y);
        }
    });
    app.on_crumb_pick(|i| {
        if let Ok(i) = usize::try_from(i) {
            crumb_pick(i);
        }
    });
    app.on_toggle_hidden(|pane| toggle_hidden(pane as usize));
    app.on_filter_edited(|pane, text| filter_edited(pane as usize, text));
    app.on_filter_apply(|pane, text| filter_apply(pane as usize, text));
    app.on_filter_clear(|pane| filter_clear(pane as usize));
    app.on_search(|pane| search_key(pane as usize));
    app.on_settings_requested(settings_open);
    app.on_settings_closed(settings_closed);
    app.on_toggle_split({
        let w = app.as_weak();
        move || {
            let Some(app) = w.upgrade() else { return };
            let now = !app.get_split();
            app.set_split(now);
            with_state(|st| schedule_save(st));
            if now {
                with_state(|st| st.split_requested = Some(perf::mark("F3: split requested")));
                let busy = perf::BusyWindow::start("10 s after split");
                with_state(|st| {
                    st.busy_timer.start(slint::TimerMode::SingleShot, std::time::Duration::from_secs(10), move || {
                        busy.finish();
                    })
                });
                let left = with_state(|st| st.tab(0).path.clone());
                navigate(1, left);
                app.invoke_focus_pane(1);
            } else {
                with_state(|st| st.watch.unwatch(1));
                app.invoke_focus_pane(0);
            }
        }
    });

    // Each pane's active tab lists its folder; the restored background tabs wait until shown.
    navigate(0, PathBuf::from(&left_paths[left_active]));
    if config.split {
        app.set_split(true);
        navigate(1, PathBuf::from(&right_paths[right_active]));
    }
    // Start with keyboard focus in the list, so arrows, Space and Ctrl+A work before a click.
    app.invoke_focus_pane(0);
    auto::start(&app);
    // The drop target goes on the window once it exists (winit creates it lazily, in `run`).
    with_state(|st| st.dnd_timer.start(slint::TimerMode::Repeated, Duration::from_millis(50), register_dnd));

    let res = app.run();
    // A change still waiting for its debounce (a scripted quit, say) is written now.
    if with_state(|st| st.save_timer.running()) {
        save_now();
    }
    // Stop the watcher thread now: at process exit Windows terminates it before the state's
    // thread-local destructor runs.
    with_state(|st| st.watch.stop());
    res
}
