//! The Windows 11 style context menu (P7-M2): which items it holds for a selection, where it
//! opens, and how the keyboard moves along it. Pure functions; `main.rs` builds the Slint models
//! from them and runs the picks, and `explorercmd` (on the verb worker) supplies the packaged
//! apps' commands and the Open with apps.
//!
//! The layout mirrors Explorer's Windows 11 menu, as Jeremy approved it on 2026-10-18
//! (`docs/phase-7-plan.md` § P7-M2): a row of icon buttons (Cut, Copy, Rename, Share, Delete where
//! they apply), Open, Open with, TwinFM's own Open in other pane and Open in terminal, the packaged
//! apps' commands, Compress to, Copy path, Properties, and Show more options for the classic menu.
//! Every item calls the command its key, its toolbar button or its P7-M1 route calls.

use crate::explorercmd::{Command, Pixels};
use crate::toolbar::Place;

/// The menu's width, a submenu's, and its parts' heights, in logical px.
pub const MENU_W: f32 = 280.0;
pub const SUB_W: f32 = 250.0;
pub const PAD: f32 = 4.0;
pub const TOOLS_H: f32 = 44.0;
pub const ITEM_H: f32 = 30.0;
pub const SEP_H: f32 = 9.0;
/// The gap kept between the menu and the window's edges.
pub const MARGIN: f32 = 4.0;

/// A menu item's command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    Cut,
    Copy,
    Rename,
    Share,
    Delete,
    Paste,
    Open,
    /// The Open with submenu.
    OpenWith,
    OpenOther,
    Terminal,
    /// The Compress to submenu.
    Compress,
    Zip,
    /// Compress to > 7z file and TAR file: TwinFM's own writers, as file jobs (P7-M3).
    SevenZ,
    Tar,
    CopyPath,
    Properties,
    More,
    NewFolder,
    NewFile,
    /// Open with's "Choose another app" (the shell's dialog).
    ChooseApp,
    /// The packaged command at that index of the worker's answer.
    Packaged(usize),
    /// Its own command at that index.
    Sub(usize, usize),
    /// The Open with app at that index of the worker's answer.
    App(usize),
}

impl Cmd {
    const FIXED: [Cmd; 20] = [
        Cmd::Cut,
        Cmd::Copy,
        Cmd::Rename,
        Cmd::Share,
        Cmd::Delete,
        Cmd::Paste,
        Cmd::Open,
        Cmd::OpenWith,
        Cmd::OpenOther,
        Cmd::Terminal,
        Cmd::Compress,
        Cmd::Zip,
        Cmd::CopyPath,
        Cmd::Properties,
        Cmd::More,
        Cmd::NewFolder,
        Cmd::NewFile,
        Cmd::ChooseApp,
        Cmd::SevenZ,
        Cmd::Tar,
    ];

    /// The id the Slint model carries: 1 to 20 for the fixed commands, 100 + i for a packaged
    /// command, 500 + k for an app, 1000 + 100 i + j for a command's own command.
    pub fn id(self) -> i32 {
        match self {
            Cmd::Packaged(i) => 100 + i as i32,
            Cmd::App(k) => 500 + k as i32,
            Cmd::Sub(i, j) => 1000 + 100 * i as i32 + j as i32,
            c => Cmd::FIXED.iter().position(|f| *f == c).map_or(0, |p| p as i32 + 1),
        }
    }

    pub fn from_id(id: i32) -> Option<Cmd> {
        match id {
            1..=20 => Some(Cmd::FIXED[id as usize - 1]),
            100..=499 => Some(Cmd::Packaged(id as usize - 100)),
            500..=999 => Some(Cmd::App(id as usize - 500)),
            1000.. => Some(Cmd::Sub((id as usize - 1000) / 100, (id as usize - 1000) % 100)),
            _ => None,
        }
    }

    /// The command of TwinFM's own key or button this item calls (`main.rs` `command`), when it is
    /// one.
    pub fn key_command(self) -> Option<&'static str> {
        Some(match self {
            Cmd::Cut => "cx",
            Cmd::Copy => "cc",
            Cmd::Paste => "cv",
            Cmd::Rename => "rename",
            Cmd::Delete => "delete",
            Cmd::NewFolder => "newfolder",
            Cmd::NewFile => "newfile",
            Cmd::Properties => "properties",
            _ => return None,
        })
    }

    /// Runs on the verb worker (a shell command or an app's): the driver's `menupick` refuses it.
    pub fn on_worker(self) -> bool {
        matches!(self, Cmd::Share | Cmd::Zip | Cmd::Properties | Cmd::ChooseApp | Cmd::Packaged(_) | Cmd::Sub(..) | Cmd::App(_))
    }
}

