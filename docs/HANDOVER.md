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

- **DIRECTION CHANGE (2026-06-09): the faithful color-selection cluster
  (PORT-ORDER 81–87, `colordlg`) is DROPPED — superseded by a *modern truecolor
  color-picker* extension.** Why: the C++ `TColorDialog` family exists solely to
  edit `app->palette->data[index]` — the flat runtime-mutable BIOS palette that
  rstv **deleted under D7** (palette → `Theme`; `Role` is a closed enum resolved
  at draw time; `WindowPalette` is only a Blue/Cyan/Gray *tag*, not data). A
  faithful 83–87 would edit a `TPalette` nothing in rstv reads — dead code by
  construction. The user chose to build a reusable truecolor color-picker instead
  (a runtime **theme editor** becomes a later consumer). **This is an
  rstv-original extension, like `RegexValidator` — off the faithful critical
  path.**
- **The color-picker design is APPROVED** (brainstorm complete, spec committed):
  [`docs/superpowers/specs/2026-06-09-color-picker-design.md`](file:///home/oetiker/checkouts/rstv/docs/superpowers/specs/2026-06-09-color-picker-design.md).
  Summary: one `ColorPicker` view owning a shared `ColorModel` (Approach A,
  surfaces as internal components, NOT separate Views); produces any `Color`
  variant (Default/Bios/Indexed/Rgb); four **tabbed** surfaces (Presets · RGB+hex
  · HSV plane · xterm-256 grid); old/new swatch + variant readout; keyboard +
  **full mouse-drag** (the `window.rs` capture seam + one new
  `Deferred::ColorPickerDrag`); `color_dialog(initial) -> Option<Color>` modal
  entry. **Next step in the brainstorming flow is `writing-plans`** (the impl plan
  was not yet written).
- **HEAD = the design-spec commit, on top of row 82.** The two *faithful* color
  rows landed earlier this session — **row 81** (`ColorItem`/`ColorGroup`/
  `ColorIndex`, `c92ed19`) and **row 82** (`ColorSelector`, `f3c34ad`) — are
  **superseded and will be REVERTED as the first step of the picker work** (the
  picker's Presets surface subsumes the 16-color grid; the palette-bookkeeping
  data classes have no consumer). They are still committed for now. The 3
  `COLOR_*` commands in `command.rs` may be kept or dropped (unused by the
  picker — see spec open items). Build currently: **924 lib tests** green; clippy
  (forced re-lint) + fmt clean.
- **Still valid & reusable from row 82:** **raw-BIOS-color drawing** — the first
  widget to draw colors literally via `Style::new(Color::Bios(..), ..)` rather
  than theme `Role`s. The picker's surfaces draw raw truecolor the same way
  (`Color::Rgb`), and the existing `ColorDepth` quantization ladder degrades it on
  lesser terminals. (The row-82 *color-changed broadcast seam* does NOT carry
  over — the picker is one view, so it needs no cross-view color sync.)
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

## Next — build the color-picker extension, then resume PORT-ORDER at row 88

**Immediate next: `writing-plans`** — turn the approved spec
([`docs/superpowers/specs/2026-06-09-color-picker-design.md`](file:///home/oetiker/checkouts/rstv/docs/superpowers/specs/2026-06-09-color-picker-design.md))
into an implementation plan, then build it **subagent-driven** (the standard
implementer → review → integrate cycle). Read the spec first — it has the full
architecture, module layout (`src/dialog/colorpick/`), the four surfaces, the
mouse-drag capture design, the testing plan, and the **plan-level open items**
(exact `Hsv` repr + rounding for deterministic snapshots, dialog/sub-rect
geometry, the curated preset list, where the BIOS→RGB table lives).

**Step 0 of the picker work: REVERT rows 81–82** — delete `src/dialog/colordlg.rs`
+ its 3 `colordlg` snapshots + the `dialog/mod.rs` exports (and decide on the 3
`COLOR_*` commands). The picker supersedes them. Then mark **PORT-ORDER rows
81–87** as dropped/superseded (a documented D7 consequence, like `TStreamable`),
pointing at the spec. *(PORT-ORDER still shows 81/82 ✅ and 83–87 as pending — it
needs this reconciliation; the user asked only for HANDOVER this turn.)*

**Reusable seams the picker leans on (already built — see the spec):** raw-color
drawing (row 82), the `window.rs` `DragCapture` capture-handler pattern (proven —
you can drag window frames in `examples/hello.rs`), the
"`Deferred` variant + pump downcast via `as_any_mut`" broker shape (scroller/
editor/`MakeButtonDefault`), and the `exec_view_with_completion` gather-closure
(inputBox) for returning the chosen `Color`.

**After the picker lands: resume the faithful port at the lowest incomplete row,
which is now `row 88` (`TNode` / the outline family 88–90)** — 81–87 are dropped,
so 88 is the next faithful work. The terminal family (91–92) follows. A future
**theme editor** (consuming this picker) needs the D7 "Theme extension point"
(runtime `Role→Style` registration) built first — a separate sub-project, not on
the critical path.

**`FileEditor::saveAs` is UNBLOCKED** (row 79 `FileDialog` landed): read the chosen
filename from `FileDialog::value()` → `FieldValue::Text`, as is `EditWindow`'s
dynamic-title (`cmUpdateTitle`) path. Still open (a follow-up, not on the PORT-ORDER
critical path): wire `FileEditor::saveAs`/`edSaveAs` to exec a `FileDialog` and read
its `value()`.

**Editor seam leftovers (still open, latent):**
- **cmQuit veto.** `valid_end`'s app-quit path *vetoes* close of a modified
  `FileEditor` **without a prompt** (the orphaned box is dropped, not leaked). C++
  prompts on quit; doing so needs a **whole-tree inline drive** (every modified
  editor prompts), not the single-id `validate_modal_close`. Deferred — **latent**
  (no runnable app wires a `FileEditor` yet); the fix is a whole-tree analogue of
  `validate_modal_close`. *(Cheap interim if a quit prompt is wanted sooner: gate
  `FileEditor::valid`'s prompt to `cmd == cmClose` so cmQuit reverts to allow-close.)*
- **Still breadcrumbed:** `saveAs`/`edSaveAs` — `TFileDialog` (row 79) has now
  landed, so this is **unblocked** (exec a `FileDialog`, read `value()`); just not
  yet wired. `edReadError` on **load** (the ctor has no `ctx`) remains.

### Other non-gating seam still open (independent of the above)
- **The `ModalFrame` deliver-outside-to-modal seam** (row 56/57 deferred — STILL
  OPEN). Un-defers the `HistoryWindow` outside-click `endModal(cmCancel)`. **NOT a
  `ModalFrame` return-value tweak:** `ModalFrame::handle` has no `group`, and
  `program_handle_event` routes outside positional events **positionally to the
  desktop**. The fix is a **delivery-path change in `program_handle_event`**:
  while a `ModalFrame` is the top capture, deliver positional events to the modal
  view by id (makeLocal to its bounds) so the modal's own routing + the
  `HistoryWindow` `mouseInView`-cancel override decide. Verify a plain `Dialog`
  still IGNORES an outside click under that delivery (C++ does). Breadcrumb in
  place: `HistoryWindow::handle_event` `TODO(row 57 modal-loop seam)`.

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
