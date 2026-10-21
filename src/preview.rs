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
//!
//! Since P8-M2 a Markdown file (`.md`, `.markdown`) is read on the same worker (its first 2 MB),
//! parsed into blocks (markdown.rs), each paragraph's, heading's and table cell's inline content
//! made a `StyledText` there, and answered as one list the UI shows in a ListView; then its local
//! pictures are decoded one at a time and answered as they come, the generation checked between
//! them. A link clicked in it is classified here and run on the UI thread (`link`).
//!
//! Since P9-M2 plain text (code blocks), HTML (html.rs, into the same blocks) and pictures (one
//! picture block and a note with its size and format) take the same path.

use crate::markdown::{self, Block, Kind};
use crate::{App, MdBlock, MdCell, MdRow, PreviewPage};
use crossbeam_channel::{Receiver, Sender, unbounded};
use slint::{ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, StyledText, Timer, TimerMode, VecModel};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, FILE_ATTRIBUTE_RECALL_ON_OPEN,
    GetFileAttributesW, INVALID_FILE_ATTRIBUTES,
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
pub const UNREADABLE: &str = "This file could not be read";
pub const PICTURE_UNREADABLE: &str = "This picture could not be read";
/// A Markdown file's pictures share this many bytes of decoded pixels (RGBA); one past it shows
/// its alt text. A picture file shown alone is scaled down to fit it (`fit_px`).
pub const PIXEL_BUDGET: u64 = 64 << 20;
/// A picture file shown alone may take this many bytes of pixels while it is decoded (`image`'s
/// own default limit; a 48-megapixel photo is 192 MB), before it is scaled down to the budget.
const DECODE_MAX: u64 = 512 << 20;

/// Plain text previewed as code blocks (P9-M2), any case. Anything not on a list is "No preview".
pub const TEXT_EXTS: &[&str] = &[
    "txt", "log", "ini", "cfg", "csv", "json", "xml", "yaml", "yml", "toml", "rs", "py", "ps1", "sh", "c", "h", "cpp", "cs", "js", "ts", "css",
    "slint", "bat", "cmd", "tsv", "conf", "reg", "inf", "diff", "patch", "psm1", "psd1", "hpp", "cc", "go", "java", "sql", "jsx", "tsx", "lock",
];
/// Pictures, decoded by the `image` crate (resvg for SVG). HEIC, HEIF and AVIF are not on it.
pub const PICTURE_EXTS: &[&str] = &["jpg", "jpeg", "jpe", "jfif", "png", "gif", "webp", "bmp", "tif", "tiff", "ico", "svg", "svgz"];
pub const HTML_EXTS: &[&str] = &["html", "htm"];
/// An SVG file larger than this is not read.
const SVG_MAX_BYTES: u64 = 16 << 20;
/// Of the blocks whose styled text fell back to plain, this many are logged one by one.
const FALLBACK_LINES: usize = 20;
/// Of a file's pictures, this many are logged one by one (a summary line follows).
const PICTURE_LINES: usize = 50;
/// The anchor's second move waits this long for the ListView to have laid out the first.
const ANCHOR_MS: u64 = 60;

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
    /// A type that is not previewed.
    Other,
    /// A file of a previewed type stored inside an archive.
    Archive,
    /// A file of a previewed type whose data is in the cloud: reading it would start a download.
    CloudOnly,
    Pdf,
    Markdown,
    Text,
    Picture,
    Html,
}

impl Class {
    /// The message the column shows instead of the document (None for a previewed type).
    pub fn message(self) -> Option<&'static str> {
        match self {
            Class::Folder | Class::Other => Some(NO_PREVIEW),
            Class::Archive => Some(ARCHIVE),
            Class::CloudOnly => Some(CLOUD),
            Class::Pdf | Class::Markdown | Class::Text | Class::Picture | Class::Html => None,
        }
    }

    /// For the `[preview]` lines.
    pub fn label(self) -> &'static str {
        match self {
            Class::Folder => "folder",
            Class::Other => "no preview for this type",
            Class::Archive => "inside an archive",
            Class::CloudOnly => "cloud-only",
            Class::Pdf => "pdf",
            Class::Markdown => "markdown",
            Class::Text => "text",
            Class::Picture => "picture",
            Class::Html => "html",
        }
    }
}

/// The listing's attributes say the file's data is not on this machine.
fn cloud_only(attrs: u32) -> bool {
    attrs & (FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_RECALL_ON_OPEN | FILE_ATTRIBUTE_OFFLINE) != 0
}

/// Classify from the listing's facts: a folder, then the type (by the extension, any case: PDF,
/// Markdown, and since P9-M2 the text, picture and HTML lists), then where it lies. `in_archive` is
/// asked only for a previewed type (it may cost a file system query on an archive-named
/// component); the cloud test is the listing's attributes, so nothing is read.
pub fn classify(name: &str, attrs: u32, is_dir: bool, in_archive: impl FnOnce() -> bool) -> Class {
    let ext = Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or("");
    let on = |list: &[&str]| list.iter().any(|x| x.eq_ignore_ascii_case(ext));
    let class = if on(&["pdf"]) {
        Class::Pdf
    } else if on(&["md", "markdown"]) {
        Class::Markdown
    } else if on(TEXT_EXTS) {
        Class::Text
    } else if on(PICTURE_EXTS) {
        Class::Picture
    } else if on(HTML_EXTS) {
        Class::Html
    } else {
        Class::Other
    };
    if is_dir {
        Class::Folder
    } else if class == Class::Other {
        Class::Other
    } else if in_archive() {
        Class::Archive
    } else if cloud_only(attrs) {
        Class::CloudOnly
    } else {
        class
    }
}

