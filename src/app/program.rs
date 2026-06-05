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
//! * **D4 — `Event::Broadcast` carries a `source: ViewId`** (the broadcast-subject
//!   successor to `infoPtr`); `Event::Command` carries only the [`Command`]. The
//!   *integer*-argument payloads are **not** served by `source` (they are not
//!   `ViewId`s) and have their own typed mechanisms: `cmTimerExpired`'s `TTimerId`
//!   is now carried by [`Event::Timer`], and `cmSelectWindowNum`'s window number
//!   has its own design (see the Alt-N breadcrumb).
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
//! * Alt-1..9 window selection (`cmSelectWindowNum`) → **realized at row 33d-2**
//!   in [`program_handle_event`] as a direct walk (the program asks the desktop to
//!   select the child whose [`number`](View::number) matches, gated by
//!   `canMoveFocus` == `deskTop.valid(cmReleasedFocus)`). Not a payload broadcast —
//!   the window number is an integer, not a `ViewId`.
//! * Status-line / menu-bar real subviews + the `getEvent` status-line
//!   pre-handling → **Phase 4, DONE.** A real bar/line are inserted by the factory
//!   closures (see `examples/hello.rs`), their ids are held
//!   ([`menu_bar`](Program::menu_bar) / [`status_line`](Program::status_line)) and
//!   seeded with the initial command-graying in [`Program::new`], and
//!   [`pump_once`](Program::pump_once) pre-routes keyDown / over-the-line mouseDown
//!   to the status line before normal dispatch. `statusLine->update()` is
//!   omit-until-consumer (see the breadcrumb in `pump_once`'s idle arm).
//! * Timer-id payload (which timer fired) → revisit when a widget needs it (D4
//!   dropped the `infoPtr` that carried it; several designs are possible — do not
//!   invent one now).

use std::collections::VecDeque;
use std::time::Duration;

use crate::backend::{Backend, Renderer};
use crate::capture::{CaptureFlow, CaptureHandler, CaptureStack};
use crate::command::{Command, CommandSet};
use crate::event::{Event, Key};
use crate::theme::Theme;
use crate::timer::Clock;
use crate::timer::TimerQueue;
use crate::view::{Context, Deferred, DrawCtx, Group, Point, Rect, SelectMode, View, ViewId};

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

    /// Follow the modal view when it is moved/resized (a dragged dialog). Without
    /// this the gate keeps the bounds captured at push time, so after a drag any
    /// positional event on the *moved* dialog that falls outside the stale bounds
    /// is swallowed — the dialog goes mouse-dead. The loop calls this from
    /// [`CaptureStack::sync_gate_bounds`] before every dispatch.
    fn set_gate_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
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
    /// Deferred effects on loop-owned state ([`Deferred`]), applied *after* each
    /// dispatch — capture pushes (→ `captures`), command enable/disable (→
    /// `command_set`), and tree mutations (bounds / state-flag / close → `group`).
    /// A downward-borrowed view / capture handler cannot touch the capture stack,
    /// the command set, or the tree inline (D3/D9), so it requests the effect via
    /// `Context` and the loop drains this one queue. A distinct field for the same
    /// disjoint-borrow reason as `out_events`. One channel — a new capability adds a
    /// [`Deferred`] variant, not a field.
    deferred: Vec<Deferred>,
    /// The enabled-command set (`curCommandSet`); see [`default_command_set`].
    command_set: CommandSet,
    /// The inserted desktop child's id (`canMoveFocus` / Alt-N target).
    desktop: Option<ViewId>,
    /// The inserted menu-bar child's id (`TProgram::menuBar`), if one was created.
    /// Held so the ctor can seed its initial command-graying and so future rows can
    /// route to it; the pump itself does not read it (see the `pump_once`
    /// destructure, where it is bound `_`).
    menu_bar: Option<ViewId>,
    /// The inserted status-line child's id (`TProgram::statusLine`), if one was
    /// created. The `getEvent` pre-routing in [`pump_once`](Self::pump_once) reads
    /// it to hand keyDown / over-the-line mouseDown events to the line first.
    status_line: Option<ViewId>,
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
        let mut deferred: Vec<Deferred> = Vec::new();

        // Insert the three subviews in C++ order: desktop, statusline, menubar.
        // Each factory receives the full extent and owns its own shrinking
        // (initDeskTop: r.a.y++; r.b.y--, etc.); for row 31 the factory does it.
        let mut desktop = None;
        let mut status_line = None;
        let mut menu_bar = None;
        if let Some(view) = create_desktop(extent) {
            desktop = Some(group.insert(view));
        }
        if let Some(view) = create_status_line(extent) {
            status_line = Some(group.insert(view));
        }
        if let Some(view) = create_menu_bar(extent) {
            menu_bar = Some(group.insert(view));
        }

        // INITIAL REGRAY (the carried gap). The menu bar / status line are born
        // all-enabled and only regray on a `cmCommandSetChanged` broadcast, which
        // does NOT fire at startup (it is queued only by an enable/disable change).
        // We hold `group` + the command set here, so seed each view's
        // command-graying cache directly via the established broker hook
        // (`View::update_menu_commands`) — no need to defer (the deferred queue is
        // not drained on the first idle pump anyway). C++ gets this for free because
        // `commandEnabled` is read live in `drawSelect`; our snapshot cache must be
        // primed once at construction.
        let command_set = default_command_set();
        for id in [menu_bar, status_line].into_iter().flatten() {
            if let Some(v) = group.find_mut(id) {
                v.update_menu_commands(&command_set);
            }
        }

        // Make the desktop current so focused (key/command) events route into it.
        // `Group::insert` (row 26) deliberately never auto-selects, so we drive it
        // here via a throwaway Context; the RECEIVED_FOCUS broadcast it queues
        // sits in `out_events` and is processed on the first pump.
        if let Some(id) = desktop {
            let now = clock.now_ms();
            let mut ctx = Context::new(&mut out_events, &mut timers, now, &mut deferred);
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
            deferred,
            command_set,
            desktop,
            menu_bar,
            status_line,
            end_state: None,
            command_set_changed: false,
        }
    }

    /// The desktop child's id, if a desktop was created.
    pub fn desktop(&self) -> Option<ViewId> {
        self.desktop
    }

    /// The menu-bar child's id, if a menu bar was created (`TProgram::menuBar`).
    pub fn menu_bar(&self) -> Option<ViewId> {
        self.menu_bar
    }

    /// The status-line child's id, if a status line was created
    /// (`TProgram::statusLine`).
    pub fn status_line(&self) -> Option<ViewId> {
        self.status_line
    }

    /// `TProgram::endModal` — request the (modal) loop end with `cmd`. Ports
    /// `TGroup::endModal`: store the end state; [`run`](Self::run) returns it once
    /// the tree validates it.
    ///
    /// **Owner-side, immediate.** This is the top-level path — call it when you
    /// hold `&mut Program` (an app `main`, startup, or a test). A *view* has no
    /// up-pointer to the program (D3) and must instead defer via
    /// [`Context::end_modal`](crate::view::Context::end_modal) (→
    /// [`Deferred::EndModal`], applied by the pump). Rule of thumb: view →
    /// `ctx.end_modal`; owner / top-level → `Program::end_modal`.
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

    /// `TApplication::getTileRect` — the rectangle tile/cascade lay windows into:
    /// the **desktop child's extent** (`(0,0,w,h)` in desktop-local coords), so it
    /// stays correct once Phase 4 insets the desktop under a menu/status bar.
    /// Returns `None` if no desktop was created. Used by `Application::tile`/`cascade`
    /// (Phase 4) and the `Application::get_tile_rect` forwarding method.
    ///
    /// Note: requires `&mut self` because `Group::find_mut` requires `&mut`.
    pub fn get_tile_rect(&mut self) -> Option<Rect> {
        let id = self.desktop?;
        self.group.find_mut(id).map(|v| v.state().get_extent())
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

    // -- exec_view: the blocking modal wrapper (row 34, D9) -----------------

    /// `TGroup::execView` (run on `TProgram`, the owner group) — insert a view
    /// modally, drive the loop until it validates an end command, and return that
    /// command. Ports `TGroup::execView` + `TGroup::execute` (`tgroup.cpp`).
    ///
    /// **Top-level only — the type system enforces it:** a [`View`] holds only
    /// `&mut Context`, never `&mut Program`, so a view *cannot* call this from
    /// inside `handle_event` (which is what makes the nested
    /// [`pump_once`](Self::pump_once) loop sound — D9 "exec_view — corrected"). Call
    /// from an app `main`, startup, or a test driving pre-queued events. The
    /// view-/menu-triggered async modal (`Deferred::OpenModal` + a posted
    /// completion command) is **Phase 4** — only the sync `exec_view` is built here.
    ///
    /// **D9 DEVIATION — program-level handling runs during the modal pump (NOT a
    /// faithful 1:1).** Under our single loop, the nested
    /// [`pump_once`](Self::pump_once) calls below still run
    /// [`program_handle_event`] every iteration — so the Alt-N window-selection
    /// block and the `cmQuit` catch are live *during* the modal. C++ does NOT do
    /// this: `TGroup::execView` → `p->execute()` (`tgroup.cpp:205`) dispatches via
    /// `p->handleEvent` (the **dialog's**), so `TProgram::handleEvent` — where
    /// `cmQuit → endModal(cmQuit)` (`tprogram.cpp:205`) and Alt-N live — is NOT in
    /// the modal dispatch path. Consequence: here a `cmQuit` arriving during a
    /// modal ends the modal (with `QUIT`); in C++ it reaches the dialog, goes
    /// unhandled, is discarded, and the modal stays open. We KEEP this behavior
    /// ("cmQuit ends the modal + app even from a dialog" is defensible UX, and no
    /// menu/Alt-N trigger exists at row 34) — see the Phase-4 modal-isolation
    /// breadcrumb on the Alt-N block in [`program_handle_event`].
    ///
    /// **HEADLESS HANG WARNING:** [`pump_once`](Self::pump_once) does not block on a
    /// headless backend, so the inner `while end_state.is_none()` loop spins until
    /// something sets `end_state`. The caller MUST ensure the modal reaches
    /// [`Context::end_modal`] (e.g. a pre-queued `cmOK`/`cmCancel`, or an Esc that a
    /// [`Dialog`](crate::dialog::Dialog) turns into a posted `cmCancel`). A modal
    /// with no path to `end_modal` hangs.
    ///
    /// Control flow (faithful to `execView`):
    /// 1. Save `current` + a clone of the command set (`getCommands`).
    /// 2. **Insert** the view into the root group (we always own it — `saveOwner ==
    ///    0` always here; the "already owned" branch has no caller at row 34).
    ///    Insert FIRST so `set_current` can resolve the id.
    /// 3. Clear `ofSelectable` on the view (`p->options &= ~ofSelectable`).
    /// 4. `setState(sfModal, True)` — set the bit **directly** (NOT via the
    ///    propagating `set_state`: C++ `TGroup::setState` never propagates `sfModal`
    ///    to children, and every existing site sets `.state.modal` directly).
    /// 5. `setCurrent(p, enterSelect)` — selects + focuses the view (fires its
    ///    command enables, deferred; unwound by the command-set restore in step 9).
    /// 6. Push the [`ModalFrame`] directly (we hold `&mut self`, not inside a
    ///    dispatch).
    /// 7. The loop: `loop { end_state = None; while none { pump_once }; if the
    ///    MODAL view's own valid(es) break es }` — validate `p`'s `valid`
    ///    (`TDialog::valid`), NOT the root group's (`tgroup.cpp:184/205`).
    /// 8. Pop the frame, `remove` the view, `setCurrent(saveCurrent, leaveSelect)`.
    /// 9. Restore the command set (`setCommands`).
    pub fn exec_view(&mut self, view: Box<dyn View>) -> Command {
        // 1. getCommands / save the outgoing current.
        let save_current = self.group.current();
        let save_commands = self.command_set.clone();

        // 2. Insert FIRST (always own it: saveOwner == 0). Insert before
        //    set_current so the group can resolve the id (set_current resolves via
        //    index_of and is a silent no-op for an absent id).
        //
        // ROOT-INSERT DEVIATION: this inserts the modal into the ROOT group — the
        // modal becomes a sibling of the desktop. Faithful to C++
        // `application->execView(pD)` (msgbox.cpp:90/186 use exactly this). The
        // alternative C++ pattern, `TProgram::executeDialog`, uses
        // `deskTop->execView(pD)` (tprogram.cpp:119) — the desktop variant, which
        // inserts into the desktop instead. Root-insert is fine for row 34. Revisit
        // when the desktop is inset by a menu/status bar (Phase 4): a desktop-inset
        // modal would then need to clip to the desktop region, compounding the
        // `ModalFrame` (0,0)-coordinate caveat. Do NOT change the insert target now.
        let id = self.group.insert(view);

        // The modal view's bounds in the root group's frame, for the ModalFrame
        // hit-test. For row 31 the root group is at (0,0), so group-local ==
        // absolute (the same ModalFrame coordinate caveat).
        let bounds = self
            .group
            .find_mut(id)
            .map(|v| v.state().get_bounds())
            .unwrap_or_default();

        // 3+4. p->options &= ~ofSelectable (a modal view is not tab-selectable among
        //      siblings — a REAL true->false flip: Window::new sets ofSelectable and a
        //      Dialog delegates `state`) + setState(sfModal, True) set directly (C++
        //      TGroup::setState propagates sfActive/sfDragging/sfFocused, NEVER sfModal,
        //      so a direct write is the faithful port). The saveOptions/restore is moot
        //      — the view is dropped on remove (step 8).
        if let Some(v) = self.group.find_mut(id) {
            let st = v.state_mut();
            st.options.selectable = false;
            st.state.modal = true;
        }

        // 5. setCurrent(p, enterSelect). enterSelect does not deselect the old
        //    current (the desktop stays selected beneath). Build a throwaway
        //    Context over the disjoint fields (the pump's discipline).
        {
            let now = self.clock.now_ms();
            let mut ctx = Context::new(
                &mut self.out_events,
                &mut self.timers,
                now,
                &mut self.deferred,
            );
            self.group
                .set_current(Some(id), SelectMode::Enter, &mut ctx);
        }

        // 6. Push the ModalFrame DIRECTLY (we hold &mut self; we are not inside a
        //    dispatch, so this is not deferred).
        self.captures.push(Box::new(ModalFrame::new(id, bounds)));

        // 7. TGroup::execute — drive the single pump in a bounded top-level loop.
        //    The inner while spins on a headless backend until the modal sets
        //    end_state (the HEADLESS HANG WARNING above); the outer loop re-runs if
        //    valid_end refuses the end command (TGroup::execute's while(!valid)).
        let retval = loop {
            self.end_state = None;
            while self.end_state.is_none() {
                // D9 DEVIATION (see this fn's doc): pump_once runs
                // program_handle_event each pass, so Alt-N + the cmQuit catch are
                // live during the modal. C++ execView -> p->execute() (tgroup.cpp:205)
                // dispatches to the dialog's handleEvent, NOT TProgram::handleEvent
                // (where cmQuit->endModal + Alt-N live, tprogram.cpp:205) — so program
                // handling is out of the modal dispatch path there. We keep ours.
                self.pump_once();
            }
            let es = self.end_state.unwrap();
            // TGroup::execView calls `p->execute()` (tgroup.cpp:205), whose outer
            // `while( !valid(endState) )` (tgroup.cpp:184) invokes the VIRTUAL
            // `valid` on `p` = the modal view (TDialog::valid: cmCancel->true,
            // else the DIALOG's own children). Validate the modal view's OWN
            // `valid` — NOT `self.group.valid` (the ROOT group), which would also
            // consult the desktop sibling (a scope C++ never uses) and is a latent
            // hang if a sibling ever vetoed (the outer loop would re-spin with
            // nothing re-issuing the command). The id still resolves here: `remove`
            // happens after this loop.
            let valid = self.group.find_mut(id).map(|v| v.valid(es)).unwrap_or(true);
            if valid {
                break es;
            }
        };

        // 8. Pop the frame (it is on top — drags self-pop on MouseUp, so nothing
        //    unbalanced remains when end_state is set), then remove the view.
        self.captures.pop();
        {
            let now = self.clock.now_ms();
            let mut ctx = Context::new(
                &mut self.out_events,
                &mut self.timers,
                now,
                &mut self.deferred,
            );
            // saveOwner == 0 -> remove. Group::remove runs reset_current (re-selects
            // the desktop), so the following setCurrent(saveCurrent, leaveSelect) is
            // a faithful no-op in the common case. The view is a direct child of the
            // root group, so Group::remove (not remove_descendant) is correct.
            self.group.remove(id, &mut ctx);
            // setCurrent(saveCurrent, leaveSelect). leaveSelect does not re-select
            // the new current (the desktop already is, via reset_current).
            self.group
                .set_current(save_current, SelectMode::Leave, &mut ctx);
        }

        // The C++ tail `p->setState(sfModal, False); p->options = saveOptions;` is
        // **moot** here: the removed view is owned by the group and dropped on
        // `remove` (we keep no Box from it), so clearing sfModal / restoring options
        // on a dropped object is unobservable — not ported (faithfulness note, D3).

        // 9. setCommands(saveCommands): restore the command set. Restoring is not an
        //    app-visible toggle the way enable/disable is, so we do NOT set
        //    command_set_changed (no re-broadcast): the modal's command enables were
        //    transient and unwinding them is internal bookkeeping, not a state the
        //    app reacts to.
        //
        //    DEVIATION: C++ TView::setCommands DOES set commandSetChanged when the
        //    sets differ — and here they do differ (the modal enabled
        //    cmNext/cmPrev/cmClose/cmZoom), so C++ fires a post-modal
        //    cmCommandSetChanged broadcast we omit. Deliberate, and moot at row 34
        //    (no observer of the command set exists yet); align when one does.
        self.command_set = save_commands;

        // TheTopView dropped (D8: no occlusion/exposed); no consumer.

        retval
    }

    /// One iteration of the loop — the heart of D9.
    ///
    /// Borrow discipline (the brief's #1 risk): `self` is destructured into field
    /// bindings at the top, so the disjoint fields backing [`Context`]
    /// (`out_events` / `timers` / `deferred`) can be borrowed alongside
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
            deferred,
            command_set,
            desktop,
            // The menu bar is not read by the pump (its events route through the
            // normal group dispatch / preProcess phase); bind it `_` so the
            // exhaustive destructure does not trip `-D warnings`.
            menu_bar: _,
            status_line,
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
                    out_events.push_back(Event::Broadcast {
                        command: Command::COMMAND_SET_CHANGED,
                        source: None,
                    });
                    *command_set_changed = false;
                }
                // collectExpiredTimers: each expired timer queues a typed
                // `Event::Timer(id)` carrying its own [`TimerId`](crate::timer::TimerId) (the successor to
                // `evBroadcast cmTimerExpired` with `message.infoPtr == TTimerId`).
                // This is strictly more correct than the old code, which queued N
                // indistinguishable `cmTimerExpired` broadcasts for N expired ids;
                // now a widget can tell *which* timer fired. (timer-payload TODO
                // RESOLVED.)
                for id in timers.collect_expired(now) {
                    out_events.push_back(Event::Timer(id));
                }
                // TProgram::idle's statusLine->update() (re-run findItems against
                // the top view's getHelpCtx + redraw) is OMITTED-UNTIL-CONSUMER: with
                // the universal TStatusDef(0, 0xFFFF) (`All`) def every real app + our
                // demo uses, find_items is INVARIANT — it selects the same def for any
                // help context, so update() is observably inert. Adding it would force
                // a View::get_help_ctx method + a TopView resolver with no consumer
                // (the row-34 omit-until-consumer rule). Revisit when a context-split
                // (`OneOf`) status line lands and the selected def actually depends on
                // the focused view's help context.
            }
            // 5. Event present -> dispatch.
            Some(mut ev) => {
                // getEvent status-line pre-routing (tprogram.cpp:153). keyDown
                // always; mouseDown only when the status line is the topmost view
                // under the cursor (firstThat(viewHasMouse) == statusLine) — else
                // its unconditional clear would eat a click meant for the desktop /
                // a dialog. This runs BEFORE drop_disabled + captures.dispatch
                // because C++ getEvent pre-routes regardless of modal state, so
                // accelerators (F10 → cmMenu, Alt-X → cmQuit) must fire even while a
                // modal dialog is open. The keyDown arm transforms `ev` into
                // Event::Command in place (no clear), so the SAME live `ev` flows on
                // into normal dispatch and routes; the mouseDown arm posts the hit
                // item's command to `out_events` and clears `ev` (routed next pump).
                if let Some(sl) = *status_line {
                    let pre = match &ev {
                        Event::KeyDown(_) => true,
                        Event::MouseDown(m) => group.topmost_child_at(m.position) == Some(sl),
                        _ => false,
                    };
                    if pre && let Some(v) = group.find_mut(sl) {
                        // LATENT COUPLING: the pre-route must not queue a Deferred —
                        // the deferred drain below is gated on `!ev.is_nothing()`, and
                        // a cleared mouseDown (the status line always clears) skips it.
                        // Safe today because the status line only defers from its
                        // Broadcast arm, which is never pre-routed (pre is keyDown /
                        // mouseDown only). Revisit if a pre-routed arm ever defers.
                        //
                        // Translate a mouse position into the status line's own frame
                        // before handing it over: normally Group::deliver does this
                        // (subtract the child's bounds top-left == makeLocal), but the
                        // pre-route bypasses the group router. A keyDown carries no
                        // position, so this is a no-op for the accelerator path. The
                        // status line always clears a MouseDown, so mutating `ev` in
                        // place is safe (nothing downstream reads the position).
                        let origin = v.state().get_bounds().a;
                        if let Event::MouseDown(m) = &mut ev {
                            m.position -= origin;
                        }
                        let mut ctx = Context::new(out_events, timers, now, deferred);
                        v.handle_event(&mut ev, &mut ctx);
                    }
                }

                // Command filtering at the program boundary (D1): drop a disabled
                // command before routing. Broadcasts/keys/mouse flow regardless.
                let drop_disabled = matches!(ev, Event::Command(c) if !command_set.has(c));
                if drop_disabled {
                    ev.clear();
                }

                if !ev.is_nothing() {
                    // Refresh bounds-gating capture handlers (the modal frame) from
                    // the live tree before dispatch, so a modal that has been
                    // dragged/resized is gated at its CURRENT position, not the one
                    // cached at push time (else the moved dialog goes mouse-dead).
                    captures
                        .sync_gate_bounds(|id| group.find_mut(id).map(|v| v.state().get_bounds()));
                    // The Context borrow ends at this block's close, before we
                    // drain the deferred queue back into loop/tree state.
                    {
                        let mut ctx = Context::new(out_events, timers, now, deferred);
                        // Offer to the capture stack first; if consumed, skip view
                        // routing.
                        let consumed = captures.dispatch(&mut ev, &mut ctx);
                        if !consumed {
                            program_handle_event(group, *desktop, &mut ev, &mut ctx, end_state);
                        }
                    }
                    // Apply the deferred queue AFTER dispatch — one drain, in
                    // insertion order. Drain to a local first (`mem::take`): the
                    // apply-Context borrows the now-empty `deferred` field (so a
                    // SetState/Close that re-queues lands for the NEXT pump), which
                    // would otherwise alias the iteration. ONE pass only — anything
                    // an applied effect re-queues (none do today) waits for the next
                    // pump; do not loop until empty (a bug would spin).
                    //
                    // The three families touch disjoint loop-owned state — capture
                    // stack / command set / view tree — so applying in insertion
                    // order (interleaving kinds) is equivalent to today's
                    // captures-then-commands-then-tree ordering: cross-family order
                    // cannot affect the result, and same-family relative order is
                    // preserved. PushCapture still applies after dispatch, so a
                    // pushed handler still sees the NEXT event (compose_full_protocol).
                    let effects: Vec<Deferred> = std::mem::take(deferred);
                    if !effects.is_empty() {
                        let mut ctx = Context::new(out_events, timers, now, deferred);
                        for effect in effects {
                            match effect {
                                Deferred::PushCapture(h) => captures.push(h),
                                // Inline the enable/disable bodies — the destructure
                                // gives the fields, not `self`. Flip
                                // `command_set_changed` on a real change so the next
                                // idle broadcasts cmCommandSetChanged.
                                Deferred::EnableCommand(cmd) => {
                                    if !command_set.has(cmd) {
                                        command_set.enable_cmd(cmd);
                                        *command_set_changed = true;
                                    }
                                }
                                Deferred::DisableCommand(cmd) => {
                                    if command_set.has(cmd) {
                                        command_set.disable_cmd(cmd);
                                        *command_set_changed = true;
                                    }
                                }
                                Deferred::ChangeBounds(id, r) => {
                                    if let Some(v) = group.find_mut(id) {
                                        v.change_bounds(r);
                                    }
                                }
                                Deferred::SetState(id, f, e) => {
                                    if let Some(v) = group.find_mut(id) {
                                        v.set_state(f, e, &mut ctx);
                                    }
                                }
                                Deferred::Close(id) => {
                                    group.remove_descendant(id, &mut ctx);
                                }
                                // TLabel::focusLink — select the linked view within
                                // its owning group (the group walk applies the
                                // ofSelectable gate). Ignore the found/not-found bool,
                                // like Close.
                                Deferred::FocusById(id) => {
                                    group.focus_descendant(id, &mut ctx);
                                }
                                // TGroup::endModal — set the loop end state; the
                                // nested exec_view loop (row 34) observes it.
                                Deferred::EndModal(cmd) => {
                                    *end_state = Some(cmd);
                                }
                                // -- row 27: TScroller cross-view broker --------
                                //
                                // The pump is the broker: a scroller (a leaf, D3)
                                // can neither read nor mutate its window-frame
                                // sibling scrollbars, so the read/write happens here
                                // where the whole tree is reachable via `group`.
                                //
                                // Read direction (TScroller::scrollDraw): resolve
                                // each bar and read its `value` (via View::value →
                                // FieldValue::Int) in its OWN find_mut so only one
                                // `&mut` is live at a time, then find_mut the
                                // scroller and push the delta in.
                                Deferred::SyncScrollerDelta { scroller, h, v } => {
                                    use crate::widgets::Scroller;
                                    let dx = h
                                        .and_then(|id| group.find_mut(id))
                                        .and_then(|view| view.value())
                                        .and_then(field_int)
                                        .unwrap_or(0);
                                    let dy = v
                                        .and_then(|id| group.find_mut(id))
                                        .and_then(|view| view.value())
                                        .and_then(field_int)
                                        .unwrap_or(0);
                                    if let Some(s) = group
                                        .find_mut(scroller)
                                        .and_then(|view| view.as_any_mut())
                                        .and_then(|a| a.downcast_mut::<Scroller>())
                                    {
                                        s.apply_delta(Point::new(dx, dy));
                                    }
                                }
                                // Write direction (TScrollBar::setParams driven by
                                // TScroller::setLimit/scrollTo): fill each `None`
                                // field from the bar's live value, then set_params
                                // (which clamps and may re-broadcast CHANGED — fine,
                                // no loop: the read-sync writes nothing back). `group`
                                // and `ctx` are disjoint borrows here.
                                Deferred::ScrollBarSetParams {
                                    id,
                                    value,
                                    min,
                                    max,
                                    page_step,
                                    arrow_step,
                                } => {
                                    use crate::widgets::ScrollBar;
                                    if let Some(sb) = group
                                        .find_mut(id)
                                        .and_then(|view| view.as_any_mut())
                                        .and_then(|a| a.downcast_mut::<ScrollBar>())
                                    {
                                        let v = value.unwrap_or(sb.value);
                                        let lo = min.unwrap_or(sb.min_value);
                                        let hi = max.unwrap_or(sb.max_value);
                                        let pg = page_step.unwrap_or(sb.page_step);
                                        let ar = arrow_step.unwrap_or(sb.arrow_step);
                                        sb.set_params(v, lo, hi, pg, ar, &mut ctx);
                                    }
                                }
                                // Visibility direction (TScroller::showSBar →
                                // show/hide): set visible directly (no propagating
                                // StateFlag::Visible; the painter skips !visible).
                                Deferred::SetVisible(id, visible) => {
                                    if let Some(view) = group.find_mut(id) {
                                        view.state_mut().state.visible = visible;
                                    }
                                }
                                // -- row 28: TListViewer read-sync broker -------
                                //
                                // Like SyncScrollerDelta, but the list base is a
                                // TRAIT (subclasses reuse its draw + override
                                // get_text/is_selected), so `dyn View → dyn
                                // ListViewer` cannot be downcast. Instead we read
                                // each bar's `value` (each in its own find_mut so
                                // only one &mut is live) and call back through the
                                // defaulted View::apply_list_scroll trait method.
                                //
                                // This read-sync WRITES BACK (apply_list_scroll →
                                // focus_item_num → focusItem → v-bar setValue), so it
                                // could re-enter — but ScrollBar::set_params is
                                // change-guarded (re-broadcasts only on an actual
                                // value change), so the write-back of the already-
                                // current value is a silent no-op and the cycle goes
                                // quiet. `ctx` is live here (same as the
                                // ScrollBarSetParams arm), so the write-back's
                                // request_scroll_bar_params lands in `deferred` for
                                // the NEXT pump.
                                Deferred::SyncListViewer { list, h, v } => {
                                    let hv = h
                                        .and_then(|id| group.find_mut(id))
                                        .and_then(|view| view.value())
                                        .and_then(field_int);
                                    let vv = v
                                        .and_then(|id| group.find_mut(id))
                                        .and_then(|view| view.value())
                                        .and_then(field_int);
                                    if let Some(view) = group.find_mut(list) {
                                        view.apply_list_scroll(hv, vv, &mut ctx);
                                    }
                                }
                                // -- row 49: TMenuView command-graying broker --
                                //
                                // The menu view (a child, D3) cannot read the
                                // command set inline — the pump owns it. Resolve
                                // the menu view and call back through the defaulted
                                // View::update_menu_commands trait method with the
                                // live `command_set` in hand (it regrays the menu
                                // tree). `group` and `command_set` are disjoint
                                // destructured fields, so no `ctx` is needed (like
                                // ChangeBounds).
                                Deferred::UpdateMenu(id) => {
                                    if let Some(v) = group.find_mut(id) {
                                        v.update_menu_commands(command_set);
                                    }
                                }
                                // -- rows 50-52: the TMenuView modal layer ------
                                //
                                // OpenMenuBox: build a MenuBox from the (cloned)
                                // submenu over `bounds` and insert it into the
                                // ROOT group with the session's pre-minted id (no
                                // focus move — a box is never current, Clean
                                // Architecture A). `bounds` is already the
                                // box-sized rect (menu_box_rect ran at the session;
                                // MenuBox::new re-clamps inside its own bounds, a
                                // no-op for an already-fitted rect since b == a+w/h).
                                Deferred::OpenMenuBox { id, menu, bounds } => {
                                    use crate::menu::MenuBox;
                                    group.insert_with_id(Box::new(MenuBox::new(bounds, menu)), id);
                                }
                                // SetMenuCurrent: write the session-owned highlight
                                // cache into the bar/box view for `draw` (the
                                // set_menu_current trait hook — no downcast, like
                                // update_menu_commands).
                                Deferred::SetMenuCurrent(id, current) => {
                                    if let Some(v) = group.find_mut(id) {
                                        v.set_menu_current(current);
                                    }
                                }
                            }
                        }
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
            &mut self.deferred,
        );
        f(&mut self.group, &mut ctx)
    }
}

