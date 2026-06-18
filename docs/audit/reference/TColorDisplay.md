# TColorDisplay  (guide p. 409)

Rust module(s): `src/dialog/colorpick/info.rs`   |   magiblot: `include/tvision/colorsel.h`

> **Note:** The guide (p. 409) says "Details of `TColorDisplay`'s fields and
> methods are in the online Help." Only three methods appear in the class
> declaration in `colorsel.h`.  The Rust counterpart is `InfoColumn` — the
> always-visible right column in `ColorPicker` that shows old/new swatches and a
> variant readout.  This is an EQUIVALENT rebuild, not a literal port.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Init` (constructor) | 409 | EQUIVALENT | OK | `InfoColumn::new(bounds, model, old)` (`src/dialog/colorpick/info.rs:18`) | 2 | C++ takes `bounds` + a text string; Rust takes `bounds + SharedModel + initial_color`.  The color-preview role is equivalent; the text-swatch approach differs (Rust shows old/new swatches instead of a text string in the selected colors). |
| `draw` (method) | 409 | EQUIVALENT | OK | `InfoColumn::draw` (`src/dialog/colorpick/info.rs:35`) | 2 | C++ drew its text string in the chosen fg/bg attribute.  Rust draws "Old:" and "New:" swatches (solid-color cells) plus a variant readout string — richer but covers the same concept.  Module doc explains the layout. |
| `handleEvent` / `HandleEvent` (method) | 409 | EQUIVALENT | OK | `InfoColumn::handle_event` (`src/dialog/colorpick/info.rs:77`) | 1 | C++ responded to `cmNewColorIndex` to call `setColor`; Rust's `handle_event` is a no-op (passive view — the shared `ColorModel` Rc drives redraws automatically). Equivalent: the display always reflects current state. Doc score 1. |
| `setColor` / `SetColor` (method) | 409 | EQUIVALENT | OK | Implicit via `SharedModel` (`src/dialog/colorpick/model.rs`) — `ColorModel::set_color` + redraw | 1 | C++ called `setColor(aColor)` to push a new color to the display.  Rust: `InfoColumn` reads `model.borrow().color` on every `draw` call; no explicit setter needed.  Equivalent. Doc score 1 (module comment only). |
| `color` (field, `TColorAttr *`) | 409 | EQUIVALENT | OK | `InfoColumn.old: Color` + `model: SharedModel` (`src/dialog/colorpick/info.rs:13–14`) | N/A | Private field.  C++ held a pointer to the current color attr; Rust holds the initial "old" color plus a shared model reference.  Equivalent storage for the two-swatch concept. |
| `text` (field, `const char *`) | 409 | NOT-PORTED | — | — | — | C++ displayed a fixed text string in the selected colors; Rust replaced this with a structured variant readout.  Capability superseded by the rebuilt display. |

## Summary

- PORTED: 0   EQUIVALENT: 4   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 3   |   → concept: 0
- Notable: The `text` field (static text string drawn in palette colors) is NOT-PORTED because the rebuilt `InfoColumn` shows structured old/new swatches and a variant readout instead — a richer, equivalent capability.
