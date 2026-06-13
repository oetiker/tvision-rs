//! `gallery` — renders a single widget per run, for the documentation's widget
//! gallery. Each `// ANCHOR: <name>`-marked builder below is included verbatim
//! into the guide, so every documented widget snippet is real, compiling code,
//! and the same builder drives the captured screenshot.
//!
//! Usage:
//! ```text
//! cargo run --example gallery -- <name>   # show one widget
//! cargo run --example gallery             # list the available names
//! ```
//!
//! Adding a widget = write a builder, add a `specimen()` arm + a `NAMES` entry,
//! and register a `Screen` in `xtask/src/screens.rs`.

use std::{env, io};

use tvision::{
    Button, ButtonFlags, CD_NORMAL, ChDirDialog, CheckBoxes, Color, ColorPicker, Command, Context,
    CrosstermBackend, Desktop, Dialog, EditWindow, Event, FD_OPEN_BUTTON, FileDialog, InputLine,
    Key, KeyEvent, Label, ListBox, Memo, Menu, MenuBar, Node, Outline, OutlineViewer, Program,
    RadioButtons, Rect, ScrollBar, StaticText, StatusDef, StatusLine, SystemClock, THistory,
    Terminal, TextDevice, Theme, View, ViewId, Window, alt, delegate, history_add, ov_update,
};

/// How a specimen is shown. Most widgets are leaf controls hosted in a dialog on
/// the desktop; the `Menu` / `Status` variants replace the corresponding chrome
/// so those two can be showcased in place.
#[derive(Clone, Copy)]
enum Specimen {
    /// A view (a dialog or a window) placed on the desktop.
    OnDesktop(fn() -> Box<dyn View>),
    /// A dialog shown modally via `exec_view` — the faithful path for the
    /// self-managing file/directory dialogs, whose lists are filled by
    /// `reset_current` when the modal becomes current.
    Modal(fn() -> Box<dyn View>),
    /// A rich menu bar (the screenshot opens it with a keystroke).
    Menu(fn() -> Menu),
    /// A rich status line.
    Status(fn() -> Vec<StatusDef>),
}

/// Map a CLI name to its specimen. Keep `NAMES` in sync.
fn specimen(name: &str) -> Option<Specimen> {
    use Specimen::*;
    Some(match name {
        "button" => OnDesktop(button),
        "menubar" => Menu(menubar),
        "statusline" => Status(statusline),
        "checkboxes" => OnDesktop(checkboxes),
        "radiobuttons" => OnDesktop(radiobuttons),
        "inputline" => OnDesktop(inputline),
        "statictext" => OnDesktop(statictext),
        "scrollbar" => OnDesktop(scrollbar),
        "history" => OnDesktop(history),
        "dialog" => OnDesktop(dialog),
        "memo" => OnDesktop(memo),
        "colorpicker" => OnDesktop(colorpicker),
        "messagebox" => OnDesktop(messagebox),
        "window" => OnDesktop(window),
        "editor" => OnDesktop(editor),
        "listbox" => OnDesktop(listbox),
        "terminal" => OnDesktop(terminal),
        "outline" => OnDesktop(outline),
        "filedialog" => Modal(filedialog),
        "chdirdialog" => Modal(chdirdialog),
        _ => return None,
    })
}

/// Every registered widget name (for the no-arg listing and the xtask registry).
const NAMES: &[&str] = &[
    "button",
    "menubar",
    "statusline",
    "checkboxes",
    "radiobuttons",
    "inputline",
    "statictext",
    "scrollbar",
    "history",
    "dialog",
    "memo",
    "colorpicker",
    "messagebox",
    "window",
    "editor",
    "listbox",
    "terminal",
    "outline",
    "filedialog",
    "chdirdialog",
];

// ===========================================================================
// Specimens — one `// ANCHOR: <name>` builder per widget.
// ===========================================================================

// ANCHOR: button
/// A dialog with a default `OK` button and a `Cancel` button. The `~` marks the
/// hot-letter; `default: true` makes `Enter` press `OK`.
fn button() -> Box<dyn View> {
    let mut dlg = Dialog::new(Rect::new(2, 1, 36, 9), Some("Buttons".to_string()));
    dlg.insert_child(Box::new(Button::new(
        Rect::new(4, 4, 16, 6),
        "~O~K",
        Command::OK,
        ButtonFlags {
            default: true,
            ..Default::default()
        },
    )));
    dlg.insert_child(Box::new(Button::new(
        Rect::new(19, 4, 31, 6),
        "~C~ancel",
        Command::CANCEL,
        ButtonFlags::default(),
    )));
    Box::new(dlg)
}
// ANCHOR_END: button

