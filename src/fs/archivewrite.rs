//! TwinFM's own archive writers (P7-M3, Jeremy's choice over the shell's zip folder): edits of a
//! zip (add files and folders, a new folder or empty file, rename, delete) and new 7z and TAR
//! archives for "Compress to". Run on the file operation worker as jobs.
//!
//! A zip edit never writes the archive in place: it writes the whole archive again into a
//! temporary file beside it (`<name>.~twinfm<pid>.tmp`), the existing entries copied raw (no
//! recompression, encrypted entries untouched), the new ones deflated, then swaps it in with
//! `ReplaceFileW` (which keeps the original's attributes and security). A failure at any point
//! removes the temporary file and leaves the archive as it was. Explorer's zip folder works the
//! same way (`x.zip~RF....TMP`, P7-M3's check).
//!
//! Paths inside a zip are given as TwinFM shows them: `\`-separated below the archive's root,
//! matched without regard to case; entry names are written with `/`.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;

/// What to do with files an add would put where files already are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conflict {
    Replace,
    Skip,
}

/// Progress: (bytes done, bytes in all).
pub type Progress<'a> = &'a mut dyn FnMut(u64, u64);

/// An entry name as compared: `/`-separated, no trailing `/`, lowercase.
fn norm(name: &str) -> String {
    name.replace('\\', "/").trim_matches('/').to_lowercase()
}

/// An entry's name ("" when it cannot be decoded; such an entry is still copied raw).
fn ename<R: Read>(e: &zip::read::ZipFile<'_, R>) -> String {
    e.name().map(|n| n.into_owned()).unwrap_or_default()
}

/// A TwinFM inner path (`a\b`) as an entry name prefix (`a/b`), "" for the root.
fn entry_path(inner: &str) -> String {
    inner.replace('\\', "/").trim_matches('/').to_string()
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() { name.to_string() } else { format!("{dir}/{name}") }
}

/// Every name the archive holds, files and folders (stored or implied by a path), normalised,
/// with whether it is a folder.
fn names(za: &mut zip::ZipArchive<std::fs::File>) -> (HashSet<String>, HashSet<String>) {
    let (mut files, mut dirs) = (HashSet::new(), HashSet::new());
    for i in 0..za.len() {
        let Ok(e) = za.by_index_raw(i) else { continue };
        let n = norm(&ename(&e));
        let mut parts: Vec<&str> = n.split('/').collect();
        if e.is_dir() {
            dirs.insert(n.clone());
        } else {
            files.insert(n.clone());
        }
        parts.pop();
        while !parts.is_empty() {
            dirs.insert(parts.join("/"));
            parts.pop();
        }
    }
    (files, dirs)
}

/// What a rewrite changes: entries left out, entries renamed (old prefix to new prefix), and
/// what is added after the kept entries.
struct Plan<'a> {
    skip: &'a dyn Fn(&str) -> bool,
    rename: Option<(String, String)>,
    add: &'a mut dyn FnMut(&mut zip::ZipWriter<std::io::BufWriter<std::fs::File>>, Progress) -> Result<(), String>,
    /// Bytes the additions will write, for the progress total.
    add_bytes: u64,
}

fn temp_for(zip: &Path) -> PathBuf {
    let name = zip.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    zip.with_file_name(format!("{name}.~twinfm{}.tmp", std::process::id()))
}

fn wide(p: &Path) -> Vec<u16> {
    p.as_os_str().encode_wide().chain([0]).collect()
}

/// Swap `temp` in for `target` (`ReplaceFileW`, keeping the target's attributes and security).
fn replace(target: &Path, temp: &Path) -> Result<(), String> {
    use windows_sys::Win32::Storage::FileSystem::{REPLACEFILE_IGNORE_MERGE_ERRORS, ReplaceFileW};
    let (t, n) = (wide(target), wide(temp));
    // SAFETY: NUL-terminated paths that outlive the call; no backup file, no reserved pointers.
    let ok = unsafe { ReplaceFileW(t.as_ptr(), n.as_ptr(), std::ptr::null(), REPLACEFILE_IGNORE_MERGE_ERRORS, std::ptr::null(), std::ptr::null()) };
    if ok == 0 {
        return Err(format!("replacing the archive failed: {}", std::io::Error::last_os_error()));
    }
    Ok(())
}

