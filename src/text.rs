//! Width-aware Unicode text: measuring, scrolling, and drawing strings by
//! **display column** rather than byte or codepoint.
//!
//! The core unit is the **grapheme cluster**: one cluster is one cell (two for
//! a double-width glyph), and a cluster's column width comes from its base
//! char. Combining marks attach to their base inside the cluster, and ZWJ
//! emoji sequences collapse into a single cell. All layout and cursor math here
//! measures in display columns.
//!
//! ### Cell occupancy of a grapheme
//! Driven by the base char's [`unicode_width`]:
//! * control char (`width() == None`) → the replacement glyph `�`, 1 column;
//! * zero-width (combining-only / ZWJ-only cluster) → 0 columns (occupies no cell);
//! * width 1 → 1 column;
//! * width ≥ 2 → a *wide* lead cell + a trailing continuation cell (2 columns).
//!
//! **Guide:** [Drawing & backends](../../../internals/drawing.html).
//!
//! # Turbo Vision heritage
//!
//! Faithful port of the `TText` primitives (`ttext.h`, `ttext.cpp`). The original
//! hand-decoded UTF-8 with a DFA and iterated per codepoint, appending combining
//! marks onto the previous cell at draw time. Because Rust strings are already
//! valid UTF-8 and `unicode-segmentation` yields grapheme clusters directly, tvision-rs
//! drops the DFA and the append-combining-mark machinery and works one grapheme at
//! a time (deviation D13) — which also clusters ZWJ emoji into one cell, where the
//! per-codepoint model split them.

use crate::color::Style;
use crate::screen::Cell;
use std::collections::BTreeMap;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthChar;

/// The replacement glyph drawn for an unprintable (control) character.
const REPLACEMENT: &str = "\u{FFFD}";

/// Width, in display columns, the first grapheme of `g` occupies. `g` must be a
/// single grapheme cluster. Returns `None` only for the empty string.
fn grapheme_columns(g: &str) -> usize {
    match g.chars().next() {
        None => 0,
        // C0/C1 control chars have no width; drawn as `�` (1 column).
        Some(c) => UnicodeWidthChar::width(c).unwrap_or(1),
    }
}

/// Byte length and column width of the first grapheme in `text`, or `None` when
/// `text` is empty.
pub fn next(text: &str) -> Option<(usize, usize)> {
    let g = text.graphemes(true).next()?;
    Some((g.len(), grapheme_columns(g)))
}

/// Byte length of the grapheme cluster *ending* at byte offset `index` in
/// `text` (the cluster you would step back over from `index`). Returns 0 when
/// `index == 0`.
///
/// Stepping `cur_pos -= prev(text, cur_pos)` lands the cursor on the previous
/// cluster boundary, never inside a multi-byte codepoint or a combining sequence.
///
/// `index` must be a `char` boundary into `text` (it always is: every call site
/// maintains `cur_pos` on grapheme boundaries). Returns 0 for `index == 0`.
pub fn prev(text: &str, index: usize) -> usize {
    if index == 0 {
        return 0;
    }
    // The last grapheme of the prefix `text[..index]` is the one ending at
    // `index`; its byte length is how far back the cursor steps.
    text[..index]
        .graphemes(true)
        .next_back()
        .map(|g| g.len())
        .unwrap_or(0)
}

/// Width, the character (grapheme) count, and the number of non-zero-width
/// graphemes of `text`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextMetrics {
    pub width: usize,
    /// Total grapheme clusters (counts *clusters*, not codepoints — the two differ
    /// across combining sequences).
    pub character_count: usize,
    /// Graphemes that occupy at least one column.
    pub grapheme_count: usize,
}

/// Display width of `text` in columns.
pub fn width(text: &str) -> usize {
    text.graphemes(true).map(grapheme_columns).sum()
}

/// Width + grapheme metrics of `text`.
pub fn measure(text: &str) -> TextMetrics {
    let mut m = TextMetrics::default();
    for g in text.graphemes(true) {
        let w = grapheme_columns(g);
        m.width += w;
        m.character_count += 1;
        m.grapheme_count += (w > 0) as usize;
    }
    m
}

