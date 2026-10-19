//! Archives listed as folders: zip since P3-M4, the formats of Windows' `ArchiveFolder` (7z, rar,
//! tar, gz and the rest) since P7-M3. A path such as `C:\dir\x.zip\sub` lies inside the archive
//! `C:\dir\x.zip` when that component is a file (a folder named `x.zip` is a folder); `split`
//! decides it, asking the file system only for components whose extension names an archive.
//!
//! A zip's central directory is read once with the `zip` crate (no decompression: the shell
//! extracts) into a map of levels, one per folder inside it. The other formats are listed by the
//! shell's `ArchiveFolder` on the archive worker (`fs::archivefolder`), one level at a time as it is
//! asked for (a tar.gz cannot seek: listing every level up front could decompress it once per
//! folder). Both are kept in a cache of the `CACHE_MAX` most recently used archives, keyed by path,
//! size and modified time, and dropped by `forget` after TwinFM writes one. Workers only: the
//! enumeration worker, the search walker and the crumb dropdown's lister call it; the UI thread
//! never reads an archive.

use crate::fs::enumerate::TypeNames;
use crate::model::{Entry, Kind, LocalZone, NameMatch};
use slint::SharedString;
use std::collections::HashMap;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, GetFileAttributesW, INVALID_FILE_ATTRIBUTES};

/// Archives whose levels stay cached (the most recently used), zip and the others alike.
pub const CACHE_MAX: usize = 8;

/// What lists an archive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handler {
    /// TwinFM's own central directory reader; TwinFM also writes zips (P7-M3).
    Zip,
    /// Windows' `ArchiveFolder`, through the shell: read-only (it refuses every write).
    Shell,
}

/// The extensions `ArchiveFolder` registers on Windows 11 (26H2: P7-M3's check). Only those whose
/// `HKCR\.ext` still names `ArchiveFolder` open as folders, so an archive another program owns
/// (7-Zip's `.7z`) keeps opening in that program.
const SHELL_CANDIDATES: [&str; 16] = ["7z", "bz2", "cpio", "gz", "mtree", "nupkg", "rar", "tar", "tbz2", "tgz", "txz", "tzst", "uu", "xar", "xz", "zst"];

/// `SHELL_CANDIDATES` that belong to `ArchiveFolder` on this machine, read from the registry once.
fn shell_exts() -> &'static [&'static str] {
    static EXTS: OnceLock<Vec<&'static str>> = OnceLock::new();
    EXTS.get_or_init(|| {
        let t0 = Instant::now();
        let exts: Vec<&str> = SHELL_CANDIDATES.iter().copied().filter(|e| prog_id(e).is_some_and(|p| p.eq_ignore_ascii_case("ArchiveFolder"))).collect();
        eprintln!(
            "[archive] ArchiveFolder takes {} of {} extensions ({}), read in {:.1} ms",
            exts.len(),
            SHELL_CANDIDATES.len(),
            exts.iter().map(|e| format!(".{e}")).collect::<Vec<_>>().join(" "),
            t0.elapsed().as_secs_f64() * 1e3
        );
        exts
    })
}

/// `HKCR\.ext`'s default value (the ProgID), if any.
fn prog_id(ext: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::{HKEY_CLASSES_ROOT, RRF_RT_REG_SZ, RegGetValueW};
    let key: Vec<u16> = format!(".{ext}").encode_utf16().chain([0]).collect();
    let mut buf = [0u16; 128];
    let mut len = (buf.len() * 2) as u32;
    // SAFETY: NUL-terminated key; `buf` and `len` describe a writable buffer of `len` bytes.
    let r = unsafe { RegGetValueW(HKEY_CLASSES_ROOT, key.as_ptr(), std::ptr::null(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut len) };
    if r != 0 {
        return None;
    }
    let n = (len as usize / 2).saturating_sub(1).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..n]))
}

/// The handler for a file named `name`, by its extension (any case): zip, an `ArchiveFolder`
/// extension of this machine, or None.
pub fn handler_for_name(name: &str) -> Option<Handler> {
    handler_with(name, shell_exts())
}

fn handler_with(name: &str, shell: &[&str]) -> Option<Handler> {
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() || ext.is_empty() {
        return None;
    }
    if ext.eq_ignore_ascii_case("zip") {
        Some(Handler::Zip)
    } else if shell.iter().any(|e| e.eq_ignore_ascii_case(ext)) {
        Some(Handler::Shell)
    } else {
        None
    }
}

