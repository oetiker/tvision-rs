//! Render back-buffer and cell diff.
//!
//! [`Buffer`] is the in-memory screen grid: the whole view tree is painted into
//! it each frame, then [`Buffer::diff`] compares it against the previous frame so
//! only changed cells are flushed to the terminal.
//!
//! The diff algorithm is adapted from ratatui's `Buffer::diff` (MIT). Our `Cell`
//! (see `cell.rs`) already encodes double-width glyphs with explicit
//! `wide`/`trail` flags, so the wide-char skipping is driven off those rather
//! than off `unicode-width`. ratatui's `skip` field is dropped — tvision-rs repaints
//! the whole tree, so there is nothing to opt out of.
//!
//! # Turbo Vision heritage
//! Replaces Turbo Vision's per-view incremental screen writes with a
//! double-buffered whole-tree repaint plus cell diff (deviation D8). The grid
//! and diff are adapted from ratatui rather than ported from the original.
//
// Portions adapted from ratatui (https://github.com/ratatui-org/ratatui),
// licensed under the MIT License. Copyright (c) 2023-2024 The Ratatui Developers.

use crate::screen::Cell;

/// In-memory screen grid; the render target for one complete frame.
///
/// Call [`Buffer::new`] to create a blank grid sized to the terminal, paint the
/// full view tree into it via the renderer, then call [`Buffer::diff`] against
/// the previous frame's buffer to obtain only the changed cells to flush to the
/// terminal. The `Program` root owns two buffers (current and previous) and
/// swaps them each iteration of the event loop.
///
/// Unlike ratatui, `Buffer` stores only `width` × `height` rather than an
/// `area: Rect`, because the back buffer is always screen-origin (`(0, 0)` to
/// `(width-1, height-1)`). There is no offset to carry.
///
/// # Turbo Vision heritage
///
/// Replaces `TVideoBuf` (a fixed 80×25 word array of `char+attr` cells) and
/// the per-view incremental `writeView` calls. The dynamic-size grid and
/// double-buffered whole-tree repaint follow magiblot/tvision's modernised
/// screen model rather than the original fixed-buffer approach.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Buffer {
    width: u16,
    height: u16,
    /// Row-major cell storage; length == `width * height`.
    content: Vec<Cell>,
}

impl Buffer {
    /// A new buffer filled with [`Cell::default()`] (a blank space, default style).
    pub fn new(width: u16, height: u16) -> Self {
        let len = width as usize * height as usize;
        Buffer {
            width,
            height,
            content: vec![Cell::default(); len],
        }
    }

    /// A new buffer where every cell is a clone of `cell`.
    pub fn filled(width: u16, height: u16, cell: Cell) -> Self {
        let len = width as usize * height as usize;
        Buffer {
            width,
            height,
            content: vec![cell; len],
        }
    }

    /// The buffer width in columns.
    pub fn width(&self) -> u16 {
        self.width
    }

    /// The buffer height in rows.
    pub fn height(&self) -> u16 {
        self.height
    }

    /// Total number of cells (`width * height`).
    pub fn area_len(&self) -> usize {
        self.width as usize * self.height as usize
    }

    /// Convert `(x, y)` to a linear index into `content`.
    ///
    /// Panics in debug builds if `x >= width` or `y >= height`.
    pub fn index_of(&self, x: u16, y: u16) -> usize {
        debug_assert!(
            x < self.width && y < self.height,
            "index_of({x}, {y}) out of bounds for {w}×{h} buffer",
            w = self.width,
            h = self.height,
        );
        // Cast to usize *before* multiplying to avoid u16 overflow.
        y as usize * self.width as usize + x as usize
    }

    /// Convert a linear index back to `(x, y)`.
    pub fn pos_of(&self, i: usize) -> (u16, u16) {
        let w = self.width as usize;
        // Widths of 0 would be degenerate; guard in debug.
        debug_assert!(w > 0, "pos_of called on zero-width buffer");
        ((i % w) as u16, (i / w) as u16)
    }

    /// Return a shared reference to the cell at `(x, y)`.
    pub fn get(&self, x: u16, y: u16) -> &Cell {
        let i = self.index_of(x, y);
        &self.content[i]
    }

    /// Return a mutable reference to the cell at `(x, y)`.
    pub fn get_mut(&mut self, x: u16, y: u16) -> &mut Cell {
        let i = self.index_of(x, y);
        &mut self.content[i]
    }

    /// A slice over all cells in row-major order (for tests / snapshots).
    pub fn cells(&self) -> &[Cell] {
        &self.content
    }