/// Byte length of a leading substring of `text` that is `count` columns wide,
/// together with that substring's actual width.
///
/// If `text` is narrower than `count`, the whole string is returned. Negative
/// `count` is treated as 0. When column `count` falls in the middle of a
/// double-width grapheme, `include_incomplete` decides whether that grapheme is
/// included.
pub fn scroll(text: &str, count: i32, include_incomplete: bool) -> (usize, usize) {
    if count <= 0 {
        return (0, 0);
    }
    let count = count as usize;
    let mut i = 0; // byte offset
    let mut w = 0; // accumulated width
    loop {
        let (i2, w2) = (i, w);
        match next(&text[i..]) {
            None => break,
            Some((len, gw)) => {
                i += len;
                w += gw;
                if w == count {
                    break;
                }
                if w > count {
                    if !include_incomplete {
                        i = i2;
                        w = w2;
                    }
                    break;
                }
            }
        }
    }
    (i, w)
}

/// Write a single grapheme from `text` (at byte offset `j`) into `cells` at index
/// `i`. Returns `(bytes_consumed, cells_advanced)`.
///
/// `cells_advanced` is 0 when there is no room (the caller stops), 0 for a
/// zero-width grapheme (consumed but draws nothing), 1 for a normal glyph, and 2
/// for a wide glyph that had room for its trailing cell.
fn draw_one_impl(cells: &mut [Cell], i: usize, text: &str, j: usize) -> (usize, usize) {
    let Some((len, w)) = next(&text[j..]) else {
        return (0, 0);
    };
    let g = &text[j..j + len];
    if w == 0 {
        // Zero-width cluster (lone combining mark / ZWJ): consume, draw nothing.
        // (Combining marks attach to their base inside the grapheme already.)
        return (len, 0);
    }
    if i >= cells.len() {
        // No room — signal the caller to stop.
        return (0, 0);
    }
    let is_control = g
        .chars()
        .next()
        .is_some_and(|c| UnicodeWidthChar::width(c).is_none());
    if is_control {
        cells[i].set_str(REPLACEMENT, false);
        return (len, 1);
    }
    if w > 1 {
        let has_trail = i + 1 < cells.len();
        cells[i].set_str(g, true);
        if has_trail {
            cells[i + 1].set_wide_trail();
        }
        (len, 1 + has_trail as usize)
    } else {
        cells[i].set_str(g, false);
        (len, 1)
    }
}

/// Apply `transform` to a cell's style in place.
fn apply(cell: &mut Cell, transform: &mut impl FnMut(&mut Style)) {
    let mut s = cell.style();
    transform(&mut s);
    cell.set_style(s);
}

/// Write a single grapheme from `text` (at byte offset `j`) into `cells` at index
/// `i`, applying `transform` to the [`Style`] of each cell written. Returns
/// `(bytes_consumed, cells_advanced)`.
///
/// A wide glyph applies `transform` to both its lead and trailing cell.
/// `cells_advanced == 0` with `bytes_consumed == 0` means there was no room — the
/// caller should stop.
pub fn draw_one(
    cells: &mut [Cell],
    i: usize,
    text: &str,
    j: usize,
    mut transform: impl FnMut(&mut Style),
) -> (usize, usize) {
    let (len, advanced) = draw_one_impl(cells, i, text, j);
    if advanced >= 1 {
        apply(&mut cells[i], &mut transform);
    }
    if advanced > 1 {
        apply(&mut cells[i + 1], &mut transform);
    }
    (len, advanced)
}

/// Copy `text` into `cells` starting at cell index `indent`, beginning from
/// column `text_indent` of `text`, applying `transform` to the [`Style`] of each
/// cell written. Returns the number of cells filled.
///
/// When `text_indent` lands in the middle of a double-width grapheme, a single
/// space is emitted in its place.
pub fn draw_str_ex(
    cells: &mut [Cell],
    indent: usize,
    text: &str,
    text_indent: i32,
    mut transform: impl FnMut(&mut Style),
) -> usize {
    let mut i = indent;
    let mut j = 0; // byte offset into text

    if text_indent > 0 {
        let (skipped_bytes, lead_width) = scroll(text, text_indent, true);
        j = skipped_bytes;
        if lead_width > text_indent as usize && i < cells.len() {
            // Skipped past the middle of a wide glyph — pad with a space.
            cells[i].set_char(' ');
            apply(&mut cells[i], &mut transform);
            i += 1;
        }
    }

    loop {
        let (len, advanced) = draw_one(cells, i, text, j, &mut transform);
        i += advanced;
        j += len;
        if len == 0 {
            break;
        }
    }
    i - indent
}

