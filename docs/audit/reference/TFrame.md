# TFrame  (guide pp. 443–445)

Rust module(s): src/frame.rs   |   magiblot: include/tvision/views.h / source/tvision/tframe.cpp

> TFrame has **no own fields** documented by the guide — it inherits all fields
> from TView. The guide documents only 5 methods and one palette entry.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Init` (constructor) | 444 | PORTED | OK | `tv::Frame::new(bounds: Rect) -> Frame` | 3 | Guide says sets `GrowMode` to `gfGrowHiX + gfGrowHiY` and `EventMask |= evBroadcast`. Rust `new` sets `grow_mode.hi_x / hi_y = true`. `evBroadcast` mask is implicit (frame receives broadcasts unconditionally per module doc). Matches. Rustdoc now explains when callers construct a Frame (Window::new only) and what the grow-mode / non-selectable setup means. |
| `Draw` (method) | 444 | PORTED | OK | `tv::Frame::draw` (impl `View::draw`) | 3 | Guide: draws border with state-dependent colours and icons (active/inactive/dragging), title from owner. Rust: full draw impl; state → role family; title/flags/number pushed down (deviation D3, documented). Double-line active / single-line passive/dragging. All icon cases (close, zoom, unzoom, resize, drag-left) handled. Palette deviation to `Role`-keyed theme is D7, documented. |
| `GetPalette` (method) | 444 | EQUIVALENT | OK | `tv::Frame::palette() -> WindowPalette` + `tv::Theme` role mapping | 3 | C++ returns `CFrame` palette (5 entries, indices 1–5 map to first 3 window palette slots). Rust uses `WindowPalette` enum pushed down by owner + `Role::Frame*` / `Role::FrameGray*` / `Role::FrameCyan*` selected at draw time — same three colour families, different shape. Rustdoc now names all three families and explains that reading the getter is rarely needed; the companion setter is `pub(crate)` called by the window. |
| `HandleEvent` (method) | 444 | PORTED | OK | `tv::Frame::handle_event` (impl `View::handle_event`) | 3 | Guide: mouse events — close icon → `cmClose`, zoom icon or double-click top row → `cmZoom`, drag top row → move window, drag lower-right → resize. Rust handles close (with release-confirm via `MouseTrackCapture`, deviation D3 push-down), zoom click and double-click, title-bar drag and bottom-corner grow left/right unconsumed for Window. Middle-button interior move also unconsumed for Window. All cases covered; all deliberate deviations commented. |
| `SetState` (method) | 444 | PORTED | OK | `tv::view::Group::set_state` propagation (no override in `Frame`) | N/A | C++ `TFrame::setState` calls `TView::setState` then redraws if `sfActive` or `sfDragging` changed. Rust Frame does NOT override `set_state`; instead, `Group::set_state` propagates state flags to children (incl. the frame), and the redraw is triggered by the whole-tree redraw on every pump tick. There is no `set_state` override in `src/frame.rs` to doc — the module doc (paragraph "How a frame gets its data", last two sentences) already covers this structural non-override. No symbol to raise; N/A. |
| `CFrame` palette (5 entries) | 445 | EQUIVALENT | OK | `tv::theme::Role::FramePassive`, `FrameActive`, `FrameDragging`, `FrameIcon` (+ Cyan/Gray families) | 3 | Guide: `CFrame` palette maps indices 1–5 to first three slots of window palette. Rust expands to three named families (`Frame*`, `FrameCyan*`, `FrameGray*`) keyed by `WindowPalette`. All 12 frame role variants already carried score-3 docs (each names the palette scheme, color, and chain). Cross-reference table added to the `Role` enum doc in the theme.rs Role pass. |

## Summary

- PORTED: 4   EQUIVALENT: 2   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: No gaps or suspect items. Raised `Frame::new` (Init) and `Frame::palette` (GetPalette) to score 3. `SetState` mapping is N/A — no override symbol in frame.rs; module doc covers the structural non-override. `CFrame` palette `Role` items raised to 3 in the theme.rs Role pass (cross-reference table added to `Role` enum doc in `src/theme.rs`).
