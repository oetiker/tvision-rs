//! A scroll bar: a draggable thumb, paging troughs, and step arrows, drawn into
//! its own 1×N (vertical) or N×1 (horizontal) bounds. Orientation is inferred
//! from the bounds at construction: `width == 1` means vertical, `height == 1`
//! means horizontal.
//!
//! # Glyphs
//!
//! The arrow/page/thumb characters live in [`crate::theme::Glyphs`]
//! (`sb_v_arrow_back`, `sb_v_arrow_fwd`, `sb_h_arrow_back`, `sb_h_arrow_fwd`,
//! `sb_page`, `sb_thumb`, `sb_page_no_range`) and are read via `ctx.glyphs()`.
//!
//! # Broadcasts
//!
//! When the value changes, the bar broadcasts [`Command::SCROLL_BAR_CHANGED`],
//! carrying its own [`ViewId`](crate::view::ViewId) as the broadcast `source`, so
//! an owner with two bars can tell which bar fired. On mouse-down / keyboard
//! action it likewise broadcasts [`Command::SCROLL_BAR_CLICKED`]. The bar only
//! emits these; a scroller, list viewer, or editor consumes them (see the
//! [scroller's read-sync](crate::widgets::Scroller)).
//!
//! # Press-and-hold auto-repeat
//!
//! The bar supports two modal holds, each driven through the mouse-track capture:
//!
//! * **Arrow press-and-hold:** `MouseDown` on an arrow does the first step, then
//!   calls [`Context::start_mouse_track`]. Each auto tick re-derives the part code
//!   under the cursor; if it still matches the clicked part, it steps the value.
//!   `MouseUp` clears tracking.
//!
//! * **Thumb drag:** `MouseDown` on the indicator (or outside the extent) does
//!   the first recompute, then arms the capture. Each tracked move recomputes the
//!   value from the thumb position. `MouseUp` clears tracking.
//!
//! # Colors
//!
//! Two [`Theme`](crate::theme::Theme) roles control the scrollbar appearance:
//!
//! * [`Role::ScrollBarPage`] — the trough (page) cells.
//! * [`Role::ScrollBarControls`] — arrow glyphs and the thumb.
//!
//! The classic blue theme resolves both to blue-on-cyan via the palette chain
//! `CScrollBar → cpBlueWindow → cpAppColor` (slots 11–12 in the app palette).
//! The C++ `CScrollBar` has three entries — page slot and two control slots
//! (arrows + indicator); those two control slots share one RGB value, so Rust
//! collapses them into the single `ScrollBarControls` role.
//!
//! # Turbo Vision heritage
//!
//! Ports `TScrollBar` (`tscrlbar.cpp`/`scrlbar.h`). The hardcoded glyph tables
//! become [`Glyphs`](crate::theme::Glyphs) fields (D7); the owner back-pointer
//! plus its event payload become [`Context`] broadcasts that carry a resolvable
//! [`ViewId`](crate::view::ViewId) `source` (D3, D4).

use crate::capture::TrackMask;
use crate::command::Command;
use crate::data::FieldValue;
use crate::event::{Event, Key, MouseWheel, ctrl_to_arrow};
use crate::theme::Role;
use crate::view::{Context, DrawCtx, GrowMode, Point, Rect, View, ViewState};

// ---------------------------------------------------------------------------
// Scrollbar part codes
// ---------------------------------------------------------------------------

/// Which part of the scrollbar a point hits.
///
/// The vertical variants ([`UpArrow`](Part::UpArrow)/[`DownArrow`](Part::DownArrow)/
/// [`PageUp`](Part::PageUp)/[`PageDown`](Part::PageDown)) mirror the four
/// horizontal ones; orientation selects which set applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    /// Horizontal: left arrow.
    LeftArrow,
    /// Horizontal: right arrow.
    RightArrow,
    /// Horizontal: page left (trough left of thumb).
    PageLeft,
    /// Horizontal: page right (trough right of thumb).
    PageRight,
    /// Vertical: up arrow.
    UpArrow,
    /// Vertical: down arrow.
    DownArrow,
    /// Vertical: page up (trough above thumb).
    PageUp,
    /// Vertical: page down (trough below thumb).
    PageDown,
    /// The thumb itself.
    Indicator,
}

impl Part {
    /// The scroll step for this part: the arrow step for arrows, the page step
    /// for troughs, negated for the back (left/up) direction.
    fn scroll_step(self, ar_step: i32, pg_step: i32) -> i32 {
        let is_page = matches!(
            self,
            Part::PageLeft | Part::PageRight | Part::PageUp | Part::PageDown
        );
        let step = if is_page { pg_step } else { ar_step };
        let is_forward = matches!(
            self,
            Part::RightArrow | Part::PageRight | Part::DownArrow | Part::PageDown
        );
        if is_forward { step } else { -step }
    }
}

// ---------------------------------------------------------------------------
// ScrollBar
// ---------------------------------------------------------------------------

/// A single-axis scroll-bar widget: a draggable thumb between two step-arrows,
/// with paging troughs on either side.
///
/// Orientation is derived from bounds at construction (`size.x == 1` →
/// vertical, `size.y == 1` → horizontal). Construct with [`ScrollBar::new`],
/// then call [`set_params`](Self::set_params) (or the convenience wrappers) to
/// configure the range. When the user interacts with the bar it broadcasts
/// [`Command::SCROLL_BAR_CHANGED`]; a paired scroller or list viewer listens
/// for this broadcast and syncs its own position. See the [module-level
/// documentation](self) for the full broadcast and press-and-hold protocol.
///
/// # Turbo Vision heritage
///
/// Ports `TScrollBar` (`tscrlbar.cpp`).
pub struct ScrollBar {
    /// View state (geometry, flags, etc.) — the composition target.
    pub state: ViewState,
    /// Current scroll position within `[min_value, max_value]`. Starts at `0`.
    ///
    /// This field is readable but treat it as read-only from outside the widget:
    /// use [`set_value`](Self::set_value) (or [`set_params`](Self::set_params))
    /// to change the position so the broker broadcast fires and the display
    /// stays consistent. The broker downcast path reads this field directly via
    /// [`View::value`].
    pub value: i32,
    /// Inclusive lower bound of the scroll range. Starts at `0`.
    ///
    /// To update the range, call [`set_range`](Self::set_range) or
    /// [`set_params`](Self::set_params) — they clamp `value` and fire the
    /// [`Command::SCROLL_BAR_CHANGED`] broadcast.
    pub min_value: i32,
    /// Inclusive upper bound of the scroll range. Starts at `0`.
    ///
    /// To update the range, call [`set_range`](Self::set_range) or
    /// [`set_params`](Self::set_params) — they clamp `value` and fire the
    /// [`Command::SCROLL_BAR_CHANGED`] broadcast.
    pub max_value: i32,
    /// Distance the thumb moves when the user clicks the trough (page area).
    /// Starts at `1`. To update both steps together, call
    /// [`set_step`](Self::set_step) or [`set_params`](Self::set_params).
    pub page_step: i32,
    /// Distance the thumb moves when the user clicks an arrow or presses an
    /// arrow key. Starts at `1`. To update both steps together, call
    /// [`set_step`](Self::set_step) or [`set_params`](Self::set_params).
    pub arrow_step: i32,
    /// Whether this is a vertical bar (`size.x == 1`). Derived at construction.
    vertical: bool,
    /// Absolute screen position of scrollbar-local `(0, 0)`, cached each `draw`
    /// so the mouse-tracking capture can convert absolute mouse coords to
    /// bar-local (the `Button::abs_origin` pattern).
    abs_origin: Point,
    /// Whether a mouse hold-track is in flight. Guards the `MouseAuto` /
    /// `MouseMove` / `MouseUp` tracking arms against stray (untracked) events.
    tracking: bool,
    /// The part that was clicked to start an arrow/page hold-track. `None` for
    /// the thumb/default branch. The part code under the cursor is re-derived on
    /// each auto tick and compared against this to decide whether to keep stepping.
    tracked_part: Option<Part>,
}

