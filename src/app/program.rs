//! `TProgram` — the live event loop (row 31, FOUNDATION, deviation **D9**).
//!
//! `TProgram` (`tprogram.cpp`) is TV's application root: it owns the single event
//! loop, the desktop / status-line / menu-bar subviews, the timer queue, and the
//! capture stack. This is the keystone that makes the row-20 [`TimerQueue`] and
//! the row-21 [`CaptureStack`] *live*.
//!
//! ## Deviations realized here
//!
//! * **D9 — one loop, no nested modal loops.** C++ `TGroup::execute` spins a
//!   blocking `getEvent`/`handleEvent` loop; modality nests another. Here
//!   [`Program::run`] is the *only* loop ([`Program::pump_once`] is one
//!   iteration), and modality is a [`ModalFrame`] on the [`CaptureStack`] — "a
//!   handler that consumes every otherwise-unhandled event *is* the modal loop".
//!   The deferred-capture handshake is exactly the [`compose_full_protocol`]
//!   blueprint from `capture.rs`, now driven by the real pump.
//!
//! * **D4 — events carry no payload.** `cmTimerExpired` dropped its `TTimerId`
//!   `infoPtr` and `cmSelectWindowNum` dropped its window number — broadcasts
//!   carry only the [`Command`]. See the timer-payload and Alt-N breadcrumbs.
//!
//! * **D8 — whole-tree redraw + diff every pass.** No damage tracking, no
//!   `sfExposed`; `setScreenMode`/`cmScreenChanged` collapse into the resize
//!   check at the top of `pump_once` (the backend reports terminal size live).
//!
//! * **D11 — injected [`Clock`] + [`Backend`].** Headless never blocks, so tests
//!   drive `pump_once` synchronously with a [`ManualClock`](crate::timer::ManualClock).
//!
//! * **D1 — string commands, no ">255 always enabled".** The enabled set
//!   (`curCommandSet`) is seeded explicitly (see [`default_command_set`]).
//!
//! ## Deferred to later rows (grep-able breadcrumbs)
//!
//! * `exec_view` / `executeDialog` / `getData` / `setData` (the blocking modal
//!   wrapper + data marshalling) → **row 34 (`TDialog`)**, built on top of the
//!   [`ModalFrame`] mechanism proven here. The sync-vs-event-driven return is
//!   decided there.
//! * Alt-1..9 window selection → **row 33+** (needs numbered windows + a payload
//!   story, D4 dropped `infoPtr`).
//! * Status-line / menu-bar real subviews + the `getEvent` status-line
//!   pre-handling + `statusLine->update()` → **Phase 4** (factories return `None`
//!   for now).
//! * Timer-id payload (which timer fired) → revisit when a widget needs it (D4
//!   dropped the `infoPtr` that carried it; several designs are possible — do not
//!   invent one now).

use std::collections::VecDeque;
use std::time::Duration;

use crate::backend::{Backend, Renderer};
use crate::capture::{CaptureFlow, CaptureHandler, CaptureStack};
use crate::command::{Command, CommandSet};
use crate::event::Event;
use crate::theme::Theme;
use crate::timer::Clock;
use crate::timer::TimerQueue;
use crate::view::{Context, DrawCtx, Group, Rect, SelectMode, View, ViewId};

/// The frame-tick timeout: ports `TProgram::eventTimeoutMs` (20 ms → 50 wakeups
/// per second). Headless ignores it (D11).
const EVENT_TIMEOUT_MS: u64 = 20;

/// The default-enabled command vocabulary — ports the initial `curCommandSet`
/// (`tview.cpp` static init: everything *except* the window-management commands
/// that start disabled, `cmZoom`/`cmClose`/`cmResize`/`cmNext`/`cmPrev`).
///
/// The C++ "all commands enabled, then disable a few" cannot be expressed over an
/// open string-command space (D1), so we enumerate the enabled set explicitly:
/// the framework's shared vocabulary minus the disabled-at-startup window
/// commands. Apps/widgets toggle it via [`Program::enable_command`] /
/// [`Program::disable_command`].
fn default_command_set() -> CommandSet {
    let mut cs = CommandSet::new();
    // Core / dialog-result / editing / window-management / app-menu commands
    // that are enabled by default (cmZoom/cmClose/cmResize/cmNext/cmPrev are
    // deliberately omitted — they start disabled in C++ until a window grants
    // them).
    for cmd in [
        Command::VALID,
        Command::QUIT,
        Command::ERROR,
        Command::MENU,
        Command::HELP,
        Command::OK,
        Command::CANCEL,
        Command::YES,
        Command::NO,
        Command::DEFAULT,
        Command::CUT,
        Command::COPY,
        Command::PASTE,
        Command::UNDO,
        Command::CLEAR,
        Command::TILE,
        Command::CASCADE,
        Command::NEW,
        Command::OPEN,
        Command::SAVE,
        Command::SAVE_AS,
        Command::SAVE_ALL,
        Command::CH_DIR,
        Command::DOS_SHELL,
        Command::CLOSE_ALL,
    ] {
        cs.enable_cmd(cmd);
    }
    cs
}

