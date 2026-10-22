//! The search box (P10-M2): the pure decisions behind the box at the right end of each pane's path
//! bar. Typing filters the listing live; Return runs the recursive name search for the text (the
//! same search as Ctrl+Shift+F and the box's button); an empty Return hands the list the focus. The
//! box is 200 px wide, 150 px in a narrow pane, so the crumbs keep their room.

/// What Return in the box does (`main.rs` `filter_return`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReturnAction {
    /// Search the tab's folder and everything under it for this pattern, in a new tab.
    Search(String),
    /// Nothing to search for: the list takes the focus.
    FocusList,
}

/// The box's width for a pane `pane_w` px wide: 200 px, or 150 px in a pane narrower than 520 px.
pub fn width(pane_w: f32) -> f32 {
    if pane_w < 520.0 { 150.0 } else { 200.0 }
}

/// Return in the box: the trimmed text is the pattern; blank text means the list takes the focus.
pub fn on_return(text: &str) -> ReturnAction {
    match text.trim() {
        "" => ReturnAction::FocusList,
        t => ReturnAction::Search(t.to_string()),
    }
}

/// The empty box's placeholder.
pub fn placeholder() -> &'static str {
    "Filter, Enter to search"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_box_is_narrower_in_a_narrow_pane() {
        assert_eq!(width(1000.0), 200.0);
        assert_eq!(width(520.0), 200.0);
        assert_eq!(width(519.9), 150.0);
        assert_eq!(width(300.0), 150.0);
    }

    #[test]
    fn return_searches_the_trimmed_text_or_hands_the_list_the_focus() {
        assert_eq!(on_return("big"), ReturnAction::Search("big".into()));
        assert_eq!(on_return("  *.toml  "), ReturnAction::Search("*.toml".into()));
        assert_eq!(on_return(""), ReturnAction::FocusList);
        assert_eq!(on_return("   "), ReturnAction::FocusList);
    }

    #[test]
    fn the_placeholder_names_both_actions() {
        assert_eq!(placeholder(), "Filter, Enter to search");
    }
}
