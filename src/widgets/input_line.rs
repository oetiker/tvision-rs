//! `TInputLine` — faithful Rust port of `tinputli.cpp` (row 39).
//!
//! A single-line text-entry field with selection, horizontal scrolling, an
//! optional [`Validator`], and the D10 typed `value`/`set_value` data protocol.
//!
//! # Coordinate model (D13 — byte offsets vs. display columns)
//!
//! `data` is a Rust [`String`]. The cursor/selection offsets
//! [`cur_pos`](InputLine::cur_pos) / [`sel_start`](InputLine::sel_start) /
//! [`sel_end`](InputLine::sel_end) / [`anchor`](InputLine::anchor) are **byte
//! offsets** into it (the C++ `int` indices into the `char*` buffer). Slicing a
//! `String` at a non-`char` boundary panics, so every step over `data` goes
//! through [`text::next`] / [`text::prev`] (whole grapheme clusters) — never
//! `+1`/`-1`. `prev_word`/`next_word` scan ASCII spaces, whose byte offsets are
//! always `char` boundaries.
//!
//! [`first_pos`](InputLine::first_pos), by contrast, is a **display column**
//! (the horizontal scroll offset): the C++ `setCursor(displayedPos(curPos) -
//! firstPos + 1, …)` and `selectAll`'s `firstPos = displayedPos(curPos) -
//! size.x + 2` are column arithmetic. `displayedPos(pos)` = the display width of
//! `data[..pos]`. For ASCII byte == column, but for multi-byte / wide content
//! they diverge, so the two units are kept strictly distinct here.
//!
//! # D-rules applied
//!
//! * **D1** the `T` prefix is dropped; `ilMaxBytes/ilMaxWidth/ilMaxChars` become
//!   the [`LimitMode`] enum.
//! * **D2** inheritance → trait + composition (`InputLine` embeds [`ViewState`];
//!   the validator is a [`Validator`] trait object).
//! * **D7** colours via `ctx.style(Role::Input*)`; the `cpInputLine` palette
//!   indices map to [`Role::InputNormal`]/[`InputSelected`]/[`InputArrow`]; the
//!   scroll arrows are [`Glyphs`] fields.
//! * **D8** draw into the back buffer through [`DrawCtx`]; `drawView`/`writeLine`
//!   dropped. The selection highlight is rendered by **redrawing the selected
//!   substring** in the selected style (the C++ `moveChar(.., 0, .., ..)`
//!   attr-only paint has no rstv primitive — the `0 = retain glyph` sentinel was
//!   dropped — so a segmented redraw produces identical output).
//! * **D9** the cursor screen position is computed in `handle_event` and stored
//!   on [`ViewState::cursor`]; the live loop's `resetCursor` (row 31) reads it
//!   before redraw, so it is NOT set inside `draw` (which runs after).
//! * **D10** [`value`](View::value)/[`set_value`](View::set_value) over
//!   [`FieldValue`] replace `getData`/`setData`/`dataSize`.
//! * **D12** `TStreamable` (`read`/`write`/`build`) dropped.
//!
//! # Deferrals (documented TODOs, not built)
//!
//! 1. **The `evCommand` clipboard block** — **landed (B3)**. `cmCut`/`cmCopy`
//!    use `ctx.set_clipboard`; `cmPaste` uses `ctx.request_input_line_paste`
//!    → `Deferred::InputLinePaste` (the pump reads the backend clipboard and
//!    inserts into the field, clamped to `max_len` and replacing any selection).
//! 2. **`updateCommands`/`canUpdateCommands`** — **landed (B1)**. The
//!    `can_update_commands` / `update_commands` helpers push deferred
//!    enable/disable ops for `cmCut`/`cmCopy` (on selection change) and
//!    `cmPaste` (always enabled when active+selected). Called from `set_state`,
//!    `handle_event` (mouse/key tail + command arm).
//! 3. **`valid()`'s `select()` side-effect** — focusing the bad field needs
//!    `&mut Context`; `valid(&self)` returns the faithful boolean only.
//!    `TODO(valid-select)`.
//! 4. **Validator `transfer`** (the D10 typed-non-text hook) — live via
//!    [`Validator::transfer_get`]/[`Validator::transfer_set`]; `RangeValidator`
//!    (row 59) overrides them. `InputLine::value`/`set_value` consult the hook
//!    before the text fallback (C++ `getData`/`setData` `voTransfer` path).

use crate::capture::TrackMask;
use crate::command::Command;
use crate::data::FieldValue;
use crate::event::{Event, Key, MouseEvent, ctrl_to_arrow};
use crate::text;
use crate::theme::Role;
use crate::validate::Validator;
use crate::view::{Context, DrawCtx, Options, Point, Rect, StateFlag, View, ViewState};

/// `Ctrl-Y` (`CONTROL_Y = 25`) — clears the whole field. In the decomposed key
/// model this is `Key::Char('y')` + `ctrl`.
const CONTROL_Y: char = 'y';

// ---------------------------------------------------------------------------
// LimitMode — D1 enum for ilMaxBytes / ilMaxWidth / ilMaxChars
// ---------------------------------------------------------------------------

/// How the `limit` ctor argument is interpreted — ports `ilMaxBytes`/
/// `ilMaxWidth`/`ilMaxChars` (`dialogs.h`), D1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LimitMode {
    /// `ilMaxBytes` (0, the C++ default) — `limit` caps the byte length
    /// (`maxLen = limit - 1`); width/char count are unbounded.
    #[default]
    MaxBytes,
    /// `ilMaxWidth` (1) — `limit` caps the display width (`maxWidth = limit`);
    /// `maxLen = 255`.
    MaxWidth,
    /// `ilMaxChars` (2) — `limit` caps the grapheme count (`maxChars = limit`);
    /// `maxLen = 255`.
    MaxChars,
}

// ---------------------------------------------------------------------------
// InputLine
// ---------------------------------------------------------------------------

/// `TInputLine` — a single-line text-entry field (D2 View trait + ViewState).
pub struct InputLine {
    /// View state (geometry, flags, cursor) — the D2 composition target.
    pub state: ViewState,
    /// `data` — the field contents.
    pub data: String,
    /// `maxLen` — the maximum byte length of `data`.
    pub max_len: i32,
    /// `maxWidth` — the maximum display width (`INT_MAX` ≈ unbounded otherwise).
    pub max_width: i32,
    /// `maxChars` — the maximum grapheme count (`INT_MAX` ≈ unbounded otherwise).
    pub max_chars: i32,
    /// `curPos` — cursor position, a **byte** offset into `data`.
    pub cur_pos: i32,
    /// `firstPos` — horizontal scroll offset, a **display column** (see module
    /// docs: NOT a byte offset).
    pub first_pos: i32,
    /// `selStart` — selection start, a **byte** offset into `data`.
    pub sel_start: i32,
    /// `selEnd` — selection end, a **byte** offset into `data`.
    pub sel_end: i32,
    /// `anchor` — the fixed end of a keyboard/mouse block extension, a **byte**
    /// offset.
    pub anchor: i32,
    /// `validator` — the optional input validator (D2 trait object).
    pub validator: Option<Box<dyn Validator>>,
    // -- validator save-state (oldData/oldCurPos/…) ------------------------
    old_data: String,
    old_cur_pos: i32,
    old_first_pos: i32,
    old_sel_start: i32,
    old_sel_end: i32,
    // -- mouse hold-tracking (A3 seam) ------------------------------------
    /// Absolute screen position of input-local `(0, 0)`, cached each `draw`
    /// so the mouse-tracking capture can convert absolute mouse coords to
    /// field-local (D3/D9 — the `Button::abs_origin` pattern).
    abs_origin: Point,
    /// Whether a mouse hold-track is in flight. Guards the `MouseAuto` /
    /// `MouseMove` / `MouseUp` tracking arms against stray events.
    tracking: bool,
    /// `true` for the drag-select branch (mask: move+auto); `false` for the
    /// edge-auto-scroll branch (mask: auto only). Distinguishes which loop
    /// body to execute in the `MouseAuto` arm.
    tracking_drag: bool,
}

impl InputLine {
    /// `TInputLine::TInputLine` — build a field from `bounds`, `limit`, an
    /// optional `validator`, and a [`LimitMode`].
    ///
    /// Faithful to the C++ ctor: `maxLen = (mode==MaxBytes) ?
    /// min(max(limit-1,0), INT_MAX-1) : 255`; `maxWidth = (mode==MaxWidth) ?
    /// limit : INT_MAX`; `maxChars = (mode==MaxChars) ? limit : INT_MAX`. Sets
    /// `sfCursorVis` and `ofSelectable | ofFirstClick`; `data` starts empty.
    pub fn new(
        bounds: Rect,
        limit: i32,
        validator: Option<Box<dyn Validator>>,
        limit_mode: LimitMode,
    ) -> Self {
        let max_len = if limit_mode == LimitMode::MaxBytes {
            // C++ min(max(limit-1, 0), INT_MAX-1); clamp is equivalent here
            // (0 < i32::MAX-1 always, so it never panics).
            (limit - 1).clamp(0, i32::MAX - 1)
        } else {
            255
        };
        let max_width = if limit_mode == LimitMode::MaxWidth {
            limit
        } else {
            i32::MAX
        };
        let max_chars = if limit_mode == LimitMode::MaxChars {
            limit
        } else {
            i32::MAX
        };

        let mut state = ViewState::new(bounds);
        state.state.cursor_vis = true; // sfCursorVis
        state.options = Options {
            selectable: true,
            first_click: true,
            ..Default::default()
        };

        let mut il = InputLine {
            state,
            data: String::new(),
            max_len,
            max_width,
            max_chars,
            cur_pos: 0,
            first_pos: 0,
            sel_start: 0,
            sel_end: 0,
            anchor: 0,
            validator,
            old_data: String::new(),
            old_cur_pos: 0,
            old_first_pos: 0,
            old_sel_start: 0,
            old_sel_end: 0,
            abs_origin: Point::new(0, 0),
            tracking: false,
            tracking_drag: false,
        };
        il.sync_cursor();
        il
    }

    /// Convenience ctor with no validator and the default `ilMaxBytes` mode.
    pub fn with_limit(bounds: Rect, limit: i32) -> Self {
        Self::new(bounds, limit, None, LimitMode::MaxBytes)
    }

    // -- geometry helpers (byte ↔ column) ----------------------------------