// ANCHOR: menubar
/// A menu bar with `File`, `Edit`, and `Window` pull-downs. Each `~`-marked
/// letter is the hot-key; `command_key` adds the accelerator shown at the right.
fn menubar() -> Menu {
    Menu::builder()
        .submenu("~F~ile", alt('f'), |m| {
            m.command_key(
                "~O~pen…",
                Command::custom("gallery.open"),
                KeyEvent::from(Key::F(3)),
                "F3",
            )
            .command_key(
                "~N~ew",
                Command::custom("gallery.new"),
                KeyEvent::from(Key::F(4)),
                "F4",
            )
            .separator()
            .command_key("E~x~it", Command::QUIT, alt('x'), "Alt-X")
        })
        .submenu("~E~dit", alt('e'), |m| {
            m.command("Cu~t~", Command::CUT)
                .command("~C~opy", Command::COPY)
                .command("~P~aste", Command::PASTE)
        })
        .submenu("~W~indow", alt('w'), |m| {
            m.command("~T~ile", Command::TILE)
                .command("C~a~scade", Command::CASCADE)
        })
        .build()
}
// ANCHOR_END: menubar

// ANCHOR: statusline
/// A status line of labelled hot-key items. Clicking a label or pressing its key
/// fires the command; `~`-marked text is highlighted.
fn statusline() -> Vec<StatusDef> {
    StatusDef::list()
        .def_all(|d| {
            d.item("~F2~ Save", KeyEvent::from(Key::F(2)), Command::SAVE)
                .item(
                    "~F3~ Open",
                    KeyEvent::from(Key::F(3)),
                    Command::custom("gallery.open"),
                )
                .item("~F10~ Menu", KeyEvent::from(Key::F(10)), Command::MENU)
                .item("~Alt-X~ Exit", alt('x'), Command::QUIT)
        })
        .build()
}
// ANCHOR_END: statusline

// ANCHOR: checkboxes
/// Three independent check boxes hosted in a dialog. `cluster.value` is a
/// bitmask: bit 0 = first item, bit 1 = second, etc. Bit 0 and bit 2 are set
/// so items A and C start checked.
fn checkboxes() -> Box<dyn View> {
    let mut dlg = Dialog::new(Rect::new(2, 1, 38, 9), Some("Check Boxes".to_string()));
    let mut cb = CheckBoxes::new(
        Rect::new(3, 3, 34, 6),
        vec![
            "~O~ption A".to_string(),
            "~O~ption B".to_string(),
            "~O~ption C".to_string(),
        ],
    );
    cb.cluster.value = 0b101; // items A and C checked
    dlg.insert_child(Box::new(cb));
    Box::new(dlg)
}
// ANCHOR_END: checkboxes

// ANCHOR: radiobuttons
/// Three mutually-exclusive radio buttons. `cluster.value` is the selected
/// index; item 1 ("Two") starts selected.
fn radiobuttons() -> Box<dyn View> {
    let mut dlg = Dialog::new(Rect::new(2, 1, 38, 9), Some("Radio Buttons".to_string()));
    let mut rb = RadioButtons::new(
        Rect::new(3, 3, 34, 6),
        vec![
            "~O~ne".to_string(),
            "~T~wo".to_string(),
            "T~h~ree".to_string(),
        ],
    );
    rb.cluster.value = 1; // "Two" selected
    dlg.insert_child(Box::new(rb));
    Box::new(dlg)
}
// ANCHOR_END: radiobuttons

// ANCHOR: inputline
/// A labeled single-line text entry. The `Label` links to the `InputLine` via
/// its `ViewId` so that pressing `~N~` focuses the field.
fn inputline() -> Box<dyn View> {
    let mut dlg = Dialog::new(Rect::new(2, 1, 44, 9), Some("Input Line".to_string()));
    let mut il = InputLine::with_limit(Rect::new(10, 3, 40, 4), 64);
    il.data = "default text".to_string();
    let il_id = dlg.insert_child(Box::new(il));
    dlg.insert_child(Box::new(Label::new(
        Rect::new(2, 3, 10, 4),
        "~N~ame:",
        Some(il_id),
    )));
    Box::new(dlg)
}
// ANCHOR_END: inputline

