//! The `View` trait + `ViewState` — `TView` ported per deviations **D2/D5**.
//!
//! `TView` is the root of TV's view hierarchy. D2 replaces inheritance with a
//! [`View`] **trait** plus a [`ViewState`] **composition target**: every widget
//! embeds a `ViewState` (TV's data members) and implements `View` (TV's virtual
//! methods). D5 turns TV's packed flag words (`sf*`/`of*`/`gf*`/`dm*`) into
//! [`State`]/[`Options`]/[`GrowMode`]/[`DragMode`] **structs-of-bools**, and the
//! `set*`/verb methods into plain field flips / small helpers.
//!
//! # What lives here (row 23) vs. elsewhere
//!
//! This module is the *abstract base*. Several `TView` methods have no home here
//! because the data they need does not exist at a bare view:
//!
//! * **Up-tree / owner operations relocate to `TGroup` (row 26)** — `focus`,
//!   `select`, `setCurrent`, sibling nav (`next`/`prev`/`makeFirst`/
//!   `putInFrontOf`/`TopView`), and the coordinate transforms
//!   (`makeGlobal`/`makeLocal`/`mouseInView`/`containsMouse`). D3 forbids
//!   up-pointers, so these are driven *top-down* by the group.
//!
//! * **`TView::handleEvent` is a no-op base here.** Its only body in C++ is the
//!   mouse-down auto-select, which calls the up-tree `focus()` — so it relocates
//!   to the group. Breadcrumb for row 26, verbatim:
//!
//!   > Row 26 (TGroup) must, on a mouse-down delivered to the top-most
//!   > `ofSelectable` child that is **not already `sfSelected` and not
//!   > `sfDisabled`**, select that child and pass the event through iff
//!   > (`options.first_click` AND focus succeeded), else consume it
//!   > (`ev.clear()`). This is the relocated body of `TView::handleEvent`
//!   > (`tview.cpp`: `if(!(state & (sfSelected | sfDisabled)) && (options &
//!   > ofSelectable))`); no row-23 test covers it.
//!
//! * **The `sfFocused` focus broadcast** (`cmReceivedFocus`/`cmReleasedFocus`,
//!   `setState` case `sfFocused`) is fired by the row-26 focus logic via
//!   `ctx.broadcast`, **not** by any base method here.
//!
//! * **Already provided elsewhere:** `setTimer`/`killTimer` →
//!   [`Context::set_timer`](crate::view::Context::set_timer) /
//!   [`kill_timer`](crate::view::Context::kill_timer); `getColor`/`getPalette`/
//!   `mapColor` → views call `ctx.style(Role::…)` directly (D7), so the trait
//!   has **no** color methods; `getClipRect` → subsumed by
//!   [`DrawCtx::clip`](crate::view::DrawCtx::clip).
//!
//! * **Deferred to later rows:** `execute`/`endModal` → TProgram/TDialog
//!   (rows 31/34, D9); `dragView` + `moveGrow`/`change` → a capture handler at
//!   TWindow (row 33, D9); `dataSize`/`getData`/`setData` → a typed
//!   `value()`/`set_value()` protocol at D10/row 39.
//!
//! * **Subsumed by the single loop / group teardown (D9/D3):** the blocking
//!   event-pump helpers (`getEvent`/`putEvent`/`eventAvail`/`keyEvent`/
//!   `mouseEvent`/`textEvent`) collapse into the one event loop (row 31, D9);
//!   `resetCursor` (hardware-cursor placement) is the loop's job once the tree
//!   gives absolute coords; `shutDown` (`hide(); owner->remove(this)`) becomes
//!   `Drop` + the group's child removal (row 26, D3).
//!
//! * **Command-enable policy.** The program-global enabled set
//!   (`curCommandSet`/`enableCommand`/`disableCommand`/`commandEnabled`) lives at
//!   TProgram (row 31), reached through `Context` at routing time. The C++
//!   "commands > 255 are always enabled" rule is **DROPPED** (D1: a command's
//!   identity is a string, not a 0..255 int); the default-enabled vocabulary is
//!   seeded by TProgram and queried through `Context`.
//!
//! * **Dropped entirely (D8/D12):** the occlusion/damage family
//!   (`drawView`/`exposed`/`drawHide`/`drawShow`/`drawUnder*`) and the TVWrite
//!   occlusion writers — replaced by [`DrawCtx`] writes + whole-tree redraw +
//!   diff; `ofBuffered`/`lock`/`unlock`/`buffer` and the `sfExposed` cache (D8);
//!   streamable `read`/`write`/`build` (D12); `showMarkers`/`errorAttr` statics
//!   (`errorAttr` → [`Role::Error`](crate::theme::Role); `showMarkers` dropped).

use crate::command::{Command, CommandSet};
use crate::data::FieldValue;
use crate::event::{Event, EventMask};
use crate::help::HelpCtx;
use crate::view::context::{Context, DrawCtx};
use crate::view::geometry::{Point, Rect};
use crate::view::id::ViewId;

// ---------------------------------------------------------------------------
// D5 flag structs (struct-of-bools replacing the packed sf*/of*/gf*/dm* words)
// ---------------------------------------------------------------------------

/// View state flags — ports the `sf*` family (`views.h`).
///
/// **Dropped:** `sfExposed` (`0x800`) — the D8 occlusion/visibility cache; under
/// whole-tree redraw + diff there is nothing to cache.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct State {
    /// `sfVisible` — the view is shown (set by default in the ctor).
    pub visible: bool,
    /// `sfCursorVis` — the hardware cursor is visible while this view is focused.
    pub cursor_vis: bool,
    /// `sfCursorIns` — the cursor is in insert (block) shape rather than underline.
    pub cursor_ins: bool,
    /// `sfShadow` — the view casts a drop shadow.
    pub shadow: bool,
    /// `sfActive` — the view is in the active window/group chain.
    pub active: bool,
    /// `sfSelected` — the view is the current (selected) one in its owner.
    pub selected: bool,
    /// `sfFocused` — the view is selected *and* its whole owner chain is active.
    pub focused: bool,
    /// `sfDragging` — the view is being dragged/resized.
    pub dragging: bool,
    /// `sfDisabled` — the view ignores events.
    pub disabled: bool,
    /// `sfModal` — the view runs a modal event loop.
    pub modal: bool,
    /// `sfDefault` — the view is the default one (e.g. the default button).
    pub default: bool,
}

