//! A folder's entries with their size on disk (P13-M3): one directory handle and
//! `GetFileInformationByHandleEx` with a directory information class, 64 KB of entries per call,
//! each entry carrying its `AllocationSize` (the clusters it holds, not its length), so the size on
//! disk costs no call per file. `FindFirstFileExW` (the listing) does not give it. The sizes view's
//! walker and a level's files read folders through this; the listing itself is unchanged.
//!
//! With `ids`, the class is `FileIdBothDirectoryInfo` and each entry carries its file ID (the same
//! for every name of a hard-linked file on the volume), so a walk can count a file once
//! (`HardLinks`); without, `FileFullDirectoryInfo`.

use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_FILES, GetLastError, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_NO_RECALL,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_FULL_DIR_INFO, FILE_ID_BOTH_DIR_INFO, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_STANDARD_INFO, FileFullDirectoryInfo,
    FileFullDirectoryRestartInfo, FileIdBothDirectoryInfo, FileIdBothDirectoryRestartInfo, FileStandardInfo,
    GetFileInformationByHandleEx, OPEN_EXISTING,
};

/// Bytes of entries asked for per call.
const BUF_BYTES: usize = 64 * 1024;

/// One entry of a folder as the directory information gives it.
pub struct Item<'a> {
    /// The name, UTF-16, not NUL-terminated.
    pub name: &'a [u16],
    pub attrs: u32,
    /// The reparse tag when `attrs` has FILE_ATTRIBUTE_REPARSE_POINT (the information's `EaSize`).
    pub tag: u32,
    /// The length (`EndOfFile`).
    pub logical: u64,
    /// The size on disk (`AllocationSize`).
    pub disk: u64,
    /// Last write, FILETIME ticks.
    pub modified: i64,
    /// The file ID with `ids`, else 0.
    pub id: u64,
}

impl Item<'_> {
    pub fn is_dot(&self) -> bool {
        let dot = b'.' as u16;
        self.name == [dot] || self.name == [dot, dot]
    }
}

/// Call `f` for every entry of `dir` (`.` and `..` included: `Item::is_dot`). Err with the Win32
/// error when the folder cannot be opened or read; an empty folder is Ok.
pub fn read(dir: &Path, ids: bool, mut f: impl FnMut(&Item)) -> Result<(), u32> {
    let wide = super::enumerate::long_wide(dir.as_os_str().encode_wide().collect(), false);
    // SAFETY: `wide` is a NUL-terminated wide path alive for the call; no security attributes or
    // template. FILE_FLAG_BACKUP_SEMANTICS opens a directory; listing rights only.
    let h = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_LIST_DIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        // SAFETY: no arguments; read right after the failing call on this thread.
        return Err(unsafe { GetLastError() });
    }
    // u64s, so every entry (8-byte aligned within the buffer) is aligned for its struct.
    let mut buf = vec![0u64; BUF_BYTES / 8];
    let mut first = true;
    let res = loop {
        let class = match (ids, first) {
            (true, true) => FileIdBothDirectoryRestartInfo,
            (true, false) => FileIdBothDirectoryInfo,
            (false, true) => FileFullDirectoryRestartInfo,
            (false, false) => FileFullDirectoryInfo,
        };
        first = false;
        // SAFETY: `h` is the open directory handle and `buf` a writable buffer of BUF_BYTES.
        let ok = unsafe { GetFileInformationByHandleEx(h, class, buf.as_mut_ptr().cast(), BUF_BYTES as u32) };
        if ok == 0 {
            // SAFETY: as above.
            let code = unsafe { GetLastError() };
            break if code == ERROR_NO_MORE_FILES { Ok(()) } else { Err(code) };
        }
        let base = buf.as_ptr().cast::<u8>();
        let mut off = 0usize;
        loop {
            // SAFETY: the call filled the buffer with a chain of entries of `class`, each starting
            // at an 8-byte aligned offset given by the previous entry's NextEntryOffset, within it;
            // the name's FileNameLength bytes follow FileName's start inside the same entry.
            let (next, item) = unsafe {
                let p = base.add(off);
                if ids {
                    let e = p.cast::<FILE_ID_BOTH_DIR_INFO>();
                    let name = std::slice::from_raw_parts(std::ptr::addr_of!((*e).FileName).cast::<u16>(), (*e).FileNameLength as usize / 2);
                    let e = &*e;
                    (e.NextEntryOffset, item_of(name, e.FileAttributes, e.EaSize, e.EndOfFile, e.AllocationSize, e.LastWriteTime, e.FileId as u64))
                } else {
                    let e = p.cast::<FILE_FULL_DIR_INFO>();
                    let name = std::slice::from_raw_parts(std::ptr::addr_of!((*e).FileName).cast::<u16>(), (*e).FileNameLength as usize / 2);
                    let e = &*e;
                    (e.NextEntryOffset, item_of(name, e.FileAttributes, e.EaSize, e.EndOfFile, e.AllocationSize, e.LastWriteTime, 0))
                }
            };
            f(&item);
            if next == 0 {
                break;
            }
            off += next as usize;
        }
    };
    // SAFETY: `h` is the handle opened above, closed exactly once.
    unsafe { CloseHandle(h) };
    res
}

