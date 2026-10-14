//! The path bar's breadcrumb: one button per path component, built from the path when a tab
//! navigates (never per frame). Pure, so the splitting is unit-tested.

use crate::fs::enumerate::{THIS_PC, is_this_pc};
use std::path::{Component, Path, PathBuf};

/// One breadcrumb button: what it reads and the folder a click opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crumb {
    pub label: String,
    pub target: PathBuf,
}

/// "This PC" first, then the drive or UNC share, then each folder: `C:\Users\x` gives
/// This PC > C: > Users > x, `\\server\share\dir` gives This PC > \\server\share > dir. Going up
/// from a drive or share root lands on This PC (main.rs `go_up`), so it heads every path.
pub fn crumbs(path: &str) -> Vec<Crumb> {
    let mut out = vec![Crumb { label: THIS_PC.into(), target: PathBuf::from(THIS_PC) }];
    if is_this_pc(path) {
        return out;
    }
    let mut acc = PathBuf::new();
    for c in Path::new(path.trim()).components() {
        match c {
            Component::Prefix(p) => {
                let label = p.as_os_str().to_string_lossy().trim_end_matches('\\').to_string();
                acc = PathBuf::from(format!("{label}\\"));
                out.push(Crumb { label, target: acc.clone() });
            }
            // The root separator after a prefix is already in `acc`.
            Component::RootDir => {
                if acc.as_os_str().is_empty() {
                    acc = PathBuf::from("\\");
                }
            }
            Component::Normal(n) => {
                acc.push(n);
                out.push(Crumb { label: n.to_string_lossy().into(), target: acc.clone() });
            }
            Component::CurDir | Component::ParentDir => {
                acc.push(c.as_os_str());
                out.push(Crumb { label: c.as_os_str().to_string_lossy().into(), target: acc.clone() });
            }
        }
    }
    out
}

/// A tab's title: the folder's own name, the drive ("C:") or share at a root, "This PC" for the
/// drive list.
pub fn title(path: &str) -> String {
    crumbs(path).pop().map(|c| c.label).unwrap_or_else(|| THIS_PC.into())
}

/// The most folders a crumb's dropdown lists; the rest is a "..." note.
pub const MENU_CAP: usize = 200;

/// For a crumb's dropdown (the chevron after it): the subfolders of `dir`, sorted by name as the
/// list sorts names, hidden and system folders left out unless `show_hidden`, at most `cap` of
/// them, with the count of all of them. "This PC" lists the drives ("C:\"). Reads the disk, so
/// it runs on a worker thread.
pub fn subfolders(dir: &Path, show_hidden: bool, cap: usize) -> Result<(Vec<String>, usize), String> {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM};
    if is_this_pc(&dir.to_string_lossy()) {
        let drives: Vec<String> = crate::fs::enumerate::drive_entries().into_iter().map(|e| e.name.to_string()).collect();
        let total = drives.len();
        return Ok((drives.into_iter().take(cap).collect(), total));
    }
    // A zip archive's crumb lists the folders inside it.
    if let Some((zip, inner)) = crate::fs::archive::split(dir) {
        return crate::fs::archive::subfolders(&zip, &inner, cap);
    }
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|de| {
            // Served from the FindNextFile data, as in fs::enumerate: no extra call per entry.
            let attrs = de.metadata().map(|m| m.file_attributes()).unwrap_or(0);
            attrs & FILE_ATTRIBUTE_DIRECTORY != 0 && (show_hidden || attrs & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) == 0)
        })
        .map(|de| de.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort_by(|a, b| crate::model::natural_cmp(a, b));
    let total = names.len();
    names.truncate(cap);
    Ok((names, total))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(p: &str) -> Vec<String> {
        crumbs(p).into_iter().map(|c| c.label).collect()
    }

    fn targets(p: &str) -> Vec<String> {
        crumbs(p).into_iter().map(|c| c.target.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn breadcrumb_splitting() {
        assert_eq!(labels("This PC"), ["This PC"]);
        assert_eq!(labels(""), ["This PC"]);
        assert_eq!(labels("this pc"), ["This PC"]);
        assert_eq!(labels(r"C:\"), ["This PC", "C:"]);
        assert_eq!(targets(r"C:\"), ["This PC", r"C:\"]);
        assert_eq!(labels(r"C:\Users\x"), ["This PC", "C:", "Users", "x"]);
        assert_eq!(targets(r"C:\Users\x"), ["This PC", r"C:\", r"C:\Users", r"C:\Users\x"]);
        assert_eq!(labels(r"C:\Users\x\"), ["This PC", "C:", "Users", "x"], "a trailing separator adds nothing");
        assert_eq!(labels(r"\\server\share\dir"), ["This PC", r"\\server\share", "dir"]);
        assert_eq!(targets(r"\\server\share\dir"), ["This PC", r"\\server\share\", r"\\server\share\dir"]);
        assert_eq!(title(r"C:\Users\x"), "x");
        assert_eq!(title(r"C:\"), "C:");
        assert_eq!(title(r"\\server\share"), r"\\server\share");
        assert_eq!(title("This PC"), "This PC");
    }

    #[test]
    fn subfolders_for_the_crumb_dropdown() {
        use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM, SetFileAttributesW};
        let dir = std::env::temp_dir().join(format!("twinfm-p2m3-crumbs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for d in ["b", "a10", "a2", "hid", "sys"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        std::fs::write(dir.join("a1.txt"), b"a file, not a folder").unwrap();
        for (d, attr) in [("hid", FILE_ATTRIBUTE_HIDDEN), ("sys", FILE_ATTRIBUTE_SYSTEM)] {
            let w = crate::shellmenu::wide(dir.join(d).as_os_str());
            // SAFETY: a NUL-terminated wide path to a folder this test made.
            assert_ne!(unsafe { SetFileAttributesW(w.as_ptr(), attr) }, 0);
        }
        // Folders only, natural name order, hidden and system left out.
        assert_eq!(subfolders(&dir, false, MENU_CAP).unwrap(), (vec!["a2".into(), "a10".into(), "b".into()], 3));
        // With hidden files shown they are listed too.
        let (all, n) = subfolders(&dir, true, MENU_CAP).unwrap();
        assert_eq!((all, n), (vec!["a2".into(), "a10".into(), "b".into(), "hid".into(), "sys".into()], 5));
        // Capped: the first `cap` and the full count.
        assert_eq!(subfolders(&dir, true, 2).unwrap(), (vec!["a2".into(), "a10".into()], 5));
        // A folder that is gone is an error, not an empty list.
        assert!(subfolders(&dir.join("missing"), false, MENU_CAP).is_err());
        // This PC lists the drives.
        let (drives, _) = subfolders(Path::new("This PC"), false, MENU_CAP).unwrap();
        assert!(drives.iter().any(|d| d.eq_ignore_ascii_case(r"C:\")), "{drives:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
