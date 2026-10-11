//! The status bar under each pane: items and hidden count on the left, the selection in the
//! middle, the volume's free space on the right. Composed on navigation, selection and operation
//! completion only; the free space is read on the enumeration worker (`disk_free`), never on the
//! UI thread and never per frame.

use crate::model::format_size;
use std::path::Path;
use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

/// Left: "N items (K hidden)", after the pane's note (the last file operation's result) when
/// there is one; "error: ..." when the listing failed. `enum_ms` is the enumeration time, shown
/// only with `TWINFM_PERF_STATUS=1`; None while the listing is still loading.
pub fn left_text(note: &str, items: usize, hidden: usize, loading: bool, enum_ms: Option<f64>, error: Option<&str>) -> String {
    if let Some(e) = error {
        return if note.is_empty() { format!("error: {e}") } else { format!("{note} | error: {e}") };
    }
    let mut s = String::new();
    if !note.is_empty() {
        s.push_str(note);
        s.push_str(" | ");
    }
    s.push_str(&format!("{items} item{}", if items == 1 { "" } else { "s" }));
    if hidden > 0 {
        s.push_str(&format!(" ({hidden} hidden)"));
    }
    if loading {
        s.push_str(" (loading...)");
    } else if let Some(ms) = enum_ms {
        s.push_str(&format!(", enumerated in {ms:.0} ms"));
    }
    s
}

/// Middle: "M selected, <total size of the selected files>", "M selected" when only folders are
/// selected, empty when nothing is.
pub fn mid_text(selected: usize, bytes: u64, any_file: bool) -> String {
    match (selected, any_file) {
        (0, _) => String::new(),
        (n, true) => format!("{n} selected, {}", format_size(bytes)),
        (n, false) => format!("{n} selected"),
    }
}

/// Right: "X free of Y" for the pane's volume, empty when unknown (This PC, an error).
pub fn free_text(free: Option<(u64, u64)>) -> String {
    match free {
        Some((free, total)) if total > 0 => format!("{} free of {}", format_size(free), format_size(total)),
        Some((free, _)) => format!("{} free", format_size(free)),
        None => String::new(),
    }
}

/// Free bytes (for this user) and total bytes of the volume holding `folder`. Can stall on a
/// disconnected network share, so it runs on the enumeration worker.
pub fn disk_free(folder: &Path) -> Option<(u64, u64)> {
    let mut wide: Vec<u16> = folder.as_os_str().to_string_lossy().encode_utf16().collect();
    if wide.last() != Some(&u16::from(b'\\')) {
        wide.push(u16::from(b'\\'));
    }
    wide.push(0);
    let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
    // SAFETY: `wide` is NUL-terminated and the three out pointers are live u64s.
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) };
    (ok != 0).then_some((avail, total))
}

/// The note with "N operations queued" after it while jobs wait behind the running one (Escape
/// in the list cancels them).
pub fn with_queue(note: &str, queued: usize) -> String {
    let q = match queued {
        0 => return note.to_string(),
        1 => "1 operation queued".to_string(),
        n => format!("{n} operations queued"),
    };
    if note.is_empty() { q } else { format!("{note}, {q}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_text() {
        assert_eq!(with_queue("copying 2 items...", 0), "copying 2 items...");
        assert_eq!(with_queue("copying 2 items...", 1), "copying 2 items..., 1 operation queued");
        assert_eq!(with_queue("", 3), "3 operations queued");
    }

    #[test]
    fn status_text_composition() {
        assert_eq!(left_text("", 40000, 0, false, None, None), "40000 items");
        assert_eq!(left_text("", 1, 0, false, None, None), "1 item");
        assert_eq!(left_text("", 12, 3, false, None, None), "12 items (3 hidden)");
        assert_eq!(left_text("", 2000, 0, true, None, None), "2000 items (loading...)");
        assert_eq!(left_text("", 5, 0, false, Some(52.4), None), "5 items, enumerated in 52 ms");
        assert_eq!(left_text("Copied 2 items", 5, 1, false, None, None), "Copied 2 items | 5 items (1 hidden)");
        assert_eq!(left_text("", 0, 0, false, None, Some("Access is denied.")), "error: Access is denied.");
        assert_eq!(mid_text(0, 0, false), "");
        assert_eq!(mid_text(3, 1536, true), "3 selected, 1.5 KB");
        assert_eq!(mid_text(2, 0, false), "2 selected");
        assert_eq!(mid_text(1, 0, true), "1 selected, 0 B");
    }

    #[test]
    fn free_space_formatting() {
        assert_eq!(free_text(None), "");
        let gb = 1024u64 * 1024 * 1024;
        assert_eq!(free_text(Some((120 * gb + gb / 2, 476 * gb))), "120.50 GB free of 476.00 GB");
        assert_eq!(free_text(Some((512 * 1024 * 1024, 0))), "512.0 MB free");
        let c = disk_free(Path::new(r"C:\")).expect("C: has a free-space figure");
        assert!(c.1 > 0 && c.0 <= c.1);
        assert_eq!(disk_free(Path::new(r"C:\Windows")).map(|f| f.1), Some(c.1), "any folder on the volume");
    }
}
