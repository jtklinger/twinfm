//! The toolbar's buttons (P6-M3): New file, New folder, Cut, Copy, Paste, Rename, Delete. Each
//! calls the command its key calls (`main.rs` `command`), so the toolbar has no behaviour of its
//! own; this module only says which buttons apply, from the facts the status bar already reads
//! (where the pane is, how many entries are selected) and whether the clipboard holds files.
//!
//! Since P13-M2 (Jeremy's layout B) also: the path bar's Back, Forward, Up and Refresh, the
//! toolbar's Undo, Redo, View menu, Two panes and More menu, which of them apply (`with_facts`), the
//! two menus' items (`view_menu`, `more_menu`), and how many buttons a bar has room for before the
//! rest go into More (`fit`).

use crate::ToolState;

/// What a pane's active tab shows, as far as the toolbar is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// A folder: every command applies.
    Folder,
    /// Search results: the results can be cut, copied, renamed and deleted; nothing is made or
    /// pasted into them (the commands refuse with a note).
    Search,
    /// This PC lists drives, which are neither copied, renamed nor deleted, and hold nothing new.
    ThisPc,
    /// A level of an archive TwinFM does not write (every format but zip, P7-M3; an archive inside
    /// an archive, P8-L3): nothing is made, pasted, cut, renamed or deleted there. Copy applies
    /// since P8-L3 (Jeremy's console check): TwinFM pastes it as an extraction, or into a zip; a
    /// path inside an archive is still no file the clipboard can carry to another program.
    Archive,
    /// A level of a zip archive, which TwinFM writes itself (P7-M3): every command applies.
    Zip,
}

/// Which buttons apply: the New buttons and Paste where something can be made (Paste only with
/// files on the clipboard), Cut, Copy and Delete with a selection, Rename with exactly one entry
/// selected.
pub fn state(place: Place, selected: usize, clipboard_files: bool) -> ToolState {
    let (make, take) = match place {
        Place::Folder | Place::Zip => (true, true),
        Place::Search => (false, true),
        Place::Archive => (false, true),
        Place::ThisPc => (false, false),
    };
    let any = take && selected > 0;
    let writes = place != Place::Archive;
    ToolState {
        new: make,
        cut: any && writes,
        copy: any,
        paste: make && clipboard_files,
        rename: take && writes && selected == 1,
        delete: any && writes,
        ..Default::default()
    }
}

/// What the path bar's buttons and the toolbar's newer ones read besides `state`'s facts (P13-M2):
/// the tab's history, whether it shows This PC, the undo and redo stacks' newest records, whether a
/// file job runs and how many wait behind it, the pane's hidden files and view.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    pub back: bool,
    pub forward: bool,
    pub this_pc: bool,
    /// The newest record's status text on each stack, None when it is empty.
    pub undo: Option<String>,
    pub redo: Option<String>,
    /// A file job runs (Ctrl+Z and Ctrl+Y refuse while one does).
    pub busy: bool,
    /// Jobs queued behind the running one (Escape cancels them).
    pub waiting: usize,
    pub hidden: bool,
    /// The pane's view: 0 details, 1 large icons, 2 medium icons (`settings::View::index`).
    pub view: i32,
}

/// `t` with the path bar's and the newer toolbar buttons' state: Back and Forward with somewhere to
/// go, Up anywhere but This PC (Refresh is never disabled), Undo and Redo with a record and no job
/// running, their tooltips' record text, Cancel with jobs waiting.
pub fn with_facts(t: ToolState, f: &Facts) -> ToolState {
    ToolState {
        back: f.back,
        forward: f.forward,
        up: !f.this_pc,
        undo: f.undo.is_some() && !f.busy,
        redo: f.redo.is_some() && !f.busy,
        undo_tip: f.undo.clone().unwrap_or_default().into(),
        redo_tip: f.redo.clone().unwrap_or_default().into(),
        cancel: f.waiting > 0,
        hidden: f.hidden,
        view: f.view,
        ..t
    }
}

