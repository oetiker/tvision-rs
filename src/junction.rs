//! View-independent vocabulary for joining box-drawing linework: which frame
//! edge a divider abuts, the weight (single/double) of each line, and the pure
//! glyph-selector functions that map an (edge, bar-weight, stem-weight) tuple to
//! the matching box character in [`Glyphs`].
//!
//! This is the rstv-local equivalent of Turbo Vision's `frameChars[mask]` table
//! (`framelin.cpp`): a small finite map with no view dependencies, so it is
//! exhaustively unit-testable. The owning [`Window`](crate::window::Window)
//! pushes [`JunctionMark`]s down to its [`Frame`](crate::frame::Frame), which
//! calls [`frame_junction`] per marked edge cell; the outer
//! [`Splitter`](crate::widgets::Splitter) calls [`divider_junction`] for its
//! interior crossings. See the design spec
//! `docs/superpowers/specs/2026-06-13-splitter-frame-joining-design.md`.

use crate::theme::Glyphs;

/// Which frame edge a divider abutment (or junction cell) lands on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

/// The drawn weight of a line (a frame border or a divider).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Weight {
    Single,
    Double,
}

/// A divider abutment the owning window pushes down to its frame: the divider's
/// line meets the frame `edge` at frame-local `offset` along that edge, drawn at
/// the divider's `stem` weight. The frame substitutes the matching tee glyph
/// (chosen from its own border weight × this `stem`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct JunctionMark {
    /// Which frame edge this lands on.
    pub edge: Edge,
    /// Frame-local position along that edge (x for Top/Bottom, y for Left/Right).
    pub offset: i32,
    /// The abutting divider's drawn weight.
    pub stem: Weight,
}

/// Where two dividers meet in the interior (Site 2). `TeeRight` = a vertical
/// line with a branch going right (`├`); `TeeDown` = a horizontal line with a
/// branch going down (`┬`); `Cross` = both perpendicular branches (`┼`). Named
/// by the visual branch direction, matching the existing `frame_tee_*` glyph
/// names in [`Glyphs`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Junction {
    TeeRight,
    TeeLeft,
    TeeUp,
    TeeDown,
    Cross,
}

/// Pick the box-drawing junction for a frame edge cell that a divider abuts.
///
/// * `edge` — which frame edge the cell is on.
/// * `bar` — the frame border's own weight (Double when the window is active).
/// * `stem` — the abutting divider's weight.
///
/// The naming key in [`Glyphs`]: `_d` = both double; `_dh`/`_dv` = double bar
/// with a single perpendicular stem; `_sh`/`_sv` = single bar with a double stem.
/// For Top/Bottom the bar is horizontal (`_dh`/`_sh`); for Left/Right the bar is
/// vertical (`_dv`/`_sv`).
pub fn frame_junction(edge: Edge, bar: Weight, stem: Weight, g: &Glyphs) -> char {
    use Edge::*;
    use Weight::*;
    match (edge, bar, stem) {
        // Top edge → tee pointing down (┬ family).
        (Top, Single, Single) => g.frame_tee_t,
        (Top, Double, Single) => g.frame_tee_t_dh,
        (Top, Double, Double) => g.frame_tee_t_d,
        (Top, Single, Double) => g.frame_tee_t_sh,
        // Bottom edge → tee pointing up (┴ family).
        (Bottom, Single, Single) => g.frame_tee_b,
        (Bottom, Double, Single) => g.frame_tee_b_dh,
        (Bottom, Double, Double) => g.frame_tee_b_d,
        (Bottom, Single, Double) => g.frame_tee_b_sh,
        // Left edge → tee pointing right (├ family).
        (Left, Single, Single) => g.frame_tee_l,
        (Left, Double, Single) => g.frame_tee_l_dv,
        (Left, Double, Double) => g.frame_tee_l_d,
        (Left, Single, Double) => g.frame_tee_l_sv,
        // Right edge → tee pointing left (┤ family).
        (Right, Single, Single) => g.frame_tee_r,
        (Right, Double, Single) => g.frame_tee_r_dv,
        (Right, Double, Double) => g.frame_tee_r_d,
        (Right, Single, Double) => g.frame_tee_r_sv,
    }
}

