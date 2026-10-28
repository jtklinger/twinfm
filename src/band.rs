//! The grid's rubber band (P4-M3): which tiles a rectangle touches and what selection the band
//! means, in content coordinates (logical px, y from the top of the first grid row), free of
//! Slint and the window. main.rs runs the gesture and applies the result to the model.
//!
//! Since P13-M2 the band runs in the details list too, as Explorer's details view does (Jeremy,
//! 2026-10-27: "dragging from a row's blank part (past the name, or on Size/Modified/Type) selects;
//! dragging the name or icon moves files"): `press_on` tells a press on a row's icon or name text
//! (a drag of the files) from one on the rest of the row (a band), and the band touches every row
//! its height overlaps, whatever its width (`touched_rows`).

use crate::icons::TileGeom;
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

/// The entries whose tile the band between `a` and `b` overlaps, `cols` tiles of `t` to a grid
/// row and `len` entries. A tile's left and top edges belong to it (half-open), so a band of zero
/// width or height touches nothing; the space right of the last column and below the last entry is
/// empty.
pub fn touched(a: (f32, f32), b: (f32, f32), cols: usize, len: usize, t: TileGeom) -> BTreeSet<usize> {
    let (x, y, w, h) = rect(a, b);
    let mut out = BTreeSet::new();
    if cols == 0 || len == 0 || w <= 0.0 || h <= 0.0 {
        return out;
    }
    // The first column and grid row the band reaches, and the last (exclusive) it reaches into.
    let c0 = (x.max(0.0) / t.w).floor() as usize;
    let c1 = (((x + w) / t.w).ceil().max(0.0) as usize).min(cols);
    let r0 = (y.max(0.0) / t.h).floor() as usize;
    let r1 = (((y + h) / t.h).ceil().max(0.0) as usize).min(len.div_ceil(cols));
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

/// Where the cursor goes as the band between `anchor` and `now` (its moving corner) selects
/// (P6-M2, Explorer): the touched tile under `now`, or, with the pointer outside the touched tiles
/// (past the last column, below the rows, above or left of the content), the touched tile nearest
/// it in the same grid row and column, and in a partial last grid row the last touched entry
/// before it. None when the band touches nothing.
pub fn cursor(anchor: (f32, f32), now: (f32, f32), cols: usize, len: usize, t: TileGeom) -> Option<usize> {
    let touched = touched(anchor, now, cols, len, t);
    let (first, last) = (*touched.first()?, *touched.last()?);
    let cols = cols.max(1);
    let (c_lo, c_hi) = touched.iter().fold((usize::MAX, 0), |(lo, hi), &i| (lo.min(i % cols), hi.max(i % cols)));
    let c = ((now.0.max(0.0) / t.w).floor() as usize).clamp(c_lo, c_hi);
    let r = ((now.1.max(0.0) / t.h).floor() as usize).clamp(first / cols, last / cols);
    touched.range(..=r * cols + c).next_back().copied()
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

/// Where a press in a details row landed (P13-M2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PressOn {
    /// The icon column: a drag moves the files.
    Icon,
    /// The name's text: a drag moves the files.
    Name,
    /// Anywhere else in the row (the name cell past the text, the Type, Size and Modified cells,
    /// the padding): a drag selects with the band.
    Blank,
}

impl PressOn {
    pub fn name(self) -> &'static str {
        match self {
            PressOn::Icon => "icon",
            PressOn::Name => "name",
            PressOn::Blank => "blank",
        }
    }

    /// The press drags the files (or else starts the band).
    pub fn drags(self) -> bool {
        self != PressOn::Blank
    }
}

/// What `TEXT_SLACK` adds to the name's drawn width: a press just past its last letter still takes
/// the name, as Explorer's does.
pub const TEXT_SLACK: f32 = 4.0;

/// Where a press `x` px from a details row's left edge lands, the icon column spanning 6 px to
/// `name_x` (`ListGeom::name_x`), the name's text `text_w` px wide from `name_x` in a name cell
/// `cell_w` px wide (the text ends where the cell elides it).
pub fn press_on(x: f32, name_x: f32, text_w: f32, cell_w: f32) -> PressOn {
    if x >= 6.0 && x < name_x {
        PressOn::Icon
    } else if x >= name_x && x < name_x + (text_w + TEXT_SLACK).min(cell_w) {
        PressOn::Name
    } else {
        PressOn::Blank
    }
}

