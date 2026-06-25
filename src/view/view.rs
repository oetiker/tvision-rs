//! The [`View`] trait + [`ViewState`] — the base every widget builds on.
//!
//! A widget *embeds* a [`ViewState`] (its bounds, state flags, options, owner id,
//! …) and *implements* [`View`] (draw, handle an event, report its value, …).
//! The packed framework flag words become the
//! [`State`]/[`Options`]/[`GrowMode`]/[`DragMode`] **structs-of-bools**, and the
//! state-mutating verbs become plain field flips or small helpers.
//!
//! # What lives here vs. elsewhere
//!
//! This module is the *abstract base*. Several operations have no home on a bare
//! view because the data they need lives in the tree or the event loop:
//!
//! * **Up-tree / owner operations live on [`Group`](crate::view::Group)** —
//!   focusing, selecting, sibling navigation, and the coordinate transforms.
//!   Because a view has no up-pointer, the group drives these *top-down*. The
//!   auto-select on a selecting mouse-down likewise lives in the group's routing;
//!   the base [`View::handle_event`] is a no-op.
//!
//! * **Already provided elsewhere:** timers →
//!   [`Context::set_timer`](crate::view::Context::set_timer) /
//!   [`kill_timer`](crate::view::Context::kill_timer); colors → views call
//!   `ctx.style(Role::…)` directly, so the trait has **no** color methods; the
//!   clip rect → [`DrawCtx::clip`](crate::view::DrawCtx::clip).
//!
//! * **Modality and data transfer:** a modal loop runs through
//!   [`Program::exec_view`](crate::app::Program::exec_view) and
//!   [`Context::end_modal`](crate::view::Context::end_modal); window drag/resize
//!   is a capture handler; getting/setting a view's contents is the typed
//!   [`View::value`] / [`View::set_value`] protocol.
//!
//! * **Subsumed by the single event loop:** the blocking event-pump and
//!   put-back helpers, cursor placement, and teardown collapse into the one loop
//!   plus `Drop` and the group's child removal.
//!
//! * **Command-enable policy.** The program-global command set lives on
//!   [`Program`](crate::app::Program) as its complement — a **disabled set**
//!   (denylist): every command, including app-minted ones, is enabled unless
//!   explicitly disabled. Views write through
//!   `Context::enable_command`/`disable_command` and read through the
//!   `Context::command_enabled` per-pump snapshot
//!   (`docs/design/command-enablement.md`).
//!
//! * **Dropped entirely:** the occlusion/damage family (per-view back buffers and
//!   the exposed-region cache) is replaced by [`DrawCtx`] writes + whole-tree
//!   redraw + diff; serialization is gone; the error attribute becomes
//!   [`Role::Error`](crate::theme::Role).
//!
//! # Turbo Vision heritage
//! Ports `TView` (`tview.cpp`/`views.h`), the root of the view hierarchy.
//! Inheritance becomes a trait plus a composed `ViewState` (deviation D2); the
//! packed `sf*`/`of*`/`gf*`/`dm*` flag words become structs-of-bools (deviation
//! D5); owner/sibling pointers become tree edges plus `ViewId` handles
//! (deviation D3).

use crate::command::{Command, CommandSet};
use crate::data::FieldValue;
use crate::event::{Event, EventMask};
use crate::help::HelpCtx;
use crate::view::context::{Context, DrawCtx};
use crate::view::geometry::{Point, Rect};
use crate::view::id::ViewId;

// ---------------------------------------------------------------------------
// Flag structs (struct-of-bools replacing the packed sf*/of*/gf*/dm* words)
// ---------------------------------------------------------------------------

/// The per-view state flags — visibility, focus, selection, drag, and the rest
/// of the activation/interaction bits a parent flips on its children.
///
/// Most flags are **set by the framework** (not by widget code directly):
/// `visible` via `show()`/`hide()`; `active`/`selected`/`focused`/`dragging`
/// via `Group::set_state`; `modal` by `Program::exec_view`. The flags you
/// commonly **read** in draw/handle code are:
///
/// * `focused` — draw the focused appearance (e.g. highlight the active item).
/// * `disabled` — skip event handling; the framework gates events before the
///   view sees them, so this is mainly useful for conditional drawing.
/// * `cursor_vis` / `cursor_ins` — set these in `handle_event` to show or shape
///   the hardware cursor after editing operations.
///
/// **Dropped:** the occlusion/visibility cache flag — under whole-tree redraw +
/// diff there is nothing to cache.
///
/// # Turbo Vision heritage
/// Ports the `sf*` flag family (`views.h`) as a struct-of-bools (deviation D5);
/// each field names its `sf*` source. The dropped flag is `sfExposed` (`0x800`).
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