/// Pick the box-drawing junction where two dividers meet in the interior.
///
/// `dir` is the visual shape (which way the branch points); `weight` is the
/// shared divider weight. For this feature both the through-divider and the
/// branching divider carry the same weight at draw time (a divider never changes
/// weight), so a single `weight` parameter suffices — mixed interior crossings
/// are out of scope (the spec's non-goals).
pub fn divider_junction(dir: Junction, weight: Weight, g: &Glyphs) -> char {
    use Junction::*;
    use Weight::*;
    match (dir, weight) {
        (TeeRight, Single) => g.frame_tee_l,   // ├
        (TeeRight, Double) => g.frame_tee_l_d, // ╠
        (TeeLeft, Single) => g.frame_tee_r,    // ┤
        (TeeLeft, Double) => g.frame_tee_r_d,  // ╣
        (TeeUp, Single) => g.frame_tee_b,      // ┴
        (TeeUp, Double) => g.frame_tee_b_d,    // ╩
        (TeeDown, Single) => g.frame_tee_t,    // ┬
        (TeeDown, Double) => g.frame_tee_t_d,  // ╦
        (Cross, Single) => g.frame_cross,      // ┼
        (Cross, Double) => g.frame_cross_d,    // ╬
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Glyphs;

    fn g() -> Glyphs {
        Glyphs::default()
    }

    #[test]
    fn frame_junction_single_bar_single_stem() {
        let g = g();
        assert_eq!(
            frame_junction(Edge::Top, Weight::Single, Weight::Single, &g),
            '┬'
        );
        assert_eq!(
            frame_junction(Edge::Bottom, Weight::Single, Weight::Single, &g),
            '┴'
        );
        assert_eq!(
            frame_junction(Edge::Left, Weight::Single, Weight::Single, &g),
            '├'
        );
        assert_eq!(
            frame_junction(Edge::Right, Weight::Single, Weight::Single, &g),
            '┤'
        );
    }

    #[test]
    fn frame_junction_double_bar_single_stem_is_mixed() {
        let g = g();
        assert_eq!(
            frame_junction(Edge::Top, Weight::Double, Weight::Single, &g),
            '╤'
        );
        assert_eq!(
            frame_junction(Edge::Bottom, Weight::Double, Weight::Single, &g),
            '╧'
        );
        assert_eq!(
            frame_junction(Edge::Left, Weight::Double, Weight::Single, &g),
            '╞'
        );
        assert_eq!(
            frame_junction(Edge::Right, Weight::Double, Weight::Single, &g),
            '╡'
        );
    }

    #[test]
    fn frame_junction_double_bar_double_stem() {
        let g = g();
        assert_eq!(
            frame_junction(Edge::Top, Weight::Double, Weight::Double, &g),
            '╦'
        );
        assert_eq!(
            frame_junction(Edge::Bottom, Weight::Double, Weight::Double, &g),
            '╩'
        );
        assert_eq!(
            frame_junction(Edge::Left, Weight::Double, Weight::Double, &g),
            '╠'
        );
        assert_eq!(
            frame_junction(Edge::Right, Weight::Double, Weight::Double, &g),
            '╣'
        );
    }

    #[test]
    fn frame_junction_single_bar_double_stem_is_rare_mixed() {
        let g = g();
        assert_eq!(
            frame_junction(Edge::Top, Weight::Single, Weight::Double, &g),
            '╥'
        );
        assert_eq!(
            frame_junction(Edge::Bottom, Weight::Single, Weight::Double, &g),
            '╨'
        );
        assert_eq!(
            frame_junction(Edge::Left, Weight::Single, Weight::Double, &g),
            '╟'
        );
        assert_eq!(
            frame_junction(Edge::Right, Weight::Single, Weight::Double, &g),
            '╢'
        );
    }

    #[test]
    fn divider_junction_all_directions() {
        let g = g();
        assert_eq!(
            divider_junction(Junction::TeeRight, Weight::Single, &g),
            '├'
        );
        assert_eq!(divider_junction(Junction::TeeLeft, Weight::Single, &g), '┤');
        assert_eq!(divider_junction(Junction::TeeUp, Weight::Single, &g), '┴');
        assert_eq!(divider_junction(Junction::TeeDown, Weight::Single, &g), '┬');
        assert_eq!(divider_junction(Junction::Cross, Weight::Single, &g), '┼');
        assert_eq!(divider_junction(Junction::Cross, Weight::Double, &g), '╬');
    }
}
