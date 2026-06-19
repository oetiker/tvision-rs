//! The desktop [`Background`] — the backdrop the desktop and its windows sit
//! on.
//!
//! The simplest concrete view: it fills its entire extent with a repeated
//! pattern character styled with [`Role::Background`], and handles no events.
//! Its grow mode stretches it with its owner on the right and bottom edges, so
//! it always covers the desktop.

use crate::theme::Role;
use crate::view::{DrawCtx, Rect, View, ViewState};

/// Desktop background fill.
///
/// Fills its extent with a single repeated `pattern` character, styled through
/// [`Role::Background`] from the active [`Theme`](crate::theme::Theme).
///
/// # Example
/// ```
/// # use tvision_rs::{Background, Rect};
/// let _bg = Background::new(Rect::new(0, 0, 80, 25), '▒');
/// ```
///
/// # Turbo Vision heritage
///
/// Ports `TBackground` (`tbkgrnd.cpp`), which overrides only its draw method. The
/// background colour index becomes [`Role::Background`] (deviation D7), and the
/// streaming machinery is dropped (deviation D12).
pub struct Background {
    st: ViewState,
    /// The character tiled across the entire background extent on every draw.
    ///
    /// Set this to any printable Unicode scalar before (or after) construction to
    /// customise the fill. The default desktop fill is `'\u{2591}'` (░ U+2591
    /// LIGHT SHADE). Any single-column glyph works; double-width characters fill
    /// correctly but leave a trailing continuation cell for each glyph.
    pub pattern: char,
}

impl Background {
    /// Construct a background filling `bounds` with `pattern`.
    ///
    /// Sets the grow mode so the background stretches with its owner on the right
    /// and bottom edges, always covering the desktop.
    pub fn new(bounds: Rect, pattern: char) -> Self {
        let mut st = ViewState::new(bounds);
        // Grow with the owner's right and bottom edges.
        st.grow_mode.hi_x = true;
        st.grow_mode.hi_y = true;
        Background { st, pattern }
    }
}

impl View for Background {
    fn state(&self) -> &ViewState {
        &self.st
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.st
    }

    /// Fill the entire extent with `pattern`, styled with [`Role::Background`].
    fn draw(&mut self, ctx: &mut DrawCtx) {
        let ext = self.st.get_extent();
        let style = ctx.style(Role::Background);
        ctx.fill(ext, self.pattern, style);
    }

    // The background does not handle events — it uses the default no-op routing
    // (mouse-down selection happens in the owning group).
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use crate::view::{DrawCtx, Point};

    // -- Constructor ---------------------------------------------------------

    #[test]
    fn new_stores_pattern_and_bounds() {
        let bg = Background::new(Rect::new(0, 0, 10, 5), '▒');
        assert_eq!(bg.pattern, '▒');
        assert_eq!(bg.st.origin, Point::new(0, 0));
        assert_eq!(bg.st.size, Point::new(10, 5));
    }

    #[test]
    fn new_sets_grow_hi_x_and_hi_y() {
        let bg = Background::new(Rect::new(0, 0, 10, 5), '▒');
        assert!(bg.st.grow_mode.hi_x, "grow-right must be set");
        assert!(bg.st.grow_mode.hi_y, "grow-down must be set");
        // lo_x, lo_y, rel, fixed must stay clear
        assert!(!bg.st.grow_mode.lo_x);
        assert!(!bg.st.grow_mode.lo_y);
        assert!(!bg.st.grow_mode.rel);
        assert!(!bg.st.grow_mode.fixed);
    }

    #[test]
    fn new_inherits_view_state_defaults() {
        let bg = Background::new(Rect::new(5, 3, 25, 10), '░');
        // visible must be set (view-state default)
        assert!(bg.st.state.visible);
        // limit_lo_y must be set (view-state default)
        assert!(bg.st.drag_mode.limit_lo_y);
    }

    // -- draw ----------------------------------------------------------------

    #[test]
    fn draw_fills_extent_with_pattern() {
        let theme = Theme::classic_blue();
        let mut bg = Background::new(Rect::new(0, 0, 4, 2), 'X');
        let mut buf = Buffer::new(4, 2);
        {
            let bounds = bg.state().get_bounds();
            let mut ctx = DrawCtx::new(&mut buf, &theme, bounds, bounds.a);
            bg.draw(&mut ctx);
        }
        // Every cell must contain 'X'
        for y in 0..2u16 {
            for x in 0..4u16 {
                assert_eq!(buf.get(x, y).symbol(), "X", "cell ({x},{y}) must be 'X'");
            }
        }
        // Style must be Role::Background
        let expected_style = theme.style(Role::Background);
        assert_eq!(buf.get(0, 0).style(), expected_style);
        assert_eq!(buf.get(3, 1).style(), expected_style);
    }

    // -- Snapshot test -------------------------------------------------------

    /// End-to-end snapshot: `Background` through the real `Renderer` +
    /// `HeadlessBackend` path (the template every widget test copies).
    /// Drawn through `&mut dyn View` so the *trait* dispatch exercises `DrawCtx`.
    #[test]
    fn background_render_pipeline_snapshot() {
        let theme = Theme::classic_blue();
        let mut bg: Box<dyn View> = Box::new(Background::new(Rect::new(0, 0, 6, 3), '▒'));
        let (backend, screen) = HeadlessBackend::new(6, 3);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = bg.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            bg.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }
}
