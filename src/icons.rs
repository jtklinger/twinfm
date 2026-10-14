//! Shell icons. A pool of worker threads (`ICON_WORKERS`), each initialised for COM (MTA), owns
//! every shell call: a worker turns a path into a 32-bit HBITMAP through
//! `IShellItemImageFactory::GetImage` and the bitmap into a premultiplied RGBA pixel buffer, with
//! the shell's overlay (a link's arrow) drawn over it for links and reparse points. The UI thread keeps one buffer per cache key in an
//! `IconTable` that both panes share, and composes the icons of the rows on screen into one
//! strip image per pane (`compose`), so the list rows carry no image at all.
//!
//! Cache keys (`icon_key`): ordinary files share one key per lowercase extension (`ext:.txt`),
//! plain directories share `dir`; files whose icon is their own (`.exe`, `.lnk`, `.ico`, `.url`,
//! `.cur`, no extension), customised folders (a `desktop.ini` that the shell honours), reparse
//! points and drives are keyed by full path. A cloud placeholder (OneDrive) is keyed as if it
//! were no reparse point: its icon is its type's, or the folder's.

use crate::model::Kind;
use crossbeam_channel::{Receiver, Sender};
use slint::{Rgba8Pixel, SharedPixelBuffer, SharedString};
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Instant;
use windows_sys::Win32::Foundation::SIZE;
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject,
    GdiFlush, GetDC, GetDIBits, GetObjectW, HBITMAP, ReleaseDC, SelectObject,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SYSTEM,
};
use windows_sys::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows_sys::Win32::UI::Shell::{
    SHCreateItemFromParsingName, SHFILEINFOW, SHGFI_ADDOVERLAYS, SHGFI_ICON, SHGFI_OVERLAYINDEX, SHGFI_SMALLICON, SHGetFileInfoW,
    SHGetImageList, SHIL_LARGE, SHIL_SMALL, SIIGBF, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{DI_NORMAL, DestroyIcon, DrawIconEx, HICON};
use windows_sys::core::{GUID, HRESULT};

/// Icon table slots that exist before any shell icon arrives, one per kind of row.
pub const PLACEHOLDER_FILE: u16 = 0;
pub const PLACEHOLDER_FOLDER: u16 = 1;
pub const PLACEHOLDER_DRIVE: u16 = 2;
pub const PLACEHOLDERS: u16 = 3;

/// The logical size icons are drawn at; the buffer is this times the window scale factor.
pub const ICON_LOGICAL_PX: f32 = 32.0;

/// Row height in the list, in logical pixels (pane.slint).
pub const ROW_PX: f32 = 32.0;

/// Extensions whose icon lives in the file itself, so they are keyed by full path.
const OWN_ICON_EXTS: [&str; 5] = ["exe", "lnk", "ico", "url", "cur"];

/// One icon fetch. `generation` is the requesting pane's navigation generation; the worker
/// skips the request when the pane has moved on.
pub struct Request {
    pub generation: u64,
    pub pane: usize,
    pub key: SharedString,
    pub path: PathBuf,
    pub size_px: u32,
    /// Draw the shell's overlay (a link's arrow) over the icon (`wants_overlay`).
    pub overlay: bool,
}

pub enum Outcome {
    Pixels(SharedPixelBuffer<Rgba8Pixel>),
    /// The shell had no icon for this path; the rows keep their placeholder.
    Failed,
    /// Not rendered (the pane navigated away, or the key was delivered before); the UI may ask again.
    Skipped,
}

pub struct Reply {
    pub key: SharedString,
    pub outcome: Outcome,
}

/// Worker-side counters, read by the UI's once-a-second `[icons]` line.
#[derive(Default)]
pub struct Stats {
    pub rendered: AtomicU64,
    pub render_ns: AtomicU64,
    /// Icons that got an overlay drawn over them.
    pub overlays: AtomicU64,
    /// Renders (overlay included) over `SLOW_MS`.
    pub slow: AtomicU64,
}

/// Lowercase extension after the last dot, as Explorer and `TypeNames` read it, or None.
fn extension(name: &str) -> Option<&str> {
    match name.rfind('.') {
        Some(i) if i + 1 < name.len() => Some(&name[i + 1..]),
        _ => None,
    }
}

/// `attrs` as the key rules read them: a cloud placeholder's icon is its type's (a file) or
/// the plain folder's, so it is keyed as if it were no reparse point. Sharing those keys is
/// what spares one shell call per placeholder (one took 568 ms in M2).
fn key_attrs(attrs: u32, kind: Kind) -> u32 {
    if kind == Kind::Cloud { attrs & !FILE_ATTRIBUTE_REPARSE_POINT } else { attrs }
}

/// The cache key for a directory entry (drives are keyed by their root, see `Entry::drive`).
/// `custom_folder` says the directory carries a `desktop.ini` the shell honours (`custom_folder`).
pub fn icon_key(folder: &Path, name: &str, attrs: u32, kind: Kind, custom_folder: bool) -> String {
    let full = || folder.join(name).to_string_lossy().into_owned();
    let attrs = key_attrs(attrs, kind);
    if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return full();
    }
    if attrs & FILE_ATTRIBUTE_DIRECTORY != 0 {
        return if custom_folder { full() } else { "dir".to_string() };
    }
    match extension(name) {
        Some(ext) if !OWN_ICON_EXTS.iter().any(|o| o.eq_ignore_ascii_case(ext)) => {
            format!("ext:.{}", ext.to_ascii_lowercase())
        }
        _ => full(),
    }
}

/// Whether an entry's icon should carry the shell's overlay: links (`.lnk`, `.url`), junctions,
/// symbolic links, app execution aliases and other reparse points. Each of them is keyed by full
/// path, so the overlay never lands on a key other files share; a folder named `x.lnk` (keyed
/// `dir`) does not qualify.
pub fn wants_overlay(kind: Kind, attrs: u32, name: &str) -> bool {
    match kind {
        Kind::Junction | Kind::Symlink | Kind::AppAlias | Kind::Reparse => true,
        Kind::Drive => false,
        _ => {
            attrs & FILE_ATTRIBUTE_DIRECTORY == 0
                && extension(name).is_some_and(|e| e.eq_ignore_ascii_case("lnk") || e.eq_ignore_ascii_case("url"))
        }
    }
}

/// Whether a directory has its own icon through `desktop.ini`. The shell reads `desktop.ini`
/// only in folders marked read-only or system, so only those cost a file probe: unfiltered, the
/// probe cost 25 to 155 us per directory (1.1 to 4.2 s for the 26,867 folders of
/// `C:\Windows\WinSxS`); filtered, 0.26 ms for System32 and nothing for WinSxS.
pub fn custom_folder(folder: &Path, name: &str, attrs: u32) -> bool {
    attrs & FILE_ATTRIBUTE_DIRECTORY != 0
        && attrs & FILE_ATTRIBUTE_REPARSE_POINT == 0
        && attrs & (FILE_ATTRIBUTE_READONLY | FILE_ATTRIBUTE_SYSTEM) != 0
        && folder.join(name).join("desktop.ini").exists()
}

/// Per-enumeration key builder: extension keys and `dir` are shared SharedStrings, so most
/// entries get a refcount bump rather than an allocation. Lives on the enumeration worker.
pub struct IconKeys {
    folder: PathBuf,
    dir: SharedString,
    ext: HashMap<String, SharedString>,
}

impl IconKeys {
    pub fn new(folder: PathBuf) -> IconKeys {
        IconKeys { folder, dir: "dir".into(), ext: HashMap::new() }
    }