/// The per-view option flags — fixed, build-time choices about how a view
/// behaves (selectable, framed, centered, pre/post-process, …), as opposed to
/// the live [`State`] bits.
///
/// Set these flags on [`ViewState::options`] before inserting the view into its
/// owner. The most commonly needed flags are:
///
/// - `selectable = true` — the view can receive focus (required for input widgets).
/// - `top_select = true` — combined with `selectable`, brings windows to the front
///   on mouse-click (set automatically by [`Window`](crate::window::Window)).
/// - `first_click = true` — the click that selects the view is also delivered as a
///   `MouseDown`; without it, the first click only selects.
/// - `pre_process = true` — the view receives focused-chain events *before* the
///   focused view (e.g. a menu bar that intercepts Alt-letter hotkeys).
/// - `post_process = true` — the view receives focused-chain events *after* the
///   focused view (plain-letter hotkeys for buttons, labels, and clusters).
/// - `center_x / center_y = true` — the owner auto-centers the view's bounds on
///   that axis. Useful for dialogs placed without an exact position.
///
/// **Dropped:** the per-view back-buffer option (tvision-rs redraws the whole tree and
/// diffs). The streaming-only version bits are dropped too.
///
/// # Turbo Vision heritage
///
/// Ports the `of*` flag family (`views.h`) as a struct-of-bools (deviation D5);
/// each field names its `of*` source. The dropped options are `ofBuffered`
/// (`0x040`) and the streaming-only `ofVersion*` bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Options {
    /// `ofSelectable` — the view can become the current/focused view.
    ///
    /// All interactive widgets (buttons, input lines, list viewers, …) set this.
    /// A view without `selectable` is purely decorative: it never receives focus.
    pub selectable: bool,
    /// `ofTopSelect` — selecting the view moves it to the front of its owner.
    ///
    /// Used by [`Window`](crate::window::Window): clicking a window selects it and
    /// brings it to the top of the z-order simultaneously.
    pub top_select: bool,
    /// `ofFirstClick` — a selecting mouse-down is also passed through to the view.
    ///
    /// Without this flag, the first click on an unfocused view only focuses it;
    /// with it, the click is also delivered as a `MouseDown` event so the view
    /// can react immediately (e.g. a button that fires on the first click).
    pub first_click: bool,
    /// `ofFramed` — the view has a frame drawn around it.
    ///
    /// Informs the owner that the view manages its own border; used by
    /// [`Frame`](crate::frame::Frame) so the owner can adjust layouts.
    pub framed: bool,
    /// `ofPreProcess` — the view sees focused-chain events before the focused view.
    ///
    /// Set on views that must intercept events at the group level before the
    /// current child sees them (e.g. a menu bar intercepting Alt+letter hotkeys).
    pub pre_process: bool,
    /// `ofPostProcess` — the view sees focused-chain events after the focused view.
    ///
    /// Set on views that handle plain-letter accelerators (e.g. buttons and
    /// clusters), which fire only when no other view consumed the key first.
    pub post_process: bool,
    /// `ofTileable` — the view participates in tile/cascade layout.
    ///
    /// Set on windows that should be included when the desktop tiles or cascades.
    /// Decorative or fixed-position windows leave this `false`.
    pub tileable: bool,
    /// `ofCenterX` — the view is centered horizontally in its owner.
    ///
    /// The owner adjusts the view's `x` position to center it. Combine with
    /// `center_y` (or use [`Options::centered`]) to center on both axes.
    pub center_x: bool,
    /// `ofCenterY` — the view is centered vertically in its owner.
    ///
    /// The owner adjusts the view's `y` position to center it.
    pub center_y: bool,
    /// `ofValidate` — the view is asked to validate (`valid(Command::RELEASED_FOCUS)`) before losing focus.
    ///
    /// When set, the group calls `view.valid(Command::RELEASED_FOCUS)` before
    /// allowing focus to move away. Return `false` from `valid` to keep focus
    /// locked (e.g. an input line with a required field).
    pub validate: bool,
}

impl Options {
    /// `ofCentered` (`ofCenterX | ofCenterY`) — centered on both axes.
    ///
    /// Returns `true` only when both [`center_x`](Self::center_x) and
    /// [`center_y`](Self::center_y) are set. Note that this is a read-only
    /// predicate, not a setter — assign both fields explicitly to enable centering.
    pub fn centered(self) -> bool {
        self.center_x && self.center_y
    }
}

/// Focused-event dispatch phase.
///
/// During a focused-class dispatch (`KeyDown`/`Command`) a group walks its
/// children three times: the pre-process children, then the current child, then
/// the post-process children. The phase names which leg the receiving child is
/// being visited on:
///
/// * [`PreProcess`](Phase::PreProcess) — the pre-process walk.
/// * [`Focused`](Phase::Focused) — the focused/current delivery; the default, and
///   the value used for broadcast/positional dispatch.
/// * [`PostProcess`](Phase::PostProcess) — the leg plain-letter hotkey
///   accelerators key off (read by buttons, clusters, and labels).
///
/// Because a view has no up-pointer, the phase rides the
/// [`Context`](super::Context) as transient routing state (see
/// [`Context::phase`](super::Context::phase)).
///
/// # Turbo Vision heritage
/// Ports `phaseType` (`views.h`), originally read as an owner field
/// (`tgroup.cpp`); here it is carried on the context (deviation D4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    /// `phPreProcess` — the pre-process walk before the focused delivery.
    PreProcess,
    /// `phFocused` — the focused/current delivery (and the resting default).
    #[default]
    Focused,
    /// `phPostProcess` — the post-process walk after the focused delivery.
    PostProcess,
}

/// Grow-mode flags — control how each edge of the view tracks its owner when the
/// owner is resized.
///
/// Set the flags on [`ViewState::grow_mode`] before inserting a view into its owner.
/// The most common combinations are:
///
/// - **Stay anchored to the bottom-right corner** (e.g. a scrollbar thumb):
///   `hi_x = true; hi_y = true`
/// - **Fill the full width of the owner** (e.g. a status bar):
///   `hi_x = true` — only the right edge tracks; the left stays fixed.
/// - **Fill the entire owner** (e.g. a text viewer inside a window):
///   use [`GrowMode::grow_all()`].
/// - **Fixed size, just anchor to the right edge** (e.g. a vertical scrollbar):
///   `hi_x = true; fixed = true` (or use [`Window::standard_scroll_bar`]).
/// - **Desktop window, scales with the desktop**:
///   `hi_x = true; hi_y = true; rel = true` — all edges scale proportionally.
///
/// # Turbo Vision heritage
///
/// Ports the `gf*` flag family (`views.h`) as a struct-of-bools (deviation D5);
/// each field names its `gf*` source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct GrowMode {
    /// `gfGrowLoX` — the left edge tracks the owner's right edge.
    ///
    /// Uncommon alone; usually paired with `hi_x` when the view should slide
    /// right with its owner rather than grow.
    pub lo_x: bool,
    /// `gfGrowLoY` — the top edge tracks the owner's bottom edge.
    ///
    /// Uncommon alone; usually paired with `hi_y` when the view should slide
    /// down with its owner rather than grow.
    pub lo_y: bool,
    /// `gfGrowHiX` — the right edge tracks the owner's right edge.
    ///
    /// The most common flag: lets the view widen when its owner widens. Set alone
    /// for a view that stays left-anchored and grows to the right.
    pub hi_x: bool,
    /// `gfGrowHiY` — the bottom edge tracks the owner's bottom edge.
    ///
    /// Lets the view grow taller when its owner grows. Pair with `hi_x` for a
    /// view that fills its owner's interior (use [`GrowMode::grow_all()`]).
    pub hi_y: bool,
    /// `gfGrowRel` — grow proportionally to the owner (windows on the desktop).
    ///
    /// When set, all active `lo_*`/`hi_*` edges scale as a fraction of the owner
    /// size rather than by a fixed delta. Use for windows that should keep their
    /// relative position when the terminal is resized.
    pub rel: bool,
    /// `gfFixed` — the view keeps its size regardless of the owner's resize.
    ///
    /// The view moves to stay in the same relative position but does not change
    /// its width or height. Combine with `hi_x`/`hi_y` to anchor to the
    /// right/bottom edge while staying fixed-size (e.g. a scrollbar).
    pub fixed: bool,
}

