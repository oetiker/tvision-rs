# TScrollBar  (guide pp. 523–526)

Rust module(s): `src/widgets/scrollbar.rs`   |   magiblot: `include/tvision/views.h` / `source/tvision/tscrlbar.cpp`

> **Palette note:** `CScrollBar` has three entries: `[1]`=Page (→app palette 11),
> `[2]`=Arrows (→app palette 12), `[3]`=Indicator (→app palette 12 — same as Arrows).
> Rust collapses Arrows+Indicator into `Role::ScrollBarControls` (they shared one C++ slot)
> and exposes `Role::ScrollBarPage` separately — a faithful idiomatic mapping (D7).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `ArStep` (field) | 523 | PORTED | OK | `ScrollBar::arrow_step: i32` | 3 | Added "how to set": `set_step`/`set_params` cross-refs + default. |
| `Max` (field) | 523 | PORTED | OK | `ScrollBar::max_value: i32` | 3 | Added cross-ref to `set_range`/`set_params` + broadcast note. |
| `Min` (field) | 523 | PORTED | OK | `ScrollBar::min_value: i32` | 3 | Same gap closed. |
| `PgStep` (field) | 524 | PORTED | OK | `ScrollBar::page_step: i32` | 3 | Added "how to set": `set_step`/`set_params` cross-refs. |
| `Value` (field) | 524 | PORTED | OK | `ScrollBar::value: i32` | 3 | Added read-only intent note + broker downcast path explanation. |
| `Init` (constructor) | 524 | PORTED | OK | `ScrollBar::new(bounds: Rect) -> ScrollBar` | 3 | Added orientation inference, grow-mode, not-selectable rationale, glyph note, and "call set_params after insert" guidance. |
| `Load` (constructor) | 524 | NOT-PORTED | — | — | — | `TStreamable` / stream serialisation dropped (serde-if-revived, known idiomatic mapping). |
| `Draw` (method) | 525 | PORTED | OK | `ScrollBar::draw` (impl `View::draw`) | 3 | Added glyph/role table showing how the five zones map to `drawPos` equivalents + empty-range behaviour. |
| `GetPalette` (method) | 525 | EQUIVALENT | OK | `Role::ScrollBarPage` + `Role::ScrollBarControls` via `ctx.style()` | N/A | Public method gone (absorbed into draw) — N/A for removed method itself. Module-level `# Colors` section now documents both roles, their palette-chain derivation (cpScrollBar→cpBlueWindow→cpAppColor), and the Arrows+Indicator collapse. |
| `HandleEvent` (method) | 525 | PORTED | OK | `ScrollBar::handle_event` (impl `View::handle_event`) | 3 | Unchanged — was already score 3. |
| `ScrollDraw` (method) | 525 | PORTED | OK | `ScrollBar::scroll_draw` (private fn) | N/A | Private — doc sufficient for internal use; not held to public bar. |
| `ScrollStep` (method) | 525 | PORTED | OK | `Part::scroll_step(self, ar_step, pg_step) -> i32` (private) | N/A | Private method on private enum — not held to public bar. |
| `SetParams` (method) | 525 | PORTED | OK | `ScrollBar::set_params(a_value, a_min, a_max, a_pg_step, a_ar_step, ctx)` | 3 | Added explicit note that `draw_view()` is omitted (D9 whole-tree redraw handles repaint) + pointer to convenience wrappers. |
| `SetRange` (method) | 526 | PORTED | OK | `ScrollBar::set_range(a_min, a_max, ctx)` | 3 | Added "when to use" (content size changes) + broadcast note. |
| `SetStep` (method) | 526 | PORTED | OK | `ScrollBar::set_step(a_pg_step, a_ar_step, ctx)` | 3 | Added "when to use" (viewport resize) + no-broadcast note. |
| `SetValue` (method) | 526 | PORTED | OK | `ScrollBar::set_value(a_value, ctx)` | 3 | Added "when to use" (owner scrolled programmatically) + broadcast note. |
| `Store` (method) | 526 | NOT-PORTED | — | — | — | `TStreamable` / stream serialisation dropped (serde-if-revived). |
| `sbLeftArrow` (constant) | 523 | EQUIVALENT | OK | `Part::LeftArrow` (private enum variant) | N/A | Private — used only inside `handle_event`. No public constant needed; `Part` is crate-internal. |
| `sbRightArrow` (constant) | 523 | EQUIVALENT | OK | `Part::RightArrow` | N/A | Same. |
| `sbPageLeft` (constant) | 523 | EQUIVALENT | OK | `Part::PageLeft` | N/A | Same. |
| `sbPageRight` (constant) | 523 | EQUIVALENT | OK | `Part::PageRight` | N/A | Same. |
| `sbUpArrow` (constant) | 523 | EQUIVALENT | OK | `Part::UpArrow` | N/A | Same. |
| `sbDownArrow` (constant) | 523 | EQUIVALENT | OK | `Part::DownArrow` | N/A | Same. |
| `sbPageUp` (constant) | 523 | EQUIVALENT | OK | `Part::PageUp` | N/A | Same. |
| `sbPageDown` (constant) | 523 | EQUIVALENT | OK | `Part::PageDown` | N/A | Same. |
| `sbIndicator` (constant) | 523 | EQUIVALENT | OK | `Part::Indicator` | N/A | Same. |
| `sbHorizontal` (option) | 524 | EQUIVALENT | OK | orientation inferred from `bounds` in `ScrollBar::new` (width==1 → vertical) | N/A | C++ `sbHorizontal=0` / `sbVertical=1` passed to `standardScrollBar`. Rust derives orientation at construction; no separate constant needed. Deviation is documented in `new`. |
| `sbVertical` (option) | 524 | EQUIVALENT | OK | orientation inferred from `bounds` in `ScrollBar::new` | N/A | Same. |
| `sbHandleKeyboard` (option) | 524 | EQUIVALENT | OK | `ScrollBar::with_keyboard()` builder / `post_process` flag | 3 | Expanded: added "when to use" (pane with focused content), builder call example, and `Window::standard_scroll_bar` cross-ref. |
| `CScrollBar` palette (3 entries) | 526 | EQUIVALENT | OK | `Role::ScrollBarPage` + `Role::ScrollBarControls` in `Theme` | 3 | Module-level `# Colors` section added: both roles documented with classic-blue palette chain (cpScrollBar→cpBlueWindow→cpAppColor slots 11–12) and the Arrows+Indicator collapse rationale. |

## Summary

- PORTED: 14   EQUIVALENT: 14   NOT-PORTED: 2   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- All previously below-bar public symbols raised to score 3. Private symbols (`scroll_draw`, `Part::scroll_step`) re-scored N/A (not held to public bar). The `GetPalette` removed-method row is N/A; its corresponding roles are now score 3 via the module-level `# Colors` section.
