//! `TWindow` core — see the [module docs](super) for the deviation summary.

use crate::capture::{CaptureFlow, CaptureHandler};
use crate::command::Command;
use crate::event::{Event, Key};
use crate::frame::Frame;
use crate::view::{Context, DragMode, Group, GrowMode, Point, Rect, StateFlag, View, ViewId};
use crate::widgets::ScrollBar;

// ---------------------------------------------------------------------------
// WindowFlags — D5 struct-of-bools for the `wf*` word (relocated from frame.rs)
// ---------------------------------------------------------------------------

/// Window decoration flags — ports the `wf*` family (`dialogs.h`), D5.
///
/// Relocated here from `frame.rs`: these belong to `TWindow` (the `Frame` only
/// renders a pushed-down copy). The window pushes its flags down to its frame
/// via [`Frame::set_flags`](crate::frame::Frame::set_flags).
///
/// The keyword-colliding `wfMove` becomes the raw identifier `r#move`,
/// consistent with the project's `r#move` / `r#union` precedent in geometry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowFlags {
    /// `wfMove` — the window can be moved by dragging its frame.
    pub r#move: bool,
    /// `wfGrow` — the window can be resized by dragging its bottom corners.
    pub grow: bool,
    /// `wfClose` — the window shows a close icon (and accepts `cmClose`).
    pub close: bool,
    /// `wfZoom` — the window shows a zoom icon (and accepts `cmZoom`).
    pub zoom: bool,
}

// ---------------------------------------------------------------------------
// WindowPalette — the `palette` member (getPalette under D7)
// ---------------------------------------------------------------------------

/// Which colour scheme the window draws in — ports the `wpBlueWindow` /
/// `wpCyanWindow` / `wpGrayWindow` palette index (`views.h`).
///
/// Under D7 there is no `getPalette` returning a `TPalette*`; the scheme is just
/// recorded here. **Multi-scheme theming is deferred to row 34:** the `Frame`
/// currently renders the single (blue) scheme via `Role::FrameActive` /
/// `FramePassive` / `FrameDragging`. Mapping `Cyan`/`Gray` to distinct theme
/// roles is row 34's job (`TDialog` uses `Gray`); we do **not** expand the
/// `Theme`/`Role` set now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WindowPalette {
    /// `wpBlueWindow` — the default window scheme (the ctor default).
    #[default]
    Blue,
    /// `wpCyanWindow` — the cyan scheme (theming → row 34).
    Cyan,
    /// `wpGrayWindow` — the gray scheme used by dialogs (theming → row 34).
    Gray,
}

// ---------------------------------------------------------------------------
// ScrollBarOptions — the `sb*` option word for standardScrollBar (views.h)
// ---------------------------------------------------------------------------

/// Options for [`Window::standard_scroll_bar`] — ports the `aOptions` word of
/// `TWindow::standardScrollBar` (`sbHorizontal`/`sbVertical`/`sbHandleKeyboard`,
/// `views.h`).
///
/// `sbHorizontal == 0` is the default (both flags false → a horizontal bar that
/// does not handle the keyboard).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollBarOptions {
    /// `sbVertical` — place the bar on the right edge (else the bottom edge).
    pub vertical: bool,
    /// `sbHandleKeyboard` — the bar opts into `ofPostProcess` so it handles
    /// focused-chain arrow keys even when not the current view.
    pub handle_keyboard: bool,
}

// ---------------------------------------------------------------------------
// Window
// ---------------------------------------------------------------------------

/// `TWindow` — a framed, selectable window: a [`Group`] that builds a
/// [`Frame`](crate::frame::Frame) around itself (D2/D3, row 33).
///
/// Build with [`Window::new`], then drive it as any other [`View`]. See the
/// [module docs](super) for the deviations and the 33c deferrals.
pub struct Window {
    /// The embedded container (D2). `Window` *is-a* `TGroup`: its state, draw,
    /// and event routing are the group's.
    group: Group,
    /// `TWindow::frame` — the frame child's id. 33c's `zoom` pushes
    /// `set_zoomed` through it; kept live now by [`frame_id`](Self::frame_id).
    frame_id: ViewId,
    /// `TWindow::flags` (D5 struct-of-bools).
    flags: WindowFlags,
    /// `TWindow::zoomRect` — the saved bounds for un-zoom, consumed by 33c's
    /// `zoom`. Kept live now by [`zoom_rect`](Self::zoom_rect).
    zoom_rect: Rect,
    /// `TWindow::number`.
    number: i16,
    /// `TWindow::palette` — the colour scheme. See [`WindowPalette`].
    palette: WindowPalette,
    /// `TWindow::title`.
    title: Option<String>,
}

