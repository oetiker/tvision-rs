//! `tcv` — **Tobi's Catalog Vision**, a homage re-port of a real 1993 Turbo
//! Pascal / Turbo Vision program (TCV v2.2, a floppy-disk catalog browser by
//! Tobias Oetiker) into idiomatic tvision-rs.
//!
//! The original read a `PROGS.TFC` text file listing every file on a stack of
//! catalogued floppies; this port embeds a small period-appropriate mock
//! catalog instead. The soul of the program is its **search-as-you-type**
//! browser (`TDirBox`): type a word and the list jumps to the next entry whose
//! rendered line contains it (case-insensitive), highlighting the match.
//!
//! Run it:  `cargo run --example tcv`
//!   - Up/Down browse the catalog.
//!   - Start typing to search; the list jumps to the next matching entry and
//!     highlights the matched substring.
//!   - In search mode, Up/Down jump to the previous/next match.
//!   - Backspace shortens the search; Esc returns to browse mode.
//!   - Enter or double-click opens the Info box for the focused entry.
//!   - The `~I~nfo` / `~A~bout` / `E~x~it` buttons do the obvious things.
//!
//! The C++/Pascal classes this mirrors: `TTCV : TApplication`, the data window
//! (`TDialog`), `TDirBox : TListBox` (the search list), `TDiskCol`
//! (the catalog collection + `DirLine`), and `TTCVStatLine : TStatusLine` (the
//! context-sensitive hint line).

use std::io;

use tvision_rs::widgets::list_viewer;
use tvision_rs::{
    Backend, Button, ButtonFlags, Command, Context, CrosstermBackend, Desktop, Dialog, DrawCtx,
    Event, GrowMode, HelpCtx, Key, Label, ListViewer, ListViewerState, MessageBoxButtons,
    MessageBoxKind, Point, Program, Rect, Role, ScrollBar, StateFlag, StatusDef, StatusLine,
    SystemClock, Theme, View, ViewId, ViewState, WindowFlags, alt, delegate,
};

// ---------------------------------------------------------------------------
// Commands & help contexts (port of the cm*/hc* constants in TCV.PAS)
// ---------------------------------------------------------------------------

const CMD_INFO: Command = Command::custom("tcv.info");
const CMD_ABOUT: Command = Command::custom("tcv.about");

const HC_BROWSE_MODE: HelpCtx = HelpCtx::custom("tcv.browse_mode");
const HC_SEARCH_MODE: HelpCtx = HelpCtx::custom("tcv.search_mode");

// ---------------------------------------------------------------------------
// Catalog data (replaces reading PROGS.TFC)
// ---------------------------------------------------------------------------

/// One catalogued file, mirroring the six `"..."`-delimited fields of a
/// `PROGS.TFC` line (`disk`, `date`, `file`, `size`, `description`, `scan`).
struct Entry {
    /// Disk (volume) label.
    disk: &'static str,
    /// File date (as stamped on the disk).
    date: &'static str,
    /// File name.
    file: &'static str,
    /// Size in bytes.
    size: u32,
    /// Description / comment.
    desc: &'static str,
    /// Date the disk was scanned into the catalog.
    scan: &'static str,
}

