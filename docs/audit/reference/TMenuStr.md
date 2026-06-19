# TMenuStr type  (guide p. 482)

Rust module(s): `src/menu/mod.rs`   |   magiblot: `include/tvision/menus.h`

> `TMenuStr` is a Pascal string alias (`string[31]`) used as the `Name` parameter
> type in `NewItem` and `NewSubMenu`. It imposes a 31-character maximum on menu
> item titles.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TMenuStr` type alias (`string[31]`) | 482 | NOT-PORTED | — | — | — | Pascal fixed-length string alias with a 31-char cap. Rust uses `String` (or `&str` / `impl Into<String>`) in builder methods — no artificial length limit. The 31-char cap is a DOS-era constraint with no value in Rust; silently exceeded strings will simply render wider than a classic TV terminal expected, which is a cosmetic concern at most. Not ported; no Rust equivalent exists or is needed. |

## Summary

- PORTED: 0   EQUIVALENT: 0   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable finding: The 31-character menu title limit is silently dropped with no documentation anywhere in the Rust codebase; a brief note in `MenuBuilder` or the module doc would help users who migrate from C++ TV and wonder whether the limit still applies.
