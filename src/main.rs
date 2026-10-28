// twinfm: Slint bootstrap, wires model <-> UI, owns the two panes and their tabs.
// A windowed program (0.1.0): no console window; stderr's lines reach a file only when the
// launcher redirects it, as the gate and soak scripts do.
#![windows_subsystem = "windows"]
slint::include_modules!();

mod fs {
    pub mod archive;
    pub mod archivefolder;
    pub mod archivewrite;
    pub mod content;
    pub mod dirinfo;
    pub mod enumerate;
    pub mod sizes;
    pub mod watch;
    pub mod zipraw;
}
mod ask;
mod auto;
mod band;
mod cloud;
mod cols;
mod devices;
mod dnd;
mod explorercmd;
mod fileops;
mod html;
mod icons;
mod logfile;
mod markdown;
mod model;
mod office;
mod pathbar;
mod perf;
mod preview;
mod searchbox;
mod settings;
mod sizeview;
mod shellmenu;
mod sidebar;
mod statusbar;
mod syntax;
mod tabs;
mod toolbar;
mod verbs;
mod wic;
mod winmenu;

use crossbeam_channel::{Receiver, Sender, unbounded};
use fileops::{Job, Outcome, clipboard, same_dir};
use fs::enumerate::{Msg, THIS_PC, is_this_pc};
use icons::IconTable;
use model::{GridModel, Kind, NameMatch, PaneModel, SelOp, SortCol, filter_sort};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::{ComponentHandle, Image, Model, ModelRc, RenderingState, SharedString, VecModel, Weak};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tabs::{Place, Tabs};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_LBUTTON, VK_RBUTTON, VK_SHIFT};
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
    /// (requests, replies, worker renders, worker ns, composes, overlays, slow renders,
    /// thumbnails, thumbnail ns, thumbnails dropped, cloud lookups) at the last report.
    reported: (u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64),
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
    /// A restored tab's saved cursor row (by name) and scroll (P6-M3): its first pass after a
    /// complete listing puts the cursor there, and the scroll is applied when that pass lands.
    restore_cursor: Option<SharedString>,
    restore_scroll: Option<f32>,
    /// What happened to its folder is unknown (it is in the background without a watch, its
    /// watch fell back or was lost, or a file operation finished while it was unwatched): it
    /// refreshes when it comes to the front.
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
    /// A sizes view tab (P13-M3, "Show sizes"): its rows are the walk's folders and the opened
    /// levels' files under `path`, its root. None for every other tab.
    sizeview: Option<SizeTab>,
    /// Folder sizes computed for this tab (selected folders, Alt+Shift+Return), and those in flight.
    sizes: fs::sizes::TabSizes,
    /// The UI asked the watcher to watch this tab's folder and has not heard `Refused` or `Lost`
    /// since (`State::watch_tabs`); `watched_bg`: that watch is a background one (shallow, no
    /// parent), so "watched in the background" is logged once per stretch in the background.
    watched: bool,
    watched_bg: bool,
    /// When the tab last came to the front (`show_tab`): background tabs are watched most
    /// recently shown first, up to the watcher's cap.
    shown_at: Instant,
    /// The lower-cased names a finished file operation created, changed or removed in this
    /// folder, and when (`job_done`): a watch window naming only these within `EXPLAINED_FOR` is
    /// the operation's own, already covered by its refresh.
    explained: Option<(HashSet<String>, Instant)>,
    /// Names (lower-cased) a watch window added or renamed to that were gone again when the edit
    /// worker statted them, so the listing never had them, with the listing's generation: a later
    /// record removing or renaming one is no unknown name (`window_refusal`, `land_edits`).
    vanished: (u64, HashSet<String>),
}

/// What a search tab searches for, and its worker's cancel flag (`fs::enumerate::SEARCH_*`).
/// `content` (P11-M1): the pattern is text searched for inside the files, not a name.
struct Search {
    pattern: NameMatch,
    content: bool,
    cancel: Arc<AtomicU8>,
    /// Its rows on screen are being statted again (`restat_results`, P11-M2).
    restating: bool,
}

/// A sizes view (`sizeview`): its walk's tree, the folders opened in place, the levels' files once
/// listed, the rows in their order and what each draws besides its entry.
#[derive(Default)]
struct SizeTab {
    /// Its walk's (unique, from `State::generation`; 0 before the first walk).
    generation: u64,
    tree: Arc<std::sync::Mutex<sizeview::Tree>>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    open: HashSet<sizeview::Id>,
    files: HashMap<sizeview::Id, Vec<sizeview::File>>,
    /// Levels whose listing is out.
    listing: HashSet<sizeview::Id>,
    rows: Vec<sizeview::Row>,
    extra: Rc<RefCell<Vec<sizeview::Extra>>>,
    /// The walk's time once it is done.
    done_ms: Option<u64>,
    /// The working set when the walk started, MB (the done line says how much the walk held).
    ws_before: f64,
}

impl Tab {
    /// Where the tab is, as its history records it: its folder, or its search and root.
    fn place(&self) -> Place {
        Place {
            path: self.path.clone(),
            search: self.search.as_ref().map(|s| s.pattern.pattern.clone()),
            content: self.search.as_ref().is_some_and(|s| s.content),
        }
    }

    /// Folder sizes are computed in folder tabs only (not for search results, not on This PC, not
    /// inside an archive).
    fn sizable(&self) -> bool {
        self.search.is_none() && self.sizeview.is_none() && !is_this_pc(&self.path.to_string_lossy()) && !fs::archive::inside(&self.path)
    }

    /// Search results or a sizes view (P13-M3): rows from many folders under `path`, never watched,
    /// no drop or new item into the root.
    fn results(&self) -> bool {
        self.search.is_some() || self.sizeview.is_some()
    }

    /// The tab leaves the foreground (a tab switch, a new tab): its finished folder sizes stay,
    /// watched below the folder in the background (`watch_tabs`); walks in flight and sizes waiting
    /// for a re-walk go (P6-M3).
    fn background_sizes(&mut self, pane: usize) {
        if !self.sizable() {
            return;
        }
        let (cancelled, stale) = self.sizes.to_background();
        for n in &cancelled {
            eprintln!("[size] cancelled {} (tab in the background)", self.path.join(n).display());
        }
        for n in &stale {
            self.model.set_size(n, None);
            eprintln!("[size] stale {}: tab in the background, size dropped", self.path.join(n).display());
        }
        if !self.sizes.cache.is_empty() {
            eprintln!("[size] pane={pane} tab={} keeps {} size(s) in the background", self.id, self.sizes.cache.len());
        }
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
    /// An undo or redo job, and the record it replays (`after_job` says which stack its
    /// completion goes to; a replay that did nothing puts the record back where it came from).
    replay: Option<(Replay, fileops::undo::Record)>,
    /// The note `submit` showed for it: its percentage shows only beside that note.
    running: String,
}

/// Which stack a replayed record came from: Ctrl+Z's or Ctrl+Y's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Replay {
    Undo,
    Redo,
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
    /// With thumbnails on: each pane's rows on screen at its last resolve (numbered by
    /// `resolve_seq`), read by the icon workers to drop thumbnails for rows scrolled past.
    icon_views: Arc<icons::Views>,
    resolve_seq: [u64; 2],
    icon_stats: Arc<icons::Stats>,
    /// The sync roots and Google Drive volumes: which folders carry cloud marks (P6-M1).
    cloud: cloud::Locations,
    icon_counts: IconCounts,
    /// The (content-y, visible-height) each list last reported, in logical px.
    view: [(f32, f32); 2],
    /// The file operation worker (STA thread, owns every IFileOperation).
    ops: fileops::Worker,
    /// The verb worker (STA thread): shell commands picked in the context menu run there (P7-M1).
    verbs: verbs::Worker,
    next_verb: u64,
    /// The Windows 11 style context menu while it is open (P7-M2), and the last menu's number.
    winmenu: Option<WinMenu>,
    winmenu_seq: u64,
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
    /// The clipboard holds files (`CF_HDROP`): what the toolbar's Paste needs. Read at start and on
    /// every `WM_CLIPBOARDUPDATE` (P6-M3).
    clip_files: bool,
    /// Ctrl+Z's record, newest last, shared by both panes (`fileops::undo`).
    undo: Vec<fileops::undo::Record>,
    /// Ctrl+Y's record, fed by completed undo jobs and cleared by the next new job.
    redo: Vec<fileops::undo::Record>,
    /// The running job's last reported percentage (job id, percent), for its tab's status bar.
    progress: Option<(u64, u32)>,
    /// The paste-of-a-cut job, if one is in flight: on success the clipboard is emptied.
    paste_move: Option<u64>,
    /// TWINFM_SOAK=1: no confirmations, and every path must lie inside one of these scratch
    /// folders (`%TEMP%\twinfm-m3` for the M3 soak, `%TEMP%\twinfm-m5` for the M5 one).
    soak: Option<Vec<PathBuf>>,
    /// What each pane's icon strip was last composed from (None: hidden), and whether its
    /// rows changed since, so the next resolve recomposes it.
    strip: [Option<icons::Composed>; 2],
    strip_dirty: [bool; 2],
    /// Each pane shows the large-icon grid (settings `view_left` / `view_right`, Ctrl+Shift+2), and
    /// the tiles to a grid row its list fits (from the list's width, `grid-cols`). With the grid
    /// on, `strip` holds grid rows and the strip is `compose_grid_strip`'s.
    grid: [bool; 2],
    grid_n: [usize; 2],
    /// Each pane's tiles in the grid (settings `tile_left` / `tile_right`, Ctrl+Shift+3), kept
    /// while it shows the details list.
    tile: [icons::TileGeom; 2],
    /// The grid's icon strip per pane: a ring of slots, one image per grid row, in the pane's
    /// `strip-rows` model (slot k holds grid row g with g % slots == k; the ring holds the
    /// visible grid rows plus the margin, so no two covered rows share a slot). `grid_slots[k]`
    /// says which grid row slot k holds, at which icon size, and whether an icon in it changed.
    grid_slots: [Vec<Option<GridSlot>>; 2],
    strip_rows: [Rc<VecModel<StripRow>>; 2],
    /// Each pane's left-button gesture on a row (click, drag, double-click), dnd::Gesture.
    gesture: [dnd::Gesture; 2],
    /// The drag image's icon asked of the icon worker at the last press on a row
    /// (`prefetch_drag_icon`): its path, and its pixels once the reply landed. The newest press
    /// wins; a drag takes it (`run_pending_drag`).
    drag_icon: Option<(PathBuf, Option<(slint::SharedPixelBuffer<slint::Rgba8Pixel>, bool)>)>,
    /// The rubber band under way (P4-M3, `band.rs`), its last window point, the timer that
    /// scrolls the list while the pointer sits at its edge, and when `[band]` last logged a count.
    band: Option<band::Band>,
    band_at: (f32, f32),
    band_timer: slint::Timer,
    band_logged: Option<Instant>,
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
    /// The directory watcher (one thread; each pane's active tab and, up to its cap, background
    /// tabs: `watch_tabs`) and its events, drained with the enumeration messages.
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
    /// The search tab whose "no watch" line each pane logged last (logged once per tab).
    no_watch_logged: [Option<u64>; 2],
    /// Watch windows applied as row edits (the driver's `waitedit` and `waitwatch` wait on it),
    /// and the worker that stats their names (`edit_worker`).
    watch_edits: u64,
    stat_tx: Sender<StatJob>,
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
        restore_cursor: None,
        restore_scroll: None,
        stale: false,
        refresh_pending: false,
        watch_refresh: false,
        history: tabs::History::default(),
        search: None,
        sizeview: None,
        sizes: fs::sizes::TabSizes::default(),
        watched: false,
        watched_bg: false,
        shown_at: Instant::now(),
        explained: None,
        vanished: (0, HashSet::new()),
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

    /// The watch set changed (a navigation, a tab switch, a listing done, sizes): `watch_tabs`.
    fn watch_active(&mut self, app: &App, _pane: usize) {
        self.watch_tabs(app);
    }

    /// Decide which tabs the watcher watches, and ask it. Every active tab (both panes with the
    /// split; the hidden second pane's tabs none) with a parent watch (and a subtree watch while
    /// folder sizes are cached or in flight); then background folder tabs whose listing is done,
    /// not stale (unless they keep folder sizes, P6-M3: their watch keeps the sizes current while
    /// the listing waits to be refreshed on show), most recently shown first, shallow (a subtree
    /// watch with sizes) and without a parent, up to
    /// `fs::watch::MAX_WATCHES`. Every other tab is unwatched; a background one goes stale (a
    /// search tab never: going stale would run its search again). Unwatches go first, so a slot
    /// freed here is free for the watches that follow. A no-op for a tab already watched the same
    /// way (the watcher compares).
    fn watch_tabs(&mut self, app: &App) {
        let split = app.get_split();
        let mut cands: Vec<WatchCand> = Vec::new();
        for (pane, tabs) in self.panes.iter().enumerate() {
            if pane == 1 && !split {
                continue;
            }
            let active = tabs.active_index();
            for (i, t) in tabs.iter().enumerate() {
                let listed = t.timing.error.is_none();
                let eligible = if i == active {
                    !t.results() && listed
                } else {
                    folder_tab(t) && t.timing.done_ms.is_some() && listed && (!t.stale || t.sizes.any())
                };
                cands.push(WatchCand { id: t.id, active: i == active, eligible, shown_at: t.shown_at });
            }
        }
        let set = watch_set(&cands, fs::watch::MAX_WATCHES);
        let mut asks: Vec<(usize, u64, PathBuf, bool, bool)> = Vec::new();
        for (pane, tabs) in self.panes.iter_mut().enumerate() {
            let active = tabs.active_index();
            for (i, t) in tabs.iter_mut().enumerate() {
                if set.contains(&t.id) {
                    continue;
                }
                // Always: a `Lost` drained after a re-watch clears `watched` while the watcher holds
                // the new slot (an unwatch of a tab it does not watch is a no-op).
                self.watch.unwatch(t.id);
                t.watched = false;
                t.watched_bg = false;
                if i == active {
                    if t.results() && (pane == 0 || split) && self.no_watch_logged[pane] != Some(t.id) {
                        // A search's results come from many folders; F5 runs it again. So does a
                        // sizes view's walk (P13-M3).
                        let what = if t.search.is_some() { "search results" } else { "sizes view" };
                        eprintln!("[watch] pane={pane} no watch for {}: {what}", t.path.display());
                        self.no_watch_logged[pane] = Some(t.id);
                    }
                    continue;
                }
                if t.results() {
                    continue;
                }
                if t.stale {
                    // Stale with sizes and past the cap: unwatched, the sizes could go out of date.
                    if t.sizes.any() {
                        t.drop_sizes(pane, "not watched in the background");
                    }
                    continue;
                }
                if cands.iter().any(|c| c.id == t.id && c.eligible) {
                    eprintln!("[watch] pane={pane} tab={} past the cap: stale when shown", t.id);
                }
                t.stale = true;
                // Unwatched, its sizes could go out of date unseen.
                t.drop_sizes(pane, "not watched in the background");
            }
        }
        for &id in &set {
            for (pane, tabs) in self.panes.iter_mut().enumerate() {
                let active = tabs.active().id == id;
                let Some(t) = tabs.iter_mut().find(|t| t.id == id) else { continue };
                if active {
                    // With folder sizes cached or in flight, every folder below is watched too
                    // (until two overflows in a row), so a change inside a sized folder drops its
                    // size.
                    let subtree = t.sizes.any() && t.sizes.overflows < 2;
                    asks.push((pane, id, t.path.clone(), subtree, true));
                    t.watched_bg = false;
                } else {
                    // A subtree watch while the tab keeps folder sizes (P6-M3), shallow otherwise.
                    let subtree = t.sizes.any() && t.sizes.overflows < 2;
                    asks.push((pane, id, t.path.clone(), subtree, false));
                    if !t.watched_bg {
                        eprintln!("[watch] pane={pane} tab={id} watched in the background");
                        t.watched_bg = true;
                    }
                }
                t.watched = true;
            }
        }
        for (pane, id, path, subtree, parent) in asks {
            self.watch.watch(pane, id, path, subtree, parent);
        }
    }

    /// The pane's active tab is a search tab.
    fn is_search(&self, pane: usize) -> bool {
        self.tab(pane).search.is_some()
    }

    /// The active tab shows search results or a sizes view (`Tab::results`).
    fn is_results(&self, pane: usize) -> bool {
        self.tab(pane).results()
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
        // A restored tab's saved cursor and scroll belong to its own folder only (P6-M3).
        if t.path != path {
            t.restore_cursor = None;
            t.restore_scroll = None;
        }
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
        t.explained = None;
        // A search tab becomes a folder tab again (its root goes on the back stack, the search
        // does not).
        if let Some(s) = t.search.take() {
            s.cancel(fs::enumerate::SEARCH_NAVIGATED);
        }
        // So does a sizes view (P13-M3).
        if let Some(sv) = t.sizeview.take() {
            if sv.done_ms.is_none() && !sv.cancel.swap(true, Ordering::Relaxed) {
                eprintln!("[sizeview] pane={pane} gen={} cancelled (navigated away)", sv.generation);
            }
        }
        t.model.search_names.set(false);
        // Another folder starts unfiltered (Explorer's behaviour).
        let filtered = !t.model.filter.borrow().is_empty();
        *t.model.filter.borrow_mut() = NameMatch::default();
        let id = t.id;
        if filtered {
            eprintln!("[filter] pane={pane} cleared");
        }
        st.filter_timer[pane].stop();
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
        note_recent(st, &app, pane);
        update_tabs(st, pane);
        update_status(st, &app, pane);
        app.invoke_reset_scroll(pane as i32);
        eprintln!("[nav] pane={pane} gen={generation} path={shown}");
        let tx = st.tx.clone();
        let cloud = cloud_of(st, pane, &path);
        fs::enumerate::spawn(path, pane, generation, st.config.thumbnails, cloud, tx, wake);
        schedule_save(st);
    });
}

/// Re-enumerate the active tab's folder in place (F5, Ctrl+R). The old rows stay on screen until
/// the new listing has been filtered and sorted; cursor, selection and scroll come back by name.
fn refresh(pane: usize) {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        // A sizes view walks again (P13-M3).
        if st.tab(pane).sizeview.is_some() {
            eprintln!("[refresh] pane={pane} sizes view: walked again");
            return start_sizes(st, &app, pane);
        }
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
            let (pattern, content, cancel, hidden) = (s.pattern.pattern.clone(), s.content, s.cancel.clone(), t.model.show());
            let what = if content { "content" } else { "start" };
            eprintln!("[search] pane={pane} gen={generation} {what} \"{pattern}\" in {}", path.display());
            let thumbs = search_thumbnails(st, &path);
            let (matcher, content) = search_spec(st, &pattern, content);
            fs::enumerate::spawn_search(path, matcher, hidden, thumbs, content, cancel, pane, generation, st.tx.clone(), wake);
            return;
        }
        let cloud = cloud_of(st, pane, &path);
        fs::enumerate::spawn(path, pane, generation, st.config.thumbnails, cloud, st.tx.clone(), wake);
    });
}

/// Whether a search from `root` keys its results' thumbnails (P9-M4): with the setting on, and not
/// in a cloud location, whose files' thumbnails could start downloads (a search tab carries no marks,
/// so nothing would keep them to the cache).
fn search_thumbnails(st: &mut State, root: &Path) -> bool {
    st.config.thumbnails && st.cloud.of(root) == icons::Cloud::None
}

/// The walk's name matcher and content search for a search tab's `pattern`: a name search matches
/// it against the names; a content search (P11-M1) reads every file for it and stays out of the
/// sync roots.
fn search_spec(st: &State, pattern: &str, content: bool) -> (NameMatch, Option<fs::enumerate::Content>) {
    if content {
        (NameMatch::default(), Some(fs::enumerate::Content { needle: fs::content::fold(pattern), cloud_roots: st.cloud.roots() }))
    } else {
        (NameMatch::new(pattern), None)
    }
}

/// A search tab's row name for `name`, the new name of the item at `source`, in its folder under
/// the search's `root` (`sub\dir\name`; `name` alone for an item in the root itself).
fn search_row_name(root: &Path, source: &Path, name: &str) -> String {
    match source.parent().and_then(|p| p.strip_prefix(root).ok()).map(|rel| rel.to_string_lossy().to_string()) {
        Some(rel) if !rel.is_empty() => format!("{rel}\\{name}"),
        _ => name.to_string(),
    }
}

/// The folder's cloud location (`cloud::Locations::of`), logged when it has one; This PC and
/// archive levels have none (nor do search tabs, which never come here).
fn cloud_of(st: &mut State, pane: usize, path: &Path) -> icons::Cloud {
    if is_this_pc(&path.to_string_lossy()) || fs::archive::split(path).is_some() {
        return icons::Cloud::None;
    }
    let c = st.cloud.of(path);
    if c != icons::Cloud::None {
        let what = if c == icons::Cloud::SyncRoot { "sync root" } else { "google drive" };
        eprintln!("[cloud] pane={pane} {}: {what}", path.display());
    }
    c
}

/// The pane's status bar from its active tab: items, hidden, note and error on the left, the
/// selection's count and size as badges after them, free space on the right. Called on navigation, enumeration
/// progress, pass swaps, selection changes and notes; never from a scroll or render path.
fn update_status(st: &State, app: &App, pane: usize) {
    let t = st.tab(pane);
    let m = &t.model;
    let loading = t.timing.done_ms.is_none() && t.timing.error.is_none();
    let enum_ms = if st.perf_status { t.timing.done_ms } else { None };
    // The running job's percentage beside its own note; jobs waiting behind it, shown on both
    // panes.
    let pct = progress_for(st.progress, &st.jobs, pane, t.id, &t.note);
    let note = statusbar::with_queue(&statusbar::with_progress(&t.note, pct), st.jobs.len().saturating_sub(1));
    let left = match &t.search {
        _ if t.sizeview.is_some() => {
            let sv = t.sizeview.as_ref().expect("checked");
            let r = *sv.tree.lock().unwrap_or_else(std::sync::PoisonError::into_inner).root();
            statusbar::sizes_text(&note, r.files, r.disk, r.folders, r.skipped, sv.done_ms.map(|ms| ms as f64 / 1e3))
        }
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
    let (count, size) = statusbar::selection_text(n, bytes, any_file, dirs);
    let s = StatusInfo {
        left: left.into(),
        count: count.into(),
        size: size.into(),
        right: statusbar::free_text(t.free).into(),
    };
    if pane == 0 { app.set_left_status(s) } else { app.set_right_status(s) }
    // Both panes: Undo, Redo and Cancel waiting jobs follow the stacks and the queue, which any pane's
    // job changes (P13-M2).
    for p in 0..2 {
        update_tools(st, app, p);
    }
}

/// Where the pane's active tab is, as the toolbar and the Windows 11 menu (P7-M2) see it.
fn tool_place(st: &State, pane: usize) -> toolbar::Place {
    let path = st.tab(pane).path.to_string_lossy();
    if is_this_pc(&path) {
        toolbar::Place::ThisPc
    } else if st.is_results(pane) {
        toolbar::Place::Search
    } else if in_archive(st, pane) {
        if in_readonly_archive(st, pane) { toolbar::Place::Archive } else { toolbar::Place::Zip }
    } else {
        toolbar::Place::Folder
    }
}

/// The pane's toolbar buttons, from what its status bar shows (where the tab is, its selection)
/// and the clipboard; set only when they change, in `update_status`'s rhythm (P6-M3).
fn update_tools(st: &State, app: &App, pane: usize) {
    let t = st.tab(pane);
    let facts = toolbar::Facts {
        back: t.history.back_len() > 0,
        forward: t.history.forward_len() > 0,
        this_pc: is_this_pc(&t.path.to_string_lossy()),
        undo: st.undo.last().map(|r| r.text.clone()),
        redo: st.redo.last().map(|r| r.text.clone()),
        busy: !st.jobs.is_empty(),
        waiting: st.jobs.len().saturating_sub(1),
        hidden: t.model.show_hidden.get(),
        view: view_of(st, pane).index(),
    };
    let tools = toolbar::with_facts(toolbar::state(tool_place(st, pane), st.model(pane).selected_count(), st.clip_files), &facts);
    let now = if pane == 0 { app.get_left_tools() } else { app.get_right_tools() };
    if now != tools {
        if pane == 0 { app.set_left_tools(tools) } else { app.set_right_tools(tools) }
    }
}

/// A toolbar button (P6-M3): the pane becomes active and the button runs its key's command, so
/// there is one path per command. The list takes the keys after it, except after the three
/// buttons that open the name editor, which takes them itself: the list's focus request lands a
/// moment later (a `changed` handler) and would close the editor again.
fn toolbar_clicked(pane: usize, button: &str) {
    // View and More open their menus (P13-M2); a button press without a point (the driver) puts the
    // menu under the pane's path bar.
    if button == "view" || button == "more" {
        let Some(at) = with_state(|st| {
            let app = st.app.upgrade()?;
            let l = window_layout(&app);
            Some((dnd::pane_x(&l, pane).0 + 8.0, dnd::list_top(&l, pane) - 34.0))
        }) else {
            return;
        };
        return open_tool_menu(pane, button, at.0, at.1);
    }
    // The Preview button (P8-M1) is Alt+P's toggle, for the window, not a pane's command.
    if button == "preview" {
        eprintln!("[toolbar] pane={pane} preview -> toggle");
        return toggle_preview();
    }
    // Two panes (P13-M1) is F3's toggle, for the window.
    if button == "split" {
        eprintln!("[toolbar] pane={pane} split -> toggle");
        if let Some(app) = with_state(|st| st.app.upgrade()) {
            app.invoke_toggle_split();
        }
        return;
    }
    let Some(cmd) = toolbar::command(button) else {
        return eprintln!("[toolbar] pane={pane} no button {button}");
    };
    eprintln!("[toolbar] pane={pane} {button} -> {cmd}");
    let Some(app) = with_state(|st| st.app.upgrade()) else { return };
    if matches!(cmd, "rename" | "newfile" | "newfolder") {
        app.set_active_pane(pane as i32);
    } else {
        app.invoke_focus_pane(pane as i32);
    }
    command(pane, cmd);
}

/// The toolbar's View or More menu (P13-M2) for `pane`, at window point (x, y) (under its button):
/// TwinFM's own Windows 11 menu, its items `toolbar::view_menu` or `toolbar::more_menu` (the buttons
/// the pane's bar had no room for first). `[toolbar] menu <name>: <items>`.
fn open_tool_menu(pane: usize, name: &str, x: f32, y: f32) {
    winmenu_close("another menu opens");
    let Some((req, ctx)) = menu_request(pane, -1, x, y, false) else { return };
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let tools = if pane == 0 { app.get_left_tools() } else { app.get_right_tools() };
        let (items, own) = if name == "view" {
            (toolbar::view_menu(tools.view), 0)
        } else {
            let fit = tool_fit(&app, pane) as usize;
            let items = toolbar::more_menu(&tools, fit, app.get_split(), tool_place(st, pane));
            let own = items.len() - 6;
            (items, own)
        };
        let entries = winmenu::tool_items(&items, own);
        eprintln!(
            "[toolbar] menu {name}: {}",
            items.iter().map(|i| format!("{}{}{}", i.text, if i.enabled { "" } else { " (off)" }, if i.ticked { " (on)" } else { "" })).collect::<Vec<_>>().join(" | ")
        );
        let wctx = winmenu::Ctx { place: tool_place(st, pane), what: winmenu::What::Background, clipboard_files: st.clip_files, same_type: false };
        st.winmenu_seq += 1;
        st.winmenu = Some(WinMenu {
            seq: st.winmenu_seq,
            pane,
            row: -1,
            at: (x, y),
            req,
            ctx,
            wctx,
            waiting: false,
            answer: explorercmd::Answer { seq: st.winmenu_seq, ..Default::default() },
            opened: Instant::now(),
            shown: false,
            tools: Vec::new(),
            items: Vec::new(),
            placed: winmenu::Placed { x, y, h: 0.0, scrolls: false, left: false, up: false },
            hl: -1,
            sub: None,
            history: None,
            tool: Some((entries, items)),
        });
    });
    show_winmenu("the toolbar");
}

/// The buttons pane `pane`'s bar shows before More: the bar across the window's for the pane it acts
/// on, else the pane's own.
fn tool_fit(app: &App, pane: usize) -> i32 {
    if app.get_toolbar() == settings::Toolbar::Top.index() {
        app.get_top_tool_fit()
    } else if pane == 0 {
        app.get_left_tool_fit()
    } else {
        app.get_right_tool_fit()
    }
}

/// A View or More menu's item, picked (P13-M2): the command its key or button calls.
/// `[toolbar] pane=N picks "<item>" -> <command>`.
fn run_tool(pane: usize, item: &toolbar::MenuItem) {
    eprintln!("[toolbar] pane={pane} picks \"{}\" -> {}", item.text, item.command);
    let Some(app) = with_state(|st| st.app.upgrade()) else { return };
    match item.command {
        "view:details" => set_view(pane, settings::View::Details),
        "view:large" => set_view(pane, settings::View::Large),
        "view:medium" => set_view(pane, settings::View::Medium),
        "selectall" => {
            app.invoke_focus_pane(pane as i32);
            app.invoke_select(pane as i32, SelOp::All as i32, current_row(&app, pane));
        }
        "copyother" | "moveother" => {
            app.invoke_focus_pane(pane as i32);
            command(pane, if item.command == "copyother" { "copy" } else { "move" });
        }
        "sizeall" => {
            app.invoke_focus_pane(pane as i32);
            command(pane, "sizeall");
        }
        "hidden" => toggle_hidden(pane),
        "cancelwaiting" => {
            cancel_queued(pane);
        }
        other => toolbar_clicked(pane, other),
    }
}

/// One of the path bar's buttons (P13-M2): its pane becomes active and goes back, forward, up or
/// refreshes, as Alt+Left, Alt+Right, Alt+Up and F5 do there. `[nav] pane=N <Back|Forward|Up|Refresh>
/// button`.
fn nav_button(pane: usize, name: &str) {
    let Some(app) = with_state(|st| st.app.upgrade()) else { return };
    app.invoke_focus_pane(pane as i32);
    let label = match name {
        "back" => "Back",
        "forward" => "Forward",
        "up" => "Up",
        "refresh" => "Refresh",
        _ => return eprintln!("[nav] pane={pane} no button {name}"),
    };
    eprintln!("[nav] pane={pane} {label} button");
    match name {
        "back" => history_move(pane, false),
        "forward" => history_move(pane, true),
        "up" => {
            go_up(pane);
        }
        _ => refresh(pane),
    }
}

/// A toolbar's width changed (`which` -1 the bar across the window, 0 or 1 a pane's own): how many of
/// its buttons fit before More (`toolbar::fit`), set when that changes. `[toolbar] top|pane=N fits K of
/// 12 buttons at W px`.
fn toolbar_width(app: &App, which: i32, w: f32) {
    if w <= 0.0 {
        return;
    }
    // The buttons' widths as pane.slint's ToolButton lays them out: 6 + 16 + 6 px icon-only, 8 + 16 + 6
    // + the label + 10 px labelled (measured in the heavier weight, so a bar never clips a label); a
    // menu button's chevron 6 + 12 px more.
    let labelled = which < 0;
    let label_w = |text: &str| if labelled { 40.0 + app.invoke_measure(text.into(), true) } else { 28.0 };
    let texts = ["New file", "New folder", "Cut", "Copy", "Paste", "Rename", "Delete", "Undo", "Redo", "View", "Two panes", "Preview pane"];
    let widths: Vec<f32> = texts.iter().map(|t| label_w(t) + if *t == "View" { 18.0 } else { 0.0 }).collect();
    let seps: Vec<bool> = toolbar::BUTTONS.iter().map(|b| toolbar::SEP_BEFORE.contains(b)).collect();
    let more_w = label_w("More") + 18.0;
    let fit = toolbar::fit(&widths, &seps, w, more_w, 2.0, 9.0, 2.0) as i32;
    let now = match which {
        -1 => app.get_top_tool_fit(),
        0 => app.get_left_tool_fit(),
        _ => app.get_right_tool_fit(),
    };
    if fit == now {
        return;
    }
    match which {
        -1 => app.set_top_tool_fit(fit),
        0 => app.set_left_tool_fit(fit),
        _ => app.set_right_tool_fit(fit),
    }
    let bar = if which < 0 { "top".to_string() } else { format!("pane={which}") };
    eprintln!("[toolbar] {bar} fits {fit} of {} buttons at {w:.0} px", widths.len());
}

/// Alt+P, the toolbar's Preview button and the dialog (P8-M1): show or hide the preview column,
/// remembered in the settings.
fn toggle_preview() {
    let Some(app) = with_state(|st| {
        schedule_save(st);
        st.app.upgrade()
    }) else {
        return;
    };
    let on = !app.get_preview();
    app.set_preview(on);
    preview::set_visible(on);
}

/// The preview's file (P8-M1): the active pane's cursor entry, from its listing (no file system
/// call); None when the cursor is on no row.
pub fn preview_target() -> Option<preview::Target> {
    with_state(|st| {
        let app = st.app.upgrade()?;
        let pane = active_pane(&app);
        let e = st.model(pane).entry(usize::try_from(current_row(&app, pane)).ok()?)?;
        Some(preview::Target { pane, path: st.tab(pane).path.join(e.name.as_str()), attrs: e.attrs, is_dir: e.is_dir() })
    })
}

/// The main window's handle, for the preview's links (P8-M2), 0 before the window exists.
pub fn preview_hwnd() -> isize {
    with_state(|st| st.app.upgrade().map_or(0, |app| main_hwnd(st, &app)))
}

