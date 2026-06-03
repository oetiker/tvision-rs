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
use crate::event::{Event, Key};
use crate::theme::Theme;
use crate::timer::Clock;
use crate::timer::TimerQueue;
use crate::view::{Context, Deferred, DrawCtx, Group, Rect, SelectMode, View, ViewId};

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
                                // TGroup::endModal — set the loop end state; the
                                // nested exec_view loop (row 34) observes it.
                                Deferred::EndModal(cmd) => {
                                    *end_state = Some(cmd);
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
}