/// Every entry's size on disk in `dir` by name (the Size on disk column's second read of a listed
/// folder); None when the folder cannot be read.
pub fn disk_by_name(dir: &Path) -> Option<std::collections::HashMap<String, u64>> {
    let mut map = std::collections::HashMap::new();
    read(dir, false, |it| {
        if !it.is_dot() {
            map.insert(String::from_utf16_lossy(it.name), it.disk);
        }
    })
    .ok()?;
    Some(map)
}

/// One file's size on disk, from its own handle (`FileStandardInfo`'s `AllocationSize`, the figure
/// the directory information gives): a file a watch edit statted. Opened for its attributes only,
/// never following a link and never recalling a cloud file. None when it cannot be opened.
pub fn disk_of(path: &Path) -> Option<u64> {
    let wide = super::enumerate::long_wide(path.as_os_str().encode_wide().collect(), true);
    // SAFETY: `wide` is a NUL-terminated wide path alive for the call; no security attributes or
    // template.
    let h = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_OPEN_NO_RECALL,
            std::ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return None;
    }
    let mut info = FILE_STANDARD_INFO::default();
    // SAFETY: `h` is the open handle and `info` a FILE_STANDARD_INFO of the size passed.
    let ok = unsafe {
        GetFileInformationByHandleEx(h, FileStandardInfo, (&mut info as *mut FILE_STANDARD_INFO).cast(), size_of::<FILE_STANDARD_INFO>() as u32)
    };
    // SAFETY: `h` is the handle opened above, closed exactly once.
    unsafe { CloseHandle(h) };
    (ok != 0).then(|| info.AllocationSize.max(0) as u64)
}

fn item_of(name: &[u16], attrs: u32, ea: u32, eof: i64, alloc: i64, modified: i64, id: u64) -> Item<'_> {
    Item {
        name,
        attrs,
        tag: if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 { ea } else { 0 },
        logical: eof.max(0) as u64,
        disk: alloc.max(0) as u64,
        modified,
        id,
    }
}

/// The files met so far with more than one name, by file ID, so a hard-linked file counts once per
/// walk. Every file's ID goes in (the directory information has no link count), 8 bytes each.
#[derive(Default)]
pub struct HardLinks {
    seen: std::collections::HashSet<u64>,
}

impl HardLinks {
    /// True the first time `id` is met (count it); false for a name of a file already counted. An
    /// ID of 0 (none given) always counts.
    pub fn first(&mut self, id: u64) -> bool {
        id == 0 || self.seen.insert(id)
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::io::AsRawHandle;
    use std::sync::atomic::AtomicBool;
    use std::time::Instant;
    use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, GetCompressedFileSizeW};
    use windows_sys::Win32::System::IO::DeviceIoControl;

    // winioctl.h; declared here rather than enabling windows-sys's Ioctl feature for two codes.
    const FSCTL_SET_COMPRESSION: u32 = 0x0009_C040;
    const FSCTL_SET_SPARSE: u32 = 0x0009_00C4;

    fn ioctl(f: &std::fs::File, code: u32, input: &[u8]) -> bool {
        let mut got = 0u32;
        // SAFETY: an open file handle, an input buffer alive for the call, no output buffer.
        let ok = unsafe {
            DeviceIoControl(f.as_raw_handle(), code, input.as_ptr().cast(), input.len() as u32, std::ptr::null_mut(), 0, &mut got, std::ptr::null_mut()) != 0
        };
        if !ok {
            eprintln!("DeviceIoControl 0x{code:08X}: {}", std::io::Error::last_os_error());
        }
        ok
    }

