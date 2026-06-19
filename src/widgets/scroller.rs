//! The base for scrollable content views (the editor, the text terminal, the
//! outline viewer). A scroller references **two sibling scroll bars** that live
//! on the window frame, mirrors their `value` into its own
//! [`delta`](Scroller::delta) (the scroll offset its subclasses draw with), and
//! pushes range/value changes back to them.
//!
//! # The cross-view scrollbar broker — read **and** write
//!
//! A scroller both *reads* its two scroll bars (their `value`, arrow step) and
//! *mutates* them (set value/params, show/hide). But a leaf view holds only
//! `&mut Context` during dispatch — no tree access — and the bars are
//! window-frame siblings it cannot own, so it can neither read nor mutate them
//! directly.
//!
//! **The event loop is the broker, in both directions.** The scroller stores its
//! bars as [`Option<ViewId>`](crate::view::ViewId) handles and issues
//! [`Deferred`](crate::view::Deferred) ops naming them; the loop performs every
//! cross-view read/write at apply time. A
//! [`SCROLL_BAR_CHANGED`](crate::command::Command::SCROLL_BAR_CHANGED)
//! broadcast's `source` is the **filter** only (the scroller reacts iff
//! `source ∈ {h, v}`); the value is not stuffed into the message — the loop
//! resolves the subject bar and reads its `value`.
//!
//! # Selected color
//!
//! The base scroller only ever draws a fill in [`Role::ScrollerNormal`]. The
//! "selected" scroller color exists in the palette but is never used by the base;
//! the editor, which selects text, applies its own selection color, so no
//! `ScrollerSelected` role is wired here.
//!
//! # Resizing
//!
//! [`View::change_bounds`](crate::view::View::change_bounds) takes no
//! [`Context`], but re-publishing scroll-bar parameters after a resize needs one.
//! So `change_bounds` stays geometry-only, and [`Scroller::set_limit`] /
//! [`Scroller::scroll_to`] are the public `Context`-taking entries that
//! (re)publish the parameters; a window or editor that resizes a scroller calls
//! `set_limit` afterwards. There is no automatic re-emit on a bare bounds change,
//! because nothing currently drives that path.
//!
//! # Turbo Vision heritage
//!
//! Ports `TScroller` (`tscrolle.cpp`). Owner back-pointers to the sibling scroll
//! bars become [`ViewId`] handles brokered by the event loop, and the
//! palette becomes [`Role`]s. The original draw-re-entrancy guard is unneeded
//! under whole-tree redraw and is dropped.

use crate::theme::Role;
use crate::view::{Context, DrawCtx, Options, Point, Rect, StateFlag, View, ViewId, ViewState};

/// The base scrollable-content view.
///
/// Manages scroll state and keeps two sibling scroll bars in sync with its
/// content offset. The base draws only a uniform fill; subclasses (e.g. the
/// editor) override [`draw`](View::draw) and use [`delta`](Self::delta) to
/// shift their content. Wire a `Scroller` into a window by passing the scroll
/// bar [`ViewId`]s at construction time; call [`set_limit`](Self::set_limit)
/// whenever the content size changes, and [`scroll_to`](Self::scroll_to) to
/// jump to a position programmatically.
///
/// Cross-view scroll-bar reads and writes are brokered by the event loop — see
/// the module-level docs for the broker pattern.
///
/// # Turbo Vision heritage
///
/// Ports `TScroller` (`tscrolle.cpp`).
pub struct Scroller {
    /// View state (geometry, flags, etc.) — the composition target.
    state: ViewState,
    /// The current scroll offset, mirrored from the scroll bars.
    ///
    /// Updated by the event loop (via [`apply_delta`](Self::apply_delta))
    /// whenever a scroll bar's value changes. Subclasses read this in their
    /// [`draw`](View::draw) override to shift content: a line at logical row
    /// `r` is rendered at screen row `r - delta.y`, and similarly for `x`.
    /// Do not write `delta` directly; use [`scroll_to`](Self::scroll_to) to
    /// request a position change.
    pub delta: Point,
    /// Content extent `(x, y)`. Private; set via [`set_limit`](Self::set_limit).
    limit: Point,
    /// The horizontal scrollbar, by id (`None` if absent).
    h_scroll_bar: Option<ViewId>,
    /// The vertical scrollbar, by id (`None` if absent).
    v_scroll_bar: Option<ViewId>,
}

impl Scroller {
    /// Create a scroller with the given bounds and optional scroll bar handles.
    ///
    /// Pass the [`ViewId`]s of the horizontal and vertical scroll bars, or
    /// `None` for either axis you don't need. The scroller starts at offset
    /// `(0, 0)` with an empty content extent; call
    /// [`set_limit`](Self::set_limit) after construction to publish the initial
    /// range to the bars. The view is marked selectable so it can receive
    /// keyboard focus.
    ///
    /// Broadcasts are delivered unconditionally — there is no `broadcast` event
    /// mask bit — so no extra setup is needed to receive
    /// [`SCROLL_BAR_CHANGED`](crate::command::Command::SCROLL_BAR_CHANGED).
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

