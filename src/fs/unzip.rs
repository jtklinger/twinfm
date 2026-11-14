//! A whole zip unpacked by TwinFM's own reader (P15-M1's Extract): the `zip` crate that lists zips
//! (`fs::archive`) reads every entry and writes it below a destination folder. The shell's zip
//! folder, which a copy out of a zip goes through, takes far longer on many small files
//! (`docs/phase-15-plan.md` § P15-M1, Settled first). Workers only: the file operation worker
//! calls it (`fileops::archive::extract`).
//!
//! An entry's name is split at `/` and `\`, and empty, `.` and `..` components are dropped, as the
//! listing does, so nothing lands outside the destination; a zip with a name Windows cannot write
//! (a `:` in it, which would also name a drive) is refused whole when it is opened.

use crate::model::LocalZone;
use std::collections::HashSet;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

/// One entry: its index in the zip, its path below the destination, and what the central
/// directory says of it.
struct Item {
    index: usize,
    rel: PathBuf,
    dir: bool,
    size: u64,
    encrypted: bool,
    /// Unix seconds, from the entry's MS-DOS time (local).
    modified: Option<i64>,
}

/// What an extract wrote.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub files: usize,
    pub bytes: u64,
}

/// An open zip and its entries.
pub struct Unzip {
    za: zip::ZipArchive<BufReader<std::fs::File>>,
    items: Vec<Item>,
}

/// An entry's path below the destination: its components without the empty ones, `.` and `..`.
/// None when nothing is left, Err for a name Windows cannot write there.
fn rel_path(name: &str) -> Result<Option<PathBuf>, String> {
    let comps: Vec<&str> = name.split(['/', '\\']).filter(|c| !c.is_empty() && *c != "." && *c != "..").collect();
    if comps.iter().any(|c| c.contains(':')) {
        return Err(format!("{name} cannot be a file name on Windows"));
    }
    Ok((!comps.is_empty()).then(|| comps.iter().collect()))
}

impl Unzip {
    /// Read the zip's central directory. A damaged archive is an error, not a panic.
    pub fn open(zip: &Path) -> Result<Unzip, String> {
        let file = std::fs::File::open(zip).map_err(|e| e.to_string())?;
        let za = zip::ZipArchive::new(BufReader::new(file)).map_err(|e| format!("not a readable zip archive: {e}"))?;
        let zone = LocalZone::current();
        let mut items = Vec::with_capacity(za.len());
        for index in 0..za.len() {
            let e = za.by_index_data(index).map_err(|e| e.to_string())?;
            let name = e.name().map_err(|e| e.to_string())?;
            let Some(rel) = rel_path(&name)? else { continue };
            let modified = e.last_modified().map(|t| {
                let tod = t.hour() as i64 * 3600 + t.minute() as i64 * 60 + t.second() as i64;
                zone.from_local(t.year() as i64, t.month() as u32, t.day() as u32, tod)
            });
            items.push(Item { index, rel, dir: e.is_dir(), size: if e.is_dir() { 0 } else { e.size() }, encrypted: e.encrypted(), modified });
        }
        Ok(Unzip { za, items })
    }

    /// The names of the items at the zip's root (each entry's first component), in the zip's
    /// order, once each whatever its case.
    pub fn roots(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        self.items
            .iter()
            .filter_map(|i| i.rel.components().next().map(|c| c.as_os_str().to_string_lossy().into_owned()))
            .filter(|r| seen.insert(r.to_lowercase()))
            .collect()
    }

    /// The zip's files that are in `dest` already, by their paths below it. Only what lies under a
    /// root item that is there is looked at, so an extract into a folder with none of them costs
    /// one query a root item.
    pub fn clashes(&self, dest: &Path) -> Vec<String> {
        let there = |p: &Path| std::fs::symlink_metadata(p).is_ok();
        let taken: HashSet<String> = self.roots().into_iter().filter(|r| there(&dest.join(r))).map(|r| r.to_lowercase()).collect();
        if taken.is_empty() {
            return Vec::new();
        }
        let under = |i: &Item| i.rel.components().next().is_some_and(|c| taken.contains(&c.as_os_str().to_string_lossy().to_lowercase()));
        self.items.iter().filter(|i| !i.dir && under(i) && there(&dest.join(&i.rel))).map(|i| i.rel.display().to_string()).collect()
    }

