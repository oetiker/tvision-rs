# TEditor  (guide pp. 421–430)

Rust module(s): src/widgets/editor.rs (`Editor`, plus the `Memo`/`FileEditor` embed-delegate wrappers)   |   magiblot: include/tvision/editors.h + source/tvision/teditor.cpp, teditor1.cpp, teditor2.cpp

> `TEditor` is the largest single widget in the guide: a 64K gap-buffer editor
> view with mouse text selection, single-level undo, clipboard cut/copy/paste,
> insert/overwrite + auto-indent modes, key binding, and search/replace. The
> Rust port keeps the gap-buffer core **context-free** (logical edit methods only
> set redraw flags) and threads `&mut Context` only through the event/flush
> boundary (the central seam documented in the module header). All cross-view
> wiring (scroll bars, indicator, clipboard editor) is by `ViewId` brokered by
> the pump (deviation D3); byte stepping becomes grapheme-aware (deviation D13).
>
> **Standing deferrals (commented, intentional — `OK`, not `SUSPECT`):**
> editor.rs:~944 `set_buf_size` shrink path (memory not reclaimed; logical text
> unaffected); editor.rs:~952 OOM return (`Vec` growth is infallible);
> editor.rs charScan/scanCode single-byte fallback (grapheme stepping replaces
> the fixed char-stack scan). The `widgets::editor_mut` hatch peels
> `FileEditor`/`Memo` to the inner `Editor`.

