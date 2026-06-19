//! Geometry primitives: [`Point`] and [`Rect`].
//!
//! Coordinates are `i32`. Signed is required: view origins go negative when
//! scrolled offscreen, and the translate/inflate operations take negative deltas.
//! Conversion to the unsigned buffer index space happens at the screen boundary.
//!
//! ### Keyword-collision renames
//! Two [`Rect`] methods would collide with Rust keywords, so they use raw
//! identifiers: [`Rect::r#move`] (translate) and [`Rect::r#union`] (bounding box).
//! Call sites read `rect.r#move(1, 2)` / `rect.r#union(&other)`.
//!
//! # Turbo Vision heritage
//! Ports `TPoint`/`TRect` (`objects.h`). The mutating methods return `&mut Self`
//! to mirror the original `TRect&` chaining idiom.

use std::ops::{Add, AddAssign, Sub, SubAssign};

/// A screen coordinate pair `(x, y)`.
///
/// Use `Point` wherever you need a column/row position: view origins, cursor
/// positions, mouse hit coordinates, or scroll offsets. Both fields are `i32`
/// because view origins go negative when a view is scrolled offscreen, and
/// translation deltas are frequently negative. Conversion to an unsigned buffer
/// index happens only at the screen boundary.
///
/// `Point` supports all four arithmetic operators (`+`, `-`, `+=`, `-=`) so
/// you can translate positions and compute offsets without converting to tuples:
///
/// ```
/// # use tvision_rs::Point;
/// let origin = Point::new(5, 3);
/// let offset = Point::new(-2, 1);
/// assert_eq!(origin + offset, Point::new(3, 4));
/// ```
///
/// `Default` returns `Point::new(0, 0)`.
///
/// # Turbo Vision heritage
/// Ports `TPoint` from `objects.h`. Field names are lowercase (`x`, `y`)
/// following Rust conventions; the original used uppercase `X`/`Y`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Point {
    /// Screen column (horizontal position).
    ///
    /// Zero is the leftmost column. Negative values occur when a view's origin
    /// is scrolled to the left of the visible area. Write this field directly
    /// when constructing geometry; use `Point::new` to set both axes at once.
    pub x: i32,
    /// Screen row (vertical position).
    ///
    /// Zero is the topmost row. Negative values occur when a view's origin is
    /// scrolled above the visible area. Write this field directly when
    /// constructing geometry; use `Point::new` to set both axes at once.
    pub y: i32,
}

impl Point {
    /// Construct a point from a column `x` and row `y`.
    ///
    /// This is a `const fn`, so it can be used in `const` and `static`
    /// contexts. For the zero point, `Default::default()` (or
    /// `Point::default()`) is equivalent and more expressive.
    ///
    /// ```
    /// # use tvision_rs::Point;
    /// const ORIGIN: Point = Point::new(0, 0);
    /// assert_eq!(ORIGIN, Point::default());
    /// ```
    pub const fn new(x: i32, y: i32) -> Self {
        Point { x, y }
    }
}