/// What the menu is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum What {
    /// The folder's background.
    Background,
    /// The selection: how many files, folders and drives.
    Items { files: usize, dirs: usize, drives: usize },
}

#[derive(Clone, Copy, Debug)]
pub struct Ctx {
    pub place: Place,
    pub what: What,
    /// The clipboard holds files (Paste applies).
    pub clipboard_files: bool,
    /// Every item is a file of one type (`same_type`): Open with applies.
    pub same_type: bool,
}

/// The items are files of one type, so Open with applies (P8-L1): one file, or several whose
/// extensions match, ignoring case, none of them empty. `exts` holds each item's extension, None
/// for a folder or a drive. Explorer offers no Open with for several items; TwinFM does when they
/// share a type (Jeremy, 2026-10-20).
pub fn same_type(exts: &[Option<&str>]) -> bool {
    match exts {
        [] => false,
        [one] => one.is_some(),
        [first, rest @ ..] => first.is_some_and(|f| !f.is_empty() && rest.iter().all(|e| e.is_some_and(|e| e.eq_ignore_ascii_case(f)))),
    }
}

/// One icon-row button.
#[derive(Clone, Debug, PartialEq)]
pub struct Tool {
    pub cmd: Cmd,
    pub glyph: &'static str,
    pub text: &'static str,
}

/// One row of the menu or a submenu: an item, or a separator (`cmd` None).
#[derive(Clone, Debug)]
pub struct Entry {
    pub cmd: Option<Cmd>,
    pub text: String,
    /// A Segoe Fluent Icons glyph, when the item has no icon of its own.
    pub glyph: &'static str,
    pub icon: Option<Pixels>,
    /// The key shown at its right ("Enter", "Shift+F10").
    pub key: &'static str,
    /// It opens a submenu.
    pub sub: bool,
    pub enabled: bool,
}

impl Entry {
    fn item(cmd: Cmd, text: &str, glyph: &'static str, key: &'static str) -> Entry {
        Entry { cmd: Some(cmd), text: text.into(), glyph, icon: None, key, sub: matches!(cmd, Cmd::OpenWith | Cmd::Compress), enabled: true }
    }

    fn sep() -> Entry {
        Entry { cmd: None, text: String::new(), glyph: "", icon: None, key: "", sub: false, enabled: false }
    }

    pub fn is_sep(&self) -> bool {
        self.cmd.is_none()
    }
}

/// The glyphs (Segoe Fluent Icons, the toolbar's font).
const G_CUT: &str = "\u{E8C6}";
const G_COPY: &str = "\u{E8C8}";
const G_RENAME: &str = "\u{E8AC}";
const G_SHARE: &str = "\u{E72D}";
const G_DELETE: &str = "\u{E74D}";
const G_PASTE: &str = "\u{E77F}";
const G_OPEN: &str = "\u{E8E5}";
const G_OPEN_WITH: &str = "\u{E7AC}";
const G_OTHER_PANE: &str = "\u{E8A0}";
const G_TERMINAL: &str = "\u{E756}";
const G_ZIP: &str = "\u{F012}";
const G_COPY_PATH: &str = "\u{E71B}";
const G_PROPERTIES: &str = "\u{E946}";
const G_MORE: &str = "\u{E712}";
const G_NEW_FOLDER: &str = "\u{E8F4}";
const G_NEW_FILE: &str = "\u{E8A5}";

