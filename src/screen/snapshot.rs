//! Deterministic textual rendering of a [`Buffer`] for golden snapshot tests —
//! deviation **D11**.
//!
//! This is the **verification backbone** of the whole port: every widget test
//! from Phase 1 onward diffs its `HeadlessBackend` screen against a golden
//! string produced here. The format is therefore frozen here, once, as a
//! FOUNDATION decision — not improvised per test.
//!
//! ## Format
//!
//! [`snapshot`] renders three layers plus cursor metadata, with **no
//! timestamps** (so goldens are stable):
//!
//! ```text
//! size: 10x3
//! cursor: 5,0
//! text:
//! |Hello     |
//! |World     |
//! |          |
//! attr:
//! |aaaaa.....|
//! |bbbbb.....|
//! |..........|
//! legend:
//!   . default
//!   a fg=BIOS(15) bg=BIOS(1)
//!   b fg=RGB(255,0,0) bg=default +bold
//! ```
//!
//! - **text layer** — the on-screen glyphs, one framed line per row. Each line
//!   is bounded by `|` so trailing spaces stay visible in the golden file (the
//!   leading/trailing `|` are frame delimiters, identified by position, not
//!   content — note box-drawing uses U+2502 `│`, distinct from ASCII `|`).
//! - **attr layer** — one key char per *display column*, aligned beneath the
//!   text. `.` is always the default style; other styles are keyed `a..z`,
//!   `A..Z`, `0..9` in row-major first-appearance order.
//! - **legend** — key → style description (`fg=… bg=… [+mod…]`), `.` first,
//!   then the rest in assignment order.
//! - **cursor** — `x,y` when shown, or `hidden`.
//!
//! ## Wide glyphs
//!
//! A double-width grapheme occupies a `wide` lead cell plus a `trail` cell in
//! the buffer. In the snapshot the lead contributes its 2-column glyph to the
//! text layer and its key **twice** to the attr layer; the trail cell is
//! absorbed (contributes nothing). So both layers are exactly `width` display
//! columns wide and stay aligned.

use crate::color::{Color, Style};
use crate::screen::{Buffer, Cell};

/// The cursor state for a snapshot: `Some((x, y))` when visible at that cell,
/// `None` when hidden.
pub type SnapshotCursor = Option<(u16, u16)>;

/// Render `buffer` (and the `cursor` state) to the canonical golden-snapshot
/// string documented on this module.
///
/// Pass the result to `insta::assert_snapshot!`. The output is fully
/// deterministic — same buffer + cursor always yields the same string, with no
/// timestamps or addresses.
///
/// # Panics
/// Panics if the buffer contains more than 63 distinct styles (the 1-char-key
/// legend is exhausted). Real widget tests use a handful of styles; a test that
/// genuinely needs more should be split.
pub fn snapshot(buffer: &Buffer, cursor: SnapshotCursor) -> String {
    let mut legend = Legend::new();
    let width = buffer.width();
    let height = buffer.height();

    let mut text = String::new();
    let mut attr = String::new();

    for y in 0..height {
        text.push('|');
        attr.push('|');
        let mut x = 0;
        while x < width {
            let cell = buffer.get(x, y);
            if cell.is_wide_trail() {
                // Absorbed by the preceding wide lead; contributes nothing.
                x += 1;
                continue;
            }
            let key = legend.key_for(cell.style());
            let cols = display_cols(cell);
            // Text layer: the glyph itself already spans `cols` display columns.
            text.push_str(cell.symbol());
            // Attr layer: one key per display column.
            for _ in 0..cols {
                attr.push(key);
            }
            x += 1;
        }
        text.push('|');
        attr.push('|');
        text.push('\n');
        attr.push('\n');
    }

    let mut out = String::new();
    out.push_str(&format!("size: {width}x{height}\n"));
    match cursor {
        Some((cx, cy)) => out.push_str(&format!("cursor: {cx},{cy}\n")),
        None => out.push_str("cursor: hidden\n"),
    }
    out.push_str("text:\n");
    out.push_str(&text);
    out.push_str("attr:\n");
    out.push_str(&attr);
    out.push_str("legend:\n");
    out.push_str(&legend.render());
    out
}

/// Display columns a cell occupies: a `wide` lead is 2, anything else is 1.
/// (Trail cells are handled by the caller and never reach here.)
fn display_cols(cell: &Cell) -> usize {
    if cell.is_wide() { 2 } else { 1 }
}

/// Assigns a stable one-character key to each distinct [`Style`], reserving `.`
/// for the default style.
struct Legend {
    /// (key, style) in assignment order; `.`/default is always first.
    entries: Vec<(char, Style)>,
}

impl Legend {
    fn new() -> Self {
        Legend {
            entries: vec![('.', Style::default())],
        }
    }

