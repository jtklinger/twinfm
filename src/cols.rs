//! The details list's column widths (P9-M1): the Type, Size and Modified columns take their widths
//! from the user, per pane, in logical px; Name takes what they leave. A grip on the boundary at a
//! column's left edge moves that boundary (Explorer's rule): the column narrows by what its left
//! neighbour (Name for the first grip) grows. Pure functions; main.rs applies them to the pane's
//! properties, which every row variant, the header and the rename editor read.
//!
//! Since P13-M1 a pane too narrow to give Name `NAME_FLOOR` beside the columns hides them, Type
//! first, then Size, then Modified (`shown`); a hidden column keeps its width and comes back at it.

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

    /// A width under its floor back at its default: a file saved before the floors (the left pane
    /// at Size 62 and Modified 61 in the 2026-10-29 console check) opens with its columns readable.
    pub fn raised(self, floors: Widths) -> Widths {
        let f = |w: f32, floor: f32, d: f32| if w < floor { d.max(floor) } else { w };
        Widths { ty: f(self.ty, floors.ty, DEFAULT.ty), size: f(self.size, floors.size, DEFAULT.size), modified: f(self.modified, floors.modified, DEFAULT.modified) }
    }

    /// What the columns shown take from the row (`shown` in `ALL`'s order).
    pub fn taken(&self, shown: [bool; 3]) -> f32 {
        ALL.iter().zip(shown).filter(|(_, s)| *s).map(|(c, _)| self.get(*c)).sum()
    }
}

/// The columns left to right, the order they are hidden in (P13-M1: Type, then Size, then Modified).
pub const ALL: [Col; 3] = [Col::Type, Col::Size, Col::Modified];

/// Which columns a pane shows (P13-M1), in `ALL`'s order: every one the setting allows (`show_type`
/// for Type), less those hidden, Type first, then Size, then Modified, until Name keeps `NAME_FLOOR`
/// of `room` (the row's width less its paddings: what Name and the columns share). A column comes
/// back as the room grows in the reverse order, each when it fits at its set width.
pub fn shown(w: Widths, show_type: bool, room: f32) -> [bool; 3] {
    let mut s = [show_type, true, true];
    for i in 0..ALL.len() {
        if room - w.taken(s) >= NAME_FLOOR {
            break;
        }
        s[i] = false;
    }
    s
}

/// The Size on disk column's width (P13-M3, the setting `size_on_disk`): fixed, at the row's right
/// end after Modified, with no grip and no sort.
pub const ONDISK_W: f32 = 100.0;

/// Whether a pane with the Size on disk column on shows it (P13-M3): it goes first, before Type, so
/// only while every other column the setting allows fits beside it with Name at `NAME_FLOOR`. The
/// others are then `shown(w, show_type, room - ONDISK_W)` when it shows, `shown(w, show_type, room)`
/// when not.
pub fn ondisk_shown(w: Widths, show_type: bool, room: f32) -> bool {
    room - ONDISK_W - w.taken([show_type, true, true]) >= NAME_FLOOR
}

/// `shown` as the `[cols]` line names it: "name, size, modified".
pub fn shown_text(s: [bool; 3]) -> String {
    let mut out = String::from("name");
    for (c, on) in ALL.iter().zip(s) {
        if on {
            out.push_str(", ");
            out.push_str(c.name());
        }
    }
    out
}

/// The column left of grip `c` among those shown, None for Name (Type hidden, Size's grip is
/// Name | Size; Size hidden too, Modified's is Name | Modified).
pub fn left_of(c: Col, shown: [bool; 3]) -> Option<Col> {
    let i = ALL.iter().position(|&a| a == c)?;
    ALL[..i].iter().zip(shown).rev().find(|(_, s)| *s).map(|(c, _)| *c)
}

/// The narrowest each column may be dragged: Type `FLOOR` (its text ends in "…"), Size and
/// Modified their longest text plus `FIT_PAD`, so a size or a date is never cut (the 2026-10-29
/// console check). `measure` gives the widest line of its text in the lists' font.
pub fn floors(measure: impl Fn(&str) -> f32) -> Widths {
    // Every digit in every place, as the font's digits may differ in width.
    let digits = |shape: &str| (0..10).map(|d| shape.replace('d', &d.to_string())).collect::<Vec<_>>().join("\n");
    let size = ["dddd.d KB", "dddd.d MB", "dddd.dd GB", "dddd B"].map(digits).join("\n");
    Widths { ty: FLOOR, size: (measure(&size) + FIT_PAD).max(FLOOR), modified: (measure(&digits("dddd-dd-dd dd:dd")) + FIT_PAD).max(FLOOR) }
}