impl ScrollBar {
    /// Create a scrollbar sized to `bounds`.
    ///
    /// Orientation is inferred at construction: `bounds.size.x == 1` means
    /// vertical; `bounds.size.y == 1` means horizontal. The grow mode is set
    /// accordingly so the bar stays pinned to its owner's edge as the owner
    /// resizes.
    ///
    /// All scroll state starts at zero (`value`, `min_value`, `max_value`) and
    /// both step sizes start at `1`. Call [`set_params`](Self::set_params) (or
    /// the convenience wrappers [`set_range`](Self::set_range) /
    /// [`set_step`](Self::set_step)) after inserting the bar into a group to
    /// configure the live range.
    ///
    /// The bar is not selectable by default (no [`ofSelectable`] flag), which is
    /// intentional: mouse events are delivered directly by the group, and
    /// keyboard events reach it only through post-processing (enabled via
    /// [`with_keyboard`](Self::with_keyboard)).
    ///
    /// Arrow/page glyph characters are read from [`Context::glyphs`] at draw
    /// time — they are not stored on the widget itself.
    ///
    /// [`ofSelectable`]: crate::view::ViewOptions::selectable
    pub fn new(bounds: Rect) -> Self {
        let mut state = ViewState::new(bounds);
        let vertical = state.size.x == 1;

        // Grow mode per orientation: a vertical bar tracks the right edge and
        // full height; a horizontal bar tracks the bottom edge and full width.
        if vertical {
            state.grow_mode = GrowMode {
                lo_x: true,
                hi_x: true,
                hi_y: true,
                ..Default::default()
            };
        } else {
            state.grow_mode = GrowMode {
                lo_y: true,
                hi_x: true,
                hi_y: true,
                ..Default::default()
            };
        }

        // Not selectable (no options set). Mouse events are delivered directly;
        // keyboard events reach it only via post-processing, which
        // standard_scroll_bar enables when ScrollBarOptions::handle_keyboard is true.

        ScrollBar {
            state,
            value: 0,
            min_value: 0,
            max_value: 0,
            page_step: 1,
            arrow_step: 1,
            vertical,
            abs_origin: Point::new(0, 0),
            tracking: false,
            tracked_part: None,
        }
    }

    /// Enable keyboard scrolling on this bar.
    ///
    /// Sets the post-process option (`ofPostProcess`) so the bar receives
    /// arrow / page / Home / End keys from the focused chain even when the bar
    /// itself is not the current view. Use this when you want the keyboard to
    /// scroll a pane that has its own focused content (e.g. an editor with an
    /// attached vertical scrollbar).
    ///
    /// Call this as a builder on [`new`](Self::new):
    /// ```ignore
    /// let bar = ScrollBar::new(bounds).with_keyboard();
    /// ```
    /// `Window::standard_scroll_bar` passes `handle_keyboard: true` to achieve
    /// the same effect automatically.
    pub fn with_keyboard(mut self) -> Self {
        self.state.options.post_process = true;
        self
    }

    /// Whether this bar is oriented vertically (width == 1).
    pub fn is_vertical(&self) -> bool {
        self.vertical
    }

    // -----------------------------------------------------------------------
    // Value / range logic (ports setParams / setValue / setRange / setStep)
    // -----------------------------------------------------------------------

    /// Update value, range, and step sizes atomically.
    ///
    /// `a_max` is floored to `a_min`; `a_value` is clamped to `[a_min, a_max]`.
    /// If `value` changed, broadcasts [`Command::SCROLL_BAR_CHANGED`] so that
    /// a linked scroller or list viewer can sync its own position (see the
    /// [module-level broadcast docs](self)). Steps are set unconditionally at
    /// the end, regardless of whether the value or range changed.
    ///
    /// The display is repainted automatically on the next whole-tree redraw pass
    /// — there is no explicit `draw_view()` call here (unlike the C++
    /// `TScrollBar::setParams`, which called `drawView()` before broadcasting).
    /// The convenience wrappers [`set_value`](Self::set_value),
    /// [`set_range`](Self::set_range), and [`set_step`](Self::set_step) all
    /// forward to this method.
    pub fn set_params(
        &mut self,
        a_value: i32,
        a_min: i32,
        a_max: i32,
        a_pg_step: i32,
        a_ar_step: i32,
        ctx: &mut Context,
    ) {
        let a_max = a_max.max(a_min);
        let a_value = a_value.clamp(a_min, a_max);
        let old_value = self.value;

        if old_value != a_value || self.min_value != a_min || self.max_value != a_max {
            self.value = a_value;
            self.min_value = a_min;
            self.max_value = a_max;
            if old_value != a_value {
                self.scroll_draw(ctx);
            }
        }
        self.page_step = a_pg_step;
        self.arrow_step = a_ar_step;
    }

    /// Move the scroll position to `a_value`, clamped to `[min_value, max_value]`.
    ///
    /// Broadcasts [`Command::SCROLL_BAR_CHANGED`] if the value actually changes.
    /// Use this when the owner has scrolled programmatically and wants the bar to
    /// reflect the new position. Forwards to [`set_params`](Self::set_params).
    pub fn set_value(&mut self, a_value: i32, ctx: &mut Context) {
        self.set_params(
            a_value,
            self.min_value,
            self.max_value,
            self.page_step,
            self.arrow_step,
            ctx,
        );
    }

