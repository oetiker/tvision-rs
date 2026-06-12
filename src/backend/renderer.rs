//! Renderer — the per-frame draw cycle.
//!
//! The `Renderer` owns the back/front `Buffer` pair plus a boxed `Backend`.  Each
//! frame it:
//! 1. Resets the back buffer.
//! 2. Runs the paint closure into the back buffer.
//! 3. Diffs the back buffer against the front buffer.
//! 4. Pushes changed cells to the backend.
//! 5. Sets the cursor.
//! 6. Flushes.
//! 7. Swaps back/front so the new frame becomes the reference for the next diff.
//!
//! This mirrors the role of ratatui's `Terminal::draw` but is simpler: there is
//! no partial-repaint or damage-tracking — the whole view tree is painted every
//! frame and the cell diff keeps the terminal I/O bounded.
//!
//! # Turbo Vision heritage
//! Replaces Turbo Vision's incremental, per-view screen writes (each view wrote
//! its buffer straight through the screen driver) with a double-buffered
//! whole-tree repaint plus diff. The per-view damage tracking is dropped in favour
//! of repaint-and-diff (deviation D8).

use crate::backend::Backend;
use crate::screen::Buffer;

/// Owns the back/front buffer pair and a boxed [`Backend`].
///
/// Constructed with [`Renderer::new`]; the frame cycle is [`Renderer::render`].
pub struct Renderer {
    backend: Box<dyn Backend>,
    /// `buffers[0]` and `buffers[1]` alternate roles each frame.
    buffers: [Buffer; 2],
    /// Index of the *back* buffer (the one being painted this frame).
    current: usize,
    /// Desired hardware cursor position: `Some((x,y))` or `None` (hidden).
    cursor: Option<(u16, u16)>,
}

impl Renderer {
    /// Create a new `Renderer` backed by `backend`.
    ///
    /// Both buffers are initialized to the backend's current size.
    pub fn new(backend: Box<dyn Backend>) -> Self {
        let (w, h) = backend.size();
        Renderer {
            backend,
            buffers: [Buffer::new(w, h), Buffer::new(w, h)],
            current: 0,
            cursor: None,
        }
    }

    /// Run one frame cycle.
    ///
    /// 1. Resets the back buffer to blank.
    /// 2. Calls `paint` with a mutable reference to the back buffer.
    /// 3. Diffs the back buffer against the front buffer.
    /// 4. Pushes changed cells to the backend.
    /// 5. Sets the cursor.
    /// 6. Flushes.
    /// 7. Swaps back/front.
    ///
    /// The borrow structure: `self.buffers[back]` and `self.buffers[front]` are
    /// distinct elements (different indices), so Rust's disjoint-field borrow
    /// lets the compiler see that `diff` borrows `self.buffers[front]` while
    /// `self.backend` is a separate field. No cloning is needed.
    pub fn render(&mut self, paint: impl FnOnce(&mut Buffer)) {
        let back = self.current;
        let front = 1 - self.current;

        // Reset the back buffer, then paint into it.
        self.buffers[back].reset();
        paint(&mut self.buffers[back]);

        // Compute the diff (borrows self.buffers[front] and self.buffers[back]).
        // We collect into a Vec to end the borrow on self.buffers before calling
        // self.backend.draw (which also needs &mut self).
        let diff: Vec<(u16, u16, _)> = self.buffers[front].diff(&self.buffers[back]);

        // Push changes to the backend.
        self.backend.draw(&diff);
        self.backend.set_cursor(self.cursor);
        self.backend.flush();

        // Swap: the back buffer is now the reference for the next frame.
        self.current = front;
    }

    /// Set the desired cursor position (used by the next [`render`](Self::render) call).
    pub fn set_cursor(&mut self, pos: Option<(u16, u16)>) {
        self.cursor = pos;
    }

    /// Borrow the backend.
    pub fn backend(&self) -> &dyn Backend {
        &*self.backend
    }

    /// Borrow the backend mutably (e.g. to call `poll_event`).
    pub fn backend_mut(&mut self) -> &mut dyn Backend {
        &mut *self.backend
    }

    /// Resize both buffers to `width × height`, resetting all content.
    ///
    /// Call this when the terminal reports a resize.
    pub fn resize(&mut self, width: u16, height: u16) {
        self.buffers[0].resize(width, height);
        self.buffers[1].resize(width, height);
    }

    /// Force a full terminal repaint on the next [`render`](Self::render) call.
    ///
    /// Clears the front (reference) buffer so the next diff sees every cell as
    /// changed. Call after [`Backend::resume`] — the terminal's alt-screen is
    /// blank, so a full repaint is required.
    pub fn invalidate_all(&mut self) {
        let front = 1 - self.current;
        self.buffers[front].reset();
    }
}