/// `path` lies inside an archive: the archive's own path and the path below its root (`\`
/// separated, empty for the root itself). None for any other path, including a folder whose name
/// ends in `.zip`. Only a component with an archive's extension costs a file system query.
pub fn split(path: &Path) -> Option<(PathBuf, String)> {
    split_with(path, |n| handler_for_name(n).is_some(), is_file)
}

/// `split`, with the name test and the file test given (the tests count the file queries).
fn split_with(path: &Path, is_archive: impl Fn(&str) -> bool, is_file: impl Fn(&Path) -> bool) -> Option<(PathBuf, String)> {
    let mut acc = PathBuf::new();
    let mut comps = path.components();
    while let Some(c) = comps.next() {
        acc.push(c.as_os_str());
        if let Component::Normal(n) = c {
            if is_archive(&n.to_string_lossy()) && is_file(&acc) {
                let inner: Vec<String> = comps.map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
                return Some((acc, inner.join("\\")));
            }
        }
    }
    None
}

/// `path` lies inside an archive (or is one, as a folder to open).
pub fn inside(path: &Path) -> bool {
    split(path).is_some()
}

/// The item at `path` is stored inside an archive: below its root, not the archive file itself
/// (`inside` says yes for both, as a folder to open; an item named by its path asks this).
pub fn within(path: &Path) -> bool {
    split(path).is_some_and(|(_, inner)| !inner.is_empty())
}

/// `path` lies inside an archive TwinFM writes (a zip).
pub fn writable(path: &Path) -> bool {
    split(path).is_some_and(|(a, _)| handler_of(&a) == Handler::Zip)
}

/// The handler of an archive `split` found (from its name).
pub fn handler_of(archive: &Path) -> Handler {
    archive.file_name().and_then(|n| handler_for_name(&n.to_string_lossy())).unwrap_or(Handler::Shell)
}

/// A file of this name opens as a folder: a zip, or an `ArchiveFolder` extension of this machine.
pub fn is_archive_name(name: &str) -> bool {
    handler_for_name(name).is_some()
}

fn is_file(p: &Path) -> bool {
    let w: Vec<u16> = p.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: `w` is a NUL-terminated wide string for the duration of the call.
    let attrs = unsafe { GetFileAttributesW(w.as_ptr()) };
    attrs != INVALID_FILE_ATTRIBUTES && attrs & FILE_ATTRIBUTE_DIRECTORY == 0
}

/// One entry of a level: a file, or a folder (stored in the archive or implied by a path).
struct Item {
    name: String,
    dir: bool,
    size: u64,
    /// Unix seconds (a zip entry's MS-DOS time, which is local, converted); `NO_TIME` for an
    /// implied folder or an item the shell gives no date, which shows none.
    modified: i64,
}

/// Out of every formatter's range: the Modified cell stays empty.
const NO_TIME: i64 = i64::MIN;

type Level = Arc<Vec<Item>>;

/// A zip's central directory as levels: the lowercase path below the root (`\` separated, ""
/// for the root) to that folder's entries.
pub struct Archive {
    /// Entries in the central directory.
    pub count: usize,
    /// Time to read the central directory and build the levels.
    pub read_ms: f64,
    levels: HashMap<String, Level>,
}

/// What the cache holds for one archive: a zip's every level, or the levels of another format
/// the shell has listed so far (with each one's listing time).
enum Levels {
    Zip(Arc<Archive>),
    Shell(HashMap<String, (Level, f64)>),
}

struct Cached {
    path: PathBuf,
    key: String,
    stamp: (u64, u64),
    levels: Levels,
}

static CACHE: Mutex<Vec<Cached>> = Mutex::new(Vec::new());

fn stamp(archive: &Path) -> Result<(u64, u64), String> {
    let meta = std::fs::metadata(archive).map_err(|e| e.to_string())?;
    Ok((meta.len(), meta.last_write_time()))
}

/// Keep `c` as the most recently used, evicting the least recently used past `CACHE_MAX`.
fn insert(cache: &mut Vec<Cached>, c: Cached) {
    cache.retain(|o| o.key != c.key);
    cache.push(c);
    while cache.len() > CACHE_MAX {
        let old = cache.remove(0);
        eprintln!("[archive] cache evicted {}", old.path.display());
    }
}

/// Drop the archive's cached levels (after TwinFM wrote it), so the next listing reads it again.
pub fn forget(archive: &Path) {
    let key = archive.to_string_lossy().to_lowercase();
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let n = cache.len();
    cache.retain(|c| c.key != key);
    if cache.len() < n {
        eprintln!("[archive] cache dropped {}", archive.display());
    }
}