/// A charming, period-appropriate early-90s shareware/utility floppy catalog.
static CATALOG: &[Entry] = &[
    Entry {
        disk: "GAMES01",
        date: "10-12-93",
        file: "DOOM1_0.ZIP",
        size: 2_311_046,
        desc: "id Software shareware DOOM v1.0",
        scan: "11-03-93",
    },
    Entry {
        disk: "GAMES01",
        date: "09-30-93",
        file: "WOLF3D.ZIP",
        size: 716_800,
        desc: "Wolfenstein 3D shareware episode 1",
        scan: "11-03-93",
    },
    Entry {
        disk: "GAMES02",
        date: "06-15-92",
        file: "COMMANDR.ZIP",
        size: 458_240,
        desc: "Commander Keen 4 shareware",
        scan: "11-03-93",
    },
    Entry {
        disk: "GAMES02",
        date: "03-21-93",
        file: "JAZZJACK.ZIP",
        size: 1_204_224,
        desc: "Jazz Jackrabbit demo",
        scan: "11-03-93",
    },
    Entry {
        disk: "UTILS03",
        date: "01-15-93",
        file: "PKZIP204.EXE",
        size: 199_245,
        desc: "PKWARE PKZIP/PKUNZIP v2.04g",
        scan: "11-04-93",
    },
    Entry {
        disk: "UTILS03",
        date: "08-02-92",
        file: "ARJ241.EXE",
        size: 121_734,
        desc: "ARJ archiver v2.41 by Robert Jung",
        scan: "11-04-93",
    },
    Entry {
        disk: "UTILS03",
        date: "11-11-91",
        file: "LHA213.EXE",
        size: 50_018,
        desc: "LHA compression utility v2.13",
        scan: "11-04-93",
    },
    Entry {
        disk: "UTILS04",
        date: "04-04-93",
        file: "4DOS502.ZIP",
        size: 412_900,
        desc: "4DOS command interpreter v5.02",
        scan: "11-04-93",
    },
    Entry {
        disk: "UTILS04",
        date: "07-19-92",
        file: "LIST92.ZIP",
        size: 60_416,
        desc: "Vernon Buerg's LIST file viewer",
        scan: "11-04-93",
    },
    Entry {
        disk: "GRAPHICS",
        date: "05-23-93",
        file: "FRACTINT.ZIP",
        size: 893_120,
        desc: "Stone Soup fractal generator v18",
        scan: "11-05-93",
    },
    Entry {
        disk: "GRAPHICS",
        date: "02-14-93",
        file: "VPIC61.ZIP",
        size: 147_456,
        desc: "VPIC image viewer v6.1",
        scan: "11-05-93",
    },
    Entry {
        disk: "GRAPHICS",
        date: "12-01-92",
        file: "POVRAY10.ZIP",
        size: 655_360,
        desc: "Persistence of Vision raytracer 1.0",
        scan: "11-05-93",
    },
    Entry {
        disk: "SOUND01",
        date: "03-30-93",
        file: "MODPLAY.ZIP",
        size: 73_728,
        desc: "ModEdit / MOD music player",
        scan: "11-05-93",
    },
    Entry {
        disk: "SOUND01",
        date: "06-06-93",
        file: "SBOS.ZIP",
        size: 98_304,
        desc: "Sound Blaster OS drivers",
        scan: "11-05-93",
    },
    Entry {
        disk: "COMMS02",
        date: "09-09-93",
        file: "TELIX321.ZIP",
        size: 524_288,
        desc: "Telix terminal program v3.21",
        scan: "11-06-93",
    },
    Entry {
        disk: "COMMS02",
        date: "07-07-92",
        file: "QMODEM46.ZIP",
        size: 466_944,
        desc: "Qmodem modem terminal v4.6",
        scan: "11-06-93",
    },
    Entry {
        disk: "PROGRAM",
        date: "01-30-93",
        file: "TPASCAL7.ZIP",
        size: 1_310_720,
        desc: "Borland Turbo Pascal 7.0 patches",
        scan: "11-06-93",
    },
    Entry {
        disk: "PROGRAM",
        date: "10-10-92",
        file: "DJGPP.ZIP",
        size: 2_097_152,
        desc: "DJ Delorie's GCC port for DOS",
        scan: "11-06-93",
    },
    Entry {
        disk: "EDITORS",
        date: "08-18-93",
        file: "QEDIT21.ZIP",
        size: 184_320,
        desc: "QEdit text editor v2.1",
        scan: "11-07-93",
    },
    Entry {
        disk: "EDITORS",
        date: "11-25-92",
        file: "VDE166.ZIP",
        size: 110_592,
        desc: "VDE WordStar-style editor v1.66",
        scan: "11-07-93",
    },
];

/// Format a catalog entry as one browser line — `TDiskCol.DirLine`: a leading
/// space, the disk label padded to 14, the file name padded to 15, then the
/// description.
fn dir_line(e: &Entry) -> String {
    format!(" {:<14}{:<15}{}", e.disk, e.file, e.desc)
}

/// The Info box body for entry `e` — the six labelled fields the original
/// `InfoBox` showed as static text lines (`TDirBox.HandleEvent.InfoBox`).
fn info_text(e: &Entry) -> String {
    format!(
        "Disk Label:  {}\nFile Name:   {}\nFile Date:   {}\n\
         Space Used:  {} Bytes\nDescription: {}\nScan Date:   {}",
        e.disk, e.file, e.date, e.size, e.desc, e.scan
    )
}

