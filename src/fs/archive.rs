//! Zip archives listed as folders (P3-M4). A path such as `C:\dir\x.zip\sub` lies inside the
//! archive `C:\dir\x.zip` when that component is a file (a folder named `x.zip` is a folder);
//! `split` decides it, asking the file system only for components whose name ends in `.zip`.
//!
//! An archive's central directory is read once with the `zip` crate (no decompression: the shell
//! extracts) into a map of levels, one per folder inside it, and kept in a cache of the
//! `CACHE_MAX` most recently used archives, keyed by path, size and modified time. Listing a level
//! is a map lookup. Workers only: the enumeration worker, the search walker and the crumb
//! dropdown's lister call it; the UI thread never reads an archive.

use crate::fs::enumerate::TypeNames;
use crate::model::{Entry, Kind, LocalZone, NameMatch};
use slint::SharedString;
use std::collections::HashMap;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, GetFileAttributesW, INVALID_FILE_ATTRIBUTES};

/// Archives whose central directory stays cached (the most recently used).
pub const CACHE_MAX: usize = 8;

/// `path` lies inside a zip archive: the archive's own path and the path below its root (`\`
/// separated, empty for the root itself). None for any other path, including a folder whose name
/// ends in `.zip`. Only a component named `*.zip` costs a file system query.
pub fn split(path: &Path) -> Option<(PathBuf, String)> {
    if !path.to_string_lossy().to_ascii_lowercase().contains(".zip") {
        return None;
    }
    let mut acc = PathBuf::new();
    let mut comps = path.components();
    while let Some(c) = comps.next() {
        acc.push(c.as_os_str());
        if let Component::Normal(n) = c {
            if n.to_string_lossy().to_ascii_lowercase().ends_with(".zip") && is_file(&acc) {
                let inner: Vec<String> = comps.map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
                return Some((acc, inner.join("\\")));
            }
        }
    }
    None
}

/// `path` lies inside a zip archive (or is one, as a folder to open).
pub fn inside(path: &Path) -> bool {
    split(path).is_some()
}

/// A name ending in `.zip` (any case): a file of that name opens as a folder.
pub fn is_zip_name(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".zip")
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
    /// Unix seconds (the entry's MS-DOS time, which is local, converted); `NO_TIME` for an implied
    /// folder, which shows no date.
    modified: i64,
}

/// Out of every formatter's range: the Modified cell stays empty.
const NO_TIME: i64 = i64::MIN;

/// An archive's central directory as levels: the lowercase path below the root (`\` separated,
/// "" for the root) to that folder's entries.
pub struct Archive {
    /// Entries in the central directory.
    pub count: usize,
    /// Time to read the central directory and build the levels.
    pub read_ms: f64,
    levels: HashMap<String, Vec<Item>>,
}

struct Cached {
    path: PathBuf,
    key: String,
    stamp: (u64, u64),
    archive: Arc<Archive>,
}

static CACHE: Mutex<Vec<Cached>> = Mutex::new(Vec::new());

/// The archive's levels, from the cache when its size and modified time are unchanged (true),
/// else read now (false) and cached, evicting the least recently used past `CACHE_MAX`.
pub fn open(zip: &Path) -> Result<(Arc<Archive>, bool), String> {
    let meta = std::fs::metadata(zip).map_err(|e| e.to_string())?;
    let stamp = (meta.len(), meta.last_write_time());
    let key = zip.to_string_lossy().to_lowercase();
    {
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = cache.iter().position(|c| c.key == key && c.stamp == stamp) {
            let c = cache.remove(i);
            let a = c.archive.clone();
            cache.push(c);
            return Ok((a, true));
        }
    }
    let archive = Arc::new(read(zip)?);
    eprintln!("[archive] open {}: {} entries, central directory {:.1} ms", zip.display(), archive.count, archive.read_ms);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache.retain(|c| c.key != key);
    cache.push(Cached { path: zip.to_path_buf(), key, stamp, archive: archive.clone() });
    while cache.len() > CACHE_MAX {
        let old = cache.remove(0);
        eprintln!("[archive] cache evicted {}", old.path.display());
    }
    Ok((archive, false))
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
    Ok(Archive { count: za.len(), read_ms: t0.elapsed().as_secs_f64() * 1e3, levels })
}

impl Archive {
    /// The level at `inner` (any case), or None when the archive has no such folder.
    fn level(&self, inner: &str) -> Option<&Vec<Item>> {
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
    /// Entries in the archive's central directory.
    pub count: usize,
    pub read_ms: f64,
    pub cached: bool,
}

/// The entries of one level of `zip` (`inner` below its root), for the enumeration worker. Err
/// with the error text when the archive cannot be read or has no such folder.
pub fn list(zip: &Path, inner: &str, types: &mut TypeNames, zone: &LocalZone) -> Result<(Vec<Entry>, LevelInfo), String> {
    let (a, cached) = open(zip)?;
    let items = a.level(inner).ok_or_else(not_found)?;
    let mut keys = IconKeys::default();
    let entries = items.iter().map(|it| entry(&it.name, it, types, &mut keys, zone)).collect();
    Ok((entries, LevelInfo { count: a.count, read_ms: a.read_ms, cached }))
}

fn not_found() -> String {
    std::io::Error::from_raw_os_error(3).to_string() // ERROR_PATH_NOT_FOUND, as a folder gives
}

/// The folder names of one level, for the crumb dropdown (sorted naturally, at most `cap`), and
/// how many there are.
pub fn subfolders(zip: &Path, inner: &str, cap: usize) -> Result<(Vec<String>, usize), String> {
    let (a, _) = open(zip)?;
    let mut names: Vec<String> = a.level(inner).ok_or_else(not_found)?.iter().filter(|i| i.dir).map(|i| i.name.clone()).collect();
    names.sort_by(|x, y| crate::model::natural_cmp(x, y));
    let total = names.len();
    names.truncate(cap);
    Ok((names, total))
}

/// The recursive name search inside an archive: every entry below `inner` whose own name matches,
/// named by its path relative to `inner` (`sub\file.txt`), handed to `emit` in one batch. Returns
/// (found, folders walked); nested archives are files here too.
pub fn walk(zip: &Path, inner: &str, matcher: &NameMatch, mut emit: impl FnMut(Vec<Entry>) -> bool) -> Result<(usize, usize), String> {
    let (a, _) = open(zip)?;
    a.level(inner).ok_or_else(not_found)?;
    let (mut types, zone, mut keys) = (TypeNames::default(), LocalZone::current(), IconKeys::default());
    let mut found = Vec::new();
    let mut walked = 0;
    // (lowercase level key, relative path prefix with a trailing `\`)
    let mut stack = vec![(inner.to_lowercase(), String::new())];
    while let Some((key, rel)) = stack.pop() {
        walked += 1;
        for it in a.levels.get(&key).map_or(&[][..], Vec::as_slice) {
            let name = format!("{rel}{}", it.name);
            if it.dir {
                let lower = it.name.to_lowercase();
                stack.push((if key.is_empty() { lower } else { format!("{key}\\{lower}") }, format!("{name}\\")));
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
        assert!(is_zip_name("X.Zip") && !is_zip_name("x.zipx"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
