//! TwinFM's own archive writers (P7-M3, Jeremy's choice over the shell's zip folder): edits of a
//! zip (add files and folders, a new folder or empty file, rename, delete, and since P8-L2 entries
//! copied from another zip or within one) and new 7z and TAR archives for "Compress to". Run on the
//! file operation worker as jobs.
//!
//! A zip edit never writes the archive in place: it writes the whole archive again into a
//! temporary file beside it (`<name>.~twinfm<pid>.tmp`), the existing entries copied as bytes
//! (`fs::zipraw` since P8-L2: every flag and field kept, so an encrypted entry stays readable), the
//! new ones deflated by the `zip` crate, then swaps it in with `ReplaceFileW` (which keeps the
//! original's attributes and security). A failure at any point removes the temporary file and
//! leaves the archive as it was. Explorer's zip folder works the same way (`x.zip~RF....TMP`,
//! P7-M3's check).
//!
//! A password-protected zip (P8-L2, Jeremy's choices): its entries are copied as they are, and
//! new files get the zip's own encryption (`Key`: ZipCrypto, or AES-256 for a zip of AES entries)
//! with a password the job asks for and `check_password` tests against an existing entry first.
//!
//! Paths inside a zip are given as TwinFM shows them: `\`-separated below the archive's root,
//! matched without regard to case; entry names are written with `/`.

use super::zipraw::{self, Directory};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use zip::write::{FileOptions, SimpleFileOptions};

/// What to do with files an add would put where files already are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conflict {
    Replace,
    Skip,
}

/// Progress: (bytes done, bytes in all).
pub type Progress<'a> = &'a mut dyn FnMut(u64, u64);

/// The crate's writer, appending new entries after the copied ones.
type Zw<'a> = zip::ZipWriter<&'a mut std::io::BufWriter<std::fs::File>>;

/// An entry name as compared: `/`-separated, no trailing `/`, lowercase.
fn norm(name: &str) -> String {
    name.replace('\\', "/").trim_matches('/').to_lowercase()
}

/// The 128 characters of code page 437 above ASCII: a zip names an entry in it unless its UTF-8
/// flag is set (the `zip` crate and Windows' zip folder read it so too).
const CP437_HIGH: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■\u{a0}";

/// An entry's name: UTF-8 when its flag says so, else Info-ZIP's Unicode path field when it has
/// one, else code page 437.
fn ename(e: &zipraw::Entry) -> String {
    if e.flags & zipraw::FLAG_UTF8 != 0 {
        return String::from_utf8_lossy(&e.name).into_owned();
    }
    if let Some(n) = e.unicode_path() {
        return n;
    }
    let high: Vec<char> = CP437_HIGH.chars().collect();
    e.name.iter().map(|&b| if b < 0x80 { b as char } else { high[(b - 0x80) as usize] }).collect()
}

/// A TwinFM inner path (`a\b`) as an entry name prefix (`a/b`), "" for the root.
fn entry_path(inner: &str) -> String {
    inner.replace('\\', "/").trim_matches('/').to_string()
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() { name.to_string() } else { format!("{dir}/{name}") }
}

fn open_dir(zip: &Path) -> Result<(std::fs::File, Directory), String> {
    let mut f = std::fs::File::open(zip).map_err(|e| e.to_string())?;
    let d = zipraw::read(&mut f)?;
    Ok((f, d))
}