// ANCHOR: statictext
/// Static text supports word wrap, left-aligned lines, and the `\x03` prefix
/// to center individual lines.
fn statictext() -> Box<dyn View> {
    let mut dlg = Dialog::new(Rect::new(2, 1, 44, 12), Some("Static Text".to_string()));
    dlg.insert_child(Box::new(StaticText::new(
        Rect::new(2, 2, 40, 9),
        "\x03Centered Title\n\nLeft-aligned body text.\nSecond line of body.\nThird line here.",
    )));
    Box::new(dlg)
}
// ANCHOR_END: statictext

// ANCHOR: scrollbar
/// One vertical and one horizontal scroll bar with a visible thumb. The thumb
/// position is set by writing the public fields before insertion.
fn scrollbar() -> Box<dyn View> {
    let mut dlg = Dialog::new(Rect::new(2, 1, 44, 14), Some("Scroll Bars".to_string()));

    // Vertical bar: 1 wide × 10 tall
    let mut vsb = ScrollBar::new(Rect::new(20, 2, 21, 12));
    vsb.min_value = 0;
    vsb.max_value = 50;
    vsb.value = 10;
    vsb.page_step = 5;
    vsb.arrow_step = 1;
    dlg.insert_child(Box::new(vsb));

    // Horizontal bar: 30 wide × 1 tall
    let mut hsb = ScrollBar::new(Rect::new(5, 5, 35, 6));
    hsb.min_value = 0;
    hsb.max_value = 50;
    hsb.value = 20;
    hsb.page_step = 5;
    hsb.arrow_step = 1;
    dlg.insert_child(Box::new(hsb));

    Box::new(dlg)
}
// ANCHOR_END: scrollbar

// ANCHOR: history
/// An input line with a `THistory` dropdown icon. Two history entries are
/// pre-loaded into channel 1 so the recall list is non-empty.
fn history() -> Box<dyn View> {
    history_add(1, "previous entry");
    history_add(1, "another entry");

    let mut dlg = Dialog::new(Rect::new(2, 1, 50, 9), Some("History".to_string()));
    let il = InputLine::with_limit(Rect::new(3, 3, 40, 4), 64);
    let il_id = dlg.insert_child(Box::new(il));
    dlg.insert_child(Box::new(THistory::new(Rect::new(40, 3, 43, 4), il_id, 1u8)));
    Box::new(dlg)
}
// ANCHOR_END: history

// ANCHOR: dialog
/// A realistic settings dialog combining a labeled text field, check boxes,
/// and OK / Cancel buttons.
fn dialog() -> Box<dyn View> {
    let mut dlg = Dialog::new(Rect::new(2, 1, 46, 14), Some("Settings".to_string()));

    // Name field with label
    let il = InputLine::with_limit(Rect::new(12, 2, 42, 3), 64);
    let il_id = dlg.insert_child(Box::new(il));
    dlg.insert_child(Box::new(Label::new(
        Rect::new(2, 2, 12, 3),
        "~N~ame:",
        Some(il_id),
    )));

    // Check boxes
    let mut cb = CheckBoxes::new(
        Rect::new(2, 5, 42, 8),
        vec![
            "~E~nable logging".to_string(),
            "~A~uto-save".to_string(),
            "~S~how hints".to_string(),
        ],
    );
    cb.cluster.value = 0b001; // "Enable logging" checked
    dlg.insert_child(Box::new(cb));

    // OK and Cancel buttons
    dlg.insert_child(Box::new(Button::new(
        Rect::new(5, 10, 17, 12),
        "~O~K",
        Command::OK,
        ButtonFlags {
            default: true,
            ..Default::default()
        },
    )));
    dlg.insert_child(Box::new(Button::new(
        Rect::new(27, 10, 41, 12),
        "~C~ancel",
        Command::CANCEL,
        ButtonFlags::default(),
    )));

    Box::new(dlg)
}
// ANCHOR_END: dialog

// ANCHOR: memo
/// A `Memo` editor with pre-filled text. Scroll-bar ids are omitted (`None`)
/// to keep the specimen self-contained.
fn memo() -> Box<dyn View> {
    let mut dlg = Dialog::new(Rect::new(2, 1, 50, 14), Some("Memo".to_string()));
    let mut m = Memo::new(Rect::new(2, 2, 46, 11), None, None, None, 4096);
    m.editor
        .set_text(b"Initial memo text.\nSecond line.\nThird line of content.");
    dlg.insert_child(Box::new(m));
    Box::new(dlg)
}
// ANCHOR_END: memo

