//! A single-line text-entry field with selection, horizontal scrolling, an
//! optional [`Validator`], and the typed [`value`](View::value)/
//! [`set_value`](View::set_value) data protocol.
//!
//! # Coordinate model — byte offsets vs. display columns
//!
//! `data` is a Rust [`String`]. The cursor/selection offsets
//! [`cur_pos`](InputLine::cur_pos) / [`sel_start`](InputLine::sel_start) /
//! [`sel_end`](InputLine::sel_end) / [`anchor`](InputLine::anchor) are **byte
//! offsets** into it. Slicing a `String` at a non-`char` boundary panics, so
//! every step over `data` goes through [`text::next`] / [`text::prev`] (whole
//! grapheme clusters) — never `+1`/`-1`. `prev_word`/`next_word` scan ASCII
//! spaces, whose byte offsets are always `char` boundaries.
//!
//! [`first_pos`](InputLine::first_pos), by contrast, is a **display column**
//! (the horizontal scroll offset). The displayed position of an offset is the
//! display width of `data[..pos]`. For ASCII, byte == column, but for multi-byte
//! / wide content they diverge, so the two units are kept strictly distinct here.
//!
//! # Clipboard and command enabling
//!
//! Cut/copy write to the clipboard via [`Context`]; paste is requested through
//! the context and the pump reads the backend clipboard and inserts into the
//! field, clamped to `max_len` and replacing any selection. The field keeps
//! `cmCut`/`cmCopy` enabled while there is a selection and `cmPaste` enabled
//! while it is active and selected, pushing those enable/disable updates as
//! selection and focus change.
//!
//! # Validation
//!
//! A [`Validator`] gates the contents. Validators may also carry typed,
//! non-text values: [`value`](View::value)/[`set_value`](View::set_value)
//! consult the validator's [`transfer_get`](Validator::transfer_get) /
//! [`transfer_set`](Validator::transfer_set) hook before falling back to the
//! text itself, so a range validator round-trips an integer rather than a string.
//!
//! Validation runs on focus loss and returns a plain boolean. Deciding validity
//! is a read-only query (`valid(&self)`); a field that fails validation is *not*
//! automatically re-focused, since moving focus would require mutable access to
//! the event loop.
//!
//! # Turbo Vision heritage
//!
//! Ports `TInputLine` (`tinputli.cpp`/`dialogs.h`). Inheritance becomes the
//! `View` trait plus `ViewState` composition (D2); the byte/width/char limit
//! constants become the [`LimitMode`] enum (D1); the get/set/size data hooks
//! become the typed [`value`](View::value)/[`set_value`](View::set_value)
//! protocol over [`FieldValue`] (D10); and the explicit byte-offset vs.
//! display-column split is the two-unit coordinate model (D13).

use crate::capture::TrackMask;
use crate::command::Command;
use crate::data::FieldValue;
use crate::event::{Event, Key, MouseEvent};
use crate::keymap::{self, KeyStroke, Resolve};
use crate::text;
use crate::theme::Role;
use crate::validate::Validator;
use crate::view::{Context, DrawCtx, Options, Point, Rect, StateFlag, View, ViewState};

// ---------------------------------------------------------------------------
// LimitMode — what the `limit` constructor argument counts
// ---------------------------------------------------------------------------

/// How the `limit` constructor argument is interpreted — which of the three
/// internal caps it sets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LimitMode {
    /// The default: `limit` caps the byte length (`max_len = limit - 1`); the
    /// display width and grapheme count are unbounded.
    #[default]
    MaxBytes,
    /// `limit` caps the display width (`max_width = limit`); `max_len` is 255.
    MaxWidth,
    /// `limit` caps the grapheme count (`max_chars = limit`); `max_len` is 255.
    MaxChars,
}

// ---------------------------------------------------------------------------
// InputLine
// ---------------------------------------------------------------------------

