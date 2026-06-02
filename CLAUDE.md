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

## How to run the port (subagent-driven, the default from Phase 1 on)

Phase 0 was FOUNDATION/INFRA — interlocking, design-heavy, mostly serial. Phase 1+
is **mostly `MECHANICAL` leaf widgets in parallel batches** (PORT-ORDER Batches
A–E), so the orchestrator runs it as **subagent-driven development**
(`superpowers:subagent-driven-development`). The main thread does **only**
coordination: design FOUNDATION seams, write precise prompts, integrate, decide.
Per row:

1. **Implementer subagent (fresh, isolated context).** Give it a *self-contained*
   prompt — never "go read the plan." Inline: the PORT-ORDER row, the relevant
   C++ source (from `magiblot-tvision/`), the D-rules that apply (Appendix B
   table), the existing types it builds on, and "run `cargo test`/`clippy
   --all-targets`/`fmt --check` + add a snapshot test (Appendix B step 4)."
   **Model by tag:** `MECHANICAL` → Sonnet; `FOUNDATION`/`INFRA` → Opus (or the
   main thread).
2. **Two-stage review (fresh subagents — do NOT just self-review in the main
   thread).** First a **spec-compliance** reviewer (does it match the C++
   behavior + the row's D-rules, nothing extra/missing?), then, once that's ✅, a
   **code-quality** reviewer. Implementer fixes; re-review until clean.
   (`feature-dev:code-reviewer` / `gsd-code-reviewer` agent types fit, or a
   plain agent with the row's spec.)
3. **Integrate + verify in the shared tree**, then mark the row done.

**Parallelism (the reconciliation):** the skill says "never dispatch parallel
implementers" because of shared-tree conflicts — but PORT-ORDER's batches are
*build-disjoint*, so dispatch them **concurrently using `isolation: "worktree"`**
(each agent self-verifies in its own checkout; the orchestrator integrates). Run
serially only for shared files (`lib.rs`, a shared `mod.rs`) and FOUNDATION rows
that gate others. The orchestrator owns the few shared-file edits (module wiring,
re-exports) to avoid races.

**Worktree gotcha (learned this milestone):** an agent worktree is branched from
the last **commit**, so uncommitted work is absent from it. **Commit completed
rows before dispatching worktree subagents that build on them** (or the agent
wastes effort re-deriving prereqs). Commit at batch boundaries.

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
    - row 16 (minimal) `Theme` → `src/theme.rs` (D7; `Role` first-party closed
      enum seeded with D7's enumerated needs, grows per-widget; `Glyphs` empty stub
      [row 9 deferred]; `classic_blue` default with *provisional* BIOS colors).
    - row 21 capture stack → `src/capture.rs` (D9; `CaptureStack` of
      `Box<dyn CaptureHandler>`; `CaptureFlow::{Pass,Consumed,ConsumedPop}` — pop
      is a return value [unambiguous: the handler that just ran], push is deferred
      via `Context`; live loop deferred to row 31).
    - row 22 `Context`/`DrawCtx` → `src/view/context.rs` (D3/D4). `DrawCtx` =
      clipped+themed writer (`put_char`/`put_str`/`fill`/`sub`; clip intersected
      with buffer bounds at construction → panic-safe; reuses row-7/8 text
      primitives). `Context` = event-side ctx (`post`/`broadcast`/`set_timer`/
      `kill_timer`/`push_capture`; distinct `&mut` fields for disjoint borrows);
      `query`/focus deferred to `TGroup` (row 26). Borrow model: loop owns the
      capture stack, `Context` only queues deferred pushes — proven by the
      `compose_full_protocol` hand-played-loop test in `src/capture.rs`.
  - **Snapshot format (FOUNDATION) frozen** in `src/screen/snapshot.rs`
    (`snapshot(&Buffer, cursor) -> String`): `size`/`cursor`/`text`/`attr`/`legend`,
    `|`-framed rows, `.`=default style, per-display-column attr keys, wide glyph
    keyed twice + trail absorbed. First end-to-end snapshot test in
    `tests/render_pipeline.rs`. Every future widget test diffs against this.
  - **Phase 0 is COMPLETE** (all primitives + INFRA substrate). The only
    deferred Phase-0 item is row 9 (glyph tables → `Glyphs`), a stub that fills in
    per-widget. 155 unit/integration tests green; `cargo clippy --all-targets` and
    `cargo fmt --check` clean.
  - **Phase 1 started — row 23 `TView` DONE** (`src/view/view.rs` + `src/help.rs`):
    the `View` trait (`state`/`state_mut`/`draw` required; `handle_event`[no-op
    base]/`valid`/`awaken`/`size_limits`/`calc_bounds`/`change_bounds` defaulted) +
    `ViewState` composition + the four D5 struct-of-bools (`State`/`Options`/
    `GrowMode`/`DragMode`) + `HelpCtx` open newtype (D1). Key decisions: base
    `handle_event` is a no-op — TView's only body (mouse-down→`focus()`) **relocates
    to TGroup row 26** (D3; breadcrumb incl. the `sfSelected`/`sfDisabled` guard is
    in the `view.rs` module doc); no `getColor`/`getPalette`/`mapColor` on the trait
    (views call `ctx.style(Role)`, D7); occlusion/buffered/`sfExposed` dropped (D8);
    `dragView`→row 33, data/`value`→D10/row 39, enabled-command set→TProgram row 31
    (the `>255`-always-enabled rule **dropped**, D1). `calcBounds`/`sizeLimits`
    growMode math ported verbatim (incl. `resize_balance` recovery) and unit-tested.
    Two-stage reviewed (spec PASS, quality PASS). 173 unit + 3 integration tests
    green; clippy/fmt clean.
  - **Batch A DONE — rows 29 `TBackground` + 25 `TScrollBar`** (parallel worktree
    implementers, orchestrator-integrated). `TBackground` → `src/desktop/`
    (pattern fill, `Role::Background`, `gfGrowHiX|HiY`). `TScrollBar` →
    `src/widgets/scrollbar.rs` (value/min/max/steps, draw via the new `Glyphs`
    scrollbar set, broadcasts `cmScrollBarChanged`). The `Glyphs` stub became a
    real struct (theme.rs) seeded with the CP437→Unicode scrollbar chars (▲▼◄►/▒
    trough U+2592/▓ no-range U+2593/■ thumb) — the **row-9 "glyphs fill in
    per-widget" convention**. Reviewed (spec+quality); fixed a CP437 shade
    off-by-one and the mouse-down page-click (must thumb-**jump** to cursor, not
    page-step — page-step is keyboard-only). **DEFERRED to D9/row 31:** scrollbar
    press-and-hold auto-repeat + thumb-drag (the C++ `mouseEvent` loops);
    `ctrlToArrow` WordStar nav (shared helper, port centrally later). 197 unit + 3
    integration tests green; clippy/fmt clean.
  - **Row 26 `TGroup` DONE** (FOUNDATION, `src/view/group.rs` + `View` trait
    growth in `view.rs`): the view container + event router. `Group` owns a
    group-local `ViewArena` + `children: Vec<Child{id,view}>` in **back-to-front
    paint order** (`children[0]`=C++ `last`/bottom, `.last()`=`first()`/top;
    `forEach`/`firstThat`=`iter().rev()`), `current: Option<ViewId>` resolved by
    `index_of`. Implements: three-phase `handle_event` (D4: focusedEvents→pre/
    focused/post, positional→topmost-visible-under-cursor, broadcast→all; with the
    `doHandleEvent` disabled/eventMask/phase gates), `draw` (drawSubViews
    back-to-front, painter's algorithm — **reversed** from C++ occlusion draw, D8),
    `change_bounds`/`valid`/`awaken`/`size_limits`, and the focus machinery
    (`insert`/`remove`/`set_current`/`reset_current`/`focus_child`/`find_next`/
    `focus_next`). **Carryovers landed:** mouse-down→auto-select (relocated
    `TView::handleEvent`) + the `sfFocused` focus broadcast — via a new defaulted
    `View::set_state(StateFlag,enable,ctx)` trait method (base flips bit +
    broadcasts `RECEIVED_FOCUS`/`RELEASED_FOCUS` on `Focused`; `Group` overrides to
    propagate `Active`/`Dragging`→all, `Focused`→current) + `StateFlag` enum +
    `ViewState::set_flag`. **Deviation:** mouse position is **view-local at each
    level** (the group subtracts child `origin` before delivery — the downward
    realization of `makeLocal`/`mouseInView`); `eventError`/bubble = leaving the
    event un-cleared as the `handle_event` stack unwinds (no owner pointer).
    **Deferred (per design):** `execute`/`execView`/the live blocking loop/modal-
    via-capture/`endModal` → row 31 (the loop owns the capture stack, so a group
    can't run a modal itself); `ofTopSelect`/`makeFirst`/`putInFrontOf` Z-reorder →
    row 33 (so `select` always goes through `set_current(Normal)`); `getData`/
    `setData`/`dataSize` → D10/row 39; `Context::query` not needed (group resolves
    `ViewId` internally); shadow casting → row 33 (no infra). **Note for row 31/33:**
    `insert` does **not** activate children (faithful — C++ `insertBefore`'s
    `sfActive`-restore is a no-op under D8); a child's `sfActive` must come from the
    group's `set_state(Active)` propagation / focus logic when that lands.
    Two-stage reviewed (SPEC-PASS + QUALITY-PASS; fixed an insert-`sfActive`
    faithfulness bug the spec reviewer caught). 214 unit + 3 integration tests
    green; clippy/fmt clean.
  - **Row 24 `TFrame` DONE** (FOUNDATION, `src/frame.rs` + `Frame`/`WindowFlags`;
    + `DrawCtx::put_cstr` in `view/context.rs`; + frame glyph set in `theme.rs`):
    the window border + centered title + number + close/zoom/resize icons, drawn
    through `DrawCtx`. **Owner-data-down seam (D3):** no owner pointer — `Frame`
    holds its own `title`/`flags`/`number`/`zoomed`, pushed down via setters by the
    owning `TWindow` (row 33); reads `sfActive`/`sfDragging` from `self.st.state`
    (arrive via `Group::set_state` propagation). `handle_event` uses the group's
    view-local coords (`makeLocal` gone). **D7:** border roles `FrameActive`/
    `FramePassive`/`FrameDragging` (dragging first→single-line; active→double-line;
    else passive single-line), icons→`FrameIcon`; title reuses the border role
    (distinct title palette 2/4 + blue/cyan/gray window schemes → row 33).
    **New FOUNDATION seam `DrawCtx::put_cstr`** ports `moveCStr`'s `~`-toggle
    (lo↔hi, `~` not drawn) — reused by buttons/labels/menus. `Glyphs` grew a frame
    set (single/double box + tee/cross + 5 icon strings; row-9 convention).
    **handle_event:** posts `cmClose`/`cmZoom` on resolvable row-0 clicks (close
    x∈2..=4, zoom x∈(w-5)..=(w-3) or double-click; active+y0). **DEFERRED to row
    33 (`TODO(row 33, D9)`):** close press-and-hold confirm (we post on down),
    `wfMove` drag, grow drags, middle-button move. **No `set_state` override** (C++
    only `drawView`, D8). **Sibling tee-walk (`├┬┤┴`) deferred** — under D3 a child
    sees no siblings; plain corners are byte-identical to C++ for the common case;
    full `framelin.cpp` `FrameMask`/`frameChars`/`initFrame` machinery + sibling
    walk lands later (needs `Group` cooperation). **Faithfulness catch:** base
    `TWindow::getTitle(short)` ignores its arg → the `-6`/`-4` budget never caps
    the drawn title (capped at `width-10`). **Relocate `WindowFlags` to the
    `window` module at row 33.** Two-stage reviewed (SPEC-PASS + QUALITY-PASS;
    fixed title-clone, dead-`l`, edge-glyph naming, empty-title flanking spaces).
    229 unit + 3 integration tests green; clippy/fmt clean.
  - **Row 31 `TProgram` DONE** (FOUNDATION, module `app` = `src/app/{mod,program}.rs`;
    + new `View::cursor_request` in `view.rs` + a `Group` override): TV's single
    event loop (D9), making the row-21 capture stack and row-20 timer queue
    **live**. `Program` **embeds a `Group`** (D2) + loop machinery (`Renderer`,
    `CaptureStack`, `TimerQueue`, injected `Box<dyn Clock>`, `out_events` queue,
    `pending_captures`, `CommandSet`, desktop `ViewId`, `end_state`). **`pump_once`**
    is the D9 `getEvent`→`handleEvent`→(eventError): drain queue → poll → capture
    stack **first** → `program_handle_event` (Alt-N stubbed, group delegate,
    `cmQuit`→`end_modal`) → apply deferred captures → resetCursor → whole-tree
    redraw+diff. **Borrow model:** top-of-fn `let Program{..}=self` destructure +
    free fns with explicit field borrows (preserves `Context`'s disjoint fields —
    the pattern to copy). **`run`** ports `TGroup::execute` incl. the outer
    `while(!valid(end_state))`; factory-injected ctor (status-line/menu-bar `None`
    stubs, real `Group`+`Background` desktop, made `current`). **resetCursor** =
    defaulted `View::cursor_request` (base: focused+cursor_vis) + `Group` descends
    `current` accumulating origin → absolute cursor (set before render).
    **Command-enable:** `curCommandSet` is an explicit allowlist (">255 always
    enabled" DROPPED, D1); `cmZoom/cmClose/cmResize/cmNext/cmPrev` seeded disabled;
    enable/disable flips `command_set_changed`→idle `cmCommandSetChanged` broadcast;
    disabled `Event::Command` filtered at the program boundary. **Resize:** poll
    `backend.size()` each pass (no `Event::Resize` churn; the D9 `setScreenMode`).
    **Modality MECHANISM only:** a `ModalFrame` capture handler gates positional
    events to a modal view; **the blocking `exec_view`/`executeDialog`/`getData` +
    the frame-pop (which must be conditional on `valid(end_state)` — Program state a
    `CaptureHandler` can't reach) defer to row 34** (zero pop-path test coverage
    until then). **Deferred-payload (D4):** timer-id + Alt-N window-number gone;
    breadcrumbs in `program.rs`. Implementer brief: `docs/briefs/row31-tprogram.md`
    (a FOUNDATION-brief template). Two-stage reviewed (SPEC-PASS; QUALITY-PASS after
    fixing a misattached doc comment + the `ModalFrame` gating doc). 238 unit + 3
    integration tests green; clippy/fmt clean.
  - Coordinates are `i32` (faithful to magiblot's `int`).
  - Deps: `unicode-segmentation`, `unicode-width`, `crossterm`; dev: `insta`.
- **Key design decisions** (recorded in `docs/PORTING-GUIDE.md` D1/D4): newtype vs
  enum by *extensibility* — open/app-extensible families (`Command`, `HelpCtx`) →
  open newtype with namespaced `&'static str` identity; closed sets (`Key`) → enum.
  Constants live with their owner (no central registry).