/// The `cmAbout` MessageBox body — the author's address homage, plus a re-port
/// note. `\x03` centers a line (the C++ `#3` center marker).
const ABOUT_TEXT: &str = "\x03CREATED in Nov '93 BY\n\n\
     \x03Tobias Oetiker\n\
     \x03Gallusstrasse 25\n\
     \x03CH-4600 Olten\n\
     \x03Switzerland\n\n\
     \x03eMail oetiker@stud.ee.ethz.ch\n\n\
     \x03USING Turbo Pascal 7.0 and Turbo Vision\n\n\
     \x03Re-ported to Rust with tvision-rs, 2026.";

/// Case-insensitive substring search — `NoCasePos`. Returns the byte position
/// (1-based, like Pascal's `Pos`) of `needle` in `haystack`, or 0 if absent. We
/// work in chars for the highlight math; here char index + 1.
fn no_case_pos(needle: &str, haystack: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    let h: Vec<char> = haystack.chars().map(|c| c.to_ascii_uppercase()).collect();
    let n: Vec<char> = needle.chars().map(|c| c.to_ascii_uppercase()).collect();
    if n.len() > h.len() {
        return 0;
    }
    for start in 0..=(h.len() - n.len()) {
        if h[start..start + n.len()] == n[..] {
            return start + 1;
        }
    }
    0
}

// ---------------------------------------------------------------------------
// DirBox — port of `TDirBox : TListBox`, the search-as-you-type catalog list.
//
// A `TListViewer` subtype (here implemented over `ListViewerState` + the
// `list_viewer` free functions, the trait realisation of the C++ abstract
// base). It overrides `draw` (to highlight the search match) and `handle_event`
// (the incremental substring search), keeping the base list nav for browse
// mode.
// ---------------------------------------------------------------------------

struct DirBox {
    lv: ListViewerState,
    /// The accumulated search string (`TDirBox.Search`). Empty = browse mode.
    search: String,
}

impl DirBox {
    fn new(bounds: Rect, h: Option<ViewId>, v: Option<ViewId>) -> Self {
        let mut lv = ListViewerState::new(bounds, 1, h, v);
        lv.range = CATALOG.len() as i32;
        lv.state.help_ctx = HC_BROWSE_MODE;
        DirBox {
            lv,
            search: String::new(),
        }
    }

    /// `TDiskCol.FindNext` — first index `>= start` whose line contains `key`
    /// (case-insensitive), or `start` if none.
    fn find_next(&self, start: i32, key: &str) -> i32 {
        let range = self.lv.range;
        if start >= 0 && start < range && !key.is_empty() {
            let mut i = start;
            while i < range {
                if no_case_pos(key, &self.get_text(i)) != 0 {
                    return i;
                }
                i += 1;
            }
            start
        } else {
            0
        }
    }

    /// `TDiskCol.FindPrev` — last index `< start` whose line contains `key`
    /// (case-insensitive), walking downwards; `start` if none / no key.
    fn find_prev(&self, start: i32, key: &str) -> i32 {
        if start >= 1 && !key.is_empty() {
            let mut i = start;
            while i >= 1 {
                i -= 1;
                if no_case_pos(key, &self.get_text(i)) != 0 {
                    return i;
                }
            }
            i
        } else {
            start
        }
    }

    /// Switch help context to match the current mode and refresh the status line
    /// (the window forwards our context via `get_help_ctx`).
    fn sync_mode(&mut self) {
        self.lv.state.help_ctx = if self.search.is_empty() {
            HC_BROWSE_MODE
        } else {
            HC_SEARCH_MODE
        };
    }

    /// Whether this view is the active/focused leaf (so search keys apply) —
    /// the port of the original's `Owner^.Phase = phFocused` guard.
    fn is_focused(&self) -> bool {
        self.lv.state.state.selected && self.lv.state.state.active
    }

    /// Open the Info box for the focused entry (the original `InfoBox`),
    /// rendered as an informational message box of labelled fields.
    fn open_info(&self, ctx: &mut Context) {
        if let Some(e) = CATALOG.get(self.lv.focused as usize) {
            ctx.request_message_box(
                info_text(e),
                MessageBoxKind::Information,
                MessageBoxButtons::ok(),
                None,
                None,
            );
        }
    }
}

