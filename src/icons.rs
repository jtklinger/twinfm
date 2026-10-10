//! Shell icons. One worker thread, initialised for COM (MTA), owns every shell call: it turns a
//! path into a 32-bit HBITMAP through `IShellItemImageFactory::GetImage` and the bitmap into a
//! premultiplied RGBA pixel buffer. The UI thread turns each buffer into a `slint::Image` once
//! per cache key and keeps the images in an `IconTable` that both panes share.
//!
//! Cache keys (`icon_key`): ordinary files share one key per lowercase extension (`ext:.txt`),
//! plain directories share `dir`; files whose icon is their own (`.exe`, `.lnk`, `.ico`, `.url`,
//! `.cur`, no extension), customised folders (a `desktop.ini` that the shell honours), reparse
//! points and drives are keyed by full path.

use crossbeam_channel::{Receiver, Sender};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer, SharedString};
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use windows_sys::Win32::Foundation::SIZE;
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW, HBITMAP,
    ReleaseDC,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SYSTEM,
};
use windows_sys::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows_sys::Win32::UI::Shell::{SHCreateItemFromParsingName, SIIGBF, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY};
use windows_sys::core::{GUID, HRESULT};

/// Icon table slots that exist before any shell icon arrives, one per kind of row.
pub const PLACEHOLDER_FILE: u16 = 0;
pub const PLACEHOLDER_FOLDER: u16 = 1;
pub const PLACEHOLDER_DRIVE: u16 = 2;
pub const PLACEHOLDERS: u16 = 3;

/// The logical size icons are drawn at; the buffer is this times the window scale factor.
pub const ICON_LOGICAL_PX: f32 = 16.0;

/// Row height in the list, in logical pixels (pane.slint).
pub const ROW_PX: f32 = 22.0;

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
}

/// Lowercase extension after the last dot, as Explorer and `TypeNames` read it, or None.
fn extension(name: &str) -> Option<&str> {
    match name.rfind('.') {
        Some(i) if i + 1 < name.len() => Some(&name[i + 1..]),
        _ => None,
    }
}

