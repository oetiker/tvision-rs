//! `TDeskTop` — the desktop group (row 30, FOUNDATION).
//!
//! `TDeskTop` (`tdesktop.cpp`) is a `TGroup` subclass that owns a
//! [`Background`](crate::desktop::Background) and (later) tiles/cascades the
//! windows inserted into it. Its value at this row is twofold: it gives
//! [`Program`](crate::app::Program) a **named real desktop**, and it is the first
//! exemplar of the **"a [`View`] that embeds a [`Group`] and delegates the whole
//! trait"** pattern — the same shape `TWindow` (row 33) copies.
//!
//! ## Deviations in play
//! * **D2** embed-and-delegate: [`Desktop`] embeds a [`Group`] and forwards every
//!   [`View`] method to it. Unlike `Program` (which is *not* a `View`), a
//!   `Desktop` *is* a `View` — a child of the program's root group — so it
//!   implements the trait by delegation.
//! * **D3** owner-data-down: no owner back-pointer; the inserted background is
//!   recorded as a local [`ViewId`] ([`Desktop::background`]).
//! * **D7** [`Role::Background`](crate::theme::Role) styles the fill (handled
//!   inside [`Background`]).
//! * **D8** whole-tree redraw; no `shutDown` redraw bracket.
//! * **D9** the `cmNext`/`cmPrev` Z-reorder behaviour of `TDeskTop::handleEvent`
//!   defers to row 33 — see the breadcrumb in [`Desktop::handle_event`].
//! * **D12** streamable `read`/`write`/`build`/`name` dropped.
//!
//! ## Deferred (no dead stubs)
//! * `tile`/`cascade`/`tileError` (the `mostEqualDivisors`/`calcTileRect`/
//!   `doCascade` tiling geometry) — needs `ofTileable` + a `locate` path; lands
//!   when windows exist (row 33+).
//! * `shutDown` (`background = 0; TGroup::shutDown()`) — no shutDown path yet.
//! * `tileColumnsFirst` field — only `tile` reads it, so it is not added.

use crate::command::Command;
use crate::event::Event;
use crate::view::{Context, DrawCtx, Group, Point, Rect, StateFlag, View, ViewId, ViewState};

use super::Background;

/// The default desktop background fill — `TDeskTop::defaultBkgrnd` (`tvtext2.cpp`).
///
/// C++ `'\xB0'` is CP437 `0xB0`, which is **U+2591 ░ LIGHT SHADE** (the project's
/// CP437 convention, the same shade family as the scrollbar glyphs in
/// `theme.rs`). This is the faithful glyph — not `'▒'` (U+2592), which appears
/// only in arbitrary test scaffolding.
const DEFAULT_BKGRND: char = '\u{2591}';

/// `TDeskTop` — the desktop group: an embedded [`Group`] that owns a
/// [`Background`] (D2/D3, row 30).
///
/// Build with [`Desktop::new`] supplying a background factory (use
/// [`Desktop::init_background`] for the faithful default), then drive it as any
/// other [`View`].
pub struct Desktop {
    /// The embedded container (D2). `Desktop` *is-a* `TGroup`: its state, draw,
    /// and event routing are the group's.
    group: Group,
    /// The inserted background child's id — `TDeskTop::background`.
    ///
    /// Consumed by `cmPrev`'s `putInFrontOf(background)` at row 33; exposed via
    /// [`background`](Self::background) now so the field stays live under
    /// `-D warnings`.
    background: Option<ViewId>,
}

impl Desktop {
    /// `TDeskTop::TDeskTop(bounds)` + `TDeskInit` — construct the desktop.
    ///
    /// Ports the C++ ctor faithfully:
    /// 1. `TGroup(bounds)`.
    /// 2. `growMode = gfGrowHiX | gfGrowHiY`.
    /// 3. `tileColumnsFirst = False` (dropped — only `tile` reads it, deferred).
    /// 4. `if( createBackground && (background = createBackground(getExtent())) )
    ///    insert(background)`.
    ///
    /// The background factory is injected (the `TDeskInit` factory-mixin, mirroring
    /// [`Program::new`](crate::app::Program::new)); pass [`Desktop::init_background`]
    /// for the faithful default. `get_extent()` is the local-origin extent.
    pub fn new(
        bounds: Rect,
        create_background: impl FnOnce(Rect) -> Option<Box<dyn View>>,
    ) -> Self {
        let mut group = Group::new(bounds);
        // C++: growMode = gfGrowHiX | gfGrowHiY
        group.state_mut().grow_mode.hi_x = true;
        group.state_mut().grow_mode.hi_y = true;

        let mut desktop = Desktop {
            group,
            background: None,
        };
        // C++: if( createBackground && (background = createBackground(getExtent())) )
        //          insert(background)
        let extent = desktop.group.state().get_extent();
        if let Some(view) = create_background(extent) {
            desktop.background = Some(desktop.group.insert(view));
        }
        desktop
    }

    /// `TDeskTop::initBackground` — the default background factory:
    /// `new TBackground(r, defaultBkgrnd)`.
    pub fn init_background(r: Rect) -> Box<dyn View> {
        Box::new(Background::new(r, DEFAULT_BKGRND))
    }

    /// `TDeskTop::background` — the background child's id (row 33's
    /// `putInFrontOf(background)` target).
    pub fn background(&self) -> Option<ViewId> {
        self.background
    }
}

impl View for Desktop {
    fn state(&self) -> &ViewState {
        self.group.state()
    }

    fn state_mut(&mut self) -> &mut ViewState {
        self.group.state_mut()
    }

    /// Delegated to the embedded group's `drawSubViews`.
    fn draw(&mut self, ctx: &mut DrawCtx) {
        self.group.draw(ctx);
    }

