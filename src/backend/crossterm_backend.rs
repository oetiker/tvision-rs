//! Crossterm backend — the production terminal implementation.
//!
//! Wraps `crossterm` for terminal I/O behind the [`Backend`] trait.
//!
//! This backend is the production path.  It does not run in CI (no TTY), but it
//! must compile and be correct/reasonable.
//!
//! ## Terminal lifecycle (RAII)
//! `CrosstermBackend::new()` performs the full terminal setup — raw mode,
//! alternate screen, mouse capture — at construction; `Drop` restores the
//! terminal (best-effort, errors ignored), so an rstv program never hand-rolls
//! terminal setup.
//!
//! Two process-global hooks (installed once, on the first construction) keep
//! the user's terminal usable when the program does not exit normally:
//!
//! * a **panic hook** that restores the terminal before delegating to the
//!   previous hook — so the panic message prints on the normal screen, and a
//!   crashed TUI app leaves the shell sane;
//! * (unix) a **signal thread** that restores the terminal on `SIGINT`,
//!   `SIGTERM`, or `SIGHUP` and exits with status `128 + signal_number` (the
//!   shell convention: 130 for SIGINT, 143 for SIGTERM, 129 for SIGHUP).
//!   Signals are handled on a dedicated thread — not in an async-signal
//!   context — so calling into crossterm is sound. (`SIGKILL` is uncatchable;
//!   a `kill -9` still leaves the terminal dirty — run `reset` to recover.)
//!
//! The restore sequence is idempotent, so Drop, the panic hook, and the signal
//! thread may each run it without harm. Both hooks persist for the process
//! lifetime (a later restore on an already-restored terminal is a no-op).
//!
//! ## Clipboard
//! The clipboard is the [`ClipboardChain`] (see `backend::clipboard`):
//!
//! - **Copy** (`set_clipboard`): OS-native via arboard (`os-clipboard`
//!   feature, on by default) → OSC 52 escape sequence queued on the normal
//!   output handle (delivered by the next `flush`) → internal string buffer.
//!   Returns `true` only on the native rung (trait contract: `false` = fell
//!   back to internal).
//! - **Paste** (`get_clipboard`): OS-native → internal buffer → `None`.
//!   There is no OSC 52 *read* rung — the capability probes it needs require
//!   owning the input parser, which crossterm (not rstv) owns.
//!
//! **SSH story:** with no display, arboard init fails and the chain runs
//! without its native rung — but the OSC 52 emit still reaches the *local*
//! terminal's clipboard on copy, which is exactly what a remote TUI wants.
//! Paste over SSH falls back to the internal buffer (copies made inside the
//! app) or the terminal's own bracketed paste, which arrives as
//! [`Event::Paste`](crate::event::Event::Paste).
//!
//! ## Color depth
//! `ColorDepth` selects which rung of the quantization ladder to use when
//! mapping our `Color` enum to crossterm colors.
//!
//! ## Key/event translation
//! `crossterm::event::Event` is translated to our `Event` enum.  Only
//! `KeyEventKind::Press` and `Repeat` are translated; `Release` is ignored.
//!
//! # Turbo Vision heritage
//! Replaces the platform driver behind `TScreen` / `THardwareInfo` and the Unix
//! terminal I/O of `unixcon.cpp` / `termio.cpp` with a crossterm-backed
//! [`Backend`]. Terminal setup that the original did in the application
//! constructor chain is RAII here: construction sets the terminal up, `Drop` tears
//! it down.

use std::io::{self, Stdout, Write as _};
use std::time::Duration;

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        KeyCode, KeyEventKind, KeyModifiers as CKeyModifiers, MouseEventKind,
    },
    execute, queue,
    style::{Attribute, Color as CColor, Colors, Print, ResetColor, SetAttribute, SetColors},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use crate::backend::clipboard::ClipboardChain;
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
/// Controls which rung of the quantization ladder is used when mapping
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
/// Construct with [`CrosstermBackend::new`].  Construction performs the full
/// terminal setup (raw mode, alternate screen, mouse capture) and `Drop`
/// restores the terminal — see the module-level `## Terminal lifecycle (RAII)`
/// note.
///
/// # Turbo Vision heritage
/// The production realization of the [`Backend`] seam — see the module-level
/// heritage note for the `TScreen` / terminal-driver lineage.
pub struct CrosstermBackend {
    out: Stdout,
    color_depth: ColorDepth,
    clipboard: ClipboardChain,
}