/// A single-line text-entry field.
pub struct InputLine {
    /// View state (geometry, flags, cursor) — the composition target.
    pub state: ViewState,
    /// The UTF-8 text currently in the field.
    ///
    /// Direct reads are fine; to replace the whole content programmatically,
    /// prefer [`View::set_value`] so the cursor and scroll position are
    /// updated consistently. Slicing this string at a non-`char` boundary
    /// panics, so all internal traversals go through [`text::next`] /
    /// [`text::prev`] rather than raw byte arithmetic.
    pub data: String,
    /// Hard cap on `data.len()` in **bytes**.
    ///
    /// Set at construction time by the `limit` argument when [`LimitMode::MaxBytes`]
    /// is active (`max_len = limit - 1`); otherwise fixed at 255. Insertions
    /// that would push `data.len()` past this value are silently dropped.
    /// See also [`max_width`](InputLine::max_width) and
    /// [`max_chars`](InputLine::max_chars) for the display-width and
    /// grapheme-count caps that the other two limit modes control.
    pub max_len: i32,
    /// Hard cap on the **display width** of `data` (in terminal columns).
    ///
    /// Active only when the field is constructed with [`LimitMode::MaxWidth`],
    /// in which case `max_width = limit` and `max_len` is fixed at 255.
    /// Defaults to `i32::MAX` (effectively unbounded) for the other two modes.
    /// Wide characters such as CJK ideographs count as 2 columns each.
    pub max_width: i32,
    /// Hard cap on the **grapheme count** of `data`.
    ///
    /// Active only when the field is constructed with [`LimitMode::MaxChars`],
    /// in which case `max_chars = limit` and `max_len` is fixed at 255.
    /// Defaults to `i32::MAX` (effectively unbounded) for the other two modes.
    /// A grapheme cluster (e.g. a base letter plus combining diacritic) counts
    /// as one regardless of its byte or column size.
    pub max_chars: i32,
    /// Cursor position, a **byte** offset into `data`.
    pub cur_pos: i32,
    /// Horizontal scroll offset, a **display column** (see module docs: NOT a
    /// byte offset).
    pub first_pos: i32,
    /// Selection start, a **byte** offset into `data`.
    pub sel_start: i32,
    /// Selection end, a **byte** offset into `data`.
    pub sel_end: i32,
    /// The fixed end of a keyboard/mouse block extension, a **byte** offset.
    pub anchor: i32,
    /// The optional input validator, or `None` when the field is unconstrained.
    ///
    /// A [`Validator`] can filter individual keystrokes (`is_valid_input`),
    /// check the whole value on focus loss (`validate`), and carry typed
    /// non-text values via the `transfer_get`/`transfer_set` hooks so that
    /// [`View::value`]/[`View::set_value`] round-trip an integer rather than a
    /// string.
    ///
    /// You can read the validator (e.g. to call `is_status_ok`), but to
    /// swap it out after construction use [`set_validator`](InputLine::set_validator)
    /// rather than assigning to this field directly — that keeps the
    /// ownership contract explicit.
    pub validator: Option<Box<dyn Validator>>,
    // -- validator save-state (oldData/oldCurPos/…) ------------------------
    old_data: String,
    old_cur_pos: i32,
    old_first_pos: i32,
    old_sel_start: i32,
    old_sel_end: i32,
    // -- mouse hold-tracking --------------------------------------------------
    /// Absolute screen position of input-local `(0, 0)`, cached each `draw`
    /// so the mouse-tracking capture can convert absolute mouse coords to
    /// field-local (the `Button::abs_origin` pattern).
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
    /// Build a field from `bounds`, a `limit`, an optional `validator`, and a
    /// [`LimitMode`] that decides what `limit` counts.
    ///
    /// The three internal caps are derived from the mode: `max_len` (byte cap)
    /// is `limit - 1` clamped to a non-negative range when the mode is byte-based,
    /// else 255; `max_width` is `limit` only in width mode, else unbounded;
    /// `max_chars` is `limit` only in char mode, else unbounded. The field is
    /// selectable, takes focus on first click, shows a cursor, and starts empty.
    ///
    /// Pass `None` for `validator` to create an unconstrained field. To attach
    /// or replace the validator after construction, use
    /// [`set_validator`](InputLine::set_validator). For the common case of a
    /// byte-limited field with no validator, prefer [`with_limit`](InputLine::with_limit).
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

    /// Convenience constructor with no validator and the default byte-limit mode.
    pub fn with_limit(bounds: Rect, limit: i32) -> Self {
        Self::new(bounds, limit, None, LimitMode::MaxBytes)
    }

    /// Replace this field's validator after construction.
    ///
    /// Pass `Some(validator)` to attach a [`Validator`](crate::validate::Validator)
    /// that filters keystrokes and checks the field on focus-change/close, or
    /// `None` to remove any constraint. The previous validator (if any) is dropped.
    ///
    /// Most fields set their validator once via [`InputLine::new`]; use this when
    /// the constraint is only known later (e.g. it depends on another control's
    /// value gathered at dialog-open time).
    ///
    /// # Turbo Vision heritage
    ///
    /// Mirrors `TInputLine::setValidator`, which disposed the old validator and
    /// assigned the new one.
    pub fn set_validator(&mut self, validator: Option<Box<dyn crate::validate::Validator>>) {
        self.validator = validator;
    }

