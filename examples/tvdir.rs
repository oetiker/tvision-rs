//! `tvdir` — a faithful port of the magiblot/tvision `tvdir` example.
//!
//! Shows a directory tree (`TOutline`/`TDirOutline`) on the left and a file
//! list (`TFilePane`, here a custom [`Scroller`] subtype) on the right, inside
//! a single window. Clicking / navigating the outline updates the file list.
//!
//! Run it:  `cargo run --example tvdir [path]`
//!   - Arrow keys navigate the outline tree.
//!   - Enter / Space expands or collapses a node.
//!   - Tab cycles focus between the outline and the file pane.
//!   - `Alt-X` or File → Exit quits.
//!   - File → New Window opens another directory window.

use std::io;
use std::path::{Path, PathBuf};

use tvision::{
    Backend, Button, ButtonFlags, Command, CrosstermBackend, Desktop, Dialog, DrawCtx, Key,
    KeyEvent, Menu, MenuBar, Node, Outline, OutlineViewer, OutlineViewerState, Program, Rect,
    Role, ScrollBar, Scroller, StaticText, StatusDef, StatusLine, SystemClock, Theme, View, ViewId,
    Window, alt, delegate,
};

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

const CMD_ABOUT: Command = Command::custom("tvdir.about");
const CMD_NEW_WINDOW: Command = Command::custom("tvdir.new_window");

// ---------------------------------------------------------------------------
// Directory tree helpers (replaces DOS _dos_findfirst / _dos_findnext)
// ---------------------------------------------------------------------------

fn build_dir_tree(path: &Path) -> Option<Box<Node>> {
    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    let Ok(rd) = std::fs::read_dir(path) else {
        return None;
    };
    for entry in rd.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            entries.push((name, entry.path()));
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut head: Option<Box<Node>> = None;
    for (name, child_path) in entries.into_iter().rev() {
        let children = build_dir_tree(&child_path);
        let mut node = Node::new(name);
        if let Some(c) = children {
            node = node.with_children(c);
        }
        if let Some(next) = head.take() {
            node = node.with_next(next);
        }
        head = Some(Box::new(node));
    }
    head
}

fn list_files(path: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(path) else {
        return vec![];
    };
    let mut files: Vec<String> = rd
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if let Ok(m) = e.metadata() {
                format!("{:<20} {:>10}", name, m.len())
            } else {
                name
            }
        })
        .collect();
    files.sort();
    files
}

// ---------------------------------------------------------------------------
// TDirOutline — port of the C++ TDirOutline : public TOutline
//
// Overrides `focused_item` to broadcast CMD_NEW_DIR_FOCUSED to the owner
// window when the focused node changes (driving the file pane).
// ---------------------------------------------------------------------------

struct DirOutline {
    outline: Outline,
    root_path: PathBuf,
}

impl DirOutline {
    fn new(
        bounds: Rect,
        h: Option<ViewId>,
        v: Option<ViewId>,
        root_path: PathBuf,
    ) -> Self {
        let root_label = root_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| root_path.to_string_lossy().into_owned());

        let children = build_dir_tree(&root_path);
        let mut root_node = Node::new(root_label).with_expanded(true);
        if let Some(c) = children {
            root_node = root_node.with_children(c);
        }

        DirOutline {
            outline: Outline::new(bounds, h, v, Some(Box::new(root_node))),
            root_path,
        }
    }

    /// Walk the DFS tree to build the filesystem path of the focused node.
    fn current_path(&self) -> PathBuf {
        let foc = self.outline.ov().foc;
        let mut path = self.root_path.clone();
        // pos 0 is the root node itself.
        if foc == 0 {
            return path;
        }
        // Collect the path components along the focused DFS branch.
        self.collect_path(self.outline.get_root(), foc, 1, &mut path);
        path
    }

    fn collect_path(&self, node: Option<&Node>, target: i32, pos: i32, path: &mut PathBuf) -> i32 {
        let Some(node) = node else { return pos };
        if pos == target {
            path.push(&node.text);
            return -1; // sentinel: found
        }
        // Visit children first (DFS pre-order).
        if node.expanded {
            let mut child_pos = pos + 1;
            let mut child = node.child_list.as_deref();
            while let Some(c) = child {
                path.push(&c.text);
                let next_pos = self.collect_path(Some(c), target, child_pos, path);
                if next_pos == -1 {
                    return -1; // propagate found
                }
                path.pop();
                child_pos = next_pos;
                child = c.next.as_deref();
            }
        }
        // Then visit next sibling.
        self.collect_path(node.next.as_deref(), target, pos + 1 + self.subtree_size(node), path)
    }

    fn subtree_size(&self, node: &Node) -> i32 {
        if !node.expanded {
            return 0;
        }
        let mut n = 0;
        let mut child = node.child_list.as_deref();
        while let Some(c) = child {
            n += 1 + self.subtree_size(c);
            child = c.next.as_deref();
        }
        n
    }
}