    /// `TInputLine::displayedPos` — the display column of the prefix
    /// `data[..pos]` (`pos` is a byte offset). The screen-column ↔ byte bridge.
    fn displayed_pos(&self, pos: i32) -> i32 {
        text::width(&self.data[..pos as usize]) as i32
    }

    /// `TInputLine::canScroll` — whether the field can scroll by `delta`
    /// (`delta < 0` left, `> 0` right). Right uses display-width arithmetic.
    fn can_scroll(&self, delta: i32) -> bool {
        if delta < 0 {
            self.first_pos > 0
        } else if delta > 0 {
            text::width(&self.data) as i32 - self.first_pos + 2 > self.state.size.x
        } else {
            false
        }
    }

    /// Store the screen-cursor position on [`ViewState::cursor`] so the loop's
    /// `resetCursor` (row 31) can read it before redraw — the D9 split of the C++
    /// `setCursor(displayedPos(curPos) - firstPos + 1, 0)` out of `draw`.
    fn sync_cursor(&mut self) {
        let x = self.displayed_pos(self.cur_pos) - self.first_pos + 1;
        self.state.set_cursor(x, 0);
    }

    // -- selection / deletion (byte offsets) -------------------------------

    /// `TInputLine::deleteSelect` — remove `data[selStart..selEnd]`, leaving the
    /// cursor at `selStart`.
    fn delete_select(&mut self) {
        if self.sel_start < self.sel_end {
            self.data
                .replace_range(self.sel_start as usize..self.sel_end as usize, "");
            self.cur_pos = self.sel_start;
        }
    }

    /// `TInputLine::deleteCurrent` — select the grapheme under the cursor (one
    /// `TText::next` step) and delete it.
    fn delete_current(&mut self) {
        let cp = self.cur_pos as usize;
        if cp < self.data.len() {
            let step = text::next(&self.data[cp..])
                .map(|(len, _)| len)
                .unwrap_or(0);
            self.sel_start = self.cur_pos;
            self.sel_end = self.cur_pos + step as i32;
            self.delete_select();
        }
    }

    /// `TInputLine::adjustSelectBlock` — order `selStart`/`selEnd` around the
    /// `anchor` after a block extension.
    fn adjust_select_block(&mut self) {
        if self.cur_pos < self.anchor {
            self.sel_start = self.cur_pos;
            self.sel_end = self.anchor;
        } else {
            self.sel_start = self.anchor;
            self.sel_end = self.cur_pos;
        }
    }

    /// `TInputLine::selectAll` — select all (or none) and optionally scroll the
    /// end into view. **Does not** draw (D8 whole-tree redraw) but does sync the
    /// cursor (the C++ `drawView()` set it). Callers that have `ctx` and where
    /// `canUpdateCommands()` may be true (set_state, handle_event) call
    /// `update_commands` themselves after `select_all`.
    pub fn select_all(&mut self, enable: bool, scroll: bool) {
        self.sel_start = 0;
        if enable {
            self.cur_pos = self.data.len() as i32;
            self.sel_end = self.cur_pos;
        } else {
            self.cur_pos = 0;
            self.sel_end = 0;
        }
        if scroll {
            self.first_pos = (self.displayed_pos(self.cur_pos) - self.state.size.x + 2).max(0);
        }
        self.sync_cursor();
    }

    /// `TInputLine::canUpdateCommands` — true when both `sfActive` and `sfSelected`
    /// are set (C++: `(~state & (sfActive | sfSelected)) == 0`). Only when both
    /// flags are set should `update_commands` push the enable/disable deferred ops.
    fn can_update_commands(&self) -> bool {
        self.state.state.active && self.state.state.selected
    }

    /// `TInputLine::updateCommands` — push enable/disable deferred ops for
    /// cmCut/cmCopy (enabled when a selection exists) and cmPaste (always enabled
    /// while this field is active+selected, faithful to `setCmdState(cmPaste, True)`).
    /// Only called when `can_update_commands()`. Faithful to tinputli.cpp.
    fn update_commands(&self, ctx: &mut Context) {
        let has_selection = self.sel_start < self.sel_end;
        if has_selection {
            ctx.enable_command(Command::CUT);
            ctx.enable_command(Command::COPY);
        } else {
            ctx.disable_command(Command::CUT);
            ctx.disable_command(Command::COPY);
        }
        // cmPaste is always enabled when this field is active+selected.
        ctx.enable_command(Command::PASTE);
    }

    // -- validator save/restore/check --------------------------------------

    /// `TInputLine::saveState` — snapshot for the validator's restore-on-reject.
    fn save_state(&mut self) {
        if self.validator.is_some() {
            self.old_data.clear();
            self.old_data.push_str(&self.data);
            self.old_cur_pos = self.cur_pos;
            self.old_first_pos = self.first_pos;
            self.old_sel_start = self.sel_start;
            self.old_sel_end = self.sel_end;
        }
    }

    /// `TInputLine::restoreState` — undo to the last [`save_state`](Self::save_state).
    fn restore_state(&mut self) {
        if self.validator.is_some() {
            self.data.clear();
            self.data.push_str(&self.old_data);
            self.cur_pos = self.old_cur_pos;
            self.first_pos = self.old_first_pos;
            self.sel_start = self.old_sel_start;
            self.sel_end = self.old_sel_end;
        }
    }

    /// `TInputLine::checkValid` — run the validator's `isValidInput` over the
    /// current `data`; on reject, restore and report `false`; on accept, clamp to
    /// `maxLen` and pull `curPos` to the new end if it sat past the old one.
    /// Returns whether the input is (still) valid.
    fn check_valid(&mut self, no_auto_fill: bool) -> bool {
        if self.validator.is_none() {
            return true;
        }
        let old_len = self.data.len() as i32;
        // isValidInput may modify the candidate in place.
        let mut candidate = self.data.clone();
        let validator = self.validator.as_ref().unwrap();
        if !validator.is_valid_input(&mut candidate, no_auto_fill) {
            self.restore_state();
            false
        } else {
            // Clamp to maxLen on a char boundary (truncate never splits a char).
            if candidate.len() as i32 > self.max_len {
                let mut cut = self.max_len as usize;
                while cut > 0 && !candidate.is_char_boundary(cut) {
                    cut -= 1;
                }
                candidate.truncate(cut);
            }
            let new_len = candidate.len() as i32;
            self.data = candidate;
            // TODO(auto-fill clamp): a mutating validator that SHRINKS data can
            // leave cur_pos past EOS / mid-grapheme; re-clamp cur_pos to a char
            // boundary <= data.len() when the first auto-fill validator lands
            // (D13 panic hazard).
            if self.cur_pos >= old_len && new_len > old_len {
                self.cur_pos = new_len;
            }
            true
        }
    }

    // -- clipboard paste (B3) ------------------------------------------------

    /// Insert `text` from the clipboard at the current cursor position,
    /// replacing any active selection and clamping the result to `max_len`.
    /// Called by the pump's `Deferred::InputLinePaste` apply arm. Mirrors the
    /// C++ `TClipboard::requestText()` completion path: insert the pasted bytes
    /// at `curPos`, replacing the selection, clamped so the total byte length
    /// does not exceed `maxLen`. Tabs/newlines are replaced with spaces
    /// (faithful to the per-character insertion in the C++ keyboard path).
    /// After insertion the cursor sits at the end of the pasted text and the
    /// selection is cleared.
    pub fn paste_text(&mut self, text: &str) {
        self.save_state();
        // Replace the selection (C++ tinputli.cpp paste lands after the cut/copy
        // block; in the original C++ cmPaste → requestText → callback inserts).
        self.delete_select();
        // Replace tabs/newlines with spaces, insert character by character,
        // stopping when max_len would be exceeded. For simplicity we insert the
        // whole normalised string in one go after clamping.
        let normalized: String = text
            .chars()
            .map(|c| {
                if c == '\t' || c == '\r' || c == '\n' {
                    ' '
                } else {
                    c
                }
            })
            .collect();
        // How many bytes we can still accept.
        let room = (self.max_len - self.data.len() as i32).max(0) as usize;
        // Clamp `normalized` to `room` bytes at a char boundary.
        let clamped = if normalized.len() <= room {
            &normalized[..]
        } else {
            let mut cut = room;
            while cut > 0 && !normalized.is_char_boundary(cut) {
                cut -= 1;
            }
            &normalized[..cut]
        };
        if !clamped.is_empty() {
            self.data.insert_str(self.cur_pos as usize, clamped);
            self.cur_pos += clamped.len() as i32;
        }
        self.sel_start = 0;
        self.sel_end = 0;
        // firstPos scroll-follow (column arithmetic).
        let cur_width = self.displayed_pos(self.cur_pos);
        if self.first_pos > cur_width {
            self.first_pos = cur_width;
        }
        let i = cur_width - self.state.size.x + 2;
        if self.first_pos < i {
            self.first_pos = i;
        }
        self.sync_cursor();
        self.check_valid(true);
    }

    // -- mouse helpers (used by the press-and-hold track arms) -------------

    /// `TInputLine::mousePos` — the byte offset under the mouse (view-local
    /// position already applied by the group).
    fn mouse_pos(&self, m: &MouseEvent) -> i32 {
        let mx = m.position.x.max(1);
        let pos = (mx + self.first_pos - 1).max(0);
        // scroll(columns) → byte length of that many columns into data.
        text::scroll(&self.data, pos, false).0 as i32
    }

    /// `TInputLine::mouseDelta` — the auto-scroll direction for a mouse at the
    /// edge. Used by `MouseDown` (the first loop iteration) and the `MouseAuto`
    /// arm (hold-repeat ticks) in both the edge-scroll and drag-select branches.
    fn mouse_delta(&self, m: &MouseEvent) -> i32 {
        if m.position.x <= 0 {
            -1
        } else if m.position.x >= self.state.size.x - 1 {
            1
        } else {
            0
        }
    }
}

/// `prevWord` (`tinputli.cpp`) — the byte offset of the start of the word before
/// `pos`. Scans ASCII spaces backwards; space bytes are never inside a multibyte
/// sequence, so every returned offset is a `char` boundary.
fn prev_word(s: &str, pos: i32) -> i32 {
    let b = s.as_bytes();
    let mut i = pos - 1;
    while i >= 1 {
        if b[i as usize] != b' ' && b[(i - 1) as usize] == b' ' {
            return i;
        }
        i -= 1;
    }
    0
}

