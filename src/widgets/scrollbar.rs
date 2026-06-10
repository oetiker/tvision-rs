//! `TScrollBar` — faithful Rust port of `tscrlbar.cpp` (row 25, MECHANICAL).
//!
//! The scrollbar draws into its own 1×N (vertical) or N×1 (horizontal) bounds.
//! Orientation is inferred from the bounds at construction time: `width == 1`
//! means vertical, `height == 1` means horizontal (faithful to the C++).
//!
//! ## Glyph convention (D7, row 9)
//!
//! The character tables that magiblot hardcodes as `static TScrollChars vChars`
//! / `hChars` in `tvtext1.cpp` live in [`crate::theme::Glyphs`] instead:
//! `sb_v_arrow_back`, `sb_v_arrow_fwd`, `sb_h_arrow_back`, `sb_h_arrow_fwd`,
//! `sb_page`, `sb_thumb`, `sb_page_no_range`. Widgets read them via
//! `ctx.glyphs()`.
//!
//! ## D4 broadcast
//!
//! When the value changes, `scroll_draw` broadcasts
//! [`Command::SCROLL_BAR_CHANGED`] via `ctx.broadcast(…)`. The C++ equivalent
//! is `message(owner, evBroadcast, cmScrollBarChanged, this)`; the `this`
//! `infoPtr` payload is **no longer dropped** — it is carried as the broadcast's
//! `source` (D4 amendment), so a scroller/editor with two bars can tell which bar
//! fired (C++ `infoPtr == hScrollBar` becomes `source == self.h_scroll_bar`).
//!
//! The C++ also sends `cmScrollBarClicked` on mouse-down / keyboard action via
//! `message(owner, evBroadcast, cmScrollBarClicked, this)`. We broadcast
//! [`Command::SCROLL_BAR_CLICKED`] the same way, also carrying `source`.
//!
//! **This widget adds no receiver logic** — `source` is purely emitted here; it
//! is consumed by a future two-bar owner (Batch B), not by the scrollbar itself.
//!
//! ## Press-and-hold auto-repeat (D9 — **deferred to row 31**)
//!
//! The C++ `handleEvent` contains two nested `do { … } while(mouseEvent(…))`
//! loops (one for arrows, one for thumb-drag) that keep reading `evMouseAuto`
//! / `evMouseMove` events while the button is held, continuously scrolling or
//! dragging the thumb. This pattern requires the live event loop (row 31, D9)
//! to drive a capture handler.
//!
//! **This row implements only the single-step-per-click behavior:** the first
//! `evMouseDown` is classified, the value is adjusted once, and the event is
//! consumed. See the `// TODO(row 31, D9)` comment in [`ScrollBar::handle_event`]
//! for the exact deferred code paths.
//!
//! (`ctrlToArrow` / WordStar Ctrl-letter navigation — formerly deferred here —
//! landed with the A5 phase-signal row: [`ScrollBar::handle_event`] passes the
//! key through `ctrl_to_arrow` before the nav switch, faithful to the C++.)

use crate::command::Command;
use crate::data::FieldValue;
use crate::event::{Event, Key, MouseWheel, ctrl_to_arrow};
use crate::theme::Role;
use crate::view::{Context, DrawCtx, GrowMode, Options, Rect, View, ViewState};

// ---------------------------------------------------------------------------
// Scrollbar part codes — ports the `sb*` enum in `views.h`
// ---------------------------------------------------------------------------

/// Which part of the scrollbar was hit. Faithful to the C++ `sb*` constants
/// in `views.h`.
///
/// The vertical variants (`sbUpArrow`/`sbDownArrow`/`sbPageUp`/`sbPageDown`)
/// are exactly `+4` from the horizontal ones (the C++ `if(size.x == 1) part += 4`
/// pattern is therefore just a variant selection, not arithmetic, here).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    /// `sbLeftArrow` (0) — horizontal: left arrow / vertical unused.
    LeftArrow,
    /// `sbRightArrow` (1) — horizontal: right arrow.
    RightArrow,
    /// `sbPageLeft` (2) — horizontal: page left (trough left of thumb).
    PageLeft,
    /// `sbPageRight` (3) — horizontal: page right (trough right of thumb).
    PageRight,
    /// `sbUpArrow` (4) — vertical: up arrow.
    UpArrow,
    /// `sbDownArrow` (5) — vertical: down arrow.
    DownArrow,
    /// `sbPageUp` (6) — vertical: page up (trough above thumb).
    PageUp,
    /// `sbPageDown` (7) — vertical: page down (trough below thumb).
    PageDown,
    /// `sbIndicator` (8) — the thumb itself.
    Indicator,
}