impl ListViewer for DirBox {
    fn lv(&self) -> &ListViewerState {
        &self.lv
    }
    fn lv_mut(&mut self) -> &mut ListViewerState {
        &mut self.lv
    }
    fn get_text(&self, item: i32) -> String {
        CATALOG.get(item as usize).map(dir_line).unwrap_or_default()
    }
}

impl View for DirBox {
    fn state(&self) -> &ViewState {
        &self.lv.state
    }
    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.lv.state
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        // Without an active search this is just the base list draw.
        if self.search.is_empty() {
            list_viewer::draw(self, ctx);
            return;
        }

        // Search mode: base draw first (colors every row + the focused cell),
        // then overlay the matched substring of the focused row in a contrasting
        // style — the C++ `TDirBox.Draw` mark color (GetColor(5) == ListSelected).
        list_viewer::draw(self, ctx);

        let focused = self.lv.focused;
        let top = self.lv.top_item;
        let size = self.lv.state.size;
        let row = focused - top;
        if row < 0 || row >= size.y {
            return;
        }
        let line = self.get_text(focused);
        let pos = no_case_pos(&self.search, &line); // 1-based char index
        if pos == 0 {
            return;
        }
        let match_len = self.search.chars().count();
        let start = pos - 1;

        let active = self.is_focused();
        let base = ctx.style(if active {
            Role::ListFocused
        } else {
            Role::ListNormalInactive
        });
        let mark = ctx.style(Role::ListSelected);

        // The list draw renders text starting one column in (col 0 is the
        // cell's left pad); re-render the focused row in three styled spans so
        // the matched chars stand out.
        let pre: String = line.chars().take(start).collect();
        let hit: String = line.chars().skip(start).take(match_len).collect();
        let rest: String = line.chars().skip(start + match_len).collect();
        // `put_str` returns the *width* it drew (columns advanced), not the next
        // absolute column — so accumulate onto `x` to keep the three spans
        // contiguous (the base list draw starts text at column 1).
        let mut x = 1;
        x += ctx.put_str(x, row, &pre, base);
        x += ctx.put_str(x, row, &hit, mark);
        ctx.put_str(x, row, &rest, base);
        // The cursor lands just past the matched text via `cursor_request`.
    }

    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        // Info / About commands (from the buttons, broadcast) — open the
        // matching modal. The list is the natural handler for Info because it
        // owns the focused entry. (`TDirBox.HandleEvent` did the same via the
        // ofPostProcess command path.)
        if let Event::Broadcast { command, .. } = *ev {
            if command == CMD_INFO {
                self.open_info(ctx);
                ev.clear();
                return;
            } else if command == CMD_ABOUT {
                ctx.request_message_box(
                    ABOUT_TEXT.to_string(),
                    MessageBoxKind::Information,
                    MessageBoxButtons::ok(),
                    None,
                    None,
                );
                ev.clear();
                return;
            }
        }

        // Mouse: a double-click opens the Info box for the clicked entry.
        if let Event::MouseDown(me) = *ev
            && me.flags.double_click
        {
            let item = me.position.y + self.lv.top_item;
            if item >= 0 && item < self.lv.range {
                if item != self.lv.focused {
                    self.search.clear();
                    list_viewer::focus_item(self, item, ctx);
                }
                self.open_info(ctx);
                ev.clear();
                self.sync_mode();
                return;
            }
        }

        // Only run the search state machine when we are the focused leaf.
        if self.is_focused()
            && let Event::KeyDown(ke) = *ev
        {
            match ke.key {
                // Printable character: extend the search and jump to the next match.
                Key::Char(c) if !ke.modifiers.ctrl && !ke.modifiers.alt => {
                    let from = if self.search.is_empty() {
                        0
                    } else {
                        self.lv.focused
                    };
                    let mut probe = self.search.clone();
                    probe.push(c);
                    let found = self.find_next(from, &probe);
                    if no_case_pos(&probe, &self.get_text(found)) != 0 {
                        self.search = probe;
                        if found != self.lv.focused {
                            list_viewer::focus_item(self, found, ctx);
                        }
                    }
                    // No match: keep the old search and stay put (a gentle no-op,
                    // tidier than the original's error message box per keystroke).
                    ev.clear();
                    self.sync_mode();
                    return;
                }
                // Backspace: shorten the search, re-find from the top.
                Key::Backspace => {
                    if !self.search.is_empty() {
                        self.search.pop();
                        if !self.search.is_empty() {
                            let found = self.find_next(0, &self.search);
                            if found != self.lv.focused {
                                list_viewer::focus_item(self, found, ctx);
                            }
                        }
                    }
                    ev.clear();
                    self.sync_mode();
                    return;
                }
                // Enter: open the Info box for the focused entry.
                Key::Enter => {
                    self.open_info(ctx);
                    ev.clear();
                    return;
                }
                // Up/Down while searching: jump to the previous/next match.
                Key::Up if !self.search.is_empty() && self.lv.focused > 0 => {
                    let found = self.find_prev(self.lv.focused, &self.search.clone());
                    list_viewer::focus_item(self, found, ctx);
                    ev.clear();
                    self.sync_mode();
                    return;
                }
                Key::Down if !self.search.is_empty() && self.lv.focused < self.lv.range - 1 => {
                    let found = self.find_next(self.lv.focused + 1, &self.search.clone());
                    list_viewer::focus_item(self, found, ctx);
                    ev.clear();
                    self.sync_mode();
                    return;
                }
                // Esc, or any other navigation: leave search / browse mode.
                _ => {
                    if !self.search.is_empty() {
                        self.search.clear();
                        self.sync_mode();
                        if matches!(ke.key, Key::Esc) {
                            ev.clear();
                            return;
                        }
                    }
                }
            }
        }

        // Browse mode (or unconsumed keys): the base list nav + scrollbar sync.
        list_viewer::handle_event(self, ev, ctx);
    }

    fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
        list_viewer::set_state(self, flag, enable, ctx);
    }

    fn cursor_request(&self) -> Option<Point> {
        let base = list_viewer::focused_cursor(self)?;
        if self.search.is_empty() {
            return Some(base);
        }
        // In search mode, place the cursor just past the matched substring.
        let line = self.get_text(self.lv.focused);
        let pos = no_case_pos(&self.search, &line);
        if pos == 0 {
            return Some(base);
        }
        let end = (pos - 1) + self.search.chars().count();
        Some(Point::new(base.x - 1 + end as i32, base.y))
    }

    fn apply_list_scroll(&mut self, h: Option<i32>, v: Option<i32>, ctx: &mut Context) {
        list_viewer::apply_scroll(self, h, v, ctx);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
}