/// Every name the archive holds, files and folders (stored or implied by a path), normalised.
fn names(dir: &Directory) -> (HashSet<String>, HashSet<String>) {
    let (mut files, mut dirs) = (HashSet::new(), HashSet::new());
    for e in &dir.entries {
        let n = norm(&ename(e));
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

/// An item and everything below it, by normalised key: the key itself or a path under it.
fn under(n: &str, key: &str) -> bool {
    n == key || n.strip_prefix(key).is_some_and(|r| r.starts_with('/'))
}

/// The name an entry gets when the item at `old` (a normalised key) is now `new` (spelled as it is
/// to be written): `new`, then the entry's own spelling of the components below `old`, and the
/// folder's trailing `/`.
fn moved_name(e: &zipraw::Entry, old: &str, new: &str) -> String {
    let raw = ename(e).replace('\\', "/");
    let parts: Vec<&str> = raw.split('/').filter(|c| !c.is_empty()).collect();
    let skip = if old.is_empty() { 0 } else { old.split('/').count() };
    let below = &parts[skip.min(parts.len())..];
    let mut name = std::iter::once(new).filter(|s| !s.is_empty()).chain(below.iter().copied()).collect::<Vec<_>>().join("/");
    if e.is_dir() {
        name.push('/');
    }
    name
}

/// New files' encryption in a password-protected zip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    None,
    ZipCrypto(String),
    Aes256(String),
}

impl Key {
    fn apply<'k>(&'k self, opts: FileOptions<'k, 'static, ()>) -> Result<FileOptions<'k, 'static, ()>, String> {
        use zip::unstable::write::FileOptionsExt;
        match self {
            Key::None => Ok(opts),
            Key::ZipCrypto(p) => opts.with_deprecated_encryption(p.as_bytes()).map_err(|e| e.to_string()),
            Key::Aes256(p) => Ok(opts.with_aes_encryption(zip::AesMode::Aes256, p)),
        }
    }

    fn encrypts(&self) -> bool {
        *self != Key::None
    }
}

/// How a zip's entries are protected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Protection {
    None,
    ZipCrypto,
    /// WinZip AES, of this strength (1: 128, 2: 192, 3: 256 bits).
    Aes(u8),
    /// An encryption TwinFM does not write beside (PKWARE's strong encryption).
    Unsupported(String),
}

impl Protection {
    /// The key new files get with `password`.
    pub fn key(&self, password: &str) -> Key {
        match self {
            Protection::ZipCrypto => Key::ZipCrypto(password.to_string()),
            Protection::Aes(_) => Key::Aes256(password.to_string()),
            Protection::None | Protection::Unsupported(_) => Key::None,
        }
    }
}

/// The zip's protection, from its first encrypted file (P8-L2; the P7-M3 refusal is gone).
pub fn protection(zip: &Path) -> Result<Protection, String> {
    let (_, dir) = open_dir(zip)?;
    let Some(e) = dir.entries.iter().find(|e| e.encrypted() && !e.is_dir()) else { return Ok(Protection::None) };
    Ok(if e.flags & zipraw::FLAG_STRONG != 0 {
        Protection::Unsupported("PKWARE strong encryption".into())
    } else if let Some(s) = e.aes_strength() {
        Protection::Aes(s)
    } else {
        Protection::ZipCrypto
    })
}

/// Whether `password` opens the zip's smallest encrypted file: read whole, so its checksum (a
/// ZipCrypto entry's one-byte check passes one wrong password in 256) or its AES authentication
/// code decides.
pub fn check_password(zip: &Path, password: &str) -> Result<bool, String> {
    let (_, dir) = open_dir(zip)?;
    let Some(i) = (0..dir.entries.len()).filter(|&i| dir.entries[i].encrypted() && !dir.entries[i].is_dir()).min_by_key(|&i| dir.entries[i].csize) else {
        return Ok(true);
    };
    let mut za = zip::ZipArchive::new(std::fs::File::open(zip).map_err(|e| e.to_string())?).map_err(|e| format!("not a readable zip archive: {e}"))?;
    if za.by_index_raw(i).map_err(|e| e.to_string())?.name_raw() != dir.entries[i].name.as_slice() {
        return Err("the zip's entries are not where TwinFM read them".into());
    }
    let res = match za.by_index_decrypt(i, password.as_bytes()) {
        Err(zip::result::ZipError::InvalidPassword) => Ok(false),
        Err(e) => Err(e.to_string()),
        Ok(mut f) => Ok(std::io::copy(&mut f, &mut std::io::sink()).is_ok()),
    };
    res
}

/// What a rewrite changes: entries left out, items renamed (old key to new spelling), entries
/// copied in after the kept ones, and what the crate adds after those.
struct Plan<'a> {
    skip: &'a dyn Fn(&str) -> bool,
    rename: Vec<(String, String)>,
    /// From another zip, or this one again: (its path, the entry's index in its directory, the
    /// entry's new name).
    copies: Vec<(PathBuf, usize, String)>,
    add: &'a mut dyn FnMut(&mut Zw<'_>, Progress) -> Result<(), String>,
    /// Bytes the additions will write, for the progress total.
    add_bytes: u64,
}

