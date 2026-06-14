//! ANSI (SGR) → themed HTML converter. Consumes `tmux capture-pane -e -p`
//! output and produces a self-contained `<pre class="tv-screen">` fragment.
//! The 16 base colors resolve through `rstv::Color::BIOS_RGB`, so embedded
//! screenshots match the running crate's palette.

use std::fmt::Write as _;

#[derive(Clone, Copy, PartialEq)]
enum Col {
    Default,
    Rgb(u8, u8, u8),
}

#[derive(Clone, Copy, PartialEq)]
struct Sgr {
    fg: Col,
    bg: Col,
    bold: bool,
    underline: bool,
    reverse: bool,
}

impl Sgr {
    fn reset() -> Self {
        Sgr {
            fg: Col::Default,
            bg: Col::Default,
            bold: false,
            underline: false,
            reverse: false,
        }
    }
}

fn bios(i: u8) -> Col {
    let (r, g, b) = rstv::Color::BIOS_RGB[(i & 0x0f) as usize];
    Col::Rgb(r, g, b)
}

/// xterm 256-color index → RGB.
fn xterm256(i: u8) -> Col {
    match i {
        0..=15 => bios(i),
        16..=231 => {
            let i = i - 16;
            let steps = [0u8, 95, 135, 175, 215, 255];
            Col::Rgb(
                steps[(i / 36) as usize],
                steps[((i / 6) % 6) as usize],
                steps[(i % 6) as usize],
            )
        }
        232..=255 => {
            let v = 8 + 10 * (i - 232);
            Col::Rgb(v, v, v)
        }
    }
}

fn push_escaped(out: &mut String, ch: char) {
    match ch {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        c => out.push(c),
    }
}

fn write_style(out: &mut String, s: Sgr) {
    let (mut fg, mut bg) = (s.fg, s.bg);
    if s.reverse {
        std::mem::swap(&mut fg, &mut bg);
    }
    out.push_str("<span style=\"");
    if let Col::Rgb(r, g, b) = fg {
        let _ = write!(out, "color:#{r:02x}{g:02x}{b:02x};");
    }
    if let Col::Rgb(r, g, b) = bg {
        let _ = write!(out, "background:#{r:02x}{g:02x}{b:02x};");
    }
    if s.bold {
        out.push_str("font-weight:bold;");
    }
    if s.underline {
        out.push_str("text-decoration:underline;");
    }
    out.push_str("\">");
}

