//! Settings: a TOML file at `%APPDATA%\twinfm\config.toml`, loaded once before the first
//! navigation and written (atomically: a `.tmp` file, then `MoveFileExW` over the old one) 500 ms
//! after the last change, through one single-shot `slint::Timer` in `main.rs`, so a splitter
//! drag writes once when it stops. A missing or malformed file gives the defaults and a line on
//! stderr, never an error. Unknown keys are ignored, so an older TwinFM reads a newer file.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW};

/// Window placement: position in physical px (the virtual screen's), size in logical px, and
/// whether it was maximised. A maximised window's x, y, width and height are its normal
/// (restored) placement, so un-maximising after a restart gives the right size.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowGeom {
    pub x: i32,
    pub y: i32,
    pub width: f32,
    pub height: f32,
    /// Missing in a file from before it was saved: not maximised.
    #[serde(default)]
    pub maximized: bool,
}

/// The window's state from its Win32 placement (`GetWindowPlacement`): `Some((true, normal))`
/// when it is maximised, where `normal` is its restored placement (`rcNormalPosition`, turned
/// from workspace into screen coordinates; the size less the window frame, in logical px);
/// `Some((false, _))` when it is not; None when it is minimised or the call fails.
pub fn placement(hwnd: isize, scale: f32) -> Option<(bool, WindowGeom)> {
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromRect};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClientRect, GetWindowPlacement, GetWindowRect, SW_SHOWMAXIMIZED, SW_SHOWMINIMIZED, WINDOWPLACEMENT,
    };
    let hwnd = hwnd as HWND;
    let mut wp: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
    wp.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
    // SAFETY: a sized WINDOWPLACEMENT; a stale or null handle makes the call fail, nothing more.
    if hwnd.is_null() || unsafe { GetWindowPlacement(hwnd, &mut wp) } == 0 {
        return None;
    }
    if wp.showCmd == SW_SHOWMINIMIZED as u32 {
        return None;
    }
    let maximized = wp.showCmd == SW_SHOWMAXIMIZED as u32;
    let n = wp.rcNormalPosition;
    // rcNormalPosition is in workspace coordinates: the monitor's work area origin is (0, 0).
    let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    // SAFETY: a valid RECT and a sized MONITORINFO.
    let (dx, dy) = if unsafe { GetMonitorInfoW(MonitorFromRect(&n, MONITOR_DEFAULTTONEAREST), &mut info) } != 0 {
        (info.rcWork.left - info.rcMonitor.left, info.rcWork.top - info.rcMonitor.top)
    } else {
        (0, 0)
    };
    // The frame (borders and title bar) is the window rectangle less the client area; it is the
    // same maximised as restored.
    let (mut outer, mut client) = (RECT { left: 0, top: 0, right: 0, bottom: 0 }, RECT { left: 0, top: 0, right: 0, bottom: 0 });
    // SAFETY: valid out pointers for a window handle that GetWindowPlacement just accepted.
    let (fw, fh) = if unsafe { GetWindowRect(hwnd, &mut outer) != 0 && GetClientRect(hwnd, &mut client) != 0 } {
        ((outer.right - outer.left) - client.right, (outer.bottom - outer.top) - client.bottom)
    } else {
        (0, 0)
    };
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let normal = WindowGeom {
        x: n.left + dx,
        y: n.top + dy,
        width: ((n.right - n.left - fw) as f32 / scale).max(1.0),
        height: ((n.bottom - n.top - fh) as f32 / scale).max(1.0),
        maximized,
    };
    Some((maximized, normal))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// What a pane opens at start when its tab list (below) is empty; set in the settings dialog
    /// (before P3-M1 every save wrote the active tabs' folders here). An empty right path means
    /// the left one. A command-line path replaces the left pane's tabs.
    pub start_left: String,
    pub start_right: String,
    /// Each pane's tabs, left to right, as folder paths (`left_tabs = ["C:\\", "D:\\work"]`), and
    /// the index of its active tab (`left_active = 1`), as TwinFM last saved them. Empty or
    /// missing: the pane opens one tab on its start folder, so a file from before tabs were
    /// saved still works. The right pane's tabs are restored only when the split is.
    pub left_tabs: Vec<String>,
    pub left_active: usize,
    pub right_tabs: Vec<String>,
    pub right_active: usize,
    /// New tabs in each pane show hidden and system files (Ctrl+H sets it for its pane).
    pub show_hidden_left: bool,
    pub show_hidden_right: bool,
    /// The second pane is open, and the split line's position (0.15 to 0.85 of the panes' width).
    pub split: bool,
    pub split_ratio: f32,
    /// The sort new tabs start with: "name", "size", "modified" or "type" (a header click sets it).
    pub sort_column: String,
    pub sort_ascending: bool,
    /// The Type column in both panes (the settings dialog sets it). Missing in an older file: on.
    pub show_type: bool,
    pub sidebar_visible: bool,
    pub sidebar_width: f32,
    /// "breadcrumb" or "entry" (the text entry for good).
    pub path_bar: String,
    /// Restored only when its title bar would be on a monitor.
    pub window: Option<WindowGeom>,
    /// Bookmarked folders in sidebar order, as `[[bookmark]]` tables with `path` and an optional
    /// `title` (without one the sidebar shows the folder's name).
    #[serde(rename = "bookmark")]
    pub bookmarks: Vec<Bookmark>,
    /// The bare list of paths older files carry (`bookmarks = ["C:\\x"]`): read, turned into
    /// `[[bookmark]]` entries when loaded, never written.
    #[serde(rename = "bookmarks", skip_serializing)]
    pub legacy_bookmarks: Vec<String>,
}

