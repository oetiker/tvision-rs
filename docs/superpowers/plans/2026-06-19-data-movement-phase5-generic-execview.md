# Data-Movement Phase 5 — generic `ExecView` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the generic view-launched modal capability ("ExecView") — `Context::request_exec_view(view, requester, then_command)` + a new `Deferred::OpenModal` variant — so a view (holding only `&mut Context`) can launch an arbitrary custom `Box<dyn View>` modal and have its result routed back; then make `examples/tcv.rs`'s Info box a real custom `Dialog` launched through it (closing the last `tcv` consumer-API gap, gap #2).

**Architecture:** This is the **view-launched** sibling of Phase 1's `Program`-launched `exec_view_with<R>` (the two modal-result paths are distinct by ownership: a view has no `&mut Program`). A view requests a modal *downward* via the existing deferred channel; the pump stashes the boxed view into the existing `pending_modal` slot with the existing `ModalCompletion::RouteModalAnswer` (which already does "deliver the close command to a view by id via `set_modal_answer` + re-inject `then_command`"); the existing `pump_and_drive` runs it via the existing single-loop `exec_view` machinery. So Phase 5 adds **one `Deferred` variant + one `Context` method + one pump arm** and reuses everything else (`pending_modal`, `pump_and_drive`, `exec_view_with_completion`, `RouteModalAnswer`, `apply_modal_completion`, `set_modal_answer`) unchanged. No new `ModalCompletion` variant.

**Tech Stack:** Rust (workspace `tvision-rs` + `tvision-rs-macros`); the single-event-loop pump + `pending_modal` driver in `src/app/program.rs`; the `Deferred` channel in `src/view/context.rs`; `insta`/`HeadlessBackend` snapshot + headless tests; `examples/tcv.rs`.

## Global Constraints

