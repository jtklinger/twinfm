//! Worker-thread directory enumeration. read_dir -> Vec<Entry> batches -> channel.
//! Never runs on the UI thread. The UI consumes batches via slint::invoke_from_event_loop.

use crate::model::Entry;
use crossbeam_channel::Sender;
use slint::SharedString;
use std::collections::HashMap;
use std::os::windows::fs::MetadataExt;
use std::path::PathBuf;
use std::time::{Duration, Instant, UNIX_EPOCH};
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY;
use windows_sys::Win32::UI::Shell::{ASSOCF_NOTRUNCATE, ASSOCSTR_FRIENDLYDOCNAME, AssocQueryStringW};

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
    let mut types = TypeNames::default();
    let mut batch: Vec<Entry> = Vec::with_capacity(BATCH_SIZE);
    let mut total = 0usize;
    for de in rd.flatten() {
        // On Windows DirEntry::metadata() is served from the FindNextFile data: no extra syscall,
        // and a reparse point's own attributes are reported (it is not followed).
        let md = de.metadata().ok();
        let attrs = md.as_ref().map(|m| m.file_attributes()).unwrap_or(0);
        let size = md.as_ref().map(|m| m.len()).unwrap_or(0);
        let modified = md
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let name = de.file_name();
        let name = name.to_string_lossy();
        let type_text = types.for_entry(&name, attrs & FILE_ATTRIBUTE_DIRECTORY != 0);
        batch.push(Entry::new(name.as_ref().into(), size, modified, attrs, type_text));
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

/// Friendly type names ("Text Document") by extension, looked up once per extension per
/// enumeration through the shell's association registry. Lives on the worker only.
#[derive(Default)]
pub struct TypeNames {
    cache: HashMap<String, SharedString>,
    key: String,
}

impl TypeNames {
    pub fn for_entry(&mut self, name: &str, is_dir: bool) -> SharedString {
        if is_dir {
            return SharedString::from("File folder");
        }
        // Text after the last dot, as Explorer does (".gitignore" has extension "gitignore").
        let ext = match name.rfind('.') {
            Some(i) if i + 1 < name.len() => &name[i + 1..],
            _ => return SharedString::from("File"),
        };
        self.key.clear();
        self.key.extend(ext.chars().map(|c| c.to_ascii_lowercase()));
        if let Some(t) = self.cache.get(&self.key) {
            return t.clone();
        }
        let t: SharedString =
            assoc_friendly_name(&self.key).unwrap_or_else(|| format!("{} File", ext.to_uppercase())).into();
        self.cache.insert(self.key.clone(), t.clone());
        t
    }
}

/// AssocQueryStringW(ASSOCSTR_FRIENDLYDOCNAME) for ".ext"; None when nothing is registered.
fn assoc_friendly_name(ext: &str) -> Option<String> {
    let assoc: Vec<u16> = ".".encode_utf16().chain(ext.encode_utf16()).chain([0]).collect();
    let mut buf = [0u16; 256];
    let mut len = buf.len() as u32;
    // SAFETY: `assoc` is NUL-terminated, `buf` holds `len` u16s, `len` is a live u32.
    let hr = unsafe {
        AssocQueryStringW(
            ASSOCF_NOTRUNCATE,
            ASSOCSTR_FRIENDLYDOCNAME,
            assoc.as_ptr(),
            std::ptr::null(),
            buf.as_mut_ptr(),
            &mut len,
        )
    };
    if hr != 0 {
        return None; // includes truncation: a type name over 255 chars is not worth a retry
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let s = String::from_utf16_lossy(&buf[..n]);
    if s.is_empty() { None } else { Some(s) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Kind;
    use crossbeam_channel::unbounded;
    use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM, SetFileAttributesW};

    fn set_attrs(path: &std::path::Path, attrs: u32) {
        let w: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().chain([0]).collect();
        // SAFETY: `w` is a NUL-terminated wide path that outlives the call.
        assert_ne!(unsafe { SetFileAttributesW(w.as_ptr(), attrs) }, 0, "SetFileAttributesW {path:?}");
    }

    fn entries_of(msgs: &[Msg]) -> Vec<Entry> {
        msgs.iter()
            .filter_map(|m| match m {
                Msg::Batch { entries, .. } => Some(entries.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    #[test]
    fn reads_hidden_and_system_attributes() {
        let dir = std::env::temp_dir().join(format!("twinfm-attr-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for n in ["plain.txt", "hidden.txt", "system.txt"] {
            std::fs::write(dir.join(n), b"x").unwrap();
        }
        set_attrs(&dir.join("hidden.txt"), FILE_ATTRIBUTE_HIDDEN);
        set_attrs(&dir.join("system.txt"), FILE_ATTRIBUTE_SYSTEM);
        let entries = entries_of(&collect(dir.clone()));
        for n in ["hidden.txt", "system.txt"] {
            set_attrs(&dir.join(n), 0x80); // FILE_ATTRIBUTE_NORMAL, so cleanup can delete them
        }
        std::fs::remove_dir_all(&dir).unwrap();

        let by = |n: &str| entries.iter().find(|e| e.name == n).unwrap().clone();
        assert!(!by("plain.txt").is_hidden() && !by("plain.txt").is_system());
        assert!(by("hidden.txt").is_hidden() && !by("hidden.txt").is_system());
        assert!(by("system.txt").is_system() && !by("system.txt").is_hidden());
    }

    #[test]
    fn type_names_by_extension() {
        let mut t = TypeNames::default();
        assert_eq!(t.for_entry("sub", true), "File folder");
        assert_eq!(t.for_entry("README", false), "File");
        assert_eq!(t.for_entry("trailing.", false), "File");
        // Nothing registers this extension, so either the shell's or our fallback applies.
        assert_eq!(t.for_entry("a.Zq7xw", false), "ZQ7XW File");
        let txt = t.for_entry("a.txt", false);
        assert!(!txt.is_empty());
        assert_eq!(t.for_entry("B.TXT", false), txt, "cache is keyed by lowercase extension");
    }

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
        let dir = std::env::temp_dir().join(format!("twinfm-enum-test-{}", std::process::id()));
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
        assert!(sub.is_dir());
        assert_eq!(sub.kind, Kind::Directory);
        assert_eq!(sub.type_text, "File folder");
        assert_eq!(sub.size_text, "");
        let f2 = entries.iter().find(|e| e.name == "f2.txt").unwrap();
        assert!(!f2.is_dir());
        assert_eq!(f2.kind, Kind::File);
        assert!(!f2.type_text.is_empty());
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
        let msgs = collect(PathBuf::from(r"C:\twinfm-enum-test-does-not-exist\nope"));
        assert_eq!(msgs.len(), 1);
        match &msgs[0] {
            Msg::Done { total: 0, error: Some(e), .. } => assert!(!e.is_empty()),
            other => panic!("unexpected message: {}", describe(other)),
        }
    }
}
