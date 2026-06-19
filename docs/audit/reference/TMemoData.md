# TMemoData  (guide p. 477)

Rust module(s): src/data.rs   |   magiblot: include/tvision/editors.h

> `TMemoData` is a Pascal record / C struct used as the raw data-transfer buffer for `TMemo::getData` and `TMemo::setData`. It carries a `length: ushort` followed by an inline byte `buffer[1]` (a flexible-array-member idiom). The caller allocates `dataSize()` bytes and passes a pointer; the memo fills or reads it.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `length` field | 477 | EQUIVALENT | OK | the `String` length in `tv::data::FieldValue::Text(String)` | N/A | C++: `ushort length` — the number of valid bytes in `buffer`. Rust: `FieldValue::Text` carries a `String`; its `.len()` is the equivalent. No separate field; length is implicit in the owned string. NOT public as a standalone symbol. |
| `buffer[1]` field (inline byte array) | 477 | EQUIVALENT | OK | the `String` content bytes in `tv::data::FieldValue::Text(String)` | N/A | C++: `char buffer[1]` — a flexible-array-member trailing the length word; `getData` writes `bufLen` bytes here. Rust: the `String`'s heap allocation holds the same bytes (via `from_utf8_lossy` in `Memo::value()`). No separate symbol. |
| `TMemoData` type itself | 477 | EQUIVALENT | OK | `tv::data::FieldValue::Text(String)` | 3 | The whole type is replaced by `FieldValue::Text` (deviation D10). C++ required the caller to pre-allocate `dataSize()` bytes and pass a raw pointer. Rust: the enum variant owns its allocation; no pointer arithmetic needed. `FieldValue::Text` rustdoc now names the `TMemoData`/`getData`/`setData` lineage in a heritage note AND explains how memo data flows through the gather/scatter walk. |
| `TMemoData` pointer alias (`PMemoData`) | 477 | NOT-PORTED | — | — | — | Pascal pointer type alias. No Rust analog needed; Rust uses owned values, not raw pointers. |

## Summary

- PORTED: 0   EQUIVALENT: 3   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: `TMemoData` is entirely subsumed by `FieldValue::Text` under D10. There is no partial port — it is a clean total replacement. The one doc gap is that `FieldValue::Text` does not cross-reference the `Memo` value/set_value round-trip anywhere in the rustdoc; a "# Turbo Vision heritage" note mentioning the `getData`/`setData`/`TMemoData` lineage would help C++ veterans.