/// Write `zip` again by `plan` into a temporary file and swap it in.
fn rewrite(zip: &Path, plan: Plan, progress: Progress) -> Result<(), String> {
    let temp = temp_for(zip);
    let res = (|| {
        let mut za = zip::ZipArchive::new(std::fs::File::open(zip).map_err(|e| e.to_string())?).map_err(|e| format!("not a readable zip archive: {e}"))?;
        let mut kept = 0u64;
        for i in 0..za.len() {
            let e = za.by_index_raw(i).map_err(|e| e.to_string())?;
            if !(plan.skip)(&norm(&ename(&e))) {
                kept += e.compressed_size();
            }
        }
        let total = kept + plan.add_bytes;
        let out = std::fs::File::create(&temp).map_err(|e| format!("cannot write beside the archive: {e}"))?;
        let mut w = zip::ZipWriter::new(std::io::BufWriter::new(out));
        if !za.comment().is_empty() {
            w.set_raw_comment(za.comment().to_vec().into_boxed_slice()).map_err(|e| e.to_string())?;
        }
        let mut done = 0u64;
        for i in 0..za.len() {
            let e = za.by_index_raw(i).map_err(|e| e.to_string())?;
            let n = norm(&ename(&e));
            if (plan.skip)(&n) {
                continue;
            }
            let size = e.compressed_size();
            // A renamed item and everything below it: the new name, then the entry's own spelling
            // of the components below the renamed one.
            let renamed = plan.rename.as_ref().and_then(|(old, new)| {
                (n == *old || n.starts_with(&format!("{old}/"))).then(|| {
                    let raw = ename(&e).replace('\\', "/");
                    let parts: Vec<&str> = raw.split('/').filter(|c| !c.is_empty()).collect();
                    let below = &parts[old.split('/').count().min(parts.len())..];
                    let mut name = std::iter::once(new.as_str()).chain(below.iter().copied()).collect::<Vec<_>>().join("/");
                    if e.is_dir() {
                        name.push('/');
                    }
                    name
                })
            });
            match renamed {
                Some(name) => w.raw_copy_file_rename(e, name).map_err(|e| e.to_string())?,
                None => w.raw_copy_file(e).map_err(|e| e.to_string())?,
            }
            done += size;
            progress(done, total);
        }
        let mut sub = |d: u64, _t: u64| progress(done + d, total);
        (plan.add)(&mut w, &mut sub)?;
        let buf = w.finish().map_err(|e| e.to_string())?;
        let file = buf.into_inner().map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        drop(za);
        replace(zip, &temp)
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    res
}

/// A file's modified time as the zip's local MS-DOS time (1980 for anything earlier).
fn dos_time(meta: &std::fs::Metadata) -> zip::DateTime {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::Storage::FileSystem::FileTimeToLocalFileTime;
    use windows_sys::Win32::System::Time::FileTimeToSystemTime;
    let t = meta.last_write_time();
    let ft = FILETIME { dwLowDateTime: t as u32, dwHighDateTime: (t >> 32) as u32 };
    let mut local = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    // SAFETY: plain conversions between stack values.
    let st = unsafe {
        let mut st: SYSTEMTIME = std::mem::zeroed();
        (FileTimeToLocalFileTime(&ft, &mut local) != 0 && FileTimeToSystemTime(&local, &mut st) != 0).then_some(st)
    };
    st.and_then(|s| zip::DateTime::from_date_and_time(s.wYear, s.wMonth as u8, s.wDay as u8, s.wHour as u8, s.wMinute as u8, s.wSecond as u8).ok())
        .unwrap_or_default()
}

/// The files and folders below `source` (itself included), as (path, name in the archive below
/// `dir`, is a folder, size). Links are followed as files; nothing is skipped.
fn walk(source: &Path, dir: &str) -> Result<Vec<(PathBuf, String, bool, u64)>, String> {
    let name = source.file_name().ok_or_else(|| format!("{} has no name", source.display()))?.to_string_lossy().into_owned();
    let top = join(dir, &name);
    let mut out = Vec::new();
    let mut stack = vec![(source.to_path_buf(), top)];
    while let Some((p, n)) = stack.pop() {
        let meta = std::fs::metadata(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        if meta.is_dir() {
            out.push((p.clone(), n.clone(), true, 0));
            let mut kids: Vec<_> = std::fs::read_dir(&p).map_err(|e| format!("{}: {e}", p.display()))?.flatten().collect();
            kids.sort_by_key(|k| std::cmp::Reverse(k.file_name()));
            for k in kids {
                stack.push((k.path(), join(&n, &k.file_name().to_string_lossy())));
            }
        } else {
            out.push((p, n, false, meta.len()));
        }
    }
    Ok(out)
}

/// Add `sources` (files and folders on disk) to the zip below `inner` (a folder inside it, ""
/// for its root). `conflicts` is asked once with the files that would replace files already
/// there (archive paths, `\`-separated) and answers Replace, Skip, or None to stop with nothing
/// written. Returns each source's path in the archive (`inner\name`). An item whose name is
/// taken by an item of the other kind (a file where a folder is) is an error.
pub fn add(zip: &Path, inner: &str, sources: &[PathBuf], conflicts: &mut dyn FnMut(&[String]) -> Option<Conflict>, progress: Progress) -> Result<Vec<String>, String> {
    let dir = entry_path(inner);
    let mut za = zip::ZipArchive::new(std::fs::File::open(zip).map_err(|e| e.to_string())?).map_err(|e| format!("not a readable zip archive: {e}"))?;
    let (files, dirs) = names(&mut za);
    drop(za);
    if !dir.is_empty() && !dirs.contains(&norm(&dir)) {
        return Err(format!("{} has no folder {inner}", zip.display()));
    }
    let mut items = Vec::new();
    let mut tops = Vec::new();
    for s in sources {
        let w = walk(s, &dir)?;
        tops.push(w[0].1.replace('/', "\\"));
        items.extend(w);
    }
    let mut clash = Vec::new();
    for (_, n, is_dir, _) in &items {
        let k = norm(n);
        if *is_dir && files.contains(&k) || !*is_dir && dirs.contains(&k) {
            return Err(format!("{} is already there as a {}", n.replace('/', "\\"), if *is_dir { "file" } else { "folder" }));
        }
        if !*is_dir && files.contains(&k) {
            clash.push(n.replace('/', "\\"));
        }
    }
    let decision = if clash.is_empty() { Conflict::Replace } else { conflicts(&clash).ok_or_else(|| "cancelled".to_string())? };
    let clashing: HashSet<String> = clash.iter().map(|c| norm(c)).collect();
    // Replace: the old entries go; Skip: the new files that clash stay out.
    let skip_old = |n: &str| decision == Conflict::Replace && clashing.contains(n);
    let new_items: Vec<_> = items
        .into_iter()
        .filter(|(_, n, d, _)| !(decision == Conflict::Skip && !*d && clashing.contains(&norm(n))) && !(*d && dirs.contains(&norm(n))))
        .collect();
    let add_bytes = new_items.iter().map(|i| i.3).sum();
    let mut add_fn = |w: &mut zip::ZipWriter<std::io::BufWriter<std::fs::File>>, progress: Progress| -> Result<(), String> {
        let mut done = 0u64;
        for (p, n, is_dir, size) in &new_items {
            let meta = std::fs::metadata(p).map_err(|e| format!("{}: {e}", p.display()))?;
            let opts = SimpleFileOptions::default().last_modified_time(dos_time(&meta));
            if *is_dir {
                w.add_directory(format!("{n}/"), opts).map_err(|e| e.to_string())?;
                continue;
            }
            let opts = opts.compression_method(zip::CompressionMethod::Deflated).large_file(*size >= u32::MAX as u64);
            w.start_file(n.as_str(), opts).map_err(|e| e.to_string())?;
            let mut f = std::fs::File::open(p).map_err(|e| format!("{}: {e}", p.display()))?;
            let mut buf = vec![0u8; 256 * 1024];
            loop {
                let k = f.read(&mut buf).map_err(|e| format!("{}: {e}", p.display()))?;
                if k == 0 {
                    break;
                }
                w.write_all(&buf[..k]).map_err(|e| e.to_string())?;
                done += k as u64;
                progress(done, add_bytes);
            }
        }
        Ok(())
    };
    rewrite(zip, Plan { skip: &skip_old, rename: None, add: &mut add_fn, add_bytes }, progress)?;
    Ok(tops)
}

/// The name a new item gets in folder `dir` of the archive: `base`, else Explorer's numbering
/// ("New folder (2)", "New Text Document (2).txt").
fn free_name(taken: &HashSet<String>, dir: &str, base: &str) -> String {
    let (stem, ext) = match base.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
        _ => (base, String::new()),
    };
    (1..)
        .map(|k| if k == 1 { base.to_string() } else { format!("{stem} ({k}){ext}") })
        .find(|n| !taken.contains(&norm(&join(dir, n))))
        .expect("an unbounded range finds a free name")
}

/// A new empty folder or file named `name` (numbered when taken) in folder `inner` of the zip.
/// Returns the name it got.
pub fn new_item(zip: &Path, inner: &str, name: &str, folder: bool, progress: Progress) -> Result<String, String> {
    let dir = entry_path(inner);
    let mut za = zip::ZipArchive::new(std::fs::File::open(zip).map_err(|e| e.to_string())?).map_err(|e| format!("not a readable zip archive: {e}"))?;
    let (files, dirs) = names(&mut za);
    drop(za);
    if !dir.is_empty() && !dirs.contains(&norm(&dir)) {
        return Err(format!("{} has no folder {inner}", zip.display()));
    }
    let taken: HashSet<String> = files.union(&dirs).cloned().collect();
    let got = free_name(&taken, &dir, name);
    let full = join(&dir, &got);
    let now = zip::DateTime::default_for_write();
    let mut add_fn = |w: &mut zip::ZipWriter<std::io::BufWriter<std::fs::File>>, _: Progress| -> Result<(), String> {
        let opts = SimpleFileOptions::default().last_modified_time(now);
        if folder {
            w.add_directory(format!("{full}/"), opts).map_err(|e| e.to_string())
        } else {
            w.start_file(full.as_str(), opts.compression_method(zip::CompressionMethod::Stored)).map_err(|e| e.to_string())
        }
    };
    rewrite(zip, Plan { skip: &|_| false, rename: None, add: &mut add_fn, add_bytes: 0 }, progress)?;
    Ok(got)
}

/// Rename the item at `inner` (a file, or a folder with everything below it) to `new_name` in
/// its folder. Returns the item's new path in the archive.
pub fn rename(zip: &Path, inner: &str, new_name: &str, progress: Progress) -> Result<String, String> {
    let old = entry_path(inner);
    let mut za = zip::ZipArchive::new(std::fs::File::open(zip).map_err(|e| e.to_string())?).map_err(|e| format!("not a readable zip archive: {e}"))?;
    let (files, dirs) = names(&mut za);
    drop(za);
    let key = norm(&old);
    if !files.contains(&key) && !dirs.contains(&key) {
        return Err(format!("{inner} is no longer in the archive"));
    }
    let parent = old.rsplit_once('/').map_or("", |(p, _)| p);
    let new = join(parent, new_name);
    let nkey = norm(&new);
    if nkey != key && (files.contains(&nkey) || dirs.contains(&nkey)) {
        return Err(format!("{new_name} is already there"));
    }
    let mut none = |_: &mut zip::ZipWriter<std::io::BufWriter<std::fs::File>>, _: Progress| Ok(());
    rewrite(zip, Plan { skip: &|_| false, rename: Some((key, new.clone())), add: &mut none, add_bytes: 0 }, progress)?;
    Ok(new.replace('/', "\\"))
}

/// Delete the items at `inners` (files, or folders with everything below them). Returns how many
/// entries went; an item that is not there is an error.
pub fn delete(zip: &Path, inners: &[String], progress: Progress) -> Result<usize, String> {
    let keys: Vec<String> = inners.iter().map(|i| norm(&entry_path(i))).collect();
    let mut za = zip::ZipArchive::new(std::fs::File::open(zip).map_err(|e| e.to_string())?).map_err(|e| format!("not a readable zip archive: {e}"))?;
    let (files, dirs) = names(&mut za);
    let gone = |n: &str| keys.iter().any(|k| n == k || n.strip_prefix(k.as_str()).is_some_and(|r| r.starts_with('/')));
    let count = (0..za.len()).filter(|&i| za.by_index_raw(i).is_ok_and(|e| gone(&norm(&ename(&e))))).count();
    drop(za);
    if let Some((i, _)) = inners.iter().zip(&keys).find(|(_, k)| !files.contains(*k) && !dirs.contains(*k)) {
        return Err(format!("{i} is no longer in the archive"));
    }
    let mut none = |_: &mut zip::ZipWriter<std::io::BufWriter<std::fs::File>>, _: Progress| Ok(());
    rewrite(zip, Plan { skip: &gone, rename: None, add: &mut none, add_bytes: 0 }, progress)?;
    Ok(count)
}

/// The formats "Compress to" makes besides ZIP (which the shell makes, P7-M2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    SevenZ,
    Tar,
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Format::SevenZ => "7z",
            Format::Tar => "tar",
        }
    }
}