impl Window {
    /// `TWindow::TWindow(bounds, aTitle, aNumber)` + `TWindowInit` — construct
    /// the window.
    ///
    /// Ports the C++ ctor faithfully (`twindow.cpp`):
    /// 1. `TGroup(bounds)`.
    /// 2. `flags = wfMove | wfGrow | wfClose | wfZoom` (all four true).
    /// 3. `zoomRect = getBounds()`.
    /// 4. `palette = wpBlueWindow`.
    /// 5. `state |= sfShadow`; `options |= ofSelectable | ofTopSelect`;
    ///    `growMode = gfGrowAll | gfGrowRel`.
    /// 6. `if( createFrame && (frame = createFrame(getExtent())) ) insert(frame)`.
    ///
    /// **Frame data is pushed down at construction (D3, brief option (a)).** We
    /// build the [`Frame`] **concretely** so we can call its owner-data-down
    /// setters ([`set_title`](Frame::set_title)/[`set_flags`](Frame::set_flags)/
    /// [`set_number`](Frame::set_number)) before boxing + inserting — no
    /// post-insert downcast seam is needed at 33b.
    ///
    /// **A frame is mandatory at 33b:** `frame_id` is non-optional. The C++
    /// `createFrame == 0` (frameless) path is the streamable case with no
    /// consumer here; supporting it would force an `Option<ViewId>` ripple for a
    /// path no caller exercises, so we always build the frame.
    pub fn new(bounds: Rect, title: Option<String>, number: i16) -> Self {
        let mut group = Group::new(bounds);

        // C++: flags = wfMove | wfGrow | wfClose | wfZoom.
        let flags = WindowFlags {
            r#move: true,
            grow: true,
            close: true,
            zoom: true,
        };
        // C++: state |= sfShadow; options |= ofSelectable | ofTopSelect.
        let st = group.state_mut();
        st.state.shadow = true;
        st.options.selectable = true;
        st.options.top_select = true;
        // C++: growMode = gfGrowAll | gfGrowRel.
        st.grow_mode = GrowMode {
            rel: true,
            ..GrowMode::grow_all()
        };

        // C++: zoomRect( getBounds() ).
        let zoom_rect = group.state().get_bounds();
        let extent = group.state().get_extent();

        // NOTE: C++ TWindowInit::createFrame lets a subclass inject a custom
        // TFrame. We build the Frame directly and push owner data into it (here at
        // ctor; the 33c downcast seam reaches it post-insert for `set_zoomed`). A
        // real createFrame/subclass-frame hook has no consumer yet; reintroduce it
        // when a subclass needs a non-default frame.
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

    // -- accessors (keep the D3 owner-data members live) --------------------

    /// `TWindow::frame` — the frame child's id (33c's `zoom` pushes
    /// `set_zoomed` through it).
    pub fn frame_id(&self) -> ViewId {
        self.frame_id
    }

    /// `TWindow::flags` — the decoration flags.
    pub fn flags(&self) -> WindowFlags {
        self.flags
    }

    /// `TWindow::zoomRect` — the saved bounds for un-zoom (consumed by 33c).
    pub fn zoom_rect(&self) -> Rect {
        self.zoom_rect
    }

    // NOTE: `TWindow::number` is exposed via the `View::number()` trait override
    // below (returning `Option<i16>` — `None` for `wnNoNumber == 0`), so Alt-N can
    // query any `&dyn View` for its number. No inherent getter.

    /// `TWindow::palette` — the colour scheme.
    pub fn palette(&self) -> WindowPalette {
        self.palette
    }

    /// `TWindow::getTitle(short)` — returns the title (the C++ ignores its
    /// `maxLength` argument and returns the full title; `frame.rs` documents
    /// this).
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    // -- setters (subclass field overrides, e.g. TDialog) -------------------

    /// Override the decoration flags after construction (`TDialog::TDialog` sets
    /// `flags = wfMove | wfClose`). Re-pushes to the frame child (D3
    /// owner-data-down): the ctor pushes `flags` to the frame once, so a later
    /// change must re-push or the frame would still draw the ctor's zoom/grow
    /// icons. Resolves `frame_id` then downcast then [`Frame::set_flags`], the same
    /// seam `zoom` uses to push `set_zoomed`.
    pub(crate) fn set_flags(&mut self, flags: WindowFlags) {
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

    /// Override the colour scheme after construction (`TDialog::TDialog` sets
    /// `palette = dpGrayDialog`). TODO(row 34 gray theming): the `Gray` scheme is
    /// only recorded here; the frame still renders the blue `Frame*` roles. The
    /// gray/cyan to theme-role mapping is a follow-on cosmetic chunk (see the
    /// `WindowPalette` doc).
    pub(crate) fn set_palette(&mut self, palette: WindowPalette) {
        self.palette = palette;
    }

    /// Override the grow mode after construction (`TDialog::TDialog` sets
    /// `growMode = 0` — a dialog does not track its owner's resize).
    pub(crate) fn set_grow_mode(&mut self, grow_mode: GrowMode) {
        self.group.state_mut().grow_mode = grow_mode;
    }

    /// Insert a child view into the embedded group.
    ///
    /// First production consumer: `THistoryWindow` (row 56), which inserts the
    /// `HistoryViewer` after the scroll bars.  Also used by `Dialog` (row 34)
    /// and the row-63 msgbox. Previously `#[cfg(test)]`; promoted to
    /// `pub(crate)` when `THistoryWindow` became the first non-test caller.
    pub(crate) fn insert_child(&mut self, view: Box<dyn View>) -> ViewId {
        self.group.insert(view)
    }

    /// Reach a direct child of the embedded group by id.
    ///
    /// Used by `THistoryWindow` to run the viewer's post-insert `setup` and to
    /// read `getSelection` via `as_any_mut` + downcast. Mirrors
    /// `Group::child_mut` without exposing the group itself.
    pub(crate) fn child_mut(&mut self, id: ViewId) -> Option<&mut dyn View> {
        self.group.child_mut(id)
    }

    /// Make `id` the current (focused) child of the embedded group, so focused
    /// (keyboard) events route to it. Promoted from a `#[cfg(test)]` hook to a
    /// production seam by row 57: `HistoryWindow` calls it on first-event setup to
    /// establish the popup's internal currency (see the faithfulness note there) —
    /// the localized stand-in for the missing `insertView→show→resetCurrent`
    /// initial-currency seam (the foundational follow-on breadcrumbed at
    /// `Program::exec_view`).
    pub(crate) fn select_child(&mut self, id: ViewId, ctx: &mut Context) {
        self.group
            .set_current(Some(id), crate::view::SelectMode::Normal, ctx);
    }

    // -- standardScrollBar ---------------------------------------------------

    /// `TWindow::standardScrollBar(aOptions)` — insert a standard scroll bar on
    /// the right (vertical) or bottom (horizontal) edge and return its
    /// [`ViewId`] (we have no pointer to return).
    ///
    /// Faithful to the C++:
    /// ```cpp
    /// TRect r = getExtent();
    /// if (aOptions & sbVertical) r = TRect(r.b.x-1, r.a.y+1, r.b.x, r.b.y-1);
    /// else                       r = TRect(r.a.x+2, r.b.y-1, r.b.x-2, r.b.y);
    /// insert(s = new TScrollBar(r));
    /// if (aOptions & sbHandleKeyboard) s->options |= ofPostProcess;
    /// ```
    ///
    /// For `handle_keyboard` we set `ofPostProcess` on the concrete `ScrollBar`
    /// **before** boxing + inserting (the simplest faithful path: `insert`
    /// consumes the box, so we mutate first).
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
        let mut sb = ScrollBar::new(r);
        if opts.handle_keyboard {
            sb.state.options.post_process = true;
        }
        self.group.insert(Box::new(sb))
    }

    // -- zoom (33c) ----------------------------------------------------------

    /// `TWindow::zoom` — toggle between the restored bounds and "filling the
    /// owner". Faithful to `twindow.cpp`:
    /// ```cpp
    /// sizeLimits( minSize, maxSize );      // max = owner->size (virtual)
    /// if( size != maxSize ) { zoomRect = getBounds(); locate(TRect(0,0,max.x,max.y)); }
    /// else                    locate( zoomRect );
    /// ```
    /// `maxSize` (= `owner->size`) is reached via the owner-extent-down channel
    /// ([`Context::owner_size`](crate::view::Context::owner_size), D3) instead of
    /// an up-pointer. The window's own [`size_limits`](View::size_limits) override
    /// (max = owner size, min = 16×6) is used.
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
        // D3: the C++ TFrame::draw recomputes `owner->size == maxSize` every draw
        // to pick the zoom vs unzoom icon. We can't read the owner from the frame,
        // so push the bool down through the downcast seam.
        // TODO(33d): re-push set_zoomed on owner resize / change_bounds (this
        // pushed bool goes stale vs C++'s per-draw recompute).
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

    /// `TView::locate` — clamp `bounds`'s size to [`size_limits`](View::size_limits)
    /// then `change_bounds` iff it differs. `owner_size` feeds the (overridden)
    /// `size_limits`. The C++ `owner != 0` shadow/`drawUnderRect` tail is dropped
    /// (D8: whole-tree redraw + diff).
    fn locate(&mut self, mut bounds: Rect, owner_size: Point) {
        let (min, max) = View::size_limits(self, owner_size);
        bounds.b.x = bounds.a.x + range(bounds.b.x - bounds.a.x, min.x, max.x);
        bounds.b.y = bounds.a.y + range(bounds.b.y - bounds.a.y, min.y, max.y);
        if bounds != self.group.state().get_bounds() {
            // Faithful: TGroup::changeBounds (resizes children by the delta).
            self.group.change_bounds(bounds);
        }
    }

    // -- drag (33d-1) --------------------------------------------------------

    /// Start a drag — the D9 replacement for `dragView`'s nested mouse loop
    /// (`tview.cpp`), reached from [`handle_event`](Self::handle_event) on a
    /// surviving title-bar / corner / middle-button `MouseDown`.
    ///
    /// Sets `sfDragging` **on** directly (it has `&mut self` + `ctx`, so
    /// `Group::set_state` propagates `Dragging` to children incl. the frame,
    /// flipping it to the single-line dragging border), then pushes a deferred
    /// [`DragCapture`]. The capture is pushed deferred, so it sees the *next* event
    /// (the first `MouseMove`), never this `MouseDown` (the `pending_captures`
    /// contract).
    ///
    /// `mouse_local` is the `MouseDown` position in **window-local** coords; adding
    /// the window's own `origin` gives the absolute mouse-down used to compute the
    /// constant grab anchor (the 3a coordinate-frame assumption on [`DragCapture`]).
    /// `owner->getExtent()` / `sizeLimits()` are read **here** (group-routed
    /// dispatch, so `ctx.owner_size()` is valid), never at drag time.
    fn start_drag(&mut self, id: ViewId, kind: DragKind, mouse_local: Point, ctx: &mut Context) {
        // dragView: setState(sfDragging, True). Set it directly (Group::set_state
        // propagates Dragging to children incl. the frame).
        View::set_state(self, StateFlag::Dragging, true, ctx);

        let origin = self.group.state().origin;
        let size = self.group.state().size;
        let mouse_abs = mouse_local + origin; // window-local -> absolute (3a assumption)
        // owner->getExtent() and sizeLimits(), via the owner-extent-down channel +
        // the window's size_limits override. owner_size is valid HERE (group-routed
        // dispatch); the capture must NOT read it at drag time (DragCapture 3a).
        let owner_size = ctx.owner_size();
        let limits = Rect::new(0, 0, owner_size.x, owner_size.y); // owner->getExtent()
        let (min, max) = View::size_limits(self, owner_size);
        // dragMode | (flags & (wfMove|wfGrow)) — only the dmLimit* bits feed
        // move_grow (the wfMove/wfGrow bits select dmDragMove/Grow, already encoded
        // in `kind`), so we carry just the window's dragMode (ctor default
        // dmLimitLoY).
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
}

/// `range(val, min, max)` (tview.cpp) — clamp `val` into `[min, max]`, pinning
/// `min` to `max` if inverted. Reimplemented locally (it is two lines) to keep
/// the `locate` seam contained — the `view.rs` `range` is private there.
fn range(val: i32, min: i32, max: i32) -> i32 {
    let min = if min > max { max } else { min };
    val.clamp(min, max)
}

/// Map a `TWindow::number` to the frame's `Option<u8>` contract: `wnNoNumber`
/// (`== 0`) → `None`; `0 < n` → `Some(value)`, faithful to the frame pushing
/// `owner->number` down — the frame's own `n < 10` draw guard then suppresses
/// any digit `>= 10`. The `Option<u8>` carrier clamps `n > 255` to `255` via
/// `unwrap_or(u8::MAX)`, but that branch is unreachable in practice (TV uses
/// `1..=9`). Negative numbers are out of contract; they map to `None` (treated
/// as "no number").
fn number_to_option(number: i16) -> Option<u8> {
    if number <= 0 {
        None
    } else {
        Some(u8::try_from(number).unwrap_or(u8::MAX))
    }
}

// ---------------------------------------------------------------------------
// Drag — the D9 capture-handler replacement for dragView's mouse loop
// ---------------------------------------------------------------------------

/// Which `dragView` form is running (mouse branch only; the keyboard `cmResize`
/// sub-mode is deferred — see [`Window::handle_event`]'s `TODO(33d-2/later, D9)`).
/// Selects how each `MouseMove` maps to new bounds (see
/// [`DragCapture::compute_bounds`]).
enum DragKind {
    /// `dmDragMove` — translate the whole window (title-bar / middle-button move).
    Move,
    /// `dmDragGrow` — drag the bottom-right corner (origin fixed, size follows).
    Grow,
    /// `dmDragGrowLeft` — drag the bottom-left corner (top-right fixed).
    GrowLeft,
}

/// The D9 replacement for `TView::dragView`'s nested `while(mouseEvent(...))`
/// loop (`tview.cpp`), realized as a [`CaptureHandler`] (D9). Under D3 the *frame*
/// cannot start the drag (it has no pointer to the window it would move); the
/// [`Window`] starts it (it knows its own id and its owner's size) via
/// [`Window::start_drag`], which pushes this handler.
///
/// **Coordinate-frame assumption (mirrors [`ModalFrame`](crate::app::ModalFrame)).**
/// The capture runs at the capture-stack level, *before* any group routing, so it
/// sees mouse events in **absolute screen coordinates**. For row 31 the root
/// `Group` covers the whole screen at `(0,0)` and the desktop is its child at
/// `(0,0)`, so **absolute == root-local == desktop-local**, and a window's
/// `origin` (relative to its owner) is in that same frame — the drag math assumes
/// this. When a menu / status bar (Phase 4) shifts the desktop off `(0,0)`, this
/// capture must translate absolute → desktop coords; revisit then. (The window
/// cannot know the desktop's offset under D3, so we do not attempt it now — the
/// same caveat `ModalFrame` carries.)
struct DragCapture {
    window_id: ViewId,
    kind: DragKind,
    /// Window bounds at drag start (the fixed corner for Grow/GrowLeft).
    init_bounds: Rect,
    /// The constant grab offset (see [`compute_bounds`](Self::compute_bounds));
    /// per-kind meaning documented in [`Window::start_drag`].
    anchor: Point,
    /// `owner->getExtent()` — captured at push time from `ctx.owner_size()`.
    limits: Rect,
    /// Minimum size (`sizeLimits().min`), captured at push time.
    min: Point,
    /// Maximum size (`sizeLimits().max`), captured at push time.
    max: Point,
    /// `owner->dragMode | (flags & ...)` — only the `dmLimit*` bits matter to
    /// [`move_grow`] (the window's default is `dmLimitLoY`).
    mode: DragMode,
}

impl DragCapture {
    /// Map the current `MouseMove`'s absolute position to the window's new bounds,
    /// replicating `dragView`'s three mouse forms (`tview.cpp`). The anchor is the
    /// **constant** grab offset captured at push time (see [`Window::start_drag`]).
    fn compute_bounds(&self, mouse_abs: Point) -> Rect {
        match self.kind {
            // p = origin - mouseDown; origin' = mouse + p.
            DragKind::Move => {
                let sz = self.init_bounds.b - self.init_bounds.a;
                let new_origin = mouse_abs + self.anchor;
                move_grow(new_origin, sz, self.limits, self.min, self.max, self.mode)
            }
            // p = size - mouseDown; size' = mouse + p.
            DragKind::Grow => {
                let o = self.init_bounds.a;
                let new_size = mouse_abs + self.anchor;
                move_grow(o, new_size, self.limits, self.min, self.max, self.mode)
            }
            // dmDragGrowLeft: bespoke pre-clamp of the moving bottom-left corner,
            // then move_grow. The top-right (`b`) is the fixed anchor.
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
                // dragView's loop ends on mouse-up; clear sfDragging (deferred — a
                // capture holds no &mut view) and pop ourselves.
                ctx.request_set_state(self.window_id, StateFlag::Dragging, false);
                CaptureFlow::ConsumedPop
            }
            // C++ `mouseEvent(event, evMouseMove)` discards everything that is not a
            // mouse-move/up while the drag runs — the drag is modal. Swallow the
            // rest (MouseAuto, keys, commands, broadcasts) without moving the window.
            _ => CaptureFlow::Consumed,
        }
    }

    fn view(&self) -> Option<ViewId> {
        Some(self.window_id)
    }
}

/// `TView::moveGrow` (`tview.cpp`) — clamp size to `[min, max]` and origin to the
/// limits, honoring the `dmLimit*` mode bits, and return the resulting bounds.
///
/// We return the rect instead of calling `locate` (the capture has no `&mut`
/// view; the loop applies it via `change_bounds` — equivalent, since `move_grow`
/// already clamps to the same sizeLimits `locate` would).
///
/// C++ uses `min(max(..))` **not** `clamp()`: when `lo > hi` (window larger than
/// the limit), `min(max(v, lo), hi)` yields `hi`. [`i32::clamp`] PANICS on
/// `lo > hi`, so we do **not** use it — replicate min/max exactly.
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
    apply_list_scroll,
    as_any_mut,
    calc_bounds,
    grabs_focus_on_click,
    select_window_num,
    set_value,
    value,
))]
impl View for Window {
    /// `TWindow::handleEvent` — delegate to the group, then handle the window's
    /// own commands + the focus-cycling keys. `TGroup::handleEvent(event)` runs
    /// **first** (faithful order), then:
    ///
    /// * `cmZoom` (if `wfZoom`) → [`zoom`](Self::zoom) + `clearEvent` (33c). The
    ///   C++ `infoPtr == 0 || == this` target guard is **not ported** — it is
    ///   provably vacuous in this architecture, not merely "payloads dropped".
    ///   The frame posts `cmZoom`/`cmClose` with `infoPtr = owner` **only while
    ///   `sfActive`** (`tframe.cpp` 152/171), so the target is always the *active*
    ///   window; `cmZoom`/`cmClose` are *focused* (`Event::Command`) events, which
    ///   the desktop routes solely to its `current` child = the active window; and
    ///   the internal queue drains fully before the next `poll_event`, so the
    ///   active window cannot change between post and dispatch. A `cmZoom`/`cmClose`
    ///   therefore always reaches exactly the window it targets, so
    ///   `infoPtr == 0 || == this` can never reject anything. **Trip-wire:** revisit
    ///   only if a future emitter targets a *non-active* window via a command.
    /// * `cmClose` (if `wfClose`) → `request_close` (the loop drains it into
    ///   `remove_descendant`), or post `cmCancel` if `sfModal` (33d-1). Same vacuous
    ///   target-guard reasoning as `cmZoom` (Phase A). Modal teardown machinery is
    ///   row 34's; only the one `sfModal → cmCancel` branch is wired here.
    /// * `kbTab` → `focusNext(False)` (forwards) + `clearEvent`.
    /// * `kbShiftTab` → `focusNext(True)` (backwards) + `clearEvent`. Shift+Tab
    ///   is `Key::Tab` + the `shift` modifier (there is no `Key::BackTab`).
    /// * A surviving title-bar / bottom-corner / middle-button `MouseDown` →
    ///   [`start_drag`](Self::start_drag) (the D9 [`DragCapture`] replacement for
    ///   `TFrame::dragWindow` → `dragView`'s mouse loop, 33d-1).
    ///
    /// Deferred C++ command/broadcast cases, each needing infrastructure not yet
    /// built:
    /// * `cmResize` → `dragView(dragMode | (flags & (wfMove|wfGrow)), limits, …)`
    ///   — **TODO(33d-2/later, D9):** the keyboard resize sub-mode (arrows until
    ///   Enter/Esc, `dragView`'s `else` branch). No menu can trigger `cmResize` yet,
    ///   so a handler would be unreachable; per 33c's principle we must not *enable*
    ///   a command we do not handle, so `cmResize` is omitted from the `set_state`
    ///   enable set.
    /// * `cmSelectWindowNum` matching `number` → `select()` — **realized at 33d-2
    ///   as a direct walk, NOT on the window.** The window number is an *integer*
    ///   argument (not a `ViewId`), so the `Broadcast` `source` substrate does not
    ///   serve it; instead `program_handle_event` asks the desktop
    ///   ([`Desktop::select_window_num`](crate::desktop::Desktop)) to select the
    ///   child whose [`number`](View::number) matches
    ///   ([`Group::focus_by_number`](crate::view::Group)). So the window has no
    ///   `cmSelectWindowNum` arm of its own.
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
        // `cmClose` — faithful to `TWindow::handleEvent`'s cmClose case +
        // `TWindow::close`. **No target guard** (`infoPtr == 0 || == this`): Phase A
        // proved it vacuous — the frame posts `cmClose` only while `sfActive`, and
        // `Event::Command` is focused-routed to the desktop's `current` (= active)
        // window, so a `cmClose` always reaches exactly its target (the same
        // trip-wire `cmZoom` documents above; revisit only if a future emitter
        // targets a non-active window via a command). Modal teardown (the
        // `sfModal → cmCancel` branch) is wired here but row 34 owns the machinery.
        if let Event::Command(c) = *ev
            && c == Command::CLOSE
            && self.flags.close
        {
            ev.clear(); // C++ clears first.
            if self.group.state().state.modal {
                // sfModal: re-issue as cmCancel (row 34 owns modal teardown).
                ctx.post(Command::CANCEL);
            } else if self.valid(Command::CLOSE) {
                // close(): if valid(cmClose). The loop drains the request and runs
                // `remove_descendant` (the close-removal channel, replacing the old
                // "needs a close-removal channel" breadcrumb).
                if let Some(id) = self.group.state().id() {
                    ctx.request_close(id);
                }
            }
        }
        if let Event::KeyDown(k) = *ev
            && k.key == Key::Tab
        {
            // C++ kbTab → focusNext(False); kbShiftTab → focusNext(True).
            self.group.focus_next(k.modifiers.shift, ctx);
            ev.clear();
        }
        // Drag detection — the D9 replacement for `TFrame::dragWindow` →
        // `dragView`'s mouse loop. Runs AFTER group delegation: the desktop
        // delivered this `MouseDown` to the window in window-local coords; the
        // group routed it positionally to the frame, which leaves a title-bar /
        // bottom-corner click UNCONSUMED (a close/zoom-icon click → Nothing, an
        // interior-child click consumed there). So a still-live `MouseDown` here is
        // a drag spot, its position window-local. An inactive window never reaches
        // here on its first click (the desktop's positional auto-select consumes
        // the selecting click), so the drag only ever starts on the active window —
        // no `sfActive` re-check needed (faithful). The geometry replicates
        // `TFrame::handleEvent`.
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
                // the title/corner cases; C++ orders it last, the branches do not
                // overlap so any order is equivalent).
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

