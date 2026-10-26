//! The sidebar: Drives, Places (known folders) and Bookmarks, as one flat list of rows with
//! section headers. Built on the UI thread only when something in it changes (shown, F5, a
//! bookmark added, removed, renamed or moved, the drive list arriving from its worker); the
//! composition and the
//! bookmark list rules are pure and unit-tested.

use crate::model::Entry;
use crate::settings::Bookmark;
use std::path::PathBuf;
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Music, FOLDERID_Pictures, FOLDERID_Profile,
    FOLDERID_SkyDrive, FOLDERID_Videos, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
};
use windows_sys::core::GUID;

/// What a row is; the discriminants are the `kind` int the Slint `SideItem` carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    Header = 0,
    Drive = 1,
    Place = 2,
    Bookmark = 3,
    /// A folder a pane showed lately (P12-M5).
    Recent = 4,
}

/// The Recent group's length (P12-M5).
pub const RECENT_MAX: usize = 8;

/// One sidebar row: its text, kind, the folder a click opens (none for a header) and, for a
/// bookmark, its index in the bookmark list (a recent folder's in the Recent list; a header's
/// section: 0 Drives, 1 Places, 2 Recent, 3 Bookmarks).
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub label: String,
    pub kind: RowKind,
    pub path: Option<PathBuf>,
    pub index: usize,
}

/// The Places section, in order: label and known folder. OneDrive's is its sync root (P6-M1: the
/// cloud marks' folder, which Explorer lists under the account's name).
const PLACES: [(&str, GUID); 8] = [
    ("Home", FOLDERID_Profile),
    ("OneDrive", FOLDERID_SkyDrive),
    ("Desktop", FOLDERID_Desktop),
    ("Documents", FOLDERID_Documents),
    ("Downloads", FOLDERID_Downloads),
    ("Pictures", FOLDERID_Pictures),
    ("Music", FOLDERID_Music),
    ("Videos", FOLDERID_Videos),
];

/// The known folders that resolve on this machine to an existing folder (a folder the shell does
/// not know, or one it names that is not there, such as OneDrive's without OneDrive set up, is
/// left out).
pub fn known_folders() -> Vec<(String, PathBuf)> {
    PLACES
        .iter()
        .filter_map(|(label, id)| {
            let mut p: windows_sys::core::PWSTR = std::ptr::null_mut();
            // SAFETY: a valid KNOWNFOLDERID and out pointer; the returned buffer is freed with
            // CoTaskMemFree whether or not the call succeeded, as the documentation requires.
            let hr = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT as u32, std::ptr::null_mut(), &mut p) };
            let path = (hr >= 0 && !p.is_null()).then(|| {
                // SAFETY: on success `p` is a NUL-terminated wide string.
                let len = (0..).take_while(|&i| unsafe { *p.add(i) } != 0).count();
                PathBuf::from(String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, len) }))
            });
            // SAFETY: CoTaskMemFree accepts null.
            unsafe { CoTaskMemFree(p as *const std::ffi::c_void) };
            path.filter(|p| p.is_dir()).map(|p| (label.to_string(), p))
        })
        .collect()
}

/// A drive row's label from the drive list's entry: "Label (C:)" as the type column has it, or
/// "Local Disk (C:)" when the volume has no label.
pub fn drive_label(e: &Entry) -> String {
    let letter = e.name.trim_end_matches('\\');
    if e.type_text.contains(&format!("({letter})")) { e.type_text.to_string() } else { format!("{} ({letter})", e.type_text) }
}

/// The whole list: a header per section, then its rows. The Bookmarks header is always there,
/// so Ctrl+D has a visible place to land.
pub fn compose(drives: &[(String, PathBuf)], places: &[(String, PathBuf)], recent: &[String], bookmarks: &[Bookmark]) -> Vec<Row> {
    let header = |label: &str, index| Row { label: label.into(), kind: RowKind::Header, path: None, index };
    let mut out = vec![header("Drives", 0)];
    out.extend(drives.iter().enumerate().map(|(i, (l, p))| Row { label: l.clone(), kind: RowKind::Drive, path: Some(p.clone()), index: i }));
    out.push(header("Places", 1));
    out.extend(places.iter().enumerate().map(|(i, (l, p))| Row { label: l.clone(), kind: RowKind::Place, path: Some(p.clone()), index: i }));
    // Recent (P12-M5), under Places, only when it holds a folder.
    if !recent.is_empty() {
        out.push(header("Recent", 2));
        out.extend(recent.iter().enumerate().map(|(i, p)| Row { label: crate::pathbar::title(p), kind: RowKind::Recent, path: Some(PathBuf::from(p)), index: i }));
    }
    out.push(header("Bookmarks", 3));
    out.extend(bookmarks.iter().enumerate().map(|(i, b)| Row {
        label: b.title.clone().unwrap_or_else(|| crate::pathbar::title(&b.path)),
        kind: RowKind::Bookmark,
        path: Some(PathBuf::from(&b.path)),
        index: i,
    }));
    out
}

