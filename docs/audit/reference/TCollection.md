# TCollection  (guide pp. 400–406)

Rust module(s): no dedicated module — `Vec<T>` / slices used inline throughout   |   magiblot: include/tvision/tvobjs.h (`TNSCollection` / `TCollection`)

> `TCollection` is the base collection class for all Turbo Vision container
> objects. In `tvision-rs`, the idiom is plain Rust `Vec<T>` (or a thin newtype
> over it). There is no `Collection` type. Every entry below is therefore either
> `EQUIVALENT` (the idiomatic Vec/iterator analog) or `NOT-PORTED` (DOS memory
> manager / stream machinery).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Count` (field) | 400 | EQUIVALENT | OK | `Vec::len()` / `slice::len()` | N/A | Read-only count of live items; `Vec::len()` is the direct analog. No single `tv::` symbol — used inline everywhere. |
| `Delta` (field) | 401 | NOT-PORTED | — | — | — | Growth-increment for DOS memory-manager realloc. Rust `Vec` grows geometrically without user control; no analog needed. |
| `Items` (field) | 401 | EQUIVALENT | OK | `Vec<T>` itself (owned) or `&[T]` / `items()` slice accessors | N/A | Pointer-to-array-of-pointers in Pascal; becomes the `Vec` itself. e.g. `FileCollection::items() -> &[SearchRec]`. |
| `Limit` (field) | 401 | EQUIVALENT | OK | `Vec::capacity()` | N/A | Currently-allocated size; `Vec::capacity()` is the exact analog. Rarely queried externally; no public symbol. |
| `Init` (constructor) | 401 | EQUIVALENT | OK | `Vec::new()` / `Vec::with_capacity(n)` | N/A | `Init(ALimit, ADelta)` allocates with initial capacity and growth delta. Rust: `Vec::with_capacity(n)`. Delta not carried (see above). |
| `Load` (constructor) | 401 | NOT-PORTED | — | — | — | Stream-load constructor. `TStreamable` dropped (deviation D12). |
| `Done` (destructor) | 401 | EQUIVALENT | OK | `Vec` drop (RAII) | N/A | Calls `FreeAll` then sets `Limit` to 0. Rust: `Vec` drops all owned items on scope exit. |
| `At` (method) | 401 | EQUIVALENT | OK | `Vec::get(index)` / `&vec[index]` / `slice[index]` | N/A | Returns pointer to item at index; calls `Error(coIndexError)` on out-of-bounds. Rust: `get()` returns `Option`, direct indexing panics. `FileCollection::at(index) -> Option<&SearchRec>` is the pattern used. |
| `AtDelete` (method) | 402 | EQUIVALENT | OK | `Vec::remove(index)` | N/A | Removes item at index, shifts remaining items left. Rust `Vec::remove` is identical behavior. Not wrapped as a named symbol; used inline. |
| `AtFree` (method) | 402 | EQUIVALENT | OK | `Vec::remove(index)` (item dropped by RAII) | N/A | `AtDelete` + `FreeItem`. In Rust, removing from `Vec` and dropping is one step (RAII). |
| `AtInsert` (method) | 402 | EQUIVALENT | OK | `Vec::insert(index, item)` | N/A | Inserts at position, shifts right. Called `coOverflow` error if full. Rust `Vec::insert` resizes automatically. |
| `AtPut` (method) | 402 | EQUIVALENT | OK | `vec[index] = item` (index assignment) | N/A | Replace item at index. Rust index-assign. |
| `Delete` (method) | 402 | EQUIVALENT | OK | `Vec::remove(pos)` after `.position()` | N/A | Remove by value (finds index first via `indexOf` in Pascal). Rust: `.iter().position(|x| x == item).map(|i| vec.remove(i))`. |
| `DeleteAll` (method) | 403 | EQUIVALENT | OK | `Vec::clear()` | N/A | Sets `Count` to zero without freeing items. `Vec::clear()` drops owned items, but for pointer collections the analog is `vec.clear()` without running element destructors — in Rust this is expressed by holding raw pointers or `ManuallyDrop`; in practice all tvision-rs collections own their data, so `clear()` is exact. |
| `Error` (method) | 403 | NOT-PORTED | — | — | — | Virtual error hook that fires `coOverflow`/`coIndexError` run-time errors. Rust uses `Option`/panics/`Result` at the call site; no virtual error method. |
| `FirstThat` (method) | 403 | EQUIVALENT | OK | `.iter().find(\|item\| predicate(item))` | N/A | Applies a Boolean function to each item in forward order, returns first match or `nil`. Rust: `Iterator::find`. |
| `ForEach` (method) | 403 | EQUIVALENT | OK | `.iter().for_each(\|item\| action(item))` / `for item in &vec` | N/A | Applies a procedure to each item. Rust: `for` loop or `Iterator::for_each`. |
| `Free` (method) | 404 | EQUIVALENT | OK | `vec.retain(\|x\| x != item)` + RAII drop | N/A | Delete + FreeItem. Rust: remove from `Vec` and let RAII drop. |
| `FreeAll` (method) | 404 | EQUIVALENT | OK | `Vec::clear()` (owned items) or `vec.drain(..)` | N/A | Delete + dispose all items. Rust drop-on-remove covers it. |
| `FreeItem` (method) | 404 | NOT-PORTED | — | — | — | Virtual hook to dispose one item (needed because Pascal `TCollection` stores `Pointer`s, not typed values). Rust ownership makes this hook unnecessary — drop is automatic and typed. |
| `GetItem` (method) | 404 | NOT-PORTED | — | — | — | Reads one item from a `TStream` during `Load`. Stream machinery dropped (D12). |
| `IndexOf` (method) | 405 | EQUIVALENT | OK | `.iter().position(\|x\| x == item)` | N/A | Returns index or `-1`. Rust: `Iterator::position` returns `Option<usize>`. |
| `Insert` (method) | 405 | EQUIVALENT | OK | `Vec::push(item)` (unsorted) or `Vec::insert(pos, item)` (sorted) | N/A | Default appends at end (calls `AtInsert(Count, item)`). Sorted subclasses override. Rust: `push` for unsorted; `partition_point` + `insert` for sorted (see `FileCollection::insert`). |
| `LastThat` (method) | 405 | EQUIVALENT | OK | `.iter().rev().find(\|item\| predicate(item))` | N/A | Like `FirstThat` but in reverse. Rust: `Iterator::rev().find`. |
| `Pack` (method) | 405 | NOT-PORTED | — | — | — | Removes `nil` pointer entries from the array. Rust `Vec` never holds null entries; this is a DOS-heap artifact with no analog. |
| `PutItem` (method) | 406 | NOT-PORTED | — | — | — | Writes one item to a `TStream` during `Store`. Stream machinery dropped (D12). |
| `SetLimit` (method) | 406 | EQUIVALENT | OK | `Vec::reserve(n)` / `Vec::with_capacity(n)` | N/A | Resizes the heap allocation for the items array. Rust: `Vec::reserve` or `with_capacity`. |
| `Store` (method) | 406 | NOT-PORTED | — | — | — | Writes entire collection to a `TStream`. Stream machinery dropped (D12). |
| `coIndexError` (constant) | 403 | NOT-PORTED | — | — | — | Error code passed to `Error` when index is out of range. No error-code constants needed; Rust uses `Option`/panic. |
| `coOverflow` (constant) | 403 | NOT-PORTED | — | — | — | Error code when collection is full and cannot grow. No analog; `Vec` grows automatically. |

## Summary

- PORTED: 0   EQUIVALENT: 20   NOT-PORTED: 10   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: No gaps or suspect items. The "collections become `Vec`" idiom is consistently applied and explicitly documented in the filedlg module doc (lines 11-14: "Following tvision-rs's 'collections become `Vec`' convention"). The 10 NOT-PORTED entries are all DOS memory-manager artifacts (`Delta`, `Pack`, `nil`-pointer handling) or `TStreamable` machinery (D12) — all intentionally absent. No `TCollection` type is needed or missing.