## Fields (pp. 421–424)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `AutoIndent` (field) | 422 | PORTED | OK | `Editor.auto_indent: bool` | N/A | private. Default `true` — **matches magiblot** (`teditor1.cpp` ctor `autoIndent( True )`); the source of truth is the C++ port, not the prose 1992 default. Toggled by `cmIndentMode` (`teditor1.cpp:723`); used by `cmNewLine`/`cmLineStart` (`indentedLineStart`). Faithful. |
| `Buffer` (field) | 422 | EQUIVALENT | OK | `Editor.buffer: Vec<u8>` (gap buffer) | N/A | private. `PEditBuffer` → owned `Vec<u8>` split by gap; see TEditBuffer.md. |
| `BufLen` (field) | 422 | PORTED | OK | `Editor.buf_len: usize` (+ `buf_len()` accessor) | 3 | private field; public read-only accessor `buf_len()` documents "logical text length". Guide: chars between start of buffer and cursor — matches (logical length up to gap). |
| `BufSize` (field) | 422 | PORTED | OK | `Editor.buf_size: usize` | N/A | private. Physical capacity in bytes; invariant `buf_len + gap_len == buf_size`. |
| `CanUndo` (field) | 422 | PORTED | OK | `Editor.can_undo: bool` | N/A | private. Default `true` (matches `TEditor::Init` sets `canUndo = True`). |
| `CurPos` (field) | 422 | PORTED | OK | `Editor.cur_pos: Point` (+ `cur_pos()` accessor) | 3 | private field; public accessor. `(col, row)` display position of the cursor. Matches CurPos.X=col, CurPos.Y=line. |
| `CurPtr` (field) | 422 | PORTED | OK | `Editor.cur_ptr: usize` (+ `cur_ptr()` accessor) | 3 | private; public accessor "cursor logical offset". The gap sits physically at `cur_ptr`. Matches. |
| `DelCount` (field) | 422 | PORTED | OK | `Editor.del_count: usize` | N/A | private. Bytes deleted into the gap tail since the last undo checkpoint. Matches DelCount used by Undo. |
| `Delta` (field) | 423 | PORTED | OK | `Editor.delta: Point` (+ `delta()` accessor) | 3 | private; public accessor "scroll offset (viewport top-left)". Delta.X leftmost col, Delta.Y topmost line. Matches. |
| `DrawLine` (field) | 423 | PORTED | OK | `Editor.draw_line: i32` | N/A | private. Display row that `draw_ptr` corresponds to (the `draw` cache). Matches the C++ draw optimization use. |
| `DrawPtr` (field) | 423 | PORTED | OK | `Editor.draw_ptr: usize` | N/A | private. Logical offset of the start of line `draw_line`. Matches. |
| `GapLen` (field) | 423 | PORTED | OK | `Editor.gap_len: usize` | N/A | private. Gap size between text-before-cursor and text-after-cursor. Matches. |
| `HScrollBar` (field) | 423 | EQUIVALENT | OK | `Editor.h_scroll_bar: Option<ViewId>` | N/A | private. `PScrollBar` up-pointer → `Option<ViewId>` handle, brokered by the pump (deviation D3). `None` = no bar (the C++ `nil`). |
| `Indicator` (field) | 423 | EQUIVALENT | OK | `Editor.indicator: Option<ViewId>` | N/A | private. `PIndicator` → `Option<ViewId>`; updated via `ctx.set_indicator_value` in `do_update`. |
| `InsCount` (field) | 423 | PORTED | OK | `Editor.ins_count: usize` | N/A | private. Bytes inserted since last cursor move (undo accounting). Matches. |
| `IsValid` (field) | 423 | PORTED | OK | `Editor.is_valid: bool` | N/A | private; surfaced through `valid()`. True if the buffer allocated; matches `Valid` reading `isValid`. |
| `Limit` (field) | 423 | PORTED | OK | `Editor.limit: Point` (+ `limit()` accessor) | 3 | private; public accessor "(max line length, line count)". Limit.X longest line (fixed at `MAX_LINE_LENGTH`=256, see note), Limit.Y line count. **Note**: C++ tracks the true longest-line length; Rust pins `limit.x = MAX_LINE_LENGTH` (256) in `set_buf_len`/ctor rather than measuring, which matches magiblot's own behavior (`maxLineLength` constant; horizontal extent is a fixed max, not measured). OK. |
| `Modified` (field) | 423 | PORTED | OK | `Editor.modified: bool` (+ `modified()` accessor) | 3 | private; public accessor. Set in `insert_buffer` only when `!is_clipboard` (matches C++ `if (isClipboard()==False) modified=True`); cleared by `clear_modified` (save tail). Matches. |
| `Overwrite` (field) | 424 | PORTED | OK | `Editor.overwrite: bool` | N/A | private. Insert vs overwrite; toggled by `toggle_ins_mode` (also flips the block cursor). Default `false` (insert). Matches. |
| `Selecting` (field) | 424 | PORTED | OK | `Editor.selecting: bool` | N/A | private. Persistent block-select in progress (between `startSelect` and end). Matches; set/cleared in `start_select`/`hide_select`/`insert_buffer`/`clip_copy`. |
| `SelEnd` (field) | 424 | PORTED | OK | `Editor.sel_end: usize` | N/A | private. End of selection (logical offset). Matches. |
| `SelStart` (field) | 424 | PORTED | OK | `Editor.sel_start: usize` | N/A | private. Start of selection (logical offset). Matches. |
| `VScrollBar` (field) | 424 | EQUIVALENT | OK | `Editor.v_scroll_bar: Option<ViewId>` | N/A | private. As `HScrollBar`. |

