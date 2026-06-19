# TFileEditor  (guide pp. 438–441)

Rust module(s): src/widgets/editor.rs (`struct FileEditor`)   |   magiblot: include/tvision/editors.h / source/tvision/tfiledtr.cpp

> TFileEditor subclasses TEditor, adding a `fileName[MAXPATH]` field and file-IO methods. It
> overrides buffer management (`initBuffer`/`doneBuffer`/`setBufSize`) to use heap-allocated
> growable memory (vs the fixed-size base), and adds `loadFile`, `save`, `saveAs`, `saveFile`,
> `updateCommands`, and `valid`. The palette is the same 2-entry editor palette (inherited).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `fileName` (field) | 438 | EQUIVALENT | OK | `tv::FileEditor::file_name: Option<PathBuf>` | 3 | Raised: doc now explains `None` = untitled, when it is updated (construction + save-as completion), and advises against direct mutation. |
| `Init` (constructor) | 438 | PORTED | OK | `tv::FileEditor::new(bounds, h_scroll_bar, v_scroll_bar, indicator, file_name)` | 3 | Raised: doc now covers None=untitled vs path=load-immediately, the path-absolutization step, the missing-file behavior, and the deferred-error pattern. Directs callers to prefer `EditWindow::new` for the typical use case. |
| `doneBuffer` (method) | 438 | NOT-PORTED | — | — | — | C++: `free(buffer)` — explicit heap deallocation. Rust: `Vec<u8>` in `Editor` drops automatically. The Rust allocator handles this; no public method needed. |
| `initBuffer` (method) | 438 | NOT-PORTED | — | — | — | C++: `buffer = (char*) malloc(bufSize)` — explicit heap allocation at construction. Rust: `Editor::new` allocates `Vec<u8>` at initialization; the file-editor path starts with size 0 and grows on demand. No separate method needed. |
| `loadFile` (method) | 439 | PORTED | OK | `tv::FileEditor::load_file(&mut self) -> bool` | 3 | Raised: doc now covers the missing-file=empty-buffer contract, the deferred-error pattern, the infallible growth, and when to call it. |
| `save` (method) | 439 | PORTED | OK | `tv::FileEditor::save(&mut self, ctx) -> bool` | 3 | Raised: doc now explains the titled vs untitled branch, the async save-as round-trip, what `false` means (deferred or write failure), and directs callers to prefer the command path over direct calls. |
| `saveAs` (method) | 440 | EQUIVALENT | OK | `tv::FileEditor::handle_event` `cmSaveAs` arm → `ctx.request_save_as_dialog` + `pending_title_update` | 3 | Maps to `handle_event` (raised below). The async seam, pending_title_update flag, and completion round-trip are documented there. |
| `saveFile` (method) | 440 | PORTED | OK | `tv::FileEditor::save_file(&mut self, ctx) -> bool` | 3 | Raised: doc now explains the backup-file rename, the unified write-error dialog, and advises callers to use `save` instead. |
| `setCmdState` (method) | 440 | PORTED | OK | `tv::Editor::set_cmd_state(&self, command, enable, ctx)` | N/A | Private `fn` (no `pub`). Internal helper that gates enable/disable on `state.active`; internal comment is adequate. Not a public API — does not count toward score-3 closure. |
| `updateCommands` (method) | 440 | PORTED | OK | `tv::Editor::update_commands(&self, ctx)` | N/A | Private `fn` (no `pub`). Grays/ungrays editing commands; gated by `file_editor` flag for Save/Save-As. Internal comment is adequate. Not a public API — does not count toward score-3 closure. |
| `valid` (method) | 440 | PORTED | OK | `tv::FileEditor::valid(&mut self, cmd, ctx) -> bool` | 3 | Doc already at score 3: explains the async Yes/No/Cancel round-trip and the `Command::VALID` fast path. |
| `setBufSize` (method, override) | 438 | PORTED | OK | `tv::Editor::set_buf_size(&mut self, new_size) -> bool` | N/A | Private `fn` (no `pub`). Grows the `Vec<u8>` in file-editor mode; inline comments explain the 0x1000 rounding and gap-tail move. Not a public API — does not count toward score-3 closure. |
| `shutDown` (method) | 441 | EQUIVALENT | OK | `Editor::set_state(Active, false, ctx)` path in `View::set_state` | N/A | Private concern; no explicit `shut_down` method. Handled by the `set_state` → `update_commands` path. Not a public API — does not count toward score-3 closure. |
| `getPalette` (inherited, not overridden) | 438 | EQUIVALENT | OK | `Role::ScrollerNormal` / `Role::ScrollerSelected` via `#[delegate(to = editor)]` | N/A | C++ palette comment in editors.h: entries 1=Normal text, 2=Selected text (same as TEditor). `FileEditor` does not override `palette()`; it delegates to `Editor` which uses `Role` lookups. Known idiomatic mapping. |
| `handleEvent` | — | PORTED | OK | `tv::FileEditor::handle_event` | 3 | Raised: doc now explains the three responsibilities (deferred load error, Save-As async flow, Save + flush + title broadcast). |
| `Load` / `Store` (stream) | 441 | NOT-PORTED | — | — | — | `TStreamable` / stream serialization dropped project-wide. Known idiomatic mapping. |

## Summary

- PORTED: 8   EQUIVALENT: 4   NOT-PORTED: 3   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: All previously below-bar **public** symbols raised to score 3. Private helpers (`set_cmd_state`, `set_buf_size`, `update_commands`, `shut_down`) are N/A — private `fn`, no public API. The async-modal pattern (save-as, valid, load-error) is now documented on each method that participates in it. The `saveAs` row maps to `handle_event` (raised together). The `saveAs`-clipboard edge case (C++ clears `fileName` after clipboard save) has no direct Rust equivalent but is inconsequential in practice.
