# Session handover — Batch B in progress; resume at the FOUNDATION rows (key helpers → TButton 37 / TLabel 41 → validator wave 35/39)

> Living handover for the **next** rstv session. Read this, then
> [CLAUDE.md](file:///home/oetiker/checkouts/rstv/CLAUDE.md) (orientation /
> Current state / Next step), then start. When the next stage lands, update or
> replace this file for the session after.

## Where things stand (git `main`)

| commit | what |
|--------|------|
| `b265a28` | Row 34 — `TDialog` + the modal `exec_view` lifecycle (Phase 2 done) |
| `3aaba20` | Demo + ModalFrame follow-the-drag fix |
| `8399d2b` | Pre-seed Theme roles for Batch B gating wave (StaticText + Cluster) |
| `45ae919` | **Batch B gating wave — TStaticText (36) + TCluster family (38/42/43/44)** |
| `8662624` | Pre-seed Theme roles + glyphs for TIndicator (45) |
| `249a56f` | **Batch B no-validator wave (part 1) — TIndicator (45) + TParamText (40)** |

**Build state:** 338 lib + 3 integration + 1 doctest green; `cargo clippy
--all-targets -- -D warnings` and `cargo fmt --check` clean. Working tree clean.

**Phase 2 is COMPLETE.** **Batch B is ~7 rows in:** 36, 38, 42, 43, 44, 45, 40
done (all two-stage reviewed SPEC-PASS + QUALITY-PASS). The **remaining Batch B
rows all have a FOUNDATION gap** and need careful per-row design (Opus / main
thread), not a clean MECHANICAL fan-out — see the seam analysis below.

## What landed this session (Batch B, rows 36/38/42/43/44/45/40)

- **TStaticText (36)** — `src/widgets/static_text.rs`. Faithful `tstatict.cpp`
  word-wrap draw (D13 via `crate::text::{scroll,next,width}`), `\x03`
  line-centering, `\n` breaks, `Role::StaticText` (D7), gfFixed, no events.
- **TCluster + TCheckBoxes/TRadioButtons/TMultiCheckBoxes (38/42/43/44)** —
  `src/widgets/cluster.rs`. **The seam that absorbed 42/43/44 into 38:** one
  `Cluster` engine + a closed **`ClusterKind`** enum (D1) carrying per-kind
  icon/marker/value semantics; three thin **D2 embed-delegate wrappers** via a
  module-local `cluster_wrapper!` macro. Faithful drawMultiBox/column/row/findSel/
  buttonState + the four arrow-nav loops; D7 lo/hi AttrPairs → `put_cstr` with
  `Cluster*` roles. Quality review caught **two real panic bugs** (fixed):
  `size_y==0` divide-by-zero, and shift-overflow at item 16 in the multi path —
  both now guarded to mirror `button_state`'s 32-item cap, with regression tests.
- **TIndicator (45)** — `src/widgets/indicator.rs`. Editor row:col strip; frame
  glyph + `IndicatorNormal/Dragging` roles, modified marker (☼), colon aligned to
  column 8 (`start_col = 7 − digits`, negative only past 8-digit rows — tested).
- **TParamText (40)** — in `src/widgets/static_text.rs`. `TStaticText` subclass
  with dynamic text; D2 embed-delegate; **printf → `format!`-at-call-site** (the
  256-byte cap dropped). Snapshot byte-identical to StaticText proves delegation.

### Theme roles already pre-seeded (orchestrator-owned, committed)
`src/theme.rs` now carries (beyond Phase 0–2): `StaticText`; the cluster palette
`ClusterNormal/Selected/NormalShortcut/SelectedShortcut/Disabled`; and
`IndicatorNormal/Dragging` + three `indicator_*` glyphs. **Provisional gray-dialog
colours** (realign with the deferred `TODO(row 34 gray theming)`).

## NEXT — the remaining Batch B rows (FOUNDATION; do per-row, full two-stage review)

These were deliberately **not** fanned out: each needs a substrate decision first.
Run them serially-ish with full per-row two-stage review (spec then quality, fresh
C++-adversarial agents — the row-34 lesson: a brief itself can be wrong). The
orchestrator pre-seeds the Theme roles/glyphs each row needs (avoids worktree
`theme.rs` conflicts), as done for the rows above.