fn no_additions(_: &mut Zw<'_>, _: Progress) -> Result<(), String> {
    Ok(())
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
        let (mut src, dir) = open_dir(zip)?;
        let mut kept: Vec<(usize, Option<String>)> = Vec::new();
        for (i, e) in dir.entries.iter().enumerate() {
            let n = norm(&ename(e));
            if (plan.skip)(&n) {
                continue;
            }
            let name = plan.rename.iter().find(|(old, _)| under(&n, old)).map(|(old, new)| moved_name(e, old, new));
            kept.push((i, name));
        }
        // The other zips copied from, each opened once.
        let mut others: Vec<(PathBuf, std::fs::File, Directory)> = Vec::new();
        for (p, _, _) in &plan.copies {
            if !others.iter().any(|(q, _, _)| q == p) {
                let (f, d) = open_dir(p)?;
                others.push((p.clone(), f, d));
            }
        }
        let size_of = |p: &Path, i: usize| others.iter().find(|(q, _, _)| q == p).map_or(0, |(_, _, d)| d.entries[i].csize);
        let total = kept.iter().map(|(i, _)| dir.entries[*i].csize).sum::<u64>() + plan.copies.iter().map(|(p, i, _)| size_of(p, *i)).sum::<u64>() + plan.add_bytes;
        let mut out = zipraw::Out::create(&temp)?;
        let mut done = 0u64;
        for (i, name) in &kept {
            let e = &dir.entries[*i];
            out.copy(&mut src, e, name.as_deref(), &mut |d| progress(done + d, total))?;
            done += e.csize;
        }
        for (p, i, name) in &plan.copies {
            let (_, f, d) = others.iter_mut().find(|(q, _, _)| q == p).expect("opened above");
            let e = &d.entries[*i];
            out.copy(f, e, Some(name), &mut |x| progress(done + x, total))?;
            done += e.csize;
        }
        {
            let mut w = zip::ZipWriter::new(out.writer());
            let mut sub = |d: u64, _t: u64| progress(done + d, total);
            (plan.add)(&mut w, &mut sub)?;
            w.finish().map_err(|e| e.to_string())?;
        }
        out.adopt()?;
        let file = out.finish(&dir.comment)?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        drop(src);
        drop(others);
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

/// The clashes of `items` (archive names, is a folder) with what the zip holds: an error for an
/// item of the other kind where one is, else the files that would replace files there.
fn clashes<'a>(items: impl Iterator<Item = (&'a str, bool)>, files: &HashSet<String>, dirs: &HashSet<String>) -> Result<Vec<String>, String> {
    let mut clash = Vec::new();
    for (n, is_dir) in items {
        let k = norm(n);
        if is_dir && files.contains(&k) || !is_dir && dirs.contains(&k) {
            return Err(format!("{} is already there as a {}", n.trim_end_matches('/').replace('/', "\\"), if is_dir { "file" } else { "folder" }));
        }
        if !is_dir && files.contains(&k) {
            clash.push(n.replace('/', "\\"));
        }
    }
    Ok(clash)
}

