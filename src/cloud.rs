//! Cloud locations (P6-M1): which folders carry cloud marks over their icons. Decided once per
//! listing on the UI thread, never per file: a folder under a registered sync root (the cloud
//! files API's providers, OneDrive among them: `HKLM\...\Explorer\SyncRootManager\*\UserSyncRoots`,
//! read once per process and again when the volumes change) is `Cloud::SyncRoot`; a folder on a
//! volume named "Google Drive" (Google Drive for desktop's virtual drive, FAT32, no sync root) is
//! `Cloud::Overlays`; everything else is `Cloud::None` and keeps today's keys and cost.

use crate::icons::Cloud;
use std::collections::HashMap;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW;
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, REG_SZ, RegCloseKey, RegEnumKeyExW, RegEnumValueW, RegOpenKeyExW,
};

/// The volume name Google Drive for desktop gives its drive. A user who renames the drive loses the
/// marks (kickoff, open question): the mount's owner cannot be read from the volume.
pub const GOOGLE_DRIVE_VOLUME: &str = "Google Drive";

const SYNC_ROOT_MANAGER: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\SyncRootManager";

/// The sync roots (lowercase, backslashes, no trailing separator) and each volume's verdict, by
/// the volume key `dnd::volume_key` gives.
#[derive(Default)]
pub struct Locations {
    roots: Vec<String>,
    volumes: HashMap<String, bool>,
}

impl Locations {
    /// Read the sync roots from the registry (logged).
    pub fn load() -> Locations {
        let mut l = Locations::default();
        l.reload();
        l
    }

    /// Read the sync roots again and forget the volumes' names (a drive came or went).
    pub fn reload(&mut self) {
        self.roots = sync_roots().iter().map(|r| normalise(r)).filter(|r| !r.is_empty()).collect();
        self.volumes.clear();
        eprintln!("[cloud] sync roots: {} ({})", self.roots.len(), self.roots.join("; "));
    }

    /// The folder's cloud location. A volume's name is asked once per process (and after a
    /// device change), never for a UNC path or a mapped network drive.
    pub fn of(&mut self, folder: &Path) -> Cloud {
        let s = normalise(&folder.to_string_lossy());
        if self.roots.iter().any(|r| under(&s, r)) {
            return Cloud::SyncRoot;
        }
        if s.starts_with(r"\\") || crate::fs::watch::is_remote(folder) {
            return Cloud::None;
        }
        let key = crate::dnd::volume_key(folder);
        let google = *self.volumes.entry(key).or_insert_with_key(|key| {
            let name = volume_name(key);
            let google = name.as_deref() == Some(GOOGLE_DRIVE_VOLUME);
            eprintln!("[cloud] volume {key} {} ({})", name.as_deref().map_or("has no name".into(), |n| format!("named \"{n}\"")), if google { "google drive" } else { "not cloud" });
            google
        });
        if google { Cloud::Overlays } else { Cloud::None }
    }

    /// The sync roots (`normalise`d), for a content search's walk to stay out of (P11-M1).
    pub fn roots(&self) -> Vec<String> {
        self.roots.clone()
    }
}

/// Lowercase, backslashes, no `\\?\` prefix, no trailing separator.
pub fn normalise(p: &str) -> String {
    let s = p.replace('/', "\\").to_lowercase();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    s.trim_end_matches('\\').to_string()
}

/// Whether `path` is `root` or below it (both `normalise`d): component-wise, so `c:\onedrive2`
/// is not under `c:\onedrive`.
pub fn under(path: &str, root: &str) -> bool {
    !root.is_empty() && path.strip_prefix(root).is_some_and(|rest| rest.is_empty() || rest.starts_with('\\'))
}

fn wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s).encode_wide().chain([0]).collect()
}

