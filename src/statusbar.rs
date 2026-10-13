//! The status bar under each pane: items and hidden count on the left, the selection in the
//! middle, the volume's free space on the right. Composed on navigation, selection and operation
//! completion only; the free space is read on the enumeration worker (`disk_free`), never on the
//! UI thread and never per frame.

use crate::fs::sizes::DirTotals;
use crate::model::format_size;
use std::path::Path;
use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

/// Left: "N items (K hidden)", after the pane's note (the last file operation's result) when
/// there is one; "error: ..." when the listing failed. `of`: a name filter is on and M entries
/// pass the hidden-file switch ("N of M items"). `enum_ms` is the enumeration time, shown only
/// with `TWINFM_PERF_STATUS=1`; None while the listing is still loading.
pub fn left_text(
    note: &str,
    items: usize,
    of: Option<usize>,
    hidden: usize,
    loading: bool,
    enum_ms: Option<f64>,
    error: Option<&str>,
) -> String {
    if let Some(e) = error {
        return if note.is_empty() { format!("error: {e}") } else { format!("{note} | error: {e}") };
    }
    let mut s = String::new();
    if !note.is_empty() {
        s.push_str(note);
        s.push_str(" | ");
    }
    let plural = if of.unwrap_or(items) == 1 { "" } else { "s" };
    match of {
        Some(m) => s.push_str(&format!("{items} of {m} item{plural}")),
        None => s.push_str(&format!("{items} item{plural}")),
    }
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

/// Left for a search tab: "Searching... N found" while the walk runs, then "N found in S s"
/// ("N found, stopped after S s" when it was stopped), with the hidden count and the note as in a
/// folder. `secs` is None while searching.
pub fn search_text(note: &str, found: usize, hidden: usize, secs: Option<f64>, stopped: bool, error: Option<&str>) -> String {
    let mut s = match (error, secs) {
        (Some(e), _) => format!("error: {e}"),
        (None, None) => format!("Searching... {found} found"),
        (None, Some(t)) if stopped => format!("{found} found, stopped after {t:.1} s"),
        (None, Some(t)) => format!("{found} found in {t:.1} s"),
    };
    if hidden > 0 && error.is_none() {
        s.push_str(&format!(" ({hidden} hidden)"));
    }
    if note.is_empty() { s } else { format!("{note} | {s}") }
}

/// Middle: "M selected, <total size>", empty when nothing is selected. Without selected folders
/// the total is the selected files'; "M selected" alone when only links or drives are selected.
/// With selected folders (`dirs`) their computed sizes count too: "X so far..." while some are
/// still being sized (finished sizes plus partial totals), "X so far (K folders not sized)" when
/// more were selected than are sized.
pub fn mid_text(selected: usize, bytes: u64, any_file: bool, dirs: DirTotals) -> String {
    if dirs.folders == 0 {
        return match (selected, any_file) {
            (0, _) => String::new(),
            (n, true) => format!("{n} selected, {}", format_size(bytes)),
            (n, false) => format!("{n} selected"),
        };
    }
    let mut s = format!("{selected} selected, {}", format_size(bytes + dirs.bytes));
    if dirs.pending > 0 || dirs.not_sized > 0 {
        s.push_str(" so far");
    }
    if dirs.pending > 0 {
        s.push_str("...");
    }
    match dirs.not_sized {
        0 => {}
        1 => s.push_str(" (1 folder not sized)"),
        k => s.push_str(&format!(" ({k} folders not sized)")),
    }
    s
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
        assert_eq!(left_text("", 40000, None, 0, false, None, None), "40000 items");
        assert_eq!(left_text("", 1, None, 0, false, None, None), "1 item");
        assert_eq!(left_text("", 12, None, 3, false, None, None), "12 items (3 hidden)");
        assert_eq!(left_text("", 2000, None, 0, true, None, None), "2000 items (loading...)");
        assert_eq!(left_text("", 5, None, 0, false, Some(52.4), None), "5 items, enumerated in 52 ms");
        assert_eq!(left_text("Copied 2 items", 5, None, 1, false, None, None), "Copied 2 items | 5 items (1 hidden)");
        assert_eq!(left_text("", 0, None, 0, false, None, Some("Access is denied.")), "error: Access is denied.");
        assert_eq!(left_text("", 12, Some(400), 3, false, None, None), "12 of 400 items (3 hidden)");
        assert_eq!(left_text("", 0, Some(1), 0, false, None, None), "0 of 1 item");
        assert_eq!(search_text("", 3, 0, None, false, None), "Searching... 3 found");
        assert_eq!(search_text("", 12, 2, Some(4.26), false, None), "12 found in 4.3 s (2 hidden)");
        assert_eq!(search_text("Deleted 1 item", 7, 0, Some(1.0), true, None), "Deleted 1 item | 7 found, stopped after 1.0 s");
        assert_eq!(search_text("", 0, 0, Some(0.0), false, Some("Access is denied.")), "error: Access is denied.");
        let none = DirTotals::default();
        assert_eq!(mid_text(0, 0, false, none), "");
        assert_eq!(mid_text(3, 1536, true, none), "3 selected, 1.5 KB");
        assert_eq!(mid_text(2, 0, false, none), "2 selected");
        assert_eq!(mid_text(1, 0, true, none), "1 selected, 0 B");
    }

    #[test]
    fn folder_sizes_in_the_middle_text() {
        let kb = 1024;
        // Two folders sized, one file: everything known.
        let done = DirTotals { folders: 2, bytes: 3 * kb, pending: 0, not_sized: 0 };
        assert_eq!(mid_text(3, kb, true, done), "3 selected, 4.0 KB");
        // Folders only, one still walking (its partial total counts).
        let walking = DirTotals { folders: 2, bytes: 2 * kb, pending: 1, not_sized: 0 };
        assert_eq!(mid_text(2, 0, false, walking), "2 selected, 2.0 KB so far...");
        // More folders than the cap: the rest are not sized.
        let capped = DirTotals { folders: 150, bytes: 5 * kb, pending: 0, not_sized: 50 };
        assert_eq!(mid_text(150, 0, false, capped), "150 selected, 5.0 KB so far (50 folders not sized)");
        let both = DirTotals { folders: 102, bytes: 0, pending: 3, not_sized: 1 };
        assert_eq!(mid_text(102, 0, false, both), "102 selected, 0 B so far... (1 folder not sized)");
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
