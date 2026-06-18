# TCommandSet type  (guide pp. 411–412)

Rust module(s): src/command.rs   |   magiblot: include/tvision/views.h / source/tvision/tcmdset.cpp

> **Denylist note (commit A1 faabc78):** The framework's enabled-by-default policy is
> stored as a **disabled set** (denylist) in `Program`. `enable_cmd`/`disable_cmd`
> mean insert/remove regardless of polarity; `insert`/`remove` are
> polarity-neutral aliases preferred when the set's meaning is not "enabled commands".

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TCommandSet` (type declaration: `set of Byte`) | 411 | EQUIVALENT | OK | `tv::CommandSet` (`HashSet<Command>`) | 3 | C++ is a 256-bit array keyed by `0..=255` integer commands. Rust uses `HashSet<Command>` where `Command` is a namespaced `&'static str` (deviation D1, documented). The open/unbounded command space means there is no `all()` constructor (not enumerable), which is documented in the type-level rustdoc. Module-level and type-level doc both explain the deviation and the denylist pattern. |
| `has` / membership test | 411 | PORTED | OK | `CommandSet::has(cmd)` + `CommandSet::contains(cmd)` | 2 | `has` is the faithful port name; `contains` is the idiomatic alias. Both documented (what). "When to prefer which alias" could be expanded. |
| `EnableCommands` / `+=` (enable a command) | 411 | PORTED | OK | `CommandSet::enable_cmd(cmd)` + `CommandSet::insert(cmd)` + `AddAssign<Command>` (`+=`) | 2 | All three forms documented (what). The `insert` alias's rationale (polarity-neutral for denylist use) is explained in the type doc but not on the method itself. |
| `DisableCommands` / `-=` (disable a command) | 411 | PORTED | OK | `CommandSet::disable_cmd(cmd)` + `CommandSet::remove(cmd)` + `SubAssign<Command>` (`-=`) | 2 | Same as above — method doc explains what, polarity rationale in type doc. |
| Set union (`+=` with another set) | 412 | PORTED | OK | `CommandSet::enable_set(other)` + `AddAssign<&CommandSet>` + `BitOrAssign<&CommandSet>` | 2 | Three forms; operator impls are doc-commented (what). |
| Set difference (`-=` with another set) | 412 | PORTED | OK | `CommandSet::disable_set(other)` + `SubAssign<&CommandSet>` | 2 | Two forms; documented (what). |
| Set intersection (`*` in Pascal, bitwise AND) | 412 | PORTED | OK | `BitAndAssign<&CommandSet>` (`&=`) | 2 | Pascal uses `*` for set intersection; Rust exposes `&=`. Operator doc comments say "set intersection". No mention of the Pascal symbol mapping. |
| Initialization from set literal (`[0..255] - [cmZoom, ...]`) | 412 | EQUIVALENT | OK | `CommandSet::new()` (empty) + `+=` / `enable_cmd` calls | 1 | C++ allows Pascal set-literal initialization. Rust has no set-literal syntax; callers build incrementally. The `new()` doc only says "An empty command set" — no example of the common pattern. |

## Summary

- PORTED: 5   EQUIVALENT: 2   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 7   |   → concept: 0
- Notable findings: All semantics are present and correct. The most pervasive doc gap is that individual method rustdocs score 2 (what, not how/when) — in particular the `insert`/`remove` polarity-neutral alias rationale lives only in the type-level doc, not on the methods themselves, so readers who arrive at the method directly miss the denylist context.
