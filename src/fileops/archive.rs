//! Jobs that touch an archive (P7-M3). `classify` decides, from the job alone, whether it writes
//! into or edits an archive and whether that is allowed: TwinFM writes zips itself
//! (`fs::archivewrite`); Windows' `ArchiveFolder` formats are read-only (it refuses every write,
//! P7-M3's check), and nothing goes from one archive into another. The app calls it before a job
//! is queued (the refusal is a note, no job), the worker again before running it, and runs the
//! zip's jobs here instead of through `IFileOperation`. A copy out of any archive (an extraction)
//! is an ordinary `IFileOperation` job and is not classified.
//!
//! Since P8-L2 items go from one zip into another, or within one, as bytes (`archivewrite::
//! copy_from_zip`), and a password-protected zip is written: the job asks for the password (one of
//! TwinFM's own questions since P12-M3, `crate::ask`, or the script's `zippassword`) when it adds files, and
//! checks it against an existing entry first. Copies from the other formats into a zip are still
//! refused.
//!
//! A move into a zip adds the items, then recycles the originals (an unoptimised move: Explorer's
//! zip folder leaves them for the source to delete, P7-M3's check); a move out of a zip extracts
//! through the shell, then deletes the items that came out from the zip. Writes into a zip are not
//! undone (Jeremy, 2026-10-19): the record says so.

use super::{ItemResult, Job, Outcome, Progress, ProgressGate, Request};
use crate::fs::archive::{self, Handler};
use crate::fs::archivewrite::{self, Conflict, Key, Protection};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Mutex;
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
    /// Copy or move the items at `inners` of the zip `from` (`zip` itself, or another) into folder
    /// `inner` of `zip` (P8-L2).
    FromZip { zip: PathBuf, inner: String, from: PathBuf, inners: Vec<String>, mv: bool },
    /// Copy `sources` (items inside an archive of another format) into folder `inner` of `zip`
    /// (P8-L3): extracted to a temporary folder by the shell, then added.
    FromShell { zip: PathBuf, inner: String, sources: Vec<PathBuf> },
}

impl Plan {
    pub fn zip(&self) -> &Path {
        match self {
            Plan::Add { zip, .. } | Plan::New { zip, .. } | Plan::Rename { zip, .. } | Plan::Delete { zip, .. } | Plan::MoveOut { zip, .. } | Plan::FromZip { zip, .. } | Plan::FromShell { zip, .. } => zip,
        }
    }
}

/// Why a write into an archive of `archive`'s kind is refused (a read-only format).
pub fn read_only_why(archive: &Path) -> String {
    let ext = archive.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    format!("Windows' archive handler cannot write into .{ext} archives")
}

/// Why a move from an archive of another format into a zip is refused (a copy works since
/// P8-L3; the source is read-only, so nothing can leave it).
pub fn move_from_read_only_why(from: &Path) -> String {
    let ext = from.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    format!("nothing can be moved out of a .{ext} archive (it is read-only): copy the items instead")
}

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
    // An archive inside an archive is read-only (P8-L3): nothing is written into it or taken out
    // of it; a copy out of it reads its copy (`archive::real`).
    // Folders (a destination) count from the inner archive's root; items only below it (the inner
    // archive file itself is an item of the archive holding it).
    let folder = |p: &PathBuf| archive::nested(p);
    let item = |p: &PathBuf| archive::nested_item(p);
    let refused = match job {
        Job::Copy { dest_dir, .. } => folder(dest_dir),
        Job::Move { sources, dest_dir } => folder(dest_dir) || sources.iter().any(item),
        Job::NewFolder { parent, .. } | Job::NewFile { parent, .. } => folder(parent),
        Job::Rename { source, .. } => item(source),
        Job::Delete { sources, .. } => sources.iter().any(item),
        Job::MoveAs { .. } | Job::Restore { .. } | Job::Compress { .. } | Job::Extract { .. } => false,
    };
    if refused {
        return Err(archive::NESTED_READ_ONLY.into());
    }
    if let Job::Copy { sources, dest_dir } = job {
        if sources.iter().any(item) {
            let sources = sources.iter().map(|s| if item(s) { archive::real(s).unwrap_or_else(|| s.clone()) } else { s.clone() }).collect();
            return classify_inner(&Job::Copy { sources, dest_dir: dest_dir.clone() });
        }
    }
    classify_inner(job)
}

fn classify_inner(job: &Job) -> Result<Option<Plan>, String> {
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
                        if inside.len() != sources.len() {
                            return Err("the items are partly inside an archive".into());
                        }
                        let from = inside[0].0.clone();
                        if inside.iter().any(|(z, _)| !same(z, &from)) {
                            return Err("the items are in different archives".into());
                        }
                        if archive::handler_of(&from) != Handler::Zip {
                            if mv {
                                return Err(move_from_read_only_why(&from));
                            }
                            return Ok(Some(Plan::FromShell { zip, inner, sources: sources.clone() }));
                        }
                        let inners = inside.into_iter().map(|(_, i)| i).collect();
                        return Ok(Some(Plan::FromZip { zip, inner, from, inners, mv }));
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
        // An archive stored in an archive is copied out first, and nothing is extracted into one
        // (P15-M1; the menu offers neither).
        Job::Extract { archive: a, dest_dir, .. } => {
            if archive::within(a) || archive::inside(dest_dir) { Err("not inside an archive".into()) } else { Ok(None) }
        }
    }
}

