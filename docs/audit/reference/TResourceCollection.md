# TResourceCollection  (guide pp. 519–520)

Rust module(s): none   |   magiblot: `include/tvision/resource.h` / `source/tvision/tresfile.cpp`

> TResourceCollection is a `TStringCollection` descendant used *internally* by
> `TResourceFile` to maintain a sorted, key-indexed collection of
> `TResourceItem` records (`{pos: int32_t; size: int32_t; key: char*}`). It
> overrides `keyOf` (returns the string key from a `TResourceItem`) and
> `freeItem`/`readItem`/`writeItem` to handle the extra `pos`/`size` fields.
>
> The entire class is a private implementation detail of `TResourceFile` — the
> guide's text says "TResourceCollection is used *internally* by TResourceFile
> objects to maintain a resource file's index" and the class has no user-facing
> API beyond its constructor. It belongs fully to the resource/streamable
> subsystem dropped project-wide.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TResourceCollection` (class / type) | 519 | NOT-PORTED | — | — | N/A | Internal index for TResourceFile; resource/streamable subsystem dropped. |
| `TResourceCollection(aLimit, aDelta)` (constructor) | 519 | NOT-PORTED | — | — | N/A | Creates a sorted collection with given capacity; dropped with subsystem. |
| `keyOf(item: Pointer): Pointer` (method) | 519 | NOT-PORTED | — | — | N/A | Returns `TResourceItem.key` from an item pointer; internal to stream index; dropped. |
| `freeItem(item: Pointer)` (private) | 519 | NOT-PORTED | — | — | N/A | Disposes a `TResourceItem` including its heap-allocated key string; dropped. |
| `readItem(ipstream&): void*` (private) | 519 | NOT-PORTED | — | — | N/A | Stream deserialization of a `TResourceItem`; dropped. |
| `writeItem(void*, opstream&)` (private) | 519 | NOT-PORTED | — | — | N/A | Stream serialization of a `TResourceItem`; dropped. |
| `streamableName()` (private) | 519 | NOT-PORTED | — | — | N/A | Stream registration; dropped. |
| `build()` (static) | 519 | NOT-PORTED | — | — | N/A | Stream factory; dropped. |
| `TResourceItem` (struct: `pos`, `size`, `key`) | — | NOT-PORTED | — | — | N/A | Wire-format record used by TResourceCollection; dropped with subsystem. |

## Summary

- PORTED: 0   EQUIVALENT: 0   NOT-PORTED: 9   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable finding: TResourceCollection has no user-facing API — it is purely an internal component of TResourceFile. The guide itself describes it as used "internally." All entries are NOT-PORTED as part of the resource/streamable subsystem drop. Nothing here represents a capability gap.