/// Every `UserSyncRoots` value's path under the sync root manager, for every provider and user.
fn sync_roots() -> Vec<String> {
    let mut out = Vec::new();
    let Some(manager) = open(HKEY_LOCAL_MACHINE, SYNC_ROOT_MANAGER) else { return out };
    let mut i = 0;
    loop {
        let mut name = [0u16; 512];
        let mut len = name.len() as u32;
        // SAFETY: `manager` is open; `name` is writable for `len` characters.
        let r = unsafe {
            RegEnumKeyExW(manager, i, name.as_mut_ptr(), &mut len, std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut())
        };
        if r != ERROR_SUCCESS {
            break;
        }
        i += 1;
        let sub = format!("{}\\UserSyncRoots", String::from_utf16_lossy(&name[..len as usize]));
        let Some(users) = open(manager, &sub) else { continue };
        let mut j = 0;
        loop {
            let (mut vname, mut data) = ([0u16; 256], [0u16; 1024]);
            let (mut vlen, mut dlen, mut kind) = (vname.len() as u32, (data.len() * 2) as u32, 0u32);
            // SAFETY: `users` is open; `vname` and `data` are writable for the lengths passed
            // (characters for the name, bytes for the data).
            let r = unsafe {
                RegEnumValueW(users, j, vname.as_mut_ptr(), &mut vlen, std::ptr::null(), &mut kind, data.as_mut_ptr() as *mut u8, &mut dlen)
            };
            if r != ERROR_SUCCESS {
                break;
            }
            j += 1;
            if kind == REG_SZ {
                let n = (dlen as usize / 2).min(data.len());
                let n = data[..n].iter().position(|&c| c == 0).unwrap_or(n);
                out.push(String::from_utf16_lossy(&data[..n]));
            }
        }
        // SAFETY: opened above, closed once.
        unsafe { RegCloseKey(users) };
    }
    // SAFETY: opened above, closed once.
    unsafe { RegCloseKey(manager) };
    out
}

fn open(parent: HKEY, sub: &str) -> Option<HKEY> {
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: `sub` is NUL-terminated and outlives the call; `key` receives an open key on success.
    let r = unsafe { RegOpenKeyExW(parent, wide(sub).as_ptr(), 0, KEY_READ, &mut key) };
    (r == ERROR_SUCCESS).then_some(key)
}

/// The volume's name (its label) for a `dnd::volume_key` (`g:`, `c:\mnt\data`), or None.
fn volume_name(key: &str) -> Option<String> {
    let root = wide(&format!("{key}\\"));
    let mut name = [0u16; 261];
    // SAFETY: `root` is NUL-terminated; `name` is writable for its length; the other outputs are
    // not asked for.
    let ok = unsafe {
        GetVolumeInformationW(root.as_ptr(), name.as_mut_ptr(), name.len() as u32, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), 0)
    };
    let n = name.iter().position(|&c| c == 0).unwrap_or(0);
    (ok != 0 && n > 0).then(|| String::from_utf16_lossy(&name[..n]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_location_rule() {
        let root = normalise(r"C:\Users\jtkli\OneDrive\");
        assert_eq!(root, r"c:\users\jtkli\onedrive");
        assert_eq!(normalise(r"\\?\C:/Users/x/"), r"c:\users\x");
        for (p, want) in [
            (r"C:\Users\jtkli\OneDrive", true),
            (r"c:\users\jtkli\onedrive\Documents", true),
            (r"C:\Users\jtkli\OneDrive\a\b\c.txt", true),
            (r"\\?\C:\Users\jtkli\OneDrive\x", true),
            (r"C:\Users\jtkli\OneDrive2", false),
            (r"C:\Users\jtkli\OneDriv", false),
            (r"C:\Users\jtkli", false),
            (r"D:\Users\jtkli\OneDrive", false),
        ] {
            assert_eq!(under(&normalise(p), &root), want, "{p}");
        }
        assert!(!under(r"c:\x", ""), "an empty root covers nothing");
        let mut l = Locations { roots: vec![root], volumes: HashMap::new() };
        assert_eq!(l.of(Path::new(r"C:\Users\jtkli\OneDrive\Pictures")), Cloud::SyncRoot);
        // A UNC path is never asked for its volume's name.
        assert_eq!(l.of(Path::new(r"\\server\share\x")), Cloud::None);
        assert!(l.volumes.is_empty());
    }

    #[test]
    fn reads_this_machines_sync_roots_and_volume_names() {
        // Reads only: the registry and the system drive's label. Whatever is registered here
        // (none on a machine without OneDrive) must come back as absolute paths.
        for r in sync_roots() {
            assert!(r.len() > 2 && (r.as_bytes()[1] == b':' || r.starts_with(r"\\")), "{r}");
        }
        let sys = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()).to_lowercase();
        assert_ne!(volume_name(&sys).as_deref(), Some(GOOGLE_DRIVE_VOLUME));
        let mut l = Locations::default();
        assert_eq!(l.of(&Path::new(&sys).join("Windows")), Cloud::None);
        assert_eq!(l.volumes.len(), 1, "one name lookup per volume");
        assert_eq!(l.of(&Path::new(&sys).join("Users")), Cloud::None);
        assert_eq!(l.volumes.len(), 1);
    }
}
