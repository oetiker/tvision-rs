# Session handover — row 34 (`TDialog`) COMPLETE; resume at Batch B (Phase 3 leaf widgets)

> Living handover for the **next** rstv session. Read this, then
> [CLAUDE.md](file:///home/oetiker/checkouts/rstv/CLAUDE.md) (orientation /
> Current state / Next step), then start. When the next stage lands, update or
> replace this file for the session after.

## Where things stand (git `main`)

| commit | what |
|--------|------|
| `15c601d` | Row 33d-2 — window selection (`TWindow` COMPLETE) |
| `7011a1c` | docs: row 33d-2 done; row 34 handover |
| `b265a28` | **Row 34 — `TDialog` + the modal `exec_view` lifecycle** |
| `233179e` | docs: row 34 done (Phase 2 complete); Batch B handover |
| `c7d3cf6` | docs: record the row-34 modal program-handling deviation (D9) |

**Build state:** 299 lib + 3 integration + 1 doctest green; `cargo clippy
--all-targets -- -D warnings` and `cargo fmt --check` clean. Working tree clean.

**Phase 2 is COMPLETE** (`TDeskTop` 30 → `TWindow` 33 → `TDialog` 34). The path
to "a window/dialog you can see and drive modally" is done. The next work is the
**Phase 3 leaf-widget fan-out** (PORT-ORDER Batch B) — the bulk `MECHANICAL`
rows, run as **parallel worktree implementer+reviewer trios**.

## What row 34 did (just landed — the modality payoff)

Brief: [`docs/briefs/row34-tdialog-modal.md`](briefs/row34-tdialog-modal.md).
Guide amended: **D9 "exec_view — corrected"** (the two modal-invocation paths).
New module `dialog` = `src/dialog/{mod.rs,dialog.rs}`.

- **`Dialog { window: Window }`** — the D2 embed-and-delegate exemplar one level
  deeper (`Dialog` is-a `Window` is-a `Group`). Delegates *all* of `View` to the
  window **except** `handle_event` + `valid`. Ctor (`tdialog.cpp`): `flags =
  wfMove|wfClose` (re-pushed to the frame via new `Window::set_flags` so no zoom
  icon), `growMode = 0`, `palette = Gray`, `wnNoNumber`.
- **`handle_event`** ports `TDialog::handleEvent`: `Window::handle_event` **first**,
  then Esc→post `cmCancel`, Enter→broadcast `cmDefault`, `cmOK/cmCancel/cmYes/cmNo`
  →`endModal` **iff `sfModal`** (else left live). **`valid`** = `cmCancel`→true else
  `Group::valid`.
- **`Program::exec_view(view) -> Command`** — the FOUNDATION crux. Ports
  `TGroup::execView` + `execute` as a **nested `while end_state.is_none() {
  pump_once() }`** loop (+ the outer `while !valid`). **Sound because a `View` holds
  only `&mut Context`, never `&mut Program`** — the compiler bars a view from
  re-entering the loop, so the sync loop only ever runs top-level (startup / app
  `main` / a test driving pre-queued events). Faithful: save/restore `current` +
  `command_set`, insert at root (faithful to `application->execView`), clear
  `ofSelectable`, set `sfModal` **directly** (C++ `setState` never propagates it),
  `set_current(Enter)`, push `ModalFrame` directly, run the loops, then
  `captures.pop()` + `remove` — **validation scoped to the modal's OWN `valid`,
  NOT the root group's** (the spec-review blocker: `tgroup.cpp:184/205` — the
  `while(!valid)` is virtual on `p`=the dialog; root-scoping ANDs the desktop
  sibling → latent hang).
- **`endModal` is downward (D3):** the dialog requests `ctx.end_modal(cmd)` →
  **`Deferred::EndModal`** → the pump sets `Program::end_state` (the `69897fe`
  "new capability adds a `Deferred` variant" rule). **`CaptureStack::pop`** added
  (the loop owns the stack, so `exec_view` — not the handler — does the
  `valid(end_state)`-conditional pop).

Two-stage reviewed: SPEC-FAIL → fixed the validation-scope **blocker** (+ a
bite-verified discriminating sibling-veto test + a root-insert deviation
breadcrumb), then QUALITY-PASS (one `find_mut`-consolidation nit applied).

## NEXT — Batch B: the Phase 3 leaf-widget fan-out begins

This is where the port **stops being serial FOUNDATION and fans out**. PORT-ORDER
**Batch B** (lines ~197–202). Run as **parallel worktree implementer+reviewer
trios** (`isolation: "worktree"`), Sonnet for `MECHANICAL`, committing at batch
boundaries. **Commit completed rows before dispatching worktree agents that build
on them** (the worktree-gotcha: a worktree branches from the last *commit*).

The dependency-ordered waves:

1. **`TCluster` (38, FOUNDATION-ish) + `TStaticText` (36, MECHANICAL)** first —
   they gate the rest of the wave. `TCluster` is the base for checkboxes/radio;
   `TStaticText` is the base for `TParamText`/`TLabel`. Build these (and review)
   **before** fanning out their dependents.
2. **No-validator wave (parallel once 36/38 land):** `TButton` (37, press
   animation via the row-20 `Clock`; broadcast/command flags), `TIndicator` (45),
   `TParamText`/`TLabel` (40/41, need 36), `TCheckBoxes`/`TRadioButtons`/
   `TMultiCheckBoxes` (42/43/44, need `TCluster` 38).
3. **Validator wave:** `TValidator` trait (35, the abstract base — D2 `transfer`
   hook feeds D10) → `TInputLine` (39, FOUNDATION: typed `value`/`set_value`).

