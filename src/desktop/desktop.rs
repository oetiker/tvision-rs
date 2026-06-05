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
//! ## Tiling geometry (row 30)
//! * `tile`/`cascade` (the `mostEqualDivisors`/`calcTileRect`/`doCascade` layout)
//!   are ported as [`View`] overrides on [`Desktop`], driven by the program's
//!   `cmTile`/`cmCascade` handler. `tileError()` is an empty C++ no-op: when its
//!   guard trips (a cell would be zero-sized, or a window's minimum exceeds the
//!   cascade rect) we simply leave bounds unchanged. `tile_columns_first`
//!   (C++ `tileColumnsFirst = False`) selects the `favorY` orientation.
//!
//! ## Deferred (no dead stubs)
//! * `shutDown` (`background = 0; TGroup::shutDown()`) — no shutDown path yet.

use crate::command::Command;
use crate::event::Event;
use crate::view::{Context, Group, Rect, View, ViewId, locate};

use super::Background;

// -- tiling geometry helpers (verbatim ports of the tdesktop.cpp file statics --
// turned into pure functions; no globals — the C++ `numCols`/`numRows`/`leftOver`
// statics are threaded as parameters).

/// `iSqr(i)` (`tdesktop.cpp`) — integer square-root-ish helper used by
/// `mostEqualDivisors`. Faithful `i32` port of the `abs((int)(res1-res2)) > 1`
/// Newton-style loop.
fn i_sqr(i: i32) -> i32 {
    let mut res1 = 2;
    let mut res2 = i / res1;
    while (res1 - res2).abs() > 1 {
        res1 = (res1 + res2) / 2;
        res2 = i / res1;
    }
    if res1 < res2 { res1 } else { res2 }
}

/// `mostEqualDivisors(n, x, y, favorY)` (`tdesktop.cpp`) — factor `n` into the
/// most-equal pair. Returns `(x, y)`. `favor_y` puts the larger factor on `y`
/// (C++ `favorY = !tileColumnsFirst`).
fn most_equal_divisors(n: i32, favor_y: bool) -> (i32, i32) {
    let mut i = i_sqr(n);
    if n % i != 0 && n % (i + 1) == 0 {
        i += 1;
    }
    if i < n / i {
        i = n / i;
    }
    if favor_y {
        (n / i, i) // x = n/i, y = i
    } else {
        (i, n / i) // x = i,   y = n/i
    }
}

/// `dividerLoc(lo, hi, num, pos)` (`tdesktop.cpp`) — the `pos`-th of `num` evenly
/// spaced divider coordinates between `lo` and `hi`. C++ does the multiply in
/// `long`; coords are `i32`, so the product is computed in `i64` to avoid
/// overflow, faithful to `int(long(hi-lo)*pos/long(num)+lo)`.
fn divider_loc(lo: i32, hi: i32, num: i32, pos: i32) -> i32 {
    ((hi - lo) as i64 * pos as i64 / num as i64) as i32 + lo
}

/// `calcTileRect(pos, r)` (`tdesktop.cpp`) — the cell rect for tile slot `pos`
/// in the `num_cols × num_rows` grid (with `left_over` columns carrying an extra
/// row). The C++ file statics `numCols`/`numRows`/`leftOver` are passed in.
fn calc_tile_rect(pos: i32, r: Rect, num_cols: i32, num_rows: i32, left_over: i32) -> Rect {
    let d = (num_cols - left_over) * num_rows;
    let (x, y) = if pos < d {
        (pos / num_rows, pos % num_rows)
    } else {
        (
            (pos - d) / (num_rows + 1) + (num_cols - left_over),
            (pos - d) % (num_rows + 1),
        )
    };
    let mut n_rect = Rect::new(0, 0, 0, 0);
    n_rect.a.x = divider_loc(r.a.x, r.b.x, num_cols, x);
    n_rect.b.x = divider_loc(r.a.x, r.b.x, num_cols, x + 1);
    if pos >= d {
        n_rect.a.y = divider_loc(r.a.y, r.b.y, num_rows + 1, y);
        n_rect.b.y = divider_loc(r.a.y, r.b.y, num_rows + 1, y + 1);
    } else {
        n_rect.a.y = divider_loc(r.a.y, r.b.y, num_rows, y);
        n_rect.b.y = divider_loc(r.a.y, r.b.y, num_rows, y + 1);
    }
    n_rect
}

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
    /// `TDeskTop::tileColumnsFirst` — orientation flag read by [`tile`](Self::tile)
    /// (`favorY = !tile_columns_first`). C++ ctor sets it `False`.
    tile_columns_first: bool,
}

