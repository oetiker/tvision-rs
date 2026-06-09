//! Capture stack — deviation **D9** (single loop + LIFO capture handlers).
//!
//! C++ Turbo Vision implements modality by spinning a *nested* blocking
//! `getEvent` loop inside `execView`; `dragView` and a pressed button's
//! mouse-tracking do the same. Rust cannot nest a blocking loop that re-borrows
//! the view tree, so D9 collapses all of them into **one** non-recursive event
//! loop plus a **LIFO stack of capture handlers** that see each event *before*
//! normal view-tree routing and may consume or pass it through. Modality, drag,
//! and press-tracking become handlers, not loops; a modal handler that consumes
//! every otherwise-unhandled event *is* the modal loop. Handlers hold
//! [`ViewId`], never view references.
//!
//! **This module builds the types only.** The live event loop lands with
//! `TProgram` (row 31); the [`tests`] module here hand-plays the loop to prove
//! the protocol composes (the capture analogue of the row-19 end-to-end snapshot
//! gate).

use crate::event::Event;
use crate::view::{Context, Rect, ViewId};

/// What a capture handler did with an event it was offered.
///
/// The return value is **authoritative** for routing — handlers must *not* rely
/// on [`Event::clear`] to signal "consumed" to the capture stack (clearing is a
/// separate downstream concern handled by normal view routing).
#[derive(Debug)]
pub enum CaptureFlow {
    /// Did not handle the event — offer it to the next (lower) handler, and
    /// then to normal view-tree routing if every handler passes.
    Pass,
    /// Handled the event; stop routing. The handler stays on the stack.
    Consumed,
    /// Handled the event **and** removes ITSELF from the stack (e.g. a modal
    /// dialog closing). Unambiguous: "pop" always means the handler that just
    /// ran.
    ConsumedPop,
}

/// A capture handler — the D9 replacement for a nested modal/drag/press loop.
///
/// Handlers are offered each event before normal view-tree routing. Identity is
/// a [`ViewId`]: a handler never holds a view reference.
pub trait CaptureHandler {
    /// Offered an event before normal routing. May read/mutate `ctx` (post
    /// commands, schedule timers, push a *nested* capture via
    /// [`Context::push_capture`]).
    ///
    /// The returned [`CaptureFlow`] is **authoritative** for routing — do *not*
    /// rely on `Event::clear()` to signal "consumed" to the capture stack.
    fn handle(&mut self, ev: &mut Event, ctx: &mut Context) -> CaptureFlow;

    /// The view this handler is associated with, if any. Identity is [`ViewId`].
    fn view(&self) -> Option<ViewId> {
        None
    }

    /// Returns `true` if this handler is a modal-bounds gate (a [`ModalFrame`]
    /// equivalent).  Used by the pump's outside-modal redirect to distinguish a
    /// true modal frame from other capture handlers that also have a `view()`
    /// (drag, menu-box, etc.).  **Default is `false`** — only `ModalFrame`
    /// overrides this.
    fn is_modal_gate(&self) -> bool {
        false
    }

    /// Update the handler's cached gating bounds for its associated view, called
    /// by [`CaptureStack::sync_gate_bounds`] before each dispatch so a handler
    /// that gates events by the view's *position* (e.g. a modal frame) follows
    /// the view when it is moved/resized (a dragged dialog).
    ///
    /// **Default is a no-op** — only a handler that gates by bounds overrides it.
    /// In particular a drag handler must NOT override it: its grab anchor /
    /// initial bounds are fixed for the duration of the drag and resyncing them
    /// from the (live, moving) tree would corrupt the drag math.
    fn set_gate_bounds(&mut self, _bounds: Rect) {}
}

/// A LIFO stack of [`CaptureHandler`]s (D9).
///
/// The most-recently pushed handler is offered events first. The live event
/// loop (row 31) owns this stack and drives [`dispatch`](Self::dispatch); a
/// handler that wants to push a nested capture does so through
/// [`Context::push_capture`], whose deferred queue the loop applies *after*
/// dispatch — so the stack is never aliased while a handler runs.
#[derive(Default)]
pub struct CaptureStack {
    handlers: Vec<Box<dyn CaptureHandler>>,
}

