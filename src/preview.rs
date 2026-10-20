//! The preview pane (P8-M1): the active pane's cursor file in a column at the window's right edge.
//! The UI thread only notices that the cursor moved (one timer restart, `kick`); once it has
//! rested `SETTLE_MS` the file is handed to one preview worker thread, which classifies it, opens a
//! PDF with hayro (pure Rust), answers its page sizes and renders the pages the UI asks for, each to
//! RGBA at the column's width in physical pixels. The UI lays the pages out from their sizes before
//! any is drawn and asks for the pages in view plus one either side as the column scrolls, keeping
//! only those images. Every request carries a generation: a newer cursor supersedes the older file
//! on the worker (its queued pages are dropped before they are rendered) and on the UI thread (a
//! late answer is dropped on arrival). The worker owns at most one open document; the UI thread
//! never reads the file.

use crate::{App, PreviewPage};
use crossbeam_channel::{Receiver, Sender, unbounded};
use slint::{ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, Timer, TimerMode, VecModel};
use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, FILE_ATTRIBUTE_RECALL_ON_OPEN,
};

/// The cursor rests this long before its file is previewed, so holding Down renders nothing.
pub const SETTLE_MS: u64 = 150;
/// The column's width rests this long before the visible pages are rendered again at it.
pub const WIDTH_SETTLE_MS: u64 = 200;
/// No page image is wider or taller than this (a tall page is scaled down to it).
pub const MAX_SIDE: u32 = 8192;
/// A page that took longer than this to render shows a note in its place instead.
pub const TOO_LONG_MS: u128 = 5000;
/// The column's share of the space right of the sidebar (settings `preview_width`).
pub const RATIO_MIN: f32 = 0.2;
pub const RATIO_MAX: f32 = 0.6;
pub const RATIO_DEFAULT: f32 = 0.35;
/// The space above the first page, between pages and below the last (preview.slint draws each page
/// 8 px in from the column's sides, at the width it reports through `resized`).
pub const PAD: f32 = 8.0;

pub const NO_PREVIEW: &str = "No preview";
pub const PASSWORD: &str = "Password-protected PDF: not previewed";
pub const DAMAGED: &str = "This PDF could not be read";
pub const CLOUD: &str = "Not previewed: the file is in the cloud and not downloaded";
pub const ARCHIVE: &str = "Not previewed inside an archive";

/// The column's share, pulled into its bounds (a hand-edited file, a drag past the end).
pub fn clamp_ratio(r: f32) -> f32 {
    if r.is_finite() { r.clamp(RATIO_MIN, RATIO_MAX) } else { RATIO_DEFAULT }
}

/// The settle: when the cursor last moved (ms on any clock), checked when the timer fires.
#[derive(Debug, Default)]
pub struct Settle {
    last: Option<u64>,
}

/// What a fired settle timer finds.
#[derive(Debug, PartialEq, Eq)]
pub enum Due {
    /// The cursor has rested long enough: preview its file (the move is spent).
    Now,
    /// It moved again since: wait this many ms more.
    Wait(u64),
    /// No move pending.
    Idle,
}

impl Settle {
    pub fn moved(&mut self, now: u64) {
        self.last = Some(now);
    }

    pub fn check(&mut self, now: u64) -> Due {
        match self.last {
            None => Due::Idle,
            Some(t) if now >= t + SETTLE_MS => {
                self.last = None;
                Due::Now
            }
            Some(t) => Due::Wait(t + SETTLE_MS - now),
        }
    }

    pub fn pending(&self) -> bool {
        self.last.is_some()
    }
}

/// What the cursor file is, as far as the preview goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Folder,
    /// Not a PDF: nothing is previewed in P8-M1 but PDFs.
    Other,
    /// A PDF stored inside an archive.
    Archive,
    /// A PDF whose data is in the cloud: reading it would start a download.
    CloudOnly,
    Pdf,
}

impl Class {
    /// The message the column shows instead of pages (None for a PDF).
    pub fn message(self) -> Option<&'static str> {
        match self {
            Class::Folder | Class::Other => Some(NO_PREVIEW),
            Class::Archive => Some(ARCHIVE),
            Class::CloudOnly => Some(CLOUD),
            Class::Pdf => None,
        }
    }

    /// For the `[preview]` lines.
    pub fn label(self) -> &'static str {
        match self {
            Class::Folder => "folder",
            Class::Other => "not a pdf",
            Class::Archive => "inside an archive",
            Class::CloudOnly => "cloud-only",
            Class::Pdf => "pdf",
        }
    }
}

