//! `TWindow` core — see the [module docs](super) for the deviation summary.

use crate::command::Command;
use crate::event::{Event, Key};
use crate::frame::Frame;
use crate::view::{
    Context, DrawCtx, Group, GrowMode, Point, Rect, StateFlag, View, ViewId, ViewState,
};
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

    /// `TWindow::number` — the window number.
    pub fn number(&self) -> i16 {
        self.number
    }

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

impl View for Window {
    fn state(&self) -> &ViewState {
        self.group.state()
    }

    fn state_mut(&mut self) -> &mut ViewState {
        self.group.state_mut()
    }

    /// `TWindow` does not override `draw`; it inherits `TGroup::drawSubViews`.
    /// The frame is the back-most child (drawn first), interior children draw
    /// over it. Shadow casting is still deferred (the `group.rs` `// TODO(row
    /// 33)`).
    fn draw(&mut self, ctx: &mut DrawCtx) {
        self.group.draw(ctx);
    }

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
    /// * `kbTab` → `focusNext(False)` (forwards) + `clearEvent`.
    /// * `kbShiftTab` → `focusNext(True)` (backwards) + `clearEvent`. Shift+Tab
    ///   is `Key::Tab` + the `shift` modifier (there is no `Key::BackTab`).
    ///
    /// Deferred C++ command/broadcast cases, each needing infrastructure not yet
    /// built:
    /// * `cmResize` → `dragView(dragMode | (flags & (wfMove|wfGrow)), limits, …)`
    ///   — **TODO(33d):** needs a transient drag capture handler (limits captured
    ///   at push time from `ctx.owner_size()`).
    /// * `cmClose` → `close()` (or post `cmCancel` if `sfModal`) — **TODO(33d):**
    ///   needs a close-removal channel; the modal path is **row 34**.
    /// * `cmSelectWindowNum` matching `number` → `select()` — **deferred to 33d.**
    ///   The blocker is the missing select machinery (`select()`/`canMoveFocus`),
    ///   not a payload story: the window number is an *integer* argument (not a
    ///   `ViewId`), so the `Broadcast` `source` substrate does not serve it. Alt-N
    ///   is realized at 33d as a **direct walk** — the program asks the desktop to
    ///   select the child whose `number` matches — not a payload-carrying broadcast
    ///   (the Alt-N deferral already noted in `program.rs`).
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
        if let Event::KeyDown(k) = *ev
            && k.key == Key::Tab
        {
            // C++ kbTab → focusNext(False); kbShiftTab → focusNext(True).
            self.group.focus_next(k.modifiers.shift, ctx);
            ev.clear();
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
    /// 33c enables **only `cmZoom`** — the one command whose handler now exists.
    /// Enabling a command whose handler is absent would be an *inert* command (the
    /// pump would route it to a window that ignores it, or filter it) — a worse
    /// state than leaving it disabled. This is the documented "enable only commands
    /// whose handlers exist" staging; the rest land in 33d / row 34 with their
    /// handlers.
    ///   33c: cmZoom (handler in [`handle_event`](Self::handle_event)).
    ///   TODO(33d): cmResize (if grow|move), cmClose (if close), cmNext, cmPrev.
    fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
        self.group.set_state(flag, enable, ctx);
        if flag == StateFlag::Selected {
            self.group.set_state(StateFlag::Active, enable, ctx);
            if self.flags.zoom {
                if enable {
                    ctx.enable_command(Command::ZOOM);
                } else {
                    ctx.disable_command(Command::ZOOM);
                }
            }
        }
    }

    fn valid(&self, cmd: Command) -> bool {
        self.group.valid(cmd)
    }

    fn awaken(&mut self) {
        self.group.awaken();
    }

    /// `TWindow::sizeLimits` — `TView::sizeLimits(min, max)` then `min =
    /// minWinSize {16, 6}`. We take the group's `(_, max)` and force the minimum.
    fn size_limits(&self, owner_size: Point) -> (Point, Point) {
        let (_min, max) = self.group.size_limits(owner_size);
        (Point::new(16, 6), max)
    }