/// Grip `c` moved `dx` px (right is positive) from the widths at the press, `start`, when Name's
/// cell was `name` px wide: `c` narrows by dx and its left neighbour widens by dx, the boundary
/// stopping at the floors (and at MAX).
pub fn drag(start: Widths, c: Col, dx: f32, shown: [bool; 3], name: f32, floors: Widths) -> Widths {
    let w = start.get(c);
    // c >= its floor and c <= MAX.
    let mut hi = w - floors.get(c);
    let mut lo = w - MAX;
    match left_of(c, shown) {
        Some(l) => {
            let lw = start.get(l);
            lo = lo.max(floors.get(l) - lw);
            hi = hi.min(MAX - lw);
        }
        // Name may grow freely and shrink to its floor (or not at all when it is below it).
        None => lo = lo.max(NAME_FLOOR.min(name) - name),
    }
    let dx = if lo > hi || !dx.is_finite() { 0.0 } else { dx.clamp(lo, hi) };
    let mut out = start;
    out.set(c, w - dx);
    if let Some(l) = left_of(c, shown) {
        out.set(l, start.get(l) + dx);
    }
    out
}

/// Column `c` set to `px` by moving its grip (as a drag's end does): `colwidth` and the fit.
pub fn set_width(start: Widths, c: Col, px: f32, shown: [bool; 3], name: f32, floors: Widths) -> Widths {
    drag(start, c, start.get(c) - px, shown, name, floors)
}

/// The width a fit gives a column whose cells on screen (and header label) are at most `widest`
/// px wide.
pub fn fit_width(widest: f32) -> f32 {
    widest + FIT_PAD
}

/// Grip `c`'s centre, in the same coordinates as `right`, the header's right edge (the list's):
/// the header cells end there, Modified rightmost; a hidden column takes no room.
pub fn grip_x(right: f32, w: Widths, c: Col, shown: [bool; 3]) -> f32 {
    let i = ALL.iter().position(|&a| a == c).unwrap_or(0);
    let mut from = [false; 3];
    from[i..].copy_from_slice(&shown[i..]);
    from[i] = true;
    right - w.taken(from)
}

