# TListViewer  (guide pp. 470–474)

Rust module(s): src/widgets/list_viewer.rs   |   magiblot: include/tvision/views.h / source/tvision/tlstview.cpp

> TListViewer is the abstract base for every list widget. It provides fields,
> a draw loop, keyboard/mouse/scrollbar event handling, and a 5-entry palette.
> In the port it is split into the `ListViewer` trait (overridable hooks) and
> `ListViewerState` (data fields), with shared logic as free functions generic
> over `L: ListViewer`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `hScrollBar` (field) | 471 | EQUIVALENT | OK | `tv::list_viewer::ListViewerState::h_scroll_bar: Option<ViewId>` | 2 | Raw pointer → ViewId handle (D3, documented in module doc). Public field. Doc explains what it is but not how to wire it. |
| `vScrollBar` (field) | 471 | EQUIVALENT | OK | `tv::list_viewer::ListViewerState::v_scroll_bar: Option<ViewId>` | 2 | Same as h_scroll_bar. Public field. |
| `numCols` (field) | 471 | PORTED | OK | `tv::list_viewer::ListViewerState::num_cols: i32` | 2 | Type widened to `i32` (faithful: C++ is `short`); `>= 1` enforced by debug_assert + clamp in constructor. |
| `topItem` (field) | 471 | PORTED | OK | `tv::list_viewer::ListViewerState::top_item: i32` | 2 | Direct counterpart. |
| `focused` (field) | 471 | PORTED | OK | `tv::list_viewer::ListViewerState::focused: i32` | 2 | Direct counterpart. |
| `range` (field) | 471 | PORTED | OK | `tv::list_viewer::ListViewerState::range: i32` | 2 | Direct counterpart. |
| `Init` (constructor) | 471 | EQUIVALENT | OK | `tv::list_viewer::ListViewerState::new(bounds, num_cols, h, v)` + `tv::list_viewer::update_steps(this, ctx)` split | 2 | C++ constructor both creates state AND calls `setStep` on bars in one shot. Rust splits construction (no `Context`) from step publication (`update_steps` called post-insert). Deliberate: D3 broker constraint. Module doc explains. |
| `changeBounds` (method) | 471 | EQUIVALENT | OK | `tv::list_viewer::on_bounds_changed(this, ctx)` | 3 | C++ `changeBounds` calls `TView::changeBounds` then re-publishes `hScrollBar->setStep(size.x/numCols, arStep)` and `vScrollBar->setStep(size.y, arStep)` preserving arStep. Rust `on_bounds_changed` does the same (preserves arrow step, uses plain `size.y` for v-bar — the resize formula, NOT the construction formula). Documented distinction in module doc. |
| `draw` (method) | 471 | PORTED | OK | `tv::list_viewer::draw(this, ctx)` free function | 3 | C++ `draw` reads `hScrollBar->value` live for indent; Rust caches it in `ListViewerState::indent` (broker constraint, D3). Color matrix matches (active=sfSelected+sfActive; inactive=sfSelected only). Column-major layout, divider, empty placeholder all present. `showMarkers` / `specialChars` (the C++ pair of bracket glyphs at cell edges) are NOT implemented — undocumented omission; the Rust draw does NOT draw bracket markers. Flag: SUSPECT on this omission — see Notes column. |
| `focusItem` (method) | 472 | PORTED | OK | `tv::list_viewer::focus_item(this, item, ctx)` free function | 3 | C++: sets `focused`, calls `vScrollBar->setValue(item)` or `drawView()` if no bar, then adjusts `topItem`. Rust: sets `focused`, requests deferred `set_value` on v-bar (no `drawView` — whole-tree redraw per D9). `topItem` adjust logic identical for both single-col and multi-col. `on_focus_changed` hook is a Rust extension not in C++ but deliberate. |
| `getText` (method) | 472 | PORTED | OK | `tv::list_viewer::ListViewer::get_text(item) -> String` | 3 | C++ virtual `char*` out-param base returns EOS. Rust returns `String::new()`. Idiomatic. Documented. |
| `getPalette` (method) | 472 | EQUIVALENT | OK | `tv::list_viewer::ListViewer::list_roles() -> ListRoles` + `tv::theme::Role::ListNormal*` etc. | 2 | C++ returns `CListViewer` (5-entry palette). Rust returns a `ListRoles` quintet of `Role` values: `ListNormalActive`, `ListNormalInactive`, `ListFocused`, `ListSelected`, `ListDivider`. Idiomatic mapping: class Palette → Theme (D7). Doc on `list_roles` explains what it is but not how to use it for subclass recoloring. |
| `handleEvent` (method) | 472 | PORTED | OK | `tv::list_viewer::handle_event(this, ev, ctx)` free function | 3 | C++ event loop: mouse hold runs as a do-while polling `mouseEvent` inside `evMouseDown`. Rust replaces the loop with a capture-based state machine (D3 broker): `MouseDown` arms a `MouseTrackCapture`; pump delivers `MouseMove`/`MouseAuto`/`MouseUp` events. Behavior matches. C++ scrollbar-changed broadcast reads `hScrollBar->value` directly inline (line 347: `focusItemNum(vScrollBar->value)`); Rust defers to a `SyncListViewer` op (pump broker). This is documented (D3). The C++ base call `TView::handleEvent(event)` is intentionally omitted — it only performs mouse-down auto-select (focus the view on click), which `Group::route_event` now owns; `TView::handleEvent` is a no-op for every other event class, so there is no base behavior to inherit. Documented in source. |
| `isSelected` (method) | 473 | PORTED | OK | `tv::list_viewer::ListViewer::is_selected(item) -> bool` | 2 | Default: `item == focused`. Direct counterpart. |
| `selectItem` (method) | 473 | PORTED | OK | `tv::list_viewer::ListViewer::select_item(item, ctx)` | 2 | C++ broadcasts `cmListItemSelected` with `this`. Rust broadcasts `Command::LIST_ITEM_SELECTED` with the view's `ViewId` as source. Idiomatic (D4 source). |
| `setRange` (method) | 473 | PORTED | OK | `tv::list_viewer::set_range(this, a_range, ctx)` free function | 3 | C++: sets `range`, resets `focused` to 0 if `>= aRange`, calls `vScrollBar->setParams` or `drawView`. Rust: same, uses deferred `request_scroll_bar_params` (no `drawView` — D9). |
| `setState` (method) | 473 | PORTED | OK | `tv::list_viewer::set_state(this, flag, enable, ctx)` free function | 2 | C++ checks `(aState & (sfSelected | sfActive | sfVisible)) != 0` to decide whether to show/hide scroll bars. Rust checks `flag == Active || flag == Selected || flag == Visible`, matching C++. The arm shows both bars iff `active && visible` via `ctx.request_set_visible` (the deferred seam). `StateFlag::Visible` is delivered by `Group::set_visible_descendant` when the pump applies a `Deferred::SetVisible` op, routing through `child.set_state` so this arm fires. |
| `focusItemNum` (method) | 473 | PORTED | OK | `tv::list_viewer::focus_item_num(this, item, ctx)` free function | 3 | Direct counterpart. Clamping logic matches C++ exactly: `< 0 → 0`; `>= range && range > 0 → range - 1`; skip if `range == 0`. |
| `CListViewer` palette (5 entries) | 474 | EQUIVALENT | OK | `tv::list_viewer::ListRoles` + `tv::theme::Role::ListNormal*` quintet | 2 | C++: `\x1A\x1A\x1B\x1C\x1D` (entries 1-5: Active, Inactive, Focused, Selected, Divider). Rust: `ListRoles` struct with 5 `Role` fields mapping to the same 5 semantic slots. Known idiomatic mapping: class Palette → Theme. The `ListRoles` struct docs explain the slots but "how to subclass recolor" could be stronger. |
| `shutDown` (method) | — | NOT-PORTED | — | — | N/A | C++ `shutDown` nulls out `hScrollBar`/`vScrollBar` pointers. ViewId handles need no nulling (Rust ownership model). Intentional; no comment in code but D3 makes it self-evident. |
| `TStreamable` / `write` / `read` / `build` (stream methods) | — | NOT-PORTED | — | — | N/A | C++ TStreamable serialization infrastructure dropped; serde if revived (known idiomatic mapping). |

## Summary

- PORTED: 13   EQUIVALENT: 6   NOT-PORTED: 2   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 12   |   → concept: 0
- Remaining notable omission: `draw` omits the C++ `showMarkers` bracket glyphs at cell edges with no comment (flagged in the `draw` Notes above).