/// View option flags — ports the `of*` family (`views.h`).
///
/// **Dropped:** `ofBuffered` (`0x040`) — D8 (per-view back buffer; we redraw the
/// whole tree and diff). The `ofVersion*` bits are D12 (streaming) and never
/// existed in this family beyond the magiblot range, so nothing to drop there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Options {
    /// `ofSelectable` — the view can become the current/focused view.
    pub selectable: bool,
    /// `ofTopSelect` — selecting the view moves it to the front of its owner.
    pub top_select: bool,
    /// `ofFirstClick` — a selecting mouse-down is also passed through to the view.
    pub first_click: bool,
    /// `ofFramed` — the view has a frame drawn around it.
    pub framed: bool,
    /// `ofPreProcess` — the view sees focused-chain events before the focused view.
    pub pre_process: bool,
    /// `ofPostProcess` — the view sees focused-chain events after the focused view.
    pub post_process: bool,
    /// `ofTileable` — the view participates in tile/cascade layout.
    pub tileable: bool,
    /// `ofCenterX` — the view is centered horizontally in its owner.
    pub center_x: bool,
    /// `ofCenterY` — the view is centered vertically in its owner.
    pub center_y: bool,
    /// `ofValidate` — the view is asked to `valid(cmReleasedFocus)` before losing focus.
    pub validate: bool,
}

impl Options {
    /// `ofCentered` (`ofCenterX | ofCenterY`) — centered on both axes.
    pub fn centered(self) -> bool {
        self.center_x && self.center_y
    }
}

/// Grow-mode flags — ports the `gf*` family (`views.h`). Controls how each edge
/// of the view tracks its owner when the owner is resized.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct GrowMode {
    /// `gfGrowLoX` — the left edge tracks the owner's right edge.
    pub lo_x: bool,
    /// `gfGrowLoY` — the top edge tracks the owner's bottom edge.
    pub lo_y: bool,
    /// `gfGrowHiX` — the right edge tracks the owner's right edge.
    pub hi_x: bool,
    /// `gfGrowHiY` — the bottom edge tracks the owner's bottom edge.
    pub hi_y: bool,
    /// `gfGrowRel` — grow proportionally to the owner (windows on the desktop).
    pub rel: bool,
    /// `gfFixed` — the view keeps its size regardless of the owner's.
    pub fixed: bool,
}

impl GrowMode {
    /// `gfGrowAll` (`gfGrowLoX | gfGrowLoY | gfGrowHiX | gfGrowHiY`) — every edge
    /// tracks the owner (the view grows with its owner on all sides).
    pub fn grow_all() -> Self {
        GrowMode {
            lo_x: true,
            lo_y: true,
            hi_x: true,
            hi_y: true,
            ..Default::default()
        }
    }
}

/// Drag-mode flags — ports the `dm*` family (`views.h`). Controls dragging and
/// the limits a dragged view is clamped to (consumed by the row-33 drag handler).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DragMode {
    /// `dmDragMove` — the view can be moved by dragging.
    pub drag_move: bool,
    /// `dmDragGrow` — the view can be resized by dragging.
    pub drag_grow: bool,
    /// `dmDragGrowLeft` — resize by dragging the left edge.
    pub drag_grow_left: bool,
    /// `dmLimitLoX` — clamp the left edge to the owner's left edge.
    pub limit_lo_x: bool,
    /// `dmLimitLoY` — clamp the top edge to the owner's top edge (ctor default).
    pub limit_lo_y: bool,
    /// `dmLimitHiX` — clamp the right edge to the owner's right edge.
    pub limit_hi_x: bool,
    /// `dmLimitHiY` — clamp the bottom edge to the owner's bottom edge.
    pub limit_hi_y: bool,
}

impl DragMode {
    /// `dmLimitAll` (`dmLimitLoX | dmLimitLoY | dmLimitHiX | dmLimitHiY`) — clamp
    /// all four edges to the owner.
    pub fn limit_all() -> Self {
        DragMode {
            limit_lo_x: true,
            limit_lo_y: true,
            limit_hi_x: true,
            limit_hi_y: true,
            ..Default::default()
        }
    }
}

/// The four state flags a parent (`TGroup`, row 26) flips on a child through
/// [`View::set_state`] — the named subset of the `sf*` family that the focus /
/// activation machinery drives. Ports the `aState` argument of
/// `TView::setState` / `TGroup::setState` for the cases that survive D8.
///
/// `sfVisible`/`sfExposed`/`sfShadow`/`sfCursor*` are **not** here: the C++
/// `setState` cases for them are the dropped D8 occlusion/cursor side effects;
/// they are flipped directly on [`ViewState`] (`show`/`hide`/`show_cursor`/…),
/// not through the propagating `set_state` hook.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateFlag {
    /// `sfActive` — the view is in the active window/group chain.
    Active,
    /// `sfSelected` — the view is the current (selected) one in its owner.
    Selected,
    /// `sfFocused` — the view is selected *and* its whole owner chain is active.
    Focused,
    /// `sfDragging` — the view is being dragged/resized.
    Dragging,
}

// ---------------------------------------------------------------------------
// ViewState — the composition target (TView's data members, D2/D5)
// ---------------------------------------------------------------------------

/// The data every view owns — `TView`'s members, ported per D2/D5.
///
/// Widgets embed a `ViewState` (typically as a field named `state`) and reach
/// its flags/geometry directly (`self.state.state.focused`, `self.state.size`),
/// matching D5's field-access idiom. The data fields are `pub`; only
/// `resize_balance` (the `calcBounds` rounding-recovery accumulator) and `id`
/// (stamped by [`Group::insert`](crate::view::Group) — write-once, enforced by
/// `pub(crate)`) are not public.
///
/// **Do not `derive(Default)`** — the all-false derive would leave the view
/// invisible with no drag limit, a silent bug. Construct via [`ViewState::new`]
/// (or [`Default`], which forwards to it with an empty rect).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewState {
    /// Top-left, relative to the owner. TV's `origin`.
    pub origin: Point,
    /// Width/height. TV's `size`.
    pub size: Point,
    /// Cursor position within the view (view-local). TV's `cursor`.
    pub cursor: Point,
    /// State flags (`sf*`).
    pub state: State,
    /// Option flags (`of*`).
    pub options: Options,
    /// Grow-mode flags (`gf*`).
    pub grow_mode: GrowMode,
    /// Drag-mode flags (`dm*`).
    pub drag_mode: DragMode,
    /// Which opt-in event classes this view wants (D4). TV's `eventMask`; the
    /// unconditional `evMouseDown|evKeyDown|evCommand` classes are not opt-in
    /// here, so only `mouse_move`/`mouse_auto` survive.
    pub event_mask: EventMask,
    /// Help context (`hc*`). TV's `helpCtx`.
    pub help_ctx: HelpCtx,
    /// This view's global identity, set by [`Group::insert`](crate::view::Group)
    /// when the view enters a group; `None` before insertion. NOT an up-pointer
    /// — it is the view's own handle (like an ECS entity id), which lets a
    /// handler/loop address it by id (D3).
    pub(crate) id: Option<ViewId>,
    /// `calcBounds` rounding-recovery accumulator — TV's `resizeBalance`.
    /// Private: only `ViewState::calc_bounds` touches it.
    resize_balance: Point,
}

