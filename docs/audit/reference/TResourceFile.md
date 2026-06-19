# TResourceFile  (guide pp. 520–523)

Rust module(s): none   |   magiblot: `include/tvision/resource.h` / `source/tvision/tresfile.cpp`

> TResourceFile implements a key-indexed binary object store on top of a
> seekable `fpstream`. Objects (any registered `TStreamable`) are stored by
> string key and retrieved by key; the index is a `TResourceCollection` sorted
> by key. The entire subsystem — `TStreamable` registration, `fpstream`,
> `TResourceCollection`, `.res` binary format, `.EXE` append support — belongs
> to the resource/streamable subsystem dropped project-wide.
>
> No Rust analog exists. The typical use cases (storing UI string tables,
> localised strings, embedded resources) are served in tvision-rs by:
> - `tv::text::StringList` for keyed string tables (covers the `TStringList`
>   use case that TResourceFile was often used to host).
> - Plain Rust `include_bytes!` / `include_str!` for compile-time embedding.
> - Standard OS file I/O for runtime resource files.
>
> These are NOT identified as `EQUIVALENT` because TResourceFile provided a
> complete generic object-store protocol (store/retrieve *any* `TStreamable`,
> not just strings, via a binary `.res` file with EXE-append support). There
> is no single Rust analog; the capability is genuinely absent. However, this
> is a deliberate drop (resource/streamable subsystem), not an oversight.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Stream` (field) | 520 | NOT-PORTED | — | — | N/A | `PStream` pointer to the backing `fpstream`; resource/streamable subsystem dropped. |
| `Modified` (field) | 520 | NOT-PORTED | — | — | N/A | Boolean dirty flag controlling `Flush` behaviour; dropped with the subsystem. |
| `basePos` (protected field) | 520 | NOT-PORTED | — | — | N/A | Internal stream position bookkeeping; dropped. |
| `indexPos` (protected field) | 520 | NOT-PORTED | — | — | N/A | Internal index position in stream; dropped. |
| `index` (protected field) | 520 | NOT-PORTED | — | — | N/A | `TResourceCollection*`; dropped. |
| `Init(AStream: PStream)` (constructor) | 520 | NOT-PORTED | — | — | N/A | Opens/creates a `.res` file on an existing stream; resource/streamable subsystem dropped. |
| `Done` (destructor) | 521 | NOT-PORTED | — | — | N/A | Flushes and disposes index + stream; dropped. |
| `Count` (method) | 521 | NOT-PORTED | — | — | N/A | Returns number of resources; dropped. |
| `Delete(Key: String)` (method) | 521 | NOT-PORTED | — | — | N/A | Removes a resource by key (space not reclaimed); dropped. |
| `Flush` (method) | 522 | NOT-PORTED | — | — | N/A | Writes updated index and header to stream if `Modified`; dropped. |
| `Get(Key: String): PObject` (method) | 522 | NOT-PORTED | — | — | N/A | Looks up key, seeks stream, calls `Stream^.Get` to deserialise; dropped. |
| `KeyAt(I: Integer): String` (method) | 522 | NOT-PORTED | — | — | N/A | Returns the key of the Ith resource (for iteration); dropped. |
| `Put(Item: PObject; Key: String)` (method) | 522 | NOT-PORTED | — | — | N/A | Appends serialised object + registers key in index; dropped. |
| `SwitchTo(AStream: PStream; Pack: Boolean): PStream` (method) | 522 | NOT-PORTED | — | — | N/A | Copies resource file to a new stream (optionally compacting); dropped. |

## Summary

- PORTED: 0   EQUIVALENT: 0   NOT-PORTED: 14   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable finding: TResourceFile is entirely stream/persistence infrastructure. All 14 entries are NOT-PORTED because the resource/streamable subsystem is dropped project-wide (serde if revived). No individual feature is missing — this is a deliberate architectural decision. The string-table capability (the most common use) is covered by `tv::text::StringList`.
