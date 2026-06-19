//! A scrolling text terminal: [`TextDevice`] (the output trait) and [`Terminal`]
//! (a ring-buffer view that shows the most recent lines).
//!
//! `Terminal` embeds a [`Scroller`] and delegates the un-overridden `View`
//! methods to it; the ring buffer stores raw bytes and the draw decodes them
//! UTF-8-width-aware. Users write to it through
//! [`write_bytes`](TextDevice::write_bytes); there is no stream wrapper.
//!
//! # Construction
//!
//! Building a terminal does not set its scroll limit, cursor, or visibility,
//! because those need a [`Context`] not available at construction. The consumer
//! calls [`Terminal::init`] once after inserting the terminal into a group (the
//! same pattern as the outline viewer).
//!
//! # Turbo Vision heritage
//!
//! Ports `TTextDevice` (`textview.cpp`) and `TTerminal` (`ttprvlns.cpp`).
//! Inheritance becomes a trait plus an embed-and-delegate wrapper over `Scroller`
//! (deviation D2); the C++ stream plumbing is replaced by the direct
//! [`write_bytes`](TextDevice::write_bytes) call (deviations D11, D12); the color
//! map becomes a [`Role`]; and the byte buffer is decoded with UTF-8-aware width
//! (deviation D13).

use crate::theme::Role;
use crate::view::{Context, DrawCtx, GrowMode, Point, Rect, View, ViewId};
use crate::widgets::Scroller;
use tvision_rs_macros::delegate;

// ---------------------------------------------------------------------------
// TextDevice — the abstract output trait
// ---------------------------------------------------------------------------

/// The abstract text-output device. Users of the terminal call
/// [`write_bytes`](Self::write_bytes) directly.
///
/// # Turbo Vision heritage
///
/// Ports `TTextDevice` (`textview.cpp`); the C++ stream layer is replaced by
/// this single write method.
pub trait TextDevice {
    /// Write `data` bytes into the device and return the number of bytes accepted
    /// (always `data.len()` for `Terminal`).
    fn write_bytes(&mut self, data: &[u8], ctx: &mut Context) -> usize;
}

// ---------------------------------------------------------------------------
// Terminal — the ring-buffer terminal view
// ---------------------------------------------------------------------------

/// A ring-buffer terminal view. Stores incoming text in a fixed-size ring buffer
/// and draws the most-recent `size.y` lines.
///
/// # Turbo Vision heritage
///
/// Ports `TTerminal` (`ttprvlns.cpp`).
pub struct Terminal {
    /// The embedded scroller — handles scrollbar sync, geometry, and all the
    /// `View` methods we do not override.
    scroller: Scroller,
    /// The ring buffer storage.
    buffer: Vec<u8>,
    /// Ring buffer capacity. Always `>= 1`; `buf_size - 1` is the max usable bytes
    /// (one slot is the "empty sentinel" that distinguishes full from empty).
    buf_size: usize,
    /// Write head — next byte goes here.
    que_front: usize,
    /// Read tail — oldest data starts here.
    que_back: usize,
}

impl Terminal {
    /// Create a terminal view backed by a ring buffer of `a_buf_size` bytes
    /// (clamped to `1..=32000`). The view grows with the lower-right corner of
    /// its owner group and the ring buffer starts empty.
    ///
    /// Call this once to allocate storage and geometry; then insert the returned
    /// value into a group and call [`Terminal::init`] to complete setup — `new`
    /// intentionally does **not** touch the scroll limit, cursor position, or
    /// cursor visibility because those require a [`Context`] that is not
    /// available until the view belongs to a group.
    ///
    /// # Example
    ///
    /// ```rust
    /// use tvision_rs::{Terminal, Rect};
    /// let term = Terminal::new(
    ///     Rect::new(0, 0, 80, 24),
    ///     None, // no horizontal scrollbar
    ///     None, // no vertical scrollbar
    ///     4096, // ring-buffer capacity in bytes
    /// );
    /// ```
    pub fn new(
        bounds: Rect,
        h_scroll_bar: Option<ViewId>,
        v_scroll_bar: Option<ViewId>,
        a_buf_size: usize,
    ) -> Self {
        let mut scroller = Scroller::new(bounds, h_scroll_bar, v_scroll_bar);
        scroller.state_mut().grow_mode = GrowMode {
            hi_x: true,
            hi_y: true,
            ..Default::default()
        };
        let buf_size = a_buf_size.clamp(1, 32000);
        let buffer = vec![0u8; buf_size];
        Terminal {
            scroller,
            buffer,
            buf_size,
            que_front: 0,
            que_back: 0,
        }
    }