    /// Return the current content extent as set by [`set_limit`](Self::set_limit).
    ///
    /// The returned point `(x, y)` is the logical width and height of the
    /// scrollable content. Use this in a subclass `draw` implementation if you
    /// need to know the total content size independently of [`delta`](Self::delta).
    /// To change the extent, call [`set_limit`](Self::set_limit) — it both
    /// stores the value and re-publishes the range to the scroll bars.
    pub fn limit(&self) -> Point {
        self.limit
    }

    /// Return the [`ViewId`] of the horizontal scroll bar, if one was supplied.
    ///
    /// Use this in a subclass when you need to address the scroll bar directly
    /// (e.g. to call [`scroll_to`](Self::scroll_to) or query the bar's
    /// [`ViewId`] for a broker operation). Returns `None` if the scroller was
    /// created without a horizontal bar.
    ///
    /// # Turbo Vision heritage
    ///
    /// Corresponds to the public `TScroller::hScrollBar` pointer. In Rust the
    /// raw pointer becomes a [`ViewId`] handle; the event loop brokers all
    /// cross-view reads and writes through it rather than allowing direct
    /// pointer access.
    pub fn h_scroll_bar(&self) -> Option<ViewId> {
        self.h_scroll_bar
    }

    /// Return the [`ViewId`] of the vertical scroll bar, if one was supplied.
    ///
    /// Use this in a subclass when you need to address the scroll bar directly.
    /// Returns `None` if the scroller was created without a vertical bar.
    ///
    /// # Turbo Vision heritage
    ///
    /// Corresponds to the public `TScroller::vScrollBar` pointer. See
    /// [`h_scroll_bar`](Self::h_scroll_bar) for the broker rationale.
    pub fn v_scroll_bar(&self) -> Option<ViewId> {
        self.v_scroll_bar
    }

    /// Apply a freshly-resolved scroll offset from the event loop's read broker.
    ///
    /// This is the Rust analog of C++ `TScroller::scrollDraw`. It is called
    /// by the pump after it resolves both scroll bars and reads their current
    /// `value`s, assembling them into `d`. If `d` differs from the stored
    /// [`delta`](Self::delta), the cursor is shifted by `old_delta - d` (using
    /// the **old** value — order matters) and then `delta` is overwritten.
    /// The whole-tree redraw that follows the pump tick replaces the C++
    /// `drawView` call.
    ///
    /// **Do not call this directly.** It is public so the pump can reach it via
    /// the `as_any_mut` downcast; call [`scroll_to`](Self::scroll_to) to
    /// request a position change from user code.
    ///
    /// # Turbo Vision heritage
    ///
    /// Ports `TScroller::scrollDraw`. The original read the bar values directly
    /// via raw pointers; here the pump reads them and passes the result in `d`.
    pub fn apply_delta(&mut self, d: Point) {
        if d != self.delta {
            // Shift the cursor using the OLD delta, before overwriting.
            let new_cursor = self.state.cursor + (self.delta - d);
            self.state.cursor = new_cursor;
            self.delta = d;
        }
    }

    /// Set the content extent to `(x, y)` and re-publish each bar's range and
    /// page parameters.
    ///
    /// Call this after construction and after any resize to tell the scroll bars
    /// the total content dimensions. For the horizontal bar: `min = 0`,
    /// `max = x - view_width`, `page_step = view_width - 1`; the vertical bar
    /// mirrors on `y` and `view_height`. The current bar `value` and arrow step
    /// are preserved. Cross-view writes are queued as deferred
    /// [`ScrollBarSetParams`](crate::view::Deferred::ScrollBarSetParams) ops
    /// executed by the pump — no direct bar access occurs.
    ///
    /// [`on_bounds_changed`](Self::on_bounds_changed) calls `set_limit` automatically
    /// after a resize with the previously stored extent, so you only need to call
    /// it explicitly when the *content* size changes.
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

    /// Request that the scroll bars move to position `(x, y)`, preserving range
    /// and steps.
    ///
    /// Queues a deferred
    /// [`ScrollBarSetParams`](crate::view::Deferred::ScrollBarSetParams) op for
    /// each present bar with only `value` updated (the range, page step, and
    /// arrow step are left unchanged). The bar clamps the value to its live
    /// range at apply time. The resulting
    /// [`SCROLL_BAR_CHANGED`](crate::command::Command::SCROLL_BAR_CHANGED)
    /// broadcast will then trigger the read broker, which calls
    /// [`apply_delta`](Self::apply_delta) to update [`delta`](Self::delta).
    ///
    /// Call this to programmatically jump to a position, e.g. "go to line N" in
    /// an editor. If you only need to update the bar's range (e.g. after loading
    /// new content), use [`set_limit`](Self::set_limit) instead.
    pub fn scroll_to(&mut self, x: i32, y: i32, ctx: &mut Context) {
        if let Some(h) = self.h_scroll_bar {
            ctx.request_scroll_bar_params(h, Some(x), None, None, None, None);
        }
        if let Some(v) = self.v_scroll_bar {
            ctx.request_scroll_bar_params(v, Some(y), None, None, None, None);
        }
    }