    pub fn for_entry(&mut self, name: &str, attrs: u32, kind: Kind) -> SharedString {
        let attrs = key_attrs(attrs, kind);
        let custom = custom_folder(&self.folder, name, attrs);
        if attrs & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) == FILE_ATTRIBUTE_DIRECTORY && !custom {
            return self.dir.clone();
        }
        if attrs & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) == 0 {
            if let Some(ext) = extension(name) {
                if let Some(k) = self.ext.get(ext) {
                    return k.clone();
                }
            }
        }
        let key = icon_key(&self.folder, name, attrs, kind, custom);
        if key.starts_with("ext:") {
            let k: SharedString = key.as_str().into();
            // Cached under the extension as spelled; another spelling misses once and caches too.
            if let Some(ext) = extension(name) {
                self.ext.insert(ext.to_string(), k.clone());
            }
            return k;
        }
        key.into()
    }
}

/// First and last row (inclusive) whose 22 px band intersects the viewport, for a list of
/// `len` rows scrolled to `content_y` (zero or negative, as Slint reports it) with `height`
/// visible. None for an empty list.
pub fn visible_range(content_y: f32, height: f32, len: usize) -> Option<(usize, usize)> {
    if len == 0 {
        return None;
    }
    let top = (-content_y).max(0.0);
    let first = ((top / ROW_PX).floor() as usize).min(len - 1);
    let last = (((top + height.max(0.0)) / ROW_PX).ceil() as usize).clamp(first, len - 1);
    Some((first, last))
}

/// Rows composed into the strip above and below the visible range, so a scroll of less than
/// this many rows moves the strip image without recomposing it.
pub const STRIP_MARGIN_ROWS: usize = 4;

/// Physical geometry of an icon strip at one window scale factor. Sizes are rounded once, so
/// the strip is composed at integer pixels whatever the scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StripGeom {
    /// Icon edge in physical pixels (32 logical px times the scale factor, rounded).
    pub icon_px: u32,
    pub scale: f32,
}

impl StripGeom {
    pub fn new(scale: f32) -> StripGeom {
        StripGeom { icon_px: ((ICON_LOGICAL_PX * scale).round() as u32).max(1), scale }
    }

    /// Physical y of the icon of the k-th row of the strip: the row's top plus the centering
    /// offset of a 32 px icon in a 32 px row.
    pub fn icon_y(&self, k: usize) -> u32 {
        ((k as f32 * ROW_PX + (ROW_PX - ICON_LOGICAL_PX) / 2.0) * self.scale).round() as u32
    }

    /// Physical height of a strip of `rows` rows.
    pub fn height(&self, rows: usize) -> u32 {
        (rows as f32 * ROW_PX * self.scale).round() as u32
    }
}

/// The rows (inclusive) a strip covers for the visible range `(first, last)` of a list of `len`
/// rows: the visible rows plus `STRIP_MARGIN_ROWS` each side, clamped to the list.
pub fn strip_range((first, last): (usize, usize), len: usize) -> (usize, usize) {
    (first.saturating_sub(STRIP_MARGIN_ROWS), (last + STRIP_MARGIN_ROWS).min(len.saturating_sub(1)))
}

/// What a pane's strip was last composed from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Composed {
    pub first: usize,
    pub last: usize,
    pub icon_px: u32,
}

/// Whether a pane's strip must be recomposed: nothing composed yet, its content went stale
/// (`dirty`: an icon landed for a row in it, or the rows changed), the scale changed, the
/// visible range left the composed rows, or the list shrank under them. A scroll that keeps
/// the visible rows inside the composed ones only moves the image.
pub fn needs_recompose(c: Option<Composed>, visible: (usize, usize), len: usize, icon_px: u32, dirty: bool) -> bool {
    match c {
        None => true,
        Some(c) => dirty || c.icon_px != icon_px || visible.0 < c.first || visible.1 > c.last || c.last >= len,
    }
}

/// Compose a strip: `icons[k]` (each `g.icon_px` square, premultiplied) is copied, one scanline
/// at a time, to `y = g.icon_y(k)` of a buffer `g.icon_px` wide and `g.height(icons.len())`
/// tall. An icon of any other size is left out (the table hands out fitted buffers).
pub fn compose(icons: &[SharedPixelBuffer<Rgba8Pixel>], g: &StripGeom) -> SharedPixelBuffer<Rgba8Pixel> {
    let w = g.icon_px as usize;
    let h = g.height(icons.len()) as usize;
    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(w as u32, h as u32);
    let dst = buf.make_mut_slice();
    for (k, icon) in icons.iter().enumerate() {
        if icon.width() != g.icon_px || icon.height() != g.icon_px {
            continue;
        }
        let y0 = g.icon_y(k) as usize;
        for (r, line) in icon.as_slice().chunks_exact(w).enumerate() {
            let at = (y0 + r) * w;
            if at + w > dst.len() {
                break;
            }
            dst[at..at + w].copy_from_slice(line);
        }
    }
    buf
}

/// Box-filter resample of a premultiplied buffer to `px` square: each target pixel averages
/// the source pixels its area covers (at least one). Used once per icon and size.
pub fn fit(src: &SharedPixelBuffer<Rgba8Pixel>, px: u32) -> SharedPixelBuffer<Rgba8Pixel> {
    let (sw, sh) = (src.width() as usize, src.height() as usize);
    if sw == 0 || sh == 0 {
        return SharedPixelBuffer::new(px, px);
    }
    if (sw, sh) == (px as usize, px as usize) {
        return src.clone();
    }
    let n = px as usize;
    let s = src.as_slice();
    let mut out = SharedPixelBuffer::<Rgba8Pixel>::new(px, px);
    let span = |i: usize, len: usize| {
        let a = i * len / n;
        (a, ((i + 1) * len / n).max(a + 1).min(len))
    };
    for (y, row) in out.make_mut_slice().chunks_exact_mut(n).enumerate() {
        let (y0, y1) = span(y, sh);
        for (x, d) in row.iter_mut().enumerate() {
            let (x0, x1) = span(x, sw);
            let mut acc = [0u32; 4];
            for yy in y0..y1 {
                for p in &s[yy * sw + x0..yy * sw + x1] {
                    acc[0] += p.r as u32;
                    acc[1] += p.g as u32;
                    acc[2] += p.b as u32;
                    acc[3] += p.a as u32;
                }
            }
            let c = ((y1 - y0) * (x1 - x0)) as u32;
            *d = Rgba8Pixel { r: (acc[0] / c) as u8, g: (acc[1] / c) as u8, b: (acc[2] / c) as u8, a: (acc[3] / c) as u8 };
        }
    }
    out
}

/// Convert GetDIBits output (top-down, 32-bit BGRA, premultiplied) to RGBA. Bitmaps from icons
/// with no alpha channel come back with every alpha byte zero; those are opaque.
pub fn bgra_to_rgba(bgra: &[u8], width: u32, height: u32) -> SharedPixelBuffer<Rgba8Pixel> {
    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
    let opaque = bgra.chunks_exact(4).all(|p| p[3] == 0);
    for (dst, src) in buf.make_mut_slice().iter_mut().zip(bgra.chunks_exact(4)) {
        *dst = Rgba8Pixel { r: src[2], g: src[1], b: src[0], a: if opaque { 255 } else { src[3] } };
    }
    buf
}

/// `over` drawn over `base`, source-over on premultiplied pixels: `dst = src + dst * (1 - src.a)`
/// per channel. The shell's overlay images are full icon size with the glyph in the bottom-left
/// corner, so the two buffers line up at (0, 0). Buffers of different sizes give `base` back.
pub fn overlay(base: &SharedPixelBuffer<Rgba8Pixel>, over: &SharedPixelBuffer<Rgba8Pixel>) -> SharedPixelBuffer<Rgba8Pixel> {
    let mut out = base.clone();
    if (base.width(), base.height()) != (over.width(), over.height()) {
        return out;
    }
    for (d, s) in out.make_mut_slice().iter_mut().zip(over.as_slice()) {
        let keep = 255 - s.a as u32;
        let mix = |s: u8, d: u8| (s as u32 + (d as u32 * keep + 127) / 255).min(255) as u8;
        *d = Rgba8Pixel { r: mix(s.r, d.r), g: mix(s.g, d.g), b: mix(s.b, d.b), a: mix(s.a, d.a) };
    }
    out
}

