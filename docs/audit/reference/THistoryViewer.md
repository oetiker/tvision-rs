# THistoryViewer  (guide p. 457)

Rust module(s): `src/widgets/history.rs` (`struct HistoryViewer`)   |   magiblot: `include/tvision/dialogs.h` / `source/tvision/thstview.cpp`

> THistoryViewer is a single-column list viewer over the global history store,
> used inside THistoryWindow. Its own documented field is `historyId`; its
> documented methods are `getText`, `getPalette`, `handleEvent`, `historyWidth`.
> Its constructor also calls `setRange`, `focusItem(1)`, and `hScrollBar->setRange`.
> Palette `CHistoryViewer` has 5 entries (active, inactive, focused, selected, divider).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `historyId` (field) | 457 | PORTED | OK | `HistoryViewer.history_id: u8` | N/A | Private field (`history_id: u8`); has an inline comment. Not public API — does not count toward public bar closure. |
| `Init` (constructor) | 457 | PORTED | OK | `HistoryViewer::new` + `HistoryViewer::setup` | 3 | Raised: `new` rustdoc now explains the two-stage pattern (new = data init, setup = context-needing tail), when to call setup, and that `HistoryWindow::new` builds this for you in practice. `setup` rustdoc explains when it runs automatically vs. when to call it directly, and documents each step including the negative-max floor for the h-bar. |
| `GetText` (method) | 457 | PORTED | OK | `HistoryViewer::get_text` (impl `ListViewer::get_text`) | 3 | Raised: rustdoc now explains what the caller sees (display string for list row `item`), that it fetches from the history channel via `history_str`, and that negative/out-of-range indices return empty string rather than panicking so the base painter can call it unconditionally. |
| `GetPalette` (method) | 457 | EQUIVALENT | OK | `HistoryViewer::LIST_ROLES` + `HistoryViewer::list_roles()` | 3 | Raised: `LIST_ROLES` rustdoc now explains the five-slot mapping (normal/inactive/selected/divider → HistoryViewerNormal; focused → HistoryViewerFocused), the visual effect (blue-on-green highlight), and why it lives on `HistoryViewer` (so `list_viewer.rs` has no dependency on history roles). |
| `HandleEvent` (method) | 457 | PORTED | OK | `HistoryViewer::handle_event` (impl `View::handle_event`) | 3 | C++: double-click or Enter → `endModal(cmOK)`; Esc or `cmCancel` → `endModal(cmCancel)`; else `TListViewer::handleEvent`. Rust: same four arms; no-modal-state gate. Fully documented in rustdoc with rationale for the no-gate decision. |
| `HistoryWidth` (method) | 457 | PORTED | OK | `HistoryViewer::history_width` (private) | N/A | Private method; has a doc comment. Not public API — does not count toward public bar closure. |
| `CHistoryViewer` palette (5 entries) | 457 | EQUIVALENT | OK | `HistoryViewer::LIST_ROLES` (5-slot `ListRoles` struct) | 3 | See `LIST_ROLES` row above — raised together with the `GetPalette` row. |

## Summary

- PORTED: 5   EQUIVALENT: 2   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Raised: `Init/new+setup` → 3; `GetText/get_text` → 3; `GetPalette/LIST_ROLES` → 3. `CHistoryViewer` palette roles addressed via `LIST_ROLES` doc; `HistoryViewerNormal`/`HistoryViewerFocused` in `src/theme.rs` confirmed score-3 in the theme pass. Private field (`history_id`) and private method (`history_width`) are N/A for the public bar.
- Notable finding: The C++ constructor calls `hScrollBar->setRange(...)` unconditionally (always has a horizontal bar). Rust `setup` guards the h-bar block with `if let Some(hbar)` — this is correct and necessary (the Rust constructor accepts `Option<ViewId>` for the bars), but the deviation from the C++ unconditional assumption is now documented in the `setup` rustdoc (noting that `ScrollBar::set_params` floors a negative max to the minimum, which is safe and tested).
