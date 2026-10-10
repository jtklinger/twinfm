//! Files on the real clipboard, in the formats Explorer reads and writes: `CF_HDROP` (a
//! `DROPFILES` header followed by a double-NUL-terminated list of wide paths) plus the
//! registered format "Preferred DropEffect" (a DWORD, `DROPEFFECT_COPY` or `DROPEFFECT_MOVE`).
//! Cheap enough for the UI thread, but call these with no app-state borrow held: emptying the
//! clipboard sends `WM_DESTROYCLIPBOARD` to the previous owner, which may be our own window.

use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;
use windows_sys::Win32::Foundation::{GlobalFree, HANDLE, HWND};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows_sys::Win32::System::Ole::{CF_HDROP, CF_UNICODETEXT, DROPEFFECT_COPY, DROPEFFECT_MOVE};

pub const EFFECT_COPY: u32 = DROPEFFECT_COPY;
pub const EFFECT_MOVE: u32 = DROPEFFECT_MOVE;

/// sizeof(DROPFILES): pFiles u32, POINT (2 x i32), fNC BOOL, fWide BOOL.
const HEADER: usize = 20;

/// The `CF_HDROP` block for `paths`: header with `pFiles = 20` and `fWide = TRUE`, then each
/// path as UTF-16 with its NUL, then one more NUL.
pub fn dropfiles_block(paths: &[PathBuf]) -> Vec<u8> {
    let mut b = Vec::with_capacity(HEADER + paths.iter().map(|p| p.as_os_str().len() * 2 + 2).sum::<usize>() + 2);
    b.extend_from_slice(&(HEADER as u32).to_le_bytes()); // pFiles
    b.extend_from_slice(&[0; 12]); // pt.x, pt.y, fNC
    b.extend_from_slice(&1u32.to_le_bytes()); // fWide
    for p in paths {
        for u in p.as_os_str().encode_wide().chain([0]) {
            b.extend_from_slice(&u.to_le_bytes());
        }
    }
    b.extend_from_slice(&[0, 0]);
    b
}

/// The paths in a `CF_HDROP` block, or None if the header is malformed. Reads wide lists;
/// an ANSI list (`fWide = FALSE`, written by old programs) is read as Latin-1.
pub fn parse_dropfiles(b: &[u8]) -> Option<Vec<PathBuf>> {
    let u32_at = |i: usize| b.get(i..i + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]));
    let start = u32_at(0)? as usize;
    let wide = u32_at(16)? != 0;
    let list = b.get(start..)?;
    let mut out = Vec::new();
    if wide {
        let units: Vec<u16> = list.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        for s in units.split(|&u| u == 0) {
            if s.is_empty() {
                break;
            }
            out.push(PathBuf::from(OsString::from_wide(s)));
        }
    } else {
        for s in list.split(|&c| c == 0) {
            if s.is_empty() {
                break;
            }
            out.push(PathBuf::from(s.iter().map(|&c| c as char).collect::<String>()));
        }
    }
    Some(out)
}

fn preferred_drop_effect() -> u32 {
    let name: Vec<u16> = "Preferred DropEffect".encode_utf16().chain([0]).collect();
    // SAFETY: NUL-terminated wide string; returns the same id for the session every time.
    unsafe { RegisterClipboardFormatW(name.as_ptr()) }
}

/// The clipboard, open; closed on drop so every early return closes it.
struct Open;

impl Open {
    fn new(hwnd: isize) -> Result<Open, String> {
        // SAFETY: plain Win32 call; fails if another window holds the clipboard open.
        if unsafe { OpenClipboard(hwnd as HWND) } == 0 {
            return Err("the clipboard is in use by another program".into());
        }
        Ok(Open)
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: pairs the successful OpenClipboard in `Open::new`.
        unsafe { CloseClipboard() };
    }
}