    /// Write every entry below `dest` (which exists): folders made as they are met, files
    /// replaced, or left when `skip_existing`, each given the entry's modified time; encrypted
    /// entries read with `password`. `progress` gets (bytes done, bytes in all) after each file;
    /// `cancelled` is asked before each entry. Ok((finished, what was written)): finished is false
    /// when it was cancelled. The first entry that cannot be read or written stops it, as Err.
    pub fn extract(&mut self, dest: &Path, password: Option<&str>, skip_existing: bool, progress: &mut dyn FnMut(u64, u64), cancelled: &dyn Fn() -> bool) -> Result<(bool, Stats), String> {
        let total: u64 = self.items.iter().map(|i| i.size).sum();
        let mut made: HashSet<PathBuf> = HashSet::new();
        let mut stats = Stats::default();
        let mut done = 0u64;
        for it in &self.items {
            if cancelled() {
                return Ok((false, stats));
            }
            let path = dest.join(&it.rel);
            let why = |e: &dyn std::fmt::Display| format!("{}: {e}", it.rel.display());
            let folder = if it.dir { Some(path.as_path()) } else { path.parent() };
            if let Some(f) = folder {
                if made.insert(f.to_path_buf()) {
                    std::fs::create_dir_all(f).map_err(|e| why(&e))?;
                }
            }
            if it.dir {
                continue;
            }
            done += it.size;
            if skip_existing && std::fs::symlink_metadata(&path).is_ok() {
                continue;
            }
            let mut from = match password {
                Some(p) if it.encrypted => self.za.by_index_decrypt(it.index, p.as_bytes()),
                _ => self.za.by_index(it.index),
            }
            .map_err(|e| why(&e))?;
            let mut out = std::fs::File::create(&path).map_err(|e| why(&e))?;
            stats.bytes += std::io::copy(&mut from, &mut out).map_err(|e| why(&e))?;
            stats.files += 1;
            if let Some(secs) = it.modified.filter(|s| *s > 0) {
                let _ = out.set_modified(UNIX_EPOCH + Duration::from_secs(secs as u64));
            }
            progress(done, total);
        }
        Ok((true, stats))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn entry_names_stay_below_the_destination() {
        assert_eq!(rel_path("a/b.txt"), Ok(Some(PathBuf::from(r"a\b.txt"))));
        assert_eq!(rel_path(r"a\b.txt"), Ok(Some(PathBuf::from(r"a\b.txt"))), "a zip written with backslashes");
        assert_eq!(rel_path("../../a/./b.txt"), Ok(Some(PathBuf::from(r"a\b.txt"))), "nothing climbs out");
        assert_eq!(rel_path("/a//b/"), Ok(Some(PathBuf::from(r"a\b"))), "no root, no empty component");
        assert_eq!(rel_path("../"), Ok(None));
        assert!(rel_path("C:/Windows/x.txt").is_err(), "a drive is never named");
        assert!(rel_path("a/b:stream").is_err());
    }

    #[test]
    fn a_zip_is_unpacked_with_its_folders_times_and_password() {
        let root = std::env::temp_dir().join(format!("twinfm-p15m1-unzip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let zip = root.join("a.zip");
        let mut zw = zip::ZipWriter::new(std::fs::File::create(&zip).unwrap());
        let when = zip::DateTime::from_date_and_time(2024, 3, 9, 10, 20, 30).unwrap();
        let plain = zip::write::SimpleFileOptions::default().last_modified_time(when);
        zw.start_file("top.txt", plain).unwrap();
        zw.write_all(b"top").unwrap();
        zw.add_directory("empty/", plain).unwrap();
        zw.start_file("sub/deep/b.txt", plain).unwrap();
        zw.write_all(b"deeper").unwrap();
        zw.start_file("../escape.txt", plain).unwrap();
        zw.write_all(b"x").unwrap();
        zw.start_file("SUB/c.txt", plain.with_aes_encryption(zip::AesMode::Aes256, "twinfm")).unwrap();
        zw.write_all(b"secret").unwrap();
        zw.finish().unwrap();

        let mut u = Unzip::open(&zip).unwrap();
        assert_eq!(u.roots(), ["top.txt", "empty", "sub", "escape.txt"], "SUB is sub again; ..\\ is dropped");
        let dest = root.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        assert!(u.clashes(&dest).is_empty());
        // Without the password the encrypted entry stops it, after the plain ones.
        let err = u.extract(&dest, None, false, &mut |_, _| {}, &|| false).unwrap_err();
        assert!(err.starts_with(r"SUB\c.txt: "), "{err}");
        let mut last = (0, 0);
        let (finished, stats) = u.extract(&dest, Some("twinfm"), false, &mut |d, t| last = (d, t), &|| false).unwrap();
        assert_eq!((finished, stats), (true, Stats { files: 4, bytes: 16 }));
        assert_eq!(last, (16, 16), "the last report is the whole of it");
        assert_eq!(std::fs::read(dest.join("top.txt")).unwrap(), b"top");
        assert_eq!(std::fs::read(dest.join(r"sub\deep\b.txt")).unwrap(), b"deeper");
        assert_eq!(std::fs::read(dest.join(r"sub\c.txt")).unwrap(), b"secret");
        assert!(dest.join("empty").is_dir());
        assert!(dest.join("escape.txt").is_file() && !root.join("escape.txt").exists(), "inside the destination");
        let zone = LocalZone::current();
        let want = UNIX_EPOCH + Duration::from_secs(zone.from_local(2024, 3, 9, 10 * 3600 + 20 * 60 + 30) as u64);
        assert_eq!(std::fs::metadata(dest.join("top.txt")).unwrap().modified().unwrap(), want, "the entry's own time");

        // Again: everything clashes; skipping leaves the files as they are, replacing writes them.
        assert_eq!(u.clashes(&dest), ["top.txt", r"sub\deep\b.txt", "escape.txt", r"SUB\c.txt"]);
        std::fs::write(dest.join("top.txt"), b"mine").unwrap();
        let (_, stats) = u.extract(&dest, Some("twinfm"), true, &mut |_, _| {}, &|| false).unwrap();
        assert_eq!(stats, Stats::default(), "all four were there");
        assert_eq!(std::fs::read(dest.join("top.txt")).unwrap(), b"mine");
        let (_, stats) = u.extract(&dest, Some("twinfm"), false, &mut |_, _| {}, &|| false).unwrap();
        assert_eq!(stats.files, 4);
        assert_eq!(std::fs::read(dest.join("top.txt")).unwrap(), b"top");

        // Cancelled before the second entry: one file written.
        let other = root.join("part");
        std::fs::create_dir_all(&other).unwrap();
        let asked = std::cell::Cell::new(0);
        let (finished, stats) = u
            .extract(&other, Some("twinfm"), false, &mut |_, _| {}, &|| {
                asked.set(asked.get() + 1);
                asked.get() > 1
            })
            .unwrap();
        assert_eq!((finished, stats.files), (false, 1));

        // A name with a drive in it refuses the whole zip.
        let bad = root.join("bad.zip");
        let mut zw = zip::ZipWriter::new(std::fs::File::create(&bad).unwrap());
        zw.start_file("C:/x.txt", plain).unwrap();
        zw.finish().unwrap();
        assert!(Unzip::open(&bad).is_err_and(|e| e.contains("cannot be a file name")));
        assert!(Unzip::open(&root.join("none.zip")).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