/// Classify from the listing's facts: a folder, then the type (only a `.pdf`, any case, is read),
/// then where it lies. `in_archive` is asked only for a PDF (it may cost a file system query on an
/// archive-named component); the cloud test is the listing's attributes, so nothing is read.
pub fn classify(name: &str, attrs: u32, is_dir: bool, in_archive: impl FnOnce() -> bool) -> Class {
    let pdf = Path::new(name).extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
    if is_dir {
        Class::Folder
    } else if !pdf {
        Class::Other
    } else if in_archive() {
        Class::Archive
    } else if attrs & (FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_RECALL_ON_OPEN | FILE_ATTRIBUTE_OFFLINE) != 0 {
        Class::CloudOnly
    } else {
        Class::Pdf
    }
}

/// One page's place in the column's scroll: its top and height (logical px).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    pub y: f32,
    pub h: f32,
}

/// Lay the pages out at `width` (logical px) from their sizes in points: each as tall as its
/// aspect makes it, `PAD` above the first, between pages and below the last. Returns the slots and
/// the content's height.
pub fn layout(sizes: &[(f32, f32)], width: f32) -> (Vec<Slot>, f32) {
    let mut y = PAD;
    let slots = sizes
        .iter()
        .map(|&(w, h)| {
            let s = Slot { y, h: if w > 0.0 && h > 0.0 { (width * h / w).max(1.0) } else { width } };
            y += s.h + PAD;
            s
        })
        .collect();
    (slots, y)
}

/// The pages a scroll at `top` with `height` on screen shows (first, last), or None for no pages.
pub fn in_view(slots: &[Slot], top: f32, height: f32) -> Option<(usize, usize)> {
    if slots.is_empty() {
        return None;
    }
    let bottom = top + height.max(0.0);
    let first = slots.partition_point(|s| s.y + s.h <= top).min(slots.len() - 1);
    let last = slots.partition_point(|s| s.y < bottom).saturating_sub(1).max(first);
    Some((first, last))
}

/// The pages kept: those in view and one either side.
pub fn kept((first, last): (usize, usize), pages: usize) -> (usize, usize) {
    (first.saturating_sub(1), (last + 1).min(pages.saturating_sub(1)))
}

/// A page's image size at `width_px` (physical px) from its size in points, scaled down so neither
/// side passes `MAX_SIDE` (true when it was).
pub fn page_px((w_pt, h_pt): (f32, f32), width_px: u32) -> (u32, u32, bool) {
    let w = f64::from(width_px.max(1));
    let h = (w * f64::from(h_pt) / f64::from(w_pt.max(1e-3))).round().max(1.0);
    let big = w.max(h);
    if big > f64::from(MAX_SIDE) {
        let f = f64::from(MAX_SIDE) / big;
        ((w * f).floor().max(1.0) as u32, (h * f).floor().max(1.0) as u32, true)
    } else {
        (w as u32, h as u32, false)
    }
}

/// What the cursor points at, from the pane's listing (main.rs `preview_target`).
#[derive(Debug, Clone)]
pub struct Target {
    pub pane: usize,
    pub path: PathBuf,
    pub attrs: u32,
    pub is_dir: bool,
}

/// To the worker.
#[derive(Debug)]
enum Req {
    Open { seq: u64, pane: usize, path: PathBuf, attrs: u32, is_dir: bool },
    /// Render page `page` (0-based) at `width_px`.
    Page { seq: u64, page: usize, width_px: u32 },
    /// Close the document (the column was hidden, or the cursor is on no file).
    Close,
}

/// What the UI wants rendered now, read by the worker before each page: the generation shown,
/// the pages kept and the width. A page outside them is skipped, not rendered.
#[derive(Debug, Clone, Copy, Default)]
struct Keep {
    seq: u64,
    first: usize,
    last: usize,
    width_px: u32,
}

/// From the worker.
enum Reply {
    Message { seq: u64, text: &'static str },
    Pdf { seq: u64, sizes: Vec<(f32, f32)> },
    Page { seq: u64, page: usize, width_px: u32, buf: SharedPixelBuffer<Rgba8Pixel> },
    /// "Page N took too long", in the page's place.
    Note { seq: u64, page: usize, width_px: u32, text: String },
    Skipped { seq: u64, page: usize, width_px: u32 },
}

/// The worker's queue: requests in order, except that everything before the newest `Open` is
/// dropped unseen (counted, for the `superseded` line).
struct Inbox {
    rx: Receiver<Req>,
    queue: VecDeque<Req>,
    dropped: usize,
}

impl Inbox {
    fn next(&mut self) -> Option<Req> {
        if self.queue.is_empty() {
            self.queue.push_back(self.rx.recv().ok()?);
        }
        self.queue.extend(self.rx.try_iter());
        if let Some(i) = self.queue.iter().rposition(|r| matches!(r, Req::Open { .. })) {
            for r in self.queue.drain(..i) {
                self.dropped += usize::from(matches!(r, Req::Page { .. }));
            }
        }
        self.queue.pop_front()
    }
}

/// The panic's message, for the log.
fn panic_text(p: &(dyn std::any::Any + Send)) -> String {
    p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "?".into())
}

