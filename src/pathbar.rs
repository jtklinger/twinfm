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
}
