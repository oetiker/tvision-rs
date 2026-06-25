//! [`Window`] core — see the [module docs](super) for the overview.

use crate::capture::{CaptureFlow, CaptureHandler};
use crate::command::Command;
use crate::event::{Event, Key};
use crate::frame::Frame;
use crate::view::{
    Context, DividerOp, DragMode, DrawCtx, Group, GrowMode, Point, Rect, StateFlag, View, ViewId,
};
use crate::widgets::{Orientation, ScrollBar};

// ---------------------------------------------------------------------------
// WindowFlags
// ---------------------------------------------------------------------------

/// Window decoration flags: which of move / grow / close / zoom the window
/// allows.
///
/// Build a customised `WindowFlags` (e.g. at construction) to change which
/// controls appear on the frame. The default (via `Default`) enables all
/// four. Disable individual fields to suppress a decoration:
///
/// ```rust
/// use tvision_rs::window::WindowFlags;
/// let no_zoom = WindowFlags { zoom: false, ..Default::default() };
/// assert!(!no_zoom.zoom);
/// ```
///
/// **`r#move` spelling:** `move` is a Rust keyword, so the field is accessed as
/// `flags.r#move`. The raw-identifier syntax is a Rust-only artefact; at the
/// semantic level it is identical to the C++ `wfMove` bit.
///
/// These flags belong to the window; the [`Frame`] receives a pushed-down copy
/// via [`Frame::set_flags`](crate::frame::Frame::set_flags) and renders the
/// corresponding icons (close `[×]`, zoom `[↕]`, grow corner `▐`).
///
/// # Turbo Vision heritage
///
/// Ports the `wfMove / wfGrow / wfClose / wfZoom` flag word (`views.h`) as a
/// struct-of-bools.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowFlags {
    /// The window can be moved by dragging its frame.
    pub r#move: bool,
    /// The window can be resized by dragging its bottom corners.
    pub grow: bool,
    /// The window shows a close icon (and accepts a close command).
    pub close: bool,
    /// The window shows a zoom icon (and accepts a zoom command).
    pub zoom: bool,
}

// ---------------------------------------------------------------------------
// WindowPalette
// ---------------------------------------------------------------------------

/// Which colour scheme the window draws in.
///
/// Choose the variant that matches the window's purpose:
///
/// - [`Blue`](WindowPalette::Blue) — plain application windows; the default.
///   Draws in the `Role::FrameActive` / `FramePassive` / `FrameDragging` /
///   `FrameIcon` / `Role::ScrollBar*` / `Role::Scroller*` role family.
/// - [`Cyan`](WindowPalette::Cyan) — secondary / highlight windows (e.g.
///   help viewers). Draws in the `Role::FrameCyanActive` / `FrameCyanPassive`
///   / `FrameCyanIcon` family.
/// - [`Gray`](WindowPalette::Gray) — dialog boxes. [`Dialog`](crate::dialog::Dialog)
///   sets this automatically; use it directly when building a dialog-styled
///   custom window. Draws in the `Role::FrameGrayActive` / `FrameGrayPassive`
///   / `FrameGrayIcon` family.
///
/// The scheme is stored on the [`Window`] and pushed down to the [`Frame`]
/// child automatically; no manual frame update is needed when the palette
/// changes.
///
/// # Turbo Vision heritage
///
/// Replaces the `wpBlueWindow = 1 / wpCyanWindow = 2 / wpGrayWindow = 3`
/// integer palette selector (`views.h`; the guide calls these
/// `dpBlueDialog / dpCyanDialog / dpGrayDialog`) with a typed enum, which the
/// frame maps to named `Role` entries in the theme. The eight
/// per-palette entries described in the guide (frame passive, frame active,
/// frame icon, scrollbar page, scrollbar controls, scroller normal, scroller
/// selected, reserved) are expressed as named `Role` variants rather than
/// integer indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WindowPalette {
    /// The default window scheme (`Role::Frame*` family).
    #[default]
    Blue,
    /// The cyan scheme (`Role::FrameCyan*` family).
    Cyan,
    /// The gray scheme used by dialogs (`Role::FrameGray*` family).
    Gray,
}

// ---------------------------------------------------------------------------
// ScrollBarOptions — placement + keyboard options for standard_scroll_bar
// ---------------------------------------------------------------------------

/// Options for [`Window::standard_scroll_bar`]: where to place the bar and
/// whether it handles the keyboard.
///
/// The default (both flags false) is a horizontal bar that does not handle the
/// keyboard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollBarOptions {
    /// Place the bar on the right edge (else the bottom edge).
    pub vertical: bool,
    /// The bar opts into post-processing so it handles focused-chain arrow keys
    /// even when it is not the current view.
    pub handle_keyboard: bool,
}

// ---------------------------------------------------------------------------
// Window
// ---------------------------------------------------------------------------

/// A framed, selectable window: a [`Group`] that builds a
/// [`Frame`](crate::frame::Frame) around itself.
///
/// Build with [`Window::new`], then drive it as any other [`View`].
///
/// # Turbo Vision heritage
///
/// Ports `TWindow` (`twindow.cpp`). The base-class container becomes an embedded
/// [`Group`] (deviation D2); the window pushes the frame's data down rather than
/// letting the frame reach up (deviation D3).
pub struct Window {
    /// The embedded container. A `Window` *is* a group: its state, draw, and
    /// event routing are the group's.
    group: Group,
    /// The frame child's id. [`zoom`](Self::zoom) pushes the zoomed flag through
    /// it.
    frame_id: ViewId,
    /// The decoration flags.
    flags: WindowFlags,
    /// The saved bounds for un-zoom, consumed by [`zoom`](Self::zoom).
    zoom_rect: Rect,
    /// The window number.
    number: i16,
    /// The colour scheme. See [`WindowPalette`].
    palette: WindowPalette,
    /// The window title.
    title: Option<String>,
}

impl Window {
    /// Construct a window over `bounds` with an optional `title` and a window
    /// `number`.
    ///
    /// **`number`:** pass a positive value (`1`–`9`) to make the window reachable
    /// via the Alt-*N* keyboard shortcut; [`Program`](crate::Program) broadcasts
    /// [`Command::SELECT_WINDOW_NUM`](crate::Command::SELECT_WINDOW_NUM) and the
    /// desktop finds the matching window by its number. Pass `0` (the
    /// `wnNoNumber` sentinel from Turbo Vision) for an unnumbered window that is
    /// never a select-by-number target — this is the common case for document
    /// windows that are cycled with `NEXT`/`PREV` instead of by number.
    ///
    /// Defaults set at construction: all four decoration flags enabled
    /// (move/grow/close/zoom); the un-zoom rect set to the current bounds; the
    /// blue colour scheme; a drop shadow; selectable + top-of-select-group; and a
    /// relative grow-all mode (children resize proportionally with the window).
    ///
    /// **Frame data is pushed down at construction.** We build the [`Frame`]
    /// **concretely** so we can call its owner-data-down setters
    /// ([`set_title`](Frame::set_title)/[`set_flags`](Frame::set_flags)/
    /// [`set_number`](Frame::set_number)) before boxing + inserting.
    ///
    /// **A frame is mandatory:** `frame_id` is non-optional. A frameless variant
    /// has no consumer here, and supporting it would force an `Option<ViewId>`
    /// ripple for a path no caller exercises, so we always build the frame.
    pub fn new(bounds: Rect, title: Option<String>, number: i16) -> Self {
        let mut group = Group::new(bounds);

        // All four decoration flags enabled by default.
        let flags = WindowFlags {
            r#move: true,
            grow: true,
            close: true,
            zoom: true,
        };
        // Drop shadow; selectable and top-of-select-group.
        let st = group.state_mut();
        st.state.shadow = true;
        st.options.selectable = true;
        st.options.top_select = true;
        // Relative grow-all: children resize proportionally with the window.
        st.grow_mode = GrowMode {
            rel: true,
            ..GrowMode::grow_all()
        };

        // Un-zoom rect = the current bounds.
        let zoom_rect = group.state().get_bounds();
        let extent = group.state().get_extent();

        // We build the Frame directly and push owner data into it at construction;
        // the downcast seam reaches it post-insert for `set_zoomed`. A
        // custom-frame injection hook has no consumer yet; reintroduce it when a
        // subtype needs a non-default frame.
        let mut frame = Frame::new(extent);
        frame.set_title(title.clone());
        frame.set_flags(flags);
        frame.set_number(number_to_option(number));
        let frame_id = group.insert(Box::new(frame));

        Window {
            group,
            frame_id,
            flags,
            zoom_rect,
            number,
            palette: WindowPalette::Blue,
            title,
        }
    }

    // -- accessors -----------------------------------------------------------

    /// The [`ViewId`] of the [`Frame`](crate::frame::Frame) child that draws the
    /// window border, title, and icons.
    ///
    /// Use this id to reach the frame via `child_mut` + downcast when you need to
    /// call a frame-specific method (e.g. to read [`Frame::zoomed`]). The window
    /// itself never holds a direct `&mut Frame` reference across calls; instead it
    /// always resolves the id on demand — the push-down pattern that avoids upward
    /// parent pointers.
    pub fn frame_id(&self) -> ViewId {
        self.frame_id
    }

    /// The current decoration flags (which of move / grow / close / zoom are enabled).
    ///
    /// Read these to check which operations the window permits; for example,
    /// `flags().zoom` is true when the zoom icon is shown and the zoom command is
    /// accepted. To change the flags after construction use `set_flags` (crate-internal).
    pub fn flags(&self) -> WindowFlags {
        self.flags
    }

    /// The pre-zoom bounds, saved when the window zooms in to fill its owner,
    /// used to restore the original size when it un-zooms.
    ///
    /// When the user zooms the window to fill the owner, the window's current
    /// bounds are stored here first; a second zoom (un-zoom) restores them from
    /// this value. At construction `zoom_rect` equals the initial bounds. This
    /// getter is primarily useful for testing and serialisation; normal application
    /// code does not need to read it directly.
    pub fn zoom_rect(&self) -> Rect {
        self.zoom_rect
    }

    // NOTE: the window number is exposed via the `View::number()` trait override
    // below (returning `Option<i16>` — `None` for an unnumbered window), so Alt-N
    // can query any `&dyn View` for its number. No inherent getter.

    /// The colour scheme used to draw this window's frame.
    ///
    /// Plain windows use [`WindowPalette::Blue`] (the default); dialogs use
    /// [`WindowPalette::Gray`] (set automatically by [`Dialog`](crate::dialog::Dialog)).
    /// Inspect this to branch on the active scheme; to change it after construction
    /// use `set_palette` (crate-internal).
    pub fn palette(&self) -> WindowPalette {
        self.palette
    }

    /// The window title displayed in the frame's title bar, or `None` for an
    /// untitled window.
    ///
    /// Pass the title at construction via [`Window::new`]. To rename the window
    /// after construction (e.g. after a save-as), use `set_title` (crate-internal),
    /// which updates both this field and the frame child. The frame truncates the
    /// title to the available title-bar width at draw time; no explicit length limit
    /// is needed here.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Update the window title in both the `Window` record and the [`Frame`]
    /// child. Called from an editor window's event handler on the
    /// update-title broadcast (an editor fires it after a save-as rename). Because
    /// the title is stored here and drawn by the frame, this recomputes the title
    /// and re-pushes it to the frame. Pattern mirrors [`set_flags`]: find the
    /// frame child, downcast, call the frame setter.
    pub(crate) fn set_title(&mut self, title: Option<String>) {
        self.title = title.clone();
        if let Some(frame) = self
            .group
            .child_mut(self.frame_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<Frame>())
        {
            frame.set_title(title);
        }
    }

    // -- setters and builders: public post-construction configuration surface --

    /// Override the decoration flags after construction. Re-pushes to the frame
    /// child: construction pushes the flags once, so a later change must re-push
    /// or the frame would still draw the original zoom/grow icons. Resolves the
    /// frame child then downcasts then calls [`Frame::set_flags`], the same seam
    /// `zoom` uses.
    pub fn set_flags(&mut self, flags: WindowFlags) {
        self.flags = flags;
        if let Some(frame) = self
            .group
            .child_mut(self.frame_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<Frame>())
        {
            frame.set_flags(flags);
        }
    }

    /// Override the colour scheme after construction. Re-pushes to the frame
    /// child (the [`set_flags`](Self::set_flags) pattern) so the frame renders
    /// the matching role family: `Blue` → `Role::Frame*`, `Cyan` →
    /// `Role::FrameCyan*`, `Gray` → `Role::FrameGray*`.
    pub fn set_palette(&mut self, palette: WindowPalette) {
        self.palette = palette;
        if let Some(frame) = self
            .group
            .child_mut(self.frame_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<Frame>())
        {
            frame.set_palette(palette);
        }
    }

    /// Override the grow mode after construction (a dialog uses an empty grow
    /// mode — it does not track its owner's resize).
    pub fn set_grow_mode(&mut self, grow_mode: GrowMode) {
        self.group.state_mut().grow_mode = grow_mode;
    }

    /// Override the drag mode after construction (the screen-edge limits the window
    /// honors while being dragged). Mirrors [`set_grow_mode`](Self::set_grow_mode):
    /// a plain write to the embedded group's [`ViewState::drag_mode`].
    pub fn set_drag_mode(&mut self, drag_mode: DragMode) {
        self.group.state_mut().drag_mode = drag_mode;
    }

