# Truecolor Color Picker — design spec

> **Status:** design approved (brainstorm complete), pending implementation plan.
> **Date:** 2026-06-09. **Type:** rstv-original *extension* (not a faithful port —
> see "Why this exists" / precedent: `RegexValidator`).

## Why this exists (and what it replaces)

The faithful tvision port reaches the **color-selection cluster** at PORT-ORDER
rows 81–87 (`TColorItem`/`TColorGroup`/`TColorIndex` → `TColorSelector` →
`TColorDisplay` → `TMonoSelector` → `TColorGroupList`/`TColorItemList` →
`TColorDialog`). That whole cluster exists in C++ for **one purpose: to edit
`app->palette->data[index]`** — the application's flat, runtime-mutable BIOS
attribute palette, indexed by `cpAppColor` offsets.

**rstv deleted that substrate.** Under **D7**, the palette chain
(`getPalette`/`getColor`/`mapColor` → `app->palette->data[]`) became a **`Theme`**:
a `Role → Style` map where `Role` is a *closed enum resolved at draw time by the
code that owns it*. There is no flat, indexable, runtime-editable attribute array
anywhere. `WindowPalette` is only a `Blue/Cyan/Gray` *tag*, not data. A faithful
port of rows 83–87 would therefore produce a dialog that edits a `TPalette`
**nothing in rstv ever reads** — dead code by construction.

