//! Tab bookkeeping for one pane: an ordered list with an active index. Pure, so the open, close,
//! next/previous and by-index rules are unit-tested without a window. `main.rs` keeps one
//! `Tabs<Tab>` per pane; the tab content (model, path, scroll, cursor) is its business.

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
