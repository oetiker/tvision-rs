# THistoryWindow  (guide pp. 457–458)

Rust module(s): `src/widgets/history.rs` (`struct HistoryWindow`)   |   magiblot: `include/tvision/dialogs.h` / `source/tvision/thistwin.cpp`

> THistoryWindow is the modal window that hosts a THistoryViewer recall list.
> Its documented fields are `viewer` and `historyId`; its documented methods are
> `getPalette`, `getSelection`, and `initViewer`. The constructor and
> `handleEvent` are also present in the C++ source (outside-click cancel).
> Palette `CHistoryWindow` has 7 entries (3 frame, 2 scrollbar, 2 viewer text).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `viewer` (field) | 457 | EQUIVALENT | OK | `HistoryWindow.viewer_id: ViewId` | 2 | C++: `TListViewer *viewer` raw pointer. Rust: `ViewId` (D3 known idiomatic mapping: pointer → ViewId). Field is private. Doc: what `viewer_id` is; resolution via `child_mut` is documented in `get_selection` and `handle_event`. |
| `historyId` (field) | 457 | NOT-PORTED | — | — | — | C++: `THistoryWindow` has no explicit `historyId` field; the id is passed to `initViewer` and stored only in the `THistoryViewer`. The guide lists it under THistoryWindow but it is not a field of the C++ class. Rust likewise stores it only in `HistoryViewer`, not in `HistoryWindow`. No gap — the guide description is imprecise. |
| `Init` (constructor) | 457 | PORTED | OK | `HistoryWindow::new(bounds, history_id)` | 2 | C++: `wfClose`, numberless, calls `initViewer` via `createListViewer`, inserts. Rust: `WindowFlags { close: true }`, numberless (`0`), builds two scroll bars then the viewer, inserts. Close-only matches. Viewer-construction indirection (via `THistInit::createListViewer` virtual chain) is inlined as noted in the struct doc. Setup-guard field (`setup_done`) is a Rust addition for the post-insert context constraint. |
| `GetPalette` (method) | 458 | EQUIVALENT | OK | delegated to `Window` → `tv::theme::Role` family | 2 | C++: `cpHistoryWindow "\x13\x13\x15\x18\x17\x13\x14"` — 7 entries (frame passive/active/icon, scrollbar page/controls, viewer normal/selected). Rust: palette chain is satisfied by the default `Window` blue palette family; viewer colors remap via `HistoryViewer::LIST_ROLES`. Known idiomatic mapping: class Palette → `tv::Theme`. No explicit override in `HistoryWindow`; the delegate macro forwards to `Window`. The struct doc notes "The window keeps the default blue Window/Frame role family." Doc score 2 — the full 7-slot chain mapping to Rust roles is not spelled out. |
| `GetSelection` (method) | 458 | PORTED | OK | `HistoryWindow::get_selection` | 2 | C++: `viewer->getText(dest, viewer->focused, 255)`. Rust: resolves `viewer_id` via `child_mut` + `downcast_mut::<HistoryViewer>()`, calls `hv.selection()`. Same semantics; downcast returns empty string on unreachable failure (documented). `pub(crate)` — not public API. Doc: what it does and the `&mut self` constraint; "why the downcast path is needed" is in code comments. |
| `HandleEvent` (method) | 458 | PORTED | OK | `HistoryWindow::handle_event` (impl `View::handle_event`) | 3 | C++: calls `TWindow::handleEvent`, then outside-click → `endModal(cmCancel)`. Rust: setup guard (A) → `window.handle_event` (B) → outside-click cancel (C). Same outside-click cancel. Setup guard is a Rust addition for the post-insert context constraint; documented in rustdoc with the three-step ordering. |
| `InitViewer` (method) | 458 | EQUIVALENT | OK | inlined in `HistoryWindow::new` | 2 | C++: `static TListViewer *initViewer(TRect, TWindow*, ushort)` — grows rect by (-1,-1), builds two scroll bars (`sbHorizontal|sbHandleKeyboard`, `sbVertical|sbHandleKeyboard`), builds `THistoryViewer`. Rust: same logic inlined in `new` (noted in struct doc: "viewer-construction indirection is inlined into the constructor"). The `THistInit` virtual-factory pattern is not needed in Rust. |
| `CHistoryWindow` palette (7 entries) | 458 | EQUIVALENT | OK | `Window` blue family + `HistoryViewer::LIST_ROLES` | 2 | C++: 7 entries — 3 frame (passive, active, icon), 2 scrollbar (page, controls), 2 viewer (normal, selected). Rust: frame/scrollbar palette slots come from the `Window` blue role family (same colors); viewer slots come from `HistoryViewer::LIST_ROLES`. Known idiomatic mapping: class Palette → `tv::Theme`. |

## Summary

- PORTED: 4   EQUIVALENT: 3   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 5   |   → concept: 0
- Notable finding: `historyId` is listed as a THistoryWindow field in the guide but is not a field of the C++ class (the id lives only in the THistoryViewer); both C++ and Rust are consistent — it is not a gap. The most actionable documentation gap is `GetPalette`: the 7-slot `cpHistoryWindow` mapping to the Rust `Window` blue family + `HistoryViewer::LIST_ROLES` is implicit and a reader tracing the C++ palette chain cannot easily verify the equivalence from the current rustdoc.