// ---------------------------------------------------------------------------
// ModalFrame — the D9 modality mechanism (a capture handler)
// ---------------------------------------------------------------------------

/// A capture handler that realizes modality (D9): while it is on the
/// [`CaptureStack`], keyboard and command events
/// [`Pass`](CaptureFlow::Pass) through to normal routing and reach the modal
/// view via focus; broadcast events also [`Pass`](CaptureFlow::Pass) and fan
/// out to **all** views by design; positional (mouse) events are gated by
/// `bounds` — inside → [`Pass`](CaptureFlow::Pass), outside →
/// [`Consumed`](CaptureFlow::Consumed) (swallowed). This is the "a handler that
/// consumes every otherwise-unhandled event *is* the modal loop" realization.
///
/// It holds the modal view's [`ViewId`] (identity, per the capture contract) and
/// its `bounds` in the root group's frame (so positional events can be hit-tested
/// without a view reference — D3). For row 31 the root group covers the whole
/// screen at `(0,0)`, so group-local == absolute == this `bounds` frame.
///
/// **Popping is row 34's job, not this row's.** [`CaptureStack`] (row 21) has no
/// pop API — a handler removes itself only by returning
/// [`CaptureFlow::ConsumedPop`]. `exec_view` / `executeDialog` (row 34, TDialog)
/// is the blocking wrapper that pushes this frame, runs the pump until
/// [`Program::end_modal`] sets the end state, then pops it and marshals dialog
/// data. This row only proves the *gating* mechanism with a synthetic modal view
/// (see `modal_frame_gates_events`); the frame stays on the stack after
/// `end_modal` here.
pub struct ModalFrame {
    id: ViewId,
    bounds: Rect,
}

impl ModalFrame {
    /// Create a modal frame for the view `id` occupying `bounds` (in the root
    /// group's coordinate frame).
    pub fn new(id: ViewId, bounds: Rect) -> Self {
        ModalFrame { id, bounds }
    }
}

impl CaptureHandler for ModalFrame {
    fn handle(&mut self, ev: &mut Event, _ctx: &mut Context) -> CaptureFlow {
        match ev {
            // Positional events: let them through only if they land on the modal
            // view's bounds; otherwise swallow them so views beneath the modal
            // never see them.
            Event::MouseDown(m) | Event::MouseUp(m) | Event::MouseMove(m) | Event::MouseAuto(m) => {
                if self.bounds.contains(m.position) {
                    CaptureFlow::Pass
                } else {
                    CaptureFlow::Consumed
                }
            }
            // Focused (keyboard/command) + broadcast events pass through to normal
            // routing, which reaches the modal view because the group focuses it.
            _ => CaptureFlow::Pass,
        }
    }

    fn view(&self) -> Option<ViewId> {
        Some(self.id)
    }
}

// ---------------------------------------------------------------------------
// Program — the application root + event loop
// ---------------------------------------------------------------------------

/// The application root and single event loop — `TProgram` (D2 embed-and-delegate
/// + D9 single loop).
///
/// `Program` is **not** a [`View`] (it is the root; nothing contains it). It
/// embeds a [`Group`] as its view container and adds the loop machinery: the
/// [`Renderer`], the live [`CaptureStack`] and [`TimerQueue`], the injected
/// [`Clock`], and the `curCommandSet`.
///
/// Construct with [`Program::new`] (backend-injected so headless tests drive it),
/// drive production with [`Program::run`], or step one iteration with
/// [`Program::pump_once`] in tests.
pub struct Program {
    /// The root container (holds desktop/status-line/menu-bar children).
    group: Group,
    /// Owns the back/front [`Buffer`](crate::screen::Buffer) pair + boxed backend.
    renderer: Renderer,
    /// Row 21 — now live: the LIFO capture stack.
    captures: CaptureStack,
    /// Row 20 — now live: the timer queue.
    timers: TimerQueue,
    /// Injected time source (D11).
    clock: Box<dyn Clock>,
    /// The active theme (the paint pass needs `&Theme` for `DrawCtx`).
    theme: Theme,
    /// Posted commands / broadcasts + queued timer-expiry broadcasts, drained
    /// before polling the backend. A distinct field so `Context` can borrow it
    /// disjointly (see the borrow-discipline note on `pump_once`).
    out_events: VecDeque<Event>,
    /// Deferred capture pushes, applied to `captures` *after* each dispatch.
    /// Distinct field for the same disjoint-borrow reason.
    pending_captures: Vec<Box<dyn CaptureHandler>>,
    /// The enabled-command set (`curCommandSet`); see [`default_command_set`].
    command_set: CommandSet,
    /// The inserted desktop child's id (`canMoveFocus` / Alt-N target).
    desktop: Option<ViewId>,
    /// `TGroup::endState` — `Some(cmd)` ends the (modal) loop.
    end_state: Option<Command>,
    /// `TProgram::commandSetChanged` — set on an enable/disable change, broadcast
    /// once on the next idle, then cleared.
    command_set_changed: bool,
}

