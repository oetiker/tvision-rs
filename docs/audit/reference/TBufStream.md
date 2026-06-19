# TBufStream  (guide pp. 383–386)

Rust module(s): N/A   |   magiblot: include/tvision/tobjstrm.h (fpstream / fpbase hierarchy); source/tvision/ (no separate TBufStream — magiblot uses C++ std streams)

> TBufStream is a buffered DOS file stream — a concrete descendant of TDosStream
> that adds an in-heap I/O buffer for efficiency.  The entire TStreamable /
> TStream subsystem was **dropped project-wide** (locked decision: "TStreamable
> dropped; serde if revived").  `rg "Stream|serde|Serialize" src/` finds only
> doc-comment references confirming the drop — no implementation exists.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `BufEnd` (field) | 405 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); offset to last used byte in heap buffer |
| `Buffer` (field) | 405 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); pointer to heap-allocated I/O buffer |
| `BufPtr` (field) | 405 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); current offset within heap buffer |
| `BufSize` (field) | 405 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); total heap buffer size in bytes |
| `Init` (constructor) | 405 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); opens named DOS file + allocates buffer |
| `Done` (destructor) | 384 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); flushes then frees buffer and closes file |
| `Flush` (method) | 384 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); writes dirty buffer to disk |
| `GetPos` (method) | 385 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); returns stream position (not buffer offset) |
| `GetSize` (method) | 385 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); flushes then returns total stream byte size |
| `Read` (method) | 385 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); low-level buffered byte read |
| `Seek` (method) | 385 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); flushes then repositions file pointer |
| `Truncate` (method) | 385 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); flushes then truncates file at current position |
| `Write` (method) | 385 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); low-level buffered byte write |

## Summary

- PORTED: 0   EQUIVALENT: 0   NOT-PORTED: 13   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0 (no public symbols)   |   → concept: 0
- Notable findings: All 4 fields and 9 methods are intentionally absent. The low-level `Read`/`Write`/`Seek`/`GetPos`/`GetSize` methods have idiomatic analogs in `std::io` (`Read`, `Write`, `Seek` traits), but since the whole object-persistence layer is dropped there is no consumer of them; they are NOT-PORTED, not EQUIVALENT.
