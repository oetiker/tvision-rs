# TDrawBuffer  (guide pp. 420–421)

Rust module(s): `src/screen/draw_buffer.rs`   |   magiblot: `include/tvision/drawbuf.h`

> `TDrawBuffer` is a plain type — no inheritance, no fields documented by the guide.
> The guide documents the type declaration (one entry) and its role (one entry), plus
> the seven public methods ported from the class definition in `drawbuf.h`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TDrawBuffer` type declaration (`array[0..MaxViewWidth-1] of Word`) | 420 | EQUIVALENT | OK | `tv::DrawBuffer` (`Vec<Cell>` of an explicit width) | 3 | Guide: fixed array of `Word` (char+attr packed). magiblot modernised to `TScreenCell *data; size_t capacity`. Rust uses `Vec<Cell>` (each `Cell` carries a Unicode cluster + `Style`). Idiomatic mapping: typed cell model (deviation D6). Module doc explains the dropped `0 = retain` sentinel and the Unicode extension. The heritage section is present and complete. |
| Function / role description ("used to declare buffers for Draw methods; filled line by line then written to screen") | 421 | PORTED | OK | module-level rustdoc of `src/screen/draw_buffer.rs` | 3 | The module doc covers the fill-and-blit usage pattern with an example that mirrors the Pascal snippet in the guide. |
| `moveChar(indent, c, attr, count)` | 421 | PORTED | OK | `tv::DrawBuffer::move_char(indent, ch, style, count)` | 3 | Guide: fills `count` cells from `indent` with `c`/`attr`; sentinel `0` means "retain existing". Rust: always writes both char and style (sentinel dropped, documented). Signature is faithful; `Style` replaces `TColorAttr`. Module note explains the drop. |
| `moveStr(indent, str, attr[, maxStrWidth, strIndent])` | 421 | PORTED | OK | `tv::DrawBuffer::move_str` + `move_str_part` | 3 | Guide: writes `str` with `attr`, optional width cap and starting offset. Rust splits into `move_str` (full-width from start) and `move_str_part` (all params exposed). Returns cells written (same as magiblot). Unicode/double-width handled via `text::draw_str` (deviation D13, documented). |
| `moveCStr(indent, str, attrs[, maxStrWidth, strIndent])` | 421 | PORTED | OK | `tv::DrawBuffer::move_cstr` + `move_cstr_part` | 3 | Guide: `~` toggles between two attributes. Rust: exact same `~`-toggle semantics, `lo`/`hi` `Style` pair instead of `TAttrPair`. Split into full-width and partial variants, matching `moveStr` pattern. |
| `moveBuf(indent, source, attr, count)` | 421 | EQUIVALENT | OK | `tv::DrawBuffer::move_buf(indent, src: &[Cell])` | 3 | Guide: copies raw word-buffer with a colour override (`attr` applied to every copied cell). magiblot modernised to `TScreenCell *source`, no attr override. Rust: copies `&[Cell]` verbatim — no attr override, matching the magiblot posture. The doc note explains this: callers pre-build the cells with the correct style. |
| `putAttribute(indent, attr)` | C++ header | PORTED | OK | `tv::DrawBuffer::put_attribute(indent, style)` | 3 | In `drawbuf.h` (inline), not in the 1992 guide body but in the magiblot header. Sets the style of a single cell, keeping its char. Doc now explains when to prefer it over `move_char` (restyle without changing the glyph). |
| `putChar(indent, c)` | C++ header | PORTED | OK | `tv::DrawBuffer::put_char(indent, ch)` | 3 | Inline in `drawbuf.h`. Sets the char of a single cell, keeping its style. Doc now explains when to prefer it over `move_char` (change glyph without changing colour). |
| `capacity` / `data` (protected fields) | C++ header | EQUIVALENT | OK | `DrawBuffer.data: Vec<Cell>` (private); `tv::DrawBuffer::capacity() -> usize` | 3 | Rust exposes capacity as a method, not a field. Data is private (idiomatic). `capacity()` doc explains it equals `width` and is the clip limit; `cells()` doc explains it as the completed-row hand-off. |

## Summary

- PORTED: 6   EQUIVALENT: 3   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: All public symbols raised to 3. `put_attribute`, `put_char`, `capacity`, and `cells` all received "when/how to use" guidance. No outstanding below-bar items.