// ---------------------------------------------------------------------------
// DataWindow — port of `TDataWin : TDialog`, the full-desktop catalog window.
//
// Holds the header label, the search list, its scrollbar, and the three
// buttons. It forwards the list's help context (browse/search) up to the status
// line via `get_help_ctx`, and turns a `CMD_INFO` broadcast into an Info box.
//
// The original TCV.PAS set `Window^.Flags := $00; GrowMode := $00` to make
// this a fixed, icon-less panel (no close/zoom icons, not movable). This is
// now faithful via the public `with_flags`/`with_grow_mode` API (landed as
// part of the consumer-API coverage axis).
// ---------------------------------------------------------------------------

struct DataWindow {
    dialog: Dialog,
}

impl DataWindow {
    fn new(bounds: Rect) -> Self {
        let mut dialog = Dialog::new(bounds, Some("Tobis Catalog Vision Version 2.2".to_string()))
            .with_flags(WindowFlags::default()) // TCV: Flags := $00 (fixed, no icons)
            .with_grow_mode(GrowMode::default()); // TCV: GrowMode := $00

        let inner = bounds; // dialog-local coords start at (0,0)
        let w = inner.b.x - inner.a.x;
        let h = inner.b.y - inner.a.y;

        // Buttons along the bottom row, mirroring the original layout.
        let btn_y = h - 3;
        dialog.insert_child(Box::new(Button::new(
            Rect::new(w - 45, btn_y, w - 33, btn_y + 2),
            "~I~nfo",
            CMD_INFO,
            ButtonFlags {
                broadcast: true,
                ..ButtonFlags::new()
            },
        )));
        dialog.insert_child(Box::new(Button::new(
            Rect::new(w - 30, btn_y, w - 18, btn_y + 2),
            "~A~bout",
            CMD_ABOUT,
            ButtonFlags {
                broadcast: true,
                ..ButtonFlags::new()
            },
        )));
        dialog.insert_child(Box::new(Button::new(
            Rect::new(w - 15, btn_y, w - 3, btn_y + 2),
            "E~x~it",
            Command::QUIT,
            ButtonFlags::new(),
        )));

        // The scrollbar lives on the right edge of the list area.
        let list_rect = Rect::new(2, 2, w - 2, h - 4);
        let sb = ScrollBar::new(Rect::new(w - 2, 2, w - 1, h - 4));
        let sb_id = dialog.insert_child(Box::new(sb));

        let list = DirBox::new(list_rect, None, Some(sb_id));
        let list_id = dialog.insert_child(Box::new(list));

        // Header label, linked to the list (Alt-D focuses it).
        dialog.insert_child(Box::new(Label::new(
            Rect::new(3, 1, w - 2, 2),
            "~D~isk          File Name      Comment",
            Some(list_id),
        )));

        DataWindow { dialog }
    }
}

