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

- **The outline family (rows 88–90) is COMPLETE and on `main`.** `Node` /
  `OutlineViewer` (trait + free functions, like `TListViewer`) / `Outline` live in
  `src/widgets/outline.rs`. **HEAD = `7472343`; 941 lib tests green; clippy + fmt
  clean.** Reusable seams added: `Role::Outline{Normal,Focused,Selected,NotExpanded}`
  (`ROLE_COUNT` 58→62), `Command::OUTLINE_ITEM_SELECTED`,
  `Deferred::SyncOutlineViewerDelta` (+ `Context::request_sync_outline_viewer_delta`
  + pump arm — the scrollbar→delta read-broker, mirrors `SyncScrollerDelta`).
  **Known follow-ups (deferred, faithful):** mouse drag-loop / edge-scroll
  (`TODO(row 31, D9)` — single-click positioning only, like every other widget);
  `Outline` ctor does not call `update()` (no `Context`), consumers call `ov_update`
  after insertion (documented, same as scroller/list-viewer); no runnable app wires
  an `Outline` yet, so the scrollbar read-broker is exercised only by unit tests.
- **The truecolor color-picker extension (tasks 0–9) is COMPLETE and on `main`.**
  Rows 81–82 were reverted (`9aa8e12`); the picker is built in
  `src/dialog/colorpick/` — `ColorModel` + `Hsv` + conversions, four surfaces
  (`PresetsSurface`, `RgbSurface`, `PlaneSurface`, `Xterm256Surface`), the
  `ColorPicker` view (tabs, info column, `color()`), and the mouse-drag broker
  (`Deferred::ColorPickerDrag` + `ColorDragCapture` + pump arm).
  **HEAD = `2b0751f` (mouse drag broker); 921 lib tests green; clippy + fmt
  clean.**
- **The picker is now fully complete** including `Program::color_dialog` (Task 10,
  `5b1fabf`). HEAD = `5b1fabf`; 924 lib tests green; clippy + fmt clean.
