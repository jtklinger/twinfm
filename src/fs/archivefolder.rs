//! Levels of the archives Windows' `ArchiveFolder` handles (7z, rar, tar, gz and the rest; P7-M3),
//! listed through the shell on one dedicated STA thread named `archive`: the shell item for the
//! level (`C:\dir\x.7z\sub`), its `IEnumShellItems` (`BindToHandler(BHID_EnumItems)`, the folder's
//! `EnumObjects`), and per item its name, attributes, size and modified time (`IShellItem2`). Every
//! COM object is made, used and released on that thread; callers (the enumeration worker, the
//! search walker, the crumb dropdown's lister) wait on a reply channel. Never the UI thread.
//! `fs::archive` caches the levels.

use crossbeam_channel::{Sender, bounded, unbounded};
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;
use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree};
use windows_sys::Win32::UI::Shell::{BHID_EnumItems, SHCreateItemFromParsingName};
use windows_sys::core::{GUID, HRESULT};

/// One item of a level, as the shell reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Raw {
    pub name: String,
    pub dir: bool,
    pub size: u64,
    /// Unix seconds, or None when the shell has no date for it.
    pub modified: Option<i64>,
}

enum Req {
    Level { path: PathBuf, reply: Sender<Result<Vec<Raw>, String>> },
    /// Copy the item at `path` into `dest` (P8-L3: an archive inside an archive).
    Extract { path: PathBuf, dest: PathBuf, reply: Sender<Result<(), String>> },
}

static TX: OnceLock<Sender<Req>> = OnceLock::new();

/// The worker, started at the first request.
fn worker() -> &'static Sender<Req> {
    TX.get_or_init(|| {
        let (tx, rx) = unbounded::<Req>();
        std::thread::Builder::new()
            .name("archive".into())
            .spawn(move || {
                // SAFETY: an STA for this thread's lifetime (the process's: it is never joined).
                let hr = unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
                eprintln!("[archive] worker CoInitializeEx(APARTMENTTHREADED) = 0x{:08X}", hr as u32);
                for req in rx {
                    match req {
                        Req::Level { path, reply } => {
                            let _ = reply.send(enumerate(&path));
                        }
                        Req::Extract { path, dest, reply } => {
                            let _ = reply.send(crate::fileops::copy_quietly(&path, &dest));
                        }
                    }
                }
            })
            .expect("spawn archive thread");
        tx
    })
}

/// Copy the item at `path` (inside an archive the shell lists) into the folder `dest`, quietly,
/// on the worker. Blocks the calling thread until it is done.
pub fn extract(path: &Path, dest: &Path) -> Result<(), String> {
    let (reply, rx) = bounded(1);
    worker()
        .send(Req::Extract { path: path.to_path_buf(), dest: dest.to_path_buf(), reply })
        .map_err(|_| "the archive worker has stopped".to_string())?;
    rx.recv().map_err(|_| "the archive worker has stopped".to_string())?
}

/// The items of the level at `path` (an archive, or a folder inside one), and the time the shell
/// took on the worker. Blocks the calling thread until the worker answers.
pub fn level(path: &Path) -> Result<(Vec<Raw>, f64), String> {
    let (reply, rx) = bounded(1);
    let t0 = Instant::now();
    worker().send(Req::Level { path: path.to_path_buf(), reply }).map_err(|_| "the archive worker has stopped".to_string())?;
    let items = rx.recv().map_err(|_| "the archive worker has stopped".to_string())??;
    Ok((items, t0.elapsed().as_secs_f64() * 1e3))
}

const IID_ISHELLITEM: GUID = GUID::from_u128(0x43826d1e_e718_42ee_bc55_a1e261c37bfe);
const IID_ISHELLITEM2: GUID = GUID::from_u128(0x7e9fb0d3_919f_4307_ab2e_9b1860310c93);
const IID_IENUMSHELLITEMS: GUID = GUID::from_u128(0x70629033_e363_4a28_a567_0db78006e6d7);
const SIGDN_PARENTRELATIVEPARSING: i32 = 0x8001_8001u32 as i32;
const SFGAO_FOLDER: u32 = 0x2000_0000;
const SFGAO_STREAM: u32 = 0x0040_0000;

