//! Jobs that touch an archive (P7-M3). `classify` decides, from the job alone, whether it writes
//! into or edits an archive and whether that is allowed: TwinFM writes zips itself
//! (`fs::archivewrite`); Windows' `ArchiveFolder` formats are read-only (it refuses every write,
//! P7-M3's check), and nothing goes from one archive into another. The app calls it before a job
//! is queued (the refusal is a note, no job), the worker again before running it, and runs the
//! zip's jobs here instead of through `IFileOperation`. A copy out of any archive (an extraction)
//! is an ordinary `IFileOperation` job and is not classified.
//!
//! A move into a zip adds the items, then recycles the originals (an unoptimised move: Explorer's
//! zip folder leaves them for the source to delete, P7-M3's check); a move out of a zip extracts
//! through the shell, then deletes the items that came out from the zip. Writes into a zip are not
//! undone (Jeremy, 2026-10-19): the record says so.

use super::{ItemResult, Job, Outcome, Progress, ProgressGate, Request};
use crate::fs::archive::{self, Handler};
use crate::fs::archivewrite::{self, Conflict};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

/// What a job does to a zip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    /// Copy or move files from disk into folder `inner` of `zip`.
    Add { zip: PathBuf, inner: String, sources: Vec<PathBuf>, mv: bool },
    /// A new empty folder or file named `name` in folder `inner`.
    New { zip: PathBuf, inner: String, name: String, folder: bool },
    Rename { zip: PathBuf, inner: String, new_name: String },
    Delete { zip: PathBuf, inners: Vec<String> },
    /// Extract `sources` (paths inside `zip`) into `dest_dir`, then delete them from the zip.
    MoveOut { zip: PathBuf, sources: Vec<PathBuf>, dest_dir: PathBuf },
}

impl Plan {
    pub fn zip(&self) -> &Path {
        match self {
            Plan::Add { zip, .. } | Plan::New { zip, .. } | Plan::Rename { zip, .. } | Plan::Delete { zip, .. } | Plan::MoveOut { zip, .. } => zip,
        }
    }
}

/// Why a write into an archive of `archive`'s kind is refused (a read-only format).
pub fn read_only_why(archive: &Path) -> String {
    let ext = archive.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    format!("Windows' archive handler cannot write into .{ext} archives")
}

pub const BETWEEN_ARCHIVES: &str = "copying from one archive into another is not supported; copy the items out first";

/// The archive each item is stored in (None for an item on disk, an archive file included), and
/// its path below the archive's root.
fn splits(paths: &[PathBuf]) -> Vec<Option<(PathBuf, String)>> {
    paths.iter().map(|p| archive::split(p).filter(|(_, inner)| !inner.is_empty())).collect()
}

fn same(a: &Path, b: &Path) -> bool {
    super::same_dir(a, b)
}

/// The zip all `inside` lie in, or why not (another format; several archives).
fn one_zip(inside: &[(PathBuf, String)]) -> Result<PathBuf, String> {
    let zip = inside[0].0.clone();
    if archive::handler_of(&zip) != Handler::Zip {
        return Err(read_only_why(&zip));
    }
    if inside.iter().any(|(z, _)| !same(z, &zip)) {
        return Err("the items are in different archives".into());
    }
    Ok(zip)
}

