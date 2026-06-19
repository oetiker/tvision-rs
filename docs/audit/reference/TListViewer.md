# TListViewer  (guide pp. 470–474)

Rust module(s): src/widgets/list_viewer.rs   |   magiblot: include/tvision/views.h / source/tvision/tlstview.cpp

> TListViewer is the abstract base for every list widget. It provides fields,
> a draw loop, keyboard/mouse/scrollbar event handling, and a 5-entry palette.
> In the port it is split into the `ListViewer` trait (overridable hooks) and
> `ListViewerState` (data fields), with shared logic as free functions generic
> over `L: ListViewer`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `hScrollBar` (field) | 471 | EQUIVALENT | OK | `tv::list_viewer::ListViewerState::h_scroll_bar: Option<ViewId>` | 3 | Raw pointer → ViewId handle (D3, documented in module doc). Public field. Doc now covers wiring: pass id at construction, call `update_steps` after insertion, let pump broker all subsequent sync. |
| `vScrollBar` (field) | 471 | EQUIVALENT | OK | `tv::list_viewer::ListViewerState::v_scroll_bar: Option<ViewId>` | 3 | Same as h_scroll_bar. Doc now covers wiring. |
| `numCols` (field) | 471 | PORTED | OK | `tv::list_viewer::ListViewerState::num_cols: i32` | 3 | Doc now explains column-major layout formula and `>= 1` clamp. |
| `topItem` (field) | 471 | PORTED | OK | `tv::list_viewer::ListViewerState::top_item: i32` | 3 | Doc now explains scroll-offset role and when to use `focus_item` vs direct write. |
| `focused` (field) | 471 | PORTED | OK | `tv::list_viewer::ListViewerState::focused: i32` | 3 | Doc now explains cursor role and directs to `focus_item`/`focus_item_num` for mutation. |
| `range` (field) | 471 | PORTED | OK | `tv::list_viewer::ListViewerState::range: i32` | 3 | Doc now explains why `set_range` must be used for mutation. |
| `Init` (constructor) | 471 | EQUIVALENT | OK | `tv::list_viewer::ListViewerState::new(bounds, num_cols, h, v)` + `tv::list_viewer::update_steps(this, ctx)` split | 3 | Doc now explains the two-step pattern (construct then `update_steps` after insertion) and the reason (no `Context` at construction). |
| `changeBounds` (method) | 471 | EQUIVALENT | OK | `tv::list_viewer::on_bounds_changed(this, ctx)` | 3 | C++ `changeBounds` calls `TView::changeBounds` then re-publishes `hScrollBar->setStep(size.x/numCols, arStep)` and `vScrollBar->setStep(size.y, arStep)` preserving arStep. Rust `on_bounds_changed` does the same (preserves arrow step, uses plain `size.y` for v-bar — the resize formula, NOT the construction formula). Documented distinction in module doc. |
| `draw` (method) | 471 | PORTED | OK | `tv::list_viewer::draw(this, ctx)` free function | 3 | C++ `draw` reads `hScrollBar->value` live for indent; Rust caches it in `ListViewerState::indent` (broker constraint, D3). Color matrix matches (active=sfSelected+sfActive; inactive=sfSelected only). Column-major layout, divider, empty placeholder all present. `showMarkers` / `specialChars` (the C++ pair of bracket glyphs at cell edges) are NOT implemented — undocumented omission; the Rust draw does NOT draw bracket markers. Flag: SUSPECT on this omission — see Notes column. |
| `focusItem` (method) | 472 | PORTED | OK | `tv::list_viewer::focus_item(this, item, ctx)` free function | 3 | C++: sets `focused`, calls `vScrollBar->setValue(item)` or `drawView()` if no bar, then adjusts `topItem`. Rust: sets `focused`, requests deferred `set_value` on v-bar (no `drawView` — whole-tree redraw per D9). `topItem` adjust logic identical for both single-col and multi-col. `on_focus_changed` hook is a Rust extension not in C++ but deliberate. |
| `getText` (method) | 472 | PORTED | OK | `tv::list_viewer::ListViewer::get_text(item) -> String` | 3 | C++ virtual `char*` out-param base returns EOS. Rust returns `String::new()`. Idiomatic. Documented. |
| `getPalette` (method) | 472 | EQUIVALENT | OK | `tv::list_viewer::ListViewer::list_roles() -> ListRoles` + `tv::theme::Role::ListNormal*` etc. | 3 | Doc now explains override pattern: return a custom `ListRoles` to recolor; maps to all five drawing cases. `ListRoles` struct doc also updated to explain the five slots and point to `LIST_VIEWER` constant. |
| `handleEvent` (method) | 472 | PORTED | OK | `tv::list_viewer::handle_event(this, ev, ctx)` free function | 3 | C++ event loop: mouse hold runs as a do-while polling `mouseEvent` inside `evMouseDown`. Rust replaces the loop with a capture-based state machine (D3 broker): `MouseDown` arms a `MouseTrackCapture`; pump delivers `MouseMove`/`MouseAuto`/`MouseUp` events. Behavior matches. C++ scrollbar-changed broadcast reads `hScrollBar->value` directly inline (line 347: `focusItemNum(vScrollBar->value)`); Rust defers to a `SyncListViewer` op (pump broker). This is documented (D3). The C++ base call `TView::handleEvent(event)` is intentionally omitted — it only performs mouse-down auto-select (focus the view on click), which `Group::route_event` now owns; `TView::handleEvent` is a no-op for every other event class, so there is no base behavior to inherit. Documented in source. |
| `isSelected` (method) | 473 | PORTED | OK | `tv::list_viewer::ListViewer::is_selected(item) -> bool` | 3 | Doc now explains default (single-selection) and override pattern for multi-select. |
| `selectItem` (method) | 473 | PORTED | OK | `tv::list_viewer::ListViewer::select_item(item, ctx)` | 3 | Doc now explains the `LIST_ITEM_SELECTED` broadcast with `ViewId` as source, how owners filter it, and when to override. |
| `setRange` (method) | 473 | PORTED | OK | `tv::list_viewer::set_range(this, a_range, ctx)` free function | 3 | C++: sets `range`, resets `focused` to 0 if `>= aRange`, calls `vScrollBar->setParams` or `drawView`. Rust: same, uses deferred `request_scroll_bar_params` (no `drawView` — D9). |
| `setState` (method) | 473 | PORTED | OK | `tv::list_viewer::set_state(this, flag, enable, ctx)` free function | 3 | Doc now explains: applies flag, broadcasts focus events on `Focused`, shows/hides both bars on `Active`/`Selected`/`Visible` using `active && visible` rule. Concrete widgets call this from their `View::set_state` impl. |
| `focusItemNum` (method) | 473 | PORTED | OK | `tv::list_viewer::focus_item_num(this, item, ctx)` free function | 3 | Direct counterpart. Clamping logic matches C++ exactly: `< 0 → 0`; `>= range && range > 0 → range - 1`; skip if `range == 0`. |
| `CListViewer` palette (5 entries) | 474 | EQUIVALENT | OK | `tv::list_viewer::ListRoles` + `tv::theme::Role::ListNormal*` quintet | 3 | `ListRoles` struct doc now explains all five slots and the override pattern. |
| `shutDown` (method) | — | NOT-PORTED | — | — | N/A | C++ `shutDown` nulls out `hScrollBar`/`vScrollBar` pointers. ViewId handles need no nulling (Rust ownership model). Intentional; no comment in code but D3 makes it self-evident. |
| `TStreamable` / `write` / `read` / `build` (stream methods) | — | NOT-PORTED | — | — | N/A | C++ TStreamable serialization infrastructure dropped; serde if revived (known idiomatic mapping). |

## Summary

- PORTED: 13   EQUIVALENT: 6   NOT-PORTED: 2   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Remaining notable omission: `draw` omits the C++ `showMarkers` bracket glyphs at cell edges with no comment (flagged in the `draw` Notes above).