    fn by_name(dir: &Path) -> std::collections::HashMap<String, (u32, u64, u64)> {
        let mut m = std::collections::HashMap::new();
        read(dir, true, |i| {
            if !i.is_dot() {
                m.insert(String::from_utf16_lossy(i.name), (i.attrs, i.logical, i.disk));
            }
        })
        .expect("listed");
        m
    }

    #[test]
    fn compressed_and_sparse_files_take_less_on_disk() {
        let dir = std::env::temp_dir().join(format!("twinfm-p13m3-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        // 1 MB of one byte: NTFS compression keeps a fraction of it.
        let plain = vec![b'x'; 1 << 20];
        std::fs::write(dir.join("plain.bin"), &plain).unwrap();
        let compressed = dir.join("compressed.bin");
        {
            let f = std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(&compressed).unwrap();
            assert!(ioctl(&f, FSCTL_SET_COMPRESSION, &1u16.to_le_bytes()), "FSCTL_SET_COMPRESSION refused");
            use std::io::Write;
            (&f).write_all(&plain).unwrap();
            // NTFS compresses as the cache writes the data out.
            f.sync_all().unwrap();
        }
        // 64 MB long, nothing written: a sparse file holds no clusters.
        let sparse = dir.join("sparse.bin");
        {
            let f = std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(&sparse).unwrap();
            assert!(ioctl(&f, FSCTL_SET_SPARSE, &[]), "FSCTL_SET_SPARSE refused");
            f.set_len(64 << 20).unwrap();
        }
        std::fs::write(dir.join("sub").join("small.txt"), b"hello").unwrap();
        // The answer GetCompressedFileSizeW gives (a call per file), to check the directory's.
        let api = |p: &Path| {
            let w: Vec<u16> = p.as_os_str().encode_wide().chain([0]).collect();
            let mut high = 0u32;
            // SAFETY: NUL-terminated path, out pointer alive for the call.
            let low = unsafe { GetCompressedFileSizeW(w.as_ptr(), &mut high) };
            (high as u64) << 32 | low as u64
        };
        let (c_api, s_api) = (api(&compressed), api(&sparse));
        let m = by_name(&dir);
        let without_ids = {
            let mut n = 0;
            read(&dir, false, |i| n += usize::from(!i.is_dot())).unwrap();
            n
        };
        let missing = read(&dir.join("not-there"), false, |_| {});
        // The Size on disk column's two reads: the folder's by name, and one file's own handle.
        let column = disk_by_name(&dir).unwrap();
        let own: Vec<Option<u64>> = ["plain.bin", "compressed.bin", "sparse.bin"].iter().map(|n| disk_of(&dir.join(n))).collect();
        let (gone, no_folder) = (disk_of(&dir.join("not-there")), disk_by_name(&dir.join("not-there")));
        std::fs::remove_dir_all(&dir).unwrap();

        let (pa, pl, pd) = m["plain.bin"];
        let (ca, cl, cd) = m["compressed.bin"];
        let (sa, sl, sd) = m["sparse.bin"];
        eprintln!("plain {pl} / {pd}, compressed {cl} / {cd} (GetCompressedFileSizeW {c_api}), sparse {sl} / {sd} (GetCompressedFileSizeW {s_api})");
        assert_eq!(pa & FILE_ATTRIBUTE_DIRECTORY, 0);
        assert_eq!((pl, pd), (1 << 20, 1 << 20), "a plain file holds its length in whole clusters");
        assert_ne!(ca & 0x800, 0, "FILE_ATTRIBUTE_COMPRESSED");
        assert_eq!(cl, 1 << 20);
        assert!(cd < 1 << 18, "a compressed file holds less than its length: {cd}");
        assert_eq!(cd, c_api, "the directory's size on disk is GetCompressedFileSizeW's");
        assert_ne!(sa & 0x200, 0, "FILE_ATTRIBUTE_SPARSE_FILE");
        assert_eq!(sl, 64 << 20);
        assert_eq!(sd, s_api, "the directory's size on disk is GetCompressedFileSizeW's");
        assert!(sd < 1 << 16, "a sparse file with nothing written holds nearly nothing: {sd}");
        assert_ne!(m["sub"].0 & FILE_ATTRIBUTE_DIRECTORY, 0);
        assert_eq!((m.len(), without_ids), (4, 4), "both classes list the same four");
        assert_eq!(missing, Err(2), "ERROR_FILE_NOT_FOUND");
        assert_eq!((column["plain.bin"], column["compressed.bin"], column["sparse.bin"], column.len()), (pd, cd, sd, 4));
        assert_eq!(own, vec![Some(pd), Some(cd), Some(sd)], "a file's own handle gives the directory's size on disk");
        assert!(gone.is_none() && no_folder.is_none());
    }

    #[test]
    fn a_file_counts_once_by_its_id() {
        let mut h = HardLinks::default();
        assert!(h.first(7) && !h.first(7) && h.first(8) && h.first(0) && h.first(0));
        assert_eq!(h.len(), 2);
    }

    /// The settle-first measurement (P13-M3), not a gate: `TWINFM_MEASURE=C:\traces\bigdir;C:\`
    /// `cargo test --release measure_walks -- --ignored --nocapture`. Each root is walked by today's
    /// folder-size walk (FindFirstFileExW), by the directory information without and with file IDs,
    /// and the IDs' set; time, totals and the working set after each. WOF files (Windows' compact
    /// files, tag 0x80000017) are counted apart, with what GetCompressedFileSizeW says of the first
    /// twenty, to check the directory's size on disk for them.
    #[test]
    #[ignore]
    fn measure_walks() {
        let roots = std::env::var("TWINFM_MEASURE").unwrap_or_else(|_| r"C:\traces\bigdir".into());
        for root in roots.split(';').map(Path::new) {
            for round in 0..2 {
                let never = AtomicBool::new(false);
                let t0 = Instant::now();
                let t = crate::fs::sizes::measure(root, crate::fs::sizes::SIZE_MAX_DEPTH, &never, |_| {}).unwrap();
                let (ws, pr) = crate::perf::memory_mb();
                eprintln!(
                    "[measure] {} round {round} find: {} bytes, {} files, {} folders, {} skipped in {:.0} ms; ws {ws:.0} MB private {pr:.0} MB",
                    root.display(),
                    t.bytes,
                    t.files,
                    t.folders,
                    t.skipped,
                    t0.elapsed().as_secs_f64() * 1e3
                );
                for ids in [false, true] {
                    let t0 = Instant::now();
                    let (mut logical, mut disk, mut files, mut folders, mut skipped, mut again, mut again_disk) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
                    let (mut wof, mut wof_disk, mut wof_logical) = (0u64, 0u64, 0u64);
                    let mut wof_check: Vec<(std::path::PathBuf, u64)> = Vec::new();
                    let mut links = HardLinks::default();
                    let mut stack = vec![root.to_path_buf()];
                    while let Some(dir) = stack.pop() {
                        let r = read(&dir, ids, |i| {
                            if i.is_dot() {
                                return;
                            }
                            if i.attrs & FILE_ATTRIBUTE_DIRECTORY != 0 {
                                folders += 1;
                                if i.attrs & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
                                    stack.push(dir.join(String::from_utf16_lossy(i.name)));
                                }
                            } else {
                                files += 1;
                                if i.tag == 0x8000_0017 {
                                    wof += 1;
                                    wof_disk += i.disk;
                                    wof_logical += i.logical;
                                    if wof_check.len() < 20 {
                                        wof_check.push((dir.join(String::from_utf16_lossy(i.name)), i.disk));
                                    }
                                }
                                if links.first(i.id) {
                                    logical += i.logical;
                                    disk += i.disk;
                                } else {
                                    again += 1;
                                    again_disk += i.disk;
                                }
                            }
                        });
                        skipped += u64::from(r.is_err());
                    }
                    let ms = t0.elapsed().as_secs_f64() * 1e3;
                    let (ws, pr) = crate::perf::memory_mb();
                    eprintln!(
                        "[measure] {} round {round} dirinfo ids={ids}: {logical} logical, {disk} on disk, {files} files, {folders} folders, {skipped} skipped, {again} names of files met before ({again_disk} bytes), {} ids, WOF {wof} files {wof_logical} -> {wof_disk}, in {ms:.0} ms; ws {ws:.0} MB private {pr:.0} MB",
                        root.display(),
                        links.len(),
                    );
                    if round == 0 && ids {
                        for (p, d) in &wof_check {
                            let w: Vec<u16> = p.as_os_str().encode_wide().chain([0]).collect();
                            let mut high = 0u32;
                            // SAFETY: NUL-terminated path, out pointer alive for the call.
                            let low = unsafe { GetCompressedFileSizeW(w.as_ptr(), &mut high) };
                            eprintln!("[measure] WOF {}: directory {d}, GetCompressedFileSizeW {}", p.display(), (high as u64) << 32 | low as u64);
                        }
                    }
                }
            }
        }
    }
}
