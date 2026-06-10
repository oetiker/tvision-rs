//! `TFrame` — faithful Rust port of `tframe.cpp` / `framelin.cpp` (row 24,
//! FOUNDATION).
//!
//! A `TFrame` is the decorative border a `TWindow` (row 33) places around its
//! interior: the box, the centered title, the optional window number, and the
//! close / zoom / resize icons. It is a passive view — it never takes focus.
//!
//! # The owner-data-down seam (D3)
//!
//! In C++ `TFrame::draw` and `handleEvent` reach **up** through `owner` into the
//! `TWindow`: `((TWindow*)owner)->{flags, number, getTitle(l), sizeLimits(), size,
//! state}`. D3 forbids that up-pointer. Instead, [`Frame`] holds its **own copy**
//! of exactly the data it reads, pushed **down** by the owning window:
//!
//! * [`title`](Frame::title) ← `getTitle(l)`
//! * [`flags`](Frame::flags) ← `owner->flags` ([`WindowFlags`], D5)
//! * [`number`](Frame::number) ← `owner->number` (`wnNoNumber` → `None`)
//! * [`zoomed`](Frame::zoomed) ← `owner->size == maxSize` (the unZoom-vs-zoom
//!   icon choice — the window computes this and pushes the bool down)
//!
//! The window calls the public setters ([`set_title`](Frame::set_title) etc.)
//! whenever its own state changes. The frame's `sfActive` / `sfDragging` state
//! instead arrive through the normal [`View::set_state`] propagation that
//! `Group` (row 26) already drives onto every child, so [`Frame::draw`] reads
//! them from `self.st.state` directly.
//!
//! # Deviations applied
//!
//! * **D2** — `TView` is the [`View`] trait + [`ViewState`]; `Frame` embeds
//!   `st: ViewState` and `impl View for Frame`.
//! * **D5** — `wf*` flag word → [`WindowFlags`] struct-of-bools. Now owned by
//!   the `window` module (relocated at row 33); `frame.rs` imports it via
//!   `use crate::window::WindowFlags;` and renders the pushed-down copy.
//! * **D7** — no `getColor`/`getPalette`; the border/icon roles come in
//!   **per-palette families** selected by the owner's
//!   [`WindowPalette`](crate::window::WindowPalette) (pushed down via
//!   [`Frame::set_palette`], D3): `Blue` → [`Role::FrameActive`] /
//!   [`Role::FramePassive`] / [`Role::FrameDragging`] / [`Role::FrameIcon`];
//!   `Gray` (dialogs) → the `Role::FrameGray*` family. `Cyan` falls back to the
//!   blue family for now (`TODO(row 34 cyan theming)`). This mirrors the C++,
//!   where `TFrame::draw` resolves its colors through the owner's palette
//!   (`cpBlueWindow` / `cpGrayDialog` / …). The C++ has distinct title palette
//!   entries (2 passive / 4 active) routed through the same chain; those belong
//!   to row 33, so the title **reuses the border role** here.
//! * **D8** — no `writeLine`/`TDrawBuffer`/`drawView`; we draw straight through
//!   [`DrawCtx`] in view-local coords. The C++ `setState` override only calls
//!   `drawView()`, which is redundant under whole-tree redraw + diff, so there
//!   is **no `set_state` override** — the base [`View::set_state`] suffices.
//!
//! # Deferred (per design)
//!
//! * **The `framelin.cpp` sibling tee-walk** (the `├┬┤┴` joins where a nested
//!   `ofFramed` view meets the group frame) needs sibling bounds, which D3
//!   denies a child. So the whole `FrameMask` / `frameChars[33]` / `initFrame`
//!   bitmask machinery + the sibling loop is **deferred to a later polish pass**
//!   (it will need `Group` cooperation to pass sibling bounds down). We draw
//!   plain corners/edges — byte-identical to C++ for the common case (a window
//!   whose border no `ofFramed` sibling touches). The tee/cross glyphs are
//!   seeded in [`Glyphs`](crate::theme::Glyphs) for completeness but unused.
//! * **The drag / press-and-hold loops** (`dragWindow`→`dragView`, the close
//!   icon's `while(mouseEvent(...))` release-confirm loop, the bottom-row
//!   grow-drag, the middle-button move) need the live event loop + capture stack
//!   (rows 31/33, D9). See [`Frame::handle_event`] — each carries a
//!   `TODO(row 33, D9)`.

