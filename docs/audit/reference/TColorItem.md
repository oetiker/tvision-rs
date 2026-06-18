# TColorItem type  (guide pp. 410–411)

Rust module(s): none (type superseded)   |   magiblot: `include/tvision/colorsel.h`

> **Note:** `TColorItem` is a Pascal record / C++ class for a linked-list node
> naming one palette entry (by display name and palette index).  The entire
> group/item palette architecture is superseded by the rebuilt `ColorPicker` and
> `Theme` system.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Name` (field, `PString`) | 410–411 | NOT-PORTED | — | — | — | Display name of the color item.  Replaced by named presets in `PresetsSurface::PRESETS` (`src/dialog/colorpick/presets.rs:13`) — but those are truecolor presets, not palette-index names. |
| `Index` (field, `Byte`) | 410–411 | NOT-PORTED | — | — | — | Palette entry index; palette-index concept dropped (D7) |
| `Next` (field, `PColorItem`) | 410–411 | NOT-PORTED | — | — | — | Next item in the linked list; superseded |
| `ColorItem` builder function (see also) | 411 | NOT-PORTED | — | — | — | Pascal helper to create/chain `TColorItem` records; superseded |
| `operator+` (item+item, group+item) | — | NOT-PORTED | — | — | — | C++ chaining operators; superseded |

## Summary

- PORTED: 0   EQUIVALENT: 0   NOT-PORTED: 5   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable: All NOT-PORTED. The named-preset concept survives in `PresetsSurface::PRESETS` (29 named entries including Default, 16 BIOS, 12 RGB), but as truecolor values rather than palette-index labels, which is strictly a superset of the Borland capability.