    /// A shared slice over row `y` (`width` cells), for width-aware row writes.
    ///
    /// Panics in debug builds if `y >= height`.
    pub fn row(&self, y: u16) -> &[Cell] {
        debug_assert!(
            y < self.height,
            "row({y}) out of bounds for height {}",
            self.height
        );
        let w = self.width as usize;
        let start = y as usize * w;
        &self.content[start..start + w]
    }

    /// A mutable slice over row `y` (`width` cells), for width-aware row writes
    /// (e.g. feeding `text::draw_str` a clipped sub-slice).
    ///
    /// Panics in debug builds if `y >= height`.
    pub fn row_mut(&mut self, y: u16) -> &mut [Cell] {
        debug_assert!(
            y < self.height,
            "row_mut({y}) out of bounds for height {}",
            self.height
        );
        let w = self.width as usize;
        let start = y as usize * w;
        &mut self.content[start..start + w]
    }

    /// Reset every cell to [`Cell::default()`].
    pub fn reset(&mut self) {
        for cell in &mut self.content {
            *cell = Cell::default();
        }
    }

    /// Resize the buffer to `width × height`, resetting all content to
    /// [`Cell::default()`].
    pub fn resize(&mut self, width: u16, height: u16) {
        let len = width as usize * height as usize;
        self.width = width;
        self.height = height;
        self.content = vec![Cell::default(); len];
    }

    /// Compare `self` (the previous/front buffer) against `other` (the
    /// next/back buffer) and return the cells in `other` that differ.
    ///
    /// Each entry is `(x, y, &cell)` pointing into `other`.
    ///
    /// Both buffers must have the same dimensions. The invariant is asserted in
    /// debug builds; in release the comparison is limited to the overlapping
    /// index range.
    ///
    /// The algorithm is adapted from ratatui's `Buffer::diff`: it iterates the
    /// flat cell array, suppressing the trail cell of a wide glyph via
    /// `to_skip`, and using `invalidated` to force repainting the cell that
    /// follows a cell whose *previous* width was greater than its *current*
    /// width (e.g. a wide glyph replaced by a narrow one — the trail cell must
    /// be repainted even if it happens to be equal to the old trail cell).
    pub fn diff<'a>(&self, other: &'a Buffer) -> Vec<(u16, u16, &'a Cell)> {
        debug_assert_eq!(
            (self.width, self.height),
            (other.width, other.height),
            "Buffer::diff called on buffers with different dimensions"
        );

        let len = self.content.len().min(other.content.len());
        let mut updates: Vec<(u16, u16, &'a Cell)> = vec![];
        let mut invalidated: usize = 0;
        let mut to_skip: usize = 0;

        for (i, (current, previous)) in other.content[..len]
            .iter()
            .zip(self.content[..len].iter())
            .enumerate()
        {
            if (current != previous || invalidated > 0) && to_skip == 0 {
                let (x, y) = self.pos_of(i);
                updates.push((x, y, current));
            }

            let current_width = render_width(current);
            let previous_width = render_width(previous);
            to_skip = current_width.saturating_sub(1);
            let affected = current_width.max(previous_width);
            invalidated = affected.max(invalidated).saturating_sub(1);
        }

        updates
    }
}

