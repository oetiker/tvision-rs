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

- **HEAD = row 75 `TDirListBox` (`DirListBox`) + deviation D14 (native Linux `/`
  paths), landed this session — see the IMPLEMENTATION-LOG top section.** Build:
  **810 lib tests** green; `cargo clippy --workspace --all-targets -- -D warnings`
  and `cargo fmt --all --check` clean (verify clippy with a forced re-lint — a
  cached run can mask a fresh warning).
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

## Next — resume PORT-ORDER at row 76 `TFileList`

**Row 75 `TDirListBox` is DONE** (landed this session; D14 + the design cycle in
the IMPLEMENTATION-LOG top section). Resume the normal "lowest-numbered incomplete
row" rule → **row 76**.

### The filedlg cluster (rows 76–80) — all inherit deviation D14 (native `/`)
**D14 is now the law for this whole cluster** (PORTING-GUIDE): `/`-separated paths,
root `/`, NO drives/`\`/"Drives" entry; FS reads via `std::fs` (follow symlinks,
like magiblot's `stat`). No `\`↔`/` translation seam anywhere.

- **Row 76 `TFileList`** (`tfillist.cpp`, subclass of `TSortedListBox`): owns the
  row-74 `FileCollection` (`Vec<SearchRec>` + `search_rec_compare`); **reads a
  directory** (the `readDirectory`/`findfirst(FA_DIREC|find-files)` loop → a
  `std::fs::read_dir` populating `SearchRec` — the fs-read layer breadcrumbed at
  rows 73/74 lands here). `getText` formats name + size/date columns; `..` handling.
  Like row 75, expect owner-coupling to `TFileDialog` (`cmFileFocused`/
  `cmFileDoubleClicked` broadcasts) — **breadcrumb the owner messages to row 79** if
  payload-carrying, same pattern as row 75's `select_item`.
  - **⚠ NOT a routine MECHANICAL leaf — the row-70/75 item-source seam comes fully
    due here.** `TFileList` stores `Vec<SearchRec>` with an overridden `getText`, so
    (like row 75) it **cannot be a D2 delegate** over `SortedListBox` — it must be a
    **direct `ListViewer` impl** over `Vec<SearchRec>`. But unlike `DirListBox` it
    *also* needs `SortedListBox`'s **incremental type-to-search**, which today lives
    *inside* `SortedListBox` operating on its inner `ListBox`'s `Vec<String>`. A
    plain direct impl would have to **duplicate** that search. **Before implementing
    76, extract `SortedListBox`'s search** (the `curString` re-seed + binary search,
    `src/widgets/list_box.rs`) into a **free function over the `ListViewer` trait**
    (mirroring `list_viewer::draw`/`handle_event`), so `FileList` is a direct
    `ListViewer` impl over `Vec<SearchRec>` *and* gets search for free. This is the
    deferred "list-viewer item-source seam refactor" — a FOUNDATION sub-step, not a
    leaf. (`get_key`/`shift_state` breadcrumbs in `list_box.rs` were left for this.)
- Then **77 `TFileInputLine`**, **78 `TFileInfoPane`**, **79 `TFileDialog`**
  (assembles 76+77+78+buttons), **80 `TChDirDialog`** (assembles `TInputLine` +
  the row-75 `DirListBox` + buttons).

### Row 80 will need the typed-payload-command seam (row 75 breadcrumbs come due)
Row 75 left two owner-coupling breadcrumbs that **row 80 `TChDirDialog` must
resolve** (it is their only consumer — the seam is designed here, at first use):
1. **`DirListBox::select_item`** must deliver `cmChangeDir` **carrying the chosen
   `DirEntry`** to the owner. rstv's `Event::Broadcast { command, source }` is
   payload-less — design a typed-payload command path (a new `Deferred`/event
   variant carrying the `DirEntry`, brokered by the pump to the owner, OR resolve
   the chosen entry by id at apply-time). Same shape will recur for row 79's
   `TFileList` → `TFileDialog` messages.
2. **`DirListBox::set_state`** must, on `sfFocused` change, `makeDefault(enable)`
   the owner `TChDirDialog`'s `chDirButton` — needs the owner-downcast + button
   default seam.
Both are `TODO(row 80 TChDirDialog)` in `src/dialog/filedlg.rs`.

**Precondition footgun:** `DirListBox::build_tree`/`new_directory` assume `dir`
ends with `/` (the ancestor walk is robust to a missing slash, but subdir nav
paths silently malform — `/home/oetikerprojects` — without it). When row 80 wires
the caller, either guarantee the trailing `/` or add the 1-line normalize at the
top of `new_directory`: `let dir = if dir.ends_with('/') { dir.to_string() } else
{ format!("{dir}/") };`.

Once **`TFileDialog` (row 79)** lands it **un-blocks** `FileEditor::saveAs` and
`EditWindow`'s dynamic-title (`cmUpdateTitle`) path.

**Editor seam leftovers (still open, latent):**
- **cmQuit veto.** `valid_end`'s app-quit path *vetoes* close of a modified
  `FileEditor` **without a prompt** (the orphaned box is dropped, not leaked). C++
  prompts on quit; doing so needs a **whole-tree inline drive** (every modified
  editor prompts), not the single-id `validate_modal_close`. Deferred — **latent**
  (no runnable app wires a `FileEditor` yet); the fix is a whole-tree analogue of
  `validate_modal_close`. *(Cheap interim if a quit prompt is wanted sooner: gate
  `FileEditor::valid`'s prompt to `cmd == cmClose` so cmQuit reverts to allow-close.)*
- **Still breadcrumbed:** `saveAs`/`edSaveAs` (needs `TFileDialog`, row 79),
  `edReadError` on **load** (the ctor has no `ctx`).

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

Phase 5 then continues in PORT-ORDER with **75** (`TDirListBox`), then
`TFileList`/`TFileDialog`/`TChDirDialog` and the color / outline families.
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
