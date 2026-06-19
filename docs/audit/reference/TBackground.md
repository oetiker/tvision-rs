# TBackground  (guide pp. 382–383)

Rust module(s): src/desktop/background.rs   |   magiblot: include/tvision/app.h / source/tvision/tbkgrnd.cpp

> TBackground has one own field (`pattern`) and three methods (`Init`/ctor,
> `draw`, `getPalette`). The C++ palette is `CBackground` (one entry, `\x01`).
> The streaming methods (`write`, `read`, `Store`, `Load`) are also in the C++
> source but are part of `TStreamable` machinery dropped by deviation D12.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `pattern` (field, `char`) | 382 | PORTED | OK | `Background.pattern: char` | 3 | C++ `char`; Rust `char` (Unicode scalar — the design-time value is always a single display glyph so this is safe). Field is `pub` in Rust (matching C++ `public`). Doc now explains "set before or after construction to customise the fill character" plus the double-width caveat. |
| `Init` (constructor) | 382 | PORTED | OK | `tv::Background::new(bounds: Rect, pattern: char) -> Background` | 3 | C++: sets `growMode = gfGrowHiX|gfGrowHiY`, stores `pattern`. Rust matches exactly. Grow mode tested. Well documented (what + how + grow-mode rationale). |
| `draw` (method) | 382 | PORTED | OK | `tv::Background::draw` (impl `View::draw`) | 3 | C++: fills `size.x × size.y` with `pattern` at `getColor(0x01)` (first application palette entry = background colour). Rust: `ctx.fill(ext, self.pattern, ctx.style(Role::Background))`. `Role::Background` chains to palette index 1 via the theme (deviation D7, documented). Functionally identical. Well documented. |
| `getPalette` (method) | 383 | EQUIVALENT | OK | `tv::theme::Role::Background` + `tv::Theme::style` | 3 | C++ returns a one-entry palette (`cpBackground = "\x01"`) mapping to application palette slot 1 (the background colour). Rust folds the entire palette chain into `Role::Background` looked up through `Theme::style` at draw time (deviation D7). `Role::Background` now documents the color (lightgray on blue, `0x71`), the chain, and the widget that uses it. Documented in `src/theme.rs` Role docs. |
| `CBackground` palette (1 entry) | 383 | EQUIVALENT | OK | `tv::theme::Role::Background` | 3 | Single entry `\x01` — documented in `src/theme.rs` Role docs (theme pass). |
| `TStreamable` / `write` / `read` / `Store` / `Load` | 383 | NOT-PORTED | — | — | — | DOS-era streaming machinery. Dropped (deviation D12). Module doc records this explicitly. |

## Summary

- PORTED: 3   EQUIVALENT: 2   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: `pattern` raised to 3. `Role::Background` raised to 3 in the theme.rs Role pass (documented in `src/theme.rs`): color chain (`cpBackground[1]=0x01 → cpAppColor[1]=0x71`), widget context, `Background::draw` call site.