    // -- geometry helpers (byte ↔ column) ----------------------------------

    /// The display column of the prefix `data[..pos]` (`pos` is a byte offset).
    /// The screen-column ↔ byte bridge.
    fn displayed_pos(&self, pos: i32) -> i32 {
        text::width(&self.data[..pos as usize]) as i32
    }

    /// Whether the field can scroll by `delta` (`delta < 0` left, `> 0` right).
    /// Right uses display-width arithmetic.
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
    /// cursor reset can read it before redraw, splitting the cursor placement
    /// out of `draw`.
    fn sync_cursor(&mut self) {
        let x = self.displayed_pos(self.cur_pos) - self.first_pos + 1;
        self.state.set_cursor(x, 0);
    }

    // -- selection / deletion (byte offsets) -------------------------------

    /// Remove the selected range `data[sel_start..sel_end]`, leaving the cursor
    /// at `sel_start`.
    fn delete_select(&mut self) {
        if self.sel_start < self.sel_end {
            self.data
                .replace_range(self.sel_start as usize..self.sel_end as usize, "");
            self.cur_pos = self.sel_start;
        }
    }

    /// Select the grapheme under the cursor (one [`text::next`] step) and delete it.
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

    /// Order `sel_start`/`sel_end` around the `anchor` after a block extension.
    fn adjust_select_block(&mut self) {
        if self.cur_pos < self.anchor {
            self.sel_start = self.cur_pos;
            self.sel_end = self.anchor;
        } else {
            self.sel_start = self.anchor;
            self.sel_end = self.cur_pos;
        }
    }

    /// Select the whole field (`enable = true`) or deselect it (`enable =
    /// false`), and optionally scroll the cursor into view (`scroll = true`).
    ///
    /// When `enable` is true the cursor moves to the end of `data` and the
    /// selection covers `[0, data.len()]`. When false both the cursor and
    /// the selection are collapsed to position 0.
    ///
    /// Pass `scroll = true` when the field is gaining focus (so the end of a
    /// long value scrolls into view). Pass `scroll = false` from [`set_state`]
    /// where the viewport position should be left as-is.
    ///
    /// This method does **not** trigger a redraw on its own — the event loop
    /// redraws the whole tree. It does update the screen cursor via
    /// [`sync_cursor`](InputLine::sync_cursor). Callers that also need to
    /// refresh cut/copy/paste command graying call `update_commands` separately.
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

    /// True only when this field is both active and selected. Command
    /// enable/disable updates are pushed only while both hold.
    fn can_update_commands(&self) -> bool {
        self.state.state.active && self.state.state.selected
    }

    /// Push enable/disable updates for cut/copy (enabled only while a selection
    /// exists) and paste (always enabled while this field is active+selected).
    /// Only called when [`can_update_commands`](Self::can_update_commands) holds.
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

    /// Snapshot the field for the validator's restore-on-reject.
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

    /// Undo to the last [`save_state`](Self::save_state).
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