impl Program {
    /// Construct the program. Ports `TProgram::TProgram` (factory-mixin
    /// deferral): the three subviews are built from injected factory closures over
    /// the full program extent; each factory owns its own shrinking (the real
    /// status-line/menu-bar are Phase 4, so they are stubbed `None` for now).
    ///
    /// Faithful ctor behavior:
    /// - Bounds = `(0, 0, w, h)` from `backend.size()`.
    /// - The group's state gets `active`/`selected`/`focused`/`modal` set
    ///   directly (C++ `state = sfVisible | sfSelected | sfFocused | sfModal |
    ///   sfExposed`; `sfExposed` dropped D8, `sfVisible` is the ctor default).
    /// - Insert desktop, status-line, menu-bar **in that order**.
    /// - The desktop is made `current` so focused events route into it (the
    ///   row-26 `insert` deliberately does not auto-select).
    pub fn new(
        backend: Box<dyn Backend>,
        clock: Box<dyn Clock>,
        theme: Theme,
        create_desktop: impl FnOnce(Rect) -> Option<Box<dyn View>>,
        create_status_line: impl FnOnce(Rect) -> Option<Box<dyn View>>,
        create_menu_bar: impl FnOnce(Rect) -> Option<Box<dyn View>>,
    ) -> Self {
        let (w, h) = backend.size();
        let extent = Rect::new(0, 0, w as i32, h as i32);

        let mut group = Group::new(extent);
        // C++ sets the bits directly here (not through the propagating set_state):
        // state = sfVisible | sfSelected | sfFocused | sfModal | sfExposed.
        // sfVisible is already the ctor default; sfExposed is dropped (D8).
        {
            let st = group.state_mut();
            st.state.active = true;
            st.state.selected = true;
            st.state.focused = true;
            st.state.modal = true;
        }

        let renderer = Renderer::new(backend);
        let mut out_events = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut pending_captures: Vec<Box<dyn CaptureHandler>> = Vec::new();

        // Insert the three subviews in C++ order: desktop, statusline, menubar.
        // Each factory receives the full extent and owns its own shrinking
        // (initDeskTop: r.a.y++; r.b.y--, etc.); for row 31 the factory does it.
        let mut desktop = None;
        if let Some(view) = create_desktop(extent) {
            desktop = Some(group.insert(view));
        }
        if let Some(view) = create_status_line(extent) {
            group.insert(view);
        }
        if let Some(view) = create_menu_bar(extent) {
            group.insert(view);
        }

        // Make the desktop current so focused (key/command) events route into it.
        // `Group::insert` (row 26) deliberately never auto-selects, so we drive it
        // here via a throwaway Context; the RECEIVED_FOCUS broadcast it queues
        // sits in `out_events` and is processed on the first pump.
        if let Some(id) = desktop {
            let now = clock.now_ms();
            let mut ctx = Context::new(&mut out_events, &mut timers, now, &mut pending_captures);
            group.set_current(Some(id), SelectMode::Normal, &mut ctx);
        }

        Program {
            group,
            renderer,
            captures: CaptureStack::new(),
            timers,
            clock,
            theme,
            out_events,
            pending_captures,
            command_set: default_command_set(),
            desktop,
            end_state: None,
            command_set_changed: false,
        }
    }

    /// The desktop child's id, if a desktop was created.
    pub fn desktop(&self) -> Option<ViewId> {
        self.desktop
    }

    /// `TProgram::endModal` — request the (modal) loop end with `cmd`. Ports
    /// `TGroup::endModal`: store the end state; [`run`](Self::run) returns it once
    /// the tree validates it.
    pub fn end_modal(&mut self, cmd: Command) {
        self.end_state = Some(cmd);
    }

    /// The current modal end state, if set (test/inspection hook).
    pub fn end_state(&self) -> Option<Command> {
        self.end_state
    }

    // -- command-enable policy (curCommandSet) ------------------------------

    /// Enable `cmd` (`TProgram`-side `enableCommand`). Sets the
    /// command-set-changed flag on a real change so the next idle broadcasts
    /// `cmCommandSetChanged`.
    pub fn enable_command(&mut self, cmd: Command) {
        if !self.command_set.has(cmd) {
            self.command_set.enable_cmd(cmd);
            self.command_set_changed = true;
        }
    }

