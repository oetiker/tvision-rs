//! `TEditor` — faithful Rust port of `teditor1.cpp` / `teditor2.cpp` /
//! `edits.cpp` (row 66, FOUNDATION).
//!
//! `TEditor` is the gap-buffer text editor: a leaf [`View`] holding a single
//! `Vec<u8>` split by a movable gap, with a cursor (`cur_ptr`), a selection
//! (`sel_start`/`sel_end`), single-level undo, and a viewport (`delta`) onto a
//! content extent (`limit`). It references two sibling scrollbars and an
//! indicator on the window frame.
//!
//! ## The ctx-threading split (the central seam)
//!
//! In C++, `update(flags)` flushes inline (`doUpdate`) whenever `lockCount == 0`,
//! including from the ctor (`setBufLen(0) → update`). We cannot flush in the ctor
//! (no [`Context`]). So we **split flag-set from flush**:
//!
//! * The **core editing methods take no `Context`** — they mutate logical state
//!   and only OR bits into [`update_flags`](Editor::update_flags). [`update`]
//!   never flushes inline.
//! * `&mut Context` is threaded **only** into [`do_update`], [`unlock`],
//!   `handle_event`, `set_state`, and the public ctx-taking entries
//!   (`apply_scroll_delta`, `insert_text`). `unlock` flushes when `lock_count`
//!   hits 0; `handle_event` flushes at the end for the arms that ran unlocked.
//! * `change_bounds` is geometry-only + a `delta` clamp + `update(ufView)` —
//!   scrollbar params republish on the next flush (mirrors `TScroller`'s seam).
//!
//! This makes the entire gap-buffer + edit core ctx-free and unit-testable in
//! isolation, where the real oracle (logical buffer state) lives.
//!
//! ## D3 — leaf view, siblings by `ViewId` via pump brokers
//!
//! The editor holds [`h_scroll_bar`](Editor::h_scroll_bar)/`v_scroll_bar`/
//! `indicator` as [`Option<ViewId>`], never pointers. Every cross-view read/write
//! is deferred to the pump: `SyncEditorDelta` (read bar values into `delta`),
//! `ScrollBarSetParams` (publish range/value), `IndicatorSetValue`, `SetVisible`.
//! Mirrors `TScroller`'s broker; the editor is its own concrete downcast target
//! (it is *not* a `Scroller`).
//!
//! ## D13 — grapheme text via [`crate::text`]
//!
//! `nextChar`/`prevChar`/`formatLine`/`nextCharAndPos` read logical bytes across
//! the gap into a small `Vec<u8>` (the `getText` port), `str`-decode the
//! contiguous slice, and run [`text::next`]/[`text::prev`]. There is no
//! `maxCharSize` stack buffer.
//!
//! ## Deferrals (breadcrumbed in the code)
//!
//! * Find/Replace **dialogs** (`editorDialog`, `find`/`replace`/prompt) — needs
//!   dialog views not yet built. [`Editor::search`] is fully ported + unit-tested.
//! * Mouse **drag-select / edge-scroll / wheel / middle-button pan** — single-click
//!   cursor positioning is kept; the loops become a `DragCapture` (TODO).
//! * Right-click **context menu** (`initContextMenu`/`popupMenu`).
//! * Internal-clipboard **TEditor branch** (`insertFrom`) — row 69.
//! * `TStreamable` (D12).

use crate::theme::Role;
use crate::view::{
    Context, DrawCtx, GrowMode, Options, Point, Rect, StateFlag, View, ViewId, ViewState,
};

// ---------------------------------------------------------------------------
// module-private flag constants (kept off Command — these are bit words)
// ---------------------------------------------------------------------------

/// `ufUpdate` — redraw the indicator/scrollbars/cursor only (no text repaint).
const UF_UPDATE: u8 = 0x01;
/// `ufLine` — repaint just the current line.
const UF_LINE: u8 = 0x02;
/// `ufView` — repaint the whole view.
const UF_VIEW: u8 = 0x04;

/// `smExtend` — extend the current selection to the new cursor position.
const SM_EXTEND: u8 = 0x01;
/// `smDouble` — word-granular selection (double-click).
const SM_DOUBLE: u8 = 0x02;
/// `smTriple` — line-granular selection (triple-click).
const SM_TRIPLE: u8 = 0x04;

// The `ef*` search-option flags. `pub(crate)` (the `editor` module is private and
// only `Editor`/`Encoding`/`LineEnding` are re-exported; `search()` takes a plain
// `opts: u16`). Promoted to `pub` when the find/replace dialogs (deferred) need
// them.
/// `efCaseSensitive` — `search` matches case exactly.
pub(crate) const EF_CASE_SENSITIVE: u16 = 0x0001;
/// `efWholeWordsOnly` — `search` rejects matches inside a larger word.
pub(crate) const EF_WHOLE_WORDS_ONLY: u16 = 0x0002;
// The remaining `ef*` flags drive the find/replace DIALOGS, which are deferred
// (row 66 find/replace dialog). Kept as the faithful constant family for the
// future consumer; `#[allow(dead_code)]` until then.
/// `efPromptOnReplace` — replace prompts before each substitution (dialog; deferred).
#[allow(dead_code)]
pub(crate) const EF_PROMPT_ON_REPLACE: u16 = 0x0004;
/// `efReplaceAll` — replace every match (dialog; deferred).
#[allow(dead_code)]
pub(crate) const EF_REPLACE_ALL: u16 = 0x0008;
/// `efDoReplace` — the operation is a replace, not a find (dialog; deferred).
#[allow(dead_code)]
pub(crate) const EF_DO_REPLACE: u16 = 0x0010;
/// `efBackupFiles` — write a backup file on save (file editor; deferred).
#[allow(dead_code)]
pub(crate) const EF_BACKUP_FILES: u16 = 0x0100;

