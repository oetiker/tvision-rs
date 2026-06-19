# TSortedCollection  (guide pp. 531–534)

Rust module(s): no dedicated module — sorted `Vec`/`partition_point` used inline   |   magiblot: include/tvision/tvobjs.h (`TNSSortedCollection`)

> `TSortedCollection` is a `TCollection` derivative that maintains items in
> sorted order using a virtual `Compare` method. In `tvision-rs`, the idiom is
> a plain `Vec` with `partition_point`-based insertion and `sort_by`/`Ord`
> comparators — there is no `SortedCollection` type. The one concrete usage in
> the codebase is `FileCollection` (`src/dialog/filedlg.rs`).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Duplicates` (field) | 531 | EQUIVALENT | OK | `partition_point` insert always keeps duplicates when the caller decides not to deduplicate; `FileCollection::insert` deduplicates implicitly by directory-listing semantics | N/A | Boolean flag; `False` by default (no duplicates). Rust: whether duplicates are admitted is a property of the insertion logic, not a stored flag. `FileCollection` silently allows the caller to insert distinct records (no dedup needed because filesystem names are unique). Intentional: no separate flag needed. |
| `Load` (constructor) | 532 | NOT-PORTED | — | — | — | Stream-load constructor reads `Duplicates` flag from stream. `TStreamable` dropped (D12). |
| `Compare` (method) | 532 | EQUIVALENT | OK | `Ord` trait / `PartialOrd` / custom comparator closure | N/A | Abstract method that must be overridden in all descendants; returns -1/0/+1. Rust: implement `Ord`/`PartialOrd` on the item type, or supply a `fn(&T, &T) -> Ordering` comparator. `FileCollection` uses `search_rec_compare(a, b) -> Ordering` (filedlg.rs). |
| `IndexOf` (method) | 532 | EQUIVALENT | OK | `Vec::binary_search_by` / `partition_point` + check | N/A | Uses `Search(KeyOf(item), I)` then returns I or -1. Rust: `binary_search_by` returns `Ok(index)` or `Err(insertion_point)`; `Ok` → found, `Err` → not present. |
| `Insert` (method) | 533 | EQUIVALENT | OK | `Vec::partition_point` + `Vec::insert` | N/A | Calls `Search(KeyOf(item), I)` then `AtInsert(I, item)` (skips if duplicate and `!Duplicates`). Rust: `let pos = vec.partition_point(\|x\| comparator(x, &item) == Less); vec.insert(pos, item)`. Implemented in `FileCollection::insert`. |
| `KeyOf` (method) | 533 | EQUIVALENT | OK | identity (item is its own key) or a field projection closure | N/A | Returns the key for an item; default returns the item itself. Rust: if item == key, no extraction needed. When a sub-field is the key, pass a closure to `sort_by_key` / `partition_point`. `FileCollection` passes the full `SearchRec` to `search_rec_compare`, which projects the relevant fields inline. |
| `Search` (method) | 533 | EQUIVALENT | OK | `Vec::binary_search_by` / `partition_point` | N/A | Binary-searches the sorted collection using `Compare`; sets `Index` to found position or insertion point. Rust: `binary_search_by` returns `Ok(i)`/`Err(i)` covering both cases. |
| `Store` (method) | 533 | NOT-PORTED | — | — | — | Writes collection + `Duplicates` flag to a `TStream`. Stream machinery dropped (D12). |

## Summary

- PORTED: 0   EQUIVALENT: 6   NOT-PORTED: 2   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: No gaps or suspect items. The sorted-insert idiom (`partition_point` + `Vec::insert`) is concretely applied in `FileCollection::insert` and documented in its module doc. The `Duplicates` flag has no Rust equivalent because whether duplicates are admitted is encoded in the insertion logic rather than a runtime flag — an intentional, reasonable deviation. Both NOT-PORTED entries are `TStreamable` machinery (D12).