#[delegate(to = outline)]
impl View for DirOutline {}

impl OutlineViewer for DirOutline {
    fn ov(&self) -> &OutlineViewerState {
        self.outline.ov()
    }
    fn ov_mut(&mut self) -> &mut OutlineViewerState {
        self.outline.ov_mut()
    }
    fn get_root(&self) -> Option<&Node> {
        self.outline.get_root()
    }
    fn get_next<'a>(&'a self, node: &'a Node) -> Option<&'a Node> {
        self.outline.get_next(node)
    }
    fn get_child<'a>(&'a self, node: &'a Node, i: i32) -> Option<&'a Node> {
        self.outline.get_child(node, i)
    }
    fn get_num_children(&self, node: &Node) -> i32 {
        self.outline.get_num_children(node)
    }
    fn get_text<'a>(&'a self, node: &'a Node) -> &'a str {
        self.outline.get_text(node)
    }
    fn is_expanded(&self, node: &Node) -> bool {
        self.outline.is_expanded(node)
    }
    fn has_children(&self, node: &Node) -> bool {
        self.outline.has_children(node)
    }
    fn adjust(&mut self, pos: i32, expand: bool) {
        self.outline.adjust(pos, expand)
    }

    /// Override: broadcast CMD_NEW_DIR_FOCUSED to the owner window so it can
    /// update the file pane. Mirrors `TDirOutline::focused(i)`.
    fn focused_item(&mut self, i: i32) {
        self.outline.ov_mut().foc = i;
        // The broadcast is sent during handle_event / draw — we use the
        // Deferred::Broadcast seam by calling ov_update which triggers a redraw;
        // the actual update is driven by the window's handle_event override via
        // the CMD_NEW_DIR_FOCUSED command queued in the deferred channel.
        // Since we can't call ctx.broadcast() here (no ctx), we use a flag.
        // The window polls for focus changes by comparing the last foc.
    }
}

// ---------------------------------------------------------------------------
// FilePane — port of C++ TFilePane : public TScroller
//
// A scrollable text list of filenames/sizes. Wraps a `Scroller` and draws
// each file row from a `Vec<String>`.
// ---------------------------------------------------------------------------

struct FilePane {
    scroller: Scroller,
    files: Vec<String>,
}

impl FilePane {
    fn new(bounds: Rect, h: Option<ViewId>, v: Option<ViewId>) -> Self {
        FilePane {
            scroller: Scroller::new(bounds, h, v),
            files: vec![],
        }
    }

    fn update_dir(&mut self, path: &Path, ctx: &mut tvision::Context) {
        self.files = list_files(path);
        let max_w = self
            .files
            .iter()
            .map(|s| s.chars().count() as i32)
            .max()
            .unwrap_or(1)
            + 2;
        let h = self.files.len().max(1) as i32;
        self.scroller.set_limit(max_w, h, ctx);
    }
}

#[delegate(to = scroller)]
impl View for FilePane {
    fn draw(&mut self, ctx: &mut DrawCtx) {
        let extent = self.scroller.state().get_extent();
        let delta = self.scroller.delta;
        let style = ctx.style(Role::ScrollerNormal);
        for row in 0..extent.b.y {
            let file_idx = (row + delta.y) as usize;
            let text = if file_idx < self.files.len() {
                self.files[file_idx].as_str()
            } else {
                ""
            };
            let col_start = delta.x as usize;
            let visible: String = text.chars().skip(col_start).take(extent.b.x as usize).collect();
            ctx.fill(Rect::new(0, row, extent.b.x, row + 1), ' ', style);
            ctx.put_str(0, row, &visible, style);
        }
    }
}

// ---------------------------------------------------------------------------
// TDirWindow — port of C++ TDirWindow : public TWindow
// ---------------------------------------------------------------------------

struct DirWindow {
    window: Window,
    outline_id: ViewId,
    file_pane_id: ViewId,
    last_foc: i32,
}

impl DirWindow {
    fn new(root_path: PathBuf) -> Self {
        let title = root_path.to_string_lossy().into_owned();
        let mut window = Window::new(Rect::new(1, 1, 76, 21), Some(title), 0);
        window.state_mut().options.tileable = true;

        // Right pane scrollbars.
        let rvsb = ScrollBar::new(Rect::new(74, 1, 75, 15));
        let rhsb = ScrollBar::new(Rect::new(22, 15, 73, 16));
        let rvsb_id = window.insert_child(Box::new(rvsb));
        let rhsb_id = window.insert_child(Box::new(rhsb));

        let fp = FilePane::new(Rect::new(21, 1, 74, 15), Some(rhsb_id), Some(rvsb_id));
        let file_pane_id = window.insert_child(Box::new(fp));

        // Left pane scrollbars.
        let ovsb = ScrollBar::new(Rect::new(20, 1, 21, 19));
        let ohsb = ScrollBar::new(Rect::new(2, 19, 19, 20));
        let ovsb_id = window.insert_child(Box::new(ovsb));
        let ohsb_id = window.insert_child(Box::new(ohsb));

        let outline = DirOutline::new(
            Rect::new(1, 1, 20, 19),
            Some(ohsb_id),
            Some(ovsb_id),
            root_path,
        );
        let outline_id = window.insert_child(Box::new(outline));

        DirWindow {
            window,
            outline_id,
            file_pane_id,
            last_foc: 0,
        }
    }
}