// ANCHOR: colorpicker
/// A full-color picker with OK and Cancel buttons. The picker needs at least
/// 56 × 18 content area; the dialog is sized to give it comfortable margins.
fn colorpicker() -> Box<dyn View> {
    let mut dlg = Dialog::new(Rect::new(1, 0, 63, 23), Some("Select Color".to_string()));
    dlg.insert_child(Box::new(ColorPicker::new(
        Rect::new(2, 2, 60, 20),
        Color::Rgb(30, 144, 255),
    )));
    dlg.insert_child(Box::new(Button::new(
        Rect::new(8, 20, 20, 22),
        "~O~K",
        Command::OK,
        ButtonFlags {
            default: true,
            ..Default::default()
        },
    )));
    dlg.insert_child(Box::new(Button::new(
        Rect::new(42, 20, 56, 22),
        "~C~ancel",
        Command::CANCEL,
        ButtonFlags::default(),
    )));
    Box::new(dlg)
}
// ANCHOR_END: colorpicker

// ANCHOR: messagebox
/// A simple information dialog — equivalent to a Turbo Vision `messageBox` —
/// built from public types: a `Dialog`, a `StaticText` message, and a default
/// OK `Button`.
fn messagebox() -> Box<dyn View> {
    let mut dlg = Dialog::new(Rect::new(5, 3, 45, 12), Some("Information".to_string()));
    dlg.insert_child(Box::new(StaticText::new(
        Rect::new(2, 2, 38, 6),
        "\x03Operation completed successfully.",
    )));
    dlg.insert_child(Box::new(Button::new(
        Rect::new(14, 6, 26, 8),
        "~O~K",
        Command::OK,
        ButtonFlags {
            default: true,
            ..Default::default()
        },
    )));
    Box::new(dlg)
}
// ANCHOR_END: messagebox

// ANCHOR: window
/// A titled `Window` with two lines of `StaticText` inside. The inner bounds
/// are inset by one cell on every side so the text clears the frame.
fn window() -> Box<dyn View> {
    let mut win = Window::new(Rect::new(2, 1, 52, 17), Some("Window".to_string()), 1);
    win.insert_child(Box::new(StaticText::new(
        Rect::new(2, 2, 47, 8),
        "This is a plain titled Window.\n\nIt can host any child views,\njust like a Dialog.",
    )));
    Box::new(win)
}
// ANCHOR_END: window

// ANCHOR: editor
/// An `EditWindow` opened on a small sample file written to a temp path. The
/// window includes scroll bars and a line:column indicator in the frame.
fn editor() -> Box<dyn View> {
    let path = std::env::temp_dir().join("rstv_gallery_sample.txt");
    let _ = std::fs::write(
        &path,
        "fn main() {\n    println!(\"Hello, rstv!\");\n}\n\n// An editor window with\n// scroll bars and a line:col indicator.\n",
    );
    let win = EditWindow::new(Rect::new(1, 0, 71, 22), Some(path), 1);
    Box::new(win)
}
// ANCHOR_END: editor

// ANCHOR: listbox
/// A `ListBox` with a vertical scroll bar, populated on the first event tick
/// via a thin wrapper view (the `new_list` call needs `&mut Context`).
struct ListBoxShowcase {
    dialog: Dialog,
    list_id: ViewId,
    populated: bool,
}

impl ListBoxShowcase {
    fn new() -> Self {
        let mut dlg = Dialog::new(Rect::new(2, 1, 46, 15), Some("List Box".to_string()));
        let vsb = ScrollBar::new(Rect::new(41, 2, 42, 12));
        let vsb_id = dlg.insert_child(Box::new(vsb));
        let lb = ListBox::new(Rect::new(2, 2, 41, 12), 1, None, Some(vsb_id));
        let list_id = dlg.insert_child(Box::new(lb));
        ListBoxShowcase {
            dialog: dlg,
            list_id,
            populated: false,
        }
    }
}