    /// Complete terminal setup after the view has been inserted into a group.
    ///
    /// Call this exactly once, immediately after [`Group::insert`](crate::view::Group::insert)
    /// returns the terminal's [`ViewId`]. It performs the context-requiring steps
    /// that [`Terminal::new`] cannot: sets the scroll limit to one line, parks
    /// the cursor at `(0, 0)`, and makes the cursor visible.
    ///
    /// Forgetting this call leaves the terminal with an uninitialised scroll
    /// limit (zero lines) so no content will ever be scrolled or displayed
    /// correctly.
    pub fn init(&mut self, ctx: &mut Context) {
        self.scroller.set_limit(0, 1, ctx);
        self.scroller.state_mut().cursor = Point::new(0, 0);
        self.scroller.state_mut().show_cursor();
    }

    // -----------------------------------------------------------------------
    // Ring-buffer helpers (private)
    // -----------------------------------------------------------------------

    /// Decrement a ring-buffer index with wrap-around.
    fn buf_dec(&self, val: usize) -> usize {
        if val == 0 { self.buf_size - 1 } else { val - 1 }
    }

    /// Increment a ring-buffer index with wrap-around.
    fn buf_inc(&self, val: usize) -> usize {
        let next = val + 1;
        if next >= self.buf_size { 0 } else { next }
    }

    /// `true` if there is room for `amount` more bytes.
    ///
    /// Keeps one slot empty as the full/empty sentinel (so `buf_size - 1` is the
    /// max usable byte count).
    fn can_insert(&self, amount: usize) -> bool {
        if self.que_front < self.que_back {
            // Normal (no-wrap) case: free space = queBack - queFront - 1
            // (the -1 is the sentinel slot).
            // can_insert iff queBack > queFront + amount
            // i.e.  queBack - queFront - 1 >= amount
            self.que_back > self.que_front + amount
        } else {
            // Wrapped (or empty) case: free = buf_size - queFront + queBack - 1.
            // C++: (long(queFront) - bufSize + amount) < queBack
            // => queFront - buf_size + amount < queBack (signed comparison)
            // => amount < queBack + buf_size - queFront
            // => amount + queFront < queBack + buf_size
            // Since queFront >= queBack in this branch, queBack + buf_size > queFront,
            // so we use saturating arithmetic; both sides are usize.
            let t = self.que_front + amount; // may not overflow if sizes are bounded
            (self.que_back + self.buf_size) > t
        }
    }

    /// Returns `true` when the ring buffer contains no data.
    ///
    /// Use this to check whether any text has been written to the terminal
    /// since it was created or last drained. An empty terminal draws only blank
    /// rows. The test is an equality check on the two ring-buffer pointers
    /// (`que_back == que_front`), so it is `O(1)`.
    pub fn que_empty(&self) -> bool {
        self.que_back == self.que_front
    }

