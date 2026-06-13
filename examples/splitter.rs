//! `splitter` — a nested-grid window using the [`Splitter`] widget: a fixed
//! tree sidebar beside a right column that stacks a scrolling list over a
//! small form. The two splitters (vertical outer + horizontal inner) are
//! nested inside a plain [`tvision::Window`]; the outer [`Splitter`] is built
//! `.joined()`, so its divider lines join each other (`├`) at every crossing —
//! and the window automatically joins a joined splitter body to its frame
//! (`┬`/`┴`/`┤`). Joining cascades, so only the outer splitter opts in.
//!
//! This is the N-ary resizable splitter in action. The panes are real Turbo
//! Vision controls (an [`Outline`] tree, a [`ListBox`], and a form [`Group`]
//! of [`InputLine`]s + a [`Button`]); the [`Splitter`] lays them out and
//! brokers the divider drags.
//!
//! Controls:
//!   - **Mouse:** drag a `Line` divider seam to resize the panes on either
//!     side. There are two seams: vertical (between tree and right column) and
//!     horizontal (between list and form).
//!   - **F6:** enter divider-reconfig mode. Then:
//!       - `Tab` / `Shift-Tab` pick which divider to move,
//!       - arrow keys nudge the selected divider,
//!       - `Enter` commits, `Esc` cancels.
//!   - **Tab** (outside reconfig) moves focus between panes / form fields.
//!   - **Alt-X** quits.
//!
//! Run it:
//! `cargo run --example splitter`

use std::io;

use tvision::{
    Backend, Button, ButtonFlags, Command, Constraints, Context, CrosstermBackend, Desktop,
    DividerStyle, Event, Group, InputLine, Key, KeyEvent, Label, ListBox, Menu, MenuBar, Node,
    Outline, Program, Rect, Splitter, StatusDef, StatusLine, SystemClock, Theme, View, alt,
    delegate,
};

// ---------------------------------------------------------------------------
// List pane — a ListBox that populates itself on first event.
//
// `ListBox::new_list` needs a `Context` (it republishes its scrollbar range),
// which the constructor does not have. The idiomatic post-insert pattern is to
// run that setup once a `Context` is in hand. This thin wrapper delegates every
// `View` method to its inner `ListBox` and seeds the items the first time it is
// handed an event (the pump broadcasts reach every view), so the list is filled
// without any library change.
// ---------------------------------------------------------------------------

struct ListPane {
    list: ListBox,
    items: Vec<String>,
    seeded: bool,
}

impl ListPane {
    fn new(bounds: Rect, items: Vec<String>) -> Self {
        // Single-column list, no external scrollbars (None, None).
        ListPane {
            list: ListBox::new(bounds, 1, None, None),
            items,
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
        if !self.seeded {
            self.seeded = true;
            let items = std::mem::take(&mut self.items);
            self.list.new_list(items, ctx);
        }
        self.list.handle_event(ev, ctx);
    }
}

// ---------------------------------------------------------------------------
// Pane builders — each returns a Box<dyn View> for the Splitter.
// ---------------------------------------------------------------------------

/// The tree pane: a small [`Node`] hierarchy in an [`Outline`].
fn build_tree(bounds: Rect) -> Box<dyn View> {
    // Animals
    //   Mammals: Cat, Dog
    //   Birds:   Owl, Wren
    let birds = Box::new(
        Node::new("Birds")
            .with_expanded(true)
            .with_children(Box::new(
                Node::new("Owl").with_next(Box::new(Node::new("Wren"))),
            )),
    );
    // Mammals is the first sibling; Birds chains after it via `with_next`.
    let mammals = Node::new("Mammals")
        .with_expanded(true)
        .with_children(Box::new(
            Node::new("Cat").with_next(Box::new(Node::new("Dog"))),
        ))
        .with_next(birds);
    let root = Box::new(
        Node::new("Animals")
            .with_expanded(true)
            .with_children(Box::new(mammals)),
    );
    Box::new(Outline::new(bounds, None, None, Some(root)))
}

/// The list pane: a [`ListBox`] of a handful of items (see [`ListPane`]).
fn build_list(bounds: Rect) -> Box<dyn View> {
    let items = vec![
        "Apricot".to_string(),
        "Blueberry".to_string(),
        "Cherry".to_string(),
        "Damson".to_string(),
        "Elderberry".to_string(),
        "Fig".to_string(),
        "Grape".to_string(),
        "Honeydew".to_string(),
    ];
    Box::new(ListPane::new(bounds, items))
}

/// The form pane: a [`Group`] holding two labelled [`InputLine`]s and a button.
fn build_form(bounds: Rect) -> Box<dyn View> {
    let mut group = Group::new(bounds);

    // First field: Name.
    let name = InputLine::with_limit(Rect::new(2, 2, 18, 3), 64);
    let name_id = group.insert(Box::new(name));
    group.insert(Box::new(Label::new(
        Rect::new(2, 1, 18, 2),
        "~N~ame",
        Some(name_id),
    )));

    // Second field: City.
    let city = InputLine::with_limit(Rect::new(2, 5, 18, 6), 64);
    let city_id = group.insert(Box::new(city));
    group.insert(Box::new(Label::new(
        Rect::new(2, 4, 18, 5),
        "~C~ity",
        Some(city_id),
    )));

    // A default OK button (cmOK just closes a dialog; here it is a focusable
    // form control demonstrating the third pane is a real interactive group).
    group.insert(Box::new(Button::new(
        Rect::new(2, 7, 12, 9),
        "~O~K",
        Command::OK,
        ButtonFlags::new(),
    )));

    Box::new(group)
}

// ---------------------------------------------------------------------------
// SplitterApp : public TApplication
// ---------------------------------------------------------------------------

struct SplitterApp {
    program: Program,
}

impl SplitterApp {
    fn new(backend: Box<dyn Backend>) -> Self {
        let program = Program::new(
            backend,
            Box::new(SystemClock::new()),
            Theme::classic_blue(),
            Self::init_desktop,
            Self::init_status_line,
            Self::init_menu_bar,
        );
        SplitterApp { program }
    }