#[delegate(to = dialog)]
impl View for DataWindow {
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
    // handle_event is forwarded by #[delegate(to = dialog)].
    // Axis C.1 (Group::get_help_ctx bubble + program idle-path) means the
    // focused DirBox's help_ctx now reaches the status line automatically —
    // no manual cache into the dialog's state is needed.
}

// ---------------------------------------------------------------------------
// TcvApp — port of `TTCV : TApplication`.
// ---------------------------------------------------------------------------

struct TcvApp {
    program: Program,
}

impl TcvApp {
    fn new(backend: Box<dyn Backend>) -> Self {
        let mut program = Program::new(
            backend,
            Box::new(SystemClock::new()),
            Theme::classic_blue(),
            Self::init_desktop,
            Self::init_status_line,
            |r| {
                // No menu bar (the original's InitMenuBar is empty); pin a
                // zero-height bar so the desktop fills from the top.
                let mut r = r;
                r.b.y = r.a.y;
                let _ = r;
                None
            },
        );
        let r = program.desktop_rect();
        program.desktop_insert(Box::new(DataWindow::new(r)));
        TcvApp { program }
    }

    fn init_desktop(r: Rect) -> Option<Box<dyn View>> {
        let mut r = r;
        r.b.y -= 1; // above the status line
        Some(Box::new(Desktop::new(r, |br| {
            Some(Desktop::init_background(br))
        })))
    }

    /// `TTCVStatLine` — context-sensitive hints keyed on the browse/search mode.
    fn init_status_line(r: Rect) -> Option<Box<dyn View>> {
        let mut r = r;
        r.a.y = r.b.y - 1;
        let defs = StatusDef::list()
            .def_all(|d| d.item("~Alt-X~ Exit", alt('x'), Command::QUIT))
            .build();
        let line = StatusLine::new(r, defs).with_hint(|ctx| {
            if ctx == HC_SEARCH_MODE {
                Some(
                    "SEARCH MODE: [UP],[DOWN] for Next Match; Continue typing; [ESC] to Browse Mode"
                        .to_string(),
                )
            } else {
                Some(
                    "BROWSE MODE: Use [UP],[DOWN] to Browse or Enter a Word you are looking for."
                        .to_string(),
                )
            }
        });
        Some(Box::new(line))
    }

    fn run(&mut self) {
        // The Info / About modals and Exit are all handled within the view tree
        // (the buttons broadcast their commands; the list opens the boxes via
        // the async-modal-from-a-view seam, and Exit is the standard cmQuit), so
        // the application-level command hook is empty.
        self.program.run_app(|_prog, _cmd| {});
    }
}