    /// Builder form of [`set_flags`](Self::set_flags).
    pub fn with_flags(mut self, flags: WindowFlags) -> Self {
        self.set_flags(flags);
        self
    }

    /// Builder form of [`set_palette`](Self::set_palette).
    pub fn with_palette(mut self, palette: WindowPalette) -> Self {
        self.set_palette(palette);
        self
    }

    /// Builder form of [`set_grow_mode`](Self::set_grow_mode).
    pub fn with_grow_mode(mut self, grow_mode: GrowMode) -> Self {
        self.set_grow_mode(grow_mode);
        self
    }

    /// Builder form of [`set_drag_mode`](Self::set_drag_mode).
    pub fn with_drag_mode(mut self, drag_mode: DragMode) -> Self {
        self.set_drag_mode(drag_mode);
        self
    }

    /// Insert a child view into the embedded group.
    ///
    /// Used by the history window (which inserts the history viewer after the
    /// scroll bars), by [`Dialog`](crate::dialog::Dialog), and by the message box.
    ///
    /// Exposed publicly so that example/application code can build custom windows
    /// with child views.
    pub fn insert_child(&mut self, view: Box<dyn View>) -> ViewId {
        self.group.insert(view)
    }

    /// Reach a direct child of the embedded group by id.
    ///
    /// Used by the history window to run the viewer's post-insert setup and to
    /// read its selection via `as_any_mut` + downcast. Mirrors `Group::child_mut`
    /// without exposing the group itself.
    pub fn child_mut(&mut self, id: ViewId) -> Option<&mut dyn View> {
        self.group.child_mut(id)
    }

    /// Gather the embedded group's whole record as one ordered
    /// [`FieldValue::List`](crate::data::FieldValue). Forwards to
    /// [`Group::gather_list`](crate::view::Group::gather_list).
    pub fn gather_list(&self) -> crate::data::FieldValue {
        self.group.gather_list()
    }

    /// Scatter an ordered [`FieldValue::List`](crate::data::FieldValue) record
    /// back into the embedded group. Forwards to
    /// [`Group::scatter_list`](crate::view::Group::scatter_list).
    pub fn scatter_list(&mut self, record: &crate::data::FieldValue, ctx: &mut Context) {
        self.group.scatter_list(record, ctx);
    }

    /// The first non-frame child that downcasts to a [`Splitter`](crate::widgets::Splitter),
    /// or `None`. Used by the auto-brokering `draw` to read divider abutments.
    fn interior_splitter_mut(&mut self) -> Option<&mut crate::widgets::Splitter> {
        let frame_id = self.frame_id;
        let ids: Vec<ViewId> = self
            .group
            .child_ids_in_order()
            .into_iter()
            .filter(|&id| id != frame_id)
            .collect();
        // Probe-then-reborrow: NLL can't prove the early-return borrow is dropped
        // on the fall-through path, so we test with one borrow and return with a
        // fresh one.
        for id in ids {
            let is_splitter = self
                .group
                .child_mut(id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<crate::widgets::Splitter>())
                .is_some();
            if is_splitter {
                return self
                    .group
                    .child_mut(id)
                    .and_then(|v| v.as_any_mut())
                    .and_then(|a| a.downcast_mut::<crate::widgets::Splitter>());
            }
        }
        None
    }

    /// The frame child, downcast to [`Frame`] — the same seam `zoom`/`set_flags` use.
    fn frame_mut(&mut self) -> Option<&mut Frame> {
        self.group
            .child_mut(self.frame_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<Frame>())
    }

    /// Make `id` the current (focused) child of the embedded group, so focused
    /// (keyboard) events route to it. Test-only: in production the insert → show →
    /// reset-current cascade is realized by `exec_view`'s post-insert
    /// `reset_current` plus the pump's `settle_currency` pass, so no per-site
    /// stand-in is needed.
    #[cfg(test)]
    pub(crate) fn select_child(&mut self, id: ViewId, ctx: &mut Context) {
        self.group
            .set_current(Some(id), crate::view::SelectMode::Normal, ctx);
    }

    // -- standard scroll bar -------------------------------------------------

    /// Insert a standard scroll bar on the right or bottom edge and return its
    /// [`ViewId`].
    ///
    /// Pass [`ScrollBarOptions`] to choose vertical (right edge) or horizontal
    /// (bottom edge) placement, and whether the bar handles keyboard arrow keys
    /// when it is not the focused view (`handle_keyboard`).
    ///
    /// The bar is sized to fit exactly inside the frame without covering the
    /// frame corners:
    /// - **Vertical** (right edge): occupies `(width−1, 1) .. (width, height−1)` —
    ///   one cell wide, inset one row from each frame corner.
    /// - **Horizontal** (bottom edge): occupies `(2, height−1) .. (width−2, height)` —
    ///   one cell tall, inset two columns from each frame corner.
    ///
    /// Call this method after construction and before the window is shown. The
    /// returned [`ViewId`] can be passed to a scroller or list viewer as its
    /// linked scroll bar.
    ///
    /// For `handle_keyboard`, post-processing is enabled on the [`ScrollBar`]
    /// **before** boxing + inserting (the `insert` call consumes the box, so
    /// the flag must be set first).
    pub fn standard_scroll_bar(&mut self, opts: ScrollBarOptions) -> ViewId {
        let ext = self.group.state().get_extent();
        let r = if opts.vertical {
            Rect::from_points(
                Point::new(ext.b.x - 1, ext.a.y + 1),
                Point::new(ext.b.x, ext.b.y - 1),
            )
        } else {
            Rect::from_points(
                Point::new(ext.a.x + 2, ext.b.y - 1),
                Point::new(ext.b.x - 2, ext.b.y),
            )
        };
        let sb = ScrollBar::new(r);
        let sb = if opts.handle_keyboard {
            sb.with_keyboard()
        } else {
            sb
        };
        self.group.insert(Box::new(sb))
    }

    // -- zoom ----------------------------------------------------------------

    /// Toggle between the restored bounds and filling the owner. If the window is
    /// not already at its maximum size, save the current bounds and grow to fill;
    /// otherwise restore the saved bounds.
    ///
    /// The maximum size (the owner's size) is reached via the owner-extent-down
    /// channel ([`Context::owner_size`](crate::view::Context::owner_size)) instead
    /// of an up-pointer. The window's own [`size_limits`](View::size_limits)
    /// override (max = owner size, min = 16×6) is used.
    fn zoom(&mut self, ctx: &mut Context) {
        let owner_size = ctx.owner_size();
        let (_min, max) = View::size_limits(self, owner_size);
        let size = self.group.state().size;
        if size != max {
            self.zoom_rect = self.group.state().get_bounds();
            self.locate(Rect::new(0, 0, max.x, max.y), owner_size);
        } else {
            let zr = self.zoom_rect;
            self.locate(zr, owner_size);
        }
        // The frame needs to know whether the window is maximized to pick the
        // zoom vs unzoom icon, but it cannot read the owner's size itself, so push
        // the bool down through the downcast seam. Re-pushed in `locate` on every
        // bounds change, so this stays current.
        let zoomed = self.group.state().size == max;
        if let Some(frame) = self
            .group
            .child_mut(self.frame_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<Frame>())
        {
            frame.set_zoomed(zoomed);
        }
    }

    /// Clamp `bounds`'s size to [`size_limits`](View::size_limits) then
    /// `change_bounds` iff it differs. `owner_size` feeds the (overridden)
    /// `size_limits`. No shadow/under-rect repaint tail is needed — whole-tree
    /// redraw + diff makes it redundant.
    fn locate(&mut self, mut bounds: Rect, owner_size: Point) {
        let (min, max) = View::size_limits(self, owner_size);
        bounds.b.x = bounds.a.x + range(bounds.b.x - bounds.a.x, min.x, max.x);
        bounds.b.y = bounds.a.y + range(bounds.b.y - bounds.a.y, min.y, max.y);
        if bounds != self.group.state().get_bounds() {
            // Resize children by the delta.
            self.group.change_bounds(bounds);
            // Re-push the zoomed flag so the frame's zoom icon reflects the new
            // size (pushed through the downcast seam since the frame can't read the
            // owner's size). Re-pushed here on every bounds change.
            let zoomed = self.group.state().size == max;
            if let Some(frame) = self
                .group
                .child_mut(self.frame_id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<Frame>())
            {
                frame.set_zoomed(zoomed);
            }
        }
    }

    // -- drag ----------------------------------------------------------------

    /// Start a drag, reached from [`handle_event`](Self::handle_event) on a
    /// surviving title-bar / corner / middle-button `MouseDown`.
    ///
    /// Sets the `Dragging` state flag **on** directly (it has `&mut self` + `ctx`,
    /// so `Group::set_state` propagates `Dragging` to children incl. the frame,
    /// flipping it to the single-line dragging border), then pushes a deferred
    /// [`DragCapture`]. The capture is pushed deferred, so it sees the *next* event
    /// (the first `MouseMove`), never this `MouseDown` (the `pending_captures`
    /// contract).
    ///
    /// `mouse_local` is the `MouseDown` position in **window-local** coords; adding
    /// the window's own `origin` gives the absolute mouse-down used to compute the
    /// constant grab anchor (the coordinate-frame assumption on [`DragCapture`]).
    /// The owner extent and size limits are read **here** (group-routed dispatch,
    /// so `ctx.owner_size()` is valid), never at drag time.
    fn start_drag(&mut self, id: ViewId, kind: DragKind, mouse_local: Point, ctx: &mut Context) {
        // Set the Dragging flag directly (Group::set_state propagates it to
        // children incl. the frame).
        View::set_state(self, StateFlag::Dragging, true, ctx);

        let origin = self.group.state().origin;
        let size = self.group.state().size;
        let mouse_abs = mouse_local + origin; // window-local -> absolute
        // The owner extent and the window's size limits, via the owner-extent-down
        // channel. owner_size is valid HERE (group-routed dispatch); the capture
        // must NOT read it at drag time.
        let owner_size = ctx.owner_size();
        let limits = Rect::new(0, 0, owner_size.x, owner_size.y); // owner extent
        let (min, max) = View::size_limits(self, owner_size);
        // The window's drag mode carries only the limit bits that feed move_grow
        // (the move/grow choice is already encoded in `kind`); the default limits
        // the low-y edge.
        let mode = self.group.state().drag_mode;
        let anchor = match kind {
            DragKind::Move => origin - mouse_abs,
            DragKind::Grow => size - mouse_abs,
            DragKind::GrowLeft => Point::new(origin.x, origin.y + size.y) - mouse_abs,
        };
        let init_bounds = self.group.state().get_bounds();
        ctx.push_capture(Box::new(DragCapture {
            window_id: id,
            kind,
            init_bounds,
            anchor,
            limits,
            min,
            max,
            mode,
        }));
    }