fn worker(rx: Receiver<Req>, keep: Arc<Mutex<Keep>>, reply: impl Fn(Reply)) {
    let mut inbox = Inbox { rx, queue: VecDeque::new(), dropped: 0 };
    let mut next = None;
    loop {
        let req = match next.take() {
            Some(r) => r,
            None => match inbox.next() {
                Some(r) => r,
                None => return,
            },
        };
        // A page or a close with no document open belongs to a generation that is gone.
        if let Req::Open { seq, pane, path, attrs, is_dir } = req {
            next = serve(&mut inbox, &keep, &reply, seq, pane, &path, attrs, is_dir);
        }
    }
}

/// Classify and open one file, answer, then render its pages until a newer request arrives (that
/// request is returned) or the document cannot go on.
#[allow(clippy::too_many_arguments)]
fn serve(inbox: &mut Inbox, keep: &Mutex<Keep>, reply: &impl Fn(Reply), seq: u64, pane: usize, path: &Path, attrs: u32, is_dir: bool) -> Option<Req> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let class = classify(&name, attrs, is_dir, || crate::fs::archive::within(path));
    if let Some(text) = class.message() {
        eprintln!("[preview] pane={pane} gen={seq} {}: {}", path.display(), class.label());
        reply(Reply::Message { seq, text });
        return None;
    }
    use hayro::hayro_syntax::{DecryptionError, LoadPdfError, Pdf};
    let t0 = Instant::now();
    // Err(None): password-protected (hayro opens no encrypted file that needs a password);
    // Err(Some(why)): anything else that stops it.
    let opened: Result<(Pdf, Vec<(f32, f32)>), Option<String>> = std::fs::read(path).map_err(|e| Some(e.to_string())).and_then(|data| {
        match std::panic::catch_unwind(|| {
            Pdf::new(data).map(|pdf| {
                let sizes: Vec<(f32, f32)> = pdf.pages().iter().map(|p| p.render_dimensions()).collect();
                (pdf, sizes)
            })
        }) {
            Ok(Ok((_, sizes))) if sizes.is_empty() => Err(Some("no pages".into())),
            Ok(Ok(o)) => Ok(o),
            Ok(Err(LoadPdfError::Decryption(DecryptionError::PasswordProtected))) => Err(None),
            Ok(Err(e)) => Err(Some(format!("{e:?}"))),
            Err(p) => {
                let msg = panic_text(&*p);
                eprintln!("[preview] worker panicked on {}: {msg}", path.display());
                Err(Some(format!("panicked: {msg}")))
            }
        }
    });
    let (pdf, sizes) = match opened {
        Ok(o) => o,
        Err(why) => {
            let what = why.as_ref().map_or_else(|| "password".to_string(), |w| format!("damaged: {w}"));
            eprintln!("[preview] pane={pane} gen={seq} {}: {what}", path.display());
            reply(Reply::Message { seq, text: if why.is_none() { PASSWORD } else { DAMAGED } });
            return None;
        }
    };
    eprintln!("[preview] pane={pane} gen={seq} {}: pdf, {} pages, opened in {:.0} ms", path.display(), sizes.len(), crate::perf::elapsed_ms(t0));
    reply(Reply::Pdf { seq, sizes: sizes.clone() });
    // The render cache (fonts, glyph outlines) lives as long as the document.
    let cache = hayro::RenderCache::new();
    let settings = hayro::hayro_interpret::InterpreterSettings::default();
    let mut dropped = 0;
    let next = loop {
        let Some(req) = inbox.next() else { break None };
        let (page, width_px) = match req {
            Req::Page { seq: g, page, width_px } if g == seq => (page, width_px),
            Req::Page { .. } => {
                dropped += 1;
                continue;
            }
            Req::Close => break None,
            open @ Req::Open { .. } => break Some(open),
        };
        let k = *keep.lock().unwrap_or_else(|e| e.into_inner());
        if k.seq != seq {
            dropped += 1;
            continue;
        }
        if page < k.first || page > k.last || width_px != k.width_px || page >= sizes.len() {
            reply(Reply::Skipped { seq, page, width_px });
            continue;
        }
        let (w, h, capped) = page_px(sizes[page], width_px);
        if capped {
            let full_h = f64::from(width_px) * f64::from(sizes[page].1) / f64::from(sizes[page].0.max(1e-3));
            eprintln!("[preview] gen={seq} page {} capped at {MAX_SIDE} px (was {width_px}x{full_h:.0})", page + 1);
        }
        let t = Instant::now();
        let rendered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let ps = hayro::PixmapSettings {
                x_scale: w as f32 / sizes[page].0.max(1e-3),
                y_scale: h as f32 / sizes[page].1.max(1e-3),
                bg_color: hayro::vello_cpu::color::palette::css::WHITE,
            };
            hayro::render(&pdf.pages()[page], &cache, &settings, &hayro::RenderSettings::default(), &ps)
        }));
        let ms = t.elapsed().as_millis();
        match rendered {
            Err(p) => {
                eprintln!("[preview] worker panicked on {}: {}", path.display(), panic_text(&*p));
                reply(Reply::Message { seq, text: DAMAGED });
                break None;
            }
            Ok(_) if ms > TOO_LONG_MS => {
                eprintln!("[preview] gen={seq} page {} took {ms} ms: too long", page + 1);
                reply(Reply::Note { seq, page, width_px, text: format!("Page {} took too long", page + 1) });
            }
            Ok(pixmap) => {
                let (pw, ph) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
                // Straight (not premultiplied) RGBA, as Slint's from_rgba8 takes it; on the white
                // background every pixel is opaque, so this is a plain copy.
                let data = pixmap.take_rgba8(hayro::vello_cpu::peniko::ImageAlphaType::Alpha);
                let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&data, pw, ph);
                eprintln!("[preview] gen={seq} page {} rendered {pw}x{ph} px in {ms} ms", page + 1);
                reply(Reply::Page { seq, page, width_px, buf });
            }
        }
    };
    dropped += std::mem::take(&mut inbox.dropped);
    if dropped > 0 {
        eprintln!("[preview] gen={seq} superseded: {dropped} queued page(s) dropped");
    }
    next
}