#[repr(C)]
struct ImageFactoryVtbl {
    _query_interface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    _add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    get_image: unsafe extern "system" fn(*mut c_void, SIZE, SIIGBF, *mut HBITMAP) -> HRESULT,
}

const IID_ISHELLITEMIMAGEFACTORY: GUID = GUID::from_u128(0xbcc18b79_ba16_442f_80c4_8a59c30c463b);

/// The shell's icon for `path` at `size_px` square, or None. Needs COM: the worker, and the UI
/// thread once per drag for the drag image (`dnd::attach_drag_image`).
pub(crate) fn render(path: &Path, size_px: u32) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut factory: *mut c_void = std::ptr::null_mut();
    // SAFETY: `wide` is NUL-terminated and outlives the call; `factory` receives an interface
    // pointer of the IID asked for, or stays null on failure.
    let hr = unsafe { SHCreateItemFromParsingName(wide.as_ptr(), std::ptr::null_mut(), &IID_ISHELLITEMIMAGEFACTORY, &mut factory) };
    if hr < 0 || factory.is_null() {
        return None;
    }
    let mut hbm: HBITMAP = std::ptr::null_mut();
    let size = SIZE { cx: size_px as i32, cy: size_px as i32 };
    // SAFETY: `factory` is a live IShellItemImageFactory, whose vtable starts with IUnknown's
    // three methods followed by GetImage; it is released exactly once here.
    let hr = unsafe {
        let vtbl = *(factory as *const *const ImageFactoryVtbl);
        let hr = ((*vtbl).get_image)(factory, size, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK, &mut hbm);
        ((*vtbl).release)(factory);
        hr
    };
    if hr < 0 || hbm.is_null() {
        return None;
    }
    let pixels = hbitmap_pixels(hbm);
    // SAFETY: GetImage handed us ownership of the bitmap.
    unsafe { DeleteObject(hbm) };
    pixels
}

/// Read a bitmap's pixels as top-down 32-bit BGRA and convert them.
fn hbitmap_pixels(hbm: HBITMAP) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    // SAFETY: `bm` is a writable BITMAP of the size passed.
    let mut bm: BITMAP = unsafe { std::mem::zeroed() };
    if unsafe { GetObjectW(hbm, std::mem::size_of::<BITMAP>() as i32, &mut bm as *mut BITMAP as *mut c_void) } == 0 {
        return None;
    }
    let (w, h) = (bm.bmWidth, bm.bmHeight.abs());
    if w <= 0 || h <= 0 || w > 1024 || h > 1024 {
        return None;
    }
    // SAFETY: zeroed BITMAPINFO is a valid starting point; the header fields are set below.
    let mut bmi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w,
        biHeight: -h, // negative: top-down rows
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..bmi.bmiHeader
    };
    let mut bgra = vec![0u8; (w * h * 4) as usize];
    // SAFETY: `bgra` holds h rows of w 32-bit pixels, as `bmi` describes; the screen DC is
    // released before returning.
    let lines = unsafe {
        let dc = GetDC(std::ptr::null_mut());
        let n = GetDIBits(dc, hbm, 0, h as u32, bgra.as_mut_ptr() as *mut c_void, &mut bmi, DIB_RGB_COLORS);
        ReleaseDC(std::ptr::null_mut(), dc);
        n
    };
    (lines == h).then(|| bgra_to_rgba(&bgra, w as u32, h as u32))
}

/// Threads that render icons. Two, so one slow shell call (a network path, a dead share, a
/// placeholder the sync engine must answer) does not hold the icons queued behind it: a
/// synchronous shell call cannot be abandoned, so the other thread serves the queue meanwhile.
pub const ICON_WORKERS: usize = 2;

/// A render (overlay included) at least this long is logged as `[icons] slow icon`.
const SLOW_MS: u64 = 100;

/// How a worker turns a path into pixels: `render`, or a stand-in in the tests.
type RenderFn = fn(&Path, u32) -> Option<SharedPixelBuffer<Rgba8Pixel>>;

/// What the worker threads share.
#[derive(Default)]
struct Pool {
    /// Requests not yet taken, served newest first, and whether the request channel has closed.
    queue: Mutex<(Vec<Request>, bool)>,
    ready: Condvar,
    /// Keys rendered or being rendered: a key is taken when a thread pops it, so the two threads
    /// never render one key twice. A failed key is given back (the UI's `failed` set is what stops
    /// it being asked for again).
    taken: Mutex<HashSet<SharedString>>,
    /// Overlay images by (overlay index, size in px), fetched once per process (`overlay_image`).
    overlays: Mutex<HashMap<(i32, u32), Option<SharedPixelBuffer<Rgba8Pixel>>>>,
}

/// A lock that a panic elsewhere cannot poison for good: the data is plain sets and queues.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Start the icon workers (`ICON_WORKERS` threads sharing one queue). They serve the queue
/// newest first, skip requests whose pane has navigated since (`generations`), render each key
/// at most once per process, draw the shell's overlay when asked, and wake the UI thread after
/// every reply.
pub fn spawn(
    rx: Receiver<Request>,
    tx: Sender<Reply>,
    generations: Arc<[AtomicU64; 2]>,
    stats: Arc<Stats>,
    wake: impl Fn() + Send + Sync + 'static,
) {
    spawn_with(rx, tx, generations, stats, wake, render);
}

/// `spawn` with the render function as a parameter, so a test can make one render slow.
fn spawn_with(
    rx: Receiver<Request>,
    tx: Sender<Reply>,
    generations: Arc<[AtomicU64; 2]>,
    stats: Arc<Stats>,
    wake: impl Fn() + Send + Sync + 'static,
    render: RenderFn,
) {
    let pool = Arc::new(Pool::default());
    // The feeder moves requests from the UI's channel into the shared queue and wakes one idle
    // worker. Workers that drained the channel themselves would leave the requests they drained
    // waiting behind their own slow render while the other thread slept in `recv`.
    let feed = pool.clone();
    std::thread::Builder::new()
        .name("icons-feed".into())
        .spawn(move || {
            for r in rx.iter() {
                lock(&feed.queue).0.push(r);
                feed.ready.notify_one();
            }
            lock(&feed.queue).1 = true;
            feed.ready.notify_all();
        })
        .expect("spawn icon feeder thread");
    let wake = Arc::new(wake);
    for i in 0..ICON_WORKERS {
        let (pool, tx, generations, stats, wake) = (pool.clone(), tx.clone(), generations.clone(), stats.clone(), wake.clone());
        std::thread::Builder::new()
            .name(format!("icons-{i}"))
            .spawn(move || work(&pool, &tx, &generations, &stats, &*wake, render))
            .expect("spawn icon thread");
    }
}

/// The newest request, and whether the queue is empty after it; None once the channel closed.
fn next(pool: &Pool) -> Option<(Request, bool)> {
    let mut q = lock(&pool.queue);
    loop {
        if let Some(r) = q.0.pop() {
            let idle = q.0.is_empty();
            return Some((r, idle));
        }
        if q.1 {
            return None;
        }
        q = pool.ready.wait(q).unwrap_or_else(PoisonError::into_inner);
    }
}

