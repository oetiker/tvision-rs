//! `tvdemo` — a faithful port of the magiblot/tvision `tvdemo` example.
//!
//! Demonstrates the Turbo Vision widget set with:
//! - A sliding tile puzzle
//! - A calendar view
//! - An ASCII chart
//! - A simple calculator
//! - An event viewer
//! - A file viewer
//! - A background pattern changer
//!
//! Run it: `cargo run --example tvdemo [file ...]`
//!   - `Alt-X` or File → Exit quits.
//!   - `F10` opens the menu.
//!   - `F4` cycles the active window: normal → frameless (fills the desktop) →
//!     fullscreen (covers the menu, which collapses to a `⋮` kebab) → normal.

use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use tvision_rs::{
    Backend, Button, ButtonFlags, ButtonRowAlign, Color, ColorPicker, Command, Constraints,
    CrosstermBackend, Desktop, Dialog, DrawCtx, Event, FindMode, GrowMode, Key, KeyEvent,
    KeyModifiers, ListBox, Menu, MenuBar, Program, Rect, Role, ScrollBarOptions, Scroller,
    Splitter, StaticText, StatusDef, StatusLine, SystemClock, Tab, Theme, View, ViewId, ViewState,
    Window, WindowFlags, alt, delegate,
};

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

const CMD_ABOUT: Command = Command::custom("tvdemo.about");
const CMD_PUZZLE: Command = Command::custom("tvdemo.puzzle");
const CMD_CALENDAR: Command = Command::custom("tvdemo.calendar");
const CMD_ASCII: Command = Command::custom("tvdemo.ascii");
const CMD_CALC: Command = Command::custom("tvdemo.calc");
const CMD_EVENT_VIEW: Command = Command::custom("tvdemo.eventview");
const CMD_CH_BG: Command = Command::custom("tvdemo.chbg");
const CMD_OPEN: Command = Command::custom("tvdemo.open");
const CALC_BUTTON: Command = Command::custom("tvdemo.calcbtn");
const ASCII_FOCUSED: Command = Command::custom("tvdemo.asciifocused");
const CMD_FND_EV_VIEW: Command = Command::custom("tvdemo.fndevview");
const CMD_COLORS: Command = Command::custom("tvdemo.colors");
const CMD_SPLIT: Command = Command::custom("tvdemo.split");

// ---------------------------------------------------------------------------
// Key helpers
// ---------------------------------------------------------------------------

fn alt0() -> KeyEvent {
    KeyEvent::new(
        Key::Char('0'),
        KeyModifiers {
            alt: true,
            ..Default::default()
        },
    )
}
fn ctrl_f5() -> KeyEvent {
    KeyEvent::new(
        Key::F(5),
        KeyModifiers {
            ctrl: true,
            ..Default::default()
        },
    )
}
fn shift_f6() -> KeyEvent {
    KeyEvent::new(
        Key::F(6),
        KeyModifiers {
            shift: true,
            ..Default::default()
        },
    )
}
fn alt_f3() -> KeyEvent {
    KeyEvent::new(
        Key::F(3),
        KeyModifiers {
            alt: true,
            ..Default::default()
        },
    )
}

// ---------------------------------------------------------------------------
// PuzzleView — TPuzzleView port
// ---------------------------------------------------------------------------

/// The 4x4 sliding tile puzzle board.
///
/// Tiles A-O plus a blank. Arrow keys slide adjacent tiles into the blank.
/// On win the board scrambles again.
struct PuzzleView {
    st: ViewState,
    board: [[char; 4]; 4],
    moves: i32,
    solved: bool,
}

const BOARD_START: [char; 16] = [
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', ' ',
];

/// Colour map — which tiles use highlight colour (alternating chequerboard).
const TILE_MAP: [bool; 15] = [
    false, true, false, true, true, false, true, false, false, true, false, true, true, false, true,
];

const SOLUTION: [char; 16] = [
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', ' ',
];

impl PuzzleView {
    fn new(bounds: Rect) -> Self {
        let mut st = ViewState::new(bounds);
        st.options.selectable = true;
        let mut board = [[' '; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                board[i][j] = BOARD_START[i * 4 + j];
            }
        }
        let mut pv = PuzzleView {
            st,
            board,
            moves: 0,
            solved: false,
        };
        pv.scramble();
        pv
    }

    /// Find the blank tile and return its (row, col).
    fn blank_pos(&self) -> (usize, usize) {
        for i in 0..4 {
            for j in 0..4 {
                if self.board[i][j] == ' ' {
                    return (i, j);
                }
            }
        }
        (3, 3)
    }

    fn move_key(&mut self, key: Key) {
        let (y, x) = self.blank_pos();
        match key {
            Key::Down if y > 0 => {
                self.board[y][x] = self.board[y - 1][x];
                self.board[y - 1][x] = ' ';
                if self.moves < 1000 {
                    self.moves += 1;
                }
            }
            Key::Up if y < 3 => {
                self.board[y][x] = self.board[y + 1][x];
                self.board[y + 1][x] = ' ';
                if self.moves < 1000 {
                    self.moves += 1;
                }
            }
            Key::Right if x > 0 => {
                self.board[y][x] = self.board[y][x - 1];
                self.board[y][x - 1] = ' ';
                if self.moves < 1000 {
                    self.moves += 1;
                }
            }
            Key::Left if x < 3 => {
                self.board[y][x] = self.board[y][x + 1];
                self.board[y][x + 1] = ' ';
                if self.moves < 1000 {
                    self.moves += 1;
                }
            }
            _ => {}
        }
    }

    fn scramble(&mut self) {
        // Reset the board to the solved state first.
        for i in 0..4 {
            for j in 0..4 {
                self.board[i][j] = BOARD_START[i * 4 + j];
            }
        }
        self.moves = 0;
        self.solved = false;
        // Make 200 random moves.
        let mut rng: u64 = 12345;
        let mut n = 0;
        while n <= 200 {
            rng = rng
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let dir = (rng >> 33) as usize % 4;
            let key = [Key::Up, Key::Down, Key::Left, Key::Right][dir];
            self.move_key(key);
            n += 1;
        }
        self.moves = 0;
    }

    fn win_check(&mut self) {
        for i in 0..4 {
            for j in 0..4 {
                if self.board[i][j] != SOLUTION[i * 4 + j] {
                    return;
                }
            }
        }
        self.solved = true;
    }
}

