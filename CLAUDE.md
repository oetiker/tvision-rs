# rstv — idiomatic Rust port of Turbo Vision

**What this is:** a faithful Rust port of **magiblot/tvision** (modern C++ Turbo
Vision). The goal is a framework a C++ tvision veteran recognizes on sight, but
that is native Rust.

## Read these first
- **`docs/PORTING-GUIDE.md`** — the deviation reference. We port *faithfully*
  from the C++; this guide documents **only the places we deviate** (D1–D13),
  each as *Baseline → Deviation → Integration*. Appendix A = C++→Rust symbol
  lookup. Appendix B = the **mechanical per-class porting recipe**.
- **`docs/PORT-ORDER.md`** — dependency-ordered checklist of 92 classes in 6
  phases, with verified C++ file mappings, target Rust modules, and
  `FOUNDATION`/`MECHANICAL`/`INFRA` tags. Port in this order.

## Source trees (not in this repo)
- **Port FROM:** `/home/oetiker/scratch/tvision-spec/magiblot-tvision/`
  (headers `include/tvision/`, impl `source/tvision/`, platform
  `source/platform/`). This is the source of truth — port its behavior verbatim.
- **Lessons reference only:** `/home/oetiker/scratch/tvision-spec/tvision/` is a
  working **Go** port. It was already mined for lessons. **Never reference the Go
  port in the guide or commits** — the guide is purely C++→Rust.

## Methodology (lean by design)
1. **Faithful by default.** If a class/behavior isn't called out as a deviation,
   translate it straight from the C++. No per-file design.
2. **Deviations are pre-decided** in the guide. Apply the relevant D-rules
   mechanically (Appendix B has the line-level substitution table).
3. **Division of labor:** `INFRA` (net-new substrate) and `FOUNDATION`
   (pattern-setting classes) need careful Opus/human work. `MECHANICAL` leaves
   are handed to **Sonnet** via Appendix B + the PORT-ORDER row — they need
   near-zero judgment. Parallelizable batches are listed in PORT-ORDER.md.
4. **Snapshot tests are the verification** (D11): port a piece, run it on the
   `HeadlessBackend`, snapshot, compare to C++ behavior. No heavy upfront plans.

## Locked decisions (details in the guide)
Crate `tvision`, house style `tv::`; drop `T` prefix; `snake_case` methods;
constant families → open newtypes with SCREAMING_SNAKE assoc consts
(`tv::Command::OK`); inheritance → `View` trait + `ViewState` composition;
pointers → `ViewId` handles + downward `Context`; events → `enum Event` + match;
flag words → struct-of-bools; palette+glyphs → `Theme`; whole-tree redraw + diff
(no damage tracking); modal loops → single loop + capture stack; `TStreamable`
dropped (serde if revived). Stack: crossterm (behind a `Backend` trait) →
vendored ratatui cell-buffer+diff (MIT) → retained view tree + event loop.

## Current state
- Planning docs written; methodology established.
- **Crate scaffolded** (`Cargo.toml` pkg `tvision`, edition 2024; `src/lib.rs`
  with the house-style root re-exports + a Phase 0 module map; `NOTICE`).
