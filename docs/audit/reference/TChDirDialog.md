# TChDirDialog  (guide pp. 391–393)

Rust module(s): src/dialog/filedlg.rs   |   magiblot: include/tvision/stddlg.h / source/tvision/tchdrdlg.cpp

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `ChDirButton` (field) | 391 | PORTED | OK | `ChDirDialog::chdir_button_id: ViewId` | 3 | Private field. Internal comment raised: explains it is stored only for the post-assembly `set_chdir_button` call and that the button manages default state through the `MakeButtonDefault` deferred channel. |
| `DirInput` (field) | 392 | PORTED | OK | `ChDirDialog::dir_input_id: ViewId` | 3 | Private field. Internal comment raised: explains it is read by `handle_event` and `valid` via `dialog.child_mut`. |
| `DirList` (field) | 392 | PORTED | OK | `ChDirDialog::dir_list_id: ViewId` | 3 | Private field. Internal comment raised: explains use by `handle_event` for `new_directory` calls and the wiring of the chdir button. |
| `OkButton` (field) | 392 | EQUIVALENT | OK | `ok_button_id` not stored | N/A | No named field; managed by the `Dialog` group via `cmOK` / `Command::OK`. Private implementation detail. |
| `Init` (constructor) | 392 | PORTED | OK | `tv::dialog::ChDirDialog::new(opts: u16, history_id: u8) -> ChDirDialog` | 3 | Already at score 3. |
| `Load` (constructor) | 392 | NOT-PORTED | — | — | — | `TStreamable` persistence dropped (D12). |
| `DataSize` (method) | 392 | EQUIVALENT | OK | `value() → None` (trait default) | N/A | Returns 0. Rust `value() → None`. |
| `GetData` (method) | 392 | EQUIVALENT | OK | `value() → None` | N/A | No-op. Same rationale as `DataSize`. |
| `HandleEvent` (method) | 392 | PORTED | OK | `ChDirDialog::handle_event` (impl `View::handle_event`) | 3 | Already at score 3. |
| `SetData` (method) | 393 | EQUIVALENT | OK | `set_value(FieldValue)` no-op | N/A | No-op. Rust: skip-listed trait default. |
| `Store` (method) | 393 | NOT-PORTED | — | — | — | `TStreamable` persistence dropped (D12). |
| `Valid` (method) | 393 | PORTED | OK | `ChDirDialog::valid(cmd, ctx) -> bool` (impl `View::valid`) | 3 | Already at score 3. |
| `cdNormal` (flag) | 391 | PORTED | OK | `tv::dialog::CD_NORMAL: u16 = 0x0000` | 3 | Raised: doc now explains "default options" and same-as-0 equivalence. |
| `cdNoLoadDir` (flag) | 391 | PORTED | OK | `tv::dialog::CD_NO_LOAD_DIR: u16 = 0x0001` | 3 | Raised: doc now explains the construction-before-visible use case and the `reset_current` interaction. |
| `cdHelpButton` (flag) | 391 | PORTED | OK | `tv::dialog::CD_HELP_BUTTON: u16 = 0x0002` | 3 | Raised: doc now explains the `Command::HELP` posting and when to omit. |

## Summary

- PORTED: 9   EQUIVALENT: 4   NOT-PORTED: 2   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- All 3 previously below-bar public symbols (`CD_NORMAL`, `CD_NO_LOAD_DIR`, `CD_HELP_BUTTON`) raised to score 3 in this pass. Private fields `chdir_button_id`, `dir_input_id`, `dir_list_id` received improved internal comments.
