# THistory  (guide pp. 455–457)

Rust module(s): `src/widgets/history.rs` (`struct THistory`)   |   magiblot: `include/tvision/dialogs.h` / `source/tvision/thistory.cpp`

> THistory is the dropdown-arrow icon view placed next to an `InputLine`. Its
> own documented fields are `link` and `historyId`; its documented methods are
> `draw`, `getPalette`, `handleEvent`, `initHistoryWindow`, and `recordHistory`.
> Palette `CHistory` has 2 entries (arrow, sides).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `link` (field) | 455 | EQUIVALENT | OK | `THistory.link: ViewId` | N/A | Private field (`link: ViewId`); has an inline comment. Not public API — does not count toward public bar closure. |
| `historyId` (field) | 455 | PORTED | OK | `THistory.history_id: u8` | N/A | Private field (`history_id: u8`); has an inline comment. Not public API — does not count toward public bar closure. |
| `Init` (constructor) | 455 | PORTED | OK | `tv::THistory::new(bounds, link, history_id)` | 3 | Raised: rustdoc now explains when to use it (next to an InputLine, size the icon 3 cells wide, insert both into the same group), plus the post-process opt-in and non-selectable behavior. |
| `Draw` (method) | 456 | PORTED | OK | `tv::THistory::draw` (impl `View::draw`) | 3 | C++: draws `icon` string with `getColor(0x0102)`. Rust: draws `"▐~↓~▌"` with `Role::HistorySides` / `Role::HistoryArrow` roles. Same two-color split icon, roles documented in method rustdoc. |
| `GetPalette` (method) | 456 | EQUIVALENT | OK | `tv::theme::Role::HistoryArrow` + `Role::HistorySides` via `ctx.style()` | 3 | `Role::HistoryArrow` and `Role::HistorySides` already carried score-3 docs (color, chain, glyph context). Confirmed in theme.rs Role pass. |
| `HandleEvent` (method) | 456 | EQUIVALENT | OK | `tv::THistory::handle_event` (impl `View::handle_event`) | 3 | C++ inline: focuses link, records, computes bounds, calls `initHistoryWindow` + `execView`, writes selection back, `selectAll`, `drawView`. Rust: deferred — queues `Deferred::OpenHistory` or `Deferred::RecordHistory`; the pump performs the modal build/drive/write-back. Semantically equivalent but structurally deferred. Documented in rustdoc. |
| `InitHistoryWindow` (method) | 457 | EQUIVALENT | OK | inline in pump via `Deferred::OpenHistory` | N/A | No separate public Rust method — the logic is inlined in the event-loop's `OpenHistory` arm. Not reachable via a doc-only change. `handle_event` rustdoc describes the deferred path. |
| `RecordHistory` (method) | 457 | EQUIVALENT | OK | inline in pump via `Deferred::RecordHistory` | N/A | No separate public Rust method — the logic is inlined in the event-loop's `RecordHistory` arm. `handle_event` rustdoc describes the broadcast arm that queues the deferred request. |
| `ShutDown` (method) | — | NOT-PORTED | — | — | — | C++: `shutDown()` nulls `link`. Rust: `ViewId` — no pointer to null; drop is automatic. No counterpart needed. |
| `Load`/`Store` (streamable) | — | NOT-PORTED | — | — | — | `TStreamable` / `write` / `read` / `build`. Dropped project-wide. |
| `CHistory` palette (2 entries) | 457 | EQUIVALENT | OK | `tv::theme::Role::HistoryArrow`, `tv::theme::Role::HistorySides` | 3 | Already score-3 in `src/theme.rs` (glyph, color, chain). Confirmed in theme.rs Role pass. |

## Summary

- PORTED: 3   EQUIVALENT: 6   NOT-PORTED: 2   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Raised: `Init/new` → 3. `GetPalette` and `CHistory` palette rows raised to 3 in the theme.rs Role pass (roles already score-3 in `src/theme.rs`). Private fields (`link`, `history_id`) and pump-inlined methods (`InitHistoryWindow`, `RecordHistory`) are N/A for the public bar.
- Notable finding: `initHistoryWindow` omits the C++ `helpCtx` copy (`p->helpCtx = link->helpCtx`). This is functionally silent (help context routing is not ported), but it is not documented as a deliberate omission — a comment on `Deferred::OpenHistory` noting the omission would close the gap for future help-routing work.