### Prereq (do FIRST) — shared key helpers (task #6)
TButton, TLabel, TScrollBar, and TCluster **all** converge on the accelerator/key
helpers, which every one of them deferred. Build them **once**, before 37/41:
- `hotKey(label) -> Option<char>` — the char after `~` (uppercased), the shortcut.
- `getAltCode(c) -> Key` — `tvtext2.cpp:82`; maps a letter to its Alt-keycode.
- `ctrlToArrow(key) -> Key` — WordStar Ctrl-letter → arrow aliases (scrollbar's
  `TODO/NOTE(ctrlToArrow)`; cluster's literal-arrows-only deferral).
Likely a small `src/event/keys.rs` (or extend `src/event/key.rs`). Faithful ports
from `tvtext2.cpp` / `tkeys`/`syskeys`. With these, the cluster/scrollbar
accelerator + WordStar TODOs can also be retired opportunistically.

### Row 37 — TButton (FOUNDATION; task #7). **Resolve the timer-id payload first.**
`tbutton.cpp`: press animation via the row-20 `Clock`, the grab/release-default
broadcast protocol, the natural **first `exec_view` consumer** (an OK/Cancel
dialog round-trip).
- **BLOCKER to decide:** the timer-expiry broadcast **drops the `TimerId`**
  (`src/app/program.rs:654`, `TODO(timer payload)` — it broadcasts
  `cmTimerExpired` with `source: None`). C++ TButton checks
  `event.message.infoPtr == animationTimer` to know it's *its* timer. With the id
  gone, our button can only check `animationTimer != 0` → **it fires on ANY
  timer's expiry the moment a second timer source exists** (latent-but-real, not a
  rare race). Two resolution shapes — **pick before implementing**:
  (a) carry the `TimerId` in the expiry event, or (b) **target** the expiry to the
  owning `ViewId` (the truer `infoPtr` successor; routes like a directed message).
  **Recommendation: resolve it now** (TButton is the forcing function, it's small,
  and it keeps the animation faithful) rather than the defer-animation fallback
  (press immediately, no flash). This is the next session's call — state the
  tradeoff in the brief.
- Commands: `DEFAULT`/`COMMAND_SET_CHANGED`/`TIMER_EXPIRED`/`RECORD_HISTORY`
  already exist in `command.rs`. `cmGrabDefault`/`cmReleaseDefault` →
  **button-local consts** in `button.rs` (D1: view-specific consts live with the
  view — no `command.rs` edit).
- Pre-seed **8 cpButton roles** (`"\x0A\x0B\x0C\x0D\x0E\x0E\x0E\x0F"`):
  Button{Normal,Default,Selected,Disabled} text + {Normal,Default,Selected}Shortcut
  + ButtonShadow. getColor map: `0x0501`=(Normal,NormalShortcut),
  `0x0602`=(Default,DefaultShortcut), `0x0703`=(Selected,SelectedShortcut),
  `0x0404`=(Disabled,Disabled), `getColor(8)`=Shadow. Plus shadow glyphs
  (`shadows[3]` in `tvtext1.cpp`).
- Defer (faithful to scrollbar/cluster): the mouse `do{}while(mouseEvent(...,move))`
  press-tracking loop → row 31, D9 (single-shot fallback: down inside clickRect →
  press). `showMarkers` dropped. Accelerators use the new shared key helpers.
- `setState`: sfFocused → `makeDefault` (grab/release-default broadcast);
  sfSelected/sfActive → redraw (D8 no-op). `press()` → broadcast (bfBroadcast) or
  post `Event::Command` (the `infoPtr`/`this` → `Broadcast{source}` or command).

### Row 41 — TLabel (FOUNDATION-ish; task #8). focus-by-id + first `Broadcast{source}` consumer.
`tlabel.cpp`: a caption that **links** to a control and focuses it on click/hotkey,
and **highlights** while its linked control is focused.
- **link is an `Option<ViewId>` (D3)** — not a `TView*`. `TLabel` is the **first
  consumer of `Broadcast{source}`** (Phase A `7efecb3`): on
  `Broadcast{RECEIVED_FOCUS|RELEASED_FOCUS, source}` where `source == link_id` →
  `light = received`. Nice payoff of Phase A.
- **focusLink needs a focus-by-`ViewId` deferred tree-op.** Reuse the **row-33d-2
  shape** (`select_window_num`/`focus_by_number`): a `View` trait tree-op (default
  no-op) + container override that resolves the id and calls `focus_child`, routed
  through a **new `Deferred` variant** (loop-owned focus state). **`request_set_state(id,
  Focused, true)` is NOT a substitute** — it bypasses `current`/select/the active
  chain. This is the row's design crux.
- Embeds `StaticText` (D2; reuse/promote the delegate boilerplate — see the
  `cluster_wrapper!` / ParamText breadcrumb). Pre-seed **4 cpLabel roles**
  (`"\x07\x08\x09\x09"`): Label{Normal,Light} text + {Normal,Light}Shortcut;
  `0x0301`=(Normal,NormalShortcut) when not lit, `0x0402`=(Light,LightShortcut)
  when lit. Accelerators via the shared key helpers.

### Validator wave (task #5) — TValidator (35) → TInputLine (39)
- **TValidator (35, FOUNDATION):** the abstract `Validator` trait
  (`is_valid_input`/`is_valid`/`transfer` — D2 hook feeding D10). `tvalidat.cpp`/
  `svalid.cpp`.