#[delegate(to = dialog)]
impl View for ListBoxShowcase {
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        if !self.populated {
            self.populated = true;
            if let Some(v) = self.dialog.child_mut(self.list_id)
                && let Some(lb) = v.as_any_mut().and_then(|a| a.downcast_mut::<ListBox>())
            {
                lb.new_list(
                    vec![
                        "Alpha".into(),
                        "Beta".into(),
                        "Gamma".into(),
                        "Delta".into(),
                        "Epsilon".into(),
                        "Zeta".into(),
                        "Eta".into(),
                        "Theta".into(),
                    ],
                    ctx,
                );
                tvision::widgets::list_viewer::update_steps(lb, ctx);
            }
        }
        self.dialog.handle_event(ev, ctx);
    }
}

fn listbox() -> Box<dyn View> {
    Box::new(ListBoxShowcase::new())
}
// ANCHOR_END: listbox

// ANCHOR: terminal
/// A `Terminal` widget initialized on the first event tick and seeded with two
/// lines of output. The wrapper pattern lets `init` and `write_bytes` receive
/// the `&mut Context` they require.
struct TerminalShowcase {
    window: Window,
    term_id: ViewId,
    initialized: bool,
}

impl TerminalShowcase {
    fn new() -> Self {
        let mut win = Window::new(Rect::new(2, 1, 58, 17), Some("Terminal".to_string()), 1);
        let term = Terminal::new(Rect::new(1, 1, 54, 15), None, None, 4096);
        let term_id = win.insert_child(Box::new(term));
        TerminalShowcase {
            window: win,
            term_id,
            initialized: false,
        }
    }
}

#[delegate(to = window)]
impl View for TerminalShowcase {
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        if !self.initialized {
            self.initialized = true;
            if let Some(v) = self.window.child_mut(self.term_id)
                && let Some(t) = v.as_any_mut().and_then(|a| a.downcast_mut::<Terminal>())
            {
                t.init(ctx);
                t.write_bytes(b"Hello from Terminal!\nLine two.\nLine three.\n", ctx);
            }
        }
        self.window.handle_event(ev, ctx);
    }
}

fn terminal() -> Box<dyn View> {
    Box::new(TerminalShowcase::new())
}
// ANCHOR_END: terminal

// ANCHOR: outline
/// An `Outline` tree viewer with horizontal and vertical scroll bars,
/// initialized on the first event tick (mirrors `tvdir.rs`'s lazy-init guard).
struct OutlineShowcase {
    window: Window,
    outline_id: ViewId,
}

impl OutlineShowcase {
    fn new() -> Self {
        let mut win = Window::new(Rect::new(2, 1, 46, 21), Some("Outline".to_string()), 1);
        let hsb = ScrollBar::new(Rect::new(1, 18, 41, 19));
        let h_id = win.insert_child(Box::new(hsb));
        let vsb = ScrollBar::new(Rect::new(41, 1, 42, 19));
        let v_id = win.insert_child(Box::new(vsb));
        let root = Node::new("Root")
            .with_expanded(true)
            .with_children(Box::new(
                Node::new("Child A")
                    .with_expanded(true)
                    .with_children(Box::new(
                        Node::new("Grandchild A1").with_next(Box::new(Node::new("Grandchild A2"))),
                    ))
                    .with_next(Box::new(
                        Node::new("Child B")
                            .with_expanded(true)
                            .with_children(Box::new(Node::new("Grandchild B1"))),
                    )),
            ));
        let ol = Outline::new(
            Rect::new(1, 1, 41, 19),
            Some(h_id),
            Some(v_id),
            Some(Box::new(root)),
        );
        let outline_id = win.insert_child(Box::new(ol));
        OutlineShowcase {
            window: win,
            outline_id,
        }
    }
}

#[delegate(to = window)]
impl View for OutlineShowcase {
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        if let Some(v) = self.window.child_mut(self.outline_id)
            && let Some(ol) = v.as_any_mut().and_then(|a| a.downcast_mut::<Outline>())
            && ol.ov().limit.y == 0
        {
            ov_update(ol, ctx);
        }
        self.window.handle_event(ev, ctx);
    }
}

fn outline() -> Box<dyn View> {
    Box::new(OutlineShowcase::new())
}
// ANCHOR_END: outline

// ANCHOR: filedialog
/// A `FileDialog` with an `Open` button. The file list is read from the current
/// directory by `reset_current` when the dialog is shown modally. (The gallery
/// first `cd`s into a small fixture dir so the listing is reproducible; a real
/// app just builds the dialog.)
fn filedialog() -> Box<dyn View> {
    enter_gallery_fixture();
    let fd = FileDialog::new("*.*", "Open a File", "~N~ame", FD_OPEN_BUTTON, 2);
    Box::new(fd)
}
// ANCHOR_END: filedialog

