//! [`Application`], the thin application wrapper over [`Program`].
//!
//! `Application` adds the application-level window commands on top of [`Program`]:
//! `cmTile` / `cmCascade` tile or cascade the desktop's windows (handled in the
//! program's command dispatch, laying windows into [`get_tile_rect`]'s rectangle),
//! and `cmDosShell` suspends the terminal. The C++ subsystem init/teardown
//! (`TAppInit`) is subsumed by the [`Backend`](crate::backend::Backend) +
//! [`Renderer`](crate::backend::Renderer) construction path, and C++'s
//! `initHistory`/`doneHistory` are moot here: the history store is a
//! `thread_local!` `Vec` that auto-initializes and auto-drops (see `history.rs`).
//! The module is thin: one [`get_tile_rect`] helper plus forwarding delegations
//! to the embedded program.
//!
//! [`get_tile_rect`]: Application::get_tile_rect
//!
//! # Turbo Vision heritage
//! Ports `TApplication` (`tapplica.cpp`). C++ `TApplication : TProgram`
//! inheritance becomes embed-and-delegate composition (deviation D2): the type
//! holds a [`Program`] and forwards to it.

use crate::app::Program;
use crate::backend::Backend;
use crate::command::Command;
use crate::theme::Theme;
use crate::timer::Clock;
use crate::view::{Rect, View, ViewId};

/// A thin embed-and-delegate wrapper over [`Program`].
///
/// `Application` provides [`Application::get_tile_rect`] (the rectangle tile and
/// cascade lay windows into) and forwards all other behavior verbatim to the
/// embedded [`Program`].
///
/// Build with [`Application::new`]; drive with [`Application::run`] or step with
/// [`Application::pump_once`].
///
/// # Turbo Vision heritage
/// Ports `TApplication` (`tapplica.cpp`); its ctor is the factory mixin
/// `TProgInit(initStatusLine, initMenuBar, initDeskTop)`. C++ inheritance from
/// `TProgram` becomes embed-and-delegate composition (deviation D2).
/// `initHistory`/`doneHistory` are moot — the store is a `thread_local!` `Vec`
/// that auto-initializes and auto-drops (see `history.rs`).
pub struct Application {
    /// The embedded program. `Application` forwards every public operation
    /// through this field — see the forwarding methods below.
    program: Program,
}

impl Application {
    /// `TApplication::TApplication` — construct the application.
    ///
    /// Ports the C++ factory-mixin ctor faithfully: forwards `create_desktop`,
    /// `create_status_line`, and `create_menu_bar` to [`Program::new`] unchanged.
    /// `TAppInit` (hardware/mouse/screen subsystem init) is subsumed by our backend
    /// construction path; no equivalent is needed here.
    pub fn new(
        backend: Box<dyn Backend>,
        clock: Box<dyn Clock>,
        theme: Theme,
        create_desktop: impl FnOnce(Rect) -> Option<Box<dyn View>>,
        create_status_line: impl FnOnce(Rect) -> Option<Box<dyn View>>,
        create_menu_bar: impl FnOnce(Rect) -> Option<Box<dyn View>>,
    ) -> Self {
        Application {
            program: Program::new(
                backend,
                clock,
                theme,
                create_desktop,
                create_status_line,
                create_menu_bar,
            ),
        }
    }

    // -- Delegations to Program (one-line forwards) --------------------------

    /// Run the event loop — delegates to [`Program::run`].
    pub fn run(&mut self) -> Command {
        self.program.run()
    }

    /// One iteration of the event loop — delegates to [`Program::pump_once`].
    pub fn pump_once(&mut self) {
        self.program.pump_once();
    }

    /// Run `view` as a modal dialog — delegates to [`Program::exec_view`].
    pub fn exec_view(&mut self, view: Box<dyn View>) -> Command {
        self.program.exec_view(view)
    }

    /// The desktop child's id — delegates to [`Program::desktop`].
    pub fn desktop(&self) -> Option<ViewId> {
        self.program.desktop()
    }

    /// Request the modal loop end — delegates to [`Program::end_modal`].
    pub fn end_modal(&mut self, cmd: Command) {
        self.program.end_modal(cmd);
    }

    /// The current modal end state — delegates to [`Program::end_state`].
    pub fn end_state(&self) -> Option<Command> {
        self.program.end_state()
    }

    /// Enable a command — delegates to [`Program::enable_command`].
    pub fn enable_command(&mut self, cmd: Command) {
        self.program.enable_command(cmd);
    }

    /// Disable a command — delegates to [`Program::disable_command`].
    pub fn disable_command(&mut self, cmd: Command) {
        self.program.disable_command(cmd);
    }

    /// Whether a command is currently enabled — delegates to
    /// [`Program::command_enabled`].
    pub fn command_enabled(&self, cmd: Command) -> bool {
        self.program.command_enabled(cmd)
    }

    // -- Escape hatches ------------------------------------------------------

    /// Shared borrow of the embedded [`Program`].
    pub fn program(&self) -> &Program {
        &self.program
    }

    /// Exclusive borrow of the embedded [`Program`].
    pub fn program_mut(&mut self) -> &mut Program {
        &mut self.program
    }

    // -- TApplication::getTileRect -------------------------------------------