    // NOTE: `calc_bounds` is deliberately NOT overridden and NOT delegated to the
    // group. The trait default routes through `Window::size_limits` (this
    // override's 16×6 floor) and mutates the group's `ViewState` via
    // `state_mut()` — faithful to C++ `TView::calcBounds` calling the *virtual*
    // `sizeLimits` (i.e. `TWindow::sizeLimits`). Delegating to
    // `self.group.calc_bounds` would use the group's `size_limits` (min 0×0) and
    // silently bypass the window's minimum on an owner-driven resize.

    fn change_bounds(&mut self, bounds: Rect) {
        self.group.change_bounds(bounds);
    }

    fn cursor_request(&self) -> Option<Point> {
        self.group.cursor_request()
    }

    /// Delegate the D3 tree-walk into the embedded group (as `draw`/`handle_event`
    /// already delegate), so a `find_mut` from above descends through the window.
    fn find_mut(&mut self, id: ViewId) -> Option<&mut dyn View> {
        self.group.find_mut(id)
    }

    /// Delegate descendant removal into the embedded group (the owning group runs
    /// the faithful removal + `reset_current`).
    fn remove_descendant(&mut self, id: ViewId, ctx: &mut Context) -> bool {
        self.group.remove_descendant(id, ctx)
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
    use std::collections::VecDeque;

    // -- test harness --------------------------------------------------------

    fn with_ctx<R>(
        out: &mut VecDeque<Event>,
        timers: &mut TimerQueue,
        f: impl FnOnce(&mut Context) -> R,
    ) -> R {
        let mut pending: Vec<Box<dyn crate::capture::CaptureHandler>> = Vec::new();
        let mut cmd_changes: Vec<(Command, bool)> = Vec::new();
        let mut ctx = Context::new(out, timers, 0, &mut pending, &mut cmd_changes);
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
        // number stored.
        assert_eq!(w.number(), 3);
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

    // -- 8. setState enables/disables cmZoom (33a channel) --------------------

    /// Selecting a `wfZoom` window queues `(Command::ZOOM, true)` on the
    /// command-change channel; deselecting queues `(Command::ZOOM, false)`.
    #[test]
    fn set_state_select_enables_and_disables_cm_zoom() {
        let mut w = window_with_frame(); // flags.zoom = true
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut pending: Vec<Box<dyn crate::capture::CaptureHandler>> = Vec::new();
        let mut cmd_changes: Vec<(Command, bool)> = Vec::new();

        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut pending, &mut cmd_changes);
            View::set_state(&mut w, StateFlag::Selected, true, &mut ctx);
        }
        assert!(
            cmd_changes.contains(&(Command::ZOOM, true)),
            "select enables cmZoom"
        );

        cmd_changes.clear();
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut pending, &mut cmd_changes);
            View::set_state(&mut w, StateFlag::Selected, false, &mut ctx);
        }
        assert!(
            cmd_changes.contains(&(Command::ZOOM, false)),
            "deselect disables cmZoom"
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
        let mut pending: Vec<Box<dyn crate::capture::CaptureHandler>> = Vec::new();
        let mut cmd_changes: Vec<(Command, bool)> = Vec::new();

        // Select the window (realism: the desktop would have it selected) and feed
        // a cmZoom directly. owner_size is set on the ctx (the desktop's job): the
        // group.handle_event inside Window restores owner_size to this value, so by
        // the time the cmZoom arm runs zoom(), owner_size == desktop_size.
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut pending, &mut cmd_changes);
            View::set_state(&mut w, StateFlag::Selected, true, &mut ctx);
        }

        // First zoom: fill the owner.
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut pending, &mut cmd_changes);
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
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut pending, &mut cmd_changes);
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
        let mut pending: Vec<Box<dyn crate::capture::CaptureHandler>> = Vec::new();
        let mut cmd_changes: Vec<(Command, bool)> = Vec::new();
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut pending, &mut cmd_changes);
            View::set_state(&mut w, StateFlag::Selected, true, &mut ctx);
        }

        // Restored snapshot.
        insta::assert_snapshot!("zoom_restored", render_window(&mut w, &theme, 24, 8));

        // Zoom to fill the screen.
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut pending, &mut cmd_changes);
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
}