use crate::command::Command;
use crate::event::Event;
use crate::theme::Role;
use crate::view::{Context, DrawCtx, GrowMode, Rect, View, ViewState};
use crate::window::{WindowFlags, WindowPalette};

// ---------------------------------------------------------------------------
// Frame
// ---------------------------------------------------------------------------

/// `TFrame` — a window's decorative border (D2 View trait + ViewState).
///
/// Embeds the pattern: `st: ViewState`, `impl View`, draw through [`DrawCtx`],
/// handle events through [`Context`]. The window-owned data it renders
/// (title / flags / number / zoomed) is pushed down by the owning `TWindow`
/// (row 33) through the public setters; see the module docs for the D3 seam.
pub struct Frame {
    /// View state (geometry, flags, etc.) — the D2 composition target.
    pub st: ViewState,
    /// The window title, pushed down from `owner->getTitle(l)` (D3).
    title: Option<String>,
    /// The window decoration flags, pushed down from `owner->flags` (D3).
    flags: WindowFlags,
    /// The window number (`wnNoNumber` → `None`); drawn only if `Some(n)` and
    /// `n < 10`. Pushed down from `owner->number` (D3).
    number: Option<u8>,
    /// Whether the window is maximized — replaces `owner->size == maxSize` for
    /// the unZoom-vs-zoom icon choice. Pushed down by the window (D3).
    zoomed: bool,
    /// The owner's colour scheme, pushed down from `owner->palette` (D3, row 34
    /// gray theming). Selects the `Role::Frame*` vs `Role::FrameGray*` family
    /// in [`draw`](View::draw).
    palette: WindowPalette,
}

impl Frame {
    /// `TFrame::TFrame(bounds)` — construct a frame.
    ///
    /// Faithful to the C++ ctor: `growMode = gfGrowHiX + gfGrowHiY` so the frame
    /// stretches with its owner on the right and bottom edges.
    ///
    /// The C++ also sets `eventMask |= evBroadcast | evMouseUp`. Under D4 those
    /// classes are delivered unconditionally (our opt-in [`EventMask`] only
    /// carries `mouse_move` / `mouse_auto`), so there is nothing to set.
    ///
    /// The frame is **not** selectable (it never takes focus), so `options`
    /// stays all-false.
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
        }
    }

    // -- Owner-data-down setters / getters (D3) ------------------------------

    /// Set the window title (`owner->getTitle(l)` pushed down).
    pub fn set_title(&mut self, title: Option<String>) {
        self.title = title;
    }

    /// The window title.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Set the window decoration flags (`owner->flags` pushed down).
    pub fn set_flags(&mut self, flags: WindowFlags) {
        self.flags = flags;
    }

    /// The window decoration flags.
    pub fn flags(&self) -> WindowFlags {
        self.flags
    }

    /// Set the window number (`owner->number` pushed down; `wnNoNumber` → `None`).
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

    /// Set the owner's colour scheme (`owner->palette` pushed down, D3 — row 34
    /// gray theming). `Gray` makes [`draw`](View::draw) use the
    /// `Role::FrameGray*` family; `Cyan` currently falls back to the blue
    /// family (`TODO(row 34 cyan theming)`).
    pub(crate) fn set_palette(&mut self, palette: WindowPalette) {
        self.palette = palette;
    }

    /// The owner's colour scheme.
    pub fn palette(&self) -> WindowPalette {
        self.palette
    }
}

impl View for Frame {
    fn state(&self) -> &ViewState {
        &self.st
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.st
    }

