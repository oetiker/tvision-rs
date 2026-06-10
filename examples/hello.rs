//! `hello` — a minimal Turbo Vision application, written in the shape a
//! magiblot/tvision (C++) veteran will recognise on sight: a desktop with a
//! patterned background and a few windows, a menu bar at the top, and a status
//! line at the bottom, all driven by the real `TProgram::run` event loop.
//!
//! The C++ skeleton this mirrors:
//!
//! ```cpp
//! class HelloApp : public TApplication {
//!     static TDeskTop    *initDeskTop(TRect);
//!     static TStatusLine *initStatusLine(TRect);
//!     static TMenuBar    *initMenuBar(TRect);
//! };
//!
//! int main() {
//!     HelloApp app;
//!     app.run();          // spins the event loop until cmQuit
//! }
//! ```
//!
//! Run it:  `cargo run --example hello`
//!   - `F10` enters the menu bar; arrows + Enter navigate it.
//!   - `Alt-F`/`Alt-W` open the File / Window menus by hot-key.
//!   - `Alt-X` (or File → Exit) quits.
//!   - `F3` or File → Open opens a file in an editor window.
//!   - `F4` or File → New opens an untitled editor window.
//!   - `F2` or File → Save saves the current editor file.
//!   - `F5` zooms the current window, `Alt-F3` closes it, `F6` cycles.
//!   - `Alt-1`..`Alt-9` select a window by number.
//!   - `Color → Color Picker…` opens the truecolor picker.

use std::io;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use tvision::{
    Backend, Color, Command, CrosstermBackend, Desktop, EditWindow, Key, KeyEvent, Menu, MenuBar,
    Program, Rect, StatusDef, StatusLine, SystemClock, Theme, View, Window, alt,
};

const CMD_COLOR_PICKER: Command = Command::custom("hello.color_picker");
const CMD_NEW: Command = Command::custom("hello.new");
const CMD_OPEN: Command = Command::custom("hello.open");

// ---------------------------------------------------------------------------
// HelloApp : public TApplication
// ---------------------------------------------------------------------------

/// The application class. In C++ this would derive `TApplication` and override
/// the three `init*` factories; here it wraps the [`Program`] it builds from
/// them via [`Program::new`] (the port's `TProgInit` factory-mixin seam).
struct HelloApp {
    program: Program,
}

impl HelloApp {
    /// `HelloApp::HelloApp` → `TProgInit(initStatusLine, initMenuBar, initDeskTop)`.
    fn new(backend: Box<dyn Backend>) -> Self {
        let mut program = Program::new(
            backend,
            Box::new(SystemClock::new()),
            Theme::classic_blue(),
            Self::init_desktop,
            Self::init_status_line,
            Self::init_menu_bar,
        );
        program.enable_command(CMD_COLOR_PICKER);
        program.enable_command(CMD_NEW);
        program.enable_command(CMD_OPEN);
        HelloApp { program }
    }

    /// `TApplication::initDeskTop` — `r.a.y++; r.b.y--` to inset the desktop one
    /// row below the menu bar and one row above the status line, the patterned
    /// background (`TDeskTop::initBackground`), then a few demo windows so the
    /// window-management commands (zoom / close / next) have something to act on.
    fn init_desktop(r: Rect) -> Option<Box<dyn View>> {
        let mut r = r;
        r.a.y += 1; // below the menu bar
        r.b.y -= 1; // above the status line
        let mut desktop = Desktop::new(r, |br| Some(Desktop::init_background(br)));
        // Insert three staggered demo windows numbered 1..=3 (Alt-1..Alt-3 select
        // them; the desktop's cmNext/cmPrev cycle them).
        for num in 1..=3i16 {
            let x = 4 + (num as i32) * 4;
            let y = 1 + (num as i32) * 2;
            let mut win = Window::new(
                Rect::new(x, y, x + 28, y + 8),
                Some(format!("Window {num}")),
                num,
            );
            // TWindow does NOT set ofTileable; the app opts its windows in so
            // Window → Tile / Cascade lay them out.
            win.state_mut().options.tileable = true;
            desktop.insert_view(Box::new(win));
        }
        Some(Box::new(desktop))
    }

    /// `TApplication::initStatusLine` — `r.a.y = r.b.y - 1` (pin to the bottom
    /// row), then the standard line: labelled `Alt-X Exit` (cmQuit) and
    /// `F10 Menu` (cmMenu), plus hidden hotkey bindings for `Alt-F3` close,
    /// `F5` zoom, `F6` next — the shape `initStatusLine` builds.
    fn init_status_line(r: Rect) -> Option<Box<dyn View>> {
        let mut r = r;
        r.a.y = r.b.y - 1;
        let defs = StatusDef::list()
            .def_all(|d| {
                d.item("~F3~ Open", KeyEvent::from(Key::F(3)), CMD_OPEN)
                    .item("~F4~ New", KeyEvent::from(Key::F(4)), CMD_NEW)
                    .item("~F10~ Menu", KeyEvent::from(Key::F(10)), Command::MENU)
                    .item("~Alt-X~ Exit", alt('x'), Command::QUIT)
                    .key_item(alt_f3(), Command::CLOSE)
                    .key_item(KeyEvent::from(Key::F(5)), Command::ZOOM)
                    .key_item(KeyEvent::from(Key::F(6)), Command::NEXT)
            })
            .build();
        Some(Box::new(StatusLine::new(r, defs)))
    }