- Git on `main`; Phase 0 rows 1–12 committed (`010584f`); the INFRA substrate
  (rows 5,17,18,19,20 + snapshot format) committed (`7f6edd9`); Phase-0 rows
  16(min), 21, 22 committed (`8045847`); **Phase-1 row 23 (`TView`) committed**
  (`a08412d`); **Batch A rows 29+25 (`TBackground`/`TScrollBar`) committed**
  (`91c50a6`); **Phase-1 row 26 (`TGroup`) committed** (`4d12a32`); **Phase-1 row
  24 (`TFrame`) committed** (`25d10b6`); **Phase-1 row 31 (`TProgram`) committed**
  (`bff4885`); **Phase-2 row 30 (`TDeskTop`) committed** (`c80a20d`); **row 33a
  (Group/Context primitives) committed** (`4da4f52`); **row 33b (`TWindow` core)
  committed** (`d44e39b`); **row 33c (`TWindow` zoom) committed** (`432c01a`).
  - **Phase-2 row 34 (`TDialog`) DONE** (module `dialog` = `src/dialog/{mod,dialog}.rs`;
    + `Program::exec_view` + `Deferred::EndModal`/`Context::end_modal` +
    `CaptureStack::pop` + `Window::set_flags`/`set_palette`/`set_grow_mode`): **the
    modality payoff.** `Dialog { window: Window }` is the D2 embed-and-delegate
    exemplar one level deeper (delegates all of `View` to the window except
    `handle_event` + `valid`); ctor overrides `flags = wfMove|wfClose`,
    `growMode = 0`, `palette = Gray`. **`handle_event`** ports `TDialog::handleEvent`
    (delegate to `Window::handle_event` FIRST, then Esc→post `cmCancel`,
    Enter→broadcast `cmDefault`, `cmOK/cmCancel/cmYes/cmNo`→`endModal` **iff
    sfModal**); **`valid`** = `cmCancel`→true else `Group::valid`. **`exec_view`**
    is the FOUNDATION crux (D9 "exec_view — corrected"): it ports `TGroup::execView`
    + `execute` as a **nested `while end_state.is_none() { pump_once() }`** loop
    that is sound because a `View` holds only `&mut Context`, never `&mut Program`,
    so **the compiler bars a view from calling it mid-dispatch** (the sync loop only
    runs top-level — startup / app `main` / a test driving pre-queued events).
    Faithful steps: save/restore `current` + `command_set` (getCommands/setCommands),
    insert at root (faithful to `application->execView`; `deskTop->execView` =
    `executeDialog`'s variant — breadcrumbed for Phase-4 desktop-inset), clear
    `ofSelectable`, set `sfModal` **directly** (C++ `setState` never propagates
    sfModal), `set_current(Enter)`, push `ModalFrame` directly, run the inner+outer
    (`while !valid`) loops, then **`captures.pop()` + `remove` conditional on the
    modal's OWN `valid`** (NOT the root group's — a spec-review BLOCKER caught from a
    wrong brief: `TGroup::execute`'s `while(!valid)` is virtual on `p`=the dialog,
    `tgroup.cpp:184/205`; the root-scoped check ANDs the desktop sibling → latent
    hang). **`endModal` is downward** (D3): the dialog can't reach `Program` so it
    requests `ctx.end_modal(cmd)`→`Deferred::EndModal`→pump sets `end_state` (the
    `69897fe` "new capability adds a Deferred variant" rule; 4th disjoint state
    target, order-equivalent). **`CaptureStack::pop`** added (the one place a frame
    is popped other than `ConsumedPop`; the loop owns the stack so `exec_view`, not
    the handler, does the `valid(end_state)`-conditional pop). **DEFERRED (no
    consumer at row 34, breadcrumbed, no dead stubs):** gray multi-scheme theming
    (`palette = Gray` recorded; frame still renders blue — `TODO(row 34 gray
    theming)`); `getData`/`setData`/`dataSize` (D10 — no data controls until Batch
    B); the return-consuming `message()`/`query` + `cmCanCloseForm` veto (`valid`
    uses only `Group::valid`); view-/menu-triggered async modal
    (`Deferred::OpenModal` + posted completion) → Phase 4; `msgbox`. **TheTopView
    dropped** (D8, no occlusion). `TheTopView`/post-remove sfModal-restore moot
    (view dropped on remove). Brief: `docs/briefs/row34-tdialog-modal.md`; guide
    amended (D9 "exec_view — corrected"). Two-stage reviewed (SPEC-FAIL→fixed the
    validation-scope BLOCKER + a discriminating bite-verified sibling-veto test +
    root-insert breadcrumb; QUALITY-PASS, one find_mut-consolidation nit applied).
    299 lib + 3 integration + 1 doctest green; clippy/fmt clean. Working tree clean.
  - **SUBSTRATE realigned (`7b15782`)** — mid-33d we stopped to fix a foundation
    instead of bandaiding around it. `ViewId` was **group-local** (each `Group`
    embedded its own generational `ViewArena`) — an unexamined default that
    contradicted D3's own "resolve a `ViewId` by tree-walk" promise and whose
    `is_valid` was dead code; it was the real obstacle behind 33d's drag/close.
    Now: one **process-global monotonic `ViewId`** (`NonZeroU64`), each view knows
    its own id (`ViewState.id`, stamped at `Group::insert`), resolved by
    `View::find_mut(id)` / `remove_descendant(id, ctx)` (Group recurses;
    Window/Desktop delegate; Frame leaf). Guide corrected: **D3 "Resolution
    substrate — corrected"** + **D4 "`message()` — corrected"**. The D4 amendment
    (designed, NOT built): `message()` ports directly onto the substrate — a
    tree-owner `message(id, ev) -> Option<ViewId>` over `find_mut`, plus a
    `ViewId` source on `Broadcast` (the resolvable `infoPtr` successor); the
    audit (42 sites) shows every return-consuming `message()` is owner-initiated,
    so the aliasing rule bars only a pattern that never occurs. Two-stage
    reviewed; 271 tests green. Working tree clean. Phase-2 stage detail lives in
    [`docs/HANDOVER.md`](docs/HANDOVER.md), not duplicated here.
  - **Phase A — `Broadcast{source:ViewId}` DONE (`7efecb3`)** — the *buildable*
    slice of the D4 amendment, landed ahead of 33d. `Event::Broadcast(Command)`
    → `Event::Broadcast { command, source: Option<ViewId> }`: `source` reinstates
    the C++ broadcast-subject `infoPtr` (`this`) as a resolvable `ViewId`, threaded
    from each emitter (focus broadcasts in `view.rs`/`group.rs` `set_state`;
    scrollbar's 4 changed/clicked sends; `None` for pump-internal + capture).
    **Data-only — no receiver reads `source` yet** (first consumer = a two-bar
    scroller, Batch B); routing unchanged. **Investigation collapsed the rest of
    Phase A to docs** (the `infoPtr` is *polymorphic*, only the subject case ports
    to a `ViewId`): the `cmZoom`/`cmClose` `infoPtr==0||==this` guard is **provably
    vacuous** (frame posts only while `sfActive` → owner is the active window, and
    focused commands route to `current` only) — **not** rebuilt; Alt-N's payload is
    an *integer* (window number) + needs `select`/`canMoveFocus` → **deferred to
    33d** as a direct number-walk, not "blocked on a payload story"; the
    return-consuming `message()`/`query` primitive → **row 34** (its first
    consumer, a dialog `cmCanCloseForm` veto). Guide `D4 "message()"` rewritten for
    the polymorphism. Brief: `docs/briefs/row33-phaseA-broadcast-source.md`.
    Two-stage reviewed (SPEC-PASS + QUALITY-PASS). 268 unit + 3 integration + 1
    doctest green; clippy/fmt clean. Working tree clean.
  - **Row 33d-1 — TWindow drag + close + setState DONE (`2887e95`)** — the
    *interactive* half of `TWindow`. **33d was split** at its natural seam (advisor
    call): 33d-1 = drag/close/setState; 33d-2 = selection (cmNext/cmPrev + Alt-N).
    The split kept 33c's "enable only handled commands" principle clean. Built:
    (a) a **deferred tree-op channel** on `Context` —
    `request_bounds`/`request_set_state`/`request_close` (shipped as a `TreeOp`
    enum + 3rd parallel channel, since **unified into one `Deferred` queue** by
    `69897fe`, see below); the pump drains+applies it after dispatch
    against the root via `find_mut`/`change_bounds`, `find_mut`/`set_state`,
    `remove_descendant` (drain-to-local-then-rebuild-ctx, the row-31 destructure
    discipline). (b) **Drag = a `DragCapture` capture handler** (D9, replaces
    `dragView`'s nested `mouseEvent` loop): the **window** — not the frame (D3: a
    frame can't name the window it moves) — starts the drag from a still-live
    `MouseDown` *after* group delegation, replicating `TFrame::handleEvent`'s
    geometry (title→Move, bottom corners→Grow/GrowLeft, middle-btn→Move);
    `move_grow` ports `TView::moveGrow` verbatim (`min(max())` **not** `clamp` — it
    panics on lo>hi); `sfDragging` on directly (propagates to the frame's
    single-line border) / off via the deferred channel on `MouseUp`. **(0,0)-desktop
    absolute-coords assumption documented** on `DragCapture` (matches `ModalFrame`'s
    caveat — revisit when a menu/status bar shifts the desktop, Phase 4). (c)
    **`cmClose`** → if `sfModal` post `cmCancel` (row 34 owns teardown) else
    `request_close` if `valid(cmClose)`; **no target guard** (Phase A vacuous). (d)
    **`setState`** enable set = `{cmClose if wfClose, cmZoom if wfZoom}` (cmNext/
    cmPrev → 33d-2; cmResize stays un-enabled). Brief: `docs/briefs/row33d-1-drag-
    close.md`. Two-stage reviewed (SPEC-PASS after strengthening a vacuous
    `dmLimitLoY` clamp test; QUALITY-PASS). 278 unit + 3 integration + 1 doctest
    green; clippy/fmt clean. Working tree clean.
  - **Refactor — unified `Deferred` channel DONE (`69897fe`)** — the three
    post-dispatch deferred channels (`pending_captures`/`command_changes`/
    `pending_tree_ops`) were one concept grown as three parallel structures: *an
    effect on loop-owned state a downward-borrowed view can't perform inline* (the
    tree is a live `&mut` borrow stack during dispatch). Collapsed into a single
    **`Deferred {PushCapture, EnableCommand, DisableCommand, ChangeBounds,
    SetState, Close}`** enum + one `deferred: Vec<Deferred>` queue (`TreeOp`
    removed). **`Context::new` is now 4 params** `(out_events, timers, now_ms,
    deferred)`; the request methods push variants; the pump drains in one
    `mem::take`+match loop. **A future deferred capability ADDS A VARIANT, not a
    `Context::new` param** — the per-row call-site churn stops. Boundary held:
    `post`/`broadcast` stay on `out_events` (input-stream, not `Deferred`). Pure
    refactor, no behavior change; ordering-equivalence verified in review (the 3
    apply-families touch disjoint state, and no dispatch co-queues order-dependent
    different kinds). Design note: `docs/design/deferred-effects.md`. Reviewed PASS
    (no findings). 282 tests green; clippy/fmt clean. Working tree clean.
  - **Row 33d-2 — window selection DONE (`15c601d`)** — the *selection* half of
    `TWindow`; **row 33 (`TWindow`) is now COMPLETE**. **`View::number() ->
    Option<i16>`** (trait, default `None`; `Window` overrides `Some(n)` iff `n>0`
    — drops Window's inherent getter, no name clash) + **`View::select_window_num`**
    (trait tree-op, default no-op; `Desktop` overrides → `group.focus_by_number` —
    via the **trait method, NOT an `as_any_mut` downcast**, keeping `Program`
    decoupled) + **`Group::focus_by_number`** (selects the `ofSelectable` child whose
    `number()` matches, via `focus_child`). **Select-vs-focus crux:** C++ uses
    `select()` (cmNext via `selectNext`, Alt-N via the window's `select()`); we have
    only `focus_child` (== `select()` + an outgoing `valid(cmReleasedFocus)` guard).
    That guard is **redundant-but-harmless** — both call sites are gated upstream
    (cmNext via the desktop's `valid`; Alt-N via `canMoveFocus`) and windows carry
    `ofTopSelect` so `focus_child`→`make_first` raises them exactly as `select()`
    does (reasoning in code comments at each site). **TDeskTop `cmNext`/`cmPrev`**
    (`tdesktop.cpp` port): `ev.clear()` **outside** the `valid()` guard (C++ `break`
    falls through); other commands → no clear (`default: return`); cmNext=
    `focus_next(false)`, cmPrev=`put_in_front_of(current, Some(background))`.
    **Alt-N (`cmSelectWindowNum`)** in `program_handle_event` **before**
    `group.handle_event` (faithful order) as a **direct walk** (the number is an
    integer, not a `ViewId` — `Broadcast{source}` doesn't serve it), with the
    three-way clear matrix (`can&&matched`→clear, `can&&!matched`→**event stays
    live**, `!can`→clear); `canMoveFocus` checks the **desktop's**
    `valid(RELEASED_FOCUS)`; added `desktop: Option<ViewId>` param. **`setState`**:
    `{cmNext, cmPrev}` enabled **UNCONDITIONALLY** (C++ has no flag guard, unlike
    cmClose/cmZoom); `cmResize` stays **dropped** (keyboard-resize sub-mode
    deviation). Two-stage reviewed (SPEC-PASS; QUALITY-PASS after making two
    round-trip tests discriminating — the no-match Alt-N test asserts the event
    stayed **live** via a recording probe [verified it bites], the cmNext-cycle test
    drives the enable through a **real `pump_once` drain**, deleting the
    `clear_deferred` scaffold). Brief: `docs/briefs/row33d-2-selection.md`. 287 lib +
    3 integration + 1 doctest green; clippy/fmt clean. Working tree clean.

## Next step
**Phase 2 in progress.** Continue subagent-driven (see "How to run the port"
above). Sequence:

1. ~~**Row 23 `TView`**~~ ✅ DONE. The pattern every widget embeds: embed
   `ViewState`, `impl View`, draw through `DrawCtx`, events through `Context`.
2. ~~**Batch A — `TScrollBar` 25 ∥ `TBackground` 29**~~ ✅ DONE. `Glyphs` is now a
   real per-widget struct.
3. ~~**`TGroup` 26**~~ ✅ DONE (FOUNDATION, see Current state). The view container +
   three-phase router + focus machinery + both row-23 carryovers landed; modal/
   live-loop deferred to row 31.
4. ~~**`TFrame` 24**~~ ✅ DONE (FOUNDATION, see Current state). Border/title/icons +
   `DrawCtx::put_cstr` + frame `Glyphs`; owner-data-down seam designed and waiting
   for `TWindow`; sibling tee-walk + drag loops deferred (rows 33/31).
5. ~~**The live loop (`TProgram` 31)**~~ ✅ DONE (FOUNDATION, see Current state).
   The single event loop (D9); capture stack + timer queue now live; modality
   mechanism (`ModalFrame`) shipped, the blocking `exec_view` + frame-pop deferred
   to row 34. Implementer brief in `docs/briefs/`.
6. **Phase 2 — `TDeskTop` 30 → `TWindow` 33 → `TDialog` 34**, FOUNDATION, main
   thread/Opus. The path to "a window you can see and drive."
   - ~~**`TDeskTop` 30**~~ ✅ DONE (`c80a20d`). `Group`+owned-`TBackground`; gives
     `Program` a real named desktop.
   - ~~**`TWindow` 33**~~ ✅ **COMPLETE** (module `window`) — the D2
     embed-and-delegate exemplar, staged: ~~33a Group/Context primitives~~ ✅,
     ~~33b core~~ ✅, ~~33c zoom~~ ✅, ~~SUBSTRATE realign (global `ViewId`)~~ ✅
     (`7b15782`), ~~Phase A `Broadcast{source}`~~ ✅ (`7efecb3`),
     ~~33d-1 drag/close/setState~~ ✅ (`2887e95`),
     ~~33d-2 selection (cmNext/cmPrev + Alt-N + numbered windows)~~ ✅ (`15c601d`,
     see Current state).
   - ~~**`TDialog` 34**~~ ✅ **DONE** (module `dialog`; see Current state). The
     modal payoff: `Dialog` embeds `Window` (D2 one level deeper) + `Program::
     exec_view` (the nested-pump modal lifecycle — sound because a `View` holds
     only `&mut Context`, never `&mut Program`, so the compiler bars a view from
     re-entering the loop). **Scoped to the modal mechanism**; gray theming /
     `getData`-`setData` (D10) / `message()`-`query` veto deferred (no consumers
     yet — see the row-34 deferrals).
7. **Then the widget batches fan out hard** (PORT-ORDER Batches B–E): Phase-3
   leaves, validators, menus, dialogs, editor — the bulk `MECHANICAL` rows; run as
   parallel worktree implementer+reviewer trios, committing at batch boundaries.

The snapshot-test workflow (Appendix B step 4) is fully unlocked: build a view on
a `HeadlessBackend`, `render`, `assert_snapshot!` against the frozen format.

## Conventions
- English for all code/comments/identifiers (user-facing strings may be localized).
- Commit messages end with the project's Co-Authored-By trailer; commit/push only
  when asked.