/// `maxLineLength` — the fixed `limit.x` content width (editors.h).
const MAX_LINE_LENGTH: i32 = 256;

/// `sfSearchFailed` sentinel — `scan`/`iScan` "not found". C++ uses `(uint)-1`.
const SEARCH_FAILED: usize = usize::MAX;

// ---------------------------------------------------------------------------
// Line ending / encoding enums (TEditor::LineEndingType / Encoding)
// ---------------------------------------------------------------------------

/// `TEditor::LineEndingType` — how line breaks are stored when text is inserted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    /// `eolCrLf` — `"\r\n"`.
    CrLf,
    /// `eolLf` — `"\n"`.
    Lf,
    /// `eolCr` — `"\r"`.
    Cr,
}

impl LineEnding {
    /// The byte sequence this line ending writes (`TEditor::getLineEnding`).
    fn bytes(self) -> &'static [u8] {
        match self {
            LineEnding::Lf => b"\n",
            LineEnding::Cr => b"\r",
            LineEnding::CrLf => b"\r\n",
        }
    }
}

/// `TEditor::Encoding` — how multibyte characters are stepped over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    /// `encDefault` — width-aware (grapheme) stepping.
    Default,
    /// `encSingleByte` — every byte is one column.
    SingleByte,
}

/// `TEditor::defaultLineEndingType`. C++ uses `eolCrLf` on DOS and `eolLf`
/// elsewhere; we pick [`LineEnding::Lf`] (the modern-host default).
const DEFAULT_LINE_ENDING: LineEnding = LineEnding::Lf;

// ---------------------------------------------------------------------------
// getCharType / isWordBoundary / isWordChar (teditor2.cpp)
// ---------------------------------------------------------------------------

/// `getCharType` (teditor2.cpp) — word-boundary classification.
fn get_char_type(ch: u8) -> u8 {
    match ch {
        b'\t' | b' ' | 0 => 0,
        b'\n' | b'\r' => 1,
        b'!'..=b'/' | b':'..=b'@' | b'['..=b'`' | b'{'..=b'~' => 2,
        _ => 3,
    }
}

/// `isWordBoundary(a, b)` — true when `a` and `b` are different char types.
fn is_word_boundary(a: u8, b: u8) -> bool {
    get_char_type(a) != get_char_type(b)
}

/// `isWordChar(ch)` — true unless `ch` is whitespace/punctuation (the
/// `" !\"#$%&'()*+,-./:;<=>?@[\\]^`{|}~\0"` set in teditor2.cpp).
fn is_word_char(ch: u8) -> bool {
    !matches!(
        ch,
        b' ' | b'!'..=b'/' | b':'..=b'@' | b'['..=b'`' | b'{'..=b'~' | 0
    )
}

/// `countLines(buf, count)` (edits.cpp) — number of line breaks in `buf`,
/// counting `\r\n` as one.
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

/// `scan(block, size, str)` (edits.cpp) — case-sensitive substring search.
/// Returns the byte offset of the first match, or [`SEARCH_FAILED`].
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

/// `iScan(block, size, str)` (edits.cpp) — case-insensitive substring search.
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

/// `TEditor` — the gap-buffer text editor (D2 View trait + ViewState).
pub struct Editor {
    /// View state (geometry, flags, etc.) — the D2 composition target.
    state: ViewState,
    /// The text buffer: `buf_len` logical bytes split by a `gap_len`-byte gap at
    /// `cur_ptr`. Always physically `buf_size` bytes long; gap bytes are stale.
    buffer: Vec<u8>,
    /// `bufSize` — physical buffer capacity (never grows in the base; see
    /// [`set_buf_size`](Editor::set_buf_size)).
    buf_size: usize,
    /// `bufLen` — logical text length. Invariant: `buf_len + gap_len == buf_size`.
    buf_len: usize,
    /// `gapLen` — gap size at `cur_ptr`.
    gap_len: usize,
    /// `selStart` — selection start (logical offset).
    sel_start: usize,
    /// `selEnd` — selection end (logical offset).
    sel_end: usize,
    /// `curPtr` — cursor position (logical offset); the gap sits here physically.
    cur_ptr: usize,
    /// `curPos` — cursor `(col, row)` in display coordinates.
    cur_pos: Point,
    /// `delta` — viewport top-left (scroll offset) in display coordinates.
    delta: Point,
    /// `limit` — content extent `(x = maxLineLength, y = line count)`.
    limit: Point,
    /// `drawLine` — the display row `draw_ptr` corresponds to.
    draw_line: i32,
    /// `drawPtr` — logical offset of the start of line `draw_line`.
    draw_ptr: usize,
    /// `delCount` — bytes deleted since the last undo checkpoint (undo accounting).
    del_count: usize,
    /// `insCount` — bytes inserted since the last undo checkpoint.
    ins_count: usize,
    /// `isValid` — buffer allocation succeeded.
    is_valid: bool,
    /// `canUndo` — undo is enabled (always true in the base editor).
    can_undo: bool,
    /// `modified` — the buffer has unsaved changes.
    modified: bool,
    /// `selecting` — a persistent selection is in progress (`startSelect`).
    selecting: bool,
    /// `overwrite` — overwrite (vs insert) mode.
    overwrite: bool,
    /// `autoIndent` — replicate leading whitespace on Enter.
    auto_indent: bool,
    /// `lockCount` — nested update locks; flush happens when this returns to 0.
    lock_count: u8,
    /// `updateFlags` — pending `uf*` redraw flags.
    update_flags: u8,
    /// `keyState` — the two-key prefix machine (0 = none, 1 = Ctrl-Q, 2 = Ctrl-K).
    key_state: i32,
    /// `lineEndingType`.
    line_ending: LineEnding,
    /// `encoding`.
    encoding: Encoding,
    /// `hScrollBar` — by id (`None` = absent).
    h_scroll_bar: Option<ViewId>,
    /// `vScrollBar` — by id (`None` = absent).
    v_scroll_bar: Option<ViewId>,
    /// `indicator` — by id (`None` = absent).
    indicator: Option<ViewId>,
    /// `findStr` — last search string.
    ///
    /// NOTE: C++ shares this statically across all editors; per-instance until the
    /// find/replace dialogs (deferred) need otherwise.
    find_str: String,
    /// `replaceStr` — last replacement string (per-instance; see [`find_str`]).
    /// Read only by the deferred find/replace dialog path (`doSearchReplace`'s
    /// `insertText(replaceStr, …)`), so unused until then.
    #[allow(dead_code)]
    replace_str: String,
    /// `editorFlags` — the `ef*` search options (per-instance; see [`find_str`]).
    editor_flags: u16,
}

