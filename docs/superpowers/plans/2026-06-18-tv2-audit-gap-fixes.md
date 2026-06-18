# TV2 Audit Gap Fixes — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the four real code gaps the TV2-guide coverage audit surfaced, plus reconcile the audit docs to current state.

**Architecture:** Faithful port of magiblot/tvision C++ behavior, applying the project D-rules. Each fix was verified against the C++ source by a read-only investigation pass; the exact recipes are inlined per task. Three fixes are mechanical, one (TMenuView help context) is moderate and touches the status-line/capture seam.

**Tech Stack:** Rust (Cargo workspace `tvision-rs` + `tvision-rs-macros`); snapshot tests via `insta` on `HeadlessBackend`.

## Global Constraints

- `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target` before any cargo command (artifacts land there, NOT `./target`).
- Limit compiler/test parallelism to **4 cores** (`--jobs 4`, `--test-threads=4`).
- Every task must pass: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`.
- English for all code/comments/identifiers.
- Commit messages end with: `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`
- Each code fix also updates its `docs/audit/reference/<Class>.md` row to reflect the **current** state (PORTED/OK) — state only the fixed reality, **no history markers** (no "was SUSPECT", "fixed in", "RESOLVED"). The audit reads as a clean current-state reference.
- Do NOT flip `MenuSession::is_modal_gate()` (Task 3) — use the dedicated capture-handler walk to avoid perturbing global modal semantics.
- Source of truth (C++): `/home/oetiker/scratch/tvision-spec/magiblot-tvision/` (`include/tvision/`, `source/tvision/`).

---

### Task 1: TListViewer — setState `sfVisible` arm + handleEvent comment

**Files:**
- Modify: `src/widgets/list_viewer.rs` (~line 461 guard; ~line 475 doc comment)
- Test: add a snapshot/unit test in the same file's `#[cfg(test)]` module
- Modify: `docs/audit/reference/TListViewer.md` (the `setState` and `handleEvent` rows)

**Interfaces:**
- Consumes: `StateFlag::Visible`, the existing deferred `ctx.request_set_visible(...)` seam.
- Produces: nothing new (behavioral fix only).

**Context:** C++ `TListViewer::setState` (`tlstview.cpp:374-395`) runs its scrollbar show/hide arm when ANY of `sfSelected | sfActive | sfVisible` changes; inside, it shows both bars iff `getState(sfActive) && getState(sfVisible)`. The Rust `set_state` (`list_viewer.rs:443-471`) guards only on `Active || Selected` — the `Visible` arm is missing, so hiding a list viewer via `sfVisible` alone leaves its scrollbars visible. The inner logic already computes `active && visible` correctly and routes through the deferred `request_set_visible` seam; only the guard is wrong.

Separately, C++ `TListViewer::handleEvent` calls `TView::handleEvent` first, which only does mouse-down auto-select; tvision-rs deliberately relocated that to `Group::route_event` (`group.rs:1204-1224`). No behavior is missing — this is a comment-only clarification so it is not re-flagged.

- [ ] **Step 1: Write the failing test** — build a `ListViewer`-backed view with scroll bars, set it active+visible (bars shown), then `set_state(StateFlag::Visible, false, ctx)` and assert both scroll bars receive a `Deferred::SetVisible(false)` request (and become hidden). Follow the existing test style around `list_viewer.rs:1137-1150` (`make_ctx`, inspect the `deferred` Vec).
- [ ] **Step 2: Run it, confirm it fails** (`CARGO_TARGET_DIR=… cargo test --workspace --jobs 4 list_viewer -- --test-threads=4`) — the bars are NOT hidden because the `Visible` arm never runs.
- [ ] **Step 3: Fix the guard** — at `list_viewer.rs:461` change
  `if flag == StateFlag::Active || flag == StateFlag::Selected {`
  to
  `if flag == StateFlag::Active || flag == StateFlag::Selected || flag == StateFlag::Visible {`
  Leave lines 463-469 unchanged (they already compute `active && visible` and defer correctly).