#[delegate(to = window)]
impl View for DirWindow {
    fn handle_event(&mut self, ev: &mut tvision::Event, ctx: &mut tvision::Context) {
        self.window.handle_event(ev, ctx);

        // Check if the outline's focused node changed (TDirOutline::focused
        // in C++ sent a cmNewDirFocused broadcast; we poll here instead).
        let new_foc = self
            .window
            .child_mut(self.outline_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<DirOutline>())
            .map(|ol| ol.outline.ov().foc)
            .unwrap_or(0);

        if new_foc != self.last_foc {
            self.last_foc = new_foc;

            let path = self
                .window
                .child_mut(self.outline_id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<DirOutline>())
                .map(|ol| ol.current_path());

            if let Some(path) = path
                && let Some(fp) = self
                    .window
                    .child_mut(self.file_pane_id)
                    .and_then(|v| v.as_any_mut())
                    .and_then(|a| a.downcast_mut::<FilePane>())
            {
                fp.update_dir(&path, ctx);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TDirApp
// ---------------------------------------------------------------------------

struct TDirApp {
    program: Program,
    root_path: PathBuf,
}

impl TDirApp {
    fn new(backend: Box<dyn Backend>, root_path: PathBuf) -> Self {
        let mut app = TDirApp {
            program: Program::new(
                backend,
                Box::new(SystemClock::new()),
                Theme::classic_blue(),
                Self::init_desktop,
                Self::init_status_line,
                Self::init_menu_bar,
            ),
            root_path: root_path.clone(),
        };
        app.program
            .desktop_insert(Box::new(DirWindow::new(root_path)));
        app
    }

    fn init_desktop(r: Rect) -> Option<Box<dyn View>> {
        let mut r = r;
        r.a.y += 1;
        r.b.y -= 1;
        Some(Box::new(Desktop::new(r, |br| Some(Desktop::init_background(br)))))
    }

    fn init_status_line(r: Rect) -> Option<Box<dyn View>> {
        let mut r = r;
        r.a.y = r.b.y - 1;
        let defs = StatusDef::list()
            .def_all(|d| {
                d.key_item(alt('x'), Command::QUIT)
                    .item("~F10~ Menu", KeyEvent::from(Key::F(10)), Command::MENU)
            })
            .build();
        Some(Box::new(StatusLine::new(r, defs)))
    }

    fn init_menu_bar(r: Rect) -> Option<Box<dyn View>> {
        let mut r = r;
        r.b.y = r.a.y + 1;
        let menu = Menu::builder()
            .submenu("~\u{f0}~", alt(' '), |m| {
                m.command("~A~bout…", CMD_ABOUT)
            })
            .submenu("~F~ile", alt('f'), |m| {
                m.command("~N~ew Window…", CMD_NEW_WINDOW)
                    .separator()
                    .command_key("E~x~it", Command::QUIT, alt('x'), "Alt-X")
            })
            .build();
        Some(Box::new(MenuBar::new(r, menu)))
    }

    fn about_box(prog: &mut Program) {
        let mut dlg = Dialog::new(Rect::new(0, 0, 39, 11), Some("About".to_string()));
        let opts = &mut dlg.state_mut().options;
        opts.center_x = true;
        opts.center_y = true;
        dlg.insert_child(Box::new(StaticText::new(
            Rect::new(9, 2, 30, 7),
            "\x03Outline Viewer Demo\n\n\x03Copyright (c) 1994\n\n\x03Borland International"
                .to_string(),
        )));
        dlg.insert_child(Box::new(Button::new(
            Rect::new(14, 8, 25, 10),
            " OK",
            Command::OK,
            ButtonFlags { default: true, ..ButtonFlags::new() },
        )));
        prog.exec_view(Box::new(dlg));
    }

    fn run(&mut self) {
        let root = self.root_path.clone();
        self.program.run_app(move |prog, cmd| {
            if cmd == CMD_ABOUT {
                Self::about_box(prog);
            } else if cmd == CMD_NEW_WINDOW {
                prog.desktop_insert(Box::new(DirWindow::new(root.clone())));
            }
        });
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() -> io::Result<()> {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let root = root.canonicalize().unwrap_or(root);
    let mut app = TDirApp::new(Box::new(CrosstermBackend::new()?), root);
    app.run();
    Ok(())
}