impl Desktop {
    /// `TDeskTop::TDeskTop(bounds)` + `TDeskInit` — construct the desktop.
    ///
    /// Ports the C++ ctor faithfully:
    /// 1. `TGroup(bounds)`.
    /// 2. `growMode = gfGrowHiX | gfGrowHiY`.
    /// 3. `tileColumnsFirst = False` (read by [`tile`](Self::tile)).
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
            // C++ ctor: tileColumnsFirst = False
            tile_columns_first: false,
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

    /// Insert an arbitrary view (a window) directly into the embedded group,
    /// returning its id — the production window-insert seam (faithful to the public
    /// `TGroup::insert` that `TDeskTop` inherits). Windows must live *inside the
    /// desktop* because the `cmNext`/`cmPrev`/Alt-N handlers live on it. Used by the
    /// 33d-2 round-trip tests and by app code that pre-populates the desktop (see
    /// `examples/hello.rs`); the geometry-laying `tile`/`cascade` helpers land later.
    pub fn insert_view(&mut self, view: Box<dyn View>) -> ViewId {
        self.group.insert(view)
    }
}

#[crate::delegate(to = group, skip(value, set_value, number, grabs_focus_on_click, apply_list_scroll, as_any_mut))]
impl View for Desktop {
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

    /// `cmSelectWindowNum` (Alt-N) — select the desktop window numbered `num`
    /// (33d-2). Realizes the C++ broadcast arm as a direct walk into the embedded
    /// group (see [`Group::focus_by_number`]). The program reaches this through the
    /// `select_window_num` trait method — **not** an `as_any_mut` downcast — so it
    /// stays decoupled from the concrete `Desktop` type.
    fn select_window_num(&mut self, num: i16, ctx: &mut Context) -> bool {
        self.group.focus_by_number(num, ctx)
    }

    /// `TDeskTop::tile(r)` — lay the tileable windows into a most-equal grid of
    /// cells over `r`. Faithful port of `tdesktop.cpp`:
    /// ```cpp
    /// numTileable = 0; forEach( doCountTileable, 0 );
    /// if( numTileable > 0 ) {
    ///     mostEqualDivisors( numTileable, numCols, numRows, !tileColumnsFirst );
    ///     if( (r.b.x-r.a.x)/numCols == 0 || (r.b.y-r.a.y)/numRows == 0 ) tileError();
    ///     else { leftOver = numTileable % numCols; tileNum = numTileable - 1;
    ///            forEach( doTile, &r ); }
    /// }
    /// ```
    /// `doTile` calls `p->locate(calcTileRect(tileNum--))` per tileable child in
    /// `forEach` order, so the *first-visited* (topmost) child takes `tileNum =
    /// numTileable - 1`. `tileError()` is an empty no-op → on the guard we leave
    /// bounds unchanged. `lock()`/`unlock()` are dropped (D8). `owner_size` is the
    /// desktop size, fed to each child's `size_limits` inside [`locate`].
    fn tile(&mut self, r: Rect) {
        let ids = self.group.tileable_ids(); // forEach order
        let n = ids.len() as i32; // numTileable
        if n == 0 {
            return;
        }
        let favor_y = !self.tile_columns_first;
        let (num_cols, num_rows) = most_equal_divisors(n, favor_y);
        // tileError guard: a cell would be zero-width or zero-height.
        if (r.b.x - r.a.x) / num_cols == 0 || (r.b.y - r.a.y) / num_rows == 0 {
            return;
        }
        let left_over = n % num_cols;
        let owner_size = self.group.state().size;
        let mut tile_num = n - 1; // FIRST visited gets numTileable - 1
        for id in ids {
            let rect = calc_tile_rect(tile_num, r, num_cols, num_rows, left_over);
            if let Some(v) = self.group.child_mut(id) {
                locate(v, rect, owner_size);
            }
            tile_num -= 1;
        }
    }

