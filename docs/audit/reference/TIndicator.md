# TIndicator  (guide pp. 458–460)

Rust module(s): `src/widgets/indicator.rs`   |   magiblot: `include/tvision/editors.h` / `source/tvision/tindictr.cpp`

> TIndicator has **two own fields** documented by the guide (`Location`,
> `Modified`), five documented entries (Init, Draw, GetPalette, SetState,
> SetValue), and one palette (`CIndicator`, 2 entries).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Location` (field) | 458 | PORTED | OK | `tv::Indicator::location: Point` | 3 | Raised: doc now explains zero-based coords, one-based display, and that callers should write only via `set_value` (which the pump invokes from `Deferred::IndicatorSetValue`). |
| `Modified` (field) | 458–459 | PORTED | OK | `tv::Indicator::modified: bool` | 3 | Raised: doc now explains the `☼` marker at column 0, that callers should not write directly, and that it is updated alongside `location` via `set_value`. |
| `Init` (constructor) | 459 | PORTED | OK | `tv::Indicator::new(bounds: Rect) -> Indicator` | 3 | Raised: doc now explains embed-in-editor-group usage, the deferred pump update path, grow_mode `lo_y | hi_y`, non-selectable policy, and a heritage note matching `gfGrowLoY | gfGrowHiY`. |
| `Draw` (method) | 459 | PORTED | OK | `tv::Indicator::draw` (impl `View::draw`) | 3 | Guide: draws `line:column` form, shows `☼` if Modified. C++: fills with `dragFrame`/`normalFrame` chars, `b.putChar(0,15)` for modified, `moveStr(8-colon_offset, s, color)`. Rust replicates all three steps faithfully. The counterintuitive C++ naming (`dragFrame` = `═` used when NOT dragging; `normalFrame` = `─` used while dragging) is explicitly called out in a comment. Colon-alignment at column 8 and the negative-start-col edge case are both documented and tested. Doc score 3: what + how + heritage section present. |
| `GetPalette` (method) | 459 | EQUIVALENT | OK | `tv::Role::IndicatorNormal` + `tv::Role::IndicatorDragging` via `tv::Theme` | 3 | Documented in `src/theme.rs` (theme pass): `Role::IndicatorNormal` (white on blue, `0x1F`, chain) and `Role::IndicatorDragging` (lightgreen on blue, `0x1A`, chain), both naming `Indicator` as the consumer. |
| `SetState` (method) | 459 | EQUIVALENT | OK | `View::set_state` default (no `Indicator` override) + whole-tree redraw (D9) | 2 | No Rust symbol to document — `Indicator` does not override `set_state`. Adding a doc comment here would require adding an impl method (code change), which is out of scope. The module-level comment already captures the deviation ("The dragging state is read live from the view state each frame"). Stays at score 2 (no code-free path to score 3). |
| `SetValue` (method) | 459 | EQUIVALENT | OK | `tv::Indicator::set_value` + `tv::view::Deferred::IndicatorSetValue` broker | 3 | Raised: doc now explains the intended call path (editor → `Deferred::IndicatorSetValue` → pump → `set_value`), the omitted no-op guard (whole-tree redraw makes it unnecessary), and a heritage note documenting the difference from C++ `setValue`. |
| `CIndicator` palette (2 entries) | 459 | EQUIVALENT | OK | `tv::Role::IndicatorNormal`, `tv::Role::IndicatorDragging` | 3 | Documented in `src/theme.rs` (theme pass) — see `GetPalette` row above. |
| `TStreamable` / stream support | 459 | NOT-PORTED | — | — | N/A | `TIndicator::build()` + stream constructor. Dropped per project-wide decision: `TStreamable` is not ported (serde-if-revived). |

## Summary

- PORTED: 4   EQUIVALENT: 4   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 1   |   → concept: 0
- Raised to 3: `location`, `modified`, `new`, `set_value` (4 symbols). `GetPalette`/`CIndicator` raised to 3 in the theme.rs Role pass.
- Remaining doc<3: `SetState` → no Indicator override exists, adding a doc comment would require a code change (out of scope).
- Notable finding: No missing or suspect items. The most important deviation (`set_value` drops the C++ no-op guard; update path goes through `Deferred::IndicatorSetValue`) is now fully documented in the rustdoc.