/// The icon row for `ctx`: the buttons that apply, in Explorer's order (Cut, Copy, Rename, Share,
/// Delete; Paste on the background). Which apply is the toolbar's rule (`toolbar::state`); Share
/// takes files only, in a folder or search results.
pub fn tools(ctx: &Ctx) -> Vec<Tool> {
    let t = |cmd, glyph, text| Tool { cmd, glyph, text };
    match ctx.what {
        What::Background => {
            let s = crate::toolbar::state(ctx.place, 0, ctx.clipboard_files);
            if s.paste { vec![t(Cmd::Paste, G_PASTE, "Paste")] } else { Vec::new() }
        }
        What::Items { files, dirs, drives } => {
            let s = crate::toolbar::state(ctx.place, files + dirs + drives, ctx.clipboard_files);
            let share = matches!(ctx.place, Place::Folder | Place::Search) && files > 0 && dirs + drives == 0;
            let mut out = Vec::new();
            if s.cut {
                out.push(t(Cmd::Cut, G_CUT, "Cut"));
            }
            if s.copy {
                out.push(t(Cmd::Copy, G_COPY, "Copy"));
            }
            if s.rename {
                out.push(t(Cmd::Rename, G_RENAME, "Rename"));
            }
            if share {
                out.push(t(Cmd::Share, G_SHARE, "Share"));
            }
            if s.delete {
                out.push(t(Cmd::Delete, G_DELETE, "Delete"));
            }
            out
        }
    }
}

/// The menu's rows for `ctx`, with the worker's packaged `commands` (empty until it answers) after
/// TwinFM's own items. Groups are separated, never by two separators or one at either end.
pub fn items(ctx: &Ctx, commands: &[Command]) -> Vec<Entry> {
    let mut groups: Vec<Vec<Entry>> = Vec::new();
    let packaged: Vec<Entry> = commands
        .iter()
        .enumerate()
        .map(|(i, c)| Entry {
            cmd: Some(Cmd::Packaged(i)),
            text: c.title.clone(),
            glyph: "",
            icon: c.icon.clone(),
            key: "",
            sub: !c.subs.is_empty(),
            enabled: c.enabled,
        })
        .collect();
    let more = vec![Entry::item(Cmd::More, "Show more options", G_MORE, "Shift+F10")];
    match ctx.what {
        What::Background => {
            let mut own = Vec::new();
            let mut last = Vec::new();
            if ctx.place == Place::Folder {
                own.push(Entry::item(Cmd::OpenOther, "Open in other pane", G_OTHER_PANE, ""));
                own.push(Entry::item(Cmd::Terminal, "Open in terminal", G_TERMINAL, ""));
            }
            if matches!(ctx.place, Place::Folder | Place::Zip) {
                last.push(Entry::item(Cmd::NewFolder, "New folder", G_NEW_FOLDER, "Ctrl+Shift+N"));
                last.push(Entry::item(Cmd::NewFile, "New file", G_NEW_FILE, "Ctrl+Alt+N"));
            }
            if matches!(ctx.place, Place::Folder | Place::Archive | Place::Zip) {
                last.push(Entry::item(Cmd::CopyPath, "Copy path", G_COPY_PATH, ""));
            }
            if matches!(ctx.place, Place::Folder | Place::ThisPc) {
                last.push(Entry::item(Cmd::Properties, "Properties", G_PROPERTIES, "Alt+Enter"));
            }
            groups.push(own);
            groups.push(packaged.into_iter().chain(last).collect());
        }
        What::Items { files, dirs, drives } => {
            let n = files + dirs + drives;
            let archive = matches!(ctx.place, Place::Archive | Place::Zip);
            let mut open = Vec::new();
            // Open for any selection, as Return (P8-L1; one item until then).
            open.push(Entry::item(Cmd::Open, "Open", G_OPEN, "Enter"));
            if files == n && ctx.same_type && !archive && ctx.place != Place::ThisPc {
                open.push(Entry::item(Cmd::OpenWith, "Open with", G_OPEN_WITH, ""));
            }
            if n == 1 && files == 0 {
                open.push(Entry::item(Cmd::OpenOther, "Open in other pane", G_OTHER_PANE, ""));
            }
            if !archive {
                open.push(Entry::item(Cmd::Terminal, "Open in terminal", G_TERMINAL, ""));
            }
            let mut last = packaged;
            if matches!(ctx.place, Place::Folder | Place::Search) && drives == 0 {
                last.push(Entry::item(Cmd::Compress, "Compress to", G_ZIP, ""));
            }
            last.push(Entry::item(Cmd::CopyPath, "Copy path", G_COPY_PATH, ""));
            if !archive {
                last.push(Entry::item(Cmd::Properties, "Properties", G_PROPERTIES, "Alt+Enter"));
            }
            groups.push(open);
            groups.push(last);
        }
    }
    groups.push(more);
    let mut out = Vec::new();
    for g in groups.into_iter().filter(|g| !g.is_empty()) {
        if !out.is_empty() {
            out.push(Entry::sep());
        }
        out.extend(g);
    }
    out
}