impl Add for Point {
    type Output = Point;
    /// Returns a new `Point` with both axes summed.
    ///
    /// Use this to apply an offset to a position without mutating the original:
    /// `child_origin + scroll_offset`.
    fn add(self, rhs: Point) -> Point {
        Point::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point {
    type Output = Point;
    /// Returns a new `Point` with both axes subtracted.
    ///
    /// Use this to compute the offset between two positions:
    /// `mouse_pos - view_origin` gives the hit position in view-local
    /// coordinates.
    fn sub(self, rhs: Point) -> Point {
        Point::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl AddAssign for Point {
    /// Shifts this point in place by adding `rhs`.
    ///
    /// Use this to accumulate translation steps without allocating a new
    /// `Point`: `cursor += delta`.
    fn add_assign(&mut self, rhs: Point) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl SubAssign for Point {
    /// Shifts this point in place by subtracting `rhs`.
    ///
    /// Use this to undo a previously applied offset in place:
    /// `cursor -= delta`.
    fn sub_assign(&mut self, rhs: Point) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}

/// A rectangle defined by its top-left corner `a` (inclusive) and bottom-right
/// corner `b` (exclusive).
///
/// The half-open convention (`[a, b)`) means `b` is one cell past the last
/// occupied column/row — the same model used by Rust ranges and ratatui.
/// The width is `b.x - a.x`; the height is `b.y - a.y`.
///
/// The mutating methods take `&mut self` and return `&mut Self`, so they chain
/// and operate in place:
///
/// ```
/// # use tvision_rs::{Point, Rect};
/// let mut r = Rect::new(0, 0, 10, 10);
/// r.grow(-1, -1).r#move(1, 1);
/// assert_eq!(r, Rect::new(2, 2, 10, 10));
/// ```
///
/// # Turbo Vision heritage
/// Ports `TRect` from `objects.h`. The two raw-identifier methods
/// [`r#move`](Rect::r#move) and [`r#union`](Rect::r#union) use raw identifiers
/// to avoid collisions with the Rust keywords `move` and `union`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rect {
    /// Top-left corner of the rectangle (inclusive).
    ///
    /// `a` is the first cell that belongs to the rectangle. Both `a.x` and
    /// `a.y` are included in [`contains`](Rect::contains) tests. You may read
    /// or write this field directly; if you set `a > b` the rectangle becomes
    /// empty (see [`is_empty`](Rect::is_empty)) but is otherwise valid.
    pub a: Point,
    /// Bottom-right corner of the rectangle (exclusive).
    ///
    /// `b` is one cell past the last occupied column (`b.x`) and the last
    /// occupied row (`b.y`). The rectangle width is `b.x - a.x`; height is
    /// `b.y - a.y`. You may read or write this field directly.
    pub b: Point,
}

impl Rect {
    /// Construct a rectangle from the four corner coordinates `(ax, ay)`–`(bx, by)`.
    ///
    /// Prefer this constructor when you have individual `x`/`y` values at hand.
    /// Use [`from_points`](Rect::from_points) when you already hold [`Point`]
    /// values. The convention is `a = (ax, ay)` (top-left, inclusive) and
    /// `b = (bx, by)` (bottom-right, exclusive).
    pub const fn new(ax: i32, ay: i32, bx: i32, by: i32) -> Self {
        Rect {
            a: Point::new(ax, ay),
            b: Point::new(bx, by),
        }
    }

    /// Construct a rectangle from two [`Point`] corners.
    ///
    /// Equivalent to [`new`](Rect::new) but more ergonomic when you already
    /// hold [`Point`] values. `p1` becomes [`a`](Rect::a) (top-left, inclusive)
    /// and `p2` becomes [`b`](Rect::b) (bottom-right, exclusive); no sorting is
    /// applied, so passing them in the wrong order yields an empty rectangle.
    pub const fn from_points(p1: Point, p2: Point) -> Self {
        Rect { a: p1, b: p2 }
    }

    /// Translate the rectangle by `(dx, dy)`, shifting both corners equally.
    ///
    /// Use this to reposition a rectangle without changing its size. Negative
    /// deltas move it left/up; positive deltas move it right/down. Because `move`
    /// is a Rust keyword, call this method as `rect.r#move(dx, dy)`.
    pub fn r#move(&mut self, dx: i32, dy: i32) -> &mut Self {
        self.a.x += dx;
        self.a.y += dy;
        self.b.x += dx;
        self.b.y += dy;
        self
    }

    /// Inflate the rectangle symmetrically by `(dx, dy)`.
    ///
    /// Expands each edge outward: `a` moves left/up by `(dx, dy)` and `b` moves
    /// right/down by the same amount, so the total width grows by `2*dx` and
    /// height by `2*dy`. Pass negative values to deflate (shrink) the rectangle;
    /// deflating past zero produces an empty (inverted) rectangle.
    pub fn grow(&mut self, dx: i32, dy: i32) -> &mut Self {
        self.a.x -= dx;
        self.a.y -= dy;
        self.b.x += dx;
        self.b.y += dy;
        self
    }

    /// Clip the rectangle to the intersection with `r`.
    ///
    /// Replaces `self` with the largest rectangle that fits inside both `self`
    /// and `r`. If the two rectangles do not overlap, the result is empty
    /// (`is_empty()` returns `true`). Use this to constrain a child view's
    /// bounds to its parent's visible area.
    pub fn intersect(&mut self, r: &Rect) -> &mut Self {
        self.a.x = self.a.x.max(r.a.x);
        self.a.y = self.a.y.max(r.a.y);
        self.b.x = self.b.x.min(r.b.x);
        self.b.y = self.b.y.min(r.b.y);
        self
    }

    /// Expand the rectangle to the bounding box of `self` and `r`.
    ///
    /// Replaces `self` with the smallest rectangle that contains both `self`
    /// and `r`. Use this to compute a dirty region that covers multiple views.
    /// Because `union` is a Rust keyword, call this method as
    /// `rect.r#union(&other)`.
    pub fn r#union(&mut self, r: &Rect) -> &mut Self {
        self.a.x = self.a.x.min(r.a.x);
        self.a.y = self.a.y.min(r.a.y);
        self.b.x = self.b.x.max(r.b.x);
        self.b.y = self.b.y.max(r.b.y);
        self
    }

    /// Returns `true` if `p` lies inside the rectangle.
    ///
    /// Uses half-open semantics matching the `[a, b)` convention: the
    /// left/top edges are **included** (`p.x >= a.x`, `p.y >= a.y`) but the
    /// right/bottom edges are **excluded** (`p.x < b.x`, `p.y < b.y`).
    /// This is the correct test for hit-testing a character-cell grid, where
    /// a point exactly on the right or bottom edge belongs to the *next* cell.
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.a.x && p.x < self.b.x && p.y >= self.a.y && p.y < self.b.y
    }

    /// Returns `true` if the rectangle has zero or negative area.
    ///
    /// This is the case when `a.x >= b.x` or `a.y >= b.y` — either the
    /// width or height is zero. An inverted rectangle (where `b < a`, e.g.
    /// produced by deflating past zero with [`grow`](Rect::grow)) is also
    /// considered empty.
    pub fn is_empty(&self) -> bool {
        self.a.x >= self.b.x || self.a.y >= self.b.y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_arithmetic() {
        let p = Point::new(3, 4) + Point::new(1, 2);
        assert_eq!(p, Point::new(4, 6));
        assert_eq!(Point::new(3, 4) - Point::new(1, 2), Point::new(2, 2));

        let mut q = Point::new(10, 10);
        q += Point::new(5, -5);
        assert_eq!(q, Point::new(15, 5));
        q -= Point::new(5, 5);
        assert_eq!(q, Point::new(10, 0));
    }

    #[test]
    fn rect_constructors_equivalent() {
        let a = Rect::new(1, 2, 3, 4);
        let b = Rect::from_points(Point::new(1, 2), Point::new(3, 4));
        assert_eq!(a, b);
        assert_eq!(Rect::default(), Rect::new(0, 0, 0, 0));
    }

    #[test]
    fn rect_move_and_grow() {
        let mut r = Rect::new(0, 0, 10, 10);
        r.r#move(2, 3);
        assert_eq!(r, Rect::new(2, 3, 12, 13));

        r.grow(1, 1);
        assert_eq!(r, Rect::new(1, 2, 13, 14));

        // negative grow shrinks
        r.grow(-1, -1);
        assert_eq!(r, Rect::new(2, 3, 12, 13));

        // chaining returns &mut Self
        let mut c = Rect::new(0, 0, 4, 4);
        c.r#move(1, 1).grow(1, 1);
        assert_eq!(c, Rect::new(0, 0, 6, 6));
    }

    #[test]
    fn rect_intersect_and_union() {
        let mut r = Rect::new(0, 0, 10, 10);
        r.intersect(&Rect::new(5, 5, 20, 8));
        assert_eq!(r, Rect::new(5, 5, 10, 8));

        let mut u = Rect::new(0, 0, 4, 4);
        u.r#union(&Rect::new(2, 2, 10, 6));
        assert_eq!(u, Rect::new(0, 0, 10, 6));
    }

    #[test]
    fn rect_contains_is_half_open() {
        let r = Rect::new(0, 0, 10, 5);
        // inside
        assert!(r.contains(Point::new(0, 0)));
        assert!(r.contains(Point::new(9, 4)));
        // right/bottom edges are EXCLUDED (classic off-by-one trap)
        assert!(!r.contains(Point::new(10, 0)));
        assert!(!r.contains(Point::new(0, 5)));
        assert!(!r.contains(Point::new(10, 5)));
        // left/top edges are INCLUDED
        assert!(r.contains(Point::new(0, 4)));
    }

    #[test]
    fn rect_is_empty() {
        assert!(Rect::new(0, 0, 0, 0).is_empty());
        assert!(Rect::new(5, 0, 5, 10).is_empty()); // zero width
        assert!(Rect::new(0, 5, 10, 5).is_empty()); // zero height
        assert!(Rect::new(3, 4, 2, 1).is_empty()); // inverted
        assert!(!Rect::new(0, 0, 1, 1).is_empty());
    }
}
