//! `external_state` — driving the UI from an external / async data source.
//!
//! This example demonstrates the canonical pattern for feeding data produced
//! outside the tvision-rs event loop (e.g. a background thread, a network
//! connection, or an async runtime) into the TUI without unsafe code or extra
//! crate dependencies.
//!
//! # Pattern overview
//!
//! ```text
//!   ┌──────────────────────────────┐       mpsc channel
//!   │  background thread           │  ──────────────────►  PumpView
//!   │  (sends Strings every 500ms) │                         │ on Event::Timer
//!   └──────────────────────────────┘                         │ drain try_recv
//!                                                            │ update AppState
//!                                                            │ broadcast REFRESH
//!                                                            ▼
//!                                                        ListPane
//!                                                        (rebuilds on REFRESH)
//! ```
//!
//! ## Key components
//!
//! - **`AppState`** — shared behind `Rc<RefCell<AppState>>` on the main thread.
//!   Holds the list of lines produced by the background thread.
//! - **`PumpView`** — a zero-area (invisible) view inserted into the window.
//!   On its first event it arms a periodic timer with [`Context::set_timer`].
//!   On every [`Event::Timer`] it drains the [`std::sync::mpsc::Receiver`],
//!   appends new lines to `AppState`, and — if anything arrived —
//!   [`Context::broadcast`]s `REFRESH` to notify the list pane.
//! - **`ListPane`** — a thin wrapper around [`ListBox`] that rebuilds its
//!   contents whenever it sees the `REFRESH` broadcast.
//!
//! ## Borrow discipline
//!
//! `Rc<RefCell<T>>` borrows are always dropped before calls that may
//! re-enter (`new_list`, `broadcast`).  Violating this rule causes a
//! `BorrowError` panic at runtime.
//!
//! ## Running the example
//!
//! ```text
//! cargo run --example external_state
//! ```
//!
//! Press `Alt-X` or select File → Exit to quit.

use std::cell::RefCell;
use std::io;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tvision_rs::{
    Backend, Command, Context, CrosstermBackend, Desktop, DrawCtx, Event, ListBox, Menu, MenuBar,
    Program, Rect, StatusDef, StatusLine, SystemClock, Theme, View, ViewState, Window, alt,
    delegate,
};

// ---------------------------------------------------------------------------
// Application-level broadcast command
// ---------------------------------------------------------------------------

/// Sent by `PumpView` whenever new data arrives from the background thread.
/// All views that display the shared state react to this broadcast and repaint.
const REFRESH: Command = Command::custom("external_state.refresh");

// ---------------------------------------------------------------------------
// Shared application state
// ---------------------------------------------------------------------------

/// All mutable state owned by the main thread.
struct AppState {
    /// Lines accumulated from the background thread.
    lines: Vec<String>,
}

impl AppState {
    fn new() -> Self {
        AppState { lines: Vec::new() }
    }
}

/// The `Rc<RefCell<AppState>>` handle passed to every view that needs state.
type Shared = Rc<RefCell<AppState>>;

// ---------------------------------------------------------------------------
// Background data source
// ---------------------------------------------------------------------------

/// Spawn a thread that sends an incrementing string every ~500 ms.
/// Returns the `Receiver` end; the `Sender` is moved into the thread.
fn spawn_data_source() -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        let mut counter: u64 = 0;
        loop {
            counter += 1;
            // Ignore errors: if the receiver is gone the loop exits cleanly.
            if tx
                .send(format!("line {counter:04} — tick from background thread"))
                .is_err()
            {
                break;
            }
            thread::sleep(Duration::from_millis(500));
        }
    });
    rx
}

// ---------------------------------------------------------------------------
// PumpView — zero-area, invisible, owns the Receiver and the periodic timer
// ---------------------------------------------------------------------------

/// A zero-area view that drains the mpsc channel on each timer tick.
///
/// - Never drawn (zero area, no-op `draw`).
/// - Receives `Event::Timer` because timer events are broadcast-class in
///   tvision-rs: they are delivered to every view in the tree, including
///   zero-area ones.
/// - Arms a periodic timer on the first `handle_event` call (the constructor
///   has no `Context`, so we defer arming).
/// - Drops the `RefCell` borrow before calling `ctx.broadcast` to satisfy
///   borrow discipline.
struct PumpView {
    vs: ViewState,
    state: Shared,
    rx: mpsc::Receiver<String>,
    armed: bool,
}

impl PumpView {
    fn new(state: Shared, rx: mpsc::Receiver<String>) -> Self {
        PumpView {
            vs: ViewState::new(Rect::new(0, 0, 0, 0)),
            state,
            rx,
            armed: false,
        }
    }
}

