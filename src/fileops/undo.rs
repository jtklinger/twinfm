//! TwinFM's own undo record (Ctrl+Z). There is no public API to invoke Explorer's undo stack
//! (`FOFX_ADDUNDORECORD` feeds it, but only Explorer can play it back), so every successful job
//! leaves a `Record` built from the real names the progress sink reported, and Ctrl+Z submits
//! its inverse through the ordinary worker path:
//!
//! - Move: move the moved items back to the folder they came from; when the shell renamed an
//!   item on collision ("a (2).txt"), a `MoveAs` that gives it its old name back.
//! - Move under new names (`MoveAs`): the same `MoveAs` back.
//! - Rename: rename the item back to its old name.
//! - New folder: recycle the folder, only if it is still empty.
//! - Copy: recycle the copies.
//! - Delete to the Recycle Bin: restore the items from the bin (`Restore`, from the `$R` paths
//!   the sink reported); a recycle that reported none records a note that undoes nothing. A
//!   permanent delete records nothing.
//! - Restore: recycle the restored items again.
//!
//! An undo job's own record is the redo: the app keeps a second stack (capped by `push` too), and
//! when an undo job completes `Done` it pushes `record_for(&undo_job, ..)` there; a redo job's
//! record goes back onto the undo stack the same way, and any other job clears the redo stack.
//! Pure apart from `check`, which looks at the disk at undo time.

use super::{ItemResult, Job, Outcome, same_dir};
use std::path::PathBuf;

/// Records kept; the oldest goes first.
pub const CAP: usize = 20;

/// How to undo one job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Undo {
    /// The job that reverses it.
    pub job: Job,
    /// Paths that must still exist where the original job left them, or the undo is refused.
    pub expect: Vec<PathBuf>,
    /// Undoing a new folder: the folder must still be empty.
    pub must_be_empty: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// None: the job cannot be undone (a recycle the bin reported no items for); Ctrl+Z says so.
    pub undo: Option<Undo>,
    /// The job's status text, for "Undid: <text>".
    pub text: String,
}

/// The note Ctrl+Z shows for a recycle whose items the bin did not report.
pub const CANNOT_UNDO_DELETE: &str = "Cannot undo: the Recycle Bin reported no items";

/// The reverse of `job` from the items the sink reported, or None when nothing can be undone.
pub fn undo_for(job: &Job, results: &[ItemResult]) -> Option<Undo> {
    let made: Vec<(PathBuf, PathBuf)> =
        results.iter().filter(|r| r.ok()).filter_map(|r| Some((r.source.clone(), r.dest.clone()?))).collect();
    if made.is_empty() {
        return None;
    }
    let dests: Vec<PathBuf> = made.iter().map(|(_, d)| d.clone()).collect();
    // The items back under their old names (the sources' names), into `from`.
    let move_as_back = |from: &std::path::Path| -> Option<Undo> {
        let items = made.iter().map(|(s, d)| Some((d.clone(), s.file_name()?.to_string_lossy().into_owned()))).collect::<Option<Vec<_>>>()?;
        Some(Undo { job: Job::MoveAs { items, dest_dir: from.to_path_buf() }, expect: dests.clone(), must_be_empty: false })
    };
    match job {
        Job::Move { sources, .. } => {
            // One folder to move them back to (a paste can gather files from several).
            let from = sources.first()?.parent()?;
            if !sources.iter().all(|s| s.parent().is_some_and(|p| same_dir(p, from))) {
                return None;
            }
            if made.iter().any(|(s, d)| s.file_name() != d.file_name()) {
                return move_as_back(from); // renamed on collision
            }
            Some(Undo { job: Job::Move { sources: dests.clone(), dest_dir: from.to_path_buf() }, expect: dests, must_be_empty: false })
        }
        Job::MoveAs { items, .. } => {
            let from = items.first()?.0.parent()?;
            if !items.iter().all(|(s, _)| s.parent().is_some_and(|p| same_dir(p, from))) {
                return None;
            }
            move_as_back(from)
        }
        Job::Delete { permanent: false, .. } => {
            Some(Undo { job: Job::Restore { items: made.iter().map(|(s, d)| (d.clone(), s.clone())).collect() }, expect: dests, must_be_empty: false })
        }
        Job::Restore { .. } => Some(Undo { job: Job::Delete { sources: dests.clone(), permanent: false }, expect: dests, must_be_empty: false }),
        Job::Copy { .. } => Some(Undo { job: Job::Delete { sources: dests.clone(), permanent: false }, expect: dests, must_be_empty: false }),
        Job::Rename { source, .. } => {
            let old = source.file_name()?.to_string_lossy().into_owned();
            let (_, now) = made.into_iter().next()?;
            Some(Undo { job: Job::Rename { source: now.clone(), new_name: old }, expect: vec![now], must_be_empty: false })
        }
        Job::NewFolder { .. } => {
            let (_, folder) = made.into_iter().next()?;
            Some(Undo { job: Job::Delete { sources: vec![folder.clone()], permanent: false }, expect: vec![folder], must_be_empty: true })
        }
        Job::Delete { permanent: true, .. } => None,
    }
}