impl CaptureStack {
    /// An empty capture stack.
    pub fn new() -> Self {
        CaptureStack {
            handlers: Vec::new(),
        }
    }

    /// Push a handler onto the top of the stack (it will see events first).
    pub fn push(&mut self, handler: Box<dyn CaptureHandler>) {
        self.handlers.push(handler);
    }

    /// Refresh every handler's gating bounds from the live tree before a dispatch.
    ///
    /// For each handler associated with a view ([`CaptureHandler::view`]), resolve
    /// that view's current bounds via `resolve` and push them down through
    /// [`CaptureHandler::set_gate_bounds`]. A bounds-gating handler (a modal frame)
    /// thus follows its view when it is dragged/resized; a handler that does not
    /// override `set_gate_bounds` (a drag handler) is unaffected. The loop owns the
    /// stack, so this is the loop's responsibility, not a handler's.
    pub fn sync_gate_bounds(&mut self, mut resolve: impl FnMut(ViewId) -> Option<Rect>) {
        for h in &mut self.handlers {
            if let Some(id) = h.view()
                && let Some(bounds) = resolve(id)
            {
                h.set_gate_bounds(bounds);
            }
        }
    }

    /// Remove and return the top handler, if any. Used by
    /// [`Program::exec_view`](crate::app::Program::exec_view) to remove the
    /// [`ModalFrame`](crate::app::ModalFrame) it pushed once the modal loop ends —
    /// the **one** place a frame is popped other than a handler self-popping via
    /// [`CaptureFlow::ConsumedPop`]. (The loop owns the stack; a handler cannot
    /// reach it to do a `valid(end_state)`-conditional pop, so the owner-side
    /// `exec_view` does it.)
    pub fn pop(&mut self) -> Option<Box<dyn CaptureHandler>> {
        self.handlers.pop()
    }

    /// Number of handlers currently on the stack.
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    /// Whether the stack has no handlers.
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    /// Returns the [`ViewId`] of the top capture handler only when it is a
    /// modal-bounds gate ([`CaptureHandler::is_modal_gate`] == `true`).
    /// Used by the pump's outside-modal redirect to avoid firing on drag or
    /// menu-box handlers that also carry a `view()`.
    pub fn top_modal_view(&self) -> Option<ViewId> {
        self.handlers
            .last()
            .and_then(|h| if h.is_modal_gate() { h.view() } else { None })
    }