**Decision (user, 2026-06-09):** drop the faithful cluster and build a *modern*
replacement instead — a reusable **truecolor color-picker** widget. A future
**theme editor** (runtime `Role → Style` editing; needs the D7 "Theme extension
point" that was deliberately not built) becomes a separate sub-project that
*consumes* this picker. This spec covers **only the picker widget + its modal
dialog**.

### Disposition of the already-landed faithful rows
- **Row 81** (`ColorItem`/`ColorGroup`/`ColorIndex`, commit `c92ed19`) and
  **row 82** (`ColorSelector`, commit `f3c34ad`) were ported faithfully before the
  premise was reconsidered. They are **superseded** by this picker (its Presets
  surface subsumes the 16-color BIOS grid; the palette-bookkeeping data classes
  have no consumer). **They will be removed** as part of this work (revert/delete
  `src/dialog/colordlg.rs`, the 3 `colordlg` snapshots, the `dialog/mod.rs`
  exports; the 3 `COLOR_*` commands in `command.rs` may be kept or dropped — they
  are unused by the picker).
- **PORT-ORDER rows 81–87** are marked **dropped/superseded** (a documented D7
  consequence, the same disposition as `TStreamable`), with a pointer to this
  spec. The port resumes at the **outline family (row 88)** after the picker
  lands (the picker is an extension, off the faithful critical path).

## Goals / non-goals

**Goals**
- A reusable, embeddable **`ColorPicker`** view that selects a single color and
  can return **any rstv `Color` variant**: `Default`, `Bios(0–15)`,
  `Indexed(0–255)`, `Rgb(r,g,b)`.
- Four input surfaces in a **tabbed** modal dialog: **Presets**, **RGB+hex**,
  **HSV plane**, **xterm-256 grid**.
- Persistent **old/new swatch + hex + variant readout**; **OK/Cancel**.
- Full **keyboard** control + **mouse incl. click-drag** scrubbing.
- Graceful degradation on lesser terminals (the existing `ColorDepth`
  quantization ladder handles this at render time — the picker always works in
  truecolor logically).

**Non-goals (v1)**
- The theme editor itself (separate sub-project; the picker is its building block).
- User-defined/persisted custom presets or a "recent colors" history (future).
- A polar color *wheel* (the rectangular HSV plane was chosen for usability).
- A `FieldValue::Color` D10 variant (the concrete `color()` accessor is the
  contract; D10 may be added later if a data-bound consumer needs it).

## Decisions (locked during brainstorming)

| Topic | Decision |
|---|---|
| First deliverable | The picker widget (standalone, reusable). Theme editor is a later consumer. |
| Output | Full `Color` model (Default / Bios / Indexed / Rgb); the chosen variant *is* the mode. |
| Surfaces | Presets · RGB+hex · HSV plane · xterm-256 grid (all four). |
| Layout | Tabbed (one surface at a time) + persistent info column + OK/Cancel. Dialog ~60×16. |
| "Wheel" | Rectangular **HSV plane** (hue strip + Saturation×Value box), not a polar disc. |
| Interaction | Keyboard everywhere + full mouse **including drag** (via the `window.rs` capture seam). |
| Architecture | **Approach A** — one `ColorPicker` view owning a shared `ColorModel`; surfaces are internal components, not separate Views. |

## Architecture (Approach A)

One `ColorPicker` `View` owns a single shared `ColorModel`. Each surface is a
focused **plain component** (not a `View`) that draws + handles events against
`&mut ColorModel`. A thin `color_dialog(initial) -> Option<Color>` modal shell
(a `Dialog` embedding the picker + OK/Cancel) is the entry point. Mouse drag uses
a per-picker `CaptureHandler` (the `window.rs DragCapture` precedent) + one new
`Deferred` variant applied in the pump.

Rationale: the four surfaces all mutate one color, so keeping them components of a
single view makes that state trivially consistent (no cross-view broadcast/broker
sync) while each surface stays small, isolated, and unit-testable. (Approach B —
separate Views synced by brokers — was rejected as overhead for one widget's
internal state; Approach C — one fat undivided view — as a grab-bag that's hard
to test.)

### Proposed module layout
```
src/dialog/colorpick/
  mod.rs        ColorPicker view, Tab enum, Surface trait, color_dialog() entry, re-exports
  model.rs      ColorModel, Hsv, conversions (rgb<->hsv, color->display-rgb, nearest-*)
  presets.rs    PresetsSurface + the preset table
  rgb.rs        RgbSurface (gauges + hex field)
  plane.rs      PlaneSurface (hue strip + SV box)
  xterm256.rs   Xterm256Surface (16x16 grid)
  drag.rs       ColorPickerDrag capture handler + apply_drag
```
Plus: `Deferred::ColorPickerDrag { picker, region, pos }` in `view/context.rs`,
its apply arm in `app/program.rs` (downcast to `ColorPicker` via `as_any_mut`),
and a BIOS→RGB table + `rgb↔hsv` helpers (in `model.rs`, or `quantize.rs` if the
BIOS table is reused elsewhere). The old `src/dialog/colordlg.rs` is removed.

## Section 1 — `ColorModel` (the shared truth)

Pure data + conversions; no drawing/events; independently unit-testable.

```rust
struct ColorModel {
    color: Color,   // the committed selection — exactly what OK returns
    hsv:   Hsv,     // working hue/sat/val, retained across edits
}
struct Hsv { h: f32 /*0..360*/, s: f32 /*0..1*/, v: f32 /*0..1*/ } // exact repr TBD in plan
```

- **`color` is the single source of truth.** Its *variant* is the "mode": picking
  from the 256-grid → `Indexed(n)`; from RGB/plane → `Rgb(r,g,b)`; from presets →
  `Default`/`Bios`/`Rgb`. No separate "current variant" flag.
- **`hsv` is retained separately** because HSV↔RGB is not round-trip-stable at the
  edges (value 0 ⇒ every hue is black; saturation 0 ⇒ hue undefined). Retaining
  working HSV means driving brightness to black and back does not scramble hue.
  When `color` is set from a non-plane surface, `hsv` is recomputed from that
  color's RGB.
- **Cursor/focus state does NOT live here.** Anything derivable from `color`/`hsv`
  is derived at draw time (256-grid cursor = current `Indexed`; plane cursor =
  current `hsv`). Only genuinely independent UI state (focused R/G/B field,
  presets scroll offset) lives in the surface that owns it.

**Conversions provided** (reusing `quantize.rs` where possible):
- `rgb ↔ hsv` — new, standard formulas (the only real new math).
- `color → display RGB` (for swatch/surfaces): `Bios(n)`→new 16-entry ANSI-RGB
  table; `Indexed(n)`→existing `xterm256_to_rgb`; `Rgb`→itself; `Default`→rendered
  as terminal-default (swatch shows a "default" marker, not a fake RGB).
- `rgb → nearest Indexed / nearest Bios` — existing `rgb_to_xterm256` /
  `rgb_to_bios` (to highlight where the current color lands on a grid).

## Section 2 — the four surfaces

Shared shape so the picker routes uniformly:
```rust
trait Surface {
    fn draw(&self, ctx: &mut DrawCtx, area: Rect, m: &ColorModel);
    fn handle_event(&mut self, ev: &mut Event, m: &mut ColorModel, ctx: &mut Context);
    fn mouse_hit(&self, p: Point, area: Rect) -> bool; // for click/drag routing
}
```
Each owns only its own UI state; all read/write the shared `&mut ColorModel`;
every edit sets `m.color` (and keeps `m.hsv` coherent).

**1. Presets** — a scrolling list of `{name, Color}`:
- `"Default"` → `Color::Default`
- the 16 BIOS colors by name → `Bios(0..15)`
  (Black, Blue, Green, Cyan, Red, Magenta, Brown, Light Gray, Dark Gray,
  Light Blue, Light Green, Light Cyan, Light Red, Light Magenta, Yellow, White)
- a curated ~12 common colors → `Rgb` (initial proposal, tweakable in the plan:
  Orange, Gold, Pink, Coral, Purple, Teal, Olive, Navy, Maroon, Lime, Aqua,
  Silver)

  Each row shows its name + a swatch cell; the row matching `m.color` is
  highlighted. *Local state:* scroll/selection index. *Nav:* ↑/↓ select → set
  `m.color`; click selects. Custom mini-list (not the `ListViewer` View).

**2. RGB + hex** — three R/G/B gauge bars (0–255, block-proportional, numeric
readout) + a `#RRGGBB` hex field + a live swatch. *Local state:* focused field.
*Nav:* ↑/↓ move between fields (R/G/B/hex); ←/→ adjust the focused channel ±1,
PgUp/PgDn ±16; typing edits the hex field (commit on a valid 6 digits). *Mouse:*
click a bar sets that channel by x-position, **drag scrubs**. Every change →
`m.color = Rgb(r,g,b)`, refresh `m.hsv`. *(No `Tab` — reserved for dialog nav.)*

**3. HSV plane** — a vertical hue spectrum strip + the Saturation×Value box
rendered in the current hue. Half-blocks double vertical resolution for smoother
gradients; truecolor cells (backend-quantized). A cursor marks `(sat, val)`; a
marker on the strip marks hue. *Local state:* none (the cursor is derived from
`m.hsv`). *Nav:* arrows move sat(x)/val(y) in the SV box; `[` / `]` change hue
(no focus toggle, so no collision with dialog `Tab`). *Mouse:* click/**drag** in
the box sets sat/val, on the strip sets hue. Every change → `m.hsv` then
`m.color = Rgb(hsv→rgb)`.

**4. xterm-256 grid** — a 16×16 grid of the 256 palette (cells via
`xterm256_to_rgb`, backend-quantized), cursor-marked. *Local state:* cursor index
`u8` (seeded from `m.color` if `Indexed`, else `rgb→nearest-256` on entry).
*Nav:* arrows move the cursor → `m.color = Indexed(idx)`; click selects a cell.

**Reuse note:** gauge bars and the hex field are local helpers; the `ListViewer`/
`InputLine` *Views* are deliberately not embedded (they'd pull in cross-view
broker machinery for one widget's internal state — the thing Approach A avoids).

## Section 3 — the `ColorPicker` view

```rust
struct ColorPicker {
    state: ViewState,
    model: ColorModel,
    active: Tab,                 // Presets | Rgb | Plane | Xterm256
    presets: PresetsSurface,
    rgb: RgbSurface,
    plane: PlaneSurface,
    grid: Xterm256Surface,
}
```
The reusable, embeddable widget (does **not** own OK/Cancel — dialog chrome).
Implements `View`.

**Layout** (from its bounds): a **tab bar** across the top; the **active surface**
in the body; a fixed-width **info column** on the right with the *old* swatch+hex,
the *new* swatch+hex, and the `Color`-variant readout (e.g. `Rgb(30,144,255)`,
`Bios(4) "Red"`, `Indexed(33)`, `Default`). Dialog sized to fit the widest
surface (grid/plane), ~60×16, tabbed.

**`draw`:** tab bar (active highlighted) + info column, then delegate the body to
`active`'s surface `draw`.

**`handle_event` order:**
1. **Tab switching first:** `Ctrl+←`/`Ctrl+→` cycle; `Alt+<hotkey>` jumps
   (P/R/W/6); mouse-click on a tab label switches.
2. Else delegate to `active`'s surface `handle_event`.
3. Plain `Tab`/`Shift+Tab` left **unhandled** so the enclosing dialog moves focus
   between the picker and OK/Cancel (faithful TV control nav).

Switching tabs never converts/commits — it shows the current `m.color` in that
surface's terms; only an actual edit changes `m.color`.

**Result accessor:** `ColorPicker::color(&self) -> Color` (the row-82 `color()`
precedent) — how the shell reads the selection. `as_any_mut → Some(self)` (already
the rstv broker-reachability convention; required by the drag apply arm).

## Section 4 — modal shell, return value, mouse drag

**Entry point** (mirrors `messageBox`/`inputBox`):
```rust
pub fn color_dialog(initial: Color) -> Option<Color>;
```
Builds a `Dialog` titled "Select Color" embedding a `ColorPicker` (seeded with
`initial`; "old" swatch = `initial`) + OK + Cancel, runs it on the existing modal
machinery, returns `Some(color)` on `cmOK` (reading `color()`), `None` on
Cancel/Esc. The chosen color is surfaced out via the existing
`exec_view_with_completion` **gather closure** (the inputBox scatter precedent) —
no new modal plumbing.

**Mouse drag** (SV box, hue strip, RGB gauges) — the proven `window.rs` capture
pattern:
- On `MouseDown` in a draggable region, the surface pushes a `ColorPickerDrag`
  **`CaptureHandler`** (identity = the picker's `ViewId` + which region).
- While captured, each `MouseMove` is offered to the handler *before* normal
  routing (so it keeps working when the mouse leaves the picker's bounds). The
  handler posts `Deferred::ColorPickerDrag { picker, region, pos }`; the pump's
  deferred-apply scope downcasts the view to `ColorPicker` and calls
  `apply_drag(region, pos)`.
- `MouseUp` pops the capture.

This is the **same shape as the existing scroller/editor brokers** (a `Deferred`
variant + a downcast in the pump apply loop) and the same capture lifecycle as
window move/resize — no new foundation, one new `Deferred` variant + its arm.
Single-click (no drag) is handled inline by each surface without capture.

## Section 5 — testing

Per rstv conventions (pure logic → unit tests; anything that draws → `insta`
snapshots, D11; `cargo-insta` not installed → `INSTA_UPDATE=always` then
hand-verify; 4-core cap; the three gates test/clippy(-forced)/fmt):

- **`ColorModel` (pure):** conversions (`rgb↔hsv`, BIOS→RGB table,
  `color→display-rgb`, nearest-256/BIOS), variant-as-mode (each surface yields the
  right variant), HSV-retention edges (value→0 and back keeps hue; saturation→0
  keeps hue).
- **Each surface (draw):** one snapshot at a representative color (presets list,
  RGB gauges, HSV plane, 256 grid). `HeadlessBackend` + the `indicator.rs`
  harness. Snapshots capture the *logical* `Color` per cell (e.g. `fg=RGB(...)`),
  so truecolor surfaces snapshot deterministically regardless of terminal depth.
- **Each surface (events):** nav against the model via the row-82 `with_ctx`
  throwaway-`Context` helper — arrows move cursor/value, ±1 / ±16 on RGB, hex
  parse commits, grid cursor moves, preset selection sets `m.color`.
- **`ColorPicker` view:** a snapshot per tab; event tests for tab switching
  (`Ctrl+←/→`, `Alt+hotkey`), plain `Tab` left unhandled, `color()` returns the
  selection.
- **Mouse drag:** a pump-level integration test (the row-80 `MakeButtonDefault`
  precedent) — `MouseDown` pushes capture, `MouseMove` posts
  `Deferred::ColorPickerDrag`, the pump downcasts + `apply_drag` updates the
  model, `MouseUp` pops.
- **`color_dialog`:** integration tests driving the modal to `cmOK`
  (`Some(color)`) and Cancel/Esc (`None`).

## Open items deferred to the implementation plan
- Exact `Hsv` representation (`f32` vs fixed-point integers) and rounding policy
  for `rgb↔hsv` (must keep snapshots deterministic).
- Exact dialog/sub-rect geometry (tab bar height, info-column width, surface
  body rects) sized to fit the 16×16 grid and the plane.
- Final curated preset list.
- Whether the BIOS→RGB table lives in `quantize.rs` (if reused) or `model.rs`.
- Whether to keep or drop the 3 `COLOR_*` commands from the reverted row 82.

## Future (out of scope here)
- **Theme editor** consuming this picker: build the D7 "Theme extension point"
  (runtime `Role → Style` registration/mutation), then an editor UI that uses two
  `ColorPicker`s (fg/bg) + modifiers per `Role`.
- Custom/recent presets; a `FieldValue::Color` D10 variant; an optional polar
  wheel surface.
