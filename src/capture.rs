//! The capture stack — the single mechanism behind modality, mouse tracking,
//! window drag/resize, and press-and-hold.
//!
//! A **LIFO stack of capture handlers** see each event *before* normal view-tree
//! routing and may consume it or pass it through. Modality, drag, and
//! press-tracking are all handlers rather than nested loops: a modal handler that
//! consumes every otherwise-unhandled event *is* the modal loop. Handlers hold a
//! [`ViewId`], never a view reference. The live event loop in
//! [`Program`](crate::app::Program) owns this stack and drives
//! [`CaptureStack::dispatch`]; the [`tests`] module here hand-plays the loop to
//! prove the protocol composes.
//!
//! **Guide:** [Event capture](../../../port/capture.html).
//!
//! # Turbo Vision heritage
//! Turbo Vision implements modality, view dragging, and a pressed button's
//! mouse-tracking by spinning *nested* blocking event loops (`tgroup.cpp`,
//! `tview.cpp`). Rust cannot nest a blocking loop that re-borrows the view tree,
//! so tvision-rs collapses all of them into one non-recursive event loop plus this LIFO
//! capture stack (deviation D9). Handlers hold a [`ViewId`] rather than a view
//! pointer (deviation D3).

use crate::event::Event;
use crate::view::{Context, Point, Rect, ViewId};

/// What a capture handler did with an event it was offered.
///
/// The return value is **authoritative** for routing — handlers must *not* rely
/// on [`Event::clear`] to signal "consumed" to the capture stack (clearing is a
/// separate downstream concern handled by normal view routing).
///
/// # Turbo Vision heritage
/// Makes explicit the consumed-vs-passed convention that Turbo Vision encodes
/// implicitly by clearing the event in place (`tview.cpp`); the self-removing
/// [`ConsumedPop`](CaptureFlow::ConsumedPop) variant replaces a nested modal
/// loop returning from its `execute` (deviation D9).
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

/// A capture handler — a participant in the capture stack that stands in for a
/// nested modal/drag/press loop.
///
/// Handlers are offered each event before normal view-tree routing. Identity is
/// a [`ViewId`]: a handler never holds a view reference.
///
/// # Turbo Vision heritage
/// Replaces the nested blocking loops Turbo Vision spins for modality, view
/// dragging, and a held button's mouse-tracking (`tgroup.cpp`, `tview.cpp`),
/// folding them into one event loop plus stacked handlers (deviation D9). Each
/// handler holds a [`ViewId`] instead of a view pointer (deviation D3).
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

    /// Returns the help context of the currently highlighted menu item, if this
    /// handler is a [`MenuSession`](crate::menu::MenuSession).
    ///
    /// Used by the status-line refresh to surface per-item help context while a
    /// menu is open. **Default is `None`** — only `MenuSession` overrides this.
    fn menu_help_ctx(&self) -> Option<crate::help::HelpCtx> {
        None
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

// ---------------------------------------------------------------------------
// MouseTrackCapture — the mouse hold-tracking seam
// ---------------------------------------------------------------------------

/// Which mouse event classes the hold-tracker forwards to its view while a
/// button is held (everything else is discarded until the terminating
/// [`MouseUp`](crate::event::Event::MouseUp)).
///
/// Different widgets opt into different subsets: a button or list viewer wants
/// only moves; a scrollbar, editor, or menu wants moves plus auto-repeat; a
/// frame wants every mouse class including the wheel. This struct-of-bools
/// names each forwarded class.
///
/// # Turbo Vision heritage
/// The idiomatic form of the `evMouse*` bit-mask `mouseEvent` accepted in
/// `tview.cpp`: `mouse_move` ↔ `evMouseMove`, `mouse_auto` ↔ `evMouseAuto`,
/// `wheel` ↔ the `evMouseWheel` slice of `evMouse`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrackMask {
    /// Forward [`MouseMove`](crate::event::Event::MouseMove) to the tracked view.
    pub mouse_move: bool,
    /// Forward [`MouseAuto`](crate::event::Event::MouseAuto) (auto-repeat while held).
    pub mouse_auto: bool,
    /// Forward [`MouseWheel`](crate::event::Event::MouseWheel) events.
    pub wheel: bool,
}

