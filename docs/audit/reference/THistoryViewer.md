# THistoryViewer  (guide p. 457)

Rust module(s): `src/widgets/history.rs` (`struct HistoryViewer`)   |   magiblot: `include/tvision/dialogs.h` / `source/tvision/thstview.cpp`

> THistoryViewer is a single-column list viewer over the global history store,
> used inside THistoryWindow. Its own documented field is `historyId`; its
> documented methods are `getText`, `getPalette`, `handleEvent`, `historyWidth`.
> Its constructor also calls `setRange`, `focusItem(1)`, and `hScrollBar->setRange`.
> Palette `CHistoryViewer` has 5 entries (active, inactive, focused, selected, divider).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `historyId` (field) | 457 | PORTED | OK | `HistoryViewer.history_id: u8` | 2 | C++: `ushort historyId`. Rust: `u8` (store key width). Field is private. Doc: why `u8` is explained in struct doc. |
| `Init` (constructor) | 457 | PORTED | OK | `HistoryViewer::new` + `HistoryViewer::setup` | 2 | C++ constructor calls `setRange`, `focusItem(1)`, `hScrollBar->setRange(0, historyWidth()-size.x+3)` inline. Rust splits into `new` (data init, no `Context`) + `setup` (context-needing tail, called post-insert). Equivalent effect; the split is documented in the struct doc ("Call `setup` after inserting"). The two-stage split is deliberate (post-insert constraint same as `ListBox`). |
| `GetText` (method) | 457 | PORTED | OK | `HistoryViewer::get_text` (impl `ListViewer::get_text`) | 2 | C++: `getText(char *dest, short item, short maxChars)` → `historyStr(historyId, item)` → `strncpy`. Rust: returns `String`; negative or out-of-range → `String::new()`. Same contract. Doc explains what it does; "how out-of-range is handled" is in code comment, not rustdoc. |
| `GetPalette` (method) | 457 | EQUIVALENT | OK | `HistoryViewer::LIST_ROLES` + `HistoryViewer::list_roles()` | 2 | C++: `cpHistoryViewer "\x06\x06\x07\x06\x06"` — 5-entry palette (active=inactive=selected=divider=0x06 → white-on-blue; focused=0x07 → white-on-green). Rust: `ListRoles` quintet with `Role::HistoryViewerNormal` (white on blue) and `Role::HistoryViewerFocused` (white on green). Exact same two-color split; known idiomatic mapping: class Palette → `tv::Theme`. `LIST_ROLES` is pub with doc score 2 (what, not the full chain). |
| `HandleEvent` (method) | 457 | PORTED | OK | `HistoryViewer::handle_event` (impl `View::handle_event`) | 3 | C++: double-click or Enter → `endModal(cmOK)`; Esc or `cmCancel` → `endModal(cmCancel)`; else `TListViewer::handleEvent`. Rust: same four arms; no-modal-state gate (same as C++, always inside a modal `HistoryWindow`). Fully documented in rustdoc with rationale for the no-gate decision. |
| `HistoryWidth` (method) | 457 | PORTED | OK | `HistoryViewer::history_width` (private) | 2 | C++: iterates channel entries with `strwidth`, returns max. Rust: iterates with `crate::text::width`, returns max. Same algorithm. Method is private (consistent with C++ — no `virtual`). O(n²) note is in method comment. |
| `CHistoryViewer` palette (5 entries) | 457 | EQUIVALENT | OK | `HistoryViewer::LIST_ROLES` (5-slot `ListRoles` struct) | 2 | C++: 5 entries (active, inactive, focused, selected, divider). Rust: 5-field `ListRoles`; active/inactive/selected/divider all map to `HistoryViewerNormal`; focused maps to `HistoryViewerFocused`. Exact match of the C++ mapping. Known idiomatic mapping: class Palette → `tv::Theme`. |

## Summary

- PORTED: 5   EQUIVALENT: 2   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 5   |   → concept: 0
- Notable finding: The C++ constructor calls `hScrollBar->setRange(...)` unconditionally (always has a horizontal bar). Rust `setup` guards the h-bar block with `if let Some(hbar)` — this is correct and necessary (the Rust constructor accepts `Option<ViewId>` for the bars), but the deviation from the C++ unconditional assumption is undocumented. For a narrow negative max (e.g. `historyWidth()-size.x+3 < 0`) the C++ and Rust both pass a negative value to `setRange`/`set_params`; the Rust `ScrollBar::set_params` floors it to min, which is safe. This is tested (test `negative_hbar_max_live_pump_no_panic`) but not mentioned in the `setup` rustdoc.