/// The path of a new archive in `dir` named after `base` (the first item's name, its extension
/// dropped for a file, as Explorer names it), numbered when taken: `a.7z`, `a (2).7z`.
pub fn archive_path(dir: &Path, base: &str, is_file: bool, fmt: Format, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let stem = if is_file { base.rsplit_once('.').filter(|(s, _)| !s.is_empty()).map_or(base, |(s, _)| s) } else { base };
    (1..)
        .map(|k| dir.join(if k == 1 { format!("{stem}.{}", fmt.ext()) } else { format!("{stem} ({k}).{}", fmt.ext()) }))
        .find(|p| !exists(p))
        .expect("an unbounded range finds a free name")
}

/// A reader that reports the bytes read through it.
struct Counted<'a, R> {
    inner: R,
    done: &'a mut u64,
    total: u64,
    progress: &'a mut dyn FnMut(u64, u64),
}

impl<R: Read> Read for Counted<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        *self.done += n as u64;
        (self.progress)(*self.done, self.total);
        Ok(n)
    }
}

/// A pax extended header carrying `path` (UTF-8 by the standard), for a name the ustar header
/// cannot carry as is (not ASCII); Windows' reader shows such names right only this way.
fn pax_path(b: &mut tar::Builder<std::io::BufWriter<std::fs::File>>, name: &str) -> std::io::Result<()> {
    let body = format!(" path={name}\n");
    // The record's length counts its own digits.
    let mut len = body.len() + 1;
    while format!("{len}{body}").len() != len {
        len += 1;
    }
    let record = format!("{len}{body}");
    let mut h = tar::Header::new_ustar();
    h.set_entry_type(tar::EntryType::XHeader);
    h.set_path("PaxHeader")?;
    h.set_size(record.len() as u64);
    h.set_mode(0o644);
    h.set_cksum();
    b.append(&h, record.as_bytes())
}