    /// Disable `cmd` (`TProgram`-side `disableCommand`). Sets the
    /// command-set-changed flag on a real change.
    pub fn disable_command(&mut self, cmd: Command) {
        if self.command_set.has(cmd) {
            self.command_set.disable_cmd(cmd);
            self.command_set_changed = true;
        }
    }

    /// Whether `cmd` is currently enabled (`TProgram::commandEnabled`). The
    /// C++ ">255 always enabled" rule is **dropped** (D1).
    pub fn command_enabled(&self, cmd: Command) -> bool {
        self.command_set.has(cmd)
    }

    // -- the run loop --------------------------------------------------------

    /// `TProgram::run` → `TGroup::execute` — the production entry point.
    ///
    /// ```text
    /// loop {
    ///     end_state = None;
    ///     while end_state.is_none() { pump_once(); }
    ///     let es = end_state.unwrap();
    ///     if valid_end(es) { return es; }
    /// }
    /// ```
    ///
    /// With a production `SystemClock` + crossterm backend `poll_event` blocks, so
    /// this does not spin. **Do not call on a headless backend without a QUIT
    /// path** — headless never blocks, so it would busy-loop; tests step
    /// [`pump_once`](Self::pump_once) instead.
    pub fn run(&mut self) -> Command {
        loop {
            self.end_state = None;
            while self.end_state.is_none() {
                self.pump_once();
            }
            let es = self.end_state.unwrap();
            if self.valid_end(es) {
                return es;
            }
        }
    }

    /// `TGroup::execute`'s outer `while( !valid(endState) )` — a modal only ends
    /// if the tree validates the end command.
    fn valid_end(&self, cmd: Command) -> bool {
        self.group.valid(cmd)
    }

    /// One iteration of the loop — the heart of D9.
    ///
    /// Borrow discipline (the brief's #1 risk): `self` is destructured into field
    /// bindings at the top, so the disjoint fields backing [`Context`]
    /// (`out_events` / `timers` / `pending_captures`) can be borrowed alongside
    /// `group` / `captures`. The dispatch is a free function with explicit field
    /// borrows; there are no `&mut self` helpers with overlapping field sets.
    pub fn pump_once(&mut self) {
        let Program {
            group,
            renderer,
            captures,
            timers,
            clock,
            theme,
            out_events,
            pending_captures,
            command_set,
            desktop: _,
            end_state,
            command_set_changed,
        } = self;

        // 1. Resize check — the D9 realization of setScreenMode/cmScreenChanged.
        //    CrosstermBackend::size() queries the terminal live, so there is no
        //    Event::Resize variant (avoids enum churn).
        let (w, h) = renderer.backend().size();
        let cur = group.state().size;
        if cur.x != w as i32 || cur.y != h as i32 {
            renderer.resize(w, h);
            group.change_bounds(Rect::new(0, 0, w as i32, h as i32));
        }

        // 2. Sample the clock once for this pass.
        let now = clock.now_ms();

        // 3. Pick the next event: drain the internal queue first, else poll.
        let timeout = event_wait_timeout(timers, now);
        let ev = match out_events.pop_front() {
            Some(e) => Some(e),
            None => renderer.backend_mut().poll_event(timeout),
        };

        match ev {
            // 4. No event -> idle (ports TProgram::idle), then fall through to
            //    the redraw (do NOT early-return).
            None => {
                if *command_set_changed {
                    out_events.push_back(Event::Broadcast(Command::COMMAND_SET_CHANGED));
                    *command_set_changed = false;
                }
                // collectExpiredTimers: D4 drops the TimerId payload — broadcast
                // carries only the command.
                // TODO(timer payload): when a widget needs to know WHICH timer
                // fired, revisit the payload story (D4 dropped infoPtr; several
                // designs are possible — do not invent one now).
                for _id in timers.collect_expired(now) {
                    out_events.push_back(Event::Broadcast(Command::TIMER_EXPIRED));
                }
                // TODO(TStatusLine row): statusLine->update() is a no-op until the
                // status line lands (Phase 4).
            }
            // 5. Event present -> dispatch.
            Some(mut ev) => {
                // TODO(TStatusLine row): getEvent's status-line pre-handling — a
                // keydown, or a mousedown whose firstThat(viewHasMouse) is the
                // status line, is handed to the status line BEFORE normal routing.
                // No-op with the status line stubbed; realize it in Phase 4.

                // Command filtering at the program boundary (D1): drop a disabled
                // command before routing. Broadcasts/keys/mouse flow regardless.
                let drop_disabled = matches!(ev, Event::Command(c) if !command_set.has(c));
                if drop_disabled {
                    ev.clear();
                }

                if !ev.is_nothing() {
                    // The Context borrow ends at this block's close, before we
                    // drain pending_captures back into the stack.
                    {
                        let mut ctx = Context::new(out_events, timers, now, pending_captures);
                        // Offer to the capture stack first; if consumed, skip view
                        // routing.
                        let consumed = captures.dispatch(&mut ev, &mut ctx);
                        if !consumed {
                            program_handle_event(group, &mut ev, &mut ctx, end_state);
                        }
                    }
                    // Apply deferred capture pushes AFTER dispatch, so a pushed
                    // handler sees the NEXT event (the compose_full_protocol
                    // invariant, now through the real pump).
                    for h in pending_captures.drain(..) {
                        captures.push(h);
                    }
                }
            }
        }

        // 7. resetCursor, then redraw. Renderer::render reads self.cursor, so the
        //    cursor must be set BEFORE render.
        let group_origin = group.state().origin;
        let cursor = group
            .cursor_request()
            .map(|p| p + group_origin)
            .map(|p| (p.x.max(0) as u16, p.y.max(0) as u16));
        renderer.set_cursor(cursor);

        renderer.render(|buf| {
            let bounds = group.state().get_bounds();
            let mut dc = DrawCtx::new(buf, theme, bounds, bounds.a);
            group.draw(&mut dc);
        });
    }