/// Hand `bytes` to the clipboard as `format`. On success the system owns the memory.
fn set(format: u32, bytes: &[u8]) -> Result<(), String> {
    // SAFETY: a moveable block of exactly `bytes.len()` bytes is filled while locked, then given
    // to SetClipboardData; if that fails the block is still ours and is freed.
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
        if h.is_null() {
            return Err("out of memory".into());
        }
        let p = GlobalLock(h) as *mut u8;
        if p.is_null() {
            GlobalFree(h);
            return Err("GlobalLock failed".into());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        GlobalUnlock(h);
        if SetClipboardData(format, h as HANDLE).is_null() {
            GlobalFree(h);
            return Err("SetClipboardData failed".into());
        }
    }
    Ok(())
}

/// A copy of the clipboard data in `format`, if present.
fn get(format: u32) -> Option<Vec<u8>> {
    // SAFETY: the handle belongs to the clipboard, which is open; it is only locked, copied
    // and unlocked here, never freed.
    unsafe {
        let h = GetClipboardData(format);
        if h.is_null() {
            return None;
        }
        let p = GlobalLock(h) as *const u8;
        if p.is_null() {
            return None;
        }
        let v = std::slice::from_raw_parts(p, GlobalSize(h)).to_vec();
        GlobalUnlock(h);
        Some(v)
    }
}

/// Put `paths` on the clipboard for a copy (`EFFECT_COPY`) or a cut (`EFFECT_MOVE`). `hwnd`
/// becomes the clipboard owner and must be a real window: with no owner, `SetClipboardData`
/// fails after `EmptyClipboard`.
pub fn write(hwnd: isize, paths: &[PathBuf], effect: u32) -> Result<(), String> {
    let block = dropfiles_block(paths);
    let fmt = preferred_drop_effect();
    let _open = Open::new(hwnd)?;
    // SAFETY: the clipboard is open (`_open`).
    if unsafe { EmptyClipboard() } == 0 {
        return Err("EmptyClipboard failed".into());
    }
    set(CF_HDROP as u32, &block)?;
    set(fmt, &effect.to_le_bytes())
}

/// The files on the clipboard and the effect the source asked for (copy when it did not say),
/// or None when the clipboard holds no files.
pub fn read(hwnd: isize) -> Result<Option<(Vec<PathBuf>, u32)>, String> {
    let fmt = preferred_drop_effect();
    let _open = Open::new(hwnd)?;
    let Some(block) = get(CF_HDROP as u32) else { return Ok(None) };
    let paths = parse_dropfiles(&block).ok_or("malformed CF_HDROP on the clipboard")?;
    let effect = get(fmt)
        .and_then(|b| b.get(..4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]])))
        .unwrap_or(EFFECT_COPY);
    // A source may offer both (COPY | MOVE); Explorer treats that as a move only when MOVE alone.
    let effect = if effect & EFFECT_MOVE != 0 && effect & EFFECT_COPY == 0 { EFFECT_MOVE } else { EFFECT_COPY };
    Ok(Some((paths, effect)))
}

/// `text` as a `CF_UNICODETEXT` block: UTF-16 with a terminating NUL.
pub fn unicode_text_block(text: &str) -> Vec<u8> {
    text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
}

/// Put `text` on the clipboard as `CF_UNICODETEXT` ("Copy path"); `hwnd` becomes the owner.
pub fn write_text(hwnd: isize, text: &str) -> Result<(), String> {
    let block = unicode_text_block(text);
    let _open = Open::new(hwnd)?;
    // SAFETY: the clipboard is open (`_open`).
    if unsafe { EmptyClipboard() } == 0 {
        return Err("EmptyClipboard failed".into());
    }
    set(CF_UNICODETEXT as u32, &block)
}

