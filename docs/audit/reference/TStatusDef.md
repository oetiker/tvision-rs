# TStatusDef  (guide pp. 537–538)

Rust module(s): src/status/mod.rs   |   magiblot: include/tvision/menus.h (TStatusDef) / source/tvision/tstatusl.cpp

> TStatusDef is a plain record (no inheritance). The guide documents four record
> fields and one constructor (`NewStatusDef`). The magiblot C++ header also
> exposes the operator+ builder syntax. All are listed below.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `next` (field) | 537 | EQUIVALENT | OK | `Vec<StatusDef>` owned by `StatusLine` | N/A | C++: `TStatusDef *next` — singly-linked list node; ownership threaded through the `TStatusLine` destructor. Rust: the `Vec<StatusDef>` owned by `StatusLine` replaces the entire linked list. Known idiomatic mapping: linked list → Vec. Private implementation detail; no public field. |
| `min` (field) | 537 | EQUIVALENT | OK | `StatusDef.range: HelpCtxRange` (`HelpCtxRange::All` or `HelpCtxRange::OneOf`) | 3 | C++: `ushort min` — lower bound of the integer help-context range `[min, max]`. Rust: the numeric range is replaced by `HelpCtxRange`, which is either `All` (matches any context) or `OneOf(Vec<HelpCtx>)` (matches named membership set). Doc score 3 — field doc adds "place specific defs before All", the C++ `min`/`max` heritage is named in the `# Turbo Vision heritage` section, and deviation D1 rationale is inline. |
| `max` (field) | 537 | EQUIVALENT | OK | `StatusDef.range: HelpCtxRange` (same field, upper bound subsumed) | 3 | C++: `ushort max` — upper bound of the `[min, max]` range. Rust: absorbed into `HelpCtxRange` alongside `min`; no separate field. Same field doc as `min` row above. |
| `items` (field) | 537 | EQUIVALENT | OK | `StatusDef.items: Vec<StatusItem>` | 3 | C++: `TStatusItem *items` — pointer to the first item in a singly-linked list. Rust: `Vec<StatusItem>`. Known idiomatic mapping: linked list → Vec. Public field. Doc score 3 — field doc adds how to build (StatusItemsBuilder via StatusDef::list), Vec order = on-screen order, and None-text items. |
| `NewStatusDef` (constructor) | 538 | EQUIVALENT | OK | `StatusDef::list() -> StatusDefListBuilder` + `.def_all()` / `.def_one_of()` / `.build()` | 3 | C++: `NewStatusDef(AMin, AMax)` — a macro/function that creates a `TStatusDef` node setting `min` and `max`, returning a reference for chaining via `operator+`. Rust: replaced by the fluent `StatusDefListBuilder` — `StatusDef::list().def_all(|d| …).build()`. The builder pattern is the idiomatic Rust analog of the C++ operator+ chain. Fully documented including the "escape hatch" `.def()` method. |
| `operator+` (C++ builder chain) | — | EQUIVALENT | OK | `StatusDefListBuilder` chaining (`.def_all`, `.def_one_of`, `.def`) | 3 | C++ `operator+(TStatusDef&, TStatusItem&)` and `operator+(TStatusDef&, TStatusDef&)` compose the definition chain. Rust: all composition is via the builder; the operators have no direct Rust counterpart. Known idiomatic mapping. Doc score 3 — struct-level doc explains start (StatusDef::list), append methods, build, and the first-match-wins ordering rule. |

## Summary

- PORTED: 0   EQUIVALENT: 6   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: All public symbols at score 3. `range` field now names the C++ `min`/`max` heritage in a heritage section with first-match-wins ordering guidance; `items` adds how-to-build context; `StatusDefListBuilder` struct doc covers the full obtain-chain-build flow with the ordering rule.
