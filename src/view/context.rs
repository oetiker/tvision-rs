//! Downward draw and event/update contexts — deviations **D3** / **D4**.
//!
//! D3 forbids up-pointers: a parent passes a context *down* carrying everything
//! a child would otherwise reach upward for. There are two:
//!
//! * [`DrawCtx`] — the clipped, themed writer a view paints through during
//!   `draw()`. It works in *view-local* coordinates; the ctx translates them to
//!   absolute screen coordinates and clips. It re-expresses the `DrawBuffer`
//!   write ops (D8 clip-for-correctness) on top of the row-18 [`Buffer`] and the
//!   row-8 [`text`](crate::text) primitives — never re-deriving wide-char logic.
//! * [`Context`] — the event/update context handlers and `handle_event` reach
//!   for. It is anchored to the decided `ctx.*` call surface (post / broadcast /
//!   timer scheduling / deferred capture push). It is built over loop-owned
//!   state as **distinct `&mut` fields** so Phase 1 can take disjoint-field
//!   borrows; the fields are deliberately not hidden behind one getter.

use crate::capture::CaptureHandler;
use crate::color::Style;
use crate::command::Command;
use crate::event::Event;
use crate::screen::Buffer;
use crate::theme::{Glyphs, Role, Theme};
use crate::timer::{TimerId, TimerQueue};
use crate::view::geometry::{Point, Rect};
use crate::view::id::ViewId;
use std::collections::VecDeque;
use std::time::Duration;
use unicode_width::UnicodeWidthChar;

// ---------------------------------------------------------------------------
// DrawCtx — the downward draw context (D3 / D8)
// ---------------------------------------------------------------------------

/// The clipped, themed writer every view paints through (D3).
///
/// All public write methods take **view-local** coordinates: `(0, 0)` is the
/// view's own top-left. The ctx adds [`origin`](Self::origin) to translate into
/// absolute screen columns/rows, and clips every write to [`clip`](Self::clip).
/// The clip is stored as an **absolute** rect already intersected with the
/// buffer bounds at construction, so a write can never index the buffer out of
/// range.
pub struct DrawCtx<'a> {
    buffer: &'a mut Buffer,
    /// Absolute clip rect, already intersected with the buffer's `(0,0,w,h)`.
    clip: Rect,
    /// View-local `(0, 0)` maps to this absolute screen position.
    origin: Point,
    theme: &'a Theme,
}

impl<'a> DrawCtx<'a> {
    /// Build a draw context.
    ///
    /// `clip` is intersected with the buffer's bounds (`(0, 0, width, height)`)
    /// at construction and stored absolute, so the write methods can never index
    /// out of bounds.
    pub fn new(buffer: &'a mut Buffer, theme: &'a Theme, clip: Rect, origin: Point) -> Self {
        let bounds = Rect::new(0, 0, buffer.width() as i32, buffer.height() as i32);
        let mut clip = clip;
        clip.intersect(&bounds);
        DrawCtx {
            buffer,
            clip,
            origin,
            theme,
        }
    }

    /// The [`Style`] for `role` from the active theme.
    pub fn style(&self, role: Role) -> Style {
        self.theme.style(role)
    }

    /// The theme's glyph holder (D7 stub for now).
    pub fn glyphs(&self) -> &Glyphs {
        self.theme.glyphs()
    }

    /// The absolute clip rect (already intersected with the buffer bounds).
    pub fn clip(&self) -> Rect {
        self.clip
    }

    /// The absolute screen position that view-local `(0, 0)` maps to.
    pub fn origin(&self) -> Point {
        self.origin
    }

