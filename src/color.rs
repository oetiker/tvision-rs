//! Typed colour and style.
//!
//! [`Color`] is a desired foreground or background colour — terminal default,
//! a 4-bit BIOS index, an xterm-256 index, or 24-bit RGB. [`Style`] pairs a
//! foreground and background `Color` with a [`Modifiers`] struct-of-bools
//! (bold, italic, underline, …). These types describe colour in the abstract;
//! converting an arbitrary colour to whatever a given terminal can actually
//! display (the RGB→256→16→BIOS quantization ladder) happens in the
//! [`Backend`](crate::Backend), since it only matters when colours are flushed
//! to a real terminal.
//!
//! **Guide:** [Theming & colors](../../../apps/theming.html).
//!
//! # Turbo Vision heritage
//!
//! magiblot packs fg/bg/style into a 64-bit `TColorAttr` whose fg/bg are each a
//! tagged-union `TColorDesired` (`colors.h`). tvision-rs keeps that four-variant
//! design but drops the bit-packing: a plain enum plus a struct-of-bools
//! (deviation D5). The quantization ladder (`mapcolor.cpp`) moves to the
//! `Backend` (deviation D6).

/// A desired foreground *or* background colour.
///
/// In a terminal, [`Color::Default`] is the colour of text with no display
/// attributes set — so a default/default cell still produces visible text.
///
/// # Turbo Vision heritage
///
/// Faithful to `TColorDesired`'s four-variant design (`colors.h`), minus the
/// bit-packing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Color {
    /// Terminal default (`ctDefault`).
    #[default]
    Default,
    /// 4-bit BIOS colour (`ctBIOS` / `TColorBIOS`). Invariant: 0..=15. C++
    /// masks `bios & 0xF` on construction; we store the raw `u8`, so callers and
    /// the quantization ladder must mask to keep a 16-entry palette lookup in
    /// range.
    Bios(u8),
    /// Index into the xterm-256 palette, 0..=255 (`ctXTerm` / `TColorXTerm`).
    Indexed(u8),
    /// 24-bit true colour (`ctRGB` / `TColorRGB`).
    Rgb(u8, u8, u8),
}

impl Color {
    /// Canonical IBM VGA text-mode RGB for each of the 16 BIOS palette indices.
    /// Single source of truth for resolving `Color::Bios(n)` to a definite
    /// true-color value (used by the default theme and the colour picker).
    pub const BIOS_RGB: [(u8, u8, u8); 16] = [
        (0, 0, 0),       // 0 Black
        (0, 0, 170),     // 1 Blue
        (0, 170, 0),     // 2 Green
        (0, 170, 170),   // 3 Cyan
        (170, 0, 0),     // 4 Red
        (170, 0, 170),   // 5 Magenta
        (170, 85, 0),    // 6 Brown
        (170, 170, 170), // 7 Light Gray
        (85, 85, 85),    // 8 Dark Gray
        (85, 85, 255),   // 9 Light Blue
        (85, 255, 85),   // 10 Light Green
        (85, 255, 255),  // 11 Light Cyan
        (255, 85, 85),   // 12 Light Red
        (255, 85, 255),  // 13 Light Magenta
        (255, 255, 85),  // 14 Yellow
        (255, 255, 255), // 15 White
    ];

    /// Resolve a 4-bit BIOS index to its canonical true-color RGB.
    /// The index is masked to 0..=15.
    pub fn bios_rgb(index: u8) -> Color {
        let (r, g, b) = Color::BIOS_RGB[(index & 0x0F) as usize];
        Color::Rgb(r, g, b)
    }

    pub fn is_default(self) -> bool {
        matches!(self, Color::Default)
    }
    pub fn is_bios(self) -> bool {
        matches!(self, Color::Bios(_))
    }
    pub fn is_indexed(self) -> bool {
        matches!(self, Color::Indexed(_))
    }
    pub fn is_rgb(self) -> bool {
        matches!(self, Color::Rgb(..))
    }
}

