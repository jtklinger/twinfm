//! The details list's column widths (P9-M1): the Type, Size and Modified columns take their widths
//! from the user, per pane, in logical px; Name takes what they leave. A grip on the boundary at a
//! column's left edge moves that boundary (Explorer's rule): the column narrows by what its left
//! neighbour (Name for the first grip) grows. Pure functions; main.rs applies them to the pane's
//! properties, which every row variant, the header and the rename editor read.

/// A column with a width of its own; the grip at its left edge is named after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Col {
    Type,
    Size,
    Modified,
}

impl Col {
    /// The driver's and the `[cols]` lines' name.
    pub fn name(self) -> &'static str {
        match self {
            Col::Type => "type",
            Col::Size => "size",
            Col::Modified => "modified",
        }
    }

    pub fn from_name(s: &str) -> Option<Col> {
        match s {
            "type" => Some(Col::Type),
            "size" => Some(Col::Size),
            "modified" => Some(Col::Modified),
            _ => None,
        }
    }

    /// The grips' int in pane.slint (0 type, 1 size, 2 modified).
    pub fn from_index(i: i32) -> Option<Col> {
        [Col::Type, Col::Size, Col::Modified].get(usize::try_from(i).ok()?).copied()
    }
}

/// A pane's three widths, logical px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Widths {
    pub ty: f32,
    pub size: f32,
    pub modified: f32,
}

/// Today's fixed columns, the widths a settings file without the keys loads.
pub const DEFAULT: Widths = Widths { ty: 120.0, size: 90.0, modified: 140.0 };
/// No column narrower than this by a drag; Name's cell no narrower than `NAME_FLOOR` by a drag
/// (a pane already narrower keeps what it has).
pub const FLOOR: f32 = 40.0;
pub const NAME_FLOOR: f32 = 120.0;
/// The widest a column may be (a hand-edited file is pulled back to it).
pub const MAX: f32 = 2000.0;
/// What a fit adds to the widest cell: the cells' gap and the header cell's padding.
pub const FIT_PAD: f32 = 12.0;

impl Widths {
    pub fn get(&self, c: Col) -> f32 {
        match c {
            Col::Type => self.ty,
            Col::Size => self.size,
            Col::Modified => self.modified,
        }
    }

    fn set(&mut self, c: Col, w: f32) {
        match c {
            Col::Type => self.ty = w,
            Col::Size => self.size = w,
            Col::Modified => self.modified = w,
        }
    }

    /// Each width pulled into FLOOR..=MAX; a non-finite one takes its default.
    pub fn clamped(self) -> Widths {
        let f = |w: f32, d: f32| if w.is_finite() { w.clamp(FLOOR, MAX) } else { d };
        Widths { ty: f(self.ty, DEFAULT.ty), size: f(self.size, DEFAULT.size), modified: f(self.modified, DEFAULT.modified) }
    }

    /// What the columns shown take from the row (the Type column only when it shows).
    pub fn shown(&self, show_type: bool) -> f32 {
        (if show_type { self.ty } else { 0.0 }) + self.size + self.modified
    }
}

/// The column left of grip `c`, None for Name (the Type column off, Size's grip is Name | Size).
pub fn left_of(c: Col, show_type: bool) -> Option<Col> {
    match c {
        Col::Type => None,
        Col::Size => show_type.then_some(Col::Type),
        Col::Modified => Some(Col::Size),
    }
}

/// Grip `c` moved `dx` px (right is positive) from the widths at the press, `start`, when Name's
/// cell was `name` px wide: `c` narrows by dx and its left neighbour widens by dx, the boundary
/// stopping at the floors (and at MAX).
pub fn drag(start: Widths, c: Col, dx: f32, show_type: bool, name: f32) -> Widths {
    let w = start.get(c);
    // c >= FLOOR and c <= MAX.
    let mut hi = w - FLOOR;
    let mut lo = w - MAX;
    match left_of(c, show_type) {
        Some(l) => {
            let lw = start.get(l);
            lo = lo.max(FLOOR - lw);
            hi = hi.min(MAX - lw);
        }
        // Name may grow freely and shrink to its floor (or not at all when it is below it).
        None => lo = lo.max(NAME_FLOOR.min(name) - name),
    }
    let dx = if lo > hi || !dx.is_finite() { 0.0 } else { dx.clamp(lo, hi) };
    let mut out = start;
    out.set(c, w - dx);
    if let Some(l) = left_of(c, show_type) {
        out.set(l, start.get(l) + dx);
    }
    out
}