impl Part {
    /// The scroll step for this part, faithful to `TScrollBar::scrollStep`.
    ///
    /// `step = arStep` if `!(part & 2)` (arrows), else `step = pgStep`.
    /// Negative if `!(part & 1)` (back direction).
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

/// `TScrollBar` — a single-axis scroll-bar widget (D2 View trait + ViewState).
///
/// Embed the pattern: `state: ViewState`, impl `View`, draw through `DrawCtx`,
/// handle events through `Context`.
///
/// Orientation is derived from bounds at construction (`size.x == 1` →
/// vertical). Characters come from `ctx.glyphs()` (D7).
pub struct ScrollBar {
    /// View state (geometry, flags, etc.) — the D2 composition target.
    pub state: ViewState,
    /// Current scroll position. `0` when constructed.
    pub value: i32,
    /// Minimum value of the range (inclusive). Faithful to C++ `minVal`.
    pub min_value: i32,
    /// Maximum value of the range (inclusive). Faithful to C++ `maxVal`.
    pub max_value: i32,
    /// Page step size (how much `PageUp`/`PageDown` moves). `pgStep`.
    pub page_step: i32,
    /// Arrow step size (how much an arrow key / click moves). `arStep`.
    pub arrow_step: i32,
    /// Whether this is a vertical bar (`size.x == 1`). Derived at construction.
    vertical: bool,
}

impl ScrollBar {
    /// Construct a scrollbar from `bounds`.
    ///
    /// Faithful to the C++ constructor: `value`/`minVal`/`maxVal` all zero,
    /// `pgStep = 1`, `arStep = 1`, `growMode` set per orientation,
    /// `eventMask |= evMouseWheel` (both orientations opt-in to wheel events
    /// — the existing `EventMask` only carries `mouse_move`/`mouse_auto`, and
    /// wheel is now an unconditional `MouseWheel` field on `MouseEvent`, so no
    /// explicit mask bit is needed).
    pub fn new(bounds: Rect) -> Self {
        let mut state = ViewState::new(bounds);
        let vertical = state.size.x == 1;

        // growMode faithful to tscrlbar.cpp:
        //   vertical:   gfGrowLoX | gfGrowHiX | gfGrowHiY
        //   horizontal: gfGrowLoY | gfGrowHiX | gfGrowHiY
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

        // The scrollbar is selectable (it handles arrow keys when focused).
        state.options = Options {
            selectable: true,
            ..Default::default()
        };

        ScrollBar {
            state,
            value: 0,
            min_value: 0,
            max_value: 0,
            page_step: 1,
            arrow_step: 1,
            vertical,
        }
    }

    /// Whether this bar is oriented vertically (width == 1).
    pub fn is_vertical(&self) -> bool {
        self.vertical
    }

    // -----------------------------------------------------------------------
    // Value / range logic (ports setParams / setValue / setRange / setStep)
    // -----------------------------------------------------------------------

