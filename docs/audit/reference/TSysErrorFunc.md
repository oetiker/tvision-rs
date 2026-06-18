# TSysErrorFunc type  (guide pp. 550–551)

Rust module(s): (none found)   |   magiblot: include/tvision/drivers.h / source/platform/ (DOS critical-error hook)

> `TSysErrorFunc` is a single-entry type (a function-pointer typedef) with no
> fields and no methods beyond its declaration. The guide's "See also" references
> four related globals: `SysErrorFunc` (the variable holding the hook), `SystemError`
> (the default handler), `InitSysError` / `DoneSysError` (install/teardown).
> Those globals are audited as separate rows below because they are the operational
> surface that `TSysErrorFunc` types.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TSysErrorFunc` (type: `function(ErrorCode: Integer; Drive: Byte): Integer`) | 550 | NOT-PORTED | — | — | — | DOS INT 24h critical-error handler hook. The type exists solely to let applications install a custom handler for DOS hardware faults (drive-not-ready, write-protect, etc.). No analog in tvision-rs: the framework targets Unix/crossterm with no DOS critical-error concept. Intentionally absent — same rationale as all DOS/EMS platform machinery (known idiomatic mapping: DOS-era → no analog). |
| `SysErrorFunc` (global variable holding the installed hook) | 550 | NOT-PORTED | — | — | — | Pascal global `var SysErrorFunc: TSysErrorFunc` — the live hook pointer. No analog; omitted with TSysErrorFunc for the same reason. |
| `SystemError` (default handler function matching TSysErrorFunc signature) | 550 | NOT-PORTED | — | — | — | The built-in DOS critical-error handler. No analog. |
| `InitSysError` (install the hook) | 550 | NOT-PORTED | — | — | — | DOS-specific setup routine. No analog. |
| `DoneSysError` (teardown the hook) | 550 | NOT-PORTED | — | — | — | DOS-specific teardown routine. No analog. |

## Summary

- PORTED: 0   EQUIVALENT: 0   NOT-PORTED: 5   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: Entirely DOS INT 24h critical-error machinery; nothing to port. All five entries (the type itself plus its four companion globals) are intentionally absent. No code gap.
