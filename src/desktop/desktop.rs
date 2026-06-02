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
//! * **D9** the `cmNext`/`cmPrev` window cycling of `TDeskTop::handleEvent` is
//!   implemented at 33d-2 (see [`Desktop::handle_event`]); Alt-N's
//!   `cmSelectWindowNum` arm is realized as the [`Desktop::select_window_num`]
//!   direct walk.
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
    /// Consumed by the `cmPrev` arm in [`handle_event`](Self::handle_event), which
    /// reads `self.background` directly for `current->putInFrontOf(background)`
    /// (send the current window to the back). Also exposed via
    /// [`background`](Self::background).
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

    /// Test hook: insert an arbitrary view (a window) directly into the embedded
    /// group, returning its id. Used by the 33d-2 round-trip tests, which must
    /// place windows *inside the desktop* (the cmNext/cmPrev/Alt-N handlers live
    /// on the desktop) — there is no production window-insert seam yet
    /// (`tile`/`cascade` land later).
    #[cfg(test)]
    pub(crate) fn insert_view(&mut self, view: Box<dyn View>) -> ViewId {
        self.group.insert(view)
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

    /// `TDeskTop::handleEvent` — delegate to the embedded group's three-phase
    /// router, then handle the desktop's own `cmNext`/`cmPrev` window cycling
    /// (33d-2). Faithful to `tdesktop.cpp`:
    /// ```cpp
    /// TGroup::handleEvent( event );
    /// if( event.what == evCommand ) switch( event.message.command ) {
    ///     case cmNext: if( valid(cmReleasedFocus) ) selectNext( False ); break;
    ///     case cmPrev: if( valid(cmReleasedFocus) ) current->putInFrontOf( background ); break;
    ///     default: return;          // NO clearEvent for other commands
    /// }
    /// clearEvent( event );          // reached ONLY for cmNext/cmPrev
    /// ```
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        self.group.handle_event(ev, ctx);
        if let Event::Command(cmd) = *ev {
            match cmd {
                Command::NEXT => {
                    if self.group.valid(Command::RELEASED_FOCUS) {
                        // selectNext(False): findNext + select. `focus_next` is that
                        // (focus_child raises ofTopSelect windows == C++ select();
                        // its outgoing validation is already gated by valid() above,
                        // so it is redundant-but-always-passes). `false` == C++
                        // `forwards == False`.
                        self.group.focus_next(false, ctx);
                    }
                    // C++ `break` falls through to clearEvent: clear even when
                    // !valid (the valid() guard wraps only the *action*).
                    ev.clear();
                }
                Command::PREV => {
                    if self.group.valid(Command::RELEASED_FOCUS)
                        && let Some(cur) = self.group.current()
                    {
                        // current->putInFrontOf(background): send current to the
                        // back, exposing the next window. NB: put_in_front_of's
                        // `target: None` means TO-TOP (the inverse); pass the
                        // resolved Some(background) so a future refactor cannot
                        // silently flip cmPrev into a raise.
                        self.group.put_in_front_of(cur, self.background, ctx);
                    }
                    ev.clear();
                }
                // C++ `default: return;` — no clearEvent for other commands.
                _ => {}
            }
        }
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

    /// Delegate the D3 tree-walk into the embedded group, so a `find_mut` from
    /// above (e.g. a root `Group` or `Program`) descends through the desktop.
    fn find_mut(&mut self, id: ViewId) -> Option<&mut dyn View> {
        self.group.find_mut(id)
    }

    /// Delegate descendant removal into the embedded group (the owning group runs
    /// the faithful removal + `reset_current`).
    fn remove_descendant(&mut self, id: ViewId, ctx: &mut Context) -> bool {
        self.group.remove_descendant(id, ctx)
    }

    /// `cmSelectWindowNum` (Alt-N) — select the desktop window numbered `num`
    /// (33d-2). Realizes the C++ broadcast arm as a direct walk into the embedded
    /// group (see [`Group::focus_by_number`]). The program reaches this through the
    /// `select_window_num` trait method — **not** an `as_any_mut` downcast — so it
    /// stays decoupled from the concrete `Desktop` type.
    fn select_window_num(&mut self, num: i16, ctx: &mut Context) -> bool {
        self.group.focus_by_number(num, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use crate::view::SelectMode;
    use crate::window::{ScrollBarOptions, Window};
    use std::collections::VecDeque;

    /// Build a throwaway `Context` over loop-owned locals, run `f`, return its
    /// value (the same harness shape the `group`/`window` test modules use).
    fn with_ctx<R>(f: impl FnOnce(&mut Context) -> R) -> R {
        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        f(&mut ctx)
    }

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

    // -- 7. D3 tree-walk resolvers through the embedders ---------------------

    /// Build `Desktop` → `Window` → probe (a standard scroll bar) and return the
    /// desktop plus the window/probe ids. The desktop's `group` is reachable here
    /// (same module); the window's probe is its scroll bar. The window is made the
    /// desktop's `current` child so a later removal's `reset_current` is
    /// observable.
    fn nested_desktop() -> (Desktop, ViewId, ViewId) {
        let mut window = Window::new(Rect::new(2, 1, 30, 12), Some("W".into()), 1);
        let probe_id = window.standard_scroll_bar(ScrollBarOptions {
            vertical: true,
            ..Default::default()
        });

        // Desktop owning the window. Reach the private group directly (same
        // module) since Desktop exposes no public arbitrary-child insert.
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 25), |_| None);
        let window_id = desktop.group.insert(Box::new(window));
        with_ctx(|ctx| {
            desktop
                .group
                .set_current(Some(window_id), SelectMode::Normal, ctx)
        });

        (desktop, window_id, probe_id)
    }

    #[test]
    fn find_mut_resolves_through_desktop_and_window() {
        let (desktop, window_id, probe_id) = nested_desktop();
        // Wrap the desktop in a plain root group to prove the walk descends
        // through *both* embedders (root Group -> Desktop -> Window -> group).
        let mut root = Group::new(Rect::new(0, 0, 80, 25));
        root.insert(Box::new(desktop));

        // The deeply-nested probe resolves through Desktop -> Window -> group.
        {
            let v = root
                .find_mut(probe_id)
                .expect("probe resolves through the embedders");
            v.state_mut().set_cursor(7, 8); // mutate a field through the reference
        }
        assert_eq!(
            root.find_mut(probe_id)
                .expect("probe resolves")
                .state()
                .cursor,
            Point::new(7, 8),
            "mutation through the nested find_mut is observed"
        );

        // The window itself resolves (it is a direct child of the desktop group).
        assert!(
            root.find_mut(window_id).is_some(),
            "the window resolves through the desktop"
        );

        // A never-inserted id resolves to None.
        let bogus = ViewId::next();
        assert!(root.find_mut(bogus).is_none(), "unknown id -> None");
    }

    /// `remove_descendant` recurses into a child group and removes a grandchild.
    ///
    /// The probe is NOT a direct child of the desktop group, so the direct-child
    /// check fails and the implementation must recurse into the window group —
    /// exercising the `for child … remove_descendant … return true` branch.
    #[test]
    fn remove_descendant_recurses_into_child_group_for_grandchild() {
        let (mut desktop, window_id, probe_id) = nested_desktop();

        // Remove the probe — it is a child of the window, not of the desktop group.
        let removed = with_ctx(|ctx| desktop.remove_descendant(probe_id, ctx));
        assert!(removed, "recursion-success branch returns true");

        // The probe is gone.
        assert!(
            desktop.find_mut(probe_id).is_none(),
            "probe is no longer reachable"
        );
        // The window itself is still present.
        assert!(
            desktop.find_mut(window_id).is_some(),
            "the window that owned the probe is still present"
        );
    }

    #[test]
    fn remove_descendant_removes_through_embedders_and_resets_current() {
        // Operate on the Desktop directly (its `remove_descendant` delegates to
        // the group), so the owning desktop group's `current` is readable here.
        let (mut desktop, window_id, probe_id) = nested_desktop();
        assert_eq!(
            desktop.group.current(),
            Some(window_id),
            "window is current"
        );

        // Bogus id: false, nothing changes.
        let removed_bogus = with_ctx(|ctx| desktop.remove_descendant(ViewId::next(), ctx));
        assert!(!removed_bogus, "unknown id -> false");
        assert!(
            desktop.find_mut(window_id).is_some(),
            "window still present"
        );

        // Remove the window (a child of the desktop group, reached via the
        // Desktop delegate). reset_current runs: no other selectable child
        // remains, so current becomes None.
        let removed = with_ctx(|ctx| desktop.remove_descendant(window_id, ctx));
        assert!(removed, "nested removal -> true");
        assert!(desktop.find_mut(window_id).is_none(), "window gone");
        assert!(
            desktop.find_mut(probe_id).is_none(),
            "the window's child is gone with it"
        );
        assert_eq!(
            desktop.group.current(),
            None,
            "reset_current ran on the owning desktop group"
        );
    }
}