    /// Write one cell at view-local `(x, y)` with `style`.
    ///
    /// A double-width `ch` sets the lead `wide` and the next cell `wide_trail`,
    /// but only if both fall inside the clip; if the trail would fall outside,
    /// a space is written instead. Anything fully outside the clip is dropped
    /// (never panics).
    pub fn put_char(&mut self, x: i32, y: i32, ch: char, style: Style) {
        if self.clip.is_empty() {
            return;
        }
        let ax = x + self.origin.x;
        let ay = y + self.origin.y;
        if ay < self.clip.a.y || ay >= self.clip.b.y {
            return;
        }
        if ax < self.clip.a.x || ax >= self.clip.b.x {
            return;
        }
        let wide = UnicodeWidthChar::width(ch).unwrap_or(1) > 1;
        let row = self.buffer.row_mut(ay as u16);
        let i = ax as usize;
        if wide && ax + 1 < self.clip.b.x {
            // Room for both halves inside the clip.
            let mut buf = [0u8; 4];
            row[i].set_str(ch.encode_utf8(&mut buf), true);
            row[i].set_style(style);
            row[i + 1].set_wide_trail();
            row[i + 1].set_style(style);
        } else if wide {
            // Trail would fall outside the clip — degrade to a space.
            row[i].set_char(' ');
            row[i].set_style(style);
        } else {
            row[i].set_char(ch);
            row[i].set_style(style);
        }
    }

    /// Write `s` at view-local `(x, y)` with a fixed `style`, width-aware and
    /// clipped. Returns the number of columns actually written.
    ///
    /// Delegates the wide-char and edge-straddle logic to [`text::draw_str`],
    /// exactly as `DrawBuffer::move_str_part` does — the string is written into
    /// the clipped sub-slice of the target buffer row, with `indent` /
    /// `text_indent` chosen so a glyph straddling either clip edge degrades the
    /// same way `move_str_part` already handles it.
    pub fn put_str(&mut self, x: i32, y: i32, s: &str, style: Style) -> i32 {
        if self.clip.is_empty() {
            return 0;
        }
        let ay = y + self.origin.y;
        if ay < self.clip.a.y || ay >= self.clip.b.y {
            return 0;
        }
        let ax = x + self.origin.x;
        // The writable window for this row is the clip's column span.
        let lo = self.clip.a.x as usize;
        let hi = self.clip.b.x as usize; // > lo, since clip is non-empty
        let row = &mut self.buffer.row_mut(ay as u16)[lo..hi];

        let (indent, text_indent) = if ax >= self.clip.a.x {
            // String starts at or after the clip left edge: indent into the
            // sub-slice; right-edge truncation falls out of `draw_str` running
            // out of cells.
            ((ax - self.clip.a.x) as usize, 0)
        } else {
            // String starts left of the clip: skip the off-screen columns via
            // text_indent (this is move_str_part's left-edge straddle path).
            (0, self.clip.a.x - ax)
        };

        crate::text::draw_str(row, indent, s, text_indent, style) as i32
    }

    /// Write `s` at view-local `(x, y)`, toggling between `lo` and `hi` styles at
    /// each `~` (the `~` itself is not drawn) — ports `TDrawBuffer::moveCStr`'s
    /// attribute-pair toggle (used by frame icons; reused by buttons/labels/menus
    /// for hotkey highlighting). Starts in `lo`. Clipped exactly like
    /// [`put_char`](Self::put_char). Returns the number of columns advanced.
    ///
    /// Faithful to [`DrawBuffer::move_cstr_part`](crate::screen::DrawBuffer): the
    /// first `~` flips `lo` → `hi`, the next flips back, and so on; the `~`
    /// characters draw nothing and do not advance the column.
    pub fn put_cstr(&mut self, x: i32, y: i32, s: &str, lo: Style, hi: Style) -> i32 {
        let mut col = 0i32;
        let mut current = lo;
        let mut hi_active = false;
        for ch in s.chars() {
            if ch == '~' {
                hi_active = !hi_active;
                current = if hi_active { hi } else { lo };
                continue;
            }
            self.put_char(x + col, y, ch, current);
            col += UnicodeWidthChar::width(ch).unwrap_or(1) as i32;
        }
        col
    }

