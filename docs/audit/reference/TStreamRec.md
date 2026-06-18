# TStreamRec type  (guide pp. 545–546)

Rust module(s): N/A   |   magiblot: include/tvision/tobjstrm.h (`TStreamableClass`, `fLink`, `TStreamableTypes`)

> TStreamRec is the Pascal record that registers a streamable object type with
> the Turbo Vision persistence engine: it holds a unique numeric type ID
> (`ObjType`), a VMT link (`VmtLink`), and pointers to the `Load` constructor
> and `Store` method.  The entire TStreamable / TStream subsystem was
> **dropped project-wide** (locked decision: "TStreamable dropped; serde if
> revived").  `rg "Stream|serde|Serialize" src/` finds only doc-comment
> references confirming the drop — no registration infrastructure exists.
>
> In magiblot's modern C++ port the equivalent mechanism is
> `TStreamableClass` / `TStreamableTypes` (tobjstrm.h) — still present for
> C++ ABI compatibility but also unused by the Rust port.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `ObjType` (record field) | 545 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); unique numeric type ID for stream dispatch |
| `VmtLink` (record field) | 545 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); VMT offset used by `Put` to look up registration |
| `Load` (record field) | 545 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); pointer to the class `Load` constructor |
| `Store` (record field) | 545 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); pointer to the class `Store` method |
| `Next` (record field) | 545 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); intrusive linked-list pointer into the global type registry |

## Summary

- PORTED: 0   EQUIVALENT: 0   NOT-PORTED: 5   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0 (no public symbols)   |   → concept: 0
- Notable findings: All 5 record fields are intentionally absent. TStreamRec is the registration spine of the entire Turbo Vision object-persistence system; with that system dropped, there is nothing for it to connect to. If serde serialization is ever revived, the `ObjType` / `VmtLink` mechanism would translate to a serde type-tag in an enum or a `typetag`-style trait-object dispatch — a natural Rust analog — but that is out of scope until the feature is explicitly revived.
