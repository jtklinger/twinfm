//! The sidebar: Drives, Places (known folders) and Bookmarks, as one flat list of rows with
//! section headers. Built on the UI thread only when something in it changes (shown, F5, a
//! bookmark added or removed, the drive list arriving from its worker); the composition and the
//! bookmark list rules are pure and unit-tested.

use crate::model::Entry;
use std::path::PathBuf;
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Music, FOLDERID_Pictures, FOLDERID_Profile,
    FOLDERID_Videos, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
};
use windows_sys::core::GUID;

/// What a row is; the discriminants are the `kind` int the Slint `SideItem` carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    Header = 0,
    Drive = 1,
    Place = 2,
    Bookmark = 3,
}

/// One sidebar row: its text, kind, the folder a click opens (none for a header) and, for a
/// bookmark, its index in the bookmark list.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub label: String,
    pub kind: RowKind,
    pub path: Option<PathBuf>,
    pub index: usize,
}

/// The Places section, in order: label and known folder.
const PLACES: [(&str, GUID); 7] = [
    ("Home", FOLDERID_Profile),
    ("Desktop", FOLDERID_Desktop),
    ("Documents", FOLDERID_Documents),
    ("Downloads", FOLDERID_Downloads),
    ("Pictures", FOLDERID_Pictures),
    ("Music", FOLDERID_Music),
    ("Videos", FOLDERID_Videos),
];

/// The known folders that resolve on this machine (a folder the shell does not know is left out).
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
            path.map(|p| (label.to_string(), p))
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
pub fn compose(drives: &[(String, PathBuf)], places: &[(String, PathBuf)], bookmarks: &[String]) -> Vec<Row> {
    let header = |label: &str| Row { label: label.into(), kind: RowKind::Header, path: None, index: 0 };
    let mut out = vec![header("Drives")];
    out.extend(drives.iter().enumerate().map(|(i, (l, p))| Row { label: l.clone(), kind: RowKind::Drive, path: Some(p.clone()), index: i }));
    out.push(header("Places"));
    out.extend(places.iter().enumerate().map(|(i, (l, p))| Row { label: l.clone(), kind: RowKind::Place, path: Some(p.clone()), index: i }));
    out.push(header("Bookmarks"));
    out.extend(bookmarks.iter().enumerate().map(|(i, b)| Row {
        label: crate::pathbar::title(b),
        kind: RowKind::Bookmark,
        path: Some(PathBuf::from(b)),
        index: i,
    }));
    out
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

/// Add `folder` at the end unless it is there already. Returns whether it was added.
pub fn add_bookmark(list: &mut Vec<String>, folder: &str) -> bool {
    if folder.trim().is_empty() || list.iter().any(|b| same_folder(b, folder)) {
        return false;
    }
    list.push(folder.to_string());
    true
}

/// Remove bookmark `i`, returning it.
pub fn remove_bookmark(list: &mut Vec<String>, i: usize) -> Option<String> {
    (i < list.len()).then(|| list.remove(i))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bookmarks_add_remove_dedupe() {
        let mut b = Vec::new();
        assert!(add_bookmark(&mut b, r"C:\Users\x"));
        assert!(!add_bookmark(&mut b, r"c:\users\X\"), "same folder, other case and a trailing separator");
        assert!(add_bookmark(&mut b, r"C:\"));
        assert!(!add_bookmark(&mut b, r"c:\"));
        assert!(!add_bookmark(&mut b, "  "));
        assert!(add_bookmark(&mut b, r"\\server\share"));
        assert_eq!(b, [r"C:\Users\x", r"C:\", r"\\server\share"]);
        assert_eq!(remove_bookmark(&mut b, 1).as_deref(), Some(r"C:\"));
        assert_eq!(remove_bookmark(&mut b, 5), None);
        assert_eq!(b, [r"C:\Users\x", r"\\server\share"]);
    }

    #[test]
    fn known_folder_list_composition() {
        let drives = vec![("Local Disk (C:)".to_string(), PathBuf::from(r"C:\"))];
        let places = vec![("Home".to_string(), PathBuf::from(r"C:\Users\x")), ("Documents".to_string(), PathBuf::from(r"C:\Users\x\Documents"))];
        let rows = compose(&drives, &places, &[r"C:\work\proj".to_string()]);
        let shown: Vec<(&str, RowKind, usize)> = rows.iter().map(|r| (r.label.as_str(), r.kind, r.index)).collect();
        assert_eq!(
            shown,
            [
                ("Drives", RowKind::Header, 0),
                ("Local Disk (C:)", RowKind::Drive, 0),
                ("Places", RowKind::Header, 0),
                ("Home", RowKind::Place, 0),
                ("Documents", RowKind::Place, 1),
                ("Bookmarks", RowKind::Header, 0),
                ("proj", RowKind::Bookmark, 0),
            ]
        );
        assert_eq!(rows[6].path.as_deref(), Some(std::path::Path::new(r"C:\work\proj")));
        assert!(rows.iter().filter(|r| r.kind == RowKind::Header).all(|r| r.path.is_none()));
        // No bookmarks: the header stays.
        assert_eq!(compose(&[], &[], &[]).len(), 3);
        // The real known folders: every one this machine resolves is an existing folder.
        let known = known_folders();
        assert!(known.iter().any(|(l, _)| l == "Documents"));
        assert_eq!(known.first().map(|(l, _)| l.as_str()), Some("Home"));
        assert!(known.iter().all(|(_, p)| p.is_dir()), "{known:?}");
        // Drive labels.
        let e = Entry::drive(r"C:\".into(), None, "Local Disk".into());
        assert_eq!(drive_label(&e), "Local Disk (C:)");
        let e = Entry::drive(r"D:\".into(), None, "Data (D:)".into());
        assert_eq!(drive_label(&e), "Data (D:)");
    }
}