/// Add `sources` (files and folders on disk) to the zip below `inner` (a folder inside it, ""
/// for its root), new files encrypted with `key`. `conflicts` is asked once with the files that
/// would replace files already there (archive paths, `\`-separated) and answers Replace, Skip, or
/// None to stop with nothing written. Returns each source's path in the archive (`inner\name`).
/// An item whose name is taken by an item of the other kind (a file where a folder is) is an error.
pub fn add(zip: &Path, inner: &str, sources: &[PathBuf], key: &Key, conflicts: &mut dyn FnMut(&[String]) -> Option<Conflict>, progress: Progress) -> Result<Vec<String>, String> {
    let dir = entry_path(inner);
    let (files, dirs) = names(&open_dir(zip)?.1);
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
    let clash = clashes(items.iter().map(|(_, n, d, _)| (n.as_str(), *d)), &files, &dirs)?;
    let decision = if clash.is_empty() { Conflict::Replace } else { conflicts(&clash).ok_or_else(|| "cancelled".to_string())? };
    let clashing: HashSet<String> = clash.iter().map(|c| norm(c)).collect();
    // Replace: the old entries go; Skip: the new files that clash stay out.
    let skip_old = |n: &str| decision == Conflict::Replace && clashing.contains(n);
    let new_items: Vec<_> = items
        .into_iter()
        .filter(|(_, n, d, _)| !(decision == Conflict::Skip && !*d && clashing.contains(&norm(n))) && !(*d && dirs.contains(&norm(n))))
        .collect();
    let add_bytes = new_items.iter().map(|i| i.3).sum();
    let mut add_fn = |w: &mut Zw<'_>, progress: Progress| -> Result<(), String> {
        let mut done = 0u64;
        for (p, n, is_dir, size) in &new_items {
            let meta = std::fs::metadata(p).map_err(|e| format!("{}: {e}", p.display()))?;
            let opts = SimpleFileOptions::default().last_modified_time(dos_time(&meta));
            if *is_dir {
                w.add_directory(format!("{n}/"), opts).map_err(|e| e.to_string())?;
                continue;
            }
            let opts = key.apply(opts.compression_method(zip::CompressionMethod::Deflated).large_file(*size >= u32::MAX as u64))?;
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
    rewrite(zip, Plan { skip: &skip_old, rename: Vec::new(), copies: Vec::new(), add: &mut add_fn, add_bytes }, progress)?;
    Ok(tops)
}

/// The name a copy gets in its own folder (P8-L2, Explorer's): "a - Copy.txt", then
/// "a - Copy (2).txt".
fn copy_name(taken: &HashSet<String>, dir: &str, base: &str) -> String {
    let (stem, ext) = match base.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
        _ => (base, String::new()),
    };
    (1..)
        .map(|k| if k == 1 { format!("{stem} - Copy{ext}") } else { format!("{stem} - Copy ({k}){ext}") })
        .find(|n| !taken.contains(&norm(&join(dir, n))))
        .expect("an unbounded range finds a free name")
}