/// One bookmark: the folder and, if renamed, the title the sidebar shows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bookmark {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl Bookmark {
    pub fn new(path: &str) -> Bookmark {
        Bookmark { path: path.to_string(), title: None }
    }
}

impl Default for Config {
    fn default() -> Config {
        Config {
            start_left: "C:\\".into(),
            start_right: String::new(),
            left_tabs: Vec::new(),
            left_active: 0,
            right_tabs: Vec::new(),
            right_active: 0,
            show_hidden_left: false,
            show_hidden_right: false,
            split: false,
            split_ratio: 0.5,
            sort_column: "name".into(),
            sort_ascending: true,
            show_type: true,
            sidebar_visible: true,
            sidebar_width: 180.0,
            path_bar: "breadcrumb".into(),
            window: None,
            bookmarks: Vec::new(),
            legacy_bookmarks: Vec::new(),
        }
    }
}

/// The settings file's name for a sort column, and back (anything unknown sorts by name).
pub fn sort_name(c: crate::model::SortCol) -> &'static str {
    use crate::model::SortCol;
    match c {
        SortCol::Name => "name",
        SortCol::Size => "size",
        SortCol::Modified => "modified",
        SortCol::Type => "type",
    }
}

pub fn sort_from_name(s: &str) -> crate::model::SortCol {
    use crate::model::SortCol;
    match s {
        "size" => SortCol::Size,
        "modified" => SortCol::Modified,
        "type" => SortCol::Type,
        _ => SortCol::Name,
    }
}

/// What the settings dialog shows and stages, one field per control. Opening it takes the live
/// values; OK applies the fields whose staged value differs (`changes`); Cancel drops them.
#[derive(Clone, Debug, PartialEq)]
pub struct Dialog {
    pub start_left: String,
    pub start_right: String,
    /// The active tab's hidden-file filter in each pane (what Ctrl+H toggles).
    pub show_hidden_left: bool,
    pub show_hidden_right: bool,
    pub show_type: bool,
    pub sidebar_visible: bool,
    /// "breadcrumb" or "entry", as in `Config::path_bar`.
    pub path_bar: String,
}

impl Dialog {
    /// The staged values as they are applied: start folders trimmed, an empty left one the
    /// default (`C:\`), an empty right one left empty (it means the left pane's folder).
    pub fn normalized(mut self) -> Dialog {
        self.start_left = match self.start_left.trim() {
            "" => Config::default().start_left,
            t => t.to_string(),
        };
        self.start_right = self.start_right.trim().to_string();
        self
    }