impl View for PuzzleView {
    fn state(&self) -> &ViewState {
        &self.st
    }
    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.st
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        // C++ cpPuzzlePalette "\x06\x07": getColor(1) → normal, getColor(2) →
        // highlight — the same chain tvision-rs exposes as ScrollerNormal/ScrollerSelected.
        let normal = ctx.style(Role::ScrollerNormal);
        let highlight = ctx.style(Role::ScrollerSelected);

        for i in 0..4i32 {
            // Fill row background.
            ctx.fill(Rect::new(0, i, 18, i + 1), ' ', normal);
            // Show "Move" and count on rows 1-2.
            if i == 1 {
                ctx.put_str(13, i, "Move", normal);
            }
            if i == 2 {
                let s = format!("{}", self.moves);
                ctx.put_str(14, i, &s, normal);
            }
            for j in 0..4i32 {
                let tile = self.board[i as usize][j as usize];
                let label = format!(" {} ", tile);
                let style = if tile == ' ' {
                    normal
                } else {
                    let idx = (tile as u8 - b'A') as usize;
                    if idx < 15 && TILE_MAP[idx] {
                        highlight
                    } else {
                        normal
                    }
                };
                ctx.put_str(j * 3, i, &label, style);
            }
        }
    }

    fn handle_event(&mut self, ev: &mut Event, ctx: &mut tvision_rs::Context) {
        // If solved: any key/mouse rescrambles.
        if self.solved {
            let triggered = matches!(
                ev,
                Event::KeyDown(_) | Event::MouseDown(_) | Event::MouseAuto(_)
            );
            if triggered {
                self.scramble();
                ev.clear();
                return;
            }
        }

        match ev {
            Event::MouseDown(me) => {
                // Map click to adjacent blank move.
                let pos = me.position;
                let (by, bx) = self.blank_pos();
                let cx = (pos.x / 3) as usize;
                let cy = pos.y as usize;
                let delta = (cy as i32 * 4 + cx as i32) - (by as i32 * 4 + bx as i32);
                let key = match delta {
                    -4 => Some(Key::Down),
                    -1 => Some(Key::Right),
                    1 => Some(Key::Left),
                    4 => Some(Key::Up),
                    _ => None,
                };
                if let Some(k) = key {
                    self.move_key(k);
                }
                ev.clear();
                self.win_check();
            }
            Event::KeyDown(ke) => {
                match ke.key {
                    Key::Up | Key::Down | Key::Left | Key::Right => {
                        let k = ke.key;
                        self.move_key(k);
                        ev.clear();
                        self.win_check();
                    }
                    _ => {}
                }
                let _ = ctx;
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// PuzzleWindow — TPuzzleWindow port
// ---------------------------------------------------------------------------

struct PuzzleWindow {
    window: Window,
}

impl PuzzleWindow {
    fn new() -> Self {
        let mut window = Window::new(Rect::new(1, 1, 21, 7), Some("Puzzle".to_string()), 0);
        // Disable grow and zoom — set_flags syncs both Window and its Frame.
        window.set_flags(WindowFlags {
            r#move: true,
            grow: false,
            close: true,
            zoom: false,
        });
        window.state_mut().grow_mode = Default::default();

        let ext = window.state().get_extent();
        let mut r = ext;
        r.grow(-1, -1);
        window.insert_child(Box::new(PuzzleView::new(r)));
        PuzzleWindow { window }
    }
}

#[delegate(to = window)]
impl View for PuzzleWindow {}

// ---------------------------------------------------------------------------
// AsciiTable — TTable port
// ---------------------------------------------------------------------------

/// The ASCII grid: 32 columns × 8 rows showing all 256 characters.
struct AsciiTable {
    st: ViewState,
    cursor_x: i32,
    cursor_y: i32,
}

impl AsciiTable {
    fn new(bounds: Rect) -> Self {
        let mut st = ViewState::new(bounds);
        st.options.selectable = true;
        // C++ ctor: blockCursor(); draw() ends with showCursor(). The selected
        // cell is shown via the hardware cursor, not a colour highlight.
        st.block_cursor();
        st.show_cursor();
        AsciiTable {
            st,
            cursor_x: 0,
            cursor_y: 0,
        }
    }

    fn char_at_cursor(&self) -> u8 {
        (self.cursor_y * 32 + self.cursor_x) as u8
    }

    fn size(&self) -> (i32, i32) {
        let ext = self.st.get_extent();
        (ext.b.x, ext.b.y)
    }
}

impl View for AsciiTable {
    fn state(&self) -> &ViewState {
        &self.st
    }
    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.st
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        let color = ctx.style(Role::ScrollerNormal);
        let (sx, sy) = self.size();
        for y in 0..sy {
            ctx.fill(Rect::new(0, y, sx, y + 1), ' ', color);
            for x in 0..sx {
                let ch_byte = (32 * y + x) as u8;
                // Control codes (0–31) have no printable Unicode glyph; show '.'.
                let ch = if ch_byte < 32 { '.' } else { ch_byte as char };
                let s = ch.to_string();
                ctx.put_str(x, y, &s, color);
            }
        }
        // showCursor(): place the hardware cursor on the selected cell.
        self.st.set_cursor(self.cursor_x, self.cursor_y);
    }

    fn handle_event(&mut self, ev: &mut Event, ctx: &mut tvision_rs::Context) {
        let (sx, sy) = self.size();
        match ev {
            Event::MouseDown(me) => {
                let pos = me.position;
                if pos.x >= 0 && pos.x < sx && pos.y >= 0 && pos.y < sy {
                    self.cursor_x = pos.x;
                    self.cursor_y = pos.y;
                    let source = self.st.id();
                    ctx.broadcast(ASCII_FOCUSED, source);
                }
                ev.clear();
            }
            Event::KeyDown(ke) => {
                let consumed = match ke.key {
                    Key::Home => {
                        self.cursor_x = 0;
                        self.cursor_y = 0;
                        true
                    }
                    Key::End => {
                        self.cursor_x = sx - 1;
                        self.cursor_y = sy - 1;
                        true
                    }
                    Key::Up => {
                        if self.cursor_y > 0 {
                            self.cursor_y -= 1;
                        }
                        true
                    }
                    Key::Down => {
                        if self.cursor_y < sy - 1 {
                            self.cursor_y += 1;
                        }
                        true
                    }
                    Key::Left => {
                        if self.cursor_x > 0 {
                            self.cursor_x -= 1;
                        }
                        true
                    }
                    Key::Right => {
                        if self.cursor_x < sx - 1 {
                            self.cursor_x += 1;
                        }
                        true
                    }
                    _ => false,
                };
                if consumed {
                    let source = self.st.id();
                    ctx.broadcast(ASCII_FOCUSED, source);
                    ev.clear();
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// AsciiReport — TReport port
// ---------------------------------------------------------------------------

/// Shows the decimal/hex value of the currently focused ASCII character.
struct AsciiReport {
    st: ViewState,
    ascii_char: u8,
}

impl AsciiReport {
    fn new(bounds: Rect) -> Self {
        let mut st = ViewState::new(bounds);
        st.options.framed = true;
        AsciiReport { st, ascii_char: 0 }
    }
}

impl View for AsciiReport {
    fn state(&self) -> &ViewState {
        &self.st
    }
    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.st
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        let color = ctx.style(Role::ScrollerNormal);
        let ext = self.st.get_extent();
        ctx.fill(ext, ' ', color);
        let display_ch = if self.ascii_char == 0 {
            ' '
        } else {
            self.ascii_char as char
        };
        let s = format!(
            "  Char: {} Decimal: {:3} Hex {:02X}",
            display_ch, self.ascii_char, self.ascii_char
        );
        ctx.put_str(0, 0, &s, color);
    }

    fn handle_event(&mut self, ev: &mut Event, _ctx: &mut tvision_rs::Context) {
        if let Event::Broadcast { command, .. } = ev {
            // Updates are managed by AsciiWindow which sets ascii_char directly.
            let _ = command;
        }
    }
}

// ---------------------------------------------------------------------------
// AsciiWindow — TAsciiChart port
// ---------------------------------------------------------------------------

struct AsciiWindow {
    window: Window,
    table_id: ViewId,
    report_id: ViewId,
}

impl AsciiWindow {
    fn new() -> Self {
        // Window size: 34 × 12 (32 chars wide + frame, 8 rows + report + frame).
        let mut window = Window::new(Rect::new(0, 0, 34, 12), Some("ASCII Chart".to_string()), 0);
        // Disable grow and zoom — set_flags syncs both Window and its Frame.
        window.set_flags(WindowFlags {
            r#move: true,
            grow: false,
            close: true,
            zoom: false,
        });
        window.state_mut().grow_mode = Default::default();
        window.state_mut().options.center_x = true;
        window.state_mut().options.center_y = true;

        // Report line at bottom.
        let ext = window.state().get_extent();
        let report_r = Rect::new(1, ext.b.y - 2, ext.b.x - 1, ext.b.y - 1);
        let report = AsciiReport::new(report_r);
        let report_id = window.insert_child(Box::new(report));

        // Table fills top area.
        let table_r = Rect::new(1, 1, ext.b.x - 1, ext.b.y - 2);
        let table = AsciiTable::new(table_r);
        let table_id = window.insert_child(Box::new(table));

        AsciiWindow {
            window,
            table_id,
            report_id,
        }
    }
}

#[delegate(to = window)]
impl View for AsciiWindow {
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut tvision_rs::Context) {
        self.window.handle_event(ev, ctx);

        // Sync the report when a broadcast arrives.
        if let Event::Broadcast { command, .. } = ev
            && *command == ASCII_FOCUSED
        {
            // Get current char from the table.
            let ch = self
                .window
                .child_mut(self.table_id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<AsciiTable>())
                .map(|t| t.char_at_cursor())
                .unwrap_or(0);
            // Update the report.
            if let Some(report) = self
                .window
                .child_mut(self.report_id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<AsciiReport>())
            {
                report.ascii_char = ch;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CalendarView — TCalendarView port
// ---------------------------------------------------------------------------

const MONTH_NAMES: [&str; 13] = [
    "",
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const DAYS_IN_MONTH: [i32; 13] = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn day_of_week(day: i32, month: i32, year: i32) -> i32 {
    let (month, year) = if month < 3 {
        (month + 10, year - 1)
    } else {
        (month - 2, year)
    };
    let century = year / 100;
    let yr = year % 100;
    let dw = ((26 * month - 2) / 10 + day + yr + yr / 4 + century / 4 - 2 * century) % 7;
    if dw < 0 { dw + 7 } else { dw }
}

struct CalendarView {
    st: ViewState,
    month: i32,
    year: i32,
    cur_day: i32,
    cur_month: i32,
    cur_year: i32,
}

impl CalendarView {
    fn new(bounds: Rect) -> Self {
        let mut st = ViewState::new(bounds);
        st.options.selectable = true;
        let now = chrono_now();
        CalendarView {
            st,
            month: now.0,
            year: now.1,
            cur_day: now.2,
            cur_month: now.0,
            cur_year: now.1,
        }
    }

    fn prev_month(&mut self) {
        self.month -= 1;
        if self.month < 1 {
            self.month = 12;
            self.year -= 1;
        }
    }

    fn next_month(&mut self) {
        self.month += 1;
        if self.month > 12 {
            self.month = 1;
            self.year += 1;
        }
    }

    fn days_this_month(&self) -> i32 {
        DAYS_IN_MONTH[self.month as usize]
            + if self.month == 2 && is_leap(self.year) {
                1
            } else {
                0
            }
    }
}

/// Returns (month 1-12, year, day).
fn chrono_now() -> (i32, i32, i32) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Simple Gregorian calendar calculation.
    let days_since_epoch = secs / 86400;
    // Unix epoch is 1970-01-01 (Thursday).
    let mut year = 1970i32;
    let mut remaining = days_since_epoch as i32;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }
    let mut month = 1i32;
    loop {
        let dim = DAYS_IN_MONTH[month as usize] + if month == 2 && is_leap(year) { 1 } else { 0 };
        if remaining < dim {
            break;
        }
        remaining -= dim;
        month += 1;
    }
    let day = remaining + 1;
    (month, year, day)
}

impl View for CalendarView {
    fn state(&self) -> &ViewState {
        &self.st
    }
    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.st
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        let color = ctx.style(Role::ScrollerNormal);
        let bold = ctx.style(Role::StatusShortcut);
        let ext = self.st.get_extent();

        // Header line: "  September 2026 ▲  ▼ "
        ctx.fill(Rect::new(0, 0, ext.b.x, 1), ' ', color);
        // Faithful layout: 9-wide month, year, then ▲ at col 15 and ▼ at col 18
        // (two spaces between) so they line up with the click targets below.
        let header = format!(
            "{:>9} {:4} {}  {} ",
            MONTH_NAMES[self.month as usize], self.year, '▲', '▼'
        );
        ctx.put_str(0, 0, &header, color);

        // Day-of-week header.
        ctx.fill(Rect::new(0, 1, ext.b.x, 2), ' ', color);
        ctx.put_str(0, 1, "Su Mo Tu We Th Fr Sa", color);

        // Calendar grid.
        let start_dow = day_of_week(1, self.month, self.year);
        let mut current = 1 - start_dow;
        let days = self.days_this_month();

        for row in 0..6i32 {
            ctx.fill(Rect::new(0, row + 2, ext.b.x, row + 3), ' ', color);
            for col in 0..7i32 {
                if current >= 1 && current <= days {
                    let is_today = self.year == self.cur_year
                        && self.month == self.cur_month
                        && current == self.cur_day;
                    let style = if is_today { bold } else { color };
                    let s = format!("{:2}", current);
                    ctx.put_str(col * 3, row + 2, &s, style);
                }
                current += 1;
            }
        }
    }

    fn handle_event(&mut self, ev: &mut Event, _ctx: &mut tvision_rs::Context) {
        match ev {
            Event::MouseDown(me) => {
                let pos = me.position;
                // Row 0: ▲ at x=15, ▼ at x=18 (approximate).
                if pos.y == 0 {
                    if pos.x >= 15 && pos.x <= 17 {
                        self.next_month();
                        ev.clear();
                    } else if pos.x >= 18 && pos.x <= 20 {
                        self.prev_month();
                        ev.clear();
                    }
                }
            }
            Event::KeyDown(ke) => match ke.key {
                Key::Up => {
                    self.prev_month();
                    ev.clear();
                }
                Key::Down => {
                    self.next_month();
                    ev.clear();
                }
                Key::Char('+') => {
                    self.next_month();
                    ev.clear();
                }
                Key::Char('-') => {
                    self.prev_month();
                    ev.clear();
                }
                _ => {}
            },
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// CalendarWindow — TCalendarWindow port
// ---------------------------------------------------------------------------

struct CalendarWindow {
    window: Window,
}

impl CalendarWindow {
    fn new() -> Self {
        let mut window = Window::new(Rect::new(1, 1, 24, 12), Some("Calendar".to_string()), 0);
        // Disable grow and zoom — set_flags syncs both Window and its Frame.
        window.set_flags(WindowFlags {
            r#move: true,
            grow: false,
            close: true,
            zoom: false,
        });
        window.state_mut().grow_mode = Default::default();

        let ext = window.state().get_extent();
        let mut r = ext;
        r.grow(-1, -1);
        window.insert_child(Box::new(CalendarView::new(r)));
        CalendarWindow { window }
    }
}

#[delegate(to = window)]
impl View for CalendarWindow {}

// ---------------------------------------------------------------------------
// CalcDisplay — TCalcDisplay port
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum CalcStatus {
    First,
    Valid,
    Error,
}

struct CalcDisplay {
    st: ViewState,
    status: CalcStatus,
    number: String,
    sign: char,
    operate: char,
    operand: f64,
}

const DISPLAY_LEN: usize = 15;

impl CalcDisplay {
    fn new(bounds: Rect) -> Self {
        let mut st = ViewState::new(bounds);
        st.options.selectable = true;
        let mut d = CalcDisplay {
            st,
            status: CalcStatus::First,
            number: String::new(),
            sign: ' ',
            operate: '=',
            operand: 0.0,
        };
        d.clear();
        d
    }

    fn clear(&mut self) {
        self.status = CalcStatus::First;
        self.number = "0".to_string();
        self.sign = ' ';
        self.operate = '=';
        self.operand = 0.0;
    }

    fn error(&mut self) {
        self.status = CalcStatus::Error;
        self.number = "Error".to_string();
        self.sign = ' ';
    }

    fn check_first(&mut self) {
        if self.status == CalcStatus::First {
            self.status = CalcStatus::Valid;
            self.number = "0".to_string();
            self.sign = ' ';
        }
    }

    fn get_display(&self) -> f64 {
        self.number.parse::<f64>().unwrap_or(0.0)
    }

    fn set_display(&mut self, r: f64) {
        if r < 0.0 {
            self.sign = '-';
            let s = format!("{}", -r);
            if s.len() > DISPLAY_LEN {
                self.error();
            } else {
                self.number = s;
            }
        } else {
            self.sign = ' ';
            let s = format!("{}", r);
            if s.len() > DISPLAY_LEN {
                self.error();
            } else {
                self.number = s;
            }
        }
    }

    fn calc_key(&mut self, key: char) {
        let key = key.to_ascii_uppercase();
        let key = if self.status == CalcStatus::Error && key != 'C' {
            ' '
        } else {
            key
        };

        match key {
            '0'..='9' => {
                self.check_first();
                if self.number.len() < DISPLAY_LEN {
                    if self.number == "0" {
                        self.number.clear();
                    }
                    self.number.push(key);
                }
            }
            '.' => {
                self.check_first();
                if !self.number.contains('.') {
                    self.number.push('.');
                }
            }
            '\x08' | '\x1b' => {
                // Backspace or escape.
                self.check_first();
                if self.number.len() == 1 {
                    self.number = "0".to_string();
                } else {
                    self.number.pop();
                }
            }
            '_' => {
                if self.sign == ' ' {
                    self.sign = '-';
                } else {
                    self.sign = ' ';
                }
            }
            '+' | '-' | '*' | '/' | '=' | '%' | '\r' => {
                if self.status == CalcStatus::Valid {
                    self.status = CalcStatus::First;
                    let r = self.get_display() * if self.sign == '-' { -1.0 } else { 1.0 };
                    let r = if key == '%' {
                        if self.operate == '+' || self.operate == '-' {
                            (self.operand * r) / 100.0
                        } else {
                            r / 100.0
                        }
                    } else {
                        r
                    };
                    match self.operate {
                        '+' => self.set_display(self.operand + r),
                        '-' => self.set_display(self.operand - r),
                        '*' => self.set_display(self.operand * r),
                        '/' => {
                            if r == 0.0 {
                                self.error();
                            } else {
                                self.set_display(self.operand / r);
                            }
                        }
                        _ => {}
                    }
                }
                self.operate = key;
                self.operand = self.get_display() * if self.sign == '-' { -1.0 } else { 1.0 };
            }
            'C' => {
                self.clear();
            }
            _ => {}
        }
    }
}

impl View for CalcDisplay {
    fn state(&self) -> &ViewState {
        &self.st
    }
    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.st
    }

    // Lets `Calculator` reach the display via downcast to feed it button keys.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        let color = ctx.style(Role::ScrollerNormal);
        let ext = self.st.get_extent();
        ctx.fill(ext, ' ', color);
        let i = (ext.b.x - self.number.len() as i32 - 2).max(0);
        let sign_str = self.sign.to_string();
        ctx.put_str(i, 0, &sign_str, color);
        ctx.put_str(i + 1, 0, &self.number, color);
    }

    fn handle_event(&mut self, ev: &mut Event, _ctx: &mut tvision_rs::Context) {
        match ev {
            Event::KeyDown(ke) => match ke.key {
                Key::Char(c) => {
                    self.calc_key(c);
                    ev.clear();
                }
                Key::Backspace => {
                    self.calc_key('\x08');
                    ev.clear();
                }
                Key::Enter => {
                    self.calc_key('\r');
                    ev.clear();
                }
                _ => {}
            },
            Event::Broadcast { command, .. } => {
                // CALC_BUTTON is handled by Calculator; display handles keyboard.
                let _ = command;
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Calculator — TCalculator port (a Dialog)
// ---------------------------------------------------------------------------

/// `(label, key)` for each calculator button. The C++ `keyChar` table uses
/// CP437 glyphs (`\x1B` = ←, `\xF1` = ±) as both the face text and the key fed
/// to `calcKey`; under UTF-8 those bytes aren't printable, so the face and the
/// key char are split: `←` deletes the last digit (`\x1b`), `±` toggles the
/// sign (`_`, the keyboard equivalent C++ also accepts).
const CALC_KEYS: [(&str, char); 20] = [
    ("C", 'C'),
    ("←", '\x1b'),
    ("%", '%'),
    ("±", '_'),
    ("7", '7'),
    ("8", '8'),
    ("9", '9'),
    ("/", '/'),
    ("4", '4'),
    ("5", '5'),
    ("6", '6'),
    ("*", '*'),
    ("1", '1'),
    ("2", '2'),
    ("3", '3'),
    ("-", '-'),
    ("0", '0'),
    (".", '.'),
    ("=", '='),
    ("+", '+'),
];

struct Calculator {
    dialog: Dialog,
    display_id: ViewId,
    /// Button `ViewId` → the key character it represents, so a `CALC_BUTTON`
    /// broadcast (whose `source` is the pressed button) can be mapped back to a
    /// key. This replaces C++'s `((TButton*)infoPtr)->title[0]` (D4: the
    /// broadcast carries a resolvable id, not the `void*` button pointer).
    buttons: Vec<(ViewId, char)>,
}

impl Calculator {
    fn new() -> Self {
        let mut dialog = Dialog::new(Rect::new(5, 3, 29, 18), Some("Calculator".to_string()));
        dialog.state_mut().options.first_click = true;

        // Calculator buttons: 4 columns × 5 rows.
        let mut buttons = Vec::with_capacity(CALC_KEYS.len());
        for (i, &(label, key)) in CALC_KEYS.iter().enumerate() {
            let x = (i % 4) as i32 * 5 + 2;
            let y = (i / 4) as i32 * 2 + 4;
            let r = Rect::new(x, y, x + 5, y + 2);
            let mut btn = Button::new(
                r,
                label,
                CALC_BUTTON,
                ButtonFlags {
                    broadcast: true,
                    ..ButtonFlags::new()
                },
            );
            btn.state_mut().options.selectable = false;
            let id = dialog.insert_child(Box::new(btn));
            buttons.push((id, key));
        }

        let display_r = Rect::new(3, 2, 21, 3);
        let display = CalcDisplay::new(display_r);
        let display_id = dialog.insert_child(Box::new(display));

        Calculator {
            dialog,
            display_id,
            buttons,
        }
    }
}

#[delegate(to = dialog)]
impl View for Calculator {
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut tvision_rs::Context) {
        self.dialog.handle_event(ev, ctx);

        // A button broadcast names the pressed button in `source`; map it to its
        // key char and feed the display (C++ TCalcDisplay's cmCalcButton arm).
        if let Event::Broadcast {
            command,
            source: Some(bid),
        } = *ev
            && command == CALC_BUTTON
            && let Some(&(_, key)) = self.buttons.iter().find(|(id, _)| *id == bid)
        {
            if let Some(display) = self
                .dialog
                .child_mut(self.display_id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<CalcDisplay>())
            {
                display.calc_key(key);
            }
            ev.clear();
        }
    }
}

// ---------------------------------------------------------------------------
// EventViewer — TEventViewer port
// ---------------------------------------------------------------------------

/// One-line description of an event for the viewer's log. Returns `None` for
/// events we don't log: broadcasts (mostly internal — and logging the
/// `cmScrollBarChanged` our own `scroll_to` emits would feed back), mouse
/// moves, auto-repeat, timers, and consumed events.
fn describe_event(ev: &Event) -> Option<String> {
    match ev {
        Event::KeyDown(ke) => Some(format!("KeyDown   {:?}", ke.key)),
        Event::MouseDown(me) => Some(format!(
            "MouseDown ({:>2},{:>2}) {:?}",
            me.position.x, me.position.y, me.buttons
        )),
        Event::MouseUp(me) => Some(format!(
            "MouseUp   ({:>2},{:>2})",
            me.position.x, me.position.y
        )),
        Event::Command(c) => Some(format!("Command   {:?}", c)),
        Event::Paste(s) => Some(format!("Paste     {:?}", s)),
        _ => None,
    }
}

/// The scrollable log interior — a [`Scroller`] over a `Vec<String>`, like the
/// C++ `TTerminal` inside `TEventViewer` (tvision-rs's `Terminal::as_any_mut`
/// delegates to its inner `Scroller`, so an owner can't reach the terminal to
/// feed it; a purpose-built log view exposes `as_any_mut` → self instead).
struct EventLog {
    scroller: Scroller,
    lines: Vec<String>,
}

impl EventLog {
    fn new(bounds: Rect, h: Option<ViewId>, v: Option<ViewId>) -> Self {
        let mut scroller = Scroller::new(bounds, h, v);
        scroller.state_mut().grow_mode = GrowMode {
            hi_x: true,
            hi_y: true,
            ..Default::default()
        };
        EventLog {
            scroller,
            lines: Vec::new(),
        }
    }

    /// Append a line, cap the backlog, refresh the scroll limits, and follow
    /// the tail so the newest line stays visible.
    fn push(&mut self, line: String, ctx: &mut tvision_rs::Context) {
        self.lines.push(line);
        const MAX_LINES: usize = 1000;
        if self.lines.len() > MAX_LINES {
            self.lines.remove(0);
        }
        let max_w = self
            .lines
            .iter()
            .map(|l| l.chars().count() as i32)
            .max()
            .unwrap_or(1);
        let n = self.lines.len() as i32;
        self.scroller.set_limit(max_w.max(1), n.max(1), ctx);
        let vis_y = self.scroller.state().get_extent().b.y;
        self.scroller.scroll_to(0, (n - vis_y).max(0), ctx);
    }
}

#[delegate(to = scroller)]
impl View for EventLog {
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        let color = ctx.style(Role::ScrollerNormal);
        let ext = self.scroller.state().get_extent();
        let delta = self.scroller.delta;
        for row in 0..ext.b.y {
            let idx = (row + delta.y) as usize;
            let text = self.lines.get(idx).map(|s| s.as_str()).unwrap_or("");
            let visible: String = text
                .chars()
                .skip(delta.x as usize)
                .take(ext.b.x as usize)
                .collect();
            ctx.fill(Rect::new(0, row, ext.b.x, row + 1), ' ', color);
            ctx.put_str(0, row, &visible, color);
        }
    }
}

/// A window that logs the events it receives. Faithful to `TEventViewer` in
/// spirit; tvision-rs's `run_app` exposes no global event hook, so this logs the
/// events routed to the viewer itself (keystrokes while focused, clicks on it,
/// posted commands) rather than every event in the program.
struct EventViewer {
    window: Window,
    log_id: ViewId,
    count: u32,
}

impl EventViewer {
    fn new(bounds: Rect) -> Self {
        let mut window = Window::new(bounds, Some("Event Viewer".to_string()), 0);
        window.state_mut().options.tileable = true;

        let ext = window.state().get_extent();
        let vsb_id = window.standard_scroll_bar(ScrollBarOptions {
            vertical: true,
            handle_keyboard: true,
        });
        let log_r = Rect::new(ext.a.x + 1, ext.a.y + 1, ext.b.x - 1, ext.b.y - 1);
        let log = EventLog::new(log_r, None, Some(vsb_id));
        let log_id = window.insert_child(Box::new(log));

        EventViewer {
            window,
            log_id,
            count: 0,
        }
    }
}

#[delegate(to = window)]
impl View for EventViewer {
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut tvision_rs::Context) {
        // Describe before forwarding — handling may consume (clear) the event.
        if let Some(desc) = describe_event(ev) {
            self.count += 1;
            let line = format!("#{:<4} {}", self.count, desc);
            if let Some(log) = self
                .window
                .child_mut(self.log_id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<EventLog>())
            {
                log.push(line, ctx);
            }
        }

        self.window.handle_event(ev, ctx);

        // Respond to CMD_FND_EV_VIEW so a caller can detect this viewer exists.
        if let Event::Broadcast { command, .. } = ev
            && *command == CMD_FND_EV_VIEW
        {
            ev.clear();
        }
    }
}

// ---------------------------------------------------------------------------
// FileViewer — TFileViewer port (a Scroller-based text file viewer)
// ---------------------------------------------------------------------------

struct FileViewer {
    scroller: Scroller,
    lines: Vec<String>,
    limit_set: bool,
}

impl FileViewer {
    fn new(bounds: Rect, h: Option<ViewId>, v: Option<ViewId>, path: &std::path::Path) -> Self {
        let mut scroller = Scroller::new(bounds, h, v);
        scroller.state_mut().grow_mode = GrowMode {
            hi_x: true,
            hi_y: true,
            ..Default::default()
        };
        let mut fv = FileViewer {
            scroller,
            lines: vec![],
            limit_set: false,
        };
        fv.read_file(path);
        fv
    }

    fn read_file(&mut self, path: &std::path::Path) {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                self.lines = content.lines().map(|l| l.to_string()).collect();
            }
            Err(_) => {
                self.lines = vec!["(Could not read file)".to_string()];
            }
        }
    }
}

#[delegate(to = scroller)]
impl View for FileViewer {
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut tvision_rs::Context) {
        // Publish the content extent on the first event — the ctor has no Context.
        // Faithful to C++ TFileViewer::readFile calling setLimit(maxWidth, lineCount).
        if !self.limit_set {
            let max_w = self
                .lines
                .iter()
                .map(|l| l.chars().count() as i32)
                .max()
                .unwrap_or(1);
            let n = self.lines.len() as i32;
            self.scroller.set_limit(max_w.max(1), n.max(1), ctx);
            self.limit_set = true;
        }
        self.scroller.handle_event(ev, ctx);
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        let color = ctx.style(Role::ScrollerNormal);
        let ext = self.scroller.state().get_extent();
        let delta = self.scroller.delta;
        for row in 0..ext.b.y {
            let line_idx = (row + delta.y) as usize;
            let text = self.lines.get(line_idx).map(|s| s.as_str()).unwrap_or("");
            let col_start = delta.x as usize;
            let visible: String = text
                .chars()
                .skip(col_start)
                .take(ext.b.x as usize)
                .collect();
            ctx.fill(Rect::new(0, row, ext.b.x, row + 1), ' ', color);
            ctx.put_str(0, row, &visible, color);
        }
    }
}

// ---------------------------------------------------------------------------
// FileWindow — TFileWindow port
// ---------------------------------------------------------------------------

struct FileWindow {
    window: Window,
}

impl FileWindow {
    fn new(path: &std::path::Path, rect: Rect, number: i16) -> Self {
        let title = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let mut window = Window::new(rect, Some(title), number);
        window.state_mut().options.tileable = true;

        let ext = window.state().get_extent();
        // C++ TFileWindow: r = getExtent(); r.grow(-1, -1)
        let r = Rect::new(ext.a.x + 1, ext.a.y + 1, ext.b.x - 1, ext.b.y - 1);
        let hsb_id = window.standard_scroll_bar(ScrollBarOptions {
            vertical: false,
            handle_keyboard: true,
        });
        let vsb_id = window.standard_scroll_bar(ScrollBarOptions {
            vertical: true,
            handle_keyboard: true,
        });

        let fv = FileViewer::new(r, Some(hsb_id), Some(vsb_id), path);
        window.insert_child(Box::new(fv));

        FileWindow { window }
    }
}

#[delegate(to = window)]
impl View for FileWindow {}

// ---------------------------------------------------------------------------
// DemoBackground — background with a changeable pattern
// ---------------------------------------------------------------------------

/// The desktop background fill character, shared between [`DemoBackground`]
/// (which reads it every `draw`) and the `Background…` menu command (which
/// rewrites it). A process-global atomic sidesteps the desktop-factory
/// signature, which cannot capture state. The whole-tree redraw (D8) picks up
/// the change on the next pump — no explicit invalidate needed.
static BG_PATTERN: AtomicU32 = AtomicU32::new('▒' as u32);

struct DemoBackground {
    st: ViewState,
}

impl DemoBackground {
    fn new(bounds: Rect) -> Self {
        let mut st = ViewState::new(bounds);
        st.grow_mode.hi_x = true;
        st.grow_mode.hi_y = true;
        DemoBackground { st }
    }
}

impl View for DemoBackground {
    fn state(&self) -> &ViewState {
        &self.st
    }
    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.st
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        let ext = self.st.get_extent();
        let style = ctx.style(Role::Background);
        let pattern = char::from_u32(BG_PATTERN.load(Ordering::Relaxed)).unwrap_or('▒');
        ctx.fill(ext, pattern, style);
    }
}

// ---------------------------------------------------------------------------
// TVDemo — main application
// ---------------------------------------------------------------------------

struct TVDemo {
    program: Program,
}

impl TVDemo {
    fn new(backend: Box<dyn Backend>) -> Self {
        TVDemo {
            program: Program::new(
                backend,
                Box::new(SystemClock::new()),
                Theme::classic_blue(),
                Self::init_desktop,
                Self::init_status_line,
                Self::init_menu_bar,
            ),
        }
    }

    fn init_desktop(r: Rect) -> Option<Box<dyn View>> {
        let mut r = r;
        r.a.y += 1;
        r.b.y -= 1;
        Some(Box::new(Desktop::new(r, |br| {
            Some(Box::new(DemoBackground::new(br)))
        })))
    }

    fn init_status_line(r: Rect) -> Option<Box<dyn View>> {
        let mut r = r;
        r.a.y = r.b.y - 1;
        let defs = StatusDef::list()
            .def_all(|d| {
                d.key_item(alt('x'), Command::QUIT)
                    .item("~F10~ Menu", KeyEvent::from(Key::F(10)), Command::MENU)
                    .item("~F3~ Open", KeyEvent::from(Key::F(3)), CMD_OPEN)
                    .item("~F5~ Zoom", KeyEvent::from(Key::F(5)), Command::ZOOM)
                    .item("~F4~ Full", KeyEvent::from(Key::F(4)), Command::FULLSCREEN)
                    .item("~F6~ Next", KeyEvent::from(Key::F(6)), Command::NEXT)
                    .item("~Alt-F3~ Close", alt_f3(), Command::CLOSE)
                    .key_item(ctrl_f5(), Command::RESIZE)
            })
            .build();
        Some(Box::new(StatusLine::new(r, defs)))
    }

    fn init_menu_bar(r: Rect) -> Option<Box<dyn View>> {
        let mut r = r;
        r.b.y = r.a.y + 1;
        let menu = Menu::builder()
            .submenu("~≡~", alt(' '), |m| {
                m.command("~A~bout…", CMD_ABOUT)
                    .separator()
                    .command("~P~uzzle", CMD_PUZZLE)
                    .command("Ca~l~endar", CMD_CALENDAR)
                    .command("Ascii ~T~able", CMD_ASCII)
                    .command("~C~alculator", CMD_CALC)
                    .command("Color Pic~k~er", CMD_COLORS)
                    .command("~S~plitter", CMD_SPLIT)
                    .command_key("~E~vent Viewer", CMD_EVENT_VIEW, alt0(), "Alt-0")
            })
            .submenu("~F~ile", alt('f'), |m| {
                m.command_key("~O~pen…", CMD_OPEN, KeyEvent::from(Key::F(3)), "F3")
                    .separator()
                    .command_key("E~x~it", Command::QUIT, alt('x'), "Alt-X")
            })
            .submenu("~W~indows", alt('w'), |m| {
                m.command_key("~S~ize/move", Command::RESIZE, ctrl_f5(), "Ctrl-F5")
                    .command_key("~Z~oom", Command::ZOOM, KeyEvent::from(Key::F(5)), "F5")
                    .command_key(
                        "~F~ull screen",
                        Command::FULLSCREEN,
                        KeyEvent::from(Key::F(4)),
                        "F4",
                    )
                    .command("~T~ile", Command::TILE)
                    .command("C~a~scade", Command::CASCADE)
                    .command_key("~N~ext", Command::NEXT, KeyEvent::from(Key::F(6)), "F6")
                    .command_key("~P~revious", Command::PREV, shift_f6(), "Shift-F6")
                    .command_key("~C~lose", Command::CLOSE, alt_f3(), "Alt-F3")
            })
            .submenu("~O~ptions", alt('o'), |m| {
                m.command("~B~ackground…", CMD_CH_BG)
            })
            .build();
        Some(Box::new(MenuBar::new(r, menu)))
    }

    fn about_box(prog: &mut Program) {
        let mut dlg = Dialog::new(Rect::new(0, 0, 39, 13), Some("About".to_string()));
        dlg.state_mut().options.center_x = true;
        dlg.state_mut().options.center_y = true;
        dlg.insert_child(Box::new(StaticText::new(
            Rect::new(9, 2, 30, 9),
            "\x03Turbo Vision Demo\n\n\x03tvision-rs\n\n\x03Copyright (c) 2026\n\n\x03Faithfully Ported".to_string(),
        )));
        dlg.insert_child(Box::new(Button::new(
            Rect::new(14, 10, 26, 12),
            " OK",
            Command::OK,
            ButtonFlags {
                default: true,
                ..ButtonFlags::new()
            },
        )));
        prog.exec_view(Box::new(dlg));
    }

    fn run(&mut self) {
        let mut next_num: i16 = 1;
        self.program.run_app(move |prog, cmd| {
            if cmd == CMD_ABOUT {
                Self::about_box(prog);
            } else if cmd == CMD_PUZZLE {
                prog.desktop_insert(Box::new(PuzzleWindow::new()));
            } else if cmd == CMD_CALENDAR {
                prog.desktop_insert(Box::new(CalendarWindow::new()));
            } else if cmd == CMD_ASCII {
                prog.desktop_insert(Box::new(AsciiWindow::new()));
            } else if cmd == CMD_CALC {
                prog.desktop_insert(Box::new(Calculator::new()));
            } else if cmd == CMD_COLORS {
                prog.desktop_insert(color_window());
            } else if cmd == CMD_SPLIT {
                prog.desktop_insert(splitter_window());
            } else if cmd == CMD_EVENT_VIEW {
                let r = prog.desktop_rect();
                prog.desktop_insert(Box::new(EventViewer::new(r)));
            } else if cmd == CMD_CH_BG {
                let cur = char::from_u32(BG_PATTERN.load(Ordering::Relaxed)).unwrap_or('▒');
                let (answer, text) = prog.input_box(
                    "Background",
                    "Enter background character:",
                    &cur.to_string(),
                    1,
                );
                if answer != Command::CANCEL
                    && let Some(ch) = text.chars().next()
                {
                    BG_PATTERN.store(ch as u32, Ordering::Relaxed);
                }
            } else if cmd == CMD_OPEN
                && let Some(path) = prog.open_file_dialog("Open a File", "*.*")
            {
                let r = prog.desktop_rect();
                let win = FileWindow::new(&path, r, next_num);
                prog.desktop_insert(Box::new(win));
                next_num += 1;
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Color picker + Splitter (tvision-rs widgets, opened as desktop windows)
// ---------------------------------------------------------------------------

/// A truecolor [`ColorPicker`] on its hue/saturation plane, in a dialog.
///
/// Layout: dialog 62 × 23 cells. Picker occupies rows 2–18 (17 rows tall),
/// leaving row 19 blank and the button row at row 20 (`height − 3`).
fn color_window() -> Box<dyn View> {
    let mut dlg = Dialog::new(Rect::new(6, 1, 68, 24), Some("Select Color".to_string()));
    let mut picker = ColorPicker::new(Rect::new(2, 2, 60, 19), Color::Rgb(30, 144, 255));
    picker.select_tab(Tab::Plane);
    dlg.insert_child(Box::new(picker));
    dlg.button_row(
        &[
            (
                "~O~K",
                Command::OK,
                ButtonFlags {
                    default: true,
                    ..ButtonFlags::new()
                },
            ),
            ("~C~ancel", Command::CANCEL, ButtonFlags::new()),
        ],
        ButtonRowAlign::Right,
    );
    Box::new(dlg)
}

/// The Splitter "list pane": a self-filtering [`ListBox`], populated on the
/// first event tick because [`ListBox::new_list`] needs a `&mut Context`.
/// [`FindMode::Filter`] lets the user type to narrow the visible items.
struct FindListPane {
    list: ListBox,
    populated: bool,
}

impl FindListPane {
    fn new() -> Self {
        let list = ListBox::new(Rect::new(0, 0, 1, 1), 1, None, None).with_find(FindMode::Filter);
        FindListPane {
            list,
            populated: false,
        }
    }
}

#[delegate(to = list)]
impl View for FindListPane {
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut tvision_rs::Context) {
        if !self.populated {
            self.populated = true;
            self.list.new_list(
                vec![
                    "Apple".into(),
                    "Apricot".into(),
                    "Banana".into(),
                    "Blueberry".into(),
                    "Cherry".into(),
                    "Cranberry".into(),
                    "Grape".into(),
                    "Mango".into(),
                    "Orange".into(),
                    "Peach".into(),
                    "Pineapple".into(),
                    "Tangerine".into(),
                ],
                ctx,
            );
            tvision_rs::widgets::list_viewer::update_steps(&self.list, ctx);
        }
        self.list.handle_event(ev, ctx);
    }
}

/// A [`Splitter`] grid: a fixed sidebar beside a column split into two stacked
/// rows, `.joined()` so the seams connect to the window frame and each other.
fn splitter_window() -> Box<dyn View> {
    let mut win = Window::new(Rect::new(3, 1, 59, 18), Some("Splitter".to_string()), 1);
    let ext = win.state().get_extent();
    let interior = Rect::new(1, 1, ext.b.x - 1, ext.b.y - 1);
    let right = Splitter::rows()
        .pane(Box::new(FindListPane::new()), Constraints::flex())
        .pane(
            Box::new(StaticText::new(Rect::new(0, 0, 1, 1), "form pane")),
            Constraints::flex(),
        );
    let split = Splitter::cols()
        .pane(
            Box::new(StaticText::new(Rect::new(0, 0, 1, 1), "tree pane")),
            Constraints::weight(2).min(12),
        )
        .pane(Box::new(right), Constraints::weight(3))
        .joined();
    let split_id = win.insert_child(Box::new(split));
    if let Some(v) = win.child_mut(split_id) {
        v.change_bounds(interior);
    }
    Box::new(win)
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() -> io::Result<()> {
    let files: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let mut app = TVDemo::new(Box::new(CrosstermBackend::new()?));
    // Open files specified on command line.
    for (i, path) in files.iter().enumerate() {
        let r = app.program.desktop_rect();
        let win = FileWindow::new(path, r, i as i16 + 1);
        app.program.desktop_insert(Box::new(win));
    }
    app.run();
    Ok(())
}