/// One worker thread: COM (MTA) for itself, then requests until the channel closes.
fn work(pool: &Pool, tx: &Sender<Reply>, generations: &[AtomicU64; 2], stats: &Stats, wake: &dyn Fn(), render: RenderFn) {
    // SAFETY: initialises COM for this thread only; balanced by CoUninitialize below.
    let com = unsafe { CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32) };
    // Newest first: the latest requests are the rows on screen now, while older ones may be
    // rows a fast scroll passed over (still rendered, later, for the cache).
    while let Some((r, idle)) = next(pool) {
        let stale = generations[r.pane].load(Ordering::Relaxed) != r.generation;
        if stale || !lock(&pool.taken).insert(r.key.clone()) {
            if tx.send(Reply { key: r.key, outcome: Outcome::Skipped }).is_err() {
                break;
            }
            if idle {
                wake(); // otherwise the next rendered reply wakes the UI for both
            }
            continue;
        }
        let t0 = Instant::now();
        let mut pixels = if com >= 0 { render(&r.path, r.size_px) } else { None };
        if r.overlay {
            if let (Some(p), Some(over)) = (&pixels, overlay_image(pool, &r.path, r.size_px)) {
                pixels = Some(overlay(&fit(p, r.size_px), &over));
                stats.overlays.fetch_add(1, Ordering::Relaxed);
            }
        }
        let took = t0.elapsed();
        stats.rendered.fetch_add(1, Ordering::Relaxed);
        stats.render_ns.fetch_add(took.as_nanos() as u64, Ordering::Relaxed);
        if took.as_millis() as u64 >= SLOW_MS {
            stats.slow.fetch_add(1, Ordering::Relaxed);
            eprintln!("[icons] slow icon {} ms: {}", took.as_millis(), r.path.display());
        }
        let outcome = match pixels {
            Some(p) => Outcome::Pixels(p),
            None => {
                lock(&pool.taken).remove(&r.key);
                Outcome::Failed
            }
        };
        if tx.send(Reply { key: r.key, outcome }).is_err() {
            break;
        }
        wake();
    }
    if com >= 0 {
        // SAFETY: pairs the successful CoInitializeEx above, on the same thread.
        unsafe { CoUninitialize() };
    }
}

/// IImageList (commoncontrols.h), every slot in declaration order; only the ones called are
/// typed. GetIcon is slot 10 and GetOverlayImage slot 31, counting IUnknown's three.
#[repr(C)]
struct ImageListVtbl {
    _query_interface: usize,
    _add_ref: usize,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    _add: usize,
    _replace_icon: usize,
    _set_overlay_image: usize,
    _replace: usize,
    _add_masked: usize,
    _draw: usize,
    _remove: usize,
    get_icon: unsafe extern "system" fn(*mut c_void, i32, u32, *mut HICON) -> HRESULT,
    _get_image_info: usize,
    _copy: usize,
    _merge: usize,
    _clone: usize,
    _get_image_rect: usize,
    get_icon_size: unsafe extern "system" fn(*mut c_void, *mut i32, *mut i32) -> HRESULT,
    _set_icon_size: usize,
    _get_image_count: usize,
    _set_image_count: usize,
    _set_bk_color: usize,
    _get_bk_color: usize,
    _begin_drag: usize,
    _end_drag: usize,
    _drag_enter: usize,
    _drag_leave: usize,
    _drag_move: usize,
    _set_drag_cursor_image: usize,
    _drag_show_nolock: usize,
    _get_drag_image: usize,
    _get_item_flags: usize,
    get_overlay_image: unsafe extern "system" fn(*mut c_void, i32, *mut i32) -> HRESULT,
}

const IID_IIMAGELIST: GUID = GUID::from_u128(0x46eb5926_582e_4017_9fdf_e8998daa0950);
/// ILD_TRANSPARENT (commctrl.h), declared here rather than enabling windows-sys's Win32_UI_Controls.
const ILD_TRANSPARENT: u32 = 1;

/// The shell's overlay for `path` drawn at `size_px`, or None when it has none. The overlay's
/// index costs one `SHGetFileInfoW` per call (the shell looks at the file); its image is fetched
/// once per (index, size) per process and shared by the worker threads.
fn overlay_image(pool: &Pool, path: &Path, size_px: u32) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let index = overlay_index(path)?;
    let mut cache = lock(&pool.overlays);
    if let Some(o) = cache.get(&(index, size_px)) {
        return o.clone();
    }
    let o = overlay_pixels(index, size_px);
    eprintln!("[icons] overlay index {index} fetched at {size_px} px{}", if o.is_some() { "" } else { ": no image" });
    cache.insert((index, size_px), o.clone());
    o
}

/// The overlay index the shell gives `path` (the top eight bits of `iIcon` with
/// SHGFI_OVERLAYINDEX), or None for no overlay. The icon handle that comes with it is destroyed.
fn overlay_index(path: &Path) -> Option<i32> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: a zeroed SHFILEINFOW is valid; `wide` is NUL-terminated; the HICON, when one is
    // returned, is ours to destroy.
    let (ok, shfi) = unsafe {
        let mut shfi: SHFILEINFOW = std::mem::zeroed();
        let flags = SHGFI_ICON | SHGFI_SMALLICON | SHGFI_ADDOVERLAYS | SHGFI_OVERLAYINDEX;
        let ok = SHGetFileInfoW(wide.as_ptr(), 0, &mut shfi, std::mem::size_of::<SHFILEINFOW>() as u32, flags);
        if !shfi.hIcon.is_null() {
            DestroyIcon(shfi.hIcon);
        }
        (ok, shfi)
    };
    let index = (shfi.iIcon >> 24) & 0xFF;
    (ok != 0 && index != 0).then_some(index)
}

/// Overlay `index`'s image from the system image list, drawn at `size_px` square: the small
/// list when its icons are at least that big, else the large one (`DrawIconEx` scales).
fn overlay_pixels(index: i32, size_px: u32) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let list_of = |which: u32| {
        let mut list: *mut c_void = std::ptr::null_mut();
        // SAFETY: `list` receives an IImageList pointer or stays null.
        let hr = unsafe { SHGetImageList(which as i32, &IID_IIMAGELIST, &mut list) };
        (hr >= 0 && !list.is_null()).then_some(list)
    };
    // SAFETY: `list` is a live IImageList from SHGetImageList, whose vtable is `ImageListVtbl`.
    let vt = |list: *mut c_void| unsafe { &**(list as *const *const ImageListVtbl) };
    let mut list = list_of(SHIL_SMALL)?;
    let (mut cx, mut cy) = (0, 0);
    // SAFETY: a live list; `cx` and `cy` are writable.
    if unsafe { (vt(list).get_icon_size)(list, &mut cx, &mut cy) } >= 0 && (cx as u32) < size_px {
        if let Some(large) = list_of(SHIL_LARGE) {
            // SAFETY: the small list is released once and replaced by the large one.
            unsafe { (vt(list).release)(list) };
            list = large;
        }
    }
    let mut image = -1;
    let mut hicon: HICON = std::ptr::null_mut();
    // SAFETY: a live list; `image` and `hicon` are writable; the icon, if made, is ours.
    let ok = unsafe {
        (vt(list).get_overlay_image)(list, index, &mut image) >= 0
            && image >= 0
            && (vt(list).get_icon)(list, image, ILD_TRANSPARENT, &mut hicon) >= 0
    };
    // SAFETY: released exactly once; nothing uses it after this.
    unsafe { (vt(list).release)(list) };
    if !ok || hicon.is_null() {
        return None;
    }
    let pixels = icon_pixels(hicon, size_px);
    // SAFETY: GetIcon made this icon for us.
    unsafe { DestroyIcon(hicon) };
    pixels
}

/// An icon drawn at `px` square into a transparent 32-bit DIB: `DrawIconEx` alpha-blends an
/// alpha icon, which leaves premultiplied BGRA. None when nothing visible was drawn.
fn icon_pixels(hicon: HICON, px: u32) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    // SAFETY: zeroed BITMAPINFO is a valid starting point; the header fields are set below.
    let mut bmi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: px as i32,
        biHeight: -(px as i32), // negative: top-down rows
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..bmi.bmiHeader
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    // SAFETY: a memory DC and a DIB section of px * px 32-bit pixels, both freed before
    // returning; `bits` covers px * px * 4 bytes while the section lives.
    let bgra = unsafe {
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        let bmp = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        let mut out = None;
        if !bmp.is_null() && !bits.is_null() {
            let old = SelectObject(dc, bmp);
            if DrawIconEx(dc, 0, 0, hicon, px as i32, px as i32, 0, std::ptr::null_mut(), DI_NORMAL) != 0 {
                GdiFlush();
                out = Some(std::slice::from_raw_parts(bits as *const u8, (px * px * 4) as usize).to_vec());
            }
            SelectObject(dc, old);
        }
        if !bmp.is_null() {
            DeleteObject(bmp);
        }
        DeleteDC(dc);
        out
    }?;
    // All alpha zero would read as an opaque square in `bgra_to_rgba`: no overlay instead.
    if bgra.chunks_exact(4).all(|p| p[3] == 0) {
        return None;
    }
    Some(bgra_to_rgba(&bgra, px, px))
}