/// The details rows (`row_h` px tall, `len` of them) a band between `a` and `b` touches: every row
/// its height overlaps, whatever its width, so a drag straight down selects (Explorer). Half-open as
/// `touched`: a band of zero height touches nothing.
pub fn touched_rows(a: (f32, f32), b: (f32, f32), len: usize, row_h: f32) -> BTreeSet<usize> {
    let (_, y, _, h) = rect(a, b);
    if len == 0 || h <= 0.0 || row_h <= 0.0 {
        return BTreeSet::new();
    }
    let r0 = (y.max(0.0) / row_h).floor() as usize;
    let r1 = (((y + h) / row_h).ceil().max(0.0) as usize).min(len);
    (r0..r1).collect()
}

/// Where the cursor goes as a details band between `anchor` and `now` selects: the touched row under
/// `now`, or the touched row nearest it above or below them.
pub fn cursor_rows(anchor: (f32, f32), now: (f32, f32), len: usize, row_h: f32) -> Option<usize> {
    let t = touched_rows(anchor, now, len, row_h);
    let (first, last) = (*t.first()?, *t.last()?);
    Some(((now.1.max(0.0) / row_h).floor() as usize).clamp(first, last))
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
    /// The details row the press landed on (P13-M2): released under the threshold, the press is a
    /// click on that row.
    pub row: Option<usize>,
    /// A band in the details list (P13-M2): its rows' height; it touches rows (`touched_rows`), not
    /// tiles.
    pub rows: Option<f32>,
}