/// The hold-tracking router: while a mouse button is held, it localizes and
/// forwards masked mouse events to the tracked view, swallows everything else,
/// and pops on [`MouseUp`](crate::event::Event::MouseUp) (forwarding the
/// localized up too — a button/frame reads the up position after the hold ends).
///
/// **A pure router, not a strategy.** The per-event handling logic stays in the
/// widgets (their `MouseMove`/`MouseAuto` arms run while held, their `MouseUp`
/// arm runs once at release): captures are `'static` and hold no view borrow,
/// and several tracked views ([`ListViewer`](crate::widgets::ListViewer),
/// [`Outline`](crate::widgets::Outline)) are trait objects the pump could not
/// downcast — so the capture only routes, via
/// [`Deferred::MouseTrack`](crate::view::Deferred), which the pump applies by
/// calling the view's `handle_event` directly. Pushed via
/// [`Context::start_mouse_track`]; because a capture push is itself applied
/// through the deferred channel, the capture sees the **next** event — so the
/// widget runs its handling once on the press before the first forwarded event.
///
/// `origin` is the absolute screen position of view-local `(0, 0)`, cached by
/// the widget's last `draw` at push time (the `Button::abs_origin` pattern).
/// Like the window drag capture, the
/// origin is fixed for the duration of the hold: if the tracked view is moved /
/// resized mid-hold the localization goes stale — acceptable, since a hold is
/// short-lived and nothing moves the view while the (modal) hold swallows all
/// other input.
///
/// # Turbo Vision heritage
/// Replaces the blocking mouse-hold loop `do { … } while (mouseEvent(event,
/// mask))` (`tview.cpp:636-643`; the cluster/frame post-loop reads at
/// `tcluster.cpp:181-184` / `tframe.cpp:159-160`) with a capture handler
/// (deviation D9) that routes via the deferred channel instead of holding a view
/// borrow (deviation D3). The `do{}while` running its body once before the first
/// wait is why the capture is applied deferred (sees the next event).
pub struct MouseTrackCapture {
    /// The view being tracked (identity only, per the capture contract).
    view: ViewId,
    /// Absolute screen position of view-local `(0, 0)` at push time.
    origin: Point,
    /// Which event classes to forward (the tracked-event mask).
    mask: TrackMask,
}

impl MouseTrackCapture {
    /// Build a tracker for `view`. Constructed only by
    /// [`Context::start_mouse_track`] (widgets never build one directly).
    pub(crate) fn new(view: ViewId, origin: Point, mask: TrackMask) -> Self {
        MouseTrackCapture { view, origin, mask }
    }
}

impl CaptureHandler for MouseTrackCapture {
    fn handle(&mut self, ev: &mut Event, ctx: &mut Context) -> CaptureFlow {
        /// Localize an absolute-position mouse record against the push-time origin.
        fn localize(m: &crate::event::MouseEvent, origin: Point) -> crate::event::MouseEvent {
            let mut local = *m;
            local.position -= origin;
            local
        }
        match ev {
            Event::MouseMove(m) if self.mask.mouse_move => {
                ctx.request_mouse_track(self.view, Event::MouseMove(localize(m, self.origin)));
                CaptureFlow::Consumed
            }
            Event::MouseAuto(m) if self.mask.mouse_auto => {
                ctx.request_mouse_track(self.view, Event::MouseAuto(localize(m, self.origin)));
                CaptureFlow::Consumed
            }
            // Mouse-wheel events (see `crossterm_backend`) — the wheel slice of
            // the tracked-event mask (the frame's hold loop).
            Event::MouseWheel(m) if self.mask.wheel => {
                ctx.request_mouse_track(self.view, Event::MouseWheel(localize(m, self.origin)));
                CaptureFlow::Consumed
            }
            // A mouse-up always ends the hold (it is implicitly always tracked):
            // forward the localized up — cluster/frame read its position
            // post-loop — and pop this handler.
            Event::MouseUp(m) => {
                ctx.request_mouse_track(self.view, Event::MouseUp(localize(m, self.origin)));
                CaptureFlow::ConsumedPop
            }
            // Broadcasts pass THROUGH to normal routing (like `ModalFrame`).
            // A broadcast is a synchronous notification, not a queued input
            // event — the original hold loop only ever consumed queued input, so
            // it never saw a broadcast to discard. tvision-rs delivers that notification
            // as a queued `Event::Broadcast`, so to stay faithful the hold must let
            // it pass — otherwise a scrollbar-changed broadcast emitted by the very
            // bar being dragged (its own value update → redraw) is swallowed and
            // the editor/scroller never scrolls. (The bug this fixes: dragging a
            // scrollbar did not move the associated text.)
            Event::Broadcast { .. } => CaptureFlow::Pass,
            // Everything else (unmasked mouse classes, keys, commands, timers) is
            // discarded until the mouse-up — the hold is modal, and idle/timer
            // work does not run inside it.
            _ => CaptureFlow::Consumed,
        }
    }

    fn view(&self) -> Option<ViewId> {
        Some(self.view)
    }
}