/// A file's attributes without opening it (so a cloud placeholder is not recalled); None when it
/// is not there.
pub fn file_attrs(path: &Path) -> Option<u32> {
    let w = crate::shellmenu::wide(path.as_os_str());
    // SAFETY: a NUL-terminated wide path that outlives the call.
    let a = unsafe { GetFileAttributesW(w.as_ptr()) };
    (a != INVALID_FILE_ATTRIBUTES).then_some(a)
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

/// One Markdown block as the worker sends it: plain data and `StyledText` (both `Send`); the UI
/// thread makes the `MdBlock` and its table models.
struct Shown {
    kind: Kind,
    level: u8,
    indent: u8,
    quote: u8,
    marker: String,
    task: u8,
    text: StyledText,
    /// A code block's lines, the note's text.
    code: String,
    /// A table's rows: the head flag and each cell's text, alignment and column share.
    rows: Vec<(bool, Vec<(StyledText, u8, f32)>)>,
    alt: String,
    first: bool,
    last: bool,
}

/// From the worker.
enum Reply {
    Message { seq: u64, text: &'static str },
    Pdf { seq: u64, sizes: Vec<(f32, f32)> },
    /// A Markdown, text, HTML or picture file's blocks, each block's anchors (a heading's slug and
    /// an HTML element's ids, space separated; "" for none), and how many pictures follow (one
    /// `Picture` each).
    Blocks { seq: u64, blocks: Vec<Shown>, anchors: Vec<String>, pictures: usize },
    /// Block `block`'s picture, or None (its alt text stays). `physical`: its pixels are screen
    /// pixels (a picture file shown alone, never drawn larger than itself), else logical px.
    Picture { seq: u64, block: usize, pic: Option<SharedPixelBuffer<Rgba8Pixel>>, physical: bool },
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
    match class {
        Class::Markdown | Class::Text | Class::Html => {
            serve_blocks(keep, reply, seq, pane, path, class);
            return None;
        }
        Class::Picture => {
            serve_picture(reply, seq, pane, path);
            return None;
        }
        _ => {}
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

/// A paragraph's, heading's or cell's inline Markdown as styled text; on Slint's refusal its plain
/// text, logged (the first `FALLBACK_LINES` one by one).
fn styled(md: &str, plain: &str, seq: u64, block: usize, fell: &mut usize) -> StyledText {
    match StyledText::from_markdown(md) {
        Ok(t) => t,
        Err(e) => {
            *fell += 1;
            if *fell <= FALLBACK_LINES {
                eprintln!("[preview] gen={seq} block {block}: styled text fell back to plain: {}", e.to_string().replace('\n', "; "));
            }
            StyledText::from_plain_text(plain)
        }
    }
}

/// Read, cut and parse a Markdown, text or HTML file, answer its blocks, then decode its local
/// pictures one at a time, each answered as it is done, until they are all done or the UI has
/// moved to another file.
fn serve_blocks(keep: &Mutex<Keep>, reply: &impl Fn(Reply), seq: u64, pane: usize, path: &Path, class: Class) {
    let t0 = Instant::now();
    let read = || -> std::io::Result<(Vec<u8>, u64)> {
        let f = std::fs::File::open(path)?;
        let size = f.metadata()?.len();
        let mut bytes = Vec::new();
        f.take(markdown::MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
        Ok((bytes, size))
    };
    let base = path.parent().unwrap_or(Path::new("")).to_path_buf();
    // The document, whether it was cut, the file's size, and (text) its lines and encoding.
    let parsed = read().map_err(|e| e.to_string()).and_then(|(bytes, size)| {
        std::panic::catch_unwind(|| {
            let (text, cut) = markdown::decode(&bytes);
            let (doc, lines) = match class {
                Class::Html => (crate::html::parse(&text, Some(&base)), 0),
                Class::Text => {
                    let (blocks, lines) = markdown::text_blocks(&text);
                    (markdown::Doc { blocks, remote: Vec::new() }, lines)
                }
                _ => (markdown::parse_doc(&text, Some(&base)), 0),
            };
            (doc, cut, size, lines, markdown::encoding(&bytes))
        })
        .map_err(|p| {
            let msg = panic_text(&*p);
            eprintln!("[preview] worker panicked on {}: {msg}", path.display());
            format!("panicked: {msg}")
        })
    });
    let (doc, cut, size, lines, encoding) = match parsed {
        Ok(p) => p,
        Err(why) => {
            eprintln!("[preview] pane={pane} gen={seq} {}: unreadable: {why}", path.display());
            reply(Reply::Message { seq, text: UNREADABLE });
            return;
        }
    };
    let mut blocks = doc.blocks;
    if cut {
        let mb = size as f64 / (1024.0 * 1024.0);
        blocks.insert(0, Block { kind: Kind::Note, plain: format!("Only the first 2 MB of this {mb:.1} MB file is shown."), ..Default::default() });
    }
    let (shown, anchors, fell) = shown_blocks(&blocks, seq);
    let count = |k: Kind| blocks.iter().filter(|b| b.kind == k && (b.first || !matches!(k, Kind::Code | Kind::Table))).count();
    let pictures = count(Kind::Image);
    let cut_note = if cut { ", cut at 2 MB" } else { "" };
    let ms = crate::perf::elapsed_ms(t0);
    let (headings, code, tables) = (count(Kind::Heading), count(Kind::Code), count(Kind::Table));
    let n = blocks.len();
    match class {
        Class::Text => eprintln!("[preview] pane={pane} gen={seq} {}: text, {lines} lines, {size} bytes, {encoding}{cut_note}", path.display()),
        _ => eprintln!(
            "[preview] pane={pane} gen={seq} {}: {}, {n} blocks ({headings} headings, {code} code, {tables} tables, {pictures} images), parsed in {ms:.0} ms{cut_note}",
            path.display(),
            class.label()
        ),
    }
    if fell > FALLBACK_LINES {
        eprintln!("[preview] gen={seq} {} more block(s) fell back to plain", fell - FALLBACK_LINES);
    }
    for src in &doc.remote {
        eprintln!("[preview] gen={seq} image {src}: alt text (remote)");
    }
    reply(Reply::Blocks { seq, blocks: shown, anchors, pictures });
    decode_pictures(keep, reply, seq, &blocks, &base, pictures);
}

/// The blocks as the worker sends them, each paragraph's, heading's and cell's styled text made,
/// with each block's anchors (an HTML element's ids, and a heading's slug), and how many fell back
/// to plain text.
fn shown_blocks(blocks: &[Block], seq: u64) -> (Vec<Shown>, Vec<String>, usize) {
    let mut fell = 0;
    let mut slugs = markdown::Slugger::default();
    let mut anchors = Vec::with_capacity(blocks.len());
    let shown: Vec<Shown> = blocks
        .iter()
        .enumerate()
        .map(|(k, b)| {
            anchors.push(match (b.kind == Kind::Heading, b.anchor.is_empty()) {
                (true, true) => slugs.slug(&b.plain),
                (true, false) => format!("{} {}", b.anchor, slugs.slug(&b.plain)),
                (false, _) => b.anchor.clone(),
            });
            let text = match b.kind {
                Kind::Para | Kind::Heading => styled(&b.md, &b.plain, seq, k, &mut fell),
                _ => StyledText::default(),
            };
            let rows = b
                .rows
                .iter()
                .map(|r| {
                    let cells = r.cells.iter().enumerate().map(|(c, cell)| (styled(&cell.md, &cell.plain, seq, k, &mut fell), b.aligns[c], b.weights[c]));
                    (r.head, cells.collect())
                })
                .collect();
            let code = if matches!(b.kind, Kind::Code | Kind::Note) { b.plain.clone() } else { String::new() };
            let alt = if b.kind == Kind::Image { b.plain.clone() } else { String::new() };
            Shown { kind: b.kind, level: b.level, indent: b.indent, quote: b.quote, marker: b.marker.clone(), task: b.task, text, code, rows, alt, first: b.first, last: b.last }
        })
        .collect();
    (shown, anchors, fell)
}

/// Decode a document's local pictures one at a time, each answered as it is done, until they are
/// all done or the UI has moved to another file.
fn decode_pictures(keep: &Mutex<Keep>, reply: &impl Fn(Reply), seq: u64, blocks: &[Block], base: &Path, pictures: usize) {
    let mut budget = PIXEL_BUDGET;
    let mut left = pictures;
    // A file shown more than once (an icon repeated down a page) is decoded and counted once.
    let mut done: HashMap<PathBuf, Result<SharedPixelBuffer<Rgba8Pixel>, String>> = HashMap::new();
    let (mut shown_n, mut alt_n, t_all) = (0, 0, Instant::now());
    for (k, b) in blocks.iter().enumerate().filter(|(_, b)| b.kind == Kind::Image) {
        if keep.lock().unwrap_or_else(|e| e.into_inner()).seq != seq {
            eprintln!("[preview] gen={seq} superseded: {left} picture(s) not decoded");
            return;
        }
        left -= 1;
        let t = Instant::now();
        let got = match markdown::image_path(&b.src, base) {
            Some(p) => done
                .entry(p)
                .or_insert_with_key(|p| {
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| picture(p, &mut budget)))
                        .unwrap_or_else(|p| Err(format!("failed: panicked: {}", panic_text(&*p))))
                        .map(|img| SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(img.as_raw(), img.width(), img.height()))
                })
                .clone(),
            None => Err("remote".into()),
        };
        let log = shown_n + alt_n < PICTURE_LINES;
        match got {
            Ok(buf) => {
                shown_n += 1;
                if log {
                    eprintln!("[preview] gen={seq} image {k} {}: {}x{} px in {:.0} ms", b.src, buf.width(), buf.height(), crate::perf::elapsed_ms(t));
                }
                reply(Reply::Picture { seq, block: k, pic: Some(buf), physical: false });
            }
            Err(why) => {
                alt_n += 1;
                if log {
                    eprintln!("[preview] gen={seq} image {k} {}: alt text ({why})", b.src);
                }
                reply(Reply::Picture { seq, block: k, pic: None, physical: false });
            }
        }
    }
    if pictures > 0 {
        eprintln!(
            "[preview] gen={seq} pictures done: {shown_n} shown, {alt_n} alt text, {} file(s) decoded, {} MB of the budget, in {:.0} ms",
            done.values().filter(|r| r.is_ok()).count(),
            (PIXEL_BUDGET - budget) >> 20,
            crate::perf::elapsed_ms(t_all)
        );
    }
}

/// A picture file (P9-M2): decoded alone, upright, scaled down to fit (`fit_px`), answered as a
/// picture block and a note with its size, the file's size and its format.
fn serve_picture(reply: &impl Fn(Reply), seq: u64, pane: usize, path: &Path) {
    let t0 = Instant::now();
    let got = std::panic::catch_unwind(|| lone_picture(path)).unwrap_or_else(|p| {
        let msg = panic_text(&*p);
        eprintln!("[preview] worker panicked on {}: {msg}", path.display());
        Err(format!("panicked: {msg}"))
    });
    let (img, (w, h), format) = match got {
        Ok(g) => g,
        Err(why) => {
            eprintln!("[preview] pane={pane} gen={seq} {}: picture failed: {why}", path.display());
            reply(Reply::Message { seq, text: PICTURE_UNREADABLE });
            return;
        }
    };
    let scaled = if img.dimensions() != (w, h) { format!(", scaled to {}x{}", img.width(), img.height()) } else { String::new() };
    eprintln!("[preview] pane={pane} gen={seq} {}: picture {w}x{h} px, {format}, decoded in {:.0} ms{scaled}", path.display(), crate::perf::elapsed_ms(t0));
    let size = std::fs::metadata(path).map_or(0, |m| m.len());
    let alt = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let blocks = [
        Block { kind: Kind::Image, plain: alt, ..Default::default() },
        Block { kind: Kind::Note, plain: format!("{w} x {h}, {}, {format}", size_text(size)), ..Default::default() },
    ];
    let (shown, anchors, _) = shown_blocks(&blocks, seq);
    reply(Reply::Blocks { seq, blocks: shown, anchors, pictures: 1 });
    let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(img.as_raw(), img.width(), img.height());
    reply(Reply::Picture { seq, block: 0, pic: Some(buf), physical: true });
}

/// A file's size for the picture's note: bytes under 1 KB, KB under 1 MB, else MB with one decimal.
pub fn size_text(bytes: u64) -> String {
    match bytes {
        b if b < 1024 => format!("{b} bytes"),
        b if b < 1024 * 1024 => format!("{} KB", (b as f64 / 1024.0).round() as u64),
        b => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
    }
}

/// The size a picture file is shown at (P9-M2): scaled down so that neither side passes `MAX_SIDE`
/// and its RGBA pixels fit `PIXEL_BUDGET`; never scaled up.
pub fn fit_px(w: u32, h: u32) -> (u32, u32) {
    let (wf, hf) = (f64::from(w.max(1)), f64::from(h.max(1)));
    let f = (f64::from(MAX_SIDE) / wf.max(hf)).min(((PIXEL_BUDGET / 4) as f64 / (wf * hf)).sqrt());
    if f >= 1.0 { (w, h) } else { ((wf * f).floor().max(1.0) as u32, (hf * f).floor().max(1.0) as u32) }
}

/// A picture file shown alone: decoded with up to `DECODE_MAX` of pixels, then scaled to `fit_px`.
/// Returns the image, its upright size before scaling, and its format's name.
fn lone_picture(path: &Path) -> Result<(image::RgbaImage, (u32, u32), &'static str), String> {
    let (img, format) = decode_picture(path, DECODE_MAX).map_err(|e| if e == "over the budget" { format!("over {} MB of pixels", DECODE_MAX >> 20) } else { e })?;
    let (w, h) = img.dimensions();
    let (fw, fh) = fit_px(w, h);
    let img = if (fw, fh) == (w, h) { img } else { image::imageops::resize(&img, fw, fh, image::imageops::FilterType::Triangle) };
    Ok((img, (w, h), format))
}

/// A local picture of a document decoded to straight RGBA, at most `MAX_SIDE` on a side (scaled
/// down to it), its decoded size taken from `budget`. Err: why its alt text shows instead
/// (`missing`, `cloud-only`, `over the budget`, `failed: ...`).
fn picture(path: &Path, budget: &mut u64) -> Result<image::RgbaImage, String> {
    let (img, _) = decode_picture(path, *budget)?;
    let (w, h) = img.dimensions();
    *budget -= u64::from(w) * u64::from(h) * 4;
    if w.max(h) <= MAX_SIDE {
        return Ok(img);
    }
    let f = f64::from(MAX_SIDE) / f64::from(w.max(h));
    let (nw, nh) = ((f64::from(w) * f).round().max(1.0) as u32, (f64::from(h) * f).round().max(1.0) as u32);
    Ok(image::imageops::resize(&img, nw.min(MAX_SIDE), nh.min(MAX_SIDE), image::imageops::FilterType::Triangle))
}

/// A picture file decoded to straight RGBA, upright (a JPEG's EXIF orientation applied, P9-M2), an
/// SVG drawn at its own size up to `MAX_SIDE`; `over the budget` when its pixels would take more
/// than `max` bytes. Also its format's name, from its bytes. The attributes come first, so a
/// cloud-only file is never opened.
fn decode_picture(path: &Path, max: u64) -> Result<(image::RgbaImage, &'static str), String> {
    match file_attrs(path) {
        None => return Err("missing".into()),
        Some(a) if a & FILE_ATTRIBUTE_DIRECTORY != 0 => return Err("failed: a folder".into()),
        Some(a) if cloud_only(a) => return Err("cloud-only".into()),
        Some(_) => {}
    }
    let fail = |e: &dyn std::fmt::Display| format!("failed: {e}");
    let svg = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg") || e.eq_ignore_ascii_case("svgz"));
    if svg {
        let f = std::fs::File::open(path).map_err(|e| fail(&e))?;
        if f.metadata().map_err(|e| fail(&e))?.len() > SVG_MAX_BYTES {
            return Err("failed: an SVG over 16 MB".into());
        }
        let mut data = Vec::new();
        f.take(SVG_MAX_BYTES).read_to_end(&mut data).map_err(|e| fail(&e))?;
        // No fonts are loaded: text in an SVG is not drawn.
        let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default()).map_err(|e| fail(&e))?;
        let size = tree.size();
        let (sw, sh) = (size.width(), size.height());
        let f = (MAX_SIDE as f32 / sw.max(sh)).min(1.0);
        let (w, h) = ((sw * f).ceil().max(1.0) as u32, (sh * f).ceil().max(1.0) as u32);
        if u64::from(w) * u64::from(h) * 4 > max {
            return Err("over the budget".into());
        }
        let mut pm = resvg::tiny_skia::Pixmap::new(w, h).ok_or_else(|| "failed: an empty picture".to_string())?;
        resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(w as f32 / sw, h as f32 / sh), &mut pm.as_mut());
        let rgba = pm.pixels().iter().flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        });
        return Ok((image::RgbaImage::from_raw(w, h, rgba.collect()).ok_or_else(|| "failed: a bad size".to_string())?, "SVG"));
    }
    use image::ImageDecoder;
    let open = || image::ImageReader::open(path).map_err(|e| fail(&e))?.with_guessed_format().map_err(|e| fail(&e));
    let (w, h) = open()?.into_dimensions().map_err(|e| fail(&e))?;
    let bytes = u64::from(w) * u64::from(h) * 4;
    if bytes > max {
        return Err("over the budget".into());
    }
    let mut r = open()?;
    let format = match r.format() {
        Some(image::ImageFormat::Jpeg) => "JPEG",
        Some(image::ImageFormat::Png) => "PNG",
        Some(image::ImageFormat::Gif) => "GIF",
        Some(image::ImageFormat::WebP) => "WebP",
        Some(image::ImageFormat::Bmp) => "BMP",
        Some(image::ImageFormat::Tiff) => "TIFF",
        Some(image::ImageFormat::Ico) => "ICO",
        _ => "picture",
    };
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(w);
    limits.max_image_height = Some(h);
    limits.max_alloc = Some(2 * bytes + (16 << 20));
    r.limits(limits);
    let mut dec = r.into_decoder().map_err(|e| fail(&e))?;
    let orientation = dec.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(dec).map_err(|e| fail(&e))?;
    img.apply_orientation(orientation);
    Ok((img.into_rgba8(), format))
}

