# TTerminal  (guide pp. 553–555)

Rust module(s): `src/widgets/terminal.rs`   |   magiblot: `include/tvision/textview.h` / `source/tvision/textview.cpp` + `ttprvlns.cpp`

> **Heritage note:** The 1992 guide describes the original Borland Pascal TV 2.0
> implementation. The magiblot C++ port (which is what `tvision-rs` ports from)
> is the authoritative C++ spec; it agrees with the guide on all public
> symbols except for `curLoc` and `screenLines` (Pascal fields not carried
> into the C++ port) and `calcWidth`/`queFull` (not present in C++ at all).
> Those entries are classified below accordingly.
>
> **Architecture shift:** The C++ `TTerminal` inherits `TTextDevice` which
> inherits `TScroller + streambuf`. The Rust port uses embed-and-delegate
> (deviation D2): `Terminal` embeds a `Scroller` and implements `TextDevice`
> as a trait. The C++ stream plumbing (`streambuf`/`do_sputn`/`overflow`/
> `xsputn`) is replaced by the direct `write_bytes` call (deviations D11,
> D12). The constructor's inline setup (`setLimit`/`setCursor`/`showCursor`)
> is deferred to `Terminal::init` because those calls need a `Context` not
> available at construction time.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Init` (constructor) | 553 | PORTED | OK | `tv::Terminal::new(bounds, h_scroll_bar, v_scroll_bar, a_buf_size) -> Terminal` + `Terminal::init(&mut self, ctx)` | 3 | C++ constructor: calls `TTextDevice(bounds, h, v)`, sets `growMode = gfGrowHiX+gfGrowHiY`, caps `bufSize` at 32000, `new`s buffer, then calls `setLimit(0,1)` / `setCursor(0,0)` / `showCursor()` inline. Rust splits this into `new` (geometry + buffer allocation) and `init` (context-requiring limit/cursor calls). `new` doc raised to score 3 with a `# Example` doctest; `init` doc raised to score 3 with a warning about what happens if it is omitted. Both explain the two-step pattern and link each other. |
| `bufSize` (field) | 553 | PORTED | OK | `Terminal.buf_size: usize` | N/A | Private field. C++: `ushort bufSize`, capped at 32000. Rust: `usize`, same cap (`clamp(1, 32000)`). `buf_size - 1` is max usable bytes (one slot is the empty sentinel) — the sentinel semantics are identical to C++. |
| `queFront` (field) | 553 | PORTED | OK | `Terminal.que_front: usize` | N/A | Private field. Write head — next byte goes here. Identical semantics to C++ `ushort queFront`. |
| `queBack` (field) | 553 | PORTED | OK | `Terminal.que_back: usize` | N/A | Private field. Read tail — oldest data starts here. Identical to C++ `ushort queBack`. |
| `curLoc` (field) | 553 | NOT-PORTED | — | — | — | Present in original Borland Pascal TV 2.0 as a `TPoint` tracking current write position; absent from the magiblot C++ port which is `tvision-rs`'s source of truth. The cursor is updated directly during `draw()` (`setCursor(x, y)` / `state_mut().cursor = Point::new(x, y)`) rather than maintained as a persistent field. |
| `screenLines` (field) | 553 | NOT-PORTED | — | — | — | In the Borland Pascal TV 2.0 source, `screenLines` was a persistent field tracking the count of lines. In the magiblot C++ port it appears only as a local variable in `do_sputn` initialized from `limit.y`. The Rust `write_bytes` replicates this exactly: `let mut screen_lines = self.scroller.limit().y`. No persistent field needed or present. |
| `bufDec` (method) | 554 | PORTED | OK | `Terminal::buf_dec(&self, val: usize) -> usize` | 2 | C++: `void bufDec(ushort& val)` — mutates in place. Rust: returns the decremented value (more idiomatic). Identical wrap semantics: `0 → buf_size - 1`. Protected in C++; private in Rust. Doc explains what; "how the wrap sentinel works" could be added. |
| `bufInc` (method) | 554 | PORTED | OK | `Terminal::buf_inc(&self, val: usize) -> usize` | 2 | C++: `void bufInc(ushort& val)` — mutates in place. Rust: returns next value. Identical wrap semantics: `>= buf_size → 0`. Doc explains what; same note as `buf_dec`. |
| `calcWidth` (method) | 554 | NOT-PORTED | — | — | — | Not present in the magiblot C++ port or either C++ header (`textview.h`). Appears to be a Borland Pascal TV 2.0-specific helper for calculating line width in bytes; in C++ this responsibility is folded into `draw()` inline. No counterpart needed. |
| `canInsert` (method) | 554 | PORTED | OK | `Terminal::can_insert(&self, amount: usize) -> bool` | 2 | C++: `Boolean canInsert(ushort amount)` with signed-cast trick for wrap case. Rust replicates the exact two-branch logic with inline comments explaining the signed-comparison equivalence. Private. Doc explains both branches; could note the sentinel-slot design. |
| `do_sputn` / write entry point | 554 | EQUIVALENT | OK | `TextDevice::write_bytes(&mut self, data: &[u8], ctx: &mut Context) -> usize` (impl on `Terminal`) | 3 | C++ `do_sputn(s, count)`: trims oversized input, counts newlines, evicts old lines, writes ring buffer, updates limit+scroll via `drawLock++`/`setLimit`/`scrollTo`/`drawLock--`/`drawView()`. Rust `write_bytes`: same algorithm; `drawLock` and `drawView()` are gone (whole-tree redraw on every pump tick, D9); Context replaces raw pointer calls. The `write_bytes` doc explains the full algorithm. Known mapping: C++ stream plumbing → direct `write_bytes` call (D11/D12). |
| `draw` (method) | 554 | PORTED | OK | `tv::Terminal::draw` (impl `View::draw`) | 3 | C++ `draw()`: calls `setCursor(-1,-1)` to hide cursor, computes `endLine`, fills blank rows below content, iterates rows newest-to-oldest calling `prevLines`/inner-while/`moveStr`/`writeBuf`, calls `setCursor(x,y)` only for the newest line. Rust replicates the full algorithm including the 256-byte chunk inner loop, the wrap branch, `valid_utf8` truncation (D13). One minor difference: the Rust `draw()` does NOT reset the cursor to `(-1,-1)` at entry. This is OK under D9 (the pump issues a whole-tree redraw each tick; the cursor is always set from scratch by `init` or the previous `write_bytes`; there is no scenario where a stale cursor position from a prior draw would survive). The draw doc explains both paths and the cursor placement. Score 3. |
| `nextLine` (method) | 554 | PORTED | OK | `Terminal::next_line(&self, pos: usize) -> usize` | 2 | C++: `ushort nextLine(ushort pos)` — advances past next `\n`. Rust: identical logic, returns new pos. Private helper. Doc explains what it does; a note on its role in eviction (`write_bytes` while-loop) would help. |
| `prevLines` (method) | 554 | PORTED | OK | `Terminal::prev_lines(&self, pos: usize, lines: usize) -> usize` | 3 | C++ in `ttprvlns.cpp`: uses `findLfBackwards` static helper, do-while loop, `bufDec`/`bufInc`. Rust replicates the algorithm faithfully including the no-early-return-on-not-found subtlety; extensive inline comments document the wrap test case. `find_lf_backwards` becomes a private method on `Terminal`. Doc has full ring-wrap explanation and multiple test cases. Score 3. |
| `queEmpty` (method) | 554 | PORTED | OK | `Terminal::que_empty(&self) -> bool` | 3 | C++: `Boolean queEmpty()` — `queBack == queFront`. Rust: identical. Public. Doc raised to score 3: states what the method returns, when to use it (checking whether any text has been written), and notes the O(1) pointer-equality implementation. |
| `queFull` (method) | 555 | NOT-PORTED | — | — | — | Not present in the magiblot C++ port or either C++ header. A Borland Pascal TV 2.0 helper; the C++ `canInsert` subsumes it. No counterpart needed. |
| `CTerminal` palette (1 entry) | 555 | EQUIVALENT | OK | `Role::ScrollerNormal` via `ctx.style(Role::ScrollerNormal)` in `Terminal::draw` | 2 | C++ `TTerminal` inherits `TScroller::getPalette()` which returns `cpScroller = "\x06\x07"` (2 entries). `TTerminal::draw()` calls `mapColor(1)` which resolves through `cpScroller[1]` → window palette slot 6 → `cpAppColor[0x0D] = 0x1E` (yellow-on-blue). Rust: `Terminal::draw` uses `Role::ScrollerNormal` directly (no `GetPalette` override). Known mapping: class Palette → `tv::Theme` (D7). The `Terminal` module doc cites this as "the color map becomes a `Role`". Doc score 2: what the role is, not the full chain. → concept: palette chain guide. |

## Summary

- PORTED: 11   EQUIVALENT: 2   NOT-PORTED: 4   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 1
- Notable finding: All four NOT-PORTED entries (`curLoc`, `screenLines`, `calcWidth`, `queFull`) are Borland Pascal TV 2.0 artifacts absent from the magiblot C++ port that is `tvision-rs`'s source of truth — no gaps. The one undocumented deviation (`draw()` omits `setCursor(-1,-1)` reset at entry) is benign under D9 whole-tree-redraw but is not called out anywhere.