impl Band {
    pub fn press(pane: usize, at: (f32, f32), mode: Mode, before: BTreeSet<usize>) -> Band {
        Band { pane, anchor: at, now: at, mode, before, active: false, row: None, rows: None }
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

    /// The tiles (or details rows) the band touches now.
    pub fn touched(&self, cols: usize, len: usize, t: TileGeom) -> BTreeSet<usize> {
        match self.rows {
            Some(h) => touched_rows(self.anchor, self.now, len, h),
            None => touched(self.anchor, self.now, cols, len, t),
        }
    }

    /// The selection the band means now.
    pub fn selection(&self, cols: usize, len: usize, t: TileGeom) -> BTreeSet<usize> {
        selection(self.mode, &self.before, &self.touched(cols, len, t))
    }

    /// Where the cursor goes now (`cursor`, or `cursor_rows` in the details list).
    pub fn cursor(&self, cols: usize, len: usize, t: TileGeom) -> Option<usize> {
        match self.rows {
            Some(h) => cursor_rows(self.anchor, self.now, len, h),
            None => cursor(self.anchor, self.now, cols, len, t),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn set(v: &[usize]) -> BTreeSet<usize> {
        v.iter().copied().collect()
    }

    const L: TileGeom = TileGeom::LARGE;

    #[test]
    fn band_cursor_follows_the_moving_corner() {
        // Five large tiles to a grid row, twelve entries (rows 0 and 1 full, row 2 holds 10 and 11).
        assert_eq!(cursor((10.0, 10.0), (300.0, 200.0), 5, 12, L), Some(8), "the tile under the corner");
        assert_eq!(cursor((300.0, 200.0), (10.0, 10.0), 5, 12, L), Some(0), "dragged up and left");
        assert_eq!(cursor((10.0, 10.0), (600.0, 200.0), 5, 12, L), Some(9), "right of the last column: the last column");
        assert_eq!(cursor((10.0, 10.0), (400.0, 340.0), 5, 12, L), Some(11), "in the partial row past its last entry");
        assert_eq!(cursor((10.0, 10.0), (300.0, 900.0), 5, 12, L), Some(11), "below the rows");
        assert_eq!(cursor((200.0, 130.0), (-40.0, -40.0), 5, 12, L), Some(0), "above and left of the content");
        assert_eq!(cursor((50.0, 10.0), (50.0, 300.0), 5, 12, L), None, "a zero-width band touches nothing");
        // Medium tiles: 80 x 107 (P8-L1).
        let m = TileGeom::MEDIUM;
        assert_eq!(cursor((5.0, 5.0), (170.0, 120.0), 5, 12, m), Some(7));
        assert_eq!(touched((5.0, 5.0), (170.0, 120.0), 5, 12, m), set(&[0, 1, 2, 5, 6, 7]));
        assert_eq!(cursor((5.0, 5.0), (170.0, 100.0), 5, 12, m), Some(2), "still in the first grid row");
    }

    #[test]
    fn band_touches_the_tiles_it_overlaps() {
        // Five tiles to a grid row, twelve entries: rows 0 and 1 full, row 2 holds 10 and 11.
        assert_eq!(touched((10.0, 10.0), (20.0, 20.0), 5, 12, L), set(&[0]), "inside one tile");
        assert_eq!(touched((90.0, 10.0), (200.0, 20.0), 5, 12, L), set(&[0, 1, 2]), "across three tiles");
        assert_eq!(touched((0.0, 120.0), (100.0, 150.0), 5, 12, L), set(&[0, 1, 5, 6]), "two rows, two columns");
        assert_eq!(touched((100.0, 150.0), (0.0, 120.0), 5, 12, L), set(&[0, 1, 5, 6]), "corners in any order");
        assert_eq!(touched((480.0, 0.0), (600.0, 240.0), 5, 12, L), set(&[]), "right of the last column");
        assert_eq!(touched((0.0, 280.0), (480.0, 400.0), 5, 12, L), set(&[10, 11]), "the partial last row");
        assert_eq!(touched((0.0, 420.0), (480.0, 500.0), 5, 12, L), set(&[]), "below the last row");
        assert_eq!(touched((-50.0, -50.0), (10.0, 10.0), 5, 12, L), set(&[0]), "above and left of the content");
        assert_eq!(touched((96.0, 0.0), (100.0, 10.0), 5, 12, L), set(&[1]), "a tile's left edge belongs to it");
        assert_eq!(touched((50.0, 136.0), (60.0, 146.0), 5, 12, L), set(&[5]), "a grid row's top edge belongs to it");
        assert_eq!(touched((50.0, 10.0), (50.0, 300.0), 5, 12, L), set(&[]), "zero width touches nothing");
        assert_eq!(touched((0.0, 50.0), (480.0, 50.0), 5, 12, L), set(&[]), "zero height touches nothing");
        assert_eq!(touched((0.0, 0.0), (480.0, 360.0), 0, 12, L), set(&[]), "no columns, nothing");
        assert_eq!(touched((0.0, 0.0), (480.0, 360.0), 5, 0, L), set(&[]), "no entries, nothing");
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
    fn a_details_press_drags_on_the_icon_and_the_name_and_bands_elsewhere() {
        // The 16 px icons' column: 6 to 28; a name 100 px wide in a 300 px cell.
        assert_eq!(press_on(3.0, 28.0, 100.0, 300.0), PressOn::Blank, "the row's left padding");
        assert_eq!(press_on(6.0, 28.0, 100.0, 300.0), PressOn::Icon);
        assert_eq!(press_on(27.9, 28.0, 100.0, 300.0), PressOn::Icon);
        assert_eq!(press_on(28.0, 28.0, 100.0, 300.0), PressOn::Name);
        assert_eq!(press_on(131.9, 28.0, 100.0, 300.0), PressOn::Name, "a few px past the last letter");
        assert_eq!(press_on(132.0, 28.0, 100.0, 300.0), PressOn::Blank, "the name cell past the text");
        assert_eq!(press_on(500.0, 28.0, 100.0, 300.0), PressOn::Blank, "the Size and Modified cells");
        // A name the cell elides ends with the cell.
        assert_eq!(press_on(128.0, 28.0, 400.0, 100.0), PressOn::Blank);
        assert_eq!(press_on(120.0, 28.0, 400.0, 100.0), PressOn::Name);
        // Thumbnails: the 32 px column and its gap, 6 to 42.
        assert_eq!(press_on(40.0, 42.0, 100.0, 300.0), PressOn::Icon);
        assert!(PressOn::Icon.drags() && PressOn::Name.drags() && !PressOn::Blank.drags());
        // The band from row 1's blank part down to row 3 touches rows 1 to 3, straight down too.
        assert_eq!(touched_rows((300.0, 30.0), (310.0, 70.0), 10, 22.0), set(&[1, 2, 3]));
        assert_eq!(touched_rows((300.0, 30.0), (300.0, 70.0), 10, 22.0), set(&[1, 2, 3]), "zero width: still the rows");
        assert_eq!(touched_rows((300.0, 30.0), (310.0, 30.0), 10, 22.0), set(&[]), "zero height: none");
        assert_eq!(touched_rows((300.0, 30.0), (310.0, 400.0), 10, 22.0), (1..10).collect(), "below the rows: to the last");
        assert_eq!(touched_rows((300.0, 400.0), (300.0, -50.0), 10, 22.0), (0..10).collect(), "from below the rows up past the top");
        assert_eq!(cursor_rows((300.0, 30.0), (310.0, 70.0), 10, 22.0), Some(3), "the cursor follows the moving corner");
        assert_eq!(cursor_rows((300.0, 400.0), (300.0, 50.0), 10, 22.0), Some(2), "dragged up from below the rows");
        assert_eq!(cursor_rows((300.0, 30.0), (300.0, 900.0), 10, 22.0), Some(9));
        let mut b = Band::press(0, (300.0, 30.0), Mode::Replace, set(&[]));
        b.rows = Some(22.0);
        b.now = (300.0, 70.0);
        assert_eq!(b.touched(1, 10, L), set(&[1, 2, 3]), "a details band touches rows");
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