    fn fields(&self) -> [(&'static str, String); 7] {
        [
            ("start_left", self.start_left.clone()),
            ("start_right", self.start_right.clone()),
            ("show_hidden_left", self.show_hidden_left.to_string()),
            ("show_hidden_right", self.show_hidden_right.to_string()),
            ("show_type", self.show_type.to_string()),
            ("sidebar_visible", self.sidebar_visible.to_string()),
            ("path_bar", self.path_bar.clone()),
        ]
    }

    /// The fields whose `staged` value differs from these (the values when the dialog opened),
    /// as (config key, old, new), in the dialog's order.
    pub fn changes(&self, staged: &Dialog) -> Vec<(&'static str, String, String)> {
        self.fields()
            .into_iter()
            .zip(staged.fields())
            .filter(|((_, old), (_, new))| old != new)
            .map(|((key, old), (_, new))| (key, old, new))
            .collect()
    }
}

/// The tabs a pane opens at start: the saved list (blank entries dropped) with its active index
/// clamped, or one tab on `fallback` (the pane's start folder) when there is no list.
pub fn start_tabs(tabs: &[String], active: usize, fallback: &str) -> (Vec<String>, usize) {
    let tabs: Vec<String> = tabs.iter().filter(|t| !t.trim().is_empty()).cloned().collect();
    if tabs.is_empty() {
        return (vec![fallback.to_string()], 0);
    }
    let active = active.min(tabs.len() - 1);
    (tabs, active)
}

pub const SIDEBAR_MIN: f32 = 120.0;
pub const SIDEBAR_MAX: f32 = 400.0;

impl Config {
    /// Parse a file's text; malformed TOML (or a value of the wrong type) gives the defaults and
    /// the reason.
    pub fn parse(text: &str) -> Result<Config, String> {
        toml::from_str::<Config>(text).map(Config::clamped).map_err(|e| e.to_string())
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }

    /// Values outside their range pulled back in (a hand-edited file), and an old bare bookmark
    /// list turned into entries (after any `[[bookmark]]` ones, duplicates dropped).
    fn clamped(mut self) -> Config {
        for path in std::mem::take(&mut self.legacy_bookmarks) {
            crate::sidebar::add_bookmark(&mut self.bookmarks, &path);
        }
        self.split_ratio = if self.split_ratio.is_finite() { self.split_ratio.clamp(0.15, 0.85) } else { 0.5 };
        self.sidebar_width =
            if self.sidebar_width.is_finite() { self.sidebar_width.clamp(SIDEBAR_MIN, SIDEBAR_MAX) } else { 180.0 };
        self
    }

    /// Read the file at `path`. Missing: the defaults, quietly. Unreadable or malformed: the
    /// defaults and a line on stderr; a malformed file is copied to `<name>.bad` first, since the
    /// next save replaces it.
    pub fn load(path: &Path) -> Config {
        match std::fs::read_to_string(path) {
            Ok(text) => Config::parse(&text).unwrap_or_else(|e| {
                let bad = path.with_extension("toml.bad");
                let kept = std::fs::copy(path, &bad).is_ok();
                eprintln!(
                    "[settings] {} is malformed, using the defaults{}: {}",
                    path.display(),
                    if kept { format!(" (copied to {})", bad.display()) } else { String::new() },
                    e.lines().next().unwrap_or("").trim()
                );
                Config::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(e) => {
                eprintln!("[settings] cannot read {}, using the defaults: {e}", path.display());
                Config::default()
            }
        }
    }

    /// Write to `path` atomically: the text goes to `<path>.tmp`, which then replaces `path`.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        write_atomic(path, &self.to_toml())
    }
}

/// The settings file: `TWINFM_CONFIG` when set; none for a scripted run (`TWINFM_SCRIPT`), so
/// gate, soak and check scripts start from the defaults and never write the user's file;
/// otherwise `%APPDATA%\twinfm\config.toml`.
pub fn config_path() -> Option<PathBuf> {
    match std::env::var_os("TWINFM_CONFIG") {
        Some(p) if !p.is_empty() => return Some(PathBuf::from(p)),
        _ => {}
    }
    if std::env::var_os("TWINFM_SCRIPT").is_some() {
        return None;
    }
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("twinfm").join("config.toml"))
}

/// Write `text` to `<path>.tmp`, then move it over `path` (`MOVEFILE_REPLACE_EXISTING |
/// MOVEFILE_WRITE_THROUGH`), so a crash mid-write never leaves a half-written settings file.
pub fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, text).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    let (from, to) = (crate::shellmenu::wide(tmp.as_os_str()), crate::shellmenu::wide(path.as_os_str()));
    // SAFETY: both are NUL-terminated wide strings that outlive the call.
    let ok = unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) };
    if ok == 0 {
        let e = std::io::Error::last_os_error();
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("replace {}: {e}", path.display()));
    }
    Ok(())
}

