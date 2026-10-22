//! The search box (P10-M2): the pure decision behind the box at the right end of each pane's path
//! bar. Typing filters the listing live; Return runs the recursive name search for the text (the
//! same search as Ctrl+Shift+F and the box's button); an empty Return hands the list the focus. The
//! box's width (200 px, 150 in a pane under 520 px) and its placeholder are the Slint's
//! (pane.slint `filter-w` and the field), with no Rust reader.

/// What Return in the box does (`main.rs` `filter_return`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReturnAction {
    /// Search the tab's folder and everything under it for this pattern, in a new tab.
    Search(String),
    /// Nothing to search for: the list takes the focus.
    FocusList,
}

/// Return in the box: the trimmed text is the pattern; blank text means the list takes the focus.
pub fn on_return(text: &str) -> ReturnAction {
    match text.trim() {
        "" => ReturnAction::FocusList,
        t => ReturnAction::Search(t.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_searches_the_trimmed_text_or_hands_the_list_the_focus() {
        assert_eq!(on_return("big"), ReturnAction::Search("big".into()));
        assert_eq!(on_return("  *.toml  "), ReturnAction::Search("*.toml".into()));
        assert_eq!(on_return(""), ReturnAction::FocusList);
        assert_eq!(on_return("   "), ReturnAction::FocusList);
    }
}
