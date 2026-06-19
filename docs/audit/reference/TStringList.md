# TStringList  (guide pp. 548–549)

Rust module(s): `src/text.rs` (`tv::text::StringList`)   |   magiblot: `include/tvision/resource.h` / `source/tvision/tstrlist.cpp`

> TStringList is a stream-based keyed string lookup: strings are stored in a
> `.res` resource stream and indexed by a `u16` key. The entire streaming and
> on-disk persistence machinery (`TStreamable` base, `Load`/`Store`, `read`,
> `write`, `build`, `TStrIndexRec` run-length index, byte-length-prefixed blob)
> is part of the resource/streamable subsystem dropped project-wide.
> The observable contract — keyed lookup of strings — survives as
> `tv::text::StringList` (`BTreeMap<u16, String>`), documented in the
> `TStreamable` deviation note in `src/text.rs` (deviation D12).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Load` (constructor) | 548 | NOT-PORTED | — | — | N/A | Stream constructor; resource/streamable subsystem dropped (serde if revived). The in-memory equivalent is `StringList::new()` + `insert`. |
| `Done` (destructor) | 548 | NOT-PORTED | — | — | N/A | Pascal destructor; Rust `Drop` is implicit. |
| `Get` (method) | 549 | EQUIVALENT | OK | `tv::text::StringList::get(key: u16) -> Option<&str>` | 3 | C++: `Get(Key: Word): String` returns `""` for missing keys. Rust returns `Option<&str>`; callers use `.unwrap_or("")` for the same default. Doc now shows runtime lookup usage with the empty-string fallback pattern, and doctest compiles under `cargo test --doc`. |
| `streamableName` (private) | — | NOT-PORTED | — | — | N/A | Stream registration machinery; dropped with TStreamable. |
| `readItem` / `read` (private) | — | NOT-PORTED | — | — | N/A | Stream deserialization; dropped. |
| `write` (private, no-op) | — | NOT-PORTED | — | — | N/A | Stream serialization no-op in TStringList (write is TStrListMaker's job); dropped. |
| `build` (static) | — | NOT-PORTED | — | — | N/A | Stream factory; dropped. |
| `ip` / `basePos` / `indexSize` / `index` (private fields) | — | NOT-PORTED | — | — | N/A | Stream-position bookkeeping; replaced by `BTreeMap<u16, String>` in `StringList`. |

> **Additional capability in Rust beyond the guide's public API:**
> `StringList::insert`, `StringList::len`, `StringList::is_empty`,
> `FromIterator<(u16, S)>`, and `Default` are Rust-idiomatic additions with no
> C++ counterpart (the guide's TStringList was read-only; building was
> TStrListMaker's role). These are extensions, not gaps.

## Summary

- PORTED: 0   EQUIVALENT: 1   NOT-PORTED: 7   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable finding: `get` raised to 3 (added runtime lookup example with empty-string fallback, doctest passes). All dropped entries are stream/persistence machinery (resource/streamable subsystem dropped project-wide). The `TStringList`/`TStrListMaker` maker-vs-reader split collapses into one type because the write/read streaming asymmetry no longer exists.