/// The PDF shown.
struct PdfDoc {
    /// The pages' sizes in points.
    sizes: Vec<(f32, f32)>,
    slots: Vec<Slot>,
    /// Per page, the render width of the image (or note) it holds.
    held: Vec<Option<u32>>,
    /// Pages asked of the worker and not answered yet.
    asked: HashSet<usize>,
    /// The pages kept (in view and one either side).
    range: Option<(usize, usize)>,
}

/// The Markdown file shown.
struct MdDoc {
    /// Each block's heading slug ("" for other blocks), for `#anchor` links.
    anchors: Vec<String>,
    /// Pictures the worker has still to answer.
    pending: usize,
}

/// What the column shows: nothing yet (or a message), a PDF's pages, or a Markdown file's blocks.
enum Doc {
    None,
    Pdf(PdfDoc),
    Markdown(MdDoc),
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
    /// An `#anchor` link's second move (`scroll_to_block`).
    anchor_timer: Timer,
    seq: u64,
    /// The file the current generation previews.
    shown: Option<PathBuf>,
    /// Waiting for the worker's answer to the open.
    opening: bool,
    doc: Doc,
    /// The width a page is drawn at (logical px, from the column) and rendered at (physical px).
    page_w: f32,
    width_px: u32,
    pages: Rc<VecModel<PreviewPage>>,
    blocks: Rc<VecModel<MdBlock>>,
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
    let blocks = Rc::new(VecModel::<MdBlock>::default());
    app.set_preview_blocks(ModelRc::from(blocks.clone()));
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
            anchor_timer: Timer::default(),
            seq: 0,
            shown: None,
            opening: false,
            doc: Doc::None,
            page_w: 0.0,
            width_px: 0,
            pages,
            blocks,
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
        u.anchor_timer.stop();
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
        if u.visible {
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
        // A Markdown file's blocks wrap at the new width by themselves.
        if u.visible && matches!(u.doc, Doc::Pdf(_)) {
            u.width_timer.start(TimerMode::SingleShot, Duration::from_millis(WIDTH_SETTLE_MS), width_settled);
        }
    });
}