- **Phase 0 started** (per `docs/PORT-ORDER.md`):
  - rows 1–2 `Point`/`Rect` → `src/view/geometry.rs` (`r#move`/`r#union` keep the
    keyword-colliding names);
  - rows 3–4 `Color`/`Style`/`Modifiers` → `src/color.rs` (D6; all 7 `sl*` flags;
    `Style::reversed()` ports `reverseAttribute`);
  - row 6 `Cell` → `src/screen/cell.rs` (D6+D13; grapheme symbol + `wide`/`trail`,
    derives `PartialEq` for the row-18 diff);
  - row 8 `Text` → `src/text.rs` (D13; `width`/`measure`/`scroll`/`next`/`draw_one`/
    `draw_str[_ex]`. Built on `unicode-segmentation` graphemes + `unicode-width`;
    drops magiblot's UTF-8 DFA — one grapheme cluster = one cell, width from the
    base char, ZWJ sequences clustered);
  - row 7 `DrawBuffer` → `src/screen/draw_buffer.rs` (`move_char`/`move_str[_part]`/
    `move_cstr[_part]`/`move_buf`/`put_char`/`put_attribute`; delegates text to row
    8; dropped the `0 = retain` sentinel — `move_char` always writes both);
  - row 10 `Key` → `src/event/key.rs` (D4/D5; **`enum Key`** + `KeyModifiers` +
    `KeyEvent`, decomposed — no modifier-combined variants, `Ctrl+C`=`Char('c')`+ctrl);
  - row 11 `Event` → `src/event/mod.rs` (D4 `enum Event` + `MouseEvent` (`position`,
    not `where`) + `EventMask`; `infoPtr`/`MessageEvent` dropped — query/broadcast
    split);
  - row 12 `Command` → `src/command.rs` (D1; **`Command(&'static str)`** namespaced
    open newtype + `Command::custom`; `CommandSet` over `HashSet`, no range guard;
    only shared-vocabulary consts here, view-specific ones live with their view).
  - **INFRA substrate (rows 5, 17, 18, 19, 20):**
    - row 5 quantization ladder → `src/backend/quantize.rs` (D6; faithful port of
      `platform/colors.cpp` + `colors.h` conversions — NOT `mapcolor.cpp`/
      `palette.cpp`, which are the D7 palette-chain walk; PORT-ORDER row 5's file
      cite was corrected). `rgb_to_xterm16/256`, `rgb_to_bios`, `bios_to_xterm16`,
      `xterm256_to_xterm16/rgb`; compile-time LUTs.
    - row 17 `ViewId` arena → `src/view/id.rs` (D3; generational *identity*
      allocator only — NOT a view store; `Option<ViewId>` niche via `NonZeroU32`;
      resolution to `&dyn View` is a later tree-walk via `Context`).
    - row 18 `Buffer` + diff → `src/screen/buffer.rs` (D8; ratatui-adapted diff,
      MIT attribution in-file; wide-char skip driven off our `wide`/`trail` flags;
      no `skip` field).
    - row 19 `Backend` + `Renderer` + `HeadlessBackend`/`HeadlessHandle` +
      `CrosstermBackend` → `src/backend/{traits,renderer,headless,crossterm_backend}.rs`
      (D11). **Object-safe `Box<dyn Backend>`** (slice-based `draw`, `poll_event`
      collapses the associated `EventSource`); `Renderer` owns back/front buffer
      pair + draw cycle; `HeadlessBackend` shares state via `HeadlessHandle` for
      test inspection/injection (no downcast); Crossterm does key/mouse/color
      translation. Deps added: `crossterm`, dev-dep `insta`.
    - row 20 `Clock` + `TimerQueue` → `src/timer.rs` (D9/D11; injected
      `Clock`/`SystemClock`/`ManualClock`; `calc_next_expires_at` verbatim; dropped
      the `collectId` dance — collect-then-dispatch; clock passed in, not stored).
  - **Snapshot format (FOUNDATION) frozen** in `src/screen/snapshot.rs`
    (`snapshot(&Buffer, cursor) -> String`): `size`/`cursor`/`text`/`attr`/`legend`,
    `|`-framed rows, `.`=default style, per-display-column attr keys, wide glyph
    keyed twice + trail absorbed. First end-to-end snapshot test in
    `tests/render_pipeline.rs`. Every future widget test diffs against this.
  - 130 unit/integration tests green; `cargo clippy --all-targets` and
    `cargo fmt --check` clean.
  - Coordinates are `i32` (faithful to magiblot's `int`).
  - Deps: `unicode-segmentation`, `unicode-width`, `crossterm`; dev: `insta`.
- **Key design decisions** (recorded in `docs/PORTING-GUIDE.md` D1/D4): newtype vs
  enum by *extensibility* — open/app-extensible families (`Command`, `HelpCtx`) →
  open newtype with namespaced `&'static str` identity; closed sets (`Key`) → enum.
  Constants live with their owner (no central registry).
- Git on `main`; Phase 0 rows 1–12 are **committed** (last commit `010584f`);
  rows 5, 17, 18, 19, 20 + the snapshot format are **uncommitted** (commit only
  when asked).

## Next step
Finish Phase 0 with the last two INFRA rows, the most interlocking in the phase:
- **minimal `Theme`** (partial row 16, a DrawCtx dependency): `Role` is a
  *first-party* enum that grows per-widget (D7 — not a newtype, not third-party
  extensible), `Glyphs` a near-empty stub (row 9 not needed yet), default = classic
  blue. Pulled in early only because `DrawCtx` needs `&Theme`.
- **row 21 capture stack** (D9): LIFO `CaptureHandler`s holding `ViewId`; modal/
  drag/press become handlers, not nested loops. The handler signature references
  `Context`, so 21 and 22 co-design.
- **row 22 `Context`/`DrawCtx`** (D3/D4): `DrawCtx` (buffer + clip + origin + theme;
  re-expresses `DrawBuffer` ops) is safe to build fully; the event-side `Context`
  is anchored to Appendix B's decided `ctx.*` calls (`broadcast`/`query`/timer
  scheduling/`push`/`pop_capture`). The one tricky bit: the loop owns the capture
  stack and `Context` exposes push/pop so a running handler isn't borrowed from the
  thing it mutates. Full loop wiring is deferred to `TProgram` (row 31).
Then Phase 0 is complete and Phase 1 (`TView` row 23) can begin.

## Conventions
- English for all code/comments/identifiers (user-facing strings may be localized).
- Commit messages end with the project's Co-Authored-By trailer; commit/push only
  when asked.