/// `nextWord` (`tinputli.cpp`) — the byte offset of the start of the word after
/// `pos`. Scans ASCII spaces forward (see [`prev_word`] re boundaries).
fn next_word(s: &str, pos: i32) -> i32 {
    let b = s.as_bytes();
    let len = b.len() as i32;
    let mut i = pos;
    while i < len - 1 {
        if b[i as usize] == b' ' && b[(i + 1) as usize] != b' ' {
            return i + 1;
        }
        i += 1;
    }
    len
}

impl View for InputLine {
    fn state(&self) -> &ViewState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }

    /// Exposes the concrete `InputLine` so the pump's
    /// [`InputLinePaste`](crate::view::Deferred::InputLinePaste) broker can
    /// downcast and call [`paste_text`](InputLine::paste_text) (B3).
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    /// `TInputLine::draw` — fill with the normal colour, draw the scrolled text,
    /// the scroll arrows, and the selection highlight. The cursor is **not** set
    /// here (D9 — see [`sync_cursor`](InputLine::sync_cursor)).
    fn draw(&mut self, ctx: &mut DrawCtx) {
        // Cache absolute origin for the mouse-tracking capture (D3/D9 — the
        // MouseTrackCapture converts abs mouse coords to field-local via this
        // value, mirroring the Button `abs_origin` pattern).
        self.abs_origin = ctx.origin();
        let size = self.state.size;
        // getColor((sfFocused)?2:1) — both palette indices map to InputNormal.
        let color = ctx.style(Role::InputNormal);
        let arrow = ctx.style(Role::InputArrow);
        let selected = ctx.style(Role::InputSelected);
        let left_arrow = ctx.glyphs().input_left_arrow;
        let right_arrow = ctx.glyphs().input_right_arrow;

        // moveChar(0, ' ', color, size.x) — fill the whole row.
        ctx.fill(Rect::new(0, 0, size.x, 1), ' ', color);
        // moveStr(1, data, color, size.x-1, firstPos) — scrolled text from col 1.
        if size.x > 1 {
            // The text window is columns 1..size.x; clip there via a sub-ctx so a
            // glyph cannot spill into col 0 or past the right edge.
            let mut sub = ctx.sub(Rect::new(1, 0, size.x, 1));
            sub.put_str_part(0, 0, &self.data, self.first_pos, color);
        }

        // Scroll arrows.
        if self.can_scroll(1) {
            ctx.put_char(size.x - 1, 0, right_arrow, arrow);
        }
        if self.can_scroll(-1) {
            ctx.put_char(0, 0, left_arrow, arrow);
        }

        // Selection highlight. C++ recolors columns [l+1 .. l+1+(r-l)) with
        // getColor(3); rstv has no attr-only paint (the `0 = retain` sentinel was
        // dropped), so we REDRAW the selected substring in the selected style at
        // its screen column — byte-identical output.
        if self.state.state.selected && self.sel_start < self.sel_end {
            // C++ l/r are display columns of the selection ends, relative to the
            // scroll window; the recolor covers view columns [l+1 .. r+1) (width
            // r-l) at `moveChar(l+1, 0, getColor(3), r-l)`.
            let l = (self.displayed_pos(self.sel_start) - self.first_pos).max(0);
            let r = (self.displayed_pos(self.sel_end) - self.first_pos).min(size.x - 2);
            if l < r {
                // We REDRAW (no attr-only paint), so the glyphs must be the ones
                // the scrolled main pass drew, NOT the head of sel_text. When the
                // selection starts left of the window (`first_pos >
                // displayed_pos(sel_start)`), `l` clamped to 0 but the substring
                // must skip those off-left columns — feed the skip as text_indent,
                // exactly as the main pass skips `first_pos` columns. Clip to
                // [l+1 .. r+1) so the right scroll-arrow cell (>= size.x-1) is never
                // overwritten.
                let skip = (self.first_pos - self.displayed_pos(self.sel_start)).max(0);
                let mut sub = ctx.sub(Rect::new(l + 1, 0, r + 1, 1));
                let sel_text = &self.data[self.sel_start as usize..self.sel_end as usize];
                sub.put_str_part(0, 0, sel_text, skip, selected);
            }
        }
    }

    /// `TInputLine::handleEvent` — the `sfSelected` keyboard/mouse block. See the
    /// module deferrals for the clipboard / command-graying parts that are
    /// intentionally not ported.
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        // Base TView::handleEvent (mouse-down auto-select is the group's job now;
        // base is a no-op).
        if !self.state.state.selected {
            return;
        }

        match ev {
            // -- Mouse positioning + hold-tracking (A3 seam) ---------------
            //
            // The C++ `handleEvent` (tinputli.cpp:312-339) has two `do{}while`
            // loops starting from the same `evMouseDown` — we port them as the
            // first iteration in `MouseDown` then arm the capture for subsequent
            // ticks.
            Event::MouseDown(m) => {
                let m = *m;
                let delta = self.mouse_delta(&m);
                if self.can_scroll(delta) {
                    // C++ tinputli.cpp:313-320 — edge auto-scroll loop.
                    // First iteration: `if canScroll(delta) firstPos += delta`.
                    self.first_pos += delta;
                    // Arm auto-only repeat via the A3 seam.
                    if let Some(id) = self.state.id() {
                        self.tracking = true;
                        self.tracking_drag = false;
                        ctx.start_mouse_track(
                            id,
                            self.abs_origin,
                            TrackMask {
                                mouse_auto: true,
                                ..Default::default()
                            },
                        );
                    }
                } else if m.flags.double_click {
                    // C++ tinputli.cpp:322 selectAll(True) — scroll arg defaults
                    // to True (dialogs.h:177), so double-click selects-all AND
                    // scrolls the end into view. No tracking loop for this branch.
                    self.select_all(true, true);
                } else {
                    // C++ tinputli.cpp:324-338 — drag-select loop.
                    // First iteration: `anchor = mousePos(event); curPos = mousePos(event);
                    // adjustSelectBlock()`.
                    let pos = self.mouse_pos(&m);
                    self.anchor = pos;
                    self.cur_pos = pos;
                    self.adjust_select_block();
                    // Arm move+auto tracking via the A3 seam.
                    if let Some(id) = self.state.id() {
                        self.tracking = true;
                        self.tracking_drag = true;
                        ctx.start_mouse_track(
                            id,
                            self.abs_origin,
                            TrackMask {
                                mouse_move: true,
                                mouse_auto: true,
                                ..Default::default()
                            },
                        );
                    }
                }
                self.sync_cursor();
                ev.clear();
            }

            // -- Mouse auto (evMouseAuto) — loop body, guarded by tracking --
            //
            // For the edge-scroll branch (tinputli.cpp:315-318):
            //   `if canScroll(delta) firstPos += delta`
            //
            // For the drag-select branch (tinputli.cpp:327-335):
            //   `if event.what==evMouseAuto: delta=mouseDelta; if canScroll(delta) firstPos+=delta`
            //   then `curPos=mousePos; adjustSelectBlock`
            Event::MouseAuto(m) if self.tracking => {
                let m = *m;
                let delta = self.mouse_delta(&m);
                if self.can_scroll(delta) {
                    // C++ tinputli.cpp:328-330 (drag branch auto body) /
                    // tinputli.cpp:315-318 (edge branch): edge-scroll
                    self.first_pos += delta;
                }
                if self.tracking_drag {
                    // C++ tinputli.cpp:332-334 (drag branch, both auto and move):
                    // `curPos = mousePos(event); adjustSelectBlock()`
                    self.cur_pos = self.mouse_pos(&m);
                    self.adjust_select_block();
                }
                self.sync_cursor();
                ev.clear();
            }

            // -- Mouse move (evMouseMove) — drag-select loop body (move ticks)
            //
            // C++ tinputli.cpp:332-334 (inside the move|auto loop, for non-auto
            // events): `curPos=mousePos(event); adjustSelectBlock()`.
            // The edge-scroll branch's C++ loop is auto-only-masked, so a move
            // during an edge track must FALL THROUGH unconsumed (the two-
            // separate-masked-loops structure, same as the scrollbar arms).
            Event::MouseMove(m) if self.tracking && self.tracking_drag => {
                let m = *m;
                // C++ tinputli.cpp:333-334: `curPos = mousePos; adjustSelectBlock`
                self.cur_pos = self.mouse_pos(&m);
                self.adjust_select_block();
                self.sync_cursor();
                ev.clear();
            }

            // -- Mouse up — post-loop (tracking ends). Guarded by `tracking`
            // (MouseUp is not mask-gated in Group::wants).
            Event::MouseUp(_) if self.tracking => {
                self.tracking = false;
                self.tracking_drag = false;
                ev.clear();
            }

            // -- Keyboard --------------------------------------------------
            Event::KeyDown(ke) => {
                self.save_state();
                let ke = ctrl_to_arrow(*ke);

                // Shift-extend applies to the genuine pad keys (Home/End/Left/
                // Right) with Shift held. ctrl_to_arrow cleared modifiers on the
                // Ctrl-letter remaps, so a remapped key never carries Shift — only
                // a literal Shift+arrow/Home/End reaches here.
                let is_pad = matches!(ke.key, Key::Home | Key::End | Key::Left | Key::Right);
                let extend_block = is_pad && ke.modifiers.shift;
                if extend_block {
                    if self.cur_pos == self.sel_end {
                        self.anchor = self.sel_start;
                    } else if self.sel_start == self.sel_end {
                        self.anchor = self.cur_pos;
                    } else {
                        self.anchor = self.sel_end;
                    }
                }

                // Distinguish Ctrl-Left/Right (word nav) from plain Left/Right.
                let ctrl = ke.modifiers.ctrl;
                let mut handled = true;
                match ke.key {
                    Key::Left if ctrl => self.cur_pos = prev_word(&self.data, self.cur_pos),
                    Key::Right if ctrl => self.cur_pos = next_word(&self.data, self.cur_pos),
                    Key::Left => {
                        self.cur_pos -= text::prev(&self.data, self.cur_pos as usize) as i32
                    }
                    Key::Right => {
                        let cp = self.cur_pos as usize;
                        let step = text::next(&self.data[cp..]).map(|(l, _)| l).unwrap_or(0);
                        self.cur_pos += step as i32;
                    }
                    Key::Home => self.cur_pos = 0,
                    Key::End => self.cur_pos = self.data.len() as i32,
                    Key::Backspace if ctrl => {
                        // kbCtrlBack / kbAltBack — delete the previous word.
                        if self.sel_start == self.sel_end {
                            self.sel_start = prev_word(&self.data, self.cur_pos);
                            self.sel_end = self.cur_pos;
                        }
                        self.delete_select();
                        self.check_valid(true);
                    }
                    Key::Backspace => {
                        if self.sel_start == self.sel_end {
                            self.sel_start =
                                self.cur_pos - text::prev(&self.data, self.cur_pos as usize) as i32;
                            self.sel_end = self.cur_pos;
                        }
                        self.delete_select();
                        self.check_valid(true);
                    }
                    Key::Delete if ctrl => {
                        // kbCtrlDel — delete to the next word.
                        if self.sel_start == self.sel_end {
                            self.sel_start = self.cur_pos;
                            self.sel_end = next_word(&self.data, self.cur_pos);
                        }
                        self.delete_select();
                        self.check_valid(true);
                    }
                    Key::Delete => {
                        if self.sel_start == self.sel_end {
                            self.delete_current();
                        } else {
                            self.delete_select();
                        }
                        self.check_valid(true);
                    }
                    Key::Insert => {
                        // C++ setState(sfCursorIns, !(state & sfCursorIns)). sfCursorIns
                        // is NOT a propagating StateFlag (it has no broadcast/selectAll
                        // side effect — see view.rs StateFlag docs), so flip it directly
                        // on ViewState; this matches the C++ setState's only effect for
                        // sfCursorIns (toggling the cursor shape).
                        self.state.state.cursor_ins = !self.state.state.cursor_ins;
                    }
                    Key::Char(c) if ctrl && c.eq_ignore_ascii_case(&CONTROL_Y) => {
                        // Ctrl-Y clears the field (handled in the C++ default's
                        // else-if). ctrl_to_arrow leaves Ctrl-Y unchanged.
                        self.data.clear();
                        self.cur_pos = 0;
                    }
                    Key::Char(c) if !ctrl && !ke.modifiers.alt => {
                        // Printable insertion. Tabs/newlines → space (faithful).
                        let ch = if c == '\t' || c == '\r' || c == '\n' {
                            ' '
                        } else {
                            c
                        };
                        let mut key_text = [0u8; 4];
                        let key_text = ch.encode_utf8(&mut key_text);
                        let len = key_text.len() as i32;

                        self.delete_select();
                        if self.state.state.cursor_ins {
                            self.delete_current();
                        }
                        if self.check_valid(true) {
                            let data_m = text::measure(&self.data);
                            let key_m = text::measure(key_text);
                            if self.data.len() as i32 + len <= self.max_len
                                && (data_m.width + key_m.width) as i32 <= self.max_width
                                && (data_m.grapheme_count + key_m.grapheme_count) as i32
                                    <= self.max_chars
                            {
                                // firstPos is a column; pull it back to the cursor
                                // column if the cursor scrolled off the left.
                                let cur_col = self.displayed_pos(self.cur_pos);
                                if self.first_pos > cur_col {
                                    self.first_pos = cur_col;
                                }
                                self.data.insert_str(self.cur_pos as usize, key_text);
                                self.cur_pos += len;
                            }
                            self.check_valid(false);
                        }
                    }
                    _ => handled = false,
                }

                if !handled {
                    // Unhandled key (Tab, Enter, a modified char, …): leave the
                    // event LIVE and uncleared so the group/dialog still sees it
                    // (the C++ `default: … else return;`).
                    return;
                }

                if extend_block {
                    self.adjust_select_block();
                } else {
                    self.sel_start = 0;
                    self.sel_end = 0;
                }

                // firstPos scroll-follow (column arithmetic).
                let cur_width = self.displayed_pos(self.cur_pos);
                if self.first_pos > cur_width {
                    self.first_pos = cur_width;
                }
                let i = cur_width - self.state.size.x + 2;
                if self.first_pos < i {
                    self.first_pos = i;
                }
                self.sync_cursor();
                ev.clear();
            }

            // -- evCommand clipboard block (B1+B3, tinputli.cpp:403-428) --------
            // C++: `if ((state & sfSelected) != 0) … case evCommand: …`
            // Faithful: only when selected (the outer guard at the top of
            // handleEvent already returns if !selected, so this arm is always
            // inside the selected block).
            Event::Command(cmd) => {
                match *cmd {
                    // B3: cmCut — copy selection to clipboard, then delete it.
                    // C++: `TStringView sel(data+selStart, selEnd-selStart);
                    //        TClipboard::setText(sel); saveState();
                    //        deleteSelect(); checkValid(True);
                    //        selStart = selEnd = 0; drawView()`.
                    // C++ always calls clearEvent regardless of whether a selection
                    // exists; the clipboard operation is guarded by the selection test.
                    Command::CUT => {
                        if self.sel_start < self.sel_end {
                            let sel = self.data[self.sel_start as usize..self.sel_end as usize]
                                .to_string();
                            ctx.set_clipboard(sel);
                            self.save_state();
                            self.delete_select();
                            self.check_valid(true);
                            self.sel_start = 0;
                            self.sel_end = 0;
                            self.sync_cursor();
                        }
                        ev.clear();
                    }
                    // B3: cmCopy — copy selection to clipboard, keep it.
                    // C++: `TStringView sel(data+selStart, selEnd-selStart);
                    //        TClipboard::setText(sel)`.
                    // C++ always calls clearEvent regardless of whether a selection
                    // exists; the clipboard operation is guarded by the selection test.
                    Command::COPY => {
                        if self.sel_start < self.sel_end {
                            let sel = self.data[self.sel_start as usize..self.sel_end as usize]
                                .to_string();
                            ctx.set_clipboard(sel);
                        }
                        ev.clear();
                    }
                    // B3: cmPaste — request async paste via the broker
                    // (Deferred::InputLinePaste, mirroring EditorPaste). The pump
                    // reads the backend clipboard and inserts at the cursor,
                    // replacing any selection and clamping to max_len.
                    // C++: `TClipboard::requestText()`.
                    Command::PASTE => {
                        if let Some(id) = self.state.id() {
                            ctx.request_input_line_paste(id);
                        }
                        ev.clear();
                    }
                    _ => {}
                }
            }

            _ => {}
        }

        // B1 — command graying: `if (canUpdateCommands()) updateCommands()`
        // (tinputli.cpp:431-432). The early-return for unhandled KeyDown already
        // exited above; this covers mouse, keyboard (handled), and command events.
        if self.can_update_commands() {
            self.update_commands(ctx);
        }
    }

    /// `TInputLine::setState` — base flag flip then, on `sfSelected` (or
    /// `sfActive` while selected), `selectAll(enable, false)`. B1: also pushes
    /// command enable/disable deferred ops via `update_commands` when the
    /// `canUpdateCommands` condition changes (active+selected transition).
    fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
        // B1 — command graying: sample canUpdate BEFORE the flag flip so we can
        // detect the transition (faithful to C++ `updateBefore = canUpdateCommands()`
        // at the start of TInputLine::setState, tinputli.cpp:292).
        let update_before = self.can_update_commands();

        // Base behaviour (replicated from View::set_state — no `super`).
        self.state.set_flag(flag, enable);
        if flag == StateFlag::Focused {
            let source = self.state.id();
            ctx.broadcast(
                if enable {
                    Command::RECEIVED_FOCUS
                } else {
                    Command::RELEASED_FOCUS
                },
                source,
            );
        }
        if flag == StateFlag::Selected || (flag == StateFlag::Active && self.state.state.selected) {
            self.select_all(enable, false);
        }
        // B1 — command graying: faithful to tinputli.cpp TInputLine::setState:
        // `if (updateBefore != updateAfter) updateCommands()`.
        // If the canUpdate condition changed, push the enable/disable deferred ops.
        let update_after = self.can_update_commands();
        if update_before != update_after {
            self.update_commands(ctx);
        }
    }

    /// `TInputLine::valid` — with a validator: `cmValid` → status OK; any other
    /// non-`cmCancel` command runs the validator and fails if invalid. Without a
    /// validator: always valid.
    fn valid(&mut self, cmd: Command, ctx: &mut Context) -> bool {
        if let Some(validator) = &self.validator {
            if cmd == Command::VALID {
                return validator.is_status_ok();
            } else if cmd != Command::CANCEL && !validator.validate(&self.data, ctx) {
                // validator.validate pops the validator's `error()` box (via
                // ctx.request_message_box) on the way to returning false — faithful
                // to TInputLine::valid calling validator->valid() which pops error().
                // TODO(valid-select): C++ valid() calls select() on the bad field
                // before returning false; needs request_focus. The error box +
                // return value are faithful; the focus side-effect is deferred.
                return false;
            }
        }
        true
    }

    /// `TInputLine::getData` — the field's text as a [`FieldValue`] (D10).
    fn value(&self) -> Option<FieldValue> {
        // C++ `getData`: `if (!validator || transfer(data, rec, vtGetData)==0)
        // memcpy(rec, data, …)`. A transfer-enabled validator (TRangeValidator
        // with voTransfer) produces a typed value; otherwise the field's text.
        if let Some(v) = self
            .validator
            .as_ref()
            .and_then(|val| val.transfer_get(&self.data))
        {
            return Some(v);
        }
        Some(FieldValue::Text(self.data.clone()))
    }

    /// `TInputLine::setData` — load text into the field and select-all (D10).
    fn set_value(&mut self, v: FieldValue) {
        // C++ `setData`: `if (!validator || transfer(data, rec, vtSetData)==0) {
        // copy text } ; selectAll(True)`. A transfer-enabled validator formats the
        // typed value into the field text; otherwise the Text path. `selectAll`
        // runs either way.
        if let Some(text) = self.validator.as_ref().and_then(|val| val.transfer_set(&v)) {
            self.data = text;
            self.select_all(true, true);
            return;
        }
        // D10 divergence: when transfer is disabled and `v` is `Int` (not `Text`),
        // the body below is skipped entirely — no data change, no `select_all` —
        // unlike C++ `setData`, which always `memcpy`s + `selectAll(True)`. An
        // `Int` into a non-transfer field is a type mismatch the typed model
        // rightly drops; this is intentional under D10, not an oversight.
        #[allow(irrefutable_let_patterns)]
        if let FieldValue::Text(s) = v {
            // TODO(max_len clamp on set_value): C++ flowback is `strnzcpy(data, s,
            // maxLen+1)` — truncates to maxLen. We assign unclamped (pre-existing
            // row-39 gap; row 57's THistory flowback is the first heavy consumer).
            self.data = s;
            self.select_all(true, true);
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
    use crate::event::{
        KeyEvent, KeyModifiers, MouseButtons, MouseEvent, MouseEventFlags, MouseWheel,
    };
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use crate::timer::TimerQueue;
    use crate::view::{Deferred, Point};
    use std::collections::VecDeque;

    // -- helpers ------------------------------------------------------------

    fn render(il: &mut InputLine) -> String {
        let theme = Theme::classic_blue();
        let size = il.state.size;
        let (backend, screen) = HeadlessBackend::new(size.x as u16, size.y as u16);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = il.state.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            il.draw(&mut dc);
        });
        screen.snapshot()
    }

    fn with_ctx<R>(f: impl FnOnce(&mut Context) -> R) -> (Vec<Event>, R) {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let r = {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            f(&mut ctx)
        };
        (out.into_iter().collect(), r)
    }

    /// Like `with_ctx` but also returns the deferred vec (for capture assertions).
    fn with_ctx_d<R>(f: impl FnOnce(&mut Context) -> R) -> (Vec<Event>, Vec<Deferred>, R) {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let r = {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            f(&mut ctx)
        };
        (out.into_iter().collect(), deferred, r)
    }

    fn key(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(k, KeyModifiers::default()))
    }

    fn ctrl_key(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(
            k,
            KeyModifiers {
                ctrl: true,
                ..Default::default()
            },
        ))
    }

    fn char_key(c: char) -> Event {
        Event::KeyDown(KeyEvent::new(Key::Char(c), KeyModifiers::default()))
    }

    /// Build a selected field with `data`, cursor at the end, no scroll.
    fn field(width: i32, data: &str) -> InputLine {
        let mut il = InputLine::with_limit(Rect::new(0, 0, width, 1), 256);
        il.state.state.selected = true;
        il.data = data.to_string();
        il.cur_pos = data.len() as i32;
        il
    }

    fn send_key(il: &mut InputLine, ev: &mut Event) {
        with_ctx(|ctx| il.handle_event(ev, ctx));
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

    /// Build a selected field with an id (simulating Group::insert).
    fn field_with_id(width: i32, data: &str) -> (InputLine, crate::view::ViewId) {
        let mut il = field(width, data);
        let id = crate::view::ViewId::next();
        il.state.id = Some(id);
        (il, id)
    }

    // -- snapshot -----------------------------------------------------------

    /// A field with content shorter than its width: text from col 1, no arrows.
    #[test]
    fn snapshot_basic_field() {
        let mut il = field(12, "hello");
        il.cur_pos = 0;
        il.first_pos = 0;
        insta::assert_snapshot!(render(&mut il));
    }

    /// A selection covering "ell" is highlighted.
    #[test]
    fn snapshot_with_selection() {
        let mut il = field(12, "hello");
        il.sel_start = 1;
        il.sel_end = 4;
        il.first_pos = 0;
        insta::assert_snapshot!(render(&mut il));
    }

    /// A string wider than the field, scrolled — both arrows present.
    #[test]
    fn snapshot_scrolled_both_arrows() {
        let mut il = field(8, "abcdefghijklmnop");
        il.first_pos = 4;
        il.cur_pos = 8;
        insta::assert_snapshot!(render(&mut il));
    }

    /// DISCRIMINATING: a scrolled field whose selection covers the start
    /// (`sel_start = 0`, off the visible left edge). Because rstv REDRAWS the
    /// selection (no attr-only paint), the highlighted cells must show the
    /// *visible scrolled glyphs* ("efgh…"), NOT the head of the string ("abcd…").
    /// This is exactly the state `select_all(true, true)` produces on a long
    /// field gaining focus.
    #[test]
    fn snapshot_scrolled_selection_from_start() {
        let mut il = field(8, "abcdefghijklmnop");
        il.first_pos = 4; // visible window starts at column 4 ('e')
        il.sel_start = 0; // selection begins off the left edge
        il.sel_end = 16; // …and runs to the end
        il.cur_pos = 16;
        let snap = render(&mut il);
        // The visible text is "efghij" (cols 1..7), arrows at 0 and 7. The
        // highlighted glyphs must be the scrolled ones, not "abcd…".
        assert!(
            snap.contains("◄efghij►"),
            "selection redraw must keep the scrolled glyphs, got:\n{snap}"
        );
        insta::assert_snapshot!(snap);
    }

    // -- editing: ASCII -----------------------------------------------------

    #[test]
    fn insert_char_at_cursor() {
        let mut il = field(12, "");
        let mut ev = char_key('a');
        send_key(&mut il, &mut ev);
        assert!(ev.is_nothing());
        assert_eq!(il.data, "a");
        assert_eq!(il.cur_pos, 1);
    }

    #[test]
    fn home_end_left_right() {
        let mut il = field(12, "abc");
        let mut ev = key(Key::Home);
        send_key(&mut il, &mut ev);
        assert_eq!(il.cur_pos, 0);
        let mut ev = key(Key::Right);
        send_key(&mut il, &mut ev);
        assert_eq!(il.cur_pos, 1);
        let mut ev = key(Key::End);
        send_key(&mut il, &mut ev);
        assert_eq!(il.cur_pos, 3);
        let mut ev = key(Key::Left);
        send_key(&mut il, &mut ev);
        assert_eq!(il.cur_pos, 2);
    }

    #[test]
    fn backspace_and_delete() {
        let mut il = field(12, "abc"); // cur at 3
        let mut ev = key(Key::Backspace);
        send_key(&mut il, &mut ev);
        assert_eq!(il.data, "ab");
        assert_eq!(il.cur_pos, 2);

        let mut il = field(12, "abc");
        il.cur_pos = 0;
        let mut ev = key(Key::Delete);
        send_key(&mut il, &mut ev);
        assert_eq!(il.data, "bc");
        assert_eq!(il.cur_pos, 0);
    }

    #[test]
    fn ctrl_y_clears() {
        let mut il = field(12, "abc");
        let mut ev = ctrl_key(Key::Char('y'));
        send_key(&mut il, &mut ev);
        assert_eq!(il.data, "");
        assert_eq!(il.cur_pos, 0);
        assert!(ev.is_nothing());
    }

    #[test]
    fn ctrl_word_nav() {
        let mut il = field(20, "foo bar baz");
        il.cur_pos = 0;
        let mut ev = ctrl_key(Key::Right);
        send_key(&mut il, &mut ev);
        assert_eq!(il.cur_pos, 4, "to start of 'bar'");
        let mut ev = ctrl_key(Key::Right);
        send_key(&mut il, &mut ev);
        assert_eq!(il.cur_pos, 8, "to start of 'baz'");
        let mut ev = ctrl_key(Key::Left);
        send_key(&mut il, &mut ev);
        assert_eq!(il.cur_pos, 4, "back to start of 'bar'");
    }

    #[test]
    fn unhandled_key_left_live() {
        // Tab and Enter are not edit/nav keys: the event must stay live so the
        // dialog can act on it (Esc/Enter routing).
        let mut il = field(12, "abc");
        let mut ev = key(Key::Tab);
        send_key(&mut il, &mut ev);
        assert!(!ev.is_nothing(), "Tab must propagate uncleared");
        let mut ev = key(Key::Enter);
        send_key(&mut il, &mut ev);
        assert!(!ev.is_nothing(), "Enter must propagate uncleared");
    }

    #[test]
    fn ins_toggles_cursor_ins() {
        let mut il = field(12, "abc");
        assert!(!il.state.state.cursor_ins);
        let mut ev = key(Key::Insert);
        send_key(&mut il, &mut ev);
        assert!(il.state.state.cursor_ins);
        let mut ev = key(Key::Insert);
        send_key(&mut il, &mut ev);
        assert!(!il.state.state.cursor_ins);
    }

    // -- editing: MULTI-BYTE (mandatory) ------------------------------------

    #[test]
    fn insert_multibyte_no_panic() {
        let mut il = field(12, "");
        let mut ev = char_key('ä'); // 2 bytes
        send_key(&mut il, &mut ev);
        assert_eq!(il.data, "ä");
        assert_eq!(il.cur_pos, 2, "cursor advances by byte length");
        let mut ev = char_key('€'); // 3 bytes
        send_key(&mut il, &mut ev);
        assert_eq!(il.data, "ä€");
        assert_eq!(il.cur_pos, 5);
    }

    #[test]
    fn backspace_over_multibyte_no_panic() {
        let mut il = field(12, "aä€"); // bytes: a(1) ä(2) €(3) = 6
        assert_eq!(il.cur_pos, 6);
        let mut ev = key(Key::Backspace);
        send_key(&mut il, &mut ev);
        assert_eq!(il.data, "aä", "deleted the 3-byte €");
        assert_eq!(il.cur_pos, 3);
        let mut ev = key(Key::Backspace);
        send_key(&mut il, &mut ev);
        assert_eq!(il.data, "a", "deleted the 2-byte ä");
        assert_eq!(il.cur_pos, 1);
    }

    #[test]
    fn delete_multibyte_no_panic() {
        let mut il = field(12, "aä€");
        il.cur_pos = 1; // before ä
        let mut ev = key(Key::Delete);
        send_key(&mut il, &mut ev);
        assert_eq!(il.data, "a€", "deleted the 2-byte ä");
        assert_eq!(il.cur_pos, 1);
    }

    #[test]
    fn left_right_over_multibyte_no_panic() {
        let mut il = field(12, "aä€");
        il.cur_pos = 0;
        let mut ev = key(Key::Right);
        send_key(&mut il, &mut ev);
        assert_eq!(il.cur_pos, 1, "over 'a'");
        let mut ev = key(Key::Right);
        send_key(&mut il, &mut ev);
        assert_eq!(il.cur_pos, 3, "over 2-byte 'ä'");
        let mut ev = key(Key::Right);
        send_key(&mut il, &mut ev);
        assert_eq!(il.cur_pos, 6, "over 3-byte '€'");
        let mut ev = key(Key::Left);
        send_key(&mut il, &mut ev);
        assert_eq!(il.cur_pos, 3, "back over '€'");
    }

    #[test]
    fn ctrl_word_nav_over_multibyte_no_panic() {
        // Words separated by ASCII spaces, but containing multibyte chars.
        let mut il = field(20, "äé foo");
        il.cur_pos = 0;
        let mut ev = ctrl_key(Key::Right);
        send_key(&mut il, &mut ev);
        // "äé" is 4 bytes, then a space at byte 4, then 'f' at byte 5.
        assert_eq!(il.cur_pos, 5, "to start of 'foo'");
        let mut ev = ctrl_key(Key::Left);
        send_key(&mut il, &mut ev);
        assert_eq!(il.cur_pos, 0, "back to start");
    }

    // -- valid() ------------------------------------------------------------

    struct RejectAll;
    impl Validator for RejectAll {
        fn is_valid(&self, _s: &str) -> bool {
            false
        }
    }

    #[test]
    fn valid_with_rejecting_validator() {
        let mut il = InputLine::new(
            Rect::new(0, 0, 12, 1),
            256,
            Some(Box::new(RejectAll)),
            LimitMode::MaxBytes,
        );
        il.data = "x".to_string();
        // A non-cancel command runs validate → false (and requests an error box).
        assert!(
            !with_ctx(|ctx| il.valid(Command::OK, ctx)).1,
            "rejecting validator blocks OK"
        );
        // cmCancel always passes (no validation).
        assert!(
            with_ctx(|ctx| il.valid(Command::CANCEL, ctx)).1,
            "cmCancel bypasses validation"
        );
        // cmValid consults status (RejectAll's status is the default OK).
        assert!(
            with_ctx(|ctx| il.valid(Command::VALID, ctx)).1,
            "cmValid consults status (OK)"
        );
    }

    #[test]
    fn valid_without_validator_is_true() {
        let mut il = field(12, "anything");
        assert!(with_ctx(|ctx| il.valid(Command::OK, ctx)).1);
        assert!(with_ctx(|ctx| il.valid(Command::CANCEL, ctx)).1);
        assert!(with_ctx(|ctx| il.valid(Command::VALID, ctx)).1);
    }

    /// A validator that rejects every candidate keystroke (`isValidInput` →
    /// false), but leaves `isValid` at its default. This exercises the
    /// reject/restore path in `check_valid`: the keyboard insertion branch
    /// `save_state()`s, mutates, then `check_valid(true)` rejects and
    /// `restore_state()`s — which the existing `is_valid`-only validators never
    /// reach (their `check_valid(true)` always returns true).
    struct RejectAllInput;
    impl Validator for RejectAllInput {
        fn is_valid_input(&self, _s: &mut String, _suppress: bool) -> bool {
            false
        }
    }

    /// DISCRIMINATING: with a validator that rejects all input, a printable
    /// keystroke must revert the field to its pre-keystroke `data`/`cur_pos`,
    /// proving `save_state` → `check_valid(true)`'s reject branch →
    /// `restore_state` ran (the `is_valid`-only validators above never reach it,
    /// since their `check_valid(true)` always returns true).
    ///
    /// BITES if `restore_state`'s body is no-op'd: the keystroke first
    /// `delete_select()`s the active selection (`"bc"`), so without restore the
    /// field would be left mutated (`data == "adef"`, `cur_pos == 1`) instead of
    /// reverted (`data == "abcdef"`, `cur_pos == 4`).
    ///
    /// Note: `sel_start`/`sel_end` are NOT asserted against the saved 1/3 — the
    /// faithful C++ resets `selStart = selEnd = 0` after a printable key
    /// regardless of the validator outcome (`tinputli.cpp:459`, rstv lines
    /// 643-645), so those offsets legitimately end at 0 here.
    #[test]
    fn rejected_input_restores_pre_keystroke_state() {
        let mut il = InputLine::new(
            Rect::new(0, 0, 12, 1),
            256,
            Some(Box::new(RejectAllInput)),
            LimitMode::MaxBytes,
        );
        il.state.state.selected = true;
        // A non-empty active selection + a cursor not at 0, so that the
        // keystroke's delete_select() visibly mutates the field and a no-op'd
        // restore_state would leave that mutation in place.
        il.data = "abcdef".to_string();
        il.cur_pos = 4;
        il.first_pos = 0;
        il.sel_start = 1;
        il.sel_end = 3;

        // Drive a printable keystroke. The insertion branch does
        // save_state → delete_select → check_valid(true)=false → restore_state.
        let mut ev = char_key('Z');
        send_key(&mut il, &mut ev);

        // Reverted to the saved snapshot: the rejected input changed nothing.
        assert_eq!(il.data, "abcdef", "rejected input must not change data");
        assert_eq!(il.cur_pos, 4, "cur_pos restored from save_state");
        assert_eq!(il.first_pos, 0, "first_pos restored from save_state");
    }

    // -- value / set_value (D10) -------------------------------------------

    #[test]
    fn value_set_value_round_trip() {
        let mut il = field(12, "");
        il.set_value(FieldValue::Text("hello".to_string()));
        assert_eq!(il.data, "hello");
        assert_eq!(il.value(), Some(FieldValue::Text("hello".to_string())));
        // set_value selects all (enable=true): cursor at end, sel covers all.
        assert_eq!(il.sel_start, 0);
        assert_eq!(il.sel_end, 5);
        assert_eq!(il.cur_pos, 5);
    }

    // -- value / set_value with a transfer-enabled validator (row 59) -------

    /// REGRESSION GUARD: with NO validator, `value()` still yields `Text`
    /// (the transfer hooks default to `None`, so the Text path is unchanged).
    #[test]
    fn value_no_validator_is_text() {
        let mut il = field(12, "");
        il.data = "42".to_string();
        assert_eq!(il.value(), Some(FieldValue::Text("42".to_string())));
    }

    /// REGRESSION GUARD: a validator is PRESENT but transfer is OFF — `value()`
    /// must still yield `Text`. Pins the `and_then(...) → None` fall-through (the
    /// exact path that breaks if the transfer gate is later inverted or
    /// `transfer_get` made unconditional); no other test bites it.
    #[test]
    fn value_with_non_transfer_validator_is_text() {
        use crate::validate::RangeValidator;
        // RangeValidator with transfer NOT enabled → transfer_get returns None.
        let rv = RangeValidator::new(0, 100);
        let mut il = InputLine::new(
            Rect::new(0, 0, 12, 1),
            256,
            Some(Box::new(rv)),
            LimitMode::MaxBytes,
        );
        il.data = "42".to_string();
        assert_eq!(il.value(), Some(FieldValue::Text("42".to_string())));
    }

    #[test]
    fn value_with_transfer_validator_is_int() {
        use crate::validate::RangeValidator;
        let mut rv = RangeValidator::new(0, 100);
        rv.set_transfer(true);
        let mut il = InputLine::new(
            Rect::new(0, 0, 12, 1),
            256,
            Some(Box::new(rv)),
            LimitMode::MaxBytes,
        );
        il.data = "42".to_string();
        assert_eq!(il.value(), Some(FieldValue::Int(42)));
    }

    #[test]
    fn set_value_with_transfer_validator_formats_int() {
        use crate::validate::RangeValidator;
        let mut rv = RangeValidator::new(0, 100);
        rv.set_transfer(true);
        let mut il = InputLine::new(
            Rect::new(0, 0, 12, 1),
            256,
            Some(Box::new(rv)),
            LimitMode::MaxBytes,
        );
        il.set_value(FieldValue::Int(42));
        assert_eq!(il.data, "42", "Int formatted into the field text");
        // selectAll runs on the transfer path too.
        assert_eq!(il.sel_start, 0);
        assert_eq!(il.sel_end, 2);
        assert_eq!(il.cur_pos, 2);
    }

    // -- cursor / firstPos scroll-follow ------------------------------------

    /// ASCII scroll-follow: a string wider than the field, cursor at the end.
    /// firstPos must follow so the cursor stays visible.
    #[test]
    fn scroll_follow_ascii() {
        // width 6 (so text area is cols 1..6 = 5 columns).
        let mut il = field(6, "");
        for c in "abcdefgh".chars() {
            let mut ev = char_key(c);
            send_key(&mut il, &mut ev);
        }
        assert_eq!(il.data, "abcdefgh");
        assert_eq!(il.cur_pos, 8);
        // cur_width = 8. firstPos clamp: i = cur_width - size.x + 2 = 8-6+2 = 4.
        assert_eq!(il.first_pos, 4, "firstPos follows the cursor (column)");
        // cursor screen col = displayedPos(curPos) - firstPos + 1 = 8-4+1 = 5.
        assert_eq!(il.state.cursor.x, 5);
    }

    /// DISCRIMINATING multibyte scroll-follow: distinguishes the column-vs-byte
    /// bug. A field of WIDE glyphs ("中中中中", each 2 columns wide, 3 bytes) in a
    /// field narrower than its width: firstPos must be in COLUMNS, not bytes.
    #[test]
    fn scroll_follow_wide_glyphs_is_columns_not_bytes() {
        // size.x = 6 → text area 5 columns. "中" is width 2, len 3 bytes.
        let mut il = field(6, "");
        for _ in 0..4 {
            let mut ev = char_key('中');
            send_key(&mut il, &mut ev);
        }
        // data = "中中中中": 12 bytes, 8 display columns.
        assert_eq!(il.data.len(), 12);
        assert_eq!(text::width(&il.data), 8);
        assert_eq!(il.cur_pos, 12, "cur_pos is a BYTE offset (12)");
        // cur_width (COLUMNS) = displayedPos(12) = 8.
        // firstPos clamp: i = 8 - 6 + 2 = 4 (a COLUMN). If firstPos were treated
        // as bytes it would be 12 - 6 + 2 = 8 — this asserts the column value.
        assert_eq!(
            il.first_pos, 4,
            "firstPos is a display COLUMN (4), not a byte offset (would be 8)"
        );
        // cursor screen col = displayedPos(curPos) - firstPos + 1 = 8 - 4 + 1 = 5.
        assert_eq!(
            il.state.cursor.x, 5,
            "cursor column = displayedPos(curPos) - firstPos + 1"
        );
    }

    // -- mouse single-shot --------------------------------------------------

    #[test]
    fn mouse_down_positions_cursor() {
        let mut il = field(12, "hello");
        il.cur_pos = 0;
        // Click at view-local x=3 → pos = mouse.x + firstPos - 1 = 3 + 0 - 1 = 2.
        let mut ev = Event::MouseDown(MouseEvent {
            position: Point::new(3, 0),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        });
        send_key(&mut il, &mut ev);
        assert!(ev.is_nothing(), "mouse-down is consumed");
        assert_eq!(il.cur_pos, 2);
    }

    /// DISCRIMINATING (M1): a field NARROWER than its content, double-clicked,
    /// must select-all AND SCROLL the end into view. The C++ selectAll(True)'s
    /// scroll arg defaults to True (dialogs.h:177); the `false` regression would
    /// leave first_pos at 0.
    #[test]
    fn double_click_scrolls_end_into_view() {
        // width 8, content 16 cols. Click at x=3 (not an edge → delta 0, so the
        // double-click branch runs).
        let mut il = field(8, "abcdefghijklmnop");
        il.cur_pos = 0;
        il.first_pos = 0;
        let mut ev = Event::MouseDown(MouseEvent {
            position: Point::new(3, 0),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            flags: crate::event::MouseEventFlags {
                double_click: true,
                ..Default::default()
            },
            ..Default::default()
        });
        send_key(&mut il, &mut ev);
        assert!(ev.is_nothing(), "mouse-down is consumed");
        // select_all(true, true): cur_pos = 16, sel covers all.
        assert_eq!(il.cur_pos, 16);
        assert_eq!(il.sel_start, 0);
        assert_eq!(il.sel_end, 16);
        // The scroll: first_pos = max(0, displayedPos(16) - 8 + 2) = 16-8+2 = 10.
        // With the `false` regression this would stay 0 — so this BITES.
        assert!(
            il.first_pos > 0,
            "double-click must scroll the end into view (first_pos > 0), got {}",
            il.first_pos
        );
        assert_eq!(
            il.first_pos, 10,
            "first_pos = displayedPos(curPos) - size.x + 2"
        );
    }

    /// DISCRIMINATING (m1): a single edge mouse-down on a scrolled-out field
    /// scrolls by ONE step (the C++ do/while's first iteration always runs),
    /// before any deferred press-and-hold auto-repeat.
    #[test]
    fn single_edge_mouse_down_scrolls_one_step() {
        // width 8, content 16 cols, scrolled so the left edge can scroll left.
        let mut il = field(8, "abcdefghijklmnop");
        il.first_pos = 5;
        il.cur_pos = 8;
        // Click at the left edge (x=0 → mouse_delta = -1, can_scroll(-1) true).
        let mut ev = Event::MouseDown(MouseEvent {
            position: Point::new(0, 0),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        });
        send_key(&mut il, &mut ev);
        assert!(ev.is_nothing(), "mouse-down is consumed");
        assert_eq!(
            il.first_pos, 4,
            "single edge mouse-down scrolls one step left"
        );

        // Right edge: x = size.x - 1 = 7 → mouse_delta = 1. can_scroll(1) true
        // while content extends past the window.
        let mut il = field(8, "abcdefghijklmnop");
        il.first_pos = 2;
        il.cur_pos = 8;
        let mut ev = Event::MouseDown(MouseEvent {
            position: Point::new(7, 0),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        });
        send_key(&mut il, &mut ev);
        assert_eq!(
            il.first_pos, 3,
            "single edge mouse-down scrolls one step right"
        );
    }

    // -- set_state selects all ----------------------------------------------

    #[test]
    fn set_state_selected_selects_all() {
        let mut il = field(12, "hello");
        il.sel_start = 0;
        il.sel_end = 0;
        il.state.state.selected = false;
        with_ctx(|ctx| il.set_state(StateFlag::Selected, true, ctx));
        assert!(il.state.state.selected);
        assert_eq!(il.sel_start, 0);
        assert_eq!(il.sel_end, 5, "selectAll on becoming selected");
        assert_eq!(il.cur_pos, 5);
    }

    // -- A3 seam: drag-select tracking (evMouseMove | evMouseAuto) -----------
    //
    // These tests drive the tracking arms directly (as the pump's Deferred::MouseTrack
    // apply does) with field-local positions. The seam itself is unit-tested in
    // capture::tests; here we verify the widget's loop body is correct.

    /// Mouse-down in the text area (not on an edge) arms drag-select tracking:
    /// anchor + cursor at the click, `tracking == true`, `tracking_drag == true`,
    /// and a `PushCapture` is deferred.
    #[test]
    fn track_drag_mouse_down_arms_capture() {
        let (mut il, _id) = field_with_id(20, "hello world");
        // Mouse-down at col 4 (character 'd' area, well inside the field).
        let mut ev = mouse_down_at(4, 0);
        let (_, deferred, ()) = with_ctx_d(|ctx| il.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "mouse-down consumed");
        assert!(il.tracking, "tracking armed");
        assert!(il.tracking_drag, "drag branch selected");
        assert!(
            deferred
                .iter()
                .any(|d| matches!(d, Deferred::PushCapture(_))),
            "PushCapture deferred for drag-select tracking"
        );
        // anchor and cur_pos are both at the clicked column (field-local: col 4 →
        // first_pos=0, so byte offset for col 3 = 3).
        assert_eq!(il.anchor, il.cur_pos, "anchor == cursor at click time");
    }

    /// `MouseMove` while tracking (drag-select): cursor moves to the new position
    /// and the selection block is adjusted (tinputli.cpp:332-334).
    #[test]
    fn track_drag_move_extends_selection() {
        let (mut il, _id) = field_with_id(20, "hello world");
        // Down at col 2 (byte 1 = 'e').
        let mut ev = mouse_down_at(2, 0);
        with_ctx_d(|ctx| il.handle_event(&mut ev, ctx));
        assert!(il.tracking_drag);

        // Move to col 8 (byte 7 = 'o').
        let mut ev = mouse_move_at(8, 0);
        let (_, _, ()) = with_ctx_d(|ctx| il.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "tracked move consumed");
        assert!(il.tracking, "still tracking after move");
        // sel_start <= anchor, sel_end >= anchor.
        assert!(il.sel_start <= il.anchor, "sel_start ≤ anchor");
        assert!(il.sel_end >= il.anchor, "sel_end ≥ anchor");
        // The selection must span more than a single point.
        assert!(
            il.sel_end > il.sel_start,
            "selection extends: sel_start={} sel_end={}",
            il.sel_start,
            il.sel_end
        );
    }

    /// `MouseAuto` while tracking (drag-select): if the cursor is at the left edge
    /// (x <= 0), `first_pos` decrements (edge-scroll; tinputli.cpp:327-330).
    /// Also updates `cur_pos` and `sel` (the drag body still runs after edge-scroll).
    #[test]
    fn track_drag_auto_edge_scrolls_and_updates_cursor() {
        // Build a long string in a narrow field so we can trigger the edge scroll.
        let (mut il, _id) = field_with_id(8, "abcdefghijklmnop");
        il.first_pos = 5; // scrolled right a bit
        il.cur_pos = 5; // cursor in the middle

        // Mouse-down in the middle of the field to arm tracking.
        let mut ev = mouse_down_at(4, 0);
        with_ctx_d(|ctx| il.handle_event(&mut ev, ctx));
        let first_pos_before = il.first_pos;

        // MouseAuto at x = 0 (left edge): `mouse_delta` returns -1, `can_scroll(-1)`
        // should return true (first_pos > 0), so first_pos should decrease.
        let mut ev = mouse_auto_at(0, 0);
        with_ctx_d(|ctx| il.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "tracked auto consumed");
        assert!(il.tracking, "still tracking");
        assert!(
            il.first_pos < first_pos_before,
            "first_pos scrolled left: was {}, now {}",
            first_pos_before,
            il.first_pos
        );
    }

    /// `MouseUp` clears the tracking flag (post-loop code).
    #[test]
    fn track_drag_mouse_up_clears_tracking() {
        let (mut il, _id) = field_with_id(20, "hello world");
        let mut ev = mouse_down_at(4, 0);
        with_ctx_d(|ctx| il.handle_event(&mut ev, ctx));
        assert!(il.tracking);

        let mut ev = mouse_up_at(4, 0);
        with_ctx_d(|ctx| il.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "MouseUp consumed");
        assert!(!il.tracking, "tracking cleared on MouseUp");
    }

    /// A stray `MouseUp` (not tracking) falls through — the guard.
    #[test]
    fn track_stray_mouse_up_falls_through() {
        let (mut il, _id) = field_with_id(20, "hello world");
        // No tracking armed.
        let mut ev = mouse_up_at(4, 0);
        let (_, _, ()) = with_ctx_d(|ctx| il.handle_event(&mut ev, ctx));
        assert!(
            !ev.is_nothing(),
            "stray MouseUp falls through (not consumed)"
        );
    }

    /// A `MouseMove` during an EDGE-scroll track falls through unconsumed —
    /// the C++ edge loop is auto-only-masked (two-separate-masked-loops, same
    /// guard discipline as the scrollbar arms).
    #[test]
    fn track_move_during_edge_track_falls_through() {
        let (mut il, _id) = field_with_id(8, "abcdefghijklmnop");
        il.first_pos = 0;

        // Arm the edge branch (auto-only) via a right-edge down.
        let mut down = mouse_down_at(7, 0);
        with_ctx_d(|ctx| il.handle_event(&mut down, ctx));
        assert!(il.tracking && !il.tracking_drag, "edge track armed");
        let cur_pos_before = il.cur_pos;

        // A MouseMove must fall through: unconsumed, no cursor/selection change.
        let mut mv = mouse_move_at(3, 0);
        with_ctx_d(|ctx| il.handle_event(&mut mv, ctx));
        assert!(
            !mv.is_nothing(),
            "MouseMove during edge track falls through unconsumed"
        );
        assert_eq!(il.cur_pos, cur_pos_before, "no cursor change");
    }

    // -- A3 seam: edge auto-scroll tracking (evMouseAuto only) ---------------

    /// Mouse-down on the right scroll edge (x >= size.x-1) arms auto-only tracking
    /// and does the first `first_pos += delta` step
    /// (tinputli.cpp:313-320 first iteration).
    #[test]
    fn track_edge_scroll_arms_capture() {
        // Field width 8, long string so can_scroll(1) is true.
        let (mut il, _id) = field_with_id(8, "abcdefghijklmnop");
        il.first_pos = 0;

        // x = size.x - 1 = 7 → mouse_delta returns +1.
        let mut ev = mouse_down_at(7, 0);
        let (_, deferred, ()) = with_ctx_d(|ctx| il.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "consumed");
        assert!(il.tracking, "tracking armed for edge scroll");
        assert!(!il.tracking_drag, "edge branch, not drag branch");
        assert!(
            deferred
                .iter()
                .any(|d| matches!(d, Deferred::PushCapture(_))),
            "PushCapture deferred for edge auto-scroll"
        );
        // First iteration ran: first_pos scrolled right by 1.
        assert_eq!(il.first_pos, 1, "first_pos incremented on first iteration");
    }

    /// `MouseAuto` while edge-scroll tracking repeats the scroll
    /// (tinputli.cpp:315-318).
    #[test]
    fn track_edge_scroll_auto_repeats() {
        let (mut il, _id) = field_with_id(8, "abcdefghijklmnop");
        il.first_pos = 0;

        // Arm tracking via right-edge down.
        let mut ev = mouse_down_at(7, 0);
        with_ctx_d(|ctx| il.handle_event(&mut ev, ctx));
        let first_pos_after_down = il.first_pos;

        // MouseAuto at x = 7 again → delta = +1, can_scroll(1) = true.
        let mut ev = mouse_auto_at(7, 0);
        with_ctx_d(|ctx| il.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "auto consumed");
        assert!(
            il.first_pos > first_pos_after_down,
            "first_pos scrolled further: was {}, now {}",
            first_pos_after_down,
            il.first_pos
        );
        // Cursor should NOT have been moved (edge branch does not drag-select).
        assert!(!il.tracking_drag, "edge branch stays edge branch");
    }

    // -- B1: updateCommands / canUpdateCommands graying ----------------------

    /// `canUpdateCommands()` is true only when both active AND selected.
    #[test]
    fn b1_can_update_commands_requires_active_and_selected() {
        let mut il = field(12, "hello");
        il.state.state.selected = true;
        il.state.state.active = false;
        assert!(
            !il.can_update_commands(),
            "selected but not active → canUpdate = false"
        );
        il.state.state.active = true;
        assert!(
            il.can_update_commands(),
            "both active+selected → canUpdate = true"
        );
    }

    /// When active+selected, a handled key event pushes deferred
    /// Enable/Disable ops for cmCut/cmCopy based on whether a selection exists
    /// (faithful to updateCommands tail at tinputli.cpp:431).
    #[test]
    fn b1_key_event_updates_commands_when_active_selected() {
        let mut il = field(12, "hello");
        il.state.state.active = true;
        // Build a selection: sel_start=0, sel_end=3.
        il.sel_start = 0;
        il.sel_end = 3;

        // A Home key: no selection change, but the tail always runs.
        let mut ev = key(Key::Home);
        let (_, deferred, ()) = with_ctx_d(|ctx| il.handle_event(&mut ev, ctx));
        // After Home, sel cleared (not extend_block): sel_start=sel_end=0.
        // So cmCut/cmCopy should be DISABLED.
        let has_disable_cut = deferred
            .iter()
            .any(|d| matches!(d, Deferred::DisableCommand(Command::CUT)));
        let has_disable_copy = deferred
            .iter()
            .any(|d| matches!(d, Deferred::DisableCommand(Command::COPY)));
        let has_enable_paste = deferred
            .iter()
            .any(|d| matches!(d, Deferred::EnableCommand(Command::PASTE)));
        assert!(has_disable_cut, "cmCut disabled when no selection");
        assert!(has_disable_copy, "cmCopy disabled when no selection");
        assert!(
            has_enable_paste,
            "cmPaste always enabled when active+selected"
        );
    }

    /// Shift+Home extends the selection, so cmCut/cmCopy should be ENABLED.
    #[test]
    fn b1_extend_selection_enables_cut_copy() {
        let mut il = field(12, "hello");
        il.state.state.active = true;
        il.cur_pos = 5; // at end
        il.sel_start = 0;
        il.sel_end = 0;

        // Shift+Home: extends selection from end to start.
        let mut ev = Event::KeyDown(KeyEvent::new(
            Key::Home,
            KeyModifiers {
                shift: true,
                ..Default::default()
            },
        ));
        let (_, deferred, ()) = with_ctx_d(|ctx| il.handle_event(&mut ev, ctx));
        // sel_start=0, sel_end=5 → selection exists → cmCut/cmCopy enabled.
        let has_enable_cut = deferred
            .iter()
            .any(|d| matches!(d, Deferred::EnableCommand(Command::CUT)));
        let has_enable_copy = deferred
            .iter()
            .any(|d| matches!(d, Deferred::EnableCommand(Command::COPY)));
        assert!(has_enable_cut, "cmCut enabled with selection");
        assert!(has_enable_copy, "cmCopy enabled with selection");
    }

    /// `set_state` with sfSelected (gaining focus) transitions canUpdate from
    /// false→true, so update_commands is pushed (deferred ops for cmCut/cmCopy/
    /// cmPaste appear).
    #[test]
    fn b1_set_state_selected_pushes_update_commands() {
        let mut il = field(12, "");
        il.state.state.active = true;
        il.state.state.selected = false;

        // set_state(Selected, true) → canUpdate transitions false→true.
        let (_, deferred, ()) = with_ctx_d(|ctx| il.set_state(StateFlag::Selected, true, ctx));
        let has_any_cmd = deferred
            .iter()
            .any(|d| matches!(d, Deferred::EnableCommand(_) | Deferred::DisableCommand(_)));
        assert!(
            has_any_cmd,
            "set_state(Selected,true) must push command enable/disable ops (B1)"
        );
    }

    // -- B3: clipboard cut/copy/paste ----------------------------------------

    /// cmCut with a selection: copies to clipboard (SetClipboard deferred) and
    /// deletes the selection from the field (faithful to tinputli.cpp:408-417).
    #[test]
    fn b3_cut_copies_to_clipboard_and_deletes_selection() {
        let mut il = field(12, "hello world");
        il.state.state.active = true;
        il.sel_start = 6; // "world"
        il.sel_end = 11;

        let mut ev = Event::Command(Command::CUT);
        let (_, deferred, ()) = with_ctx_d(|ctx| il.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "cmCut is consumed");
        // Selection text copied to clipboard via deferred SetClipboard.
        let clipboard_text = deferred.iter().find_map(|d| {
            if let Deferred::SetClipboard(s) = d {
                Some(s.as_str())
            } else {
                None
            }
        });
        assert_eq!(
            clipboard_text,
            Some("world"),
            "SetClipboard must carry the selection text"
        );
        // The selection is deleted from the field.
        assert_eq!(il.data, "hello ", "selection deleted after cut");
        assert_eq!(il.sel_start, 0);
        assert_eq!(il.sel_end, 0);
    }

    /// cmCopy with a selection: copies to clipboard, field unchanged.
    #[test]
    fn b3_copy_copies_to_clipboard_keeps_selection() {
        let mut il = field(12, "hello world");
        il.state.state.active = true;
        il.sel_start = 0;
        il.sel_end = 5;
        let data_before = il.data.clone();

        let mut ev = Event::Command(Command::COPY);
        let (_, deferred, ()) = with_ctx_d(|ctx| il.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "cmCopy is consumed");
        let clipboard_text = deferred.iter().find_map(|d| {
            if let Deferred::SetClipboard(s) = d {
                Some(s.as_str())
            } else {
                None
            }
        });
        assert_eq!(clipboard_text, Some("hello"));
        assert_eq!(il.data, data_before, "copy does not modify the field");
    }

    /// cmPaste defers an InputLinePaste with the field's id.
    #[test]
    fn b3_paste_defers_input_line_paste_with_id() {
        let (mut il, id) = field_with_id(20, "hello");
        il.state.state.active = true;

        let mut ev = Event::Command(Command::PASTE);
        let (_, deferred, ()) = with_ctx_d(|ctx| il.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "cmPaste is consumed");
        let paste_id = deferred.iter().find_map(|d| {
            if let Deferred::InputLinePaste(i) = d {
                Some(*i)
            } else {
                None
            }
        });
        assert_eq!(
            paste_id,
            Some(id),
            "InputLinePaste deferred with the field's own id"
        );
    }

    /// `paste_text` inserts at the cursor, replacing any selection, clamped to
    /// max_len.
    #[test]
    fn b3_paste_text_inserts_at_cursor_replaces_selection_clamps() {
        // Paste with no selection: inserts at cursor.
        let mut il = field(20, "helo");
        il.cur_pos = 3; // before 'o', position of missing 'l'
        il.paste_text("l");
        assert_eq!(il.data, "hello", "paste inserts at cursor");
        assert_eq!(il.cur_pos, 4, "cursor advances past inserted text");

        // Paste replacing a selection.
        let mut il = field(20, "hello world");
        il.sel_start = 6;
        il.sel_end = 11;
        il.paste_text("Rust");
        assert_eq!(il.data, "hello Rust", "paste replaces selection");
        assert_eq!(il.sel_start, 0);
        assert_eq!(il.sel_end, 0);

        // Paste clamped to max_len (limit=5 → max_len=4 bytes).
        let mut il = InputLine::with_limit(Rect::new(0, 0, 20, 1), 5);
        il.state.state.selected = true;
        il.data = "abc".to_string();
        il.cur_pos = 3;
        il.paste_text("XXXX"); // 4 bytes, only 1 fits (max_len=4, 3 already used)
        assert_eq!(
            il.data, "abcX",
            "paste clamped to max_len: only 1 char fits"
        );
        assert_eq!(il.cur_pos, 4);
    }

    /// cmCut without a selection is a no-op data-wise but the event IS consumed.
    /// C++ always calls clearEvent for cmCut/cmCopy regardless of selection;
    /// the clipboard operation is only performed when a selection exists.
    #[test]
    fn b3_cut_without_selection_consumes_event() {
        let mut il = field(12, "hello");
        il.state.state.active = true;
        il.sel_start = 0;
        il.sel_end = 0; // no selection

        let data_before = il.data.clone();
        let mut ev = Event::Command(Command::CUT);
        let (_, deferred, ()) = with_ctx_d(|ctx| il.handle_event(&mut ev, ctx));
        // C++ always clears the event for cmCut/cmCopy, even without a selection.
        assert!(
            ev.is_nothing(),
            "cmCut always consumes the event (C++ clearEvent)"
        );
        assert_eq!(il.data, data_before, "no data change without selection");
        assert!(
            !deferred
                .iter()
                .any(|d| matches!(d, Deferred::SetClipboard(_))),
            "no SetClipboard when no selection"
        );
    }
}
