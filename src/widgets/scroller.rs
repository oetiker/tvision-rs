//! `TScroller` — faithful Rust port of `tscrolle.cpp` (row 27, FOUNDATION).
//!
//! `TScroller` is the base class for scrollable content views (`TEditor` row 66,
//! `TTextDevice`/`TTerminal`, `TOutlineViewer`). It references **two sibling
//! `TScrollBar`s** that live on the window frame, mirrors their `value` into its
//! own [`delta`](Scroller::delta) (the scroll offset its subclasses draw with),
//! and pushes range/value changes back to them.
//!
//! ## The cross-view scrollbar broker (D3) — read **and** write
//!
//! In C++ the scroller holds raw pointers to its two scrollbars and both *reads*
//! their fields (`->value`, `->arStep`) and *mutates* them (`setValue`/
//! `setParams`/`show`/`hide`). Under D3 a leaf view holds only `&mut Context`
//! during dispatch — no tree access, so it can neither read nor mutate a sibling.
//! The scrollbars are window-frame siblings, so the scroller cannot own them.
//!
//! **The pump is the broker, in both directions.** The scroller stores its
//! scrollbars as [`Option<ViewId>`](crate::view::ViewId) handles and issues
//! [`Deferred`](crate::view::Deferred) ops naming them; the pump performs every
//! cross-view read/write at deferred-apply time via `group.find_mut(id)` (see the
//! `SyncScrollerDelta` / `ScrollBarSetParams` / `SetVisible` apply arms in
//! `program.rs`). The `cmScrollBarChanged` broadcast's `source` is the **filter**
//! only (the scroller reacts iff `source ∈ {h_id, v_id}`) — this is the first real
//! consumer of `Broadcast{source}`. The value is **not** stuffed into the message;
//! the pump resolves the subject bar and reads its `value` (the faithful successor
//! to C++ reading `->value` off the `infoPtr` subject).
//!
//! ## D8: `drawLock`/`drawFlag`/`checkDraw` dropped
//!
//! The C++ `drawLock`/`drawFlag`/`checkDraw` are a synchronous re-entrancy guard
//! around C++'s immediate `drawView()`. Under D8 (whole-tree redraw + diff every
//! pass) there is no immediate draw to guard, and our mutations are **deferred**
//! (applied in one post-dispatch drain), so the batching the lock provided is
//! structural. Dropped entirely, like `buffered`/the lock elsewhere.
//!
//! ## Other deviations / drops
//!
//! - **D3:** scrollbars referenced by `ViewId`, not pointers; all cross-view
//!   read/write brokered by the pump.
//! - **D12/D2:** `shutDown`/`write`/`read`/streaming dropped.
//! - **getPalette → Theme roles** (D7): [`Role::ScrollerNormal`]
//!   (`cpScroller` idx 1); `ScrollerSelected` (idx 2) deferred to
//!   `TEditor` row 66.
//! - **`change_bounds` trait signature:** the base `View::change_bounds(bounds)`
//!   takes no `Context`, but re-publishing scrollbar params on resize needs one.
//!   Rather than widen the trait (a FOUNDATION decision with one consumer), this
//!   row keeps `change_bounds` geometry-only (the base default sets bounds) and
//!   exposes [`Scroller::set_limit`]/[`Scroller::scroll_to`] as the public
//!   `Context`-taking entries that (re)publish scrollbar params. A window/editor
//!   consumer that resizes a scroller will call `set_limit(ctx)` afterwards.
//!   **TODO(resize):** re-emit limit params automatically when a window consumer
//!   wires the resize path.

use crate::theme::Role;
use crate::view::{Context, DrawCtx, Options, Point, Rect, StateFlag, View, ViewId, ViewState};

