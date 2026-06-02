//! `TStaticText` — faithful Rust port of `tstatict.cpp` (row 36, MECHANICAL).
//!
//! Displays a read-only multi-line text block, word-wrapping to fill its bounds.
//!
//! ## Algorithm (word-wrap)
//!
//! For each output row `y` in `0..size.y`: fill with `StaticText` color; if text
//! remains, handle a leading `\x03` (ETX) byte as a *center-this-line* flag
//! (persists across wrapped continuations until `\n` resets it); compute `last`
//! = the byte offset reached by scrolling `size.x` display columns
//! (`text::scroll`, relative to `i`); pack whole words (advance word-by-word
//! within the `last` limit); handle the back-off (if we overshot `last`, step
//! back to the last word-boundary `j`, or hard-break at `last` if the first word
//! alone overflows); draw `[i,p)` at column `draw_col` (`(size.x − width) / 2`
//! when centered, else 0); consume trailing spaces; on `\n` reset `center` and
//! advance past the newline.
//!
//! ## D-rules applied
//!
//! - **D1**: drop `T` prefix → `StaticText`; `snake_case` methods.
//! - **D2/D5**: `View` trait + `ViewState` composition. `growMode |= gfFixed`
//!   sets `state.grow_mode.fixed = true` (faithful to the C++ bit).
//! - **D7**: no palette chain — `ctx.style(Role::StaticText)` instead of
//!   `getColor(1)`.
//! - **D8**: draw via `DrawCtx`; no `writeLine`/`writeBuf`/`TDrawBuffer`.
//! - **D12**: no `TStreamable` (`read`/`write`/`build` dropped).
//! - **D13**: grapheme-based; byte-offset word-scan safe because space/newline/
//!   ETX (0x03) are single-byte ASCII — only `text::next` is used to advance
//!   through non-ASCII runs.

use crate::text;
use crate::theme::Role;
use crate::view::{DrawCtx, GrowMode, Rect, View, ViewState};

// ---------------------------------------------------------------------------
// StaticText
// ---------------------------------------------------------------------------

/// `TStaticText` — a read-only, word-wrapped text block (D2 View trait +
/// ViewState).
///
/// Embed pattern: `state: ViewState`, `impl View`, draw through `DrawCtx`.
/// No events (no `handle_event` override); not selectable.
pub struct StaticText {
    /// View state (geometry, flags, etc.) — the D2 composition target.
    pub state: ViewState,
    /// The text content. Held as a full `String`; no 255-byte truncation.
    text: String,
}

impl StaticText {
    /// Construct a static text view from `bounds` and `text`.
    ///
    /// Faithful to `TStaticText::TStaticText`:
    /// - `growMode |= gfFixed` → `state.grow_mode.fixed = true`.
    /// - Not selectable (static text ignores focus).
    pub fn new(bounds: Rect, text: impl Into<String>) -> Self {
        let mut state = ViewState::new(bounds);
        // gfFixed: the view keeps its size regardless of the owner's resize.
        state.grow_mode = GrowMode {
            fixed: true,
            ..Default::default()
        };
        // Not selectable — `TStaticText` has no `handleEvent` and is not
        // interactive (`options.selectable` is already false by default).
        StaticText {
            state,
            text: text.into(),
        }
    }

    /// The current text content.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Replace the text content. Caller is responsible for triggering a redraw
    /// (the whole-tree repaint under D8 handles it on the next pump pass).
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }
}

