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
| `TDirCollection` (class / collection container) | 418 | EQUIVALENT | OK | `tv::dialog::DirCollection` = `Vec<DirEntry>` type alias | 3 | Raised: doc now explains when to construct directly vs. letting `DirListBox::new_directory` manage it, plus the rationale for the `Vec` collapse. |
| `at(index)` (method) | — | EQUIVALENT | OK | `Vec::get(index)` / index operator on `&[DirEntry]` | N/A | C++ typed wrapper returning `TDirEntry*`. Rust: callers use standard slice indexing or `Vec::get`. No named wrapper needed. |
| `indexOf(item)` (method) | — | NOT-PORTED | — | — | — | C++ `TCollection` indexOf (pointer identity search). Not needed by any consumer; intentionally omitted. |
| `remove(item)` (method) | — | NOT-PORTED | — | — | — | Same rationale as `indexOf`. Not needed; intentionally omitted. |
| `free(item)` (method) | — | NOT-PORTED | — | — | — | C++ manual memory management. Rust `Vec` owns by value; drop is automatic. |
| `atInsert(index, item)` (method) | — | NOT-PORTED | — | — | — | Positional insert. Not needed; intentionally omitted. |
| `atPut(index, item)` (method) | — | NOT-PORTED | — | — | — | Typed replace-at. Not needed; intentionally omitted. |
| `insert(item)` (method) | — | EQUIVALENT | OK | `Vec::push` | N/A | Used in `DirListBox::new_directory` via `build_tree`. |
| `firstThat` / `lastThat` (methods) | — | NOT-PORTED | — | — | — | Predicate-search methods. Not needed; Rust callers use `Iterator::find`. |
| `freeItem` (private method) | — | NOT-PORTED | — | — | — | C++ manual `delete`. Rust ownership model handles deallocation. |
| `TStreamable` machinery | — | NOT-PORTED | — | — | — | Stream persistence dropped project-wide. No Rust analog. |

## Summary

- PORTED: 0   EQUIVALENT: 3   NOT-PORTED: 8   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- The one previously below-bar public symbol (`DirCollection` alias) raised to score 3 in this pass.
