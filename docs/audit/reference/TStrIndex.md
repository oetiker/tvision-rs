# TStrIndex type / TStrIndexRec type  (guide pp. 546–547)

Rust module(s): none   |   magiblot: `include/tvision/resource.h`

> `TStrIndex` is a Pascal array type (`array[0..9999] of TStrIndexRec`) used
> purely as an internal implementation detail of the stream serialization format
> for `TStringList` / `TStrListMaker`. `TStrIndexRec` is the element type of
> that array — a 6-byte record holding `Key`, `Count`, and `Offset` (all
> `ushort`/`Word`) that describe a contiguous run of up to 16 keys in the
> compressed string table on a resource stream.
>
> Both types exist entirely to support the resource/streamable subsystem, which
> is dropped project-wide. `tv::text::StringList` uses a `BTreeMap<u16, String>`
> and has no need for a run-length index or a fixed binary layout.
>
> `TStrIndexRec` is documented on p. 547 of the guide. Per the task instructions
> it is folded into this file rather than given its own file.

---

## TStrIndex  (guide p. 546)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TStrIndex` (type) | 546 | NOT-PORTED | — | — | N/A | Pascal array type used as the in-memory index buffer in `TStrListMaker` / `TStringList` stream serialization. No analog: `BTreeMap<u16, String>` subsumes this. Resource/streamable subsystem dropped. |

## TStrIndexRec  (guide p. 547)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TStrIndexRec` (type) | 547 | NOT-PORTED | — | — | N/A | 6-byte record (`Key: Word; Count: Word; Offset: Word`) encoding one run of up to 16 contiguous string-list keys in the binary stream format. Purely a stream-serialization detail; dropped with the resource/streamable subsystem. |
| `Key` (field) | 547 | NOT-PORTED | — | — | N/A | First key in a contiguous run; stream detail. |
| `Count` (field) | 547 | NOT-PORTED | — | — | N/A | Number of keys in the run (≤ 16); stream detail. |
| `Offset` (field) | 547 | NOT-PORTED | — | — | N/A | Byte offset of the first string in the run within the string blob; stream detail. |
| `TStrIndexRec()` (default constructor) | — | NOT-PORTED | — | — | N/A | Zero-initializes the record; no analog needed. |

## Summary

- PORTED: 0   EQUIVALENT: 0   NOT-PORTED: 6   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable finding: Both types are purely internal stream-serialization scaffolding with no user-visible API. The guide itself labels them "used internally by TStringList and TStrListMaker." Nothing to port; nothing missing.