- [ ] **Step 4: Add the handleEvent comment** — just above `pub fn handle_event<…>(…)` (~line 475), add a doc comment explaining that the C++ `TView::handleEvent` base call is intentionally omitted: it only performs mouse-down auto-select, which `Group::route_event` (group.rs) now owns, and `TView::handleEvent` is a no-op for all other event classes — so there is no base behavior to inherit here.
- [ ] **Step 5: Run the test, confirm it passes.**
- [ ] **Step 6: Update `docs/audit/reference/TListViewer.md`** — the `setState` row: Corr `OK`, note states the arm covers Active/Selected/Visible matching C++. The `handleEvent` row: Corr `OK`, note states the base auto-select is relocated to `Group` (documented in source). Update the section Summary counts (SUSPECT count drops by the relevant amount). No history phrasing.
- [ ] **Step 7: Full gate** — `cargo test --workspace --jobs 4`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`. Then commit.

---

### Task 2: TSortedListBox — `set_value_ctx` (dialog scatter)

**Files:**
- Modify: `src/widgets/list_box.rs` (add `set_value_ctx` to `impl View for SortedListBox`, after `value()` ~line 354)
- Test: add a round-trip test in the file's `#[cfg(test)]` module
- Modify: `docs/audit/reference/TSortedListBox.md` (the inherited getData/setData row + Summary)

**Interfaces:**
- Consumes: `FieldValue::Int`, `list_viewer::focus_item_num(self, idx, ctx)` (generic over `ListViewer`, already used by `ListBox::set_value_ctx` at `list_box.rs:164`).
- Produces: scatter symmetry with `SortedListBox::value()`.

**Context:** C++ `TSortedListBox` inherits `TListBox::setData`, which calls `focusItem(p->selection)` (`tlistbox.cpp:76-82`). Rust `SortedListBox` implements `value()` (gather, `list_box.rs:352`) but not `set_value_ctx`, so the `View` default no-op runs (`view.rs:728-731`) and a dialog scatter silently fails to focus the item. `ListBox` does it right at `list_box.rs:164` via `focus_item_num`. The value is an index into the (sorted) Vec, which is exactly what `value()` returns, so scatter is symmetric.

- [ ] **Step 1: Write the failing test** — `sorted_lb_set_value_ctx_focuses_the_item`: build a `SortedListBox`, `new_list(vec!["alpha","beta","charlie"], ctx)`, assert `value() == Some(FieldValue::Int(0))`, call `set_value_ctx(FieldValue::Int(2), ctx)`, assert `value() == Some(FieldValue::Int(2))` and that a scroll-bar param deferral was queued. Mirror the existing `make_ctx`/`deferred` test harness in this file.
- [ ] **Step 2: Run it, confirm it fails** — focus stays at 0 (no-op default).
- [ ] **Step 3: Add the method** to `impl View for SortedListBox` after `value()`:
  ```rust
  /// Set the focused item and republish the vertical bar. Mirrors [`ListBox::set_value_ctx`].
  /// The value is an index into the (sorted) item Vec — symmetric with `value()`.
  fn set_value_ctx(&mut self, v: FieldValue, ctx: &mut Context) {
      if let FieldValue::Int(idx) = v {
          list_viewer::focus_item_num(self, idx, ctx);
      }
  }
  ```
- [ ] **Step 4: Run the test, confirm it passes.**
- [ ] **Step 5: Update `docs/audit/reference/TSortedListBox.md`** — the inherited `getData`/`setData` row: Corr `OK`, bucket reflects scatter is now implemented mirroring `ListBox` (focuses `p->selection`). Update Summary (SUSPECT 1→0; the "Most important finding" line restated as the current correct behavior, no history).
- [ ] **Step 6: Full gate** (test/clippy/fmt), then commit.

---

### Task 3: TMenuView — `getHelpCtx` (highlighted-item help context to status line)

**Files:**
- Modify: `src/menu/menu_session.rs` (add `help_ctx()` method to `MenuSession`)
- Modify: `src/app/program.rs` (status-line refresh at ~line 1753; add `use crate::menu::MenuSession;`)
- Test: add a unit test for `MenuSession::help_ctx()` (and, if feasible, an end-to-end status-line test)
- Modify: `docs/audit/reference/TMenuView.md` (the `getHelpCtx` row + Summary; MISSING→PORTED)

**Interfaces:**
- Consumes: `MenuLevel { current: Option<usize>, menu }`, `MenuItem` variants (`Command`/`SubMenu`/`Separator`) each carrying `help_ctx`/`name`/`disabled` (`src/menu/mod.rs:84,97`), `HelpCtx::NO_CONTEXT`, `captures` handler list, `StatusLine::set_help_ctx`.
- Produces: `MenuSession::help_ctx() -> HelpCtx` and the wiring that feeds it to the status line while a menu is open.

