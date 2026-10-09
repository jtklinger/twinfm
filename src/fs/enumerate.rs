//! Worker-thread directory enumeration. read_dir -> Vec<Entry> batches -> channel.
//! Never runs on the UI thread. The UI consumes batches via slint::invoke_from_event_loop.

use crate::model::Entry;
use crossbeam_channel::Sender;
use std::path::PathBuf;
use std::time::{Duration, Instant, UNIX_EPOCH};

pub const BATCH_SIZE: usize = 2000;

pub enum Msg {
    Batch { pane: usize, generation: u64, entries: Vec<Entry> },
    Done { pane: usize, generation: u64, total: usize, elapsed: Duration, error: Option<String> },
}

pub fn spawn(path: PathBuf, pane: usize, generation: u64, tx: Sender<Msg>, wake: impl Fn() + Send + 'static) {
    std::thread::Builder::new()
        .name(format!("enum-{pane}-{generation}"))
        .spawn(move || run(path, pane, generation, tx, wake))
        .expect("spawn enumeration thread");
}

fn run(path: PathBuf, pane: usize, generation: u64, tx: Sender<Msg>, wake: impl Fn()) {
    let t0 = Instant::now();
    let rd = match std::fs::read_dir(&path) {
        Ok(rd) => rd,
        Err(e) => {
            let _ = tx.send(Msg::Done { pane, generation, total: 0, elapsed: t0.elapsed(), error: Some(e.to_string()) });
            wake();
            return;
        }
    };
    let mut batch: Vec<Entry> = Vec::with_capacity(BATCH_SIZE);
    let mut total = 0usize;
    for de in rd.flatten() {
        // On Windows DirEntry::metadata() is served from the FindNextFile data: no extra syscall.
        let md = de.metadata().ok();
        let is_dir = md.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let size = md.as_ref().map(|m| m.len()).unwrap_or(0);
        let modified = md
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        batch.push(Entry::new(de.file_name().to_string_lossy().as_ref().into(), size, modified, is_dir));
        if batch.len() >= BATCH_SIZE {
            total += batch.len();
            let full = std::mem::replace(&mut batch, Vec::with_capacity(BATCH_SIZE));
            if tx.send(Msg::Batch { pane, generation, entries: full }).is_err() {
                return;
            }
            wake();
        }
    }
    if !batch.is_empty() {
        total += batch.len();
        let _ = tx.send(Msg::Batch { pane, generation, entries: batch });
    }
    let _ = tx.send(Msg::Done { pane, generation, total, elapsed: t0.elapsed(), error: None });
    wake();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;

    fn collect(path: PathBuf) -> Vec<Msg> {
        let (tx, rx) = unbounded();
        spawn(path, 1, 7, tx, || {});
        let mut out = Vec::new();
        loop {
            let m = rx.recv().expect("worker sends Done before exiting");
            let done = matches!(m, Msg::Done { .. });
            out.push(m);
            if done {
                return out;
            }
        }
    }

    fn describe(m: &Msg) -> String {
        match m {
            Msg::Batch { entries, .. } => format!("Batch of {}", entries.len()),
            Msg::Done { total, error, .. } => format!("Done total={total} error={error:?}"),
        }
    }

    #[test]
    fn enumerates_files_and_a_subdirectory() {
        let dir = std::env::temp_dir().join(format!("rustfm-enum-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        for i in 0..3 {
            std::fs::write(dir.join(format!("f{i}.txt")), vec![b'x'; i * 100]).unwrap();
        }
        let msgs = collect(dir.clone());
        std::fs::remove_dir_all(&dir).unwrap();

        let batches: Vec<&Vec<Entry>> = msgs
            .iter()
            .filter_map(|m| match m {
                Msg::Batch { entries, pane: 1, generation: 7 } => Some(entries),
                _ => None,
            })
            .collect();
        assert_eq!(batches.len(), 1, "4 entries fit in one batch of {BATCH_SIZE}");
        let entries = batches[0];
        assert_eq!(entries.len(), 4);
        let sub = entries.iter().find(|e| e.name == "sub").expect("subdirectory listed");
        assert!(sub.is_dir);
        assert_eq!(sub.size_text, "");
        let f2 = entries.iter().find(|e| e.name == "f2.txt").unwrap();
        assert!(!f2.is_dir);
        assert_eq!(f2.size, 200);
        assert_eq!(f2.size_text, "200 B");
        assert_eq!(f2.modified_text.len(), 16);
        match msgs.last().unwrap() {
            Msg::Done { pane: 1, generation: 7, total: 4, error: None, .. } => {}
            other => panic!("unexpected final message: {}", describe(other)),
        }
    }

    #[test]
    fn bad_path_yields_done_with_error() {
        let msgs = collect(PathBuf::from(r"C:\rustfm-enum-test-does-not-exist\nope"));
        assert_eq!(msgs.len(), 1);
        match &msgs[0] {
            Msg::Done { total: 0, error: Some(e), .. } => assert!(!e.is_empty()),
            other => panic!("unexpected message: {}", describe(other)),
        }
    }
}
