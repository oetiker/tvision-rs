# Session handover — Batch B; TLabel (41) DONE. Resume at validator wave (TValidator 35 → TInputLine 39)

> Living handover for the **next** rstv session. Read this, then
> [CLAUDE.md](file:///home/oetiker/checkouts/rstv/CLAUDE.md) (orientation /
> Current state / Next step), then start. When the next stage lands, update or
> replace this file for the session after.

## Where things stand (git `main`)

| commit | what |
|--------|------|
| `249a56f` | Batch B no-validator wave (part 1) — TIndicator (45) + TParamText (40) |
| `fa3c767` | docs: Batch B progress + FOUNDATION seam analysis |
| `b53c618` | **Shared key helpers (task #6)** — `hot_key`/`ctrl_to_arrow`/`is_alt_hotkey`/`is_plain_hotkey` |
| `9c5fda7` | **`Event::Timer(TimerId)`** — typed timer-expiry; resolves the timer-id payload gap |
| `fbb0de3` | Pre-seed Theme roles + shadow glyphs for TButton (37) |
| `71fc6c3` | docs: D7 — `Role` closed-enum is a port-phase default, not a hard rule |
| `53d011e` | **`View::grabs_focus_on_click`** — per-view opt-out for the mouse-down auto-select (restores `bfGrabFocus`) |
| `6d763dc` | **TButton (37)** — clickable command button + animation |
| `6b890a1` | Pre-seed 4 cpLabel Theme roles for TLabel (41) |
| `3483760` | **TLabel (41)** + focus-by-ViewId deferred tree-op seam |

**Build state:** 408 lib + 3 integration + 2 doctests green; `cargo clippy
--all-targets -- -D warnings` and `cargo fmt --check` clean. Working tree clean.

**Phase 2 is COMPLETE.** **Batch B:** 36, 38, 42, 43, 44, 45, 40, **37**, **41**
done (all two-stage reviewed SPEC-PASS + QUALITY-PASS). The **remaining Batch B
rows still have a FOUNDATION gap** and need careful per-row design (Opus / main
thread), not a clean MECHANICAL fan-out — see the seam analysis below. **NEXT =
the validator wave (TValidator 35 → TInputLine 39).**

## What landed THIS session (TLabel 41 + focus-by-ViewId seam)
- **Pre-seed 4 cpLabel roles (`6b890a1`)** — `LabelNormal/LabelLight` +
  `LabelNormalShortcut/LabelLightShortcut` in `theme.rs` (provisional gray-dialog
  colours; `cpLabel "\x07\x08\x09\x09"` → dialog idx 7/8/9; both shortcut indices
  map to entry 9, so the two `*Shortcut` roles share a value but stay distinct).
- **TLabel (41) + the FOUNDATION seam (`3483760`)** — the *focus-by-`ViewId`
  deferred tree-op*, mirroring the `remove_descendant` family:
  `Deferred::FocusById(ViewId)` + `Context::request_focus` (context.rs);
  `View::focus_descendant` default no-op (view.rs); `Group::focus_descendant`
  (ofSelectable gate at the owning group → `focus_child` == C++ `select()`;
  recurse otherwise; return `true` on any match to stop the walk); Window/Desktop/
  Dialog/ParamText/Label delegate, leaves don't; pump applies on the deferred-drain
  branch (program.rs). The `Label` widget (static_text.rs, D2 embed of
  `StaticText`): single-row draw (fill + `put_cstr` at col 1), `ofPreProcess|
  ofPostProcess` (so a non-selectable label gets its Alt-hotkey via the group's
  pre/post phases), `focusLink → request_focus` (clearEvent **unconditional**).
  **First consumer of `Broadcast{source}`** (Phase A): `light` tracks the link's
  `RECEIVED/RELEASED_FOCUS` gated on `source == link`.
  **Deferred/breadcrumbed:** plain-letter postProcess accelerator (needs the
  `Context` phase signal — same as TButton); owner-chain `focus()` walk (flat
  modal dialogs only); `showMarkers`. **Known substrate limitation** (documented
  in the Label type-doc): a stale `light` survives if a *selectable link is
  removed at runtime* — `Group::remove` is release-after-remove (row-26 ordering),
  so no `RELEASED_FOCUS{source==link}` fires; C++ `tgroup.cpp` does `hide()`
  before `removeView`. Revisit if a consumer ever removes a bare link.

## What landed the PRIOR session (key helpers + Event::Timer + grabs_focus_on_click + TButton 37)
- **Shared key helpers (`b53c618`)** — in `src/event/key.rs` (re-exported via
  `event/mod.rs` + `lib.rs`). Decomposed-model adaptations: `getAltCode`'s
  combined-scancode return has no meaning, so the accelerator idiom becomes the
  predicates `is_alt_hotkey` (any alt+char, case-fold, only `alt` required) +
  `is_plain_hotkey` (`!alt && !ctrl` — C++ `charScan.charCode` is the control
  code under those mods). `ctrl_to_arrow` = WordStar Ctrl-letter → bare arrow.
  Spec review caught a real BLOCKER (plain-hotkey false-matching Ctrl+letter).
- **`Event::Timer(TimerId)` (`9c5fda7`)** — the typed successor to `evBroadcast
  cmTimerExpired`+`infoPtr==TTimerId`. Broadcast-class routed (group's
  all-children arm; `deliver`'s gates already pass it). Pump emits one
  `Event::Timer(id)` per expired id. `Command::TIMER_EXPIRED` removed. **First
  consumer = TButton's animation.** (Per Phase-A precedent: integer infoPtr
  payloads get their own typed mechanism, not `Broadcast{source}`.)
- **`View::grabs_focus_on_click` (`53d011e`)** — the group's carryover #1
  (relocated `TView::handleEvent` mouse-down auto-select) was unconditional;
  C++ lets each view opt out (TButton selects only with `bfGrabFocus`). New
  defaulted trait method (true); Button overrides → its `grab_focus` flag; the
  group consults it before `focus_child`. Default-true = zero change to all
  existing views. **Goes live at TLabel/TInputLine** (clicking OK must not steal
  focus from an input line). This was an advisor-flagged group divergence, fixed
  as substrate per [[fix-foundations-not-bandaids]], not a button footnote.
- **TButton (37) (`6d763dc`)** — `src/widgets/button.rs`. Full draw (face +
  ▄█▀ drop shadow + `~`-title), single-shot mouse press, Alt-hotkey/Space →
  one-shot animation timer firing on `Event::Timer`, the grab/release-default
  dance (`makeDefault` inverted guard), `set_state` base-replication + makeDefault.
  **Three TButton deferrals → become the next FOUNDATION primitives:**
  (1) **command-enabled graying** — needs a `Context` command-set query (ctor
  `sfDisabled`-from-command + `cmCommandSetChanged` handler dropped; correctness
  preserved by the program's boundary command filter). **Menus will force this.**
  (2) **plain-letter postProcess accelerator** — needs a *phase signal* on
  `Context` (only Alt-hotkey + focused-Space honored; `is_plain_hotkey`
  intentionally unused — ungated it would steal plain letters from a focused
  input line). **TLabel/cluster accelerators also want this.**
  (3) mouse hold-tracking + pressed-flash → row 31, D9 (single-shot for now).

## What landed an earlier session (Batch B, rows 36/38/42/43/44/45/40)

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

### ✅ DONE — shared key helpers (task #6, `b53c618`) and TButton (37, `6d763dc`)
Both shipped the prior session (see "What landed the PRIOR session" above). The key helpers
landed as **predicates** (`is_alt_hotkey`/`is_plain_hotkey`), not the literal
`getAltCode`, because of the decomposed key model. The TButton timer-id blocker was
resolved with **`Event::Timer(TimerId)`** (option (a) — carry the id; option (b)
target-by-ViewId was rejected: it can't tell two timers of one view apart and
needs unbuilt directed-routing). **TButton's two deferrals are the next two
FOUNDATION primitives** (a `Context` command-set query for graying; a phase signal
for the plain-letter accelerator) — see the rows below + the deferral notes above.
The cluster/scrollbar `ctrlToArrow`/accelerator TODOs can now be retired
opportunistically using the shared helpers.

### ✅ DONE — TLabel (41, `3483760`). focus-by-id seam + first `Broadcast{source}` consumer
Shipped this session (see "What landed THIS session" above). The focus-by-`ViewId`
deferred tree-op (`Deferred::FocusById` + `View::focus_descendant`, mirroring
`remove_descendant`) is now substrate the validator wave can reuse if a control
ever needs to focus another by id. `Label` is the D2 embed-of-`StaticText`
template, alongside `ParamText` in `static_text.rs`. The promised
`request_set_state(id, Focused, true)` non-substitute warning held — `focus_child`
(== C++ `select()`) is what `focus_descendant` calls.

### Validator wave (task #5) — TValidator (35) → TInputLine (39). **← START HERE**
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

## Still deferred (older + new this session)
- ✅ **Timer-id payload (D4) — RESOLVED** (`9c5fda7`, `Event::Timer(TimerId)`).
- **NEW — `Context` command-set query** (for command-enabled *graying*): TButton
  deferred its ctor `sfDisabled`-from-command + `cmCommandSetChanged` handler
  because `Context` carries no read of `curCommandSet`. Correctness is preserved
  (the program filters disabled `Event::Command` at its boundary); only the visual
  gray-out is missing. **Menus (Batch C) force this** — add a read-only command-set
  accessor to `Context` then (the deferred-effects refactor stabilized
  `Context::new` for *effects*; a read accessor is a separate, additive concern).
- **phase signal on `Context`** (for the plain-letter postProcess accelerator):
  TButton honors only Alt-hotkey + focused-Space; **TLabel now also defers** the
  same plain-letter branch (`static_text.rs` `TODO(label/button: plain-hotkey
  postProcess accelerator …)`). The C++ `owner->phase == phPostProcess`
  plain-letter branch needs the dispatch phase exposed to the view. The cluster's
  deferred accelerators want the same. Add when the first widget genuinely needs
  plain-letter accelerators (3 consumers now waiting: button, label, cluster).
- **NEW — `Group::remove` release-after-remove ordering** (row-26 substrate): a
  removed selectable child never gets `set_state(Focused,false)` before removal, so
  no `RELEASED_FOCUS{source}` fires. C++ `tgroup.cpp` does `hide()` **before**
  `removeView`. Today's only observable consequence: a `TLabel` whose **selectable
  link is removed at runtime** keeps a stale `light` (documented in the Label
  type-doc). No current consumer removes a bare link, so deferred; fix the ordering
  if/when one does (or when validating the focus/destroy path generally).
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
- ✅ `TODO(timer payload)` — RESOLVED (removed; now `Event::Timer`).
- `TODO(button: command-enabled graying ...)` + `TODO(button/cluster: plain-hotkey
  postProcess accelerator ...)` in `src/widgets/button.rs` + `TODO(label/button:
  plain-hotkey postProcess accelerator ...)` in `src/widgets/static_text.rs` — the
  two new `Context` primitives (see "Still deferred").
- `TODO(row 31, D9)` in `src/widgets/scrollbar.rs` + `src/widgets/cluster.rs` +
  `src/widgets/button.rs` — press-and-hold / drag-tracking loops.
- `TODO(row 41, accelerators)` / `TODO(ctrlToArrow)` in `src/widgets/cluster.rs` —
  shared key helpers now EXIST (`b53c618`); retire opportunistically.
- `TODO(row 34 gray theming)` in `src/window/window.rs`.
- `TODO(33d-2/later, D9)` in `src/window/window.rs` — cmResize keyboard sub-mode.
- `TODO(row 33, D9)` in `src/frame.rs`; `TODO(row 33)` in `src/view/group.rs`.
- Row 9 `Glyphs` continues to fill in per-widget.
