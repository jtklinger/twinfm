//! A finished search tab follows changes (P16-M6). The tab takes a subtree watch on its root, as a
//! sizes view does, and each window's records (`fs::watch::Event::Subtree`'s `deep`: FILE_ACTION_*
//! and the path under the root) are read here: `plan` nets them into the paths to look at again
//! (created, renamed to; for a content search modified too) and the paths that went (removed,
//! renamed away); `look`, on a worker thread, stats each path to look at, tests it against the
//! tab's matcher as the walk would (`fs::enumerate::walk`: no folder the hidden-file switches keep
//! out, no link, no sync root for a content search), reads it for a content search, and walks a
//! folder that arrived whole. The UI thread turns both into row edits (`main.rs` `land_follow`).

use crate::fs::enumerate::{Content, SEARCH_MAX_DEPTH, SEARCH_RUNNING, TypeNames, WalkStats, stat_entry, walk};
use crate::model::{Entry, Kind, LocalZone, NameMatch, Show, shows};
use std::collections::{HashMap, HashSet};
use std::path::Path;

const FILE_ACTION_ADDED: u32 = 1;
const FILE_ACTION_REMOVED: u32 = 2;
const FILE_ACTION_MODIFIED: u32 = 3;
const FILE_ACTION_RENAMED_OLD_NAME: u32 = 4;
const FILE_ACTION_RENAMED_NEW_NAME: u32 = 5;

/// What a window's records ask of a search tab's rows (paths under the root, as recorded).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// To stat and test: created or renamed to (a content search: modified too) and not removed
    /// again, nor under a folder removed later, in the order first recorded. True for one that
    /// arrived (created or renamed to): a folder that arrived is walked, since one moved in comes
    /// as one record; a folder only modified (an entry in it came or went) is not.
    pub look: Vec<(String, bool)>,
    /// Removed or renamed away at some point in the window: rows of these names and below them go
    /// (but one found again by `look` keeps its row).
    pub gone: Vec<String>,
}

/// The records of one or more windows, in order, netted. Paths are compared ignoring case.
pub fn plan(records: &[(u32, String)], content: bool) -> Plan {
    // Lowercase path -> (the record that first put it there, the path as recorded, it arrived).
    let mut look: HashMap<String, (usize, &str, bool)> = HashMap::new();
    let mut gone: HashMap<String, (usize, &str)> = HashMap::new();
    for (i, (action, path)) in records.iter().enumerate() {
        let key = path.to_lowercase();
        match *action {
            FILE_ACTION_ADDED | FILE_ACTION_RENAMED_NEW_NAME => {
                // The name as it is now (a rename may change only its letter case).
                let at = look.get(&key).map_or(i, |v| v.0);
                look.insert(key, (at, path, true));
            }
            FILE_ACTION_MODIFIED if content => {
                look.entry(key).or_insert((i, path, false));
            }
            FILE_ACTION_REMOVED | FILE_ACTION_RENAMED_OLD_NAME => {
                look.remove(&key);
                gone.insert(key, (i, path));
            }
            _ => {}
        }
    }
    // A path put there before a folder above it went is gone with the folder.
    let removed_later = |key: &str, at: usize| key.match_indices('\\').any(|(k, _)| gone.get(&key[..k]).is_some_and(|(g, _)| *g > at));
    let mut l: Vec<(usize, &str, bool)> = look.iter().filter(|(k, (at, ..))| !removed_later(k, *at)).map(|(_, v)| *v).collect();
    l.sort_unstable();
    let mut g: Vec<(usize, &str)> = gone.values().copied().collect();
    g.sort_unstable();
    Plan { look: l.into_iter().map(|(_, p, arrived)| (p.to_string(), arrived)).collect(), gone: g.into_iter().map(|(_, p)| p.to_string()).collect() }
}

/// `name` (a row's path under the root) is one of `gone` (lowercase paths) or below one.
pub fn under_any(name: &str, gone: &HashSet<String>) -> bool {
    if gone.is_empty() {
        return false;
    }
    let key = name.to_lowercase();
    gone.contains(&key) || key.match_indices('\\').any(|(k, _)| gone.contains(&key[..k]))
}

