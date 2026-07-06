//! The abstract base for every list widget ([`ListBox`](crate::widgets::ListBox),
//! history viewers, color/file lists). It lays out `range` items in `num_cols`
//! columns, tracks a `focused` item and a `top_item` scroll offset, and drives
//! **two sibling scroll bars** that live on the window frame.
//!
//! # A trait, not a concrete struct
//!
//! A list box reuses the base `draw` (it does **not** override it) and overrides
//! `get_text`/`is_selected`. A concrete-struct-embedded base physically cannot
//! dispatch from the base's own `draw` back into the embedder's `get_text`. So
//! the abstract base is modeled as a **trait** (the same pattern as
//! [`Validator`](crate::validate::Validator)): [`ListViewer`] carries the
//! overridable methods, [`ListViewerState`] carries the data members, and the
//! shared draw/event/nav logic lives as **free functions generic over
//! `<L: ListViewer + ?Sized>`** so a concrete widget's `View` impl reuses them
//! verbatim while they call back into `get_text`/`is_selected`/`select_item`.
//!
//! [`ListViewer`] is intentionally **not object-safe** (`get_text -> String`);
//! that is fine — concrete widgets are stored as `Box<dyn View>`, and
//! `ListViewer` is only ever a generic bound behind a concrete type.
//!
//! # The cross-view scrollbar read-sync
//!
//! A list viewer holds only `&mut Context` during dispatch and so can neither
//! **read** nor **mutate** its window-frame sibling scroll bars. The pump is the
//! broker: the list stores its bars as [`Option<ViewId>`] handles and a cached
//! [`indent`](ListViewerState::indent) (the live horizontal-bar `value` the draw
//! needs, refreshed by the read-sync). On a
//! [`SCROLL_BAR_CHANGED`](crate::command::Command::SCROLL_BAR_CHANGED) broadcast
//! naming one of its bars as `source`, the list requests
//! [`Deferred::ScrollSync`](crate::view::Deferred::ScrollSync); the pump
//! reads both bars' `value`s and calls back through
//! [`View::apply_scroll_sync`](crate::view::View::apply_scroll_sync) →
//! [`apply_scroll`].
//!
//! ## Termination
//!
//! Unlike a plain scroller, this read-sync **writes back**: `apply_scroll`'s
//! vertical-bar branch runs `focus_item_num` → [`focus_item`] → a vertical-bar
//! `set_value(focused)`. That would re-broadcast the changed signal and
//! re-enter the sync — except
//! [`ScrollBar::set_params`](crate::widgets::ScrollBar::set_params) is
//! **change-guarded** (re-broadcasts only on an actual value change), so the
//! write-back of the already-current value is a silent no-op. Steady state
//! (bar == focused): quiescent. After a clamp: one extra round, then quiescent.
//!
//! # Mouse press-and-hold
//!
//! A mouse-down arms the mouse-track capture; the subsequent move/auto/up events
//! route the hold loop, auto-scrolling when the mouse moves out of view (skipping
//! four auto ticks per step).
//!
//! # Colors
//!
//! Each list role is a [`Role`]: [`Role::ListNormal`] /
//! [`Role::ListInactive`] / [`Role::ListSurface`] / [`Role::ListFocused`] /
//! [`Role::ListSelected`] / [`Role::ListDivider`]. A subclass that wanted a
//! different palette surfaces a different [`ListRoles`] sextet from
//! [`ListViewer::list_roles`].
//!
//! # Resizing
//!
//! The scroll-bar step republish on a bounds change is not wired, because nothing
//! currently resizes a list viewer's bounds. A future resize consumer must apply
//! the resize step formula directly — vertical bar `set_step(size.y, …)` and
//! horizontal bar `set_step(size.x / num_cols, …)`, both preserving the existing
//! arrow step — and must NOT reuse [`update_steps`], which reproduces the
//! *constructor* formula instead.
//!
//! # Turbo Vision heritage
//!
//! Ports `TListViewer` (`tlstview.cpp`). The abstract-class inheritance becomes a
//! trait plus a state struct plus generic free functions (D2), because the base's
//! draw must call back into the subclass's `get_text`. Owner back-pointers to the
//! sibling scroll bars become [`ViewId`] handles brokered by the event loop (D3),
//! and the palette becomes [`Role`]s.

use crate::capture::TrackMask;
use crate::command::Command;
use crate::event::{Event, Key, KeyEvent, ctrl_to_arrow};
use crate::theme::{Role, SurfaceRoles};
use crate::view::{Context, DrawCtx, Point, StateFlag, View, ViewId, ViewState};

/// The empty-list placeholder text.
const EMPTY_TEXT: &str = "<empty>";

/// Number of auto-repeat ticks to accumulate before stepping the focus by one
/// item when the mouse is held outside the view.
const MOUSE_AUTOS_TO_SKIP: i32 = 4;

/// Per-hold tracking state — initialized by `MouseDown` and cleared by `MouseUp`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LvTrack {
    /// Accumulated auto-repeat ticks since the last step/reset (re-initialised to
    /// 0 each time it reaches [`MOUSE_AUTOS_TO_SKIP`]).
    count: i32,
    /// The focused item at the START of the current tick; used to decide whether
    /// a re-focus + redraw is needed (only when the new item differs).
    old_item: i32,
}

// ---------------------------------------------------------------------------
// FindMode — opt-in incremental find-and-highlight mode
// ---------------------------------------------------------------------------

/// Find-and-highlight mode for a list. Opt-in; the default [`FindMode::Off`]
/// keeps the classic type-to-search prefix lookup unchanged.
///
/// # Turbo Vision heritage
///
/// An rstv extension on top of the faithful `TListViewer` lookup — see the
/// design note `docs/superpowers/specs/2026-06-30-listviewer-incremental-find-design.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindMode {
    /// Off — the classic prefix lookup (today's behaviour).
    Off,
    /// Query + highlight only; the host supplies and filters the rows.
    Highlight,
    /// Query + highlight + self-filter: the list narrows its own source set.
    Filter,
}

// ---------------------------------------------------------------------------
// ListViewerState — the data members
// ---------------------------------------------------------------------------

/// The shared state of every list-viewer. A concrete list widget embeds one and
/// exposes it via [`ListViewer::lv`]/[`ListViewer::lv_mut`].
///
/// # Turbo Vision heritage
///
/// The data half of `TListViewer` (`tlstview.cpp`) — its instance fields.
pub struct ListViewerState {
    /// View state (geometry, flags, …) — the `View` composition target.
    pub state: ViewState,
    /// The number of columns (`>= 1`) the items are laid out in.
    ///
    /// Controls the column-major item layout: relative to the first visible
    /// row [`top_item`](Self::top_item), column `j` holds items
    /// `top_item + j * size.y .. top_item + (j + 1) * size.y`. Must be `>= 1`; the constructor clamps
    /// smaller values to `1` and fires a `debug_assert`.
    pub num_cols: i32,
    /// The item index drawn at the top-left cell (the scroll offset).
    ///
    /// Adjusted by [`focus_item`] to keep `focused` visible. Read it to find
    /// out which item is at the top of the current view; set it directly only
    /// when constructing a pre-scrolled list before insertion.
    pub top_item: i32,
    /// The index of the item that holds the keyboard cursor.
    ///
    /// Changed by keyboard nav, mouse clicks, and the scrollbar read-sync.
    /// Read it to find the current selection; move it by calling
    /// [`focus_item`] or [`focus_item_num`] (both keep `top_item` in sync).
    pub focused: i32,
    /// The total number of items in the list.
    ///
    /// Set this by calling [`set_range`], which also resets `focused` if it
    /// now falls past the end and (re)publishes the vertical bar's range.
    /// Reading it directly is fine; writing it directly is not (the bar and
    /// `focused` would be left inconsistent).
    pub range: i32,
    /// **Cached** horizontal-bar `value` — `draw` cannot reach the sibling bar,
    /// so the value is cached here and refreshed by the read-sync
    /// ([`apply_scroll`]).
    pub indent: i32,
    /// The horizontal scrollbar, identified by [`ViewId`] (`None` if absent).
    ///
    /// Pass the bar's id when constructing via [`ListViewerState::new`].
    /// After insertion call [`update_steps`] to publish the initial step sizes.
    /// The pump then brokers all subsequent reads and writes through the
    /// [`apply_scroll`] / `Deferred::SyncListViewer` seam — do NOT read or
    /// write the bar directly from event handlers.
    pub h_scroll_bar: Option<ViewId>,
    /// The vertical scrollbar, identified by [`ViewId`] (`None` if absent).
    ///
    /// Same wiring rules as [`h_scroll_bar`](Self::h_scroll_bar): pass the id
    /// at construction, call [`update_steps`] after insertion, and let the pump
    /// broker all bar ↔ list synchronization.
    pub v_scroll_bar: Option<ViewId>,
    /// Absolute screen position of this view's `(0, 0)`, cached by the last
    /// `draw` call — feeds the [`MouseTrackCapture`] origin for localizing
    /// subsequent `MouseMove`/`MouseAuto` events.
    pub(crate) abs_origin: Point,
    /// Per-hold mouse-tracking state — `Some` while a track is in flight
    /// (between `MouseDown` and `MouseUp`), `None` otherwise. Guards the
    /// tracking arms against stray (untracked) events.
    pub(crate) track: Option<LvTrack>,
    /// Find mode (opt-in). `Off` keeps the classic prefix lookup; the other
    /// variants enable the accumulated-query find-and-highlight.
    pub find_mode: FindMode,
    /// The accumulated find query (find mode only; always empty when `Off`).
    pub query: String,
}