fn names(n: usize) -> String {
    if n == 1 { "1 item".into() } else { format!("{n} items") }
}

/// The question for files that would replace files in the zip (or, for an extract, in the folder
/// it unpacks into, P15-M1): Replace, Skip them, or Cancel to stop. One of TwinFM's own questions
/// (`crate::ask`, P12-M3), asked from this worker.
fn ask_conflicts(zip: &Path, clash: &[String]) -> Option<Conflict> {
    let shown: Vec<&str> = clash.iter().take(8).map(String::as_str).collect();
    let more = if clash.len() > shown.len() { format!("\n... and {} more", clash.len() - shown.len()) } else { String::new() };
    let name = zip.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let a = crate::ask::ask(crate::ask::Question {
        kind: "replace",
        title: format!("Replace files in {name}?"),
        text: format!("{} already in {name}:\n\n{}{more}", names(clash.len()), shown.join("\n")),
        password: false,
        buttons: vec!["Cancel", "Skip", "Replace"],
        primary: 2,
        cancel: 0,
    });
    let r = match a.button {
        2 => Some(Conflict::Replace),
        1 => Some(Conflict::Skip),
        _ => None,
    };
    eprintln!("[archive] {} file(s) already in {}: {}", clash.len(), zip.display(), match r {
        Some(Conflict::Replace) => "replace",
        Some(Conflict::Skip) => "skip",
        None => "cancelled",
    });
    r
}

