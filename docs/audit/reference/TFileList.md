# TFileList  (guide p. 441)

Rust module(s): `src/dialog/filedlg.rs`   |   magiblot: `include/tvision/stddlg.h` / `source/tvision/tfillist.cpp`

> The 1992 print guide gives only a brief stub for `TFileList` (p. 441: "Details
> of TFileList's fields and methods are in the online Help"). The authoritative
> method specification is in `stddlg.h` and `tfillist.cpp`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `focusItem` (method) | 441 | EQUIVALENT | OK | `FileList::on_focus_changed` (impl `ListViewer`) | 3 | C++: `focusItem` calls `TSortedListBox::focusItem` then broadcasts `cmFileFocused` with `list()->at(item)` as `infoPtr`. Rust: `on_focus_changed` broadcasts `Command::FILE_FOCUSED` with `self` as the resolvable source id; the pump's `ResolveFocusedFile` broker reads `focused_rec()` and delivers it to subscribers. Same semantics, different payload shape (D4 — `infoPtr` becomes resolvable `ViewId`). |
| `getText` (method) | 441 | PORTED | OK | `FileList::get_text` (impl `ListViewer`) | 2 | Returns the entry's name; appends `/` for directories. C++ also appended `/`. Matches. |
| `getKey` (method, private) | impl | EQUIVALENT | OK | `FileList::search` (impl `SortedSearch`) | 3 | C++: `getKey` builds a `TSearchRec` key from the typed prefix string for the sorted-collection binary search. Rust: `search` constructs the same key including the `FA_DIREC`-routing attr, and does the binary search explicitly. Functionally equivalent; see the doc block for the non-obvious attr-routing invariant. |
| `handleEvent` (method) | 441 | PORTED | OK | `FileList::handle_event` (impl `View::handle_event`) → `sorted_handle_event` | 2 | Incremental type-to-search machine via `sorted_handle_event`. C++ equivalent in `TSortedListBox::handleEvent`. |
| `readDirectory(dir, wildCard)` (method) | 441 | PORTED | OK | `FileList::read_directory` | 3 | Reads and publishes the listing with a `Context` (scrollbar sync + FILE_FOCUSED broadcast on completion). Also `read_directory_listing` (ctx-free sibling for construction/tests). |
| `readDirectory(wildCard)` (1-arg overload) | 441 | EQUIVALENT | OK | Called via `FileDialog::navigate` (uses cached `self.directory`) | 2 | C++ 1-arg version uses the dialog's `directory` field. Rust `navigate` passes `self.directory` explicitly to `FileList::read_directory`. |
| `selectItem` (method) | 441 | PORTED | OK | `FileList::select_item` (impl `ListViewer`) | 3 | C++: broadcasts `cmFileDoubleClicked` with `list()->at(item)`. Rust: broadcasts `Command::FILE_DOUBLE_CLICKED` with `self` as source. Payload difference is same D4 mapping as `focusItem`. |
| `getData` (method) | impl | NOT-PORTED | — | — | — | C++ `getData`/`setData`/`dataSize` are all no-ops (size 0, empty bodies). Rust `value()` returns `None` for the same reason: the file list contributes nothing to dialog data transfer. Correctly modeled. |
| `setData` (method) | impl | NOT-PORTED | — | — | — | See `getData` above. |
| `dataSize` (method) | impl | NOT-PORTED | — | — | — | Returns 0 in C++. Rust: `value() == None`. |
| `newList` (method) | impl | NOT-PORTED | — | — | — | C++ collection-management API (`TSortedCollection` swap). Rust: items are a plain `Vec<SearchRec>` set directly by `read_directory`; no list-swap API needed. |
| `list()` accessor | impl | NOT-PORTED | — | — | — | C++ returns the typed `TFileCollection*`. Rust: `FileList::list() -> &[SearchRec]` provides equivalent read access; the inner `FileCollection` is consumed during `read_directory`. |
| `focused_rec` (Rust-only) | impl | EQUIVALENT | OK | `FileList::focused_rec() -> Option<SearchRec>` | 3 | No C++ equivalent (inline `list()->at(focused)` in `focusItem`). Rust extracts this as a method so the `ResolveFocusedFile` broker can call it. Extension required by D3/D4 broker seam. |
| `wildcard_match` (Rust-only) | impl | EQUIVALENT | OK | `FileList::wildcard_match` (private) | 3 | C++ used `fnmatch` / `findfirst`'s own filter. Rust implements `*`/`?` glob matching from scratch (pure, no OS call). Deviation D14 (native paths / no DOS findfirst). |
| `shiftState` (field) | impl | PORTED | OK | `FileList.shift_state: u8` | 2 | Inherited from `TSortedListBox`. Same semantics: routes type-to-search into the dir section when Shift held. |
| `searchPos` (field) | impl | PORTED | OK | `FileList.search_pos: i32` | 2 | Inherited from `TSortedListBox`. Tracks the last matched char position; `-1` = no active search. |
| Two-column layout (`numCols = 2`) | impl | PORTED | OK | `ListViewerState::new(bounds, 2, …)` in `FileList::new` | 2 | C++: `TSortedListBox(bounds, 2, sb)`. Rust: `num_cols = 2` in `ListViewerState`. |
| `tooManyFiles` error (static) | impl | NOT-PORTED | — | — | — | DOS out-of-memory guard for collection growth. Rust `Vec` grows until true OOM (aborts); no soft guard needed. |

## Summary

- PORTED: 7   EQUIVALENT: 5   NOT-PORTED: 5   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 6   |   → concept: 0
- Notable findings: No gaps or suspect items. The five NOT-PORTED entries are all correctly modeled: `getData`/`setData`/`dataSize` were no-ops in C++ (→ `value() == None`); `newList`/`list()` are replaced by the Vec-based `read_directory`; and `tooManyFiles` has no Rust analog. The most non-obvious entry is `getKey`→`search` with its attr-routing invariant (the discriminating test in `search_attr_routes_into_file_vs_dir_section` proves it correctly).