/// A link in the Markdown preview (P8-M2) to a folder (`cursor` false): the active pane goes there;
/// to a program or script (`cursor` true): the active pane goes to its folder with the cursor on it.
pub fn show_in_pane(path: &Path, cursor: bool) {
    let Some(pane) = with_state(|st| st.app.upgrade().map(|app| active_pane(&app))) else { return };
    if !cursor {
        return navigate(pane, path.to_path_buf());
    }
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else { return };
    let name: SharedString = name.to_string_lossy().as_ref().into();
    navigate(pane, dir.to_path_buf());
    // The first pass selects it and scrolls it into view; the pass after the listing is complete
    // puts the cursor on it, however long the folder.
    with_state(|st| {
        let t = st.tab_mut(pane);
        t.focus_name = Some(name.clone());
        t.restore_cursor = Some(name);
    });
}

/// The running job's percentage for a tab's status bar: only on the pane and tab that submitted
/// it, while the tab still shows the job's own running note.
fn progress_for(progress: Option<(u64, u32)>, jobs: &HashMap<u64, JobInfo>, pane: usize, tab: u64, note: &str) -> Option<u32> {
    let (id, p) = progress?;
    jobs.get(&id).filter(|j| j.pane == pane && j.tab == tab && j.running == note).map(|_| p)
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

/// The pane's tab strip changed width (pane.slint `strip-resized`): its active tab is scrolled back
/// into view, logged when the strip's offset moves (a splitter drag calls this on every move).
fn strip_resized(pane: usize) {
    let Some((app, active, n)) = with_state(|st| Some((st.app.upgrade()?, st.panes[pane].active_index(), st.panes[pane].len()))) else {
        return;
    };
    let before = app.invoke_tab_offset(pane as i32);
    let x = app.invoke_reveal_tab(pane as i32, active as i32);
    if (x - before).abs() >= 0.5 {
        eprintln!("[tab] pane={pane} strip resized: scrolled to {x:.0} (tab {} of {n} in view)", active + 1);
    }
}

/// A tab's title: its folder's name, or `Search: <pattern>` for a search tab (`Text: <text>` for
/// a content search, P11-M1).
fn tab_title(t: &Tab) -> String {
    if t.sizeview.is_some() {
        return format!("Sizes: {}", pathbar::title(&t.path.to_string_lossy()));
    }
    match &t.search {
        Some(s) if s.content => format!("Text: {}", s.pattern.pattern),
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
        let what = if s.content { "Text" } else { "Search" };
        crumbs.push(Crumb { label: format!("{what}: \"{}\"", s.pattern.pattern).into() });
    }
    if t.sizeview.is_some() {
        crumbs.push(Crumb { label: "Sizes".into() });
    }
    crumb_geometry_reset(pane, crumbs.len());
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

/// A history place as the settings file keeps it, and back (P11-M3).
fn saved_place(p: &Place) -> settings::SavedPlace {
    settings::SavedPlace { path: p.path.to_string_lossy().to_string(), search: p.search.clone(), content: p.content }
}

fn place_of(s: &settings::SavedPlace) -> Place {
    Place { path: PathBuf::from(&s.path), search: s.search.clone(), content: s.content }
}

/// Gather the settings from the window and the panes and write them (atomically).
fn save_now() {
    let Some((config, path)) = with_state(|st| {
        st.save_timer.stop();
        let app = st.app.upgrade()?;
        // Each pane's tabs; a pane whose tabs never had a folder (the second pane before the
        // split was first opened) keeps the saved list.
        // Each tab with its view (P6-M3): the active one's cursor and scroll as the list has them
        // now, the others' as they were left.
        let tab_paths = |pane: usize, tabs: &Tabs<Tab>| {
            let active = tabs.active_index();
            let v: Vec<settings::SavedTab> = tabs
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let (cursor, scroll) = if i == active { (current_row(&app, pane), app.invoke_get_scroll(pane as i32)) } else { (t.cursor, t.scroll) };
                    let cursor = t.restore_cursor.clone().or_else(|| usize::try_from(cursor).ok().and_then(|r| t.model.entry(r)).map(|e| e.name));
                    let (back, forward) = t.history.saved(settings::HISTORY_SAVED);
                    settings::SavedTab {
                        back: back.iter().map(saved_place).collect(),
                        forward: forward.iter().map(saved_place).collect(),
                        path: t.path.to_string_lossy().to_string(),
                        sort: Some(settings::sort_name(t.model.sort_col.get()).into()),
                        ascending: Some(t.model.sort_asc.get()),
                        hidden: Some(t.model.show_hidden.get()),
                        cursor: cursor.map(|c| c.to_string()),
                        scroll: Some(t.restore_scroll.unwrap_or(scroll)),
                        sizes: t.sizeview.is_some(),
                    }
                })
                .collect();
            v.iter().all(|t| !t.path.is_empty()).then_some((v, active))
        };
        let (left_tabs, right_tabs) = (tab_paths(0, &st.panes[0]), tab_paths(1, &st.panes[1]));
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
        c.preview = app.get_preview();
        c.preview_width = app.get_preview_width();
        c.preview_zoom = app.get_preview_zoom() as u32;
        c.syntax_colours = app.get_syntax_colours();
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

/// A pane showed its active tab's folder (P12-M5, a navigation or a tab put on screen): the folder
/// goes to the top of the sidebar's Recent group, unless it is This PC, an archive level, a search tab
/// or the hidden right pane's; the sidebar is rebuilt only when the list changed.
fn note_recent(st: &mut State, app: &App, pane: usize) {
    let t = st.tab(pane);
    let path = t.path.to_string_lossy().to_string();
    if (pane == 1 && !app.get_split()) || t.results() || is_this_pc(&path) || fs::archive::split(&t.path).is_some() {
        return;
    }
    if sidebar::note_recent(&mut st.config.recent, &path) {
        eprintln!("[sidebar] recent: {path} ({} kept)", st.config.recent.len());
        rebuild_sidebar(st);
        schedule_save(st);
    }
}

/// The Recent header's "Clear recent" (P12-M5).
fn recent_clear() {
    with_state(|st| {
        let n = st.config.recent.len();
        st.config.recent.clear();
        eprintln!("[sidebar] recent cleared ({n} removed)");
        rebuild_sidebar(st);
        schedule_save(st);
    });
}

/// Rebuild the sidebar's rows from its sections (on a change only).
fn rebuild_sidebar(st: &mut State) {
    st.side = sidebar::compose(&st.drives, &st.places, &st.config.recent, &st.config.bookmarks);
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

/// A key in the sidebar once it has the keys (P7-M4): Up, Down, Home and End move the current row
/// (`sidebar::step`), Return opens the current row's folder in the active pane as a click does.
/// Returns the row current after it.
fn side_step(current: i32, key: &str) -> i32 {
    if key == "Return" {
        if let Ok(i) = usize::try_from(current) {
            eprintln!("[sidebar] Return on row {i}");
            side_clicked(i, false);
        }
        return current;
    }
    let kinds: Vec<_> = with_state(|st| st.side.iter().map(|r| r.kind).collect());
    let Some(to) = sidebar::step(&kinds, current, key) else { return current };
    let label = with_state(|st| st.side.get(to).map(|r| r.label.clone())).unwrap_or_default();
    eprintln!("[sidebar] key {key}: row {to} ({label})");
    to as i32
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

/// A bookmark dragged in the sidebar (P12-M5) and dropped in gap `gap`.
fn bookmark_drag(from: usize, gap: usize) {
    with_state(|st| {
        let to = sidebar::drag_target(from, gap);
        if !sidebar::move_bookmark_to(&mut st.config.bookmarks, from, to) {
            return eprintln!("[sidebar] bookmark {from} dragged to its own place: not moved");
        }
        rebuild_sidebar(st);
        schedule_save(st);
        eprintln!("[sidebar] bookmark moved {from} -> {to} (drag)");
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
                Msg::Done { pane, generation, total, elapsed, error, free, reparse, archive } => {
                    let Some(t) = st.by_generation(pane, generation) else { continue };
                    let wall = t.timing.requested.map(perf::elapsed_ms).unwrap_or(0.0);
                    if t.model.is_refreshing() {
                        let (carried, looked) = t.model.finish_refresh();
                        if looked > 0 {
                            eprintln!("[icons] pane={pane} marks carried: {carried} of {looked} across the refresh");
                        }
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
                            let level = archive.map_or(String::new(), |a| format!(" ({})", a.describe()));
                            eprintln!(
                                "[enum] pane={pane} done: {total} entries{level}, worker {:.1} ms, request-to-done {wall:.1} ms{split}",
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
                        // A folder that cannot be listed is not watched (`watch_tabs`).
                        st.watch_active(&app, pane);
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

/// More changes than this in one watch window: the whole-folder refresh, cheaper than as many
/// single-row edits (each moves the rest of `shown` on a long listing).
const EDIT_CAP: usize = 500;

/// How many unknown names `land_edits` lets off (drops or turns into adds) in one window before it
/// falls back.
const LENIENT_TRIES: usize = 8;

/// How long after a file operation finished a watch window it fully explains is ignored.
const EXPLAINED_FOR: Duration = Duration::from_secs(3);

/// A tab as `watch_set` sees it.
#[derive(Clone, Copy, Debug)]
struct WatchCand {
    id: u64,
    active: bool,
    /// May be watched at all (an active tab that is no search and whose listing has not failed; a
    /// background folder tab that is listed and not stale).
    eligible: bool,
    shown_at: Instant,
}

/// The tabs to watch, in the order asked: the eligible active tabs, then the eligible background
/// ones most recently shown first, at most `cap` in all.
fn watch_set(cands: &[WatchCand], cap: usize) -> Vec<u64> {
    let mut v: Vec<&WatchCand> = cands.iter().filter(|c| c.eligible).collect();
    v.sort_by(|a, b| b.active.cmp(&a.active).then(b.shown_at.cmp(&a.shown_at)));
    v.into_iter().take(cap).map(|c| c.id).collect()
}

/// A folder tab: not a search, not This PC, not a level inside an archive (never watched).
fn folder_tab(t: &Tab) -> bool {
    !t.results() && !is_this_pc(&t.path.to_string_lossy()) && !fs::archive::inside(&t.path)
}

/// Every name the window's changes touch (both names of a rename) is in `set` (lower-cased).
fn explained_by(changes: &[fs::watch::Change], set: &HashSet<String>) -> bool {
    use fs::watch::Change;
    let has = |n: &str| set.contains(&n.to_lowercase());
    !changes.is_empty()
        && changes.iter().all(|c| match c {
            Change::Added(n) | Change::Removed(n) | Change::Modified(n) => has(n),
            Change::Renamed { from, to } => has(from) && has(to),
        })
}

/// The lower-cased names a finished job touched in `dir`: every source and destination whose
/// folder is `dir`, and the name a rename or a new folder asked for.
fn explained_names(dir: &Path, job: &Job, results: &[fileops::ItemResult]) -> HashSet<String> {
    let mut set = HashSet::new();
    let mut add = |p: &Path| {
        if let (Some(d), Some(n)) = (p.parent(), p.file_name()) {
            if same_dir(d, dir) {
                set.insert(n.to_string_lossy().to_lowercase());
            }
        }
    };
    for r in results {
        add(&r.source);
        if let Some(d) = &r.dest {
            add(d);
        }
    }
    match job {
        Job::Rename { source, new_name } => add(&source.with_file_name(new_name)),
        Job::NewFolder { parent, name } | Job::NewFile { parent, name } => add(&parent.join(name)),
        _ => {}
    }
    set
}

/// A window's changes as row edits, given a fresh stat per name (None: the name is gone again):
/// an add that is gone is nothing, a modified name that is gone is removed, a rename whose new
/// name is gone removes the old one. Also the names added or renamed to that were gone (the
/// listing will not have them; `Tab::vanished`).
fn edits_from(changes: &[fs::watch::Change], mut stat: impl FnMut(&str) -> Option<model::Entry>) -> (Vec<model::Edit>, Vec<String>) {
    use fs::watch::Change;
    use model::Edit;
    let mut edits = Vec::with_capacity(changes.len());
    let mut vanished = Vec::new();
    for c in changes {
        match c {
            Change::Added(n) => match stat(n) {
                Some(entry) => edits.push(Edit::Upsert(entry)),
                None => vanished.push(n.clone()),
            },
            Change::Removed(n) => edits.push(Edit::Remove(n.as_str().into())),
            Change::Modified(n) => edits.push(stat(n).map_or_else(|| Edit::Remove(n.as_str().into()), Edit::Upsert)),
            Change::Renamed { from, to } => edits.push(match stat(to) {
                Some(entry) => Edit::Rename { from: from.as_str().into(), entry },
                None => {
                    vanished.push(to.clone());
                    Edit::Remove(from.as_str().into())
                }
            }),
        }
    }
    (edits, vanished)
}

/// A watch window for the edit worker: the tab, its folder (as the tab held it when the window was
/// sent), the changes, and the listing they were checked against (its generation and pass), which
/// `land_edits` checks again.
struct StatJob {
    pane: usize,
    tab: u64,
    path: PathBuf,
    changes: Vec<fs::watch::Change>,
    /// The window's record count, for the fallback lines.
    records: usize,
    generation: u64,
    pass: u64,
    thumbnails: bool,
    /// The folder's cloud location, for the edited entries' keys.
    cloud: icons::Cloud,
    /// The listing is younger than the longest window (`young_listing`): a name it does not have
    /// may be one the window's early records saw go before the listing began.
    young: bool,
}

/// The folder the edit worker stats under: absolute, backslashes, no `.` or `..` (Win32's
/// `GetFullPathNameW`, through `std::path::absolute`), as `stat_entry`'s verbatim prefix needs. A
/// tab's path is kept as typed (`twinfm.exe C:/x`, `C:\a\..\b`, a saved tab).
fn stat_dir(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The edit worker: stats the names of each watch window (one window at a time, in order, so
/// windows land in the order they came) and posts the edits to the UI thread. Never the UI thread
/// or the watcher thread: a stat can block on a busy disk. When a stat fails and the folder itself
/// is gone (deleted, or renamed since the window was sent), the window is no removal of its names:
/// `land_edits` falls back instead.
fn edit_worker() -> Sender<StatJob> {
    let (tx, rx) = unbounded::<StatJob>();
    std::thread::Builder::new()
        .name("watch-edits".into())
        .spawn(move || {
            // A sync root's changed files are statted with their placeholders shown (P12-M7), as
            // its listing is, so their marks follow their states; nothing else lists differently.
            fs::enumerate::expose_placeholders("watch edits");
            for mut job in rx {
                let t0 = Instant::now();
                let dir = stat_dir(&job.path);
                let mut types = fs::enumerate::TypeNames::default();
                let mut keys = icons::IconKeys::new(job.path.clone(), job.thumbnails).with_cloud(job.cloud);
                let zone = model::LocalZone::current();
                let mut failed = false;
                let (edits, vanished) = edits_from(&job.changes, |n| {
                    let entry = fs::enumerate::stat_entry(&dir, n, &mut types, &mut keys, &zone);
                    failed |= entry.is_none();
                    entry
                });
                let folder_gone = failed && std::fs::metadata(&dir).is_err();
                let stat_ms = perf::elapsed_ms(t0);
                job.changes = Vec::new();
                let _ = slint::invoke_from_event_loop(move || land_edits(job, edits, vanished, folder_gone, stat_ms));
            }
        })
        .expect("spawn watch edit thread");
    tx
}

/// Why a tab's listing cannot take a window's edits now (listed without error, no refresh or pass
/// in flight), or None.
fn unsettled(t: &Tab) -> Option<&'static str> {
    if t.timing.done_ms.is_none() || t.timing.error.is_some() {
        Some("listing in flight")
    } else if !t.model.settled() {
        Some("pass in flight")
    } else {
        None
    }
}

/// The whole-folder refresh in place of a window's edits: the active tab refreshes (after the
/// listing in flight, if there is one), a background tab goes stale (and gives up its watch).
/// Returns the outcome for the caller's line.
fn fall_back(st: &mut State, pane: usize, tab_id: u64, again: &mut Vec<usize>) -> &'static str {
    let active = st.is_active(pane, tab_id);
    let Some(t) = st.find(pane, tab_id) else { return "tab closed" };
    if !active {
        t.stale = true;
        if let Some(app) = st.app.upgrade() {
            st.watch_tabs(&app);
        }
        "stale"
    } else if t.timing.done_ms.is_none() && t.timing.error.is_none() {
        t.refresh_pending = true;
        "refresh after it"
    } else {
        t.watch_refresh = true;
        if !again.contains(&pane) {
            again.push(pane);
        }
        "refresh"
    }
}

/// A directory watch event (on the UI thread, inside `drain`). A window of changes is applied to
/// the tab's listing as row edits (its names statted on the edit worker, then `land_edits`) when
/// the listing is settled and the window allows it (`window_refusal`); otherwise the whole-folder
/// refresh, with its reason (`fall_back`). A window that nets to nothing, or that a file
/// operation's own results explain, does nothing. A lost watch refreshes the active tab once, so
/// its error shows. A subtree watch's change below the folder only drops the sizes of the
/// subfolders it lay under (and sizes them again if they are selected); a change in the folder
/// itself, or an overflow, refreshes.
fn watch_event(st: &mut State, ev: fs::watch::Event, again: &mut Vec<usize>) {
    use fs::watch::Event;
    let (pane, tab_id) = match &ev {
        Event::Changed { pane, tab_id, .. }
        | Event::Lost { pane, tab_id, .. }
        | Event::Subtree { pane, tab_id, .. }
        | Event::Refused { pane, tab_id, .. }
        | Event::Moved { pane, tab_id, .. } => (*pane, *tab_id),
    };
    let active = st.is_active(pane, tab_id);
    let Some(t) = st.find(pane, tab_id) else { return };
    let (mut resize, mut stale) = (false, false);
    let records = match ev {
        Event::Refused { path, .. } => {
            // The watcher logged why. For an older request (the tab has moved on) nothing changes.
            if path == t.path {
                t.watched = false;
                t.watched_bg = false;
                if !active && t.search.is_none() {
                    t.stale = true;
                    t.drop_sizes(pane, "not watched in the background");
                }
            }
            return;
        }
        Event::Moved { to, .. } => {
            // The watcher follows the folder; so do the tab, its title and the saved tab list. The
            // history needs nothing: its current place is the tab's folder.
            t.path = to;
            t.explained = None;
            eprintln!("[nav] pane={pane} path follows the rename: {}", t.path.display());
            // A listing of the old path still in flight will fail: list the new one.
            if t.timing.done_ms.is_none() {
                if active {
                    if !again.contains(&pane) {
                        again.push(pane);
                    }
                } else if t.search.is_none() {
                    t.stale = true;
                }
            }
            if let Some(app) = st.app.upgrade() {
                if active {
                    set_path(st, &app, pane);
                }
                // A `watch_tabs` between the watcher's `Moved` and this asked for the old path,
                // which the watcher refused: ask again, for the new one.
                st.watch_tabs(&app);
            }
            update_tabs(st, pane);
            schedule_save(st);
            return;
        }
        Event::Lost { error, .. } => {
            t.watched = false;
            t.watched_bg = false;
            if active {
                eprintln!("[watch] pane={pane} tab={tab_id} watch lost ({error}): refresh");
                if !again.contains(&pane) {
                    again.push(pane);
                }
            } else if t.search.is_none() {
                t.stale = true;
                eprintln!("[watch] pane={pane} tab={tab_id} watch lost ({error}): stale");
                t.drop_sizes(pane, "watch lost");
            }
            return;
        }
        Event::Changed { changes, records, overflow, .. } => {
            // A stale background tab is watched only for its folder sizes; its listing is
            // refreshed when it is shown.
            if !active && t.stale {
                return;
            }
            take_window(st, pane, tab_id, active, (!overflow).then_some(changes), records, again);
            return;
        }
        Event::Subtree { top, changes, overflow, edits, records, .. } => {
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
                    // A background tab walks nothing: a changed folder's size is dropped there.
                    if !active {
                        if let Some(name) = t.sizes.cache.keys().find(|k| k.eq_ignore_ascii_case(child)).cloned() {
                            t.sizes.forget(&name);
                            t.model.set_size(&name, None);
                            eprintln!("[size] invalidated {} ({change}): tab in the background, size dropped", t.path.join(&name).display());
                        }
                        continue;
                    }
                    if let Some((name, delay)) = t.sizes.invalidate(child, now) {
                        let path = t.path.join(&name);
                        eprintln!("[size] invalidated {} ({change})", path.display());
                        eprintln!("[size] stale {}: re-walk in {:.1} s", path.display(), delay.as_secs_f64());
                        stale = true;
                    }
                }
            }
            // As for `Changed`: a stale background tab refreshes when it is shown.
            if !overflow && !active && t.stale {
                return;
            }
            if stale {
                arm_rewalk(st, pane);
            }
            // Re-sizing the selection is the active tab's (a background one keeps what it has).
            let resize = resize && active;
            if !(top || overflow) {
                eprintln!("[watch] pane={pane} tab={tab_id} changed below the folder: no refresh");
                if resize {
                    resize_after_change(st, pane);
                }
                return;
            }
            // A change in the folder itself: its own records become row edits as a plain watch's
            // do (P7-M4), unless there were too many to keep.
            if let (false, Some(edits)) = (overflow, edits) {
                take_window(st, pane, tab_id, active, Some(edits), records, again);
                return;
            }
            changes.len()
        }
    };
    // An overflow, or more records in the folder itself than a window keeps: the whole-folder
    // refresh (the count is of the children it named).
    let outcome = fall_back(st, pane, tab_id, again);
    eprintln!("[watch] pane={pane} tab={tab_id} fallback: subtree watch ({records} records): {outcome}");
    if resize {
        resize_after_change(st, pane);
    }
}

/// A window of a tab's folder: its net `changes` from `records` raw records (None: an overflow)
/// become row edits, statted on the edit worker, when `window_refusal` allows; else the
/// whole-folder refresh with its reason. A plain watch's `Changed` and, since P7-M4, a subtree
/// watch's records in the folder itself come here.
fn take_window(st: &mut State, pane: usize, tab_id: u64, active: bool, changes: Option<Vec<fs::watch::Change>>, records: usize, again: &mut Vec<usize>) {
    let Some(t) = st.find(pane, tab_id) else { return };
    let reason = match changes {
        None => "overflow".to_string(),
        Some(changes) if changes.is_empty() => {
            eprintln!("[watch] pane={pane} tab={tab_id} {records} records netted to nothing");
            return;
        }
        Some(changes) => match window_refusal(t, active, &changes) {
            Some(why) if why.is_empty() => {
                eprintln!("[watch] pane={pane} tab={tab_id} {records} records explained by the operation: no refresh");
                return;
            }
            Some(why) => why,
            None => {
                let job = StatJob {
                    pane,
                    tab: tab_id,
                    path: t.path.clone(),
                    changes,
                    records,
                    generation: t.model.generation.get(),
                    pass: t.model.pass.get(),
                    young: young_listing(t),
                    thumbnails: st.config.thumbnails,
                    cloud: icons::Cloud::None,
                };
                let cloud = st.cloud.of(&job.path);
                let job = StatJob { cloud, ..job };
                let _ = st.stat_tx.send(job);
                return;
            }
        },
    };
    let outcome = fall_back(st, pane, tab_id, again);
    eprintln!("[watch] pane={pane} tab={tab_id} fallback: {reason} ({records} records): {outcome}");
}

/// Why a window of changes for tab `t` cannot be applied as row edits (the fallback's reason); ""
/// when a file operation's results explain all of it (nothing to do); None when it can.
fn window_refusal(t: &mut Tab, active: bool, changes: &[fs::watch::Change]) -> Option<String> {
    if active {
        match &t.explained {
            Some((_, at)) if at.elapsed() > EXPLAINED_FOR => t.explained = None,
            Some((set, _)) if explained_by(changes, set) => return Some(String::new()),
            _ => {}
        }
    }
    if !folder_tab(t) {
        return Some("not a folder listing".into());
    }
    if let Some(why) = unsettled(t) {
        return Some(why.into());
    }
    if fs::watch::is_remote(&t.path) {
        return Some("network path".into());
    }
    // A name the listing does not have is `apply_edits`'s to find (`land_edits`), not a scan here.
    if changes.len() > EDIT_CAP {
        return Some(format!("{} changes over the cap", changes.len()));
    }
    None
}

/// The tab's listing began less than the longest watch window (plus one short one) ago: a window
/// delivered now can hold records from before it, so a name it removes or renames may already be
/// gone from the listing. Such names are no reason to list the folder again (the listing is newer
/// than they are); `land_edits` drops their removal and turns their rename into an add.
fn young_listing(t: &Tab) -> bool {
    t.timing.requested.is_some_and(|r| r.elapsed() < fs::watch::COALESCE_MAX + fs::watch::COALESCE)
}

/// The name an `apply_edits` error says the listing does not have (`remove of "n": not in the
/// listing`, `rename of "n" to "m": not in the listing`). A Win32 file name holds no `"`.
fn unknown_name(why: &str) -> Option<&str> {
    why.ends_with(": not in the listing").then(|| why.split('"').nth(1)).flatten()
}

/// A window's edits came back from the edit worker (on the UI thread): apply them to the tab's
/// listing if it is still the one they were checked against, else drop them and refresh (the
/// records are spent). A removal of a name the listing does not have because it vanished before
/// it was listed (`Tab::vanished`), or went before a young listing began (`young`), is dropped, a
/// rename from one is an add. A window statted under a folder that has gone (`folder_gone`), or
/// under a path the tab no longer has (the folder renamed since), falls back.
fn land_edits(job: StatJob, edits: Vec<model::Edit>, gone: Vec<String>, folder_gone: bool, stat_ms: f64) {
    let StatJob { pane, tab: tab_id, path, records, generation, pass, young, .. } = job;
    let again = with_state(|st| {
        let mut again = Vec::new();
        let app = st.app.upgrade()?;
        let active = st.is_active(pane, tab_id);
        let Some(t) = st.find(pane, tab_id) else {
            eprintln!("[watch] pane={pane} tab={tab_id} edits dropped: tab closed");
            return None;
        };
        let why = if t.model.generation.get() != generation {
            Some("the listing changed")
        } else if t.model.pass.get() != pass {
            Some("a pass was requested")
        } else if t.path != path {
            Some("the path changed")
        } else {
            unsettled(t)
        };
        if let Some(why) = why {
            let outcome = fall_back(st, pane, tab_id, &mut again);
            eprintln!("[watch] pane={pane} tab={tab_id} edits dropped: {why}: {outcome}");
            return Some(again);
        }
        if folder_gone {
            let outcome = fall_back(st, pane, tab_id, &mut again);
            eprintln!("[watch] pane={pane} tab={tab_id} fallback: folder gone ({records} records): {outcome}");
            return Some(again);
        }
        if t.vanished.0 != generation {
            t.vanished = (generation, HashSet::new());
        }
        let cursor = if active { current_row(&app, pane) } else { t.cursor };
        // `apply_edits` is all-or-nothing and names the first name the listing does not have. One
        // that vanished before it was listed, or went before a young listing began, is no reason to
        // list again: its removal is dropped, a rename from it becomes an add, and the rest is
        // applied again (a few times at most). A name with a `~` is never let off: a record can
        // carry a short name (`LONGFI~1.TXT`) for a file the listing has under its long one.
        let mut edits = edits;
        let mut tries = 0;
        let t0 = Instant::now();
        let res = loop {
            let why = match t.model.apply_edits(edits.clone(), cursor) {
                Ok(a) => break Ok(a),
                Err(why) => why,
            };
            let Some(name) = unknown_name(&why).map(str::to_string) else { break Err(why) };
            let lenient = !name.contains('~') && (t.vanished.1.remove(&name.to_lowercase()) || young);
            let at = edits.iter().position(|e| matches!(e, model::Edit::Remove(n) | model::Edit::Rename { from: n, .. } if n.as_str() == name));
            match at {
                Some(i) if lenient && tries < LENIENT_TRIES => {
                    tries += 1;
                    if let model::Edit::Rename { entry, .. } = edits.remove(i) {
                        edits.insert(i, model::Edit::Upsert(entry));
                    }
                }
                _ => break Err(format!("unknown name {name} ({records} records)")),
            }
        };
        let apply_ms = perf::elapsed_ms(t0);
        match res {
            Ok(a) => {
                for e in &edits {
                    if let model::Edit::Upsert(entry) = e {
                        t.vanished.1.remove(&entry.name.to_lowercase());
                    }
                }
                t.vanished.1.extend(gone.iter().map(|n| n.to_lowercase()));
                // A folder that leaves the listing, or is renamed, loses its size (and its walk);
                // `apply_edits` carries no size across a rename.
                for e in &edits {
                    let gone = match e {
                        model::Edit::Remove(n) | model::Edit::Rename { from: n, .. } => n.as_str(),
                        model::Edit::Upsert(_) => continue,
                    };
                    if let Some(p) = t.sizes.pending.remove(gone) {
                        p.cancel.store(true, Ordering::Relaxed);
                    }
                    t.sizes.forget(gone);
                }
                t.model.set_grid_cursor(a.cursor);
                if !active {
                    t.cursor = a.cursor;
                }
                eprintln!(
                    "[watch] pane={pane} tab={tab_id} applied: +{} -{} ~{} (stat {stat_ms:.1} ms, apply {apply_ms:.1} ms)",
                    a.added, a.removed, a.changed
                );
                st.watch_edits += 1;
                if active {
                    set_current_row(&app, pane, usize::try_from(a.cursor).ok());
                    // The cursor keeps its row number, so a deleted file's row may now hold the
                    // next file with no `changed` firing: the preview asks again (it reads only a
                    // file other than the one it shows).
                    preview::kick();
                    size_selection(st, pane);
                    update_status(st, &app, pane);
                    st.strip_dirty[pane] = true;
                    resolve_view(st, &app, pane);
                }
            }
            Err(why) => {
                let outcome = fall_back(st, pane, tab_id, &mut again);
                eprintln!("[watch] pane={pane} tab={tab_id} fallback: {why}: {outcome}");
            }
        }
        Some(again)
    });
    for pane in again.unwrap_or_default() {
        refresh(pane);
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
/// or sized by Alt+Shift+Return in this listing); forget the others. Their old size stays shown until
/// the new one lands.
fn rewalk_due(pane: usize) {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let State { panes, sizer, .. } = &mut *st;
        let t = panes[pane].active_mut();
        let selected: std::collections::HashSet<String> = t.model.selected_folders().into_iter().collect();
        for n in t.sizes.due(Instant::now()) {
            if t.sizes.pending.contains_key(&n) {
                continue; // being walked already (Alt+Shift+Return, a selection)
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
        if t.search.is_none() && fs::archive::inside(&t.path) {
            let n = t.model.selected_folders().len();
            if n > 0 {
                eprintln!("[archive] refused: folder sizes for {n} selected folder(s) in {} (inside an archive)", t.path.display());
            }
        }
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

/// Alt+Shift+Return in the list: size every folder of the listing (at most `ALL_CAP`), the ones nearest
/// the cursor first.
fn size_all(st: &mut State, app: &App, pane: usize) {
    let cursor = usize::try_from(current_row(app, pane)).unwrap_or(0);
    if !st.tab(pane).sizable() {
        let why = if st.is_results(pane) {
            "search results have no folder sizes"
        } else if in_archive(st, pane) {
            eprintln!("[archive] refused: folder sizes (inside an archive)");
            "folders inside an archive are not sized"
        } else {
            "drives show their free space"
        };
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
    eprintln!("[size] pane={pane} Alt+Shift+Return: {} of {} folder(s) to size", rows.len(), m);
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
    let (col, asc, show_hidden) = (model.sort_col.get(), model.sort_asc.get(), model.show());
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
    // A restored tab's cursor row, once its listing is complete (P6-M3).
    if t.timing.done_ms.is_some() {
        if let Some(name) = t.restore_cursor.take() {
            restore.cursor = Some(name);
        }
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
    model.set_landed_pass(pass);
    model.set_grid_cursor(cursor.map_or(-1, |c| c as i32));
    if !active {
        t.cursor = cursor.map_or(-1, |c| c as i32);
        return;
    }
    set_current_row(&app, pane, cursor);
    // As after row edits: the same row number may hold another file now.
    preview::kick();
    if let (Some(y), Some(_)) = (scroll, cursor) {
        app.invoke_set_scroll(pane as i32, y);
        eprintln!("[refresh] pane={pane} scroll restored to {y:.0} (now {:.0})", app.invoke_get_scroll(pane as i32));
    }
    // A restored tab's saved scroll, on the pass that placed its cursor (P6-M3).
    if t.restore_cursor.is_none() && t.timing.done_ms.is_some() {
        if let Some(y) = t.restore_scroll.take() {
            app.invoke_set_scroll(pane as i32, y);
            eprintln!("[tab] pane={pane} tab={id} restored: cursor {cursor:?}, scroll {y:.0} (now {:.0})", app.invoke_get_scroll(pane as i32));
        }
    }
    // Once per pass swap, never per frame: bring an operation's cursor row (its grid row in the
    // grid) into view.
    if let (true, Some(row)) = (reveal, cursor) {
        let y = app.invoke_get_scroll(pane as i32);
        let (line, line_px) = line_of(st, pane, row);
        if let Some(to) = model::reveal_row(y, st.view[pane].1, line, line_px) {
            app.invoke_set_scroll(pane as i32, to);
            eprintln!("[refresh] pane={pane} cursor row {row} scrolled into view: {y:.0} -> {to:.0}");
        }
    }
    size_selection(st, pane);
    update_status(st, &app, pane);
    st.strip_dirty[pane] = true;
    resolve_view(st, &app, pane);
}

/// What a slot of a pane's grid strip was composed from (`compose_grid_row`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct GridSlot {
    row: usize,
    icon_px: u32,
    dirty: bool,
}

/// The line of the pane's list that holds entry `row` (the row itself, or its grid row in the
/// grid) and the line's height.
fn line_of(st: &State, pane: usize, row: usize) -> (usize, f32) {
    if st.grid[pane] {
        (row / st.model(pane).grid_n(), st.tile[pane].h)
    } else {
        (row, list_geom(st).row_px)
    }
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
    let scale = app.window().scale_factor();
    let geom = icons::StripGeom::new(scale, list_geom(st));
    // The grid: visible GRID rows of the tile's height, `n` entries each, icons at the tile's size;
    // the strip covers whole grid rows. The details list: its rows, as before.
    let grid = st.grid[pane];
    let n = if grid { model.grid_n() } else { 1 };
    let grid_geom = icons::GridGeom::new(scale, n, st.tile[pane]);
    let (lines, line_px, size_px) = if grid {
        (model.grid_len(), grid_geom.tile.h, grid_geom.icon_px)
    } else {
        (model.len(), geom.list.row_px, geom.icon_px)
    };
    let Some(visible) = icons::visible_range(y, h, lines, line_px) else {
        hide_strip(st, app, pane);
        return;
    };
    let (first, last) = (visible.0 * n, ((visible.1 + 1) * n - 1).min(model.len() - 1));
    let t0 = Instant::now();
    let generation = model.generation.get();
    let (lo, hi) = (first.saturating_sub(ICON_MARGIN_ROWS), last + ICON_MARGIN_ROWS);
    // With thumbnails on, the workers learn which rows this resolve asks for (once per resolve),
    // and the rows on screen are asked for last, so the workers (newest first) render them first.
    // The grid asks for its thumbnails at the tiles' icon size (P4-M2), through the same path.
    let thumbs = st.config.thumbnails;
    let wanted = if thumbs {
        st.resolve_seq[pane] += 1;
        st.icon_views.publish(pane, st.resolve_seq[pane], (lo, hi.min(model.len() - 1)));
        st.resolve_seq[pane]
    } else {
        0
    };
    let (folder, tx) = (&t.path, &st.icon_tx);
    let mut sent = 0;
    let mut ask = |i: usize, e: &model::Entry| {
        let path = if e.kind == Kind::Drive { PathBuf::from(e.name.as_str()) } else { folder.join(e.name.as_str()) };
        let overlay = icons::wants_overlay(e.kind, e.attrs, &e.name);
        let thumbnail = thumbs && e.icon_key.starts_with("thumb:");
        let (row, wanted) = if thumbnail { (i, wanted) } else { (if icons::is_look_key(&e.icon_key) { i } else { 0 }, 0) };
        let mark = icons::key_mark(&e.icon_key);
        let _ = tx.send(icons::Request { generation, pane, key: e.icon_key.clone(), path, size_px, overlay, thumbnail, row, wanted, mark });
        sent += 1;
    };
    let changed = if thumbs {
        let mut changed = if first > lo { model.resolve_icons(lo, first - 1, size_px, &mut ask) } else { Vec::new() };
        changed.extend(model.resolve_icons(last + 1, hi, size_px, &mut ask));
        changed.extend(model.resolve_icons(first, last, size_px, &mut ask));
        changed
    } else {
        model.resolve_icons(lo, hi, size_px, ask)
    };
    if grid {
        // An icon landed for an entry: the slot holding its grid row is recomposed.
        let slots = st.grid_slots[pane].len().max(1);
        for &i in &changed {
            if let Some(Some(s)) = st.grid_slots[pane].get_mut((i / n) % slots) {
                if s.row == i / n {
                    s.dirty = true;
                }
            }
        }
    } else if let Some(c) = st.strip[pane] {
        if changed.iter().any(|&i| (c.first..=c.last).contains(&i)) {
            st.strip_dirty[pane] = true;
        }
    }
    let c = &mut st.icon_counts;
    c.requests += sent;
    c.resolves += 1;
    c.resolve_ns += t0.elapsed().as_nanos() as u64;
    if grid {
        compose_grid_rows(st, pane, visible, h, &grid_geom);
    } else if icons::needs_recompose(st.strip[pane], visible, lines, size_px, st.strip_dirty[pane]) {
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
    let strip = icons::compose(&bufs, &model.cut_flags(first, last), geom);
    st.icon_counts.composes += 1;
    st.icon_counts.compose_ns += t0.elapsed().as_nanos() as u64;
    let (w, h) = (strip.width() as f32 / geom.scale, strip.height() as f32 / geom.scale);
    let top = first as f32 * geom.list.row_px;
    app.invoke_set_strip(pane as i32, Image::from_rgba8_premultiplied(strip), 6.0, top, w, h, true);
    st.strip[pane] = Some(icons::Composed { first, last, icon_px: geom.icon_px });
    st.strip_dirty[pane] = false;
}

/// `compose_strip` for the large-icon grid: the visible grid rows plus the grid margin, each as
/// its own image of `cols` icons at the tiles' positions, in the ring of slots (`State::grid_slots`;
/// `visible_h` sizes the ring: a viewport of grid rows, a partial row at each end, the margin). A
/// slot already holding its row at this icon size, with no icon changed in it, is left alone, so a
/// scroll of one grid row composes and uploads one row (the P3-M5 trace recomposed and uploaded
/// the whole strip, about a third of the grid's busy, ab14).
fn compose_grid_rows(st: &mut State, pane: usize, visible: (usize, usize), visible_h: f32, geom: &icons::GridGeom) {
    let model = st.panes[pane].active().model.clone();
    let (first, last) = icons::grid_strip_range(visible, model.grid_len());
    let slots = (visible_h.max(0.0) / geom.tile.h).ceil() as usize + 1 + 2 * icons::GRID_MARGIN_ROWS;
    let reset = st.grid_slots[pane].len() != slots || st.strip_dirty[pane];
    if reset {
        st.grid_slots[pane] = vec![None; slots];
        if st.strip_rows[pane].row_count() != slots {
            st.strip_rows[pane].set_vec(vec![StripRow::default(); slots]);
        }
        st.strip_dirty[pane] = false;
    }
    let n = geom.cols;
    for g in first..=last {
        let k = g % slots;
        let want = GridSlot { row: g, icon_px: geom.icon_px, dirty: false };
        if st.grid_slots[pane][k] == Some(want) {
            continue;
        }
        let t0 = Instant::now();
        // A thumbnail's picture is framed (P6-M2); a type icon is not.
        let (bufs, frames): (Vec<_>, Vec<_>) = {
            let mut table = st.icons.borrow_mut();
            model.icon_indices(g * n, (g + 1) * n - 1).into_iter().map(|i| (table.fitted(i, geom.icon_px), table.picture(i, geom.icon_px))).unzip()
        };
        let row = icons::compose_grid(&bufs, &model.cut_flags(g * n, (g + 1) * n - 1), &frames, geom, 1);
        st.icon_counts.composes += 1;
        st.icon_counts.compose_ns += t0.elapsed().as_nanos() as u64;
        let (w, h) = (row.width() as f32 / geom.scale, row.height() as f32 / geom.scale);
        st.strip_rows[pane].set_row_data(
            k,
            StripRow { img: Image::from_rgba8_premultiplied(row), top: g as f32 * geom.tile.h, w, h, shown: true },
        );
        st.grid_slots[pane][k] = Some(want);
    }
    // After a reset (a filter pass, a new listing), a slot the rows above did not fill again still
    // holds its old image: hide it, or a grid that got shorter shows its old rows' icons under its
    // tiles (P7-M4 console check). Only after a reset, so a scroll pays nothing for it.
    if reset {
        for k in 0..slots {
            if st.grid_slots[pane][k].is_none() && st.strip_rows[pane].row_data(k).is_some_and(|r| r.shown) {
                st.strip_rows[pane].set_row_data(k, StripRow::default());
            }
        }
    }
}

/// Hide the pane's icon strip (empty list, or a navigation under way): the details strip, and the
/// grid's slots (their images are dropped; the next resolve fills the ring again).
fn hide_strip(st: &mut State, app: &App, pane: usize) {
    if st.strip[pane].take().is_some() {
        app.invoke_set_strip(pane as i32, Image::default(), 0.0, 0.0, 0.0, 0.0, false);
    }
    if st.strip_rows[pane].row_count() > 0 {
        st.strip_rows[pane].set_vec(Vec::new());
        st.grid_slots[pane].clear();
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

/// The pane's list now fits `n` tiles to a grid row (its width changed: split, resize; Slint
/// reports it while the pane shows the grid): the active tab's grid re-maps and the strip is
/// recomposed. Retried like `on_viewport`.
fn on_grid_cols(pane: usize, n: usize) {
    let busy = STATE.with(|s| {
        let Ok(mut guard) = s.try_borrow_mut() else { return true };
        let Some(st) = guard.as_mut() else { return false };
        st.grid_n[pane] = n.max(1);
        if st.grid[pane] && st.model(pane).set_grid_n(n) {
            eprintln!("[grid] pane={pane} {n} tiles to a row, {} grid rows", st.model(pane).grid_len());
            st.strip_dirty[pane] = true;
            if let Some(app) = st.app.upgrade() {
                resolve_view(st, &app, pane);
            }
        }
        false
    });
    if busy {
        slint::Timer::single_shot(Duration::ZERO, move || on_grid_cols(pane, n));
    }
}

/// The pane's cursor entry changed (keys, clicks or Rust) while it shows the grid: its tile shows it.
fn on_grid_cursor(pane: usize, row: i32) {
    let busy = STATE.with(|s| {
        let Ok(mut guard) = s.try_borrow_mut() else { return true };
        let Some(st) = guard.as_mut() else { return false };
        st.model(pane).set_grid_cursor(row);
        false
    });
    if busy {
        slint::Timer::single_shot(Duration::ZERO, move || on_grid_cursor(pane, row));
    }
}

/// The pane's view now (details, or the grid at its tile size).
fn view_of(st: &State, pane: usize) -> settings::View {
    match (st.grid[pane], st.tile[pane] == icons::TileGeom::MEDIUM) {
        (false, _) => settings::View::Details,
        (true, false) => settings::View::Large,
        (true, true) => settings::View::Medium,
    }
}

/// Ctrl+Shift+1 (details), Ctrl+Shift+2 (large icons) or Ctrl+Shift+3 (medium icons) on the active
/// pane, or the settings dialog. The host swaps its list component for the pane (the rows, cursor
/// and selection are the host's and stay); the tiles to a grid row are what the list fits at the
/// new tile width (the host's `cols` follows `medium` as soon as it is set); the scroll is
/// re-aimed so the first visible line still starts at the same entry; the strip is hidden and
/// recomposed at the new icon size (icons are kept per size, P3-M5); the setting is saved. The
/// Slint whole-row snap on a rebuilt list shows here (phase-4-plan.md, open questions).
fn set_view(pane: usize, view: settings::View) {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        // A sizes view is always a list (P13-M3): the pane's view is set for its other tabs.
        if st.tab(pane).sizeview.is_some() {
            st.config.set_view(pane, view);
            schedule_save(st);
            return eprintln!("[view] pane={pane} {} kept for the pane's other tabs: the sizes view stays a list", view.label());
        }
        let old = view_of(st, pane);
        if old == view {
            return eprintln!("[view] pane={pane} already {}", view.label());
        }
        let model = st.panes[pane].active().model.clone();
        let y = st.view[pane].0;
        let list_px = list_geom(st).row_px;
        let (old_px, old_n) = if old.is_grid() { (st.tile[pane].h, model.grid_n()) } else { (list_px, 1) };
        let first_entry = ((-y).max(0.0) / old_px).floor() as usize * old_n;
        let grid = view.is_grid();
        if grid {
            st.tile[pane] = icons::TileGeom::of(view.is_medium());
        }
        if pane == 0 { app.set_left_medium(st.tile[pane] == icons::TileGeom::MEDIUM) } else { app.set_right_medium(st.tile[pane] == icons::TileGeom::MEDIUM) }
        let cols = (if pane == 0 { app.get_left_grid_cols() } else { app.get_right_grid_cols() }).max(1) as usize;
        st.grid[pane] = grid;
        st.grid_n[pane] = cols;
        st.config.set_view(pane, view);
        model.set_grid_n(cols);
        model.set_grid_cursor(current_row(&app, pane));
        if pane == 0 { app.set_left_grid(grid) } else { app.set_right_grid(grid) }
        let (new_px, new_n) = if grid { (st.tile[pane].h, cols) } else { (list_px, 1) };
        let to = if first_entry == 0 { 0.0 } else { -((first_entry / new_n) as f32 * new_px) };
        app.invoke_set_scroll(pane as i32, to);
        hide_strip(st, &app, pane);
        st.strip_dirty[pane] = true;
        // The icon size changed: every tab of the pane asks for its icons again at the new size.
        for t in st.panes[pane].iter() {
            t.model.forget_icons();
        }
        eprintln!(
            "[view] pane={pane} {} -> {} ({cols} tiles to a row, {} grid rows), scroll {y:.0} -> {to:.0} (entry {first_entry} first)",
            old.label(),
            view.label(),
            model.grid_len()
        );
        resolve_view(st, &app, pane);
        schedule_save(st);
    });
}

/// The pane shows the grid when its view is one and its active tab is no sizes view (P13-M3: that
/// is always a list): called as a tab comes to the front; switches the pane's list as `set_view`
/// does, without touching the setting.
fn sync_grid(st: &mut State, app: &App, pane: usize) {
    let view = st.config.view(pane);
    let want = view.is_grid() && st.tab(pane).sizeview.is_none();
    if st.grid[pane] == want {
        return;
    }
    if want {
        st.tile[pane] = icons::TileGeom::of(view.is_medium());
    }
    if pane == 0 { app.set_left_medium(st.tile[pane] == icons::TileGeom::MEDIUM) } else { app.set_right_medium(st.tile[pane] == icons::TileGeom::MEDIUM) }
    let cols = (if pane == 0 { app.get_left_grid_cols() } else { app.get_right_grid_cols() }).max(1) as usize;
    st.grid[pane] = want;
    st.grid_n[pane] = cols;
    if pane == 0 { app.set_left_grid(want) } else { app.set_right_grid(want) }
    hide_strip(st, app, pane);
    st.strip_dirty[pane] = true;
    for t in st.panes[pane].iter() {
        t.model.forget_icons();
    }
    eprintln!("[view] pane={pane} shows {}", if want { view.label() } else { "a list (the sizes view)" });
}

/// Take every icon reply the worker has sent, intern the images, then re-resolve both panes'
/// visible rows once (one walk over a screenful per pane, however many replies arrived).
fn drain_icons() {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let mut any = false;
        let mut looked = Vec::new();
        {
            let mut table = st.icons.borrow_mut();
            while let Ok(r) = st.icon_rx.try_recv() {
                if icons::is_drag_key(&r.key) {
                    // A drag image's icon: kept for the press that asked for it, never in the table.
                    // A thumbnail answers as a picture (P9-M4), an icon as pixels.
                    let got = match r.outcome {
                        icons::Outcome::Pixels(p) => Some((p, false)),
                        icons::Outcome::Picture(p, _) => Some((p, true)),
                        _ => None,
                    };
                    if let (Some((path, slot)), Some(got)) = (&mut st.drag_icon, got) {
                        if r.key == icons::drag_key(path) {
                            *slot = Some(got);
                        }
                    }
                    continue;
                }
                if let icons::Outcome::Looked(mark) = r.outcome {
                    looked.push((r.pane, r.row, r.key.clone(), mark));
                }
                table.complete(r.key, r.size_px, r.outcome);
                st.icon_counts.replies += 1;
                any = true;
            }
        }
        // A lookup's entry takes its marked key (a placeholder until that key's icon lands; a
        // reply whose entry is gone, refreshed or navigated away, finds nothing).
        for (pane, row, key, mark) in looked {
            if let Some(to) = icons::looked(&key, mark) {
                st.panes[pane].active().model.relabel(row, &key, to.into());
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
        // Thumbnails the cache had not got yet are asked for again once their wait runs out.
        let due = st.icons.borrow_mut().retry_due(Instant::now());
        if due > 0 {
            eprintln!("[icons] {due} thumbnail(s) asked again");
            if let Some(app) = st.app.upgrade() {
                resolve_view(st, &app, 0);
                resolve_view(st, &app, 1);
            }
        }
        let c = &mut st.icon_counts;
        let rendered = st.icon_stats.rendered.load(Ordering::Relaxed);
        let render_ns = st.icon_stats.render_ns.load(Ordering::Relaxed);
        let overlays = st.icon_stats.overlays.load(Ordering::Relaxed);
        let slow = st.icon_stats.slow.load(Ordering::Relaxed);
        let thumbs = st.icon_stats.thumbs.load(Ordering::Relaxed);
        let thumb_ns = st.icon_stats.thumb_ns.load(Ordering::Relaxed);
        let dropped = st.icon_stats.dropped.load(Ordering::Relaxed);
        let cloud = st.icon_stats.cloud.load(Ordering::Relaxed);
        let (req0, rep0, ren0, ns0, comp0, ov0, slow0, th0, tns0, drop0, cloud0) = c.reported;
        if c.requests == req0 && c.replies == rep0 && c.composes == comp0 {
            return;
        }
        let n = rendered - ren0;
        let t = thumbs - th0;
        eprintln!(
            "[icons] +{} requests +{} replies (total {} / {}), table {} images, worker {n} rendered avg {:.2} ms/icon, overlays {}, slow {}, thumbs {t} avg {:.2} ms dropped {}, cloud {}, resolve {} calls avg {:.1} us, compose {} strips avg {:.1} us",
            c.requests - req0,
            c.replies - rep0,
            c.requests,
            c.replies,
            st.icons.borrow().len(),
            if n > 0 { (render_ns - ns0) as f64 / n as f64 / 1e6 } else { 0.0 },
            overlays - ov0,
            slow - slow0,
            if t > 0 { (thumb_ns - tns0) as f64 / t as f64 / 1e6 } else { 0.0 },
            dropped - drop0,
            cloud - cloud0,
            c.resolves,
            if c.resolves > 0 { c.resolve_ns as f64 / c.resolves as f64 / 1e3 } else { 0.0 },
            c.composes,
            if c.composes > 0 { c.compose_ns as f64 / c.composes as f64 / 1e3 } else { 0.0 },
        );
        c.reported = (c.requests, c.replies, rendered, render_ns, c.composes, overlays, slow, thumbs, thumb_ns, dropped, cloud);
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
                st.watch.set_hwnd(st.hwnd);
                // The title bar's colour, the moment the window exists (P10-M3).
                set_title_bar(st.hwnd, title_bar_mode(&st.config.theme));
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

/// The pane's active tab shows a level of an archive.
fn in_archive(st: &State, pane: usize) -> bool {
    fs::archive::inside(&st.tab(pane).path)
}

/// The pane's active tab shows a level of an archive TwinFM does not write (P7-M3: every format
/// but zip, which Windows' handler refuses to write).
fn in_readonly_archive(st: &State, pane: usize) -> bool {
    let path = &st.tab(pane).path;
    fs::archive::inside(path) && !fs::archive::writable(path)
}

/// Why a write into the pane's read-only archive is refused.
fn read_only_why(st: &State, pane: usize) -> String {
    if fs::archive::nested(&st.tab(pane).path) {
        return fs::archive::NESTED_READ_ONLY.into();
    }
    fs::archive::split(&st.tab(pane).path).map_or_else(String::new, |(a, _)| fileops::archive::read_only_why(&a))
}

/// A write into, or an edit inside, an archive that is refused: say so in the pane and the log,
/// before any shell call.
fn refuse_archive(st: &mut State, app: &App, pane: usize, what: &str, why: &str) {
    eprintln!("[archive] refused: {what} ({why})");
    note(st, app, pane, format!("{what}: {why}"));
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

thread_local! {
    /// While file jobs are in flight: twice a second, the shell's dialogs hidden behind our window
    /// come to the front (`raise_job_dialogs`).
    static DIALOG_TIMER: slint::Timer = slint::Timer::default();
    /// Dialogs already logged as left behind while another program was in front (once each).
    static DIALOGS_LEFT: RefCell<HashSet<isize>> = RefCell::new(HashSet::new());
}

/// The shell's dialogs of a running job (a password prompt, a conflict) come to the front when
/// TwinFM is the foreground program: the shell makes them on threads of its own and leaves them
/// behind our window (P7-M3's console check). Stops itself when no job is in flight.
fn raise_job_dialogs() {
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow};
    if with_state(|st| st.jobs.is_empty()) {
        DIALOG_TIMER.with(|t| t.stop());
        DIALOGS_LEFT.with(|d| d.borrow_mut().clear());
        return;
    }
    // SAFETY: plain queries; only our own windows are raised, and only while one of ours is in front.
    let ours = unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
        pid == GetCurrentProcessId()
    };
    if !ours {
        for (h, title) in fileops::hidden_dialogs() {
            if DIALOGS_LEFT.with(|d| d.borrow_mut().insert(h)) {
                eprintln!("[fileops] dialog \"{title}\" is open while another program is in front: left there");
            }
        }
        return;
    }
    for (h, title) in fileops::hidden_dialogs() {
        // SAFETY: a window of this process that EnumWindows just reported.
        let ok = unsafe { SetForegroundWindow(h as _) };
        eprintln!("[fileops] dialog \"{title}\" was behind the window: brought to the front ({})", if ok != 0 { "ok" } else { "refused" });
    }
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
    // Writes into archives (P7-M3): a zip's go to TwinFM's writer, the rest are refused here.
    if let Err(why) = fileops::archive::classify(&job) {
        let what = fileops::running_text(&job);
        refuse_archive(st, app, pane, what.trim_end_matches("..."), &why);
        return None;
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
    st.jobs.insert(id, JobInfo { pane, tab, fallback: None, replay: None, running: running.clone() });
    DIALOG_TIMER.with(|t| {
        if !t.running() {
            t.start(slint::TimerMode::Repeated, Duration::from_millis(500), raise_job_dialogs);
        }
    });
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
        // A cut out of an archive would paste as a move out of it.
        "cx" if with_state(|st| in_readonly_archive(st, pane)) => {
            return with_state(|st| {
                if let Some(app) = st.app.upgrade() {
                    let why = read_only_why(st, pane);
                    refuse_archive(st, &app, pane, "cut", &why);
                }
            });
        }
        "cc" | "cx" => return clipboard_put(pane, cmd == "cx"),
        // Alt+Down (P11-M3): the history menu, under the pane's path bar.
        "histmenu" => {
            let Some(at) = with_state(|st| {
                let app = st.app.upgrade()?;
                let l = window_layout(&app);
                Some((dnd::pane_x(&l, pane).0 + 8.0, dnd::list_top(&l, pane) - 34.0))
            }) else {
                return;
            };
            return open_history_menu(pane, at.0, at.1);
        }
        "cv" => return clipboard_paste(pane),
        // The sizes view's Right and Left (P13-M3): the cursor row's folder opened or closed in place.
        "sizeopen" | "sizeclose" => {
            let row = with_state(|st| st.app.upgrade().and_then(|app| usize::try_from(current_row(&app, pane)).ok()));
            if let Some(row) = row {
                size_toggle(pane, row, Some(cmd == "sizeopen"));
            }
            return;
        }
        "undo" => return undo(pane),
        "redo" => return redo(pane),
        "properties" => return properties(pane),
        "cancelqueued" => {
            if band_cancel(pane) {
                return;
            }
            // Explorer's Escape clears the selection; ours stops a search and cancels queued
            // jobs first (P6-M3), then cancels a cut (P7-M1), and clears the selection only when
            // it did none of those.
            if !cancel_queued(pane) && !cancel_cut(pane) {
                clear_selection(pane);
            }
            return;
        }
        _ => {}
    }
    let edit = with_state(|st| {
        let app = st.app.upgrade()?;
        let folder = st.tab(pane).path.clone();
        let at_drives = is_this_pc(&folder.to_string_lossy());
        match cmd {
            "sizeall" => size_all(st, &app, pane),
            "newfolder" | "newfile" if at_drives => note(st, &app, pane, "This PC lists drives; open a folder first".into()),
            "newfolder" if st.is_results(pane) => note(st, &app, pane, "search results: open a folder to make a folder in it".into()),
            "newfile" if st.is_results(pane) => note(st, &app, pane, "search results: open a folder to make a file in it".into()),
            // The sizes view's rows are under folders the editor does not stand over (P13-M3).
            "rename" if st.tab(pane).sizeview.is_some() => note(st, &app, pane, "the sizes view: rename in a folder tab (Return opens a folder in one)".into()),
            "newfolder" | "newfile" if in_readonly_archive(st, pane) => {
                let why = read_only_why(st, pane);
                refuse_archive(st, &app, pane, if cmd == "newfolder" { "new folder" } else { "new file" }, &why);
            }
            "newfolder" => return Some((app, 2, SharedString::from("New folder"))),
            // Explorer's name for a new empty file (P6-M3); the shell numbers it on a collision.
            "newfile" => return Some((app, 3, SharedString::from(fileops::NEW_FILE_NAME))),
            "rename" | "delete" | "sdelete" if in_readonly_archive(st, pane) => {
                let what = match cmd {
                    "sdelete" => "permanent delete",
                    c => c,
                };
                let why = read_only_why(st, pane);
                refuse_archive(st, &app, pane, what, &why);
            }
            "move" if in_readonly_archive(st, pane) => {
                let why = read_only_why(st, pane);
                refuse_archive(st, &app, pane, "move out of an archive", &why);
            }
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
                if st.is_results(1 - pane) {
                    note(st, &app, pane, "the other pane shows search results; open a folder there first".into());
                    return None;
                }
                if is_this_pc(&dest.to_string_lossy()) {
                    note(st, &app, pane, "the other pane lists drives; open a folder there first".into());
                    return None;
                }
                if in_readonly_archive(st, 1 - pane) {
                    let why = read_only_why(st, 1 - pane);
                    refuse_archive(st, &app, pane, &format!("{cmd} into {}", dest.display()), &why);
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

/// The inline name editor's Return: rename the row, or create the folder or the file.
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
            3 => Job::NewFile { parent: folder, name: name.into() },
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
/// Refused with a note while an operation is in flight, or when the items are no longer where it
/// left them; the undo job runs through the worker like any other (the soak fence included), and
/// its own record, when it completes, goes onto the redo stack.
fn undo(pane: usize) {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        if replay_busy(st, &app, pane) {
            return;
        }
        let Some(rec) = st.undo.pop() else {
            eprintln!("[undo] nothing to undo");
            return note(st, &app, pane, "Nothing to undo".into());
        };
        let Some(u) = rec.undo else {
            eprintln!("[undo] \"{}\" cannot be undone", rec.text);
            return note(st, &app, pane, rec.cannot.into());
        };
        if let Err(why) = fileops::undo::check(&u) {
            eprintln!("[undo] \"{}\" refused: {why}", rec.text);
            return note(st, &app, pane, why);
        }
        eprintln!("[undo] \"{}\" with {:?}", rec.text, u.job);
        let job = u.job.clone();
        submit_replay(st, &app, pane, job, (Replay::Undo, fileops::undo::Record { undo: Some(u), text: rec.text, cannot: rec.cannot }));
    });
}

/// Ctrl+Y in a pane's list: redo the newest undone operation (the job that reverses its undo),
/// refused like an undo while an operation is in flight or when the items are no longer where
/// the undo left them.
fn redo(pane: usize) {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        if replay_busy(st, &app, pane) {
            return;
        }
        let Some(rec) = st.redo.pop() else {
            eprintln!("[undo] nothing to redo");
            return note(st, &app, pane, "Nothing to redo".into());
        };
        // A redo record can have no reverse: the undo of a copy, a new folder or a restore is a
        // recycle, and a volume with no Recycle Bin reports no `$R` paths for it.
        let Some(u) = rec.undo else {
            eprintln!("[undo] redo of \"{}\" cannot be done", rec.text);
            return note(st, &app, pane, format!("Cannot redo: {}", rec.text));
        };
        if let Err(why) = fileops::undo::check(&u) {
            eprintln!("[undo] redo \"{}\" refused: {why}", rec.text);
            return note(st, &app, pane, why.replacen("Cannot undo", "Cannot redo", 1));
        }
        eprintln!("[undo] redo \"{}\" with {:?}", rec.text, u.job);
        let job = u.job.clone();
        submit_replay(st, &app, pane, job, (Replay::Redo, fileops::undo::Record { undo: Some(u), text: rec.text, cannot: rec.cannot }));
    });
}

/// Ctrl+Z and Ctrl+Y wait for the operations in flight (the record they would pop may belong to
/// one of them, or be changed by it): true, with a note, when any is.
fn replay_busy(st: &mut State, app: &App, pane: usize) -> bool {
    if st.jobs.is_empty() {
        return false;
    }
    eprintln!("[undo] busy: {} operation(s) in flight", st.jobs.len());
    note(st, app, pane, "Wait for the running operation to finish".into());
    true
}

/// Submit an undo or redo job; one that takes rows out of this pane's folder (a recycle, a move
/// back) gets that tab's cursor fallback. A job the worker refuses puts its record back.
fn submit_replay(st: &mut State, app: &App, pane: usize, job: Job, replay: (Replay, fileops::undo::Record)) {
    let names = removed_names(&job, &st.tab(pane).path);
    let fallback = if names.is_empty() { None } else { removal_fallback(st, app, pane, &names) };
    match submit_removing(st, app, pane, job, fallback) {
        Some(id) => {
            if let Some(j) = st.jobs.get_mut(&id) {
                j.replay = Some(replay);
            }
        }
        None => keep_record(st, replay),
    }
}

/// An undo or redo that did nothing: its record goes back onto the stack it came from.
fn keep_record(st: &mut State, (replay, rec): (Replay, fileops::undo::Record)) {
    eprintln!("[undo] record kept: \"{}\"", rec.text);
    fileops::undo::push(if replay == Replay::Undo { &mut st.undo } else { &mut st.redo }, rec);
}

/// The names `job` takes out of `folder`: the sources of a delete or a move (a move-as's items)
/// that lie in it.
fn removed_names(job: &Job, folder: &Path) -> Vec<SharedString> {
    let sources: Vec<&Path> = match job {
        Job::Delete { sources, .. } | Job::Move { sources, .. } => sources.iter().map(PathBuf::as_path).collect(),
        Job::MoveAs { items, .. } => items.iter().map(|(p, _)| p.as_path()).collect(),
        _ => Vec::new(),
    };
    sources
        .into_iter()
        .filter(|s| s.parent().is_some_and(|p| same_dir(p, folder)))
        .filter_map(|s| Some(s.file_name()?.to_string_lossy().as_ref().into()))
        .collect()
}

/// Where a finished job's record goes (None when it left none): a new job's onto the undo stack,
/// clearing the redo stack (as Explorer does) when it `changed` something (an item came out ok);
/// an undo job's onto the redo stack; a redo job's back onto the undo stack. Returns how many
/// redo records were cleared.
fn after_job(
    replay: Option<Replay>,
    changed: bool,
    record: Option<fileops::undo::Record>,
    undo: &mut Vec<fileops::undo::Record>,
    redo: &mut Vec<fileops::undo::Record>,
) -> usize {
    let cleared = if replay.is_none() && changed { std::mem::take(redo).len() } else { 0 };
    if let Some(r) = record {
        fileops::undo::push(if replay == Some(Replay::Undo) { redo } else { undo }, r);
    }
    cleared
}

/// The note an undo or redo job leaves: "Undid: <the record's text>" when it was `done`, else
/// "Undo: <its own status text>" (and "Redid:" / "Redo:" for a redo).
fn replay_text(replay: Replay, done: bool, orig: &str, status: &str) -> String {
    match (replay, done) {
        (Replay::Undo, true) => format!("Undid: {orig}"),
        (Replay::Undo, false) => format!("Undo: {status}"),
        (Replay::Redo, true) => format!("Redid: {orig}"),
        (Replay::Redo, false) => format!("Redo: {status}"),
    }
}

/// Escape in a pane's list: cancel every queued job that has not started (each reports
/// `Cancelled` through `job_done`); the running one goes on.
fn cancel_queued(pane: usize) -> bool {
    with_state(|st| {
        // Escape in a search tab's list also stops its walk; the results so far stay.
        let t = st.tab(pane);
        let mut stopped = false;
        if let Some(s) = t.search.as_ref().filter(|_| t.timing.done_ms.is_none() && t.timing.error.is_none()) {
            s.cancel(fs::enumerate::SEARCH_ESCAPE);
            eprintln!("[search] pane={pane} Escape: stopping the search");
            stopped = true;
        }
        let queued = st.jobs.len().saturating_sub(1);
        if queued == 0 {
            return stopped;
        }
        st.ops.cancel_queued(st.next_job);
        eprintln!("[fileops] Escape: cancelling {queued} queued operation(s) up to id {}", st.next_job);
        if let Some(app) = st.app.upgrade() {
            note(st, &app, pane, format!("Cancelling {queued} queued operation{}", if queued == 1 { "" } else { "s" }));
        }
        true
    })
}

/// Escape over a cut (P7-M1): the cut marks go, and the clipboard is emptied while it still holds
/// TwinFM's cut, so a later paste cannot move the files. False when no cut is marked.
fn cancel_cut(pane: usize) -> bool {
    let Some((hwnd, ours)) = with_state(|st| {
        st.cut_source?;
        clear_cuts(st);
        Some((st.hwnd, clipboard::sequence() == st.clip_seq))
    }) else {
        return false;
    };
    // Outside the state borrow: the clipboard's own change notice arrives as a message.
    let emptied = ours && hwnd != 0 && match clipboard::clear(hwnd) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("[clipboard] pane={pane} Escape: emptying the clipboard failed: {e}");
            false
        }
    };
    eprintln!(
        "[clipboard] pane={pane} Escape: cut cancelled, marks cleared, {}",
        if emptied { "clipboard emptied" } else { "clipboard left (not TwinFM's cut any more)" }
    );
    with_state(|st| {
        if let Some(app) = st.app.upgrade() {
            note(st, &app, pane, "Cut cancelled".into());
        }
    });
    true
}

/// Escape with nothing to cancel: the pane's selection is cleared (the cursor stays).
fn clear_selection(pane: usize) {
    with_state(|st| {
        let model = st.model(pane);
        if model.selected_count() == 0 {
            return;
        }
        model.select(SelOp::Clear, 0);
        eprintln!("[select] pane={pane} Escape: selection cleared");
        size_selection(st, pane);
        if let Some(app) = st.app.upgrade() {
            update_status(st, &app, pane);
        }
    });
}

/// Clear the cut marks in every tab (the clipboard no longer holds a cut), and recompose the
/// icon strip that dimmed them.
fn clear_cuts(st: &mut State) {
    if let Some(p) = take_cuts(st) {
        recompose_strip(st, p);
    }
}

/// `clear_cuts` without the recompose: the pane whose active tab showed the cut, if any.
fn take_cuts(st: &mut State) -> Option<usize> {
    let shown = st.cut_source.take().filter(|&(p, tab)| st.is_active(p, tab)).map(|(p, _)| p);
    for tabs in &st.panes {
        for t in tabs.iter() {
            t.model.set_cut(Vec::new());
        }
    }
    shown
}

/// A pane's cut marks changed: its icon strip (the cut icons dim in the compose) is composed
/// again, once.
fn recompose_strip(st: &mut State, pane: usize) {
    st.strip_dirty[pane] = true;
    if let Some(app) = st.app.upgrade() {
        resolve_view(st, &app, pane);
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
                let mut dimmed: Vec<usize> = take_cuts(st).into_iter().collect();
                if cut {
                    st.model(pane).set_cut(names);
                    st.cut_source = Some((pane, st.tab(pane).id));
                    if !dimmed.contains(&pane) {
                        dimmed.push(pane);
                    }
                }
                for p in dimmed {
                    recompose_strip(st, p);
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
        if st.is_results(pane) {
            note(st, &app, pane, "search results: paste into a folder".into());
            return None;
        }
        if in_readonly_archive(st, pane) {
            let why = read_only_why(st, pane);
            refuse_archive(st, &app, pane, "paste", &why);
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
fn mark_background_stale(st: &mut State, watched_too: bool) {
    for tabs in st.panes.iter_mut() {
        let active = tabs.active_index();
        for (i, t) in tabs.iter_mut().enumerate() {
            if i != active && (watched_too || !t.watched) {
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
        let info = st.jobs.remove(&d.id).unwrap_or(JobInfo { pane: 0, tab: 0, fallback: None, replay: None, running: String::new() });
        let (pane, tab) = (info.pane, info.tab);
        if st.progress.is_some_and(|(id, _)| id == d.id) {
            st.progress = None;
        }
        let text = fileops::status_text(&d.job, &d.outcome, &d.results);
        let done = d.outcome == Outcome::Done;
        let changed = d.results.iter().any(fileops::ItemResult::ok);
        // An undo or redo job's record keeps the text of the record it replayed.
        let (text, record) = match &info.replay {
            None => {
                let r = fileops::undo::record_for(&d.job, &d.outcome, &d.results, &text);
                // A copy or move that replaced a file is not undone (P11-M3).
                (text, fileops::undo::forbid_if_replaced(r, &d.existed, &d.results))
            }
            Some((r, orig)) => {
                let shown = replay_text(*r, done, &orig.text, &text);
                let what = if *r == Replay::Undo { "done" } else { "redo done" };
                eprintln!("[undo] {what} id={} {:?}: {shown}", d.id, d.outcome);
                (shown, fileops::undo::record_for(&d.job, &d.outcome, &d.results, &orig.text))
            }
        };
        let replay = info.replay.as_ref().map(|(r, _)| *r);
        if let Some(r) = &record {
            let stack = if replay == Some(Replay::Undo) { "redo recorded" } else { "recorded" };
            eprintln!("[undo] {stack} \"{}\": {:?}", r.text, r.undo.as_ref().map(|u| &u.job));
        }
        let cleared = after_job(replay, changed, record, &mut st.undo, &mut st.redo);
        if cleared > 0 {
            eprintln!("[undo] redo cleared ({cleared})");
        }
        // An undo or redo that changed nothing (cancelled while queued, refused, failed on its
        // first item) can be tried again.
        if let Some(r) = info.replay.filter(|_| !done && !changed) {
            keep_record(st, r);
        }
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
                    Job::Rename { new_name: n, .. } | Job::NewFolder { name: n, .. } | Job::NewFile { name: n, .. } => {
                        let real = fileops::real_name(&d.job, &d.results).unwrap_or_else(|| n.clone());
                        // A search tab's rows are named by their path under its root (P11-M2).
                        let real = match &d.job {
                            Job::Rename { source, .. } if t.search.is_some() => search_row_name(&t.path, source, &real),
                            _ => real,
                        };
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
        // A watched background tab gets the job's changes from its watch.
        mark_background_stale(st, false);
        // A sizes view takes what went off its totals instead of walking again (P13-M3).
        let sizes = sizes_removed(st, &d.job, &d.results);
        let split = st.app.upgrade().is_some_and(|a| a.get_split());
        let panes: Vec<usize> = if split { vec![0, 1] } else { vec![0] }.into_iter().filter(|&p| !sizes[p]).collect();
        // The refresh below covers what the job did: its own records, arriving after it, are
        // not refreshed again.
        for &p in &panes {
            let t = st.tab_mut(p);
            let names = explained_names(&t.path, &d.job, &d.results);
            t.explained = (!names.is_empty()).then(|| (names, Instant::now()));
        }
        panes
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

/// The running job's progress (on the UI thread, from the worker's wake-up, at most four a
/// second): its tab's status bar shows the percentage. A report for a job already done is dropped.
fn job_progress(p: fileops::Progress) {
    with_state(|st| {
        let Some(pane) = st.jobs.get(&p.id).map(|j| j.pane) else { return };
        st.progress = Some((p.id, p.percent));
        eprintln!("[fileops] progress id={} {}%", p.id, p.percent);
        if let Some(app) = st.app.upgrade() {
            update_status(st, &app, pane);
        }
    });
}

/// What `context_menu` needs after the menu closes, taken out of the state before it opens.
struct MenuContext {
    hwnd: isize,
    /// The directory "Open in other pane" and "Open in terminal" use.
    dir: PathBuf,
    /// The full paths "Copy path" puts on the clipboard.
    paths: Vec<PathBuf>,
}

/// The context menu: for the selection (`row` >= 0; a right press has already applied the
/// right-click selection rule) or the folder background (`row` -1, or nothing to act on), at
/// window position (`x`, `y`) in logical px. The Windows 11 style menu (P7-M2), or with Shift held
/// (`extended`) the shell's classic menu at once, with its extended verbs, as in Explorer.
fn context_menu(pane: usize, row: i32, x: f32, y: f32, extended: bool) {
    if shellmenu::is_open() {
        eprintln!("[menu] pane={pane} a menu is already open; ignored");
        return;
    }
    if extended {
        classic_menu(pane, row, x, y, true);
    } else {
        open_winmenu(pane, row, x, y);
    }
}

/// The shell's classic context menu (`IContextMenu`), for Shift+right-click and "Show more
/// options". Arguments as `context_menu`'s.
fn classic_menu(pane: usize, row: i32, x: f32, y: f32, extended: bool) {
    // Take everything the menu needs out of the state, then drop the borrow: the menu runs a
    // modal message loop (see shellmenu), and no RefCell borrow may be held across it.
    let Some((req, ctx)) = menu_request(pane, row, x, y, extended) else { return };
    let picked = shellmenu::show(&req);
    match picked {
        Err(e) => with_state(|st| {
            if let Some(app) = st.app.upgrade() {
                note(st, &app, pane, format!("context menu: {e}"));
            }
        }),
        Ok(picked) => menu_picked(pane, picked, &req, ctx),
    }
}

/// What the context menu needs, taken out of the state: the request for `shellmenu::show` and
/// what its own items need afterwards. None (with a note) when there is no menu to show.
fn menu_request(pane: usize, row: i32, x: f32, y: f32, extended: bool) -> Option<(shellmenu::Request, MenuContext)> {
    with_state(|st| {
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
        if targets.is_empty() && st.is_results(pane) {
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
    })
}

/// The driver's `menuverb <pane> <verb>`: as if the shell command with that canonical verb had
/// been picked in the menu for the cursor row's selection (the driver never presses Return in a
/// shell menu): routed to TwinFM's command, or run on the verb worker by its verb.
pub fn script_menu_verb(pane: usize, verb: &str) {
    let Some(row) = with_state(|st| st.app.upgrade().map(|app| current_row(&app, pane))) else { return };
    let Some((req, ctx)) = menu_request(pane, row, 0.0, 0.0, false) else { return };
    eprintln!("[auto] menuverb pane={pane} \"{verb}\" for {} item(s)", req.names.len());
    if let Some(path) = verb.strip_prefix(TEST_TRY_VERB) {
        // The dry test verb (P12-M6): the command at that label path in the classic menu, handed
        // on as a pick there hands it, resolved on the verb worker and never invoked.
        match shellmenu::find_shown(&req, path) {
            Ok((verb, offset, label)) => {
                eprintln!("[auto] menuverb try: \"{label}\" at offset {offset} in the menu shown, verb \"{verb}\"");
                let request = shellmenu::VerbRequest {
                    folder: req.folder.clone(),
                    names: req.names.clone(),
                    verb,
                    offset,
                    label,
                    x: req.x,
                    y: req.y,
                    extended: req.extended,
                    find: false,
                    default_item: false,
                };
                queue_verb(pane, request, Some(TestVerb::Try));
            }
            Err(e) => eprintln!("[auto] menuverb try: {e}"),
        }
        return;
    }
    let picked = match shellmenu::route(verb) {
        Some("rename") => shellmenu::Picked::Rename,
        Some(cmd) => shellmenu::Picked::Route(cmd),
        None => shellmenu::Picked::Shell { verb: verb.to_string(), offset: 0, label: verb.to_string() },
    };
    menu_picked(pane, picked, &req, ctx);
}

/// What the menu's pick does, once the menu has closed (and for the driver's `menuverb`).
fn menu_picked(pane: usize, picked: shellmenu::Picked, req: &shellmenu::Request, ctx: MenuContext) {
    match picked {
        shellmenu::Picked::Nothing => {}
        shellmenu::Picked::Rename => command(pane, "rename"),
        shellmenu::Picked::Own(item) => own_item(pane, item, ctx),
        shellmenu::Picked::Route(cmd) => {
            // The menu's Delete with Shift held deletes permanently, as the key does.
            // SAFETY: a plain key state query on the UI thread.
            let shift = unsafe { GetKeyState(VK_SHIFT as i32) } < 0;
            command(pane, if cmd == "delete" && shift { "sdelete" } else { cmd });
        }
        shellmenu::Picked::Shell { verb, offset, label } => {
            let sleep_ms = verb.strip_prefix(TEST_SLEEP_VERB).and_then(|ms| ms.parse().ok());
            let request = shellmenu::VerbRequest {
                folder: req.folder.clone(),
                names: req.names.clone(),
                verb,
                offset,
                label,
                x: req.x,
                y: req.y,
                extended: req.extended,
                find: false,
                default_item: false,
            };
            queue_verb(pane, request, sleep_ms.map(TestVerb::Sleep));
        }
    }
}

/// Queue a shell command on the verb worker (a test verb when `test` is given).
fn queue_verb(pane: usize, request: shellmenu::VerbRequest, test: Option<TestVerb>) {
    with_state(|st| {
        st.next_verb += 1;
        let id = st.next_verb;
        eprintln!(
            "[menu] verb \"{}\" (\"{}\") queued on the verb worker{}, id {id}",
            request.verb,
            request.label,
            if request.find {
                " (found there by its label)"
            } else if request.default_item {
                " (the default command)"
            } else if matches!(test, Some(TestVerb::Try)) {
                " (the test verb: resolved, not invoked)"
            } else {
                ""
            }
        );
        let work = match test {
            Some(TestVerb::Sleep(ms)) => verbs::Work::Sleep(request, ms),
            Some(TestVerb::Try) => verbs::Work::Try(request),
            None => verbs::Work::Verb(request),
        };
        if !st.verbs.submit(verbs::Job { id, pane, work }) {
            if let Some(app) = st.app.upgrade() {
                note(st, &app, pane, "context menu: the verb worker has stopped".into());
            }
        }
    });
}

/// Alt+Return in a list, and the Windows 11 menu's Properties (P7-M2): the shell's Properties for
/// the cursor row's selection, or the folder's own with no cursor row, on the verb worker.
fn properties(pane: usize) {
    let Some(row) = with_state(|st| st.app.upgrade().map(|app| current_row(&app, pane))) else { return };
    let Some((req, _)) = menu_request(pane, row, 0.0, 0.0, false) else { return };
    queue_verb(pane, verb_request(&req, "properties", "Properties", false), None);
}

/// A verb job for the items of `req`, by its canonical verb, or (`find`) by its label in the menu
/// the worker builds.
fn verb_request(req: &shellmenu::Request, verb: &str, label: &str, find: bool) -> shellmenu::VerbRequest {
    shellmenu::VerbRequest {
        folder: req.folder.clone(),
        names: req.names.clone(),
        verb: verb.into(),
        offset: 0,
        label: label.into(),
        x: req.x,
        y: req.y,
        extended: false,
        find,
        default_item: false,
    }
}

/// The driver's test verb: `menuverb <pane> sleep:<ms>` sleeps on the verb worker.
const TEST_SLEEP_VERB: &str = "sleep:";
/// The driver's dry test verb (P12-M6): `menuverb <pane> try:<label path>` resolves a classic-menu
/// command on the verb worker as a pick would, and invokes nothing.
const TEST_TRY_VERB: &str = "try:";

/// The driver's test verbs on the verb worker.
enum TestVerb {
    Sleep(u64),
    Try,
}

/// A verb job finished on the verb worker (on the UI thread).
fn verb_done(d: verbs::Done) {
    let msg = match (&d.how, &d.error) {
        (_, Some(e)) => Some(format!("\"{}\": {e}", d.label)),
        (Some(shellmenu::Invoke::Refuse(why)), _) => Some(format!("\"{}\" was not run: {why}", d.label)),
        _ if d.hr < 0 && d.hr != fileops::HR_CANCELLED => Some(format!("\"{}\" failed: 0x{:08X}", d.label, d.hr as u32)),
        _ => None,
    };
    if d.how.is_none() && d.error.is_none() {
        return; // the test verb: nothing ran
    }
    // The shell may have created, renamed or deleted entries in either folder.
    let split = with_state(|st| {
        if let (Some(m), Some(app)) = (msg, st.app.upgrade()) {
            note(st, &app, d.pane, m);
        }
        mark_background_stale(st, false);
        st.app.upgrade().is_some_and(|a| a.get_split())
    });
    refresh(0);
    if split {
        refresh(1);
    }
}

/// Shift+F10 or the Menu key in a pane's list: the menu for the cursor row's selection, under
/// that row (already scrolled into view), or the background menu when there is no cursor row.
fn context_key(pane: usize, row: i32, list_x: f32, list_y: f32, content_y: f32, visible_h: f32) {
    let (len, g, grid) = with_state(|st| {
        (st.model(pane).len(), list_geom(st), st.grid[pane].then(|| (st.model(pane).grid_n(), st.tile[pane])))
    });
    match usize::try_from(row).ok().filter(|&r| r < len) {
        Some(r) => {
            let (x, y) = match grid {
                Some((cols, tile)) => shellmenu::tile_menu_point(list_x, list_y, content_y, visible_h, r, cols, tile),
                None => shellmenu::row_menu_point(list_x, list_y, content_y, visible_h, r, g),
            };
            context_menu(pane, row, x, y, false);
        }
        None => context_menu(pane, -1, list_x + g.name_x(), list_y, false),
    }
}

/// How long a new Windows 11 menu waits for the verb worker's answer (the packaged commands and
/// the Open with apps) before it shows without them; an answer after that rebuilds it.
const WINMENU_WAIT_MS: u64 = 250;

/// The Windows 11 style menu while it is open (P7-M2): what it was opened for, the verb worker's
/// answer, and what it shows.
struct WinMenu {
    seq: u64,
    pane: usize,
    /// The row it was opened for (-1: the background) and the point, window logical px: "Show more
    /// options" opens the classic menu for the same row at the same point.
    row: i32,
    at: (f32, f32),
    req: shellmenu::Request,
    ctx: MenuContext,
    wctx: winmenu::Ctx,
    /// A query is with the verb worker.
    waiting: bool,
    answer: explorercmd::Answer,
    opened: Instant,
    shown: bool,
    tools: Vec<winmenu::Tool>,
    items: Vec<winmenu::Entry>,
    placed: winmenu::Placed,
    hl: i32,
    sub: Option<SubMenu>,
    /// The history menu (P11-M3): its rows and each one's steps from the current place, in place
    /// of the selection's items.
    history: Option<(Vec<winmenu::Entry>, Vec<isize>)>,
    /// The toolbar's View or More menu (P13-M2): its rows and its items.
    tool: Option<(Vec<winmenu::Entry>, Vec<toolbar::MenuItem>)>,
}

/// An open submenu: whose it is, its rows, where it is, its highlight, and whether the keyboard
/// is in it.
struct SubMenu {
    of: winmenu::Cmd,
    items: Vec<winmenu::Entry>,
    placed: winmenu::Placed,
    hl: i32,
    keys: bool,
}

/// What the manifests' item types call one entry of the selection.
fn menu_target(e: &model::Entry) -> explorercmd::Target {
    if e.kind == Kind::Drive {
        explorercmd::Target::Drive
    } else if e.is_dir() {
        explorercmd::Target::Dir
    } else {
        let ext = Path::new(e.name.as_str()).extension().map(|x| format!(".{}", x.to_string_lossy().to_lowercase())).unwrap_or_default();
        explorercmd::Target::File(ext)
    }
}

/// Open the Windows 11 style menu: its own items at once, the packaged commands and Open with apps
/// asked of the verb worker, shown when it answers or after `WINMENU_WAIT_MS`.
fn open_winmenu(pane: usize, row: i32, x: f32, y: f32) {
    winmenu_close("another menu opens");
    let Some((req, ctx)) = menu_request(pane, row, x, y, false) else { return };
    let opened = with_state(|st| {
        let app = st.app.upgrade()?;
        let place = tool_place(st, pane);
        let entries = if req.names.is_empty() { Vec::new() } else { st.model(pane).targets(current_row(&app, pane)) };
        let targets: Vec<explorercmd::Target> = if entries.is_empty() { vec![explorercmd::Target::Background] } else { entries.iter().map(menu_target).collect() };
        let what = if entries.is_empty() {
            winmenu::What::Background
        } else {
            let count = |f: fn(&explorercmd::Target) -> bool| targets.iter().filter(|t| f(t)).count();
            winmenu::What::Items {
                files: count(|t| matches!(t, explorercmd::Target::File(_))),
                dirs: count(|t| matches!(t, explorercmd::Target::Dir)),
                drives: count(|t| matches!(t, explorercmd::Target::Drive)),
            }
        };
        let exts: Vec<Option<&str>> = targets.iter().map(|t| if let explorercmd::Target::File(x) = t { Some(x.as_str()) } else { None }).collect();
        let same_type = !entries.is_empty() && winmenu::same_type(&exts);
        let open_with = match exts.first() {
            Some(Some(ext)) if same_type && !ext.is_empty() && matches!(place, toolbar::Place::Folder | toolbar::Place::Search) => Some(ext.to_string()),
            _ => None,
        };
        st.winmenu_seq += 1;
        let seq = st.winmenu_seq;
        // Nothing to ask inside an archive (its paths are no files to an app) or on This PC's own
        // background.
        let ask = !matches!(place, toolbar::Place::Archive | toolbar::Place::Zip) && !(place == toolbar::Place::ThisPc && entries.is_empty());
        let waiting = ask && {
            let q = explorercmd::Query {
                seq,
                folder: req.folder.clone(),
                names: req.names.clone(),
                targets,
                icon_px: (16.0 * app.window().scale_factor()).round() as u32,
                open_with,
            };
            st.next_verb += 1;
            st.verbs.submit(verbs::Job { id: st.next_verb, pane, work: verbs::Work::Query(q) })
        };
        eprintln!(
            "[winmenu] pane={pane} seq={seq} {} for {} item(s) in {}: {}",
            if entries.is_empty() { "background menu" } else { "item menu" },
            entries.len(),
            req.folder.display(),
            if waiting { "query queued on the verb worker" } else { "no query" }
        );
        let wctx = winmenu::Ctx { place, what, clipboard_files: st.clip_files, same_type };
        let placed = winmenu::Placed { x, y, h: 0.0, scrolls: false, left: false, up: false };
        st.winmenu = Some(WinMenu {
            seq,
            pane,
            row,
            at: (x, y),
            req,
            ctx,
            wctx,
            waiting,
            answer: explorercmd::Answer { seq, ..Default::default() },
            opened: Instant::now(),
            shown: false,
            tools: Vec::new(),
            items: Vec::new(),
            placed,
            hl: -1,
            sub: None,
            history: None,
            tool: None,
        });
        Some((seq, waiting))
    });
    match opened {
        Some((seq, true)) => slint::Timer::single_shot(Duration::from_millis(WINMENU_WAIT_MS), move || {
            if with_state(|st| st.winmenu.as_ref().is_some_and(|m| m.seq == seq && !m.shown)) {
                show_winmenu(&format!("the worker had not answered after {WINMENU_WAIT_MS} ms"));
            }
        }),
        Some((_, false)) => show_winmenu("no query"),
        None => {}
    }
}

/// The history menu (P11-M3): Alt+Down, or a right click on the path bar. The pane's places,
/// forward ones first, the current one checked, back ones after it, in the Windows 11 menu at
/// (`x`, `y`), window logical px; a pick goes that many steps (`history_jump`).
fn open_history_menu(pane: usize, x: f32, y: f32) {
    winmenu_close("another menu opens");
    let Some((req, ctx)) = menu_request(pane, -1, x, y, false) else { return };
    with_state(|st| {
        let t = st.tab(pane);
        let here = t.place();
        let list = t.history.list(&here);
        let labels: Vec<(String, bool)> = list.iter().map(|(steps, p)| (p.label(), *steps == 0)).collect();
        eprintln!("[nav] pane={pane} history menu: {} entries ({} back, {} forward)", list.len(), t.history.back_len(), t.history.forward_len());
        let wctx = winmenu::Ctx { place: tool_place(st, pane), what: winmenu::What::Background, clipboard_files: st.clip_files, same_type: false };
        st.winmenu_seq += 1;
        st.winmenu = Some(WinMenu {
            seq: st.winmenu_seq,
            pane,
            row: -1,
            at: (x, y),
            req,
            ctx,
            wctx,
            waiting: false,
            answer: explorercmd::Answer { seq: st.winmenu_seq, ..Default::default() },
            opened: Instant::now(),
            shown: false,
            tools: Vec::new(),
            items: Vec::new(),
            placed: winmenu::Placed { x, y, h: 0.0, scrolls: false, left: false, up: false },
            hl: -1,
            sub: None,
            history: Some((winmenu::history_items(&labels), list.iter().map(|(s, _)| *s).collect())),
            tool: None,
        });
    });
    show_winmenu("history");
}

/// The verb worker's answer for a menu (on the UI thread): shown now, or rebuilt when the menu
/// showed without it.
fn winmenu_answer(a: explorercmd::Answer) {
    let seq = a.seq;
    let was = with_state(|st| {
        let m = st.winmenu.as_mut().filter(|m| m.seq == seq)?;
        m.waiting = false;
        m.answer = a;
        Some(m.shown)
    });
    match was {
        Some(false) => show_winmenu("the worker answered"),
        Some(true) => show_winmenu("rebuilt with the worker's answer"),
        None => eprintln!("[winmenu] seq={seq} answered after its menu closed: dropped"),
    }
}

/// The Slint rows for menu entries, with their tops.
fn menu_entries(items: &[winmenu::Entry]) -> (ModelRc<MenuEntry>, f32) {
    let (ys, h) = winmenu::rows_y(items);
    let rows: Vec<MenuEntry> = items
        .iter()
        .zip(ys)
        .map(|(e, y)| MenuEntry {
            id: e.cmd.map_or(0, winmenu::Cmd::id),
            text: e.text.as_str().into(),
            glyph: e.glyph.into(),
            icon: e.icon.clone().map(Image::from_rgba8).unwrap_or_default(),
            has_icon: e.icon.is_some(),
            key: e.key.into(),
            sub: e.sub,
            enabled: e.enabled,
            sep: e.is_sep(),
            y,
        })
        .collect();
    (ModelRc::from(Rc::new(VecModel::from(rows))), h)
}

/// The window's size in logical px.
fn window_logical(app: &App) -> (f32, f32) {
    let (size, scale) = (app.window().size(), app.window().scale_factor());
    (size.width as f32 / scale, size.height as f32 / scale)
}

/// Build the open menu's rows from what it knows now, place it and show it (or update it when it
/// is shown already).
fn show_winmenu(why: &str) {
    let first = with_state(|st| {
        let app = st.app.upgrade()?;
        let m = st.winmenu.as_mut()?;
        (m.tools, m.items) = match (&m.history, &m.tool) {
            (Some((items, _)), _) | (_, Some((items, _))) => (Vec::new(), items.clone()),
            _ => (winmenu::tools(&m.wctx), winmenu::items(&m.wctx, &m.answer.commands)),
        };
        let win = window_logical(&app);
        let h = winmenu::menu_height(!m.tools.is_empty(), &m.items);
        m.placed = winmenu::place(m.at, winmenu::MENU_W, h, win);
        m.hl = -1;
        m.sub = None;
        let tools: Vec<MenuTool> = m.tools.iter().map(|t| MenuTool { id: t.cmd.id(), glyph: t.glyph.into(), text: t.text.into() }).collect();
        let (rows, list_h) = menu_entries(&m.items);
        app.set_wm_tools(ModelRc::from(Rc::new(VecModel::from(tools))));
        app.set_wm_items(rows);
        app.set_wm_list_h(list_h);
        app.set_wm_x(m.placed.x);
        app.set_wm_y(m.placed.y);
        app.set_wm_h(m.placed.h);
        app.set_wm_scroll(0.0);
        app.set_wm_hl(-1);
        app.set_wm_sub_shown(false);
        let p = &m.placed;
        eprintln!(
            "[winmenu] pane={} seq={} shown after {:.0} ms ({why}): {}",
            m.pane,
            m.seq,
            perf::elapsed_ms(m.opened),
            winmenu::describe(&m.tools, &m.items)
        );
        eprintln!(
            "[winmenu] placed at ({:.0}, {:.0}) {:.0} x {:.0} for ({:.0}, {:.0}) in a {:.0} x {:.0} window: {}, {}{}",
            p.x,
            p.y,
            winmenu::MENU_W,
            p.h,
            m.at.0,
            m.at.1,
            win.0,
            win.1,
            if p.left { "leftward" } else { "rightward" },
            if p.up { "upward" } else { "downward" },
            if p.scrolls { ", its rows scroll" } else { "" }
        );
        let first = !m.shown;
        m.shown = true;
        first.then_some(app)
    });
    if let Some(app) = first {
        app.invoke_show_wm();
    }
}

/// Close the Windows 11 menu (when one is open), give the keys back to its pane's list, and hand
/// back what it was for.
fn winmenu_close(why: &str) -> Option<WinMenu> {
    let (m, app) = with_state(|st| Some((st.winmenu.take()?, st.app.upgrade()?)))?;
    eprintln!("[winmenu] pane={} seq={} closed ({why})", m.pane, m.seq);
    app.invoke_close_wm();
    app.invoke_focus_pane(m.pane as i32);
    Some(m)
}

/// The item with this id opens a submenu (Open with, Compress to, a packaged command with its own
/// commands).
fn opens_sub(items: &[winmenu::Entry], cmd: winmenu::Cmd) -> bool {
    items.iter().any(|e| e.cmd == Some(cmd) && e.sub)
}

/// Open the submenu of `cmd` beside its item (`keys`: from the keyboard, which moves into it).
fn winmenu_open_sub(cmd: winmenu::Cmd, keys: bool) {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let Some(m) = st.winmenu.as_mut() else { return };
        if let Some(s) = m.sub.as_mut().filter(|s| s.of == cmd) {
            if keys && !s.keys {
                s.keys = true;
                s.hl = winmenu::next_hl(&s.items, -1, true);
                app.set_wm_sub_hl(s.hl);
            }
            return;
        }
        let Some(i) = m.items.iter().position(|e| e.cmd == Some(cmd)) else { return };
        let several = matches!(m.wctx.what, winmenu::What::Items { files, .. } if files > 1);
        let items = winmenu::sub_items(cmd, &m.answer.commands, &m.answer.apps, several);
        let (ys, _) = winmenu::rows_y(&m.items);
        let list_top = m.placed.y + winmenu::PAD + if m.tools.is_empty() { 0.0 } else { winmenu::TOOLS_H + winmenu::SEP_H };
        let item_y = list_top + ys[i] + app.get_wm_scroll();
        let h = 2.0 * winmenu::PAD + winmenu::rows_y(&items).1;
        let placed = winmenu::place_sub(&m.placed, winmenu::MENU_W, item_y, winmenu::SUB_W, h, window_logical(&app));
        let hl = if keys { winmenu::next_hl(&items, -1, true) } else { -1 };
        let (rows, list_h) = menu_entries(&items);
        app.set_wm_sub_items(rows);
        app.set_wm_sub_list_h(list_h);
        app.set_wm_sub_x(placed.x);
        app.set_wm_sub_y(placed.y);
        app.set_wm_sub_h(placed.h);
        app.set_wm_sub_scroll(0.0);
        app.set_wm_sub_hl(hl);
        app.set_wm_sub_shown(true);
        m.hl = i as i32;
        app.set_wm_hl(m.hl);
        eprintln!("[winmenu] submenu \"{}\" at ({:.0}, {:.0}): {}", m.items[i].text, placed.x, placed.y, winmenu::describe(&[], &items));
        m.sub = Some(SubMenu { of: cmd, items, placed, hl, keys });
    });
}

fn winmenu_close_sub() {
    with_state(|st| {
        let (Some(app), Some(m)) = (st.app.upgrade(), st.winmenu.as_mut()) else { return };
        if m.sub.take().is_some() {
            app.set_wm_sub_shown(false);
        }
    });
}

/// The pointer entered an item: it takes the highlight; one with a submenu opens it, any other
/// in the menu closes an open submenu.
fn winmenu_hover(id: i32, in_sub: bool) {
    let Some(cmd) = winmenu::Cmd::from_id(id) else { return };
    let opens = with_state(|st| {
        let app = st.app.upgrade()?;
        let m = st.winmenu.as_mut()?;
        if in_sub {
            if let Some(s) = m.sub.as_mut() {
                s.hl = s.items.iter().position(|e| e.cmd == Some(cmd)).map_or(-1, |i| i as i32);
                app.set_wm_sub_hl(s.hl);
            }
            return None;
        }
        m.hl = m.items.iter().position(|e| e.cmd == Some(cmd)).map_or(-1, |i| i as i32);
        app.set_wm_hl(m.hl);
        Some(opens_sub(&m.items, cmd))
    });
    match opens {
        Some(true) => winmenu_open_sub(cmd, false),
        Some(false) => winmenu_close_sub(),
        None => {}
    }
}

/// A key in the menu (the popup's FocusScope, and the driver's `menukey`): Down, Up, Home and End
/// move the highlight (in the submenu when the keyboard is in it), Right opens the highlighted
/// item's submenu, Left and Escape leave a submenu, Escape closes the menu, Return picks, and
/// Shift+F10 ("more") opens the classic menu.
fn winmenu_key(name: &str) {
    enum Then {
        Nothing,
        Pick(i32),
        Open(winmenu::Cmd),
        CloseSub,
        Close,
    }
    let then = with_state(|st| {
        let Some(app) = st.app.upgrade() else { return Then::Nothing };
        let Some(m) = st.winmenu.as_mut() else { return Then::Nothing };
        let in_sub = m.sub.as_ref().is_some_and(|s| s.keys);
        let (items, hl) = match (in_sub, m.sub.as_mut()) {
            (true, Some(s)) => (&s.items, &mut s.hl),
            _ => (&m.items, &mut m.hl),
        };
        let moved = match name {
            "down" => Some(winmenu::next_hl(items, *hl, true)),
            "up" => Some(winmenu::next_hl(items, *hl, false)),
            "home" => Some(winmenu::next_hl(items, -1, true)),
            "end" => Some(winmenu::next_hl(items, -1, false)),
            _ => None,
        };
        if let Some(n) = moved {
            *hl = n;
            let (ys, _) = winmenu::rows_y(items);
            let top = usize::try_from(n).ok().and_then(|i| ys.get(i).copied()).unwrap_or(0.0);
            if in_sub {
                let view = m.sub.as_ref().map_or(0.0, |s| s.placed.h) - 2.0 * winmenu::PAD;
                app.set_wm_sub_hl(n);
                app.set_wm_sub_scroll(winmenu::scroll_into_view(app.get_wm_sub_scroll(), top, winmenu::ITEM_H, view));
            } else {
                let tools = if m.tools.is_empty() { 0.0 } else { winmenu::TOOLS_H + winmenu::SEP_H };
                let view = m.placed.h - 2.0 * winmenu::PAD - tools;
                app.set_wm_hl(n);
                app.set_wm_scroll(winmenu::scroll_into_view(app.get_wm_scroll(), top, winmenu::ITEM_H, view));
            }
            return Then::Nothing;
        }
        let current = usize::try_from(*hl).ok().and_then(|i| items.get(i)).and_then(|e| e.cmd.map(|c| (c, e.sub)));
        match name {
            "right" => match current {
                Some((c, true)) if !in_sub => Then::Open(c),
                _ => Then::Nothing,
            },
            "left" if in_sub => Then::CloseSub,
            "escape" if m.sub.is_some() => Then::CloseSub,
            "escape" => Then::Close,
            "return" => match current {
                Some((c, true)) if !in_sub => Then::Open(c),
                Some((c, _)) => Then::Pick(c.id()),
                None => Then::Nothing,
            },
            "more" => Then::Pick(winmenu::Cmd::More.id()),
            _ => Then::Nothing,
        }
    });
    match then {
        Then::Nothing => {}
        Then::Pick(id) => winmenu_picked(id),
        Then::Open(c) => winmenu_open_sub(c, true),
        Then::CloseSub => winmenu_close_sub(),
        Then::Close => {
            winmenu_close("Escape");
        }
    }
}

/// An item or icon-row button was picked: an item with a submenu opens it; any other closes the
/// menu and runs its command, the one its key, its toolbar button or its P7-M1 route calls.
pub fn winmenu_picked(id: i32) {
    let Some(cmd) = winmenu::Cmd::from_id(id) else { return };
    let (sub, enabled) = with_state(|st| {
        st.winmenu.as_ref().map_or((false, false), |m| {
            let all = m.items.iter().chain(m.sub.iter().flat_map(|s| s.items.iter()));
            let tool = m.tools.iter().any(|t| t.cmd == cmd);
            (opens_sub(&m.items, cmd), tool || all.clone().any(|e| e.cmd == Some(cmd) && e.enabled))
        })
    });
    if sub {
        return winmenu_open_sub(cmd, false);
    }
    if !enabled {
        return;
    }
    let Some(m) = winmenu_close(&format!("picked {cmd:?}")) else { return };
    // On the next turn of the event loop: closing the popup gives the keys back to the list after
    // this callback, which would take them from a name editor the pick opens (New folder, Rename).
    slint::Timer::single_shot(Duration::ZERO, move || run_winmenu_pick(m, cmd));
}

/// Compress to > 7z file / TAR file (P7-M3): a file job that makes the archive beside the items
/// (a search result's own folder), named after the first item.
fn compress(pane: usize, folder: &Path, names: &[String], format: fs::archivewrite::Format) {
    let sources: Vec<PathBuf> = names.iter().map(|n| folder.join(n)).collect();
    let Some(dest_dir) = sources.first().and_then(|s| s.parent()).map(Path::to_path_buf) else { return };
    with_state(|st| {
        if let Some(app) = st.app.upgrade() {
            submit(st, &app, pane, Job::Compress { sources, dest_dir, format });
        }
    });
}

/// Run a picked item, after its menu closed.
fn run_winmenu_pick(m: WinMenu, cmd: winmenu::Cmd) {
    use winmenu::Cmd;
    let pane = m.pane;
    eprintln!("[winmenu] pane={pane} runs {cmd:?}");
    match cmd {
        Cmd::History(i) => {
            if let Some(steps) = m.history.as_ref().and_then(|(_, s)| s.get(i)) {
                history_jump(pane, *steps);
            }
        }
        Cmd::Tool(i) => {
            if let Some(item) = m.tool.as_ref().and_then(|(_, items)| items.get(i)) {
                run_tool(pane, item);
            }
        }
        Cmd::Delete => {
            // Shift held deletes permanently, as the key and the classic menu's Delete do.
            // SAFETY: a plain key state query on the UI thread.
            let shift = unsafe { GetKeyState(VK_SHIFT as i32) } < 0;
            command(pane, if shift { "sdelete" } else { "delete" });
        }
        c if c.key_command().is_some() => command(pane, c.key_command().unwrap_or_default()),
        Cmd::Open => {
            let rows = with_state(|st| {
                let app = st.app.upgrade()?;
                let sel = st.model(pane).selected_set();
                let cursor = usize::try_from(current_row(&app, pane)).ok();
                Some(if sel.is_empty() { cursor.into_iter().collect() } else { sel.into_iter().collect() })
            });
            if let Some(rows) = rows {
                open_rows(pane, rows);
            }
        }
        Cmd::OpenOther => own_item(pane, shellmenu::OwnItem::OpenOther, m.ctx),
        Cmd::Terminal => own_item(pane, shellmenu::OwnItem::Terminal, m.ctx),
        Cmd::CopyPath => own_item(pane, shellmenu::OwnItem::CopyPath, m.ctx),
        Cmd::Bookmark => menu_bookmarks(pane, &m.req.folder, &m.req.names),
        // A sizes tab for each selected folder or drive, or for the folder shown (P13-M3).
        Cmd::Sizes => {
            if m.req.names.is_empty() {
                open_sizeview(pane, m.req.folder.clone());
            }
            for n in &m.req.names {
                let p = m.req.folder.join(n);
                if p.is_dir() {
                    open_sizeview(pane, p);
                }
            }
        }
        Cmd::Share => queue_verb(pane, verb_request(&m.req, "", "Share", true), None),
        Cmd::Zip => queue_verb(pane, verb_request(&m.req, "", ZIP_LABEL, true), None),
        Cmd::SevenZ | Cmd::Tar => {
            let format = if cmd == Cmd::SevenZ { fs::archivewrite::Format::SevenZ } else { fs::archivewrite::Format::Tar };
            compress(pane, &m.req.folder, &m.req.names, format);
        }
        Cmd::ChooseApp => queue_verb(pane, verb_request(&m.req, "openas", "Choose another app", false), None),
        Cmd::More => {
            // After a frame, so the window is drawn without the menu before the classic one's
            // modal loop starts (nothing renders while it runs).
            let (row, (x, y)) = (m.row, m.at);
            slint::Timer::single_shot(Duration::from_millis(30), move || classic_menu(pane, row, x, y, false));
        }
        Cmd::Packaged(_) | Cmd::Sub(..) | Cmd::App(_) => {
            let (pick, title) = match cmd {
                Cmd::Packaged(i) => (explorercmd::Pick::Command(i), m.answer.commands.get(i).map(|c| c.title.clone())),
                Cmd::Sub(i, j) => (explorercmd::Pick::Sub(i, j), m.answer.commands.get(i).and_then(|c| c.subs.get(j)).map(|c| c.title.clone())),
                Cmd::App(k) => (explorercmd::Pick::App(k), m.answer.apps.get(k).map(|a| format!("Open with {}", a.name))),
                _ => return,
            };
            let run = explorercmd::Run { seq: m.seq, pick, folder: m.req.folder.clone(), names: m.req.names.clone(), title: title.unwrap_or_default() };
            with_state(|st| {
                st.next_verb += 1;
                let id = st.next_verb;
                eprintln!("[winmenu] {:?} \"{}\" queued on the verb worker, id {id}", run.pick, run.title);
                if !st.verbs.submit(verbs::Job { id, pane, work: verbs::Work::Run(run) }) {
                    if let Some(app) = st.app.upgrade() {
                        note(st, &app, pane, "context menu: the verb worker has stopped".into());
                    }
                }
            });
        }
        // Submenus open; they are never run.
        Cmd::OpenWith | Cmd::Compress => {}
        _ => {}
    }
}

/// The classic menu's "Send to > Compressed (zipped) folder", the Windows 11 menu's Compress to >
/// ZIP file (P7-M2; Explorer's own Compress to is not reachable from another program).
const ZIP_LABEL: &str = "Send to>Compressed (zipped) folder";

/// For the scripted driver: the Windows 11 menu is shown, and (`settled`) the worker has answered
/// or was not asked.
pub fn winmenu_shown(settled: bool) -> bool {
    STATE.with(|s| s.try_borrow().ok().and_then(|g| g.as_ref().and_then(|st| st.winmenu.as_ref().map(|m| m.shown && (!settled || !m.waiting)))).unwrap_or(false))
}

/// For the scripted driver's in-menu steps on the Windows 11 menu: `open` a submenu by its item's
/// text, `pick` an item (the submenu's first, then the icon row's, then the menu's) by its text,
/// `key` a key name as `winmenu_key` takes it, `close` the menu. A pick of an item that runs on the
/// verb worker is refused (the menu closes instead), except Compress to > ZIP file inside the soak
/// fence. False when no Windows 11 menu is open.
pub fn script_winmenu(what: &str, arg: &str) -> bool {
    use winmenu::Cmd;
    let Some((found, soak_ok)) = with_state(|st| {
        let m = st.winmenu.as_ref().filter(|m| m.shown)?;
        let by_text = |e: &winmenu::Entry| !e.is_sep() && e.text.eq_ignore_ascii_case(arg.trim());
        let found = m
            .sub
            .iter()
            .flat_map(|s| s.items.iter())
            .find(|e| by_text(e))
            .and_then(|e| e.cmd)
            .or_else(|| m.tools.iter().find(|t| t.text.eq_ignore_ascii_case(arg.trim())).map(|t| t.cmd))
            .or_else(|| m.items.iter().find(|e| by_text(e)).and_then(|e| e.cmd));
        let soak_ok = st.soak.as_ref().is_some_and(|roots| roots.iter().any(|r| inside(&m.req.folder, r)));
        Some((found, soak_ok))
    }) else {
        return false;
    };
    match what {
        "key" => winmenu_key(arg),
        "close" => {
            winmenu_close("menuclose");
        }
        "open" => match found {
            Some(c) if c == Cmd::More => winmenu_more_now(),
            Some(c) => winmenu_open_sub(c, false),
            None => eprintln!("[auto] menuopen \"{arg}\": no such item in the menu"),
        },
        _ => match found {
            Some(c) if c.on_worker() && !(c == Cmd::Zip && soak_ok) => {
                eprintln!("[auto] menupick \"{arg}\": {c:?} runs on the verb worker; closing instead");
                winmenu_close("menupick refused");
            }
            Some(c) if c == Cmd::More => winmenu_more_now(),
            Some(c) => {
                eprintln!("[auto] menupick \"{arg}\": {c:?}");
                winmenu_picked(c.id());
            }
            None => eprintln!("[auto] menupick \"{arg}\": no such item in the menu"),
        },
    }
    true
}

/// "Show more options" from the driver: the classic menu at once, inside this call (its in-menu
/// steps run from the driver's Win32 timer), without the frame's wait the pick takes.
fn winmenu_more_now() {
    let Some(m) = winmenu_close("Show more options") else { return };
    classic_menu(m.pane, m.row, m.at.0, m.at.1, false);
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

/// Return, or a double-click that was not the tail of a drag: the row, or the whole selection
/// when the row is in a selection of several (P8-L1, Explorer's; `model::open_set`).
fn open_row(pane: usize, row: i32) {
    let Ok(row) = usize::try_from(row) else { return };
    let Some(rows) = with_state(|st| Some(model::open_set(&st.model(pane).selected_set(), row))) else { return };
    open_rows(pane, rows);
}

/// Open `rows` (P8-L1): one row as `open_one`; several, after a question above
/// `model::OPEN_ASK_ABOVE` items (none under TWINFM_SOAK): the folders, drives and archives each
/// in a new tab of the pane, in order, the last one shown; the files with the shell's default
/// command for all of them at once on the verb worker (what Explorer runs: a type whose app takes
/// several files at once gets them together), or inside an archive each through its PIDL.
fn open_rows(pane: usize, rows: Vec<usize>) {
    if rows.len() <= 1 {
        if let Some(&row) = rows.first() {
            open_one(pane, row as i32);
        }
        return;
    }
    let Some((hwnd, folder, archive, soak, items)) = with_state(|st| {
        let app = st.app.upgrade()?;
        let folder = st.tab(pane).path.clone();
        let archive = in_archive(st, pane);
        let items: Vec<(bool, String)> = rows
            .iter()
            .filter_map(|&r| st.model(pane).entry(r))
            .map(|e| (e.is_dir() || fs::archive::is_archive_name(&e.name), e.name.to_string()))
            .collect();
        Some((main_hwnd(st, &app), folder, archive, st.soak.is_some(), items))
    }) else {
        return;
    };
    let (dirs, files): (Vec<_>, Vec<_>) = items.into_iter().partition(|(enter, _)| *enter);
    let n = dirs.len() + files.len();
    eprintln!("[open] pane={pane} {n} items: {} folder(s) to tabs, {} file(s)", dirs.len(), files.len());
    // "Open N items at once?", as Explorer asks before opening many: one of TwinFM's own questions
    // (P12-M3), answered later, so the items wait in its reply.
    if n > model::OPEN_ASK_ABOVE && !soak {
        let q = ask::Question {
            kind: "open many",
            title: format!("Open {n} items at once?"),
            text: "Each folder opens in a new tab, and each file in its program.".into(),
            password: false,
            buttons: vec!["Cancel", "Open"],
            primary: 1,
            cancel: 0,
        };
        ask::ask_here(q, "the UI thread", move |a| {
            if a.button == 1 {
                open_items(pane, hwnd, &folder, archive, dirs, files);
            } else {
                eprintln!("[open] pane={pane} {n} items: not opened (the question was declined)");
            }
        });
        return;
    }
    open_items(pane, hwnd, &folder, archive, dirs, files);
}

/// The items `open_rows` gathered, opened: the folders in new tabs, the files as below.
fn open_items(pane: usize, hwnd: isize, folder: &Path, archive: bool, dirs: Vec<(bool, String)>, files: Vec<(bool, String)>) {
    for (_, name) in &dirs {
        open_tab(pane, Some(folder.join(name)));
    }
    if files.is_empty() {
        return;
    }
    if archive {
        for (_, name) in &files {
            let file = folder.join(name);
            let res = shellmenu::open_in_archive(hwnd, &file);
            eprintln!("[open] pane={pane} {}: {res:?}", file.display());
        }
        return;
    }
    // One job per folder: a search tab's files can lie in several, and one menu takes one folder's.
    let mut by_dir: Vec<(PathBuf, Vec<String>)> = Vec::new();
    for (_, name) in files {
        let path = folder.join(&name);
        let (dir, leaf) = (path.parent().map_or_else(|| folder.to_path_buf(), Path::to_path_buf), path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(name));
        match by_dir.iter_mut().find(|(d, _)| *d == dir) {
            Some((_, names)) => names.push(leaf),
            None => by_dir.push((dir, vec![leaf])),
        }
    }
    for (dir, names) in by_dir {
        let request = shellmenu::VerbRequest {
            folder: dir,
            names,
            verb: String::new(),
            offset: 0,
            label: "Open".into(),
            x: 0,
            y: 0,
            extended: false,
            find: false,
            default_item: true,
        };
        queue_verb(pane, request, None);
    }
}

/// One row: enter its directory (also a reparse directory or a drive) or zip archive, or open the
/// file with its default program (a file inside an archive through its PIDL: the shell extracts
/// it and opens the copy).
fn open_one(pane: usize, row: i32) {
    let target = with_state(|st| {
        let e = st.model(pane).entry(usize::try_from(row).ok()?)?;
        let app = st.app.upgrade()?;
        let folder = st.tab(pane).path.clone();
        let hwnd = main_hwnd(st, &app);
        let path = folder.join(e.name.as_str());
        // The file's own folder (a search result's may lie below the tab's).
        let dir = path.parent().map_or(folder, Path::to_path_buf);
        let archive = in_archive(st, pane);
        // An archive opens as a folder, one inside an archive too (P8-L3: read-only, from its copy).
        let enter = e.is_dir() || fs::archive::is_archive_name(&e.name);
        Some((enter, path, dir, hwnd, archive, st.tab(pane).sizeview.is_some()))
    });
    match target {
        // In a sizes view a folder opens in a normal tab (its chevron opens it in place, P13-M3).
        Some((true, p, _, _, _, true)) => open_tab(pane, Some(p)),
        Some((true, p, _, _, _, false)) => navigate(pane, p),
        Some((false, file, folder, hwnd, archive, _)) => {
            // No state borrow across ShellExecuteExW: the shell can show a dialog from inside it.
            let res = if archive { shellmenu::open_in_archive(hwnd, &file) } else { shellmenu::open_item(hwnd, &file, &folder) };
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
    if let Some(p) = target {
        go_to_place(pane, p);
    }
}

/// Go to a place taken from the tab's history, without recording the move: a search in it runs
/// again, in this tab, under the same root.
fn go_to_place(pane: usize, place: Place) {
    match place {
        Place { path, search: Some(pattern), content } => start_search(pane, &pattern, content, Some(path)),
        Place { path, search: None, .. } => navigate_to(pane, path, false),
    }
}

/// A history menu pick (P11-M3): `steps` places back (negative) or forward, in one move.
fn history_jump(pane: usize, steps: isize) {
    let target = with_state(|st| {
        let t = st.tab_mut(pane);
        let here = t.place();
        let to = t.history.jump(&here, steps);
        match &to {
            Some(to) => eprintln!("[nav] pane={pane} history jump {steps}: {here} -> {to} ({} back, {} forward left)", t.history.back_len(), t.history.forward_len()),
            None => eprintln!("[nav] pane={pane} history jump {steps}: nothing there"),
        }
        to
    });
    if let Some(p) = target {
        go_to_place(pane, p);
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

/// The middle button on a row: a folder (or a drive, a reparse directory, a zip archive) opens in
/// a new tab in the same pane, as a middle-click in the sidebar does; a file does nothing.
fn middle_click_row(pane: usize, row: usize) {
    let target = with_state(|st| {
        let e = st.model(pane).entry(row)?;
        let archive = fs::archive::is_archive_name(&e.name);
        Some((e.is_dir() || archive, st.tab(pane).path.join(e.name.as_str())))
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
            prefetch_drag_icon(st, pane, row);
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
        // The sizes view's chevron (P13-M3).
        8 => size_toggle(pane, row, None),
        _ => {}
    }
}

/// A press on `row` that may become a drag (the left button, or the right): ask the icon worker
/// for the drag image's icon now, one channel send, so the drag that may follow finds it rendered
/// instead of asking the shell on the UI thread (113 ms for the first drag of a run, P2-M4). Not
/// for a drive row (never dragged) nor for a grid tile showing a real picture (the image is the
/// tile's).
fn prefetch_drag_icon(st: &mut State, pane: usize, row: usize) {
    let Some(app) = st.app.upgrade() else { return };
    let model = st.model(pane);
    let Some(e) = model.entry(row) else { return };
    let grid = st.grid[pane];
    if e.kind == Kind::Drive || (grid && e.icon >= icons::PLACEHOLDERS) {
        return;
    }
    let scale = app.window().scale_factor();
    let size_px = if grid { dnd::ImageGeom::tile(scale, st.tile[pane]).icon } else { dnd::ImageGeom::new(scale).icon };
    let path = st.tab(pane).path.join(e.name.as_str());
    let generation = model.generation.get();
    let key = icons::drag_key(&path);
    // A row that shows a thumbnail gets one in its drag image too (P9-M4), at the image's icon size
    // and kept to the cache as the row's own is (its mark); the worker falls back to the icon.
    let thumbnail = st.config.thumbnails && e.icon_key.starts_with("thumb:");
    let mark = if thumbnail { icons::key_mark(&e.icon_key) } else { icons::Mark::None };
    let _ = st.icon_tx.send(icons::Request { generation, pane, key, path: path.clone(), size_px, overlay: false, thumbnail, row: 0, wanted: 0, mark });
    st.drag_icon = Some((path, None));
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
            tile: st.grid[pane].then_some(st.tile[pane]),
            tile_picture: shown.icon >= icons::PLACEHOLDERS,
            prefetched: None,
            prefetched_thumbnail: false,
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
    let Some((pane, mut req)) = PENDING_DRAG.with(|p| p.borrow_mut().take()) else { return };
    let right = req.right;
    if !state_is_free() {
        // Not expected: the message is dispatched by an event loop, never inside a borrow.
        eprintln!("[dnd] pane={pane} pending drag dropped: app state busy");
        with_state(|st| st.gesture[pane].drag_ended());
        return slint::Timer::single_shot(Duration::ZERO, move || release_after_drag(right));
    }
    // The drag image's icon from the press, when the icon worker has answered it (its reply may
    // still wait in the channel: the posted message can come before the worker's wake-up).
    drain_icons();
    with_state(|st| {
        if let (Some(img), Some((path, Some((p, thumb))))) = (req.image.as_mut(), st.drag_icon.take()) {
            if path == img.path {
                img.prefetched = Some(p);
                img.prefetched_thumbnail = thumb;
            }
        }
    });
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
    let left = if app.get_sidebar_visible() { app.get_sidebar_width() + 4.0 } else { dnd::RAIL_W };
    dnd::Layout {
        width: size.width,
        height: size.height,
        split: app.get_split(),
        ratio: app.get_split_ratio(),
        // Every pane shows its tab strip, one tab too (P12-M5).
        tabs: [true; 2],
        left,
        preview: if app.get_preview() { dnd::preview_w(size.width, left, app.get_preview_width()) } else { 0.0 },
        row_px: icons::ListGeom::new(app.get_thumbnails(), app.get_comfortable()).row_px,
        grid: [
            app.get_left_grid().then(|| app.get_left_grid_cols().max(1) as usize),
            app.get_right_grid().then(|| app.get_right_grid_cols().max(1) as usize),
        ],
        tile: [icons::TileGeom::of(app.get_left_medium()), icons::TileGeom::of(app.get_right_medium())],
        top: if app.get_toolbar() == settings::Toolbar::Top.index() { dnd::TOP_BAR } else { 0.0 },
        bar: if app.get_toolbar() == settings::Toolbar::Pane.index() { dnd::PANE_BAR } else { 0.0 },
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
        ARCHIVE_REFUSED.with(|c| c.borrow_mut().take());
    }
    spot
}

thread_local! {
    /// The search tab (pane, tab id) a drag over it was last refused for, so the refusal is
    /// logged once per hover rather than on every DragOver.
    static DROP_REFUSED: std::cell::Cell<Option<(usize, u64)>> = const { std::cell::Cell::new(None) };
    /// The folder inside an archive a drag was last refused for (logged once per hover).
    static ARCHIVE_REFUSED: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// No drop into an archive TwinFM does not write (P7-M3: every format but zip): no spot, logged
/// and noted in `pane` once per hover (the note through the event loop: this runs inside DragOver
/// with the app state borrowed).
fn refuse_archive_drop(pane: usize, dest: &Path) -> Option<dnd::DropSpot> {
    if ARCHIVE_REFUSED.with(|c| c.borrow_mut().replace(dest.to_path_buf())).as_deref() != Some(dest) {
        let why = fs::archive::split(dest).map_or_else(String::new, |(a, _)| fileops::archive::read_only_why(&a));
        eprintln!("[archive] refused: drop into {} ({why})", dest.display());
        let text = format!("drop: {why}");
        let _ = slint::invoke_from_event_loop(move || drop_note(pane, text));
    }
    None
}

/// `dest` lies inside an archive TwinFM does not write.
fn read_only_dest(dest: &Path) -> bool {
    fs::archive::inside(dest) && !fs::archive::writable(dest)
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
        let (crumb_row, crumbs) = crumb_geometry_now(&app);
        let strips = strips(st, &app, &side_rows, crumb_row, &crumbs);
        let (pane, row) = match dnd::hit_all(&layout, &strips, px as f32 / scale, py as f32 / scale, content_y, len) {
            dnd::Spot::Outside => return None,
            dnd::Spot::Pane(p) => (p, None),
            dnd::Spot::Row(p, r) => (p, Some(r)),
            // The Bookmarks gap (P12-M5): the folders dropped are bookmarked there, nothing copied.
            dnd::Spot::SideGap(k) => {
                return Some(dnd::DropSpot { pane: active_pane(&app), mark: dnd::Mark::Gap(k), dest: PathBuf::new(), springs: false });
            }
            // A sidebar row's folder (a drive's root, a place, a bookmark), noted in the active pane.
            dnd::Spot::Side(i) => {
                let dest = st.side.get(i)?.path.clone()?;
                if read_only_dest(&dest) {
                    return refuse_archive_drop(active_pane(&app), &dest);
                }
                // Held there, it opens in the active pane (P11-M3), never inside an archive.
                let springs = !fs::archive::inside(&dest);
                return Some(dnd::DropSpot { pane: active_pane(&app), mark: dnd::Mark::Side(i), dest, springs });
            }
            // A tab's folder; a tab on This PC is no folder. A tab other than the active one opens
            // when the drag is held over it.
            dnd::Spot::Tab(p, i) => {
                let tab = st.panes[p].iter().nth(i)?;
                if tab.results() {
                    refuse_search_drop(p, tab.id);
                    return None;
                }
                let dest = tab.path.clone();
                if dest.as_os_str().is_empty() || is_this_pc(&dest.to_string_lossy()) {
                    return None;
                }
                if read_only_dest(&dest) {
                    return refuse_archive_drop(p, &dest);
                }
                let springs = st.panes[p].active_index() != i;
                return Some(dnd::DropSpot { pane: p, mark: dnd::Mark::Tab(i), dest, springs });
            }
            // A crumb's folder (the This PC crumb is none). A search tab's breadcrumb is its root's
            // crumbs, real folders, then the search's own crumb, which is no folder.
            dnd::Spot::Crumb(p, i) => {
                let tab = st.tab(p);
                let Some(c) = pathbar::crumbs(&tab.path.to_string_lossy()).into_iter().nth(i) else {
                    if tab.results() {
                        refuse_search_drop(p, tab.id);
                    }
                    return None;
                };
                if is_this_pc(&c.target.to_string_lossy()) {
                    return None;
                }
                if read_only_dest(&c.target) {
                    return refuse_archive_drop(p, &c.target);
                }
                // Held there, it opens in its pane (P11-M3): not the folder the pane already shows,
                // never inside an archive.
                let springs = !c.target.as_os_str().eq_ignore_ascii_case(tab.path.as_os_str()) && !fs::archive::inside(&c.target);
                return Some(dnd::DropSpot { pane: p, mark: dnd::Mark::Crumb(i), dest: c.target, springs });
            }
        };
        if st.is_results(pane) {
            refuse_search_drop(pane, st.tab(pane).id);
            return None;
        }
        if in_readonly_archive(st, pane) {
            return refuse_archive_drop(pane, &st.tab(pane).path);
        }
        let folder = &st.tab(pane).path;
        let entry = row.and_then(|r| st.model(pane).entry(r));
        let this_pc = is_this_pc(&folder.to_string_lossy());
        let (dest, on_row) = dnd::drop_folder(folder, this_pc, entry.as_ref().map(|e| (e.name.as_str(), e.is_dir())))?;
        let row = if on_row { row.map_or(-1, |r| r as i32) } else { -1 };
        // A folder row or tile (a drive row on This PC too) opens when the drag is held over it;
        // never inside an archive (a zip's levels take drops since P7-M3).
        Some(dnd::DropSpot { pane, mark: dnd::Mark::Row(row), dest, springs: row >= 0 && !in_archive(st, pane) })
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
            sidebar::RowKind::Header => (32.0, false),
            _ if i as i32 == editing => (30.0, true),
            _ => (32.0, true),
        })
        .collect()
}

/// The sidebar, the tab strips and the breadcrumbs (`crumb_geometry_now`) for `dnd::hit_all`.
fn strips<'a>(st: &State, app: &App, side_rows: &'a [(f32, bool)], crumb_row: [(f32, f32); 2], crumbs: &'a [Vec<(f32, f32)>; 2]) -> dnd::Strips<'a> {
    let tab = |p: usize| (st.panes[p].len(), app.invoke_tab_width(p as i32), app.invoke_tab_offset(p as i32));
    dnd::Strips {
        side_w: if app.get_sidebar_visible() { app.get_sidebar_width() } else { dnd::RAIL_W },
        side_scroll: app.invoke_side_scroll(),
        side_rows,
        side_bookmarks: (st.side.len() - st.config.bookmarks.len(), st.config.bookmarks.len()),
        tabs: [tab(0), tab(1)],
        crumb_row,
        crumbs: [&crumbs[0], &crumbs[1]],
    }
}

/// One pane's breadcrumb geometry as Slint reports it (`crumb-geometry`): the crumb row's offset
/// in the bar and the width the bar shows of it, each crumb's x in the row and its label's width
/// (None until that crumb has reported), and whether the full set was logged.
struct CrumbGeom {
    row: (f32, f32),
    crumbs: Vec<Option<(f32, f32)>>,
    logged: bool,
}

thread_local! {
    /// Both panes' breadcrumb geometry. Kept outside the app state: Slint reports it from its
    /// instantiation and change-handler passes, which a call holding the state's borrow could
    /// set off. Until the bar's width is reported, the whole row counts as shown.
    static CRUMB_GEOM: RefCell<[CrumbGeom; 2]> = const {
        RefCell::new([
            CrumbGeom { row: (0.0, f32::MAX), crumbs: Vec::new(), logged: false },
            CrumbGeom { row: (0.0, f32::MAX), crumbs: Vec::new(), logged: false },
        ])
    };
}

/// The pane's breadcrumb was rebuilt with `n` crumbs (`set_path`): their geometry comes again as
/// Slint makes the buttons.
fn crumb_geometry_reset(pane: usize, n: usize) {
    CRUMB_GEOM.with(|g| {
        let g = &mut g.borrow_mut()[pane];
        g.crumbs = vec![None; n];
        g.logged = false;
    });
}

/// `crumb-geometry(pane, index, x, width)` from pane.slint: crumb `index`'s x in the crumb row and
/// its label's width, or with index -1 the row's offset in the bar and the bar's shown width. Once
/// per crumb when the path is rebuilt and once per change of its place after that (a resize, the
/// filter box opening); one `[crumb]` line per rebuild, when every crumb has reported.
fn crumb_geometry(pane: usize, index: i32, x: f32, w: f32) {
    CRUMB_GEOM.with(|g| {
        let g = &mut g.borrow_mut()[pane];
        match usize::try_from(index) {
            Err(_) => g.row = (x, w),
            Ok(i) => {
                if let Some(c) = g.crumbs.get_mut(i) {
                    *c = Some((x, w));
                }
            }
        }
        if !g.logged && !g.crumbs.is_empty() && g.crumbs.iter().all(Option::is_some) {
            g.logged = true;
            eprintln!("[crumb] pane={pane} geometry: {} crumbs", g.crumbs.len());
        }
    });
}

/// The breadcrumbs as `dnd::Strips` takes them: each pane's row offset and shown width, and its
/// crumbs (none while its bar shows the text entry; a crumb that has not reported yet has no
/// width, so nothing hits it).
fn crumb_geometry_now(app: &App) -> ([(f32, f32); 2], [Vec<(f32, f32)>; 2]) {
    CRUMB_GEOM.with(|g| {
        let g = g.borrow();
        let crumbs = |p: usize| {
            if app.invoke_path_entry_shown(p as i32) { Vec::new() } else { g[p].crumbs.iter().map(|c| c.unwrap_or((0.0, 0.0))).collect() }
        };
        ([g[0].row, g[1].row], [crumbs(0), crumbs(1)])
    })
}

/// For the driver's `dropfiles crumb` and `dragover crumb`: the centre of crumb `i`'s label in the
/// window (logical px); a negative `i` counts from the end (-1: the folder itself, -2 its parent).
/// None when the crumb is not on screen.
pub fn crumb_centre(app: &App, pane: usize, i: i32) -> Option<slint::LogicalPosition> {
    let (row, crumbs) = crumb_geometry_now(app);
    let c = &crumbs[pane];
    let i = if i < 0 { c.len().checked_sub(i.unsigned_abs() as usize)? } else { i as usize };
    let (x, w) = *c.get(i)?;
    let layout = window_layout(app);
    let bx = row[pane].0 + x + w / 2.0;
    if w <= 0.0 || bx < 0.0 || bx >= row[pane].1 {
        return None;
    }
    let (left, _) = dnd::pane_x(&layout, pane);
    Some(slint::LogicalPosition::new(left + dnd::PANE_PAD + dnd::NAV_W + bx, dnd::crumb_top(&layout, pane) + dnd::CRUMB_H / 2.0))
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
        Some(dnd::side_top(&window_layout(&app)) + app.invoke_side_scroll() + dnd::side_row_top(&rows, i) + h / 2.0)
    })
}

/// The window y of the line before bookmark `k` in the sidebar (the count: after the last), the
/// driver's `dropfiles sidegap` (P12-M5).
pub fn side_gap_y(k: usize) -> Option<f32> {
    with_state(|st| {
        let app = st.app.upgrade()?;
        let rows = side_rows(st, &app);
        let first = st.side.len().checked_sub(st.config.bookmarks.len())?;
        Some(dnd::side_top(&window_layout(&app)) + app.invoke_side_scroll() + dnd::side_row_top(&rows, first + k.min(st.config.bookmarks.len())))
    })
}

/// What `drop_autoscroll` scrolls, read under the state's borrow: a list's line (`edge_line`); the
/// sidebar list in a direction; a pane's tab strip in a direction, with its tab count and its
/// width.
enum AutoScroll {
    List(EdgeLine),
    Side(App, i32),
    Strip(App, usize, i32, f32, f32),
}

/// For the drop target's DragOver: a drag hovering near a list's top or bottom edge (client
/// point, physical px) scrolls that list by one row; near the sidebar list's top or bottom, that
/// list by one 24 px row (sidebar.slint `SideRow`); near either end of a tab strip whose tabs
/// overflow it, the strip by one tab. True when it scrolled. The state is read under a short
/// borrow, released before the scroll is set (each scroll is a property set).
pub fn drop_autoscroll(px: i32, py: i32) -> bool {
    let found = STATE.with(|s| {
        let guard = s.try_borrow().ok()?;
        let st = guard.as_ref()?;
        let app = st.app.upgrade()?;
        let scale = app.window().scale_factor();
        let layout = window_layout(&app);
        let side_rows = side_rows(st, &app);
        // The edges need no crumbs.
        let no_crumbs = [Vec::new(), Vec::new()];
        let strips = strips(st, &app, &side_rows, [(0.0, 0.0); 2], &no_crumbs);
        Some(match dnd::edge_scroll_all(&layout, &strips, px as f32 / scale, py as f32 / scale)? {
            dnd::Edge::List(pane, dir) => AutoScroll::List(edge_line(st, &app, &layout, pane, dir)),
            dnd::Edge::Side(dir) => AutoScroll::Side(app.clone_strong(), dir),
            dnd::Edge::Strip(pane, dir) => {
                let (a, b) = dnd::pane_x(&layout, pane);
                AutoScroll::Strip(app.clone_strong(), pane, dir, st.panes[pane].len() as f32, b - a - 2.0 * dnd::PANE_PAD)
            }
        })
    });
    let Some(scroll) = found else { return false };
    match scroll {
        AutoScroll::List(line) => {
            let Some((pane, dir, from, to)) = scroll_line(line) else { return false };
            eprintln!("[dnd] auto-scroll pane {pane} {}: content-y {from:.0} -> {to:.0}", if dir < 0 { "up" } else { "down" });
        }
        AutoScroll::Side(app, dir) => {
            // The ListView's own content height (an estimate for rows of mixed heights, a px or
            // two off their sum, which it corrects as it scrolls), so the end is where it stops.
            let from = app.invoke_side_scroll();
            let to = dnd::scroll_step(from, dir, 32.0, app.invoke_side_content_height(), app.invoke_side_visible_height());
            if to == from {
                return false;
            }
            app.invoke_set_side_scroll(to);
            eprintln!("[dnd] auto-scroll sidebar {}: content-y {from:.0} -> {to:.0}", if dir < 0 { "up" } else { "down" });
        }
        AutoScroll::Strip(app, pane, dir, tabs, visible) => {
            let (w, from) = (app.invoke_tab_width(pane as i32), app.invoke_tab_offset(pane as i32));
            let to = dnd::scroll_step(from, dir, w, tabs * w, visible);
            if to == from {
                return false;
            }
            app.invoke_set_tab_scroll(pane as i32, to);
            eprintln!("[dnd] auto-scroll pane {pane} tab strip {}: x {from:.0} -> {to:.0}", if dir < 0 { "left" } else { "right" });
        }
    }
    dnd::repaint_during_own_drag();
    true
}

/// One line's auto-scroll of `pane` in `dir`, to run outside the state borrow: the app, the
/// pane, the direction, the lines the list scrolls by (rows, or grid rows of tiles), the
/// viewport height and the line height.
type EdgeLine = (App, usize, i32, usize, f32, f32);

fn edge_line(st: &State, app: &App, layout: &dnd::Layout, pane: usize, dir: i32) -> EdgeLine {
    let visible = layout.height - dnd::LIST_BOTTOM_GAP - dnd::list_top(layout, pane);
    let lines = match layout.grid[pane] {
        Some(cols) => model::grid_rows(st.model(pane).len(), cols),
        None => st.model(pane).len(),
    };
    (app.clone_strong(), pane, dir, lines, visible, layout.row_px_of(pane))
}

/// Scroll the line `edge_line` described; the pane, direction and content-y before and after,
/// or None when the list was already at that end.
fn scroll_line((app, pane, dir, len, visible, line_px): EdgeLine) -> Option<(usize, i32, f32, f32)> {
    let from = app.invoke_get_scroll(pane as i32);
    let to = dnd::scrolled(from, dir, len, visible, line_px);
    if to == from {
        return None;
    }
    app.invoke_set_scroll(pane as i32, to);
    Some((pane, dir, from, to))
}

// --- The rubber band (P4-M3) --------------------------------------------------------------------

/// A window point as content coordinates of `pane`'s list (x from the list's left edge, y from
/// the top of the first grid row).
fn band_point(app: &App, layout: &dnd::Layout, pane: usize, at: (f32, f32)) -> (f32, f32) {
    let (left, _) = dnd::pane_x(layout, pane);
    let content_y = app.invoke_get_scroll(pane as i32);
    (at.0 - left - dnd::PANE_PAD, at.1 - dnd::list_top(layout, pane) - content_y)
}

/// The left button on a grid pane's empty space (`band-pointer`): kind 0 down, 1 move, 2 up, 3
/// cancel, at window point `at` (logical px). The band starts once the pointer leaves the drag
/// threshold, selects the tiles it touches on every move, scrolls the list while the pointer sits
/// at its top or bottom edge, and ends on the release; a press released under the threshold is a
/// click on empty space, which clears the selection (Explorer) unless Ctrl or Shift is held.
fn band_pointer(pane: usize, kind: i32, at: (f32, f32), ctrl: bool, shift: bool) {
    match kind {
        0 => with_state(|st| {
            let Some(app) = st.app.upgrade() else { return };
            let layout = window_layout(&app);
            if layout.grid[pane].is_none() {
                return;
            }
            let p = band_point(&app, &layout, pane, at);
            st.band = Some(band::Band::press(pane, p, band::Mode::from_keys(ctrl, shift), st.model(pane).selected_set()));
            st.band_at = at;
            st.band_logged = None;
        }),
        1 => band_moved(pane, at),
        2 | 3 => {
            let taken = with_state(|st| {
                let b = st.band.take().filter(|b| b.pane == pane)?;
                st.band_timer.stop();
                Some((b, st.app.upgrade()?))
            });
            let Some((b, app)) = taken else { return };
            app.set_band_pane(-1);
            if kind == 3 {
                band_restore(pane, &b, "the pointer grab ended");
            } else if b.active {
                eprintln!("[band] pane={pane} done: {} selected, cursor {}", with_state(|st| st.model(pane).selected_count()), current_row(&app, pane));
            } else if b.mode == band::Mode::Replace {
                with_state(|st| {
                    let model = st.panes[pane].active().model.clone();
                    if model.selected_count() > 0 {
                        model.select(SelOp::Clear, 0);
                        log_select(&model, pane, SelOp::Clear, 0);
                        size_selection(st, pane);
                        update_status(st, &app, pane);
                    }
                });
            }
        }
        _ => {}
    }
}

/// The pointer moved to `at` with the band's button down (or the timer fired with it still at
/// the edge).
fn band_moved(pane: usize, at: (f32, f32)) {
    let edge = with_state(|st| {
        let app = st.app.upgrade()?;
        let layout = window_layout(&app);
        let threshold = (app.get_drag_dx(), app.get_drag_dy());
        let p = band_point(&app, &layout, pane, at);
        let b = st.band.as_mut().filter(|b| b.pane == pane)?;
        st.band_at = at;
        if !b.moved(p, threshold) {
            return None;
        }
        if app.get_band_pane() != pane as i32 {
            eprintln!("[band] pane={pane} start at ({:.0}, {:.0}), {:?}", b.anchor.0, b.anchor.1, b.mode);
            app.set_band_pane(pane as i32);
        }
        band_apply(st, &app, pane, true);
        // At the list's top or bottom edge, or beyond it: scroll a grid row every SCROLL_EVERY
        // until the pointer leaves the edge or the button comes up; each scroll moves the band's
        // far edge.
        match band::edge(dnd::list_top(&layout, pane), layout.height - dnd::LIST_BOTTOM_GAP, at.1) {
            Some(dir) => {
                if !st.band_timer.running() {
                    st.band_timer.start(slint::TimerMode::Repeated, dnd::SCROLL_EVERY, move || band_tick(pane));
                }
                Some(edge_line(st, &app, &layout, pane, dir))
            }
            None => {
                st.band_timer.stop();
                None
            }
        }
    });
    if let Some((_, dir, from, to)) = edge.and_then(scroll_line) {
        eprintln!("[band] auto-scroll pane {pane} {}: content-y {from:.0} -> {to:.0}", if dir < 0 { "up" } else { "down" });
        // The content moved under the pointer: the band's far edge and the touched tiles with it.
        with_state(|st| {
            let Some(app) = st.app.upgrade() else { return };
            let layout = window_layout(&app);
            let p = band_point(&app, &layout, pane, st.band_at);
            if let Some(b) = st.band.as_mut().filter(|b| b.pane == pane) {
                b.now = p;
                band_apply(st, &app, pane, false);
            }
        });
    }
}

/// The band's timer: the pointer still sits at the edge, so scroll another line.
fn band_tick(pane: usize) {
    let at = with_state(|st| st.band.as_ref().filter(|b| b.pane == pane).map(|_| st.band_at));
    match at {
        Some(at) => band_moved(pane, at),
        None => with_state(|st| st.band_timer.stop()),
    }
}

/// Draw the band where it is and select the tiles it touches, when that changed anything.
/// `log`: a `[band]` line for a changed count, at most four a second.
fn band_apply(st: &mut State, app: &App, pane: usize, log: bool) {
    let Some(b) = st.band.as_ref().filter(|b| b.pane == pane) else { return };
    let layout = window_layout(app);
    let Some(cols) = layout.grid[pane] else { return };
    let tile = layout.tile[pane];
    let (x, y, w, h) = band::rect(b.anchor, b.now);
    app.set_band_x(x);
    app.set_band_y(y);
    app.set_band_w(w);
    app.set_band_h(h);
    let model = st.panes[pane].active().model.clone();
    let want = b.selection(cols, model.len(), tile);
    // The cursor follows the band (P6-M2, Explorer): to the touched tile under the band's moving
    // corner, which also anchors Shift, so the keys and Shift+click continue from there.
    if let Some(c) = b.cursor(cols, model.len(), tile) {
        if current_row(app, pane) != c as i32 {
            set_current_row(app, pane, Some(c));
        }
        model.set_anchor(c);
    }
    if want == model.selected_set() {
        return;
    }
    model.set_selection(want);
    size_selection(st, pane);
    update_status(st, app, pane);
    if log && st.band_logged.is_none_or(|t| t.elapsed() >= Duration::from_millis(250)) {
        st.band_logged = Some(Instant::now());
        eprintln!("[band] pane={pane} {} selected, rect ({x:.0}, {y:.0}) {w:.0} x {h:.0}", model.selected_count());
    }
}

/// Escape with a band under way: the band ends and the selection goes back to what it was at
/// the press. True when there was one.
fn band_cancel(pane: usize) -> bool {
    let taken = with_state(|st| {
        let b = st.band.take().filter(|b| b.pane == pane)?;
        st.band_timer.stop();
        Some((b, st.app.upgrade()))
    });
    let Some((b, app)) = taken else { return false };
    if let Some(app) = app {
        app.set_band_pane(-1);
    }
    band_restore(pane, &b, "Escape");
    true
}

fn band_restore(pane: usize, b: &band::Band, why: &str) {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let model = st.panes[pane].active().model.clone();
        if b.active {
            model.set_selection(b.before.clone());
            size_selection(st, pane);
            update_status(st, &app, pane);
        }
        eprintln!("[band] pane={pane} cancelled ({why}): {} selected", model.selected_count());
    });
}

/// For the drop target: highlight what a drop would land on (a pane's row, or with row -1 its
/// whole list; a tab; a sidebar row; a crumb), or nothing. Sets the properties only when they
/// change.
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
    let (pane, row, tab_pane, tab, crumb_pane, crumb) = match at {
        Some((p, dnd::Mark::Row(r))) => (p as i32, r, -1, -1, -1, -1),
        Some((p, dnd::Mark::Tab(i))) => (-1, -1, p as i32, i as i32, -1, -1),
        Some((p, dnd::Mark::Crumb(i))) => (-1, -1, -1, -1, p as i32, i as i32),
        _ => (-1, -1, -1, -1, -1, -1),
    };
    let (side_row, side_y, side_h) = side.unwrap_or((-1, 0.0, 0.0));
    // The Bookmarks gap's insertion line (P12-M5).
    let gap = if let Some((_, dnd::Mark::Gap(k))) = at { k as i32 } else { -1 };
    if app.get_drop_side_gap() != gap {
        app.set_drop_side_gap(gap);
        dnd::repaint_during_own_drag();
    }
    let now = (
        (app.get_drop_pane(), app.get_drop_row(), app.get_drop_tab_pane(), app.get_drop_tab(), app.get_drop_side_row()),
        (app.get_drop_crumb_pane(), app.get_drop_crumb()),
    );
    if now == ((pane, row, tab_pane, tab, side_row), (crumb_pane, crumb)) {
        return;
    }
    app.set_drop_pane(pane);
    app.set_drop_row(row);
    app.set_drop_tab_pane(tab_pane);
    app.set_drop_tab(tab);
    app.set_drop_crumb_pane(crumb_pane);
    app.set_drop_crumb(crumb);
    app.set_drop_side_y(side_y);
    app.set_drop_side_h(side_h);
    app.set_drop_side_row(side_row);
    dnd::repaint_during_own_drag();
}

/// The Windows 11 menu's "Add to bookmarks" (P12-M5): each folder or drive among `names` in `folder`
/// (a file is left out), or with no names the folder itself, appended to the bookmarks; one
/// bookmarked already stays where it is, noted as Ctrl+D notes it.
fn menu_bookmarks(pane: usize, folder: &Path, names: &[String]) {
    let paths: Vec<PathBuf> = if names.is_empty() { vec![folder.to_path_buf()] } else { names.iter().map(|n| folder.join(n)).filter(|p| p.is_dir()).collect() };
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let mut added = false;
        let mut noted = String::new();
        for p in &paths {
            let f = p.to_string_lossy().to_string();
            let at = st.config.bookmarks.len();
            if sidebar::insert_bookmark(&mut st.config.bookmarks, &f, at) {
                eprintln!("[sidebar] bookmark added {f} at {at} (menu)");
                noted = format!("Bookmarked {f}");
                added = true;
            } else {
                eprintln!("[sidebar] {f} is bookmarked already");
                noted = format!("{f} is bookmarked already");
            }
        }
        if added {
            rebuild_sidebar(st);
            schedule_save(st);
        }
        if !noted.is_empty() {
            note(st, &app, pane, noted);
        }
    });
}

/// Folders dropped into the Bookmarks gap (P12-M5): each folder bookmarked at gap `at` in order (a
/// file is left out; a folder bookmarked already stays where it is, noted as Ctrl+D notes it).
pub fn drop_bookmarks(at: usize, sources: Vec<PathBuf>) {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let pane = active_pane(&app);
        let mut k = at;
        let mut noted = String::new();
        for p in sources.iter().filter(|p| p.is_dir()) {
            let folder = p.to_string_lossy().to_string();
            if sidebar::insert_bookmark(&mut st.config.bookmarks, &folder, k) {
                eprintln!("[sidebar] bookmark added {folder} at {k} (drop)");
                noted = format!("Bookmarked {folder}");
                k += 1;
            } else {
                eprintln!("[sidebar] {folder} is bookmarked already");
                noted = format!("{folder} is bookmarked already");
            }
        }
        if k > at {
            rebuild_sidebar(st);
            schedule_save(st);
        }
        if !noted.is_empty() {
            note(st, &app, pane, noted);
        }
    });
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

/// A drag held over a folder row or tile (`dnd::Spring`): that pane's active tab opens it, as a
/// click would. Called from DragOver, which may run inside our own drag loop, so the navigation
/// runs from the event loop, as the closures that loop already runs do (they take `with_state`
/// and open no modal loop).
pub fn spring_open(pane: usize, dest: PathBuf) {
    let _ = slint::invoke_from_event_loop(move || navigate(pane, dest));
}

/// A drag held over tab `index` of the pane's strip: it becomes the pane's active tab, as a
/// click on it does (`tab-select`), through the event loop as `spring_open` does.
pub fn spring_tab(pane: usize, index: usize) {
    let _ = slint::invoke_from_event_loop(move || {
        switch_tab(pane, |t| {
            t.select(index);
        })
    });
}

/// A virtual-file drop's progress or result (posted through the event loop by the drop target or
/// its worker): the note of the pane it was dropped on.
pub fn drop_note(pane: usize, text: String) {
    with_state(|st| {
        if let Some(app) = st.app.upgrade() {
            note(st, &app, pane, text);
        }
    });
}

/// For the scripted driver's in-menu steps: no borrow of the app state is held right now.
pub fn state_is_free() -> bool {
    STATE.with(|s| s.try_borrow_mut().is_ok())
}

/// For the scripted driver: a job is in flight, a tab is still re-enumerating, or a virtual-file
/// drop is still writing.
pub fn ops_busy() -> bool {
    if dnd::virtual_drops_running() || verbs::busy() {
        return true;
    }
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
    st.tab_mut(pane).shown_at = Instant::now();
    sync_grid(st, app, pane);
    let t = st.tab(pane);
    let (model, cursor, scroll) = (t.model.clone(), t.cursor, t.scroll);
    let rows = ModelRc::from(model.clone());
    if pane == 0 { app.set_left_rows(rows) } else { app.set_right_rows(rows) }
    // A sizes view (P13-M3): its own list over the same rows.
    let sizes = t.sizeview.as_ref().map(|s| ModelRc::new(sizeview::SizeModel::new(model.clone(), s.extra.clone())));
    let on = sizes.is_some();
    if let Some(m) = sizes {
        if pane == 0 { app.set_left_size_rows(m) } else { app.set_right_size_rows(m) }
    }
    if pane == 0 { app.set_left_sizes(on) } else { app.set_right_sizes(on) }
    model.set_grid_n(st.grid_n[pane]);
    model.set_grid_cursor(cursor);
    let grid = ModelRc::new(GridModel::new(model.clone()));
    if pane == 0 { app.set_left_grid_rows(grid) } else { app.set_right_grid_rows(grid) }
    let (col, asc) = (model.sort_col.get() as i32, model.sort_asc.get());
    if pane == 0 {
        app.set_left_sort_col(col);
        app.set_left_sort_asc(asc);
    } else {
        app.set_right_sort_col(col);
        app.set_right_sort_asc(asc);
    }
    set_path(st, app, pane);
    note_recent(st, app, pane);
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
    restat_results(st, app, pane);
    // Another tab under the same cursor row number is another file (P8-M1).
    preview::kick();
}

/// A watched background tab coming to the front: its watch says nothing when its folder went
/// away in a way the shallow watch cannot see, so check the folder is still there (one metadata
/// call; never on a network path). Gone: stale, so it refreshes and shows the error.
fn check_gone(st: &mut State, pane: usize) {
    let t = st.tab_mut(pane);
    if t.watched && !t.stale && folder_tab(t) && !fs::watch::is_remote(&t.path) && std::fs::metadata(&t.path).is_err() {
        t.stale = true;
        eprintln!("[watch] pane={pane} tab={} folder gone on show: refresh", t.id);
    }
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
        // It keeps a watch if the watch set has room for it (`watch_tabs`, from `show_tab`), a
        // subtree watch while it has folder sizes, which it keeps; else it goes stale.
        if let Some(t) = st.panes[pane].iter_mut().nth(before) {
            t.background_sizes(pane);
        }
        check_gone(st, pane);
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
        // Leaves the foreground, watched in the background if the set has room (see switch_tab).
        st.tab_mut(pane).background_sizes(pane);
        let cur = st.tab(pane);
        let (col, asc, show) = (cur.model.sort_col.get(), cur.model.sort_asc.get(), cur.model.show());
        let path = path.unwrap_or_else(|| cur.path.clone());
        let t = st.new_tab(path.clone());
        t.model.sort_col.set(col);
        t.model.sort_asc.set(asc);
        t.model.show_hidden.set(show.hidden);
        t.model.show_protected.set(show.protected);
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
        if let Some(sv) = &closed.sizeview {
            if sv.done_ms.is_none() && !sv.cancel.swap(true, Ordering::Relaxed) {
                eprintln!("[sizeview] pane={pane} gen={} cancelled (tab closed)", sv.generation);
            }
        }
        closed.drop_sizes(pane, "tab closed");
        st.watch.unwatch(closed.id);
        eprintln!("[tab] pane={pane} closed tab {} (id {}), {} left", i + 1, closed.id, st.panes[pane].len());
        schedule_save(st);
        if was_active {
            check_gone(st, pane);
            show_tab(st, &app, pane);
            Some(st.tab(pane).stale)
        } else {
            // Its slot may go to a background tab.
            st.watch_tabs(&app);
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
        // The "+" after a pane's tab strip (P12-M5): that pane becomes active, then Ctrl+T's tab.
        "plus" => {
            eprintln!("[tab] pane={pane} + button: new tab");
            if let Some(app) = with_state(|st| st.app.upgrade()) {
                app.invoke_focus_pane(pane as i32);
            }
            open_tab(pane, None);
        }
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
        // Paste applies with files on the clipboard (the toolbar), whoever put them there.
        let files = toolbar::clipboard_has_files();
        if files != st.clip_files {
            st.clip_files = files;
            eprintln!("[toolbar] clipboard {} files", if files { "holds" } else { "holds no" });
            if let Some(app) = st.app.upgrade() {
                update_tools(st, &app, 0);
                update_tools(st, &app, 1);
            }
        }
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
    with_state(|st| st.cloud.reload());
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

/// The settings dialog's protected-files switch for a pane (P6-M3): every tab of the pane follows
/// it, the active one at once, the others when next listed (their pass runs on show).
fn set_protected(pane: usize, on: bool) {
    with_state(|st| {
        if pane == 0 { st.config.show_protected_left = on } else { st.config.show_protected_right = on }
        for t in st.panes[pane].iter() {
            t.model.show_protected.set(on);
        }
        eprintln!("[hidden] pane={pane} show_protected={on}");
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

/// Down in the search box: apply the text at once (an empty one closes the box).
fn filter_apply(pane: usize, text: SharedString) {
    let Some((app, id)) = with_state(|st| {
        st.filter_timer[pane].stop();
        Some((st.app.upgrade()?, st.tab(pane).id))
    }) else {
        return;
    };
    apply_filter(pane, id, &text);
    if text.is_empty() {
        set_filter_box(&app, pane, "");
    }
}

/// Escape in the filter box or its clear glyph (the box has closed itself): no filter, at once.
fn filter_clear(pane: usize) {
    let id = with_state(|st| {
        st.filter_timer[pane].stop();
        st.tab(pane).id
    });
    apply_filter(pane, id, "");
}

/// Return in the search box (P10-M2): the recursive name search for its text, in a new tab; with a
/// blank box the list takes the focus (searchbox.rs `on_return`).
fn filter_return(pane: usize, text: SharedString) {
    match searchbox::on_return(&text) {
        searchbox::ReturnAction::Search(pattern) => {
            let content = content_mode(pane);
            eprintln!("[search] pane={pane} box Return: searches \"{pattern}\"{}", if content { " inside files" } else { "" });
            start_search(pane, &pattern, content, None);
        }
        searchbox::ReturnAction::FocusList => {
            eprintln!("[search] pane={pane} box Return: empty, the list takes the focus");
            if let Some(app) = with_state(|st| st.app.upgrade()) {
                app.invoke_focus_pane(pane as i32);
            }
        }
    }
}

/// The search box's button (P10-M2): Ctrl+Shift+F's search for the box's text.
fn search_button(pane: usize) {
    eprintln!("[search] pane={pane} search button");
    search_key(pane);
}

/// The pane's search box is in content mode (P11-M1, its toggle): Return, the button and
/// Ctrl+Shift+F search inside files, and typing filters nothing.
fn content_mode(pane: usize) -> bool {
    with_state(|st| st.app.upgrade()).is_some_and(|app| if pane == 0 { app.get_left_content_mode() } else { app.get_right_content_mode() })
}

/// The search box's content toggle (P11-M1), after it flipped: into content mode the tab's name
/// filter goes (the text stays in the box, to be searched for); back to name mode the box's text
/// filters the listing again, at once.
fn content_toggled(pane: usize, on: bool) {
    let Some((app, id)) = with_state(|st| {
        st.filter_timer[pane].stop();
        Some((st.app.upgrade()?, st.tab(pane).id))
    }) else {
        return;
    };
    let text = if pane == 0 { app.get_left_filter_text() } else { app.get_right_filter_text() };
    eprintln!("[search] pane={pane} content mode {}", if on { "on" } else { "off" });
    apply_filter(pane, id, if on { "" } else { text.as_str() });
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

/// Ctrl+Shift+F, the box's button and the driver's `search`: search the active tab's folder and
/// everything under it for the search box's text. With a blank box, focus the box: Return there
/// searches (P10-M2; until then the box was "armed" for that Return, now Return always searches).
fn search_key(pane: usize) {
    let Some(app) = with_state(|st| st.app.upgrade()) else { return };
    let text = if pane == 0 { app.get_left_filter_text() } else { app.get_right_filter_text() };
    if text.trim().is_empty() {
        eprintln!("[search] pane={pane} no pattern yet: the search box takes the focus, Return searches");
        return app.invoke_open_filter(pane as i32);
    }
    start_search(pane, text.trim(), content_mode(pane), None);
}

/// Start a recursive name search for `pattern` (with `content`, a search for that text inside the
/// files, P11-M1) under the active tab's folder: in a new tab after
/// it, or in place when the active tab is a search tab already (its old search is cancelled). The
/// search worker sends its matches through the enumeration channel with the tab's generation, so
/// they stream in as a listing's batches do.
///
/// `history_root`: a search taken from the tab's history (Back or Forward): it runs again in this
/// tab under that root, and nothing is pushed. Otherwise the search is a navigation: a new search
/// tab starts with its root folder on its back stack (it was searched from there), and a new search
/// in a search tab pushes the old one.
fn start_search(pane: usize, pattern: &str, content: bool, history_root: Option<PathBuf>) {
    let started = with_state(|st| {
        let app = st.app.upgrade()?;
        st.filter_timer[pane].stop();
        if is_this_pc(&st.tab(pane).path.to_string_lossy()) {
            note(st, &app, pane, "This PC lists drives; open a folder to search in it".into());
            return None;
        }
        // A content search never reads a cloud location (a read downloads the file) nor inside an
        // archive (P11-M1).
        let root = history_root.clone().unwrap_or_else(|| st.tab(pane).path.clone());
        if content {
            let refusal = if fs::archive::inside(&root) {
                Some("A search inside files does not read inside archives")
            } else if st.cloud.of(&root) != icons::Cloud::None {
                Some("A search inside files does not read cloud folders: reading a file there downloads it")
            } else {
                None
            };
            if let Some(why) = refusal {
                eprintln!("[search] pane={pane} content \"{pattern}\" in {}: refused ({why})", root.display());
                note(st, &app, pane, why.into());
                return None;
            }
        }
        let to = |root: &Path| Place::search(root, pattern, content);
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
            // Leaves the foreground, watched in the background if the set has room (see
            // switch_tab).
            st.tab_mut(pane).background_sizes(pane);
            let cur = st.tab(pane);
            let (col, asc, show, root) = (cur.model.sort_col.get(), cur.model.sort_asc.get(), cur.model.show(), cur.path.clone());
            let mut t = st.new_tab(root.clone());
            t.model.sort_col.set(col);
            t.model.sort_asc.set(asc);
            t.model.show_hidden.set(show.hidden);
            t.model.show_protected.set(show.protected);
            if t.history.push(&Place::folder(root.clone()), &to(&root)) {
                eprintln!("[nav] pane={pane} history push {} ({} back)", root.display(), t.history.back_len());
            }
            let id = t.id;
            st.panes[pane].open(t);
            let tabs = &st.panes[pane];
            eprintln!("[tab] pane={pane} opened tab {} of {} (id {id}) on {} for a search", tabs.active_index() + 1, tabs.len(), root.display());
        }
        let generation = st.generation();
        let (matcher, spec) = search_spec(st, pattern, content);
        let cancel = Arc::new(AtomicU8::new(fs::enumerate::SEARCH_RUNNING));
        let t = st.tab_mut(pane);
        if let Some(old) = t.search.take() {
            old.cancel(fs::enumerate::SEARCH_REPLACED);
        }
        t.search = Some(Search { pattern: NameMatch::new(pattern), content, cancel: cancel.clone(), restating: false });
        t.model.search_names.set(true);
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
        let (id, root, hidden) = (t.id, t.path.clone(), t.model.show());
        if st.cut_source == Some((pane, id)) {
            st.cut_source = None;
        }
        st.icon_gens[pane].store(generation, Ordering::Relaxed);
        show_tab(st, &app, pane);
        schedule_save(st);
        let what = if content { "content" } else { "start" };
        eprintln!("[search] pane={pane} gen={generation} {what} \"{pattern}\" in {}", root.display());
        let thumbs = search_thumbnails(st, &root);
        fs::enumerate::spawn_search(root, matcher, hidden, thumbs, spec, cancel, pane, generation, st.tx.clone(), wake);
        Some(app)
    });
    // The search started from the filter box (Ctrl+Shift+F there, or Return with a search waiting)
    // or from the list: either way the keys go to the results list now. Outside the state borrow:
    // moving the focus runs the pane's handlers.
    if let Some(app) = started {
        app.invoke_focus_pane(pane as i32);
    }
}

/// "Show sizes" (the Windows 11 menu, the driver's `sizeview`): a sizes tab on `root` after the
/// active one (P13-M3). This PC sizes every fixed drive but Google Drive's; inside an archive and on
/// Google Drive's volume it is refused (its sizes are not space on this PC, and a walk there may
/// list remotely).
fn open_sizeview(pane: usize, root: PathBuf) {
    let started = with_state(|st| {
        let app = st.app.upgrade()?;
        let this_pc = is_this_pc(&root.to_string_lossy());
        let refusal = if this_pc {
            None
        } else if fs::archive::inside(&root) {
            Some("The sizes view does not read inside archives")
        } else if st.cloud.of(&root) == icons::Cloud::Overlays {
            Some("The sizes view does not walk Google Drive: its sizes are not space on this PC")
        } else {
            None
        };
        if let Some(why) = refusal {
            eprintln!("[sizeview] pane={pane} {}: refused ({why})", root.display());
            note(st, &app, pane, why.into());
            return None;
        }
        save_view(st, &app, pane);
        st.tab_mut(pane).background_sizes(pane);
        let root = if this_pc { PathBuf::from(THIS_PC) } else { root };
        let mut t = st.new_tab(root.clone());
        t.sizeview = Some(SizeTab::default());
        let id = t.id;
        st.panes[pane].open(t);
        let tabs = &st.panes[pane];
        eprintln!("[tab] pane={pane} opened tab {} of {} (id {id}) on {} for its sizes", tabs.active_index() + 1, tabs.len(), root.display());
        start_sizes(st, &app, pane);
        Some(app)
    });
    if let Some(app) = started {
        app.invoke_focus_pane(pane as i32);
    }
}

/// (Re)start the walk of pane `pane`'s active tab, a sizes tab (opened, F5, restored, after a file
/// operation that was not a removal): a new tree, nothing opened, the root's files listed.
fn start_sizes(st: &mut State, app: &App, pane: usize) {
    let generation = st.generation();
    let root = st.tab(pane).path.clone();
    let this_pc = is_this_pc(&root.to_string_lossy());
    let drives: Vec<PathBuf> = if this_pc {
        sizeview::fixed_drives().into_iter().filter(|d| st.cloud.of(d) != icons::Cloud::Overlays).collect()
    } else {
        Vec::new()
    };
    let mut tree = sizeview::Tree::new(if this_pc { THIS_PC.into() } else { root.to_string_lossy() }.as_ref(), this_pc);
    let roots: Vec<(sizeview::Id, PathBuf)> =
        if this_pc { drives.iter().map(|d| (tree.add_folder(0, &d.to_string_lossy()), d.clone())).collect() } else { vec![(0, root.clone())] };
    let tree = Arc::new(std::sync::Mutex::new(tree));
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let t = st.tab_mut(pane);
    if let Some(old) = &t.sizeview {
        if old.generation != 0 && old.done_ms.is_none() && !old.cancel.swap(true, Ordering::Relaxed) {
            eprintln!("[sizeview] pane={pane} gen={} cancelled (a new walk)", old.generation);
        }
    }
    let id = t.id;
    t.sizeview = Some(SizeTab { generation, tree: tree.clone(), cancel: cancel.clone(), ws_before: perf::memory_mb().0, ..Default::default() });
    t.model.generation.set(generation);
    t.model.clear();
    t.timing = PaneTiming { requested: Some(Instant::now()), ..Default::default() };
    t.note.clear();
    t.cursor = -1;
    t.scroll = 0.0;
    t.stale = false;
    t.free = None;
    if st.cut_source == Some((pane, id)) {
        st.cut_source = None;
    }
    st.icon_gens[pane].store(generation, Ordering::Relaxed);
    show_tab(st, app, pane);
    schedule_save(st);
    eprintln!("[sizeview] pane={pane} gen={generation} start {}", root.display());
    if !this_pc {
        list_level(st, pane, 0);
    }
    std::thread::Builder::new()
        .name("sizeview".into())
        .spawn(move || {
            // No "insert a disk" dialog for an empty drive met on the way.
            // SAFETY: plain flag call on this (worker) thread; the old mode is not needed.
            unsafe {
                windows_sys::Win32::System::Diagnostics::Debug::SetThreadErrorMode(
                    windows_sys::Win32::System::Diagnostics::Debug::SEM_FAILCRITICALERRORS,
                    std::ptr::null_mut(),
                )
            };
            sizeview::walk(&tree, &roots, &cancel, |p| {
                let end = match p {
                    sizeview::Progress::Partial => None,
                    sizeview::Progress::Done { ms, ids } => Some((ms, ids)),
                    sizeview::Progress::Cancelled => return,
                };
                let _ = slint::invoke_from_event_loop(move || sizes_tick(pane, id, generation, end));
            });
        })
        .expect("spawn sizeview thread");
}

/// A sizes walk's partial totals (`end` None, at most every 250 ms) or its end, on the UI thread:
/// logged, and the tab's rows built again from the tree.
fn sizes_tick(pane: usize, id: u64, generation: u64, end: Option<(u64, usize)>) {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let Some(t) = st.find(pane, id) else { return };
        let Some(sv) = t.sizeview.as_mut().filter(|s| s.generation == generation) else { return };
        let (root, kept) = {
            let tree = sv.tree.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            (*tree.root(), tree.nodes.len())
        };
        match end {
            None => eprintln!("[sizeview] pane={pane} gen={generation} partial: {} files, {} bytes on disk", root.files, root.disk),
            Some((ms, ids)) => {
                sv.done_ms = Some(ms);
                t.timing.done_ms = Some(ms as f64);
                let (ws, private) = perf::memory_mb();
                eprintln!(
                    "[sizeview] pane={pane} gen={generation} done: {} bytes on disk ({} logical), {} files, {} folders, {} skipped, in {ms} ms; working set {ws:.0} MB ({:+.0} MB since the start), private {private:.0} MB, {kept} folders and {ids} file IDs kept",
                    root.disk,
                    root.logical,
                    root.files,
                    root.folders,
                    root.skipped,
                    ws - sv.ws_before
                );
            }
        }
        rebuild_sizes(st, &app, pane, id);
    });
}

/// Build a sizes tab's rows again from its tree (a tick, a level opened or listed, a removal): the
/// order and numbers change, the cursor and the selection stay on their items (by path).
fn rebuild_sizes(st: &mut State, app: &App, pane: usize, id: u64) {
    let active = st.is_active(pane, id);
    let cursor_now = if active { current_row(app, pane) } else { st.find(pane, id).map_or(-1, |t| t.cursor) };
    let y = active.then(|| app.invoke_get_scroll(pane as i32));
    let zone = model::LocalZone::current();
    let Some(t) = st.find(pane, id) else { return };
    let root = t.path.clone();
    let Some(sv) = t.sizeview.as_mut() else { return };
    let (entries, extra) = {
        let tree = sv.tree.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        sv.rows = sizeview::rows(&tree, &sv.open, &sv.files);
        sizeview::entries(&tree, &sv.files, &root, &sv.rows, &zone)
    };
    let r = t.model.capture(cursor_now);
    *sv.extra.borrow_mut() = extra;
    let cursor = t.model.set_rows(entries, &r);
    if !active {
        t.cursor = cursor.map_or(-1, |c| c as i32);
        return;
    }
    set_current_row(app, pane, cursor);
    if let Some(y) = y {
        app.invoke_set_scroll(pane as i32, y);
    }
    update_status(st, app, pane);
    st.strip_dirty[pane] = true;
    resolve_view(st, app, pane);
}

/// List level `node` of pane `pane`'s sizes tab (its files, each with its size on disk) on a thread
/// of its own; `land_level` puts them in.
fn list_level(st: &mut State, pane: usize, node: sizeview::Id) {
    let t = st.tab_mut(pane);
    let (id, root) = (t.id, t.path.clone());
    let Some(sv) = t.sizeview.as_mut() else { return };
    if !sv.listing.insert(node) {
        return;
    }
    let dir = if node == 0 { root } else { root.join(sv.tree.lock().unwrap_or_else(std::sync::PoisonError::into_inner).rel(node)) };
    let generation = sv.generation;
    std::thread::spawn(move || {
        let t0 = Instant::now();
        let res = sizeview::level_files(&dir);
        let ms = perf::elapsed_ms(t0);
        let _ = slint::invoke_from_event_loop(move || land_level(pane, id, generation, node, dir, res, ms));
    });
}

fn land_level(pane: usize, id: u64, generation: u64, node: sizeview::Id, dir: PathBuf, res: Result<Vec<sizeview::File>, u32>, ms: f64) {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let Some(sv) = st.find(pane, id).and_then(|t| t.sizeview.as_mut()).filter(|s| s.generation == generation) else { return };
        sv.listing.remove(&node);
        match res {
            Ok(files) => {
                eprintln!("[sizeview] pane={pane} listed {}: {} files in {ms:.1} ms", dir.display(), files.len());
                sv.files.insert(node, files);
            }
            Err(e) => eprintln!("[sizeview] pane={pane} could not list {}: os error {e}", dir.display()),
        }
        rebuild_sizes(st, &app, pane, id);
    });
}

/// The ▸ of row `row` (a click, Right or Left, the driver's `sizeopen`): `open` None flips it. A
/// folder opens in place one level down (its subfolders at once, from the tree; its files when its
/// listing lands) or closes; Left on a file or a closed folder moves the cursor to its parent's
/// row, as a tree does.
fn size_toggle(pane: usize, row: usize, open: Option<bool>) {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let t = st.tab_mut(pane);
        let (id, root) = (t.id, t.path.clone());
        let Some(sv) = t.sizeview.as_mut() else { return };
        let Some(r) = sv.rows.get(row).cloned() else { return };
        let (parent, node) = {
            let tree = sv.tree.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            match r.item {
                sizeview::Item::Folder(n) => (tree.nodes[n as usize].parent, Some(n)),
                sizeview::Item::File(p, _) => (p, None),
            }
        };
        let is_open = node.is_some_and(|n| sv.open.contains(&n));
        let want = open.unwrap_or(!is_open);
        let Some(node) = node.filter(|_| want != is_open) else {
            // Left on a file or on a closed folder: to its parent's row.
            if open == Some(false) {
                if let Some(i) = sv.rows.iter().position(|x| x.item == sizeview::Item::Folder(parent)) {
                    set_current_row(&app, pane, Some(i));
                    st.model(pane).select(SelOp::Only, i);
                }
            }
            return;
        };
        let path = root.join(&r.rel);
        if want {
            sv.open.insert(node);
            let kids = sv.tree.lock().unwrap_or_else(std::sync::PoisonError::into_inner).children(node).count();
            eprintln!("[sizeview] pane={pane} opened {} in place ({kids} children)", path.display());
            if !sv.files.contains_key(&node) {
                list_level(st, pane, node);
            }
        } else {
            sv.open.remove(&node);
            eprintln!("[sizeview] pane={pane} closed {} in place", path.display());
        }
        rebuild_sizes(st, &app, pane, id);
    });
}

/// A delete or a move out finished (`job_done`): every sizes view showing what went takes it off
/// its totals at once, with no new walk (P13-M3). Returns the panes whose active tab is a sizes
/// view, which therefore need no refresh.
fn sizes_removed(st: &mut State, job: &Job, results: &[fileops::ItemResult]) -> [bool; 2] {
    let gone: Vec<&Path> = match job {
        Job::Delete { .. } | Job::Move { .. } => results.iter().filter(|r| r.ok()).map(|r| r.source.as_path()).collect(),
        _ => Vec::new(),
    };
    let Some(app) = st.app.upgrade() else { return [false; 2] };
    let mut skip = [false; 2];
    for pane in 0..2 {
        skip[pane] = st.tab(pane).sizeview.is_some();
        let ids: Vec<u64> = st.panes[pane].iter().filter(|t| t.sizeview.is_some()).map(|t| t.id).collect();
        for id in ids {
            let Some(t) = st.find(pane, id) else { continue };
            let root = t.path.clone();
            let Some(sv) = t.sizeview.as_mut() else { continue };
            let mut changed = false;
            for p in &gone {
                // A move within the root changes none of the root's totals: not followed (F5 walks).
                if let Job::Move { dest_dir, .. } = job {
                    if sizeview::rel_under(&root, dest_dir).is_some() || same_dir(&root, dest_dir) {
                        continue;
                    }
                }
                let Some(rel) = sizeview::rel_under(&root, p) else { continue };
                let mut tree = sv.tree.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some((disk, k)) = sizeview::remove(&mut tree, &mut sv.files, &rel) {
                    eprintln!("[sizeview] pane={pane} removed {}: {disk} bytes off {k} totals", p.display());
                    changed = true;
                }
            }
            if changed {
                rebuild_sizes(st, &app, pane, id);
            }
        }
    }
    skip
}

/// For the driver's `waitsizeview`: a sizes walk or a level's listing is still out.
pub fn sizes_view_running() -> bool {
    STATE.with(|s| match s.try_borrow() {
        Ok(g) => g.as_ref().is_some_and(|st| {
            st.panes.iter().any(|p| p.iter().any(|t| t.sizeview.as_ref().is_some_and(|s| (s.done_ms.is_none() && s.generation != 0) || !s.listing.is_empty())))
        }),
        Err(_) => true,
    })
}

/// The driver's `sizeview <pane> [<path>]`: the view on `path`, or on the cursor row's folder, or
/// on the pane's own folder.
pub fn script_sizeview(pane: usize, path: Option<PathBuf>) {
    let root = path.or_else(|| {
        with_state(|st| {
            let app = st.app.upgrade()?;
            let folder = st.tab(pane).path.clone();
            let e = usize::try_from(current_row(&app, pane)).ok().and_then(|r| st.model(pane).entry(r));
            Some(match e {
                Some(e) if e.is_dir() => folder.join(e.name.as_str()),
                _ => folder,
            })
        })
    });
    if let Some(root) = root {
        open_sizeview(pane, root);
    }
}

/// The driver's `sizeopen <row>`: the ▸ of that row in the active pane.
pub fn script_sizeopen(row: usize) {
    let pane = with_state(|st| st.app.upgrade().map(|app| active_pane(&app)));
    if let Some(pane) = pane {
        size_toggle(pane, row, None);
    }
}

/// How often a search tab in front states its rows on screen again (P11-M2).
const RESTAT_EVERY: Duration = Duration::from_secs(5);

/// A finished search tab in front of pane `pane` (P11-M2: results follow changes; a watch per
/// result folder would be too many): its rows on screen are statted again on a worker thread, and
/// `land_restat` removes the gone and updates the changed. Called every `RESTAT_EVERY` and when a
/// tab comes to the front; nothing while a re-stat is out or the listing is not settled.
fn restat_results(st: &mut State, app: &App, pane: usize) {
    if pane == 1 && !app.get_split() {
        return;
    }
    // A folder tab (nearly every tick) asks nothing more.
    let t = st.tab(pane);
    if !t.search.as_ref().is_some_and(|s| !s.restating) || unsettled(t).is_some() {
        return;
    }
    let range = visible_rows(st, app, pane);
    let thumbs = search_thumbnails(st, &st.tab(pane).path.clone());
    let t = st.tab_mut(pane);
    let Some((first, last)) = range else { return };
    let rows: Vec<model::Entry> = (first..=last).filter_map(|i| t.model.entry(i)).collect();
    if rows.is_empty() {
        return;
    }
    let s = t.search.as_mut().expect("checked above");
    s.restating = true;
    let (id, root, generation, pass) = (t.id, t.path.clone(), t.model.generation.get(), t.model.pass.get());
    std::thread::spawn(move || {
        let t0 = Instant::now();
        let (mut types, zone) = (fs::enumerate::TypeNames::default(), model::LocalZone::current());
        let fresh: Vec<Option<model::Entry>> = rows
            .iter()
            .map(|r| {
                let (rel, leaf) = r.name.rsplit_once('\\').unwrap_or(("", r.name.as_str()));
                let dir = stat_dir(&root.join(rel));
                let mut keys = icons::IconKeys::new(dir.clone(), thumbs);
                fs::enumerate::stat_entry(&dir, leaf, &mut types, &mut keys, &zone)
            })
            .collect();
        let ms = perf::elapsed_ms(t0);
        let _ = slint::invoke_from_event_loop(move || land_restat(pane, id, generation, pass, rows, fresh, ms));
    });
}

/// A search tab's re-stat came back (on the UI thread): applied when its listing is still the one
/// statted, else dropped (the next tick states again).
fn land_restat(pane: usize, id: u64, generation: u64, pass: u64, rows: Vec<model::Entry>, fresh: Vec<Option<model::Entry>>, stat_ms: f64) {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let active = st.is_active(pane, id);
        let Some(t) = st.find(pane, id) else { return };
        let Some(s) = t.search.as_mut() else { return };
        s.restating = false;
        if t.model.generation.get() != generation || t.model.pass.get() != pass || unsettled(t).is_some() {
            return eprintln!("[search] pane={pane} results re-stated: dropped (the listing changed)");
        }
        let (edits, gone, changed) = model::restat_edits(&rows, fresh);
        eprintln!("[search] pane={pane} results re-stated: {} rows, {gone} gone, {changed} changed, in {stat_ms:.1} ms", rows.len());
        if edits.is_empty() {
            return;
        }
        let cursor = if active { current_row(&app, pane) } else { t.cursor };
        match t.model.apply_edits(edits, cursor) {
            Ok(a) => {
                t.model.set_grid_cursor(a.cursor);
                if !active {
                    t.cursor = a.cursor;
                }
            }
            Err(e) => return eprintln!("[search] pane={pane} results re-stated: not applied ({e})"),
        }
        if active {
            update_status(st, &app, pane);
            resolve_view(st, &app, pane);
        }
    });
}

/// The rows of pane `pane`'s list on screen (entries, the grid's whole grid rows), as
/// `resolve_view` finds them; None when none are.
fn visible_rows(st: &State, app: &App, pane: usize) -> Option<(usize, usize)> {
    let model = &st.panes[pane].active().model;
    let (y, h) = st.view[pane];
    let n = if st.grid[pane] { model.grid_n() } else { 1 };
    let (lines, line_px) = if st.grid[pane] {
        (model.grid_len(), icons::GridGeom::new(app.window().scale_factor(), n, st.tile[pane]).tile.h)
    } else {
        (model.len(), list_geom(st).row_px)
    };
    let visible = icons::visible_range(y, h, lines, line_px)?;
    Some((visible.0 * n, ((visible.1 + 1) * n - 1).min(model.len().checked_sub(1)?)))
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

/// Pane `pane`'s column widths as the window has them (P9-M1).
fn cols_of(app: &App, pane: usize) -> cols::Widths {
    if pane == 0 {
        cols::Widths { ty: app.get_left_col_type(), size: app.get_left_col_size(), modified: app.get_left_col_modified() }
    } else {
        cols::Widths { ty: app.get_right_col_type(), size: app.get_right_col_size(), modified: app.get_right_col_modified() }
    }
}

fn set_cols(app: &App, pane: usize, w: cols::Widths) {
    if pane == 0 {
        app.set_left_col_type(w.ty);
        app.set_left_col_size(w.size);
        app.set_left_col_modified(w.modified);
    } else {
        app.set_right_col_type(w.ty);
        app.set_right_col_size(w.size);
        app.set_right_col_modified(w.modified);
    }
    update_shown(app, pane);
}

/// The columns pane `pane` shows (P13-M1), in `cols::ALL`'s order, as the window has them.
fn shown_of(app: &App, pane: usize) -> [bool; 3] {
    if pane == 0 {
        [app.get_left_shows_type(), app.get_left_shows_size(), app.get_left_shows_modified()]
    } else {
        [app.get_right_shows_type(), app.get_right_shows_size(), app.get_right_shows_modified()]
    }
}

thread_local! {
    /// Each pane's list width as Slint last reported it (pane.slint `list-width`), logical px; 0
    /// before the first layout and while the right pane is hidden.
    static LIST_W: std::cell::Cell<[f32; 2]> = const { std::cell::Cell::new([0.0; 2]) };
}

/// Pane `pane`'s list is `w` px wide now (a window resize, the splitter, the split, the sidebar, the
/// preview column): the columns it shows follow.
fn list_width(app: &App, pane: usize, w: f32) {
    LIST_W.with(|c| {
        let mut a = c.get();
        a[pane] = w;
        c.set(a);
    });
    update_shown(app, pane);
}

/// The columns pane `pane` shows, from its widths, the Type setting and its list's width
/// (`cols::shown`), set on the pane when they change: `[cols] pane=N shows <columns> at W px`. A
/// pane with no width yet (or the right pane hidden) keeps what it has, and so does a pane whose
/// grip is held (the set waits for the release: a column coming back mid-drag would change the
/// grip's neighbour under the pointer, the P13-M1 soak's finding).
fn update_shown(app: &App, pane: usize) {
    let w = LIST_W.with(|c| c.get())[pane];
    // The hidden right pane is laid out at its list's minimum width: nothing to show there.
    if w <= 0.0 || (pane == 1 && !app.get_split()) || COL_DRAG.with(|d| d.get()).is_some_and(|d| d.0 == pane) {
        return;
    }
    let geom = icons::ListGeom::new(app.get_thumbnails(), app.get_comfortable());
    let room = w - geom.name_x() - dnd::ROW_PAD_RIGHT;
    let s = cols::shown(cols_of(app, pane), app.get_show_type(), room);
    if s == shown_of(app, pane) {
        return;
    }
    if pane == 0 {
        app.set_left_shows_type(s[0]);
        app.set_left_shows_size(s[1]);
        app.set_left_shows_modified(s[2]);
    } else {
        app.set_right_shows_type(s[0]);
        app.set_right_shows_size(s[1]);
        app.set_right_shows_modified(s[2]);
    }
    eprintln!("[cols] pane={pane} shows {} at {w:.0} px", cols::shown_text(s));
}

/// The width of pane `pane`'s name cell with widths `w`, as the window is laid out now.
fn name_px(app: &App, pane: usize, w: cols::Widths) -> f32 {
    dnd::name_cell(&window_layout(app), pane, w.taken(shown_of(app, pane))).1
}

/// One `[cols]` line per column whose width differs between `old` and `new`.
fn log_col_changes(pane: usize, old: cols::Widths, new: cols::Widths, why: &str) {
    for c in [cols::Col::Type, cols::Col::Size, cols::Col::Modified] {
        if old.get(c) != new.get(c) {
            eprintln!("[cols] pane={pane} {} {:.0} -> {:.0} px ({why})", c.name(), old.get(c), new.get(c));
        }
    }
}

fn log_widths(app: &App, pane: usize) {
    let w = cols_of(app, pane);
    eprintln!(
        "[cols] pane={pane} widths type {:.0} size {:.0} modified {:.0} (name {:.0} px)",
        w.ty,
        w.size,
        w.modified,
        name_px(app, pane, w)
    );
}

/// Pane `pane`'s widths changed from `old` to `new` (set already): logged, kept for the file.
fn cols_changed(st: &mut State, app: &App, pane: usize, old: cols::Widths, new: cols::Widths, why: &str) {
    if old == new {
        return;
    }
    log_col_changes(pane, old, new, why);
    log_widths(app, pane);
    st.config.set_cols(pane, new);
    schedule_save(st);
}

thread_local! {
    /// The column grip being dragged: pane, grip, the widths and the name cell's width at the press.
    static COL_DRAG: std::cell::Cell<Option<(usize, cols::Col, cols::Widths, f32)>> = const { std::cell::Cell::new(None) };
}

/// A header grip's press (phase 0), move (1) or release (2), `dx` the pointer's travel since the
/// press: the boundary moves from the widths at the press, so the grip stays under the pointer.
fn col_drag(pane: usize, col: i32, dx: f32, phase: i32) {
    let Some(c) = cols::Col::from_index(col) else { return };
    let Some(app) = with_state(|st| st.app.upgrade()) else { return };
    if phase == 0 {
        let w = cols_of(&app, pane);
        COL_DRAG.with(|d| d.set(Some((pane, c, w, name_px(&app, pane, w)))));
        return;
    }
    let Some((p, c0, start, name)) = COL_DRAG.with(|d| d.get()) else { return };
    if (p, c0) != (pane, c) {
        return;
    }
    // The columns shown stay as they were at the press until the release (`update_shown`).
    let w = cols::drag(start, c, dx, shown_of(&app, pane), name);
    set_cols(&app, pane, w);
    if phase == 2 {
        COL_DRAG.with(|d| d.set(None));
        update_shown(&app, pane);
        with_state(|st| cols_changed(st, &app, pane, start, w, "drag"));
    }
}

/// Column `c` of pane `pane` set to `px` by moving its grip: `colwidth` (why "script") and the fit.
fn col_set(app: &App, pane: usize, c: cols::Col, px: f32, why: &str) {
    let start = cols_of(app, pane);
    let w = cols::set_width(start, c, px, shown_of(app, pane), name_px(app, pane, start));
    set_cols(app, pane, w);
    with_state(|st| cols_changed(st, app, pane, start, w, why));
}

/// A double-click on grip `col`: its column fitted to the widest of its cells on screen (and its
/// header label), never the whole listing.
fn col_fit(pane: usize, col: i32) {
    let Some(c) = cols::Col::from_index(col) else { return };
    let Some(app) = with_state(|st| st.app.upgrade()) else { return };
    // The double-click's own press and release are not a drag (the release would put the widths
    // back to those at its press).
    COL_DRAG.with(|d| d.set(None));
    let rows = if pane == 0 { app.get_left_rows() } else { app.get_right_rows() };
    let l = window_layout(&app);
    let top = dnd::list_top(&l, pane);
    let y = -app.invoke_get_scroll(pane as i32);
    let h = (l.height - dnd::LIST_BOTTOM_GAP - top).max(0.0);
    let first = (y / l.row_px).floor().max(0.0) as usize;
    let end = (((y + h) / l.row_px).ceil().max(0.0) as usize).min(rows.row_count());
    let cells: Vec<String> = (first..end)
        .filter_map(|i| rows.row_data(i))
        .map(|r| match c {
            cols::Col::Type => r.type_text,
            cols::Col::Size => r.size,
            cols::Col::Modified => r.modified,
        })
        .map(|t| t.to_string())
        .collect();
    let widest = app.invoke_measure(cells.join("
").into(), false);
    eprintln!("[cols] pane={pane} fit {}: widest of {} rows on screen = {widest:.0} px", c.name(), cells.len());
    let label = match c {
        cols::Col::Type => "Type",
        cols::Col::Size => "Size",
        cols::Col::Modified => "Modified",
    };
    let label = app.invoke_measure(label.into(), true);
    col_set(&app, pane, c, cols::fit_width(widest.max(label)), "fit");
}

/// The Type column on or off in both panes. Off, every tab sorted by Type is sorted by name
/// instead (a header click can no longer reach Type).
fn set_show_type(st: &mut State, app: &App, on: bool) {
    app.set_show_type(on);
    st.config.show_type = on;
    eprintln!("[settings] Type column {}", if on { "shown" } else { "removed from the header and the rows" });
    for pane in 0..2 {
        update_shown(app, pane);
    }
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

/// Thumbnails on or off in both panes: each pane's list becomes the 32 px (or the 22 px) variant
/// and its strip is cleared. The icon keys are made on the enumeration worker, so every tab lists
/// its folder again: the panes on screen at once (the panes returned, for `refresh`), the others
/// when they come to the front.
fn set_thumbnails(st: &mut State, app: &App, on: bool) -> Vec<usize> {
    st.config.thumbnails = on;
    app.set_thumbnails(on);
    log_list_geom(list_geom(st));
    // The name starts 14 px further right beside a thumbnail: the room for the columns changed.
    for pane in 0..2 {
        update_shown(app, pane);
    }
    for pane in 0..2 {
        hide_strip(st, app, pane);
    }
    // New icon keys: every listing again, watched or not.
    mark_background_stale(st, true);
    if app.get_split() { vec![0, 1] } else { vec![0] }
}

/// Comfortable rows on or off (P12-M4): the details lists without thumbnails become 30 px (or 22
/// px) rows. The icons stay 16 px, so their keys stand: the strips are composed again at the new
/// pitch, and each list keeps its first row at the top.
fn set_row_height(st: &mut State, app: &App, comfortable: bool) {
    let old = list_geom(st).row_px;
    st.config.row_height = if comfortable { "comfortable" } else { "compact" }.into();
    app.set_comfortable(comfortable);
    let g = list_geom(st);
    log_list_geom(g);
    for pane in 0..2 {
        hide_strip(st, app, pane);
        if !st.grid[pane] && g.row_px != old {
            let y = st.view[pane].0 * g.row_px / old;
            st.view[pane].0 = y;
            app.invoke_set_scroll(pane as i32, y);
        }
        resolve_view(st, app, pane);
    }
}

/// The details lists' geometry from the settings: thumbnails, comfortable or compact rows.
fn list_geom(st: &State) -> icons::ListGeom {
    icons::ListGeom::new(st.config.thumbnails, st.config.comfortable())
}

fn log_list_geom(g: icons::ListGeom) {
    eprintln!("[icons] list geometry: {} px rows, {} px icons", g.row_px, g.icon_px);
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
            show_protected_left: st.config.show_protected_left,
            show_protected_right: st.config.show_protected_right,
            view_left: view_of(st, 0).name().into(),
            view_right: view_of(st, 1).name().into(),
            show_type: st.config.show_type,
            thumbnails: st.config.thumbnails,
            row_height: if st.config.comfortable() { "comfortable" } else { "compact" }.into(),
            sidebar_visible: app.get_sidebar_visible(),
            path_bar: if app.get_path_entry_sticky() { "entry" } else { "breadcrumb" }.into(),
            toolbar: settings::Toolbar::from_index(app.get_toolbar()).name().into(),
            theme: st.config.theme.clone(),
            preview: app.get_preview(),
            syntax_colours: app.get_syntax_colours(),
            log: st.config.log,
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
    app.set_dlg_protected_left(d.show_protected_left);
    app.set_dlg_protected_right(d.show_protected_right);
    app.set_dlg_view_left(settings::View::from_name(&d.view_left).index());
    app.set_dlg_view_right(settings::View::from_name(&d.view_right).index());
    app.set_dlg_type_column(d.show_type);
    app.set_dlg_thumbnails(d.thumbnails);
    app.set_dlg_row_height(i32::from(d.row_height == "comfortable"));
    app.set_dlg_sidebar(d.sidebar_visible);
    app.set_dlg_crumbs(d.path_bar != "entry");
    app.set_dlg_toolbar(settings::Toolbar::from_name(&d.toolbar).index());
    app.set_dlg_theme(settings::ThemeMode::from_name(&d.theme).index());
    app.set_dlg_preview(d.preview);
    app.set_dlg_start_warning("".into());
    app.set_dlg_syntax_colours(d.syntax_colours);
    app.set_dlg_log(d.log);
    app.set_dlg_cols_reset(false);
    app.set_settings_note(note.into());
    eprintln!("[settings] dialog opened");
    // Outside the state borrow: showing the popup moves keyboard focus.
    app.invoke_show_settings();
}

/// The settings dialog's Browse button (P11-M3): the shell's folder picker on the verb worker,
/// opening at the field's folder (`browse_done` fills it).
fn browse_start(field: &str) {
    with_state(|st| {
        let Some(app) = st.app.upgrade() else { return };
        let text = if field == "start_left" { app.get_dlg_start_left() } else { app.get_dlg_start_right() };
        let hwnd = main_hwnd(st, &app);
        st.next_verb += 1;
        let work = verbs::Work::PickFolder { hwnd, start: PathBuf::from(text.as_str()), field: field.to_string() };
        let queued = st.verbs.submit(verbs::Job { id: st.next_verb, pane: active_pane(&app), work });
        eprintln!("[settings] browse {field}: the folder picker {}", if queued { "queued on the verb worker" } else { "not queued (the worker is gone)" });
    });
}

/// The folder picker answered: the field takes the folder (staged as typing stages it).
fn browse_done(field: &str, path: Option<PathBuf>, error: Option<String>) {
    let Some(app) = with_state(|st| st.app.upgrade()) else { return };
    match (path, error) {
        (Some(p), _) => {
            eprintln!("[settings] browse {field}: {}", p.display());
            app.invoke_settings_stage(field.into(), p.to_string_lossy().as_ref().into());
        }
        (None, Some(e)) => eprintln!("[settings] browse {field}: failed: {e}"),
        (None, None) => eprintln!("[settings] browse {field}: cancelled"),
    }
}

/// A start folder typed, picked or set by the driver (P11-M3): the dialog's footer says when it is
/// not a folder. No check for a network path (a dead share would stall the window here).
fn start_folder_check(field: &str, text: &str) {
    let Some(app) = with_state(|st| st.app.upgrade()) else { return };
    let side = if field == "start_left" { "Left" } else { "Right" };
    let t = text.trim();
    let missing = !t.is_empty() && !is_this_pc(t) && !t.starts_with(r"\\") && !fs::watch::is_remote(Path::new(t)) && !Path::new(t).is_dir();
    let warning = if missing { format!("{side} start folder not found: {t}") } else { String::new() };
    if missing {
        eprintln!("[settings] dialog: {field} \"{t}\" is not a folder");
    }
    if app.get_dlg_start_warning() != warning.as_str() {
        app.set_dlg_start_warning(warning.into());
    }
}

/// The folder the log is written to (P11-M7): the settings file's; none in a scripted run without
/// `TWINFM_CONFIG`.
fn log_dir(st: &State) -> Option<PathBuf> {
    st.config_path.as_deref().and_then(Path::parent).map(Path::to_path_buf)
}

/// The log file's first line: the version and the start time.
fn log_first_line() -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    format!("TwinFM {} log, started {}", env!("CARGO_PKG_VERSION"), model::LocalZone::current().format(now))
}

fn log_start(dir: &Path) {
    match logfile::start(dir, &log_first_line()) {
        Ok(p) => eprintln!("[log] writing to {} (the last run's kept as {})", p.display(), logfile::PREVIOUS),
        Err(e) => eprintln!("[log] cannot write to {}: {e}", dir.display()),
    }
}

/// The log setting applied (the dialog's OK): on writes `twinfm.log` from now, off gives the lines
/// back to wherever they went at start.
fn set_log(st: &State, on: bool) {
    match (on, log_dir(st)) {
        (true, Some(dir)) => log_start(&dir),
        (true, None) => eprintln!("[log] not written: no settings file in a scripted run"),
        (false, _) => {
            if let Some(p) = logfile::stop() {
                eprintln!("[log] stopped writing to {}", p.display());
            }
        }
    }
}

/// The dialog's "Open log folder" (P11-M7): the dialog closes as OK does, and the active pane shows
/// the log's folder with the log selected (the folder alone before a log was written).
fn open_log_folder() {
    let Some((app, dir)) = with_state(|st| Some((st.app.upgrade()?, log_dir(st)?))) else { return };
    app.invoke_settings_ok();
    let file = dir.join(logfile::NAME);
    eprintln!("[log] open log folder: {}", dir.display());
    slint::Timer::single_shot(Duration::ZERO, move || if file.exists() { show_in_pane(&file, true) } else { show_in_pane(&dir, false) });
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
        show_protected_left: app.get_dlg_protected_left(),
        show_protected_right: app.get_dlg_protected_right(),
        view_left: settings::View::from_index(app.get_dlg_view_left()).name().into(),
        view_right: settings::View::from_index(app.get_dlg_view_right()).name().into(),
        show_type: app.get_dlg_type_column(),
        thumbnails: app.get_dlg_thumbnails(),
        row_height: if app.get_dlg_row_height() == 1 { "comfortable" } else { "compact" }.into(),
        sidebar_visible: app.get_dlg_sidebar(),
        path_bar: if app.get_dlg_crumbs() { "breadcrumb" } else { "entry" }.into(),
        toolbar: settings::Toolbar::from_index(app.get_dlg_toolbar()).name().into(),
        theme: settings::ThemeMode::from_index(app.get_dlg_theme()).name().into(),
        preview: app.get_dlg_preview(),
        syntax_colours: app.get_dlg_syntax_colours(),
        log: app.get_dlg_log(),
    }
    .normalized();
    let changes = opened.changes(&staged);
    // "Reset column widths" (P11-M2), counted as a change of its own.
    let reset = app.get_dlg_cols_reset();
    if !ok {
        return eprintln!("[settings] dialog closed (cancel), {} change(s) discarded", changes.len() + usize::from(reset));
    }
    if reset {
        eprintln!("[settings] dialog: cols_reset");
        with_state(|st| {
            for pane in 0..2 {
                let old = cols_of(&app, pane);
                set_cols(&app, pane, cols::DEFAULT);
                eprintln!("[cols] pane={pane} widths reset to {:.0}, {:.0}, {:.0}", cols::DEFAULT.ty, cols::DEFAULT.size, cols::DEFAULT.modified);
                cols_changed(st, &app, pane, old, cols::DEFAULT, "reset");
            }
        });
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
            "show_protected_left" | "show_protected_right" => {
                let pane = usize::from(*key == "show_protected_right");
                set_protected(pane, if pane == 0 { staged.show_protected_left } else { staged.show_protected_right });
            }
            "view_left" | "view_right" => set_view(usize::from(*key == "view_right"), settings::View::from_name(new)),
            "show_type" => with_state(|st| set_show_type(st, &app, staged.show_type)),
            "thumbnails" => {
                for pane in with_state(|st| set_thumbnails(st, &app, staged.thumbnails)) {
                    refresh(pane);
                }
            }
            "row_height" => with_state(|st| set_row_height(st, &app, staged.row_height == "comfortable")),
            "sidebar_visible" => {
                if app.get_sidebar_visible() != staged.sidebar_visible {
                    app.invoke_toggle_sidebar();
                }
            }
            "path_bar" => app.set_path_entry_sticky(staged.path_bar == "entry"),
            "toolbar" => {
                let t = settings::Toolbar::from_name(&staged.toolbar);
                app.set_toolbar(t.index());
                with_state(|st| st.config.toolbar = t.name().into());
            }
            "theme" => {
                let m = settings::ThemeMode::from_name(&staged.theme);
                apply_theme(&app, m);
                with_state(|st| st.config.theme = m.name().into());
            }
            "preview" => {
                if app.get_preview() != staged.preview {
                    toggle_preview();
                }
            }
            "syntax_colours" => {
                app.set_syntax_colours(staged.syntax_colours);
                preview::recolour();
            }
            "log" => with_state(|st| {
                st.config.log = staged.log;
                set_log(st, staged.log);
            }),
            _ => {}
        }
    }
    if !changes.is_empty() {
        with_state(|st| schedule_save(st));
    }
    eprintln!("[settings] dialog closed (ok), {} change(s)", changes.len() + usize::from(reset));
}

/// The colour scheme from the `theme` setting (P10-M1 task 7), live: light or dark set the fluent
/// `Palette.color-scheme`; system hands it back to Windows' app mode (unknown, which the style
/// resolves from the OS and follows again). `TWINFM_SCHEME` (a scripted screenshot) takes
/// precedence, so a run under it ignores the setting.
fn apply_theme(app: &App, mode: settings::ThemeMode) {
    use slint::private_unstable_api::re_exports::ColorScheme;
    if std::env::var("TWINFM_SCHEME").is_ok() {
        return;
    }
    // System: Windows' app mode as it is now. (Setting the scheme to unknown leaves the fluent
    // palette following the OS but loses Slint's default text colour in dark mode, so the
    // resolved mode is set instead; at start a system setting sets nothing, so the style's own
    // binding follows the OS live as it always did, and a switch back to system at runtime keeps
    // the mode Windows had at that moment until the next start: P10-M3.)
    app.global::<Palette>().set_color_scheme(match mode {
        settings::ThemeMode::Light => ColorScheme::Light,
        settings::ThemeMode::Dark => ColorScheme::Dark,
        settings::ThemeMode::System => if system_dark() { ColorScheme::Dark } else { ColorScheme::Light },
    });
    // The title bar too, once the window exists (at start `main_hwnd` sets it when it first
    // fetches the handle).
    // At start the state is not made yet (`main_hwnd` sets the bar when it first fetches the
    // handle); afterwards the handle is known.
    let hwnd = STATE.with(|s| s.try_borrow().ok().and_then(|g| g.as_ref().map(|st| st.hwnd)).unwrap_or(0));
    if hwnd != 0 {
        set_title_bar(hwnd, mode);
    }
}

/// The mode the title bar follows: `TWINFM_SCHEME` when set (as the scheme does), else the
/// setting.
fn title_bar_mode(setting: &str) -> settings::ThemeMode {
    match std::env::var("TWINFM_SCHEME").as_deref() {
        Ok("light") => settings::ThemeMode::Light,
        Ok("dark") => settings::ThemeMode::Dark,
        _ => settings::ThemeMode::from_name(setting),
    }
}

/// Windows' app mode: `AppsUseLightTheme` under the Personalize key, 0 meaning dark (missing:
/// light, Windows' own default).
fn system_dark() -> bool {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0".encode_utf16().collect();
    let name: Vec<u16> = "AppsUseLightTheme\0".encode_utf16().collect();
    let mut value: u32 = 1;
    let mut size: u32 = 4;
    let rc = unsafe { RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(), &mut value as *mut u32 as *mut _, &mut size) };
    rc == 0 && value == 0
}

/// Draw the native title bar dark or light for `mode` (P10-M3): `DWMWA_USE_IMMERSIVE_DARK_MODE`,
/// which Windows 11 honours at once. System mode asks for what Windows' app mode would draw.
fn set_title_bar(hwnd: isize, mode: settings::ThemeMode) {
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
    let dark = mode.title_bar_dark(system_dark());
    let value: i32 = i32::from(dark);
    let hr = unsafe { DwmSetWindowAttribute(hwnd as windows_sys::Win32::Foundation::HWND, DWMWA_USE_IMMERSIVE_DARK_MODE as u32, &value as *const i32 as *const _, 4) };
    eprintln!("[settings] title bar: {} ({}){}", if dark { "dark" } else { "light" }, mode.name(), if hr == 0 { String::new() } else { format!(": DwmSetWindowAttribute = 0x{hr:08X}") });
}

/// For the scripted driver's `waitsize`: a folder size is still queued or walking.
pub fn sizes_busy() -> bool {
    STATE.with(|s| match s.try_borrow() {
        // A stale size waiting for its re-walk counts too (P11-M3).
        Ok(g) => g.as_ref().is_some_and(|st| st.panes.iter().any(|p| p.iter().any(|t| !t.sizes.pending.is_empty() || !t.sizes.stale.is_empty()))),
        Err(_) => true,
    })
}

/// For the scripted driver: watch-driven refreshes completed so far.
pub fn watch_refreshes() -> u64 {
    STATE.with(|s| s.try_borrow().ok().and_then(|g| g.as_ref().map(|st| st.watch_refreshes)).unwrap_or(0))
}

/// For the scripted driver: watch windows applied as row edits so far.
pub fn watch_edits() -> u64 {
    STATE.with(|s| s.try_borrow().ok().and_then(|g| g.as_ref().map(|st| st.watch_edits)).unwrap_or(0))
}

/// `name` in the pane's folder for the driver's file steps, or None (logged): no such pane, This
/// PC, or under TWINFM_SOAK=1 a path outside the soak fence.
fn script_path(pane: usize, name: &str) -> Option<PathBuf> {
    let Some((folder, soak)) = with_state(|st| Some((st.panes.get(pane)?.active().path.clone(), st.soak.clone()))) else {
        eprintln!("[auto] no pane {pane}");
        return None;
    };
    if is_this_pc(&folder.to_string_lossy()) {
        eprintln!("[auto] pane {pane} lists drives; no file there");
        return None;
    }
    let path = folder.join(name);
    if let Some(roots) = soak {
        if !roots.iter().any(|r| inside(&path, r)) {
            eprintln!("[auto] soak: refused {} (outside the soak folders)", path.display());
            return None;
        }
    }
    Some(path)
}

/// For the scripted driver's `mvfile`: rename a file in the pane's folder (same fence as `touch`).
pub fn script_rename(pane: usize, old: &str, new: &str) {
    let (Some(from), Some(to)) = (script_path(pane, old), script_path(pane, new)) else { return };
    let res = std::fs::rename(&from, &to);
    eprintln!("[auto] mvfile {} -> {}: {res:?}", from.display(), to.display());
}

/// For the scripted driver's `touch` and `rmfile`: create (or update the time of) an empty file
/// in the pane's folder, or delete one. Under TWINFM_SOAK=1 the path must lie in the soak fence.
pub fn script_file(pane: usize, name: &str, remove: bool) {
    let Some(path) = script_path(pane, name) else { return };
    let res = if remove {
        std::fs::remove_file(&path)
    } else {
        std::fs::OpenOptions::new().create(true).append(true).open(&path).and_then(|f| f.set_modified(std::time::SystemTime::now()))
    };
    eprintln!("[auto] {} {}: {res:?}", if remove { "rmfile" } else { "touch" }, path.display());
}

fn main() -> Result<(), slint::PlatformError> {
    let app = App::new()?;
    // TWINFM_SCHEME=light|dark forces the colour scheme (P10-M1: a scripted screenshot of the scheme
    // this machine is not in); unset, the system's app mode decides as always. The enum lives where the
    // generated bindings take it from (the builtin ColorScheme is not re-exported at slint::).
    match std::env::var("TWINFM_SCHEME").as_deref() {
        Ok("light") => app.global::<Palette>().set_color_scheme(slint::private_unstable_api::re_exports::ColorScheme::Light),
        Ok("dark") => app.global::<Palette>().set_color_scheme(slint::private_unstable_api::re_exports::ColorScheme::Dark),
        _ => {}
    }
    let icons = Rc::new(RefCell::new(IconTable::default()));
    let (tx, rx) = unbounded();
    let (icon_tx, icon_requests) = unbounded();
    let (icon_replies, icon_rx) = unbounded();
    let icon_gens = Arc::new([AtomicU64::new(0), AtomicU64::new(0)]);
    let icon_stats = Arc::new(icons::Stats::default());
    let icon_views = Arc::new(icons::Views::default());
    icons::spawn(icon_requests, icon_replies, icon_gens.clone(), icon_views.clone(), icon_stats.clone(), || {
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
    }, |p| {
        let _ = slint::invoke_from_event_loop(move || job_progress(p));
    });
    let verbs = verbs::Worker::spawn(|r| {
        let _ = slint::invoke_from_event_loop(move || match r {
            verbs::Reply::Done(d) => verb_done(d),
            verbs::Reply::Answer(a) => winmenu_answer(a),
            verbs::Reply::Folder { field, path, error } => browse_done(&field, path, error),
        });
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
    // The log (P11-M7), as early as the settings allow, so the start's own lines are in it.
    if config.log {
        if let Some(dir) = config_path.as_deref().and_then(Path::parent) {
            log_start(dir);
        }
    }
    eprintln!("[mem] process heap: {}", perf::process_heap());
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
        Some(a) => (vec![settings::SavedTab::path(a)], 0),
        None => settings::start_tabs(&config.left_tabs, config.left_active, &config.start_left),
    };
    let right_start = if config.start_right.is_empty() { left_paths[left_active].path.clone() } else { config.start_right.clone() };
    let (right_paths, right_active) = if config.split {
        settings::start_tabs(&config.right_tabs, config.right_active, &right_start)
    } else {
        (vec![settings::SavedTab::path("")], 0)
    };
    let mut next_tab = 0;
    // A tab saved with its view (P6-M3) gets its sort, hidden files, cursor and scroll back; a bare
    // path takes the defaults new tabs have. A saved Type sort with the Type column off sorts by name.
    let mut restore = |paths: &[settings::SavedTab], active: usize, hidden: bool, protected: bool| {
        let mut tabs: Vec<Tab> = paths
            .iter()
            .enumerate()
            .map(|(i, p)| {
                next_tab += 1;
                let mut t = Tab { path: PathBuf::from(&p.path), stale: i != active, ..blank_tab(next_tab, &icons) };
                // A sizes view walks again when it is shown (P13-M3): `refresh` starts its walk.
                if p.sizes {
                    t.sizeview = Some(SizeTab::default());
                    t.stale = true;
                }
                let sort = settings::sort_from_name(p.sort.as_deref().unwrap_or(&config.sort_column));
                t.model.sort_col.set(if sort == SortCol::Type && !config.show_type { SortCol::Name } else { sort });
                t.model.sort_asc.set(p.ascending.unwrap_or(config.sort_ascending));
                t.model.show_hidden.set(p.hidden.unwrap_or(hidden));
                t.model.show_protected.set(protected);
                t.restore_cursor = p.cursor.as_deref().map(SharedString::from);
                t.restore_scroll = p.scroll.filter(|y| y.is_finite() && *y < 0.0);
                // Its Back and Forward places (P11-M3).
                t.history = tabs::History::restored(p.back.iter().map(place_of).collect(), p.forward.iter().map(place_of).collect(), &t.place());
                t
            })
            .collect();
        let first = tabs.remove(0);
        Tabs::restored(first, tabs, active)
    };
    let panes = [
        restore(&left_paths, left_active, config.show_hidden_left, config.show_protected_left),
        restore(&right_paths, right_active, config.show_hidden_right, config.show_protected_right),
    ];
    for (pane, tabs) in panes.iter().enumerate() {
        if tabs.len() > 1 {
            eprintln!("[tab] pane={pane} restored {} tabs, tab {} active", tabs.len(), tabs.active_index() + 1);
        }
    }
    app.set_left_rows(ModelRc::from(panes[0].active().model.clone()));
    app.set_right_rows(ModelRc::from(panes[1].active().model.clone()));
    app.set_left_grid_rows(ModelRc::new(GridModel::new(panes[0].active().model.clone())));
    app.set_right_grid_rows(ModelRc::new(GridModel::new(panes[1].active().model.clone())));
    let strip_rows: [Rc<VecModel<StripRow>>; 2] = Default::default();
    app.set_left_strip_rows(ModelRc::from(strip_rows[0].clone()));
    app.set_right_strip_rows(ModelRc::from(strip_rows[1].clone()));
    let views = [config.view(0), config.view(1)];
    let grid = views.map(settings::View::is_grid);
    // A pane on details keeps its saved tile size for its next Ctrl+Shift+2 or 3.
    let tile = [icons::TileGeom::of(config.tile_left == "medium"), icons::TileGeom::of(config.tile_right == "medium")];
    app.set_left_grid(grid[0]);
    app.set_right_grid(grid[1]);
    app.set_left_medium(tile[0] == icons::TileGeom::MEDIUM);
    app.set_right_medium(tile[1] == icons::TileGeom::MEDIUM);
    for (pane, v) in views.iter().enumerate() {
        if v.is_grid() {
            eprintln!("[view] pane={pane} starts as {}", v.label());
        }
    }
    app.set_sidebar_visible(config.sidebar_visible);
    app.set_sidebar_width(config.sidebar_width);
    app.set_split_ratio(config.split_ratio);
    app.set_path_entry_sticky(config.path_bar == "entry");
    app.set_toolbar(settings::Toolbar::from_name(&config.toolbar).index());
    // A system setting sets nothing at start (the style follows the OS live); the title bar is
    // set when the handle is first fetched.
    if settings::ThemeMode::from_name(&config.theme) != settings::ThemeMode::System {
        apply_theme(&app, settings::ThemeMode::from_name(&config.theme));
    }
    app.set_preview_width(config.preview_width);
    app.set_preview_zoom(config.preview_zoom as i32);
    app.set_syntax_colours(config.syntax_colours);
    app.set_preview(config.preview);
    preview::init(&app);
    app.set_show_type(config.show_type);
    app.set_thumbnails(config.thumbnails);
    app.set_comfortable(config.comfortable());
    if config.thumbnails || config.comfortable() {
        log_list_geom(icons::ListGeom::new(config.thumbnails, config.comfortable()));
    }
    for pane in 0..2 {
        set_cols(&app, pane, config.cols(pane));
        log_col_changes(pane, cols::DEFAULT, config.cols(pane), "config");
    }
    app.set_version(env!("CARGO_PKG_VERSION").into());
    if let Some(g) = config.window {
        app.window().set_size(slint::LogicalSize::new(g.width, g.height));
        if settings::on_screen(g.x, g.y, g.width as i32, &settings::monitors()) {
            app.window().set_position(slint::PhysicalPosition::new(g.x, g.y));
        } else {
            eprintln!("[settings] saved window position ({}, {}) is off screen; left to Windows", g.x, g.y);
        }
    }
    // Once the window has its size (the name cell's width reads the layout).
    let weak = app.as_weak();
    slint::Timer::single_shot(Duration::from_millis(400), move || {
        if let Some(app) = weak.upgrade() {
            log_widths(&app, 0);
            log_widths(&app, 1);
        }
    });
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
            icon_views,
            resolve_seq: [0; 2],
            icon_stats,
            cloud: cloud::Locations::load(),
            icon_counts: IconCounts::default(),
            view: [(0.0, 0.0); 2],
            ops,
            verbs,
            next_verb: 0,
            winmenu: None,
            winmenu_seq: 0,
            hwnd: 0,
            next_job: 0,
            jobs: HashMap::new(),
            cut_source: None,
            clip_seq: 0,
            clip_files: toolbar::clipboard_has_files(),
            undo: Vec::new(),
            redo: Vec::new(),
            progress: None,
            paste_move: None,
            soak,
            strip: [None; 2],
            strip_dirty: [false; 2],
            grid,
            grid_n: [1; 2],
            tile,
            grid_slots: Default::default(),
            strip_rows,
            gesture: Default::default(),
            drag_icon: None,
            band: None,
            band_at: (0.0, 0.0),
            band_timer: slint::Timer::default(),
            band_logged: None,
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
            no_watch_logged: [None; 2],
            watch_edits: 0,
            stat_tx: edit_worker(),
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
    // A long run's log is cut past 20 MB (P11-M7): its size looked at once a minute.
    let log_timer = slint::Timer::default();
    log_timer.start(slint::TimerMode::Repeated, Duration::from_secs(60), || {
        if logfile::cut_if_over(logfile::CUT_BYTES, &log_first_line()) {
            eprintln!("[log] cut at {} MB: a fresh {}", logfile::CUT_BYTES >> 20, logfile::NAME);
        }
    });
    // A search tab's rows on screen follow changes (P11-M2).
    let restat_timer = slint::Timer::default();
    restat_timer.start(slint::TimerMode::Repeated, RESTAT_EVERY, || {
        with_state(|st| {
            if let Some(app) = st.app.upgrade() {
                restat_results(st, &app, 0);
                restat_results(st, &app, 1);
            }
        })
    });
    app.on_viewport_changed(|pane, y, h| on_viewport(pane as usize, y, h));
    app.on_grid_cols(|pane, n| on_grid_cols(pane as usize, n.max(1) as usize));
    app.on_grid_cursor(|pane, row| on_grid_cursor(pane as usize, row));
    app.on_set_view(|pane, view| set_view(pane as usize, settings::View::from_index(view)));

    app.on_navigate(|pane, path| navigate(pane as usize, PathBuf::from(path.as_str())));
    app.on_open_row(|pane, row| open_row(pane as usize, row));
    app.on_row_gesture(|pane, row, what, ctrl, shift| row_gesture(pane as usize, row, what, ctrl, shift));
    app.on_band_pointer(|pane, kind, x, y, ctrl, shift| band_pointer(pane as usize, kind, (x, y), ctrl, shift));
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
            if op == SelOp::Context {
                // A right press on a row, which may become a right-button drag.
                prefetch_drag_icon(st, pane as usize, row as usize);
            }
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
    app.on_side_step(|current, key| side_step(current, key.as_str()));
    app.on_side_row_top(|i| {
        with_state(|st| {
            let app = st.app.upgrade()?;
            Some(sidebar::row_top(&side_rows(st, &app).iter().map(|r| r.0).collect::<Vec<_>>(), usize::try_from(i).ok()?))
        })
        .unwrap_or(0.0)
    });
    app.on_bookmark_add(bookmark_add);
    app.on_recent_clear(recent_clear);
    app.on_bookmark_drag(|from, gap| {
        if let (Ok(from), Ok(gap)) = (usize::try_from(from), usize::try_from(gap)) {
            bookmark_drag(from, gap);
        }
    });
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
    app.on_toggle_preview(toggle_preview);
    app.on_preview_cursor(preview::kick);
    app.on_preview_scrolled(preview::scrolled);
    app.on_preview_resized(preview::resized);
    app.on_preview_link(preview::link);
    app.on_preview_zoom_step(preview::zoom);
    app.on_preview_zoom_key(preview::zoom_key);
    app.on_preview_menu(preview::menu);
    app.on_preview_recolour(preview::recolour);
    app.on_preview_password(preview::password);
    app.on_preview_copy(|k| preview::copy(usize::try_from(k).ok()));
    // Closing the window writes the settings at once (the window's placement included).
    app.window().on_close_requested(|| {
        save_now();
        slint::CloseRequestResponse::HideWindow
    });
    app.on_context_menu(|pane, row, x, y, extended| context_menu(pane as usize, row, x, y, extended));
    app.on_context_key(|pane, row, lx, ly, cy, vh| context_key(pane as usize, row, lx, ly, cy, vh));
    app.on_wm_picked(winmenu_picked);
    app.on_wm_hover(winmenu_hover);
    app.on_wm_key(|k| winmenu_key(k.as_str()));
    app.on_wm_dismissed(|| {
        winmenu_close("a press outside it");
    });
    app.on_command(|pane, cmd| command(pane as usize, cmd.as_str()));
    app.on_strip_resized(|pane| strip_resized(pane as usize));
    app.on_toolbar_clicked(|pane, button| toolbar_clicked(pane as usize, button.as_str()));
    app.on_col_drag(|pane, col, dx, phase| col_drag(pane as usize, col, dx, phase));
    app.on_col_fit(|pane, col| col_fit(pane as usize, col));
    app.on_nav_button(|pane, name| nav_button(pane as usize, name.as_str()));
    app.on_toolbar_menu(|pane, name, x, y| open_tool_menu(pane as usize, name.as_str(), x, y));
    app.on_toolbar_width({
        let w = app.as_weak();
        move |which, px| {
            if let Some(app) = w.upgrade() {
                toolbar_width(&app, which, px);
            }
        }
    });
    app.on_list_width({
        let w = app.as_weak();
        move |pane, px| {
            if let Some(app) = w.upgrade() {
                list_width(&app, pane as usize, px);
            }
        }
    });
    app.global::<Paging>().on_target(|cursor, content_y, visible_h, row_h, len, down| {
        model::page_target(cursor, content_y, visible_h, row_h, len, down)
    });
    app.global::<Paging>().on_grid_target(|cursor, cols, content_y, visible_h, tile_h, len, down| {
        model::grid_page_target(cursor, cols, content_y, visible_h, tile_h, len, down)
    });
    app.on_toolbar_tip(|pane, text| {
        let place = if pane < 0 { "top".to_string() } else { format!("pane={pane}") };
        eprintln!("[toolbar] {place} tooltip \"{text}\"");
    });
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
    // Tab and Shift+Tab between the lists (Jeremy, 2026-10-26: "tab switches panes").
    app.on_keys_by_tab(|pane, row| eprintln!("[focus] pane={pane} has the keys by Tab, cursor {row}"));
    app.on_crumb_clicked(|pane, i| {
        if let Ok(i) = usize::try_from(i) {
            crumb_clicked(pane as usize, i);
        }
    });
    app.on_history_menu(|pane, x, y| open_history_menu(pane as usize, x, y));
    app.on_browse_start(|field| browse_start(field.as_str()));
    app.on_open_log_folder(open_log_folder);
    app.on_start_folder_check(|field, text| start_folder_check(field.as_str(), text.as_str()));
    app.on_crumb_menu(|pane, i, x, y| {
        if let Ok(i) = usize::try_from(i) {
            crumb_menu(pane as usize, i, x, y);
        }
    });
    app.on_crumb_geometry(|pane, i, x, w| crumb_geometry(pane as usize, i, x, w));
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
    app.on_filter_return(|pane, text| filter_return(pane as usize, text));
    app.on_search_pressed(|pane| search_button(pane as usize));
    app.on_content_toggled(|pane, on| content_toggled(pane as usize, on));
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
                with_state(|st| {
                    st.watch.unwatch_pane(1);
                    st.watch_tabs(&app)
                });
                app.invoke_focus_pane(0);
            }
        }
    });

    // Each pane's active tab lists its folder (a sizes view walks it, P13-M3); the restored
    // background tabs wait until shown.
    let first = |pane: usize, p: &settings::SavedTab| if p.sizes { refresh(pane) } else { navigate(pane, PathBuf::from(&p.path)) };
    first(0, &left_paths[left_active]);
    if config.split {
        app.set_split(true);
        first(1, &right_paths[right_active]);
    }
    // Start with keyboard focus in the list, so arrows, Space and Ctrl+A work before a click.
    app.invoke_focus_pane(0);
    if config.preview {
        preview::set_visible(true);
    }
    ask::init(&app);
    auto::start(&app);
    // The drop target goes on the window once it exists (winit creates it lazily, in `run`).
    with_state(|st| st.dnd_timer.start(slint::TimerMode::Repeated, Duration::from_millis(50), register_dnd));

    let res = app.run();
    // A worker still waiting on a question gets Cancel, so it never waits for ever (P12-M3).
    ask::quit();
    // The copies of archives inside archives (P8-L3).
    fs::archive::clear_nested();
    // A change still waiting for its debounce (a scripted quit, say) is written now.
    if with_state(|st| st.save_timer.running()) {
        save_now();
    }
    // Stop the watcher thread now: at process exit Windows terminates it before the state's
    // thread-local destructor runs.
    with_state(|st| st.watch.stop());
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs::watch::Change;
    use model::Edit;

    #[test]
    fn a_renamed_search_result_keeps_its_folder() {
        let root = Path::new(r"C:\t\root");
        assert_eq!(search_row_name(root, Path::new(r"C:\t\root\s2\deep\hit-b.txt"), "hit-y.txt"), r"s2\deep\hit-y.txt");
        assert_eq!(search_row_name(root, Path::new(r"C:\t\root\hit-b.txt"), "hit-y.txt"), "hit-y.txt", "a result in the root");
        assert_eq!(search_row_name(root, Path::new(r"D:\elsewhere\b.txt"), "y.txt"), "y.txt", "not under the root: the name");
    }

    fn s(n: &str) -> String {
        n.to_string()
    }

    fn entry(n: &str) -> model::Entry {
        model::Entry::new(n.into(), 1, 0, 0x20, "Text Document".into())
    }

    #[test]
    fn watch_set_takes_active_tabs_then_the_most_recently_shown_up_to_the_cap() {
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let c = |id, active, eligible, ms| WatchCand { id, active, eligible, shown_at: at(ms) };
        // An active tab shown long ago still comes first; ineligible tabs never count.
        let cands = [c(1, false, true, 30), c(2, true, true, 0), c(3, false, true, 50), c(4, false, false, 90), c(5, true, false, 80), c(6, false, true, 10)];
        assert_eq!(watch_set(&cands, 8), vec![2, 3, 1, 6]);
        assert_eq!(watch_set(&cands, 3), vec![2, 3, 1], "the least recently shown is past the cap");
        assert_eq!(watch_set(&cands, 1), vec![2]);
        // Two active tabs (the split) both come before every background one.
        let cands = [c(1, false, true, 90), c(2, true, true, 0), c(3, true, true, 5)];
        assert_eq!(watch_set(&cands, 2), vec![3, 2]);
        assert!(watch_set(&[], 8).is_empty());
    }

    #[test]
    fn explained_by_needs_every_name_of_the_window() {
        let set: HashSet<String> = ["a.txt", "b - copy.txt"].into_iter().map(s).collect();
        assert!(explained_by(&[Change::Added(s("A.TXT")), Change::Modified(s("b - Copy.txt"))], &set));
        assert!(!explained_by(&[Change::Added(s("a.txt")), Change::Removed(s("c.txt"))], &set), "one name not the job's");
        assert!(explained_by(&[Change::Renamed { from: s("a.txt"), to: s("b - copy.txt") }], &set));
        assert!(!explained_by(&[Change::Renamed { from: s("a.txt"), to: s("z.txt") }], &set), "both names of a rename");
        assert!(!explained_by(&[], &set), "an empty window explains nothing");
    }

    #[test]
    fn explained_names_are_the_jobs_names_in_that_folder() {
        let dir = PathBuf::from(r"C:\x\here");
        let item = |src: &str, dest: Option<&str>| fileops::ItemResult { source: src.into(), dest: dest.map(PathBuf::from), hr: 0 };
        let job = Job::Copy { sources: vec![r"C:\x\other\A.txt".into()], dest_dir: dir.clone() };
        let names = explained_names(&dir, &job, &[item(r"C:\x\other\A.txt", Some(r"C:\X\Here\A.txt"))]);
        assert_eq!(names, ["a.txt"].into_iter().map(s).collect(), "the copy, not its source in another folder");
        let job = Job::Rename { source: r"C:\x\here\old.txt".into(), new_name: s("New.txt") };
        let names = explained_names(&dir, &job, &[]);
        assert_eq!(names, ["new.txt"].into_iter().map(s).collect(), "the name asked for even with no results");
        let job = Job::NewFolder { parent: dir.clone(), name: s("New folder") };
        let names = explained_names(&dir, &job, &[item(r"C:\x\here", Some(r"C:\x\here\New folder (2)"))]);
        assert_eq!(names, ["new folder", "new folder (2)"].into_iter().map(s).collect(), "not the folder itself");
        // A move-as back from another folder: the item's name here, from its result.
        let job = Job::MoveAs { items: vec![(r"C:\x\other\a (2).txt".into(), s("a.txt"))], dest_dir: dir.clone() };
        let names = explained_names(&dir, &job, &[item(r"C:\x\other\a (2).txt", Some(r"C:\x\here\a.txt"))]);
        assert_eq!(names, ["a.txt"].into_iter().map(s).collect(), "the old name it went back under");
        // A move-as out of this folder: the name that left.
        let job = Job::MoveAs { items: vec![(r"C:\x\here\b (2).txt".into(), s("b.txt"))], dest_dir: r"C:\x\other".into() };
        let names = explained_names(&dir, &job, &[item(r"C:\x\here\b (2).txt", Some(r"C:\x\other\b.txt"))]);
        assert_eq!(names, ["b (2).txt"].into_iter().map(s).collect());
        // A restore: the originals, never the bin's `$R` names.
        let bin = r"C:\$Recycle.Bin\S-1-5-21\$RAB12CD.txt";
        let job = Job::Restore { items: vec![(bin.into(), r"C:\x\here\c.txt".into())] };
        let names = explained_names(&dir, &job, &[item(bin, Some(r"C:\x\here\c.txt"))]);
        assert_eq!(names, ["c.txt"].into_iter().map(s).collect());
    }

    #[test]
    fn removed_names_are_the_sources_in_that_folder() {
        let dir = PathBuf::from(r"C:\x\here");
        let names = |job: &Job| removed_names(job, &dir).iter().map(|n| n.to_string()).collect::<Vec<_>>();
        let delete = Job::Delete { sources: vec![r"C:\x\here\a.txt".into(), r"C:\x\other\b.txt".into()], permanent: false };
        assert_eq!(names(&delete), ["a.txt"], "the source in another folder is not a row here");
        let mv = Job::Move { sources: vec![r"C:\X\Here\c.txt".into()], dest_dir: r"C:\x\other".into() };
        assert_eq!(names(&mv), ["c.txt"], "folders compare as the file system does");
        let move_as = Job::MoveAs { items: vec![(r"C:\x\here\d (2).txt".into(), s("d.txt"))], dest_dir: r"C:\x\other".into() };
        assert_eq!(names(&move_as), ["d (2).txt"], "the name it has here, not the one it goes back under");
        let restore = Job::Restore { items: vec![(r"C:\$Recycle.Bin\S-1\$R1.txt".into(), r"C:\x\here\e.txt".into())] };
        assert!(names(&restore).is_empty(), "a restore adds rows");
        let rename = Job::Rename { source: r"C:\x\here\f.txt".into(), new_name: s("g.txt") };
        assert!(names(&rename).is_empty());
    }

    #[test]
    fn after_job_feeds_the_two_stacks() {
        use fileops::undo::Record;
        let rec = |t: &str| Some(Record { undo: None, text: t.into(), cannot: "" });
        let texts = |v: &[Record]| v.iter().map(|r| r.text.clone()).collect::<Vec<_>>();
        let (mut undo, mut redo) = (Vec::new(), Vec::new());
        assert_eq!(after_job(None, true, rec("a"), &mut undo, &mut redo), 0);
        assert_eq!(after_job(None, true, rec("b"), &mut undo, &mut redo), 0);
        // Ctrl+Z took "b" off the undo stack; its undo job's record goes onto the redo stack.
        undo.pop();
        assert_eq!(after_job(Some(Replay::Undo), true, rec("b"), &mut undo, &mut redo), 0);
        assert_eq!((texts(&undo), texts(&redo)), (vec![s("a")], vec![s("b")]));
        // Ctrl+Y took it back; the redo job's record goes onto the undo stack.
        redo.pop();
        assert_eq!(after_job(Some(Replay::Redo), true, rec("b"), &mut undo, &mut redo), 0);
        assert_eq!((texts(&undo), texts(&redo)), (vec![s("a"), s("b")], vec![]));
        // An undo that failed leaves no record; a new job that changed something clears the redo
        // stack, record or not (a failed one with some items done, a permanent delete).
        undo.pop();
        assert_eq!(after_job(Some(Replay::Undo), false, None, &mut undo, &mut redo), 0);
        redo.push(Record { undo: None, text: s("x"), cannot: "" });
        redo.push(Record { undo: None, text: s("y"), cannot: "" });
        // A job cancelled while queued, or one whose every item failed, leaves redo alone.
        assert_eq!(after_job(None, false, None, &mut undo, &mut redo), 0);
        assert_eq!(texts(&redo), [s("x"), s("y")]);
        assert_eq!(after_job(None, true, None, &mut undo, &mut redo), 2);
        assert!(redo.is_empty());
        assert_eq!(after_job(None, true, rec("c"), &mut undo, &mut redo), 0);
        assert_eq!(texts(&undo), [s("a"), s("c")]);
    }

    #[test]
    fn progress_shows_beside_the_running_jobs_own_note() {
        let job = |pane, tab, running: &str| JobInfo { pane, tab, fallback: None, replay: None, running: s(running) };
        let jobs: HashMap<u64, JobInfo> = [(7, job(0, 3, "copying 3 items...")), (8, job(1, 4, "moving 1 item..."))].into_iter().collect();
        let p = Some((7, 42));
        assert_eq!(progress_for(p, &jobs, 0, 3, "copying 3 items..."), Some(42), "its pane, its tab, its note");
        assert_eq!(progress_for(p, &jobs, 0, 5, "copying 3 items..."), None, "another tab");
        assert_eq!(progress_for(p, &jobs, 1, 3, "copying 3 items..."), None, "the other pane");
        assert_eq!(progress_for(p, &jobs, 0, 3, "moving 1 item..."), None, "a later job's note replaced it");
        assert_eq!(progress_for(p, &jobs, 0, 3, "Nothing to undo"), None, "a note replaced it");
        assert_eq!(progress_for(Some((9, 42)), &jobs, 0, 3, "copying 3 items..."), None, "no such job (done)");
        assert_eq!(progress_for(None, &jobs, 0, 3, "copying 3 items..."), None, "no progress yet");
    }

    #[test]
    fn replay_text_names_the_record_when_done() {
        let orig = "Deleted 1 item to the Recycle Bin";
        assert_eq!(replay_text(Replay::Undo, true, orig, "Restored 1 item"), format!("Undid: {orig}"));
        assert_eq!(replay_text(Replay::Undo, false, orig, "Cancelled"), "Undo: Cancelled");
        assert_eq!(replay_text(Replay::Redo, true, orig, "x"), format!("Redid: {orig}"));
        assert_eq!(replay_text(Replay::Redo, false, orig, "Failed: 0x80004005"), "Redo: Failed: 0x80004005");
    }

    #[test]
    fn edits_from_folds_failed_stats() {
        let changes = [
            Change::Added(s("new.txt")),
            Change::Added(s("gone-again.txt")),
            Change::Removed(s("old.txt")),
            Change::Modified(s("kept.txt")),
            Change::Modified(s("vanished.txt")),
            Change::Renamed { from: s("a.txt"), to: s("b.txt") },
            Change::Renamed { from: s("c.txt"), to: s("d.txt") },
        ];
        let on_disk = ["new.txt", "kept.txt", "b.txt"];
        let mut asked = Vec::new();
        let (edits, vanished) = edits_from(&changes, |n| {
            asked.push(n.to_string());
            on_disk.contains(&n).then(|| entry(n))
        });
        assert_eq!(vanished, ["gone-again.txt", "d.txt"], "added or renamed to, gone when statted");
        assert_eq!(asked, ["new.txt", "gone-again.txt", "kept.txt", "vanished.txt", "b.txt", "d.txt"], "a removal is not statted");
        let shape: Vec<String> = edits
            .iter()
            .map(|e| match e {
                Edit::Upsert(e) => format!("upsert {}", e.name),
                Edit::Remove(n) => format!("remove {n}"),
                Edit::Rename { from, entry } => format!("rename {from} -> {}", entry.name),
            })
            .collect();
        assert_eq!(shape, ["upsert new.txt", "remove old.txt", "upsert kept.txt", "remove vanished.txt", "rename a.txt -> b.txt", "remove c.txt"]);
    }

    #[test]
    fn unknown_name_reads_apply_edits_errors() {
        assert_eq!(unknown_name(r#"remove of "a b.txt": not in the listing"#), Some("a b.txt"));
        assert_eq!(unknown_name(r#"rename of "x" to "y": not in the listing"#), Some("x"));
        assert_eq!(unknown_name("a refresh is in flight"), None);
    }

    #[test]
    fn stat_dir_normalises_a_typed_path() {
        // As strings: Path equality compares components, which hides the slashes and the `.`.
        assert_eq!(stat_dir(Path::new("C:/x/./y")).to_str(), Some(r"C:\x\y"));
        assert_eq!(stat_dir(Path::new(r"C:\a\..\b")).to_str(), Some(r"C:\b"));
        assert_eq!(stat_dir(Path::new(r"C:\x\y")).to_str(), Some(r"C:\x\y"));
    }
}
