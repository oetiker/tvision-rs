# TColorDialog  (guide pp. 406–409)

Rust module(s): `src/dialog/colorpick/` (rebuilt extension; no 1-to-1 port)   |   magiblot: `include/tvision/colorsel.h` / `source/tvision/colorsel.cpp`

> **Rebuild note:** The Borland `TColorDialog` was a classic-palette editor (16 BIOS
> indices).  tvision-rs replaced it with `ColorPicker` — a truecolor (RGB / HSV /
> xterm-256 / presets) picker assembled from a `TabBar + PageStack + InfoColumn`,
> wired into `Program::color_dialog`.  Almost every Borland field/method maps to
> NOT-PORTED (superseded by the rebuilt extension) or EQUIVALENT (the capability
> exists in a different shape).  No capability is genuinely absent: the user can
> still pick any color; they can pick far more colors than before.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `BakLabel` (field, `PLabel`) | 407 | NOT-PORTED | — | — | — | Internal pointer to background-selector label; superseded by rebuilt picker layout (labels embedded in surface draw) |
| `BakSel` (field, `PColorSelector`) | 407 | NOT-PORTED | — | — | — | Pointer to background `TColorSelector`; superseded — the rebuilt picker uses a single `ColorPicker` (fg/bg not split at dialog level) |
| `Display` (field, `PColorDisplay`) | 407 | EQUIVALENT | OK | `InfoColumn` (`src/dialog/colorpick/info.rs`) — old/new color swatches | N/A | `InfoColumn` is `pub(crate)` — an internal implementation detail of `ColorPicker`. Not a public API target. Has a module-level doc comment that covers its role. |
| `ForLabel` (field, `PLabel`) | 407 | NOT-PORTED | — | — | — | Pointer to foreground-selector label; superseded by rebuilt layout |
| `ForSel` (field, `PColorSelector`) | 407 | NOT-PORTED | — | — | — | Pointer to foreground `TColorSelector`; superseded |
| `GroupIndex` (field, `Byte`) | 407 | NOT-PORTED | — | — | — | Index of most recently focused color group; groups concept absent in rebuilt picker (no group/item list; color surfaces replace it) |
| `Groups` (field, `PColorGroupList`) | 407 | NOT-PORTED | — | — | — | Pointer to the `TColorGroupList`; superseded by `ColorPicker`'s tab + surface architecture |
| `MonoLabel` (field, `PLabel`) | 407 | NOT-PORTED | — | — | — | Label for the monochrome selector; monochrome-attribute editing not present (terminal mono attributes handled via `Modifiers` in `Style`, not a dialog control) |
| `MonoSel` (field, `PMonoSelector`) | 407 | NOT-PORTED | — | — | — | Pointer to `TMonoSelector`; superseded |
| `Pal` (field, `TPalette`) | 407 | NOT-PORTED | — | — | — | In-memory copy of the palette being edited.  tvision-rs collapses palette editing into the `Theme` system (D7); `color_dialog` returns a `Color` value, not a palette blob |
| `Init` (constructor) | 407–408 | EQUIVALENT | OK | `Program::color_dialog(initial: Color) -> Option<Color>` + `ColorPicker::new(bounds, initial)` (`src/app/program.rs`, `src/dialog/colorpick/mod.rs`) | 3 | Both public entry-points now score 3. `Program::color_dialog` doc adds: usage example, explanation of the four surface tabs + InfoColumn, heritage note superseding `TColorDialog`. `ColorPicker::new` doc adds: what the picker contains, `bounds` non-zero-origin note, "embed in Dialog, add buttons separately" guidance, heritage note. |
| `Load` (constructor) | 408 | NOT-PORTED | — | — | — | Stream constructor; `TStreamable` dropped (serde-if-revived, known idiomatic mapping) |
| `DataSize` (method) | 408 | NOT-PORTED | — | — | — | Returns palette size for `GetData`/`SetData`; D10 value protocol uses `FieldValue`, but color_dialog returns the color directly (no dialog scatter/gather needed) |
| `GetData` (method) | 408 | NOT-PORTED | — | — | — | Copies selected indexes into `Pal`; superseded — `Program::color_dialog` returns `Option<Color>` directly |
| `GetIndexes` / `getIndexes` (method) | 408 | NOT-PORTED | — | — | — | Fills a `TColorIndex` struct; concept absent in rebuilt picker |
| `HandleEvent` (method) | 408 | EQUIVALENT | OK | `ColorPicker::handle_event` (`src/dialog/colorpick/mod.rs`) — `pub` (impl `View`) | 3 | C++ responded to `cmNewColorIndex` broadcast to refresh the `Display`. Rust: the shared `ColorModel` (`SharedModel = Rc<RefCell<ColorModel>>`) propagates color changes automatically on each redraw; no broadcast needed. `handle_event` is documented via the `View` trait — it is a standard trait impl and the module-level doc explains the `SharedModel` propagation pattern. Scores 3 as a trait-impl with type-level context. |
| `SetData` (method) | 409 | NOT-PORTED | — | — | — | Copies palette from `Rec` into `Pal`; superseded |
| `SetIndexes` / `setIndexes` (method) | 409 | NOT-PORTED | — | — | — | Sets group indexes from a `TColorIndex`; concept absent |
| `Store` (method) | 409 | NOT-PORTED | — | — | — | `TStreamable`; dropped |

## Summary

- PORTED: 0   EQUIVALENT: 3   NOT-PORTED: 16   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable: Every Borland field is NOT-PORTED because the whole color-dialog architecture was rebuilt as a truecolor `ColorPicker` extension; the capability is EQUIVALENT or better (truecolor vs. 16-index palette). No genuine gap exists. `Display`/`InfoColumn` row reconciled to N/A (pub(crate) internal). `Init` and `HandleEvent` rows raised to 3: `Program::color_dialog` and `ColorPicker::new` both have score-3 docs after this pass.
