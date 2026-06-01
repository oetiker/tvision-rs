//! Headless backend — the deterministic test backend (deviation **D11**).
//!
//! `HeadlessBackend` stores an in-memory cell buffer and a programmable event
//! queue.  It never performs I/O and never blocks.  Tests keep a `HeadlessHandle`
//! to inject events and inspect the screen after each `Renderer::render` call.
//!
//! The shared-state pattern avoids downcasting: the handle and the backend share
//! the same `Rc<RefCell<…>>` cells, so tests can observe the screen without
//! needing to reach through the `Box<dyn Backend>` that the `Renderer` owns.

use std::cell::{Cell as StdCell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use crate::backend::Backend;
use crate::event::{Event, Key, KeyEvent, KeyModifiers};
use crate::screen::{Buffer, Cell};

// ---------------------------------------------------------------------------
// HeadlessHandle — the test-visible window into backend state
// ---------------------------------------------------------------------------

/// A cloneable window into a [`HeadlessBackend`]'s screen and event queue.
///
/// Retain this value after constructing the backend with [`HeadlessBackend::new`]
/// — the backend is then boxed and given to the [`Renderer`](crate::backend::Renderer).
/// The handle outlives that move and lets tests inspect or drive the backend:
///
/// ```ignore
/// let (backend, screen) = HeadlessBackend::new(80, 25);
/// let mut renderer = Renderer::new(Box::new(backend));
/// // … render frames …
/// insta::assert_snapshot!(screen.snapshot());
/// ```
#[derive(Clone)]
pub struct HeadlessHandle {
    buffer: Rc<RefCell<Buffer>>,
    cursor: Rc<StdCell<Option<(u16, u16)>>>,
    events: Rc<RefCell<VecDeque<Event>>>,
}

impl HeadlessHandle {
    /// The golden-snapshot string of the current screen.
    ///
    /// Delegates to [`crate::screen::snapshot::snapshot`], which produces the
    /// frozen `size / cursor / text / attr / legend` format documented on that
    /// module.  Pass the result to `insta::assert_snapshot!`.
    pub fn snapshot(&self) -> String {
        crate::screen::snapshot::snapshot(&self.buffer.borrow(), self.cursor.get())
    }

    /// Queue an input event to be returned by the next `poll_event`.
    pub fn push_event(&self, ev: Event) {
        self.events.borrow_mut().push_back(ev);
    }

    /// Convenience: queue a [`Event::KeyDown`] event.
    ///
    /// Builds a `KeyEvent` from `key` and `modifiers` and pushes
    /// `Event::KeyDown(…)` onto the queue.
    pub fn push_key(&self, key: Key, modifiers: KeyModifiers) {
        self.push_event(Event::KeyDown(KeyEvent::new(key, modifiers)));
    }

    /// Borrow the current screen buffer (for assertions beyond the snapshot).
    pub fn buffer(&self) -> std::cell::Ref<'_, Buffer> {
        self.buffer.borrow()
    }

    /// The current cursor position: `Some((x, y))` when visible, `None` when hidden.
    pub fn cursor(&self) -> Option<(u16, u16)> {
        self.cursor.get()
    }
}

// ---------------------------------------------------------------------------
// HeadlessBackend
// ---------------------------------------------------------------------------

/// In-memory backend for tests (D11).
///
/// Create with [`HeadlessBackend::new`]; move the backend into a
/// [`Renderer`](crate::backend::Renderer) and keep the returned
/// [`HeadlessHandle`] to observe and drive it.
pub struct HeadlessBackend {
    shared: HeadlessHandle,
    size: (u16, u16),
    clipboard: String,
}

impl HeadlessBackend {
    /// Create a `width × height` headless backend.
    ///
    /// Returns `(backend, handle)`: move `backend` into a `Renderer` and retain
    /// `handle` to inspect the screen and inject input.
    pub fn new(width: u16, height: u16) -> (Self, HeadlessHandle) {
        let shared = HeadlessHandle {
            buffer: Rc::new(RefCell::new(Buffer::new(width, height))),
            cursor: Rc::new(StdCell::new(None)),
            events: Rc::new(RefCell::new(VecDeque::new())),
        };
        let backend = HeadlessBackend {
            shared: shared.clone(),
            size: (width, height),
            clipboard: String::new(),
        };
        (backend, shared)
    }
}

impl Backend for HeadlessBackend {
    fn size(&self) -> (u16, u16) {
        self.size
    }

    /// Write each changed cell into the internal buffer.
    fn draw(&mut self, content: &[(u16, u16, &Cell)]) {
        let mut buf = self.shared.buffer.borrow_mut();
        for &(x, y, cell) in content {
            *buf.get_mut(x, y) = cell.clone();
        }
    }

    /// No-op: headless has no output stream.
    fn flush(&mut self) {}

    fn set_cursor(&mut self, pos: Option<(u16, u16)>) {
        self.shared.cursor.set(pos);
    }

    /// Pop the front event from the queue, or `None` if the queue is empty.
    ///
    /// **Never blocks** regardless of `timeout`.  This is the D11 determinism
    /// contract: tests drive the loop synchronously by pre-loading events.
    fn poll_event(&mut self, _timeout: Option<Duration>) -> Option<Event> {
        self.shared.events.borrow_mut().pop_front()
    }

    /// Store `text` in an internal buffer; always returns `false` (no system
    /// clipboard in headless mode).
    fn set_clipboard(&mut self, text: &str) -> bool {
        self.clipboard = text.to_string();
        false // internal fallback — no real clipboard
    }

    /// Return the most recently stored clipboard text, or `None` if nothing
    /// has been written.
    fn get_clipboard(&mut self) -> Option<String> {
        if self.clipboard.is_empty() {
            None
        } else {
            Some(self.clipboard.clone())
        }
    }
}