    /// Scan backward from `pos`, counting `lines` newline characters, and return
    /// the position of the first character on the `lines`-th-previous logical
    /// line. Handles ring-buffer wrap.
    ///
    /// Key subtlety: [`find_lf_backwards`](Self::find_lf_backwards) returns
    /// `(found, last_pos)`. On `!found` the loop continues — there is **no early
    /// return**. This correctly handles the wrap case where a newline lies in the
    /// "other half" of the buffer not covered by the current `count` window.
    fn prev_lines(&self, mut pos: usize, mut lines: usize) -> usize {
        if lines > 0 && pos != self.que_back {
            loop {
                if pos == self.que_back {
                    return self.que_back;
                }
                pos = self.buf_dec(pos);
                // count = number of bytes to scan backward from `pos` (inclusive)
                // C++: `pos >= queBack ? pos - queBack : pos` + 1
                let count = if pos >= self.que_back {
                    pos - self.que_back + 1
                } else {
                    pos + 1
                };
                // find_lf_backwards: scan backward, mutates pos to last-checked byte.
                // Returns (true, lf_pos) on found, (false, last_byte) on not-found.
                // The C++ do-while continues regardless; only --lines is conditional.
                let (found, last_pos) = self.find_lf_backwards(pos, count);
                pos = last_pos;
                if found {
                    lines -= 1;
                    if lines == 0 {
                        break;
                    }
                }
                // If !found, loop continues naturally (faithful to C++ do-while).
            }
            pos = self.buf_inc(pos);
        }
        pos
    }

    /// Helper for [`prev_lines`](Self::prev_lines).
    ///
    /// Scans backward from `pos` for up to `count` bytes.
    /// Returns `(true, pos_of_newline)` when found, or `(false, last_checked_pos)`
    /// when not found. Returning the last-checked position lets the caller
    /// continue iterating from where this scan stopped.
    fn find_lf_backwards(&self, mut pos: usize, count: usize) -> (bool, usize) {
        // C++: ++pos; do { if (buffer[--pos] == '\n') return True; } while (--count > 0);
        // We start from `pos` (already at the character) and scan backward.
        let mut remaining = count;
        loop {
            if self.buffer[pos] == b'\n' {
                return (true, pos);
            }
            if remaining <= 1 {
                return (false, pos); // last byte checked
            }
            remaining -= 1;
            pos = self.buf_dec(pos);
        }
    }

    /// Advance `pos` past the next `\n` in the ring buffer.
    fn next_line(&self, mut pos: usize) -> usize {
        while pos != self.que_front && self.buffer[pos] != b'\n' {
            pos = self.buf_inc(pos);
        }
        if pos != self.que_front {
            pos = self.buf_inc(pos);
        }
        pos
    }

