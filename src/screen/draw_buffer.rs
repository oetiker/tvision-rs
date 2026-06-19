//! `DrawBuffer` — a scratch row of [`Cell`]s.
//!
//! A view fills a `DrawBuffer` one display line at a time and then blits it. The
//! buffer is a `Vec<Cell>` of an explicit width; writes past the end are clipped
//! against that width. Text writing delegates to the [`text`] primitives, so
//! width-aware truncation and double-width handling are shared with the rest of
//! the renderer.
//!
//! ### No `0 = retain` sentinel
//! [`move_char`](DrawBuffer::move_char) always writes both the char and the
//! style. For the rare "change only the attribute / only the char of a cell"
//! cases, use [`put_attribute`](DrawBuffer::put_attribute) /
//! [`put_char`](DrawBuffer::put_char).
//!
//! # Turbo Vision heritage
//! Ports `TDrawBuffer` (`drawbuf.h`, `drivers.cpp`), which allocated a fixed
//! capacity sized to the screen; here it is a `Vec<Cell>` of an explicit width.
//! The original packed a `0` char/attribute to mean "keep what is already there";
//! that sentinel is dropped in favour of the typed cell model (deviation D6).
//! Text drawing routes through the shared Unicode-aware text primitives
//! (deviation D13).

use crate::color::Style;
use crate::screen::Cell;
use crate::text;

/// A fixed-width row of screen cells under construction.
#[derive(Clone, Debug)]
pub struct DrawBuffer {
    data: Vec<Cell>,
}

impl DrawBuffer {
    /// A blank buffer `width` columns wide (every cell a space with the default
    /// style). `width` is the `capacity` against which all writes are clipped.
    pub fn new(width: usize) -> Self {
        DrawBuffer {
            data: vec![Cell::default(); width],
        }
    }

    /// The buffer's width in columns; all write operations are clipped to this limit.
    ///
    /// Equals the `width` passed to [`DrawBuffer::new`]. Use this to cap a fill
    /// count or guard a manual write loop so it stays within the allocated buffer.
    pub fn capacity(&self) -> usize {
        self.data.len()
    }

    /// A read-only view of all cells in the buffer, from column 0 to
    /// [`capacity`](DrawBuffer::capacity) − 1.
    ///
    /// Call this after filling the buffer to hand the completed row to a
    /// drawing context (e.g. pass it to `ctx.write_buf`), or to inspect
    /// individual cells in tests. The slice length always equals `capacity`.
    pub fn cells(&self) -> &[Cell] {
        &self.data
    }

    /// Set the char of a single cell at `indent`, leaving its style untouched.
    ///
    /// Prefer this over [`move_char`](DrawBuffer::move_char) when you only want to
    /// change the glyph of an already-styled cell — for example, placing a border
    /// corner over a cell whose colour was set by an earlier pass. Out-of-range
    /// `indent` is a silent no-op.
    pub fn put_char(&mut self, indent: usize, ch: char) {
        if indent < self.data.len() {
            self.data[indent].set_char(ch);
        }
    }

    /// Set the style of a single cell at `indent`, leaving its char untouched.
    ///
    /// Prefer this over [`move_char`](DrawBuffer::move_char) when you only want to
    /// restyle a cell without changing its glyph — for example, applying a
    /// highlight colour to text that was placed by an earlier pass. Out-of-range
    /// `indent` is a silent no-op.
    pub fn put_attribute(&mut self, indent: usize, style: Style) {
        if indent < self.data.len() {
            self.data[indent].set_style(style);
        }
    }

    /// Fill `count` cells from `indent` with `ch` and `style`, clipped to
    /// capacity. (See the module note on the dropped `0 = retain` sentinel.)
    pub fn move_char(&mut self, indent: usize, ch: char, style: Style, count: usize) {
        let cap = self.data.len();
        if count == 0 || indent >= cap {
            return;
        }
        let count = if indent.saturating_add(count) >= cap {
            cap - indent
        } else {
            count
        };
        let mut cell = Cell::default();
        cell.set_char(ch);
        cell.set_style(style);
        for slot in &mut self.data[indent..indent + count] {
            *slot = cell.clone();
        }
    }

