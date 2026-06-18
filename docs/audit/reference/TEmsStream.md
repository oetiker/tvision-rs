# TEmsStream  (guide pp. 432–434)

Rust module(s): N/A   |   magiblot: not present (EMS/expanded-memory is DOS-only; magiblot's modern C++ port dropped it entirely)

> TEmsStream is a DOS EMS (Expanded Memory Specification) stream — stores
> stream data in hardware-banked EMS memory pages rather than on disk.
> This is doubly non-portable: (1) the entire TStreamable / TStream
> subsystem was **dropped project-wide** (locked decision: "TStreamable
> dropped; serde if revived"), and (2) EMS is a DOS 8086/286 memory-extension
> technology with no analog on any modern OS.  magiblot's own modern C++ port
> omits TEmsStream entirely.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Handle` (field) | 432 | NOT-PORTED | — | — | — | EMS handle (`Word`); DOS EMS-specific + TStreamable subsystem dropped project-wide (serde if revived) |
| `PageCount` (field) | 432 | NOT-PORTED | — | — | — | Number of allocated EMS 16 KB pages; DOS EMS-specific + TStreamable subsystem dropped project-wide (serde if revived) |
| `Position` (field) | 433 | NOT-PORTED | — | — | — | Current stream position; DOS EMS-specific + TStreamable subsystem dropped project-wide (serde if revived) |
| `Size` (field) | 433 | NOT-PORTED | — | — | — | Total stream size in bytes; DOS EMS-specific + TStreamable subsystem dropped project-wide (serde if revived) |
| `Init` (constructor) | 433 | NOT-PORTED | — | — | — | DOS EMS-specific + TStreamable subsystem dropped project-wide (serde if revived); allocates EMS pages for MinSize..MaxSize bytes |
| `Done` (destructor) | 434 | NOT-PORTED | — | — | — | DOS EMS-specific + TStreamable subsystem dropped project-wide (serde if revived); releases EMS pages |
| `GetPos` (method) | 434 | NOT-PORTED | — | — | — | DOS EMS-specific + TStreamable subsystem dropped project-wide (serde if revived) |
| `GetSize` (method) | 434 | NOT-PORTED | — | — | — | DOS EMS-specific + TStreamable subsystem dropped project-wide (serde if revived) |
| `Read` (method) | 434 | NOT-PORTED | — | — | — | DOS EMS-specific + TStreamable subsystem dropped project-wide (serde if revived) |
| `Seek` (method) | 434 | NOT-PORTED | — | — | — | DOS EMS-specific + TStreamable subsystem dropped project-wide (serde if revived) |
| `Truncate` (method) | 434 | NOT-PORTED | — | — | — | DOS EMS-specific + TStreamable subsystem dropped project-wide (serde if revived) |
| `Write` (method) | 434 | NOT-PORTED | — | — | — | DOS EMS-specific + TStreamable subsystem dropped project-wide (serde if revived) |

## Summary

- PORTED: 0   EQUIVALENT: 0   NOT-PORTED: 12   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0 (no public symbols)   |   → concept: 0
- Notable findings: All 4 fields and 8 methods are intentionally absent for a double reason — both the TStreamable subsystem drop and the DOS/EMS hardware dependency. magiblot's own modern C++ tvision also omits TEmsStream, confirming this is the correct treatment; nothing is missing from either the port or the reference implementation.