/// A submenu's rows: Open with's apps then "Choose another app" (one file only: Windows' chooser
/// takes one; `several` files get the apps alone, or a disabled row when there are none),
/// Compress to's formats, or a
/// packaged command's own commands.
pub fn sub_items(cmd: Cmd, commands: &[Command], apps: &[crate::explorercmd::App], several: bool) -> Vec<Entry> {
    match cmd {
        Cmd::OpenWith => {
            let mut out: Vec<Entry> = apps
                .iter()
                .enumerate()
                .map(|(k, a)| Entry { cmd: Some(Cmd::App(k)), text: a.name.clone(), glyph: "", icon: a.icon.clone(), key: "", sub: false, enabled: true })
                .collect();
            if several {
                if out.is_empty() {
                    out.push(Entry { enabled: false, ..Entry::item(Cmd::ChooseApp, "No apps for this type", "", "") });
                }
                return out;
            }
            if !out.is_empty() {
                out.push(Entry::sep());
            }
            out.push(Entry::item(Cmd::ChooseApp, "Choose another app", G_OPEN_WITH, ""));
            out
        }
        Cmd::Compress => vec![Entry::item(Cmd::Zip, "ZIP file", G_ZIP, ""), Entry::item(Cmd::SevenZ, "7z file", G_ZIP, ""), Entry::item(Cmd::Tar, "TAR file", G_ZIP, "")],
        Cmd::Packaged(i) => commands.get(i).map_or_else(Vec::new, |c| {
            c.subs
                .iter()
                .enumerate()
                .map(|(j, s)| Entry { cmd: Some(Cmd::Sub(i, j)), text: s.title.clone(), glyph: "", icon: s.icon.clone(), key: "", sub: false, enabled: s.enabled })
                .collect()
        }),
        _ => Vec::new(),
    }
}

/// Each row's top in the list, and the list's height.
pub fn rows_y(items: &[Entry]) -> (Vec<f32>, f32) {
    let mut y = 0.0;
    let ys = items
        .iter()
        .map(|e| {
            let top = y;
            y += if e.is_sep() { SEP_H } else { ITEM_H };
            top
        })
        .collect();
    (ys, y)
}

/// The menu's height: the padding, the icon row and its separator when there is one, the rows.
pub fn menu_height(tools: bool, items: &[Entry]) -> f32 {
    2.0 * PAD + if tools { TOOLS_H + SEP_H } else { 0.0 } + rows_y(items).1
}

/// Where a menu of `w` x `h` opens for a pointer at `(ax, ay)` in a window of `win_w` x `win_h`
/// (all logical px): right of and below the point, leftward or upward when it would pass the
/// window's edge (and pinned to the edge when neither way fits), its height cut to the window
/// with the rows scrolling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub x: f32,
    pub y: f32,
    pub h: f32,
    pub scrolls: bool,
    pub left: bool,
    pub up: bool,
}

fn fit(a: f32, size: f32, limit: f32) -> (f32, bool) {
    if a + size <= limit - MARGIN {
        (a, false)
    } else if a - size >= MARGIN {
        (a - size, true)
    } else {
        ((limit - MARGIN - size).max(MARGIN), true)
    }
}

pub fn place((ax, ay): (f32, f32), w: f32, h: f32, (win_w, win_h): (f32, f32)) -> Placed {
    let room = (win_h - 2.0 * MARGIN).max(ITEM_H);
    let shown = h.min(room);
    let (x, left) = fit(ax, w, win_w);
    let (y, up) = fit(ay, shown, win_h);
    Placed { x, y, h: shown, scrolls: shown < h, left, up }
}

/// Where a submenu of `w` x `h` opens beside its item (whose top is `item_y` in the window): to the
/// right of the menu `m` (of width `menu_w`) when it fits, else to its left; its top level with the
/// item's, moved up to fit.
pub fn place_sub(m: &Placed, menu_w: f32, item_y: f32, w: f32, h: f32, (win_w, win_h): (f32, f32)) -> Placed {
    let room = (win_h - 2.0 * MARGIN).max(ITEM_H);
    let shown = h.min(room);
    let right = m.x + menu_w - 2.0;
    let (x, left) = if right + w <= win_w - MARGIN { (right, false) } else { ((m.x - w + 2.0).max(MARGIN), true) };
    let top = item_y - PAD;
    let (y, up) = if top + shown <= win_h - MARGIN { (top.max(MARGIN), false) } else { ((win_h - MARGIN - shown).max(MARGIN), true) };
    Placed { x, y, h: shown, scrolls: shown < h, left, up }
}

