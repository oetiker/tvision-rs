# TColorSelector / TColorSel  (guide p. 411)

Rust module(s): `src/dialog/colorpick/` (rebuilt extension)   |   magiblot: `include/tvision/colorsel.h`

> **Note:** The guide (p. 411) says "Details of `TColorSelector`'s fields and
> methods are in the online Help."  The magiblot header shows it is a `TView`
> with `color`, `selType` (foreground / background), and `colorChanged()`.
> `TColorSel` is the enum `{ csBackground, csForeground }` that selects which
> kind of selector this is.
>
> In tvision-rs the fg/bg 4×4 BIOS-color grid concept is superseded by the
> rebuilt `ColorPicker` surfaces (Presets, RGB, HSV plane, Xterm-256).  There is
> no separate foreground vs. background selector at the picker level; the picker
> returns a single `Color` value, and the caller decides which role it applies to
> (via `Deferred::OpenColorDialogForRole` in the theme editor).  This is a
> deliberate architecture improvement, not a gap.

## TColorSelector object

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Init` (constructor) | 411 | NOT-PORTED | — | — | — | C++ `TColorSelector(bounds, selType)` creates a 4×4 BIOS-color grid; superseded by the rebuilt `ColorPicker` surfaces |
| `draw` (method) | 411 | NOT-PORTED | — | — | — | Draws the 4×4 BIOS color grid; superseded by `PresetsSurface::draw`, `RgbSurface::draw`, `PlaneSurface::draw`, `Xterm256Surface::draw` |
| `handleEvent` (method) | 411 | NOT-PORTED | — | — | — | Mouse/keyboard events on the grid; superseded by per-surface `handle_event` methods |
| `color` (field, `uchar`) | — | NOT-PORTED | — | — | — | Current selected color index; superseded by `ColorModel.color: Color` (`src/dialog/colorpick/model.rs:80`) |
| `selType` (field, `ColorSel`) | — | NOT-PORTED | — | — | — | Fg vs. bg selector discriminant; superseded — the picker returns one `Color` and the caller assigns it |
| `colorChanged` (private method) | — | NOT-PORTED | — | — | — | Broadcast `cmColorForegroundChanged` / `cmColorBackgroundChanged`; superseded by the `SharedModel` propagation pattern |

## TColorSel type (folded in)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TColorSel` enum (`csBackground`, `csForeground`) | 411 | NOT-PORTED | — | — | — | Discriminant for foreground vs. background color selector kind; no equivalent needed in the rebuilt picker (single-color-return design) |

## Summary

- PORTED: 0   EQUIVALENT: 0   NOT-PORTED: 7   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable: All NOT-PORTED. The 4×4 BIOS grid and fg/bg split are superseded by the truecolor picker surfaces. The fg/bg assignment responsibility is intentionally pushed to the caller (`theme_editor.rs` via `OpenColorDialogForRole`), which is a cleaner design than having the picker itself know about fg vs. bg.