#[repr(C)]
struct PropertyKey {
    fmtid: GUID,
    pid: u32,
}
/// PKEY_Size and PKEY_DateModified: {B725F130-47EF-101A-A5F1-02608C9EEBAC}, 12 and 14.
const PKEY_SIZE: PropertyKey = PropertyKey { fmtid: GUID::from_u128(0xb725f130_47ef_101a_a5f1_02608c9eebac), pid: 12 };
const PKEY_DATE_MODIFIED: PropertyKey = PropertyKey { fmtid: GUID::from_u128(0xb725f130_47ef_101a_a5f1_02608c9eebac), pid: 14 };

type Slot = *const c_void;

#[repr(C)]
struct UnknownVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    _add_ref: Slot,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
}

/// IShellItem, then the IShellItem2 methods up to GetUInt64 (shobjidl_core.h order).
#[repr(C)]
struct ShellItem2Vtbl {
    base: UnknownVtbl,
    bind_to_handler: unsafe extern "system" fn(*mut c_void, *mut c_void, *const GUID, *const GUID, *mut *mut c_void) -> HRESULT,
    _get_parent: Slot,
    get_display_name: unsafe extern "system" fn(*mut c_void, i32, *mut *mut u16) -> HRESULT,
    get_attributes: unsafe extern "system" fn(*mut c_void, u32, *mut u32) -> HRESULT,
    _compare: Slot,
    _get_property_store: Slot,
    _get_property_store_with_create_object: Slot,
    _get_property_store_for_keys: Slot,
    _get_property_description_list: Slot,
    _update: Slot,
    _get_property: Slot,
    _get_clsid: Slot,
    get_file_time: unsafe extern "system" fn(*mut c_void, *const PropertyKey, *mut FILETIME) -> HRESULT,
    _get_int32: Slot,
    _get_string: Slot,
    _get_uint32: Slot,
    get_uint64: unsafe extern "system" fn(*mut c_void, *const PropertyKey, *mut u64) -> HRESULT,
}

#[repr(C)]
struct EnumShellItemsVtbl {
    base: UnknownVtbl,
    next: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void, *mut u32) -> HRESULT,
}

/// An owned interface pointer, released on drop (on this worker).
struct Com(*mut c_void);

impl Drop for Com {
    fn drop(&mut self) {
        // SAFETY: a live interface we hold one reference to; every vtable starts with IUnknown.
        unsafe { ((**(self.0 as *const *const UnknownVtbl)).release)(self.0) };
    }
}

impl Com {
    /// SAFETY: the pointer must be an interface whose vtable starts as `T`.
    unsafe fn vtbl<T>(&self) -> &T {
        unsafe { &**(self.0 as *const *const T) }
    }
}

/// FILETIME (100 ns since 1601, UTC) as Unix seconds; None for zero.
fn unix(ft: FILETIME) -> Option<i64> {
    let t = ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;
    (t != 0).then(|| (t as i64 - 116_444_736_000_000_000) / 10_000_000)
}