    /// Begin a resize session on the body splitter (if any) and return its
    /// divider targets. The body is the first non-frame child that is a `Splitter`.
    fn begin_body_splitter_session(&mut self) -> Vec<ResizeTarget> {
        self.interior_splitter_mut()
            .map(|sp| {
                sp.begin_resize_session()
                    .into_iter()
                    .map(|(splitter, index, orientation)| ResizeTarget::Divider {
                        splitter,
                        index,
                        orientation,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// True if the body splitter has ≥1 movable divider.
    fn body_has_movable_divider(&mut self) -> bool {
        self.interior_splitter_mut()
            .map(|sp| sp.has_movable_divider())
            .unwrap_or(false)
    }
}

/// Clamp `val` into `[min, max]`, pinning `min` to `max` if inverted.
/// Reimplemented locally (it is two lines) to keep the `locate` seam contained —
/// the `view.rs` `range` is private there.
fn range(val: i32, min: i32, max: i32) -> i32 {
    let min = if min > max { max } else { min };
    val.clamp(min, max)
}

/// Map a window number to the frame's `Option<u8>` contract: `0` (no number) →
/// `None`; `0 < n` → `Some(value)`, pushed down to the frame, whose own `n < 10`
/// draw guard then suppresses any digit `>= 10`. The `Option<u8>` carrier clamps
/// `n > 255` to `255` via `unwrap_or(u8::MAX)`, but that branch is unreachable in
/// practice (numbers are `1..=9`). Negative numbers are out of contract; they map
/// to `None` (treated as "no number").
fn number_to_option(number: i16) -> Option<u8> {
    if number <= 0 {
        None
    } else {
        Some(u8::try_from(number).unwrap_or(u8::MAX))
    }
}

// ---------------------------------------------------------------------------
// Drag — the capture handler that runs an interactive window drag
// ---------------------------------------------------------------------------

/// Which drag form is running (the mouse branch; the keyboard resize sub-mode
/// runs through [`KeyboardResizeCapture`]). Selects how each `MouseMove` maps to
/// new bounds (see [`DragCapture::compute_bounds`]).
enum DragKind {
    /// Translate the whole window (title-bar / middle-button move).
    Move,
    /// Drag the bottom-right corner (origin fixed, size follows).
    Grow,
    /// Drag the bottom-left corner (top-right fixed).
    GrowLeft,
}

/// The [`CaptureHandler`] that runs an interactive window drag. The *frame*
/// cannot start the drag (it has no pointer to the window it would move); the
/// [`Window`] starts it (it knows its own id and its owner's size) via
/// [`Window::start_drag`], which pushes this handler.
///
/// **Coordinate-frame assumption (mirrors [`ModalFrame`](crate::app::ModalFrame)).**
/// The capture runs at the capture-stack level, *before* any group routing, so it
/// sees mouse events in **absolute screen coordinates**. The root `Group` covers
/// the whole screen at `(0,0)` and the desktop is its child at `(0,0)`, so
/// **absolute == root-local == desktop-local**, and a window's `origin` (relative
/// to its owner) is in that same frame — the drag math assumes this.
struct DragCapture {
    window_id: ViewId,
    kind: DragKind,
    /// Window bounds at drag start (the fixed corner for Grow/GrowLeft).
    init_bounds: Rect,
    /// The constant grab offset (see [`compute_bounds`](Self::compute_bounds));
    /// per-kind meaning documented in [`Window::start_drag`].
    anchor: Point,
    /// The owner's extent, captured at push time from `ctx.owner_size()`.
    limits: Rect,
    /// Minimum window size, captured at push time.
    min: Point,
    /// Maximum window size, captured at push time.
    max: Point,
    /// The drag mode — only the limit bits matter to [`move_grow`] (the window's
    /// default limits the low-y edge).
    mode: DragMode,
}

impl DragCapture {
    /// Map the current `MouseMove`'s absolute position to the window's new bounds,
    /// for each of the three drag forms. The anchor is the **constant** grab
    /// offset captured at push time (see [`Window::start_drag`]).
    fn compute_bounds(&self, mouse_abs: Point) -> Rect {
        match self.kind {
            // New origin = mouse + (initial origin - mouse-down).
            DragKind::Move => {
                let sz = self.init_bounds.b - self.init_bounds.a;
                let new_origin = mouse_abs + self.anchor;
                move_grow(new_origin, sz, self.limits, self.min, self.max, self.mode)
            }
            // New size = mouse + (initial size - mouse-down).
            DragKind::Grow => {
                let o = self.init_bounds.a;
                let new_size = mouse_abs + self.anchor;
                move_grow(o, new_size, self.limits, self.min, self.max, self.mode)
            }
            // Grow-left: bespoke pre-clamp of the moving bottom-left corner, then
            // move_grow. The top-right (`b`) is the fixed anchor.
            DragKind::GrowLeft => {
                let corner = mouse_abs + self.anchor; // = mouse + (botLeft - mouseDown)
                let b = self.init_bounds.b; // fixed top-right anchor
                let ax = corner.x.max(b.x - self.max.x).min(b.x - self.min.x);
                let a = Point::new(ax, self.init_bounds.a.y); // a.y stays initial
                let by = corner.y; // bottom edge follows mouse
                move_grow(
                    a,
                    Point::new(b.x - a.x, by - a.y),
                    self.limits,
                    self.min,
                    self.max,
                    self.mode,
                )
            }
        }
    }
}

impl CaptureHandler for DragCapture {
    fn handle(&mut self, ev: &mut Event, ctx: &mut Context) -> CaptureFlow {
        match ev {
            Event::MouseMove(m) => {
                // `m.position` is ABSOLUTE here (capture runs before group routing).
                let r = self.compute_bounds(m.position);
                ctx.request_bounds(self.window_id, r);
                CaptureFlow::Consumed
            }
            Event::MouseUp(_) => {
                // The drag ends on mouse-up; clear the Dragging flag (via a
                // request, since a capture holds no &mut view) and pop ourselves.
                ctx.request_set_state(self.window_id, StateFlag::Dragging, false);
                CaptureFlow::ConsumedPop
            }
            // The drag is modal: swallow everything that is not a mouse-move/up
            // (auto-repeat, keys, commands, broadcasts) without moving the window.
            _ => CaptureFlow::Consumed,
        }
    }

    fn view(&self) -> Option<ViewId> {
        Some(self.window_id)
    }
}

// ---------------------------------------------------------------------------
// KeyboardResizeCapture — runs an interactive keyboard move/resize
// ---------------------------------------------------------------------------

/// The capture handler that runs an interactive keyboard move/resize, reached
/// from [`Window::handle_event`] on a resize command. Mirrors [`DragCapture`] for
/// the mouse side.
///
/// State captured at push time:
/// - `save_bounds` — bounds at entry, restored on Esc.
/// - `limits`     — owner's extent at entry.
/// - `min`, `max` — the size limits at entry.
/// - `mode`       — the drag mode (limit bits + the move/grow choice).
/// - `origin`, `size` — current position/size, updated by each arrow key.
///
/// Each arrow key produces a delta; if the mode allows moving, the delta shifts
/// the origin, and if it allows growing, the delta changes the size. Plain arrows
/// map to a 1-cell delta; Ctrl variants to `(±8, 0)` / `(0, ±4)`.
/// A keyboard-resize target: the window itself, or one divider of a splitter
/// in the window body. Dividers are addressed only by id (the capture never
/// touches the `Splitter` inline — it brokers via `DividerOp`).
enum ResizeTarget {
    Window,
    Divider {
        splitter: ViewId,
        index: usize,
        orientation: Orientation,
    },
}

struct KeyboardResizeCapture {
    window_id: ViewId,
    /// Bounds at entry — restored by Esc.
    save_bounds: Rect,
    /// The owner's extent, captured at push time.
    limits: Rect,
    /// Minimum window size.
    min: Point,
    /// Maximum window size.
    max: Point,
    /// The drag mode — limit bits + the move/grow choice.
    mode: DragMode,
    /// Current window origin (top-left), updated by each arrow.
    origin: Point,
    /// Current window size, updated by each arrow.
    size: Point,
    /// Cycle targets: `targets[current]` is live. Tab/Shift+Tab move `current`.
    targets: Vec<ResizeTarget>,
    current: usize,
}

impl KeyboardResizeCapture {
    /// Apply an arrow-key delta to the window target. Faithful to `TView::change`:
    /// plain arrows MOVE (when `drag_move`), Shift+arrows GROW (when `drag_grow`).
    /// `Ctrl` only scales the step (handled by the caller); it does not pick
    /// move-vs-grow.
    fn apply_delta(&mut self, delta: Point, shift: bool, ctx: &mut Context) {
        if self.mode.drag_move && !shift {
            self.origin += delta;
        } else if self.mode.drag_grow && shift {
            self.size += delta;
        }
        let r = move_grow(
            self.origin,
            self.size,
            self.limits,
            self.min,
            self.max,
            self.mode,
        );
        // Sync tracked origin/size to the clamped result so repeated presses stay
        // within limits.
        self.origin = r.a;
        self.size = r.b - r.a;
        ctx.request_bounds(self.window_id, r);
    }

    fn current_is_window(&self) -> bool {
        matches!(self.targets.get(self.current), Some(ResizeTarget::Window))
    }

    /// Turn the current target's highlight on/off.
    fn set_highlight(&self, on: bool, ctx: &mut Context) {
        match self.targets.get(self.current) {
            Some(ResizeTarget::Window) => {
                ctx.request_set_state(self.window_id, StateFlag::Dragging, on);
            }
            Some(ResizeTarget::Divider {
                splitter, index, ..
            }) => {
                ctx.splitter_divider(*splitter, DividerOp::SetActive(on.then_some(*index)));
            }
            None => {}
        }
    }

    /// Tab/Shift+Tab: hand the highlight from the old target to the next.
    fn cycle(&mut self, forward: bool, ctx: &mut Context) {
        let n = self.targets.len();
        if n < 2 {
            return;
        }
        self.set_highlight(false, ctx);
        self.current = if forward {
            (self.current + 1) % n
        } else {
            (self.current + n - 1) % n
        };
        self.set_highlight(true, ctx);
    }

    /// A ±1 arrow nudge for a DIVIDER target, along the divider's axis only.
    /// Cross-axis arrows are ignored. The window target is handled inline in
    /// `handle` (see the arrow-key arm there).
    fn arrow(&mut self, key: Key, ctx: &mut Context) {
        if let Some(ResizeTarget::Divider {
            splitter,
            index,
            orientation,
        }) = self.targets.get(self.current)
        {
            let delta = match (orientation, key) {
                (Orientation::Cols, Key::Left) => -1,
                (Orientation::Cols, Key::Right) => 1,
                (Orientation::Rows, Key::Up) => -1,
                (Orientation::Rows, Key::Down) => 1,
                _ => 0,
            };
            if delta != 0 {
                ctx.splitter_divider(
                    *splitter,
                    DividerOp::Nudge {
                        index: *index,
                        delta,
                    },
                );
            }
        }
    }

    /// Enter (commit) / Esc (cancel): clear every highlight and end every session.
    fn finish(&self, commit: bool, ctx: &mut Context) {
        let mut seen: Vec<ViewId> = Vec::new();
        let mut window_in_targets = false;
        for t in &self.targets {
            match t {
                ResizeTarget::Window => window_in_targets = true,
                ResizeTarget::Divider { splitter, .. } => {
                    if !seen.contains(splitter) {
                        seen.push(*splitter);
                        ctx.splitter_divider(*splitter, DividerOp::EndSession { commit });
                    }
                }
            }
        }
        if window_in_targets {
            if !commit {
                ctx.request_bounds(self.window_id, self.save_bounds);
            }
            ctx.request_set_state(self.window_id, StateFlag::Dragging, false);
        }
    }
}

impl CaptureHandler for KeyboardResizeCapture {
    /// Each key adjusts origin/size by its delta then clamps via [`move_grow`];
    /// Enter accepts, Esc restores.
    fn handle(&mut self, ev: &mut Event, ctx: &mut Context) -> CaptureFlow {
        let Event::KeyDown(k) = ev else {
            // Swallow everything that is not a key (mouse events, commands,
            // broadcasts) while the keyboard resize is modal.
            return CaptureFlow::Consumed;
        };
        match k.key {
            Key::Tab => {
                // Tab cycles the resize target forward; Shift+Tab backward.
                self.cycle(!k.modifiers.shift, ctx);
                CaptureFlow::Consumed
            }
            Key::Left | Key::Right | Key::Up | Key::Down => {
                if self.current_is_window() {
                    // Ctrl scales the step (±8 horiz / ±4 vert); Shift picks grow vs move.
                    let (mh, mv) = if k.modifiers.ctrl { (8, 4) } else { (1, 1) };
                    let d = match k.key {
                        Key::Left => Point::new(-mh, 0),
                        Key::Right => Point::new(mh, 0),
                        Key::Up => Point::new(0, -mv),
                        Key::Down => Point::new(0, mv),
                        _ => Point::new(0, 0),
                    };
                    self.apply_delta(d, k.modifiers.shift, ctx);
                } else {
                    // Divider target: ±1 nudge along the divider's axis (Shift/Ctrl ignored).
                    self.arrow(k.key, ctx);
                }
                CaptureFlow::Consumed
            }
            Key::Home if self.current_is_window() => {
                // Home: snap the left edge to the owner's left.
                // p.x = limits.a.x
                self.origin.x = self.limits.a.x;
                let r = move_grow(
                    self.origin,
                    self.size,
                    self.limits,
                    self.min,
                    self.max,
                    self.mode,
                );
                self.origin = r.a;
                self.size = r.b - r.a;
                ctx.request_bounds(self.window_id, r);
                CaptureFlow::Consumed
            }
            Key::End if self.current_is_window() => {
                // End: snap the right edge to the owner's right.
                // p.x = limits.b.x - s.x
                self.origin.x = self.limits.b.x - self.size.x;
                let r = move_grow(
                    self.origin,
                    self.size,
                    self.limits,
                    self.min,
                    self.max,
                    self.mode,
                );
                self.origin = r.a;
                self.size = r.b - r.a;
                ctx.request_bounds(self.window_id, r);
                CaptureFlow::Consumed
            }
            Key::PageUp if self.current_is_window() => {
                // PageUp: snap the top edge to the owner's top.
                // p.y = limits.a.y
                self.origin.y = self.limits.a.y;
                let r = move_grow(
                    self.origin,
                    self.size,
                    self.limits,
                    self.min,
                    self.max,
                    self.mode,
                );
                self.origin = r.a;
                self.size = r.b - r.a;
                ctx.request_bounds(self.window_id, r);
                CaptureFlow::Consumed
            }
            Key::PageDown if self.current_is_window() => {
                // PageDown: snap the bottom edge to the owner's bottom.
                // p.y = limits.b.y - s.y
                self.origin.y = self.limits.b.y - self.size.y;
                let r = move_grow(
                    self.origin,
                    self.size,
                    self.limits,
                    self.min,
                    self.max,
                    self.mode,
                );
                self.origin = r.a;
                self.size = r.b - r.a;
                ctx.request_bounds(self.window_id, r);
                CaptureFlow::Consumed
            }
            Key::Enter => {
                // Accept — clear every highlight + end every session, then pop.
                self.finish(true, ctx);
                CaptureFlow::ConsumedPop
            }
            Key::Esc => {
                // Cancel — restore window bounds + divider weights, then pop.
                self.finish(false, ctx);
                CaptureFlow::ConsumedPop
            }
            _ => {
                // Any other key passes through (the resize does not break on
                // unknown keys).
                CaptureFlow::Pass
            }
        }
    }

    fn view(&self) -> Option<ViewId> {
        Some(self.window_id)
    }
}

/// Clamp size to `[min, max]` and origin to the limits, honoring the limit-edge
/// mode bits, and return the resulting bounds.
///
/// We return the rect instead of calling `locate` (the capture has no `&mut`
/// view; the caller applies it via `change_bounds` — equivalent, since
/// `move_grow` already clamps to the same size limits `locate` would).
///
/// This uses explicit `min(max(..))` **not** [`i32::clamp`]: when `lo > hi`
/// (window larger than the limit), `min(max(v, lo), hi)` yields `hi`, whereas
/// `clamp` PANICS on `lo > hi`. So we replicate min/max exactly.
fn move_grow(
    mut p: Point,
    mut s: Point,
    limits: Rect,
    min: Point,
    max: Point,
    mode: DragMode,
) -> Rect {
    s.x = s.x.max(min.x).min(max.x);
    s.y = s.y.max(min.y).min(max.y);
    p.x = p.x.max(limits.a.x - s.x + 1).min(limits.b.x - 1);
    p.y = p.y.max(limits.a.y - s.y + 1).min(limits.b.y - 1);
    if mode.limit_lo_x {
        p.x = p.x.max(limits.a.x);
    }
    if mode.limit_lo_y {
        p.y = p.y.max(limits.a.y);
    }
    if mode.limit_hi_x {
        p.x = p.x.min(limits.b.x - s.x);
    }
    if mode.limit_hi_y {
        p.y = p.y.min(limits.b.y - s.y);
    }
    Rect::from_points(p, p + s)
}

#[crate::delegate(to = group, skip(
    apply_scroll_sync,
    as_any_mut,
    calc_bounds,
    grabs_focus_on_click,
    select_window_num,
    set_value,
    value,
))]
impl View for Window {
    /// Always delegates the draw to the group; first, if this window hosts a
    /// [`Splitter`](crate::widgets::Splitter) body, it reads that splitter's
    /// divider abutment marks (parent→child) and pushes them to the frame child
    /// (owner-data-down) so the frame composes connected tees. The marks are empty
    /// unless the splitter opted in via
    /// [`Splitter::joined`](crate::widgets::Splitter::joined), so a plain window —
    /// or one whose splitter is not joined — is byte-for-byte unchanged (an empty
    /// mark set reverts the frame to a plain border). Both child borrows are
    /// sequential (the marks `Vec` is owned between them, so only one `&mut` child
    /// is live at a time).
    fn draw(&mut self, ctx: &mut DrawCtx) {
        let fb = self.group.state().get_extent();
        let marks = self
            .interior_splitter_mut()
            .map(|s| s.frame_junction_marks(fb));
        if let Some(marks) = marks
            && let Some(frame) = self.frame_mut()
        {
            frame.set_junction_marks(marks);
        }
        self.group.draw(ctx);
    }

    /// Delegate to the group first, then handle the window's own commands and the
    /// focus-cycling keys:
    ///
    /// * **zoom command** (if zoom is allowed) → [`zoom`](Self::zoom) + consume.
    ///   No "is this the right window?" target guard is needed here: it is provably
    ///   vacuous in this architecture. The frame emits the zoom/close commands only
    ///   while it is active, so the target is always the *active* window; these are
    ///   *focused* command events, which the desktop routes solely to its current
    ///   child = the active window; and the internal queue drains fully before the
    ///   next event poll, so the active window cannot change between emit and
    ///   dispatch. A zoom/close therefore always reaches exactly the window it
    ///   targets. **Trip-wire:** revisit only if a future emitter targets a
    ///   *non-active* window via a command.
    /// * **close command** (if close is allowed) → `request_close` (the loop
    ///   drains it into `remove_descendant`), or post a cancel command if modal.
    ///   Same vacuous target-guard reasoning as zoom. The modal-cancel branch is
    ///   wired here; the broader modal teardown lives in the dialog layer.
    /// * **Tab** → cycle focus forward + consume.
    /// * **Shift+Tab** → cycle focus backward + consume. Shift+Tab is `Key::Tab` +
    ///   the `shift` modifier (there is no separate back-tab key).
    /// * A surviving title-bar / bottom-corner / middle-button `MouseDown` →
    ///   [`start_drag`](Self::start_drag) (which pushes a [`DragCapture`]).
    /// * **resize command** → enable the `Dragging` flag and push a
    ///   [`KeyboardResizeCapture`] that handles arrow keys until Enter (accept) or
    ///   Esc (restore). The resize command is enabled in
    ///   [`set_state`](Self::set_state) when the window is selected and move or grow
    ///   is allowed.
    /// * **select-window-by-number** is **not** handled here. The window number is
    ///   an *integer* argument, so the broadcast-source substrate does not serve
    ///   it; instead `program_handle_event` asks the desktop
    ///   ([`Desktop::select_window_num`](crate::desktop::Desktop)) to select the
    ///   child whose [`number`](View::number) matches
    ///   ([`Group::focus_by_number`](crate::view::Group)).
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        self.group.handle_event(ev, ctx);
        // A consumed event is already `Nothing`, so each branch self-guards.
        if let Event::Command(c) = *ev
            && c == Command::ZOOM
            && self.flags.zoom
        {
            self.zoom(ctx);
            ev.clear();
        }
        // Close command. **No target guard** is needed: it is provably vacuous —
        // the frame emits close only while active, and focused command events are
        // routed to the desktop's current (= active) window, so a close always
        // reaches exactly its target (the same trip-wire the zoom branch documents
        // above; revisit only if a future emitter targets a non-active window via a
        // command). The modal-cancel branch is wired here; the dialog layer owns
        // the broader machinery.
        if let Event::Command(c) = *ev
            && c == Command::CLOSE
            && self.flags.close
        {
            ev.clear(); // consume the close first.
            if self.group.state().state.modal {
                // Modal: re-issue as a cancel (the dialog layer owns modal teardown).
                ctx.post(Command::CANCEL);
            } else if self.valid(Command::CLOSE, ctx) {
                // Closing is allowed: the loop drains the request and runs
                // `remove_descendant` (the close-removal channel).
                if let Some(id) = self.group.state().id() {
                    ctx.request_close(id);
                }
            }
        }
        // Resize command: begin a session on the body splitter (if any) and push
        // a unified KeyboardResizeCapture whose Tab cycles the resize target
        // (window then each divider). Arrow keys move the active target; Enter
        // commits, Esc cancels. Runs whenever the window is movable/growable OR
        // the body splitter has a movable divider (a fixed window can still
        // resize its dividers).
        if let Event::Command(c) = *ev
            && c == Command::RESIZE
            && let Some(id) = self.group.state().id()
        {
            let can_window = self.flags.r#move || self.flags.grow;
            let div_targets = self.begin_body_splitter_session();
            if can_window || !div_targets.is_empty() {
                let mut targets = Vec::new();
                if can_window {
                    targets.push(ResizeTarget::Window);
                }
                targets.extend(div_targets);

                let owner_size = ctx.owner_size();
                let limits = Rect::new(0, 0, owner_size.x, owner_size.y);
                let (min, max) = View::size_limits(self, owner_size);
                let save_bounds = self.group.state().get_bounds();
                let origin = save_bounds.a;
                let size = save_bounds.b - save_bounds.a;
                // Build a DragMode that carries the window's limit bits AND the
                // move/grow bits from the decoration flags.
                let mut mode = self.group.state().drag_mode; // limit bits
                mode.drag_move = self.flags.r#move;
                mode.drag_grow = self.flags.grow;

                // Initial highlight for targets[0].
                match targets.first() {
                    Some(ResizeTarget::Window) => {
                        View::set_state(self, StateFlag::Dragging, true, ctx);
                    }
                    Some(ResizeTarget::Divider {
                        splitter, index, ..
                    }) => {
                        ctx.splitter_divider(*splitter, DividerOp::SetActive(Some(*index)));
                    }
                    None => {}
                }

                ctx.push_capture(Box::new(KeyboardResizeCapture {
                    window_id: id,
                    save_bounds,
                    limits,
                    min,
                    max,
                    mode,
                    origin,
                    size,
                    targets,
                    current: 0,
                }));
                ev.clear();
            }
        }
        if let Event::KeyDown(k) = *ev
            && k.key == Key::Tab
        {
            // The group's hierarchical Tab pass (in `Group::handle_event`, run via
            // the delegation above) already advanced focus within the focused
            // subtree, descending into nested groups/splitters. A live Tab here
            // means the whole window tree was exhausted — wrap to the first (Tab)
            // or last (Shift+Tab) focusable leaf.
            self.group.focus_to_edge(k.modifiers.shift, ctx);
            ev.clear();
        }
        // Drag detection. Runs AFTER group delegation: the desktop delivered this
        // `MouseDown` to the window in window-local coords; the group routed it
        // positionally to the frame, which leaves a title-bar / bottom-corner click
        // UNCONSUMED (a close/zoom-icon click → Nothing, an interior-child click
        // consumed there). So a still-live `MouseDown` here is a drag spot, its
        // position window-local. An inactive window never reaches here on its first
        // click (the desktop's positional auto-select consumes the selecting
        // click), so the drag only ever starts on the active window — no active
        // re-check needed.
        if let Event::MouseDown(m) = *ev {
            let w = self.group.state().size.x;
            let h = self.group.state().size.y;
            let pos = m.position;
            let kind = if m.buttons.middle
                && self.flags.r#move
                && pos.x > 0
                && pos.x < w - 1
                && pos.y > 0
                && pos.y < h - 1
            {
                // Middle-button interior move (mutually exclusive by geometry with
                // the title/corner cases; the branches do not overlap, so order is
                // irrelevant).
                Some(DragKind::Move)
            } else if pos.y == 0 && self.flags.r#move {
                Some(DragKind::Move) // title-bar move
            } else if pos.y >= h - 1 && self.flags.grow && pos.x >= w - 2 {
                Some(DragKind::Grow) // bottom-right grow
            } else if pos.y >= h - 1 && self.flags.grow && pos.x <= 1 {
                Some(DragKind::GrowLeft) // bottom-left grow
            } else {
                None
            };
            if let Some(kind) = kind
                && let Some(id) = self.group.state().id()
            {
                self.start_drag(id, kind, m.position, ctx);
                ev.clear();
            }
        }
    }