/// What `job` does to an archive: Ok(None) when it writes into none (an extraction included),
/// Ok(Some(plan)) for a zip's write, Err(why) when it is refused.
pub fn classify(job: &Job) -> Result<Option<Plan>, String> {
    match job {
        Job::Copy { sources, dest_dir } | Job::Move { sources, dest_dir } => {
            let mv = matches!(job, Job::Move { .. });
            let src = splits(sources);
            let inside: Vec<(PathBuf, String)> = src.iter().flatten().cloned().collect();
            match archive::split(dest_dir) {
                Some((zip, inner)) => {
                    if archive::handler_of(&zip) != Handler::Zip {
                        return Err(read_only_why(&zip));
                    }
                    if !inside.is_empty() {
                        return Err(BETWEEN_ARCHIVES.into());
                    }
                    Ok(Some(Plan::Add { zip, inner, sources: sources.clone(), mv }))
                }
                None if inside.is_empty() || !mv => Ok(None),
                None => {
                    if inside.len() != sources.len() {
                        return Err("the items are partly inside an archive".into());
                    }
                    let zip = one_zip(&inside)?;
                    Ok(Some(Plan::MoveOut { zip, sources: sources.clone(), dest_dir: dest_dir.clone() }))
                }
            }
        }
        Job::NewFolder { parent, name } | Job::NewFile { parent, name } => match archive::split(parent) {
            None => Ok(None),
            Some((zip, inner)) => {
                if archive::handler_of(&zip) != Handler::Zip {
                    return Err(read_only_why(&zip));
                }
                Ok(Some(Plan::New { zip, inner, name: name.clone(), folder: matches!(job, Job::NewFolder { .. }) }))
            }
        },
        Job::Rename { source, new_name } => match archive::split(source) {
            None => Ok(None),
            Some((zip, inner)) => {
                if archive::handler_of(&zip) != Handler::Zip {
                    return Err(read_only_why(&zip));
                }
                if inner.is_empty() {
                    return Ok(None); // the archive file itself
                }
                Ok(Some(Plan::Rename { zip, inner, new_name: new_name.clone() }))
            }
        },
        Job::Delete { sources, .. } => {
            let inside: Vec<(PathBuf, String)> = splits(sources).into_iter().flatten().collect();
            if inside.is_empty() {
                return Ok(None);
            }
            if inside.len() != sources.len() {
                return Err("the items are partly inside an archive".into());
            }
            let zip = one_zip(&inside)?;
            Ok(Some(Plan::Delete { zip, inners: inside.into_iter().map(|(_, i)| i).collect() }))
        }
        // Undo jobs and Compress never name a path inside an archive: refuse one that does.
        Job::MoveAs { .. } | Job::Restore { .. } | Job::Compress { .. } => {
            if job.paths().iter().any(|p| archive::within(p)) { Err("not inside an archive".into()) } else { Ok(None) }
        }
    }
}

fn names(n: usize) -> String {
    if n == 1 { "1 item".into() } else { format!("{n} items") }
}

/// The question for files that would replace files in the zip: Yes replaces, No skips them,
/// Cancel stops. A message box owned by the app's window, on this worker (as `IFileOperation`'s
/// own dialogs are).
fn ask_conflicts(hwnd: isize, zip: &Path, clash: &[String]) -> Option<Conflict> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{IDNO, IDYES, MB_ICONWARNING, MB_YESNOCANCEL, MessageBoxW};
    let shown: Vec<&str> = clash.iter().take(8).map(String::as_str).collect();
    let more = if clash.len() > shown.len() { format!("\n... and {} more", clash.len() - shown.len()) } else { String::new() };
    let text = format!(
        "{} already in {}:\n\n{}{more}\n\nReplace them?\n\nYes: replace them. No: skip them. Cancel: stop.",
        names(clash.len()),
        zip.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        shown.join("\n")
    );
    let t: Vec<u16> = text.encode_utf16().chain([0]).collect();
    let c: Vec<u16> = "TwinFM".encode_utf16().chain([0]).collect();
    // SAFETY: NUL-terminated strings that outlive the call; the owner is the app's window or none.
    let r = unsafe { MessageBoxW(hwnd as _, t.as_ptr(), c.as_ptr(), MB_YESNOCANCEL | MB_ICONWARNING) };
    eprintln!("[archive] {} file(s) already in {}: {}", clash.len(), zip.display(), match r {
        IDYES => "replace",
        IDNO => "skip",
        _ => "cancelled",
    });
    match r {
        IDYES => Some(Conflict::Replace),
        IDNO => Some(Conflict::Skip),
        _ => None,
    }
}

/// "Permanently delete N items from x.zip?" (a zip has no Recycle Bin). True to go on.
fn confirm_delete(hwnd: isize, zip: &Path, n: usize) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{IDYES, MB_ICONWARNING, MB_YESNO, MessageBoxW};
    let text = format!(
        "Permanently delete {} from {}?\n\nA zip archive has no Recycle Bin, and this cannot be undone.",
        names(n),
        zip.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    );
    let t: Vec<u16> = text.encode_utf16().chain([0]).collect();
    let c: Vec<u16> = "TwinFM".encode_utf16().chain([0]).collect();
    // SAFETY: as in `ask_conflicts`.
    unsafe { MessageBoxW(hwnd as _, t.as_ptr(), c.as_ptr(), MB_YESNO | MB_ICONWARNING) == IDYES }
}

