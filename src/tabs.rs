//! Tab bookkeeping for one pane: an ordered list with an active index. Pure, so the open, close,
//! next/previous and by-index rules are unit-tested without a window. `main.rs` keeps one
//! `Tabs<Tab>` per pane; the tab content (model, path, scroll, cursor) is its business. `History`
//! is one tab's back/forward list, pure for the same reason.

use std::path::PathBuf;

/// Folders a tab keeps to go back to (the oldest is dropped past this).
pub const HISTORY_CAP: usize = 100;

/// Where a tab is: a folder, or a recursive name search (`search`: the pattern) under the folder
/// `path`, its root. A search is a history entry like a folder (a page in a browser).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub path: PathBuf,
    pub search: Option<String>,
}

impl Place {
    pub fn folder(path: impl Into<PathBuf>) -> Place {
        Place { path: path.into(), search: None }
    }

    pub fn search(root: impl Into<PathBuf>, pattern: &str) -> Place {
        Place { path: root.into(), search: Some(pattern.to_string()) }
    }

    /// The same place: the same folder ignoring ASCII case (Windows paths), the same search.
    fn same(&self, other: &Place) -> bool {
        self.path.as_os_str().eq_ignore_ascii_case(other.path.as_os_str()) && self.search == other.search
    }
}

impl std::fmt::Display for Place {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.search {
            Some(p) => write!(f, "search \"{p}\" in {}", self.path.display()),
            None => write!(f, "{}", self.path.display()),
        }
    }
}

/// One tab's back/forward history: the places left behind (newest last) and the ones gone back
/// from (next forward last). Starts empty for every tab; not saved.
#[derive(Debug, Default)]
pub struct History {
    back: Vec<Place>,
    forward: Vec<Place>,
}

impl History {
    /// An ordinary navigation from `from` to `to`: `from` goes on the back stack (the oldest
    /// entry is dropped past `HISTORY_CAP`) and the forward stack is cleared. Nothing changes
    /// when `from` is empty (a tab's first folder) or is `to` itself (the same folder twice in a
    /// row). True when `from` was pushed.
    pub fn push(&mut self, from: &Place, to: &Place) -> bool {
        if from.path.as_os_str().is_empty() || from.same(to) {
            return false;
        }
        if self.back.len() >= HISTORY_CAP {
            self.back.remove(0);
        }
        self.back.push(from.clone());
        self.forward.clear();
        true
    }

    /// Back from `current`: the folder to go to, with `current` put on the forward stack; None
    /// when there is nothing to go back to.
    pub fn back(&mut self, current: &Place) -> Option<Place> {
        let to = self.back.pop()?;
        self.forward.push(current.clone());
        Some(to)
    }

    /// Forward from `current`: the folder to go to, with `current` put on the back stack; None
    /// when there is nothing to go forward to.
    pub fn forward(&mut self, current: &Place) -> Option<Place> {
        let to = self.forward.pop()?;
        self.back.push(current.clone());
        Some(to)
    }

    pub fn back_len(&self) -> usize {
        self.back.len()
    }

    pub fn forward_len(&self) -> usize {
        self.forward.len()
    }
}

#[derive(Debug)]
pub struct Tabs<T> {
    items: Vec<T>,
    active: usize,
}

impl<T> Tabs<T> {
    /// A pane always has at least one tab. (The app builds its panes with `restored`.)
    #[cfg(test)]
    pub fn new(first: T) -> Tabs<T> {
        Tabs { items: vec![first], active: 0 }
    }

    /// Tabs restored from the settings: `first` then `rest`, with tab `active` active (clamped to
    /// the last tab).
    pub fn restored(first: T, rest: Vec<T>, active: usize) -> Tabs<T> {
        let mut items = vec![first];
        items.extend(rest);
        let active = active.min(items.len() - 1);
        Tabs { items, active }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn active(&self) -> &T {
        &self.items[self.active]
    }

    pub fn active_mut(&mut self) -> &mut T {
        &mut self.items[self.active]
    }

    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.items.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.items.iter_mut()
    }

    /// Open `t` right after the active tab and make it active (browser order). Returns its index.
    pub fn open(&mut self, t: T) -> usize {
        let at = self.active + 1;
        self.items.insert(at, t);
        self.active = at;
        at
    }

    /// Close tab `i`. The last tab of a pane is never closed (None). Closing the active tab
    /// activates its right neighbour, or its left one when it was the rightmost; closing a tab
    /// left of the active one keeps the same tab active.
    pub fn close(&mut self, i: usize) -> Option<T> {
        if self.items.len() <= 1 || i >= self.items.len() {
            return None;
        }
        let t = self.items.remove(i);
        if i < self.active || (i == self.active && self.active == self.items.len()) {
            self.active -= 1;
        }
        Some(t)
    }