- **Spec authority:** `docs/superpowers/specs/2026-06-18-unified-data-movement-design.md` §3.4 (Generic `ExecView` — the view-launched path: `request_exec_view(view, requester, then_command)` → `Deferred::OpenModal` → `pending_modal` → deliver result to `requester`) and §5 Phase 5. Also `docs/PORTING-GUIDE.md` D9 (the pre-named `Deferred::OpenModal(Box<dyn View>)` plan: "requests the modal downward … `run()` drains the request between pumps and calls `exec_view` itself; the result is delivered back to the requester via a posted completion `Command`. *Designed, not built.*").
- **This is a NEW capability** (not behavior-preserving): it adds the ExecView seam and changes one `tcv` behavior (the Info box becomes a real custom dialog instead of a built-in `request_message_box`). Existing tests must stay green; the `tcv` smoke/help-ctx tests must stay green.
- **Command path only — the data-back `FieldValue` path is DEFERRED (YAGNI, recorded).** The sole Phase-5 consumer (`tcv`'s Info box) is read-only (OK = `Command::CANCEL`, nothing read back), and spec §3.4 scopes the `FieldValue` path to input dialogs. Do NOT build a generic "deliver the closed modal's `value()` to the requester" arm in this phase — it would ship untested. Record the deferral with a one-line reason (the house "ported-or-deliberately-not-with-reason" rule, §2.1). A future input-dialog consumer adds it.
- **Reuse `ModalCompletion::RouteModalAnswer`** for the command path — do NOT add a new `ModalCompletion` variant, and do NOT route the boxed view back through a completion for `as_any` reads (that would re-introduce the downcast this whole effort removes).
- **No *framework-internal* `dyn Any`.** The new seam introduces no downcasts.
- **Headless modal-drive discipline (HANG GUARD):** `pending_modal` is SET by `pump_once` but CONSUMED + run by `pump_and_drive` (`src/app/program.rs:764`). A test that calls `pump_and_drive` on an open modal **must pre-queue the key that closes it** (Enter/Esc on the OK button → `end_modal`), or the modal loop spins forever headless (see the warning at `src/app/program.rs:4117`). A test that only wants to verify the *wiring* uses `pump_once` and inspects `pending_modal` directly **without** driving it (model: `open_save_as_dialog_deferred_stashes_pending_modal`, `src/app/program.rs:4119`).
- **`Box<dyn View>` in `Deferred` is fine:** `Deferred` (`src/view/context.rs:66`) derives nothing and already owns `Box<dyn CaptureHandler>` (`:70`); every drain path moves each `Deferred` by value (`std::mem::take` + match-by-value at `program.rs:2009/2018`, `:808/810`, `:1611/1613`), so an arm can move the boxed view straight into `pending_modal`. No `Clone`/`Debug` constraint, no `Option<Box>`/side-slot workaround.
- **Commands:** workspace build. `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target`. **Max 2 cores:** `cargo test --workspace -j2 -- --test-threads=2`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, `cargo build --examples`. Commit messages end with the project Co-Authored-By trailer.

---

## File Structure

- `src/view/context.rs` — add `Deferred::OpenModal { view, requester, then_command }` (near `OpenMessageBox`, ~`:404`) + `Context::request_exec_view` (near `request_message_box`, ~`:1277`).
- `src/app/program.rs` — add the `Deferred::OpenModal` drain arm in the main `pump_once` deferred drain (near the other `Open*` arms, ~`:2499`), stashing into `pending_modal` with `ModalCompletion::RouteModalAnswer`. Add the seam tests in the `#[cfg(test)] mod tests`.
- `examples/tcv.rs` — replace `DirBox::open_info`'s `request_message_box` with a real custom `Dialog` built by a new testable `build_info_dialog(entry) -> Dialog` helper, launched via `ctx.request_exec_view(...)`; update the gap-#2 workaround prose in the header/comments; add tests.
- Docs: `docs/book/src/port/modal.md` (the `request_exec_view`/`exec_view_with` pair, with `tcv`'s Info box as the worked example), `docs/book/src/port/deferred.md` + `docs/book/src/internals/deferred.md` (the `OpenModal` variant), `docs/book/src/reference/symbol-map.md` / `reference/deviations.md` (new symbols), `docs/IMPLEMENTATION-LOG.md`, `docs/HANDOVER.md`. (Generated `docs/book/book/**` is regenerated by `cargo xtask docs` — never hand-edit.)

---

## Task 1: The ExecView seam — `Deferred::OpenModal` + `Context::request_exec_view` + pump arm

**Files:**
- Modify: `src/view/context.rs` (variant + method)
- Modify: `src/app/program.rs` (pump arm + tests)

**Interfaces:**
- Produces: `Deferred::OpenModal { view: Box<dyn View>, requester: ViewId, then_command: Option<Command> }`.
- Produces: `Context::request_exec_view(&mut self, view: Box<dyn View>, requester: ViewId, then_command: Option<Command>)` — queues `Deferred::OpenModal`. The view-launched modal entry point (the sibling of `Program::exec_view_with<R>`). Consumed by Task 2.
- Reuses: `pending_modal` (`program.rs:351`), `ModalCompletion::RouteModalAnswer` (`program.rs:382`), `pump_and_drive` (`program.rs:764`).

- [ ] **Step 1: Write the failing seam test (stash-inspect — does NOT drive the modal)**

In `src/app/program.rs`, in the `#[cfg(test)] mod tests`, add a test modeled on `open_save_as_dialog_deferred_stashes_pending_modal` (`src/app/program.rs:4119` — read it first for the exact `Program` test-construction + `pending_modal` access pattern). The test: build a headless `Program`, get a requester `ViewId` (any inserted view's id — e.g. a desktop child, or the desktop itself), have a `Context`/handler call `request_exec_view` with a trivial `Box::new(Dialog::new(...))` modal + the requester id + `Some(some_command)`, run **`pump_once`** (NOT `pump_and_drive`), then assert `program.pending_modal` is `Some((_, ModalCompletion::RouteModalAnswer { answer_to, then_command }, None))` with `answer_to == requester` and `then_command == Some(some_command)`. Match the existing test's exact mechanism for reaching `request_exec_view` (how that test triggers `request_save_as_dialog` — via a view's `handle_event` queuing a deferred, or a direct `Context` construction; mirror it). If `pending_modal`/`ModalCompletion` are private, the test is in-module so it can read them (the model test does).

- [ ] **Step 2: Run it — confirm it fails to compile (`OpenModal`/`request_exec_view` don't exist)**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test --workspace -j2 -- --test-threads=2 request_exec_view 2>&1 | tail -20
```
Expected: compile error — `Deferred::OpenModal` and `Context::request_exec_view` are undefined.

- [ ] **Step 3: Add the `Deferred::OpenModal` variant**

In `src/view/context.rs`, in `pub enum Deferred` (near the `OpenMessageBox` variant, ~`:404`), add:
```rust
    /// **Generic view-launched modal (ExecView).** A view (holding only
    /// `&mut Context`, never `&mut Program`) requests an arbitrary custom modal
    /// be run. The pump moves `view` into the loop-owned `pending_modal` slot with
    /// a [`RouteModalAnswer`] completion; the outer `pump_and_drive` runs it via the
    /// existing single-loop `exec_view` machinery, then delivers the modal's close
    /// command to `requester` (via [`View::set_modal_answer`]) and re-injects
    /// `then_command`. The view-launched sibling of [`Program::exec_view_with`].
    ///
    /// Queued by [`Context::request_exec_view`]. The boxed view is owned by the
    /// variant and moved out at drain (the deferred queue is drained by value, so a
    /// `Box<dyn View>` field needs no `Clone`).
    OpenModal {
        /// The custom modal to run.
        view: Box<dyn View>,
        /// The view to deliver the modal's close command to (by id).
        requester: ViewId,
        /// Command to re-inject after the modal closes (`None` = nothing to re-post).
        then_command: Option<Command>,
    },
```
Fix the doc intra-links to fully-qualified forms if the crate requires (`[`RouteModalAnswer`]` may need a code span if `ModalCompletion` is `pub(crate)`-and-not-linkable — match how other `Deferred` docs reference pump-local types; a plain code span `` `RouteModalAnswer` `` is safe).

- [ ] **Step 4: Add the `Context::request_exec_view` method**

In `src/view/context.rs`, near `request_message_box` (~`:1277`), add:
```rust
    /// Request a generic view-launched modal (ExecView): run `view` as a modal,
    /// then deliver its close command to `requester` (via
    /// [`View::set_modal_answer`]) and re-inject `then_command`. The view-launched
    /// counterpart of [`Program::exec_view_with`](crate::app::Program::exec_view_with)
    /// (which a view cannot call — a view holds only `&mut Context`, never
    /// `&mut Program`). Queues [`Deferred::OpenModal`].
    ///
    /// `then_command` is `None` for a fire-and-forget modal (e.g. a read-only info
    /// dialog). To act on the result, the `requester` overrides
    /// [`View::set_modal_answer`] to cache the close command and acts on
    /// `then_command` when it is re-posted.
    pub fn request_exec_view(
        &mut self,
        view: Box<dyn View>,
        requester: ViewId,
        then_command: Option<Command>,
    ) {
        self.deferred
            .push(Deferred::OpenModal { view, requester, then_command });
    }
```
(Match the exact `self.deferred` field name/push idiom used by the neighbouring `request_*` methods — read `request_message_box` first.)

- [ ] **Step 5: Add the pump drain arm**

In `src/app/program.rs`, in the main `pump_once` deferred-drain `match effect { ... }` (the one fed by `let effects = std::mem::take(deferred);` ~`:2009`/`:2018`), near the other `Open*` arms (~`:2499`, after the `OpenMessageBox` arm), add:
```rust
                            // Generic view-launched modal (ExecView): move the
                            // caller-built modal into pending_modal with a
                            // RouteModalAnswer completion (deliver the close command
                            // to `requester` by id + re-inject `then_command`). The
                            // outer pump_and_drive execs it via the existing single
                            // loop. Reuses the Open*Dialog → pending_modal pattern;
                            // no new ModalCompletion variant, no downcast. `None`
                            // initial focus = the modal focuses its own first view.
                            Deferred::OpenModal {
                                view,
                                requester,
                                then_command,
                            } => {
                                *pending_modal = Some((
                                    view,
                                    ModalCompletion::RouteModalAnswer {
                                        answer_to: requester,
                                        then_command,
                                    },
                                    None,
                                ));
                            }
```
**Note:** the `valid_end` (`:808`) and `validate_modal_close` (`:1611`) drains partition only `OpenMessageBox`/`OpenSaveAsDialog`; an `OpenModal` queued during end-validation falls into their `other => self.deferred.push(other)` arm (re-queued by value to the next pump). That is acceptable — no view requests a custom modal from inside `valid()` today; do NOT special-case it.

- [ ] **Step 6: Run the seam test (now passes)**

```bash
cargo test --workspace -j2 -- --test-threads=2 request_exec_view 2>&1 | tail -20
```
Expected: the Step-1 test PASSES (`pump_once` stashes `pending_modal` with `RouteModalAnswer { answer_to: requester, then_command }`).

- [ ] **Step 7: Add a round-trip drive test (modal opens, closes, requester gets the answer + then_command fires)**

Add a second test that proves the end-to-end route, modeled on the `pump_and_drive`-driving tests at `src/app/program.rs:9227`/`:10571` (read one first). It needs a tiny **recorder requester view** — a test-only `View` whose `set_modal_answer` records the `Command` it received (e.g. into a `Rc<Cell<Option<Command>>>` or a field readable after) — inserted into the group so `group.find_mut(requester)` resolves it. Then: a handler calls `request_exec_view(Box::new(<a Dialog with an OK button emitting Command::CANCEL>), recorder_id, Some(SOME_CMD))`; **pre-queue the close** (`screen.push_key(Key::Enter, ...)` or `Key::Esc` so the modal's OK/cancel fires `end_modal`); call `program.pump_and_drive()`; assert (a) the recorder's `set_modal_answer` was called with the modal's end command, and (b) `SOME_CMD` was re-injected (observe it via the recorder reacting, or by asserting it appears in `out_events`/is dispatched on the next pump). Keep it minimal but real — it must actually drive the loop to `end_modal`, not just inspect `pending_modal`. If a full recorder view is too heavy, at minimum assert the modal runs and closes without hanging and `pending_modal` is `None` afterward, and document that the answer-routing is covered by the `tcv` integration test in Task 2.

- [ ] **Step 8: Build, test, lint**

```bash
cargo test --workspace -j2 -- --test-threads=2
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```
Expected: all green; both new tests pass; no snapshot changes (no existing view's rendering changed).

- [ ] **Step 9: Commit**

```bash
git add -A
git commit
# message: "feat(context,program): request_exec_view + Deferred::OpenModal — generic view-launched modal"
```

---

## Task 2: Make `tcv`'s Info box a real custom `Dialog` launched via `request_exec_view`

**Files:**
- Modify: `examples/tcv.rs` — replace `DirBox::open_info` (~`:356-368`); add a `build_info_dialog(entry) -> Dialog` helper; update the gap-#2 workaround prose; add tests.

**Interfaces:**
- Consumes: `Context::request_exec_view` (Task 1).
- Produces: `build_info_dialog(&CatalogEntry) -> Dialog` (or whatever the entry type is named — confirm by reading `tcv.rs`) — a testable factory for the Info dialog (six labelled rows + OK button).

- [ ] **Step 1: Read the current Info-box workaround + the entry type**

Read `examples/tcv.rs:237-245` (`info_text` helper — the six fields), `:356-368` (`DirBox::open_info` — the current `request_message_box`), `:447-451` (the `CMD_INFO` handler), and the gap-#2 prose in the header (grep `gap`/`Info`/`ExecView`/`request_message_box`). Note the entry struct name + its field accessors (Disk Label / File Name / File Date / Space Used / Description / Scan Date) and how `info_text` formats them.

- [ ] **Step 2: Write the failing dialog-builder test (content, rendered standalone via the D11 snapshot pattern)**

Add a `#[cfg(test)]` test that builds an Info dialog for a known entry via `build_info_dialog(entry)` and renders it standalone on a `HeadlessBackend` (follow the existing widget snapshot-test pattern — build view, render, `screen.snapshot()`, assert). Assert the rendered frame contains the entry's key fields (e.g. the disk label, the file name, the description text) and an "OK" button. (Use a plain `assert!(frame.contains(...))` like the existing `tcv` tests at `:769`/`:781`, not `insta`, unless you add a frozen snapshot — a `contains` assertion on the distinctive field values is sufficient and robust.) This will fail to compile (`build_info_dialog` doesn't exist).

- [ ] **Step 3: Add `build_info_dialog` and rewrite `open_info`**

Add a `build_info_dialog(entry: &<EntryType>) -> Dialog` free fn / helper that builds the dialog the C++ `TDirBox.HandleEvent` built: a centered `Dialog` titled e.g. `"Information"` sized to fit six rows + a button, with six `Label`/static-text rows (one per field, reusing the `info_text` field formatting — either six `Label`s or the `\n`-joined text in a multi-line static text; prefer six discrete labelled rows for faithfulness, but a single multi-line static text is acceptable if simpler and renders the same content), and an OK `Button` whose command is `Command::CANCEL` (the read-only-info convention, spec "OK = `cmCancel`"; functionally just closes the modal — nothing is read back).

Then rewrite `DirBox::open_info` (`:356-368`) to:
```rust
    fn open_info(&mut self, ctx: &mut Context) {
        if let Some(entry) = /* the focused entry, as today */ {
            let dialog = build_info_dialog(entry);
            if let Some(id) = self.state().id() {
                ctx.request_exec_view(Box::new(dialog), id, None);
            }
        }
    }
```
(Match the real local API: how the focused entry is obtained today in `open_info`, and the correct `state().id()` accessor for `DirBox`. `then_command = None` and `DirBox` needs NO `set_modal_answer` override — the info box is fire-and-forget.) Keep the `CMD_INFO` handler (`:447-451`) unchanged.

- [ ] **Step 4: Run the builder test (passes)**

```bash
cargo test --workspace -j2 -- --test-threads=2 -p tvision-rs --example tcv 2>&1 | tail -20
# (or the workspace test invocation that runs example tests — confirm how tcv tests are run; they live in examples/tcv.rs under #[cfg(test)])
```
Expected: the builder test passes — the dialog renders the entry's fields + OK button.

- [ ] **Step 5: Add the integration test (CMD_INFO opens a modal via the seam)**

Add a `tcv` test modeled on the existing `constructs_and_renders_without_panic` (`:758`): construct `TcvApp`, `pump_once` to render, focus the list + select an entry, push the key that triggers `CMD_INFO` (Enter on a focused entry — confirm the binding at `:524`), `pump_once`, and assert `app.program` now has a `pending_modal` set (the Info box opened) — OR, if `pending_modal` isn't reachable from the example test, drive it: **pre-queue the close key** (Esc/Enter on OK), call `app.program.pump_and_drive()`, and assert it returns without hanging and the app is back to the browse view (the modal closed). Pick whichever `pending_modal` visibility allows from the example crate; the drive-with-pre-queued-close variant is the robust fallback. Do NOT call `pump_and_drive` on an open modal without a pre-queued close (hang guard).

- [ ] **Step 6: Update the gap-#2 workaround prose**

In `examples/tcv.rs`, update the header/comment that documented the Info-box as a *workaround* (the gap-#2 note) to state it is now a real custom `Dialog` launched via `request_exec_view` (gap #2 closed). Remove any "faked as a built-in message box" / "LIMITATION"-style wording. Keep the `CMD_ABOUT` box as a `request_message_box` (out of scope — it is a genuine built-in info box; note this if the prose references it).

- [ ] **Step 7: Build, test, lint (incl. examples)**

```bash
cargo test --workspace -j2 -- --test-threads=2
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo build --examples
```
Expected: all green; the existing `tcv` smoke + help-ctx tests still pass; the new builder + integration tests pass.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit
# message: "example(tcv): Info box is a real custom Dialog via request_exec_view (closes consumer-API gap #2)"
```

---

## Task 3: Docs + record the deferred data-back path + final whole-phase verification

**Files:**
- Modify: `docs/book/src/port/modal.md` (the `request_exec_view` / `exec_view_with` pair, `tcv` Info box as the worked example).
- Modify: `docs/book/src/port/deferred.md` + `docs/book/src/internals/deferred.md` (the new `OpenModal` variant).
- Modify: `docs/book/src/reference/symbol-map.md` + `docs/book/src/reference/deviations.md` (new symbols: `request_exec_view`, `Deferred::OpenModal`).
- Modify: `docs/IMPLEMENTATION-LOG.md` (prepend Phase 5 section), `docs/HANDOVER.md` (mark Phase 5 done; the data-movement stack is now complete).
- Modify: the worktree SDD ledger is maintained by the controller — do NOT edit it.

**Interfaces:** none (docs + verification).

- [ ] **Step 1: Record the deferred data-back path at the code seam**

In `src/view/context.rs`, on the `Deferred::OpenModal` doc (or `request_exec_view` doc), append a one-line recorded deferral: the **data-back `FieldValue` path is deliberately not built** — the result is delivered as the close *command* (via `RouteModalAnswer` → `set_modal_answer`); a future input-dialog consumer that needs the modal's typed `value()` back would add a generic data arm (read `modal_id.value()` → `requester.set_modal_data(...)`), but no current consumer needs it (tcv's Info box is command-only), so it is not shipped untested (§3.4/§2.1). Commit this with the docs (or fold into Task 1's commit if you prefer — but keep it).

- [ ] **Step 2: Update `port/modal.md`**

Document the modal-launch decision rule: a **`Program` method** launches a modal and gets the result by value via `exec_view_with<R>` (Phase 1); a **view** launches a modal via `Context::request_exec_view(view, requester, then_command)` and gets the close command routed back to it (via `set_modal_answer`) + `then_command` re-injected. Use `tcv`'s Info box as the worked example — ideally via `{{#rustdoc_include ../../../examples/tcv.rs:ANCHOR}}` IF you add `// ANCHOR:`/`// ANCHOR_END:` markers around `build_info_dialog`/`open_info` in `tcv.rs` (then it stays `rust,ignore` and compiles via `cargo build --examples`); otherwise a prose-only description with a hidden-`# use tvision_rs as tv;` doctest snippet that compiles under `cargo xtask test`. Prefer prose + the rustdoc_include if anchors are clean; do NOT add an uncompilable ```rust block.

- [ ] **Step 3: Update `deferred.md` (both) + `symbol-map.md` / `deviations.md`**

In `port/deferred.md` + `internals/deferred.md`, add `OpenModal` to the documented `Deferred` variants (the view-launched-modal request; reuses `pending_modal` + `RouteModalAnswer`). In `reference/symbol-map.md`, map C++ `execView` (from within a view) → `Context::request_exec_view` (and note `Program::exec_view_with<R>` is the method-caller path). In `reference/deviations.md`, note the consolidation against D9 (the pre-named `OpenModal` plan is now built).

- [ ] **Step 4: Prepend the IMPLEMENTATION-LOG section**

Newest-first Phase 5 section: the `request_exec_view` + `Deferred::OpenModal` seam (reuses `pending_modal`/`RouteModalAnswer`/`pump_and_drive`, no new `ModalCompletion` variant, no downcast); `tcv`'s Info box is now a real custom `Dialog` (gap #2 closed); the data-back `FieldValue` path deliberately deferred (reason recorded). Note this **completes the data-movement effort (Phases 1–5)**.

- [ ] **Step 5: Update HANDOVER**

In `docs/HANDOVER.md`'s `## Current state (2026-06-19)` section: move Phase 5 into the "stacked on the branch" list (done); update the "Next" line — the **data-movement stack (Phases 1–5) is now complete**; the remaining open items are the recorded deferred follow-ons (`inventory`-collected `Program::self_check()`, `MultiCheckBoxes::value()` → `Bits`, the deferred ExecView data-back path) and the **whole-stack merge decision** (disposition has been KEEP STACKING — the stack lands together; flag that the branch is now feature-complete and awaiting the merge call). Keep HANDOVER slim.

- [ ] **Step 6: Verification gate (whole phase)**

```bash
cargo test --workspace -j2 -- --test-threads=2
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo build --examples
cargo xtask test
cargo xtask docs
grep -rl rustdoc_include docs/book/book/ || echo "no leftover include directives"
```
Expected: all green; `cargo xtask test` compiles any new guide rust block; `cargo xtask docs` builds + link-checks (only the documented pre-existing link warnings — add no new ones for your new symbols). If `cargo xtask docs` re-captures screenshot HTML under `docs/book/src/screens/` (tmux drift), do NOT commit that drift — Phase 5 changes no widget rendering except adding the tcv Info dialog (which has no committed screenshot); revert spurious screenshot changes.

- [ ] **Step 7: Commit**

```bash
git add -A   # docs only — verify `git status` shows no spurious screenshot drift staged
git commit
# message: "docs(guide,log,handover): Phase 5 generic ExecView + tcv Info box; data-movement stack complete"
```

---

## Self-Review

**Spec coverage (§3.4 / §5 Phase 5):**
- "`Context::request_exec_view(view, requester, then_command)` queues `Deferred::OpenModal`" → Task 1 Steps 3–4 (exact signature) + Step 5 (the arm).
- "The pump stashes it in `pending_modal`, runs it via the existing single-loop machinery, and on close delivers the result to `requester` (command via `set_modal_answer`) and re-injects `then_command`" → Task 1 Step 5 reuses `pending_modal` + `RouteModalAnswer` (which does exactly that, `program.rs:3022`); verified by the Task 1 round-trip test (Step 7) + the tcv integration test (Task 2 Step 5).
- "make `tcv`'s Info box a real custom Dialog launched from the list" → Task 2 (the `build_info_dialog` + `request_exec_view` rewrite, gap #2 closed).
- "`tcv`'s Info box exercises only the command path; the `FieldValue` path serves input dialogs" → the data-back path is deferred + recorded (Global Constraints + Task 3 Step 1).
- "Snapshot" (§5) → Task 1 seam + round-trip tests; Task 2 builder snapshot + integration test.
- Docs land with the phase (§9) → Task 3.

**Placeholder scan:** the seam code (variant, method, arm) is given verbatim; the tests cite concrete existing models (`program.rs:4119` stash-inspect, `:9227`/`:10571` drive) rather than inline full code, because the exact `Program` test-construction harness must be copied from those models — the implementer reads the model and mirrors it (the one place local harness detail must come from the source, not a guess). `build_info_dialog`/`open_info` cite the exact `tcv.rs` lines to mirror.

**Type consistency:** `Deferred::OpenModal { view: Box<dyn View>, requester: ViewId, then_command: Option<Command> }` is identical across the variant def (T1 S3), `request_exec_view` (T1 S4), and the pump arm (T1 S5). The arm stashes `ModalCompletion::RouteModalAnswer { answer_to: requester, then_command }` — matching the existing variant's field names (`program.rs:382`). `request_exec_view`'s signature matches spec §3.4 verbatim.

**Out-of-scope guard:** no new `ModalCompletion` variant (reuse `RouteModalAnswer`); no data-back `FieldValue` arm (deferred, recorded); `CMD_ABOUT` left as `request_message_box`; `valid_end`/`validate_modal_close` drains not special-cased (the `other =>` re-queue is acceptable, documented in T1 S5).