    /// Write `text` at column `indent`, starting from column `str_indent` of
    /// `text`, with a fixed `style`, writing at most `max_width` columns. Returns
    /// the number of cells written.
    pub fn move_str_part(
        &mut self,
        indent: usize,
        text: &str,
        style: Style,
        max_width: usize,
        str_indent: usize,
    ) -> usize {
        let cap = self.data.len();
        if indent >= cap || text.is_empty() || max_width == 0 {
            return 0;
        }
        let end = if indent.saturating_add(max_width) >= cap {
            cap
        } else {
            indent + max_width
        };
        text::draw_str(
            &mut self.data[..end],
            indent,
            text,
            str_indent as i32,
            style,
        )
    }

    /// Write `text` at `indent` with `style`, unbounded width, from the start of
    /// `text`.
    pub fn move_str(&mut self, indent: usize, text: &str, style: Style) -> usize {
        self.move_str_part(indent, text, style, self.data.len(), 0)
    }

    /// Write a *control string*: `text` is drawn with `lo`, and each `~` toggles
    /// between `lo` and `hi` (the classic hotkey-highlight markup). Returns the
    /// number of cells written.
    pub fn move_cstr_part(
        &mut self,
        indent: usize,
        text: &str,
        lo: Style,
        hi: Style,
        max_width: usize,
        str_indent: usize,
    ) -> usize {
        let cap = self.data.len();
        if indent >= cap || text.is_empty() || max_width == 0 {
            return 0;
        }
        let end = if indent.saturating_add(max_width) >= cap {
            cap
        } else {
            indent + max_width
        };
        let dest = &mut self.data[..end];
        let attrs = [lo, hi];
        let mut cur = lo;
        let mut toggle = 1usize; // first '~' selects attrs[1] (hi)
        let mut i = indent;
        let mut j = 0usize; // byte offset into text
        let mut w = 0usize; // columns consumed while skipping str_indent
        let bytes = text.as_bytes();
        while j < text.len() {
            if bytes[j] == b'~' {
                cur = attrs[toggle];
                toggle = 1 - toggle;
                j += 1;
            } else if str_indent <= w {
                let (len, adv) = text::draw_one(dest, i, text, j, |s| *s = cur);
                if len == 0 {
                    break;
                }
                i += adv;
                j += len;
            } else {
                match text::next(&text[j..]) {
                    None => break,
                    Some((len, gw)) => {
                        j += len;
                        w += gw;
                        if str_indent < w && i < dest.len() {
                            // str_indent fell inside a double-width glyph.
                            dest[i].set_char(' ');
                            dest[i].set_style(cur);
                            i += 1;
                        }
                    }
                }
            }
        }
        i - indent
    }

    /// Write a control string at full width, from the start.
    pub fn move_cstr(&mut self, indent: usize, text: &str, lo: Style, hi: Style) -> usize {
        self.move_cstr_part(indent, text, lo, hi, self.data.len(), 0)
    }