impl ViewState {
    /// Construct view state for `bounds`, with `TView::TView`'s exact defaults.
    ///
    /// Faithful to the C++ ctor (`tview.cpp`): `state = sfVisible`,
    /// `dragMode = dmLimitLoY`, `helpCtx = hcNoContext`, everything else zero.
    /// `eventMask` is all-false here because its three TV bits
    /// (`evMouseDown|evKeyDown|evCommand`) are unconditional under D4 and so are
    /// not opt-in flags.
    pub fn new(bounds: Rect) -> Self {
        let mut s = ViewState {
            origin: Point::new(0, 0),
            size: Point::new(0, 0),
            cursor: Point::new(0, 0),
            state: State {
                visible: true,
                ..Default::default()
            },
            options: Options::default(),
            grow_mode: GrowMode::default(),
            drag_mode: DragMode {
                limit_lo_y: true,
                ..Default::default()
            },
            event_mask: EventMask::default(),
            help_ctx: HelpCtx::NO_CONTEXT,
            id: None,
            resize_balance: Point::new(0, 0),
        };
        s.set_bounds(bounds);
        s
    }

    // -- Geometry (faithful inline bodies from views.h / tview.cpp) ----------

    /// `TView::getBounds` — `{ origin, origin + size }`.
    pub fn get_bounds(&self) -> Rect {
        Rect::from_points(self.origin, self.origin + self.size)
    }

    /// `TView::getExtent` — `{ 0, 0, size.x, size.y }` (view-local).
    pub fn get_extent(&self) -> Rect {
        Rect::new(0, 0, self.size.x, self.size.y)
    }

    /// `TView::setBounds` — `origin = bounds.a; size = bounds.b - bounds.a`.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.origin = bounds.a;
        self.size = bounds.b - bounds.a;
    }

    /// `TView::moveTo` — relocate the top-left to `(x, y)`, keeping the size.
    ///
    /// In C++ this routes through `locate`, whose `sizeLimits` clamp needs the
    /// owner; that clamp relocates to row 26 (the group drives resize). Here it
    /// reduces to recomputing bounds; the D8 redraw is the loop's whole-tree
    /// repaint.
    pub fn move_to(&mut self, x: i32, y: i32) {
        self.set_bounds(Rect::new(x, y, x + self.size.x, y + self.size.y));
    }

    /// `TView::growTo` — keep the origin, set the size to `(x, y)`.
    ///
    /// Like [`move_to`](Self::move_to), `locate`'s owner-dependent `sizeLimits`
    /// clamp relocates to row 26; here it is a plain bounds recompute.
    pub fn grow_to(&mut self, x: i32, y: i32) {
        self.set_bounds(Rect::new(
            self.origin.x,
            self.origin.y,
            self.origin.x + x,
            self.origin.y + y,
        ));
    }

    // -- Verb helpers (D5; the dropped D8 redraw side effects noted inline) --

    /// `TView::show` — make the view visible.
    ///
    /// `setState(sfVisible, True)`'s `drawShow`/`resetCurrent` side effects are
    /// dropped under D8: the loop repaints the whole tree, and the group owns
    /// `resetCurrent` (row 26).
    pub fn show(&mut self) {
        self.state.visible = true;
    }

    /// `TView::hide` — make the view invisible (see [`show`](Self::show) re D8).
    pub fn hide(&mut self) {
        self.state.visible = false;
    }

    /// `TView::setCursor` — move the cursor to view-local `(x, y)`.
    ///
    /// The hardware-cursor push (`drawCursor`/`resetCursor`) needs the tree for
    /// absolute coordinates, so it is deferred to the group/loop (row 26+).
    pub fn set_cursor(&mut self, x: i32, y: i32) {
        self.cursor = Point::new(x, y);
    }

    /// `TView::showCursor` — make the hardware cursor visible (`sfCursorVis`).
    /// The actual cursor placement is the loop's job (needs absolute coords).
    pub fn show_cursor(&mut self) {
        self.state.cursor_vis = true;
    }

    /// `TView::hideCursor` — hide the hardware cursor (`sfCursorVis`).
    pub fn hide_cursor(&mut self) {
        self.state.cursor_vis = false;
    }

    /// `TView::blockCursor` — block (insert) cursor shape (`sfCursorIns`).
    pub fn block_cursor(&mut self) {
        self.state.cursor_ins = true;
    }

    /// `TView::normalCursor` — underline cursor shape (`sfCursorIns` off).
    pub fn normal_cursor(&mut self) {
        self.state.cursor_ins = false;
    }

    /// This view's global identity, set by [`Group::insert`](crate::view::Group)
    /// when the view enters a group; `None` before insertion. NOT an up-pointer
    /// — it is the view's own handle (like an ECS entity id), which lets a
    /// handler/loop address it by id.
    pub fn id(&self) -> Option<ViewId> {
        self.id
    }

    /// `TView::getHelpCtx` — [`HelpCtx::DRAGGING`] while dragging, else `help_ctx`.
    pub fn get_help_ctx(&self) -> HelpCtx {
        if self.state.dragging {
            HelpCtx::DRAGGING
        } else {
            self.help_ctx
        }
    }

    /// Flip the [`State`] bool named by `flag` — the field-access realization of
    /// `TView::setState`'s `state |= aState` / `state &= ~aState` for the four
    /// propagating flags (see [`StateFlag`]). The broadcast / propagation side
    /// effects live in [`View::set_state`], not here.
    pub fn set_flag(&mut self, flag: StateFlag, enable: bool) {
        match flag {
            StateFlag::Active => self.state.active = enable,
            StateFlag::Selected => self.state.selected = enable,
            StateFlag::Focused => self.state.focused = enable,
            StateFlag::Dragging => self.state.dragging = enable,
        }
    }

    // -- Resize math (calcBounds / sizeLimits, pure functions; tview.cpp) ----

    /// `TView::sizeLimits` — the `(min, max)` size this view may take inside an
    /// owner of `owner_size`.
    ///
    /// Default: `min = (0, 0)`; `max = owner_size` unless `grow_mode.fixed`, in
    /// which case `max = (i32::MAX, i32::MAX)`. Widgets (e.g. TWindow) override to
    /// impose a minimum size.
    pub(crate) fn size_limits(&self, owner_size: Point) -> (Point, Point) {
        let min = Point::new(0, 0);
        let max = if self.grow_mode.fixed {
            Point::new(i32::MAX, i32::MAX)
        } else {
            owner_size
        };
        (min, max)
    }

    /// `TView::calcBounds` — the new bounds for this view after its owner resized
    /// from `owner_size - delta` to `owner_size` (so `delta = new - old`).
    ///
    /// Faithful port: applies each enabled `gf*` edge via `grow`, then clamps
    /// to `(min_lim, max_lim)` through `fit_to_limits`, updating the private
    /// `resize_balance` accumulator so the view can recover its size. **Returns**
    /// the bounds; it does *not* apply them ([`change_bounds`](View::change_bounds)
    /// does).
    ///
    /// The size limits are passed in (rather than read here) so the **single
    /// source of grow math** can be driven by the overridable
    /// [`View::size_limits`] hook — see [`View::calc_bounds`], the normal caller.
    /// Compute them with [`size_limits`](Self::size_limits) for the un-overridden
    /// default.
    pub(crate) fn calc_bounds(
        &mut self,
        owner_size: Point,
        delta: Point,
        min_lim: Point,
        max_lim: Point,
    ) -> Rect {
        let mut bounds = self.get_bounds();
        let gm = self.grow_mode;

        if gm.lo_x {
            grow(gm, owner_size.x, delta.x, &mut bounds.a.x);
        }
        if gm.hi_x {
            grow(gm, owner_size.x, delta.x, &mut bounds.b.x);
        }
        if gm.lo_y {
            grow(gm, owner_size.y, delta.y, &mut bounds.a.y);
        }
        if gm.hi_y {
            grow(gm, owner_size.y, delta.y, &mut bounds.b.y);
        }

        fit_to_limits(
            bounds.a.x,
            &mut bounds.b.x,
            min_lim.x,
            max_lim.x,
            &mut self.resize_balance.x,
        );
        fit_to_limits(
            bounds.a.y,
            &mut bounds.b.y,
            min_lim.y,
            max_lim.y,
            &mut self.resize_balance.y,
        );
        bounds
    }
}

