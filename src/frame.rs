//! The decorative border [`Frame`] a [`Window`](crate::window::Window) places
//! around its interior: the box, the centered title, the optional window
//! number, and the close / zoom / resize icons. It is a passive view — it
//! never takes focus.
//!
//! # How a frame gets its data
//!
//! A frame never reaches up into the window that owns it. Instead, [`Frame`]
//! holds its **own copy** of exactly the data it renders, pushed **down** by the
//! owning window through public setters:
//!
//! * [`title`](Frame::title) — the window title
//! * [`flags`](Frame::flags) — the window decoration flags ([`WindowFlags`])
//! * [`number`](Frame::number) — the window number (none → not drawn)
//! * [`zoomed`](Frame::zoomed) — whether the window is maximized (drives the
//!   zoom-vs-unzoom icon)
//! * [`palette`](Frame::palette) — the owner's colour scheme
//!   ([`WindowPalette`](crate::window::WindowPalette)), which selects the border
//!   and icon roles: `Blue` → [`Role::FrameActive`] / [`Role::FramePassive`] /
//!   [`Role::FrameDragging`] / [`Role::FrameIcon`]; `Cyan` → the
//!   `Role::FrameCyan*` family; `Gray` (dialogs) → the `Role::FrameGray*`
//!   family.
//!
//! The window calls these setters whenever its own state changes. The frame's
//! active / dragging state instead arrives through the normal
//! [`View::set_state`] propagation that [`Group`](crate::view::Group) drives
//! onto every child, so [`Frame::draw`] reads it from its own state directly.
//!
//! # Drag, resize, and close
//!
//! The frame deliberately leaves window-drag `MouseDown` events (title-bar move,
//! bottom-corner resize, middle-button move) unconsumed so they fall through to
//! the owning [`Window`](crate::window::Window), which runs the drag. The close
//! icon, by contrast, is handled here: pressing it arms a press-and-hold
//! [`MouseTrackCapture`](crate::capture::MouseTrackCapture) and posts
//! [`Command::CLOSE`](crate::Command::CLOSE) only if the button is released back
//! over the close zone.
//!
//! **Guide:** [Windows & the desktop](../../../apps/windows.html).
//!
//! # Turbo Vision heritage
//!
//! Ports `TFrame` (`tframe.cpp` / `framelin.cpp`). The original frame's draw and
//! event handling reached up through the owner into the window; tvision-rs instead
//! pushes the needed data down from the window (deviation D3). Inheritance becomes
//! the [`View`] trait plus [`ViewState`] composition (deviation D2), the frame
//! flag word becomes [`WindowFlags`] (deviation D5), and palette lookups become
//! [`Role`]-keyed theme styles (deviation D7).
//!
//! The classic sibling tee-walk (`TFrame::frameLine` reaching sideways to read
//! its siblings' bounds) is **not** reproduced as a sideways walk — deviation D3
//! forbids a child reaching its siblings. Instead, a window that hosts a joined
//! [`Splitter`](crate::widgets::Splitter) (opted in via
//! [`Splitter::joined`](crate::widgets::Splitter::joined)) auto-brokers: it
//! computes the divider abutments from its `Splitter` child and pushes them
//! **down** to this frame as
//! [`JunctionMark`](crate::junction::JunctionMark)s via
//! [`set_junction_marks`](Frame::set_junction_marks); the frame then substitutes
//! the matching tee glyph at each marked edge cell — the same visual result as
//! `frameLine`, fed by pushed data. A frame with no marks (every window without a
//! joined splitter) draws plain corners and edges, exactly as before.

use crate::capture::TrackMask;
use crate::command::Command;
use crate::event::Event;
use crate::junction::{Edge, JunctionMark, Weight, frame_junction};
use crate::theme::Role;
use crate::view::{Context, DrawCtx, GrowMode, Point, Rect, View, ViewState};
use crate::window::{WindowFlags, WindowPalette};

// ---------------------------------------------------------------------------
// Frame
// ---------------------------------------------------------------------------

/// A window's decorative border: box, title, number, and close/zoom/resize
/// icons.
///
/// Embeds `st: ViewState` and `impl View`, draws through [`DrawCtx`], and
/// handles events through [`Context`]. The window-owned data it renders
/// (title / flags / number / zoomed) is pushed down by the owning
/// [`Window`](crate::window::Window) through the public setters; see the module
/// docs.
pub struct Frame {
    /// View state (geometry, flags, etc.).
    pub st: ViewState,
    /// The window title, pushed down by the owning window.
    title: Option<String>,
    /// The window decoration flags, pushed down by the owning window.
    flags: WindowFlags,
    /// The window number (none → not drawn); drawn only if `Some(n)` and
    /// `n < 10`. Pushed down by the owning window.
    number: Option<u8>,
    /// Whether the window is maximized — drives the zoom-vs-unzoom icon choice.
    /// Pushed down by the owning window.
    zoomed: bool,
    /// The owner's colour scheme, pushed down by the owning window. Selects the
    /// `Role::Frame*` vs `Role::FrameGray*` family in [`draw`](View::draw).
    palette: WindowPalette,
    /// Absolute screen position of view-local `(0, 0)`, cached each `draw` so
    /// the close-icon mouse-tracking capture can convert absolute mouse coords
    /// to view-local.
    abs_origin: Point,
    /// Whether the close-icon press-and-hold track is in flight. Guarded
    /// against stray `MouseUp` events.
    close_pressed: bool,
    /// Divider abutment marks pushed down by the owning window each draw
    /// (owner-data-down). Empty = today's plain frame, so non-joined windows are
    /// byte-for-byte unchanged. See [`set_junction_marks`](Frame::set_junction_marks).
    junction_marks: Vec<JunctionMark>,
    /// Whether to draw the border box, title, and icons. `false` for a borderless
    /// window — the interior background fill stays unconditional, but every
    /// edge/title/icon is suppressed. Pushed down by the owning window like
    /// [`zoomed`]. See [`set_border_visible`](Frame::set_border_visible).
    border_visible: bool,
}