/// Copy `text` into `cells` at `indent`/`text_indent` with a fixed `style`.
pub fn draw_str(
    cells: &mut [Cell],
    indent: usize,
    text: &str,
    text_indent: i32,
    style: Style,
) -> usize {
    draw_str_ex(cells, indent, text, text_indent, |s| *s = style)
}

// ---------------------------------------------------------------------------
// StringList
// ---------------------------------------------------------------------------

/// A keyed lookup of strings (`u16` key → `String`).
///
/// Backed by a `BTreeMap<u16, String>`, so iteration is in ascending key order.
/// A missing key returns `None` (callers that want an empty-string fallback can
/// use `.unwrap_or("")`).
///
/// # Turbo Vision heritage
///
/// Ports `TStringList` / `TStrListMaker` (`tstrlist.cpp`). The original classes
/// exist entirely to serialize a compressed keyed-string table to/from a
/// resource (`.res`) stream; tvision-rs drops that streaming/persistence machinery
/// (`TStrIndexRec`, the run-length index, the byte-length-prefixed blob, and
/// `build`/`read`/`write`) and keeps only the observable contract — a keyed
/// lookup of strings (deviation D12). The maker/list split, which existed only
/// for the write-vs-read streaming asymmetry, collapses into one type.
pub struct StringList {
    map: BTreeMap<u16, String>,
}

impl StringList {
    /// Create an empty `StringList`.
    pub fn new() -> Self {
        StringList {
            map: BTreeMap::new(),
        }
    }

    /// Associate `value` with `key`, overwriting any previous entry for that key.
    ///
    /// Use this to build a `StringList` at startup (the equivalent of
    /// `TStrListMaker::Put` from Turbo Vision). Keys are arbitrary `u16` values;
    /// iteration and lookup are in ascending key order.
    ///
    /// ```
    /// use tvision_rs::text::StringList;
    ///
    /// let mut list = StringList::new();
    /// list.insert(10, "Open");
    /// list.insert(20, "Save");
    /// list.insert(10, "Open…"); // overwrite key 10
    /// assert_eq!(list.get(10), Some("Open…"));
    /// assert_eq!(list.len(), 2);
    /// ```
    pub fn insert(&mut self, key: u16, value: impl Into<String>) {
        self.map.insert(key, value.into());
    }

    /// Look up the string stored under `key`, returning a borrowed slice, or
    /// `None` when no entry exists for that key.
    ///
    /// Use this to display or process a string identified by a numeric key — for
    /// example, to retrieve a help message at runtime:
    ///
    /// ```
    /// use tvision_rs::text::StringList;
    ///
    /// let mut list = StringList::new();
    /// list.insert(1, "File not found");
    /// assert_eq!(list.get(1), Some("File not found"));
    /// assert_eq!(list.get(99), None);
    ///
    /// // Callers that need an empty-string fallback for missing keys:
    /// let msg = list.get(99).unwrap_or("");
    /// assert_eq!(msg, "");
    /// ```
    pub fn get(&self, key: u16) -> Option<&str> {
        self.map.get(&key).map(String::as_str)
    }