    /// The **activation** half of state changes.
    ///
    /// Delegate to `Group::set_state` (flips the flag + propagates to children),
    /// then — iff the `Selected` flag is being set — also set the `Active` flag on
    /// the group. That `Active` propagation flips **every** child (incl. the frame)
    /// active / passive, so the frame does not need to be pushed manually.
    ///
    /// On selection, the window-command set is enabled (and disabled on
    /// deselection):
    ///   - next/prev window: unconditional (handled by the desktop).
    ///   - zoom: if zoom is allowed (handled in
    ///     [`handle_event`](Self::handle_event)).
    ///   - close: if close is allowed (handled in `handle_event`).
    ///   - resize: if move or grow is allowed (handled in `handle_event` →
    ///     [`KeyboardResizeCapture`]).
    fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
        // Compute BEFORE the closure borrows `ctx` (and `self` immutably).
        let has_div = flag == StateFlag::Selected && self.body_has_movable_divider();
        self.group.set_state(flag, enable, ctx);
        if flag == StateFlag::Selected {
            self.group.set_state(StateFlag::Active, enable, ctx);
            // Window commands enabled/disabled together while selected.
            //
            // next/prev are UNCONDITIONAL — they have no flag guard (every
            // selectable window can be cycled), so they do NOT go through the
            // flag-gated `toggle` closure. Their handler is the desktop.
            if enable {
                ctx.enable_command(Command::NEXT);
                ctx.enable_command(Command::PREV);
            } else {
                ctx.disable_command(Command::NEXT);
                ctx.disable_command(Command::PREV);
            }
            // The flag-gated subset: close (if close allowed), zoom (if zoom
            // allowed), resize (if move or grow allowed) — all handled in
            // handle_event.
            let mut toggle = |cmd: Command, cond: bool| {
                if cond {
                    if enable {
                        ctx.enable_command(cmd);
                    } else {
                        ctx.disable_command(cmd);
                    }
                }
            };
            toggle(Command::CLOSE, self.flags.close);
            toggle(Command::ZOOM, self.flags.zoom);
            toggle(
                Command::RESIZE,
                self.flags.r#move || self.flags.grow || has_div,
            );
        }
    }

    /// The window's size limits: the owner-derived maximum with the minimum forced
    /// to 16 columns × 6 rows — the smallest a window can be while still showing
    /// its frame icons legibly.
    ///
    /// This `size_limits` override is intentionally *not* in the `#[delegate]`
    /// skip list, while `calc_bounds` *is* skipped: `calc_bounds` therefore routes
    /// through the *trait default*, which calls **this** `size_limits` override
    /// (giving the 16×6 floor). If `calc_bounds` were delegated to the inner `Group`, it would
    /// use the group's own `size_limits` (min 0×0) and silently bypass the window
    /// minimum on owner-driven resizes.
    fn size_limits(&self, owner_size: Point) -> (Point, Point) {
        let (_min, max) = self.group.size_limits(owner_size);
        (Point::new(16, 6), max)
    }

    // NOTE: `calc_bounds` is in the skip list above — NOT forwarded to the group.
    // The trait default routes through `Window::size_limits` (this override's
    // 16×6 floor) and mutates the group's `ViewState` via `state_mut()`.
    // Forwarding to `self.group.calc_bounds` would use the group's `size_limits`
    // (min 0×0) and silently bypass the window's minimum on an owner-driven
    // resize.

    /// The window number (1–9), or `None` for an unnumbered window.
    ///
    /// A positive number makes the window reachable by Alt+*N* (where *N* is the
    /// digit). The program's event handler broadcasts `cmSelectWindowNum` and the
    /// desktop walks its children calling `View::number()` on each; the first
    /// window whose number matches `N` is focused
    /// ([`Group::focus_by_number`](crate::view::Group)). A window constructed with
    /// `number == 0` (or any non-positive value) returns `None` and is never a
    /// select-by-number target.
    fn number(&self) -> Option<i16> {
        if self.number > 0 {
            Some(self.number)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::event::{KeyEvent, KeyModifiers};
    use crate::screen::Buffer;
    use crate::theme::{Role, Theme};
    use crate::timer::TimerQueue;
    use crate::view::{DrawCtx, ViewState};
    use std::collections::VecDeque;

    // -- test harness --------------------------------------------------------

    fn with_ctx<R>(
        out: &mut VecDeque<Event>,
        timers: &mut TimerQueue,
        f: impl FnOnce(&mut Context) -> R,
    ) -> R {
        let mut deferred: Vec<crate::view::Deferred> = Vec::new();
        let mut ctx = Context::new(out, timers, 0, &mut deferred);
        f(&mut ctx)
    }

    fn tab_event(shift: bool) -> Event {
        Event::KeyDown(KeyEvent::new(
            Key::Tab,
            KeyModifiers {
                shift,
                ..Default::default()
            },
        ))
    }

    /// A minimal selectable probe view (the frame is not selectable, so Tab
    /// cycling needs real selectable children to move between).
    struct Probe {
        st: ViewState,
    }
    impl Probe {
        fn boxed(bounds: Rect) -> Box<dyn View> {
            let mut st = ViewState::new(bounds);
            st.options.selectable = true;
            Box::new(Probe { st })
        }
    }
    impl View for Probe {
        fn state(&self) -> &ViewState {
            &self.st
        }
        fn state_mut(&mut self) -> &mut ViewState {
            &mut self.st
        }
        fn draw(&mut self, _ctx: &mut DrawCtx) {}
    }

    fn window_with_frame() -> Window {
        Window::new(Rect::new(0, 0, 40, 15), Some("Edit".into()), 3)
    }

    // -- 1. ctor -------------------------------------------------------------

    #[test]
    fn new_ports_ctor_defaults() {
        let w = window_with_frame();
        // flags all-true.
        assert_eq!(
            w.flags(),
            WindowFlags {
                r#move: true,
                grow: true,
                close: true,
                zoom: true,
            }
        );
        // zoomRect == bounds.
        assert_eq!(w.zoom_rect(), Rect::new(0, 0, 40, 15));
        // palette == Blue.
        assert_eq!(w.palette(), WindowPalette::Blue);
        // number stored (now via the View::number() trait override).
        assert_eq!(View::number(&w), Some(3));
        // group state: shadow, selectable, top_select.
        let st = w.state();
        assert!(st.state.shadow, "sfShadow set");
        assert!(st.options.selectable, "ofSelectable set");
        assert!(st.options.top_select, "ofTopSelect set");
        // growMode = gfGrowAll | gfGrowRel.
        let gm = st.grow_mode;
        assert!(gm.lo_x && gm.lo_y && gm.hi_x && gm.hi_y, "gfGrowAll");
        assert!(gm.rel, "gfGrowRel");
        assert!(!gm.fixed);
        // the frame was inserted and its id resolves.
        assert!(
            w.group.index_of_pub(w.frame_id()).is_some(),
            "frame child id resolves"
        );
    }

    /// The frame received the pushed-down title / flags / number at construction.
    #[test]
    fn new_pushes_frame_data_down() {
        let mut w = window_with_frame();
        let idx = w.group.index_of_pub(w.frame_id()).unwrap();
        // Render the (active) frame and read its title back off the top row.
        w.group.child_state_mut(idx).state.active = true;
        let theme = Theme::classic_blue();
        let mut buf = Buffer::new(40, 15);
        {
            let bounds = w.state().get_bounds();
            let mut dc = DrawCtx::new(&mut buf, &theme, bounds, bounds.a);
            w.draw(&mut dc);
        }
        // Title "Edit" (4 cols): lw = min(4, w-10=30) = 4; i = (40-4)>>1 = 18.
        let title: String = (18..22)
            .map(|x| buf.get(x, 0).symbol().to_string())
            .collect();
        assert_eq!(title, "Edit", "pushed-down title renders");
        // Number 3 drawn (flags.zoom true → at w-7 = 33).
        assert_eq!(buf.get(33, 0).symbol(), "3", "pushed-down number renders");
    }

    // -- 2. title / size_limits ----------------------------------------------

    #[test]
    fn title_and_size_limits() {
        let w = window_with_frame();
        assert_eq!(w.title(), Some("Edit"));
        // min forced to the window minimum {16, 6}; max is the owner size.
        let (min, max) = w.size_limits(Point::new(80, 25));
        assert_eq!(min, Point::new(16, 6), "window minimum");
        assert_eq!(max, Point::new(80, 25), "max is the owner size");
    }

    /// An owner-driven resize must honour the window's 16×6 floor (because
    /// `calc_bounds` routes through the *window's*
    /// `size_limits`, NOT the group's 0×0). Shrink the window's right/bottom
    /// edges below the floor via `calc_bounds` and assert it clamps to ≥ 16×6.
    #[test]
    fn calc_bounds_honours_min_win_size() {
        let mut w = window_with_frame(); // bounds 0,0,40,15
        w.state_mut().grow_mode = GrowMode {
            hi_x: true,
            hi_y: true,
            ..Default::default()
        };
        // Owner shrank to (10, 4): far below 16×6. delta = new(10,4) - old(40,15).
        let owner = Point::new(10, 4);
        let delta = Point::new(10 - 40, 4 - 15);
        let b = View::calc_bounds(&mut w, owner, delta);
        let size = b.b - b.a;
        assert!(
            size.x >= 16 && size.y >= 6,
            "window must not shrink below minWinSize {{16,6}}, got {size:?}"
        );
    }

    // -- 3. set_state activation flips the frame active ----------------------

    #[test]
    fn select_activates_window_and_frame() {
        let mut w = window_with_frame();
        let frame_idx = w.group.index_of_pub(w.frame_id()).unwrap();
        assert!(
            !w.group.child_state_mut(frame_idx).state.active,
            "frame starts passive"
        );

        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        with_ctx(&mut out, &mut timers, |ctx| {
            View::set_state(&mut w, StateFlag::Selected, true, ctx)
        });

        // The window (group) is active...
        assert!(w.state().state.active, "window active after select");
        // ...and the Active propagation flipped the frame child active (no manual push).
        let frame_idx = w.group.index_of_pub(w.frame_id()).unwrap();
        assert!(
            w.group.child_state_mut(frame_idx).state.active,
            "frame went active via Group::set_state(Active) propagation"
        );

        // Deselecting reverses it.
        with_ctx(&mut out, &mut timers, |ctx| {
            View::set_state(&mut w, StateFlag::Selected, false, ctx)
        });
        let frame_idx = w.group.index_of_pub(w.frame_id()).unwrap();
        assert!(!w.state().state.active, "window passive after deselect");
        assert!(
            !w.group.child_state_mut(frame_idx).state.active,
            "frame went passive again"
        );
    }

    // -- 4. standard_scroll_bar ----------------------------------------------

    #[test]
    fn standard_scroll_bar_vertical_rect_and_keyboard() {
        let mut w = window_with_frame(); // extent 0,0,40,15 -> w=40, h=15
        let id = w.standard_scroll_bar(ScrollBarOptions {
            vertical: true,
            handle_keyboard: true,
        });
        let idx = w.group.index_of_pub(id).unwrap();
        // vertical: (w-1, 1, w, h-1) = (39, 1, 40, 14).
        assert_eq!(
            w.group.child_state_mut(idx).get_bounds(),
            Rect::new(39, 1, 40, 14),
            "vertical bar at the right edge"
        );
        assert!(
            w.group.child_state_mut(idx).options.post_process,
            "sbHandleKeyboard → ofPostProcess"
        );
    }

    #[test]
    fn standard_scroll_bar_horizontal_rect_no_keyboard() {
        let mut w = window_with_frame(); // w=40, h=15
        let id = w.standard_scroll_bar(ScrollBarOptions::default());
        let idx = w.group.index_of_pub(id).unwrap();
        // horizontal: (2, h-1, w-2, h) = (2, 14, 38, 15).
        assert_eq!(
            w.group.child_state_mut(idx).get_bounds(),
            Rect::new(2, 14, 38, 15),
            "horizontal bar at the bottom edge"
        );
        assert!(
            !w.group.child_state_mut(idx).options.post_process,
            "no sbHandleKeyboard → no ofPostProcess"
        );
    }

    // -- 5. Tab focus cycling ------------------------------------------------

    #[test]
    fn kb_tab_cycles_focus_and_consumes() {
        let mut w = window_with_frame();
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        // Two selectable probe children + a vertical scrollbar (also selectable).
        let id_a = w.group.insert(Probe::boxed(Rect::new(1, 1, 10, 5)));
        let id_b = w.group.insert(Probe::boxed(Rect::new(1, 6, 10, 10)));
        // Establish a current (focus_next/find_next return None if current is None).
        with_ctx(&mut out, &mut timers, |ctx| {
            w.group
                .set_current(Some(id_b), crate::view::SelectMode::Normal, ctx)
        });
        assert_eq!(w.group.current(), Some(id_b));

        // Tab (forwards) moves current to the next selectable child + consumes.
        // Children in insert order: [frame, id_a, id_b]; current == id_b. Forward
        // tab order is decreasing Vec index with wrap (see `Group::find_next`), so
        // from id_b (idx 2) the next eligible child is id_a (idx 1) — the frame
        // (idx 0) is not selectable. Focus lands deterministically on id_a.
        let mut ev = tab_event(false);
        with_ctx(&mut out, &mut timers, |ctx| w.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "Tab consumed");
        assert_eq!(
            w.group.current(),
            Some(id_a),
            "forward tab moves focus from id_b to id_a"
        );

        // Shift+Tab (backwards) also consumes.
        let mut ev2 = tab_event(true);
        with_ctx(&mut out, &mut timers, |ctx| w.handle_event(&mut ev2, ctx));
        assert!(ev2.is_nothing(), "Shift+Tab consumed");
    }

    // -- 6. WindowFlags relocation (compiles + frame tests still pass) --------

    /// `WindowFlags` now lives here; the crate-root re-export resolves through
    /// this module. `frame.rs` imports it from here (its own tests cover the
    /// frame side). This test just exercises the relocated type.
    #[test]
    fn window_flags_relocated_here() {
        let f = WindowFlags {
            close: true,
            ..Default::default()
        };
        assert!(f.close && !f.zoom);
        // The frame's pushed-down flags use the same (relocated) type.
        let mut frame = Frame::new(Rect::new(0, 0, 10, 5));
        frame.set_flags(f);
        assert!(frame.flags().close);
    }

    // -- 7. mandatory snapshot -----------------------------------------------

    /// End-to-end: a selected (active) `Window` with a title + a vertical
    /// scrollbar, drawn through `&mut dyn View` on a `HeadlessBackend`. Shows
    /// the double-line active border, centered title, icons, and the scrollbar.
    #[test]
    fn selected_window_with_scrollbar_snapshot() {
        let theme = Theme::classic_blue();
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut w = Window::new(Rect::new(0, 0, 24, 8), Some("Edit".into()), 1);
        w.standard_scroll_bar(ScrollBarOptions {
            vertical: true,
            handle_keyboard: true,
        });
        // Select → active frame (double-line border + icons).
        with_ctx(&mut out, &mut timers, |ctx| {
            View::set_state(&mut w, StateFlag::Selected, true, ctx)
        });

        let mut view: Box<dyn View> = Box::new(w);
        let (backend, screen) = HeadlessBackend::new(24, 8);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = view.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            view.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    // -- 8. set_state enables/disables the zoom + close commands --------------

    /// Selecting a zoom/close-enabled window queues `(Command::ZOOM, true)` and
    /// `(Command::CLOSE, true)` on the command-change channel; deselecting queues
    /// the matching `false` pairs.
    #[test]
    fn set_state_select_enables_and_disables_cm_zoom_and_close() {
        let mut w = window_with_frame(); // flags.zoom = true, flags.close = true
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = Vec::new();

        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            View::set_state(&mut w, StateFlag::Selected, true, &mut ctx);
        }
        use crate::view::Deferred;
        assert!(
            deferred
                .iter()
                .any(|d| matches!(d, Deferred::EnableCommand(Command::ZOOM))),
            "select enables cmZoom"
        );
        assert!(
            deferred
                .iter()
                .any(|d| matches!(d, Deferred::EnableCommand(Command::CLOSE))),
            "select enables cmClose"
        );

        deferred.clear();
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            View::set_state(&mut w, StateFlag::Selected, false, &mut ctx);
        }
        assert!(
            deferred
                .iter()
                .any(|d| matches!(d, Deferred::DisableCommand(Command::ZOOM))),
            "deselect disables cmZoom"
        );
        assert!(
            deferred
                .iter()
                .any(|d| matches!(d, Deferred::DisableCommand(Command::CLOSE))),
            "deselect disables cmClose"
        );
    }

    // -- 9. zoom() toggles bounds + pushes the frame zoomed bool --------------

    /// Read the frame child's pushed `zoomed` flag through the downcast seam.
    fn frame_zoomed(w: &mut Window) -> bool {
        w.group
            .child_mut(w.frame_id())
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<Frame>())
            .expect("frame child resolves through child_mut + as_any_mut")
            .zoomed()
    }

    #[test]
    fn zoom_toggles_bounds_and_pushes_frame_zoomed() {
        // Window smaller than its owner desktop.
        let mut w = Window::new(Rect::new(2, 1, 22, 9), Some("Edit".into()), 1); // 20x8
        let original = w.state().get_bounds();
        let desktop_size = Point::new(80, 25);

        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = Vec::new();

        // Select the window (realism: the desktop would have it selected) and feed
        // a zoom command directly. owner_size is set on the ctx (the desktop's
        // job): the group.handle_event inside Window restores owner_size to this
        // value, so by the time the zoom arm runs zoom(), owner_size ==
        // desktop_size.
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            View::set_state(&mut w, StateFlag::Selected, true, &mut ctx);
        }

        // First zoom: fill the owner.
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            ctx.set_owner_size(desktop_size);
            let mut ev = Event::Command(Command::ZOOM);
            w.handle_event(&mut ev, &mut ctx);
            assert!(ev.is_nothing(), "cmZoom consumed");
        }
        assert_eq!(
            w.state().get_bounds(),
            Rect::new(0, 0, desktop_size.x, desktop_size.y),
            "first zoom fills the owner"
        );
        assert_eq!(
            w.zoom_rect(),
            original,
            "zoom_rect saved the original bounds"
        );
        assert!(frame_zoomed(&mut w), "frame pushed zoomed = true");

        // Second zoom (toggle): restore.
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            ctx.set_owner_size(desktop_size);
            let mut ev = Event::Command(Command::ZOOM);
            w.handle_event(&mut ev, &mut ctx);
            assert!(ev.is_nothing(), "cmZoom consumed");
        }
        assert_eq!(
            w.state().get_bounds(),
            original,
            "second zoom restores the original bounds"
        );
        assert!(!frame_zoomed(&mut w), "frame pushed zoomed = false");
    }

    // -- 10. mandatory snapshot: restored vs zoomed --------------------------

    fn render_window(view: &mut dyn View, theme: &Theme, w: u16, h: u16) -> String {
        let (backend, screen) = HeadlessBackend::new(w, h);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = view.state().get_bounds();
            let mut dc = DrawCtx::new(buf, theme, bounds, bounds.a);
            view.draw(&mut dc);
        });
        screen.snapshot()
    }

    #[test]
    fn zoom_restored_vs_filled_snapshot() {
        let theme = Theme::classic_blue();
        let screen = Point::new(24, 8);
        // A window occupying the upper-left, smaller than the 24x8 screen.
        let mut w = Window::new(Rect::new(0, 0, 14, 5), Some("Edit".into()), 1);
        w.standard_scroll_bar(ScrollBarOptions {
            vertical: true,
            handle_keyboard: true,
        });

        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = Vec::new();
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            View::set_state(&mut w, StateFlag::Selected, true, &mut ctx);
        }

        // Restored snapshot.
        insta::assert_snapshot!("zoom_restored", render_window(&mut w, &theme, 24, 8));

        // Zoom to fill the screen.
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            ctx.set_owner_size(screen);
            let mut ev = Event::Command(Command::ZOOM);
            w.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(w.state().get_bounds(), Rect::new(0, 0, 24, 8));

        // Zoomed snapshot: frame fills the whole 24x8 screen.
        insta::assert_snapshot!("zoom_filled", render_window(&mut w, &theme, 24, 8));
    }

    /// A selected (active) window draws its frame in the double-line
    /// [`Role::FrameActive`] style: the top-left corner is the `╔` glyph and the
    /// cell's style is `theme.style(Role::FrameActive)`. A cheap direct assertion
    /// alongside the snapshot.
    #[test]
    fn active_frame_uses_active_border_style() {
        let mut w = window_with_frame();
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        with_ctx(&mut out, &mut timers, |ctx| {
            View::set_state(&mut w, StateFlag::Selected, true, ctx)
        });
        let theme = Theme::classic_blue();
        let mut buf = Buffer::new(40, 15);
        {
            let bounds = w.state().get_bounds();
            let mut dc = DrawCtx::new(&mut buf, &theme, bounds, bounds.a);
            w.draw(&mut dc);
        }
        // The active border corner uses the double-line glyph in FrameActive style.
        assert_eq!(buf.get(0, 0).symbol(), "╔", "active double-line corner");
        assert_eq!(
            buf.get(0, 0).style(),
            theme.style(Role::FrameActive),
            "active border style"
        );
    }

    // -- 11. drag-start detection (unit; no pump) -----------------------------

    use crate::event::{MouseButtons, MouseEvent};

    /// A left-button `MouseDown` at window-local `(x, y)`.
    fn mouse_down_local(x: i32, y: i32) -> Event {
        Event::MouseDown(MouseEvent {
            position: Point::new(x, y),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        })
    }

    /// A middle-button `MouseDown` at window-local `(x, y)`.
    fn mouse_down_middle(x: i32, y: i32) -> Event {
        Event::MouseDown(MouseEvent {
            position: Point::new(x, y),
            buttons: MouseButtons {
                middle: true,
                ..Default::default()
            },
            ..Default::default()
        })
    }

    /// Insert a fresh `Window` into a throwaway parent `Group` so it gets its
    /// `ViewId` stamped (the substrate stamps the id at `Group::insert`, NOT at
    /// `Window::new`; `start_drag`'s `self.group.state().id()` is `None` otherwise).
    /// Returns the parent group (owning the window) + the window's id.
    fn window_in_group(bounds: Rect) -> (Group, ViewId) {
        let mut parent = Group::new(Rect::new(0, 0, 80, 25));
        let w = Window::new(bounds, Some("Edit".into()), 1);
        let id = parent.insert(Box::new(w));
        (parent, id)
    }

    /// Helper: build a `Context`, call `handle_event` on the window resolved by
    /// `id` inside `parent`, and report (event-consumed, pushed-capture-count).
    fn drive_window(
        parent: &mut Group,
        id: ViewId,
        ev: &mut Event,
        owner_size: Point,
    ) -> (bool, usize) {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = Vec::new();
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            ctx.set_owner_size(owner_size);
            let win = parent.find_mut(id).expect("window resolves");
            win.handle_event(ev, &mut ctx);
        }
        let pushed = deferred
            .iter()
            .filter(|d| matches!(d, crate::view::Deferred::PushCapture(_)))
            .count();
        (ev.is_nothing(), pushed)
    }

    /// A title-bar `MouseDown` (window-local `y == 0`) starts a Move drag: the
    /// window's `dragging` goes true, the event is consumed, and one capture is
    /// queued in the local `deferred` Vec.
    #[test]
    fn title_bar_mouse_down_starts_move_drag() {
        let (mut parent, id) = window_in_group(Rect::new(2, 1, 22, 9)); // 20x8
        let mut ev = mouse_down_local(6, 0); // title bar, away from icons
        let (consumed, pending_len) = drive_window(&mut parent, id, &mut ev, Point::new(80, 25));
        assert!(consumed, "drag-start consumes the MouseDown");
        assert_eq!(pending_len, 1, "one DragCapture queued");
        assert!(
            parent.find_mut(id).unwrap().state().state.dragging,
            "sfDragging set on the window"
        );
    }

    /// A bottom-right corner `MouseDown` (grow allowed) starts a Grow; a
    /// bottom-left corner starts a GrowLeft; both consume + queue one capture.
    #[test]
    fn bottom_corner_mouse_down_starts_grow() {
        // Bottom-right: window-local (w-1, h-1).
        let (mut parent, id) = window_in_group(Rect::new(2, 1, 22, 9)); // 20x8
        let mut ev = mouse_down_local(19, 7);
        let (consumed, n) = drive_window(&mut parent, id, &mut ev, Point::new(80, 25));
        assert!(consumed && n == 1, "bottom-right starts a grow drag");
        assert!(parent.find_mut(id).unwrap().state().state.dragging);

        // Bottom-left: window-local (0, h-1).
        let (mut parent2, id2) = window_in_group(Rect::new(2, 1, 22, 9));
        let mut ev2 = mouse_down_local(0, 7);
        let (consumed2, n2) = drive_window(&mut parent2, id2, &mut ev2, Point::new(80, 25));
        assert!(consumed2 && n2 == 1, "bottom-left starts a grow-left drag");
        assert!(parent2.find_mut(id2).unwrap().state().state.dragging);
    }

    /// An interior non-edge left click starts no drag (and reaches no drag spot):
    /// nothing is queued and `dragging` stays false.
    #[test]
    fn interior_mouse_down_starts_no_drag() {
        let (mut parent, id) = window_in_group(Rect::new(2, 1, 22, 9)); // 20x8
        let mut ev = mouse_down_local(8, 4); // interior, not title/corner
        let (_consumed, n) = drive_window(&mut parent, id, &mut ev, Point::new(80, 25));
        assert_eq!(n, 0, "no DragCapture queued for an interior click");
        assert!(
            !parent.find_mut(id).unwrap().state().state.dragging,
            "interior click does not start a drag"
        );
    }

    /// A middle-button interior `MouseDown` (move allowed) starts a Move drag.
    #[test]
    fn middle_button_interior_starts_move_drag() {
        let (mut parent, id) = window_in_group(Rect::new(2, 1, 22, 9)); // 20x8
        let mut ev = mouse_down_middle(8, 4);
        let (consumed, n) = drive_window(&mut parent, id, &mut ev, Point::new(80, 25));
        assert!(
            consumed && n == 1,
            "middle-button interior starts a move drag"
        );
        assert!(parent.find_mut(id).unwrap().state().state.dragging);
    }

    // -- 12. move_grow unit tests (pure fn) -----------------------------------

    /// `move_grow` must use `min(max())`, NOT `clamp()`. The classic TV inversion
    /// is the owner SMALLER than the window's minimum size (`min > max`): then the
    /// size clamp is `s.x.max(16).min(10)` = `clamp(16, 10)`, which **panics** with
    /// `i32::clamp` (lo > hi) but yields `hi` (= max) with `min(max())`. This test
    /// would fail (panic) if someone "simplified" the correct min/max into clamp().
    #[test]
    fn move_grow_oversized_min_uses_min_max_not_clamp() {
        let limits = Rect::new(0, 0, 20, 10);
        let min = Point::new(16, 6); // window minimum
        let max = Point::new(10, 4); // owner SMALLER than the window minimum (inverted)
        let mode = DragMode {
            limit_lo_y: true,
            ..Default::default()
        };
        // Reaching the assert at all proves no panic; the size clamps to `hi` (=max)
        // per "min(max()) yields hi when lo>hi".
        let r = move_grow(Point::new(0, 0), Point::new(20, 20), limits, min, max, mode);
        let sz = r.b - r.a;
        assert_eq!(
            sz,
            Point::new(10, 4),
            "size clamps to max (hi) when min>max, via min(max()) not clamp()"
        );
    }

    /// An ordinary in-range move: a small window inside a roomy limit, no
    /// high-edge limit bits, lands where requested (after the general
    /// [a-s+1, b-1] band, which a centrally-placed window is well inside).
    #[test]
    fn move_grow_in_range_move() {
        let limits = Rect::new(0, 0, 80, 25);
        let min = Point::new(16, 6);
        let max = Point::new(80, 25);
        let mode = DragMode {
            limit_lo_y: true,
            ..Default::default()
        };
        let r = move_grow(Point::new(10, 5), Point::new(20, 8), limits, min, max, mode);
        assert_eq!(r, Rect::new(10, 5, 30, 13), "in-range move passes through");
    }

    // -- View::number override ------------------------------------------------

    #[test]
    fn view_number_some_when_positive_none_when_zero() {
        // Positive number -> Some(n).
        let w = Window::new(Rect::new(0, 0, 20, 6), Some("A".into()), 4);
        assert_eq!(View::number(&w), Some(4), "number > 0 -> Some");
        // 0 -> None (an unnumbered window is never a select-by-number target).
        let w0 = Window::new(Rect::new(0, 0, 20, 6), Some("B".into()), 0);
        assert_eq!(View::number(&w0), None, "number == 0 (unnumbered) -> None");
    }

    #[test]
    fn set_state_select_enables_cm_next_and_prev_unconditionally() {
        let mut w = window_with_frame();
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = Vec::new();
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            View::set_state(&mut w, StateFlag::Selected, true, &mut ctx);
        }
        use crate::view::Deferred;
        assert!(
            deferred
                .iter()
                .any(|d| matches!(d, Deferred::EnableCommand(Command::NEXT))),
            "select enables cmNext (unconditional)"
        );
        assert!(
            deferred
                .iter()
                .any(|d| matches!(d, Deferred::EnableCommand(Command::PREV))),
            "select enables cmPrev (unconditional)"
        );
        deferred.clear();
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            View::set_state(&mut w, StateFlag::Selected, false, &mut ctx);
        }
        assert!(
            deferred
                .iter()
                .any(|d| matches!(d, Deferred::DisableCommand(Command::NEXT))),
            "deselect disables cmNext"
        );
        assert!(
            deferred
                .iter()
                .any(|d| matches!(d, Deferred::DisableCommand(Command::PREV))),
            "deselect disables cmPrev"
        );
    }

    // -- 13. locate() re-pushes set_zoomed on bounds change (B5) ---------------

    /// After `locate` changes the window bounds (non-trivially), the frame's
    /// `zoomed` flag is updated to reflect the new size vs max. This verifies
    /// B5: the pushed bool no longer goes stale on owner resize / change_bounds.
    #[test]
    fn locate_repushes_set_zoomed_after_bounds_change() {
        let mut w = Window::new(Rect::new(0, 0, 20, 8), Some("Edit".into()), 1);
        let desktop_size = Point::new(80, 25);

        // Manually set frame zoomed = true to confirm it gets RESET after a
        // non-max locate.
        {
            let frame_id = w.frame_id();
            if let Some(frame) = w
                .group
                .child_mut(frame_id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<Frame>())
            {
                frame.set_zoomed(true);
            }
        }
        assert!(
            frame_zoomed(&mut w),
            "precondition: frame zoomed = true before locate"
        );

        // locate to a smaller-than-max rect → zoomed must become false.
        w.locate(Rect::new(5, 3, 30, 12), desktop_size);
        assert!(
            !frame_zoomed(&mut w),
            "locate to non-max rect pushes zoomed = false"
        );

        // locate to exactly the max rect → zoomed must become true.
        let (_, max) = View::size_limits(&w, desktop_size);
        w.locate(Rect::new(0, 0, max.x, max.y), desktop_size);
        assert!(
            frame_zoomed(&mut w),
            "locate to max rect pushes zoomed = true"
        );
    }

    // -- 14. joined splitter: divider tees auto-brokered into the frame --------

    /// A minimal fill view for splitter-pane snapshot tests.
    fn plain_fill(ch: char) -> Box<dyn View> {
        struct F(char, ViewState);
        impl View for F {
            fn state(&self) -> &ViewState {
                &self.1
            }
            fn state_mut(&mut self) -> &mut ViewState {
                &mut self.1
            }
            fn draw(&mut self, ctx: &mut DrawCtx) {
                let b = self.1.get_bounds();
                let (w, h) = (b.b.x - b.a.x, b.b.y - b.a.y);
                ctx.fill(
                    Rect::new(0, 0, w, h),
                    self.0,
                    ctx.style(crate::theme::Role::Normal),
                );
            }
        }
        Box::new(F(ch, ViewState::new(Rect::new(0, 0, 1, 1))))
    }

    #[test]
    fn joined_splitter_frame_tees_top_and_bottom() {
        use crate::widgets::{Constraints, Splitter};
        let theme = Theme::classic_blue();
        let mut win = Window::new(Rect::new(0, 0, 16, 6), None, 0);
        let ext = win.state().get_extent();
        let interior = Rect::new(1, 1, ext.b.x - 1, ext.b.y - 1);
        let split = Splitter::cols()
            .pane(plain_fill('A'), Constraints::flex())
            .pane(plain_fill('B'), Constraints::flex())
            .joined();
        let sid = win.insert_child(Box::new(split));
        if let Some(v) = win.child_mut(sid) {
            v.change_bounds(interior);
        }
        let (backend, screen) = HeadlessBackend::new(16, 6);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = win.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            win.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    #[test]
    fn joined_splitter_active_frame_uses_mixed_tees() {
        use crate::widgets::{Constraints, Splitter};
        let theme = Theme::classic_blue();
        let mut win = Window::new(Rect::new(0, 0, 16, 6), None, 0);
        let ext = win.state().get_extent();
        let interior = Rect::new(1, 1, ext.b.x - 1, ext.b.y - 1);
        let split = Splitter::cols()
            .pane(plain_fill('A'), Constraints::flex())
            .pane(plain_fill('B'), Constraints::flex())
            .joined();
        let sid = win.insert_child(Box::new(split));
        if let Some(v) = win.child_mut(sid) {
            v.change_bounds(interior);
        }
        // Activate the frame CHILD directly so it draws its double-line box (the
        // group's own active flag does not propagate to children without set_state).
        let fid = win.frame_id();
        if let Some(v) = win.child_mut(fid) {
            v.state_mut().state.active = true;
        }
        let (backend, screen) = HeadlessBackend::new(16, 6);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = win.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            win.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    #[test]
    fn unjoined_splitter_frame_is_plain() {
        use crate::widgets::{Constraints, Splitter};
        let theme = Theme::classic_blue();
        let mut win = Window::new(Rect::new(0, 0, 16, 6), None, 0);
        let ext = win.state().get_extent();
        let interior = Rect::new(1, 1, ext.b.x - 1, ext.b.y - 1);
        // Splitter NOT joined → the window auto-broker pushes empty marks.
        let split = Splitter::cols()
            .pane(plain_fill('A'), Constraints::flex())
            .pane(plain_fill('B'), Constraints::flex());
        let sid = win.insert_child(Box::new(split));
        if let Some(v) = win.child_mut(sid) {
            v.change_bounds(interior);
        }
        let (backend, screen) = HeadlessBackend::new(16, 6);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = win.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            win.draw(&mut dc);
        });
        let snap = screen.snapshot();
        // The first interior row (the top frame edge) must contain no junction tee.
        let top_edge = snap
            .lines()
            .find(|l| l.contains('┌') || l.contains('╔'))
            .unwrap_or("");
        assert!(
            !top_edge.contains('┬') && !top_edge.contains('╤'),
            "no tee without the flag: {top_edge:?}"
        );
        // Freeze a baseline reference rendering for the no-flag case.
        insta::assert_snapshot!(snap);
    }

    #[test]
    fn toggling_splitter_joined_off_reverts_to_plain_frame() {
        use crate::widgets::{Constraints, Splitter};
        let theme = Theme::classic_blue();
        let mut win = Window::new(Rect::new(0, 0, 16, 6), None, 0);
        let ext = win.state().get_extent();
        let interior = Rect::new(1, 1, ext.b.x - 1, ext.b.y - 1);
        let split = Splitter::cols()
            .pane(plain_fill('A'), Constraints::flex())
            .pane(plain_fill('B'), Constraints::flex())
            .joined();
        let sid = win.insert_child(Box::new(split));
        if let Some(v) = win.child_mut(sid) {
            v.change_bounds(interior);
        }

        // First draw with the splitter joined: the auto-broker adds a top tee.
        let render = |win: &mut Window| {
            let (backend, screen) = HeadlessBackend::new(16, 6);
            let mut r = Renderer::new(Box::new(backend));
            r.render(|buf: &mut Buffer| {
                let bounds = win.state().get_bounds();
                let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
                win.draw(&mut dc);
            });
            screen.snapshot()
        };
        let on = render(&mut win);
        assert!(on.contains('┬'), "joined: top tee present");

        // Un-join the splitter itself; the auto-broker now pushes empty marks and
        // the frame reverts to a plain border.
        if let Some(sp) = win
            .child_mut(sid)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<Splitter>())
        {
            sp.set_joined(false);
        }
        let off = render(&mut win);
        assert!(
            !off.contains('┬') && !off.contains('╤'),
            "after un-join, plain frame: {off:?}"
        );
    }

    /// A rows (horizontal-divider) splitter inside an ACTIVE (double-line) window
    /// frame: the divider's ends abut the LEFT and RIGHT frame edges.  At those
    /// junctions the frame's double vertical bar (║) meets the divider's single
    /// horizontal stem — the correct glyphs are ╟ (left, U+255F) and ╢ (right,
    /// U+2562).  Before the glyph-table fix this test would see ╞/╡ instead.
    #[test]
    fn joined_rows_splitter_active_frame_side_tees_are_double_bar_single_stem() {
        use crate::widgets::{Constraints, Splitter};
        let theme = Theme::classic_blue();
        let mut win = Window::new(Rect::new(0, 0, 16, 7), None, 0);
        // Activate the frame child directly so it draws its double-line box.
        let fid = win.frame_id();
        if let Some(v) = win.child_mut(fid) {
            v.state_mut().state.active = true;
        }
        let ext = win.state().get_extent();
        let interior = Rect::new(1, 1, ext.b.x - 1, ext.b.y - 1);
        // A rows splitter: two stacked panes with one horizontal divider that
        // spans the full interior width → its left end meets the LEFT frame edge
        // and its right end meets the RIGHT frame edge.
        let split = Splitter::rows()
            .pane(plain_fill('A'), Constraints::flex())
            .pane(plain_fill('B'), Constraints::flex())
            .joined();
        let sid = win.insert_child(Box::new(split));
        if let Some(v) = win.child_mut(sid) {
            v.change_bounds(interior);
        }
        // Render into a direct Buffer so we can read individual cells.
        let mut buf = Buffer::new(16, 7);
        {
            let bounds = win.state().get_bounds();
            let mut dc = DrawCtx::new(&mut buf, &theme, bounds, bounds.a);
            win.draw(&mut dc);
        }
        // Scan interior rows (1..6) for the divider row: the row where the left
        // frame cell (col 0) is a tee glyph rather than the plain double bar ║.
        let mut divider_row: Option<u16> = None;
        for y in 1_u16..6 {
            let cell = buf.get(0, y).symbol().to_string();
            if cell != "║" {
                divider_row = Some(y);
                break;
            }
        }
        let y =
            divider_row.expect("a left-frame junction must exist on the horizontal divider row");
        assert_eq!(
            buf.get(0, y).symbol(),
            "╟",
            "left frame: double bar (║ weight) + single divider stem → ╟ (U+255F)"
        );
        assert_eq!(
            buf.get(15, y).symbol(),
            "╢",
            "right frame: double bar (║ weight) + single divider stem → ╢ (U+2562)"
        );
    }

    // -- unified keyboard resize: Tab cycles window <-> dividers --------------

    /// Build a parent `Group` holding a movable/growable `Window` whose body is a
    /// 3-pane cols `Splitter`. Returns `(parent, window_id, splitter_id, pane0_id)`.
    /// The splitter fills the window interior so its panes have non-zero bounds.
    fn window_with_splitter_body() -> (Group, ViewId, ViewId, ViewId) {
        use crate::widgets::{Constraints, Splitter};

        let mut parent = Group::new(Rect::new(0, 0, 80, 25));
        let mut win = Window::new(Rect::new(2, 2, 42, 14), Some("S".into()), 1); // 40x12

        let mut sp = Splitter::cols();
        let p0 = sp.insert(Probe::boxed(Rect::new(0, 0, 0, 0)), Constraints::flex());
        sp.insert(Probe::boxed(Rect::new(0, 0, 0, 0)), Constraints::flex());
        sp.insert(Probe::boxed(Rect::new(0, 0, 0, 0)), Constraints::flex());

        let sp_id = win.insert_child(Box::new(sp));
        // Size the splitter to the window interior (1-cell frame inset each side).
        let ext = win.state().get_extent();
        let interior = Rect::new(1, 1, ext.b.x - 1, ext.b.y - 1);
        if let Some(v) = win.child_mut(sp_id) {
            v.change_bounds(interior);
        }

        let id = parent.insert(Box::new(win));
        (parent, id, sp_id, p0)
    }

    /// Read pane `id`'s right boundary (`bounds.b.x`) by resolving it in `parent`.
    fn pane_right(parent: &mut Group, id: ViewId) -> i32 {
        parent
            .find_mut(id)
            .map(|v| v.state().get_bounds().b.x)
            .expect("pane resolves")
    }

    /// Apply every `Deferred::SplitterDivider` op in `deferred` to the splitter,
    /// mirroring the pump's D3 broker arm (downcast → `Splitter` → method).
    ///
    /// NOTE: this intentionally applies ONLY `SplitterDivider` ops. All other
    /// deferred effects (`request_bounds`, `request_set_state`, `push_capture`,
    /// …) are silently ignored — so a future test that asserts on window bounds
    /// or the `Dragging` flag through this helper would NOT observe them; such a
    /// test must drain those deferreds itself (or drive a real pump).
    fn apply_divider_ops(parent: &mut Group, deferred: &[crate::view::Deferred]) {
        use crate::view::{Deferred, DividerOp};
        use crate::widgets::Splitter;
        for d in deferred {
            if let Deferred::SplitterDivider { splitter, op } = d
                && let Some(sp) = parent
                    .find_mut(*splitter)
                    .and_then(|v| v.as_any_mut())
                    .and_then(|a| a.downcast_mut::<Splitter>())
            {
                match op {
                    DividerOp::SetActive(sel) => sp.set_active_divider(*sel),
                    DividerOp::Nudge { index, delta } => sp.nudge_divider(*index, *delta),
                    DividerOp::EndSession { commit } => sp.end_resize_session(*commit),
                }
            }
        }
    }

    /// Drive `handle_event` on the window resolved by `id`, returning the drained
    /// deferred Vec (so the caller can inspect/apply the queued ops).
    fn drive_collect(
        parent: &mut Group,
        id: ViewId,
        ev: &mut Event,
        owner_size: Point,
    ) -> Vec<crate::view::Deferred> {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = Vec::new();
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            ctx.set_owner_size(owner_size);
            let win = parent.find_mut(id).expect("window resolves");
            win.handle_event(ev, &mut ctx);
        }
        deferred
    }

    /// Feed `ev` to `cap` (a pushed capture) with a fresh `Context`; apply the
    /// queued divider ops to `parent` and return the `CaptureFlow`.
    fn feed_capture(
        parent: &mut Group,
        cap: &mut dyn CaptureHandler,
        mut ev: Event,
    ) -> CaptureFlow {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = Vec::new();
        let flow = {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            ctx.set_owner_size(Point::new(80, 25));
            cap.handle(&mut ev, &mut ctx)
        };
        apply_divider_ops(parent, &deferred);
        flow
    }

    /// Feed `ev` to `cap`; apply divider ops to `parent`; return `(flow, deferred)`.
    /// Use this when you also need to inspect `ChangeBounds` or other deferred ops.
    fn feed_capture_full(
        parent: &mut Group,
        cap: &mut dyn CaptureHandler,
        mut ev: Event,
    ) -> (CaptureFlow, Vec<crate::view::Deferred>) {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = Vec::new();
        let flow = {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            ctx.set_owner_size(Point::new(80, 25));
            cap.handle(&mut ev, &mut ctx)
        };
        apply_divider_ops(parent, &deferred);
        (flow, deferred)
    }

    /// Extract the last `ChangeBounds` rect for `id` from a deferred Vec.
    fn last_bounds(deferred: &[crate::view::Deferred], id: ViewId) -> Option<Rect> {
        use crate::view::Deferred;
        deferred.iter().rev().find_map(|d| {
            if let Deferred::ChangeBounds(vid, r) = d
                && *vid == id
            {
                Some(*r)
            } else {
                None
            }
        })
    }

    fn key(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(k, KeyModifiers::default()))
    }

    fn key_shift(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(
            k,
            KeyModifiers {
                shift: true,
                ..Default::default()
            },
        ))
    }

    fn key_ctrl(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(
            k,
            KeyModifiers {
                ctrl: true,
                ..Default::default()
            },
        ))
    }

    fn key_ctrl_shift(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(
            k,
            KeyModifiers {
                ctrl: true,
                shift: true,
                ..Default::default()
            },
        ))
    }

    /// Extract the (single) `PushCapture` handler from a drained deferred Vec.
    fn take_capture(deferred: Vec<crate::view::Deferred>) -> Box<dyn CaptureHandler> {
        for d in deferred {
            if let crate::view::Deferred::PushCapture(h) = d {
                return h;
            }
        }
        panic!("RESIZE must push a capture");
    }

    #[test]
    fn resize_mode_tab_cycles_to_divider_and_arrow_moves_it() {
        let (mut parent, id, sp_id, p0) = window_with_splitter_body();

        // 1+2) deliver Command::RESIZE -> enters resize mode, target = Window.
        let mut ev = Event::Command(Command::RESIZE);
        let deferred = drive_collect(&mut parent, id, &mut ev, Point::new(80, 25));
        apply_divider_ops(&mut parent, &deferred); // no divider op for target[0]=Window
        let mut cap = take_capture(deferred);

        // Record divider-0 pane width before moving it.
        let before = pane_right(&mut parent, p0);

        // 3) Tab -> target = divider 0 (window Dragging cleared, divider 0 active).
        let f = feed_capture(&mut parent, cap.as_mut(), key(Key::Tab));
        assert!(matches!(f, CaptureFlow::Consumed), "Tab is consumed");

        // 4) Right -> divider 0 moves +1 along the cols axis.
        let f = feed_capture(&mut parent, cap.as_mut(), key(Key::Right));
        assert!(matches!(f, CaptureFlow::Consumed), "arrow is consumed");

        // 5) Enter -> commit.
        let f = feed_capture(&mut parent, cap.as_mut(), key(Key::Enter));
        assert!(
            matches!(f, CaptureFlow::ConsumedPop),
            "Enter commits + pops"
        );

        let after = pane_right(&mut parent, p0);
        assert_eq!(
            after,
            before + 1,
            "Tab→divider 0, Right nudges its right boundary +1 (committed)"
        );
        let _ = sp_id;
    }

    #[test]
    fn resize_mode_esc_restores_divider() {
        let (mut parent, id, _sp_id, p0) = window_with_splitter_body();

        let mut ev = Event::Command(Command::RESIZE);
        let deferred = drive_collect(&mut parent, id, &mut ev, Point::new(80, 25));
        apply_divider_ops(&mut parent, &deferred);
        let mut cap = take_capture(deferred);

        let before = pane_right(&mut parent, p0);

        feed_capture(&mut parent, cap.as_mut(), key(Key::Tab)); // -> divider 0
        feed_capture(&mut parent, cap.as_mut(), key(Key::Right));
        feed_capture(&mut parent, cap.as_mut(), key(Key::Right));
        // moved by +2 before cancel
        assert_eq!(pane_right(&mut parent, p0), before + 2, "moved before Esc");

        let f = feed_capture(&mut parent, cap.as_mut(), key(Key::Esc));
        assert!(matches!(f, CaptureFlow::ConsumedPop), "Esc cancels + pops");

        assert_eq!(
            pane_right(&mut parent, p0),
            before,
            "Esc restores divider-0 to its pre-mode position"
        );
    }

    // -- 15. keyboard resize: plain arrow moves, Shift+arrow resizes (faithful TView::change) ---

    /// Enter RESIZE mode (target = Window). Feed a plain Right arrow.
    /// Assert: origin.x increased by 1, size (width/height) UNCHANGED.
    #[test]
    fn resize_mode_plain_arrow_moves_window_not_resizes() {
        let (mut parent, id, _sp_id, _p0) = window_with_splitter_body();

        // Snapshot the window's bounds before entering resize mode.
        let init_bounds = parent.find_mut(id).unwrap().state().get_bounds();

        // Deliver RESIZE command → enters resize mode, target[0] = Window.
        let mut ev = Event::Command(Command::RESIZE);
        let deferred = drive_collect(&mut parent, id, &mut ev, Point::new(80, 25));
        apply_divider_ops(&mut parent, &deferred);
        let window_id = id; // captured for ChangeBounds lookup
        let mut cap = take_capture(deferred);

        // Feed plain Right (no modifiers) → should MOVE only.
        let (_flow, deferred) = feed_capture_full(&mut parent, cap.as_mut(), key(Key::Right));

        let new_bounds = last_bounds(&deferred, window_id)
            .expect("a plain arrow on the Window target must emit ChangeBounds");

        assert_eq!(
            new_bounds.b.x - new_bounds.a.x,
            init_bounds.b.x - init_bounds.a.x,
            "plain Right: window width must be UNCHANGED (move, not grow)"
        );
        assert_eq!(
            new_bounds.b.y - new_bounds.a.y,
            init_bounds.b.y - init_bounds.a.y,
            "plain Right: window height must be UNCHANGED (move, not grow)"
        );
        assert_eq!(
            new_bounds.a.x,
            init_bounds.a.x + 1,
            "plain Right: window origin.x must increase by 1"
        );
        assert_eq!(
            new_bounds.a.y, init_bounds.a.y,
            "plain Right: window origin.y must be UNCHANGED"
        );
    }

    /// Enter RESIZE mode (target = Window). Feed Shift+Right.
    /// Assert: width increased by 1, origin UNCHANGED.
    #[test]
    fn resize_mode_shift_arrow_resizes_window_not_moves() {
        let (mut parent, id, _sp_id, _p0) = window_with_splitter_body();

        let init_bounds = parent.find_mut(id).unwrap().state().get_bounds();

        let mut ev = Event::Command(Command::RESIZE);
        let deferred = drive_collect(&mut parent, id, &mut ev, Point::new(80, 25));
        apply_divider_ops(&mut parent, &deferred);
        let window_id = id;
        let mut cap = take_capture(deferred);

        // Feed Shift+Right → should GROW only.
        let (_flow, deferred) = feed_capture_full(&mut parent, cap.as_mut(), key_shift(Key::Right));

        let new_bounds = last_bounds(&deferred, window_id)
            .expect("Shift+arrow on the Window target must emit ChangeBounds");

        assert_eq!(
            new_bounds.a.x, init_bounds.a.x,
            "Shift+Right: window origin.x must be UNCHANGED (grow, not move)"
        );
        assert_eq!(
            new_bounds.a.y, init_bounds.a.y,
            "Shift+Right: window origin.y must be UNCHANGED"
        );
        assert_eq!(
            new_bounds.b.x - new_bounds.a.x,
            init_bounds.b.x - init_bounds.a.x + 1,
            "Shift+Right: window width must increase by 1"
        );
        assert_eq!(
            new_bounds.b.y - new_bounds.a.y,
            init_bounds.b.y - init_bounds.a.y,
            "Shift+Right: window height must be UNCHANGED"
        );
    }

    /// Enter RESIZE mode (target = Window). Feed Ctrl+Right (big step, no shift).
    /// Assert: origin.x increased by 8, size UNCHANGED.
    #[test]
    fn resize_mode_ctrl_arrow_big_move() {
        let (mut parent, id, _sp_id, _p0) = window_with_splitter_body();

        let init_bounds = parent.find_mut(id).unwrap().state().get_bounds();

        let mut ev = Event::Command(Command::RESIZE);
        let deferred = drive_collect(&mut parent, id, &mut ev, Point::new(80, 25));
        apply_divider_ops(&mut parent, &deferred);
        let window_id = id;
        let mut cap = take_capture(deferred);

        // Feed Ctrl+Right (big step, plain = move) → origin shifts by 8.
        let (_flow, deferred) = feed_capture_full(&mut parent, cap.as_mut(), key_ctrl(Key::Right));

        let new_bounds =
            last_bounds(&deferred, window_id).expect("Ctrl+arrow on Window must emit ChangeBounds");

        // Size must be unchanged.
        assert_eq!(
            new_bounds.b.x - new_bounds.a.x,
            init_bounds.b.x - init_bounds.a.x,
            "Ctrl+Right: width unchanged (big move, not grow)"
        );
        // Origin.x should move by 8 (clamped to limits; the window starts at x=2,
        // so +8 = 10 is well within 80-wide owner).
        assert_eq!(
            new_bounds.a.x,
            init_bounds.a.x + 8,
            "Ctrl+Right: origin.x increases by the large step (8)"
        );
    }

    /// Enter RESIZE mode (target = Window). Feed Ctrl+Shift+Right (big grow).
    /// Ctrl scales the step to 8; Shift selects grow. Assert: width += 8,
    /// origin UNCHANGED.
    #[test]
    fn resize_mode_ctrl_shift_arrow_big_resizes_window() {
        let (mut parent, id, _sp_id, _p0) = window_with_splitter_body();

        let init_bounds = parent.find_mut(id).unwrap().state().get_bounds();

        let mut ev = Event::Command(Command::RESIZE);
        let deferred = drive_collect(&mut parent, id, &mut ev, Point::new(80, 25));
        apply_divider_ops(&mut parent, &deferred);
        let window_id = id;
        let mut cap = take_capture(deferred);

        // Feed Ctrl+Shift+Right (big step + grow) → width grows by 8.
        let (_flow, deferred) =
            feed_capture_full(&mut parent, cap.as_mut(), key_ctrl_shift(Key::Right));

        let new_bounds = last_bounds(&deferred, window_id)
            .expect("Ctrl+Shift+arrow on Window must emit ChangeBounds");

        // Origin must be unchanged (grow, not move).
        assert_eq!(
            new_bounds.a.x, init_bounds.a.x,
            "Ctrl+Shift+Right: origin.x must be UNCHANGED (grow, not move)"
        );
        assert_eq!(
            new_bounds.a.y, init_bounds.a.y,
            "Ctrl+Shift+Right: origin.y must be UNCHANGED"
        );
        // Width grows by the large step (8); the window is 40 wide starting at
        // x=2, so 40+8 = 48 fits well within the 80-wide owner.
        assert_eq!(
            new_bounds.b.x - new_bounds.a.x,
            init_bounds.b.x - init_bounds.a.x + 8,
            "Ctrl+Shift+Right: width grows by the large step (8)"
        );
        // Height unchanged (horizontal arrow).
        assert_eq!(
            new_bounds.b.y - new_bounds.a.y,
            init_bounds.b.y - init_bounds.a.y,
            "Ctrl+Shift+Right: height must be UNCHANGED"
        );
    }

    // -- consumer API: public decoration setters + with_* builders ------------

    #[test]
    fn flags_off_window_is_a_fixed_iconless_panel() {
        // A consumer building TCV's fixed full-desktop panel: all decoration off.
        let w = Window::new(Rect::new(0, 0, 24, 8), Some("Catalog".into()), 0)
            .with_flags(WindowFlags::default()) // all four false
            .with_grow_mode(GrowMode::default())
            .with_drag_mode(DragMode::default());
        assert_eq!(
            w.flags(),
            WindowFlags {
                r#move: false,
                grow: false,
                close: false,
                zoom: false,
            },
            "consumer can clear all decoration flags"
        );
        let gm = w.state().grow_mode;
        assert!(
            !gm.lo_x && !gm.lo_y && !gm.hi_x && !gm.hi_y && !gm.rel && !gm.fixed,
            "consumer can clear grow_mode"
        );
    }

    #[test]
    fn with_palette_sets_and_pushes_to_frame() {
        let mut w = Window::new(Rect::new(0, 0, 24, 8), Some("Cyan".into()), 1)
            .with_palette(WindowPalette::Cyan);
        assert_eq!(w.palette(), WindowPalette::Cyan);
        let frame_id = w.frame_id();
        let frame = w
            .child_mut(frame_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<crate::frame::Frame>())
            .expect("window has a Frame child");
        assert_eq!(
            frame.palette(),
            WindowPalette::Cyan,
            "with_palette must propagate to the frame child"
        );
    }
}