    /// Update the `[min, max]` range, keeping value and step sizes.
    ///
    /// `value` is clamped to the new range and, if it changed, a
    /// [`Command::SCROLL_BAR_CHANGED`] broadcast is emitted. Call this when the
    /// total size of the scrollable content changes (e.g. the file being edited
    /// gains or loses lines). Forwards to [`set_params`](Self::set_params).
    pub fn set_range(&mut self, a_min: i32, a_max: i32, ctx: &mut Context) {
        self.set_params(
            self.value,
            a_min,
            a_max,
            self.page_step,
            self.arrow_step,
            ctx,
        );
    }

    /// Update the page and arrow step sizes, keeping value and range.
    ///
    /// Changing step sizes does not broadcast — only a `value` change triggers
    /// [`Command::SCROLL_BAR_CHANGED`]. Use this when the visible area of the
    /// scrollable content changes (e.g. the window is resized), since the page
    /// size typically tracks the viewport height. Forwards to
    /// [`set_params`](Self::set_params).
    pub fn set_step(&mut self, a_pg_step: i32, a_ar_step: i32, ctx: &mut Context) {
        self.set_params(
            self.value,
            self.min_value,
            self.max_value,
            a_pg_step,
            a_ar_step,
            ctx,
        );
    }

    /// Broadcast [`Command::SCROLL_BAR_CHANGED`], carrying this bar's id as the
    /// broadcast `source` so a two-bar owner can tell which bar fired.
    fn scroll_draw(&self, ctx: &mut Context) {
        ctx.broadcast(Command::SCROLL_BAR_CHANGED, self.state().id());
    }

    // -----------------------------------------------------------------------
    // Position / size math
    // -----------------------------------------------------------------------

    /// The scrollbar's active length in cells: the axis length, at least 3.
    pub fn get_size(&self) -> i32 {
        let s = if self.vertical {
            self.state.size.y
        } else {
            self.state.size.x
        };
        s.max(3)
    }

    /// The thumb position: a 1-based cell index along the bar's axis
    /// (`1..=get_size()-2` inclusive).
    ///
    /// With an empty range the thumb sits at 1; otherwise the value is mapped
    /// linearly across the inner cells (`get_size() - 3`), rounded to nearest.
    pub fn get_pos(&self) -> i32 {
        let r = self.max_value - self.min_value;
        if r == 0 {
            1
        } else {
            ((i64::from(self.value - self.min_value) * i64::from(self.get_size() - 3)
                + i64::from(r >> 1))
                / i64::from(r)) as i32
                + 1
        }
    }

    // -----------------------------------------------------------------------
    // Part classification
    // -----------------------------------------------------------------------

    /// Classify which [`Part`] a point hits.
    ///
    /// `mark` is the position along the scrollbar axis (x for horizontal,
    /// y for vertical), in view-local coords.  `pos` is the current thumb
    /// cell index. `s` is [`get_size`](Self::get_size)` - 1`.
    ///
    /// Returns `None` if the point is outside the view extent.
    fn get_part_code(&self, mark: i32, pos: i32, s: i32) -> Option<Part> {
        if mark == pos {
            Some(Part::Indicator)
        } else if mark < 1 {
            if self.vertical {
                Some(Part::UpArrow)
            } else {
                Some(Part::LeftArrow)
            }
        } else if mark < pos {
            if self.vertical {
                Some(Part::PageUp)
            } else {
                Some(Part::PageLeft)
            }
        } else if mark < s {
            if self.vertical {
                Some(Part::PageDown)
            } else {
                Some(Part::PageRight)
            }
        } else {
            if self.vertical {
                Some(Part::DownArrow)
            } else {
                Some(Part::RightArrow)
            }
        }
    }
}