    /// The key for `style`, assigning a fresh one on first sighting.
    fn key_for(&mut self, style: Style) -> char {
        if let Some(&(key, _)) = self.entries.iter().find(|&&(_, s)| s == style) {
            return key;
        }
        let key = nth_key(self.entries.len() - 1); // -1: default doesn't consume the a/b/c… space
        self.entries.push((key, style));
        key
    }

    /// The `legend:` body, one `  <key> <desc>` line per style. The default
    /// style is shown as the shorthand `default`.
    fn render(&self) -> String {
        let mut s = String::new();
        for &(key, style) in &self.entries {
            let desc = if style == Style::default() {
                "default".to_string()
            } else {
                describe_style(style)
            };
            s.push_str(&format!("  {key} {desc}\n"));
        }
        s
    }
}

/// The `n`-th legend key (0-based) over the alphabet `a..z`, `A..Z`, `0..9`.
fn nth_key(n: usize) -> char {
    const KEYS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    *KEYS
        .get(n)
        .expect("snapshot legend exhausted: more than 63 distinct styles in one buffer") as char
}

/// Human-readable one-line style description, e.g. `fg=BIOS(15) bg=default +bold +reverse`.
fn describe_style(style: Style) -> String {
    let mut s = format!(
        "fg={} bg={}",
        describe_color(style.fg),
        describe_color(style.bg)
    );
    let m = style.modifiers;
    for (on, name) in [
        (m.bold, "bold"),
        (m.italic, "italic"),
        (m.underline, "underline"),
        (m.blink, "blink"),
        (m.reverse, "reverse"),
        (m.strike, "strike"),
        (m.no_shadow, "no_shadow"),
    ] {
        if on {
            s.push_str(" +");
            s.push_str(name);
        }
    }
    s
}

/// Human-readable color, matching the [`Color`] variants.
fn describe_color(color: Color) -> String {
    match color {
        Color::Default => "default".to_string(),
        Color::Bios(n) => format!("BIOS({n})"),
        Color::Indexed(n) => format!("idx({n})"),
        Color::Rgb(r, g, b) => format!("RGB({r},{g},{b})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{Color, Style};

    #[test]
    fn blank_buffer_is_all_default() {
        let buf = Buffer::new(3, 2);
        let s = snapshot(&buf, None);
        assert_eq!(
            s,
            "size: 3x2\n\
             cursor: hidden\n\
             text:\n\
             |   |\n\
             |   |\n\
             attr:\n\
             |...|\n\
             |...|\n\
             legend:\n\
             \x20\x20. default\n"
        );
    }

    #[test]
    fn text_and_styles_with_legend() {
        let mut buf = Buffer::new(5, 1);
        let a = Style::new(Color::Bios(0xF), Color::Bios(0x1));
        for (i, ch) in "Hi".chars().enumerate() {
            let c = buf.get_mut(i as u16, 0);
            c.set_char(ch);
            c.set_style(a);
        }
        let s = snapshot(&buf, Some((2, 0)));
        let expected = "size: 5x1\n\
             cursor: 2,0\n\
             text:\n\
             |Hi   |\n\
             attr:\n\
             |aa...|\n\
             legend:\n\
             \x20\x20. default\n\
             \x20\x20a fg=BIOS(15) bg=BIOS(1)\n";
        assert_eq!(s, expected);
    }

    #[test]
    fn wide_glyph_lead_keyed_twice_trail_absorbed() {
        let mut buf = Buffer::new(4, 1);
        let a = Style::new(Color::Rgb(255, 0, 0), Color::Default);
        buf.get_mut(0, 0).set_str("中", true);
        buf.get_mut(0, 0).set_style(a);
        buf.get_mut(1, 0).set_wide_trail();
        let s = snapshot(&buf, None);
        // text: 中 (2 cols) then two blanks = 4 display cols.
        // attr: lead key twice, then two defaults.
        let expected = "size: 4x1\n\
             cursor: hidden\n\
             text:\n\
             |中  |\n\
             attr:\n\
             |aa..|\n\
             legend:\n\
             \x20\x20. default\n\
             \x20\x20a fg=RGB(255,0,0) bg=default\n";
        assert_eq!(s, expected);
    }

    #[test]
    fn modifiers_are_described() {
        let mut buf = Buffer::new(1, 1);
        let mut style = Style::new(Color::Default, Color::Default);
        style.modifiers.bold = true;
        style.modifiers.reverse = true;
        buf.get_mut(0, 0).set_style(style);
        let s = snapshot(&buf, None);
        assert!(
            s.contains("a fg=default bg=default +bold +reverse"),
            "got:\n{s}"
        );
    }

    #[test]
    fn default_style_always_keyed_dot_even_if_seen_later() {
        // A non-default style appears first (cell 0), default second (cell 1).
        let mut buf = Buffer::new(2, 1);
        buf.get_mut(0, 0)
            .set_style(Style::new(Color::Bios(2), Color::Default));
        let s = snapshot(&buf, None);
        // cell 0 -> 'a', cell 1 (default) -> '.'
        assert!(s.contains("|a.|"), "attr row wrong:\n{s}");
    }
}