/// `TScroller` — the base scrollable-content view (D2 View trait + ViewState).
///
/// The base draws only a fill (it is an abstract base in C++; subclasses such as
/// the editor override `draw` and consume [`delta`](Self::delta)). It references
/// two sibling scrollbars by [`ViewId`] and brokers all reads/writes through the
/// pump (see the module docs).
pub struct Scroller {
    /// View state (geometry, flags, etc.) — the D2 composition target.
    state: ViewState,
    /// Scroll offset — the value mirrored from the scrollbars. Subclasses (the
    /// editor) draw their content shifted by this. Public so subclasses read it.
    pub delta: Point,
    /// Content extent `(x, y)` — TV's `limit`. Set via [`set_limit`](Self::set_limit).
    limit: Point,
    /// The horizontal scrollbar, by id (`None` if absent). TV's `hScrollBar`.
    h_scroll_bar: Option<ViewId>,
    /// The vertical scrollbar, by id (`None` if absent). TV's `vScrollBar`.
    v_scroll_bar: Option<ViewId>,
}

impl Scroller {
    /// Construct a scroller from `bounds` and its two scrollbars (by id, `None` if
    /// absent).
    ///
    /// Faithful to the C++ ctor: `delta = limit = (0, 0)`, `options |=
    /// ofSelectable`. The C++ `eventMask |= evBroadcast` has **no analogue** — under
    /// D4 broadcasts are delivered unconditionally (there is no `broadcast` bit in
    /// [`EventMask`](crate::event::EventMask)), so the scroller already receives them.
    pub fn new(bounds: Rect, h_scroll_bar: Option<ViewId>, v_scroll_bar: Option<ViewId>) -> Self {
        let mut state = ViewState::new(bounds);
        state.options = Options {
            selectable: true,
            ..Default::default()
        };
        Scroller {
            state,
            delta: Point::new(0, 0),
            limit: Point::new(0, 0),
            h_scroll_bar,
            v_scroll_bar,
        }
    }

    /// The content extent (`limit`), as set by [`set_limit`](Self::set_limit).
    pub fn limit(&self) -> Point {
        self.limit
    }

    /// The horizontal scrollbar id (test/subclass hook).
    pub fn h_scroll_bar(&self) -> Option<ViewId> {
        self.h_scroll_bar
    }

    /// The vertical scrollbar id (test/subclass hook).
    pub fn v_scroll_bar(&self) -> Option<ViewId> {
        self.v_scroll_bar
    }

    /// Apply a freshly-read scrollbar delta — the body of `TScroller::scrollDraw`.
    ///
    /// Called by the pump (the read broker) after it resolves the bars and reads
    /// their `value`s. Faithful: if `d != delta`, shift the cursor by the **old**
    /// `delta - d` (`setCursor(cursor + delta - d)`), then overwrite `delta = d`.
    /// The order matters — the cursor adjust must use the old `delta`. No draw call
    /// (D8: whole-tree redraw).
    pub fn apply_delta(&mut self, d: Point) {
        if d != self.delta {
            // setCursor( cursor + delta - d ) — uses the OLD delta, before overwrite.
            let new_cursor = self.state.cursor + (self.delta - d);
            self.state.cursor = new_cursor;
            self.delta = d;
        }
    }

    /// `TScroller::setLimit` — set the content extent and (re)publish each bar's
    /// range/page params.
    ///
    /// Faithful to the C++: `limit = (x, y)`; for the H bar push
    /// `setParams(value, 0, x - size.x, size.x - 1, arStep)` — i.e. preserve
    /// `value` and `arStep` (`None`), set `min = 0`, `max = x - size.x`,
    /// `page_step = size.x - 1`. The V bar mirrors on `y`/`size.y`. The
    /// `drawLock`/`drawFlag`/`drawView` are dropped (D8); the cross-view writes are
    /// **deferred** ([`Context::request_scroll_bar_params`]).
    pub fn set_limit(&mut self, x: i32, y: i32, ctx: &mut Context) {
        self.limit = Point::new(x, y);
        let size = self.state.size;
        if let Some(h) = self.h_scroll_bar {
            ctx.request_scroll_bar_params(
                h,
                None,             // preserve value
                Some(0),          // min
                Some(x - size.x), // max
                Some(size.x - 1), // page_step
                None,             // preserve arrow_step
            );
        }
        if let Some(v) = self.v_scroll_bar {
            ctx.request_scroll_bar_params(
                v,
                None,
                Some(0),
                Some(y - size.y),
                Some(size.y - 1),
                None,
            );
        }
    }