impl GrowMode {
    /// `gfGrowAll` (`gfGrowLoX | gfGrowLoY | gfGrowHiX | gfGrowHiY`) — every edge
    /// tracks the owner (the view grows with its owner on all sides).
    ///
    /// Use for a view that should fill its owner's interior and resize with it,
    /// such as a text editor pane or a list viewer inside a window.
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

/// Drag-mode flags — control whether a view can be moved or resized interactively
/// and the owner-boundary limits applied during the drag.
///
/// Set `DragMode` on a [`ViewState`] (via `state_mut().drag_mode = …`) during
/// construction. [`Window`](crate::Window) sets both `drag_move` and `drag_grow`
/// with `limit_lo_y = true` by default so windows can be moved and resized within
/// the desktop. A plain embedded widget typically leaves all fields `false`.
///
/// The `limit_*` fields are combined with the drag rectangle at each mouse-move
/// step: a true `limit_lo_x` means the view's left edge cannot move past the
/// owner's left edge, and so on. Use [`DragMode::limit_all`] for the common case
/// of clamping all four edges.
///
/// # Turbo Vision heritage
/// Ports the `dm*` flag family (`views.h`) as a struct-of-bools (deviation D5);
/// each field names its `dm*` source. `dmDragGrowLeft` is a tvision-rs extension
/// (no C++ equivalent).
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

/// The state flags a parent group flips on a child through
/// [`View::set_state`] — the subset of [`State`] that the focus / activation
/// and visibility machinery drives, with side effects.
///
/// `Visible` is included here but does **not** propagate to children
/// (whole-tree redraw means there is no occlusion cache to maintain).
/// It is delivered per-child by [`Group::set_visible_descendant`](crate::view::Group) so
/// that widgets owning sibling scroll bars (e.g. `ListViewer`) can react. The
/// occlusion, shadow, and cursor flags remain excluded because their dropped
/// side effects are never routed through `set_state` at all.
///
/// # Turbo Vision heritage
/// The `sf*` flags routed through `set_state`: `sfActive`/`sfSelected`/
/// `sfFocused`/`sfDragging`/`sfVisible`. Excluded: `sfExposed`/`sfShadow`/`sfCursor*`.
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
    /// `sfVisible` — the view is shown/hidden.
    ///
    /// Unlike `Active`/`Selected`, this flag does NOT propagate to children
    /// (whole-tree redraw means there is no occlusion cache to maintain).
    /// Delivered by [`Group::set_visible_descendant`](crate::view::Group) so
    /// that widgets that own sibling bars (e.g. `ListViewer`, `Scroller`) can
    /// show/hide them in sync.
    Visible,
}

// ---------------------------------------------------------------------------
// ViewState — the composition target (every view's data members)
// ---------------------------------------------------------------------------

/// The data every view owns — origin, size, cursor, state/option/grow/drag
/// flags, owner id, and more.
///
/// Widgets embed a `ViewState` (typically as a field named `state`) and reach
/// its flags/geometry directly (`self.state.state.focused`, `self.state.size`).
/// The data fields are `pub`; only `resize_balance` (the resize rounding-recovery
/// accumulator) and `id` (stamped by [`Group::insert`](crate::view::Group) —
/// write-once, enforced by `pub(crate)`) are not public.
///
/// **Do not `derive(Default)`** — the all-false derive would leave the view
/// invisible with no drag limit, a silent bug. Construct via [`ViewState::new`]
/// (or [`Default`], which forwards to it with an empty rect).
///
/// # Turbo Vision heritage
/// Holds `TView`'s data members (`tview.cpp`/`views.h`). Composing this struct
/// into each widget replaces inheriting those fields (deviation D2); the packed
/// flag words become structs-of-bools (deviation D5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewState {
    /// Top-left corner of the view in its owner's coordinate space.
    ///
    /// Read this when you need the view's owner-relative position (e.g. to
    /// compute sibling offsets). Write via [`set_bounds`](Self::set_bounds) or
    /// [`move_to`](Self::move_to) — never set the field directly inside
    /// [`View::change_bounds`], which calls those helpers.
    pub origin: Point,
    /// Width and height of the view (`x` = columns, `y` = rows).
    ///
    /// Together with [`origin`](Self::origin) this defines the view's bounds in
    /// owner space. Read it via [`get_bounds`](Self::get_bounds) /
    /// [`get_extent`](Self::get_extent); set via `set_bounds` or `grow_to`.
    pub size: Point,
    /// View-local hardware-cursor position (column, row) within this view.
    ///
    /// Set via [`set_cursor`](Self::set_cursor). It is only shown when both
    /// [`State::cursor_vis`] is `true` and the view is focused; the event
    /// loop reads this field and places the hardware cursor in absolute
    /// coordinates (the loop knows the tree; a leaf does not). Reading the
    /// field directly is fine; prefer [`View::cursor_request`] when you need
    /// the loop-side absolute-cursor logic.
    pub cursor: Point,
    /// State flags.
    pub state: State,
    /// Option flags.
    pub options: Options,
    /// Grow-mode flags.
    pub grow_mode: GrowMode,
    /// Drag-mode flags.
    pub drag_mode: DragMode,
    /// Which opt-in event classes this view wants. The always-delivered classes
    /// (mouse-down, key-down, command) are unconditional rather than opt-in, so
    /// only the mouse-move / auto-repeat opt-ins survive here.
    pub event_mask: EventMask,
    /// The help context for status-line switching while this view is focused.
    ///
    /// Set during construction (e.g. `state.help_ctx = HelpCtx::custom("myapp.topic")`).
    /// The value [`HelpCtx::NO_CONTEXT`] (the ctor default) means no context is
    /// provided; the status line typically falls back to the owner's context in
    /// that case. While the view is being dragged, [`get_help_ctx`](Self::get_help_ctx)
    /// returns [`HelpCtx::DRAGGING`] regardless of this field.
    pub help_ctx: HelpCtx,
    /// This view's global identity, set by [`Group::insert`](crate::view::Group)
    /// when the view enters a group; `None` before insertion. NOT an up-pointer
    /// — it is the view's own handle (like an ECS entity id), which lets a
    /// handler/loop address it by id.
    pub(crate) id: Option<ViewId>,
    /// Resize rounding-recovery accumulator, used by proportional grow. Private:
    /// only `ViewState::calc_bounds` touches it.
    resize_balance: Point,
}

impl ViewState {
    /// Construct view state for `bounds`, with the canonical view defaults.
    ///
    /// Defaults: `visible`, the top-edge drag limit, no help context, everything
    /// else off. `event_mask` is all-false because the always-delivered classes
    /// (mouse-down, key-down, command) are unconditional rather than opt-in.
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