/// Copy, or move (`mv`), the items at `inners` (files, or folders with everything below them) of
/// the zip `from` into folder `inner` of `zip` (P8-L2): their entries copied as bytes, an
/// encrypted one keeping its own password, except that a file not encrypted going into a zip whose
/// new files `key` encrypts is read and written again with it. `from` may be `zip` itself: a copy
/// into an item's own folder is named Explorer's way ("a - Copy.txt"), and a move is a rename of
/// the items' paths, in one rewrite. A move between two zips is the copy here; the job removes the
/// items from `from` after it. `conflicts` as for `add`. Returns each item's path in `zip`.
pub fn copy_from_zip(zip: &Path, inner: &str, from: &Path, inners: &[String], mv: bool, key: &Key, conflicts: &mut dyn FnMut(&[String]) -> Option<Conflict>, progress: Progress) -> Result<Vec<String>, String> {
    let same = same_file(zip, from);
    let dest = entry_path(inner);
    let (files, dirs) = names(&open_dir(zip)?.1);
    if !dest.is_empty() && !dirs.contains(&norm(&dest)) {
        return Err(format!("{} has no folder {inner}", zip.display()));
    }
    let (_, src) = open_dir(from)?;
    let (sfiles, sdirs) = names(&src);
    let taken: HashSet<String> = files.union(&dirs).cloned().collect();
    // Each item: its key in `from`, its new spelled path in `zip`.
    let mut moves: Vec<(String, String)> = Vec::new();
    for i in inners {
        let key_from = norm(&entry_path(i));
        if !sfiles.contains(&key_from) && !sdirs.contains(&key_from) {
            return Err(format!("{i} is no longer in the archive"));
        }
        let base = entry_path(i).rsplit('/').next().unwrap_or_default().to_string();
        let parent = key_from.rsplit_once('/').map_or("", |(p, _)| p);
        if same && (norm(&dest) == key_from || under(&norm(&dest), &key_from)) {
            return Err(format!("{base} cannot go into itself"));
        }
        let into_own_folder = same && parent == norm(&dest);
        if into_own_folder && mv {
            return Err("source and destination are the same folder".into());
        }
        let name = if into_own_folder { copy_name(&taken, &dest, &base) } else { base };
        moves.push((key_from, join(&dest, &name)));
    }
    let tops: Vec<String> = moves.iter().map(|(_, n)| n.replace('/', "\\")).collect();
    // What each item's entries become, and the clashes with what the zip holds.
    let picked: Vec<(usize, String)> = src
        .entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let n = norm(&ename(e));
            moves.iter().find(|(old, _)| under(&n, old)).map(|(old, new)| (i, moved_name(e, old, new)))
        })
        .collect();
    // Folders only implied by the paths below them in `from` are implied in `zip` too; the items
    // themselves are stored when they were.
    let clash = clashes(picked.iter().map(|(i, n)| (n.as_str(), src.entries[*i].is_dir())), &files, &dirs)?;
    // A move inside one zip renames whole items: one that would replace a file there is refused
    // rather than merged file by file.
    if same && mv {
        if let Some(c) = clash.first() {
            return Err(format!("{c} is already there"));
        }
    }
    let decision = if clash.is_empty() { Conflict::Replace } else { conflicts(&clash).ok_or_else(|| "cancelled".to_string())? };
    let clashing: HashSet<String> = clash.iter().map(|c| norm(c)).collect();
    if same && mv {
        // The items' paths renamed, in place.
        let rename: Vec<(String, String)> = moves.clone();
        let mut none = no_additions;
        rewrite(zip, Plan { skip: &|_| false, rename, copies: Vec::new(), add: &mut none, add_bytes: 0 }, progress)?;
        return Ok(tops);
    }
    let picked: Vec<(usize, String)> = picked
        .into_iter()
        .filter(|(i, n)| {
            let k = norm(n);
            let d = src.entries[*i].is_dir();
            !(decision == Conflict::Skip && !d && clashing.contains(&k)) && !(d && dirs.contains(&k))
        })
        .collect();
    let skip_old = |n: &str| decision == Conflict::Replace && clashing.contains(n);
    // Files without encryption going into a zip that encrypts its new files are written again.
    let (recode, raw): (Vec<(usize, String)>, Vec<(usize, String)>) =
        picked.into_iter().partition(|(i, _)| key.encrypts() && !src.entries[*i].encrypted() && !src.entries[*i].is_dir());
    let copies = raw.into_iter().map(|(i, n)| (from.to_path_buf(), i, n)).collect();
    let add_bytes = recode.iter().map(|(i, _)| src.entries[*i].usize).sum();
    let mut add_fn = |w: &mut Zw<'_>, progress: Progress| -> Result<(), String> {
        if recode.is_empty() {
            return Ok(());
        }
        let mut za = zip::ZipArchive::new(std::fs::File::open(from).map_err(|e| e.to_string())?).map_err(|e| format!("not a readable zip archive: {e}"))?;
        let mut done = 0u64;
        for (i, n) in &recode {
            let mut f = za.by_index(*i).map_err(|e| e.to_string())?;
            if f.name_raw() != src.entries[*i].name.as_slice() {
                return Err("the zip's entries are not where TwinFM read them".into());
            }
            let mut opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(f.size() >= u32::MAX as u64);
            if let Some(t) = f.last_modified() {
                opts = opts.last_modified_time(t);
            }
            w.start_file(n.as_str(), key.apply(opts)?).map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; 256 * 1024];
            loop {
                let k = f.read(&mut buf).map_err(|e| e.to_string())?;
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
    rewrite(zip, Plan { skip: &skip_old, rename: Vec::new(), copies, add: &mut add_fn, add_bytes }, progress)?;
    Ok(tops)
}

/// Whether the items at `inners` of the zip `from` hold a file without encryption (which a
/// password-protected destination encrypts, so its job asks for the password).
pub fn plain_files_among(from: &Path, inners: &[String]) -> Result<bool, String> {
    let (_, dir) = open_dir(from)?;
    let keys: Vec<String> = inners.iter().map(|i| norm(&entry_path(i))).collect();
    Ok(dir.entries.iter().any(|e| !e.is_dir() && !e.encrypted() && keys.iter().any(|k| under(&norm(&ename(e)), k))))
}