/// What the table knows about a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyState {
    Ready(u16),
    Pending,
    Failed,
    Missing,
}

/// UI-thread icon table shared by both panes: pixel buffers by index (the first
/// `PLACEHOLDERS` are the placeholders), the key of every interned buffer, and the keys in
/// flight or failed, so each key is requested once however many rows show it. `fitted` caches
/// each buffer resampled to the strip's icon size, for the size last asked for.
pub struct IconTable {
    bufs: Vec<SharedPixelBuffer<Rgba8Pixel>>,
    fitted: Vec<Option<SharedPixelBuffer<Rgba8Pixel>>>,
    fitted_px: u32,
    index: HashMap<SharedString, u16>,
    pending: HashSet<SharedString>,
    failed: HashSet<SharedString>,
}

impl Default for IconTable {
    fn default() -> Self {
        IconTable {
            bufs: vec![placeholder_file(), placeholder_folder(), placeholder_drive()],
            fitted: Vec::new(),
            fitted_px: 0,
            index: HashMap::new(),
            pending: HashSet::new(),
            failed: HashSet::new(),
        }
    }
}

impl IconTable {
    /// The buffer at `i` as delivered (an out-of-range index reads as the file placeholder): the
    /// tests, and the drag image when the shell has no large icon for the item.
    pub fn buffer(&self, i: u16) -> &SharedPixelBuffer<Rgba8Pixel> {
        self.bufs.get(i as usize).unwrap_or(&self.bufs[PLACEHOLDER_FILE as usize])
    }

    /// The buffer at `i` resampled to `px` square, computed once per index and size (a
    /// refcount bump after that). A new size drops the cache. An out-of-range index reads as
    /// the file placeholder.
    pub fn fitted(&mut self, i: u16, px: u32) -> SharedPixelBuffer<Rgba8Pixel> {
        if self.fitted_px != px {
            self.fitted.clear();
            self.fitted_px = px;
        }
        let i = if (i as usize) < self.bufs.len() { i as usize } else { PLACEHOLDER_FILE as usize };
        if self.fitted.len() <= i {
            self.fitted.resize(self.bufs.len(), None);
        }
        self.fitted[i].get_or_insert_with(|| fit(&self.bufs[i], px)).clone()
    }

    /// Buffers held, placeholders included.
    pub fn len(&self) -> usize {
        self.bufs.len()
    }

    pub fn state(&self, key: &SharedString) -> KeyState {
        if let Some(&i) = self.index.get(key) {
            KeyState::Ready(i)
        } else if self.pending.contains(key) {
            KeyState::Pending
        } else if self.failed.contains(key) {
            KeyState::Failed
        } else {
            KeyState::Missing
        }
    }

    /// Mark a key as requested.
    pub fn begin(&mut self, key: &SharedString) {
        self.pending.insert(key.clone());
    }

    /// Apply a worker reply. Returns true when a new image was interned.
    pub fn complete(&mut self, key: SharedString, outcome: Outcome) -> bool {
        self.pending.remove(&key);
        match outcome {
            Outcome::Pixels(p) => {
                if self.index.contains_key(&key) || self.bufs.len() >= u16::MAX as usize {
                    return false;
                }
                self.index.insert(key, self.bufs.len() as u16);
                self.bufs.push(p);
                true
            }
            Outcome::Failed => {
                self.failed.insert(key);
                false
            }
            Outcome::Skipped => false,
        }
    }
}

/// A 16 x 16 opaque-on-transparent placeholder: `rects` are (x0, y0, x1, y1, rgb) filled in order.
fn placeholder(rects: &[(u32, u32, u32, u32, [u8; 3])]) -> SharedPixelBuffer<Rgba8Pixel> {
    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(16, 16);
    let px = buf.make_mut_slice();
    for &(x0, y0, x1, y1, [r, g, b]) in rects {
        for y in y0..y1 {
            for x in x0..x1 {
                px[(y * 16 + x) as usize] = Rgba8Pixel { r, g, b, a: 255 };
            }
        }
    }
    buf
}

fn placeholder_file() -> SharedPixelBuffer<Rgba8Pixel> {
    placeholder(&[(3, 1, 13, 15, [150, 150, 150]), (4, 2, 12, 14, [245, 245, 245])])
}

fn placeholder_folder() -> SharedPixelBuffer<Rgba8Pixel> {
    placeholder(&[(1, 3, 7, 5, [214, 170, 60]), (1, 5, 15, 14, [232, 190, 80])])
}

