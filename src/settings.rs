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
    /// Each pane's tabs, left to right, as `[[left_tabs]]` tables (`SavedTab`: the folder, and
    /// since P6-M3 its sort, hidden files, cursor and scroll), and the index of its active tab
    /// (`left_active = 1`), as TwinFM last saved them. A bare list of paths (`left_tabs = ["C:\\"]`,
    /// before P6-M3) still loads. Empty or missing: the pane opens one tab on its start folder.
    /// The right pane's tabs are restored only when the split is.
    pub left_tabs: Vec<SavedTab>,
    pub left_active: usize,
    pub right_tabs: Vec<SavedTab>,
    pub right_active: usize,
    /// New tabs in each pane show files with the hidden attribute (Ctrl+H sets it for its pane).
    pub show_hidden_left: bool,
    pub show_hidden_right: bool,
    /// Each pane shows protected operating system files (hidden and system both) while it shows
    /// hidden files (P6-M3; the settings dialog only). A file with the system attribute alone is
    /// always shown. Missing in an older file: off.
    pub show_protected_left: bool,
    pub show_protected_right: bool,
    /// The second pane is open, and the split line's position (0.15 to 0.85 of the panes' width).
    pub split: bool,
    pub split_ratio: f32,
    /// The sort new tabs start with: "name", "size", "modified" or "type" (a header click sets it).
    pub sort_column: String,
    pub sort_ascending: bool,
    /// The Type column in both panes (the settings dialog sets it). Missing in an older file: on.
    pub show_type: bool,
    /// Thumbnails for image, video and document files, in 32 px rows with 32 px icons, in both
    /// panes (the settings dialog sets it). Missing in an older file: off (22 px rows).
    pub thumbnails: bool,
    /// The details list's rows without thumbnails (P12-M4): "compact" (22 px) or "comfortable" (30
    /// px, the 16 px icons centred in them; the dialog sets it). Missing in an older file, or anything
    /// else: compact. The thumbnail lists and the grids are unchanged.
    pub row_height: String,
    /// Each pane's view: "details" (the list) or "icons" (the icon grid, Ctrl+Shift+2 and
    /// Ctrl+Shift+3, or the settings dialog). Missing in an older file: details. Anything else
    /// reads as details.
    pub view_left: String,
    pub view_right: String,
    /// Each pane's grid tiles (P6-M2): "large" or "medium" (Ctrl+Shift+3). Missing in an older
    /// file: large. Anything else reads as large. Kept while the pane shows the details list.
    pub tile_left: String,
    pub tile_right: String,
    pub sidebar_visible: bool,
    pub sidebar_width: f32,
    /// "breadcrumb" or "entry" (the text entry for good).
    pub path_bar: String,
    /// Where the toolbar is (P6-M3): "top" (labelled, across the window, acting on the active
    /// pane), "pane" (icons only, inside each pane above its path bar) or "none". Missing in an
    /// older file: top. Anything else reads as top.
    pub toolbar: String,
    /// The colour scheme (P10-M1 task 7, Jeremy's "Let's make light/dark a selectable switch inside
    /// twinfm"): "system" (Windows' app mode, followed live), "light" or "dark". Missing in an older
    /// file: system. Anything else reads as system. `TWINFM_SCHEME` (scripts) takes precedence.
    pub theme: String,
    /// The preview column at the window's right edge (P8-M1; Alt+P, the toolbar's Preview button,
    /// the dialog), and its share of the space right of the sidebar (0.2 to 0.6, its handle's
    /// drag). Missing in an older file: off, 0.35.
    pub preview: bool,
    pub preview_width: f32,
    /// A PDF's zoom in the column, percent (P11-M4; `preview::ZOOMS`, 50 to 400). Missing: 100.
    pub preview_zoom: u32,
    /// Text and source in the preview coloured by language (P11-M5, in the dialog). Missing: on.
    pub syntax_colours: bool,
    /// Write the stderr lines to `twinfm.log` beside this file (P11-M7, `logfile`; in the dialog).
    /// Missing: off.
    pub log: bool,
    /// Each pane's column widths in the details list (P9-M1), logical px: Type, Size and Modified
    /// (`cols`; a header grip's drag or double-click sets them). Missing in an older file: 120, 90,
    /// 140. Pulled into 40 to 2,000 px on load.
    pub col_type_left: f32,
    pub col_size_left: f32,
    pub col_modified_left: f32,
    pub col_type_right: f32,
    pub col_size_right: f32,
    pub col_modified_right: f32,
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