// ---------------------------------------------------------------------------
// program-level handle_event — ports TProgram::handleEvent (free fn, D9 borrows)
// ---------------------------------------------------------------------------

/// Extract the `i32` out of a [`FieldValue::Int`](crate::data::FieldValue::Int),
/// or `None` for any other variant. Used by the row-27 `TScroller` read-broker to
/// read a scrollbar's `value` through the generic [`View::value`](crate::view::View::value)
/// (the successor to C++ `hScrollBar->value`).
fn field_int(v: crate::data::FieldValue) -> Option<i32> {
    match v {
        crate::data::FieldValue::Int(n) => Some(n),
        _ => None,
    }
}

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
    desktop: Option<ViewId>,
    ev: &mut Event,
    ctx: &mut Context,
    end_state: &mut Option<Command>,
) {
    // TODO(Phase 4: modal isolation): when menus + multiple windows + a modal
    // coexist, program-level interception (this Alt-N block + the cmQuit catch
    // below) should be SUPPRESSED while a modal is active — C++'s nested
    // `p->execute()` (tgroup.cpp:205) structurally prevents it by dispatching to
    // the dialog's handleEvent, not TProgram's. Our single loop (D9) runs this on
    // every pump, including modal pumps (deviation documented on `exec_view`). No
    // trigger exists yet (no menu/Alt-N source at row 34), so this is a breadcrumb.
    //
    // Alt+digit window selection (cmSelectWindowNum). Faithful TProgram::handleEvent
    // order: the Alt-N block runs BEFORE the group dispatch. The window NUMBER is an
    // integer, not a ViewId, so this is a DIRECT walk (the program asks the desktop
    // to select the child whose `number` matches), NOT a Broadcast{source} — that
    // substrate serves the polymorphic infoPtr *subject* case, not an int payload.
    //
    // The three-way clear matrix (faithful to the C++):
    //   can && matched  -> clear (the select consumed it).
    //   can && !matched -> do NOT clear (event stays live, falls through to the
    //                      group; C++ `message()==0` path: no clearEvent).
    //   !can            -> clear (C++ else branch).
    if let Event::KeyDown(k) = *ev
        && let Key::Char(c) = k.key
        && ('1'..='9').contains(&c)
        && k.modifiers.alt
        && !k.modifiers.ctrl
        && !k.modifiers.shift
    {
        let num = (c as i16) - ('0' as i16);
        // canMoveFocus(): deskTop->valid(cmReleasedFocus) — desktop-specific, NOT
        // the root group's valid().
        let can = desktop
            .and_then(|id| group.find_mut(id))
            .is_some_and(|dt| dt.valid(Command::RELEASED_FOCUS));
        if can {
            let matched = desktop
                .and_then(|id| group.find_mut(id))
                .is_some_and(|dt| dt.select_window_num(num, ctx));
            if matched {
                ev.clear();
            }
            // can-but-no-match: leave the event LIVE — it falls through to
            // group.handle_event below.
        } else {
            ev.clear(); // !canMoveFocus -> clearEvent.
        }
    }

    group.handle_event(ev, ctx);

    // C++: endModal(cmQuit); clearEvent(event).
    if *ev == Event::Command(Command::QUIT) {
        *end_state = Some(Command::QUIT);
        ev.clear();
    }

    // cmTile/cmCascade — program-level commands (TApplication::handleEvent,
    // tapplica.cpp). C++ calls TProgram::handleEvent FIRST, then handles these — so
    // this slot is after group dispatch, beside the QUIT catch. Faithful:
    //   case cmTile:    deskTop->tile(    getTileRect() ); clearEvent(); break;
    //   case cmCascade: deskTop->cascade( getTileRect() ); clearEvent(); break;
    // getTileRect() == the desktop child's local extent; computed inline via two
    // find_mut calls (the first borrow ends when `r` becomes an owned Rect), mirroring
    // the Alt-N block's borrow style. cmDosShell is still deferred (needs a backend
    // suspend seam).
    if let Event::Command(cmd) = *ev
        && (cmd == Command::TILE || cmd == Command::CASCADE)
        && let Some(id) = desktop
    {
        let r = group.find_mut(id).map(|v| v.state().get_extent());
        if let (Some(r), Some(dt)) = (r, group.find_mut(id)) {
            if cmd == Command::TILE {
                dt.tile(r);
            } else {
                dt.cascade(r);
            }
        }
        ev.clear(); // clearEvent after handling.
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
    use crate::timer::{ManualClock, TimerId};
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

    fn mouse_move_at(x: i32, y: i32) -> Event {
        Event::MouseMove(MouseEvent {
            position: Point::new(x, y),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        })
    }

    fn mouse_up_at(x: i32, y: i32) -> Event {
        Event::MouseUp(MouseEvent {
            position: Point::new(x, y),
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

    /// Build a `Program` whose desktop holds `n` selectable numbered windows
    /// (numbered `1..=n`, all `wfMove|wfGrow|wfClose|wfZoom` defaults). Returns the
    /// program and the window ids (index 0 == window #1). Window #1 is selected by
    /// injecting `Alt+'1'` and running a real `pump_once` — so the *production*
    /// path selects it AND drains `deferred`, enabling `{cmNext, cmPrev}` through
    /// the program's command set exactly as it would at runtime (no test-only
    /// command-enable shortcut). The round-trip tests therefore start from a
    /// genuinely focused-window state whose command enables came from the pump.
    ///
    /// Windows must live *inside the desktop* (the cmNext/cmPrev/Alt-N handlers are
    /// on the desktop), so they are inserted in the `create_desktop` closure where
    /// the `Desktop` is still concrete, and their ids leak out via an `Rc<RefCell>`.
    fn program_with_windows(
        screen_w: u16,
        screen_h: u16,
        n: i16,
    ) -> (Program, Vec<crate::view::ViewId>) {
        let (backend, _handle) = HeadlessBackend::new(screen_w, screen_h);
        let theme = Theme::classic_blue();
        let clock = Rc::new(ManualClock::new(0));
        let ids: Rc<RefCell<Vec<crate::view::ViewId>>> = Rc::new(RefCell::new(Vec::new()));
        let ids_cap = ids.clone();
        let mut program = Program::new(
            Box::new(backend),
            Box::new(clock),
            theme,
            move |r| {
                let mut desktop = Desktop::new(r, |r2| Some(Desktop::init_background(r2)));
                for num in 1..=n {
                    // Stagger the windows so each occupies a distinct rect.
                    let x = 2 + (num as i32) * 2;
                    let win = Window::new(
                        Rect::new(x, num as i32, x + 20, num as i32 + 8),
                        Some(format!("W{num}")),
                        num,
                    );
                    ids_cap
                        .borrow_mut()
                        .push(desktop.insert_view(Box::new(win)));
                }
                Some(Box::new(desktop))
            },
            |_r| None,
            |_r| None,
        );
        program.out_events.clear();

        // Select window #1 through the *production* path: inject Alt+'1' and pump.
        // At construction the desktop is root-current with its own `current == None`,
        // so `desktop.valid(cmReleasedFocus)` (canMoveFocus) is true and the Alt-N
        // walk selects window 1. The pump then drains `deferred`, so the
        // `EnableCommand(cmNext/cmPrev)` that `set_state(Selected)` queued is really
        // applied to `command_set` — exactly the enable-filter path the cmNext/cmPrev
        // round-trip tests exercise. No test-only command-enable shortcut.
        program.out_events.push_back(alt_digit('1'));
        program.pump_once();
        program.out_events.clear();

        let id_vec = ids.borrow().clone();
        (program, id_vec)
    }

    /// Read whether the window `id` is the desktop's selected (current) window —
    /// its own `sfSelected`, set by `set_current`'s `Selected` propagation.
    fn win_selected(program: &mut Program, id: crate::view::ViewId) -> bool {
        program
            .group_mut()
            .find_mut(id)
            .map(|v| v.state().state.selected)
            .unwrap_or(false)
    }

    fn alt_digit(c: char) -> Event {
        Event::KeyDown(KeyEvent::new(
            Key::Char(c),
            KeyModifiers {
                alt: true,
                ..Default::default()
            },
        ))
    }

    /// Read window `id`'s current bounds (for the tile/cascade pump test).
    fn win_bounds(program: &mut Program, id: crate::view::ViewId) -> Rect {
        program
            .group_mut()
            .find_mut(id)
            .map(|v| v.state().get_bounds())
            .expect("window resolves")
    }

    /// Build a program whose desktop holds `n` **tileable** numbered windows; the
    /// command set keeps its `cmTile`/`cmCascade` defaults. Returns the program and
    /// the window ids (index 0 == window #1). No window is pre-selected (cmTile is a
    /// program-level command that does not require a focused window).
    fn program_with_tileable_windows(n: i16) -> (Program, Vec<crate::view::ViewId>) {
        let (backend, _handle) = HeadlessBackend::new(80, 25);
        let theme = Theme::classic_blue();
        let clock = Rc::new(ManualClock::new(0));
        let ids: Rc<RefCell<Vec<crate::view::ViewId>>> = Rc::new(RefCell::new(Vec::new()));
        let ids_cap = ids.clone();
        let mut program = Program::new(
            Box::new(backend),
            Box::new(clock),
            theme,
            move |r| {
                let mut desktop = Desktop::new(r, |r2| Some(Desktop::init_background(r2)));
                for num in 1..=n {
                    let x = 2 + (num as i32) * 2;
                    let mut win = Window::new(
                        Rect::new(x, num as i32, x + 20, num as i32 + 8),
                        Some(format!("W{num}")),
                        num,
                    );
                    win.state_mut().options.tileable = true;
                    ids_cap
                        .borrow_mut()
                        .push(desktop.insert_view(Box::new(win)));
                }
                Some(Box::new(desktop))
            },
            |_r| None,
            |_r| None,
        );
        program.out_events.clear();
        let id_vec = ids.borrow().clone();
        (program, id_vec)
    }

    // -- row 30: cmTile routes through the pump to Desktop::tile --------------

    /// End-to-end breadcrumb: posting `cmTile` (as a menu item would) makes
    /// `pump_once` lay the desktop's tileable windows into a grid AND clear the
    /// command event. Bite: windows must move to their `calc_tile_rect` cells (a
    /// missing/wrong wiring leaves them at their staggered ctor bounds), and no
    /// `Command(cmTile)` may survive in the queue.
    #[test]
    fn cm_tile_relocates_windows_through_pump() {
        let (mut program, ids) = program_with_tileable_windows(2);
        let (w1, w2) = (ids[0], ids[1]);
        let before1 = win_bounds(&mut program, w1);
        let before2 = win_bounds(&mut program, w2);

        program.out_events.push_back(Event::Command(Command::TILE));
        program.pump_once();

        let after1 = win_bounds(&mut program, w1);
        let after2 = win_bounds(&mut program, w2);
        assert_ne!(after1, before1, "window 1 relocated by cmTile");
        assert_ne!(after2, before2, "window 2 relocated by cmTile");
        // n=2 over the full 80×25 desktop extent → num_cols=1, num_rows=2 → two
        // stacked half-height cells. forEach order = [w2, w1]; tile_num = 1, 0.
        // (getTileRect is the desktop child's local extent, 0,0,80,25.)
        assert_eq!(
            after2,
            Rect::new(0, 12, 80, 25),
            "topmost (w2) gets tile_num 1 → bottom cell"
        );
        assert_eq!(
            after1,
            Rect::new(0, 0, 80, 12),
            "w1 gets tile_num 0 → top cell"
        );
        // clearEvent after handling: no live cmTile command survives.
        assert!(
            !program
                .out_events
                .iter()
                .any(|e| matches!(e, Event::Command(c) if *c == Command::TILE)),
            "cmTile was consumed (clearEvent)"
        );
    }

    // -- row 30: cmCascade routes through the pump to Desktop::cascade --------

    /// End-to-end breadcrumb mirror of the cmTile test: posting `cmCascade` makes
    /// `pump_once` cascade the desktop's tileable windows AND clear the command.
    /// Bite: the first-visited (topmost, last-inserted) window must take offset
    /// `+ (n-1)` and the last `+ 0` (a reversed order or wrong start fails the exact
    /// bounds), and no `Command(cmCascade)` may survive the queue.
    #[test]
    fn cm_cascade_relocates_windows_through_pump() {
        let (mut program, ids) = program_with_tileable_windows(2);
        let (w1, w2) = (ids[0], ids[1]);
        let before1 = win_bounds(&mut program, w1);
        let before2 = win_bounds(&mut program, w2);

        program
            .out_events
            .push_back(Event::Command(Command::CASCADE));
        program.pump_once();

        let after1 = win_bounds(&mut program, w1);
        let after2 = win_bounds(&mut program, w2);
        assert_ne!(after1, before1, "window 1 relocated by cmCascade");
        assert_ne!(after2, before2, "window 2 relocated by cmCascade");
        // getTileRect = desktop child extent (0,0,80,25). n=2 → offsets 1, 0 in
        // forEach order [w2, w1]. locate clamps to size_limits (window min 16×6,
        // no max), so the offset rects pass through unchanged.
        assert_eq!(
            after2,
            Rect::new(1, 1, 80, 25),
            "topmost (w2) gets r.a + (n-1) = +1"
        );
        assert_eq!(after1, Rect::new(0, 0, 80, 25), "w1 gets r.a + 0");
        // clearEvent after handling: no live cmCascade command survives.
        assert!(
            !program
                .out_events
                .iter()
                .any(|e| matches!(e, Event::Command(c) if *c == Command::CASCADE)),
            "cmCascade was consumed (clearEvent)"
        );
    }

    // -- 33d-2: Alt-N selects a numbered window ------------------------------

    #[test]
    fn alt_n_selects_numbered_window() {
        let (mut program, ids) = program_with_windows(80, 25, 2);
        let (w1, w2) = (ids[0], ids[1]);
        assert!(win_selected(&mut program, w1), "window 1 starts selected");
        assert!(
            !win_selected(&mut program, w2),
            "window 2 starts unselected"
        );

        // Alt+2 selects window 2.
        program.out_events.push_back(alt_digit('2'));
        program.pump_once();
        assert!(win_selected(&mut program, w2), "Alt+2 selects window 2");
        assert!(
            !win_selected(&mut program, w1),
            "window 1 deselected (focus moved)"
        );
        // The Alt-N keydown was consumed (can && matched -> clear). It must not
        // survive as a KeyDown in the queue (selection legitimately *does* enqueue
        // focus-change Broadcasts, so the queue is not empty — assert on KeyDown).
        assert!(
            !program
                .out_events
                .iter()
                .any(|e| matches!(e, Event::KeyDown(_))),
            "Alt+2 was consumed: no KeyDown survives"
        );
    }

    // -- row 41: Deferred::FocusById wires through the pump ------------------

    /// The FOUNDATION seam end-to-end: a `Deferred::FocusById(id)` queued **during
    /// an event dispatch** (exactly when `TLabel::focusLink`'s `ctx.request_focus`
    /// runs — from inside `handle_event`) is drained by that same `pump_once` pass,
    /// resolved via `group.focus_descendant`, and focuses (selects) the named view.
    ///
    /// The apply loop only runs on the event-dispatch branch (a label never queues
    /// `FocusById` without a triggering MouseDown/key), so the test injects a benign
    /// broadcast to drive a dispatch and pushes the `FocusById` alongside it — the
    /// faithful shape of "a label converts the dispatched event into a focus
    /// request". Uses the production desktop+windows tree (windows are selectable),
    /// so `FocusById(w2)` must make window 2 the selected (current) one.
    #[test]
    fn deferred_focus_by_id_selects_target_through_pump() {
        let (mut program, ids) = program_with_windows(80, 25, 2);
        let (w1, w2) = (ids[0], ids[1]);
        assert!(win_selected(&mut program, w1), "window 1 starts selected");
        assert!(!win_selected(&mut program, w2));

        // Queue the focus-by-id effect exactly as `Context::request_focus` would
        // from inside a label's `handle_event`, and drive a dispatch with a benign
        // broadcast so the pump reaches its deferred-apply loop. The apply loop then
        // drains `FocusById` into `group.focus_descendant(w2)`.
        program.deferred.push(Deferred::FocusById(w2));
        program.out_events.push_back(Event::Broadcast {
            command: Command::custom("test.noop"),
            source: None,
        });
        program.pump_once();

        assert!(
            win_selected(&mut program, w2),
            "Deferred::FocusById(w2) selected window 2 through the pump"
        );
        assert!(
            !win_selected(&mut program, w1),
            "window 1 deselected (focus moved to w2)"
        );
    }

    #[test]
    fn alt_n_no_match_does_not_change_selection() {
        let (mut program, ids) = program_with_windows(80, 25, 2);
        let (w1, w2) = (ids[0], ids[1]);
        assert!(win_selected(&mut program, w1));

        // Insert a recording probe into the ROOT group with `ofPreProcess` (NOT
        // current — making it current would release the desktop's focus and muddy
        // the selection-unchanged assertion). PreProcess puts it in the focused-event
        // path regardless of who is current, so it sees any KeyDown that survives the
        // program-level Alt-N block and reaches `group.handle_event`.
        let probe_log = Rc::new(RefCell::new(Vec::new()));
        {
            let mut probe = Probe::new(Rect::new(0, 0, 4, 2), 'P', probe_log.clone());
            probe.st.options.pre_process = true;
            program.group_mut().insert(Box::new(probe));
        }
        program.out_events.clear();

        // Alt+9: no window 9. can && !matched -> event stays LIVE, falls through to
        // group.handle_event (C++ message()==0 path: no clearEvent). This is the
        // discriminating teeth: a wrongly-cleared event would ALSO leave selection
        // unchanged, so we must prove the event was NOT cleared — i.e. the probe
        // received it. (The matched-case sibling asserts the inverse: no KeyDown
        // survives.)
        program.out_events.push_back(alt_digit('9'));
        program.pump_once();
        assert!(
            win_selected(&mut program, w1),
            "current unchanged on no match"
        );
        assert!(!win_selected(&mut program, w2), "window 2 still unselected");
        assert!(
            probe_log
                .borrow()
                .iter()
                .any(|e| matches!(e, Event::KeyDown(k) if k.key == Key::Char('9'))),
            "can && !matched: the live Alt+9 fell through to the group (not cleared)"
        );
    }

    // -- 33d-2: cmNext cycles windows ----------------------------------------

    #[test]
    fn cm_next_cycles_to_findnext_window() {
        let (mut program, ids) = program_with_windows(80, 25, 2);
        let (w1, w2) = (ids[0], ids[1]);
        assert!(win_selected(&mut program, w1), "w1 current at start");

        // cmNext: must be ENABLED (selecting w1 enabled {cmNext,cmPrev}); the
        // command survives the program's command-set filter, routes to the desktop's
        // current child = the desktop, whose handle_event runs focus_next(false).
        program.out_events.push_back(Event::Command(Command::NEXT));
        program.pump_once();
        assert!(
            win_selected(&mut program, w2),
            "cmNext advanced to window 2"
        );
        assert!(!win_selected(&mut program, w1), "window 1 deselected");
    }

    /// If cmNext were dropped by the command-set filter (i.e. not enabled), this
    /// would be a no-op — guarding the enable-filter path the brief calls out.
    #[test]
    fn cm_next_is_dropped_when_disabled() {
        let (mut program, ids) = program_with_windows(80, 25, 2);
        let (w1, w2) = (ids[0], ids[1]);
        program.disable_command(Command::NEXT);
        program.out_events.clear();

        program.out_events.push_back(Event::Command(Command::NEXT));
        program.pump_once();
        assert!(
            win_selected(&mut program, w1),
            "disabled cmNext is filtered: no cycle"
        );
        assert!(!win_selected(&mut program, w2));
    }

    // -- 33d-2: cmPrev sends current to back ---------------------------------

    #[test]
    fn cm_prev_sends_current_to_back_and_cycles() {
        // Three windows so the Z-order change is observable as a focus move.
        let (mut program, ids) = program_with_windows(80, 25, 3);
        let w1 = ids[0];
        assert!(win_selected(&mut program, w1), "w1 current at start");

        // cmPrev: current->putInFrontOf(background) sends w1 to the back; the
        // trailing resetCurrent (in put_in_front_of, ofSelectable) re-selects the
        // new front-most selectable window — so w1 is no longer current.
        program.out_events.push_back(Event::Command(Command::PREV));
        program.pump_once();
        assert!(
            !win_selected(&mut program, w1),
            "cmPrev sent w1 to the back; a different window is now current"
        );
        // Some other window became current (Z-order changed).
        let some_other_current = ids[1..].iter().any(|&id| win_selected(&mut program, id));
        assert!(
            some_other_current,
            "another window became current after cmPrev"
        );
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
        // a timer on its first event. Capture the armed TimerId so the test can
        // assert the expiry event carries *that* id (the identity, not just kind).
        let arming = Rc::new(RefCell::new(true));
        let armed_id: Rc<RefCell<Option<TimerId>>> = Rc::new(RefCell::new(None));
        {
            let arming = arming.clone();
            let armed_id = armed_id.clone();
            let mut probe = Probe::new(Rect::new(0, 0, 4, 2), 'P', log.clone());
            probe.action = Some(Box::new(move |ctx: &mut Context| {
                if *arming.borrow() {
                    let id = ctx.set_timer(Duration::from_millis(50), None);
                    *armed_id.borrow_mut() = Some(id);
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
        program.out_events.push_back(Event::Broadcast {
            command: Command::SCROLL_BAR_CHANGED,
            source: None,
        });
        program.pump_once(); // probe arms a 50ms timer at now=0
        assert_eq!(program.timers.len(), 1, "probe armed a timer");
        let expected_id = armed_id.borrow().expect("probe captured the armed TimerId");

        // Advance past expiry; an idle pump (no queued events, none polled)
        // collects the timer and queues a typed Event::Timer(id).
        clock.advance(60);
        log.borrow_mut().clear();
        program.pump_once(); // idle: collect -> queue Event::Timer(id)
        assert!(
            program
                .out_events
                .iter()
                .any(|e| matches!(e, Event::Timer(id) if *id == expected_id)),
            "expired timer queued Event::Timer carrying the armed id"
        );

        // Next pump routes the queued timer event; the probe records it.
        program.pump_once();
        assert!(
            log.borrow()
                .iter()
                .any(|e| matches!(e, Event::Timer(id) if *id == expected_id)),
            "probe received Event::Timer carrying the armed id"
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
            .filter(|e| {
                matches!(
                    e,
                    Event::Broadcast { command, .. } if *command == Command::COMMAND_SET_CHANGED
                )
            })
            .count();
        assert_eq!(count, 1, "command-set change broadcasts exactly once");

        // A second idle pump does NOT re-broadcast (flag cleared). Drain the queue
        // first so the previous broadcast does not linger.
        program.out_events.clear();
        program.pump_once();
        let count2 = program
            .out_events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Event::Broadcast { command, .. } if *command == Command::COMMAND_SET_CHANGED
                )
            })
            .count();
        assert_eq!(count2, 0, "no re-broadcast after the flag is cleared");
    }

    // -- 9. deferred command-enable channel (Context -> Program) -------------

    #[test]
    fn ctx_enable_command_applies_after_dispatch_and_unblocks_routing() {
        let (mut program, _screen, _clock) = program_with_desktop(12, 4);
        let log = Rc::new(RefCell::new(Vec::new()));

        // cmZoom starts DISABLED (default_command_set omits it).
        assert!(!program.command_enabled(Command::ZOOM));

        // A probe that enables cmZoom via the downward Context on its first event.
        let enabled = Rc::new(RefCell::new(false));
        {
            let enabled = enabled.clone();
            let mut probe = Probe::new(Rect::new(0, 0, 4, 2), 'P', log.clone());
            probe.action = Some(Box::new(move |ctx: &mut Context| {
                if !*enabled.borrow() {
                    ctx.enable_command(Command::ZOOM);
                    *enabled.borrow_mut() = true;
                }
            }));
            let id = program.group_mut().insert(Box::new(probe));
            program.with_ctx(|g, ctx| g.set_current(Some(id), SelectMode::Normal, ctx));
        }

        // Send a key the probe reacts to; the enable is deferred and applied AFTER
        // dispatch (mirrors pending_captures).
        program.out_events.clear();
        program.out_events.push_back(key(Key::Char('z')));
        program.pump_once();
        assert!(
            program.command_enabled(Command::ZOOM),
            "deferred enable_command applied after dispatch"
        );

        // A previously-filtered cmZoom now reaches routing: send it and confirm the
        // probe (current child) records it (it is no longer dropped at the program
        // boundary).
        log.borrow_mut().clear();
        program.out_events.push_back(Event::Command(Command::ZOOM));
        program.pump_once();
        assert!(
            log.borrow().contains(&Event::Command(Command::ZOOM)),
            "now-enabled command reaches routing instead of being filtered"
        );
    }

    // -- 10. drag move round-trip (33d-1, mandatory) -------------------------

    use crate::view::StateFlag;
    use crate::window::Window;

    /// Read a window's `ViewState` by resolving its id through the root group.
    fn win_state(program: &mut Program, id: ViewId) -> ViewState {
        program.group_mut().find_mut(id).unwrap().state().clone()
    }

    /// End-to-end drag: MouseDown(title) → MouseMove×2 → MouseUp, driven through
    /// `pump_once`. Proves the deferred round-trip: capture consumes the
    /// `MouseMove`, `request_bounds` queues a `Deferred`, the loop drains it and
    /// applies `change_bounds`, and `MouseUp` clears `sfDragging` + pops the
    /// capture (the deferred SetState applied).
    #[test]
    fn drag_move_round_trip() {
        let (mut program, _screen, _clock) = program_with_desktop(80, 25);
        // Insert a wfMove window into the ROOT group at (2,1,22,9) and select it.
        let id = {
            let w = Window::new(Rect::new(2, 1, 22, 9), Some("Edit".into()), 1);
            program.group_mut().insert(Box::new(w))
        };
        program.with_ctx(|g, ctx| {
            g.set_current(Some(id), SelectMode::Normal, ctx);
            g.find_mut(id)
                .unwrap()
                .set_state(StateFlag::Selected, true, ctx);
        });
        program.out_events.clear();

        // MouseDown on the title bar: absolute (8,1) → window-local (6,0).
        program.out_events.push_back(mouse_down_at(8, 1));
        program.pump_once();
        let st = win_state(&mut program, id);
        assert!(st.state.dragging, "drag started: sfDragging set");
        assert_eq!(program.capture_len(), 1, "DragCapture pushed (deferred)");

        // The Move anchor: new_origin = mouse_abs - mouse_local_down. mouse_local
        // down = (6,0), so origin = mouse_abs - (6,0).
        // MouseMove to absolute (12,4) → expected origin (6,4).
        program.out_events.push_back(mouse_move_at(12, 4));
        program.pump_once();
        let st = win_state(&mut program, id);
        assert_eq!(st.origin, Point::new(6, 4), "window tracked the first move");

        // Second MouseMove to absolute (20,8) → expected origin (14,8).
        program.out_events.push_back(mouse_move_at(20, 8));
        program.pump_once();
        let st = win_state(&mut program, id);
        assert_eq!(
            st.origin,
            Point::new(14, 8),
            "window tracked the second move"
        );

        // MouseUp ends the drag: sfDragging cleared, capture popped.
        program.out_events.push_back(mouse_up_at(20, 8));
        program.pump_once();
        let st = win_state(&mut program, id);
        assert!(!st.state.dragging, "drag ended: sfDragging cleared");
        assert_eq!(program.capture_len(), 0, "DragCapture popped on MouseUp");
    }

    // -- 11. drag clamps to limits -------------------------------------------

    /// Dragging the title to a position whose raw `origin.y` would be negative is
    /// pinned to 0 by `dmLimitLoY`.
    ///
    /// Window: origin=(2,1), size=(20,8).  Grab title at window-local (6,0) →
    /// absolute (8,1).  Anchor for a Move drag = origin − mouse_abs = (2,1)−(8,1)
    /// = (−6, 0).  MouseMove to absolute (0,−5): raw new_origin.y = −5 + 0 = −5.
    /// General band: (−5).max(0 − 8 + 1) = (−5).max(−7) = −5 (survives the band).
    /// Without `dmLimitLoY` origin.y would be −5; WITH it the clamp pins it to 0.
    #[test]
    fn drag_move_clamps_to_limits() {
        let (mut program, _screen, _clock) = program_with_desktop(80, 25);
        let id = {
            let w = Window::new(Rect::new(2, 1, 22, 9), Some("Edit".into()), 1);
            program.group_mut().insert(Box::new(w))
        };
        program.with_ctx(|g, ctx| g.set_current(Some(id), SelectMode::Normal, ctx));
        program.out_events.clear();

        // Grab the title at window-local (6,0): absolute (8,1).
        // anchor.y = origin.y − mouse_abs.y = 1 − 1 = 0.
        program.out_events.push_back(mouse_down_at(8, 1));
        program.pump_once();
        assert!(win_state(&mut program, id).state.dragging);

        // Move to absolute (0,−5): raw new_origin.y = −5 + 0 = −5, which survives
        // the general band (−7 ≤ −5) but is negative.  `dmLimitLoY` must pin it
        // to 0; without that clamp origin.y would be −5 and the test would fail.
        program.out_events.push_back(mouse_move_at(0, -5));
        program.pump_once();
        let st = win_state(&mut program, id);
        assert_eq!(
            st.origin.y, 0,
            "dmLimitLoY must pin origin.y to 0, got {}",
            st.origin.y
        );
        // General band keeps origin.x within [a−s+1, b−1] = [−19, 79].
        let size_x = st.size.x;
        assert!(
            st.origin.x > -size_x && st.origin.x < 80,
            "origin.x within [a-s+1, b-1], got {}",
            st.origin.x
        );
    }

    // -- 12. close round-trip ------------------------------------------------

    /// `cmClose` on a `wfClose` window removes it from the tree (the deferred
    /// `request_close` → `remove_descendant` round-trip). A `sfModal` window
    /// instead posts `cmCancel` and is NOT removed.
    #[test]
    fn close_round_trip_removes_window() {
        let (mut program, _screen, _clock) = program_with_desktop(80, 25);
        let id = {
            let w = Window::new(Rect::new(2, 1, 22, 9), Some("Edit".into()), 1);
            program.group_mut().insert(Box::new(w))
        };
        // Select it (enables cmClose via the command-change channel) + make current
        // so the focused cmClose routes to it.
        program.with_ctx(|g, ctx| {
            g.set_current(Some(id), SelectMode::Normal, ctx);
            g.find_mut(id)
                .unwrap()
                .set_state(StateFlag::Selected, true, ctx);
        });
        // Apply the deferred enable (it sits in pending_command_changes until a
        // dispatch drains it); just enable directly to be unambiguous.
        program.enable_command(Command::CLOSE);
        program.out_events.clear();

        assert!(
            program.group_mut().find_mut(id).is_some(),
            "window present before close"
        );
        program.out_events.push_back(Event::Command(Command::CLOSE));
        program.pump_once();
        assert!(
            program.group_mut().find_mut(id).is_none(),
            "window removed via remove_descendant after cmClose"
        );
    }

    // -- 13. exec_view modal round-trips (row 34, FOUNDATION gate) -----------

    use crate::dialog::Dialog;

    /// `exec_view` full round-trip: pre-queue `cmOK`, run a `Dialog` modally, and
    /// assert it returns `Command::OK`. The trace: `exec_view` inserts + selects +
    /// pushes the frame + enters the loop -> pump 1 pops the queued `cmOK` -> routes
    /// to the current (modal) dialog -> `end_modal(OK)` deferred -> the pump applies
    /// it -> `end_state = Some(OK)` -> the inner loop exits -> `valid(OK)` true ->
    /// returns OK. Post-conditions: the frame was popped (`capture_len == 0`), the
    /// dialog was removed (the root child count returned to its pre-exec value), and
    /// `current` was restored to the saved value (the desktop).
    #[test]
    fn exec_view_returns_ok_via_queued_command() {
        let (mut program, _screen, _clock) = program_with_desktop(40, 12);
        let children_before = program.group_mut().len();
        let current_before = program.group_mut().current();
        assert_eq!(program.capture_len(), 0);

        // Pre-queue cmOK BEFORE exec_view: it sits ahead of the set_current focus
        // broadcasts, so pump 1 consumes it and routes it to the modal dialog.
        program.out_events.push_back(Event::Command(Command::OK));

        let dialog = Dialog::new(Rect::new(4, 2, 36, 10), Some("Setup".into()));
        let result = program.exec_view(Box::new(dialog));

        assert_eq!(
            result,
            Command::OK,
            "exec_view returns the modal end command"
        );
        assert_eq!(program.capture_len(), 0, "ModalFrame popped after the loop");
        assert_eq!(
            program.group_mut().len(),
            children_before,
            "dialog removed: child count restored"
        );
        assert_eq!(
            program.group_mut().current(),
            current_before,
            "current restored to the saved value (the desktop)"
        );
    }

    /// `exec_view` returns `cmCancel` via Esc: pre-queue an Esc `KeyDown`. The
    /// dialog turns it into a posted `cmCancel` (a later pump consumes that), so
    /// the modal ends with cmCancel. Multiple pumps — still hang-safe because an
    /// end-command is always in flight once the Esc is processed.
    #[test]
    fn exec_view_returns_cancel_via_esc() {
        let (mut program, _screen, _clock) = program_with_desktop(40, 12);

        // Pre-queue Esc: pump 1 routes it to the dialog -> posts cmCancel; a later
        // pump routes cmCancel -> end_modal(Cancel) -> exits.
        program.out_events.push_back(key(Key::Esc));

        let dialog = Dialog::new(Rect::new(4, 2, 36, 10), Some("Setup".into()));
        let result = program.exec_view(Box::new(dialog));

        assert_eq!(result, Command::CANCEL, "Esc -> cmCancel ends the modal");
        assert_eq!(program.capture_len(), 0, "ModalFrame popped");
    }

    /// `cmQuit` during a modal (the non-obvious edge). Inside the modal,
    /// `Event::Command(Command::QUIT)` is caught by `program_handle_event` ->
    /// `end_state = Some(QUIT)`. The inner loop exits, `valid_end(QUIT)` ->
    /// `group.valid(QUIT)` -> true (the dialog's `valid` defers to the group, no
    /// child vetoes QUIT), so `exec_view` returns `QUIT` and pops the frame.
    ///
    /// **This asserts a DELIBERATE D9 DEVIATION, not faithful C++ behavior.** Under
    /// our single loop, `program_handle_event` (the `cmQuit` catch) runs during the
    /// modal pump, so `cmQuit` ends the modal with `QUIT`. In C++,
    /// `TGroup::execView` → `p->execute()` (`tgroup.cpp:205`) dispatches to the
    /// **dialog's** `handleEvent`, so the `cmQuit → endModal` catch in
    /// `TProgram::handleEvent` (`tprogram.cpp:205`) is out of the modal dispatch
    /// path — there `cmQuit` reaches the dialog, goes unhandled, is discarded, and
    /// the modal STAYS OPEN. We keep our behavior (see `exec_view`'s doc); the
    /// assertions below verify it (no hang, no panic, frame popped).
    #[test]
    fn exec_view_cm_quit_ends_modal_deviation_from_cpp() {
        let (mut program, _screen, _clock) = program_with_desktop(40, 12);

        program.out_events.push_back(Event::Command(Command::QUIT));

        let dialog = Dialog::new(Rect::new(4, 2, 36, 10), Some("Setup".into()));
        let result = program.exec_view(Box::new(dialog));

        assert_eq!(
            result,
            Command::QUIT,
            "cmQuit during a modal ends the modal with cmQuit (caller propagates)"
        );
        assert_eq!(program.capture_len(), 0, "ModalFrame popped on cmQuit");
    }

    /// A sibling view in the ROOT group whose `valid` vetoes one specific command
    /// (and is otherwise valid). Used to prove `exec_view`'s outer validation is
    /// scoped to the MODAL view, not the root group (a root-scoped check would also
    /// consult this sibling).
    struct VetoView {
        st: ViewState,
        veto: Command,
    }
    impl View for VetoView {
        fn state(&self) -> &ViewState {
            &self.st
        }
        fn state_mut(&mut self) -> &mut ViewState {
            &mut self.st
        }
        fn draw(&mut self, _ctx: &mut DrawCtx) {}
        fn valid(&self, cmd: Command) -> bool {
            cmd != self.veto
        }
    }

    /// DISCRIMINATING (Fix 1): `exec_view`'s outer `while(!valid)` validates the
    /// MODAL view's own `valid` (TDialog::valid, scoped to the dialog's children) —
    /// NOT the root group's `valid` (which would also consult the desktop's
    /// siblings). We insert a sibling into the ROOT group whose `valid` vetoes
    /// `cmOK`, then run a dialog modally that ends with `cmOK`. The dialog's own
    /// `valid(cmOK)` is true (no validating children), so `exec_view` returns OK.
    ///
    /// **Bite verification (no CI-hang risk):** with the BUGGY `self.group.valid(es)`,
    /// the sibling's `cmOK` veto makes the root `valid(cmOK)` false, the outer loop
    /// re-spins with `end_state = None` and nothing queued, and the inner `while`
    /// HANGS — so we cannot run the buggy code under CI. The bite was confirmed
    /// MANUALLY by temporarily reverting Fix 1 to `self.valid_end(es)` and adding a
    /// bounded outer-iteration guard (max 3 spins -> panic): under the bug the guard
    /// fired (re-spin observed); with the fix the loop breaks on the first pass.
    /// The guard + revert were removed after confirming; the committed test asserts
    /// only the fixed behavior (returns OK despite the sibling veto).
    #[test]
    fn exec_view_outer_valid_scopes_to_modal_not_root_group() {
        let (mut program, _screen, _clock) = program_with_desktop(40, 12);
        // Sibling in the ROOT group that vetoes cmOK (and only cmOK).
        {
            let sibling = VetoView {
                st: ViewState::new(Rect::new(0, 0, 2, 2)),
                veto: Command::OK,
            };
            program.group_mut().insert(Box::new(sibling));
        }
        // Pre-queue cmOK so the modal ends with OK.
        program.out_events.push_back(Event::Command(Command::OK));

        let dialog = Dialog::new(Rect::new(4, 2, 36, 10), Some("Setup".into()));
        let result = program.exec_view(Box::new(dialog));

        assert_eq!(
            result,
            Command::OK,
            "exec_view validates the MODAL view's own valid (dialog: OK ok), \
             NOT the root group (where the sibling vetoes OK) — the sibling's \
             veto must NOT keep the loop spinning"
        );
        assert_eq!(program.capture_len(), 0, "ModalFrame popped");
    }

    /// A validator that rejects every final value (`is_valid` → false). Attached
    /// to a dialog's `InputLine`, it makes `InputLine::valid(cmOK)` false.
    struct RejectAll;
    impl crate::validate::Validator for RejectAll {
        fn is_valid(&self, _s: &str) -> bool {
            false
        }
    }

    /// CROSS-ROW (the reviewer's gap): the **headline** behavior of
    /// `TInputLine::valid()` end-to-end — a modal dialog must **not** close on OK
    /// while a child input line's validator rejects, but must close on Cancel.
    ///
    /// Isolated tests only call `InputLine::valid()` directly; the actual veto
    /// lives in `exec_view`'s outer `while !valid(end_state)` loop (faithful to
    /// `TGroup::execute`). The trace this proves:
    /// - pump #1: queued `cmOK` → `Dialog::handle_event` → `end_modal(OK)` → the
    ///   pump sets `end_state = Some(OK)` → the inner loop exits → the outer loop
    ///   checks the MODAL view's `valid(OK)` → `Dialog::valid` → `Window::valid`
    ///   → `Group::valid` (cmOK ≠ cmReleasedFocus, so `children.all(valid)`) →
    ///   `InputLine::valid(OK)` runs the validator → **false** → the modal stays
    ///   open (the loop re-spins with `end_state = None`).
    /// - pump #2: queued `cmCancel` → `Dialog::handle_event` → `end_modal(CANCEL)`
    ///   → outer-loop `valid(CANCEL)` → `Dialog::valid` short-circuits cmCancel →
    ///   **true** → break → `exec_view` returns `cmCancel`.
    ///
    /// We queue `[cmOK, cmCancel]` precisely because `[cmOK]` alone would hang
    /// forever (a permanently-rejecting field can never close — that IS the
    /// faithful behavior). Asserting `cmCancel` (NOT `cmOK`) proves the cmOK
    /// end-state was vetoed and only the un-vetoable Cancel ended the modal.
    ///
    /// The InputLine is inserted but NOT made the dialog's `current`: `Group::valid`
    /// for any non-`cmReleasedFocus` command walks ALL children unconditionally
    /// (`group.rs`: `children.iter().all(|c| c.view.valid(cmd))`), so the veto holds
    /// regardless of focus — the "focused child" framing is setup flavor, and there
    /// is no clean seam to make a dialog child current here. Omission is deliberate.
    ///
    /// BITE-VERIFIED (manually, documented — no source edit needed): swapping the
    /// validator to `None` (accept-all) makes `InputLine::valid(OK)` true, so pump
    /// #1's `dialog.valid(OK)` is true, the outer loop breaks on the FIRST pass, and
    /// `exec_view` returns `Command::OK` (never reaching the queued cmCancel). I ran
    /// that variant locally and observed `result == Command::OK` — proving (a) cmOK
    /// is genuinely processed and reaches end-modal (not silently dropped, else the
    /// accept-all run would also fall through to cmCancel), and (b) the validator is
    /// the sole thing flipping OK→vetoed here. The committed test keeps `RejectAll`
    /// and asserts the CANCEL outcome.
    #[test]
    fn exec_view_ok_vetoed_by_rejecting_input_line_cancel_closes() {
        use crate::widgets::{InputLine, LimitMode};

        let (mut program, _screen, _clock) = program_with_desktop(40, 12);

        let mut dialog = Dialog::new(Rect::new(4, 2, 36, 10), Some("Setup".into()));
        // A child input line whose validator rejects every final value, with some
        // data so it is a realistic "user typed something invalid" field.
        let mut input = InputLine::new(
            Rect::new(2, 2, 28, 3),
            256,
            Some(Box::new(RejectAll)),
            LimitMode::MaxBytes,
        );
        input.data = "bad".to_string();
        dialog.insert_child(Box::new(input));

        // Pre-queue cmOK THEN cmCancel: pump #1 routes cmOK -> end_modal(OK) ->
        // outer valid(OK) vetoed by the field -> reopen; pump #2 routes cmCancel ->
        // end_modal(CANCEL) -> valid(CANCEL) always true -> break.
        program.out_events.push_back(Event::Command(Command::OK));
        program
            .out_events
            .push_back(Event::Command(Command::CANCEL));

        let result = program.exec_view(Box::new(dialog));

        assert_eq!(
            result,
            Command::CANCEL,
            "OK must NOT close the modal while the input line's validator rejects; \
             only the un-vetoable Cancel ends it"
        );
        assert_eq!(program.capture_len(), 0, "ModalFrame popped after the loop");
    }

    /// A `sfModal` window posts `cmCancel` on `cmClose` and is NOT removed (row 34
    /// owns the actual modal teardown; only this branch is wired in 33d-1).
    #[test]
    fn close_modal_window_posts_cancel_not_removed() {
        let (mut program, _screen, _clock) = program_with_desktop(80, 25);
        let id = {
            let mut w = Window::new(Rect::new(2, 1, 22, 9), Some("Edit".into()), 1);
            w.state_mut().state.modal = true;
            program.group_mut().insert(Box::new(w))
        };
        program.with_ctx(|g, ctx| g.set_current(Some(id), SelectMode::Normal, ctx));
        program.enable_command(Command::CLOSE);
        program.out_events.clear();

        program.out_events.push_back(Event::Command(Command::CLOSE));
        program.pump_once();
        assert!(
            program.group_mut().find_mut(id).is_some(),
            "modal window NOT removed on cmClose"
        );
        assert!(
            program
                .out_events
                .iter()
                .any(|e| *e == Event::Command(Command::CANCEL)),
            "modal cmClose posts cmCancel"
        );
    }

    /// Regression: a modal frame must follow its dialog when it is dragged, so a
    /// SECOND mouse interaction on the moved dialog is not swallowed by a stale
    /// gate. Replicates the `exec_view` modal setup, drags the dialog far to the
    /// right, then attempts a second drag whose grab point is on the MOVED title
    /// but OUTSIDE the original (push-time) bounds — which the unfixed
    /// `ModalFrame` swallowed (`capture_len` stuck at 1).
    #[test]
    fn modal_frame_follows_dragged_dialog() {
        let (mut program, _screen, _clock) = program_with_desktop(100, 30);
        // Original bounds (27,8)-(73,22).
        let id = {
            let w = Dialog::new(Rect::new(27, 8, 73, 22), Some("About".into()));
            program.group_mut().insert(Box::new(w))
        };
        // Replicate exec_view modal setup (steps 3-6).
        if let Some(v) = program.group_mut().find_mut(id) {
            let st = v.state_mut();
            st.options.selectable = false;
            st.state.modal = true;
        }
        program.with_ctx(|g, ctx| g.set_current(Some(id), SelectMode::Enter, ctx));
        let bounds = program
            .group_mut()
            .find_mut(id)
            .unwrap()
            .state()
            .get_bounds();
        program.captures.push(Box::new(ModalFrame::new(id, bounds)));
        program.out_events.clear();

        // First drag: grab the title at abs (44,8), move right to (54,8), release.
        program.out_events.push_back(mouse_down_at(44, 8));
        program.pump_once();
        assert_eq!(program.capture_len(), 2, "first drag started");
        for x in [46, 48, 50, 52, 54] {
            program.out_events.push_back(mouse_move_at(x, 8));
            program.pump_once();
        }
        program.out_events.push_back(mouse_up_at(54, 8));
        program.pump_once();
        assert_eq!(
            program.capture_len(),
            1,
            "first drag ended (only ModalFrame)"
        );
        // Dialog moved +10 right: new bounds (37,8)-(83,22).
        assert_eq!(win_state(&mut program, id).origin, Point::new(37, 8));

        // Second drag: grab the MOVED title at abs (80,8) — inside the new bounds
        // (37..83) but OUTSIDE the original (27..73). The fixed gate must let this
        // through so a second DragCapture is pushed (capture_len 2). Pre-fix this
        // was swallowed (capture_len stayed 1).
        program.out_events.push_back(mouse_down_at(80, 8));
        program.pump_once();
        assert_eq!(
            program.capture_len(),
            2,
            "second drag on the moved dialog must start (modal frame followed the move)"
        );
        assert!(
            win_state(&mut program, id).state.dragging,
            "second drag set sfDragging"
        );
    }

    // -- row 27: TScroller cross-view broker (pump-side apply) ----------------
    //
    // These drive the broker end-to-end through `pump_once`: the scroller and its
    // two bars are inserted into the ROOT group (so the pump's `group.find_mut`
    // resolves all three), and the deferred `SyncScrollerDelta` /
    // `ScrollBarSetParams` / `SetVisible` ops are applied by the real apply loop.

    use crate::widgets::{ScrollBar, Scroller};

    /// Insert an h-bar, a v-bar, and a scroller into the program's root group.
    /// Returns `(h_id, v_id, scroller_id)`. The scroller is not made current — the
    /// tests address it / the bars by id directly.
    fn insert_scroller(program: &mut Program) -> (ViewId, ViewId, ViewId) {
        let g = program.group_mut();
        // Horizontal bar 20×1, vertical bar 1×10.
        let h = g.insert(Box::new(ScrollBar::new(Rect::new(0, 24, 20, 25))));
        let v = g.insert(Box::new(ScrollBar::new(Rect::new(79, 0, 80, 10))));
        // Scroller 10×5.
        let s = g.insert(Box::new(Scroller::new(
            Rect::new(0, 0, 10, 5),
            Some(h),
            Some(v),
        )));
        program.out_events.clear();
        (h, v, s)
    }

    fn scroller_delta(program: &mut Program, id: ViewId) -> Point {
        program
            .group_mut()
            .find_mut(id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<Scroller>())
            .map(|s| s.delta)
            .expect("scroller resolves")
    }

    fn bar_params(program: &mut Program, id: ViewId) -> (i32, i32, i32, i32, i32) {
        program
            .group_mut()
            .find_mut(id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<ScrollBar>())
            .map(|b| (b.value, b.min_value, b.max_value, b.page_step, b.arrow_step))
            .expect("scrollbar resolves")
    }

    fn set_bar_value(program: &mut Program, id: ViewId, value: i32) {
        // Give the bar a real range first, then set its value (through the pump's
        // own deferred channel would be circular; set directly for setup).
        let g = program.group_mut();
        let b = g
            .find_mut(id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<ScrollBar>())
            .expect("scrollbar resolves");
        b.min_value = 0;
        b.max_value = 100;
        b.value = value;
    }

    /// Read broker (#2): a `cmScrollBarChanged` broadcast whose `source` is the
    /// scroller's h-bar makes the pump read that bar's `value` and update the
    /// scroller's `delta` — and a broadcast from a NON-bar source is ignored.
    #[test]
    fn scroller_read_broker_syncs_delta_through_pump() {
        let (mut program, _h2, _c) = program_with_desktop(80, 25);
        let (h, v, s) = insert_scroller(&mut program);

        // Pre-set the H bar's value (the broker reads `value` off the resolved bar).
        set_bar_value(&mut program, h, 7);
        // V bar stays value 0.
        set_bar_value(&mut program, v, 0);

        assert_eq!(scroller_delta(&mut program, s), Point::new(0, 0));

        // Inject the CHANGED broadcast sourced by the H bar and pump once: the
        // broadcast phase delivers it to the scroller (which queues
        // SyncScrollerDelta), then the apply loop reads the bars and pushes the
        // delta into the scroller.
        program.out_events.push_back(Event::Broadcast {
            command: Command::SCROLL_BAR_CHANGED,
            source: Some(h),
        });
        program.pump_once();
        assert_eq!(
            scroller_delta(&mut program, s),
            Point::new(7, 0),
            "delta.x mirrors the H bar's value; delta.y mirrors the V bar (0)"
        );

        // Now move the V bar and fire a CHANGED sourced by V.
        set_bar_value(&mut program, v, 3);
        program.out_events.push_back(Event::Broadcast {
            command: Command::SCROLL_BAR_CHANGED,
            source: Some(v),
        });
        program.pump_once();
        assert_eq!(scroller_delta(&mut program, s), Point::new(7, 3));

        // Negative case: a CHANGED broadcast from a non-bar source (the scroller's
        // own id) must NOT change the delta (the source filter bites). Move a bar
        // first so a *would-be* sync would be observable.
        set_bar_value(&mut program, h, 42);
        program.out_events.push_back(Event::Broadcast {
            command: Command::SCROLL_BAR_CHANGED,
            source: Some(s),
        });
        program.pump_once();
        assert_eq!(
            scroller_delta(&mut program, s),
            Point::new(7, 3),
            "broadcast from a non-bar source must leave delta unchanged"
        );
    }

    /// Write broker (#3): `Scroller::set_limit` queues `ScrollBarSetParams`, and the
    /// pump applies them — setting each bar's range/page while PRESERVING its value
    /// and arrow step.
    #[test]
    fn scroller_set_limit_write_broker_through_pump() {
        let (mut program, _h, _c) = program_with_desktop(80, 25);
        let (h, v, s) = insert_scroller(&mut program);

        // Give the bars distinct live value + arrow_step so "preserve" is testable.
        {
            let g = program.group_mut();
            for id in [h, v] {
                let b = g
                    .find_mut(id)
                    .and_then(|x| x.as_any_mut())
                    .and_then(|a| a.downcast_mut::<ScrollBar>())
                    .unwrap();
                b.min_value = 0;
                b.max_value = 1000; // wide enough that value 4 stays in range
                b.value = 4;
                b.arrow_step = 9;
            }
        }

        // Drive set_limit through a dispatch: queue the broker request exactly as a
        // subclass would from handle_event, alongside a benign broadcast to reach the
        // apply loop. We call set_limit via a temporary Context-bearing path: push the
        // deferred ops by reaching the scroller and invoking set_limit with a Context.
        // Simplest: queue them directly the way the scroller would, by calling
        // set_limit on the resolved scroller against a throwaway Context whose
        // `deferred` is then merged — instead, drive it the production way:
        {
            // Resolve the scroller and call set_limit with a real Context that writes
            // into the program's deferred queue, then pump to apply.
            let Program {
                group,
                out_events,
                timers,
                deferred,
                ..
            } = &mut program;
            let mut ctx = Context::new(out_events, timers, 0, deferred);
            let sc = group
                .find_mut(s)
                .and_then(|x| x.as_any_mut())
                .and_then(|a| a.downcast_mut::<Scroller>())
                .unwrap();
            sc.set_limit(100, 50, &mut ctx); // size 10×5
        }
        // A benign broadcast drives a dispatch so the apply loop runs.
        program.out_events.push_back(Event::Broadcast {
            command: Command::custom("test.noop"),
            source: None,
        });
        program.pump_once();

        // H bar: value 4 preserved, min 0, max 100-10=90, page_step 10-1=9,
        //        arrow_step 9 preserved.
        let (hv, hmin, hmax, hpg, har) = bar_params(&mut program, h);
        assert_eq!(hv, 4, "H value preserved");
        assert_eq!(hmin, 0);
        assert_eq!(hmax, 90, "H max = x - size.x");
        assert_eq!(hpg, 9, "H page_step = size.x - 1");
        assert_eq!(har, 9, "H arrow_step preserved");

        // V bar: max 50-5=45, page_step 5-1=4.
        let (vv, _vmin, vmax, vpg, var) = bar_params(&mut program, v);
        assert_eq!(vv, 4, "V value preserved");
        assert_eq!(vmax, 45, "V max = y - size.y");
        assert_eq!(vpg, 4, "V page_step = size.y - 1");
        assert_eq!(var, 9, "V arrow_step preserved");
    }

    /// Write broker (#4): `Scroller::scroll_to` sets each bar's value (clamped to
    /// the live range), preserving range and steps.
    #[test]
    fn scroller_scroll_to_write_broker_through_pump() {
        let (mut program, _h, _c) = program_with_desktop(80, 25);
        let (h, v, s) = insert_scroller(&mut program);

        // Bars with range [0, 8] so scroll_to(10, 5) clamps the H value to 8.
        {
            let g = program.group_mut();
            for id in [h, v] {
                let b = g
                    .find_mut(id)
                    .and_then(|x| x.as_any_mut())
                    .and_then(|a| a.downcast_mut::<ScrollBar>())
                    .unwrap();
                b.min_value = 0;
                b.max_value = 8;
                b.value = 0;
            }
        }

        {
            let Program {
                group,
                out_events,
                timers,
                deferred,
                ..
            } = &mut program;
            let mut ctx = Context::new(out_events, timers, 0, deferred);
            let sc = group
                .find_mut(s)
                .and_then(|x| x.as_any_mut())
                .and_then(|a| a.downcast_mut::<Scroller>())
                .unwrap();
            sc.scroll_to(10, 5, &mut ctx);
        }
        program.out_events.push_back(Event::Broadcast {
            command: Command::custom("test.noop"),
            source: None,
        });
        program.pump_once();

        let (hv, _, hmax, _, _) = bar_params(&mut program, h);
        assert_eq!(hmax, 8, "H range preserved");
        assert_eq!(hv, 8, "H value clamped to max (scroll_to 10 > 8)");
        let (vv, _, _, _, _) = bar_params(&mut program, v);
        assert_eq!(vv, 5, "V value set to 5 (in range)");
    }

    /// Visibility broker (#5): selecting/deselecting the scroller shows/hides both
    /// bars through the deferred `SetVisible` ops applied by the pump.
    #[test]
    fn scroller_set_state_shows_and_hides_bars_through_pump() {
        let (mut program, _h, _c) = program_with_desktop(80, 25);
        let (h, v, s) = insert_scroller(&mut program);

        let visible = |program: &mut Program, id: ViewId| {
            program
                .group_mut()
                .find_mut(id)
                .map(|x| x.state().state.visible)
                .unwrap()
        };

        // Drive set_state(Selected, true) through a dispatch + apply.
        {
            let Program {
                group,
                out_events,
                timers,
                deferred,
                ..
            } = &mut program;
            let mut ctx = Context::new(out_events, timers, 0, deferred);
            if let Some(sc) = group.find_mut(s) {
                sc.set_state(crate::view::StateFlag::Selected, true, &mut ctx);
            }
        }
        program.out_events.push_back(Event::Broadcast {
            command: Command::custom("test.noop"),
            source: None,
        });
        program.pump_once();
        assert!(
            visible(&mut program, h),
            "H bar shown when scroller selected"
        );
        assert!(
            visible(&mut program, v),
            "V bar shown when scroller selected"
        );

        // Deselect → both hidden.
        {
            let Program {
                group,
                out_events,
                timers,
                deferred,
                ..
            } = &mut program;
            let mut ctx = Context::new(out_events, timers, 0, deferred);
            if let Some(sc) = group.find_mut(s) {
                sc.set_state(crate::view::StateFlag::Selected, false, &mut ctx);
            }
        }
        program.out_events.push_back(Event::Broadcast {
            command: Command::custom("test.noop"),
            source: None,
        });
        program.pump_once();
        assert!(
            !visible(&mut program, h),
            "H bar hidden when scroller deselected"
        );
        assert!(
            !visible(&mut program, v),
            "V bar hidden when scroller deselected"
        );
    }

    // -- row 28: TListViewer read-sync broker + the TERMINATION property -------
    //
    // The list-viewer read-sync WRITES BACK (focus_item_num -> focusItem -> a
    // deferred v-bar setValue(focused)), unlike the scroller. The cycle
    // (cmScrollBarChanged -> SyncListViewer -> apply_scroll -> setValue ->
    // possible re-broadcast) terminates ONLY because ScrollBar::set_params is
    // change-guarded (re-broadcasts SCROLL_BAR_CHANGED solely on an actual value
    // change). These tests drive it through real pump_once drains and assert the
    // cycle goes QUIET while focused/top_item settle correctly.

    use crate::widgets::ListViewerState;
    use crate::widgets::list_viewer;

    /// A minimal concrete `ListViewer` for the pump-level broker tests (the
    /// program-test analogue of `list_viewer`'s `FakeList`, which is private to
    /// that module). Delegates the shared logic to the `list_viewer` free fns.
    struct ProgList {
        lv: ListViewerState,
    }

    impl ProgList {
        fn new(bounds: Rect, num_cols: i32, n: i32, h: Option<ViewId>, v: Option<ViewId>) -> Self {
            let mut lv = ListViewerState::new(bounds, num_cols, h, v);
            lv.range = n;
            ProgList { lv }
        }
    }

    impl View for ProgList {
        fn state(&self) -> &ViewState {
            &self.lv.state
        }
        fn state_mut(&mut self) -> &mut ViewState {
            &mut self.lv.state
        }
        fn draw(&mut self, ctx: &mut DrawCtx) {
            list_viewer::draw(self, ctx);
        }
        fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
            list_viewer::handle_event(self, ev, ctx);
        }
        fn apply_list_scroll(&mut self, h: Option<i32>, v: Option<i32>, ctx: &mut Context) {
            list_viewer::apply_scroll(self, h, v, ctx);
        }
        fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
            Some(self)
        }
    }

    impl list_viewer::ListViewer for ProgList {
        fn lv(&self) -> &ListViewerState {
            &self.lv
        }
        fn lv_mut(&mut self) -> &mut ListViewerState {
            &mut self.lv
        }
        fn get_text(&self, item: i32) -> String {
            format!("row{item}")
        }
    }

    /// Insert an h-bar, a v-bar (with a real range), and a `ProgList` into the
    /// program's root group. Returns `(h_id, v_id, list_id)`.
    fn insert_list(program: &mut Program, n: i32) -> (ViewId, ViewId, ViewId) {
        let (h, v) = {
            let g = program.group_mut();
            let h = g.insert(Box::new(ScrollBar::new(Rect::new(0, 24, 20, 25))));
            let v = g.insert(Box::new(ScrollBar::new(Rect::new(79, 0, 80, 10))));
            (h, v)
        };
        // Give the v-bar a real range [0, n-1] so its value tracks `focused`.
        {
            let g = program.group_mut();
            let b = g
                .find_mut(v)
                .and_then(|x| x.as_any_mut())
                .and_then(|a| a.downcast_mut::<ScrollBar>())
                .unwrap();
            b.min_value = 0;
            b.max_value = n - 1;
            b.value = 0;
        }
        let list = program.group_mut().insert(Box::new(ProgList::new(
            Rect::new(0, 0, 10, 5),
            1,
            n,
            Some(h),
            Some(v),
        )));
        program.out_events.clear();
        program.deferred.clear();
        (h, v, list)
    }

    fn list_focus_top(program: &mut Program, id: ViewId) -> (i32, i32) {
        program
            .group_mut()
            .find_mut(id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<ProgList>())
            .map(|l| (l.lv.focused, l.lv.top_item))
            .expect("list resolves")
    }

    fn bar_value(program: &mut Program, id: ViewId) -> i32 {
        program
            .group_mut()
            .find_mut(id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<ScrollBar>())
            .map(|b| b.value)
            .expect("bar resolves")
    }

    /// A benign broadcast that drives a dispatch (so the pump reaches its
    /// deferred-apply loop) without itself triggering any list/bar reaction — the
    /// faithful stand-in for "the next event after a scroll". Each pass we re-inject
    /// one and then assert nothing of OURS (SCROLL_BAR_CHANGED / SyncListViewer)
    /// re-appears: that is the cycle being QUIET.
    fn noop_broadcast() -> Event {
        Event::Broadcast {
            command: Command::custom("test.noop"),
            source: None,
        }
    }

    /// THE TERMINATION TEST (brief D-D): moving the v-bar to a new value and
    /// firing a `cmScrollBarChanged` drives the read-sync that WRITES BACK
    /// `setValue(focused)`. Because the write-back equals the bar's now-current
    /// value, `set_params`'s change-guard suppresses the re-broadcast, so the
    /// cycle goes QUIET.
    ///
    /// Each subsequent pump is driven by a benign broadcast (the
    /// deferred-apply loop only runs on an event-dispatch — a deferred write-back
    /// is applied by the *next* dispatch, exactly as in production). We assert that
    /// across many such dispatches NO `SCROLL_BAR_CHANGED` is ever produced by the
    /// write-back and NO `SyncListViewer` is re-queued, while focused/top_item
    /// settle to the v-bar's value.
    ///
    /// Bite-check: were `ScrollBar::set_params` NOT change-guarded, applying the
    /// write-back `setValue(8)` would re-broadcast SCROLL_BAR_CHANGED (even with an
    /// unchanged value), the broadcast phase would re-queue SyncListViewer, whose
    /// apply would write back again — forever. The quiet-pump assertions below
    /// would then fire on the first re-broadcast. The guard is the fixed point.
    #[test]
    fn list_viewer_vbar_sync_write_back_terminates() {
        let (mut program, _h2, _c) = program_with_desktop(80, 25);
        let (_h, v, list) = insert_list(&mut program, 20);

        // Move the v-bar to value 8 (in range [0,19]) and fire CHANGED sourced by
        // it — exactly what TScrollBar::handleEvent would do on a user scroll.
        {
            let g = program.group_mut();
            let b = g
                .find_mut(v)
                .and_then(|x| x.as_any_mut())
                .and_then(|a| a.downcast_mut::<ScrollBar>())
                .unwrap();
            b.value = 8;
        }
        program.out_events.push_back(Event::Broadcast {
            command: Command::SCROLL_BAR_CHANGED,
            source: Some(v),
        });

        // Pump #1: broadcast phase delivers CHANGED -> list queues SyncListViewer;
        // apply loop reads the bars and runs apply_scroll -> focus_item_num(8) ->
        // focus_item -> deferred v-bar setValue(8). That setValue lands in
        // `deferred` for the NEXT dispatch.
        program.pump_once();
        let (f1, t1) = list_focus_top(&mut program, list);
        assert_eq!(f1, 8, "focused tracked the v-bar value");
        // size.y=5, numCols=1: focusItem(8) with topItem 0: 8 >= 0+5 ->
        // topItem = 8 - 5 + 1 = 4.
        assert_eq!(t1, 4, "top_item adjusted to keep item 8 visible");

        // Now drive dispatches with benign broadcasts. Pump #2's dispatch applies
        // the deferred setValue(8); the bar's value is ALREADY 8, so set_params's
        // change-guard suppresses the re-broadcast: no new SCROLL_BAR_CHANGED. From
        // there it must stay quiet across many dispatches.
        for pass in 0..6 {
            program.out_events.push_back(noop_broadcast());
            program.pump_once();
            // After the dispatch, the only things in the queues must be unrelated:
            // no SCROLL_BAR_CHANGED re-broadcast, no SyncListViewer re-queue.
            assert!(
                !program.out_events.iter().any(|e| matches!(
                    e,
                    Event::Broadcast { command, .. } if *command == Command::SCROLL_BAR_CHANGED
                )),
                "pass {pass}: no re-broadcast (the change-guard made the cycle quiet)"
            );
            assert!(
                !program
                    .deferred
                    .iter()
                    .any(|d| matches!(d, Deferred::SyncListViewer { .. })),
                "pass {pass}: no SyncListViewer re-queued (cycle terminated)"
            );
            assert_eq!(bar_value(&mut program, v), 8, "pass {pass}: v-bar value 8");
            let (f, t) = list_focus_top(&mut program, list);
            assert_eq!((f, t), (8, 4), "pass {pass}: focused/top_item stable");
        }
    }

    /// After a clamp (v-bar value beyond the LIST range) the brief promises "one
    /// extra round, then quiescent". Drive the v-bar to a value past the list
    /// range; the read-sync clamps `focused` to range-1 and writes THAT back (a
    /// real change → exactly one corrective broadcast), after which it is quiet.
    #[test]
    fn list_viewer_vbar_sync_clamps_then_terminates() {
        let (mut program, _h2, _c) = program_with_desktop(80, 25);
        let (_h, v, list) = insert_list(&mut program, 20);

        // Widen the v-bar range so it can HOLD a value (99) past the LIST range
        // (20) — the clamp happens inside focus_item_num, not the bar.
        {
            let g = program.group_mut();
            let b = g
                .find_mut(v)
                .and_then(|x| x.as_any_mut())
                .and_then(|a| a.downcast_mut::<ScrollBar>())
                .unwrap();
            b.max_value = 999;
            b.value = 99;
        }
        program.out_events.push_back(Event::Broadcast {
            command: Command::SCROLL_BAR_CHANGED,
            source: Some(v),
        });

        // Drive dispatches (each via a benign broadcast so the deferred write-back
        // gets applied). The clamp + write-back of range-1 (19) settles within a
        // few corrective rounds, then quiesces.
        for _ in 0..8 {
            program.out_events.push_back(noop_broadcast());
            program.pump_once();
        }
        let (f, _t) = list_focus_top(&mut program, list);
        assert_eq!(f, 19, "focused clamped to range-1 = 19");
        assert_eq!(
            bar_value(&mut program, v),
            19,
            "v-bar value corrected to 19 (the clamp written back)"
        );
        // Quiet now: more dispatches produce no further SyncListViewer.
        for pass in 0..4 {
            program.out_events.push_back(noop_broadcast());
            program.pump_once();
            assert!(
                !program
                    .deferred
                    .iter()
                    .any(|d| matches!(d, Deferred::SyncListViewer { .. })),
                "pass {pass}: quiescent after the corrective round"
            );
            assert_eq!(
                bar_value(&mut program, v),
                19,
                "pass {pass}: v-bar value stable at 19"
            );
        }
    }

    /// The read broker also refreshes the cached `indent` from the h-bar (the
    /// draw-uses-cached-h-bar-value seam), and a CHANGED from a NON-bar source is
    /// ignored (the source filter, like the scroller).
    #[test]
    fn list_viewer_hbar_sync_updates_indent_and_filters_foreign_source() {
        let (mut program, _h2, _c) = program_with_desktop(80, 25);
        let (h, _v, list) = insert_list(&mut program, 20);

        // Pre-set the h-bar value; fire CHANGED sourced by it.
        {
            let g = program.group_mut();
            let b = g
                .find_mut(h)
                .and_then(|x| x.as_any_mut())
                .and_then(|a| a.downcast_mut::<ScrollBar>())
                .unwrap();
            b.min_value = 0;
            b.max_value = 50;
            b.value = 6;
        }
        program.out_events.push_back(Event::Broadcast {
            command: Command::SCROLL_BAR_CHANGED,
            source: Some(h),
        });
        program.pump_once();
        let indent = program
            .group_mut()
            .find_mut(list)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<ProgList>())
            .map(|l| l.lv.indent)
            .unwrap();
        assert_eq!(indent, 6, "h-bar value cached into indent by the read-sync");

        // A CHANGED from a foreign source (the list's own id) must be ignored.
        program.deferred.clear();
        program.out_events.clear();
        program.out_events.push_back(Event::Broadcast {
            command: Command::SCROLL_BAR_CHANGED,
            source: Some(list),
        });
        program.pump_once();
        assert!(
            !program
                .deferred
                .iter()
                .any(|d| matches!(d, Deferred::SyncListViewer { .. })),
            "foreign-source broadcast ignored (source filter bites)"
        );
    }

    // -- row 49: TMenuView command-graying broker end-to-end -----------------

    /// A concrete, test-only menu view (the FakeList precedent: a *real*
    /// consumer of the broker, not a dead stub). It embeds [`MenuViewState`] and
    /// wires `handle_event` + `update_menu_commands` to the row-49 free
    /// functions, exactly as the row-50/51 menu views will. `as_any_mut` lets the
    /// test observe its menu's regrayed `disabled` flags through the tree.
    struct MenuProbe {
        mv: crate::menu::MenuViewState,
    }

    impl MenuProbe {
        fn new(bounds: Rect, menu: crate::menu::Menu) -> Self {
            MenuProbe {
                mv: crate::menu::MenuViewState::new(ViewState::new(bounds), menu),
            }
        }
        /// The `disabled` flag of the first (command) item — what the broker
        /// regrays.
        fn first_disabled(&self) -> bool {
            match &self.mv.menu.items[0] {
                crate::menu::MenuItem::Command { disabled, .. } => *disabled,
                _ => panic!("items[0] must be a command item"),
            }
        }
    }

    impl View for MenuProbe {
        fn state(&self) -> &ViewState {
            &self.mv.state
        }
        fn state_mut(&mut self) -> &mut ViewState {
            &mut self.mv.state
        }
        fn draw(&mut self, _ctx: &mut DrawCtx) {}
        fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
            crate::menu::menu_view::handle_event(&self.mv, ev, ctx);
        }
        fn update_menu_commands(&mut self, cs: &crate::command::CommandSet) {
            crate::menu::menu_view::update_menu_commands(&mut self.mv.menu, cs);
        }
        fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
            Some(self)
        }
    }

    /// The §2 broker, end-to-end through the real pump: a command-set change
    /// broadcasts `cmCommandSetChanged`, which reaches the menu view, which
    /// requests `UpdateMenu`, which the pump applies → the menu item regrays.
    ///
    /// **Discriminating** (per the brief): we first ENABLE the command and prove
    /// the item reads *enabled*, then DISABLE it and prove it reads *disabled* —
    /// so a pass cannot come from the command merely never being in the default
    /// set. It passes ONLY via the broadcast → request → regray path; remove the
    /// broker arm (or the request) and the item never flips.
    #[test]
    fn command_set_change_regrays_menu_through_pump() {
        let cmd = Command::custom("test.menu_probe_cmd");
        let menu = crate::menu::Menu::builder()
            .command_key(
                "~P~robe",
                cmd,
                KeyEvent::new(Key::F(9), KeyModifiers::default()),
                "F9",
            )
            .build();

        let (mut program, _screen, _clock) = program_with_desktop(40, 10);
        let probe_id = program
            .group_mut()
            .insert(Box::new(MenuProbe::new(Rect::new(0, 0, 40, 1), menu)));
        program.out_events.clear();

        // Pump until idle settles so any pre-existing command-set churn from
        // insertion clears.
        program.pump_once();

        // Helper: read the probe's first-item disabled flag through the tree.
        fn probe_disabled(p: &mut Program, id: crate::view::ViewId) -> bool {
            p.group_mut()
                .find_mut(id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<MenuProbe>())
                .map(|mp| mp.first_disabled())
                .expect("probe reachable")
        }

        // 1. ENABLE the command → idle pump broadcasts cmCommandSetChanged →
        //    next pump delivers it → probe requests UpdateMenu → apply regrays.
        program.enable_command(cmd);
        program.pump_once(); // idle: emits the broadcast, clears the flag
        program.pump_once(); // delivers the broadcast → probe requests UpdateMenu
        program.pump_once(); // applies UpdateMenu (any residual)
        assert!(
            !probe_disabled(&mut program, probe_id),
            "after ENABLE + regray the item must be ENABLED (disabled == false)"
        );

        // 2. DISABLE the command → same path → the item regrays to disabled.
        program.disable_command(cmd);
        program.pump_once(); // idle: emits the broadcast
        program.pump_once(); // delivers it → probe requests UpdateMenu
        program.pump_once(); // applies UpdateMenu
        assert!(
            probe_disabled(&mut program, probe_id),
            "after DISABLE + regray the item must be DISABLED (disabled == true)"
        );
    }

    /// The passive accelerator path through the real pump: a `KeyDown` matching a
    /// menu item's `key_code` makes the menu view post that command.
    ///
    /// Discriminating in two directions:
    /// - **Enabled** + regrayed → the accelerator posts the command.
    /// - **Disabled** + regrayed → `hot_key`'s cached-`disabled` filter (kept
    ///   current by the §2 broker) skips the item, so **nothing is posted** — the
    ///   primary safety net for the omitted C++ `commandEnabled` re-check. (The
    ///   pump's boundary `drop_disabled` filter is the secondary net for the
    ///   one-idle-cycle staleness window; it is already covered by
    ///   `cm_next_is_dropped_when_disabled`.)
    #[test]
    fn accelerator_key_posts_enabled_command_and_skips_when_regrayed_disabled() {
        let cmd = Command::custom("test.menu_accel_cmd");
        let accel = KeyEvent::new(Key::F(9), KeyModifiers::default());
        let menu = crate::menu::Menu::builder()
            .command_key("~P~robe", cmd, accel, "F9")
            .build();

        let (mut program, _screen, _clock) = program_with_desktop(40, 10);
        {
            let mut probe = MenuProbe::new(Rect::new(0, 0, 40, 1), menu);
            // ofPreProcess so the probe sees the KeyDown regardless of who is
            // current (the desktop is current after startup).
            probe.mv.state.options.pre_process = true;
            program.group_mut().insert(Box::new(probe));
        }
        program.out_events.clear();

        // ENABLE + regray so the cached `disabled` is false, then inject the key.
        program.enable_command(cmd);
        program.pump_once(); // idle: emits cmCommandSetChanged
        program.pump_once(); // delivers it → probe regrays (enabled)
        program.pump_once(); // applies UpdateMenu
        program.out_events.clear();

        program.out_events.push_back(Event::KeyDown(accel));
        program.pump_once();
        assert!(
            program
                .out_events
                .iter()
                .any(|e| matches!(e, Event::Command(c) if *c == cmd)),
            "enabled accelerator posts its command"
        );

        // DISABLE + regray so the cached `disabled` is true, then inject the key:
        // hot_key skips the now-disabled item → nothing is posted.
        program.out_events.clear();
        program.disable_command(cmd);
        program.pump_once(); // idle: emits cmCommandSetChanged
        program.pump_once(); // delivers it → probe regrays (disabled)
        program.pump_once(); // applies UpdateMenu
        program.out_events.clear();

        program.out_events.push_back(Event::KeyDown(accel));
        program.pump_once();
        assert!(
            !program
                .out_events
                .iter()
                .any(|e| matches!(e, Event::Command(c) if *c == cmd)),
            "a regrayed-disabled item's accelerator posts nothing (cached-disabled filter)"
        );
    }

    // -- rows 50-52: the TMenuView modal layer (MenuSession), end-to-end -------

    use crate::command::Command as Cmd;
    use crate::menu::{Menu, MenuBar, MenuBox, alt};

    /// The canonical test bar: File ▸ {Open(cmOpen, accel F3), More ▸
    /// {Refresh(cmRefresh)}}, Edit ▸ {Cut(cmCut)}. Open is File's default
    /// (index 0); More is index 1. Open carries an F3 accelerator (`keyCode`) so
    /// the hotKey-while-open path (`topMenu()->hotKey`) is reachable.
    fn modal_menu() -> Menu {
        Menu::builder()
            .submenu("~F~ile", alt('f'), |m| {
                m.command_key("~O~pen", Cmd::OPEN, KeyEvent::from(Key::F(3)), "F3")
                    .submenu("~M~ore", alt('m'), |s| {
                        s.command("~R~efresh", Cmd::custom("test.refresh"))
                    })
            })
            .submenu("~E~dit", alt('e'), |m| m.command("~C~ut", Cmd::CUT))
            .build()
    }

    /// A program with a desktop AND a real `MenuBar` inserted into the root group.
    /// Returns the program, the bar id, and the child count *before* any menu box
    /// is opened (the baseline a closed session must return to).
    fn program_with_menu_bar(w: u16, h: u16) -> (Program, crate::view::ViewId, usize) {
        let (mut program, _handle, _clock) = program_with_desktop(w, h);
        let bar_id = program.group_mut().insert(Box::new(MenuBar::new(
            Rect::new(0, 0, w as i32, 1),
            modal_menu(),
        )));
        program.out_events.clear();
        let baseline = program.group_mut().len();
        (program, bar_id, baseline)
    }

    /// The topmost `MenuBox` child's highlight `current`, or `None` if no box is
    /// open. The session inserts boxes on top, so the *last* box child is the
    /// active level.
    fn top_box_current(program: &mut Program) -> Option<Option<usize>> {
        let n = program.group_mut().len();
        for idx in (0..n).rev() {
            let st = program.group_mut().child_state_mut(idx);
            let id = st.id();
            if let Some(id) = id
                && let Some(b) = program
                    .group_mut()
                    .find_mut(id)
                    .and_then(|v| v.as_any_mut())
                    .and_then(|a| a.downcast_mut::<MenuBox>())
            {
                return Some(b.current());
            }
        }
        None
    }

    /// The bar's highlight `current`.
    fn bar_current(program: &mut Program, bar_id: crate::view::ViewId) -> Option<usize> {
        program
            .group_mut()
            .find_mut(bar_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<MenuBar>())
            .and_then(|b| b.current())
    }

    /// Drive `cmMenu` then `kbDown` to OPEN File's dropdown (cmMenu alone only
    /// highlights — Blocker 1). After: one box open, File box highlighting Open.
    fn open_file_box(program: &mut Program) {
        program.out_events.push_back(Event::Command(Cmd::MENU));
        program.pump_once(); // highlight File, NO box
        program.out_events.push_back(key(Key::Down));
        program.pump_once(); // bar kbDown → autoSelect → open File box
    }

    /// cmMenu (F10) highlights the default title but opens NO dropdown, and leaves
    /// the session armed (`tmnuview.cpp:193,343-350,368` — the re-posted cmMenu
    /// hits the `evCommand cmMenu` arm, autoSelect stays False, the open-gate is
    /// false). Then a kbDown opens File's box (proving the session is live).
    ///
    /// BITE: restore the old "open the box on cmMenu" behavior (gate `open_submenu`
    /// on `initial` not `open_index`) → after the first pump `group.len()` is
    /// `baseline + 1`, failing the "no box" assert.
    #[test]
    fn f10_highlights_default_without_opening_box() {
        let (mut program, bar_id, baseline) = program_with_menu_bar(40, 12);

        program.out_events.push_back(Event::Command(Cmd::MENU));
        program.pump_once();

        assert_eq!(
            program.group_mut().len(),
            baseline,
            "F10 opens NO dropdown (only highlights)"
        );
        assert_eq!(
            program.capture_len(),
            1,
            "the MenuSession is armed on the capture stack"
        );
        assert_eq!(
            bar_current(&mut program, bar_id),
            Some(0),
            "F10 highlights the default title (File)"
        );
        assert!(
            !program
                .out_events
                .iter()
                .any(|e| matches!(e, Event::Command(_))),
            "F10 posts no command"
        );

        // A subsequent kbDown opens File's box — proves the session is live.
        program.out_events.push_back(key(Key::Down));
        program.pump_once();
        assert_eq!(
            program.group_mut().len(),
            baseline + 1,
            "kbDown after F10 opens File's dropdown"
        );
        assert_eq!(
            top_box_current(&mut program),
            Some(Some(0)),
            "File box highlights its default (Open)"
        );
    }

    /// kbDown moves the open box's highlight (Open idx 0 → More idx 1).
    ///
    /// BITE: a `nextItem` that does not advance (or wraps wrong) leaves `current`
    /// at 0. Asserting exactly 1 pins the move.
    #[test]
    fn arrow_down_moves_box_highlight() {
        let (mut program, _bar_id, _baseline) = program_with_menu_bar(40, 12);
        open_file_box(&mut program);
        assert_eq!(
            top_box_current(&mut program),
            Some(Some(0)),
            "File box starts on Open"
        );

        program.out_events.push_back(key(Key::Down));
        program.pump_once();
        assert_eq!(
            top_box_current(&mut program),
            Some(Some(1)),
            "kbDown moved File box highlight Open(0) → More(1)"
        );
    }

    /// kbEnter on a submenu item (More) opens a NESTED box (the submenu recursion).
    /// After: two boxes open, the nested box highlighting its default (Refresh).
    ///
    /// BITE: if submenu-open did not push a level / queue OpenMenuBox, the group
    /// would not gain a second box.
    #[test]
    fn enter_on_submenu_item_opens_nested_box() {
        let (mut program, _bar_id, baseline) = program_with_menu_bar(40, 12);
        open_file_box(&mut program); // File box (baseline + 1), on Open
        program.out_events.push_back(key(Key::Down));
        program.pump_once(); // highlight More (idx 1)
        assert_eq!(
            top_box_current(&mut program),
            Some(Some(1)),
            "More highlighted"
        );
        assert_eq!(program.group_mut().len(), baseline + 1, "still one box");

        program.out_events.push_back(key(Key::Enter));
        program.pump_once(); // Enter on More → open nested box

        assert_eq!(
            program.group_mut().len(),
            baseline + 2,
            "Enter on the More submenu opened a nested box"
        );
        assert_eq!(
            top_box_current(&mut program),
            Some(Some(0)),
            "nested box highlights its default (Refresh)"
        );
    }

    /// kbEnter on a command item posts that command AND closes ALL boxes (group
    /// back to baseline) — the command-select path.
    ///
    /// BITE: if the command-select arm did not `end_session_with`, the boxes would
    /// stay open (len > baseline) and no command would post.
    #[test]
    fn enter_on_command_posts_and_closes() {
        let (mut program, _bar_id, baseline) = program_with_menu_bar(40, 12);
        open_file_box(&mut program); // File box, highlight Open (idx 0)
        assert_eq!(
            top_box_current(&mut program),
            Some(Some(0)),
            "Open highlighted"
        );

        program.out_events.push_back(key(Key::Enter));
        program.pump_once(); // Enter on Open → post cmOpen + close

        assert_eq!(
            program.group_mut().len(),
            baseline,
            "selecting a command closed all menu boxes (back to baseline)"
        );
        assert_eq!(program.capture_len(), 0, "the session popped itself");
        assert!(
            program
                .out_events
                .iter()
                .any(|e| matches!(e, Event::Command(c) if *c == Cmd::OPEN)),
            "Enter on Open posted cmOpen"
        );
    }

    /// ONE kbEsc from a FIRST-level dropdown closes the WHOLE menu (box + session)
    /// without posting — `tmnuview.cpp:308-312`: at a 1st-level box `clearEvent`
    /// does NOT run (parent is the bar, size.y == 1), so the Esc is re-applied up
    /// to the bar, whose Esc (parentMenu == 0) clears + returns → menu closes.
    ///
    /// BITE: drop the not-cleared re-apply (treat the box Esc as cleared) → after
    /// one Esc the bar level survives (capture_len == 1, no bar-highlight clear),
    /// failing the asserts. Equivalently, restore the old two-Esc test.
    #[test]
    fn one_esc_from_first_level_closes_whole_menu() {
        let (mut program, bar_id, baseline) = program_with_menu_bar(40, 12);
        open_file_box(&mut program); // File box open
        assert_eq!(program.group_mut().len(), baseline + 1, "box open");

        program.out_events.push_back(key(Key::Esc));
        program.pump_once(); // ONE Esc → box closes AND session ends

        assert_eq!(
            program.group_mut().len(),
            baseline,
            "one Esc closed the dropdown (back to baseline)"
        );
        assert_eq!(program.capture_len(), 0, "one Esc popped the whole session");
        assert_eq!(
            bar_current(&mut program, bar_id),
            None,
            "bar highlight cleared"
        );
        assert!(
            !program
                .out_events
                .iter()
                .any(|e| matches!(e, Event::Command(_))),
            "Esc posts no command"
        );
    }

    /// ONE kbEsc from a SECOND-level box closes ONLY that inner box; the session
    /// and the first-level box stay open — the C++ `clearEvent` asymmetry
    /// (`tmnuview.cpp:310`: a 2nd-level box's parent is a box, size.y != 1, so the
    /// Esc IS cleared and does not propagate). This pins the asymmetry against
    /// `one_esc_from_first_level_closes_whole_menu`.
    ///
    /// BITE: drop the `esc_clear_event` guard (always re-apply) → the inner Esc
    /// would unwind to the bar and close everything (len == baseline,
    /// capture_len == 0), failing "session still open".
    #[test]
    fn one_esc_from_second_level_closes_only_inner_box() {
        let (mut program, _bar_id, baseline) = program_with_menu_bar(40, 12);
        open_file_box(&mut program); // File box (baseline + 1), on Open
        program.out_events.push_back(key(Key::Down));
        program.pump_once(); // highlight More
        program.out_events.push_back(key(Key::Enter));
        program.pump_once(); // open the More box (baseline + 2)
        assert_eq!(program.group_mut().len(), baseline + 2, "two boxes open");

        program.out_events.push_back(key(Key::Esc));
        program.pump_once(); // Esc at the 2nd-level box → close ONLY it

        assert_eq!(
            program.group_mut().len(),
            baseline + 1,
            "Esc closed only the inner box; the File box stays open"
        );
        assert_eq!(
            program.capture_len(),
            1,
            "the session is still active (not popped)"
        );
        assert_eq!(
            top_box_current(&mut program),
            Some(Some(1)),
            "the now-top box is File, still highlighting More"
        );
    }

    /// kbRight from an open first-level dropdown unwinds to the bar, walks to the
    /// adjacent title, and RE-OPENS its dropdown — `tmnuview.cpp:287-293` (box
    /// returns, not cleared) + the persisted bar `autoSelect` re-opening the
    /// neighbour (Blocker 3). F10 → kbDown (File box) → kbRight → Edit box.
    ///
    /// BITE: make kbRight on a box "just close the box" (cleared, no re-apply) →
    /// after kbRight no box is open and the bar did not advance, failing the
    /// "Edit box open" + "bar == Edit" asserts. Equivalently, drop the per-level
    /// `auto_select` (the bar would walk but NOT re-open).
    #[test]
    fn right_from_dropdown_walks_bar_and_reopens_neighbour() {
        let (mut program, bar_id, baseline) = program_with_menu_bar(40, 12);
        open_file_box(&mut program); // File box open, bar on File (0)
        assert_eq!(bar_current(&mut program, bar_id), Some(0), "bar on File");

        program.out_events.push_back(key(Key::Right));
        program.pump_once(); // box returns → bar trackKey → re-open Edit box

        assert_eq!(
            bar_current(&mut program, bar_id),
            Some(1),
            "kbRight walked the bar File → Edit"
        );
        assert_eq!(
            program.group_mut().len(),
            baseline + 1,
            "exactly one box open (File closed, Edit opened)"
        );
        assert_eq!(
            top_box_current(&mut program),
            Some(Some(0)),
            "the open box is Edit's, highlighting its default (Cut)"
        );
    }

    /// F10 then kbRight (NO dropdown yet, bar `autoSelect == False`) moves the bar
    /// title WITHOUT opening a box — the open-gate needs `autoSelect`, which cmMenu
    /// leaves False (Blocker 1/3 interplay).
    ///
    /// BITE: if cmMenu set autoSelect True (or activation opened a box), kbRight
    /// would open Edit's box → `group.len()` would be `baseline + 1`, failing.
    #[test]
    fn f10_then_right_moves_title_without_opening_box() {
        let (mut program, bar_id, baseline) = program_with_menu_bar(40, 12);
        program.out_events.push_back(Event::Command(Cmd::MENU));
        program.pump_once(); // F10: highlight File, no box, autoSelect False

        program.out_events.push_back(key(Key::Right));
        program.pump_once();

        assert_eq!(
            bar_current(&mut program, bar_id),
            Some(1),
            "kbRight walked the bar File → Edit"
        );
        assert_eq!(
            program.group_mut().len(),
            baseline,
            "no box opened (autoSelect False after F10)"
        );
    }

    /// Alt-shortcut activation: Alt+E opens the session at Edit (idx 1), with Edit's
    /// box open highlighting its default (Cut). Proves `findAltShortcut` activation
    /// opens the matched title's box directly (autoSelect True).
    ///
    /// BITE: if alt-shortcut activation opened at the menu default (File) instead
    /// of the matched item, the bar would highlight 0, not 1.
    #[test]
    fn alt_shortcut_opens_at_matched_item() {
        let (mut program, bar_id, baseline) = program_with_menu_bar(40, 12);

        program.out_events.push_back(alt_digit_letter('e'));
        program.pump_once();

        assert_eq!(
            bar_current(&mut program, bar_id),
            Some(1),
            "Alt+E highlights Edit (idx 1)"
        );
        assert_eq!(
            program.group_mut().len(),
            baseline + 1,
            "Alt+E opened the Edit box"
        );
        assert_eq!(
            top_box_current(&mut program),
            Some(Some(0)),
            "Edit box highlights its default (Cut)"
        );
    }

    /// A hotKey accelerator (F3 = Open) pressed while a dropdown is OPEN closes the
    /// WHOLE menu and posts the command at ANY depth — `tmnuview.cpp:392`: a
    /// `result` propagates up through every nested execView. Open File's box, then
    /// press F3; cmOpen must post AND the session must end.
    ///
    /// BITE: handle the hotKey result inside the per-level Return-pop (cleared) →
    /// one box pops, Consumed returns, the command is dropped and the session stays
    /// (capture_len == 1, no cmOpen). This is the QUALITY blocker-1 regression.
    #[test]
    fn hotkey_accelerator_closes_whole_menu_from_open_box() {
        let (mut program, _bar_id, baseline) = program_with_menu_bar(40, 12);
        open_file_box(&mut program); // File box open (baseline + 1)
        assert_eq!(program.group_mut().len(), baseline + 1, "box open");

        program
            .out_events
            .push_back(Event::KeyDown(KeyEvent::from(Key::F(3))));
        program.pump_once();

        assert_eq!(
            program.capture_len(),
            0,
            "the hotKey result ended the whole session"
        );
        assert_eq!(
            program.group_mut().len(),
            baseline,
            "every box closed (back to baseline)"
        );
        assert!(
            program
                .out_events
                .iter()
                .any(|e| matches!(e, Event::Command(c) if *c == Cmd::OPEN)),
            "F3 (Open's accelerator) posted cmOpen even from a deep box"
        );
    }

    /// A non-cmMenu command arriving while the menu is open closes the menu AND is
    /// re-posted (`tmnuview.cpp:403-405`: `putEvent(e)` when `e.what == evCommand`),
    /// so it survives for the view after the menu closes. Open the session, post an
    /// arbitrary command, pump: the menu closes and the command stays in the queue.
    ///
    /// BITE: drop the `ctx.put_event` re-post in the non-cmMenu command arm → the
    /// command is consumed/lost; the "survives" assert fails.
    #[test]
    fn foreign_command_closes_menu_and_is_reposted() {
        let (mut program, _bar_id, baseline) = program_with_menu_bar(40, 12);
        open_file_box(&mut program); // File box open
        assert_eq!(program.group_mut().len(), baseline + 1, "box open");

        // cmCut is in the default command set, so it is not dropped at the boundary.
        program.out_events.push_back(Event::Command(Cmd::CUT));
        program.pump_once();

        assert_eq!(
            program.capture_len(),
            0,
            "a foreign command closed the whole menu"
        );
        assert_eq!(
            program.group_mut().len(),
            baseline,
            "every box closed (back to baseline)"
        );
        assert!(
            program
                .out_events
                .iter()
                .any(|e| matches!(e, Event::Command(c) if *c == Cmd::CUT)),
            "the foreign command was re-posted (put_event) and survived"
        );
    }

    fn alt_digit_letter(c: char) -> Event {
        Event::KeyDown(KeyEvent::new(
            Key::Char(c),
            KeyModifiers {
                alt: true,
                ..Default::default()
            },
        ))
    }

    // -- rows 50-52, Step-2 stage 2: the MenuSession MOUSE arms ----------------
    //
    // Geometry for the `modal_menu` bar (computed from item_rect_local, mirrored in
    // these comments so the click points are auditable):
    //
    //   Bar (Rect(0,0,40,1)): File = item_rect_local(0) = Rect(1,0,7,1)  → x∈[1,7)
    //                         Edit = item_rect_local(1) = Rect(7,0,13,1) → x∈[7,13)
    //   File box opened below File: hint = Rect(0,1,40,12) (bar shift a.x--),
    //     menu_box_rect → Rect(0,1,14,5). Box rows (item_rect_global, +(0,1)):
    //       Open(0)  → Rect(2,2,12,3)  (y=2, x∈[2,12))
    //       More(1)  → Rect(2,3,12,4)  (y=3)
    //     A box-interior margin point not on any item: (1,2) (left frame column,
    //     x=1 < 2 so off every item rect, inside the box bounds).

    /// MouseDown at root-frame `(x, y)` with the left button held.
    fn m_down(x: i32, y: i32) -> Event {
        mouse_down_at(x, y)
    }
    /// MouseMove at root-frame `(x, y)` with the left button held (drag).
    fn m_move(x: i32, y: i32) -> Event {
        mouse_move_at(x, y)
    }
    /// MouseUp at root-frame `(x, y)` (no button — release).
    fn m_up(x: i32, y: i32) -> Event {
        mouse_up_at(x, y)
    }

    /// Click the bar's File title to open its box via the mouse activation path
    /// (`activate_mouse` → re-posted click → evMouseDown arm → open-gate). Needs two
    /// pumps: pump 1 reaches the bar's `handle_event` (pushes the session, re-posts
    /// the click); pump 2 runs the session's evMouseDown arm (opens the File box).
    fn click_file_title(program: &mut Program) {
        program.out_events.push_back(m_down(2, 0));
        program.pump_once(); // bar handle_event: activate_mouse, re-post the click
        program.pump_once(); // session evMouseDown: track File → open-gate → File box
    }

    /// (1) A MouseDown on a bar title opens its dropdown — the `do_a_select`
    /// activation flow (`tmnuview.cpp:505-516`) + the evMouseDown open-gate.
    ///
    /// DEVIATION FROM THE BRIEF (test 1 expectation): the brief asserts
    /// `top_box_current == Some(Some(0))`, but the C++ is faithful to `Some(None)` —
    /// the carried MouseDown (still at the bar row y=0) re-applies into the freshly
    /// opened box, whose `trackMouse` (`tmnuview.cpp:97-108`) finds no box item under
    /// (2,0) and so leaves `current == 0` (None). Real Turbo Vision shows the
    /// dropdown UNhighlighted until the mouse moves into it. So we assert the
    /// faithful `Some(None)`.
    ///
    /// BITE: if the bar's evMouseDown arm did not set `auto_select` (or the open-gate
    /// did not `continue`/open), no box would open (len stays baseline).
    #[test]
    fn click_bar_title_opens_box() {
        let (mut program, _bar_id, baseline) = program_with_menu_bar(40, 12);

        click_file_title(&mut program);

        assert_eq!(
            program.group_mut().len(),
            baseline + 1,
            "clicking File opened its dropdown"
        );
        assert_eq!(
            program.capture_len(),
            1,
            "the MenuSession is armed on the capture stack"
        );
        assert_eq!(
            top_box_current(&mut program),
            Some(None),
            "the freshly opened box is unhighlighted (carried click at the bar row \
             hits no box item — faithful to C++ trackMouse leaving current == 0)"
        );
    }

    /// (2) THE CRUX (brief §3.1): clicking an OPEN title closes its box.
    /// Click File (opens box), click File again → box closes, bar still highlights
    /// File. Driven by the pop-time `last_target_item = current` (set when the box
    /// pops) which makes the second click's `auto_select` come out False.
    ///
    /// BITE: drop the pop-time `parent.last_target_item = Some(cur)` assignment → the
    /// second click's `auto_select = !current || last_target != current` is True
    /// again → the File box REOPENS (len == baseline+1), failing the "closed" assert.
    #[test]
    fn click_open_title_closes_box() {
        let (mut program, bar_id, baseline) = program_with_menu_bar(40, 12);

        click_file_title(&mut program);
        assert_eq!(program.group_mut().len(), baseline + 1, "File box open");

        // Second click on the SAME (now open) title → closes it.
        program.out_events.push_back(m_down(2, 0));
        program.pump_once();

        assert_eq!(
            program.group_mut().len(),
            baseline,
            "clicking the open File title closed its box (back to baseline)"
        );
        assert_eq!(
            program.capture_len(),
            1,
            "the session stays armed at the bar (only the box closed)"
        );
        assert_eq!(
            bar_current(&mut program, bar_id),
            Some(0),
            "the bar still highlights File after the close-click"
        );
    }

    /// (3) Dragging (button held) from an open title to a neighbour title closes the
    /// first box and opens the neighbour's — the cross-level re-apply: the box's
    /// evMouseMove `!(mouseInView||mouseInOwner) && mouseInMenus → doReturn` arm
    /// (`tmnuview.cpp:267-269`) unwinds the box onto the bar, which trackMouses to the
    /// neighbour and re-opens it (the bar's persisted `auto_select` from activation,
    /// reinforced by the evMouseMove bar drag-open arm `:273`).
    ///
    /// BITE: drop the box's evMouseMove `mouse_in_menus → doReturn` arm → the box
    /// never returns to the bar, so the bar stays on File (`bar_current == Some(0)`),
    /// failing the "walked File → Edit" assert.
    #[test]
    fn drag_to_neighbour_title_reopens() {
        let (mut program, bar_id, baseline) = program_with_menu_bar(40, 12);

        click_file_title(&mut program); // File box open
        assert_eq!(program.group_mut().len(), baseline + 1, "File box open");

        // Drag (button held) onto the Edit title (x∈[7,13), y=0).
        program.out_events.push_back(m_move(8, 0));
        program.pump_once();

        assert_eq!(
            bar_current(&mut program, bar_id),
            Some(1),
            "the drag walked the bar File → Edit"
        );
        assert_eq!(
            program.group_mut().len(),
            baseline + 1,
            "exactly one box open (File closed, Edit opened)"
        );
    }

    /// (4) A MouseDown OUTSIDE the bar and box closes the whole menu AND re-posts the
    /// click to the view tree (brief §3.5, `tmnuview.cpp:220-222`
    /// `putClickEventOnExit`), so the view under the click recovers focus.
    ///
    /// BITE: drop the `ctx.put_event(ev)` re-post in the bar exit-click branch → the
    /// click is consumed/lost; the "MouseDown survives in out_events" assert fails.
    #[test]
    fn click_outside_closes_and_reposts() {
        let (mut program, _bar_id, baseline) = program_with_menu_bar(40, 12);

        click_file_title(&mut program); // File box open
        assert_eq!(program.group_mut().len(), baseline + 1, "File box open");
        program.out_events.clear(); // drop any pending set-current echoes

        // Click well outside the bar (y=0) and the File box (Rect(0,1,14,5)):
        // (30, 8) is on the bare desktop.
        program.out_events.push_back(m_down(30, 8));
        program.pump_once();

        assert_eq!(
            program.capture_len(),
            0,
            "clicking outside closed the session"
        );
        assert_eq!(
            program.group_mut().len(),
            baseline,
            "every box closed (back to baseline)"
        );
        assert!(
            program.out_events.iter().any(|e| matches!(
                e,
                Event::MouseDown(m) if m.position == Point::new(30, 8)
            )),
            "the exit click was re-posted to the view tree (putClickEventOnExit)"
        );
    }

    /// (5) Releasing on a submenu item inside a box opens its nested box and KEEPS
    /// it open (the child's `first_event` guard stops the instant-close).
    ///
    /// DEVIATION FROM THE BRIEF (test 5 description): the brief says a
    /// `MouseMove(button)` onto More opens the nested box, but per the C++ a BOX's
    /// `evMouseMove`/`evMouseDown` arms never set `autoSelect` (only the bar does,
    /// `tmnuview.cpp:273`), so a hover/press inside a box does NOT auto-open a
    /// submenu — only a `MouseUp` on it does (the `current != lastTargetItem →
    /// doSelect` arm, `tmnuview.cpp:233`, which feeds the open-gate). So we drag onto
    /// More to highlight it, then RELEASE on it to open the nested box.
    ///
    /// The brief's §3.3 mouse-down/move `continue` (re-applying the carried event
    /// into a freshly opened child) is the SEPARATE discriminator exercised by
    /// `click_bar_title_opens_box` (test 1): the carried bar-row MouseDown re-applies
    /// into the File box and `track_mouse` clears its `current` to None — which is
    /// exactly what makes test 1 assert `Some(None)` instead of `Some(Some(0))`. If
    /// the open-gate returned `Consumed` instead of `continue` for the mouse path,
    /// the carried click would not re-apply and test 1 would observe `Some(Some(0))`.
    ///
    /// BITE: break the evMouseUp `current != lastTargetItem → doSelect` arm (so a
    /// release on More does not feed the open-gate) → the nested box never opens
    /// (len stays baseline / the session even closes), failing the "+2" assert. (The
    /// `first_event` guard is independently load-bearing: forcing every box's
    /// `first_event` to false also breaks this test, since the File box's carried
    /// opening click would then instant-close it.)
    #[test]
    fn drag_into_submenu_keeps_open() {
        let (mut program, _bar_id, baseline) = program_with_menu_bar(40, 12);

        click_file_title(&mut program); // File box open (baseline + 1)
        // Drag onto the More submenu row (item 1 → Rect(2,3,12,4), y=3) to highlight.
        program.out_events.push_back(m_move(5, 3));
        program.pump_once();
        assert_eq!(
            top_box_current(&mut program),
            Some(Some(1)),
            "the drag highlighted More in the File box"
        );
        assert_eq!(
            program.group_mut().len(),
            baseline + 1,
            "a box hover does NOT auto-open the submenu (box never sets auto_select)"
        );

        // Release on More → doSelect (current != lastTargetItem) → open the nested
        // box, which stays open (first_event guard).
        program.out_events.push_back(m_up(5, 3));
        program.pump_once();

        assert_eq!(
            program.group_mut().len(),
            baseline + 2,
            "releasing on the More submenu opened its nested box (and it stays open)"
        );
        assert_eq!(
            program.capture_len(),
            1,
            "the session is still active (nested box did not instantly close)"
        );
    }

    /// (6) A MouseUp on a command item posts that command and closes the session —
    /// the evMouseUp `current != lastTargetItem → doSelect` arm (`tmnuview.cpp:233`).
    /// Click File to open its box, then release on Open: `current(Some(0)) !=
    /// last_target(None)` → doSelect → cmOpen posts + session ends.
    ///
    /// BITE: drop the evMouseUp `doSelect` arm (treat a release on a command as
    /// doNothing) → no command posts and the session stays open.
    #[test]
    fn mouseup_on_command_posts() {
        let (mut program, _bar_id, baseline) = program_with_menu_bar(40, 12);

        click_file_title(&mut program); // File box open (baseline + 1)
        program.out_events.clear();

        // Release on the Open row (Rect(2,2,12,3), y=2).
        program.out_events.push_back(m_up(4, 2));
        program.pump_once();

        assert_eq!(
            program.capture_len(),
            0,
            "releasing on a command ended the whole session"
        );
        assert_eq!(
            program.group_mut().len(),
            baseline,
            "every box closed (back to baseline)"
        );
        assert!(
            program
                .out_events
                .iter()
                .any(|e| matches!(e, Event::Command(c) if *c == Cmd::OPEN)),
            "releasing on Open posted cmOpen"
        );
    }

    /// (7) A MouseUp on a box margin (not on any item row) resets the highlight to
    /// the box default and KEEPS the box open — the evMouseUp box-margin arm
    /// (`tmnuview.cpp:251-261`, the `else if size.y != 1` reset). First move the
    /// highlight to More, then release on the left-frame margin → back to the
    /// default (Open, idx 0).
    ///
    /// BITE: drop the `else if !is_bar` reset arm → `track_mouse` left `current ==
    /// None` (the margin hit no item) and nothing restores it, so `top_box_current`
    /// is `Some(None)`, failing the "reset to default" assert.
    #[test]
    fn mouseup_on_box_margin_resets_to_default() {
        let (mut program, _bar_id, baseline) = program_with_menu_bar(40, 12);

        click_file_title(&mut program); // File box open, current = None
        // Drag onto More (idx 1) so the highlight is NOT the default before release.
        program.out_events.push_back(m_move(5, 3));
        program.pump_once();
        assert_eq!(
            top_box_current(&mut program),
            Some(Some(1)),
            "More highlighted before the margin release",
        );

        // Release on a box-interior margin point (1,2): inside the box bounds,
        // off every item rect (x = 1 < 2).
        program.out_events.push_back(m_up(1, 2));
        program.pump_once();

        assert_eq!(
            program.group_mut().len(),
            baseline + 1,
            "the File box stays open after a margin release",
        );
        assert_eq!(
            top_box_current(&mut program),
            Some(Some(0)),
            "the margin release reset the highlight to the box default (Open)",
        );
    }

    /// (8) `cmMenu` while a NESTED box is open closes every box back to the bar and
    /// leaves the session armed with the bar highlighted — the `execute()` evCommand
    /// arm (`tmnuview.cpp:343-350`): a box (`parentMenu != 0`) doReturns (not
    /// cleared), the tail re-posts cmMenu up, unwinding through every box to the bar,
    /// which resets autoSelect/lastTargetItem and stays open (`doNothing`).
    ///
    /// BITE: the OLD "reset the top level and return Consumed" (no doReturn) leaves
    /// the open box(es) on the stack — `group.len()` stays `baseline + 2`, failing
    /// the "closed back to the bar" assert.
    #[test]
    fn cmmenu_from_nested_box_closes_to_bar() {
        let (mut program, bar_id, baseline) = program_with_menu_bar(40, 12);

        // Open File box, then the nested More box (two box levels) via the keyboard.
        open_file_box(&mut program); // File box (baseline + 1), on Open
        program.out_events.push_back(key(Key::Down));
        program.pump_once(); // highlight More (idx 1)
        program.out_events.push_back(key(Key::Enter));
        program.pump_once(); // open the nested More box (baseline + 2)
        assert_eq!(program.group_mut().len(), baseline + 2, "two boxes open");

        // cmMenu arrives while the nested box is active → unwind to the bar.
        program.out_events.push_back(Event::Command(Cmd::MENU));
        program.pump_once();

        assert_eq!(
            program.group_mut().len(),
            baseline,
            "cmMenu closed every box back to the bar"
        );
        assert_eq!(
            program.capture_len(),
            1,
            "the session stays armed at the bar (cmMenu does not end it)"
        );
        assert_eq!(
            bar_current(&mut program, bar_id),
            Some(0),
            "the bar stays highlighted on File after cmMenu unwound"
        );
    }

    // -- row 52: TMenuPopup (popup_menu) ---------------------------------------
    //
    // A standalone popup menu (no bar). Geometry for `popup_data` opened at
    // `where_ = (5, 2)` on a 40×12 desktop (auto_place_popup, mirrored here so the
    // click points are auditable):
    //
    //   menu_box_rect(Rect(5,2,5,2), popup_data): w = 10 (every item label fits the
    //     minimum), h = 2 + 2 items = 4 → size_x = 10, size_y = 4.
    //   d = (40,12) - (5,2) = (35,10); r.move(min(10,35), min(5,10)) = move(10, 5)
    //     → box = Rect(5,3,15,7) (top-left at (p.x, p.y+1) = (5,3); room everywhere,
    //     so the contains-p shift does NOT fire).
    //   Box rows (item_rect_global, origin (5,3) + item_rect_local Rect(2,1+i,8,2+i)):
    //       Cut(0)  → Rect(7,4,13,5)  (y=4, x∈[7,13))
    //       Copy(1) → Rect(7,5,13,6)  (y=5)
    //   A point well outside the box (and there is no bar): (30, 10).

    /// A flat command popup menu: {Cut(cmCut), Copy(cmCopy)}. Builder-built, so its
    /// `default` is `Some(0)` — which the popup must CLEAR (no highlight on open).
    fn popup_data() -> Menu {
        Menu::builder()
            .command("~C~ut", Cmd::CUT)
            .command("~C~opy", Cmd::custom("test.copy"))
            .build()
    }

    /// A test-only view whose `handle_event` opens a [`popup_menu`] on a MouseDown —
    /// the harness equivalent of the editor right-click that is the C++
    /// `popupMenu`'s only consumer. (Mouse events route positionally to a view's
    /// `handle_event`; a bare `Event::Command` does not, so the trigger is a click.)
    struct PopupProbe {
        st: ViewState,
        where_: Point,
        menu: Menu,
    }
    impl PopupProbe {
        fn new(bounds: Rect, where_: Point, menu: Menu) -> Self {
            PopupProbe {
                st: ViewState::new(bounds),
                where_,
                menu,
            }
        }
    }
    impl View for PopupProbe {
        fn state(&self) -> &ViewState {
            &self.st
        }
        fn state_mut(&mut self) -> &mut ViewState {
            &mut self.st
        }
        fn draw(&mut self, _ctx: &mut DrawCtx) {}
        fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
            if let Event::MouseDown(_) = ev {
                crate::menu::popup_menu(self.where_, self.menu.clone(), ctx.owner_size(), ctx);
                ev.clear();
            }
        }
        fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
            Some(self)
        }
    }

    /// Open a popup by clicking a `PopupProbe`. Unlike the bar's `activate_mouse`,
    /// `popup_menu` does NOT re-post the trigger click, so a SINGLE pump opens the
    /// box (the probe's `handle_event` queues OpenMenuBox/SetMenuCurrent/PushCapture,
    /// which the deferred-apply phase of the same pump runs). Returns (program, baseline).
    fn open_popup(w: u16, h: u16, where_: Point) -> (Program, usize) {
        let (mut program, _handle, _clock) = program_with_desktop(w, h);
        program.group_mut().insert(Box::new(PopupProbe::new(
            Rect::new(0, 0, w as i32, h as i32),
            where_,
            popup_data(),
        )));
        program.out_events.clear();
        let baseline = program.group_mut().len();
        // Click the probe (anywhere) to trigger popup_menu.
        program.out_events.push_back(m_down(0, 0));
        program.pump_once();
        (program, baseline)
    }

    /// (P1) A popup opens its box with NO highlight (`menu->deflt = 0`,
    /// `tmenupop.cpp:51`): exactly one MenuBox child exists and its `current == None`.
    ///
    /// BITE: revert the popup level to `current: menu.default` (and skip the
    /// `menu.default = None`) → the builder default `Some(0)` highlights Cut → the
    /// box reads `Some(Some(0))`, failing the no-highlight assert.
    #[test]
    fn popup_opens_box_with_no_highlight() {
        let (mut program, baseline) = open_popup(40, 12, Point::new(5, 2));

        assert_eq!(
            program.group_mut().len(),
            baseline + 1,
            "popup_menu opened exactly one box (a single pump, no re-posted click)"
        );
        assert_eq!(
            program.capture_len(),
            1,
            "the popup MenuSession is armed on the capture stack"
        );
        assert_eq!(
            top_box_current(&mut program),
            Some(None),
            "the popup box has NO default highlight on open (menu->deflt = 0)"
        );
    }

    /// (P2) THE ANCHOR (the constraint that makes a popup a popup,
    /// `putClickEventOnExit = False`, `tmenupop.cpp:45`): a click OUTSIDE the popup
    /// closes it but does NOT re-post the click to the view tree.
    ///
    /// Contrast: the identical click-outside on a **bar** session
    /// (`click_outside_closes_and_reposts`) DOES re-post. The pair is the
    /// discriminator — dropping `&& self.put_click_event_on_exit` from the run() gate
    /// makes THIS test fail (popup re-posts) while the bar test still passes; an
    /// always-false gate breaks the bar test instead. That mutual break proves the
    /// flag is wired, not a no-op.
    ///
    /// BITE: drop `&& self.put_click_event_on_exit` → the popup re-posts → the
    /// "no MouseDown survives" assert fails.
    #[test]
    fn popup_click_outside_does_not_repost() {
        let (mut program, baseline) = open_popup(40, 12, Point::new(5, 2));
        assert_eq!(program.group_mut().len(), baseline + 1, "popup box open");
        program.out_events.clear(); // drop any pending set-current echoes

        // Click well outside the popup box (Rect(5,3,15,7)): (30, 10) is bare desktop.
        program.out_events.push_back(m_down(30, 10));
        program.pump_once();

        assert_eq!(
            program.capture_len(),
            0,
            "clicking outside closed the popup session"
        );
        assert_eq!(
            program.group_mut().len(),
            baseline,
            "the popup box closed (back to baseline)"
        );
        assert!(
            !program.out_events.iter().any(|e| matches!(
                e,
                Event::MouseDown(m) if m.position == Point::new(30, 10)
            )),
            "the popup exit-click was NOT re-posted (putClickEventOnExit = False)"
        );
    }

    /// (P3) Selecting a command in a popup posts that command and closes the session
    /// — a MouseDown then MouseUp on the Cut row (the evMouseUp `current !=
    /// lastTargetItem → doSelect` arm). After: cmCut posted, box gone, capture popped.
    ///
    /// BITE: same as `mouseup_on_command_posts` — break the doSelect arm and no
    /// command posts / the box stays open.
    #[test]
    fn popup_select_command_posts_and_closes() {
        let (mut program, baseline) = open_popup(40, 12, Point::new(5, 2));
        assert_eq!(program.group_mut().len(), baseline + 1, "popup box open");
        program.out_events.clear();

        // Press then release on the Cut row (Rect(7,4,13,5), y=4).
        program.out_events.push_back(m_down(9, 4));
        program.pump_once();
        program.out_events.push_back(m_up(9, 4));
        program.pump_once();

        assert_eq!(
            program.capture_len(),
            0,
            "selecting a command ended the popup session"
        );
        assert_eq!(
            program.group_mut().len(),
            baseline,
            "the popup box closed (back to baseline)"
        );
        assert!(
            program
                .out_events
                .iter()
                .any(|e| matches!(e, Event::Command(c) if *c == Cmd::CUT)),
            "selecting Cut posted cmCut"
        );
    }

    // -- row 52: TMenuPopup with a SUBMENU level (multi-level exit-click) -------
    //
    // A standalone popup containing a submenu, opened at `where_ = (5, 2)` on a
    // 40×12 desktop. Geometry (mirrored here so the click points are auditable):
    //
    //   popup_submenu_data = {Cut(0), More ▸ {Refresh}(1), Copy(2)}.
    //   menu_box_rect(Rect(5,2,5,2), …): w = 13 (More's "~M~ore" = 4 chars + 6 + 3
    //     for the submenu ► marker), h = 2 + 3 items = 5 → size_x = 13, size_y = 5.
    //   d = (40,12) - (5,2) = (35,10); r.move(min(13,35), min(6,10)) = move(13, 6)
    //     → popup box = Rect(5,3,18,8) (top-left (p.x, p.y+1) = (5,3); room, no shift).
    //   Popup box rows (item_rect_global, origin (5,3) + local Rect(2,1+i,11,2+i)):
    //       Cut(0)  → Rect(7,4,16,5)  (y=4)
    //       More(1) → Rect(7,5,16,6)  (y=5)   ← the submenu row
    //       Copy(2) → Rect(7,6,16,7)  (y=6)
    //   Opening the More submenu (open_submenu, parent origin (5,3), not a bar):
    //       hint = Rect(2+5, 3+3, 40, 12) = Rect(7,6,40,12);
    //       submenu {Refresh}: w = 13 (cstrlen("~R~efresh")=7 +6), h = 2+1 = 3;
    //       menu_box_rect(Rect(7,6,40,12), …) → submenu box = Rect(7,6,20,9).
    //       Refresh(0) → Rect(9,7,18,8) (y=7).
    //   A point outside BOTH boxes (popup Rect(5,3,18,8), submenu Rect(7,6,20,9))
    //     and inside the desktop: (30, 11).

    /// A popup with a SUBMENU: {Cut(cmCut), More ▸ {Refresh}, Copy(test.copy)}.
    /// Exercises the multi-level popup exit-click path (a deeper box level on top of
    /// the bottom popup level), which the flat `popup_data` cannot reach.
    fn popup_submenu_data() -> Menu {
        Menu::builder()
            .command("~C~ut", Cmd::CUT)
            .submenu("~M~ore", alt('m'), |s| {
                s.command("~R~efresh", Cmd::custom("test.refresh"))
            })
            .command("~C~opy", Cmd::custom("test.copy"))
            .build()
    }

    /// Like [`open_popup`] but with a caller-supplied `menu` fixture (so the flat
    /// `popup_data` consumers P1–P3 keep their hardcoded opener untouched). Returns
    /// (program, baseline).
    fn open_popup_menu(w: u16, h: u16, where_: Point, menu: Menu) -> (Program, usize) {
        let (mut program, _handle, _clock) = program_with_desktop(w, h);
        program.group_mut().insert(Box::new(PopupProbe::new(
            Rect::new(0, 0, w as i32, h as i32),
            where_,
            menu,
        )));
        program.out_events.clear();
        let baseline = program.group_mut().len();
        program.out_events.push_back(m_down(0, 0));
        program.pump_once();
        (program, baseline)
    }

    /// (P4) THE MULTI-LEVEL EXIT-CLICK (the path the SPEC reviewer proved faithful
    /// only by reasoning): open a popup, open its SUBMENU box (a second level), then
    /// click OUTSIDE all boxes. The whole session must collapse in ONE pump (the
    /// submenu doReturns and re-applies the carried `evMouseDown` down to the bottom
    /// popup level, which ends the session) AND the exit-click must NOT be re-posted
    /// — the popup's bottom-level `put_click_event_on_exit == false` swallows it even
    /// though the click originated while a deeper submenu level was on top. This is
    /// the C++ putEvent single-slot collapse modelled as one session-wide flag.
    ///
    /// BITE (verified): drop `&& self.put_click_event_on_exit` from the run() gate →
    /// the bottom popup level re-posts the carried exit-click → a `MouseDown` at
    /// (30,11) survives in `out_events`, failing the "no MouseDown re-posted" assert.
    /// With the gate present the test passes. (Same gate the flat P2 test bites; the
    /// added value here is COVERAGE of the multi-level carry-up, not a new mechanism.)
    #[test]
    fn popup_submenu_click_outside_collapses_without_repost() {
        let (mut program, baseline) =
            open_popup_menu(40, 12, Point::new(5, 2), popup_submenu_data());
        assert_eq!(
            program.group_mut().len(),
            baseline + 1,
            "popup box open (one level)"
        );

        // Open the More submenu (idx 1 → Rect(7,5,16,6), y=5): a box hover does NOT
        // auto-open, so drag to highlight More then RELEASE on it to open the nested
        // box (the evMouseUp `current != lastTargetItem → doSelect → open` arm).
        program.out_events.push_back(m_move(10, 5));
        program.pump_once();
        assert_eq!(
            top_box_current(&mut program),
            Some(Some(1)),
            "the drag highlighted More in the popup box"
        );
        program.out_events.push_back(m_up(10, 5));
        program.pump_once();
        assert_eq!(
            program.group_mut().len(),
            baseline + 2,
            "releasing on More opened the nested submenu box (two box levels)"
        );
        assert_eq!(
            program.capture_len(),
            1,
            "the session is still armed with the submenu open",
        );
        program.out_events.clear(); // drop any pending set-current echoes

        // Click well outside BOTH boxes (popup Rect(5,3,18,8), submenu Rect(7,6,20,9)):
        // (30, 11) is bare desktop. ONE pump must collapse submenu → popup → end.
        program.out_events.push_back(m_down(30, 11));
        program.pump_once();

        assert_eq!(
            program.capture_len(),
            0,
            "the outside click collapsed the WHOLE popup session in one pump"
        );
        assert_eq!(
            program.group_mut().len(),
            baseline,
            "both boxes closed (back to baseline child count)"
        );
        assert!(
            !program.out_events.iter().any(|e| matches!(
                e,
                Event::MouseDown(m) if m.position == Point::new(30, 11)
            )),
            "no MouseDown re-posted: the bottom popup level's \
             put_click_event_on_exit = False swallows the exit-click even from a \
             deeper submenu level"
        );
    }

    /// (9) A MouseUp whose position is on the PARENT title (mouseInOwner) resets the
    /// box highlight to the menu default and keeps the box open — the evMouseUp
    /// `mouseInOwner → current = menu->deflt` arm (`tmnuview.cpp:227-228`). Open File,
    /// drag-highlight More (idx 1, NOT the default), then release ON the File title
    /// in the bar → the box's highlight snaps back to Open (idx 0, File's default).
    ///
    /// BITE: drop the `mouse_in_owner → default` arm → `track_mouse` (which ran on a
    /// bar-row point that hits no box item) left `current == None`, so
    /// `top_box_current` is `Some(None)`, failing the "reset to default" assert.
    #[test]
    fn mouseup_on_parent_title_resets_box_to_default() {
        let (mut program, _bar_id, baseline) = program_with_menu_bar(40, 12);

        click_file_title(&mut program); // File box open (baseline + 1), current = None
        // Drag onto More (idx 1 → Rect(2,3,12,4), y=3) so current != the default (0).
        program.out_events.push_back(m_move(5, 3));
        program.pump_once();
        assert_eq!(
            top_box_current(&mut program),
            Some(Some(1)),
            "More highlighted before the release"
        );

        // Release with the position ON the File title in the bar (Rect(1,0,7,1)):
        // mouseInOwner is true → current = menu->deflt (Open, idx 0).
        program.out_events.push_back(m_up(2, 0));
        program.pump_once();

        assert_eq!(
            program.group_mut().len(),
            baseline + 1,
            "the File box stays open after a release on its parent title"
        );
        assert_eq!(
            program.capture_len(),
            1,
            "the session is still armed (the release did not close anything)"
        );
        assert_eq!(
            top_box_current(&mut program),
            Some(Some(0)),
            "the release on the parent title reset the box highlight to File's default"
        );
    }

    /// (10) A MouseUp OUTSIDE the box after the mouse has activated closes the menu —
    /// the evMouseUp `mouseActive && !mouseInView → doReturn` arm
    /// (`tmnuview.cpp:248-249`), distinct from the evMouseDown-outside path. Open File
    /// (the activation click sets the BAR's `mouse_active`), drag onto a box item (sets
    /// the BOX's `mouse_active`), then release at a point outside the box entirely:
    /// the box doReturns and re-applies up to the bar, whose own `mouse_active &&
    /// !mouseInView` arm ends the session.
    ///
    /// BITE: drop the `mouse_active && !mouse_in_view → doReturn` arm → the
    /// release-outside does nothing (action doNothing), the box stays open and the
    /// session stays armed, failing the "closed / popped" asserts.
    #[test]
    fn mouseup_outside_box_after_activating_closes() {
        let (mut program, _bar_id, baseline) = program_with_menu_bar(40, 12);

        click_file_title(&mut program); // File box open; bar mouse_active set
        // Drag (button held) onto the Open row (Rect(2,2,12,3), y=2) → box mouse_active.
        program.out_events.push_back(m_move(5, 2));
        program.pump_once();
        assert_eq!(
            top_box_current(&mut program),
            Some(Some(0)),
            "Open highlighted (box mouse_active set)"
        );

        // Release well outside the File box (Rect(0,1,14,5)) and off the bar:
        // (30, 8) is bare desktop.
        program.out_events.push_back(m_up(30, 8));
        program.pump_once();

        assert_eq!(
            program.group_mut().len(),
            baseline,
            "releasing outside the box after activating closed it (back to baseline)"
        );
        assert_eq!(
            program.capture_len(),
            0,
            "the session popped (the box returned, the bar's mouse_active arm ended it)"
        );
    }

    // -- Phase 4: real menu bar + status line wired into Program --------------
    //
    // These exercise the FULL factory path (Program::new builds + inserts a real
    // MenuBar and StatusLine, seeds their initial command graying, and pump_once
    // pre-routes keyDown / over-the-line mouseDown to the status line BEFORE
    // normal dispatch). Unlike `program_with_menu_bar` above (which inserts a bar
    // by hand), this drives the production construction.
    mod wiring {
        use super::*;
        use crate::menu::{Menu, alt};
        use crate::status::{StatusDef, StatusLine};
        use crate::view::ViewId;

        /// `Alt-X` — the canonical quit accelerator.
        fn alt_x() -> Event {
            Event::KeyDown(KeyEvent::new(
                Key::Char('x'),
                KeyModifiers {
                    alt: true,
                    ..Default::default()
                },
            ))
        }

        /// `F10` — the menu accelerator.
        fn f10() -> Event {
            Event::KeyDown(KeyEvent::new(Key::F(10), KeyModifiers::default()))
        }

        /// `Alt-X` as a `KeyEvent` (for menu/status accelerators).
        fn alt_x_key() -> KeyEvent {
            KeyEvent::new(
                Key::Char('x'),
                KeyModifiers {
                    alt: true,
                    ..Default::default()
                },
            )
        }

        /// A demo menu: File ▸ Exit (cmQuit), Window ▸ Next (cmNext).
        fn demo_menu() -> Menu {
            Menu::builder()
                .submenu("~F~ile", alt('f'), |m| {
                    m.command_key("E~x~it", Command::QUIT, alt_x_key(), "Alt-X")
                })
                .submenu("~W~indow", alt('w'), |m| m.command("~N~ext", Command::NEXT))
                .build()
        }

        /// A demo status line: labelled Alt-X Exit + F10 Menu, plus textless F5
        /// Zoom (a startup-DISABLED command, for the regray test).
        fn demo_status() -> Vec<StatusDef> {
            StatusDef::list()
                .def_all(|d| {
                    d.item("~Alt-X~ Exit", alt_x_key(), Command::QUIT)
                        .item("~F10~ Menu", KeyEvent::from(Key::F(10)), Command::MENU)
                        .key_item(KeyEvent::from(Key::F(5)), Command::ZOOM)
                })
                .build()
        }

        /// Build a program with a real desktop + status line + menu bar through the
        /// factory closures (the production path). Returns the program, the
        /// headless screen handle, and the (status_line, menu_bar) ids.
        fn program_full(w: u16, h: u16) -> (Program, HeadlessHandle, ViewId, ViewId) {
            let (backend, handle) = HeadlessBackend::new(w, h);
            let theme = Theme::classic_blue();
            let clock = Rc::new(ManualClock::new(0));
            let program = Program::new(
                Box::new(backend),
                Box::new(clock),
                theme,
                |r| {
                    let mut r = r;
                    r.a.y += 1;
                    r.b.y -= 1;
                    Some(Box::new(Desktop::new(r, |br| {
                        Some(Desktop::init_background(br))
                    })))
                },
                |r| {
                    let mut r = r;
                    r.a.y = r.b.y - 1;
                    Some(Box::new(StatusLine::new(r, demo_status())))
                },
                |r| {
                    let mut r = r;
                    r.b.y = r.a.y + 1;
                    Some(Box::new(MenuBar::new(r, demo_menu())))
                },
            );
            let mut program = program;
            // Drain the startup desktop-focus RECEIVED_FOCUS broadcast that
            // Program::new queues (program.rs:299), exactly like
            // `program_with_desktop` (program.rs:1204) — so a test's first injected
            // event is the one the first pump consumes (else it is off by one).
            program.out_events.clear();
            let sl = program.status_line().expect("status line created");
            let mb = program.menu_bar().expect("menu bar created");
            (program, handle, sl, mb)
        }

        // -- 1. Full-screen layout snapshot -----------------------------------

        #[test]
        fn snapshot_full_screen_layout() {
            // 40x10: menu bar pinned at row 0, status line at row 9, desktop in
            // between. Proves the inset (desktop r.a.y++/r.b.y--).
            let (mut program, handle, _sl, _mb) = program_full(40, 10);
            program.pump_once();
            insta::assert_snapshot!(handle.snapshot());
        }

        // -- 2. F10 -> menu opens (status-line keyDown accelerator) -----------

        #[test]
        fn f10_enters_menu_via_status_accelerator() {
            // F10 is pre-routed to the status line, transformed in place to
            // Event::Command(cmMenu), then propagates through normal dispatch to
            // the menu bar (preProcess) which activates a session (pushes a
            // capture). Proves the keyDown accelerator -> propagation.
            let (mut program, _handle, _sl, _mb) = program_full(40, 10);
            assert_eq!(program.capture_len(), 0, "no session open at startup");
            program.out_events.push_back(f10());
            program.pump_once();
            assert_eq!(
                program.capture_len(),
                1,
                "F10 -> cmMenu -> the bar activated a menu session (a pushed capture)"
            );
        }

        // -- 3. Alt-X -> quit (ONE pump: transform-in-place propagates) -------

        #[test]
        fn alt_x_quits_in_one_pump() {
            // Alt-X pre-routed -> transformed to cmQuit IN PLACE -> the SAME live
            // event flows through drop_disabled (cmQuit is enabled) ->
            // program_handle_event's cmQuit catch sets end_state. One pump.
            let (mut program, _handle, _sl, _mb) = program_full(40, 10);
            program.out_events.push_back(alt_x());
            program.pump_once();
            assert_eq!(
                program.end_state(),
                Some(Command::QUIT),
                "Alt-X reaches the status line, becomes cmQuit, ends the loop in one pump"
            );
        }

        // -- 4. THE crux: the accelerator fires DURING a modal -----------------

        #[test]
        fn accelerator_fires_during_a_modal() {
            // Push a ModalFrame directly (a dialog is open), then inject Alt-X.
            // Because the getEvent pre-routing runs BEFORE captures.dispatch, the
            // status line still sees the key, transforms it to cmQuit, and it
            // routes -> end_state set. BITE: moving the pre-route to AFTER
            // captures.dispatch makes this fail (the ModalFrame would gate, and the
            // raw keyDown — not yet a command — would never reach the status line).
            let (mut program, _handle, _sl, _mb) = program_full(40, 10);
            // A synthetic modal occupying a central rect.
            let modal_id = ViewId::next();
            let bounds = Rect::new(5, 3, 35, 8);
            program
                .captures
                .push(Box::new(ModalFrame::new(modal_id, bounds)));
            assert_eq!(program.capture_len(), 1, "modal frame pushed");

            program.out_events.push_back(alt_x());
            program.pump_once();
            assert_eq!(
                program.end_state(),
                Some(Command::QUIT),
                "Alt-X reaches the status line even with a modal open (pre-route is BEFORE dispatch)"
            );
        }

        /// Positive end-to-end: Alt-X pre-queued, then exec_view drives the modal
        /// loop — the accelerator ends the modal (returns QUIT). Mirrors the C++
        /// "cmQuit from a modal" path our exec_view documents.
        #[test]
        fn alt_x_ends_an_exec_view_modal() {
            let (mut program, _handle, _sl, _mb) = program_full(40, 10);
            // Pre-queue Alt-X so the first modal pump pre-routes it -> cmQuit ->
            // end_state, ending exec_view (else it would spin on headless).
            program.out_events.push_back(alt_x());
            let dialog = crate::dialog::Dialog::new(Rect::new(8, 3, 32, 7), Some("Modal".into()));
            let result = program.exec_view(Box::new(dialog));
            assert_eq!(
                result,
                Command::QUIT,
                "the modal ended on the Alt-X accelerator"
            );
        }

        // -- 5. mouseDown pre-route gating ------------------------------------

        #[test]
        fn mouse_down_on_status_line_posts_its_command() {
            // DISCRIMINATING (mirrors the keyDown modal crux): the pre-route's ONLY
            // observable difference from normal positional routing is that it runs
            // BEFORE captures.dispatch — so a click on the status line still reaches
            // it even when a modal capture gate would otherwise swallow it. Push a
            // ModalFrame whose bounds EXCLUDE the status-line row (rows 0..9, the
            // line is row 9), then click the line at "Alt-X Exit" (span [0, 12)):
            // normal routing would be gated out (the click is outside the modal ->
            // ModalFrame returns Consumed), so only the pre-route can deliver it.
            // The line posts cmQuit + clears; cmQuit routes next pump.
            //
            // BITE: removing the mouseDown pre-route arm makes the modal gate eat the
            // click -> the line never posts -> end_state stays None -> red.
            let (mut program, _handle, _sl, _mb) = program_full(40, 10);
            let modal_id = ViewId::next();
            // Modal covers rows 0..9 — the whole screen EXCEPT the status-line row.
            program
                .captures
                .push(Box::new(ModalFrame::new(modal_id, Rect::new(0, 0, 40, 9))));

            program.out_events.push_back(mouse_down_at(2, 9));
            program.pump_once(); // pre-route delivers to the line: posts cmQuit, clears
            program.pump_once(); // posted cmQuit routes -> cmQuit catch
            assert_eq!(
                program.end_state(),
                Some(Command::QUIT),
                "a click on the status line is pre-routed even past a modal gate that excludes it"
            );
        }

        #[test]
        fn mouse_down_in_desktop_reaches_the_desktop_not_the_cleared_line() {
            // DISCRIMINATING: the mouseDown gate's REAL job is preventing the status
            // line's unconditional ev.clear() from eating a click meant for the
            // desktop. A desktop-area click (NOT row h-1) must NOT be pre-routed, so
            // it survives to normal routing and reaches the view under it. We insert
            // a Probe at the click point and assert it RECEIVED the MouseDown.
            //
            // BITE: removing the `topmost_child_at(..) == Some(sl)` guard pre-routes
            // the desktop click to the status line, whose mouse arm clears it (misses
            // every item, y translates out of range) -> the cleared event skips
            // normal routing -> the Probe never sees it -> red.
            let (mut program, _handle, _sl, _mb) = program_full(40, 10);
            let log: Rc<RefCell<Vec<Event>>> = Rc::new(RefCell::new(Vec::new()));
            // A Probe covering a desktop-area rect (rows 1..8, between bar and line).
            // Inserted into the ROOT group on top of the desktop so it is the topmost
            // child at the click point (and records the MouseDown it receives).
            let mut probe = Probe::new(Rect::new(10, 3, 30, 7), 'P', log.clone());
            // ofFirstClick: deliver the focusing click to the view too (else the
            // auto-select-on-click in route_event clears it before `deliver`).
            probe.state_mut().options.first_click = true;
            program.group_mut().insert(Box::new(probe));
            program.out_events.clear(); // drop the insert's focus broadcast

            program.out_events.push_back(mouse_down_at(20, 5));
            program.pump_once();
            assert!(
                log.borrow()
                    .iter()
                    .any(|e| matches!(e, Event::MouseDown(_))),
                "a desktop-area click is NOT pre-routed/cleared and reaches the view under it"
            );
            assert_eq!(
                program.end_state(),
                None,
                "and it posts no spurious status-line command"
            );
        }

        // -- 6. Initial regray (no pump needed) -------------------------------

        #[test]
        fn initial_regray_greys_startup_disabled_commands() {
            // cmZoom is NOT in default_command_set (a startup-disabled window
            // command). After Program::new, the status line's cached command set
            // must already reflect that (seeded directly in the ctor — no pump),
            // and the menu has no cmZoom item but the bar's Window>Next (cmNext,
            // also startup-disabled) must be greyed.
            let (mut program, _handle, sl, mb) = program_full(40, 10);

            // Status line: cmd_set cached immediately, cmZoom disabled, cmQuit enabled.
            let cs = program
                .group_mut()
                .find_mut(sl)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_ref::<StatusLine>())
                .expect("status line resolves")
                .cmd_set()
                .cloned();
            let cs = cs.expect("initial regray seeded the status-line command-set cache (no pump)");
            assert!(cs.has(Command::QUIT), "cmQuit enabled at startup");
            assert!(
                !cs.has(Command::ZOOM),
                "cmZoom is a startup-disabled command -> greyed in the cache"
            );

            // Menu bar: Window > Next (cmNext, startup-disabled) must be greyed.
            let next_disabled = program
                .group_mut()
                .find_mut(mb)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_ref::<MenuBar>())
                .map(|bar| {
                    let window = &bar.menu().items[1]; // index 1 == ~W~indow
                    match window {
                        crate::menu::MenuItem::SubMenu { menu, .. } => {
                            matches!(&menu.items[0], crate::menu::MenuItem::Command { disabled, .. } if *disabled)
                        }
                        _ => panic!("expected the Window submenu"),
                    }
                })
                .expect("menu bar resolves");
            assert!(
                next_disabled,
                "cmNext (startup-disabled) is greyed in the bar immediately after Program::new"
            );
        }

        /// BITE for the initial regray: without the ctor seeding, the status-line
        /// cache would stay None (all-enabled). We can't easily disable the ctor
        /// seed, so this asserts the DISCRIMINATING fact the seed provides: a fresh
        /// `StatusLine` (never seeded) reports `cmd_set() == None` and treats
        /// everything as enabled — the gap the ctor closes.
        #[test]
        fn bite_unseeded_status_line_is_all_enabled() {
            let line = StatusLine::new(Rect::new(0, 0, 40, 1), demo_status());
            assert!(
                line.cmd_set().is_none(),
                "an unseeded line has no cache (the startup gap Program::new closes by seeding)"
            );
        }
    }
}