impl Default for ViewState {
    /// Empty-rect view state with the real ctor defaults (visible, `dmLimitLoY`).
    /// **Not** a `derive` — the all-false derive would be a silent bug.
    fn default() -> Self {
        ViewState::new(Rect::default())
    }
}

// -- calcBounds private helpers (verbatim ports of the tview.cpp statics) -----

/// `range` (tview.cpp) — clamp `val` into `[min, max]`, `min` pinned to `max` if
/// inverted.
fn range(val: i32, min: i32, max: i32) -> i32 {
    let min = if min > max { max } else { min };
    val.clamp(min, max)
}

/// `TView::locate` (tview.cpp) — clamp `bounds` to the view's size limits and
/// apply them if they changed. A **free function** over `&mut dyn View` (NOT a
/// `View` trait method): a trait method would be forwarded by the `#[delegate]`
/// macro to the *inner group* for wrappers like [`Window`](crate::widgets::window::Window),
/// whose group has a 0×0 `size_limits`, bypassing the window's 16×6 minimum
/// (the hazard at `window.rs`). As a free fn, `size_limits` dispatches virtually
/// to the wrapper's override and `change_bounds` forwards to the group (faithful
/// `TGroup::changeBounds`). The C++ `drawView`/shadow tail is moot under D8
/// (whole-tree redraw). Backs [`TDeskTop::tile`/`cascade`](crate::desktop::Desktop).
pub(crate) fn locate(view: &mut dyn View, mut bounds: Rect, owner_size: Point) {
    let (min, max) = view.size_limits(owner_size);
    bounds.b.x = bounds.a.x + range(bounds.b.x - bounds.a.x, min.x, max.x);
    bounds.b.y = bounds.a.y + range(bounds.b.y - bounds.a.y, min.y, max.y);
    if bounds != view.state().get_bounds() {
        view.change_bounds(bounds);
    }
}

/// `balancedRange` (tview.cpp) — fit `val` into `[min, max]` while accumulating
/// the remainder in `balance`, so a later resize can give the size back.
fn balanced_range(val: i32, min: i32, mut max: i32, balance: &mut i32) -> i32 {
    if min > max {
        max = min;
    }
    if val < min {
        *balance += val - min;
        min
    } else if val > max {
        *balance += val - max;
        max
    } else {
        let offset = range(val + *balance, min, max) - val;
        *balance -= offset;
        val + offset
    }
}

/// `fitToLimits` (tview.cpp) — `b = a + balancedRange(b - a, min, max, balance)`.
fn fit_to_limits(a: i32, b: &mut i32, min: i32, max: i32, balance: &mut i32) {
    *b = a + balanced_range(*b - a, min, max, balance);
}

/// `grow` (tview.cpp) — advance one bound coordinate `i` for owner dimension `s`
/// (the *new* owner size) and delta `d`.
///
/// For `gfGrowRel` the bound scales proportionally; `s - d` is the *old* owner
/// size, and the `if s != d` guard is a **divide-by-zero guard** (old size 0) —
/// ported verbatim. Otherwise the bound shifts by `d`.
///
/// Coordinate-magnitude assumption: `*i * s` is `i32 * i32` and, faithful to the
/// C++ `int` math, assumes screen-scale coordinates well below `i32::MAX`; an
/// out-of-range coordinate would panic in debug (overflow) rather than silently
/// wrap.
fn grow(gm: GrowMode, s: i32, d: i32, i: &mut i32) {
    if gm.rel {
        if s != d {
            *i = (*i * s + ((s - d) >> 1)) / (s - d);
        }
    } else {
        *i += d;
    }
}

// ---------------------------------------------------------------------------
// View trait (TView's virtuals, D2)
// ---------------------------------------------------------------------------

/// The behavior every view implements — `TView`'s virtual methods, ported per
/// D2 (inheritance → trait). Widgets supply [`state`](View::state) /
/// [`state_mut`](View::state_mut) / [`draw`](View::draw); the rest default.
// MAINTENANCE: when adding a defaulted method to this trait, also add a
// forwarder entry to `tvision-macros/src/specs.rs` (`view()`) AND the
// `expected` list in `tests/delegate_view.rs`. Required methods (no default)
// catch omission at compile time; defaulted ones would silently fall back to
// the default at every `#[delegate]` site if the forwarder is missing.
pub trait View {
    /// Borrow the embedded [`ViewState`] (TV's data members).
    fn state(&self) -> &ViewState;