/// The zip's levels, from the cache when its size and modified time are unchanged (true), else
/// read now (false) and cached.
pub fn open(zip: &Path) -> Result<(Arc<Archive>, bool), String> {
    let stamp = stamp(zip)?;
    let key = zip.to_string_lossy().to_lowercase();
    {
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = cache.iter().position(|c| c.key == key && c.stamp == stamp && matches!(c.levels, Levels::Zip(_))) {
            let c = cache.remove(i);
            let Levels::Zip(a) = &c.levels else { unreachable!("matched above") };
            let a = a.clone();
            cache.push(c);
            return Ok((a, true));
        }
    }
    let archive = Arc::new(read(zip)?);
    eprintln!("[archive] open {}: {} entries, central directory {:.1} ms", zip.display(), archive.count, archive.read_ms);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    insert(&mut cache, Cached { path: zip.to_path_buf(), key, stamp, levels: Levels::Zip(archive.clone()) });
    Ok((archive, false))
}

/// One level of an `ArchiveFolder` archive: from the cache (true) when the archive is unchanged
/// and the level was listed before, else listed now by the shell on the archive worker (false),
/// with the time that took.
fn shell_level(archive: &Path, inner: &str) -> Result<(Level, f64, bool), String> {
    let stamp = stamp(archive)?;
    let key = archive.to_string_lossy().to_lowercase();
    let lkey = inner.to_lowercase();
    {
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = cache.iter().position(|c| c.key == key && c.stamp == stamp) {
            let c = cache.remove(i);
            let hit = match &c.levels {
                Levels::Shell(m) => m.get(&lkey).map(|(l, ms)| (l.clone(), *ms)),
                Levels::Zip(_) => None,
            };
            cache.push(c);
            if let Some((l, ms)) = hit {
                return Ok((l, ms, true));
            }
        }
    }
    // Windows' handler matches names inside the archive case-sensitively: each folder's stored
    // name comes from its parent level (listed and cached as any level).
    let mut stored = String::new();
    for c in inner.split(['\\', '/']).filter(|c| !c.is_empty()) {
        let (up, _, _) = shell_level(archive, &stored)?;
        let c = c.to_lowercase();
        let name = &up.iter().find(|i| i.name.to_lowercase() == c).ok_or_else(not_found)?.name;
        stored = if stored.is_empty() { name.clone() } else { format!("{stored}\\{name}") };
    }
    let full = if stored.is_empty() { archive.to_path_buf() } else { archive.join(&stored) };
    let (raw, ms) = crate::fs::archivefolder::level(&full)?;
    let items: Vec<Item> = raw
        .into_iter()
        .map(|r| Item { name: r.name, dir: r.dir, size: r.size, modified: r.modified.unwrap_or(NO_TIME) })
        .collect();
    eprintln!("[archive] open {} level \"{inner}\": {} entries through ArchiveFolder in {ms:.1} ms", archive.display(), items.len());
    let level: Level = Arc::new(items);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    match cache.iter_mut().find(|c| c.key == key && c.stamp == stamp) {
        Some(Cached { levels: Levels::Shell(m), .. }) => {
            m.insert(lkey, (level.clone(), ms));
        }
        _ => {
            let mut m = HashMap::new();
            m.insert(lkey, (level.clone(), ms));
            insert(&mut cache, Cached { path: archive.to_path_buf(), key, stamp, levels: Levels::Shell(m) });
        }
    }
    Ok((level, ms, false))
}