    /// If the scratch buffer is full (256 bytes), trim any trailing incomplete
    /// UTF-8 sequence.
    ///
    /// Returns a `&str` slice of valid UTF-8 from the scratch buffer.
    fn valid_utf8(scratch: &[u8]) -> &str {
        match std::str::from_utf8(scratch) {
            Ok(s) => s,
            Err(e) => {
                // Trim to the last valid UTF-8 boundary.
                std::str::from_utf8(&scratch[..e.valid_up_to()]).unwrap_or("")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TextDevice impl
// ---------------------------------------------------------------------------

impl TextDevice for Terminal {
    /// Write `data` into the ring buffer, evicting old lines as needed, then
    /// update the scrollbar limits. Uses `usize` arithmetic throughout.
    fn write_bytes(&mut self, mut data: &[u8], ctx: &mut Context) -> usize {
        let original_len = data.len();

        // Trim to the max insertable size (buf_size - 1 usable bytes).
        let max_bytes = self.buf_size - 1;
        if data.len() > max_bytes {
            data = &data[data.len() - max_bytes..];
        }

        let count = data.len();
        if count == 0 {
            return original_len;
        }

        // Read limit.y BEFORE any set_limit call mutates it.
        let mut screen_lines = self.scroller.limit().y;

        // Count newlines in the new data.
        for &b in data {
            if b == b'\n' {
                screen_lines += 1;
            }
        }

        // Evict old lines from the tail until there is room.
        while !self.can_insert(count) {
            self.que_back = self.next_line(self.que_back);
            if screen_lines > 1 {
                screen_lines -= 1;
            }
        }

        // Write into the ring buffer (handle wrap-around).
        if self.que_front + count >= self.buf_size {
            let first = self.buf_size - self.que_front;
            self.buffer[self.que_front..self.buf_size].copy_from_slice(&data[..first]);
            self.buffer[..count - first].copy_from_slice(&data[first..]);
            self.que_front = count - first;
        } else {
            self.buffer[self.que_front..self.que_front + count].copy_from_slice(data);
            self.que_front += count;
        }

        // Publish new scrollbar limits and scroll position.
        self.scroller
            .set_limit(self.scroller.limit().x, screen_lines, ctx);
        self.scroller.scroll_to(0, screen_lines + 1, ctx);

        original_len
    }
}

// ---------------------------------------------------------------------------
// View impl — draw and as_any_mut are overridden; everything else delegates.
// ---------------------------------------------------------------------------

#[delegate(to = scroller)]
impl View for Terminal {
    /// Render the ring-buffer contents from newest to oldest. Colors come from
    /// [`Role::ScrollerNormal`] and text is rendered UTF-8-width-aware.
    fn draw(&mut self, ctx: &mut DrawCtx) {
        let color = ctx.style(Role::ScrollerNormal);
        let size = self.scroller.state().size;
        let limit_y = self.scroller.limit().y;
        let delta_y = self.scroller.delta.y;

        // bottomLine = size.y + delta.y
        let bottom_line = size.y + delta_y;

        // Find end_line: the ring position past the last byte of the last visible line.
        let end_line = if limit_y > bottom_line {
            let mut el = self.prev_lines(self.que_front, (limit_y - bottom_line) as usize);
            el = self.buf_dec(el);
            el
        } else {
            self.que_front
        };

        // Determine how many rows we actually draw (y starts from the bottom).
        let y_start = if limit_y > size.y {
            size.y - 1
        } else {
            // Fill empty rows below the content.
            let blank_rect = Rect::new(0, limit_y, size.x, size.y);
            if limit_y < size.y {
                ctx.fill(blank_rect, ' ', color);
            }
            limit_y - 1
        };

        // Draw rows from y_start down to 0, newest to oldest.
        // Use `while y >= 0` (mirrors C++ `for (; y >= 0; y--)`) so that when
        // size.y == 0 and y_start == -1, the body is skipped entirely instead
        // of looping forever on a y that never reaches 0.
        let mut cur_end = end_line;
        let mut y = y_start;
        while y >= 0 {
            let beg_line = self.prev_lines(cur_end, 1);

            // Inner loop: process the line in 256-byte chunks, faithful to the C++
            // inner while-loop which declares `char s[256]` as a stack-local each
            // iteration. `x` accumulates across chunks (C++: `x += b.moveStr(x,...)`).
            const MAX_SCRATCH: usize = 256;
            let mut x = 0i32;
            let mut line_pos = beg_line;
            while line_pos != cur_end {
                // Fresh scratch buffer each chunk iteration (faithful to C++ `char s[256]`).
                let mut scratch: Vec<u8> = Vec::with_capacity(MAX_SCRATCH);

                if cur_end >= line_pos {
                    // No wrap: copy min(cur_end - line_pos, MAX_SCRATCH) bytes.
                    let copy_len = (cur_end - line_pos).min(MAX_SCRATCH);
                    scratch.extend_from_slice(&self.buffer[line_pos..line_pos + copy_len]);

                    // Compute raw byte count to advance line_pos (always positive,
                    // preventing hang when a chunk starts with an invalid/continuation
                    // UTF-8 byte that makes valid_utf8 return "" i.e. slen == 0).
                    let raw_advance = copy_len;

                    // Trim possibly-truncated UTF-8 at end only when at the 256-byte cap
                    // (faithful to C++ `discardPossiblyTruncatedCharsAtEnd`). slen is
                    // used for rendering only — NOT for advancing line_pos.
                    let slen = if scratch.len() == MAX_SCRATCH {
                        Self::valid_utf8(&scratch).len()
                    } else {
                        scratch.len()
                    };
                    scratch.truncate(slen);

                    // Advance line_pos by raw count, not slen.
                    if line_pos + raw_advance >= self.buf_size {
                        line_pos = raw_advance - (self.buf_size - line_pos);
                    } else {
                        line_pos += raw_advance;
                    }
                } else {
                    // Wrap: copy buf[line_pos..buf_size] then buf[0..cur_end].
                    let fst_len = (self.buf_size - line_pos).min(MAX_SCRATCH);
                    scratch.extend_from_slice(&self.buffer[line_pos..line_pos + fst_len]);
                    let snd_len = cur_end.min(MAX_SCRATCH - fst_len);
                    scratch.extend_from_slice(&self.buffer[..snd_len]);

                    // Raw byte count for advancing line_pos (always >= 1 since fst_len >= 1).
                    let raw_advance = fst_len + snd_len;

                    // Trim at cap only; raw_advance is used for line_pos, not slen.
                    let slen = if scratch.len() == MAX_SCRATCH {
                        Self::valid_utf8(&scratch).len()
                    } else {
                        scratch.len()
                    };
                    scratch.truncate(slen);

                    // Advance line_pos by raw count, not slen.
                    if line_pos + raw_advance >= self.buf_size {
                        line_pos = raw_advance - (self.buf_size - line_pos);
                    } else {
                        line_pos += raw_advance;
                    }
                }

                // Render this chunk and advance x (C++: `x += b.moveStr(x, y, s, slen, ...)`).
                let text = Self::valid_utf8(&scratch);
                x += ctx.put_str(x, y, text, color);
            }

            // Pad the rest of the row with spaces.
            if x < size.x {
                ctx.fill(Rect::new(x, y, size.x, y + 1), ' ', color);
            }

            // Position cursor at end of newest line (faithful to C++ setCursor(x,y)).
            if cur_end == self.que_front {
                self.scroller.state_mut().cursor = Point::new(x, y);
            }

            y -= 1;

            cur_end = beg_line;
            cur_end = self.buf_dec(cur_end);
        }
    }

    /// Returns `Some(&mut self.scroller)` via the inner `Scroller::as_any_mut`.
    /// Retained as a generic concrete-reach hatch. The `Deferred::ScrollSync`
    /// apply arm does NOT use it — it routes `apply_scroll_sync` to the inner
    /// `Scroller` through the `#[delegate(to = scroller)]` trait forwarder.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        self.scroller.as_any_mut()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use crate::view::{Deferred, Group};
    use std::collections::VecDeque;

    fn make_ctx<'a>(
        out: &'a mut VecDeque<crate::event::Event>,
        timers: &'a mut crate::timer::TimerQueue,
        deferred: &'a mut Vec<Deferred>,
    ) -> Context<'a> {
        Context::new(out, timers, 0, deferred)
    }

    fn make_terminal(w: u16, h: u16, buf_size: usize) -> Terminal {
        Terminal::new(Rect::new(0, 0, w as i32, h as i32), None, None, buf_size)
    }

    // -- Ring-buffer helpers --------------------------------------------------

    #[test]
    fn buf_inc_wraps() {
        let t = make_terminal(10, 5, 8);
        assert_eq!(t.buf_inc(7), 0, "inc wraps at buf_size");
        assert_eq!(t.buf_inc(0), 1);
        assert_eq!(t.buf_inc(6), 7);
    }

    #[test]
    fn buf_dec_wraps() {
        let t = make_terminal(10, 5, 8);
        assert_eq!(t.buf_dec(0), 7, "dec wraps at 0");
        assert_eq!(t.buf_dec(1), 0);
        assert_eq!(t.buf_dec(7), 6);
    }

    #[test]
    fn can_insert_empty_buffer() {
        let t = make_terminal(10, 5, 8);
        // Empty buffer: que_front == que_back == 0.
        // buf_size=8 means 7 usable bytes (buf_size-1 sentinel).
        assert!(t.can_insert(1));
        assert!(t.can_insert(7), "can insert all 7 usable slots");
        // C++: amount=8 → T = 0 - 8 + 8 = 0, queBack(0) > 0 → false
        assert!(!t.can_insert(8), "cannot insert 8 (buf_size) bytes");
    }

    #[test]
    fn can_insert_after_wrap() {
        let mut t = make_terminal(10, 5, 8);
        // Write 5 bytes: que_front=5, que_back=0
        // free = 0 + 8 - 5 - 1 = 2 usable slots
        t.que_front = 5;
        t.que_back = 0;
        assert!(t.can_insert(2));
        assert!(!t.can_insert(3));

        // Wrapped: que_front=2, que_back=5 → free = 5 - 2 - 1 = 2
        t.que_front = 2;
        t.que_back = 5;
        assert!(t.can_insert(2));
        assert!(!t.can_insert(3));
    }

    #[test]
    fn prev_lines_on_simple_sequence() {
        // Buffer: "hello\nworld\n" (indices 0-11), que_front=12, que_back=0.
        // Tracing the C++ algorithm:
        //   prevLines(12, 1): bufDec→11, findLf finds '\n' at 11 → lines=0,
        //     bufInc(11)→12. Returns 12 (the "empty line" after the trailing '\n').
        //   prevLines(12, 2): first finds '\n' at 11, then finds '\n' at 5 (the
        //     '\n' after "hello"), bufInc(5)→6. Returns 6 (start of "world\n").
        //   prevLines(12, 3): reaches queBack=0. Returns 0 (start of "hello").
        let mut t = make_terminal(20, 5, 64);
        let data = b"hello\nworld\n";
        t.buffer[..data.len()].copy_from_slice(data);
        t.que_front = data.len();
        t.que_back = 0;

        let pos = t.prev_lines(t.que_front, 1);
        assert_eq!(pos, 12, "prevLines(front,1) → 12 (empty trailing line)");

        let pos2 = t.prev_lines(t.que_front, 2);
        assert_eq!(pos2, 6, "prevLines(front,2) → 6 (start of 'world')");

        let pos3 = t.prev_lines(t.que_front, 3);
        assert_eq!(pos3, 0, "prevLines(front,3) → 0 (start of 'hello')");
    }

    #[test]
    fn prev_lines_wrap_around() {
        // Ring buffer with wrap: "world\n" at start [0..6], "hello\n" at end [10..16].
        // buf_size=16, que_back=10, que_front=6.
        // Logical order (oldest to newest): "hello\n" (at 10-15) then "world\n" (at 0-5).
        //
        // prevLines(6, 1): que_front=6, queBack=10.
        //   bufDec(6)→5. count=(5>=10?...:5)+1=6. findLf backward from 5, count=6:
        //   buffer[5]='\n' → found. lines=0. bufInc(5)→6. Returns 6.
        //   (The empty trailing line after "world\n".)
        //
        // prevLines(6, 2): iteration 1 — finds '\n' at 5 (lines→1), pos=5.
        //   iteration 2: pos=5≠10. bufDec(5)→4. count=(4>=10?...:4)+1=5.
        //   findLf backward from 4: scans 4,3,2,1,0 — no '\n' (buffer= "world").
        //   Returns (false, 0). pos=0. lines stays 1. Loop continues (no early return).
        //   iteration 3: pos=0≠10. bufDec(0)→15. count=(15-10+1)=6.
        //   findLf backward from 15, count=6: buffer[15]='\n' → found. lines→0. Break.
        //   bufInc(15)→0. Returns 0 (start of "world\n", the byte after "hello\n"'s '\n').
        let mut t = make_terminal(20, 5, 16);
        let first_line = b"hello\n"; // written first, sits at [10..16]
        let second_line = b"world\n"; // written second (wrapped), sits at [0..6]
        t.buffer[10..16].copy_from_slice(first_line);
        t.buffer[0..6].copy_from_slice(second_line);
        t.que_back = 10;
        t.que_front = 6;

        let pos = t.prev_lines(6, 1);
        assert_eq!(
            pos, 6,
            "prevLines(front,1) → 6 (empty trailing line after 'world\\n')"
        );

        let pos2 = t.prev_lines(6, 2);
        assert_eq!(
            pos2, 0,
            "prevLines(front,2) → 0 (start of 'world', C++ faithful)"
        );
    }

    // -- write_bytes / TextDevice --------------------------------------------

    #[test]
    fn write_bytes_counts_newlines() {
        let mut t = make_terminal(20, 5, 256);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            t.init(&mut ctx);
        }
        deferred.clear();
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            let n = t.write_bytes(b"line1\nline2\nline3\n", &mut ctx);
            assert_eq!(n, 18);
        }
        // Initial limit.y = 1; 3 newlines → limit.y = 4.
        // set_limit queues 2 deferred (h and v bars are None → no ops),
        // scroll_to queues nothing either. With no bars, deferred is empty.
        // Check by inspecting the limit directly:
        assert_eq!(t.scroller.limit().y, 4);
    }

