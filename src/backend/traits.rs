//! The `Backend` trait — deviation **D11**.
//!
//! The app holds a `Box<dyn Backend>`; the view tree never carries a `<B>`
//! type parameter. This trait must therefore be **object-safe** — no generic
//! methods.

use std::time::Duration;

use crate::event::Event;
use crate::screen::Cell;

/// Platform seam between the framework and the terminal (D11).
///
/// Two implementations exist:
/// - [`CrosstermBackend`](crate::backend::CrosstermBackend) — production, wraps crossterm.
/// - [`HeadlessBackend`](crate::backend::HeadlessBackend) — tests, in-memory buffer.
///
/// The trait is object-safe; the app holds `Box<dyn Backend>`.
pub trait Backend {
    /// Terminal size in cells `(cols, rows)`.
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
    ///   (headless — see D11 determinism note).
    /// - Returns `None` on timeout or when the queue is empty.
    ///
    /// **Headless never blocks** — it pops the next queued event or returns
    /// `None` immediately, ignoring the timeout value.  This is the D11
    /// determinism contract: test code injects events and drives the loop
    /// synchronously without wall-clock waits.
    fn poll_event(&mut self, timeout: Option<Duration>) -> Option<Event>;

    /// Write `text` to the system clipboard.
    ///
    /// Returns `false` when the implementation fell back to an internal buffer
    /// (i.e. the terminal has no clipboard support).  The caller can treat a
    /// `false` return as "clipboard unavailable but the string is stored
    /// internally and can be retrieved via `get_clipboard`".
    fn set_clipboard(&mut self, text: &str) -> bool;

    /// Read the clipboard.
    ///
    /// Returns the most-recently written text, or `None` if nothing has been
    /// written yet.
    fn get_clipboard(&mut self) -> Option<String>;
}