    /// The view's bounds in owner coordinates: `{ origin, origin + size }`.
    ///
    /// Use this whenever you need the half-open rect that describes where the
    /// view sits in its parent's frame, e.g. for hit-testing or sibling-offset
    /// calculations. For the same rect anchored at `(0, 0)` (local frame), use
    /// [`get_extent`](Self::get_extent) instead.
    pub fn get_bounds(&self) -> Rect {
        Rect::from_points(self.origin, self.origin + self.size)
    }

    /// The view's extent in its own local coordinates: `{ 0, 0, size.x, size.y }`.
    ///
    /// Use this inside [`View::draw`] to fill the whole view, clip child rects, or
    /// compute layout in view-local space. Because the origin is always `(0, 0)`,
    /// there is no translation needed.
    pub fn get_extent(&self) -> Rect {
        Rect::new(0, 0, self.size.x, self.size.y)
    }

    /// Set the bounds: `origin = bounds.a; size = bounds.b - bounds.a`.
    ///
    /// The low-level primitive used by [`move_to`](Self::move_to),
    /// [`grow_to`](Self::grow_to), and [`ViewState::new`]. Call through
    /// [`View::change_bounds`] in normal use so that groups can propagate the
    /// resize to their children.
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.origin = bounds.a;
        self.size = bounds.b - bounds.a;
    }

    /// Relocate the top-left to `(x, y)`, keeping the size.
    ///
    /// A low-level position change that updates only the bounds fields (no
    /// redraw, no size-limit clamp). Typically called from within a group's
    /// layout pass or from [`View::change_bounds`]; the loop's whole-tree
    /// repaint redraws the view at its new position automatically.
    pub fn move_to(&mut self, x: i32, y: i32) {
        self.set_bounds(Rect::new(x, y, x + self.size.x, y + self.size.y));
    }

    /// Keep the origin, set the size to `(x, y)`.
    ///
    /// A low-level size change symmetric with [`move_to`](Self::move_to): no
    /// redraw or size-limit clamp. Size-limit enforcement lives on the group (which
    /// calls [`View::size_limits`] before adjusting children); use
    /// [`View::change_bounds`] or the free function `locate` for the full path.
    pub fn grow_to(&mut self, x: i32, y: i32) {
        self.set_bounds(Rect::new(
            self.origin.x,
            self.origin.y,
            self.origin.x + x,
            self.origin.y + y,
        ));
    }

    // -- Verb helpers (the dropped redraw side effects noted inline) --

    /// Make the view visible (`state.visible = true`).
    ///
    /// The plain field write; the loop's whole-tree redraw picks up the change
    /// on the next pump. Call this inside a widget's constructor or a layout
    /// pass. To show a view that is already inserted into a live group (so that
    /// sibling scroll bars track it), use
    /// [`Context::request_set_visible`](crate::view::Context::request_set_visible)
    /// (deferred, runs the owner's currency tail).
    pub fn show(&mut self) {
        self.state.visible = true;
    }

    /// Make the view invisible (`state.visible = false`).
    ///
    /// Like [`show`](Self::show): a plain field flip for use during construction
    /// or layout. To hide a view that is already inserted into a live group
    /// (and keep sibling bars in sync), use
    /// [`Context::request_set_visible`](crate::view::Context::request_set_visible).
    pub fn hide(&mut self) {
        self.state.visible = false;
    }

    /// Move the cursor to view-local `(x, y)`.
    ///
    /// Call this inside `draw` or `handle_event` to position the insertion
    /// point (e.g. an input line moving the caret after editing). The cursor
    /// is only shown when the view is focused and [`State::cursor_vis`] is
    /// `true`; make both conditions hold with [`show_cursor`](Self::show_cursor).
    /// The event loop translates the view-local position to absolute screen
    /// coordinates using the tree — no manual offset is needed here.
    pub fn set_cursor(&mut self, x: i32, y: i32) {
        self.cursor = Point::new(x, y);
    }

    /// Make the hardware cursor visible while this view is focused.
    ///
    /// Sets [`State::cursor_vis`]. The event loop places the cursor at the
    /// absolute screen position corresponding to [`cursor`](Self::cursor). Call
    /// once in your widget's constructor (or just after [`set_cursor`](Self::set_cursor))
    /// if you want an insertion-point cursor; pair with [`block_cursor`](Self::block_cursor)
    /// or [`normal_cursor`](Self::normal_cursor) to pick the shape.
    pub fn show_cursor(&mut self) {
        self.state.cursor_vis = true;
    }

    /// Hide the hardware cursor while this view is focused.
    ///
    /// Clears [`State::cursor_vis`]. The hardware cursor will not be shown even
    /// if the view is focused. Widgets that draw a cursor in software (e.g. by
    /// painting a highlighted cell) call this to suppress the hardware cursor.
    pub fn hide_cursor(&mut self) {
        self.state.cursor_vis = false;
    }

    /// Switch the hardware cursor to block (insert / overwrite) shape.
    ///
    /// Sets [`State::cursor_ins`]; [`normal_cursor`](Self::normal_cursor) clears
    /// it. Many input-line widgets switch between shapes depending on an insert
    /// mode toggle, calling this or `normal_cursor` on each toggle.
    pub fn block_cursor(&mut self) {
        self.state.cursor_ins = true;
    }

    /// Switch the hardware cursor to underline (normal) shape.
    ///
    /// Clears [`State::cursor_ins`]. This is the default shape; call this after
    /// [`block_cursor`](Self::block_cursor) when leaving insert mode.
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

    /// Returns the effective help context: [`HelpCtx::DRAGGING`] while the view
    /// is being dragged, otherwise [`help_ctx`](Self::help_ctx).
    ///
    /// The status line calls this (via [`View::get_help_ctx`]) on the focused
    /// view to decide which help topic to display. Usually you read
    /// `view.help_ctx` directly; call this method only when you need the
    /// drag-override behavior included.
    pub fn get_help_ctx(&self) -> HelpCtx {
        if self.state.dragging {
            HelpCtx::DRAGGING
        } else {
            self.help_ctx
        }
    }

    /// Flip the [`State`] bool named by `flag` — the plain field write for each
    /// propagating [`StateFlag`]. The broadcast / propagation side effects live
    /// in [`View::set_state`], not here.
    pub fn set_flag(&mut self, flag: StateFlag, enable: bool) {
        match flag {
            StateFlag::Active => self.state.active = enable,
            StateFlag::Selected => self.state.selected = enable,
            StateFlag::Focused => self.state.focused = enable,
            StateFlag::Dragging => self.state.dragging = enable,
            StateFlag::Visible => self.state.visible = enable,
        }
    }

    // -- Resize math (size limits + bounds recompute, pure functions) --------

    /// The `(min, max)` size this view may take inside an owner of `owner_size`.
    ///
    /// Default: `min = (0, 0)`; `max = owner_size` unless `grow_mode.fixed`, in
    /// which case `max = (i32::MAX, i32::MAX)`. Widgets (e.g. windows) override to
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

    /// The new bounds for this view after its owner resized from `owner_size -
    /// delta` to `owner_size` (so `delta = new - old`).
    ///
    /// Applies each enabled grow-mode edge via `grow`, then clamps to
    /// `(min_lim, max_lim)` through `fit_to_limits`, updating the private
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
    /// Empty-rect view state with the canonical defaults (visible, top-edge drag
    /// limit). **Not** a `derive` — the all-false derive would be a silent bug.
    fn default() -> Self {
        ViewState::new(Rect::default())
    }
}