/// Make the archive `dest` of `sources` (files and folders, each at the archive's root with its
/// own name) in `fmt`, through a temporary file renamed into place when complete. Returns the
/// number of files written.
pub fn compress(dest: &Path, sources: &[PathBuf], fmt: Format, progress: Progress) -> Result<usize, String> {
    let mut items = Vec::new();
    for s in sources {
        items.extend(walk(s, "")?);
    }
    let total: u64 = items.iter().map(|i| i.3).sum();
    let temp = temp_for(dest);
    let res = (|| -> Result<usize, String> {
        let mut done = 0u64;
        let mut files = 0;
        match fmt {
            Format::SevenZ => {
                let mut w = sevenz_rust2::ArchiveWriter::create(&temp).map_err(|e| e.to_string())?;
                for (p, n, is_dir, size) in &items {
                    let entry = sevenz_rust2::ArchiveEntry::from_path(p, n.clone());
                    if *is_dir || *size == 0 {
                        w.push_archive_entry::<std::fs::File>(entry, None).map_err(|e| e.to_string())?;
                    } else {
                        let f = std::fs::File::open(p).map_err(|e| format!("{}: {e}", p.display()))?;
                        let r = Counted { inner: std::io::BufReader::new(f), done: &mut done, total, progress: &mut *progress };
                        w.push_archive_entry(entry, Some(r)).map_err(|e| e.to_string())?;
                    }
                    files += usize::from(!*is_dir);
                }
                w.finish().map_err(|e| e.to_string())?;
            }
            Format::Tar => {
                let out = std::fs::File::create(&temp).map_err(|e| e.to_string())?;
                let mut b = tar::Builder::new(std::io::BufWriter::new(out));
                b.follow_symlinks(true);
                for (p, n, is_dir, _) in &items {
                    if !n.is_ascii() {
                        let shown = if *is_dir { format!("{n}/") } else { n.clone() };
                        pax_path(&mut b, &shown).map_err(|e| e.to_string())?;
                    }
                    if *is_dir {
                        b.append_dir(n, p).map_err(|e| format!("{}: {e}", p.display()))?;
                    } else {
                        let f = std::fs::File::open(p).map_err(|e| format!("{}: {e}", p.display()))?;
                        let meta = f.metadata().map_err(|e| e.to_string())?;
                        let mut h = tar::Header::new_gnu();
                        h.set_metadata(&meta);
                        h.set_size(meta.len());
                        let r = Counted { inner: std::io::BufReader::new(f), done: &mut done, total, progress: &mut *progress };
                        b.append_data(&mut h, n, r).map_err(|e| format!("{}: {e}", p.display()))?;
                    }
                    files += usize::from(!*is_dir);
                }
                let w = b.into_inner().map_err(|e| e.to_string())?;
                let f = w.into_inner().map_err(|e| e.to_string())?;
                f.sync_all().map_err(|e| e.to_string())?;
            }
        }
        std::fs::rename(&temp, dest).map_err(|e| format!("naming the archive failed: {e}"))?;
        Ok(files)
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every entry of the zip: (name as stored, contents; None for a folder).
    fn entries(zip: &Path) -> Vec<(String, Option<Vec<u8>>)> {
        let mut za = zip::ZipArchive::new(std::fs::File::open(zip).unwrap()).unwrap();
        let mut out: Vec<(String, Option<Vec<u8>>)> = (0..za.len())
            .map(|i| {
                let mut e = za.by_index(i).unwrap();
                let name = e.name().unwrap().into_owned();
                if e.is_dir() {
                    (name, None)
                } else {
                    let mut v = Vec::new();
                    e.read_to_end(&mut v).unwrap();
                    (name, Some(v))
                }
            })
            .collect();
        out.sort();
        out
    }

    fn names_of(zip: &Path) -> Vec<String> {
        entries(zip).into_iter().map(|(n, _)| n).collect()
    }

    fn make(zip: &Path, files: &[(&str, &[u8])]) {
        let mut w = zip::ZipWriter::new(std::fs::File::create(zip).unwrap());
        for (n, d) in files {
            if n.ends_with('/') {
                w.add_directory(*n, SimpleFileOptions::default()).unwrap();
            } else {
                w.start_file(*n, SimpleFileOptions::default()).unwrap();
                w.write_all(d).unwrap();
            }
        }
        w.finish().unwrap();
    }

    fn quiet() -> impl FnMut(u64, u64) {
        |_, _| {}
    }

    /// Adds, conflicts, new items, renames and deletes on real zips in a scratch folder, each read
    /// back; a failed edit leaves the archive byte for byte and no temporary file.
    #[test]
    fn zip_edits_read_back() {
        let dir = std::env::temp_dir().join(format!("twinfm-p7m3-zipw-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src").join("folder").join("deep")).unwrap();
        std::fs::write(dir.join("src").join("a.txt"), b"new a").unwrap();
        std::fs::write(dir.join("src").join("folder").join("one.txt"), b"one").unwrap();
        std::fs::write(dir.join("src").join("folder").join("deep").join("two.bin"), vec![7u8; 100_000]).unwrap();
        std::fs::write(dir.join("src").join("日本語.txt"), b"utf8").unwrap();
        let z = dir.join("t.zip");
        make(&z, &[("a.txt", b"old a"), ("sub/", b""), ("sub/b.txt", b"bb"), ("implied/c.txt", b"cc")]);

        // Into the root: a file that clashes, a folder, a non-ASCII name.
        let mut asked = Vec::new();
        let mut progress_seen = 0u64;
        let srcs = [dir.join("src").join("a.txt"), dir.join("src").join("folder"), dir.join("src").join("日本語.txt")];
        let got = add(&z, "", &srcs, &mut |c| {
            asked = c.to_vec();
            Some(Conflict::Skip)
        }, &mut |d, _| progress_seen = d)
        .unwrap();
        assert_eq!(got, vec!["a.txt".to_string(), "folder".into(), "日本語.txt".into()]);
        assert_eq!(asked, vec!["a.txt".to_string()], "one file clashes");
        assert!(progress_seen >= 100_000, "progress reached the bytes added: {progress_seen}");
        let e = entries(&z);
        assert_eq!(e.iter().find(|(n, _)| n == "a.txt").unwrap().1.as_deref(), Some(&b"old a"[..]), "skipped: the old file stays");
        assert_eq!(e.iter().find(|(n, _)| n == "folder/deep/two.bin").unwrap().1.as_ref().unwrap().len(), 100_000);
        assert!(e.iter().any(|(n, d)| n == "folder/deep/" && d.is_none()), "folders are stored entries");
        assert!(e.iter().any(|(n, _)| n == "日本語.txt"));
        // Replace, into a folder inside: the old entry goes, the new one comes.
        std::fs::write(dir.join("src").join("b.txt"), b"new b").unwrap();
        add(&z, "sub", &[dir.join("src").join("b.txt")], &mut |_| Some(Conflict::Replace), &mut quiet()).unwrap();
        let e = entries(&z);
        assert_eq!(e.iter().filter(|(n, _)| n == "sub/b.txt").count(), 1);
        assert_eq!(e.iter().find(|(n, _)| n == "sub/b.txt").unwrap().1.as_deref(), Some(&b"new b"[..]));
        // Into an implied folder; cancelled at the question: nothing written.
        add(&z, "IMPLIED", &[dir.join("src").join("b.txt")], &mut |_| None, &mut quiet()).unwrap();
        let before = std::fs::read(&z).unwrap();
        let err = add(&z, "sub", &[dir.join("src").join("b.txt")], &mut |_| None, &mut quiet()).unwrap_err();
        assert_eq!(err, "cancelled");
        assert_eq!(std::fs::read(&z).unwrap(), before, "a cancel writes nothing");
        // A file where a folder is; a source that is not there; a folder the zip lacks.
        std::fs::write(dir.join("src").join("sub"), b"x").unwrap();
        assert!(add(&z, "", &[dir.join("src").join("sub")], &mut |_| Some(Conflict::Replace), &mut quiet()).unwrap_err().contains("already there as a folder"));
        assert!(add(&z, "", &[dir.join("src").join("missing.txt")], &mut |_| Some(Conflict::Replace), &mut quiet()).is_err());
        assert!(add(&z, "nope", &[dir.join("src").join("b.txt")], &mut |_| Some(Conflict::Replace), &mut quiet()).is_err());
        assert_eq!(std::fs::read(&z).unwrap(), before, "failed edits leave the archive as it was");
        let leftovers: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".tmp")).collect();
        assert!(leftovers.is_empty(), "no temporary file left: {leftovers:?}");

        // New folder and new file, numbered as Explorer numbers them.
        assert_eq!(new_item(&z, "", "New folder", true, &mut quiet()).unwrap(), "New folder");
        assert_eq!(new_item(&z, "", "New folder", true, &mut quiet()).unwrap(), "New folder (2)");
        assert_eq!(new_item(&z, "sub", "New Text Document.txt", false, &mut quiet()).unwrap(), "New Text Document.txt");
        assert_eq!(new_item(&z, "sub", "New Text Document.txt", false, &mut quiet()).unwrap(), "New Text Document (2).txt");
        let n = names_of(&z);
        assert!(n.contains(&"New folder/".to_string()) && n.contains(&"New folder (2)/".to_string()));
        assert!(n.contains(&"sub/New Text Document (2).txt".to_string()));

        // Renames: a file, a stored folder with everything below it, an implied folder.
        assert_eq!(rename(&z, "a.txt", "A2.txt", &mut quiet()).unwrap(), "A2.txt");
        assert_eq!(rename(&z, r"folder", "Renamed", &mut quiet()).unwrap(), "Renamed");
        assert_eq!(rename(&z, "implied", "imp2", &mut quiet()).unwrap(), "imp2");
        let e = entries(&z);
        let n: Vec<&str> = e.iter().map(|(n, _)| n.as_str()).collect();
        assert!(n.contains(&"A2.txt") && !n.contains(&"a.txt"));
        assert!(n.contains(&"Renamed/") && n.contains(&"Renamed/deep/") && n.contains(&"Renamed/deep/two.bin") && n.contains(&"Renamed/one.txt"));
        assert!(!n.iter().any(|x| x.starts_with("folder")));
        assert!(n.contains(&"imp2/c.txt") && n.contains(&"imp2/b.txt"));
        assert_eq!(e.iter().find(|(n, _)| n == "Renamed/one.txt").unwrap().1.as_deref(), Some(&b"one"[..]), "raw copies keep the data");
        assert!(rename(&z, "A2.txt", "sub", &mut quiet()).unwrap_err().contains("already there"));
        assert!(rename(&z, "gone.txt", "x", &mut quiet()).is_err());

        // Deletes: a file and a folder with everything below it.
        assert_eq!(delete(&z, &["A2.txt".into(), r"Renamed".into()], &mut quiet()).unwrap(), 5);
        let n = names_of(&z);
        assert!(!n.iter().any(|x| x == "A2.txt" || x.starts_with("Renamed")), "{n:?}");
        assert!(delete(&z, &["A2.txt".into()], &mut quiet()).is_err(), "already gone");

        // A zip another tool made (Windows' bsdtar writes Info-ZIP's Unix extra field, 0x7875,
        // which a raw copy keeps): edits read back.
        let tar = Path::new(r"C:\Windows\System32\tar.exe");
        if tar.exists() {
            let bz = dir.join("bsdtar.zip");
            let st = std::process::Command::new(tar).arg("-cf").arg(&bz).args(["--format", "zip", "-C"]).arg(dir.join("src")).args(["a.txt", "folder"]).status().unwrap();
            assert!(st.success());
            add(&bz, "folder", &[dir.join("src").join("b.txt")], &mut |_| Some(Conflict::Replace), &mut quiet()).unwrap();
            rename(&bz, "a.txt", "a3.txt", &mut quiet()).unwrap();
            delete(&bz, &[r"folder\deep".into()], &mut quiet()).unwrap();
            let e = entries(&bz);
            assert_eq!(e.iter().find(|(n, _)| n == "a3.txt").unwrap().1.as_deref(), Some(&b"new a"[..]));
            assert!(e.iter().any(|(n, _)| n == "folder/b.txt") && !e.iter().any(|(n, _)| n.starts_with("folder/deep")), "{e:?}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn new_archive_names() {
        let dir = Path::new(r"C:\x");
        let none = |_: &Path| false;
        assert_eq!(archive_path(dir, "report.docx", true, Format::SevenZ, none), dir.join("report.7z"));
        assert_eq!(archive_path(dir, "photos.2024", false, Format::Tar, none), dir.join("photos.2024.tar"), "a folder keeps its whole name");
        assert_eq!(archive_path(dir, ".hidden", true, Format::Tar, none), dir.join(".hidden.tar"));
        let taken = |p: &Path| p.file_name().is_some_and(|n| n == "a.7z" || n == "a (2).7z");
        assert_eq!(archive_path(dir, "a.txt", true, Format::SevenZ, taken), dir.join("a (3).7z"));
        let mut t = HashSet::new();
        t.insert("new folder".to_string());
        t.insert("sub/new text document.txt".to_string());
        assert_eq!(free_name(&t, "", "New folder"), "New folder (2)");
        assert_eq!(free_name(&t, "sub", "New Text Document.txt"), "New Text Document (2).txt");
        assert_eq!(free_name(&t, "", "New Text Document.txt"), "New Text Document.txt");
    }

    /// 7z and TAR of a tree with non-ASCII names, read back by their libraries and listed by
    /// Windows' own `ArchiveFolder` with the names right.
    #[test]
    fn compress_seven_z_and_tar() {
        let dir = std::env::temp_dir().join(format!("twinfm-p7m3-pack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let src = dir.join("répertoire");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("日本語.txt"), b"nihongo").unwrap();
        std::fs::write(src.join("sub").join("big.bin"), vec![3u8; 300_000]).unwrap();
        std::fs::write(src.join("empty.txt"), b"").unwrap();
        std::fs::write(dir.join("loose.txt"), b"loose").unwrap();
        let sources = [src.clone(), dir.join("loose.txt")];
        for fmt in [Format::SevenZ, Format::Tar] {
            let dest = archive_path(&dir, "répertoire", false, fmt, |p| p.exists());
            let mut last = (0, 0);
            let n = compress(&dest, &sources, fmt, &mut |d, t| last = (d, t)).unwrap();
            assert_eq!(n, 4, "{fmt:?}: four files");
            assert_eq!(last, (300_012, 300_012), "{fmt:?}: progress reached the total");
            let mut got: Vec<(String, usize)> = Vec::new();
            match fmt {
                Format::SevenZ => {
                    let mut r = sevenz_rust2::ArchiveReader::open(&dest, sevenz_rust2::Password::empty()).unwrap();
                    r.for_each_entries(|e, rd| {
                        let mut v = Vec::new();
                        rd.read_to_end(&mut v).unwrap();
                        got.push((format!("{}{}", e.name(), if e.is_directory() { "/" } else { "" }), v.len()));
                        Ok(true)
                    })
                    .unwrap();
                }
                Format::Tar => {
                    let mut a = tar::Archive::new(std::fs::File::open(&dest).unwrap());
                    for e in a.entries().unwrap() {
                        let mut e = e.unwrap();
                        let name = e.path().unwrap().to_string_lossy().replace('\\', "/");
                        let mut v = Vec::new();
                        e.read_to_end(&mut v).unwrap();
                        let dir = e.header().entry_type().is_dir();
                        got.push((format!("{}{}", name.trim_end_matches('/'), if dir { "/" } else { "" }), v.len()));
                    }
                }
            }
            got.sort();
            assert_eq!(
                got,
                vec![
                    ("loose.txt".to_string(), 5),
                    ("répertoire/".into(), 0),
                    ("répertoire/empty.txt".into(), 0),
                    ("répertoire/sub/".into(), 0),
                    ("répertoire/sub/big.bin".into(), 300_000),
                    ("répertoire/日本語.txt".into(), 7),
                ],
                "{fmt:?}"
            );
            // Windows' own reader shows the same names (where Windows has `ArchiveFolder` for it).
            if !crate::fs::archive::is_archive_name(&dest.to_string_lossy()) {
                eprintln!("ArchiveFolder does not take .{} here: not listed through it", fmt.ext());
                continue;
            }
            let (raw, _) = crate::fs::archivefolder::level(&dest.join("répertoire")).unwrap();
            let mut shown: Vec<(String, bool, u64)> = raw.into_iter().map(|r| (r.name, r.dir, r.size)).collect();
            shown.sort();
            assert_eq!(shown, vec![("empty.txt".to_string(), false, 0), ("sub".into(), true, 0), ("日本語.txt".into(), false, 7)], "{fmt:?} through ArchiveFolder");
        }
        assert!(std::fs::read_dir(&dir).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().ends_with(".tmp")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