/// What a finished job leaves on the undo stack (or, for an undo job, on the redo stack): only
/// successful jobs leave anything; a recycle the bin reported no items for leaves a record that
/// undoes nothing, a permanent delete none.
pub fn record_for(job: &Job, outcome: &Outcome, results: &[ItemResult], text: &str) -> Option<Record> {
    if *outcome != Outcome::Done {
        return None;
    }
    match job {
        Job::Delete { permanent: true, .. } => None,
        Job::Delete { permanent: false, .. } => Some(Record { undo: undo_for(job, results), text: text.into() }),
        _ => Some(Record { undo: Some(undo_for(job, results)?), text: text.into() }),
    }
}

/// Push a record, dropping the oldest beyond `CAP`.
pub fn push(stack: &mut Vec<Record>, r: Record) {
    stack.push(r);
    if stack.len() > CAP {
        stack.remove(0);
    }
}

/// At undo time: the items are still where the job left them (a restore's `$R` paths still in
/// the bin, a new folder still empty), or why not.
pub fn check(u: &Undo) -> Result<(), String> {
    let name = |p: &PathBuf| p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned());
    for p in &u.expect {
        if std::fs::symlink_metadata(p).is_err() {
            return Err(format!("Cannot undo: {} is no longer there", name(p)));
        }
        if u.must_be_empty && std::fs::read_dir(p).map_or(true, |mut d| d.next().is_some()) {
            return Err(format!("Cannot undo: {} is not empty", name(p)));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(source: &str, dest: &str) -> ItemResult {
        ItemResult { source: source.into(), dest: Some(dest.into()), hr: 0 }
    }

    #[test]
    fn which_job_undoes_which() {
        let mv = Job::Move { sources: vec![r"C:\a\x.txt".into(), r"C:\a\y.txt".into()], dest_dir: r"C:\b".into() };
        let res = [ok(r"C:\a\x.txt", r"C:\b\x.txt"), ok(r"C:\a\y.txt", r"C:\b\y.txt")];
        let u = undo_for(&mv, &res).unwrap();
        assert_eq!(u.job, Job::Move { sources: vec![r"C:\b\x.txt".into(), r"C:\b\y.txt".into()], dest_dir: r"C:\a".into() });
        assert_eq!(u.expect.len(), 2);
        // A failed item is not moved back.
        let partial = [ok(r"C:\a\x.txt", r"C:\b\x.txt"), ItemResult { hr: -1, ..ok(r"C:\a\y.txt", r"C:\b\y.txt") }];
        assert_eq!(undo_for(&mv, &partial).unwrap().expect, [PathBuf::from(r"C:\b\x.txt")]);
        // Sources from two folders: no single place to move them back to.
        let mixed = Job::Move { sources: vec![r"C:\a\x.txt".into(), r"C:\c\y.txt".into()], dest_dir: r"C:\b".into() };
        assert_eq!(undo_for(&mixed, &res), None);

        let ren = Job::Rename { source: r"C:\a\old.txt".into(), new_name: "new.txt".into() };
        let u = undo_for(&ren, &[ok(r"C:\a\old.txt", r"C:\a\new.txt")]).unwrap();
        assert_eq!(u.job, Job::Rename { source: r"C:\a\new.txt".into(), new_name: "old.txt".into() });

        let nf = Job::NewFolder { parent: r"C:\a".into(), name: "New folder".into() };
        let u = undo_for(&nf, &[ok(r"C:\a", r"C:\a\New folder (2)")]).unwrap();
        assert_eq!(u.job, Job::Delete { sources: vec![r"C:\a\New folder (2)".into()], permanent: false }, "the real name");
        assert!(u.must_be_empty);

        let cp = Job::Copy { sources: vec![r"C:\a\x.txt".into()], dest_dir: r"C:\a".into() };
        let u = undo_for(&cp, &[ok(r"C:\a\x.txt", r"C:\a\x - Copy.txt")]).unwrap();
        assert_eq!(u.job, Job::Delete { sources: vec![r"C:\a\x - Copy.txt".into()], permanent: false });
        assert!(!u.must_be_empty);

        assert_eq!(undo_for(&cp, &[]), None, "nothing reported, nothing to undo");
    }

    #[test]
    fn moves_under_new_names_recycles_and_restores() {
        // The shell renamed one item on collision: the undo gives it its old name back.
        let mv = Job::Move { sources: vec![r"C:\a\a.txt".into(), r"C:\a\b.txt".into()], dest_dir: r"C:\b".into() };
        let res = [ok(r"C:\a\a.txt", r"C:\b\a (2).txt"), ok(r"C:\a\b.txt", r"C:\b\b.txt")];
        let u = undo_for(&mv, &res).unwrap();
        let back = Job::MoveAs {
            items: vec![(r"C:\b\a (2).txt".into(), "a.txt".into()), (r"C:\b\b.txt".into(), "b.txt".into())],
            dest_dir: r"C:\a".into(),
        };
        assert_eq!(u.job, back);
        assert_eq!(u.expect, [PathBuf::from(r"C:\b\a (2).txt"), PathBuf::from(r"C:\b\b.txt")]);

        // A MoveAs is undone by a MoveAs back under the items' old names.
        let u = undo_for(&back, &[ok(r"C:\b\a (2).txt", r"C:\a\a.txt"), ok(r"C:\b\b.txt", r"C:\a\b.txt")]).unwrap();
        assert_eq!(
            u.job,
            Job::MoveAs { items: vec![(r"C:\a\a.txt".into(), "a (2).txt".into()), (r"C:\a\b.txt".into(), "b.txt".into())], dest_dir: r"C:\b".into() }
        );
        let mixed = Job::MoveAs { items: vec![(r"C:\b\x".into(), "x".into()), (r"C:\c\y".into(), "y".into())], dest_dir: r"C:\a".into() };
        assert_eq!(undo_for(&mixed, &[ok(r"C:\b\x", r"C:\a\x"), ok(r"C:\c\y", r"C:\a\y")]), None, "from two folders");

        // A recycle is undone by a restore from the `$R` paths, failed items left out.
        let r = r"C:\$Recycle.Bin\S-1-5-21-1\$RAB12CD.txt";
        let del = Job::Delete { sources: vec![r"C:\a\x.txt".into(), r"C:\a\y.txt".into()], permanent: false };
        let res = [ok(r"C:\a\x.txt", r), ItemResult { hr: -1, ..ok(r"C:\a\y.txt", r"C:\$Recycle.Bin\S-1-5-21-1\$RZZ.txt") }];
        let u = undo_for(&del, &res).unwrap();
        assert_eq!(u.job, Job::Restore { items: vec![(r.into(), r"C:\a\x.txt".into())] });
        assert_eq!(u.expect, [PathBuf::from(r)], "the `$R` path must still be in the bin");
        assert_eq!(undo_for(&Job::Delete { sources: vec![], permanent: true }, &res), None);

        // A restore is undone (redone) by recycling the originals again.
        let restore = Job::Restore { items: vec![(r.into(), r"C:\a\x.txt".into())] };
        let u = undo_for(&restore, &[ok(r, r"C:\a\x.txt")]).unwrap();
        assert_eq!(u.job, Job::Delete { sources: vec![r"C:\a\x.txt".into()], permanent: false });
    }

    #[test]
    fn records_and_the_cap() {
        let del = Job::Delete { sources: vec![r"C:\a\x.txt".into()], permanent: false };
        let rec = record_for(&del, &Outcome::Done, &[], "Deleted 1 item to the Recycle Bin").unwrap();
        assert_eq!(rec.undo, None, "a recycle the bin reported no items for is recorded as not undoable");
        let res = [ok(r"C:\a\x.txt", r"C:\$Recycle.Bin\S-1-5-21-1\$RAB12CD.txt")];
        let rec = record_for(&del, &Outcome::Done, &res, "Deleted 1 item to the Recycle Bin").unwrap();
        assert!(matches!(rec.undo, Some(Undo { job: Job::Restore { .. }, .. })), "{rec:?}");
        let restore = Job::Restore { items: vec![(r"C:\$Recycle.Bin\S-1-5-21-1\$RAB12CD.txt".into(), r"C:\a\x.txt".into())] };
        let res = [ok(r"C:\$Recycle.Bin\S-1-5-21-1\$RAB12CD.txt", r"C:\a\x.txt")];
        let rec = record_for(&restore, &Outcome::Done, &res, "Restored 1 item from the Recycle Bin").unwrap();
        assert_eq!(rec.undo.unwrap().job, Job::Delete { sources: vec![r"C:\a\x.txt".into()], permanent: false }, "the redo of a restore");
        let nuke = Job::Delete { sources: vec![r"C:\a\x.txt".into()], permanent: true };
        assert_eq!(record_for(&nuke, &Outcome::Done, &[], "x"), None, "a permanent delete records nothing");
        let ren = Job::Rename { source: r"C:\a\old.txt".into(), new_name: "new.txt".into() };
        let res = [ok(r"C:\a\old.txt", r"C:\a\new.txt")];
        assert_eq!(record_for(&ren, &Outcome::Cancelled, &res, "x"), None, "only successful jobs");
        assert_eq!(record_for(&ren, &Outcome::Failed(-1), &res, "x"), None);
        let r = record_for(&ren, &Outcome::Done, &res, "Renamed to new.txt").unwrap();
        assert_eq!(r.text, "Renamed to new.txt");

        let mut stack = Vec::new();
        for i in 0..CAP + 5 {
            push(&mut stack, Record { undo: None, text: format!("{i}") });
        }
        assert_eq!(stack.len(), CAP);
        assert_eq!(stack[0].text, "5", "the oldest went first");
        assert_eq!(stack.last().unwrap().text, format!("{}", CAP + 4));
    }

    #[test]
    fn check_refuses_missing_and_non_empty() {
        let root = std::env::temp_dir().join(format!("twinfm-m3-test-undo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("empty")).unwrap();
        std::fs::create_dir_all(root.join("full")).unwrap();
        std::fs::write(root.join("full").join("f"), b"x").unwrap();
        let del = |p: PathBuf, empty: bool| Undo { job: Job::Delete { sources: vec![p.clone()], permanent: false }, expect: vec![p], must_be_empty: empty };
        assert_eq!(check(&del(root.join("empty"), true)), Ok(()));
        assert_eq!(check(&del(root.join("full"), true)), Err("Cannot undo: full is not empty".into()));
        assert_eq!(check(&del(root.join("full"), false)), Ok(()));
        assert_eq!(check(&del(root.join("gone"), false)), Err("Cannot undo: gone is no longer there".into()));
        let restore = |r: PathBuf| Undo { job: Job::Restore { items: vec![(r.clone(), root.join("x.txt"))] }, expect: vec![r], must_be_empty: false };
        assert_eq!(check(&restore(root.join("full").join("f"))), Ok(()));
        assert_eq!(check(&restore(root.join("$RGONE.txt"))), Err("Cannot undo: $RGONE.txt is no longer there".into()), "emptied from the bin");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