**`TButton` is the natural first exec_view *consumer*:** a dialog with an OK/Cancel
button posting `cmOK`/`cmCancel` is the first realistic `exec_view` round-trip with
real controls. **`msgbox` (63)** (`messageBox`/`inputBox`) becomes buildable once
`TButton` + `TStaticText` exist — it is also the first consumer of the **D9
view-triggered async-modal path** (result via a posted completion `Command`); the
guide's D9 "exec_view — corrected" carries that design (built at Phase 4 / when a
menu or msgbox needs it, NOT now).

## Row-34 deferrals that become buildable as controls land (don't forget these)
- **`getData`/`setData`/`dataSize` (D10) — build at `TInputLine` (39)**, the first
  data-bearing control. The typed `value`/`set_value` protocol + the dialog
  gather/scatter group-walk. No stub exists today (correctly).
- **Gray multi-scheme theming** — `Dialog` records `palette = Gray` but the frame
  still renders the blue `Frame*` roles. Mapping `Gray`/`Cyan` → distinct `Theme`
  roles (push palette to the frame + branch role selection + new `Theme` entries)
  is a standalone cosmetic chunk. `TODO(row 34 gray theming)` in `window.rs`
  (`set_palette`) + the `dialog` module doc. Good first-thing when a dialog's look
  matters (e.g. the color dialog, Batch E).
- **The return-consuming `message()`/`query` tree-owner primitive + `cmCanCloseForm`
  veto** — `Dialog::valid` uses only `Group::valid` today. The veto needs a
  *validating control* (`TInputLine` + a `TValidator`); build the primitive at its
  first real consumer (guide D4 "message() — corrected" is the design of record,
  "designed but not built").

## Still deferred (older, unchanged by row 34)
- **`cmResize` keyboard resize sub-mode** (`window.rs` `TODO(33d-2/later, D9)`) —
  enable in `setState` only when a menu can trigger `cmResize`.
- **Scrollbar auto-repeat + thumb-drag** (`scrollbar.rs` `TODO(row 31, D9)`) →
  Batch B widget pass (good to fold in while touching widgets).
- **Close press-and-hold release-confirm** (`frame.rs` `TODO(row 33, D9)`).
- **Sibling tee-walk** (`framelin.cpp` `FrameMask`), **shadow casting**
  (`group.rs` `TODO(row 33)`), **row-9 glyphs** continue per-widget.
- **View-/menu-triggered async modal** (`Deferred::OpenModal` + posted completion)
  → Phase 4 (no menu/button exists yet); guide D9 carries the design.
- **Modal isolation (Phase 4, D9 deviation).** Our single loop runs
  `program_handle_event` (Alt-N + the cmQuit→`end_state` catch) during the modal
  pumps too — UNLIKE C++ `execView`→`p->execute()`→the dialog's `handleEvent`
  (program-level handling out of the modal path). Today that means cmQuit ends a
  modal here (C++ discards it at the dialog — a deliberate, documented deviation,
  see `program.rs` `exec_view` doc + the `..._deviation_from_cpp` test) and Alt-N
  could reach the desktop under a modal. `TODO(Phase 4: modal isolation)` in
  `program_handle_event`: suppress program-level command interception while a
  modal is active, once menus + windows + modals coexist. (Also: `exec_view`'s
  command-set restore omits the C++ `cmCommandSetChanged` re-broadcast — moot, no
  observer; align when one exists.)

## Process reminders
- **The fan-out changes the cadence:** Phase 0–2 were serial main-thread
  FOUNDATION. Batch B is **parallel `MECHANICAL` leaves** → dispatch
  implementer+reviewer trios in **worktrees** (`isolation: "worktree"`), Sonnet for
  `MECHANICAL`, Opus for the FOUNDATION rows (`TCluster` 38, `TValidator` 35,
  `TInputLine` 39). The orchestrator owns shared-file edits (`lib.rs`,
  `widgets/mod.rs` re-exports) to avoid races. Commit at batch boundaries.
- **Per-row brief is still inline + self-contained** (the PORT-ORDER row + the C++
  + the D-rules from Appendix B + the existing types it builds on + "run
  test/clippy/fmt + add a snapshot test"). Never "go read the plan."
- **Two-stage review stays mandatory** (spec → quality, fresh adversarial agents
  against the **C++ + guide**, not just the brief — row 34 proved the brief itself
  can be wrong: a brief error put the modal validation at the wrong scope, and only
  a C++-adversarial spec reviewer caught it). **Make round-trip tests
  discriminating + bite-checked.**
- **Snapshot-test workflow** (Appendix B step 4) is fully unlocked: build a widget
  on a `HeadlessBackend`, `render`, `assert_snapshot!` against the frozen format.
  NB cargo-insta is **not installed** in this env — `insta::assert_snapshot!`
  compares against the committed `.snap`; for a new snapshot, run with
  `INSTA_UPDATE=always cargo test <name>` to generate, then review the `.snap` by
  hand (row 34 hand-wrote one and verified it regenerates identically).

## Outstanding TODOs seeded in code (grep)
- `TODO(row 34 gray theming)` in `src/window/window.rs` — gray/cyan scheme roles.
- `TODO(33d-2/later, D9)` in `src/window/window.rs` — cmResize keyboard sub-mode.
- `TODO(row 31, D9)` in `src/widgets/scrollbar.rs` — auto-repeat + thumb-drag.
- `TODO(row 33, D9)` in `src/frame.rs` — close press-and-hold confirm.
- `TODO(row 33)` in `src/view/group.rs` — shadow casting in `Group::draw`.
- Row 9 `Glyphs` continues to fill in per-widget.