/// The two paths name the same file (case and spelling aside).
fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a.as_os_str().eq_ignore_ascii_case(b.as_os_str()),
    }
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

/// A new empty folder or file named `name` (numbered when taken) in folder `inner` of the zip, a
/// file encrypted with `key`. Returns the name it got.
pub fn new_item(zip: &Path, inner: &str, name: &str, folder: bool, key: &Key, progress: Progress) -> Result<String, String> {
    let dir = entry_path(inner);
    let (files, dirs) = names(&open_dir(zip)?.1);
    if !dir.is_empty() && !dirs.contains(&norm(&dir)) {
        return Err(format!("{} has no folder {inner}", zip.display()));
    }
    let taken: HashSet<String> = files.union(&dirs).cloned().collect();
    let got = free_name(&taken, &dir, name);
    let full = join(&dir, &got);
    let now = zip::DateTime::default_for_write();
    let mut add_fn = |w: &mut Zw<'_>, _: Progress| -> Result<(), String> {
        let opts = SimpleFileOptions::default().last_modified_time(now);
        if folder {
            w.add_directory(format!("{full}/"), opts).map_err(|e| e.to_string())
        } else {
            w.start_file(full.as_str(), key.apply(opts.compression_method(zip::CompressionMethod::Stored))?).map_err(|e| e.to_string())
        }
    };
    rewrite(zip, Plan { skip: &|_| false, rename: Vec::new(), copies: Vec::new(), add: &mut add_fn, add_bytes: 0 }, progress)?;
    Ok(got)
}

/// Rename the item at `inner` (a file, or a folder with everything below it) to `new_name` in
/// its folder. Returns the item's new path in the archive.
pub fn rename(zip: &Path, inner: &str, new_name: &str, progress: Progress) -> Result<String, String> {
    let old = entry_path(inner);
    let (files, dirs) = names(&open_dir(zip)?.1);
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
    let mut none = no_additions;
    rewrite(zip, Plan { skip: &|_| false, rename: vec![(key, new.clone())], copies: Vec::new(), add: &mut none, add_bytes: 0 }, progress)?;
    Ok(new.replace('/', "\\"))
}

