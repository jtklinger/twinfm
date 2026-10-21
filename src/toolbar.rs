//! The toolbar's buttons (P6-M3): New file, New folder, Cut, Copy, Paste, Rename, Delete. Each
//! calls the command its key calls (`main.rs` `command`), so the toolbar has no behaviour of its
//! own; this module only says which buttons apply, from the facts the status bar already reads
//! (where the pane is, how many entries are selected) and whether the clipboard holds files.

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
    }
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
    fn each_button_calls_its_keys_command() {
        let names = ["newfile", "newfolder", "cut", "copy", "paste", "rename", "delete"];
        let cmds: Vec<_> = names.iter().map(|n| command(n)).collect();
        assert_eq!(cmds, [Some("newfile"), Some("newfolder"), Some("cx"), Some("cc"), Some("cv"), Some("rename"), Some("delete")]);
        assert_eq!(command("sdelete"), None, "Delete recycles; Shift+Delete has no button");
    }
}
