//! The grid's rubber band (P4-M3): which tiles a rectangle touches and what selection the band
//! means, in content coordinates (logical px, y from the top of the first grid row), free of
//! Slint and the window. main.rs runs the gesture and applies the result to the model.

use crate::icons::{TILE_H, TILE_W};
use std::collections::BTreeSet;

/// What the band does to the selection it found at the press: Ctrl toggles the touched tiles,
/// Shift adds them, neither replaces the selection (Explorer; Ctrl wins over Shift, as a click's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Replace,
    Toggle,
    Add,
}

impl Mode {
    pub fn from_keys(ctrl: bool, shift: bool) -> Mode {
        if ctrl {
            Mode::Toggle
        } else if shift {
            Mode::Add
        } else {
            Mode::Replace
        }
    }
}

/// The band's rectangle (x, y, w, h) between corners `a` and `b`, in any order.
pub fn rect(a: (f32, f32), b: (f32, f32)) -> (f32, f32, f32, f32) {
    let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
    let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
    (x0, y0, x1 - x0, y1 - y0)
}

/// The entries whose tile the band between `a` and `b` overlaps, `cols` tiles to a grid row and
/// `len` entries. A tile's left and top edges belong to it (half-open), so a band of zero width or
/// height touches nothing; the space right of the last column and below the last entry is empty.
pub fn touched(a: (f32, f32), b: (f32, f32), cols: usize, len: usize) -> BTreeSet<usize> {
    let (x, y, w, h) = rect(a, b);
    let mut out = BTreeSet::new();
    if cols == 0 || len == 0 || w <= 0.0 || h <= 0.0 {
        return out;
    }
    // The first column and grid row the band reaches, and the last (exclusive) it reaches into.
    let c0 = (x.max(0.0) / TILE_W).floor() as usize;
    let c1 = (((x + w) / TILE_W).ceil().max(0.0) as usize).min(cols);
    let r0 = (y.max(0.0) / TILE_H).floor() as usize;
    let r1 = (((y + h) / TILE_H).ceil().max(0.0) as usize).min(len.div_ceil(cols));
    for r in r0..r1 {
        for c in c0..c1 {
            let i = r * cols + c;
            if i < len {
                out.insert(i);
            }
        }
    }
    out
}

/// The selection the band means: `before` (the selection at the press) combined with the
/// `touched` tiles by `mode`.
pub fn selection(mode: Mode, before: &BTreeSet<usize>, touched: &BTreeSet<usize>) -> BTreeSet<usize> {
    match mode {
        Mode::Replace => touched.clone(),
        Mode::Toggle => before.symmetric_difference(touched).copied().collect(),
        Mode::Add => before.union(touched).copied().collect(),
    }
}

/// The direction a band's pointer at window `y` scrolls a list spanning `top..bottom`: -1 within
/// `dnd::EDGE_PX` of the top or above the list, 1 within it of the bottom or below, else None.
/// Unlike a hovering drag (`dnd::edge_scroll`), the band's pointer is grabbed, so beyond an edge
/// counts as at it (Explorer).
pub fn edge(top: f32, bottom: f32, y: f32) -> Option<i32> {
    if y < top + crate::dnd::EDGE_PX {
        Some(-1)
    } else if y >= bottom - crate::dnd::EDGE_PX {
        Some(1)
    } else {
        None
    }
}

/// A band gesture in one pane, from the press on empty space to the release. Points are content
/// coordinates of that pane's list.
#[derive(Clone, Debug)]
pub struct Band {
    pub pane: usize,
    /// Where the button went down.
    pub anchor: (f32, f32),
    /// Where the pointer is now.
    pub now: (f32, f32),
    pub mode: Mode,
    /// The selection at the press: what Toggle and Add build on, and what Escape restores.
    pub before: BTreeSet<usize>,
    /// The pointer has left the drag threshold since the press: the band shows and selects.
    pub active: bool,
}

impl Band {
    pub fn press(pane: usize, at: (f32, f32), mode: Mode, before: BTreeSet<usize>) -> Band {
        Band { pane, anchor: at, now: at, mode, before, active: false }
    }

    /// The pointer moved to `at` with the button down; `threshold` is the drag threshold (x, y).
    /// True once the band is active (this move or an earlier one).
    pub fn moved(&mut self, at: (f32, f32), threshold: (f32, f32)) -> bool {
        self.now = at;
        if !self.active && ((at.0 - self.anchor.0).abs() > threshold.0 || (at.1 - self.anchor.1).abs() > threshold.1) {
            self.active = true;
        }
        self.active
    }

    /// The tiles the band touches now.
    pub fn touched(&self, cols: usize, len: usize) -> BTreeSet<usize> {
        touched(self.anchor, self.now, cols, len)
    }