    /// One window filling most of the desktop, with the splitter inside it.
    fn init_desktop(r: Rect) -> Option<Box<dyn View>> {
        let mut r = r;
        r.a.y += 1; // below the menu bar
        r.b.y -= 1; // above the status line
        let mut desktop = Desktop::new(r, |br| Some(Desktop::init_background(br)));

        // A window sized to a generous chunk of the desktop.
        let win_rect = Rect::new(r.a.x + 2, r.a.y + 1, r.b.x - 2, r.b.y - 1);
        let mut win = tvision::Window::new(win_rect, Some("Multi-pane Splitter".to_string()), 1);

        // The window interior in LOCAL coords: frame-inset by one cell each side.
        let ext = win.state().get_extent();
        let interior = Rect::new(1, 1, ext.b.x - 1, ext.b.y - 1);

        let tree = build_tree(interior);
        let list = build_list(interior);
        let form = build_form(interior);

        // Right side: list stacked over form, separated by a horizontal divider.
        let right = Splitter::rows()
            .pane(list, Constraints::flex().min(3))
            .pane(form, Constraints::flex().min(6));

        // Outer: a fixed tree sidebar column beside the right grid, with a thin
        // Line divider. Built `.joined()` so the seam joins the window frame
        // (┬/┴, auto-brokered by the window) and the inner horizontal divider (├);
        // joining cascades to the inner `right` splitter.
        let split = Splitter::cols()
            .pane(tree, Constraints::fixed(22))
            .pane(Box::new(right), Constraints::flex())
            .divider(0, DividerStyle::Line)
            .joined();

        let split_id = win.insert_child(Box::new(split));
        // Size the splitter to fill the window interior so it lays the panes out.
        if let Some(v) = win.child_mut(split_id) {
            v.change_bounds(interior);
        }

        desktop.insert_view(Box::new(win));
        Some(Box::new(desktop))
    }

    fn init_status_line(r: Rect) -> Option<Box<dyn View>> {
        let mut r = r;
        r.a.y = r.b.y - 1;
        let defs = StatusDef::list()
            .def_all(|d| {
                d.item(
                    "~F6~ Resize panes",
                    KeyEvent::from(Key::F(6)),
                    Command::custom("noop"),
                )
                .item("~Alt-X~ Exit", alt('x'), Command::QUIT)
            })
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
// int main()
// ---------------------------------------------------------------------------

fn main() -> io::Result<()> {
    let mut app = SplitterApp::new(Box::new(CrosstermBackend::new()?));
    let _result: Command = app.run();
    Ok(())
}
