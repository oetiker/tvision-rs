# TMenu type  (guide pp. 477–478)

Rust module(s): `src/menu/mod.rs`   |   magiblot: `include/tvision/menus.h` / `source/tvision/menu.cpp`

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Items` (field, `PMenuItem`) | 477 | EQUIVALENT | OK | `Menu::items: Vec<MenuItem>` | 3 | C++ linked list via pointer becomes an owned `Vec`. Known idiomatic mapping (D1). Doc now covers what, how to iterate/modify, and the session-clone caveat (changes during an open session are invisible until the next activation). |
| `Default` (field, `PMenuItem`) | 477 | EQUIVALENT | OK | `Menu::default: Option<usize>` | 3 | C++ pointer to the default item becomes an index into `items` (or `None`). D1. Doc now covers what, how to override it after building, and how the session clones and reads it once as the initial highlight (re-activating always restarts on this index). |
| `NewMenu` (constructor function) | 477 | EQUIVALENT | OK | `Menu::builder() -> MenuBuilder` + `MenuBuilder::build()` | 3 | C++ `NewMenu(items)` heap-allocates a `TMenu` with `items = deflt = &itemList`. Rust uses a fluent `MenuBuilder`; `Menu::builder()` doc now explains when to use builder vs struct literal (struct literal for non-first default). |
| `DisposeMenu` (destructor procedure) | 477 | NOT-PORTED | — | — | — | DOS Pascal manual memory management; Rust ownership + `Drop` makes an explicit `DisposeMenu` unnecessary. |
| default `TMenu()` (empty constructor) | 477 | EQUIVALENT | OK | `Menu::default()` (`#[derive(Default)]`) | 3 | C++ `TMenu()` sets `items = deflt = 0`. Rust `Default` gives `items: vec![], default: None`. The struct-level `Menu` doc now explicitly mentions `Default::default()` gives an empty menu with no pre-selection, and a doctest demonstrates the struct literal. Covered. |
| `TMenu(itemList)` (single-arg constructor) | 477 | EQUIVALENT | OK | `MenuBuilder` first `.item()` / `.command()` / `.submenu()` call | 3 | Sets `items = deflt = &itemList` (first item is the default). `MenuBuilder::item()` sets `default = Some(0)` on first push — same semantics. Covered by builder doc. |
| `TMenu(itemList, theDefault)` (two-arg constructor) | 477 | EQUIVALENT | OK | `Menu { items: ..., default: Some(n) }` struct literal (escape hatch) | 3 | C++ lets caller pass a separate default item. Rust: the builder always uses `Some(0)`; `MenuBuilder` doc now explicitly calls out the struct-literal escape hatch for a non-first default, with `menu.default = Some(n)` override as the idiomatic alternative. |

## Summary

- PORTED: 0   EQUIVALENT: 6   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- All previously below-bar items raised to score 3. The two-arg `TMenu(itemList, theDefault)` constructor is now documented in `MenuBuilder` as a struct-literal escape hatch (`menu.default = Some(n)` after build).