    /// `TDeskTop::cascade(r)` — stack the tileable windows offset by one cell each.
    /// Faithful port of `tdesktop.cpp`:
    /// ```cpp
    /// cascadeNum = 0; forEach( doCount, 0 );   // cascadeNum = count, lastView = last
    /// if( cascadeNum > 0 ) {
    ///     lastView->sizeLimits( min, max );
    ///     if( min.x > r.b.x-r.a.x-cascadeNum || min.y > r.b.y-r.a.y-cascadeNum )
    ///         tileError();
    ///     else { cascadeNum--; forEach( doCascade, &r ); }
    /// }
    /// ```
    /// `doCount` leaves `cascadeNum == n` and `lastView` = the *last*-visited
    /// tileable child; the error check subtracts the **full** count `n`. Then
    /// `cascadeNum--` (→ `n-1`) and `doCascade` offsets each child's `a` by the
    /// running `cascadeNum`, so the first-visited (topmost) gets `+ (n-1)` and the
    /// last gets `+ 0`. `tileError()` no-op → leave bounds unchanged on the guard.
    fn cascade(&mut self, r: Rect) {
        let ids = self.group.tileable_ids(); // forEach order
        let n = ids.len() as i32; // doCount's cascadeNum
        if n == 0 {
            return;
        }
        let owner_size = self.group.state().size;
        // lastView = last tileable in forEach order; error check uses the full n.
        if let Some(&last_id) = ids.last() {
            let (min, _max) = self
                .group
                .child_mut(last_id)
                .expect("tileable id resolves")
                .size_limits(owner_size);
            if min.x > r.b.x - r.a.x - n || min.y > r.b.y - r.a.y - n {
                return; // tileError
            }
        }
        let mut cascade_num = n - 1; // C++ decrements once before doCascade
        for id in ids {
            if cascade_num >= 0 {
                let mut nr = r;
                nr.a.x += cascade_num;
                nr.a.y += cascade_num;
                if let Some(v) = self.group.child_mut(id) {
                    locate(v, nr, owner_size);
                }
                cascade_num -= 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use crate::view::{DrawCtx, Point, SelectMode};
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

    // -- tiling geometry (row 30) -------------------------------------------

    /// A visible, tileable window with number `n` at `bounds`.
    fn tileable_window(bounds: Rect, n: i16) -> Box<dyn View> {
        let mut w = Window::new(bounds, Some("W".into()), n);
        w.state_mut().options.tileable = true;
        Box::new(w)
    }

    /// Read a child's current bounds by id (same-module access to the group).
    fn child_bounds(desktop: &mut Desktop, id: ViewId) -> Rect {
        desktop.group.child_mut(id).unwrap().state().get_bounds()
    }

    /// `most_equal_divisors` swaps `(x, y)` on the `favor_y` flag. For a
    /// non-square `n` the two orientations differ, so this pins the otherwise
    /// uncovered `favor_y == false` branch (the `tile_columns_first == true` path).
    ///
    /// Hand-traced for `n = 6` (C++ `mostEqualDivisors`):
    /// `iSqr(6)` → res1=2, res2=3, `|2-3| == 1` (loop skipped), returns 2 → `i = 2`;
    /// `6 % 2 == 0` (no `+1`); `2 < 6/2 == 3` → `i = 3`. Then
    /// `favorY` → `x = n/i = 2, y = i = 3`; `!favorY` → `x = i = 3, y = n/i = 2`.
    #[test]
    fn most_equal_divisors_swaps_on_favor_y() {
        // favor_y == true (tile_columns_first == false): larger factor on y.
        assert_eq!(most_equal_divisors(6, true), (2, 3));
        // favor_y == false (tile_columns_first == true): larger factor on x — swapped.
        assert_eq!(most_equal_divisors(6, false), (3, 2));
    }

    /// Test 1 — tile lays N windows into `calc_tile_rect` cells, in forEach order.
    /// Bite: the topmost (last-inserted) window must take `tile_num = n-1`; an
    /// off-by-one or reversed order lands a window in the wrong cell.
    #[test]
    fn tile_lays_windows_into_calc_tile_cells() {
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24), |_| None);
        // Insert first→last; ids[0] in forEach order == the LAST inserted (topmost).
        let w0 = desktop.insert_view(tileable_window(Rect::new(1, 1, 20, 8), 1));
        let w1 = desktop.insert_view(tileable_window(Rect::new(2, 2, 21, 9), 2));
        let w2 = desktop.insert_view(tileable_window(Rect::new(3, 3, 22, 10), 3));

        desktop.tile(Rect::new(0, 0, 80, 24));

        // n=3, favor_y=true → num_cols=1, num_rows=3 → 3 stacked cells.
        // forEach order = [w2, w1, w0]; tile_num = 2,1,0.
        assert_eq!(
            child_bounds(&mut desktop, w2),
            Rect::new(0, 16, 80, 24),
            "topmost (last-inserted) window gets tile_num n-1 = 2 → bottom cell"
        );
        assert_eq!(
            child_bounds(&mut desktop, w1),
            Rect::new(0, 8, 80, 16),
            "middle window gets tile_num 1 → middle cell"
        );
        assert_eq!(
            child_bounds(&mut desktop, w0),
            Rect::new(0, 0, 80, 8),
            "first-inserted window gets tile_num 0 → top cell"
        );
    }

    /// Test 2 — non-tileable and invisible children are skipped; tileable still
    /// lay out. Bite: if the filter were dropped, the non-tileable/invisible window
    /// would move and/or n would be wrong, shifting the tileable cells.
    #[test]
    fn tile_skips_non_tileable_and_invisible() {
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24), |_| None);
        // A plain (non-tileable) window.
        let plain_bounds = Rect::new(1, 1, 30, 12);
        let plain = desktop.insert_view(Box::new(Window::new(plain_bounds, Some("P".into()), 1)));
        // An invisible-but-tileable window.
        let mut inv = Window::new(Rect::new(2, 2, 31, 13), Some("I".into()), 2);
        inv.state_mut().options.tileable = true;
        inv.state_mut().state.visible = false;
        let inv_bounds = inv.state().get_bounds();
        let inv_id = desktop.insert_view(Box::new(inv));
        // Two genuine tileable windows.
        let a = desktop.insert_view(tileable_window(Rect::new(3, 3, 20, 9), 3));
        let b = desktop.insert_view(tileable_window(Rect::new(4, 4, 21, 10), 4));

