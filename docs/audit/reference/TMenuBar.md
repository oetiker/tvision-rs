# TMenuBar  (guide pp. 478–480)

Rust module(s): `src/menu/menu_bar.rs`   |   magiblot: `include/tvision/menus.h` / `source/tvision/tmenubar.cpp`

> TMenuBar is the horizontal one-row menu bar. Its bar-specific work is
> `draw` and `getItemRect`; all other behaviour is inherited from TMenuView.
> In tvision-rs, the `TMenuView` base becomes the `MenuView` trait (deviation D2).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Init` (constructor) | 479 | PORTED | OK | `tv::MenuBar::new(bounds: Rect, menu: Menu) -> MenuBar` | 3 | Guide: calls `TMenuView.Init`, sets `GrowMode` to `gfGrowHiX`, sets `Options` to `ofPreProcess`. Rust: `MenuBar::new` sets `grow_mode.hi_x = true` and `options.pre_process = true` — exact match. Unit test `ctor_sets_grow_and_preprocess` verifies both flags. Doc now explains the `bounds` convention, what both flags do, and when to use `MenuBar` vs `popup_menu` for a context popup. |
| `Done` (destructor) | 479 | NOT-PORTED | — | — | — | Guide: calls `TMenuView.Done` then `DisposeMenu`. Rust: `Menu` is owned and dropped automatically; no manual destructor needed. Deviation D12 (TStreamable/manual memory management dropped). |
| `Draw` (method) | 479 | PORTED | OK | `tv::MenuBar::draw` (impl `View::draw`) | 3 | Guide: draws bar with default palette, reads `Name` and `Disabled` fields, highlights `Current` item. Rust: fills row with `cNormal`, walks items left-to-right, skips separators (C++ `p->name == 0`), resolves color via `MenuColors::item(disabled, selected)`, renders `put_char` + `put_cstr` + `put_char` per item — exact structural match to the C++ `draw()`. Overflow guard (`x + l < size.x`) ported faithfully. Snapshot tests `snapshot_bar_highlight_and_disabled` and `snapshot_bar_narrow_drops_overflowing_item` exercise the key paths. Doc comment explains each step with C++ callout. Score 3. |
| `GetItemRect` (method) | 479 | PORTED | OK | `tv::MenuBar::get_item_rect` (impl `MenuView::get_item_rect`) | 3 | Guide: overrides the abstract TMenuView method; returns the rectangle occupied by the given item. Rust: left-to-right accumulator starting at `x = 1`, each named item advances by `cstrlen + 2`, separators carry `a.x = b.x` without advancing `b.x`. Unit tests `get_item_rect_accumulates_horizontally` and `get_item_rect_separator_consumes_no_x` verify both paths. The C++ uses a `TMenuItem *` pointer walk; Rust uses an index with matching semantics. Doc explains the accumulation rule and separator behaviour. Score 3. |
| `CMenuView` palette | 479 | EQUIVALENT | OK | `MenuColors::resolve(ctx)` → `Role::Menu*` via `Theme` | 3 | Guide: "Menu bars, like all menu views, use the default palette CMenuView to map onto the 2nd through 7th entries in the standard application palette." Rust: six `Role::Menu*` variants documented in `src/theme.rs` (theme pass) with color, chain, and widget context for each (e.g. `MenuNormal`: black on lightgray, `cpMenuView[1]=0x02 → cpAppColor[2]=0x70`). |

## Summary

- PORTED: 3   EQUIVALENT: 1   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- `MenuBar::new` raised to score 3. `CMenuView palette` raised to 3 in the theme.rs Role pass (all six `Role::Menu*` variants documented in `src/theme.rs`). All bar-specific methods (`draw`, `get_item_rect`) were already score 3.