    /// Run the validator's input check over the current `data`; on reject,
    /// restore the snapshot and report `false`; on accept, clamp to the byte cap
    /// and pull the cursor back to the new end if it sat past the old one.
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
            // Note: a future mutating validator that SHRINKS data could leave
            // cur_pos past end-of-string / mid-grapheme. No bundled validator
            // shrinks, so this is not handled; an auto-fill validator would need
            // to re-clamp cur_pos to a char boundary <= data.len() to avoid a
            // slicing panic.
            if self.cur_pos >= old_len && new_len > old_len {
                self.cur_pos = new_len;
            }
            true
        }
    }

    // -- clipboard paste ------------------------------------------------------

    /// Insert `text` from the clipboard at the current cursor position,
    /// replacing any active selection and clamping the result to `max_len`.
    /// Called by the pump's `Deferred::InputLinePaste` apply arm once the backend
    /// has supplied the clipboard text: insert the pasted bytes at the cursor,
    /// replacing the selection, clamped so the total byte length does not exceed
    /// the byte cap. Tabs/newlines are replaced with spaces. After insertion the
    /// cursor sits at the end of the pasted text and the selection is cleared.
    pub fn paste_text(&mut self, text: &str) {
        self.save_state();
        // Replace the current selection before inserting.
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

    /// The byte offset under the mouse (view-local position already applied by
    /// the group).
    fn mouse_pos(&self, m: &MouseEvent) -> i32 {
        let mx = m.position.x.max(1);
        let pos = (mx + self.first_pos - 1).max(0);
        // scroll(columns) → byte length of that many columns into data.
        text::scroll(&self.data, pos, false).0 as i32
    }

    /// The auto-scroll direction for a mouse at the edge. Used by the mouse-down
    /// arm and the hold-repeat (`MouseAuto`) arm in both the edge-scroll and
    /// drag-select branches.
    fn mouse_delta(&self, m: &MouseEvent) -> i32 {
        if m.position.x <= 0 {
            -1
        } else if m.position.x >= self.state.size.x - 1 {
            1
        } else {
            0
        }
    }

    // -- clipboard helpers (extracted from the command arm) --------------

    /// Cut: copy the current selection to the clipboard, then delete it. The
    /// clipboard operation is guarded by the selection test; the cut command is
    /// always consumed regardless of whether a selection existed, which the
    /// callers handle.
    fn do_cut(&mut self, ctx: &mut Context) {
        if self.sel_start < self.sel_end {
            let sel = self.data[self.sel_start as usize..self.sel_end as usize].to_string();
            ctx.set_clipboard(sel);
            self.save_state();
            self.delete_select();
            self.check_valid(true);
            self.sel_start = 0;
            self.sel_end = 0;
            self.sync_cursor();
        }
    }

    /// Copy the current selection to the clipboard, keeping it (the
    /// `Command::COPY` body).
    fn do_copy(&mut self, ctx: &mut Context) {
        if self.sel_start < self.sel_end {
            let sel = self.data[self.sel_start as usize..self.sel_end as usize].to_string();
            ctx.set_clipboard(sel);
        }
    }

    /// Request an async paste via the broker (the `Command::PASTE` body).
    fn do_paste(&mut self, ctx: &mut Context) {
        if let Some(id) = self.state.id() {
            ctx.request_input_line_paste(id);
        }
    }

    // -- keymap dispatch ----------------------------------------------------

    /// Apply a resolved editor command within the single-line repertoire.
    /// Returns `true` if handled; `false` means "not ours — let it bubble".
    fn apply_input_command(&mut self, cmd: Command, ctx: &mut Context) -> bool {
        match cmd {
            Command::CHAR_LEFT => {
                self.cur_pos -= text::prev(&self.data, self.cur_pos as usize) as i32
            }
            Command::CHAR_RIGHT => {
                let cp = self.cur_pos as usize;
                let step = text::next(&self.data[cp..]).map(|(l, _)| l).unwrap_or(0);
                self.cur_pos += step as i32;
            }
            Command::WORD_LEFT => self.cur_pos = prev_word(&self.data, self.cur_pos),
            Command::WORD_RIGHT => self.cur_pos = next_word(&self.data, self.cur_pos),
            Command::LINE_START => self.cur_pos = 0,
            Command::LINE_END => self.cur_pos = self.data.len() as i32,
            Command::BACK_SPACE => {
                if self.sel_start == self.sel_end {
                    self.sel_start =
                        self.cur_pos - text::prev(&self.data, self.cur_pos as usize) as i32;
                    self.sel_end = self.cur_pos;
                }
                self.delete_select();
                self.check_valid(true);
            }
            Command::DEL_WORD_LEFT => {
                // kbCtrlBack / kbAltBack — delete the previous word.
                if self.sel_start == self.sel_end {
                    self.sel_start = prev_word(&self.data, self.cur_pos);
                    self.sel_end = self.cur_pos;
                }
                self.delete_select();
                self.check_valid(true);
            }
            Command::DEL_CHAR => {
                if self.sel_start == self.sel_end {
                    self.delete_current();
                } else {
                    self.delete_select();
                }
                self.check_valid(true);
            }
            Command::DEL_WORD => {
                // kbCtrlDel — delete to the next word.
                if self.sel_start == self.sel_end {
                    self.sel_start = self.cur_pos;
                    self.sel_end = next_word(&self.data, self.cur_pos);
                }
                self.delete_select();
                self.check_valid(true);
            }
            Command::INS_MODE => {
                // C++ setState(sfCursorIns, !(state & sfCursorIns)). sfCursorIns
                // is NOT a propagating StateFlag, so flip it directly on ViewState.
                self.state.state.cursor_ins = !self.state.state.cursor_ins;
            }
            Command::DEL_LINE => {
                // Ctrl-Y clears the field (the C++ default's else-if).
                self.data.clear();
                self.cur_pos = 0;
            }
            Command::SELECT_ALL => {
                self.sel_start = 0;
                self.sel_end = self.data.len() as i32;
                self.cur_pos = self.data.len() as i32;
            }
            Command::CUT => {
                self.do_cut(ctx);
                return true;
            }
            Command::COPY => {
                self.do_copy(ctx);
                return true;
            }
            Command::PASTE => {
                self.do_paste(ctx);
                return true;
            }
            _ => return false, // outside the single-line repertoire → bubble
        }
        true
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
    /// downcast and call [`paste_text`](InputLine::paste_text).
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    /// Render the field: background fill, scrolled text, scroll arrows at the
    /// edges when the content overflows, and the selection highlight.
    ///
    /// Colors come from three theme roles: [`Role::InputNormal`] for the
    /// background and unselected text (both focused and unfocused use the same
    /// normal style), [`Role::InputArrow`] for the `◄`/`►` overflow indicators,
    /// and [`Role::InputSelected`] for highlighted text. Because tvision-rs has
    /// no attribute-only paint, the selected substring is **redrawn** (not just
    /// re-attributed) in the selected style at the correct scroll offset — the
    /// visible glyphs of the scrolled window, not the raw head of the selection.
    ///
    /// The screen cursor position is **not** set here; it is computed separately
    /// by [`sync_cursor`](InputLine::sync_cursor) before each redraw so the event
    /// loop can place the cursor without going through the draw path.
    fn draw(&mut self, ctx: &mut DrawCtx) {
        // Cache absolute origin for the mouse-tracking capture: the
        // MouseTrackCapture converts absolute mouse coords to field-local via
        // this value, mirroring the Button `abs_origin` pattern.
        self.abs_origin = ctx.origin();
        let size = self.state.size;
        // Focused and unfocused both use the normal input role.
        let color = ctx.style(Role::InputNormal);
        let arrow = ctx.style(Role::InputArrow);
        let selected = ctx.style(Role::InputSelected);
        let left_arrow = ctx.glyphs().input_left_arrow;
        let right_arrow = ctx.glyphs().input_right_arrow;

        // Fill the whole row with the background color.
        ctx.fill(Rect::new(0, 0, size.x, 1), ' ', color);
        // Scrolled text from column 1, offset by first_pos.
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

        // Selection highlight. There is no attr-only paint, so we REDRAW the
        // selected substring in the selected style at its screen column —
        // byte-identical output.
        if self.state.state.selected && self.sel_start < self.sel_end {
            // `l`/`r` are the display columns of the selection ends relative to
            // the scroll window; the highlight covers view columns [l+1 .. r+1).
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

    /// Process keyboard, mouse, and command events while the field is selected.
    ///
    /// **Keyboard:** printable characters are inserted at the cursor (replacing
    /// any active selection, with validator filtering and all three caps checked).
    /// Navigation keys (arrows, Home, End, Ctrl-arrows, Ins, Delete, Backspace,
    /// Ctrl-Y) are dispatched through the global keymap and applied by
    /// [`apply_input_command`](InputLine::apply_input_command). Shift-held
    /// movement extends the selection block. Unhandled keys (Tab, Enter, Escape,
    /// modified characters outside the repertoire) are left live so the enclosing
    /// dialog can route them.
    ///
    /// **Mouse:** a single click positions the cursor; a double-click selects
    /// all. Click-and-drag or a click on a scroll-arrow arms a press-and-hold
    /// tracking capture that continues updating selection or scroll position on
    /// each [`Event::MouseAuto`] / [`Event::MouseMove`] tick until mouse-up.
    ///
    /// **Commands:** [`Command::CUT`], [`Command::COPY`], and [`Command::PASTE`]
    /// are consumed here; other commands fall through.
    ///
    /// After each handled event, cut/copy/paste command graying is refreshed via
    /// [`update_commands`](InputLine::update_commands) (only while both active
    /// and selected).
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        // Mouse-down auto-select is the group's job; nothing to do while unselected.
        if !self.state.state.selected {
            return;
        }

        match ev {
            // -- Mouse positioning + hold-tracking ------------------------------
            //
            // The two hold loops (edge auto-scroll, drag-select) both start from
            // the same mouse-down — they become the first iteration here, then
            // arm the capture for subsequent ticks.
            Event::MouseDown(m) => {
                let m = *m;
                let delta = self.mouse_delta(&m);
                if self.can_scroll(delta) {
                    // Edge auto-scroll: first iteration steps first_pos by delta.
                    self.first_pos += delta;
                    // Arm auto-only repeat.
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
                    // Arm move+auto tracking.
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
            // The edge-scroll track is auto-only, so a move during an edge track
            // must FALL THROUGH unconsumed (same split-loop structure as the
            // scrollbar arms).
            Event::MouseMove(m) if self.tracking && self.tracking_drag => {
                let m = *m;
                // Drag-select: move the cursor to the mouse and re-order the block.
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
                let shift = ke.modifiers.shift;
                let stroke = KeyStroke::from_event(*ke);
                let cmd = match keymap::resolve_global(None, stroke) {
                    Resolve::Command(c) => Some(c),
                    // A `Prefix` (input fields use no 2-key chords) or `None` both
                    // fall through to the printable/bubble path.
                    _ => None,
                };

                // Shift-extend applies only to movement commands with Shift held.
                let is_move = matches!(cmd, Some(c) if
                    c == Command::CHAR_LEFT
                        || c == Command::CHAR_RIGHT
                        || c == Command::WORD_LEFT
                        || c == Command::WORD_RIGHT
                        || c == Command::LINE_START
                        || c == Command::LINE_END);
                let extend_block = is_move && shift;
                if extend_block {
                    if self.cur_pos == self.sel_end {
                        self.anchor = self.sel_start;
                    } else if self.sel_start == self.sel_end {
                        self.anchor = self.cur_pos;
                    } else {
                        self.anchor = self.sel_end;
                    }
                }

                // The post-dispatch tail clears the selection on any non-extend
                // movement. These commands must bypass that reset:
                //  * SELECT_ALL — its effect IS the selection.
                //  * PASTE — `do_paste` only QUEUES a deferred paste; the tail runs
                //    synchronously first, so zeroing the selection here would make
                //    the deferred `paste_text` find nothing to replace (regression).
                //  * COPY — keep the visible selection after copying (faithful to
                //    the old Command-arm COPY).
                //  * CUT — `do_cut` already zeroed the selection; listed for
                //    consistency (harmless either way).
                let keep_selection = matches!(
                    cmd,
                    Some(Command::SELECT_ALL | Command::CUT | Command::COPY | Command::PASTE)
                );

                let mut handled = true;
                match cmd {
                    Some(c) => handled = self.apply_input_command(c, ctx),
                    None => {
                        // Printable insertion: only a plain Char with no ctrl/alt.
                        match ke.key {
                            Key::Char(c) if !ke.modifiers.ctrl && !ke.modifiers.alt => {
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
                    }
                }

                if !handled {
                    // Unhandled key (Tab, Enter, a modified char, …): leave the
                    // event LIVE and uncleared so the group/dialog still sees it
                    // (the C++ `default: … else return;`).
                    return;
                }

                if extend_block {
                    self.adjust_select_block();
                } else if !keep_selection {
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

            // -- evCommand clipboard block --------------------------------------
            // Only reached when selected (the outer guard returns otherwise).
            Event::Command(cmd) => {
                // Only CUT/COPY/PASTE are handled via the command channel — the
                // input line must NOT newly react to movement/select commands that
                // arrive here (the keymap-driven repertoire applies to KeyDown).
                // The command is always consumed regardless of whether a
                // selection exists; the clipboard operation is guarded inside each.
                match *cmd {
                    Command::CUT => {
                        self.do_cut(ctx);
                        ev.clear();
                    }
                    Command::COPY => {
                        self.do_copy(ctx);
                        ev.clear();
                    }
                    Command::PASTE => {
                        self.do_paste(ctx);
                        ev.clear();
                    }
                    _ => {}
                }
            }

            _ => {}
        }

        // Command graying: refresh the cut/copy/paste enable state. The
        // early-return for unhandled KeyDown already exited above; this covers
        // mouse, keyboard (handled), and command events.
        if self.can_update_commands() {
            self.update_commands(ctx);
        }
    }

    /// Apply a state-flag change and react to focus transitions.
    ///
    /// Flips the requested flag on [`ViewState`], then:
    ///
    /// - On [`StateFlag::Selected`] (or [`StateFlag::Active`] while the field is
    ///   already selected): calls [`select_all(enable, false)`](InputLine::select_all)
    ///   so the field is fully selected when it gains focus and deselected when it
    ///   loses it.
    /// - When the active-and-selected condition changes (e.g. the field gains or
    ///   loses focus): pushes cut/copy/paste command enable/disable updates via
    ///   [`update_commands`](InputLine::update_commands). This is what grays the
    ///   Edit menu items when focus leaves the field.
    fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
        // Command graying: sample the enable condition BEFORE the flag flip so
        // we can detect the transition.
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
        // Command graying: if the enable condition changed, push the
        // enable/disable updates.
        let update_after = self.can_update_commands();
        if update_before != update_after {
            self.update_commands(ctx);
        }
    }

    /// With a validator: the validate command reports the validator's status;
    /// any other non-cancel command runs the validator and fails if invalid.
    /// Without a validator: always valid.
    fn valid(&mut self, cmd: Command, ctx: &mut Context) -> bool {
        if let Some(validator) = &self.validator {
            if cmd == Command::VALID {
                return validator.is_status_ok();
            } else if cmd != Command::CANCEL && !validator.validate(&self.data, ctx) {
                // validator.validate pops the validator's error box (via
                // ctx.request_message_box) on the way to returning false, then we
                // refocus the offending field.
                if let Some(id) = self.state.id() {
                    ctx.request_focus(id);
                }
                return false;
            }
        }
        true
    }

    /// Return the field's current value as a [`FieldValue`].
    ///
    /// If the attached validator supports typed transfer (`transfer_get`
    /// returns `Some`), that typed value is returned — for example, a
    /// [`RangeValidator`](crate::validate::RangeValidator) with transfer
    /// enabled yields `FieldValue::Int(n)` rather than `FieldValue::Text`.
    /// Otherwise (no validator, or transfer disabled) the raw field text is
    /// returned as `FieldValue::Text`.
    ///
    /// Call this from a dialog's gather pass (or directly) to read the field's
    /// value in a type-safe way. For a plain text field with no validator the
    /// result is always `Some(FieldValue::Text(…))`.
    fn value(&self) -> Option<FieldValue> {
        // A transfer-enabled validator (a range validator, say) produces a typed
        // value; otherwise the result is the field's text.
        if let Some(v) = self
            .validator
            .as_ref()
            .and_then(|val| val.transfer_get(&self.data))
        {
            return Some(v);
        }
        Some(FieldValue::Text(self.data.clone()))
    }

    /// Load a value into the field, replacing the current contents, and select all.
    ///
    /// If the attached validator supports typed transfer (`transfer_set` returns
    /// `Some`), the validator formats the value into the display text (e.g.
    /// `FieldValue::Int(42)` becomes `"42"`) and [`select_all(true, true)`](InputLine::select_all)
    /// is called. Otherwise, a `FieldValue::Text` value is loaded directly,
    /// **truncated to `max_len` bytes** at the nearest `char` boundary if it
    /// is too long, and the field is selected. A `FieldValue::Int` into a field
    /// with no transfer-enabled validator is silently ignored (type mismatch).
    ///
    /// Use this from a dialog's scatter pass or any time you need to populate
    /// the field programmatically and want the cursor/scroll state reset.
    fn set_value(&mut self, v: FieldValue) {
        // A transfer-enabled validator formats the typed value into the field
        // text; otherwise the Text path is used. Select-all runs either way.
        if let Some(text) = self.validator.as_ref().and_then(|val| val.transfer_set(&v)) {
            self.data = text;
            self.select_all(true, true);
            return;
        }
        // When transfer is disabled and `v` is `Int` (not `Text`), the body
        // below is skipped entirely — no data change, no `select_all`. An `Int`
        // into a non-transfer field is a type mismatch the typed model rightly
        // drops; this is intentional, not an oversight.
        #[allow(irrefutable_let_patterns)]
        if let FieldValue::Text(s) = v {
            // Truncate to maxLen.
            let limit = self.max_len as usize;
            self.data = if s.len() <= limit {
                s
            } else {
                let mut cut = limit;
                while cut > 0 && !s.is_char_boundary(cut) {
                    cut -= 1;
                }
                s[..cut].to_string()
            };
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
    /// (`sel_start = 0`, off the visible left edge). Because tvision-rs REDRAWS the
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
    /// regardless of the validator outcome (`tinputli.cpp:459`, tvision-rs lines
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

    // -- value / set_value ----------------------------------------------------

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

    // -- value / set_value with a transfer-enabled validator ------------------

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

    // -- drag-select tracking (evMouseMove | evMouseAuto) ---------------------
    //
    // These tests drive the tracking arms directly (as the pump's Deferred::MouseTrack
    // apply does) with field-local positions. The capture itself is unit-tested
    // in capture::tests; here we verify the widget's loop body is correct.

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

    // -- edge auto-scroll tracking (evMouseAuto only) -------------------------

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

    // -- updateCommands / canUpdateCommands graying ---------------------------

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

    // -- clipboard cut/copy/paste ---------------------------------------------

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

    /// Cut without a selection is a no-op data-wise but the event IS consumed.
    /// Cut/copy always consume the event regardless of selection; the clipboard
    /// operation is only performed when a selection exists.
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
        assert!(ev.is_nothing(), "cut always consumes the event");
        assert_eq!(il.data, data_before, "no data change without selection");
        assert!(
            !deferred
                .iter()
                .any(|d| matches!(d, Deferred::SetClipboard(_))),
            "no SetClipboard when no selection"
        );
    }

    // -- Phase 3: global-keymap dispatch ------------------------------------

    /// Regression: under the default (WordStar) preset the field's editing keys
    /// still work — plain Backspace deletes, Home moves to start.
    #[test]
    fn keymap_default_backspace_and_nav_still_work() {
        let mut il = field(12, "abc"); // cursor at end (3)
        let mut ev = key(Key::Backspace);
        send_key(&mut il, &mut ev);
        assert_eq!(il.data, "ab");
        let mut ev = key(Key::Home);
        send_key(&mut il, &mut ev);
        assert_eq!(il.cur_pos, 0);
    }

    /// Under the CUA preset, Ctrl-A selects all and Ctrl-C copies the selection
    /// to the clipboard (via the SetClipboard deferred, like the existing
    /// copy/cut tests).
    #[test]
    fn cua_ctrl_c_copies_in_input_line() {
        let _g = crate::keymap::GlobalKeymapGuard::new(crate::keymap::Keymap::cua());
        let (mut il, _id) = field_with_id(12, "hello");
        il.state.state.active = true;

        // Ctrl-A → SELECT_ALL.
        let mut ev = ctrl_key(Key::Char('a'));
        send_key(&mut il, &mut ev);
        assert_eq!(il.sel_start, 0);
        assert_eq!(il.sel_end, 5);

        // Ctrl-C → COPY: the selection text lands on the clipboard.
        let mut ev = ctrl_key(Key::Char('c'));
        let (_, deferred, ()) = with_ctx_d(|ctx| il.handle_event(&mut ev, ctx));
        let clipboard_text = deferred.iter().find_map(|d| {
            if let Deferred::SetClipboard(s) = d {
                Some(s.as_str())
            } else {
                None
            }
        });
        assert_eq!(clipboard_text, Some("hello"));
        // COPY must LEAVE the selection intact (the KeyDown tail's
        // sel-reset is bypassed for clipboard commands).
        assert_eq!(il.sel_start, 0, "selection survives COPY");
        assert_eq!(il.sel_end, 5, "selection survives COPY");

        // PASTE over the live selection must replace it: do_paste queues a
        // deferred InputLinePaste, and the selection must still be present for
        // the pump's paste_text/delete_select to act on (regression guard — the
        // tail must not have zeroed it).
        let mut ev = ctrl_key(Key::Char('v'));
        let (_, deferred, ()) = with_ctx_d(|ctx| il.handle_event(&mut ev, ctx));
        assert!(
            deferred
                .iter()
                .any(|d| matches!(d, Deferred::InputLinePaste(_))),
            "Ctrl-V queues a deferred paste"
        );
        assert_eq!(
            il.sel_end, 5,
            "selection still live when the deferred paste runs"
        );
    }

    /// Under every preset, plain Enter resolves to NEW_LINE — outside the
    /// single-line repertoire — so it must remain LIVE/unhandled and bubble to
    /// the dialog (default-button / focus / cancel routing).
    #[test]
    fn enter_tab_esc_bubble_under_every_preset() {
        for preset in [
            crate::keymap::Keymap::word_star(),
            crate::keymap::Keymap::cua(),
            crate::keymap::Keymap::emacs(),
        ] {
            let _g = crate::keymap::GlobalKeymapGuard::new(preset);
            let mut il = field(12, "x");
            let mut ev = key(Key::Enter);
            send_key(&mut il, &mut ev);
            assert!(
                matches!(ev, Event::KeyDown(_)),
                "Enter must remain live (bubble) under the active preset"
            );
        }
    }

    #[test]
    fn set_validator_replaces_constructor_validator() {
        use crate::validate::FilterValidator;
        // Build with no validator, then attach one that only allows digits.
        let mut line = InputLine::new(Rect::new(0, 0, 10, 1), 9, None, LimitMode::default());
        assert!(line.validator.is_none());

        line.set_validator(Some(Box::new(FilterValidator::new("0123456789"))));
        assert!(line.validator.is_some());
        // The freshly-attached validator rejects a non-digit keystroke.
        let mut non_digit = String::from("a");
        let mut digit = String::from("7");
        assert!(
            !line
                .validator
                .as_ref()
                .unwrap()
                .is_valid_input(&mut non_digit, false)
        );
        assert!(
            line.validator
                .as_ref()
                .unwrap()
                .is_valid_input(&mut digit, false)
        );

        // Clearing it removes the constraint.
        line.set_validator(None);
        assert!(line.validator.is_none());
    }
}