    /// Number of entries in this `StringList`.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// `true` when the `StringList` contains no entries.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

impl Default for StringList {
    fn default() -> Self {
        StringList::new()
    }
}

impl<S: Into<String>> FromIterator<(u16, S)> for StringList {
    fn from_iter<I: IntoIterator<Item = (u16, S)>>(iter: I) -> Self {
        let mut list = StringList::new();
        for (key, value) in iter {
            list.insert(key, value);
        }
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    fn render(cells: &[Cell]) -> String {
        cells.iter().map(|c| c.symbol()).collect()
    }

    #[test]
    fn width_ascii_and_wide() {
        assert_eq!(width("hello"), 5);
        assert_eq!(width(""), 0);
        assert_eq!(width("中文"), 4); // two double-width ideographs
        assert_eq!(width("a中b"), 4); // 1 + 2 + 1
    }

    #[test]
    fn width_combining_clusters_as_one() {
        // "e" + combining acute = one grapheme, width 1.
        assert_eq!(width("e\u{0301}"), 1);
        // Precomposed é is also width 1.
        assert_eq!(width("é"), 1);
    }

    #[test]
    fn measure_counts() {
        let m = measure("a中");
        assert_eq!(m.width, 3);
        assert_eq!(m.character_count, 2);
        assert_eq!(m.grapheme_count, 2);

        // A combining mark clusters with its base: one grapheme, width 1.
        let m2 = measure("e\u{0301}");
        assert_eq!(m2.character_count, 1);
        assert_eq!(m2.width, 1);
    }

    #[test]
    fn prev_steps_back_one_grapheme() {
        // ASCII: each step is one byte.
        assert_eq!(prev("abc", 3), 1);
        assert_eq!(prev("abc", 1), 1);
        assert_eq!(prev("abc", 0), 0);
        // Multi-byte: "ä" is 2 bytes in UTF-8; stepping back from the end of
        // "aä" (3 bytes) returns 2 (the whole grapheme), never 1.
        assert_eq!("aä".len(), 3);
        assert_eq!(prev("aä", 3), 2);
        assert_eq!(prev("aä", 1), 1);
        // A combining sequence "e\u{0301}" (3 bytes) is one grapheme.
        let s = "xe\u{0301}";
        assert_eq!(prev(s, s.len()), "e\u{0301}".len());
        // An emoji ZWJ cluster steps back as a single unit.
        let fam = "👨\u{200d}👩\u{200d}👧";
        let s2 = format!("a{fam}");
        assert_eq!(prev(&s2, s2.len()), fam.len());
    }

    #[test]
    fn scroll_basic() {
        // First 3 columns of "abcdef" -> 3 bytes, width 3.
        assert_eq!(scroll("abcdef", 3, true), (3, 3));
        // Wider than the string -> whole string.
        assert_eq!(scroll("abc", 10, true), (3, 3));
        // Non-positive count -> empty.
        assert_eq!(scroll("abc", 0, true), (0, 0));
        assert_eq!(scroll("abc", -5, true), (0, 0));
    }

    #[test]
    fn scroll_straddles_wide_glyph() {
        // "中" is 2 columns. Asking for 1 column lands mid-glyph.
        // include_incomplete = true -> include the glyph (2 bytes? no, 3 bytes UTF-8).
        let (bytes_incl, w_incl) = scroll("中x", 1, true);
        assert_eq!((bytes_incl, w_incl), ("中".len(), 2));
        // include_incomplete = false -> exclude it (stop before).
        let (bytes_excl, w_excl) = scroll("中x", 1, false);
        assert_eq!((bytes_excl, w_excl), (0, 0));
    }

    #[test]
    fn draw_str_ascii() {
        let mut cells = vec![Cell::default(); 10];
        let style = Style::new(Color::Bios(0xF), Color::Bios(0x1));
        let n = draw_str(&mut cells, 0, "hi", 0, style);
        assert_eq!(n, 2);
        assert_eq!(cells[0].symbol(), "h");
        assert_eq!(cells[0].style(), style);
        assert_eq!(cells[1].symbol(), "i");
        // untouched cells remain blank
        assert_eq!(cells[2].symbol(), " ");
    }

    #[test]
    fn draw_str_wide_glyph_sets_trail() {
        let mut cells = vec![Cell::default(); 10];
        let n = draw_str(&mut cells, 0, "中", 0, Style::default());
        assert_eq!(n, 2); // lead + trail
        assert!(cells[0].is_wide());
        assert_eq!(cells[0].symbol(), "中");
        assert!(cells[1].is_wide_trail());
        assert_eq!(cells[1].symbol(), "");
    }

    #[test]
    fn draw_str_indent_into_cells() {
        let mut cells = vec![Cell::default(); 10];
        draw_str(&mut cells, 3, "ab", 0, Style::default());
        assert_eq!(cells[2].symbol(), " ");
        assert_eq!(cells[3].symbol(), "a");
        assert_eq!(cells[4].symbol(), "b");
    }

    #[test]
    fn draw_str_text_indent_skips_columns() {
        let mut cells = vec![Cell::default(); 10];
        let n = draw_str(&mut cells, 0, "abcdef", 2, Style::default());
        assert_eq!(n, 4);
        assert_eq!(render(&cells[..4]), "cdef");
    }

    #[test]
    fn draw_str_text_indent_mid_wide_glyph_pads_space() {
        // "中" occupies columns 0..2. Starting at column 1 splits it -> a space.
        let mut cells = vec![Cell::default(); 10];
        let n = draw_str(&mut cells, 0, "中x", 1, Style::default());
        assert_eq!(cells[0].symbol(), " "); // padding for the split wide glyph
        assert_eq!(cells[1].symbol(), "x");
        assert_eq!(n, 2);
    }

    #[test]
    fn draw_str_truncates_at_cell_boundary() {
        let mut cells = vec![Cell::default(); 3];
        let n = draw_str(&mut cells, 0, "abcdef", 0, Style::default());
        assert_eq!(n, 3);
        assert_eq!(render(&cells), "abc");
    }

    #[test]
    fn draw_str_wide_glyph_truncated_when_no_room_for_trail() {
        // Only one cell free, but a wide glyph needs two: lead is written,
        // trail is truncated, count is 1.
        let mut cells = vec![Cell::default(); 1];
        let n = draw_str(&mut cells, 0, "中", 0, Style::default());
        assert_eq!(n, 1);
        assert!(cells[0].is_wide());
    }

    #[test]
    fn draw_str_control_char_becomes_replacement() {
        let mut cells = vec![Cell::default(); 5];
        let n = draw_str(&mut cells, 0, "a\u{0007}b", 0, Style::default()); // BEL
        assert_eq!(n, 3);
        assert_eq!(cells[0].symbol(), "a");
        assert_eq!(cells[1].symbol(), REPLACEMENT);
        assert_eq!(cells[2].symbol(), "b");
    }

    #[test]
    fn draw_str_ex_transform_sees_each_cell() {
        let mut cells = vec![Cell::default(); 5];
        let mut count = 0;
        draw_str_ex(&mut cells, 0, "中a", 0, |s| {
            count += 1;
            s.modifiers.bold = true;
        });
        // lead + trail + 'a' = 3 transform calls
        assert_eq!(count, 3);
        assert!(cells[0].style().modifiers.bold);
        assert!(cells[1].style().modifiers.bold); // trail too
        assert!(cells[2].style().modifiers.bold);
    }

    // --- StringList tests ---

    #[test]
    fn string_list_insert_and_get_round_trip() {
        let mut sl = StringList::new();
        sl.insert(10, "hello");
        sl.insert(20, "world");
        assert_eq!(sl.get(10), Some("hello"));
        assert_eq!(sl.get(20), Some("world"));
    }

    #[test]
    fn string_list_missing_key_returns_none() {
        let sl = StringList::new();
        assert_eq!(sl.get(42), None);
    }

    #[test]
    fn string_list_overwrite_keeps_latest_value() {
        let mut sl = StringList::new();
        sl.insert(5, "first");
        sl.insert(5, "second");
        assert_eq!(sl.get(5), Some("second"));
        assert_eq!(sl.len(), 1);
    }

    #[test]
    fn string_list_ordered_iteration() {
        let mut sl = StringList::new();
        // Insert out of order.
        sl.insert(30, "c");
        sl.insert(10, "a");
        sl.insert(20, "b");
        // BTreeMap guarantees ascending key order.
        let pairs: Vec<(&u16, &String)> = sl.map.iter().collect();
        assert_eq!(*pairs[0].0, 10);
        assert_eq!(*pairs[1].0, 20);
        assert_eq!(*pairs[2].0, 30);
    }

    #[test]
    fn string_list_len_and_is_empty() {
        let mut sl = StringList::new();
        assert!(sl.is_empty());
        assert_eq!(sl.len(), 0);
        sl.insert(1, "x");
        assert!(!sl.is_empty());
        assert_eq!(sl.len(), 1);
        sl.insert(2, "y");
        assert_eq!(sl.len(), 2);
    }

    #[test]
    fn string_list_from_iterator() {
        let entries: Vec<(u16, &str)> = vec![(1, "one"), (2, "two"), (3, "three")];
        let sl: StringList = entries.into_iter().collect();
        assert_eq!(sl.len(), 3);
        assert_eq!(sl.get(1), Some("one"));
        assert_eq!(sl.get(2), Some("two"));
        assert_eq!(sl.get(3), Some("three"));
        assert_eq!(sl.get(4), None);
    }

    #[test]
    fn string_list_from_iterator_owned_strings() {
        let entries: Vec<(u16, String)> = vec![
            (100, "hundred".to_string()),
            (200, "two hundred".to_string()),
        ];
        let sl: StringList = entries.into_iter().collect();
        assert_eq!(sl.get(100), Some("hundred"));
        assert_eq!(sl.get(200), Some("two hundred"));
    }

    #[test]
    fn string_list_default_is_empty() {
        let sl = StringList::default();
        assert!(sl.is_empty());
    }
}
