//! A 1-row strip that displays an editor's current cursor position (row:col).
//! It is owned by the editor and updated through [`Indicator::set_value`].
//!
//! # Color
//!
//! [`Role::IndicatorNormal`] when not dragging, [`Role::IndicatorDragging`]
//! while the editor window is being dragged.
//!
//! # Glyphs
//!
//! Glyphs come from `ctx.glyphs()`:
//! - `indicator_frame_normal` (`═`) — fill char when **not** dragging.
//! - `indicator_frame_dragging` (`─`) — fill char **while** dragging.
//! - `indicator_modified` (`☼`) — the "buffer modified" marker.
//!
//! # Colon alignment
//!
//! The position string is placed so the `:` lands at column 8. The string is
//! `" {row}:{col} "`, so the colon sits at byte index `1 + digits(row)` and
//! drawing starts at `7 - digits(row)`. That only goes negative once the row
//! number has 8 or more digits (row ≥ 10,000,000) — unreachable in a real
//! editor, but handled gracefully: [`DrawCtx::put_str`] skips the off-screen
//! prefix without panicking.
//!
//! There is no event handling; the indicator is display-only. The dragging
//! state is read live from the view state each frame.
//!
//! # Turbo Vision heritage
//!
//! Ports `TIndicator` (`tindictr.cpp`). The hardcoded frame characters become
//! [`Glyphs`](crate::theme::Glyphs) fields and the palette becomes [`Role`]s
//! (deviation D7).

use crate::theme::Role;
use crate::view::{DrawCtx, GrowMode, Point, Rect, View, ViewState};

// ---------------------------------------------------------------------------
// Indicator
// ---------------------------------------------------------------------------

/// A row/column position display for an editor.
///
/// The owner pushes updates via [`set_value`](Self::set_value); the next
/// whole-tree redraw reads the updated fields.
///
/// # Turbo Vision heritage
///
/// Ports `TIndicator` (`tindictr.cpp`).
pub struct Indicator {
    /// View state (geometry, flags, etc.) — the composition target.
    pub state: ViewState,
    /// Current cursor position displayed as `row:col`.
    ///
    /// `y` is the zero-based row; `x` is the zero-based column. Both are
    /// displayed one-based (so `(0, 0)` renders as `"1:1"`). Read this
    /// field to inspect the editor's last-reported position; write it only
    /// through [`set_value`](Self::set_value), which the pump calls when it
    /// applies a `Deferred::IndicatorSetValue` queued by the editor.
    ///
    /// Initialized to `(0, 0)` on construction.
    pub location: Point,
    /// Whether the editor buffer has unsaved changes.
    ///
    /// When `true` the indicator renders a `☼` marker at column 0. Updated
    /// alongside [`location`](Self::location) through
    /// [`set_value`](Self::set_value); do not write directly.
    ///
    /// Initialized to `false` on construction.
    pub modified: bool,
}

impl Indicator {
    /// Create a display-only indicator sized to `bounds`.
    ///
    /// Embed the returned `Indicator` in an editor group, then let the pump
    /// keep it current: whenever the editor's cursor moves it queues a
    /// `Deferred::IndicatorSetValue`; the pump resolves that into a call to
    /// [`set_value`](Self::set_value) and the next whole-tree redraw picks up
    /// the new values.
    ///
    /// The indicator starts at position `(0, 0)` and unmodified. Its
    /// `grow_mode` is set to `lo_y | hi_y` so that it follows the bottom edge
    /// of the enclosing window when the window is resized. It is never
    /// selectable — the indicator is display-only and cannot receive focus.
    ///
    /// # Turbo Vision heritage
    ///
    /// Mirrors `TIndicator::TIndicator(bounds)`: sets
    /// `growMode = gfGrowLoY | gfGrowHiY` and does not set `ofSelectable`.
    pub fn new(bounds: Rect) -> Self {
        let mut state = ViewState::new(bounds);
        state.grow_mode = GrowMode {
            lo_y: true,
            hi_y: true,
            ..Default::default()
        };
        Indicator {
            state,
            location: Point::new(0, 0),
            modified: false,
        }
    }