fn enumerate(path: &Path) -> Result<Vec<Raw>, String> {
    let w: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut p: *mut c_void = std::ptr::null_mut();
    // SAFETY: `w` is NUL-terminated; `p` receives an IShellItem or stays null.
    let hr = unsafe { SHCreateItemFromParsingName(w.as_ptr(), std::ptr::null_mut(), &IID_ISHELLITEM, &mut p) };
    if hr < 0 || p.is_null() {
        return Err(format!("the shell cannot open it (0x{:08X})", hr as u32));
    }
    let item = Com(p);
    let mut e: *mut c_void = std::ptr::null_mut();
    // SAFETY: `item` is a live IShellItem; `e` receives an IEnumShellItems.
    let hr = unsafe { (item.vtbl::<ShellItem2Vtbl>().bind_to_handler)(item.0, std::ptr::null_mut(), &BHID_EnumItems, &IID_IENUMSHELLITEMS, &mut e) };
    if hr < 0 || e.is_null() {
        return Err(format!("the shell cannot list it (0x{:08X})", hr as u32));
    }
    let e = Com(e);
    let mut out = Vec::new();
    loop {
        let mut it: *mut c_void = std::ptr::null_mut();
        let mut got = 0u32;
        // SAFETY: a live enumerator; one item out, ours to release.
        let hr = unsafe { (e.vtbl::<EnumShellItemsVtbl>().next)(e.0, 1, &mut it, &mut got) };
        if hr != 0 || got == 0 || it.is_null() {
            break;
        }
        let it = Com(it);
        let mut i2: *mut c_void = std::ptr::null_mut();
        // SAFETY: QueryInterface on a live item.
        if unsafe { (it.vtbl::<UnknownVtbl>().query_interface)(it.0, &IID_ISHELLITEM2, &mut i2) } < 0 || i2.is_null() {
            continue;
        }
        let i2 = Com(i2);
        // SAFETY: `i2` is a live IShellItem2; the name is freed with CoTaskMemFree; the out
        // values are plain locals.
        unsafe {
            let v = i2.vtbl::<ShellItem2Vtbl>();
            let mut s: *mut u16 = std::ptr::null_mut();
            if (v.get_display_name)(i2.0, SIGDN_PARENTRELATIVEPARSING, &mut s) < 0 || s.is_null() {
                continue;
            }
            let len = (0..).take_while(|&k| *s.add(k) != 0).count();
            let name = String::from_utf16_lossy(std::slice::from_raw_parts(s, len));
            CoTaskMemFree(s as *const c_void);
            let mut attrs = 0u32;
            (v.get_attributes)(i2.0, SFGAO_FOLDER | SFGAO_STREAM, &mut attrs);
            // An archive inside the archive is a stream that the shell may also call a folder.
            let dir = attrs & SFGAO_FOLDER != 0 && attrs & SFGAO_STREAM == 0;
            let mut size = 0u64;
            if !dir && (v.get_uint64)(i2.0, &PKEY_SIZE, &mut size) < 0 {
                size = 0;
            }
            let mut ft = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
            let modified = if (v.get_file_time)(i2.0, &PKEY_DATE_MODIFIED, &mut ft) >= 0 { unix(ft) } else { None };
            out.push(Raw { name, dir, size, modified });
        }
    }
    Ok(out)
}

/// Removes a test's scratch folder after it listed archives in it through `ArchiveFolder`. A
/// sharing violation (os error 32) on that removal came once in about 1,500 test runs under load
/// (2026-10-19): something still held a file there for a moment, though every COM object above is
/// released before `level` returns. So the removal is tried again, 100 ms apart, for up to 2 s.
#[cfg(test)]
pub(crate) fn remove_scratch(dir: &Path) {
    for k in 1.. {
        match std::fs::remove_dir_all(dir) {
            Ok(()) => return,
            Err(e) if e.raw_os_error() == Some(32) && k < 20 => {
                eprintln!("[archive] {} held (try {k}): {e}", dir.display());
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => panic!("cannot remove {}: {e}", dir.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filetime_to_unix() {
        let ft = |t: u64| FILETIME { dwLowDateTime: t as u32, dwHighDateTime: (t >> 32) as u32 };
        assert_eq!(unix(ft(116_444_736_000_000_000)), Some(0));
        assert_eq!(unix(ft(116_444_736_000_000_000 + 10_000_000 * 86_400)), Some(86_400));
        assert_eq!(unix(ft(0)), None);
    }
}
