# TMonoSelector  (guide p. 485)

Rust module(s): none   |   magiblot: `include/tvision/colorsel.h`

> **Note:** The guide (p. 485) says "Details about `TMonoSelector` are included
> in the online Help."  The magiblot header shows it derives from `TCluster`
> with four methods (`draw`, `handleEvent`, `mark`, `press`) plus `newColor` and
> `movedTo`.  It displays four radio-button-style choices: Normal, Highlight,
> Underline, Inverse.
>
> tvision-rs uses `Style` / `Modifiers` (struct-of-bools: `bold`, `italic`,
> `underline`, `reverse`, `dim`) at the cell level (`src/color.rs`), and the
> rebuilt `ColorPicker` is purely a color picker (not a mono-attribute picker).
> There is no interactive monochrome-attribute selector widget.  The `Modifiers`
> struct-of-bools covers the *capability* (you can set underline/reverse/bold on
> any cell), but there is no dialog control to let the user pick them
> interactively.  This is a genuine capability gap for apps wanting a
> mono-attribute selector.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Init` / constructor | 485 | NOT-PORTED | — | — | — | Creates the monochrome attribute selector cluster (Normal/Highlight/Underline/Inverse radio buttons).  No equivalent widget exists. |
| `draw` (method) | 485 | NOT-PORTED | — | — | — | Draws four radio-style attribute choices; no equivalent. |
| `handleEvent` (method) | 485 | NOT-PORTED | — | — | — | Handles keyboard/mouse for the cluster; no equivalent. |
| `mark` (method) | 485 | NOT-PORTED | — | — | — | Returns whether the given item (attribute) is currently selected; no equivalent. |
| `newColor` (method) | 485 | NOT-PORTED | — | — | — | Broadcasts `cmColorSet` with the new attribute; no equivalent. |
| `press` (method) | 485 | NOT-PORTED | — | — | — | Sets the selected attribute; no equivalent. |
| `movedTo` (method) | 485 | NOT-PORTED | — | — | — | Updates focus and calls `newColor`; no equivalent. |
| Normal/Highlight/Underline/Inverse attribute choices | 485 | EQUIVALENT | OK | `crate::color::Modifiers` struct-of-bools (`bold`, `italic`, `underline`, `blink`, `reverse`, `strike`, `no_shadow`) in `src/color.rs` | 3 | The four Borland attributes (Normal, Highlight=bold, Underline, Inverse=reverse) all have counterparts in `Modifiers`. Struct doc now adds: how to use in a `Style::with_modifiers` call (with a doctest example), per-terminal support note, `no_shadow` marker explanation, and a heritage note explicitly mapping the four `TMonoSelector` attribute choices. |

## Summary

- PORTED: 0   EQUIVALENT: 1   NOT-PORTED: 7   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable: The monochrome attribute *values* (normal/bold/underline/reverse) are EQUIVALENT via `Modifiers`, but the interactive `TMonoSelector` widget itself is NOT-PORTED and has no replacement. This is a genuine gap for applications that want a user-facing attribute picker; however, the `TMonoSelector` was only ever shown inside `TColorDialog` (which is itself superseded by the rebuilt `ColorPicker`). The `Modifiers` row raised to 3: struct doc now adds a usage example, per-terminal support note, and explicit heritage mapping of the four `TMonoSelector` attribute choices.