    /// Copy a run of pre-built cells into the buffer at `indent`, clipped to
    /// capacity.
    ///
    /// Use `move_buf` when you have already assembled the styled cells — for
    /// example, copying another view's row out of a [`DrawBuffer`] or blitting a
    /// pre-rendered sprite. For the common cases of filling with a single
    /// character+style use [`move_char`](Self::move_char); for rendering a string
    /// use [`move_str`](Self::move_str) or [`move_cstr`](Self::move_cstr).
    ///
    /// In the typed cell model the meaningful operation is copying cells, so this
    /// takes a `&[Cell]` rather than a raw byte buffer.
    ///
    /// # Turbo Vision heritage
    ///
    /// Ports `MoveBuf` (`drivers.cpp`); the C++ `Word`/attribute pair buffer
    /// becomes a typed `&[Cell]` slice. `MoveBuf`'s raw-byte
    /// coupling to the `TDrawBuffer` layout has no equivalent here.
    pub fn move_buf(&mut self, indent: usize, src: &[Cell]) {
        let cap = self.data.len();
        if indent >= cap {
            return;
        }
        let n = src.len().min(cap - indent);
        self.data[indent..indent + n].clone_from_slice(&src[..n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    fn render(b: &DrawBuffer) -> String {
        b.cells().iter().map(|c| c.symbol()).collect()
    }

    #[test]
    fn move_char_fills_and_clips() {
        let mut b = DrawBuffer::new(5);
        let s = Style::new(Color::Bios(0xF), Color::Bios(0x1));
        b.move_char(1, '*', s, 2);
        assert_eq!(render(&b), " **  "); // width-5 buffer
        assert_eq!(b.cells()[1].style(), s);
        assert_eq!(b.cells()[0].style(), Style::default());

        // count past the end is clamped
        b.move_char(3, '#', s, 99);
        assert_eq!(render(&b), " **##");
    }

    #[test]
    fn move_str_writes_and_returns_width() {
        let mut b = DrawBuffer::new(10);
        let s = Style::new(Color::Bios(0x7), Color::Default);
        let n = b.move_str(2, "hello", s);
        assert_eq!(n, 5);
        assert_eq!(&render(&b)[..8], "  hello ");
        assert_eq!(b.cells()[2].style(), s);
    }

    #[test]
    fn move_str_clips_to_capacity_and_max_width() {
        let mut b = DrawBuffer::new(6);
        // capacity clip: only 4 cells from indent 2
        let n = b.move_str(2, "abcdef", Style::default());
        assert_eq!(n, 4);
        assert_eq!(render(&b), "  abcd");

        // explicit max_width clip
        let mut b2 = DrawBuffer::new(10);
        let n2 = b2.move_str_part(0, "abcdef", Style::default(), 3, 0);
        assert_eq!(n2, 3);
        assert_eq!(&render(&b2)[..3], "abc");
    }

    #[test]
    fn move_str_indent_into_text() {
        let mut b = DrawBuffer::new(10);
        let n = b.move_str_part(0, "abcdef", Style::default(), 10, 2);
        assert_eq!(n, 4);
        assert_eq!(&render(&b)[..4], "cdef");
    }

    #[test]
    fn move_cstr_toggles_attribute_on_tilde() {
        let mut b = DrawBuffer::new(10);
        let lo = Style::new(Color::Bios(0x7), Color::Default);
        let hi = Style::new(Color::Bios(0xF), Color::Default);
        // "~O~pen": 'O' is highlighted, "pen" reverts to lo.
        let n = b.move_cstr(0, "~O~pen", lo, hi);
        assert_eq!(n, 4); // O p e n  (tildes consume no cell)
        assert_eq!(render(&b), "Open      ");
        assert_eq!(b.cells()[0].style(), hi); // O highlighted
        assert_eq!(b.cells()[1].style(), lo); // p reverted
        assert_eq!(b.cells()[2].style(), lo);
    }

    #[test]
    fn move_cstr_plain_text_uses_lo() {
        let mut b = DrawBuffer::new(10);
        let lo = Style::new(Color::Bios(0x7), Color::Default);
        let hi = Style::new(Color::Bios(0xF), Color::Default);
        let n = b.move_cstr(0, "Cancel", lo, hi);
        assert_eq!(n, 6);
        assert_eq!(render(&b), "Cancel    ");
        for i in 0..6 {
            assert_eq!(b.cells()[i].style(), lo);
        }
    }

    #[test]
    fn put_char_and_put_attribute() {
        let mut b = DrawBuffer::new(4);
        let s = Style::new(Color::Bios(0x1), Color::Default);
        b.put_char(1, 'Z');
        b.put_attribute(1, s);
        assert_eq!(b.cells()[1].symbol(), "Z");
        assert_eq!(b.cells()[1].style(), s);
        // out of range is a no-op
        b.put_char(99, 'X');
        assert_eq!(render(&b), " Z  ");
    }

    #[test]
    fn move_buf_copies_cells() {
        let mut src = DrawBuffer::new(3);
        src.move_str(0, "xyz", Style::new(Color::Bios(0x2), Color::Default));

        let mut b = DrawBuffer::new(6);
        b.move_buf(2, src.cells());
        assert_eq!(render(&b), "  xyz ");
        assert_eq!(
            b.cells()[2].style(),
            Style::new(Color::Bios(0x2), Color::Default)
        );

        // clipped at capacity
        let mut b2 = DrawBuffer::new(4);
        b2.move_buf(3, src.cells());
        assert_eq!(render(&b2), "   x");
    }

    #[test]
    fn move_str_wide_glyph_in_buffer() {
        let mut b = DrawBuffer::new(6);
        let n = b.move_str(0, "中a", Style::default());
        assert_eq!(n, 3); // wide lead + trail + 'a'
        assert!(b.cells()[0].is_wide());
        assert!(b.cells()[1].is_wide_trail());
        assert_eq!(b.cells()[2].symbol(), "a");
    }
}