/// A LIFO stack of [`CaptureHandler`]s.
///
/// The most-recently pushed handler is offered events first. The live event loop
/// in [`Program`](crate::app::Program) owns this stack and drives
/// [`dispatch`](Self::dispatch); a handler that wants to push a nested capture
/// does so through [`Context::push_capture`], whose deferred queue the loop
/// applies *after* dispatch — so the stack is never aliased while a handler runs.
///
/// # Turbo Vision heritage
/// Has no direct analogue as a data structure: it replaces the nesting of
/// blocking event loops Turbo Vision uses for modality, drag, and press-tracking
/// with one non-recursive loop plus this stack (deviation D9).
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

    /// Returns the help context from an active [`MenuSession`] on the stack, if
    /// any. Consults **only the topmost handler**: a menu preempts the help
    /// context iff it is the topmost capture handler, matching C++
    /// `TopView()->getHelpCtx()` where `TopView()` is the active `TMenuView`.
    /// Checking only the top handler avoids a scan that would let a menu sitting
    /// *below* a dialog wrongly win.
    ///
    /// Used by the status-line refresh to surface per-item help while a menu is
    /// open, without disturbing the existing `top_modal_view` path.
    pub fn active_menu_help_ctx(&self) -> Option<crate::help::HelpCtx> {
        self.handlers.last().and_then(|h| h.menu_help_ctx())
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
            self.log.borrow_mut().push(ev.clone());
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
        // Loop-owned state as locals, exactly as the real event loop holds it.
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

    // -- MouseTrackCapture (the hold-tracking router) ----------------------

    use crate::event::{MouseButtons, MouseEvent, MouseWheel};
    use crate::view::Deferred;

    /// Origin used by all router tests: view-local (0,0) sits at abs (5,3).
    const ORIGIN: Point = Point::new(5, 3);

    fn track_stack(mask: TrackMask) -> (CaptureStack, ViewId) {
        let id = ViewId::next();
        let mut stack = CaptureStack::new();
        stack.push(Box::new(MouseTrackCapture::new(id, ORIGIN, mask)));
        (stack, id)
    }

    fn mouse_record(x: i32, y: i32) -> MouseEvent {
        MouseEvent {
            position: Point::new(x, y),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Dispatch one event through the stack, returning (consumed, deferred).
    fn play(stack: &mut CaptureStack, mut ev: Event) -> (bool, Vec<crate::view::Deferred>) {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = Vec::new();
        let consumed = {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            stack.dispatch(&mut ev, &mut ctx)
        };
        (consumed, deferred)
    }

    /// A masked `MouseMove` is forwarded as a **localized** `Deferred::MouseTrack`
    /// payload; the handler stays on the stack (`Consumed`).
    #[test]
    fn track_masked_move_forwards_localized() {
        let (mut stack, id) = track_stack(TrackMask {
            mouse_move: true,
            ..Default::default()
        });
        let (consumed, deferred) = play(&mut stack, Event::MouseMove(mouse_record(8, 4)));
        assert!(consumed);
        assert_eq!(stack.len(), 1, "Consumed keeps the tracker on the stack");
        assert_eq!(deferred.len(), 1);
        match &deferred[0] {
            Deferred::MouseTrack {
                view,
                event: Event::MouseMove(m),
            } => {
                assert_eq!(*view, id);
                assert_eq!(m.position, Point::new(3, 1), "abs (8,4) − origin (5,3)");
            }
            _ => panic!("expected a localized MouseTrack MouseMove"),
        }
    }

    /// A masked `MouseAuto` forwards localized, like the move.
    #[test]
    fn track_masked_auto_forwards_localized() {
        let (mut stack, id) = track_stack(TrackMask {
            mouse_auto: true,
            ..Default::default()
        });
        let (consumed, deferred) = play(&mut stack, Event::MouseAuto(mouse_record(6, 3)));
        assert!(consumed);
        assert_eq!(deferred.len(), 1);
        match &deferred[0] {
            Deferred::MouseTrack {
                view,
                event: Event::MouseAuto(m),
            } => {
                assert_eq!(*view, id);
                assert_eq!(m.position, Point::new(1, 0));
            }
            _ => panic!("expected a localized MouseTrack MouseAuto"),
        }
    }

    /// An UNmasked mouse class is swallowed without forwarding (the hold is
    /// modal — only masked classes and the terminating up reach the view).
    #[test]
    fn track_unmasked_classes_are_swallowed() {
        let (mut stack, _id) = track_stack(TrackMask {
            mouse_move: true,
            ..Default::default()
        });
        // MouseAuto not in the mask: consumed, nothing forwarded.
        let (consumed, deferred) = play(&mut stack, Event::MouseAuto(mouse_record(8, 4)));
        assert!(consumed, "unmasked auto is still consumed (modal hold)");
        assert!(deferred.is_empty(), "…but not forwarded");
        assert_eq!(stack.len(), 1);
    }

    /// An `evMouseWheel` event forwards only under `mask.wheel`; a real-button
    /// `MouseDown` is swallowed regardless.
    #[test]
    fn track_wheel_event_respects_wheel_mask() {
        // wheel-masked: forwarded localized.
        let (mut stack, id) = track_stack(TrackMask {
            wheel: true,
            ..Default::default()
        });
        let wheel_down = Event::MouseWheel(MouseEvent {
            position: Point::new(7, 5),
            wheel: MouseWheel::Up,
            ..Default::default()
        });
        let (consumed, deferred) = play(&mut stack, wheel_down);
        assert!(consumed);
        assert_eq!(deferred.len(), 1);
        match &deferred[0] {
            Deferred::MouseTrack {
                view,
                event: Event::MouseWheel(m),
            } => {
                assert_eq!(*view, id);
                assert_eq!(m.position, Point::new(2, 2));
                assert_eq!(m.wheel, MouseWheel::Up);
            }
            _ => panic!("expected a localized MouseTrack wheel event"),
        }

        // A real-button down is swallowed even with mask.wheel.
        let (consumed, deferred) = play(&mut stack, Event::MouseDown(mouse_record(7, 5)));
        assert!(consumed);
        assert!(deferred.is_empty(), "buttoned down is not a wheel event");

        // Without mask.wheel the wheel is swallowed too.
        let (mut stack, _id) = track_stack(TrackMask::default());
        let wheel_down = Event::MouseWheel(MouseEvent {
            wheel: MouseWheel::Down,
            ..Default::default()
        });
        let (consumed, deferred) = play(&mut stack, wheel_down);
        assert!(consumed);
        assert!(deferred.is_empty());
    }

    /// A `Broadcast` during the hold PASSES THROUGH (not consumed) so normal
    /// routing delivers it to the tree. A broadcast is a synchronous notification,
    /// not queued input, so the hold must not swallow it. This is what lets a
    /// scrollbar being dragged notify the editor/scroller (its value-changed
    /// broadcast) so the text scrolls.
    #[test]
    fn track_broadcast_passes_through() {
        let (mut stack, _id) = track_stack(TrackMask {
            mouse_move: true,
            mouse_auto: true,
            wheel: true,
        });
        let bcast = Event::Broadcast {
            command: Command::COMMAND_SET_CHANGED,
            source: None,
        };
        let (consumed, deferred) = play(&mut stack, bcast);
        assert!(
            !consumed,
            "broadcast passes through the hold (C++ message() bypasses the loop)"
        );
        assert!(
            deferred.is_empty(),
            "the tracker forwards nothing for a broadcast"
        );
        assert_eq!(stack.len(), 1, "tracker stays on the stack until MouseUp");
    }

    /// A `KeyDown` during the hold vanishes: consumed, nothing forwarded
    /// (everything outside `mask | evMouseUp` is discarded, tview.cpp:636).
    #[test]
    fn track_key_down_vanishes() {
        let (mut stack, _id) = track_stack(TrackMask {
            mouse_move: true,
            mouse_auto: true,
            wheel: true,
        });
        let (consumed, deferred) = play(&mut stack, key_event(Key::Char('x')));
        assert!(consumed, "key is swallowed (hold is modal)");
        assert!(deferred.is_empty(), "nothing forwarded for a key");
        assert_eq!(stack.len(), 1, "tracker stays until MouseUp");
    }

    /// `MouseUp` pops the tracker AND forwards the localized up (cluster/frame
    /// read the up position post-loop) — regardless of the mask.
    #[test]
    fn track_mouse_up_pops_and_forwards() {
        let (mut stack, id) = track_stack(TrackMask::default());
        let up = Event::MouseUp(MouseEvent {
            position: Point::new(9, 4),
            ..Default::default()
        });
        let (consumed, deferred) = play(&mut stack, up);
        assert!(consumed);
        assert_eq!(stack.len(), 0, "MouseUp pops the tracker");
        assert_eq!(deferred.len(), 1);
        match &deferred[0] {
            Deferred::MouseTrack {
                view,
                event: Event::MouseUp(m),
            } => {
                assert_eq!(*view, id);
                assert_eq!(m.position, Point::new(4, 1), "localized up position");
            }
            _ => panic!("expected a localized MouseTrack MouseUp"),
        }
    }
}