    /// Show or hide one scrollbar based on this scroller's active/selected state:
    /// shown when either is set, hidden otherwise. Realized as a deferred
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

    /// A uniform fill of the entire view rect with [`Role::ScrollerNormal`] (no
    /// active/selected branch). The "selected" scroller color is never used by
    /// the base; the editor applies its own selection color instead.
    ///
    /// Subclasses (the editor) override `draw` and consume [`delta`](Self::delta).
    fn draw(&mut self, ctx: &mut DrawCtx) {
        let style = ctx.style(Role::ScrollerNormal);
        let extent = self.state.get_extent();
        ctx.fill(extent, ' ', style);
    }

    /// React to a [`SCROLL_BAR_CHANGED`](crate::command::Command::SCROLL_BAR_CHANGED)
    /// broadcast whose `source` is one of this scroller's two bars by requesting a
    /// deferred delta-sync (the read broker; see the module docs).
    ///
    /// The `source ∈ {h_id, v_id}` test is the filter that decides whether this
    /// scroller owns the firing bar.
    fn handle_event(&mut self, ev: &mut crate::event::Event, ctx: &mut Context) {
        if let crate::event::Event::Broadcast { command, source } = *ev
            && command == crate::command::Command::SCROLL_BAR_CHANGED
            && source.is_some()
            && (source == self.h_scroll_bar || source == self.v_scroll_bar)
        {
            // The scroller must itself be inserted (have an id) to be addressable.
            if let Some(scroller) = self.state.id() {
                ctx.request_scroll_sync(scroller, self.h_scroll_bar, self.v_scroll_bar);
            }
        }
    }

    /// Called by the pump after it reads the sibling scrollbar values. Applies the
    /// resulting delta to this scroller. A missing bar (`None`) is treated as
    /// delta 0 — faithful to the previous `.unwrap_or(0)` read in the old pump arm.
    fn apply_scroll_sync(&mut self, h: Option<i32>, v: Option<i32>, _ctx: &mut Context) {
        self.apply_delta(Point::new(h.unwrap_or(0), v.unwrap_or(0)));
    }

    /// Flip a state flag and show or hide the scroll bars when focus changes.
    ///
    /// After updating the flag, if `flag` is [`Active`](StateFlag::Active) or
    /// [`Selected`](StateFlag::Selected), each scroll bar is shown when
    /// *either* flag is set and hidden when both are clear. This matches the
    /// C++ behaviour: bars are visible whenever the scroller (or one of its
    /// ancestors) has input focus. Visibility is applied via a deferred
    /// [`SetVisible`](crate::view::Deferred::SetVisible) op brokered by the pump,
    /// not by a direct call into the bar.
    ///
    /// When `flag` is [`Focused`](StateFlag::Focused), the standard
    /// [`RECEIVED_FOCUS`](crate::command::Command::RECEIVED_FOCUS) /
    /// [`RELEASED_FOCUS`](crate::command::Command::RELEASED_FOCUS) broadcast is
    /// emitted; no bar visibility change occurs.
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

    /// Re-publish scroll bar range parameters after a bounds change.
    ///
    /// Called by the pump after it has applied a `Deferred::ChangeBounds` op,
    /// so `self.size` already reflects the new dimensions. Calls
    /// [`set_limit`](Self::set_limit) with the previously stored `limit` values
    /// so the bars' `max` and `page_step` are recomputed for the new view size.
    ///
    /// Subclasses that override this should call `super.on_bounds_changed(ctx)`
    /// (or call `set_limit` themselves) to keep the bars consistent.
    fn on_bounds_changed(&mut self, ctx: &mut Context) {
        let (x, y) = (self.limit.x, self.limit.y);
        self.set_limit(x, y, ctx);
    }

    /// Concrete-reach hatch (the sanctioned downcast): retained as a generic
    /// escape so `Terminal` can delegate `as_any_mut` to its inner `Scroller`.
    /// The `Deferred::ScrollSync` arm does NOT use this hatch — it dispatches
    /// `apply_scroll_sync` to the `Scroller` override through the `View` trait
    /// (delegate forwarder), not via `as_any_mut`.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
//
// The cross-view *broker* (pump-side apply of ScrollSync /
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
        // NOTE: opting into the broadcast class has no analogue here —
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

        // (a) Broadcast sourced by the H bar → ScrollSync queued.
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
            Deferred::ScrollSync { target, h: rh, v: rv }
                if target == scroller_id && rh == Some(h) && rv == Some(v)
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
