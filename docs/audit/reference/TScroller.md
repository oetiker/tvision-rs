# TScroller  (guide pp. 527–530)

Rust module(s): `src/widgets/scroller.rs`   |   magiblot: `include/tvision/views.h` / `source/tvision/tscrolle.cpp`

> **Key seam:** In C++, `TScroller` holds raw `TScrollBar*` pointers and calls them
> directly (`hScrollBar->setValue(x)`, reading `hScrollBar->value`). In Rust, a leaf
> holds only `&mut Context`, so the scroller stores `Option<ViewId>` handles and the
> event loop (pump) brokers all cross-view reads and writes at deferred-apply time (D3).
> `scrollDraw` (read sync) becomes `Deferred::SyncScrollerDelta`; `scrollTo`/`setLimit`
> (writes) become `Deferred::ScrollBarSetParams`. The `drawLock`/`drawFlag` re-entrancy
> guard for the draw-suppression-during-setLimit path is unneeded under whole-tree
> redraw (D9) and is dropped.
>
> **Palette note:** `CScroller` has 2 entries: `[1]`=Normal (→window slot 6),
> `[2]`=Selected/Highlight (→window slot 7). Rust maps these to `Role::ScrollerNormal`
> and `Role::ScrollerSelected` (D7). The base `Scroller::draw` uses only
> `ScrollerNormal`; `ScrollerSelected` exists for subclasses (the editor).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Delta` (field) | 528 | PORTED | OK | `Scroller::delta: Point` (pub) | 2 | Guide: `Delta.X`/`Delta.Y` hold the scroll offset; subclasses draw content shifted by it. Rust: identical semantics, `pub` so subclasses read it. Doc explains what; missing "subclasses must consume this in their `draw` override" guidance. |
| `HScrollBar` (field) | 528 | EQUIVALENT | OK | `Scroller::h_scroll_bar: Option<ViewId>` (private) + accessor `h_scroll_bar() -> Option<ViewId>` | 2 | C++: raw `TScrollBar*` pointer (public, Read only in guide). Rust: `Option<ViewId>` handle + pub accessor. Known mapping: raw pointer → `ViewId` (D3). Private field, pub accessor. Doc on accessor: what it returns; the "why ViewId" broker story is in the module doc, not the accessor. |
| `Limit` (field) | 528 | PORTED | OK | `Scroller::limit: Point` (private) + `limit() -> Point` accessor | 2 | Guide: `Limit.X`/`Limit.Y` are max allowed values for `Delta`. Rust: private field, pub accessor. Doc explains what; missing "set via `set_limit`" cross-ref. |
| `VScrollBar` (field) | 528 | EQUIVALENT | OK | `Scroller::v_scroll_bar: Option<ViewId>` (private) + accessor `v_scroll_bar() -> Option<ViewId>` | 2 | Same as `HScrollBar`. |
| `drawLock` (field) | — | NOT-PORTED | — | — | — | C++ re-entrancy counter to suppress `drawView` calls during `setLimit`/`scrollTo` (used with `checkDraw`). Dropped under whole-tree redraw (D9): the pump redraws the full tree once per tick; no re-entrancy guard is needed. The module doc calls this out explicitly: "The original draw-re-entrancy guard is unneeded under whole-tree redraw and is dropped." |
| `drawFlag` (field) | — | NOT-PORTED | — | — | — | Deferred-draw flag paired with `drawLock`. Dropped for the same D9 reason as `drawLock`. |
| `Init` (constructor) | 528 | PORTED | OK | `Scroller::new(bounds, h_scroll_bar, v_scroll_bar)` | 2 | Guide: sets `ofSelectable`, `evBroadcast` mask, zeros delta/limit, takes bar pointers. Rust: sets `options.selectable = true`; `evBroadcast` mask is absent because broadcasts are delivered unconditionally (noted in both constructor doc and module doc); zeros delta/limit; takes `Option<ViewId>`. Faithful modulo D3 pointer→ViewId. Doc explains what; "why no evBroadcast" note in doc earns credit but is in the module-level prose rather than the constructor doc itself. Score 2. |
| `Load` (constructor) | 528 | NOT-PORTED | — | — | — | `TStreamable` / stream serialisation dropped (serde-if-revived). |
| `ChangeBounds` (method) | 529 | EQUIVALENT | OK | `Scroller::on_bounds_changed(ctx)` (impl `View::on_bounds_changed`) | 2 | C++ `changeBounds`: calls `setBounds`, then wraps `setLimit` in a `drawLock++/--` guard to suppress immediate draw, then calls `drawView`. Rust: bounds geometry is applied by the pump via `Deferred::ChangeBounds`; afterwards `on_bounds_changed` is called, which calls `set_limit` to re-publish bar params. No drawLock needed (D9). The shape is different (`changeBounds` hook → `on_bounds_changed`) but the effect is identical. Doc explains what; the D9 reason for no drawLock could be more explicit. |
| `GetPalette` (method) | 529 | EQUIVALENT | OK | `Role::ScrollerNormal` + `Role::ScrollerSelected` via `ctx.style()` | 2 | C++ returns `CScroller` 2-entry palette. Rust uses `Role` variants read from `Theme`. Known mapping: class Palette → `tv::Theme` (D7). Roles documented (what); palette chain not in scroller rustdoc. → concept: palette chain guide. |
| `HandleEvent` (method) | 529 | PORTED | OK | `Scroller::handle_event` (impl `View::handle_event`) | 3 | Guide: handles `cmScrollBarChanged` broadcast from either bar by calling `scrollDraw`. Rust: filters on `source ∈ {h_scroll_bar, v_scroll_bar}` and queues `Deferred::SyncScrollerDelta` (the read broker). The `source` acts as a filter (D4). Not calling `scrollDraw` directly — broker does the actual sync. The module doc and the handle_event doc both explain the broker pattern fully. Score 3. |
| `ScrollDraw` (method) | 529 | EQUIVALENT | OK | `Scroller::apply_delta(d: Point)` called by the pump as the read-broker | 2 | C++ `scrollDraw`: reads bar values directly, adjusts cursor, sets delta, calls `drawView` (or sets drawFlag). Rust: the pump resolves bars, reads their `value`, assembles `d`, calls `apply_delta` on the scroller, which adjusts the cursor and updates `delta`. Whole-tree redraw replaces `drawView` (D9). The `apply_delta` doc explains the cursor-adjust order (old delta must precede overwrite). Missing: explicit note that `apply_delta` IS the Rust analog of `scrollDraw`. |
| `ScrollTo` (method) | 529 | EQUIVALENT | OK | `Scroller::scroll_to(x, y, ctx)` — issues `Deferred::ScrollBarSetParams` for value only | 2 | C++ calls `hScrollBar->setValue(x)` / `vScrollBar->setValue(y)` directly (wrapped in drawLock). Rust queues `request_scroll_bar_params` with only `value` set. Same net effect; drawLock not needed (D9). Doc explains what; missing drawLock/D9 rationale. |
| `SetLimit` (method) | 529 | EQUIVALENT | OK | `Scroller::set_limit(x, y, ctx)` — issues `Deferred::ScrollBarSetParams` for range+page | 2 | C++ calls `hScrollBar->setParams(value, 0, x-size.x, size.x-1, arStep)` directly (wrapped in drawLock). Rust queues `request_scroll_bar_params` preserving value and arrow_step, setting min/max/page_step. Semantically identical; drawLock/D9 deviation undocumented at method level. |
| `SetState` (method) | 529 | PORTED | OK | `Scroller::set_state` (impl `View::set_state`) | 2 | Guide: when `sfActive` or `sfSelected` changes, show or hide both bars. Rust: overrides `set_state`; on `Active` or `Selected` flags, calls `show_sbar` → deferred `SetVisible`. Also emits `RECEIVED_FOCUS`/`RELEASED_FOCUS` broadcast on `Focused` (standard `View` base behaviour). Bars shown when `active || selected` — matches C++ `getState(sfActive | sfSelected) != 0`. Doc explains what; "why deferred SetVisible instead of direct show/hide" could be noted. |
| `Store` (method) | 530 | NOT-PORTED | — | — | — | `TStreamable` / stream serialisation dropped (serde-if-revived). |
| `shutDown` (method) | — | NOT-PORTED | — | — | — | C++ `shutDown` nulls the bar pointers on teardown. Rust: `ViewId` handles are just integers; no nulling needed — the scroller is simply dropped. No analog required. |
| `checkDraw` (method) | — | NOT-PORTED | — | — | — | Helper that calls `drawView` when `drawLock == 0 && drawFlag`. Dropped with `drawLock`/`drawFlag` (D9). |
| `CScroller` palette (2 entries) | 530 | EQUIVALENT | OK | `Role::ScrollerNormal` + `Role::ScrollerSelected` in `Theme` | 2 | Entries 1–2 map to window palette slots 6–7. Rust: `ScrollerNormal` (app palette index 13 for blue window = `0x1E` yellow-on-blue) and `ScrollerSelected` (index 14 = `0x71` blue-on-lightgray). Note: the base `Scroller::draw` only uses `ScrollerNormal`; `ScrollerSelected` is intentionally unused in the base (module doc explains this explicitly — the editor applies its own selection colour). Known mapping: class Palette → `tv::Theme` (D7). |

## Summary

- PORTED: 6   EQUIVALENT: 8   NOT-PORTED: 5   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 11   |   → concept: 1
- Notable finding: `apply_delta` (the Rust analog of C++ `scrollDraw`) does not identify itself as such in its rustdoc — a reader looking for the `scrollDraw` counterpart will find the connection only by reading the module-level "Turbo Vision heritage" section, not the method doc.
