# TColorGroupList  (guide p. 410)

Rust module(s): none (class superseded)   |   magiblot: `include/tvision/colorsel.h`

> **Note:** The guide (p. 410) says "Details of `TColorGroupList`'s fields and
> methods are in the online Help."  The magiblot header shows it derives from
> `TListViewer` with a `groups` field and four override methods.  The whole
> group-list concept is superseded by `ColorPicker`'s `TabBar + PageStack`
> architecture.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `focusItem` (method) | 410 | NOT-PORTED | — | — | — | Overrides `TListViewer::focusItem` to broadcast `cmNewColorItem`; superseded by the tab/page model |
| `getGroup` (method) | 410 | NOT-PORTED | — | — | — | Returns the `TColorGroup` at a given index; group concept absent |
| `getText` (method) | 410 | NOT-PORTED | — | — | — | Returns group name for list display; superseded by `TabBar` labels |
| `handleEvent` (method) | 410 | NOT-PORTED | — | — | — | Handled scroll/selection events for the group list; superseded by `ColorPicker::handle_event` + `TabBar` |
| `groups` (field, `TColorGroup *`) | — | NOT-PORTED | — | — | — | Linked-list head of `TColorGroup`; entire linked-list architecture superseded |
| `setGroupIndex` (method, magiblot) | — | NOT-PORTED | — | — | — | Sets the focused item index in a group; concept absent |
| `getGroupIndex` (method, magiblot) | — | NOT-PORTED | — | — | — | Returns focused item index; concept absent |
| `getNumGroups` (method, magiblot) | — | NOT-PORTED | — | — | — | Returns number of groups; concept absent |

## Summary

- PORTED: 0   EQUIVALENT: 0   NOT-PORTED: 8   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable: All methods NOT-PORTED; the scrollable group-list view is superseded by the `TabBar` in `ColorPicker`. The `focusItem` → broadcast-`cmNewColorItem` → item list update chain is replaced by the `SharedModel` Rc propagation pattern.