/// The UI thread's side.
struct Ui {
    app: slint::Weak<App>,
    tx: Sender<Req>,
    keep: Arc<Mutex<Keep>>,
    /// The column shows.
    visible: bool,
    /// The `shown at` line is still to be logged, by its timer.
    announce: bool,
    announce_timer: Timer,
    settle: Settle,
    settle_timer: Timer,
    width_timer: Timer,
    seq: u64,
    /// The file the current generation previews.
    shown: Option<PathBuf>,
    /// Waiting for the worker's answer to the open.
    opening: bool,
    /// The pages' sizes in points (empty: no PDF shown).
    sizes: Vec<(f32, f32)>,
    slots: Vec<Slot>,
    /// The width a page is drawn at (logical px, from the column) and rendered at (physical px).
    page_w: f32,
    width_px: u32,
    pages: Rc<VecModel<PreviewPage>>,
    /// Per page, the render width of the image (or note) it holds.
    held: Vec<Option<u32>>,
    /// Pages asked of the worker and not answered yet.
    asked: HashSet<usize>,
    /// The pages kept (in view and one either side).
    range: Option<(usize, usize)>,
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
    static START: Instant = Instant::now();
}

fn now_ms() -> u64 {
    START.with(|s| s.elapsed().as_millis() as u64)
}

/// A short borrow of the UI state; None before `init` or if it is busy (not expected: no borrow
/// is held across a call that could come back here).
fn with_ui<R>(f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    UI.with(|u| u.try_borrow_mut().ok()?.as_mut().map(f))
}

/// Start the worker and give the column its page model.
pub fn init(app: &App) {
    let (tx, rx) = unbounded::<Req>();
    let keep = Arc::new(Mutex::new(Keep::default()));
    let k2 = keep.clone();
    let spawned = std::thread::Builder::new().name("preview".into()).spawn(move || {
        worker(rx, k2, |r| {
            let _ = slint::invoke_from_event_loop(move || on_reply(r));
        })
    });
    if let Err(e) = spawned {
        eprintln!("[preview] worker not started: {e}");
    }
    let pages = Rc::new(VecModel::<PreviewPage>::default());
    app.set_preview_pages(ModelRc::from(pages.clone()));
    UI.with(|u| {
        *u.borrow_mut() = Some(Ui {
            app: app.as_weak(),
            tx,
            keep,
            visible: false,
            announce: false,
            announce_timer: Timer::default(),
            settle: Settle::default(),
            settle_timer: Timer::default(),
            width_timer: Timer::default(),
            seq: 0,
            shown: None,
            opening: false,
            sizes: Vec::new(),
            slots: Vec::new(),
            page_w: 0.0,
            width_px: 0,
            pages,
            held: Vec::new(),
            asked: HashSet::new(),
            range: None,
        })
    });
}

/// The column was shown or hidden (Alt+P, the toolbar, the dialog, the settings at start).
pub fn set_visible(on: bool) {
    if !on {
        eprintln!("[preview] hidden");
    }
    with_ui(|u| {
        u.visible = on;
        // "shown at" is logged when the column reports its width (`resized`), which it does as
        // it appears; at start the window has no size yet.
        u.announce = on;
        u.announce_timer.stop();
        u.settle = Settle::default();
        u.settle_timer.stop();
        u.width_timer.stop();
        u.show(None);
        if on {
            // On the next turn of the event loop, once the column has its width.
            u.settle_timer.start(TimerMode::SingleShot, Duration::from_millis(1), refresh);
        }
    });
}