    /// Activate tab `i`; false when there is no such tab.
    pub fn select(&mut self, i: usize) -> bool {
        if i < self.items.len() {
            self.active = i;
            true
        } else {
            false
        }
    }

    /// Move tab `from` to position `to`, shifting the tabs between; the active tab stays active
    /// wherever it lands. False (and nothing moves) when either index is out of range or they
    /// are equal.
    pub fn move_tab(&mut self, from: usize, to: usize) -> bool {
        let n = self.items.len();
        if from >= n || to >= n || from == to {
            return false;
        }
        let t = self.items.remove(from);
        self.items.insert(to, t);
        let a = self.active;
        self.active = if a == from {
            to
        } else if from < a && to >= a {
            a - 1
        } else if from > a && to <= a {
            a + 1
        } else {
            a
        };
        true
    }

    /// The next tab, wrapping from the last to the first.
    pub fn next(&mut self) {
        self.active = (self.active + 1) % self.items.len();
    }

    /// The previous tab, wrapping from the first to the last.
    pub fn prev(&mut self) {
        self.active = (self.active + self.items.len() - 1) % self.items.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Place {
        Place::folder(s)
    }

    #[test]
    fn history_holds_searches_like_folders() {
        let mut h = History::default();
        let root = r"C:\Temp\root";
        let search = Place::search(root, "hit");
        // A search started from its root (a new search tab), then a folder result opened.
        assert!(h.push(&p(root), &search));
        assert!(h.push(&search, &p(r"C:\Temp\root\sub\hit-folder")));
        // Back re-runs the search (root and pattern), then goes to the root; Forward the other way.
        assert_eq!(h.back(&p(r"C:\Temp\root\sub\hit-folder")), Some(search.clone()));
        assert_eq!(h.back(&search), Some(p(root)));
        assert_eq!(h.forward(&p(root)), Some(search.clone()));
        assert_eq!(h.forward(&search), Some(p(r"C:\Temp\root\sub\hit-folder")));
        // The search and its root are different places; the same search twice in a row is one.
        assert!(!h.push(&Place::search(r"c:\temp\ROOT", "hit"), &search));
        assert!(h.push(&Place::search(root, "hit"), &Place::search(root, "HIT")));
        assert_eq!(search.to_string(), r#"search "hit" in C:\Temp\root"#);
        assert_eq!(p(root).to_string(), root);
    }

    #[test]
    fn history_push_back_forward() {
        let mut h = History::default();
        // A tab's first folder pushes nothing; neither does the same folder twice in a row.
        assert!(!h.push(&p(""), &p(r"C:\")));
        assert!(!h.push(&p(r"C:\Windows"), &p(r"c:\windows")));
        assert!(h.back(&p(r"C:\")).is_none(), "nothing to go back to");
        assert!(h.forward(&p(r"C:\")).is_none(), "nothing to go forward to");
        // C:\ -> Windows -> System32, back twice, forward once.
        assert!(h.push(&p(r"C:\"), &p(r"C:\Windows")));
        assert!(h.push(&p(r"C:\Windows"), &p(r"C:\Windows\System32")));
        assert_eq!((h.back_len(), h.forward_len()), (2, 0));
        assert_eq!(h.back(&p(r"C:\Windows\System32")), Some(p(r"C:\Windows")));
        assert_eq!(h.back(&p(r"C:\Windows")), Some(p(r"C:\")));
        assert_eq!((h.back_len(), h.forward_len()), (0, 2));
        assert!(h.back(&p(r"C:\")).is_none());
        assert_eq!(h.forward(&p(r"C:\")), Some(p(r"C:\Windows")));
        assert_eq!((h.back_len(), h.forward_len()), (1, 1));
        // The same folder again keeps the forward stack; a new one clears it.
        assert!(!h.push(&p(r"C:\Windows"), &p(r"C:\Windows")));
        assert_eq!(h.forward_len(), 1);
        assert!(h.push(&p(r"C:\Windows"), &p(r"C:\Users")));
        assert_eq!((h.back_len(), h.forward_len()), (2, 0));
        assert!(h.forward(&p(r"C:\Users")).is_none(), "forward cleared by the push");
        // A folder reached twice, not in a row, is two entries.
        assert!(h.push(&p(r"C:\Users"), &p(r"C:\")));
        assert_eq!(h.back(&p(r"C:\")), Some(p(r"C:\Users")));
        assert_eq!(h.back(&p(r"C:\Users")), Some(p(r"C:\Windows")));
        assert_eq!(h.back(&p(r"C:\Windows")), Some(p(r"C:\")));
        assert!(h.back(&p(r"C:\")).is_none());
    }

    #[test]
    fn history_drops_the_oldest_past_the_cap() {
        let mut h = History::default();
        for i in 0..HISTORY_CAP + 5 {
            assert!(h.push(&p(&format!(r"C:\d{i}")), &p(&format!(r"C:\d{}", i + 1))));
        }
        assert_eq!(h.back_len(), HISTORY_CAP);
        let mut last = None;
        while let Some(to) = h.back(&p("x")) {
            last = Some(to);
        }
        assert_eq!(last, Some(p(r"C:\d5")), "the five oldest were dropped");
        assert_eq!(h.forward_len(), HISTORY_CAP);
    }

    fn order(t: &Tabs<char>) -> String {
        t.iter().collect()
    }

    #[test]
    fn open_close_and_cycle() {
        let mut t = Tabs::new('a');
        assert!(t.close(0).is_none(), "the last tab never closes");
        assert_eq!((t.len(), *t.active()), (1, 'a'));
        assert_eq!(t.open('b'), 1);
        assert_eq!(t.open('c'), 2);
        assert_eq!((order(&t), *t.active()), ("abc".into(), 'c'));
        // A new tab goes right after the active one.
        t.select(0);
        assert_eq!(t.open('d'), 1);
        assert_eq!(order(&t), "adbc");
        // next and prev wrap.
        t.select(3);
        t.next();
        assert_eq!(*t.active(), 'a');
        t.prev();
        assert_eq!(*t.active(), 'c');
        assert!(!t.select(4), "no fifth tab");
        assert_eq!(*t.active(), 'c');
        let mut one = Tabs::new('x');
        one.next();
        one.prev();
        assert_eq!(*one.active(), 'x');
    }

    #[test]
    fn restored_clamps_the_active_index() {
        let t = Tabs::restored('a', vec!['b', 'c'], 1);
        assert_eq!((order(&t), *t.active()), ("abc".into(), 'b'));
        let t = Tabs::restored('a', vec!['b'], 9);
        assert_eq!(*t.active(), 'b', "clamped to the last tab");
        let t = Tabs::restored('a', Vec::new(), 0);
        assert_eq!((t.len(), *t.active()), (1, 'a'));
    }

    #[test]
    fn move_tab_both_ways_and_the_active_follows() {
        let mut t = Tabs::restored('a', vec!['b', 'c', 'd'], 1);
        // The active tab moving right, then left.
        assert!(t.move_tab(1, 2));
        assert_eq!((order(&t), *t.active(), t.active_index()), ("acbd".into(), 'b', 2));
        assert!(t.move_tab(2, 0));
        assert_eq!((order(&t), *t.active(), t.active_index()), ("bacd".into(), 'b', 0));
        // To the last place and back to the first (the edges).
        assert!(t.move_tab(0, 3));
        assert_eq!((order(&t), t.active_index()), ("acdb".into(), 3));
        assert!(t.move_tab(3, 0));
        assert_eq!((order(&t), t.active_index()), ("bacd".into(), 0));
        // Another tab passing the active one from the right, then from the left.
        t.select(1); // 'a'
        assert!(t.move_tab(3, 0));
        assert_eq!((order(&t), *t.active(), t.active_index()), ("dbac".into(), 'a', 2));
        assert!(t.move_tab(0, 3));
        assert_eq!((order(&t), *t.active(), t.active_index()), ("bacd".into(), 'a', 1));
        // Moves that do not cross the active tab leave its index alone.
        assert!(t.move_tab(2, 3));
        assert_eq!((order(&t), t.active_index()), ("badc".into(), 1));
        // Out of range or a no-op: nothing moves.
        assert!(!t.move_tab(0, 4));
        assert!(!t.move_tab(4, 0));
        assert!(!t.move_tab(2, 2));
        assert_eq!((order(&t), t.active_index()), ("badc".into(), 1));
        let mut one = Tabs::new('x');
        assert!(!one.move_tab(0, 0));
    }

    #[test]
    fn closing_moves_to_a_neighbour() {
        let mut t = Tabs::new('a');
        t.open('b');
        t.open('c');
        t.open('d');
        // Active is 'd' (rightmost): closing it moves left.
        assert_eq!(t.close(3), Some('d'));
        assert_eq!(*t.active(), 'c');
        // Closing the active middle tab moves right.
        t.select(1);
        assert_eq!(t.close(1), Some('b'));
        assert_eq!((order(&t), *t.active()), ("ac".into(), 'c'));
        // Closing a tab left of the active one keeps the active tab.
        t.open('e');
        assert_eq!((order(&t), *t.active()), ("ace".into(), 'e'));
        assert_eq!(t.close(0), Some('a'));
        assert_eq!((order(&t), *t.active(), t.active_index()), ("ce".into(), 'e', 1));
        // Closing a tab right of the active one keeps it too.
        t.select(0);
        assert_eq!(t.close(1), Some('e'));
        assert_eq!((order(&t), *t.active()), ("c".into(), 'c'));
        assert!(t.close(0).is_none());
        assert!(t.close(7).is_none());
    }
}
