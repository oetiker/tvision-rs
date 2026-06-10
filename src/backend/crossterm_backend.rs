//! Crossterm backend — production terminal implementation (deviation **D11**).
//!
//! Wraps `crossterm` for terminal I/O behind the [`Backend`] trait.
//!
//! This backend is the production path.  It does not run in CI (no TTY), but it
//! must compile and be correct/reasonable.
//!
//! ## Setup (TODO)
//! Raw-mode, alternate-screen, and mouse-capture setup are not yet wired into
//! `CrosstermBackend::new()`.  The event loop (`Program`, row 31) is live, but
//! the terminal-lifecycle calls (`enable_raw_mode`, `EnterAlternateScreen`,
//! `EnableMouseCapture`) have not been added here yet.
//!
//! ## Clipboard
//! The clipboard is implemented as an internal string buffer.  OSC 52 terminal
//! clipboard passthrough will be added when the editor (row 66) needs it.
//!
//! ## Color depth
//! `ColorDepth` selects which rung of the quantization ladder (row 5) to use
//! when mapping our `Color` enum to crossterm colors.
//!
//! ## Key/event translation
//! `crossterm::event::Event` is translated to our `Event` enum.  Only
//! `KeyEventKind::Press` and `Repeat` are translated; `Release` is ignored.

use std::io::{self, Stdout, Write as _};
use std::time::Duration;

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, KeyCode, KeyEventKind, KeyModifiers as CKeyModifiers, MouseEventKind},
    queue,
    style::{Attribute, Color as CColor, Colors, Print, ResetColor, SetAttribute, SetColors},
};

use crate::backend::{Backend, bios_to_xterm16, rgb_to_xterm256, xterm256_to_xterm16};
use crate::color::{Color, Style};
use crate::event::{
    Event, Key, KeyEvent, KeyModifiers, MouseButtons, MouseEvent, MouseEventFlags, MouseWheel,
};
use crate::screen::Cell;
use crate::view::Point;

// ---------------------------------------------------------------------------
// ColorDepth
// ---------------------------------------------------------------------------

/// Terminal color capability level.
///
/// Controls which rung of the quantization ladder (row 5) is used when mapping
/// `Color` values to crossterm colors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorDepth {
    /// 24-bit true color.  `Color::Rgb` is passed through unchanged.
    #[default]
    TrueColor,
    /// xterm-256 palette.  `Color::Rgb` is quantized to the nearest cube/gray entry.
    Xterm256,
    /// ANSI 16 colors.  All RGB and indexed colors are reduced to the 16-color set.
    Ansi16,
    /// No color.  All colors map to the terminal default (reset).
    NoColor,
}

// ---------------------------------------------------------------------------
// CrosstermBackend
// ---------------------------------------------------------------------------

/// Production backend backed by crossterm.
///
/// Construct with [`CrosstermBackend::new`].  Raw mode and alternate-screen
/// setup are intentionally deferred to the event-loop row (row 31).
pub struct CrosstermBackend {
    out: Stdout,
    color_depth: ColorDepth,
    clipboard: String,
}

impl CrosstermBackend {
    /// Construct a `CrosstermBackend` writing to stdout.
    ///
    /// NOTE: raw mode, alternate-screen, and mouse capture are not yet set up
    /// here (see the module-level `## Setup (TODO)` note).
    pub fn new() -> Self {
        CrosstermBackend {
            out: io::stdout(),
            color_depth: ColorDepth::TrueColor,
            clipboard: String::new(),
        }
    }

    /// Construct with an explicit color depth (useful when the caller has
    /// already detected terminal capabilities).
    pub fn with_color_depth(depth: ColorDepth) -> Self {
        CrosstermBackend {
            out: io::stdout(),
            color_depth: depth,
            clipboard: String::new(),
        }
    }
}

impl Default for CrosstermBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for CrosstermBackend {
    fn size(&self) -> (u16, u16) {
        crossterm::terminal::size().unwrap_or((80, 25))
    }

