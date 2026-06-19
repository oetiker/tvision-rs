# TMenuBox  (guide pp. 480–481)

Rust module(s): `src/menu/menu_box.rs`   |   magiblot: `include/tvision/menus.h` / `source/tvision/tmenubox.cpp`

> TMenuBox is the framed vertical drop-down menu box. It adjusts its bounds
> to fit the menu items, casts a shadow, and draws a framed column.
> In tvision-rs, the `TMenuView` base becomes the `MenuView` trait (deviation D2).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Init` (constructor) | 480 | PORTED | OK | `tv::MenuBox::new(bounds: Rect, menu: Menu) -> MenuBox` | 3 | Guide: adjusts `Bounds` to fit width and height of items, calls `TMenuView.Init`, sets `ofPreProcess`, sets `sfShadow`, sets `Menu` and `ParentMenu`. Rust: `menu_box_rect(bounds, &menu)` computes the fitted rect, then `ViewState` sets `state.state.shadow = true` and `options.pre_process = true`. `ParentMenu` is not stored on the struct (deviation D3 — parent managed by session stack). Doc now explains the `bounds` hint convention, what both flags do, and that under normal use the session constructs boxes on your behalf. |
| `Draw` (method) | 480 | PORTED | OK | `tv::MenuBox::draw` (impl `View::draw`) | 3 | Guide: draws the framed menu box and menu items in default colors. Rust: draws top border via `frame_line(Top)`, iterates items, for each separator draws `frame_line(Separator)`, for each named item draws `frame_line(Middle)` with interior filled in per-item colour, then the label with `put_cstr`, then either `►` marker at `size.x-4` (submenu, C++ `b.putChar(size.x-4, 16)`) or right-aligned `param` text. Bottom border via `frame_line(Bottom)`. The inset-frame convention (columns 0 and `size.x-1` are blanks) is documented in the module doc. Snapshot test `snapshot_box_frame_highlight_disabled_separator_param_submenu` exercises every branch. Score 3. |
| `GetItemRect` (method) | 481 | PORTED | OK | `tv::MenuBox::get_item_rect` (impl `MenuView::get_item_rect`) | 3 | Guide: overrides TMenuView abstract method; returns the rect of the given item. Rust: closed-form `y = 1 + index as i32`, `x` span `[2, size.x-2)` — every item (separators included) occupies one row, so no walk is needed. C++ walks a pointer list to find the index; Rust uses the direct formula. Unit test `get_item_rect_counts_rows_from_one_including_separators` verifies separators still advance `y`. Doc explains closed-form and contrast with the bar's walk. Score 3. |
| `CMenuView` palette | 481 | EQUIVALENT | OK | `MenuColors::resolve(ctx)` → `Role::Menu*` via `Theme` | 3 | Guide: "Menu boxes, like all menu views, use the default palette CMenuView to map onto the 2nd through 7th entries in the standard application palette." Rust: six `Role::Menu*` variants documented in `src/theme.rs` (theme pass) with color, chain, and widget context. |
| `menu_box_rect` sizing helper | — | PORTED | OK | `tv::menu_box::menu_box_rect(bounds: Rect, menu: &Menu) -> Rect` | 3 | Not directly in the guide (static `getRect` in `tmenubox.cpp`). Port is exact. Doc now spells out the per-column accounting formula for each item type (command with shortcut, submenu with `►`, plain command, separator), and notes when to call it directly vs relying on the session. Four unit tests verify width/height/clamping/submenu discriminator. |
| `frameLine` (private method) | — | PORTED | OK | `MenuBox::frame_line` (private method) | N/A | Not in the guide (private `frameLine` in `tmenubox.cpp`). Rust `frame_line` takes a `FrameKind` enum instead of `short n` offset — cleaner than the C++ array-offset encoding. The inset layout (`cols 0 and size.x-1 are blank`) is documented in the module doc. Private; N/A doc score. |

## Summary

- PORTED: 5   EQUIVALENT: 1   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- `MenuBox::new` and `menu_box_rect` raised to score 3. `CMenuView palette` raised to 3 in the theme.rs Role pass (all six `Role::Menu*` variants documented in `src/theme.rs`). All other public symbols were already score 3.