impl ListViewerState {
    /// Create list-viewer state for `bounds`, with `num_cols` columns and
    /// optional scroll bar ids.
    ///
    /// Call this before inserting the widget into its parent group. Because
    /// there is no `Context` at construction time, bar step sizes cannot be
    /// published yet — call [`update_steps`] immediately after the widget is
    /// inserted (the same two-step pattern the scroller uses). The state starts
    /// at the top of an empty list (`top_item = 0`, `focused = 0`, `range = 0`).
    ///
    /// Sets `ofFirstClick` and `ofSelectable` on the embedded `ViewState`.
    /// `num_cols < 1` is clamped to `1` with a `debug_assert`.
    pub fn new(
        bounds: crate::view::Rect,
        num_cols: i32,
        h_scroll_bar: Option<ViewId>,
        v_scroll_bar: Option<ViewId>,
    ) -> Self {
        debug_assert!(
            num_cols >= 1,
            "ListViewer num_cols must be >= 1 (size.x / num_cols and col_width math \
             divide by it); clamping to 1"
        );
        let num_cols = num_cols.max(1);
        let mut state = ViewState::new(bounds);
        state.options.first_click = true;
        state.options.selectable = true;
        ListViewerState {
            state,
            num_cols,
            top_item: 0,
            focused: 0,
            range: 0,
            indent: 0,
            h_scroll_bar,
            v_scroll_bar,
            abs_origin: Point::new(0, 0),
            track: None,
            find_mode: FindMode::Off,
            query: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// ListRoles — the per-class color sextet
// ---------------------------------------------------------------------------

/// The six [`Role`]s that [`draw`] maps its color matrix through.
///
/// To recolor a list widget, override [`ListViewer::list_roles`] and return a
/// custom `ListRoles` with different role values. The fields map directly to
/// the six drawing cases: a normal item in an active / inactive / surface
/// list, the focused cursor item, a selected (multi-select) item, and the
/// inter-column divider. The constant [`ListRoles::LIST_VIEWER`] holds the
/// base sextet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListRoles {
    /// A normal item of an owner-active, self-focused (or non-selectable)
    /// list (also the `<empty>` fill).
    pub normal: Role,
    /// A normal item when the owning pane is inactive.
    pub inactive: Role,
    /// The middle content surface: active pane, a selectable sibling holds
    /// focus.
    pub surface: Role,
    /// The focused (cursor) item, shown when this list is the focused control.
    pub focused: Role,
    /// A selected item.
    pub selected: Role,
    /// The inter-column divider.
    pub divider: Role,
}

impl ListRoles {
    /// The base list-viewer role family.
    pub const LIST_VIEWER: ListRoles = ListRoles {
        normal: Role::ListNormal,
        inactive: Role::ListInactive,
        surface: Role::ListSurface,
        focused: Role::ListFocused,
        selected: Role::ListSelected,
        divider: Role::ListDivider,
    };
}

// ---------------------------------------------------------------------------
// ListViewer — the overridable methods (a trait)
// ---------------------------------------------------------------------------

/// The abstract list-viewer base, as a trait of overridable methods.
///
/// Concrete list widgets implement [`lv`](Self::lv)/[`lv_mut`](Self::lv_mut)
/// (the data accessors) and override [`get_text`](Self::get_text)/
/// [`is_selected`](Self::is_selected)/[`select_item`](Self::select_item) as
/// needed; the shared draw/event/nav logic (the free functions in this module)
/// is generic over `L: ListViewer` and calls back into these.
///
/// **Wiring caveat (no compile-time enforcement):** a concrete list widget MUST
/// delegate ALL of these `View` methods to this module's free functions:
/// [`draw`], [`handle_event`], [`set_state`], [`View::cursor_request`](crate::view::View::cursor_request)
/// → [`focused_cursor`], [`View::apply_scroll_sync`](crate::view::View::apply_scroll_sync)
/// → [`apply_scroll`], and [`View::as_any_mut`](crate::view::View::as_any_mut)
/// (the cross-view broker downcasts through it). In particular, forgetting to
/// override `apply_scroll_sync` is **silent** — its base default is a no-op, so the
/// widget compiles but loses all scrollbar read-sync with no error. (See `FakeList`
/// in this module's tests for the full delegation template.)
///
/// # Turbo Vision heritage
///
/// The trait half of `TListViewer` (`tlstview.cpp`): the overridable hooks
/// (item text / selection / commit / palette), with the data members in
/// [`ListViewerState`] and the shared logic in this module's free functions.
pub trait ListViewer: View {
    /// Borrow the embedded [`ListViewerState`].
    fn lv(&self) -> &ListViewerState;
    /// Mutably borrow the embedded [`ListViewerState`].
    fn lv_mut(&mut self) -> &mut ListViewerState;

    /// The text for `item`. The default returns empty; concrete list widgets
    /// (e.g. a list box) override it.
    fn get_text(&self, _item: i32) -> String {
        String::new()
    }

    /// The six color roles [`draw`] uses for this list's color matrix.
    ///
    /// Override to recolor the whole list. Return a [`ListRoles`] struct with
    /// different [`Role`] values for any of the six slots; the base
    /// implementation returns [`ListRoles::LIST_VIEWER`]. A history viewer, for
    /// example, overrides this to use its own lighter palette.
    fn list_roles(&self) -> ListRoles {
        ListRoles::LIST_VIEWER
    }

    /// Whether `item` should be drawn in the selected color.
    ///
    /// The default returns `item == focused` (single-selection behavior).
    /// Override in a multi-select widget to mark additional items as selected.
    fn is_selected(&self, item: i32) -> bool {
        item == self.lv().focused
    }

    /// Called when the user commits to `item` (double-click, Space, or Enter).
    ///
    /// The default broadcasts [`Command::LIST_ITEM_SELECTED`] with this view's
    /// [`ViewId`] as `source`, so an owner dialog can catch it with
    /// `Event::Broadcast { command: Command::LIST_ITEM_SELECTED, source }` and
    /// filter on `source` to distinguish which list fired. Override to perform
    /// a direct action instead (e.g. close a dialog or load a file).
    fn select_item(&mut self, _item: i32, ctx: &mut Context) {
        let source = self.lv().state.id();
        ctx.broadcast(Command::LIST_ITEM_SELECTED, source);
    }

    /// A hook that runs after the focus moves. The base [`focus_item`] free fn
    /// calls this after moving `focused` (and adjusting `top_item`), so an
    /// override fires on EVERY focus change (keyboard, mouse, scrollbar sync, …).
    /// Default: no-op. A file list overrides it to broadcast that the focused
    /// file changed.
    fn on_focus_changed(&mut self, _ctx: &mut Context) {}

    /// The current non-empty find query, or `None` when find mode is `Off` or
    /// the query is empty. The shared [`draw`] reads this to highlight matches;
    /// hosts read it to mirror the query elsewhere.
    fn find_query(&self) -> Option<&str> {
        let lv = self.lv();
        if lv.find_mode == FindMode::Off || lv.query.is_empty() {
            None
        } else {
            Some(&lv.query)
        }
    }

    /// Set the find query from an external source — e.g. a host `InputLine` that
    /// owns the text and drives an unfocused list's incremental find (a combobox).
    /// The mirror image of typed find, minus the keystrokes: a whole-string setter
    /// because a text source always holds the complete string.
    ///
    /// No-op when find mode is [`FindMode::Off`] or the query is unchanged;
    /// otherwise replaces the query, fires [`Command::LIST_FIND_CHANGED`] (source =
    /// this list) and runs [`Self::on_query_changed`] — the same change tail
    /// keystroke find runs. Passing `""` is exactly [`Self::clear_find`] reached
    /// through the same door, so a host need not special-case the empty field.
    fn set_find_query(&mut self, query: &str, ctx: &mut Context) {
        if self.lv().find_mode == FindMode::Off || self.lv().query == query {
            return;
        }
        self.lv_mut().query = query.to_string();
        find_notify(self, ctx);
    }

    /// Clear the find query — the host-callable Esc equivalent. Equivalent to
    /// `set_find_query("", ctx)`: no-op when find is `Off` or already empty,
    /// else fires [`Command::LIST_FIND_CHANGED`] and runs [`Self::on_query_changed`].
    fn clear_find(&mut self, ctx: &mut Context) {
        self.set_find_query("", ctx);
    }

    /// Hook fired after the find query changes (default: no-op). A self-filtering
    /// concrete widget overrides it to re-derive its visible rows from its
    /// source. Called by the shared `handle_event` and by [`Self::clear_find`].
    fn on_query_changed(&mut self, _ctx: &mut Context) {}
}

// ---------------------------------------------------------------------------
// Shared logic — free functions generic over <L: ListViewer + ?Sized>
// ---------------------------------------------------------------------------

/// Clamp `item` into the valid range, then focus it (only when the list is
/// non-empty).
///
/// `item < 0 → 0`; `item >= range && range > 0 → range - 1`; then focus it only
/// when the range is non-empty.
pub fn focus_item_num<L: ListViewer + ?Sized>(this: &mut L, mut item: i32, ctx: &mut Context) {
    if item < 0 {
        item = 0;
    } else if item >= this.lv().range && this.lv().range > 0 {
        item = this.lv().range - 1;
    }
    if this.lv().range != 0 {
        focus_item(this, item, ctx);
    }
}

/// Set `focused = item`, push the new value to the vertical bar, and adjust
/// `top_item` so the focused item is visible.
///
/// Sets `focused = item`; if a vertical bar exists, requests `set_value(item)`
/// on it. Then the `top_item` adjust block (guarded by `size.y > 0`, with
/// separate single-column and multi-column cases).
pub fn focus_item<L: ListViewer + ?Sized>(this: &mut L, item: i32, ctx: &mut Context) {
    this.lv_mut().focused = item;
    if let Some(v) = this.lv().v_scroll_bar {
        // Push the value to the bar — the write-back the termination property
        // relies on (a no-op when the bar's value already == item, since
        // set_params is change-guarded).
        ctx.request_scroll_bar_params(v, Some(item), None, None, None, None);
    }

    let size_y = this.lv().state.size.y;
    let num_cols = this.lv().num_cols;
    let top_item = this.lv().top_item;
    if size_y > 0 {
        if item < top_item {
            this.lv_mut().top_item = if num_cols == 1 {
                item
            } else {
                item - item % size_y
            };
        } else if item >= top_item + size_y * num_cols {
            this.lv_mut().top_item = if num_cols == 1 {
                item - size_y + 1
            } else {
                item - item % size_y - (size_y * (num_cols - 1))
            };
        }
    }

    // The post-focus hook (default: no-op). Fires AFTER the `focused`/`top_item`
    // move, so an override sees the settled position.
    this.on_focus_changed(ctx);
}

/// Set the list length, resetting `focused` if it now falls past the end, and
/// (re)publish the vertical bar's range.
///
/// Sets `range = a_range`; if `focused` now falls past the end it resets to 0;
/// if a vertical bar exists, requests `set_params(focused, 0, a_range - 1, …)`,
/// preserving the existing page and arrow steps.
pub fn set_range<L: ListViewer + ?Sized>(this: &mut L, a_range: i32, ctx: &mut Context) {
    this.lv_mut().range = a_range;
    if this.lv().focused >= a_range {
        this.lv_mut().focused = 0;
    }
    let focused = this.lv().focused;
    if let Some(v) = this.lv().v_scroll_bar {
        ctx.request_scroll_bar_params(v, Some(focused), Some(0), Some(a_range - 1), None, None);
    }
}

/// The displayed view of `source` for a find mode/query: the full source unless
/// `Filter` mode with a non-empty query narrows it to the rows containing the
/// query (case-insensitive substring), preserving `source` order.
pub(crate) fn filtered_view(source: &[String], mode: FindMode, query: &str) -> Vec<String> {
    if mode == FindMode::Filter && !query.is_empty() {
        source
            .iter()
            .filter(|s| find_match(s, query).is_some())
            .cloned()
            .collect()
    } else {
        source.to_vec()
    }
}

/// Republish `len` as the range and place focus: to the top when `reset_focus`
/// (a fresh `new_list`), else clamp the existing focus into the new range (a
/// query change). The shared tail of both concrete widgets' `rebuild_view`.
pub(crate) fn apply_view_len<L: ListViewer + ?Sized>(
    this: &mut L,
    len: i32,
    reset_focus: bool,
    ctx: &mut Context,
) {
    set_range(this, len, ctx);
    if reset_focus {
        if len > 0 {
            focus_item(this, 0, ctx);
        }
    } else {
        let f = this.lv().focused.min(len - 1).max(0);
        focus_item_num(this, f, ctx);
    }
}

/// The body of the scroll-bar-changed read-sync, called by the pump (the read
/// broker) after it resolves both bars and reads their `value`s.
///
/// The horizontal-bar branch refreshes the cached
/// [`indent`](ListViewerState::indent); the vertical-bar branch runs
/// [`focus_item_num`] on the bar's value. Reading both each sync is harmless —
/// the vertical-bar write-back is a no-op in steady state.
pub fn apply_scroll<L: ListViewer + ?Sized>(
    this: &mut L,
    h: Option<i32>,
    v: Option<i32>,
    ctx: &mut Context,
) {
    if let Some(hv) = h {
        this.lv_mut().indent = hv;
    }
    if let Some(vv) = v {
        focus_item_num(this, vv, ctx);
    }
}

/// The list-viewer **construction** step formula — (re)publish each bar's
/// page/arrow step. Exposed as a `Context`-taking entry the consumer/test calls
/// **after insertion** (the no-`Context` constructor cannot reach the bars — the
/// same constraint the scroller hit).
///
/// - vertical bar: single column → page step `size.y - 1`, arrow step 1; else
///   page step `size.y * num_cols`, arrow step `size.y`.
/// - horizontal bar: page step `size.x / num_cols`, arrow step 1.
///
/// **This is the CONSTRUCTION formula, NOT the resize formula.** The resize path
/// ([`on_bounds_changed`]) uses a **different** step: a plain `size.y` page step
/// for the vertical bar, and it **preserves the live arrow step** for both bars.
/// A future resize consumer must use that formula directly — do **NOT** call
/// `update_steps` for a resize.
pub fn update_steps<L: ListViewer + ?Sized>(this: &L, ctx: &mut Context) {
    let size = this.lv().state.size;
    let num_cols = this.lv().num_cols;
    if let Some(v) = this.lv().v_scroll_bar {
        let (pg_step, ar_step) = if num_cols == 1 {
            (size.y - 1, 1)
        } else {
            (size.y * num_cols, size.y)
        };
        ctx.request_scroll_bar_params(v, None, None, None, Some(pg_step), Some(ar_step));
    }
    if let Some(h) = this.lv().h_scroll_bar {
        ctx.request_scroll_bar_params(h, None, None, None, Some(size.x / num_cols), Some(1));
    }
}

/// The resize-step formula — called from `on_bounds_changed` for concrete
/// `ListViewer` implementors.
///
/// After the new bounds are applied, re-publish each scrollbar's page step while
/// preserving its arrow step: the vertical bar uses a plain `size.y` page step
/// and the horizontal bar uses `size.x / num_cols`.
///
/// **Differs from the construction formula** ([`update_steps`]): resize uses
/// `size.y` for the vertical bar (not `size.y - 1` / `size.y * num_cols`) and
/// does NOT touch the arrow step.
pub fn on_bounds_changed<L: ListViewer + ?Sized>(this: &L, ctx: &mut Context) {
    let lv = this.lv();
    let size = lv.state.size;
    let num_cols = lv.num_cols.max(1);
    if let Some(v) = lv.v_scroll_bar {
        ctx.request_scroll_bar_params(
            v,
            None,         // preserve value
            None,         // preserve min
            None,         // preserve max
            Some(size.y), // page step = size.y (resize formula)
            None,         // preserve arrow step
        );
    }
    if let Some(h) = lv.h_scroll_bar {
        ctx.request_scroll_bar_params(
            h,
            None,                    // preserve value
            None,                    // preserve min
            None,                    // preserve max
            Some(size.x / num_cols), // page step = size.x / num_cols
            None,                    // preserve arrow step
        );
    }
}

/// Update a state flag and propagate scrollbar visibility.
///
/// Applies `flag`/`enable` to the embedded `ViewState`. On a `Focused` change
/// it also broadcasts [`Command::RECEIVED_FOCUS`] or [`Command::RELEASED_FOCUS`].
/// On `Active`, `Selected`, or `Visible` changes it shows or hides both scroll
/// bars: bars are **visible iff `active && visible`** (unlike the scroller,
/// which uses `active || selected`). Concrete list widgets call this from their
/// `View::set_state` implementation.
pub fn set_state<L: ListViewer + ?Sized>(
    this: &mut L,
    flag: StateFlag,
    enable: bool,
    ctx: &mut Context,
) {
    this.lv_mut().state.set_flag(flag, enable);
    if flag == StateFlag::Focused {
        let source = this.lv().state.id();
        ctx.broadcast(
            if enable {
                Command::RECEIVED_FOCUS
            } else {
                Command::RELEASED_FOCUS
            },
            source,
        );
    }
    if flag == StateFlag::Active || flag == StateFlag::Selected || flag == StateFlag::Visible {
        // Show iff active && visible — BOTH, not the scroller's active||selected.
        let visible = this.lv().state.state.active && this.lv().state.state.visible;
        if let Some(h) = this.lv().h_scroll_bar {
            ctx.request_set_visible(h, visible);
        }
        if let Some(v) = this.lv().v_scroll_bar {
            ctx.request_set_visible(v, visible);
        }
    }
}

/// Route a keystroke into the find query when find mode is active. Returns
/// `true` if the key was a find key (and the event was consumed), `false` if it
/// is not a find key (the caller continues with normal navigation). Backspace
/// on an empty query and Esc on an empty query both return `false` so they
/// propagate (an empty-query Esc lets a host dialog close).
fn find_route_key<L: ListViewer + ?Sized>(
    this: &mut L,
    ke: KeyEvent,
    ev: &mut Event,
    ctx: &mut Context,
) -> bool {
    match ke.key {
        Key::Char(c) if !ke.modifiers.ctrl && !ke.modifiers.alt => {
            this.lv_mut().query.push(c);
            find_after_change(this, ev, ctx);
            true
        }
        Key::Backspace => {
            if this.lv().query.is_empty() {
                return false;
            }
            this.lv_mut().query.pop();
            find_after_change(this, ev, ctx);
            true
        }
        Key::Esc => {
            if this.lv().query.is_empty() {
                return false;
            }
            this.lv_mut().query.clear();
            find_after_change(this, ev, ctx);
            true
        }
        _ => false,
    }
}

/// The shared find-query change tail: broadcast the change (this list as
/// `source`, mirroring `select_item` / `ScrollBar`) and run the self-filter hook.
/// Reused by the keystroke path (`find_after_change`, which also consumes the
/// event) and the host-callable path (`set_find_query` / `clear_find`, which have
/// no event to consume).
fn find_notify<L: ListViewer + ?Sized>(this: &mut L, ctx: &mut Context) {
    let source = this.lv().state.id();
    ctx.broadcast(Command::LIST_FIND_CHANGED, source);
    this.on_query_changed(ctx);
}

/// Keystroke-path tail: the shared `find_notify` plus consuming the key event.
fn find_after_change<L: ListViewer + ?Sized>(this: &mut L, ev: &mut Event, ctx: &mut Context) {
    find_notify(this, ctx);
    ev.clear();
}

/// Mouse + keyboard nav + the scrollbar broadcast filter. Reused verbatim by
/// concrete list widgets.
///
/// **Intentional omission:** C++ `TListViewer::handleEvent` calls
/// `TView::handleEvent(event)` first (line 221 of `tlstview.cpp`). That base
/// call only performs mouse-down auto-select (focus the view on click), which
/// tvision-rs relocates to `Group::route_event`. `TView::handleEvent` is a
/// no-op for every other event class, so there is no base behavior to inherit.
pub fn handle_event<L: ListViewer + ?Sized>(this: &mut L, ev: &mut Event, ctx: &mut Context) {
    match *ev {
        // -------------------------------------------------------------------
        // evMouseDown — first loop iteration: position + optional select, then
        // arm the mouse-track capture.
        //
        // The first iteration runs on the down event; subsequent iterations
        // arrive as tracked MouseMove/MouseAuto events; the post-loop
        // `focus_item_num` runs in the MouseUp arm.
        //
        // Double-click break: if the down event is a double-click, the loop
        // body breaks immediately after the first iteration — no capture is
        // needed, so `start_mouse_track` is skipped.
        // -------------------------------------------------------------------
        Event::MouseDown(me) => {
            let size = this.lv().state.size;
            let num_cols = this.lv().num_cols;
            let col_width = size.x / num_cols + 1;
            let top_item = this.lv().top_item;
            // mouse is view-local already (the group delivers view-local coords).
            let mouse = me.position;
            let new_item = mouse.y + size.y * (mouse.x / col_width) + top_item;
            focus_item_num(this, new_item, ctx);
            if me.flags.double_click {
                // Double-click: break immediately (no tracking). Post-loop:
                // focusItemNum(newItem) already done above; select if in range.
                if this.lv().range > new_item {
                    this.select_item(new_item, ctx);
                }
            } else if let Some(id) = this.lv().state.id() {
                // Non-double-click: arm the mouse-track capture. Subsequent
                // MouseMove/MouseAuto/MouseUp events are routed back into this
                // handle_event via Deferred::MouseTrack, localized to view-local
                // coords via abs_origin.
                let abs_origin = this.lv().abs_origin;
                this.lv_mut().track = Some(LvTrack {
                    count: 0,
                    old_item: new_item,
                });
                ctx.start_mouse_track(
                    id,
                    abs_origin,
                    TrackMask {
                        mouse_move: true,
                        mouse_auto: true,
                        ..Default::default()
                    },
                );
            } else {
                // Degenerate fallback: an uninserted (test-only) list has no id
                // (ids are stamped at Group::insert), so the capture broker
                // cannot resolve it. Position-only single-shot behavior — no
                // hold tracking.
            }
            ev.clear();
        }

        // -------------------------------------------------------------------
        // evMouseMove (tracked) — the loop body's in-view move case.
        //
        // C++ tlstview.cpp:229-231: if `mouseInView` → compute item from pos.
        // Out-of-view moves do nothing (only evMouseAuto steps the focus).
        // Guarded by `track.is_some()`.
        // -------------------------------------------------------------------
        Event::MouseMove(me) if this.lv().track.is_some() => {
            let size = this.lv().state.size;
            let num_cols = this.lv().num_cols;
            let col_width = size.x / num_cols + 1;
            let top_item = this.lv().top_item;
            let mouse = me.position;
            // mouseInView equivalent: position is view-local from the capture.
            let in_view = mouse.x >= 0 && mouse.y >= 0 && mouse.x < size.x && mouse.y < size.y;
            if in_view {
                let new_item = mouse.y + size.y * (mouse.x / col_width) + top_item;
                let old_item = this.lv().track.map(|t| t.old_item).unwrap_or(new_item);
                if new_item != old_item {
                    focus_item_num(this, new_item, ctx);
                }
                if let Some(t) = this.lv_mut().track.as_mut() {
                    t.old_item = new_item;
                }
            }
            // Out-of-view moves: no-op (C++ does nothing in the out-of-view
            // branch for evMouseMove — only evMouseAuto steps the focus).
            ev.clear();
        }

        // -------------------------------------------------------------------
        // evMouseAuto (tracked) — the loop body's auto-scroll case.
        //
        // C++ tlstview.cpp:229-263: in-view → same as move; out-of-view →
        // increment count, every `mouseAutosToSkip` ticks step the focused
        // item by ±1 (single-col) or ±size.y / page (multi-col).
        // Guarded by `track.is_some()`.
        // -------------------------------------------------------------------
        Event::MouseAuto(me) if this.lv().track.is_some() => {
            let size = this.lv().state.size;
            let num_cols = this.lv().num_cols;
            let col_width = size.x / num_cols + 1;
            let top_item = this.lv().top_item;
            let focused = this.lv().focused;
            let mouse = me.position;
            let in_view = mouse.x >= 0 && mouse.y >= 0 && mouse.x < size.x && mouse.y < size.y;

            let new_item = if in_view {
                // In-view: same computation as MouseDown/MouseMove.
                mouse.y + size.y * (mouse.x / col_width) + top_item
            } else {
                // Out-of-view: increment the auto-skip counter; every
                // MOUSE_AUTOS_TO_SKIP ticks step the focus by the geometry
                // rules from tlstview.cpp:233-263.
                let t = this.lv_mut().track.as_mut().unwrap();
                t.count += 1;
                if t.count == MOUSE_AUTOS_TO_SKIP {
                    t.count = 0;
                    if num_cols == 1 {
                        // Single-col: step by ±1 based on y.
                        if mouse.y < 0 {
                            focused - 1
                        } else if mouse.y >= size.y {
                            focused + 1
                        } else {
                            focused // in-band y but out-of-band x? keep focused
                        }
                    } else {
                        // Multi-col: step by column (±size.y), or page edges.
                        if mouse.x < 0 {
                            focused - size.y
                        } else if mouse.x >= size.x {
                            focused + size.y
                        } else if mouse.y < 0 {
                            focused - focused % size.y
                        } else if mouse.y > size.y {
                            focused - focused % size.y + size.y - 1
                        } else {
                            focused
                        }
                    }
                } else {
                    // Not yet at the skip threshold: no step.
                    focused
                }
            };

            let old_item = this.lv().track.map(|t| t.old_item).unwrap_or(new_item);
            if new_item != old_item {
                focus_item_num(this, new_item, ctx);
            }
            if let Some(t) = this.lv_mut().track.as_mut() {
                t.old_item = new_item;
            }
            ev.clear();
        }

        // -------------------------------------------------------------------
        // Tracked mouse-up — post-loop: re-focus the settled item + clear track.
        //
        // We do `focus_item_num(focused)` (the last-known focused item, which
        // equals the settled position after the final loop iteration) then clear
        // the track. Double-click is NOT re-checked
        // here: this MouseUp POPS the capture (ConsumedPop), so the hold is
        // already over before any second down can arrive — the second down of a
        // double-click re-enters the MouseDown arm as a fresh event and fires
        // the select path there. The C++ meDoubleClick check on the up path
        // (tlstview.cpp:276) is semantically unreachable in tvision-rs because
        // MouseUp never carries double_click (a down-event-only flag).
        // Guarded by `track.is_some()`.
        // -------------------------------------------------------------------
        Event::MouseUp(_) if this.lv().track.is_some() => {
            this.lv_mut().track = None;
            // Post-loop focusItemNum(newItem): re-focus the current focused item
            // (clamped, v-bar synced). Faithful to C++:274 `focusItemNum(newItem)`.
            let focused = this.lv().focused;
            focus_item_num(this, focused, ctx);
            ev.clear();
        }

        // -------------------------------------------------------------------
        // evKeyDown — Space → select, else the nav switch (via ctrlToArrow).
        // -------------------------------------------------------------------
        Event::KeyDown(ke) => {
            // Find mode (opt-in) intercepts query keys before navigation; a
            // non-query key (arrows, Enter, …) falls through to the nav below.
            if this.lv().find_mode != FindMode::Off && find_route_key(this, ke, ev, ctx) {
                return;
            }

            let focused = this.lv().focused;
            let range = this.lv().range;
            let size_y = this.lv().state.size.y;
            let num_cols = this.lv().num_cols;

            let new_item: i32;
            // charCode == ' ' && focused < range -> selectItem(focused).
            if matches!(ke.key, Key::Char(' '))
                && !ke.modifiers.ctrl
                && !ke.modifiers.alt
                && focused < range
            {
                this.select_item(focused, ctx);
                new_item = focused;
            } else if matches!(ke.key, Key::PageDown) && ke.modifiers.ctrl {
                // kbCtrlPgDn -> last item. Matched on the decomposed key
                // (PageDown + ctrl) BEFORE ctrl_to_arrow, which would otherwise
                // see no Char to remap and pass PageDown through as a plain page
                // jump.
                new_item = range - 1;
            } else if matches!(ke.key, Key::PageUp) && ke.modifiers.ctrl {
                // kbCtrlPgUp -> first item.
                new_item = 0;
            } else {
                // ctrlToArrow(keyCode) — the WordStar Ctrl-letter nav aliases.
                let mapped = ctrl_to_arrow(ke);
                new_item = match mapped.key {
                    Key::Up => focused - 1,
                    Key::Down => focused + 1,
                    // Left/Right only navigate when there is more than one column;
                    // with numCols == 1 the C++ `return`s (event left uncleared) —
                    // realized here by the guard falling through to `_ => return`.
                    Key::Right if num_cols > 1 => focused + size_y,
                    Key::Left if num_cols > 1 => focused - size_y,
                    Key::PageDown => focused + size_y * num_cols,
                    Key::PageUp => focused - size_y * num_cols,
                    Key::Home => this.lv().top_item,
                    Key::End => this.lv().top_item + (size_y * num_cols) - 1,
                    _ => return, // default (incl. single-col Left/Right): return.
                };
            }
            focus_item_num(this, new_item, ctx);
            ev.clear();
        }

        // -------------------------------------------------------------------
        // Broadcast — own-bar clicked → select; own-bar changed → request a
        // read-sync (the source filter, like the scroller).
        // -------------------------------------------------------------------
        Event::Broadcast { command, source } => {
            // Only a selectable list reacts.
            if !this.lv().state.options.selectable {
                return;
            }
            let h = this.lv().h_scroll_bar;
            let v = this.lv().v_scroll_bar;
            let from_own_bar = source.is_some() && (source == h || source == v);
            if command == Command::SCROLL_BAR_CLICKED && from_own_bar {
                // Focus this view within its owning group. Requires this view be
                // inserted (have an id).
                if let Some(id) = this.lv().state.id() {
                    ctx.request_focus(id);
                }
            } else if command == Command::SCROLL_BAR_CHANGED && from_own_bar {
                // The pump brokers the read (resolve the bars, read value, call
                // back through apply_scroll_sync). Requires this view have an id.
                if let Some(id) = this.lv().state.id() {
                    ctx.request_scroll_sync(id, h, v);
                }
            }
        }

        _ => {}
    }
}

/// Draw `text` at `(x, y)` after skipping `indent` leading display columns,
/// painting the half-open **char** range `[m0, m1)` in `accent` and the rest in
/// `base`. Splits the row into before/match/after and emits up to three
/// `put_str_part` calls, threading the horizontal-scroll `indent` budget across
/// the pieces (a piece fully left of the scroll offset draws nothing and just
/// consumes the budget). The split is by `char`, so it is multibyte-safe; the
/// per-piece column accounting assumes one column per char (wide CJK glyphs in a
/// horizontally-scrolled list are the one imperfect case — acceptable: the
/// default has no h-scroll, and the classic lookup shares the assumption).
#[allow(clippy::too_many_arguments)]
fn draw_highlighted(
    ctx: &mut DrawCtx,
    x: i32,
    y: i32,
    text: &str,
    indent: i32,
    base: crate::color::Style,
    accent: crate::color::Style,
    m0: usize,
    m1: usize,
) {
    let chars: Vec<char> = text.chars().collect();
    let before: String = chars[..m0].iter().collect();
    let matched: String = chars[m0..m1].iter().collect();
    let after: String = chars[m1..].iter().collect();
    let mut cx = x;
    let mut rem = indent;
    for (piece, style) in [(before, base), (matched, accent), (after, base)] {
        let w = piece.chars().count() as i32;
        if rem >= w {
            // Entirely scrolled off to the left; just consume the indent budget.
            rem -= w;
        } else {
            let drawn = ctx.put_str_part(cx, y, &piece, rem, style);
            cx += drawn;
            rem = 0;
        }
    }
}

/// Render the `range` items in `num_cols` columns. Reused verbatim by concrete
/// list widgets; calls back into [`get_text`](ListViewer::get_text) /
/// [`is_selected`](ListViewer::is_selected).
///
/// Draws the two-axis color matrix — the row surface (the three-surface rule:
/// normal/surface/inactive, see
/// [`DrawCtx::content_surface`](crate::view::DrawCtx::content_surface)) tracks
/// [`DrawCtx::owner_active`](crate::view::DrawCtx::owner_active) (is the owning
/// pane focused?) crossed with this list's own focus and selectability, while
/// the current-item highlight (focused/selected) tracks this list's own
/// `state.focused` (is *this* list the focused control?) — the per-cell
/// item/column layout, the `indent` column-skip (the cached horizontal-bar
/// value), the `<empty>` placeholder, the `│` divider, and the focused-cell
/// cursor.
///
/// Also caches `abs_origin` for the mouse-track capture.
///
/// NOTE: `this: &mut L` (not `&L`) — the `abs_origin` cache write requires
/// mutability. Do NOT revert to `&L`: the draw is logically const, but the
/// origin stored here feeds [`Context::start_mouse_track`] (the
/// `Button::abs_origin` pattern).
pub fn draw<L: ListViewer + ?Sized>(this: &mut L, ctx: &mut DrawCtx) {
    // Cache the absolute origin for the mouse-tracking capture: the
    // MouseTrackCapture converts absolute mouse coords to view-local via this
    // value, mirroring the Button::abs_origin pattern.
    this.lv_mut().abs_origin = ctx.origin();
    let lv = this.lv();
    let st = &lv.state.state;
    let list_focused = st.focused; // highlight axis: am I the focused list?

    // Color matrix via the class's role sextet (list_roles).
    let roles = this.list_roles();
    // Surface axis: the three-surface rule (owner_active x own-focus x
    // selectability) — see `DrawCtx::content_surface`.
    let normal = ctx.content_surface(
        SurfaceRoles {
            normal: roles.normal,
            surface: roles.surface,
            inactive: roles.inactive,
        },
        list_focused,
        lv.state.options.selectable,
    );
    let selected = ctx.style(roles.selected);
    let focused_color = if list_focused {
        Some(ctx.style(roles.focused))
    } else {
        None
    };
    let divider_color = ctx.style(roles.divider);
    let empty_color = ctx.style(roles.normal);
    // The find-highlight accent reuses the list's `selected` role: it pops on
    // normal (cyan) and focused (green) rows; on a multi-select-selected row it
    // matches the row colour (acceptable — selection already marks that row).
    let accent = ctx.style(roles.selected);

    let size = lv.state.size;
    let indent = lv.indent; // the CACHE (not a live h-bar read).
    let num_cols = lv.num_cols;
    let top_item = lv.top_item;
    let range = lv.range;
    let focused = lv.focused;

    let col_width = size.x / num_cols + 1;

    for i in 0..size.y {
        for j in 0..num_cols {
            let item = j * size.y + i + top_item;
            let cur_col = j * col_width;

            let color = if list_focused && focused == item && range > 0 {
                // Focused cell: drawn in the focused color; the hardware cursor for
                // this cell is surfaced via `focused_cursor`, not set here (the
                // read-only draw plus a top-down cursor walk derives it on demand).
                focused_color.unwrap_or(normal)
            } else if item < range && this.is_selected(item) {
                selected
            } else {
                normal
            };

            // Fill the cell with the chosen color.
            ctx.fill(
                crate::view::Rect::new(cur_col, i, cur_col + col_width, i + 1),
                ' ',
                color,
            );

            if item < range {
                // Draw the item text from column +1, skipping `indent` leading
                // columns (the horizontal scroll offset).
                let text = this.get_text(item);
                match this.find_query().and_then(|q| find_match(&text, q)) {
                    Some((m0, m1)) => {
                        draw_highlighted(ctx, cur_col + 1, i, &text, indent, color, accent, m0, m1);
                    }
                    None => {
                        ctx.put_str_part(cur_col + 1, i, &text, indent, color);
                    }
                }
            } else if i == 0 && j == 0 {
                // Past the end of the list. With an active find query the empty
                // view means "no row survived the filter" — show the query so an
                // over-typed/mistyped query is always visible.
                match this.find_query() {
                    Some(q) => {
                        let msg = format!("No match: {q}");
                        ctx.put_str(cur_col + 1, i, &msg, empty_color);
                    }
                    None => {
                        ctx.put_str(cur_col + 1, i, EMPTY_TEXT, empty_color);
                    }
                }
            }

            // The inter-column divider at the right edge of the cell.
            let vbar = ctx.glyphs().frame_v;
            ctx.put_char(cur_col + col_width - 1, i, vbar, divider_color);
        }
    }
    // Hiding the cursor when no focused cell is visible is realized by
    // `focused_cursor` returning `None`, which a concrete widget surfaces via
    // `cursor_request` — not a mutation here (drawing is read-only under the
    // top-down cursor walk).
}

/// The view-local cursor position the focused cell sits at, or `None` if no
/// focused cell is visible. Computed `&self`-only so a concrete widget can
/// surface it via [`View::cursor_request`](crate::view::View::cursor_request).
///
/// The cursor position is derived on demand here — under the read-only `&self`
/// draw and top-down cursor walk, `draw` does not place the cursor itself.
pub fn focused_cursor<L: ListViewer + ?Sized>(this: &L) -> Option<Point> {
    let lv = this.lv();
    let st = &lv.state.state;
    if !(st.selected && st.active) || lv.range <= 0 {
        return None;
    }
    let size = lv.state.size;
    let num_cols = lv.num_cols;
    let col_width = size.x / num_cols + 1;
    let top_item = lv.top_item;
    let focused = lv.focused;
    for i in 0..size.y {
        for j in 0..num_cols {
            let item = j * size.y + i + top_item;
            if focused == item {
                return Some(Point::new(j * col_width + 1, i));
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// SortedSearch — the type-to-search hooks (a sub-trait)
// ---------------------------------------------------------------------------

/// The shift mask = left-shift | right-shift = `0x01 | 0x02`. The
/// incremental-search state machine captures it at the `search_pos -1↔0`
/// transition; file/dir subclasses test `shift_state() & KB_SHIFT`.
pub const KB_SHIFT: u8 = 0x03;

/// Case-insensitive equality of two `char`s. Cheap ASCII path first, then a
/// Unicode `to_lowercase` sequence compare (per-char, so indices stay aligned).
fn ci_char_eq(a: char, b: char) -> bool {
    a == b || a.eq_ignore_ascii_case(&b) || a.to_lowercase().eq(b.to_lowercase())
}

/// The half-open `[start, end)` **char** index range of the first
/// case-insensitive occurrence of `query` in `text`, or `None`. Returns `None`
/// for an empty query or a query longer than `text`. Char-indexed: each query
/// char matches exactly one text char, so the range is safe to slice by `char`.
pub(crate) fn find_match(text: &str, query: &str) -> Option<(usize, usize)> {
    let q: Vec<char> = query.chars().collect();
    if q.is_empty() {
        return None;
    }
    let t: Vec<char> = text.chars().collect();
    if q.len() > t.len() {
        return None;
    }
    'outer: for start in 0..=(t.len() - q.len()) {
        for k in 0..q.len() {
            if !ci_char_eq(t[start + k], q[k]) {
                continue 'outer;
            }
        }
        return Some((start, start + q.len()));
    }
    None
}

/// Case-insensitive equality of the first `n` chars.
fn ci_prefix_eq(a: &[char], b: &[char], n: usize) -> bool {
    if a.len() < n || b.len() < n {
        return false;
    }
    a[..n]
        .iter()
        .zip(&b[..n])
        .all(|(x, y)| x.eq_ignore_ascii_case(y))
}

/// The type-to-search hooks — the polymorphic parts of the incremental-search
/// state machine. The shared machine is [`sorted_handle_event`] +
/// [`sorted_cursor`]; concrete widgets (a sorted list box, a file list)
/// implement these.
pub trait SortedSearch: ListViewer {
    /// The index of the last matched char in the focused item's text; -1 = no
    /// active search.
    fn search_pos(&self) -> i32;
    fn set_search_pos(&mut self, pos: i32);
    /// The shift bits captured at the `search_pos -1↔0` transition. A plain
    /// sorted list box never reads it; file/dir subclasses' `search` does.
    fn shift_state(&self) -> u8;
    fn set_shift_state(&mut self, s: u8);
    /// Given the prepared prefix chars `cur` (already truncated by the state
    /// machine), return the insertion index in `0..=range` of the first item NOT
    /// LESS than the key.
    fn search(&self, cur: &[char]) -> i32;
}

/// The incremental type-to-search state machine. Reusable verbatim by every
/// [`SortedSearch`] widget (a sorted list box, a file list).
///
/// PITFALL 1: `cur` is re-seeded from the FOCUSED ITEM'S text every keystroke,
/// not from an accumulated typed-chars buffer. `search_pos` indexes into `cur`.
///
/// PITFALL 2: exact sequence: save `old_value = focused` → delegate to the base
/// [`handle_event`] → reset `search_pos = -1` if `focused` changed OR a
/// released-focus broadcast arrived → THEN gate on `ev` still being a `KeyDown`.
pub fn sorted_handle_event<L: SortedSearch + ?Sized>(
    this: &mut L,
    ev: &mut Event,
    ctx: &mut Context,
) {
    let old_value = this.lv().focused;
    handle_event(this, ev, ctx); // (1) base first

    // Find mode replaces the type-to-search prefix lookup entirely; the base
    // call above already routed any find key.
    if this.lv().find_mode != FindMode::Off {
        return;
    }

    // (2) reset search on focus change OR a released-focus broadcast.
    let released = matches!(ev,
        Event::Broadcast { command, .. } if *command == Command::RELEASED_FOCUS);
    if old_value != this.lv().focused || released {
        this.set_search_pos(-1);
    }

    // (3) only keys the base passed through are STILL KeyDown here.
    let ke = match *ev {
        Event::KeyDown(ke) => ke,
        _ => return,
    };

    // charScan.charCode != 0: only Char(..) and Backspace produce a charCode.
    // Other passed-through keys (charCode 0) are ignored.
    // Determine the acting char (None = Backspace; Some(c) = a character).
    let acting: Option<char> = match ke.key {
        Key::Char(c) => Some(c),
        Key::Backspace => None,
        _ => return, // charCode == 0 → ignore
    };

    let range = this.lv().range;
    let value0 = this.lv().focused;
    // (A) seed cur from the FOCUSED item's text every keystroke.
    let mut cur: Vec<char> = if value0 < range {
        this.get_text(value0).chars().collect()
    } else {
        Vec::new()
    };
    let old_pos = this.search_pos();

    match acting {
        None => {
            // kbBack branch.
            if this.search_pos() == -1 {
                return;
            }
            this.set_search_pos(this.search_pos() - 1);
            if this.search_pos() == -1 {
                // C++ captures controlKeyState here; capture the real kbShift bit
                // (FileList's `search` reads it; the base SortedListBox does not).
                this.set_shift_state(if ke.modifiers.shift { KB_SHIFT } else { 0 });
            }
            cur.truncate((this.search_pos() + 1).max(0) as usize);
        }
        Some('.') => {
            // Dot branch: jump to the focused item's '.' separator.
            match cur.iter().position(|&c| c == '.') {
                None => this.set_search_pos(-1),
                Some(i) => this.set_search_pos(i as i32),
            }
        }
        Some(c) => {
            // Character branch.
            this.set_search_pos(this.search_pos() + 1);
            if this.search_pos() == 0 {
                // C++ captures controlKeyState here; capture the real kbShift bit.
                this.set_shift_state(if ke.modifiers.shift { KB_SHIFT } else { 0 });
            }
            let idx = this.search_pos() as usize;
            if idx < cur.len() {
                cur[idx] = c;
            } else {
                cur.push(c);
            }
            cur.truncate(idx + 1);
        }
    }

    // key = getKey(curString); search; confirm; focus or revert.
    //
    // The search key is the WHOLE `cur`, mirroring C++ exactly: only the char
    // and back branches re-terminate `curString` (`curString[searchPos+1]=EOS`),
    // which we mirror with `cur.truncate(...)` above — so for those branches
    // `cur` IS the prefix. The DOT branch does NOT truncate, leaving `cur` as
    // the full focused item (e.g. "file.txt"); C++ then searches that full text
    // (NOT "file."). Only the *confirm* below uses `prefix_len` via
    // `ci_prefix_eq`, which reads just the first `prefix_len` chars regardless
    // of `cur`'s length.
    let prefix_len = (this.search_pos() + 1).max(0) as usize;
    let value = this.search(&cur);
    if value < range {
        let new_string: Vec<char> = this.get_text(value).chars().collect();
        if ci_prefix_eq(&cur, &new_string, prefix_len) {
            if value != old_value {
                focus_item(this, value, ctx);
            }
            // Cursor advance is handled by sorted_cursor (derives from search_pos).
        } else {
            this.set_search_pos(old_pos);
        }
    } else {
        this.set_search_pos(old_pos);
    }

    // Consume iff the search advanced OR the key was an alphabetic char.
    let is_alpha = matches!(acting, Some(c) if c.is_ascii_alphabetic());
    if this.search_pos() != old_pos || is_alpha {
        ev.clear();
    }
}

/// The cursor advanced past the matched prefix.
///
/// [`focused_cursor`] returns `x = col*col_width + 1` (the text-start column).
/// Adding `(search_pos + 1)` positions the cursor just after the matched prefix.
/// With `search_pos == -1` the offset is 0 — no advance.
///
/// The cursor is derived ABSOLUTELY from `search_pos` (`base.x + search_pos +
/// 1`) rather than accumulated incrementally: `base.x` is the text-start column
/// re-derived each frame, so there is no running cursor state to keep in sync.
pub fn sorted_cursor<L: SortedSearch + ?Sized>(this: &L) -> Option<Point> {
    let base = focused_cursor(this)?;
    Some(Point::new(base.x + (this.search_pos() + 1).max(0), base.y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use crate::view::{Deferred, Group, Point, Rect};
    use std::collections::HashSet;
    use std::collections::VecDeque;

    // -- FakeList: a test-only concrete ListViewer ----------------------------

    /// A concrete list viewer over a `Vec<String>` with a `HashSet<i32>` of
    /// selected items, used to drive the draw/nav/sync tests. `ListBox` is the
    /// production consumer.
    struct FakeList {
        lv: ListViewerState,
        items: Vec<String>,
        selected: HashSet<i32>,
    }

    impl FakeList {
        fn new(
            bounds: Rect,
            num_cols: i32,
            items: Vec<String>,
            h: Option<ViewId>,
            v: Option<ViewId>,
        ) -> Self {
            let mut lv = ListViewerState::new(bounds, num_cols, h, v);
            lv.range = items.len() as i32;
            FakeList {
                lv,
                items,
                selected: HashSet::new(),
            }
        }
    }

    impl View for FakeList {
        fn state(&self) -> &ViewState {
            &self.lv.state
        }
        fn state_mut(&mut self) -> &mut ViewState {
            &mut self.lv.state
        }
        fn draw(&mut self, ctx: &mut DrawCtx) {
            draw(self, ctx);
        }
        fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
            handle_event(self, ev, ctx);
        }
        fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
            set_state(self, flag, enable, ctx);
        }
        fn cursor_request(&self) -> Option<Point> {
            focused_cursor(self)
        }
        fn apply_scroll_sync(&mut self, h: Option<i32>, v: Option<i32>, ctx: &mut Context) {
            apply_scroll(self, h, v, ctx);
        }
        fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
            Some(self)
        }
    }

    impl ListViewer for FakeList {
        fn lv(&self) -> &ListViewerState {
            &self.lv
        }
        fn lv_mut(&mut self) -> &mut ListViewerState {
            &mut self.lv
        }
        fn get_text(&self, item: i32) -> String {
            self.items.get(item as usize).cloned().unwrap_or_default()
        }
        fn is_selected(&self, item: i32) -> bool {
            // Honor the explicit selected-set; fall back to the base (== focused).
            self.selected.contains(&item) || item == self.lv.focused
        }
    }

    fn items(n: i32) -> Vec<String> {
        (0..n).map(|i| format!("item{i}")).collect()
    }

    fn make_ctx<'a>(
        out: &'a mut VecDeque<Event>,
        timers: &'a mut crate::timer::TimerQueue,
        deferred: &'a mut Vec<Deferred>,
    ) -> Context<'a> {
        Context::new(out, timers, 0, deferred)
    }

    /// Mint a real `ViewId` by inserting a throwaway view into a group.
    fn mint_id() -> (Group, ViewId) {
        let mut g = Group::new(Rect::new(0, 0, 4, 4));
        let id = g.insert(Box::new(FakeList::new(
            Rect::new(0, 0, 1, 1),
            1,
            vec![],
            None,
            None,
        )));
        (g, id)
    }

    // -- 1. ctor defaults ----------------------------------------------------

    #[test]
    fn ctor_sets_options_and_zeroes_fields() {
        let l = FakeList::new(Rect::new(0, 0, 10, 5), 1, vec![], None, None);
        assert!(l.lv.state.options.first_click, "ofFirstClick set");
        assert!(l.lv.state.options.selectable, "ofSelectable set");
        assert_eq!(l.lv.top_item, 0);
        assert_eq!(l.lv.focused, 0);
        assert_eq!(l.lv.indent, 0);
        assert_eq!(l.lv.num_cols, 1);
        // range is set by the FakeList ctor from items (empty -> 0).
        assert_eq!(l.lv.range, 0);
        // No special broadcast mask bit (broadcasts are always delivered).
        assert_eq!(l.lv.state.event_mask, crate::event::EventMask::default());
    }

    // -- 2. focus_item_num clamp matrix --------------------------------------

    #[test]
    fn focus_item_num_clamps_negative_and_over_range_and_skips_empty() {
        let (_g, v) = mint_id();
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];

        // range 5: clamp -3 -> 0.
        let mut l = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(5), None, Some(v));
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            focus_item_num(&mut l, -3, &mut ctx);
        }
        assert_eq!(l.lv.focused, 0, "negative clamps to 0");