/// A writer's (bytes done, bytes in all) as the job's percentage, through the gate (in KiB, so
/// archives past 4 GB fit the percentage's `u32`s).
fn reporter(id: u64, progress: &Rc<dyn Fn(Progress)>) -> impl FnMut(u64, u64) + '_ {
    let mut gate = ProgressGate::new();
    move |done, total| {
        if let Some(percent) = super::percent((total / 1024).max(1) as u32, (done / 1024) as u32) {
            if gate.due(Instant::now(), percent) {
                progress(Progress { id, percent });
            }
        }
    }
}

/// Run a zip's job on the worker (`perform` for the rest), reporting progress through the gate.
pub fn run(req: &Request, plan: Plan, progress: &Rc<dyn Fn(Progress)>) -> (Outcome, Vec<ItemResult>) {
    let t0 = Instant::now();
    let zip = plan.zip().to_path_buf();
    let mut report = reporter(req.id, progress);
    let (outcome, results) = match &plan {
        Plan::Add { inner, sources, mv, .. } => {
            let hwnd = req.hwnd;
            let no_confirm = req.no_confirm;
            let mut ask = |clash: &[String]| if no_confirm { Some(Conflict::Replace) } else { ask_conflicts(hwnd, &zip, clash) };
            match archivewrite::add(&zip, inner, sources, &mut ask, &mut report) {
                Err(e) if e == "cancelled" => (Outcome::Cancelled, Vec::new()),
                Err(e) => (Outcome::Error(e), Vec::new()),
                Ok(tops) => {
                    let mut results: Vec<ItemResult> =
                        sources.iter().zip(&tops).map(|(s, t)| ItemResult { source: s.clone(), dest: Some(zip.join(t)), hr: 0 }).collect();
                    if *mv {
                        // The originals to the Recycle Bin, now that the zip holds them.
                        let del = Request { id: req.id, job: Job::Delete { sources: sources.clone(), permanent: false }, hwnd: req.hwnd, no_confirm: req.no_confirm };
                        let (o, r) = super::perform(&del, progress);
                        eprintln!("[archive] move into {}: originals recycled: {o:?} ({} item(s))", zip.display(), r.len());
                        if o != Outcome::Done {
                            for x in &mut results {
                                x.hr = r.iter().find(|d| super::same_path(&d.source, &x.source)).map_or(x.hr, |d| d.hr.min(0));
                            }
                            (o, results)
                        } else {
                            (Outcome::Done, results)
                        }
                    } else {
                        (Outcome::Done, results)
                    }
                }
            }
        }
        Plan::New { inner, name, folder, .. } => match archivewrite::new_item(&zip, inner, name, *folder, &mut report) {
            Ok(got) => {
                let parent = if inner.is_empty() { zip.clone() } else { zip.join(inner) };
                (Outcome::Done, vec![ItemResult { source: parent.clone(), dest: Some(parent.join(got)), hr: 0 }])
            }
            Err(e) => (Outcome::Error(e), Vec::new()),
        },
        Plan::Rename { inner, new_name, .. } => match archivewrite::rename(&zip, inner, new_name, &mut report) {
            Ok(now) => (Outcome::Done, vec![ItemResult { source: zip.join(inner), dest: Some(zip.join(now)), hr: 0 }]),
            Err(e) => (Outcome::Error(e), Vec::new()),
        },
        Plan::Delete { inners, .. } => {
            if !req.no_confirm && !confirm_delete(req.hwnd, &zip, inners.len()) {
                (Outcome::Cancelled, Vec::new())
            } else {
                match archivewrite::delete(&zip, inners, &mut report) {
                    Ok(_) => (Outcome::Done, inners.iter().map(|i| ItemResult { source: zip.join(i), dest: None, hr: 0 }).collect()),
                    Err(e) => (Outcome::Error(e), Vec::new()),
                }
            }
        }
        Plan::MoveOut { sources, dest_dir, .. } => {
            let copy = Request { id: req.id, job: Job::Copy { sources: sources.clone(), dest_dir: dest_dir.clone() }, hwnd: req.hwnd, no_confirm: req.no_confirm };
            let (o, results) = super::perform(&copy, progress);
            // Only what came out leaves the zip.
            let out: Vec<String> = sources
                .iter()
                .filter(|s| results.iter().any(|r| r.ok() && r.dest.is_some() && super::same_path(&r.source, s)))
                .filter_map(|s| archive::split(s).map(|(_, i)| i))
                .collect();
            if out.is_empty() {
                (if o == Outcome::Done { Outcome::Error("nothing came out of the archive".into()) } else { o }, results)
            } else {
                match archivewrite::delete(&zip, &out, &mut |_, _| {}) {
                    Ok(_) => (o, results),
                    Err(e) => (Outcome::Error(format!("extracted, but removing from the archive failed: {e}")), results),
                }
            }
        }
    };
    archive::forget(&zip);
    eprintln!("[archive] job id={} {:?} on {}: {outcome:?} in {} ms", req.id, plan_name(&plan), zip.display(), t0.elapsed().as_millis());
    (outcome, results)
}