/// Empty the clipboard (after pasting a cut, as Explorer does).
pub fn clear(hwnd: isize) -> Result<(), String> {
    let _open = Open::new(hwnd)?;
    // SAFETY: the clipboard is open.
    if unsafe { EmptyClipboard() } == 0 { Err("EmptyClipboard failed".into()) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::Shell::DROPFILES;
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, HWND_MESSAGE};

    #[test]
    fn dropfiles_layout_and_round_trip() {
        assert_eq!(std::mem::size_of::<DROPFILES>(), HEADER);
        let paths = vec![PathBuf::from(r"C:\a.txt"), PathBuf::from(r"C:\dir\b \u{e9}.bin")];
        let b = dropfiles_block(&paths);
        assert_eq!(&b[0..4], &20u32.to_le_bytes(), "pFiles points just past the header");
        assert_eq!(&b[4..16], &[0; 12], "pt and fNC are zero");
        assert_eq!(&b[16..20], &1u32.to_le_bytes(), "fWide");
        let a_len = r"C:\a.txt".len();
        assert_eq!(&b[20..22], &(b'C' as u16).to_le_bytes());
        assert_eq!(&b[20 + a_len * 2..22 + a_len * 2], &[0, 0], "first path NUL-terminated");
        assert_eq!(&b[b.len() - 4..], &[0, 0, 0, 0], "last path's NUL plus the list's final NUL");
        let total_units: usize = paths.iter().map(|p| p.as_os_str().encode_wide().count() + 1).sum::<usize>() + 1;
        assert_eq!(b.len(), HEADER + total_units * 2);
        assert_eq!(parse_dropfiles(&b).unwrap(), paths);
        assert_eq!(parse_dropfiles(&dropfiles_block(&[])).unwrap(), Vec::<PathBuf>::new());
        assert!(parse_dropfiles(&b[..10]).is_none(), "truncated header");
        // An ANSI list.
        let mut ansi = b[..20].to_vec();
        ansi[16..20].copy_from_slice(&0u32.to_le_bytes());
        ansi.extend_from_slice(b"C:\\x\0D:\\y\0\0");
        assert_eq!(parse_dropfiles(&ansi).unwrap(), vec![PathBuf::from(r"C:\x"), PathBuf::from(r"D:\y")]);
    }

    #[test]
    fn unicode_text_is_nul_terminated_utf16() {
        let text = "C:\\a b\r\nC:\\\u{e9}";
        let b = unicode_text_block(text);
        let units: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        assert_eq!(units.last(), Some(&0), "NUL-terminated");
        assert_eq!(String::from_utf16(&units[..units.len() - 1]).unwrap(), text);
    }

    /// Touches the real clipboard of this desktop: whatever was on it is replaced, and it is left empty.
    #[test]
    fn real_clipboard_round_trip_overwrites_clipboard() {
        let class: Vec<u16> = "STATIC".encode_utf16().chain([0]).collect();
        // SAFETY: a message-only STATIC window, destroyed at the end of the test, to own the clipboard.
        let hwnd = unsafe {
            CreateWindowExW(0, class.as_ptr(), std::ptr::null(), 0, 0, 0, 0, 0, HWND_MESSAGE, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null())
        };
        assert!(!hwnd.is_null());
        let h = hwnd as isize;
        let dir = std::env::temp_dir().join(format!("twinfm-m3-test-clip-{}", std::process::id()));
        let paths = vec![dir.join("one.txt"), dir.join("two words.txt")];
        // Another program (a clipboard history, a viewer) may hold the clipboard for a moment.
        fn retry<T>(f: impl Fn() -> Result<T, String>) -> T {
            for _ in 0..20 {
                if let Ok(v) = f() {
                    return v;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            f().unwrap()
        }
        retry(|| write(h, &paths, EFFECT_MOVE));
        assert_eq!(retry(|| read(h)), Some((paths.clone(), EFFECT_MOVE)));
        retry(|| write(h, &paths, EFFECT_COPY));
        assert_eq!(retry(|| read(h)), Some((paths.clone(), EFFECT_COPY)));
        retry(|| clear(h));
        assert_eq!(retry(|| read(h)), None);
        // SAFETY: the window created above, on this thread.
        unsafe { DestroyWindow(hwnd) };
    }
}