        // clamp 99 -> range-1 = 4.
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            focus_item_num(&mut l, 99, &mut ctx);
        }
        assert_eq!(l.lv.focused, 4, ">= range clamps to range-1");

        // range == 0: focus_item is NOT called (focused stays whatever it was).
        let mut empty = FakeList::new(Rect::new(0, 0, 10, 5), 1, vec![], None, None);
        empty.lv.focused = 7; // a sentinel
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            focus_item_num(&mut empty, 3, &mut ctx);
        }
        assert_eq!(empty.lv.focused, 7, "range==0 -> focus_item skipped");
    }

    // -- 3. focus_item topItem adjust ----------------------------------------

    #[test]
    fn focus_item_single_col_scrolls_top_item_both_directions() {
        // size.y = 5, numCols = 1, 20 items. Scroll down past the bottom, then up.
        let mut l = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(20), None, None);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];

        // Focus item 7: 7 >= topItem(0) + size.y(5)*1 -> topItem = 7 - 5 + 1 = 3.
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            focus_item(&mut l, 7, &mut ctx);
        }
        assert_eq!(l.lv.top_item, 3, "scroll down: topItem = item - size.y + 1");

        // Focus item 1: 1 < topItem(3) -> topItem = item = 1.
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            focus_item(&mut l, 1, &mut ctx);
        }
        assert_eq!(l.lv.top_item, 1, "scroll up: topItem = item");
    }

    #[test]
    fn focus_item_multi_col_scrolls_top_item() {
        // size.y = 3, numCols = 2 -> a page is size.y*numCols = 6 items.
        let mut l = FakeList::new(Rect::new(0, 0, 10, 3), 2, items(40), None, None);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];

        // Focus item 10: 10 >= topItem(0) + 6 -> multi-col:
        //   topItem = item - item%size.y - size.y*(numCols-1)
        //           = 10 - 10%3 - 3*1 = 10 - 1 - 3 = 6.
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            focus_item(&mut l, 10, &mut ctx);
        }
        assert_eq!(l.lv.top_item, 6, "multi-col scroll down");

        // Now focus item 2: 2 < topItem(6) -> multi-col: topItem = item - item%size.y
        //   = 2 - 2%3 = 2 - 2 = 0.
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            focus_item(&mut l, 2, &mut ctx);
        }
        assert_eq!(l.lv.top_item, 0, "multi-col scroll up");
    }

    #[test]
    fn focus_item_queues_v_bar_set_value() {
        let (_g, v) = mint_id();
        let mut l = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(20), None, Some(v));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            focus_item(&mut l, 7, &mut ctx);
        }
        assert_eq!(deferred.len(), 1, "one setValue op");
        assert!(matches!(
            deferred[0],
            Deferred::ScrollBarSetParams { id, value: Some(7), min: None, max: None, page_step: None, arrow_step: None }
                if id == v
        ));
    }

    // -- 4. set_range --------------------------------------------------------

    #[test]
    fn set_range_resets_focused_past_end_and_queues_v_params() {
        let (_g, v) = mint_id();
        let mut l = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(20), None, Some(v));
        l.lv.focused = 15;
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            set_range(&mut l, 10, &mut ctx); // focused 15 >= 10 -> reset to 0
        }
        assert_eq!(l.lv.range, 10);
        assert_eq!(l.lv.focused, 0, "focused reset when >= new range");
        assert_eq!(deferred.len(), 1);
        // setParams(focused=0, 0, aRange-1=9, preserve pg, preserve ar).
        assert!(matches!(
            deferred[0],
            Deferred::ScrollBarSetParams { id, value: Some(0), min: Some(0), max: Some(9), page_step: None, arrow_step: None }
                if id == v
        ));
    }

    #[test]
    fn set_range_keeps_focused_when_in_range() {
        let mut l = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(20), None, None);
        l.lv.focused = 3;
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            set_range(&mut l, 10, &mut ctx);
        }
        assert_eq!(l.lv.focused, 3, "focused kept (still in range)");
        assert!(deferred.is_empty(), "no v-bar -> no params op");
    }

    // -- 5. update_steps -----------------------------------------------------

    #[test]
    fn update_steps_single_col_vbar_and_hbar() {
        let (_g, h) = mint_id();
        let (_g2, v) = mint_id();
        // size 12×5, numCols 1.
        let l = FakeList::new(Rect::new(0, 0, 12, 5), 1, items(5), Some(h), Some(v));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            update_steps(&l, &mut ctx);
        }
        assert_eq!(deferred.len(), 2);
        // v-bar: numCols==1 -> pgStep = size.y-1 = 4, arStep = 1.
        assert!(matches!(
            deferred[0],
            Deferred::ScrollBarSetParams { id, value: None, min: None, max: None, page_step: Some(4), arrow_step: Some(1) }
                if id == v
        ));
        // h-bar: setStep(size.x/numCols = 12, 1).
        assert!(matches!(
            deferred[1],
            Deferred::ScrollBarSetParams { id, page_step: Some(12), arrow_step: Some(1), .. }
                if id == h
        ));
    }

    #[test]
    fn update_steps_multi_col_vbar() {
        let (_g2, v) = mint_id();
        // size 12×4, numCols 3.
        let l = FakeList::new(Rect::new(0, 0, 12, 4), 3, items(5), None, Some(v));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            update_steps(&l, &mut ctx);
        }
        // v-bar: multi -> pgStep = size.y*numCols = 12, arStep = size.y = 4.
        assert!(matches!(
            deferred[0],
            Deferred::ScrollBarSetParams { id, page_step: Some(12), arrow_step: Some(4), .. }
                if id == v
        ));
    }

    // -- 7. handle_event nav / select / scrollbar filter ---------------------

    fn key_ev(k: Key) -> Event {
        Event::KeyDown(crate::event::KeyEvent::new(
            k,
            crate::event::KeyModifiers::default(),
        ))
    }

    #[test]
    fn key_down_and_up_move_focus_and_clear() {
        let mut l = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(20), None, None);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];

        let mut ev = key_ev(Key::Down);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(l.lv.focused, 1, "Down -> focused+1");
        assert!(ev.is_nothing(), "Down consumed");

        let mut ev = key_ev(Key::Up);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(l.lv.focused, 0, "Up -> focused-1");
    }

    #[test]
    fn key_home_end_pgdn_pgup() {
        let mut l = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(50), None, None);
        l.lv.top_item = 10;
        l.lv.focused = 12;
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];

        // Home -> topItem (10).
        let mut ev = key_ev(Key::Home);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(l.lv.focused, 10, "Home -> topItem");

        // End -> topItem + size.y*numCols - 1 = 10 + 5 - 1 = 14.
        let mut ev = key_ev(Key::End);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(l.lv.focused, 14, "End -> topItem + page - 1");

        // PgDn -> focused + size.y*numCols = 14 + 5 = 19.
        let mut ev = key_ev(Key::PageDown);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(l.lv.focused, 19, "PgDn -> +page");

        // PgUp -> focused - page = 19 - 5 = 14.
        let mut ev = key_ev(Key::PageUp);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(l.lv.focused, 14, "PgUp -> -page");
    }

    #[test]
    fn key_ctrl_pgdn_pgup_jump_to_ends() {
        let mut l = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(50), None, None);
        l.lv.focused = 20;
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];

        let ctrl = crate::event::KeyModifiers {
            ctrl: true,
            ..Default::default()
        };
        // Ctrl+PgDn -> range-1 = 49.
        let mut ev = Event::KeyDown(crate::event::KeyEvent::new(Key::PageDown, ctrl));
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(l.lv.focused, 49, "Ctrl+PgDn -> range-1");

        // Ctrl+PgUp -> 0.
        let mut ev = Event::KeyDown(crate::event::KeyEvent::new(Key::PageUp, ctrl));
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(l.lv.focused, 0, "Ctrl+PgUp -> 0");
    }

    #[test]
    fn left_right_no_op_single_col_leaves_event_live() {
        let mut l = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(20), None, None);
        l.lv.focused = 3;
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];

        for k in [Key::Left, Key::Right] {
            let mut ev = key_ev(k);
            {
                let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
                l.handle_event(&mut ev, &mut ctx);
            }
            assert_eq!(l.lv.focused, 3, "{k:?} is a no-op when numCols==1");
            assert!(
                !ev.is_nothing(),
                "{k:?} leaves the event LIVE (C++ return, no clearEvent)"
            );
        }
    }

    #[test]
    fn left_right_move_by_size_y_multi_col() {
        let mut l = FakeList::new(Rect::new(0, 0, 12, 3), 2, items(40), None, None);
        l.lv.focused = 5;
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];

        // Right -> focused + size.y = 5 + 3 = 8.
        let mut ev = key_ev(Key::Right);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(l.lv.focused, 8, "Right -> +size.y (multi-col)");
        assert!(ev.is_nothing(), "Right consumed (multi-col)");

        // Left -> focused - size.y = 8 - 3 = 5.
        let mut ev = key_ev(Key::Left);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(l.lv.focused, 5, "Left -> -size.y (multi-col)");
    }

    #[test]
    fn space_selects_focused_and_broadcasts() {
        // The list must have an id for the broadcast source; insert it.
        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        let id = group.insert(Box::new(FakeList::new(
            Rect::new(0, 0, 10, 5),
            1,
            items(20),
            None,
            None,
        )));
        if let Some(v) = group.find_mut(id) {
            v.state_mut().state.focused = true;
        }
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];

        let mut ev = key_ev(Key::Char(' '));
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            group.find_mut(id).unwrap().handle_event(&mut ev, &mut ctx);
        }
        // selectItem broadcasts cmListItemSelected sourced by the list.
        assert!(
            out.iter().any(|e| matches!(
                e,
                Event::Broadcast { command, source }
                    if *command == Command::LIST_ITEM_SELECTED && *source == Some(id)
            )),
            "Space broadcasts cmListItemSelected with self as source"
        );
        assert!(ev.is_nothing(), "Space consumed");
    }

    #[test]
    fn double_click_selects_item_under_cursor() {
        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        let id = group.insert(Box::new(FakeList::new(
            Rect::new(0, 0, 10, 5),
            1,
            items(20),
            None,
            None,
        )));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];

        // Double-click at view-local (3, 2): newItem = 2 + 5*(3/11) + 0 = 2.
        let me = crate::event::MouseEvent {
            position: Point::new(3, 2),
            flags: crate::event::MouseEventFlags {
                double_click: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut ev = Event::MouseDown(me);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            group.find_mut(id).unwrap().handle_event(&mut ev, &mut ctx);
        }
        // focusItemNum(2) -> focused 2; double-click + range > 2 -> selectItem(2).
        let focused = group
            .find_mut(id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<FakeList>())
            .map(|l| l.lv.focused)
            .unwrap();
        assert_eq!(focused, 2, "click positioned focus to item 2");
        assert!(
            out.iter().any(|e| matches!(
                e,
                Event::Broadcast { command, .. } if *command == Command::LIST_ITEM_SELECTED
            )),
            "double-click selects -> cmListItemSelected"
        );
        assert!(ev.is_nothing(), "mouse-down consumed");
    }

    #[test]
    fn scrollbar_changed_filter_requests_sync_only_for_own_bars() {
        let mut group = Group::new(Rect::new(0, 0, 30, 20));
        let (_gh, h) = mint_id();
        let (_gv, v) = mint_id();
        let id = group.insert(Box::new(FakeList::new(
            Rect::new(0, 0, 10, 5),
            1,
            items(20),
            Some(h),
            Some(v),
        )));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        // (a) CHANGED from own h-bar -> ScrollSync queued.
        let mut ev = Event::Broadcast {
            command: Command::SCROLL_BAR_CHANGED,
            source: Some(h),
        };
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            group.find_mut(id).unwrap().handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(deferred.len(), 1);
        assert!(matches!(
            deferred[0],
            Deferred::ScrollSync { target, h: rh, v: rv }
                if target == id && rh == Some(h) && rv == Some(v)
        ));

        // (b) CHANGED from a foreign source -> nothing.
        deferred.clear();
        let mut ev = Event::Broadcast {
            command: Command::SCROLL_BAR_CHANGED,
            source: Some(id),
        };
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            group.find_mut(id).unwrap().handle_event(&mut ev, &mut ctx);
        }
        assert!(deferred.is_empty(), "foreign source ignored (filter bites)");

        // (c) CLICKED from own v-bar -> FocusById queued (select()).
        let mut ev = Event::Broadcast {
            command: Command::SCROLL_BAR_CLICKED,
            source: Some(v),
        };
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            group.find_mut(id).unwrap().handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(deferred.len(), 1);
        assert!(matches!(deferred[0], Deferred::FocusById(fid) if fid == id));
    }

    // -- apply_scroll body ---------------------------------------------------

    #[test]
    fn apply_scroll_h_updates_indent_v_focuses() {
        let (_g, v) = mint_id();
        let mut l = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(20), None, Some(v));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            apply_scroll(&mut l, Some(4), Some(8), &mut ctx);
        }
        assert_eq!(l.lv.indent, 4, "h branch refreshes the cached indent");
        assert_eq!(l.lv.focused, 8, "v branch focusItemNum(8)");
    }

    // -- focused_cursor (the &self setCursor successor) -----------------------

    #[test]
    fn focused_cursor_visible_and_offscreen() {
        // size 16×3, numCols 2 -> colWidth = 16/2 + 1 = 9. Items lay column-major:
        //   item = j*size.y + i + top_item.
        // With top_item = 0: col0 = items 0,1,2 at rows 0,1,2; col1 = items 3,4,5.
        let mut l = FakeList::new(Rect::new(0, 0, 16, 3), 2, items(20), None, None);
        l.lv.state.state.selected = true;
        l.lv.state.state.active = true;

        // (a) focused item 4 is in col j=1, row i=1 (4 = 1*3 + 1 + 0). Visible.
        //   x = j*col_width + 1 = 1*9 + 1 = 10; y = i = 1.
        // A column-major bug (e.g. row-major item = i*numCols + j) would put item 4
        // at a different cell, so the (10, 1) assertion bites.
        l.lv.focused = 4;
        assert_eq!(
            focused_cursor(&l),
            Some(Point::new(10, 1)),
            "focused item 4 -> col1 row1 -> view-local (10, 1)"
        );

        // (b) focused item 2 is col0 row2 -> (1, 2).
        l.lv.focused = 2;
        assert_eq!(
            focused_cursor(&l),
            Some(Point::new(1, 2)),
            "focused item 2 -> col0 row2 -> view-local (1, 2)"
        );

        // (c) focused item scrolled BELOW the visible page (a page = size.y*numCols
        //   = 6 items; with top_item 0 the visible items are 0..=5). Item 9 is off.
        l.lv.focused = 9;
        assert_eq!(focused_cursor(&l), None, "focused below page -> None");

        // (d) focused item scrolled ABOVE top_item.
        l.lv.top_item = 6; // visible items now 6..=11
        l.lv.focused = 3; // 3 < top_item -> not in the visible grid
        assert_eq!(focused_cursor(&l), None, "focused above top_item -> None");
    }

    // -- 6. draw snapshots ---------------------------------------------------

    fn render(l: &mut FakeList, w: u16, h: u16) -> String {
        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(w, h);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = l.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            l.draw(&mut dc);
        });
        screen.snapshot()
    }

    #[test]
    fn snapshot_single_col_active_focused_and_selected() {
        let mut l = FakeList::new(Rect::new(0, 0, 12, 4), 1, items(3), None, None);
        // Focused list (selected + active + focused) so focused/selected colors show.
        l.lv.state.state.selected = true;
        l.lv.state.state.active = true;
        l.lv.state.state.focused = true;
        l.lv.focused = 1;
        l.selected.insert(2); // item 2 explicitly selected
        insta::assert_snapshot!(render(&mut l, 12, 4));
    }

    #[test]
    fn snapshot_multi_col() {
        // size 16×3, numCols 2 -> colWidth = 16/2 + 1 = 9. 8 items laid column-
        // major: col0 = items 0,1,2; col1 = items 3,4,5.
        let mut l = FakeList::new(Rect::new(0, 0, 16, 3), 2, items(8), None, None);
        l.lv.state.state.selected = true;
        l.lv.state.state.active = true;
        l.lv.state.state.focused = true;
        insta::assert_snapshot!(render(&mut l, 16, 3));
    }

    #[test]
    fn snapshot_empty_shows_placeholder() {
        let mut l = FakeList::new(Rect::new(0, 0, 12, 3), 1, vec![], None, None);
        l.lv.state.state.selected = true;
        l.lv.state.state.active = true;
        insta::assert_snapshot!(render(&mut l, 12, 3));
    }

    #[test]
    fn snapshot_find_highlights_first_match() {
        let mut fake = FakeList::new(
            Rect::new(0, 0, 14, 4),
            1,
            vec!["banana".into(), "orange".into(), "grape".into()],
            None,
            None,
        );
        fake.lv.state.state.selected = true;
        fake.lv.state.state.active = true;
        fake.lv.state.state.focused = true;
        fake.lv.range = 3;
        fake.lv.find_mode = FindMode::Highlight;
        fake.lv.query = "an".into();
        insta::assert_snapshot!(render(&mut fake, 14, 4));
    }

    #[test]
    fn snapshot_find_empty_shows_no_match_placeholder() {
        let mut fake = FakeList::new(Rect::new(0, 0, 16, 3), 1, vec![], None, None);
        fake.lv.state.state.selected = true;
        fake.lv.state.state.active = true;
        fake.lv.range = 0;
        fake.lv.find_mode = FindMode::Filter;
        fake.lv.query = "xyz".into();
        insta::assert_snapshot!(render(&mut fake, 16, 3));
    }

    // -- active-aware surfaces: three-surface draw (Task 3) ------------------
    //
    // The single `selected && active` predicate is split into two independent
    // axes: the row SURFACE (the three-surface rule — `ctx.owner_active()`
    // crossed with this list's own focus and selectability, see
    // `DrawCtx::content_surface`) and the item HIGHLIGHT (`state.focused` —
    // am I, this specific list, the focused control?). These tests prove each
    // axis in isolation, then the real end-to-end splitter/shuttle scenario.

    /// Row background at item index 1 of a 2-item list — never the cursor
    /// item (index 0, which the base `is_selected` always marks) and never
    /// the highlight-axis focused cell, so it isolates the row SURFACE color.
    fn surface_bg_at(theme: &Theme, owner_active: bool, list_focused: bool) -> crate::color::Color {
        let mut l = FakeList::new(Rect::new(0, 0, 10, 2), 1, items(2), None, None);
        l.lv.state.state.selected = true;
        l.lv.state.state.active = true;
        l.lv.state.state.focused = list_focused;
        let bounds = l.state().get_bounds();
        let mut buf = Buffer::new(10, 2);
        let mut dc = DrawCtx::new(&mut buf, theme, bounds, bounds.a);
        dc.set_owner_active(owner_active);
        l.draw(&mut dc);
        buf.get(1, 1).style().bg
    }

    /// Surface axis: with a theme where `ListNormal`/`ListSurface`/
    /// `ListInactive` are all distinct, a selectable list's row surface
    /// follows the three-surface rule: `owner_active == false` recedes to
    /// `ListInactive` regardless of the list's own focus; within an active
    /// pane, the list's own `state.focused` picks well (`ListNormal`) vs.
    /// surface (`ListSurface`, "a selectable sibling holds focus").
    #[test]
    fn surface_axis_tracks_owner_active_not_own_state() {
        use crate::color::{Color, Style};

        let mut theme = Theme::classic_blue();
        // classic_blue collapses ListSurface onto ListNormal — override both
        // non-normal roles here so all three differ and each state paints a
        // distinct color.
        theme.set_style(
            Role::ListSurface,
            Style::new(Color::bios_rgb(0xA), Color::bios_rgb(0x5)),
        );
        theme.set_style(
            Role::ListInactive,
            Style::new(Color::bios_rgb(0xE), Color::bios_rgb(0x4)),
        );
        let normal_bg = theme.style(Role::ListNormal).bg;
        let surface_bg = theme.style(Role::ListSurface).bg;
        let inactive_bg = theme.style(Role::ListInactive).bg;
        assert!(
            normal_bg != surface_bg && surface_bg != inactive_bg && normal_bg != inactive_bg,
            "test theme must distinguish all three roles"
        );

        // owner_active == false → inactive, regardless of the list's own focus.
        assert_eq!(surface_bg_at(&theme, false, true), inactive_bg);
        assert_eq!(surface_bg_at(&theme, false, false), inactive_bg);

        // owner_active == true, list focused → the well.
        assert_eq!(surface_bg_at(&theme, true, true), normal_bg);

        // owner_active == true, list unfocused, selectable → the pane surface.
        assert_eq!(surface_bg_at(&theme, true, false), surface_bg);
    }

    /// Non-selectable fixture: a list with `options.selectable = false` can
    /// never win (or lose) a focus contest, so it always stays the well in an
    /// active pane — never `ListSurface` — and still recedes to `ListInactive`
    /// in an inactive pane.
    #[test]
    fn non_selectable_list_never_shows_surface() {
        use crate::color::{Color, Style};

        let mut theme = Theme::classic_blue();
        theme.set_style(
            Role::ListSurface,
            Style::new(Color::bios_rgb(0xA), Color::bios_rgb(0x5)),
        );
        theme.set_style(
            Role::ListInactive,
            Style::new(Color::bios_rgb(0xE), Color::bios_rgb(0x4)),
        );
        let normal_bg = theme.style(Role::ListNormal).bg;
        let inactive_bg = theme.style(Role::ListInactive).bg;

        let bg_at = |owner_active: bool, list_focused: bool| {
            let mut l = FakeList::new(Rect::new(0, 0, 10, 2), 1, items(2), None, None);
            l.lv.state.state.selected = true;
            l.lv.state.state.active = true;
            l.lv.state.state.focused = list_focused;
            l.lv.state.options.selectable = false;
            let bounds = l.state().get_bounds();
            let mut buf = Buffer::new(10, 2);
            let mut dc = DrawCtx::new(&mut buf, &theme, bounds, bounds.a);
            dc.set_owner_active(owner_active);
            l.draw(&mut dc);
            buf.get(1, 1).style().bg
        };

        // Active pane: never ListSurface, regardless of own focus.
        assert_eq!(bg_at(true, false), normal_bg);
        assert_eq!(bg_at(true, true), normal_bg);
        // Inactive pane: still recedes to ListInactive.
        assert_eq!(bg_at(false, false), inactive_bg);
    }

    /// Highlight axis: with `ctx.owner_active()` held `true` both times (the
    /// "sibling in the focused pane" case), the current row is drawn bright
    /// (`ListFocused`) iff the list's OWN `state.focused` is true — being in a
    /// focused pane is not enough if this particular list isn't the focused
    /// control.
    #[test]
    fn highlight_axis_tracks_own_state_focused_not_owner_active() {
        let render_with = |list_focused: bool| {
            let mut l = FakeList::new(Rect::new(0, 0, 10, 2), 1, items(2), None, None);
            l.lv.state.state.selected = true;
            l.lv.state.state.active = true;
            l.lv.state.state.focused = list_focused;
            l.lv.focused = 0;
            let theme = Theme::classic_blue();
            let bounds = l.state().get_bounds();
            let mut buf = Buffer::new(10, 2);
            let mut dc = DrawCtx::new(&mut buf, &theme, bounds, bounds.a);
            dc.set_owner_active(true); // owning pane IS focused in both renders
            l.draw(&mut dc);
            crate::screen::snapshot::snapshot(&buf, None)
        };

        let focused_list = render_with(true);
        let unfocused_sibling = render_with(false);
        assert_ne!(
            focused_list, unfocused_sibling,
            "current-row highlight must track the list's own state.focused, \
             not merely owner_active"
        );
    }

    /// End-to-end: a splitter Group holding two pane Groups, each holding a
    /// list with an identical current item. Pane A is focused, pane B is not
    /// (but pane B's list is still `selected && active`, reproducing the old
    /// conflated predicate's trigger). Only pane A's list may show its current
    /// row in the bright `ListFocused` color; pane B's current row falls back
    /// to `ListSelected` (a single-select list's current item is always its
    /// own "selected" row — see `FakeList::is_selected` — but never the bright
    /// focused color unless that specific list has keyboard focus).
    #[test]
    fn list_current_item_bright_only_in_the_focused_pane() {
        let mut list_a = FakeList::new(Rect::new(0, 0, 8, 2), 1, items(2), None, None);
        list_a.lv.state.state.selected = true;
        list_a.lv.state.state.active = true;
        list_a.lv.state.state.focused = true; // pane A's list IS the focused control
        list_a.lv.focused = 0;

        let mut list_b = FakeList::new(Rect::new(0, 0, 8, 2), 1, items(2), None, None);
        list_b.lv.state.state.selected = true;
        list_b.lv.state.state.active = true;
        list_b.lv.state.state.focused = false; // current in pane B, but not focused
        list_b.lv.focused = 0;

        let mut pane_a = Group::new(Rect::new(0, 0, 8, 2));
        pane_a.insert(Box::new(list_a));
        pane_a.state_mut().state.focused = true; // pane A has keyboard focus

        let mut pane_b = Group::new(Rect::new(9, 0, 17, 2));
        pane_b.insert(Box::new(list_b));
        pane_b.state_mut().state.focused = false; // pane B does not

        let mut splitter = Group::new(Rect::new(0, 0, 17, 2));
        splitter.insert(Box::new(pane_a));
        splitter.insert(Box::new(pane_b));

        let theme = Theme::classic_blue();
        let mut buf = Buffer::new(17, 2);
        let mut dc = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 17, 2), Point::new(0, 0));
        splitter.draw(&mut dc);
        insta::assert_snapshot!(crate::screen::snapshot::snapshot(&buf, None));
    }

    // -- mouse-track: ListViewer ----------------------------------------------
    //
    // These tests drive the tracking arms directly (as the pump's
    // Deferred::MouseTrack does), verifying that the MouseDown arms capture with
    // the view-id payload, MouseMove/MouseAuto recompute focus, MouseAuto steps
    // out-of-view after the skip count, and MouseUp clears the track.

    fn mouse_down_at(x: i32, y: i32) -> Event {
        Event::MouseDown(crate::event::MouseEvent {
            position: Point::new(x, y),
            buttons: crate::event::MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        })
    }

    fn mouse_move_at(x: i32, y: i32) -> Event {
        Event::MouseMove(crate::event::MouseEvent {
            position: Point::new(x, y),
            buttons: crate::event::MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        })
    }

    fn mouse_auto_at(x: i32, y: i32) -> Event {
        Event::MouseAuto(crate::event::MouseEvent {
            position: Point::new(x, y),
            ..Default::default()
        })
    }

    fn mouse_up_at(x: i32, y: i32) -> Event {
        Event::MouseUp(crate::event::MouseEvent {
            position: Point::new(x, y),
            ..Default::default()
        })
    }

    fn double_click_at(x: i32, y: i32) -> Event {
        Event::MouseDown(crate::event::MouseEvent {
            position: Point::new(x, y),
            flags: crate::event::MouseEventFlags {
                double_click: true,
                ..Default::default()
            },
            buttons: crate::event::MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        })
    }

    /// Helper: stamp the list with a fresh ViewId (as Group::insert would do).
    fn give_id(l: &mut FakeList) -> ViewId {
        let id = ViewId::next();
        l.lv.state.id = Some(id);
        id
    }

    /// `MouseDown` (non-double-click) on an inserted list: arms tracking with
    /// the correct view-id payload in the PushCapture deferred.
    #[test]
    fn mouse_down_arms_tracking_and_pushes_capture() {
        // 10×5 single-col list, 20 items. Click at (3, 2) → newItem = 2.
        let mut l = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(20), None, None);
        let id = give_id(&mut l);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        let mut ev = mouse_down_at(3, 2);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "MouseDown consumed");
        assert_eq!(l.lv.focused, 2, "focus positioned to item 2");
        assert!(l.lv.track.is_some(), "track state armed");
        // The PushCapture deferred must name this list's id.
        assert_eq!(deferred.len(), 1, "one PushCapture deferred");
        assert!(
            matches!(deferred[0], Deferred::PushCapture(_)),
            "deferred[0] is PushCapture"
        );
        if let Deferred::PushCapture(ref h) = deferred[0] {
            assert_eq!(h.view(), Some(id), "capture tracks the list's id");
        }
    }

    /// `MouseDown` without an id (uninserted list): single-shot behavior,
    /// no tracking, no capture — faithful fallback.
    #[test]
    fn mouse_down_without_id_is_single_shot() {
        let mut l = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(20), None, None);
        // No id assigned.
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        let mut ev = mouse_down_at(3, 2);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert!(l.lv.track.is_none(), "no track without an id");
        assert!(deferred.is_empty(), "no capture pushed for id-less list");
    }

    /// Double-click: positions + selects immediately; no tracking armed.
    #[test]
    fn double_click_selects_and_does_not_arm_tracking() {
        let mut g = Group::new(Rect::new(0, 0, 20, 10));
        let id = g.insert(Box::new(FakeList::new(
            Rect::new(0, 0, 10, 5),
            1,
            items(20),
            None,
            None,
        )));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        let mut ev = double_click_at(3, 2);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            g.find_mut(id).unwrap().handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "double-click consumed");
        // No PushCapture — the loop broke immediately on double-click.
        assert!(
            deferred
                .iter()
                .all(|d| !matches!(d, Deferred::PushCapture(_))),
            "double-click does NOT arm tracking"
        );
        // selectItem was called (cmListItemSelected broadcast).
        assert!(
            out.iter().any(|e| matches!(e,
                Event::Broadcast { command, .. } if *command == Command::LIST_ITEM_SELECTED
            )),
            "double-click selects (cmListItemSelected)"
        );
    }

    /// `MouseMove` while tracking (in-view): recomputes item + updates focus.
    #[test]
    fn mouse_move_in_view_recomputes_focus() {
        let mut l = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(20), None, None);
        give_id(&mut l);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        // Arm tracking with a MouseDown at item 0.
        let mut ev = mouse_down_at(0, 0);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(l.lv.focused, 0);
        deferred.clear();

        // MouseMove to row 3 (item 3 in single-col, colWidth=11).
        let mut ev = mouse_move_at(2, 3);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "MouseMove consumed");
        assert_eq!(l.lv.focused, 3, "focus moves with the mouse");
        assert!(l.lv.track.is_some(), "still tracking after move");
    }

    /// `MouseMove` outside the view (tracking): no-op (C++ only reacts to
    /// evMouseAuto for out-of-view scrolling, not evMouseMove).
    #[test]
    fn mouse_move_out_of_view_is_noop() {
        let mut l = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(20), None, None);
        give_id(&mut l);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        let mut ev = mouse_down_at(0, 2);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        let focused_after_down = l.lv.focused;
        deferred.clear();

        // Move outside (y = -1): no focus change.
        let mut ev = mouse_move_at(0, -1);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(
            l.lv.focused, focused_after_down,
            "out-of-view MouseMove does not change focus"
        );
    }

    /// `MouseAuto` in-view (tracking): recomputes item just like MouseMove.
    #[test]
    fn mouse_auto_in_view_recomputes_focus() {
        let mut l = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(20), None, None);
        give_id(&mut l);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        let mut ev = mouse_down_at(0, 0);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        deferred.clear();

        // MouseAuto at row 4 (in-view): item 4.
        let mut ev = mouse_auto_at(0, 4);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "MouseAuto consumed");
        assert_eq!(l.lv.focused, 4, "in-view MouseAuto repositions focus");
    }

    /// `MouseAuto` out-of-view (single-col, tracking): skips the first 3 ticks
    /// (count 1, 2, 3), then on the 4th tick steps the focused item by +1.
    #[test]
    fn mouse_auto_out_of_view_skips_then_steps_single_col() {
        let mut l = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(20), None, None);
        give_id(&mut l);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        // Focus item 2.
        let mut ev = mouse_down_at(0, 2);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(l.lv.focused, 2);
        deferred.clear();

        // 3 ticks below the view (y >= size.y = 5): count reaches 1, 2, 3 — no step.
        for tick in 1..=3 {
            let mut ev = mouse_auto_at(0, 7); // y=7 >= size.y=5
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
            assert_eq!(
                l.lv.focused, 2,
                "tick {tick}: focus stays at 2 before threshold"
            );
        }
        // 4th tick: count == MOUSE_AUTOS_TO_SKIP (4) → step forward.
        let mut ev = mouse_auto_at(0, 7);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(l.lv.focused, 3, "4th tick steps focus by +1 (below view)");
        // count is reset to 0: next 3 ticks should not step.
        for tick in 1..=3 {
            let mut ev = mouse_auto_at(0, 7);
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
            assert_eq!(l.lv.focused, 3, "after reset, tick {tick}: no step");
        }
    }

    /// `MouseAuto` out-of-view above (single-col): steps backward on tick 4.
    #[test]
    fn mouse_auto_out_of_view_above_steps_backward() {
        let mut l = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(20), None, None);
        give_id(&mut l);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        // Focus item 5.
        let mut ev = mouse_down_at(0, 2);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        l.lv.focused = 5;
        l.lv.track = Some(LvTrack {
            count: 0,
            old_item: 5,
        });
        deferred.clear();

        // 4 ticks above (y < 0): step back by 1.
        for _ in 1..=3 {
            let mut ev = mouse_auto_at(0, -1);
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(l.lv.focused, 5, "first 3 ticks: no step");
        let mut ev = mouse_auto_at(0, -1);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(l.lv.focused, 4, "4th tick steps focus by -1 (above view)");
    }

    /// `MouseUp` while tracking: clears track, re-focuses current item.
    #[test]
    fn mouse_up_clears_track_and_refocuses() {
        let mut l = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(20), None, None);
        give_id(&mut l);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        // Arm tracking.
        let mut ev = mouse_down_at(0, 3);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert!(l.lv.track.is_some());
        deferred.clear();

        // MouseUp: clears track.
        let mut ev = mouse_up_at(0, 3);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "MouseUp consumed");
        assert!(l.lv.track.is_none(), "track cleared on MouseUp");
    }

    /// Stray `MouseUp` (not tracking) falls through unconsumed — the mandatory
    /// tracking-arm guard (MouseUp is not mask-gated in Group::wants).
    #[test]
    fn stray_mouse_up_falls_through() {
        let mut l = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(20), None, None);
        give_id(&mut l);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        // No tracking armed.
        let mut ev = mouse_up_at(0, 3);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert!(
            !ev.is_nothing(),
            "stray MouseUp falls through (not consumed)"
        );
    }

    /// Stray `MouseMove` (not tracking) falls through unconsumed.
    #[test]
    fn stray_mouse_move_falls_through() {
        let mut l = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(20), None, None);
        give_id(&mut l);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        let mut ev = mouse_move_at(3, 2);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert!(
            !ev.is_nothing(),
            "stray MouseMove falls through (not consumed)"
        );
    }

    /// `MouseAuto` out-of-view (multi-col): steps by ±size.y in the column
    /// direction when outside column bounds.
    #[test]
    fn mouse_auto_multi_col_steps_by_size_y() {
        // size 12×4, numCols 2 → colWidth = 7. size.y = 4.
        let mut l = FakeList::new(Rect::new(0, 0, 12, 4), 2, items(40), None, None);
        give_id(&mut l);
        l.lv.focused = 8;
        l.lv.track = Some(LvTrack {
            count: 0,
            old_item: 8,
        });
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        // 4 ticks with x >= size.x (12): should step +size.y = +4 on tick 4.
        for _ in 1..=3 {
            let mut ev = mouse_auto_at(13, 2); // x=13 >= size.x=12
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(l.lv.focused, 8, "3 ticks: no step yet");
        let mut ev = mouse_auto_at(13, 2);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(l.lv.focused, 12, "4th tick: +size.y = +4 (right of view)");
    }

    // -- set_state sfVisible hides scroll bars ---------------------------------

    /// Clearing `sfVisible` on an active+visible list viewer enqueues
    /// `SetVisible(_, false)` for both scroll bars.
    #[test]
    fn set_state_visible_false_hides_both_scroll_bars() {
        let (_gh, h) = mint_id();
        let (_gv, v) = mint_id();

        // Construct active+visible so bars would normally be shown.
        let mut l = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(5), Some(h), Some(v));
        // Manually set active+visible in the state flags so the inner body sees
        // them.  We set the bits directly on the ViewState to avoid needing a
        // full Group context for set_state(Active/Selected).
        l.lv.state.state.active = true;
        l.lv.state.state.visible = true;

        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        // Now hide the view via sfVisible — bars should be enqueued for hiding.
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            l.set_state(StateFlag::Visible, false, &mut ctx);
        }

        // Exactly two SetVisible(_, false) ops, one per bar.
        assert_eq!(
            deferred.len(),
            2,
            "expected 2 SetVisible ops, got {}",
            deferred.len()
        );
        assert!(
            matches!(deferred[0], Deferred::SetVisible(id, false) if id == h),
            "first op must be SetVisible(h_scroll_bar, false)"
        );
        assert!(
            matches!(deferred[1], Deferred::SetVisible(id, false) if id == v),
            "second op must be SetVisible(v_scroll_bar, false)"
        );
    }

    #[test]
    fn find_query_reflects_mode_and_emptiness() {
        let mut fake = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(3), None, None);
        assert_eq!(fake.find_query(), None, "Off → None");
        fake.lv.find_mode = FindMode::Highlight;
        assert_eq!(fake.find_query(), None, "empty query → None");
        fake.lv.query = "ab".into();
        assert_eq!(fake.find_query(), Some("ab"));
        fake.lv.find_mode = FindMode::Off;
        assert_eq!(fake.find_query(), None, "Off overrides a non-empty query");
    }

    // -- set_find_query (external find input) ---------------------------------

    /// Count LIST_FIND_CHANGED broadcasts in an out-queue, and return the last
    /// broadcast's `source` if any.
    fn find_broadcasts(out: &VecDeque<Event>) -> (usize, Option<ViewId>) {
        let mut count = 0;
        let mut last_source = None;
        for e in out.iter() {
            if let Event::Broadcast { command, source } = e
                && *command == Command::LIST_FIND_CHANGED
            {
                count += 1;
                last_source = *source;
            }
        }
        (count, last_source)
    }

    #[test]
    fn set_find_query_sets_query_and_broadcasts_once() {
        let mut fake = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(3), None, None);
        fake.lv.find_mode = FindMode::Highlight;
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.set_find_query("ab", &mut ctx);
        }
        assert_eq!(fake.find_query(), Some("ab"), "query is set");
        let (count, source) = find_broadcasts(&out);
        assert_eq!(count, 1, "exactly one LIST_FIND_CHANGED");
        assert_eq!(source, fake.state().id(), "source = this list's id");
    }

    #[test]
    fn set_find_query_same_text_is_a_noop() {
        let mut fake = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(3), None, None);
        fake.lv.find_mode = FindMode::Highlight;
        fake.lv.query = "ab".into();
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.set_find_query("ab", &mut ctx);
        }
        assert_eq!(fake.lv.query, "ab", "unchanged");
        assert_eq!(
            find_broadcasts(&out).0,
            0,
            "no broadcast on the change guard"
        );
    }

    #[test]
    fn set_find_query_empty_clears_like_clear_find() {
        let mut fake = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(3), None, None);
        fake.lv.find_mode = FindMode::Highlight;
        fake.lv.query = "ab".into();
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.set_find_query("", &mut ctx);
        }
        assert_eq!(fake.find_query(), None, "empty query reads as None");
        assert_eq!(fake.lv.query, "", "query emptied");
        assert_eq!(find_broadcasts(&out).0, 1, "empties and notifies once");
    }

    #[test]
    fn set_find_query_off_mode_is_total_noop() {
        let mut fake = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(3), None, None);
        // find_mode defaults to Off.
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.set_find_query("x", &mut ctx);
        }
        assert_eq!(fake.lv.query, "", "Off: query untouched");
        assert_eq!(find_broadcasts(&out).0, 0, "Off: no broadcast");
    }

    #[test]
    fn clear_find_delegates_to_set_find_query() {
        let mut fake = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(3), None, None);
        fake.lv.find_mode = FindMode::Highlight;
        fake.lv.query = "ab".into();
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.clear_find(&mut ctx);
        }
        assert_eq!(fake.find_query(), None, "clear_find still empties");
        assert_eq!(find_broadcasts(&out).0, 1, "clear_find still notifies once");

        // Already-empty clear_find is a no-op (the folded guard still holds).
        out.clear();
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.clear_find(&mut ctx);
        }
        assert_eq!(
            find_broadcasts(&out).0,
            0,
            "already-empty clear_find is a no-op"
        );
    }

    #[test]
    fn find_match_first_occurrence_char_indexed() {
        assert_eq!(find_match("banana", "an"), Some((1, 3)), "middle");
        assert_eq!(find_match("banana", "ba"), Some((0, 2)), "start");
        assert_eq!(find_match("banana", "na"), Some((2, 4)), "first of repeats");
        assert_eq!(
            find_match("Banana", "an"),
            Some((1, 3)),
            "case-insensitive text"
        );
        assert_eq!(
            find_match("banana", "BAN"),
            Some((0, 3)),
            "case-insensitive query"
        );
        assert_eq!(find_match("banana", "xyz"), None, "no match");
        assert_eq!(find_match("ab", ""), None, "empty query");
        assert_eq!(find_match("a", "abc"), None, "query longer than text");
        assert_eq!(
            find_match("café", "é"),
            Some((3, 4)),
            "multibyte char index"
        );
        assert_eq!(find_match("naïve", "ï"), Some((2, 3)), "multibyte mid-word");
    }

    #[test]
    fn filtered_view_narrows_only_in_filter_mode() {
        let src = vec!["apple".to_string(), "banana".into(), "orange".into()];
        assert_eq!(
            filtered_view(&src, FindMode::Off, "an"),
            src,
            "Off: full source"
        );
        assert_eq!(
            filtered_view(&src, FindMode::Highlight, "an"),
            src,
            "Highlight: full source"
        );
        assert_eq!(
            filtered_view(&src, FindMode::Filter, ""),
            src,
            "empty query: full source"
        );
        assert_eq!(
            filtered_view(&src, FindMode::Filter, "an"),
            vec!["banana".to_string(), "orange".into()],
            "Filter narrows, order preserved"
        );
    }

    // -- find key routing (Task 3) -------------------------------------------

    #[test]
    fn find_mode_accumulates_query_and_broadcasts_on_change() {
        let mut fake = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(5), None, None);
        fake.lv.find_mode = FindMode::Highlight;
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];
        for c in ['a', 'b', ' ', 'c'] {
            let mut ev = key_ev(Key::Char(c));
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.handle_event(&mut ev, &mut ctx);
            assert!(ev.is_nothing(), "find key consumed");
        }
        assert_eq!(fake.lv.query, "ab c", "Space appends to the query");
        let broadcasts = out
            .iter()
            .filter(|e| {
                matches!(e,
                Event::Broadcast { command, .. } if *command == Command::LIST_FIND_CHANGED)
            })
            .count();
        assert_eq!(broadcasts, 4, "one broadcast per query change");
    }

    #[test]
    fn find_backspace_and_esc_behaviour() {
        let mut fake = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(5), None, None);
        fake.lv.find_mode = FindMode::Highlight;
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];

        // Backspace pops; consumed.
        fake.lv.query = "ab".into();
        {
            let mut ev = key_ev(Key::Backspace);
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.handle_event(&mut ev, &mut ctx);
            assert!(ev.is_nothing());
        }
        assert_eq!(fake.lv.query, "a");

        // Backspace on empty: no change, no broadcast, NOT consumed (propagates).
        fake.lv.query.clear();
        out.clear();
        {
            let mut ev = key_ev(Key::Backspace);
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.handle_event(&mut ev, &mut ctx);
            assert!(!ev.is_nothing(), "empty Backspace propagates");
        }
        assert!(out.is_empty(), "no broadcast on a no-op Backspace");

        // Esc with a query: clears + consumes.
        fake.lv.query = "ab".into();
        {
            let mut ev = key_ev(Key::Esc);
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.handle_event(&mut ev, &mut ctx);
            assert!(ev.is_nothing(), "Esc with a query is consumed");
        }
        assert_eq!(fake.lv.query, "");

        // Esc with empty query: propagates (a host dialog can still close).
        {
            let mut ev = key_ev(Key::Esc);
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.handle_event(&mut ev, &mut ctx);
            assert!(!ev.is_nothing(), "empty-query Esc propagates");
        }
    }

    #[test]
    fn find_mode_lets_arrows_navigate_and_keeps_query() {
        let mut fake = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(5), None, None);
        fake.lv.find_mode = FindMode::Highlight;
        fake.lv.query = "a".into();
        fake.lv.focused = 0;
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];
        let mut ev = key_ev(Key::Down);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(fake.lv.focused, 1, "Down still navigates in find mode");
        assert_eq!(fake.lv.query, "a", "query persists across navigation");
    }
}