/// Undo the terminal setup: disable mouse capture, leave the alternate screen,
/// disable raw mode.  Best-effort and idempotent — safe to call more than once
/// (`Drop`, the panic hook, and the signal thread may each run it).
fn restore_terminal() {
    let _ = execute!(
        io::stdout(),
        DisableBracketedPaste,
        DisableMouseCapture,
        LeaveAlternateScreen
    );
    let _ = disable_raw_mode();
}

/// Install the process-global restore hooks: the panic hook (always) and the
/// unix signal thread, no matter how many backends are constructed.
///
/// **What `Ok(())` guarantees:** the panic hook is installed **and** (on
/// unix) the signal-restore thread is running.  The two are latched
/// separately:
///
/// * the panic-hook install is infallible, so it uses a plain
///   [`Once`](std::sync::Once);
/// * the signal-thread setup can fail (`Signals::new`), so it latches
///   **success-only** (see [`start_signal_thread`]): a transient first
///   failure is retried on the next construction, and a consistent failure
///   keeps returning the real `Err` instead of silently reporting `Ok`
///   without a signal thread.
///
/// The panic hook restores the terminal *before* delegating to the previous
/// hook, so the panic message prints on the normal screen.
fn install_restore_hooks() -> io::Result<()> {
    use std::sync::Once;
    static PANIC_HOOK: Once = Once::new();

    PANIC_HOOK.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_terminal();
            prev(info);
        }));
    });

    #[cfg(unix)]
    start_signal_thread()?;

    Ok(())
}

/// Start the unix signal-restore thread, at most once per process
/// (success-only latching — see [`install_restore_hooks`]).
///
/// The thread waits for `SIGINT`/`SIGTERM`/`SIGHUP`, restores the terminal,
/// and exits with status `128 + signal_number` — the shell convention (130
/// for SIGINT, 143 for SIGTERM, 129 for SIGHUP).  `Drop` does not run on
/// `process::exit`, but the terminal has already been restored.
///
/// The latch is a `Mutex<bool>` rather than an atomic: the lock is held
/// across `Signals::new` + spawn, so two backends constructed concurrently
/// cannot both spawn a thread (an `AtomicBool` checked before the fallible
/// registration would permit a benign but pointless double-spawn).  This is
/// cold-path construction code — the lock cost is irrelevant.
#[cfg(unix)]
fn start_signal_thread() -> io::Result<()> {
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    use signal_hook::iterator::Signals;
    use std::sync::Mutex;

    static STARTED: Mutex<bool> = Mutex::new(false);
    let mut started = STARTED.lock().unwrap_or_else(|e| e.into_inner());
    if *started {
        return Ok(());
    }

    let mut signals = Signals::new([SIGINT, SIGTERM, SIGHUP])?;
    std::thread::spawn(move || {
        if let Some(signum) = signals.forever().next() {
            restore_terminal();
            // `128 + signum` is the shell convention for death-by-signal
            // (130 SIGINT, 143 SIGTERM, 129 SIGHUP).
            std::process::exit(128 + signum);
        }
    });
    *started = true;
    Ok(())
}

impl CrosstermBackend {
    /// Construct a `CrosstermBackend` writing to stdout, performing the full
    /// terminal setup: enable raw mode, enter the alternate screen, enable
    /// mouse capture.  Also installs the process-global restore hooks (panic
    /// hook + unix signal thread) on first use.
    ///
    /// On error the partial setup is rolled back before returning.  The
    /// terminal is restored when the backend is dropped.
    ///
    /// # Note
    /// See the single-instance contract on [`CrosstermBackend::with_color_depth`].
    pub fn new() -> io::Result<Self> {
        Self::with_color_depth(ColorDepth::TrueColor)
    }