/// The cursor moved or the active pane changed (app.slint's `preview-cursor`, `show_tab`): restart
/// the settle timer. Nothing else here, so a held Down key costs one timer restart per row.
pub fn kick() {
    with_ui(|u| {
        if u.visible {
            u.settle.moved(now_ms());
            u.settle_timer.start(TimerMode::SingleShot, Duration::from_millis(SETTLE_MS), settle_fired);
        }
    });
}

fn settle_fired() {
    match with_ui(|u| u.settle.check(now_ms())) {
        Some(Due::Now) => refresh(),
        Some(Due::Wait(ms)) => {
            with_ui(|u| u.settle_timer.start(TimerMode::SingleShot, Duration::from_millis(ms), settle_fired));
        }
        _ => {}
    }
}

/// Preview the cursor file now, unless it is the one shown.
fn refresh() {
    if with_ui(|u| u.visible) != Some(true) {
        return;
    }
    let target = crate::preview_target();
    with_ui(|u| {
        if target.as_ref().map(|t| &t.path) != u.shown.as_ref() {
            u.show(target);
        }
    });
}

/// The column's scroll moved (app.slint `preview-scrolled`): only the pages kept are compared, and
/// work follows only when they changed.
pub fn scrolled(top: f32, height: f32) {
    with_ui(|u| {
        if u.visible && !u.slots.is_empty() {
            u.ask(top, height);
        }
    });
}

/// The column's page width changed (logical px): lay out and render again once it has settled.
pub fn resized(page_w: f32) {
    with_ui(|u| {
        if (page_w - u.page_w).abs() < 0.5 {
            return;
        }
        u.page_w = page_w;
        if u.visible && u.announce && page_w > 0.0 {
            // Once the width has rested: at start the window takes its size in steps.
            u.announce_timer.start(TimerMode::SingleShot, Duration::from_millis(WIDTH_SETTLE_MS), || {
                with_ui(|u| {
                    u.announce = false;
                    let ratio = u.app.upgrade().map_or(0.0, |a| a.get_preview_width());
                    eprintln!("[preview] shown at {:.0} px (ratio {ratio:.2})", u.page_w + 2.0 * PAD);
                });
            });
        }
        if u.visible && !u.sizes.is_empty() {
            u.width_timer.start(TimerMode::SingleShot, Duration::from_millis(WIDTH_SETTLE_MS), width_settled);
        }
    });
}

fn width_settled() {
    with_ui(|u| {
        eprintln!("[preview] width {:.0} px: re-render after settle", u.page_w);
        u.relayout();
        u.range = None;
        u.asked.clear();
        u.ask_now();
    });
}

/// For the driver's `waitpreview`: no settle or width timer runs, and the current file has its
/// answer (a message, or every page kept holding its image or note at the current width).
pub fn settled() -> bool {
    with_ui(|u| {
        !u.settle.pending()
            && !u.settle_timer.running()
            && !u.width_timer.running()
            && !u.opening
            && u.range.is_none_or(|(a, b)| (a..=b).all(|p| u.held.get(p).copied().flatten() == Some(u.width_px)))
    })
    .unwrap_or(true)
}

fn on_reply(r: Reply) {
    with_ui(|u| {
        let seq = match &r {
            Reply::Message { seq, .. } | Reply::Pdf { seq, .. } | Reply::Page { seq, .. } | Reply::Note { seq, .. } | Reply::Skipped { seq, .. } => *seq,
        };
        if seq != u.seq {
            return;
        }
        let Some(app) = u.app.upgrade() else { return };
        match r {
            Reply::Message { text, .. } => {
                u.opening = false;
                app.set_preview_message(text.into());
            }
            Reply::Pdf { sizes, .. } => {
                u.opening = false;
                u.held = vec![None; sizes.len()];
                u.sizes = sizes;
                u.relayout();
                u.ask_now();
            }
            Reply::Page { page, width_px, buf, .. } => u.land(page, width_px, Image::from_rgba8(buf), String::new()),
            Reply::Note { page, width_px, text, .. } => u.land(page, width_px, Image::default(), text),
            Reply::Skipped { page, width_px, .. } => {
                if width_px == u.width_px {
                    u.asked.remove(&page);
                    u.ask_now();
                }
            }
        }
    });
}