/// The grips a pane's header has: one at the left edge of each column shown.
pub fn grips(shown: [bool; 3]) -> Vec<Col> {
    ALL.iter().zip(shown).filter(|(_, s)| *s).map(|(c, _)| *c).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL3: [bool; 3] = [true; 3];
    const NO_TYPE: [bool; 3] = [false, true, true];
    /// The one 40 px floor every column had before `floors`.
    const F40: Widths = Widths { ty: FLOOR, size: FLOOR, modified: FLOOR };

    #[test]
    fn clamp_pulls_widths_into_range() {
        let w = Widths { ty: 5.0, size: 9000.0, modified: f32::NAN }.clamped();
        assert_eq!(w, Widths { ty: FLOOR, size: MAX, modified: DEFAULT.modified });
        assert_eq!(DEFAULT.clamped(), DEFAULT);
        assert_eq!(DEFAULT.taken([true; 3]), 350.0);
        assert_eq!(DEFAULT.taken([false, true, true]), 230.0, "the fixed columns of today, with and without Type");
    }

    #[test]
    fn a_grip_moves_its_boundary_and_stops_at_the_floors() {
        // Size | Modified right by 30: Size grows, Modified gives it up.
        let w = drag(DEFAULT, Col::Modified, 30.0, ALL3, 300.0, F40);
        assert_eq!((w.ty, w.size, w.modified), (120.0, 120.0, 110.0));
        // Too far right: Modified stops at its floor.
        let w = drag(DEFAULT, Col::Modified, 500.0, ALL3, 300.0, F40);
        assert_eq!((w.size, w.modified), (190.0, FLOOR));
        // Too far left: Size stops at its floor.
        let w = drag(DEFAULT, Col::Modified, -500.0, ALL3, 300.0, F40);
        assert_eq!((w.size, w.modified), (FLOOR, 190.0));
        // Name | Type left: Name shrinks to its floor, Type takes it.
        let w = drag(DEFAULT, Col::Type, -500.0, ALL3, 300.0, F40);
        assert_eq!(w.ty, 120.0 + 180.0);
        // Name already below its floor: it may grow but not shrink.
        assert_eq!(drag(DEFAULT, Col::Type, -20.0, ALL3, 100.0, F40), DEFAULT);
        assert_eq!(drag(DEFAULT, Col::Type, 20.0, ALL3, 100.0, F40).ty, 100.0);
        // Without the Type column Size's grip is Name | Size, and Type is untouched.
        let w = drag(DEFAULT, Col::Size, -50.0, NO_TYPE, 300.0, F40);
        assert_eq!((w.ty, w.size), (120.0, 140.0));
        // The boundary moves by what the pointer moved: the grip stays under it.
        let start = DEFAULT;
        let after = drag(start, Col::Size, 25.0, ALL3, 300.0, F40);
        assert_eq!(grip_x(500.0, after, Col::Size, ALL3) - grip_x(500.0, start, Col::Size, ALL3), 25.0);
    }

    #[test]
    fn set_width_and_fit_move_the_same_boundary() {
        let w = set_width(DEFAULT, Col::Size, 60.0, ALL3, 300.0, F40);
        assert_eq!((w.ty, w.size), (150.0, 60.0), "Type takes what Size gave up");
        let w = set_width(DEFAULT, Col::Type, fit_width(188.0), ALL3, 300.0, F40);
        assert_eq!(w.ty, 200.0, "Name gives the fit its 80 px");
        // A fit wider than Name can give stops at Name's floor.
        assert_eq!(set_width(DEFAULT, Col::Type, 600.0, ALL3, 300.0, F40).ty, 120.0 + 180.0);
        assert_eq!(set_width(DEFAULT, Col::Modified, 10.0, ALL3, 300.0, F40).modified, FLOOR);
    }

    #[test]
    fn grips_and_names() {
        assert_eq!(grips(ALL3).len(), 3);
        assert_eq!(grips(NO_TYPE), [Col::Size, Col::Modified]);
        assert_eq!(grips([false, false, true]), [Col::Modified]);
        assert_eq!(grip_x(500.0, DEFAULT, Col::Type, ALL3), 150.0);
        // Type hidden: Size's grip where Type's was not, at Size's own left edge.
        assert_eq!(grip_x(500.0, DEFAULT, Col::Size, NO_TYPE), 270.0);
        assert_eq!(grip_x(500.0, DEFAULT, Col::Modified, [false, false, true]), 360.0);
        for c in grips(ALL3) {
            assert_eq!(Col::from_name(c.name()), Some(c));
        }
        assert_eq!(Col::from_index(2), Some(Col::Modified));
        assert_eq!(Col::from_index(3), None);
        assert_eq!(Col::from_index(-1), None);
    }

    #[test]
    fn columns_go_type_size_modified_and_come_back_in_reverse() {
        // DEFAULT: 120 + 90 + 140 = 350; Name keeps 120.
        assert_eq!(shown(DEFAULT, true, 470.0), ALL3, "470 px: all shown, Name at 120");
        assert_eq!(shown(DEFAULT, true, 469.0), NO_TYPE, "a px less: Type goes first");
        assert_eq!(shown(DEFAULT, true, 350.0), NO_TYPE, "230 + 120");
        assert_eq!(shown(DEFAULT, true, 349.0), [false, false, true], "then Size");
        assert_eq!(shown(DEFAULT, true, 260.0), [false, false, true], "140 + 120");
        assert_eq!(shown(DEFAULT, true, 259.0), [false; 3], "then Modified; Name never goes");
        assert_eq!(shown(DEFAULT, true, 0.0), [false; 3]);
        // Widening: each comes back at its own boundary, Modified first.
        let mut last = [false; 3];
        let mut back = Vec::new();
        for room in 0..1000 {
            let s = shown(DEFAULT, true, room as f32);
            for i in 0..3 {
                if s[i] && !last[i] {
                    back.push((ALL[i], room));
                }
                assert!(!(last[i] && !s[i]), "nothing goes while the room grows");
            }
            last = s;
        }
        assert_eq!(back, [(Col::Modified, 260), (Col::Size, 350), (Col::Type, 470)]);
        // A hidden column keeps its width: a wide Type comes back only when it fits.
        let wide = Widths { ty: 300.0, ..DEFAULT };
        assert_eq!(shown(wide, true, 649.0), NO_TYPE);
        assert_eq!(shown(wide, true, 650.0), ALL3);
        // The Type column off: never shown, and the others need no room for it.
        assert_eq!(shown(DEFAULT, false, 1000.0), NO_TYPE);
        assert_eq!(shown(DEFAULT, false, 350.0), NO_TYPE);
        assert_eq!(shown(DEFAULT, false, 349.0), [false, false, true]);
        assert_eq!(shown_text(NO_TYPE), "name, size, modified");
        assert_eq!(shown_text([false; 3]), "name");
    }

    #[test]
    fn a_drag_keeps_its_neighbour_while_a_column_could_come_back() {
        // The soak's case: a 499 px list, 461 px of room, Type hidden, so Size's grip is Name | Size.
        let frozen = shown(DEFAULT, true, 461.0);
        assert_eq!(frozen, NO_TYPE);
        let dragged = drag(DEFAULT, Col::Size, 30.0, frozen, 231.0, F40);
        assert_eq!((dragged.ty, dragged.size), (120.0, 60.0), "Name takes what Size gives up; Type untouched");
        // The narrower Size lets Type back in: the release applies that (main.rs `update_shown` waits
        // while the grip is held), so the drag never switches to Type | Size under the pointer.
        assert_eq!(shown(dragged, true, 461.0), ALL3);
        assert_eq!(drag(DEFAULT, Col::Size, 30.0, ALL3, 231.0, F40).ty, 150.0, "with Type shown the same drag widens Type");
    }

    #[test]
    fn a_grip_beside_a_hidden_column_moves_against_name() {
        // Size hidden: Modified's grip is Name | Modified, so Name gives what Modified takes.
        let only_mod = [false, false, true];
        assert_eq!(left_of(Col::Modified, only_mod), None);
        assert_eq!(left_of(Col::Modified, ALL3), Some(Col::Size));
        assert_eq!(left_of(Col::Size, NO_TYPE), None);
        let w = drag(DEFAULT, Col::Modified, -40.0, only_mod, 300.0, F40);
        assert_eq!((w.size, w.modified), (90.0, 180.0), "Size's own width untouched");
    }

    #[test]
    fn size_and_modified_never_go_under_their_text() {
        // A measure of 5 px a character, 6 for the digit 8: the widest digit sets each floor.
        let measure = |s: &str| s.lines().map(|l| l.chars().map(|c| if c == '8' { 6.0 } else { 5.0 }).sum::<f32>()).fold(0.0, f32::max);
        let f = floors(measure);
        // "8888.88 GB": 6 eights and 4 others; "8888-88-88 88:88": 12 eights and 4 others.
        assert_eq!((f.ty, f.size, f.modified), (FLOOR, 36.0 + 20.0 + FIT_PAD, 72.0 + 20.0 + FIT_PAD));
        // A drag stops at them, and so does the neighbour giving way.
        assert_eq!(drag(DEFAULT, Col::Modified, 500.0, ALL3, 300.0, f).modified, f.modified);
        assert_eq!(drag(DEFAULT, Col::Modified, -500.0, ALL3, 300.0, f).size, f.size);
        assert_eq!(set_width(DEFAULT, Col::Size, 10.0, ALL3, 300.0, f).size, f.size);
        // The console check's saved widths come back at the defaults; wider ones stay.
        let saved = Widths { ty: 103.0, size: 62.2, modified: 60.8 };
        assert_eq!(saved.raised(f), Widths { ty: 103.0, ..DEFAULT });
        assert_eq!(Widths { ty: 50.0, size: 120.0, modified: 200.0 }.raised(f), Widths { ty: 50.0, size: 120.0, modified: 200.0 });
    }

    #[test]
    fn size_on_disk_goes_first_and_comes_back_last() {
        // P13-M3: 100 px after Modified; it shows only while Type, Size and Modified fit beside it.
        let all = DEFAULT.taken(ALL3); // 350
        assert!(ondisk_shown(DEFAULT, true, NAME_FLOOR + all + ONDISK_W));
        assert!(!ondisk_shown(DEFAULT, true, NAME_FLOOR + all + ONDISK_W - 1.0), "it goes before Type");
        assert_eq!(shown(DEFAULT, true, NAME_FLOOR + all + ONDISK_W - 1.0), ALL3, "Type stays while it can");
        // With the Type column off, Type's width is not asked for.
        assert!(ondisk_shown(DEFAULT, false, NAME_FLOOR + DEFAULT.taken(NO_TYPE) + ONDISK_W));
        assert!(!ondisk_shown(DEFAULT, false, 0.0));
    }
}
