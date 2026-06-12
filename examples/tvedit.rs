//! `tvedit` — a faithful port of the magiblot/tvision `tvedit` example.
//!
//! This combines `tvedit1.cpp` (application logic), `tvedit2.cpp` (find/replace
//! dialogs — built into rstv's `Editor` via the `Deferred` seam), and
//! `tvedit3.cpp` (menu bar + status line).
//!
//! Run it:  `cargo run --example tvedit [file ...]`
//!   - `F3` / File → Open opens a file.
//!   - `Ctrl-N` / File → New opens an empty buffer.
//!   - `F2` / File → Save saves the current file.
//!   - File → Save as… renames and saves.
//!   - File → Change dir… changes the working directory.
//!   - File → DOS shell suspends to the terminal.
//!   - Edit → Undo/Cut/Copy/Paste/Clear and keyboard equivalents.
//!   - Search → Find/Replace/Search again (built-in editor dialogs).
//!   - Windows → Tile/Cascade/Next/Previous/Zoom/Size·Move/Close.
//!   - `Alt-X` or File → Exit quits.

use std::io;
use std::path::PathBuf;

use tvision::{
    Backend, CD_NORMAL, ChDirDialog, Command, CrosstermBackend, Desktop, EditWindow, Key, KeyEvent,
    Menu, MenuBar, Program, Rect, StatusDef, StatusLine, SystemClock, Theme, View, alt,
};

// ---------------------------------------------------------------------------
// Example-local command constants for keymap switching
// ---------------------------------------------------------------------------

const KEYMAP_WORDSTAR: Command = Command::custom("tvedit.keymap.wordstar");
const KEYMAP_CUA: Command = Command::custom("tvedit.keymap.cua");
const KEYMAP_EMACS: Command = Command::custom("tvedit.keymap.emacs");

// ---------------------------------------------------------------------------
// TEditorApp
// ---------------------------------------------------------------------------

struct TEditApp {
    program: Program,
}

impl TEditApp {
    fn new(backend: Box<dyn Backend>, files: Vec<PathBuf>) -> Self {
        let mut app = TEditApp {
            program: Program::new(
                backend,
                Box::new(SystemClock::new()),
                Theme::classic_blue(),
                Self::init_desktop,
                Self::init_status_line,
                Self::init_menu_bar,
            ),
        };
        // Open files specified on the command line (TEditorApp constructor body).
        let r = app.program.desktop_rect();
        for path in files {
            let win = EditWindow::new(r, Some(path), 0);
            app.program.desktop_insert(Box::new(win));
        }
        app
    }

    /// `TApplication::initDeskTop` — full-screen desktop with one row inset top
    /// (menu bar) and one row inset bottom (status line).
    fn init_desktop(r: Rect) -> Option<Box<dyn View>> {
        let mut r = r;
        r.a.y += 1;
        r.b.y -= 1;
        Some(Box::new(Desktop::new(r, |br| {
            Some(Desktop::init_background(br))
        })))
    }

    /// `TEditorApp::initStatusLine` — pins to the bottom row.
    fn init_status_line(r: Rect) -> Option<Box<dyn View>> {
        let mut r = r;
        r.a.y = r.b.y - 1;
        let defs = StatusDef::list()
            .def_all(|d| {
                d.key_item(alt('x'), Command::QUIT)
                    .item("~F2~ Save", KeyEvent::from(Key::F(2)), Command::SAVE)
                    .item("~F3~ Open", KeyEvent::from(Key::F(3)), Command::OPEN)
                    .item("~Ctrl-W~ Close", ctrl_w(), Command::CLOSE)
                    .item("~F5~ Zoom", KeyEvent::from(Key::F(5)), Command::ZOOM)
                    .item("~F6~ Next", KeyEvent::from(Key::F(6)), Command::NEXT)
                    .item("~F10~ Menu", KeyEvent::from(Key::F(10)), Command::MENU)
                    .key_item(shift_del(), Command::CUT)
                    .key_item(ctrl_ins(), Command::COPY)
                    .key_item(shift_ins(), Command::PASTE)
                    .key_item(ctrl_f5(), Command::RESIZE)
            })
            .build();
        Some(Box::new(StatusLine::new(r, defs)))
    }