impl Frame {
    /// Construct a frame sized to `bounds`.
    ///
    /// Normally called only by [`Window::new`](crate::window::Window::new), which
    /// creates the frame, inserts it as its first child, and then pushes title /
    /// flags / number / palette down through the owner-data setters. App code that
    /// subclasses `Window` (by wrapping it and overriding behaviour) does not
    /// construct a `Frame` directly — the window handles that internally.
    ///
    /// The grow mode is set so the frame stretches with its owner on the right and
    /// bottom edges (`hi_x` + `hi_y`). The frame is **not** selectable (it never
    /// takes focus). Broadcast and mouse-up events are delivered unconditionally.
    pub fn new(bounds: Rect) -> Self {
        let mut st = ViewState::new(bounds);
        st.grow_mode = GrowMode {
            hi_x: true,
            hi_y: true,
            ..Default::default()
        };
        Frame {
            st,
            title: None,
            flags: WindowFlags::default(),
            number: None,
            zoomed: false,
            palette: WindowPalette::Blue,
            abs_origin: Point::new(0, 0),
            close_pressed: false,
            junction_marks: Vec::new(),
            border_visible: true,
        }
    }

    // -- Owner-data-down setters / getters -----------------------------------

    /// Set the window title (pushed down by the owning window).
    pub fn set_title(&mut self, title: Option<String>) {
        self.title = title;
    }

    /// The window title.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Set the window decoration flags (pushed down by the owning window).
    pub fn set_flags(&mut self, flags: WindowFlags) {
        self.flags = flags;
    }

    /// The window decoration flags.
    pub fn flags(&self) -> WindowFlags {
        self.flags
    }

    /// Set the window number (pushed down by the owning window; none → not drawn).
    pub fn set_number(&mut self, number: Option<u8>) {
        self.number = number;
    }

    /// The window number.
    pub fn number(&self) -> Option<u8> {
        self.number
    }

    /// Set whether the window is maximized (drives the unZoom-vs-zoom icon).
    pub fn set_zoomed(&mut self, zoomed: bool) {
        self.zoomed = zoomed;
    }

    /// Whether the window is maximized.
    pub fn zoomed(&self) -> bool {
        self.zoomed
    }

    /// Show or hide the border box, title, and icons (the interior fill is
    /// unaffected). The owning [`Window`](crate::window::Window) pushes this from
    /// its independent `set_bordered` primitive, via the same `child_mut` +
    /// downcast seam as [`set_zoomed`](Frame::set_zoomed).
    pub(crate) fn set_border_visible(&mut self, v: bool) {
        self.border_visible = v;
    }

    /// Set the owner's colour scheme (pushed down by the owning window).
    /// [`draw`](View::draw) selects the matching role family:
    /// `Blue` → `Role::Frame*`, `Cyan` → `Role::FrameCyan*`,
    /// `Gray` → `Role::FrameGray*`.
    pub(crate) fn set_palette(&mut self, palette: WindowPalette) {
        self.palette = palette;
    }

    /// The owner's colour scheme, which selects the border-role family used
    /// during [`draw`](View::draw).
    ///
    /// `Blue` selects `Role::Frame*` (classic window), `Cyan` selects
    /// `Role::FrameCyan*`, and `Gray` selects `Role::FrameGray*` (dialogs).
    /// Reading this value is rarely needed by callers; it is mainly a getter
    /// companion for the `pub(crate)` `set_palette` that the owning window calls
    /// when it changes colour scheme.
    pub fn palette(&self) -> WindowPalette {
        self.palette
    }

    /// Owner-data-down: the owning window pushes the divider abutment marks the
    /// frame should join into its border. Replaced each draw; empty = a plain
    /// frame (non-joined windows unchanged). Faithful re-expression of TV's
    /// `frameLine` tee-walk, fed by pushed data instead of a sideways sibling walk.
    ///
    /// Called by `Window::draw` when the window opts into joined lines.
    pub(crate) fn set_junction_marks(&mut self, marks: Vec<JunctionMark>) {
        self.junction_marks = marks;
    }

    /// The junction glyph to substitute at an **interior** border cell on `edge`
    /// at `offset`, if a mark lands there; `None` to keep the plain edge glyph.
    /// `bar` is the frame's own weight. Callers only invoke this on interior edge
    /// cells, so corner offsets never reach here — the corner guard is structural
    /// (the draw loops skip the corners).
    fn junction_at(
        &self,
        edge: Edge,
        offset: i32,
        bar: Weight,
        g: &crate::theme::Glyphs,
    ) -> Option<char> {
        self.junction_marks
            .iter()
            .find(|m| m.edge == edge && m.offset == offset)
            .map(|m| frame_junction(edge, bar, m.stem, g))
    }
}

impl View for Frame {
    fn state(&self) -> &ViewState {
        &self.st
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.st
    }