/// The row current after a key in the sidebar (P7-M4): Up and Down move to the previous or next
/// row that is not a header (none past the ends: the row stays), Home and End to the first or
/// last such row; with no row current (-1), Up and Down start at the first. None when the sidebar
/// has no such row, or for any other key.
pub fn step(kinds: &[RowKind], current: i32, key: &str) -> Option<usize> {
    let rows: Vec<usize> = (0..kinds.len()).filter(|&i| kinds[i] != RowKind::Header).collect();
    let (first, last) = (*rows.first()?, *rows.last()?);
    let cur = usize::try_from(current).ok().filter(|&c| c < kinds.len());
    Some(match (key, cur) {
        ("Home", _) | ("Up" | "Down", None) => first,
        ("End", _) => last,
        ("Up", Some(c)) => rows.iter().rev().copied().find(|&i| i < c).unwrap_or(if kinds[c] == RowKind::Header { first } else { c }),
        ("Down", Some(c)) => rows.iter().copied().find(|&i| i > c).unwrap_or(if kinds[c] == RowKind::Header { last } else { c }),
        _ => return None,
    })
}

/// The top of row `i` in the sidebar list, from each row's height (`main.rs` `side_rows`).
pub fn row_top(heights: &[f32], i: usize) -> f32 {
    heights.iter().take(i).sum()
}

/// Folders compare case-insensitively and without a trailing separator (`C:\` keeps its own).
fn same_folder(a: &str, b: &str) -> bool {
    let n = |s: &str| {
        let t = s.trim();
        let t = if t.len() > 3 { t.trim_end_matches('\\') } else { t };
        t.to_lowercase()
    };
    n(a) == n(b)
}

/// A pane showed `folder` (P12-M5): it goes to the top of the Recent list (moved there when the list
/// holds it already, by the folder test bookmarks use), which keeps `RECENT_MAX`. False when it was
/// at the top already: nothing changed.
pub fn note_recent(list: &mut Vec<String>, folder: &str) -> bool {
    if folder.trim().is_empty() || list.first().is_some_and(|f| same_folder(f, folder)) {
        return false;
    }
    list.retain(|f| !same_folder(f, folder));
    list.insert(0, folder.to_string());
    list.truncate(RECENT_MAX);
    true
}

/// Add `folder` at the end unless it is there already. Returns whether it was added.
pub fn add_bookmark(list: &mut Vec<Bookmark>, folder: &str) -> bool {
    if folder.trim().is_empty() || list.iter().any(|b| same_folder(&b.path, folder)) {
        return false;
    }
    list.push(Bookmark::new(folder));
    true
}

/// Remove bookmark `i`, returning it.
pub fn remove_bookmark(list: &mut Vec<Bookmark>, i: usize) -> Option<Bookmark> {
    (i < list.len()).then(|| list.remove(i))
}

/// Give bookmark `i` the sidebar title `title` (trimmed); an empty one, or the folder's own name,
/// clears it, so the row shows the folder's name again. False when there is no bookmark `i`.
pub fn rename_bookmark(list: &mut [Bookmark], i: usize, title: &str) -> bool {
    let Some(b) = list.get_mut(i) else { return false };
    let t = title.trim();
    b.title = (!t.is_empty() && t != crate::pathbar::title(&b.path)).then(|| t.to_string());
    true
}

