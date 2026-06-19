# TMenuItem type  (guide p. 481)

Rust module(s): `src/menu/mod.rs`   |   magiblot: `include/tvision/menus.h` / `source/tvision/menu.cpp`

> The guide documents `TMenuItem` as a Pascal record with 7 named fields plus a
> variant part (`case Integer of`), and three constructor functions (`NewItem`,
> `NewLine`, `NewSubMenu`). `TSubMenu` (a subclass of `TMenuItem` in the C++ port)
> is also noted; in the Rust port all three are folded into one `MenuItem` enum.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Next` (field, `PMenuItem`) | 481 | EQUIVALENT | OK | implicit `Vec` membership in `Menu::items` | N/A | C++ singly-linked list threading. Rust: items live in `Menu::items: Vec<MenuItem>`; "next" is array succession. Known idiomatic mapping (D1). No public `next` field needed. |
| `Name` (field, `PString`) | 481 | EQUIVALENT | OK | `MenuItem::Command { name: String, .. }` / `MenuItem::SubMenu { name: String, .. }` | 3 | Field doc explains tilde-hotkey syntax and when to use it. |
| `Command` (field, `Word`) | 481 | EQUIVALENT | OK | `MenuItem::Command { command: Command, .. }` | 3 | Field doc explains what the command is, when it's posted, and how graying interacts. |
| `Disabled` (field, `Boolean`) | 481 | PORTED | OK | `MenuItem::Command { disabled: bool, .. }` / `MenuItem::SubMenu { disabled: bool, .. }` | 3 | Field doc explains runtime graying by the menu session, when to read vs. mutate, and `disabled_mut()` usage. |
| `KeyCode` (field, `Word`) | 481 | EQUIVALENT | OK | `MenuItem::Command { key_code: Option<KeyEvent>, .. }` / `MenuItem::SubMenu { key_code: Option<KeyEvent>, .. }` | 3 | Field doc links `alt()` and explains how to construct other accelerator types. |
| `HelpCtx` (field, `Word`) | 481 | PORTED | OK | `MenuItem::Command { help_ctx: HelpCtx, .. }` / `MenuItem::SubMenu { help_ctx: HelpCtx, .. }` | 3 | Field doc explains `NO_CONTEXT` default and when to supply a custom value. |
| `Param` (field, variant case 0, `PString`) | 481 | EQUIVALENT | OK | `MenuItem::Command { param: Option<String>, .. }` | 3 | Field doc explains display-text-only nature and empty-string-to-None coercion. |
| `SubMenu` (field, variant case 1, `PMenu`) | 481 | EQUIVALENT | OK | `MenuItem::SubMenu { menu: Menu, .. }` | 3 | Field doc explains owned semantics vs. C++ raw pointer and how to build. |
| `NewItem` (constructor function) | 481 | EQUIVALENT | OK | `MenuBuilder::command_key` / `MenuBuilder::command` | 3 | Both builder methods now carry explicit C++ heritage notes (`NewItem` signature equivalents). |
| `NewLine` (constructor function) | 481 | EQUIVALENT | OK | `MenuBuilder::separator` | 3 | `separator()` doc names both `NewLine` and `newLine()` spellings from the C++ API. |
| `NewSubMenu` (constructor function) | 481 | EQUIVALENT | OK | `MenuBuilder::submenu` | 3 | `submenu()` doc explains closure pattern, the `help_ctx` limitation, and the `MenuItem::item` escape hatch. Notable finding now documented on the method. |
| `TSubMenu` (subclass, magiblot C++) | 481 | EQUIVALENT | OK | `MenuItem::SubMenu { .. }` variant | 3 | `MenuItem::SubMenu` variant doc explains the C++ subclass fold-in and when to use this variant vs. the builder. |
| `operator+` overloads (magiblot C++) | 481 | EQUIVALENT | OK | `MenuBuilder` chaining (`.command().submenu()...`) | 3 | `MenuBuilder` struct doc names the `operator+` heritage explicitly with a working example. |

## Summary

- PORTED: 2   EQUIVALENT: 11   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable finding (now documented): `MenuBuilder::submenu()` uses `HelpCtx::NO_CONTEXT` and provides no direct way to set a custom `help_ctx` for the submenu item — the `MenuBuilder::item` + `MenuItem::SubMenu` literal escape hatch is now documented on the method.