// -- resize-math private helpers (ports of the tview.cpp statics) -------------

/// Clamp `val` into `[min, max]`, with `min` pinned to `max` if inverted.
fn range(val: i32, min: i32, max: i32) -> i32 {
    let min = if min > max { max } else { min };
    val.clamp(min, max)
}

/// Clamp `bounds` to the view's size limits and apply them if they changed.
///
/// A **free function** over `&mut dyn View` (NOT a [`View`] trait method): a
/// trait method would be forwarded by the `#[delegate]` macro to the *inner
/// group* for wrappers like [`Window`](crate::widgets::window::Window), whose
/// group has a 0×0 [`size_limits`](View::size_limits), bypassing the window's
/// 16×6 minimum (the hazard at `window.rs`). As a free fn,
/// [`size_limits`](View::size_limits) dispatches virtually to the wrapper's
/// override and [`change_bounds`](View::change_bounds) forwards to the group.
/// The repaint/shadow tail is moot under whole-tree redraw. Backs the desktop's
/// [`tile`/`cascade`](crate::desktop::Desktop).
///
/// # Turbo Vision heritage
/// Ports `TView::locate` (`tview.cpp`).
pub(crate) fn locate(view: &mut dyn View, mut bounds: Rect, owner_size: Point) {
    let (min, max) = view.size_limits(owner_size);
    bounds.b.x = bounds.a.x + range(bounds.b.x - bounds.a.x, min.x, max.x);
    bounds.b.y = bounds.a.y + range(bounds.b.y - bounds.a.y, min.y, max.y);
    if bounds != view.state().get_bounds() {
        view.change_bounds(bounds);
    }
}

/// Fit `val` into `[min, max]` while accumulating the remainder in `balance`, so
/// a later resize can give the size back.
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

/// `b = a + balanced_range(b - a, min, max, balance)`.
fn fit_to_limits(a: i32, b: &mut i32, min: i32, max: i32, balance: &mut i32) {
    *b = a + balanced_range(*b - a, min, max, balance);
}

/// Advance one bound coordinate `i` for owner dimension `s` (the *new* owner
/// size) and delta `d`.
///
/// For the proportional grow mode (`grow_mode.rel`) the bound scales
/// proportionally; `s - d` is the *old* owner size, and the `if s != d` guard
/// avoids dividing by a zero old size. Otherwise the bound shifts by `d`.
///
/// Coordinate-magnitude assumption: `*i * s` is `i32 * i32` and assumes
/// screen-scale coordinates well below `i32::MAX`; an out-of-range coordinate
/// would panic in debug (overflow) rather than silently wrap.
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
// View trait (the per-view behavior every widget implements)
// ---------------------------------------------------------------------------

/// The behavior every view implements. Widgets supply [`state`](View::state) /
/// [`state_mut`](View::state_mut) / [`draw`](View::draw); the rest default.
///
/// # Turbo Vision heritage
/// Ports `TView`'s virtual methods (`tview.cpp`/`views.h`). Inheritance becomes
/// this trait plus a composed [`ViewState`] (deviation D2); methods that reached
/// up an owner pointer instead take the downward
/// [`Context`](crate::view::Context) (deviation D3).
// MAINTENANCE: when adding a defaulted method to this trait, also add a
// forwarder entry to `tvision-rs-macros/src/specs.rs` (`view()`) AND the
// `expected` list in `tests/delegate_view.rs`. Required methods (no default)
// catch omission at compile time; defaulted ones would silently fall back to
// the default at every `#[delegate]` site if the forwarder is missing.
pub trait View {
    /// Borrow the embedded [`ViewState`] (the view's data members).
    fn state(&self) -> &ViewState;

    /// Mutably borrow the embedded [`ViewState`].
    fn state_mut(&mut self) -> &mut ViewState;

    /// Paint the view through `ctx`. **Must be overridden.**
    ///
    /// There is no sensible default paint (a bare view has no palette to fill
    /// with and is never instantiated), so `draw` is required rather than
    /// defaulted.
    fn draw(&mut self, ctx: &mut DrawCtx);

    /// Handle an event. The **base is a no-op** (the event passes through).
    ///
    /// The one piece of shared base behavior — auto-selecting a view on a
    /// selecting mouse-down — lives on [`Group`](crate::view::Group), because it
    /// drives focus from the parent down.
    fn handle_event(&mut self, _ev: &mut Event, _ctx: &mut Context) {}