    /// `TApplication::initMenuBar` — `r.b.y = r.a.y + 1` (pin to the top row),
    /// then a File / Window menu. The Window menu now includes **Tile** and
    /// **Cascade** (cmTile/cmCascade route through `program_handle_event` to
    /// `Desktop::tile`/`cascade`); every item wires a command that routes.
    fn init_menu_bar(r: Rect) -> Option<Box<dyn View>> {
        let mut r = r;
        r.b.y = r.a.y + 1;
        let menu = Menu::builder()
            .submenu("~F~ile", alt('f'), |m| {
                m.command_key("~O~pen…", CMD_OPEN, KeyEvent::from(Key::F(3)), "F3")
                    .command_key("~N~ew", CMD_NEW, KeyEvent::from(Key::F(4)), "F4")
                    .separator()
                    .command_key("~S~ave", Command::SAVE, KeyEvent::from(Key::F(2)), "F2")
                    .command("Save ~A~s…", Command::SAVE_AS)
                    .separator()
                    .command_key("E~x~it", Command::QUIT, alt('x'), "Alt-X")
            })
            .submenu("~W~indow", alt('w'), |m| {
                m.command_key("~N~ext", Command::NEXT, KeyEvent::from(Key::F(6)), "F6")
                    .command_key("~Z~oom", Command::ZOOM, KeyEvent::from(Key::F(5)), "F5")
                    .command_key("~C~lose", Command::CLOSE, alt_f3(), "Alt-F3")
                    .command("~T~ile", Command::TILE)
                    .command("C~a~scade", Command::CASCADE)
            })
            .submenu("~C~olor", alt('c'), |m| {
                m.command("Color ~P~icker…", CMD_COLOR_PICKER)
            })
            .build();
        Some(Box::new(MenuBar::new(r, menu)))
    }

    /// `TApplication::run` — spin the real event loop until a `cmQuit` ends it.
    /// Handles application-level commands (the `TApplication::handleEvent` slot).
    fn run(&mut self) -> Command {
        let mut next_num: i16 = 4; // demo windows 1-3 are pre-inserted; start at 4
        self.program.run_app(move |prog, cmd| {
            if cmd == CMD_NEW {
                let r = prog.desktop_rect();
                let win = EditWindow::new(r, None, next_num);
                prog.desktop_insert(Box::new(win));
                next_num += 1;
            } else if cmd == CMD_OPEN {
                if let Some(path) = prog.open_file_dialog("Open a File", "*.*") {
                    let r = prog.desktop_rect();
                    let win = EditWindow::new(r, Some(path), next_num);
                    prog.desktop_insert(Box::new(win));
                    next_num += 1;
                }
            } else if cmd == CMD_COLOR_PICKER {
                let initial = Color::Rgb(30, 144, 255);
                if let Some(color) = prog.color_dialog(initial) {
                    eprintln!("Color picker returned: {color:?}");
                }
            }
        })
    }
}

/// `Alt-F3` — the classic "close window" accelerator (`kbAltF3`).
fn alt_f3() -> KeyEvent {
    use tvision::KeyModifiers;
    KeyEvent::new(
        Key::F(3),
        KeyModifiers {
            alt: true,
            ..Default::default()
        },
    )
}

// ---------------------------------------------------------------------------
// Terminal setup (deferred out of CrosstermBackend until a later row)
// ---------------------------------------------------------------------------

/// Undo the terminal setup. Idempotent and safe to call more than once (Drop +
/// the signal thread may both run).
fn restore_terminal() {
    let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
    let _ = disable_raw_mode();
}

/// RAII terminal guard: raw mode + alternate screen + mouse capture on entry,
/// restored on `Drop` — so a panic unwinding through `run` still restores the
/// terminal. It also installs a signal thread so a `kill` (SIGTERM), a hangup
/// (SIGHUP), or SIGINT restores the terminal before exiting — without it the
/// shell is left in raw mode on the alternate screen. (SIGKILL is uncatchable; a
/// `kill -9` will still leave the terminal dirty — run `reset` to recover.)
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;

        // Restore on fatal signals. We handle them on a dedicated thread (not in
        // an async-signal context), so calling into crossterm is sound. On the
        // first such signal we restore and exit (130 = 128 + SIGINT, the usual
        // shell convention); Drop does not run on `process::exit`, but we have
        // already restored.
        let mut signals = Signals::new([SIGINT, SIGTERM, SIGHUP])?;
        std::thread::spawn(move || {
            if signals.forever().next().is_some() {
                restore_terminal();
                std::process::exit(130);
            }
        });

        Ok(TerminalGuard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

// ---------------------------------------------------------------------------
// int main()
// ---------------------------------------------------------------------------

fn main() -> io::Result<()> {
    let _guard = TerminalGuard::enter()?;

    let mut app = HelloApp::new(Box::new(CrosstermBackend::new()));
    let _result: Command = app.run();

    Ok(())
}
