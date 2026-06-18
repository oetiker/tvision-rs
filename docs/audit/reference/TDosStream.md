# TDosStream  (guide pp. 419–420)

Rust module(s): N/A   |   magiblot: include/tvision/tobjstrm.h (fpbase / ifpstream / ofpstream / fpstream hierarchy)

> TDosStream is an unbuffered DOS file stream — the direct concrete descendant
> of TStream for named file I/O.  The entire TStreamable / TStream subsystem
> was **dropped project-wide** (locked decision: "TStreamable dropped; serde if
> revived").  `rg "Stream|serde|Serialize" src/` finds only doc-comment
> references confirming the drop — no implementation exists.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Handle` (field) | 419 | NOT-PORTED | — | — | — | DOS file handle (`Word`); TStreamable subsystem dropped + DOS-specific API with no Rust analog |
| `Init` (constructor) | 419 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); opens named DOS file with stCreate/stOpenRead/stOpenWrite/stOpen mode |
| `Done` (destructor) | 420 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); closes DOS file handle |
| `GetPos` (method) | 420 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); returns current file position |
| `GetSize` (method) | 420 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); returns total file size in bytes |
| `Read` (method) | 420 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); unbuffered DOS file read |
| `Seek` (method) | 420 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); positions DOS file pointer |
| `Truncate` (method) | 420 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); truncates DOS file at current position |
| `Write` (method) | 420 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); unbuffered DOS file write |

## Summary

- PORTED: 0   EQUIVALENT: 0   NOT-PORTED: 9   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0 (no public symbols)   |   → concept: 0
- Notable findings: All 1 field and 8 methods are intentionally absent. Two reasons compound here: (1) TStreamable subsystem dropped project-wide, and (2) DOS-specific file-handle API (`Handle: Word` using INT 21h calls) has no meaningful Rust analog — `std::fs::File` would be the idiomatic replacement but has no consumer since the whole persistence layer is dropped.