/// "Permanently delete N items from x.zip?" (a zip has no Recycle Bin), one of TwinFM's own
/// questions (P12-M3). True to go on.
fn confirm_delete(zip: &Path, n: usize) -> bool {
    let name = zip.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let a = crate::ask::ask(crate::ask::Question {
        kind: "zip delete",
        title: format!("Permanently delete {}?", names(n)),
        text: format!("From {name}. A zip archive has no Recycle Bin, and this cannot be undone."),
        password: false,
        buttons: vec!["Cancel", "Delete"],
        primary: 1,
        cancel: 0,
    });
    a.button == 1
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
    // Before anything is written or extracted (here, on the worker: the UI thread reads no archive).
    let protection = match archivewrite::protection(&zip) {
        Ok(p) => p,
        Err(e) => return (Outcome::Error(e), Vec::new()),
    };
    let key = match needs_key(&plan) {
        Ok(false) => Key::None,
        Ok(true) if protection == Protection::None => Key::None,
        Ok(true) => match key_for(&zip, &protection) {
            Ok(Some(k)) => k,
            Ok(None) => {
                eprintln!("[archive] job id={} {:?} on {}: Cancelled at the password", req.id, plan_name(&plan), zip.display());
                return (Outcome::Cancelled, Vec::new());
            }
            Err(e) => {
                eprintln!("[archive] job id={} {:?} on {}: refused ({e})", req.id, plan_name(&plan), zip.display());
                return (Outcome::Error(e), Vec::new());
            }
        },
        Err(e) => return (Outcome::Error(e), Vec::new()),
    };
    let mut report = reporter(req.id, progress);
    let (outcome, results) = match &plan {
        Plan::Add { inner, sources, mv, .. } => {
            let no_confirm = req.no_confirm;
            let mut ask = |clash: &[String]| if no_confirm { Some(Conflict::Replace) } else { ask_conflicts(&zip, clash) };
            match archivewrite::add(&zip, inner, sources, &key, &mut ask, &mut report) {
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
        Plan::New { inner, name, folder, .. } => match archivewrite::new_item(&zip, inner, name, *folder, &key, &mut report) {
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
            if !req.no_confirm && !confirm_delete(&zip, inners.len()) {
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
        Plan::FromZip { inner, from, inners, mv, .. } => {
            let no_confirm = req.no_confirm;
            let mut ask = |clash: &[String]| if no_confirm { Some(Conflict::Replace) } else { ask_conflicts(&zip, clash) };
            match archivewrite::copy_from_zip(&zip, inner, from, inners, *mv, &key, &mut ask, &mut report) {
                Err(e) if e == "cancelled" => (Outcome::Cancelled, Vec::new()),
                Err(e) => (Outcome::Error(e), Vec::new()),
                Ok(tops) => {
                    let results: Vec<ItemResult> = inners.iter().zip(&tops).map(|(i, t)| ItemResult { source: from.join(i), dest: Some(zip.join(t)), hr: 0 }).collect();
                    // A move from another zip: the items leave it now that this one holds them.
                    if *mv && !same(from, &zip) {
                        let res = archivewrite::delete(from, inners, &mut |_, _| {});
                        archive::forget(from);
                        eprintln!("[archive] move from {} into {}: removed from the source: {res:?}", from.display(), zip.display());
                        match res {
                            Ok(_) => (Outcome::Done, results),
                            Err(e) => (Outcome::Error(format!("copied, but removing them from {} failed: {e}", from.display())), results),
                        }
                    } else {
                        (Outcome::Done, results)
                    }
                }
            }
        }
        Plan::FromShell { inner, sources, .. } => {
            let no_confirm = req.no_confirm;
            let mut ask = |clash: &[String]| if no_confirm { Some(Conflict::Replace) } else { ask_conflicts(&zip, clash) };
            from_shell(req, &zip, inner, sources, &key, &mut ask, progress, &mut report)
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
        Plan::FromZip { mv: false, .. } => "copy from a zip",
        Plan::FromZip { mv: true, .. } => "move from a zip",
        Plan::FromShell { .. } => "copy from an archive",
    }
}

/// A copy from an archive of another format into a zip (P8-L3): the shell extracts the items
/// into a temporary folder (its own progress window, and its password question for a protected
/// 7z), TwinFM adds what came out, and the folder goes.
#[allow(clippy::too_many_arguments)]
fn from_shell(
    req: &Request,
    zip: &Path,
    inner: &str,
    sources: &[PathBuf],
    key: &Key,
    ask: &mut dyn FnMut(&[String]) -> Option<Conflict>,
    progress: &Rc<dyn Fn(Progress)>,
    report: &mut dyn FnMut(u64, u64),
) -> (Outcome, Vec<ItemResult>) {
    let tmp = std::env::temp_dir().join(format!("twinfm-copy-{}-{}", std::process::id(), req.id));
    let _ = std::fs::remove_dir_all(&tmp);
    if let Err(e) = std::fs::create_dir_all(&tmp) {
        return (Outcome::Error(format!("cannot make a temporary folder: {e}")), Vec::new());
    }
    let copy = Request { id: req.id, job: Job::Copy { sources: sources.to_vec(), dest_dir: tmp.clone() }, hwnd: req.hwnd, no_confirm: req.no_confirm };
    let (o, _) = super::perform(&copy, progress);
    // What came out: the temporary folder's contents (the shell's item reports for these formats
    // come twice, or not at all for an archive inside one: P8-L3's scripted check).
    let mut out: Vec<PathBuf> = std::fs::read_dir(&tmp).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    out.sort();
    eprintln!("[archive] copy from an archive: {} item(s) extracted to {}: {o:?}", out.len(), tmp.display());
    let res = if out.is_empty() {
        (if o == Outcome::Done { Outcome::Error("nothing came out of the archive".into()) } else { o }, Vec::new())
    } else {
        match archivewrite::add(zip, inner, &out, key, ask, report) {
            Err(e) if e == "cancelled" => (Outcome::Cancelled, Vec::new()),
            Err(e) => (Outcome::Error(e), Vec::new()),
            Ok(tops) => {
                let results = out
                    .iter()
                    .zip(&tops)
                    .filter_map(|(d, t)| {
                        let name = d.file_name()?;
                        let source = sources.iter().find(|s| s.file_name().is_some_and(|n| n.eq_ignore_ascii_case(name)))?;
                        Some(ItemResult { source: source.clone(), dest: Some(zip.join(t)), hr: 0 })
                    })
                    .collect();
                (o, results)
            }
        }
    };
    let _ = std::fs::remove_dir_all(&tmp);
    res
}

/// The plan writes new data that a password-protected zip encrypts: files added, a new file, or
/// files without encryption copied in from a zip.
fn needs_key(plan: &Plan) -> Result<bool, String> {
    match plan {
        Plan::Add { .. } => Ok(true),
        Plan::New { folder, .. } => Ok(!folder),
        Plan::FromZip { from, inners, .. } => archivewrite::plain_files_among(from, inners),
        Plan::FromShell { .. } => Ok(true),
        Plan::Rename { .. } | Plan::Delete { .. } | Plan::MoveOut { .. } => Ok(false),
    }
}

/// The password the script gives (`zippassword <text>`, TWINFM_SCRIPT only): taken by the next
/// question in place of the dialog.
static SCRIPT_PASSWORD: Mutex<Option<String>> = Mutex::new(None);

/// The driver's `zippassword <text>`: the answer to the next password question.
pub fn script_password(password: &str) {
    *SCRIPT_PASSWORD.lock().unwrap_or_else(|e| e.into_inner()) = Some(password.to_string());
}

/// The key new files get in the password-protected `zip`: the password asked (the script's, or
/// the question's) and checked against an existing entry, asked again while it is wrong. None: the
/// question was cancelled. A scripted run without a password of the script's is refused, never
/// shown a dialog.
fn key_for(zip: &Path, protection: &Protection) -> Result<Option<Key>, String> {
    if let Protection::Unsupported(what) = protection {
        return Err(format!("this zip uses {what}, which TwinFM does not write beside"));
    }
    Ok(password_for(zip, protection, "to encrypt the files added")?.map(|p| protection.key(&p)))
}

/// The password of the password-protected `zip`, asked `what_for` (the script's, or the
/// question's) and checked against an existing entry, asked again while it is wrong. None: the
/// question was cancelled. A scripted run without a password of the script's is refused, never
/// shown a dialog.
fn password_for(zip: &Path, protection: &Protection, what_for: &str) -> Result<Option<String>, String> {
    let method = if matches!(protection, Protection::Aes(_)) { "AES-256" } else { "ZipCrypto" };
    let scripted = std::env::var_os("TWINFM_SCRIPT").is_some();
    let mut wrong = false;
    loop {
        let given = SCRIPT_PASSWORD.lock().unwrap_or_else(|e| e.into_inner()).take();
        let (password, from_script) = match given {
            Some(p) => {
                eprintln!("[archive] password for {} ({method}): from the script", zip.display());
                (p, true)
            }
            None if scripted => return Err("this zip is password-protected and the script gave no password".into()),
            None => {
                eprintln!("[archive] password for {} ({method}): asked{}", zip.display(), if wrong { " again" } else { "" });
                let name = zip.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let prompt = if wrong {
                    format!("That password does not open {name}. Type it again:")
                } else {
                    format!("{name} is password-protected. Its password, {what_for} ({method}):")
                };
                match ask_password(&format!("Password for {name}"), &prompt) {
                    Some(p) => (p, false),
                    None => {
                        eprintln!("[archive] password for {}: cancelled", zip.display());
                        return Ok(None);
                    }
                }
            }
        };
        if archivewrite::check_password(zip, &password)? {
            eprintln!("[archive] password for {}: right", zip.display());
            return Ok(Some(password));
        }
        eprintln!("[archive] password for {}: wrong", zip.display());
        if scripted || from_script {
            return Err("the password does not open this zip".into());
        }
        wrong = true;
    }
}

/// A password, asked as one of TwinFM's own questions (`crate::ask`, P12-M3): `prompt` above a
/// masked field, OK and Cancel, from the calling worker (the file operation worker for a zip, the
/// preview worker for a PDF). None: cancelled. The password is never written to a line.
pub(crate) fn ask_password(title: &str, prompt: &str) -> Option<String> {
    let a = crate::ask::ask(crate::ask::Question {
        kind: "password",
        title: title.to_string(),
        text: prompt.to_string(),
        password: true,
        buttons: vec!["Cancel", "OK"],
        primary: 1,
        cancel: 0,
    });
    (a.button == 1).then_some(a.password)
}

/// An Extract job on the worker (P15-M1): the whole of the archive `a` into a new folder in
/// `dest_dir` named after it (`named`; numbered when the name is taken) or its root items into
/// `dest_dir` itself. A zip is read by TwinFM's own reader (`fs::unzip`): the job's percentage, the
/// cancel (`cancelled`, asked before each file), TwinFM's password question and, into a folder
/// that has some of the files, its replace question (replace under `no_confirm`). The other
/// formats go through the shell as a copy of the root items (Windows' handler is their only
/// reader), with its progress window and its own questions. Returns the outcome, the results (the
/// folder made, or the root items that landed: the undo recycles them) and the root items'
/// destinations that were there before (`undo::forbid_if_replaced`).
pub fn extract(req: &Request, a: &Path, dest_dir: &Path, named: bool, progress: &Rc<dyn Fn(Progress)>, cancelled: &dyn Fn() -> bool) -> (Outcome, Vec<ItemResult>, Vec<PathBuf>) {
    let t0 = Instant::now();
    let fail = |why: String| {
        eprintln!("[archive] extract {}: {:?} in {} ms", a.display(), Outcome::Error(why.clone()), t0.elapsed().as_millis());
        (Outcome::Error(why), Vec::new(), Vec::new())
    };
    if let Err(why) = classify(&req.job) {
        eprintln!("[archive] refused on the worker: {why}");
        return fail(why);
    }
    let there = |p: &Path| std::fs::symlink_metadata(p).is_ok();
    let name = a.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    // The root, listed here on the worker (the UI thread reads no archive).
    let mut unzip = None;
    let roots = match archive::handler_of(a) {
        Handler::Zip => crate::fs::unzip::Unzip::open(a).map(|u| {
            let roots = u.roots();
            unzip = Some(u);
            roots
        }),
        Handler::Shell => archive::root_names(a),
    };
    let roots = match roots {
        Ok(r) if r.is_empty() => return fail("the archive is empty".into()),
        Ok(r) => r,
        Err(e) => return fail(e),
    };
    // A zip's password, before anything is made.
    let mut password = None;
    if unzip.is_some() {
        match archivewrite::protection(a) {
            Err(e) => return fail(e),
            Ok(Protection::None) => {}
            Ok(Protection::Unsupported(what)) => return fail(format!("this zip uses {what}, which TwinFM does not read")),
            Ok(p) => match password_for(a, &p, "to extract it") {
                Ok(Some(p)) => password = Some(p),
                Ok(None) => {
                    eprintln!("[archive] extract {}: Cancelled at the password", a.display());
                    return (Outcome::Cancelled, Vec::new(), Vec::new());
                }
                Err(e) => return fail(e),
            },
        }
    }
    let dest = if named {
        let free = archive::extract_folder(dest_dir, &name, there);
        let made = free.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if made != archive::folder_name(&name) {
            eprintln!("[archive] extract: folder {} taken: {made}", archive::folder_name(&name));
        }
        if let Err(e) = std::fs::create_dir(&free) {
            return fail(format!("cannot make the folder {}: {e}", free.display()));
        }
        free
    } else {
        dest_dir.to_path_buf()
    };
    let engine = if unzip.is_some() { "TwinFM's reader" } else { "the shell" };
    eprintln!("[archive] extract {} -> {} ({}): {} root item(s), {engine}", a.display(), dest.display(), if named { "named" } else { "here" }, roots.len());
    let existed: Vec<PathBuf> = if named { Vec::new() } else { roots.iter().map(|r| dest.join(r)).filter(|p| there(p)).collect() };
    let (outcome, tail) = match unzip {
        Some(mut u) => {
            let clash = if named { Vec::new() } else { u.clashes(&dest) };
            let conflict = if clash.is_empty() || req.no_confirm { Some(Conflict::Replace) } else { ask_conflicts(&dest, &clash) };
            match conflict {
                None => (Outcome::Cancelled, "0 files, 0 bytes".to_string()),
                Some(c) => {
                    let mut report = reporter(req.id, progress);
                    match u.extract(&dest, password.as_deref(), c == Conflict::Skip, &mut report, cancelled) {
                        Ok((finished, s)) => (if finished { Outcome::Done } else { Outcome::Cancelled }, format!("{} files, {} bytes", s.files, s.bytes)),
                        Err(e) => (Outcome::Error(e), "stopped at that file".to_string()),
                    }
                }
            }
        }
        None => {
            // A copy of the root items out of the archive, as a copy out of one is, but without
            // `own_results`' pass over the sink's reports (it is quadratic in the items, and for
            // these formats the reports come twice or not at all, P8-L3): what landed is read
            // from the folder below.
            let sources = roots.iter().map(|r| a.join(r)).collect();
            let copy = Request { id: req.id, job: Job::Copy { sources, dest_dir: dest.clone() }, hwnd: req.hwnd, no_confirm: req.no_confirm };
            let mut reports = Vec::new();
            let outcome = super::outcome_of(super::perform_inner(&copy, progress, &mut reports));
            (outcome, format!("the sink reported {} item(s)", reports.len()))
        }
    };
    // What landed. An extract that did not finish claims nothing that was there before it.
    let landed: Vec<&String> = roots.iter().filter(|r| there(&dest.join(r)) && (outcome == Outcome::Done || !existed.contains(&dest.join(r)))).collect();
    let results = if !named {
        landed.iter().map(|r| ItemResult { source: a.join(r), dest: Some(dest.join(r)), hr: 0 }).collect()
    } else if landed.is_empty() {
        // Nothing came out: the folder made for it goes again.
        let _ = std::fs::remove_dir(&dest);
        Vec::new()
    } else {
        vec![ItemResult { source: a.to_path_buf(), dest: Some(dest.clone()), hr: 0 }]
    };
    eprintln!("[archive] extract {}: {outcome:?} in {} ms, {tail}, {} of {} root item(s) landed", a.display(), t0.elapsed().as_millis(), landed.len(), roots.len());
    (outcome, results, existed)
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
    use std::io::{Read, Write};
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
        // From one zip into another, and within one (P8-L2); from another format, still refused.
        assert!(matches!(classify(&copy(vec![b.join("x")], a.clone())), Ok(Some(Plan::FromZip { ref from, mv: false, .. })) if *from == b));
        assert!(matches!(classify(&mv(vec![a.join("x")], a.join("sub"))), Ok(Some(Plan::FromZip { ref from, ref inner, mv: true, .. })) if *from == a && inner == "sub"), "within one zip");
        assert_eq!(classify(&copy(vec![a.join("x"), disk.clone()], b.clone())).unwrap_err(), "the items are partly inside an archive");
        if archive::is_archive_name("c.7z") {
            // P8-L3: a copy from another format into a zip is a plan; a move is refused.
            assert!(matches!(classify(&copy(vec![c.join("x")], a.join("sub"))), Ok(Some(Plan::FromShell { ref inner, .. })) if inner == "sub"));
            assert_eq!(classify(&mv(vec![c.join("x")], a.clone())).unwrap_err(), move_from_read_only_why(&c));
        }
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
        // Extract (P15-M1): an archive on disk into a folder on disk, never one stored in an
        // archive, never into one.
        let extract = |archive: PathBuf, dest_dir: PathBuf| Job::Extract { archive, dest_dir, named: true };
        assert_eq!(classify(&extract(a.clone(), dir.clone())), Ok(None));
        assert_eq!(classify(&extract(a.join("inner.zip"), dir.clone())).unwrap_err(), "not inside an archive");
        assert!(classify(&extract(a.clone(), b.clone())).is_err(), "into a zip's root");
        assert!(classify(&extract(a.clone(), b.join("sub"))).is_err());
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
        // A password-protected zip (P8-L2): a wrong password refuses the job with the zip
        // untouched; the right one adds the file under the zip's own encryption, and the tar.exe
        // entries stay readable through a rename (P7-M3's console check found a raw copy that
        // dropped their encryption flag). No move out here: the shell would ask for the password.
        let tar = Path::new(r"C:\Windows\System32\tar.exe");
        if tar.exists() {
            let pw = dir.join("pw.zip");
            let st = std::process::Command::new(tar)
                .arg("-cf")
                .arg(&pw)
                .args(["--format", "zip", "--options", "zip:encryption=zipcrypt", "--passphrase", "x", "-C"])
                .arg(dir.join("out"))
                .arg("d")
                .status()
                .unwrap();
            assert!(st.success());
            let before = std::fs::read(&pw).unwrap();
            script_password("nope");
            let d = run(Job::Copy { sources: vec![dir.join("src").join("f.txt")], dest_dir: pw.clone() });
            assert_eq!(d.outcome, Outcome::Error("the password does not open this zip".into()));
            assert_eq!(std::fs::read(&pw).unwrap(), before, "a wrong password writes nothing");
            script_password("x");
            let d = run(Job::Copy { sources: vec![dir.join("src").join("f.txt")], dest_dir: pw.clone() });
            assert_eq!(d.outcome, Outcome::Done);
            let d = run(Job::Rename { source: pw.join("d"), new_name: "e".into() });
            assert_eq!(d.outcome, Outcome::Done, "a rename needs no password");
            let mut za = zip::ZipArchive::new(std::fs::File::open(&pw).unwrap()).unwrap();
            for (name, data) in [("f.txt", &b"file"[..]), ("e/g.txt", &b"gg"[..])] {
                let mut f = za.by_name_decrypt(name, b"x").unwrap();
                assert!(f.encrypted(), "{name} is encrypted");
                let mut v = Vec::new();
                f.read_to_end(&mut v).unwrap();
                assert_eq!(v, data, "{name}");
            }
            // An extract of it (P15-M1) asks for the password too: a wrong one makes no folder,
            // the right one unpacks it.
            script_password("nope");
            let d = run(Job::Extract { archive: pw.clone(), dest_dir: dir.clone(), named: true });
            assert_eq!(d.outcome, Outcome::Error("the password does not open this zip".into()));
            assert!(!dir.join("pw").exists(), "nothing is made before the password is right");
            script_password("x");
            let d = run(Job::Extract { archive: pw.clone(), dest_dir: dir.clone(), named: true });
            assert_eq!((d.outcome.clone(), d.results[0].dest.clone()), (Outcome::Done, Some(dir.join("pw"))));
            assert_eq!(std::fs::read(dir.join("pw").join("f.txt")).unwrap(), b"file");
            assert_eq!(std::fs::read(dir.join("pw").join("e").join("g.txt")).unwrap(), b"gg");
        }
        // From one zip into another, and within one (P8-L2): entries copied as bytes.
        let z2 = dir.join("t2.zip");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&z2).unwrap());
        w.start_file("keep.txt", zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(b"keep").unwrap();
        w.finish().unwrap();
        let d = run(Job::Copy { sources: vec![z.join("f2.txt")], dest_dir: z2.clone() });
        assert_eq!((d.outcome.clone(), d.results[0].dest.clone()), (Outcome::Done, Some(z2.join("f2.txt"))));
        assert_eq!(zip_names(&z2), vec!["f2.txt", "keep.txt"]);
        assert_eq!(zip_names(&z), vec!["f2.txt"], "a copy leaves the source zip as it was");
        let d = run(Job::Copy { sources: vec![z2.join("keep.txt")], dest_dir: z2.clone() });
        assert_eq!(d.results[0].dest, Some(z2.join("keep - Copy.txt")), "a copy into its own folder");
        let d = run(Job::Move { sources: vec![z2.join("keep - Copy.txt")], dest_dir: z.clone() });
        assert_eq!(d.outcome, Outcome::Done);
        assert_eq!((zip_names(&z), zip_names(&z2)), (vec!["f2.txt".to_string(), "keep - Copy.txt".into()], vec!["f2.txt".to_string(), "keep.txt".into()]));
        let d = run(Job::NewFolder { parent: z2.clone(), name: "sub".into() });
        assert_eq!(d.outcome, Outcome::Done);
        let d = run(Job::Move { sources: vec![z2.join("keep.txt")], dest_dir: z2.join("sub") });
        assert_eq!(d.outcome, Outcome::Done);
        assert_eq!(zip_names(&z2), vec!["f2.txt", "sub/", "sub/keep.txt"], "a move within one zip renames");
        let d = run(Job::Move { sources: vec![z2.join("sub")], dest_dir: z2.join("sub") });
        assert_eq!(d.outcome, Outcome::Error("sub cannot go into itself".into()));
        // From a 7z into a zip (P8-L3): the shell extracts into a temporary folder, TwinFM adds.
        let seven = dir.join("s.7z");
        crate::fs::archivewrite::compress(&seven, &[dir.join("src").join("f.txt")], crate::fs::archivewrite::Format::SevenZ, &mut |_, _| {}).unwrap();
        if archive::is_archive_name("s.7z") {
            let d = run(Job::Copy { sources: vec![seven.join("f.txt")], dest_dir: z2.join("sub") });
            assert_eq!((d.outcome.clone(), d.results.iter().map(|r| r.dest.clone()).collect::<Vec<_>>()), (Outcome::Done, vec![Some(z2.join(r"sub\f.txt"))]));
            assert_eq!(zip_names(&z2), vec!["f2.txt", "sub/", "sub/f.txt", "sub/keep.txt"]);
            let mut za = zip::ZipArchive::new(std::fs::File::open(&z2).unwrap()).unwrap();
            let mut v = Vec::new();
            za.by_name("sub/f.txt").unwrap().read_to_end(&mut v).unwrap();
            assert_eq!(v, b"file");
            let leftovers: Vec<_> = std::fs::read_dir(std::env::temp_dir()).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with(&format!("twinfm-copy-{}-", std::process::id()))).collect();
            assert!(leftovers.is_empty(), "the temporary folder goes: {leftovers:?}");
        }
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

    /// Extract jobs through the real worker (P15-M1): a zip by TwinFM's reader into a folder named
    /// after it (numbered the second time) and into a folder that has one of its items (replaced
    /// without a question under no-confirm, and then not undone), a 7z and a tar.gz through the
    /// shell, and the refusals. The undo of each is checked and its items deleted permanently:
    /// this test leaves the Recycle Bin alone.
    #[test]
    fn extracts_on_the_worker() {
        let dir = std::env::temp_dir().join(format!("twinfm-p15m1-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src").join("tree").join("d")).unwrap();
        let dir = std::fs::canonicalize(&dir).unwrap();
        let dir = PathBuf::from(dir.to_string_lossy().trim_start_matches(r"\\?\"));
        let src = dir.join("src");
        std::fs::write(src.join("top.txt"), b"top").unwrap();
        std::fs::write(src.join("tree").join("f.txt"), b"file").unwrap();
        std::fs::write(src.join("tree").join("d").join("g.txt"), b"gg").unwrap();
        let z = dir.join("a.zip");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&z).unwrap());
        for (name, data) in [("top.txt", &b"top"[..]), ("tree/f.txt", &b"file"[..]), ("tree/d/g.txt", &b"gg"[..])] {
            w.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();

        let (tx, rx) = crossbeam_channel::unbounded();
        let worker = Worker::spawn(move |d| {
            let _ = tx.send(d);
        }, |_| {});
        let mut id = 0;
        let mut run = |job: Job| -> Done {
            id += 1;
            assert!(worker.submit(Request { id, job, hwnd: 0, no_confirm: true }));
            rx.recv_timeout(Duration::from_secs(60)).expect("the job finishes within 60 s")
        };
        let text = |d: &Done| crate::fileops::status_text(&d.job, &d.outcome, &d.results);
        let whole = |at: &Path| {
            assert_eq!(std::fs::read(at.join("top.txt")).unwrap(), b"top", "{}", at.display());
            assert_eq!(std::fs::read(at.join("tree").join("f.txt")).unwrap(), b"file");
            assert_eq!(std::fs::read(at.join("tree").join("d").join("g.txt")).unwrap(), b"gg");
        };
        let named = |archive: &Path| Job::Extract { archive: archive.to_path_buf(), dest_dir: dir.clone(), named: true };

        // Into a folder named after it; a second time, numbered.
        let d = run(named(&z));
        assert_eq!((d.outcome.clone(), text(&d)), (Outcome::Done, "Extracted a.zip to a".to_string()));
        assert_eq!(d.results, [ItemResult { source: z.clone(), dest: Some(dir.join("a")), hr: 0 }]);
        whole(&dir.join("a"));
        assert_eq!(crate::fileops::real_name(&d.job, &d.results), Some("a".into()), "where the cursor goes");
        let rec = undo::forbid_if_replaced(undo::record_for(&d.job, &d.outcome, &d.results, &text(&d)), &d.existed, &d.results).unwrap();
        let u = rec.undo.expect("a new folder is undone");
        assert_eq!((u.job.clone(), u.must_be_empty), (Job::Delete { sources: vec![dir.join("a")], permanent: false }, false), "the folder to the Recycle Bin, full as it is");
        let d = run(named(&z));
        assert_eq!(text(&d), "Extracted a.zip to a (2)");
        whole(&dir.join("a (2)"));
        let d = run(Job::Delete { sources: vec![dir.join("a"), dir.join("a (2)")], permanent: true });
        assert_eq!(d.outcome, Outcome::Done);

        // Here: the root items into a folder. One that is there already is replaced (no question
        // under no-confirm), and the extract is then not undone.
        let here = dir.join("here");
        std::fs::create_dir_all(&here).unwrap();
        std::fs::write(here.join("top.txt"), b"mine").unwrap();
        let d = run(Job::Extract { archive: z.clone(), dest_dir: here.clone(), named: false });
        assert_eq!((d.outcome.clone(), text(&d)), (Outcome::Done, "Extracted 2 items from a.zip".to_string()));
        assert_eq!(d.results.iter().map(|r| (r.source.clone(), r.dest.clone().unwrap())).collect::<Vec<_>>(), [(z.join("top.txt"), here.join("top.txt")), (z.join("tree"), here.join("tree"))]);
        whole(&here);
        assert_eq!(d.existed, [here.join("top.txt")]);
        let rec = undo::forbid_if_replaced(undo::record_for(&d.job, &d.outcome, &d.results, &text(&d)), &d.existed, &d.results).unwrap();
        assert_eq!((rec.undo, rec.cannot), (None, undo::CANNOT_UNDO_REPLACED));
        // Into an empty folder: undone by recycling the items that landed.
        let fresh = dir.join("fresh");
        std::fs::create_dir_all(&fresh).unwrap();
        let d = run(Job::Extract { archive: z.clone(), dest_dir: fresh.clone(), named: false });
        assert!(d.existed.is_empty());
        let u = undo::forbid_if_replaced(undo::record_for(&d.job, &d.outcome, &d.results, &text(&d)), &d.existed, &d.results).unwrap().undo.unwrap();
        assert_eq!(u.job, Job::Delete { sources: vec![fresh.join("top.txt"), fresh.join("tree")], permanent: false });

        // The other formats, through the shell: a 7z TwinFM writes, a tar.gz Windows' tar writes.
        let seven = dir.join("b.7z");
        crate::fs::archivewrite::compress(&seven, &[src.join("top.txt"), src.join("tree")], crate::fs::archivewrite::Format::SevenZ, &mut |_, _| {}).unwrap();
        if archive::is_archive_name("b.7z") {
            let d = run(named(&seven));
            assert_eq!((d.outcome.clone(), text(&d)), (Outcome::Done, "Extracted b.7z to b".to_string()), "{:?}", d.results);
            whole(&dir.join("b"));
            assert_eq!(undo::undo_for(&d.job, &d.results).unwrap().job, Job::Delete { sources: vec![dir.join("b")], permanent: false });
            let into = dir.join("seven");
            std::fs::create_dir_all(&into).unwrap();
            let d = run(Job::Extract { archive: seven.clone(), dest_dir: into.clone(), named: false });
            assert_eq!((d.outcome.clone(), text(&d)), (Outcome::Done, "Extracted 2 items from b.7z".to_string()));
            whole(&into);
            assert!(d.existed.is_empty());
        } else {
            eprintln!("ArchiveFolder does not take .7z here: its extract not checked");
        }
        let tar = Path::new(r"C:\Windows\System32\tar.exe");
        if tar.exists() && archive::is_archive_name("c.tar.gz") {
            let tgz = dir.join("c.tar.gz");
            let st = std::process::Command::new(tar).arg("-czf").arg(&tgz).arg("-C").arg(&src).args(["top.txt", "tree"]).status().unwrap();
            assert!(st.success());
            let d = run(named(&tgz));
            assert_eq!((d.outcome.clone(), text(&d)), (Outcome::Done, "Extracted c.tar.gz to c".to_string()), "{:?}", d.results);
            whole(&dir.join("c"));
        } else {
            eprintln!("no tar.exe, or ArchiveFolder does not take .gz here: a tar.gz's extract not checked");
        }

        // Refused: an archive stored in an archive, a destination inside one; a damaged zip makes
        // no folder; a job cancelled while it waited never runs.
        let d = run(named(&z.join("inner.zip")));
        assert_eq!(d.outcome, Outcome::Error("not inside an archive".into()));
        let d = run(Job::Extract { archive: z.clone(), dest_dir: z.join("tree"), named: true });
        assert_eq!(d.outcome, Outcome::Error("not inside an archive".into()));
        let bad = dir.join("bad.zip");
        std::fs::write(&bad, b"not a zip").unwrap();
        let d = run(named(&bad));
        assert!(matches!(&d.outcome, Outcome::Error(e) if e.starts_with("not a readable zip archive")), "{:?}", d.outcome);
        assert!(!dir.join("bad").exists());
        worker.cancel_queued(u64::MAX);
        let d = run(named(&z));
        assert_eq!(d.outcome, Outcome::Cancelled);
        assert!(!dir.join("a").exists(), "cancelled before it started");
        drop(worker);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The P15-M1 probe (not part of the gates: `cargo test --release -- --ignored probe_extract
    /// --nocapture`): a whole zip (`TWINFM_PROBE_ZIP`, else `C:\traces\bigdir.zip`) unpacked by
    /// TwinFM's reader through the worker, then by the shell as a copy of its root items on this
    /// thread, with the time `own_results` takes over what the sink reported.
    #[test]
    #[ignore]
    fn probe_extract_engines() {
        use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        let zip = std::env::var_os("TWINFM_PROBE_ZIP").map_or_else(|| PathBuf::from(r"C:\traces\bigdir.zip"), PathBuf::from);
        let dir = std::env::temp_dir().join(format!("twinfm-p15m1-probe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("own")).unwrap();
        std::fs::create_dir_all(dir.join("shell")).unwrap();
        let (tx, rx) = crossbeam_channel::unbounded();
        let worker = Worker::spawn(move |d| {
            let _ = tx.send(d);
        }, |_| {});
        let t0 = Instant::now();
        assert!(worker.submit(Request { id: 1, job: Job::Extract { archive: zip.clone(), dest_dir: dir.join("own"), named: true }, hwnd: 0, no_confirm: true }));
        let d = rx.recv_timeout(Duration::from_secs(3600)).unwrap();
        eprintln!("[probe] TwinFM's reader: {:?} in {} ms", d.outcome, t0.elapsed().as_millis());
        drop(worker);
        if std::env::var_os("TWINFM_PROBE_NO_SHELL").is_some() {
            return;
        }
        let roots = crate::fs::unzip::Unzip::open(&zip).unwrap().roots();
        let job = Job::Copy { sources: roots.iter().map(|r| zip.join(r)).collect(), dest_dir: dir.join("shell") };
        let req = Request { id: 2, job, hwnd: 0, no_confirm: true };
        // SAFETY: this test thread becomes an STA for the shell's copy engine, as the worker is.
        unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
        let progress: Rc<dyn Fn(Progress)> = Rc::new(|_| {});
        let mut reports = Vec::new();
        let t0 = Instant::now();
        let res = crate::fileops::perform_inner(&req, &progress, &mut reports);
        eprintln!("[probe] the shell: {res:?} in {} ms, {} root item(s), the sink reported {} item(s)", t0.elapsed().as_millis(), roots.len(), reports.len());
        let t0 = Instant::now();
        let own = crate::fileops::own_results(&req.job, reports);
        eprintln!("[probe] own_results: {} of {} kept in {} ms", own.len(), roots.len(), t0.elapsed().as_millis());
    }
}