    /// `TWindow::setState` — for 33b: the **activation** half only.
    ///
    /// C++:
    /// ```cpp
    /// TGroup::setState(aState, enable);
    /// if (aState & sfSelected) {
    ///     setState(sfActive, enable);          // self-recursion
    ///     if (frame) frame->setState(sfActive, enable);
    ///     // ...build + enable/disable windowCommands...
    /// }
    /// ```
    /// We delegate to `Group::set_state` (flips the flag + propagates to
    /// children), then — iff `Selected` — call `Group::set_state(Active)`. That
    /// `Active` propagation flips **every** child (incl. the frame) active /
    /// passive, so the explicit C++ `frame->setState(sfActive)` is redundant
    /// here (as `frame.rs` notes) — we do NOT push the frame manually.
    ///
    /// **DIVERGENCE from C++ (the spec reviewer will check this):** C++
    /// `TWindow::setState` enables the **full set** `{cmNext, cmPrev, cmResize if
    /// (grow|move), cmClose if close, cmZoom if zoom}` atomically on `sfSelected`.
    /// We enable every member **whose handler exists** ("enable only commands whose
    /// handlers exist" staging; enabling an inert command — routed to a window that
    /// ignores it, or filtered — is a worse state than leaving it disabled):
    ///   cmNext, cmPrev: UNCONDITIONAL (handler in `TDeskTop::handleEvent`, 33d-2).
    ///   33c: cmZoom (if `wfZoom`; handler in [`handle_event`](Self::handle_event)).
    ///   33d-1: cmClose (if `wfClose`; handler in `handle_event`).
    ///   DROPPED (no handler): cmResize (the keyboard resize sub-mode).
    fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
        self.group.set_state(flag, enable, ctx);
        if flag == StateFlag::Selected {
            self.group.set_state(StateFlag::Active, enable, ctx);
            // Window commands enabled/disabled together while selected (C++
            // enableCommands).
            //
            // cmNext/cmPrev are UNCONDITIONAL (C++ `windowCommands += cmNext; +=
            // cmPrev;` has NO flag guard — every selectable window can be cycled),
            // so they do NOT go through the flag-gated `toggle` closure. Their
            // handler is `TDeskTop::handleEvent` (33d-2).
            if enable {
                ctx.enable_command(Command::NEXT);
                ctx.enable_command(Command::PREV);
            } else {
                ctx.disable_command(Command::NEXT);
                ctx.disable_command(Command::PREV);
            }
            // The flag-gated subset: cmClose (if wfClose), cmZoom (if wfZoom) —
            // both handled in handle_event. cmResize stays DROPPED (no keyboard
            // resize handler yet — the `TODO(33d-2/later, D9)` in handle_event).
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
        }
    }

    /// `TWindow::sizeLimits` — `TView::sizeLimits(min, max)` then `min =
    /// minWinSize {16, 6}`. We take the group's `(_, max)` and force the minimum.
    fn size_limits(&self, owner_size: Point) -> (Point, Point) {
        let (_min, max) = self.group.size_limits(owner_size);
        (Point::new(16, 6), max)
    }

    // NOTE: `calc_bounds` is in the skip list above — NOT forwarded to the group.
    // The trait default routes through `Window::size_limits` (this override's
    // 16×6 floor) and mutates the group's `ViewState` via `state_mut()` —
    // faithful to C++ `TView::calcBounds` calling the *virtual* `sizeLimits`
    // (i.e. `TWindow::sizeLimits`). Forwarding to `self.group.calc_bounds` would
    // use the group's `size_limits` (min 0×0) and silently bypass the window's
    // minimum on an owner-driven resize.

    /// `TWindow::number` — the window number, or `None` for `wnNoNumber` (`0`). A
    /// window numbered `0` is never an Alt-N (`cmSelectWindowNum`) target.
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

    /// A minimal selectable probe view (the frame is not selectable, so kbTab
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

    /// The frame received the pushed-down title / flags / number at construction
    /// (D3 owner-data-down at ctor).
    #[test]
    fn new_pushes_frame_data_down() {
        let mut w = window_with_frame();
        let idx = w.group.index_of_pub(w.frame_id()).unwrap();
        // Render the (active) frame and read its title back off row 0.
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

    // -- 2. getTitle / sizeLimits --------------------------------------------

    #[test]
    fn title_and_size_limits() {
        let w = window_with_frame();
        assert_eq!(w.title(), Some("Edit"));
        // min forced to minWinSize {16, 6}; max is the owner size.
        let (min, max) = w.size_limits(Point::new(80, 25));
        assert_eq!(min, Point::new(16, 6), "minWinSize");
        assert_eq!(max, Point::new(80, 25), "max is the owner size");
    }

    /// The 33b correctness blind spot: an owner-driven resize must honour the
    /// window's 16×6 floor (because `calc_bounds` routes through the *window's*
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

    // -- 3. setState activation flips the frame active -----------------------

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

    // -- 5. kbTab focus cycling ----------------------------------------------

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

        // kbTab (forwards) moves current to the next selectable child + consumes.
        // Children in insert order: [frame, id_a, id_b]; current == id_b. Forward
        // tab order is decreasing Vec index with wrap (see `Group::find_next`), so
        // from id_b (idx 2) the next eligible child is id_a (idx 1) — the frame
        // (idx 0) is not selectable. Focus lands deterministically on id_a.
        let mut ev = tab_event(false);
        with_ctx(&mut out, &mut timers, |ctx| w.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "kbTab consumed");
        assert_eq!(
            w.group.current(),
            Some(id_a),
            "forward tab moves focus from id_b to id_a"
        );

        // kbShiftTab (backwards) also consumes.
        let mut ev2 = tab_event(true);
        with_ctx(&mut out, &mut timers, |ctx| w.handle_event(&mut ev2, ctx));
        assert!(ev2.is_nothing(), "kbShiftTab consumed");
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

    // -- 8. setState enables/disables cmZoom + cmClose (33a channel) ----------

    /// Selecting a `wfZoom`/`wfClose` window queues `(Command::ZOOM, true)` and
    /// `(Command::CLOSE, true)` on the command-change channel; deselecting queues
    /// the matching `false` pairs (33d-1 added cmClose alongside cmZoom).
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

    /// Read the frame child's pushed `zoomed` flag through the 33c seam.
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
        // a cmZoom directly. owner_size is set on the ctx (the desktop's job): the
        // group.handle_event inside Window restores owner_size to this value, so by
        // the time the cmZoom arm runs zoom(), owner_size == desktop_size.
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

    // -- 11. drag-start detection (33d-1, unit; no pump) ----------------------

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

    /// A bottom-right corner `MouseDown` (`wfGrow`) starts a Grow; a bottom-left
    /// corner starts a GrowLeft; both consume + queue one capture.
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

    /// A middle-button interior `MouseDown` (`wfMove`) starts a Move drag.
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
    /// dmLimitHi bits, lands where requested (after the general [a-s+1, b-1] band,
    /// which a centrally-placed window is well inside).
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

    // -- View::number override (33d-2) ---------------------------------------

    #[test]
    fn view_number_some_when_positive_none_when_zero() {
        // Positive number -> Some(n).
        let w = Window::new(Rect::new(0, 0, 20, 6), Some("A".into()), 4);
        assert_eq!(View::number(&w), Some(4), "number > 0 -> Some");
        // wnNoNumber (0) -> None (never an Alt-N target).
        let w0 = Window::new(Rect::new(0, 0, 20, 6), Some("B".into()), 0);
        assert_eq!(View::number(&w0), None, "number == 0 (wnNoNumber) -> None");
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
}
