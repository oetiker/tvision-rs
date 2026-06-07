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

- **HEAD `3a3661b`+ (row 67 `TMemo` lands this session).** Build: **758 lib tests** green; `cargo clippy --workspace
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
  initial-modal-currency seam** (`View::reset_current`), and **all of row 63
  (`messageBox`/`messageBoxRect`/`inputBox`/`inputBoxRect`)** — the latter via the
  **single-input scatter/gather seam** (`exec_view_with_completion`'s `gather`
  param), and **row 64 (`StringList`)** — a D12 minimal port (`BTreeMap<u16,String>`
  wrapper in `src/text.rs`; the `TStreamable` resource-stream machinery dropped),
  and **row 66 (`TEditor`) core** — gap-buffer editor, nav, edit, undo, selection,
  draw, search, keyboard+clipboard; (find/replace dialogs + mouse-drag + context
  menu + clipboard-editor deferred — see row-66 deferrals below),
  and **row 67 (`TMemo`)** — a D2 embed-delegate wrapper over `Editor`
  (`#[delegate(to = editor)]`, no skip — `as_any_mut` delegates so the editor's
  pump brokers reach through a `Memo`); overrides Tab-swallow + D10
  `value`/`set_value` (new inherent `Editor::set_text`); `dataSize`/`getPalette`
  dropped (D10/D7). Fixed a latent row-66 editor bug along the way (Shift+Tab was
  wrongly insertable — `kbShiftTab` charCode 0 must not insert).
  The `#[delegate]` proc-macro is landed and adopted codebase-wide.

## Next — lowest-numbered remaining work

**Row 67 `TMemo` is ✅ this session.** The next porting row is **68
`TFileEditor`** (MECHANICAL — `TEditor` subclass with load/save file backing,
`tfiledtr.cpp`), then **69 `TEditWindow`** (`TWindow` owning a `TFileEditor` +
scrollbars + `TIndicator`, where the deferred row-66 clipboard-editor branch and
find/replace dialogs wire in). They do **not** structurally require the other
deferred row-66 sub-features. Two non-gating seams remain available before or
alongside the editor family:

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

2. **Validator `error()` → `messageBox` wiring (its own seam).** All five
   validators' `error()` are `TODO(row 63)` no-op breadcrumbs preserving the exact
   C++ strings. Wiring them is **blocked on `Validator::error(&self)` having no
   `Context`** — it cannot reach a deferred channel to request a modal. Needs a
   trait-signature change (thread a `&mut Context` / a deferred handle through
   `error()` and its `InputLine::valid` caller), then a `Deferred::OpenMessageBox`
   variant + the row-57 `pending_modal` async path (the `OpenHistory` precedent).
   The sync `Program::message_box` exists now; this is the *async-from-a-view* face.

**Row 66 deferred sub-features** (breadcrumbed TODOs in `editor.rs`; pick up when
relevant prerequisites land):
1. **Find/Replace dialogs** (`editorDialog`, `find()`/`replace()`/`efPromptOnReplace`)
   — `search()` is live; `cmFind`/`cmReplace` are no-ops until the std dialog views exist.
2. **Mouse drag-select/edge-scroll/wheel/middle-button pan** — single-click
   positioning is live; the `while(mouseEvent)` drag loops need a `DragCapture`
   handler (precedent: `window.rs DragCapture`; also deferred for scrollbar, `TODO(row 31)`).
3. **Right-click context menu** (`initContextMenu` + `popupMenu`).
4. **Internal-clipboard `TEditor` branch** (`insertFrom` from a sibling editor) —
   deferred to row 69 (`TEditWindow` wires the clipboard editor).
5. `TStreamable` write/read/build (D12).

Phase 5 then continues in PORT-ORDER with **68** (`TFileEditor`) and **69**
(`TEditWindow`), then the std-dialog / file / color / outline families.
`cmDosShell` is still deferred — needs a backend terminal-suspend seam + SIGTSTP.

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
- **Non-Scroller D3 broker pattern established** (`SyncEditorDelta` +
  `Editor::apply_scroll_delta`): a non-`Scroller` view that needs scrollbar
  siblings adds a new `Deferred` variant with a concrete downcast in the
  pump's deferred-apply loop. Future views follow the same pattern.
- **`Deferred::IndicatorSetValue { indicator, location, modified }`** is live
  — any editor-like view drives its `TIndicator` sibling through the pump via
  this variant (downcast to `Indicator`, `set_value`).
- **Clipboard broker** (`Deferred::SetClipboard(String)` + `Deferred::EditorPaste(ViewId)`)
  is live — the deferred-apply scope in `program.rs` reaches
  `renderer.backend_mut()` for clipboard I/O; paste re-queues scrollbar-param
  ops that settle on the next pump (one-pass drain is expected).
- **`Role::ScrollerSelected`** is now filled (the `theme.rs` breadcrumb is
  cleared); editor normal text reuses `Role::ScrollerNormal`.
- **Editor ctx-threading split** is a reusable pattern for ctor-state-heavy
  widgets: keep core mutation methods `Context`-free (accumulate into flag fields);
  let `&mut Context` thread only into the handful of entry points that actually need
  it. Makes the whole widget unit-testable without a running pump.

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