    /// Mutably borrow the embedded [`ViewState`].
    fn state_mut(&mut self) -> &mut ViewState;

    /// `TView::draw` — paint the view through `ctx`. **Must be overridden.**
    ///
    /// The C++ base fills the extent with blanks in `getColor(1)`; with no
    /// palette chain (D7) and no instantiable bare `TView`, there is no sensible
    /// default, so `draw` is required rather than defaulted.
    fn draw(&mut self, ctx: &mut DrawCtx);

    /// `TView::handleEvent` — the **base is a no-op** (the event passes through).
    ///
    /// C++'s only base body is the mouse-down auto-select, which relocates to
    /// `TGroup` (row 26) because it calls the up-tree `focus()` (D3). See the
    /// module-level breadcrumb.
    fn handle_event(&mut self, _ev: &mut Event, _ctx: &mut Context) {}

    /// `TView::setState` — flip a propagating state flag and run its side
    /// effects. The base body (relocated from `tview.cpp`'s `setState`) flips the
    /// flag and, for [`StateFlag::Focused`], emits the focus broadcast
    /// (`cmReceivedFocus`/`cmReleasedFocus`) via `ctx` — the **carryover #2**
    /// focus broadcast that row 23 deferred to here.
    ///
    /// The C++ `message(owner, evBroadcast, …, this)` is reduced under D3/D4: only
    /// the `owner` receiver is dropped (the broadcast goes to the loop's queue, not
    /// a receiver); the `this` `infoPtr` payload is **carried** as the broadcast's
    /// `source` (D4 amendment) — `self.state().id()`, the view whose focus changed.
    /// `TGroup` (row 26) overrides this to also propagate to its children. The
    /// dropped D8 `setState` cases (`sfVisible`/`sfExposed`/`sfShadow`/`sfCursor*`
    /// redraw/occlusion) have no analogue here.
    fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
        self.state_mut().set_flag(flag, enable);
        if flag == StateFlag::Focused {
            let source = self.state().id(); // self == C++ `this`
            ctx.broadcast(
                if enable {
                    Command::RECEIVED_FOCUS
                } else {
                    Command::RELEASED_FOCUS
                },
                source,
            );
        }
    }

    /// `TView::valid` — whether the view is in a valid state for `cmd` (e.g. a
    /// modal end / focus release). Base is always `true`.
    fn valid(&self, _cmd: Command) -> bool {
        true
    }

    /// `TView::getData` — this control's typed value as a [`FieldValue`] (D10),
    /// or `None` for a non-data view. The successor to the untyped `getData`
    /// `memcpy`. Base: `None` (a bare view carries no transferable data); data
    /// controls (e.g. [`InputLine`](crate::widgets::InputLine)) override.
    fn value(&self) -> Option<FieldValue> {
        None
    }

    /// `TView::setData` — load a typed [`FieldValue`] into this control (D10).
    /// Base: ignore (a non-data view has nowhere to put it); data controls
    /// override. A control ignores a `FieldValue` variant it does not understand.
    fn set_value(&mut self, _v: FieldValue) {}

    /// `TView::awaken` — called after a view tree is loaded/created so the view
    /// can finish initializing. Base is a no-op.
    fn awaken(&mut self) {}

    /// `TView::sizeLimits` — delegates to `ViewState::size_limits`; override to
    /// impose a minimum size (TWindow does).
    fn size_limits(&self, owner_size: Point) -> (Point, Point) {
        self.state().size_limits(owner_size)
    }

    /// `TView::calcBounds` — the new bounds after the owner resized to
    /// `owner_size` (`delta = new - old`). Computes the size limits via the
    /// overridable [`size_limits`](View::size_limits) hook — so a widget override
    /// (e.g. TWindow's minimum) participates — then delegates the grow math to
    /// `ViewState::calc_bounds` (the single source). Returns the bounds; it does
    /// not apply them ([`change_bounds`](View::change_bounds) does).
    fn calc_bounds(&mut self, owner_size: Point, delta: Point) -> Rect {
        let (min_lim, max_lim) = self.size_limits(owner_size);
        self.state_mut()
            .calc_bounds(owner_size, delta, min_lim, max_lim)
    }

    /// `TView::changeBounds` — apply `bounds`. Base just sets them (the C++
    /// `drawView()` after is automatic under D8). `TGroup`/`TWindow` override to
    /// propagate the resize to children.
    fn change_bounds(&mut self, bounds: Rect) {
        self.state_mut().set_bounds(bounds);
    }

    /// `TView::resetCursor` support — the view-local hardware-cursor position
    /// this view wants shown, or `None` to hide it. Base: `Some(cursor)` iff the
    /// view is focused with a visible cursor (`sfFocused && sfCursorVis`), else
    /// `None`.
    ///
    /// This is the top-down realization of the C++ focused-chain cursor walk
    /// (`TView::resetCursor` / `TView::drawCursor`): the live loop (row 31) asks
    /// the root for the absolute cursor each pass. [`Group`](crate::view::Group)
    /// overrides this to descend into its `current` child, accumulating the
    /// child's origin at each level.
    fn cursor_request(&self) -> Option<Point> {
        let s = self.state();
        if s.state.focused && s.state.cursor_vis {
            Some(s.cursor)
        } else {
            None
        }
    }

    /// Resolve `id` to a **descendant** of this view (never self — the *parent*
    /// identifies a view by id). A leaf has no descendants, so the base returns
    /// `None`; a [`Group`](crate::view::Group) overrides to search its children
    /// and recurse; a `Group`-embedding view delegates to its inner group. This is
    /// the "tree-walk via Context" promised by D3 — the uniform way the event loop
    /// / a capture handler acts on a view it holds only by id (move a window's
    /// bounds, flip `sfDragging`, …).
    fn find_mut(&mut self, id: ViewId) -> Option<&mut dyn View> {
        let _ = id;
        None
    }

    /// Remove the descendant named by `id` from whichever group owns it (faithful
    /// `destroy`/self-removal). Returns `true` if it was found+removed. Distinct
    /// from [`find_mut`](View::find_mut) because removal happens in the *owner's*
    /// child `Vec` (a view cannot remove itself — it doesn't know its owner, D3)
    /// and must run the owning group's `reset_current`. Base: `false` (a leaf owns
    /// nothing).
    fn remove_descendant(&mut self, id: ViewId, ctx: &mut Context) -> bool {
        let _ = (id, ctx);
        false
    }

    /// Focus (select) the descendant named by `id` within whichever group owns it
    /// — the tree-op behind `TLabel::focusLink` (`link->focus()`). Returns `true`
    /// if `id` was found in this subtree (selectable or not — finding it stops the
    /// walk). Distinct from [`find_mut`](View::find_mut) because focusing happens in
    /// the *owning group* (a view cannot select itself within itself, D3): the
    /// owning [`Group`](crate::view::Group) calls `focus_child` after applying the
    /// `ofSelectable` gate (faithful to C++ `focusLink`'s `link->options &
    /// ofSelectable` check). Base: `false` (a leaf owns nothing).
    ///
    /// **Scope (breadcrumb):** this focuses the link *within its owning group*, not
    /// the full ancestor chain C++ `TView::focus` walks up. That is correct for the
    /// label/link sibling case (label and link share a group already on the focused
    /// path); a cross-group link would need an up-chain walk, which has no consumer.
    fn focus_descendant(&mut self, id: ViewId, ctx: &mut Context) -> bool {
        let _ = (id, ctx);
        false
    }

    /// `TView`/`TWindow::number` — the window number for Alt-N selection. Base
    /// views are unnumbered (`None`); [`Window`](crate::window::Window) overrides
    /// to return its number when `> 0` (`wnNoNumber == 0` → `None`).
    fn number(&self) -> Option<i16> {
        None
    }

    /// Whether a mouse-down inside this view should auto-select (focus) it — the
    /// per-view opt-out for the relocated `TView::handleEvent` mouse-down
    /// auto-select (carryover #1, see `Group::route_event`). C++ views opt in by
    /// calling `TView::handleEvent`'s base body; the canonical opt-OUT is
    /// `TButton`, which calls it only when `bfGrabFocus` is set. Default `true`
    /// (the common case — matches every view that calls the base auto-select);
    /// `Button` overrides to return its `bfGrabFocus` flag. A view returning
    /// `false` is **not** focused by a click but still **receives** the click
    /// (so it can act, e.g. press, without becoming `current`).
    fn grabs_focus_on_click(&self) -> bool {
        true
    }

    /// Tree-op: ask this subtree to select the window numbered `num`. Returns
    /// whether one matched. Consistent with the [`find_mut`](View::find_mut) /
    /// [`remove_descendant`](View::remove_descendant) tree-op family: the live
    /// loop holds a subtree only by id (here, the desktop) and asks it to act.
    /// Default: no-op (`false`). [`Desktop`](crate::desktop::Desktop) overrides.
    fn select_window_num(&mut self, num: i16, ctx: &mut Context) -> bool {
        let _ = (num, ctx);
        false
    }

    /// Tree-op: lay this subtree's tileable windows out in a grid (`cmTile` →
    /// `TApplication::handleEvent` → `deskTop->tile(getTileRect())`). Base: no-op
    /// (only [`Desktop`](crate::desktop::Desktop) lays out windows). `r` is the
    /// desktop-local layout rect. Mirrors [`select_window_num`](View::select_window_num):
    /// the live loop drives the desktop by id through `&mut dyn View`.
    fn tile(&mut self, _r: Rect) {}

    /// Tree-op: lay this subtree's tileable windows out cascaded (`cmCascade` →
    /// `deskTop->cascade(getTileRect())`). Base: no-op; [`Desktop`](crate::desktop::Desktop)
    /// overrides.
    fn cascade(&mut self, _r: Rect) {}

    /// The `TListViewer` read-sync broker hook (row 28). Defaulted no-op;
    /// concrete list widgets override to delegate to
    /// [`list_viewer::apply_scroll`](crate::widgets::list_viewer::apply_scroll).
    /// The pump passes the freshly-read h/v scrollbar values (`None` if the bar
    /// is absent), resolved through [`View::value`].
    ///
    /// This parallels the row-27 [`Deferred::SyncScrollerDelta`](crate::view::Deferred::SyncScrollerDelta)
    /// read-sync, but goes through a trait method instead of a hard downcast to a
    /// concrete struct: `TListViewer` is a *trait* (subclasses reuse its `draw`
    /// and override `get_text`/`is_selected`), so a `dyn View → dyn ListViewer`
    /// downcast is impossible. The two read-sync mechanisms could later unify;
    /// out of scope for row 28.
    fn apply_list_scroll(&mut self, _h: Option<i32>, _v: Option<i32>, _ctx: &mut Context) {}

    /// The `TMenuView` command-graying broker hook (row 49). Defaulted no-op;
    /// menu views override to regray their menu tree against the program's live
    /// command set (the free fn
    /// [`menu::menu_view::update_menu_commands`](crate::menu::menu_view::update_menu_commands),
    /// the port of `TMenuView::updateMenu`).
    ///
    /// This is the §2 broker, the exact precedent of
    /// [`apply_list_scroll`](View::apply_list_scroll): a menu view (a child, D3)
    /// cannot read the program's [`CommandSet`](crate::CommandSet) inline — the
    /// pump owns it, and storing a `&CommandSet` on [`Context`] would alias the
    /// apply-loop's `&mut command_set` mutation (the
    /// `EnableCommand`/`DisableCommand` arms). So the view requests
    /// [`Deferred::UpdateMenu`](crate::view::Deferred::UpdateMenu) by its own id,
    /// and the pump calls back here at apply time with the live set in hand. The
    /// C++ `updateMenu` return-bool (`if changed drawView`) is dropped — under
    /// whole-tree redraw (D8) the next pump repaints unconditionally.
    fn update_menu_commands(&mut self, _cs: &CommandSet) {}

    /// The `TMenuView` highlight write-back hook (rows 50–52). Defaulted no-op;
    /// menu views ([`MenuBar`](crate::menu::MenuBar) /
    /// [`MenuBox`](crate::menu::MenuBox)) override to set their
    /// [`MenuViewState::current`](crate::menu::MenuViewState) — the **write-only
    /// display cache** the `draw` reads to pick the selected colour.
    ///
    /// While a menu session is active the
    /// [`MenuSession`](crate::menu::MenuSession) capture handler owns the
    /// `execute()` state machine (Clean Architecture A); the boxes are never
    /// focused and run no event logic. When the session changes a level's
    /// `current` it requests
    /// [`Deferred::SetMenuCurrent`](crate::view::Deferred::SetMenuCurrent) by the
    /// box/bar id, and the pump calls back here at apply time. A trait method (not
    /// a `MenuBar`/`MenuBox` downcast) keeps the broker uniform across the two
    /// concrete menu views, exactly like
    /// [`update_menu_commands`](View::update_menu_commands).
    fn set_menu_current(&mut self, _current: Option<usize>) {}

    /// Downcast hook for the rare owner→child push that needs the concrete type
    /// (e.g. `TWindow::zoom` pushing `set_zoomed` to its `TFrame`). Base returns
    /// `None`; only views that must be reached concretely override it. (`Any`
    /// requires `'static`, which every view is.)
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::screen::Buffer;
    use crate::theme::{Role, Theme};

    // -- ViewState ctor defaults --------------------------------------------

    #[test]
    fn new_sets_ctor_defaults() {
        let v = ViewState::new(Rect::new(3, 4, 13, 9));
        assert_eq!(v.origin, Point::new(3, 4));
        assert_eq!(v.size, Point::new(10, 5));
        assert_eq!(v.cursor, Point::new(0, 0));
        // sfVisible set, everything else clear.
        assert!(v.state.visible);
        assert_eq!(
            v.state,
            State {
                visible: true,
                ..Default::default()
            }
        );
        // dmLimitLoY set, everything else clear.
        assert!(v.drag_mode.limit_lo_y);
        assert_eq!(
            v.drag_mode,
            DragMode {
                limit_lo_y: true,
                ..Default::default()
            }
        );
        // eventMask all-false (the three TV bits are unconditional under D4).
        assert_eq!(v.event_mask, EventMask::default());
        assert!(!v.event_mask.mouse_move && !v.event_mask.mouse_auto);
        assert_eq!(v.help_ctx, HelpCtx::NO_CONTEXT);
        assert_eq!(v.options, Options::default());
        assert_eq!(v.grow_mode, GrowMode::default());
    }

    #[test]
    fn default_matches_new_with_empty_rect_and_is_not_all_false() {
        let v = ViewState::default();
        // The regression guard: a derive(Default) would make these false.
        assert!(
            v.state.visible,
            "default must be visible (not derive(Default))"
        );
        assert!(
            v.drag_mode.limit_lo_y,
            "default must keep dmLimitLoY (not derive(Default))"
        );
        assert_eq!(v.origin, Point::new(0, 0));
        assert_eq!(v.size, Point::new(0, 0));
    }

    // -- Geometry ------------------------------------------------------------

    #[test]
    fn get_bounds_and_extent() {
        let v = ViewState::new(Rect::new(2, 3, 12, 8));
        assert_eq!(v.get_bounds(), Rect::new(2, 3, 12, 8));
        assert_eq!(v.get_extent(), Rect::new(0, 0, 10, 5));
    }

    #[test]
    fn set_bounds_recomputes_origin_and_size() {
        let mut v = ViewState::new(Rect::new(0, 0, 1, 1));
        v.set_bounds(Rect::new(5, 6, 15, 11));
        assert_eq!(v.origin, Point::new(5, 6));
        assert_eq!(v.size, Point::new(10, 5));
    }

    #[test]
    fn move_to_keeps_size() {
        let mut v = ViewState::new(Rect::new(0, 0, 10, 5));
        v.move_to(7, 8);
        assert_eq!(v.get_bounds(), Rect::new(7, 8, 17, 13));
        assert_eq!(v.size, Point::new(10, 5));
    }

    #[test]
    fn grow_to_keeps_origin() {
        let mut v = ViewState::new(Rect::new(3, 4, 10, 9));
        v.grow_to(20, 6);
        assert_eq!(v.get_bounds(), Rect::new(3, 4, 23, 10));
        assert_eq!(v.origin, Point::new(3, 4));
    }

    // -- Flag struct defaults + verb helpers ---------------------------------

    #[test]
    fn flag_structs_default_all_false() {
        let s = State::default();
        assert!(!s.visible && !s.cursor_vis && !s.focused && !s.disabled && !s.default);
        let o = Options::default();
        assert!(!o.selectable && !o.framed && !o.validate && !o.center_x);
        let g = GrowMode::default();
        assert!(!g.lo_x && !g.rel && !g.fixed);
        let d = DragMode::default();
        assert!(!d.drag_move && !d.limit_lo_y && !d.limit_hi_y);
    }

    #[test]
    fn options_centered_and_growmode_grow_all() {
        let mut o = Options::default();
        assert!(!o.centered());
        o.center_x = true;
        assert!(!o.centered());
        o.center_y = true;
        assert!(o.centered());
        let g = GrowMode::grow_all();
        assert!(g.lo_x && g.lo_y && g.hi_x && g.hi_y && !g.rel && !g.fixed);
        let dm = DragMode::limit_all();
        assert!(dm.limit_lo_x && dm.limit_lo_y && dm.limit_hi_x && dm.limit_hi_y);
        assert!(!dm.drag_move);
    }

    #[test]
    fn show_hide_flip_visible() {
        let mut v = ViewState::new(Rect::new(0, 0, 1, 1));
        v.hide();
        assert!(!v.state.visible);
        v.show();
        assert!(v.state.visible);
    }

    #[test]
    fn cursor_verbs_flip_the_right_flags() {
        let mut v = ViewState::new(Rect::new(0, 0, 1, 1));
        v.set_cursor(3, 4);
        assert_eq!(v.cursor, Point::new(3, 4));
        assert!(!v.state.cursor_vis);
        v.show_cursor();
        assert!(v.state.cursor_vis);
        v.hide_cursor();
        assert!(!v.state.cursor_vis);
        v.block_cursor();
        assert!(v.state.cursor_ins);
        v.normal_cursor();
        assert!(!v.state.cursor_ins);
    }

    #[test]
    fn get_help_ctx_uses_dragging_flag() {
        let mut v = ViewState::new(Rect::new(0, 0, 1, 1));
        v.help_ctx = HelpCtx::custom("app.topic");
        assert_eq!(v.get_help_ctx(), HelpCtx::custom("app.topic"));
        v.state.dragging = true;
        assert_eq!(v.get_help_ctx(), HelpCtx::DRAGGING);
    }

    // -- Resize math ---------------------------------------------------------

    #[test]
    fn size_limits_fixed_vs_non_fixed() {
        let mut v = ViewState::new(Rect::new(0, 0, 5, 5));
        let owner = Point::new(40, 20);
        assert_eq!(v.size_limits(owner), (Point::new(0, 0), owner));
        v.grow_mode.fixed = true;
        assert_eq!(
            v.size_limits(owner),
            (Point::new(0, 0), Point::new(i32::MAX, i32::MAX))
        );
    }

    #[test]
    fn calc_bounds_absolute_grow_hi() {
        // gfGrowHiX|gfGrowHiY (no rel): the hi edge shifts by delta.
        let mut v = ViewState::new(Rect::new(0, 0, 10, 5));
        v.grow_mode.hi_x = true;
        v.grow_mode.hi_y = true;
        // owner grew from (20,10) to (25,13): delta (5,3), new owner size (25,13).
        let owner = Point::new(25, 13);
        let (min, max) = v.size_limits(owner);
        let b = v.calc_bounds(owner, Point::new(5, 3), min, max);
        assert_eq!(b, Rect::new(0, 0, 15, 8));
    }

    #[test]
    fn calc_bounds_proportional_grow_rel() {
        // gfGrowRel|gfGrowHiX: coordinate scales as the owner does.
        // bounds (0,0,10,10), owner doubled 20 -> 40 (delta 20, new size 40):
        // b.x = (10*40 + (20>>1))/20 = 410/20 = 20.
        let mut v = ViewState::new(Rect::new(0, 0, 10, 10));
        v.grow_mode.rel = true;
        v.grow_mode.hi_x = true;
        let owner = Point::new(40, 0);
        let (min, max) = v.size_limits(owner);
        let b = v.calc_bounds(owner, Point::new(20, 0), min, max);
        assert_eq!(b.b.x, 20);
    }

    #[test]
    fn grow_divide_by_zero_guard() {
        // gfGrowRel with s == d (old owner size 0): the guard must skip the
        // division, leaving the bound unchanged. `fixed` makes size_limits max
        // i32::MAX so the post-grow fit_to_limits clamp does not mask the guard.
        let mut v = ViewState::new(Rect::new(0, 0, 10, 10));
        v.grow_mode.rel = true;
        v.grow_mode.hi_x = true;
        v.grow_mode.fixed = true;
        let owner = Point::new(7, 0);
        let (min, max) = v.size_limits(owner);
        let b = v.calc_bounds(owner, Point::new(7, 0), min, max);
        assert_eq!(b.b.x, 10, "s == d must not divide");
    }

    #[test]
    fn calc_bounds_resize_balance_recovers_clamped_size() {
        // The `resize_balance` accumulator is cross-call state: when a resize
        // clamps the view smaller than it wants, the lost amount is banked, and a
        // later resize that makes room gives it back. One `ViewState`, two calls,
        // `set_bounds` between (mirroring a real resize loop). No grow flag is
        // needed — `fit_to_limits` runs every call regardless of grow mode; the
        // owner-derived `max` does the clamping. owner-y is kept large so only x
        // is exercised.
        let mut v = ViewState::new(Rect::new(0, 0, 10, 5)); // size.x = 10
        assert_eq!(v.resize_balance, Point::new(0, 0));

        // Call 1: owner shrinks so size_limits.max.x = 6 < wanted 10.
        //   fit_to_limits(a=0, b=10, min=0, max=6, bal=0):
        //     val = 10 > max = 6  ->  bal += (10 - 6) = 4, returns 6.
        //   => b1.b.x = 6, resize_balance.x = 4.
        let owner1 = Point::new(6, 20);
        let (min1, max1) = v.size_limits(owner1);
        let b1 = v.calc_bounds(owner1, Point::new(0, 0), min1, max1);
        assert_eq!(b1.b.x, 6, "call 1 clamps the right edge to the owner");
        assert_eq!(
            v.resize_balance.x, 4,
            "call 1 banks the 4 columns it could not keep"
        );

        // Apply the clamped bounds, as a resize loop would.
        v.set_bounds(b1); // size.x = 6

        // Call 2: owner grows so size_limits.max.x = 12 (room to spare).
        //   fit_to_limits(a=0, b=6, min=0, max=12, bal=4):
        //     val = 6 in [0,12] -> offset = clamp(6 + 4, 0, 12) - 6 = 4,
        //                          bal -= 4 = 0, returns 6 + 4 = 10.
        //   => b2.b.x = 10 (original size recovered), resize_balance.x = 0.
        let owner2 = Point::new(12, 20);
        let (min2, max2) = v.size_limits(owner2);
        let b2 = v.calc_bounds(owner2, Point::new(0, 0), min2, max2);
        assert_eq!(
            b2.b.x, 10,
            "call 2 recovers the original size via the balance"
        );
        assert_eq!(v.resize_balance.x, 0, "the banked balance is fully spent");
    }

    // -- View trait ----------------------------------------------------------

    /// A trivial concrete view: fills its extent with a styled glyph.
    struct FillView {
        st: ViewState,
        ch: char,
    }
    impl View for FillView {
        fn state(&self) -> &ViewState {
            &self.st
        }
        fn state_mut(&mut self) -> &mut ViewState {
            &mut self.st
        }
        fn draw(&mut self, ctx: &mut DrawCtx) {
            let ext = self.st.get_extent();
            ctx.fill(ext, self.ch, ctx.style(Role::Background));
        }
    }

    #[test]
    fn base_handle_event_is_noop_and_passes_through() {
        let mut v = FillView {
            st: ViewState::new(Rect::new(0, 0, 4, 2)),
            ch: '#',
        };
        let before = v.st.state;
        let mut ev = Event::KeyDown(crate::event::KeyEvent::new(
            crate::event::Key::Enter,
            crate::event::KeyModifiers::default(),
        ));
        let mut out = std::collections::VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        v.handle_event(&mut ev, &mut ctx);
        // Event untouched (still non-Nothing), state unchanged.
        assert!(!ev.is_nothing());
        assert_eq!(v.st.state, before);
        assert!(out.is_empty());
    }

    #[test]
    fn base_number_is_none_and_select_window_num_is_noop() {
        let mut v = FillView {
            st: ViewState::new(Rect::new(0, 0, 4, 2)),
            ch: '#',
        };
        assert_eq!(v.number(), None, "base view is unnumbered");
        let mut out = std::collections::VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        assert!(
            !v.select_window_num(1, &mut ctx),
            "base select_window_num is a no-op (false)"
        );
    }

    #[test]
    fn base_valid_is_true() {
        let v = FillView {
            st: ViewState::new(Rect::new(0, 0, 1, 1)),
            ch: ' ',
        };
        assert!(v.valid(Command::OK));
    }

    /// The mandatory snapshot for this abstract row: the trait drives the render
    /// pipeline end-to-end, through the real `Renderer` + `HeadlessBackend` path
    /// (the template every widget test copies). Drawn through `&mut dyn View` so
    /// it is the *trait* (not the inherent method) exercising `DrawCtx`.
    #[test]
    fn trait_drives_render_pipeline_snapshot() {
        let theme = Theme::classic_blue();
        let mut view: Box<dyn View> = Box::new(FillView {
            st: ViewState::new(Rect::new(1, 1, 5, 3)),
            ch: '*',
        });
        let (backend, screen) = HeadlessBackend::new(6, 3);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = view.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            view.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }
}