    /// Fill view-local rect `area_local` (clipped) with `ch` styled `style`.
    pub fn fill(&mut self, area_local: Rect, ch: char, style: Style) {
        if self.clip.is_empty() {
            return;
        }
        // Translate to absolute and clip.
        let mut abs = area_local;
        abs.r#move(self.origin.x, self.origin.y);
        abs.intersect(&self.clip);
        if abs.is_empty() {
            return;
        }
        for ay in abs.a.y..abs.b.y {
            let row = self.buffer.row_mut(ay as u16);
            for ax in abs.a.x..abs.b.x {
                let cell = &mut row[ax as usize];
                cell.set_char(ch);
                cell.set_style(style);
            }
        }
    }

    /// A child context for a sub-view at view-local rect `area_local`.
    ///
    /// The child's clip is `self.clip ∩ (area_local translated by origin)`, and
    /// its origin is `self.origin + area_local.a`. The buffer is reborrowed for
    /// the child's shorter lifetime. No re-intersection with the buffer bounds
    /// is needed — `self.clip` is already inside them.
    pub fn sub(&mut self, area_local: Rect) -> DrawCtx<'_> {
        let mut abs = area_local;
        abs.r#move(self.origin.x, self.origin.y);
        let mut clip = self.clip;
        clip.intersect(&abs);
        DrawCtx {
            buffer: &mut *self.buffer,
            clip,
            origin: self.origin + area_local.a,
            theme: self.theme,
        }
    }
}

// ---------------------------------------------------------------------------
// Context — the downward event/update context (D3 / D4)
// ---------------------------------------------------------------------------

/// The event/update context `handle_event` and capture handlers reach for (D3).
///
/// Built over loop-owned state as **distinct `&mut` fields** (not hidden behind
/// a single getter) so Phase 1 can borrow them disjointly. The live event loop
/// (row 31) owns the backing `VecDeque` / [`TimerQueue`] / pending-capture
/// `Vec` and constructs a fresh `Context` per dispatch.
///
/// `query(ViewId, …) -> Option<T>` / `message(ViewId, …)` are **tree-owner**
/// primitives (Group/Program over `find_mut`), *not* `Context` methods — a
/// `Context` deliberately holds no tree to route through. They are **deferred to
/// row 34** (their first return-consumer, a dialog `cmCanCloseForm` veto), so
/// they are intentionally not stubbed here.
pub struct Context<'a> {
    /// Posted commands / broadcasts, drained by the loop after dispatch.
    out_events: &'a mut VecDeque<Event>,
    /// The loop's timer queue.
    timers: &'a mut TimerQueue,
    /// The clock value sampled for this dispatch pass.
    now_ms: u64,
    /// Deferred capture pushes — applied by the loop *after* the current
    /// dispatch, so a pushed handler sees the next event, never the current one.
    pending_captures: &'a mut Vec<Box<dyn CaptureHandler>>,
    /// Deferred command-enable changes (`(cmd, enable)`; `true` = enable, `false`
    /// = disable) — applied by the loop *after* the current dispatch, exactly like
    /// [`pending_captures`](Self::pending_captures). A view (D3) has no handle to
    /// the program's command set, so `enableCommand`/`disableCommand` are realized
    /// as a downward request queue the loop drains into `curCommandSet`.
    command_changes: &'a mut Vec<(Command, bool)>,
    /// The size of the view's owner (the group currently routing to it), so a child
    /// can reach `owner->size` / `owner->getExtent()` without an up-pointer (D3).
    /// Used by `TWindow::zoom`/`sizeLimits` (33c) and the drag limits (33d).
    ///
    /// **Transient routing state**, NOT a loop-owned channel: each
    /// `Group::handle_event` sets it to its own size before delivering to children
    /// and restores it on exit (so nesting root→desktop→window works). It is valid
    /// **only during group-routed dispatch**; a capture handler runs *before* group
    /// routing and sees the default `(0,0)`. That is fine — 33d's drag handler must
    /// capture its limits at *push time* (inside the window's `handle_event`, where
    /// `owner_size` is correctly set), never read them at drag time.
    owner_size: Point,
}