/// One saved tab (P6-M3): its folder and, when TwinFM saved them, its sort (`sort_name`), its
/// hidden-file switch, the name of its cursor row and its list's scroll offset (logical px, zero or
/// negative). A missing field (a bare path from an older file) takes the default a new tab has.
/// Since P11-M3 its history too: the places Back and Forward go to (`back` newest last, `forward`
/// next last, at most `HISTORY_SAVED` each).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "TabEntry")]
pub struct SavedTab {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ascending: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub back: Vec<SavedPlace>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub forward: Vec<SavedPlace>,
}

/// The places of a tab's history saved each way (P11-M3).
pub const HISTORY_SAVED: usize = 50;

/// One place of a saved history: a folder, or a search (its pattern, `content` for a search inside
/// files) under the folder, which runs again when Back or Forward reaches it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedPlace {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub content: bool,
}

impl SavedTab {
    /// A tab that is only a folder (an old entry, a start folder).
    pub fn path(path: &str) -> SavedTab {
        SavedTab { path: path.into(), sort: None, ascending: None, hidden: None, cursor: None, scroll: None, back: Vec::new(), forward: Vec::new() }
    }
}

impl From<&str> for SavedTab {
    fn from(path: &str) -> SavedTab {
        SavedTab::path(path)
    }
}

/// What a `left_tabs` / `right_tabs` entry may be in the file: a table, or a bare path.
#[derive(Deserialize)]
#[serde(untagged)]
enum TabEntry {
    Path(String),
    Table {
        path: String,
        sort: Option<String>,
        ascending: Option<bool>,
        hidden: Option<bool>,
        cursor: Option<String>,
        scroll: Option<f32>,
        #[serde(default)]
        back: Vec<SavedPlace>,
        #[serde(default)]
        forward: Vec<SavedPlace>,
    },
}

impl From<TabEntry> for SavedTab {
    fn from(e: TabEntry) -> SavedTab {
        match e {
            TabEntry::Path(p) => SavedTab::path(&p),
            TabEntry::Table { path, sort, ascending, hidden, cursor, scroll, back, forward } => {
                SavedTab { path, sort, ascending, hidden, cursor, scroll, back, forward }
            }
        }
    }
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
            show_protected_left: false,
            show_protected_right: false,
            split: false,
            split_ratio: 0.5,
            sort_column: "name".into(),
            sort_ascending: true,
            show_type: true,
            thumbnails: false,
            row_height: "compact".into(),
            view_left: "details".into(),
            view_right: "details".into(),
            tile_left: "large".into(),
            tile_right: "large".into(),
            sidebar_visible: true,
            sidebar_width: 180.0,
            path_bar: "breadcrumb".into(),
            toolbar: "top".into(),
            theme: "system".into(),
            preview: false,
            preview_width: crate::preview::RATIO_DEFAULT,
            preview_zoom: 100,
            syntax_colours: true,
            log: false,
            col_type_left: crate::cols::DEFAULT.ty,
            col_size_left: crate::cols::DEFAULT.size,
            col_modified_left: crate::cols::DEFAULT.modified,
            col_type_right: crate::cols::DEFAULT.ty,
            col_size_right: crate::cols::DEFAULT.size,
            col_modified_right: crate::cols::DEFAULT.modified,
            window: None,
            bookmarks: Vec::new(),
            legacy_bookmarks: Vec::new(),
        }
    }
}

/// A pane's view: the details list, or the grid at large or medium tiles (Ctrl+Shift+1, 2, 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Details,
    Large,
    Medium,
}

