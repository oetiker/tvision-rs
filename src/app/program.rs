//! The live event loop and application root.
//!
//! [`Program`] is the application root: it owns the single event loop, the
//! desktop / status-line / menu-bar subviews, the [`TimerQueue`], and the
//! [`CaptureStack`]. [`Program::run`] is the *only* loop ([`Program::pump_once`]
//! is one iteration), and modality is a [`ModalFrame`] on the [`CaptureStack`]:
//! a handler that consumes every otherwise-unhandled event *is* the modal loop.
//!
//! ## How it works
//!
//! * **One loop, no nested modal loops.** [`Program::run`] is the single event
//!   loop and `pump_once` is one iteration; modal dialogs are realized as a
//!   [`ModalFrame`] capture handler rather than a nested blocking loop.
//!
//! * **Broadcasts carry a subject.** `Event::Broadcast` carries a
//!   `source: ViewId` identifying the subject view; `Event::Command` carries only
//!   the [`Command`]. Integer payloads are not served by `source` (they are not
//!   `ViewId`s): a fired timer's id is carried by [`Event::Timer`], and a window
//!   number is resolved by a direct desktop walk.
//!
//! * **Whole-tree redraw + diff every pass.** No damage tracking; a terminal
//!   resize is picked up by the size check at the top of `pump_once` (the backend
//!   reports terminal size live).
//!
//! * **Injected [`Clock`] + [`Backend`].** Headless never blocks, so tests drive
//!   `pump_once` synchronously with a
//!   [`ManualClock`](crate::timer::ManualClock).
//!
//! * **String commands, enabled-by-default.** The command set is stored as its
//!   complement — the **disabled** set, seeded with the five startup-disabled
//!   window commands (see [`initial_disabled_commands`]). Every command not in it,
//!   including any app-minted [`Command::custom`], is enabled. See
//!   `docs/design/command-enablement.md`.
//!
//! Modal dialogs ([`exec_view`](Program::exec_view) / `messageBox` / `inputBox`)
//! run on top of the [`ModalFrame`] mechanism; Alt-1..9 window selection walks the
//! desktop directly for the child whose [`number`](View::number) matches; the
//! status line and menu bar are real subviews that the pump pre-routes keyDown and
//! over-the-line mouseDown events to before normal dispatch.
//!
//! # Turbo Vision heritage
//! Ports `TProgram` (`tprogram.cpp`/`tprogram.h`), the application root and its
//! blocking event loop. The single non-recursive loop plus a capture stack
//! replaces the original's nested modal loops (deviation D9); broadcasts carry a
//! [`ViewId`] subject instead of a raw pointer (deviation D4).

use std::collections::VecDeque;
use std::time::Duration;

use crate::backend::{Backend, Renderer};
use crate::capture::{CaptureFlow, CaptureHandler, CaptureStack};
use crate::command::{Command, CommandSet};
use crate::desktop::Desktop;
use crate::event::{Event, Key};
use crate::theme::Theme;
use crate::timer::Clock;
use crate::timer::TimerQueue;
use crate::view::{Context, Deferred, DrawCtx, Group, Point, Rect, SelectMode, View, ViewId};

/// The frame-tick timeout (20 ms → 50 wakeups per second). The maximum time a
/// production pump pass blocks waiting for a backend event before looping for a
/// frame tick. Headless ignores it.
///
/// # Turbo Vision heritage
/// Ports `TProgram::eventTimeoutMs` (`tprogram.cpp`).
const EVENT_TIMEOUT_MS: u64 = 20;

/// Auto-repeat initial delay before the first synthesized [`Event::MouseAuto`],
/// in ms. While a mouse button is held still, the first auto-repeat fires this
/// long after the press.
///
/// The value is 8 legacy 55 ms timer ticks (8 × 55 ms = **440 ms**), preserving
/// the press-and-hold cadence the widget code was written against.
const MOUSE_AUTO_DELAY_MS: u64 = 440;

/// Auto-repeat steady-state period between synthesized [`Event::MouseAuto`]
/// events, in ms. After the first auto fires, subsequent ones repeat on this
/// cadence for as long as the button stays held.
///
/// The value is two legacy 55 ms timer ticks (**110 ms**).
const MOUSE_AUTO_PERIOD_MS: u64 = 110;

// ---------------------------------------------------------------------------
// MouseAutoState — the global evMouseAuto synthesizer
// ---------------------------------------------------------------------------

/// The pump's mouse auto-repeat synthesizer state.
///
/// While a real mouse button is held, an otherwise idle pump pass synthesizes
/// [`Event::MouseAuto`] carrying the current (last-known) position: the first
/// after [`MOUSE_AUTO_DELAY_MS`], then every [`MOUSE_AUTO_PERIOD_MS`]. Real
/// events always win — the synthesizer only fires on a pass that produced no
/// other event.
///
/// **Why a timer-driven synthesizer:** a modern terminal only delivers mouse
/// reports while the cursor actually moves, so a held-still button would never
/// generate repeats on its own. Widgets that depend on press-and-hold (scrollbar
/// arrows, editor drag-scroll, menus) expect a steady stream of auto events
/// while a button is down; this clock-driven synthesizer supplies them.
///
/// # Turbo Vision heritage
/// Ports the `autoTicks` / `autoDelay` slice of `TEventQueue::getMouseEvent`
/// (`tevent.cpp`); the original derived auto-repeat from a hardware tick counter.
#[derive(Debug, Default)]
struct MouseAutoState {
    /// The held-button record: buttons from the press, position/modifiers
    /// updated by subsequent moves, `flags` cleared (a synthesized auto event
    /// carries no event flags). `None` = no button held.
    held: Option<crate::event::MouseEvent>,
    /// Clock deadline (ms) for the next synthesized auto.
    next_auto_ms: u64,
}

impl MouseAutoState {
    /// Bookkeeping on every *real* picked event (queue or backend), BEFORE
    /// dispatch mutates/localizes it.
    fn observe(&mut self, ev: &Event, now: u64) {
        match ev {
            // A press with a REAL button arms (re-arms) the delay —
            // `autoTicks = downTicks = ev.what; autoDelay = repeatDelay`
            // (tevent.cpp:167-168). Wheel events (`Event::MouseWheel`) must NOT
            // arm auto-repeat: they never reach this `MouseDown` arm, and C++
            // returns on the wheel arm before reaching the press/auto
            // bookkeeping (tevent.cpp:176-186).
            Event::MouseDown(m) if m.buttons.left || m.buttons.right || m.buttons.middle => {
                let mut held = *m;
                held.flags = crate::event::MouseEventFlags::default();
                self.held = Some(held);
                self.next_auto_ms = now + MOUSE_AUTO_DELAY_MS;
            }
            // A move while held updates the stored position/modifiers ONLY —
            // faithful: the C++ move arm updates `lastMouse` without touching
            // `autoTicks`/`autoDelay` (tevent.cpp:188-194), so a move does NOT
            // reset the cadence.
            Event::MouseMove(m) => {
                if let Some(h) = &mut self.held {
                    h.position = m.position;
                    h.modifiers = m.modifiers;
                }
            }
            // Release disarms.
            Event::MouseUp(_) => self.held = None,
            _ => {}
        }
    }

    /// On an idle pass (no real event), synthesize an [`Event::MouseAuto`] at
    /// the last-known position once the deadline has passed, then re-arm for the
    /// steady-state [`MOUSE_AUTO_PERIOD_MS`] cadence.
    fn synthesize(&mut self, now: u64) -> Option<Event> {
        let held = self.held?;
        if now >= self.next_auto_ms {
            self.next_auto_ms = now + MOUSE_AUTO_PERIOD_MS;
            Some(Event::MouseAuto(held))
        } else {
            None
        }
    }
}

/// The startup-disabled command seed.
///
/// The program tracks its command set as a **disabled set** (denylist):
/// everything not in it is enabled, so the open string-command space is
/// enabled-by-default and any app-minted [`Command::custom`] works without
/// registration. Only the five window-management commands
/// (`ZOOM`/`CLOSE`/`RESIZE`/`NEXT`/`PREV`) start disabled and are seeded here; a
/// window grants them on selection. Apps/widgets toggle commands via
/// [`Program::enable_command`] / [`Program::disable_command`]. See
/// `docs/design/command-enablement.md`.
///
/// # Turbo Vision heritage
/// Ports `TView::initCommands` (`tview.cpp`), which enabled all built-in
/// commands then disabled those same five (deviation D1: denylist representation).
fn initial_disabled_commands() -> CommandSet {
    let mut cs = CommandSet::new();
    for cmd in [
        Command::ZOOM,
        Command::CLOSE,
        Command::RESIZE,
        Command::NEXT,
        Command::PREV,
    ] {
        cs.insert(cmd); // insert into the DISABLED set: these start disabled
    }
    cs
}

// ---------------------------------------------------------------------------
// ModalFrame — the modality mechanism (a capture handler)
// ---------------------------------------------------------------------------

/// A capture handler that realizes modality: while it is on the
/// [`CaptureStack`], keyboard and command events
/// [`Pass`](CaptureFlow::Pass) through to normal routing and reach the modal
/// view via focus; broadcast events also [`Pass`](CaptureFlow::Pass) and fan
/// out to **all** views by design; positional (mouse) events are gated by
/// `bounds` — inside → [`Pass`](CaptureFlow::Pass), outside →
/// [`Consumed`](CaptureFlow::Consumed) (swallowed). This is the "a handler that
/// consumes every otherwise-unhandled event *is* the modal loop" realization.
///
/// It holds the modal view's [`ViewId`] (identity, per the capture contract) and
/// its `bounds` in the root group's frame, so positional events can be hit-tested
/// without holding a view reference. The root group covers the whole screen at
/// `(0,0)`, so group-local == absolute == this `bounds` frame.
///
/// [`exec_view`](Program::exec_view) is the blocking wrapper that pushes this
/// frame, runs the pump until [`Program::end_modal`] sets the end state, then
/// pops it (the one place a frame is popped other than a handler self-popping via
/// [`CaptureFlow::ConsumedPop`]) and marshals dialog data.
///
/// # Turbo Vision heritage
/// Replaces the nested blocking modal loop run by `TGroup::execute` /
/// `TGroup::execView` (`tgroup.cpp`). The single non-recursive loop plus this
/// capture handler stands in for that nesting (deviation D9), and the handler
/// holds a [`ViewId`] rather than a view pointer (deviation D3).
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

    fn is_modal_gate(&self) -> bool {
        true
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

/// The application root and single event loop.
///
/// `Program` is **not** a [`View`] (it is the root; nothing contains it). It
/// embeds a [`Group`] as its view container and adds the loop machinery: the
/// [`Renderer`], the live [`CaptureStack`] and [`TimerQueue`], the injected
/// [`Clock`], and the current command set.
///
/// Construct with [`Program::new`] (backend-injected so headless tests drive it),
/// drive production with [`Program::run`], or step one iteration with
/// [`Program::pump_once`] in tests.
///
/// # Turbo Vision heritage
/// Boxed per-idle-pass callback registered via [`Program::set_on_idle`].
type IdleHook = Box<dyn FnMut(&mut Program)>;

/// Loop-owned record of the window currently in frameless-fullscreen, used to
/// re-fit on resize and restore chrome if the window is removed out from under us.
#[derive(Clone, Copy)]
struct FullscreenSlot {
    window: crate::view::ViewId,
    mode: crate::window::Fullscreen,
    /// Pre-fullscreen window bounds, restored on exit.
    restore: Rect,
    /// Pre-fullscreen shadow flag, restored verbatim on exit.
    shadow: bool,
}

/// Ports `TProgram` (`tprogram.cpp`). The original derived an application class
/// from this root by inheritance; here [`Application`] embeds a `Program` and
/// forwards to it (deviation D2), and the single event loop replaces the nested
/// modal loops (deviation D9).
pub struct Program {
    /// The root container (holds desktop/status-line/menu-bar children).
    group: Group,
    /// Owns the back/front [`Buffer`](crate::screen::Buffer) pair + boxed backend.
    renderer: Renderer,
    /// The LIFO capture stack.
    captures: CaptureStack,
    /// The timer queue.
    timers: TimerQueue,
    /// Injected time source (so headless tests drive a manual clock).
    clock: Box<dyn Clock>,
    /// The active theme (the paint pass needs `&Theme` for `DrawCtx`).
    theme: Theme,
    /// Posted commands / broadcasts + queued timer-expiry broadcasts, drained
    /// before polling the backend. A distinct field so `Context` can borrow it
    /// disjointly (see the borrow-discipline note on `pump_once`).
    out_events: VecDeque<Event>,
    /// Deferred effects on loop-owned state ([`Deferred`]), applied *after* each
    /// dispatch — capture pushes (→ `captures`), command enable/disable (→
    /// `disabled_commands`), and tree mutations (bounds / state-flag / close →
    /// `group`).
    /// A downward-borrowed view / capture handler cannot touch the capture stack,
    /// the command set, or the tree inline, so it requests the effect via
    /// `Context` and the loop drains this one queue. A distinct field for the same
    /// disjoint-borrow reason as `out_events`. One channel — a new capability adds a
    /// [`Deferred`] variant, not a field.
    deferred: Vec<Deferred>,
    /// The **disabled**-command set — `curCommandSet` stored as its complement
    /// (denylist): a command is enabled iff it is NOT in here, so the open
    /// string-command space is enabled-by-default. Seeded by
    /// [`initial_disabled_commands`].
    disabled_commands: CommandSet,
    /// The inserted desktop child's id (the focus-move target for Alt-N window
    /// selection).
    desktop: Option<ViewId>,
    /// The inserted menu-bar child's id, if one was created.
    /// Held so the ctor can seed its initial command-graying and so future rows can
    /// route to it; the pump itself does not read it (see the `pump_once`
    /// destructure, where it is bound `_`).
    menu_bar: Option<ViewId>,
    /// The inserted status-line child's id, if one was created. The event
    /// pre-routing in [`pump_once`](Self::pump_once) reads it to hand keyDown /
    /// over-the-line mouseDown events to the line first.
    status_line: Option<ViewId>,
    /// The window currently in frameless-fullscreen, if any (loop-owned).
    fullscreen: Option<FullscreenSlot>,
    /// The global mouse auto-repeat synthesizer: while a real mouse button is
    /// held, idle pump passes synthesize [`Event::MouseAuto`] on the 440 ms /
    /// 110 ms cadence (see [`MouseAutoState`]).
    mouse_auto: MouseAutoState,
    /// `Some(cmd)` ends the (modal) loop with that command as its result.
    end_state: Option<Command>,
    /// Set on an enable/disable change; the changed command set is broadcast once
    /// on the next idle pass, then this flag is cleared.
    command_set_changed: bool,
    /// A view-requested modal awaiting top-level execution. Set by the
    /// `OpenHistory` / `OpenMessageBox` apply arms in the `pump_once` deferred drain
    /// (a view cannot call `exec_view` — top-level only); drained by the outer
    /// driver [`pump_and_drive`](Self::pump_and_drive) after `pump_once` returns,
    /// where a whole `&mut self` is held. The tuple is `(modal, completion,
    /// initial_focus)`: the boxed view is the modal; the [`ModalCompletion`] runs
    /// after the modal loop ends but before the view is removed/dropped (so it can
    /// read the modal's final state); `initial_focus` is the child to focus on open
    /// — `Some(first_button)` for a `messageBox` so the default button (Yes/OK) is
    /// focused, `None` for `OpenHistory` (the `HistoryWindow` manages its own
    /// focus).
    pending_modal: Option<(Box<dyn View>, ModalCompletion, Option<ViewId>)>,
    /// Commands that survived every level of handling (not consumed by any view or
    /// the built-in program-level handlers for QUIT/TILE/CASCADE/Alt-N). Drained by
    /// [`run_app`](Self::run_app) between pump cycles — the slot for
    /// application-level command handling.
    app_commands: VecDeque<Command>,
    /// The registered internal-clipboard editor ID (a process-global; `None` =
    /// use the OS clipboard). Set via `Deferred::RegisterClipboardEditor` in the
    /// pump drain.
    clipboard_editor_id: Option<ViewId>,
    /// Whether the current clipboard editor has a non-empty selection. Refreshed
    /// after `ClipboardEditorReceive` and passed to the `Context` clipboard snapshot
    /// so `update_commands` can gate paste correctly without a live tree borrow.
    clipboard_has_selection: bool,
    /// Optional hook that produces the shell-suspend message printed before the
    /// terminal is yielded to the shell (`Command::DOS_SHELL`). When `None` the
    /// platform default is used — see [`default_shell_msg`].
    ///
    /// Successor to `TApplication::writeShellMsg` (virtual in C++); set via
    /// [`Program::set_shell_msg_hook`].
    shell_msg_hook: Option<Box<dyn Fn() -> String>>,
    /// Optional per-idle-pass callback (see [`Program::set_on_idle`]). Fired from
    /// the run loop — never inside [`pump_once`](Self::pump_once)'s destructured
    /// borrow — on every event-less pass.
    on_idle: Option<IdleHook>,
}

/// What to do with a view-triggered modal's result, run AFTER the modal loop ends
/// but BEFORE the modal view is removed/dropped (so it can read the modal's final
/// state, e.g. `get_selection`). An enum, not a boxed `FnOnce`: a view-made closure
/// cannot hold `&mut Program`, and the codebase's pattern is to add a variant for
/// each new completion kind.
enum ModalCompletion {
    /// History list completion: on [`Command::OK`], read the `HistoryWindow`'s
    /// selection and `set_value` it into the linked input line (data +
    /// `select_all`). On cancel, nothing.
    HistoryPick { link: ViewId },
    /// The async-modal-from-a-view message-box completion (handle_event paths):
    /// route the user's chosen button [`Command`] back to the requesting view via
    /// [`View::set_modal_answer`], then re-post `then_command` (e.g.
    /// [`Command::CLOSE`]) so the original action re-runs `valid()` with the
    /// cached answer.
    RouteModalAnswer {
        /// The view to route the answer to (the `valid()` requester).
        answer_to: ViewId,
        /// The focused command to re-post after routing (`None` = no re-post).
        then_command: Option<Command>,
    },
    /// An informational (OK-only) async message box with no requester to route to
    /// (a validator `error`, a `FileEditor` save-error popup). The box just shows;
    /// nothing happens on close.
    Informational,
    /// "Save as" result (the view-triggered `FileDialog` seam): on a non-cancel
    /// close, read the filename from the in-tree `FileDialog` (`value()` →
    /// `FieldValue::Text`), set it on the `FileEditor` (`editor_id`), flag
    /// `pending_title_update`, and re-inject `Command::SAVE` so the normal save
    /// path runs with a full `ctx`. On cancel, nothing.
    ///
    /// The accept test is `result != Command::CANCEL` (NOT `== OK`): the
    /// `FileDialog`'s OK button ends the modal with its own file-open command,
    /// not a plain `OK`.
    SaveAsPick { editor_id: ViewId },

    /// Find-dialog result: on non-cancel, read `find_str` + options from the
    /// in-tree Find dialog, update the editor, and re-inject
    /// [`Command::SEARCH_AGAIN`].
    FindPick {
        editor_id: ViewId,
        /// `ViewId` of the "Text to find" `InputLine` child.
        find_id: ViewId,
        /// `ViewId` of the `CheckBoxes` child (case/words options).
        opts_id: ViewId,
    },

    /// Replace-dialog result: on non-cancel, read find+replace strings + options
    /// from the in-tree Replace dialog, set `EF_DO_REPLACE`, and re-inject
    /// [`Command::SEARCH_AGAIN`].
    ReplacePick {
        editor_id: ViewId,
        /// `ViewId` of the "Text to find" `InputLine` child.
        find_id: ViewId,
        /// `ViewId` of the "New text" `InputLine` child.
        replace_id: ViewId,
        /// `ViewId` of the `CheckBoxes` child (case/words/prompt/all options).
        opts_id: ViewId,
    },

    /// Result from the per-role color picker opened from `ThemeEditorBody`.
    /// On [`Command::OK`], read the `ColorPicker`'s color() and update the
    /// `ThemeEditorBody`'s working theme for the given role/fg. On cancel,
    /// nothing.
    ///
    /// **Deliberate cluster-D exception (spec §3.3/§2.1):** unlike `FindPick`/
    /// `ReplacePick` (which deliver their result downcast-free via
    /// [`View::set_modal_data`]), this completion stays downcasting because its
    /// payload is a [`Color`](crate::color::Color), which is deliberately **not** a
    /// [`FieldValue`](crate::data::FieldValue) (`FieldValue::Color` is an explicit
    /// non-goal). The result never crosses a `FieldValue` boundary, so the
    /// `set_modal_data` path does not apply — recorded, not a downcast we claim to
    /// delete.
    ThemeColorPick {
        /// `ViewId` of the `ThemeEditorBody` to update.
        editor_id: ViewId,
        /// `ViewId` of the `ColorPicker` child inside the color-picker dialog.
        picker: ViewId,
        /// The role being edited.
        role: crate::theme::Role,
        /// `true` = foreground, `false` = background.
        fg: bool,
    },
}

impl Program {
    /// Construct the program. The three subviews are built from injected factory
    /// closures over the full program extent; each factory owns its own shrinking
    /// (the status-line / menu-bar factories may return `None` to omit them).
    ///
    /// Construction behavior:
    /// - Bounds = `(0, 0, w, h)` from `backend.size()`.
    /// - The root group's state gets `active`/`selected`/`focused`/`modal` set
    ///   directly (it is the always-visible, always-focused root).
    /// - Insert desktop, status-line, menu-bar **in that order**.
    /// - The desktop is made `current` so focused events route into it (the
    ///   `insert` deliberately does not auto-select).
    ///
    /// # Turbo Vision heritage
    /// Ports `TProgram::TProgram` (`tprogram.cpp`); the factory closures stand in
    /// for the original's virtual `initDeskTop` / `initStatusLine` / `initMenuBar`
    /// factory-mixin hooks.
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
        // sfVisible is already the ctor default; sfExposed is dropped (no
        // occlusion tracking — the whole tree is redrawn each pass).
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
        // (initDeskTop: r.a.y++; r.b.y--, etc.); the factory does it.
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
        // We hold `group` + the disabled set here, so seed each view's
        // command-graying cache directly via the established broker hook
        // (`View::update_menu_commands`, contract: the DISABLED set) — no need to
        // defer (the deferred queue is not drained on the first idle pump anyway).
        // C++ gets this for free because `commandEnabled` is read live in
        // `drawSelect`; our snapshot cache must be primed once at construction.
        let disabled_commands = initial_disabled_commands();
        for id in [menu_bar, status_line].into_iter().flatten() {
            if let Some(v) = group.find_mut(id) {
                v.update_menu_commands(&disabled_commands);
            }
        }

        // STARTUP CURRENCY: one eager settle pass over the whole tree — the
        // insert-time show()->resetCurrent cascade C++ runs inline at every
        // insert of an ofSelectable view (`show() -> setState(sfVisible) ->
        // owner->resetCurrent()`). The ctx-less inserts above (desktop / status
        // line / menu bar, plus anything the factories pre-inserted, e.g. the
        // examples/hello.rs window stack) each marked their owning group
        // `currency_dirty`; settle_currency runs the pending reset_currents
        // POST-ORDER (children first), so:
        //   - every pre-inserted window's INTERNAL currency exists before the
        //     desktop's reset descends into it (the formerly-latent nested gap —
        //     a window's own children settle before the window is focused);
        //   - the desktop's reset makes the topmost ofTopSelect window current;
        //   - the root's reset makes the desktop current (firstMatch checks
        //     children[0] == the desktop, which is selectable) and — the root
        //     group is already sfFocused (the C++ ctor state bits above) — the
        //     focus cascade descends desktop -> window -> child.
        // With no selectable child anywhere this whole pass is a no-op.
        //
        // Side-effect bookkeeping: a
        // window's set_state(Selected) queues Deferred::EnableCommand(NEXT/...)
        // and the focus cascade queues RECEIVED_FOCUS broadcasts into
        // out_events. The first pump pops one of those broadcasts (out_events is
        // non-empty: the desktop-focus broadcast is unconditional), and its
        // post-dispatch drain applies the whole deferred queue — so nothing is
        // lost and no explicit apply step is needed here.
        {
            let now = clock.now_ms();
            let mut ctx = Context::new(&mut out_events, &mut timers, now, &mut deferred);
            ctx.set_disabled_commands(disabled_commands.clone());
            group.settle_currency(&mut ctx);
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
            disabled_commands,
            desktop,
            menu_bar,
            status_line,
            fullscreen: None,
            mouse_auto: MouseAutoState::default(),
            end_state: None,
            command_set_changed: true, // first idle pump broadcasts cmCommandSetChanged so startup-disabled buttons self-gray
            pending_modal: None,
            app_commands: VecDeque::new(),
            clipboard_editor_id: None,
            clipboard_has_selection: false,
            shell_msg_hook: None,
            on_idle: None,
        }
    }

    /// The desktop child's [`ViewId`], or `None` if the `create_desktop` factory
    /// returned `None` at construction.
    ///
    /// The id is stable for the application lifetime. Use it when you need to
    /// reach the desktop view directly (e.g. to call a method on a custom desktop
    /// subtype via `as_any_mut`). To open new windows in the desktop at runtime,
    /// prefer [`Program::desktop_insert`] — it handles the `Context` and focus
    /// bookkeeping for you.
    ///
    /// # Turbo Vision heritage
    /// Ports the C++ `TProgram::deskTop` global static pointer
    /// (`include/tvision/app.h`), reshaped into an owned `ViewId` handle (global
    /// raw pointer → handle, resolved via `find_mut`).
    pub fn desktop(&self) -> Option<ViewId> {
        self.desktop
    }

    /// The menu-bar child's [`ViewId`], or `None` if the `create_menu_bar` factory
    /// returned `None` at construction.
    ///
    /// The handle is stable for the application lifetime. Use it to resolve the
    /// menu bar view when you need to update its items at runtime (e.g. to rebuild
    /// the menu on locale change). For command-enablement, prefer
    /// [`Program::enable_command`] / [`Program::disable_command`] — the menu bar
    /// observes the `cmCommandSetChanged` broadcast automatically.
    ///
    /// # Turbo Vision heritage
    /// Ports the C++ `TProgram::menuBar` global static pointer
    /// (`include/tvision/app.h`), reshaped into an owned `ViewId` handle.
    pub fn menu_bar(&self) -> Option<ViewId> {
        self.menu_bar
    }

    /// The status-line child's [`ViewId`], or `None` if the `create_status_line`
    /// factory returned `None` at construction.
    ///
    /// The pump pre-routes `KeyDown` events to the status line first (so status-bar
    /// hot keys fire regardless of focus), and over-the-line `MouseDown` events are
    /// also pre-routed to it. Use the id when you need to reach the status-line view
    /// directly (e.g. to call a custom method after downcasting via `as_any_mut`).
    ///
    /// # Turbo Vision heritage
    /// Ports the C++ `TProgram::statusLine` global static pointer
    /// (`include/tvision/app.h`), reshaped into an owned `ViewId` handle. The
    /// pre-routing behavior mirrors `TProgram::getEvent` (`tprogram.cpp:153`).
    pub fn status_line(&self) -> Option<ViewId> {
        self.status_line
    }

    /// Register a closure that produces the shell-suspend message printed to the
    /// terminal before the application yields to the shell (`Command::DOS_SHELL`,
    /// typically wired to a "Shell" menu item or Ctrl-Z).
    ///
    /// Call this once during setup when the default platform message is not
    /// appropriate — for example, to include your app name or instructions specific
    /// to your shell environment:
    ///
    /// ```rust,ignore
    /// program.set_shell_msg_hook(Box::new(|| {
    ///     "MyApp is suspended. Type `fg` to return.".to_string()
    /// }));
    /// ```
    ///
    /// When not set, the built-in platform default is used (Windows: "Type EXIT to
    /// return..."; Unix: the `fg` return instruction).
    ///
    /// # Turbo Vision heritage
    /// Replaces the virtual `TApplication::writeShellMsg` override point
    /// (`tapplica.cpp`). The C++ default printed `"Type EXIT to return..."` on
    /// Windows/DOS and the `fg`-instruction on unix; that same branching logic
    /// lives in the crate-private `default_shell_msg` helper.
    pub fn set_shell_msg_hook(&mut self, hook: Box<dyn Fn() -> String>) {
        self.shell_msg_hook = Some(hook);
    }

    /// Register a callback run once on every **idle** pass of the event loop —
    /// each iteration where no input event was waiting.
    ///
    /// Use it for background work that should advance whenever the app is not
    /// busy: a clock, an animation frame, a periodic refresh. The callback gets
    /// `&mut Program`, so it can insert/close windows, post commands, or read
    /// state. Keep it cheap — it runs on the loop's idle cadence (the 20 ms frame
    /// tick), not a real-time scheduler. For exact timing, prefer a timer
    /// ([`Event::Timer`]).
    ///
    /// Only one idle callback is held; a second call replaces the first.
    ///
    /// The hook fires on the idle passes of **any** loop level, including while a
    /// modal dialog, message box, or other `exec_view` is open — so a clock keeps
    /// ticking during a dialog rather than freezing. It does **not** re-enter
    /// itself: while the hook runs, it is taken out, so a modal it opens will not
    /// fire the hook recursively.
    ///
    /// # Driving the UI from external / async data sources
    ///
    /// `set_on_idle` is the **program-level** hook for draining external data into
    /// the view tree. The pattern:
    ///
    /// 1. Hold shared application state in an `Rc<RefCell<AppState>>` cloned into
    ///    each view factory closure and into the idle callback.
    /// 2. In the callback, drain the external source (e.g. `try_recv` on a
    ///    `std::sync::mpsc::Receiver`) into the shared state.
    /// 3. Broadcast a refresh command so interested views repaint from the updated
    ///    state. Use a stable `Command::custom("myapp.refresh")` constant.
    ///
    /// ```rust,ignore
    /// let shared = Rc::new(RefCell::new(AppState::default()));
    /// let shared_idle = Rc::clone(&shared);
    /// program.set_on_idle(move |prog| {
    ///     let mut state = shared_idle.borrow_mut();
    ///     if let Ok(msg) = receiver.try_recv() {
    ///         state.update(msg);
    ///         drop(state); // release borrow before broadcast
    ///         prog.broadcast(REFRESH, None);
    ///     }
    /// });
    /// ```
    ///
    /// For **view-owned** drains (where the drain logic lives inside a view with
    /// access to `Context` rather than `&mut Program`), prefer a periodic timer
    /// set via [`Context::set_timer`] combined with [`Context::broadcast`]. A
    /// zero-area child view can hold the timer and be invisible on screen while
    /// still receiving every `Event::Timer` tick — see [`Context::set_timer`] for
    /// the cross-reference.
    ///
    /// # Turbo Vision heritage
    ///
    /// The successor to overriding `TProgram::idle`, which Turbo Vision called
    /// once per event-less loop pass (the guide's clock / heap-display pattern),
    /// including during modal `execute()` loops.
    pub fn set_on_idle(&mut self, f: impl FnMut(&mut Program) + 'static) {
        self.on_idle = Some(Box::new(f));
    }

    /// Thin test accessor over [`resolve_shell_msg`]: returns the same string
    /// the production `DOS_SHELL` branch prints, without triggering the actual
    /// suspend/SIGTSTP/resume sequence.
    #[cfg(test)]
    pub(crate) fn shell_msg(&self) -> String {
        resolve_shell_msg(&self.shell_msg_hook)
    }

    /// Signal that the event loop should stop, returning `cmd` as the result
    /// of [`run`](Self::run) once the tree validates it.
    ///
    /// **Owner-side, immediate.** Call this when you already hold
    /// `&mut Program` — for example from an application `main`, a test
    /// harness, or a startup sequence. For the far more common case where a
    /// *view* closes a dialog from inside `handle_event`, use
    /// [`Context::end_modal`](crate::view::Context::end_modal) instead (which
    /// pushes a deferred effect that the pump applies after dispatch unwinds).
    /// Rule of thumb: view → `ctx.end_modal`; owner / top-level →
    /// `Program::end_modal`.
    ///
    /// See also: [ending a modal](../../../port/modal.html#endmodal) in the
    /// tvision-rs guide.
    ///
    /// # Turbo Vision heritage
    /// Ports `TGroup::endModal` (`tgroup.cpp`).
    pub fn end_modal(&mut self, cmd: Command) {
        self.end_state = Some(cmd);
    }

    /// The current modal end state, if set (test/inspection hook).
    pub fn end_state(&self) -> Option<Command> {
        self.end_state
    }

    // -- command-enable policy ----------------------------------------------

    /// Enable `cmd` program-wide. On a real change (the command was previously
    /// disabled) this sets the command-set-changed flag so the next idle pass
    /// broadcasts the new command set, letting menus and the status line regray.
    ///
    /// # Turbo Vision heritage
    /// Ports `TView::enableCommand` (`tview.cpp`), program-side.
    pub fn enable_command(&mut self, cmd: Command) {
        if self.disabled_commands.has(cmd) {
            self.disabled_commands.remove(cmd);
            self.command_set_changed = true;
        }
    }

    /// Disable `cmd` program-wide. On a real change (the command was previously
    /// enabled) this sets the command-set-changed flag (see
    /// [`enable_command`](Self::enable_command)).
    ///
    /// # Turbo Vision heritage
    /// Ports `TView::disableCommand` (`tview.cpp`), program-side.
    pub fn disable_command(&mut self, cmd: Command) {
        if !self.disabled_commands.has(cmd) {
            self.disabled_commands.insert(cmd);
            self.command_set_changed = true;
        }
    }

    /// Whether `cmd` is currently enabled: enabled iff not in the disabled set.
    /// The command space is open strings — all enabled by default and all
    /// maskable.
    ///
    /// # Turbo Vision heritage
    /// Ports `TView::commandEnabled` (`tview.cpp`).
    pub fn command_enabled(&self, cmd: Command) -> bool {
        !self.disabled_commands.has(cmd)
    }

    /// The rectangle that the desktop's tile and cascade layout operations lay
    /// windows into: the desktop child's local-origin extent `(0, 0, w, h)`.
    ///
    /// Returns `None` if no desktop was created. The `TILE` and `CASCADE` command
    /// handlers read this rect so window layout stays within the desktop area even
    /// when the desktop is inset below a menu bar or above a status line. If you
    /// want to restrict tiling to a sub-region, size the desktop accordingly rather
    /// than overriding this method.
    ///
    /// Requires `&mut self` because `Group::find_mut` requires `&mut`.
    ///
    /// # Turbo Vision heritage
    /// Ports `TApplication::getTileRect` (`tapplica.cpp`).
    pub fn get_tile_rect(&mut self) -> Option<Rect> {
        let id = self.desktop?;
        self.group.find_mut(id).map(|v| v.state().get_extent())
    }

    // -- the run loop --------------------------------------------------------

    /// Drive the event loop until the application posts a quit command that
    /// passes validation, then return it.
    ///
    /// This is the outer loop of a tvision-rs application. Call it once from
    /// `main` after building the [`Program`] and inserting startup views:
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
    /// Each iteration of [`pump_once`](Self::pump_once) picks one event,
    /// routes it through the view tree, applies deferred effects, then
    /// redraws and diffs. The outer loop re-runs the entire validate/pump
    /// cycle if a view refuses the end state (e.g. an unsaved-file guard
    /// that cancels the quit).
    ///
    /// For app-level command handling use
    /// [`run_app`](Self::run_app) instead; for a single pump step (tests,
    /// headless drivers) use [`pump_once`](Self::pump_once).
    ///
    /// With a production `SystemClock` + a real backend, polling for an event
    /// blocks, so this does not spin. **Do not call on a headless backend
    /// without a QUIT path** — headless never blocks, so it would busy-loop;
    /// tests step [`pump_once`](Self::pump_once) instead.
    ///
    /// See also: [the single modal loop](../../../port/modal.html#the-modal-loop-execute)
    /// in the tvision-rs guide.
    ///
    /// # Turbo Vision heritage
    /// Ports `TProgram::run` → `TGroup::execute` (`tprogram.cpp` / `tgroup.cpp`).
    /// The per-group nested loop of the original is replaced by a single top-level
    /// loop plus a [`ModalFrame`] capture handler for modality.
    pub fn run(&mut self) -> Command {
        loop {
            self.end_state = None;
            while self.end_state.is_none() {
                self.pump_and_drive();
            }
            let es = self.end_state.unwrap();
            if self.valid_end(es) {
                return es;
            }
        }
    }

    /// Run the application, calling `on_command` for each [`Command`] that reaches
    /// the program level but is not consumed by any view or the built-in program
    /// handlers (QUIT, TILE, CASCADE, Alt-N window select). This is the hook for
    /// application-level commands such as "File → Color Picker" → `color_dialog`.
    ///
    /// The handler receives `&mut Program` so it can call methods like
    /// [`color_dialog`](Self::color_dialog), [`message_box`](Self::message_box),
    /// and [`input_box`](Self::input_box) in response to menu-driven commands.
    ///
    /// # Example
    /// ```rust,no_run
    /// use tvision_rs::{Color, Command};
    /// const CMD_PICK: Command = Command::custom("my_app.pick_color");
    /// // program.run_app(|prog, cmd| {
    /// //     if cmd == CMD_PICK { prog.color_dialog(Color::Default); }
    /// // });
    /// ```
    pub fn run_app<F>(&mut self, mut on_command: F) -> Command
    where
        F: FnMut(&mut Self, Command),
    {
        loop {
            self.end_state = None;
            while self.end_state.is_none() {
                self.pump_and_drive();
                // Drain any commands that survived all routing — these are meant
                // for the application level (TApplication::handleEvent slot).
                let cmds: Vec<Command> = self.app_commands.drain(..).collect();
                for cmd in cmds {
                    on_command(self, cmd);
                }
            }
            let es = self.end_state.unwrap();
            if self.valid_end(es) {
                return es;
            }
        }
    }

    /// One pump iteration, then drive any modal a view requested during it. The
    /// bare [`pump_once`](Self::pump_once) cannot open a modal — a view's
    /// `OpenHistory` apply arm only stashes the built `HistoryWindow` into
    /// [`pending_modal`](Self::pending_modal), because the apply phase runs inside
    /// the `pump_once` destructure (a split borrow) and a view cannot call
    /// `exec_view` (top-level only). This outer driver, holding a whole `&mut self`,
    /// runs the re-entrant `exec_view` at top level. The `end_state` save/restore in
    /// [`exec_view_with_completion`](Self::exec_view_with_completion) keeps the inner
    /// modal transparent to the enclosing loop.
    ///
    /// Used in place of the bare `pump_once` in **both** [`run`](Self::run)'s inner
    /// `while` AND `exec_view`'s inner `while` (a `THistory` lives in a `Dialog`
    /// usually opened via `exec_view` → this is a modal-from-modal).
    ///
    /// **Quit-from-popup note:** the inner `exec_view`'s result is discarded here
    /// and `end_state` restored, so a quit command ending the *inner* history modal
    /// is swallowed (no app quit from inside the popup). The popup is dismiss-only.
    ///
    /// On an **idle** pass (no input event — [`pump_once`](Self::pump_once)
    /// returned `true`) it fires the user idle hook
    /// ([`set_on_idle`](Self::set_on_idle)). The hook runs **outside** any
    /// `pump_once` destructured borrow: this method holds a whole `&mut self`, so
    /// it takes the boxed `FnMut` out (calling it then holds the only `&mut self`)
    /// and restores it — unless the callback replaced it via `set_on_idle`, in
    /// which case the new box is kept. Returns the `was_idle` bool.
    fn pump_and_drive(&mut self) -> bool {
        let was_idle = self.pump_once();
        if let Some((view, completion, initial_focus)) = self.pending_modal.take() {
            self.exec_view_with_completion(view, Some(completion), initial_focus, None, false);
        }
        if was_idle {
            let mut h = self.on_idle.take();
            if let Some(f) = h.as_mut() {
                f(self);
            }
            // Restore unless the callback replaced it via set_on_idle.
            if self.on_idle.is_none() {
                self.on_idle = h;
            }
        }
        was_idle
    }

    /// The end-command validation gate for the **app run loop**
    /// ([`run`](Self::run)) — the app only ends if the whole root-group tree
    /// validates the end command.
    ///
    /// When a [`FileEditor`](crate::widgets::FileEditor) has unsaved changes,
    /// `group.valid(cmd, ctx)` queues [`Deferred::OpenMessageBox`] and returns `false`.
    /// We drive the box **inline** (we hold `&mut self` between pump iterations) via
    /// [`exec_view_with_completion`](Self::exec_view_with_completion), route the answer
    /// through [`View::set_modal_answer`], and re-validate in a loop. When the user
    /// answers "Yes" to an untitled editor, `save()` queues
    /// [`Deferred::OpenSaveAsDialog`]; that is also driven inline in the same pass,
    /// followed by a `pump_once` to service the re-injected [`Command::SAVE`].
    ///
    /// Anything else in `self.deferred` after the walk is put back for the next pump.
    ///
    /// Mirrors [`validate_modal_close`](Self::validate_modal_close) (the single-view
    /// modal-close twin) but operates on the whole root group.
    ///
    /// # Turbo Vision heritage
    /// Realizes the outer `while( !valid(endState) )` of `TGroup::execute`
    /// (`tgroup.cpp`).
    fn valid_end(&mut self, cmd: Command) -> bool {
        loop {
            // 1. Walk the whole root group.
            let valid = {
                let now = self.clock.now_ms();
                let mut ctx = Context::new(
                    &mut self.out_events,
                    &mut self.timers,
                    now,
                    &mut self.deferred,
                );
                self.group.valid(cmd, &mut ctx)
            };

            // 2. Partition out OpenMessageBox and OpenSaveAsDialog requests queued by valid().
            //    Anything else is unexpected here — put back for the next real pump.
            let drained = std::mem::take(&mut self.deferred);
            let mut requests: Vec<Deferred> = Vec::new();
            for d in drained {
                match d {
                    req @ Deferred::OpenMessageBox { .. } => requests.push(req),
                    req @ Deferred::OpenSaveAsDialog { .. } => requests.push(req),
                    other => self.deferred.push(other),
                }
            }
            if requests.is_empty() {
                return valid;
            }

            // 3. Drive each request INLINE (we hold &mut self). Re-loop only if an
            //    answer was routed (i.e. re-validation is needed).
            let mut revalidate = false;
            for req in requests {
                match req {
                    Deferred::OpenMessageBox {
                        text,
                        kind,
                        buttons,
                        answer_to,
                        then_command: _, // ignored here — we re-loop inline
                    } => {
                        let r = self.centered_msgbox_rect(&text);
                        let (d, first) = crate::dialog::build_message_box(r, &text, kind, buttons);
                        let (answer, _) =
                            self.exec_view_with_completion(Box::new(d), None, first, None, false);
                        if let Some(target) = answer_to {
                            if let Some(v) = self.group.find_mut(target) {
                                v.set_modal_answer(answer);
                            }
                            revalidate = true;
                        }
                    }
                    Deferred::OpenSaveAsDialog { editor_id } => {
                        if self.drive_save_as_inline(editor_id) {
                            revalidate = true;
                        }
                    }
                    _ => unreachable!("partitioned to OpenMessageBox / OpenSaveAsDialog only"),
                }
            }
            if !revalidate {
                return valid;
            }
        }
    }

    // -- exec_view: the blocking modal wrapper --------------------------------

    /// Run `view` as a modal dialog: insert it into the root group, pump the event
    /// loop until the view (or the user) posts an end command, then remove it and
    /// return that command.
    ///
    /// Call this from an app `main` or test harness when you need a blocking
    /// modal — e.g. an open-file dialog at startup, a settings panel, or a
    /// confirm-quit box triggered by app-level code. For the common built-in dialogs
    /// prefer the typed wrappers: [`message_box`](Self::message_box),
    /// [`input_box`](Self::input_box), [`open_file_dialog`](Self::open_file_dialog).
    ///
    /// **Top-level only — the type system enforces it:** a [`View`] holds only
    /// `&mut Context`, never `&mut Program`, so a view *cannot* call this from
    /// inside `handle_event` (which is what keeps the nested
    /// [`pump_once`](Self::pump_once) loop sound). Call from an app `main`,
    /// startup, or a test driving pre-queued events. A view-/menu-triggered modal
    /// instead requests one through the deferred channel
    /// (`Deferred::OpenModal` + a posted completion command).
    ///
    /// **Program-level handling runs during the modal pump.** Under the single
    /// loop, the nested [`pump_once`](Self::pump_once) calls below still run
    /// [`program_handle_event`] every iteration — so the Alt-N window-selection
    /// block and the quit catch are live *during* the modal. A `Command::QUIT`
    /// arriving while a modal is open therefore ends the modal (with `QUIT`) and
    /// quits the app, even from inside a dialog. This is intentional UX (and a
    /// deliberate deviation from the original's modal dispatch, which would have
    /// discarded it and left the modal open).
    ///
    /// **HEADLESS HANG WARNING:** [`pump_once`](Self::pump_once) does not block on a
    /// headless backend, so the inner `while end_state.is_none()` loop spins until
    /// something sets `end_state`. The caller MUST ensure the modal reaches
    /// [`Context::end_modal`] (e.g. a pre-queued OK/Cancel, or an Esc that a
    /// [`Dialog`](crate::dialog::Dialog) turns into a posted cancel). A modal with
    /// no path to `end_modal` hangs.
    ///
    /// Control flow:
    /// 1. Save the current focus + a clone of the command set.
    /// 2. **Insert** the view into the root group. Insert FIRST so `set_current`
    ///    can resolve the id.
    /// 3. Clear the selectable option on the view (so it is not part of normal
    ///    focus rotation while modal).
    /// 4. Set the modal state bit **directly** (it does not propagate to children).
    /// 5. Make the view current — selects + focuses it (fires its command enables,
    ///    deferred; unwound by the command-set restore in step 9).
    /// 6. Push the [`ModalFrame`] directly (we hold `&mut self`, not inside a
    ///    dispatch).
    /// 7. The loop: `loop { end_state = None; while none { pump_once }; if the
    ///    MODAL view's own valid(es) break es }` — validate against the modal
    ///    view's own `valid`, NOT the root group's.
    /// 8. Pop the frame, remove the view, restore the saved focus.
    /// 9. Restore the command set.
    ///
    /// See also: [the modal chapter](../../../port/modal.html#ending-a-modal-execview)
    /// in the tvision-rs guide.
    ///
    /// # Turbo Vision heritage
    /// Ports `TGroup::execView` + `TGroup::execute` (`tgroup.cpp`), run on the
    /// owner program group. The blocking nested loop becomes a single pump plus a
    /// [`ModalFrame`] capture handler (deviation D9).
    pub fn exec_view(&mut self, view: Box<dyn View>) -> Command {
        self.exec_view_with_completion(view, None, None, None, false)
            .0
    }

    // -- message box ---------------------------------------------------------

    /// Build and run a message-box dialog at an explicit `Rect`. Construction and
    /// execution are split: `build_message_box` builds the dialog (pure, testable),
    /// and `exec_view` runs it.
    ///
    /// `kind` picks the title (Warning / Error / Information / Confirm).
    /// `buttons` selects which of [Yes, No, OK, Cancel] to show.
    ///
    /// Returns the [`Command`] the user chose (`Command::OK`, `Command::CANCEL`,
    /// `Command::YES`, `Command::NO`).
    ///
    /// # Turbo Vision heritage
    /// Ports `messageBoxRect` (`msgbox.cpp`).
    pub fn message_box_rect(
        &mut self,
        r: Rect,
        msg: &str,
        kind: crate::dialog::MessageBoxKind,
        buttons: crate::dialog::MessageBoxButtons,
    ) -> Command {
        let (d, first_btn) = crate::dialog::build_message_box(r, msg, kind, buttons);
        self.exec_view_with_completion(Box::new(d), None, first_btn, None, false)
            .0
    }

    /// Build and run a message-box dialog auto-centered on the desktop.
    ///
    /// Sizing logic:
    /// * Base rect `(0, 0, 40, 9)`.
    /// * If `msg.chars().count() > (40-7) * (9-6)`, expand the height:
    ///   `h = char_count / (40-7) + 6 + 1`.
    /// * Center within the desktop's size (or the root group's size if no
    ///   desktop was created).
    ///
    /// **Coordinate note:** `exec_view` root-inserts the modal, so the rect is
    /// in absolute/root coords, while centering uses the desktop's SIZE. When the
    /// desktop is inset by a menu/status bar, the centered box can therefore sit
    /// off by the menu-bar offset.
    ///
    /// # Turbo Vision heritage
    /// Ports `messageBox` + its `makeRect` helper (`msgbox.cpp`).
    pub fn message_box(
        &mut self,
        msg: &str,
        kind: crate::dialog::MessageBoxKind,
        buttons: crate::dialog::MessageBoxButtons,
    ) -> Command {
        let r = self.centered_msgbox_rect(msg);
        self.message_box_rect(r, msg, kind, buttons)
    }

    /// The message-box sizing + desktop-centering, factored out so both
    /// [`message_box`](Self::message_box) and the async-modal-from-a-view drain
    /// ([`Deferred::OpenMessageBox`](crate::view::Deferred::OpenMessageBox)) build
    /// the box at the same centered rect.
    fn centered_msgbox_rect(&mut self, msg: &str) -> Rect {
        centered_msgbox_rect_for(&self.group, self.desktop, msg)
    }

    /// The desktop view's SIZE, used to center modal standard dialogs
    /// ([`message_box`](Self::message_box) / [`input_box`](Self::input_box)).
    /// Falls back to the root group's size if no desktop was created. The actual
    /// centering move stays in each caller.
    fn desktop_size(&mut self) -> Point {
        if let Some(id) = self.desktop {
            self.group
                .find_mut(id)
                .map(|v| v.state().size)
                .unwrap_or_else(|| self.group.state().size)
        } else {
            self.group.state().size
        }
    }

    // -- input box -----------------------------------------------------------

    /// Build and run a single-line input dialog at an explicit `Rect`. The modal
    /// insertion / teardown lives here in [`Program`] and the typed value currency
    /// carries the scatter (seed) / gather (read-back). Construction is split out
    /// into the pure [`build_input_box`](crate::dialog::build_input_box) builder.
    ///
    /// `label` is the prompt drawn left of the field; `initial` is the starting
    /// text, seeded into the input line with select-all; `limit` caps the field's
    /// byte length (max length = `limit - 1`).
    ///
    /// Returns `(cmd, text)` where `cmd` is the end [`Command`] (`Command::OK` /
    /// `Command::CANCEL`). On a non-cancel result, `text` is the field's final
    /// contents; on cancel, `text` is the unchanged `initial`.
    ///
    /// **Single-field shortcut (not the general group-walk).** The input box has
    /// exactly one transferable field (the lone
    /// [`InputLine`](crate::widgets::InputLine)), so scatter = `set_value` on it
    /// and gather = `value()` on it. The general `Dialog` gather/scatter group-walk
    /// is supplied by its multi-field consumers.
    ///
    /// # Turbo Vision heritage
    /// Ports `inputBoxRect` (`msgbox.cpp`); the value currency replaces the
    /// original's `setData`/`getData` transfer (deviation D10).
    pub fn input_box_rect(
        &mut self,
        bounds: Rect,
        title: &str,
        label: &str,
        initial: &str,
        limit: i32,
    ) -> (Command, String) {
        let (mut d, input_id) = crate::dialog::build_input_box(bounds, title, label, limit);

        // Scatter (C++ `dialog->setData(s)`): seed the lone input line with the
        // initial text via the typed value currency.
        if let Some(v) = d.find_mut(input_id) {
            v.set_value(crate::data::FieldValue::Text(initial.to_string()));
        }

        // initial_focus AND gather are both the input line: selectNext(False)
        // focuses the first selectable child (the input line), and getData reads
        // it back out.
        let (cmd, gathered) = self.exec_view_with_completion(
            Box::new(d),
            None,
            Some(input_id),
            Some(input_id),
            false,
        );

        // On cancel/none, `s` is unchanged (faithful) → return `initial`.
        let text = match gathered {
            Some(crate::data::FieldValue::Text(s)) => s,
            // None = cancel (or no gather); Some(Int) is unreachable — InputLine only yields Text.
            _ => initial.to_string(),
        };
        (cmd, text)
    }

    /// Build and exec a single-line input dialog auto-centered on the desktop.
    ///
    /// Convenience wrapper around [`input_box_rect`](Self::input_box_rect) that
    /// computes the position automatically (base rect `60 × 8`, centered within
    /// the desktop). Use this when you don't need precise placement; use
    /// `input_box_rect` when you need an explicit position.
    ///
    /// `title` is the dialog frame label. `label` is the prompt drawn left of the
    /// field. `initial` seeds the input line (the entire text is pre-selected so
    /// the user can immediately type a replacement). `limit` caps the field byte
    /// length (max length = `limit - 1`).
    ///
    /// Returns `(cmd, text)`. If the user presses OK, `cmd = Command::OK` and
    /// `text` holds what they typed. If the user presses Cancel or Esc,
    /// `cmd = Command::CANCEL` and `text` is the unchanged `initial`.
    ///
    /// **Coordinate note:** centering uses the desktop's SIZE; when the desktop is
    /// inset by a menu/status bar, the box can sit off by the menu-bar offset.
    ///
    /// # Turbo Vision heritage
    ///
    /// Ports `inputBox` (`msgbox.cpp`); the size offset is `deskTop->size.x/y`.
    pub fn input_box(
        &mut self,
        title: &str,
        label: &str,
        initial: &str,
        limit: i32,
    ) -> (Command, String) {
        // C++: TRect r(0, 0, 60, 8); r.move((deskTop->size.x - 60)/2, (size.y - 8)/2).
        let mut r = Rect::new(0, 0, 60, 8);
        let desk_size = self.desktop_size();
        r.r#move((desk_size.x - 60) / 2, (desk_size.y - 8) / 2);
        self.input_box_rect(r, title, label, initial, limit)
    }

    /// Open the truecolor color-picker modal seeded with `initial`; return the
    /// chosen [`Color`](crate::color::Color) on OK, or `None` on Cancel/Esc.
    ///
    /// The picker presents four surface tabs (Presets, RGB, Plane, Xterm-256),
    /// an Info column showing old and new swatches, and OK/Cancel buttons. Call
    /// this from your application's event handler when the user selects a
    /// "Pick Color" menu item or similar:
    ///
    /// ```rust,ignore
    /// let new_color = program.color_dialog(current_color);
    /// if let Some(c) = new_color {
    ///     // apply c
    /// }
    /// ```
    ///
    /// An tvision-rs-original extension (no direct Borland/magiblot counterpart).
    /// The chosen color is read out of the modal's own
    /// [`ColorPicker`](crate::dialog::ColorPicker) and returned **by value** via
    /// [`exec_view_with`](Self::exec_view_with)-style capture. `Color` is
    /// deliberately not a `FieldValue` (a 4-variant enum, not a packable scalar;
    /// spec non-goal).
    ///
    /// # Turbo Vision heritage
    /// Supersedes `TColorDialog` (guide pp. 406–409), which was an interactive
    /// editor for the 16-entry BIOS palette. tvision-rs replaces it with a
    /// truecolor picker that returns an `Option<Color>` directly instead of
    /// mutating an in-memory `TPalette` blob.
    pub fn color_dialog(&mut self, initial: crate::color::Color) -> Option<crate::color::Color> {
        use crate::dialog::{ColorPicker, Dialog};
        use crate::widgets::{Button, ButtonFlags};

        // 60 x 23 dialog, centered on the desktop (mirrors input_box centering).
        let mut r = Rect::new(0, 0, 60, 23);
        let desk = self.desktop_size();
        r.r#move((desk.x - 60) / 2, (desk.y - 23) / 2);
        let mut d = Dialog::new(r, Some("Select Color".to_string()));

        let picker_id =
            d.insert_child(Box::new(ColorPicker::new(Rect::new(2, 2, 58, 20), initial)));
        d.insert_child(Box::new(Button::new(
            Rect::new(20, 20, 30, 22),
            "O~K~",
            Command::OK,
            ButtonFlags {
                default: true,
                ..Default::default()
            },
        )));
        d.insert_child(Box::new(Button::new(
            Rect::new(31, 20, 41, 22),
            "~C~ancel",
            Command::CANCEL,
            ButtonFlags::default(),
        )));

        // Read the chosen color out of the modal's own ColorPicker child by value
        // at close (spec §6.6: a helper reaching its own known child; Color is not
        // a FieldValue, spec C-1). No Rc sink, no ModalCompletion variant.
        self.exec_view_capture(
            Box::new(d),
            None,
            Some(picker_id),
            None,
            false,
            |modal, cmd| {
                if cmd == Command::OK {
                    modal
                        .find_mut(picker_id)
                        .and_then(|v| v.as_any_mut())
                        .and_then(|a| a.downcast_mut::<ColorPicker>())
                        .map(|p| p.color())
                } else {
                    None
                }
            },
        )
        .2
    }

    /// Install a new theme and force a full repaint (theme editor).
    pub fn set_theme(&mut self, theme: crate::theme::Theme) {
        self.theme = theme;
        self.renderer.invalidate_all();
    }

    /// Open the theme editor dialog. On OK, installs the modified theme via
    /// [`set_theme`](Self::set_theme). On cancel, does nothing.
    ///
    /// Entry point for an app's "edit colors" menu command.
    pub fn theme_editor(&mut self) {
        use crate::dialog::{Dialog, ThemeEditorBody};
        use crate::widgets::{Button, ButtonFlags};

        let current_theme = self.theme.clone();

        // 64-wide × 24-tall dialog, centered on the desktop.
        let mut r = Rect::new(0, 0, 64, 24);
        let desk = self.desktop_size();
        r.r#move((desk.x - 64) / 2, (desk.y - 24) / 2);
        let mut d = Dialog::new(r, Some("Theme Editor".to_string()));

        // ThemeEditorBody fills the content area: x=1..63, y=1..19.
        let te_id = d.insert_child(Box::new(ThemeEditorBody::new(
            Rect::new(1, 1, 63, 19),
            current_theme,
        )));

        // Fg / Bg edit buttons.
        d.insert_child(Box::new(Button::new(
            Rect::new(2, 20, 12, 22),
            "~F~g",
            Command::THEME_EDIT_FG,
            ButtonFlags::default(),
        )));
        d.insert_child(Box::new(Button::new(
            Rect::new(13, 20, 23, 22),
            "~B~g",
            Command::THEME_EDIT_BG,
            ButtonFlags::default(),
        )));

        // OK / Cancel buttons.
        d.insert_child(Box::new(Button::new(
            Rect::new(42, 20, 52, 22),
            "O~K~",
            Command::OK,
            ButtonFlags {
                default: true,
                ..Default::default()
            },
        )));
        d.insert_child(Box::new(Button::new(
            Rect::new(53, 20, 63, 22),
            "~C~ancel",
            Command::CANCEL,
            ButtonFlags::default(),
        )));

        // Read the edited working theme out of the modal's own ThemeEditorBody by
        // value at close (spec §6.6: a helper reaching its own known child; a whole
        // Theme is too large to be a FieldValue, spec C-1). No Rc sink, no variant.
        let new_theme = self
            .exec_view_capture(Box::new(d), None, Some(te_id), None, false, |modal, cmd| {
                if cmd == Command::OK {
                    modal
                        .find_mut(te_id)
                        .and_then(|v| v.as_any_mut())
                        .and_then(|a| a.downcast_mut::<ThemeEditorBody>())
                        .map(|te| te.working_theme().clone())
                } else {
                    None
                }
            })
            .2;
        if let Some(new_theme) = new_theme {
            self.set_theme(new_theme);
        }
    }

    /// The desktop's local extent `(0,0,w,h)`. Use it to compute bounds for
    /// windows opened at runtime (e.g. `CMD_NEW`). Returns a zero rect when no
    /// desktop was created.
    pub fn desktop_rect(&mut self) -> Rect {
        let size = self.desktop_size();
        Rect::new(0, 0, size.x, size.y)
    }

    /// Insert `view` into the desktop and make it the current (focused) window.
    /// This is the standard way to open a new window at runtime: call it from your
    /// `run_app` command handler (e.g. on `CMD_NEW`) with a freshly constructed
    /// [`Window`](crate::window::Window) or `Dialog`.
    ///
    /// Returns the new view's [`ViewId`] on success, or `None` if no desktop was
    /// created or the desktop downcast fails.
    ///
    /// # Turbo Vision heritage
    ///
    /// C++ `TProgram::insertWindow` (`tprogram.cpp`) disposes a window being
    /// inserted when the active window cannot release focus (`validView` /
    /// `canMoveFocus`). tvision-rs does **not** gate a programmatic insert: the
    /// focus-release check (`valid(RELEASED_FOCUS)`) is applied where it matters
    /// interactively — Alt-N window selection and modal close — not on insert. An
    /// app that inserts a window expects it to appear; refusing the insert
    /// (DOS-era behavior) would surprise more than it protects.
    pub fn desktop_insert(&mut self, view: Box<dyn View>) -> Option<ViewId> {
        let desk_id = self.desktop?;
        let now = self.clock.now_ms();
        let mut ctx = Context::new(
            &mut self.out_events,
            &mut self.timers,
            now,
            &mut self.deferred,
        );
        let dt = self.group.find_mut(desk_id)?;
        let desk = dt.as_any_mut()?.downcast_mut::<Desktop>()?;
        Some(desk.insert_and_focus(view, &mut ctx))
    }

    /// Show a file-open dialog and return the chosen [`PathBuf`], or `None` on
    /// cancel. Runs a [`FileDialog`](crate::dialog::FileDialog) modally and reads
    /// back the resolved filename.
    ///
    /// `wild_card` is the initial filename pattern (e.g. `"*.*"`), `title` is the
    /// dialog caption (e.g. `"Open a File"`).
    pub fn open_file_dialog(&mut self, title: &str, wild_card: &str) -> Option<std::path::PathBuf> {
        use crate::data::FieldValue;
        use crate::dialog::{FD_OPEN_BUTTON, FileDialog};
        let fd = FileDialog::new(wild_card, title, "~N~ame", FD_OPEN_BUTTON, 100);
        // gather_self = true: pre-mints the dialog's id and reads FileDialog::value()
        // (FieldValue::Text(resolved_name)) while the modal is still in the tree.
        let (cmd, gathered) = self.exec_view_with_completion(Box::new(fd), None, None, None, true);
        if cmd != Command::CANCEL
            && let Some(FieldValue::Text(name)) = gathered
        {
            return Some(std::path::PathBuf::from(name));
        }
        None
    }

    /// Execute `view` as a modal, applying an optional completion and gathering
    /// an optional field — the no-result-extraction entry point. See
    /// [`exec_view_capture`](Self::exec_view_capture) for the generic core.
    fn exec_view_with_completion(
        &mut self,
        view: Box<dyn View>,
        completion: Option<ModalCompletion>,
        initial_focus: Option<ViewId>,
        gather: Option<ViewId>,
        gather_self: bool,
    ) -> (Command, Option<crate::data::FieldValue>) {
        let (cmd, gathered, ()) = self.exec_view_capture(
            view,
            completion,
            initial_focus,
            gather,
            gather_self,
            |_, _| (),
        );
        (cmd, gathered)
    }

    /// Execute `view` as a modal and return a caller-typed result extracted from
    /// the finished modal **by value** — no shared `Rc` cell and no `dyn Any` in
    /// the framework. `extract` runs once, at modal close, receiving the modal's
    /// own `&mut dyn View` and the end [`Command`] while the view is still in the
    /// tree; whatever it returns is handed straight back to the caller.
    ///
    /// `R` is named by the caller and never by the framework. A consumer that
    /// needs a single field can read it through [`View::value`]; a consumer that
    /// needs a richer native value (a [`Color`](crate::color::Color), a whole
    /// [`Theme`](crate::theme::Theme)) returns it directly from `extract`.
    ///
    /// # Turbo Vision heritage
    /// The value-returning twin of `TGroup::execView` (`tgroup.cpp:188`), which
    /// returns a `ushort` end command to its method caller. Where C++ then reads
    /// results out of the still-live dialog with `getData`, `extract` reads them
    /// by value here.
    pub fn exec_view_with<R>(
        &mut self,
        view: Box<dyn View>,
        extract: impl FnOnce(&mut dyn View, Command) -> R,
    ) -> R {
        self.exec_view_capture(view, None, None, None, false, extract)
            .2
    }

    /// The unified `exec_view` body. Identical to the simple `exec_view` except
    /// for three additions the view-triggered async-modal seam needs:
    ///
    /// * **`end_state` save/restore** for re-entrancy. A `THistory` lives in a
    ///   `Dialog` usually opened via `exec_view`, so this is a modal-from-modal.
    ///   Without save/restore, when the inner `exec_view` returns,
    ///   `self.end_state` still holds the inner end command and the **outer**
    ///   `while self.end_state.is_none()` would spuriously exit. The modal still
    ///   **returns** its end command (a quit command is unchanged — the
    ///   quit-ends-modal deviation still holds); only the leftover `self.end_state`
    ///   is restored to the enclosing loop's value.
    /// * **the completion**, run after the loop breaks but BEFORE remove/drop,
    ///   while the modal is still in the tree by `id` (so it can read its final
    ///   selection). It is a DIRECT `group` mutation, NOT a deferred queue
    ///   entry — the deferred drain in `pump_once` fires only when a `Some(ev)`
    ///   pump pass runs, and would never fire from here in a headless test.
    /// * **the `gather`** seam. If `Some(gid)` and the result is not
    ///   [`Command::CANCEL`], the value of the view with that id is read
    ///   (`View::value`, the typed currency) while the modal is still in the tree
    ///   by id, and returned as the second tuple element. On cancel/`None` the
    ///   result is `None` (the caller leaves its input unchanged).
    fn exec_view_capture<R>(
        &mut self,
        view: Box<dyn View>,
        completion: Option<ModalCompletion>,
        initial_focus: Option<ViewId>,
        gather: Option<ViewId>,
        gather_self: bool,
        extract: impl FnOnce(&mut dyn View, Command) -> R,
    ) -> (Command, Option<crate::data::FieldValue>, R) {
        // 1. getCommands / save the outgoing current.
        let save_current = self.group.current();
        let save_commands = self.disabled_commands.clone();
        // end_state save/restore (REQUIRED for re-entrancy — see the doc above).
        // Take it at ENTRY, before the retval loop's `self.end_state = None`.
        let saved_end_state = self.end_state.take();

        // 2. Insert FIRST (always own it: saveOwner == 0). Insert before
        //    set_current so the group can resolve the id (set_current resolves via
        //    index_of and is a silent no-op for an absent id).
        //
        // ROOT-INSERT DEVIATION: this inserts the modal into the ROOT group — the
        // modal becomes a sibling of the desktop. Faithful to C++
        // `application->execView(pD)` (msgbox.cpp:90/186 use exactly this). The
        // alternative C++ pattern, `TProgram::executeDialog`, uses
        // `deskTop->execView(pD)` (tprogram.cpp:119) — the desktop variant, which
        // inserts into the desktop instead. tvision-rs root-inserts. When the desktop is
        // inset by a menu/status bar, a desktop-inset modal would need to clip to
        // the desktop region, compounding the `ModalFrame` (0,0)-coordinate caveat.
        // When `gather_self` is true we pre-mint the id and insert with that id so
        // the gather step below can read the modal's OWN `value()` by id.
        let (id, effective_gather) = if gather_self {
            let pre_id = ViewId::next();
            self.group.insert_with_id(view, pre_id);
            (pre_id, Some(pre_id))
        } else {
            (self.group.insert(view), gather)
        };

        // The modal view's bounds in the root group's frame, for the ModalFrame
        // hit-test. The root group is at (0,0), so group-local == absolute (the
        // same ModalFrame coordinate caveat).
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
        //      — the view is dropped on remove (step 8). Clearing ofSelectable here
        //      also means the step-8 `Group::remove` never fires the stage-4
        //      visible+selectable removal tail for the modal: its reset_current
        //      runs through the `was_current` leg instead (the modal was
        //      set_current'd in step 5), exactly the pre-stage-4 behavior.
        if let Some(v) = self.group.find_mut(id) {
            let st = v.state_mut();
            st.options.selectable = false;
            st.state.modal = true;
        }

        // 5. setCurrent(p, enterSelect). enterSelect does not deselect the old
        //    current (the desktop stays selected beneath). Build a throwaway
        //    Context over the disjoint fields (the pump's discipline).
        //
        // 5a. THE FAITHFUL OPEN HOOK (not a compensation). This
        // VIRTUAL `reset_current` on the freshly-inserted modal is the C++
        // open-time `insertView → show → resetCurrent` for the modal itself, and
        // it must stay even though the pump's settle_currency pass now
        // covers plain inserts, because:
        //   - it carries the VIRTUAL overrides' one-time init —
        //     `FileDialog::reset_current`'s initial `readDirectory` (filedlg.rs)
        //     and `ChDirDialog`'s `setUpDialog` — which the settle pass cannot
        //     reach (settle runs the INHERENT `Group::reset_current` by design);
        //   - it must run BEFORE the `set_current(Enter)` focus below (so focus
        //     cascades into the modal's first selectable child) and before
        //     `initial_focus`, i.e. earlier than the next pump's settle;
        //   - it clears the modal group's `currency_dirty` flag (reset_current →
        //     set_current), so the settle pass never double-runs on this modal
        //     and cannot clobber the `initial_focus` applied below.
        {
            let now = self.clock.now_ms();
            let mut ctx = Context::new(
                &mut self.out_events,
                &mut self.timers,
                now,
                &mut self.deferred,
            );
            // Establish the modal's INTERNAL currency (selected, unfocused) so
            // that when set_current(Enter) focuses the modal, focus cascades
            // into its first selectable child. Without this the modal opens
            // keyboard-dead until the next pump's settle (an immediate Esc/Enter
            // queued before the open would reach no child).
            if let Some(v) = self.group.find_mut(id) {
                v.reset_current(&mut ctx);
            }
            self.group
                .set_current(Some(id), SelectMode::Enter, &mut ctx);
        }

        // 6. Push the ModalFrame DIRECTLY (we hold &mut self; we are not inside a
        //    dispatch, so this is not deferred).
        self.captures.push(Box::new(ModalFrame::new(id, bounds)));

        // selectNext(False) faithfulness (msgbox): the caller asked for a SPECIFIC
        // child to be focused on open (e.g. messageBox's first button), overriding
        // the generic reset_current(firstMatch). The modal is already inserted +
        // focused, so focus_descendant moves internal focus to focus_id within the
        // dialog's group.
        if let Some(focus_id) = initial_focus {
            let now = self.clock.now_ms();
            let mut ctx = Context::new(
                &mut self.out_events,
                &mut self.timers,
                now,
                &mut self.deferred,
            );
            if let Some(v) = self.group.find_mut(id) {
                v.focus_descendant(focus_id, &mut ctx);
            }
        }

        // 7. TGroup::execute — drive the single pump in a bounded top-level loop.
        //    The inner while spins on a headless backend until the modal sets
        //    end_state (the HEADLESS HANG WARNING above); the outer loop re-runs if
        //    valid_end refuses the end command (TGroup::execute's while(!valid)).
        let retval = loop {
            self.end_state = None;
            while self.end_state.is_none() {
                // DEVIATION (see this fn's doc): pump_once runs
                // program_handle_event each pass, so Alt-N + the cmQuit catch are
                // live during the modal. C++ execView -> p->execute() (tgroup.cpp:205)
                // dispatches to the dialog's handleEvent, NOT TProgram::handleEvent
                // (where cmQuit->endModal + Alt-N live, tprogram.cpp:205) — so program
                // handling is out of the modal dispatch path there. We keep ours.
                //
                // pump_and_drive: a THistory inside this modal dialog can
                // itself request a modal (the history popup), driven re-entrantly at
                // top level here after each pump.
                self.pump_and_drive();
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
            //
            // ASYNC-MODAL-FROM-A-VIEW (the modal-close path, §6 of the design note):
            // the modal view's `valid` may request a `messageBox` (e.g. a FileEditor
            // modified-save prompt). The deferred drain is event-gated inside
            // pump_once and would NEVER fire here, so validate_modal_close DRIVES the
            // box inline (we hold &mut self) and re-validates in a loop.
            let valid = self.validate_modal_close(id, es);
            if valid {
                break es;
            }
        };

        // Run the completion BEFORE remove/drop, while the modal is still in the
        // tree by `id` (so it can read e.g. get_selection). Direct group mutation —
        // NOT the deferred queue (that drain fires only when a Some(ev) pump pass
        // runs inside pump_once, and would never fire here in a headless test).
        if let Some(c) = completion {
            // RouteModalAnswer returns a `then_command` event to re-post (the async
            // modal-from-a-view round-trip); push it into the re-inject queue so the
            // next pump pops it (program.rs pump_once pops out_events before polling).
            if let Some(reinject) = apply_modal_completion(c, retval, &mut self.group, id) {
                self.out_events.push_back(reinject);
            }
        }

        // C++ inputBox: `if (c != cmCancel) dialog->getData(s)`. Read the gather
        // target's value while the modal is still in the tree by id, before drop.
        let gathered = if retval != Command::CANCEL {
            effective_gather.and_then(|gid| self.group.find_mut(gid).and_then(|v| v.value()))
        } else {
            None
        };

        // Extract the caller's typed result while the modal is STILL in the tree
        // by `id` — the same pre-drop window as the completion + gather above.
        // The modal is guaranteed present: inserted in step 2, removed only below.
        let extracted = {
            let modal = self
                .group
                .find_mut(id)
                .expect("modal is in the tree by id until remove() below");
            extract(modal, retval)
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
        // on a dropped object is unobservable, so it is intentionally omitted.

        // 9. setCommands(saveCommands): restore the command set. Restoring is not an
        //    app-visible toggle the way enable/disable is, so we do NOT set
        //    command_set_changed (no re-broadcast): the modal's command enables were
        //    transient and unwinding them is internal bookkeeping, not a state the
        //    app reacts to.
        //
        //    DEVIATION: C++ TView::setCommands DOES set commandSetChanged when the
        //    sets differ — and here they do differ (the modal enabled
        //    cmNext/cmPrev/cmClose/cmZoom), so C++ fires a post-modal
        //    cmCommandSetChanged broadcast that tvision-rs omits. Deliberate: the
        //    command set is restored wholesale on modal exit.
        self.disabled_commands = save_commands;

        // Restore the enclosing loop's end_state (re-entrancy — see the doc above).
        // The modal's own end command lives in `retval` (the source of truth); the
        // leftover `self.end_state` must NOT leak out to end an outer modal/run loop.
        self.end_state = saved_end_state;

        // TheTopView dropped (no occlusion/exposed tracking); no consumer.

        (retval, gathered, extracted)
    }

    /// The end-command validation gate for the **modal-close path** (§6 of
    /// `docs/design/async-modal-from-view.md`) — the asymmetric twin of the
    /// handle_event paths.
    ///
    /// We are BETWEEN pump iterations (called from `exec_view_capture`'s
    /// retval loop, holding `&mut self`), so the event-gated deferred drain inside
    /// [`pump_once`](Self::pump_once) will NEVER fire here. A modal view's `valid`
    /// (e.g. a [`FileEditor`](crate::widgets::FileEditor) modified-save prompt) that
    /// requests a [`OpenMessageBox`](crate::view::Deferred::OpenMessageBox) would
    /// hang forever waiting for that drain. So this DRIVES the box inline (via the
    /// re-entrant [`exec_view_with_completion`](Self::exec_view_with_completion)),
    /// routes the answer through [`View::set_modal_answer`], and re-validates in a
    /// loop. The `then_command` carried by the request is IGNORED here (we re-loop
    /// inline instead of re-posting it — the whole two-path asymmetry). When `save()`
    /// queues a [`Deferred::OpenSaveAsDialog`] (untitled file), the FileDialog is
    /// driven inline with [`ModalCompletion::SaveAsPick`], followed by a `pump_once`
    /// to service the re-injected [`Command::SAVE`].
    fn validate_modal_close(&mut self, id: ViewId, es: Command) -> bool {
        loop {
            // 1. Run the modal view's own valid (carries &mut Context for any request).
            let valid = {
                let now = self.clock.now_ms();
                let mut ctx = Context::new(
                    &mut self.out_events,
                    &mut self.timers,
                    now,
                    &mut self.deferred,
                );
                self.group
                    .find_mut(id)
                    .map(|v| v.valid(es, &mut ctx))
                    .unwrap_or(true)
            };

            // 2. Partition out any OpenMessageBox and OpenSaveAsDialog requests valid()
            //    queued. Anything else in `deferred` here is unexpected (no event drove
            //    it) — keep it by re-pushing so the next real pump drains it.
            let drained = std::mem::take(&mut self.deferred);
            let mut requests: Vec<Deferred> = Vec::new();
            for d in drained {
                match d {
                    req @ Deferred::OpenMessageBox { .. } => requests.push(req),
                    req @ Deferred::OpenSaveAsDialog { .. } => requests.push(req),
                    other => self.deferred.push(other),
                }
            }
            if requests.is_empty() {
                return valid;
            }

            // 3. Drive each requested box INLINE (we hold &mut self), routing the
            //    answer back to its requester. Re-loop only if an answer was routed
            //    (an informational `answer_to == None` box just shows, then we keep
            //    the current — false — valid).
            let mut revalidate = false;
            for req in requests {
                match req {
                    Deferred::OpenMessageBox {
                        text,
                        kind,
                        buttons,
                        answer_to,
                        then_command: _,
                    } => {
                        let r = self.centered_msgbox_rect(&text);
                        let (d, first) = crate::dialog::build_message_box(r, &text, kind, buttons);
                        let (answer, _) =
                            self.exec_view_with_completion(Box::new(d), None, first, None, false);
                        if let Some(target) = answer_to {
                            if let Some(v) = self.group.find_mut(target) {
                                v.set_modal_answer(answer);
                            }
                            revalidate = true;
                        }
                    }
                    Deferred::OpenSaveAsDialog { editor_id } => {
                        if self.drive_save_as_inline(editor_id) {
                            revalidate = true;
                        }
                    }
                    _ => unreachable!("partitioned to OpenMessageBox / OpenSaveAsDialog only"),
                }
            }
            if !revalidate {
                return valid;
            }
        }
    }

    /// Drive a `SaveAs` [`FileDialog`](crate::dialog::FileDialog) for `editor_id`
    /// inline (we hold `&mut self` between pump iterations).
    ///
    /// Used by both [`valid_end`](Self::valid_end) and
    /// [`validate_modal_close`](Self::validate_modal_close) when a `valid()` call
    /// queues [`Deferred::OpenSaveAsDialog`]. Builds the "save as" dialog,
    /// pre-fills the input with the editor's current filename, runs it via
    /// [`exec_view_with_completion`](Self::exec_view_with_completion) with
    /// [`ModalCompletion::SaveAsPick`], and calls [`pump_once`](Self::pump_once)
    /// to service the re-injected [`Command::SAVE`] on accept.
    ///
    /// Returns `true` if the dialog was accepted (revalidation needed), `false`
    /// on Cancel (no revalidation).
    fn drive_save_as_inline(&mut self, editor_id: ViewId) -> bool {
        use crate::dialog::{FD_OK_BUTTON, FileDialog};
        let initial = self
            .group
            .find_mut(editor_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<crate::widgets::FileEditor>())
            .and_then(|fe| {
                fe.file_name
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned())
            });
        let mut fd = FileDialog::new("*.*", "Save file as", "~N~ame", FD_OK_BUTTON, 101);
        if let Some(name) = initial {
            crate::view::View::set_value(&mut fd, crate::data::FieldValue::Text(name));
        }
        let (result, _) = self.exec_view_with_completion(
            Box::new(fd),
            Some(ModalCompletion::SaveAsPick { editor_id }),
            None,
            None,
            false,
        );
        if result != Command::CANCEL {
            // SaveAsPick sets file_name on the editor and re-injects cmSave into
            // out_events. Drain it now so save_file runs before the re-validate
            // pass sees modified=false.
            self.pump_once();
            true
        } else {
            false
        }
    }

    /// One iteration of the event loop.
    ///
    /// Borrow discipline: `self` is destructured into field
    /// bindings at the top, so the disjoint fields backing [`Context`]
    /// (`out_events` / `timers` / `deferred`) can be borrowed alongside
    /// `group` / `captures`. The dispatch is a free function with explicit field
    /// borrows; there are no `&mut self` helpers with overlapping field sets.
    ///
    /// Returns `true` when this pass was **idle** — the `None =>` arm ran because
    /// no input event (real, queued, or synthesized mouse-auto) was waiting. The
    /// run loop uses this to fire the user idle hook ([`set_on_idle`](Self::set_on_idle))
    /// outside this destructured borrow.
    pub fn pump_once(&mut self) -> bool {
        let Program {
            group,
            renderer,
            captures,
            timers,
            clock,
            theme,
            out_events,
            deferred,
            disabled_commands,
            desktop,
            // The menu bar is used by the fullscreen layout engine (apply_fullscreen).
            menu_bar,
            status_line,
            fullscreen,
            mouse_auto,
            end_state,
            command_set_changed,
            pending_modal,
            app_commands,
            clipboard_editor_id,
            clipboard_has_selection,
            shell_msg_hook,
            // The idle hook is fired from the run loop (outside this destructured
            // borrow), never here; bind it `_` to satisfy the exhaustive
            // destructure under `-D warnings`.
            on_idle: _,
        } = self;

        // 1. Resize check — the realization of setScreenMode/cmScreenChanged.
        //    CrosstermBackend::size() queries the terminal live, so there is no
        //    Event::Resize variant (avoids enum churn).
        let (w, h) = renderer.backend().size();
        let cur = group.state().size;
        let size_changed = cur.x != w as i32 || cur.y != h as i32;
        if size_changed {
            renderer.resize(w, h);
            group.change_bounds(Rect::new(0, 0, w as i32, h as i32));
        }

        // 2. Sample the clock once for this pass.
        let now = clock.now_ms();

        // 2a. Fullscreen layout maintenance: re-fit the tracked window after a
        //     resize (the growMode cascade just re-stretched the collapsed menu
        //     bar — re-shrink it), or restore chrome if the window was removed.
        if let Some(slot) = *fullscreen {
            let mut ctx = Context::new(out_events, timers, now, deferred);
            ctx.set_disabled_commands(disabled_commands.clone());
            ctx.set_clipboard_snapshot(*clipboard_editor_id, *clipboard_has_selection);
            if group.find_mut(slot.window).is_none() {
                apply_fullscreen(
                    group,
                    *desktop,
                    *menu_bar,
                    *status_line,
                    fullscreen,
                    slot.window,
                    crate::window::Fullscreen::Off,
                    &mut ctx,
                );
            } else if size_changed {
                apply_fullscreen(
                    group,
                    *desktop,
                    *menu_bar,
                    *status_line,
                    fullscreen,
                    slot.window,
                    slot.mode,
                    &mut ctx,
                );
            }
        }

        // 2b. Settle pending insert-time currency cascades BEFORE the event
        //     pick, so the dispatched event sees C++-equivalent currency: in C++
        //     every insert of a visible+selectable view ran show()->resetCurrent
        //     inline, so the very next event already routed to the new currency.
        //     A group whose `currency_dirty` was set by a ctx-less insert (and
        //     not superseded by an explicit set_current since) reset-currents
        //     here; everywhere else this walk is a no-op.
        {
            let mut ctx = Context::new(out_events, timers, now, deferred);
            ctx.set_disabled_commands(disabled_commands.clone());
            ctx.set_clipboard_snapshot(*clipboard_editor_id, *clipboard_has_selection);
            group.settle_currency(&mut ctx);
        }

        // 3. Pick the next event: drain the internal queue first, else poll.
        //    Note the timeout: the 20 ms frame tick (event_wait_timeout, the
        //    C++ eventTimeoutMs = 20, tprogram.cpp:38) already bounds the
        //    mouse-auto jitter below to +20 ms — the same wake cadence C++ runs
        //    its getMouseEvent checks on. No shorter wait is needed.
        let timeout = event_wait_timeout(timers, now);
        let ev = match out_events.pop_front() {
            Some(e) => Some(e),
            None => renderer.backend_mut().poll_event(timeout),
        };

        // 3b. The evMouseAuto synthesizer (see MouseAutoState): a real
        //     picked event updates the held-button bookkeeping (BEFORE dispatch
        //     localizes/mutates it); an empty pick may instead synthesize an
        //     auto, which then dispatches exactly like a real event. Real
        //     events always win over autos — the C++ auto arm is the LAST check
        //     in getMouseEvent (tevent.cpp:196).
        let ev = match ev {
            Some(e) => {
                mouse_auto.observe(&e, now);
                Some(e)
            }
            None => mouse_auto.synthesize(now),
        };

        // A pass is "idle" iff the final event (after the mouse-auto synthesizer)
        // is None — i.e. the `None =>` arm below runs. The run loop fires the user
        // idle hook on these passes.
        let was_idle = ev.is_none();

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
                // This is strictly more correct than queuing N indistinguishable
                // `cmTimerExpired` broadcasts for N expired ids: a widget can tell
                // *which* timer fired from the carried id.
                for id in timers.collect_expired(now) {
                    out_events.push_back(Event::Timer(id));
                }
                // TProgram::idle's statusLine->update() (tstatusl.cpp:209):
                // re-run find_items against the top view's help context + redraw.
                //
                // Faithful to C++ TView::TopView() (tview.cpp:879): when an execView
                // modal is active, captures.top_modal_view() returns its ViewId, and
                // we read that modal's get_help_ctx(). When no modal is running
                // (top_modal_view() == None), C++ TopView() does NOT return 0 — it
                // walks UP the owner chain to the first sfModal view (the application
                // root) and returns it; TGroup::getHelpCtx then recurses DOWN via the
                // current chain to the focused leaf. Our root `group` IS that sfModal
                // app root, so the None arm reads group.get_help_ctx() directly (Task
                // 4 made get_help_ctx recursive). This surfaces a non-modal desktop
                // window's focused control to the status line (gap #3 fix).
                //
                // No explicit redraw is needed — the whole-tree redraw runs every
                // pump cycle after this arm, so set_help_ctx's internal state
                // update is picked up on the next render.
                if let Some(sl_id) = *status_line {
                    // Step 1: read the effective help ctx.
                    // Priority: an open MenuSession on the capture stack wins —
                    // it is the topmost handler and its `menu_help_ctx` yields
                    // `Some` even for NO_CONTEXT, matching C++ `TopView()->
                    // getHelpCtx()` where `TopView()` is the active `TMenuView`.
                    // Otherwise fall to the modal/non-modal view chain below.
                    let top_ctx = captures.active_menu_help_ctx().unwrap_or_else(|| {
                        match captures.top_modal_view() {
                            Some(modal_id) => group
                                .find_mut(modal_id)
                                .map(|v| v.get_help_ctx())
                                .unwrap_or(crate::help::HelpCtx::NO_CONTEXT),
                            // No execView modal: faithful to C++ TView::TopView() (tview.cpp:879) —
                            // when TheTopView == 0 it walks UP to the first sfModal view, the
                            // application root, whose TGroup::getHelpCtx (group.rs) recurses DOWN
                            // the current chain to the focused leaf. Our root `group` IS that modal
                            // app root, so read its (now-recursive, Task 4) help context.
                            None => group.get_help_ctx(),
                        }
                    });
                    // Step 2: update status line (separate find_mut borrow).
                    use crate::status::StatusLine;
                    if let Some(sl) = group
                        .find_mut(sl_id)
                        .and_then(|v| v.as_any_mut())
                        .and_then(|a| a.downcast_mut::<StatusLine>())
                    {
                        sl.set_help_ctx(top_ctx);
                    }
                }
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
                        // Pre-route deferreds are FIRST-CLASS: the deferred drain at
                        // the tail of this arm runs even when the pre-route consumed
                        // (cleared) the event, so anything the status line queues here
                        // (e.g. the mouse-track PushCapture, status-line pre-route)
                        // applies through the same drain as every other widget.
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
                        ctx.set_disabled_commands(disabled_commands.clone());
                        ctx.set_clipboard_snapshot(*clipboard_editor_id, *clipboard_has_selection);
                        v.handle_event(&mut ev, &mut ctx);
                    }
                }

                // Command filtering at the program boundary: drop a command
                // only when it is explicitly disabled (denylist — everything else,
                // including unregistered custom commands, flows). Broadcasts/keys/
                // mouse flow regardless.
                let drop_disabled = matches!(ev, Event::Command(c) if disabled_commands.has(c));
                if drop_disabled {
                    ev.clear();
                }

                if !ev.is_nothing() {
                    // The Context borrow ends at this block's close, before we
                    // drain the deferred queue back into loop/tree state.
                    //
                    // Refresh bounds-gating capture handlers (the modal frame)
                    // from the live tree BEFORE dispatching: this picks up both
                    // the current pump's resize relayout (top of pump_once) and
                    // every previous pump's applied deferreds (ChangeBounds from
                    // a drag, captures pushed via the pre-route path) — a moved
                    // dialog must not go mouse-dead or mis-gate outside clicks.
                    captures
                        .sync_gate_bounds(|id| group.find_mut(id).map(|v| v.state().get_bounds()));
                    {
                        let mut ctx = Context::new(out_events, timers, now, deferred);
                        // Per-pump refresh of the Context's disabled-command
                        // SNAPSHOT (denylist), backing ctx.command_enabled —
                        // an owned clone, so no aliasing with the deferred-apply
                        // arms that mutate the live set below.
                        ctx.set_disabled_commands(disabled_commands.clone());
                        ctx.set_clipboard_snapshot(*clipboard_editor_id, *clipboard_has_selection);
                        // Outside-modal redirect: while a ModalFrame is the top capture,
                        // deliver outside-bounds positional events directly to the modal
                        // view (localized to its bounds) so the view decides — HistoryWindow
                        // cancels; plain Dialog ignores. C++: THistoryWindow::handleEvent
                        // checks !mouseInView AFTER base.
                        let modal_handled = {
                            // Resolve top-capture view id and its bounds (only when
                            // the top handler is a ModalFrame, not drag/menu handlers).
                            let modal = captures.top_modal_view().and_then(|id| {
                                group.find_mut(id).map(|v| (id, v.state().get_bounds()))
                            });
                            if let Some((modal_id, modal_bounds)) = modal {
                                let outside = match &ev {
                                    Event::MouseDown(m)
                                    | Event::MouseUp(m)
                                    | Event::MouseMove(m)
                                    | Event::MouseAuto(m) => !modal_bounds.contains(m.position),
                                    _ => false,
                                };
                                if outside {
                                    // Localize: subtract the modal view's top-left (makeLocal).
                                    let origin = modal_bounds.a;
                                    match &mut ev {
                                        Event::MouseDown(m) => m.position -= origin,
                                        Event::MouseUp(m) => m.position -= origin,
                                        Event::MouseMove(m) => m.position -= origin,
                                        Event::MouseAuto(m) => m.position -= origin,
                                        _ => {}
                                    }
                                    if let Some(v) = group.find_mut(modal_id) {
                                        v.handle_event(&mut ev, &mut ctx);
                                    }
                                    true
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        };
                        if !modal_handled {
                            // Offer to the capture stack first; if consumed, skip view
                            // routing.
                            let consumed = captures.dispatch(&mut ev, &mut ctx);
                            if !consumed {
                                program_handle_event(
                                    group,
                                    *desktop,
                                    &mut ev,
                                    &mut ctx,
                                    end_state,
                                    app_commands,
                                    renderer,
                                    shell_msg_hook,
                                );
                            }
                        }
                    }
                }
                // Apply the deferred queue AFTER dispatch — one drain, in
                // insertion order. INVARIANT: the drain runs even when the
                // pre-route consumed the event — pre-route deferreds are
                // first-class (the status-line pre-route: a status-line MouseDown that
                // arms a mouse track queues its PushCapture from the pre-route,
                // where the `!ev.is_nothing()` dispatch gate above is skipped).
                //
                // Drain to a local first (`mem::take`): the
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
                    // Snapshot taken BEFORE the Enable/DisableCommand arms
                    // mutate the live set: an apply-phase callee reading
                    // ctx.command_enabled sees this pass's pre-apply state
                    // (snapshot semantics; next pump sees the change).
                    ctx.set_disabled_commands(disabled_commands.clone());
                    ctx.set_clipboard_snapshot(*clipboard_editor_id, *clipboard_has_selection);
                    for effect in effects {
                        match effect {
                            Deferred::PushCapture(h) => captures.push(h),
                            // Inline the enable/disable bodies — the destructure
                            // gives the fields, not `self`. The set holds DISABLED
                            // commands (denylist), so enable removes / disable
                            // inserts. Flip `command_set_changed` on a real change
                            // so the next idle broadcasts cmCommandSetChanged.
                            Deferred::EnableCommand(cmd) => {
                                if disabled_commands.has(cmd) {
                                    disabled_commands.remove(cmd);
                                    *command_set_changed = true;
                                }
                            }
                            Deferred::DisableCommand(cmd) => {
                                if !disabled_commands.has(cmd) {
                                    disabled_commands.insert(cmd);
                                    *command_set_changed = true;
                                }
                            }
                            Deferred::ChangeBounds(id, r) => {
                                if let Some(v) = group.find_mut(id) {
                                    v.change_bounds(r);
                                    // TScroller/TListViewer::changeBounds call
                                    // setLimit/setStep after setBounds — realized as
                                    // this post-apply hook (view.rs on_bounds_changed).
                                    v.on_bounds_changed(&mut ctx);
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
                            // nested exec_view loop observes it.
                            Deferred::EndModal(cmd) => {
                                *end_state = Some(cmd);
                            }
                            // Frameless-fullscreen cross-tree layout engine: collapse/
                            // restore the menu bar, re-bound the desktop, and re-fit
                            // the window — all through the View trait, no downcast.
                            Deferred::SetFullscreen { window, mode } => {
                                apply_fullscreen(
                                    group,
                                    *desktop,
                                    *menu_bar,
                                    *status_line,
                                    fullscreen,
                                    window,
                                    mode,
                                    &mut ctx,
                                );
                            }
                            // -- TScroller cross-view broker --------
                            //
                            // Unified sibling-scrollbar read-sync (replaces the
                            // four separate per-type pump arms that each did a
                            // concrete downcast): read each bar's `value` (each
                            // in its own find_mut so only one &mut is live) and call
                            // back through the defaulted View::apply_scroll_sync —
                            // virtual dispatch, never a downcast. The list-viewer
                            // override writes back (v-bar setValue); it terminates
                            // because ScrollBar::set_params is change-guarded.
                            Deferred::ScrollSync { target, h, v } => {
                                let hv = h
                                    .and_then(|id| group.find_mut(id))
                                    .and_then(|view| view.value())
                                    .and_then(field_int);
                                let vv = v
                                    .and_then(|id| group.find_mut(id))
                                    .and_then(|view| view.value())
                                    .and_then(field_int);
                                if let Some(view) = group.find_mut(target) {
                                    view.apply_scroll_sync(hv, vv, &mut ctx);
                                }
                            }
                            // -- Splitter keyboard-resize broker (D3) -------
                            //
                            // The window resize capture holds only &mut Context;
                            // it cannot reach the splitter (a sibling in the
                            // window's group) inline. It queues this; the pump
                            // resolves and downcasts to `Splitter` (same
                            // `as_any_mut` pattern as the Scroller broker above).
                            Deferred::SplitterDivider { splitter, op } => {
                                use crate::view::DividerOp;
                                use crate::widgets::Splitter;
                                if let Some(sp) = group
                                    .find_mut(splitter)
                                    .and_then(|view| view.as_any_mut())
                                    .and_then(|a| a.downcast_mut::<Splitter>())
                                {
                                    match op {
                                        DividerOp::SetActive(sel) => sp.set_active_divider(sel),
                                        DividerOp::Nudge { index, delta } => {
                                            sp.nudge_divider(index, delta)
                                        }
                                        DividerOp::EndSession { commit } => {
                                            sp.end_resize_session(commit)
                                        }
                                    }
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
                            // Visibility direction: routes via
                            // set_visible_descendant, which delivers
                            // StateFlag::Visible via child.set_state so widgets
                            // with scroll bars (e.g. ListViewer) can react.
                            // StateFlag::Visible does NOT propagate to children.
                            // If the flag changed and the child is selectable,
                            // the owning group resetCurrents (both directions).
                            // Today's scroll bar consumers are non-selectable,
                            // so the currency tail is a no-op for them.
                            Deferred::SetVisible(id, visible) => {
                                group.set_visible_descendant(id, visible, &mut ctx);
                            }
                            // -- TMenuView command-graying broker --
                            //
                            // The menu view (a child) cannot read the
                            // command set inline — the pump owns it. Resolve
                            // the menu view and call back through the defaulted
                            // View::update_menu_commands trait method with the
                            // live DISABLED set in hand (it regrays the menu
                            // tree: an item grays iff its command is in the
                            // set). `group` and `disabled_commands` are disjoint
                            // destructured fields, so no `ctx` is needed (like
                            // ChangeBounds).
                            Deferred::UpdateMenu(id) => {
                                if let Some(v) = group.find_mut(id) {
                                    v.update_menu_commands(disabled_commands);
                                }
                            }
                            // -- the TMenuView modal layer ------
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
                            // -- the THistory view-triggered async-modal seam --
                            //
                            // recordHistory(link->data) for the broadcast arm:
                            // read the link's current text and history_add it.
                            Deferred::RecordHistory { link, history_id } => {
                                record_history_for(group, link, history_id);
                            }
                            // OpenHistory: the THistory leaf holds only the link's
                            // id and cannot call exec_view (top-level only), so
                            // it requested the open and the pump does everything
                            // reachable here (group + ctx + pending_modal, none
                            // aliased — ctx borrows out_events/timers/deferred, not
                            // group/pending_modal). It does NOT exec_view here: it
                            // stashes the built window into pending_modal for the
                            // outer pump_and_drive to run at top level.
                            Deferred::OpenHistory {
                                link,
                                history_id,
                                require_focus,
                            } => {
                                let focused = group
                                    .find_mut(link)
                                    .map(|v| v.state().state.focused)
                                    .unwrap_or(false);
                                // Keyboard-trigger gate (faithful to
                                // `(link->state & sfFocused)`): the keyboard arm
                                // only opens when the link is already focused; the
                                // mouse arm (require_focus == false) always opens.
                                if require_focus && !focused {
                                    // not focused — drop the request (no open).
                                } else if let Some(bounds) = build_history_bounds(group, link) {
                                    // link->focus() — DEVIATION: focus here is
                                    // requested through the deferred channel
                                    // (focus_descendant) with no inline success
                                    // bool, so the C++ focus-abort
                                    // (`if (!link->focus()) return`) is dropped;
                                    // request focus + proceed.
                                    group.focus_descendant(link, &mut ctx);
                                    // recordHistory(link->data): the link's CURRENT
                                    // text at OPEN, never the picked value (faithful
                                    // pin — the completion never re-records).
                                    record_history_for(group, link, history_id);
                                    // initHistoryWindow + stash for the outer drive.
                                    // helpCtx propagation is intentionally omitted:
                                    // tvision-rs has no per-view help-context plumbing for
                                    // the HistoryWindow to inherit.
                                    let hw = crate::widgets::HistoryWindow::new(bounds, history_id);
                                    *pending_modal = Some((
                                        Box::new(hw),
                                        ModalCompletion::HistoryPick { link },
                                        None, // HistoryWindow manages its own focus
                                    ));
                                }
                            }
                            // -- the TEditor cross-view brokers ----
                            //
                            // Indicator write (TEditor::doUpdate →
                            // indicator->setValue): resolve the indicator and
                            // call the View trait hook — virtual dispatch, no
                            // downcast.
                            Deferred::IndicatorSetValue {
                                indicator,
                                location,
                                modified,
                            } => {
                                if let Some(ind) = group.find_mut(indicator) {
                                    ind.set_indicator_value(location, modified);
                                }
                            }
                            // Clipboard copy (TEditor::clipCopy → setText):
                            // the backend is reachable here via renderer.
                            Deferred::SetClipboard(s) => {
                                renderer.backend_mut().set_clipboard(&s);
                            }
                            // Clipboard paste (TEditor::clipPaste →
                            // requestText): read the backend clipboard, then
                            // downcast the editor and insert. The insert pushes
                            // further deferred scrollbar-param ops that settle
                            // next pump (ONE-pass drain — expected).
                            Deferred::EditorPaste(id) => {
                                let txt = renderer.backend_mut().get_clipboard();
                                // The id may resolve to a FileEditor or a plain
                                // Editor/Memo — editor_mut peels to the inner Editor.
                                if let Some(t) = txt
                                    && let Some(ed) =
                                        group.find_mut(id).and_then(crate::widgets::editor_mut)
                                {
                                    ed.insert_text(t.as_bytes(), false, &mut ctx);
                                }
                            }
                            // clipboard paste into an InputLine
                            // (tinputli.cpp cmPaste → TClipboard::requestText):
                            // read the backend clipboard, downcast the view to
                            // InputLine, and call paste_text — which inserts at
                            // the cursor, replacing any selection and clamping to
                            // max_len (same broker shape as EditorPaste).
                            Deferred::InputLinePaste(id) => {
                                let txt = renderer.backend_mut().get_clipboard();
                                if let Some(t) = txt
                                    && let Some(il) = group
                                        .find_mut(id)
                                        .and_then(|view| view.as_any_mut())
                                        .and_then(|a| a.downcast_mut::<crate::widgets::InputLine>())
                                {
                                    il.paste_text(&t);
                                }
                            }
                            // -- the internal-clipboard TEditor broker -----
                            //
                            // Three variants: register, receive (copy into
                            // clipboard editor), and paste (copy out to dest
                            // editor). All touch the view-tree family.

                            // Register an internal-clipboard editor
                            // (`TEditor::clipboard = editor`).
                            // Store the ID, mark the editor is_clipboard=true,
                            // and set the EditWindow title to "Clipboard".
                            Deferred::RegisterClipboardEditor {
                                editor_id,
                                window_id,
                            } => {
                                *clipboard_editor_id = Some(editor_id);
                                // Mark the editor as the clipboard (is_clipboard = true).
                                if let Some(fe) = group
                                    .find_mut(editor_id)
                                    .and_then(|v| v.as_any_mut())
                                    .and_then(|a| a.downcast_mut::<crate::widgets::FileEditor>())
                                {
                                    fe.editor.is_clipboard = true;
                                }
                                // Set the hosting EditWindow's title to "Clipboard".
                                if let Some(ew) = group
                                    .find_mut(window_id)
                                    .and_then(|v| v.as_any_mut())
                                    .and_then(|a| a.downcast_mut::<crate::widgets::EditWindow>())
                                {
                                    ew.window.set_title(Some("Clipboard".to_string()));
                                }
                            }
                            // C++: `clipboard->insertFrom(source)` from clipCopy().
                            // Insert source bytes into the clipboard editor.
                            // The clipboard editor selects the inserted content
                            // (is_clipboard=true → select_text=true in insert_from).
                            Deferred::ClipboardEditorReceive { clipboard_id, data } => {
                                // Use editor_mut (same as EditorPaste) to handle both
                                // FileEditor and plain Editor clipboard registrations.
                                if let Some(ed) = group
                                    .find_mut(clipboard_id)
                                    .and_then(crate::widgets::editor_mut)
                                {
                                    ed.insert_from(&data, &mut ctx);
                                    // Refresh the has_selection snapshot after insert.
                                    *clipboard_has_selection = ed.has_selection();
                                }
                            }
                            // C++: `insertFrom(clipboard)` from clipPaste().
                            // Step 1: read clipboard editor's selection bytes
                            // (clone to free the borrow).
                            // Step 2: insert into dest editor.
                            Deferred::ClipboardEditorPaste {
                                dest_id,
                                clipboard_id,
                            } => {
                                let data: Option<Vec<u8>> = group
                                    .find_mut(clipboard_id)
                                    .and_then(|v| v.as_any_mut())
                                    .and_then(|a| a.downcast_mut::<crate::widgets::FileEditor>())
                                    .map(|fe| fe.editor.selection_bytes());
                                if let Some(d) = data
                                    && let Some(ed) =
                                        group.find_mut(dest_id).and_then(crate::widgets::editor_mut)
                                {
                                    ed.insert_from(&d, &mut ctx);
                                }
                            }
                            // -- cmFileFocused payload broker -------
                            //
                            // Resolve the payload-carrying cmFileFocused
                            // broadcast (source is the resolvable subject,
                            // not a value carrier). Read the focused SearchRec
                            // from the source FileList in its OWN find_mut and
                            // drop the borrow, THEN find_mut the subscriber and
                            // write it — only one &mut is live at a time, like
                            // ScrollSync's read-then-write.
                            Deferred::ResolveFocusedFile { subscriber, source } => {
                                use crate::dialog::{FileInfoPane, FileInputLine, FileList};
                                let rec = group
                                    .find_mut(source)
                                    .and_then(|view| view.as_any_mut())
                                    .and_then(|a| a.downcast_mut::<FileList>())
                                    .and_then(|fl| fl.focused_rec());
                                // Two consumers share the broker: a FileInputLine
                                // and a FileInfoPane. Try each
                                // downcast in turn; `rec` moves into the matching
                                // arm (the two are mutually exclusive).
                                if let Some(fil) = group
                                    .find_mut(subscriber)
                                    .and_then(|view| view.as_any_mut())
                                    .and_then(|a| a.downcast_mut::<FileInputLine>())
                                {
                                    fil.on_file_focused(rec);
                                } else if let Some(fip) = group
                                    .find_mut(subscriber)
                                    .and_then(|view| view.as_any_mut())
                                    .and_then(|a| a.downcast_mut::<FileInfoPane>())
                                {
                                    fip.on_file_focused(rec);
                                }
                            }
                            // -- TDirListBox → chDirButton makeDefault --
                            //
                            // The dir list (a leaf view) gained/lost focus and
                            // wants its sibling chDirButton to grab/release the
                            // default look. Resolve the button, downcast, and call
                            // make_default(enable, ctx); its GRAB/RELEASE_DEFAULT
                            // re-broadcast settles next pump (like EditorPaste).
                            Deferred::MakeButtonDefault { button, enable } => {
                                use crate::widgets::Button;
                                if let Some(b) = group
                                    .find_mut(button)
                                    .and_then(|view| view.as_any_mut())
                                    .and_then(|a| a.downcast_mut::<Button>())
                                {
                                    b.make_default(enable, &mut ctx);
                                }
                            }
                            // -- the MouseTrackCapture router --
                            //
                            // The capture localized + forwarded a masked mouse
                            // event during the hold; deliver it straight to the
                            // tracked view's handle_event — the apply-time
                            // analogue of the pump's outside-modal redirect
                            // above. No downcast: the widget's own
                            // MouseMove/MouseAuto/MouseUp arms ARE the C++
                            // hold-loop body / post-loop code (decisive for
                            // trait-object viewers). `ctx` already carries the
                            // disabled-command snapshot (set above, mirroring
                            // the redirect's Context), and its phase defaults
                            // to Focused — correct for a directly-addressed
                            // delivery (no pre/post walk is in flight).
                            Deferred::MouseTrack { view, event } => {
                                if let Some(v) = group.find_mut(view) {
                                    let mut ev = event;
                                    v.handle_event(&mut ev, &mut ctx);
                                }
                            }
                            // -- PageStack↔TabBar read-sync broker ----------
                            //
                            // On a TAB_BAR_CHANGED broadcast, PageStack queues
                            // this. The pump resolves `tab_bar`, reads its
                            // `value()` (FieldValue::Int index), then calls
                            // `apply_page_sync(idx, &mut ctx)` on `page_stack`
                            // through the trait method (virtual dispatch, no
                            // downcast) — which switches the active page via
                            // `set_active`. Mirrors ScrollSync but reads one
                            // bar into one index.
                            Deferred::PageStackSync {
                                page_stack,
                                tab_bar,
                            } => {
                                // TabBar::value() is always non-negative, so a plain
                                // `as usize` is safe (no defensive max(0)).
                                let idx = group
                                    .find_mut(tab_bar)
                                    .and_then(|v| v.value())
                                    .and_then(field_int)
                                    .unwrap_or(0);
                                if let Some(ps) = group.find_mut(page_stack) {
                                    ps.apply_page_sync(idx as usize, &mut ctx);
                                }
                            }
                            // -- the async-modal-from-a-view seam (handle_event paths) --
                            //
                            // A downward-borrowed `&mut View`'s valid() requested
                            // a messageBox. Build the centered box + stash it into
                            // pending_modal for the outer pump_and_drive to exec at
                            // top level (a view cannot call exec_view here — same
                            // structural constraint as OpenHistory). The completion
                            // routes the answer back (RouteModalAnswer) and re-posts
                            // then_command. (The modal-close path at 886 drives its
                            // own box inline — it is NOT on this event-gated drain.)
                            Deferred::OpenMessageBox {
                                text,
                                kind,
                                buttons,
                                answer_to,
                                then_command,
                            } => {
                                let r = centered_msgbox_rect_for(group, *desktop, &text);
                                let (d, first) =
                                    crate::dialog::build_message_box(r, &text, kind, buttons);
                                let completion = match answer_to {
                                    Some(answer_to) => ModalCompletion::RouteModalAnswer {
                                        answer_to,
                                        then_command,
                                    },
                                    // Informational (OK-only) box: nothing to route.
                                    None => ModalCompletion::Informational,
                                };
                                // Thread the first-button focus (C++ selectNext(False))
                                // so the default button (Yes/OK) is focused on open —
                                // matching the sync `message_box_rect` + inline
                                // `validate_modal_close` paths.
                                *pending_modal = Some((Box::new(d), completion, first));
                            }
                            // -- saveAs: the view-triggered FileDialog seam ----
                            //
                            // A FileEditor leaf requested a save-as picker (it
                            // holds only &mut Context and cannot exec a nested
                            // modal — same structural constraint as OpenHistory /
                            // OpenMessageBox). Build the C++ edSaveAs dialog
                            // (`TFileDialog("*.*", "Save file as", "~N~ame",
                            // fdOKButton, 101)`), pre-fill the input line with the
                            // editor's current filename (C++ passes `fileName` to
                            // editorDialog → setData), and stash it into
                            // pending_modal for the outer pump_and_drive. The
                            // SaveAsPick completion reads the picked name back +
                            // re-injects cmSave.
                            Deferred::OpenSaveAsDialog { editor_id } => {
                                use crate::dialog::{FD_OK_BUTTON, FileDialog};
                                // Pre-fill: the editor's current filename, if any
                                // (C++ saveAs starts the input at `fileName`).
                                let initial = group
                                    .find_mut(editor_id)
                                    .and_then(|v| v.as_any_mut())
                                    .and_then(|a| a.downcast_mut::<crate::widgets::FileEditor>())
                                    .and_then(|fe| {
                                        fe.file_name
                                            .as_ref()
                                            .map(|p| p.to_string_lossy().into_owned())
                                    });
                                // C++ edSaveAs: TFileDialog("*.*", "Save file as",
                                // "~N~ame", fdOKButton, 101). The dialog self-centers
                                // (ofCentered + its 49x19 floor) — no bounds param.
                                let mut fd = FileDialog::new(
                                    "*.*",
                                    "Save file as",
                                    "~N~ame",
                                    FD_OK_BUTTON,
                                    101,
                                );
                                if let Some(name) = initial {
                                    crate::view::View::set_value(
                                        &mut fd,
                                        crate::data::FieldValue::Text(name),
                                    );
                                }
                                // FileDialog manages its own focus (reset_current
                                // focuses the input line) — no initial_focus.
                                *pending_modal = Some((
                                    Box::new(fd),
                                    ModalCompletion::SaveAsPick { editor_id },
                                    None,
                                ));
                            }

                            // Generic view-launched modal (ExecView): move the
                            // caller-built modal into pending_modal with a
                            // RouteModalAnswer completion (deliver the close command
                            // to `requester` by id + re-inject `then_command`). The
                            // outer pump_and_drive execs it via the existing single
                            // loop. Reuses the Open*Dialog → pending_modal pattern;
                            // no new ModalCompletion variant, no downcast. `None`
                            // initial focus = the modal focuses its own first view.
                            Deferred::OpenModal {
                                view,
                                requester,
                                then_command,
                            } => {
                                *pending_modal = Some((
                                    view,
                                    ModalCompletion::RouteModalAnswer {
                                        answer_to: requester,
                                        then_command,
                                    },
                                    None,
                                ));
                            }

                            // -- find dialog (edFind) seam -------------------
                            //
                            // An Editor requested cmFind. Build a 38×12 "Find"
                            // dialog (faithful to C++ tvedit2.cpp::createFindDialog),
                            // pre-fill from editor state, and stash into
                            // pending_modal with FindPick.
                            Deferred::OpenFindDialog { editor_id } => {
                                use crate::data::FieldValue;
                                use crate::dialog::Dialog;
                                use crate::view::Rect;
                                use crate::widgets::{
                                    Button, ButtonFlags, CheckBoxes, InputLine, Label, LimitMode,
                                    THistory,
                                };

                                // NOTE (Phase 4, spec §4.4): this is a dialog-OPEN
                                // pre-fill read — the pump reads the editor's current
                                // search state to seed the freshly-built dialog. That
                                // is "build UI from a known widget's state," the kept
                                // structural-read category (like FileDialog readback),
                                // NOT a cluster-D modal-result read. It stays a
                                // downcast deliberately; only the completion reads
                                // (FindPick/ReplacePick) went downcast-free.
                                // Read current editor search state.
                                let (find_str, editor_flags) = group
                                    .find_mut(editor_id)
                                    .and_then(|v| v.as_any_mut())
                                    .and_then(|a| a.downcast_mut::<crate::widgets::Editor>())
                                    .map(|e| (e.find_str().to_owned(), e.editor_flags()))
                                    .unwrap_or_default();

                                // Build dialog (0,0,38,12), centered, title "Find".
                                let mut d =
                                    Dialog::new(Rect::new(0, 0, 38, 12), Some("Find".into()));
                                {
                                    let opts = &mut d.state_mut().options;
                                    opts.center_x = true;
                                    opts.center_y = true;
                                }

                                // InputLine: rect (3,3,32,4), max_len 80 bytes.
                                let mut il = InputLine::new(
                                    Rect::new(3, 3, 32, 4),
                                    81, // MaxBytes: max_len = limit-1 = 80
                                    None,
                                    LimitMode::MaxBytes,
                                );
                                il.set_value(FieldValue::Text(find_str));
                                let find_id = d.insert_child(Box::new(il));

                                // Label: linked to find InputLine.
                                d.insert_child(Box::new(Label::new(
                                    Rect::new(2, 2, 15, 3),
                                    "~T~ext to find",
                                    Some(find_id),
                                )));
                                // History button (C++ tvedit2: THistory(32,3,35,4), id=10).
                                d.insert_child(Box::new(THistory::new(
                                    Rect::new(32, 3, 35, 4),
                                    find_id,
                                    10,
                                )));

                                // CheckBoxes: rect (3,5,35,7), 2 items.
                                let mut cb = CheckBoxes::new(
                                    Rect::new(3, 5, 35, 7),
                                    vec!["~C~ase sensitive".into(), "~W~hole words only".into()],
                                );
                                // Pre-fill bits 0-1 from editor_flags.
                                cb.cluster.value = (editor_flags & 0x0003) as u32;
                                let opts_id = d.insert_child(Box::new(cb));

                                // OK (default) + Cancel buttons.
                                d.insert_child(Box::new(Button::new(
                                    Rect::new(14, 9, 24, 11),
                                    "O~K~",
                                    Command::OK,
                                    ButtonFlags {
                                        default: true,
                                        ..Default::default()
                                    },
                                )));
                                d.insert_child(Box::new(Button::new(
                                    Rect::new(26, 9, 36, 11),
                                    "Cancel",
                                    Command::CANCEL,
                                    ButtonFlags::new(),
                                )));

                                *pending_modal = Some((
                                    Box::new(d),
                                    ModalCompletion::FindPick {
                                        editor_id,
                                        find_id,
                                        opts_id,
                                    },
                                    Some(find_id),
                                ));
                            }

                            // -- replace dialog (edReplace) seam -------------
                            Deferred::OpenReplaceDialog { editor_id } => {
                                use crate::data::FieldValue;
                                use crate::dialog::Dialog;
                                use crate::view::Rect;
                                use crate::widgets::{
                                    Button, ButtonFlags, CheckBoxes, InputLine, Label, LimitMode,
                                    THistory,
                                };

                                // NOTE (Phase 4, spec §4.4): this is a dialog-OPEN
                                // pre-fill read — the pump reads the editor's current
                                // search state to seed the freshly-built dialog. That
                                // is "build UI from a known widget's state," the kept
                                // structural-read category (like FileDialog readback),
                                // NOT a cluster-D modal-result read. It stays a
                                // downcast deliberately; only the completion reads
                                // (FindPick/ReplacePick) went downcast-free.
                                let (find_str, replace_str, editor_flags) = group
                                    .find_mut(editor_id)
                                    .and_then(|v| v.as_any_mut())
                                    .and_then(|a| a.downcast_mut::<crate::widgets::Editor>())
                                    .map(|e| {
                                        (
                                            e.find_str().to_owned(),
                                            e.replace_str().to_owned(),
                                            e.editor_flags(),
                                        )
                                    })
                                    .unwrap_or_default();

                                // Build dialog (0,0,40,16), centered, title "Replace".
                                let mut d =
                                    Dialog::new(Rect::new(0, 0, 40, 16), Some("Replace".into()));
                                {
                                    let opts = &mut d.state_mut().options;
                                    opts.center_x = true;
                                    opts.center_y = true;
                                }

                                // "Text to find" InputLine at (3,3,34,4).
                                let mut il1 = InputLine::new(
                                    Rect::new(3, 3, 34, 4),
                                    81,
                                    None,
                                    LimitMode::MaxBytes,
                                );
                                il1.set_value(FieldValue::Text(find_str));
                                let find_id = d.insert_child(Box::new(il1));
                                d.insert_child(Box::new(Label::new(
                                    Rect::new(2, 2, 15, 3),
                                    "~T~ext to find",
                                    Some(find_id),
                                )));
                                // History (C++ tvedit2: THistory(34,3,37,4), id=10).
                                d.insert_child(Box::new(THistory::new(
                                    Rect::new(34, 3, 37, 4),
                                    find_id,
                                    10,
                                )));

                                // "New text" InputLine at (3,6,34,7).
                                let mut il2 = InputLine::new(
                                    Rect::new(3, 6, 34, 7),
                                    81,
                                    None,
                                    LimitMode::MaxBytes,
                                );
                                il2.set_value(FieldValue::Text(replace_str));
                                let replace_id = d.insert_child(Box::new(il2));
                                d.insert_child(Box::new(Label::new(
                                    Rect::new(2, 5, 12, 6),
                                    "~N~ew text",
                                    Some(replace_id),
                                )));
                                // History (C++ tvedit2: THistory(34,6,37,7), id=11).
                                d.insert_child(Box::new(THistory::new(
                                    Rect::new(34, 6, 37, 7),
                                    replace_id,
                                    11,
                                )));

                                // CheckBoxes (3,8,37,12), 4 items.
                                let mut cb = CheckBoxes::new(
                                    Rect::new(3, 8, 37, 12),
                                    vec![
                                        "~C~ase sensitive".into(),
                                        "~W~hole words only".into(),
                                        "~P~rompt on replace".into(),
                                        "~R~eplace all".into(),
                                    ],
                                );
                                cb.cluster.value = (editor_flags & 0x000F) as u32;
                                let opts_id = d.insert_child(Box::new(cb));

                                d.insert_child(Box::new(Button::new(
                                    Rect::new(17, 13, 27, 15),
                                    "O~K~",
                                    Command::OK,
                                    ButtonFlags {
                                        default: true,
                                        ..Default::default()
                                    },
                                )));
                                d.insert_child(Box::new(Button::new(
                                    Rect::new(28, 13, 38, 15),
                                    "Cancel",
                                    Command::CANCEL,
                                    ButtonFlags::new(),
                                )));

                                *pending_modal = Some((
                                    Box::new(d),
                                    ModalCompletion::ReplacePick {
                                        editor_id,
                                        find_id,
                                        replace_id,
                                        opts_id,
                                    },
                                    Some(find_id),
                                ));
                            }

                            // -- per-role color-picker seam (theme editor) --
                            //
                            // A ThemeEditorBody leaf cannot exec a modal inline
                            // (holds only &mut Context). It queues this; the
                            // pump builds a 60×23 "Select Color" dialog seeded with
                            // `current` and stashes it into pending_modal with a
                            // ThemeColorPick completion. pump_and_drive runs it at
                            // top level. On OK the completion downcasts the in-tree
                            // ColorPicker and writes the chosen color back into the
                            // ThemeEditorBody's working theme.
                            Deferred::OpenColorDialogForRole {
                                editor_id,
                                role,
                                fg,
                                current,
                            } => {
                                use crate::dialog::{ColorPicker, Dialog};
                                use crate::widgets::{Button, ButtonFlags};

                                // 60 × 23 dialog, centered on the desktop.
                                let mut r = crate::view::Rect::new(0, 0, 60, 23);
                                let desk_size = {
                                    let bounds = group.state().get_bounds();
                                    crate::view::Point::new(bounds.b.x, bounds.b.y)
                                };
                                r.r#move((desk_size.x - 60) / 2, (desk_size.y - 23) / 2);
                                let mut d = Dialog::new(r, Some("Select Color".to_string()));
                                let picker_id = d.insert_child(Box::new(ColorPicker::new(
                                    crate::view::Rect::new(2, 2, 58, 20),
                                    current,
                                )));
                                d.insert_child(Box::new(Button::new(
                                    crate::view::Rect::new(20, 20, 30, 22),
                                    "O~K~",
                                    Command::OK,
                                    ButtonFlags {
                                        default: true,
                                        ..Default::default()
                                    },
                                )));
                                d.insert_child(Box::new(Button::new(
                                    crate::view::Rect::new(31, 20, 41, 22),
                                    "~C~ancel",
                                    Command::CANCEL,
                                    ButtonFlags::default(),
                                )));
                                *pending_modal = Some((
                                    Box::new(d),
                                    ModalCompletion::ThemeColorPick {
                                        editor_id,
                                        picker: picker_id,
                                        role,
                                        fg,
                                    },
                                    Some(picker_id),
                                ));
                            }
                        }
                    }
                }
                // (Gate-bounds refresh happens at the TOP of the dispatch gate
                // above — captures only act during dispatch, so syncing just
                // before it covers the current pump's resize AND all previous
                // pumps' applied deferreds, including pre-route-pushed captures
                // whose first dispatch necessarily passes that sync.)
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

        was_idle
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
// program-level handle_event — ports TProgram::handleEvent (free fn, disjoint borrows)
// ---------------------------------------------------------------------------

/// Extract the `i32` out of a [`FieldValue::Int`](crate::data::FieldValue::Int),
/// or `None` for any other variant. Used by the scroller read-broker to read a
/// scrollbar's value through the generic
/// [`View::value`](crate::view::View::value).
fn field_int(v: crate::data::FieldValue) -> Option<i32> {
    match v {
        crate::data::FieldValue::Int(n) => Some(n),
        _ => None,
    }
}

/// Extract the [`String`] out of a [`FieldValue::Text`](crate::data::FieldValue::Text),
/// or `None` for any other variant. The text sibling of [`field_int`], used by the
/// history-list brokers to read the linked input line's text through
/// [`View::value`].
fn field_text(v: crate::data::FieldValue) -> Option<String> {
    match v {
        crate::data::FieldValue::Text(s) => Some(s),
        _ => None,
    }
}

/// Extract the `u32` bit word out of a [`FieldValue::Bits`](crate::data::FieldValue::Bits),
/// or `None` for any other variant. The `Bits` sibling of [`field_int`]/[`field_text`],
/// used by the Find/Replace modal-result reads to pull a `CheckBoxes` options word
/// through [`View::value`].
fn field_bits(v: crate::data::FieldValue) -> Option<u32> {
    match v {
        crate::data::FieldValue::Bits(b) => Some(b),
        _ => None,
    }
}

/// Record a history entry for a history-list link: resolve `link`, read the
/// linked input line's text via [`View::value`], and `history_add` it to the
/// channel. A free fn so it composes with the pump's destructured `group` borrow
/// (no `&mut self`).
fn record_history_for(group: &mut Group, link: ViewId, history_id: u8) {
    if let Some(t) = group
        .find_mut(link)
        .and_then(|v| v.value())
        .and_then(field_text)
    {
        crate::widgets::history_add(history_id, &t);
    }
}

/// Build the history-list popup bounds: grow the link's rect (1 cell
/// left/right/up, 7 down), clamp it to the screen, then shrink the bottom by 1.
/// Our `exec_view` root-inserts the modal and `ModalFrame` hit-tests in
/// **root/absolute** coords (the root-insert + (0,0) caveat documented on
/// `exec_view`), so we work in absolute coords throughout.
///
/// **Two geometry deviations (documented, same family as the ModalFrame caveat):**
/// 1. Absolute via [`View::descendant_global_bounds`] instead of the link's
///    owner-local bounds — correct for any nesting depth.
/// 2. Clamp to the **screen** extent instead of the owning dialog's extent. We
///    root-insert, so the screen is the outer frame; the difference only matters
///    when the dialog is inset from the screen.
fn build_history_bounds(group: &mut Group, link: ViewId) -> Option<Rect> {
    let mut r = group.descendant_global_bounds(link, Point::new(0, 0))?;
    // C++ grow: r.a.x--; r.b.x++; r.a.y--; r.b.y += 7 (1 left, 1 right, 1 up, 7 down).
    r.a.x -= 1;
    r.b.x += 1;
    r.a.y -= 1;
    r.b.y += 7;
    // Clamp to the SCREEN extent (deviation 2).
    let screen = Rect::new(0, 0, group.state().size.x, group.state().size.y);
    r.intersect(&screen);
    r.b.y -= 1; // shrink bottom by 1 (C++ r.b.y--).
    Some(r)
}

/// Size a message-box rect from its text and center it on the desktop, as a free
/// fn so the pump's destructured-borrow `OpenMessageBox` drain can reuse it (it
/// cannot call the `&mut self` [`Program::centered_msgbox_rect`]). Centers within
/// the desktop's SIZE, falling back to the root group.
///
/// # Turbo Vision heritage
/// Ports the static `makeRect(text)` + desktop-centering in `msgbox.cpp`; the
/// offset is `deskTop->size`.
fn centered_msgbox_rect_for(group: &Group, desktop: Option<ViewId>, msg: &str) -> Rect {
    let base_w = 40_i32;
    let base_h = 9_i32;
    let char_count = msg.chars().count() as i32;
    let text_area = (base_w - 7) * (base_h - 6); // 33*3 = 99
    let h = if char_count > text_area {
        char_count / (base_w - 7) + 6 + 1
    } else {
        base_h
    };
    let mut r = Rect::new(0, 0, base_w, h);
    let desk_size = desktop
        .and_then(|id| group.descendant_global_bounds(id, Point::new(0, 0)))
        .map(|b| Point::new(b.b.x - b.a.x, b.b.y - b.a.y))
        .unwrap_or_else(|| group.state().size);
    r.r#move((desk_size.x - base_w) / 2, (desk_size.y - h) / 2);
    r
}

/// Apply a fullscreen `mode` to `window` across the tree. Border visibility is
/// toggled inline by `Window::set_fullscreen`; this performs the cross-tree work:
/// collapse/restore the menu bar (+ its bounds), re-bound the desktop, and re-fit
/// the window — all through the `View` trait (no downcast). Tracks/clears the
/// loop-owned `slot`. Reused by the deferred drain and the resize/vanish path.
#[allow(clippy::too_many_arguments)]
fn apply_fullscreen(
    group: &mut Group,
    desktop: Option<ViewId>,
    menu_bar: Option<ViewId>,
    status_line: Option<ViewId>,
    slot: &mut Option<FullscreenSlot>,
    window: ViewId,
    mode: crate::window::Fullscreen,
    ctx: &mut Context,
) {
    use crate::window::Fullscreen;
    let screen = group.state().size;
    let (w, h) = (screen.x, screen.y);
    let menu_present = menu_bar.is_some();
    let status_h = i32::from(status_line.is_some());

    // 1. Edge bookkeeping: capture restore bounds + shadow on first entering;
    //    clear the shadow while fullscreen.
    let entering =
        slot.as_ref().is_none_or(|s| s.mode == Fullscreen::Off) && mode != Fullscreen::Off;
    if entering {
        if let Some(v) = group.find_mut(window) {
            let restore = v.state().get_bounds();
            let shadow = v.state().state.shadow;
            v.state_mut().state.shadow = false;
            *slot = Some(FullscreenSlot {
                window,
                mode,
                restore,
                shadow,
            });
        }
    } else if let Some(s) = slot.as_mut() {
        s.mode = mode;
    }

    // 2. Menu bar: collapse + bounds (⋮ cell when Screen, full top row otherwise).
    if let Some(mb) = menu_bar {
        let collapsed = mode == Fullscreen::Screen;
        if let Some(v) = group.find_mut(mb) {
            if let Some(bar) = v
                .as_any_mut()
                .and_then(|a| a.downcast_mut::<crate::menu::MenuBar>())
            {
                bar.set_collapsed(collapsed);
            }
            let bounds = if collapsed {
                Rect::new(w - 1, 0, w, 1)
            } else {
                Rect::new(0, 0, w, 1)
            };
            v.change_bounds(bounds);
        }
    }

    // 3. Desktop bounds: top row 0 when Screen, else below the menu bar.
    let top = if mode == Fullscreen::Screen {
        0
    } else {
        i32::from(menu_present)
    };
    if let Some(dt) = desktop
        && let Some(v) = group.find_mut(dt)
    {
        v.change_bounds(Rect::new(0, top, w, h - status_h));
    }

    // 4. Window bounds: fill the (now-sized) desktop, or restore on Off.
    let target = if mode == Fullscreen::Off {
        slot.as_ref().map(|s| s.restore)
    } else {
        let dh = (h - status_h) - top;
        Some(Rect::new(0, 0, w, dh)) // desktop-local: the window fills its owner
    };
    if let Some(rect) = target
        && let Some(v) = group.find_mut(window)
    {
        v.change_bounds(rect);
        v.on_bounds_changed(ctx);
    }

    // 5. Exit: restore the shadow verbatim and clear the slot.
    if mode == Fullscreen::Off
        && let Some(s) = slot.take()
        && let Some(v) = group.find_mut(window)
    {
        v.state_mut().state.shadow = s.shadow;
    }
}

/// Run a [`ModalCompletion`] as a DIRECT `group` mutation, while the modal
/// is still in the tree by `modal_id`. NOT a deferred queue entry (that drain
/// fires only when a `Some(ev)` pump pass runs inside `pump_once`, and would
/// never fire from `exec_view`). Two sequential `find_mut` borrows — never simultaneous.
fn apply_modal_completion(
    c: ModalCompletion,
    result: Command,
    group: &mut Group,
    modal_id: ViewId,
) -> Option<Event> {
    match c {
        ModalCompletion::HistoryPick { link } => {
            if result == Command::OK {
                // getSelection is read while the modal still exists (faithful pin):
                // downcast the modal dyn View to HistoryWindow and read its selection.
                let s = group
                    .find_mut(modal_id)
                    .and_then(|v| v.as_any_mut())
                    .and_then(|a| a.downcast_mut::<crate::widgets::HistoryWindow>())
                    .map(|hw| hw.get_selection());
                if let Some(s) = s {
                    // strnzcpy + selectAll(True): InputLine::set_value already does
                    // `data = s; select_all(true, true)` (value flowback).
                    if let Some(lv) = group.find_mut(link) {
                        lv.set_value(crate::data::FieldValue::Text(s));
                    }
                }
            }
            None
        }
        // Async-modal-from-a-view (handle_event paths): route the answer + re-post.
        ModalCompletion::RouteModalAnswer {
            answer_to,
            then_command,
        } => {
            if let Some(v) = group.find_mut(answer_to) {
                v.set_modal_answer(result);
            }
            then_command.map(Event::Command)
        }
        // Informational box: nothing to route or re-post.
        ModalCompletion::Informational => None,
        // FindPick: on non-cancel, read the find string + options from the
        // in-tree Find dialog, update the editor (clear EF_DO_REPLACE), and
        // re-inject cmSearchAgain so do_search_replace runs on the editor.
        ModalCompletion::FindPick {
            editor_id,
            find_id,
            opts_id,
        } => {
            if result == Command::CANCEL {
                return None;
            }
            // Read find string from the InputLine.
            let find_str = group
                .find_mut(find_id)
                .and_then(|v| v.value())
                .and_then(field_text)
                .unwrap_or_default();
            // Read the options bit word from the CheckBoxes via value() (Bits) —
            // no downcast. Mask to bits 0-1 (case sensitive, whole words).
            let opts = group
                .find_mut(opts_id)
                .and_then(|v| v.value())
                .and_then(field_bits)
                .unwrap_or(0)
                & 0x0003;
            // Deliver the search record to the editor via set_modal_data (the
            // 2-element Find shape: find + flags, no EF_DO_REPLACE) — virtual
            // dispatch, never a downcast. The editor leaves replace_str untouched.
            if let Some(ed) = group.find_mut(editor_id) {
                ed.set_modal_data(crate::data::FieldValue::List(vec![
                    crate::data::FieldValue::Text(find_str),
                    crate::data::FieldValue::Bits(opts),
                ]));
            }
            // Re-inject cmSearchAgain to run do_search_replace on the editor.
            Some(Event::Command(Command::SEARCH_AGAIN))
        }

        // ReplacePick: on non-cancel, read find+replace strings + options,
        // set EF_DO_REPLACE, and re-inject cmSearchAgain.
        ModalCompletion::ReplacePick {
            editor_id,
            find_id,
            replace_id,
            opts_id,
        } => {
            if result == Command::CANCEL {
                return None;
            }
            let find_str = group
                .find_mut(find_id)
                .and_then(|v| v.value())
                .and_then(field_text)
                .unwrap_or_default();
            let replace_str = group
                .find_mut(replace_id)
                .and_then(|v| v.value())
                .and_then(field_text)
                .unwrap_or_default();
            // Options via value() (Bits) — no downcast. Mask to bits 0-3
            // (case, whole-words, prompt, replace-all), then set EF_DO_REPLACE
            // unconditionally for the replace flow.
            let opts = (group
                .find_mut(opts_id)
                .and_then(|v| v.value())
                .and_then(field_bits)
                .unwrap_or(0)
                & 0x000F)
                | crate::widgets::EF_DO_REPLACE as u32;
            // Deliver via set_modal_data (the 3-element Replace shape: find +
            // replace + flags) — virtual dispatch, never a downcast.
            if let Some(ed) = group.find_mut(editor_id) {
                ed.set_modal_data(crate::data::FieldValue::List(vec![
                    crate::data::FieldValue::Text(find_str),
                    crate::data::FieldValue::Text(replace_str),
                    crate::data::FieldValue::Bits(opts),
                ]));
            }
            Some(Event::Command(Command::SEARCH_AGAIN))
        }

        // saveAs result: read the picked filename from the in-tree FileDialog,
        // set it on the editor, flag the title update, and re-inject cmSave so the
        // normal save path runs save_file(ctx). The accept test is `!= CANCEL`
        // (the FileDialog's FD_OK_BUTTON ends with cmFileOpen, not cmOK — faithful
        // to C++ `saveAs`'s `editorDialog(edSaveAs, …) != cmCancel`).
        ModalCompletion::SaveAsPick { editor_id } => {
            if result != Command::CANCEL {
                // Read the chosen filename while the FileDialog is still in tree.
                // `value()` returns the resolved_name cache, kept current by the
                // `validate_modal_close → valid(endState)` that just ran.
                let filename = group
                    .find_mut(modal_id)
                    .and_then(|v| v.value())
                    .and_then(|fv| match fv {
                        crate::data::FieldValue::Text(s) => Some(s),
                        _ => None,
                    })
                    .filter(|s| !s.is_empty());
                if let Some(name) = filename
                    && let Some(ed) = group
                        .find_mut(editor_id)
                        .and_then(|v| v.as_any_mut())
                        .and_then(|a| a.downcast_mut::<crate::widgets::FileEditor>())
                {
                    // C++ saveAs: fexpand(fileName); message(owner, cmUpdateTitle);
                    // res = saveFile(). We set the name + flag the title broadcast,
                    // then re-inject cmSave to run save_file with a full ctx (the
                    // editor's cmSave handler fires the cmUpdateTitle broadcast).
                    ed.file_name = Some(std::path::PathBuf::from(&name));
                    ed.pending_title_update = true;
                    return Some(Event::Command(Command::SAVE));
                }
            }
            None
        }

        // per-role color picker result — downcast the in-tree ColorPicker,
        // read color(), write it into the ThemeEditorBody's working theme.
        ModalCompletion::ThemeColorPick {
            editor_id,
            picker,
            role,
            fg,
        } => {
            if result == Command::OK {
                let color = group
                    .find_mut(picker)
                    .and_then(|v| v.as_any_mut())
                    .and_then(|a| a.downcast_mut::<crate::dialog::ColorPicker>())
                    .map(|p| p.color());
                if let (Some(c), Some(te)) = (
                    color,
                    group
                        .find_mut(editor_id)
                        .and_then(|v| v.as_any_mut())
                        .and_then(|a| a.downcast_mut::<crate::dialog::ThemeEditorBody>()),
                ) {
                    let existing = te.working_theme().style(role);
                    let new_style = if fg {
                        crate::color::Style::new(c, existing.bg)
                    } else {
                        crate::color::Style::new(existing.fg, c)
                    };
                    te.set_role_style(role, new_style);
                }
            }
            None
        }
    }
}

/// How long the next pump pass may block waiting for a backend event:
/// `min(20 ms, time_until_next_timer)`. With no timer it is just the 20 ms frame
/// tick. Returned for the backend poll; headless ignores it and never blocks. A
/// free function (not a method) so it composes with the pump's destructured
/// borrows.
///
/// # Turbo Vision heritage
/// Ports `TProgram::eventWaitTimeout` (`tprogram.cpp`).
fn event_wait_timeout(timers: &TimerQueue, now: u64) -> Option<Duration> {
    let frame = Duration::from_millis(EVENT_TIMEOUT_MS);
    match timers.time_until_next(now) {
        Some(until) => Some(frame.min(until)),
        None => Some(frame),
    }
}

/// The platform-default shell-suspend message: the text printed before the
/// terminal is yielded to the shell (`Command::DOS_SHELL`).
///
/// Mirrors the two branches of `TApplication::writeShellMsg` (`tapplica.cpp`):
/// - Windows/DOS: `"Type EXIT to return..."`
/// - Unix: the SIGTSTP return instruction.
///
/// Users may replace this with a custom message via
/// [`Program::set_shell_msg_hook`].
fn default_shell_msg() -> String {
    #[cfg(not(unix))]
    {
        "Type EXIT to return...".to_string()
    }
    #[cfg(unix)]
    {
        "The application has been stopped. You can return by entering 'fg'.".to_string()
    }
}

/// Resolve the shell-suspend message: the registered hook if set, else the
/// platform default. The single source of truth for both the DOS_SHELL
/// handler and the tests.
fn resolve_shell_msg(hook: &Option<Box<dyn Fn() -> String>>) -> String {
    hook.as_ref().map(|h| h()).unwrap_or_else(default_shell_msg)
}

/// The program's own event handling (Alt-N window selection, quit, tile/cascade,
/// DOS-shell), then delegation to the embedded group's three-phase router.
///
/// A free function taking explicit field borrows so it composes with the pump's
/// disjoint borrows (the borrow discipline the pump requires).
///
/// # Turbo Vision heritage
/// Ports `TProgram::handleEvent` (`tprogram.cpp`).
#[allow(clippy::too_many_arguments)]
fn program_handle_event(
    group: &mut Group,
    desktop: Option<ViewId>,
    ev: &mut Event,
    ctx: &mut Context,
    end_state: &mut Option<Command>,
    app_commands: &mut VecDeque<Command>,
    renderer: &mut Renderer,
    shell_msg_hook: &Option<Box<dyn Fn() -> String>>,
) {
    // Modal-isolation note: program-level interception (this Alt-N block + the
    // cmQuit catch below) is NOT suppressed while a modal is active. C++'s nested
    // `p->execute()` (tgroup.cpp:205) structurally avoids it by dispatching to the
    // dialog's handleEvent, not TProgram's; tvision-rs's single loop runs this on every
    // pump, including modal pumps (the deviation documented on `exec_view`).
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
            .is_some_and(|dt| dt.valid(Command::RELEASED_FOCUS, ctx));
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
    // the Alt-N block's borrow style.
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

    // cmDosShell — suspend the terminal, raise SIGTSTP, resume (TApplication::dosShell,
    // tapplica.cpp: suspend() -> writeShellMsg() -> raise(SIGTSTP) -> resume() -> redraw()).
    if let Event::Command(cmd) = *ev
        && cmd == Command::DOS_SHELL
    {
        renderer.backend_mut().suspend();
        println!("{}", resolve_shell_msg(shell_msg_hook));
        #[cfg(all(unix, not(test)))]
        {
            extern crate libc;
            // SAFETY: FFI call with no memory-safety preconditions. `libc::SIGTSTP`
            // is a valid signal constant on all unix targets. The process stops here
            // and is resumed by SIGCONT ('fg'); no Rust invariants are violated
            // across the pause.
            unsafe {
                libc::raise(libc::SIGTSTP);
            }
        }
        renderer.backend_mut().resume();
        renderer.invalidate_all();
        ev.clear();
    }

    // Any command that nobody cleared is available for application-level handling
    // (the TApplication::handleEvent slot). Deposit it so run_app can drain it
    // after the pump cycle and call the user's handler with &mut Program.
    if let Event::Command(cmd) = *ev {
        app_commands.push_back(cmd);
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
            self.log.borrow_mut().push(ev.clone());
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
            self.log.borrow_mut().push(ev.clone());
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
            |_r| None, // status line stubbed
            |_r| None, // menu bar stubbed
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
        // At construction the desktop is root-current with its own `current` on the
        // topmost window #n (Program::new's startup settle_currency — the C++
        // insert-time invariant); window valid(cmReleasedFocus) (canMoveFocus) is
        // true by default and the Alt-N walk selects window 1. The pump then drains
        // `deferred`, so the
        // `EnableCommand(cmNext/cmPrev)` that `set_state(Selected)` queued is really
        // applied to `disabled_commands` — exactly the enable-filter path the
        // cmNext/cmPrev round-trip tests exercise. No test-only command-enable
        // shortcut.
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

    // -- cmTile routes through the pump to Desktop::tile --------------

    /// End to end: posting `cmTile` (as a menu item would) makes
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

    // -- cmCascade routes through the pump to Desktop::cascade --------

    /// End-to-end mirror of the cmTile test: posting `cmCascade` makes
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

    // -- Alt-N selects a numbered window ------------------------------

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

    // -- Deferred::FocusById wires through the pump ------------------

    /// End-to-end: a `Deferred::FocusById(id)` queued **during an event dispatch**
    /// (exactly when a label's `ctx.request_focus` runs — from inside
    /// `handle_event`) is drained by that same `pump_once` pass, resolved via
    /// `group.focus_descendant`, and focuses (selects) the named view.
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

    // -- Deferred::SetFullscreen wires through the pump ------------------

    /// Build a `Program` with a desktop (one window inside), a stubbed status
    /// line, and a real `MenuBar`. Returns `(program, window_id, desktop_id)`.
    /// The menu bar id is available via `program.menu_bar()` after construction.
    fn program_with_fullscreen_scaffold(
        w: u16,
        h: u16,
    ) -> (Program, crate::view::ViewId, crate::view::ViewId) {
        use crate::menu::MenuBar;
        use crate::window::Window;
        let (backend, _handle) = HeadlessBackend::new(w, h);
        let theme = Theme::classic_blue();
        let clock = Rc::new(ManualClock::new(0));
        let win_id: Rc<RefCell<Option<crate::view::ViewId>>> = Rc::new(RefCell::new(None));
        let win_id_cap = win_id.clone();
        let w_i32 = w as i32;
        let mut program = Program::new(
            Box::new(backend),
            Box::new(clock),
            theme,
            move |r| {
                let mut desktop = Desktop::new(r, |r2| Some(Desktop::init_background(r2)));
                let win = Window::new(Rect::new(2, 2, 20, 8), Some("Test".into()), 1);
                *win_id_cap.borrow_mut() = Some(desktop.insert_view(Box::new(win)));
                Some(Box::new(desktop))
            },
            |_r| None, // no status line
            move |_r| {
                Some(Box::new(MenuBar::new(
                    Rect::new(0, 0, w_i32, 1),
                    modal_menu(),
                )))
            },
        );
        program.out_events.clear();
        let window_id = win_id.borrow().unwrap();
        let desktop_id = program.desktop.unwrap();
        (program, window_id, desktop_id)
    }

    /// End-to-end: `Deferred::SetFullscreen { mode: Screen }` collapses the menu
    /// bar to the `⋮` cell, expands the desktop to cover row 0, and records the
    /// fullscreen slot.
    #[test]
    fn set_fullscreen_screen_collapses_menu_and_covers_top() {
        use crate::view::Deferred;
        use crate::window::Fullscreen;

        let (mut program, window_id, desktop_id) = program_with_fullscreen_scaffold(40, 12);

        // Push the fullscreen request and a benign broadcast so the deferred
        // drain runs (the drain requires an event dispatch pass).
        program.deferred.push(Deferred::SetFullscreen {
            window: window_id,
            mode: Fullscreen::Screen,
        });
        program.out_events.push_back(Event::Broadcast {
            command: Command::custom("test.noop"),
            source: None,
        });
        program.pump_once();

        // Menu bar collapsed to the ⋮ cell (top-right corner, width 1).
        let mb_id = program.menu_bar().expect("menu bar present");
        let mb = program
            .group_mut()
            .find_mut(mb_id)
            .expect("menu bar")
            .state()
            .get_bounds();
        assert_eq!(
            (mb.a.x, mb.b.x, mb.a.y, mb.b.y),
            (39, 40, 0, 1),
            "menu bar is the ⋮ cell at top-right"
        );

        // Desktop top moved to row 0 (covers the former menu row).
        let dt = program
            .group_mut()
            .find_mut(desktop_id)
            .expect("desktop")
            .state()
            .get_bounds();
        assert_eq!(
            dt.a.y, 0,
            "desktop covers the menu row after Screen fullscreen"
        );

        // The fullscreen slot is recorded.
        assert!(
            program.fullscreen.is_some(),
            "fullscreen slot set after SetFullscreen"
        );
    }

    /// End-to-end: `Command::FULLSCREEN` routed to the active window cycles it
    /// through Desktop → Screen modes; pump effects confirm the cross-tree layout.
    #[test]
    fn fullscreen_command_cycles_to_screen() {
        use crate::window::Fullscreen;

        let (mut program, window_id, desktop_id) = program_with_fullscreen_scaffold(40, 12);

        // Select the numbered window so the focused FULLSCREEN command routes to it.
        program.out_events.push_back(alt_digit('1'));
        program.pump_once();
        program.out_events.clear();

        // Cycle Off -> Desktop, then Desktop -> Screen (one command per pump pass;
        // the window updates its own `fullscreen` inline, so the 2nd read sees Desktop).
        program
            .out_events
            .push_back(Event::Command(Command::FULLSCREEN));
        program.pump_once();
        program
            .out_events
            .push_back(Event::Command(Command::FULLSCREEN));
        program.pump_once();

        // The slot records Screen for our window.
        let slot = program.fullscreen.as_ref().expect("fullscreen slot set");
        assert_eq!(slot.mode, Fullscreen::Screen);
        assert_eq!(slot.window, window_id);

        // Menu bar collapsed to the ⋮ cell (top-right corner, width 1).
        let mb_id = program.menu_bar().unwrap();
        let mb = program
            .group_mut()
            .find_mut(mb_id)
            .unwrap()
            .state()
            .get_bounds();
        assert_eq!((mb.a.x, mb.b.x, mb.a.y, mb.b.y), (39, 40, 0, 1));

        // Desktop covers the menu row.
        let dt = program
            .group_mut()
            .find_mut(desktop_id)
            .unwrap()
            .state()
            .get_bounds();
        assert_eq!(dt.a.y, 0);
    }

    /// **The ONLY end-to-end test of the real `program.rs`
    /// [`ResolveFocusedFile`](crate::view::Deferred::ResolveFocusedFile) pump arm**
    /// (the `cmFileFocused` payload broker). Every other test for this
    /// chain is unit-level: filedlg's tests either count the broadcast/request or
    /// *emulate* the broker apply by hand (`file_focused_round_trip_through_broker`
    /// runs `find_mut(src).focused_rec()` + `on_file_focused` itself). This test
    /// drives the genuine production chain through `pump_once`:
    ///
    /// `FileList` focus move (real `Down` key, routed by the group to the current
    /// child) → `on_focus_changed` broadcasts `FILE_FOCUSED { source = filelist }`
    /// → the broadcast is redelivered next pump to the sibling `FileInputLine`,
    /// whose `handle_event` (NOT selected, so past the `!sfSelected` guard)
    /// requests `ResolveFocusedFile` → the SAME pump's deferred-drain runs the real
    /// `program.rs` broker arm: `group.find_mut(source)` downcasts the `FileList`,
    /// reads `focused_rec()`, then `find_mut(subscriber)` downcasts the
    /// `FileInputLine` and calls `on_file_focused`, writing the focused name into
    /// the field.
    #[test]
    fn file_focused_broker_updates_input_line_through_pump() {
        use crate::dialog::{FileInputLine, FileList};

        // Deterministic listing WITHOUT the real FS leaking in: build a temp dir
        // with two known plain files. `read_directory_listing` is ctx-free and
        // reads exactly this dir; `build_listing` sorts files-before-dirs then
        // appends ".." (non-root) -> [a.rs, b.rs, ..]. So focused 0 == "a.rs",
        // and a single Down -> focused 1 == "b.rs" (both plain files, so the
        // field text is the bare name — no "name/<wildcard>" dir append to model).
        let uniq = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("rstv_file_focused_{}_{}", std::process::id(), uniq));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(dir.join("a.rs"), b"a").expect("a.rs");
        std::fs::write(dir.join("b.rs"), b"b").expect("b.rs");
        let dir_str = format!("{}/", dir.to_string_lossy());

        // Insert the FileList FIRST (so it is `current` after focus) and the
        // FileInputLine SECOND, both as siblings of the desktop group. The list is
        // populated at construction time via the ctx-free `read_directory_listing`.
        let ids: Rc<RefCell<(Option<crate::view::ViewId>, Option<crate::view::ViewId>)>> =
            Rc::new(RefCell::new((None, None)));
        let ids_cap = ids.clone();
        let dir_cap = dir_str.clone();
        let (backend, _handle) = HeadlessBackend::new(80, 25);
        let theme = Theme::classic_blue();
        let clock = Rc::new(ManualClock::new(0));
        let mut program = Program::new(
            Box::new(backend),
            Box::new(clock),
            theme,
            move |r| {
                let mut desktop = Desktop::new(r, |r2| Some(Desktop::init_background(r2)));
                let mut fl = FileList::new(Rect::new(2, 2, 32, 12), None, None);
                fl.read_directory_listing(&dir_cap, "*");
                let fl_id = desktop.insert_view(Box::new(fl));
                let fil_id = desktop.insert_view(Box::new(FileInputLine::new(
                    Rect::new(2, 14, 32, 15),
                    80,
                    "*.rs",
                )));
                *ids_cap.borrow_mut() = (Some(fl_id), Some(fil_id));
                Some(Box::new(desktop))
            },
            |_r| None,
            |_r| None,
        );
        let (fl_id, fil_id) = {
            let b = ids.borrow();
            (b.0.unwrap(), b.1.unwrap())
        };
        program.out_events.clear();

        // Focus the FileList through the production focus path (the same
        // `focus_descendant` the pump's FocusById arm uses). This makes it the
        // desktop group's `current` AND deselects the input line — so the Down key
        // routes to the list and the input line is past its `!sfSelected` guard.
        program.with_ctx(|g, ctx| g.focus_descendant(fl_id, ctx));

        // Precondition: the FileList is genuinely focused at the focused entry 0
        // ("a.rs"); fail HERE (not at a confusing empty-text assertion) if focus
        // silently failed.
        let focused_before = program
            .group_mut()
            .find_mut(fl_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<FileList>())
            .and_then(|fl| fl.focused_rec())
            .map(|r| r.name);
        assert_eq!(
            focused_before,
            Some("a.rs".to_string()),
            "FileList starts focused on item 0 (a.rs)"
        );
        assert!(
            program
                .group_mut()
                .find_mut(fil_id)
                .map(|v| !v.state().state.selected)
                .unwrap_or(false),
            "FileInputLine is NOT selected (so it will request the broker)"
        );
        program.out_events.clear();

        // Drive the focus change through the real pump: a Down key routed to the
        // current FileList moves focused 0 -> 1, firing on_focus_changed ->
        // FILE_FOCUSED broadcast (an out-event). Pump until the chain settles,
        // bounded so a wiring break fails the assertion below instead of hanging.
        // The broadcast is queued on pump N, delivered to the input line on pump
        // N+1 (which requests ResolveFocusedFile), and the broker arm runs in that
        // SAME pump's deferred-drain — so 2 pumps after the key event settle it.
        program.out_events.push_back(Event::KeyDown(KeyEvent::new(
            Key::Down,
            KeyModifiers::default(),
        )));
        let mut pumps = 0;
        let mut text = String::new();
        while pumps < 5 {
            program.pump_once();
            pumps += 1;
            text = program
                .group_mut()
                .find_mut(fil_id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<FileInputLine>())
                .map(|fil| fil.text().to_string())
                .unwrap_or_default();
            if text == "b.rs" {
                break;
            }
        }

        // Clean up the temp fixture before the assertion (so a failure still tidies).
        let _ = std::fs::remove_dir_all(&dir);

        // The REAL broker arm resolved the FileList's focused_rec() == "b.rs" and
        // wrote it into the input line — proving the production ResolveFocusedFile
        // path ran end-to-end (not an emulation).
        assert_eq!(
            text, "b.rs",
            "the production ResolveFocusedFile broker wrote the focused file name \
             into the input line through pump_once"
        );
        assert!(
            (2..=3).contains(&pumps),
            "the chain settled in 2 pumps after the Down key (broadcast queued, \
             redelivered + broker-applied next pump); took {pumps}"
        );
    }

    // -- Deferred::MakeButtonDefault wires through the pump -----------

    /// **The end-to-end test of the real `program.rs`
    /// [`MakeButtonDefault`](crate::view::Deferred::MakeButtonDefault) pump arm**
    /// (a directory list focus-change asking its companion button to become the
    /// default). filedlg's unit tests assert that `DirListBox::set_state` *queues*
    /// the variant; this drives the genuine production arm through `pump_once`:
    /// `group.find_mut(button)` downcasts the `Button` and calls
    /// `make_default(enable, ctx)`, whose grab-default re-broadcast then makes the
    /// previous default button relinquish the look — the exact
    /// `find_mut(button)`-reaching-a-nested-button path the unit tests cannot
    /// confirm.
    ///
    /// Mirrors [`deferred_focus_by_id_selects_target_through_pump`]: a benign
    /// broadcast drives a dispatch so the pump reaches its deferred-apply loop, and
    /// the `MakeButtonDefault` is pushed alongside it (the shape of "the dir list's
    /// `set_state` queues the poke during its own dispatch"). The `chDirButton` is
    /// `bfNormal`, the `okButton` `bfDefault` — so after settling the Chdir button
    /// grabbed the default and OK relinquished it.
    #[test]
    fn deferred_make_button_default_grabs_default_through_pump() {
        use crate::command::Command;
        use crate::widgets::{Button, ButtonFlags};

        fn am_default(program: &mut Program, id: crate::view::ViewId) -> bool {
            program
                .group_mut()
                .find_mut(id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<Button>())
                .map(|b| b.am_default)
                .expect("button resolves")
        }

        let ids: Rc<RefCell<(Option<crate::view::ViewId>, Option<crate::view::ViewId>)>> =
            Rc::new(RefCell::new((None, None)));
        let ids_cap = ids.clone();
        let (backend, _handle) = HeadlessBackend::new(80, 25);
        let theme = Theme::classic_blue();
        let clock = Rc::new(ManualClock::new(0));
        let mut program = Program::new(
            Box::new(backend),
            Box::new(clock),
            theme,
            move |r| {
                let mut desktop = Desktop::new(r, |r2| Some(Desktop::init_background(r2)));
                // okButton: bfDefault (the initial default). chDirButton: bfNormal.
                // Insert chdir FIRST so the bfDefault okButton is topmost: startup
                // currency (Program::new's settle_currency, the C++ insert-time
                // show()->resetCurrent invariant) focuses the topmost selectable
                // child, and a focused NON-default button would grab the default
                // (TButton::setState sfFocused -> makeDefault) before the test's
                // preconditions run. Focusing the bfDefault button is a no-op
                // (makeDefault's `(flags & bfDefault) == 0` guard).
                let ok = Button::new(
                    Rect::new(2, 2, 12, 4),
                    "O~K~",
                    Command::OK,
                    ButtonFlags {
                        default: true,
                        ..Default::default()
                    },
                );
                let chdir = Button::new(
                    Rect::new(2, 5, 12, 7),
                    "~C~hdir",
                    Command::CHANGE_DIR,
                    ButtonFlags::new(),
                );
                let chdir_id = desktop.insert_view(Box::new(chdir));
                let ok_id = desktop.insert_view(Box::new(ok));
                *ids_cap.borrow_mut() = (Some(ok_id), Some(chdir_id));
                Some(Box::new(desktop))
            },
            |_r| None,
            |_r| None,
        );
        let (ok_id, chdir_id) = {
            let b = ids.borrow();
            (b.0.unwrap(), b.1.unwrap())
        };
        program.out_events.clear();

        // Preconditions: bfDefault initializes am_default; bfNormal does not.
        assert!(
            am_default(&mut program, ok_id),
            "okButton starts the default"
        );
        assert!(
            !am_default(&mut program, chdir_id),
            "chDirButton starts non-default"
        );

        // Queue the makeDefault poke exactly as `DirListBox::set_state` would on an
        // sfFocused change, and drive a dispatch with a benign broadcast so the pump
        // reaches its deferred-apply loop. The arm downcasts the chDirButton and
        // calls make_default(true): it grabs the default + broadcasts cmGrabDefault.
        program.deferred.push(Deferred::MakeButtonDefault {
            button: chdir_id,
            enable: true,
        });
        program.out_events.push_back(Event::Broadcast {
            command: Command::custom("test.noop"),
            source: None,
        });
        program.pump_once();

        assert!(
            am_default(&mut program, chdir_id),
            "the MakeButtonDefault arm made the chDirButton the default through the pump"
        );

        // The make_default broadcast (cmGrabDefault, source = chDirButton) settles
        // next pump: the bfDefault okButton receives it and relinquishes the look.
        program.pump_once();
        assert!(
            !am_default(&mut program, ok_id),
            "okButton relinquished the default on the chDirButton's cmGrabDefault"
        );
    }

    // -- saveAs: the SaveAsPick modal completion -------------------------------

    /// `apply_modal_completion(SaveAsPick, FILE_OPEN, …)` reads the picked filename
    /// from the in-tree modal, sets it on the `FileEditor`, flags the title update,
    /// and returns a re-injected `cmSave`. The accept command is `cmFileOpen` (the
    /// FileDialog's FD_OK_BUTTON), NOT `cmOK` — the `!= CANCEL` test must accept it.
    #[test]
    fn save_as_pick_sets_filename_and_reinjects_save() {
        use crate::data::FieldValue;
        use crate::widgets::{FileEditor, InputLine};

        let mut group = Group::new(Rect::new(0, 0, 80, 25));
        let editor_id = group.insert(Box::new(FileEditor::new(
            Rect::new(0, 0, 40, 10),
            None,
            None,
            None,
            None,
        )));
        // Stand-in modal whose `value()` yields the picked Text (a FileDialog's
        // resolved_name role) — an InputLine returns FieldValue::Text by default.
        let mut il = InputLine::with_limit(Rect::new(0, 0, 40, 1), 256);
        il.set_value(FieldValue::Text("/tmp/picked.txt".to_string()));
        let modal_id = group.insert(Box::new(il));

        let reinject = apply_modal_completion(
            ModalCompletion::SaveAsPick { editor_id },
            Command::FILE_OPEN, // FD_OK_BUTTON ends the modal with cmFileOpen
            &mut group,
            modal_id,
        );

        assert_eq!(
            reinject,
            Some(Event::Command(Command::SAVE)),
            "SaveAsPick re-injects cmSave"
        );
        let fe = group
            .find_mut(editor_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<FileEditor>())
            .expect("editor resolves");
        assert_eq!(
            fe.file_name.as_deref(),
            Some(std::path::Path::new("/tmp/picked.txt")),
            "picked filename set on the editor"
        );
        assert!(fe.pending_title_update, "title-update flag set");
    }

    /// `SaveAsPick` on cmCancel sets nothing and re-injects nothing.
    #[test]
    fn save_as_pick_cancel_is_noop() {
        use crate::data::FieldValue;
        use crate::widgets::{FileEditor, InputLine};

        let mut group = Group::new(Rect::new(0, 0, 80, 25));
        let editor_id = group.insert(Box::new(FileEditor::new(
            Rect::new(0, 0, 40, 10),
            None,
            None,
            None,
            None,
        )));
        let mut il = InputLine::with_limit(Rect::new(0, 0, 40, 1), 256);
        il.set_value(FieldValue::Text("/tmp/ignored.txt".to_string()));
        let modal_id = group.insert(Box::new(il));

        let reinject = apply_modal_completion(
            ModalCompletion::SaveAsPick { editor_id },
            Command::CANCEL,
            &mut group,
            modal_id,
        );
        assert_eq!(reinject, None, "cancel re-injects nothing");
        let fe = group
            .find_mut(editor_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<FileEditor>())
            .unwrap();
        assert!(fe.file_name.is_none(), "cancel leaves the editor untitled");
        assert!(!fe.pending_title_update, "cancel sets no title flag");
    }

    /// The `OpenSaveAsDialog` deferred, when drained by the pump, builds a
    /// `FileDialog` and stashes it into `pending_modal` with a `SaveAsPick`
    /// completion. Driven directly through the pump's deferred-drain arm (not via
    /// `pump_and_drive`, which would launch the modal loop and hang headless).
    #[test]
    fn open_save_as_dialog_deferred_stashes_pending_modal() {
        use crate::widgets::FileEditor;

        let (mut program, _handle, _clock) = program_with_desktop(80, 25);
        let editor_id = program.group_mut().insert(Box::new(FileEditor::new(
            Rect::new(0, 0, 40, 10),
            None,
            None,
            None,
            None,
        )));
        program.out_events.clear();

        // Queue the request + a benign broadcast so the pump picks a Some(ev)
        // and reaches its deferred drain (which runs for every picked event,
        // consumed-by-pre-route or not).
        program
            .deferred
            .push(Deferred::OpenSaveAsDialog { editor_id });
        program.out_events.push_back(Event::Broadcast {
            command: Command::custom("test.noop"),
            source: None,
        });
        program.pump_once();

        let stashed = program.pending_modal.take().expect("pending_modal set");
        let (view, completion, focus) = stashed;
        assert!(
            matches!(completion, ModalCompletion::SaveAsPick { editor_id: e } if e == editor_id),
            "SaveAsPick completion targets the editor"
        );
        assert!(focus.is_none(), "FileDialog manages its own focus");
        // The stashed view is a FileDialog (downcast via as_any_mut on the box).
        let mut view = view;
        assert!(
            view.as_any_mut()
                .and_then(|a| a.downcast_mut::<crate::dialog::FileDialog>())
                .is_some(),
            "the stashed modal is a FileDialog"
        );
    }

    /// `Context::request_exec_view` queues a `Deferred::OpenModal`; the pump's
    /// deferred-drain arm moves the boxed modal into `pending_modal` with a
    /// `RouteModalAnswer { answer_to, then_command }` completion and `None` initial
    /// focus (the modal focuses its own first view). Driven through `pump_once`
    /// (NOT `pump_and_drive`, which would launch the modal loop and hang headless —
    /// see the warning above `open_save_as_dialog_deferred_stashes_pending_modal`).
    #[test]
    fn request_exec_view_deferred_stashes_pending_modal() {
        use crate::dialog::Dialog;

        let (mut program, _handle, _clock) = program_with_desktop(80, 25);
        // Any inserted view's id serves as the requester (here: a plain child).
        let requester = program
            .group_mut()
            .insert(Box::new(Dialog::new(Rect::new(0, 0, 20, 8), None)));
        program.out_events.clear();

        let then_cmd = Command::custom("test.exec_view.then");
        // Reach `request_exec_view` through a throwaway Context (the view-facing
        // entry point), exactly as a leaf view would from `handle_event`.
        program.with_ctx(|_g, ctx| {
            ctx.request_exec_view(
                Box::new(Dialog::new(
                    Rect::new(5, 5, 35, 15),
                    Some("Modal".to_string()),
                )),
                requester,
                Some(then_cmd),
            );
        });
        // A benign broadcast so the pump picks a Some(ev) and reaches its deferred
        // drain (mirrors the save-as seam test).
        program.out_events.push_back(Event::Broadcast {
            command: Command::custom("test.noop"),
            source: None,
        });
        program.pump_once();

        let stashed = program.pending_modal.take().expect("pending_modal set");
        let (view, completion, focus) = stashed;
        assert!(
            matches!(
                completion,
                ModalCompletion::RouteModalAnswer { answer_to, then_command }
                    if answer_to == requester && then_command == Some(then_cmd)
            ),
            "RouteModalAnswer routes to the requester + carries then_command"
        );
        assert!(focus.is_none(), "the modal focuses its own first view");
        // The caller-built modal box was moved verbatim into pending_modal (Dialog
        // does not override `as_any_mut`, so identity is established by the move —
        // the completion routing above is the load-bearing assertion).
        let _ = view;
    }

    /// A test-only requester view whose [`View::set_modal_answer`] records the
    /// command the pump routed to it (the modal's close command). Non-selectable so
    /// it never steals focus from the modal.
    struct ModalAnswerRecorder {
        st: ViewState,
        answer: Rc<RefCell<Option<Command>>>,
    }
    impl ModalAnswerRecorder {
        fn new(bounds: Rect, answer: Rc<RefCell<Option<Command>>>) -> Self {
            ModalAnswerRecorder {
                st: ViewState::new(bounds),
                answer,
            }
        }
    }
    impl View for ModalAnswerRecorder {
        fn state(&self) -> &ViewState {
            &self.st
        }
        fn state_mut(&mut self) -> &mut ViewState {
            &mut self.st
        }
        fn draw(&mut self, _ctx: &mut DrawCtx) {}
        fn set_modal_answer(&mut self, cmd: Command) {
            *self.answer.borrow_mut() = Some(cmd);
        }
    }

    /// End-to-end round-trip: a `request_exec_view` modal opens, the user closes it
    /// (Enter → OK button → `end_modal(Command::OK)`), and the completion routes the
    /// close command to the `requester` via `set_modal_answer` AND re-injects
    /// `then_command` into `out_events`. Drives the real `pump_and_drive` loop to
    /// completion (Enter is pre-queued so the modal closes — never an unbounded
    /// headless spin).
    #[test]
    fn request_exec_view_round_trip_routes_answer_and_reinjects() {
        let (mut program, _handle, _clock) = program_with_desktop(80, 25);

        let answer = Rc::new(RefCell::new(None));
        let recorder_id = program
            .group_mut()
            .insert(Box::new(ModalAnswerRecorder::new(
                Rect::new(0, 0, 10, 3),
                answer.clone(),
            )));
        program.out_events.clear();

        let then_cmd = Command::custom("test.exec_view.reinject");
        // Build an OK-only message box (Command::OK ends its modal) and request it
        // as a view-launched modal.
        program.with_ctx(|_g, ctx| {
            let r = Rect::new(20, 8, 60, 16);
            let (boxd, _first) = crate::dialog::build_message_box(
                r,
                "exec view",
                crate::dialog::MessageBoxKind::Information,
                crate::dialog::MessageBoxButtons::ok(),
            );
            ctx.request_exec_view(Box::new(boxd), recorder_id, Some(then_cmd));
        });

        // Sequence the re-inject queue (events are popped before the backend poll,
        // by both pump_once and the inner modal loop):
        //   1. a benign broadcast so the FIRST pump_once picks a Some(ev) and
        //      reaches its deferred drain (which stashes pending_modal),
        //   2. Command::OK so the modal loop (run by the same pump_and_drive)
        //      closes the box with OK — mirrors message_box_direct_ok_returns_ok.
        // Without (2) the headless modal loop would spin (it never blocks).
        program.out_events.push_back(Event::Broadcast {
            command: Command::custom("test.noop"),
            source: None,
        });
        program.out_events.push_back(Event::Command(Command::OK));

        // Drive: first pump stashes pending_modal, the next executes the modal
        // (which closes on the queued Command::OK). The RouteModalAnswer completion pushes
        // `then_cmd` into out_events right after the modal closes; a later pump pops
        // it. Scan out_events each iteration so the re-injection is observed before
        // it is consumed. Bounded — never unbounded.
        let mut reinjected_seen = false;
        for _ in 0..12 {
            if program
                .out_events
                .iter()
                .any(|e| matches!(e, Event::Command(c) if *c == then_cmd))
            {
                reinjected_seen = true;
            }
            program.pump_and_drive();
        }
        // Final scan: the re-injected `then_cmd` may have been queued by the
        // final `pump_and_drive` iteration and not yet consumed by a subsequent
        // pump, so the in-loop scan could miss it.  Check once more after the
        // loop to catch this case.
        if program
            .out_events
            .iter()
            .any(|e| matches!(e, Event::Command(c) if *c == then_cmd))
        {
            reinjected_seen = true;
        }

        // The pump routed the modal's close command (Command::OK) to the recorder.
        assert_eq!(
            *answer.borrow(),
            Some(Command::OK),
            "the modal's close command is delivered to the requester via set_modal_answer"
        );
        // The modal is gone.
        assert!(
            program.pending_modal.is_none(),
            "pending_modal is cleared after the modal closes"
        );
        // The then_command was re-injected by the RouteModalAnswer completion.
        assert!(
            reinjected_seen,
            "then_command was re-injected into out_events after the modal closed"
        );
    }

    // -- button mouse hold-tracking end-to-end through the pump ------------

    /// **End-to-end test of the button mouse press-and-hold tracking (the
    /// `MouseTrackCapture` seam + `Deferred::MouseTrack` pump arm).**
    ///
    /// A button is inserted into the desktop at `(5, 5, 15, 7)` so its absolute
    /// origin is `(5, 5)`. An initial pump-draw caches `abs_origin`. Then:
    ///
    /// 1. `MouseDown` at absolute `(8, 5)` = button-local `(3, 0)` (inside
    ///    `clickRect`) → `button.down == true`, one capture pushed.
    /// 2. `MouseMove` at absolute `(4, 5)` = button-local `(-1, 0)` (outside
    ///    `trackRect`) → the capture routes the localized move into the
    ///    button's loop-body arm → `button.down == false`.
    /// 3. `MouseMove` at absolute `(8, 5)` = button-local `(3, 0)` (inside) →
    ///    `button.down == true`.
    /// 4. `MouseUp` → the post-loop arm presses (last tracked state was
    ///    inside) → command fired, `button.down == false`, capture popped.
    ///
    /// Button bounds: `(5, 5, 15, 7)` — size 10×2.
    ///   clickRect  = (1, 0, 9, 1) (button-local)
    ///   trackRect  = (1, 0, 10, 1) (clickRect widened b.x by 1)
    ///   abs_origin = (5, 5) (desktop is at (0,0) in root)
    #[test]
    fn button_track_capture_end_to_end_through_pump() {
        use crate::widgets::{Button, ButtonFlags};

        // Helper to read b.down from the tree.
        fn btn_down(program: &mut Program, id: ViewId) -> bool {
            program
                .group_mut()
                .find_mut(id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<Button>())
                .map(|b| b.down)
                .expect("button resolves")
        }

        let ids: Rc<RefCell<Option<ViewId>>> = Rc::new(RefCell::new(None));
        let ids_cap = ids.clone();
        let (backend, _handle) = HeadlessBackend::new(80, 25);
        let theme = Theme::classic_blue();
        let clock = Rc::new(ManualClock::new(0));
        let mut program = Program::new(
            Box::new(backend),
            Box::new(clock),
            theme,
            move |r| {
                let mut desktop = Desktop::new(r, |r2| Some(Desktop::init_background(r2)));
                // Button at (5, 5, 15, 7) — 10×2 in the desktop.
                let btn = Button::new(
                    Rect::new(5, 5, 15, 7),
                    "~O~K",
                    Command::OK,
                    ButtonFlags::new(),
                );
                let btn_id = desktop.insert_view(Box::new(btn));
                *ids_cap.borrow_mut() = Some(btn_id);
                Some(Box::new(desktop))
            },
            |_r| None,
            |_r| None,
        );
        let btn_id = ids.borrow().expect("button id set");
        program.out_events.clear();

        // Pump a noop broadcast so the tree draws and button.abs_origin is cached.
        program.out_events.push_back(Event::Broadcast {
            command: Command::custom("test.noop"),
            source: None,
        });
        program.pump_once();
        // abs_origin is now (5, 5): the button is at (5, 5) in the desktop
        // which is at (0, 0) in the root group.

        // --- Step 1: MouseDown at abs (8, 5) = button-local (3, 0) — inside
        // clickRect (1, 0, 9, 1). Expect: down == true, capture pushed.
        program.out_events.push_back(mouse_down_at(8, 5));
        program.pump_once();
        assert!(
            btn_down(&mut program, btn_id),
            "button.down = true after MouseDown inside clickRect"
        );
        assert_eq!(program.capture_len(), 1, "capture pushed after MouseDown");
        // No command posted yet (press fires on MouseUp, not MouseDown).
        assert!(
            !program
                .out_events
                .iter()
                .any(|e| *e == Event::Command(Command::OK)),
            "no command immediately after MouseDown"
        );

        // --- Step 2: MouseMove at abs (4, 5) = button-local (-1, 0) — outside
        // trackRect (1, 0, 10, 1). Capture posts Deferred::MouseTrack with the
        // localized move; the pump delivers it to the button's loop-body arm:
        // button.down = false.
        program.out_events.push_back(mouse_move_at(4, 5));
        program.pump_once();
        assert!(
            !btn_down(&mut program, btn_id),
            "button.down = false after MouseMove outside trackRect"
        );
        assert_eq!(
            program.capture_len(),
            1,
            "capture still live after MouseMove outside"
        );

        // --- Step 3: MouseMove at abs (8, 5) = button-local (3, 0) — inside.
        program.out_events.push_back(mouse_move_at(8, 5));
        program.pump_once();
        assert!(
            btn_down(&mut program, btn_id),
            "button.down = true after MouseMove back inside trackRect"
        );
        assert_eq!(
            program.capture_len(),
            1,
            "capture still live after re-enter"
        );

        // --- Step 4: MouseUp — forwarded to the post-loop arm; last tracked
        // state was inside → press() fires, down = false, capture popped.
        program.out_events.push_back(mouse_up_at(8, 5));
        program.pump_once();
        assert!(
            !btn_down(&mut program, btn_id),
            "button.down = false after MouseUp"
        );
        assert_eq!(program.capture_len(), 0, "capture popped on MouseUp");
        // press() posts RECORD_HISTORY + Command::OK.
        let drained: Vec<Event> = program.out_events.drain(..).collect();
        assert!(
            drained.contains(&Event::Broadcast {
                command: Command::RECORD_HISTORY,
                source: None
            }),
            "RECORD_HISTORY broadcast fired"
        );
        assert!(
            drained.contains(&Event::Command(Command::OK)),
            "Command::OK fired after MouseUp inside"
        );
    }

    /// Press inside, drag outside, release: NO press fires (the C++ post-loop
    /// `if (down)` on the LAST MOVE's tracked containment) — through real pumps.
    #[test]
    fn button_release_outside_does_not_press_through_pump() {
        use crate::widgets::{Button, ButtonFlags};

        let (backend, _handle) = HeadlessBackend::new(80, 25);
        let theme = Theme::classic_blue();
        let clock = Rc::new(ManualClock::new(0));
        let ids: Rc<RefCell<Option<ViewId>>> = Rc::new(RefCell::new(None));
        let ids_cap = ids.clone();
        let mut program = Program::new(
            Box::new(backend),
            Box::new(clock),
            theme,
            move |r| {
                let mut desktop = Desktop::new(r, |r2| Some(Desktop::init_background(r2)));
                let btn = Button::new(
                    Rect::new(5, 5, 15, 7),
                    "~O~K",
                    Command::OK,
                    ButtonFlags::new(),
                );
                *ids_cap.borrow_mut() = Some(desktop.insert_view(Box::new(btn)));
                Some(Box::new(desktop))
            },
            |_r| None,
            |_r| None,
        );
        program.out_events.clear();

        // Draw once so abs_origin is cached, then press inside (abs (8,5)).
        program.out_events.push_back(Event::Broadcast {
            command: Command::custom("test.noop"),
            source: None,
        });
        program.pump_once();
        program.out_events.push_back(mouse_down_at(8, 5));
        program.pump_once();
        assert_eq!(program.capture_len(), 1, "tracking armed");

        // Drag outside the track rect, then release.
        program.out_events.push_back(mouse_move_at(2, 10));
        program.pump_once();
        program.out_events.push_back(mouse_up_at(2, 10));
        program.pump_once();
        assert_eq!(program.capture_len(), 0, "capture popped on MouseUp");
        let drained: Vec<Event> = program.out_events.drain(..).collect();
        assert!(
            !drained.contains(&Event::Command(Command::OK)),
            "release outside the track rect must not press"
        );
    }

    /// End-to-end: a held mouse button on a scrollbar's down-arrow steps the
    /// value on the synthesizer's Borland cadence (440 ms then 110 ms), pauses
    /// when the held position moves off the arrow, and stops on MouseUp — all
    /// through real pumps (MouseDown via the backend queue arms the synthesizer;
    /// idle pumps synthesize `MouseAuto`s; the `MouseTrackCapture` localizes and
    /// forwards them to the bar's `MouseAuto` arm via `Deferred::MouseTrack`).
    ///
    /// Scrollbar bounds: `(10, 5, 11, 15)` — vertical 1×10 in the desktop.
    ///   s = 9; value 50 of [0,100] → pos = 5.
    ///   down-arrow cell = bar-local (0, 9) = abs (10, 14).
    ///   trough (PageUp) cell = bar-local (0, 3) = abs (10, 8).
    #[test]
    fn scrollbar_arrow_hold_auto_repeats_through_pump() {
        use crate::widgets::ScrollBar;

        // Helper to read sb.value from the tree.
        fn sb_value(program: &mut Program, id: ViewId) -> i32 {
            program
                .group_mut()
                .find_mut(id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<ScrollBar>())
                .map(|sb| sb.value)
                .expect("scrollbar resolves")
        }

        let ids: Rc<RefCell<Option<ViewId>>> = Rc::new(RefCell::new(None));
        let ids_cap = ids.clone();
        let (backend, handle) = HeadlessBackend::new(80, 25);
        let theme = Theme::classic_blue();
        let clock = Rc::new(ManualClock::new(0));
        let mut program = Program::new(
            Box::new(backend),
            Box::new(clock.clone()),
            theme,
            move |r| {
                let mut desktop = Desktop::new(r, |r2| Some(Desktop::init_background(r2)));
                let mut sb = ScrollBar::new(Rect::new(10, 5, 11, 15)); // vertical 1×10
                // Range/value set directly (pub fields) — set_params needs a ctx.
                sb.value = 50;
                sb.min_value = 0;
                sb.max_value = 100;
                sb.page_step = 5;
                sb.arrow_step = 1;
                *ids_cap.borrow_mut() = Some(desktop.insert_view(Box::new(sb)));
                Some(Box::new(desktop))
            },
            |_r| None,
            |_r| None,
        );
        let sb_id = ids.borrow().expect("scrollbar id set");

        // Settle the startup queue, then draw once so abs_origin is cached (10,5).
        for _ in 0..3 {
            program.pump_once();
        }
        program.out_events.clear();

        // --- MouseDown on the down-arrow (abs (10,14) = bar-local (0,9)) via the
        // backend queue, so the pump's pick observes it and ARMS the synthesizer.
        handle.push_event(mouse_down_at(10, 14));
        program.pump_once();
        assert_eq!(
            sb_value(&mut program, sb_id),
            51,
            "first step on MouseDown (loop body runs once before the first wait)"
        );
        assert_eq!(program.capture_len(), 1, "auto track armed");
        // Quiesce the CLICKED/CHANGED broadcasts so later picks are idle.
        pump_until_quiet(&mut program);

        // --- +439 ms: inside the 440 ms initial delay — no auto, no step.
        clock.set(439);
        program.pump_once();
        pump_until_quiet(&mut program);
        assert_eq!(
            sb_value(&mut program, sb_id),
            51,
            "no step inside the delay"
        );

        // --- +440 ms: the first synthesized auto, still over the arrow → step 2.
        clock.set(440);
        program.pump_once();
        assert_eq!(
            sb_value(&mut program, sb_id),
            52,
            "second step on the first auto (+440 ms)"
        );
        pump_until_quiet(&mut program);

        // --- +550 ms (440 + 110): the second auto → step 3.
        clock.set(550);
        program.pump_once();
        assert_eq!(
            sb_value(&mut program, sb_id),
            53,
            "third step on the second auto (+550 ms)"
        );
        pump_until_quiet(&mut program);

        // --- Held position moves OFF the arrow into the trough (abs (10,8) =
        // bar-local (0,3) = PageUp). The capture swallows the move (auto-only
        // mask) but the synthesizer's position bookkeeping updates, so the next
        // auto re-derives PageUp ≠ DownArrow → the stepping PAUSES.
        handle.push_event(mouse_move_at(10, 8));
        program.pump_once();
        pump_until_quiet(&mut program);
        clock.set(660);
        program.pump_once();
        pump_until_quiet(&mut program);
        assert_eq!(
            sb_value(&mut program, sb_id),
            53,
            "auto over the trough does not step (part mismatch pauses the repeat)"
        );
        assert_eq!(program.capture_len(), 1, "capture stays during the pause");

        // --- MouseUp ends the hold: capture pops, synthesizer disarms; a far
        // clock advance produces no further step.
        handle.push_event(mouse_up_at(10, 8));
        program.pump_once();
        assert_eq!(program.capture_len(), 0, "capture popped on MouseUp");
        pump_until_quiet(&mut program);
        clock.set(5_000);
        program.pump_once();
        program.pump_once();
        assert_eq!(
            sb_value(&mut program, sb_id),
            53,
            "no step after MouseUp (synthesizer disarmed, track ended)"
        );
    }

    // -- the global evMouseAuto synthesizer ------------------------------

    /// Build a program with a 10×4 recording probe at the screen origin whose
    /// `event_mask.mouse_auto` is `mouse_auto_mask`, settle the startup queue,
    /// and clear the log. Probe-local == absolute (probe at (0,0) in the root).
    fn auto_probe_program(
        mouse_auto_mask: bool,
    ) -> (
        Program,
        HeadlessHandle,
        Rc<ManualClock>,
        Rc<RefCell<Vec<Event>>>,
    ) {
        let (mut program, handle, clock) = program_with_desktop(80, 25);
        let log = Rc::new(RefCell::new(Vec::new()));
        {
            let mut probe = Probe::new(Rect::new(0, 0, 10, 4), 'P', log.clone());
            probe.st.event_mask.mouse_auto = mouse_auto_mask;
            program.group_mut().insert(Box::new(probe));
        }
        // Settle: drain insert-time broadcasts / command-set-changed idle posts
        // so later passes are genuinely idle (the synthesizer only fires on an
        // idle pick — real events always win).
        for _ in 0..3 {
            program.pump_once();
        }
        program.out_events.clear();
        log.borrow_mut().clear();
        (program, handle, clock, log)
    }

    /// Pump until the internal queue is empty, so the next pass is genuinely
    /// idle (a routed MouseDown queues focus broadcasts that would otherwise
    /// win the pick over the synthesizer — real events always win).
    fn pump_until_quiet(program: &mut Program) {
        for _ in 0..10 {
            if program.out_events.is_empty() {
                return;
            }
            program.pump_once();
        }
        panic!("queue did not quiesce within 10 pumps");
    }

    fn autos_at(log: &Rc<RefCell<Vec<Event>>>) -> Vec<Point> {
        log.borrow()
            .iter()
            .filter_map(|e| match e {
                Event::MouseAuto(m) => Some(m.position),
                _ => None,
            })
            .collect()
    }

    /// The Borland cadence: no auto at +439 ms; the first at +440 carrying the
    /// down position; none at +549; the second at +550 (440 + 110).
    #[test]
    fn mouse_auto_fires_at_delay_then_period() {
        let (mut program, handle, clock, log) = auto_probe_program(true);

        // Press at (2, 1) over the probe: arms the synthesizer.
        handle.push_event(mouse_down_at(2, 1));
        program.pump_once();
        pump_until_quiet(&mut program);
        log.borrow_mut().clear();

        // +439 ms: idle pass, still inside the initial delay.
        clock.set(439);
        program.pump_once();
        assert!(
            autos_at(&log).is_empty(),
            "no auto at +439 ms (delay is 440)"
        );

        // +440 ms: the first auto, at the down position.
        clock.set(440);
        program.pump_once();
        assert_eq!(
            autos_at(&log),
            vec![Point::new(2, 1)],
            "first auto at +440 ms carries the down position"
        );

        // +549 ms: inside the 110 ms steady-state period.
        clock.set(549);
        program.pump_once();
        assert_eq!(
            autos_at(&log).len(),
            1,
            "no auto at +549 ms (period is 110)"
        );

        // +550 ms: the second auto.
        clock.set(550);
        program.pump_once();
        assert_eq!(autos_at(&log).len(), 2, "second auto at +550 ms");
    }

    /// An interleaved `MouseMove` updates the auto position WITHOUT resetting
    /// the cadence — faithful: the C++ move arm updates `lastMouse` only
    /// (tevent.cpp:188-194); only a new press re-arms the 440 ms delay.
    #[test]
    fn mouse_auto_move_updates_position_without_resetting_cadence() {
        let (mut program, handle, clock, log) = auto_probe_program(true);

        handle.push_event(mouse_down_at(2, 1));
        program.pump_once();
        pump_until_quiet(&mut program);
        log.borrow_mut().clear();

        // +200 ms: a real move to (5, 2) — position bookkeeping only.
        clock.set(200);
        handle.push_event(mouse_move_at(5, 2));
        program.pump_once();
        pump_until_quiet(&mut program);

        // +440 ms (NOT 200 + 440): the auto fires on the original deadline,
        // carrying the MOVED position.
        clock.set(440);
        program.pump_once();
        assert_eq!(
            autos_at(&log),
            vec![Point::new(5, 2)],
            "auto fires at the un-reset +440 deadline, at the moved position"
        );
    }

    /// `MouseUp` stops the autos; a re-press re-arms the full 440 ms delay.
    #[test]
    fn mouse_auto_stops_on_up_and_repress_rearms() {
        let (mut program, handle, clock, log) = auto_probe_program(true);

        handle.push_event(mouse_down_at(2, 1));
        program.pump_once();
        pump_until_quiet(&mut program);
        clock.set(440);
        program.pump_once();
        assert_eq!(autos_at(&log).len(), 1, "held button autos");
        log.borrow_mut().clear();

        // Release: no more autos, ever.
        handle.push_event(mouse_up_at(2, 1));
        program.pump_once();
        pump_until_quiet(&mut program);
        clock.set(2000);
        program.pump_once();
        program.pump_once();
        assert!(autos_at(&log).is_empty(), "MouseUp disarms the synthesizer");

        // Re-press at +2000: the full 440 ms delay applies again.
        handle.push_event(mouse_down_at(3, 1));
        program.pump_once();
        pump_until_quiet(&mut program);
        clock.set(2439);
        program.pump_once();
        assert!(autos_at(&log).is_empty(), "re-press re-arms the full delay");
        clock.set(2440);
        program.pump_once();
        assert_eq!(
            autos_at(&log),
            vec![Point::new(3, 1)],
            "auto at re-press + 440 ms"
        );
    }

    /// An `Event::MouseWheel` event (crossterm ScrollUp/Down) must never arm
    /// the synthesizer.
    #[test]
    fn mouse_auto_wheel_pseudo_down_never_arms() {
        let (mut program, handle, clock, log) = auto_probe_program(true);

        handle.push_event(Event::MouseWheel(MouseEvent {
            position: Point::new(2, 1),
            wheel: crate::event::MouseWheel::Up,
            ..Default::default()
        }));
        program.pump_once();
        pump_until_quiet(&mut program);
        clock.set(1000);
        program.pump_once();
        program.pump_once();
        assert!(
            autos_at(&log).is_empty(),
            "a buttonless wheel pseudo-down never arms evMouseAuto"
        );
    }

    /// End-to-end routing proof without any press-and-hold widget: a probe with
    /// `event_mask.mouse_auto = true` receives the synthesized autos through
    /// the normal positional routing (`Group::wants`); a probe without the
    /// opt-in does not.
    #[test]
    fn mouse_auto_routing_respects_event_mask() {
        // Opted out: the synthesizer fires, but Group::wants gates delivery.
        let (mut program, handle, clock, log) = auto_probe_program(false);
        handle.push_event(mouse_down_at(2, 1));
        program.pump_once();
        pump_until_quiet(&mut program);
        clock.set(440);
        program.pump_once();
        assert!(
            autos_at(&log).is_empty(),
            "a probe without event_mask.mouse_auto receives no autos"
        );
        // (The opted-in case is covered by mouse_auto_fires_at_delay_then_period.)
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

    // -- startup currency: insert-time show()->resetCurrent, collapsed ---------

    /// The examples/hello.rs shape: a desktop pre-populated with staggered
    /// windows via the ctx-less `Desktop::insert_view`. The eager
    /// `settle_currency` pass at the end of `Program::new` guarantees the topmost
    /// top-select window is current by `run()` time. Bite (the fixed bug):
    /// without it NO window was focused at startup, and a click on the topmost
    /// window was a complete no-op (focus_child then make_first hit
    /// put_in_front_of's already-in-place no-op, so set_current never ran).
    #[test]
    fn startup_focuses_topmost_preinserted_window_and_click_moves_focus() {
        // Build in the hello.rs shape (program_with_windows minus its Alt-1
        // pre-selection pump — this test asserts the STARTUP state itself).
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
                for num in 1..=3i16 {
                    // Staggered: w1 (4,1)-(24,9), w2 (6,2)-(26,10), w3 (8,3)-(28,11).
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
        let (w1, w2, w3) = {
            let b = ids.borrow();
            (b[0], b[1], b[2])
        };

        // Startup = new + first pump (the pump pops the startup focus broadcasts
        // and drains the deferred command enables).
        program.pump_once();

        // The LAST-inserted (topmost) window is the desktop's current: selected,
        // focused, and active (the active-frame render state).
        let flags = |program: &mut Program, id| {
            program
                .group_mut()
                .find_mut(id)
                .map(|v| {
                    let s = &v.state().state;
                    (s.selected, s.focused, s.active)
                })
                .expect("window resolves")
        };
        assert_eq!(
            flags(&mut program, w3),
            (true, true, true),
            "topmost pre-inserted window is current at startup (selected+focused+active frame)"
        );
        assert!(!win_selected(&mut program, w1), "w1 not selected");
        assert!(!win_selected(&mut program, w2), "w2 not selected");

        // Drain the remaining startup focus broadcasts (the pump pops one event
        // per pass; the click below must be the next event the pump sees).
        program.out_events.clear();

        // Regression for the normal path: click a LOWER window's title bar
        // ((7,2) is on w2's frame row, outside w3) — focus must move to it.
        program.out_events.push_back(mouse_down_at(7, 2));
        program.pump_once();
        assert_eq!(
            flags(&mut program, w2),
            (true, true, true),
            "clicking a lower window focuses it"
        );
        assert!(
            !win_selected(&mut program, w3),
            "w3 deselected after the click"
        );
    }

    // -- the settle_currency cascade (insert-time show()->resetCurrent) ---

    /// Downcast the program's desktop child to the concrete [`Desktop`] (the
    /// `as_any_mut` hatch) — the currency tests' route to `current_child` /
    /// `insert_view`.
    fn desktop_concrete(program: &mut Program) -> &mut Desktop {
        let id = program.desktop().expect("a desktop exists");
        program
            .group_mut()
            .find_mut(id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<Desktop>())
            .expect("the desktop child downcasts to Desktop")
    }

    /// NESTED-GAP BITE. Showing a selectable view resets its owning group's
    /// currency at EVERY level of the tree, so a ctor-built desktop holding a
    /// window that itself holds a selectable child has the full currency chain
    /// (desktop→window→child) before the first event. tvision-rs's ctx-less inserts
    /// defer that to `Program::new`'s eager `settle_currency` pass, which runs
    /// POST-ORDER (children first) so the window's INTERNAL currency exists before
    /// the desktop's focus cascade descends into it. An earlier version reset only
    /// the DESKTOP's currency: the window became current+focused but its own
    /// `current` stayed `None` — the child unfocused, typing lost (this test
    /// fails there on every assertion past the first).
    #[test]
    fn startup_settles_nested_preinserted_window_currency() {
        let (backend, _handle) = HeadlessBackend::new(80, 25);
        let theme = Theme::classic_blue();
        let clock = Rc::new(ManualClock::new(0));
        let log: Rc<RefCell<Vec<Event>>> = Rc::new(RefCell::new(Vec::new()));
        let ids: Rc<RefCell<Vec<crate::view::ViewId>>> = Rc::new(RefCell::new(Vec::new()));
        let (log_cap, ids_cap) = (log.clone(), ids.clone());
        let mut program = Program::new(
            Box::new(backend),
            Box::new(clock),
            theme,
            move |r| {
                let mut desktop = Desktop::new(r, |r2| Some(Desktop::init_background(r2)));
                let mut win = Window::new(Rect::new(4, 2, 40, 14), Some("W".into()), 1);
                // A selectable, event-logging child INSIDE the window's group.
                let child =
                    win.insert_child(Box::new(Probe::new(Rect::new(2, 2, 20, 6), 'P', log_cap)));
                let win_id = desktop.insert_view(Box::new(win));
                ids_cap.borrow_mut().extend([win_id, child]);
                Some(Box::new(desktop))
            },
            |_r| None,
            |_r| None,
        );
        let (win_id, child_id) = {
            let b = ids.borrow();
            (b[0], b[1])
        };

        // After Program::new ALONE (no pump): the whole chain is settled.
        assert_eq!(
            desktop_concrete(&mut program).current_child(),
            Some(win_id),
            "desktop current = the pre-inserted window"
        );
        let (sel, foc) = program
            .group_mut()
            .find_mut(child_id)
            .map(|v| (v.state().state.selected, v.state().state.focused))
            .expect("child resolves");
        assert!(
            sel,
            "window current = child (selected by the window's settled reset_current — \
             the formerly-latent nested gap)"
        );
        assert!(
            foc,
            "child focused (the focus cascade descended desktop→window→child)"
        );

        // A typed key reaches the child.
        program.out_events.clear(); // drop the startup focus broadcasts
        program.out_events.push_back(key(Key::Char('x')));
        program.pump_once();
        assert!(
            log.borrow()
                .iter()
                .any(|e| matches!(e, Event::KeyDown(k) if k.key == Key::Char('x'))),
            "a typed key routes desktop→window→child"
        );
    }

    /// SETTLE-BEFORE-DISPATCH. A plain ctx-less insert between pumps (the
    /// bare `Desktop::insert_view` seam — no focus_child, no reset_current
    /// anywhere) must be keyboard-live by the very next event: `pump_once`
    /// settles pending currency (step 2b) BEFORE the event pick, so the
    /// insert-time currency cascade completes before any subsequent event.
    #[test]
    fn plain_insert_between_pumps_routes_next_key_into_new_window() {
        let (mut program, _handle, _clock) = program_with_desktop(80, 25);
        program.pump_once(); // steady state
        program.out_events.clear();

        let log: Rc<RefCell<Vec<Event>>> = Rc::new(RefCell::new(Vec::new()));
        let mut win = Window::new(Rect::new(4, 2, 40, 14), Some("W".into()), 1);
        let _child = win.insert_child(Box::new(Probe::new(
            Rect::new(2, 2, 20, 6),
            'K',
            log.clone(),
        )));
        desktop_concrete(&mut program).insert_view(Box::new(win)); // plain insert

        // ONE pump: settle runs before the event pick → the key lands in the child.
        program.out_events.push_back(key(Key::Char('k')));
        program.pump_once();
        assert!(
            log.borrow()
                .iter()
                .any(|e| matches!(e, Event::KeyDown(k) if k.key == Key::Char('k'))),
            "the first key after a plain insert reaches the new window's child"
        );
    }

    /// HIDE/SHOW CURRENCY. Toggling the visibility of a *selectable* view resets
    /// its owning group's currency in BOTH directions (show and hide).
    /// `Deferred::SetVisible` routes through `set_visible_descendant`, which runs
    /// that reset in the OWNING group; a non-selectable child's visibility never
    /// moves currency.
    #[test]
    fn set_visible_deferred_moves_currency_both_directions() {
        let (mut program, _handle, _clock) = program_with_desktop(80, 25);
        let log: Rc<RefCell<Vec<Event>>> = Rc::new(RefCell::new(Vec::new()));
        let mut win = Window::new(Rect::new(4, 2, 40, 14), Some("W".into()), 1);
        // B below A (A topmost) so firstMatch lands on A; C is a non-selectable
        // topmost sibling.
        let b = win.insert_child(Box::new(Probe::new(
            Rect::new(2, 2, 10, 4),
            'B',
            log.clone(),
        )));
        let a = win.insert_child(Box::new(Probe::new(
            Rect::new(12, 2, 20, 4),
            'A',
            log.clone(),
        )));
        let mut c_probe = Probe::new(Rect::new(22, 2, 30, 4), 'C', log.clone());
        c_probe.st.options.selectable = false;
        let c = win.insert_child(Box::new(c_probe));
        desktop_concrete(&mut program).insert_view(Box::new(win));
        program.pump_once(); // settle: window current = A (topmost selectable)
        program.out_events.clear();

        let focused = |program: &mut Program, id| {
            program
                .group_mut()
                .find_mut(id)
                .map(|v| v.state().state.focused)
                .expect("probe resolves")
        };
        assert!(focused(&mut program, a), "A current+focused after settle");

        // Hide A: the owning group's reset_current snaps currency to B.
        // (The drain only runs after a dispatched event, so push a benign key.)
        program.deferred.push(Deferred::SetVisible(a, false));
        program.out_events.push_back(key(Key::Char('.')));
        program.pump_once();
        assert!(!focused(&mut program, a), "hidden A lost focus");
        assert!(focused(&mut program, b), "hide direction: current == B");

        // Show A: the reset RE-RAN (A is the topmost selectable again).
        program.deferred.push(Deferred::SetVisible(a, true));
        program.out_events.push_back(key(Key::Char('.')));
        program.pump_once();
        assert!(
            focused(&mut program, a),
            "show direction: reset re-ran, currency back on A"
        );
        assert!(!focused(&mut program, b), "B released focus");

        // Hiding the NON-selectable C does not move currency.
        program.deferred.push(Deferred::SetVisible(c, false));
        program.out_events.push_back(key(Key::Char('.')));
        program.pump_once();
        assert!(
            focused(&mut program, a),
            "hiding a non-selectable child leaves currency untouched"
        );
    }

    // -- cmNext cycles windows ----------------------------------------

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

    // -- cmPrev sends current to back ---------------------------------

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
        // Reset command_set_changed so the idle pump below only queues the timer
        // event (not a spurious cmCommandSetChanged broadcast that would consume
        // the routing pump slot and delay the timer delivery by one cycle).
        program.command_set_changed = false;
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

        // A click outside the modal view is delivered to the modal view (localized),
        // NOT to views beneath. The beneath probe must NOT see it.
        program.out_events.clear(); // drop the set_current focus broadcasts
        program.out_events.push_back(mouse_down_at(2, 2));
        program.pump_once();
        assert!(
            beneath_log.borrow().is_empty(),
            "outside click is NOT routed to views beneath the modal"
        );
        // The modal view itself receives the outside click (localized to its frame).
        assert_eq!(
            modal_log.borrow().len(),
            1,
            "outside click is delivered to the modal view (localized)"
        );

        // A click on the modal view also reaches it.
        program.out_events.push_back(mouse_down_at(12, 2));
        program.pump_once();
        assert_eq!(
            modal_log.borrow().len(),
            2,
            "inside click also reaches the modal view"
        );

        // end_modal surfaces the end state. NOTE: end_modal does NOT pop the frame
        // here — `CaptureStack` has no pop API; a handler self-pops only by
        // returning `ConsumedPop` (proven generically by
        // `capture::tests::consumed_pop_removes_handler`). The blocking wrapper
        // that pushes the frame, runs the pump until end_modal, then pops it is
        // `exec_view`, built on this frame. So the frame is still on the stack
        // after end_modal — the truthful state.
        assert_eq!(program.capture_len(), 1, "modal frame still present");
        program.end_modal(Command::OK);
        assert_eq!(program.end_state(), Some(Command::OK));
        assert_eq!(
            program.capture_len(),
            1,
            "end_modal does not pop the frame; exec_view owns push+pop"
        );
    }

    // -- 5b. Outside-modal redirect -------------------------------------------

    /// Verifies that when a ModalFrame is the top capture and a MouseDown lands
    /// outside the modal view's bounds, the event is delivered to the modal view
    /// (localized), not silently swallowed or routed to views beneath.
    #[test]
    fn outside_modal_click_delivered_to_modal_view() {
        let (mut program, _screen, _clock) = program_with_desktop(20, 10);
        let modal_log = Rc::new(RefCell::new(Vec::new()));

        // Modal probe occupies the right half of the screen.
        let modal_bounds = Rect::new(10, 0, 20, 10);
        let modal_id = {
            let modal = Probe::new(modal_bounds, 'M', modal_log.clone());
            program.group_mut().insert(Box::new(modal))
        };
        program.with_ctx(|g, ctx| g.set_current(Some(modal_id), SelectMode::Normal, ctx));
        program
            .captures
            .push(Box::new(ModalFrame::new(modal_id, modal_bounds)));

        // A click at (2, 2) is outside modal_bounds (x=10..20). It must be
        // delivered to the modal view with a localized position, not swallowed.
        program.out_events.clear();
        program.out_events.push_back(mouse_down_at(2, 2));
        program.pump_once();

        assert_eq!(
            modal_log.borrow().len(),
            1,
            "outside click must be delivered to the modal view"
        );
        // Verify the localized position: (2,2) - modal_bounds.a=(10,0) = (-8, 2).
        if let Event::MouseDown(m) = modal_log.borrow()[0] {
            assert_eq!(
                m.position,
                Point::new(-8, 2),
                "position localized to modal frame"
            );
        } else {
            panic!("expected MouseDown in modal_log");
        }
    }

    /// Verifies that a MouseDown INSIDE the modal bounds still goes through
    /// normal dispatch (not the outside-modal redirect).
    #[test]
    fn inside_modal_click_uses_normal_dispatch() {
        let (mut program, _screen, _clock) = program_with_desktop(20, 10);
        let modal_log = Rc::new(RefCell::new(Vec::new()));

        let modal_bounds = Rect::new(10, 0, 20, 10);
        let modal_id = {
            let modal = Probe::new(modal_bounds, 'M', modal_log.clone());
            program.group_mut().insert(Box::new(modal))
        };
        program.with_ctx(|g, ctx| g.set_current(Some(modal_id), SelectMode::Normal, ctx));
        program
            .captures
            .push(Box::new(ModalFrame::new(modal_id, modal_bounds)));

        // A click at (12, 2) is INSIDE modal_bounds (x=10..20). It reaches the
        // modal view via normal dispatch (ModalFrame passes it through).
        program.out_events.clear();
        program.out_events.push_back(mouse_down_at(12, 2));
        program.pump_once();

        assert_eq!(
            modal_log.borrow().len(),
            1,
            "inside click reaches modal view via normal dispatch"
        );
        // Normal dispatch: position localized to modal frame by Group::deliver.
        // modal_bounds.a = (10, 0), so (12, 2) -> (2, 2).
        if let Event::MouseDown(m) = modal_log.borrow()[0] {
            assert_eq!(
                m.position,
                Point::new(2, 2),
                "position localized by group deliver"
            );
        } else {
            panic!("expected MouseDown in modal_log");
        }
    }

    /// A plain modal (Probe, simulating a Dialog without click-outside-to-cancel
    /// logic) must NOT cancel when an outside click is delivered. Only the
    /// `HistoryWindow` popup cancels on a click outside its bounds; a plain
    /// `Dialog` does not.
    #[test]
    fn plain_dialog_modal_ignores_outside_click() {
        let (mut program, _screen, _clock) = program_with_desktop(20, 10);
        let modal_log = Rc::new(RefCell::new(Vec::new()));

        let modal_bounds = Rect::new(5, 2, 15, 8);
        let modal_id = {
            let modal = Probe::new(modal_bounds, 'D', modal_log.clone());
            program.group_mut().insert(Box::new(modal))
        };
        program.with_ctx(|g, ctx| g.set_current(Some(modal_id), SelectMode::Normal, ctx));
        program
            .captures
            .push(Box::new(ModalFrame::new(modal_id, modal_bounds)));

        // Click at (1, 1) is outside modal_bounds (x=5..15, y=2..8).
        program.out_events.clear();
        program.out_events.push_back(mouse_down_at(1, 1));
        program.pump_once();

        // The modal view receives the outside click (delivery path confirmed).
        assert_eq!(
            modal_log.borrow().len(),
            1,
            "outside click delivered to modal"
        );
        // No endModal was posted — end_state stays None (the plain Probe does
        // not call ctx.end_modal, so no modal close).
        assert!(
            program.end_state().is_none(),
            "plain Dialog must not cancel on outside click"
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

        // cmZoom starts DISABLED (in the initial_disabled_commands seed).
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

    // -- 9b. denylist command enablement (the allowlist → denylist flip) --

    /// (a) An arbitrary app-minted command is enabled by default — the heart of
    /// the denylist: no registration, no allowlist to extend.
    #[test]
    fn custom_command_enabled_by_default() {
        let (program, _screen, _clock) = program_with_desktop(12, 4);
        assert!(
            program.command_enabled(Command::custom("x.y")),
            "an unregistered custom command is enabled by default (denylist)"
        );
        // And the framework vocabulary that the old allowlist enumerated is
        // enabled the same way — by NOT being in the disabled seed.
        assert!(program.command_enabled(Command::OK));
        assert!(program.command_enabled(Command::QUIT));
    }

    /// (b) Exactly the five window-management commands C++'s `initCommands`
    /// disables start disabled.
    #[test]
    fn window_commands_start_disabled() {
        let (program, _screen, _clock) = program_with_desktop(12, 4);
        for cmd in [
            Command::ZOOM,
            Command::CLOSE,
            Command::RESIZE,
            Command::NEXT,
            Command::PREV,
        ] {
            assert!(
                !program.command_enabled(cmd),
                "{cmd:?} starts disabled (initCommands seed)"
            );
        }
    }

    /// (c) disable→enable round-trips toggle `command_enabled`, and the changed
    /// flag fires on REAL transitions only (faithful to the C++ `has`-guarded
    /// `commandSetChanged` updates).
    #[test]
    fn enable_disable_round_trip_sets_changed_flag_on_real_transitions_only() {
        let (mut program, _screen, _clock) = program_with_desktop(12, 4);
        let cmd = Command::custom("test.round_trip");

        // Enabled by default; enabling again is NOT a transition.
        assert!(program.command_enabled(cmd));
        program.command_set_changed = false;
        program.enable_command(cmd);
        assert!(
            !program.command_set_changed,
            "enabling an already-enabled command is not a change"
        );

        // disable: a real transition.
        program.disable_command(cmd);
        assert!(!program.command_enabled(cmd));
        assert!(program.command_set_changed, "real disable flips the flag");

        // disable again: NOT a transition.
        program.command_set_changed = false;
        program.disable_command(cmd);
        assert!(
            !program.command_set_changed,
            "disabling an already-disabled command is not a change"
        );

        // enable: a real transition back.
        program.enable_command(cmd);
        assert!(program.command_enabled(cmd));
        assert!(program.command_set_changed, "real enable flips the flag");
    }

    /// (d) An `Event::Command` carrying an unregistered custom command passes the
    /// pump's boundary filter and reaches routing — the symptom-level proof the
    /// allowlist's silent drop (the "OK does nothing" class of bug) is gone.
    #[test]
    fn custom_command_passes_pump_filter_without_registration() {
        let (mut program, _screen, _clock) = program_with_desktop(12, 4);
        let log = Rc::new(RefCell::new(Vec::new()));
        let cmd = Command::custom("test.unregistered");

        let probe = Probe::new(Rect::new(0, 0, 4, 2), 'P', log.clone());
        let id = program.group_mut().insert(Box::new(probe));
        program.with_ctx(|g, ctx| g.set_current(Some(id), SelectMode::Normal, ctx));

        program.out_events.clear();
        program.out_events.push_back(Event::Command(cmd));
        program.pump_once();
        assert!(
            log.borrow().contains(&Event::Command(cmd)),
            "an unregistered custom command flows through the filter to routing"
        );

        // And the inverse: once explicitly disabled, the SAME command is dropped
        // at the boundary (the filter still bites — denylist, not no-list).
        program.disable_command(cmd);
        log.borrow_mut().clear();
        program.out_events.clear();
        program.out_events.push_back(Event::Command(cmd));
        program.pump_once();
        assert!(
            !log.borrow().contains(&Event::Command(cmd)),
            "an explicitly disabled command is dropped at the boundary"
        );
    }

    /// (e) `Context::command_enabled` (the snapshot query) reflects
    /// the program's set: a `ctx.disable_command` deferred in pump N is visible
    /// to the Context snapshot in pump N+1 (snapshot semantics).
    #[test]
    fn ctx_command_enabled_reflects_set_after_deferred_apply() {
        let (mut program, _screen, _clock) = program_with_desktop(12, 4);
        let log = Rc::new(RefCell::new(Vec::new()));
        let cmd = Command::custom("test.ctx_snapshot");

        // The probe records what its Context's snapshot says about `cmd` on
        // every event, and requests the disable on the first.
        let seen: Rc<RefCell<Vec<bool>>> = Rc::new(RefCell::new(Vec::new()));
        {
            let seen = seen.clone();
            let mut disabled_requested = false;
            let mut probe = Probe::new(Rect::new(0, 0, 4, 2), 'P', log.clone());
            probe.action = Some(Box::new(move |ctx: &mut Context| {
                seen.borrow_mut().push(ctx.command_enabled(cmd));
                if !disabled_requested {
                    ctx.disable_command(cmd);
                    disabled_requested = true;
                }
            }));
            let id = program.group_mut().insert(Box::new(probe));
            program.with_ctx(|g, ctx| g.set_current(Some(id), SelectMode::Normal, ctx));
        }

        // Pump 1: the snapshot still says enabled (taken before the deferred
        // apply); the disable is applied after dispatch.
        program.out_events.clear();
        program.out_events.push_back(key(Key::Char('a')));
        program.pump_once();
        assert_eq!(
            seen.borrow().as_slice(),
            &[true],
            "pump 1 snapshot: still enabled (disable is deferred)"
        );
        assert!(
            !program.command_enabled(cmd),
            "after the deferred apply the program set has it disabled"
        );

        // Pump 2: the refreshed snapshot reflects the applied disable.
        program.out_events.push_back(key(Key::Char('b')));
        program.pump_once();
        assert_eq!(
            seen.borrow().as_slice(),
            &[true, false],
            "pump 2 snapshot: the Context sees the disabled command"
        );
    }

    // -- 10. drag move round-trip -------------------------

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

    // -- 13. exec_view modal round-trips -----------

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

    /// Integration SMOKE test for the initial-currency seam: `exec_view` on a plain
    /// `Dialog` holding a selectable `Button`, driven by Esc as the first event, runs
    /// to completion (returns cmCancel, frame popped) with NO hang. This exercises
    /// the `find_mut(id) -> reset_current` call wired into `exec_view`.
    ///
    /// NOTE — not the discriminating guard. A `Dialog` converts Esc into a cancel
    /// command at the *dialog* level, independent of internal currency, so this test
    /// passes even with the seam reverted (verified). The seam itself is guarded by
    /// `group::tests::reset_current_via_trait_sets_current_to_first_selectable`, which
    /// asserts the trait dispatch flips `current` from None to the first selectable
    /// child. This test confirms the wiring path is sound and hang-free end-to-end.
    #[test]
    fn plain_dialog_keyboard_live_on_first_event_esc_cancels() {
        use crate::widgets::{Button, ButtonFlags};

        let (mut program, _screen, _clock) = program_with_desktop(40, 12);

        let mut dialog = Dialog::new(Rect::new(4, 2, 36, 10), Some("Setup".into()));
        // A single selectable child. With reset_current at open this becomes the
        // dialog's `current`, making the dialog keyboard-live immediately.
        dialog.insert_child(Box::new(Button::new(
            Rect::new(2, 5, 12, 7),
            "OK",
            Command::OK,
            ButtonFlags::new(),
        )));

        // Esc as the FIRST event, no prior nav: pump 1 routes it to the focused
        // child / dialog -> posts cmCancel; a later pump routes cmCancel ->
        // end_modal(Cancel) -> exits. Before the seam this hung.
        program.out_events.push_back(key(Key::Esc));

        let result = program.exec_view(Box::new(dialog));

        assert_eq!(
            result,
            Command::CANCEL,
            "Esc on a fresh dialog with a selectable child ends the modal with \
             cmCancel — proves the dialog is keyboard-live on the first event"
        );
        assert_eq!(program.capture_len(), 0, "ModalFrame popped");
    }

    /// Phase signal, end-to-end leg 1: a FOCUSED view that CONSUMES the
    /// letter starves the post-process accelerator. Dialog = InputLine
    /// (current, eats letters) + a "~K~ick" button (ofPostProcess). Typing 'k'
    /// lands in the input line (proof: its text becomes "k"), so the post-loop
    /// never sees a live event and the button must NOT press — exactly the
    /// C++ contract (the focused leg runs before phPostProcess and a cleared
    /// event stops the walk).
    #[test]
    fn plain_hotkey_consumed_by_focused_field_starves_post_process() {
        use crate::data::FieldValue;
        use crate::widgets::{Button, ButtonFlags, InputLine, LimitMode};

        let (mut program, _screen, _clock) = program_with_desktop(40, 12);

        let mut dialog = Dialog::new(Rect::new(2, 1, 38, 11), Some("D".into()));
        let btn_id = dialog.insert_child(Box::new(Button::new(
            Rect::new(2, 4, 12, 6),
            "~K~ick",
            Command::custom("test.kick"),
            ButtonFlags::new(),
        )));
        // Inserted LAST → topmost, so the insert-time reset_current (C++
        // firstMatch = topmost visible+selectable) makes it the dialog's
        // current (focused) child.
        let il_id = dialog.insert_child(Box::new(InputLine::new(
            Rect::new(2, 2, 20, 3),
            40,
            None,
            LimitMode::MaxBytes,
        )));
        program
            .desktop_insert(Box::new(dialog))
            .expect("dialog inserted into the desktop");
        // Drop the insert/focus broadcasts so the next pumped event IS the key
        // (pump_once processes exactly one queued event per call).
        program.out_events.clear();

        // Type the button's hot letter — the focused input line eats it.
        program.out_events.push_back(key(Key::Char('k')));
        program.pump_once();

        // Proof the key was routed: the input line holds the character.
        let il_value = program
            .group_mut()
            .find_mut(il_id)
            .and_then(|v| v.value())
            .expect("input line found with a value");
        assert_eq!(
            il_value,
            FieldValue::Text("k".into()),
            "the focused input line consumed the letter"
        );

        let btn = program
            .group_mut()
            .find_mut(btn_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<Button>())
            .expect("button found");
        assert!(!btn.down, "consumed letter must NOT press the button");
        assert!(btn.animation_timer.is_none(), "no press animation armed");
    }

    /// Phase signal, end-to-end leg 2: when the FOCUSED view does NOT
    /// consume the letter, the post-process walk delivers it and the plain
    /// hotkey presses the (unfocused) button. Dialog = two buttons: "~K~ick"
    /// and "~M~ore" (inserted last → topmost → current; ignores a plain 'k'
    /// on its Focused leg). Typing 'k' falls through to phPostProcess where
    /// the unfocused "~K~ick" arms its press.
    #[test]
    fn plain_hotkey_presses_unfocused_button_via_post_process() {
        use crate::widgets::{Button, ButtonFlags};

        let (mut program, _screen, _clock) = program_with_desktop(40, 12);

        let mut dialog = Dialog::new(Rect::new(2, 1, 38, 11), Some("D".into()));
        let kick_id = dialog.insert_child(Box::new(Button::new(
            Rect::new(2, 5, 12, 7),
            "~K~ick",
            Command::custom("test.kick"),
            ButtonFlags::new(),
        )));
        // Inserted LAST → topmost → the dialog's current (focused) child.
        let more_id = dialog.insert_child(Box::new(Button::new(
            Rect::new(2, 2, 12, 4),
            "~M~ore",
            Command::custom("test.more"),
            ButtonFlags::new(),
        )));
        program
            .desktop_insert(Box::new(dialog))
            .expect("dialog inserted into the desktop");
        // Drop the insert/focus broadcasts (one pumped event per pump_once).
        program.out_events.clear();

        program.out_events.push_back(key(Key::Char('k')));
        program.pump_once();

        {
            let kick = program
                .group_mut()
                .find_mut(kick_id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<Button>())
                .expect("kick button found");
            assert!(kick.down, "the postProcess plain hotkey pressed '~K~ick'");
            assert!(kick.animation_timer.is_some(), "press animation armed");
        }
        let more = program
            .group_mut()
            .find_mut(more_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<Button>())
            .expect("more button found");
        assert!(!more.down, "the focused '~M~ore' button ignored the letter");
    }

    /// `Command::QUIT` during a modal (the non-obvious edge). Inside the modal,
    /// `Event::Command(Command::QUIT)` is caught by `program_handle_event` ->
    /// `end_state = Some(QUIT)`. The inner loop exits, `valid_end(QUIT)` ->
    /// `group.valid(QUIT)` -> true (the dialog's `valid` defers to the group, no
    /// child vetoes QUIT), so `exec_view` returns `QUIT` and pops the frame.
    ///
    /// **This asserts a DELIBERATE DEVIATION (D9).** Under the single loop,
    /// `program_handle_event` (the quit catch) runs during the modal pump, so a
    /// quit command ends the modal with `QUIT` and quits the app. The original's
    /// modal dispatch never reached its program-level quit catch, so there the
    /// command would have been discarded and the modal would stay open. We keep our
    /// behavior (see `exec_view`'s doc); the assertions below verify it (no hang,
    /// no panic, frame popped).
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
        fn valid(&mut self, cmd: Command, _ctx: &mut Context) -> bool {
            cmd != self.veto
        }
    }

    /// DISCRIMINATING (Fix 1): `exec_view`'s outer `while(!valid)` validates the
    /// MODAL view's own `valid` (a dialog's `valid`, scoped to the dialog's
    /// children) — NOT the root group's `valid` (which would also consult the
    /// desktop's siblings). We insert a sibling into the ROOT group whose `valid`
    /// vetoes
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

    /// CROSS-ROW (the reviewer's gap): the **headline** behavior of input-line
    /// validation end-to-end — a modal dialog must **not** close on OK while a
    /// child input line's validator rejects, but must close on Cancel.
    ///
    /// Isolated tests only call `InputLine::valid()` directly; the actual veto
    /// lives in `exec_view`'s outer `while !valid(end_state)` loop. The trace this
    /// proves:
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

    /// A `sfModal` window posts `cmCancel` on `cmClose` and is NOT removed
    /// (`exec_view` owns the actual modal teardown).
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

    // -- TScroller cross-view broker (pump-side apply) ----------------
    //
    // These drive the broker end-to-end through `pump_once`: the scroller and its
    // two bars are inserted into the ROOT group (so the pump's `group.find_mut`
    // resolves all three), and the deferred `ScrollSync` /
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
        // ScrollSync), then the apply loop reads the bars and pushes the
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

    // -- PageStack↔TabBar read-sync broker (pump-side apply) ---------
    //
    // Model: same as scroller_read_broker_syncs_delta_through_pump.
    // A TabBar and a PageStack (2 pages) are inserted into the program's
    // root group. Injecting a TAB_BAR_CHANGED broadcast sourced by the
    // TabBar makes the pump read its value and call set_active on the
    // PageStack. A broadcast from a non-bound source is ignored.

    use crate::widgets::{PageStack, TabBar};

    fn insert_tab_bar_page_stack(program: &mut Program) -> (ViewId, ViewId, ViewId, ViewId) {
        let g = program.group_mut();
        let tab_id = g.insert(Box::new(TabBar::new(Rect::new(0, 0, 30, 1), &["A", "B"])));
        // Two page stubs at the full 30×9 content area.
        let page_rect = Rect::new(0, 1, 30, 10);
        let mut ps = PageStack::new(page_rect);
        let p0 = ps.insert_page(Box::new(crate::widgets::StaticText::new(
            page_rect, "page0",
        )));
        let p1 = ps.insert_page(Box::new(crate::widgets::StaticText::new(
            page_rect, "page1",
        )));
        ps.bind_tab_bar(tab_id);
        let ps_id = g.insert(Box::new(ps));
        program.out_events.clear();
        (tab_id, ps_id, p0, p1)
    }

    fn page_stack_active(program: &mut Program, ps_id: ViewId) -> usize {
        program
            .group_mut()
            .find_mut(ps_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<PageStack>())
            .map(|ps| ps.active())
            .expect("page stack resolves")
    }

    fn page_visible(program: &mut Program, ps_id: ViewId, page_id: ViewId) -> bool {
        program
            .group_mut()
            .find_mut(ps_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<PageStack>())
            .map(|ps| ps.page_visible(page_id))
            .expect("page stack resolves")
    }

    fn set_tab_bar_value(program: &mut Program, tab_id: ViewId, value: usize) {
        // Set directly to avoid the broadcast loop (test setup only).
        use crate::data::FieldValue;
        let g = program.group_mut();
        let tb = g.find_mut(tab_id).expect("tab bar resolves");
        tb.set_value(FieldValue::Int(value as i32));
    }

    /// Read broker: a `TAB_BAR_CHANGED` broadcast whose `source` is the bound
    /// TabBar makes the pump read that bar's `value` and call `set_active` on
    /// the PageStack. A broadcast from a non-bound source is ignored.
    #[test]
    fn page_stack_read_broker_switches_active_page_through_pump() {
        let (mut program, _h2, _c) = program_with_desktop(80, 25);
        let (tab_id, ps_id, p0, p1) = insert_tab_bar_page_stack(&mut program);

        // Initial state: tab bar at index 0, page 0 visible.
        assert_eq!(page_stack_active(&mut program, ps_id), 0);
        assert!(page_visible(&mut program, ps_id, p0));
        assert!(!page_visible(&mut program, ps_id, p1));

        // Pre-set the TabBar's value to index 1 (the broker will read this).
        set_tab_bar_value(&mut program, tab_id, 1);

        // Inject TAB_BAR_CHANGED broadcast sourced by the TabBar and pump once:
        // the broadcast phase delivers it to the PageStack (which queues
        // PageStackSync), then the apply loop reads the TabBar's value (1) and
        // calls set_active(1) on the PageStack.
        program.out_events.push_back(Event::Broadcast {
            command: Command::TAB_BAR_CHANGED,
            source: Some(tab_id),
        });
        program.pump_once();

        assert_eq!(
            page_stack_active(&mut program, ps_id),
            1,
            "active index must switch to 1 after TAB_BAR_CHANGED from the bound tab bar"
        );
        assert!(
            !page_visible(&mut program, ps_id, p0),
            "page 0 must be hidden after switching to page 1"
        );
        assert!(
            page_visible(&mut program, ps_id, p1),
            "page 1 must be visible after the switch"
        );

        // Negative case: a TAB_BAR_CHANGED broadcast from an unbound source must
        // NOT switch the active page.
        set_tab_bar_value(&mut program, tab_id, 0);
        let other_id = ViewId::next();
        program.out_events.push_back(Event::Broadcast {
            command: Command::TAB_BAR_CHANGED,
            source: Some(other_id),
        });
        program.pump_once();
        assert_eq!(
            page_stack_active(&mut program, ps_id),
            1,
            "broadcast from an unbound source must leave the active page unchanged"
        );
    }

    /// End-to-end editor scroll wiring through a real `EditWindow` in the desktop:
    ///
    /// 1. Cursor moves update the **indicator** (the new position is pushed via
    ///    the `IndicatorSetValue` broker — regresses the missing `Indicator::as_any_mut`
    ///    that left the indicator frozen at "1:1").
    /// 2. Dragging the **vertical scrollbar** scrolls the editor's `delta`
    ///    (the scrollbar-changed broadcast from the dragged bar must reach the editor
    ///    through its own `MouseTrackCapture` hold — regresses the swallowed broadcast
    ///    that left scrollbar-drag dead).
    #[test]
    fn editwindow_indicator_updates_and_scrollbar_drag_scrolls() {
        use crate::widgets::{EditWindow, Indicator, editor_mut};

        let (mut program, _h, _c) = program_with_desktop(100, 30);

        // A 40-line buffer (taller than the ~27-row editor viewport, so there is
        // real vertical scroll range).
        let mut text = String::new();
        for i in 1..=40 {
            text.push_str(&format!("line {i:02} content\n"));
        }
        let path = std::env::temp_dir().join("rstv_editwindow_scroll_test.txt");
        std::fs::write(&path, &text).unwrap();

        let r = program.desktop_rect();
        let win = EditWindow::new(r, Some(path.clone()), 1);
        let editor_id = win.editor_id;
        let ind_id = win.indicator_id;
        let vbar_id = win.v_scroll_bar_id;
        program.desktop_insert(Box::new(win));
        for _ in 0..3 {
            program.pump_once();
        }

        let editor_delta = |program: &mut Program| {
            program
                .group_mut()
                .find_mut(editor_id)
                .and_then(editor_mut)
                .map(|e| e.delta())
                .expect("editor resolves")
        };
        let indicator_loc = |program: &mut Program| {
            program
                .group_mut()
                .find_mut(ind_id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<Indicator>())
                .map(|i| i.location)
                .expect("indicator resolves + downcasts")
        };

        // -- (1) Cursor moves update the indicator. --------------------------
        // Before the fix the indicator was frozen at (0,0) ("1:1") because
        // `Indicator` lacked `as_any_mut`, so the `IndicatorSetValue` broker's
        // downcast always yielded `None`. It must now track the cursor row.
        assert_eq!(
            indicator_loc(&mut program),
            Point::new(0, 0),
            "starts at 1:1"
        );
        for _ in 0..5 {
            program
                .out_events
                .push_back(Event::KeyDown(KeyEvent::from(Key::Down)));
            program.pump_once();
        }
        let loc = indicator_loc(&mut program);
        assert!(
            loc.y > 0,
            "after several Down keys the indicator must track the cursor row, not stay frozen at 1:1 (got {loc:?})"
        );
        // And it must match the editor's own cursor row.
        let cur_row = program
            .group_mut()
            .find_mut(editor_id)
            .and_then(editor_mut)
            .map(|e| e.cur_pos())
            .expect("editor resolves");
        assert_eq!(
            loc, cur_row,
            "indicator location mirrors the editor cursor position"
        );

        // -- (2) Dragging the v-bar scrolls the editor. ----------------------
        assert_eq!(editor_delta(&mut program).y, 0, "not scrolled yet");
        let vb = program
            .group_mut()
            .descendant_global_bounds(vbar_id, Point::new(0, 0))
            .expect("v-bar absolute bounds");
        // Click low in the trough (just above the down-arrow) → thumb-jump down.
        let cx = vb.a.x;
        let cy = vb.b.y - 2;
        program.out_events.push_back(Event::MouseDown(MouseEvent {
            position: Point::new(cx, cy),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        }));
        for _ in 0..4 {
            program.pump_once();
        }
        assert!(
            editor_delta(&mut program).y > 0,
            "dragging the scrollbar must scroll the editor (delta.y advanced past 0)"
        );
        // Release the hold.
        program.out_events.push_back(Event::MouseUp(MouseEvent {
            position: Point::new(cx, cy),
            ..Default::default()
        }));
        program.pump_once();

        let _ = std::fs::remove_file(&path);
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

    // -- TListViewer read-sync broker + the TERMINATION property -------
    //
    // The list-viewer read-sync WRITES BACK (focus_item_num -> focusItem -> a
    // deferred v-bar setValue(focused)), unlike the scroller. The cycle
    // (cmScrollBarChanged -> ScrollSync -> apply_scroll -> setValue ->
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
        fn apply_scroll_sync(&mut self, h: Option<i32>, v: Option<i32>, ctx: &mut Context) {
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
    /// one and then assert nothing of OURS (SCROLL_BAR_CHANGED / ScrollSync)
    /// re-appears: that is the cycle being QUIET.
    fn noop_broadcast() -> Event {
        Event::Broadcast {
            command: Command::custom("test.noop"),
            source: None,
        }
    }

    /// THE TERMINATION TEST: moving the v-bar to a new value and firing a
    /// scrollbar-changed broadcast drives the read-sync that WRITES BACK the
    /// focused index onto the bar. Because the write-back equals the bar's
    /// now-current value, `set_params`'s change-guard suppresses the re-broadcast,
    /// so the cycle goes QUIET.
    ///
    /// Each subsequent pump is driven by a benign broadcast (the
    /// deferred-apply loop only runs on an event-dispatch — a deferred write-back
    /// is applied by the *next* dispatch, exactly as in production). We assert that
    /// across many such dispatches NO `SCROLL_BAR_CHANGED` is ever produced by the
    /// write-back and NO `ScrollSync` is re-queued, while focused/top_item
    /// settle to the v-bar's value.
    ///
    /// Bite-check: were `ScrollBar::set_params` NOT change-guarded, applying the
    /// write-back `setValue(8)` would re-broadcast SCROLL_BAR_CHANGED (even with an
    /// unchanged value), the broadcast phase would re-queue ScrollSync, whose
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

        // Pump #1: broadcast phase delivers CHANGED -> list queues ScrollSync;
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
            // no SCROLL_BAR_CHANGED re-broadcast, no ScrollSync re-queue.
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
                    .any(|d| matches!(d, Deferred::ScrollSync { .. })),
                "pass {pass}: no ScrollSync re-queued (cycle terminated)"
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
        // Quiet now: more dispatches produce no further ScrollSync.
        for pass in 0..4 {
            program.out_events.push_back(noop_broadcast());
            program.pump_once();
            assert!(
                !program
                    .deferred
                    .iter()
                    .any(|d| matches!(d, Deferred::ScrollSync { .. })),
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
                .any(|d| matches!(d, Deferred::ScrollSync { .. })),
            "foreign-source broadcast ignored (source filter bites)"
        );
    }

    // -- TMenuView command-graying broker end-to-end -----------------

    /// A concrete, test-only menu view (the FakeList precedent: a *real*
    /// consumer of the broker, not a dead stub). It embeds [`MenuViewState`] and
    /// wires `handle_event` + `update_menu_commands` to the shared menu free
    /// functions, exactly as the real menu views do. `as_any_mut` lets the
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
    /// **Discriminating** (per the brief): under the denylist a custom command
    /// starts ENABLED, so we first DISABLE it (a real transition) and prove the
    /// item regrays to *disabled*, then ENABLE it back (another real transition)
    /// and prove it regrays to *enabled* — the second leg cannot pass from the
    /// item merely starting enabled. Both legs pass ONLY via the broadcast →
    /// request → regray path; remove the broker arm (or the request) and the
    /// item never flips.
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

        // A custom command is enabled by default (denylist).
        assert!(program.command_enabled(cmd));

        // 1. DISABLE the command → idle pump broadcasts cmCommandSetChanged →
        //    next pump delivers it → probe requests UpdateMenu → apply regrays.
        program.disable_command(cmd);
        program.pump_once(); // idle: emits the broadcast, clears the flag
        program.pump_once(); // delivers the broadcast → probe requests UpdateMenu
        program.pump_once(); // applies UpdateMenu (any residual)
        assert!(
            probe_disabled(&mut program, probe_id),
            "after DISABLE + regray the item must be DISABLED (disabled == true)"
        );

        // 2. ENABLE the command back → same path → the item regrays to enabled.
        program.enable_command(cmd);
        program.pump_once(); // idle: emits the broadcast
        program.pump_once(); // delivers it → probe requests UpdateMenu
        program.pump_once(); // applies UpdateMenu
        assert!(
            !probe_disabled(&mut program, probe_id),
            "after ENABLE + regray the item must be ENABLED (disabled == false)"
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

    // -- the TMenuView modal layer (MenuSession), end-to-end -------

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
    /// BITE: a next-item move that does not advance (or wraps wrong) leaves
    /// `current` at 0. Asserting exactly 1 pins the move.
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

    /// ONE Esc from a FIRST-level dropdown closes the WHOLE menu (box + session)
    /// without posting. At a first-level box the Esc is NOT consumed (its parent is
    /// the bar), so it is re-applied up to the bar; the bar (the top level) then
    /// consumes it and closes the menu.
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

    /// ONE Esc from a SECOND-level box closes ONLY that inner box; the session
    /// and the first-level box stay open. The asymmetry: a second-level box's
    /// parent is another box (not the bar), so the Esc is consumed there and does
    /// not propagate up. This pins the behavior against
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

    /// A hot-key accelerator (F3 = Open) pressed while a dropdown is OPEN closes
    /// the WHOLE menu and posts the command at ANY depth — the chosen command
    /// propagates up through every open menu level, closing them all. Open File's
    /// box, then press F3; the Open command must post AND the session must end.
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

    // -- the MenuSession MOUSE arms ----------------
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

    // -- TMenuPopup (popup_menu) ---------------------------------------
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

    // -- TMenuPopup with a SUBMENU level (multi-level exit-click) -------
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

    // -- real menu bar + status line wired into Program --------------
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
            //
            // Adapted for the press-and-hold seam (post-on-release):
            //   pump 1 (MouseDown): pre-route arms tracking + PushCapture applied;
            //                       ev cleared, no command yet.
            //   pump 2 (MouseUp):   MouseTrackCapture (top of stack) forwards the
            //                       localized MouseUp to the status line; its MouseUp
            //                       arm fires cmQuit and clears. The capture pops.
            //   pump 3:             the posted cmQuit routes to the cmQuit catch.
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
            program.pump_once(); // pre-route delivers MouseDown: arms tracking, no command yet
            program.out_events.push_back(mouse_up_at(2, 9));
            program.pump_once(); // MouseTrackCapture delivers MouseUp -> status line fires cmQuit
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
            // cmZoom is in initial_disabled_commands (a startup-disabled window
            // command). After Program::new, the status line's cached disabled set
            // must already reflect that (seeded directly in the ctor — no pump),
            // and the menu has no cmZoom item but the bar's Window>Next (cmNext,
            // also startup-disabled) must be greyed.
            let (mut program, _handle, sl, mb) = program_full(40, 10);

            // Status line: disabled_cmds cached immediately — cmZoom in it
            // (disabled), cmQuit not (enabled).
            let cs = program
                .group_mut()
                .find_mut(sl)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_ref::<StatusLine>())
                .expect("status line resolves")
                .disabled_cmds()
                .cloned();
            let cs =
                cs.expect("initial regray seeded the status-line disabled-set cache (no pump)");
            assert!(!cs.has(Command::QUIT), "cmQuit enabled at startup");
            assert!(
                cs.has(Command::ZOOM),
                "cmZoom is a startup-disabled command -> in the disabled cache"
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
        /// `StatusLine` (never seeded) reports `disabled_cmds() == None` and treats
        /// everything as enabled — the gap the ctor closes.
        #[test]
        fn bite_unseeded_status_line_is_all_enabled() {
            let line = StatusLine::new(Rect::new(0, 0, 40, 1), demo_status());
            assert!(
                line.disabled_cmds().is_none(),
                "an unseeded line has no cache (the startup gap Program::new closes by seeding)"
            );
        }

        // -- 7. OneOf status line switches def when a modal's help ctx matches --

        /// A status line with a `OneOf([specific])` def first, then an `All` def
        /// second — the same layout a "Find" dialog would use to show different
        /// status items while the dialog is open.
        fn oneof_status(ctx: crate::help::HelpCtx) -> Vec<StatusDef> {
            use crate::event::Key;
            StatusDef::list()
                .def_one_of(vec![ctx], |d| {
                    d.item("~F3~ Find", KeyEvent::from(Key::F(3)), Command::FIND)
                })
                .def_all(|d| d.item("~F1~ Help", KeyEvent::from(Key::F(1)), Command::HELP))
                .build()
        }

        /// Program factory that uses `oneof_status` instead of the default one.
        fn program_oneof(
            w: u16,
            h: u16,
            ctx: crate::help::HelpCtx,
        ) -> (Program, HeadlessHandle, ViewId) {
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
                    Some(Box::new(StatusLine::new(r, oneof_status(ctx))))
                },
                |_r| None, // no menu bar
            );
            let mut program = program;
            program.out_events.clear();
            let sl = program.status_line().expect("status line created");
            (program, handle, sl)
        }

        #[test]
        fn status_line_switches_def_for_modal_help_ctx() {
            // A status line with two defs:
            // - OneOf([HelpCtx::custom("app.find")]) → item: "~F3~ Find"
            // - All → item: "~F1~ Help"
            //
            // Baseline (no modal): idle arm calls set_help_ctx(NO_CONTEXT) → All
            // def selected (index 1 = second def).
            //
            // After pushing a ModalFrame whose view has helpCtx = "app.find":
            // the idle arm reads the modal's get_help_ctx() and calls
            // set_help_ctx("app.find") → OneOf def selected (index 0 = first def).
            let find_ctx = crate::help::HelpCtx::custom("app.find");
            let (mut program, _handle, sl) = program_oneof(40, 10, find_ctx);

            // Pump with no modal — idle arm sets NO_CONTEXT → All def (index 1).
            program.pump_once();
            let def_no_modal = program
                .group_mut()
                .find_mut(sl)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_ref::<StatusLine>())
                .expect("status line found")
                .selected_def();
            assert_eq!(
                def_no_modal,
                Some(1),
                "no modal → NO_CONTEXT → All def is index 1"
            );

            // Push a ModalFrame whose associated ViewId has helpCtx = "app.find".
            // We insert a view with that help_ctx into the group, then push the
            // ModalFrame pointing at it, so find_mut succeeds and returns the ctx.
            use crate::view::Rect;
            let bounds = Rect::new(5, 3, 35, 8);

            // Build a minimal view with the matching help_ctx.
            struct HelpView {
                st: crate::view::ViewState,
            }
            impl crate::view::View for HelpView {
                fn state(&self) -> &crate::view::ViewState {
                    &self.st
                }
                fn state_mut(&mut self) -> &mut crate::view::ViewState {
                    &mut self.st
                }
                fn draw(&mut self, _ctx: &mut DrawCtx) {}
            }
            let mut hv = HelpView {
                st: crate::view::ViewState::new(bounds),
            };
            hv.st.help_ctx = find_ctx;
            let modal_id = program.group_mut().insert(Box::new(hv));
            program.out_events.clear(); // discard focus broadcast

            program
                .captures
                .push(Box::new(ModalFrame::new(modal_id, bounds)));

            // Pump again — no events → idle arm reads modal's get_help_ctx()
            // = "app.find" → set_help_ctx("app.find") → OneOf def selected (index 0).
            program.pump_once();
            let def_with_modal = program
                .group_mut()
                .find_mut(sl)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_ref::<StatusLine>())
                .expect("status line found")
                .selected_def();
            assert_eq!(
                def_with_modal,
                Some(0),
                "modal with app.find helpCtx → OneOf def is index 0"
            );
        }

        // -- 8. Non-modal: idle path bubbles focused leaf help ctx ---------------

        /// REGRESSION TEST (gap #3): when no execView modal is active, the idle
        /// status-line arm must read `group.get_help_ctx()` (which recurses down the
        /// `current` chain to the focused leaf), not hard-code `NO_CONTEXT`.
        ///
        /// C++ baseline: `TStatusLine::update` calls `TopView()->getHelpCtx()`.
        /// `TView::TopView()` with no modal (`TheTopView == 0`) walks *up* the owner
        /// chain to the first `sfModal` view — the application root — whose
        /// `TGroup::getHelpCtx` then recurses *down* via `current`. Our root `group`
        /// IS that modal app root.
        ///
        /// Setup: a `Program` with a `OneOf(["app.search"]) + All` status line. A
        /// focusable leaf with `help_ctx = "app.search"` is inserted into the desktop
        /// (via `desktop_insert`, which focuses it). No modal is pushed. A single
        /// `pump_once` with no pending events triggers the idle arm. The status line
        /// must select def index 0 (the OneOf def keyed on "app.search"), NOT index 1
        /// (the All def, which would be selected if top_ctx were NO_CONTEXT).
        #[test]
        fn status_line_bubbles_focused_leaf_help_ctx_without_modal() {
            let find_ctx = crate::help::HelpCtx::custom("app.search");
            let (mut program, _handle, sl) = program_oneof(40, 10, find_ctx);

            // Build a minimal selectable view with the matching help_ctx.
            struct HelpLeaf {
                st: crate::view::ViewState,
            }
            impl crate::view::View for HelpLeaf {
                fn state(&self) -> &crate::view::ViewState {
                    &self.st
                }
                fn state_mut(&mut self) -> &mut crate::view::ViewState {
                    &mut self.st
                }
                fn draw(&mut self, _ctx: &mut DrawCtx) {}
            }
            use crate::view::Rect;
            let bounds = Rect::new(2, 2, 20, 4);
            let mut leaf = HelpLeaf {
                st: crate::view::ViewState::new(bounds),
            };
            leaf.st.help_ctx = find_ctx;
            // Make selectable so desktop_insert can focus it.
            leaf.st.options.selectable = true;

            // Insert into the desktop — this focuses the leaf, making it the
            // current of the desktop, which in turn is the current of the root group.
            program.desktop_insert(Box::new(leaf));
            // Clear RECEIVED_FOCUS broadcasts so the next pump_once hits the idle arm.
            program.out_events.clear();

            // No modal is active; pump_once must take the None branch and call
            // group.get_help_ctx(), which recurses root → desktop → leaf → find_ctx.
            program.pump_once();

            let def_no_modal = program
                .group_mut()
                .find_mut(sl)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_ref::<StatusLine>())
                .expect("status line found")
                .selected_def();
            assert_eq!(
                def_no_modal,
                Some(0),
                "non-modal desktop leaf with app.search helpCtx → OneOf def is index 0"
            );
        }
    }

    // -----------------------------------------------------------------------
    // History: the view-triggered async-modal seam
    // -----------------------------------------------------------------------
    mod history57 {
        use super::*;
        use crate::data::FieldValue;
        use crate::dialog::Dialog;
        use crate::view::SelectMode;
        use crate::widgets::{
            InputLine, LimitMode, THistory, clear_history, history_add, history_count, history_str,
        };

        /// An empty (no-validator) input line of `limit` bytes.
        fn input_line(bounds: Rect) -> InputLine {
            InputLine::new(bounds, 256, None, LimitMode::MaxBytes)
        }

        /// Read a view's text value through the generic `value()` protocol.
        fn link_text(program: &mut Program, id: ViewId) -> String {
            program
                .group_mut()
                .find_mut(id)
                .and_then(|v| v.value())
                .and_then(field_text)
                .unwrap_or_default()
        }

        /// Pump until `end_state` is set or `max` iterations elapse (headless never
        /// blocks, so a bounded loop is required). Uses the outer driver so a
        /// view-requested modal is actually executed.
        fn drive_until_idle(program: &mut Program, max: usize) {
            for _ in 0..max {
                program.pump_and_drive();
                if program.end_state.is_some() {
                    break;
                }
            }
        }

        // 6.1 — headline: pick → flowback into the linked input line.
        //
        // link + THistory as DIRECT ROOT CHILDREN (so the link survives the inner
        // modal's remove/drop — only the HistoryWindow is removed). Mouse trigger
        // (require_focus=false) sidesteps focus plumbing. Channel = [a, b]; setup
        // focuses item 1 ("b"), and the first-event currency fix routes the Enter
        // straight to the viewer → it picks "b" with no prior nav. Link kept EMPTY
        // so recordHistory-at-open ignores it (channel stays [a, b]).
        //
        // Bite: a no-op set_value → link stays empty → assert fails.
        #[test]
        fn pick_flows_back_into_link() {
            clear_history();
            history_add(7, "a");
            history_add(7, "b");

            let (mut program, handle, _clock) = program_with_desktop(80, 25);
            let link = program
                .group_mut()
                .insert(Box::new(input_line(Rect::new(5, 5, 25, 6))));
            let _hist = program.group_mut().insert(Box::new(THistory::new(
                Rect::new(25, 5, 28, 6),
                link,
                7,
            )));

            // Mouse-trigger the icon (root == absolute), then Enter picks the
            // setup-focused entry (item 1 == "b") with NO prior nav — the
            // first-event currency fix routes Enter straight to the viewer.
            handle.push_event(mouse_down_at(26, 5));
            handle.push_event(key(Key::Enter));

            drive_until_idle(&mut program, 30);

            let expected = history_str(7, 1).unwrap(); // "b" (setup focuses item 1)
            assert_eq!(
                link_text(&mut program, link),
                expected,
                "OK pick flows back into the link (expected the focused entry {expected:?})"
            );
        }

        // 6.2 — cancel writes nothing.
        #[test]
        fn cancel_leaves_link_unchanged() {
            clear_history();
            history_add(8, "a");
            history_add(8, "b");

            let (mut program, handle, _clock) = program_with_desktop(80, 25);
            let link = program
                .group_mut()
                .insert(Box::new(input_line(Rect::new(5, 5, 25, 6))));
            let _hist = program.group_mut().insert(Box::new(THistory::new(
                Rect::new(25, 5, 28, 6),
                link,
                8,
            )));

            // Esc with NO prior nav — the first-event currency fix routes it
            // straight to the viewer → endModal(cmCancel).
            handle.push_event(mouse_down_at(26, 5));
            handle.push_event(key(Key::Esc));

            drive_until_idle(&mut program, 30);

            assert_eq!(
                link_text(&mut program, link),
                "",
                "cancel must write nothing back (link stays empty)"
            );
        }

        // BITE for the open-time currency seam. A freshly-opened HistoryWindow must
        // dismiss on the FIRST focused event with NO prior nav: bare exec_view +
        // [Esc] → CANCEL, + [Enter] → OK. Without open-time currency the window's
        // internal `current` is None on open (Group::insert has no ctx), so
        // Esc/Enter reach no child, never set end_state, and the inner exec_view
        // spins forever (headless never blocks). The currency is established by
        // exec_view's kept post-insert virtual `reset_current` (the faithful open
        // hook) — the viewer is the popup's first visible+selectable child. The
        // first-event `select_child` workaround that used to live in
        // HistoryWindow::handle_event was redundant with that hook (this test
        // stays green with it retired) and has been removed.
        #[test]
        fn no_nav_first_event_dismisses_popup_bite() {
            clear_history();
            history_add(13, "a");
            history_add(13, "b");

            // [Esc] with no prior nav → CANCEL.
            let (mut program, handle, _clock) = program_with_desktop(80, 25);
            handle.push_event(key(Key::Esc));
            let hw = crate::widgets::HistoryWindow::new(Rect::new(5, 3, 45, 18), 13);
            assert_eq!(
                program.exec_view(Box::new(hw)),
                Command::CANCEL,
                "Esc as the first event dismisses the popup (cmCancel)"
            );

            // [Enter] with no prior nav → OK.
            let (mut program, handle, _clock) = program_with_desktop(80, 25);
            handle.push_event(key(Key::Enter));
            let hw = crate::widgets::HistoryWindow::new(Rect::new(5, 3, 45, 18), 13);
            assert_eq!(
                program.exec_view(Box::new(hw)),
                Command::OK,
                "Enter as the first event confirms the popup (cmOK)"
            );
        }

        // 6.3 — recordHistory records the link's CURRENT text at OPEN, and the
        // PICKED value is NOT re-recorded — driven through the OK path (the cancel
        // path never writes back, so it could not bite the "no double-record" half).
        //
        // Setup: channel = ["old"]; link text = "typed". The OpenHistory apply
        // records the link's CURRENT text → channel = ["old", "typed"] (oldest→
        // newest) BEFORE the popup's setup runs, so setup focuses item 1 ("typed").
        // We then Up to item 0 ("old") and Enter → OK picks "old" (flows back into
        // the link). The pick must NOT append a second "old".
        #[test]
        fn record_history_at_open_not_pick() {
            clear_history();
            history_add(9, "old"); // one existing entry

            let (mut program, handle, _clock) = program_with_desktop(80, 25);
            let mut il = input_line(Rect::new(5, 5, 25, 6));
            il.set_value(FieldValue::Text("typed".into())); // link's CURRENT text
            let link = program.group_mut().insert(Box::new(il));
            let _hist = program.group_mut().insert(Box::new(THistory::new(
                Rect::new(25, 5, 28, 6),
                link,
                9,
            )));

            // Open (records "typed" at OPEN), Up to item 0 ("old"), Enter → OK picks
            // it. First-event currency routes Up/Enter straight to the viewer.
            handle.push_event(mouse_down_at(26, 5));
            handle.push_event(key(Key::Up));
            handle.push_event(key(Key::Enter));
            drive_until_idle(&mut program, 30);

            // The OK pick flowed "old" back into the link (proves we drove OK, so the
            // no-double-record assertion below is non-vacuous).
            assert_eq!(
                link_text(&mut program, link),
                "old",
                "OK pick ('old', item 0) flows back into the link"
            );

            let entries: Vec<String> = (0..history_count(9))
                .filter_map(|i| history_str(9, i))
                .collect();
            // "typed" (the link's CURRENT text at OPEN) was recorded.
            assert!(
                entries.iter().any(|e| e == "typed"),
                "the link's CURRENT text at OPEN is recorded: {entries:?}"
            );
            // The PICKED value ("old") is NOT re-recorded by the OK flowback: still
            // exactly one "old". (recordHistory ran once, at OPEN, on the link's text;
            // the completion only set_values the link — it never history_adds.)
            assert_eq!(
                entries.iter().filter(|e| *e == "old").count(),
                1,
                "the picked value is never re-recorded on OK"
            );
        }

        // 6.4 — keyboard gate: ▼ with the link NOT focused → no modal; focused →
        // modal; mouse trigger opens regardless of focus.
        //
        // Uses pump_once (NOT pump_and_drive) so pending_modal is observable —
        // pump_and_drive would take + run it, always leaving None.
        #[test]
        fn keyboard_gate_requires_focus() {
            clear_history();
            history_add(11, "a");
            history_add(11, "b");

            let (mut program, handle, _clock) = program_with_desktop(80, 25);
            let link = program
                .group_mut()
                .insert(Box::new(input_line(Rect::new(5, 5, 25, 6))));
            let _hist = program.group_mut().insert(Box::new(THistory::new(
                Rect::new(25, 5, 28, 6),
                link,
                11,
            )));

            // (a) NOT focused: the keyDown still reaches THistory via postProcess,
            // but the require_focus gate drops the open.
            handle.push_event(key(Key::Down));
            program.pump_once();
            assert!(
                program.pending_modal.is_none(),
                "▼ with the link unfocused must NOT open a modal (the gate)"
            );

            // Focus the link, verify the bit is set (premise check), then ▼ opens.
            program.with_ctx(|g, ctx| g.set_current(Some(link), SelectMode::Normal, ctx));
            assert!(
                program
                    .group_mut()
                    .find_mut(link)
                    .map(|v| v.state().state.focused)
                    .unwrap_or(false),
                "premise: set_current(Normal) focuses the link"
            );
            // Discard the set_current side-effects (a queued RECEIVED/RELEASED_FOCUS
            // broadcast in out_events + command enables in deferred) so the next
            // pump_once pops OUR Down, not the leftover focus broadcast.
            program.out_events.clear();
            program.deferred.clear();
            handle.push_event(key(Key::Down));
            program.pump_once();
            assert!(
                program.pending_modal.is_some(),
                "▼ with the link focused opens the modal"
            );
            program.pending_modal = None; // discard (don't drive it)

            // (c) mouse trigger opens regardless of focus — clear focus first.
            program.with_ctx(|g, ctx| g.set_current(None, SelectMode::Normal, ctx));
            program.out_events.clear();
            program.deferred.clear();
            handle.push_event(mouse_down_at(26, 5));
            program.pump_once();
            assert!(
                program.pending_modal.is_some(),
                "mouse trigger opens regardless of the link's focus"
            );
        }

        // 6.5 — re-entrancy / end_state: the inner modal's end command does NOT
        // leak out to end the OUTER dialog modal. THistory lives INSIDE a Dialog
        // run via exec_view; mouse-trigger the icon at absolute coords, pick OK,
        // then cmCancel the dialog. exec_view must return CANCEL (not OK).
        //
        // Bite: removing the end_state save/restore → the inner OK leaks into the
        // dialog's `while end_state.is_none()` → exec_view returns OK before
        // cmCancel is processed.
        #[test]
        fn inner_modal_end_does_not_leak_to_outer() {
            clear_history();
            history_add(12, "a");
            history_add(12, "b");

            let (mut program, handle, _clock) = program_with_desktop(80, 25);

            // Dialog at a non-zero origin; THistory is a child of the dialog's
            // window-group (children share the window-group origin == dialog.a).
            let dlg_a = Point::new(10, 4);
            let mut dialog = Dialog::new(
                Rect::new(dlg_a.x, dlg_a.y, dlg_a.x + 30, dlg_a.y + 12),
                None,
            );
            let link = dialog.insert_child(Box::new(input_line(Rect::new(3, 3, 20, 4))));
            // THistory at window-group-local (20, 3) → absolute (30, 7).
            let hist_local = Point::new(20, 3);
            let _hist = dialog.insert_child(Box::new(THistory::new(
                Rect::new(
                    hist_local.x,
                    hist_local.y,
                    hist_local.x + 3,
                    hist_local.y + 1,
                ),
                link,
                12,
            )));

            // Mouse-trigger the icon at its absolute position, pick OK (Enter with
            // no prior nav — first-event currency fix), then cancel the dialog.
            let abs = Point::new(dlg_a.x + hist_local.x, dlg_a.y + hist_local.y);
            handle.push_event(mouse_down_at(abs.x, abs.y));
            handle.push_event(key(Key::Enter)); // inner HistoryWindow → endModal(OK)
            handle.push_event(Event::Command(Command::CANCEL)); // dialog → endModal(CANCEL)

            let result = program.exec_view(Box::new(dialog));
            assert_eq!(
                result,
                Command::CANCEL,
                "the inner modal's OK must NOT end the outer dialog; it ends on its own cmCancel"
            );
        }

        // 6.6 — descendant_global_bounds: nested root → dialog → link returns the
        // link's ABSOLUTE bounds (dialog origin + link-local). Non-zero dialog
        // origin so an identity-conversion bug would fail. Uses a real Dialog so
        // the Dialog→Window→inner-Group forward chain is exercised.
        #[test]
        fn descendant_global_bounds_through_dialog() {
            clear_history();

            let (mut program, _handle, _clock) = program_with_desktop(80, 25);

            let dlg_a = Point::new(10, 4);
            let mut dialog = Dialog::new(
                Rect::new(dlg_a.x, dlg_a.y, dlg_a.x + 30, dlg_a.y + 12),
                None,
            );
            // Link at window-group-local (3, 3)..(20, 4).
            let link_local = Rect::new(3, 3, 20, 4);
            let link = dialog.insert_child(Box::new(input_line(link_local)));
            let dlg_id = program.group_mut().insert(Box::new(dialog));

            // Absolute = dialog origin + link-local.
            let got = program
                .group_mut()
                .descendant_global_bounds(link, Point::new(0, 0));
            let expected = Rect::new(
                dlg_a.x + link_local.a.x,
                dlg_a.y + link_local.a.y,
                dlg_a.x + link_local.b.x,
                dlg_a.y + link_local.b.y,
            );
            assert_eq!(
                got,
                Some(expected),
                "absolute bounds = dialog-origin + link-local (non-identity conversion)"
            );

            // The dialog itself (a direct root child) resolves to its own absolute
            // bounds (acc == (0,0), so absolute == its root-local bounds).
            assert_eq!(
                program
                    .group_mut()
                    .descendant_global_bounds(dlg_id, Point::new(0, 0)),
                Some(Rect::new(dlg_a.x, dlg_a.y, dlg_a.x + 30, dlg_a.y + 12)),
                "the dialog itself (a direct root child) resolves to its own absolute bounds"
            );

            // A foreign id (minted but never inserted anywhere) resolves to None.
            let foreign = crate::view::ViewId::next();
            assert_eq!(
                program
                    .group_mut()
                    .descendant_global_bounds(foreign, Point::new(0, 0)),
                None,
                "an id absent from the tree resolves to None"
            );
        }
    }

    // -- message box ------------------------------------------------

    mod msgbox {
        use super::*;
        use crate::dialog::{MessageBoxButtons, MessageBoxKind};

        /// Esc → cmCancel: the simplest smoke test. Shows the dialog is keyboard-live
        /// on the first event (the reset_current seam established by exec_view).
        #[test]
        fn message_box_rect_esc_returns_cancel() {
            let (mut program, _handle, _clock) = program_with_desktop(80, 25);
            // Pre-queue Esc: the dialog converts it → cmCancel → endModal.
            program.out_events.push_back(key(Key::Esc));
            let r = crate::view::Rect::new(10, 5, 50, 14);
            let result = program.message_box_rect(
                r,
                "Something failed.",
                MessageBoxKind::Error,
                MessageBoxButtons::ok_cancel(),
            );
            assert_eq!(
                result,
                Command::CANCEL,
                "Esc → cmCancel ends the message box"
            );
            assert_eq!(program.capture_len(), 0, "ModalFrame popped");
        }

        /// `message_box` auto-centers on the desktop and returns OK via a direct
        /// `Event::Command(Command::OK)`.
        #[test]
        fn message_box_direct_ok_returns_ok() {
            let (mut program, _handle, _clock) = program_with_desktop(80, 25);
            program.out_events.push_back(Event::Command(Command::OK));
            let result = program.message_box(
                "Press OK to continue.",
                MessageBoxKind::Information,
                MessageBoxButtons::ok(),
            );
            assert_eq!(
                result,
                Command::OK,
                "direct cmOK ends the message box with OK"
            );
        }

        /// CLOBBER GUARD. `exec_view`'s `initial_focus` (C++
        /// messageBox's `selectNext(False)`) must SURVIVE the pump's
        /// `settle_currency` pass: every explicit `set_current` clears the
        /// owning group's pending `currency_dirty` (including on the
        /// `current == p` early-return leg — see
        /// `group::tests::set_current_early_return_still_clears_pending_insert_reset`
        /// for the direct pin), so the settle never re-runs `reset_current`
        /// over a deliberately-chosen focus. A regression here would snap the
        /// focused Yes button back to firstMatch (Cancel) on the first pump.
        #[test]
        fn settle_does_not_clobber_msgbox_initial_focus() {
            use crate::dialog::build_message_box;
            use crate::view::SelectMode;

            let (mut program, _handle, _clock) = program_with_desktop(80, 25);
            let (d, first_btn) = build_message_box(
                crate::view::Rect::new(10, 5, 50, 14),
                "Delete everything?",
                MessageBoxKind::Confirmation,
                MessageBoxButtons::yes_no_cancel(),
            );
            let first_btn = first_btn.expect("yes_no_cancel has an enabled first button");

            // Replicate exec_view's open steps (sans the blocking loop) — the
            // focused_space test's established pattern.
            let id = program.group_mut().insert(Box::new(d));
            if let Some(v) = program.group_mut().find_mut(id) {
                v.state_mut().options.selectable = false;
                v.state_mut().state.modal = true;
            }
            program.with_ctx(|g, ctx| {
                if let Some(v) = g.find_mut(id) {
                    v.reset_current(ctx); // step 5a: the faithful open hook
                }
                g.set_current(Some(id), SelectMode::Enter, ctx);
                if let Some(v) = g.find_mut(id) {
                    v.focus_descendant(first_btn, ctx); // initial_focus
                }
            });
            program.out_events.clear();

            let yes_focused = |program: &mut Program| {
                program
                    .group_mut()
                    .find_mut(first_btn)
                    .map(|v| v.state().state.focused)
                    .expect("first button resolves")
            };
            assert!(
                yes_focused(&mut program),
                "initial_focus focused the first (Yes) button"
            );

            // Pump once: the settle pass (step 2b) runs and must be a NO-OP —
            // every insert-time flag was superseded by the explicit currency ops.
            program.pump_once();
            assert!(
                yes_focused(&mut program),
                "settle did not clobber initial_focus back to firstMatch (Cancel)"
            );
        }

        /// Keyboard end-to-end: proves the currency seam (reset_current at open)
        /// AND the initial-focus seam (focus_descendant on the first button) work
        /// together, so focused-Space fires the FIRST button specifically.
        ///
        /// Strategy:
        /// 1. Build the dialog and insert it using `exec_view` internals, manually
        ///    stepping pump_once so we can interleave clock advancement.
        /// 2. Apply the focus_descendant step (mirrors exec_view's initial_focus).
        /// 3. Drain all startup/focus broadcasts so out_events is clean.
        /// 4. Pump Space → timer armed on the focused button (fail here if no focus).
        /// 5. Advance clock 200ms → pump → timer fires → command posted.
        /// 6. Pump to route the command → Dialog endModal.
        /// 7. Assert end_state == YES (the first button in yes_no_cancel order).
        ///
        /// This FAILS under the old Cancel-focus behavior (end_state would be CANCEL),
        /// pinning the selectNext(False) faithfulness fix.
        #[test]
        fn focused_space_fires_focused_button_discriminating() {
            use crate::dialog::build_message_box;
            use crate::view::{Rect, SelectMode};

            let (mut program, _handle, clock) = program_with_desktop(80, 25);

            // Build a Yes/No/Cancel message box. first_btn is the Yes button id
            // (the first enabled in [Yes, No, OK, Cancel] order).
            let bounds = Rect::new(10, 5, 50, 14);
            let (d, first_btn) = build_message_box(
                bounds,
                "Delete everything?",
                MessageBoxKind::Confirmation,
                MessageBoxButtons::yes_no_cancel(),
            );

            // -- Replicate exec_view setup (sans the outer loop) --
            let id = program.group_mut().insert(Box::new(d));

            // Set sfModal, clear ofSelectable (exec_view steps 3+4).
            if let Some(v) = program.group.find_mut(id) {
                v.state_mut().options.selectable = false;
                v.state_mut().state.modal = true;
            }

            // reset_current (the currency seam — exec_view step 5a).
            // This establishes the dialog's internal current (first selectable child
            // per firstMatch order) and then focuses the dialog in the root group,
            // cascading Focused down to that child.
            program.with_ctx(|g, ctx| {
                if let Some(v) = g.find_mut(id) {
                    v.reset_current(ctx);
                }
                g.set_current(Some(id), SelectMode::Enter, ctx);
            });

            // Push the ModalFrame (exec_view step 6).
            let bounds_for_frame = program
                .group
                .find_mut(id)
                .map(|v| v.state().get_bounds())
                .unwrap_or_default();
            program
                .captures
                .push(Box::new(crate::app::ModalFrame::new(id, bounds_for_frame)));

            // focus_descendant: mirrors exec_view's initial_focus step.
            // C++ selectNext(False) focuses the FIRST button (Yes for yes_no_cancel),
            // overriding the generic reset_current(firstMatch) which would focus Cancel.
            if let Some(focus_id) = first_btn {
                program.with_ctx(|g, ctx| {
                    if let Some(v) = g.find_mut(id) {
                        v.focus_descendant(focus_id, ctx);
                    }
                });
            }

            // Drain all queued focus broadcasts (RECEIVED_FOCUS, cmGrabDefault, etc.)
            // before injecting the Space key, so the first pump after push processes
            // Space (not a stale broadcast). The focus STATE was set synchronously
            // inside with_ctx; the broadcasts are cosmetic here.
            program.out_events.clear();

            // -- Pump Space: arms the animation timer on the focused button. --
            // The focused button is now Yes (first in insertion order, per the
            // initial_focus / focus_descendant step above).
            // Without the seam, no button is focused → Space is not consumed → timer not armed.
            program.out_events.push_back(key(Key::Char(' ')));
            program.pump_once();

            // The button should have armed a ~100ms animation timer.
            assert!(
                !program.timers.is_empty(),
                "Space on the focused button armed the animation timer — \
                 if no timer was armed, the button was NOT focused (currency seam absent)"
            );

            // -- Advance clock past timer expiry and pump until endModal. --
            //
            // The focused button is now Yes (first inserted for yes_no_cancel).
            // C++ selectNext(False) faithfulness: focus_descendant moved focus
            // from Cancel (firstMatch default) to Yes (first button).
            //
            // 6 pumps needed (not 4) because the deferred EnableCommand(NEXT/PREV/CLOSE)
            // effects from Window::set_state(Selected, true) are drained in pump 1
            // (Space), which sets command_set_changed=true. Pump A then generates BOTH
            // Broadcast(COMMAND_SET_CHANGED) AND Event::Timer(tid) into out_events:
            //
            //   pump A: idle → COMMAND_SET_CHANGED + collect_expired → Timer(tid)
            //   pump B: Broadcast(COMMAND_SET_CHANGED) → fan-out, no endModal
            //   pump C: Event::Timer(tid) → Yes fires → RECORD_HISTORY + YES
            //   pump D: Broadcast(RECORD_HISTORY) → fan-out, no endModal
            //   pump E: Command(YES) → Dialog → Deferred::EndModal(YES) → end_state=YES
            //   pump F: extra (cleanup)
            clock.advance(200);
            for _ in 0..6 {
                program.pump_once();
            }

            // The focused Yes button fired — assert we got YES (not Cancel).
            // This assertion FAILS under the old Cancel-focus behavior, pinning the fix.
            assert_eq!(
                program.end_state,
                Some(Command::YES),
                "focused Yes button fired YES; would be CANCEL under old Cancel-focus behavior"
            );

            // Cleanup: pop the capture frame and remove the dialog.
            program.captures.pop();
            program.with_ctx(|g, ctx| {
                g.remove(id, ctx);
            });
        }

        /// Two consecutive round-trips prove there is no leaked state between calls.
        #[test]
        fn message_box_rect_two_round_trips() {
            let (mut program, _handle, _clock) = program_with_desktop(80, 25);
            let r = crate::view::Rect::new(5, 3, 45, 12);

            // First: Esc → Cancel
            program.out_events.push_back(key(Key::Esc));
            let r1 = program.message_box_rect(
                r,
                "First dialog.",
                MessageBoxKind::Warning,
                MessageBoxButtons::ok_cancel(),
            );
            assert_eq!(r1, Command::CANCEL);
            assert_eq!(program.capture_len(), 0, "frame popped after first dialog");

            // Second: direct OK → OK
            program.out_events.push_back(Event::Command(Command::OK));
            let r2 = program.message_box_rect(
                r,
                "Second dialog.",
                MessageBoxKind::Information,
                MessageBoxButtons::ok(),
            );
            assert_eq!(r2, Command::OK);
            assert_eq!(program.capture_len(), 0, "frame popped after second dialog");
        }

        // -- input box --------------------------------------

        /// Cancel via Esc: `input_box_rect` returns `(CANCEL, initial)` — the
        /// initial string is left unchanged (no gather happens on cancel).
        #[test]
        fn input_box_rect_esc_returns_cancel_unchanged() {
            let (mut program, _handle, _clock) = program_with_desktop(80, 25);
            // Pre-queue Esc: the dialog converts it → cmCancel → endModal.
            program.out_events.push_back(key(Key::Esc));
            let r = crate::view::Rect::new(10, 5, 70, 13);
            let (cmd, text) = program.input_box_rect(r, "Title", "Name", "hello", 20);
            assert_eq!(cmd, Command::CANCEL, "Esc → cmCancel ends the input box");
            assert_eq!(
                text, "hello",
                "on cancel the initial string is returned unchanged"
            );
            assert_eq!(program.capture_len(), 0, "ModalFrame popped");
        }

        /// OK path: a direct `Event::Command(cmOK)` ends the modal with OK, and
        /// since the test types nothing, the gathered text is the scattered
        /// `initial` (the scatter→gather round-trip on the lone input line).
        #[test]
        fn input_box_rect_ok_returns_scattered_initial() {
            let (mut program, _handle, _clock) = program_with_desktop(80, 25);
            program.out_events.push_back(Event::Command(Command::OK));
            let r = crate::view::Rect::new(10, 5, 70, 13);
            let (cmd, text) = program.input_box_rect(r, "Title", "Name", "hello", 20);
            assert_eq!(cmd, Command::OK, "direct cmOK ends the input box with OK");
            assert_eq!(
                text, "hello",
                "OK gathers the scattered initial text back out (getData)"
            );
            assert_eq!(program.capture_len(), 0, "ModalFrame popped");
        }

        /// OK path with a TYPED edit — distinguishes a working gather from a
        /// broken one returning `None`. We scatter "hello", type 'X' into the
        /// focused input line (the modal's initial focus), then end with OK.
        /// `set_value` select-all's the scattered text, so the single typed char
        /// replaces the whole field → the gathered value is "X", which DIFFERS
        /// from `initial`. If the gather seam were broken (yielding `None`), the
        /// `input_box_rect` fallback would return the unchanged `initial`
        /// ("hello") and the `== "X"` assertion would fail — that is the point.
        #[test]
        fn input_box_rect_ok_returns_typed_edit() {
            let (mut program, _handle, _clock) = program_with_desktop(80, 25);
            // Queue a printable key (delivered to the focused input line) THEN OK.
            program.out_events.push_back(key(Key::Char('X')));
            program.out_events.push_back(Event::Command(Command::OK));
            let r = crate::view::Rect::new(10, 5, 70, 13);
            let (cmd, text) = program.input_box_rect(r, "Title", "Name", "hello", 20);
            assert_eq!(cmd, Command::OK, "direct cmOK ends the input box with OK");
            assert_eq!(
                text, "X",
                "the typed edit (replacing the selected scattered text) is gathered \
                 back out; a broken gather→None would return the unchanged \"hello\""
            );
            assert_eq!(program.capture_len(), 0, "ModalFrame popped");
        }

        /// `input_box` auto-centers on the desktop and round-trips the same way.
        #[test]
        fn input_box_centered_ok_round_trip() {
            let (mut program, _handle, _clock) = program_with_desktop(80, 25);
            program.out_events.push_back(Event::Command(Command::OK));
            let (cmd, text) = program.input_box("Enter", "Path", "/tmp", 40);
            assert_eq!(cmd, Command::OK);
            assert_eq!(text, "/tmp");
        }

        // -- open_file_dialog / FileDialog command-filter regression ------------

        /// REGRESSION: `cmFileOpen` (C++ stddlg.h 1001 — a `> 255` always-enabled
        /// command) must survive the pump's command filter, or the FileDialog
        /// OK/Open button does nothing (the modal never ends). Under the
        /// denylist it is enabled by default like every command not explicitly
        /// disabled (the historic allowlist dropped it — the "OK does nothing"
        /// bug). Drives the REAL pump.
        #[test]
        fn file_dialog_open_command_survives_pump_filter() {
            use crate::data::FieldValue;
            use crate::dialog::{FD_OPEN_BUTTON, FileDialog};
            let (mut program, _handle, _clock) = program_with_desktop(80, 25);
            let mut fd = FileDialog::new("*.*", "Open", "~N~ame", FD_OPEN_BUTTON, 100);
            // A concrete (non-wildcard, non-dir) name so valid() ACCEPTS instead of
            // navigating — isolates the filter fix from directory-navigation.
            View::set_value(&mut fd, FieldValue::Text("regression_test.txt".into()));
            program
                .out_events
                .push_back(Event::Command(Command::FILE_OPEN));
            let cmd = program.exec_view(Box::new(fd));
            assert_eq!(
                cmd,
                Command::FILE_OPEN,
                "cmFileOpen must end the modal — a dropped command would spin/hang \
                 (the 'OK does nothing' bug)"
            );
            assert_eq!(program.capture_len(), 0, "ModalFrame popped on close");
        }

        /// The file-dialog result commands C++ treats as always-enabled (`> 255`)
        /// are enabled by default under the denylist — no registration, no
        /// bandaid list (the allowlist-era fix this replaces).
        #[test]
        fn file_dialog_result_commands_enabled_by_default() {
            let (program, _handle, _clock) = program_with_desktop(12, 4);
            for cmd in [
                Command::FILE_OPEN,
                Command::FILE_REPLACE,
                Command::FILE_CLEAR,
                Command::FILE_INIT,
                Command::CHANGE_DIR,
                Command::REVERT,
            ] {
                assert!(
                    program.command_enabled(cmd),
                    "{cmd:?} must be enabled by default"
                );
            }
        }

        // -- EditWindow desktop_insert focus regression -------------------------

        /// REGRESSION: an EditWindow inserted via desktop_insert must arrive with its
        /// FileEditor focused, or typing (and Save) do nothing — the "edit/save does not
        /// work" bug. C++ focuses the editor via show()->resetCurrent at insert; tvision-rs's
        /// ctx-less Group::insert skips that, so insert_and_focus must reset_current
        /// before focus_child. Drives the REAL pump and asserts the typed char lands in
        /// the editor buffer (symptom-level, not a focus-flag proxy).
        #[test]
        fn inserted_edit_window_receives_typed_characters() {
            let (mut program, _handle, _clock) = program_with_desktop(80, 25);
            let r = program.desktop_rect();
            let ew = crate::widgets::EditWindow::new(r, None, 1);
            let editor_id = ew.editor_id;
            program.desktop_insert(Box::new(ew));
            // Clear any RECEIVED_FOCUS broadcasts queued by the insert (they would
            // be processed before the typed key and consume the single pump_once
            // call, leaving the KeyDown undelivered).
            program.out_events.clear();

            program.out_events.push_back(Event::KeyDown(KeyEvent::new(
                Key::Char('X'),
                KeyModifiers::default(),
            )));
            program.pump_once();

            let text = program
                .group_mut()
                .find_mut(editor_id)
                .and_then(crate::widgets::editor_mut)
                .map(|e| String::from_utf8_lossy(&e.text()).into_owned())
                .unwrap_or_default();
            assert_eq!(
                text, "X",
                "the typed char must land in the inserted EditWindow's editor — a window \
                 that opens keyboard-dead (editor never focused) would leave this empty"
            );
        }

        // -- color_dialog (tvision-rs-original extension) --------------------

        /// OK returns `Some(color)` — the initial color is returned unchanged when
        /// nothing is edited (exec_view_with extracts the picker's color on cmOK).
        #[test]
        fn color_dialog_ok_returns_initial_color() {
            use crate::color::Color;
            let (mut program, _handle, _clock) = program_with_desktop(80, 30);
            program.out_events.push_back(Event::Command(Command::OK));
            let result = program.color_dialog(Color::Rgb(30, 144, 255));
            assert_eq!(
                result,
                Some(Color::Rgb(30, 144, 255)),
                "OK with no edits returns the seeded initial color"
            );
            assert_eq!(program.capture_len(), 0, "ModalFrame popped on close");
        }

        /// Cancel returns `None` — `color_dialog` yields `None` when the dialog ends
        /// with `cmCancel`.
        #[test]
        fn color_dialog_cancel_returns_none() {
            use crate::color::Color;
            let (mut program, _handle, _clock) = program_with_desktop(80, 30);
            program
                .out_events
                .push_back(Event::Command(Command::CANCEL));
            let result = program.color_dialog(Color::Rgb(255, 0, 0));
            assert_eq!(result, None, "Cancel yields None");
            assert_eq!(program.capture_len(), 0, "ModalFrame popped on cancel");
        }

        /// Esc triggers cmCancel via the Dialog's key handler, returning `None`.
        #[test]
        fn color_dialog_esc_returns_none() {
            use crate::color::Color;
            let (mut program, _handle, _clock) = program_with_desktop(80, 30);
            program.out_events.push_back(key(Key::Esc));
            let result = program.color_dialog(Color::Default);
            assert_eq!(result, None, "Esc → cmCancel → None");
        }
    }

    // -- theme editor -------------------------------------------------------
    mod theme_editor_c8 {
        use super::*;

        /// Cancel: the dialog ends with `cmCancel` and no new theme is installed.
        #[test]
        fn theme_editor_cancel_leaves_theme_unchanged() {
            let (mut program, _handle, _clock) = program_with_desktop(80, 30);
            let original_theme = program.theme.clone();
            program
                .out_events
                .push_back(Event::Command(Command::CANCEL));
            program.theme_editor();
            assert_eq!(
                program.theme, original_theme,
                "cancel must not change the installed theme"
            );
            assert_eq!(program.capture_len(), 0, "ModalFrame popped on cancel");
        }

        /// OK with no edits installs an equal theme (the working copy was not
        /// modified, so the result is the same as the original).
        #[test]
        fn theme_editor_ok_installs_equal_theme() {
            let (mut program, _handle, _clock) = program_with_desktop(80, 30);
            let original_theme = program.theme.clone();
            program.out_events.push_back(Event::Command(Command::OK));
            program.theme_editor();
            assert_eq!(
                program.theme, original_theme,
                "OK with no edits yields an equivalent theme"
            );
            assert_eq!(program.capture_len(), 0, "ModalFrame popped on OK");
        }

        /// OK extracts the ThemeEditorBody's modified working theme BY VALUE,
        /// exercising the public `exec_view_with` wrapper over the same core
        /// (`exec_view_capture`) that `Program::theme_editor` uses.
        #[test]
        fn theme_editor_ok_installs_new_theme() {
            use crate::color::{Color, Style};
            use crate::dialog::{Dialog, ThemeEditorBody};
            use crate::theme::{Role, Theme};

            let original = Theme::classic_blue();
            let mut modified = original.clone();
            modified.set_style(
                Role::Background,
                Style::new(Color::Bios(0xF), Color::Bios(0x0)),
            );
            assert_ne!(modified, original, "test setup: modified theme must differ");

            let (mut program, _handle, _clock) = program_with_desktop(80, 30);

            // A theme-editor-shaped modal pre-seeded with the modified working theme.
            let mut d = Dialog::new(
                crate::view::Rect::new(0, 0, 64, 24),
                Some("Theme Editor".to_string()),
            );
            let te_id = d.insert_child(Box::new(ThemeEditorBody::new(
                crate::view::Rect::new(1, 1, 63, 19),
                modified.clone(),
            )));

            program.out_events.push_back(Event::Command(Command::OK));
            let extracted = program.exec_view_with(Box::new(d), |modal, cmd| {
                if cmd == Command::OK {
                    modal
                        .find_mut(te_id)
                        .and_then(|v| v.as_any_mut())
                        .and_then(|a| a.downcast_mut::<ThemeEditorBody>())
                        .map(|te| te.working_theme().clone())
                } else {
                    None
                }
            });
            assert_eq!(
                extracted,
                Some(modified),
                "OK must extract the modified working theme by value"
            );
        }
    }

    // -- the clipboard chain at pump level ------------------------
    //
    // The editor broker unit tests (widgets/editor.rs "clipboard broker") prove
    // clipCopy/clipPaste QUEUE the right Deferred ops; these prove the pump
    // APPLIES them against the backend — observable through the new
    // `HeadlessHandle::clipboard`/`set_clipboard` accessors (the headless
    // backend keeps plain internal-string semantics by design; the production
    // chain is unit-tested in backend/clipboard.rs).
    mod clipboard_a6 {
        use super::*;
        use crate::event::{Key, KeyEvent, KeyModifiers};
        use crate::widgets::EditWindow;

        /// Insert an EditWindow (its FileEditor arrives focused — proven by
        /// `inserted_edit_window_receives_typed_characters`), clear the insert
        /// broadcasts, and return the editor id.
        fn editor_program() -> (Program, crate::backend::HeadlessHandle, ViewId) {
            let (mut program, handle, _clock) = program_with_desktop(80, 25);
            let r = program.desktop_rect();
            let ew = EditWindow::new(r, None, 1);
            let editor_id = ew.editor_id;
            program.desktop_insert(Box::new(ew));
            program.out_events.clear();
            (program, handle, editor_id)
        }

        fn push_key(program: &mut Program, key: Key, modifiers: KeyModifiers) {
            program
                .out_events
                .push_back(Event::KeyDown(KeyEvent::new(key, modifiers)));
        }

        /// cmCopy path: type text, Shift+Home (select to line start), Ctrl+Insert
        /// (the C++ kbCtrlIns → cmCopy mapping) — the queued
        /// `Deferred::SetClipboard` must land on the backend, visible through
        /// `HeadlessHandle::clipboard`.
        #[test]
        fn copy_keystroke_reaches_backend_clipboard() {
            let (mut program, handle, _ed) = editor_program();
            for c in "hello".chars() {
                push_key(&mut program, Key::Char(c), KeyModifiers::default());
            }
            push_key(
                &mut program,
                Key::Home,
                KeyModifiers {
                    shift: true,
                    ..Default::default()
                },
            );
            push_key(
                &mut program,
                Key::Insert,
                KeyModifiers {
                    ctrl: true,
                    ..Default::default()
                },
            );
            assert_eq!(handle.clipboard(), None, "clipboard starts empty");
            for _ in 0..10 {
                program.pump_once();
            }
            assert_eq!(
                handle.clipboard().as_deref(),
                Some("hello"),
                "Ctrl+Insert copies the selection onto the backend clipboard"
            );
        }

        /// cmPaste path: seed the backend clipboard via the handle, Shift+Insert
        /// (kbShiftIns → cmPaste) — the `Deferred::EditorPaste` broker must read
        /// the backend and insert into the editor buffer.
        #[test]
        fn seeded_clipboard_pastes_into_editor() {
            let (mut program, handle, editor_id) = editor_program();
            handle.set_clipboard("pasted");
            push_key(
                &mut program,
                Key::Insert,
                KeyModifiers {
                    shift: true,
                    ..Default::default()
                },
            );
            for _ in 0..4 {
                program.pump_once();
            }
            let text = program
                .group_mut()
                .find_mut(editor_id)
                .and_then(crate::widgets::editor_mut)
                .map(|e| String::from_utf8_lossy(&e.text()).into_owned())
                .unwrap_or_default();
            assert_eq!(
                text, "pasted",
                "Shift+Insert inserts the seeded backend clipboard text"
            );
        }
    }

    // -- bracketed-paste ------------------------------------------------------
    //
    // Proves that `Event::Paste` is routed to the focused editor and inserts
    // multi-char / multi-line text into the buffer (porting kbPaste /
    // setPasteText / getPasteEvent from tevent.cpp).
    mod bracketed_paste_c9 {
        use super::*;
        use crate::widgets::EditWindow;

        fn editor_program() -> (Program, crate::backend::HeadlessHandle, ViewId) {
            let (mut program, handle, _clock) = program_with_desktop(80, 25);
            let r = program.desktop_rect();
            let ew = EditWindow::new(r, None, 1);
            let editor_id = ew.editor_id;
            program.desktop_insert(Box::new(ew));
            program.out_events.clear();
            (program, handle, editor_id)
        }

        /// `Event::Paste` inserts multi-char text into the focused editor.
        #[test]
        fn bracketed_paste_inserts_into_editor() {
            let (mut program, handle, editor_id) = editor_program();
            handle.push_paste("hello world");
            for _ in 0..4 {
                program.pump_once();
            }
            let text = program
                .group_mut()
                .find_mut(editor_id)
                .and_then(crate::widgets::editor_mut)
                .map(|e| String::from_utf8_lossy(&e.text()).into_owned())
                .unwrap_or_default();
            assert_eq!(
                text, "hello world",
                "bracketed paste inserts multi-char text"
            );
        }

        /// Multi-line paste inserts newlines faithfully.
        #[test]
        fn bracketed_paste_with_newlines() {
            let (mut program, handle, editor_id) = editor_program();
            handle.push_paste("line1\nline2");
            for _ in 0..4 {
                program.pump_once();
            }
            let text = program
                .group_mut()
                .find_mut(editor_id)
                .and_then(crate::widgets::editor_mut)
                .map(|e| String::from_utf8_lossy(&e.text()).into_owned())
                .unwrap_or_default();
            assert_eq!(text, "line1\nline2", "bracketed paste preserves newlines");
        }
    }

    // -- editor right-click context menu -------------------------------------
    //
    // Proves that a right-click MouseDown on the editor opens a 4-item popup
    // menu session (Cut / Copy / Paste / Undo), wiring `initContextMenu` +
    // `popupMenu` (teditor2.cpp:97-107, teditor1.cpp:532-536).
    mod context_menu_c2 {
        use super::*;
        use crate::event::{Event, MouseButtons, MouseEvent};
        use crate::widgets::EditWindow;

        fn right_click_editor(program: &mut Program, x: i32, y: i32) {
            program.out_events.push_back(Event::MouseDown(MouseEvent {
                position: Point::new(x, y),
                buttons: MouseButtons {
                    right: true,
                    ..Default::default()
                },
                ..Default::default()
            }));
            program.pump_once();
        }

        /// Right-clicking the editor opens a 4-item context menu popup.
        /// Verifies the capture stack grows by 1 (the popup session is armed).
        #[test]
        fn right_click_opens_context_menu() {
            let (mut program, _handle, _clock) = program_with_desktop(80, 25);
            let r = program.desktop_rect();
            let ew = EditWindow::new(r, None, 1);
            program.desktop_insert(Box::new(ew));
            program.out_events.clear();

            let baseline = program.capture_len();
            // The EditWindow has a 1-cell frame; editor content starts at
            // (r.a.x+1, r.a.y+1). Click inside the editor area.
            right_click_editor(&mut program, r.a.x + 2, r.a.y + 2);

            assert_eq!(
                program.capture_len(),
                baseline + 1,
                "right-click opens exactly one popup menu session on the capture stack"
            );
        }
    }

    // -- InputLine clipboard at pump level --------------------------------
    //
    // Proves that the Deferred::SetClipboard / Deferred::InputLinePaste brokers
    // are applied by the pump — observable through HeadlessHandle::clipboard /
    // set_clipboard (analogous to the editor clipboard_a6 suite above).
    mod input_line_clipboard_b1_b3 {
        use super::*;
        use crate::dialog::Dialog;
        use crate::event::{Key, KeyEvent, KeyModifiers};
        use crate::widgets::{Button, ButtonFlags, InputLine};

        /// Insert a Dialog containing one focused InputLine; return the program,
        /// handle, and the InputLine's id. The dialog is focused via desktop_insert.
        fn input_line_program() -> (Program, crate::backend::HeadlessHandle, ViewId) {
            let (mut program, handle, _clock) = program_with_desktop(40, 12);
            let mut dialog = Dialog::new(Rect::new(2, 1, 38, 11), Some("Test".into()));
            let il_id =
                dialog.insert_child(Box::new(InputLine::with_limit(Rect::new(2, 2, 28, 3), 64)));
            program
                .desktop_insert(Box::new(dialog))
                .expect("dialog inserted");
            program.out_events.clear();
            (program, handle, il_id)
        }

        fn push_key(program: &mut Program, key: Key, modifiers: KeyModifiers) {
            program
                .out_events
                .push_back(Event::KeyDown(KeyEvent::new(key, modifiers)));
        }

        /// Copy path: type text into the focused InputLine, select-all, then
        /// send cmCopy — the Deferred::SetClipboard must land on the backend,
        /// visible through HeadlessHandle::clipboard.
        #[test]
        fn b3_copy_reaches_backend_clipboard() {
            let (mut program, handle, il_id) = input_line_program();

            // Type "hello" into the focused InputLine.
            for c in "hello".chars() {
                push_key(&mut program, Key::Char(c), KeyModifiers::default());
            }
            for _ in 0..6 {
                program.pump_once();
            }

            // Shift+Home selects all the typed text (the field is short enough).
            push_key(
                &mut program,
                Key::Home,
                KeyModifiers {
                    shift: true,
                    ..Default::default()
                },
            );
            program.pump_once();

            // Inject cmCopy directly (the InputLine handles evCommand).
            program.out_events.push_back(Event::Command(Command::COPY));
            assert_eq!(handle.clipboard(), None, "clipboard starts empty");
            program.pump_once();

            assert_eq!(
                handle.clipboard().as_deref(),
                Some("hello"),
                "cmCopy must land the selection on the backend clipboard"
            );
            let _ = il_id; // keep id in scope
        }

        /// Paste path: seed the backend clipboard, inject cmPaste —
        /// the Deferred::InputLinePaste broker must read it and insert into the
        /// InputLine.
        #[test]
        fn b3_paste_from_backend_clipboard_into_input_line() {
            let (mut program, handle, il_id) = input_line_program();

            // Seed the clipboard BEFORE the pump applies the paste.
            handle.set_clipboard("world");

            // Inject cmPaste.
            program.out_events.push_back(Event::Command(Command::PASTE));
            for _ in 0..4 {
                program.pump_once();
            }

            // Read the InputLine's data via its value().
            let val = program
                .group_mut()
                .find_mut(il_id)
                .and_then(|v| v.value())
                .expect("InputLine found");
            use crate::data::FieldValue;
            assert_eq!(
                val,
                FieldValue::Text("world".into()),
                "Deferred::InputLinePaste must insert clipboard text into the field"
            );
        }

        /// Button graying at pump level: a button whose command is disabled via
        /// `program.disable_command` transitions to `disabled = true` after the idle
        /// pump broadcasts cmCommandSetChanged.
        #[test]
        fn b1_button_grays_after_disable_command_broadcast() {
            let (mut program, _handle, _clock) = program_with_desktop(40, 12);
            let mut dialog = Dialog::new(Rect::new(2, 1, 38, 11), Some("G".into()));
            let btn_id = dialog.insert_child(Box::new(Button::new(
                Rect::new(2, 4, 12, 6),
                "OK",
                Command::OK,
                ButtonFlags::new(),
            )));
            program
                .desktop_insert(Box::new(dialog))
                .expect("dialog inserted");
            program.out_events.clear();

            // Disable cmOK: the next idle pump broadcasts cmCommandSetChanged.
            program.disable_command(Command::OK);

            // Run several pumps so the idle broadcast fires and the button reacts.
            for _ in 0..8 {
                program.pump_once();
            }

            let btn = program
                .group_mut()
                .find_mut(btn_id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<Button>())
                .expect("button found");
            assert!(
                btn.state.state.disabled,
                "button must be disabled after cmCommandSetChanged broadcast"
            );
        }
    }

    // -- the async-modal-from-a-view seam (messageBox from valid()) -----------
    //
    // Three tests for the three structurally-different valid() call sites
    // (docs/design/async-modal-from-view.md "Verification"):
    //   1. FileEditor modified-save prompt over the deferred handle_event path
    //      (window-close → pending_modal → RouteModalAnswer → re-post cmClose).
    //   2. Validator error box over the INLINE modal-close path (validate_modal_close
    //      drives the box inline; driven surgically — the full exec_view drive would
    //      busy-loop headlessly, which is the correct runtime "block for the user").
    //   3. Validator error box over the deferred focus-leave path (Deferred queued).
    mod async_modal_from_view {
        use super::*;
        use crate::command::Command;
        use crate::event::{Key, KeyEvent, KeyModifiers};
        use crate::validate::Validator;
        use crate::view::{StateFlag, View};
        use crate::widgets::{EditWindow, InputLine, LimitMode};

        /// A validator that rejects every final value (so `valid()` fails) AND pops
        /// an error box — mirrors the concrete validators (whose `error` emits the
        /// box; the abstract-base default is a no-op, so a test stub must override
        /// `error` to exercise the seam).
        struct RejectAll;
        impl Validator for RejectAll {
            fn is_valid(&self, _s: &str) -> bool {
                false
            }
            fn error(&self, ctx: &mut Context) {
                ctx.request_message_box(
                    "rejected".to_string(),
                    crate::dialog::MessageBoxKind::Error,
                    crate::dialog::MessageBoxButtons::ok(),
                    None,
                    None,
                );
            }
        }

        /// A unique temp path for a save test (cleaned up by the caller).
        fn tmp(tag: &str) -> std::path::PathBuf {
            let pid = std::process::id();
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            std::env::temp_dir().join(format!("rstv_amfv_{tag}_{pid}_{nanos}.txt"))
        }

        /// Insert an `EditWindow` (backed by `path`) into the desktop's group,
        /// select + make-current the window so the focused `cmClose` routes to it,
        /// enable `cmClose`, mark the editor modified (by feeding a key straight to
        /// the editor — no focus needed), and return the window + editor ids.
        fn modified_edit_window(
            program: &mut Program,
            path: Option<std::path::PathBuf>,
        ) -> (ViewId, ViewId) {
            let ew = EditWindow::new(Rect::new(2, 1, 60, 18), path, 1);
            let editor_id = ew.editor_id;
            let win_id = program.group_mut().insert(Box::new(ew));

            // Select + current (mirror close_round_trip_removes_window).
            program.with_ctx(|g, ctx| {
                g.set_current(Some(win_id), SelectMode::Normal, ctx);
                g.find_mut(win_id)
                    .unwrap()
                    .set_state(StateFlag::Selected, true, ctx);
            });
            program.enable_command(Command::CLOSE);

            // Mark modified by feeding a printable key straight to the editor (focus
            // is about routing; a direct handle_event bypasses it).
            program.with_ctx(|g, ctx| {
                let mut ev = Event::KeyDown(KeyEvent::new(Key::Char('Z'), KeyModifiers::default()));
                g.find_mut(editor_id).unwrap().handle_event(&mut ev, ctx);
            });
            // Setup assertion: the buffer is genuinely modified now. The child is a
            // FileEditor (EditWindow's editor), so peel to its inner Editor via
            // editor_mut (its own as_any_mut returns the FileEditor).
            let modified = program
                .group_mut()
                .find_mut(editor_id)
                .and_then(crate::widgets::editor_mut)
                .map(|e| e.modified())
                .unwrap_or(false);
            assert!(modified, "setup: the editor must be modified before close");

            program.out_events.clear();
            (win_id, editor_id)
        }

        /// Drive `pump_and_drive` a bounded number of times (the box opens via
        /// `pending_modal`, so a plain `pump_once` is not enough). Never unbounded —
        /// a headless backend never blocks.
        fn drive(program: &mut Program, n: usize) {
            for _ in 0..n {
                program.pump_and_drive();
            }
        }

        /// 1a. cmClose on a modified FileEditor + pre-queued cmYes → file written,
        /// window removed.
        #[test]
        fn file_editor_close_yes_saves_and_closes() {
            let (mut program, _h, _c) = program_with_desktop(80, 25);
            let path = tmp("yes");
            let _ = std::fs::remove_file(&path);
            let (win_id, _ed) = modified_edit_window(&mut program, Some(path.clone()));

            // cmClose triggers the prompt; cmYes answers it.
            program.out_events.push_back(Event::Command(Command::CLOSE));
            program.out_events.push_back(Event::Command(Command::YES));
            drive(&mut program, 12);

            assert!(
                program.group_mut().find_mut(win_id).is_none(),
                "Yes → save succeeds → window removed"
            );
            assert!(path.exists(), "Yes → file written to disk");
            let _ = std::fs::remove_file(&path);
        }

        /// The pending_modal (handle_event) route must thread the FIRST-button focus
        /// (C++ `messageBox` selectNext(False)) so the box's default button (Yes for
        /// yes_no_cancel) is focused on open — NOT `None`, which would let
        /// `reset_current`'s firstMatch focus the LAST button (Cancel). We use
        /// `pump_once` (not `pump_and_drive`) so the box is STASHED in `pending_modal`
        /// but not yet executed, then inspect the carried `initial_focus`.
        ///
        /// The proof: `initial_focus` is `Some(id)` (a regressed `None` fails here),
        /// that `id` resolves inside the stashed box, AND it sits at the FIRST-button
        /// (Yes) POSITION — not the last (Cancel) a `None`/firstMatch would yield.
        ///
        /// Ids cannot be compared across box instances (the global `ViewId::next()`
        /// counter gives a fresh reference box higher ids), so we compare **layout
        /// positions** via `descendant_global_bounds`: button layout is deterministic
        /// from the box RECT, so a reference box built with the stashed box's own
        /// bounds has a byte-identical interior. The Yes/No/Cancel buttons share a
        /// y-row and differ in x, so `Rect` equality discriminates first-vs-last. The
        /// reference's first button is `build_message_box`'s documented first
        /// (already proven to fire `YES` by
        /// `focused_space_fires_focused_button_discriminating`); a regressed
        /// `Some(Cancel)` focus would land at a different x and fail the `assert_eq!`.
        #[test]
        fn file_editor_close_prompt_focuses_first_button_on_pending_modal_path() {
            use crate::dialog::{MessageBoxButtons, MessageBoxKind, build_message_box};

            let (mut program, _h, _c) = program_with_desktop(80, 25);
            let _ = modified_edit_window(&mut program, Some(tmp("focus")));

            // cmClose triggers the prompt; pump_once stashes the box into
            // pending_modal WITHOUT driving it (so we can inspect initial_focus).
            program.out_events.push_back(Event::Command(Command::CLOSE));
            program.pump_once();

            let (modal, _completion, initial_focus) = program
                .pending_modal
                .as_mut()
                .expect("cmClose on a modified editor stashes the prompt in pending_modal");

            // (a) A real focus target is threaded (the regression was `None`).
            let focus_id =
                initial_focus.expect("the first-button focus is threaded (not None / Cancel)");

            // (b) It resolves to a child of the stashed message box (with bounds).
            let focus_bounds = modal
                .descendant_global_bounds(focus_id, Point::new(0, 0))
                .expect("the threaded initial_focus resolves to a descendant with bounds");

            // (c) It sits at the FIRST-button (Yes) position. Layout is deterministic
            //     from the RECT, so build the reference with the STASHED box's own
            //     bounds — identical interior — and compare the first-button POSITION.
            let box_bounds = modal.state().get_bounds();
            let (ref_box, ref_first) = build_message_box(
                box_bounds,
                "Save untitled file?",
                MessageBoxKind::Information,
                MessageBoxButtons::yes_no_cancel(),
            );
            let ref_first_bounds = ref_box
                .descendant_global_bounds(
                    ref_first.expect("yes_no_cancel has a first (Yes) button"),
                    Point::new(0, 0),
                )
                .expect("the reference first button has bounds");

            assert_eq!(
                focus_bounds, ref_first_bounds,
                "threaded initial_focus sits at the FIRST-button (Yes) position; a \
                 regressed last-button (Cancel) focus would be at a different x"
            );
        }

        /// 1b. cmNo → window removed, file NOT written, modified cleared.
        #[test]
        fn file_editor_close_no_discards_and_closes() {
            let (mut program, _h, _c) = program_with_desktop(80, 25);
            let path = tmp("no");
            let _ = std::fs::remove_file(&path);
            let (win_id, _ed) = modified_edit_window(&mut program, Some(path.clone()));

            program.out_events.push_back(Event::Command(Command::CLOSE));
            program.out_events.push_back(Event::Command(Command::NO));
            drive(&mut program, 12);

            assert!(
                program.group_mut().find_mut(win_id).is_none(),
                "No → allow-close → window removed"
            );
            assert!(!path.exists(), "No → file NOT written");
            let _ = std::fs::remove_file(&path);
        }

        /// 1c. cmCancel → window stays open, still modified.
        #[test]
        fn file_editor_close_cancel_keeps_window() {
            let (mut program, _h, _c) = program_with_desktop(80, 25);
            let path = tmp("cancel");
            let _ = std::fs::remove_file(&path);
            let (win_id, editor_id) = modified_edit_window(&mut program, Some(path.clone()));

            program.out_events.push_back(Event::Command(Command::CLOSE));
            program
                .out_events
                .push_back(Event::Command(Command::CANCEL));
            drive(&mut program, 12);

            assert!(
                program.group_mut().find_mut(win_id).is_some(),
                "Cancel → veto close → window stays"
            );
            let still_modified = program
                .group_mut()
                .find_mut(editor_id)
                .and_then(crate::widgets::editor_mut)
                .map(|e| e.modified())
                .unwrap_or(false);
            assert!(still_modified, "Cancel → buffer still modified");
            assert!(!path.exists(), "Cancel → file NOT written");
            let _ = std::fs::remove_file(&path);
        }

        /// Test 2 — validator error box over the INLINE modal-close path. Driven
        /// surgically via `validate_modal_close` — a full `exec_view` drive would
        /// busy-loop headlessly (the correct runtime "block for the user"). The
        /// proof a box was DRIVEN: it consumed the pre-queued cmOK and the field
        /// stays invalid (returns false).
        #[test]
        fn validator_error_box_inline_on_modal_close() {
            let (mut program, _h, _c) = program_with_desktop(80, 25);

            // A Dialog with a rejecting-validator InputLine, inserted into the root.
            let mut d = crate::dialog::Dialog::new(Rect::new(5, 3, 55, 15), Some("D".into()));
            let il = InputLine::new(
                Rect::new(2, 2, 30, 3),
                40,
                Some(Box::new(RejectAll)),
                LimitMode::MaxBytes,
            );
            d.insert_child(Box::new(il));
            let dialog_id = program.group_mut().insert(Box::new(d));

            // The error box (OK-only) consumes this cmOK when driven inline.
            program.out_events.push_back(Event::Command(Command::OK));
            let valid = program.validate_modal_close(dialog_id, Command::OK);

            assert!(!valid, "a rejecting field keeps the dialog invalid");
            // Proof the box was DRIVEN inline: it consumed the pre-queued cmOK (it
            // ended the box). If no box had run, the cmOK would still be queued.
            // (Modal open/close emits focus broadcasts, so the whole queue is not
            // empty — assert the cmOK specifically is gone, not the whole queue.)
            assert!(
                !program
                    .out_events
                    .iter()
                    .any(|e| matches!(e, Event::Command(c) if *c == Command::OK)),
                "the error box was driven inline (it consumed the queued cmOK)"
            );
        }

        /// Test 3 — validator error box over the deferred focus-leave path: tabbing
        /// out of a bad ofValidate field queues a `Deferred::OpenMessageBox` (and
        /// refuses the focus switch). Inspected on a bare Group + local ctx — fully
        /// deterministic, no pump/modal.
        #[test]
        fn validator_error_box_deferred_on_focus_leave() {
            let mut group = Group::new(Rect::new(0, 0, 40, 12));
            let first = {
                let mut il = InputLine::new(
                    Rect::new(2, 1, 30, 2),
                    40,
                    Some(Box::new(RejectAll)),
                    LimitMode::MaxBytes,
                );
                let st = View::state_mut(&mut il);
                st.options.selectable = true;
                st.options.validate = true; // ofValidate gates the RELEASED_FOCUS branch
                group.insert(Box::new(il))
            };
            let second = {
                let mut il = InputLine::new(Rect::new(2, 4, 30, 5), 40, None, LimitMode::MaxBytes);
                View::state_mut(&mut il).options.selectable = true;
                group.insert(Box::new(il))
            };

            let mut out = std::collections::VecDeque::new();
            let mut timers = crate::timer::TimerQueue::new();
            let mut deferred: Vec<Deferred> = Vec::new();

            // Make `first` current, then try to move focus to `second`.
            {
                let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
                group.set_current(Some(first), SelectMode::Normal, &mut ctx);
            }
            deferred.clear();
            let switched = {
                let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
                group.focus_child(second, &mut ctx)
            };

            assert!(!switched, "a bad ofValidate field refuses the focus switch");
            assert!(
                deferred
                    .iter()
                    .any(|d| matches!(d, Deferred::OpenMessageBox { .. })),
                "the validator's error box was queued as Deferred::OpenMessageBox"
            );
        }

        /// valid_end drives "Save?" inline and returns true when user picks No (discard).
        #[test]
        fn valid_end_quit_no_discards_and_allows_quit() {
            let (mut program, _h, _c) = program_with_desktop(80, 25);
            let (_win_id, _ed) = modified_edit_window(&mut program, Some(tmp("quit_no")));

            // Pre-queue cmNo — the inline message box will consume it.
            program.out_events.push_back(Event::Command(Command::NO));
            let valid = program.valid_end(Command::QUIT);
            assert!(valid, "No → discard changes → allow quit");
            assert!(
                !program
                    .deferred
                    .iter()
                    .any(|d| matches!(d, Deferred::OpenMessageBox { .. })),
                "no OpenMessageBox leaks after inline drive"
            );
        }

        /// valid_end drives "Save?" inline and returns false when user picks Cancel.
        #[test]
        fn valid_end_quit_cancel_vetoes_quit() {
            let (mut program, _h, _c) = program_with_desktop(80, 25);
            let (_win_id, _ed) = modified_edit_window(&mut program, Some(tmp("quit_cancel")));

            // Pre-queue cmCancel — the inline message box will consume it.
            program
                .out_events
                .push_back(Event::Command(Command::CANCEL));
            let valid = program.valid_end(Command::QUIT);
            assert!(!valid, "Cancel → veto quit");
            assert!(
                !program
                    .deferred
                    .iter()
                    .any(|d| matches!(d, Deferred::OpenMessageBox { .. })),
                "no OpenMessageBox leaks after inline drive"
            );
        }

        /// valid_end drives "Save?" inline and returns true when user picks Yes (named file saves).
        #[test]
        fn valid_end_quit_yes_saves_and_allows_quit() {
            let (mut program, _h, _c) = program_with_desktop(80, 25);
            let path = tmp("quit_yes");
            let _ = std::fs::remove_file(&path);
            let (_win_id, _ed) = modified_edit_window(&mut program, Some(path.clone()));

            // Pre-queue cmYes — the inline message box will consume it.
            program.out_events.push_back(Event::Command(Command::YES));
            let valid = program.valid_end(Command::QUIT);
            assert!(valid, "Yes → save succeeds → allow quit");
            assert!(path.exists(), "Yes → file written to disk");
            let _ = std::fs::remove_file(&path);
        }

        /// valid_end with an untitled modified editor: drives "Save untitled file?"
        /// inline, user picks Yes → triggers OpenSaveAsDialog inline → user Cancels
        /// the FileDialog → close is vetoed → valid_end returns false.
        ///
        /// This exercises the OpenSaveAsDialog arm of valid_end (the most novel
        /// branch). The happy-path (Yes + save) is covered by
        /// `validate_modal_close`'s existing SaveAs tests.
        #[test]
        fn valid_end_quit_yes_untitled_cancel_saveas_vetoes_quit() {
            let (mut program, _h, _c) = program_with_desktop(80, 25);
            let (_win_id, _ed) = modified_edit_window(&mut program, None); // untitled

            // cmYes answers "Save untitled file?" → save() → OpenSaveAsDialog inline.
            // cmCancel answers the FileDialog → drive_save_as_inline returns false
            // → revalidate stays false → valid_end returns false (veto).
            program.out_events.push_back(Event::Command(Command::YES));
            program
                .out_events
                .push_back(Event::Command(Command::CANCEL));
            let valid = program.valid_end(Command::QUIT);
            assert!(!valid, "Cancel on SaveAs → veto quit");
            assert!(
                !program
                    .deferred
                    .iter()
                    .any(|d| matches!(d, Deferred::OpenSaveAsDialog { .. })),
                "no OpenSaveAsDialog leaks after inline drive"
            );
        }
    }

    // -- SplitterDivider deferred broker ------------------------------------
    //
    // Proves the D3 sibling-broker arm in the deferred drain:
    // `Deferred::SplitterDivider { splitter, op: DividerOp::Nudge }` reaches
    // the splitter via `find_mut → as_any_mut → downcast_mut::<Splitter>` and
    // calls `nudge_divider`, changing the first pane's bounds.

    /// Build a `Program` whose desktop holds a window whose body is a 3-pane
    /// cols `Splitter`. Returns the program and `(splitter_id, pane0_id)`.
    ///
    /// The window is 32-wide × 10-tall at (2, 2). The splitter body fills
    /// the window's interior (30 × 8 after the 1-cell frame), so its content
    /// length is 30: three equal panes of ≈9 cells each, with 2 divider gaps.
    fn program_with_splitter_window(
        w: u16,
        h: u16,
    ) -> (Program, crate::view::ViewId, crate::view::ViewId) {
        use crate::widgets::{Constraints, Splitter};

        let ids: Rc<RefCell<Option<(crate::view::ViewId, crate::view::ViewId)>>> =
            Rc::new(RefCell::new(None));
        let ids_cap = ids.clone();

        let (backend, _handle) = HeadlessBackend::new(w, h);
        let theme = Theme::classic_blue();
        let clock = Rc::new(ManualClock::new(0));
        let mut program = Program::new(
            Box::new(backend),
            Box::new(clock),
            theme,
            move |r| {
                let mut desktop = Desktop::new(r, |r2| Some(Desktop::init_background(r2)));
                let mut win = Window::new(Rect::new(2, 2, 34, 12), Some("S".into()), 1);

                // 3-pane cols splitter; window interior is 30×8 after frame.
                let mut sp = Splitter::cols();
                // Probe views as pane content (just need any view).
                let p0 = sp.insert(
                    Box::new(Probe::new(
                        Rect::new(0, 0, 0, 0),
                        'A',
                        Rc::new(RefCell::new(Vec::new())),
                    )),
                    Constraints::flex(),
                );
                sp.insert(
                    Box::new(Probe::new(
                        Rect::new(0, 0, 0, 0),
                        'B',
                        Rc::new(RefCell::new(Vec::new())),
                    )),
                    Constraints::flex(),
                );
                sp.insert(
                    Box::new(Probe::new(
                        Rect::new(0, 0, 0, 0),
                        'C',
                        Rc::new(RefCell::new(Vec::new())),
                    )),
                    Constraints::flex(),
                );

                let sp_id = win.insert_child(Box::new(sp));
                // Size the splitter to the window interior (1-cell frame inset on
                // each side), so `content_len` > 0 and pane bounds are non-zero.
                let ext = win.state().get_extent();
                let interior = Rect::new(1, 1, ext.b.x - 1, ext.b.y - 1);
                if let Some(v) = win.child_mut(sp_id) {
                    v.change_bounds(interior);
                }
                desktop.insert_view(Box::new(win));
                *ids_cap.borrow_mut() = Some((sp_id, p0));
                Some(Box::new(desktop))
            },
            |_r| None,
            |_r| None,
        );
        program.out_events.clear();

        let (sp_id, p0) = ids.borrow().expect("splitter inserted");
        (program, sp_id, p0)
    }

    /// Return the right edge (`bounds.b.x`) of the view named by `id`.
    fn pane_right(program: &mut Program, id: crate::view::ViewId) -> i32 {
        program
            .group_mut()
            .find_mut(id)
            .map(|v| v.state().get_bounds().b.x)
            .expect("pane resolves")
    }

    /// The `SplitterDivider` broker arm must resolve the splitter, downcast, and
    /// call `nudge_divider`, shifting the first pane's right boundary by `delta`.
    #[test]
    fn splitter_divider_broker_nudge_changes_pane_bounds() {
        use crate::view::DividerOp;
        use crate::widgets::Splitter;

        let (mut program, sp_id, p0) = program_with_splitter_window(80, 25);

        // Begin a resize session to mirror the production call sequence (the
        // window capture always does this before queuing a Nudge); it also
        // populates saved_weights so cancel-path tests work.
        program
            .group_mut()
            .find_mut(sp_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<Splitter>())
            .expect("splitter resolves")
            .begin_resize_session();

        // Record the first pane's right boundary before brokering the nudge.
        let before = pane_right(&mut program, p0);

        // Push the broker op directly onto the deferred queue (the same path the
        // window resize capture will use). The deferred drain runs only when the
        // pump picks an event, so inject a dummy broadcast that nobody handles
        // (it just gets dispatched and cleared, allowing the drain to run).
        program.deferred.push(Deferred::SplitterDivider {
            splitter: sp_id,
            op: DividerOp::Nudge { index: 0, delta: 1 },
        });
        program.out_events.push_back(noop_broadcast());
        program.pump_once();

        let after = pane_right(&mut program, p0);
        assert_eq!(
            after,
            before + 1,
            "SplitterDivider broker must call nudge_divider, shifting pane[0] right by 1"
        );
    }

    /// The broker arm is a no-op when the splitter id cannot be resolved
    /// (already closed or never inserted) — must not panic.
    #[test]
    fn splitter_divider_broker_missing_id_is_no_op() {
        use crate::view::DividerOp;

        let (mut program, _h, _c) = program_with_desktop(48, 14);
        // Allocate a fresh ViewId then immediately discard the owning view; the id
        // will never appear in the program's tree.
        let phantom = crate::view::ViewId::next();
        program.deferred.push(Deferred::SplitterDivider {
            splitter: phantom,
            op: DividerOp::Nudge { index: 0, delta: 1 },
        });
        program.out_events.push_back(noop_broadcast());
        // Must not panic; the pump simply skips the missing id.
        program.pump_once();
    }

    /// `exec_view_with` returns the closure's value BY VALUE, and the closure sees
    /// the modal's end command. OK → the extracted value; Cancel → the cancel value.
    /// Proves the by-value channel: the closure's return crosses back by value.
    #[test]
    fn exec_view_with_returns_extract_value_by_command() {
        use crate::dialog::Dialog;
        let (mut program, _handle, _clock) = program_with_desktop(80, 30);

        // OK path.
        let d_ok = Dialog::new(crate::view::Rect::new(0, 0, 20, 6), Some("t".to_string()));
        program.out_events.push_back(Event::Command(Command::OK));
        let ok: &str = program.exec_view_with(Box::new(d_ok), |_modal, cmd| {
            if cmd == Command::OK { "ok" } else { "other" }
        });
        assert_eq!(
            ok, "ok",
            "extract must see cmOK and its return is handed back by value"
        );
        assert_eq!(program.capture_len(), 0, "ModalFrame popped on close");

        // Cancel path.
        let d_cancel = Dialog::new(crate::view::Rect::new(0, 0, 20, 6), Some("t".to_string()));
        program
            .out_events
            .push_back(Event::Command(Command::CANCEL));
        let cancelled: &str = program.exec_view_with(Box::new(d_cancel), |_modal, cmd| {
            if cmd == Command::OK { "ok" } else { "other" }
        });
        assert_eq!(cancelled, "other", "extract must see cmCancel");
    }

    // ---------------------------------------------------------------------------
    // shell_msg hook tests
    // ---------------------------------------------------------------------------

    /// With no hook set, `shell_msg()` returns the platform default message.
    #[test]
    fn shell_msg_default_returns_platform_string() {
        let (program, _handle, _clock) = program_with_desktop(80, 25);
        let msg = program.shell_msg();
        assert_eq!(
            msg,
            default_shell_msg(),
            "no-hook path must equal default_shell_msg()"
        );
        // Spot-check the unix text on unix targets so the actual string is exercised.
        #[cfg(unix)]
        assert!(
            msg.contains("fg"),
            "unix default must mention 'fg': {msg:?}"
        );
    }

    /// With a hook registered, `shell_msg()` returns the hook's string.
    #[test]
    fn shell_msg_hook_overrides_default() {
        let (mut program, _handle, _clock) = program_with_desktop(80, 25);
        program.set_shell_msg_hook(Box::new(|| "custom message".to_string()));
        assert_eq!(program.shell_msg(), "custom message");
    }

    /// The hook is called each time (not just once); replacing it again works.
    #[test]
    fn shell_msg_hook_replaceable() {
        let (mut program, _handle, _clock) = program_with_desktop(80, 25);
        program.set_shell_msg_hook(Box::new(|| "first".to_string()));
        assert_eq!(program.shell_msg(), "first");
        program.set_shell_msg_hook(Box::new(|| "second".to_string()));
        assert_eq!(program.shell_msg(), "second");
    }

    #[test]
    fn on_idle_fires_each_idle_pass() {
        use std::cell::Cell;
        use std::rc::Rc;

        let (mut program, _handle, _clock) = program_with_desktop(80, 25);

        // Settle startup: the first idle pass queues a COMMAND_SET_CHANGED
        // broadcast (command_set_changed == true at construction), and the next
        // pass dispatches it — a non-idle pass. Drain both so the loop below sees
        // only genuinely event-less passes.
        program.pump_and_drive(); // idle: emits the startup broadcast
        program.pump_and_drive(); // non-idle: dispatches it

        let ticks = Rc::new(Cell::new(0u32));
        let ticks_in = ticks.clone();
        program.set_on_idle(move |_p| {
            ticks_in.set(ticks_in.get() + 1);
        });

        // No events queued -> every pump pass is now idle. Drive a few passes.
        for _ in 0..3 {
            program.pump_and_drive();
        }

        assert_eq!(
            ticks.get(),
            3,
            "idle hook should fire once on each event-less pass, got {}",
            ticks.get()
        );
    }
}