/// Move bookmark `i` one place up or down; its new index, or None at the edge (or no such
/// bookmark).
pub fn move_bookmark(list: &mut [Bookmark], i: usize, up: bool) -> Option<usize> {
    let to = if up { i.checked_sub(1)? } else { i + 1 };
    if i >= list.len() || to >= list.len() {
        return None;
    }
    list.swap(i, to);
    Some(to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_move_along_the_rows_past_the_headers() {
        use RowKind::*;
        // Drives: C:, G:; Places: Home; Bookmarks: none.
        let k = [Header, Drive, Drive, Header, Place, Header];
        assert_eq!(step(&k, 1, "Down"), Some(2));
        assert_eq!(step(&k, 2, "Down"), Some(4), "past the Places header");
        assert_eq!(step(&k, 4, "Down"), Some(4), "the last row stays (the Bookmarks header is no row)");
        assert_eq!(step(&k, 4, "Up"), Some(2));
        assert_eq!(step(&k, 1, "Up"), Some(1), "the first row stays");
        assert_eq!(step(&k, -1, "Down"), Some(1), "no row current: the first");
        assert_eq!(step(&k, -1, "Up"), Some(1));
        assert_eq!(step(&k, 4, "Home"), Some(1));
        assert_eq!(step(&k, 1, "End"), Some(4));
        assert_eq!(step(&k, 3, "Down"), Some(4), "from a header (a stale current row)");
        assert_eq!(step(&k, 9, "Up"), Some(1), "a current row past the end counts as none");
        assert_eq!(step(&k, 1, "Return"), None);
        assert_eq!(step(&[Header, Header], -1, "Down"), None, "nothing but headers");
        assert_eq!(row_top(&[26.0, 24.0, 24.0, 26.0, 24.0], 4), 100.0);
        assert_eq!(row_top(&[26.0, 24.0], 0), 0.0);
    }

    #[test]
    fn bookmarks_add_remove_dedupe() {
        let mut b = Vec::new();
        assert!(add_bookmark(&mut b, r"C:\Users\x"));
        assert!(!add_bookmark(&mut b, r"c:\users\X\"), "same folder, other case and a trailing separator");
        assert!(add_bookmark(&mut b, r"C:\"));
        assert!(!add_bookmark(&mut b, r"c:\"));
        assert!(!add_bookmark(&mut b, "  "));
        assert!(add_bookmark(&mut b, r"\\server\share"));
        let paths = |b: &[Bookmark]| b.iter().map(|b| b.path.clone()).collect::<Vec<_>>();
        assert_eq!(paths(&b), [r"C:\Users\x", r"C:\", r"\\server\share"]);
        assert_eq!(remove_bookmark(&mut b, 1).map(|b| b.path).as_deref(), Some(r"C:\"));
        assert_eq!(remove_bookmark(&mut b, 5), None);
        assert_eq!(paths(&b), [r"C:\Users\x", r"\\server\share"]);
    }

    #[test]
    fn recent_keeps_the_last_eight_newest_first() {
        let mut r: Vec<String> = Vec::new();
        assert!(note_recent(&mut r, r"C:\a"));
        assert!(note_recent(&mut r, r"C:\b"));
        assert_eq!(r, [r"C:\b", r"C:\a"], "newest first");
        assert!(!note_recent(&mut r, r"c:\B\"), "the same folder at the top: no change");
        assert!(note_recent(&mut r, r"c:\A"), "a folder further down moves to the top");
        assert_eq!(r, [r"c:\A", r"C:\b"], "once, as last written");
        assert!(!note_recent(&mut r, "  "));
        for i in 0..10 {
            note_recent(&mut r, &format!(r"C:\f{i}"));
        }
        assert_eq!(r.len(), RECENT_MAX);
        assert_eq!((r[0].as_str(), r[7].as_str()), (r"C:\f9", r"C:\f2"), "the oldest dropped");
    }

    #[test]
    fn bookmarks_rename_and_move() {
        let mut b = vec![Bookmark::new(r"C:\a"), Bookmark::new(r"C:\b"), Bookmark::new(r"C:\c")];
        assert!(rename_bookmark(&mut b, 1, "  Bee  "));
        assert_eq!(b[1].title.as_deref(), Some("Bee"));
        assert!(rename_bookmark(&mut b, 1, " "), "an empty title clears it");
        assert_eq!(b[1].title, None);
        assert!(rename_bookmark(&mut b, 2, "c"), "the folder's own name clears it too");
        assert_eq!(b[2].title, None);
        assert!(!rename_bookmark(&mut b, 3, "x"));
        rename_bookmark(&mut b, 0, "Alpha");
        let labels = |b: &[Bookmark]| compose(&[], &[], &[], b).into_iter().skip(3).map(|r| (r.label, r.index)).collect::<Vec<_>>();
        assert_eq!(labels(&b), [("Alpha".to_string(), 0), ("b".to_string(), 1), ("c".to_string(), 2)]);
        // Move down, then up; nothing past either end.
        assert_eq!(move_bookmark(&mut b, 0, false), Some(1));
        assert_eq!(labels(&b), [("b".to_string(), 0), ("Alpha".to_string(), 1), ("c".to_string(), 2)]);
        assert_eq!(move_bookmark(&mut b, 2, true), Some(1));
        assert_eq!(labels(&b), [("b".to_string(), 0), ("c".to_string(), 1), ("Alpha".to_string(), 2)]);
        assert_eq!(move_bookmark(&mut b, 0, true), None);
        assert_eq!(move_bookmark(&mut b, 2, false), None);
        assert_eq!(move_bookmark(&mut b, 7, true), None);
        assert_eq!(b[2].path, r"C:\a", "the title travels with its bookmark");
    }

    #[test]
    fn known_folder_list_composition() {
        let drives = vec![("Local Disk (C:)".to_string(), PathBuf::from(r"C:\"))];
        let places = vec![("Home".to_string(), PathBuf::from(r"C:\Users\x")), ("Documents".to_string(), PathBuf::from(r"C:\Users\x\Documents"))];
        let rows = compose(&drives, &places, &[], &[Bookmark::new(r"C:\work\proj")]);
        let shown: Vec<(&str, RowKind, usize)> = rows.iter().map(|r| (r.label.as_str(), r.kind, r.index)).collect();
        assert_eq!(
            shown,
            [
                ("Drives", RowKind::Header, 0),
                ("Local Disk (C:)", RowKind::Drive, 0),
                ("Places", RowKind::Header, 1),
                ("Home", RowKind::Place, 0),
                ("Documents", RowKind::Place, 1),
                ("Bookmarks", RowKind::Header, 3),
                ("proj", RowKind::Bookmark, 0),
            ]
        );
        assert_eq!(rows[6].path.as_deref(), Some(std::path::Path::new(r"C:\work\proj")));
        assert!(rows.iter().filter(|r| r.kind == RowKind::Header).all(|r| r.path.is_none()));
        // No bookmarks: the header stays. No recent folder: no Recent header.
        assert_eq!(compose(&[], &[], &[], &[]).len(), 3);
        // Recent folders (P12-M5): under Places, newest first, by their folder's name.
        let recent = vec![r"C:\work\proj".to_string(), r"D:\".to_string()];
        let rows = compose(&drives, &places, &recent, &[]);
        let shown: Vec<(&str, RowKind, usize)> = rows.iter().skip(4).map(|r| (r.label.as_str(), r.kind, r.index)).collect();
        assert_eq!(shown, [("Documents", RowKind::Place, 1), ("Recent", RowKind::Header, 2), ("proj", RowKind::Recent, 0), ("D:", RowKind::Recent, 1), ("Bookmarks", RowKind::Header, 3)]);
        assert_eq!(rows[6].path.as_deref(), Some(std::path::Path::new(r"C:\work\proj")));
        // The real known folders: every one this machine resolves is an existing folder.
        let known = known_folders();
        assert!(known.iter().any(|(l, _)| l == "Documents"));
        assert_eq!(known.first().map(|(l, _)| l.as_str()), Some("Home"));
        assert!(known.iter().all(|(_, p)| p.is_dir()), "{known:?}");
        // OneDrive, where this machine has it set up, comes right after Home.
        if let Some(i) = known.iter().position(|(l, _)| l == "OneDrive") {
            assert_eq!(i, 1, "{known:?}");
        }
        // Drive labels.
        let e = Entry::drive(r"C:\".into(), None, "Local Disk".into());
        assert_eq!(drive_label(&e), "Local Disk (C:)");
        let e = Entry::drive(r"D:\".into(), None, "Data (D:)".into());
        assert_eq!(drive_label(&e), "Data (D:)");
    }
}