    /// `TScroller::scrollTo` — set each bar's value (`setValue`), preserving range
    /// and steps.
    ///
    /// Faithful to the C++: H bar → `setValue(x)`, V bar → `setValue(y)`. Realized
    /// as a deferred [`ScrollBarSetParams`](crate::view::Deferred::ScrollBarSetParams)
    /// with only `value` set (the rest preserved). `set_params` clamps to the live
    /// range. The `drawLock`/`checkDraw` are dropped (D8).
    pub fn scroll_to(&mut self, x: i32, y: i32, ctx: &mut Context) {
        if let Some(h) = self.h_scroll_bar {
            ctx.request_scroll_bar_params(h, Some(x), None, None, None, None);
        }
        if let Some(v) = self.v_scroll_bar {
            ctx.request_scroll_bar_params(v, Some(y), None, None, None, None);
        }
    }

    /// `TScroller::showSBar` — show or hide one scrollbar based on this scroller's
    /// active/selected state. Faithful: `getState(sfActive | sfSelected) != 0`
    /// (either bit) → show, else hide. Realized as a deferred
    /// [`SetVisible`](crate::view::Deferred::SetVisible).
    fn show_sbar(&self, sbar: Option<ViewId>, ctx: &mut Context) {
        if let Some(id) = sbar {
            let visible = self.state.state.active || self.state.state.selected;
            ctx.request_set_visible(id, visible);
        }
    }
}

impl View for Scroller {
    fn state(&self) -> &ViewState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }

    /// Inherits `TView::draw` — a uniform fill: `moveChar(0, ' ', getColor(1),
    /// size.x)` for each row, palette index 1 only, no active/selected branch.
    /// Fills the entire view rect with [`Role::ScrollerNormal`].
    ///
    /// `Role::ScrollerSelected` (`cpScroller` idx 2) is deferred to the first
    /// subclass that draws with it (`TEditor`, row 66).
    ///
    /// Subclasses (the editor) override `draw` and consume [`delta`](Self::delta).
    fn draw(&mut self, ctx: &mut DrawCtx) {
        let style = ctx.style(Role::ScrollerNormal);
        let extent = self.state.get_extent();
        ctx.fill(extent, ' ', style);
    }

    /// `TScroller::handleEvent` — react to a `cmScrollBarChanged` broadcast whose
    /// `source` is one of this scroller's two bars by requesting a deferred
    /// delta-sync (the read broker; see the module docs).
    ///
    /// The C++ `TView::handleEvent(event)` super-call is a no-op in our trait, so it
    /// is omitted (matches every other widget). The `infoPtr == hScrollBar || ==
    /// vScrollBar` guard becomes the `source ∈ {h_id, v_id}` filter.
    fn handle_event(&mut self, ev: &mut crate::event::Event, ctx: &mut Context) {
        if let crate::event::Event::Broadcast { command, source } = *ev
            && command == crate::command::Command::SCROLL_BAR_CHANGED
            && source.is_some()
            && (source == self.h_scroll_bar || source == self.v_scroll_bar)
        {
            // The scroller must itself be inserted (have an id) to be addressable.
            if let Some(scroller) = self.state.id() {
                ctx.request_sync_scroller_delta(scroller, self.h_scroll_bar, self.v_scroll_bar);
            }
        }
    }

    /// `TScroller::setState` — after the base flips the flag, when `flag` is
    /// `Active` or `Selected`, show/hide both bars per the new active/selected
    /// state. Reads the **post-update** bits from `self` (the base already flipped
    /// them).
    fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
        // Base: flip the flag (+ the Focused broadcast).
        self.state_mut().set_flag(flag, enable);
        if flag == StateFlag::Focused {
            let source = self.state.id();
            ctx.broadcast(
                if enable {
                    crate::command::Command::RECEIVED_FOCUS
                } else {
                    crate::command::Command::RELEASED_FOCUS
                },
                source,
            );
        }
        if flag == StateFlag::Active || flag == StateFlag::Selected {
            self.show_sbar(self.h_scroll_bar, ctx);
            self.show_sbar(self.v_scroll_bar, ctx);
        }
    }

    /// `TScroller::changeBounds` — after the pump applies new bounds via
    /// `Deferred::ChangeBounds`, re-publish scrollbar range/page params with the
    /// stored `limit` and the **new** `size` (faithful: `setLimit(limit.x, limit.y)`
    /// after `setBounds`, tscrolle.cpp changeBounds).
    fn on_bounds_changed(&mut self, ctx: &mut Context) {
        let (x, y) = (self.limit.x, self.limit.y);
        self.set_limit(x, y, ctx);
    }

    /// Concrete-reach hatch (the sanctioned downcast, same as `TWindow::zoom`): the
    /// pump downcasts to `&mut Scroller` to call [`apply_delta`](Self::apply_delta)
    /// when applying a `Deferred::SyncScrollerDelta`.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