/// The keyboard highlight after Down (`down`) or Up from `hl` (-1: none): the next enabled item
/// that way, wrapping round as Explorer's menu does; -1 when none is enabled.
pub fn next_hl(items: &[Entry], hl: i32, down: bool) -> i32 {
    let n = items.len() as i32;
    if n == 0 {
        return -1;
    }
    let mut i = if hl < 0 { if down { -1 } else { n } } else { hl };
    for _ in 0..n {
        i = if down { (i + 1).rem_euclid(n) } else { (i - 1).rem_euclid(n) };
        if items[i as usize].enabled && !items[i as usize].is_sep() {
            return i;
        }
    }
    -1
}

/// The rows' scroll offset (zero or negative) that keeps the row at `top` of height `h` inside a
/// viewport `view` px tall, moving it as little as possible.
pub fn scroll_into_view(scroll: f32, top: f32, h: f32, view: f32) -> f32 {
    if scroll + top < 0.0 {
        -top
    } else if scroll + top + h > view {
        view - top - h
    } else {
        scroll
    }
}

/// The rows as text for the log: `---` a separator, ` >` after a submenu, `(off)` after a
/// disabled item.
pub fn describe(tools: &[Tool], items: &[Entry]) -> String {
    let t: Vec<&str> = tools.iter().map(|t| t.text).collect();
    let rows: Vec<String> = items
        .iter()
        .map(|e| match () {
            _ if e.is_sep() => "---".to_string(),
            _ => format!("{}{}{}", e.text, if e.sub { " >" } else { "" }, if e.enabled { "" } else { " (off)" }),
        })
        .collect();
    format!("[{}] {}", t.join(" "), rows.join(" | "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(place: Place, what: What) -> Ctx {
        // One file (Open with's case until P8-L1) is of one type; several are not unless a test says so.
        let same_type = matches!(what, What::Items { files: 1, dirs: 0, drives: 0 });
        Ctx { place, what, clipboard_files: false, same_type }
    }

    fn files(n: usize) -> What {
        What::Items { files: n, dirs: 0, drives: 0 }
    }

    fn texts(items: &[Entry]) -> Vec<String> {
        items.iter().map(|e| if e.is_sep() { "---".into() } else { e.text.clone() }).collect()
    }

    fn tool_texts(t: &[Tool]) -> Vec<&str> {
        t.iter().map(|t| t.text).collect()
    }

    fn cmd(title: &str) -> Command {
        Command { title: title.into(), icon: None, enabled: true, subs: Vec::new() }
    }

    #[test]
    fn a_file_gets_explorers_layout() {
        let c = ctx(Place::Folder, files(1));
        assert_eq!(tool_texts(&tools(&c)), ["Cut", "Copy", "Rename", "Share", "Delete"]);
        let apps = [cmd("Edit in Notepad"), cmd("Rename with PowerRename")];
        assert_eq!(
            texts(&items(&c, &apps)),
            ["Open", "Open with", "Open in terminal", "---", "Edit in Notepad", "Rename with PowerRename", "Compress to", "Copy path", "Properties", "---", "Show more options"]
        );
        let it = items(&c, &[]);
        assert_eq!(texts(&it), ["Open", "Open with", "Open in terminal", "---", "Compress to", "Copy path", "Properties", "---", "Show more options"], "before the worker answers");
        assert_eq!(it[0].key, "Enter");
        assert!(it[1].sub && it[4].sub, "Open with and Compress to open submenus");
        assert_eq!(it.iter().find(|e| e.cmd == Some(Cmd::Properties)).unwrap().key, "Alt+Enter");
        assert_eq!(it.last().unwrap().key, "Shift+F10");
    }

    #[test]
    fn open_with_takes_files_of_one_type() {
        assert!(same_type(&[Some(".txt")]));
        assert!(same_type(&[Some("")]), "one file without an extension keeps its Open with");
        assert!(same_type(&[Some(".txt"), Some(".TXT"), Some(".Txt")]), "case is ignored");
        assert!(!same_type(&[Some(".txt"), Some(".png")]));
        assert!(!same_type(&[Some(""), Some("")]), "several without an extension have no type to share");
        assert!(!same_type(&[Some(".txt"), None]), "a folder among them");
        assert!(!same_type(&[None]));
        assert!(!same_type(&[]));
    }

    #[test]
    fn folders_drives_and_many() {
        let dir = ctx(Place::Folder, What::Items { files: 0, dirs: 1, drives: 0 });
        assert_eq!(tool_texts(&tools(&dir)), ["Cut", "Copy", "Rename", "Delete"], "no Share for a folder");
        assert_eq!(
            texts(&items(&dir, &[cmd("Open in Terminal")])),
            ["Open", "Open in other pane", "Open in terminal", "---", "Open in Terminal", "Compress to", "Copy path", "Properties", "---", "Show more options"]
        );
        let many = ctx(Place::Folder, What::Items { files: 2, dirs: 1, drives: 0 });
        assert_eq!(tool_texts(&tools(&many)), ["Cut", "Copy", "Delete"], "Rename takes one entry; Share files only");
        assert_eq!(texts(&items(&many, &[])), ["Open", "Open in terminal", "---", "Compress to", "Copy path", "Properties", "---", "Show more options"], "Open for several (P8-L1)");
        let two_files = ctx(Place::Folder, files(2));
        assert_eq!(tool_texts(&tools(&two_files)), ["Cut", "Copy", "Share", "Delete"]);
        assert_eq!(texts(&items(&two_files, &[]))[..2], ["Open", "Open in terminal"], "two types: no Open with");
        let same = Ctx { same_type: true, ..two_files };
        assert_eq!(texts(&items(&same, &[]))[..3], ["Open", "Open with", "Open in terminal"], "two of one type: Open with");
        let in_zip = Ctx { place: Place::Zip, ..same };
        assert_eq!(texts(&items(&in_zip, &[]))[..2], ["Open", "---"], "no Open with inside an archive");
        let drive = ctx(Place::ThisPc, What::Items { files: 0, dirs: 0, drives: 1 });
        assert!(tools(&drive).is_empty(), "drives are neither cut, copied, renamed nor deleted");
        assert_eq!(texts(&items(&drive, &[])), ["Open", "Open in other pane", "Open in terminal", "---", "Copy path", "Properties", "---", "Show more options"]);
    }

    #[test]
    fn archives_search_and_backgrounds() {
        let zip_file = ctx(Place::Archive, files(1));
        assert!(tools(&zip_file).is_empty(), "nothing is written in an archive");
        assert_eq!(texts(&items(&zip_file, &[])), ["Open", "---", "Copy path", "---", "Show more options"]);
        let zip_dir = ctx(Place::Archive, What::Items { files: 0, dirs: 1, drives: 0 });
        assert_eq!(texts(&items(&zip_dir, &[])), ["Open", "Open in other pane", "---", "Copy path", "---", "Show more options"]);
        let found = ctx(Place::Search, files(1));
        assert_eq!(tool_texts(&tools(&found)), ["Cut", "Copy", "Rename", "Share", "Delete"]);
        assert_eq!(texts(&items(&found, &[]))[..2], ["Open", "Open with"]);
        let bg = Ctx { place: Place::Folder, what: What::Background, clipboard_files: true, same_type: false };
        assert_eq!(tool_texts(&tools(&bg)), ["Paste"]);
        assert!(tools(&ctx(Place::Folder, What::Background)).is_empty(), "nothing to paste");
        assert_eq!(
            texts(&items(&bg, &[cmd("Open in Terminal")])),
            ["Open in other pane", "Open in terminal", "---", "Open in Terminal", "New folder", "New file", "Copy path", "Properties", "---", "Show more options"]
        );
        assert_eq!(texts(&items(&ctx(Place::ThisPc, What::Background), &[])), ["Properties", "---", "Show more options"]);
        assert_eq!(texts(&items(&ctx(Place::Archive, What::Background), &[])), ["Copy path", "---", "Show more options"]);
    }

    #[test]
    fn packaged_commands_keep_their_state_and_submenus() {
        let mut idle = cmd("Edit in IDLE");
        idle.subs = vec![cmd("Edit in IDLE 3.14"), Command { enabled: false, ..cmd("Edit in IDLE 3.13") }];
        let off = Command { enabled: false, ..cmd("Unlock with File Locksmith") };
        let c = ctx(Place::Folder, files(1));
        let it = items(&c, &[idle.clone(), off.clone()]);
        let p0 = it.iter().find(|e| e.cmd == Some(Cmd::Packaged(0))).unwrap();
        assert!(p0.sub && p0.enabled);
        assert!(!it.iter().find(|e| e.cmd == Some(Cmd::Packaged(1))).unwrap().enabled);
        let subs = sub_items(Cmd::Packaged(0), &[idle, off], &[], false);
        assert_eq!(texts(&subs), ["Edit in IDLE 3.14", "Edit in IDLE 3.13"]);
        assert_eq!((subs[0].cmd, subs[1].enabled), (Some(Cmd::Sub(0, 0)), false));
        assert!(sub_items(Cmd::Packaged(5), &[], &[], false).is_empty());
        let apps = [crate::explorercmd::App { name: "Notepad".into(), icon: None }, crate::explorercmd::App { name: "Word".into(), icon: None }];
        assert_eq!(texts(&sub_items(Cmd::OpenWith, &[], &apps, false)), ["Notepad", "Word", "---", "Choose another app"]);
        assert_eq!(texts(&sub_items(Cmd::OpenWith, &[], &[], false)), ["Choose another app"]);
        assert_eq!(sub_items(Cmd::OpenWith, &[], &apps, false)[1].cmd, Some(Cmd::App(1)));
        // Several files (P8-L1): the apps alone; with none, one disabled row.
        assert_eq!(texts(&sub_items(Cmd::OpenWith, &[], &apps, true)), ["Notepad", "Word"]);
        let none = sub_items(Cmd::OpenWith, &[], &[], true);
        assert_eq!((texts(&none), none[0].enabled), (vec!["No apps for this type".to_string()], false), "disabled, not a separator");
        assert_eq!(texts(&sub_items(Cmd::Compress, &[], &[], false)), ["ZIP file", "7z file", "TAR file"]);
    }

    #[test]
    fn ids_round_trip() {
        for c in Cmd::FIXED.into_iter().chain([Cmd::Packaged(0), Cmd::Packaged(12), Cmd::App(0), Cmd::App(15), Cmd::Sub(0, 0), Cmd::Sub(3, 31)]) {
            assert_eq!(Cmd::from_id(c.id()), Some(c), "{c:?}");
        }
        assert_eq!(Cmd::from_id(0), None);
        assert_eq!(Cmd::from_id(21), None);
        assert_eq!(Cmd::Cut.key_command(), Some("cx"));
        assert_eq!(Cmd::Properties.key_command(), Some("properties"), "Alt+Enter's command");
        assert_eq!(Cmd::Share.key_command(), None);
        assert!(Cmd::Share.on_worker() && Cmd::Packaged(0).on_worker() && Cmd::App(1).on_worker());
        assert!(!Cmd::Delete.on_worker() && !Cmd::CopyPath.on_worker() && !Cmd::More.on_worker());
    }

    #[test]
    fn heights_and_rows() {
        let it = items(&ctx(Place::Folder, files(1)), &[]);
        let (ys, h) = rows_y(&it);
        assert_eq!(ys[..4], [0.0, 30.0, 60.0, 90.0]);
        assert_eq!(ys[4], 99.0, "after a 9 px separator");
        assert_eq!(h, 7.0 * ITEM_H + 2.0 * SEP_H);
        assert_eq!(menu_height(true, &it), 8.0 + 44.0 + 9.0 + h);
        assert_eq!(menu_height(false, &it), 8.0 + h);
    }

    #[test]
    fn the_menu_opens_away_from_the_edges() {
        let win = (1000.0, 700.0);
        // Room on both sides: right of and below the pointer.
        assert_eq!(place((100.0, 100.0), 280.0, 300.0, win), Placed { x: 100.0, y: 100.0, h: 300.0, scrolls: false, left: false, up: false });
        // Near the right edge: leftward; near the bottom: upward.
        let p = place((900.0, 600.0), 280.0, 300.0, win);
        assert_eq!((p.x, p.y, p.left, p.up), (620.0, 300.0, true, true));
        // Exactly fitting with the margin stays right and down.
        assert_eq!(place((716.0, 396.0), 280.0, 300.0, win).x, 716.0);
        assert!(!place((716.0, 396.0), 280.0, 300.0, win).up);
        // Neither way fits (a narrow window): pinned inside the far edge, never off the left.
        let narrow = place((150.0, 50.0), 280.0, 300.0, (300.0, 700.0));
        assert_eq!((narrow.x, narrow.left), (16.0, true));
        let tiny = place((10.0, 10.0), 280.0, 300.0, (200.0, 700.0));
        assert_eq!(tiny.x, MARGIN, "wider than the window: at the left margin");
        // Taller than the window: cut to it, scrolling, from the top margin.
        let tall = place((100.0, 200.0), 280.0, 900.0, win);
        assert_eq!((tall.y, tall.h, tall.scrolls), (MARGIN, 692.0, true));
        // Upward when it fits above, else pinned to the bottom margin.
        let mid = place((100.0, 450.0), 280.0, 400.0, win);
        assert_eq!((mid.y, mid.up), (50.0, true));
        let low = place((100.0, 350.0), 280.0, 400.0, win);
        assert_eq!((low.y, low.up), (296.0, true), "neither way fits: inside the bottom margin");
    }

    #[test]
    fn submenus_open_beside_their_item() {
        let win = (1000.0, 700.0);
        let m = place((100.0, 100.0), MENU_W, 300.0, win);
        let s = place_sub(&m, MENU_W, 160.0, SUB_W, 100.0, win);
        assert_eq!((s.x, s.y, s.left, s.up), (378.0, 156.0, false, false), "right of the menu, level with the item");
        // Near the right edge: on the menu's left.
        let r = place((700.0, 100.0), MENU_W, 300.0, win);
        let s = place_sub(&r, MENU_W, 160.0, SUB_W, 100.0, win);
        assert_eq!((s.x, s.left), (700.0 - SUB_W + 2.0, true));
        // Near the bottom: moved up to fit.
        let s = place_sub(&m, MENU_W, 650.0, SUB_W, 100.0, win);
        assert_eq!((s.y, s.up), (596.0, true));
        // A long submenu in a short window scrolls.
        let s = place_sub(&m, MENU_W, 160.0, SUB_W, 2000.0, win);
        assert_eq!((s.y, s.h, s.scrolls), (MARGIN, 692.0, true));
    }

    #[test]
    fn the_highlight_skips_separators_and_disabled_items_and_wraps() {
        let mut it = items(&ctx(Place::Folder, files(1)), &[]);
        // Open, Open with, Open in terminal, ---, Compress to, Copy path, Properties, ---, Show more options
        assert_eq!(next_hl(&it, -1, true), 0, "Down from nothing: the first item");
        assert_eq!(next_hl(&it, -1, false), 8, "Up from nothing: the last");
        assert_eq!(next_hl(&it, 2, true), 4, "over the separator");
        assert_eq!(next_hl(&it, 4, false), 2);
        assert_eq!(next_hl(&it, 8, true), 0, "wraps round");
        assert_eq!(next_hl(&it, 0, false), 8);
        it[4].enabled = false;
        assert_eq!(next_hl(&it, 2, true), 5, "a disabled item is skipped");
        assert_eq!(next_hl(&[], -1, true), -1);
        let none = vec![Entry::sep()];
        assert_eq!(next_hl(&none, -1, true), -1);
    }

    #[test]
    fn scrolling_keeps_the_highlight_in_view() {
        assert_eq!(scroll_into_view(0.0, 60.0, 30.0, 200.0), 0.0, "already in view");
        assert_eq!(scroll_into_view(0.0, 300.0, 30.0, 200.0), -130.0, "below: its bottom at the viewport's");
        assert_eq!(scroll_into_view(-130.0, 30.0, 30.0, 200.0), -30.0, "above: its top at the viewport's");
    }

    #[test]
    fn the_log_line_reads_like_the_menu() {
        let c = ctx(Place::Folder, files(1));
        let mut it = items(&c, &[]);
        it[2].enabled = false;
        assert_eq!(
            describe(&tools(&c), &it),
            "[Cut Copy Rename Share Delete] Open | Open with > | Open in terminal (off) | --- | Compress to > | Copy path | Properties | --- | Show more options"
        );
    }
}
