//! A gap-buffer text editor: a leaf [`View`] holding a single `Vec<u8>` split by
//! a movable gap, with a cursor, a selection, single-level undo, and a viewport
//! (`delta`) onto a content extent (`limit`). It references two sibling scroll
//! bars and an indicator on the window frame, runs Find / Replace dialogs, and
//! cuts/copies/pastes through a shared internal clipboard editor.
//!
//! # The context-threading split (the central seam)
//!
//! Redraw work needs a [`Context`], but a [`View`] has none at construction, so
//! requesting a redraw is **split from performing it**:
//!
//! * The **core editing methods take no `Context`** — they mutate logical state
//!   and only OR bits into [`update_flags`](Editor::update_flags). [`update`]
//!   only records what changed; it never repaints inline.
//! * `&mut Context` is threaded **only** into [`do_update`], [`unlock`],
//!   `handle_event`, `set_state`, and the public ctx-taking entries
//!   (`apply_scroll_delta`, `insert_text`). `unlock` flushes when the lock count
//!   hits 0; `handle_event` flushes at the end for the arms that ran unlocked.
//! * `change_bounds` is geometry-only plus a `delta` clamp plus a view-update
//!   request — scroll-bar params republish on the next flush.
//!
//! This makes the entire gap-buffer + edit core context-free and unit-testable in
//! isolation, where the real oracle (logical buffer state) lives.
//!
//! # Siblings by `ViewId`
//!
//! The editor holds [`h_scroll_bar`](Editor::h_scroll_bar)/`v_scroll_bar`/
//! `indicator` as [`Option<ViewId>`], never pointers. Every cross-view read/write
//! is routed through the event loop: read the bar values into `delta`, publish
//! range/value, set the indicator value, show/hide. The editor is its own
//! concrete downcast target (it is *not* a `Scroller`).
//!
//! # Grapheme text
//!
//! The character-stepping helpers read logical bytes across the gap into a small
//! buffer, `str`-decode the contiguous slice, and step whole grapheme clusters
//! via [`text::next`]/[`text::prev`]. There is no fixed-size character stack
//! buffer.
//!
//! # Mouse hold-tracking
//!
//! The two mouse-down hold loops (drag-select and pan) become tracked event arms:
//! the `MouseDown` arm runs the first loop iteration and arms the capture; the
//! move/auto/wheel arms are the loop bodies; `MouseUp` clears. Which loop is in
//! flight lives in [`EditorTrack`] (`Select` carries the live select mode; `Pan`
//! carries the last mouse position). A wheel during a drag-select forwards to both scroll
//! bars and self-posts a delta-sync, so the new offset lands on the next pump.
//! Outside a hold, a wheel falls through unconsumed (the editor's event mask
//! excludes the wheel).
//!
//! # Turbo Vision heritage
//!
//! Ports `TEditor` (`teditor1.cpp`, `teditor2.cpp`, `edits.cpp`). Up-pointers to
//! the scroll bars and indicator become [`ViewId`] handles brokered by the event
//! loop (deviation D3); raw byte indices become grapheme-aware stepping (deviation
//! D13); color lookups become [`Role`]s; serialization is dropped.

use crate::keymap::{self, KeyStroke, Resolve};
use crate::theme::Role;
use crate::view::{
    Context, DrawCtx, GrowMode, Options, Point, Rect, StateFlag, View, ViewId, ViewState,
};
use crate::widgets::{Indicator, ScrollBar};

// ---------------------------------------------------------------------------
// module-private flag constants (kept off Command — these are bit words)
// ---------------------------------------------------------------------------

/// Redraw the indicator/scrollbars/cursor only (no text repaint).
const UF_UPDATE: u8 = 0x01;
/// Repaint just the current line.
const UF_LINE: u8 = 0x02;
/// Repaint the whole view.
const UF_VIEW: u8 = 0x04;

/// Extend the current selection to the new cursor position.
const SM_EXTEND: u8 = 0x01;
/// Word-granular selection (double-click).
const SM_DOUBLE: u8 = 0x02;
/// Line-granular selection (triple-click).
const SM_TRIPLE: u8 = 0x04;

// The search-option flags. `pub(crate)` (the `editor` module is private and only
// `Editor`/`Encoding`/`LineEnding` are re-exported; `search()` takes a plain
// `opts: u16`).
/// Match case exactly.
pub(crate) const EF_CASE_SENSITIVE: u16 = 0x0001;
/// Reject matches that fall inside a larger word.
pub(crate) const EF_WHOLE_WORDS_ONLY: u16 = 0x0002;
// The flags that drive the find/replace dialogs.
/// Prompt before each substitution.
pub(crate) const EF_PROMPT_ON_REPLACE: u16 = 0x0004;
/// Replace every match.
pub(crate) const EF_REPLACE_ALL: u16 = 0x0008;
/// The operation is a replace, not a find.
pub(crate) const EF_DO_REPLACE: u16 = 0x0010;
/// Rename the existing file to `<name>~` before saving.
pub(crate) const EF_BACKUP_FILES: u16 = 0x0100;

/// The fixed content width (the maximum line length).
const MAX_LINE_LENGTH: i32 = 256;

/// Which mouse-button hold is in flight, mirroring the scroll-bar tracking
/// discipline. `None` in [`Editor::track`] means no hold is active; the tracked
/// `MouseMove`/`MouseAuto`/wheel/`MouseUp` arms are guarded on it, so a stray
/// event falls through unconsumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorTrack {
    /// A primary-button drag that extends the selection. `select_mode` is seeded
    /// by the press (`SM_EXTEND`/`SM_DOUBLE`/`SM_TRIPLE`) and gains `SM_EXTEND`
    /// after every move, so a word- or line-granular drag (double/triple-click)
    /// keeps growing in that granularity.
    Select {
        /// The live select-mode byte (`SM_*` bits).
        select_mode: u8,
    },
    /// A middle-button pan: each tick scrolls by the mouse delta and never
    /// touches the cursor or selection.
    Pan {
        /// The previous tick's view-local mouse position.
        last: Point,
    },
}

/// Sentinel returned by the buffer scan helpers to mean "not found".
const SEARCH_FAILED: usize = usize::MAX;

// ---------------------------------------------------------------------------
// Line ending / encoding enums
// ---------------------------------------------------------------------------

/// How line breaks are stored when text is inserted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    /// `"\r\n"`.
    CrLf,
    /// `"\n"`.
    Lf,
    /// `"\r"`.
    Cr,
}

impl LineEnding {
    /// The byte sequence this line ending writes.
    fn bytes(self) -> &'static [u8] {
        match self {
            LineEnding::Lf => b"\n",
            LineEnding::Cr => b"\r",
            LineEnding::CrLf => b"\r\n",
        }
    }
}

/// How multibyte characters are stepped over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    /// Width-aware (grapheme) stepping.
    Default,
    /// Every byte is one column.
    SingleByte,
}

/// The line ending used for newly inserted breaks: [`LineEnding::Lf`], the
/// modern-host default.
const DEFAULT_LINE_ENDING: LineEnding = LineEnding::Lf;

// ---------------------------------------------------------------------------
// getCharType / isWordBoundary / isWordChar (teditor2.cpp)
// ---------------------------------------------------------------------------

/// Classify a byte for word-boundary detection (whitespace / line break /
/// punctuation / word character).
fn get_char_type(ch: u8) -> u8 {
    match ch {
        b'\t' | b' ' | 0 => 0,
        b'\n' | b'\r' => 1,
        b'!'..=b'/' | b':'..=b'@' | b'['..=b'`' | b'{'..=b'~' => 2,
        _ => 3,
    }
}

/// True when `a` and `b` fall in different character classes.
fn is_word_boundary(a: u8, b: u8) -> bool {
    get_char_type(a) != get_char_type(b)
}

/// True unless `ch` is whitespace or punctuation — i.e. it is part of a word.
fn is_word_char(ch: u8) -> bool {
    !matches!(
        ch,
        b' ' | b'!'..=b'/' | b':'..=b'@' | b'['..=b'`' | b'{'..=b'~' | 0
    )
}

/// Number of line breaks in `buf`, counting `\r\n` as one.
fn count_lines(buf: &[u8]) -> i32 {
    let mut lines = 0;
    let mut i = 0;
    while i < buf.len() {
        if buf[i] == b'\r' {
            lines += 1;
            if i + 1 < buf.len() && buf[i + 1] == b'\n' {
                i += 1;
            }
        } else if buf[i] == b'\n' {
            lines += 1;
        }
        i += 1;
    }
    lines
}

/// Case-sensitive substring search. Returns the byte offset of the first match,
/// or [`SEARCH_FAILED`].
fn scan(block: &[u8], needle: &[u8]) -> usize {
    let len = needle.len();
    if len == 0 {
        return SEARCH_FAILED;
    }
    let size = block.len();
    let mut i = 0;
    while i < size {
        let mut j = i;
        let mut k = 0;
        while j < size && block[j] == needle[k] {
            j += 1;
            k += 1;
            if k == len {
                return i;
            }
        }
        i += 1;
    }
    SEARCH_FAILED
}

/// Case-insensitive substring search.
fn i_scan(block: &[u8], needle: &[u8]) -> usize {
    let len = needle.len();
    if len == 0 {
        return SEARCH_FAILED;
    }
    let size = block.len();
    let mut i = 0;
    while i < size {
        let mut j = i;
        let mut k = 0;
        while j < size && block[j].eq_ignore_ascii_case(&needle[k]) {
            j += 1;
            k += 1;
            if k == len {
                return i;
            }
        }
        i += 1;
    }
    SEARCH_FAILED
}

// ---------------------------------------------------------------------------
// Grapheme stepping over raw (possibly-invalid-UTF-8) bytes
// ---------------------------------------------------------------------------
//
// The buffer holds raw bytes (an invalid byte is reachable via the public
// `insert_text(&[u8])`). `from_utf8_lossy` would expand an invalid byte into the
// 3-byte U+FFFD, so advancing by `text::next`'s byte length on the lossy string
// over-shoots the logical buffer and desyncs the cursor from a grapheme
// boundary. These helpers step over exactly the **logical** bytes consumed: a
// valid grapheme advances by its real length; an invalid lead byte advances by 1.

/// Byte length of the first grapheme in `chunk` (raw bytes), or `None` when
/// empty. An invalid UTF-8 lead byte consumes exactly 1 byte.
fn next_grapheme_byte_len(chunk: &[u8]) -> Option<usize> {
    next_grapheme_with_width(chunk).map(|(len, _)| len)
}

/// Byte length and display width of the first grapheme in `chunk` (raw bytes),
/// or `None` when empty. An invalid lead byte → `(1, 1)` (one logical byte, one
/// replacement column).
fn next_grapheme_with_width(chunk: &[u8]) -> Option<(usize, usize)> {
    if chunk.is_empty() {
        return None;
    }
    // Decode only the valid UTF-8 prefix; if the very first byte is invalid the
    // prefix is empty and we advance one raw byte.
    let valid = match std::str::from_utf8(chunk) {
        Ok(s) => s,
        Err(e) => {
            let upto = e.valid_up_to();
            if upto == 0 {
                return Some((1, 1));
            }
            // SAFETY: `..upto` is the verified-valid prefix.
            unsafe { std::str::from_utf8_unchecked(&chunk[..upto]) }
        }
    };
    match crate::text::next(valid) {
        Some((len, w)) => Some((len.max(1), w)),
        // Non-empty raw bytes but no decodable grapheme → advance one byte.
        None => Some((1, 1)),
    }
}

