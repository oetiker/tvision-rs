//! Headless backend — the deterministic test backend.
//!
//! `HeadlessBackend` stores an in-memory cell buffer and a programmable event
//! queue.  It never performs I/O and never blocks.  Tests keep a `HeadlessHandle`
//! to inject events and inspect the screen after each `Renderer::render` call.
//!
//! The shared-state pattern avoids downcasting: the handle and the backend share
//! the same `Rc<RefCell<…>>` cells, so tests can observe the screen without
//! needing to reach through the `Box<dyn Backend>` that the `Renderer` owns.
//!
//! # Turbo Vision heritage
//! An rstv addition with no Turbo Vision counterpart: it stands in for the
//! platform terminal driver so the snapshot test suite can drive the framework
//! synchronously, with injected input and an inspectable screen (deviation D11).

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
    clipboard: Rc<RefCell<String>>,
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

    /// Queue a bracketed-paste event.  Equivalent to the terminal delivering
    /// an `EnableBracketedPaste`-wrapped paste sequence.
    pub fn push_paste(&self, text: impl Into<String>) {
        self.push_event(Event::Paste(text.into()));
    }

    /// Borrow the current screen buffer (for assertions beyond the snapshot).
    pub fn buffer(&self) -> std::cell::Ref<'_, Buffer> {
        self.buffer.borrow()
    }

    /// The current cursor position: `Some((x, y))` when visible, `None` when hidden.
    pub fn cursor(&self) -> Option<(u16, u16)> {
        self.cursor.get()
    }

    /// The backend's clipboard text, or `None` if nothing has been written or
    /// the stored text is empty (an empty string is indistinguishable from
    /// never-written — mirroring `ClipboardChain::get`'s empty→`None`) —
    /// lets tests assert what a copy path (`Deferred::SetClipboard`) stored.
    pub fn clipboard(&self) -> Option<String> {
        let clip = self.clipboard.borrow();
        if clip.is_empty() {
            None
        } else {
            Some(clip.clone())
        }
    }

    /// Seed the backend clipboard — lets tests stage text for a paste path
    /// (`Deferred::EditorPaste`) without going through `set_clipboard`.
    pub fn set_clipboard(&self, text: &str) {
        *self.clipboard.borrow_mut() = text.to_string();
    }
}

// ---------------------------------------------------------------------------
// HeadlessBackend
// ---------------------------------------------------------------------------

/// In-memory backend for tests.
///
/// Create with [`HeadlessBackend::new`]; move the backend into a
/// [`Renderer`](crate::backend::Renderer) and keep the returned
/// [`HeadlessHandle`] to observe and drive it.
pub struct HeadlessBackend {
    shared: HeadlessHandle,
    size: (u16, u16),
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
            clipboard: Rc::new(RefCell::new(String::new())),
        };
        let backend = HeadlessBackend {
            shared: shared.clone(),
            size: (width, height),
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
    /// **Never blocks** regardless of `timeout`.  This is the determinism
    /// contract: tests drive the loop synchronously by pre-loading events.
    fn poll_event(&mut self, _timeout: Option<Duration>) -> Option<Event> {
        self.shared.events.borrow_mut().pop_front()
    }

    /// Store `text` in the shared internal buffer; always returns `false`.
    ///
    /// Headless deliberately does **not** run the production
    /// [`ClipboardChain`](super::clipboard::ClipboardChain) — it is the test
    /// fake: no OS clipboard, no OSC 52 bytes, just a plain string tests can
    /// read via [`HeadlessHandle::clipboard`] and seed via
    /// [`HeadlessHandle::set_clipboard`].
    fn set_clipboard(&mut self, text: &str) -> bool {
        *self.shared.clipboard.borrow_mut() = text.to_string();
        false // internal fallback — no real clipboard
    }

    /// Return the most recently stored clipboard text, or `None` if nothing
    /// has been written.
    fn get_clipboard(&mut self) -> Option<String> {
        self.shared.clipboard()
    }
}
