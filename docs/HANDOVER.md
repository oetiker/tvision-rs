# rstv — session handover (forward-looking)

> What the **next** session needs: current state, what's next, and the
> non-obvious gotchas. The per-session implementation narrative + the git-commit
> changelog live in
> [`docs/IMPLEMENTATION-LOG.md`](file:///home/oetiker/checkouts/rstv/docs/IMPLEMENTATION-LOG.md).
> Read this, then [CLAUDE.md](file:///home/oetiker/checkouts/rstv/CLAUDE.md)
> (orientation / locked decisions / cross-cutting seams), then start.
>
> **Direction = [`docs/PORT-ORDER.md`](file:///home/oetiker/checkouts/rstv/docs/PORT-ORDER.md)** —
> dependency-ordered; the **lowest-numbered incomplete row is the work** (✅ marks
> done rows). "Parallelizable batches" are an efficiency, not a competing
> direction. When a stage lands: add a section to the IMPLEMENTATION-LOG, tick the
> PORT-ORDER row, and update this file's *Current state* / *Next*.

## Current state

- **HEAD `352c949`.** Build: **698 lib tests** green; `cargo clippy --workspace
  --all-targets -- -D warnings` and `cargo fmt --all --check` clean (verify clippy
  with a forced re-lint — a cached run can mask a fresh warning).
- **Cargo workspace** (`tvision` + `tvision-macros`) — use `--workspace` for
  test/clippy/fmt. Artifacts land in
  `CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target` (export it). `cargo build
  --example hello` builds the drivable demo app.
- **Done:** Phase 0 (primitives + INFRA), Phase 1 (`TView`/`TGroup`/`TFrame`/
  `TProgram`/`TApplication`), Phase 2 (`TDeskTop`/`TWindow`/`TDialog`), Batch B
  Phase-3 leaves, `TScroller`/`TListViewer`/`TListBox` (27/28/48), the whole
  menu + status-line stack (46/49/50/51/52/47/53) **wired into `Program`**
  (`examples/hello.rs` is a drivable TV app), `Desktop::tile`/`cascade` +
  `cmTile`/`cmCascade`, the history cluster (54–57), **Phase 5 Batch C
  validators 58–62** + a new **`RegexValidator`** extension, the **general
  initial-modal-currency seam** (`View::reset_current`), and **`messageBox`/
  `messageBoxRect` (row 63 PART 1)**. The `#[delegate]` proc-macro is landed and
  adopted codebase-wide.

## Next — lowest-numbered remaining work

**Row 63 PART 2 (`inputBox`) + the validator-`error()` wiring** are the natural
next work, then `64`. The two items below were the previously-listed FOUNDATION
seams; **item 2 (initial-modal-currency) is DONE this session** — see *Done*. The
remaining open seam is item 1.

1. **The `ModalFrame` deliver-outside-to-modal seam** (row 56/57 deferred — STILL
   OPEN). Un-defers the `HistoryWindow` outside-click `endModal(cmCancel)`. **NOT a
   `ModalFrame` return-value tweak:** `ModalFrame::handle` has no `group`, and
   `program_handle_event` routes outside positional events **positionally to the
   desktop**. The fix is a **delivery-path change in `program_handle_event`**:
   while a `ModalFrame` is the top capture, deliver positional events to the modal
   view by id (makeLocal to its bounds) so the modal's own routing + the
   `HistoryWindow` `mouseInView`-cancel override decide. Verify a plain `Dialog`
   still IGNORES an outside click under that delivery (C++ does). Breadcrumb in
   place: `HistoryWindow::handle_event` `TODO(row 57 modal-loop seam)`.

2. **`inputBox`/`inputBoxRect` (row 63 PART 2) — needs a net-new FOUNDATION seam,
   NOT mechanical.** `dialog->setData(s)` / `getData(s)` is the **D10 dialog-level
   group-walk gather/scatter**, which does NOT exist yet (`Dialog` has no
   `value`/`set_value` — CLAUDE.md deferred it "to its first consumer"; inputBox
   IS that consumer). Two ways: (a) build the general gather/scatter (walk the
   dialog's selectable children in insertion order, scatter a `&[FieldValue]` /
   gather into one), or (b) shortcut for the single-input-line case (find the lone
   `InputLine` by id and read/`set_value` it directly, like `HistoryPick` does with
   its link). For a **top-level** `Program::input_box` the cleanest is to give
   `exec_view` a way to hand back data before the modal is dropped (the dialog is
   consumed inside `exec_view`); the `ModalCompletion`/async path is only needed for
   a view-triggered `inputBox`, which has no consumer yet. The sync `message_box`
   wrappers + the `initial_focus` seam built this session are the template.

3. **Validator `error()` → `messageBox` wiring (its own seam).** All five
   validators' `error()` are `TODO(row 63)` no-op breadcrumbs preserving the exact
   C++ strings. Wiring them is **blocked on `Validator::error(&self)` having no
   `Context`** — it cannot reach a deferred channel to request a modal. Needs a
   trait-signature change (thread a `&mut Context` / a deferred handle through
   `error()` and its `InputLine::valid` caller), then a `Deferred::OpenMessageBox`
   variant + the row-57 `pending_modal` async path (the `OpenHistory` precedent).
   The sync `Program::message_box` exists now; this is the *async-from-a-view* face.

Then Phase 5 continues in PORT-ORDER: **64** (`TStringList`/`TStrListMaker`,
minimal D12-adjacent port), **66** `TEditor` (FOUNDATION — gap-buffer editor),
then the std-dialog / file / color / outline families. `cmDosShell` is still
deferred — needs a backend terminal-suspend seam + SIGTSTP.

## What this session left available / changed

- **The `transfer`/D10 hook is live** (`Validator::transfer_get`/`transfer_set`,
  default `None`; `RangeValidator` is the first overrider, gated on
  `transfer_enabled`). `InputLine::value`/`set_value` consult it before the text
  fallback. A future typed-value validator just overrides those two methods.
- **`RegexValidator` is an extension *beyond* the C++ port** (not a tvision
  class) — when next editing `docs/PORTING-GUIDE.md`, add a short note that it
  exists as an rstv-original (the picture-mask DSL `PXPictureValidator` is the
  faithful port; `RegexValidator` is the modern alternative living alongside it).
- **`PXPictureValidator::error` deviation watch:** `is_valid` does not replicate
  the C++ 256-byte stack buffer (Vec grows); documented, not a divergence in
  practice (inputs are maxLen-bounded).

## Non-obvious gotchas (read before starting)

- **Worktrees** live under `/scratch/oetiker/claude-worktrees/<project>-<name>`.
  A `WorktreeCreate` hook redirects `isolation:"worktree"` there, **but only
  activates on a session restart** — until then, create the worktree manually at
  the `/scratch` path + dispatch a non-isolated subagent. Parallel worktree agents
  **share one cargo target dir** — their clippy/build "clean" is unreliable;
  re-verify on the integrated tree.
- **Commit completed rows before dispatching worktree subagents that build on
  them** (a worktree branches from the last commit; uncommitted work is absent).
- Verification is **snapshot tests** (D11, `insta`) for anything that draws;
  validators/data render nothing → unit tests only. `cargo-insta` is **not
  installed** — generate `.snap`s via `INSTA_UPDATE=always`, hand-verify, commit.

## Standing deferrals (still open — grep the TODOs)

- **idle→`statusLine->update()` help-ctx refresh** — inert under a single `All`
  `StatusDef`; only worth doing when a context-split `OneOf` line lands (needs a
  `View::get_help_ctx` + a `TopView` resolver).
- **status-line press-and-hold drag-highlight** (`drawSelect(Some)` hover) —
  `TODO(row 31, D9)`.
- **`program_handle_event` modal-isolation** breadcrumb (suppress program-level
  interception while a `MenuSession`/modal is active); the `ModalFrame`/
  `DragCapture` "(0,0)-desktop absolute-coords" caveat (the bar shifts the desktop
  down by 1 — re-examine when a dialog must position relative to the desktop).
- **`max_len` clamp on `InputLine::set_value`** — C++ flowback is
  `strnzcpy(data, s, maxLen+1)`; we assign unclamped (row-39 gap).

## Standing process reminders

- **Subagent-driven** (CLAUDE.md "How to run the port"): per row → fresh
  implementer (Sonnet for MECHANICAL, Opus for FOUNDATION) → **two-stage review**
  (fresh SPEC then QUALITY agents — do NOT self-review in the main thread) → fix →
  integrate → commit. Briefs are **self-contained** (inline the C++ + D-rules +
  existing types), never "go read the plan."
- **`git diff` the whole tree** after an implementer before integrating —
  implementers do out-of-scope refactors scoped reviewers miss.
- When you add a `View` trait method, add a matching forwarder to
  `tvision-macros/src/specs.rs` (the `delegate_view` spy test catches a forgotten
  forwarder for existing methods, but a brand-new defaulted method silently
  won't forward). **Validator-trait methods are NOT `View` methods** — no
  forwarder (e.g. `transfer_get`/`transfer_set`).