        desktop.tile(Rect::new(0, 0, 80, 24));

        // The non-tileable + invisible windows must NOT move.
        assert_eq!(
            child_bounds(&mut desktop, plain),
            plain_bounds,
            "non-tileable window untouched"
        );
        assert_eq!(
            child_bounds(&mut desktop, inv_id),
            inv_bounds,
            "invisible window untouched"
        );
        // n=2 → num_cols=1, num_rows=2 → 2 stacked cells. forEach [b, a]; tile_num 1,0.
        assert_eq!(
            child_bounds(&mut desktop, b),
            Rect::new(0, 12, 80, 24),
            "topmost tileable → tile_num 1 → bottom half"
        );
        assert_eq!(
            child_bounds(&mut desktop, a),
            Rect::new(0, 0, 80, 12),
            "other tileable → tile_num 0 → top half"
        );
    }

    /// Test 3 — tileError guard (a cell would be zero-width/height) leaves bounds
    /// unchanged. Bite: without the guard, `divider_loc`/`locate` would still run
    /// and (after the 16×6 clamp) move the windows.
    #[test]
    fn tile_error_guard_leaves_bounds_unchanged() {
        // Desktop 0,0,2,24: with n=3 → num_cols=1, num_rows=3, (2-0)/1=2 ok but
        // make it too narrow: width 0 is impossible; use a rect whose width/cols == 0.
        // n=2 → num_cols=1,num_rows=2; rect width 0 → (0)/1 == 0 → guard trips.
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24), |_| None);
        let a_bounds = Rect::new(1, 1, 20, 9);
        let b_bounds = Rect::new(2, 2, 21, 10);
        let a = desktop.insert_view(tileable_window(a_bounds, 1));
        let b = desktop.insert_view(tileable_window(b_bounds, 2));

        // Zero-width layout rect → (r.b.x - r.a.x)/num_cols == 0 → tileError no-op.
        desktop.tile(Rect::new(5, 0, 5, 24));

        assert_eq!(child_bounds(&mut desktop, a), a_bounds, "a unchanged");
        assert_eq!(child_bounds(&mut desktop, b), b_bounds, "b unchanged");
    }

    /// Test 4 — cascade offsets run `n-1 … 0`: topmost (last-inserted, `ids[0]`)
    /// gets `a == r.a + (n-1)`; the bottom (first-inserted) gets `a == r.a + 0`.
    /// Bite: this assertion flips if forEach order or the `n-1` start is wrong.
    #[test]
    fn cascade_offsets_run_n_minus_1_down_to_0() {
        let mut desktop = Desktop::new(Rect::new(0, 0, 80, 24), |_| None);
        let bottom = desktop.insert_view(tileable_window(Rect::new(1, 1, 40, 12), 1));
        let _mid = desktop.insert_view(tileable_window(Rect::new(2, 2, 41, 13), 2));
        let top = desktop.insert_view(tileable_window(Rect::new(3, 3, 42, 14), 3));

        let r = Rect::new(0, 0, 80, 24);
        desktop.cascade(r);

        // n=3 → offsets 2,1,0 in forEach order [top, mid, bottom].
        let top_b = child_bounds(&mut desktop, top);
        assert_eq!(top_b.a, Point::new(2, 2), "topmost gets r.a + (n-1) = +2");
        let bottom_b = child_bounds(&mut desktop, bottom);
        assert_eq!(bottom_b.a, Point::new(0, 0), "bottom gets r.a + 0");
    }

    /// Test 5 — cascade sub-minimum guard subtracts the FULL count `n` (not `n-1`).
    /// Bite: desktop width = min.x + n - 1 trips the correct `min.x > w - n` check
    /// (16 > 17-2) but would NOT trip a buggy `min.x > w - (n-1)` (16 > 17-1 false);
    /// windows must not move.
    #[test]
    fn cascade_sub_minimum_guard_uses_full_count() {
        // Window min is 16×6. n=2, desktop width = 16 + 2 - 1 = 17.
        let mut desktop = Desktop::new(Rect::new(0, 0, 17, 24), |_| None);
        let a_bounds = Rect::new(0, 0, 16, 6);
        let b_bounds = Rect::new(1, 1, 17, 7);
        let a = desktop.insert_view(tileable_window(a_bounds, 1));
        let b = desktop.insert_view(tileable_window(b_bounds, 2));

        // r width 17: min.x(16) > 17 - n(2) == 15 → guard trips → no-op.
        desktop.cascade(Rect::new(0, 0, 17, 24));

        assert_eq!(
            child_bounds(&mut desktop, a),
            a_bounds,
            "a unchanged (guard)"
        );
        assert_eq!(
            child_bounds(&mut desktop, b),
            b_bounds,
            "b unchanged (guard)"
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
