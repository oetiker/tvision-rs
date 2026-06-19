# TDesktop  (guide pp. 412–415)

Rust module(s): src/desktop/desktop.rs   |   magiblot: include/tvision/app.h / source/tvision/tdesktop.cpp

> TDeskTop inherits from TGroup (and TDeskInit). Its guide-documented own
> members are: two fields (`background`, `tileColumnsFirst`) and six methods
> (`Init`/ctor, `cascade`, `handleEvent`, `initBackground`, `tile`, `tileError`).
> `getTileRect` is documented under **TApplication** (guide p. 413 sidebar),
> not TDeskTop; it is included here because the task spec lists it.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `background` (field, `TBackground*`) | 412 | EQUIVALENT | OK | `Desktop.background: Option<ViewId>` | 3 | C++ stores a raw pointer; Rust uses a `ViewId` handle. The field is private but exposed via `Desktop::background() -> Option<ViewId>`. Doc now explains the PREV-cycle usage (pass to `put_in_front_of` to send the current window behind the background). |
| `tileColumnsFirst` (field, `Boolean`) | 412 | PORTED | OK | `Desktop.tile_columns_first: bool` | 2 | Protected in C++; private in Rust (same effective visibility — only `tile` reads it). Defaults to `false` in both. Doc on `tile` names the field; the field itself has an inline comment only — no rustdoc. Score reflects the method doc's coverage. |
| `Init` (constructor) | 413 | PORTED | OK | `tv::Desktop::new(bounds, create_background)` | 3 | C++ sets `growMode = gfGrowHiX|gfGrowHiY`, `tileColumnsFirst = False`, inserts background via `createBackground`. Rust matches exactly; background factory injected rather than virtual (deviation D2 embed-and-delegate). Well documented. |
| `cascade` (method) | 413 | PORTED | OK | `tv::Desktop::cascade` (impl `View::cascade`) | 3 | C++: count tileable views, check min-size against `r` minus count, call `tileError` on failure, else `lock`/walk/`unlock`. Rust: same fit guard, same offset formula (`n-1` → `0`), no `lock`/`unlock` (whole-tree redraw, D9, documented). `tileError()` replaced by a silent early return (see `tileError` row). Well documented. |
| `handleEvent` (method) | 413 | PORTED | OK | `tv::Desktop::handle_event` (impl `View::handle_event`) | 3 | C++: `cmNext` → `selectNext(False)` (forwards); `cmPrev` → `current->putInFrontOf(background)`; both clear event. Rust: `NEXT` → `focus_next(false)`; `PREV` → `put_in_front_of(cur, self.background)`. Guard `valid(RELEASED_FOCUS)` on both arms; event cleared unconditionally (matching C++ `default: return` before `clearEvent`). Documented. |
| `initBackground` (static method) | 413 | PORTED | OK | `tv::Desktop::init_background(r: Rect) -> Box<dyn View>` | 3 | C++ creates `TBackground(r, defaultBkgrnd)` where `defaultBkgrnd = '\xB0'` (CP437 light shade). Rust creates `Background::new(r, '\u{2591}')` (Unicode U+2591 LIGHT SHADE — the Unicode counterpart of CP437 0xB0). Deviation documented in inline comment and snapshot test. |
| `tile` (method) | 414 | PORTED | OK | `tv::Desktop::tile` (impl `View::tile`) | 3 | C++: count tileable, `mostEqualDivisors`, fit guard → `tileError`, else `lock`/walk/`unlock`. Rust: identical grid math, no `lock`/`unlock` (D9), silent early return replaces `tileError()`. Documented. |
| `tileError` (method) | 414 | NOT-PORTED | — | — | — | C++ `tileError()` is a virtual no-op hook for subclasses to display an error. Rust replaces both call sites (in `tile` and `cascade`) with a silent early return — the caller simply leaves window bounds unchanged. The hook could be re-added if a subclass ever needs it; noted in comments. Not ported: no subclassing model (D2 embed-and-delegate). |
| `shutDown` (method) | 414 | NOT-PORTED | — | — | — | C++ `shutDown` nulls `background` then calls `TGroup::shutDown`. Rust drops the whole tree via RAII; no separate shutdown phase (deviation D12, documented in module doc). |
| `getTileRect` (method on TApplication) | 415 | EQUIVALENT | OK | `tv::Application::get_tile_rect() -> Option<Rect>` | 3 | C++ lives on `TApplication`, not `TDeskTop`. Rust lands on `Application` (`src/app/application.rs`). Returns the desktop's local-origin extent (the rect passed to `tile`/`cascade`). Returns `None` when no desktop was inserted. Rustdoc now adds "how to use" context: call it directly for window-positioning logic, adjust by sizing the desktop rather than overriding, heritage note on virtual C++ method. |

## Summary

- PORTED: 6   EQUIVALENT: 2   NOT-PORTED: 2   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: `background()` raised to 3 (added PREV-cycle usage context). `getTileRect`/`Application::get_tile_rect` raised to 3 — rustdoc now adds "how to use" guidance (call for window positioning, adjust by sizing desktop, heritage note on virtual C++).