fn width_settled() {
    with_ui(|u| {
        if let Doc::Pdf(p) = &mut u.doc {
            eprintln!("[preview] width {:.0} px: re-render after settle", u.page_w);
            p.range = None;
            p.asked.clear();
            u.relayout();
            u.ask_now();
        }
    });
}

/// For the driver's `waitpreview`: no settle, width or anchor timer runs, and the current file has
/// its answer (a message; a PDF with every page kept holding its image or note at the current
/// width; a Markdown file with its blocks and every picture answered).
pub fn settled() -> bool {
    with_ui(|u| {
        !u.settle.pending()
            && !u.settle_timer.running()
            && !u.width_timer.running()
            && !u.anchor_timer.running()
            && !u.opening
            && match &u.doc {
                Doc::None => true,
                Doc::Pdf(p) => p.range.is_none_or(|(a, b)| (a..=b).all(|i| p.held.get(i).copied().flatten() == Some(u.width_px))),
                Doc::Markdown(m) => m.pending == 0,
            }
    })
    .unwrap_or(true)
}

/// A block as the column's model takes it (its table rows as models of their own).
fn md_block(s: Shown) -> MdBlock {
    let rows = s.rows.into_iter().map(|(head, cells)| {
        let cells: Vec<MdCell> = cells.into_iter().map(|(text, align, weight)| MdCell { text, align: i32::from(align), weight }).collect();
        MdRow { head, cells: ModelRc::new(VecModel::from(cells)) }
    });
    let rows: Vec<MdRow> = rows.collect();
    MdBlock {
        kind: s.kind as i32,
        level: i32::from(s.level),
        indent: i32::from(s.indent),
        quote: i32::from(s.quote),
        marker: s.marker.into(),
        task: i32::from(s.task),
        text: s.text,
        code: s.code.into(),
        rows: if rows.is_empty() { ModelRc::default() } else { ModelRc::new(VecModel::from(rows)) },
        img: Image::default(),
        img_w: 0.0,
        img_h: 0.0,
        alt: s.alt.into(),
        first: s.first,
        last: s.last,
    }
}