    // -- test/inspection accessors ------------------------------------------

    /// Mutably borrow the embedded root group (test/inspection hook).
    #[cfg(test)]
    fn group_mut(&mut self) -> &mut Group {
        &mut self.group
    }

    /// The number of live capture handlers (test/inspection hook).
    #[cfg(test)]
    fn capture_len(&self) -> usize {
        self.captures.len()
    }

    /// Build a throwaway [`Context`] over the loop-owned state (test hook): used
    /// to drive group focus from tests, since the backing fields are private.
    #[cfg(test)]
    fn with_ctx<R>(&mut self, f: impl FnOnce(&mut Group, &mut Context) -> R) -> R {
        let now = self.clock.now_ms();
        let mut ctx = Context::new(
            &mut self.out_events,
            &mut self.timers,
            now,
            &mut self.pending_captures,
        );
        f(&mut self.group, &mut ctx)
    }
}

// ---------------------------------------------------------------------------
// program-level handle_event — ports TProgram::handleEvent (free fn, D9 borrows)
// ---------------------------------------------------------------------------

/// `TProgram::eventWaitTimeout` — `min(20 ms, time_until_next_timer)`. With no
/// timer it is just the 20 ms frame tick. Returned for `poll_event`; headless
/// ignores it and never blocks (D11). A free function (not a method) so it
/// composes with the pump's destructured borrows.
fn event_wait_timeout(timers: &TimerQueue, now: u64) -> Option<Duration> {
    let frame = Duration::from_millis(EVENT_TIMEOUT_MS);
    match timers.time_until_next(now) {
        Some(until) => Some(frame.min(until)),
        None => Some(frame),
    }
}