    /// The selection the band means now.
    pub fn selection(&self, cols: usize, len: usize) -> BTreeSet<usize> {
        selection(self.mode, &self.before, &self.touched(cols, len))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn set(v: &[usize]) -> BTreeSet<usize> {
        v.iter().copied().collect()
    }

    #[test]
    fn band_touches_the_tiles_it_overlaps() {
        // Five tiles to a grid row, twelve entries: rows 0 and 1 full, row 2 holds 10 and 11.
        assert_eq!(touched((10.0, 10.0), (20.0, 20.0), 5, 12), set(&[0]), "inside one tile");
        assert_eq!(touched((90.0, 10.0), (200.0, 20.0), 5, 12), set(&[0, 1, 2]), "across three tiles");
        assert_eq!(touched((0.0, 100.0), (100.0, 130.0), 5, 12), set(&[0, 1, 5, 6]), "two rows, two columns");
        assert_eq!(touched((100.0, 130.0), (0.0, 100.0), 5, 12), set(&[0, 1, 5, 6]), "corners in any order");
        assert_eq!(touched((480.0, 0.0), (600.0, 240.0), 5, 12), set(&[]), "right of the last column");
        assert_eq!(touched((0.0, 240.0), (480.0, 360.0), 5, 12), set(&[10, 11]), "the partial last row");
        assert_eq!(touched((0.0, 400.0), (480.0, 500.0), 5, 12), set(&[]), "below the last row");
        assert_eq!(touched((-50.0, -50.0), (10.0, 10.0), 5, 12), set(&[0]), "above and left of the content");
        assert_eq!(touched((96.0, 0.0), (100.0, 10.0), 5, 12), set(&[1]), "a tile's left edge belongs to it");
        assert_eq!(touched((50.0, 120.0), (60.0, 130.0), 5, 12), set(&[5]), "a grid row's top edge belongs to it");
        assert_eq!(touched((50.0, 10.0), (50.0, 300.0), 5, 12), set(&[]), "zero width touches nothing");
        assert_eq!(touched((0.0, 50.0), (480.0, 50.0), 5, 12), set(&[]), "zero height touches nothing");
        assert_eq!(touched((0.0, 0.0), (480.0, 360.0), 0, 12), set(&[]), "no columns, nothing");
        assert_eq!(touched((0.0, 0.0), (480.0, 360.0), 5, 0), set(&[]), "no entries, nothing");
    }

    #[test]
    fn band_selection_replaces_toggles_or_adds() {
        let before = set(&[1, 2, 7]);
        let touched = set(&[2, 3]);
        assert_eq!(selection(Mode::Replace, &before, &touched), set(&[2, 3]));
        assert_eq!(selection(Mode::Toggle, &before, &touched), set(&[1, 3, 7]), "Ctrl: the touched tiles flip");
        assert_eq!(selection(Mode::Add, &before, &touched), set(&[1, 2, 3, 7]), "Shift: the touched tiles join");
        assert_eq!(Mode::from_keys(false, false), Mode::Replace);
        assert_eq!(Mode::from_keys(true, true), Mode::Toggle, "Ctrl wins over Shift, as a click's does");
        assert_eq!(Mode::from_keys(false, true), Mode::Add);
    }

    #[test]
    fn band_scrolls_at_and_beyond_the_list_edges() {
        // The list spans y 63..779 in the window; the band's pointer is grabbed, so beyond an
        // edge counts as at it (a hovering drag, dnd::edge_scroll, must be over the list).
        assert_eq!(edge(63.0, 779.0, 300.0), None, "inside, away from the edges");
        assert_eq!(edge(63.0, 779.0, 86.9), Some(-1), "near the top");
        assert_eq!(edge(63.0, 779.0, 87.0), None);
        assert_eq!(edge(63.0, 779.0, 10.0), Some(-1), "above the list: still up");
        assert_eq!(edge(63.0, 779.0, 755.0), Some(1), "near the bottom");
        assert_eq!(edge(63.0, 779.0, 754.9), None);
        assert_eq!(edge(63.0, 779.0, 803.0), Some(1), "below the list: still down");
    }

    #[test]
    fn band_rect_is_normalised() {
        assert_eq!(rect((100.0, 130.0), (0.0, 100.0)), (0.0, 100.0, 100.0, 30.0));
        assert_eq!(rect((5.0, 5.0), (5.0, 5.0)), (5.0, 5.0, 0.0, 0.0));
    }

    #[test]
    fn band_gesture_waits_for_the_threshold_and_restores_on_cancel() {
        let mut b = Band::press(1, (300.0, 50.0), Mode::Replace, set(&[4]));
        assert_eq!(b.pane, 1);
        assert!(!b.moved((302.0, 53.0), (4.0, 4.0)), "under the threshold: no band yet");
        assert!(!b.active);
        assert!(b.moved((310.0, 53.0), (4.0, 4.0)), "past it: the band starts");
        assert!(b.active);
        assert!(b.moved((305.0, 51.0), (4.0, 4.0)), "and stays on while the button is down");
        assert_eq!(b.now, (305.0, 51.0));
        assert_eq!(b.anchor, (300.0, 50.0));
        assert_eq!(b.before, set(&[4]), "the selection at the press, for Escape");
    }
}