    /// `TFrame::draw` — paint the border, title, number and icons.
    ///
    /// State selection is faithful to C++ (`dragging` checked first, then
    /// `!active`, else `active`); the role family follows the owner's
    /// [`WindowPalette`] (blue family shown; `Gray` substitutes `FrameGray*`):
    ///
    /// | state    | border role     | line set    |
    /// |----------|-----------------|-------------|
    /// | dragging | `FrameDragging` | single      |
    /// | passive  | `FramePassive`  | single      |
    /// | active   | `FrameActive`   | double      |
    ///
    /// We draw the box fully first, then overlay the number / title / icons onto
    /// row 0 (and the resize icons onto the bottom row). The C++ draws into a
    /// `TDrawBuffer` row then blits; the visual result is identical (D8).
    ///
    /// The `framelin.cpp` sibling tee-walk is deferred (D3) — we draw plain
    /// corners/edges; see the module docs.
    fn draw(&mut self, ctx: &mut DrawCtx) {
        let glyphs = *ctx.glyphs();
        let w = self.st.size.x;
        let h = self.st.size.y;
        if w <= 0 || h <= 0 {
            return;
        }

        // -- Palette → role family (the C++ resolves through the owner's
        // palette: cpBlueWindow vs cpGrayDialog). Cyan falls back to the blue
        // family — TODO(row 34 cyan theming): faithful cpCyanWindow values.
        let (r_dragging, r_passive, r_active, r_icon) = match self.palette {
            WindowPalette::Blue | WindowPalette::Cyan => (
                Role::FrameDragging,
                Role::FramePassive,
                Role::FrameActive,
                Role::FrameIcon,
            ),
            WindowPalette::Gray => (
                Role::FrameGrayDragging,
                Role::FrameGrayPassive,
                Role::FrameGrayActive,
                Role::FrameGrayIcon,
            ),
        };

        // -- State → (border role, double-line) -- faithful order: dragging first.
        let (border_role, double) = if self.st.state.dragging {
            (r_dragging, false)
        } else if !self.st.state.active {
            (r_passive, false)
        } else {
            (r_active, true)
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

        // -- 1. The box (all in the border style). --------------------------
        // Top row: tl, ─ across the interior, tr.
        ctx.put_char(0, 0, tl, border);
        for x in 1..w - 1 {
            ctx.put_char(x, 0, h_edge, border);
        }
        if w >= 2 {
            ctx.put_char(w - 1, 0, tr, border);
        }
        // Middle rows: │, spaces across the interior, │.
        for y in 1..h - 1 {
            ctx.put_char(0, y, v_edge, border);
            for x in 1..w - 1 {
                ctx.put_char(x, y, ' ', border);
            }
            if w >= 2 {
                ctx.put_char(w - 1, y, v_edge, border);
            }
        }
        // Bottom row: bl, ─ across, br.
        if h >= 2 {
            ctx.put_char(0, h - 1, bl, border);
            for x in 1..w - 1 {
                ctx.put_char(x, h - 1, h_edge, border);
            }
            if w >= 2 {
                ctx.put_char(w - 1, h - 1, br, border);
            }
        }

        // -- 2. Title budget `l` (dropped). --------------------------------
        // C++ builds a budget `l = size.x - 10`, then `l -= 6` if (wfClose|wfZoom)
        // and `l -= 4` if a number is shown, and passes it to `getTitle(l)`.
        // But base `TWindow::getTitle(short)` **ignores its argument and returns
        // the full title** (`twindow.cpp`), and the drawn width is then recomputed
        // as `min(strwidth(title), width-10)` — so the `-6` / `-4` reductions never
        // cap the drawn title for a base window (a subclass could abbreviate). The
        // budget is therefore dead here and is not computed; we cap the *drawn*
        // title to `width - 10` in step 4, matching `moveStr(i, title, …, l)`.

        // -- 3. Window number (row 0). -------------------------------------
        // C++: if number != wnNoNumber && number < 10 { l -= 4; ... } — the
        // `l -= 4` only fed the dropped budget above, so only the draw remains.
        if let Some(n) = self.number
            && n < 10
        {
            let i = if self.flags.zoom { 7 } else { 3 };
            if let Some(digit) = char::from_digit(u32::from(n), 10) {
                ctx.put_char(w - i, 0, digit, border);
            }
        }

        // -- 4. Title (row 0), centered. -----------------------------------
        // C++: title = getTitle(l); l = min(strwidth(title), width-10); l = max(l, 0);
        //      i = (width - l) >> 1; putChar(i-1, ' '); moveStr(i, title, cTitle, l);
        //      putChar(i+l, ' ');
        // Base `getTitle` returns the full title, so the effective cap is
        // `width - 10` (truncation = `TText::scroll`, our `text::scroll`).
        if let Some(title) = &self.title {
            let cap = w - 10;
            let (end, lw) = crate::text::scroll(title, cap, false);
            let lw = lw as i32;
            let truncated = &title[..end];
            // C++ centers at i=(width-l)>>1 with flanking spaces at i-1 and i+l,
            // drawn unconditionally once the title pointer is non-null (so Some("")
            // and the w<10 clamp-to-0 case still punch two spaces into the border
            // center). Flanking spaces + title, all in the border style (D7 note).
            let i = (w - lw) >> 1;
            ctx.put_char(i - 1, 0, ' ', border);
            ctx.put_str(i, 0, truncated, border);
            ctx.put_char(i + lw, 0, ' ', border);
        }

        // -- 5. Active-only icons (row 0). ---------------------------------
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

        // -- 6. Active + grow resize icons (bottom row). -------------------
        if self.st.state.active && self.flags.grow && h >= 2 {
            ctx.put_cstr(0, h - 1, glyphs.drag_left_icon, border, icon);
            ctx.put_cstr(w - 2, h - 1, glyphs.drag_icon, border, icon);
        }
    }

    /// `TFrame::handleEvent` — minimal scope; the drag loops are deferred (D9).
    ///
    /// The mouse position delivered by `Group` (row 26) is already **view-local**
    /// (the group subtracts the child origin), so `makeLocal` is gone — `m`'s
    /// position is used directly.
    ///
    /// Handled now (row-0 clicks while active):
    /// * **close** — `x` in `2..=4` with `wfClose`: `post(cmClose)`, consume.
    /// * **zoom** — `x` in `(w-5)..=(w-3)` (or a double-click) with `wfZoom`:
    ///   `post(cmZoom)`, consume. Checked *after* close, so a double-click inside
    ///   the close hot-zone resolves to close (faithful: the close branch runs
    ///   first).
    ///
    /// **TODO(row 33, D9)** — deferred, all needing the live loop + capture stack:
    /// * the close icon's press-and-hold release-confirm loop
    ///   (`while(mouseEvent(...))`): we `post(cmClose)` on mouse-**down** instead.
    /// * `wfMove` frame-drag (`dragWindow(dmDragMove)`): the row-0 click that is
    ///   not on an icon — left unconsumed.
    /// * the bottom-row grow drags (`wfGrow`: `x>=size.x-2` → `dragGrow`,
    ///   `x<=1` → `dragGrowLeft`).
    /// * the middle-button move.
    ///
    /// The base `View::handle_event` is a no-op, so there is nothing to call
    /// through to (the C++ `TView::handleEvent(event)` did the auto-select, which
    /// relocated to `Group`; a frame is not selectable anyway).
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        if let Event::MouseDown(m) = *ev {
            let w = self.st.size.x;
            if m.position.y == 0 && self.st.state.active {
                if self.flags.close && (2..=4).contains(&m.position.x) {
                    // TODO(row 33, D9): the C++ runs a press-and-hold
                    // `while(mouseEvent(event, evMouseMove))` release-confirm
                    // loop and only posts cmClose if the button is released over
                    // the icon. We post on mouse-down for now.
                    ctx.post(Command::CLOSE);
                    ev.clear();
                } else if self.flags.zoom
                    && ((w - 5..=w - 3).contains(&m.position.x) || m.flags.double_click)
                {
                    ctx.post(Command::ZOOM);
                    ev.clear();
                }
                // else: wfMove frame-drag — TODO(row 33, D9). Left unconsumed.
            }
            // else: bottom-row grow drags + middle-button move — TODO(row 33, D9).
        }
    }

    /// Downcast seam (33c, D3): `TWindow::zoom` pushes `set_zoomed` to its frame
    /// child via [`Group::child_mut`](crate::view::Group::child_mut) +
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

    // -- draw: title cap is width-10 even with close/zoom (faithful getTitle) --

    /// The reduced budget `l` (after the `-6` close/zoom and `-4` number
    /// subtractions) is passed only to `getTitle`, which the **base** window
    /// ignores; the drawn title is capped to `width - 10`. So with close+zoom
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
        // run of ASCII letters/spaces on row 0 between the border corners.
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

    // -- draw: gray palette → FrameGray* role family (row 34 gray theming) ----

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

    // -- handle_event: close --------------------------------------------------

    #[test]
    fn click_close_icon_posts_close_and_consumes() {
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
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], Event::Command(Command::CLOSE));
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

    // -- handle_event: double-click anywhere on row 0 → zoom ------------------

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
        f.st.state.active = true; // no group to propagate sfActive in the test
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

    /// The 33c downcast seam: `Frame` overrides `as_any_mut` so an owner can reach
    /// it concretely (e.g. `TWindow::zoom` pushing `set_zoomed`); a plain view's
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