/// Returns the number of terminal columns a cell occupies.
///
/// A `wide` lead cell occupies 2 columns; every other cell (including the
/// `trail` continuation cell) counts as 1. This mirrors ratatui's skipping of
/// empty continuation cells, but is driven off our typed flags instead of
/// `unicode-width`.
fn render_width(cell: &Cell) -> usize {
    if cell.is_wide() { 2 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{Color, Style};

    // --- construction ---

    #[test]
    fn new_has_correct_dimensions() {
        let b = Buffer::new(80, 25);
        assert_eq!(b.width(), 80);
        assert_eq!(b.height(), 25);
        assert_eq!(b.area_len(), 80 * 25);
        assert_eq!(b.cells().len(), 80 * 25);
    }

    #[test]
    fn new_all_cells_default() {
        let b = Buffer::new(5, 3);
        for cell in b.cells() {
            assert_eq!(*cell, Cell::default());
        }
    }

    #[test]
    fn filled_sets_every_cell() {
        let s = Style::new(Color::Bios(0xF), Color::Bios(0x1));
        let seed = Cell::blank(s);
        let b = Buffer::filled(4, 4, seed.clone());
        for cell in b.cells() {
            assert_eq!(*cell, seed);
        }
    }

    // --- index_of / pos_of round-trip ---

    #[test]
    fn index_pos_roundtrip() {
        let b = Buffer::new(10, 5);
        for y in 0..5u16 {
            for x in 0..10u16 {
                let i = b.index_of(x, y);
                assert_eq!(b.pos_of(i), (x, y));
            }
        }
    }

    #[test]
    fn index_of_row_major() {
        let b = Buffer::new(10, 5);
        assert_eq!(b.index_of(0, 0), 0);
        assert_eq!(b.index_of(9, 0), 9);
        assert_eq!(b.index_of(0, 1), 10);
        assert_eq!(b.index_of(9, 4), 49);
    }

    // --- get / get_mut ---

    #[test]
    fn get_mut_mutates() {
        let mut b = Buffer::new(5, 5);
        b.get_mut(2, 3).set_char('X');
        assert_eq!(b.get(2, 3).symbol(), "X");
        // neighbouring cells unaffected
        assert_eq!(b.get(1, 3).symbol(), " ");
        assert_eq!(b.get(3, 3).symbol(), " ");
    }

    // --- diff: identical buffers ---

    #[test]
    fn diff_identical_is_empty() {
        let a = Buffer::new(5, 3);
        let b = Buffer::new(5, 3);
        assert!(a.diff(&b).is_empty());
    }

    // --- diff: changed cells ---

    #[test]
    fn diff_returns_changed_cells() {
        let front = Buffer::new(5, 3);
        let mut back = Buffer::new(5, 3);
        // change three scattered cells
        back.get_mut(0, 0).set_char('A');
        back.get_mut(3, 1).set_char('B');
        back.get_mut(4, 2).set_char('C');

        let changes: Vec<(u16, u16)> = front.diff(&back).iter().map(|&(x, y, _)| (x, y)).collect();
        assert_eq!(changes.len(), 3);
        assert!(changes.contains(&(0, 0)));
        assert!(changes.contains(&(3, 1)));
        assert!(changes.contains(&(4, 2)));

        // verify the cell content points into `back`
        for &(x, y, cell) in &front.diff(&back) {
            assert_eq!(*cell, *back.get(x, y));
        }
    }

    // --- diff: wide glyph — lead emitted once, trail suppressed ---

    #[test]
    fn diff_wide_lead_emitted_once_trail_suppressed() {
        let front = Buffer::new(4, 1);
        let mut back = Buffer::new(4, 1);
        // place a double-width glyph at columns 0–1
        back.get_mut(0, 0).set_str("中", true);
        back.get_mut(1, 0).set_wide_trail();

        let changes = front.diff(&back);
        // Only the lead cell at (0, 0) should appear; the trail is suppressed
        // by `to_skip = 1`.
        let coords: Vec<(u16, u16)> = changes.iter().map(|&(x, y, _)| (x, y)).collect();
        assert_eq!(coords, vec![(0, 0)]);

        // The emitted cell is indeed the lead cell.
        let (_, _, cell) = changes[0];
        assert!(cell.is_wide());
        assert_eq!(cell.symbol(), "中");
    }

    // --- diff: narrow replaces wide — trail repainted via `invalidated` ---

    #[test]
    fn diff_narrow_replacing_wide_repaints_trail_via_invalidated() {
        // front has a wide glyph at (0,0)+(1,0)
        let mut front = Buffer::new(4, 1);
        front.get_mut(0, 0).set_str("中", true);
        front.get_mut(1, 0).set_wide_trail();

        // back has a narrow char at (0,0) and the *same* trail as front at (1,0)
        // so (1,0) is NOT different by equality — only `invalidated` forces it.
        let mut back = Buffer::new(4, 1);
        back.get_mut(0, 0).set_char('a');
        back.get_mut(1, 0).set_wide_trail(); // equal to front[1]

        let changes = front.diff(&back);
        let coords: Vec<(u16, u16)> = changes.iter().map(|&(x, y, _)| (x, y)).collect();
        // Both (0,0) and (1,0) must be emitted.
        assert!(
            coords.contains(&(0, 0)),
            "expected (0,0) in changes, got {coords:?}"
        );
        assert!(
            coords.contains(&(1, 0)),
            "expected (1,0) in changes (via invalidated), got {coords:?}"
        );
    }

    // --- reset ---

    #[test]
    fn reset_clears_all_cells() {
        let mut b = Buffer::new(3, 3);
        for y in 0..3u16 {
            for x in 0..3u16 {
                b.get_mut(x, y).set_char('X');
            }
        }
        b.reset();
        for cell in b.cells() {
            assert_eq!(*cell, Cell::default());
        }
    }

    // --- resize ---

    #[test]
    fn resize_changes_dimensions_and_resets() {
        let mut b = Buffer::new(5, 5);
        b.get_mut(0, 0).set_char('Z');
        b.resize(10, 3);
        assert_eq!(b.width(), 10);
        assert_eq!(b.height(), 3);
        assert_eq!(b.area_len(), 30);
        assert_eq!(b.cells().len(), 30);
        // content reset
        for cell in b.cells() {
            assert_eq!(*cell, Cell::default());
        }
    }
}