/// The cache key for a directory entry (drives are keyed by their root, see `Entry::drive`).
/// `custom_folder` says the directory carries a `desktop.ini` the shell honours (`custom_folder`).
pub fn icon_key(folder: &Path, name: &str, attrs: u32, custom_folder: bool) -> String {
    let full = || folder.join(name).to_string_lossy().into_owned();
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

    pub fn for_entry(&mut self, name: &str, attrs: u32) -> SharedString {
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
        let key = icon_key(&self.folder, name, attrs, custom);
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

#[repr(C)]
struct ImageFactoryVtbl {
    _query_interface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    _add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    get_image: unsafe extern "system" fn(*mut c_void, SIZE, SIIGBF, *mut HBITMAP) -> HRESULT,
}

const IID_ISHELLITEMIMAGEFACTORY: GUID = GUID::from_u128(0xbcc18b79_ba16_442f_80c4_8a59c30c463b);

/// The shell's icon for `path` at `size_px` square, or None. Worker thread only (COM).
fn render(path: &Path, size_px: u32) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
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

/// Start the icon worker. It serves its queue newest first, skips requests whose pane has
/// navigated since (`generations`), renders each key at most once per process, and wakes the
/// UI thread after every reply.
pub fn spawn(
    rx: Receiver<Request>,
    tx: Sender<Reply>,
    generations: Arc<[AtomicU64; 2]>,
    stats: Arc<Stats>,
    wake: impl Fn() + Send + 'static,
) {
    std::thread::Builder::new()
        .name("icons".into())
        .spawn(move || {
            // SAFETY: initialises COM for this thread only; balanced by CoUninitialize below.
            let com = unsafe { CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32) };
            let mut delivered: HashSet<SharedString> = HashSet::new();
            // Newest first: the latest requests are the rows on screen now, while older ones
            // may be rows a fast scroll passed over (still rendered, later, for the cache).
            let mut queue: Vec<Request> = Vec::new();
            loop {
                if queue.is_empty() {
                    match rx.recv() {
                        Ok(r) => queue.push(r),
                        Err(_) => break,
                    }
                }
                queue.extend(rx.try_iter());
                let Some(r) = queue.pop() else { continue };
                let stale = generations[r.pane].load(Ordering::Relaxed) != r.generation;
                if stale || delivered.contains(&r.key) {
                    if tx.send(Reply { key: r.key, outcome: Outcome::Skipped }).is_err() {
                        break;
                    }
                    if queue.is_empty() {
                        wake(); // otherwise the next rendered reply wakes the UI for both
                    }
                    continue;
                }
                let t0 = Instant::now();
                let pixels = if com >= 0 { render(&r.path, r.size_px) } else { None };
                stats.rendered.fetch_add(1, Ordering::Relaxed);
                stats.render_ns.fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);
                let outcome = match pixels {
                    Some(p) => {
                        delivered.insert(r.key.clone());
                        Outcome::Pixels(p)
                    }
                    None => Outcome::Failed,
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
        })
        .expect("spawn icon thread");
}

/// What the table knows about a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyState {
    Ready(u16),
    Pending,
    Failed,
    Missing,
}

/// UI-thread icon table shared by both panes: images by index (the first `PLACEHOLDERS` are
/// the placeholders), the key of every interned image, and the keys in flight or failed, so
/// each key is requested once however many rows show it.
pub struct IconTable {
    images: Vec<Image>,
    index: HashMap<SharedString, u16>,
    pending: HashSet<SharedString>,
    failed: HashSet<SharedString>,
}

impl Default for IconTable {
    fn default() -> Self {
        IconTable {
            images: vec![placeholder_file(), placeholder_folder(), placeholder_drive()],
            index: HashMap::new(),
            pending: HashSet::new(),
            failed: HashSet::new(),
        }
    }
}

impl IconTable {
    pub fn image(&self, i: u16) -> Image {
        self.images.get(i as usize).cloned().unwrap_or_default()
    }

    /// Images held, placeholders included.
    pub fn len(&self) -> usize {
        self.images.len()
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
                if self.index.contains_key(&key) || self.images.len() >= u16::MAX as usize {
                    return false;
                }
                self.index.insert(key, self.images.len() as u16);
                self.images.push(Image::from_rgba8_premultiplied(p));
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
fn placeholder(rects: &[(u32, u32, u32, u32, [u8; 3])]) -> Image {
    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(16, 16);
    let px = buf.make_mut_slice();
    for &(x0, y0, x1, y1, [r, g, b]) in rects {
        for y in y0..y1 {
            for x in x0..x1 {
                px[(y * 16 + x) as usize] = Rgba8Pixel { r, g, b, a: 255 };
            }
        }
    }
    Image::from_rgba8_premultiplied(buf)
}

fn placeholder_file() -> Image {
    placeholder(&[(3, 1, 13, 15, [150, 150, 150]), (4, 2, 12, 14, [245, 245, 245])])
}

fn placeholder_folder() -> Image {
    placeholder(&[(1, 3, 7, 5, [214, 170, 60]), (1, 5, 15, 14, [232, 190, 80])])
}

fn placeholder_drive() -> Image {
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
        assert_eq!(icon_key(f, "a.TXT", 0, false), "ext:.txt");
        assert_eq!(icon_key(f, "archive.tar.gz", 0, false), "ext:.gz");
        assert_eq!(icon_key(f, ".gitignore", 0, false), "ext:.gitignore");
        for own in ["setup.exe", "Link.LNK", "app.ico", "site.url", "arrow.cur", "README", "trailing."] {
            assert_eq!(icon_key(f, own, 0, false), format!(r"C:\some\dir\{own}"), "{own}");
        }
        assert_eq!(icon_key(f, "sub", FILE_ATTRIBUTE_DIRECTORY, false), "dir");
        assert_eq!(icon_key(f, "Docs", FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_READONLY, true), r"C:\some\dir\Docs");
        let junction = FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT;
        assert_eq!(icon_key(f, "j", junction, false), r"C:\some\dir\j");
        assert_eq!(icon_key(f, "cloud.txt", FILE_ATTRIBUTE_REPARSE_POINT, false), r"C:\some\dir\cloud.txt");
        assert_eq!(icon_key(Path::new(r"C:\"), "x.exe", 0, false), r"C:\x.exe");

        // The cached builder agrees with the pure rules and shares extension keys.
        let mut keys = IconKeys::new(f.to_path_buf());
        let a = keys.for_entry("a.txt", 0);
        let b = keys.for_entry("b.txt", 0);
        assert_eq!((a.as_str(), b.as_str()), ("ext:.txt", "ext:.txt"));
        assert_eq!(keys.for_entry("B.TXT", 0), "ext:.txt");
        assert_eq!(keys.for_entry("sub", FILE_ATTRIBUTE_DIRECTORY), "dir");
        assert_eq!(keys.for_entry("x.exe", 0), r"C:\some\dir\x.exe");
        // A read-only folder with no desktop.ini (this path does not exist) stays `dir`.
        assert_eq!(keys.for_entry("ro", FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_READONLY), "dir");
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
        assert_eq!(visible_range(0.0, 220.0, 1000), Some((0, 10)));
        assert_eq!(visible_range(-22.0, 220.0, 1000), Some((1, 11)));
        assert_eq!(visible_range(-30.0, 100.0, 1000), Some((1, 6))); // 30..130 px: rows 1..=5, ceil to 6
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
        assert_eq!(t.image(PLACEHOLDERS).size().width, 2);
        assert_eq!(t.image(PLACEHOLDER_FOLDER).size().width, 16);
    }

    #[test]
    fn worker_renders_exe_drive_and_extension() {
        let (req_tx, req_rx) = unbounded();
        let (rep_tx, rep_rx) = unbounded();
        let gens = Arc::new([AtomicU64::new(1), AtomicU64::new(0)]);
        spawn(req_rx, rep_tx, gens.clone(), Arc::new(Stats::default()), || {});
        let txt = std::env::temp_dir().join(format!("twinfm-icon-{}.txt", std::process::id()));
        std::fs::write(&txt, b"x").unwrap();
        let jobs = [
            (r"C:\Windows\notepad.exe".to_string(), PathBuf::from(r"C:\Windows\notepad.exe")),
            (r"C:\".to_string(), PathBuf::from(r"C:\")),
            ("ext:.txt".to_string(), txt.clone()),
        ];
        for (key, path) in &jobs {
            req_tx.send(Request { generation: 1, pane: 0, key: key.as_str().into(), path: path.clone(), size_px: 16 }).unwrap();
        }
        // A stale request (pane 1 is at generation 0) is skipped, not rendered.
        req_tx.send(Request { generation: 5, pane: 1, key: "stale".into(), path: txt.clone(), size_px: 16 }).unwrap();
        let mut got = HashMap::new();
        while got.len() < 4 {
            let r = rep_rx.recv_timeout(Duration::from_secs(5)).expect("reply within 5 s");
            got.insert(r.key.to_string(), r.outcome);
        }
        std::fs::remove_file(&txt).unwrap();
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
    }
}