/// One item of the View or More menu: its text, the key at its right, the command it runs
/// (`main.rs` `run_tool`), whether it applies, and whether it shows ticked.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem {
    pub text: &'static str,
    pub key: &'static str,
    pub command: &'static str,
    pub enabled: bool,
    pub ticked: bool,
}

fn item(text: &'static str, key: &'static str, command: &'static str, enabled: bool, ticked: bool) -> MenuItem {
    MenuItem { text, key, command, enabled, ticked }
}

/// The View menu: Details, Large icons, Medium icons, the pane's own ticked (Ctrl+Shift+1/2/3).
pub fn view_menu(view: i32) -> Vec<MenuItem> {
    vec![
        item("Details", "Ctrl+Shift+1", "view:details", true, view == 0),
        item("Large icons", "Ctrl+Shift+2", "view:large", true, view == 1),
        item("Medium icons", "Ctrl+Shift+3", "view:medium", true, view == 2),
    ]
}

/// The toolbar's buttons that can overflow into More, in the bar's order, by the name `tool(name)`
/// reports; a separator stands before each in `SEP_BEFORE`. More itself never overflows.
pub const BUTTONS: [&str; 12] = ["newfile", "newfolder", "cut", "copy", "paste", "rename", "delete", "undo", "redo", "view", "split", "preview"];
pub const SEP_BEFORE: [&str; 4] = ["cut", "rename", "undo", "view"];

/// The More menu for a pane whose bar shows `fit` of `BUTTONS`: first each button that did not fit,
/// in the bar's order (the View button as the view menu's three items), then More's own items:
/// Select all, Copy and Move to the other pane (with two panes), Size all folders (in a folder), Show
/// hidden files (ticked while on), Cancel waiting jobs (with jobs waiting).
pub fn more_menu(t: &ToolState, fit: usize, split: bool, place: Place) -> Vec<MenuItem> {
    let mut out = Vec::new();
    for b in BUTTONS.iter().skip(fit) {
        match *b {
            "newfile" => out.push(item("New file", "Ctrl+Alt+N", "newfile", t.new, false)),
            "newfolder" => out.push(item("New folder", "Ctrl+Shift+N", "newfolder", t.new, false)),
            "cut" => out.push(item("Cut", "Ctrl+X", "cut", t.cut, false)),
            "copy" => out.push(item("Copy", "Ctrl+C", "copy", t.copy, false)),
            "paste" => out.push(item("Paste", "Ctrl+V", "paste", t.paste, false)),
            "rename" => out.push(item("Rename", "F2", "rename", t.rename, false)),
            "delete" => out.push(item("Delete", "Delete", "delete", t.delete, false)),
            "undo" => out.push(item("Undo", "Ctrl+Z", "undo", t.undo, false)),
            "redo" => out.push(item("Redo", "Ctrl+Y", "redo", t.redo, false)),
            "view" => out.extend(view_menu(t.view)),
            "split" => out.push(item("Two panes", "F3", "split", true, split)),
            "preview" => out.push(item("Preview pane", "Alt+P", "preview", true, false)),
            _ => {}
        }
    }
    let folder = matches!(place, Place::Folder | Place::Zip);
    out.extend([
        item("Select all", "Ctrl+A", "selectall", true, false),
        item("Copy to other pane", "Shift+F5", "copyother", split && t.copy, false),
        item("Move to other pane", "Shift+F6", "moveother", split && t.cut, false),
        item("Size all folders", "Alt+Shift+Enter", "sizeall", folder, false),
        item("Show hidden files", "Ctrl+H", "hidden", true, t.hidden),
        item("Cancel waiting jobs", "Esc", "cancelwaiting", t.cancel, false),
    ]);
    out
}