/// A monitor's work area, physical px: left, top, right, bottom.
pub type Rect = (i32, i32, i32, i32);

/// Whether a window at physical (`x`, `y`) with physical width `w` keeps enough of its title bar
/// (the top 32 px, at least 100 px of it across) on one of the monitors to be dragged back.
pub fn on_screen(x: i32, y: i32, w: i32, monitors: &[Rect]) -> bool {
    monitors.iter().any(|&(l, t, r, b)| {
        let across = (x + w).min(r) - x.max(l);
        let down = (y + 32).min(b) - y.max(t);
        across >= 100 && down >= 16
    })
}

/// The work areas of the monitors attached now.
pub fn monitors() -> Vec<Rect> {
    use windows_sys::Win32::Foundation::{LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};
    unsafe extern "system" fn each(m: HMONITOR, _dc: HDC, _r: *mut RECT, data: LPARAM) -> windows_sys::core::BOOL {
        // SAFETY: `data` is the `&mut Vec<Rect>` passed below, alive for the whole enumeration.
        let out = unsafe { &mut *(data as *mut Vec<Rect>) };
        let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        // SAFETY: a monitor handle from the enumeration and a sized MONITORINFO.
        if unsafe { GetMonitorInfoW(m, &mut info) } != 0 {
            let w = info.rcWork;
            out.push((w.left, w.top, w.right, w.bottom));
        }
        1
    }
    let mut out: Vec<Rect> = Vec::new();
    // SAFETY: no DC and no clip rectangle (all monitors); the callback only touches `out`.
    unsafe { EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(each), &mut out as *mut Vec<Rect> as LPARAM) };
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trip_unknown_keys_and_malformed() {
        let d = Config::default();
        assert_eq!(Config::parse(&d.to_toml()).unwrap(), d, "defaults survive a round trip");
        let mut c = Config {
            start_left: r"C:\traces\bigdir".into(),
            split: true,
            split_ratio: 0.3,
            bookmarks: vec![Bookmark::new(r"C:\Users"), Bookmark { path: r"D:\".into(), title: Some("Data".into()) }],
            path_bar: "entry".into(),
            window: Some(WindowGeom { x: -1900, y: 40, width: 1100.0, height: 700.0, maximized: false }),
            ..Config::default()
        };
        c.show_hidden_right = true;
        let text = c.to_toml();
        assert_eq!(Config::parse(&text).unwrap(), c);
        // Unknown keys and tables are ignored; missing keys take their defaults.
        let older = "split = true\nfuture_option = 3\n[future_table]\nx = 1\n";
        let parsed = Config::parse(older).unwrap();
        assert!(parsed.split);
        assert_eq!(parsed.start_left, d.start_left);
        // Out-of-range values are pulled in.
        let wild = Config::parse("split_ratio = 9.0\nsidebar_width = 5.0\n").unwrap();
        assert_eq!((wild.split_ratio, wild.sidebar_width), (0.85, SIDEBAR_MIN));
        use crate::model::SortCol;
        for c in [SortCol::Name, SortCol::Size, SortCol::Modified, SortCol::Type] {
            assert_eq!(sort_from_name(sort_name(c)), c);
        }
        assert_eq!(sort_from_name("colour"), SortCol::Name);
        // Malformed text or a wrong type is an error; load turns it into the defaults.
        assert!(Config::parse("split = [").is_err());
        assert!(Config::parse("split = \"yes\"").is_err());
        let dir = std::env::temp_dir().join(format!("twinfm-m6-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bad = dir.join("bad.toml");
        std::fs::write(&bad, "this is = = not toml").unwrap();
        assert_eq!(Config::load(&bad), d);
        assert_eq!(std::fs::read_to_string(dir.join("bad.toml.bad")).unwrap(), "this is = = not toml", "kept aside");
        assert_eq!(Config::load(&dir.join("missing.toml")), d);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tab_lists_round_trip_and_fall_back() {
        let c = Config {
            start_left: r"C:\b".into(),
            left_tabs: vec![r"C:\a".into(), r"C:\b".into(), "This PC".into()],
            left_active: 1,
            right_tabs: vec![r"D:\x".into()],
            ..Config::default()
        };
        let text = c.to_toml();
        assert!(text.contains("left_tabs = ["), "{text}");
        assert_eq!(Config::parse(&text).unwrap(), c);
        let (tabs, active) = start_tabs(&c.left_tabs, c.left_active, &c.start_left);
        assert_eq!((tabs.len(), active), (3, 1));
        // A file from before tabs were saved: one tab on the start folder.
        let old = Config::parse("start_left = 'C:\\Windows'\nstart_right = 'D:\\'\n").unwrap();
        assert!(old.left_tabs.is_empty() && old.right_tabs.is_empty());
        assert_eq!(start_tabs(&old.left_tabs, old.left_active, &old.start_left), (vec![r"C:\Windows".to_string()], 0));
        assert_eq!(start_tabs(&old.right_tabs, old.right_active, &old.start_right), (vec![r"D:\".to_string()], 0));
        // An active index past the end is clamped; blank entries are dropped.
        assert_eq!(start_tabs(&["a".into(), " ".into(), "b".into()], 7, "x"), (vec!["a".to_string(), "b".to_string()], 1));
        assert_eq!(start_tabs(&["".into()], 0, "x"), (vec!["x".to_string()], 0));
    }

    #[test]
    fn bookmark_entries_and_the_old_bare_list() {
        let c = Config {
            bookmarks: vec![Bookmark::new(r"C:\work"), Bookmark { path: r"D:\".into(), title: Some("Data drive".into()) }],
            ..Config::default()
        };
        let text = c.to_toml();
        assert!(text.contains("[[bookmark]]") && text.contains("title = \"Data drive\""), "{text}");
        assert!(!text.contains("bookmarks"), "the old key is never written: {text}");
        assert_eq!(text.matches("title").count(), 1, "no title line for an untitled bookmark: {text}");
        assert_eq!(Config::parse(&text).unwrap(), c);
        // A file from before titles: a bare list of paths, turned into entries on load (and
        // written as [[bookmark]] tables by the next save).
        let old = Config::parse("bookmarks = ['C:\\Users', 'D:\\']\nsplit = true\n").unwrap();
        assert_eq!(old.bookmarks, vec![Bookmark::new(r"C:\Users"), Bookmark::new(r"D:\")]);
        assert!(old.legacy_bookmarks.is_empty());
        assert!(old.to_toml().contains("[[bookmark]]"));
        // Both in one file: the entries first, the old list after them, duplicates dropped.
        let both = Config::parse("bookmarks = ['C:\\a', 'c:\\b\\']\n[[bookmark]]\npath = 'C:\\b'\ntitle = 'B'\n").unwrap();
        assert_eq!(both.bookmarks, vec![Bookmark { path: r"C:\b".into(), title: Some("B".into()) }, Bookmark::new(r"C:\a")]);
    }

    #[test]
    fn show_type_defaults_on_and_round_trips_off() {
        // A file from before the key existed: the Type column is on.
        assert!(Config::parse("split = true\n").unwrap().show_type);
        assert!(Config::default().show_type);
        let c = Config { show_type: false, ..Config::default() };
        let text = c.to_toml();
        assert!(text.contains("show_type = false"), "{text}");
        assert_eq!(Config::parse(&text).unwrap(), c);
    }

    #[test]
    fn dialog_changes_are_the_differing_fields() {
        let open = Dialog {
            start_left: r"C:\".into(),
            start_right: String::new(),
            show_hidden_left: false,
            show_hidden_right: false,
            show_type: true,
            sidebar_visible: true,
            path_bar: "breadcrumb".into(),
        };
        assert!(open.changes(&open.clone().normalized()).is_empty(), "nothing staged, nothing applied");
        let staged = Dialog { show_hidden_left: true, show_type: false, path_bar: "entry".into(), ..open.clone() };
        assert_eq!(
            open.changes(&staged),
            vec![
                ("show_hidden_left", "false".into(), "true".into()),
                ("show_type", "true".into(), "false".into()),
                ("path_bar", "breadcrumb".into(), "entry".into()),
            ]
        );
        // Start folders: trimmed; an empty left one is the default, so blanking it changes nothing.
        let typed = Dialog { start_left: "   ".into(), start_right: "  D:\\work ".into(), ..open.clone() }.normalized();
        assert_eq!(typed.start_left, r"C:\");
        assert_eq!(open.changes(&typed), vec![("start_right", String::new(), r"D:\work".into())]);
        // A folder that does not exist is accepted as typed.
        let missing = Dialog { start_left: r"Q:\nowhere".into(), ..open.clone() }.normalized();
        assert_eq!(open.changes(&missing), vec![("start_left", r"C:\".into(), r"Q:\nowhere".into())]);
    }

    #[test]
    fn maximized_window_round_trip() {
        let c = Config { window: Some(WindowGeom { x: 104, y: 80, width: 1150.0, height: 760.0, maximized: true }), ..Config::default() };
        let text = c.to_toml();
        assert!(text.contains("maximized = true"), "{text}");
        assert_eq!(Config::parse(&text).unwrap(), c);
        // A file from before the flag existed: not maximised, the rest as saved.
        let old = Config::parse("[window]\nx = 10\ny = 20\nwidth = 900.0\nheight = 600.0\n").unwrap();
        assert_eq!(old.window, Some(WindowGeom { x: 10, y: 20, width: 900.0, height: 600.0, maximized: false }));
        // No window handle, no placement.
        assert_eq!(placement(0, 1.0), None);
    }

    #[test]
    fn atomic_write_replaces_an_existing_file() {
        let dir = std::env::temp_dir().join(format!("twinfm-m6-atomic-{}", std::process::id()));
        let path = dir.join("sub").join("config.toml");
        write_atomic(&path, "first = 1\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first = 1\n", "creates the folder and the file");
        write_atomic(&path, "second = 2\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second = 2\n", "replaces it");
        assert!(!path.with_extension("toml.tmp").exists(), "no .tmp left behind");
        let c = Config { bookmarks: vec![Bookmark::new("C:\\x")], ..Config::default() };
        c.save(&path).unwrap();
        assert_eq!(Config::load(&path), c);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn window_position_on_screen() {
        let one = [(0, 0, 1920, 1040)];
        assert!(on_screen(100, 100, 1200, &one));
        assert!(on_screen(-1100, 0, 1200, &one), "100 px of the title bar still on screen");
        assert!(!on_screen(-1150, 0, 1200, &one), "only 50 px left");
        assert!(!on_screen(100, 1030, 1200, &one), "title bar below the work area");
        assert!(!on_screen(100, -30, 1200, &one), "title bar above it");
        assert!(!on_screen(3000, 100, 1200, &one), "off to the right");
        // A second monitor to the left (negative coordinates) makes the same spot fine.
        let two = [(0, 0, 1920, 1040), (-1920, 0, 0, 1040)];
        assert!(on_screen(-1800, 200, 1200, &two));
        assert!(!on_screen(100, 100, 1200, &[]), "no monitors at all");
        assert!(!monitors().is_empty(), "this machine has a monitor");
    }
}
