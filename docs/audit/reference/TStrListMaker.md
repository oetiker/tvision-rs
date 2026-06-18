# TStrListMaker  (guide pp. 549–550)

Rust module(s): `src/text.rs` (`tv::text::StringList`)   |   magiblot: `include/tvision/resource.h` / `source/tvision/tstrlist.cpp`

> TStrListMaker is the write-side counterpart to TStringList: it builds a keyed
> string table in memory and stores it to a resource stream. The entire
> streaming/persistence machinery is part of the resource/streamable subsystem
> dropped project-wide. In tvision-rs the maker-vs-reader split collapses into
> one type (`tv::text::StringList`) because the streaming asymmetry no longer
> exists — `insert` plays the role of `Put` (deviation D12, documented in
> `src/text.rs`).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Init(AStrSize, AIndexSize: Word)` (constructor) | 550 | NOT-PORTED | — | — | N/A | Allocates a fixed-size in-memory string buffer + index buffer for stream serialization. No analog needed: `StringList::new()` uses a `BTreeMap` with no fixed-size allocation. |
| `Done` (destructor) | 550 | NOT-PORTED | — | — | N/A | Pascal destructor; Rust `Drop` implicit. |
| `Put(Key: Word; S: String)` (method) | 550 | EQUIVALENT | OK | `tv::text::StringList::insert(key: u16, value: impl Into<String>)` | 2 | Same observable contract: associate a string with a numeric key, overwriting any prior value. Shape is idiomatic Rust. Doc explains *what*, not *how/when to use* in context. |
| `Store(var S: TStream)` (method) | 550 | NOT-PORTED | — | — | N/A | Serializes the string list to a resource stream using a compressed run-length index + byte-length-prefixed blob. Resource/streamable subsystem dropped (serde if revived). |
| `streamableName` (private) | — | NOT-PORTED | — | — | N/A | Returns `TStringList::name`; stream registration dropped. |
| `write` (private) | — | NOT-PORTED | — | — | N/A | Internal stream write implementation; dropped. |
| `read` (private, returns 0) | — | NOT-PORTED | — | — | N/A | TStrListMaker is write-only in the C++; dropped. |
| `build` (static) | — | NOT-PORTED | — | — | N/A | Stream factory; dropped. |
| `strPos` / `strSize` / `strings` / `indexPos` / `indexSize` / `index` / `cur` (private fields) | — | NOT-PORTED | — | — | N/A | Fixed-size buffer bookkeeping for stream serialization; replaced by `BTreeMap`. |

## Summary

- PORTED: 0   EQUIVALENT: 1   NOT-PORTED: 9   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 1   |   → concept: 0
- Notable finding: `Put` is the only user-visible capability and it maps cleanly to `StringList::insert`. All other entries are stream/persistence machinery (resource/streamable subsystem dropped, deviation D12). The maker-vs-reader split is intentionally collapsed.
