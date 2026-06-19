# TStream  (guide pp. 542–545)

Rust module(s): N/A   |   magiblot: include/tvision/tobjstrm.h (pstream / ipstream / opstream hierarchy)

> TStream is the abstract base class for all object-persistence streams.
> The entire TStreamable / TStream subsystem was **dropped project-wide**
> (locked decision: "TStreamable dropped; serde if revived").
> `rg "Stream|serde|Serialize" src/` finds only doc-comment references
> confirming the drop — no implementation exists.
>
> `stXXXX` stream status constants and stream access-mode constants
> (guide p. 373, Table 19.34–19.35) are audited at the bottom of this file.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Status` (field) | 542 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived) |
| `ErrorInfo` (field) | 542 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived) |
| `CopyFrom` (method) | 543 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived) |
| `Error` (method) | 543 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived) |
| `Flush` (method) | 543 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived) |
| `Get` (method) | 543 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); `Get` reads a registered object from stream — the entire registration / type-id mechanism is dropped |
| `GetPos` (method, abstract) | 544 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived) |
| `GetSize` (method, abstract) | 544 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived) |
| `Put` (method) | 544 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); `Put` writes a registered object to stream |
| `Read` (method, abstract) | 544 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); low-level raw-byte read; no analog without the stream type |
| `ReadStr` (method) | 544 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived) |
| `Reset` (method) | 545 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived) |
| `Seek` (method, abstract) | 545 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived) |
| `Truncate` (method, abstract) | 545 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived) |
| `Write` (method, abstract) | 545 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived); low-level raw-byte write |
| `WriteStr` (method) | 545 | NOT-PORTED | — | — | — | TStreamable object-persistence subsystem dropped project-wide (serde if revived) |
| `stCreate` (access-mode constant) | 373 | NOT-PORTED | — | — | — | DOS file-open mode constant; TStreamable subsystem dropped project-wide (serde if revived) |
| `stOpenRead` (access-mode constant) | 373 | NOT-PORTED | — | — | — | DOS file-open mode constant; TStreamable subsystem dropped project-wide (serde if revived) |
| `stOpenWrite` (access-mode constant) | 373 | NOT-PORTED | — | — | — | DOS file-open mode constant; TStreamable subsystem dropped project-wide (serde if revived) |
| `stOpen` (access-mode constant) | 373 | NOT-PORTED | — | — | — | DOS file-open mode constant; TStreamable subsystem dropped project-wide (serde if revived) |
| `stOK` (error-status constant) | 373 | NOT-PORTED | — | — | — | TStreamable subsystem dropped project-wide (serde if revived) |
| `stError` (error-status constant) | 373 | NOT-PORTED | — | — | — | TStreamable subsystem dropped project-wide (serde if revived) |
| `stInitError` (error-status constant) | 373 | NOT-PORTED | — | — | — | TStreamable subsystem dropped project-wide (serde if revived) |
| `stReadError` (error-status constant) | 373 | NOT-PORTED | — | — | — | TStreamable subsystem dropped project-wide (serde if revived) |
| `stWriteError` (error-status constant) | 373 | NOT-PORTED | — | — | — | TStreamable subsystem dropped project-wide (serde if revived) |
| `stGetError` (error-status constant) | 373 | NOT-PORTED | — | — | — | TStreamable subsystem dropped project-wide (serde if revived); fired when `Get` reads an unregistered object type |
| `stPutError` (error-status constant) | 373 | NOT-PORTED | — | — | — | TStreamable subsystem dropped project-wide (serde if revived); fired when `Put` writes an unregistered object type |

## Summary

- PORTED: 0   EQUIVALENT: 0   NOT-PORTED: 27   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0 (no public symbols)   |   → concept: 0
- Notable findings: The entire TStream abstract hierarchy — including all 16 methods/fields and the 11 stXXXX status and access-mode constants (Table 19.34–19.35, guide p. 373) — is intentionally absent. The locked project decision "TStreamable dropped; serde if revived" covers every entry. No gaps; no unexpected omissions.