// ANCHOR: chdirdialog
/// A `ChDirDialog` (Change Directory). The directory tree reflects the current
/// directory, read by `reset_current` when the dialog is shown modally. (The
/// gallery `cd`s into a fixture dir first for a reproducible tree.)
fn chdirdialog() -> Box<dyn View> {
    enter_gallery_fixture();
    let cd = ChDirDialog::new(CD_NORMAL, 3);
    Box::new(cd)
}
// ANCHOR_END: chdirdialog

/// Switch into a committed fixture directory (`examples/gallery_fixture`) so the
/// file/directory dialogs list fixed, reproducible content instead of whatever
/// happens to be in the working directory. Gallery scaffolding only — not part
/// of the widget API.
fn enter_gallery_fixture() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/gallery_fixture");
    let _ = std::env::set_current_dir(dir);
}

// ===========================================================================
// Default chrome (used whenever a specimen does not replace it).
// ===========================================================================

/// A representative File / Edit / Window menu bar.
fn default_menu() -> Menu {
    Menu::builder()
        .submenu("~F~ile", alt('f'), |m| {
            m.command_key(
                "~O~pen…",
                Command::custom("gallery.open"),
                KeyEvent::from(Key::F(3)),
                "F3",
            )
            .separator()
            .command_key("E~x~it", Command::QUIT, alt('x'), "Alt-X")
        })
        .submenu("~E~dit", alt('e'), |m| {
            m.command("Cu~t~", Command::CUT)
                .command("~C~opy", Command::COPY)
                .command("~P~aste", Command::PASTE)
        })
        .submenu("~W~indow", alt('w'), |m| {
            m.command("~T~ile", Command::TILE)
                .command("C~a~scade", Command::CASCADE)
        })
        .build()
}

/// A representative status line.
fn default_status() -> Vec<StatusDef> {
    StatusDef::list()
        .def_all(|d| {
            d.item("~F10~ Menu", KeyEvent::from(Key::F(10)), Command::MENU)
                .item("~Alt-X~ Exit", alt('x'), Command::QUIT)
        })
        .build()
}

// ===========================================================================
// Program assembly: the three factories consult the selected specimen.
// ===========================================================================

fn make_desktop(extent: Rect, spec: Specimen) -> Box<dyn View> {
    let mut r = extent;
    r.a.y += 1; // below the menu bar
    r.b.y -= 1; // above the status line
    let mut desktop = Desktop::new(r, |br| Some(Desktop::init_background(br)));
    if let Specimen::OnDesktop(build) = spec {
        desktop.insert_view(build());
    }
    Box::new(desktop)
}

fn make_status(extent: Rect, spec: Specimen) -> Box<dyn View> {
    let mut r = extent;
    r.a.y = r.b.y - 1;
    let defs = match spec {
        Specimen::Status(build) => build(),
        _ => default_status(),
    };
    Box::new(StatusLine::new(r, defs))
}

fn make_menu(extent: Rect, spec: Specimen) -> Box<dyn View> {
    let mut r = extent;
    r.b.y = r.a.y + 1;
    let menu = match spec {
        Specimen::Menu(build) => build(),
        _ => default_menu(),
    };
    Box::new(MenuBar::new(r, menu))
}

fn main() -> io::Result<()> {
    let name = env::args().nth(1).unwrap_or_default();
    let Some(spec) = specimen(&name) else {
        eprintln!("usage: cargo run --example gallery -- <name>");
        eprintln!("widgets: {}", NAMES.join(", "));
        return Ok(());
    };

    let mut program = Program::new(
        Box::new(CrosstermBackend::new()?),
        Box::new(SystemClock::new()),
        Theme::classic_blue(),
        |r| Some(make_desktop(r, spec)),
        |r| Some(make_status(r, spec)),
        |r| Some(make_menu(r, spec)),
    );
    // Both `run` and `exec_view` idle after painting; the screenshot tooling
    // captures the static frame and then kills the session (no quit/close
    // command is ever sent). A `Modal` specimen is shown via `exec_view` so its
    // `reset_current` runs — that is what fills the file/directory lists.
    match spec {
        Specimen::Modal(build) => {
            let _ = program.exec_view(build());
        }
        _ => {
            let _ = program.run();
        }
    }
    Ok(())
}