impl View for ScrollBar {
    fn state(&self) -> &ViewState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }

    /// The bar's `value` exposed as the typed transfer currency — a scroller,
    /// list viewer, or editor read-broker reads this through the trait (the pump
    /// resolves the bar by id and reads `value`).
    fn value(&self) -> Option<FieldValue> {
        Some(FieldValue::Int(self.value))
    }

    /// Concrete-reach hatch (the sanctioned downcast): the pump downcasts to
    /// `&mut ScrollBar` to call `set_params` when applying a
    /// `Deferred::ScrollBarSetParams` from a scroller.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    /// Paint the scrollbar into its one-cell-wide (or one-cell-tall) bounds.
    ///
    /// The bar is divided into five logical zones (shown for vertical; horizontal
    /// mirrors on the x-axis):
    ///
    /// | Cell(s)         | Glyph source                    | Theme role                      |
    /// |-----------------|---------------------------------|---------------------------------|
    /// | 0               | `glyphs.sb_v_arrow_back`        | [`Role::ScrollBarControls`]     |
    /// | 1 .. pos-1      | `glyphs.sb_page`                | [`Role::ScrollBarPage`]         |
    /// | pos             | `glyphs.sb_thumb`               | [`Role::ScrollBarControls`]     |
    /// | pos+1 .. s-1    | `glyphs.sb_page`                | [`Role::ScrollBarPage`]         |
    /// | s               | `glyphs.sb_v_arrow_fwd`         | [`Role::ScrollBarControls`]     |
    ///
    /// When `min_value == max_value` (empty range), every inner cell is drawn
    /// with `glyphs.sb_page_no_range` and the thumb is omitted.  Glyphs are
    /// read from [`DrawCtx::glyphs`] at paint time and are never stored on the
    /// widget. This replaces the C++ `drawPos` helper arrays and the hardcoded
    /// `vChars`/`hChars` tables.
    fn draw(&mut self, ctx: &mut DrawCtx) {
        // Cache absolute origin for the mouse-tracking capture: the
        // MouseTrackCapture converts absolute mouse coords to bar-local via this
        // value, mirroring the Button `abs_origin` pattern.
        self.abs_origin = ctx.origin();
        let glyphs = *ctx.glyphs();
        let page_style = ctx.style(Role::ScrollBarPage);
        let ctrl_style = ctx.style(Role::ScrollBarControls);

        let s = self.get_size() - 1; // last cell index
        let pos = self.get_pos();
        let no_range = self.max_value == self.min_value;

        if self.vertical {
            // Draw each row of the 1×height bar.
            for row in 0..=s {
                let (ch, style) = if row == 0 {
                    (glyphs.sb_v_arrow_back, ctrl_style)
                } else if row == s {
                    (glyphs.sb_v_arrow_fwd, ctrl_style)
                } else if no_range {
                    (glyphs.sb_page_no_range, page_style)
                } else if row == pos {
                    (glyphs.sb_thumb, ctrl_style)
                } else {
                    (glyphs.sb_page, page_style)
                };
                ctx.put_char(0, row, ch, style);
            }
        } else {
            // Draw each column of the width×1 bar.
            for col in 0..=s {
                let (ch, style) = if col == 0 {
                    (glyphs.sb_h_arrow_back, ctrl_style)
                } else if col == s {
                    (glyphs.sb_h_arrow_fwd, ctrl_style)
                } else if no_range {
                    (glyphs.sb_page_no_range, page_style)
                } else if col == pos {
                    (glyphs.sb_thumb, ctrl_style)
                } else {
                    (glyphs.sb_page, page_style)
                };
                ctx.put_char(col, 0, ch, style);
            }
        }
    }

    /// Keyboard and mouse input.
    ///
    /// Handles:
    /// - Mouse wheel: adjust by `3 * arrow_step`, broadcasting clicked then
    ///   changed (via `set_value`).
    /// - Mouse down: classify the part hit. Arrow parts do a first step and arm
    ///   an auto-repeat track (`TrackMask { mouse_auto: true }`). The
    ///   indicator/default branch does a first thumb-jump and arms a move track
    ///   (`TrackMask { mouse_move: true }`). Broadcasts clicked.
    /// - Tracked auto tick: re-derive the part; step only if it still matches the
    ///   originally-clicked part (the arrow/page hold loop body).
    /// - Tracked move: recompute the thumb value from the cursor position (the
    ///   drag loop body).
    /// - Tracked mouse up: clear the tracking flag.
    /// - Key down (when visible + focused): arrow/page/home/end keys; broadcasts
    ///   clicked then adjusts the value.
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        let visible = self.state.state.visible;

        match *ev {
            // ------------------------------------------------------------------
            // Mouse wheel (evMouseWheel) — visible check matches C++ `sfVisible`
            // ------------------------------------------------------------------
            Event::MouseWheel(me) if visible => {
                let step = if self.vertical {
                    match me.wheel {
                        MouseWheel::Up => -self.arrow_step,
                        MouseWheel::Down => self.arrow_step,
                        _ => 0,
                    }
                } else {
                    match me.wheel {
                        MouseWheel::Left => -self.arrow_step,
                        MouseWheel::Right => self.arrow_step,
                        _ => 0,
                    }
                };
                if step != 0 {
                    ctx.broadcast(Command::SCROLL_BAR_CLICKED, self.state().id());
                    self.set_value(self.value + 3 * step, ctx);
                    ev.clear();
                }
            }

            // ------------------------------------------------------------------
            // Mouse down (evMouseDown) — tscrlbar.cpp:173-210
            // ------------------------------------------------------------------
            Event::MouseDown(me) => {
                ctx.broadcast(Command::SCROLL_BAR_CLICKED, self.state().id());

                // C++:173-179 — capture mouse, extent, pos, size before the
                // loop (these become `s` / `p` / `extent` globals in C++).
                let local = me.position; // already in view-local coords
                let mark = if self.vertical { local.y } else { local.x };
                let pos = self.get_pos(); // C++ `p = getPos()`
                let s = self.get_size() - 1; // C++ `s = getSize() - 1`

                // C++:176-178: `extent = getExtent(); extent.grow(1, 1);`
                // getPartCode returns -1 (None here) when outside this expanded extent.
                let extent = self.state.get_extent();
                let expanded = Rect::new(
                    extent.a.x - 1,
                    extent.a.y - 1,
                    extent.b.x + 1,
                    extent.b.y + 1,
                );
                // C++:180 — `clickPart = getPartCode()`
                let click_part = if expanded.contains(local) {
                    self.get_part_code(mark, pos, s)
                } else {
                    None // C++ getPartCode() == -1 → falls into default: (thumb-jump)
                };

                match click_part {
                    // Arrow branch: do the first step (the loop body runs once
                    // before the first wait), then arm auto-repeat.
                    Some(
                        p @ (Part::LeftArrow | Part::RightArrow | Part::UpArrow | Part::DownArrow),
                    ) => {
                        // C++:188-190: `mouse = makeLocal(…); if getPartCode()==clickPart
                        // setValue(value + scrollStep(clickPart))`.
                        // First iteration: the click position IS the arrow, so the check
                        // always passes on the first iteration.
                        self.set_value(
                            self.value + p.scroll_step(self.arrow_step, self.page_step),
                            ctx,
                        );
                        // Enter the loop: arm auto-repeat. The C++ loop re-classifies
                        // the part under the cursor on each `evMouseAuto` tick.
                        if let Some(id) = self.state.id() {
                            self.tracking = true;
                            self.tracked_part = Some(p);
                            ctx.start_mouse_track(
                                id,
                                self.abs_origin,
                                TrackMask {
                                    mouse_auto: true,
                                    ..Default::default()
                                },
                            );
                        }
                    }
                    // C++:193-207 — default branch (page, indicator, out-of-extent):
                    // move the thumb to the cursor position. First iteration of the
                    // `evMouseMove` drag loop.
                    _ => {
                        // C++:195-205: `i = clamp(mouse.y or .x, 1, s-1);
                        // if s>2: setValue(((p-1)*(max-min) + ((s-2)>>1)) / (s-2) + min)`
                        let i = mark.max(1).min(s - 1);
                        if s > 2 {
                            let new_val = (i64::from(i - 1)
                                * i64::from(self.max_value - self.min_value)
                                + i64::from((s - 2) >> 1))
                                / i64::from(s - 2)
                                + i64::from(self.min_value);
                            self.set_value(new_val as i32, ctx);
                        }
                        // Enter the drag loop: arm move-tracking.
                        if let Some(id) = self.state.id() {
                            self.tracking = true;
                            self.tracked_part = None; // thumb branch: no part to re-match
                            ctx.start_mouse_track(
                                id,
                                self.abs_origin,
                                TrackMask {
                                    mouse_move: true,
                                    ..Default::default()
                                },
                            );
                        }
                    }
                }

                // Always consume the event after a mouse-down.
                ev.clear();
            }

            // ------------------------------------------------------------------
            // Mouse auto (evMouseAuto) — the arrow hold-loop body. Guarded by
            // `tracking` AND `tracked_part.is_some()`: there are two SEPARATE
            // masked loops (auto-only for arrows, move-only for the thumb), so an
            // auto event during a thumb track must fall through, not hit this arm.
            // ------------------------------------------------------------------
            Event::MouseAuto(me) if self.tracking && self.tracked_part.is_some() => {
                // Position is already bar-local (the capture localized it via abs_origin).
                let local = me.position;
                let mark = if self.vertical { local.y } else { local.x };
                let pos = self.get_pos(); // re-derive the thumb position
                let s = self.get_size() - 1;
                // Re-classify under the current cursor and step only if the mouse
                // is still over the originally-clicked arrow part.
                let cur_part = self.get_part_code(mark, pos, s);
                if cur_part == self.tracked_part
                    && let Some(p) = self.tracked_part
                {
                    // C++:190: `setValue(value + scrollStep(clickPart))`
                    self.set_value(
                        self.value + p.scroll_step(self.arrow_step, self.page_step),
                        ctx,
                    );
                }
                ev.clear();
            }

            // ------------------------------------------------------------------
            // Mouse move (evMouseMove) — the thumb drag-loop body,
            // tscrlbar.cpp:195-205. Guarded by `tracking` AND
            // `tracked_part.is_none()`: mirrors the C++'s two separate masked
            // loops — a move event during an arrow track must fall through.
            // ------------------------------------------------------------------
            Event::MouseMove(me) if self.tracking && self.tracked_part.is_none() => {
                // C++:195: `mouse = makeLocal(event.mouse.where)` — position is
                // already bar-local (the capture localized it via abs_origin).
                let local = me.position;
                let mark = if self.vertical { local.y } else { local.x };
                let s = self.get_size() - 1;
                // C++:197-201: `i = mouse.y or .x; clamp to [1, s-1]`
                let i = mark.max(1).min(s - 1);
                // C++:203-204: `if s > 2: setValue(((p-1)*(max-min)+…)/… + min)`
                if s > 2 {
                    let new_val = (i64::from(i - 1) * i64::from(self.max_value - self.min_value)
                        + i64::from((s - 2) >> 1))
                        / i64::from(s - 2)
                        + i64::from(self.min_value);
                    self.set_value(new_val as i32, ctx);
                }
                ev.clear();
            }

            // ------------------------------------------------------------------
            // Mouse up — post-loop code (both branches). Guarded by `tracking`
            // (MouseUp is not mask-gated in Group::wants, so a stray up must
            // fall through). tscrlbar.cpp:191 / :206 — loop exits on MouseUp.
            // ------------------------------------------------------------------
            Event::MouseUp(_) if self.tracking => {
                self.tracking = false;
                self.tracked_part = None;
                ev.clear();
            }

            // ------------------------------------------------------------------
            // Key down (evKeyDown) — only when visible (sfVisible check)
            // ------------------------------------------------------------------
            Event::KeyDown(ke) if visible => {
                // `switch (ctrlToArrow(event.keyDown.keyCode))` — WordStar
                // Ctrl-letter nav aliases (Ctrl+S→Left, Ctrl+E→Up, …). The
                // helper only maps Ctrl+letter (modifiers cleared on a match);
                // Ctrl+arrow combos pass through unchanged, so the `ctrl`
                // reads below still see them.
                let ke = ctrl_to_arrow(ke);
                let key = ke.key;
                let ctrl = ke.modifiers.ctrl;

                // Map key to a part code (or a direct target value).
                // Horizontal bar (size.y==1) uses Left/Right/Ctrl+Left/Ctrl+Right,
                //   and Home/End jump to min/max.
                // Vertical bar uses Up/Down/PgUp/PgDn and
                //   Ctrl+PgUp/PgDn jump to min/max.
                let action: Option<PartOrValue> = if !self.vertical {
                    // Horizontal scrollbar key mapping (faithful to C++ size.y==1 branch).
                    match (key, ctrl) {
                        (Key::Left, false) => Some(PartOrValue::P(Part::LeftArrow)),
                        (Key::Right, false) => Some(PartOrValue::P(Part::RightArrow)),
                        (Key::Left, true) => Some(PartOrValue::P(Part::PageLeft)),
                        (Key::Right, true) => Some(PartOrValue::P(Part::PageRight)),
                        (Key::Up, true) => Some(PartOrValue::P(Part::PageUp)),
                        (Key::Down, true) => Some(PartOrValue::P(Part::PageDown)),
                        (Key::Home, _) => Some(PartOrValue::V(self.min_value)),
                        (Key::End, _) => Some(PartOrValue::V(self.max_value)),
                        _ => None,
                    }
                } else {
                    // Vertical scrollbar key mapping (faithful to C++ size.y!=1 branch).
                    match (key, ctrl) {
                        (Key::Up, false) => Some(PartOrValue::P(Part::UpArrow)),
                        (Key::Down, false) => Some(PartOrValue::P(Part::DownArrow)),
                        (Key::PageUp, false) => Some(PartOrValue::P(Part::PageUp)),
                        (Key::PageDown, false) => Some(PartOrValue::P(Part::PageDown)),
                        (Key::PageUp, true) => Some(PartOrValue::V(self.min_value)),
                        (Key::PageDown, true) => Some(PartOrValue::V(self.max_value)),
                        _ => None,
                    }
                };

                if let Some(act) = action {
                    ctx.broadcast(Command::SCROLL_BAR_CLICKED, self.state().id());
                    let new_val = match act {
                        PartOrValue::P(part) => {
                            self.value + part.scroll_step(self.arrow_step, self.page_step)
                        }
                        PartOrValue::V(v) => v,
                    };
                    self.set_value(new_val, ctx);
                    ev.clear();
                }
            }

            _ => {}
        }
    }
}

