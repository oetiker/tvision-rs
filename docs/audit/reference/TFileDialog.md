# TFileDialog  (guide pp. 435–438)

Rust module(s): `src/dialog/filedlg.rs`   |   magiblot: `include/tvision/stddlg.h` / `source/tvision/tfildlg.cpp`

> The 1992 print guide gives only a brief stub for `TFileDialog` (p. 435 sidebar
> and p. 441 stubs for its sub-components). The authoritative field/method
> specification is in `stddlg.h` and `tfildlg.cpp`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `fileName` (field, `TFileInputLine*`) | 435 | EQUIVALENT | OK | `FileDialog.file_name_id: ViewId` | 2 | C++: raw pointer to the child. Rust: `ViewId` handle; child accessed via `dialog.child_mut(file_name_id)`. Known D3 mapping (pointer → ViewId). |
| `wildCard` (field, `char[MAXPATH]`) | 435 | PORTED | OK | `FileDialog.wild_card: String` | 2 | C++: fixed-length DOS path buffer; Rust: owned `String`. Functionally identical. |
| `fileList` (field, `TFileList*`) | 435 | EQUIVALENT | OK | `FileDialog.file_list_id: ViewId` | 2 | C++: raw pointer to child. Rust: `ViewId` handle. Same D3 mapping. |
| `directory` (field, `const char*`) | 435 | PORTED | OK | `FileDialog.directory: String` | 2 | C++: heap `char*` via `newStr`; Rust: owned `String`, `/`-terminated. |
| `getData` (method) | 435 | EQUIVALENT | OK | `FileDialog::value() -> Option<FieldValue::Text>` | 2 | C++ copies the resolved path into a `char[]` buffer. Rust returns `FieldValue::Text(resolved_name)`. D10 value protocol — known idiomatic mapping. `getData` was no-op on `TFileList`; here it routes through `value()`. |
| `getFileName` (method) | 435 | PORTED | OK | `FileDialog::get_file_name(&mut self) -> String` | 3 | Resolves the input-line text against `directory`, splits dir+file, appends wildcard for bare-directory paths. Signature is `&mut self` (needs `child_mut`); refreshes `resolved_name` cache for `value()`. Behaviour fully matches the C++ logic. |
| `handleEvent` (method) | 435 | PORTED | OK | `FileDialog::handle_event` (impl `View::handle_event`) | 3 | Delegates to `Dialog::handle_event` first; then: FILE_OPEN/FILE_REPLACE/FILE_CLEAR → `end_modal`; FILE_DOUBLE_CLICKED broadcast → re-post `Command::OK`. One-time screen-relative resize runs before first child dispatch (no `Context` at construction — same intent as C++ `TProgram` resize). |
| `readDirectory` (method, private) | 435 | EQUIVALENT | OK | `FileDialog::navigate` (private) + `reset_current` | 2 | C++: `readDirectory()` re-reads the file list. Rust splits into `reset_current` (one-time initial read) and `navigate` (subsequent re-reads on wildcard/dir change). Behaviour equivalent. Private; not public API. |
| `setData` (method) | 435 | EQUIVALENT | OK | `FileDialog::set_value(FieldValue::Text)` | 2 | C++ copies a `char[]` into the filename field. Rust forwards `FieldValue::Text` to the `FileInputLine` via D10 value protocol. |
| `valid` (method) | 435 | PORTED | OK | `FileDialog::valid(&mut self, cmd, ctx) -> bool` | 3 | Full navigate/accept gate: VALID → true immediately; group-valid check; wildcard → navigate+false; existing dir → navigate into+false; valid filename → true; else error box+false. Matches C++ logic including the cmFileInit branch (skips focus request). |
| `shutDown` (method) | 435 | NOT-PORTED | — | — | — | C++ virtual destructor shim (`TStreamable` teardown + `delete directory`). Rust: RAII via `Drop`; no DOS heap allocations. |
| `sizeLimits` (method) | 435 | PORTED | OK | `FileDialog::size_limits` | 2 | Sets minimum `{49, 19}` (identical to C++ `{49, 19}`); delegates max to inner dialog. |
| `fdOKButton` (constant) | 436 | PORTED | OK | `pub const FD_OK_BUTTON: u16 = 0x0001` | 2 | Value matches C++ `fdOKButton = 0x0001`. |
| `fdOpenButton` (constant) | 436 | PORTED | OK | `pub const FD_OPEN_BUTTON: u16 = 0x0002` | 2 | Value matches `fdOpenButton = 0x0002`. |
| `fdReplaceButton` (constant) | 436 | PORTED | OK | `pub const FD_REPLACE_BUTTON: u16 = 0x0004` | 2 | Value matches `fdReplaceButton = 0x0004`. |
| `fdClearButton` (constant) | 436 | PORTED | OK | `pub const FD_CLEAR_BUTTON: u16 = 0x0008` | 2 | Value matches `fdClearButton = 0x0008`. |
| `fdHelpButton` (constant) | 436 | PORTED | OK | `pub const FD_HELP_BUTTON: u16 = 0x0010` | 2 | Value matches `fdHelpButton = 0x0010`. |
| `fdNoLoadDir` (constant) | 436 | PORTED | OK | `pub const FD_NO_LOAD_DIR: u16 = 0x0100` | 2 | Value matches `fdNoLoadDir = 0x0100`. |
| `Load` / `Store` constructors | 435 | NOT-PORTED | — | — | — | `TStreamable` persistence — dropped per D12; no serde analog implemented. |
| `checkDirectory` (method, private) | impl | PORTED | OK | `FileDialog::check_directory` (private) | 2 | Pops error box via `ctx.request_message_box`; refocuses filename field. Matches C++ intent; uses async-modal-from-view seam. |
| `button_specs` helper | impl | EQUIVALENT | OK | `fn button_specs(options: u16)` (private fn) | 2 | C++ inline loop in constructor; Rust extracted to a pure function for testability. Same button/command/default/y logic. |
| `needs_screen_resize` field | impl | EQUIVALENT | OK | `FileDialog.needs_screen_resize: bool` | 2 | C++ does the resize in `Init` after `TProgram` is live; Rust defers to first `handle_event` where `ctx.owner_size()` is available. Same intent, different lifecycle seam. |
| `resolved_name` cache field | impl | EQUIVALENT | OK | `FileDialog.resolved_name: String` | 2 | No C++ analog — Rust-specific cache enabling `&self` `value()` after `&mut self` `valid()`. Not a deviation from behavior; purely implementation. |
| `info_pane_id` field | impl | EQUIVALENT | OK | `FileDialog.info_pane_id: ViewId` | 2 | C++ has no explicit pointer (pane is found by iteration); Rust caches the id for O(1) child access. |
| `cmFileOpen` / `cmFileReplace` / `cmFileClear` / `cmFileInit` (constants) | 435 | PORTED | OK | `Command::FILE_OPEN` / `FILE_REPLACE` / `FILE_CLEAR` / `FILE_INIT` | 2 | Integer values differ (Rust uses namespaced string identity per D1), but semantics match. |
| `cmFileFocused` / `cmFileDoubleClicked` (message constants) | 435 | PORTED | OK | `Command::FILE_FOCUSED` / `FILE_DOUBLE_CLICKED` | 2 | Same mapping as above. |

## Summary

- PORTED: 13   EQUIVALENT: 9   NOT-PORTED: 2   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 18   |   → concept: 0
- Notable findings: No missing or suspect items. The two most notable idiomatic deviations — raw `TFileInputLine*`/`TFileList*` pointers becoming `ViewId` handles (D3), and `getData`/`setData` becoming the D10 value protocol — are both deliberate, documented, and correctly implemented. The `Load`/`Store`/`shutDown` trio is cleanly not-ported per D12 with no stubs left behind.
