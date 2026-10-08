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
        batch.push(Entry { name: de.file_name().to_string_lossy().as_ref().into(), size, modified, is_dir });
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
