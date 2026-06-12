//! A single screen cell: its text (a grapheme cluster), its colour [`Style`],
//! and flags tracking double-width glyphs.
//!
//! One cell can hold a whole grapheme cluster (a base character plus combining
//! marks / ZWJ sequence), stored as an owned `String`. Double-width handling: a
//! width-2 grapheme occupies a [`wide`](Cell::is_wide) lead cell immediately
//! followed by a [`trail`](Cell::is_wide_trail) continuation cell whose symbol
//! is empty.
//!
//! # Turbo Vision heritage
//! Ports `TScreenCell` + `TCellChar` (`scrncell.h`). The original packs the text
//! inline as up to 15 UTF-8 bytes; here it is an owned `String`. The colour is a
//! typed [`Style`] rather than a packed attribute byte (deviation D6), and the
//! inline grapheme model carries combining marks and ZWJ sequences (deviation
//! D13).

use crate::color::Style;

/// One character cell of the screen: its text (a grapheme cluster), its colour
/// [`Style`], and the wide/trail flags that track double-width glyphs.
///
/// `PartialEq` is derived because the render diff compares the back buffer
/// against the front buffer cell-by-cell to emit only changed cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    /// The grapheme cluster shown in this cell. Empty for a wide-char trail.
    symbol: String,
    /// Colour attributes.
    style: Style,
    /// This cell holds the lead of a double-width grapheme.
    wide: bool,
    /// This cell is the trailing half of the preceding double-width grapheme.
    trail: bool,
}

impl Default for Cell {
    /// A blank cell: a single space with the default style. (A space is the
    /// conventional on-screen blank and matches the ratatui-derived buffer shape.)
    fn default() -> Self {
        Cell {
            symbol: String::from(" "),
            style: Style::default(),
            wide: false,
            trail: false,
        }
    }
}

impl Cell {
    /// A blank cell carrying the given style (the usual fill for clearing a
    /// region to a background colour).
    pub fn blank(style: Style) -> Self {
        Cell {
            symbol: String::from(" "),
            style,
            wide: false,
            trail: false,
        }
    }

    /// Set the text to a single `char`, clearing the wide/trail flags.
    pub fn set_char(&mut self, ch: char) {
        self.symbol.clear();
        self.symbol.push(ch);
        self.wide = false;
        self.trail = false;
    }

    /// Set the text to a grapheme cluster, flagging it `wide` if it is a
    /// double-width glyph.
    pub fn set_str(&mut self, s: &str, wide: bool) {
        self.symbol.clear();
        self.symbol.push_str(s);
        self.wide = wide;
        self.trail = false;
    }

    /// Mark this cell as the trailing half of a double-width glyph (empty symbol,
    /// `trail` set).
    pub fn set_wide_trail(&mut self) {
        self.symbol.clear();
        self.wide = false;
        self.trail = true;
    }

    /// The grapheme cluster occupying this cell (empty for a wide trail).
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    pub fn style(&self) -> Style {
        self.style
    }

    pub fn set_style(&mut self, style: Style) {
        self.style = style;
    }

    /// Whether this cell leads a double-width glyph.
    pub fn is_wide(&self) -> bool {
        self.wide
    }

    /// Whether this cell is the trailing half of a double-width glyph.
    pub fn is_wide_trail(&self) -> bool {
        self.trail
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    #[test]
    fn default_is_blank_space() {
        let c = Cell::default();
        assert_eq!(c.symbol(), " ");
        assert_eq!(c.style(), Style::default());
        assert!(!c.is_wide());
        assert!(!c.is_wide_trail());
    }

    #[test]
    fn set_char_clears_flags() {
        let mut c = Cell::default();
        c.set_wide_trail();
        c.set_char('A');
        assert_eq!(c.symbol(), "A");
        assert!(!c.is_wide());
        assert!(!c.is_wide_trail());
    }

    #[test]
    fn wide_glyph_and_trail() {
        // A double-width grapheme (e.g. a CJK ideograph) leads, trail follows.
        let mut lead = Cell::default();
        lead.set_str("中", true);
        assert_eq!(lead.symbol(), "中");
        assert!(lead.is_wide());
        assert!(!lead.is_wide_trail());

        let mut trail = Cell::default();
        trail.set_wide_trail();
        assert_eq!(trail.symbol(), "");
        assert!(!trail.is_wide());
        assert!(trail.is_wide_trail());
    }

    #[test]
    fn grapheme_cluster_symbol() {
        // Combining mark clusters into one cell.
        let mut c = Cell::default();
        c.set_str("é", false); // e + combining acute, as one grapheme
        assert_eq!(c.symbol(), "é");
        assert!(!c.is_wide());
    }

    #[test]
    fn style_roundtrips() {
        let mut c = Cell::default();
        let s = Style::new(Color::Bios(0xF), Color::Bios(0x1));
        c.set_style(s);
        assert_eq!(c.style(), s);
        assert_eq!(Cell::blank(s).style(), s);
    }

    #[test]
    fn equality_tracks_all_fields() {
        let mut a = Cell::default();
        let mut b = Cell::default();
        assert_eq!(a, b);
        a.set_char('X');
        assert_ne!(a, b);
        b.set_char('X');
        assert_eq!(a, b);
        b.set_style(Style::new(Color::Bios(1), Color::Default));
        assert_ne!(a, b);
    }
}