    fn draw(&mut self, content: &[(u16, u16, &Cell)]) {
        for &(x, y, cell) in content {
            // Skip wide-glyph trail cells — the terminal advances automatically
            // when the lead cell is printed.
            if cell.is_wide_trail() {
                continue;
            }

            let style = cell.style();
            let colors = map_colors(style, self.color_depth);
            let attrs = map_attributes(style);

            // Reset attributes first, then apply the cell's style.
            let _ = queue!(self.out, MoveTo(x, y), ResetColor, SetColors(colors),);
            for attr in attrs {
                let _ = queue!(self.out, SetAttribute(attr));
            }
            let _ = queue!(self.out, Print(cell.symbol()));
        }
    }

    fn flush(&mut self) {
        let _ = self.out.flush();
    }

    fn set_cursor(&mut self, pos: Option<(u16, u16)>) {
        match pos {
            Some((x, y)) => {
                let _ = queue!(self.out, MoveTo(x, y), Show);
            }
            None => {
                let _ = queue!(self.out, Hide);
            }
        }
    }

    fn poll_event(&mut self, timeout: Option<Duration>) -> Option<Event> {
        let timeout = timeout.unwrap_or(Duration::MAX);
        if event::poll(timeout).ok()? {
            translate_event(event::read().ok()?)
        } else {
            None
        }
    }

    /// Store `text` in an internal buffer; returns `false` (no OSC 52 yet).
    ///
    /// TODO: OSC 52 (D11) when the editor (row 66) needs it.
    fn set_clipboard(&mut self, text: &str) -> bool {
        self.clipboard = text.to_string();
        false // internal fallback — OSC 52 not yet implemented
    }

    fn get_clipboard(&mut self) -> Option<String> {
        if self.clipboard.is_empty() {
            None
        } else {
            Some(self.clipboard.clone())
        }
    }
}

// ---------------------------------------------------------------------------
// Color mapping helpers
// ---------------------------------------------------------------------------

/// Map a `Style`'s fg/bg to a crossterm `Colors`, applying the quantization
/// ladder at the appropriate depth.
fn map_colors(style: Style, depth: ColorDepth) -> Colors {
    Colors {
        foreground: Some(map_color(style.fg, depth)),
        background: Some(map_color(style.bg, depth)),
    }
}

/// Map a single `Color` to a `crossterm::style::Color` at the given depth.
///
/// The mapping is:
/// - `Color::Default` → `CColor::Reset` (always, regardless of depth).
/// - `Color::Rgb(r,g,b)`:
///   - TrueColor → `CColor::Rgb{r,g,b}` (passthrough).
///   - Xterm256  → `CColor::AnsiValue(rgb_to_xterm256(r,g,b))`.
///   - Ansi16    → 16-color via `rgb_to_xterm16`.
///   - NoColor   → `CColor::Reset`.
/// - `Color::Indexed(n)`:
///   - TrueColor/Xterm256 → `CColor::AnsiValue(n)`.
///   - Ansi16             → `CColor::AnsiValue(xterm256_to_xterm16(n))`.
///   - NoColor            → `CColor::Reset`.
/// - `Color::Bios(n)`:
///   4-bit BIOS index (bit0=blue, bit1=green, bit2=red, bit3=bright).
///   Convert to xterm-16 via `bios_to_xterm16(n & 0xF)`, then to a crossterm
///   16-color variant via `xterm16_to_crossterm`.
fn map_color(color: Color, depth: ColorDepth) -> CColor {
    match color {
        Color::Default => CColor::Reset,

        Color::Rgb(r, g, b) => match depth {
            ColorDepth::TrueColor => CColor::Rgb { r, g, b },
            ColorDepth::Xterm256 => CColor::AnsiValue(rgb_to_xterm256(r, g, b)),
            ColorDepth::Ansi16 => xterm16_to_crossterm(crate::backend::rgb_to_xterm16(r, g, b)),
            ColorDepth::NoColor => CColor::Reset,
        },

        Color::Indexed(n) => match depth {
            ColorDepth::TrueColor | ColorDepth::Xterm256 => CColor::AnsiValue(n),
            ColorDepth::Ansi16 => CColor::AnsiValue(xterm256_to_xterm16(n)),
            ColorDepth::NoColor => CColor::Reset,
        },

        Color::Bios(n) => {
            // BIOS 4-bit index: mask to 0..=15, then bit-swap to xterm-16 order.
            let xt16 = bios_to_xterm16(n & 0xF);
            match depth {
                ColorDepth::NoColor => CColor::Reset,
                _ => xterm16_to_crossterm(xt16),
            }
        }
    }
}

