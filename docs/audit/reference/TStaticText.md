# TStaticText  (guide pp. 536–537)

Rust module(s): src/widgets/static_text.rs   |   magiblot: include/tvision/dialogs.h / source/tvision/tstatict.cpp

> TStaticText has one own field (`Text`) and five methods documented by the guide,
> plus one palette entry. `Init`, `Load`, `Done`, and `Store` are lifecycle
> methods; `Draw`, `GetPalette`, and `GetText` are the operational API.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Text` (field) | 536 | EQUIVALENT | OK | `StaticText.text: String` (private) | N/A | C++: `const char*` (PString), 255-byte cap via `strncpy` in `getText`. Rust: `String`, no length cap — documented in struct and `set_text` doc. Private field; accessed via `text()` / `set_text()`. Known idiomatic mapping: owned String vs raw pointer. |
| `Init` (constructor) | 536 | PORTED | OK | `tv::StaticText::new(bounds: Rect, text: impl Into<String>) -> StaticText` | 3 | Guide: calls `TView::Init`, sets `Text` to `NewStr(AText)`, sets `GrowMode |= gfFixed`. Rust: sets `grow_mode.fixed = true`, stores text as String. Matches. Rustdoc now explains when to use (read-only captions/paragraphs), the `\n`/`\x03` embedding hints, the non-selectable/fixed-grow defaults, and the `ParamText` alternative for runtime-mutable content. |
| `Load` (constructor) | 536 | NOT-PORTED | — | — | — | `TStreamable` / stream persistence dropped project-wide (serde-if-revived, documented in CLAUDE.md). |
| `Done` (destructor) | 537 | NOT-PORTED | — | — | — | Rust `Drop` handles deallocation automatically; no explicit destructor needed. |
| `Draw` (method) | 537 | PORTED | OK | `tv::StaticText::draw` (impl `View::draw`) | 3 | Guide: draws text word-wrapped; `Ctrl+M` (0x0D) = new line, `Ctrl+C` (0x03) = center-this-line. C++ uses `0x0D` for newline in the guide description but actual source uses `'\n'` (0x0A); magiblot source likewise uses `'\n'`. Rust matches magiblot: uses `'\n'` for line break and `\x03` (ETX) for centering. Full algorithm documented in module-level doc and `draw` method doc. `gfFixed` + non-selectable defaults documented in `new`. |
| `GetPalette` (method) | 537 | EQUIVALENT | OK | `tv::theme::Role::StaticText` via `ctx.style(Role::StaticText)` in `draw` | N/A | C++ returns `CStaticText` (1 entry, maps to dialog palette entry 6). Rust uses `Role::StaticText` directly in `draw`; no public `palette()` method needed since the `Theme` system centralises colour lookup (D7). Private draw impl; N/A for rustdoc score. |
| `GetText` (method) | 537 | EQUIVALENT | OK | `tv::StaticText::text() -> &str` | 3 | C++ `getText(char* S)` copies text into a caller-provided buffer, capped at 255 bytes. Rust `text()` returns a `&str` reference — no copy, no cap. Rustdoc now explicitly calls out the zero-copy/no-cap deviation from the C++ buffer-out approach, addressing the C++ veteran comprehension gap. |
| `Store` (method) | 537 | NOT-PORTED | — | — | — | `TStreamable` / stream persistence dropped project-wide. |
| `CStaticText` palette (1 entry) | 537 | EQUIVALENT | OK | `tv::theme::Role::StaticText` | N/A | Guide: 1-entry palette, maps to dialog palette entry 6 ("Text color"). Rust uses `Role::StaticText` in `Theme`; `classic_blue()` assigns it the faithful BIOS color. Known idiomatic mapping: class Palette → `tv::Theme` (D7). Not a public symbol; N/A. |

## Summary

- PORTED: 2   EQUIVALENT: 4   NOT-PORTED: 3   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: No gaps or suspect items. The three NOT-PORTED entries are all stream-persistence machinery (`Load`/`Done`/`Store`) dropped project-wide by design. `new` and `text()` raised to score 3: `new` now covers when to use, formatting hints, and the `ParamText` alternative; `text()` explicitly documents the zero-copy/no-cap deviation from the C++ buffer-out approach.