/// Read the central directory and build the levels. A damaged archive is an error, not a panic.
fn read(zip: &Path) -> Result<Archive, String> {
    let t0 = Instant::now();
    let file = std::fs::File::open(zip).map_err(|e| e.to_string())?;
    let za = zip::ZipArchive::new(std::io::BufReader::new(file)).map_err(|e| format!("not a readable zip archive: {e}"))?;
    let zone = LocalZone::current();
    let mut levels: HashMap<String, Vec<Item>> = HashMap::new();
    levels.insert(String::new(), Vec::new());
    // Full lowercase path of every item made so far: its level and index there.
    let mut made: HashMap<String, (String, usize)> = HashMap::new();
    for i in 0..za.len() {
        let Ok(e) = za.by_index_data(i) else { continue };
        let Ok(name) = e.name() else { continue };
        let comps: Vec<&str> = name.split(['/', '\\']).filter(|c| !c.is_empty() && *c != "." && *c != "..").collect();
        let modified = e.last_modified().map_or(NO_TIME, |t| {
            let tod = t.hour() as i64 * 3600 + t.minute() as i64 * 60 + t.second() as i64;
            zone.from_local(t.year() as i64, t.month() as u32, t.day() as u32, tod)
        });
        let mut parent = String::new();
        for (k, c) in comps.iter().enumerate() {
            let last = k + 1 == comps.len();
            let dir = !last || e.is_dir();
            let lower = c.to_lowercase();
            let full = if parent.is_empty() { lower } else { format!("{parent}\\{lower}") };
            match made.get(&full) {
                // A folder stored after a file inside it implied it: give it its own time.
                Some((level, idx)) if last && dir => {
                    if let Some(item) = levels.get_mut(level).and_then(|l| l.get_mut(*idx)) {
                        item.modified = modified;
                    }
                }
                Some(_) => {}
                None => {
                    let level = levels.entry(parent.clone()).or_default();
                    level.push(Item {
                        name: c.to_string(),
                        dir,
                        size: if dir { 0 } else { e.size() },
                        modified: if last { modified } else { NO_TIME },
                    });
                    made.insert(full.clone(), (parent.clone(), level.len() - 1));
                    if dir {
                        levels.entry(full.clone()).or_default();
                    }
                }
            }
            parent = full;
        }
    }
    let levels = levels.into_iter().map(|(k, v)| (k, Arc::new(v))).collect();
    Ok(Archive { count: za.len(), read_ms: t0.elapsed().as_secs_f64() * 1e3, levels })
}

impl Archive {
    /// The level at `inner` (any case), or None when the archive has no such folder.
    fn level(&self, inner: &str) -> Option<&Level> {
        self.levels.get(&inner.to_lowercase())
    }
}

/// An item as a listing's entry: name, uncompressed size, modified time, the type by extension,
/// the icon keyed by extension (no full-path keys: there is no file to ask about).
fn entry(name: &str, it: &Item, types: &mut TypeNames, keys: &mut IconKeys, zone: &LocalZone) -> Entry {
    let attrs = if it.dir { FILE_ATTRIBUTE_DIRECTORY } else { 0 };
    let kind = Kind::from_attrs(attrs, 0);
    let leaf = name.rsplit('\\').next().unwrap_or(name);
    let type_text = types.for_entry(leaf, it.dir);
    let key = keys.for_item(leaf, it.dir);
    Entry::with_zone(name.into(), it.size, it.modified, attrs, kind, type_text, key, zone)
}

/// Icon keys inside an archive: `dir` for folders, `ext:.<ext>` for files (an `.exe` too: its own
/// icon would need the file), `ext:` for files without an extension.
#[derive(Default)]
struct IconKeys {
    dir: SharedString,
    ext: HashMap<String, SharedString>,
}

impl IconKeys {
    fn for_item(&mut self, name: &str, dir: bool) -> SharedString {
        if dir {
            if self.dir.is_empty() {
                self.dir = "dir".into();
            }
            return self.dir.clone();
        }
        let ext = match name.rfind('.') {
            Some(i) if i + 1 < name.len() => name[i + 1..].to_ascii_lowercase(),
            _ => String::new(),
        };
        self.ext.entry(ext).or_insert_with_key(|e| if e.is_empty() { "ext:".into() } else { format!("ext:.{e}").into() }).clone()
    }
}

/// What the `[enum]` line says about an archive level.
#[derive(Clone, Copy, Debug)]
pub struct LevelInfo {
    pub handler: Handler,
    /// A zip: entries in its central directory. Another format: entries in the level.
    pub count: usize,
    /// A zip: the central directory's read. Another format: the shell's listing of the level.
    pub read_ms: f64,
    pub cached: bool,
}

impl LevelInfo {
    /// The `[enum] ... done:` line's words for the level.
    pub fn describe(&self) -> String {
        let cached = if self.cached { "cached" } else { "fresh" };
        match self.handler {
            Handler::Zip => format!("archive level, central directory {} entries, read in {:.1} ms, {cached}", self.count, self.read_ms),
            Handler::Shell => format!("archive level, ArchiveFolder, listed in {:.1} ms, {cached}", self.read_ms),
        }
    }
}

/// One level of `archive` (`inner` below its root), whichever handler lists it.
fn level_of(archive: &Path, inner: &str) -> Result<(Level, LevelInfo), String> {
    match handler_of(archive) {
        Handler::Zip => {
            let (a, cached) = open(archive)?;
            let l = a.level(inner).ok_or_else(not_found)?.clone();
            Ok((l, LevelInfo { handler: Handler::Zip, count: a.count, read_ms: a.read_ms, cached }))
        }
        Handler::Shell => {
            let (l, ms, cached) = shell_level(archive, inner)?;
            let count = l.len();
            Ok((l, LevelInfo { handler: Handler::Shell, count, read_ms: ms, cached }))
        }
    }
}