**Context:** While a menu is open it is a `MenuSession` capture, not a focused subtree. The status-line refresh (`program.rs:1753-1768`) asks `captures.top_modal_view()` for the modal's `get_help_ctx()`, but `top_modal_view()` (`capture.rs:297-301`) returns `None` for a `MenuSession` (it isn't an `is_modal_gate`), so the status line gets `NO_CONTEXT` and never shows per-item help. C++ `TMenuView::getHelpCtx` (`tmnuview.cpp:453-468`) walks the `parentMenu` chain for the deepest level whose `current` item is named and has a non-`hcNoContext` helpCtx; `TStatusLine::update` (`tstatusl.cpp:209-219`) reads `TopView()->getHelpCtx()` each refresh.

**Decision (binding):** Do **not** change `MenuSession::is_modal_gate()`. Instead, in the status-line refresh, scan the capture handlers for a live `MenuSession` (downcast) and, if present, use its `help_ctx()`; otherwise keep the existing `top_modal_view()` path. This avoids perturbing global modal semantics.

- [ ] **Step 1: Write the failing test** — `menu_item_help_ctx_surfaces`: construct a `MenuSession` with a bar level + an open File submenu level whose `current` highlights a `Command` item carrying `HelpCtx::custom("file.new")`. Assert `session.help_ctx() == HelpCtx::custom("file.new")`. (Use whatever public/`pub(crate)` constructors the menu module exposes; if direct `MenuLevel` construction isn't accessible from the test, drive it through the menu-open path.)
- [ ] **Step 2: Run it, confirm it fails** (method doesn't exist / returns NO_CONTEXT).
- [ ] **Step 3: Add `MenuSession::help_ctx()`** — walk `self.levels` from the deepest (top) level toward the bar; for the first level whose `current` item is a named, non-disabled `Command`/`SubMenu` with `help_ctx != HelpCtx::NO_CONTEXT`, return that `help_ctx`; else `HelpCtx::NO_CONTEXT`. Skip `Separator`. (This mirrors the C++ parentMenu walk's intent in capture-stack terms.)
- [ ] **Step 4: Wire it into the status-line refresh** in `program.rs` (~1753): before/instead of the blind `top_modal_view()` query, look for a `MenuSession` among the active capture handlers; if found, feed `menu_session.help_ctx()` to `StatusLine::set_help_ctx`. Keep the existing modal-view path for the non-menu case. Add `use crate::menu::MenuSession;`.
- [ ] **Step 5: Run the test(s), confirm pass.** Add an end-to-end test if the harness supports opening a menu and reading the status line's help ctx.
- [ ] **Step 6: Update `docs/audit/reference/TMenuView.md`** — `getHelpCtx` row: bucket PORTED, Corr `OK`, note describes the MenuSession `help_ctx()` walk feeding the status line. Update Summary (MISSING 1→0). Restate as current behavior, no history.
- [ ] **Step 7: Full gate** (test/clippy/fmt), then commit.

---

### Task 4: TApplication — `WriteShellMsg` closure hook + Windows/Unix text

**Files:**
- Modify: `src/app/program.rs` (add `shell_msg_hook` field on `Program`, `set_shell_msg_hook` setter, init in `Program::new`, use it in the DOS_SHELL handler ~line 3308)
- Modify: `src/app/application.rs` (forward `set_shell_msg_hook` to `program`)
- Modify: `src/app/mod.rs` if a re-export is needed for the hook type
- Test: a unit test verifying the hook overrides the message
- Modify: `docs/audit/reference/TApplication.md` (the `WriteShellMsg` row + Summary)

**Interfaces:**
- Consumes: the existing DOS_SHELL command handler.
- Produces: `Program::set_shell_msg_hook(Box<dyn Fn() -> String>)` (+ `Application` forward). When unset, the default text matches C++: `"Type EXIT to return..."` on non-unix, `"The application has been stopped. You can return by entering 'fg'."` on unix.

**Context:** C++ `TApplication::writeShellMsg` is `virtual` (`app.h:342`, `tapplica.cpp:129-136`) with a DOS/Windows branch and a unix branch. Rust inlines a single unix-only `println!` (`program.rs:3308`), losing both the Windows text and the override point. The decision (confirmed with the human) is a closure hook — lightest true-runtime-customization, matching the `run_app` callback idiom.

- [ ] **Step 1: Write the failing test** — set a `shell_msg_hook` returning a sentinel string, trigger the shell-message path (or call the small helper that produces the message), and assert the sentinel is used; with no hook set, assert the platform-correct default. (If the print is not directly observable, refactor the message production into a small `fn shell_msg(&self) -> String` that the handler prints — test that.)
- [ ] **Step 2: Run it, confirm it fails.**
- [ ] **Step 3: Add the field** `shell_msg_hook: Option<Box<dyn Fn() -> String>>` to `struct Program`; init `None` in `Program::new`.
- [ ] **Step 4: Add `pub fn set_shell_msg_hook(&mut self, hook: Box<dyn Fn() -> String>)`** on `Program`; forward from `Application`.
- [ ] **Step 5: Use it in the DOS_SHELL handler** (~line 3308): compute the message as `self.shell_msg_hook.as_ref().map(|h| h()).unwrap_or_else(default_shell_msg)`, where the default is `#[cfg(not(unix))] "Type EXIT to return..."` / `#[cfg(unix)] "The application has been stopped. You can return by entering 'fg'."`, then `println!("{msg}")`. Prefer extracting the default into a small private `fn`/closure so the test can assert it.
- [ ] **Step 6: Run the test, confirm pass.**
- [ ] **Step 7: Update `docs/audit/reference/TApplication.md`** — `WriteShellMsg` row: Corr `OK`, bucket reflects the overridable closure-hook seam + platform-correct default text. Update Summary (SUSPECT count down by 1). No history phrasing.
- [ ] **Step 8: Full gate** (test/clippy/fmt), then commit.

---

### Task 5: Reconcile audit roll-ups to the fixed state

**Files:**
- Modify: `docs/audit/gap-report.md` (§1 Missing, §2 Wrong, Summary line)
- Modify: `docs/audit/coverage-matrix.md` (TOTAL counts: MISSING, SUSPECT, the affected section rows)

**Interfaces:**
- Consumes: the per-section reference files updated in Tasks 1–4.
- Produces: roll-ups consistent with the reference files (single source of truth).

**Context:** After Tasks 1–4, the genuine gaps are closed: TMenuView getHelpCtx (was the 1 MISSING) is ported; TListViewer setState, TSortedListBox scatter, and TApplication WriteShellMsg (3 of the 4 SUSPECT) are resolved; TListViewer handleEvent (the 4th SUSPECT) is reclassified OK (comment added). The roll-ups must match — stating current reality only, no creation/revision history.

- [ ] **Step 1: Rewrite `gap-report.md` §1 and §2** to reflect 0 missing / 0 wrong (or whatever genuinely remains — e.g. any §2b secondary observation deliberately left). Update the Summary line counts. Keep §2b (secondary observations) and §3 (NOT-PORTED register) as-is unless a fixed item appears there.
- [ ] **Step 2: Update `coverage-matrix.md` TOTAL** — MISSING 1→0, SUSPECT 4→0 (or remaining), and the affected per-section rows (TListViewer, TSortedListBox, TMenuView, TApplication). Recompute any derived totals.
- [ ] **Step 3: Cross-check** — grep `docs/audit/` for `SUSPECT`/`MISSING` and confirm every remaining occurrence is intentional and consistent between reference files and roll-ups. Confirm no history markers were introduced.
- [ ] **Step 4: Commit.**

---

## Notes for the orchestrator

- Tasks 1, 2 are independent mechanical leaves (Sonnet). Task 3 is moderate (Sonnet capable, watch the capture-handler downcast wiring). Task 4 is small-moderate (Sonnet). Task 5 is a docs reconciliation (cheap model).
- Tasks 3 and 4 both touch `program.rs` (different regions: ~1753 vs ~3308). Run implementers serially (the skill mandates this anyway) — no parallel implementers on the shared tree.
- After each task: two-stage review (spec-compliance then code-quality) per subagent-driven-development, fix loop until clean, then mark done in the ledger.
- Verify clippy/fmt/test on the integrated tree after each merge (worktree/shared-target "clean" claims are unreliable).