/// How many of `widths` (the bar's buttons in order, px) a bar `avail` px wide shows before the
/// rest go into More: with `spacing` between buttons, `sep_w` for each separator that stands before a
/// button shown (`seps[i]`: one stands before button i), `pad` at each end, and `more_w` for the More
/// button, which always shows. Every button when they all fit with More.
pub fn fit(widths: &[f32], seps: &[bool], avail: f32, more_w: f32, spacing: f32, sep_w: f32, pad: f32) -> usize {
    let mut used = 2.0 * pad + more_w;
    for (i, w) in widths.iter().enumerate() {
        used += w + spacing + if seps.get(i).copied().unwrap_or(false) && i > 0 { sep_w + spacing } else { 0.0 };
        if used > avail {
            return i;
        }
    }
    widths.len()
}

/// The command a button calls (its key's, `pane.slint` `FileListHost.key`), or None for a name
/// that is no button.
pub fn command(button: &str) -> Option<&'static str> {
    Some(match button {
        "newfile" => "newfile",
        "newfolder" => "newfolder",
        "cut" => "cx",
        "copy" => "cc",
        "paste" => "cv",
        "rename" => "rename",
        "delete" => "delete",
        "undo" => "undo",
        "redo" => "redo",
        _ => return None,
    })
}

/// Whether the clipboard holds files (`CF_HDROP`), what a paste takes. No clipboard access is
/// needed for the question.
pub fn clipboard_has_files() -> bool {
    use windows_sys::Win32::System::DataExchange::IsClipboardFormatAvailable;
    use windows_sys::Win32::System::Ole::CF_HDROP;
    // SAFETY: a plain query; it neither opens nor changes the clipboard.
    unsafe { IsClipboardFormatAvailable(CF_HDROP as u32) != 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(t: ToolState) -> [bool; 6] {
        [t.new, t.cut, t.copy, t.paste, t.rename, t.delete]
    }

    #[test]
    fn buttons_apply_where_their_command_would() {
        // A folder: New always, Paste with files on the clipboard, the rest by the selection.
        assert_eq!(on(state(Place::Folder, 0, false)), [true, false, false, false, false, false]);
        assert_eq!(on(state(Place::Folder, 0, true)), [true, false, false, true, false, false]);
        assert_eq!(on(state(Place::Folder, 1, false)), [true, true, true, false, true, true]);
        assert_eq!(on(state(Place::Folder, 3, true)), [true, true, true, true, false, true], "Rename takes one entry");
        // Search results: no New, no Paste; the results themselves can be acted on.
        assert_eq!(on(state(Place::Search, 1, true)), [false, true, true, false, true, true]);
        assert_eq!(on(state(Place::Search, 0, true)), [false; 6]);
        // This PC: nothing. A read-only archive level: Copy alone, with a selection (P8-L3).
        assert_eq!(on(state(Place::ThisPc, 1, true)), [false; 6]);
        assert_eq!(on(state(Place::Archive, 2, true)), [false, false, true, false, false, false]);
        assert_eq!(on(state(Place::Archive, 1, true)), [false, false, true, false, false, false], "no Rename");
        assert_eq!(on(state(Place::Archive, 0, true)), [false; 6]);
        assert_eq!(on(state(Place::Zip, 1, true)), [true; 6], "a zip is written as a folder is");
    }

    #[test]
    fn the_newer_buttons_follow_the_history_the_stacks_and_the_queue() {
        let t = state(Place::Folder, 0, false);
        let f = Facts { back: true, undo: Some("Copied 3 items".into()), waiting: 2, view: 1, ..Facts::default() };
        let s = with_facts(t.clone(), &f);
        assert!(s.back && !s.forward && s.up, "Back with somewhere to go, Up out of a folder");
        assert!(s.undo && !s.redo, "Undo with a record, Redo with none");
        assert_eq!(s.undo_tip, "Copied 3 items");
        assert!(s.cancel, "jobs waiting: Cancel applies");
        assert!(s.new, "the older buttons kept");
        let busy = with_facts(t.clone(), &Facts { busy: true, ..f.clone() });
        assert!(!busy.undo, "no Undo while a job runs");
        let pc = with_facts(state(Place::ThisPc, 0, false), &Facts { this_pc: true, ..Facts::default() });
        assert!(!pc.up && !pc.back && !pc.cancel, "This PC has no parent");
    }

    #[test]
    fn the_menus_tick_and_disable_as_their_keys_would() {
        let names = |v: &[MenuItem]| v.iter().map(|i| (i.text, i.enabled, i.ticked)).collect::<Vec<_>>();
        assert_eq!(names(&view_menu(2)), [("Details", true, false), ("Large icons", true, false), ("Medium icons", true, true)]);
        let mut t = with_facts(state(Place::Folder, 1, false), &Facts { hidden: true, ..Facts::default() });
        let all = more_menu(&t, BUTTONS.len(), true, Place::Folder);
        assert_eq!(
            names(&all),
            [
                ("Select all", true, false),
                ("Copy to other pane", true, false),
                ("Move to other pane", true, false),
                ("Size all folders", true, false),
                ("Show hidden files", true, true),
                ("Cancel waiting jobs", false, false)
            ]
        );
        let one = more_menu(&t, BUTTONS.len(), false, Place::Search);
        assert!(!one[1].enabled && !one[2].enabled, "one pane: nothing to copy to");
        assert!(!one[3].enabled, "no folder sizes in search results");
        // Three buttons past the bar's edge come first, in the bar's order, View as its three items.
        t.view = 0;
        let over = more_menu(&t, BUTTONS.len() - 3, true, Place::Folder);
        let texts: Vec<_> = over.iter().take(5).map(|i| i.text).collect();
        assert_eq!(texts, ["Details", "Large icons", "Medium icons", "Two panes", "Preview pane"]);
        assert!(over[3].ticked, "Two panes ticked while split");
        assert_eq!(over.len(), 5 + 6);
    }

    #[test]
    fn a_narrow_bar_overflows_from_its_end() {
        // Four 30 px buttons, a separator before the third, 2 px spacing, 9 px separators, 2 px pads,
        // a 40 px More: all four take 4 + 40 + 4 * 32 + 11 = 183.
        let w = [30.0; 4];
        let seps = [false, false, true, false];
        assert_eq!(fit(&w, &seps, 183.0, 40.0, 2.0, 9.0, 2.0), 4);
        assert_eq!(fit(&w, &seps, 182.9, 40.0, 2.0, 9.0, 2.0), 3, "the last goes first");
        assert_eq!(fit(&w, &seps, 151.0, 40.0, 2.0, 9.0, 2.0), 3);
        assert_eq!(fit(&w, &seps, 150.9, 40.0, 2.0, 9.0, 2.0), 2, "the third and its separator");
        assert_eq!(fit(&w, &seps, 44.0, 40.0, 2.0, 9.0, 2.0), 0, "More alone");
        assert_eq!(fit(&w, &seps, 0.0, 40.0, 2.0, 9.0, 2.0), 0, "More never overflows");
        assert_eq!(BUTTONS.len(), 12);
        assert!(SEP_BEFORE.iter().all(|s| BUTTONS.contains(s)));
    }

    #[test]
    fn each_button_calls_its_keys_command() {
        let names = ["newfile", "newfolder", "cut", "copy", "paste", "rename", "delete", "undo", "redo"];
        let cmds: Vec<_> = names.iter().map(|n| command(n)).collect();
        assert_eq!(cmds, [Some("newfile"), Some("newfolder"), Some("cx"), Some("cc"), Some("cv"), Some("rename"), Some("delete"), Some("undo"), Some("redo")]);
        assert_eq!(command("sdelete"), None, "Delete recycles; Shift+Delete has no button");
    }
}