fn on_reply(r: Reply) {
    with_ui(|u| {
        let seq = match &r {
            Reply::Message { seq, .. }
            | Reply::Pdf { seq, .. }
            | Reply::Blocks { seq, .. }
            | Reply::Picture { seq, .. }
            | Reply::Page { seq, .. }
            | Reply::Note { seq, .. }
            | Reply::Skipped { seq, .. } => *seq,
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
                u.doc = Doc::Pdf(PdfDoc { held: vec![None; sizes.len()], sizes, slots: Vec::new(), asked: HashSet::new(), range: None });
                app.set_preview_mode(0);
                u.relayout();
                u.ask_now();
            }
            Reply::Blocks { seq, blocks, anchors, pictures } => {
                u.opening = false;
                u.doc = Doc::Markdown(MdDoc { anchors, pending: pictures });
                let t = Instant::now();
                let n = blocks.len();
                app.set_preview_mode(1);
                u.blocks.set_vec(blocks.into_iter().map(md_block).collect::<Vec<_>>());
                app.invoke_preview_set_scroll(0.0);
                eprintln!("[preview] gen={seq} {n} blocks in the list in {:.1} ms (UI thread)", crate::perf::elapsed_ms(t));
            }
            Reply::Picture { block, pic, physical, .. } => {
                if let Doc::Markdown(m) = &mut u.doc {
                    m.pending = m.pending.saturating_sub(1);
                }
                if let (Some(buf), Some(mut row)) = (pic, u.blocks.row_data(block)) {
                    // A document's picture's pixels count as logical px, as a browser counts them;
                    // a picture file's as screen px, so it is never drawn larger than itself. The
                    // column scales a wider one down to its width.
                    let s = if physical { app.window().scale_factor() } else { 1.0 };
                    (row.img_w, row.img_h) = (buf.width() as f32 / s, buf.height() as f32 / s);
                    row.img = Image::from_rgba8(buf);
                    u.blocks.set_row_data(block, row);
                }
            }
            Reply::Page { page, width_px, buf, .. } => u.land(page, width_px, Image::from_rgba8(buf), String::new()),
            Reply::Note { page, width_px, text, .. } => u.land(page, width_px, Image::default(), text),
            Reply::Skipped { page, width_px, .. } => {
                if let (Doc::Pdf(p), true) = (&mut u.doc, width_px == u.width_px) {
                    p.asked.remove(&page);
                    u.ask_now();
                }
            }
        }
    });
}

/// A link clicked in the Markdown preview (StyledText's `link-clicked`, with the destination as
/// written), or the driver's `previewlink`: a web or mail link opens in the default browser or mail
/// program, `#anchor` scrolls to its heading, a local folder opens in the active pane, a program or
/// script is shown there (never run), another file opens as a double-click does. A scripted run
/// never starts a program: the browser and the document are only logged.
pub fn link(dest: SharedString) {
    // No borrow held past here: navigating the pane moves its cursor, which comes back to `kick`.
    let Some(base) = with_ui(|u| u.shown.as_ref().and_then(|p| p.parent()).map(Path::to_path_buf)).flatten() else { return };
    let scripted = std::env::var_os("TWINFM_SCRIPT").is_some();
    let note = if scripted { " (scripted: not launched)" } else { "" };
    match markdown::classify_link(&dest, &base) {
        markdown::Link::Url(url) => {
            eprintln!("[preview] link \"{dest}\": browser{note}");
            if !scripted {
                if let Err(e) = crate::shellmenu::open_url(crate::preview_hwnd(), &url) {
                    eprintln!("[preview] link \"{dest}\": ShellExecuteExW failed, error {e}");
                }
            }
        }
        markdown::Link::Anchor(slug) => match with_ui(|u| match &u.doc {
            Doc::Markdown(m) => m.anchors.iter().position(|a| a.split(' ').any(|x| x == slug)),
            _ => None,
        }) {
            Some(Some(k)) => {
                eprintln!("[preview] link \"{dest}\": anchor -> block {k}");
                scroll_to_block(k);
            }
            _ => eprintln!("[preview] link \"{dest}\": refused: no heading #{slug}"),
        },
        markdown::Link::Local(path) => match file_attrs(&path) {
            None => eprintln!("[preview] link \"{dest}\": refused: {} is not there", path.display()),
            Some(a) => match markdown::local_kind(&path, a & FILE_ATTRIBUTE_DIRECTORY != 0) {
                markdown::Local::Folder => {
                    eprintln!("[preview] link \"{dest}\": open {}", path.display());
                    crate::show_in_pane(&path, false);
                }
                markdown::Local::Program => {
                    eprintln!("[preview] link \"{dest}\": show {}", path.display());
                    crate::show_in_pane(&path, true);
                }
                markdown::Local::Document => {
                    eprintln!("[preview] link \"{dest}\": open {}{note}", path.display());
                    if !scripted {
                        let dir = path.parent().map_or_else(PathBuf::new, Path::to_path_buf);
                        let res = crate::shellmenu::open_item(crate::preview_hwnd(), &path, &dir);
                        eprintln!("[open] preview link {}: {res:?}", path.display());
                    }
                }
            },
        },
        markdown::Link::Refused(why) => eprintln!("[preview] link \"{dest}\": refused: {why}"),
    }
}