/// Column `c` set to `px` by moving its grip (as a drag's end does): `colwidth` and the fit.
pub fn set_width(start: Widths, c: Col, px: f32, show_type: bool, name: f32) -> Widths {
    drag(start, c, start.get(c) - px, show_type, name)
}

/// The width a fit gives a column whose cells on screen (and header label) are at most `widest`
/// px wide.
pub fn fit_width(widest: f32) -> f32 {
    widest + FIT_PAD
}

/// Grip `c`'s centre, in the same coordinates as `right`, the header's right edge (the list's):
/// the header cells end there, Modified rightmost.
pub fn grip_x(right: f32, w: Widths, c: Col) -> f32 {
    match c {
        Col::Modified => right - w.modified,
        Col::Size => right - w.modified - w.size,
        Col::Type => right - w.modified - w.size - w.ty,
    }
}

/// The grips a pane's header has: Name | Type, Type | Size, Size | Modified, or without the Type
/// column Name | Size, Size | Modified.
pub fn grips(show_type: bool) -> &'static [Col] {
    if show_type { &[Col::Type, Col::Size, Col::Modified] } else { &[Col::Size, Col::Modified] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_pulls_widths_into_range() {
        let w = Widths { ty: 5.0, size: 9000.0, modified: f32::NAN }.clamped();
        assert_eq!(w, Widths { ty: FLOOR, size: MAX, modified: DEFAULT.modified });
        assert_eq!(DEFAULT.clamped(), DEFAULT);
        assert_eq!(DEFAULT.shown(true), 350.0);
        assert_eq!(DEFAULT.shown(false), 230.0, "the fixed columns of today, with and without Type");
    }

    #[test]
    fn a_grip_moves_its_boundary_and_stops_at_the_floors() {
        // Size | Modified right by 30: Size grows, Modified gives it up.
        let w = drag(DEFAULT, Col::Modified, 30.0, true, 300.0);
        assert_eq!((w.ty, w.size, w.modified), (120.0, 120.0, 110.0));
        // Too far right: Modified stops at its floor.
        let w = drag(DEFAULT, Col::Modified, 500.0, true, 300.0);
        assert_eq!((w.size, w.modified), (190.0, FLOOR));
        // Too far left: Size stops at its floor.
        let w = drag(DEFAULT, Col::Modified, -500.0, true, 300.0);
        assert_eq!((w.size, w.modified), (FLOOR, 190.0));
        // Name | Type left: Name shrinks to its floor, Type takes it.
        let w = drag(DEFAULT, Col::Type, -500.0, true, 300.0);
        assert_eq!(w.ty, 120.0 + 180.0);
        // Name already below its floor: it may grow but not shrink.
        assert_eq!(drag(DEFAULT, Col::Type, -20.0, true, 100.0), DEFAULT);
        assert_eq!(drag(DEFAULT, Col::Type, 20.0, true, 100.0).ty, 100.0);
        // Without the Type column Size's grip is Name | Size, and Type is untouched.
        let w = drag(DEFAULT, Col::Size, -50.0, false, 300.0);
        assert_eq!((w.ty, w.size), (120.0, 140.0));
        // The boundary moves by what the pointer moved: the grip stays under it.
        let start = DEFAULT;
        let after = drag(start, Col::Size, 25.0, true, 300.0);
        assert_eq!(grip_x(500.0, after, Col::Size) - grip_x(500.0, start, Col::Size), 25.0);
    }

    #[test]
    fn set_width_and_fit_move_the_same_boundary() {
        let w = set_width(DEFAULT, Col::Size, 60.0, true, 300.0);
        assert_eq!((w.ty, w.size), (150.0, 60.0), "Type takes what Size gave up");
        let w = set_width(DEFAULT, Col::Type, fit_width(188.0), true, 300.0);
        assert_eq!(w.ty, 200.0, "Name gives the fit its 80 px");
        // A fit wider than Name can give stops at Name's floor.
        assert_eq!(set_width(DEFAULT, Col::Type, 600.0, true, 300.0).ty, 120.0 + 180.0);
        assert_eq!(set_width(DEFAULT, Col::Modified, 10.0, true, 300.0).modified, FLOOR);
    }

    #[test]
    fn grips_and_names() {
        assert_eq!(grips(true).len(), 3);
        assert_eq!(grips(false), &[Col::Size, Col::Modified]);
        assert_eq!(grip_x(500.0, DEFAULT, Col::Type), 150.0);
        for c in grips(true) {
            assert_eq!(Col::from_name(c.name()), Some(*c));
        }
        assert_eq!(Col::from_index(2), Some(Col::Modified));
        assert_eq!(Col::from_index(3), None);
        assert_eq!(Col::from_index(-1), None);
    }
}