/// Delete the items at `inners` (files, or folders with everything below them). Returns how many
/// entries went; an item that is not there is an error.
pub fn delete(zip: &Path, inners: &[String], progress: Progress) -> Result<usize, String> {
    let keys: Vec<String> = inners.iter().map(|i| norm(&entry_path(i))).collect();
    let (_, dir) = open_dir(zip)?;
    let (files, dirs) = names(&dir);
    let gone = |n: &str| keys.iter().any(|k| under(n, k));
    let count = dir.entries.iter().filter(|e| gone(&norm(&ename(e)))).count();
    if let Some((i, _)) = inners.iter().zip(&keys).find(|(_, k)| !files.contains(*k) && !dirs.contains(*k)) {
        return Err(format!("{i} is no longer in the archive"));
    }
    let mut none = no_additions;
    rewrite(zip, Plan { skip: &gone, rename: Vec::new(), copies: Vec::new(), add: &mut none, add_bytes: 0 }, progress)?;
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
        let got = add(&z, "", &srcs, &Key::None, &mut |c| {
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
        add(&z, "sub", &[dir.join("src").join("b.txt")], &Key::None, &mut |_| Some(Conflict::Replace), &mut quiet()).unwrap();
        let e = entries(&z);
        assert_eq!(e.iter().filter(|(n, _)| n == "sub/b.txt").count(), 1);
        assert_eq!(e.iter().find(|(n, _)| n == "sub/b.txt").unwrap().1.as_deref(), Some(&b"new b"[..]));
        // Into an implied folder; cancelled at the question: nothing written.
        add(&z, "IMPLIED", &[dir.join("src").join("b.txt")], &Key::None, &mut |_| None, &mut quiet()).unwrap();
        let before = std::fs::read(&z).unwrap();
        let err = add(&z, "sub", &[dir.join("src").join("b.txt")], &Key::None, &mut |_| None, &mut quiet()).unwrap_err();
        assert_eq!(err, "cancelled");
        assert_eq!(std::fs::read(&z).unwrap(), before, "a cancel writes nothing");
        // A file where a folder is; a source that is not there; a folder the zip lacks.
        std::fs::write(dir.join("src").join("sub"), b"x").unwrap();
        assert!(add(&z, "", &[dir.join("src").join("sub")], &Key::None, &mut |_| Some(Conflict::Replace), &mut quiet()).unwrap_err().contains("already there as a folder"));
        assert!(add(&z, "", &[dir.join("src").join("missing.txt")], &Key::None, &mut |_| Some(Conflict::Replace), &mut quiet()).is_err());
        assert!(add(&z, "nope", &[dir.join("src").join("b.txt")], &Key::None, &mut |_| Some(Conflict::Replace), &mut quiet()).is_err());
        assert_eq!(std::fs::read(&z).unwrap(), before, "failed edits leave the archive as it was");
        let leftovers: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".tmp")).collect();
        assert!(leftovers.is_empty(), "no temporary file left: {leftovers:?}");

        // New folder and new file, numbered as Explorer numbers them.
        assert_eq!(new_item(&z, "", "New folder", true, &Key::None, &mut quiet()).unwrap(), "New folder");
        assert_eq!(new_item(&z, "", "New folder", true, &Key::None, &mut quiet()).unwrap(), "New folder (2)");
        assert_eq!(new_item(&z, "sub", "New Text Document.txt", false, &Key::None, &mut quiet()).unwrap(), "New Text Document.txt");
        assert_eq!(new_item(&z, "sub", "New Text Document.txt", false, &Key::None, &mut quiet()).unwrap(), "New Text Document (2).txt");
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
            add(&bz, "folder", &[dir.join("src").join("b.txt")], &Key::None, &mut |_| Some(Conflict::Replace), &mut quiet()).unwrap();
            rename(&bz, "a.txt", "a3.txt", &mut quiet()).unwrap();
            delete(&bz, &[r"folder\deep".into()], &mut quiet()).unwrap();
            let e = entries(&bz);
            assert_eq!(e.iter().find(|(n, _)| n == "a3.txt").unwrap().1.as_deref(), Some(&b"new a"[..]));
            assert!(e.iter().any(|(n, _)| n == "folder/b.txt") && !e.iter().any(|(n, _)| n.starts_with("folder/deep")), "{e:?}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Password-protected zips (P8-L2): ZipCrypto and AES zips made by the crate are recognised,
    /// the password checked, files added under the zip's own encryption, the old entries copied as
    /// bytes through a rename and a delete, and a plain file copied in from another zip encrypted
    /// on the way while an encrypted one keeps its own password.
    #[test]
    fn password_zips_and_copies_between_zips() {
        use zip::unstable::write::FileOptionsExt;
        let dir = std::env::temp_dir().join(format!("twinfm-p8l2-pw-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("n.txt"), b"new file").unwrap();
        let read = |z: &Path, name: &str, pw: &[u8]| -> (bool, Vec<u8>) {
            let mut za = zip::ZipArchive::new(std::fs::File::open(z).unwrap()).unwrap();
            let mut f = za.by_name_decrypt(name, pw).unwrap();
            let mut v = Vec::new();
            f.read_to_end(&mut v).unwrap();
            (f.encrypted(), v)
        };
        let crypto = dir.join("crypto.zip");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&crypto).unwrap());
        w.start_file("s/secret.txt", SimpleFileOptions::default().with_deprecated_encryption(b"pw").unwrap()).unwrap();
        w.write_all(b"secret").unwrap();
        w.start_file("open.txt", SimpleFileOptions::default()).unwrap();
        w.write_all(b"open").unwrap();
        w.finish().unwrap();
        let aes = dir.join("aes.zip");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&aes).unwrap());
        w.start_file("a.txt", SimpleFileOptions::default().with_aes_encryption(zip::AesMode::Aes128, "pw")).unwrap();
        w.write_all(b"aes").unwrap();
        w.finish().unwrap();
        let plain = dir.join("plain.zip");
        make(&plain, &[("p.txt", b"plain")]);

        assert_eq!(protection(&crypto).unwrap(), Protection::ZipCrypto);
        assert_eq!(protection(&aes).unwrap(), Protection::Aes(1));
        assert_eq!(protection(&plain).unwrap(), Protection::None);
        assert_eq!((check_password(&crypto, "pw").unwrap(), check_password(&crypto, "PW").unwrap()), (true, false));
        assert_eq!((check_password(&aes, "pw").unwrap(), check_password(&aes, "nope").unwrap()), (true, false));
        assert_eq!(Protection::Aes(1).key("k"), Key::Aes256("k".into()), "an AES zip's new files get AES-256");

        // Adds under the zip's own encryption; the old entries keep theirs.
        add(&crypto, "s", &[dir.join("n.txt")], &Protection::ZipCrypto.key("pw"), &mut |_| None, &mut quiet()).unwrap();
        assert_eq!(read(&crypto, "s/n.txt", b"pw"), (true, b"new file".to_vec()));
        add(&aes, "", &[dir.join("n.txt")], &Protection::Aes(1).key("pw"), &mut |_| None, &mut quiet()).unwrap();
        assert_eq!(read(&aes, "n.txt", b"pw"), (true, b"new file".to_vec()));
        assert_eq!(read(&aes, "a.txt", b"pw"), (true, b"aes".to_vec()));
        assert_eq!(new_item(&crypto, "", "e.txt", false, &Protection::ZipCrypto.key("pw"), &mut quiet()).unwrap(), "e.txt");
        assert!(read(&crypto, "e.txt", b"pw").0, "a new empty file is encrypted too");
        // A rename and a delete copy the rest as bytes: still readable with the password.
        rename(&crypto, "s", "t", &mut quiet()).unwrap();
        delete(&crypto, &["open.txt".into()], &mut quiet()).unwrap();
        assert_eq!(read(&crypto, "t/secret.txt", b"pw"), (true, b"secret".to_vec()));
        assert_eq!(read(&crypto, "t/n.txt", b"pw"), (true, b"new file".to_vec()));
        assert!(plain_files_among(&plain, &["p.txt".into()]).unwrap() && !plain_files_among(&crypto, &["t".into()]).unwrap());

        // Between zips: a plain file into the protected zip is encrypted on the way; an encrypted
        // one into the plain zip keeps its own password; within one zip, a copy beside itself.
        let got = copy_from_zip(&crypto, "", &plain, &["p.txt".into()], false, &Protection::ZipCrypto.key("pw"), &mut |_| None, &mut quiet()).unwrap();
        assert_eq!(got, vec!["p.txt".to_string()]);
        assert_eq!(read(&crypto, "p.txt", b"pw"), (true, b"plain".to_vec()));
        copy_from_zip(&plain, "", &crypto, &["t".into()], false, &Key::None, &mut |_| None, &mut quiet()).unwrap();
        assert_eq!(read(&plain, "t/secret.txt", b"pw"), (true, b"secret".to_vec()));
        assert_eq!(copy_from_zip(&plain, "", &plain, &["p.txt".into()], false, &Key::None, &mut |_| None, &mut quiet()).unwrap(), vec!["p - Copy.txt".to_string()]);
        // A clash asks; Skip leaves the old file.
        let mut asked = Vec::new();
        copy_from_zip(&plain, "t", &crypto, &[r"t\secret.txt".into()], false, &Key::None, &mut |c| {
            asked = c.to_vec();
            Some(Conflict::Skip)
        }, &mut quiet())
        .unwrap();
        assert_eq!(asked, vec![r"t\secret.txt".to_string()]);
        // A move within one zip renames; into itself, or onto a file there, it is refused.
        copy_from_zip(&plain, "t", &plain, &["p - Copy.txt".into()], true, &Key::None, &mut |_| None, &mut quiet()).unwrap();
        assert_eq!(read(&plain, "t/p - Copy.txt", b"").1, b"plain".to_vec());
        assert!(copy_from_zip(&plain, "t", &plain, &["t".into()], true, &Key::None, &mut |_| None, &mut quiet()).unwrap_err().contains("into itself"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn code_page_437_has_128_characters() {
        assert_eq!(CP437_HIGH.chars().count(), 128);
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
        crate::fs::archivefolder::remove_scratch(&dir);
    }
}