/// Map an xterm-16 index (0..=15) to a `crossterm::style::Color` named-color variant.
///
/// xterm-16 layout: bit0=red, bit1=green, bit2=blue, bit3=bright.
///
/// ```text
///  0 = Black          8 = DarkGrey
///  1 = DarkRed        9 = Red
///  2 = DarkGreen     10 = Green
///  3 = DarkYellow    11 = Yellow
///  4 = DarkBlue      12 = Blue
///  5 = DarkMagenta   13 = Magenta
///  6 = DarkCyan      14 = Cyan
///  7 = Grey          15 = White
/// ```
fn xterm16_to_crossterm(idx: u8) -> CColor {
    match idx & 0xF {
        0 => CColor::Black,
        1 => CColor::DarkRed,
        2 => CColor::DarkGreen,
        3 => CColor::DarkYellow,
        4 => CColor::DarkBlue,
        5 => CColor::DarkMagenta,
        6 => CColor::DarkCyan,
        7 => CColor::Grey,
        8 => CColor::DarkGrey,
        9 => CColor::Red,
        10 => CColor::Green,
        11 => CColor::Yellow,
        12 => CColor::Blue,
        13 => CColor::Magenta,
        14 => CColor::Cyan,
        15 => CColor::White,
        _ => unreachable!("masked to 0..=15"),
    }
}

/// Map `Style` modifiers to crossterm `Attribute`s.
///
/// `no_shadow` is an internal rstv marker (D6) with no crossterm equivalent;
/// it is silently ignored here.
fn map_attributes(style: Style) -> Vec<Attribute> {
    let m = style.modifiers;
    let mut attrs = Vec::new();
    if m.bold {
        attrs.push(Attribute::Bold);
    }
    if m.italic {
        attrs.push(Attribute::Italic);
    }
    if m.underline {
        attrs.push(Attribute::Underlined);
    }
    if m.blink {
        attrs.push(Attribute::SlowBlink);
    }
    if m.reverse {
        attrs.push(Attribute::Reverse);
    }
    if m.strike {
        attrs.push(Attribute::CrossedOut);
    }
    // m.no_shadow: internal marker, no crossterm equivalent — ignored.
    attrs
}

// ---------------------------------------------------------------------------
// Event translation
// ---------------------------------------------------------------------------

/// Translate a `crossterm::event::Event` to our `Event`.
///
/// Returns `None` for event types we don't model yet (resize, paste, focus).
fn translate_event(ev: crossterm::event::Event) -> Option<Event> {
    match ev {
        crossterm::event::Event::Key(k) => translate_key(k),
        crossterm::event::Event::Mouse(m) => translate_mouse(m),
        crossterm::event::Event::Resize(_, _) => {
            // NOTE: resize is handled without an Event variant — the pump
            // (`Program::pump_once`) polls `backend.size()` each iteration and
            // calls `renderer.resize` + `group.change_bounds` on change.  This
            // is the D9 realization of `setScreenMode`/`cmScreenChanged`;
            // deliberately no `Event::Resize` variant (avoids enum churn).
            None
        }
        crossterm::event::Event::Paste(_) => {
            // TODO(paste): bracketed paste is not modeled yet (the editor's
            // kbPaste path is also deferred).
            None
        }
        crossterm::event::Event::FocusGained | crossterm::event::Event::FocusLost => {
            // TODO(focus-events): not modeled yet.
            None
        }
    }
}

/// Translate a crossterm `KeyEvent` to our `Event::KeyDown`.
///
/// Only `Press` and `Repeat` kinds are translated; `Release` is ignored.
fn translate_key(k: crossterm::event::KeyEvent) -> Option<Event> {
    match k.kind {
        KeyEventKind::Press | KeyEventKind::Repeat => {}
        KeyEventKind::Release => return None,
    }

    let key = translate_key_code(k.code)?;
    let mut modifiers = translate_modifiers(k.modifiers);

    // crossterm reports BackTab as a separate code; normalize to Tab + shift.
    if k.code == KeyCode::BackTab {
        modifiers.shift = true;
    }

    Some(Event::KeyDown(KeyEvent::new(key, modifiers)))
}