/// Helper: a key event action is either a part (relative step) or a direct value.
enum PartOrValue {
    P(Part),
    V(i32),
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::event::{KeyEvent, KeyModifiers, MouseButtons, MouseEvent, MouseEventFlags};
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use crate::view::Point;
    use std::collections::VecDeque;

    // -- Helper to build a Context with local backing storage ----------------

    fn make_ctx<'a>(
        out: &'a mut VecDeque<Event>,
        timers: &'a mut crate::timer::TimerQueue,
        deferred: &'a mut Vec<crate::view::Deferred>,
    ) -> Context<'a> {
        Context::new(out, timers, 0, deferred)
    }

    // -- Helpers for building key events -------------------------------------

    fn key_ev(key: Key) -> Event {
        Event::KeyDown(KeyEvent::from(key))
    }

    fn ctrl_key_ev(key: Key) -> Event {
        Event::KeyDown(KeyEvent::new(
            key,
            KeyModifiers {
                ctrl: true,
                ..Default::default()
            },
        ))
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
            modifiers: KeyModifiers::default(),
        })
    }

    fn mouse_auto_at(x: i32, y: i32) -> Event {
        Event::MouseAuto(MouseEvent {
            position: Point::new(x, y),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        })
    }

    fn mouse_move_at(x: i32, y: i32) -> Event {
        Event::MouseMove(MouseEvent {
            position: Point::new(x, y),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        })
    }

    fn mouse_up_at(x: i32, y: i32) -> Event {
        Event::MouseUp(MouseEvent {
            position: Point::new(x, y),
            ..Default::default()
        })
    }

    /// Stamp a fresh ViewId onto a scrollbar (simulating Group::insert).
    fn stamp_id(sb: &mut ScrollBar) -> crate::view::ViewId {
        let id = crate::view::ViewId::next();
        sb.state.id = Some(id);
        id
    }

    // -----------------------------------------------------------------------
    // set_value clamps and broadcasts
    // -----------------------------------------------------------------------

    #[test]
    fn set_value_clamps_to_range() {
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(5, 0, 20, 1, 1, &mut ctx);
        }
        assert_eq!(sb.value, 5);
        assert_eq!(sb.min_value, 0);
        assert_eq!(sb.max_value, 20);

        // Clamp above max.
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_value(100, &mut ctx);
        }
        assert_eq!(sb.value, 20);

        // Clamp below min.
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_value(-5, &mut ctx);
        }
        assert_eq!(sb.value, 0);
    }

    #[test]
    fn set_value_broadcasts_changed_on_value_change() {
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(0, 0, 20, 1, 1, &mut ctx);
        }
        // No broadcast yet — value didn't change (was 0, still 0).
        assert_eq!(out.len(), 0);

        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_value(5, &mut ctx);
        }
        assert_eq!(out.len(), 1);
        assert!(matches!(
            out[0],
            Event::Broadcast { command, .. } if command == Command::SCROLL_BAR_CHANGED
        ));
    }

    #[test]
    fn broadcast_source_is_the_inserted_scrollbars_id() {
        // The `cmScrollBarChanged` broadcast must carry `source == the emitting
        // scrollbar's id` (the C++ `this`), not `None`. The id is
        // stamped at `Group::insert`, so the scrollbar must be inserted first.
        use crate::view::Group;

        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];

        // Build a scrollbar with a real range, then insert it into a group so it
        // is assigned a process-global id.
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(5, 0, 20, 1, 1, &mut ctx);
        }
        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        let id = group.insert(Box::new(sb));
        out.clear();

        // Drive a value change on the inserted scrollbar (a vertical bar accepts
        // Key::Down) and capture its broadcast. We send the key straight to the
        // resolved child via the `View` trait — we only care that the *emitter*
        // threads its own stamped id, not about group focus routing.
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            let child = group.find_mut(id).expect("scrollbar resolves by id");
            let mut ev = key_ev(Key::Down);
            child.handle_event(&mut ev, &mut ctx);
        }

        // The queued CHANGED broadcast must name the scrollbar as its source.
        assert!(
            out.iter().any(|e| matches!(
                e,
                Event::Broadcast { command, source }
                    if *command == Command::SCROLL_BAR_CHANGED && *source == Some(id)
            )),
            "scroll-bar broadcast must carry source == the emitting scrollbar's id, not None"
        );
    }

    #[test]
    fn set_value_no_broadcast_when_value_unchanged() {
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(5, 0, 20, 1, 1, &mut ctx);
        }
        out.clear();
        // Setting same value should not broadcast.
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_value(5, &mut ctx);
        }
        assert_eq!(out.len(), 0);
    }

    // -----------------------------------------------------------------------
    // set_params / set_range / set_step
    // -----------------------------------------------------------------------

    #[test]
    fn set_range_clamps_value_if_out_of_new_range() {
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(15, 0, 20, 1, 1, &mut ctx);
        }
        assert_eq!(sb.value, 15);
        out.clear();
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_range(0, 10, &mut ctx);
        }
        assert_eq!(sb.value, 10, "value clamped to new max");
        assert_eq!(out.len(), 1);
        assert!(matches!(
            out[0],
            Event::Broadcast { command, .. } if command == Command::SCROLL_BAR_CHANGED
        ));
    }

    #[test]
    fn set_step_updates_steps_without_broadcast() {
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_step(5, 2, &mut ctx);
        }
        assert_eq!(sb.page_step, 5);
        assert_eq!(sb.arrow_step, 2);
        assert_eq!(out.len(), 0);
    }

    // -----------------------------------------------------------------------
    // with_keyboard builder
    // -----------------------------------------------------------------------

    #[test]
    fn with_keyboard_sets_post_process() {
        let r = Rect::new(0, 0, 1, 10);
        // A plain new() bar must NOT have post_process (the default is false;
        // no options are set).
        let sb_plain = ScrollBar::new(r);
        assert!(
            !sb_plain.state.options.post_process,
            "plain ScrollBar::new must NOT have post_process"
        );
        // with_keyboard() sets it.
        let sb_kb = ScrollBar::new(r).with_keyboard();
        assert!(
            sb_kb.state.options.post_process,
            "with_keyboard() must set post_process (ofPostProcess / sbHandleKeyboard)"
        );
    }

    // -----------------------------------------------------------------------
    // get_pos / get_size math
    // -----------------------------------------------------------------------

    #[test]
    fn get_size_minimum_is_3() {
        let sb = ScrollBar::new(Rect::new(0, 0, 1, 2)); // height 2 < 3
        assert_eq!(sb.get_size(), 3);
        let sb2 = ScrollBar::new(Rect::new(0, 0, 1, 10));
        assert_eq!(sb2.get_size(), 10);
    }

    #[test]
    fn get_pos_returns_1_when_range_is_zero() {
        let sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        // min == max == 0, value == 0
        assert_eq!(sb.get_pos(), 1);
    }

    #[test]
    fn get_pos_at_max_is_size_minus_2() {
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(100, 0, 100, 1, 1, &mut ctx);
        }
        // getPos: ((100-0)*(10-3) + 50) / 100 + 1 = (700+50)/100+1 = 7+1 = 8
        // size-1 = 9, size-2 = 8 — thumb at position 8.
        assert_eq!(sb.get_pos(), sb.get_size() - 2);
    }

    #[test]
    fn get_pos_midpoint() {
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(50, 0, 100, 1, 1, &mut ctx);
        }
        // getPos: (50*(10-3) + 50) / 100 + 1 = (350+50)/100 + 1 = 4+1 = 5
        assert_eq!(sb.get_pos(), 5);
    }

    // -----------------------------------------------------------------------
    // Key event scrolls one step
    // -----------------------------------------------------------------------

    #[test]
    fn key_up_scrolls_vertical_bar_by_arrow_step() {
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(10, 0, 100, 5, 1, &mut ctx);
        }
        out.clear();

        let mut ev = key_ev(Key::Up);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "event consumed");
        assert_eq!(sb.value, 9, "arrow_step 1 → value decremented by 1");
        // Should have broadcast CLICKED then CHANGED.
        assert!(out.iter().any(|e| matches!(
            e,
            Event::Broadcast { command, .. } if *command == Command::SCROLL_BAR_CLICKED
        )));
        assert!(out.iter().any(|e| matches!(
            e,
            Event::Broadcast { command, .. } if *command == Command::SCROLL_BAR_CHANGED
        )));
    }

    #[test]
    fn key_page_down_scrolls_by_page_step() {
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(10, 0, 100, 5, 1, &mut ctx);
        }
        out.clear();

        let mut ev = key_ev(Key::PageDown);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing());
        assert_eq!(sb.value, 15, "page_step 5 → value incremented by 5");
    }

    #[test]
    fn key_home_jumps_to_min() {
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(50, 5, 100, 5, 1, &mut ctx);
        }
        out.clear();

        // The C++ uses Home/End for horizontal bars (size.y==1) and
        // Ctrl+PgUp/PgDn for vertical bars:
        // - size.y == 1 (horizontal): kbHome → i = minVal, kbEnd → i = maxVal
        // - vertical (size.x==1): kbCtrlPgUp → i = minVal, kbCtrlPgDn → i = maxVal
        // Our implementation is faithful to this. Verify Ctrl+PageUp → minVal for vertical.
        let mut ev = ctrl_key_ev(Key::PageUp);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing());
        assert_eq!(sb.value, 5, "ctrl+pageup → min_value");
    }

    #[test]
    fn horizontal_key_right_scrolls() {
        let mut sb = ScrollBar::new(Rect::new(0, 0, 20, 1)); // horizontal
        assert!(!sb.is_vertical());
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(10, 0, 100, 5, 2, &mut ctx);
        }
        out.clear();

        let mut ev = key_ev(Key::Right);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing());
        assert_eq!(sb.value, 12, "arrow_step 2 → value incremented by 2");
    }

    /// `ctrlToArrow`: Ctrl+S is the WordStar alias for Left (`tscrlbar.cpp`
    /// wraps the keyCode through `ctrlToArrow` before the switch).
    #[test]
    fn ctrl_s_aliases_left_on_horizontal_bar() {
        let mut sb = ScrollBar::new(Rect::new(0, 0, 20, 1)); // horizontal
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(10, 0, 100, 5, 2, &mut ctx);
        }
        out.clear();

        // Ctrl+S → Left (the helper clears the ctrl modifier on the match,
        // so this is LeftArrow, not Ctrl+Left = PageLeft).
        let mut ev = ctrl_key_ev(Key::Char('s'));
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "Ctrl+S is consumed like Left");
        assert_eq!(sb.value, 8, "arrow_step 2 → value decremented by 2");
    }

    // -----------------------------------------------------------------------
    // Mouse click on arrow cell scrolls one step
    // -----------------------------------------------------------------------

    #[test]
    fn mouse_click_on_up_arrow_decrements() {
        // Vertical bar: height 10, value at 10.
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(10, 0, 100, 5, 1, &mut ctx);
        }
        out.clear();

        // Click at (0, 0) = the up-arrow cell.
        let mut ev = mouse_down_at(0, 0);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing());
        assert_eq!(sb.value, 9);
        assert!(out.iter().any(|e| matches!(
            e,
            Event::Broadcast { command, .. } if *command == Command::SCROLL_BAR_CLICKED
        )));
    }

    #[test]
    fn mouse_click_on_down_arrow_increments() {
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(10, 0, 100, 5, 1, &mut ctx);
        }
        out.clear();

        // Click at (0, 9) = the down-arrow cell (s = getSize()-1 = 9).
        let mut ev = mouse_down_at(0, 9);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing());
        assert_eq!(sb.value, 11);
    }

    /// A mouse-down in the page/trough area must produce a **thumb-jump** to
    /// the clicked position, NOT a page-step.
    ///
    /// Setup: vertical 1×10, value=50 of [0,100], page_step=5.
    ///   get_pos() = 5, s = 9.
    ///   Click y=2 → PageUp region.
    ///   Thumb-jump formula: i=2, ((2-1)*100 + (7>>1)) / 7 + 0 = 103/7 = 14.
    ///   Page-step would give 50-5 = 45.
    ///   Arrow-step would give 50-1 = 49.
    #[test]
    fn mouse_click_on_page_area_thumb_jumps_not_page_steps() {
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            // value=50, min=0, max=100, page_step=5, arrow_step=1
            sb.set_params(50, 0, 100, 5, 1, &mut ctx);
        }
        out.clear();

        // get_pos() = ((50-0)*(10-3) + 50) / 100 + 1 = (350+50)/100 + 1 = 5
        // s = get_size()-1 = 9
        // Click y=2: this is in the PageUp region (1 <= 2 < 5 = pos).
        let mut ev = mouse_down_at(0, 2);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "event consumed");
        // Thumb-jump: i=2, ((2-1)*100 + ((9-2)>>1)) / (9-2) + 0
        //           = (100 + 3) / 7 + 0 = 103/7 = 14 (integer division)
        assert_eq!(
            sb.value, 14,
            "page-area click must thumb-jump (not page-step: 45, not arrow-step: 49)"
        );
        // Must broadcast CLICKED (not just CHANGED).
        assert!(
            out.iter().any(|e| matches!(
                e,
                Event::Broadcast { command, .. } if *command == Command::SCROLL_BAR_CLICKED
            )),
            "SCROLL_BAR_CLICKED must be broadcast on mouse-down"
        );
    }

    // -----------------------------------------------------------------------
    // Snapshot tests
    // -----------------------------------------------------------------------

    #[test]
    fn snapshot_vertical_scrollbar_at_midpoint() {
        let theme = Theme::classic_blue();

        // Vertical bar: 1×10, value at 50 of 100.
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(50, 0, 100, 5, 1, &mut ctx);
        }

        let (backend, screen) = HeadlessBackend::new(1, 10);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = sb.state.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            sb.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    #[test]
    fn snapshot_horizontal_scrollbar_at_start() {
        let theme = Theme::classic_blue();

        // Horizontal bar: 20×1, value at 0 of 100.
        let mut sb = ScrollBar::new(Rect::new(0, 0, 20, 1));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(0, 0, 100, 5, 1, &mut ctx);
        }

        let (backend, screen) = HeadlessBackend::new(20, 1);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = sb.state.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            sb.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    #[test]
    fn snapshot_vertical_scrollbar_no_range() {
        let theme = Theme::classic_blue();

        // Vertical bar with range == 0 (all trough with sb_page_no_range).
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 6));

        let (backend, screen) = HeadlessBackend::new(1, 6);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = sb.state.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            sb.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    // -----------------------------------------------------------------------
    // mouse-track — arrow hold-repeat (evMouseAuto)
    // -----------------------------------------------------------------------

    /// Mouse-down on the up-arrow does the first step AND arms tracking
    /// (`PushCapture` deferred, `tracking == true`, `tracked_part == UpArrow`).
    /// The single-shot first-step behavior is unchanged.
    #[test]
    fn track_arrow_mouse_down_arms_capture() {
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let _id = stamp_id(&mut sb);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(10, 0, 100, 5, 1, &mut ctx);
        }
        out.clear();
        deferred.clear();

        // Click at (0, 0) = the up-arrow cell.
        let mut ev = mouse_down_at(0, 0);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "mouse-down consumed");
        assert_eq!(sb.value, 9, "first step applied (10 - arStep=1 = 9)");
        assert!(sb.tracking, "tracking flag set");
        // A PushCapture must have been deferred.
        assert!(
            deferred
                .iter()
                .any(|d| matches!(d, crate::view::Deferred::PushCapture(_))),
            "PushCapture deferred for the arrow hold-track"
        );
    }

    /// `MouseAuto` while tracking on the up-arrow, cursor still over the arrow:
    /// value decrements again (the loop body repeats).
    #[test]
    fn track_arrow_auto_repeats_while_on_same_part() {
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let _id = stamp_id(&mut sb);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(10, 0, 100, 5, 1, &mut ctx);
        }
        // Arm the track via a MouseDown on the up-arrow.
        let mut ev = mouse_down_at(0, 0);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(sb.value, 9);
        out.clear();
        deferred.clear();

        // MouseAuto at (0, 0) — still over the up-arrow → another step.
        let mut ev = mouse_auto_at(0, 0);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "MouseAuto consumed");
        assert_eq!(sb.value, 8, "second step: 9 - 1 = 8");
        assert!(sb.tracking, "still tracking after auto");
    }

    /// `MouseAuto` while tracking on the up-arrow, cursor moved off to the page
    /// area: value must NOT change (tscrlbar.cpp:189 `getPartCode() == clickPart`).
    #[test]
    fn track_arrow_auto_pauses_when_off_part() {
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let _id = stamp_id(&mut sb);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(50, 0, 100, 5, 1, &mut ctx);
        }
        // Arm the track via a MouseDown on the up-arrow.
        let mut ev = mouse_down_at(0, 0);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        let value_after_first = sb.value;
        out.clear();
        deferred.clear();

        // MouseAuto at (0, 3) — moved into the page-up region, not the up-arrow.
        let mut ev = mouse_auto_at(0, 3);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "MouseAuto still consumed (modal hold)");
        assert_eq!(
            sb.value, value_after_first,
            "no step when cursor off the arrow"
        );
        assert!(sb.tracking, "still tracking");
    }

    /// `MouseUp` clears the tracking flag (post-loop code, tscrlbar.cpp:191).
    #[test]
    fn track_arrow_mouse_up_clears_tracking() {
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let _id = stamp_id(&mut sb);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(10, 0, 100, 5, 1, &mut ctx);
        }
        let mut ev = mouse_down_at(0, 0);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        assert!(sb.tracking);

        let mut ev = mouse_up_at(0, 0);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "MouseUp consumed");
        assert!(!sb.tracking, "tracking cleared on MouseUp");
    }

    /// A stray `MouseUp` (no tracking in flight) must fall through — the guard
    /// protects against untracked ups (MouseUp not mask-gated in Group::wants).
    #[test]
    fn track_stray_mouse_up_falls_through() {
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let _id = stamp_id(&mut sb);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        // No tracking armed.
        let mut ev = mouse_up_at(0, 0);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        assert!(
            !ev.is_nothing(),
            "stray MouseUp falls through (not consumed)"
        );
    }

    // -----------------------------------------------------------------------
    // mouse-track — thumb drag (evMouseMove)
    // -----------------------------------------------------------------------

    /// Mouse-down in the page/trough area arms a move-track (not auto-track).
    #[test]
    fn track_thumb_mouse_down_arms_move_capture() {
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let _id = stamp_id(&mut sb);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(50, 0, 100, 5, 1, &mut ctx);
        }
        deferred.clear();
        out.clear();

        // Click in the trough (y=3, which is in PageUp for value=50, s=9, pos=5).
        let mut ev = mouse_down_at(0, 3);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing());
        assert!(sb.tracking, "tracking armed for thumb drag");
        assert!(
            deferred
                .iter()
                .any(|d| matches!(d, crate::view::Deferred::PushCapture(_))),
            "PushCapture deferred for the thumb drag-track"
        );
    }

    /// `MouseMove` while tracking in the trough recomputes the value from cursor
    /// position (thumb drag loop body, tscrlbar.cpp:195-205).
    #[test]
    fn track_thumb_move_updates_value() {
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let _id = stamp_id(&mut sb);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(50, 0, 100, 5, 1, &mut ctx);
        }
        // Arm the drag-track via a MouseDown in the trough.
        let mut ev = mouse_down_at(0, 3);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        let value_after_down = sb.value;
        out.clear();
        deferred.clear();

        // MouseMove to y=7 (near the bottom): value should jump higher.
        // Formula: i=7, ((7-1)*100 + ((9-2)>>1)) / (9-2) + 0 = (600+3)/7 = 86.
        let mut ev = mouse_move_at(0, 7);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "tracked MouseMove consumed");
        assert!(sb.tracking, "still tracking after move");
        assert_ne!(sb.value, value_after_down, "value changed on move");
        // The exact value is (6*100+3)/7 = 603/7 = 86.
        assert_eq!(sb.value, 86);
    }

    /// The two track kinds are discriminated (two SEPARATE masked tracks): a
    /// `MouseAuto` during a THUMB track and a `MouseMove` during an ARROW track
    /// must both fall through unconsumed — they belong to the other track's mask,
    /// and only the capture's mask filtering makes them unreachable in normal flow.
    #[test]
    fn track_wrong_event_kind_falls_through() {
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = vec![];

        // -- Thumb track (tracked_part == None): MouseAuto must fall through.
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let _id = stamp_id(&mut sb);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(50, 0, 100, 5, 1, &mut ctx);
        }
        let mut ev = mouse_down_at(0, 3); // trough → thumb-drag track
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        assert!(sb.tracking && sb.tracked_part.is_none());
        let value_before = sb.value;

        let mut ev = mouse_auto_at(0, 0);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        assert!(
            !ev.is_nothing(),
            "MouseAuto during a thumb track falls through unconsumed"
        );
        assert_eq!(sb.value, value_before, "…and changes nothing");

        // -- Arrow track (tracked_part == Some): MouseMove must fall through.
        let mut sb = ScrollBar::new(Rect::new(0, 0, 1, 10));
        let _id = stamp_id(&mut sb);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.set_params(50, 0, 100, 5, 1, &mut ctx);
        }
        let mut ev = mouse_down_at(0, 0); // up-arrow → auto track
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        assert!(sb.tracking && sb.tracked_part.is_some());
        let value_before = sb.value;

        let mut ev = mouse_move_at(0, 7);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            sb.handle_event(&mut ev, &mut ctx);
        }
        assert!(
            !ev.is_nothing(),
            "MouseMove during an arrow track falls through unconsumed"
        );
        assert_eq!(sb.value, value_before, "…and changes nothing");
    }
}