    /// Paint the border, title, number and icons.
    ///
    /// State selection checks `dragging` first, then `!active`, else `active`; the
    /// role family follows the owner's [`WindowPalette`] (blue family shown; `Gray`
    /// substitutes `FrameGray*`):
    ///
    /// | state    | border role     | line set    |
    /// |----------|-----------------|-------------|
    /// | dragging | `FrameDragging` | single      |
    /// | passive  | `FramePassive`  | single      |
    /// | active   | `FrameActive`   | double      |
    ///
    /// We draw the box fully first, then overlay the number / title / icons onto
    /// the top row (and the resize icons onto the bottom row).
    ///
    /// Marked interior edge cells get a junction tee (owner-data-down via
    /// `set_junction_marks`); with no marks the corners/edges are plain. See the
    /// module docs.
    fn draw(&mut self, ctx: &mut DrawCtx) {
        // Cache the absolute origin for the close-icon mouse-tracking capture:
        // the MouseTrackCapture converts absolute mouse coords to view-local
        // via this value.
        self.abs_origin = ctx.origin();

        let glyphs = *ctx.glyphs();
        let w = self.st.size.x;
        let h = self.st.size.y;
        if w <= 0 || h <= 0 {
            return;
        }

        // -- Palette → role family (selected by the owner's window palette:
        // blue window vs cyan window vs gray dialog).
        let (r_dragging, r_passive, r_active, r_icon) = match self.palette {
            WindowPalette::Blue => (
                Role::FrameDragging,
                Role::FramePassive,
                Role::FrameActive,
                Role::FrameIcon,
            ),
            WindowPalette::Cyan => (
                Role::FrameCyanDragging,
                Role::FrameCyanPassive,
                Role::FrameCyanActive,
                Role::FrameCyanIcon,
            ),
            WindowPalette::Gray => (
                Role::FrameGrayDragging,
                Role::FrameGrayPassive,
                Role::FrameGrayActive,
                Role::FrameGrayIcon,
            ),
        };

        // -- State → (border role, double-line) -- check dragging first.
        let (border_role, double) = if self.st.state.dragging {
            (r_dragging, false)
        } else if !self.st.state.active {
            (r_passive, false)
        } else {
            (r_active, true)
        };
        let bar = if double {
            Weight::Double
        } else {
            Weight::Single
        };
        let border = ctx.style(border_role);
        let icon = ctx.style(r_icon);

        // Pick the single- or double-line box glyphs.
        let (tl, tr, bl, br, h_edge, v_edge) = if double {
            (
                glyphs.frame_tl_d,
                glyphs.frame_tr_d,
                glyphs.frame_bl_d,
                glyphs.frame_br_d,
                glyphs.frame_h_d,
                glyphs.frame_v_d,
            )
        } else {
            (
                glyphs.frame_tl,
                glyphs.frame_tr,
                glyphs.frame_bl,
                glyphs.frame_br,
                glyphs.frame_h,
                glyphs.frame_v,
            )
        };

        // -- 1a. Interior background fill (UNCONDITIONAL — a frameless window still
        // paints its body; content overdraws this). Same `border` role that paints
        // a bordered window's interior.
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                ctx.put_char(x, y, ' ', border);
            }
        }

        // -- 1b..6. Border box, title, number, and icons — only when bordered.
        if self.border_visible {
            // Top row: tl, ─ (or a tee at a marked cell) across the interior, tr.
            ctx.put_char(0, 0, tl, border);
            for x in 1..w - 1 {
                let ch = self
                    .junction_at(Edge::Top, x, bar, &glyphs)
                    .unwrap_or(h_edge);
                ctx.put_char(x, 0, ch, border);
            }
            if w >= 2 {
                ctx.put_char(w - 1, 0, tr, border);
            }
            // Middle-row left/right edges (interior already filled above).
            for y in 1..h - 1 {
                let lch = self
                    .junction_at(Edge::Left, y, bar, &glyphs)
                    .unwrap_or(v_edge);
                ctx.put_char(0, y, lch, border);
                if w >= 2 {
                    let rch = self
                        .junction_at(Edge::Right, y, bar, &glyphs)
                        .unwrap_or(v_edge);
                    ctx.put_char(w - 1, y, rch, border);
                }
            }
            // Bottom row.
            if h >= 2 {
                ctx.put_char(0, h - 1, bl, border);
                for x in 1..w - 1 {
                    let ch = self
                        .junction_at(Edge::Bottom, x, bar, &glyphs)
                        .unwrap_or(h_edge);
                    ctx.put_char(x, h - 1, ch, border);
                }
                if w >= 2 {
                    ctx.put_char(w - 1, h - 1, br, border);
                }
            }

            // -- 2. Title budget (dropped). ------------------------------------
            // The original computed a width budget to pass to its title getter, but the
            // base window's getter ignores that argument and returns the full title;
            // the drawn width is then recomputed as min(title width, width-10). So the
            // budget never caps the drawn title for a base window (a subclass could
            // abbreviate). It is therefore dead here and not computed; we cap the
            // *drawn* title to `width - 10` in step 4.

            // Window number (top row).
            if let Some(n) = self.number
                && n < 10
            {
                let i = if self.flags.zoom { 7 } else { 3 };
                if let Some(digit) = char::from_digit(u32::from(n), 10) {
                    ctx.put_char(w - i, 0, digit, border);
                }
            }

            // Title (top row), centered.
            if let Some(title) = &self.title {
                let cap = w - 10;
                let (end, lw) = crate::text::scroll(title, cap, false);
                let lw = lw as i32;
                let truncated = &title[..end];
                let i = (w - lw) >> 1;
                ctx.put_char(i - 1, 0, ' ', border);
                ctx.put_str(i, 0, truncated, border);
                ctx.put_char(i + lw, 0, ' ', border);
            }

            // Active-only icons (top row).
            if self.st.state.active {
                if self.flags.close {
                    ctx.put_cstr(2, 0, glyphs.close_icon, border, icon);
                }
                if self.flags.zoom {
                    let zi = if self.zoomed {
                        glyphs.unzoom_icon
                    } else {
                        glyphs.zoom_icon
                    };
                    ctx.put_cstr(w - 5, 0, zi, border, icon);
                }
            }

            // Active + grow resize icons (bottom row).
            if self.st.state.active && self.flags.grow && h >= 2 {
                ctx.put_cstr(0, h - 1, glyphs.drag_left_icon, border, icon);
                ctx.put_cstr(w - 2, h - 1, glyphs.drag_icon, border, icon);
            }
        }
    }

    /// Handle the close/zoom icon clicks; the window drag loops live in the
    /// owning [`Window`](crate::window::Window).
    ///
    /// The mouse position delivered by [`Group`](crate::view::Group) is already
    /// **view-local** (the group subtracts the child origin), so the position is
    /// used directly.
    ///
    /// Handled here (top-row clicks while active):
    /// * **close** — `x` in `2..=4` when the close flag is set and active: arm a
    ///   mouse-track capture (up-only mask, swallowing everything until release);
    ///   post [`Command::CLOSE`] only on release over the close zone.
    /// * **zoom** — `x` in `(w-5)..=(w-3)` (or a double-click) when the zoom flag is
    ///   set: post [`Command::ZOOM`], consume. Checked *after* close, so a
    ///   double-click inside the close hot-zone resolves to close.
    ///
    /// Drag cases are handled by the owning `Window`:
    /// * title-bar move drag: the top-row click not on an icon is left unconsumed
    ///   on purpose — the window starts the move drag.
    /// * bottom-row grow drags: left unconsumed so the window starts a grow capture.
    /// * middle-button move: left unconsumed so the window starts a move drag.
    ///
    /// A frame is not selectable, so there is no auto-select on click and nothing
    /// to call through to.
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        match *ev {
            Event::MouseDown(m) => {
                if !self.border_visible {
                    return; // frameless: no close/zoom/drag hotspots
                }
                let w = self.st.size.x;
                if m.position.y == 0 && self.st.state.active {
                    if self.flags.close && (2..=4).contains(&m.position.x) {
                        // Arm an up-only capture (mouse_move / mouse_auto / wheel all
                        // false — swallows every mouse event until release), then
                        // confirm the release position in the MouseUp arm.
                        if let Some(id) = self.st.id() {
                            self.close_pressed = true;
                            ctx.start_mouse_track(
                                id,
                                self.abs_origin,
                                TrackMask {
                                    mouse_move: false,
                                    mouse_auto: false,
                                    wheel: false,
                                },
                            );
                            ev.clear();
                        } else {
                            // Degenerate fallback (no ViewId — ids are stamped
                            // at Group::insert, so this is test-only): post ON
                            // DOWN, skipping the release-confirm.
                            ctx.post(Command::CLOSE);
                            ev.clear();
                        }
                    } else if self.flags.zoom
                        && ((w - 5..=w - 3).contains(&m.position.x) || m.flags.double_click)
                    {
                        ctx.post(Command::ZOOM);
                        ev.clear();
                    }
                    // else: title-bar move drag — left unconsumed ON PURPOSE so the
                    // window picks it up and starts the move drag.
                }
                // else: bottom-row grow drags + middle-button move — left unconsumed
                // ON PURPOSE so the window starts the grow/move drag.
            }

            // ---------------------------------------------------------------
            // MouseUp arm — release-confirm: post a Close command only if the
            // button is released over the close zone. Guarded by `close_pressed`
            // against stray MouseUp events.
            // ---------------------------------------------------------------
            Event::MouseUp(m) if self.close_pressed && self.border_visible => {
                self.close_pressed = false;
                // Confirm the release is on the top row, over the close zone.
                if m.position.y == 0 && (2..=4).contains(&m.position.x) {
                    ctx.post(Command::CLOSE);
                }
                ev.clear();
            }

            _ => {}
        }
    }

    /// Downcast seam: `Window::zoom` pushes `set_zoomed` to its frame child via
    /// [`Group::child_mut`](crate::view::Group::child_mut) +
    /// `downcast_mut::<Frame>()`. See [`View::as_any_mut`].
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::event::{MouseButtons, MouseEvent, MouseEventFlags, MouseWheel};
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use crate::view::Point;
    use std::collections::VecDeque;

    // -- Context / event helpers --------------------------------------------

    fn make_ctx<'a>(
        out: &'a mut VecDeque<Event>,
        timers: &'a mut crate::timer::TimerQueue,
        deferred: &'a mut Vec<crate::view::Deferred>,
    ) -> Context<'a> {
        Context::new(out, timers, 0, deferred)
    }

    fn mouse_down_at(x: i32, y: i32) -> Event {
        Event::MouseDown(MouseEvent {
            position: Point::new(x, y),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            flags: MouseEventFlags::default(),
            wheel: MouseWheel::None,
            modifiers: Default::default(),
        })
    }

    fn mouse_up_at(x: i32, y: i32) -> Event {
        Event::MouseUp(MouseEvent {
            position: Point::new(x, y),
            buttons: MouseButtons::default(),
            flags: MouseEventFlags::default(),
            wheel: MouseWheel::None,
            modifiers: Default::default(),
        })
    }

    fn double_click_at(x: i32, y: i32) -> Event {
        Event::MouseDown(MouseEvent {
            position: Point::new(x, y),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            flags: MouseEventFlags {
                double_click: true,
                ..Default::default()
            },
            wheel: MouseWheel::None,
            modifiers: Default::default(),
        })
    }

    /// Stamp a `ViewId` onto a frame (as `Group::insert` would).
    fn stamp_id(f: &mut Frame) -> crate::view::ViewId {
        let id = crate::view::ViewId::next();
        f.st.id = Some(id);
        id
    }

    /// Collect the rendered glyphs of one row as a String.
    fn row_text(buf: &Buffer, y: u16) -> String {
        (0..buf.width())
            .map(|x| buf.get(x, y).symbol().to_string())
            .collect()
    }

    fn render_frame(frame: &mut Frame, w: u16, h: u16) -> Buffer {
        let theme = Theme::classic_blue();
        let mut buf = Buffer::new(w, h);
        let bounds = frame.st.get_bounds();
        let mut dc = DrawCtx::new(&mut buf, &theme, bounds, bounds.a);
        frame.draw(&mut dc);
        buf
    }

    // -- Constructor --------------------------------------------------------

    #[test]
    fn new_sets_grow_hi_and_is_not_selectable() {
        let f = Frame::new(Rect::new(0, 0, 20, 6));
        assert!(f.st.grow_mode.hi_x, "gfGrowHiX must be set");
        assert!(f.st.grow_mode.hi_y, "gfGrowHiY must be set");
        assert!(!f.st.grow_mode.lo_x && !f.st.grow_mode.lo_y);
        assert!(!f.st.options.selectable, "frame is not selectable");
        assert_eq!(f.title(), None);
        assert_eq!(f.number(), None);
        assert!(!f.zoomed());
        assert_eq!(f.flags(), WindowFlags::default());
    }

    // -- Owner-data-down setters --------------------------------------------

    #[test]
    fn setters_push_owner_data_down() {
        let mut f = Frame::new(Rect::new(0, 0, 20, 6));
        f.set_title(Some("Hello".into()));
        f.set_flags(WindowFlags {
            close: true,
            zoom: true,
            ..Default::default()
        });
        f.set_number(Some(3));
        f.set_zoomed(true);
        assert_eq!(f.title(), Some("Hello"));
        assert!(f.flags().close && f.flags().zoom);
        assert_eq!(f.number(), Some(3));
        assert!(f.zoomed());
    }

    // -- draw: title cap is width-10 even with close/zoom (faithful title fetch) --

    /// The reduced budget `l` (after the `-6` close/zoom and `-4` number
    /// subtractions) is passed only to the title-fetch step, which the **base**
    /// window ignores; the drawn title is capped to `width - 10`. So with close+zoom
    /// set and a title of 8 cols (≤ width-10 = 10, but > width-10-6 = 4), the
    /// full 8 chars must still show (not 4).
    #[test]
    fn title_cap_is_width_minus_10_regardless_of_close_zoom() {
        let mut f = Frame::new(Rect::new(0, 0, 20, 6));
        f.st.state.active = true;
        f.set_flags(WindowFlags {
            close: true,
            zoom: true,
            ..Default::default()
        });
        f.set_title(Some("Document".into())); // 8 cols
        let buf = render_frame(&mut f, 20, 6);
        // All 8 letters of "Document" must render. lw = min(8, 10) = 8;
        // i = (20 - 8) >> 1 = 6. Title occupies cols 6..14.
        let title: String = (6..14)
            .map(|x| buf.get(x, 0).symbol().to_string())
            .collect();
        assert_eq!(
            title, "Document",
            "the reduced budget must NOT cap the drawn title (base getTitle ignores it)"
        );
    }

    // -- draw: number drawn only when Some(n<10) ----------------------------

    #[test]
    fn number_drawn_only_when_present_and_below_ten() {
        // active so the box is drawn the same way; number is independent of state.
        let mut f = Frame::new(Rect::new(0, 0, 20, 6));
        f.st.state.active = true;
        f.set_number(Some(3));
        // flags.zoom is false → i = 3 → digit at column w-3 = 17.
        let buf = render_frame(&mut f, 20, 6);
        assert_eq!(buf.get(17, 0).symbol(), "3", "number drawn at w-3");

        // n >= 10 → not drawn.
        let mut f2 = Frame::new(Rect::new(0, 0, 20, 6));
        f2.st.state.active = true;
        f2.set_number(Some(10));
        let buf2 = render_frame(&mut f2, 20, 6);
        assert_eq!(
            buf2.get(17, 0).symbol(),
            "═",
            "n>=10 leaves the border glyph"
        );

        // None → not drawn.
        let mut f3 = Frame::new(Rect::new(0, 0, 20, 6));
        f3.st.state.active = true;
        let buf3 = render_frame(&mut f3, 20, 6);
        assert_eq!(
            buf3.get(17, 0).symbol(),
            "═",
            "no number leaves the border glyph"
        );
    }

    // -- draw: long title is truncated + centered within w-10 ---------------

    #[test]
    fn long_title_truncated_and_centered_within_budget() {
        // w=20 → title budget l = 20-10 = 10 (no close/zoom/number).
        let mut f = Frame::new(Rect::new(0, 0, 20, 6));
        f.st.state.active = true;
        f.set_title(Some("This title is far too long to fit".into()));
        let buf = render_frame(&mut f, 20, 6);
        let row0 = row_text(&buf, 0);
        // The visible title text must be at most 10 columns wide. Count the
        // run of ASCII letters/spaces on the top row between the border corners.
        let title_chars: String = row0
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == ' ')
            .collect();
        assert!(
            title_chars.trim().chars().count() <= 10,
            "title must be truncated to the 10-column budget, got: {title_chars:?}"
        );
        // Centered: budget 10, so i = (20 - 10) >> 1 = 5; the title starts at col 5.
        assert_eq!(
            buf.get(5, 0).symbol(),
            "T",
            "truncated title starts centered at col 5"
        );
    }

    // -- draw: dragging state → single-line box in the FrameDragging style ----

    /// When `dragging` is set, the state match takes the dragging arm (first arm,
    /// `Role::FrameDragging`, single-line) regardless of `active`. The box must
    /// use single-line glyphs (┌, not ╔) and the border cells must carry the
    /// `Role::FrameDragging` style.
    #[test]
    fn dragging_draws_single_line_box_in_dragging_style() {
        let mut f = Frame::new(Rect::new(0, 0, 20, 6));
        f.st.state.active = true; // dragging wins over active
        f.st.state.dragging = true;
        let buf = render_frame(&mut f, 20, 6);
        // (a) single-line corner, not the double-line active corner.
        assert_eq!(
            buf.get(0, 0).symbol(),
            "┌",
            "dragging frame uses single-line box glyphs"
        );
        assert_ne!(buf.get(0, 0).symbol(), "╔");
        // (b) border style is the FrameDragging style.
        let expected = Theme::classic_blue().style(Role::FrameDragging);
        assert_eq!(buf.get(0, 0).style(), expected, "top-left border style");
    }

    // -- draw: gray palette → FrameGray* role family ----

    /// With `palette = Gray` (a dialog's frame), the border AND interior must
    /// carry the `FrameGray*` styles: `FrameGrayActive` when active,
    /// `FrameGrayPassive` when passive — never the blue `Frame*` family.
    #[test]
    fn gray_palette_draws_border_and_interior_in_gray_roles() {
        let theme = Theme::classic_blue();

        // Active gray frame → FrameGrayActive everywhere (border + interior).
        let mut f = Frame::new(Rect::new(0, 0, 20, 6));
        f.st.state.active = true;
        f.set_palette(WindowPalette::Gray);
        let buf = render_frame(&mut f, 20, 6);
        let expected = theme.style(Role::FrameGrayActive);
        assert_eq!(buf.get(0, 0).style(), expected, "active border corner");
        assert_eq!(buf.get(5, 2).style(), expected, "active interior fill");
        assert_ne!(
            expected,
            theme.style(Role::FrameActive),
            "gray and blue active styles must differ for the test to be meaningful"
        );

        // Passive gray frame → FrameGrayPassive.
        let mut p = Frame::new(Rect::new(0, 0, 20, 6));
        p.set_palette(WindowPalette::Gray);
        let bufp = render_frame(&mut p, 20, 6);
        let expected_p = theme.style(Role::FrameGrayPassive);
        assert_eq!(bufp.get(0, 0).style(), expected_p, "passive border corner");
        assert_eq!(bufp.get(5, 2).style(), expected_p, "passive interior fill");

        // Dragging gray frame → FrameGrayDragging.
        let mut d = Frame::new(Rect::new(0, 0, 20, 6));
        d.st.state.active = true;
        d.st.state.dragging = true;
        d.set_palette(WindowPalette::Gray);
        let bufd = render_frame(&mut d, 20, 6);
        assert_eq!(
            bufd.get(0, 0).style(),
            theme.style(Role::FrameGrayDragging),
            "dragging border corner"
        );
    }

    // -- draw: cyan palette → FrameCyan* role family ----

    /// With `palette = Cyan`, the border AND interior must
    /// carry the `FrameCyan*` styles — never the blue `Frame*` family.
    #[test]
    fn cyan_palette_draws_border_and_interior_in_cyan_roles() {
        let theme = Theme::classic_blue();

        // Active cyan frame → FrameCyanActive everywhere (border + interior).
        let mut f = Frame::new(Rect::new(0, 0, 20, 6));
        f.st.state.active = true;
        f.set_palette(WindowPalette::Cyan);
        let buf = render_frame(&mut f, 20, 6);
        let expected = theme.style(Role::FrameCyanActive);
        assert_eq!(buf.get(0, 0).style(), expected, "active border corner");
        assert_eq!(buf.get(5, 2).style(), expected, "active interior fill");
        assert_ne!(
            expected,
            theme.style(Role::FrameActive),
            "cyan and blue active styles must differ for the test to be meaningful"
        );

        // Passive cyan frame → FrameCyanPassive.
        let mut p = Frame::new(Rect::new(0, 0, 20, 6));
        p.set_palette(WindowPalette::Cyan);
        let bufp = render_frame(&mut p, 20, 6);
        let expected_p = theme.style(Role::FrameCyanPassive);
        assert_eq!(bufp.get(0, 0).style(), expected_p, "passive border corner");
        assert_eq!(bufp.get(5, 2).style(), expected_p, "passive interior fill");

        // Dragging cyan frame → FrameCyanDragging.
        let mut d = Frame::new(Rect::new(0, 0, 20, 6));
        d.st.state.active = true;
        d.st.state.dragging = true;
        d.set_palette(WindowPalette::Cyan);
        let bufd = render_frame(&mut d, 20, 6);
        assert_eq!(
            bufd.get(0, 0).style(),
            theme.style(Role::FrameCyanDragging),
            "dragging border corner"
        );
    }

    // -- handle_event: close (release-confirm) --------------------------------

    /// Degenerate fallback (no ViewId — uninserted frame): the Close command fires
    /// on mouse-**down**, preserving backwards compat for pure-unit tests.
    #[test]
    fn click_close_icon_posts_close_on_down_no_id() {
        let mut f = Frame::new(Rect::new(0, 0, 20, 6));
        f.st.state.active = true;
        f.set_flags(WindowFlags {
            close: true,
            ..Default::default()
        });
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        let mut ev = mouse_down_at(3, 0); // close hot-zone is x in 2..=4
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            f.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "close click consumed");
        assert_eq!(
            out.len(),
            1,
            "immediate cmClose on down (no ViewId fallback)"
        );
        assert_eq!(out[0], Event::Command(Command::CLOSE));
    }

    /// With a ViewId (inserted frame): mouse-down in close zone arms tracking;
    /// the Close command fires only on `MouseUp` over the close zone
    /// (release-confirm).
    #[test]
    fn click_close_icon_release_confirm_with_id() {
        let mut f = Frame::new(Rect::new(0, 0, 20, 6));
        f.st.state.active = true;
        f.set_flags(WindowFlags {
            close: true,
            ..Default::default()
        });
        let _id = stamp_id(&mut f);

        // MouseDown in the close zone: arms tracking, no Close command yet.
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            let mut ev = mouse_down_at(3, 0);
            f.handle_event(&mut ev, &mut ctx);
            assert!(ev.is_nothing(), "mouse-down consumed");
        }
        assert!(
            out.is_empty(),
            "no Close command on down — deferred release-confirm"
        );
        assert!(f.close_pressed, "tracking armed (no command yet)");
        assert_eq!(deferred.len(), 1, "PushCapture deferred");
        assert!(
            matches!(deferred[0], crate::view::Deferred::PushCapture(_)),
            "deferred[0] is PushCapture"
        );
        if let crate::view::Deferred::PushCapture(ref h) = deferred[0] {
            assert_eq!(h.view(), Some(_id), "capture routes to this frame's id");
        }

        // MouseUp over the close zone: the Close command fires.
        let mut out2 = VecDeque::new();
        let mut deferred2: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out2, &mut timers, &mut deferred2);
            let mut ev = mouse_up_at(3, 0);
            f.handle_event(&mut ev, &mut ctx);
            assert!(ev.is_nothing(), "mouse-up consumed");
        }
        assert!(!f.close_pressed, "tracking cleared");
        assert_eq!(out2.len(), 1);
        assert_eq!(out2[0], Event::Command(Command::CLOSE));
    }

    /// Press close icon, release OUTSIDE the close zone: no Close command
    /// (the position check fails).
    #[test]
    fn close_icon_release_outside_no_close() {
        let mut f = Frame::new(Rect::new(0, 0, 20, 6));
        f.st.state.active = true;
        f.set_flags(WindowFlags {
            close: true,
            ..Default::default()
        });
        let _id = stamp_id(&mut f);

        // MouseDown in the close zone: arm tracking.
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            let mut ev = mouse_down_at(3, 0);
            f.handle_event(&mut ev, &mut ctx);
        }
        assert!(f.close_pressed);

        // MouseUp outside the close zone (x = 10, top row): no Close command.
        let mut out2 = VecDeque::new();
        let mut deferred2: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out2, &mut timers, &mut deferred2);
            let mut ev = mouse_up_at(10, 0);
            f.handle_event(&mut ev, &mut ctx);
            assert!(ev.is_nothing(), "tracked up consumed");
        }
        assert!(!f.close_pressed, "tracking cleared even without close");
        assert!(
            out2.is_empty(),
            "no Close command when released outside the close zone"
        );
    }

    // -- handle_event: zoom (click inside w-5..=w-3) --------------------------

    #[test]
    fn click_zoom_icon_posts_zoom_and_consumes() {
        let mut f = Frame::new(Rect::new(0, 0, 20, 6));
        f.st.state.active = true;
        f.set_flags(WindowFlags {
            zoom: true,
            ..Default::default()
        });
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        // w=20 → zoom hot-zone is x in 15..=17. Click at w-4 = 16.
        let mut ev = mouse_down_at(16, 0);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            f.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "zoom click consumed");
        assert_eq!(out[0], Event::Command(Command::ZOOM));
    }

    // -- handle_event: double-click anywhere on top row → zoom ----------------

    #[test]
    fn double_click_row0_posts_zoom() {
        let mut f = Frame::new(Rect::new(0, 0, 20, 6));
        f.st.state.active = true;
        f.set_flags(WindowFlags {
            zoom: true,
            ..Default::default()
        });
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        // Double-click outside the close hot-zone (e.g. x=10) → zoom.
        let mut ev = double_click_at(10, 0);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            f.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing());
        assert_eq!(out[0], Event::Command(Command::ZOOM));
    }

    // -- handle_event: passive frame ignores clicks ---------------------------

    #[test]
    fn passive_frame_ignores_clicks() {
        let mut f = Frame::new(Rect::new(0, 0, 20, 6));
        // not active
        f.set_flags(WindowFlags {
            close: true,
            zoom: true,
            ..Default::default()
        });
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        let mut ev = mouse_down_at(3, 0);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            f.handle_event(&mut ev, &mut ctx);
        }
        assert!(!ev.is_nothing(), "passive frame does not consume the click");
        assert!(out.is_empty(), "passive frame posts nothing");
    }

    // -- handle_event: close takes priority over zoom on overlap --------------

    #[test]
    fn close_wins_over_zoom_in_close_hot_zone() {
        // No stamp_id → exercises the no-id fallback (post on down), which is
        // sufficient here: close-vs-zoom priority is structural (`else if`
        // branch order), identical on the tracked path.
        // A narrow frame where the close zone (2..=4) and zoom zone overlap is
        // contrived; instead verify the branch order: with both flags set, a
        // click at x=3 (close zone) must post CLOSE, not ZOOM.
        let mut f = Frame::new(Rect::new(0, 0, 20, 6));
        f.st.state.active = true;
        f.set_flags(WindowFlags {
            close: true,
            zoom: true,
            ..Default::default()
        });
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        let mut ev = mouse_down_at(3, 0);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            f.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(out[0], Event::Command(Command::CLOSE));
    }

    // -- Snapshot: active frame ---------------------------------------------

    /// Active frame: double-line box, centered title, `[■]` close + `[↑]` zoom
    /// icons on the top row, resize icons on the bottom row.
    #[test]
    fn snapshot_active_frame() {
        let theme = Theme::classic_blue();
        let mut f = Frame::new(Rect::new(0, 0, 20, 6));
        f.st.state.active = true; // no group to propagate the active state in the test
        f.set_title(Some("Edit".into()));
        f.set_flags(WindowFlags {
            r#move: true,
            grow: true,
            close: true,
            zoom: true,
        });

        let (backend, screen) = HeadlessBackend::new(20, 6);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = f.st.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            f.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    // -- Snapshot: passive frame --------------------------------------------

    /// Passive frame: single-line box, centered title, **no icons** even though
    /// the flags are set (icons are active-only).
    #[test]
    fn snapshot_passive_frame() {
        let theme = Theme::classic_blue();
        let mut f = Frame::new(Rect::new(0, 0, 20, 6));
        // not active → passive single-line, no icons.
        f.set_title(Some("Edit".into()));
        f.set_flags(WindowFlags {
            r#move: true,
            grow: true,
            close: true,
            zoom: true,
        });

        let (backend, screen) = HeadlessBackend::new(20, 6);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = f.st.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            f.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    // -- junction marks: tee substitution ------------------------------------

    #[test]
    fn junction_marks_substitute_tees_on_interior_edges() {
        use crate::junction::{Edge, JunctionMark, Weight};
        // Passive (single-line) frame so the bar weight is Single.
        let mut f = Frame::new(Rect::new(0, 0, 20, 6));
        f.set_junction_marks(vec![
            JunctionMark {
                edge: Edge::Top,
                offset: 8,
                stem: Weight::Single,
            },
            JunctionMark {
                edge: Edge::Bottom,
                offset: 8,
                stem: Weight::Single,
            },
            JunctionMark {
                edge: Edge::Right,
                offset: 3,
                stem: Weight::Single,
            },
            JunctionMark {
                edge: Edge::Left,
                offset: 3,
                stem: Weight::Single,
            },
        ]);
        let buf = render_frame(&mut f, 20, 6);
        assert_eq!(buf.get(8, 0).symbol(), "┬", "top-edge mark → ┬");
        assert_eq!(buf.get(8, 5).symbol(), "┴", "bottom-edge mark → ┴");
        assert_eq!(buf.get(19, 3).symbol(), "┤", "right-edge mark → ┤");
        assert_eq!(buf.get(0, 3).symbol(), "├", "left-edge mark → ├");
        assert_eq!(buf.get(5, 0).symbol(), "─", "unmarked top stays ─");
        assert_eq!(buf.get(0, 0).symbol(), "┌");
        assert_eq!(buf.get(19, 0).symbol(), "┐");
    }

    #[test]
    fn no_marks_is_byte_for_byte_unchanged() {
        let mut a = Frame::new(Rect::new(0, 0, 20, 6));
        a.st.state.active = true;
        let mut b = Frame::new(Rect::new(0, 0, 20, 6));
        b.st.state.active = true;
        b.set_junction_marks(vec![]);
        let ba = render_frame(&mut a, 20, 6);
        let bb = render_frame(&mut b, 20, 6);
        for y in 0..6 {
            assert_eq!(row_text(&ba, y), row_text(&bb, y), "row {y} identical");
        }
    }

    #[test]
    fn active_double_frame_uses_mixed_tee_for_single_stem() {
        use crate::junction::{Edge, JunctionMark, Weight};
        let mut f = Frame::new(Rect::new(0, 0, 20, 6));
        f.st.state.active = true; // double-line bar
        f.set_junction_marks(vec![JunctionMark {
            edge: Edge::Top,
            offset: 8,
            stem: Weight::Single,
        }]);
        let buf = render_frame(&mut f, 20, 6);
        assert_eq!(buf.get(8, 0).symbol(), "╤", "double bar + single stem → ╤");
    }

    // -- frameless: no border, no title, no icons ------------------------------

    #[test]
    fn frameless_draws_no_border() {
        // A frame with border_visible=false fills its interior background but draws
        // no box edges, title, or icons.
        let theme = crate::theme::Theme::classic_blue();
        let (backend, screen) = crate::backend::HeadlessBackend::new(12, 4);
        let mut r = crate::backend::Renderer::new(Box::new(backend));
        let mut frame = Frame::new(Rect::new(0, 0, 12, 4));
        frame.set_title(Some("Hi".into()));
        frame.st.state.active = true;
        frame.set_border_visible(false);
        r.render(|buf: &mut crate::screen::Buffer| {
            let b = frame.st.get_bounds();
            let mut dc = crate::view::DrawCtx::new(buf, &theme, b, b.a);
            frame.draw(&mut dc);
        });
        let snap = screen.snapshot();
        assert!(
            !snap.contains('═') && !snap.contains('║'),
            "no double-line border"
        );
        assert!(!snap.contains("Hi"), "no title when frameless");
    }

    /// The downcast seam: `Frame` overrides `as_any_mut` so an owner can reach
    /// it concretely (e.g. a window's zoom pushing `set_zoomed`); a plain view's
    /// base `as_any_mut` returns `None`.
    #[test]
    fn as_any_mut_seam_resolves_frame_but_not_a_plain_view() {
        let mut f = Frame::new(Rect::new(0, 0, 10, 5));
        // The frame downcasts to its concrete type and is mutable through it.
        {
            let any = View::as_any_mut(&mut f).expect("Frame overrides as_any_mut");
            let frame = any
                .downcast_mut::<Frame>()
                .expect("downcasts to the concrete Frame");
            frame.set_zoomed(true);
        }
        assert!(f.zoomed(), "the concrete push through the seam took effect");

        // A plain view (base impl) returns None.
        struct Plain {
            st: ViewState,
        }
        impl View for Plain {
            fn state(&self) -> &ViewState {
                &self.st
            }
            fn state_mut(&mut self) -> &mut ViewState {
                &mut self.st
            }
            fn draw(&mut self, _ctx: &mut DrawCtx) {}
        }
        let mut p = Plain {
            st: ViewState::new(Rect::new(0, 0, 4, 4)),
        };
        assert!(
            View::as_any_mut(&mut p).is_none(),
            "base as_any_mut returns None"
        );
    }
}
