//! The `Backend` trait — the object-safe terminal seam.
//!
//! The app holds a `Box<dyn Backend>`; the view tree never carries a `<B>`
//! type parameter. This trait must therefore be **object-safe** — no generic
//! methods.

use std::time::Duration;

use crate::event::Event;
use crate::screen::Cell;

/// Platform seam between the framework and the terminal.
///
/// Two implementations exist:
/// - [`CrosstermBackend`](crate::backend::CrosstermBackend) — production, wraps crossterm.
/// - [`HeadlessBackend`](crate::backend::HeadlessBackend) — tests, in-memory buffer.
///
/// The trait is object-safe; the app holds `Box<dyn Backend>`.
///
/// # Turbo Vision heritage
/// Stands in for the platform layer of `TScreen` / `THardwareInfo`
/// (`hardware.cpp`, `tscreen.cpp`) plus the event source pump. The original
/// selected a platform driver at link time; here the seam is a runtime trait
/// object so the view tree carries no backend type parameter, and tests can swap
/// in an in-memory fake (deviation D11).
pub trait Backend {
    /// Live terminal dimensions in cells, returned as `(cols, rows)`.
    ///
    /// Called on each pump cycle to size the root group and therefore the
    /// desktop. Because the query is on-demand there is no mutable global to
    /// keep in sync: the program resizes the desktop whenever the value
    /// changes between cycles.
    ///
    /// The production [`CrosstermBackend`](crate::backend::CrosstermBackend)
    /// asks the terminal at every call; the
    /// [`HeadlessBackend`](crate::backend::HeadlessBackend) returns the fixed
    /// size passed to [`HeadlessBackend::new`](crate::backend::HeadlessBackend::new).
    ///
    /// Custom backend implementors should query the OS terminal size here —
    /// the result is used directly for desktop layout, so a stale value will
    /// leave the view tree sized incorrectly after a terminal resize.
    ///
    /// # Turbo Vision heritage
    ///
    /// Replaces the `ScreenWidth` and `ScreenHeight` mutable globals
    /// (`Byte` each, set once by `InitVideo` in `drivers.cpp`). The Rust port
    /// queries on demand instead of caching a stale global, so terminal
    /// resizes are picked up automatically on the next pump cycle.
    fn size(&self) -> (u16, u16);

    /// Apply changed cells to the screen.
    ///
    /// Each tuple is `(x, y, &cell)`; the backend writes that cell at position
    /// `(x, y)`.  Called by [`Renderer::render`](crate::backend::Renderer::render)
    /// with the diff slice produced by [`Buffer::diff`](crate::screen::Buffer::diff).
    fn draw(&mut self, content: &[(u16, u16, &Cell)]);

    /// Flush buffered output to the terminal. No-op for headless.
    fn flush(&mut self);

    /// Show the hardware cursor at `pos`, or hide it when `pos` is `None`.
    fn set_cursor(&mut self, pos: Option<(u16, u16)>);

    /// Wait up to `timeout` for the next input event.
    ///
    /// - `None` timeout → block indefinitely (production) or return immediately
    ///   (headless — see the determinism note below).
    /// - Returns `None` on timeout or when the queue is empty.
    ///
    /// **Headless never blocks** — it pops the next queued event or returns
    /// `None` immediately, ignoring the timeout value.  This is the headless
    /// determinism contract: test code injects events and drives the loop
    /// synchronously without wall-clock waits.
    ///
    /// # Turbo Vision heritage
    ///
    /// There is deliberately no app-level `getEvent` override / event-source
    /// injection seam (C++ `TProgram::getEvent`). For periodic work use the timer
    /// queue ([`Event::Timer`]) or [`crate::app::Program::set_on_idle`]; to feed synthetic
    /// input in tests, push onto the headless backend's event queue.
    fn poll_event(&mut self, timeout: Option<Duration>) -> Option<Event>;

    /// Write `text` to the system clipboard.
    ///
    /// Returns `false` when the implementation fell back to an internal buffer
    /// (no native clipboard took the text).  The caller can treat a `false`
    /// return as "clipboard unavailable but the string is stored internally
    /// and can be retrieved via `get_clipboard`".  Native first, internal only
    /// on failure.  The production impl runs the full fallback chain (native →
    /// OSC 52 emit → internal — see `backend::clipboard`); headless is a
    /// plain internal string by design (the test fake).
    fn set_clipboard(&mut self, text: &str) -> bool;

    /// Read the clipboard: native clipboard first, else the internal buffer,
    /// else `None`.
    fn get_clipboard(&mut self) -> Option<String>;

    /// Suspend the terminal: leave alt-screen and restore normal terminal mode.
    ///
    /// Called by the event loop immediately before the process is suspended
    /// (e.g. `SIGTSTP` on Unix, or the DOS-shell command on any platform).
    /// A correct implementation tears down raw mode and the alternate screen
    /// so the shell that takes over sees a clean terminal.
    ///
    /// The production
    /// [`CrosstermBackend`](crate::backend::CrosstermBackend) executes
    /// crossterm's `LeaveAlternateScreen` + `DisableMouseCapture` + `disable_raw_mode`
    /// sequence. The
    /// [`HeadlessBackend`](crate::backend::HeadlessBackend) is a no-op by
    /// design (no real terminal to tear down).
    ///
    /// Custom backend implementors **must** pair this with [`resume`](Self::resume):
    /// every `suspend` call will be followed by exactly one `resume` call when
    /// the process is brought back to the foreground.
    ///
    /// # Turbo Vision heritage
    ///
    /// Stands in for `TApplication::suspend` (`tapplica.cpp`), which called
    /// `TSystemError::suspend`, `TEventQueue::suspend`, and `TScreen::suspend`
    /// in sequence. The Rust port collapses all three subsystem calls into this
    /// single trait method on the `Backend`.
    fn suspend(&mut self) {}

    /// Resume the terminal: re-enter alt-screen, raw mode, and mouse capture.
    ///
    /// Called by the event loop immediately after the process returns to the
    /// foreground following a [`suspend`](Self::suspend). A correct
    /// implementation restores the full terminal state so the next draw cycle
    /// re-paints the TUI correctly.
    ///
    /// The production
    /// [`CrosstermBackend`](crate::backend::CrosstermBackend) executes
    /// crossterm's `enable_raw_mode` + `EnterAlternateScreen` + `EnableMouseCapture`
    /// sequence, followed by a full redraw. The
    /// [`HeadlessBackend`](crate::backend::HeadlessBackend) is a no-op.
    ///
    /// # Turbo Vision heritage
    ///
    /// Stands in for `TApplication::resume` (`tapplica.cpp`), which called
    /// `TScreen::resume`, `TEventQueue::resume`, and `TSystemError::resume`
    /// in sequence.
    fn resume(&mut self) {}
}