/// Scroll the Markdown list so block `k` is at the top. The ListView knows only the rows it has
/// built, and places the others by their average height: a move far from those rows is a seek that
/// puts row k exactly at the top, a move near them a walk that lands near k by the real heights.
/// So the list goes to the far end from the target first, and to the target 60 ms later, once it
/// has laid that out.
fn scroll_to_block(k: usize) {
    with_ui(|u| {
        let Some(app) = u.app.upgrade() else { return };
        let n = u.blocks.row_count().max(1) as f32;
        let (total, vis) = (app.invoke_preview_total_h(), app.invoke_preview_visible_h());
        let target = k as f32 * total / n;
        app.invoke_preview_set_scroll(if target > 2.0 * vis { 0.0 } else { (total - vis).max(0.0) });
        let seq = u.seq;
        u.anchor_timer.start(TimerMode::SingleShot, Duration::from_millis(ANCHOR_MS), move || {
            with_ui(|u| {
                let Some(app) = u.app.upgrade() else { return };
                if u.seq != seq {
                    return;
                }
                let n = u.blocks.row_count().max(1) as f32;
                let (total, vis) = (app.invoke_preview_total_h(), app.invoke_preview_visible_h());
                app.invoke_preview_set_scroll((k as f32 * total / n).min((total - vis).max(0.0)));
                eprintln!("[preview] gen={seq} scrolled to block {k}: top {:.0} of {total:.0} px", app.invoke_preview_scroll_top());
            });
        });
    });
}

