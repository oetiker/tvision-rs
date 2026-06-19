# TColorItemList  (guide p. 411)

Rust module(s): none (class superseded)   |   magiblot: `include/tvision/colorsel.h`

> **Note:** The guide (p. 411) says "Details of `TColorItemList`'s fields and
> methods are in the online Help."  The magiblot header shows it derives from
> `TListViewer` with an `items` field and three override methods.  The whole
> item-list view is superseded by `PresetsSurface` in the rebuilt picker.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `focusItem` (method) | 411 | EQUIVALENT | OK | `PresetsSurface::handle_event` arrow keys / mouse click (`src/dialog/colorpick/presets.rs`) | N/A | `PresetsSurface` is `pub(crate)` — an internal implementation detail of `ColorPicker`; not a public API target. C++ `focusItem` broadcast `cmNewColorIndex`; Rust `PresetsSurface::handle_event` updates the `ColorModel` directly (shared model propagates automatically). Equivalent behavior; N/A for public doc scoring. |
| `getText` (method) | 411 | EQUIVALENT | OK | `PresetsSurface::draw` renders each row's name from `PRESETS[i].0` (`src/dialog/colorpick/presets.rs`) | N/A | `PresetsSurface` is `pub(crate)`. C++ fetched item name from linked list; Rust reads the `pub(crate) PRESETS` static slice. Equivalent display; N/A for public doc scoring. |
| `handleEvent` (method) | 411 | EQUIVALENT | OK | `PresetsSurface::handle_event` (`src/dialog/colorpick/presets.rs`) | N/A | `PresetsSurface` is `pub(crate)`. Handles Up/Down keys and mouse clicks equivalently to C++. N/A for public doc scoring. |
| `items` (field, `TColorItem *`) | — | NOT-PORTED | — | — | — | Linked-list head of `TColorItem`; replaced by the static `PRESETS` slice. |

## Summary

- PORTED: 0   EQUIVALENT: 3   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable: The three behavioral methods are EQUIVALENT via `PresetsSurface`. The `items` linked-list field is replaced by the static `PRESETS` slice. All three EQUIVALENT rows reconciled to N/A: `PresetsSurface` and `PRESETS` are `pub(crate)` — internal implementation details of the rebuilt `ColorPicker`; they are not public API targets.