/// What `look` found.
#[derive(Debug, Default)]
pub struct Looked {
    /// Matches, each named by its path under the root, as the walk names its results.
    pub found: Vec<Entry>,
    /// Paths looked at that a row may still show and that are no match now (gone again, behind a
    /// folder the walk does not enter, the text no longer in them): a row of that name goes. A
    /// name search's path whose name does not match is not among them: no row ever had it.
    pub missing: Vec<String>,
    /// Files a content search opened.
    pub read: usize,
}

/// Look at `paths` (`Plan::look`, under `root`, which is absolute: `stat_dir`) as the search's
/// walk would have met them. A folder that arrived is also walked.
pub fn look(root: &Path, paths: &[(String, bool)], matcher: &NameMatch, content: Option<&Content>, show: Show, thumbnails: bool) -> Looked {
    let mut out = Looked::default();
    let mut types = TypeNames::default();
    let zone = LocalZone::current();
    let mut stats = WalkStats::default();
    // Folder under the root (lowercase) -> the walk enters it.
    let mut entered: HashMap<String, bool> = HashMap::new();
    for (p, arrived) in paths {
        let (rel_dir, leaf) = p.rsplit_once('\\').unwrap_or(("", p.as_str()));
        if !enters(root, rel_dir, show, content, &mut entered, &mut types, &zone) {
            out.missing.push(p.clone());
            continue;
        }
        let dir = root.join(rel_dir);
        let mut keys = crate::icons::IconKeys::new(dir.clone(), thumbnails);
        let Some(mut e) = stat_entry(&dir, leaf, &mut types, &mut keys, &zone) else {
            out.missing.push(p.clone());
            continue;
        };
        // A folder's own entry modified (something in it came or went) says nothing of the folder.
        if e.is_dir() && !arrived {
            continue;
        }
        let path = dir.join(leaf);
        let walk_it = e.kind == Kind::Directory && depth(p) < SEARCH_MAX_DEPTH && shows(&e, show) && !content.is_some_and(|c| c.in_cloud(&path));
        let hit = match content {
            None => matcher.matches(leaf),
            Some(c) => matcher.matches(leaf) && !e.is_dir() && c.holds(&mut e, &path, &mut stats),
        };
        if hit {
            e.name = p.as_str().into();
            out.found.push(e);
        } else if content.is_some() {
            out.missing.push(p.clone());
        }
        if walk_it {
            let running = std::sync::atomic::AtomicU8::new(SEARCH_RUNNING);
            let below = walk(&path, matcher, show, thumbnails, content, &running, |batch| {
                out.found.extend(batch.into_iter().map(|mut e| {
                    e.name = format!("{p}\\{}", e.name).into();
                    e
                }));
                true
            });
            if let Ok(s) = below {
                stats.read += s.read;
            }
        }
    }
    out.read = stats.read;
    out
}

/// Folders above a path's last name.
fn depth(rel: &str) -> usize {
    rel.matches('\\').count() + 1
}

