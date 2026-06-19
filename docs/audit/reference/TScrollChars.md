# TScrollChars type  (guide p. 527)

Rust module(s): `src/theme.rs` (`Glyphs` struct)   |   magiblot: `include/tvision/views.h` / `source/tvision/tvtext1.cpp`

> `TScrollChars` is `array[0..4] of Char` — a 5-element glyph array used to draw
> a `TScrollBar`. The C++ code keeps two static arrays (`vChars`, `hChars`) and copies
> the appropriate one into `TScrollBar::chars` at construction. The Rust port expands
> this into a unified `Glyphs` struct with 7 named fields (separating the v/h arrow
> pairs), held on `Theme` and read via `ctx.glyphs()`. This is `EQUIVALENT` (D7).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TScrollChars` type declaration (`array[0..4] of Char`) | 527 | EQUIVALENT | OK | `crate::theme::Glyphs` (struct with 7 `char` fields) | 3 | C++ `chars[0]`=back-arrow, `[1]`=fwd-arrow, `[2]`=page, `[3]`=indicator/thumb, `[4]`=no-range page. Two static arrays (`vChars`=`{'\x1E','\x1F','\xB1','\xFE','\xB2'}`, `hChars`=`{'\x11','\x10','\xB1','\xFE','\xB2'}`) are merged into `Glyphs`: `sb_v_arrow_back`/`sb_v_arrow_fwd` + `sb_h_arrow_back`/`sb_h_arrow_fwd` (split v/h arrows) + `sb_page` + `sb_thumb` + `sb_page_no_range`. All 5 logical glyph slots are present with Unicode codepoint cross-references (▲=`\u{25B2}`, ▒=`\u{2592}`, ■=`\u{25A0}`, etc.) documented on each `Glyphs` field. The `Glyphs` struct doc also provides a compact reference table for the original CP437 byte values. |
| `TScrollBar.chars` field (instance copy) | 524 | EQUIVALENT | OK | `ctx.glyphs()` read in `ScrollBar::draw` | N/A | C++ copies `vChars`/`hChars` into a per-instance `chars` field, allowing per-instance customization. Rust reads from the shared `Theme::glyphs()` — no per-instance override. This is an intentional simplification (no existing code overrides per-instance chars). If per-instance glyphs were needed the deviation would need a workaround. Not currently SUSPECT because no subclass or user code in the codebase relies on per-instance override. |

## Summary

- PORTED: 0   EQUIVALENT: 2   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- `TScrollChars` type declaration raised to 3 in the theme.rs Role pass: `Glyphs` struct fields already carry Unicode codepoint cross-references matching the original `vChars`/`hChars` arrays. Notable finding: The per-instance `TScrollBar::chars` field (enabling custom glyphs per scrollbar instance) has no Rust analog — `Glyphs` is theme-global. This is a deliberate simplification (no per-instance glyph hook).