fn placeholder_drive() -> SharedPixelBuffer<Rgba8Pixel> {
    placeholder(&[(1, 5, 15, 12, [120, 120, 120]), (2, 6, 14, 11, [200, 200, 200]), (11, 8, 13, 10, [60, 160, 60])])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    use std::time::Duration;

    #[test]
    fn key_rules() {
        let f = Path::new(r"C:\some\dir");
        let (file, dir) = (Kind::File, Kind::Directory);
        assert_eq!(icon_key(f, "a.TXT", 0, file, false), "ext:.txt");
        assert_eq!(icon_key(f, "archive.tar.gz", 0, file, false), "ext:.gz");
        assert_eq!(icon_key(f, ".gitignore", 0, file, false), "ext:.gitignore");
        for own in ["setup.exe", "Link.LNK", "app.ico", "site.url", "arrow.cur", "README", "trailing."] {
            assert_eq!(icon_key(f, own, 0, file, false), format!(r"C:\some\dir\{own}"), "{own}");
        }
        assert_eq!(icon_key(f, "sub", FILE_ATTRIBUTE_DIRECTORY, dir, false), "dir");
        assert_eq!(icon_key(f, "Docs", FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_READONLY, dir, true), r"C:\some\dir\Docs");
        let rdir = FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT;
        let rfile = FILE_ATTRIBUTE_REPARSE_POINT;
        assert_eq!(icon_key(f, "j", rdir, Kind::Junction, false), r"C:\some\dir\j");
        assert_eq!(icon_key(f, "l", rdir, Kind::Symlink, false), r"C:\some\dir\l");
        assert_eq!(icon_key(f, "l.txt", rfile, Kind::Symlink, false), r"C:\some\dir\l.txt");
        assert_eq!(icon_key(f, "x.txt", rfile, Kind::Reparse, false), r"C:\some\dir\x.txt");
        assert_eq!(icon_key(f, "wt.exe", rfile, Kind::AppAlias, false), r"C:\some\dir\wt.exe");
        // Cloud placeholders share their type's key, or `dir`, unless the icon is their own.
        assert_eq!(icon_key(f, "cloud.TXT", rfile, Kind::Cloud, false), "ext:.txt");
        assert_eq!(icon_key(f, "cloud.lnk", rfile, Kind::Cloud, false), r"C:\some\dir\cloud.lnk");
        assert_eq!(icon_key(f, "csub", rdir, Kind::Cloud, false), "dir");
        assert_eq!(icon_key(f, "Pictures", rdir | FILE_ATTRIBUTE_READONLY, Kind::Cloud, true), r"C:\some\dir\Pictures");
        assert_eq!(icon_key(Path::new(r"C:\"), "x.exe", 0, file, false), r"C:\x.exe");

        // The cached builder agrees with the pure rules and shares extension keys.
        let mut keys = IconKeys::new(f.to_path_buf());
        let a = keys.for_entry("a.txt", 0, file);
        let b = keys.for_entry("b.txt", 0, file);
        assert_eq!((a.as_str(), b.as_str()), ("ext:.txt", "ext:.txt"));
        assert_eq!(keys.for_entry("B.TXT", 0, file), "ext:.txt");
        assert_eq!(keys.for_entry("sub", FILE_ATTRIBUTE_DIRECTORY, dir), "dir");
        assert_eq!(keys.for_entry("x.exe", 0, file), r"C:\some\dir\x.exe");
        // A read-only folder with no desktop.ini (this path does not exist) stays `dir`.
        assert_eq!(keys.for_entry("ro", FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_READONLY, dir), "dir");
        assert_eq!(keys.for_entry("cloud.txt", rfile, Kind::Cloud), "ext:.txt");
        assert_eq!(keys.for_entry("csub", rdir | FILE_ATTRIBUTE_READONLY, Kind::Cloud), "dir");
        assert_eq!(keys.for_entry("j", rdir, Kind::Junction), r"C:\some\dir\j");
        assert_eq!(keys.for_entry("l.txt", rfile, Kind::Symlink), r"C:\some\dir\l.txt");
    }

    #[test]
    fn custom_folder_needs_desktop_ini_and_ro_or_system() {
        let dir = std::env::temp_dir().join(format!("twinfm-icon-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("custom")).unwrap();
        std::fs::create_dir_all(dir.join("plain")).unwrap();
        std::fs::write(dir.join("custom").join("desktop.ini"), b"[.ShellClassInfo]\r\n").unwrap();
        let ro = FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_READONLY;
        assert!(custom_folder(&dir, "custom", ro));
        assert!(!custom_folder(&dir, "custom", FILE_ATTRIBUTE_DIRECTORY), "the shell ignores desktop.ini without RO/system");
        assert!(!custom_folder(&dir, "plain", ro));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn bgra_conversion_keeps_premultiplied_alpha() {
        // 2 x 2: opaque blue, half-transparent premultiplied red, transparent, opaque white.
        let bgra = [255, 0, 0, 255, 0, 0, 128, 128, 0, 0, 0, 0, 255, 255, 255, 255];
        let buf = bgra_to_rgba(&bgra, 2, 2);
        let px = buf.as_slice();
        assert_eq!((buf.width(), buf.height()), (2, 2));
        assert_eq!(px[0], Rgba8Pixel { r: 0, g: 0, b: 255, a: 255 });
        assert_eq!(px[1], Rgba8Pixel { r: 128, g: 0, b: 0, a: 128 });
        assert_eq!(px[2], Rgba8Pixel { r: 0, g: 0, b: 0, a: 0 });
        assert_eq!(px[3], Rgba8Pixel { r: 255, g: 255, b: 255, a: 255 });
        // No alpha channel at all (every alpha zero): treated as opaque.
        let flat = bgra_to_rgba(&[10, 20, 30, 0, 40, 50, 60, 0, 0, 0, 0, 0, 1, 2, 3, 0], 2, 2);
        assert_eq!(flat.as_slice()[0], Rgba8Pixel { r: 30, g: 20, b: 10, a: 255 });
        assert!(flat.as_slice().iter().all(|p| p.a == 255));
    }

    #[test]
    fn visible_range_arithmetic() {
        assert_eq!(visible_range(0.0, 220.0, 0), None);
        assert_eq!(visible_range(0.0, 220.0, 1000), Some((0, 7)));
        assert_eq!(visible_range(-32.0, 220.0, 1000), Some((1, 8)));
        assert_eq!(visible_range(-40.0, 100.0, 1000), Some((1, 5))); // 40..140 px: rows 1..=4, ceil to 5
        assert_eq!(visible_range(0.0, 220.0, 5), Some((0, 4)), "clamped to the last row");
        assert_eq!(visible_range(-100_000.0, 220.0, 5), Some((4, 4)), "scrolled past the end");
        assert_eq!(visible_range(10.0, 44.0, 100), Some((0, 2)), "overscroll at the top");
    }

    fn px() -> Outcome {
        Outcome::Pixels(SharedPixelBuffer::new(2, 2))
    }

    #[test]
    fn table_interns_once_and_tracks_pending_and_failed() {
        let mut t = IconTable::default();
        assert_eq!(t.len(), PLACEHOLDERS as usize);
        let txt: SharedString = "ext:.txt".into();
        assert_eq!(t.state(&txt), KeyState::Missing);
        t.begin(&txt);
        assert_eq!(t.state(&txt), KeyState::Pending);
        assert!(t.complete(txt.clone(), px()));
        assert_eq!(t.state(&txt), KeyState::Ready(PLACEHOLDERS));
        assert!(!t.complete(txt.clone(), px()), "a duplicate reply does not intern twice");
        assert_eq!(t.len(), PLACEHOLDERS as usize + 1);
        let gone: SharedString = r"C:\gone.exe".into();
        t.begin(&gone);
        assert!(!t.complete(gone.clone(), Outcome::Failed));
        assert_eq!(t.state(&gone), KeyState::Failed);
        let skip: SharedString = "dir".into();
        t.begin(&skip);
        t.complete(skip.clone(), Outcome::Skipped);
        assert_eq!(t.state(&skip), KeyState::Missing, "a skipped key may be requested again");
        assert_eq!(t.buffer(PLACEHOLDERS).width(), 2);
        assert_eq!(t.buffer(PLACEHOLDER_FOLDER).width(), 16);
        // Fitted to the strip's icon size, once per size.
        assert_eq!(t.fitted(PLACEHOLDERS, 4).width(), 4);
        assert_eq!(t.fitted(PLACEHOLDER_FOLDER, 24).height(), 24);
        assert_eq!(t.fitted(PLACEHOLDER_FOLDER, 16).as_slice(), t.buffer(PLACEHOLDER_FOLDER).as_slice());
        assert_eq!(t.fitted(999, 16).as_slice(), t.buffer(PLACEHOLDER_FILE).as_slice(), "out of range reads as the file placeholder");
    }

    #[test]
    fn strip_layout_arithmetic() {
        let g1 = StripGeom::new(1.0);
        assert_eq!(g1.icon_px, 32);
        assert_eq!((g1.icon_y(0), g1.icon_y(1), g1.icon_y(10)), (0, 32, 320), "row top plus (32 - 32) / 2");
        assert_eq!((g1.height(0), g1.height(1), g1.height(50)), (0, 32, 1600));
        let g15 = StripGeom::new(1.5);
        assert_eq!(g15.icon_px, 48);
        assert_eq!((g15.icon_y(0), g15.icon_y(1), g15.icon_y(3)), (0, 48, 144));
        assert_eq!(g15.height(3), 144);
        let g125 = StripGeom::new(1.25);
        assert_eq!(g125.icon_px, 40);
        assert_eq!(g125.height(3), 120);
        // Every icon lies inside its own row band at any of these scales.
        for g in [g1, g15, g125, StripGeom::new(1.75), StripGeom::new(2.0)] {
            for k in 0..200 {
                assert!(g.icon_y(k) >= g.height(k), "scale {} row {k}", g.scale);
                assert!(g.icon_y(k) + g.icon_px <= g.height(k + 1), "scale {} row {k}", g.scale);
            }
        }
        // Range: the visible rows plus the margin, clamped to the list.
        let m = STRIP_MARGIN_ROWS;
        assert_eq!(strip_range((10, 40), 1000), (10 - m, 40 + m));
        assert_eq!(strip_range((0, 30), 1000), (0, 30 + m));
        assert_eq!(strip_range((0, 4), 5), (0, 4), "list shorter than the viewport");
        assert_eq!(strip_range((990, 999), 1000), (990 - m, 999), "range at the end");
    }

    #[test]
    fn recompose_decision() {
        let c = Some(Composed { first: 6, last: 44, icon_px: 16 });
        assert!(needs_recompose(None, (0, 30), 100, 16, false), "nothing composed yet");
        assert!(!needs_recompose(c, (10, 40), 100, 16, false), "same range, no new key");
        assert!(!needs_recompose(c, (6, 36), 100, 16, false), "scrolled inside the margin");
        assert!(!needs_recompose(c, (14, 44), 100, 16, false), "scrolled inside the margin");
        assert!(needs_recompose(c, (5, 35), 100, 16, false), "one row above the strip");
        assert!(needs_recompose(c, (15, 45), 100, 16, false), "one row below the strip");
        assert!(needs_recompose(c, (10, 40), 100, 16, true), "an icon landed or the rows changed");
        assert!(needs_recompose(c, (10, 40), 100, 24, false), "scale changed");
        assert!(needs_recompose(c, (10, 40), 44, 16, false), "list shrank under the strip");
        assert!(needs_recompose(c, (10, 50), 100, 16, false), "viewport grew");
        assert!(!needs_recompose(c, (10, 20), 100, 16, false), "viewport shrank");
    }

    #[test]
    fn compose_places_icons_at_row_offsets() {
        let solid = |v: u8| {
            let mut b = SharedPixelBuffer::<Rgba8Pixel>::new(2, 2);
            for (i, p) in b.make_mut_slice().iter_mut().enumerate() {
                *p = Rgba8Pixel { r: v, g: i as u8, b: 0, a: 255 };
            }
            b
        };
        let mut t = IconTable::default();
        t.complete("a".into(), Outcome::Pixels(solid(10)));
        t.complete("b".into(), Outcome::Pixels(solid(20)));
        // Three rows: key a, a row with no icon yet (the file placeholder), key b.
        let g = StripGeom { icon_px: 2, scale: 1.0 };
        let icons: Vec<_> = [PLACEHOLDERS, PLACEHOLDER_FILE, PLACEHOLDERS + 1].iter().map(|&i| t.fitted(i, 2)).collect();
        let strip = compose(&icons, &g);
        assert_eq!((strip.width(), strip.height()), (2, 96));
        let px = strip.as_slice();
        let at = |x: usize, y: usize| px[y * 2 + x];
        // Row 0 at y 0..2 and row 2 at y 64..66, scanlines in order.
        assert_eq!(at(0, 0), Rgba8Pixel { r: 10, g: 0, b: 0, a: 255 });
        assert_eq!(at(1, 0), Rgba8Pixel { r: 10, g: 1, b: 0, a: 255 });
        assert_eq!(at(0, 1), Rgba8Pixel { r: 10, g: 2, b: 0, a: 255 });
        assert_eq!(at(1, 1), Rgba8Pixel { r: 10, g: 3, b: 0, a: 255 });
        assert_eq!(at(0, 64), Rgba8Pixel { r: 20, g: 0, b: 0, a: 255 });
        assert_eq!(at(1, 65), Rgba8Pixel { r: 20, g: 3, b: 0, a: 255 });
        // Row 1 at y 32..34 holds the file placeholder fitted to 2 x 2.
        assert_eq!(&px[32 * 2..34 * 2], t.fitted(PLACEHOLDER_FILE, 2).as_slice());
        assert!(at(0, 32).a > 0, "the placeholder is visible");
        // Everything else is transparent.
        let filled: Vec<usize> = (0..96).filter(|&y| at(0, y).a != 0 || at(1, y).a != 0).collect();
        assert_eq!(filled, [0, 1, 32, 33, 64, 65]);
        // An empty strip, and an icon of the wrong size left out.
        assert_eq!(compose(&[], &g).height(), 0);
        assert!(compose(&[solid(1)], &StripGeom { icon_px: 4, scale: 1.0 }).as_slice().iter().all(|p| p.a == 0));
    }

    #[test]
    fn fit_box_filters_premultiplied() {
        // 4 x 4 of two colours in 2 x 2 blocks, down to 2 x 2: one pixel per block.
        let red = Rgba8Pixel { r: 200, g: 0, b: 0, a: 255 };
        let blue = Rgba8Pixel { r: 0, g: 0, b: 100, a: 100 };
        let mut b = SharedPixelBuffer::<Rgba8Pixel>::new(4, 4);
        for (i, p) in b.make_mut_slice().iter_mut().enumerate() {
            *p = if (i % 4 < 2) == (i / 4 < 2) { red } else { blue };
        }
        let f = fit(&b, 2);
        assert_eq!(f.as_slice(), [red, blue, blue, red]);
        // Down to 1 x 1: the average of all sixteen.
        assert_eq!(fit(&b, 1).as_slice()[0], Rgba8Pixel { r: 100, g: 0, b: 50, a: 177 });
        // Up 2 to 3: every target pixel takes at least one source pixel.
        let up = fit(&f, 3);
        assert_eq!(up.as_slice().len(), 9);
        assert!(up.as_slice().iter().all(|p| p.a != 0));
    }

    #[test]
    fn overlay_blends_source_over_premultiplied() {
        let buf = |px: [Rgba8Pixel; 4]| {
            let mut b = SharedPixelBuffer::<Rgba8Pixel>::new(2, 2);
            b.make_mut_slice().copy_from_slice(&px);
            b
        };
        let p = |r, g, b, a| Rgba8Pixel { r, g, b, a };
        let clear = p(0, 0, 0, 0);
        // Base: opaque red, opaque red, half-transparent green (premultiplied), transparent.
        let base = buf([p(255, 0, 0, 255), p(255, 0, 0, 255), p(0, 128, 0, 128), clear]);
        // Overlay: nothing, opaque white, half-transparent blue (premultiplied), opaque black.
        let over = buf([clear, p(255, 255, 255, 255), p(0, 0, 128, 128), p(0, 0, 0, 255)]);
        let out = overlay(&base, &over);
        assert_eq!(out.as_slice()[0], p(255, 0, 0, 255), "a transparent overlay pixel keeps the base");
        assert_eq!(out.as_slice()[1], p(255, 255, 255, 255), "an opaque overlay pixel replaces it");
        // 128 + 128 * 127 / 255 = 128 + 64 (rounded): half blue over half green.
        assert_eq!(out.as_slice()[2], p(0, 64, 128, 192));
        assert_eq!(out.as_slice()[3], p(0, 0, 0, 255), "over a transparent base the overlay shows as is");
        assert_eq!(base.as_slice()[1], p(255, 0, 0, 255), "the base buffer is not changed");
        // Different sizes: the base comes back unchanged.
        assert_eq!(overlay(&base, &SharedPixelBuffer::new(4, 4)).as_slice(), base.as_slice());
    }

    #[test]
    fn overlay_only_for_links_and_reparse_points() {
        let rdir = FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT;
        assert!(wants_overlay(Kind::File, 0, "Notes.LNK"));
        assert!(wants_overlay(Kind::File, 0, "site.url"));
        assert!(wants_overlay(Kind::Cloud, FILE_ATTRIBUTE_REPARSE_POINT, "synced.lnk"));
        assert!(wants_overlay(Kind::Junction, rdir, "Application Data"));
        assert!(wants_overlay(Kind::Symlink, FILE_ATTRIBUTE_REPARSE_POINT, "link.txt"));
        assert!(wants_overlay(Kind::AppAlias, FILE_ATTRIBUTE_REPARSE_POINT, "wt.exe"));
        assert!(wants_overlay(Kind::Reparse, FILE_ATTRIBUTE_REPARSE_POINT, "x.dat"));
        assert!(!wants_overlay(Kind::File, 0, "a.txt"));
        assert!(!wants_overlay(Kind::File, 0, "setup.exe"));
        assert!(!wants_overlay(Kind::Directory, FILE_ATTRIBUTE_DIRECTORY, "odd.lnk"), "keyed `dir`, shared");
        assert!(!wants_overlay(Kind::Cloud, rdir, "csub"));
        assert!(!wants_overlay(Kind::Cloud, FILE_ATTRIBUTE_REPARSE_POINT, "doc.docx"));
        assert!(!wants_overlay(Kind::Drive, FILE_ATTRIBUTE_DIRECTORY, r"C:\"));
    }

    fn req(generation: u64, key: &str, path: &str) -> Request {
        Request { generation, pane: 0, key: key.into(), path: PathBuf::from(path), size_px: 1, overlay: false }
    }

    /// A stand-in render: "slow" takes 2 s, "busy" 50 ms (counted in BUSY_RENDERS), others none.
    fn fake_render(path: &Path, _size_px: u32) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
        match path.to_str() {
            Some("slow") => std::thread::sleep(Duration::from_secs(2)),
            Some("busy") => {
                BUSY_RENDERS.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(50));
            }
            Some("fail") => return None,
            _ => {}
        }
        Some(SharedPixelBuffer::new(1, 1))
    }

    static BUSY_RENDERS: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn pool_serves_others_while_one_icon_is_slow() {
        let (req_tx, req_rx) = unbounded();
        let (rep_tx, rep_rx) = unbounded();
        let gens = Arc::new([AtomicU64::new(1), AtomicU64::new(0)]);
        let stats = Arc::new(Stats::default());
        spawn_with(req_rx, rep_tx, gens.clone(), stats.clone(), || {}, fake_render);

        // One slow render takes a thread; three requests sent after it go to the other one.
        req_tx.send(req(1, "slow", "slow")).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let t0 = Instant::now();
        for k in ["a", "b", "c"] {
            req_tx.send(req(1, k, k)).unwrap();
        }
        let mut fast = HashSet::new();
        while fast.len() < 3 {
            let r = rep_rx.recv_timeout(Duration::from_millis(500)).expect("the others reply within 500 ms");
            assert!(matches!(r.outcome, Outcome::Pixels(_)), "{} rendered", r.key);
            fast.insert(r.key.to_string());
        }
        assert!(t0.elapsed() < Duration::from_millis(500), "{:?}", t0.elapsed());
        assert_eq!(fast, HashSet::from(["a".to_string(), "b".to_string(), "c".to_string()]));

        // A stale generation is skipped, not rendered, even while a thread is busy.
        req_tx.send(req(9, "stale", "x")).unwrap();
        let r = rep_rx.recv_timeout(Duration::from_millis(500)).expect("stale reply");
        assert_eq!(r.key, "stale");
        assert!(matches!(r.outcome, Outcome::Skipped));

        // A failed key is given back, so a later request for it renders (and fails) again.
        req_tx.send(req(1, "f", "fail")).unwrap();
        assert!(matches!(rep_rx.recv_timeout(Duration::from_millis(500)).unwrap().outcome, Outcome::Failed));
        req_tx.send(req(1, "f", "fail")).unwrap();
        assert!(matches!(rep_rx.recv_timeout(Duration::from_millis(500)).unwrap().outcome, Outcome::Failed));

        let r = rep_rx.recv_timeout(Duration::from_secs(3)).expect("the slow icon arrives");
        assert_eq!(r.key, "slow");
        assert_eq!(stats.slow.load(Ordering::Relaxed), 1, "only the 2 s render is slow");
    }

    #[test]
    fn pool_renders_each_key_once() {
        let (req_tx, req_rx) = unbounded();
        let (rep_tx, rep_rx) = unbounded();
        let gens = Arc::new([AtomicU64::new(1), AtomicU64::new(0)]);
        spawn_with(req_rx, rep_tx, gens, Arc::new(Stats::default()), || {}, fake_render);
        for _ in 0..20 {
            req_tx.send(req(1, "same", "busy")).unwrap();
        }
        let mut pixels = 0;
        for _ in 0..20 {
            match rep_rx.recv_timeout(Duration::from_secs(2)).expect("20 replies").outcome {
                Outcome::Pixels(_) => pixels += 1,
                Outcome::Skipped => {}
                Outcome::Failed => panic!("the fake render does not fail here"),
            }
        }
        assert_eq!(pixels, 1, "one reply carries the icon, the other 19 are skipped");
        assert_eq!(BUSY_RENDERS.load(Ordering::SeqCst), 1, "rendered once across the pool");
        // Delivered keys stay taken.
        req_tx.send(req(1, "same", "busy")).unwrap();
        assert!(matches!(rep_rx.recv_timeout(Duration::from_secs(2)).unwrap().outcome, Outcome::Skipped));
        drop(req_tx);
        assert!(rep_rx.recv_timeout(Duration::from_secs(2)).is_err(), "the workers exit when the channel closes");
    }

    #[test]
    fn worker_renders_exe_drive_and_extension() {
        let (req_tx, req_rx) = unbounded();
        let (rep_tx, rep_rx) = unbounded();
        let gens = Arc::new([AtomicU64::new(1), AtomicU64::new(0)]);
        spawn(req_rx, rep_tx, gens.clone(), Arc::new(Stats::default()), || {});
        let txt = std::env::temp_dir().join(format!("twinfm-icon-{}.txt", std::process::id()));
        std::fs::write(&txt, b"x").unwrap();
        // A junction, asked for twice: plain and with the shell's overlay (the link arrow).
        let dir = std::env::temp_dir().join(format!("twinfm-p2m5-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("target")).unwrap();
        let junction = dir.join("junc");
        let made = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&junction)
            .arg(dir.join("target"))
            .output()
            .is_ok_and(|o| o.status.success());
        assert!(made, "mklink /J failed");
        let jobs = [
            (r"C:\Windows\notepad.exe".to_string(), PathBuf::from(r"C:\Windows\notepad.exe")),
            (r"C:\".to_string(), PathBuf::from(r"C:\")),
            ("ext:.txt".to_string(), txt.clone()),
            ("junction".to_string(), junction.clone()),
        ];
        for (key, path) in &jobs {
            req_tx.send(Request { generation: 1, pane: 0, key: key.as_str().into(), path: path.clone(), size_px: 16, overlay: false }).unwrap();
        }
        let key = "junction+overlay";
        req_tx.send(Request { generation: 1, pane: 0, key: key.into(), path: junction.clone(), size_px: 16, overlay: true }).unwrap();
        // A stale request (pane 1 is at generation 0) is skipped, not rendered.
        req_tx.send(Request { generation: 5, pane: 1, key: "stale".into(), path: txt.clone(), size_px: 16, overlay: false }).unwrap();
        let mut got = HashMap::new();
        while got.len() < 6 {
            let r = rep_rx.recv_timeout(Duration::from_secs(5)).expect("reply within 5 s");
            got.insert(r.key.to_string(), r.outcome);
        }
        std::fs::remove_file(&txt).unwrap();
        std::fs::remove_dir_all(&dir).unwrap(); // removes the junction, not its target's contents
        assert!(matches!(got.get("stale"), Some(Outcome::Skipped)));
        if jobs.iter().all(|(k, _)| matches!(got.get(k), Some(Outcome::Failed))) {
            eprintln!("skipping: the shell returned no icons at all (COM unavailable in this environment?)");
            return;
        }
        for (key, _) in &jobs {
            match got.get(key) {
                Some(Outcome::Pixels(p)) => {
                    assert!(p.width() >= 16 && p.height() >= 16, "{key}: {}x{}", p.width(), p.height());
                    assert!(p.as_slice().iter().any(|q| q.a != 0), "{key}: some visible pixels");
                }
                Some(Outcome::Failed) => panic!("{key}: the shell returned no icon"),
                _ => panic!("{key}: no reply"),
            }
        }
        match (got.get("junction"), got.get(key)) {
            (Some(Outcome::Pixels(plain)), Some(Outcome::Pixels(with))) => {
                assert_eq!((with.width(), with.height()), (16, 16), "fitted to the size asked for, then overlaid");
                assert_ne!(fit(plain, 16).as_slice(), with.as_slice(), "the overlay changed the junction's icon");
            }
            _ => panic!("the junction's icons did not both arrive"),
        }
    }
}