//
// The cross-view *broker* (pump-side apply of SyncScrollerDelta /
// ScrollBarSetParams / SetVisible) is tested end-to-end through `Program::pump_once`
// in `src/app/program.rs` (it needs the pump's `group.find_mut`). Here we test the
// scroller-side pieces directly: the ctor, `apply_delta` (the cursor-adjust order),
// the request methods (which `Deferred` they queue), the `source` filter in
// `handle_event`, `set_state`'s show/hide, and the base fill draw.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::command::Command;
    use crate::event::Event;
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use crate::view::{Deferred, Group, ViewId};
    use std::collections::VecDeque;

    fn make_ctx<'a>(
        out: &'a mut VecDeque<Event>,
        timers: &'a mut crate::timer::TimerQueue,
        deferred: &'a mut Vec<Deferred>,
    ) -> Context<'a> {
        Context::new(out, timers, 0, deferred)
    }

    /// A fake `ViewId` for filter tests: insert a throwaway view into a group and
    /// read its stamped id. (Ids are only minted at `Group::insert`.)
    fn mint_id() -> (Group, ViewId) {
        let mut g = Group::new(Rect::new(0, 0, 4, 4));
        let id = g.insert(Box::new(Scroller::new(Rect::new(0, 0, 1, 1), None, None)));
        (g, id)
    }

    // -- 1. ctor -------------------------------------------------------------

    #[test]
    fn ctor_sets_selectable_and_zero_delta_limit() {
        let s = Scroller::new(Rect::new(0, 0, 10, 5), None, None);
        assert!(s.state.options.selectable, "ofSelectable set");
        assert_eq!(s.delta, Point::new(0, 0));
        assert_eq!(s.limit(), Point::new(0, 0));
        // NOTE: the C++ `eventMask |= evBroadcast` has no analogue under D4 —
        // broadcasts are delivered unconditionally (no `broadcast` bit in
        // EventMask), so there is nothing to assert about the mask here.
        assert_eq!(s.state.event_mask, crate::event::EventMask::default());
    }

    #[test]
    fn ctor_records_scroll_bar_ids() {
        let (_g, h) = mint_id();
        let (_g2, v) = mint_id();
        let s = Scroller::new(Rect::new(0, 0, 10, 5), Some(h), Some(v));
        assert_eq!(s.h_scroll_bar(), Some(h));
        assert_eq!(s.v_scroll_bar(), Some(v));
    }

    // -- 6. cursor adjust (apply_delta order) --------------------------------

    /// The cursor must be shifted by the **old** `delta - d`, then `delta`
    /// overwritten. Setup: cursor=(5,3), delta=(2,1); apply d=(4,0) →
    /// cursor = (5,3) + ((2,1) - (4,0)) = (5-2, 3+1) = (3, 4); delta = (4,0).
    #[test]
    fn apply_delta_shifts_cursor_by_old_delta_minus_new() {
        let mut s = Scroller::new(Rect::new(0, 0, 10, 5), None, None);
        s.state.cursor = Point::new(5, 3);
        s.delta = Point::new(2, 1);
        s.apply_delta(Point::new(4, 0));
        assert_eq!(s.delta, Point::new(4, 0), "delta overwritten with d");
        assert_eq!(
            s.state.cursor,
            Point::new(3, 4),
            "cursor shifted by OLD delta - d = (2-4, 1-0) = (-2, +1)"
        );
    }

    #[test]
    fn apply_delta_noop_when_unchanged() {
        let mut s = Scroller::new(Rect::new(0, 0, 10, 5), None, None);
        s.state.cursor = Point::new(5, 3);
        s.delta = Point::new(2, 1);
        s.apply_delta(Point::new(2, 1)); // same delta
        assert_eq!(s.state.cursor, Point::new(5, 3), "cursor untouched");
        assert_eq!(s.delta, Point::new(2, 1));
    }

    // -- 3. set_limit (write broker — queues the right Deferred) -------------

    #[test]
    fn set_limit_queues_params_preserving_value_and_arrow_step() {
        let (_g, h) = mint_id();
        let (_g2, v) = mint_id();
        // 10×5 scroller (size.x=10, size.y=5).
        let mut s = Scroller::new(Rect::new(0, 0, 10, 5), Some(h), Some(v));

        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            s.set_limit(100, 50, &mut ctx);
        }
        assert_eq!(s.limit(), Point::new(100, 50));
        assert_eq!(deferred.len(), 2, "one param op per bar");

        // H bar: min=0, max=100-10=90, page_step=10-1=9, value/arrow preserved (None).
        match deferred[0] {
            Deferred::ScrollBarSetParams {
                id,
                value,
                min,
                max,
                page_step,
                arrow_step,
            } => {
                assert_eq!(id, h);
                assert_eq!(value, None, "value preserved");
                assert_eq!(min, Some(0));
                assert_eq!(max, Some(90), "max = x - size.x");
                assert_eq!(page_step, Some(9), "page_step = size.x - 1");
                assert_eq!(arrow_step, None, "arrow_step preserved");
            }
            _ => panic!("expected H ScrollBarSetParams"),
        }
        // V bar: min=0, max=50-5=45, page_step=5-1=4.
        match deferred[1] {
            Deferred::ScrollBarSetParams {
                id, max, page_step, ..
            } => {
                assert_eq!(id, v);
                assert_eq!(max, Some(45), "max = y - size.y");
                assert_eq!(page_step, Some(4), "page_step = size.y - 1");
            }
            _ => panic!("expected V ScrollBarSetParams"),
        }
    }

    #[test]
    fn set_limit_with_no_bars_queues_nothing() {
        let mut s = Scroller::new(Rect::new(0, 0, 10, 5), None, None);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            s.set_limit(100, 50, &mut ctx);
        }
        assert_eq!(s.limit(), Point::new(100, 50));
        assert!(deferred.is_empty(), "no bars → no param ops");
    }

    // -- 4. scroll_to (write broker — value only) ----------------------------

    #[test]
    fn scroll_to_queues_value_only() {
        let (_g, h) = mint_id();
        let (_g2, v) = mint_id();
        let mut s = Scroller::new(Rect::new(0, 0, 10, 5), Some(h), Some(v));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            s.scroll_to(10, 5, &mut ctx);
        }
        assert_eq!(deferred.len(), 2);
        match deferred[0] {
            Deferred::ScrollBarSetParams {
                id,
                value,
                min,
                max,
                page_step,
                arrow_step,
            } => {
                assert_eq!(id, h);
                assert_eq!(value, Some(10), "H value set");
                assert!(
                    min.is_none() && max.is_none() && page_step.is_none() && arrow_step.is_none(),
                    "everything but value preserved"
                );
            }
            _ => panic!("expected H value op"),
        }
        match deferred[1] {
            Deferred::ScrollBarSetParams { id, value, .. } => {
                assert_eq!(id, v);
                assert_eq!(value, Some(5), "V value set");
            }
            _ => panic!("expected V value op"),
        }
    }

    // -- 5. set_state / show_sbar --------------------------------------------

    #[test]
    fn set_state_select_shows_bars_deselect_hides() {
        let (_g, h) = mint_id();
        let (_g2, v) = mint_id();
        let mut s = Scroller::new(Rect::new(0, 0, 10, 5), Some(h), Some(v));

        // Select → both bars requested visible.
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            s.set_state(StateFlag::Selected, true, &mut ctx);
        }
        assert!(s.state.state.selected);
        assert_eq!(deferred.len(), 2);
        assert!(matches!(deferred[0], Deferred::SetVisible(id, true) if id == h));
        assert!(matches!(deferred[1], Deferred::SetVisible(id, true) if id == v));

        // Deselect → both bars requested hidden (active still false).
        deferred.clear();
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            s.set_state(StateFlag::Selected, false, &mut ctx);
        }
        assert!(!s.state.state.selected);
        assert!(matches!(deferred[0], Deferred::SetVisible(id, false) if id == h));
        assert!(matches!(deferred[1], Deferred::SetVisible(id, false) if id == v));
    }

    #[test]
    fn set_state_active_keeps_bars_visible_even_when_not_selected() {
        // showSBar uses (sfActive | sfSelected): active alone keeps them shown.
        let (_g, h) = mint_id();
        let mut s = Scroller::new(Rect::new(0, 0, 10, 5), Some(h), None);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            s.set_state(StateFlag::Active, true, &mut ctx);
        }
        assert!(s.state.state.active);
        assert!(
            matches!(deferred[0], Deferred::SetVisible(id, true) if id == h),
            "active (not selected) still shows the bar"
        );
    }

    #[test]
    fn set_state_non_active_selected_flag_does_not_touch_bars() {
        let (_g, h) = mint_id();
        let mut s = Scroller::new(Rect::new(0, 0, 10, 5), Some(h), None);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            // Focused triggers a broadcast but NOT showSBar.
            s.set_state(StateFlag::Focused, true, &mut ctx);
        }
        assert!(
            !deferred
                .iter()
                .any(|d| matches!(d, Deferred::SetVisible(..))),
            "Focused must not show/hide bars"
        );
    }

    // -- 2. handle_event source filter (read broker request side) ------------

    #[test]
    fn handle_event_requests_sync_only_for_own_bars() {
        // The scroller must be inserted to have an id (and to be addressable).
        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        let (_gh, h) = mint_id();
        let (_gv, v) = mint_id();
        let scroller_id = group.insert(Box::new(Scroller::new(
            Rect::new(0, 0, 10, 5),
            Some(h),
            Some(v),
        )));

        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        // (a) Broadcast sourced by the H bar → SyncScrollerDelta queued.
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            let mut ev = Event::Broadcast {
                command: Command::SCROLL_BAR_CHANGED,
                source: Some(h),
            };
            group
                .find_mut(scroller_id)
                .unwrap()
                .handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(deferred.len(), 1, "own-bar broadcast → one sync request");
        assert!(matches!(
            deferred[0],
            Deferred::SyncScrollerDelta { scroller, h: rh, v: rv }
                if scroller == scroller_id && rh == Some(h) && rv == Some(v)
        ));

        // (b) Broadcast sourced by an UNRELATED view (the scroller itself — a real
        //     id that is neither bar) → filter bites, nothing queued.
        deferred.clear();
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            let mut ev = Event::Broadcast {
                command: Command::SCROLL_BAR_CHANGED,
                source: Some(scroller_id),
            };
            group
                .find_mut(scroller_id)
                .unwrap()
                .handle_event(&mut ev, &mut ctx);
        }
        assert!(
            deferred.is_empty(),
            "broadcast from a non-bar source must be ignored (the source filter bites)"
        );

        // (c) A different command from the H bar → also ignored.
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            let mut ev = Event::Broadcast {
                command: Command::SCROLL_BAR_CLICKED,
                source: Some(h),
            };
            group
                .find_mut(scroller_id)
                .unwrap()
                .handle_event(&mut ev, &mut ctx);
        }
        assert!(
            deferred.is_empty(),
            "only cmScrollBarChanged triggers a sync"
        );
    }

    // -- 7. trivial snapshot -------------------------------------------------

    #[test]
    fn snapshot_base_scroller_fill() {
        let theme = Theme::classic_blue();
        let mut s = Scroller::new(Rect::new(0, 0, 8, 4), None, None);

        let (backend, screen) = HeadlessBackend::new(8, 4);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = s.state.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            s.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }
}
