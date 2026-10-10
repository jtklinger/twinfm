//! TwinFM's own undo record (Ctrl+Z). There is no public API to invoke Explorer's undo stack
//! (`FOFX_ADDUNDORECORD` feeds it, but only Explorer can play it back), so every successful job
//! leaves a `Record` built from the real names the progress sink reported, and Ctrl+Z submits
//! its inverse through the ordinary worker path:
//!
//! - Move: move the moved items back to the folder they came from.
//! - Rename: rename the item back to its old name.
//! - New folder: recycle the folder, only if it is still empty.
//! - Copy: recycle the copies.
//! - Delete to the Recycle Bin: a record that undoes nothing and says so (restoring from the
//!   Recycle Bin is not done yet). A permanent delete records nothing.
//!
//! Undo jobs are not recorded themselves (no redo). Pure apart from `check`, which looks at the
//! disk at undo time.

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
    /// None: the job cannot be undone in TwinFM (a delete to the Recycle Bin); Ctrl+Z says so.
    pub undo: Option<Undo>,
    /// The job's status text, for "Undid: <text>".
    pub text: String,
}

/// The note Ctrl+Z shows for a delete to the Recycle Bin.
pub const CANNOT_UNDO_DELETE: &str = "Cannot undo a delete yet (use the Recycle Bin)";

/// The reverse of `job` from the items the sink reported, or None when nothing can be undone.
pub fn undo_for(job: &Job, results: &[ItemResult]) -> Option<Undo> {
    let made: Vec<(PathBuf, PathBuf)> =
        results.iter().filter(|r| r.ok()).filter_map(|r| Some((r.source.clone(), r.dest.clone()?))).collect();
    if made.is_empty() {
        return None;
    }
    let dests: Vec<PathBuf> = made.iter().map(|(_, d)| d.clone()).collect();
    match job {
        Job::Move { sources, .. } => {
            // One folder to move them back to (a paste can gather files from several).
            let from = sources.first()?.parent()?;
            if !sources.iter().all(|s| s.parent().is_some_and(|p| same_dir(p, from))) {
                return None;
            }
            Some(Undo { job: Job::Move { sources: dests.clone(), dest_dir: from.to_path_buf() }, expect: dests, must_be_empty: false })
        }
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
        Job::Delete { .. } => None,
    }
}

/// What a finished job leaves on the undo stack: only successful jobs leave anything; a delete
/// to the Recycle Bin leaves a record that undoes nothing, a permanent delete none.
pub fn record_for(job: &Job, outcome: &Outcome, results: &[ItemResult], text: &str) -> Option<Record> {
    if *outcome != Outcome::Done {
        return None;
    }
    match job {
        Job::Delete { permanent: true, .. } => None,
        Job::Delete { permanent: false, .. } => Some(Record { undo: None, text: text.into() }),
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

/// At undo time: the items are still where the job left them (and a new folder is still empty),
/// or why not.
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
    fn records_and_the_cap() {
        let del = Job::Delete { sources: vec![r"C:\a\x.txt".into()], permanent: false };
        let rec = record_for(&del, &Outcome::Done, &[], "Deleted 1 item to the Recycle Bin").unwrap();
        assert_eq!(rec.undo, None, "a recycle is recorded as not undoable");
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
        std::fs::remove_dir_all(&root).unwrap();
    }
}