impl View for StaticText {
    fn state(&self) -> &ViewState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }

    /// `TStaticText::draw` — word-wrap and paint.
    ///
    /// Faithful port of `tstatict.cpp`:
    ///
    /// ```text
    /// color = getColor(1);
    /// getText(buf); s = buf;  // we hold the full String, no 255-byte truncation
    /// l = s.size(); p = 0; y = 0; center = False;
    /// while (y < size.y) {
    ///     b.moveChar(0, ' ', color, size.x);
    ///     if (p < l) {
    ///         if (s[p] == 3) { center = True; ++p; }
    ///         i = p;
    ///         last = i + TText::scroll(s.substr(i), size.x, False).bytes;
    ///         do {
    ///             j = p;
    ///             while (p<l && s[p]==' ') p++;
    ///             while (p<l && s[p]!=' ' && s[p]!='\n') p += TText::next(…).bytes;
    ///         } while (p<l && p<last && s[p]!='\n');
    ///         if (p > last) { p = (j > i) ? j : last; }
    ///         width = strwidth(s[i..p]);
    ///         draw_col = center ? (size.x - width) / 2 : 0;
    ///         b.moveStr(draw_col, s[i..p], color, width);
    ///         while (p<l && s[p]==' ') p++;
    ///         if (p<l && s[p]=='\n') { center = False; p++; }
    ///     }
    ///     writeLine(0, y++, size.x, 1, b);
    /// }
    /// ```
    ///
    /// ## Byte-offset discipline (D13)
    ///
    /// The word-scan advances through the text as **byte offsets** (`p`, `i`,
    /// `j`/`word_start`, `last`). ASCII guards (`' '`, `'\n'`, `\x03`) are
    /// safe as single-byte comparisons. Non-ASCII word characters advance by
    /// `text::next(&self.text[p..]).0` (grapheme cluster byte length); this
    /// naturally aligns `p` to grapheme boundaries, so `&self.text[i..p]` is
    /// always a valid UTF-8 slice.
    fn draw(&mut self, ctx: &mut DrawCtx) {
        let color = ctx.style(Role::StaticText);
        let size_x = self.state.size.x;
        let size_y = self.state.size.y;

        // Fill the entire view with the StaticText color first (faithful: C++
        // calls moveChar for every row, even after text is exhausted).
        ctx.fill(self.state.get_extent(), ' ', color);

        let s = self.text.as_bytes(); // byte slice for ASCII comparisons
        let l = s.len();
        let mut p: usize = 0; // current byte position in text
        // `center` persists across wrapped continuations; only `\n` resets it.
        let mut center = false;

        for y in 0..size_y {
            if p < l {
                // Check for ETX (0x03) center-toggle prefix.
                if s[p] == 3 {
                    center = true;
                    p += 1;
                }

                let i = p; // start of this line's text slice

                // `last` = byte offset (absolute) of the last byte that fits in
                // `size_x` display columns. `text::scroll` returns relative bytes.
                let last = i + text::scroll(&self.text[i..], size_x, false).0;

                // Word-pack loop (do-while: body runs at least once).
                //
                // `word_start` is the C++ `j`: the byte offset at the start of
                // the last word's leading spaces, set at the top of every body
                // iteration. After the loop, if `p > last`, we back off to
                // `word_start` (or hard-break at `last` if the first word
                // overflowed). Declared uninitialized: the `loop` body
                // unconditionally assigns `word_start = p` before any read, so
                // definite-assignment is satisfied without a dummy initializer.
                let mut word_start;
                loop {
                    word_start = p; // j = p (C++)
                    // Skip leading spaces.
                    while p < l && s[p] == b' ' {
                        p += 1;
                    }
                    // Skip a word (non-space, non-newline), advancing by grapheme.
                    // The `map_or(1, …)` fallback is unreachable under the `p < l`
                    // guard (a non-empty slice always yields a grapheme); 1 is a
                    // safe defensive default.
                    while p < l && s[p] != b' ' && s[p] != b'\n' {
                        p += text::next(&self.text[p..]).map_or(1, |(len, _)| len);
                    }
                    // Continue packing words while still within `last` and not at
                    // a hard newline.
                    if !(p < l && p < last && s[p] != b'\n') {
                        break;
                    }
                }

                // Back-off: if we overshot `last`, retreat to the last word
                // boundary (`word_start`), or hard-break at `last` if the very
                // first word overflowed.
                if p > last {
                    p = if word_start > i { word_start } else { last };
                }

                // Compute display width of the slice [i, p) and draw column.
                let slice = &self.text[i..p];
                let width = text::width(slice);
                let draw_col = if center {
                    (size_x - width as i32) / 2
                } else {
                    0
                };

                // Draw the slice. `put_str` clips to the view bounds; the slice
                // is exactly `width` columns wide (never wider than `size_x`
                // since `last` was computed from `scroll(size_x, false)`).
                ctx.put_str(draw_col, y, slice, color);

                // Consume trailing spaces.
                while p < l && s[p] == b' ' {
                    p += 1;
                }
                // On newline: reset centering and advance past it.
                if p < l && s[p] == b'\n' {
                    center = false;
                    p += 1;
                }
            }
            // Rows beyond text are already filled with spaces (done above).
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
    use crate::screen::Buffer;
    use crate::theme::Theme;

    // -- Snapshot helper (mirrors scrollbar tests) ---------------------------

    fn render_static_text(bounds: Rect, text: &str, buf_w: u16, buf_h: u16) -> String {
        let theme = Theme::classic_blue();
        let mut st = StaticText::new(bounds, text);
        let (backend, screen) = HeadlessBackend::new(buf_w, buf_h);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let b = st.state.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, b, b.a);
            st.draw(&mut dc);
        });
        screen.snapshot()
    }

    // -- Snapshot section parsers -------------------------------------------

    /// Extract text rows from the `text:` section of a snapshot string.
    /// Returns the inner content of each `|...|` line, without the pipes.
    fn text_rows(snap: &str) -> Vec<String> {
        let mut result = Vec::new();
        let mut in_section = false;
        for line in snap.lines() {
            match line.trim() {
                "text:" => {
                    in_section = true;
                }
                "attr:" | "legend:" => {
                    in_section = false;
                }
                _ if in_section && line.starts_with('|') => {
                    // Strip leading and trailing '|'.
                    result.push(line[1..line.len() - 1].to_string());
                }
                _ => {}
            }
        }
        result
    }

    /// Extract attr rows from the `attr:` section of a snapshot string.
    fn attr_rows(snap: &str) -> Vec<String> {
        let mut result = Vec::new();
        let mut in_section = false;
        for line in snap.lines() {
            match line.trim() {
                "attr:" => {
                    in_section = true;
                }
                "legend:" => {
                    in_section = false;
                }
                _ if in_section && line.starts_with('|') => {
                    result.push(line[1..line.len() - 1].to_string());
                }
                _ => {}
            }
        }
        result
    }

    // -- Algorithm unit tests ------------------------------------------------

    /// Simple wrapping: "hello world" in a 6-wide, 2-high box wraps after
    /// "hello" (5 chars fit, space consumed, "world" on row 1).
    #[test]
    fn wraps_at_word_boundary() {
        let snap = render_static_text(Rect::new(0, 0, 6, 2), "hello world", 6, 2);
        let rows = text_rows(&snap);
        assert_eq!(rows.len(), 2, "should have 2 text rows");
        assert_eq!(rows[0], "hello ", "row 0: 'hello' + trailing space fill");
        assert_eq!(rows[1], "world ", "row 1: 'world' + trailing space fill");
    }

    /// A word longer than the view is hard-broken at `last` (no word boundary).
    #[test]
    fn hard_break_when_first_word_overflows() {
        // Width 4, text "abcdefgh" — each row fits 4 chars.
        let snap = render_static_text(Rect::new(0, 0, 4, 2), "abcdefgh", 4, 2);
        let rows = text_rows(&snap);
        assert_eq!(rows[0], "abcd", "row 0: first 4 chars");
        assert_eq!(rows[1], "efgh", "row 1: next 4 chars");
    }

    /// `\x03` (ETX) at the start of a paragraph centers that line (and
    /// continuations) horizontally within the view.
    #[test]
    fn etx_centers_the_line() {
        // Width 10, "\x03hi" → "hi" (width 2) centered: (10-2)/2 = 4 spaces of indent.
        let snap = render_static_text(Rect::new(0, 0, 10, 1), "\x03hi", 10, 1);
        let rows = text_rows(&snap);
        assert_eq!(rows.len(), 1);
        // "    hi    " — 4 leading spaces (centered), 4 trailing fill.
        assert_eq!(&rows[0][..4], "    ", "should have 4 leading spaces");
        assert_eq!(&rows[0][4..6], "hi", "text should be 'hi'");
    }

    /// `\n` forces a line break and resets the centering flag.
    #[test]
    fn newline_forces_break_and_resets_centering() {
        // "\x03A\nB" — row 0: "A" centered; row 1: "B" left-aligned.
        let snap = render_static_text(Rect::new(0, 0, 6, 2), "\x03A\nB", 6, 2);
        let rows = text_rows(&snap);
        assert_eq!(rows.len(), 2);
        // Row 0: "A" centered in 6 → (6-1)/2 = 2 spaces before "A".
        assert_eq!(
            &rows[0][..2],
            "  ",
            "row 0 should have 2 leading spaces (centered)"
        );
        assert_eq!(&rows[0][2..3], "A", "row 0 text should be 'A'");
        // Row 1: "B" left-aligned (centering was reset by \n).
        assert_eq!(&rows[1][..1], "B", "row 1 text should start with 'B'");
        assert_ne!(&rows[1][..2], "  ", "row 1 should not be centered");
    }

    /// Centering persists across wrapped rows until a `\n`.
    ///
    /// Discriminating width: the CONTINUATION row's centered indent must be
    /// non-zero, so the test bites if carry-over is removed. Width 4, text
    /// "\x03aa bb": ETX sets center; "aa" (width 2) fits row 0, "bb" (width 2)
    /// wraps to row 1. Centered in 4: (4-2)/2 = 1 leading space on BOTH rows.
    #[test]
    fn centering_persists_across_wrapped_rows() {
        let snap = render_static_text(Rect::new(0, 0, 4, 2), "\x03aa bb", 4, 2);
        let rows = text_rows(&snap);
        assert_eq!(rows.len(), 2);
        // Row 0: " aa " (1 leading space, centered).
        assert_eq!(&rows[0][..1], " ", "row 0 centered: 1 leading space");
        assert_eq!(&rows[0][1..3], "aa", "row 0 text 'aa'");
        // Row 1: the continuation must STILL be centered (carry-over). 1 leading
        // space, "bb" at col 1 — would be col 0 ("bb..") if centering were lost.
        assert_eq!(
            &rows[1][..1],
            " ",
            "row 1 (continuation) stays centered: 1 leading space"
        );
        assert_eq!(&rows[1][1..3], "bb", "row 1 text 'bb' at the centered col");
        assert_ne!(
            &rows[1][..2],
            "bb",
            "row 1 must NOT be left-aligned (bites if carry-over breaks)"
        );
    }

    /// Rows beyond the text are filled with StaticText-colored spaces (not
    /// default-style blanks).
    #[test]
    fn trailing_rows_use_static_text_color() {
        // 1 row of text in a 2-row view: row 1 should appear in the attr map.
        let snap = render_static_text(Rect::new(0, 0, 3, 2), "hi", 3, 2);
        assert!(snap.contains("attr:"), "snapshot should have attr section");
        let a_rows = attr_rows(&snap);
        assert_eq!(a_rows.len(), 2, "should have 2 attr rows");
        // Row 1 should not be all '.' (default style).
        assert!(
            !a_rows[1].chars().all(|c| c == '.'),
            "trailing row should carry StaticText color, not default style"
        );
    }

    // -- Snapshot test -------------------------------------------------------

    #[test]
    fn snapshot_static_text_word_wrap() {
        let theme = Theme::classic_blue();
        let mut st = StaticText::new(Rect::new(0, 0, 12, 3), "hello world foo");
        let (backend, screen) = HeadlessBackend::new(12, 3);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let b = st.state.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, b, b.a);
            st.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    #[test]
    fn snapshot_static_text_centered() {
        let theme = Theme::classic_blue();
        let mut st = StaticText::new(Rect::new(0, 0, 12, 2), "\x03centered\nplain");
        let (backend, screen) = HeadlessBackend::new(12, 2);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let b = st.state.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, b, b.a);
            st.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }
}