    /// `TScrollBar::setParams` — update all parameters atomically.
    ///
    /// Faithful port: `aMax` is floored to `aMin`; `aValue` is clamped to
    /// `[aMin, aMax]`. Triggers a draw and, if `value` changed, broadcasts
    /// `cmScrollBarChanged`. Steps are set regardless of whether the range
    /// changed (faithful to the C++ where `pgStep`/`arStep` are updated
    /// unconditionally at the end).
    ///
    /// **D8 note:** `drawView()` in C++ triggers a damage-tracked repaint of
    /// just this view; under D8 the whole tree is redrawn, so we skip the
    /// `drawView` call and rely on the loop's render pass.
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
            // drawView() would go here under C++ (D8: skip).
            if old_value != a_value {
                self.scroll_draw(ctx);
            }
        }
        self.page_step = a_pg_step;
        self.arrow_step = a_ar_step;
    }

    /// `TScrollBar::setValue` — set the value, clamping to `[min, max]`.
    ///
    /// Forwards to [`set_params`](Self::set_params).
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

    /// `TScrollBar::setRange` — update `[min, max]`, keeping other params.
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

    /// `TScrollBar::setStep` — update the step sizes, keeping other params.
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

    /// `TScrollBar::scrollDraw` — broadcast `cmScrollBarChanged` (D4).
    ///
    /// C++ equivalent: `message(owner, evBroadcast, cmScrollBarChanged, this)`.
    /// The `this` payload is carried as the broadcast `source` (D4 amendment) so a
    /// two-bar owner can tell which bar fired.
    fn scroll_draw(&self, ctx: &mut Context) {
        ctx.broadcast(Command::SCROLL_BAR_CHANGED, self.state().id());
    }

    // -----------------------------------------------------------------------
    // Position / size math (ports getPos / getSize)
    // -----------------------------------------------------------------------

    /// `TScrollBar::getSize` — the scrollbar's active length in cells.
    ///
    /// Faithful port: `max(3, size.y or size.x)`.
    pub fn get_size(&self) -> i32 {
        let s = if self.vertical {
            self.state.size.y
        } else {
            self.state.size.x
        };
        s.max(3)
    }

    /// `TScrollBar::getPos` — the thumb position (1-based cell index in the
    /// scrollbar row/column, i.e. 1..=getSize()-2 inclusive).
    ///
    /// Faithful port:
    /// ```text
    /// if r == 0: return 1
    /// else: ((value - minVal) * (getSize() - 3) + r/2) / r + 1
    /// ```
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
    // Part classification (ports getPartCode)
    // -----------------------------------------------------------------------

    /// `TScrollBar::getPartCode` — classify which part a point hits.
    ///
    /// `mark` is the position along the scrollbar axis (x for horizontal,
    /// y for vertical), in view-local coords.  `pos` is the current thumb
    /// cell index. `s` is `getSize() - 1`.
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

    /// `TScrollBar::value` exposed as the D10 transfer currency — the row-27
    /// `TScroller` read-broker reads this through the trait (the pump resolves the
    /// bar by id and reads `value`, the successor to C++ `hScrollBar->value`).
    fn value(&self) -> Option<FieldValue> {
        Some(FieldValue::Int(self.value))
    }

    /// Concrete-reach hatch (the sanctioned downcast, same as `TWindow::zoom`'s
    /// frame push): the pump downcasts to `&mut ScrollBar` to call `set_params`
    /// when applying a `Deferred::ScrollBarSetParams` from a scroller.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    /// `TScrollBar::draw` + `drawPos` — paint the scrollbar.
    ///
    /// Layout (using vertical as example; horizontal mirrors on x-axis):
    /// - Cell 0:         back-arrow (`sb_v_arrow_back` / `sb_h_arrow_back`), controls role.
    /// - Cells 1..pos-1: trough/page (`sb_page` or `sb_page_no_range`), page role.
    /// - Cell pos:       thumb (`sb_thumb`), controls role. (omitted if range==0)
    /// - Cells pos+1..s-1: trough/page, page role.
    /// - Cell s:         fwd-arrow (`sb_v_arrow_fwd` / `sb_h_arrow_fwd`), controls role.
    ///
    /// Palette mapping (cpScrollBar `"\x04\x05\x05"`, indices 1-based):
    /// - Index 1 (trough/page):    `Role::ScrollBarPage`
    /// - Index 2 (arrows):         `Role::ScrollBarControls`
    /// - Index 3 (thumb):          `Role::ScrollBarControls`
    ///
    /// C++ `drawPos` writes into a `TDrawBuffer` then calls `writeBuf`; under
    /// D8 we write directly through `DrawCtx`.
    fn draw(&mut self, ctx: &mut DrawCtx) {
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

    /// `TScrollBar::handleEvent` — keyboard and mouse input.
    ///
    /// Handles:
    /// - `evMouseWheel`: adjust by `3 * arStep`, broadcast `SCROLL_BAR_CLICKED`
    ///   then `SCROLL_BAR_CHANGED` (via `set_value`).
    /// - `evMouseDown`: classify the part hit. **Only the four arrow parts**
    ///   perform a `value + scroll_step(part)` step. Page parts, the indicator,
    ///   and out-of-extent clicks all perform a **thumb-jump** to the mouse
    ///   position using the C++ formula
    ///   `((p-1)*(maxVal-minVal) + ((s-2)>>1)) / (s-2) + minVal`.
    ///   Broadcasts `SCROLL_BAR_CLICKED` first.
    /// - `evKeyDown` (when visible + focused): arrow/page/home/end keys.
    ///   Broadcasts `SCROLL_BAR_CLICKED` then adjusts value.
    ///
    /// **TODO(row 31, D9):** The C++ `handleEvent` contains two nested
    /// `do { … } while (mouseEvent(event, evMouseAuto/evMouseMove))` loops
    /// that continuously scroll / drag the thumb while the mouse button is
    /// held. This is a classic "synchronous inner event pump" pattern that
    /// requires the live event loop (row 31) to be driven as a capture handler
    /// (D9). The two loops are:
    ///
    /// 1. **Arrow press-and-hold** (`evMouseAuto`): while the button is held,
    ///    re-classify the part under the cursor each tick and call
    ///    `set_value(value + scroll_step(part))` if still over the original
    ///    arrow part. Should become a capture handler that fires on
    ///    `Event::MouseAuto`.
    ///
    /// 2. **Thumb drag** (`evMouseMove`): while the button is held, read the
    ///    cursor position, clamp to `[1, s-1]`, and recompute
    ///    `value = (pos-1)*(max-min) / (s-2) + min`. Should become a capture
    ///    handler that fires on `Event::MouseMove`.
    ///
    /// Until row 31 is done, each mouse-down produces exactly one step/jump.
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        let visible = self.state.state.visible;

        match *ev {
            // ------------------------------------------------------------------
            // Mouse wheel (evMouseWheel) — visible check matches C++ `sfVisible`
            // ------------------------------------------------------------------
            Event::MouseDown(me) if me.wheel != MouseWheel::None && visible => {
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
            // Mouse down (evMouseDown)
            // ------------------------------------------------------------------
            Event::MouseDown(me) => {
                ctx.broadcast(Command::SCROLL_BAR_CLICKED, self.state().id());

                // Compute the local mark (axis position) and thumb position.
                let local = me.position; // already in view-local coords per D3
                let mark = if self.vertical { local.y } else { local.x };
                let pos = self.get_pos();
                let s = self.get_size() - 1;

                // C++: `extent = getExtent(); extent.grow(1, 1);`
                // getPartCode returns -1 (None here) when outside this expanded extent.
                let extent = self.state.get_extent();
                let expanded = Rect::new(
                    extent.a.x - 1,
                    extent.a.y - 1,
                    extent.b.x + 1,
                    extent.b.y + 1,
                );
                // When outside the expanded extent, getPartCode() returns -1 which
                // falls into the C++ `default:` branch (thumb-jump). We encode that
                // by setting click_part = None here and routing None → jump below.
                let click_part = if expanded.contains(local) {
                    self.get_part_code(mark, pos, s)
                } else {
                    None // C++ getPartCode() == -1 → falls into default: (thumb-jump)
                };

                match click_part {
                    // The four arrow parts → single scroll-step (first click).
                    // TODO(row 31, D9): Replace with a capture handler that fires on
                    // Event::MouseAuto, re-classifying the part under the cursor each
                    // tick and calling set_value(value + scroll_step(part)) if the
                    // part still matches the original click_part. Loop runs until
                    // Event::MouseUp.
                    Some(
                        p @ (Part::LeftArrow | Part::RightArrow | Part::UpArrow | Part::DownArrow),
                    ) => {
                        self.set_value(
                            self.value + p.scroll_step(self.arrow_step, self.page_step),
                            ctx,
                        );
                    }
                    // Page parts, Indicator, and out-of-extent → thumb-jump to the
                    // mouse cursor position (C++ `default:` branch).
                    // TODO(row 31, D9): Replace with a capture handler that fires on
                    // Event::MouseMove, continuously recomputing:
                    //   i = clamp(mouse.y or .x, 1, s-1);
                    //   value = ((i-1)*(max-min) + ((s-2)>>1)) / (s-2) + min;
                    // until the button is released (Event::MouseUp).
                    _ => {
                        let i = mark.max(1).min(s - 1);
                        if s > 2 {
                            let new_val = (i64::from(i - 1)
                                * i64::from(self.max_value - self.min_value)
                                + i64::from((s - 2) >> 1))
                                / i64::from(s - 2)
                                + i64::from(self.min_value);
                            self.set_value(new_val as i32, ctx);
                        }
                    }
                }

                // clearEvent(event) — always runs after mouse-down (no early return).
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
        // D4 amendment: the `cmScrollBarChanged` broadcast must carry `source ==
        // the emitting scrollbar's id` (the C++ `this`), not `None`. The id is
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
}
