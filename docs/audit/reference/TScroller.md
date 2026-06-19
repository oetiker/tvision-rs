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
| `Delta` (field) | 528 | PORTED | OK | `Scroller::delta: Point` (pub) | 3 | Raised to 3: doc now explains what it is + how subclasses use it in `draw` (shift content by `r - delta.y`), and explicitly says not to write it directly (use `scroll_to`). |
| `HScrollBar` (field) | 528 | EQUIVALENT | OK | `Scroller::h_scroll_bar: Option<ViewId>` (private) + accessor `h_scroll_bar() -> Option<ViewId>` | 3 | Raised to 3: field is private (internal comment only); public accessor doc now explains when to use it (subclass broker operations), what `None` means, and includes a `# Turbo Vision heritage` section explaining the raw-pointer → `ViewId` broker story. |
| `Limit` (field) | 528 | PORTED | OK | `Scroller::limit: Point` (private) + `limit() -> Point` accessor | 3 | Raised to 3: field is private (internal comment). Public accessor doc now explains the returned value, subclass use cases, and cross-refs `set_limit` for mutations. |
| `VScrollBar` (field) | 528 | EQUIVALENT | OK | `Scroller::v_scroll_bar: Option<ViewId>` (private) + accessor `v_scroll_bar() -> Option<ViewId>` | 3 | Raised to 3: same treatment as `HScrollBar`; cross-references h_scroll_bar for broker rationale. |
| `drawLock` (field) | — | NOT-PORTED | — | — | — | C++ re-entrancy counter to suppress `drawView` calls during `setLimit`/`scrollTo` (used with `checkDraw`). Dropped under whole-tree redraw (D9): the pump redraws the full tree once per tick; no re-entrancy guard is needed. The module doc calls this out explicitly: "The original draw-re-entrancy guard is unneeded under whole-tree redraw and is dropped." |
| `drawFlag` (field) | — | NOT-PORTED | — | — | — | Deferred-draw flag paired with `drawLock`. Dropped for the same D9 reason as `drawLock`. |
| `Init` (constructor) | 528 | PORTED | OK | `Scroller::new(bounds, h_scroll_bar, v_scroll_bar)` | 3 | Raised to 3: doc now explains how to wire bars at construction time, that `set_limit` must be called afterwards, that the view is selectable, and why no broadcast mask setup is needed (broadcasts are unconditional). |
| `Load` (constructor) | 528 | NOT-PORTED | — | — | — | `TStreamable` / stream serialisation dropped (serde-if-revived). |
| `ChangeBounds` (method) | 529 | EQUIVALENT | OK | `Scroller::on_bounds_changed(ctx)` (impl `View::on_bounds_changed`) | 3 | Raised to 3: doc now explains that it is called by the pump *after* bounds are applied (so `self.size` is already updated), re-publishes via `set_limit`, and notes that subclass overrides should call `set_limit` or delegate. |
| `GetPalette` (method) | 529 | EQUIVALENT | OK | `Role::ScrollerNormal` + `Role::ScrollerSelected` via `ctx.style()` | 3 | `Role::ScrollerNormal` (yellow on blue, `0x1E`, chain) and `Role::ScrollerSelected` (blue on lightgray, `0x71`, chain) documented in `src/theme.rs` (theme pass), both naming `Scroller`/`Editor` as consumers. No public scroller `get_palette` method exists — lookup is inlined at draw time per deviation D7. |
| `HandleEvent` (method) | 529 | PORTED | OK | `Scroller::handle_event` (impl `View::handle_event`) | 3 | Guide: handles `cmScrollBarChanged` broadcast from either bar by calling `scrollDraw`. Rust: filters on `source ∈ {h_scroll_bar, v_scroll_bar}` and queues `Deferred::SyncScrollerDelta` (the read broker). The `source` acts as a filter (D4). Not calling `scrollDraw` directly — broker does the actual sync. The module doc and the handle_event doc both explain the broker pattern fully. Score 3. |
| `ScrollDraw` (method) | 529 | EQUIVALENT | OK | `Scroller::apply_delta(d: Point)` called by the pump as the read-broker | 3 | Raised to 3: doc now explicitly states this is the Rust analog of C++ `scrollDraw`, explains it is called by the pump (not user code), the cursor-adjust order, and includes a `# Turbo Vision heritage` section naming the original. |
| `ScrollTo` (method) | 529 | EQUIVALENT | OK | `Scroller::scroll_to(x, y, ctx)` — issues `Deferred::ScrollBarSetParams` for value only | 3 | Raised to 3: doc explains the deferred-op path, that the bar clamps the value, that a `SCROLL_BAR_CHANGED` broadcast follows, and when to use `scroll_to` vs `set_limit`. |
| `SetLimit` (method) | 529 | EQUIVALENT | OK | `Scroller::set_limit(x, y, ctx)` — issues `Deferred::ScrollBarSetParams` for range+page | 3 | Raised to 3: doc spells out the formula for each bar's `min`/`max`/`page_step`, notes preserved fields, explains the deferred-op path, and cross-refs `on_bounds_changed` (auto-called on resize). |
| `SetState` (method) | 529 | PORTED | OK | `Scroller::set_state` (impl `View::set_state`) | 3 | Raised to 3: doc now explains the `Active || Selected` visibility rule, that visibility goes through a deferred `SetVisible` op brokered by the pump, and what happens for `Focused` (broadcast but no bar change). |
| `Store` (method) | 530 | NOT-PORTED | — | — | — | `TStreamable` / stream serialisation dropped (serde-if-revived). |
| `shutDown` (method) | — | NOT-PORTED | — | — | — | C++ `shutDown` nulls the bar pointers on teardown. Rust: `ViewId` handles are just integers; no nulling needed — the scroller is simply dropped. No analog required. |
| `checkDraw` (method) | — | NOT-PORTED | — | — | — | Helper that calls `drawView` when `drawLock == 0 && drawFlag`. Dropped with `drawLock`/`drawFlag` (D9). |
| `CScroller` palette (2 entries) | 530 | EQUIVALENT | OK | `Role::ScrollerNormal` + `Role::ScrollerSelected` in `Theme` | 3 | Both roles documented in `src/theme.rs` (theme pass) — see `GetPalette` row above. |

## Summary

- PORTED: 5   EQUIVALENT: 8   NOT-PORTED: 6   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- All public symbols raised to score 3. `GetPalette` and `CScroller` palette rows raised to 3 in the theme.rs Role pass: `Role::ScrollerNormal` (yellow on blue) and `Role::ScrollerSelected` (blue on lightgray) now carry full chain + widget context in `src/theme.rs`. Notable resolution: `apply_delta` explicitly identifies itself as the Rust analog of C++ `scrollDraw`.