impl Editor {
    /// `TEditor::TEditor(bounds, hScrollBar, vScrollBar, indicator, bufSize)`.
    ///
    /// Faithful ctor: `growMode = gfGrowHiX | gfGrowHiY`, `options |=
    /// ofSelectable`, `showCursor`, `initBuffer`, `setBufLen(0)`. The C++ flush in
    /// `setBufLen → update` is dropped (no `Context`); initial state is consistent
    /// for `draw`, and scrollbar params publish on the first flush.
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
        // initBuffer: `buffer = new char[bufSize]`. We keep it physically full.
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
            modified: false,
            selecting: false,
            overwrite: false,
            auto_indent: true,
            lock_count: 0,
            update_flags: 0,
            key_state: 0,
            line_ending: DEFAULT_LINE_ENDING,
            encoding: Encoding::Default,
            h_scroll_bar,
            v_scroll_bar,
            indicator,
            find_str: String::new(),
            replace_str: String::new(),
            editor_flags: 0,
        };
        // setBufLen(0) — flag-set only (no flush; ctor has no Context).
        ed.set_buf_len(0);
        ed
    }

    // -- test/inspection accessors ------------------------------------------

    /// Logical text length (`bufLen`).
    pub fn buf_len(&self) -> usize {
        self.buf_len
    }

    /// The cursor position (`curPtr`).
    pub fn cur_ptr(&self) -> usize {
        self.cur_ptr
    }

    /// The content extent (`limit`).
    pub fn limit(&self) -> Point {
        self.limit
    }

    /// The scroll offset (`delta`).
    pub fn delta(&self) -> Point {
        self.delta
    }

    /// Whether the buffer has unsaved changes (`modified`).
    pub fn modified(&self) -> bool {
        self.modified
    }

    /// Reconstruct the logical text (test oracle): `bufChar(0..buf_len)`.
    pub fn text(&self) -> Vec<u8> {
        (0..self.buf_len).map(|p| self.buf_char(p)).collect()
    }

    // -- gap arithmetic (edits.cpp) -----------------------------------------

    /// `bufPtr(P)` — physical index of logical offset `P`.
    fn buf_ptr(&self, p: usize) -> usize {
        if p < self.cur_ptr {
            p
        } else {
            p + self.gap_len
        }
    }

    /// `bufChar(P)` — the byte at logical offset `P`.
    fn buf_char(&self, p: usize) -> u8 {
        self.buffer[self.buf_ptr(p)]
    }

    /// `getText(p, dest)` — copy up to `dest.len()` logical bytes from `p` into
    /// `dest`; returns the count copied. Used to materialize a contiguous slice
    /// across the gap for grapheme decoding.
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

    // -- character navigation (edits.cpp) -----------------------------------

    /// `nextChar(P)` — advance one grapheme (or `\r\n` pair, or one byte if
    /// `encSingleByte`).
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

    /// `prevChar(P)` — retreat one grapheme (or `\r\n` pair, or one byte).
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
            // p == 0 or p == 1 → 0 (C++ returns 0 for P <= 1).
            0
        }
    }

    /// `nextCharAndPos(p, pos)` — advance `p` over one char and `pos` over its
    /// display width (tabs round up to the next multiple of 8). Returns false at
    /// end of buffer.
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

    /// `charPos(p, target)` — display column of `target`, scanning from `p`.
    fn char_pos(&self, mut p: usize, target: usize) -> i32 {
        let mut pos = 0;
        while p < target {
            if !self.next_char_and_pos(&mut p, &mut pos) {
                break;
            }
        }
        pos
    }

    /// `charPtr(p, target)` — logical offset at display column `target` on the
    /// line starting at `p` (stops at a line break).
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

    /// `lineEnd(P)` — offset of the first line break at or after `P` (or buf_len).
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

    /// `lineStart(P)` — offset of the start of the line containing `P`.
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

    /// `nextLine(P)` — start of the line after `P`.
    fn next_line(&self, p: usize) -> usize {
        self.next_char(self.line_end(p))
    }

    /// `prevLine(P)` — start of the line before `P`.
    fn prev_line(&self, p: usize) -> usize {
        self.line_start(self.prev_char(p))
    }

    /// `nextWord(P)` — start of the next word.
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

    /// `prevWord(P)` — start of the previous word.
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

    /// `indentedLineStart(P)` — first non-whitespace offset on `P`'s line (or the
    /// line start if that equals `P`).
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

    /// `lineMove(p, count)` — move `count` display lines, preserving column.
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

    /// `getMousePtr(m)` — logical offset under the (global) mouse point.
    fn get_mouse_ptr(&self, mouse_local: Point) -> usize {
        let mx = mouse_local.x.max(0).min(self.state.size.x - 1);
        let my = mouse_local.y.max(0).min(self.state.size.y - 1);
        self.char_ptr(
            self.line_move(self.draw_ptr, my + self.delta.y - self.draw_line),
            mx + self.delta.x,
        )
    }

    // -- selection / cursor (teditor2.cpp) ----------------------------------

    /// `hasSelection()`.
    fn has_selection(&self) -> bool {
        self.sel_start != self.sel_end
    }

    /// `detectLineEndingType()` — infer the line ending from the first break.
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

    /// `lengthWithConvertedLineEndings(p, length)` — length after rewriting all
    /// breaks in `p[..length]` to [`line_ending`](Editor::line_ending).
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

    /// `copyAndConvertLineEndings(dest, src)` — write `src` into `dest` with
    /// converted breaks. Writes exactly
    /// `length_with_converted_line_endings(src)` bytes starting at `dest_off`.
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

    /// `setBufLen(length)` — reset the buffer to `length` logical bytes (the gap
    /// is everything after), zero cursor/selection/scroll, recompute `limit.y`.
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

    /// `setBufSize(newSize)` — base editor never grows: succeeds iff `newSize`
    /// already fits.
    fn set_buf_size(&self, new_size: usize) -> bool {
        new_size <= self.buf_size
    }

    /// `setSelect(newStart, newEnd, curStart)` — move the gap to the chosen
    /// endpoint and set the selection. The gap memmove is the load-bearing op.
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
                // memmove(&buffer[curPtr], &buffer[curPtr+gapLen], l)
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
                // memmove(&buffer[curPtr+gapLen], &buffer[curPtr], l)
                let dst = self.cur_ptr + self.gap_len;
                self.buffer.copy_within(self.cur_ptr..self.cur_ptr + l, dst);
            }
            self.del_count = 0;
            self.ins_count = 0;
            // setBufSize(bufLen) — no-op for the base (never shrinks).
        }
        self.draw_line = self.cur_pos.y;
        self.draw_ptr = self.line_start(p);
        self.cur_pos.x = self.char_pos(self.draw_ptr, p);
        self.sel_start = new_start;
        self.sel_end = new_end;
        self.update(flags);
    }

    /// `setCurPtr(p, selectMode)` — move the cursor to `p`, optionally extending
    /// (and snapping to word/line granularity).
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

    /// `startSelect()` — begin a persistent selection.
    fn start_select(&mut self) {
        self.hide_select();
        self.selecting = true;
    }

    /// `hideSelect()` — collapse the selection to the cursor.
    fn hide_select(&mut self) {
        self.selecting = false;
        self.set_select(self.cur_ptr, self.cur_ptr, false);
    }

    /// `toggleInsMode()` — flip overwrite mode + the block-cursor flag.
    fn toggle_ins_mode(&mut self) {
        self.overwrite = !self.overwrite;
        let ins = self.state.state.cursor_ins;
        self.state.state.cursor_ins = !ins;
    }

    /// `toggleEncoding()` — flip the multibyte/single-byte encoding.
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

    // -- insertion / deletion (teditor2.cpp) --------------------------------

    /// `insertBuffer(p, offset, length, allowUndo, selectText)` — THE core edit.
    ///
    /// `p` must **not** alias `self.buffer` (callers snapshot first; the base never
    /// reallocates, so the C++ `p -= ptrdiff_t(buffer)` fixup is dropped).
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
        // isClipboard() == false → modified = true (the static clipboard editor is
        // null in this row, so we are never the clipboard).
        self.modified = true;
        // setBufSize(bufLen + delCount) — no-op for the base.
        if sel_lines == 0 && lines == 0 {
            self.update(UF_LINE);
        } else {
            self.update(UF_VIEW);
        }
        true
    }

    /// `insertText(text, length, selectText)` — public insert (ctx-free core).
    fn insert_text_core(&mut self, text: &[u8], select_text: bool) -> bool {
        self.insert_buffer(text, 0, text.len(), self.can_undo, select_text)
    }

    /// `deleteSelect()` — delete the current selection.
    fn delete_select(&mut self) {
        self.insert_buffer(&[], 0, 0, self.can_undo, false);
    }

    /// `deleteRange(startPtr, endPtr, delSelect)` — delete a range, honoring an
    /// existing selection when `del_select`.
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

    /// `newLine()` — insert a line break with optional auto-indent.
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
            // self.buffer). The run is physically contiguous (it precedes curPtr,
            // and after the "\n" insert the gap is at the new curPtr, past it).
            let indent: Vec<u8> = (p..p + (i - p)).map(|q| self.buf_char(q)).collect();
            self.insert_text_core(&indent, false);
        }
    }

    /// `undo()` — single-level undo (restore the deleted text, drop the inserted).
    fn undo(&mut self) {
        if self.del_count != 0 || self.ins_count != 0 {
            self.sel_start = self.cur_ptr - self.ins_count;
            self.sel_end = self.cur_ptr;
            let length = self.del_count;
            self.del_count = 0;
            self.ins_count = 0;
            // Source = the deleted text, which lives in the gap at
            // [curPtr+gapLen-length .. curPtr+gapLen). Snapshot first (no alias).
            let start = self.cur_ptr + self.gap_len - length;
            let snapshot: Vec<u8> = self.buffer[start..start + length].to_vec();
            self.insert_buffer(&snapshot, 0, length, false, true);
        }
    }

    // -- search (teditor2.cpp / edits.cpp) ----------------------------------

    /// `TEditor::search(findStr, opts)` — find `needle` from the cursor; on a hit,
    /// select it and track the cursor. Fully ported + unit-tested (the dialog-
    /// driven `find`/`replace` are deferred).
    ///
    /// Ctx-free: the C++ `lock`/`trackCursor`/`unlock` flush is replaced by a
    /// flag-set (`trackCursor` records the scroll target; the flush happens on the
    /// next `handle_event` boundary).
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

    // -- viewport (teditor2.cpp) --------------------------------------------

    /// `cursorVisible()`.
    fn cursor_visible(&self) -> bool {
        self.cur_pos.y >= self.delta.y && self.cur_pos.y < self.delta.y + self.state.size.y
    }

    /// `scrollTo(x, y)` — set `delta` (clamped) and flag a redraw.
    fn scroll_to(&mut self, x: i32, y: i32) {
        let x = 0.max(x.min(self.limit.x - self.state.size.x));
        let y = 0.max(y.min(self.limit.y - self.state.size.y));
        if x != self.delta.x || y != self.delta.y {
            self.delta.x = x;
            self.delta.y = y;
            self.update(UF_VIEW);
        }
    }

    /// `trackCursor(center)` — scroll so the cursor is visible.
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

    /// `checkScrollBar` body — applied by the pump after reading bar values.
    ///
    /// Public ctx-taking entry: the pump reads each scrollbar's `value`, then calls
    /// this with `dx`/`dy` (`None` = no bar). For each present bar, if its value
    /// differs from `delta`, adopt it and flag a `ufView` redraw; then flush.
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

    /// `insertText` — public ctx-taking entry used by the clipboard-paste broker.
    /// Inserts then flushes (the flush republishes scrollbar params next pump).
    pub fn insert_text(&mut self, text: &[u8], select_text: bool, ctx: &mut Context) {
        self.lock();
        self.insert_text_core(text, select_text);
        let center = !self.cursor_visible();
        self.track_cursor(center);
        self.unlock(ctx);
    }

    // -- update / lock / flush (teditor2.cpp) -------------------------------

    /// `update(flags)` — flag-set only (no inline flush; see the module seam).
    fn update(&mut self, flags: u8) {
        self.update_flags |= flags;
    }

    /// `lock()`.
    fn lock(&mut self) {
        self.lock_count += 1;
    }

    /// `unlock()` — decrement; flush when the count returns to 0.
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

    /// `doUpdate()` — publish cursor, scrollbar params, and the indicator value if
    /// any update is pending. The C++ inline `drawView`/`drawLines` are dropped
    /// (D8 whole-tree redraw).
    fn do_update(&mut self, ctx: &mut Context) {
        if self.update_flags == 0 {
            return;
        }
        // setCursor(curPos.x - delta.x, curPos.y - delta.y)
        self.state
            .set_cursor(self.cur_pos.x - self.delta.x, self.cur_pos.y - self.delta.y);
        // drawView / drawLines: dropped (whole-tree redraw).
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

    /// `setCmdState(command, enable)` — enable iff `enable && active`, else disable.
    fn set_cmd_state(&self, command: crate::command::Command, enable: bool, ctx: &mut Context) {
        if enable && self.state.state.active {
            ctx.enable_command(command);
        } else {
            ctx.disable_command(command);
        }
    }

    /// `updateCommands()` — gray/ungray the editing commands by current state.
    fn update_commands(&self, ctx: &mut Context) {
        use crate::command::Command;
        let has_undo = self.del_count != 0 || self.ins_count != 0;
        self.set_cmd_state(Command::UNDO, has_undo, ctx);
        // isClipboard() == false (the static clipboard is null in this row).
        let has_sel = self.has_selection();
        self.set_cmd_state(Command::CUT, has_sel, ctx);
        self.set_cmd_state(Command::COPY, has_sel, ctx);
        // clipboard == 0 → paste always enabled.
        self.set_cmd_state(Command::PASTE, true, ctx);
        self.set_cmd_state(Command::CLEAR, has_sel, ctx);
        self.set_cmd_state(Command::FIND, true, ctx);
        self.set_cmd_state(Command::REPLACE, true, ctx);
        self.set_cmd_state(Command::SEARCH_AGAIN, true, ctx);
    }

    // -- clipboard (teditor1.cpp; D11 system-clipboard path) ----------------

    /// `clipCopy()` — copy the selection to the system clipboard.
    fn clip_copy(&mut self, ctx: &mut Context) -> bool {
        // TODO(row 69): internal-clipboard TEditor branch (insertFrom) — null in
        // row 66, system-clipboard path only.
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

    /// `clipCut()` — copy then delete.
    fn clip_cut(&mut self, ctx: &mut Context) {
        if self.clip_copy(ctx) {
            self.delete_select();
        }
    }

    /// `clipPaste()` — request the system-clipboard text be inserted (deferred).
    fn clip_paste(&mut self, ctx: &mut Context) {
        // TODO(row 69): internal-clipboard TEditor branch (insertFrom) — null in
        // row 66, system-clipboard path only.
        if let Some(id) = self.state.id() {
            ctx.editor_paste(id);
        }
    }

    // -- find/replace dialogs (DEFERRED) ------------------------------------

    /// `doSearchReplace()` — repeated find/replace. The dialog-driven prompt is
    /// stubbed (no `editorDialog`); with an empty `find_str` this is inert.
    fn do_search_replace(&mut self) {
        // TODO(row 66 find/replace dialog): needs editorDialog + std find/replace
        // dialogs. The replace loop + efPromptOnReplace prompt are deferred; with
        // an empty find_str, search returns false and this is a no-op.
        let _ = self.search(&self.find_str.clone(), self.editor_flags);
    }

    // -- formatLine / draw (edits.cpp / teditor1.cpp) -----------------------

    /// `getColorAt(P)` — selected role inside the selection, else normal.
    fn color_at(&self, p: usize) -> Role {
        if self.sel_start <= p && p < self.sel_end {
            Role::ScrollerSelected
        } else {
            Role::ScrollerNormal
        }
    }

    /// `formatLine(b, linePtr, hScroll, width)` — render one display row into the
    /// row at view-local `y`.
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

    /// `drawLines(y, count, linePtr)` — render `count` rows from `line_ptr`.
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

    /// `TEditor::draw` — recompute `draw_ptr` for `delta.y`, then render the
    /// viewport rows. NB: `draw_ptr`/`draw_line` are display caches; mutating them
    /// in `draw` is faithful to the C++ (it does the same).
    fn draw(&mut self, ctx: &mut DrawCtx) {
        if self.draw_line != self.delta.y {
            self.draw_ptr = self.line_move(self.draw_ptr, self.delta.y - self.draw_line);
            self.draw_line = self.delta.y;
        }
        let count = self.state.size.y;
        let draw_ptr = self.draw_ptr;
        self.draw_lines(ctx, 0, count, draw_ptr);
    }

    /// `TEditor::handleEvent` — keyboard editing, command dispatch, single-click
    /// mouse positioning, and the scrollbar-changed broadcast.
    fn handle_event(&mut self, ev: &mut crate::event::Event, ctx: &mut Context) {
        use crate::command::Command;
        use crate::event::Event;

        let center_cursor = !self.cursor_visible();

        // selectMode: smExtend if a persistent selection is active or shift is held.
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

        // convertEvent: keymap + the Ctrl-K/Ctrl-Q two-key prefix. Transforms a
        // KeyDown into a Command in place (or clears it for a prefix key).
        self.convert_event(ev);

        match ev {
            Event::MouseDown(m) => {
                let m = *m;
                if m.buttons.right {
                    // TODO(row 66 context menu): initContextMenu + popupMenu.
                    ev.clear();
                    return;
                }
                if m.buttons.middle {
                    // TODO(row 66 mouse drag-select, D9): port as a DragCapture
                    // capture-handler (see window.rs DragCapture) — extend
                    // selection on MouseMove, edge-scroll on MouseAuto,
                    // wheel→scrollbars; deferred like scrollbar's own drag (TODO
                    // row 31). Middle-button pan deferred with the same handler.
                    ev.clear();
                    return;
                }
                if m.flags.double_click {
                    select_mode |= SM_DOUBLE;
                } else if m.flags.triple_click {
                    select_mode |= SM_TRIPLE;
                }
                // Single-click cursor positioning (the inner mouse loop is the
                // TODO above). Position is already view-local (Group::deliver
                // makeLocal'd it).
                self.lock();
                let ptr = self.get_mouse_ptr(m.position);
                self.set_cur_ptr(ptr, select_mode);
                self.unlock(ctx);
            }
            Event::KeyDown(k) => {
                let k = *k;
                // Insertable character? (printable, or tab). Faithful to the C++
                // charCode 9 / [32,255) gate, decomposed to our Key model.
                let insertable = match k.key {
                    crate::event::Key::Char(_) if !k.modifiers.ctrl && !k.modifiers.alt => true,
                    crate::event::Key::Tab if !k.modifiers.ctrl && !k.modifiers.alt => true,
                    _ => false,
                };
                if insertable {
                    self.lock();
                    // TODO(row 66): kbPaste / textLength / bracketed-paste multi-char
                    // not in the event model yet — single-char insert only.
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
            Event::Command(cmd) => {
                let cmd = *cmd;
                match cmd {
                    Command::FIND | Command::REPLACE | Command::SEARCH_AGAIN => {
                        // TODO(row 66 find/replace dialog): needs editorDialog + std
                        // find/replace dialogs. cmSearchAgain runs doSearchReplace
                        // (inert with empty find_str); cmFind/cmReplace need dialogs.
                        if cmd == Command::SEARCH_AGAIN {
                            self.do_search_replace();
                            self.flush_if_unlocked(ctx);
                        }
                        // cmFind / cmReplace: no-op (dialogs deferred).
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
                // DEVIATION from C++ `clearEvent`: the row-27 TScroller (the direct
                // analogue) deliberately does NOT clear cmScrollBarChanged — the
                // codebase convention is to leave broadcasts live for siblings.
                // Match it (return without clearing); functionally inert since the
                // broadcast only concerns this editor's own bar.
                return;
            }
            _ => return,
        }
        ev.clear();
    }

    /// `TEditor::setState` — after the base flips the flag, show/hide the
    /// scrollbars + indicator on `sfActive` and re-gray commands; on `sfExposed`
    /// the C++ unlocks (we have no exposed flag — see note).
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
            // updateCommands runs whenever active changes; flag a redraw so the
            // first flush publishes params/indicator too.
            self.update(UF_VIEW);
            self.update_commands(ctx);
            self.flush_if_unlocked(ctx);
        }
        // NOTE: the C++ `sfExposed` arm (`if (enable) unlock()`) has no analogue —
        // there is no sfExposed flag (D8 dropped it). The initial flush instead
        // happens on the first active/event boundary.
    }

    /// `TEditor::changeBounds` — geometry + clamp `delta` + flag a redraw.
    /// Scrollbar params republish on the next flush (mirrors `TScroller`'s seam).
    fn change_bounds(&mut self, bounds: Rect) {
        self.state.set_bounds(bounds);
        self.delta.x = 0.max(self.delta.x.min(self.limit.x - self.state.size.x));
        self.delta.y = 0.max(self.delta.y.min(self.limit.y - self.state.size.y));
        self.update(UF_VIEW);
    }

    /// `TEditor::valid` — the buffer allocated successfully.
    fn valid(&self, _cmd: crate::command::Command) -> bool {
        self.is_valid
    }

    /// Concrete-reach hatch: the pump downcasts to `&mut Editor` for the
    /// `SyncEditorDelta` / `EditorPaste` brokers.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
}

impl Editor {
    /// `convertEvent` — translate a `KeyDown` into a `Command` (or a cleared
    /// prefix), honoring the Ctrl-K / Ctrl-Q two-key prefix machine.
    fn convert_event(&mut self, ev: &mut crate::event::Event) {
        use crate::event::Event;
        if let Event::KeyDown(k) = ev {
            let k = *k;
            // shift+arrow charCode-zeroing: a no-op in our model (shift+arrow is a
            // non-Char Key, never insertable). TODO(row 66): charScan.scanCode
            // zeroing not representable; the simple path is already correct.
            let cmd = self.scan_key_map(self.key_state, k);
            self.key_state = 0;
            match cmd {
                KeyMapResult::Prefix(state) => {
                    self.key_state = state;
                    ev.clear();
                }
                KeyMapResult::Command(c) => {
                    *ev = Event::Command(c);
                }
                KeyMapResult::None => {
                    // Leave the event unchanged (an insertable char, or unhandled).
                }
            }
        }
    }

    /// `scanKeyMap` — resolve `key` against the keymap for `key_state`.
    fn scan_key_map(&self, key_state: i32, k: crate::event::KeyEvent) -> KeyMapResult {
        use crate::command::Command;
        use crate::event::Key;

        match key_state {
            0 => {
                // firstKeys table. Two-key prefixes: Ctrl-Q → state 1, Ctrl-K → 2.
                if k.modifiers.ctrl
                    && let Key::Char(c) = k.key
                {
                    let lc = c.to_ascii_lowercase();
                    return match lc {
                        'a' => KeyMapResult::Command(Command::SELECT_ALL),
                        'c' => KeyMapResult::Command(Command::PAGE_DOWN),
                        'd' => KeyMapResult::Command(Command::CHAR_RIGHT),
                        'e' => KeyMapResult::Command(Command::LINE_UP),
                        'f' => KeyMapResult::Command(Command::WORD_RIGHT),
                        'g' => KeyMapResult::Command(Command::DEL_CHAR),
                        'h' => KeyMapResult::Command(Command::BACK_SPACE),
                        'k' => KeyMapResult::Prefix(2),
                        'l' => KeyMapResult::Command(Command::SEARCH_AGAIN),
                        'm' => KeyMapResult::Command(Command::NEW_LINE),
                        'o' => KeyMapResult::Command(Command::INDENT_MODE),
                        'p' => KeyMapResult::Command(Command::ENCODING),
                        'q' => KeyMapResult::Prefix(1),
                        'r' => KeyMapResult::Command(Command::PAGE_UP),
                        's' => KeyMapResult::Command(Command::CHAR_LEFT),
                        't' => KeyMapResult::Command(Command::DEL_WORD),
                        'u' => KeyMapResult::Command(Command::UNDO),
                        'v' => KeyMapResult::Command(Command::INS_MODE),
                        'x' => KeyMapResult::Command(Command::LINE_DOWN),
                        'y' => KeyMapResult::Command(Command::DEL_LINE),
                        _ => KeyMapResult::None,
                    };
                }
                // Named keys + their Ctrl/Shift variants.
                match (k.key, k.modifiers.ctrl, k.modifiers.shift, k.modifiers.alt) {
                    (Key::Left, false, _, false) => KeyMapResult::Command(Command::CHAR_LEFT),
                    (Key::Right, false, _, false) => KeyMapResult::Command(Command::CHAR_RIGHT),
                    (Key::Backspace, _, _, true) => KeyMapResult::Command(Command::DEL_WORD_LEFT),
                    (Key::Backspace, true, _, _) => KeyMapResult::Command(Command::DEL_WORD_LEFT),
                    // Ctrl-Del → cmDelWord: firstKeys lists `kbCtrlDel, cmDelWord`
                    // (teditor1.cpp:71) BEFORE the dead `kbCtrlDel, cmClear` (:87),
                    // and scanKeyMap returns the FIRST match. So cmClear is
                    // unreachable from the keyboard — faithful.
                    (Key::Delete, true, _, _) => KeyMapResult::Command(Command::DEL_WORD),
                    (Key::Left, true, _, _) => KeyMapResult::Command(Command::WORD_LEFT),
                    (Key::Right, true, _, _) => KeyMapResult::Command(Command::WORD_RIGHT),
                    (Key::Home, false, _, _) => KeyMapResult::Command(Command::LINE_START),
                    (Key::End, false, _, _) => KeyMapResult::Command(Command::LINE_END),
                    (Key::Up, false, _, _) => KeyMapResult::Command(Command::LINE_UP),
                    (Key::Down, false, _, _) => KeyMapResult::Command(Command::LINE_DOWN),
                    (Key::PageUp, false, _, _) => KeyMapResult::Command(Command::PAGE_UP),
                    (Key::PageDown, false, _, _) => KeyMapResult::Command(Command::PAGE_DOWN),
                    (Key::Home, true, _, _) => KeyMapResult::Command(Command::TEXT_START),
                    (Key::End, true, _, _) => KeyMapResult::Command(Command::TEXT_END),
                    (Key::Insert, false, false, false) => KeyMapResult::Command(Command::INS_MODE),
                    (Key::Delete, false, false, false) => KeyMapResult::Command(Command::DEL_CHAR),
                    (Key::Insert, false, true, false) => KeyMapResult::Command(Command::PASTE),
                    (Key::Delete, false, true, false) => KeyMapResult::Command(Command::CUT),
                    (Key::Insert, true, false, false) => KeyMapResult::Command(Command::COPY),
                    (Key::Enter, _, _, _) => KeyMapResult::Command(Command::NEW_LINE),
                    _ => KeyMapResult::None,
                }
            }
            1 => {
                // quickKeys (Ctrl-Q prefix). Second key normalized to uppercase.
                if let Key::Char(c) = k.key {
                    return match c.to_ascii_uppercase() {
                        'A' => KeyMapResult::Command(Command::REPLACE),
                        'C' => KeyMapResult::Command(Command::TEXT_END),
                        'D' => KeyMapResult::Command(Command::LINE_END),
                        'F' => KeyMapResult::Command(Command::FIND),
                        'H' => KeyMapResult::Command(Command::DEL_START),
                        'R' => KeyMapResult::Command(Command::TEXT_START),
                        'S' => KeyMapResult::Command(Command::LINE_START),
                        'Y' => KeyMapResult::Command(Command::DEL_END),
                        _ => KeyMapResult::None,
                    };
                }
                KeyMapResult::None
            }
            2 => {
                // blockKeys (Ctrl-K prefix).
                if let Key::Char(c) = k.key {
                    return match c.to_ascii_uppercase() {
                        'B' => KeyMapResult::Command(Command::START_SELECT),
                        'C' => KeyMapResult::Command(Command::PASTE),
                        'H' => KeyMapResult::Command(Command::HIDE_SELECT),
                        'K' => KeyMapResult::Command(Command::COPY),
                        'Y' => KeyMapResult::Command(Command::CUT),
                        _ => KeyMapResult::None,
                    };
                }
                KeyMapResult::None
            }
            _ => KeyMapResult::None,
        }
    }

    /// The `evCommand` default-arm dispatch (the inner `switch` in handleEvent).
    /// Returns false for an unhandled command (the C++ `default: unlock; return`).
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

/// The result of resolving a key against the keymap (`scanKeyMap`'s return).
enum KeyMapResult {
    /// A resolved editor command.
    Command(crate::command::Command),
    /// A two-key prefix was started; the value is the new `key_state`.
    Prefix(i32),
    /// No mapping — the event is left unchanged (an insertable char or unhandled).
    None,
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
        let e = ed();
        use crate::event::Key;
        assert!(matches!(
            e.scan_key_map(0, key(Key::Left)),
            KeyMapResult::Command(c) if c == Command::CHAR_LEFT
        ));
        assert!(matches!(
            e.scan_key_map(0, key(Key::Home)),
            KeyMapResult::Command(c) if c == Command::LINE_START
        ));
        assert!(matches!(
            e.scan_key_map(0, key(Key::PageDown)),
            KeyMapResult::Command(c) if c == Command::PAGE_DOWN
        ));
        assert!(matches!(
            e.scan_key_map(0, key(Key::Enter)),
            KeyMapResult::Command(c) if c == Command::NEW_LINE
        ));
        assert!(matches!(
            e.scan_key_map(0, key(Key::Delete)),
            KeyMapResult::Command(c) if c == Command::DEL_CHAR
        ));
    }

    #[test]
    fn keymap_ctrl_letters() {
        let e = ed();
        assert!(matches!(
            e.scan_key_map(0, ctrl('s')),
            KeyMapResult::Command(c) if c == Command::CHAR_LEFT
        ));
        assert!(matches!(
            e.scan_key_map(0, ctrl('y')),
            KeyMapResult::Command(c) if c == Command::DEL_LINE
        ));
        assert!(matches!(
            e.scan_key_map(0, ctrl('u')),
            KeyMapResult::Command(c) if c == Command::UNDO
        ));
    }

    #[test]
    fn keymap_two_key_prefixes() {
        let e = ed();
        // Ctrl-Q → prefix state 1.
        assert!(matches!(
            e.scan_key_map(0, ctrl('q')),
            KeyMapResult::Prefix(1)
        ));
        // Ctrl-K → prefix state 2.
        assert!(matches!(
            e.scan_key_map(0, ctrl('k')),
            KeyMapResult::Prefix(2)
        ));
        // In state 1, 'F' → cmFind.
        assert!(matches!(
            e.scan_key_map(1, key(crate::event::Key::Char('f'))),
            KeyMapResult::Command(c) if c == Command::FIND
        ));
        // In state 2, 'B' → cmStartSelect.
        assert!(matches!(
            e.scan_key_map(2, key(crate::event::Key::Char('b'))),
            KeyMapResult::Command(c) if c == Command::START_SELECT
        ));
    }

    #[test]
    fn convert_event_prefix_then_command() {
        let mut e = ed();
        // Ctrl-K starts a prefix and clears the event.
        let mut ev = Event::KeyDown(ctrl('k'));
        e.convert_event(&mut ev);
        assert!(ev.is_nothing(), "prefix key is cleared");
        assert_eq!(e.key_state, 2);
        // The next 'b' resolves to cmStartSelect.
        let mut ev2 = Event::KeyDown(key(crate::event::Key::Char('b')));
        e.convert_event(&mut ev2);
        assert_eq!(ev2, Event::Command(Command::START_SELECT));
        assert_eq!(e.key_state, 0, "prefix consumed");
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

    /// Regression: Ctrl-Del must map to cmDelWord (delete word to the right), not
    /// cmClear — firstKeys lists `kbCtrlDel, cmDelWord` before the dead
    /// `kbCtrlDel, cmClear`, and scanKeyMap returns the FIRST match.
    #[test]
    fn ctrl_del_deletes_word_to_the_right() {
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

    /// cmSelectAll selects the whole buffer.
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
}