    /// `TApplication::getTileRect` — the rectangle tile/cascade lay windows into:
    /// the **desktop child's extent** (`(0,0,w,h)` in desktop-local coords), so it
    /// stays correct when the desktop is inset under a menu/status bar.
    /// Returns `None` if no desktop was created.
    ///
    /// Requires `&mut self` because the underlying `Group::find_mut` requires
    /// `&mut`, preferred over adding a `&self` resolver to `group.rs`.
    pub fn get_tile_rect(&mut self) -> Option<Rect> {
        self.program.get_tile_rect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::HeadlessBackend;
    use crate::desktop::Desktop;
    use crate::event::Event;
    use crate::timer::ManualClock;
    use std::rc::Rc;

    /// Build an `Application` with a real `Desktop` and stubbed status-line/menu-bar,
    /// over a headless backend. Mirrors the `program_with_desktop` harness in
    /// `program.rs`.
    fn application_with_desktop(w: u16, h: u16) -> Application {
        let (backend, _handle) = HeadlessBackend::new(w, h);
        let theme = Theme::classic_blue();
        let clock = Rc::new(ManualClock::new(0));
        Application::new(
            Box::new(backend),
            Box::new(clock),
            theme,
            |r| {
                Some(Box::new(Desktop::new(r, |r2| {
                    Some(Desktop::init_background(r2))
                })))
            },
            |_r| None, // status line stubbed
            |_r| None, // menu bar stubbed
        )
    }

    /// `get_tile_rect` returns the **desktop child's** extent in desktop-local coords
    /// (`(0,0,dw,dh)`), NOT the screen extent. The desktop is created with an inset
    /// rect (`(0,0,80,20)`) inside an 80×25 backend, so any code that returned the
    /// screen rect would produce `(0,0,80,25)` and fail this assertion.
    #[test]
    fn get_tile_rect_returns_desktop_extent() {
        // Backend is 80×25; desktop is inset to 80×20 (a menu/status bar shrinks
        // it further — this exercises that property).
        let (backend, _handle) = HeadlessBackend::new(80, 25);
        let theme = Theme::classic_blue();
        let clock = Rc::new(ManualClock::new(0));
        let mut app = Application::new(
            Box::new(backend),
            Box::new(clock),
            theme,
            |_r| {
                // Ignore the full-screen `r`; create the desktop with an inset rect
                // so the test can distinguish desktop-extent from screen-extent.
                Some(Box::new(Desktop::new(Rect::new(0, 0, 80, 20), |r2| {
                    Some(Desktop::init_background(r2))
                })))
            },
            |_r| None,
            |_r| None,
        );
        let rect = app.get_tile_rect();
        assert_eq!(
            rect,
            Some(Rect::new(0, 0, 80, 20)),
            "get_tile_rect must return the desktop's local-origin extent, not the screen rect"
        );
    }

    /// `get_tile_rect` returns `None` when no desktop was created.
    #[test]
    fn get_tile_rect_none_without_desktop() {
        let (backend, _handle) = HeadlessBackend::new(40, 12);
        let theme = Theme::classic_blue();
        let clock = Rc::new(ManualClock::new(0));
        let mut app = Application::new(
            Box::new(backend),
            Box::new(clock),
            theme,
            |_r| None, // no desktop
            |_r| None,
            |_r| None,
        );
        assert_eq!(app.get_tile_rect(), None);
    }

    /// Delegation smoke test: `enable_command` + `command_enabled` round-trips
    /// through `Application`, and `desktop()` returns `Some` when a desktop exists.
    #[test]
    fn enable_command_and_desktop_delegation() {
        let mut app = application_with_desktop(80, 25);
        assert!(
            app.desktop().is_some(),
            "desktop() must return Some after construction with a desktop factory"
        );
        // CLOSE starts disabled (per Program's initial_disabled_commands seed);
        // enable it and verify the forwarding path.
        assert!(
            !app.command_enabled(Command::CLOSE),
            "cmClose starts disabled"
        );
        app.enable_command(Command::CLOSE);
        assert!(
            app.command_enabled(Command::CLOSE),
            "enable_command forwarded: cmClose is now enabled"
        );
        app.disable_command(Command::CLOSE);
        assert!(
            !app.command_enabled(Command::CLOSE),
            "disable_command forwarded: cmClose disabled again"
        );
    }

    /// `Event::Command(Command::DOS_SHELL)` is consumed cleanly — no crash,
    /// `end_state` stays `None`. HeadlessBackend::suspend/resume are no-ops;
    /// `raise(SIGTSTP)` is `#[cfg(all(unix, not(test)))]`-gated (no suspension).
    #[test]
    fn dos_shell_consumed_cleanly() {
        let (backend, handle) = HeadlessBackend::new(80, 25);
        let theme = Theme::classic_blue();
        let clock = Rc::new(ManualClock::new(0));
        let mut app = Application::new(
            Box::new(backend),
            Box::new(clock),
            theme,
            |r| {
                Some(Box::new(Desktop::new(r, |r2| {
                    Some(Desktop::init_background(r2))
                })))
            },
            |_r| None,
            |_r| None,
        );
        handle.push_event(Event::Command(Command::DOS_SHELL));
        app.pump_once();
        assert_eq!(app.end_state(), None, "DOS_SHELL must not set end_state");
    }
}