/// Byte length of the **last** grapheme in `chunk` (raw bytes); how far back the
/// cursor steps. A trailing invalid byte steps back exactly 1 byte.
fn prev_grapheme_byte_len(chunk: &[u8]) -> usize {
    if chunk.is_empty() {
        return 0;
    }
    // Find the longest valid-UTF-8 suffix and step back one grapheme within it.
    // If the final byte is invalid, the suffix is empty → step back 1 raw byte.
    match std::str::from_utf8(chunk) {
        Ok(s) => crate::text::prev(s, s.len()).max(1),
        Err(e) => {
            let upto = e.valid_up_to();
            if upto == chunk.len() {
                // Whole chunk valid (unreachable here, but keep total): step back
                // one grapheme.
                // SAFETY: the whole chunk is valid UTF-8.
                let s = unsafe { std::str::from_utf8_unchecked(chunk) };
                crate::text::prev(s, s.len()).max(1)
            } else {
                // A byte at/after `upto` is invalid. If the LAST byte is part of
                // the invalid run, step back 1; otherwise the valid prefix ends
                // before the invalid byte and the cursor was already past it.
                1
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Editor
// ---------------------------------------------------------------------------

/// The gap-buffer text editor.
///
/// Search/replace state — the find string, the replacement string, and the
/// `EF_*` option flags — is **per-editor**: each editor independently
/// remembers its own last search. (C++ Turbo Vision kept these in class-static
/// globals shared across every editor; per-instance is the intentional
/// deviation, and is usually what a user wants.)
///
/// # Turbo Vision heritage
///
/// Ports `TEditor` (`teditor1.cpp`, `teditor2.cpp`, `edits.cpp`).
pub struct Editor {
    /// View state (geometry, flags, etc.) — the composition target.
    state: ViewState,
    /// The text buffer: `buf_len` logical bytes split by a `gap_len`-byte gap at
    /// `cur_ptr`. Always physically `buf_size` bytes long; gap bytes are stale.
    buffer: Vec<u8>,
    /// Physical buffer capacity (never grows in the base editor; see
    /// [`set_buf_size`](Editor::set_buf_size)).
    buf_size: usize,
    /// Logical text length. Invariant: `buf_len + gap_len == buf_size`.
    buf_len: usize,
    /// Gap size at `cur_ptr`.
    gap_len: usize,
    /// Selection start (logical offset).
    sel_start: usize,
    /// Selection end (logical offset).
    sel_end: usize,
    /// Cursor position (logical offset); the gap sits here physically.
    cur_ptr: usize,
    /// Cursor `(col, row)` in display coordinates.
    cur_pos: Point,
    /// Viewport top-left (scroll offset) in display coordinates.
    delta: Point,
    /// Content extent `(x = max line length, y = line count)`.
    limit: Point,
    /// The display row that `draw_ptr` corresponds to.
    draw_line: i32,
    /// Logical offset of the start of line `draw_line`.
    draw_ptr: usize,
    /// Bytes deleted since the last undo checkpoint (undo accounting).
    del_count: usize,
    /// Bytes inserted since the last undo checkpoint.
    ins_count: usize,
    /// Buffer allocation succeeded.
    is_valid: bool,
    /// Undo is enabled (always true in the base editor).
    can_undo: bool,
    /// File-editor mode: the buffer grows on demand and the save commands are
    /// enabled while focused. False for the plain editor / memo (fixed buffer).
    file_editor: bool,
    /// Whether this editor IS the internal clipboard editor. Set by the pump
    /// drain when the register-clipboard-editor deferred effect is processed.
    pub(crate) is_clipboard: bool,
    /// The buffer has unsaved changes.
    modified: bool,
    /// A persistent selection is in progress (selection mode toggled on).
    selecting: bool,
    /// Overwrite (vs insert) mode.
    overwrite: bool,
    /// Replicate leading whitespace on Enter.
    auto_indent: bool,
    /// Nested update locks; flush happens when this returns to 0.
    lock_count: u8,
    /// Pending redraw flags (`UF_*` bits).
    update_flags: u8,
    /// The pending first stroke of a two-key chord. Set when a key resolves to
    /// [`Resolve::Prefix`]; combined with the next stroke and then cleared.
    /// Replaces a small prefix state machine (idle / Ctrl-Q / Ctrl-K).
    pending: Option<KeyStroke>,
    /// Line ending used for newly inserted breaks.
    line_ending: LineEnding,
    /// Character-stepping encoding.
    encoding: Encoding,
    /// Horizontal scroll bar, by id (`None` = absent).
    h_scroll_bar: Option<ViewId>,
    /// Vertical scroll bar, by id (`None` = absent).
    v_scroll_bar: Option<ViewId>,
    /// Frame indicator, by id (`None` = absent).
    indicator: Option<ViewId>,
    /// Last search string. Stored per-instance (each editor remembers its own).
    find_str: String,
    /// Last replacement string (per-instance; see [`find_str`]).
    replace_str: String,
    /// The `EF_*` search options (per-instance; see [`find_str`]).
    editor_flags: u16,
    /// Absolute screen position of view-local `(0, 0)`, cached by `draw` so the
    /// mouse-tracking capture can localize absolute mouse coords (the same
    /// `abs_origin` pattern buttons use).
    abs_origin: Point,
    /// Per-hold mouse-track state — `Some` while a hold is in flight (see
    /// [`EditorTrack`]). Guards the tracked `MouseMove`/`MouseAuto`/wheel/
    /// `MouseUp` arms against stray events.
    track: Option<EditorTrack>,
    /// Cached answer from the "Replace this occurrence?" prompt
    /// ([`Context::request_message_box`] with `yes_no_cancel`). Set via
    /// [`View::set_modal_answer`]; consumed by [`do_search_replace`] on the
    /// next search-again dispatch to act on the user's choice before searching
    /// for the next match.
    pending_replace_answer: Option<crate::command::Command>,
}

impl Editor {
    /// Build an editor over a `buf_size`-byte buffer, wired to the given scroll
    /// bars and frame indicator (each optional, by id).
    ///
    /// The view grows with its parent on the right/bottom, is selectable, shows a
    /// cursor, and starts with an empty buffer. No redraw is performed here (a
    /// [`View`] has no [`Context`] at construction); the initial state is
    /// consistent for the first `draw`, and scroll-bar params publish on the
    /// first flush.
    pub fn new(
        bounds: Rect,
        h_scroll_bar: Option<ViewId>,
        v_scroll_bar: Option<ViewId>,
        indicator: Option<ViewId>,
        buf_size: usize,
    ) -> Self {
        let mut state = ViewState::new(bounds);
        state.grow_mode = GrowMode {
            hi_x: true,
            hi_y: true,
            ..Default::default()
        };
        state.options = Options {
            selectable: true,
            ..Default::default()
        };
        state.show_cursor();
        // Allocate the buffer at full physical size up front; gap bytes are stale.
        let buffer = vec![0u8; buf_size];
        let mut ed = Editor {
            state,
            buffer,
            buf_size,
            buf_len: 0,
            gap_len: buf_size,
            sel_start: 0,
            sel_end: 0,
            cur_ptr: 0,
            cur_pos: Point::new(0, 0),
            delta: Point::new(0, 0),
            limit: Point::new(MAX_LINE_LENGTH, 1),
            draw_line: 0,
            draw_ptr: 0,
            del_count: 0,
            ins_count: 0,
            is_valid: buf_size != 0,
            can_undo: true,
            file_editor: false,
            is_clipboard: false,
            modified: false,
            selecting: false,
            overwrite: false,
            auto_indent: true,
            lock_count: 0,
            update_flags: 0,
            pending: None,
            line_ending: DEFAULT_LINE_ENDING,
            encoding: Encoding::Default,
            h_scroll_bar,
            v_scroll_bar,
            indicator,
            find_str: String::new(),
            replace_str: String::new(),
            editor_flags: 0,
            abs_origin: Point::new(0, 0),
            track: None,
            pending_replace_answer: None,
        };
        // Initialize logical length to 0 — flag-set only (no flush; no Context).
        ed.set_buf_len(0);
        ed
    }

    /// Construct an `Editor` in file-editor mode (growable buffer, save commands).
    /// The buffer starts empty and `set_buf_size` grows it on load. A growable
    /// empty buffer is valid (unlike a fixed 0-size buffer, which would be
    /// `is_valid == false`).
    pub(crate) fn new_file_editor(
        bounds: Rect,
        h_scroll_bar: Option<ViewId>,
        v_scroll_bar: Option<ViewId>,
        indicator: Option<ViewId>,
    ) -> Self {
        let mut ed = Editor::new(bounds, h_scroll_bar, v_scroll_bar, indicator, 0);
        ed.file_editor = true;
        ed.is_valid = true; // a growable (file) editor with an empty buffer is valid
        ed
    }

    // -- test/inspection accessors ------------------------------------------

    /// Logical text length.
    pub fn buf_len(&self) -> usize {
        self.buf_len
    }

    /// The cursor position (logical offset).
    pub fn cur_ptr(&self) -> usize {
        self.cur_ptr
    }

    /// The content extent (`(max line length, line count)`).
    pub fn limit(&self) -> Point {
        self.limit
    }

    /// The scroll offset (viewport top-left).
    pub fn delta(&self) -> Point {
        self.delta
    }

    /// The cursor's display position (zero-based row/col). Inspection hook for
    /// tests asserting indicator/cursor wiring.
    pub fn cur_pos(&self) -> Point {
        self.cur_pos
    }

    /// Whether the buffer has unsaved changes.
    pub fn modified(&self) -> bool {
        self.modified
    }

    /// Reconstruct the logical text (test oracle).
    pub fn text(&self) -> Vec<u8> {
        (0..self.buf_len).map(|p| self.buf_char(p)).collect()
    }

    // -- gap arithmetic -----------------------------------------------------

    /// Physical buffer index of logical offset `p`.
    fn buf_ptr(&self, p: usize) -> usize {
        if p < self.cur_ptr {
            p
        } else {
            p + self.gap_len
        }
    }

    /// The byte at logical offset `p`.
    fn buf_char(&self, p: usize) -> u8 {
        self.buffer[self.buf_ptr(p)]
    }

    /// Copy up to `dest.len()` logical bytes from `p` into `dest`; returns the
    /// count copied. Used to materialize a contiguous slice across the gap for
    /// grapheme decoding.
    fn get_text(&self, p: usize, dest: &mut [u8]) -> usize {
        if p < self.buf_len {
            let count = dest.len().min(self.buf_len - p);
            for (i, slot) in dest.iter_mut().enumerate().take(count) {
                *slot = self.buf_char(p + i);
            }
            count
        } else {
            0
        }
    }

    /// Read up to `n` logical bytes starting at `p` into a `Vec` (the contiguous
    /// materialization used by the grapheme helpers).
    fn read_chunk(&self, p: usize, n: usize) -> Vec<u8> {
        let mut buf = vec![0u8; n];
        let count = self.get_text(p, &mut buf);
        buf.truncate(count);
        buf
    }

    // -- character navigation -----------------------------------------------

    /// Advance one grapheme from `p` (or a `\r\n` pair, or one byte under
    /// single-byte encoding).
    fn next_char(&self, p: usize) -> usize {
        if p + 1 < self.buf_len {
            if self.buf_char(p) == b'\r' && self.buf_char(p + 1) == b'\n' {
                return p + 2;
            }
            if self.encoding == Encoding::SingleByte {
                return p + 1;
            }
            // Materialize up to 4 bytes and step one grapheme. Clamp the advance
            // to bytes actually consumed from `chunk`: an invalid UTF-8 byte makes
            // `from_utf8_lossy` expand 1 byte → the 3-byte U+FFFD, so without the
            // clamp the cursor would jump 3 logical bytes and desync from a
            // grapheme boundary.
            let chunk = self.read_chunk(p, 4);
            match next_grapheme_byte_len(&chunk) {
                Some(len) => p + len,
                None => self.buf_len,
            }
        } else {
            self.buf_len
        }
    }

    /// Retreat one grapheme from `p` (or a `\r\n` pair, or one byte).
    fn prev_char(&self, p: usize) -> usize {
        if p > 1 {
            if self.buf_char(p - 2) == b'\r' && self.buf_char(p - 1) == b'\n' {
                return p - 2;
            }
            if self.encoding == Encoding::SingleByte {
                return p - 1;
            }
            let count = 4.min(p);
            let chunk = self.read_chunk(p - count, count);
            // Step back over the last grapheme in `chunk`, clamped to its real
            // byte length: a trailing invalid byte must retreat exactly 1 logical
            // byte (not the 3 of an expanded U+FFFD).
            let back = prev_grapheme_byte_len(&chunk);
            p - back
        } else {
            // p == 0 or p == 1 → 0.
            0
        }
    }

    /// Advance `p` over one char and `pos` over its display width (tabs round up
    /// to the next multiple of 8). Returns false at end of buffer.
    fn next_char_and_pos(&self, p: &mut usize, pos: &mut i32) -> bool {
        if *p < self.buf_len {
            if self.encoding == Encoding::SingleByte {
                *p += 1;
                *pos += 1;
            } else {
                let chunk = self.read_chunk(*p, 4);
                if chunk.first() == Some(&b'\t') {
                    *p += 1;
                    *pos = (*pos | 7) + 1;
                } else {
                    // Width-aware step, with the advance clamped to bytes actually
                    // consumed from `chunk` (an invalid byte advances exactly 1; see
                    // `next_char`).
                    match next_grapheme_with_width(&chunk) {
                        Some((len, w)) => {
                            *p += len;
                            *pos += w as i32;
                        }
                        None => {
                            *p += 1;
                        }
                    }
                }
            }
            true
        } else {
            false
        }
    }

    /// Display column of `target`, scanning from `p`.
    fn char_pos(&self, mut p: usize, target: usize) -> i32 {
        let mut pos = 0;
        while p < target {
            if !self.next_char_and_pos(&mut p, &mut pos) {
                break;
            }
        }
        pos
    }

    /// Logical offset at display column `target` on the line starting at `p`
    /// (stops at a line break).
    fn char_ptr(&self, mut p: usize, target: i32) -> usize {
        let mut pos = 0;
        let mut prev_p = p;
        while p < self.buf_len && pos < target {
            let c = self.buf_char(p);
            if c == b'\r' || c == b'\n' {
                break;
            }
            prev_p = p;
            if !self.next_char_and_pos(&mut p, &mut pos) {
                break;
            }
        }
        if pos > target {
            p = prev_p;
        }
        p
    }

    /// Offset of the first line break at or after `p` (or `buf_len`).
    fn line_end(&self, p: usize) -> usize {
        let mut i = p;
        while i < self.buf_len {
            let c = self.buf_char(i);
            if c == b'\r' || c == b'\n' {
                return i;
            }
            i += 1;
        }
        self.buf_len
    }

    /// Offset of the start of the line containing `p`.
    fn line_start(&self, p: usize) -> usize {
        let mut i = p;
        while i > 0 {
            i -= 1;
            let c = self.buf_char(i);
            if c == b'\r' {
                if i + 1 != self.cur_ptr && i + 1 != self.buf_len && self.buf_char(i + 1) == b'\n' {
                    return i + 2;
                }
                return i + 1;
            } else if c == b'\n' {
                return i + 1;
            }
        }
        0
    }

    /// Start of the line after `p`.
    fn next_line(&self, p: usize) -> usize {
        self.next_char(self.line_end(p))
    }

    /// Start of the line before `p`.
    fn prev_line(&self, p: usize) -> usize {
        self.line_start(self.prev_char(p))
    }

    /// Start of the next word.
    fn next_word(&self, mut p: usize) -> usize {
        if p < self.buf_len {
            let mut a = self.buf_char(p);
            loop {
                let b = a;
                p = self.next_char(p);
                if p >= self.buf_len {
                    break;
                }
                a = self.buf_char(p);
                if is_word_boundary(a, b) {
                    break;
                }
            }
        }
        p
    }

    /// Start of the previous word.
    fn prev_word(&self, mut p: usize) -> usize {
        if p > 0 {
            p = self.prev_char(p);
            if p > 0 {
                let mut a = self.buf_char(p);
                let mut b;
                loop {
                    b = a;
                    p = self.prev_char(p);
                    a = self.buf_char(p);
                    if p == 0 || is_word_boundary(a, b) {
                        break;
                    }
                }
                if is_word_boundary(a, b) {
                    p = self.next_char(p);
                }
            }
        }
        p
    }

    /// First non-whitespace offset on `p`'s line (or the line start if that
    /// equals `p`).
    fn indented_line_start(&self, p: usize) -> usize {
        let start_ptr = self.line_start(p);
        let mut dest_ptr = start_ptr;
        while dest_ptr < self.buf_len {
            let c = self.buf_char(dest_ptr);
            if c == b' ' || c == b'\t' {
                dest_ptr += 1;
            } else {
                break;
            }
        }
        if dest_ptr == p { start_ptr } else { dest_ptr }
    }

    /// Move `count` display lines from `p`, preserving the column.
    fn line_move(&self, mut p: usize, mut count: i32) -> usize {
        let i0 = p;
        p = self.line_start(p);
        let pos = self.char_pos(p, i0);
        let mut i = p;
        while count != 0 {
            i = p;
            if count < 0 {
                p = self.prev_line(p);
                count += 1;
            } else {
                p = self.next_line(p);
                count -= 1;
            }
        }
        if p != i {
            p = self.char_ptr(p, pos);
        }
        p
    }

    /// Logical offset under the view-local mouse point.
    fn get_mouse_ptr(&self, mouse_local: Point) -> usize {
        let mx = mouse_local.x.max(0).min(self.state.size.x - 1);
        let my = mouse_local.y.max(0).min(self.state.size.y - 1);
        self.char_ptr(
            self.line_move(self.draw_ptr, my + self.delta.y - self.draw_line),
            mx + self.delta.x,
        )
    }

    // -- selection / cursor -------------------------------------------------

    /// Whether a non-empty selection exists.
    pub(crate) fn has_selection(&self) -> bool {
        self.sel_start != self.sel_end
    }

    /// Infer the line ending from the first break in the buffer.
    fn detect_line_ending_type(&self) -> LineEnding {
        for p in 0..self.buf_len {
            match self.buf_char(p) {
                b'\r' => {
                    if p + 1 < self.buf_len && self.buf_char(p + 1) == b'\n' {
                        return LineEnding::CrLf;
                    }
                    return LineEnding::Cr;
                }
                b'\n' => return LineEnding::Lf,
                _ => {}
            }
        }
        DEFAULT_LINE_ENDING
    }

    /// Length of `p` after rewriting all of its line breaks to
    /// [`line_ending`](Editor::line_ending).
    fn length_with_converted_line_endings(&self, p: &[u8]) -> usize {
        let le = self.line_ending.bytes().len();
        let mut new_len = 0;
        let mut i = 0;
        while i < p.len() {
            if p[i] == b'\n' || p[i] == b'\r' {
                new_len += le;
                if p[i] == b'\r' && i + 1 < p.len() && p[i + 1] == b'\n' {
                    i += 1;
                }
            } else {
                new_len += 1;
            }
            i += 1;
        }
        new_len
    }

    /// Write `src` into the buffer at `dest_off` with its line breaks converted.
    /// Writes exactly `length_with_converted_line_endings(src)` bytes.
    fn copy_and_convert_line_endings(&mut self, dest_off: usize, src: &[u8]) {
        let le = self.line_ending.bytes();
        let mut d = dest_off;
        let mut i = 0;
        while i < src.len() {
            let c = src[i];
            if c == b'\n' || c == b'\r' {
                self.buffer[d] = le[0];
                d += 1;
                if le.len() > 1 {
                    self.buffer[d] = le[1];
                    d += 1;
                }
                if c == b'\r' && i + 1 < src.len() && src[i + 1] == b'\n' {
                    i += 1;
                }
            } else {
                self.buffer[d] = c;
                d += 1;
            }
            i += 1;
        }
    }

    /// Reset the buffer to `length` logical bytes (the gap is everything after),
    /// zero cursor/selection/scroll, and recompute the line count.
    fn set_buf_len(&mut self, length: usize) {
        self.buf_len = length;
        self.gap_len = self.buf_size - length;
        self.sel_start = 0;
        self.sel_end = 0;
        self.cur_ptr = 0;
        self.delta = Point::new(0, 0);
        self.cur_pos = self.delta;
        self.limit.x = MAX_LINE_LENGTH;
        // countLines(&buffer[gapLen], bufLen): the logical bytes live after the
        // gap when curPtr == 0.
        let lines = count_lines(&self.buffer[self.gap_len..self.gap_len + self.buf_len]);
        self.limit.y = lines + 1;
        self.draw_line = 0;
        self.draw_ptr = 0;
        self.del_count = 0;
        self.ins_count = 0;
        self.modified = false;
        self.line_ending = self.detect_line_ending_type();
        self.update(UF_VIEW);
    }

    /// Ensure the buffer can hold `new_size` bytes. The plain editor never grows
    /// (returns whether `new_size` already fits). In **file-editor mode** it grows
    /// the buffer: round `new_size` up to a 0x1000 boundary, resize the `Vec`, and
    /// move the post-gap tail to the new end so the gap widens in place.
    fn set_buf_size(&mut self, new_size: usize) -> bool {
        if new_size <= self.buf_size {
            // A shrink path (reclaiming memory when new_size < buf_size) is not
            // modeled — memory is not reclaimed, but the logical text and all
            // invariants are unaffected.
            return true;
        }
        if !self.file_editor {
            return false; // plain editor / memo: fixed buffer, cannot grow.
        }
        // Grow path. `Vec` growth is infallible, so there is no
        // out-of-memory return.
        let rounded = (new_size + 0x0FFF) & !0x0FFF; // round up to 0x1000
        let old_size = self.buf_size;
        let n = self.buf_len - self.cur_ptr + self.del_count; // bytes after the gap
        self.buffer.resize(rounded, 0);
        // Move the tail [old_size - n .. old_size] to [rounded - n .. rounded]
        // (copy_within is overlap-safe).
        self.buffer.copy_within(old_size - n..old_size, rounded - n);
        self.buf_size = rounded;
        self.gap_len = self.buf_size - self.buf_len;
        true
    }

    /// Replace the whole buffer with `text` (used by [`Memo::set_value`]).
    /// All-or-nothing: if `text` does not fit the buffer and it cannot grow
    /// (plain editor / memo have a fixed buffer; file-editor mode grows instead),
    /// it is a no-op. The replaced text is left unselected.
    pub fn set_text(&mut self, text: &[u8]) {
        if self.set_buf_size(text.len()) {
            let start = self.buf_size - text.len();
            self.buffer[start..].copy_from_slice(text);
            self.set_buf_len(text.len());
        }
    }

    /// Set the selection to `[new_start, new_end)` and place the cursor at the
    /// start endpoint when `cur_start`, else the end. Moving the gap to the chosen
    /// endpoint is the load-bearing operation.
    fn set_select(&mut self, new_start: usize, new_end: usize, cur_start: bool) {
        let p = if cur_start { new_start } else { new_end };

        let mut flags = UF_UPDATE;
        if (new_start != self.sel_start || new_end != self.sel_end)
            && (new_start != new_end || self.sel_start != self.sel_end)
        {
            flags = UF_VIEW;
        }

        if p != self.cur_ptr {
            if p > self.cur_ptr {
                // Move text from after the gap to before it.
                let l = p - self.cur_ptr;
                let src = self.cur_ptr + self.gap_len;
                self.buffer.copy_within(src..src + l, self.cur_ptr);
                let lines = count_lines(&self.buffer[self.cur_ptr..self.cur_ptr + l]);
                self.cur_pos.y += lines;
                self.cur_ptr = p;
            } else {
                let l = self.cur_ptr - p;
                self.cur_ptr = p;
                let lines = count_lines(&self.buffer[self.cur_ptr..self.cur_ptr + l]);
                self.cur_pos.y -= lines;
                // Move text from before the gap to after it.
                let dst = self.cur_ptr + self.gap_len;
                self.buffer.copy_within(self.cur_ptr..self.cur_ptr + l, dst);
            }
            self.del_count = 0;
            self.ins_count = 0;
            // A shrink-to-fit here is a no-op (the buffer never shrinks; see
            // set_buf_size).
        }
        self.draw_line = self.cur_pos.y;
        self.draw_ptr = self.line_start(p);
        self.cur_pos.x = self.char_pos(self.draw_ptr, p);
        self.sel_start = new_start;
        self.sel_end = new_end;
        self.update(flags);
    }

    /// Move the cursor to `p`, optionally extending the selection (and snapping to
    /// word/line granularity per `select_mode`).
    fn set_cur_ptr(&mut self, mut p: usize, select_mode: u8) {
        let mut anchor = if (select_mode & SM_EXTEND) == 0 {
            p
        } else if self.cur_ptr == self.sel_start {
            self.sel_end
        } else {
            self.sel_start
        };

        if p < anchor {
            if (select_mode & SM_DOUBLE) != 0 {
                p = self.prev_word(self.next_word(p));
                anchor = self.next_word(self.prev_word(anchor));
            } else if (select_mode & SM_TRIPLE) != 0 {
                p = self.prev_line(self.next_line(p));
                anchor = self.next_line(self.prev_line(anchor));
            }
            self.set_select(p, anchor, true);
        } else {
            if (select_mode & SM_DOUBLE) != 0 {
                p = self.next_word(p);
                anchor = self.prev_word(self.next_word(anchor));
            } else if (select_mode & SM_TRIPLE) != 0 {
                p = self.next_line(p);
                anchor = self.prev_line(self.next_line(anchor));
            }
            self.set_select(anchor, p, false);
        }
    }

    /// Begin a persistent selection.
    fn start_select(&mut self) {
        self.hide_select();
        self.selecting = true;
    }

    /// Collapse the selection to the cursor.
    fn hide_select(&mut self) {
        self.selecting = false;
        self.set_select(self.cur_ptr, self.cur_ptr, false);
    }

    /// Flip overwrite mode and the block-cursor flag.
    fn toggle_ins_mode(&mut self) {
        self.overwrite = !self.overwrite;
        let ins = self.state.state.cursor_ins;
        self.state.state.cursor_ins = !ins;
    }

    /// Flip between multibyte and single-byte encoding.
    fn toggle_encoding(&mut self) {
        self.encoding = if self.encoding == Encoding::Default {
            Encoding::SingleByte
        } else {
            Encoding::Default
        };
        self.update_flags |= UF_VIEW;
        let cur_start = self.cur_ptr < self.sel_end;
        self.set_select(self.sel_start, self.sel_end, cur_start);
    }

    // -- insertion / deletion -----------------------------------------------

    /// THE core edit: replace a range with bytes from `p`, with optional undo and
    /// optional selection of the inserted text.
    ///
    /// `p` must **not** alias `self.buffer` — callers snapshot first.
    fn insert_buffer(
        &mut self,
        p: &[u8],
        offset: usize,
        length: usize,
        allow_undo: bool,
        select_text: bool,
    ) -> bool {
        self.selecting = false;
        let sel_len = self.sel_end - self.sel_start;
        if sel_len == 0 && length == 0 {
            return true;
        }

        let mut del_len = 0;
        if allow_undo {
            if self.cur_ptr == self.sel_start {
                del_len = sel_len;
            } else if sel_len > self.ins_count {
                del_len = sel_len - self.ins_count;
            }
        }

        let ins_len = self.length_with_converted_line_endings(&p[offset..offset + length]);
        let new_size = self.buf_len + self.del_count - sel_len + del_len + ins_len;

        if new_size > self.buf_len + self.del_count && !self.set_buf_size(new_size) {
            // edOutOfMemory — collapse the selection and bail.
            self.sel_end = self.sel_start;
            return false;
        }

        let sel_lines = count_lines(&self.buffer[self.buf_ptr(self.sel_start)..][..sel_len]);
        if self.cur_ptr == self.sel_end {
            if allow_undo {
                if del_len > 0 {
                    // memmove(&buffer[curPtr+gapLen-delCount-delLen],
                    //         &buffer[selStart], delLen)
                    let dst = self.cur_ptr + self.gap_len - self.del_count - del_len;
                    let src = self.sel_start;
                    self.buffer.copy_within(src..src + del_len, dst);
                }
                self.ins_count -= sel_len - del_len;
            }
            self.cur_ptr = self.sel_start;
            self.cur_pos.y -= sel_lines;
        }
        if self.delta.y > self.cur_pos.y {
            self.delta.y -= sel_lines;
            if self.delta.y < self.cur_pos.y {
                self.delta.y = self.cur_pos.y;
            }
        }

        if length > 0 {
            self.copy_and_convert_line_endings(self.cur_ptr, &p[offset..offset + length]);
        }

        let lines = count_lines(&self.buffer[self.cur_ptr..self.cur_ptr + ins_len]);
        self.cur_ptr += ins_len;
        // bufLen += insLen - selLen; gapLen -= insLen - selLen. The delta is signed
        // (negative on a net deletion), so do it with isize to avoid usize wrap.
        let delta_len = ins_len as isize - sel_len as isize;
        self.buf_len = (self.buf_len as isize + delta_len) as usize;
        self.gap_len = (self.gap_len as isize - delta_len) as usize;
        self.cur_pos.y += lines;
        self.draw_line = self.cur_pos.y;
        self.draw_ptr = self.line_start(self.cur_ptr);
        self.cur_pos.x = self.char_pos(self.draw_ptr, self.cur_ptr);
        if !select_text {
            self.sel_start = self.cur_ptr;
        }
        self.sel_end = self.cur_ptr;
        if allow_undo {
            self.del_count += del_len;
            self.ins_count += ins_len;
        }
        self.limit.y += lines - sel_lines;
        self.delta.y = 0.max(self.delta.y.min(self.limit.y - self.state.size.y));
        // C++: `if (isClipboard() == False) modified = True;`
        // The clipboard editor itself stays unmodified (it is not a "file" —
        // modified tracking is for the save guard).
        if !self.is_clipboard {
            self.modified = true;
        }
        // setBufSize(bufLen + delCount) — no-op for the base.
        if sel_lines == 0 && lines == 0 {
            self.update(UF_LINE);
        } else {
            self.update(UF_VIEW);
        }
        true
    }

    /// Insert `text` at the cursor (the ctx-free insert core), optionally
    /// selecting the inserted bytes.
    fn insert_text_core(&mut self, text: &[u8], select_text: bool) -> bool {
        self.insert_buffer(text, 0, text.len(), self.can_undo, select_text)
    }

    /// Delete the current selection.
    fn delete_select(&mut self) {
        self.insert_buffer(&[], 0, 0, self.can_undo, false);
    }

    /// Insert `data` (another editor's selection bytes) into this editor. The
    /// inserted text is selected only when this is the clipboard editor; a normal
    /// destination editor leaves it unselected.
    ///
    /// Called by the pump's clipboard brokers: the cut/copy path (destination =
    /// clipboard editor, selects the content) and the paste path (destination =
    /// normal editor, no selection). After the insert the caller must call
    /// `flush_if_unlocked` to publish updated scroll-bar params.
    pub(crate) fn insert_from(&mut self, data: &[u8], ctx: &mut Context) -> bool {
        let res = self.insert_buffer(data, 0, data.len(), self.can_undo, self.is_clipboard);
        self.flush_if_unlocked(ctx);
        res
    }

    /// Extract the current selection as a byte vec (for the clipboard broker).
    /// The selection bytes are always physically contiguous in the gap buffer (the
    /// cursor sits at one selection endpoint, so the gap never lies inside the
    /// selection).
    pub(crate) fn selection_bytes(&self) -> Vec<u8> {
        let len = self.sel_end - self.sel_start;
        let start = self.buf_ptr(self.sel_start);
        self.buffer[start..start + len].to_vec()
    }

    /// Delete the range `[start_ptr, end_ptr)`, honoring an existing selection
    /// when `del_select`.
    fn delete_range(&mut self, start_ptr: usize, end_ptr: usize, del_select: bool) {
        if self.has_selection() && del_select {
            self.delete_select();
        } else {
            self.set_select(self.cur_ptr, end_ptr, true);
            self.delete_select();
            self.set_select(start_ptr, self.cur_ptr, false);
            self.delete_select();
        }
    }

    /// Insert a line break, replicating the current line's leading indent when
    /// auto-indent is on.
    fn new_line(&mut self) {
        let p = self.line_start(self.cur_ptr);
        let mut i = p;
        while i < self.cur_ptr {
            let c = self.buffer[self.buf_ptr(i)];
            if c == b' ' || c == b'\t' {
                i += 1;
            } else {
                break;
            }
        }
        self.insert_text_core(b"\n", false);
        if self.auto_indent {
            // Snapshot the indent run BEFORE inserting (source must not alias
            // self.buffer). The run is physically contiguous (it precedes the
            // cursor, and after the "\n" insert the gap sits past the cursor).
            let indent: Vec<u8> = (p..p + (i - p)).map(|q| self.buf_char(q)).collect();
            self.insert_text_core(&indent, false);
        }
    }

    /// Single-level undo: restore the most recently deleted text and drop the most
    /// recently inserted.
    fn undo(&mut self) {
        if self.del_count != 0 || self.ins_count != 0 {
            self.sel_start = self.cur_ptr - self.ins_count;
            self.sel_end = self.cur_ptr;
            let length = self.del_count;
            self.del_count = 0;
            self.ins_count = 0;
            // Source = the deleted text, which lives in the gap just before its
            // end. Snapshot first (no alias).
            let start = self.cur_ptr + self.gap_len - length;
            let snapshot: Vec<u8> = self.buffer[start..start + length].to_vec();
            self.insert_buffer(&snapshot, 0, length, false, true);
        }
    }

    // -- search -------------------------------------------------------------

    /// Find `needle` from the cursor; on a hit, select it and track the cursor.
    /// This is the low-level search primitive; the interactive Find / Replace
    /// dialogs drive it through [`do_search_replace`].
    ///
    /// Context-free: instead of flushing inline, this only records the scroll
    /// target; the actual flush happens on the next `handle_event` boundary.
    pub fn search(&mut self, needle: &str, opts: u16) -> bool {
        let needle = needle.as_bytes();
        let mut pos = self.cur_ptr;
        loop {
            // bufLen - pos logical bytes are reachable from `pos`; they are NOT
            // contiguous across the gap, so materialize them.
            let block = self.read_chunk(pos, self.buf_len - pos);
            let i = if (opts & EF_CASE_SENSITIVE) != 0 {
                scan(&block, needle)
            } else {
                i_scan(&block, needle)
            };
            if i == SEARCH_FAILED {
                return false;
            }
            let hit = i + pos;
            let nlen = needle.len();
            let whole_ok = (opts & EF_WHOLE_WORDS_ONLY) == 0
                || !((hit != 0 && is_word_char(self.buf_char(hit - 1)))
                    || (hit + nlen != self.buf_len && is_word_char(self.buf_char(hit + nlen))));
            if whole_ok {
                self.set_select(hit, hit + nlen, false);
                let center = !self.cursor_visible();
                self.track_cursor(center);
                return true;
            } else {
                pos = hit + 1;
            }
        }
    }

    // -- viewport -----------------------------------------------------------

    /// Whether the cursor row is within the visible viewport.
    fn cursor_visible(&self) -> bool {
        self.cur_pos.y >= self.delta.y && self.cur_pos.y < self.delta.y + self.state.size.y
    }

    /// Set the scroll offset (clamped to the content extent) and flag a redraw.
    fn scroll_to(&mut self, x: i32, y: i32) {
        let x = 0.max(x.min(self.limit.x - self.state.size.x));
        let y = 0.max(y.min(self.limit.y - self.state.size.y));
        if x != self.delta.x || y != self.delta.y {
            self.delta.x = x;
            self.delta.y = y;
            self.update(UF_VIEW);
        }
    }

    /// One tick of the middle-button pan: scroll by `last - mouse` (the movement
    /// since the previous tick) and remember the new position, never touching the
    /// cursor or selection. The pan does not take an update lock, so the scroll's
    /// view-repaint flushes immediately, which `flush_if_unlocked` mirrors.
    fn pan_tick(&mut self, last: Point, mouse: Point, ctx: &mut Context) {
        let d = Point::new(
            self.delta.x + last.x - mouse.x,
            self.delta.y + last.y - mouse.y,
        );
        self.scroll_to(d.x, d.y);
        self.track = Some(EditorTrack::Pan { last: mouse });
        self.flush_if_unlocked(ctx);
    }

    /// Scroll so the cursor is visible (centering it when `center`).
    fn track_cursor(&mut self, center: bool) {
        if center {
            self.scroll_to(
                self.cur_pos.x - self.state.size.x + 1,
                self.cur_pos.y - self.state.size.y / 2,
            );
        } else {
            self.scroll_to(
                (self.cur_pos.x - self.state.size.x + 1).max(self.delta.x.min(self.cur_pos.x)),
                (self.cur_pos.y - self.state.size.y + 1).max(self.delta.y.min(self.cur_pos.y)),
            );
        }
    }

    /// Adopt new scroll offsets read from the scroll bars (applied by the pump).
    ///
    /// Public ctx-taking entry: the pump reads each scroll bar's value, then calls
    /// this with `dx`/`dy` (`None` = no bar). For each present bar, if its value
    /// differs from the current offset, adopt it and flag a full redraw; then
    /// flush.
    pub fn apply_scroll_delta(&mut self, dx: Option<i32>, dy: Option<i32>, ctx: &mut Context) {
        if let Some(x) = dx
            && x != self.delta.x
        {
            self.delta.x = x;
            self.update(UF_VIEW);
        }
        if let Some(y) = dy
            && y != self.delta.y
        {
            self.delta.y = y;
            self.update(UF_VIEW);
        }
        self.flush_if_unlocked(ctx);
    }

    /// Public ctx-taking insert used by the clipboard-paste broker. Inserts then
    /// flushes (the flush republishes scroll-bar params next pump).
    pub fn insert_text(&mut self, text: &[u8], select_text: bool, ctx: &mut Context) {
        self.lock();
        self.insert_text_core(text, select_text);
        let center = !self.cursor_visible();
        self.track_cursor(center);
        self.unlock(ctx);
    }

    // -- update / lock / flush ----------------------------------------------

    /// Record pending redraw flags; no inline flush (see the module seam).
    fn update(&mut self, flags: u8) {
        self.update_flags |= flags;
    }

    /// Take an update lock (deferring flushes until it is released).
    fn lock(&mut self) {
        self.lock_count += 1;
    }

    /// Release an update lock; flush when the count returns to 0.
    fn unlock(&mut self, ctx: &mut Context) {
        if self.lock_count > 0 {
            self.lock_count -= 1;
            if self.lock_count == 0 {
                self.do_update(ctx);
            }
        }
    }

    /// Flush if not inside a lock — the trailing flush for `handle_event`'s
    /// unlocked arms.
    fn flush_if_unlocked(&mut self, ctx: &mut Context) {
        if self.lock_count == 0 {
            self.do_update(ctx);
        }
    }

    /// Publish cursor, scrollbar params, and the indicator value if any update is
    /// pending. The whole-tree redraw handles the repaint.
    fn do_update(&mut self, ctx: &mut Context) {
        if self.update_flags == 0 {
            return;
        }
        // Cursor in view-local coords = content position minus the scroll offset.
        self.state
            .set_cursor(self.cur_pos.x - self.delta.x, self.cur_pos.y - self.delta.y);
        // No explicit text repaint here: the whole tree is redrawn and diffed.
        let size = self.state.size;
        if let Some(h) = self.h_scroll_bar {
            ctx.request_scroll_bar_params(
                h,
                Some(self.delta.x),
                Some(0),
                Some(self.limit.x - size.x),
                Some(size.x / 2),
                Some(1),
            );
        }
        if let Some(v) = self.v_scroll_bar {
            ctx.request_scroll_bar_params(
                v,
                Some(self.delta.y),
                Some(0),
                Some(self.limit.y - size.y),
                Some(size.y - 1),
                Some(1),
            );
        }
        if let Some(ind) = self.indicator {
            ctx.set_indicator_value(ind, self.cur_pos, self.modified);
        }
        if self.state.state.active {
            self.update_commands(ctx);
        }
        self.update_flags = 0;
    }

    /// Enable `command` iff `enable` and this editor is active, else disable it.
    fn set_cmd_state(&self, command: crate::command::Command, enable: bool, ctx: &mut Context) {
        if enable && self.state.state.active {
            ctx.enable_command(command);
        } else {
            ctx.disable_command(command);
        }
    }

    /// Gray/ungray the editing commands according to the current state.
    fn update_commands(&self, ctx: &mut Context) {
        use crate::command::Command;
        let has_undo = self.del_count != 0 || self.ins_count != 0;
        self.set_cmd_state(Command::UNDO, has_undo, ctx);
        // The clipboard editor does not update cut/copy/paste (it is not a
        // user-editable file editor).
        if !self.is_clipboard {
            let has_sel = self.has_selection();
            self.set_cmd_state(Command::CUT, has_sel, ctx);
            self.set_cmd_state(Command::COPY, has_sel, ctx);
            // Paste is allowed when there is no clipboard editor, or it has a
            // selection to paste.
            let paste_ok = ctx.clipboard_editor_id().is_none() || ctx.clipboard_has_selection();
            self.set_cmd_state(Command::PASTE, paste_ok, ctx);
        }
        self.set_cmd_state(Command::CLEAR, self.has_selection(), ctx);
        self.set_cmd_state(Command::FIND, true, ctx);
        self.set_cmd_state(Command::REPLACE, true, ctx);
        self.set_cmd_state(Command::SEARCH_AGAIN, true, ctx);
        if self.file_editor {
            // File editor: Save / Save As are always enabled while active.
            self.set_cmd_state(Command::SAVE, true, ctx);
            self.set_cmd_state(Command::SAVE_AS, true, ctx);
        }
    }

    /// Clear the dirty flag and request an indicator/cursor redraw (the tail of a
    /// successful save).
    fn clear_modified(&mut self) {
        self.modified = false;
        self.update(UF_UPDATE);
    }

    // -- clipboard ------------------------------------------------------------

    /// Copy the selection to the internal or system clipboard.
    fn clip_copy(&mut self, ctx: &mut Context) -> bool {
        // The clipboard editor cannot copy from itself.
        if ctx.clipboard_editor_id() == self.state.id() && self.state.id().is_some() {
            return false;
        }
        if let Some(clipboard_id) = ctx.clipboard_editor_id() {
            // Internal clipboard: snapshot the selection bytes and hand them to
            // the clipboard editor.
            let data = self.selection_bytes();
            ctx.clipboard_editor_receive(clipboard_id, data);
            self.selecting = false;
            self.update(UF_UPDATE);
            true
        } else {
            // System clipboard path.
            let len = self.sel_end - self.sel_start;
            let start = self.buf_ptr(self.sel_start);
            // The selection is always physically contiguous (curPtr is always at
            // selStart or selEnd, so the gap never sits inside the selection).
            let text = String::from_utf8_lossy(&self.buffer[start..start + len]).into_owned();
            ctx.set_clipboard(text);
            self.selecting = false;
            self.update(UF_UPDATE);
            true
        }
    }

    /// Cut: copy the selection, then delete it.
    fn clip_cut(&mut self, ctx: &mut Context) {
        if self.clip_copy(ctx) {
            self.delete_select();
        }
    }

    /// Paste from the internal or system clipboard.
    fn clip_paste(&mut self, ctx: &mut Context) {
        if let Some(clipboard_id) = ctx.clipboard_editor_id() {
            // Internal clipboard: ClipboardEditorPaste broker reads clipboard's
            // selection and inserts into self. C++: `insertFrom(clipboard)`.
            if let Some(id) = self.state.id() {
                ctx.clipboard_editor_paste(id, clipboard_id);
            }
        } else {
            // System clipboard path.
            if let Some(id) = self.state.id() {
                ctx.editor_paste(id);
            }
        }
    }

    // -- find/replace accessor methods ----------------------------------------

    /// Read the find string — for the drain-handler pre-fill.
    pub(crate) fn find_str(&self) -> &str {
        &self.find_str
    }

    /// Read the replace string — for the drain-handler pre-fill.
    pub(crate) fn replace_str(&self) -> &str {
        &self.replace_str
    }

    /// Read `editor_flags` — for the drain-handler pre-fill.
    pub(crate) fn editor_flags(&self) -> u16 {
        self.editor_flags
    }

    /// Set `find_str` — called from `FindPick`/`ReplacePick` completion.
    pub(crate) fn set_find_str(&mut self, s: String) {
        self.find_str = s;
    }

    /// Set `replace_str` — called from `ReplacePick` completion.
    pub(crate) fn set_replace_str(&mut self, s: String) {
        self.replace_str = s;
    }

    /// Set `editor_flags` — called from `FindPick`/`ReplacePick` completion.
    pub(crate) fn set_editor_flags(&mut self, f: u16) {
        self.editor_flags = f;
    }

    // -- find/replace dialogs -----------------------------------------------

    /// One pass of the search/replace loop.
    ///
    /// The "Replace this occurrence?" prompt is asynchronous: it goes through the
    /// `request_message_box` seam rather than a blocking dialog call, and
    /// `pending_replace_answer` carries the user's choice between pump iterations.
    fn do_search_replace(&mut self, ctx: &mut Context) {
        use crate::command::Command;
        use crate::dialog::{MessageBoxButtons, MessageBoxKind};

        let opts = self.editor_flags;
        let do_replace = (opts & EF_DO_REPLACE) != 0;
        let replace_all = (opts & EF_REPLACE_ALL) != 0;
        let prompt = (opts & EF_PROMPT_ON_REPLACE) != 0;

        // `search`/`insert_text` need `&mut self`, so the find/replace strings
        // can't be borrowed across those calls — clone once up front rather than
        // per loop iteration.
        let find = self.find_str.clone();
        let replacement = self.replace_str.clone();

        // If there is a pending answer from a previous replace-prompt dialog,
        // act on it before searching for the next occurrence.
        if let Some(answer) = self.pending_replace_answer.take() {
            match answer {
                Command::YES => {
                    // Replace the current selection (still set from the last search).
                    self.insert_text(replacement.as_bytes(), false, ctx);
                    if !replace_all {
                        return;
                    }
                    // Fall through to search for the next occurrence.
                }
                Command::CANCEL => {
                    return; // user cancelled the replace loop
                }
                _ => {
                    // cmNo: C++ loop condition is `while (i != cmCancel && efReplaceAll)`,
                    // so when efReplaceAll is NOT set the loop exits on cmNo too.
                    if !replace_all {
                        return;
                    }
                    // efReplaceAll: fall through to search next (curPtr is at selEnd).
                }
            }
        }

        // Main search/replace loop (runs at least once; loops only on replace_all
        // without a prompt since the prompt path returns above).
        loop {
            if !self.search(&find, opts) {
                // Search string not found. C++ only shows the dialog when NOT
                // (replace_all && do_replace).
                if !(replace_all && do_replace) {
                    ctx.request_message_box(
                        "Search string not found.".into(),
                        MessageBoxKind::Error,
                        MessageBoxButtons::ok(),
                        None,
                        None,
                    );
                }
                return;
            }

            if do_replace {
                if prompt {
                    // Ask user — deferred async message box; answer routes back
                    // via set_modal_answer + SEARCH_AGAIN re-inject.
                    if let Some(id) = self.state.id() {
                        ctx.request_message_box(
                            "Replace this occurrence?".into(),
                            MessageBoxKind::Information,
                            MessageBoxButtons::yes_no_cancel(),
                            Some(id),                    // answer_to = this editor
                            Some(Command::SEARCH_AGAIN), // re-run after answer
                        );
                    }
                    return; // wait for answer
                } else {
                    // No prompt: replace immediately.
                    self.insert_text(replacement.as_bytes(), false, ctx);
                    if !replace_all {
                        return;
                    }
                    // Continue loop for replace_all (no prompt).
                }
            } else {
                return; // found, no replace
            }
        }
    }

    // -- line formatting / draw ---------------------------------------------

    /// The color role at offset `p`: the selected role inside the selection, else
    /// the normal role.
    fn color_at(&self, p: usize) -> Role {
        if self.sel_start <= p && p < self.sel_end {
            Role::ScrollerSelected
        } else {
            Role::ScrollerNormal
        }
    }

    /// Render one display row (the line starting at `line_ptr`) into the row at
    /// view-local `y`, honoring the horizontal scroll and width.
    fn format_line(&self, ctx: &mut DrawCtx, y: i32, line_ptr: usize, h_scroll: i32, width: i32) {
        let h_scroll = h_scroll.max(0);
        let width = width.max(0);

        let mut p = line_ptr;
        let mut pos = 0i32;
        let mut x = 0i32;
        while p < self.buf_len {
            let mut next_p = p;
            let mut next_pos = pos;
            self.next_char_and_pos(&mut next_p, &mut next_pos);

            if x > width || (x == width && pos < next_pos) {
                break;
            }

            let char_len = next_p - p;
            let chunk = self.read_chunk(p, char_len);
            if chunk.first() == Some(&b'\r') || chunk.first() == Some(&b'\n') {
                break;
            }

            if next_pos > h_scroll {
                let role = self.color_at(p);
                let style = ctx.style(role);
                let char_width = next_pos - pos.max(h_scroll);
                if chunk.first() == Some(&b'\t') || pos < h_scroll {
                    for k in 0..char_width {
                        ctx.put_char(x + k, y, ' ', style);
                    }
                } else {
                    let s = String::from_utf8_lossy(&chunk);
                    ctx.put_str(x, y, &s, style);
                }
                x += char_width;
            }

            p = next_p;
            pos = next_pos;
        }

        if x < width {
            let role = self.color_at(p);
            let style = ctx.style(role);
            for k in 0..(width - x) {
                ctx.put_char(x + k, y, ' ', style);
            }
        }
    }

    /// Render `count` rows starting at view-local `y`, from the line at
    /// `line_ptr`.
    fn draw_lines(&self, ctx: &mut DrawCtx, y: i32, count: i32, mut line_ptr: usize) {
        for yy in y..y + count {
            self.format_line(ctx, yy, line_ptr, self.delta.x, self.state.size.x);
            line_ptr = self.next_line(line_ptr);
        }
    }
}

// ---------------------------------------------------------------------------
// View impl
// ---------------------------------------------------------------------------

impl View for Editor {
    fn state(&self) -> &ViewState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }

    /// Recompute `draw_ptr` for the current vertical scroll, then render the
    /// viewport rows. `draw_ptr`/`draw_line` are display caches, so mutating them
    /// during drawing is intentional.
    fn draw(&mut self, ctx: &mut DrawCtx) {
        // Cache the absolute origin for the mouse-tracking capture, which uses it
        // to localize absolute mouse coords (the same `abs_origin` pattern
        // buttons use).
        self.abs_origin = ctx.origin();
        if self.draw_line != self.delta.y {
            self.draw_ptr = self.line_move(self.draw_ptr, self.delta.y - self.draw_line);
            self.draw_line = self.delta.y;
        }
        let count = self.state.size.y;
        let draw_ptr = self.draw_ptr;
        self.draw_lines(ctx, 0, count, draw_ptr);
    }

    /// Handle keyboard editing, command dispatch, single-click mouse positioning,
    /// and scroll-bar change notifications.
    fn handle_event(&mut self, ev: &mut crate::event::Event, ctx: &mut Context) {
        use crate::command::Command;
        use crate::event::Event;

        let center_cursor = !self.cursor_visible();

        // Extend the selection if a persistent selection is active or shift is held.
        let mut select_mode: u8 = 0;
        let shift_held = match ev {
            Event::KeyDown(k) => k.modifiers.shift,
            Event::MouseDown(m) | Event::MouseUp(m) | Event::MouseMove(m) | Event::MouseAuto(m) => {
                m.modifiers.shift
            }
            _ => false,
        };
        if self.selecting || shift_held {
            select_mode = SM_EXTEND;
        }

        // Resolve the keymap and the Ctrl-K/Ctrl-Q two-key prefix, turning a
        // KeyDown into a command in place (or clearing it for a prefix key).
        self.convert_event(ev);

        match ev {
            // -- mouse wheel --------------------------------------------------
            // During a hold the capture forwards the wheel under the loop's mask.
            // Outside a hold the editor's event mask excludes the wheel, so an
            // untracked wheel falls through unconsumed.
            Event::MouseWheel(m) => {
                let m = *m;
                match self.track {
                    // The wheel iteration of the drag-select loop: forward the
                    // wheel to both scroll bars, then run the unconditional body.
                    // The bars are siblings, so the wheel is delivered through the
                    // mouse-track deferred effect (resolve the bar, hand it the
                    // event). The bar's resulting change notification would be
                    // swallowed by the modal capture, so the editor self-posts a
                    // delta-sync instead. Both land on the next pump, so this
                    // iteration's cursor move still sees the pre-scroll `delta`;
                    // the next tick corrects it.
                    Some(EditorTrack::Select { select_mode: sm }) => {
                        self.lock();
                        if let Some(v) = self.v_scroll_bar {
                            ctx.request_mouse_track(v, Event::MouseWheel(m));
                        }
                        if let Some(h) = self.h_scroll_bar {
                            ctx.request_mouse_track(h, Event::MouseWheel(m));
                        }
                        if let Some(id) = self.state.id() {
                            ctx.request_sync_editor_delta(id, self.h_scroll_bar, self.v_scroll_bar);
                        }
                        // :580-581 — setCurPtr(getMousePtr(where),
                        // selectMode); selectMode |= smExtend.
                        let ptr = self.get_mouse_ptr(m.position);
                        self.set_cur_ptr(ptr, sm);
                        self.track = Some(EditorTrack::Select {
                            select_mode: sm | SM_EXTEND,
                        });
                        self.unlock(ctx);
                    }
                    // The pan loop's mask is evMouse (teditor1.cpp:542),
                    // which includes evMouseWheel: a wheel tick runs the
                    // same scroll-by-mouse-delta body (:543-548).
                    Some(EditorTrack::Pan { last }) => {
                        self.pan_tick(last, m.position, ctx);
                    }
                    None => return, // untracked wheel — fall through
                }
                ev.clear();
                return;
            }
            Event::MouseDown(m) => {
                let m = *m;
                if m.buttons.right {
                    use crate::event::{Key, KeyEvent, KeyModifiers};
                    use crate::menu::{Menu, popup_menu};
                    let global = m.position + self.abs_origin;
                    let menu = Menu::builder()
                        .command_key(
                            "Cu~t~",
                            Command::CUT,
                            KeyEvent::new(
                                Key::Delete,
                                KeyModifiers {
                                    shift: true,
                                    ..Default::default()
                                },
                            ),
                            "Shift-Del",
                        )
                        .command_key(
                            "~C~opy",
                            Command::COPY,
                            KeyEvent::new(
                                Key::Insert,
                                KeyModifiers {
                                    ctrl: true,
                                    ..Default::default()
                                },
                            ),
                            "Ctrl-Ins",
                        )
                        .command_key(
                            "~P~aste",
                            Command::PASTE,
                            KeyEvent::new(
                                Key::Insert,
                                KeyModifiers {
                                    shift: true,
                                    ..Default::default()
                                },
                            ),
                            "Shift-Ins",
                        )
                        .command_key(
                            "~U~ndo",
                            Command::UNDO,
                            KeyEvent::new(
                                Key::Char('u'),
                                KeyModifiers {
                                    ctrl: true,
                                    ..Default::default()
                                },
                            ),
                            "Ctrl-U",
                        )
                        .build();
                    popup_menu(global, menu, ctx.owner_size(), ctx);
                    ev.clear();
                    return;
                }
                if m.buttons.middle {
                    // Middle-button pan (teditor1.cpp:540-551): `lastMouse =
                    // makeLocal(event.mouse.where); while (mouseEvent(event,
                    // evMouse)) { … }` — a WHILE loop, so the press itself runs
                    // no body: record `lastMouse` and arm the capture with the
                    // evMouse mask (move + auto + wheel). Without an id
                    // (uninserted) there is nothing to track — the press is a
                    // no-op, like the C++ loop with no further events.
                    if let Some(id) = self.state.id() {
                        self.track = Some(EditorTrack::Pan { last: m.position });
                        ctx.start_mouse_track(
                            id,
                            self.abs_origin,
                            crate::capture::TrackMask {
                                mouse_move: true,
                                mouse_auto: true,
                                wheel: true,
                            },
                        );
                    }
                    ev.clear();
                    return;
                }
                if m.flags.double_click {
                    select_mode |= SM_DOUBLE;
                } else if m.flags.triple_click {
                    select_mode |= SM_TRIPLE;
                }
                // The first iteration of the drag-select do{}while
                // (teditor1.cpp:557-583 — the body runs once for the press):
                // lock; setCurPtr(getMousePtr(where), selectMode); selectMode
                // |= smExtend; unlock. Position is already view-local
                // (Group::deliver makeLocal'd it). Then enter the loop: arm the
                // capture with the loop mask evMouseMove + evMouseAuto +
                // evMouseWheel (:583), carrying the live selectMode in the
                // track state (the C++ loop-local survives across iterations).
                // Without an id (uninserted) the press stays single-shot.
                self.lock();
                let ptr = self.get_mouse_ptr(m.position);
                self.set_cur_ptr(ptr, select_mode);
                self.unlock(ctx);
                if let Some(id) = self.state.id() {
                    self.track = Some(EditorTrack::Select {
                        select_mode: select_mode | SM_EXTEND,
                    });
                    ctx.start_mouse_track(
                        id,
                        self.abs_origin,
                        crate::capture::TrackMask {
                            mouse_move: true,
                            mouse_auto: true,
                            wheel: true,
                        },
                    );
                }
            }
            // -- evMouseMove (tracked) — a loop-body tick. Guarded by `track`:
            // a stray move falls through unconsumed.
            Event::MouseMove(m) if self.track.is_some() => {
                let m = *m;
                match self.track {
                    // Drag-select body (teditor1.cpp:558,580-582): lock;
                    // setCurPtr(getMousePtr(where), selectMode); selectMode |=
                    // smExtend; unlock. (Neither the auto nor the wheel branch
                    // applies to a plain move.)
                    Some(EditorTrack::Select { select_mode: sm }) => {
                        self.lock();
                        let ptr = self.get_mouse_ptr(m.position);
                        self.set_cur_ptr(ptr, sm);
                        self.track = Some(EditorTrack::Select {
                            select_mode: sm | SM_EXTEND,
                        });
                        self.unlock(ctx);
                    }
                    // Pan body (teditor1.cpp:543-548).
                    Some(EditorTrack::Pan { last }) => self.pan_tick(last, m.position, ctx),
                    None => unreachable!("guarded by track.is_some()"),
                }
            }
            // -- auto-repeat (tracked) — a loop-body tick with the edge-scroll
            // prelude. Guarded by `track`.
            Event::MouseAuto(m) if self.track.is_some() => {
                let m = *m;
                match self.track {
                    // Drag-select auto body: per-axis out-of-bounds check against
                    // the view size, edge-scroll by one, THEN move the cursor.
                    // Order matters — the cursor read sees the post-scroll delta.
                    Some(EditorTrack::Select { select_mode: sm }) => {
                        self.lock();
                        let mouse = m.position;
                        let mut d = self.delta;
                        if mouse.x < 0 {
                            d.x -= 1;
                        }
                        if mouse.x >= self.state.size.x {
                            d.x += 1;
                        }
                        if mouse.y < 0 {
                            d.y -= 1;
                        }
                        if mouse.y >= self.state.size.y {
                            d.y += 1;
                        }
                        self.scroll_to(d.x, d.y);
                        let ptr = self.get_mouse_ptr(mouse);
                        self.set_cur_ptr(ptr, sm);
                        self.track = Some(EditorTrack::Select {
                            select_mode: sm | SM_EXTEND,
                        });
                        self.unlock(ctx);
                    }
                    // Pan body (teditor1.cpp:543-548): an auto at the held
                    // position has lastMouse == mouse — a no-op scroll,
                    // faithful to the C++ evMouse-masked loop.
                    Some(EditorTrack::Pan { last }) => self.pan_tick(last, m.position, ctx),
                    None => unreachable!("guarded by track.is_some()"),
                }
            }
            // -- evMouseUp (tracked) — both loops simply exit (`mouseEvent`
            // returns False on evMouseUp, tview.cpp:642; no post-loop code at
            // teditor1.cpp:551/:583-584). Guarded by `track`: MouseUp is not
            // mask-gated in Group::wants, so a stray, untracked up must fall
            // through unconsumed.
            Event::MouseUp(_) if self.track.is_some() => {
                self.track = None;
            }
            Event::KeyDown(k) => {
                let k = *k;
                // Insertable character? (printable, or tab). Faithful to the C++
                // charCode 9 / [32,255) gate, decomposed to our Key model.
                let insertable = match k.key {
                    crate::event::Key::Char(_) if !k.modifiers.ctrl && !k.modifiers.alt => true,
                    // Shift+Tab (kbShiftTab, charCode 0) is NOT insertable — it bubbles
                    // to dialog back-tab nav; only plain Tab (kbTab, charCode 9) inserts.
                    crate::event::Key::Tab
                        if !k.modifiers.shift && !k.modifiers.ctrl && !k.modifiers.alt =>
                    {
                        true
                    }
                    _ => false,
                };
                if insertable {
                    self.lock();
                    if self.overwrite
                        && !self.has_selection()
                        && self.cur_ptr != self.line_end(self.cur_ptr)
                    {
                        self.sel_end = self.next_char(self.cur_ptr);
                    }
                    let bytes: Vec<u8> = match k.key {
                        crate::event::Key::Char(c) => {
                            let mut b = [0u8; 4];
                            c.encode_utf8(&mut b).as_bytes().to_vec()
                        }
                        crate::event::Key::Tab => b"\t".to_vec(),
                        _ => unreachable!(),
                    };
                    self.insert_text_core(&bytes, false);
                    self.track_cursor(center_cursor);
                    self.unlock(ctx);
                } else {
                    return;
                }
            }
            Event::Paste(text) => {
                let text = std::mem::take(text);
                ev.clear();
                self.insert_text(text.as_bytes(), false, ctx);
            }
            Event::Command(cmd) => {
                let cmd = *cmd;
                match cmd {
                    Command::FIND => {
                        if let Some(id) = self.state.id() {
                            ctx.open_find_dialog(id);
                        }
                    }
                    Command::REPLACE => {
                        if let Some(id) = self.state.id() {
                            ctx.open_replace_dialog(id);
                        }
                    }
                    Command::SEARCH_AGAIN => {
                        self.do_search_replace(ctx);
                        self.flush_if_unlocked(ctx);
                    }
                    Command::ENCODING => {
                        self.toggle_encoding();
                        self.flush_if_unlocked(ctx);
                    }
                    _ => {
                        self.lock();
                        let handled = self.handle_edit_command(cmd, select_mode, ctx);
                        if !handled {
                            self.unlock(ctx);
                            return;
                        }
                        self.track_cursor(center_cursor);
                        self.unlock(ctx);
                    }
                }
            }
            Event::Broadcast { command, source }
                if *command == Command::SCROLL_BAR_CHANGED
                    && source.is_some()
                    && (*source == self.h_scroll_bar || *source == self.v_scroll_bar) =>
            {
                if let Some(id) = self.state.id() {
                    ctx.request_sync_editor_delta(id, self.h_scroll_bar, self.v_scroll_bar);
                }
                // Do NOT consume the scroll-bar-changed broadcast: by codebase
                // convention broadcasts are left live for sibling views (the
                // scroller behaves the same way). Functionally inert here, since
                // the broadcast only concerns this editor's own bar.
                return;
            }
            _ => return,
        }
        ev.clear();
    }

    /// Flip a state flag, then react: on a change of activity, show/hide the
    /// scroll bars and indicator and re-gray the editing commands; on focus,
    /// broadcast the focus gain/loss.
    fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
        self.state.set_flag(flag, enable);
        if flag == StateFlag::Focused {
            let source = self.state.id();
            ctx.broadcast(
                if enable {
                    crate::command::Command::RECEIVED_FOCUS
                } else {
                    crate::command::Command::RELEASED_FOCUS
                },
                source,
            );
        }
        if flag == StateFlag::Active {
            if let Some(h) = self.h_scroll_bar {
                ctx.request_set_visible(h, enable);
            }
            if let Some(v) = self.v_scroll_bar {
                ctx.request_set_visible(v, enable);
            }
            if let Some(i) = self.indicator {
                ctx.request_set_visible(i, enable);
            }
            // Re-gray commands whenever activity changes; flag a redraw so the
            // first flush publishes the scroll-bar params/indicator too.
            self.update(UF_VIEW);
            self.update_commands(ctx);
            self.flush_if_unlocked(ctx);
        }
        // There is no "exposed" flag here; the initial flush instead happens on
        // the first active/event boundary.
    }

    /// Geometry + clamp `delta` + flag a redraw. Scrollbar params republish on
    /// the next flush (mirrors the scroller).
    fn change_bounds(&mut self, bounds: Rect) {
        self.state.set_bounds(bounds);
        self.delta.x = 0.max(self.delta.x.min(self.limit.x - self.state.size.x));
        self.delta.y = 0.max(self.delta.y.min(self.limit.y - self.state.size.y));
        self.update(UF_VIEW);
    }

    /// The editor is valid when its buffer allocated successfully.
    fn valid(&mut self, _cmd: crate::command::Command, _ctx: &mut Context) -> bool {
        self.is_valid
    }

    /// Concrete-reach hatch: the pump downcasts to `&mut Editor` for the
    /// `SyncEditorDelta` / `EditorPaste` brokers, and for the `FindPick` /
    /// `ReplacePick` completion to set search state.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    /// Cache the user's answer to the "Replace this occurrence?" prompt, routed
    /// back from the message box. Consumed by [`do_search_replace`] on the next
    /// search-again dispatch.
    fn set_modal_answer(&mut self, cmd: crate::command::Command) {
        self.pending_replace_answer = Some(cmd);
    }
}

impl Editor {
    /// Translate a `KeyDown` into a `Command` (or a cleared prefix), routing
    /// through the process-global keymap.
    fn convert_event(&mut self, ev: &mut crate::event::Event) {
        use crate::event::Event;
        if let Event::KeyDown(k) = ev {
            let stroke = KeyStroke::from_event(*k);
            let pending = self.pending.take();
            match keymap::resolve_global(pending, stroke) {
                Resolve::Prefix => {
                    self.pending = Some(stroke);
                    ev.clear();
                }
                Resolve::Command(c) => {
                    *ev = Event::Command(c);
                }
                Resolve::None => {
                    // Insertable char or unhandled — leave the event unchanged.
                }
            }
        }
    }

    /// Dispatch an editing command. Returns false for an unhandled command, so
    /// the caller can fall through.
    fn handle_edit_command(
        &mut self,
        cmd: crate::command::Command,
        select_mode: u8,
        ctx: &mut Context,
    ) -> bool {
        use crate::command::Command;
        match cmd {
            Command::CUT => self.clip_cut(ctx),
            Command::COPY => {
                self.clip_copy(ctx);
            }
            Command::PASTE => self.clip_paste(ctx),
            Command::UNDO => self.undo(),
            Command::CLEAR => self.delete_select(),
            Command::CHAR_LEFT => self.set_cur_ptr(self.prev_char(self.cur_ptr), select_mode),
            Command::CHAR_RIGHT => self.set_cur_ptr(self.next_char(self.cur_ptr), select_mode),
            Command::WORD_LEFT => self.set_cur_ptr(self.prev_word(self.cur_ptr), select_mode),
            Command::WORD_RIGHT => self.set_cur_ptr(self.next_word(self.cur_ptr), select_mode),
            Command::LINE_START => {
                let p = if self.auto_indent {
                    self.indented_line_start(self.cur_ptr)
                } else {
                    self.line_start(self.cur_ptr)
                };
                self.set_cur_ptr(p, select_mode);
            }
            Command::LINE_END => self.set_cur_ptr(self.line_end(self.cur_ptr), select_mode),
            Command::LINE_UP => self.set_cur_ptr(self.line_move(self.cur_ptr, -1), select_mode),
            Command::LINE_DOWN => self.set_cur_ptr(self.line_move(self.cur_ptr, 1), select_mode),
            Command::PAGE_UP => self.set_cur_ptr(
                self.line_move(self.cur_ptr, -(self.state.size.y - 1)),
                select_mode,
            ),
            Command::PAGE_DOWN => self.set_cur_ptr(
                self.line_move(self.cur_ptr, self.state.size.y - 1),
                select_mode,
            ),
            Command::TEXT_START => self.set_cur_ptr(0, select_mode),
            Command::TEXT_END => self.set_cur_ptr(self.buf_len, select_mode),
            Command::NEW_LINE => self.new_line(),
            Command::BACK_SPACE => {
                self.delete_range(self.prev_char(self.cur_ptr), self.cur_ptr, true)
            }
            Command::DEL_CHAR => {
                self.delete_range(self.cur_ptr, self.next_char(self.cur_ptr), true)
            }
            Command::DEL_WORD => {
                self.delete_range(self.cur_ptr, self.next_word(self.cur_ptr), false)
            }
            Command::DEL_WORD_LEFT => {
                self.delete_range(self.prev_word(self.cur_ptr), self.cur_ptr, false)
            }
            Command::DEL_START => {
                self.delete_range(self.line_start(self.cur_ptr), self.cur_ptr, false)
            }
            Command::DEL_END => self.delete_range(self.cur_ptr, self.line_end(self.cur_ptr), false),
            Command::DEL_LINE => self.delete_range(
                self.line_start(self.cur_ptr),
                self.next_line(self.cur_ptr),
                false,
            ),
            Command::INS_MODE => self.toggle_ins_mode(),
            Command::START_SELECT => self.start_select(),
            Command::HIDE_SELECT => self.hide_select(),
            Command::INDENT_MODE => self.auto_indent = !self.auto_indent,
            Command::SELECT_ALL => {
                self.set_cur_ptr(0, select_mode);
                self.set_cur_ptr(self.buf_len, select_mode | SM_EXTEND);
            }
            _ => return false,
        }
        true
    }
}

// ---------------------------------------------------------------------------
// Memo — a thin embed-delegate wrapper over Editor.
// ---------------------------------------------------------------------------

/// A single-field multi-line editor for use inside a dialog. An
/// embed-and-delegate wrapper over [`Editor`]; the only behavioural differences
/// from the base editor are: it swallows a plain `Tab` keypress (so Tab navigates
/// the dialog rather than inserting), and it exposes its text as a typed
/// [`FieldValue`] for dialog gather/scatter.
///
/// A memo reuses the editor's drawing and so its scroller colors; it carries no
/// separate palette of its own.
///
/// # Turbo Vision heritage
///
/// Ports `TMemo` (`tmemo.cpp`). Data exchange becomes the typed value protocol
/// (deviation D10) and the palette is folded into the editor's colors.
pub struct Memo {
    /// The shared editor engine (buffer, nav, edit, undo, draw, brokers).
    pub editor: Editor,
}

impl Memo {
    /// Build a memo over a `buf_size`-byte buffer, wired to the given scroll bars
    /// and frame indicator. Forwards straight to the [`Editor`] constructor.
    pub fn new(
        bounds: Rect,
        h_scroll_bar: Option<ViewId>,
        v_scroll_bar: Option<ViewId>,
        indicator: Option<ViewId>,
        buf_size: usize,
    ) -> Self {
        Memo {
            editor: Editor::new(bounds, h_scroll_bar, v_scroll_bar, indicator, buf_size),
        }
    }
}

#[crate::delegate(to = editor)]
impl View for Memo {
    /// Swallow a plain `Tab` KeyDown (so it bubbles to the dialog's focus
    /// navigation) and forward everything else to the editor.
    fn handle_event(&mut self, ev: &mut crate::event::Event, ctx: &mut Context) {
        use crate::event::{Event, Key};
        // Shift/Ctrl/Alt+Tab ARE forwarded; only the unmodified Tab is swallowed.
        if let Event::KeyDown(k) = ev
            && k.key == Key::Tab
            && !k.modifiers.shift
            && !k.modifiers.ctrl
            && !k.modifiers.alt
        {
            // Swallow WITHOUT clearing — let it propagate to dialog focus-nav.
            return;
        }
        self.editor.handle_event(ev, ctx);
    }

    /// The memo's text as a typed [`FieldValue`].
    fn value(&self) -> Option<crate::data::FieldValue> {
        Some(crate::data::FieldValue::Text(
            String::from_utf8_lossy(&self.editor.text()).into_owned(),
        ))
    }

    /// Load text into the buffer. Ignores a non-`Text` variant (a type mismatch
    /// the typed model drops, like `InputLine::set_value`).
    fn set_value(&mut self, v: crate::data::FieldValue) {
        if let crate::data::FieldValue::Text(s) = v {
            self.editor.set_text(s.as_bytes());
        }
    }
}

// ---------------------------------------------------------------------------
// FileEditor — a file-editor-mode Editor.
// ---------------------------------------------------------------------------

/// An [`Editor`] backed by a file on disk. An embed-and-delegate wrapper; the
/// editor runs in file-editor mode (growable buffer plus save commands).
///
/// It loads and saves over real `std::fs`, grows the buffer to fit a file, and
/// handles the save commands. Validation runs the full modified-save
/// Yes/No/Cancel prompt and a write-error popup, both through the
/// async-modal-from-a-view mechanism: `valid` requests a message box, caches the
/// answer in [`set_modal_answer`](View::set_modal_answer), then re-validates.
///
/// Save-as (an explicit Save-As command, or saving an untitled buffer) opens a
/// file dialog through [`Context::request_save_as_dialog`]: the loop builds and
/// runs the dialog, the completion sets `file_name` plus [`pending_title_update`]
/// and re-injects a Save command, which writes the file and broadcasts a
/// title-update (refreshing the hosting [`EditWindow`]'s frame title).
///
/// A read error is reported through a message box on the first event (the
/// constructor has no [`Context`], so the error is cached at load time).
/// Optional backup files (off by default) append `~` to the filename
/// (`foo.txt` → `foo.txt~`), following the Unix convention.
///
/// A couple of details collapse on a modern host: `std::fs::write` does not
/// distinguish create-vs-write failure, so both report one "Error writing file …"
/// box, and buffer growth is infallible, so there is no out-of-memory path.
///
/// # Turbo Vision heritage
///
/// Ports `TFileEditor` (`tfiledtr.cpp`). Data exchange uses the typed value
/// protocol (deviation D10); serialization is dropped.
pub struct FileEditor {
    /// The editor engine, in file-editor mode.
    pub editor: Editor,
    /// The backing file, or `None` for an untitled buffer.
    pub file_name: Option<std::path::PathBuf>,
    /// The user's answer to the modified-save prompt, cached by
    /// [`View::set_modal_answer`] between the `valid()` that requested the box and
    /// the re-validate that consumes it (the async-modal-from-a-view round-trip).
    /// `None` until an answer is routed back; consumed (taken) on the next `valid`.
    pending_save_answer: Option<crate::command::Command>,
    /// Set by the save-as dialog completion (after the user picks a filename) so
    /// the next successful save broadcasts a title-update to refresh the hosting
    /// [`EditWindow`]'s frame title. The broadcast is deferred to the re-injected
    /// save so it fires from a full [`Context`].
    pub pending_title_update: bool,
    /// Error message from `load_file` to display on the first `handle_event` call.
    /// Set when a real I/O error (not NotFound) occurs during `load_file`, which is
    /// called from the constructor and has no [`Context`]. Shown via
    /// `request_message_box` on the first `handle_event` (the established pattern
    /// for work deferred out of a constructor).
    pending_load_error: Option<String>,
}

impl FileEditor {
    /// Build a file editor wired to the given scroll bars and frame indicator.
    /// A `None` `file_name` is untitled; a given one is loaded immediately, and
    /// `is_valid` reflects whether the load succeeded.
    pub fn new(
        bounds: Rect,
        h_scroll_bar: Option<ViewId>,
        v_scroll_bar: Option<ViewId>,
        indicator: Option<ViewId>,
        file_name: Option<std::path::PathBuf>,
    ) -> Self {
        let mut fe = FileEditor {
            editor: Editor::new_file_editor(bounds, h_scroll_bar, v_scroll_bar, indicator),
            file_name: None,
            pending_save_answer: None,
            pending_title_update: false,
            pending_load_error: None,
        };
        if let Some(path) = file_name {
            // Make the path absolute relative to the CWD, resolving . and .., but
            // do NOT require it to exist. std::path::absolute matches this
            // contract (stable since Rust 1.79); fall back to the original path if
            // the CWD cannot be determined (the only failure mode).
            let path = std::path::absolute(&path).unwrap_or(path);
            fe.file_name = Some(path);
            if fe.editor.is_valid {
                fe.editor.is_valid = fe.load_file();
            }
        }
        fe
    }

    /// Read the whole file into the buffer.
    /// Missing/unopenable file ⇒ empty buffer, success.
    /// A real read error ⇒ false; stores `pending_load_error` for display on first `handle_event`.
    pub fn load_file(&mut self) -> bool {
        let Some(path) = self.file_name.clone() else {
            self.editor.set_buf_len(0);
            return true;
        };
        match std::fs::read(&path) {
            Ok(bytes) => {
                // set_text grows the buffer to fit and places the bytes at the end.
                self.editor.set_text(&bytes);
                true
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.editor.set_buf_len(0); // can't open ⇒ empty, valid (C++ True)
                true
            }
            Err(e) => {
                // Store the read error for display on first handle_event (the
                // constructor has no ctx). OS detail is omitted because the dialog
                // sizer doesn't handle embedded newlines.
                let _ = e;
                let msg = format!("Error reading file {}.", path.display());
                self.pending_load_error = Some(msg);
                false
            }
        }
    }

    /// Write the buffer's logical text to `file_name`. On a write failure, pops an
    /// error message box ([`Context::request_message_box`]).
    pub fn save_file(&mut self, ctx: &mut Context) -> bool {
        let Some(path) = self.file_name.clone() else {
            return false;
        };
        // Backup-files option: rename the existing file to <name>~ before writing
        // the new content, appending "~" to the full filename (Unix convention).
        // No filename component: silently skip.
        if (self.editor.editor_flags() & EF_BACKUP_FILES) != 0
            && let Some(name) = path.file_name()
        {
            let mut bname = name.to_os_string();
            bname.push("~");
            let backup = path.with_file_name(bname);
            let _ = std::fs::remove_file(&backup); // Windows: rename fails if dest exists
            let _ = std::fs::rename(&path, &backup); // ignore — original may not exist (first save)
        }
        // The logical text skips the gap; the editor's text() helper does that.
        let bytes = self.editor.text();
        match std::fs::write(&path, &bytes) {
            Ok(()) => {
                self.editor.clear_modified();
                true
            }
            Err(_) => {
                // A failed write reports one "Error writing file …" box. `std::fs::write`
                // opens and writes atomically, so a create failure and a write failure
                // cannot be distinguished here.
                let msg = format!("Error writing file {}.", path.display());
                ctx.request_message_box(
                    msg,
                    crate::dialog::MessageBoxKind::Error,
                    crate::dialog::MessageBoxButtons::ok(),
                    None,
                    None,
                );
                false
            }
        }
    }

    /// Save to the existing file, or — if untitled — start a save-as.
    ///
    /// The untitled branch is asynchronous: a leaf view holds only `&mut Context`
    /// and cannot run a nested modal dialog inline, so it **requests** the save-as
    /// dialog ([`Context::request_save_as_dialog`]) and returns `false` for now.
    /// The real save happens after the dialog closes: the completion sets
    /// `file_name` and re-injects a Save command, which re-runs this method — now
    /// with a non-empty `file_name`, so it writes the file.
    ///
    /// For the modal-close path, `validate_modal_close` drives the save-as dialog
    /// inline and pumps once to service the re-injected Save.
    pub fn save(&mut self, ctx: &mut Context) -> bool {
        if self.file_name.is_some() {
            self.save_file(ctx)
        } else {
            if let Some(id) = View::state(self).id() {
                ctx.request_save_as_dialog(id);
            }
            false
        }
    }
}

#[crate::delegate(to = editor)]
impl View for FileEditor {
    /// Concrete-reach hatch: the pump's save-as brokers and
    /// `EditWindow::handle_event` downcast a group child back to `&mut FileEditor`
    /// to set `file_name` / read `pending_title_update`. WITHOUT this override the
    /// `#[delegate(to = editor)]` macro would forward `as_any_mut` to the inner
    /// [`Editor`], so the downcast would silently miss (returning the inner
    /// editor's `Any`).
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    /// Run the editor first, then handle the Save / Save-As commands.
    fn handle_event(&mut self, ev: &mut crate::event::Event, ctx: &mut Context) {
        use crate::command::Command;
        use crate::event::Event;
        if let Some(msg) = self.pending_load_error.take() {
            ctx.request_message_box(
                msg,
                crate::dialog::MessageBoxKind::Error,
                crate::dialog::MessageBoxButtons::ok(),
                None,
                None,
            );
        }
        self.editor.handle_event(ev, ctx);
        // Save-As: always open the file dialog to pick a (possibly new) name,
        // regardless of whether the buffer is titled. The completion sets the
        // picked name and re-injects a Save.
        if let Event::Command(cmd) = ev
            && *cmd == Command::SAVE_AS
        {
            if let Some(id) = View::state(self).id() {
                ctx.request_save_as_dialog(id);
            }
            ev.clear();
            return; // do NOT fall through to the cmSave arm below.
        }
        if let Event::Command(cmd) = ev
            && *cmd == Command::SAVE
        {
            let ok = self.save(ctx);
            // Publish modified=false to the indicator and re-gray the save commands
            // now (the inner editor already flushed with modified still true, since
            // a Save is not itself an edit).
            self.editor.flush_if_unlocked(ctx);
            // After a successful save-as (the completion set the flag and re-injected
            // this Save), broadcast a title-update so the hosting EditWindow
            // refreshes its frame title.
            if ok && self.pending_title_update {
                self.pending_title_update = false;
                ctx.broadcast(Command::UPDATE_TITLE, View::state(self).id());
            }
            ev.clear();
        }
    }

    /// Validity check. For the plain validity command this reflects buffer
    /// validity; for any other command, a **modified** buffer first prompts a
    /// Yes/No/Cancel save dialog, and the answer decides the result: Yes → save,
    /// No → clear the dirty flag and allow, Cancel → veto.
    ///
    /// The prompt is asynchronous: the first call requests the box
    /// ([`Context::request_message_box`], routing the answer back to this view and
    /// re-posting the close command) and returns `false` (veto for now). When the
    /// user picks, the pump routes the answer back through
    /// [`View::set_modal_answer`] (caching it in `pending_save_answer`) and
    /// re-posts the close, which re-runs this check; this time the cached choice is
    /// applied.
    fn valid(&mut self, cmd: crate::command::Command, ctx: &mut Context) -> bool {
        use crate::command::Command;
        if cmd == Command::VALID {
            return self.editor.valid(cmd, ctx);
        }
        // Re-validate pass: consume the cached answer from the async box.
        if let Some(answer) = self.pending_save_answer.take() {
            return match answer {
                Command::YES => self.save(ctx),
                Command::NO => {
                    self.editor.clear_modified();
                    true
                }
                // Cancel (or anything else, e.g. an OK-only box) → veto the close.
                _ => false,
            };
        }
        // First pass: a modified buffer needs the save prompt.
        if self.editor.modified() {
            // self id = self.state().id() (the state delegates to the inner editor,
            // so this is the id the owning group stored for the FileEditor box). The
            // close path always runs against an inserted tree; if somehow absent,
            // fall back to allow-close (cannot route an answer with no id).
            let Some(my_id) = View::state(self).id() else {
                return true;
            };
            // edSaveUntitled / edSaveModify (msgbox uses Information per editorDialog).
            let msg = match &self.file_name {
                None => "Save untitled file?".to_string(),
                Some(p) => format!("{} has been modified. Save?", p.display()),
            };
            ctx.request_message_box(
                msg,
                crate::dialog::MessageBoxKind::Information,
                crate::dialog::MessageBoxButtons::yes_no_cancel(),
                Some(my_id),
                Some(Command::CLOSE),
            );
            return false; // veto until the answer comes back and re-validates.
        }
        true
    }

    /// Cache the user's Yes/No/Cancel choice from the modified-save box for the
    /// re-validate pass (the async-modal-from-a-view round-trip).
    fn set_modal_answer(&mut self, cmd: crate::command::Command) {
        self.pending_save_answer = Some(cmd);
    }
}

/// Reach the inner [`Editor`] engine of a group child that may be a plain
/// [`Editor`], a [`Memo`], or a [`FileEditor`].
///
/// `FileEditor::as_any_mut` returns the `FileEditor` itself (so the save-as
/// brokers can downcast to it), NOT the inner `Editor`. The editor cross-view brokers
/// (`SyncEditorDelta` / `EditorPaste`) target the inserted view's id — which, in an
/// `EditWindow`, IS a `FileEditor` — yet they need the inner `Editor`. This helper
/// bridges that: it tries the `FileEditor` downcast first (peeling to its
/// `.editor`), and otherwise falls back to a direct `Editor` downcast (covering a
/// plain `Editor` and a `Memo`, both of whose `as_any_mut` forward to the inner
/// `Editor`). The `is::<>()`-first form avoids the NLL conditional-borrow error.
pub(crate) fn editor_mut(v: &mut dyn View) -> Option<&mut Editor> {
    let any = v.as_any_mut()?;
    if any.is::<FileEditor>() {
        return any.downcast_mut::<FileEditor>().map(|fe| &mut fe.editor);
    }
    any.downcast_mut::<Editor>()
}

// ---------------------------------------------------------------------------
// EditWindow
// ---------------------------------------------------------------------------

/// A [`Window`] hosting a [`FileEditor`] with two scroll bars and an
/// [`Indicator`]. An embed-and-delegate wrapper over [`Window`]; the constructor
/// wires the editor to the (initially hidden) scroll bars and indicator by id.
///
/// When a close is requested while this window hosts the internal clipboard
/// editor, the window hides instead of closing. The clipboard editor is never
/// hosted in an `EditWindow` in practice, so closing is normally the plain window
/// close.
///
/// # Turbo Vision heritage
///
/// Ports `TEditWindow` (`teditwnd.cpp`).
pub struct EditWindow {
    /// The embedded window (frame + children).
    pub window: crate::window::Window,
    /// The inserted [`FileEditor`]'s id (for reachability / tests).
    pub editor_id: ViewId,
    /// The (initially hidden) horizontal scroll bar.
    pub h_scroll_bar_id: ViewId,
    /// The (initially hidden) vertical scroll bar.
    pub v_scroll_bar_id: ViewId,
    /// The (initially hidden) indicator.
    pub indicator_id: ViewId,
}

impl EditWindow {
    /// Build an edit window hosting `file_name` (or untitled) with window number
    /// `number`.
    ///
    /// Inserts the (hidden) scroll bars and indicator FIRST to obtain their
    /// [`ViewId`]s, then constructs the [`FileEditor`] wired to those ids, then
    /// inserts the editor. The title is the filename, or "Untitled".
    pub fn new(bounds: Rect, file_name: Option<std::path::PathBuf>, number: i16) -> Self {
        // Title: filename or "Untitled".
        let title = match &file_name {
            Some(p) => p.to_string_lossy().into_owned(),
            None => "Untitled".to_string(),
        };
        let mut window = crate::window::Window::new(bounds, Some(title), number);

        // Edit windows participate in tiling.
        View::state_mut(&mut window).options.tileable = true;

        let size = View::state(&window).size;

        // Horizontal scrollbar — hidden, row-bottom, columns 18..size.x-2.
        let mut h = ScrollBar::new(Rect::new(18, size.y - 1, size.x - 2, size.y));
        h.state.hide();
        let h_scroll_bar_id = window.insert_child(Box::new(h));

        // Vertical scrollbar — hidden, right column, rows 1..size.y-1.
        let mut v = ScrollBar::new(Rect::new(size.x - 1, 1, size.x, size.y - 1));
        v.state.hide();
        let v_scroll_bar_id = window.insert_child(Box::new(v));

        // Indicator — hidden, row-bottom, columns 2..16.
        let mut ind = Indicator::new(Rect::new(2, size.y - 1, 16, size.y));
        ind.state.hide();
        let indicator_id = window.insert_child(Box::new(ind));

        // FileEditor over the inner extent, wired to the three ids.
        let mut r = View::state(&window).get_extent();
        r.grow(-1, -1);
        let editor = FileEditor::new(
            r,
            Some(h_scroll_bar_id),
            Some(v_scroll_bar_id),
            Some(indicator_id),
            file_name,
        );
        let editor_id = window.insert_child(Box::new(editor));

        EditWindow {
            window,
            editor_id,
            h_scroll_bar_id,
            v_scroll_bar_id,
            indicator_id,
        }
    }
}

#[crate::delegate(
    to = window,
    skip(
        apply_list_scroll,
        as_any_mut,
        calc_bounds,
        grabs_focus_on_click,
        select_window_num,
        set_value,
        size_limits,
        value
    )
)]
impl View for EditWindow {
    /// Run the window first, then refresh the frame title on a title-update
    /// broadcast (the hosted [`FileEditor`] fires it after a save-as rename).
    ///
    /// The title is stored, so it is recomputed from the editor's current
    /// `file_name` (or "Untitled") and re-pushed to the frame via
    /// [`Window::set_title`](crate::window::Window).
    ///
    /// The event is left live (not consumed). The title-update is a broadcast that
    /// fans out to every window; consuming it on the first window dispatched would
    /// starve the others. Instead, every edit window refreshes its own title from
    /// its own editor — idempotent for windows that did not save, and
    /// order-independent — so the broadcast can safely reach them all.
    fn handle_event(&mut self, ev: &mut crate::event::Event, ctx: &mut crate::view::Context) {
        use crate::command::Command;
        use crate::event::Event;
        // Hide instead of close when this window hosts the clipboard editor.
        if let Event::Command(ref cmd) = *ev
            && *cmd == Command::CLOSE
            && ctx.clipboard_editor_id() == Some(self.editor_id)
            && let Some(id) = crate::view::View::state(self).id()
        {
            ctx.request_set_visible(id, false);
            ev.clear();
            return;
        }
        self.window.handle_event(ev, ctx);
        if let Event::Broadcast { command, .. } = ev
            && *command == Command::UPDATE_TITLE
        {
            let title = self
                .window
                .child_mut(self.editor_id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<FileEditor>())
                .map(|fe| match &fe.file_name {
                    Some(p) => p.to_string_lossy().into_owned(),
                    None => "Untitled".to_string(),
                });
            if let Some(t) = title {
                self.window.set_title(Some(t));
            }
        }
    }