- **TInputLine (39, FOUNDATION):** the first **data-bearing** control — builds the
  **D10 typed `value`/`set_value` protocol** (this is where the row-34/38 deferred
  `getData`/`setData`/`dataSize` finally lands; the dialog gather/scatter
  group-walk). Optional `Validator`; text selection; arrow glyphs (D7).
  `tinputli.cpp`/`sinputli.cpp`.
- This wave is independent of 37/41 (shares no state) and could run concurrently
  with them, but both halves here are FOUNDATION → main thread / Opus.

### After Batch B: `msgbox` (63) becomes buildable
Once `TButton` + `TStaticText` (+ `TInputLine`) exist, `messageBox`/`inputBox`
(`msgbox.cpp`) is the first consumer of the **D9 view-triggered async-modal path**
(result via a posted completion `Command`) — guide D9 "exec_view — corrected"
carries that design; build when a menu/msgbox needs it (Phase 4), not before.

## Row-34/38 deferrals that become buildable as controls land (unchanged)
- **`getData`/`setData`/`dataSize` (D10) — build at `TInputLine` (39).** The
  cluster recorded the breadcrumb (`TRadioButtons::setData` also sets `sel=value`).
- **Gray multi-scheme theming** — all the provisional `*` colours realign here
  (`TODO(row 34 gray theming)` in `window.rs`). Good first-thing when a dialog's
  look matters (color dialog, Batch E).
- **`message()`/`query` + `cmCanCloseForm` veto** — needs a validating control
  (`TInputLine` + a `TValidator`); guide D4 "message() — corrected" is the design.

## Still deferred (older, unchanged)
- **Timer-id payload (D4)** — `program.rs:654`; **TButton 37 forces this** (above).
- **`cmResize` keyboard resize sub-mode** (`window.rs`).
- **Scrollbar auto-repeat + thumb-drag** (`scrollbar.rs` `TODO(row 31, D9)`).
- **Cluster mouse drag-cursor loop** (`cluster.rs` `TODO(row 31, D9)`); cluster +
  scrollbar **ctrlToArrow / accelerator** TODOs (retire via the shared key helpers).
- **Close press-and-hold confirm** (`frame.rs`); **sibling tee-walk**
  (`framelin.cpp`); **shadow casting** (`group.rs`); **row-9 glyphs** per-widget.
- **View-/menu-triggered async modal** (`Deferred::OpenModal` + posted completion)
  → Phase 4. **Modal isolation (Phase 4, D9)** — `program.rs` `exec_view` doc.

## Process reminders
- **The fan-out cadence applies only to gap-free MECHANICAL leaves** (as the 7
  done rows were: parallel worktree implementers, `isolation: "worktree"`, Sonnet,
  orchestrator integrates the shared `mod.rs` + pre-seeds `theme.rs`). The
  remaining rows are FOUNDATION → **per-row, Opus, full two-stage review** (revert
  the combined-review shortcut used for the two trivial leaves).
- **Two-stage review stays mandatory** (spec → quality, fresh C++-adversarial
  agents against the **C++ + guide**, not the brief — the brief can be wrong).
  Make round-trip tests **discriminating + bite-checked**. The cluster quality
  pass caught two reachable panics the spec pass rated NITs — quality review earns
  its keep.
- **Commit completed rows before dispatching worktree agents that build on them**
  (worktree-gotcha: a worktree branches from the last *commit*). Pre-seed shared
  files (`theme.rs` roles/glyphs) in their own commit first.
- **Per-row brief is inline + self-contained** (the row + the C++ + the D-rules +
  the existing types it builds on + "run test/clippy/fmt + add a snapshot test").
  Keep briefs **tight** — an over-long brief crashed a Sonnet implementer's
  context this session (it still produced correct code; verify worktree output
  directly if an agent fails to report).
- **Snapshot workflow** (Appendix B step 4): `cargo-insta` is NOT installed →
  generate a new `.snap` with `INSTA_UPDATE=always cargo test <name>`, verify by
  hand, re-run plain, commit the `.snap`.

## Outstanding TODOs seeded in code (grep)
- `TODO(timer payload)` in `src/app/program.rs` — **TButton 37 forces this**.
- `TODO(row 31, D9)` in `src/widgets/scrollbar.rs` + `src/widgets/cluster.rs` —
  press-and-hold / drag-tracking loops.
- `TODO(row 41, accelerators)` / `TODO(ctrlToArrow)` in `src/widgets/cluster.rs` —
  retire via the shared key helpers (task #6).
- `TODO(row 34 gray theming)` in `src/window/window.rs`.
- `TODO(33d-2/later, D9)` in `src/window/window.rs` — cmResize keyboard sub-mode.
- `TODO(row 33, D9)` in `src/frame.rs`; `TODO(row 33)` in `src/view/group.rs`.
- Row 9 `Glyphs` continues to fill in per-widget.