- **Direction change summary:** PORT-ORDER rows 81–87 (`colordlg`) are DROPPED —
  the C++ `TColorDialog` family edits a flat BIOS `TPalette` rstv deleted under
  D7 (palette → `Theme`; `Role` is a closed enum). The truecolor picker replaces
  them (spec:
  [`docs/superpowers/specs/2026-06-09-color-picker-design.md`](file:///home/oetiker/checkouts/rstv/docs/superpowers/specs/2026-06-09-color-picker-design.md),
  plan:
  [`docs/superpowers/plans/2026-06-09-color-picker.md`](file:///home/oetiker/checkouts/rstv/docs/superpowers/plans/2026-06-09-color-picker.md)).
  A future **theme editor** will consume `color_dialog` (needs the D7 "Theme
  extension point" first — a separate sub-project, not on the critical path).
- **Key seams the picker adds (reusable):**
  - **`Deferred::ColorPickerDrag` + pump arm** — the `window.rs DragCapture`
    pattern reused for a non-window view: a `CaptureHandler` converts absolute
    `MouseMove` → picker-local, posts the deferred, pump downcasts to
    `ColorPicker::apply_drag`. Coordinate contract: ONE frame (picker-local)
    everywhere; each surface subtracts `body.a` exactly once.
  - **`ModalCompletion::ColorPick { picker, sink }`** (Task 10, not yet built):
    on `cmOK`, downcasts the in-tree modal `ColorPicker` to read `color()` and
    write into an `Rc<Cell<Option<Color>>>` — same shape as `HistoryPick`.
    **No `FieldValue::Color`** (the spec's explicit non-goal; `color()` is the
    contract). Do not edit `data.rs`.
- **The makeDefault broker is now built** (FOUNDATION, row 80):
  `Deferred::MakeButtonDefault { button, enable }` + `Context::make_button_default`
  + a pump arm that downcasts `Button` and calls `make_default(enable, ctx)`.
  `Button::make_default` is now `pub(crate)` and `Button::as_any_mut` returns
  `Some(self)`. Reuse this for any future "a leaf view makes a sibling button the
  default" need. The two row-75 `DirListBox` breadcrumbs are resolved (row 80 was
  their only consumer): `select_item`→`ctx.post(cmChangeDir)` + the dialog reads
  `focused_entry()`; `set_state`→the new broker.
- **The payload-carrying-broadcast seam is now built** (FOUNDATION, row 77):
  `Deferred::ResolveFocusedFile { subscriber, source }` + a defaulted
  `ListViewer::on_focus_changed` hook (called at the `focus_item` tail — the
  faithful virtual-`focusItem`). `FileList` broadcasts payload-less
  `FILE_FOCUSED {source=self}` on every focus change; the pump resolves
  `focused_rec()` and concrete-downcasts the subscriber (`FileInputLine` /
  `FileInfoPane`). Reuse this for any future payload-carrying broadcast.
- **The sorted-search seam** (`SortedSearch: ListViewer` sub-trait +
  `sorted_handle_event`/`sorted_cursor` free fns in `list_viewer.rs`): both
  `SortedListBox` and `FileList` are direct `ListViewer` impls implementing it.
  Row 80's `TChDirDialog` uses `DirListBox` (a direct impl that does NOT need the
  search machine).
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
  wrongly insertable — `kbShiftTab` charCode 0 must not insert),
  and **row 68 (`TFileEditor`) core** — a D2 embed-delegate `FileEditor`; the inner
  `Editor` gained a **flag-gated growable buffer** (`file_editor` flag,
  `set_buf_size(&mut)` grow branch, `new_file_editor` ctor) — base/`Memo`
  fixed-buffer behavior provably unchanged; `load_file`/`save_file`/`save` over real
  `std::fs`, `handle_event` cmSave, `valid` cmValid (saveAs + error/confirm dialogs +
  the modified-prompt forced-deferred on `TFileDialog`/async-modal-from-view),
  and **row 69 (`TEditWindow`)** — a D2 embed-delegate `EditWindow` over `Window`
  assembling hidden `ScrollBar`×2 + `Indicator` + a `FileEditor` (ViewId-at-insertion
  wiring order; `ofTileable`; `size_limits` {24,6} with the mandatory `calc_bounds`
  skip; hidden aux children excluded from `reset_current` so the editor is current).
  **The `TEditor` family (66–69) is now complete** modulo the breadcrumbed
  editor sub-features,
  and **row 70 (`TSortedListBox`)** — a D2 embed-delegate `SortedListBox` over
  `ListBox` with type-to-search incremental search over a case-insensitively-sorted
  `Vec<String>` (no generic `TSortedCollection`; the `curString`-re-seed model +
  delegate→reset→gate sequence ported faithfully),
  and **rows 71–74 (file-dialog data classes)** — `DirEntry`/`SearchRec` structs,
  `DirCollection = Vec<DirEntry>` alias, and `FileCollection` (`Vec<SearchRec>` +
  verbatim `search_rec_compare` + sorted insert) in `src/dialog/filedlg.rs` (pure
  data; collections→Vec; batched),
  and **the async-modal-from-a-view seam** (FOUNDATION detour) — a downward-borrowed
  `&mut View` can now request a modal `messageBox` from the pump and observe the
  choice (`Deferred::OpenMessageBox` + `Context::request_message_box`,
  `View::set_modal_answer`, `ModalCompletion::{RouteModalAnswer,Informational}`,
  `apply_modal_completion`→`Option<Event>` re-injected into `out_events`, the inline
  `validate_modal_close` drive for the event-gated modal-close path, and the
  `View::valid(&mut self, cmd, ctx)` signature change). **Retired three consumer
  clusters:** all 5 validator `error()` boxes, `FileEditor::valid`'s Yes/No/Cancel
  modified-save prompt, and `FileEditor` save-error boxes (design note:
  `docs/design/async-modal-from-view.md`),
  and **row 75 (`TDirListBox`)** — `DirListBox`, a **direct `ListViewer` impl** over
  `Vec<DirEntry>` (NOT a D2 delegate — a delegate would consult `ListBox`'s
  `Vec<String>` `get_text`); introduced **deviation D14 (native Linux `/` paths)**:
  `showDrives`/drive-letters/"Drives"/`\` dropped, `showDirs` → a pure
  `build_tree` (root `/` + `/`-segment ancestors + sorted `read_dir` subdirs,
  dotfiles skipped, symlinks followed) split from the FS read for
  snapshot-testability, faithful unconditional last-entry glyph fix-up; `select_item`
  payload-command + `set_state` `chDirButton` poke breadcrumbed → row 80. The
  `#[delegate]` proc-macro is landed and adopted codebase-wide.

## Next — all 92 rows complete; the backlog run

**All 92 PORT-ORDER rows are COMPLETE.** The work is now the **post-port
backlog run** — direction lives in
[`docs/BACKLOG.md`](file:///home/oetiker/checkouts/rstv/docs/BACKLOG.md)
(audited 2026-06-10; FOUNDATION seams Phase A first, mechanical fan-out
Phase B after, big features Phase C backlogged). Two standing user
directives recorded there: **OS clipboard by default** (row A6) and **no
hand-rolled terminal setup in app code** (row B7). **HEAD = `faabc78`; 995
lib tests green; clippy + fmt clean.** Landed from the backlog run so far:
**A1** (the CommandSet denylist flip — `Context::command_enabled` now exists
for B1; `docs/design/command-enablement.md`) and **A4** (the theme
chain-verification pass — every `theme.rs` value chain-documented, cyan
window scheme live, `ListRoles`/`list_roles()` seam). Cleared recently: the **currency foundation fix** (`focus_child`
self-heal + `Program::new` startup `reset_current`; the `insert_and_focus`
DEVIATION workaround retired — pre-inserted desktop windows now start focused
and the topmost is clickable), **button mouse hold-tracking** (the button deferral-3
D9 capture: press-down, track, fire-on-release-inside) + the **gray dialog
surface** (row-34 gray theming: `FrameGray*` roles, `Frame.palette` role-family
selection, `Window::set_palette` propagation — cyan still blue-fallback) + the
**ButtonShadow chain fix** (black-on-lightgray 0x70), the **D8 window-shadow
pass** (the row-33 TODO — `Role::Shadow`, `DrawCtx::cast_shadow`, the
`Group::draw` hook), the **ModalFrame outside-click seam** (row 56/57),
**`FileEditor::saveAs`** (rows 68/69 breadcrumb), and the **`hello.rs` demo
app** wiring `EditWindow + FileDialog` end-to-end — see IMPLEMENTATION-LOG.

**Rows 91–92 (terminal family) are COMPLETE and on `main`.** `TextDevice` (trait)
and `Terminal` (ring-buffer terminal view) live in `src/widgets/terminal.rs`.

Key design choices:
- `TextDevice` is a plain trait (D11: `streambuf` dropped); users call `write_bytes`.
- `Terminal` embeds a `Scroller` with `#[delegate(to = scroller)]` on the `View`
  impl; `as_any_mut` auto-forwards to the inner `Scroller`, so the existing
  `SyncScrollerDelta` pump arm works without a new `Deferred` variant.
- Ctor takes no `Context`; consumer calls `Terminal::init(&mut self, ctx)` after
  insertion (same pattern as `TOutline`).
- `draw()`: faithful ring-buffer backward scan via `prev_lines`/`find_lf_backwards`
  (from `ttprvlns.cpp`); UTF-8 boundary trimming via `str::from_utf8`.

**The 92-class porting checklist is now fully complete.**

**Entry point for `color_dialog`:** `Program::color_dialog(initial: Color) ->
Option<Color>` at `src/app/program.rs`. Also re-exported as `tvision::ColorPicker`
and `tvision::Tab`. A future **theme editor** will consume `color_dialog` — that
needs the D7 "Theme extension point" (runtime `Role→Style` registration) first,
a separate sub-project not on the critical path.

**`FileEditor::saveAs` is DONE** (view-triggered FileDialog seam): `cmSaveAs` /
untitled `cmSave` open a `FileDialog` via `Deferred::OpenSaveAsDialog` →
`ModalCompletion::SaveAsPick`; the completion sets `file_name` + re-injects `cmSave`,
which saves and broadcasts `cmUpdateTitle` to refresh the `EditWindow` frame title
(`Window::set_title`). See the IMPLEMENTATION-LOG entry. **Accept test is
`!= CANCEL` (FD_OK_BUTTON ends with `cmFileOpen`, not `cmOK`).** New reusable hatch:
`widgets::editor_mut(&mut dyn View) -> Option<&mut Editor>` peels a `FileEditor`
(whose own `as_any_mut` now returns the `FileEditor`) or a plain `Editor`/`Memo` to
the inner `Editor` — the editor brokers (`SyncEditorDelta`/`EditorPaste`) go through
it.

**Editor seam leftovers (still open, latent):**
- **cmQuit veto.** `valid_end`'s app-quit path *vetoes* close of a modified
  `FileEditor` **without a prompt** (the orphaned box is dropped, not leaked). C++
  prompts on quit; doing so needs a **whole-tree inline drive** (every modified
  editor prompts), not the single-id `validate_modal_close`. Deferred — **latent**
  (no runnable app wires a `FileEditor` yet); the fix is a whole-tree analogue of
  `validate_modal_close`. *(Cheap interim if a quit prompt is wanted sooner: gate
  `FileEditor::valid`'s prompt to `cmd == cmClose` so cmQuit reverts to allow-close.)*
- **saveAs modified-close path.** `valid()` (cmClose → Yes → untitled `save()`)
  vetoes the close, then the saveAs dialog opens *separately* (the deferred fires
  next pump). A full fix needs `validate_modal_close` to drive an
  `OpenSaveAsDialog` inline (the §6 modal-close twin). Breadcrumbed in `save()`.
- **Still breadcrumbed:** `edReadError` on **load** (the ctor has no `ctx`) remains.

### Other non-gating seam
- **The `ModalFrame` deliver-outside-to-modal seam** (row 56/57 — **DONE**, HEAD
  `af109fc`/`95ba912`). Outside-bounds positional events are now delivered to the
  active modal view (localized) instead of being swallowed. `HistoryWindow::handle_event`
  part (C) is implemented: `!mouseInView → endModal(cmCancel)`. Plain `Dialog` ignores
  outside clicks (no cancel override). Key seams added: `CaptureHandler::is_modal_gate()`
  default false, `ModalFrame` overrides true; `CaptureStack::top_modal_view()`;
  pump pre-dispatch redirect block (before `captures.dispatch`).

**Row 66 deferred sub-features** (breadcrumbed TODOs in `editor.rs`; pick up when
relevant prerequisites land):
1. **Find/Replace dialogs** (`editorDialog`, `find()`/`replace()`/`efPromptOnReplace`)
   — `search()` is live; `cmFind`/`cmReplace` are no-ops until the std dialog views exist.
2. **Mouse drag-select/edge-scroll/wheel/middle-button pan** — single-click
   positioning is live; the `while(mouseEvent)` drag loops need a `DragCapture`
   handler (precedent: `window.rs DragCapture`; also deferred for scrollbar, `TODO(row 31)`).
3. **Right-click context menu** (`initContextMenu` + `popupMenu`).
4. **Internal-clipboard `TEditor` branch** (`insertFrom` from a sibling editor) —
   STILL deferred (row 69 `EditWindow` landed but does **not** wire a clipboard
   editor; that needs a dedicated clipboard `EditWindow` + the `insertFrom` branch).
   `EditWindow::close`'s `isClipboard→hide` branch is breadcrumbed for it.
5. `TStreamable` write/read/build (D12).

After the color-picker extension, the faithful port resumes at **row 88**
(`TNode` / the outline family) — the color cluster (81–87) is dropped (see
*Current state*). `cmDosShell` is still deferred — needs a backend
terminal-suspend seam + SIGTSTP.

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

- **✅ RESOLVED (backlog A1, `faabc78`): `CommandSet` allowlist → denylist.**
  Enablement is now the faithful `initCommands` denylist (everything enabled,
  5-command seed); the bandaid is deleted; `Context::command_enabled` exists
  for B1 graying. See `docs/design/command-enablement.md`.
- **🔴 missing `show()→resetCurrent` cascade at insert (architecture pass).**
  C++ `TView::setState(sfVisible)` calls `owner->resetCurrent()` for an
  `ofSelectable` view, so inserting a selectable child establishes the group's
  `current` automatically (an `EditWindow`'s `FileEditor` becomes current at
  construction). rstv's `Group::insert` is deliberately ctx-less and **skips**
  this, so every focus-establishing path must remember to call `reset_current`
  itself. This is now compensated in **three** places — `exec_view`
  (`program.rs`), `HistoryWindow::select_child` (`history.rs`), and
  `Desktop::insert_and_focus` (`desktop.rs`, added when an inserted EditWindow
  opened keyboard-dead: typing + Save routed to a `current == None`). Same
  "every caller must remember the central thing" smell as the CommandSet one.
  **Fix to consider:** establish currency at insert/show time (a ctx-taking
  insert, or wiring the `set_visible` deferred to run `resetCurrent`) so the
  per-caller compensations collapse. See memory `show-resetcurrent-cascade-gap`.
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
