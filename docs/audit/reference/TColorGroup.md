# TColorGroup type  (guide pp. 409–410)

Rust module(s): none (type superseded)   |   magiblot: `include/tvision/colorsel.h`

> **Note:** `TColorGroup` is a Pascal record / C++ class that formed a linked-list
> node in Borland's palette-group architecture.  tvision-rs replaced the entire
> palette-group/item/index architecture with the `Theme` system (D7) and the
> rebuilt `ColorPicker` (`src/dialog/colorpick/`).  There are no groups, items, or
> palette indexes in the rebuilt picker; instead, colors are picked as `Color`
> values (truecolor) via surface tabs.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Name` (field, `PString`) | 409–410 | NOT-PORTED | — | — | — | Group name string; no group concept in rebuilt picker |
| `Index` (field, `Byte`) | 409–410 | NOT-PORTED | — | — | — | Ordinal position in the color list; palette index concept dropped (D7) |
| `Items` (field, `PColorItem`) | 409–410 | NOT-PORTED | — | — | — | Pointer to first `TColorItem` in the group's linked list; superseded |
| `Next` (field, `PColorGroup`) | 409–410 | NOT-PORTED | — | — | — | Next group in the linked list; superseded |
| `ColorGroup` builder function (see also) | 410 | NOT-PORTED | — | — | — | The Pascal helper function to create/chain `TColorGroup` records; superseded |
| `operator+` (group+item, group+group) | — | NOT-PORTED | — | — | — | C++ convenience operators for chaining groups and items; superseded |

## Summary

- PORTED: 0   EQUIVALENT: 0   NOT-PORTED: 6   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable: The entire `TColorGroup` record is NOT-PORTED because the palette-group architecture was replaced by the rebuilt truecolor picker and `Theme` (D7). No capability gap: users pick any color via the picker; app palette grouping is replaced by semantic `Role` assignment in `Theme`.
