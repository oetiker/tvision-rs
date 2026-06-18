# TScrollBar  (guide pp. 523–526)

Rust module(s): `src/widgets/scrollbar.rs`   |   magiblot: `include/tvision/views.h` / `source/tvision/tscrlbar.cpp`

> **Palette note:** `CScrollBar` has three entries: `[1]`=Page (→app palette 11),
> `[2]`=Arrows (→app palette 12), `[3]`=Indicator (→app palette 12 — same as Arrows).
> Rust collapses Arrows+Indicator into `Role::ScrollBarControls` (they shared one C++ slot)
> and exposes `Role::ScrollBarPage` separately — a faithful idiomatic mapping (D7).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `ArStep` (field) | 523 | PORTED | OK | `ScrollBar::arrow_step: i32` | 2 | Public field. Default 1. Guide says "amount added/subtracted when arrow area clicked." Doc states what it is; missing "how to set it" (use `set_step`/`set_params`). |
| `Max` (field) | 523 | PORTED | OK | `ScrollBar::max_value: i32` | 2 | Public field. Default 0. Doc explains what; missing cross-ref to `set_range`. |
| `Min` (field) | 523 | PORTED | OK | `ScrollBar::min_value: i32` | 2 | Public field. Default 0. Same gap. |
| `PgStep` (field) | 524 | PORTED | OK | `ScrollBar::page_step: i32` | 2 | Public field. Default 1. Doc explains what; missing "how to set". |
| `Value` (field) | 524 | PORTED | OK | `ScrollBar::value: i32` | 2 | Public field. Default 0. Doc explains what; missing "read-only" intent note (guide flags it Read only; Rust exposes it mutable — intentional for the broker downcast path). |
| `Init` (constructor) | 524 | PORTED | OK | `ScrollBar::new(bounds: Rect) -> ScrollBar` | 2 | Matches: value/min/max = 0, pgStep/arStep = 1, grow mode per orientation. `chars` init (vChars/hChars) becomes `Glyphs` in `Theme` (D7). Mouse-wheel event mask is implicit (crossterm delivers wheel without a mask). "When to use" + constructor note about glyphs could be added. |
| `Load` (constructor) | 524 | NOT-PORTED | — | — | — | `TStreamable` / stream serialisation dropped (serde-if-revived, known idiomatic mapping). |
| `Draw` (method) | 525 | PORTED | OK | `ScrollBar::draw` (impl `View::draw`) | 2 | Guide: draws from `Value`, `Bounds`, palette. Rust: draws in `draw` using `Glyphs` + `Role`. `drawPos` helper inlined into `draw`. Whole-tree redraw (D9) replaces the explicit `drawView` call in C++ `setParams`. Doc explains what; "how the glyph/role choices map to the C++ `drawPos` array" would reach score 3. |
| `GetPalette` (method) | 525 | EQUIVALENT | OK | `Role::ScrollBarPage` + `Role::ScrollBarControls` via `ctx.style()` | 2 | C++ returns `CScrollBar` (3-entry palette, page=slot4, arrows=indicator=slot5). Rust reads two `Role` variants from `Theme`. Arrows and Indicator share one C++ palette slot and one Rust role (`ScrollBarControls`) — faithful collapse. Known mapping: class Palette → `tv::Theme` (D7). Public method gone (absorbed into draw); N/A for doc score of the removed method; the roles themselves score 2. |
| `HandleEvent` (method) | 525 | PORTED | OK | `ScrollBar::handle_event` (impl `View::handle_event`) | 3 | Guide: mouse wheel (value ±3×step, broadcast clicked+changed), mouse down (arrow hold or thumb drag), key down (arrow/page/home/end). Rust: all cases ported verbatim from `tscrlbar.cpp`. The two modal loops become two discriminated track arms (`MouseAuto` vs `MouseMove`) gated by `tracked_part`. `ctrlToArrow` helper used. All cases covered with inline C++ line refs. Doc score 3 (the module-level doc explains both hold patterns and the track guards). |
| `ScrollDraw` (method) | 525 | PORTED | OK | `ScrollBar::scroll_draw` (private fn) | 2 | C++: `Message(Owner, evBroadcast, cmScrollBarChanged, @Self)`. Rust: `ctx.broadcast(Command::SCROLL_BAR_CHANGED, self.state().id())` — same semantics, `@Self` becomes a `ViewId source` (D3, D4). Private; doc on the method explains what; "override" guidance not applicable in Rust (non-virtual). |
| `ScrollStep` (method) | 525 | PORTED | OK | `Part::scroll_step(self, ar_step, pg_step) -> i32` (private) | 2 | C++ `scrollStep(part: int)` is `virtual` to allow override. Rust implementation is a private method on the `Part` enum — not overridable. No public override surface exists. This is a deliberate simplification (no subclass currently overrides it); if a future subclass needs it, it would need to be exposed. Doc on `Part::scroll_step` is inline and sufficient. |
| `SetParams` (method) | 525 | PORTED | OK | `ScrollBar::set_params(a_value, a_min, a_max, a_pg_step, a_ar_step, ctx)` | 2 | C++ calls `drawView()` first then `scrollDraw()` if value changed. Rust omits `draw_view()` — whole-tree redraw (D9) handles repaint; only `scroll_draw` (broadcast) is called when value changes. Deviation not explicitly called out in the method's doc comment (the module-level heritage section mentions D9 in context of `draw`). Doc score 2: what it does is explained; the D9 omission of draw_view is not noted. |
| `SetRange` (method) | 526 | PORTED | OK | `ScrollBar::set_range(a_min, a_max, ctx)` | 2 | Delegates to `set_params`. Matches C++. Doc explains what. |
| `SetStep` (method) | 526 | PORTED | OK | `ScrollBar::set_step(a_pg_step, a_ar_step, ctx)` | 2 | Delegates to `set_params`. Matches C++. Doc explains what. |
| `SetValue` (method) | 526 | PORTED | OK | `ScrollBar::set_value(a_value, ctx)` | 2 | Delegates to `set_params`. Matches C++. Doc explains what. |
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
| `sbHandleKeyboard` (option) | 524 | EQUIVALENT | OK | `ScrollBar::with_keyboard()` builder / `post_process` flag | 2 | C++ passes `sbHandleKeyboard` to `standardScrollBar`. Rust exposes `with_keyboard()` builder that sets `state.options.post_process`. Functionally identical. Doc explains what and when. |
| `CScrollBar` palette (3 entries) | 526 | EQUIVALENT | OK | `Role::ScrollBarPage` + `Role::ScrollBarControls` in `Theme` | 2 | See Palette note above. The roles are documented (what they are); the chain derivation (cpScrollBar→cpBlueWindow→cpAppColor) is in theme.rs comments but not in the scrollbar rustdoc itself. → concept: a guide section on palette chains would cover this. |

## Summary

- PORTED: 11   EQUIVALENT: 13   NOT-PORTED: 2   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 13   |   → concept: 1
- Notable finding: `set_params` omits the C++ `drawView()` call (D9 whole-tree redraw handles repaint) but does not call this out explicitly in its own doc comment — a reader searching for why the scrollbar does not call `draw_view` on a range change will find the answer only in the module-level heritage section, not at the method level.