    /// `TEditorApp::initMenuBar` — File / Edit / Search / Windows submenus.
    fn init_menu_bar(r: Rect) -> Option<Box<dyn View>> {
        let mut r = r;
        r.b.y = r.a.y + 1;
        let menu = Menu::builder()
            .submenu("~F~ile", alt('f'), |m| {
                m.command_key("~O~pen", Command::OPEN, KeyEvent::from(Key::F(3)), "F3")
                    .command_key("~N~ew", Command::NEW, ctrl_n(), "Ctrl-N")
                    .command_key("~S~ave", Command::SAVE, KeyEvent::from(Key::F(2)), "F2")
                    .command("S~a~ve as…", Command::SAVE_AS)
                    .separator()
                    .command("~C~hange dir…", Command::CH_DIR)
                    .command("~D~OS shell", Command::DOS_SHELL)
                    .command_key("E~x~it", Command::QUIT, ctrl_q(), "Ctrl-Q")
            })
            .submenu("~E~dit", alt('e'), |m| {
                m.command_key("~U~ndo", Command::UNDO, ctrl_u(), "Ctrl-U")
                    .separator()
                    .command_key("Cu~t~", Command::CUT, shift_del(), "Shift-Del")
                    .command_key("~C~opy", Command::COPY, ctrl_ins(), "Ctrl-Ins")
                    .command_key("~P~aste", Command::PASTE, shift_ins(), "Shift-Ins")
                    .separator()
                    .command_key("~C~lear", Command::CLEAR, ctrl_del(), "Ctrl-Del")
            })
            .submenu("~S~earch", alt('s'), |m| {
                m.command("~F~ind…", Command::FIND)
                    .command("~R~eplace…", Command::REPLACE)
                    .command("~S~earch again", Command::SEARCH_AGAIN)
            })
            .submenu("~W~indows", alt('w'), |m| {
                m.command_key("~S~ize/move", Command::RESIZE, ctrl_f5(), "Ctrl-F5")
                    .command_key("~Z~oom", Command::ZOOM, KeyEvent::from(Key::F(5)), "F5")
                    .command("~T~ile", Command::TILE)
                    .command("C~a~scade", Command::CASCADE)
                    .command_key("~N~ext", Command::NEXT, KeyEvent::from(Key::F(6)), "F6")
                    .command_key("~P~revious", Command::PREV, shift_f6(), "Shift-F6")
                    .command_key("~C~lose", Command::CLOSE, ctrl_w(), "Ctrl-W")
            })
            .submenu("~O~ptions", alt('o'), |m| {
                m.submenu("~K~eyboard mapping", None::<KeyEvent>, |k| {
                    k.command("~W~ordStar", KEYMAP_WORDSTAR)
                        .command("~C~UA", KEYMAP_CUA)
                        .command("~E~macs", KEYMAP_EMACS)
                })
            })
            .build();
        Some(Box::new(MenuBar::new(r, menu)))
    }

    /// `TEditorApp::run` — spins the event loop; dispatches OPEN/NEW/CH_DIR to
    /// application-level handlers. Find/replace/save-as are wired inside `Editor`
    /// via the `Deferred` seam (C1/C5) and need no application-level handling.
    fn run(&mut self) {
        let mut next_num: i16 = 1;
        self.program.run_app(move |prog, cmd| {
            if cmd == Command::OPEN {
                if let Some(path) = prog.open_file_dialog("Open file", "*.*") {
                    let r = prog.desktop_rect();
                    let win = EditWindow::new(r, Some(path), next_num);
                    prog.desktop_insert(Box::new(win));
                    next_num += 1;
                }
            } else if cmd == Command::NEW {
                let r = prog.desktop_rect();
                let win = EditWindow::new(r, None, next_num);
                prog.desktop_insert(Box::new(win));
                next_num += 1;
            } else if cmd == Command::CH_DIR {
                prog.exec_view(Box::new(ChDirDialog::new(CD_NORMAL, 0)));
            } else if cmd == KEYMAP_WORDSTAR {
                tvision::keymap::set_global(tvision::Keymap::word_star());
            } else if cmd == KEYMAP_CUA {
                tvision::keymap::set_global(tvision::Keymap::cua());
            } else if cmd == KEYMAP_EMACS {
                tvision::keymap::set_global(tvision::Keymap::emacs());
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Key helpers (faithful to the C++ constants in tvedit.h / tv.h)
// ---------------------------------------------------------------------------

fn ctrl_n() -> KeyEvent {
    ctrl(Key::Char('n'))
}
fn ctrl_q() -> KeyEvent {
    ctrl(Key::Char('q'))
}
fn ctrl_u() -> KeyEvent {
    ctrl(Key::Char('u'))
}
fn ctrl_w() -> KeyEvent {
    ctrl(Key::Char('w'))
}
fn ctrl_f5() -> KeyEvent {
    use tvision::KeyModifiers;
    KeyEvent::new(
        Key::F(5),
        KeyModifiers {
            ctrl: true,
            ..Default::default()
        },
    )
}
fn ctrl_ins() -> KeyEvent {
    use tvision::KeyModifiers;
    KeyEvent::new(
        Key::Insert,
        KeyModifiers {
            ctrl: true,
            ..Default::default()
        },
    )
}
fn shift_del() -> KeyEvent {
    use tvision::KeyModifiers;
    KeyEvent::new(
        Key::Delete,
        KeyModifiers {
            shift: true,
            ..Default::default()
        },
    )
}
fn shift_ins() -> KeyEvent {
    use tvision::KeyModifiers;
    KeyEvent::new(
        Key::Insert,
        KeyModifiers {
            shift: true,
            ..Default::default()
        },
    )
}
fn shift_f6() -> KeyEvent {
    use tvision::KeyModifiers;
    KeyEvent::new(
        Key::F(6),
        KeyModifiers {
            shift: true,
            ..Default::default()
        },
    )
}
fn ctrl_del() -> KeyEvent {
    use tvision::KeyModifiers;
    KeyEvent::new(
        Key::Delete,
        KeyModifiers {
            ctrl: true,
            ..Default::default()
        },
    )
}
fn ctrl(k: Key) -> KeyEvent {
    use tvision::KeyModifiers;
    KeyEvent::new(
        k,
        KeyModifiers {
            ctrl: true,
            ..Default::default()
        },
    )
}

// ---------------------------------------------------------------------------
// int main(int argc, char **argv)
// ---------------------------------------------------------------------------

fn main() -> io::Result<()> {
    let files: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let mut app = TEditApp::new(Box::new(CrosstermBackend::new()?), files);
    app.run();
    Ok(())
}