fn plan_name(p: &Plan) -> &'static str {
    match p {
        Plan::Add { mv: false, .. } => "copy in",
        Plan::Add { mv: true, .. } => "move in",
        Plan::New { folder: true, .. } => "new folder",
        Plan::New { folder: false, .. } => "new file",
        Plan::Rename { .. } => "rename",
        Plan::Delete { .. } => "delete",
        Plan::MoveOut { .. } => "move out",
    }
}

/// A Compress job (7z or TAR) on the worker: the archive named after the first item, beside it.
pub fn compress(req: &Request, sources: &[PathBuf], dest_dir: &Path, fmt: archivewrite::Format, progress: &Rc<dyn Fn(Progress)>) -> (Outcome, Vec<ItemResult>) {
    let t0 = Instant::now();
    let Some(first) = sources.first() else { return (Outcome::Error("nothing to compress".into()), Vec::new()) };
    let base = first.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let dest = archivewrite::archive_path(dest_dir, &base, first.is_file(), fmt, |p| std::fs::symlink_metadata(p).is_ok());
    let mut report = reporter(req.id, progress);
    let res = archivewrite::compress(&dest, sources, fmt, &mut report);
    eprintln!("[archive] compress {} item(s) to {}: {res:?} in {} ms", sources.len(), dest.display(), t0.elapsed().as_millis());
    match res {
        Ok(_) => (Outcome::Done, vec![ItemResult { source: first.clone(), dest: Some(dest), hr: 0 }]),
        Err(e) => (Outcome::Error(e), Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fileops::{Done, Worker, undo};
    use std::io::Write;
    use std::time::Duration;

    fn zip_names(zip: &Path) -> Vec<String> {
        let mut za = zip::ZipArchive::new(std::fs::File::open(zip).unwrap()).unwrap();
        let mut v: Vec<String> = (0..za.len()).map(|i| za.by_index_raw(i).unwrap().name().unwrap().into_owned()).collect();
        v.sort();
        v
    }

    /// What each job does to an archive, decided from the job: a zip's writes are plans, the other
    /// formats and archive-to-archive copies are refused, extractions are ordinary jobs.
    #[test]
    fn jobs_are_classified() {
        let dir = std::env::temp_dir().join(format!("twinfm-p7m3-classify-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("folder.zip")).unwrap();
        for f in ["a.zip", "b.zip", "c.7z"] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        let (a, b, c) = (dir.join("a.zip"), dir.join("b.zip"), dir.join("c.7z"));
        let disk = dir.join("f.txt");
        let copy = |s: Vec<PathBuf>, d: PathBuf| Job::Copy { sources: s, dest_dir: d };
        let mv = |s: Vec<PathBuf>, d: PathBuf| Job::Move { sources: s, dest_dir: d };
        assert_eq!(classify(&copy(vec![disk.clone()], dir.clone())), Ok(None), "no archive");
        assert_eq!(classify(&copy(vec![disk.clone()], dir.join("folder.zip"))), Ok(None), "a folder named .zip");
        assert_eq!(
            classify(&copy(vec![disk.clone()], a.join("sub"))),
            Ok(Some(Plan::Add { zip: a.clone(), inner: "sub".into(), sources: vec![disk.clone()], mv: false }))
        );
        assert!(matches!(classify(&mv(vec![disk.clone()], a.clone())), Ok(Some(Plan::Add { mv: true, .. }))));
        assert_eq!(classify(&copy(vec![a.join("x.txt")], dir.clone())), Ok(None), "an extraction is an ordinary copy");
        // An archive file on disk is a file, not an item inside an archive (Jeremy's console check:
        // a 7z copied into a zip was refused as "from one archive into another").
        assert!(matches!(classify(&copy(vec![c.clone(), b.clone()], a.clone())), Ok(Some(Plan::Add { .. }))), "archives into a zip");
        assert_eq!(classify(&mv(vec![a.clone()], dir.join("folder.zip"))), Ok(None), "a zip moved on disk");
        assert_eq!(
            classify(&Job::Compress { sources: vec![a.clone(), c.clone()], dest_dir: dir.clone(), format: crate::fs::archivewrite::Format::Tar }),
            Ok(None),
            "archives compressed"
        );
        assert_eq!(classify(&copy(vec![c.join("x.txt")], dir.clone())), Ok(None), "from any format");
        assert!(matches!(classify(&mv(vec![a.join("x.txt")], dir.clone())), Ok(Some(Plan::MoveOut { .. }))));
        if archive::is_archive_name("c.7z") {
            let ro = classify(&copy(vec![disk.clone()], c.clone())).unwrap_err();
            assert_eq!(ro, "Windows' archive handler cannot write into .7z archives");
            assert_eq!(classify(&mv(vec![c.join("x.txt")], dir.clone())).unwrap_err(), ro, "no move out of a read-only archive");
            assert!(classify(&Job::NewFile { parent: c.clone(), name: "n".into() }).is_err());
            assert!(classify(&Job::Delete { sources: vec![c.join("x")], permanent: true }).is_err());
        } else {
            eprintln!("ArchiveFolder does not take .7z here: its refusals not checked");
        }
        assert_eq!(classify(&copy(vec![b.join("x")], a.clone())).unwrap_err(), BETWEEN_ARCHIVES);
        assert_eq!(classify(&copy(vec![a.join("x")], a.join("sub"))).unwrap_err(), BETWEEN_ARCHIVES, "within one zip too");
        assert!(classify(&mv(vec![a.join("x"), b.join("y")], dir.clone())).is_err(), "two archives");
        assert!(classify(&mv(vec![a.join("x"), disk.clone()], dir.clone())).is_err(), "partly inside");
        assert!(matches!(classify(&Job::NewFolder { parent: a.join("sub"), name: "n".into() }), Ok(Some(Plan::New { folder: true, .. }))));
        assert!(matches!(classify(&Job::Rename { source: a.join("x"), new_name: "y".into() }), Ok(Some(Plan::Rename { .. }))));
        assert_eq!(classify(&Job::Rename { source: a.clone(), new_name: "y.zip".into() }), Ok(None), "the archive file itself");
        assert_eq!(
            classify(&Job::Delete { sources: vec![a.join("x"), a.join(r"s\y")], permanent: false }),
            Ok(Some(Plan::Delete { zip: a.clone(), inners: vec!["x".into(), r"s\y".into()] }))
        );
        assert_eq!(classify(&Job::Delete { sources: vec![a.clone()], permanent: false }), Ok(None), "the archive file itself");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A zip's jobs through the real worker (`TWINFM_SOAK`'s no-confirm path, so no question is
    /// asked): copy in, new folder, rename, delete, move out (the shell extracts), a refused copy
    /// into a 7z, and a Compress (its undo, recycling the archive, is checked, not run: this test
    /// leaves the Recycle Bin alone). A zip write's record says it cannot be undone.
    #[test]
    fn zip_jobs_on_the_worker() {
        let dir = std::env::temp_dir().join(format!("twinfm-p7m3-jobs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src").join("d")).unwrap();
        std::fs::create_dir_all(dir.join("out")).unwrap();
        let dir = std::fs::canonicalize(&dir).unwrap();
        let dir = PathBuf::from(dir.to_string_lossy().trim_start_matches(r"\\?\"));
        std::fs::write(dir.join("src").join("f.txt"), b"file").unwrap();
        std::fs::write(dir.join("src").join("d").join("g.txt"), b"gg").unwrap();
        let z = dir.join("t.zip");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&z).unwrap());
        w.start_file("old.txt", zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(b"old").unwrap();
        w.finish().unwrap();
        std::fs::write(dir.join("r.7z"), b"not read").unwrap();

        let (tx, rx) = crossbeam_channel::unbounded();
        let worker = Worker::spawn(move |d| {
            let _ = tx.send(d);
        }, |_| {});
        let mut id = 0;
        let mut run = |job: Job| -> Done {
            id += 1;
            assert!(worker.submit(Request { id, job, hwnd: 0, no_confirm: true }));
            rx.recv_timeout(Duration::from_secs(30)).expect("the job finishes within 30 s")
        };
        let d = run(Job::Copy { sources: vec![dir.join("src").join("f.txt"), dir.join("src").join("d")], dest_dir: z.clone() });
        assert_eq!(d.outcome, Outcome::Done);
        assert_eq!(d.results.iter().map(|r| r.dest.clone().unwrap()).collect::<Vec<_>>(), vec![z.join("f.txt"), z.join("d")]);
        let rec = undo::record_for(&d.job, &d.outcome, &d.results, "Copied 2 items").unwrap();
        assert_eq!((rec.undo, rec.cannot), (None, undo::CANNOT_UNDO_ARCHIVE), "a write into a zip is not undone");
        assert!(dir.join("src").join("f.txt").exists(), "a copy leaves the original");
        let d = run(Job::NewFolder { parent: z.join("d"), name: "New folder".into() });
        assert_eq!((d.outcome.clone(), crate::fileops::real_name(&d.job, &d.results)), (Outcome::Done, Some("New folder".into())));
        let d = run(Job::Rename { source: z.join("f.txt"), new_name: "f2.txt".into() });
        assert_eq!(crate::fileops::status_text(&d.job, &d.outcome, &d.results), "Renamed to f2.txt");
        let d = run(Job::Delete { sources: vec![z.join("old.txt")], permanent: false });
        assert_eq!(crate::fileops::status_text(&d.job, &d.outcome, &d.results), "Deleted 1 item from the archive");
        assert_eq!(zip_names(&z), vec!["d/", "d/New folder/", "d/g.txt", "f2.txt"]);
        // Move out: the shell extracts, then the items leave the zip.
        let d = run(Job::Move { sources: vec![z.join("d")], dest_dir: dir.join("out") });
        assert_eq!(d.outcome, Outcome::Done, "{:?}", d.results);
        assert_eq!(std::fs::read(dir.join("out").join("d").join("g.txt")).unwrap(), b"gg");
        assert!(dir.join("out").join("d").join("New folder").is_dir());
        assert_eq!(zip_names(&z), vec!["f2.txt"]);
        // Refused on the worker too (a job queued by another path).
        if archive::is_archive_name("r.7z") {
            let d = run(Job::Copy { sources: vec![dir.join("src").join("f.txt")], dest_dir: dir.join("r.7z") });
            assert_eq!(d.outcome, Outcome::Error("Windows' archive handler cannot write into .7z archives".into()));
        }
        // Compress: the archive beside the items, named after the first; undone by recycling it.
        let d = run(Job::Compress { sources: vec![dir.join("src").join("f.txt")], dest_dir: dir.join("src"), format: crate::fs::archivewrite::Format::Tar });
        assert_eq!(d.outcome, Outcome::Done);
        assert_eq!(crate::fileops::status_text(&d.job, &d.outcome, &d.results), "Compressed 1 item to f.tar");
        assert!(dir.join("src").join("f.tar").is_file());
        let u = undo::undo_for(&d.job, &d.results).unwrap();
        assert_eq!(u.job, Job::Delete { sources: vec![dir.join("src").join("f.tar")], permanent: false });
        drop(worker);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