impl View {
    /// From the settings file's `view_*` and `tile_*` values: "icons" is the grid, anything else
    /// the details list; "medium" the medium tiles, anything else the large ones.
    pub fn from_config(view: &str, tile: &str) -> View {
        match (view == "icons", tile == "medium") {
            (false, _) => View::Details,
            (true, false) => View::Large,
            (true, true) => View::Medium,
        }
    }

    /// Ctrl+Shift+1, 2 and 3 (`set-view`'s int) and the dialog's choice: 0 details, 1 large, 2
    /// medium (anything else details).
    pub fn from_index(i: i32) -> View {
        match i {
            1 => View::Large,
            2 => View::Medium,
            _ => View::Details,
        }
    }

    pub fn index(self) -> i32 {
        match self {
            View::Details => 0,
            View::Large => 1,
            View::Medium => 2,
        }
    }

    pub fn is_grid(self) -> bool {
        self != View::Details
    }

    pub fn is_medium(self) -> bool {
        self == View::Medium
    }

    /// The `view_*` value it saves as.
    pub fn view_value(self) -> &'static str {
        if self.is_grid() { "icons" } else { "details" }
    }

    /// The dialog's name for it (`settings set view_left <name>`, the `[settings]` lines).
    pub fn name(self) -> &'static str {
        match self {
            View::Details => "details",
            View::Large => "large",
            View::Medium => "medium",
        }
    }

    pub fn from_name(s: &str) -> View {
        match s {
            "large" => View::Large,
            "medium" => View::Medium,
            _ => View::Details,
        }
    }

    /// For the `[view]` lines.
    pub fn label(self) -> &'static str {
        match self {
            View::Details => "details",
            View::Large => "large icons",
            View::Medium => "medium icons",
        }
    }
}

impl Config {
    /// Pane `pane`'s view, from `view_*` and `tile_*`.
    pub fn view(&self, pane: usize) -> View {
        if pane == 0 { View::from_config(&self.view_left, &self.tile_left) } else { View::from_config(&self.view_right, &self.tile_right) }
    }

    /// Set pane `pane`'s view: `view_*` always, `tile_*` for a grid (the details list keeps the
    /// last tile size, so Ctrl+Shift+1 then back to the grid keeps it in the file too).
    pub fn set_view(&mut self, pane: usize, v: View) {
        let (view, tile) = if pane == 0 { (&mut self.view_left, &mut self.tile_left) } else { (&mut self.view_right, &mut self.tile_right) };
        *view = v.view_value().into();
        if v.is_grid() {
            *tile = if v.is_medium() { "medium" } else { "large" }.into();
        }
    }

    /// Pane `pane`'s column widths.
    pub fn cols(&self, pane: usize) -> crate::cols::Widths {
        if pane == 0 {
            crate::cols::Widths { ty: self.col_type_left, size: self.col_size_left, modified: self.col_modified_left }
        } else {
            crate::cols::Widths { ty: self.col_type_right, size: self.col_size_right, modified: self.col_modified_right }
        }
    }

    pub fn set_cols(&mut self, pane: usize, w: crate::cols::Widths) {
        let (t, s, m) = if pane == 0 {
            (&mut self.col_type_left, &mut self.col_size_left, &mut self.col_modified_left)
        } else {
            (&mut self.col_type_right, &mut self.col_size_right, &mut self.col_modified_right)
        };
        (*t, *s, *m) = (w.ty, w.size, w.modified);
    }
}

/// Where the toolbar is (`Config::toolbar`; the app's `toolbar` property is `index`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toolbar {
    None,
    Top,
    Pane,
}

impl Toolbar {
    /// "none" and "pane" as written; anything else (a missing key, a typo) the default, top.
    pub fn from_name(s: &str) -> Toolbar {
        match s {
            "none" => Toolbar::None,
            "pane" => Toolbar::Pane,
            _ => Toolbar::Top,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Toolbar::None => "none",
            Toolbar::Top => "top",
            Toolbar::Pane => "pane",
        }
    }