/// Translate a crossterm `KeyCode` to our `Key`.
fn translate_key_code(code: KeyCode) -> Option<Key> {
    match code {
        KeyCode::Char(c) => Some(Key::Char(c)),
        KeyCode::F(n) => Some(Key::F(n)),
        KeyCode::Enter => Some(Key::Enter),
        KeyCode::Esc => Some(Key::Esc),
        KeyCode::Backspace => Some(Key::Backspace),
        KeyCode::Tab | KeyCode::BackTab => Some(Key::Tab),
        KeyCode::Up => Some(Key::Up),
        KeyCode::Down => Some(Key::Down),
        KeyCode::Left => Some(Key::Left),
        KeyCode::Right => Some(Key::Right),
        KeyCode::Home => Some(Key::Home),
        KeyCode::End => Some(Key::End),
        KeyCode::PageUp => Some(Key::PageUp),
        KeyCode::PageDown => Some(Key::PageDown),
        KeyCode::Insert => Some(Key::Insert),
        KeyCode::Delete => Some(Key::Delete),
        // All other keycodes (null, modifier-only keys, etc.) are not modeled.
        _ => None,
    }
}

/// Translate crossterm modifier flags to our `KeyModifiers`.
fn translate_modifiers(mods: CKeyModifiers) -> KeyModifiers {
    KeyModifiers {
        shift: mods.contains(CKeyModifiers::SHIFT),
        ctrl: mods.contains(CKeyModifiers::CONTROL),
        alt: mods.contains(CKeyModifiers::ALT),
    }
}

/// Translate a crossterm `MouseEvent` to one of our mouse `Event` variants.
fn translate_mouse(m: crossterm::event::MouseEvent) -> Option<Event> {
    let position = Point::new(m.column as i32, m.row as i32);
    let modifiers = KeyModifiers {
        shift: m.modifiers.contains(CKeyModifiers::SHIFT),
        ctrl: m.modifiers.contains(CKeyModifiers::CONTROL),
        alt: m.modifiers.contains(CKeyModifiers::ALT),
    };

    match m.kind {
        MouseEventKind::Down(btn) => {
            let mut buttons = MouseButtons::default();
            set_button(&mut buttons, btn);
            Some(Event::MouseDown(MouseEvent {
                position,
                buttons,
                modifiers,
                ..Default::default()
            }))
        }
        MouseEventKind::Up(btn) => {
            let mut buttons = MouseButtons::default();
            set_button(&mut buttons, btn);
            Some(Event::MouseUp(MouseEvent {
                position,
                buttons,
                modifiers,
                ..Default::default()
            }))
        }
        MouseEventKind::Drag(btn) => {
            let mut buttons = MouseButtons::default();
            set_button(&mut buttons, btn);
            Some(Event::MouseMove(MouseEvent {
                position,
                buttons,
                modifiers,
                flags: MouseEventFlags {
                    mouse_moved: true,
                    ..Default::default()
                },
                ..Default::default()
            }))
        }
        MouseEventKind::Moved => Some(Event::MouseMove(MouseEvent {
            position,
            modifiers,
            flags: MouseEventFlags {
                mouse_moved: true,
                ..Default::default()
            },
            ..Default::default()
        })),
        MouseEventKind::ScrollUp => Some(Event::MouseDown(MouseEvent {
            position,
            modifiers,
            wheel: MouseWheel::Up,
            ..Default::default()
        })),
        MouseEventKind::ScrollDown => Some(Event::MouseDown(MouseEvent {
            position,
            modifiers,
            wheel: MouseWheel::Down,
            ..Default::default()
        })),
        MouseEventKind::ScrollLeft => Some(Event::MouseDown(MouseEvent {
            position,
            modifiers,
            wheel: MouseWheel::Left,
            ..Default::default()
        })),
        MouseEventKind::ScrollRight => Some(Event::MouseDown(MouseEvent {
            position,
            modifiers,
            wheel: MouseWheel::Right,
            ..Default::default()
        })),
    }
}

/// Set the matching button field on `buttons`.
fn set_button(buttons: &mut MouseButtons, btn: crossterm::event::MouseButton) {
    match btn {
        crossterm::event::MouseButton::Left => buttons.left = true,
        crossterm::event::MouseButton::Right => buttons.right = true,
        crossterm::event::MouseButton::Middle => buttons.middle = true,
    }
}
