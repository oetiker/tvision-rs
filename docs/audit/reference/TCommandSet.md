# TCommandSet type  (guide pp. 411–412)

Rust module(s): src/command.rs   |   magiblot: include/tvision/views.h / source/tvision/tcmdset.cpp

> **Denylist note (commit A1 faabc78):** The framework's enabled-by-default policy is
> stored as a **disabled set** (denylist) in `Program`. `enable_cmd`/`disable_cmd`
> mean insert/remove regardless of polarity; `insert`/`remove` are
> polarity-neutral aliases preferred when the set's meaning is not "enabled commands".

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TCommandSet` (type declaration: `set of Byte`) | 411 | EQUIVALENT | OK | `tv::CommandSet` (`HashSet<Command>`) | 3 | C++ is a 256-bit array keyed by `0..=255` integer commands. Rust uses `HashSet<Command>` where `Command` is a namespaced `&'static str` (deviation D1, documented). The open/unbounded command space means there is no `all()` constructor (not enumerable), which is documented in the type-level rustdoc. Module-level and type-level doc both explain the deviation and the denylist pattern. |
| `has` / membership test | 411 | PORTED | OK | `CommandSet::has(cmd)` + `CommandSet::contains(cmd)` | 3 | Both documented: `has` is the faithful port name; `contains` the idiomatic alias. Doc now adds when to prefer each name and the denylist interpretation note. |
| `EnableCommands` / `+=` (enable a command) | 411 | PORTED | OK | `CommandSet::enable_cmd(cmd)` + `CommandSet::insert(cmd)` + `AddAssign<Command>` (`+=`) | 3 | All three forms documented: `enable_cmd` doc explains enabled-set meaning and denylist caveat; `insert` doc explains polarity-neutral naming rationale; `+=` operator doc adds incremental-build usage example. |
| `DisableCommands` / `-=` (disable a command) | 411 | PORTED | OK | `CommandSet::disable_cmd(cmd)` + `CommandSet::remove(cmd)` + `SubAssign<Command>` (`-=`) | 3 | `disable_cmd` doc explains greyout semantics and denylist caveat; `remove` polarity-neutral rationale present; `-=` operator notes revoke usage. |
| Set union (`+=` with another set) | 412 | PORTED | OK | `CommandSet::enable_set(other)` + `AddAssign<&CommandSet>` + `BitOrAssign<&CommandSet>` | 3 | `enable_set` doc adds post-call semantics and denylist re-enable note; `+=`/`\|=` operator docs distinguish Boolean-OR vs Pascal-add semantics. |
| Set difference (`-=` with another set) | 412 | PORTED | OK | `CommandSet::disable_set(other)` + `SubAssign<&CommandSet>` | 3 | `disable_set` doc adds post-call semantics (what self loses) and denylist-block note; `-=` operator doc present. |
| Set intersection (`*` in Pascal, bitwise AND) | 412 | PORTED | OK | `BitAndAssign<&CommandSet>` (`&=`) | 3 | `&=` operator doc now names Pascal `*` equivalence, explains retain-overlap semantics, and gives "all active views" use-case. |
| Initialization from set literal (`[0..255] - [cmZoom, ...]`) | 412 | EQUIVALENT | OK | `CommandSet::new()` (empty) + `+=` / `enable_cmd` calls | 3 | `new()` doc now explains the incremental-build pattern and introduces the denylist idiom (start empty, call `insert` per blocked command). The `+=` operator doc gives a concrete multi-step example. |

## Summary

- PORTED: 6   EQUIVALENT: 2   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: All semantics are present and correct. All 7 previously-score-2 rows raised to 3 in this pass: `new()` gains denylist-start pattern; `has`/`contains` gain alias-selection guidance and denylist-interpretation note; `enable_cmd`/`disable_cmd` gain semantic context and denylist caveats; `insert`/`remove` gain polarity-neutral rationale inline (no longer only in the type doc); `enable_set`/`disable_set` gain post-call semantics; `BitAndAssign &=` gains Pascal `*` mapping and overlap use-case; all operator impls gain how/when context.
