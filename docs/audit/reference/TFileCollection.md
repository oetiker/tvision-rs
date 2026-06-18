# TFileCollection  (guide p. 435)

Rust module(s): `src/dialog/filedlg.rs`   |   magiblot: `include/tvision/stddlg.h` / `source/tvision/tfildlg.cpp`

> The 1992 print guide mentions `TFileCollection` only briefly as the sorted
> collection type used by `TFileList`. The specification is in `stddlg.h`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TFileCollection` class | 435 | EQUIVALENT | OK | `struct FileCollection` | 3 | C++: a `TSortedCollection` subclass managing heap-allocated `TSearchRec*` pointers, with a virtual `compare` and `freeItem`. Rust: a newtype wrapping `Vec<SearchRec>` with a sorted insert. Known idiomatic mapping: `TCollection` family → idiomatic Rust `Vec`. Module doc explains the collapse. |
| `compare` (private virtual method) | 435 | EQUIVALENT | OK | `fn search_rec_compare(a: &SearchRec, b: &SearchRec) -> Ordering` | 3 | C++ virtual `compare(void*, void*)`. Rust: free function `search_rec_compare`; used by both `FileCollection::insert` and `FileList::search`. Public (used in tests). The sort order (files alpha, dirs alpha, `".."` last) is documented and tested. |
| `freeItem` (private virtual method) | 435 | NOT-PORTED | — | — | — | C++: `delete (TSearchRec*)item` — heap item destructor. Rust: RAII via `Drop` on `Vec<SearchRec>`; no explicit free. |
| `insert` (method, virtual) | 435 | PORTED | OK | `FileCollection::insert(&mut self, rec: SearchRec)` | 3 | C++: sorted insert via `TSortedCollection::insert`. Rust: binary search via `partition_point` then `Vec::insert`. Maintains the `search_rec_compare` sort order. Duplicate names do not occur (noted in the doc). |
| `at` (method) | 435 | PORTED | OK | `FileCollection::at(index: usize) -> Option<&SearchRec>` | 2 | C++: `at(ccIndex)` returns `TSearchRec*`. Rust: returns `Option<&SearchRec>`; out-of-bounds → `None` (no panic). |
| `len` (method) | impl | EQUIVALENT | OK | `FileCollection::len() -> usize` | 2 | C++: `count` field from `TCollection`. Rust: `Vec::len`. |
| `is_empty` (method) | impl | EQUIVALENT | OK | `FileCollection::is_empty() -> bool` | 2 | No C++ equivalent — idiomatic Rust addition. |
| `items` / `into_items` accessors | impl | EQUIVALENT | OK | `FileCollection::items() -> &[SearchRec]`, `::into_items() -> Vec<SearchRec>` | 2 | No C++ equivalent — expose the sorted slice for read access and ownership transfer. `into_items` avoids a clone in `build_listing` (a temporary `FileCollection`). |
| `indexOf` / `remove` / `free` / `atInsert` / `atPut` / `firstThat` / `lastThat` (methods) | 435 | NOT-PORTED | — | — | — | General-purpose `TCollection` API. Module doc explicitly states: "The general-purpose collection API (index-of, remove, replace-at, find-first, …) is omitted; nothing here needs it." Correct — only sorted insert and indexed read are used by `FileList::build_listing` and `FileList::search`. |
| `readItem` / `writeItem` / `streamableName` / `build` | 435 | NOT-PORTED | — | — | — | `TStreamable` persistence — dropped per D12. |
| `search_rec_compare` doctest | impl | EQUIVALENT | OK | Inline doctest in `search_rec_compare` | 3 | C++ has no doctest. Rust includes a doctest demonstrating the `".."` sorts-last invariant — verifies the public comparator contract. |

## Summary

- PORTED: 2   EQUIVALENT: 7   NOT-PORTED: 4   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 4   |   → concept: 0
- Notable findings: No gaps or suspect items. The four NOT-PORTED entries are all correct: `freeItem` is RAII; the general-purpose collection manipulation methods are genuinely not needed (the file dialog only inserts and reads by index); and `TStreamable` is dropped per D12. The most important correctness point is the `search_rec_compare` invariant — "The sign of every branch matters — do not 'tidy' it" is documented in the source, and the test suite verifies all seven comparison branches.
