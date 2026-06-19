# TStringCollection  (guide pp. 547–548)

Rust module(s): `src/validate.rs` (`StringLookupValidator`); `src/widgets/cluster.rs` (`Cluster.strings`); `src/widgets/list_box.rs` (`ListBox.items`)   |   magiblot: include/tvision/tvobjs.h (`TNSSortedCollection`) — `TStringCollection` adds `Compare`/`FreeItem`/`GetItem`/`PutItem` overrides

> `TStringCollection` is a `TSortedCollection` of Pascal heap-allocated strings,
> providing ASCII-sorted order and stream I/O for string items. In `tvision-rs`,
> sorted string lists are plain `Vec<String>` with Rust's built-in `Ord` on
> `String` (UTF-8, but ASCII-compatible for the ASCII sort ordering the guide
> specifies). The concrete usages are: `Cluster.strings: Vec<String>` (cluster
> item labels), `ListBox.items: Vec<String>` (list box entries), and
> `StringLookupValidator.strings: Vec<String>` (valid-input lookup set).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Compare` (method) | 547 | EQUIVALENT | OK | `String`'s `Ord` implementation (`str::cmp` / `<str as PartialOrd>`) | N/A | Guide: compares two strings returning -1/0/+1 for ASCII ordering. Rust: `String` implements `Ord` via lexicographic byte comparison (same as ASCII ordering for ASCII text). Used automatically by `sort`, `binary_search`, `partition_point`, etc. No explicit `compare` symbol needed. |
| `FreeItem` (method) | 547 | EQUIVALENT | OK | RAII drop of `String` | N/A | Overrides `TCollection::FreeItem` to call `DisposeStr` (Pascal heap-string free). Rust: `String` is dropped automatically when removed from the `Vec`. No explicit `free_item` hook. |
| `GetItem` (method) | 547 | NOT-PORTED | — | — | — | Reads a string from a `TStream` via `S.ReadStr`. Stream machinery dropped (D12). |
| `PutItem` (method) | 548 | NOT-PORTED | — | — | — | Writes a string to a `TStream` via `S.WriteStr`. Stream machinery dropped (D12). |

## Summary

- PORTED: 0   EQUIVALENT: 2   NOT-PORTED: 2   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: No gaps or suspect items. `TStringCollection` is a thin specialization — its only non-stream behavior is the `Compare` override (ASCII string ordering) and `FreeItem` (string deallocation). Both are handled automatically in Rust via `String`'s `Ord` and RAII; no dedicated type is needed. The two NOT-PORTED entries are `TStreamable` methods (D12). Three distinct call-sites use `Vec<String>` in the codebase: cluster labels (`Cluster.strings`), list box entries (`ListBox.items`), and lookup-validator sets (`StringLookupValidator.strings`).