    /// Offer `ev` to the handlers top-down (last pushed first).
    ///
    /// - [`CaptureFlow::Pass`] → continue to the next lower handler;
    /// - [`CaptureFlow::Consumed`] → stop, return `true`;
    /// - [`CaptureFlow::ConsumedPop`] → remove *that* handler, stop, return
    ///   `true`.
    ///
    /// Returns `false` if every handler passed (the loop then runs normal
    /// view-tree routing).
    ///
    /// A handler may push a nested capture during its `handle` call — but that
    /// goes into [`Context`]'s separate deferred queue, never into
    /// `self.handlers`, so there is no aliasing of the stack. The `ConsumedPop`
    /// removal happens *after* `handle` returns (NLL releases the index borrow
    /// at the end of the call expression).
    pub fn dispatch(&mut self, ev: &mut Event, ctx: &mut Context) -> bool {
        for i in (0..self.handlers.len()).rev() {
            match self.handlers[i].handle(ev, ctx) {
                CaptureFlow::Pass => {}
                CaptureFlow::Consumed => return true,
                CaptureFlow::ConsumedPop => {
                    self.handlers.remove(i);
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::event::{Event, Key, KeyEvent};
    use crate::timer::TimerQueue;
    use crate::view::Context;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;
    use std::time::Duration;

    /// A test handler that records every event it is offered and returns a
    /// configured [`CaptureFlow`].
    struct Recorder {
        log: Rc<RefCell<Vec<Event>>>,
        flow: fn() -> CaptureFlow,
    }

    impl Recorder {
        fn new(log: Rc<RefCell<Vec<Event>>>, flow: fn() -> CaptureFlow) -> Self {
            Recorder { log, flow }
        }
    }

    impl CaptureHandler for Recorder {
        fn handle(&mut self, ev: &mut Event, _ctx: &mut Context) -> CaptureFlow {
            self.log.borrow_mut().push(*ev);
            (self.flow)()
        }
    }

    /// A handler that, on its first (and every) event, pushes another handler
    /// via the deferred `ctx.push_capture` queue, then consumes the event.
    struct Pusher {
        /// The recorder log the pushed handler will write to.
        pushed_log: Rc<RefCell<Vec<Event>>>,
        /// Set once we've pushed, so we only push a single nested handler.
        pushed: bool,
    }

    impl CaptureHandler for Pusher {
        fn handle(&mut self, _ev: &mut Event, ctx: &mut Context) -> CaptureFlow {
            if !self.pushed {
                // Exercise the full `ctx.*` surface *during dispatch*, exactly as
                // the doc contract on `CaptureHandler::handle` promises a handler
                // may: post / broadcast / schedule a timer / push a nested capture.
                ctx.post(Command::OK);
                ctx.broadcast(Command::COMMAND_SET_CHANGED, None);
                let _tid = ctx.set_timer(Duration::from_millis(50), None);
                let inner = Recorder::new(self.pushed_log.clone(), || CaptureFlow::Consumed);
                ctx.push_capture(Box::new(inner));
                self.pushed = true;
            }
            CaptureFlow::Consumed
        }
    }

    fn key_event(k: Key) -> Event {
        Event::KeyDown(KeyEvent::from(k))
    }

    // -- per-piece protocol facts -------------------------------------------

    #[test]
    fn pass_lets_lower_handler_see_event() {
        let lower_log = Rc::new(RefCell::new(Vec::new()));
        let upper_log = Rc::new(RefCell::new(Vec::new()));

        let mut stack = CaptureStack::new();
        // lower pushed first -> seen last
        stack.push(Box::new(Recorder::new(lower_log.clone(), || {
            CaptureFlow::Consumed
        })));
        stack.push(Box::new(Recorder::new(upper_log.clone(), || {
            CaptureFlow::Pass
        })));

        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = Vec::new();
        let mut ev = key_event(Key::Enter);

        let consumed = {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            stack.dispatch(&mut ev, &mut ctx)
        };
        for effect in deferred.drain(..) {
            if let crate::view::Deferred::PushCapture(h) = effect {
                stack.push(h);
            }
        }

        // Upper passed, lower consumed.
        assert!(consumed);
        assert_eq!(upper_log.borrow().len(), 1, "upper handler saw the event");
        assert_eq!(
            lower_log.borrow().len(),
            1,
            "lower handler saw it after Pass"
        );
        // Both still on the stack (Pass + Consumed neither pop).
        assert_eq!(stack.len(), 2);
    }

    #[test]
    fn consumed_stops_routing_and_stays() {
        let lower_log = Rc::new(RefCell::new(Vec::new()));
        let upper_log = Rc::new(RefCell::new(Vec::new()));

        let mut stack = CaptureStack::new();
        stack.push(Box::new(Recorder::new(lower_log.clone(), || {
            CaptureFlow::Consumed
        })));
        stack.push(Box::new(Recorder::new(upper_log.clone(), || {
            CaptureFlow::Consumed
        })));

        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = Vec::new();
        let mut ev = key_event(Key::Esc);

        let consumed = {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            stack.dispatch(&mut ev, &mut ctx)
        };
        for effect in deferred.drain(..) {
            if let crate::view::Deferred::PushCapture(h) = effect {
                stack.push(h);
            }
        }

        assert!(consumed);
        assert_eq!(upper_log.borrow().len(), 1, "upper consumed it");
        assert_eq!(
            lower_log.borrow().len(),
            0,
            "lower never saw it (routing stopped)"
        );
        assert_eq!(stack.len(), 2, "Consumed keeps the handler on the stack");
    }

    #[test]
    fn consumed_pop_removes_handler() {
        let log = Rc::new(RefCell::new(Vec::new()));

        let mut stack = CaptureStack::new();
        stack.push(Box::new(Recorder::new(log.clone(), || {
            CaptureFlow::ConsumedPop
        })));
        assert_eq!(stack.len(), 1);

        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = Vec::new();

        // First event: consumed-and-popped.
        let mut ev1 = key_event(Key::Enter);
        let consumed1 = {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            stack.dispatch(&mut ev1, &mut ctx)
        };
        for effect in deferred.drain(..) {
            if let crate::view::Deferred::PushCapture(h) = effect {
                stack.push(h);
            }
        }
        assert!(consumed1);
        assert_eq!(stack.len(), 0, "ConsumedPop removed the handler");
        assert_eq!(log.borrow().len(), 1);

        // Second event: the popped handler must not see it (stack empty -> false).
        let mut ev2 = key_event(Key::Esc);
        let consumed2 = {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            stack.dispatch(&mut ev2, &mut ctx)
        };
        for effect in deferred.drain(..) {
            if let crate::view::Deferred::PushCapture(h) = effect {
                stack.push(h);
            }
        }
        assert!(!consumed2, "no handler left to consume");
        assert_eq!(
            log.borrow().len(),
            1,
            "popped handler did not see the later event"
        );
    }

    // -- the full compose test ----------------------------------------------

    #[test]
    fn compose_full_protocol() {
        // Loop-owned state as locals, exactly as the real loop (row 31) will hold it.
        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = Vec::new();

        let pushed_log = Rc::new(RefCell::new(Vec::new()));

        let mut stack = CaptureStack::new();
        // Bottom of the stack: a Pusher that defers a nested handler then consumes.
        stack.push(Box::new(Pusher {
            pushed_log: pushed_log.clone(),
            pushed: false,
        }));

        // -- Event 1: drives the Pusher. ------------------------------------
        // `Pusher::handle` itself posts/broadcasts/schedules a timer and pushes a
        // nested capture during dispatch (the `ctx.*` handler contract); we assert
        // those side effects landed in the loop-owned state afterward.
        let mut ev1 = key_event(Key::Char('a'));
        let consumed1 = {
            let mut ctx = Context::new(&mut out, &mut timers, 1_000, &mut deferred);
            assert_eq!(ctx.now_ms(), 1_000);
            stack.dispatch(&mut ev1, &mut ctx)
        };
        // The deferred push is still in `deferred` and has NOT been applied yet.
        assert_eq!(deferred.len(), 1, "push_capture deferred the handler");
        assert_eq!(
            pushed_log.borrow().len(),
            0,
            "pushed handler must NOT see the current event"
        );
        assert!(consumed1, "Pusher consumed event 1");

        // The loop applies deferred pushes AFTER dispatch.
        for effect in deferred.drain(..) {
            if let crate::view::Deferred::PushCapture(h) = effect {
                stack.push(h);
            }
        }
        assert_eq!(stack.len(), 2, "nested handler now on the stack");

        // post / broadcast landed in out_events.
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], Event::Command(Command::OK));
        assert_eq!(
            out[1],
            Event::Broadcast {
                command: Command::COMMAND_SET_CHANGED,
                source: None
            }
        );
        // set_timer registered in the queue.
        assert_eq!(timers.len(), 1);

        // -- Event 2: the nested handler (top of stack) now sees it. --------
        let mut ev2 = key_event(Key::Char('b'));
        let consumed2 = {
            let mut ctx = Context::new(&mut out, &mut timers, 1_050, &mut deferred);
            stack.dispatch(&mut ev2, &mut ctx)
        };
        for effect in deferred.drain(..) {
            if let crate::view::Deferred::PushCapture(h) = effect {
                stack.push(h);
            }
        }
        assert!(consumed2);
        assert_eq!(
            pushed_log.borrow().len(),
            1,
            "pushed handler saw the NEXT event after the deferred push was applied"
        );
        assert_eq!(pushed_log.borrow()[0], key_event(Key::Char('b')));
    }
}
