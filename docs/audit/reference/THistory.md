# THistory  (guide pp. 455–457)

Rust module(s): `src/widgets/history.rs` (`struct THistory`)   |   magiblot: `include/tvision/dialogs.h` / `source/tvision/thistory.cpp`

> THistory is the dropdown-arrow icon view placed next to an `InputLine`. Its
> own documented fields are `link` and `historyId`; its documented methods are
> `draw`, `getPalette`, `handleEvent`, `initHistoryWindow`, and `recordHistory`.
> Palette `CHistory` has 2 entries (arrow, sides).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `link` (field) | 455 | EQUIVALENT | OK | `THistory.link: ViewId` | 2 | C++: `TInputLine *link` raw pointer. Rust: `ViewId` (D3 known idiomatic mapping: pointer → ViewId). Private-ish field (struct not pub-fields). Doc: what it is, not how it resolves. |
| `historyId` (field) | 455 | PORTED | OK | `THistory.history_id: u8` | 2 | C++: `ushort historyId`. Rust narrows to `u8` (matching the store's key width). Field is private; module doc explains the `u8` choice. Slightly narrow vs. `ushort` but intentional and noted. |
| `Init` (constructor) | 455 | PORTED | OK | `tv::THistory::new(bounds, link, history_id)` | 2 | C++: sets `ofPostProcess`, `evBroadcast` mask. Rust: `state.options.post_process = true`; event mask implicit via `View::handle_event` match. Matches. "how/when to construct" could be expanded. |
| `Draw` (method) | 456 | PORTED | OK | `tv::THistory::draw` (impl `View::draw`) | 3 | C++: draws `icon` string with `getColor(0x0102)`. Rust: draws `"▐~↓~▌"` with `Role::HistorySides` / `Role::HistoryArrow` roles. Same two-color split icon, roles documented in method rustdoc. |
| `GetPalette` (method) | 456 | EQUIVALENT | OK | `tv::theme::Role::HistoryArrow` + `Role::HistorySides` via `ctx.style()` | 2 | C++ returns `cpHistory` 2-entry palette (`\x16\x17`). Rust resolves those same two palette indices through the full chain (documented in `theme.rs` comments) into `Role::HistoryArrow` / `Role::HistorySides`. Known idiomatic mapping: class Palette → `tv::Theme`. Public roles documented (what), not the full chain derivation. |
| `HandleEvent` (method) | 456 | EQUIVALENT | OK | `tv::THistory::handle_event` (impl `View::handle_event`) | 3 | C++ inline: focuses link, records, computes bounds, calls `initHistoryWindow` + `execView`, writes selection back, `selectAll`, `drawView`. Rust: deferred — queues `Deferred::OpenHistory` or `Deferred::RecordHistory`; the pump performs the modal build/drive/write-back. Semantically equivalent but structurally deferred (D-deferred-effects). Documented in rustdoc. |
| `InitHistoryWindow` (method) | 457 | EQUIVALENT | OK | inline in pump via `Deferred::OpenHistory` | 2 | C++: `THistory::initHistoryWindow` builds a `THistoryWindow` and copies `link->helpCtx`. Rust: the pump's `OpenHistory` arm builds the `HistoryWindow`; `helpCtx` copy is NOT present (no `help_ctx` on the Rust `InputLine`). The help-context carry is omitted — consistent with the project not porting help-context routing. Doc: rustdoc on `Deferred::OpenHistory` explains the deferred path; the help-ctx omission is not separately called out. |
| `RecordHistory` (method) | 457 | EQUIVALENT | OK | inline in pump via `Deferred::RecordHistory` | 2 | C++: `THistory::recordHistory(s)` calls `historyAdd(historyId, s)`. Rust: `Deferred::RecordHistory` queued; pump reads the link's value and calls `history_add`. Same effect, deferred shape. Doc describes the deferred arm in `handle_event`; could cross-reference the pump. |
| `ShutDown` (method) | — | NOT-PORTED | — | — | — | C++: `shutDown()` nulls `link`. Rust: `ViewId` — no pointer to null; drop is automatic. No counterpart needed (D3 idiomatic mapping). |
| `Load`/`Store` (streamable) | — | NOT-PORTED | — | — | — | `TStreamable` / `write` / `read` / `build`. Dropped project-wide (known: `TStreamable` dropped; serde-if-revived). |
| `CHistory` palette (2 entries) | 457 | EQUIVALENT | OK | `tv::theme::Role::HistoryArrow`, `tv::theme::Role::HistorySides` | 2 | C++: `cpHistory "\x16\x17"` → 2 entries (arrow, sides). Rust: two named `Role` variants; `classic_blue()` derives them via the full documented palette chain (theme.rs:785–786). Known idiomatic mapping: class Palette → `tv::Theme`. The roles themselves are documented at score 2 (what they represent, not the full derivation chain). |

## Summary

- PORTED: 3   EQUIVALENT: 5   NOT-PORTED: 2   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 8   |   → concept: 0
- Notable finding: `initHistoryWindow` omits the C++ `helpCtx` copy (`p->helpCtx = link->helpCtx`). This is functionally silent (help context routing is not ported), but it is not documented as a deliberate omission — a comment on `Deferred::OpenHistory` noting the omission would close the gap for future help-routing work.