    /// The minimum window size is 24×6. `calc_bounds` is in the skip list so a
    /// parent-driven resize routes through this override (via the trait default of
    /// `calc_bounds`) instead of the window's 16×6 floor.
    fn size_limits(&self, owner_size: Point) -> (Point, Point) {
        let (_min, max) = View::size_limits(&self.window, owner_size);
        (Point::new(24, 6), max)
    }
}

// ---------------------------------------------------------------------------
// Tests — the real oracle is logical buffer state (not just snapshots).
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::event::Event;
    use crate::view::{Deferred, Group};
    use std::collections::VecDeque;

    /// Build an editor with an LF default line ending (the host default) and a
    /// generous buffer.
    fn ed() -> Editor {
        Editor::new(Rect::new(0, 0, 40, 10), None, None, None, 1024)
    }

    /// A throwaway Context for driving the ctx-taking methods in tests.
    struct Cx {
        out: VecDeque<Event>,
        timers: crate::timer::TimerQueue,
        deferred: Vec<Deferred>,
    }
    impl Cx {
        fn new() -> Self {
            Cx {
                out: VecDeque::new(),
                timers: crate::timer::TimerQueue::new(),
                deferred: vec![],
            }
        }
        fn ctx(&mut self) -> Context<'_> {
            Context::new(&mut self.out, &mut self.timers, 0, &mut self.deferred)
        }
    }

    /// Insert `s` at the cursor via the ctx-free core (the common test verb).
    fn insert(e: &mut Editor, s: &str) {
        e.insert_text_core(s.as_bytes(), false);
    }

    /// The buffer invariant that must hold after every op.
    fn check_invariant(e: &Editor) {
        assert_eq!(
            e.buf_len + e.gap_len,
            e.buf_size,
            "buf_len + gap_len == buf_size"
        );
    }

    fn text(e: &Editor) -> String {
        String::from_utf8(e.text()).unwrap()
    }

    // -- ctor ----------------------------------------------------------------

    #[test]
    fn ctor_defaults() {
        let e = ed();
        assert!(e.state.options.selectable);
        assert!(e.state.grow_mode.hi_x && e.state.grow_mode.hi_y);
        assert!(e.state.state.cursor_vis, "showCursor sets cursor_vis");
        assert_eq!(e.buf_len, 0);
        assert_eq!(e.gap_len, e.buf_size);
        assert_eq!(e.limit, Point::new(MAX_LINE_LENGTH, 1));
        assert!(e.is_valid);
        check_invariant(&e);
    }

    // -- (a) insert spanning the gap -----------------------------------------

    #[test]
    fn insert_basic_and_across_gap() {
        let mut e = ed();
        insert(&mut e, "hello");
        assert_eq!(text(&e), "hello");
        assert_eq!(e.cur_ptr, 5);
        assert_eq!(e.buf_len, 5);
        check_invariant(&e);

        // Move cursor to start, then insert — exercises the gap memmove on the
        // physical buffer (text after the gap).
        e.set_cur_ptr(0, 0);
        assert_eq!(e.cur_ptr, 0);
        insert(&mut e, "XY");
        assert_eq!(text(&e), "XYhello");
        assert_eq!(e.cur_ptr, 2);
        check_invariant(&e);
    }

    // -- (b) insert, move left (setSelect gap move), insert again ------------

    #[test]
    fn insert_move_left_insert_again() {
        let mut e = ed();
        insert(&mut e, "abcdef");
        e.set_cur_ptr(3, 0); // gap moves to between 'c' and 'd'
        assert_eq!(e.cur_ptr, 3);
        check_invariant(&e);
        insert(&mut e, "--");
        assert_eq!(text(&e), "abc--def");
        assert_eq!(e.cur_ptr, 5);
        check_invariant(&e);
    }

    // -- (c) delete-select ---------------------------------------------------

    #[test]
    fn delete_select_removes_range() {
        let mut e = ed();
        insert(&mut e, "hello world");
        // Select "hello " (0..6): cursor at 6, anchor at 0.
        e.set_select(0, 6, false); // cur at selEnd=6
        assert!(e.has_selection());
        e.delete_select();
        assert_eq!(text(&e), "world");
        assert!(!e.has_selection());
        check_invariant(&e);
    }

    // -- (d) newLine with autoIndent -----------------------------------------

    #[test]
    fn new_line_with_auto_indent_replicates_leading_ws() {
        let mut e = ed();
        insert(&mut e, "    code");
        e.new_line();
        assert_eq!(text(&e), "    code\n    ");
        assert_eq!(e.cur_pos.x, 4, "cursor after the replicated indent");
        check_invariant(&e);
    }

    #[test]
    fn new_line_no_auto_indent() {
        let mut e = ed();
        e.auto_indent = false;
        insert(&mut e, "    code");
        e.new_line();
        assert_eq!(text(&e), "    code\n");
        check_invariant(&e);
    }

    // -- (e) search hit + whole-word reject ----------------------------------

    #[test]
    fn search_finds_from_cursor() {
        let mut e = ed();
        insert(&mut e, "the cat sat on the mat");
        e.set_cur_ptr(0, 0);
        assert!(e.search("cat", 0));
        assert_eq!(e.sel_start, 4);
        assert_eq!(e.sel_end, 7);
    }

    #[test]
    fn search_case_insensitive_default() {
        let mut e = ed();
        insert(&mut e, "Hello World");
        e.set_cur_ptr(0, 0);
        assert!(e.search("world", 0), "default search is case-insensitive");
        assert_eq!(e.sel_start, 6);
    }

    #[test]
    fn search_case_sensitive_rejects_wrong_case() {
        let mut e = ed();
        insert(&mut e, "Hello World");
        e.set_cur_ptr(0, 0);
        assert!(!e.search("world", EF_CASE_SENSITIVE));
    }

    #[test]
    fn search_whole_word_rejects_substring() {
        let mut e = ed();
        insert(&mut e, "category cat");
        e.set_cur_ptr(0, 0);
        // Whole-word "cat" must skip "category" and match the standalone "cat".
        assert!(e.search("cat", EF_WHOLE_WORDS_ONLY));
        assert_eq!(e.sel_start, 9, "matched the standalone 'cat' at 9");
    }

    #[test]
    fn search_whole_word_no_match() {
        let mut e = ed();
        insert(&mut e, "category");
        e.set_cur_ptr(0, 0);
        assert!(!e.search("cat", EF_WHOLE_WORDS_ONLY));
    }

    // -- (f) undo after insert restores prior text & cursor ------------------

    #[test]
    fn undo_after_insert_removes_it() {
        let mut e = ed();
        insert(&mut e, "abc");
        // A real cursor move establishes an undo checkpoint (setSelect's gap move
        // zeroes the ins/del counts), so the next insert is the only thing undo
        // reverts. (A no-op move does NOT checkpoint — faithful to C++.)
        e.set_cur_ptr(0, 0);
        let before_cur = e.cur_ptr;
        e.insert_text_core(b"XYZ", false);
        assert_eq!(text(&e), "XYZabc");
        e.undo();
        assert_eq!(text(&e), "abc", "undo removes only the last insert run");
        assert_eq!(e.cur_ptr, before_cur, "cursor restored to before insert");
        check_invariant(&e);
    }

    /// The highest-bug-density path: deleting pre-existing text selected FORWARD
    /// (cursor at sel_end, `del_len > 0`) hits the undo memmove + the `dst`
    /// underflow arithmetic in insert_buffer. The most ordinary edit there is.
    #[test]
    fn undo_after_forward_delete_of_existing_text() {
        let mut e = ed();
        insert(&mut e, "hello world");
        e.set_cur_ptr(0, 0); // checkpoint: ins_count → 0
        e.set_select(0, 6, false); // cursor at sel_end=6, selects "hello "
        e.delete_select(); // del_len = 6 - 0 = 6 → hits the memmove
        assert_eq!(text(&e), "world");
        check_invariant(&e);
        e.undo();
        assert_eq!(
            text(&e),
            "hello world",
            "undo restores the forward-deleted text"
        );
        check_invariant(&e);
    }

    #[test]
    fn undo_after_delete_restores_text() {
        let mut e = ed();
        insert(&mut e, "hello");
        // Delete "ello" via backspace-like range delete with undo.
        e.set_select(1, 5, true); // cur at selStart=1
        e.delete_select();
        assert_eq!(text(&e), "h");
        e.undo();
        assert_eq!(text(&e), "hello", "undo restores the deleted text");
        check_invariant(&e);
    }

    // -- (g) backspace / delChar / delWord / delLine ranges ------------------

    #[test]
    fn backspace_deletes_prev_char() {
        let mut e = ed();
        insert(&mut e, "abc");
        e.delete_range(e.prev_char(e.cur_ptr), e.cur_ptr, true);
        assert_eq!(text(&e), "ab");
        check_invariant(&e);
    }

    #[test]
    fn del_char_deletes_next() {
        let mut e = ed();
        insert(&mut e, "abc");
        e.set_cur_ptr(0, 0);
        e.delete_range(e.cur_ptr, e.next_char(e.cur_ptr), true);
        assert_eq!(text(&e), "bc");
        check_invariant(&e);
    }

    #[test]
    fn del_word_deletes_to_next_word() {
        let mut e = ed();
        insert(&mut e, "foo bar baz");
        e.set_cur_ptr(0, 0);
        // delWord deletes up to the next word boundary (the space after "foo").
        e.delete_range(e.cur_ptr, e.next_word(e.cur_ptr), false);
        assert_eq!(text(&e), " bar baz");
        check_invariant(&e);
    }

    #[test]
    fn del_line_deletes_whole_line() {
        let mut e = ed();
        insert(&mut e, "line1\nline2\nline3");
        // Put the cursor on line2.
        e.set_cur_ptr(8, 0);
        e.delete_range(e.line_start(e.cur_ptr), e.next_line(e.cur_ptr), false);
        assert_eq!(text(&e), "line1\nline3");
        check_invariant(&e);
    }

    #[test]
    fn del_start_deletes_to_line_start() {
        let mut e = ed();
        insert(&mut e, "abcdef");
        e.set_cur_ptr(3, 0);
        // cmDelStart: deleteRange(lineStart, curPtr, False).
        e.delete_range(e.line_start(e.cur_ptr), e.cur_ptr, false);
        assert_eq!(text(&e), "def");
        check_invariant(&e);
    }

    #[test]
    fn del_end_deletes_to_line_end() {
        let mut e = ed();
        insert(&mut e, "abcdef");
        e.set_cur_ptr(3, 0);
        // cmDelEnd: deleteRange(curPtr, lineEnd, False).
        e.delete_range(e.cur_ptr, e.line_end(e.cur_ptr), false);
        assert_eq!(text(&e), "abc");
        check_invariant(&e);
    }

    // -- line endings --------------------------------------------------------

    #[test]
    fn insert_converts_line_endings_to_lf() {
        let mut e = ed(); // default LF
        e.insert_text_core(b"a\r\nb\rc\n", false);
        assert_eq!(text(&e), "a\nb\nc\n");
        check_invariant(&e);
    }

    #[test]
    fn crlf_editor_converts_to_crlf() {
        let mut e = Editor::new(Rect::new(0, 0, 40, 10), None, None, None, 1024);
        e.line_ending = LineEnding::CrLf;
        e.insert_text_core(b"a\nb", false);
        assert_eq!(text(&e), "a\r\nb");
        check_invariant(&e);
    }

    #[test]
    fn limit_y_counts_lines() {
        let mut e = ed();
        insert(&mut e, "one\ntwo\nthree");
        assert_eq!(e.limit.y, 3, "3 lines → limit.y = 2 breaks + 1");
        check_invariant(&e);
    }

    // -- navigation ----------------------------------------------------------

    #[test]
    fn line_move_preserves_column() {
        let mut e = ed();
        insert(&mut e, "abcdef\nghijkl\nmnopqr");
        // Cursor at column 3 of last line.
        e.set_cur_ptr(0, 0);
        e.set_cur_ptr(3, 0); // col 3 line 0
        assert_eq!(e.cur_pos, Point::new(3, 0));
        let p = e.line_move(e.cur_ptr, 1);
        e.set_cur_ptr(p, 0);
        assert_eq!(e.cur_pos.y, 1);
        assert_eq!(e.cur_pos.x, 3, "column preserved across line move");
    }

    #[test]
    fn word_navigation() {
        let mut e = ed();
        insert(&mut e, "foo bar");
        e.set_cur_ptr(0, 0);
        // nextWord stops at the first word boundary — the space after "foo" (3).
        assert_eq!(e.next_word(0), 3, "next word boundary is the space at 3");
        // prevWord from end → start of "bar" (4).
        assert_eq!(e.prev_word(7), 4, "prev word from end → 'bar' start");
    }

    // -- toggleInsMode / encoding -------------------------------------------

    #[test]
    fn toggle_ins_mode_flips_overwrite_and_cursor() {
        let mut e = ed();
        assert!(!e.overwrite);
        e.toggle_ins_mode();
        assert!(e.overwrite);
        assert!(e.state.state.cursor_ins);
        e.toggle_ins_mode();
        assert!(!e.overwrite);
        assert!(!e.state.state.cursor_ins);
    }

    // -- convertEvent keymap -------------------------------------------------

    fn key(k: crate::event::Key) -> crate::event::KeyEvent {
        crate::event::KeyEvent::from(k)
    }
    fn ctrl(c: char) -> crate::event::KeyEvent {
        crate::event::KeyEvent::new(
            crate::event::Key::Char(c),
            crate::event::KeyModifiers {
                ctrl: true,
                ..Default::default()
            },
        )
    }

    #[test]
    fn keymap_arrows_and_named() {
        use crate::event::Key;
        use crate::keymap::{KeyStroke, Keymap, Resolve};
        let km = Keymap::word_star();
        let r = |k| km.resolve(None, KeyStroke::from_event(key(k)));
        assert!(matches!(r(Key::Left),     Resolve::Command(c) if c == Command::CHAR_LEFT));
        assert!(matches!(r(Key::Home),     Resolve::Command(c) if c == Command::LINE_START));
        assert!(matches!(r(Key::PageDown), Resolve::Command(c) if c == Command::PAGE_DOWN));
        assert!(matches!(r(Key::Enter),    Resolve::Command(c) if c == Command::NEW_LINE));
        assert!(matches!(r(Key::Delete),   Resolve::Command(c) if c == Command::DEL_CHAR));
    }

    #[test]
    fn keymap_ctrl_letters() {
        use crate::keymap::{KeyStroke, Keymap, Resolve};
        let km = Keymap::word_star();
        let r = |ke| km.resolve(None, KeyStroke::from_event(ke));
        assert!(matches!(r(ctrl('s')), Resolve::Command(c) if c == Command::CHAR_LEFT));
        assert!(matches!(r(ctrl('y')), Resolve::Command(c) if c == Command::DEL_LINE));
        assert!(matches!(r(ctrl('u')), Resolve::Command(c) if c == Command::UNDO));
    }

    #[test]
    fn keymap_two_key_prefixes() {
        use crate::event::Key;
        use crate::keymap::{KeyStroke, Keymap, Resolve};
        let km = Keymap::word_star();
        let ks = KeyStroke::from_event;
        // Ctrl-Q → prefix.
        assert!(matches!(km.resolve(None, ks(ctrl('q'))), Resolve::Prefix));
        // Ctrl-K → prefix.
        assert!(matches!(km.resolve(None, ks(ctrl('k'))), Resolve::Prefix));
        // Ctrl-Q then 'f' → FIND.
        let q = ks(ctrl('q'));
        assert!(matches!(
            km.resolve(Some(q), ks(key(Key::Char('f')))),
            Resolve::Command(c) if c == Command::FIND
        ));
        // Ctrl-K then 'b' → START_SELECT.
        let k = ks(ctrl('k'));
        assert!(matches!(
            km.resolve(Some(k), ks(key(Key::Char('b')))),
            Resolve::Command(c) if c == Command::START_SELECT
        ));
    }

    #[test]
    fn convert_event_prefix_then_command() {
        let _g = crate::keymap::GlobalKeymapGuard::new(crate::keymap::Keymap::word_star());
        let mut e = ed();
        // Ctrl-K starts a prefix and clears the event.
        let mut ev = Event::KeyDown(ctrl('k'));
        e.convert_event(&mut ev);
        assert!(ev.is_nothing(), "prefix key is cleared");
        assert!(e.pending.is_some(), "pending stroke stored");
        // The next 'b' resolves to cmStartSelect.
        let mut ev2 = Event::KeyDown(key(crate::event::Key::Char('b')));
        e.convert_event(&mut ev2);
        assert_eq!(ev2, Event::Command(Command::START_SELECT));
        assert!(e.pending.is_none(), "prefix consumed");
    }

    /// Regression: plain Backspace must delete the character to the left.
    /// The WordStar default (seeded in Phase 1) binds `backspace → BACK_SPACE`,
    /// so this test needs no `set_global`.  Before the Phase 2 refactor,
    /// plain Backspace had no binding in the hardcoded match table and was a
    /// no-op.
    #[test]
    fn plain_backspace_deletes_char_left() {
        let mut e = ed();
        insert(&mut e, "ab");
        let mut cx = Cx::new();
        let mut ev = Event::KeyDown(crate::event::KeyEvent::from(crate::event::Key::Backspace));
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(
            text(&e),
            "a",
            "plain Backspace must delete the char to the left"
        );
        check_invariant(&e);
    }

    // -- handle_event end-to-end (a typed char) ------------------------------

    #[test]
    fn handle_event_inserts_typed_char() {
        let mut e = ed();
        let mut cx = Cx::new();
        let mut ev = Event::KeyDown(key(crate::event::Key::Char('A')));
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(text(&e), "A");
        assert!(ev.is_nothing(), "consumed");
    }

    /// Regression: Shift+Tab is NOT insertable — it must not write a `\t`. Only a
    /// plain Tab inserts; Shift+Tab falls through and bubbles to the dialog for
    /// backward focus navigation.
    #[test]
    fn shift_tab_does_not_insert_tab() {
        let mut e = ed();
        let mut cx = Cx::new();
        let shift_tab = crate::event::KeyEvent::new(
            crate::event::Key::Tab,
            crate::event::KeyModifiers {
                shift: true,
                ..Default::default()
            },
        );
        let mut ev = Event::KeyDown(shift_tab);
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(text(&e), "", "Shift+Tab must NOT insert a tab");
        assert_eq!(
            ev,
            Event::KeyDown(shift_tab),
            "Shift+Tab must survive uncleared — it bubbles to dialog back-tab nav"
        );
    }

    #[test]
    fn handle_event_char_left_command() {
        let mut e = ed();
        insert(&mut e, "abc");
        let mut cx = Cx::new();
        let mut ev = Event::Command(Command::CHAR_LEFT);
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(e.cur_ptr, 2, "cursor moved left one char");
    }

    /// Regression: Ctrl-Del must delete the word to the right, not clear the
    /// selection — the WordStar preset binds `ctrl+delete → DEL_WORD` and omits the
    /// dead clear binding per the keymap design.
    #[test]
    fn ctrl_del_deletes_word_to_the_right() {
        let _g = crate::keymap::GlobalKeymapGuard::new(crate::keymap::Keymap::word_star());
        let mut e = ed();
        insert(&mut e, "foo bar");
        e.set_cur_ptr(0, 0);
        let mut cx = Cx::new();
        let ctrl_del = crate::event::KeyEvent::new(
            crate::event::Key::Delete,
            crate::event::KeyModifiers {
                ctrl: true,
                ..Default::default()
            },
        );
        let mut ev = Event::KeyDown(ctrl_del);
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }
        // delWord = deleteRange(curPtr, nextWord(curPtr), False); nextWord("foo bar")
        // from 0 lands on the space after "foo" (3), so "foo" is deleted (the
        // standalone trailing-space removal happens on the NEXT delWord).
        assert_eq!(text(&e), " bar", "Ctrl-Del deleted the word, not all text");
        check_invariant(&e);
    }

    /// Select-All selects the whole buffer.
    #[test]
    fn select_all_selects_whole_buffer() {
        let mut e = ed();
        insert(&mut e, "hello world");
        let mut cx = Cx::new();
        let mut ev = Event::Command(Command::SELECT_ALL);
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(e.sel_start, 0);
        assert_eq!(e.sel_end, e.buf_len);
        assert!(e.has_selection());
    }

    /// Overwrite mode replaces the char under the cursor instead of inserting.
    #[test]
    fn overwrite_mode_replaces_char() {
        let mut e = ed();
        insert(&mut e, "abc");
        e.set_cur_ptr(1, 0); // between 'a' and 'b'
        e.toggle_ins_mode(); // overwrite on
        assert!(e.overwrite);
        let len_before = e.buf_len;
        let mut cx = Cx::new();
        let mut ev = Event::KeyDown(key(crate::event::Key::Char('X')));
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(text(&e), "aXc", "overwrote 'b'");
        assert_eq!(e.buf_len, len_before, "buf_len unchanged in overwrite");
        check_invariant(&e);
    }

    /// Invalid UTF-8 bytes (reachable via the public byte API) advance exactly one
    /// logical byte per `next_char`, never desyncing from a grapheme boundary.
    #[test]
    fn next_char_over_invalid_utf8_advances_one_byte() {
        let mut e = ed();
        let mut cx = Cx::new();
        {
            let mut ctx = cx.ctx();
            e.insert_text(&[b'a', 0xFF, b'b'], false, &mut ctx);
        }
        assert_eq!(e.buf_len, 3);
        check_invariant(&e);
        // Walk: 0 → 1 ('a') → 2 (invalid 0xFF, one byte) → 3 ('b').
        let p1 = e.next_char(0);
        assert_eq!(p1, 1);
        let p2 = e.next_char(p1);
        assert_eq!(p2, 2, "invalid byte advances exactly one logical byte");
        let p3 = e.next_char(p2);
        assert_eq!(p3, 3);
        // And prev_char steps back symmetrically.
        assert_eq!(e.prev_char(3), 2);
        assert_eq!(e.prev_char(2), 1, "step back over the invalid byte by one");
        assert_eq!(e.prev_char(1), 0);
        check_invariant(&e);
    }

    // -- clipboard broker (deferred ops) -------------------------------------

    #[test]
    fn clip_copy_queues_set_clipboard() {
        let mut e = ed();
        insert(&mut e, "hello");
        e.set_select(0, 5, false);
        let mut cx = Cx::new();
        {
            let mut ctx = cx.ctx();
            assert!(e.clip_copy(&mut ctx));
        }
        assert!(
            cx.deferred
                .iter()
                .any(|d| matches!(d, Deferred::SetClipboard(s) if s == "hello")),
            "clipCopy queues SetClipboard with the selection"
        );
    }

    #[test]
    fn clip_paste_queues_editor_paste() {
        // The editor must have an id to be addressable.
        let mut group = Group::new(Rect::new(0, 0, 40, 10));
        let id = group.insert(Box::new(ed()));
        let mut cx = Cx::new();
        {
            let mut ctx = cx.ctx();
            let v = group.find_mut(id).unwrap();
            let e = v.as_any_mut().unwrap().downcast_mut::<Editor>().unwrap();
            e.clip_paste(&mut ctx);
        }
        assert!(
            cx.deferred
                .iter()
                .any(|d| matches!(d, Deferred::EditorPaste(eid) if *eid == id))
        );
    }

    // -- scrollbar broker request --------------------------------------------

    #[test]
    fn broadcast_from_own_bar_requests_sync() {
        // Mint scrollbar ids.
        let mut bg = Group::new(Rect::new(0, 0, 4, 4));
        let h = bg.insert(Box::new(crate::widgets::ScrollBar::new(Rect::new(
            0, 0, 1, 4,
        ))));
        let mut group = Group::new(Rect::new(0, 0, 40, 10));
        let id = group.insert(Box::new(Editor::new(
            Rect::new(0, 0, 40, 10),
            Some(h),
            None,
            None,
            1024,
        )));
        let mut cx = Cx::new();
        {
            let mut ctx = cx.ctx();
            let mut ev = Event::Broadcast {
                command: Command::SCROLL_BAR_CHANGED,
                source: Some(h),
            };
            group.find_mut(id).unwrap().handle_event(&mut ev, &mut ctx);
        }
        assert!(
            cx.deferred
                .iter()
                .any(|d| matches!(d, Deferred::SyncEditorDelta { editor, .. } if *editor == id))
        );
    }

    // -- snapshot ------------------------------------------------------------

    #[test]
    fn snapshot_two_lines_with_selection() {
        use crate::backend::{HeadlessBackend, Renderer};
        use crate::screen::Buffer;
        use crate::theme::Theme;

        let theme = Theme::classic_blue();
        let mut e = Editor::new(Rect::new(0, 0, 12, 4), None, None, None, 1024);
        insert(&mut e, "hello\nworld");
        // Select "ello" on line 0.
        e.set_select(1, 5, false);

        let (backend, screen) = HeadlessBackend::new(12, 4);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = e.state.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            e.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    // -- Memo ----------------------------------------------------------------

    fn memo() -> Memo {
        Memo::new(Rect::new(0, 0, 40, 10), None, None, None, 1024)
    }

    #[test]
    fn memo_value_set_value_round_trip() {
        let mut m = memo();
        m.set_value(crate::data::FieldValue::Text("hello\nworld".into()));
        assert_eq!(
            m.value(),
            Some(crate::data::FieldValue::Text("hello\nworld".into()))
        );
    }

    #[test]
    fn memo_set_value_non_text_is_noop() {
        let mut m = memo();
        m.set_value(crate::data::FieldValue::Text("initial".into()));
        m.set_value(crate::data::FieldValue::Int(7));
        assert_eq!(
            m.value(),
            Some(crate::data::FieldValue::Text("initial".into()))
        );
    }

    #[test]
    fn memo_tab_swallowed_not_cleared() {
        let mut m = memo();
        let mut cx = Cx::new();
        // Plain Tab: Memo swallows it (returns without clearing) — dialog can use it.
        let mut ev = Event::KeyDown(crate::event::KeyEvent::from(crate::event::Key::Tab));
        {
            let mut ctx = cx.ctx();
            m.handle_event(&mut ev, &mut ctx);
        }
        // The event must still be a Tab KeyDown (not consumed / not cleared).
        assert_eq!(
            ev,
            Event::KeyDown(crate::event::KeyEvent::from(crate::event::Key::Tab)),
            "plain Tab must NOT be consumed by Memo"
        );
        // The buffer must still be empty (Tab was not inserted).
        assert_eq!(text(&m.editor), "", "Tab must not insert text");

        // A plain printable char IS forwarded and inserts text.
        let mut ev2 = Event::KeyDown(key(crate::event::Key::Char('x')));
        {
            let mut ctx = cx.ctx();
            m.handle_event(&mut ev2, &mut ctx);
        }
        assert!(
            ev2.is_nothing(),
            "printable char must be consumed by editor"
        );
        assert_eq!(text(&m.editor), "x", "char 'x' must be inserted");
    }

    /// Memo only swallows *plain* Tab; Shift+Tab is forwarded to the editor,
    /// which (post-fix) treats it as non-insertable — so no `\t` is written and
    /// it bubbles to the dialog for backward focus navigation.
    #[test]
    fn memo_shift_tab_does_not_insert() {
        let mut m = memo();
        let mut cx = Cx::new();
        let shift_tab = crate::event::KeyEvent::new(
            crate::event::Key::Tab,
            crate::event::KeyModifiers {
                shift: true,
                ..Default::default()
            },
        );
        let mut ev = Event::KeyDown(shift_tab);
        {
            let mut ctx = cx.ctx();
            m.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(text(&m.editor), "", "Shift+Tab must NOT insert text");
        assert_eq!(
            ev,
            Event::KeyDown(shift_tab),
            "Shift+Tab must survive uncleared — it bubbles to dialog back-tab nav"
        );
    }

    #[test]
    fn memo_snapshot() {
        use crate::backend::{HeadlessBackend, Renderer};
        use crate::screen::Buffer;
        use crate::theme::Theme;

        let theme = Theme::classic_blue();
        let mut m = Memo::new(Rect::new(0, 0, 20, 4), None, None, None, 1024);
        m.set_value(crate::data::FieldValue::Text("line one\nline two".into()));

        let (backend, screen) = HeadlessBackend::new(20, 4);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = m.editor.state.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            m.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    // -- FileEditor -----------------------------------------------------------

    /// A unique temp-file path per test (no `tempfile` dev-dep).
    fn tmp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "rstv_fileeditor_{}_{}.txt",
            std::process::id(),
            name
        ))
    }

    /// An untitled file-editor (growable buffer, no backing file).
    fn untitled_fe() -> FileEditor {
        FileEditor::new(Rect::new(0, 0, 40, 10), None, None, None, None)
    }

    /// A base editor / Memo with a fixed buffer REFUSES to grow.
    /// `set_buf_size` past capacity returns false,
    /// and `set_text` of an over-capacity payload is a no-op (text unchanged).
    #[test]
    fn base_editor_buffer_does_not_grow() {
        // Fixed 8-byte buffer.
        let mut e = Editor::new(Rect::new(0, 0, 40, 10), None, None, None, 8);
        assert!(!e.file_editor, "base editor is not in file-editor mode");
        // BEFORE state: the buffer fits 8 bytes; asking for more must fail.
        assert!(e.set_buf_size(8), "exactly-fitting size succeeds");
        assert!(
            !e.set_buf_size(9),
            "over-capacity grow refused (default off)"
        );
        assert_eq!(e.buf_size, 8, "buf_size unchanged by a refused grow");

        // set_text of a 20-byte payload (over capacity) is an all-or-nothing no-op.
        e.set_text(b"0123456789abcdefghij");
        assert_eq!(e.buf_len, 0, "over-capacity set_text left the buffer empty");
        assert!(e.text().is_empty(), "text unchanged after refused grow");
        assert_eq!(e.buf_size, 8, "buf_size still fixed at 8");
    }

    /// Memo (a Memo wraps a base Editor) likewise refuses to grow.
    #[test]
    fn memo_buffer_does_not_grow() {
        let mut m = Memo::new(Rect::new(0, 0, 40, 10), None, None, None, 8);
        assert!(!m.editor.file_editor);
        m.editor.set_text(b"0123456789abcdefghij");
        assert!(
            m.editor.text().is_empty(),
            "Memo's fixed buffer did not grow"
        );
        assert_eq!(m.editor.buf_size, 8);
    }

    /// Growth works when file-editor mode is on: a >0x1000 payload round-trips and
    /// `buf_size` grows to a 0x1000 multiple ≥ payload.
    #[test]
    fn file_editor_buffer_grows() {
        let mut fe = untitled_fe();
        assert!(fe.editor.file_editor);
        assert_eq!(fe.editor.buf_size, 0, "starts empty (TFileEditor model)");

        let payload = vec![b'x'; 0x1000 + 37]; // > one 0x1000 page
        fe.editor.set_text(&payload);

        assert_eq!(fe.editor.text(), payload, "payload round-trips");
        assert_eq!(fe.editor.buf_len, payload.len());
        assert!(
            fe.editor.buf_size >= payload.len(),
            "buf_size accommodates the payload"
        );
        assert_eq!(
            fe.editor.buf_size % 0x1000,
            0,
            "buf_size is a 0x1000 multiple"
        );
        assert_eq!(fe.editor.buf_size, 0x2000, "rounded up to two pages");
        check_invariant(&fe.editor);
    }

    /// Grow with content present and the cursor mid-buffer — exercises the
    /// non-degenerate `n > 0` tail memmove in `set_buf_size` (the other growth
    /// tests all start empty, so `n == 0` and the move is a no-op).
    #[test]
    fn file_editor_buffer_grows_with_content() {
        let mut fe = untitled_fe();
        fe.editor.set_text(&vec![b'a'; 0x1000]); // one full page; gap_len == 0
        assert_eq!(fe.editor.buf_size, 0x1000);
        fe.editor.set_cur_ptr(0x800, 0); // cursor in the middle (n becomes 0x800)
        insert(&mut fe.editor, "Z"); // forces set_buf_size(0x1001) -> grow

        let mut expected = vec![b'a'; 0x800];
        expected.push(b'Z');
        expected.extend(std::iter::repeat_n(b'a', 0x800));
        assert_eq!(
            fe.editor.text(),
            expected,
            "text intact across a grow with n > 0"
        );
        assert_eq!(fe.editor.buf_size, 0x2000, "grew to two pages");
        check_invariant(&fe.editor);
    }

    /// load_file round-trip: a written file loads into the editor and is valid.
    #[test]
    fn file_editor_load_round_trip() {
        let path = tmp_path("load_round_trip");
        let content = b"first line\nsecond line\nthird line\n";
        std::fs::write(&path, content).unwrap();

        let fe = FileEditor::new(
            Rect::new(0, 0, 40, 10),
            None,
            None,
            None,
            Some(path.clone()),
        );
        assert!(fe.editor.is_valid, "loaded editor is valid");
        assert_eq!(fe.editor.text(), content, "loaded text matches the file");

        let _ = std::fs::remove_file(&path);
    }

    /// A missing file loads as a valid, empty buffer.
    #[test]
    fn file_editor_load_missing_file() {
        let path = tmp_path("missing_does_not_exist");
        let _ = std::fs::remove_file(&path); // ensure absent
        let fe = FileEditor::new(
            Rect::new(0, 0, 40, 10),
            None,
            None,
            None,
            Some(path.clone()),
        );
        assert!(fe.editor.is_valid, "missing file ⇒ valid");
        assert!(fe.editor.text().is_empty(), "missing file ⇒ empty buffer");
    }

    /// Unreadable file (exists, mode 0o000) ⇒ is_valid=false, pending_load_error stored,
    /// and `handle_event` queues an `OpenMessageBox` on the first call.
    #[cfg(unix)]
    #[test]
    fn file_editor_load_unreadable_queues_error_box() {
        use crate::view::Deferred;
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;

        // Root bypasses DAC permission checks — mode 0o000 still opens for root.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }

        let path = tmp_path("unreadable");
        let _ = std::fs::remove_file(&path);
        std::fs::File::create(&path).expect("create temp file");
        std::fs::set_permissions(&path, Permissions::from_mode(0o000)).expect("set mode 0o000");

        let mut fe = FileEditor::new(
            Rect::new(0, 0, 40, 10),
            None,
            None,
            None,
            Some(path.clone()),
        );

        // Restore permissions so cleanup can proceed regardless of outcome.
        let _ = std::fs::set_permissions(&path, Permissions::from_mode(0o600));
        let _ = std::fs::remove_file(&path);

        assert!(!fe.editor.is_valid, "unreadable file ⇒ is_valid=false");
        assert!(
            fe.pending_load_error.is_some(),
            "pending_load_error populated after load failure"
        );

        // First handle_event must flush the error as an OpenMessageBox.
        let mut cx = Cx::new();
        let mut ev = crate::event::Event::Nothing;
        {
            let mut ctx = cx.ctx();
            fe.handle_event(&mut ev, &mut ctx);
        }
        assert!(
            fe.pending_load_error.is_none(),
            "pending_load_error consumed after handle_event"
        );
        assert!(
            cx.deferred.iter().any(|d| matches!(
                d,
                Deferred::OpenMessageBox { kind, buttons, .. }
                    if *kind == crate::dialog::MessageBoxKind::Error
                    && *buttons == crate::dialog::MessageBoxButtons::ok()
            )),
            "handle_event queued an OpenMessageBox(Error) for the read failure"
        );
    }

    /// save_file round-trip: an untitled buffer, given a filename, writes to disk
    /// and clears `modified`.
    #[test]
    fn file_editor_save_round_trip() {
        let path = tmp_path("save_round_trip");
        let _ = std::fs::remove_file(&path);

        let mut fe = untitled_fe();
        insert(&mut fe.editor, "saved content\nmore text\n");
        assert!(fe.editor.modified(), "insert marks modified");

        fe.file_name = Some(path.clone());
        let mut cx = Cx::new();
        assert!(fe.save(&mut cx.ctx()), "save() to a named file succeeds");

        let on_disk = std::fs::read(&path).unwrap();
        assert_eq!(on_disk, fe.editor.text(), "disk bytes equal editor text");
        assert!(!fe.editor.modified(), "modified cleared after save");

        let _ = std::fs::remove_file(&path);
    }

    /// EF_BACKUP_FILES: when the flag is set, save_file renames the existing file
    /// to `<name>~` before writing the new content, using the Unix `~` convention.
    #[test]
    fn backup_file_created_on_save_with_ef_backup_files() {
        let path = tmp_path("backup_test");
        let backup = {
            let mut s = path.as_os_str().to_os_string();
            s.push("~");
            std::path::PathBuf::from(s)
        };
        // Ensure clean state.
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&backup);

        // Write original content so there is something to back up.
        let original = b"original content\n";
        std::fs::write(&path, original).unwrap();

        // Build a FileEditor with EF_BACKUP_FILES enabled.
        let mut fe = FileEditor::new(
            Rect::new(0, 0, 40, 10),
            None,
            None,
            None,
            Some(path.clone()),
        );
        assert!(fe.editor.is_valid, "loaded editor is valid");
        fe.editor.set_editor_flags(EF_BACKUP_FILES);

        // Replace content via insert and save.
        fe.editor.set_buf_len(0); // clear the buffer
        insert(&mut fe.editor, "new content\n");
        let mut cx = Cx::new();
        assert!(fe.save_file(&mut cx.ctx()), "save_file succeeds");

        // The backup must contain the original bytes.
        let backed_up = std::fs::read(&backup).expect("backup file must exist");
        assert_eq!(backed_up, original, "backup holds the original content");

        // The main file must contain the new content.
        let on_disk = std::fs::read(&path).unwrap();
        assert_eq!(
            on_disk,
            fe.editor.text(),
            "saved file holds the new content"
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&backup);
    }

    /// Without EF_BACKUP_FILES the backup file must NOT be created.
    #[test]
    fn no_backup_without_ef_backup_files() {
        let path = tmp_path("no_backup_test");
        let backup = {
            let mut s = path.as_os_str().to_os_string();
            s.push("~");
            std::path::PathBuf::from(s)
        };
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&backup);

        std::fs::write(&path, b"original\n").unwrap();

        let mut fe = FileEditor::new(
            Rect::new(0, 0, 40, 10),
            None,
            None,
            None,
            Some(path.clone()),
        );
        assert!(fe.editor.is_valid);
        // flag NOT set — default editor_flags = 0
        let mut cx = Cx::new();
        fe.save_file(&mut cx.ctx());

        assert!(
            !backup.exists(),
            "backup must not be created when EF_BACKUP_FILES is off"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// A Save command via handle_event writes the file and clears the event.
    #[test]
    fn file_editor_handle_save_command() {
        let path = tmp_path("handle_save_command");
        let _ = std::fs::remove_file(&path);

        let mut fe = untitled_fe();
        insert(&mut fe.editor, "via handle_event\n");
        fe.file_name = Some(path.clone());

        let mut cx = Cx::new();
        let mut ev = Event::Command(Command::SAVE);
        fe.handle_event(&mut ev, &mut cx.ctx());

        assert!(ev.is_nothing(), "cmSave was cleared by handle_event");
        let on_disk = std::fs::read(&path).unwrap();
        assert_eq!(on_disk, fe.editor.text(), "file written via cmSave");
        assert!(!fe.editor.modified(), "modified cleared");
        // clear_modified's UF_UPDATE was flushed inline (not left pending) — the
        // indicator/commands publish modified=false on this event, not the next.
        assert_eq!(
            fe.editor.update_flags, 0,
            "no pending update flag after cmSave flush"
        );

        let _ = std::fs::remove_file(&path);
    }

    /// Calling `save()` directly on an untitled buffer is a no-op (it has no name
    /// to write to; the save-as dialog supplies one) — returns false, no panic.
    #[test]
    fn file_editor_save_untitled_is_noop() {
        let mut fe = untitled_fe();
        insert(&mut fe.editor, "unsaved");
        let mut cx = Cx::new();
        assert!(
            !fe.save(&mut cx.ctx()),
            "untitled save is a no-op (the save-as dialog supplies a name)"
        );
        assert!(fe.editor.modified(), "still modified — nothing was saved");
    }

    /// valid: the plain validity command reflects is_valid; a modified buffer
    /// requests the save prompt and vetoes (false) until the answer routes back;
    /// an unmodified buffer allows close.
    #[test]
    fn file_editor_valid() {
        let mut fe = untitled_fe();
        assert!(fe.editor.is_valid);
        let mut cx = Cx::new();
        assert!(
            fe.valid(Command::VALID, &mut cx.ctx()),
            "cmValid reflects a valid buffer"
        );
        // Unmodified, non-cmValid ⇒ allow close (no prompt).
        assert!(
            fe.valid(Command::CLOSE, &mut cx.ctx()),
            "unmodified buffer allows close"
        );
    }

    // -- save-as: the view-triggered file dialog -------------------------------

    /// A Save-As command on a FileEditor requests the save-as file dialog (the
    /// deferred push), regardless of whether the buffer is titled, and clears the
    /// event.
    #[test]
    fn save_as_requests_dialog() {
        let mut group = Group::new(Rect::new(0, 0, 40, 10));
        let id = group.insert(Box::new(untitled_fe()));
        let mut cx = Cx::new();
        {
            let mut ctx = cx.ctx();
            let mut ev = Event::Command(Command::SAVE_AS);
            group.find_mut(id).unwrap().handle_event(&mut ev, &mut ctx);
            assert!(ev.is_nothing(), "cmSaveAs is consumed (cleared)");
        }
        assert!(
            cx.deferred
                .iter()
                .any(|d| matches!(d, Deferred::OpenSaveAsDialog { editor_id } if *editor_id == id)),
            "cmSaveAs queues OpenSaveAsDialog for the editor"
        );
    }

    /// Saving an untitled buffer also requests the dialog — it cannot save without
    /// a name.
    #[test]
    fn untitled_save_requests_dialog() {
        let mut group = Group::new(Rect::new(0, 0, 40, 10));
        let id = group.insert(Box::new(untitled_fe()));
        let mut cx = Cx::new();
        {
            let mut ctx = cx.ctx();
            let v = group.find_mut(id).unwrap();
            let fe = v
                .as_any_mut()
                .unwrap()
                .downcast_mut::<FileEditor>()
                .unwrap();
            assert!(
                !fe.save(&mut ctx),
                "untitled save returns false (async path)"
            );
        }
        assert!(
            cx.deferred
                .iter()
                .any(|d| matches!(d, Deferred::OpenSaveAsDialog { editor_id } if *editor_id == id)),
            "untitled save queues OpenSaveAsDialog"
        );
    }

    /// After the save-as dialog completion set `file_name` + `pending_title_update`,
    /// the re-injected Save writes the new file AND broadcasts a title-update (the
    /// EditWindow title refresh). Drives the editor through a real Save over a temp
    /// path.
    #[test]
    fn save_as_then_save_writes_and_broadcasts_title() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("rstv_saveas_test_{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let mut group = Group::new(Rect::new(0, 0, 40, 10));
        let id = group.insert(Box::new(untitled_fe()));
        // Put some content + the SaveAsPick post-conditions on the editor.
        {
            let v = group.find_mut(id).unwrap();
            let fe = v
                .as_any_mut()
                .unwrap()
                .downcast_mut::<FileEditor>()
                .unwrap();
            fe.editor.set_text(b"hello saveAs");
            fe.file_name = Some(path.clone());
            fe.pending_title_update = true;
        }
        let mut cx = Cx::new();
        {
            let mut ctx = cx.ctx();
            let mut ev = Event::Command(Command::SAVE);
            group.find_mut(id).unwrap().handle_event(&mut ev, &mut ctx);
            assert!(ev.is_nothing(), "cmSave consumed");
        }

        // The file was written with the buffer contents.
        let written = std::fs::read(&path).expect("file written");
        assert_eq!(written, b"hello saveAs");
        let _ = std::fs::remove_file(&path);

        // cmUpdateTitle was broadcast (sourced from the editor) and the flag cleared.
        assert!(
            cx.out.iter().any(|e| matches!(
                e,
                Event::Broadcast { command, source } if *command == Command::UPDATE_TITLE && *source == Some(id)
            )),
            "successful saveAs broadcasts cmUpdateTitle"
        );
        {
            let v = group.find_mut(id).unwrap();
            let fe = v
                .as_any_mut()
                .unwrap()
                .downcast_mut::<FileEditor>()
                .unwrap();
            assert!(!fe.pending_title_update, "title-update flag consumed");
        }
    }

    /// `EditWindow::handle_event` refreshes its frame title on a title-update
    /// broadcast, recomputing it from the hosted editor's current `file_name`.
    #[test]
    fn edit_window_updates_title_on_broadcast() {
        let mut ew = EditWindow::new(Rect::new(0, 0, 40, 15), None, 0);
        assert_eq!(ew.window.title(), Some("Untitled"));
        let editor_id = ew.editor_id;
        // Rename the hosted editor (simulating a completed saveAs).
        {
            let fe = ew
                .window
                .child_mut(editor_id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<FileEditor>())
                .expect("editor child");
            fe.file_name = Some(std::path::PathBuf::from("/tmp/renamed.txt"));
        }
        let mut cx = Cx::new();
        {
            let mut ctx = cx.ctx();
            let mut ev = Event::Broadcast {
                command: Command::UPDATE_TITLE,
                source: Some(editor_id),
            };
            View::handle_event(&mut ew, &mut ev, &mut ctx);
        }
        assert_eq!(
            ew.window.title(),
            Some("/tmp/renamed.txt"),
            "frame title refreshed from the editor's new file_name"
        );
    }

    // -----------------------------------------------------------------------
    // EditWindow tests
    // -----------------------------------------------------------------------

    /// 1a. Title is the filename stem when a path is given.
    #[test]
    fn edit_window_title_from_path() {
        let ew = EditWindow::new(
            Rect::new(0, 0, 40, 15),
            Some(std::path::PathBuf::from("/tmp/foo.txt")),
            1,
        );
        let t = ew.window.title().unwrap_or("");
        assert!(
            t.contains("foo.txt"),
            "title should contain the filename; got {t:?}"
        );
    }

    /// 1b. Title is "Untitled" when no path is given.
    #[test]
    fn edit_window_title_untitled() {
        let ew = EditWindow::new(Rect::new(0, 0, 40, 15), None, 0);
        assert_eq!(
            ew.window.title(),
            Some("Untitled"),
            "untitled edit window title"
        );
    }

    /// 2. size_limits returns (24,6) as the minimum.
    #[test]
    fn edit_window_size_limits_min() {
        let ew = EditWindow::new(Rect::new(0, 0, 40, 15), None, 0);
        let (min, _max) = View::size_limits(&ew, Point::new(80, 25));
        assert_eq!(min, Point::new(24, 6), "minEditWinSize = {{24, 6}}");
    }

    /// 3. Construction invariants: editor child is visible+selectable; the
    ///    scrollbars/indicator start hidden (load-bearing — `reset_current`
    ///    picks the first visible+selectable child, so a stray-visible scrollbar
    ///    would steal currency from the editor); the window is tileable.
    #[test]
    fn edit_window_child_visibility_invariant() {
        let mut ew = EditWindow::new(Rect::new(0, 0, 40, 15), None, 0);
        let editor_id = ew.editor_id;

        // Editor child is visible and selectable.
        {
            let fe = ew.window.child_mut(editor_id).expect("editor child exists");
            let st = fe.state();
            assert!(st.state.visible, "editor child is visible");
            assert!(
                st.options.selectable,
                "editor child is selectable (reset_current picks it)"
            );
        }

        // The scrollbars and indicator start hidden.
        for id in [ew.h_scroll_bar_id, ew.v_scroll_bar_id, ew.indicator_id] {
            let v = ew.window.child_mut(id).expect("aux child exists");
            assert!(
                !v.state().state.visible,
                "aux child {id:?} must start hidden"
            );
        }

        // The tileable flag was set.
        assert!(
            View::state(&ew).options.tileable,
            "ofTileable set on EditWindow"
        );
    }

    /// 4. Snapshot: an untitled EditWindow renders as a framed window.
    #[test]
    fn edit_window_snapshot() {
        use crate::backend::{HeadlessBackend, Renderer};
        use crate::screen::Buffer;
        use crate::theme::Theme;

        let theme = Theme::classic_blue();
        let ew = EditWindow::new(Rect::new(0, 0, 30, 8), None, 1);
        let mut view: Box<dyn View> = Box::new(ew);
        let (backend, screen) = HeadlessBackend::new(30, 8);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = view.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            view.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    // -- mouse hold-tracking --------------------------------------------------
    //
    // These tests drive the tracked arms directly with view-local positions, as
    // the pump's Deferred::MouseTrack apply does (the capture itself is
    // unit-tested in capture::tests; here we verify the editor's loop bodies).

    use crate::event::{MouseButtons, MouseEvent, MouseEventFlags, MouseWheel};

    fn left_mouse(x: i32, y: i32) -> MouseEvent {
        MouseEvent {
            position: Point::new(x, y),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn mouse_down_at(x: i32, y: i32) -> Event {
        Event::MouseDown(left_mouse(x, y))
    }

    fn mouse_move_at(x: i32, y: i32) -> Event {
        Event::MouseMove(left_mouse(x, y))
    }

    fn mouse_auto_at(x: i32, y: i32) -> Event {
        Event::MouseAuto(left_mouse(x, y))
    }

    fn mouse_up_at(x: i32, y: i32) -> Event {
        Event::MouseUp(MouseEvent {
            position: Point::new(x, y),
            ..Default::default()
        })
    }

    fn middle_down_at(x: i32, y: i32) -> Event {
        Event::MouseDown(MouseEvent {
            position: Point::new(x, y),
            buttons: MouseButtons {
                middle: true,
                ..Default::default()
            },
            ..Default::default()
        })
    }

    /// A mouse-wheel event (crossterm ScrollUp/Down → `Event::MouseWheel`).
    fn wheel_down_at(x: i32, y: i32, wheel: MouseWheel) -> Event {
        Event::MouseWheel(MouseEvent {
            position: Point::new(x, y),
            wheel,
            ..Default::default()
        })
    }

    /// Stamp the editor with a fresh ViewId (as Group::insert would do).
    fn give_id(e: &mut Editor) -> ViewId {
        let id = ViewId::next();
        e.state.id = Some(id);
        id
    }

    /// A 40×10 editor holding 15 one-word lines (`line00\n` … `line14\n`,
    /// 7 bytes each) — tall enough to vertical-scroll (limit.y = 16 > 10).
    fn tall_ed() -> Editor {
        let mut e = ed();
        let text: String = (0..15).map(|i| format!("line{i:02}\n")).collect();
        insert(&mut e, &text);
        e.set_cur_ptr(0, 0);
        e
    }

    /// A left press on an inserted editor: the first iteration positions the
    /// cursor, the track state carries the select mode with `SM_EXTEND` set, and
    /// the push-capture deferred names this editor's id.
    #[test]
    fn track_mouse_down_arms_select_capture() {
        let mut e = ed();
        insert(&mut e, "hello world");
        let id = give_id(&mut e);

        let mut cx = Cx::new();
        let mut ev = mouse_down_at(2, 0);
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "MouseDown consumed");
        assert_eq!(e.cur_ptr, 2, "first iteration: setCurPtr(getMousePtr)");
        assert_eq!(
            e.track,
            Some(EditorTrack::Select {
                select_mode: SM_EXTEND
            }),
            "track carries selectMode |= smExtend after the first iteration"
        );
        let pushes: Vec<_> = cx
            .deferred
            .iter()
            .filter_map(|d| match d {
                Deferred::PushCapture(h) => Some(h.view()),
                _ => None,
            })
            .collect();
        assert_eq!(pushes, vec![Some(id)], "one PushCapture naming the editor");
    }

    /// A double-click press seeds word-granular mode into the live track select
    /// mode, which persists across drag iterations for a word-granular drag.
    #[test]
    fn track_double_click_persists_sm_double() {
        let mut e = ed();
        insert(&mut e, "hello world");
        give_id(&mut e);

        let mut cx = Cx::new();
        let mut ev = Event::MouseDown(MouseEvent {
            position: Point::new(1, 0),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            flags: MouseEventFlags {
                double_click: true,
                ..Default::default()
            },
            ..Default::default()
        });
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(
            e.track,
            Some(EditorTrack::Select {
                select_mode: SM_DOUBLE | SM_EXTEND
            }),
            "smDouble persists in the live selectMode"
        );
    }

    /// A move while drag-tracking extends the selection over the real buffer text
    /// and does NOT scroll.
    #[test]
    fn track_move_extends_selection() {
        let mut e = ed();
        insert(&mut e, "hello world");
        give_id(&mut e);

        let mut cx = Cx::new();
        let mut ev = mouse_down_at(2, 0);
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }

        let mut ev = mouse_move_at(8, 0);
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "tracked move consumed");
        assert_eq!((e.sel_start, e.sel_end), (2, 8), "selection 2..8");
        assert_eq!(&text(&e)[2..8], "llo wo", "selection covers the real text");
        assert_eq!(e.cur_ptr, 8, "cursor at the drag position");
        assert_eq!(e.delta, Point::new(0, 0), "a plain move never scrolls");
        assert!(e.track.is_some(), "still tracking after the move");
    }

    /// Auto-repeat below the view edge-scrolls down by one THEN extends the
    /// selection to the post-scroll mouse position (the unconditional cursor-move
    /// tail).
    #[test]
    fn track_auto_below_edge_scrolls_then_extends() {
        let mut e = tall_ed();
        give_id(&mut e);

        // Down on row 5 (line05, offset 35).
        let mut cx = Cx::new();
        let mut ev = mouse_down_at(0, 5);
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(e.cur_ptr, 35);

        // Auto at y = 10 (== size.y, out of bounds below): scroll down one,
        // then setCurPtr at the clamped row 9 + new delta.y 1 = line 10.
        let mut ev = mouse_auto_at(0, 10);
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "tracked auto consumed");
        assert_eq!(e.delta.y, 1, "scrolled down one row");
        assert_eq!(
            (e.sel_start, e.sel_end),
            (35, 70),
            "selection extended to the start of line10 (post-scroll getMousePtr)"
        );
    }

    /// Auto-repeat above the view edge-scrolls up by one then extends the
    /// selection backwards.
    #[test]
    fn track_auto_above_edge_scrolls_then_extends() {
        let mut e = tall_ed();
        give_id(&mut e);
        e.scroll_to(0, 3); // viewport starts at line 3

        // Down on view row 2 = buffer line 5 (offset 35).
        let mut cx = Cx::new();
        let mut ev = mouse_down_at(0, 2);
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(e.cur_ptr, 35);

        // Auto at y = -1 (out of bounds above): delta.y 3 → 2, then setCurPtr
        // at clamped row 0 + delta.y 2 = line 2 (offset 14).
        let mut ev = mouse_auto_at(0, -1);
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "tracked auto consumed");
        assert_eq!(e.delta.y, 2, "scrolled up one row");
        assert_eq!(
            (e.sel_start, e.sel_end),
            (14, 35),
            "selection extended backwards to the start of line02"
        );
    }

    /// A mouse wheel during the drag-select hold forwards to BOTH scroll bars (via
    /// the mouse-track deferred effect), self-posts a delta-sync (the bar's change
    /// notification would otherwise be swallowed by the modal capture), and still
    /// runs the unconditional cursor-move tail.
    #[test]
    fn track_wheel_in_hold_forwards_to_bars_and_syncs() {
        let hbar = ViewId::next();
        let vbar = ViewId::next();
        let mut e = Editor::new(Rect::new(0, 0, 40, 10), Some(hbar), Some(vbar), None, 1024);
        insert(&mut e, "hello world");
        let id = give_id(&mut e);

        let mut cx = Cx::new();
        let mut ev = mouse_down_at(2, 0);
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }

        // The wheel arrives view-localized via the capture's wheel mask.
        let mut cx = Cx::new();
        let mut ev = wheel_down_at(8, 0, MouseWheel::Down);
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "wheel-in-hold consumed");
        // Forwarded to the v-bar then the h-bar (C++ order), wheel payload intact.
        let forwards: Vec<_> = cx
            .deferred
            .iter()
            .filter_map(|d| match d {
                Deferred::MouseTrack {
                    view,
                    event: Event::MouseWheel(m),
                } if m.wheel == MouseWheel::Down => Some(*view),
                _ => None,
            })
            .collect();
        assert_eq!(forwards, vec![vbar, hbar], "wheel forwarded to both bars");
        assert!(
            cx.deferred.iter().any(|d| matches!(
                d,
                Deferred::SyncEditorDelta { editor, h, v }
                    if *editor == id && *h == Some(hbar) && *v == Some(vbar)
            )),
            "SyncEditorDelta self-posted (the swallowed-broadcast workaround)"
        );
        // The unconditional body still ran: selection extended to the wheel pos.
        assert_eq!((e.sel_start, e.sel_end), (2, 8), "setCurPtr tail ran");
    }

    /// A mouse wheel with NO hold in flight falls through unconsumed — the
    /// editor's event mask excludes the wheel outside a hold.
    #[test]
    fn track_wheel_outside_hold_falls_through() {
        let mut e = ed();
        insert(&mut e, "hello world");
        give_id(&mut e);
        let cur_before = e.cur_ptr;

        let mut cx = Cx::new();
        let mut ev = wheel_down_at(3, 0, MouseWheel::Up);
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }
        assert!(!ev.is_nothing(), "untracked wheel falls through");
        assert_eq!(e.cur_ptr, cur_before, "no cursor positioning from a wheel");
        assert!(cx.deferred.is_empty(), "nothing armed, nothing forwarded");
    }

    /// A middle-button press arms the pan track: the mouse position is recorded
    /// and the capture pushed, but nothing scrolls on the press itself — the
    /// cursor and selection are untouched.
    #[test]
    fn track_middle_down_arms_pan() {
        let mut e = tall_ed();
        let id = give_id(&mut e);
        let cur_before = e.cur_ptr;

        let mut cx = Cx::new();
        let mut ev = middle_down_at(5, 5);
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "middle down consumed");
        assert_eq!(
            e.track,
            Some(EditorTrack::Pan {
                last: Point::new(5, 5)
            }),
            "pan track armed with lastMouse"
        );
        assert_eq!(e.cur_ptr, cur_before, "no cursor change on the press");
        assert!(!e.has_selection(), "no selection from a pan press");
        let pushes: Vec<_> = cx
            .deferred
            .iter()
            .filter_map(|d| match d {
                Deferred::PushCapture(h) => Some(h.view()),
                _ => None,
            })
            .collect();
        assert_eq!(pushes, vec![Some(id)], "one PushCapture naming the editor");
    }

    /// A move while panning scrolls by the mouse delta and never touches the
    /// cursor or selection — the track-kind guard in the other direction.
    #[test]
    fn track_pan_move_scrolls_without_selection() {
        let mut e = tall_ed();
        give_id(&mut e);
        let cur_before = e.cur_ptr;

        let mut cx = Cx::new();
        let mut ev = middle_down_at(5, 5);
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }

        // Move up-left by (2,1): d = delta(0,0) + last(5,5) − mouse(3,4) = (2,1).
        let mut ev = mouse_move_at(3, 4);
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "pan move consumed");
        assert_eq!(e.delta, Point::new(2, 1), "scrolled by the mouse delta");
        assert_eq!(
            e.track,
            Some(EditorTrack::Pan {
                last: Point::new(3, 4)
            }),
            "lastMouse updated"
        );
        assert_eq!(e.cur_ptr, cur_before, "pan never moves the cursor");
        assert!(!e.has_selection(), "pan never selects");
    }

    /// A mouse wheel during the pan hold runs the same pan body — no scroll-bar
    /// forwarding.
    #[test]
    fn track_pan_wheel_tick_pans() {
        let mut e = tall_ed();
        give_id(&mut e);

        let mut cx = Cx::new();
        let mut ev = middle_down_at(5, 5);
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }

        let mut cx = Cx::new();
        let mut ev = wheel_down_at(4, 3, MouseWheel::Up);
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "pan wheel tick consumed");
        // d = delta(0,0) + last(5,5) − mouse(4,3) = (1,2).
        assert_eq!(e.delta, Point::new(1, 2), "wheel tick pans by the delta");
        assert!(
            !cx.deferred
                .iter()
                .any(|d| matches!(d, Deferred::MouseTrack { .. })),
            "no scrollbar forwarding from the pan body"
        );
    }

    /// A release clears the track (both holds just end; there is no post-hold
    /// work).
    #[test]
    fn track_up_clears_track() {
        for arm in [mouse_down_at(2, 0), middle_down_at(2, 0)] {
            let mut e = tall_ed();
            give_id(&mut e);

            let mut cx = Cx::new();
            let mut ev = arm;
            {
                let mut ctx = cx.ctx();
                e.handle_event(&mut ev, &mut ctx);
            }
            assert!(e.track.is_some(), "armed");

            let mut ev = mouse_up_at(2, 0);
            {
                let mut ctx = cx.ctx();
                e.handle_event(&mut ev, &mut ctx);
            }
            assert!(ev.is_nothing(), "tracked up consumed");
            assert!(e.track.is_none(), "track cleared on MouseUp");
        }
    }

    /// Stray `MouseUp` / `MouseMove` / `MouseAuto` with no track in flight fall
    /// through unconsumed (the tracking guard).
    #[test]
    fn track_stray_events_fall_through() {
        let mut e = tall_ed();
        give_id(&mut e);

        for mut ev in [mouse_up_at(2, 0), mouse_move_at(2, 0), mouse_auto_at(2, 0)] {
            let mut cx = Cx::new();
            {
                let mut ctx = cx.ctx();
                e.handle_event(&mut ev, &mut ctx);
            }
            assert!(!ev.is_nothing(), "stray {ev:?} falls through unconsumed");
            assert!(e.track.is_none(), "no track armed by a stray event");
        }
    }

    /// `MouseDown` on an editor without an id (uninserted): single-shot cursor
    /// positioning, no track, no capture — the faithful fallback.
    #[test]
    fn track_mouse_down_without_id_single_shot() {
        let mut e = ed();
        insert(&mut e, "hello world");
        // No id assigned.
        let mut cx = Cx::new();
        let mut ev = mouse_down_at(2, 0);
        {
            let mut ctx = cx.ctx();
            e.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "MouseDown still consumed");
        assert_eq!(e.cur_ptr, 2, "cursor positioned single-shot");
        assert!(e.track.is_none(), "no track without an id");
        assert!(
            cx.deferred.is_empty(),
            "no capture pushed for id-less editor"
        );
    }

    /// The forwarded track events reach the inner Editor through a FileEditor
    /// wrapper (the mouse-track deferred apply calls the wrapper's handle_event,
    /// which delegates to the editor — the edit-window hosting path).
    ///
    /// NOTE: this drives handle_event directly (handler-level), bypassing the
    /// pump's Deferred::MouseTrack drain; the deferred-apply path
    /// (group.find_mut + handle_event) is covered by the capture tests in
    /// capture::tests and the scrollbar pump round-trip in program.rs.
    #[test]
    fn track_through_file_editor_delegation() {
        let mut fe = FileEditor::new(Rect::new(0, 0, 40, 10), None, None, None, None);
        fe.editor.insert_text_core(b"hello world", false);
        fe.editor.set_cur_ptr(0, 0);
        let id = ViewId::next();
        fe.editor.state.id = Some(id);

        let mut cx = Cx::new();
        let mut ev = mouse_down_at(2, 0);
        {
            let mut ctx = cx.ctx();
            View::handle_event(&mut fe, &mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "MouseDown consumed through the wrapper");
        assert!(fe.editor.track.is_some(), "inner editor armed");
        let pushes: Vec<_> = cx
            .deferred
            .iter()
            .filter_map(|d| match d {
                Deferred::PushCapture(h) => Some(h.view()),
                _ => None,
            })
            .collect();
        assert_eq!(pushes, vec![Some(id)], "capture names the wrapper's id");

        let mut ev = mouse_move_at(8, 0);
        {
            let mut ctx = cx.ctx();
            View::handle_event(&mut fe, &mut ev, &mut ctx);
        }
        assert_eq!(
            (fe.editor.sel_start, fe.editor.sel_end),
            (2, 8),
            "drag-select body ran on the inner editor via delegation"
        );
    }

    // -- find/replace dialog layout snapshots ---------------------------------

    /// Render the Find dialog to verify its layout.
    #[test]
    fn find_dialog_layout() {
        use crate::backend::{HeadlessBackend, Renderer};
        use crate::data::FieldValue;
        use crate::dialog::Dialog;
        use crate::screen::Buffer;
        use crate::theme::Theme;
        use crate::view::{DrawCtx, Rect, View};
        use crate::widgets::{
            Button, ButtonFlags, CheckBoxes, InputLine, Label, LimitMode, THistory,
        };

        let mut d = Dialog::new(Rect::new(0, 0, 38, 12), Some("Find".into()));
        {
            let opts = &mut d.state_mut().options;
            opts.center_x = true;
            opts.center_y = true;
        }

        let mut il = InputLine::new(Rect::new(3, 3, 32, 4), 81, None, LimitMode::MaxBytes);
        il.set_value(FieldValue::Text("hello".into()));
        let find_id = d.insert_child(Box::new(il));
        d.insert_child(Box::new(Label::new(
            Rect::new(2, 2, 15, 3),
            "~T~ext to find",
            Some(find_id),
        )));
        d.insert_child(Box::new(THistory::new(
            Rect::new(32, 3, 35, 4),
            find_id,
            10,
        )));

        let mut cb = CheckBoxes::new(
            Rect::new(3, 5, 35, 7),
            vec!["~C~ase sensitive".into(), "~W~hole words only".into()],
        );
        cb.cluster.value = 0x0001; // case sensitive pre-selected
        d.insert_child(Box::new(cb));

        d.insert_child(Box::new(Button::new(
            Rect::new(14, 9, 24, 11),
            "O~K~",
            crate::command::Command::OK,
            ButtonFlags {
                default: true,
                ..Default::default()
            },
        )));
        d.insert_child(Box::new(Button::new(
            Rect::new(26, 9, 36, 11),
            "Cancel",
            crate::command::Command::CANCEL,
            ButtonFlags::new(),
        )));

        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(38, 12);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = d.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            d.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    /// Render the Replace dialog to verify its layout.
    #[test]
    fn replace_dialog_layout() {
        use crate::backend::{HeadlessBackend, Renderer};
        use crate::data::FieldValue;
        use crate::dialog::Dialog;
        use crate::screen::Buffer;
        use crate::theme::Theme;
        use crate::view::{DrawCtx, Rect, View};
        use crate::widgets::{
            Button, ButtonFlags, CheckBoxes, InputLine, Label, LimitMode, THistory,
        };

        let mut d = Dialog::new(Rect::new(0, 0, 40, 16), Some("Replace".into()));
        {
            let opts = &mut d.state_mut().options;
            opts.center_x = true;
            opts.center_y = true;
        }

        let mut il1 = InputLine::new(Rect::new(3, 3, 34, 4), 81, None, LimitMode::MaxBytes);
        il1.set_value(FieldValue::Text("foo".into()));
        let find_id = d.insert_child(Box::new(il1));
        d.insert_child(Box::new(Label::new(
            Rect::new(2, 2, 15, 3),
            "~T~ext to find",
            Some(find_id),
        )));
        d.insert_child(Box::new(THistory::new(
            Rect::new(34, 3, 37, 4),
            find_id,
            10,
        )));

        let mut il2 = InputLine::new(Rect::new(3, 6, 34, 7), 81, None, LimitMode::MaxBytes);
        il2.set_value(FieldValue::Text("bar".into()));
        let replace_id = d.insert_child(Box::new(il2));
        d.insert_child(Box::new(Label::new(
            Rect::new(2, 5, 12, 6),
            "~N~ew text",
            Some(replace_id),
        )));
        d.insert_child(Box::new(THistory::new(
            Rect::new(34, 6, 37, 7),
            replace_id,
            11,
        )));

        let mut cb = CheckBoxes::new(
            Rect::new(3, 8, 37, 12),
            vec![
                "~C~ase sensitive".into(),
                "~W~hole words only".into(),
                "~P~rompt on replace".into(),
                "~R~eplace all".into(),
            ],
        );
        cb.cluster.value = 0x0005; // case + prompt pre-selected
        d.insert_child(Box::new(cb));

        d.insert_child(Box::new(Button::new(
            Rect::new(17, 13, 27, 15),
            "O~K~",
            crate::command::Command::OK,
            ButtonFlags {
                default: true,
                ..Default::default()
            },
        )));
        d.insert_child(Box::new(Button::new(
            Rect::new(28, 13, 38, 15),
            "Cancel",
            crate::command::Command::CANCEL,
            ButtonFlags::new(),
        )));

        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(40, 16);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = d.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            d.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    // -- internal-clipboard tests ---------------------------------------------

    /// `insert_from` inserts another editor's bytes into self (non-clipboard dest).
    #[test]
    fn insert_from_copies_bytes() {
        let mut a = ed();
        insert(&mut a, "hello");
        // Select the "hello" content.
        a.set_select(0, 5, false);
        let data = a.selection_bytes();
        assert_eq!(data, b"hello");

        let mut b = ed();
        let mut cx = Cx::new();
        {
            let mut ctx = cx.ctx();
            let ok = b.insert_from(&data, &mut ctx);
            assert!(ok);
        }
        assert_eq!(text(&b), "hello");
    }

    /// When `is_clipboard = true`, `insert_from` selects the inserted content.
    #[test]
    fn clipboard_editor_receive_selects_inserted_text() {
        let mut e = ed();
        e.is_clipboard = true;
        let mut cx = Cx::new();
        {
            let mut ctx = cx.ctx();
            e.insert_from(b"world", &mut ctx);
        }
        assert!(
            e.has_selection(),
            "clipboard editor selects after insert_from"
        );
        assert_eq!(e.selection_bytes(), b"world");
    }

    /// `clip_copy` with an internal clipboard queues `ClipboardEditorReceive`, not `SetClipboard`.
    #[test]
    fn clip_copy_internal_routes_to_deferred_receive() {
        use crate::view::ViewId;
        // Mint a fake clipboard editor id.
        let fake_clipboard_id = ViewId::next();
        let mut e = ed();
        insert(&mut e, "hello");
        e.set_select(0, 5, false);
        let mut cx = Cx::new();
        {
            let mut ctx = cx.ctx();
            ctx.set_clipboard_snapshot(Some(fake_clipboard_id), false);
            assert!(e.clip_copy(&mut ctx));
        }
        assert!(
            cx.deferred.iter().any(|d| matches!(
                d,
                Deferred::ClipboardEditorReceive { clipboard_id, data }
                if *clipboard_id == fake_clipboard_id && data == b"hello"
            )),
            "clip_copy with internal clipboard queues ClipboardEditorReceive"
        );
        assert!(
            !cx.deferred
                .iter()
                .any(|d| matches!(d, Deferred::SetClipboard(..))),
            "clip_copy with internal clipboard must NOT queue SetClipboard"
        );
    }

    /// `clip_paste` with an internal clipboard queues `ClipboardEditorPaste`, not `EditorPaste`.
    #[test]
    fn clip_paste_internal_routes_to_deferred_paste() {
        use crate::view::ViewId;
        let fake_clipboard_id = ViewId::next();
        // The editor must have an id to be addressable.
        let mut group = Group::new(Rect::new(0, 0, 40, 10));
        let id = group.insert(Box::new(ed()));
        let mut cx = Cx::new();
        {
            let mut ctx = cx.ctx();
            ctx.set_clipboard_snapshot(Some(fake_clipboard_id), true);
            let v = group.find_mut(id).unwrap();
            let e = v.as_any_mut().unwrap().downcast_mut::<Editor>().unwrap();
            e.clip_paste(&mut ctx);
        }
        assert!(
            cx.deferred.iter().any(|d| matches!(
                d,
                Deferred::ClipboardEditorPaste { dest_id, clipboard_id }
                if *dest_id == id && *clipboard_id == fake_clipboard_id
            )),
            "clip_paste with internal clipboard queues ClipboardEditorPaste"
        );
        assert!(
            !cx.deferred
                .iter()
                .any(|d| matches!(d, Deferred::EditorPaste(..))),
            "clip_paste with internal clipboard must NOT queue EditorPaste"
        );
    }

    /// The clipboard editor skips cut/copy/paste commands in `update_commands`.
    #[test]
    fn update_commands_clipboard_editor_skips_cut_copy_paste() {
        let mut e = ed();
        e.is_clipboard = true;
        // Give it a selection (so it WOULD enable CUT/COPY/CLEAR if it weren't a clipboard).
        e.state.state.active = true;
        insert(&mut e, "abc");
        e.set_select(0, 3, false);
        let mut cx = Cx::new();
        {
            let mut ctx = cx.ctx();
            e.update_commands(&mut ctx);
        }
        // CUT and COPY should NOT appear in deferred (skipped for clipboard editor).
        assert!(
            !cx.deferred.iter().any(|d| matches!(
                d,
                Deferred::EnableCommand(c) if *c == Command::CUT || *c == Command::COPY || *c == Command::PASTE
            )),
            "clipboard editor must not enable CUT, COPY, or PASTE"
        );
        // CLEAR should still be enabled (has_selection AND active).
        assert!(
            cx.deferred.iter().any(|d| matches!(
                d,
                Deferred::EnableCommand(c) if *c == Command::CLEAR
            )),
            "CLEAR should be enabled for active clipboard editor with selection"
        );
    }
}