    #[test]
    fn write_bytes_evicts_old_lines_when_full() {
        // Small buffer: 16 bytes (15 usable).
        let mut t = make_terminal(20, 5, 16);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            t.init(&mut ctx);
            // Write exactly 14 bytes (one less than usable limit).
            t.write_bytes(b"AAAA\nBBBB\nCCC\n", &mut ctx); // 14 bytes, 3 newlines
            // limit.y = 1 + 3 = 4
            assert_eq!(t.scroller.limit().y, 4);
            // Now write another 5 bytes that force eviction.
            t.write_bytes(b"DDDD\n", &mut ctx); // 5 bytes
            // The buffer cannot hold 14 + 5 = 19 bytes; lines evicted from front.
            assert!(t.que_back > 0, "que_back advanced — old data was evicted");
            // Limit should be bounded (not more than the lines that fit in the buffer).
            assert!(t.scroller.limit().y <= 5, "limit bounded after eviction");
        }
    }

    // -- draw snapshot --------------------------------------------------------

    fn render_terminal(t: &mut Terminal, w: u16, h: u16) -> String {
        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(w, h);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = t.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            t.draw(&mut dc);
        });
        screen.snapshot()
    }

    #[test]
    fn draw_empty_terminal() {
        let mut t = make_terminal(20, 5, 256);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            t.init(&mut ctx);
        }
        insta::assert_snapshot!(render_terminal(&mut t, 20, 5));
    }

    #[test]
    fn draw_with_lines() {
        let mut t = make_terminal(20, 5, 256);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            t.init(&mut ctx);
            t.write_bytes(b"hello\nworld\nfoo\n", &mut ctx);
        }
        insta::assert_snapshot!(render_terminal(&mut t, 20, 5));
    }

    #[test]
    fn draw_with_ring_wrap() {
        // buf_size=32 (31 usable). Write enough data to force que_front to wrap
        // around the end of the ring buffer, exercising the wrap branch in draw().
        let mut t = make_terminal(20, 5, 32);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            t.init(&mut ctx);
            // First write: 19 bytes, 3 newlines — fills most of the 31-byte buffer.
            t.write_bytes(b"first\nsecond\nthird\n", &mut ctx);
            // Second write: 13 bytes — causes que_front to wrap around buf end.
            t.write_bytes(b"fourth\nfifth\n", &mut ctx);
        }
        // Should not panic; snapshot verifies rendering of wrapped ring buffer.
        insta::assert_snapshot!(render_terminal(&mut t, 20, 5));
    }

    #[test]
    fn as_any_mut_returns_scroller() {
        // Verify that as_any_mut() on Terminal downcasts to Scroller (not Terminal),
        // so the ScrollSync pump arm can dispatch apply_scroll_sync to it.
        let mut group = Group::new(Rect::new(0, 0, 20, 5));
        let t = Terminal::new(Rect::new(0, 0, 20, 5), None, None, 256);
        let id = group.insert(Box::new(t));
        let scroller = group
            .find_mut(id)
            .unwrap()
            .as_any_mut()
            .unwrap()
            .downcast_mut::<Scroller>();
        assert!(scroller.is_some(), "as_any_mut must downcast to Scroller");
    }
}
