# TFileList  (guide p. 441)

Rust module(s): `src/dialog/filedlg.rs`   |   magiblot: `include/tvision/stddlg.h` / `source/tvision/tfillist.cpp`

> The 1992 print guide gives only a brief stub for `TFileList` (p. 441: "Details
> of TFileList's fields and methods are in the online Help"). The authoritative
> method specification is in `stddlg.h` and `tfillist.cpp`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `focusItem` (method) | 441 | EQUIVALENT | OK | `FileList::on_focus_changed` (impl `ListViewer`) | 3 | Already at score 3. |
| `getText` (method) | 441 | PORTED | OK | `FileList::get_text` (impl `ListViewer`) | 3 | Raised: doc now explains the trailing `/` for dirs and the never-panic out-of-bounds behavior. |
| `getKey` (method, private) | impl | EQUIVALENT | OK | `FileList::search` (impl `SortedSearch`) | 3 | Already at score 3. |
| `handleEvent` (method) | 441 | PORTED | OK | `FileList::handle_event` (impl `View::handle_event`) → `sorted_handle_event` | 3 | Raised: doc now explains the Shift→dir-section routing, double-click→`FILE_DOUBLE_CLICKED` flow, and that callers do not call this directly. |
| `readDirectory(dir, wildCard)` (method) | 441 | PORTED | OK | `FileList::read_directory` | 3 | Raised: doc added (was missing entirely). Now explains the ctx-ful vs ctx-free distinction. |
| `readDirectory(wildCard)` (1-arg overload) | 441 | EQUIVALENT | OK | Called via `FileDialog::navigate` (uses cached `self.directory`) | N/A | No dedicated Rust method; `FileDialog::navigate` passes the cached directory explicitly. |
| `selectItem` (method) | 441 | PORTED | OK | `FileList::select_item` (impl `ListViewer`) | 3 | Already at score 3. |
| `getData` (method) | impl | NOT-PORTED | — | — | — | C++ no-op. Rust `value()` returns `None`. |
| `setData` (method) | impl | NOT-PORTED | — | — | — | See `getData`. |
| `dataSize` (method) | impl | NOT-PORTED | — | — | — | Returns 0 in C++. Rust: `value() == None`. |
| `newList` (method) | impl | NOT-PORTED | — | — | — | C++ collection-swap API. Rust: `Vec` set directly by `read_directory`. |
| `list()` accessor | impl | NOT-PORTED | — | — | — | C++ returns typed collection pointer. Rust: `FileList::list() -> &[SearchRec]` — raised below. |
| `focused_rec` (Rust-only) | impl | EQUIVALENT | OK | `FileList::focused_rec() -> Option<SearchRec>` | 3 | Already at score 3. |
| `wildcard_match` (Rust-only) | impl | EQUIVALENT | OK | `FileList::wildcard_match` (private) | 3 | Already at score 3 (private). |
| `shiftState` (field) | impl | PORTED | OK | `FileList.shift_state: u8` | 3 | Private field. Internal comment raised: explains the `search_pos` transition and how `sorted_handle_event` sets it. |
| `searchPos` (field) | impl | PORTED | OK | `FileList.search_pos: i32` | 3 | Private field. Internal comment raised: explains `-1` = no-search sentinel and the `SortedSearch` accessor. |
| Two-column layout (`numCols = 2`) | impl | PORTED | OK | `ListViewerState::new(bounds, 2, …)` in `FileList::new` | 3 | Raised in `FileList::new` doc: explains `num_cols = 2`, cursor at column 1, and `h` parity. |
| `tooManyFiles` error (static) | impl | NOT-PORTED | — | — | — | DOS OOM guard. Not needed on Rust's `Vec`. |
| `list()` (Rust public accessor) | impl | EQUIVALENT | OK | `FileList::list() -> &[SearchRec]` | 3 | Raised: doc now explains sort order and refers to `focused_rec` for the cursor entry. |

## Summary

- PORTED: 7   EQUIVALENT: 5   NOT-PORTED: 6   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Previously below-bar public symbols raised: `get_text`, `handle_event`, `read_directory` (was missing doc entirely), `list`, `new`. Private fields `shift_state` and `search_pos` received improved internal comments.
