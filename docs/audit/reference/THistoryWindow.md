# THistoryWindow  (guide pp. 457–458)

Rust module(s): `src/widgets/history.rs` (`struct HistoryWindow`)   |   magiblot: `include/tvision/dialogs.h` / `source/tvision/thistwin.cpp`

> THistoryWindow is the modal window that hosts a THistoryViewer recall list.
> Its documented fields are `viewer` and `historyId`; its documented methods are
> `getPalette`, `getSelection`, and `initViewer`. The constructor and
> `handleEvent` are also present in the C++ source (outside-click cancel).
> Palette `CHistoryWindow` has 7 entries (3 frame, 2 scrollbar, 2 viewer text).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `viewer` (field) | 457 | EQUIVALENT | OK | `HistoryWindow.viewer_id: ViewId` | N/A | Private field (`viewer_id: ViewId`); has an inline comment. Not public API — does not count toward public bar closure. |
| `historyId` (field) | 457 | NOT-PORTED | — | — | — | Not a C++ field (the id lives only in `THistoryViewer`); the guide description is imprecise. Rust likewise stores it only in `HistoryViewer`. No gap. |
| `Init` (constructor) | 457 | PORTED | OK | `HistoryWindow::new(bounds, history_id)` | 3 | Raised: rustdoc now explains when to use it (typically the event loop drains `Deferred::OpenHistory`; pass to `Program::exec_view`, then call `get_selection` after `Command::OK`), plus the three construction steps (numberless close-only window, two scroll bars, deferred viewer setup). |
| `GetPalette` (method) | 458 | EQUIVALENT | OK | delegated to `Window` → `tv::theme::Role` family | 3 | C++ `CHistoryWindow` has 7 slots: 3 frame (Window blue family = `Role::Frame*`), 2 scrollbar (`Role::ScrollBarPage/Controls`), 2 viewer text (`HistoryViewerNormal/Focused`). All relevant `Role` variants documented at score 3 in `src/theme.rs` (theme pass). |
| `GetSelection` (method) | 458 | PORTED | OK | `HistoryWindow::get_selection` | N/A | `pub(crate)` — not public API. Has a doc comment explaining the `&mut self` constraint and the downcast path. Does not count toward public bar closure. |
| `HandleEvent` (method) | 458 | PORTED | OK | `HistoryWindow::handle_event` (impl `View::handle_event`) | 3 | C++: calls `TWindow::handleEvent`, then outside-click → `endModal(cmCancel)`. Rust: setup guard (A) → `window.handle_event` (B) → outside-click cancel (C). Same outside-click cancel. Setup guard is a Rust addition; documented in rustdoc with the three-step ordering. |
| `InitViewer` (method) | 458 | EQUIVALENT | OK | inlined in `HistoryWindow::new` | N/A | No separate public Rust method — the `THistInit` virtual-factory pattern is not needed in Rust; the logic is inlined in `new`. `new` rustdoc describes the construction steps. |
| `CHistoryWindow` palette (7 entries) | 458 | EQUIVALENT | OK | `Window` blue family + `HistoryViewer::LIST_ROLES` | 3 | See `GetPalette` row above — all 7 slots map to score-3 `Role` variants in `src/theme.rs`. |

## Summary

- PORTED: 3   EQUIVALENT: 4   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Raised: `Init/new` → 3. `GetPalette` and `CHistoryWindow` palette raised to 3 in the theme.rs Role pass (all 7 slots map to score-3 `Role` variants in `src/theme.rs`). Private/crate fields (`viewer_id`) and `pub(crate)` methods (`get_selection`, `InitViewer` inlined in `new`) are N/A for the public bar.
- Notable finding: `historyId` is listed as a THistoryWindow field in the guide but is not a field of the C++ class (the id lives only in the THistoryViewer); both C++ and Rust are consistent — it is not a gap.