/// Apply one CSI `…m` parameter list to the running SGR state.
fn apply_sgr(state: &mut Sgr, params: &[i64]) {
    let mut it = params.iter().copied().peekable();
    while let Some(p) = it.next() {
        match p {
            0 => *state = Sgr::reset(),
            1 => state.bold = true,
            22 => state.bold = false,
            4 => state.underline = true,
            24 => state.underline = false,
            7 => state.reverse = true,
            27 => state.reverse = false,
            30..=37 => state.fg = bios((p - 30) as u8),
            90..=97 => state.fg = bios((p - 90 + 8) as u8),
            39 => state.fg = Col::Default,
            40..=47 => state.bg = bios((p - 40) as u8),
            100..=107 => state.bg = bios((p - 100 + 8) as u8),
            49 => state.bg = Col::Default,
            38 | 48 => {
                let target_fg = p == 38;
                match it.next() {
                    Some(5) => {
                        if let Some(n) = it.next() {
                            let c = xterm256(n as u8);
                            if target_fg {
                                state.fg = c
                            } else {
                                state.bg = c
                            }
                        }
                    }
                    Some(2) => {
                        let r = it.next().unwrap_or(0) as u8;
                        let g = it.next().unwrap_or(0) as u8;
                        let b = it.next().unwrap_or(0) as u8;
                        let c = Col::Rgb(r, g, b);
                        if target_fg {
                            state.fg = c
                        } else {
                            state.bg = c
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

/// One rendered screen cell: a character with its resolved RGB colors.
/// Produced by [`parse_grid`] for the GIF rasterizer (`gif.rs`).
#[derive(Clone, Copy)]
pub struct Cell {
    pub ch: char,
    pub fg: (u8, u8, u8),
    pub bg: (u8, u8, u8),
    pub bold: bool,
}

fn resolve(c: Col, default_rgb: (u8, u8, u8)) -> (u8, u8, u8) {
    match c {
        Col::Rgb(r, g, b) => (r, g, b),
        Col::Default => default_rgb,
    }
}

/// Parse ANSI/SGR text (from `tmux capture-pane -e -p -N`) into a grid of cells,
/// one inner `Vec` per screen row. Shares the SGR engine with [`ansi_to_html`];
/// `Col::Default` resolves to light-grey on black (TV paints real colours, so
/// the defaults only show through where tmux emitted no SGR).
pub fn parse_grid(input: &str) -> Vec<Vec<Cell>> {
    const DEF_FG: (u8, u8, u8) = (170, 170, 170);
    const DEF_BG: (u8, u8, u8) = (0, 0, 0);
    let mut rows: Vec<Vec<Cell>> = vec![Vec::new()];
    let mut state = Sgr::reset();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                let mut buf = String::new();
                let mut final_byte = None;
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        final_byte = Some(c);
                        break;
                    }
                    buf.push(c);
                }
                if final_byte == Some('m') {
                    let params: Vec<i64> = if buf.is_empty() {
                        vec![0]
                    } else {
                        buf.split(';')
                            .map(|s| s.parse::<i64>().unwrap_or(0))
                            .collect()
                    };
                    apply_sgr(&mut state, &params);
                }
            }
            continue;
        }
        if ch == '\r' {
            continue;
        }
        if ch == '\n' {
            rows.push(Vec::new());
            continue;
        }
        let (mut fg, mut bg) = (state.fg, state.bg);
        if state.reverse {
            std::mem::swap(&mut fg, &mut bg);
        }
        rows.last_mut().unwrap().push(Cell {
            ch,
            fg: resolve(fg, DEF_FG),
            bg: resolve(bg, DEF_BG),
            bold: state.bold,
        });
    }
    if rows.last().is_some_and(|r| r.is_empty()) {
        rows.pop();
    }
    rows
}

/// Convert ANSI/SGR text (from `tmux capture-pane -e -p`) to an HTML fragment.
pub fn ansi_to_html(input: &str) -> String {
    let mut out = String::from("<pre class=\"tv-screen\">");
    let mut state = Sgr::reset();
    let mut span_open = false;
    let mut chars = input.chars().peekable();

    let close_span = |out: &mut String, open: &mut bool| {
        if *open {
            out.push_str("</span>");
            *open = false;
        }
    };

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Expect CSI: '[' … final byte. Only `m` (SGR) is meaningful here;
            // any other CSI final byte is consumed and ignored.
            if chars.peek() == Some(&'[') {
                chars.next();
                let mut buf = String::new();
                let mut final_byte = None;
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        final_byte = Some(c);
                        break;
                    }
                    buf.push(c);
                }
                if final_byte == Some('m') {
                    let params: Vec<i64> = if buf.is_empty() {
                        vec![0]
                    } else {
                        buf.split(';')
                            .map(|s| s.parse::<i64>().unwrap_or(0))
                            .collect()
                    };
                    close_span(&mut out, &mut span_open);
                    apply_sgr(&mut state, &params);
                }
            }
            continue;
        }

        // Printable (or newline). Open a span lazily if the state is non-default.
        if !span_open && state != Sgr::reset() {
            write_style(&mut out, state);
            span_open = true;
        }
        push_escaped(&mut out, ch);
    }

    close_span(&mut out, &mut span_open);
    out.push_str("</pre>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::ansi_to_html;

    fn rgb(i: usize) -> String {
        let (r, g, b) = rstv::Color::BIOS_RGB[i];
        format!("#{r:02x}{g:02x}{b:02x}")
    }

    #[test]
    fn wraps_in_pre_and_escapes_html() {
        let out = ansi_to_html("a <b> & \"c\"");
        assert!(out.starts_with("<pre class=\"tv-screen\">"));
        assert!(out.trim_end().ends_with("</pre>"));
        assert!(out.contains("a &lt;b&gt; &amp; \"c\""));
        assert!(!out.contains("<b>"));
    }

    #[test]
    fn foreground_base_color_maps_to_bios_rgb() {
        // SGR 31 = foreground BIOS index 1 (blue in the TV palette).
        let out = ansi_to_html("\x1b[31mX\x1b[0m");
        assert!(out.contains(&format!("color:{}", rgb(1))), "got: {out}");
        assert!(out.contains(">X<"));
    }

    #[test]
    fn background_and_bright_and_bold() {
        // 42 = bg index 2; 1 = bold; 97 = fg bright index 15.
        let out = ansi_to_html("\x1b[42;1;97mY\x1b[0m");
        assert!(out.contains(&format!("background:{}", rgb(2))));
        assert!(out.contains(&format!("color:{}", rgb(15))));
        assert!(out.contains("font-weight:bold"));
    }

    #[test]
    fn reset_closes_styling() {
        let out = ansi_to_html("\x1b[31mA\x1b[0mB");
        assert!(out.ends_with("B</pre>\n"), "got: {out}");
        // B sits outside any span: reset returned the state to default.
        let tail = &out[out.rfind("</span>").unwrap()..];
        assert_eq!(tail, "</span>B</pre>\n");
    }

    #[test]
    fn truecolor_fg() {
        let out = ansi_to_html("\x1b[38;2;10;20;30mZ\x1b[0m");
        assert!(out.contains("color:#0a141e"), "got: {out}");
    }

    #[test]
    fn indexed_256_uses_bios_for_low_16() {
        let out = ansi_to_html("\x1b[38;5;1mQ\x1b[0m");
        assert!(out.contains(&format!("color:{}", rgb(1))), "got: {out}");
    }

    #[test]
    fn indexed_256_cube_value() {
        // 16 = first cube cell = rgb(0,0,0).
        let out = ansi_to_html("\x1b[38;5;16mC\x1b[0m");
        assert!(out.contains("color:#000000"), "got: {out}");
        // 231 = last cube cell = rgb(255,255,255).
        let out2 = ansi_to_html("\x1b[38;5;231mD\x1b[0m");
        assert!(out2.contains("color:#ffffff"), "got: {out2}");
    }

    #[test]
    fn preserves_box_drawing_utf8() {
        let out = ansi_to_html("┌─┐");
        assert!(out.contains("┌─┐"));
    }

    #[test]
    fn reverse_swaps_fg_bg() {
        // fg=1, bg=2, then reverse → effective fg uses index 2, bg uses index 1.
        let out = ansi_to_html("\x1b[31;42;7mR\x1b[0m");
        assert!(out.contains(&format!("color:{}", rgb(2))), "got: {out}");
        assert!(
            out.contains(&format!("background:{}", rgb(1))),
            "got: {out}"
        );
    }
}
