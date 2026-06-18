# TDirCollection  (guide p. 418)

Rust module(s): src/dialog/filedlg.rs   |   magiblot: include/tvision/stddlg.h

> Guide (p. 418): "TDirCollection is a collection of TDirEntry records used by
> TDirListBox. Details of TDirCollection's fields and methods are in the online
> Help." The public surface in stddlg.h is a `TCollection` subclass with typed
> wrappers for `at`, `indexOf`, `remove`, `free`, `atInsert`, `atPut`,
> `insert`, `firstThat`, `lastThat`, plus the private `freeItem` and
> `TStreamable` machinery.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TDirCollection` (class / collection container) | 418 | EQUIVALENT | OK | `tv::dialog::DirCollection` = `Vec<DirEntry>` type alias | 2 | C++ is a heap-allocated pointer collection (`TCollection` subclass). Rust collapses it to a plain `Vec<DirEntry>` type alias — the known idiomatic mapping (`TCollection` family → `Vec`). The alias is documented with a note on why the collection API is omitted ("nothing here needs it"). |
| `at(index)` (method) | — | EQUIVALENT | OK | `Vec::get(index)` / index operator on `&[DirEntry]` | N/A | C++ typed wrapper returning `TDirEntry*`. Rust: callers use standard slice indexing or `Vec::get`. No named wrapper needed; the `DirListBox` accesses the slice directly via `self.items`. |
| `indexOf(item)` (method) | — | NOT-PORTED | — | — | — | C++ `TCollection` indexOf (pointer identity search). `DirListBox` never needs index-of-by-pointer; the Rust port does not walk the collection to look up a `DirEntry` by identity. Intentional omission: module doc notes "the general-purpose collection API (index-of, remove, replace-at, find-first, …) is omitted; nothing here needs it." |
| `remove(item)` (method) | — | NOT-PORTED | — | — | — | Same rationale as `indexOf`. Not needed by any consumer; intentionally omitted (see module doc). |
| `free(item)` (method) | — | NOT-PORTED | — | — | — | C++ manual memory management (`TCollection::free` → `delete`). Rust `Vec` owns its elements by value; drop is automatic. No Rust analog needed. |
| `atInsert(index, item)` (method) | — | NOT-PORTED | — | — | — | Positional insert into a pointer collection. Not needed; intentionally omitted (see module doc). |
| `atPut(index, item)` (method) | — | NOT-PORTED | — | — | — | Typed replace-at. Not needed; intentionally omitted. |
| `insert(item)` (method) | — | EQUIVALENT | OK | `Vec::push` | N/A | C++ typed insert (delegates to `TCollection::insert`). Rust: `Vec::push`. Used in `DirListBox::new_directory` via `build_tree`. |
| `firstThat` / `lastThat` (methods) | — | NOT-PORTED | — | — | — | Predicate-search methods from `TCollection`. Not needed; Rust callers use `Iterator::find`/`Iterator::position` directly. Intentionally omitted (module doc). |
| `freeItem` (private method) | — | NOT-PORTED | — | — | — | C++ manual `delete (TDirEntry*)item`. Rust ownership model handles deallocation; no analog needed. |
| `TStreamable` machinery (`name`, `build`, `readItem`, `writeItem`) | — | NOT-PORTED | — | — | — | Stream persistence dropped project-wide (deviation D12). No Rust analog. |

## Summary

- PORTED: 0   EQUIVALENT: 3   NOT-PORTED: 8   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 1   |   → concept: 0
- Notable finding: The entire `TCollection` sub-API (indexOf, remove, free, atInsert, atPut, firstThat, lastThat) is intentionally omitted because no consumer uses it — this is well-reasoned and documented in the module doc. The only public symbol, the `DirCollection` type alias, scores 2 (explains what it is and the rationale for the Vec collapse, but does not explain when a caller would create one directly vs. letting `DirListBox::new_directory` manage it).