/// The walk would have entered folder `rel` under the root (the root itself: always): every
/// folder on the way is a plain directory (no link, no placeholder) that the hidden-file
/// switches let through, not past the depth limit, and for a content search not under a sync root.
fn enters(root: &Path, rel: &str, show: Show, content: Option<&Content>, known: &mut HashMap<String, bool>, types: &mut TypeNames, zone: &LocalZone) -> bool {
    if rel.is_empty() {
        return true;
    }
    let key = rel.to_lowercase();
    if let Some(&yes) = known.get(&key) {
        return yes;
    }
    let (above, leaf) = rel.rsplit_once('\\').unwrap_or(("", rel));
    let yes = depth(rel) <= SEARCH_MAX_DEPTH && enters(root, above, show, content, known, types, zone) && {
        let dir = root.join(above);
        let mut keys = crate::icons::IconKeys::new(dir.clone(), false);
        stat_entry(&dir, leaf, types, &mut keys, zone)
            .is_some_and(|e| e.kind == Kind::Directory && shows(&e, show) && !content.is_some_and(|c| c.in_cloud(&dir.join(leaf))))
    };
    known.insert(key, yes);
    yes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recs(v: &[(u32, &str)]) -> Vec<(u32, String)> {
        v.iter().map(|(a, p)| (*a, p.to_string())).collect()
    }

    #[test]
    fn a_windows_records_are_netted() {
        const ADD: u32 = FILE_ACTION_ADDED;
        const DEL: u32 = FILE_ACTION_REMOVED;
        const MODIFIED: u32 = FILE_ACTION_MODIFIED;
        const OLD: u32 = FILE_ACTION_RENAMED_OLD_NAME;
        const NEW: u32 = FILE_ACTION_RENAMED_NEW_NAME;
        let p = |v: &[(u32, &str)], content: bool| plan(&recs(v), content);
        // A looked-at path written with a trailing `~` is one only modified.
        let plan_of = |look: &[&str], gone: &[&str]| Plan {
            look: look.iter().map(|s| (s.trim_end_matches('~').to_string(), !s.ends_with('~'))).collect(),
            gone: gone.iter().map(|s| s.to_string()).collect(),
        };
        assert_eq!(p(&[], false), Plan::default());
        // Created, removed, renamed: the new name is looked at, the old one goes.
        assert_eq!(p(&[(ADD, r"a\new.txt"), (DEL, "old.txt"), (OLD, r"b\x.txt"), (NEW, r"b\y.txt")], false), plan_of(&[r"a\new.txt", r"b\y.txt"], &["old.txt", r"b\x.txt"]));
        // A modified file says nothing to a name search; a content search reads it again, once.
        let v = [(MODIFIED, "m.txt"), (MODIFIED, "M.TXT"), (ADD, "n.txt"), (MODIFIED, "n.txt")];
        assert_eq!(p(&v, false), plan_of(&["n.txt"], &[]));
        assert_eq!(p(&v, true), plan_of(&["m.txt~", "n.txt"], &[]));
        // Modified, then renamed to (a save through a temporary name): it arrived.
        assert_eq!(p(&[(MODIFIED, "s.txt"), (OLD, "s.tmp"), (NEW, "S.txt")], true), plan_of(&["S.txt"], &["s.tmp"]));
        // A folder whose entry is modified because a file in it was made: only the file arrived.
        assert_eq!(p(&[(ADD, r"src\new.rs"), (MODIFIED, r"src\new.rs"), (MODIFIED, "src")], true), plan_of(&[r"src\new.rs", "src~"], &[]));
        // Made and removed within the window: nothing to look at, and it goes (no row has it).
        assert_eq!(p(&[(ADD, "t.tmp"), (MODIFIED, "t.tmp"), (DEL, "T.tmp")], true), plan_of(&[], &["T.tmp"]));
        // Saved by removing and writing again (or a rename over it): it goes and is looked at.
        assert_eq!(p(&[(DEL, "doc.txt"), (OLD, "doc.tmp"), (NEW, "doc.txt")], false), plan_of(&["doc.txt"], &["doc.txt", "doc.tmp"]));
        // A file made in a folder that then went is gone with it; one made after the folder came
        // back is looked at.
        assert_eq!(p(&[(ADD, r"d\f.txt"), (DEL, "d")], false), plan_of(&[], &["d"]));
        assert_eq!(p(&[(ADD, r"d\f.txt"), (DEL, "D"), (ADD, "d"), (ADD, r"d\g.txt")], false), plan_of(&["d", r"d\g.txt"], &["D"]));
        // A folder renamed: its old name goes (and every row below it), its new name is looked at.
        assert_eq!(p(&[(OLD, "src"), (NEW, "source")], false), plan_of(&["source"], &["src"]));
        // Unknown actions are skipped.
        assert_eq!(p(&[(9, "x")], true), Plan::default());
    }

    #[test]
    fn rows_below_a_gone_path_go() {
        let gone: HashSet<String> = [r"src\old", "a.txt"].iter().map(|s| s.to_string()).collect();
        assert!(under_any("a.txt", &gone) && under_any("A.TXT", &gone));
        assert!(under_any(r"src\old", &gone) && under_any(r"SRC\Old\deep\f.rs", &gone));
        assert!(!under_any(r"src\older\f.rs", &gone), "a longer name is another folder");
        assert!(!under_any(r"src\a.txt", &gone) && !under_any("b.txt", &gone) && !under_any("src", &gone));
        assert!(!under_any("a.txt", &HashSet::new()));
    }

    /// A real tree: what `look` finds is what a walk of the same tree finds for those paths.
    #[test]
    fn looked_at_paths_match_as_the_walk_does() {
        let root = std::env::temp_dir().join(format!("twinfm-p16m6-look-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(r"sub\deep")).unwrap();
        std::fs::create_dir_all(root.join("hid")).unwrap();
        std::fs::create_dir_all(root.join(r"moved\in")).unwrap();
        std::fs::write(root.join("top.txt"), "needle at the top").unwrap();
        std::fs::write(root.join("other.log"), "no match by name").unwrap();
        std::fs::write(root.join(r"sub\deep\f.txt"), "nothing here").unwrap();
        std::fs::write(root.join(r"hid\h.txt"), "needle hidden").unwrap();
        std::fs::write(root.join(r"moved\m.txt"), "a Needle").unwrap();
        std::fs::write(root.join(r"moved\in\n.txt"), "x").unwrap();
        let hide = std::process::Command::new("attrib").arg("+h").arg(root.join("hid")).status().map(|s| s.success()).unwrap_or(false);
        let linked = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(root.join("link"))
            .arg(root.join("sub"))
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        // Every path arrived, but `sub\deep`, which was only modified.
        let paths: Vec<(String, bool)> = ["top.txt", "other.log", r"sub\deep\f.txt", r"hid\h.txt", "gone.txt", "moved", r"link\deep\f.txt", "sub", r"sub\deep~"]
            .iter()
            .map(|s| (s.trim_end_matches('~').to_string(), !s.ends_with('~')))
            .collect();
        let names = |l: &Looked| {
            let mut v: Vec<String> = l.found.iter().map(|e| e.name.to_string()).collect();
            v.sort();
            v
        };
        // A name search for *.txt: the files by their own names, the folder that arrived walked.
        let by_name = look(&root, &paths, &NameMatch::new("*.txt"), None, Show::OFF, false);
        // The hidden folder's file with Ctrl+H on.
        let with_hidden = look(&root, &[(r"hid\h.txt".to_string(), true)], &NameMatch::new("*.txt"), None, Show::HIDDEN, false);
        // A name search that matches folders finds the folder itself and what is below it.
        let folders = look(&root, &[("moved".to_string(), true)], &NameMatch::new("*m*"), None, Show::OFF, false);
        // A content search: the files that hold the text, read once each.
        let content = Content { needle: crate::fs::content::Needle::Text(crate::fs::content::fold("needle")), cloud_roots: Vec::new() };
        let by_text = look(&root, &paths, &NameMatch::default(), Some(&content), Show::OFF, false);
        std::fs::remove_dir_all(&root).unwrap();

        if hide {
            assert_eq!(names(&by_name), [r"moved\in\n.txt", r"moved\m.txt", r"sub\deep\f.txt", r"sub\deep\f.txt", "top.txt"], "sub is walked too");
        }
        let mut missing = by_name.missing.clone();
        missing.sort();
        let mut want = vec!["gone.txt"];
        if hide {
            want.push(r"hid\h.txt");
        }
        if linked {
            want.push(r"link\deep\f.txt");
        }
        want.sort();
        if hide && linked {
            assert_eq!(missing, want, "gone, behind a hidden folder or a link");
        }
        assert_eq!(names(&with_hidden), [r"hid\h.txt"]);
        assert_eq!(names(&folders), ["moved", r"moved\m.txt"]);
        assert_eq!(names(&by_text), [r"moved\m.txt", "top.txt"]);
        assert_eq!(by_text.found.iter().find(|e| e.name == "top.txt").map(|e| e.found.to_string()).as_deref(), Some("needle at the top"));
        assert_eq!(by_text.read, 6, "top.txt, other.log, f.txt, moved's two files, f.txt again below sub; nothing for sub\\deep, only modified");
    }
}
