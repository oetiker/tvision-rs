# TColorIndex type  (guide p. 410)

Rust module(s): none (type superseded)   |   magiblot: `include/tvision/colorsel.h`

> **Note:** `TColorIndex` is a record used only by `TColorDialog::GetIndexes` /
> `SetIndexes` to save and restore the dialog box's focused-item state on a
> stream.  tvision-rs does not have a dialog that needs this state-save protocol:
> `Program::color_dialog` opens a fresh `ColorPicker` each time, returning an
> `Option<Color>`.  The entire mechanism is superseded.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `GroupIndex` (field, `Byte`) | 410 | NOT-PORTED | — | — | — | Index of the focused color group; concept absent |
| `ColorSize` (field, `Byte`) | 410 | NOT-PORTED | — | — | — | Number of valid entries in `ColorIndex[]`; concept absent |
| `ColorIndex` (field, `array[0..255] of Byte`) | 410 | NOT-PORTED | — | — | — | Per-group focused item indexes; entire state-save mechanism superseded by returning `Option<Color>` |

## Summary

- PORTED: 0   EQUIVALENT: 0   NOT-PORTED: 3   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable: Entire record NOT-PORTED. The state-restoration purpose is obsolete because `color_dialog` does not persist group/item focus state; it opens fresh each time.