impl<'a> Context<'a> {
    /// Build an event/update context over the loop-owned state.
    pub fn new(
        out_events: &'a mut VecDeque<Event>,
        timers: &'a mut TimerQueue,
        now_ms: u64,
        pending_captures: &'a mut Vec<Box<dyn CaptureHandler>>,
        command_changes: &'a mut Vec<(Command, bool)>,
    ) -> Self {
        Context {
            out_events,
            timers,
            now_ms,
            pending_captures,
            command_changes,
            owner_size: Point::default(),
        }
    }

    /// Post a targeted command (`Event::Command`) into the loop's queue.
    pub fn post(&mut self, cmd: Command) {
        self.out_events.push_back(Event::Command(cmd));
    }

    /// Broadcast a command (`Event::Broadcast`) into the loop's queue. `source`
    /// names the view the broadcast is about (the `infoPtr` successor; D4
    /// amendment), or `None` if it concerns no particular view.
    pub fn broadcast(&mut self, command: Command, source: Option<ViewId>) {
        self.out_events
            .push_back(Event::Broadcast { command, source });
    }

    /// Arm a timer, returning its handle. `now_ms` is supplied from this
    /// context's dispatch snapshot (D9: clock not stored in the queue).
    pub fn set_timer(&mut self, timeout: Duration, period: Option<Duration>) -> TimerId {
        self.timers.set_timer(self.now_ms, timeout, period)
    }

    /// Cancel a pending timer.
    pub fn kill_timer(&mut self, id: TimerId) {
        self.timers.kill_timer(id);
    }

    /// Push a capture handler — **deferred**. The loop applies pending pushes
    /// after the current dispatch, so the pushed handler sees the *next* event,
    /// never the current one.
    ///
    /// There is intentionally **no `pop_capture`**: a handler pops itself by
    /// returning [`CaptureFlow::ConsumedPop`](crate::capture::CaptureFlow::ConsumedPop).
    pub fn push_capture(&mut self, handler: Box<dyn CaptureHandler>) {
        self.pending_captures.push(handler);
    }

    /// Request `cmd` be enabled in the program's command set — **deferred**. The
    /// loop applies queued changes after the current dispatch (mirrors
    /// [`push_capture`](Self::push_capture)). Realizes `TView::enableCommand` from
    /// a view that has no up-pointer to the program (D3).
    pub fn enable_command(&mut self, cmd: Command) {
        self.command_changes.push((cmd, true));
    }

    /// Request `cmd` be disabled — **deferred** (see
    /// [`enable_command`](Self::enable_command)).
    pub fn disable_command(&mut self, cmd: Command) {
        self.command_changes.push((cmd, false));
    }

    /// The clock value sampled for this dispatch pass.
    pub fn now_ms(&self) -> u64 {
        self.now_ms
    }

    /// The owner's size for the view currently being routed to — the downward
    /// realization of `owner->size` / `owner->getExtent()` (D3). See the
    /// [`owner_size`](Self::owner_size) field docs: it is **transient routing
    /// state** set/restored by each [`Group::handle_event`](crate::view::Group)
    /// around delivery, valid only during group-routed dispatch. Defaults to
    /// `(0, 0)`.
    pub fn owner_size(&self) -> Point {
        self.owner_size
    }

