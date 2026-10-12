//! Volume arrival and removal. A permanent comctl32 subclass on the main window watches
//! `WM_DEVICECHANGE`; on `DBT_DEVICEARRIVAL` or `DBT_DEVICEREMOVECOMPLETE` for a volume it posts
//! `crate::volumes_changed` to the event loop (which debounces, refreshes the drive lists and any
//! tab showing This PC). Everything else goes down the subclass chain untouched.
//!
//! The same subclass also gets `WM_CLIPBOARDUPDATE` (the window is a clipboard format listener)
//! and posts `crate::clipboard_changed`, which clears the cut marks when another program took
//! the clipboard.
//!
//! The subclass procedure can run inside a modal loop (a context menu, a drag, a shell dialog),
//! so it touches no app state: it only posts.

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::DataExchange::AddClipboardFormatListener;
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DBT_DEVICEARRIVAL, DBT_DEVICEREMOVECOMPLETE, DBT_DEVTYP_VOLUME, DEV_BROADCAST_HDR, DEV_BROADCAST_VOLUME, WM_CLIPBOARDUPDATE,
    WM_DEVICECHANGE,
};

/// Our permanent subclass id on the main window (shellmenu's temporary one is "twfm").
const SUBCLASS_ID: usize = 0x7477_6476; // "twdv"

/// Subclass the main window. Call once, on the UI thread, when its HWND is first known.
pub fn install(hwnd: isize) {
    // SAFETY: subclassing our own window on its own thread; the procedure needs no refdata.
    let ok = unsafe { SetWindowSubclass(hwnd as HWND, Some(subclass_proc), SUBCLASS_ID, 0) };
    eprintln!("[devices] WM_DEVICECHANGE subclass {}", if ok != 0 { "installed" } else { "failed" });
    // SAFETY: our own window; the listener goes away with the window (no removal at exit).
    let listening = unsafe { AddClipboardFormatListener(hwnd as HWND) };
    eprintln!("[clipboard] format listener {}", if listening != 0 { "added" } else { "failed" });
}

unsafe extern "system" fn subclass_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    if msg == crate::dnd::WM_RUN_DRAG {
        // The one exception to "only posts": the drag a row asked for runs here, dispatched by
        // winit's message loop outside its event handler (see `crate::run_pending_drag`, which
        // checks that no app-state borrow is held).
        crate::run_pending_drag();
        return 0;
    }
    if msg == WM_CLIPBOARDUPDATE {
        let _ = slint::invoke_from_event_loop(crate::clipboard_changed);
    }
    let event = wp as u32;
    if msg == WM_DEVICECHANGE && (event == DBT_DEVICEARRIVAL || event == DBT_DEVICEREMOVECOMPLETE) && lp != 0 {
        // SAFETY: for these two events lParam points to a DEV_BROADCAST_HDR.
        let hdr = unsafe { &*(lp as *const DEV_BROADCAST_HDR) };
        if hdr.dbch_devicetype == DBT_DEVTYP_VOLUME {
            // SAFETY: the header says this is a DEV_BROADCAST_VOLUME.
            let mask = unsafe { (*(lp as *const DEV_BROADCAST_VOLUME)).dbcv_unitmask };
            let what = if event == DBT_DEVICEARRIVAL { "arrived" } else { "removed" };
            eprintln!("[devices] volume {what}, drive mask 0x{mask:08x}");
            let _ = slint::invoke_from_event_loop(crate::volumes_changed);
        }
    }
    // SAFETY: pass everything down the subclass chain (winit's window procedure).
    unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
}