    /// The app's `toolbar` property and the dialog's choice: 0 none, 1 top, 2 in each pane.
    pub fn index(self) -> i32 {
        match self {
            Toolbar::None => 0,
            Toolbar::Top => 1,
            Toolbar::Pane => 2,
        }
    }

    pub fn from_index(i: i32) -> Toolbar {
        match i {
            0 => Toolbar::None,
            2 => Toolbar::Pane,
            _ => Toolbar::Top,
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

/// The colour scheme (`Config::theme`; the dialog's choice is `index`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

impl ThemeMode {
    /// "light" and "dark" as written; anything else (a missing key, a typo) the default, system.
    pub fn from_name(s: &str) -> ThemeMode {
        match s {
            "light" => ThemeMode::Light,
            "dark" => ThemeMode::Dark,
            _ => ThemeMode::System,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }

    /// The dialog's choice: 0 system, 1 light, 2 dark.
    pub fn index(self) -> i32 {
        match self {
            ThemeMode::System => 0,
            ThemeMode::Light => 1,
            ThemeMode::Dark => 2,
        }
    }

    pub fn from_index(i: i32) -> ThemeMode {
        match i {
            1 => ThemeMode::Light,
            2 => ThemeMode::Dark,
            _ => ThemeMode::System,
        }
    }

    /// Whether the native title bar is drawn dark for this mode (P10-M3): light and dark say so
    /// themselves; system takes Windows' app mode, `system_dark`.
    pub fn title_bar_dark(self, system_dark: bool) -> bool {
        match self {
            ThemeMode::Light => false,
            ThemeMode::Dark => true,
            ThemeMode::System => system_dark,
        }
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
    /// Each pane's protected-files setting (`Config::show_protected_*`).
    pub show_protected_left: bool,
    pub show_protected_right: bool,
    /// Each pane's view, as `View::name`: "details", "large" or "medium" (P6-M2).
    pub view_left: String,
    pub view_right: String,
    pub show_type: bool,
    pub thumbnails: bool,
    /// "compact" or "comfortable", as in `Config::row_height` (P12-M4).
    pub row_height: String,
    pub sidebar_visible: bool,
    /// "breadcrumb" or "entry", as in `Config::path_bar`.
    pub path_bar: String,
    /// "top", "pane" or "none", as in `Config::toolbar`.
    pub toolbar: String,
    /// "system", "light" or "dark", as in `Config::theme` (P10-M1 task 7).
    pub theme: String,
    /// The preview column (`Config::preview`).
    pub preview: bool,
    /// `Config::syntax_colours` (P11-M5).
    pub syntax_colours: bool,
    /// `Config::log` (P11-M7).
    pub log: bool,
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

    fn fields(&self) -> [(&'static str, String); 18] {
        [
            ("start_left", self.start_left.clone()),
            ("start_right", self.start_right.clone()),
            ("show_hidden_left", self.show_hidden_left.to_string()),
            ("show_hidden_right", self.show_hidden_right.to_string()),
            ("show_protected_left", self.show_protected_left.to_string()),
            ("show_protected_right", self.show_protected_right.to_string()),
            ("view_left", self.view_left.clone()),
            ("view_right", self.view_right.clone()),
            ("show_type", self.show_type.to_string()),
            ("thumbnails", self.thumbnails.to_string()),
            ("row_height", self.row_height.clone()),
            ("sidebar_visible", self.sidebar_visible.to_string()),
            ("path_bar", self.path_bar.clone()),
            ("toolbar", self.toolbar.clone()),
            ("theme", self.theme.clone()),
            ("preview", self.preview.to_string()),
            ("syntax_colours", self.syntax_colours.to_string()),
            ("log", self.log.to_string()),
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
pub fn start_tabs(tabs: &[SavedTab], active: usize, fallback: &str) -> (Vec<SavedTab>, usize) {
    let tabs: Vec<SavedTab> = tabs.iter().filter(|t| !t.path.trim().is_empty()).cloned().collect();
    if tabs.is_empty() {
        return (vec![SavedTab::path(fallback)], 0);
    }
    let active = active.min(tabs.len() - 1);
    (tabs, active)
}

pub const SIDEBAR_MIN: f32 = 120.0;
pub const SIDEBAR_MAX: f32 = 400.0;

impl Config {
    /// Comfortable rows (`row_height`, P12-M4).
    pub fn comfortable(&self) -> bool {
        self.row_height == "comfortable"
    }

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
        self.preview_width = crate::preview::clamp_ratio(self.preview_width);
        self.preview_zoom = crate::preview::clamp_zoom(self.preview_zoom);
        for pane in 0..2 {
            self.set_cols(pane, self.cols(pane).clamped());
        }
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
    fn column_widths_round_trip_default_and_clamp() {
        use crate::cols::{Widths, DEFAULT, FLOOR, MAX};
        let d = Config::default();
        assert_eq!((d.cols(0), d.cols(1)), (DEFAULT, DEFAULT), "a file without the keys loads today's layout");
        assert_eq!(Config::parse("split = true
").unwrap().cols(1), DEFAULT);
        let mut c = Config::default();
        c.set_cols(0, Widths { ty: 200.0, size: 110.0, modified: 180.0 });
        c.set_cols(1, Widths { ty: 64.5, size: 90.0, modified: 300.0 });
        let text = c.to_toml();
        assert!(text.contains("col_type_left = 200.0") && text.contains("col_modified_right = 300.0"), "{text}");
        let back = Config::parse(&text).unwrap();
        assert_eq!((back.cols(0), back.cols(1)), (c.cols(0), c.cols(1)));
        let wild = Config::parse("col_type_left = 3.0
col_size_right = 99999.0
").unwrap();
        assert_eq!((wild.cols(0).ty, wild.cols(1).size, wild.cols(1).ty), (FLOOR, MAX, DEFAULT.ty));
    }

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
        let wild = Config::parse("split_ratio = 9.0\nsidebar_width = 5.0\npreview_width = 0.9\n").unwrap();
        assert_eq!((wild.split_ratio, wild.sidebar_width, wild.preview_width), (0.85, SIDEBAR_MIN, 0.6));
        assert_eq!((d.preview, d.preview_width), (false, 0.35), "the preview column is off by default (P8-M1)");
        assert!(d.syntax_colours && !Config::parse("syntax_colours = false\n").unwrap().syntax_colours, "P11-M5");
        assert!(!d.log && Config::parse("log = true\n").unwrap().log, "P11-M7");
        assert_eq!((d.preview_zoom, Config::parse("preview_zoom = 5000\n").unwrap().preview_zoom), (100, 400), "P11-M4");
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
        let full = SavedTab {
            path: r"C:\b".into(),
            sort: Some("size".into()),
            ascending: Some(false),
            hidden: Some(true),
            cursor: Some("notes.txt".into()),
            scroll: Some(-440.0),
            // P11-M3: its history, a content search among the places.
            back: vec![
                SavedPlace { path: r"C:\a".into(), search: None, content: false },
                SavedPlace { path: r"C:\a".into(), search: Some("needle".into()), content: true },
            ],
            forward: vec![SavedPlace { path: r"C:\b\sub".into(), search: None, content: false }],
        };
        let c = Config {
            start_left: r"C:\b".into(),
            left_tabs: vec![r"C:\a".into(), full.clone(), "This PC".into()],
            left_active: 1,
            right_tabs: vec![r"D:\x".into()],
            ..Config::default()
        };
        let text = c.to_toml();
        assert!(text.contains("[[left_tabs]]") && text.contains("cursor = \"notes.txt\""), "{text}");
        assert_eq!(Config::parse(&text).unwrap(), c);
        let (tabs, active) = start_tabs(&c.left_tabs, c.left_active, &c.start_left);
        assert_eq!((tabs.len(), active), (3, 1));
        assert_eq!(tabs[1], full, "a tab's view comes back with it (P6-M3)");
        // A bare list of paths, from before P6-M3: folders only.
        let bare = Config::parse("left_tabs = ['C:\\a', 'D:\\']\nleft_active = 1\n").unwrap();
        assert_eq!(bare.left_tabs, vec![SavedTab::path(r"C:\a"), SavedTab::path(r"D:\")]);
        // A file from before tabs were saved: one tab on the start folder.
        let old = Config::parse("start_left = 'C:\\Windows'\nstart_right = 'D:\\'\n").unwrap();
        assert!(old.left_tabs.is_empty() && old.right_tabs.is_empty());
        assert_eq!(start_tabs(&old.left_tabs, old.left_active, &old.start_left), (vec![SavedTab::path(r"C:\Windows")], 0));
        assert_eq!(start_tabs(&old.right_tabs, old.right_active, &old.start_right), (vec![SavedTab::path(r"D:\")], 0));
        // An active index past the end is clamped; blank entries are dropped.
        assert_eq!(start_tabs(&["a".into(), " ".into(), "b".into()], 7, "x"), (vec![SavedTab::path("a"), SavedTab::path("b")], 1));
        assert_eq!(start_tabs(&["".into()], 0, "x"), (vec![SavedTab::path("x")], 0));
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
    fn view_defaults_to_details_and_round_trips_icons() {
        // A file from before the keys existed: both panes show the details list.
        let old = Config::parse("split = true\n").unwrap();
        assert_eq!((old.view(0), old.view(1)), (View::Details, View::Details));
        // A file from before P6-M2 (no tile keys): its grid is the large one.
        let p4 = Config::parse("view_right = \"icons\"\n").unwrap();
        assert_eq!((p4.view(0), p4.view(1)), (View::Details, View::Large));
        assert_eq!(View::from_config("icons", "medium"), View::Medium);
        assert_eq!(View::from_config("icons", "huge"), View::Large, "an unknown tile is large");
        assert_eq!(View::from_config("tiles", "medium"), View::Details, "an unknown view is details");
        let mut c = Config::default();
        c.set_view(1, View::Medium);
        let text = c.to_toml();
        assert!(text.contains("view_left = \"details\"") && text.contains("view_right = \"icons\""), "{text}");
        assert!(text.contains("tile_left = \"large\"") && text.contains("tile_right = \"medium\""), "{text}");
        let back = Config::parse(&text).unwrap();
        assert_eq!((back.view(0), back.view(1)), (View::Details, View::Medium));
        // Back to details keeps the last tile size in the file.
        c.set_view(1, View::Details);
        assert_eq!((c.view_right.as_str(), c.tile_right.as_str()), ("details", "medium"));
        c.set_view(1, View::Large);
        assert_eq!((c.view_right.as_str(), c.tile_right.as_str()), ("icons", "large"));
        for v in [View::Details, View::Large, View::Medium] {
            assert_eq!(View::from_index(v.index()), v);
            assert_eq!(View::from_name(v.name()), v);
        }
        assert_eq!(View::from_index(7), View::Details);
        assert_eq!(View::from_name("icons"), View::Details, "the dialog's names, not the file's");
        assert!(View::Medium.is_grid() && View::Medium.is_medium() && View::Large.is_grid() && !View::Details.is_grid());
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
    fn thumbnails_default_off_and_round_trip_on() {
        // A file from before the key existed: no thumbnails, 22 px rows.
        assert!(!Config::parse("split = true
show_type = false
").unwrap().thumbnails);
        assert!(!Config::default().thumbnails);
        let c = Config { thumbnails: true, ..Config::default() };
        let text = c.to_toml();
        assert!(text.contains("thumbnails = true"), "{text}");
        assert_eq!(Config::parse(&text).unwrap(), c);
    }

    #[test]
    fn dialog_changes_are_the_differing_fields() {
        let open = Dialog {
            start_left: r"C:\".into(),
            start_right: String::new(),
            show_hidden_left: false,
            show_hidden_right: false,
            show_protected_left: false,
            show_protected_right: false,
            view_left: "details".into(),
            view_right: "large".into(),
            show_type: true,
            thumbnails: false,
            row_height: "compact".into(),
            sidebar_visible: true,
            path_bar: "breadcrumb".into(),
            toolbar: "top".into(),
            theme: "system".into(),
            preview: false,
            syntax_colours: true,
            log: false,
        };
        assert!(open.changes(&open.clone().normalized()).is_empty(), "nothing staged, nothing applied");
        let staged = Dialog { show_hidden_left: true, show_protected_right: true, view_right: "medium".into(), show_type: false, thumbnails: true, row_height: "comfortable".into(), path_bar: "entry".into(), toolbar: "pane".into(), theme: "dark".into(), preview: true, syntax_colours: false, log: true, ..open.clone() };
        assert_eq!(
            open.changes(&staged),
            vec![
                ("show_hidden_left", "false".into(), "true".into()),
                ("show_protected_right", "false".into(), "true".into()),
                ("view_right", "large".into(), "medium".into()),
                ("show_type", "true".into(), "false".into()),
                ("thumbnails", "false".into(), "true".into()),
                ("row_height", "compact".into(), "comfortable".into()),
                ("path_bar", "breadcrumb".into(), "entry".into()),
                ("toolbar", "top".into(), "pane".into()),
                ("theme", "system".into(), "dark".into()),
                ("preview", "false".into(), "true".into()),
                ("syntax_colours", "true".into(), "false".into()),
                ("log", "false".into(), "true".into()),
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
    fn row_height_defaults_to_compact_and_round_trips() {
        assert!(!Config::default().comfortable());
        // A file from before P12-M4 has no key: compact.
        assert!(!Config::parse("split = true\n").unwrap().comfortable());
        let c = Config::parse("row_height = \"comfortable\"\n").unwrap();
        assert!(c.comfortable());
        assert!(Config::parse(&c.to_toml()).unwrap().comfortable(), "written and read back");
        assert!(!Config::parse("row_height = \"tall\"\n").unwrap().comfortable(), "anything else: compact");
    }

    #[test]
    fn toolbar_defaults_to_top_and_round_trips() {
        assert_eq!(Toolbar::from_name(&Config::default().toolbar), Toolbar::Top);
        // A file from before P6-M3 has no key: top.
        assert_eq!(Config::parse("split = true\n").unwrap().toolbar, "top");
        for t in [Toolbar::None, Toolbar::Top, Toolbar::Pane] {
            assert_eq!(Toolbar::from_name(t.name()), t);
            assert_eq!(Toolbar::from_index(t.index()), t);
            let c = Config { toolbar: t.name().into(), ..Config::default() };
            assert_eq!(Config::parse(&c.to_toml()).unwrap(), c);
        }
        assert_eq!(Toolbar::from_name("left"), Toolbar::Top, "anything else is the default");
    }

    #[test]
    fn theme_defaults_to_system_and_round_trips() {
        assert_eq!(ThemeMode::from_name(&Config::default().theme), ThemeMode::System);
        // A file from before P10-M1 has no key: system.
        assert_eq!(Config::parse("split = true\n").unwrap().theme, "system");
        for t in [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark] {
            assert_eq!(ThemeMode::from_name(t.name()), t);
            assert_eq!(ThemeMode::from_index(t.index()), t);
            let c = Config { theme: t.name().into(), ..Config::default() };
            assert_eq!(Config::parse(&c.to_toml()).unwrap(), c);
        }
        assert_eq!(ThemeMode::from_name("auto"), ThemeMode::System, "anything else is the default");
    }

    #[test]
    fn the_title_bar_follows_the_mode_and_system_follows_windows() {
        assert!(!ThemeMode::Light.title_bar_dark(true));
        assert!(ThemeMode::Dark.title_bar_dark(false));
        assert!(ThemeMode::System.title_bar_dark(true));
        assert!(!ThemeMode::System.title_bar_dark(false));
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