    /// Construct with an explicit color depth (useful when the caller has
    /// already detected terminal capabilities).  Performs the same terminal
    /// setup as [`CrosstermBackend::new`].
    ///
    /// # Note
    /// At most one `CrosstermBackend` should be live per process.  A second
    /// instance re-enters the alternate screen (terminal-dependent visual
    /// corruption); the `Drop` teardown is idempotent and harmless on its
    /// own, but two instances with different lifetimes can leave the
    /// terminal stuck in the alternate screen (the first drop restores the
    /// terminal out from under the still-live instance).
    pub fn with_color_depth(depth: ColorDepth) -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(e) = execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste
        ) {
            // Full restore, not just disable_raw_mode(): on the Windows-<10
            // WinAPI fallback crossterm executes the two commands one by one,
            // so EnterAlternateScreen may have succeeded before
            // EnableMouseCapture failed.  restore_terminal() is idempotent
            // and undoes everything in reverse order.
            restore_terminal();
            return Err(e);
        }
        if let Err(e) = install_restore_hooks() {
            restore_terminal();
            return Err(e);
        }
        Ok(CrosstermBackend {
            out: io::stdout(),
            color_depth: depth,
            // Constructed after the terminal setup succeeded; arboard init
            // failure is swallowed inside (clipboard absence must not fail
            // backend construction).
            clipboard: ClipboardChain::with_os_native(),
        })
    }
}

impl Drop for CrosstermBackend {
    /// Restore the terminal (best-effort; errors during drop are ignored).
    fn drop(&mut self) {
        restore_terminal();
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

    /// Run the copy chain (module `## Clipboard` docs): native → OSC 52 →
    /// internal. The OSC sequence is queued on the normal output handle and
    /// delivered by the next `flush` (every pump iteration flushes).
    fn set_clipboard(&mut self, text: &str) -> bool {
        self.clipboard.set(text, &mut self.out)
    }

    /// Run the paste chain: native → internal buffer → `None`.
    fn get_clipboard(&mut self) -> Option<String> {
        self.clipboard.get()
    }

    fn suspend(&mut self) {
        restore_terminal(); // idempotent teardown (already used in Drop)
    }

    fn resume(&mut self) {
        // Best-effort re-setup — mirror the setup in with_color_depth but skip
        // hook installation (already done once-per-process in the constructor).
        let _ = enable_raw_mode();
        let _ = execute!(
            self.out,
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste
        );
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
/// `no_shadow` is an internal rstv marker with no crossterm equivalent;
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
/// Returns `None` for events with no `Event` counterpart: resize (handled by
/// polling `backend.size()` in the pump) and terminal focus changes.
fn translate_event(ev: crossterm::event::Event) -> Option<Event> {
    match ev {
        crossterm::event::Event::Key(k) => translate_key(k),
        crossterm::event::Event::Mouse(m) => translate_mouse(m),
        crossterm::event::Event::Resize(_, _) => {
            // Resize is handled without an Event variant — the pump
            // (`Program::pump_once`) polls `backend.size()` each iteration and
            // calls `renderer.resize` + `group.change_bounds` on change. This
            // is the equivalent of `setScreenMode`/`cmScreenChanged`, done by
            // polling rather than via an `Event::Resize` variant (avoids enum
            // churn).
            None
        }
        crossterm::event::Event::Paste(text) => Some(Event::Paste(text)),
        crossterm::event::Event::FocusGained | crossterm::event::Event::FocusLost => {
            // Terminal focus-gained/lost has no Turbo Vision counterpart, so
            // there is no `Event` variant for it — these are dropped.
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
        MouseEventKind::ScrollUp => Some(Event::MouseWheel(MouseEvent {
            position,
            modifiers,
            wheel: MouseWheel::Up,
            ..Default::default()
        })),
        MouseEventKind::ScrollDown => Some(Event::MouseWheel(MouseEvent {
            position,
            modifiers,
            wheel: MouseWheel::Down,
            ..Default::default()
        })),
        MouseEventKind::ScrollLeft => Some(Event::MouseWheel(MouseEvent {
            position,
            modifiers,
            wheel: MouseWheel::Left,
            ..Default::default()
        })),
        MouseEventKind::ScrollRight => Some(Event::MouseWheel(MouseEvent {
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