    /// Flip a propagating state flag and run its side effects. The base flips the
    /// flag and, for [`StateFlag::Focused`], emits the focus broadcast
    /// (`RECEIVED_FOCUS`/`RELEASED_FOCUS`) via `ctx`.
    ///
    /// The broadcast carries the view whose focus changed as its `source`
    /// (`self.state().id()`); it goes to the loop's event queue rather than to a
    /// specific receiver. A [`Group`](crate::view::Group) overrides this to also
    /// propagate to its children. The flags handled directly on [`ViewState`]
    /// (visibility, cursor, shadow) are not driven through this hook.
    fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
        self.state_mut().set_flag(flag, enable);
        if flag == StateFlag::Focused {
            let source = self.state().id(); // the view whose focus changed
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

    /// Whether the view is in a valid state for `cmd` (e.g. a modal end / focus
    /// release). Base is always `true`.
    ///
    /// **Carries `&mut Context`** (the async-modal-from-a-view seam): a control's
    /// validation may need to pop a modal message box (a validator error, the
    /// [`FileEditor`](crate::widgets::FileEditor) modified-save prompt) and observe
    /// the answer. A downward-borrowed `&mut View` cannot run a nested modal
    /// inline, so it **requests** one via [`Context::request_message_box`] and
    /// re-validates once the answer is routed back (see
    /// `docs/design/async-modal-from-view.md`).
    fn valid(&mut self, _cmd: Command, _ctx: &mut Context) -> bool {
        true
    }

    /// Stash the user's choice from an async modal message box this view
    /// requested via [`Context::request_message_box`] (the
    /// `answer_to`/`RouteModalAnswer` round-trip). Default: no-op. Overridden by
    /// views that drive a Yes/No/Cancel prompt out of `valid` (e.g.
    /// [`FileEditor`](crate::widgets::FileEditor) caches it for the re-validate).
    fn set_modal_answer(&mut self, _cmd: Command) {}

    /// This control's current value as a [`FieldValue`], or `None` for a view
    /// that carries no transferable data.
    ///
    /// The dialog gather walk calls this on every child in insertion order and
    /// collects the results into a `Vec<Option<FieldValue>>`. Override in a
    /// data-bearing control (e.g. `InputLine` returns `FieldValue::Text`,
    /// `ScrollBar` returns `FieldValue::Int`). The base returns `None` — a bare
    /// view carries nothing. Controls that don't participate (buttons, labels)
    /// simply leave the base in place.
    fn value(&self) -> Option<FieldValue> {
        None
    }

    /// Load a typed [`FieldValue`] into this control.
    ///
    /// The dialog scatter walk calls this on every child in insertion order,
    /// distributing an edited record. Override in a data-bearing control; the base
    /// ignores the call. A control **should** silently ignore a [`FieldValue`]
    /// variant it does not understand (e.g. an `InputLine` ignores `FieldValue::Int`)
    /// so that record schemas with mixed control types work without downcasting.
    fn set_value(&mut self, _v: FieldValue) {}

    /// Deliver a finished modal's **typed result record** to the view that
    /// launched it. Defaulted no-op; a launcher view overrides it to load the
    /// result the pump read out of the modal's fields (each via [`View::value`])
    /// and packed into an ordered [`FieldValue::List`](crate::data::FieldValue::List).
    ///
    /// Distinct from [`set_value`](View::set_value): `set_value` carries a view's
    /// **own** field/document data (e.g. the editor's text), whereas
    /// `set_modal_data` carries a **separate modal-result record** addressed to the
    /// launcher by id — the two channels must not collide. Driven from
    /// `apply_modal_completion` (the cluster-D modal-result path); the pump resolves
    /// the launcher with `group.find_mut(id)` and calls this method by **virtual
    /// dispatch**, never a downcast.
    ///
    /// # Turbo Vision heritage
    /// The return-less successor to delivering a dialog's gathered record back to
    /// the requester (`getData` read at the modal's close, handed to the owner).
    fn set_modal_data(&mut self, _data: crate::data::FieldValue) {}

    /// Scatter a typed [`FieldValue`] into this control with a `Context`. Default:
    /// calls [`set_value`](Self::set_value) (the context-free setter). Override
    /// when scatter needs deferred publishing (e.g.
    /// [`ListBox`](crate::widgets::ListBox) republishes its vertical scrollbar via
    /// `focus_item`).
    fn set_value_ctx(&mut self, v: FieldValue, ctx: &mut Context) {
        let _ = ctx;
        self.set_value(v);
    }

    /// Post-construction initialization hook. Called once after the view has
    /// been inserted into its group and the group's layout is complete — the
    /// earliest point at which a view may safely resolve sibling ids or run
    /// initialization that requires a fully-wired tree. Override when a widget
    /// needs to do work that cannot happen during construction (e.g. reading a
    /// linked control's current value). Base is a no-op.
    fn awaken(&mut self) {}

    /// The `(min, max)` size this view may take inside an owner of `owner_size`.
    ///
    /// Returns `(Point::new(0,0), owner_size)` by default, or
    /// `(0, (i32::MAX, i32::MAX))` when [`GrowMode::fixed`](GrowMode) is set
    /// (the view does not track the owner's size). Override to impose a minimum
    /// size: `Window` returns a 16×6 minimum so the frame stays readable. The
    /// result is consumed by [`View::calc_bounds`] and the free function
    /// `locate`; callers generally should not call this directly — use
    /// `calc_bounds` instead, which keeps the two in sync.
    fn size_limits(&self, owner_size: Point) -> (Point, Point) {
        self.state().size_limits(owner_size)
    }

    /// The new bounds after the owner resized to `owner_size` (`delta = new -
    /// old`). Computes the size limits via the overridable
    /// [`size_limits`](View::size_limits) hook — so a widget override (e.g. a
    /// window's minimum) participates — then delegates the grow math to
    /// `ViewState::calc_bounds` (the single source). Returns the bounds; it does
    /// not apply them ([`change_bounds`](View::change_bounds) does).
    fn calc_bounds(&mut self, owner_size: Point, delta: Point) -> Rect {
        let (min_lim, max_lim) = self.size_limits(owner_size);
        self.state_mut()
            .calc_bounds(owner_size, delta, min_lim, max_lim)
    }

    /// Apply `bounds` to this view and propagate as needed.
    ///
    /// The base implementation sets the bounds fields via
    /// [`ViewState::set_bounds`] with no redraw (whole-tree redraw is automatic).
    /// Groups and windows override to call `calc_bounds`/`change_bounds` on each
    /// child so they track the resize. When you need to resize a view from
    /// outside the tree (e.g. a capture handler), prefer
    /// [`Context::request_bounds`](crate::view::Context::request_bounds) (deferred)
    /// over calling this directly — the tree is `&mut`-borrowed during dispatch.
    fn change_bounds(&mut self, bounds: Rect) {
        self.state_mut().set_bounds(bounds);
    }

    /// Called by the pump after [`change_bounds`](Self::change_bounds) is applied via
    /// [`Deferred::ChangeBounds`](crate::view::Deferred::ChangeBounds) — provides a
    /// `Context` for re-publishing state that depends on the new bounds (e.g. scrollbar
    /// params). Default implementation is a no-op.
    ///
    /// Scrollers re-apply their scroll limit after a resize, and list viewers
    /// re-publish their step params; both do so by overriding this hook in their
    /// respective concrete types.
    fn on_bounds_changed(&mut self, _ctx: &mut Context) {}

    /// The view-local hardware-cursor position this view wants shown, or `None` to
    /// hide it. Base: `Some(cursor)` iff the view is focused with a visible cursor,
    /// else `None`.
    ///
    /// This is the top-down realization of the focused-chain cursor walk: the live
    /// event loop asks the root for the absolute cursor each pass.
    /// [`Group`](crate::view::Group) overrides this to descend into its current
    /// child, accumulating the child's origin at each level.
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
    /// the "tree-walk via Context" — the uniform way the event loop
    /// / a capture handler acts on a view it holds only by id (move a window's
    /// bounds, flip the dragging flag, …).
    fn find_mut(&mut self, id: ViewId) -> Option<&mut dyn View> {
        let _ = id;
        None
    }

    /// Remove the descendant named by `id` from whichever group owns it.
    /// Returns `true` if it was found+removed. Distinct
    /// from [`find_mut`](View::find_mut) because removal happens in the *owner's*
    /// child `Vec` (a view cannot remove itself — it doesn't know its owner)
    /// and must run the owning group's `reset_current`. Base: `false` (a leaf owns
    /// nothing).
    fn remove_descendant(&mut self, id: ViewId, ctx: &mut Context) -> bool {
        let _ = (id, ctx);
        false
    }

    /// Focus (select) the descendant named by `id` within whichever group owns it
    /// — the tree-op behind a label focusing its linked control. Returns `true`
    /// if `id` was found in this subtree (selectable or not — finding it stops the
    /// walk). Distinct from [`find_mut`](View::find_mut) because focusing happens in
    /// the *owning group* (a view cannot select itself within itself): the
    /// owning [`Group`](crate::view::Group) calls `focus_child` after applying the
    /// `selectable` gate. Base: `false` (a leaf owns nothing).
    ///
    /// **Scope:** this focuses the link *within its owning group*, not the full
    /// ancestor chain. That is correct for the label/link sibling case (label and
    /// link share a group already on the focused path); a cross-group link would
    /// need an up-chain walk, which has no consumer.
    fn focus_descendant(&mut self, id: ViewId, ctx: &mut Context) -> bool {
        let _ = (id, ctx);
        false
    }

    /// Whether this view's subtree contains a focusable leaf (visible, enabled,
    /// selectable). Base: whether THIS view is itself a focusable leaf; `Group`
    /// overrides to recurse into children; wrapper views delegate. Used by the
    /// hierarchical Tab traversal to skip selectable-but-empty subtrees.
    fn has_focusable_leaf(&self) -> bool {
        let s = self.state();
        s.state.visible && !s.state.disabled && s.options.selectable
    }

    /// Focus the first (Tab, `backward = false`) or last (Shift-Tab,
    /// `backward = true`) focusable leaf in this view's subtree, establishing the
    /// full focus path down to it. Returns `true` if a focusable leaf was focused.
    ///
    /// Base: a leaf reports whether IT is focusable — its owning group has already
    /// made it current, so there is nothing further to descend into. `Group`
    /// overrides to pick its edge child and recurse; wrapper views delegate. Used
    /// by the hierarchical Tab traversal to ENTER a group at its edge.
    fn focus_to_edge(&mut self, backward: bool, ctx: &mut Context) -> bool {
        let _ = (backward, ctx);
        let s = self.state();
        s.state.visible && !s.state.disabled && s.options.selectable
    }

    /// Establish this view's INTERNAL currency — for a group-bearing view, set its
    /// current child to the first visible+selectable child. The insert-time
    /// currency cascade cannot run inside the context-less `Group::insert`, so
    /// `exec_view` calls this on a freshly-inserted modal BEFORE focusing it, so
    /// the modal's first selectable child is current on open (otherwise the modal
    /// is keyboard-dead until a nav event — see the seam note in `exec_view`).
    /// Base: no-op (a leaf has no internal currency); `Group` overrides;
    /// `Window`/`Dialog` delegate.
    fn reset_current(&mut self, _ctx: &mut Context) {}

    /// Run any pending insert-time reset-current cascades in this subtree.
    ///
    /// When a visible, selectable view is inserted, the owning group must re-pick
    /// its current child. The context-less `Group::insert` cannot run that at
    /// insert time; instead the insert marks the group `currency_dirty` and the
    /// pump / `Program::new` settles it here, BEFORE the next event pick.
    ///
    /// Post-order (children first): a child group's currency exists before its
    /// owner's focus cascade descends into it. Runs the INHERENT
    /// `Group::reset_current` (not the virtual one) — embedders that key one-time
    /// init off `reset_current` (a file dialog's initial directory read) get it
    /// from `exec_view`'s kept virtual call instead, never from the settle pass.
    /// Base: no-op (a leaf has no children); `Group` overrides; embedders forward
    /// via `#[delegate]` (the specs.rs forwarder).
    fn settle_currency(&mut self, _ctx: &mut Context) {}

    /// Tree-op: set the `visible` flag of the descendant named by `id` from its
    /// OWNING group, running the owning group's currency tail. Toggling
    /// visibility on a selectable view re-picks the owner's current child, in both
    /// directions (show and hide). Returns `true` if `id` was found in this
    /// subtree.
    ///
    /// Symmetric with [`remove_descendant`](View::remove_descendant) /
    /// [`focus_descendant`](View::focus_descendant): the flag write and the
    /// `reset_current` happen in the *owning group* (a view cannot re-current its
    /// owner — it doesn't know it). Backs
    /// [`Deferred::SetVisible`](crate::view::Deferred::SetVisible) (a scroller
    /// showing/hiding its scrollbar). Base: `false` (a leaf owns nothing).
    fn set_visible_descendant(&mut self, id: ViewId, visible: bool, ctx: &mut Context) -> bool {
        let _ = (id, visible, ctx);
        false
    }

    /// The window number for Alt-N selection. Base views are unnumbered (`None`);
    /// [`Window`](crate::window::Window) overrides to return its number when `> 0`
    /// (0 means unnumbered → `None`).
    fn number(&self) -> Option<i16> {
        None
    }

    /// Whether a mouse-down inside this view should auto-select (focus) it — the
    /// per-view opt-out for the group's mouse-down auto-select (see
    /// `Group::route_event`). Default `true` (the common case); a
    /// [`Button`](crate::widgets::Button) overrides to return its grab-focus flag.
    /// A view returning `false` is **not** focused by a click but still
    /// **receives** the click (so it can act, e.g. press, without becoming the
    /// current view).
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

    /// Tree-op: lay this subtree's tileable windows out in a grid (the tile
    /// command). Base: no-op (only [`Desktop`](crate::desktop::Desktop) lays out
    /// windows). `r` is the desktop-local layout rect. Mirrors
    /// [`select_window_num`](View::select_window_num): the live loop drives the
    /// desktop by id through `&mut dyn View`.
    fn tile(&mut self, _r: Rect) {}

    /// Tree-op: lay this subtree's tileable windows out cascaded (the cascade
    /// command). Base: no-op; [`Desktop`](crate::desktop::Desktop) overrides.
    fn cascade(&mut self, _r: Rect) {}

    /// The shared scrollbar read-sync broker hook. Defaulted no-op; scroll-aware
    /// widgets override it to apply a freshly-read scrollbar delta to themselves.
    /// The pump passes the horizontal/vertical scrollbar values (`None` if that bar
    /// is absent or unresolved), each read via [`View::value`]. Overridden by the
    /// list viewers (delegate to
    /// [`list_viewer::apply_scroll`](crate::widgets::list_viewer::apply_scroll)),
    /// [`Scroller`](crate::widgets::Scroller), the outline viewer, and the editor —
    /// every sibling-scrollbar *read* sync routes through this one method instead of
    /// a pump downcast.
    ///
    /// Each widget interprets `None` per its own semantics (a read-only scroller
    /// treats a missing bar as delta `0`; the editor preserves `None` to skip that
    /// axis). Driven by [`Deferred::ScrollSync`](crate::view::Deferred::ScrollSync).
    fn apply_scroll_sync(&mut self, _h: Option<i32>, _v: Option<i32>, _ctx: &mut Context) {}

    /// The menu command-graying broker hook. Defaulted no-op; menu views override
    /// to re-gray their menu tree against the program's live **disabled-command
    /// set** (denylist — the argument is the set of commands currently *disabled*;
    /// an item grays iff its command is in it). The free fn
    /// [`menu::menu_view::update_menu_commands`](crate::menu::menu_view::update_menu_commands)
    /// implements the re-gray walk.
    ///
    /// A menu view (a child) cannot borrow the program's
    /// [`CommandSet`](crate::CommandSet) inline — the pump owns it, and storing a
    /// `&CommandSet` on [`Context`] would alias the apply-loop's mutation of the
    /// disabled set. So the view requests
    /// [`Deferred::UpdateMenu`](crate::view::Deferred::UpdateMenu) by its own id,
    /// and the pump calls back here at apply time with the live set in hand. (A
    /// plain *read* needs no broker — `Context::command_enabled` answers from an
    /// owned per-pump snapshot.) No "changed" return is needed — under whole-tree
    /// redraw the next pump repaints unconditionally.
    fn update_menu_commands(&mut self, _disabled_cmds: &CommandSet) {}

    /// The menu-highlight write-back hook. Defaulted no-op; menu views
    /// ([`MenuBar`](crate::menu::MenuBar) / [`MenuBox`](crate::menu::MenuBox))
    /// override to set their
    /// [`MenuViewState::current`](crate::menu::MenuViewState) — the **write-only
    /// display cache** their `draw` reads to pick the selected colour.
    ///
    /// While a menu session is active the
    /// [`MenuSession`](crate::menu::MenuSession) capture handler owns the
    /// interaction; the boxes are never focused and run no event logic. When the
    /// session changes a level's highlight it requests
    /// [`Deferred::SetMenuCurrent`](crate::view::Deferred::SetMenuCurrent) by the
    /// box/bar id, and the pump calls back here at apply time. A trait method (not
    /// a downcast) keeps the broker uniform across the two concrete menu views,
    /// exactly like [`update_menu_commands`](View::update_menu_commands).
    fn set_menu_current(&mut self, _current: Option<usize>) {}

    /// The effective help context for status-line switching. Returns
    /// [`HelpCtx::DRAGGING`] while the view is being dragged, otherwise the
    /// view's own [`ViewState::help_ctx`].
    ///
    /// The status-line widget calls this on the currently focused view each pump
    /// to decide which help topic to display. Leaf views need not override —
    /// the default delegates to [`ViewState::get_help_ctx`] and the `#[delegate]`
    /// macro forwards it automatically through wrapper types. Override only when
    /// the type aggregates children's contexts (like `Group`, which descends into
    /// its current child).
    fn get_help_ctx(&self) -> HelpCtx {
        self.state().get_help_ctx()
    }

    /// Downcast hook for the rare parent→child push that needs the concrete type
    /// (e.g. a window pushing its zoomed flag to its frame). Base returns `None`;
    /// only views that must be reached concretely override it. (`Any` requires
    /// `'static`, which every view is.)
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        None
    }