    /// Set the owner size for the routed view — called by
    /// [`Group::handle_event`](crate::view::Group) before delivering to children
    /// (set to the group's own size) and to restore it on exit.
    pub fn set_owner_size(&mut self, size: Point) {
        self.owner_size = size;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    fn style(fg: u8, bg: u8) -> Style {
        Style::new(Color::Bios(fg), Color::Bios(bg))
    }

    // -- DrawCtx ------------------------------------------------------------

    #[test]
    fn put_char_writes_at_origin_offset() {
        let mut buf = Buffer::new(10, 5);
        let theme = Theme::classic_blue();
        let s = style(0xF, 0x1);
        {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 5), Point::new(2, 1));
            // local (0,0) -> absolute (2,1)
            ctx.put_char(0, 0, 'X', s);
        }
        assert_eq!(buf.get(2, 1).symbol(), "X");
        assert_eq!(buf.get(2, 1).style(), s);
        // origin cell (0,0) untouched
        assert_eq!(buf.get(0, 0).symbol(), " ");
    }

    #[test]
    fn put_char_outside_clip_is_dropped() {
        let mut buf = Buffer::new(10, 5);
        let theme = Theme::classic_blue();
        {
            // clip only covers columns 2..5, rows 1..3
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(2, 1, 5, 3), Point::new(0, 0));
            ctx.put_char(0, 0, 'A', style(0xF, 0x1)); // outside clip
            ctx.put_char(3, 2, 'B', style(0xF, 0x1)); // inside clip
        }
        assert_eq!(
            buf.get(0, 0).symbol(),
            " ",
            "outside clip must not be written"
        );
        assert_eq!(buf.get(3, 2).symbol(), "B");
    }

    #[test]
    fn put_char_never_writes_out_of_buffer_with_huge_clip() {
        let mut buf = Buffer::new(4, 2);
        let theme = Theme::classic_blue();
        {
            // clip far larger than the buffer; construction intersects it down.
            let mut ctx = DrawCtx::new(
                &mut buf,
                &theme,
                Rect::new(0, 0, 1000, 1000),
                Point::new(0, 0),
            );
            // off the buffer edge -> dropped, no panic
            ctx.put_char(100, 100, 'Z', style(0xF, 0x1));
            ctx.put_char(3, 1, 'Q', style(0xF, 0x1));
        }
        assert_eq!(buf.get(3, 1).symbol(), "Q");
    }

    #[test]
    fn put_char_wide_at_clip_right_edge_degrades_to_space() {
        let mut buf = Buffer::new(10, 1);
        let theme = Theme::classic_blue();
        {
            // clip columns 0..3; place a wide glyph whose lead is at col 2,
            // so its trail (col 3) is outside the clip.
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 3, 1), Point::new(0, 0));
            ctx.put_char(2, 0, '中', style(0xF, 0x1));
        }
        assert_eq!(
            buf.get(2, 0).symbol(),
            " ",
            "wide lead with no room degrades to space"
        );
        assert!(!buf.get(2, 0).is_wide());
        assert_eq!(buf.get(3, 0).symbol(), " ", "outside clip untouched");
    }

    #[test]
    fn put_char_wide_with_room_sets_lead_and_trail() {
        let mut buf = Buffer::new(10, 1);
        let theme = Theme::classic_blue();
        {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 1), Point::new(0, 0));
            ctx.put_char(1, 0, '中', style(0xF, 0x1));
        }
        assert!(buf.get(1, 0).is_wide());
        assert_eq!(buf.get(1, 0).symbol(), "中");
        assert!(buf.get(2, 0).is_wide_trail());
    }

    #[test]
    fn put_str_writes_and_returns_columns() {
        let mut buf = Buffer::new(10, 2);
        let theme = Theme::classic_blue();
        let s = style(0xF, 0x1);
        let n = {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 2), Point::new(1, 0));
            ctx.put_str(0, 0, "hi", s)
        };
        assert_eq!(n, 2);
        assert_eq!(buf.get(1, 0).symbol(), "h");
        assert_eq!(buf.get(2, 0).symbol(), "i");
        assert_eq!(buf.get(1, 0).style(), s);
    }

    #[test]
    fn put_str_truncates_at_clip_right_edge() {
        let mut buf = Buffer::new(10, 1);
        let theme = Theme::classic_blue();
        let n = {
            // clip columns 0..4
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 4, 1), Point::new(0, 0));
            ctx.put_str(0, 0, "abcdefgh", style(0xF, 0x1))
        };
        assert_eq!(n, 4, "only the clip width is written");
        assert_eq!(buf.get(3, 0).symbol(), "d");
        // beyond the clip stays blank
        assert_eq!(buf.get(4, 0).symbol(), " ");
    }

    #[test]
    fn put_str_starting_left_of_clip_skips_offscreen_columns() {
        let mut buf = Buffer::new(10, 1);
        let theme = Theme::classic_blue();
        {
            // clip columns 2..10. Draw "abcdef" starting at absolute col 0:
            // columns 0,1 ('a','b') are off the clip left edge and skipped.
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(2, 0, 10, 1), Point::new(0, 0));
            ctx.put_str(0, 0, "abcdef", style(0xF, 0x1));
        }
        assert_eq!(buf.get(0, 0).symbol(), " ");
        assert_eq!(buf.get(1, 0).symbol(), " ");
        assert_eq!(buf.get(2, 0).symbol(), "c");
        assert_eq!(buf.get(3, 0).symbol(), "d");
    }

    #[test]
    fn put_cstr_toggles_style_on_tilde() {
        let mut buf = Buffer::new(10, 1);
        let theme = Theme::classic_blue();
        let lo = style(0xF, 0x1);
        let hi = style(0xA, 0x1);
        let n = {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 1), Point::new(0, 0));
            // "[~X~]" -> '[' and ']' in lo, 'X' in hi; tildes draw nothing.
            ctx.put_cstr(0, 0, "[~X~]", lo, hi)
        };
        assert_eq!(n, 3, "three visible columns advanced (the ~ draw nothing)");
        assert_eq!(buf.get(0, 0).symbol(), "[");
        assert_eq!(buf.get(0, 0).style(), lo);
        assert_eq!(buf.get(1, 0).symbol(), "X");
        assert_eq!(buf.get(1, 0).style(), hi, "between the ~ the style is hi");
        assert_eq!(buf.get(2, 0).symbol(), "]");
        assert_eq!(
            buf.get(2, 0).style(),
            lo,
            "after the closing ~ the style is lo"
        );
    }

    #[test]
    fn put_cstr_clips_like_put_char() {
        let mut buf = Buffer::new(10, 1);
        let theme = Theme::classic_blue();
        let lo = style(0xF, 0x1);
        let hi = style(0xA, 0x1);
        {
            // clip columns 0..2; "[~X~]" draws '[' at 0, 'X' at 1, ']' at 2 (clipped).
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 2, 1), Point::new(0, 0));
            ctx.put_cstr(0, 0, "[~X~]", lo, hi);
        }
        assert_eq!(buf.get(0, 0).symbol(), "[");
        assert_eq!(buf.get(1, 0).symbol(), "X");
        assert_eq!(buf.get(2, 0).symbol(), " ", "beyond the clip stays blank");
    }

    #[test]
    fn fill_clips_to_clip_rect() {
        let mut buf = Buffer::new(6, 4);
        let theme = Theme::classic_blue();
        let s = style(0x0, 0x3);
        {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(1, 1, 4, 3), Point::new(0, 0));
            // fill a local rect bigger than the clip
            ctx.fill(Rect::new(0, 0, 6, 4), '.', s);
        }
        // inside the clip
        assert_eq!(buf.get(1, 1).symbol(), ".");
        assert_eq!(buf.get(3, 2).symbol(), ".");
        // outside the clip untouched
        assert_eq!(buf.get(0, 0).symbol(), " ");
        assert_eq!(buf.get(4, 2).symbol(), " ");
        assert_eq!(buf.get(1, 1).style(), s);
    }

    #[test]
    fn sub_narrows_clip_and_shifts_origin() {
        let mut buf = Buffer::new(10, 10);
        let theme = Theme::classic_blue();
        {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 10), Point::new(0, 0));
            let mut child = ctx.sub(Rect::new(3, 2, 6, 5));
            assert_eq!(child.origin(), Point::new(3, 2));
            assert_eq!(child.clip(), Rect::new(3, 2, 6, 5));
            // child-local (0,0) -> absolute (3,2)
            child.put_char(0, 0, 'C', style(0xF, 0x1));
            // child-local write outside the child clip is dropped
            child.put_char(100, 100, 'X', style(0xF, 0x1));
        }
        assert_eq!(buf.get(3, 2).symbol(), "C");
    }

    #[test]
    fn sub_clip_intersects_parent() {
        let mut buf = Buffer::new(10, 10);
        let theme = Theme::classic_blue();
        {
            // parent clip 2..6 x 2..6
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(2, 2, 6, 6), Point::new(0, 0));
            // child local rect spans 0..10 -> intersect with parent clip
            let child = ctx.sub(Rect::new(0, 0, 10, 10));
            assert_eq!(child.clip(), Rect::new(2, 2, 6, 6));
        }
    }

    #[test]
    fn empty_clip_writes_nothing() {
        let mut buf = Buffer::new(5, 5);
        let theme = Theme::classic_blue();
        {
            // a clip that does not overlap the buffer at all
            let mut ctx = DrawCtx::new(
                &mut buf,
                &theme,
                Rect::new(100, 100, 200, 200),
                Point::new(0, 0),
            );
            assert!(ctx.clip().is_empty());
            ctx.put_char(0, 0, 'X', style(0xF, 0x1));
            ctx.put_str(0, 0, "hello", style(0xF, 0x1));
            ctx.fill(Rect::new(0, 0, 5, 5), '#', style(0xF, 0x1));
        }
        for cell in buf.cells() {
            assert_eq!(cell.symbol(), " ");
        }
    }

    // -- Context ------------------------------------------------------------

    #[test]
    fn context_post_and_broadcast_land_in_out_events() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut pending: Vec<Box<dyn CaptureHandler>> = Vec::new();
        let mut cmd_changes: Vec<(Command, bool)> = Vec::new();
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut pending, &mut cmd_changes);
            ctx.post(Command::OK);
            ctx.broadcast(Command::QUIT, None);
        }
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], Event::Command(Command::OK));
        assert_eq!(
            out[1],
            Event::Broadcast {
                command: Command::QUIT,
                source: None
            }
        );
    }

    #[test]
    fn context_set_and_kill_timer() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut pending: Vec<Box<dyn CaptureHandler>> = Vec::new();
        let mut cmd_changes: Vec<(Command, bool)> = Vec::new();
        let id = {
            let mut ctx = Context::new(&mut out, &mut timers, 100, &mut pending, &mut cmd_changes);
            assert_eq!(ctx.now_ms(), 100);
            ctx.set_timer(Duration::from_millis(50), None)
        };
        assert_eq!(timers.len(), 1);
        {
            let mut ctx = Context::new(&mut out, &mut timers, 100, &mut pending, &mut cmd_changes);
            ctx.kill_timer(id);
        }
        assert_eq!(timers.len(), 0);
    }

    #[test]
    fn context_command_changes_queue_enable_and_disable() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut pending: Vec<Box<dyn CaptureHandler>> = Vec::new();
        let mut cmd_changes: Vec<(Command, bool)> = Vec::new();
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut pending, &mut cmd_changes);
            ctx.enable_command(Command::OK);
            ctx.disable_command(Command::CANCEL);
        }
        assert_eq!(cmd_changes.len(), 2);
        assert_eq!(cmd_changes[0], (Command::OK, true));
        assert_eq!(cmd_changes[1], (Command::CANCEL, false));
    }

    #[test]
    fn context_owner_size_defaults_zero_and_round_trips() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut pending: Vec<Box<dyn CaptureHandler>> = Vec::new();
        let mut cmd_changes: Vec<(Command, bool)> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut pending, &mut cmd_changes);
        // Context::new defaults owner_size to (0, 0).
        assert_eq!(ctx.owner_size(), Point::new(0, 0));
        // The setter round-trips.
        ctx.set_owner_size(Point::new(80, 25));
        assert_eq!(ctx.owner_size(), Point::new(80, 25));
    }
}