    /// `TDeskTop::handleEvent` — for row 30 this only delegates to the embedded
    /// group's three-phase router.
    ///
    // TODO(row 33, D9): TDeskTop::handleEvent's command override. After delegating
    // to the group, if event is a command:
    //   cmNext: if valid(cmReleasedFocus) { selectNext(false) }   // findNext+select
    //   cmPrev: if valid(cmReleasedFocus) { current.putInFrontOf(background) }  // Z-reorder
    //   default: return WITHOUT clearing the event.
    // clearEvent is reached ONLY for cmNext/cmPrev. Needs ofTopSelect/makeFirst/
    // putInFrontOf (row 33) + numbered windows, so deferred whole. Both commands
    // start disabled in default_command_set and there are no windows at row 30, so
    // the override has zero observable effect here.
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        self.group.handle_event(ev, ctx);
    }

    fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
        self.group.set_state(flag, enable, ctx);
    }

    fn valid(&self, cmd: Command) -> bool {
        self.group.valid(cmd)
    }

    fn awaken(&mut self) {
        self.group.awaken();
    }

    fn size_limits(&self, owner_size: Point) -> (Point, Point) {
        self.group.size_limits(owner_size)
    }

    fn calc_bounds(&mut self, owner_size: Point, delta: Point) -> Rect {
        self.group.calc_bounds(owner_size, delta)
    }

    fn change_bounds(&mut self, bounds: Rect) {
        self.group.change_bounds(bounds);
    }

    fn cursor_request(&self) -> Option<Point> {
        self.group.cursor_request()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::screen::Buffer;
    use crate::theme::Theme;

    // -- 1. ctor inserts background + records its id --------------------------

    #[test]
    fn new_inserts_background_and_records_id() {
        let desktop = Desktop::new(Rect::new(0, 0, 10, 5), |r| {
            Some(Desktop::init_background(r))
        });
        assert!(
            desktop.background().is_some(),
            "the default factory inserts a background and its id is recorded"
        );
        assert_eq!(desktop.group.len(), 1, "exactly one child (the background)");
    }

    // -- 2. growMode = gfGrowHiX | gfGrowHiY ---------------------------------

    #[test]
    fn new_sets_grow_hi_x_and_hi_y() {
        let desktop = Desktop::new(Rect::new(0, 0, 10, 5), |r| {
            Some(Desktop::init_background(r))
        });
        let gm = desktop.state().grow_mode;
        assert!(gm.hi_x, "gfGrowHiX must be set");
        assert!(gm.hi_y, "gfGrowHiY must be set");
        // Others must stay clear.
        assert!(!gm.lo_x);
        assert!(!gm.lo_y);
        assert!(!gm.rel);
        assert!(!gm.fixed);
    }

    // -- 3. init_background fill char is ░ (U+2591) --------------------------

    #[test]
    fn init_background_fills_with_light_shade() {
        let theme = Theme::classic_blue();
        let mut bg = Desktop::init_background(Rect::new(0, 0, 4, 2));
        let mut buf = Buffer::new(4, 2);
        {
            let bounds = bg.state().get_bounds();
            let mut ctx = DrawCtx::new(&mut buf, &theme, bounds, bounds.a);
            bg.draw(&mut ctx);
        }
        // Guards the faithfulness bug directly: must be U+2591 LIGHT SHADE.
        assert_eq!(
            buf.get(0, 0).symbol(),
            "\u{2591}",
            "default background fill is ░ U+2591 (CP437 0xB0), not ▒"
        );
        assert_eq!(buf.get(3, 1).symbol(), "\u{2591}");
    }

    // -- 4. no-background factory --------------------------------------------

    #[test]
    fn no_background_factory_leaves_group_empty() {
        let mut desktop = Desktop::new(Rect::new(0, 0, 10, 5), |_| None);
        assert_eq!(desktop.background(), None, "no id recorded");
        assert!(desktop.group.is_empty(), "no children inserted");
        // draw is a no-op (must not panic).
        let theme = Theme::classic_blue();
        let mut buf = Buffer::new(10, 5);
        let bounds = desktop.state().get_bounds();
        let mut ctx = DrawCtx::new(&mut buf, &theme, bounds, bounds.a);
        desktop.draw(&mut ctx);
    }

    // -- 5. mandatory snapshot -----------------------------------------------

    /// End-to-end snapshot: a `Desktop` (via the faithful `init_background`)
    /// through the real `Renderer` + `HeadlessBackend`, drawn through
    /// `&mut dyn View`. A full-area ░ fill.
    #[test]
    fn desktop_render_pipeline_snapshot() {
        let theme = Theme::classic_blue();
        let mut desktop: Box<dyn View> = Box::new(Desktop::new(Rect::new(0, 0, 8, 4), |r| {
            Some(Desktop::init_background(r))
        }));
        let (backend, screen) = HeadlessBackend::new(8, 4);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = desktop.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            desktop.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    // -- 6. resize delegates to the group ------------------------------------

    #[test]
    fn change_bounds_delegates_and_grows_background() {
        let mut desktop = Desktop::new(Rect::new(0, 0, 20, 10), |r| {
            Some(Desktop::init_background(r))
        });
        // Background was inserted at the desktop's local extent (0,0,20,10) with
        // gfGrowHiX|HiY, so its hi edges track the desktop.
        View::change_bounds(&mut desktop, Rect::new(0, 0, 25, 13)); // UFCS: disambiguate the trait method
        let child_bounds = desktop.group.child_state_mut(0).get_bounds();
        assert_eq!(
            child_bounds,
            Rect::new(0, 0, 25, 13),
            "the background grew with the desktop (delegation to Group::change_bounds works)"
        );
    }
}