## Methods (pp. 424–428)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Init` (constructor) | 424 | PORTED | OK | `Editor::new(bounds, h_scroll_bar, v_scroll_bar, indicator, buf_size)` (+ `new_file_editor`) | 3 | Sets GrowMode hi_x+hi_y, Options selectable, shows cursor, EventMask (mouse+key+command+broadcast implied by the handled arms), wires scroll bars/indicator/bufSize, sets CanUndo=true, allocates the buffer (no `EditorDialog` OOM box — `Vec` alloc; OOM is a standing deferral), and calls `set_buf_len(0)`. Faithful. Public, well-documented. |
| `Load` (constructor) | 424 | NOT-PORTED | — | — | — | Stream constructor. `TStreamable`/streams dropped project-wide (idiomatic mapping: serde-if-revived). No analog. |
| `Done` (destructor) | 425 | NOT-PORTED | — | — | — | C++ destructor frees the buffer via `DoneBuffer` + `TView::Done`. Rust uses `Drop`/ownership: the `Vec` frees itself; no explicit destructor needed (idiomatic mapping). |
| `BufChar` (method) | 425 | PORTED | OK | `Editor::buf_char(p) -> u8` | N/A | private. Returns the byte at logical offset `p` (via `buf_ptr`). Matches "Pth character". |
| `BufPtr` (method) | 425 | PORTED | OK | `Editor::buf_ptr(p) -> usize` | N/A | private. Physical index of logical offset `p`, accounting for the gap. Matches. |
| `ChangeBounds` (method) | 425 | PORTED | OK | `Editor::change_bounds` (impl `View::change_bounds`) | 3 | Sets bounds, clamps `delta` to the content extent, flags a full redraw; scroll-bar params republish on next flush. Matches "adjust Delta so text stays visible, redraw". |
| `ClipCopy` (Edit cmd) | (Ch.15) | PORTED | OK | `Editor::clip_copy(ctx) -> bool` | N/A | private. Internal-clipboard path (hands selection bytes to the clipboard editor by id) OR system-clipboard path (`ctx.set_clipboard`); refuses to copy the clipboard editor from itself. Selection bytes are contiguous (gap is at an endpoint). Idiomatic mapping: TClipboard chain via `Context`/`src/backend/clipboard.rs`. |
| `ClipCut` (Edit cmd) | (Ch.15) | PORTED | OK | `Editor::clip_cut(ctx)` | N/A | private. `clip_copy` then `delete_select` on success. Matches `ClipCut`. |
| `ClipPaste` (Edit cmd) | (Ch.15) | PORTED | OK | `Editor::clip_paste(ctx)` | N/A | private. Internal path = `clipboard_editor_paste` broker (`insertFrom(clipboard)`); system path = `editor_paste` broker. Matches. |
| `ConvertEvent` (method) | 425 | PORTED | OK | `Editor::convert_event(&mut ev)` | N/A | private. Resolves the global keymap incl. the Ctrl-K/Ctrl-Q two-key prefix (a `pending: Option<KeyStroke>` replaces the C++ keyState/prefix machine), rewriting a `KeyDown` into a `Command` in place or clearing it for a prefix. Matches "key binding remap; override to extend". Key-binding tables live in `src/keymap.rs` (the C++ `TEditor::Init` static key maps). |
| `CursorVisible` (method) | 425 | PORTED | OK | `Editor::cursor_visible() -> bool` | N/A | private. `cur_pos.y` within `[delta.y, delta.y+size.y)`. Matches. |
| `DeleteSelect` (method) | 425 | PORTED | OK | `Editor::delete_select()` | N/A | private. `insert_buffer(&[],0,0,can_undo,false)` — deletes the selection. Matches. |
| `DoneBuffer` (method) | 425 | NOT-PORTED | — | — | — | Frees the edit buffer and nils `Buffer`. The `Vec` owns its storage and frees on drop; no explicit deallocator (idiomatic mapping: DOS/manual memory → ownership). |
| `Draw` (method) | 425 | PORTED | OK | `Editor::draw` (impl `View::draw`) | 3 | Recomputes `draw_ptr` for the current `delta.y` (via `line_move`), caches `abs_origin` for mouse tracking, then `draw_lines` renders the viewport rows honoring `delta` and horizontal scroll. Matches "draw lines within bounds, accounting for Delta". |
| `GetPalette` (method) | 425 | EQUIVALENT | OK | (no explicit getter; colors via `color_at` → `Role::ScrollerNormal`/`Role::ScrollerSelected`, theme) | N/A | C++ returns `CEditor` (2 entries → window-palette slots 6,7: Normal, Highlight). Rust folds this into the `Theme` role system: `color_at(p)` returns `ScrollerSelected` inside the selection, else `ScrollerNormal`. Known idiomatic mapping: class Palette → `tv::Theme` (D7). The two CEditor slots (normal/highlight i.e. selected) map to the two Role variants. No standalone `palette()` method (the editor reuses the scroller color roles — see the Memo doc "carries no separate palette"). |
| `HandleEvent` (method) | 426 | PORTED | OK | `Editor::handle_event` (impl `View::handle_event`) | 3 | Calls `convert_event` (the C++ `convertEvent` after `TView::handleEvent`), then dispatches: mouse text selection (drag-select do/while → tracked `MouseDown`/`Move`/`Auto`/`Up` arms via the capture stack, deviation D3), middle-button pan (`evMouse` loop → `Pan` track), right-button context menu (a modern extension — Cut/Copy/Paste/Undo popup, not in 1992; faithful-port-plus, OK), key char insert/overwrite, command dispatch (`handle_edit_command`), and scroll-bar-changed broadcast (re-sync `delta` via `request_sync_editor_delta`, left live per convention). Bracketed `Paste` event handled. All four guide bullet categories (Mouse/Key/Command/Broadcast) covered. |
| `InitBuffer` (method) | 426 | EQUIVALENT | OK | (inlined into `Editor::new`: `vec![0u8; buf_size]`) | N/A | C++ `InitBuffer` calls `MemAlloc(BufSize)`. Rust allocates the `Vec` in the constructor; no separate overridable `InitBuffer` (TMemo's override-of-InitBuffer-for-a-bigger-buffer is instead a `buf_size` ctor argument). Idiomatic. |
| `InsertBuffer` (method) | 426 | PORTED | OK | `Editor::insert_buffer(p, offset, length, allow_undo, select_text) -> bool` | N/A | private. THE core insert/replace: deletes the selection, records undo info when `allow_undo`, converts line endings, grows the buffer (file-editor mode) or returns false (OOM → collapse selection), updates cursor/limit/delta, selects the inserted text when `select_text`, sets `modified` (unless clipboard). Faithful to the teditor.cpp `insertBuffer` arithmetic (the `delLen`/`insCount`/gap-move steps are commented against the C++ memmoves). Caller must snapshot `p` (no aliasing). |
| `InsertFrom` (method) | 426 | PORTED | OK | `Editor::insert_from(data, ctx) -> bool` | N/A | `pub(crate)`. Inserts another editor's selection bytes (clipboard cut/copy/paste brokers). Selects only when self is the clipboard editor. Then `flush_if_unlocked`. Matches "insert selected text from Editor via InsertBuffer" (the `Editor*` arg becomes brokered selection bytes — D3). |
| `InsertText` (method) | 426 | PORTED | OK | `Editor::insert_text(text, select_text, ctx)` (public, ctx-taking) + `insert_text_core` (ctx-free) | 3 | C++ `InsertText` copies Length bytes from Text, selecting if SelectText. Rust splits into `insert_text_core` (the ctx-free `insert_buffer` call) and the public `insert_text` that locks, inserts, tracks the cursor, and unlocks/flushes — the context-threading split (deviation, documented in the module seam). Faithful. |
| `ScrollTo` (method) | 426 | PORTED | OK | `Editor::scroll_to(x, y)` | N/A | private. Clamps `(x,y)` to the content extent and flags a full redraw if changed. Matches "move column X / line Y to upper-left, redraw as needed". |
| `Search` (method) | 427 | PORTED | OK | `Editor::search(needle: &str, opts: u16) -> bool` (public) | 3 | Searches from `cur_ptr`; `efCaseSensitive`/`efWholeWordsOnly` honored (`EF_*` consts); on a hit selects the match and tracks (centering) the cursor; else false. Materializes the post-cursor logical bytes across the gap before scanning (`read_chunk`). Matches. Well-documented (it is the public search primitive driving the Find/Replace dialogs). |
| `SetBufSize` (method) | 427 | PORTED | OK | `Editor::set_buf_size(new_size) -> bool` | N/A | private. C++: returns whether the buffer *can* be resized (the actual resize is `SetBufferSize`). Rust merges them: returns `true` if `new_size` already fits; in **file-editor mode** it actually grows the `Vec` (round up to 0x1000, move the gap tail). Plain editor/memo return false on a request to grow. The "can/does" merge and the shrink no-op (memory not reclaimed) are the documented standing deferrals — OK. |
| `SetSelect` (method) | 427 | PORTED | OK | `Editor::set_select(new_start, new_end, cur_start)` | N/A | private. Moves the gap to the chosen endpoint (the load-bearing op), updates `cur_pos`/`draw_ptr`/`draw_line`, sets the selection, and flags `UF_VIEW`/`UF_UPDATE` per the change. The redraw-decision predicate matches the C++ `(newStart!=selStart||newEnd!=selEnd) && (newStart!=newEnd || selStart!=selEnd)`. Matches "place cursor at start of block if CurStart else end". |
| `SetState` (method) | 427 | PORTED | OK | `Editor::set_state` (impl `View::set_state`) | 3 | Sets the flag, then on `Active` show/hide the scroll bars + indicator and `update_commands`; on `Focused` broadcast received/released focus. Matches "TView::setState then show/hide associated views, then updateCommands". The C++ comment "override UpdateCommands not SetState" is honored — extension goes in `update_commands`/`FileEditor`. |
| `Store` (method) | 427 | NOT-PORTED | — | — | — | Stream serialization. Dropped project-wide. |
| `TrackCursor` (method) | 427 | PORTED | OK | `Editor::track_cursor(center)` | N/A | private. Scrolls so the cursor is visible; centers the line when `center`. Matches the C++ `scrollTo` clamp expressions exactly. |
| `Undo` (method) | 428 | PORTED | OK | `Editor::undo()` | N/A | private. Single-level: selects `[cur_ptr-ins_count, cur_ptr]`, restores `del_count` bytes from the gap tail (`insert_buffer(..., allow_undo=false, select=true)`). Matches "restore to last cursor movement". |
| `UpdateCommands` (method) | 428 | PORTED | OK | `Editor::update_commands(ctx)` | N/A | private. Enables `cmUndo` iff edits since last move; cut/copy gated on selection; paste gated on clipboard editor/selection; `cmClear` on selection; `cmFind`/`cmReplace`/`cmSearchAgain` always; file-editor adds `cmSave`/`cmSaveAs`. Verified line-for-line against teditor2.cpp:625–636 (+ tfiledtr.cpp:257–261). The clipboard editor skips cut/copy/paste (not a user file). Matches. |
| `Valid` (method) | 428 | PORTED | OK | `Editor::valid` (impl `View::valid`) | 3 | Ignores the command and returns `is_valid` (false only if buffer alloc failed). Matches. FileEditor overrides to run the modified-save Yes/No/Cancel prompt (TFileEditor::valid). |
| `SetCmdState` (helper) | (impl) | PORTED | OK | `Editor::set_cmd_state(command, enable, ctx)` | N/A | private. Enables `command` iff `enable && active`, else disables — matches teditor2.cpp:449 (`enableCommands`/`disableCommands` gated by active). Maps to the D-rule deferred enable/disable-command effects. |
| `StartSelect` (Edit cmd) | (Ch.15) | PORTED | OK | `Editor::start_select()` | N/A | private. `hide_select` then `selecting = true`. Matches the `cmStartSelect` handler. |
| Edit-menu / cursor commands (`convertEvent` → cmCharLeft/Right, cmWordLeft/Right, cmLineStart/End, cmLineUp/Down, cmPageUp/Down, cmTextStart/End, cmNewLine, cmBackSpace, cmDelChar, cmDelWord, cmDelStart/End, cmDelLine, cmInsMode, cmStartSelect, cmHideSelect, cmIndentMode, cmUndo, cmCut/Copy/Paste/Clear, cmFind/Replace/SearchAgain) | 426 | PORTED | OK | `Editor::handle_edit_command(cmd, select_mode, ctx)` (+ FIND/REPLACE/SEARCH_AGAIN/ENCODING arms in `handle_event`) | N/A | private. The full command dispatch from `TEditor::handleEvent`'s big `switch`. Navigation commands call `set_cur_ptr(..., select_mode)`; delete commands call `delete_range`; mode toggles flip `overwrite`/`auto_indent`. `cmSelectAll` (a modern addition) and `cmEncoding` (single-byte toggle, magiblot extension) present. FIND/REPLACE open async dialogs; SEARCH_AGAIN runs `do_search_replace`. Matches magiblot's command set including its post-1992 additions. |
| `doSearchReplace` (helper) | (Ch.15) | PORTED | OK | `Editor::do_search_replace(ctx)` | N/A | private. The Find/Replace driver. The "Replace this occurrence?" prompt is **async** (`request_message_box` + `pending_replace_answer` cached via `set_modal_answer`, re-injected as `SEARCH_AGAIN`) instead of a blocking dialog — a deliberate, documented deviation for the single-event-loop model (D9). Loop/exit conditions for `efReplaceAll`/`efPromptOnReplace`/cmNo/cmCancel match the C++ `while (i != cmCancel && efReplaceAll)`. "Search string not found" box suppressed during a no-prompt replace-all, matching C++. |
| `getMousePtr` (helper) | (impl) | PORTED | OK | `Editor::get_mouse_ptr(mouse_local) -> usize` | N/A | private. Clamps the view-local mouse to the view, maps to a logical offset via `line_move`+`char_ptr`. Matches `getMousePtr`. |
| `toggleInsMode` (helper) | (impl) | PORTED | OK | `Editor::toggle_ins_mode()` | N/A | private. Flips `overwrite` and the block-cursor (`cursor_ins`). Matches `cmInsMode` behavior. |

## Palette (p. 428)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `CEditor` palette (2 entries: Normal, Highlight) | 428 | EQUIVALENT | OK | `Role::ScrollerNormal`, `Role::ScrollerSelected` (via `color_at`) | N/A | Maps onto window-palette slots 6,7. Rust folds the two editor colors into the existing scroller role pair (normal text / selected text); selected by `color_at` at draw time. Known idiomatic mapping: class Palette → `tv::Theme` (D7). Memo/FileEditor carry no separate palette (documented). |

## Related types in the guide range (pp. 421, 428–430)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TEditBuffer` type | 421 | EQUIVALENT | OK | `Editor.buffer: Vec<u8>` | N/A | Covered in TEditBuffer.md. |
| `TEditorDialog` type (procedural type) | 428 | EQUIVALENT | OK | `Context::request_message_box` / `open_find_dialog` / `open_replace_dialog` / `request_save_as_dialog` | N/A | C++ uses a global `EditorDialog` function pointer dispatching on `edXXXX` constants (the app supplies the dialog). Rust replaces the function-pointer indirection with typed `Context` request seams that the pump fulfills (the dialogs are app-independent in the framework). Idiomatic; no global mutable function pointer. |
| `edXXXX` constants (edOutOfMemory, edReadError, edWriteError, edCreateError, edSaveModify, edSaveUntitled, edSaveAs, edFind, edSearchFailed, edReplace, edReplacePrompt) | 429–430 | EQUIVALENT | OK | typed `Context` requests + `MessageBoxKind`/`MessageBoxButtons` + FileEditor flow | N/A | The dialog-selector enum becomes concrete typed request methods. edOutOfMemory has no analog (infallible `Vec`, standing deferral). edReadError → `pending_load_error` box; edWrite/edCreateError → one "Error writing file" box (`std::fs::write` does not distinguish, documented); edSaveModify/edSaveUntitled → FileEditor `valid()` Yes/No/Cancel; edSaveAs → `request_save_as_dialog`; edFind/edReplace → open_find/replace_dialog; edSearchFailed → "Search string not found"; edReplacePrompt → async "Replace this occurrence?". All behaviors present; the *registry-of-constants* shape is intentionally dropped. |
| `TEditWindow` object | 430 | PORTED | OK | `EditWindow` (src/widgets/editor.rs / windows module) | — | The hosting window (file title, auto scroll bars + indicator). Embeds a `FileEditor`; updates its frame title on the save broadcast. Out of strict TEditor scope but in the guide pages; present and tested (`edit_window_*` tests). |

## Summary

- PORTED: 50   EQUIVALENT: 10   NOT-PORTED: 4   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0 (all below-bar accessors and View impl methods raised to 3)   |   → concept: 0
- Notable findings: **No missing or suspect behavior** — the entire TEditor
  surface (gap buffer, undo, selection, clipboard, search/replace, file editor,
  edit-menu command set incl. magiblot's post-1992 additions) is ported and the
  hot paths are commented against the C++ memmove arithmetic. The single most
  important design point is the **context-threading split**: the gap-buffer edit
  core is context-free (methods only OR redraw flags), and `&mut Context` is
  threaded only at the event/flush boundary — this is what lets the editor live
  in the single-event-loop model and unit-test the buffer in isolation, and it
  reshapes `InsertText`, `doSearchReplace`'s replace-prompt (now async via a
  cached modal answer), and all sibling/clipboard wiring into `ViewId`-brokered
  deferred effects (D3/D9). Every field/method default was checked against
  magiblot (the source of truth), including `auto_indent`/`can_undo` = true —
  both match the C++ ctor. No SUSPECT items.