impl View for PumpView {
    fn state(&self) -> &ViewState {
        &self.vs
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.vs
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn draw(&mut self, _ctx: &mut DrawCtx) {
        // Zero-area view — nothing to draw.
    }

    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        // Arm the periodic timer the first time we have a Context in hand.
        if !self.armed {
            self.armed = true;
            // ~10 Hz poll — frequent enough to feel responsive without
            // saturating the event loop.
            ctx.set_timer(Duration::from_millis(100), Some(Duration::from_millis(100)));
        }

        if matches!(ev, Event::Timer(_)) {
            // Drain the channel; collect all ready messages without blocking.
            let mut new_data = false;
            loop {
                match self.rx.try_recv() {
                    Ok(line) => {
                        // Borrow, append, drop borrow — all before broadcast.
                        self.state.borrow_mut().lines.push(line);
                        new_data = true;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => break,
                }
            }
            // RefCell borrow is fully dropped here before broadcast.
            if new_data {
                ctx.broadcast(REFRESH, None);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ListPane — a ListBox that (re)populates from shared state on REFRESH
// ---------------------------------------------------------------------------

/// A `ListBox` wrapper that rebuilds its item list on every `REFRESH` broadcast.
///
/// The `new_list` call requires a `&mut Context`, which is not available in the
/// constructor — so we populate on the first event (seed-once) and again on
/// every `REFRESH`. The `#[delegate(to = list)]` macro forwards all remaining
/// `View` methods to the inner `ListBox`.
struct ListPane {
    list: ListBox,
    state: Shared,
    seeded: bool,
}

impl ListPane {
    fn new(bounds: Rect, state: Shared) -> Self {
        ListPane {
            list: ListBox::new(bounds, 1, None, None),
            state,
            seeded: false,
        }
    }
}

#[delegate(to = list)]
impl View for ListPane {
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        let is_refresh = matches!(ev, Event::Broadcast { command, .. } if *command == REFRESH);

        if !self.seeded || is_refresh {
            self.seeded = true;
            // Collect lines into a local Vec while NOT holding the RefCell borrow
            // — new_list needs &mut self (via list) and must not find the borrow live.
            let lines: Vec<String> = self.state.borrow().lines.clone();
            self.list.new_list(lines, ctx);
            // borrow dropped before new_list is called (clone already finished).
        }

        self.list.handle_event(ev, ctx);
    }
}

// ---------------------------------------------------------------------------
// ExternalStateApp — the application wrapper
// ---------------------------------------------------------------------------

struct ExternalStateApp {
    program: Program,
}

impl ExternalStateApp {
    fn new(backend: Box<dyn Backend>, state: Shared, rx: mpsc::Receiver<String>) -> Self {
        // Capture state and rx into the desktop closure; the other factories are
        // plain fn pointers.
        let program = Program::new(
            backend,
            Box::new(SystemClock::new()),
            Theme::classic_blue(),
            move |r| Self::init_desktop(r, state.clone(), rx),
            Self::init_status_line,
            Self::init_menu_bar,
        );
        ExternalStateApp { program }
    }

    fn init_desktop(r: Rect, state: Shared, rx: mpsc::Receiver<String>) -> Option<Box<dyn View>> {
        let mut r = r;
        r.a.y += 1; // below the menu bar
        r.b.y -= 1; // above the status line

        let mut desktop = Desktop::new(r, |br| Some(Desktop::init_background(br)));

        // A window that fills most of the desktop.
        let win_rect = Rect::new(r.a.x + 2, r.a.y + 1, r.b.x - 2, r.b.y - 1);
        let mut win = Window::new(
            win_rect,
            Some("External State — background thread feed".to_string()),
            1,
        );

        // Interior in LOCAL coords, inset by one cell on each side for the frame.
        let ext = win.state().get_extent();
        let interior = Rect::new(1, 1, ext.b.x - 1, ext.b.y - 1);

        // The visible list pane — rebuilt from AppState on every REFRESH.
        win.insert_child(Box::new(ListPane::new(interior, state.clone())));

        // The pump view — zero-area, owns the Receiver, drives the timer cycle.
        win.insert_child(Box::new(PumpView::new(state, rx)));

        desktop.insert_view(Box::new(win));
        Some(Box::new(desktop))
    }

    fn init_status_line(r: Rect) -> Option<Box<dyn View>> {
        let mut r = r;
        r.a.y = r.b.y - 1;
        let defs = StatusDef::list()
            .def_all(|d| d.item("~Alt-X~ Exit", alt('x'), Command::QUIT))
            .build();
        Some(Box::new(StatusLine::new(r, defs)))
    }

    fn init_menu_bar(r: Rect) -> Option<Box<dyn View>> {
        let mut r = r;
        r.b.y = r.a.y + 1;
        let menu = Menu::builder()
            .submenu("~F~ile", alt('f'), |m| {
                m.command_key("E~x~it", Command::QUIT, alt('x'), "Alt-X")
            })
            .build();
        Some(Box::new(MenuBar::new(r, menu)))
    }

    fn run(&mut self) -> Command {
        self.program.run_app(|_prog, _cmd| {})
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() -> io::Result<()> {
    let state: Shared = Rc::new(RefCell::new(AppState::new()));
    let rx = spawn_data_source();
    let mut app = ExternalStateApp::new(Box::new(CrosstermBackend::new()?), state, rx);
    let _result: Command = app.run();
    Ok(())
}