fn main() -> io::Result<()> {
    let mut app = TcvApp::new(Box::new(CrosstermBackend::new()?));
    app.run();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tvision_rs::HeadlessBackend;
    use tvision_rs::KeyModifiers;

    /// Smoke test: the whole app constructs on a headless backend, renders a
    /// frame, accepts a search keystroke, and pumps several frames without
    /// panicking. Also checks the dir-line / search helpers directly.
    #[test]
    fn constructs_and_renders_without_panic() {
        let (backend, screen) = HeadlessBackend::new(80, 25);
        let mut app = TcvApp::new(Box::new(backend));

        // One frame: the catalog window draws.
        app.program.pump_once();
        let frame = screen.snapshot();
        assert!(
            frame.contains("Tobis Catalog Vision"),
            "title should render; got:\n{frame}"
        );

        // Type a search and pump it through — must not panic and should jump to
        // a matching entry.
        for c in "doom".chars() {
            screen.push_key(Key::Char(c), KeyModifiers::default());
            app.program.pump_once();
        }
        let frame = screen.snapshot();
        assert!(
            frame.to_lowercase().contains("doom"),
            "search should surface the DOOM entry; got:\n{frame}"
        );

        // Backspace + Esc must also pump cleanly.
        screen.push_key(Key::Backspace, KeyModifiers::default());
        app.program.pump_once();
        screen.push_key(Key::Esc, KeyModifiers::default());
        app.program.pump_once();
    }

    /// Regression guard for the search-overlay column bug: `put_str` returns the
    /// *width* it drew (not the next absolute column), so the highlight overlay
    /// must accumulate `x` (`x += put_str(...)`). When it used `x = put_str(...)`
    /// the focused row was mangled (the `rest` span landed ~14 columns left). We
    /// focus the list, type a search, and assert the matched row still renders
    /// its full, contiguous text.
    #[test]
    fn search_does_not_corrupt_focused_row() {
        use tvision_rs::{MouseButtons, MouseEvent};
        let (backend, screen) = HeadlessBackend::new(80, 25);
        let mut app = TcvApp::new(Box::new(backend));
        app.program.pump_once();

        // The WOLF3D row, rendered verbatim (file padded to 15, then the comment).
        let intact = "WOLF3D.ZIP     Wolfenstein 3D shareware episode 1";
        assert!(
            screen.snapshot().contains(intact),
            "row should render intact in browse mode"
        );

        // Click a list row to focus the list.  Send MouseDown + MouseUp to
        // release the mouse-track capture before typing; while the capture is
        // live (between Down and Up) keyboard events are swallowed by the
        // hold handler.
        screen.push_event(Event::MouseDown(MouseEvent {
            position: Point::new(10, 5),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        }));
        app.program.pump_once();
        screen.push_event(Event::MouseUp(MouseEvent {
            position: Point::new(10, 5),
            buttons: MouseButtons::default(),
            ..Default::default()
        }));
        app.program.pump_once();

        // After focusing the list the DirBox is in browse mode; the bubble
        // (DirBox.help_ctx → Group::get_help_ctx → status-line idle path)
        // must surface "BROWSE MODE" on the status line.
        {
            let frame = screen.snapshot();
            assert!(
                frame.contains("BROWSE MODE"),
                "status line should show BROWSE MODE after focusing the list; got:\n{frame}"
            );
        }

        // Type a search for "wolf" — now that the hold is released, keys reach
        // the DirBox and activate search mode.
        for c in "wolf".chars() {
            screen.push_key(Key::Char(c), KeyModifiers::default());
            app.program.pump_once();
        }
        // Drain deferred broadcasts (RECEIVED_FOCUS etc.) then get one true
        // idle pump: the status-line idle arm reads group.get_help_ctx() only
        // when out_events is empty.  Each key pump may leave 1-2 broadcasts;
        // 8 extra pumps is conservative.
        for _ in 0..8 {
            app.program.pump_once();
        }

        // Search is active: status line must now show SEARCH MODE.
        {
            let frame = screen.snapshot();
            assert!(
                frame.contains("SEARCH MODE"),
                "status line should show SEARCH MODE while searching; got:\n{frame}"
            );
        }

        // The focused row's text is still contiguous — not shifted/duplicated.
        assert!(
            screen.snapshot().contains(intact),
            "search overlay must not corrupt the focused row; got:\n{}",
            screen.snapshot()
        );
    }

    #[test]
    fn dir_line_and_search_helpers() {
        let e = &CATALOG[0];
        let line = dir_line(e);
        assert!(line.starts_with(' '));
        assert!(line.contains(e.disk) && line.contains(e.file) && line.contains(e.desc));

        // Case-insensitive substring search (NoCasePos), 1-based.
        assert_eq!(no_case_pos("doom", "  DOOM1_0.ZIP"), 3);
        assert_eq!(no_case_pos("zzz", "abc"), 0);
        assert_eq!(no_case_pos("", "abc"), 0);
    }
}