/// Text-style modifiers — bold, italic, underline, and so on — as a
/// struct-of-bools.
///
/// Set these on a [`Style`] to control per-cell terminal attributes. Most
/// terminals support at least `bold`, `underline`, and `reverse`; `italic`,
/// `blink`, and `strike` are honored on terminals that declare support for
/// them. All default to `false` (normal text).
///
/// ```rust
/// use tvision_rs::color::{Color, Modifiers, Style};
///
/// // Underlined text on the default background.
/// let s = Style::with_modifiers(
///     Color::Default,
///     Color::Default,
///     Modifiers { underline: true, ..Default::default() },
/// );
/// ```
///
/// `no_shadow` is a per-cell marker that window shadows must not be cast over
/// this cell (used internally by the window shadow pass; leave it `false` in
/// normal use).
///
/// # Turbo Vision heritage
///
/// Faithful to the `sl*` masks of `TColorAttr`'s 10-bit style word
/// (`colors.h`), unpacked into a struct-of-bools. The four
/// `TMonoSelector` attributes (Normal, Highlight=bold, Underline, Inverse=reverse)
/// all have counterparts in this struct. `no_shadow` is the private `slNoShadow`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub bold: bool,      // slBold
    pub italic: bool,    // slItalic
    pub underline: bool, // slUnderline
    pub blink: bool,     // slBlink
    pub reverse: bool,   // slReverse  (prefer Style::reversed())
    pub strike: bool,    // slStrike
    pub no_shadow: bool, // slNoShadow (private)
}

/// The colour attributes of a screen cell: a foreground colour, a background
/// colour, and a set of style modifiers.
///
/// A zero-initialized `Style` (via [`Default`]) has both colours `Default` and
/// no modifiers — so a zero-initialized cell still produces visible text.
///
/// # Turbo Vision heritage
///
/// Faithful to `TColorAttr` (`colors.h`), minus the bit-packing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    pub modifiers: Modifiers,
}

impl Style {
    /// A style with the given colours and no modifiers.
    pub fn new(fg: Color, bg: Color) -> Self {
        Style {
            fg,
            bg,
            modifiers: Modifiers::default(),
        }
    }

    /// A style with explicit colours and modifiers.
    pub fn with_modifiers(fg: Color, bg: Color, modifiers: Modifiers) -> Self {
        Style { fg, bg, modifiers }
    }

    /// Port of the free function `reverseAttribute` (`colors.h`).
    ///
    /// The `slReverse` attribute is rendered inconsistently across terminals, so
    /// TV swaps the colours manually — *unless* either colour is `Default`, in
    /// which case there is nothing meaningful to swap and it falls back to
    /// toggling the reverse flag.
    pub fn reversed(self) -> Style {
        let mut out = self;
        if self.fg.is_default() || self.bg.is_default() {
            out.modifiers.reverse = !out.modifiers.reverse;
        } else {
            out.fg = self.bg;
            out.bg = self.fg;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_predicates() {
        assert!(Color::Default.is_default());
        assert!(Color::Bios(7).is_bios());
        assert!(Color::Indexed(200).is_indexed());
        assert!(Color::Rgb(1, 2, 3).is_rgb());
        assert!(!Color::Rgb(0, 0, 0).is_default());
    }

    #[test]
    fn default_style_is_visible_text() {
        let s = Style::default();
        assert_eq!(s.fg, Color::Default);
        assert_eq!(s.bg, Color::Default);
        assert_eq!(s.modifiers, Modifiers::default());
    }

    #[test]
    fn reversed_swaps_concrete_colors() {
        let s = Style::new(Color::Bios(0x7), Color::Bios(0x1));
        let r = s.reversed();
        assert_eq!(r.fg, Color::Bios(0x1));
        assert_eq!(r.bg, Color::Bios(0x7));
        assert!(!r.modifiers.reverse); // flag untouched when colours swapped
    }

    #[test]
    fn reversed_toggles_flag_when_a_color_is_default() {
        // Default foreground -> swap is meaningless, toggle the flag instead.
        let s = Style::new(Color::Default, Color::Bios(0x1));
        let r = s.reversed();
        assert_eq!(r.fg, Color::Default);
        assert_eq!(r.bg, Color::Bios(0x1));
        assert!(r.modifiers.reverse);

        // toggling twice returns to the original
        assert!(!r.reversed().modifiers.reverse);
    }

    #[test]
    fn bios_rgb_canonical_values() {
        assert_eq!(Color::bios_rgb(6), Color::Rgb(170, 85, 0)); // Brown special-case
        assert_eq!(Color::bios_rgb(1), Color::Rgb(0, 0, 170)); // Blue
        // Masking: 0x16 & 0x0F == 6, same as index 6
        assert_eq!(Color::bios_rgb(0x16), Color::bios_rgb(6));
    }
}