impl Ui {
    /// A new generation for `target` (None: no file), the column emptied for it.
    fn show(&mut self, target: Option<Target>) {
        self.seq += 1;
        self.keep.lock().unwrap_or_else(|e| e.into_inner()).seq = self.seq;
        self.doc = Doc::None;
        self.anchor_timer.stop();
        self.pages.set_vec(Vec::new());
        self.blocks.set_vec(Vec::new());
        self.shown = target.as_ref().map(|t| t.path.clone());
        self.opening = target.is_some();
        if let Some(app) = self.app.upgrade() {
            let title = target.as_ref().and_then(|t| t.path.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            app.set_preview_title(title.into());
            app.set_preview_message("".into());
            app.set_preview_content_h(0.0);
            app.set_preview_mode(0);
            app.invoke_preview_set_scroll(0.0);
        }
        let _ = self.tx.send(match target {
            Some(t) => Req::Open { seq: self.seq, pane: t.pane, path: t.path, attrs: t.attrs, is_dir: t.is_dir },
            None => Req::Close,
        });
    }

    /// The PDF's slots at the current page width, the model's rows moved to them, and the render
    /// width.
    fn relayout(&mut self) {
        let Some(app) = self.app.upgrade() else { return };
        let Doc::Pdf(p) = &mut self.doc else { return };
        let (slots, total) = layout(&p.sizes, self.page_w.max(1.0));
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
        p.slots = slots;
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
        let Doc::Pdf(d) = &mut self.doc else { return };
        let Some(view) = in_view(&d.slots, top, height) else { return };
        let range = kept(view, d.slots.len());
        if Some(range) != d.range {
            d.range = Some(range);
            {
                let mut k = self.keep.lock().unwrap_or_else(|e| e.into_inner());
                (k.first, k.last) = range;
            }
            for p in (0..d.held.len()).filter(|p| !(range.0..=range.1).contains(p)) {
                if d.held[p].take().is_some() {
                    if let Some(mut row) = self.pages.row_data(p) {
                        (row.img, row.note) = (Image::default(), "".into());
                        self.pages.set_row_data(p, row);
                    }
                }
            }
            let kept_n = d.held.iter().filter(|h| h.is_some()).count();
            eprintln!("[preview] gen={} pages in view {}..{}, kept {kept_n} image(s)", self.seq, view.0 + 1, view.1 + 1);
        }
        if self.width_px == 0 {
            return;
        }
        let order = (view.0..=view.1).chain([range.0, range.1]);
        for p in order {
            if d.held[p] != Some(self.width_px) && d.asked.insert(p) {
                let _ = self.tx.send(Req::Page { seq: self.seq, page: p, width_px: self.width_px });
            }
        }
    }

    /// A page's image (or note) arrived: shown if it is still kept at the current width.
    fn land(&mut self, page: usize, width_px: u32, img: Image, note: String) {
        let Doc::Pdf(d) = &mut self.doc else { return };
        if width_px != self.width_px {
            return;
        }
        d.asked.remove(&page);
        if !d.range.is_some_and(|(a, b)| (a..=b).contains(&page)) || page >= d.held.len() {
            return;
        }
        if let Some(mut row) = self.pages.row_data(page) {
            (row.img, row.note) = (img, note.into());
            self.pages.set_row_data(page, row);
            d.held[page] = Some(width_px);
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
        assert_eq!(classify("data.dat", 0, false, never), Class::Other);
        assert_eq!(classify("pdf", 0, false, never), Class::Other, "no extension");
        assert_eq!(classify("x.pdf.dat", RECALL, false, never), Class::Other, "type first: no cloud message for a type not previewed");
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
        // Markdown (P8-M2): the same checks as a PDF.
        assert_eq!(classify("README.md", 0, false, || false), Class::Markdown);
        assert_eq!(classify("notes.MARKDOWN", 0, false, || false), Class::Markdown);
        assert_eq!(classify("a.md", 0, false, || true), Class::Archive);
        assert_eq!(classify("a.md", RECALL, false, || false), Class::CloudOnly);
        assert_eq!(classify("a.mdx", 0, false, never), Class::Other);
        assert_eq!(Class::Markdown.message(), None);
        // Text, pictures and HTML (P9-M2): the same checks.
        assert_eq!(classify("notes.txt", 0, false, || false), Class::Text);
        assert_eq!(classify("main.RS", 0, false, || false), Class::Text);
        assert_eq!(classify("Cargo.lock", 0, false, || false), Class::Text);
        assert_eq!(classify("photo.JPG", 0, false, || false), Class::Picture);
        assert_eq!(classify("logo.svg", 0, false, || false), Class::Picture);
        assert_eq!(classify("scan.tiff", 0, false, || false), Class::Picture);
        assert_eq!(classify("page.htm", 0, false, || false), Class::Html);
        assert_eq!(classify("a.txt", 0, false, || true), Class::Archive);
        assert_eq!(classify("a.png", RECALL, false, || false), Class::CloudOnly);
        assert_eq!(classify("a.html", RECALL, false, || false), Class::CloudOnly);
        assert_eq!(classify("phone.heic", 0, false, never), Class::Other, "HEIC is not previewed");
        assert_eq!(classify("Makefile", 0, false, never), Class::Other, "no extension, no guess");
        for c in [Class::Text, Class::Picture, Class::Html] {
            assert_eq!(c.message(), None);
        }
    }

    #[test]
    fn a_picture_file_fits_and_is_never_scaled_up() {
        assert_eq!(fit_px(100, 50), (100, 50), "small: as it is");
        assert_eq!(fit_px(4032, 3024), (4032, 3024), "a 12-megapixel phone photo fits the budget");
        assert_eq!(fit_px(10_000, 1_000), (MAX_SIDE, 819), "a 10,000 px side scaled to 8,192");
        assert_eq!(fit_px(1_000, 10_000), (819, MAX_SIDE));
        // 48 megapixels: the budget (64 MB of RGBA, 16.7 megapixels) decides, the shape kept.
        let (w, h) = fit_px(8000, 6000);
        assert!(u64::from(w) * u64::from(h) * 4 <= PIXEL_BUDGET && (w, h) == (4729, 3547), "{w}x{h}");
        let (w, h) = fit_px(10_000, 3_000);
        assert!(w < MAX_SIDE && u64::from(w) * u64::from(h) * 4 <= PIXEL_BUDGET, "a panorama: {w}x{h}");
        assert_eq!(fit_px(0, 0), (0, 0), "nothing to scale");
        assert_eq!(size_text(812), "812 bytes");
        assert_eq!(size_text(340 * 1024 + 100), "340 KB");
        assert_eq!(size_text(2_411_724), "2.3 MB");
    }

    /// A JPEG of `w` x `h` (left half red, right half blue) with an EXIF orientation tag.
    fn jpeg_with_orientation(w: u32, h: u32, orientation: u8) -> Vec<u8> {
        let img = image::RgbImage::from_fn(w, h, |x, _| if x < w / 2 { image::Rgb([255, 0, 0]) } else { image::Rgb([0, 0, 255]) });
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
        let jpg = out.into_inner();
        // APP1 "Exif": a little-endian TIFF header and one IFD entry, 0x0112 SHORT = orientation.
        let mut app1 = b"Exif\0\0II*\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0".to_vec();
        app1.extend([orientation, 0, 0, 0, 0, 0, 0, 0]);
        let mut with = jpg[..2].to_vec();
        with.extend([0xFF, 0xE1]);
        with.extend(((app1.len() + 2) as u16).to_be_bytes());
        with.extend(app1);
        with.extend(&jpg[2..]);
        with
    }

    #[test]
    fn a_phone_photo_shows_upright() {
        let dir = std::env::temp_dir().join(format!("twinfm-p9m2-pic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (rotated, plain, bmp, bad) = (dir.join("phone.jpg"), dir.join("plain.jpg"), dir.join("a.bmp"), dir.join("bad.png"));
        std::fs::write(&rotated, jpeg_with_orientation(40, 20, 6)).unwrap();
        std::fs::write(&plain, jpeg_with_orientation(40, 20, 1)).unwrap();
        image::RgbImage::from_pixel(3, 2, image::Rgb([0, 200, 0])).save(&bmp).unwrap();
        std::fs::write(&bad, b"not a png").unwrap();
        // Orientation 6: turned a quarter clockwise to be upright, so the red half is on top.
        let (img, size, format) = lone_picture(&rotated).unwrap();
        assert_eq!((img.dimensions(), size, format), ((20, 40), (20, 40), "JPEG"));
        let top = img.get_pixel(10, 5).0;
        assert!(top[0] > 200 && top[2] < 60, "red on top: {top:?}");
        let (img, ..) = lone_picture(&plain).unwrap();
        assert_eq!(img.dimensions(), (40, 20));
        let (img, _, format) = lone_picture(&bmp).unwrap();
        assert_eq!((img.dimensions(), format), ((3, 2), "BMP"));
        assert!(lone_picture(&bad).unwrap_err().starts_with("failed: "));
        assert_eq!(lone_picture(&dir.join("gone.png")).unwrap_err(), "missing");
        // A document's pictures are upright too.
        let mut budget = PIXEL_BUDGET;
        assert_eq!(picture(&rotated, &mut budget).unwrap().dimensions(), (20, 40));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A PNG of `w` x `h` pixels of one colour, encoded by the image crate.
    fn png(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
        let mut img = image::RgbImage::new(w, h);
        img.pixels_mut().for_each(|p| p.0 = rgb);
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn pictures_are_capped() {
        let dir = std::env::temp_dir().join(format!("twinfm-p8m2-pic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (small, wide, svg) = (dir.join("small.png"), dir.join("wide.png"), dir.join("a.svg"));
        std::fs::write(&small, png(40, 20, [255, 0, 0])).unwrap();
        std::fs::write(&wide, png(9000, 10, [0, 0, 255])).unwrap();
        std::fs::write(&svg, r#"<svg xmlns="http://www.w3.org/2000/svg" width="30" height="10"><rect width="30" height="10" fill="lime"/></svg>"#).unwrap();
        std::fs::write(dir.join("bad.png"), b"not a png").unwrap();
        let mut budget = PIXEL_BUDGET;
        let img = picture(&small, &mut budget).unwrap();
        assert_eq!((img.dimensions(), img.get_pixel(5, 5).0), ((40, 20), [255, 0, 0, 255]));
        assert_eq!(budget, PIXEL_BUDGET - 40 * 20 * 4, "its decoded size taken from the budget");
        let img = picture(&wide, &mut budget).unwrap();
        assert_eq!(img.dimensions(), (MAX_SIDE, 9), "a side over 8,192 px scaled down");
        let img = picture(&svg, &mut budget).unwrap();
        assert_eq!((img.dimensions(), img.get_pixel(15, 5).0), ((30, 10), [0, 255, 0, 255]));
        let mut little = 100;
        assert_eq!(picture(&small, &mut little), Err("over the budget".into()));
        assert_eq!(little, 100);
        assert_eq!(picture(&dir.join("gone.png"), &mut budget), Err("missing".into()));
        assert!(picture(&dir.join("bad.png"), &mut budget).unwrap_err().starts_with("failed: "));
        assert!(picture(&dir, &mut budget).unwrap_err().starts_with("failed: "), "a folder");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_worker_previews_markdown_with_a_picture() {
        let dir = std::env::temp_dir().join(format!("twinfm-p8m2-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("img")).unwrap();
        std::fs::write(dir.join("img").join("dot.png"), png(8, 4, [0, 128, 0])).unwrap();
        let md = dir.join("README.md");
        std::fs::write(&md, "# Title\n\nSome *text* and a [link](#title).\n\n![dot](img/dot.png)\n\n![gone](img/gone.png)\n\n| a | b |\n|---|--:|\n| 1 | 2 |\n").unwrap();
        let big = dir.join("big.md");
        std::fs::write(&big, "line\n".repeat(markdown::MAX_BYTES / 5 + 10)).unwrap();

        let (tx, rx) = unbounded();
        let (rtx, rrx) = std::sync::mpsc::channel::<Reply>();
        let keep = Arc::new(Mutex::new(Keep { seq: 1, ..Default::default() }));
        let k2 = keep.clone();
        let rtx = Mutex::new(rtx);
        let w = std::thread::spawn(move || worker(rx, k2, move |r| rtx.lock().unwrap().send(r).unwrap()));
        let recv = || rrx.recv_timeout(Duration::from_secs(30)).expect("a reply");
        let open = |seq, path: &Path| Req::Open { seq, pane: 0, path: path.to_path_buf(), attrs: 0, is_dir: false };

        tx.send(open(1, &md)).unwrap();
        match recv() {
            Reply::Blocks { seq: 1, blocks, anchors, pictures } => {
                let kinds: Vec<Kind> = blocks.iter().map(|b| b.kind).collect();
                assert_eq!(kinds, vec![Kind::Heading, Kind::Para, Kind::Image, Kind::Image, Kind::Table]);
                assert_eq!(anchors[0], "title");
                assert_eq!(pictures, 2);
                assert_eq!(blocks[2].alt, "dot");
                assert_eq!(blocks[4].rows.len(), 2);
                assert_eq!(blocks[4].rows[1].1[1].1, 2, "the second column right-aligned");
            }
            _ => panic!("the blocks"),
        }
        match recv() {
            Reply::Picture { seq: 1, block: 2, pic: Some(buf), .. } => {
                assert_eq!((buf.width(), buf.height()), (8, 4));
                assert_eq!(buf.as_slice()[0], Rgba8Pixel { r: 0, g: 128, b: 0, a: 255 });
            }
            _ => panic!("the picture"),
        }
        assert!(matches!(recv(), Reply::Picture { seq: 1, block: 3, pic: None, .. }), "a missing picture keeps its alt text");

        // Over 2 MB: the note first, the rest cut.
        keep.lock().unwrap().seq = 2;
        tx.send(open(2, &big)).unwrap();
        match recv() {
            Reply::Blocks { seq: 2, blocks, pictures: 0, .. } => {
                assert_eq!(blocks[0].kind, Kind::Note);
                assert!(blocks[0].code.starts_with("Only the first 2 MB"));
            }
            _ => panic!("the cut file"),
        }
        tx.send(open(3, &dir.join("gone.md"))).unwrap();
        assert!(matches!(recv(), Reply::Message { seq: 3, text: UNREADABLE }));

        drop(tx);
        w.join().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_worker_previews_text_pictures_and_html() {
        let dir = std::env::temp_dir().join(format!("twinfm-p9m2-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("img")).unwrap();
        std::fs::write(dir.join("img").join("dot.png"), png(8, 4, [0, 128, 0])).unwrap();
        let txt = dir.join("main.rs");
        std::fs::write(&txt, (0..45).map(|i| format!("\tline {i}\r\n")).collect::<String>()).unwrap();
        let utf16 = dir.join("wide.txt");
        std::fs::write(&utf16, [0xFF, 0xFE].into_iter().chain("é\nü\n".encode_utf16().flat_map(u16::to_le_bytes)).collect::<Vec<u8>>()).unwrap();
        let big = dir.join("big.log");
        std::fs::write(&big, "log line\n".repeat(markdown::MAX_BYTES / 9 + 10)).unwrap();
        let pic = dir.join("photo.jpg");
        std::fs::write(&pic, jpeg_with_orientation(40, 20, 6)).unwrap();
        let broken = dir.join("broken.png");
        std::fs::write(&broken, b"\x89PNG not really").unwrap();
        let page = dir.join("page.html");
        std::fs::write(&page, "<html><head><title>T</title><script>x()</script></head><body><h1 id=top>Title</h1><p>Text <a href='#top'>up</a></p><img src='img/dot.png' alt=dot><ul><li>one</ul></body></html>").unwrap();

        let (tx, rx) = unbounded();
        let (rtx, rrx) = std::sync::mpsc::channel::<Reply>();
        let keep = Arc::new(Mutex::new(Keep { seq: 1, ..Default::default() }));
        let k2 = keep.clone();
        let rtx = Mutex::new(rtx);
        let w = std::thread::spawn(move || worker(rx, k2, move |r| rtx.lock().unwrap().send(r).unwrap()));
        let recv = || rrx.recv_timeout(Duration::from_secs(30)).expect("a reply");
        let open = |seq, path: &Path| Req::Open { seq, pane: 0, path: path.to_path_buf(), attrs: 0, is_dir: false };

        // Text: code blocks of 40 lines, tabs as spaces, no pictures.
        tx.send(open(1, &txt)).unwrap();
        match recv() {
            Reply::Blocks { seq: 1, blocks, pictures: 0, .. } => {
                assert_eq!(blocks.iter().map(|b| b.kind).collect::<Vec<_>>(), vec![Kind::Code, Kind::Code]);
                assert!(blocks[0].code.starts_with("    line 0\n    line 1\n"));
                assert_eq!(blocks[1].code.lines().count(), 5);
            }
            _ => panic!("the text"),
        }
        keep.lock().unwrap().seq = 2;
        tx.send(open(2, &utf16)).unwrap();
        assert!(matches!(recv(), Reply::Blocks { seq: 2, blocks, .. } if blocks[0].code == "é\nü"), "UTF-16 by its BOM");
        keep.lock().unwrap().seq = 3;
        tx.send(open(3, &big)).unwrap();
        match recv() {
            Reply::Blocks { seq: 3, blocks, .. } => {
                assert_eq!(blocks[0].kind, Kind::Note);
                assert!(blocks[0].code.starts_with("Only the first 2 MB"));
                assert_eq!(blocks[1].kind, Kind::Code);
            }
            _ => panic!("the cut log"),
        }

        // A picture: its block and its note, then its pixels, upright and physical.
        keep.lock().unwrap().seq = 4;
        tx.send(open(4, &pic)).unwrap();
        match recv() {
            Reply::Blocks { seq: 4, blocks, pictures: 1, .. } => {
                assert_eq!(blocks.iter().map(|b| b.kind).collect::<Vec<_>>(), vec![Kind::Image, Kind::Note]);
                assert!(blocks[1].code.starts_with("20 x 40, ") && blocks[1].code.ends_with(" bytes, JPEG"), "{}", blocks[1].code);
            }
            _ => panic!("the picture's blocks"),
        }
        assert!(matches!(recv(), Reply::Picture { seq: 4, block: 0, pic: Some(b), physical: true } if (b.width(), b.height()) == (20, 40)));
        keep.lock().unwrap().seq = 5;
        tx.send(open(5, &broken)).unwrap();
        assert!(matches!(recv(), Reply::Message { seq: 5, text: PICTURE_UNREADABLE }));

        // HTML: the head and script gone, the heading anchored by its id and slug, the local picture.
        keep.lock().unwrap().seq = 6;
        tx.send(open(6, &page)).unwrap();
        match recv() {
            Reply::Blocks { seq: 6, blocks, anchors, pictures: 1 } => {
                assert_eq!(blocks.iter().map(|b| b.kind).collect::<Vec<_>>(), vec![Kind::Heading, Kind::Para, Kind::Image, Kind::Para]);
                assert_eq!(anchors[0], "top title");
                assert_eq!(blocks[3].marker, "\u{2022}");
            }
            _ => panic!("the page"),
        }
        assert!(matches!(recv(), Reply::Picture { seq: 6, block: 2, pic: Some(_), physical: false }));

        drop(tx);
        w.join().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
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
        let (pdf, bad, txt) = (dir.join("two.pdf"), dir.join("bad.PDF"), dir.join("a.dat"));
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