impl Ui {
    /// A new generation for `target` (None: no file), the column emptied for it.
    fn show(&mut self, target: Option<Target>) {
        self.seq += 1;
        self.keep.lock().unwrap_or_else(|e| e.into_inner()).seq = self.seq;
        self.sizes.clear();
        self.slots.clear();
        self.held.clear();
        self.asked.clear();
        self.range = None;
        self.pages.set_vec(Vec::new());
        self.shown = target.as_ref().map(|t| t.path.clone());
        self.opening = target.is_some();
        if let Some(app) = self.app.upgrade() {
            let title = target.as_ref().and_then(|t| t.path.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            app.set_preview_title(title.into());
            app.set_preview_message("".into());
            app.set_preview_content_h(0.0);
            app.invoke_preview_set_scroll(0.0);
        }
        let _ = self.tx.send(match target {
            Some(t) => Req::Open { seq: self.seq, pane: t.pane, path: t.path, attrs: t.attrs, is_dir: t.is_dir },
            None => Req::Close,
        });
    }

    /// The slots at the current page width, the model's rows moved to them, and the render width.
    fn relayout(&mut self) {
        let Some(app) = self.app.upgrade() else { return };
        let (slots, total) = layout(&self.sizes, self.page_w.max(1.0));
        self.width_px = (self.page_w.max(1.0) * app.window().scale_factor()).round() as u32;
        if self.pages.row_count() != slots.len() {
            self.pages.set_vec(slots.iter().map(|s| PreviewPage { y: s.y, h: s.h, img: Image::default(), note: "".into() }).collect::<Vec<_>>());
        } else {
            for (i, s) in slots.iter().enumerate() {
                if let Some(mut row) = self.pages.row_data(i) {
                    (row.y, row.h) = (s.y, s.h);
                    self.pages.set_row_data(i, row);
                }
            }
        }
        self.slots = slots;
        app.set_preview_content_h(total);
        let mut k = self.keep.lock().unwrap_or_else(|e| e.into_inner());
        k.width_px = self.width_px;
    }

    /// `ask` at the column's scroll as it is now.
    fn ask_now(&mut self) {
        let Some(app) = self.app.upgrade() else { return };
        self.ask(app.invoke_preview_scroll_top(), app.invoke_preview_visible_h());
    }

    /// The pages kept at this scroll: when they changed, drop the images outside them (logged);
    /// then ask the worker for those not held at the current width, the ones in view first.
    fn ask(&mut self, top: f32, height: f32) {
        let Some(view) = in_view(&self.slots, top, height) else { return };
        let range = kept(view, self.slots.len());
        if Some(range) != self.range {
            self.range = Some(range);
            {
                let mut k = self.keep.lock().unwrap_or_else(|e| e.into_inner());
                (k.first, k.last) = range;
            }
            for p in (0..self.held.len()).filter(|p| !(range.0..=range.1).contains(p)) {
                if self.held[p].take().is_some() {
                    if let Some(mut row) = self.pages.row_data(p) {
                        (row.img, row.note) = (Image::default(), "".into());
                        self.pages.set_row_data(p, row);
                    }
                }
            }
            let kept_n = self.held.iter().filter(|h| h.is_some()).count();
            eprintln!("[preview] gen={} pages in view {}..{}, kept {kept_n} image(s)", self.seq, view.0 + 1, view.1 + 1);
        }
        if self.width_px == 0 {
            return;
        }
        let order = (view.0..=view.1).chain([range.0, range.1]);
        for p in order {
            if self.held[p] != Some(self.width_px) && self.asked.insert(p) {
                let _ = self.tx.send(Req::Page { seq: self.seq, page: p, width_px: self.width_px });
            }
        }
    }

    /// A page's image (or note) arrived: shown if it is still kept at the current width.
    fn land(&mut self, page: usize, width_px: u32, img: Image, note: String) {
        if width_px != self.width_px {
            return;
        }
        self.asked.remove(&page);
        if !self.range.is_some_and(|(a, b)| (a..=b).contains(&page)) || page >= self.held.len() {
            return;
        }
        if let Some(mut row) = self.pages.row_data(page) {
            (row.img, row.note) = (img, note.into());
            self.pages.set_row_data(page, row);
            self.held[page] = Some(width_px);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settle_waits_for_the_cursor_to_rest() {
        let mut s = Settle::default();
        assert_eq!(s.check(1000), Due::Idle, "no move, nothing to do");
        s.moved(1000);
        s.moved(1040);
        s.moved(1100);
        assert!(s.pending());
        assert_eq!(s.check(1150), Due::Wait(100), "a timer from an earlier move: wait for the last one");
        assert_eq!(s.check(1250), Due::Now);
        assert!(!s.pending());
        assert_eq!(s.check(1400), Due::Idle, "the move is spent");
        // Holding Down: a move every 30 ms never settles until the key is let go.
        let mut t = 0;
        for _ in 0..100 {
            s.moved(t);
            assert!(matches!(s.check(t + 30), Due::Wait(_)));
            t += 30;
        }
        assert_eq!(s.check(t - 30 + SETTLE_MS), Due::Now);
    }

    #[test]
    fn files_are_classified_without_reading_them() {
        const RECALL: u32 = FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS;
        let never = || panic!("asked only for a pdf");
        assert_eq!(classify("docs", 0x10, true, never), Class::Folder);
        assert_eq!(classify("a.pdf", 0x10, true, never), Class::Folder, "a folder named .pdf");
        assert_eq!(classify("notes.txt", 0, false, never), Class::Other);
        assert_eq!(classify("pdf", 0, false, never), Class::Other, "no extension");
        assert_eq!(classify("x.pdf.txt", RECALL, false, never), Class::Other, "type first: no cloud message for a text file");
        assert_eq!(classify("a.pdf", 0, false, || false), Class::Pdf);
        assert_eq!(classify("A.PDF", 0, false, || false), Class::Pdf, "any case");
        assert_eq!(classify("a.Pdf", 0, false, || true), Class::Archive);
        assert_eq!(classify("a.pdf", RECALL, false, || false), Class::CloudOnly);
        assert_eq!(classify("a.pdf", FILE_ATTRIBUTE_RECALL_ON_OPEN, false, || false), Class::CloudOnly);
        assert_eq!(classify("a.pdf", FILE_ATTRIBUTE_OFFLINE, false, || false), Class::CloudOnly);
        assert_eq!(classify("a.pdf", RECALL, false, || true), Class::Archive, "inside an archive first");
        assert_eq!(Class::Pdf.message(), None);
        assert_eq!(Class::Other.message(), Some(NO_PREVIEW));
        assert_eq!(Class::CloudOnly.message(), Some(CLOUD));
    }

    #[test]
    fn pages_are_laid_out_from_their_sizes() {
        // Letter portrait, A4 landscape, a square, at 300 px.
        let (slots, total) = layout(&[(612.0, 792.0), (842.0, 595.0), (100.0, 100.0)], 300.0);
        let h0 = 300.0 * 792.0 / 612.0;
        let h1 = 300.0 * 595.0 / 842.0;
        assert_eq!(slots[0], Slot { y: PAD, h: h0 });
        assert_eq!(slots[1], Slot { y: PAD + h0 + PAD, h: h1 });
        assert_eq!(slots[2], Slot { y: PAD + h0 + PAD + h1 + PAD, h: 300.0 });
        assert_eq!(total, slots[2].y + 300.0 + PAD);
        assert_eq!(layout(&[], 300.0), (vec![], PAD));
        // A size of zero (a broken page box) is laid out square, never zero high.
        assert_eq!(layout(&[(0.0, 50.0)], 200.0).0[0].h, 200.0);
    }

    #[test]
    fn the_pages_in_view_and_one_either_side_are_kept() {
        // Ten pages 100 px high, 8 px apart: page i spans 8 + 108 i to 108 (i + 1).
        let (slots, _) = layout(&[(100.0, 100.0); 10], 100.0);
        assert_eq!(in_view(&slots, 0.0, 150.0), Some((0, 1)));
        assert_eq!(in_view(&slots, 108.0, 100.0), Some((1, 1)), "only page 2 (its top at 116 is past 108, the gap shows page 1's end)");
        assert_eq!(in_view(&slots, 107.0, 100.0), Some((0, 1)), "page 1's last pixel still shows");
        assert_eq!(in_view(&slots, 500.0, 300.0), Some((4, 7)));
        assert_eq!(in_view(&slots, 5000.0, 300.0), Some((9, 9)), "past the end: the last page");
        assert_eq!(in_view(&slots, 0.0, 0.0), Some((0, 0)));
        assert_eq!(in_view(&[], 0.0, 100.0), None);
        assert_eq!(kept((0, 1), 10), (0, 2));
        assert_eq!(kept((4, 7), 10), (3, 8));
        assert_eq!(kept((9, 9), 10), (8, 9));
        assert_eq!(kept((0, 0), 1), (0, 0));
    }

    #[test]
    fn no_page_image_passes_8192_px() {
        assert_eq!(page_px((612.0, 792.0), 600), (600, 776, false));
        assert_eq!(page_px((842.0, 595.0), 1000), (1000, 707, false));
        // A receipt 20 times as tall as wide: 600 px wide would be 12,000 high.
        let (w, h, capped) = page_px((100.0, 2000.0), 600);
        assert!(capped);
        assert_eq!(h, MAX_SIDE);
        assert_eq!(w, (600.0f64 * 8192.0 / 12000.0).floor() as u32);
        // A banner wider than the cap.
        assert_eq!(page_px((1000.0, 100.0), 9000), (8192, 819, true));
        assert_eq!(page_px((100.0, 100.0), 8192), (8192, 8192, false), "at the cap is not over it");
        assert_eq!(page_px((100.0, 0.0), 10), (10, 1, false), "never zero high");
    }

    #[test]
    fn the_column_width_stays_in_bounds() {
        assert_eq!(clamp_ratio(0.35), 0.35);
        assert_eq!(clamp_ratio(0.05), RATIO_MIN);
        assert_eq!(clamp_ratio(0.9), RATIO_MAX);
        assert_eq!(clamp_ratio(f32::NAN), RATIO_DEFAULT);
        assert_eq!(clamp_ratio(f32::INFINITY), RATIO_DEFAULT);
    }

    /// A valid PDF of `pages` pages of 200 x 300 pt, each a red rectangle inset 20 pt on white,
    /// written by hand with a correct cross-reference table.
    pub fn tiny_pdf(pages: usize) -> Vec<u8> {
        let content = "1 0 0 rg 20 20 160 260 re f";
        let mut objs = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            format!("<< /Type /Pages /Kids [{}] /Count {pages} >>", (0..pages).map(|i| format!("{} 0 R", 3 + 2 * i)).collect::<Vec<_>>().join(" ")),
        ];
        for i in 0..pages {
            objs.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Contents {} 0 R >>", 4 + 2 * i));
            objs.push(format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()));
        }
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, o) in objs.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).bytes());
        for off in offsets {
            out.extend(format!("{off:010} 00000 n \n").bytes());
        }
        out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).bytes());
        out
    }

    #[test]
    fn the_worker_renders_a_pdf_and_refuses_a_damaged_one() {
        let dir = std::env::temp_dir().join(format!("twinfm-p8m1-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (pdf, bad, txt) = (dir.join("two.pdf"), dir.join("bad.PDF"), dir.join("a.txt"));
        std::fs::write(&pdf, tiny_pdf(2)).unwrap();
        std::fs::write(&bad, b"%PDF-1.4\nthis is not a pdf at all").unwrap();
        std::fs::write(&txt, b"text").unwrap();

        let (tx, rx) = unbounded();
        let (rtx, rrx) = std::sync::mpsc::channel::<Reply>();
        let keep = Arc::new(Mutex::new(Keep { seq: 1, first: 0, last: 1, width_px: 100 }));
        let k2 = keep.clone();
        let rtx = Mutex::new(rtx);
        let w = std::thread::spawn(move || worker(rx, k2, move |r| rtx.lock().unwrap().send(r).unwrap()));
        let recv = || rrx.recv_timeout(Duration::from_secs(30)).expect("a reply");
        let open = |seq, path: &Path| Req::Open { seq, pane: 0, path: path.to_path_buf(), attrs: 0, is_dir: false };

        tx.send(open(1, &pdf)).unwrap();
        match recv() {
            Reply::Pdf { seq: 1, sizes } => assert_eq!(sizes, vec![(200.0, 300.0); 2]),
            _ => panic!("page sizes"),
        }
        tx.send(Req::Page { seq: 1, page: 1, width_px: 100 }).unwrap();
        match recv() {
            Reply::Page { seq: 1, page: 1, width_px: 100, buf } => {
                assert_eq!((buf.width(), buf.height()), (100, 150));
                let px = |x: u32, y: u32| buf.as_slice()[(y * 100 + x) as usize];
                assert_eq!(px(50, 75), Rgba8Pixel { r: 255, g: 0, b: 0, a: 255 }, "the red rectangle");
                assert_eq!(px(2, 2), Rgba8Pixel { r: 255, g: 255, b: 255, a: 255 }, "the white page");
            }
            _ => panic!("page 2 rendered"),
        }
        // Out of the kept range or at another width: skipped, not rendered.
        keep.lock().unwrap().first = 1;
        tx.send(Req::Page { seq: 1, page: 0, width_px: 100 }).unwrap();
        assert!(matches!(recv(), Reply::Skipped { seq: 1, page: 0, width_px: 100 }));

        // A newer file supersedes the pages still queued for the older one.
        keep.lock().unwrap().seq = 2;
        tx.send(Req::Page { seq: 1, page: 1, width_px: 100 }).unwrap();
        tx.send(open(2, &bad)).unwrap();
        assert!(matches!(recv(), Reply::Message { seq: 2, text: DAMAGED }));
        tx.send(open(3, &txt)).unwrap();
        assert!(matches!(recv(), Reply::Message { seq: 3, text: NO_PREVIEW }));
        tx.send(Req::Open { seq: 4, pane: 0, path: dir.clone(), attrs: 0x10, is_dir: true }).unwrap();
        assert!(matches!(recv(), Reply::Message { seq: 4, text: NO_PREVIEW }));
        tx.send(open(5, &dir.join("missing.pdf"))).unwrap();
        assert!(matches!(recv(), Reply::Message { seq: 5, text: DAMAGED }), "a file that cannot be read");

        drop(tx);
        w.join().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