    /// Resolve descendant `id` to its **absolute** bounds, given `acc` = this
    /// view's own absolute origin. Base returns `None` (a leaf owns no
    /// descendants); a [`Group`](crate::view::Group) overrides to walk its children
    /// accumulating origins, and a `Group`-embedding view delegates to its inner
    /// group. The [`THistory`](crate::widgets::THistory) open path needs its linked
    /// input line's bounds in the **root/absolute** frame, because `exec_view`
    /// root-inserts the modal and the modal frame hit-tests in absolute coords (the
    /// documented ROOT-INSERT + (0,0) caveat). Mirrors
    /// [`find_mut`](View::find_mut)'s recursion but returns geometry, not a borrow.
    fn descendant_global_bounds(&self, _id: ViewId, _acc: Point) -> Option<Rect> {
        None
    }

    /// The editor→indicator status-push broker hook. Defaulted no-op; the editor's
    /// status [`Indicator`](crate::widgets::Indicator) overrides it to store the new
    /// cursor `location` + `modified` flag. Driven by
    /// [`Deferred::IndicatorSetValue`](crate::view::Deferred::IndicatorSetValue):
    /// the editor (a leaf) cannot reach its indicator sibling inline, so it requests
    /// this and the pump calls the method by id — virtual dispatch, not a downcast.
    fn set_indicator_value(&mut self, _location: Point, _modified: bool) {}

    /// The tab-bar→page-stack switch broker hook. Defaulted no-op;
    /// [`PageStack`](crate::widgets::PageStack) overrides it to make page `idx`
    /// active. Driven by
    /// [`Deferred::PageStackSync`](crate::view::Deferred::PageStackSync): the pump
    /// reads the bound tab bar's `value` and calls this method by id — virtual
    /// dispatch, not a downcast.
    fn apply_page_sync(&mut self, _idx: usize, _ctx: &mut Context) {}
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
        // eventMask all-false (the three TV bits are unconditional).
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
        let mut v = FillView {
            st: ViewState::new(Rect::new(0, 0, 1, 1)),
            ch: ' ',
        };
        let mut out = std::collections::VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        assert!(v.valid(Command::OK, &mut ctx));
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