/// The entries of one level of `archive` (`inner` below its root), for the enumeration worker.
/// Err with the error text when the archive cannot be read or has no such folder.
pub fn list(archive: &Path, inner: &str, types: &mut TypeNames, zone: &LocalZone) -> Result<(Vec<Entry>, LevelInfo), String> {
    let (items, info) = level_of(archive, inner)?;
    let mut keys = IconKeys::default();
    let entries = items.iter().map(|it| entry(&it.name, it, types, &mut keys, zone)).collect();
    Ok((entries, info))
}

fn not_found() -> String {
    std::io::Error::from_raw_os_error(3).to_string() // ERROR_PATH_NOT_FOUND, as a folder gives
}

/// The folder names of one level, for the crumb dropdown (sorted naturally, at most `cap`), and
/// how many there are.
pub fn subfolders(archive: &Path, inner: &str, cap: usize) -> Result<(Vec<String>, usize), String> {
    let (items, _) = level_of(archive, inner)?;
    let mut names: Vec<String> = items.iter().filter(|i| i.dir).map(|i| i.name.clone()).collect();
    names.sort_by(|x, y| crate::model::natural_cmp(x, y));
    let total = names.len();
    names.truncate(cap);
    Ok((names, total))
}

/// The recursive name search inside an archive: every entry below `inner` whose own name matches,
/// named by its path relative to `inner` (`sub\file.txt`), handed to `emit` in one batch. Returns
/// (found, folders walked); nested archives are files here too. Another format's levels are
/// listed by the shell one by one (and cached).
pub fn walk(archive: &Path, inner: &str, matcher: &NameMatch, mut emit: impl FnMut(Vec<Entry>) -> bool) -> Result<(usize, usize), String> {
    level_of(archive, inner)?;
    let (mut types, zone, mut keys) = (TypeNames::default(), LocalZone::current(), IconKeys::default());
    let mut found = Vec::new();
    let mut walked = 0;
    // (level path as given, relative path prefix with a trailing `\`)
    let mut stack = vec![(inner.to_string(), String::new())];
    while let Some((at, rel)) = stack.pop() {
        walked += 1;
        let Ok((items, _)) = level_of(archive, &at) else { continue };
        for it in items.iter() {
            let name = format!("{rel}{}", it.name);
            if it.dir {
                stack.push((if at.is_empty() { it.name.clone() } else { format!("{at}\\{}", it.name) }, format!("{name}\\")));
            }
            if matcher.matches(&it.name) {
                found.push(entry(&name, it, &mut types, &mut keys, &zone));
            }
        }
    }
    let n = found.len();
    if !found.is_empty() {
        emit(found);
    }
    Ok((n, walked))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    /// Held by the tests that read the cache back: `central_directory_levels` fills it past
    /// `CACHE_MAX`, which would evict another test's archive between two of its listings.
    static CACHE_TESTS: Mutex<()> = Mutex::new(());

    fn names(entries: &[Entry]) -> Vec<(String, Kind, u64)> {
        let mut v: Vec<(String, Kind, u64)> = entries.iter().map(|e| (e.name.to_string(), e.kind, e.size)).collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    /// A zip of `files` (name, contents; a name ending in `/` is a stored folder entry).
    fn make_zip(path: &Path, files: &[(&str, &[u8])], opts: SimpleFileOptions) {
        let mut w = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, data) in files {
            if name.ends_with('/') {
                w.add_directory(*name, opts).unwrap();
            } else {
                w.start_file(*name, opts).unwrap();
                w.write_all(data).unwrap();
            }
        }
        w.finish().unwrap();
    }

    fn level(zip: &Path, inner: &str) -> Result<Vec<Entry>, String> {
        list(zip, inner, &mut TypeNames::default(), &LocalZone::current()).map(|(e, _)| e)
    }

    #[test]
    fn central_directory_levels() {
        let _cache = CACHE_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("twinfm-p3m4-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let time = zip::DateTime::from_date_and_time(2024, 7, 9, 13, 45, 30).unwrap();
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored).last_modified_time(time);

        // Nested folders, a file at depth 3, a stored folder entry and folders only implied by paths.
        let nested = dir.join("nested.zip");
        make_zip(&nested, &[("top.txt", b"12345"), ("a/", b""), ("a/one.bin", b"1"), ("a/b/c/deep.txt", b"deep!"), ("x/y.TXT", b"yy")], opts);
        let (top, info) = list(&nested, "", &mut TypeNames::default(), &LocalZone::current()).unwrap();
        assert_eq!((info.count, info.cached), (5, false), "five central directory entries, read fresh");
        assert_eq!(
            names(&top),
            vec![("a".into(), Kind::Directory, 0), ("top.txt".into(), Kind::File, 5), ("x".into(), Kind::Directory, 0)]
        );
        let t = top.iter().find(|e| e.name == "top.txt").unwrap();
        assert_eq!(t.modified_text.as_str(), "2024-07-09 13:45", "the MS-DOS time is local time");
        assert_eq!(t.icon_key.as_str(), "ext:.txt");
        assert_eq!(top.iter().find(|e| e.name == "a").unwrap().icon_key.as_str(), "dir");
        // An implied folder has no date; a stored one has its own.
        assert_eq!(top.iter().find(|e| e.name == "x").unwrap().modified_text.as_str(), "");
        assert_eq!(top.iter().find(|e| e.name == "a").unwrap().modified_text.as_str(), "2024-07-09 13:45");
        assert_eq!(names(&level(&nested, "a").unwrap()), vec![("b".into(), Kind::Directory, 0), ("one.bin".into(), Kind::File, 1)]);
        assert_eq!(names(&level(&nested, r"A\B\c").unwrap()), vec![("deep.txt".into(), Kind::File, 5)], "any case");
        assert!(level(&nested, "missing").is_err());
        assert!(level(&nested, "top.txt").is_err(), "a file is no level");
        let (_, info) = list(&nested, "a", &mut TypeNames::default(), &LocalZone::current()).unwrap();
        assert!(info.cached, "the second listing comes from the cache");
        assert_eq!(subfolders(&nested, "", 10).unwrap(), (vec!["a".to_string(), "x".to_string()], 2));
        // The search inside an archive: every match below the level, named by its relative path.
        let mut found = Vec::new();
        let (n, walked) = walk(&nested, "", &NameMatch::new("*.txt"), |b| {
            found.extend(b.into_iter().map(|e| e.name.to_string()));
            true
        })
        .unwrap();
        found.sort();
        assert_eq!((n, walked), (3, 5), "three matches in five folders");
        assert_eq!(found, vec![r"a\b\c\deep.txt".to_string(), "top.txt".into(), r"x\y.TXT".into()]);

        // A zip whose only entry is a top-level folder; an empty zip.
        let toponly = dir.join("toponly.zip");
        make_zip(&toponly, &[("only/", b"")], opts);
        assert_eq!(names(&level(&toponly, "").unwrap()), vec![("only".into(), Kind::Directory, 0)]);
        assert!(level(&toponly, "only").unwrap().is_empty());
        let empty = dir.join("empty.zip");
        make_zip(&empty, &[], opts);
        assert!(level(&empty, "").unwrap().is_empty());

        // Non-ASCII names (the writer sets the UTF-8 flag) and a zip64 entry.
        let utf8 = dir.join("utf8.zip");
        make_zip(&utf8, &[("répertoire/naïve ß.txt", b"x"), ("日本語.txt", b"yz")], opts);
        assert_eq!(names(&level(&utf8, "").unwrap()), vec![("répertoire".into(), Kind::Directory, 0), ("日本語.txt".into(), Kind::File, 2)]);
        assert_eq!(names(&level(&utf8, "RÉPERTOIRE").unwrap()), vec![("naïve ß.txt".into(), Kind::File, 1)]);
        let z64 = dir.join("zip64.zip");
        make_zip(&z64, &[("big/large.bin", b"zip64 extra field")], opts.large_file(true));
        assert_eq!(names(&level(&z64, "big").unwrap()), vec![("large.bin".into(), Kind::File, 17)]);

        // A damaged central directory is an error, not a panic.
        let corrupt = dir.join("corrupt.zip");
        let mut bytes = std::fs::read(&nested).unwrap();
        let n = bytes.len();
        bytes[n - 22..].fill(0xEE); // the end-of-central-directory record
        std::fs::write(&corrupt, &bytes).unwrap();
        let e = level(&corrupt, "").unwrap_err();
        assert!(e.starts_with("not a readable zip archive"), "{e}");
        std::fs::write(&corrupt, b"not a zip at all").unwrap();
        assert!(level(&corrupt, "").is_err());

        // A changed archive is read again; past CACHE_MAX archives the oldest is evicted.
        make_zip(&nested, &[("new.txt", b"n")], opts);
        let (e, info) = list(&nested, "", &mut TypeNames::default(), &LocalZone::current()).unwrap();
        assert_eq!((names(&e), info.cached), (vec![("new.txt".into(), Kind::File, 1)], false), "size or time changed: read fresh");
        for k in 0..CACHE_MAX {
            let z = dir.join(format!("c{k}.zip"));
            make_zip(&z, &[("f", b"")], opts);
            level(&z, "").unwrap();
        }
        let key = nested.to_string_lossy().to_lowercase();
        assert!(!CACHE.lock().unwrap().iter().any(|c| c.key == key), "evicted after {CACHE_MAX} newer archives");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn archive_paths_are_decided_by_the_file_system() {
        let dir = std::env::temp_dir().join(format!("twinfm-p3m4-split-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("folder.zip").join("inner")).unwrap();
        std::fs::write(dir.join("a.zip"), b"PK").unwrap();
        std::fs::write(dir.join("a.zipx"), b"PK").unwrap();
        let a = dir.join("a.zip");
        assert_eq!(split(&a.join("b")), Some((a.clone(), "b".into())), "inside");
        assert_eq!(split(&a.join("b").join("c d.txt")), Some((a.clone(), r"b\c d.txt".into())));
        assert_eq!(split(&a), Some((a.clone(), String::new())), "the root");
        assert_eq!(split(&dir.join("A.ZIP").join("b")).map(|(_, i)| i), Some("b".into()), "any case");
        assert_eq!(split(&dir.join("a.zipx")), None, "not a zip");
        assert_eq!(split(&dir.join("a.zipx").join("b")), None);
        assert_eq!(split(&dir.join("folder.zip").join("inner")), None, "a folder named .zip is a folder");
        assert_eq!(split(&dir.join("missing.zip").join("b")), None, "nothing there");
        assert_eq!(split(&dir), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn handlers_by_extension() {
        let shell = ["7z", "gz", "tar", "rar"];
        assert_eq!(handler_with("a.zip", &shell), Some(Handler::Zip));
        assert_eq!(handler_with("A.ZIP", &[]), Some(Handler::Zip), "zip is TwinFM's own, whatever the registry says");
        assert_eq!(handler_with("x.7z", &shell), Some(Handler::Shell));
        assert_eq!(handler_with("x.TAR.GZ", &shell), Some(Handler::Shell), "a .tar.gz is a .gz");
        assert_eq!(handler_with("x.7z", &["gz"]), None, "an extension another program owns opens as a file");
        assert_eq!(handler_with("a.zipx", &shell), None);
        assert_eq!(handler_with(".7z", &shell), None, "no stem: a dot file");
        assert_eq!(handler_with("x.", &shell), None);
        assert_eq!(handler_with("readme", &shell), None);
    }

    /// The split asks the file system only about components named like archives, stops at the
    /// first archive (an archive inside one is a path inside the outer), and treats a folder named
    /// like an archive as a folder.
    #[test]
    fn split_asks_only_about_archive_names() {
        let shell = ["7z", "gz"];
        let is_archive = |n: &str| handler_with(n, &shell).is_some();
        let asked = std::cell::RefCell::new(Vec::new());
        let files = [r"C:\d\x.7z", r"C:\d\a.zip", r"C:\d\t.tar.gz"];
        let is_file = |p: &Path| {
            asked.borrow_mut().push(p.to_string_lossy().into_owned());
            files.iter().any(|f| Path::new(f) == p)
        };
        let split = |p: &str| split_with(Path::new(p), is_archive, is_file);
        assert_eq!(split(r"C:\Windows\System32\drivers\etc"), None);
        assert!(asked.borrow().is_empty(), "no archive names: no query");
        assert_eq!(split(r"C:\d\x.7z\sub\f.txt"), Some((PathBuf::from(r"C:\d\x.7z"), r"sub\f.txt".into())));
        assert_eq!(split(r"C:\d\t.tar.gz"), Some((PathBuf::from(r"C:\d\t.tar.gz"), String::new())), "the root");
        assert_eq!(split(r"C:\d\a.zip\inner.7z\x"), Some((PathBuf::from(r"C:\d\a.zip"), r"inner.7z\x".into())), "the outer archive");
        asked.borrow_mut().clear();
        assert_eq!(split(r"C:\d\folder.7z\x"), None, "a folder named .7z");
        assert_eq!(*asked.borrow(), vec![r"C:\d\folder.7z".to_string()], "one query, for the archive-named component");
        assert_eq!(split(r"C:\d\x.rar\y"), None, "not this machine's archive");
    }

    /// A 7z TwinFM makes, listed through Windows' `ArchiveFolder` on the archive worker: its levels,
    /// the cache, the crumb dropdown's folders, the search inside it, and `forget`.
    #[test]
    fn shell_levels_are_listed_and_cached() {
        let _cache = CACHE_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("twinfm-p7m3-shell-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let top = dir.join("top");
        std::fs::create_dir_all(top.join("b").join("c")).unwrap();
        std::fs::write(top.join("one.txt"), b"12345").unwrap();
        std::fs::write(top.join("b").join("two.TXT"), b"22").unwrap();
        std::fs::write(top.join("b").join("c").join("deep.bin"), b"d").unwrap();
        let a = dir.join("t.7z");
        crate::fs::archivewrite::compress(&a, &[top.clone()], crate::fs::archivewrite::Format::SevenZ, &mut |_, _| {}).unwrap();
        if handler_for_name("t.7z") != Some(Handler::Shell) {
            eprintln!("ArchiveFolder does not take .7z on this machine: skipped");
            return std::fs::remove_dir_all(&dir).unwrap();
        }
        assert_eq!(split(&a.join("top")), Some((a.clone(), "top".into())));
        let (root, info) = list(&a, "", &mut TypeNames::default(), &LocalZone::current()).unwrap();
        assert_eq!(names(&root), vec![("top".into(), Kind::Directory, 0)]);
        assert_eq!((info.handler, info.count, info.cached), (Handler::Shell, 1, false));
        let (lvl, _) = list(&a, "top", &mut TypeNames::default(), &LocalZone::current()).unwrap();
        assert_eq!(names(&lvl), vec![("b".into(), Kind::Directory, 0), ("one.txt".into(), Kind::File, 5)]);
        let one = lvl.iter().find(|e| e.name == "one.txt").unwrap();
        assert!(!one.modified_text.is_empty(), "the shell gives the file's date");
        assert_eq!(one.icon_key.as_str(), "ext:.txt");
        let (_, info) = list(&a, "TOP", &mut TypeNames::default(), &LocalZone::current()).unwrap();
        assert!(info.cached, "the level again (any case) comes from the cache");
        assert!(info.describe().starts_with("archive level, ArchiveFolder, listed in "), "{}", info.describe());
        assert!(list(&a, r"top\missing", &mut TypeNames::default(), &LocalZone::current()).is_err());
        assert_eq!(subfolders(&a, "top", 10).unwrap(), (vec!["b".to_string()], 1));
        let mut found = Vec::new();
        let (n, walked) = walk(&a, "top", &NameMatch::new("*.txt"), |b| {
            found.extend(b.into_iter().map(|e| e.name.to_string()));
            true
        })
        .unwrap();
        found.sort();
        assert_eq!((n, walked), (2, 3), "two matches in three folders");
        assert_eq!(found, vec![r"b\two.TXT".to_string(), "one.txt".into()]);
        forget(&a);
        let (_, info) = list(&a, "top", &mut TypeNames::default(), &LocalZone::current()).unwrap();
        assert!(!info.cached, "forgotten: listed again");
        crate::fs::archivefolder::remove_scratch(&dir);
    }

    /// Windows' `ArchiveFolder` matches names inside an archive case-sensitively; a level asked for
    /// in another case, cold, is found by its stored names (a typed path, `x.7z\TOP\B`).
    #[test]
    fn shell_levels_take_any_case() {
        let _cache = CACHE_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("twinfm-archive-case-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let top = dir.join("top");
        std::fs::create_dir_all(top.join("b")).unwrap();
        std::fs::write(top.join("b").join("x.txt"), b"x").unwrap();
        let a = dir.join("t.7z");
        crate::fs::archivewrite::compress(&a, &[top.clone()], crate::fs::archivewrite::Format::SevenZ, &mut |_, _| {}).unwrap();
        if handler_for_name("t.7z") != Some(Handler::Shell) {
            eprintln!("ArchiveFolder does not take .7z on this machine: skipped");
            return std::fs::remove_dir_all(&dir).unwrap();
        }
        let (lvl, info) = list(&a, r"TOP\B", &mut TypeNames::default(), &LocalZone::current()).unwrap();
        assert_eq!(names(&lvl), vec![("x.txt".into(), Kind::File, 1)]);
        assert!(!info.cached, "the first listing of that level");
        assert!(list(&a, r"top\b", &mut TypeNames::default(), &LocalZone::current()).unwrap().1.cached, "the same level in its stored case");
        assert_eq!(list(&a, r"Top\Missing", &mut TypeNames::default(), &LocalZone::current()).unwrap_err(), not_found());
        crate::fs::archivefolder::remove_scratch(&dir);
    }
}
