# TFileDialog  (guide pp. 435–438)

Rust module(s): `src/dialog/filedlg.rs`   |   magiblot: `include/tvision/stddlg.h` / `source/tvision/tfildlg.cpp`

> The 1992 print guide gives only a brief stub for `TFileDialog` (p. 435 sidebar
> and p. 441 stubs for its sub-components). The authoritative field/method
> specification is in `stddlg.h` and `tfildlg.cpp`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `fileName` (field, `TFileInputLine*`) | 435 | EQUIVALENT | OK | `FileDialog.file_name_id: ViewId` | 3 | C++: raw pointer to the child. Rust: `ViewId` handle; child accessed via `dialog.child_mut(file_name_id)`. Known D3 mapping. Private; internal comment updated to note it is read by `get_file_name`/`valid`. |
| `wildCard` (field, `char[MAXPATH]`) | 435 | PORTED | OK | `FileDialog.wild_card: String` | 3 | C++: fixed-length DOS path buffer; Rust: owned `String`. Private; internal comment now describes the push-to-children pattern and the update path via `valid`'s wildcard branch. |
| `fileList` (field, `TFileList*`) | 435 | EQUIVALENT | OK | `FileDialog.file_list_id: ViewId` | 3 | C++: raw pointer to child. Rust: `ViewId` handle. Private; internal comment updated to note it is used by `valid`/`navigate` and `handle_event`. |
| `directory` (field, `const char*`) | 435 | PORTED | OK | `FileDialog.directory: String` | 3 | C++: heap `char*` via `newStr`; Rust: owned `String`, `/`-terminated. Private; internal comment updated to note always-`/`-terminated and who sets/reads it. |
| `getData` (method) | 435 | EQUIVALENT | OK | `FileDialog::value() -> Option<FieldValue::Text>` | 3 | C++ copies the resolved path into a `char[]` buffer. Rust returns `FieldValue::Text(resolved_name)`. Doc now explains how/when callers read it (modal gather, or directly after the modal) and that cancel/file-clear paths also populate the cache. |
| `getFileName` (method) | 435 | PORTED | OK | `FileDialog::get_file_name(&mut self) -> String` | 3 | Resolves the input-line text against `directory`, splits dir+file, appends wildcard for bare-directory paths. Signature is `&mut self` (needs `child_mut`); refreshes `resolved_name` cache for `value()`. Behaviour fully matches the C++ logic. |
| `handleEvent` (method) | 435 | PORTED | OK | `FileDialog::handle_event` (impl `View::handle_event`) | 3 | Delegates to `Dialog::handle_event` first; then: FILE_OPEN/FILE_REPLACE/FILE_CLEAR → `end_modal`; FILE_DOUBLE_CLICKED broadcast → re-post `Command::OK`. One-time screen-relative resize runs before first child dispatch (no `Context` at construction — same intent as C++ `TProgram` resize). |
| `readDirectory` (method, private) | 435 | EQUIVALENT | OK | `FileDialog::navigate` (private) + `reset_current` | 3 | C++: `readDirectory()` re-reads the file list. Rust splits into `reset_current` (one-time initial read; public via View trait, now documented as framework hook + FD_NO_LOAD_DIR note) and `navigate` (private; has good internal comment). |
| `setData` (method) | 435 | EQUIVALENT | OK | `FileDialog::set_value(FieldValue::Text)` | 3 | C++ copies a `char[]` into the filename field. Rust forwards `FieldValue::Text` to the `FileInputLine` via D10 value protocol. Doc now explains when to use (pre-fill before show), the no-Context constraint, and why no initial navigate is triggered. |
| `valid` (method) | 435 | PORTED | OK | `FileDialog::valid(&mut self, cmd, ctx) -> bool` | 3 | Full navigate/accept gate: VALID → true immediately; group-valid check; wildcard → navigate+false; existing dir → navigate into+false; valid filename → true; else error box+false. Matches C++ logic including the cmFileInit branch (skips focus request). |
| `shutDown` (method) | 435 | NOT-PORTED | — | — | — | C++ virtual destructor shim (`TStreamable` teardown + `delete directory`). Rust: RAII via `Drop`; no DOS heap allocations. |
| `sizeLimits` (method) | 435 | PORTED | OK | `FileDialog::size_limits` | 3 | Sets minimum `{49, 19}` (identical to C++ `{49, 19}`); delegates max to inner dialog. Doc now explains WHY the floor (sub-pane positions), HOW it interacts with `calc_bounds` skip, and that callers don't invoke it directly. |
| `fdOKButton` (constant) | 436 | PORTED | OK | `pub const FD_OK_BUTTON: u16 = 0x0001` | 3 | Value matches C++ `fdOKButton = 0x0001`. Doc now explains what action it triggers, how it relates to `FD_OPEN_BUTTON`, and how to pass it to `FileDialog::new`. |
| `fdOpenButton` (constant) | 436 | PORTED | OK | `pub const FD_OPEN_BUTTON: u16 = 0x0002` | 3 | Value matches `fdOpenButton = 0x0002`. Doc now explains how it differs from `FD_OK_BUTTON` (label only) and when to prefer each. |
| `fdReplaceButton` (constant) | 436 | PORTED | OK | `pub const FD_REPLACE_BUTTON: u16 = 0x0004` | 3 | Value matches `fdReplaceButton = 0x0004`. Doc now explains the overwrite-signal semantics and typical use in save-as dialogs. |
| `fdClearButton` (constant) | 436 | PORTED | OK | `pub const FD_CLEAR_BUTTON: u16 = 0x0008` | 3 | Value matches `fdClearButton = 0x0008`. Doc now explains no-path-check behavior, the `FILE_CLEAR` command, and that `value()` returns empty. |
| `fdHelpButton` (constant) | 436 | PORTED | OK | `pub const FD_HELP_BUTTON: u16 = 0x0010` | 3 | Value matches `fdHelpButton = 0x0010`. Doc now explains the button is never default, never triggers validation, and how to wire a `HELP` handler. |
| `fdNoLoadDir` (constant) | 436 | PORTED | OK | `pub const FD_NO_LOAD_DIR: u16 = 0x0100` | 3 | Value matches `fdNoLoadDir = 0x0100`. Doc now explains when to suppress the initial read and the consequence of omitting it. |
| `Load` / `Store` constructors | 435 | NOT-PORTED | — | — | — | `TStreamable` persistence — dropped per D12; no serde analog implemented. |
| `checkDirectory` (method, private) | impl | PORTED | OK | `FileDialog::check_directory` (private) | 3 | Pops error box via `ctx.request_message_box`; refocuses filename field. Matches C++ intent; uses async-modal-from-view seam. Private; has a clear 3-line internal doc. |
| `button_specs` helper | impl | EQUIVALENT | OK | `fn button_specs(options: u16)` (private fn) | 3 | C++ inline loop in constructor; Rust extracted to a pure function for testability. Private; has detailed internal doc explaining the default-chain logic. |
| `needs_screen_resize` field | impl | EQUIVALENT | OK | `FileDialog.needs_screen_resize: bool` | 3 | C++ does the resize in `Init` after `TProgram` is live; Rust defers to first `handle_event` where `ctx.owner_size()` is available. Private; has a 5-line internal doc explaining the lifecycle seam. |
| `resolved_name` cache field | impl | EQUIVALENT | OK | `FileDialog.resolved_name: String` | 3 | No C++ analog — Rust-specific cache enabling `&self` `value()` after `&mut self` `valid()`. Private; has a 4-line internal doc explaining the invariant. |
| `info_pane_id` field | impl | EQUIVALENT | OK | `FileDialog.info_pane_id: ViewId` | 3 | C++ has no explicit pointer (pane is found by iteration); Rust caches the id for O(1) child access. Private; internal comment updated to note who reads it. |
| `cmFileOpen` / `cmFileReplace` / `cmFileClear` / `cmFileInit` (constants) | 435 | PORTED | OK | `Command::FILE_OPEN` / `FILE_REPLACE` / `FILE_CLEAR` / `FILE_INIT` | N/A | Live in `src/command.rs`, NOT in `src/dialog/filedlg.rs`. Out of scope for this pass (only `src/dialog/filedlg.rs` may be touched). |
| `cmFileFocused` / `cmFileDoubleClicked` (message constants) | 435 | PORTED | OK | `Command::FILE_FOCUSED` / `FILE_DOUBLE_CLICKED` | N/A | Same: live in `src/command.rs`. Out of scope for this pass. |

## Summary

- PORTED: 15   EQUIVALENT: 9   NOT-PORTED: 2   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0 (N/A: 2 Command constants in src/command.rs — out of scope for this pass)
- Notable findings: No missing or suspect items. The two most notable idiomatic deviations — raw `TFileInputLine*`/`TFileList*` pointers becoming `ViewId` handles (D3), and `getData`/`setData` becoming the D10 value protocol — are both deliberate, documented, and correctly implemented. The `Load`/`Store`/`shutDown` trio is cleanly not-ported per D12 with no stubs left behind.