/// `TProgram::handleEvent` — the program's own event handling, then delegate to
/// the embedded group's three-phase router.
///
/// A free function taking explicit field borrows so it composes with the pump's
/// disjoint borrows (the brief's borrow discipline).
fn program_handle_event(
    group: &mut Group,
    ev: &mut Event,
    ctx: &mut Context,
    end_state: &mut Option<Command>,
) {
    // TODO(row 33+): Alt-1..9 window select; needs numbered windows + a payload
    // story (D4 dropped infoPtr that carried the window number). Stubbed here so
    // no half path exists.

    group.handle_event(ev, ctx);

    // C++: endModal(cmQuit); clearEvent(event).
    if *ev == Event::Command(Command::QUIT) {
        *end_state = Some(Command::QUIT);
        ev.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, HeadlessHandle};
    use crate::color::{Color, Style};
    use crate::desktop::Desktop;
    use crate::event::{Event, Key, KeyEvent, KeyModifiers, MouseButtons, MouseEvent};
    use crate::theme::Theme;
    use crate::timer::ManualClock;
    use crate::view::{DrawCtx, Point, Rect, View, ViewState};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    // -- test harness --------------------------------------------------------

    /// A per-event action a [`Probe`] runs during dispatch (post / set a timer /
    /// push a capture).
    type ProbeAction = Box<dyn FnMut(&mut Context)>;

    /// A probe view: fills its extent with `ch`, records every event it is handed,
    /// and runs an optional per-event action (so a probe can post / set a timer /
    /// push a capture during dispatch). Consumes its trigger key but passes
    /// commands and broadcasts through (so the program's QUIT round-trip and
    /// broadcast fan-out are observable).
    struct Probe {
        st: ViewState,
        ch: char,
        log: Rc<RefCell<Vec<Event>>>,
        action: Option<ProbeAction>,
    }

    impl Probe {
        fn new(bounds: Rect, ch: char, log: Rc<RefCell<Vec<Event>>>) -> Self {
            let mut st = ViewState::new(bounds);
            st.options.selectable = true;
            Probe {
                st,
                ch,
                log,
                action: None,
            }
        }
    }

    impl View for Probe {
        fn state(&self) -> &ViewState {
            &self.st
        }
        fn state_mut(&mut self) -> &mut ViewState {
            &mut self.st
        }
        fn draw(&mut self, ctx: &mut DrawCtx) {
            let ext = self.st.get_extent();
            ctx.fill(ext, self.ch, Style::new(Color::Bios(0xF), Color::Bios(0x1)));
        }
        fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
            self.log.borrow_mut().push(*ev);
            if let Some(action) = self.action.as_mut() {
                action(ctx);
            }
            // Consume only key events; let commands and broadcasts flow so the
            // program-level handling (QUIT) and broadcast fan-out are observable.
            if matches!(ev, Event::KeyDown(_)) {
                ev.clear();
            }
        }
    }

    /// A capture handler that records every event it is offered and passes it on.
    struct RecordingCapture {
        log: Rc<RefCell<Vec<Event>>>,
    }
    impl CaptureHandler for RecordingCapture {
        fn handle(&mut self, ev: &mut Event, _ctx: &mut Context) -> CaptureFlow {
            self.log.borrow_mut().push(*ev);
            CaptureFlow::Pass
        }
    }

    fn key(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(k, KeyModifiers::default()))
    }

    fn mouse_down_at(x: i32, y: i32) -> Event {
        Event::MouseDown(MouseEvent {
            position: Point::new(x, y),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        })
    }

    /// Build a `Program` with a real desktop (a `Group` containing a `Background`)
    /// and stubbed status-line/menu-bar, over a headless backend + a shared
    /// `ManualClock` the test retains so it can advance time.
    fn program_with_desktop(w: u16, h: u16) -> (Program, HeadlessHandle, Rc<ManualClock>) {
        let (backend, handle) = HeadlessBackend::new(w, h);
        let theme = Theme::classic_blue();
        let clock = Rc::new(ManualClock::new(0));
        let mut program = Program::new(
            Box::new(backend),
            Box::new(clock.clone()),
            theme,
            // The desktop: a faithful `Desktop` (a `Group` owning a `Background`,
            // filled with the default ░ U+2591 light shade).
            |r| {
                Some(Box::new(Desktop::new(r, |r2| {
                    Some(Desktop::init_background(r2))
                })))
            },
            |_r| None, // status line stubbed (Phase 4)
            |_r| None, // menu bar stubbed (Phase 4)
        );
        // Drain the startup desktop-focus broadcast so tests start with a clean
        // queue (the RECEIVED_FOCUS that Program::new queues when it focuses the
        // desktop). Behaviorally it would be processed on the first pump; the
        // tests assert on their own injected events.
        program.out_events.clear();
        (program, handle, clock)
    }

    // -- 1. End-to-end loop snapshot (mandatory gate) ------------------------

    #[test]
    fn pump_renders_desktop_snapshot() {
        let (mut program, screen, _clock) = program_with_desktop(12, 4);
        program.pump_once();
        insta::assert_snapshot!(screen.snapshot());
    }

    // -- 2. Quit -------------------------------------------------------------

    #[test]
    fn quit_command_sets_end_state() {
        let (mut program, _screen, _clock) = program_with_desktop(12, 4);
        // Post QUIT into the queue (as a status-line / menu item would).
        program.out_events.push_back(Event::Command(Command::QUIT));
        assert_eq!(program.end_state(), None);
        // Pump: the queued QUIT re-enters as an event, routes, and the program's
        // handle_event sets end_state.
        program.pump_once();
        assert_eq!(
            program.end_state(),
            Some(Command::QUIT),
            "QUIT command ends the loop"
        );
    }

    // -- 3. Timer dispatch ---------------------------------------------------

    #[test]
    fn timer_expiry_broadcasts_and_routes() {
        let (mut program, _screen, clock) = program_with_desktop(12, 4);
        let log = Rc::new(RefCell::new(Vec::new()));

        // Insert a probe into the desktop group that records broadcasts and arms
        // a timer on its first event.
        let arming = Rc::new(RefCell::new(true));
        {
            let arming = arming.clone();
            let mut probe = Probe::new(Rect::new(0, 0, 4, 2), 'P', log.clone());
            probe.action = Some(Box::new(move |ctx: &mut Context| {
                if *arming.borrow() {
                    ctx.set_timer(Duration::from_millis(50), None);
                    *arming.borrow_mut() = false;
                }
            }));
            // Insert into the program's root group and make it current so it
            // receives focused events / broadcasts.
            let id = program.group_mut().insert(Box::new(probe));
            program.with_ctx(|g, ctx| g.set_current(Some(id), SelectMode::Normal, ctx));
        }

        // Arm the timer by sending a broadcast the probe records and reacts to.
        program.out_events.clear();
        program
            .out_events
            .push_back(Event::Broadcast(Command::SCROLL_BAR_CHANGED));
        program.pump_once(); // probe arms a 50ms timer at now=0
        assert_eq!(program.timers.len(), 1, "probe armed a timer");

        // Advance past expiry; an idle pump (no queued events, none polled)
        // collects the timer and queues a TIMER_EXPIRED broadcast.
        clock.advance(60);
        log.borrow_mut().clear();
        program.pump_once(); // idle: collect -> queue TIMER_EXPIRED
        assert!(
            program
                .out_events
                .iter()
                .any(|e| *e == Event::Broadcast(Command::TIMER_EXPIRED)),
            "expired timer queued a TIMER_EXPIRED broadcast"
        );

        // Next pump routes the queued broadcast; the probe records it.
        program.pump_once();
        assert!(
            log.borrow()
                .contains(&Event::Broadcast(Command::TIMER_EXPIRED)),
            "probe received cmTimerExpired"
        );
    }

    // -- 4. Capture stack live -----------------------------------------------

    #[test]
    fn pushed_capture_sees_next_event_not_current() {
        let (mut program, _screen, _clock) = program_with_desktop(12, 4);
        let cap_log = Rc::new(RefCell::new(Vec::new()));

        // A probe that pushes a recording capture on its first event.
        let pushed = Rc::new(RefCell::new(false));
        {
            let pushed = pushed.clone();
            let cap_log = cap_log.clone();
            let mut probe = Probe::new(
                Rect::new(0, 0, 4, 2),
                'P',
                Rc::new(RefCell::new(Vec::new())),
            );
            probe.action = Some(Box::new(move |ctx: &mut Context| {
                if !*pushed.borrow() {
                    ctx.push_capture(Box::new(RecordingCapture {
                        log: cap_log.clone(),
                    }));
                    *pushed.borrow_mut() = true;
                }
            }));
            let id = program.group_mut().insert(Box::new(probe));
            program.with_ctx(|g, ctx| g.set_current(Some(id), SelectMode::Normal, ctx));
        }

        // Event 1: the probe pushes a capture during dispatch.
        program.out_events.clear(); // drop the set_current focus broadcasts
        program.out_events.push_back(key(Key::Char('a')));
        assert_eq!(program.capture_len(), 0);
        program.pump_once();
        assert_eq!(
            program.capture_len(),
            1,
            "deferred push applied after dispatch"
        );
        assert!(
            cap_log.borrow().is_empty(),
            "pushed capture did NOT see the current event"
        );

        // Event 2: the now-live capture sees it first.
        program.out_events.push_back(key(Key::Char('b')));
        program.pump_once();
        assert_eq!(
            cap_log.borrow().len(),
            1,
            "pushed capture saw the next event"
        );
        assert_eq!(cap_log.borrow()[0], key(Key::Char('b')));
    }

    // -- 5. Modal frame ------------------------------------------------------

    #[test]
    fn modal_frame_gates_events() {
        let (mut program, _screen, _clock) = program_with_desktop(20, 10);
        let modal_log = Rc::new(RefCell::new(Vec::new()));
        let beneath_log = Rc::new(RefCell::new(Vec::new()));

        // A non-modal probe beneath (left half) and a modal probe (right half).
        let beneath_bounds = Rect::new(0, 0, 6, 6);
        let modal_bounds = Rect::new(10, 0, 16, 6);
        {
            let beneath = Probe::new(beneath_bounds, 'B', beneath_log.clone());
            program.group_mut().insert(Box::new(beneath));
        }
        let modal_id = {
            let modal = Probe::new(modal_bounds, 'M', modal_log.clone());
            program.group_mut().insert(Box::new(modal))
        };
        // Focus the modal view and push the modal frame.
        program.with_ctx(|g, ctx| g.set_current(Some(modal_id), SelectMode::Normal, ctx));
        program
            .captures
            .push(Box::new(ModalFrame::new(modal_id, modal_bounds)));

        // A click outside the modal view is swallowed: the beneath probe must NOT
        // see it.
        program.out_events.clear(); // drop the set_current focus broadcasts
        program.out_events.push_back(mouse_down_at(2, 2));
        program.pump_once();
        assert!(
            beneath_log.borrow().is_empty(),
            "modal swallows clicks aimed at views beneath it"
        );

        // A click on the modal view reaches it.
        program.out_events.push_back(mouse_down_at(12, 2));
        program.pump_once();
        assert_eq!(
            modal_log.borrow().len(),
            1,
            "modal view receives clicks aimed at it"
        );

        // end_modal surfaces the end state. NOTE: row 31 does NOT pop the frame
        // here — `CaptureStack` (row 21) has no pop API; a handler self-pops only
        // by returning `ConsumedPop` (proven generically by
        // `capture::tests::consumed_pop_removes_handler`). The blocking wrapper
        // that pushes the frame, runs the pump until end_modal, then pops it is
        // `exec_view`/`executeDialog` at **row 34** (TDialog), built on this frame.
        // So the frame is still on the stack after end_modal — the truthful state.
        assert_eq!(program.capture_len(), 1, "modal frame still present");
        program.end_modal(Command::OK);
        assert_eq!(program.end_state(), Some(Command::OK));
        assert_eq!(
            program.capture_len(),
            1,
            "row 31 does not pop the frame; exec_view (row 34) owns push+pop"
        );
    }

    // -- 6. resetCursor ------------------------------------------------------

    #[test]
    fn reset_cursor_places_absolute_focused_cursor() {
        let (mut program, screen, _clock) = program_with_desktop(20, 10);
        // A focused probe at origin (5, 3) with a visible cursor at local (2, 1)
        // -> absolute (7, 4).
        let id = {
            let mut probe = Probe::new(
                Rect::new(5, 3, 11, 9),
                'P',
                Rc::new(RefCell::new(Vec::new())),
            );
            probe.st.state.cursor_vis = true;
            probe.st.cursor = Point::new(2, 1);
            program.group_mut().insert(Box::new(probe))
        };
        // The group is focused (set in Program::new); make the probe current and
        // focused so the cursor walk reaches it.
        program.with_ctx(|g, ctx| g.set_current(Some(id), SelectMode::Normal, ctx));

        program.pump_once();
        assert_eq!(
            screen.cursor(),
            Some((7, 4)),
            "cursor placed at the focused child's absolute cursor"
        );

        // Hide the cursor -> the loop hides it.
        program.with_ctx(|g, _ctx| {
            let i = g.current().and_then(|id| g.index_of_pub(id)).unwrap();
            g.child_state_mut(i).state.cursor_vis = false;
        });
        program.pump_once();
        assert_eq!(screen.cursor(), None, "hidden cursor -> None");
    }

    // -- 7. Posted command re-entry ------------------------------------------

    #[test]
    fn posted_command_re_enters_as_event() {
        let (mut program, _screen, _clock) = program_with_desktop(12, 4);
        let log = Rc::new(RefCell::new(Vec::new()));

        // A probe that posts OK on its first key event.
        let posted = Rc::new(RefCell::new(false));
        {
            let posted = posted.clone();
            let mut probe = Probe::new(Rect::new(0, 0, 4, 2), 'P', log.clone());
            probe.action = Some(Box::new(move |ctx: &mut Context| {
                if !*posted.borrow() {
                    ctx.post(Command::OK);
                    *posted.borrow_mut() = true;
                }
            }));
            let id = program.group_mut().insert(Box::new(probe));
            program.with_ctx(|g, ctx| g.set_current(Some(id), SelectMode::Normal, ctx));
        }

        // Send a key: the probe posts OK during dispatch.
        program.out_events.clear(); // drop the set_current focus broadcasts
        program.out_events.push_back(key(Key::Char('x')));
        program.pump_once();
        assert!(
            program
                .out_events
                .iter()
                .any(|e| *e == Event::Command(Command::OK)),
            "posted command landed in out_events"
        );

        // Next pump routes the posted OK back as an Event::Command to the probe.
        log.borrow_mut().clear();
        program.pump_once();
        assert!(
            log.borrow().contains(&Event::Command(Command::OK)),
            "posted command re-entered as an Event::Command and routed"
        );
    }

    // -- 8. commandSetChanged idle broadcast ---------------------------------

    #[test]
    fn command_set_change_broadcasts_once_on_idle() {
        let (mut program, _screen, _clock) = program_with_desktop(12, 4);

        // Disable a command (cmClose starts disabled, so enable then disable to
        // force a real change either way; use a default-enabled command).
        assert!(program.command_enabled(Command::OK));
        program.disable_command(Command::OK);
        assert!(!program.command_enabled(Command::OK));

        // An idle pump (no queued/polled events) broadcasts COMMAND_SET_CHANGED
        // once and clears the flag.
        program.out_events.clear();
        program.pump_once();
        let count = program
            .out_events
            .iter()
            .filter(|e| **e == Event::Broadcast(Command::COMMAND_SET_CHANGED))
            .count();
        assert_eq!(count, 1, "command-set change broadcasts exactly once");

        // A second idle pump does NOT re-broadcast (flag cleared). Drain the queue
        // first so the previous broadcast does not linger.
        program.out_events.clear();
        program.pump_once();
        let count2 = program
            .out_events
            .iter()
            .filter(|e| **e == Event::Broadcast(Command::COMMAND_SET_CHANGED))
            .count();
        assert_eq!(count2, 0, "no re-broadcast after the flag is cleared");
    }
}