    /// Update the cursor position and modified flag displayed by the indicator.
    ///
    /// Do not call this directly from application code. The intended call path
    /// is: editor queues `Deferred::IndicatorSetValue` → pump resolves the
    /// deferred by downcasting via `as_any_mut` to `&mut Indicator` and
    /// calling this method. The next whole-tree redraw then picks up the new
    /// values without any explicit `drawView` call.
    ///
    /// This differs from the C++ `TIndicator::setValue`, which calls `drawView`
    /// immediately and skips the update entirely when neither value changes. The
    /// Rust port omits the no-op guard (the unconditional whole-tree redraw
    /// makes it unnecessary) and defers the redraw to the pump cycle.
    ///
    /// # Turbo Vision heritage
    ///
    /// Ports `TIndicator::setValue(loc, mod)` (`tindictr.cpp`), minus the
    /// early-return guard for unchanged values.
    pub fn set_value(&mut self, location: Point, modified: bool) {
        self.location = location;
        self.modified = modified;
    }
}

impl View for Indicator {
    fn state(&self) -> &ViewState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }

    /// Concrete-reach hatch (the sanctioned downcast): the pump downcasts to
    /// `&mut Indicator` to call [`set_value`](Self::set_value) when applying a
    /// [`Deferred::IndicatorSetValue`](crate::view::Deferred::IndicatorSetValue)
    /// from an editor's `doUpdate`. Without this the broker's downcast yields
    /// `None` and the indicator never updates (stuck at its `(0,0)` → "1:1").
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    /// Paint the one-row position display.
    ///
    /// Draw order:
    /// 1. Fill row 0 with the frame char (`═` or `─`) in the current role.
    /// 2. If `modified`, overwrite column 0 with `☼` in the same role.
    /// 3. Draw the position string so that the `:` lands at column 8.
    fn draw(&mut self, ctx: &mut DrawCtx) {
        let glyphs = *ctx.glyphs();
        let dragging = self.state.state.dragging;

        // Faithful inversion: C++ draws `dragFrame` (═) when NOT dragging,
        // and `normalFrame` (─) while dragging. Our glyph names follow the
        // C++ semantics verbatim despite the inverted field names.
        let (frame_ch, color) = if !dragging {
            (
                glyphs.indicator_frame_normal,
                ctx.style(Role::IndicatorNormal),
            )
        } else {
            (
                glyphs.indicator_frame_dragging,
                ctx.style(Role::IndicatorDragging),
            )
        };

        // Step 1: fill the entire row with the frame character.
        ctx.fill(Rect::new(0, 0, self.state.size.x, 1), frame_ch, color);

        // Step 2: modified marker at column 0 (C++ `b.putChar(0, 15)`; char 15 = ☼).
        if self.modified {
            ctx.put_char(0, 0, glyphs.indicator_modified, color);
        }

        // Step 3: position string — " row:col " with ':' at column 8.
        //
        // C++: `os << ' ' << (location.y+1) << ':' << (location.x+1) << ' '`
        // i.e. y (row) before x (col), one-based, space-padded on both sides.
        let s = format!(" {}:{} ", self.location.y + 1, self.location.x + 1);
        // The colon's byte offset equals its column index (ASCII prefix only).
        let colon_col = s.find(':').expect("format string always contains ':'") as i32;
        let start_col = 8 - colon_col;
        // start_col can be negative for very large row numbers; put_str handles
        // that via its text_indent path (skips off-screen prefix without panic).
        ctx.put_str(start_col, 0, &s, color);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use crate::view::Point;

    // -----------------------------------------------------------------------
    // set_value
    // -----------------------------------------------------------------------

    #[test]
    fn set_value_updates_location_and_modified() {
        let mut ind = Indicator::new(Rect::new(0, 0, 20, 1));
        assert_eq!(ind.location, Point::new(0, 0));
        assert!(!ind.modified);

        ind.set_value(Point::new(4, 2), true);
        assert_eq!(ind.location, Point::new(4, 2));
        assert!(ind.modified);

        ind.set_value(Point::new(10, 99), false);
        assert_eq!(ind.location, Point::new(10, 99));
        assert!(!ind.modified);
    }

    #[test]
    fn set_value_overwrites_previous() {
        let mut ind = Indicator::new(Rect::new(0, 0, 20, 1));
        ind.set_value(Point::new(1, 1), true);
        ind.set_value(Point::new(7, 3), false);
        assert_eq!(ind.location, Point::new(7, 3));
        assert!(!ind.modified);
    }

    // -----------------------------------------------------------------------
    // grow_mode
    // -----------------------------------------------------------------------

    #[test]
    fn grow_mode_is_lo_y_and_hi_y() {
        let ind = Indicator::new(Rect::new(0, 0, 20, 1));
        assert!(ind.state.grow_mode.lo_y, "gfGrowLoY must be set");
        assert!(ind.state.grow_mode.hi_y, "gfGrowHiY must be set");
        assert!(!ind.state.grow_mode.lo_x);
        assert!(!ind.state.grow_mode.hi_x);
    }

    // -----------------------------------------------------------------------
    // Snapshot tests
    // -----------------------------------------------------------------------

    /// Normal state: location (x=4, y=2) → displays " 3:5 " with ':' at col 8.
    /// Frame = ═ (indicator_frame_normal), role = IndicatorNormal.
    #[test]
    fn snapshot_normal() {
        let theme = Theme::classic_blue();
        let mut ind = Indicator::new(Rect::new(0, 0, 16, 1));
        ind.set_value(Point::new(4, 2), false);

        let (backend, screen) = HeadlessBackend::new(16, 1);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = ind.state.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            ind.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    /// Modified marker: same position but `modified = true` → ☼ at col 0.
    #[test]
    fn snapshot_modified() {
        let theme = Theme::classic_blue();
        let mut ind = Indicator::new(Rect::new(0, 0, 16, 1));
        ind.set_value(Point::new(4, 2), true);

        let (backend, screen) = HeadlessBackend::new(16, 1);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = ind.state.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            ind.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    /// Dragging state: `sfDragging` set → ─ frame + IndicatorDragging role.
    #[test]
    fn snapshot_dragging() {
        let theme = Theme::classic_blue();
        let mut ind = Indicator::new(Rect::new(0, 0, 16, 1));
        ind.set_value(Point::new(4, 2), false);
        // Set dragging directly — no ctx needed (no side effects to propagate).
        ind.state.state.dragging = true;

        let (backend, screen) = HeadlessBackend::new(16, 1);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = ind.state.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            ind.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    /// Colon alignment check: location (x=0, y=0) → " 1:1 "
    /// colon is at byte 2, so start_col = 8 - 2 = 6; ':' rendered at col 8.
    #[test]
    fn snapshot_colon_at_column_8_small_numbers() {
        let theme = Theme::classic_blue();
        let mut ind = Indicator::new(Rect::new(0, 0, 16, 1));
        ind.set_value(Point::new(0, 0), false);

        let (backend, screen) = HeadlessBackend::new(16, 1);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = ind.state.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            ind.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    /// Large row number: location (x=0, y=9999) → " 10000:1 "
    /// colon at byte 6, start_col = 7 - 5 digits = 2; ':' at col 8. No panic.
    #[test]
    fn snapshot_large_row_number() {
        let theme = Theme::classic_blue();
        let mut ind = Indicator::new(Rect::new(0, 0, 16, 1));
        ind.set_value(Point::new(0, 9999), false);

        let (backend, screen) = HeadlessBackend::new(16, 1);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = ind.state.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            ind.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    /// Genuinely-negative `start_col`: an 8-digit displayed row (y=9_999_999 →
    /// " 10000000:1 ", colon at byte 9, `start_col = 8 - 9 = -1`). Exercises
    /// `DrawCtx::put_str`'s off-screen-prefix skip — must not panic, and the
    /// leading space of the string is clipped off the left.
    #[test]
    fn negative_start_col_does_not_panic() {
        let theme = Theme::classic_blue();
        let mut ind = Indicator::new(Rect::new(0, 0, 16, 1));
        ind.set_value(Point::new(0, 9_999_999), false);

        let (backend, screen) = HeadlessBackend::new(16, 1);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = ind.state.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            ind.draw(&mut dc);
        });
        // start_col = -1: the leading ' ' is clipped, so "10000000:1 " begins at
        // col 0 and the colon lands at col 7 (was 8 before the left-clip).
        let snap = screen.snapshot();
        assert!(
            snap.contains("10000000:1"),
            "row string rendered, left-clipped"
        );
    }
}
