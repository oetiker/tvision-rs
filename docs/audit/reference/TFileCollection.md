# TFileCollection  (guide p. 435)

Rust module(s): `src/dialog/filedlg.rs`   |   magiblot: `include/tvision/stddlg.h` / `source/tvision/tfildlg.cpp`

> The 1992 print guide mentions `TFileCollection` only briefly as the sorted
> collection type used by `TFileList`. The specification is in `stddlg.h`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TFileCollection` class | 435 | EQUIVALENT | OK | `struct FileCollection` | 3 | C++: a `TSortedCollection` subclass managing heap-allocated `TSearchRec*` pointers, with a virtual `compare` and `freeItem`. Rust: a newtype wrapping `Vec<SearchRec>` with a sorted insert. |
| `compare` (private virtual method) | 435 | EQUIVALENT | OK | `fn search_rec_compare(a: &SearchRec, b: &SearchRec) -> Ordering` | 3 | C++ virtual `compare(void*, void*)`. Rust: free function; used by both `FileCollection::insert` and `FileList::search`. |
| `freeItem` (private virtual method) | 435 | NOT-PORTED | — | — | — | C++: `delete (TSearchRec*)item` — heap item destructor. Rust: RAII via `Drop`. |
| `insert` (method, virtual) | 435 | PORTED | OK | `FileCollection::insert(&mut self, rec: SearchRec)` | 3 | C++: sorted insert via `TSortedCollection::insert`. Rust: binary search via `partition_point` then `Vec::insert`. |
| `at` (method) | 435 | PORTED | OK | `FileCollection::at(index: usize) -> Option<&SearchRec>` | 3 | Raised: doc now explains the `Option` return (never panics), its use by `FileList`, and when to prefer `items()` for iteration. |
| `len` (method) | impl | EQUIVALENT | OK | `FileCollection::len() -> usize` | 3 | Raised: doc now notes its relationship to the list-viewer's `range` after `FileList::read_directory`. |
| `is_empty` (method) | impl | EQUIVALENT | OK | `FileCollection::is_empty() -> bool` | 3 | Raised: doc now notes when it returns `true` (before first insert / after empty-dir read). |
| `items` / `into_items` accessors | impl | EQUIVALENT | OK | `FileCollection::items() -> &[SearchRec]`, `::into_items() -> Vec<SearchRec>` | 3 | Raised: doc now explains the distinction — `items` for iteration/search, `into_items` for ownership transfer without clone. |
| `indexOf` / `remove` / `free` / `atInsert` / `atPut` / `firstThat` / `lastThat` (methods) | 435 | NOT-PORTED | — | — | — | General-purpose `TCollection` API. Intentionally omitted; nothing here needs it. |
| `readItem` / `writeItem` / `streamableName` / `build` | 435 | NOT-PORTED | — | — | — | `TStreamable` persistence — dropped per D12. |
| `search_rec_compare` doctest | impl | EQUIVALENT | OK | Inline doctest in `search_rec_compare` | 3 | Demonstrates the `".."` sorts-last invariant. |

## Summary

- PORTED: 2   EQUIVALENT: 6   NOT-PORTED: 3   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- All 4 previously below-bar public symbols (`at`, `len`, `is_empty`, `items`/`into_items`) raised to score 3 in this pass.
