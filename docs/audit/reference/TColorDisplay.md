# TColorDisplay  (guide p. 409)

Rust module(s): `src/dialog/colorpick/info.rs`   |   magiblot: `include/tvision/colorsel.h`

> **Note:** The guide (p. 409) says "Details of `TColorDisplay`'s fields and
> methods are in the online Help." Only three methods appear in the class
> declaration in `colorsel.h`.  The Rust counterpart is `InfoColumn` — the
> always-visible right column in `ColorPicker` that shows old/new swatches and a
> variant readout.  This is an EQUIVALENT rebuild, not a literal port.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Init` (constructor) | 409 | EQUIVALENT | OK | `InfoColumn::new(bounds, model, old)` (`src/dialog/colorpick/info.rs:18`) | N/A | `InfoColumn` is `pub(crate)` — internal to the colorpick module. Not held to the public doc bar. C++ takes `bounds` + a text string; Rust takes `bounds + SharedModel + initial_color`. |
| `draw` (method) | 409 | EQUIVALENT | OK | `InfoColumn::draw` (`src/dialog/colorpick/info.rs:35`) | N/A | `InfoColumn` is `pub(crate)` — internal. C++ drew text in fg/bg attribute; Rust draws old/new swatches + variant readout. Module doc explains the layout. |
| `handleEvent` / `HandleEvent` (method) | 409 | EQUIVALENT | OK | `InfoColumn::handle_event` (`src/dialog/colorpick/info.rs:77`) | N/A | `InfoColumn` is `pub(crate)` — internal. Rust's `handle_event` is a no-op (passive view — the shared `ColorModel` Rc drives redraws automatically). |
| `setColor` / `SetColor` (method) | 409 | EQUIVALENT | OK | Implicit via `SharedModel` (`src/dialog/colorpick/model.rs`) — `ColorModel::set_color` + redraw | N/A | `InfoColumn` is `pub(crate)` — internal. `InfoColumn` reads `model.borrow().color` on every `draw` call; no explicit setter needed. |
| `color` (field, `TColorAttr *`) | 409 | EQUIVALENT | OK | `InfoColumn.old: Color` + `model: SharedModel` (`src/dialog/colorpick/info.rs:13–14`) | N/A | Private field.  C++ held a pointer to the current color attr; Rust holds the initial "old" color plus a shared model reference.  Equivalent storage for the two-swatch concept. |
| `text` (field, `const char *`) | 409 | NOT-PORTED | — | — | — | C++ displayed a fixed text string in the selected colors; Rust replaced this with a structured variant readout.  Capability superseded by the rebuilt display. |

## Summary

- PORTED: 0   EQUIVALENT: 5   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable: `InfoColumn` is `pub(crate)` — all methods are internal to the colorpick module and are not held to the public doc bar. All previously-below-bar rows re-scored N/A. The `text` field is NOT-PORTED because the rebuilt `InfoColumn` shows structured old/new swatches and a variant readout instead — a richer, equivalent capability.
