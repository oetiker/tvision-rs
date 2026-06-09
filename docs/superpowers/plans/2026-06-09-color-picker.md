# Truecolor Color Picker — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a reusable, embeddable `ColorPicker` view + a `color_dialog(initial) -> Option<Color>` modal entry point that selects any rstv `Color` variant (`Default`/`Bios`/`Indexed`/`Rgb`) through four tabbed surfaces (Presets · RGB+hex · HSV plane · xterm-256 grid), with full keyboard + mouse-drag control.

**Architecture:** Approach A — one `ColorPicker` `View` owns a single shared `ColorModel`; each of the four surfaces is a plain component (not a `View`) that draws + handles events against `&mut ColorModel`. Mouse drag reuses the proven capture-handler pattern (a new `Deferred::ColorPickerDrag` variant + a pump apply arm that downcasts to `ColorPicker`). The modal result is read by downcasting the in-tree modal `ColorPicker` to call `color()` before removal — a new `ModalCompletion::ColorPick` variant writing into an `Rc<Cell<Option<Color>>>` sink (the `HistoryPick`/`get_selection()` precedent). **No `FieldValue::Color` variant** (explicit spec non-goal — `color()` is the contract).

**Tech Stack:** Rust (Cargo workspace `tvision` + `tvision-macros`); `insta` snapshot tests; `HeadlessBackend`; the D3/D9 capture + deferred machinery already in the tree.

**Source spec:** [`docs/superpowers/specs/2026-06-09-color-picker-design.md`](file:///home/oetiker/checkouts/rstv/docs/superpowers/specs/2026-06-09-color-picker-design.md). This plan resolves its open items — read the **Decisions** section below before any task.

---

## Decisions (resolving the spec's open items — read first)

These pin every "TBD" the spec deferred to the plan. They are **load-bearing**; do not silently override them — two override apparent spec phrasings and the reconciliation is stated.

1. **Result extraction = downcast-`color()`, NOT `FieldValue::Color`.** The spec lists "A `FieldValue::Color` D10 variant" as an explicit **non-goal** and names "the concrete `color()` accessor is the contract." The spec's phrase "surfaced out via the existing `exec_view_with_completion` **gather closure**" is satisfied by a **completion** (the `ModalCompletion` hook), not the `gather: Option<ViewId>` path (which reads `value()` → `FieldValue` and would force a `Color` variant). Mechanism: a new `ModalCompletion::ColorPick { picker, sink }` whose apply-arm, on `cmOK`, downcasts the in-tree modal to `ColorPicker`, reads `color()`, and writes it into an `Rc<Cell<Option<Color>>>` the caller owns — exactly the `ModalCompletion::HistoryPick` → `get_selection()` shape at `program.rs:1797`. **Do not edit `data.rs`.** All three spec statements reconcile only under this path.

2. **xterm-256 grid is a true 16×16 (16 physical rows).** The spec uses half-blocks **only** for the HSV plane ("Half-blocks double vertical resolution"); for the grid it says "a **16×16 grid**" and the geometry open-item says size the dialog "to fit **the 16×16 grid**." The grid stays 16 physical rows × 16 cells (one keypress = one cell); the **dialog grows taller** to fit it (the "~60×16" in the spec is approximate; sizing-to-fit is explicitly licensed). Do **not** pack 256 colors into 8 half-block rows.

3. **`Hsv` representation = `f32` with deterministic rounding.** `Hsv { h: f32 (0.0..360.0), s: f32 (0.0..1.0), v: f32 (0.0..1.0) }`. `hsv_to_rgb` rounds each channel as `(c * 255.0 + 0.5).clamp(0.0, 255.0) as u8` (round-half-up, deterministic). Snapshots capture the **logical** `Color` (e.g. `Rgb(30,144,255)`) per cell — `u8` channels — so f32 internals are snapshot-deterministic. Round-trip instability at the edges (value 0 ⇒ hue lost; saturation 0 ⇒ hue undefined) is handled by **retaining `hsv` in the model**, not by the conversion.

4. **BIOS→RGB display table lives in `model.rs`** (NOT `quantize.rs`). It is picker-only display data (the 16-entry ANSI/xterm default palette), distinct from `quantize.rs::XTERM256_TO_RGB` (which deliberately leaves indices 0..15 = 0 and is the *quantization* ladder, not a display palette). Canonical values are in Task 2.

5. **The 3 `COLOR_*` commands are DROPPED.** `COLOR_FOREGROUND_CHANGED` / `COLOR_BACKGROUND_CHANGED` / `COLOR_SET` in `command.rs` are unused by the picker (one view, no cross-view color sync). Remove them in Task 1 with the row-82 revert.

6. **`color_dialog` is a `Program` method**, not a free function. The spec's `pub fn color_dialog(initial: Color) -> Option<Color>` needs `&mut Program` to exec a modal (like `message_box`/`input_box`, which are `Program` methods). Signature: `pub fn color_dialog(&mut self, initial: Color) -> Option<Color>`.

7. **Dialog geometry (tunable when snapshots are first generated):**
   - Dialog bounds `Rect::new(0, 0, 60, 23)`, centered on the desktop.
   - `ColorPicker` child at dialog-local `Rect::new(2, 2, 58, 20)` → **picker-local size 56 × 18**.
   - OK button `Rect::new(20, 20, 30, 22)` (`bfDefault`, `Command::OK`); Cancel `Rect::new(31, 20, 41, 22)` (`Command::CANCEL`). Bottom frame at row 22.
   - **Picker-local layout** (56 wide × 18 tall):
     - Row 0: **tab bar**.
     - Rows 1..18 (17 rows): body, split `[surface body | info column]`.
     - **Info column**: picker-local x `38..56` (18 wide), rows 1..18.
     - **Surface body**: picker-local x `0..37` (37 wide), rows 1..18 (17 rows tall).
   - The 16×16 grid (2 cols/cell = 32 wide, 16 rows) fits the 37×17 body. The plane's SV box ≈ 30 wide × 16 rows (half-blocks → 32 vertical levels) + a 2-col hue strip fits too.
   - These constants are defined **once** in `mod.rs` as `const` items (Task 7); surfaces receive their body `Rect` from the picker and must not hardcode offsets.

8. **Curated preset `Rgb` list (final, 12 entries):** Orange `(255,165,0)`, Gold `(255,215,0)`, Pink `(255,192,203)`, Coral `(255,127,80)`, Purple `(128,0,128)`, Teal `(0,128,128)`, Olive `(128,128,0)`, Navy `(0,0,128)`, Maroon `(128,0,0)`, Lime `(0,255,0)`, Aqua `(0,255,255)`, Silver `(192,192,192)`.

---

## File structure

```
src/dialog/colorpick/
  mod.rs        ColorPicker view, Tab enum, Surface trait, layout consts, color_dialog wiring, re-exports
  model.rs      ColorModel, Hsv, BIOS_RGB table, rgb<->hsv, color->display-rgb, nearest-* helpers
  presets.rs    PresetsSurface + the preset table (Default + 16 BIOS + 12 curated Rgb)
  rgb.rs        RgbSurface (3 gauge bars + hex field)
  plane.rs      PlaneSurface (hue strip + SV box, half-blocks)
  xterm256.rs   Xterm256Surface (true 16x16 grid)
  drag.rs       ColorDragCapture handler + ColorDragRegion enum
```
Plus, in existing files:
- `src/view/context.rs`: `Deferred::ColorPickerDrag { picker, pos }` (no widget type — the region lives in the picker's `active_drag`) + `Context::request_color_drag(picker, pos)`.
- `src/app/program.rs`: the `ColorPickerDrag` deferred-apply arm; `ModalCompletion::ColorPick { picker, sink }` + its `apply_modal_completion` arm; `Program::color_dialog`.
- `src/dialog/mod.rs`: `mod colorpick;` + re-exports; **remove** `mod colordlg;` + its exports.
- `src/lib.rs`: re-export `ColorPicker`, `color_dialog`-adjacent types as needed.
- `src/command.rs`: **remove** the 3 `COLOR_*` consts.
- `src/dialog/colordlg.rs` + its 3 `.snap`s: **deleted**.

---

## Task 0: Pre-flight — confirm a green baseline

**Files:** none (verification only).

- [ ] **Step 1: Confirm the build is green before touching anything**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test --workspace 2>&1 | tail -20
```
Expected: all tests pass (the handover records **924 lib tests** green). Note the exact count — Task 1 will reduce it (the row-82 tests are deleted).

---

## Task 1: Revert rows 81–82 (delete `colordlg`, drop the `COLOR_*` commands)

**Files:**
- Delete: `src/dialog/colordlg.rs`
- Delete: `src/dialog/snapshots/tvision__dialog__colordlg__tests__snapshot_background.snap`
- Delete: `src/dialog/snapshots/tvision__dialog__colordlg__tests__snapshot_foreground.snap`
- Delete: `src/dialog/snapshots/tvision__dialog__colordlg__tests__snapshot_foreground_selected.snap`
- Modify: `src/dialog/mod.rs` (remove `mod colordlg;` + the `pub use colordlg::{...}` line)
- Modify: `src/command.rs` (remove the 3 `COLOR_*` consts + their doc comments)

- [ ] **Step 1: Delete the module + its snapshots**

```bash
cd /home/oetiker/checkouts/rstv
git rm src/dialog/colordlg.rs \
  src/dialog/snapshots/tvision__dialog__colordlg__tests__snapshot_background.snap \
  src/dialog/snapshots/tvision__dialog__colordlg__tests__snapshot_foreground.snap \
  src/dialog/snapshots/tvision__dialog__colordlg__tests__snapshot_foreground_selected.snap
```

- [ ] **Step 2: Remove the `colordlg` wiring from `src/dialog/mod.rs`**

Delete this line (currently line 39):
```rust
mod colordlg;
```
And delete the `ColorSel`/`ColorSelector`/`ColorItem`/`ColorGroup`/`ColorIndex` re-export (currently line 45):
```rust
pub use colordlg::{ColorGroup, ColorIndex, ColorItem, ColorSel, ColorSelector};
```

- [ ] **Step 3: Remove the 3 `COLOR_*` commands from `src/command.rs`**

Delete the three consts (around lines 245–255) **and their doc comments**:
```rust
pub const COLOR_FOREGROUND_CHANGED: Command = Command("tv.color_foreground_changed");
pub const COLOR_BACKGROUND_CHANGED: Command = Command("tv.color_background_changed");
pub const COLOR_SET: Command = Command("tv.color_set");
```

- [ ] **Step 4: Grep for any dangling references**

Run: `cd /home/oetiker/checkouts/rstv && grep -rn "ColorSelector\|ColorItem\|ColorGroup\|ColorIndex\|ColorSel\b\|COLOR_FOREGROUND_CHANGED\|COLOR_BACKGROUND_CHANGED\|COLOR_SET\|colordlg" src/`
Expected: **no matches** (lib.rs does not re-export these; confirm). If any appear, remove them.

- [ ] **Step 5: Verify the tree builds green (minus the deleted tests)**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test --workspace 2>&1 | tail -20
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5
cargo fmt --all --check
```
Expected: all pass; lib-test count drops by the ~40 deleted row-81/82 tests.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "revert: drop faithful color rows 81-82 (superseded by the color-picker extension)

Rows 81 (ColorItem/ColorGroup/ColorIndex) and 82 (ColorSelector) edited a
TPalette that rstv deletes under D7 — dead code by construction. The truecolor
color-picker extension supersedes them (its Presets surface subsumes the
16-color BIOS grid). Removes src/dialog/colordlg.rs, its 3 snapshots, the
dialog/mod.rs exports, and the 3 unused COLOR_* commands.

See docs/superpowers/specs/2026-06-09-color-picker-design.md.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: `ColorModel` + conversions (`model.rs`) — pure logic, unit-tested

**Files:**
- Create: `src/dialog/colorpick/model.rs`
- Create: `src/dialog/colorpick/mod.rs` (minimal scaffold so the module compiles)
- Modify: `src/dialog/mod.rs` (add `mod colorpick;`)

This task is **pure data + math** — no drawing, no events. Build it TDD.

- [ ] **Step 1: Scaffold the module so it compiles**

Create `src/dialog/colorpick/mod.rs` with just:
```rust
//! Truecolor color-picker — an rstv-original extension (NOT a faithful port).
//!
//! See `docs/superpowers/specs/2026-06-09-color-picker-design.md`. One
//! [`ColorPicker`] view owns a shared [`model::ColorModel`]; four surfaces draw +
//! handle events against it. Produces any [`Color`](crate::color::Color) variant.

pub mod model;
```
Add to `src/dialog/mod.rs` (next to the other `mod` lines):
```rust
mod colorpick;
```
(Re-exports come in Task 7; for now the module just needs to compile.)

- [ ] **Step 2: Write the failing tests for the BIOS→RGB table + `color_to_display_rgb`**

Create `src/dialog/colorpick/model.rs` with the test module first:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    #[test]
    fn bios_rgb_table_has_16_canonical_entries() {
        assert_eq!(BIOS_RGB[0], (0, 0, 0)); // Black
        assert_eq!(BIOS_RGB[1], (0, 0, 170)); // Blue
        assert_eq!(BIOS_RGB[7], (170, 170, 170)); // Light Gray
        assert_eq!(BIOS_RGB[8], (85, 85, 85)); // Dark Gray
        assert_eq!(BIOS_RGB[15], (255, 255, 255)); // White
    }

    #[test]
    fn display_rgb_maps_each_variant() {
        assert_eq!(color_to_display_rgb(Color::Rgb(30, 144, 255)), Some((30, 144, 255)));
        assert_eq!(color_to_display_rgb(Color::Bios(4)), Some((170, 0, 0))); // Red
        // Indexed uses the quantize.rs xterm-256 table.
        assert_eq!(color_to_display_rgb(Color::Indexed(16)), Some((0, 0, 0)));
        assert_eq!(color_to_display_rgb(Color::Indexed(231)), Some((255, 255, 255)));
        // Default has no concrete RGB (swatch shows a "default" marker).
        assert_eq!(color_to_display_rgb(Color::Default), None);
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target && cargo test --lib colorpick::model 2>&1 | tail -15`
Expected: FAIL — `BIOS_RGB` / `color_to_display_rgb` not found.

- [ ] **Step 4: Implement the table + `color_to_display_rgb`**

Prepend to `src/dialog/colorpick/model.rs` (above the test module):
```rust
//! [`ColorModel`] — the picker's single source of truth, plus the conversions.
//!
//! Pure data + math (no drawing/events), independently unit-testable. `color` is
//! the committed selection (its *variant* is the picker's "mode"); `hsv` is the
//! retained working hue/sat/val (HSV↔RGB is not round-trip-stable at the edges,
//! so retaining it keeps hue across brightness→0 and back).

use crate::backend::quantize::xterm256_to_rgb;
use crate::color::Color;

/// The 16 BIOS colors as **display** RGB (the ANSI/xterm default 16-color
/// palette). Picker-display data only — distinct from `quantize.rs`'s
/// `XTERM256_TO_RGB`, which leaves 0..15 = 0 and is the quantization ladder.
/// Order is the BIOS index order (0 Black … 15 White).
pub const BIOS_RGB: [(u8, u8, u8); 16] = [
    (0, 0, 0),       // 0 Black
    (0, 0, 170),     // 1 Blue
    (0, 170, 0),     // 2 Green
    (0, 170, 170),   // 3 Cyan
    (170, 0, 0),     // 4 Red
    (170, 0, 170),   // 5 Magenta
    (170, 85, 0),    // 6 Brown
    (170, 170, 170), // 7 Light Gray
    (85, 85, 85),    // 8 Dark Gray
    (85, 85, 255),   // 9 Light Blue
    (85, 255, 85),   // 10 Light Green
    (85, 255, 255),  // 11 Light Cyan
    (255, 85, 85),   // 12 Light Red
    (255, 85, 255),  // 13 Light Magenta
    (255, 255, 85),  // 14 Yellow
    (255, 255, 255), // 15 White
];

/// The display RGB for a [`Color`], or `None` for [`Color::Default`] (rendered as
/// a "default" marker, not a fake RGB). `Bios(n)` → [`BIOS_RGB`] (masked to 0..15);
/// `Indexed(n)` → `quantize::xterm256_to_rgb`; `Rgb` → itself.
pub fn color_to_display_rgb(c: Color) -> Option<(u8, u8, u8)> {
    match c {
        Color::Default => None,
        Color::Bios(n) => Some(BIOS_RGB[(n & 0x0F) as usize]),
        Color::Indexed(n) => Some(xterm256_to_rgb(n)),
        Color::Rgb(r, g, b) => Some((r, g, b)),
    }
}
```
Check `xterm256_to_rgb` is re-exported from `crate::backend::quantize` (it is `pub const fn` at `quantize.rs:261`). If the `quantize` module is not `pub`, use the existing public re-export path — grep `grep -rn "pub use.*quantize\|pub mod quantize" src/backend/mod.rs` and adjust the `use`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib colorpick::model 2>&1 | tail -10`
Expected: PASS (2 tests).

- [ ] **Step 6: Write the failing `rgb↔hsv` round-trip + edge tests**

Add to the `tests` module:
```rust
    fn approx(a: f32, b: f32, eps: f32) -> bool { (a - b).abs() <= eps }

    #[test]
    fn hsv_to_rgb_primaries() {
        assert_eq!(hsv_to_rgb(Hsv { h: 0.0, s: 1.0, v: 1.0 }), (255, 0, 0)); // red
        assert_eq!(hsv_to_rgb(Hsv { h: 120.0, s: 1.0, v: 1.0 }), (0, 255, 0)); // green
        assert_eq!(hsv_to_rgb(Hsv { h: 240.0, s: 1.0, v: 1.0 }), (0, 0, 255)); // blue
        assert_eq!(hsv_to_rgb(Hsv { h: 0.0, s: 0.0, v: 1.0 }), (255, 255, 255)); // white
        assert_eq!(hsv_to_rgb(Hsv { h: 0.0, s: 0.0, v: 0.0 }), (0, 0, 0)); // black
    }

    #[test]
    fn rgb_to_hsv_primaries() {
        let h = rgb_to_hsv(255, 0, 0);
        assert!(approx(h.h, 0.0, 0.5) && approx(h.s, 1.0, 0.01) && approx(h.v, 1.0, 0.01));
        let h = rgb_to_hsv(0, 0, 255);
        assert!(approx(h.h, 240.0, 0.5));
    }

    #[test]
    fn rgb_hsv_round_trip_is_stable_for_saturated_colors() {
        for (r, g, b) in [(30u8, 144, 255), (255, 165, 0), (128, 0, 128)] {
            let (r2, g2, b2) = hsv_to_rgb(rgb_to_hsv(r, g, b));
            // round-half-up to u8 may differ by 1 at most
            assert!((r as i16 - r2 as i16).abs() <= 1, "r {r}->{r2}");
            assert!((g as i16 - g2 as i16).abs() <= 1, "g {g}->{g2}");
            assert!((b as i16 - b2 as i16).abs() <= 1, "b {b}->{b2}");
        }
    }
```

- [ ] **Step 7: Run to verify failure, then implement `Hsv` + `rgb↔hsv`**

Run: `cargo test --lib colorpick::model 2>&1 | tail -10` → FAIL (`Hsv`/`hsv_to_rgb`/`rgb_to_hsv` missing).

Add to `model.rs` (above the tests):
```rust
/// Working hue/sat/val, retained in the model across edits. `h` is degrees
/// `0.0..360.0`; `s`,`v` are `0.0..1.0`. f32 with deterministic rounding to u8
/// (see [`hsv_to_rgb`]) — snapshots capture logical `Color`, so this is
/// snapshot-deterministic.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
}

/// HSV → RGB (standard sextant formula). Each channel rounds half-up:
/// `(c * 255.0 + 0.5).clamp(0.0, 255.0) as u8`.
pub fn hsv_to_rgb(hsv: Hsv) -> (u8, u8, u8) {
    let h = hsv.h.rem_euclid(360.0);
    let s = hsv.s.clamp(0.0, 1.0);
    let v = hsv.v.clamp(0.0, 1.0);
    let c = v * s;
    let h6 = h / 60.0;
    let x = c * (1.0 - (h6.rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = match h6 as u8 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x), // 5 and the h==360 wrap (h6 in [5,6))
    };
    let m = v - c;
    let to_u8 = |f: f32| ((f + m) * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
    (to_u8(r1), to_u8(g1), to_u8(b1))
}

/// RGB → HSV (standard). Hue 0 when chroma is 0 (saturation 0).
pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> Hsv {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let c = max - min;
    let h = if c == 0.0 {
        0.0
    } else if max == rf {
        60.0 * (((gf - bf) / c).rem_euclid(6.0))
    } else if max == gf {
        60.0 * ((bf - rf) / c + 2.0)
    } else {
        60.0 * ((rf - gf) / c + 4.0)
    };
    let s = if max == 0.0 { 0.0 } else { c / max };
    Hsv { h: h.rem_euclid(360.0), s, v: max }
}
```

Run: `cargo test --lib colorpick::model 2>&1 | tail -10` → PASS.

- [ ] **Step 8: Write the failing `ColorModel` tests (variant-as-mode + HSV retention edges)**

Add to the `tests` module:
```rust
    #[test]
    fn model_set_rgb_sets_variant_and_refreshes_hsv() {
        let mut m = ColorModel::new(Color::Default);
        m.set_rgb(255, 0, 0);
        assert_eq!(m.color, Color::Rgb(255, 0, 0));
        assert!(approx(m.hsv.h, 0.0, 0.5) && approx(m.hsv.s, 1.0, 0.01));
    }

    #[test]
    fn model_set_indexed_sets_variant() {
        let mut m = ColorModel::new(Color::Default);
        m.set_indexed(33);
        assert_eq!(m.color, Color::Indexed(33));
    }

    #[test]
    fn model_set_color_preset_keeps_variant() {
        let mut m = ColorModel::new(Color::Default);
        m.set_color(Color::Bios(4));
        assert_eq!(m.color, Color::Bios(4)); // variant preserved (Presets)
    }

    #[test]
    fn hsv_retention_keeps_hue_through_black_and_back() {
        // Start saturated orange, drive value to 0, then back up: hue must survive.
        let mut m = ColorModel::new(Color::Rgb(255, 165, 0));
        let hue0 = m.hsv.h;
        m.set_hsv(Hsv { v: 0.0, ..m.hsv }); // brightness → black
        assert_eq!(m.color, Color::Rgb(0, 0, 0));
        assert!(approx(m.hsv.h, hue0, 0.5), "hue must be retained at v=0");
        m.set_hsv(Hsv { v: 1.0, ..m.hsv }); // back to full brightness
        assert!(approx(m.hsv.h, hue0, 0.5), "hue must survive the round-trip");
    }

    #[test]
    fn hsv_retention_keeps_hue_through_gray() {
        let mut m = ColorModel::new(Color::Rgb(0, 0, 255)); // blue, hue 240
        let hue0 = m.hsv.h;
        m.set_hsv(Hsv { s: 0.0, ..m.hsv }); // saturation → gray (hue undefined in RGB)
        assert!(approx(m.hsv.h, hue0, 0.5), "hue retained at s=0");
    }
```

- [ ] **Step 9: Run to verify failure, then implement `ColorModel`**

Run: `cargo test --lib colorpick::model 2>&1 | tail -10` → FAIL.

Add to `model.rs` (above the tests):
```rust
/// The picker's single source of truth (Approach A). `color` is the committed
/// selection (its variant *is* the mode); `hsv` is retained working HSV.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorModel {
    /// The committed selection — exactly what `color_dialog`/`ColorPicker::color` returns.
    pub color: Color,
    /// Retained working hue/sat/val (see [`Hsv`]).
    pub hsv: Hsv,
}

impl ColorModel {
    /// Seed from an initial color. `hsv` is computed from the color's display RGB
    /// (or `{0,0,0}` for `Default`).
    pub fn new(color: Color) -> Self {
        let (r, g, b) = color_to_display_rgb(color).unwrap_or((0, 0, 0));
        ColorModel { color, hsv: rgb_to_hsv(r, g, b) }
    }

    /// Set from a non-plane surface (presets/grid): set `color` and recompute
    /// `hsv` from its display RGB (so the plane cursor follows). `Default` leaves
    /// `hsv` unchanged (no RGB to derive).
    pub fn set_color(&mut self, c: Color) {
        self.color = c;
        if let Some((r, g, b)) = color_to_display_rgb(c) {
            self.hsv = rgb_to_hsv(r, g, b);
        }
    }

    /// Set from the RGB surface: `color = Rgb(r,g,b)`, refresh `hsv`.
    pub fn set_rgb(&mut self, r: u8, g: u8, b: u8) {
        self.color = Color::Rgb(r, g, b);
        self.hsv = rgb_to_hsv(r, g, b);
    }

    /// Set from the xterm-256 grid: `color = Indexed(idx)`, refresh `hsv` from its RGB.
    pub fn set_indexed(&mut self, idx: u8) {
        self.color = Color::Indexed(idx);
        let (r, g, b) = xterm256_to_rgb(idx);
        self.hsv = rgb_to_hsv(r, g, b);
    }

    /// Set from the HSV plane: store `hsv` (retained verbatim) and derive
    /// `color = Rgb(hsv→rgb)`. The retained `hsv` is the source of truth here —
    /// this is the one setter that does NOT recompute hsv from rgb (preserving
    /// hue across value/saturation edges).
    pub fn set_hsv(&mut self, hsv: Hsv) {
        self.hsv = hsv;
        let (r, g, b) = hsv_to_rgb(hsv);
        self.color = Color::Rgb(r, g, b);
    }
}
```

Run: `cargo test --lib colorpick::model 2>&1 | tail -10` → PASS.

- [ ] **Step 10: Verify gates + commit**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test --lib colorpick 2>&1 | tail -10
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5
cargo fmt --all --check
git add -A
git commit -m "feat(colorpick): ColorModel + rgb<->hsv + BIOS display table

The picker's shared single-source-of-truth and conversions: ColorModel
(color + retained hsv), Hsv (f32, deterministic round-half-up), the 16-entry
BIOS_RGB display palette, color_to_display_rgb, rgb<->hsv. HSV is retained so
hue survives brightness->0 and saturation->0 round-trips. Pure logic, unit-tested.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: The `Surface` trait + a throwaway-`Context` test helper

**Files:**
- Modify: `src/dialog/colorpick/mod.rs` (add the `Surface` trait + layout consts)

The four surfaces share one shape so the picker routes uniformly. This task defines the trait + the shared layout constants; the surfaces implement it in Tasks 4–7.

- [ ] **Step 1: Add the `Surface` trait + layout consts to `mod.rs`**

Append to `src/dialog/colorpick/mod.rs`:
```rust
use crate::color::Color;
use crate::event::Event;
use crate::view::{Context, DrawCtx, Point, Rect};
use model::ColorModel;

// -- shared layout (picker-local; see the plan's geometry decision) -----------
/// Picker-local tab-bar row.
pub(crate) const TAB_BAR_Y: i32 = 0;
/// Picker-local x where the info column starts (right edge of the surface body).
pub(crate) const INFO_COL_X: i32 = 38;
/// Picker-local body top (first row below the tab bar).
pub(crate) const BODY_TOP: i32 = 1;

/// A picker surface — draws + handles events against the shared [`ColorModel`].
/// Each owns only its own UI state; all read/write the shared model. `body` is the
/// surface's picker-local draw rect (left of the info column, below the tab bar),
/// passed in by the [`ColorPicker`] so a surface never hardcodes offsets.
pub(crate) trait Surface {
    /// Draw into `body` (picker-local rect) reading the shared model.
    fn draw(&self, ctx: &mut DrawCtx, body: Rect, m: &ColorModel);

    /// Handle a key/mouse event against the shared model. Mouse positions are
    /// picker-local (D3: the group delivers local positions). `body` is the
    /// surface's picker-local rect.
    fn handle_event(&mut self, ev: &mut Event, body: Rect, m: &mut ColorModel, ctx: &mut Context);

    /// For a `MouseDown` at picker-local `p`: which draggable region (if any) was
    /// hit, so the [`ColorPicker`] can push a drag capture. `None` = not draggable
    /// (single-click handled inline by `handle_event`).
    fn drag_region_at(&self, _p: Point, _body: Rect) -> Option<drag::ColorDragRegion> {
        None
    }

    /// Apply a drag update at picker-local `p` (a broker callback from the pump).
    /// Default: no-op (non-draggable surfaces). `body` is the surface's rect.
    fn apply_drag(&mut self, _region: drag::ColorDragRegion, _p: Point, _body: Rect, _m: &mut ColorModel) {}
}
```
Add `pub(crate) mod drag;` near the top `mod` declarations (the module is created in Task 8 — for now create a stub `src/dialog/colorpick/drag.rs` with just the enum so this compiles):

Create `src/dialog/colorpick/drag.rs`:
```rust
//! Mouse-drag capture for the color picker (the `window.rs DragCapture` pattern).
//! The capture handler + pump apply arm land in Task 8; this enum is needed by the
//! `Surface` trait now.

/// Which draggable region of the active surface a drag is scrubbing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorDragRegion {
    /// The HSV plane's Saturation×Value box.
    SvBox,
    /// The HSV plane's vertical hue strip.
    HueStrip,
    /// An RGB gauge bar — `0`=R, `1`=G, `2`=B.
    RgbBar(u8),
}
```

- [ ] **Step 2: Verify it compiles**

Run: `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target && cargo build --lib 2>&1 | tail -10`
Expected: builds (dead-code warnings for the unused trait/consts are fine at this stage; if clippy `-D warnings` is run later it'll be satisfied once surfaces use them — do NOT add `#[allow(dead_code)]`, the next tasks consume these).

Note: because the gates use `-D warnings`, do **not** run clippy at the end of this task in isolation — it will fail on dead code. This task has **no commit of its own**; fold it into Task 4's commit (the first surface consumes the trait). Proceed directly to Task 4.

---

## Task 4: `PresetsSurface` (`presets.rs`)

**Files:**
- Create: `src/dialog/colorpick/presets.rs`
- Modify: `src/dialog/colorpick/mod.rs` (add `pub(crate) mod presets;`)
- Test: in `presets.rs` `#[cfg(test)]`

A scrolling list of `{name, Color}`: "Default" + 16 BIOS + 12 curated `Rgb`. ↑/↓ select → `m.set_color`; click selects. Custom mini-list (not the `ListViewer` View).

- [ ] **Step 1: Write the failing event test (nav sets the model)**

Create `src/dialog/colorpick/presets.rs` with the test module first:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::dialog::colorpick::model::ColorModel;
    use crate::event::{Event, Key, KeyEvent, KeyModifiers};
    use crate::timer::TimerQueue;
    use crate::view::{Context, Deferred, Point, Rect};
    use std::collections::VecDeque;

    fn with_ctx<R>(f: impl FnOnce(&mut Context) -> R) -> R {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        f(&mut ctx)
    }

    fn key(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(k, KeyModifiers::default()))
    }
    const BODY: Rect = Rect { a: Point { x: 0, y: 1 }, b: Point { x: 37, y: 18 } };

    #[test]
    fn first_entry_is_default() {
        assert_eq!(PRESETS[0].1, Color::Default);
        assert_eq!(PRESETS[1].1, Color::Bios(0)); // Black
    }

    #[test]
    fn preset_table_has_default_16_bios_and_12_rgb() {
        assert_eq!(PRESETS.len(), 1 + 16 + 12);
    }

    #[test]
    fn down_arrow_advances_selection_and_sets_color() {
        let mut s = PresetsSurface::new(&ColorModel::new(Color::Default));
        let mut m = ColorModel::new(Color::Default);
        let mut ev = key(Key::Down);
        with_ctx(|ctx| <PresetsSurface as crate::dialog::colorpick::Surface>::handle_event(
            &mut s, &mut ev, BODY, &mut m, ctx,
        ));
        assert_eq!(m.color, Color::Bios(0)); // moved Default -> Black
        assert!(ev.is_nothing());
    }

    #[test]
    fn up_arrow_at_top_does_not_wrap() {
        let mut s = PresetsSurface::new(&ColorModel::new(Color::Default));
        let mut m = ColorModel::new(Color::Default);
        let mut ev = key(Key::Up);
        with_ctx(|ctx| <PresetsSurface as crate::dialog::colorpick::Surface>::handle_event(
            &mut s, &mut ev, BODY, &mut m, ctx,
        ));
        assert_eq!(m.color, Color::Default); // clamped at top
    }

    #[test]
    fn new_seeds_selection_from_model_color() {
        // A model already on Bios(4) seeds the cursor onto that row.
        let s = PresetsSurface::new(&ColorModel::new(Color::Bios(4)));
        assert_eq!(PRESETS[s.selected].1, Color::Bios(4));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib colorpick::presets 2>&1 | tail -15` → FAIL (types missing).

- [ ] **Step 3: Implement `PresetsSurface` + the table**

Prepend to `presets.rs`:
```rust
//! Presets surface — a scrolling `{name, Color}` list (Default + 16 BIOS + 12 Rgb).

use crate::color::{Color, Style};
use crate::dialog::colorpick::model::ColorModel;
use crate::dialog::colorpick::{drag::ColorDragRegion, Surface};
use crate::event::{ctrl_to_arrow, Event, Key};
use crate::theme::Role;
use crate::view::{Context, DrawCtx, Point, Rect};

/// The preset table: (display name, the `Color` it yields). Index 0 = Default,
/// 1..=16 = BIOS by name, 17.. = the curated Rgb set.
pub(crate) const PRESETS: &[(&str, Color)] = &[
    ("Default", Color::Default),
    ("Black", Color::Bios(0)),
    ("Blue", Color::Bios(1)),
    ("Green", Color::Bios(2)),
    ("Cyan", Color::Bios(3)),
    ("Red", Color::Bios(4)),
    ("Magenta", Color::Bios(5)),
    ("Brown", Color::Bios(6)),
    ("Light Gray", Color::Bios(7)),
    ("Dark Gray", Color::Bios(8)),
    ("Light Blue", Color::Bios(9)),
    ("Light Green", Color::Bios(10)),
    ("Light Cyan", Color::Bios(11)),
    ("Light Red", Color::Bios(12)),
    ("Light Magenta", Color::Bios(13)),
    ("Yellow", Color::Bios(14)),
    ("White", Color::Bios(15)),
    ("Orange", Color::Rgb(255, 165, 0)),
    ("Gold", Color::Rgb(255, 215, 0)),
    ("Pink", Color::Rgb(255, 192, 203)),
    ("Coral", Color::Rgb(255, 127, 80)),
    ("Purple", Color::Rgb(128, 0, 128)),
    ("Teal", Color::Rgb(0, 128, 128)),
    ("Olive", Color::Rgb(128, 128, 0)),
    ("Navy", Color::Rgb(0, 0, 128)),
    ("Maroon", Color::Rgb(128, 0, 0)),
    ("Lime", Color::Rgb(0, 255, 0)),
    ("Aqua", Color::Rgb(0, 255, 255)),
    ("Silver", Color::Rgb(192, 192, 192)),
];

/// The Presets surface. Owns only its scroll/selection state; reads/writes the
/// shared model.
pub(crate) struct PresetsSurface {
    /// Selected row in [`PRESETS`].
    pub(crate) selected: usize,
    /// First visible row (scroll offset).
    top: usize,
}

impl PresetsSurface {
    /// Seed selection from the model's current color (first matching preset, else 0).
    pub(crate) fn new(m: &ColorModel) -> Self {
        let selected = PRESETS.iter().position(|&(_, c)| c == m.color).unwrap_or(0);
        PresetsSurface { selected, top: 0 }
    }

    /// Keep `selected` visible within `rows` visible lines.
    fn scroll_into_view(&mut self, rows: usize) {
        if self.selected < self.top {
            self.top = self.selected;
        } else if rows > 0 && self.selected >= self.top + rows {
            self.top = self.selected + 1 - rows;
        }
    }
}

impl Surface for PresetsSurface {
    fn draw(&self, ctx: &mut DrawCtx, body: Rect, _m: &ColorModel) {
        let rows = (body.b.y - body.a.y) as usize;
        let normal = ctx.style(Role::ScrollerNormal);
        let selected = ctx.style(Role::ScrollerSelected);
        for i in 0..rows {
            let idx = self.top + i;
            if idx >= PRESETS.len() {
                break;
            }
            let y = body.a.y + i as i32;
            let (name, color) = PRESETS[idx];
            let row_style = if idx == self.selected { selected } else { normal };
            // Clear the row, draw a swatch cell at body.a.x..+2, then the name.
            ctx.fill(Rect::new(body.a.x, y, body.b.x, y + 1), ' ', row_style);
            let swatch = match crate::dialog::colorpick::model::color_to_display_rgb(color) {
                Some((r, g, b)) => Style::new(Color::Rgb(r, g, b), Color::Rgb(r, g, b)),
                None => row_style, // Default: no swatch fill
            };
            ctx.fill(Rect::new(body.a.x, y, body.a.x + 2, y + 1), ' ', swatch);
            ctx.put_str(body.a.x + 3, y, name, row_style);
        }
    }

    fn handle_event(&mut self, ev: &mut Event, body: Rect, m: &mut ColorModel, _ctx: &mut Context) {
        let rows = (body.b.y - body.a.y) as usize;
        match *ev {
            Event::KeyDown(ke) => {
                let ke = ctrl_to_arrow(ke);
                match ke.key {
                    Key::Up => {
                        if self.selected > 0 {
                            self.selected -= 1;
                        }
                    }
                    Key::Down => {
                        if self.selected + 1 < PRESETS.len() {
                            self.selected += 1;
                        }
                    }
                    _ => return, // not ours — leave the event for the picker/dialog
                }
                self.scroll_into_view(rows);
                m.set_color(PRESETS[self.selected].1);
                ev.clear();
            }
            Event::MouseDown(me) => {
                let y = me.position.y - body.a.y;
                if y >= 0 && me.position.x >= body.a.x && me.position.x < body.b.x {
                    let idx = self.top + y as usize;
                    if idx < PRESETS.len() {
                        self.selected = idx;
                        m.set_color(PRESETS[idx].1);
                        ev.clear();
                    }
                }
            }
            _ => {}
        }
    }
}
```
Add `pub(crate) mod presets;` to `mod.rs` (with the other surface `mod`s). Confirm `ctrl_to_arrow` is `pub` in `crate::event` (it is used by the old `colordlg.rs` at `ctrl_to_arrow(ke)`; grep `grep -rn "pub fn ctrl_to_arrow" src/event/`). Confirm `Role::ScrollerNormal`/`ScrollerSelected` exist (`grep -n "ScrollerNormal\|ScrollerSelected" src/theme.rs`).

- [ ] **Step 4: Run to verify the event tests pass**

Run: `cargo test --lib colorpick::presets 2>&1 | tail -10` → PASS.

- [ ] **Step 5: Add the snapshot test**

Add to the `tests` module:
```rust
    fn render(s: &PresetsSurface, m: &ColorModel) -> String {
        use crate::backend::{HeadlessBackend, Renderer};
        use crate::screen::Buffer;
        use crate::theme::Theme;
        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(40, 18);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = Rect::new(0, 0, 40, 18);
            let mut dc = crate::view::DrawCtx::new(buf, &theme, bounds, bounds.a);
            <PresetsSurface as crate::dialog::colorpick::Surface>::draw(s, &mut dc, BODY, m);
        });
        screen.snapshot()
    }

    #[test]
    fn snapshot_presets_at_red() {
        let m = ColorModel::new(Color::Bios(4));
        let s = PresetsSurface::new(&m);
        insta::assert_snapshot!(render(&s, &m));
    }
```

- [ ] **Step 6: Generate + hand-verify the snapshot**

Run: `INSTA_UPDATE=always cargo test --lib colorpick::presets 2>&1 | tail -10`
Then `cat src/dialog/colorpick/snapshots/*presets*.snap` — hand-verify: the "Red" row is highlighted, each row shows a 2-cell swatch in the row's color, "Default" has no swatch. Re-run `cargo test --lib colorpick::presets` → PASS.

- [ ] **Step 7: Verify gates + commit**

```bash
cargo test --lib colorpick 2>&1 | tail -10
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5
cargo fmt --all --check
git add -A
git commit -m "feat(colorpick): Surface trait + PresetsSurface

The shared Surface trait (draw/handle_event/drag hooks) + layout consts, and the
first surface: a scrolling Default+16-BIOS+12-Rgb preset list with arrow nav,
click select, per-row swatches. Snapshot + event tests.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: `RgbSurface` (`rgb.rs`)

**Files:**
- Create: `src/dialog/colorpick/rgb.rs`
- Modify: `src/dialog/colorpick/mod.rs` (add `pub(crate) mod rgb;`)

Three R/G/B gauge bars (0–255, block-proportional, numeric readout) + a `#RRGGBB` hex field + a live swatch. ↑/↓ move focus between fields (R/G/B/Hex); ←/→ adjust focused channel ±1; PgUp/PgDn ±16; typing edits hex (commit on valid 6 digits). Click a bar sets that channel by x; **drag scrubs**. Every change → `m.set_rgb(...)`. No `Tab` (reserved for dialog nav).

- [ ] **Step 1: Write the failing event tests**

Create `src/dialog/colorpick/rgb.rs` with tests first:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::dialog::colorpick::model::ColorModel;
    use crate::dialog::colorpick::Surface;
    use crate::event::{Event, Key, KeyEvent, KeyModifiers};
    use crate::timer::TimerQueue;
    use crate::view::{Context, Deferred, Point, Rect};
    use std::collections::VecDeque;

    fn with_ctx<R>(f: impl FnOnce(&mut Context) -> R) -> R {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        f(&mut ctx)
    }
    fn key(k: Key) -> Event { Event::KeyDown(KeyEvent::new(k, KeyModifiers::default())) }
    fn ch(c: char) -> Event {
        Event::KeyDown(KeyEvent::new(Key::Char(c), KeyModifiers::default()))
    }
    const BODY: Rect = Rect { a: Point { x: 0, y: 1 }, b: Point { x: 37, y: 18 } };

    #[test]
    fn right_arrow_increments_focused_channel() {
        let mut s = RgbSurface::new(); // focus starts on R
        let mut m = ColorModel::new(Color::Rgb(10, 20, 30));
        let mut ev = key(Key::Right);
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        assert_eq!(m.color, Color::Rgb(11, 20, 30));
    }

    #[test]
    fn right_arrow_saturates_at_255() {
        let mut s = RgbSurface::new();
        let mut m = ColorModel::new(Color::Rgb(255, 0, 0));
        let mut ev = key(Key::Right);
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        assert_eq!(m.color, Color::Rgb(255, 0, 0));
    }

    #[test]
    fn pgup_steps_focused_channel_by_16() {
        let mut s = RgbSurface::new();
        let mut m = ColorModel::new(Color::Rgb(0, 0, 0));
        let mut ev = key(Key::PageUp);
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        assert_eq!(m.color, Color::Rgb(16, 0, 0));
    }

    #[test]
    fn down_arrow_moves_focus_to_green() {
        let mut s = RgbSurface::new();
        let mut m = ColorModel::new(Color::Rgb(0, 0, 0));
        let mut ev = key(Key::Down);
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        let mut ev = key(Key::Right); // now adjusts G
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        assert_eq!(m.color, Color::Rgb(0, 1, 0));
    }

    #[test]
    fn typing_six_hex_digits_commits() {
        let mut s = RgbSurface::new();
        // move focus to hex field (R,G,B,Hex => 3 downs)
        let mut m = ColorModel::new(Color::Rgb(0, 0, 0));
        for _ in 0..3 {
            let mut ev = key(Key::Down);
            with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        }
        for c in "1E90FF".chars() {
            let mut ev = ch(c);
            with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        }
        assert_eq!(m.color, Color::Rgb(0x1E, 0x90, 0xFF));
    }
}
```

- [ ] **Step 2: Run to verify failure, then implement `RgbSurface`**

Run: `cargo test --lib colorpick::rgb 2>&1 | tail -15` → FAIL.

Prepend the implementation. Design notes the implementer must follow:
- Fields: `focus: u8` (0=R,1=G,2=B,3=Hex), `hex_buf: String` (accumulates typed hex digits; cleared on commit or on focus change).
- Read current channels from `m.color` via `color_to_display_rgb(m.color).unwrap_or((0,0,0))` at the top of each adjust (so it works even if `m.color` is `Bios`/`Indexed` on entry — the first edit converts it to `Rgb`).
- `Right`/`Left`: `chan = chan.saturating_add/sub(1)` on the focused channel (R/G/B; on Hex, ←/→ do nothing). `PageUp`/`PageDown`: ±16 saturating.
- `Up`/`Down`: move `focus` in `0..=3` (clamp, no wrap — matches Presets' non-wrapping nav and leaves `Tab` for dialog).
- Char digit on Hex focus: push the hex digit (0-9a-fA-F) into `hex_buf`; when `hex_buf.len() == 6`, parse `#RRGGBB`, `m.set_rgb(...)`, clear `hex_buf`. Non-hex chars: ignore (do not clear the event — let it pass).
- Any channel adjust: `m.set_rgb(r, g, b)` with the updated triple; `ev.clear()`.
- `drag_region_at`: if the `MouseDown` y is on one of the three bar rows (compute bar rows from `body`), return `Some(ColorDragRegion::RgbBar(channel))`. `apply_drag`: map `p.x` within the bar's value span (0..255) and `m.set_rgb`.
- A bar is drawn as `█`-proportional `(chan as i32 * bar_width / 255)` filled cells; label `"R 011"` etc; hex field shows `#RRGGBB`; a live swatch cell uses `Style::new(Color::Rgb(r,g,b), Color::Rgb(r,g,b))`.

Provide a complete `handle_event` covering the cases above; the `draw` method is mechanical (three labelled proportional bars + a hex line + a swatch) — implement it fully (no placeholder) following the layout, but its exact glyphs are pinned by the snapshot in Step 4.

Add `pub(crate) mod rgb;` to `mod.rs`.

- [ ] **Step 3: Run the event tests**

Run: `cargo test --lib colorpick::rgb 2>&1 | tail -10` → PASS.

- [ ] **Step 4: Add + generate the snapshot**

Add a `render`/`snapshot_rgb_at_dodger_blue` test (model `Color::Rgb(30,144,255)`, focus on R), mirroring Task 4 Step 5 (40×18 backend, `BODY` rect). Generate with `INSTA_UPDATE=always`, hand-verify (three bars proportional to 30/144/255, hex shows `#1E90FF`, swatch in dodger blue), re-run to PASS.

- [ ] **Step 5: Verify gates + commit**

```bash
cargo test --lib colorpick 2>&1 | tail -10
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5
cargo fmt --all --check
git add -A
git commit -m "feat(colorpick): RgbSurface (R/G/B gauges + hex field)

Three proportional gauge bars + a #RRGGBB hex field + live swatch. Up/Down move
field focus, Left/Right adjust +/-1, PgUp/PgDn +/-16, typed hex commits on 6
digits, click+drag scrubs a bar. Every edit -> m.set_rgb. No Tab (dialog nav).

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: `PlaneSurface` (`plane.rs`)

**Files:**
- Create: `src/dialog/colorpick/plane.rs`
- Modify: `src/dialog/colorpick/mod.rs` (add `pub(crate) mod plane;`)

A vertical hue spectrum strip + a Saturation×Value box rendered in the current hue. Half-blocks (`▀` U+2580, fg=top cell color, bg=bottom cell color) double vertical resolution. Cursor marks `(sat,val)` in the box; a marker on the strip marks hue. **No local state** (the cursor derives from `m.hsv`). Arrows move sat(x)/val(y); `[`/`]` change hue. Click/drag in the box sets sat/val; on the strip sets hue. Every change → `m.set_hsv`.

- [ ] **Step 1: Write the failing event tests**

Create `src/dialog/colorpick/plane.rs` with tests first:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::dialog::colorpick::model::{ColorModel, Hsv};
    use crate::dialog::colorpick::Surface;
    use crate::event::{Event, Key, KeyEvent, KeyModifiers};
    use crate::timer::TimerQueue;
    use crate::view::{Context, Deferred, Point, Rect};
    use std::collections::VecDeque;

    fn with_ctx<R>(f: impl FnOnce(&mut Context) -> R) -> R {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        f(&mut ctx)
    }
    fn key(k: Key) -> Event { Event::KeyDown(KeyEvent::new(k, KeyModifiers::default())) }
    fn ch(c: char) -> Event { Event::KeyDown(KeyEvent::new(Key::Char(c), KeyModifiers::default())) }
    const BODY: Rect = Rect { a: Point { x: 0, y: 1 }, b: Point { x: 37, y: 18 } };

    #[test]
    fn right_arrow_increases_saturation() {
        let mut s = PlaneSurface::new();
        let mut m = ColorModel::new(Color::Rgb(128, 128, 128)); // mid, low sat
        let s0 = m.hsv.s;
        let mut ev = key(Key::Right);
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        assert!(m.hsv.s > s0, "saturation should rise");
        assert!(matches!(m.color, Color::Rgb(..)));
    }

    #[test]
    fn bracket_changes_hue() {
        let mut s = PlaneSurface::new();
        let mut m = ColorModel::new(Color::Rgb(255, 0, 0)); // hue 0
        let mut ev = ch(']');
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        assert!(m.hsv.h > 0.0, "] should advance hue");
    }

    #[test]
    fn down_arrow_decreases_value_without_scrambling_hue() {
        let mut s = PlaneSurface::new();
        let mut m = ColorModel::new(Color::Rgb(255, 165, 0)); // orange
        let h0 = m.hsv.h;
        for _ in 0..40 {
            let mut ev = key(Key::Down);
            with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        }
        assert!((m.hsv.h - h0).abs() <= 0.5, "hue retained as value drops to 0");
    }
}
```

- [ ] **Step 2: Run to verify failure, then implement `PlaneSurface`**

Run: `cargo test --lib colorpick::plane 2>&1 | tail -15` → FAIL.

Implementation notes the implementer must follow:
- **No fields** other than a unit struct (cursor derives from `m.hsv`). `PlaneSurface::new() -> Self`.
- Layout from `body`: hue strip = leftmost 2 cols (`body.a.x .. body.a.x+2`); SV box = `body.a.x+3 .. body.b.x`, rows `body.a.y .. body.b.y`. Box width `bw = body.b.x - (body.a.x+3)`, height in half-block levels `bh = (body.b.y - body.a.y) * 2`.
- `Right`/`Left`: `s.hsv.s` ±`(1.0/bw)` clamped 0..1. `Down`/`Up`: `val` ∓`(1.0/bh)` clamped 0..1 (Down lowers value). `]`/`[`: hue ±(360/ a sensible step, e.g. ± `360.0 / (body.b.y-body.a.y) as f32` per press, or a fixed `±6.0`); use a fixed `±6.0` degrees. After any: `m.set_hsv(new)`, `ev.clear()`.
- `draw`: for each box cell `(cx, cy)`, the two half-block sub-rows map to two `val` levels; `sat = (cx - boxX) / bw`, `val_top/val_bottom` from the row; build each color via `hsv_to_rgb(Hsv{ h: m.hsv.h, s, v })`; draw `▀` with `fg = top color`, `bg = bottom color`. The hue strip cells map `body` rows to hue `0..360` and draw `█` in `hsv_to_rgb(Hsv{h, s:1, v:1})`. Draw the SV cursor (a contrasting glyph, e.g. `+`) at the cell nearest `(m.hsv.s, m.hsv.v)`, and a `◄`/marker on the strip at the row nearest `m.hsv.h`.
- `drag_region_at`: `SvBox` if `p` in the box rect; `HueStrip` if in the strip rect. `apply_drag`: same mapping as click — map `p` → sat/val (SvBox) or hue (HueStrip), `m.set_hsv`.

Add `pub(crate) mod plane;` to `mod.rs`.

- [ ] **Step 3: Run the event tests** → `cargo test --lib colorpick::plane 2>&1 | tail -10` → PASS.

- [ ] **Step 4: Add + generate the snapshot** (model `Color::Rgb(255,165,0)`, 40×18 backend, `BODY`). Generate `INSTA_UPDATE=always`, hand-verify (a hue strip, an SV gradient in orange's hue, cursor near the top-right), re-run → PASS.

- [ ] **Step 5: Verify gates + commit**

```bash
cargo test --lib colorpick 2>&1 | tail -10
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5
cargo fmt --all --check
git add -A
git commit -m "feat(colorpick): PlaneSurface (hue strip + half-block SV box)

A vertical hue strip + a Saturation x Value box in the current hue, half-blocks
doubling vertical resolution. Cursor derives from m.hsv (no local state). Arrows
move sat/val, [ ] change hue, click+drag scrubs. Every edit -> m.set_hsv,
retaining hue across value->0.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: `Xterm256Surface` (`xterm256.rs`)

**Files:**
- Create: `src/dialog/colorpick/xterm256.rs`
- Modify: `src/dialog/colorpick/mod.rs` (add `pub(crate) mod xterm256;`)

A **true 16×16** grid of the 256 palette (cells via `xterm256_to_rgb`), cursor-marked. Local state: cursor index `u8` (seeded from `m.color` if `Indexed`, else `rgb_to_xterm256(rgb)` on entry). Arrows move the cursor → `m.set_indexed`; click selects a cell.

- [ ] **Step 1: Write the failing event tests**

Create `src/dialog/colorpick/xterm256.rs` with tests first:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::dialog::colorpick::model::ColorModel;
    use crate::dialog::colorpick::Surface;
    use crate::event::{Event, Key, KeyEvent, KeyModifiers};
    use crate::timer::TimerQueue;
    use crate::view::{Context, Deferred, Point, Rect};
    use std::collections::VecDeque;

    fn with_ctx<R>(f: impl FnOnce(&mut Context) -> R) -> R {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        f(&mut ctx)
    }
    fn key(k: Key) -> Event { Event::KeyDown(KeyEvent::new(k, KeyModifiers::default())) }
    const BODY: Rect = Rect { a: Point { x: 0, y: 1 }, b: Point { x: 37, y: 18 } };

    #[test]
    fn new_seeds_cursor_from_indexed() {
        let s = Xterm256Surface::new(&ColorModel::new(Color::Indexed(33)));
        assert_eq!(s.cursor, 33);
    }

    #[test]
    fn right_moves_cursor_and_sets_indexed() {
        let mut s = Xterm256Surface::new(&ColorModel::new(Color::Indexed(0)));
        let mut m = ColorModel::new(Color::Indexed(0));
        let mut ev = key(Key::Right);
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        assert_eq!(s.cursor, 1);
        assert_eq!(m.color, Color::Indexed(1));
    }

    #[test]
    fn down_moves_cursor_one_row() {
        let mut s = Xterm256Surface::new(&ColorModel::new(Color::Indexed(0)));
        let mut m = ColorModel::new(Color::Indexed(0));
        let mut ev = key(Key::Down);
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        assert_eq!(s.cursor, 16); // one row down = +16
        assert_eq!(m.color, Color::Indexed(16));
    }

    #[test]
    fn right_at_255_clamps() {
        let mut s = Xterm256Surface::new(&ColorModel::new(Color::Indexed(255)));
        let mut m = ColorModel::new(Color::Indexed(255));
        let mut ev = key(Key::Right);
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        assert_eq!(s.cursor, 255);
    }
}
```

- [ ] **Step 2: Run to verify failure, then implement `Xterm256Surface`**

Run: `cargo test --lib colorpick::xterm256 2>&1 | tail -15` → FAIL.

Implementation notes:
- Field: `pub(crate) cursor: u8`.
- `new(m)`: if `m.color` is `Indexed(n)` → `cursor = n`; else seed from `rgb_to_xterm256(rgb)` where `rgb = color_to_display_rgb(m.color).unwrap_or((0,0,0))`.
- Grid: `col = cursor % 16`, `row = cursor / 16`. Arrows move row/col with **clamp** (no wrap): Left `col>0`, Right `col<15`, Up `row>0`, Down `row<15`. Recompute `cursor = row*16 + col`, `m.set_indexed(cursor)`, `ev.clear()`. Non-arrow → return without clearing.
- `MouseDown`: map `(p.x - body.a.x)/2` → col, `(p.y - body.a.y)` → row (cells 2 cols wide, 1 row tall); if in 0..16 each, set cursor + `m.set_indexed`, clear.
- `draw`: 16 rows × 16 cells; each cell 2 cols of `█` in `Style::new(Color::Rgb(xterm256_to_rgb(idx)), …)`; cursor cell overlaid with a contrasting marker (e.g. `◘` middle cell, like the old selector). Grid origin at `body.a`.

Add `pub(crate) mod xterm256;` to `mod.rs`. Confirm `rgb_to_xterm256` import path (`crate::backend::quantize::rgb_to_xterm256`, `pub fn` at `quantize.rs:160`).

- [ ] **Step 3: Run the event tests** → PASS.

- [ ] **Step 4: Add + generate the snapshot** (model `Color::Indexed(33)`, body must fit 32 cols × 16 rows → use a **34×18 backend** here so the full grid is captured; `BODY = Rect::new(0,1,34,18)` for this test). Generate, hand-verify (16×16 color grid, cursor on cell 33), re-run → PASS.

- [ ] **Step 5: Verify gates + commit**

```bash
cargo test --lib colorpick 2>&1 | tail -10
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5
cargo fmt --all --check
git add -A
git commit -m "feat(colorpick): Xterm256Surface (true 16x16 grid)

A 16x16 grid of the xterm-256 palette (2 cols/cell), cursor-marked. Arrows move
the cursor (clamped), click selects; every move -> m.set_indexed. Cursor seeds
from Indexed(n) or rgb->nearest-256 on entry.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: The `ColorPicker` view (assemble surfaces, tabs, info column, `color()`)

**Files:**
- Modify: `src/dialog/colorpick/mod.rs` (add the `ColorPicker` struct + `View` impl + `Tab` enum)
- Test: in `mod.rs` `#[cfg(test)]`

The reusable, embeddable widget. Owns `model` + the four surface components + `active: Tab`. Implements `View`. Draws the tab bar + info column + delegates the body to the active surface. `handle_event`: tab switching first (`Ctrl+←/→`, `Alt+hotkey`, tab-label click), else delegate to the active surface, leaving plain `Tab`/`Shift+Tab` unhandled. `color() -> Color`; `as_any_mut → Some(self)`. Caches the body's absolute origin each draw (for Task 9 drag).

- [ ] **Step 1: Write the failing tests (tabs + color accessor + plain-Tab-passthrough)**

Add a `#[cfg(test)] mod view_tests` to `mod.rs`:
```rust
#[cfg(test)]
mod view_tests {
    use super::*;
    use crate::color::Color;
    use crate::event::{Event, Key, KeyEvent, KeyModifiers};
    use crate::timer::TimerQueue;
    use crate::view::{Context, Deferred, Rect, View};
    use std::collections::VecDeque;

    fn with_ctx<R>(f: impl FnOnce(&mut Context) -> R) -> R {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        f(&mut ctx)
    }
    fn ctrl(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(k, KeyModifiers { ctrl: true, ..Default::default() }))
    }
    fn plain(k: Key) -> Event { Event::KeyDown(KeyEvent::new(k, KeyModifiers::default())) }

    #[test]
    fn color_returns_seed() {
        let p = ColorPicker::new(Rect::new(0, 0, 56, 18), Color::Rgb(30, 144, 255));
        assert_eq!(p.color(), Color::Rgb(30, 144, 255));
    }

    #[test]
    fn ctrl_right_cycles_tab_forward() {
        let mut p = ColorPicker::new(Rect::new(0, 0, 56, 18), Color::Default);
        assert_eq!(p.active, Tab::Presets);
        let mut ev = ctrl(Key::Right);
        with_ctx(|ctx| p.handle_event(&mut ev, ctx));
        assert_eq!(p.active, Tab::Rgb);
        assert!(ev.is_nothing());
    }

    #[test]
    fn ctrl_left_cycles_tab_backward_with_wrap() {
        let mut p = ColorPicker::new(Rect::new(0, 0, 56, 18), Color::Default);
        let mut ev = ctrl(Key::Left);
        with_ctx(|ctx| p.handle_event(&mut ev, ctx));
        assert_eq!(p.active, Tab::Xterm256); // wrapped
    }

    #[test]
    fn plain_tab_is_left_unhandled() {
        let mut p = ColorPicker::new(Rect::new(0, 0, 56, 18), Color::Default);
        let mut ev = plain(Key::Tab);
        with_ctx(|ctx| p.handle_event(&mut ev, ctx));
        assert!(!ev.is_nothing(), "plain Tab must pass to the dialog for focus nav");
    }

    #[test]
    fn switching_tab_does_not_change_color() {
        let mut p = ColorPicker::new(Rect::new(0, 0, 56, 18), Color::Rgb(10, 20, 30));
        let mut ev = ctrl(Key::Right);
        with_ctx(|ctx| p.handle_event(&mut ev, ctx));
        assert_eq!(p.color(), Color::Rgb(10, 20, 30));
    }
}
```

- [ ] **Step 2: Run to verify failure, then implement `ColorPicker` + `Tab`**

Run: `cargo test --lib colorpick::view_tests 2>&1 | tail -15` → FAIL.

Append to `mod.rs` (above the test mods):
```rust
use crate::view::{View, ViewState};

/// The active surface tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Presets,
    Rgb,
    Plane,
    Xterm256,
}

impl Tab {
    /// Tab order for `Ctrl+←/→` cycling.
    const ORDER: [Tab; 4] = [Tab::Presets, Tab::Rgb, Tab::Plane, Tab::Xterm256];
    /// `(label, Alt-hotkey char)` for the tab bar.
    fn label(self) -> &'static str {
        match self {
            Tab::Presets => "~P~resets",
            Tab::Rgb => "~R~GB",
            Tab::Plane => "Plane (~W~)",
            Tab::Xterm256 => "~6~",
        }
    }
    fn idx(self) -> usize {
        Self::ORDER.iter().position(|&t| t == self).unwrap()
    }
    fn cycle(self, forward: bool) -> Tab {
        let i = self.idx();
        let n = Self::ORDER.len();
        Self::ORDER[if forward { (i + 1) % n } else { (i + n - 1) % n }]
    }
}

/// The reusable, embeddable truecolor color-picker view (Approach A). Owns the
/// shared [`ColorModel`] + the four surfaces; does NOT own OK/Cancel (dialog chrome).
pub struct ColorPicker {
    state: ViewState,
    model: ColorModel,
    active: Tab,
    presets: presets::PresetsSurface,
    rgb: rgb::RgbSurface,
    plane: plane::PlaneSurface,
    grid: xterm256::Xterm256Surface,
    /// **Picker-local** origin (= `ctx.origin()`), cached each `draw` so the Task-9
    /// drag handler can convert an absolute `MouseMove` back to picker-local.
    /// ONE frame: keyboard, single-click, and drag all use picker-local positions;
    /// surfaces subtract `body.a` exactly once.
    body_origin: Point,
    /// The region the in-flight drag is scrubbing, set when the capture is pushed
    /// (Task 9). Keeps widget-specific drag state in the widget, so
    /// `Deferred::ColorPickerDrag` carries only `{picker, pos}` — no widget type
    /// leaks into the FOUNDATION `Deferred` enum.
    active_drag: Option<drag::ColorDragRegion>,
}

impl ColorPicker {
    /// Build the picker, seeded with `initial` (the "old" color the dialog shows).
    pub fn new(bounds: Rect, initial: Color) -> Self {
        use crate::view::Options;
        let mut state = ViewState::new(bounds);
        state.options = Options {
            selectable: true,
            first_click: true,
            ..Default::default()
        };
        let model = ColorModel::new(initial);
        ColorPicker {
            presets: presets::PresetsSurface::new(&model),
            rgb: rgb::RgbSurface::new(),
            plane: plane::PlaneSurface::new(),
            grid: xterm256::Xterm256Surface::new(&model),
            model,
            active: Tab::Presets,
            state,
            body_origin: Point::new(0, 0),
            active_drag: None,
        }
    }

    /// The current selection — the contract `color_dialog` reads (NOT a `FieldValue`).
    pub fn color(&self) -> Color {
        self.model.color
    }

    /// Picker-local surface body rect (left of the info column, below the tab bar).
    fn body_rect(&self) -> Rect {
        let sz = self.state.size;
        Rect::new(0, BODY_TOP, INFO_COL_X, sz.y)
    }

    fn active_surface(&self) -> &dyn Surface {
        match self.active {
            Tab::Presets => &self.presets,
            Tab::Rgb => &self.rgb,
            Tab::Plane => &self.plane,
            Tab::Xterm256 => &self.grid,
        }
    }
    fn active_surface_mut(&mut self) -> &mut dyn Surface {
        match self.active {
            Tab::Presets => &mut self.presets,
            Tab::Rgb => &mut self.rgb,
            Tab::Plane => &mut self.plane,
            Tab::Xterm256 => &mut self.grid,
        }
    }

    /// Apply a drag broker callback from the pump (Task 9 wiring). `pos` is
    /// **picker-local** (the handler converted from absolute via `body_origin`);
    /// the active surface subtracts `body.a` itself. The region comes from
    /// `active_drag` (set when the capture was pushed) — so the `Deferred` variant
    /// need not carry a widget type.
    pub(crate) fn apply_drag(&mut self, pos: Point) {
        let body = self.body_rect();
        if let Some(region) = self.active_drag {
            self.active_surface_mut().apply_drag(region, pos, body, &mut self.model);
        }
    }
}

impl View for ColorPicker {
    fn state(&self) -> &ViewState { &self.state }
    fn state_mut(&mut self) -> &mut ViewState { &mut self.state }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        // cache the picker-local origin (= absolute origin of picker-local 0,0) for
        // the drag handler. ONE frame everywhere — no +BODY_TOP here.
        self.body_origin = ctx.origin();
        // tab bar
        let sz = self.state.size;
        let bar = ctx.style(crate::theme::Role::FramePassive);
        ctx.fill(Rect::new(0, TAB_BAR_Y, sz.x, TAB_BAR_Y + 1), ' ', bar);
        let mut x = 1;
        for t in Tab::ORDER {
            let style = if t == self.active {
                ctx.style(crate::theme::Role::ButtonSelected)
            } else {
                ctx.style(crate::theme::Role::ButtonNormal)
            };
            let w = ctx.put_str(x, TAB_BAR_Y, t.label(), style);
            x += w + 1;
        }
        // body
        let body = self.body_rect();
        self.active_surface().draw(ctx, body, &self.model);
        // info column (old/new swatch + variant readout) — drawn by Task 9; for
        // now draw the *new* swatch + variant text.
        self.draw_info_column(ctx);
    }

    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        // 1. tab switching first
        if let Event::KeyDown(ke) = *ev {
            if ke.modifiers.ctrl && matches!(ke.key, Key::Left | Key::Right) {
                self.active = self.active.cycle(matches!(ke.key, Key::Right));
                ev.clear();
                return;
            }
            if ke.modifiers.alt {
                if let Key::Char(c) = ke.key {
                    let up = c.to_ascii_uppercase();
                    for t in Tab::ORDER {
                        if crate::event::hot_key(t.label()) == Some(up) {
                            self.active = t;
                            ev.clear();
                            return;
                        }
                    }
                }
            }
            // 3. leave plain Tab/Shift+Tab for the dialog
            if ke.key == Key::Tab {
                return;
            }
        }
        // tab-label click switches (Task 9 may refine hit rects); compute here.
        if let Event::MouseDown(me) = *ev {
            if me.position.y == TAB_BAR_Y {
                // crude: pick the tab whose label span contains x (recomputed)
                let mut x = 1;
                for t in Tab::ORDER {
                    let w = t.label().chars().filter(|&c| c != '~').count() as i32;
                    if me.position.x >= x && me.position.x < x + w {
                        self.active = t;
                        ev.clear();
                        return;
                    }
                    x += w + 1;
                }
            }
        }
        // 2. else delegate to the active surface
        let body = self.body_rect();
        // drag: push capture if the surface reports a draggable region at a MouseDown.
        // (Filled in Task 9 — leave this block out in Task 8; surfaces still handle
        // single clicks inline via handle_event below.)
        // TODO(Task 9): push drag capture — see Task 9 Step 3.
        self.active_surface_mut().handle_event(ev, body, &mut self.model, ctx);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
}
```
Also add a `draw_info_column` inherent method (full impl — the new swatch in `color_to_display_rgb` or a "DEFAULT" marker, the variant readout string `Rgb(r,g,b)` / `Bios(n) "Name"` / `Indexed(n)` / `Default`). The **old** swatch is added in Task 9 (the picker needs the seed; store `initial` in a field `old: Color` set in `new`).

The `ColorDragCapture::new(...)` referenced here is implemented in Task 9 — to keep Task 8 building, **either** sequence Task 9's `drag.rs` capture handler before this `handle_event` drag block, **or** stub the drag block out and add it in Task 9. **Recommended:** move the drag-push block (the `if let Event::MouseDown … drag_region_at …` branch) into Task 9 and land Task 8 without it (surfaces still handle single clicks inline). Mark it with `// TODO(Task 9): push drag capture` so Task 9 fills it. This keeps each task independently green.

- [ ] **Step 3: Run the view tests** → PASS (with the drag block deferred to Task 9).

Confirm `ctx.origin()` exists on `DrawCtx` (it does, `context.rs:428`), `Role::FramePassive`/`ButtonSelected`/`ButtonNormal` exist (`grep -n "FramePassive\|ButtonSelected\|ButtonNormal" src/theme.rs`), and `ViewState::id()`/`size` fields are accessible.

- [ ] **Step 4: Add a per-tab snapshot test**

Add a `render_picker(active: Tab, initial: Color) -> String` helper (60-wide × 18 backend; build a `ColorPicker`, set `.active`, draw at `Rect::new(0,0,56,18)`), and one snapshot per tab (`snapshot_picker_presets`, `_rgb`, `_plane`, `_xterm256`). Generate with `INSTA_UPDATE=always`, hand-verify each (tab bar with the active tab highlighted, the right surface in the body, the info column on the right), re-run → PASS.

- [ ] **Step 5: Verify gates + commit**

```bash
cargo test --lib colorpick 2>&1 | tail -10
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5
cargo fmt --all --check
git add -A
git commit -m "feat(colorpick): ColorPicker view — tabs, info column, color()

Assembles the four surfaces under a tab bar + info column. Ctrl+Left/Right cycle
tabs, Alt+hotkey jumps, tab-label click switches; plain Tab passes to the dialog
for focus nav. Switching never converts/commits. color() is the result contract;
as_any_mut -> Some(self) for the drag broker. Per-tab snapshots.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 9: Mouse drag — `Deferred::ColorPickerDrag` + capture handler + pump arm

**Files:**
- Modify: `src/view/context.rs` (add `Deferred::ColorPickerDrag` + `Context::request_color_drag`)
- Modify: `src/dialog/colorpick/drag.rs` (add `ColorDragCapture` capture handler)
- Modify: `src/dialog/colorpick/mod.rs` (fill the Task-8 drag-push TODO in `handle_event`)
- Modify: `src/app/program.rs` (add the `ColorPickerDrag` deferred-apply arm)
- Test: a pump-level integration test in `program.rs`

The proven `window.rs DragCapture` pattern: MouseDown in a draggable region pushes a `ColorDragCapture`; each MouseMove posts `Deferred::ColorPickerDrag`; the pump downcasts to `ColorPicker` + `apply_drag`; MouseUp pops.

- [ ] **Step 1: Add the `Deferred` variant + the `Context` helper**

In `src/view/context.rs`, add to the `Deferred` enum (near `MakeButtonDefault`, with a doc comment matching the house style). **Carries only `{picker, pos}`** — NO widget type: the region lives in the picker's `active_drag` (set when the capture is pushed), so the FOUNDATION `Deferred` enum stays free of widget-layer types, exactly like `MakeButtonDefault`/the scroller brokers (ViewId + primitives in; the pump downcasts; widget state stays in the widget):
```rust
    /// **Color-picker drag broker** (the picker is one view, so a leaf surface
    /// can't reach the picker's `apply_drag` inline — D3). The drag capture handler
    /// posts this on each `MouseMove`/`MouseUp`; the pump resolves `picker`,
    /// downcasts to [`ColorPicker`](crate::dialog::ColorPicker) via `as_any_mut`,
    /// and calls `apply_drag(pos)` (which reads the picker's own `active_drag`
    /// region). `pos` is **picker-local** (the handler converted from absolute via
    /// the picker's cached `body_origin`). Same family (view tree) as the scroller
    /// brokers.
    ColorPickerDrag {
        /// The picker whose active surface to scrub.
        picker: ViewId,
        /// Picker-local pointer position.
        pos: Point,
    },
```
And a `Context` method (near `make_button_default`):
```rust
    /// Request a color-picker drag update — **deferred**
    /// ([`Deferred::ColorPickerDrag`]). Posted by the picker's drag capture handler.
    pub fn request_color_drag(&mut self, picker: ViewId, pos: Point) {
        self.deferred.push(Deferred::ColorPickerDrag { picker, pos });
    }
```
This adds **no** `view::context` → `dialog::colorpick` dependency (the prior draft did; storing the region in the widget removes the inversion). The `Point` type is already in scope in `context.rs`.

- [ ] **Step 2: Implement the `ColorDragCapture` handler in `drag.rs`**

Append to `src/dialog/colorpick/drag.rs`:
```rust
use crate::capture::{CaptureFlow, CaptureHandler};
use crate::event::Event;
use crate::view::{Context, Point, ViewId};

/// The D9 drag capture for the color picker (the `window.rs DragCapture` analogue).
/// Holds the picker's id + the picker's **picker-local** origin (`body_origin`,
/// cached from the picker's last `draw` = the absolute screen pos of picker-local
/// (0,0)), so each absolute `MouseMove` converts to picker-local before posting the
/// broker request. The region being scrubbed lives in the picker (`active_drag`),
/// not here — so neither this handler nor the `Deferred` variant carries a widget
/// type.
pub(crate) struct ColorDragCapture {
    picker: ViewId,
    /// Absolute screen pos of picker-local (0,0) — the picker's `body_origin`.
    origin: Point,
}

impl ColorDragCapture {
    pub(crate) fn new(picker: ViewId, origin: Point) -> Self {
        ColorDragCapture { picker, origin }
    }
}

impl CaptureHandler for ColorDragCapture {
    fn handle(&mut self, ev: &mut Event, ctx: &mut Context) -> CaptureFlow {
        match ev {
            Event::MouseMove(m) => {
                let local = m.position - self.origin; // abs -> picker-local
                ctx.request_color_drag(self.picker, local);
                CaptureFlow::Consumed
            }
            Event::MouseUp(m) => {
                let local = m.position - self.origin;
                ctx.request_color_drag(self.picker, local);
                CaptureFlow::ConsumedPop
            }
            _ => CaptureFlow::Pass,
        }
    }
    fn view(&self) -> Option<ViewId> {
        Some(self.picker)
    }
}
```
**Coordinate contract — ONE frame everywhere (picker-local). Assert it in Step 5.**
Every position the surfaces see is **picker-local**; each surface subtracts `body.a`
**exactly once** (it already does, e.g. presets' `y = me.position.y - body.a.y`).
The three entry points all feed picker-local positions:
- **Keyboard** — no position; n/a.
- **Single-click** (picker `handle_event`, Step 3): pass `me.position` **unmodified**
  to `apply_drag` (it is already picker-local — D3 delivers group-local == picker-local).
- **Drag** (this handler): `local = m.position - origin` where `origin = body_origin =
  ctx.origin()` (picker-local 0,0). Post that picker-local `local`; the pump's
  `apply_drag(pos)` passes it straight to the surface, which subtracts `body.a`.
Do **not** pre-subtract `BODY_TOP` anywhere (the prior draft did, in three different
places — that was the bug). The Step-5 drag test asserts a known `pos` maps to a known
sat/val, locking the frame.

- [ ] **Step 3: Fill the Task-8 drag-push TODO in `mod.rs`**

Replace the `// TODO(Task 9): push drag capture` line in `ColorPicker::handle_event` (just above `self.active_surface_mut().handle_event(...)`) with this block. It records the region in `active_drag`, applies the down-click inline (so a plain click works), and pushes the capture — all in **picker-local** coords:
```rust
        if let Event::MouseDown(me) = *ev {
            if let Some(region) = self.active_surface().drag_region_at(me.position, body) {
                if let Some(id) = self.state.id() {
                    self.active_drag = Some(region);
                    let origin = self.body_origin;
                    // apply the down-click immediately (single click works)…
                    self.active_surface_mut()
                        .apply_drag(region, me.position, body, &mut self.model);
                    // …then push a capture so subsequent moves keep scrubbing.
                    ctx.push_capture(Box::new(drag::ColorDragCapture::new(id, origin)));
                    ev.clear();
                    return;
                }
            }
        }

- [ ] **Step 4: Add the pump apply arm in `program.rs`**

In the deferred-apply `match` (next to `Deferred::MakeButtonDefault` at `program.rs:1602`), add:
```rust
                                Deferred::ColorPickerDrag { picker, pos } => {
                                    if let Some(p) = self
                                        .group
                                        .find_mut(picker)
                                        .and_then(|v| v.as_any_mut())
                                        .and_then(|a| {
                                            a.downcast_mut::<crate::dialog::ColorPicker>()
                                        })
                                    {
                                        p.apply_drag(pos);
                                    }
                                }
```
Confirm `ColorPicker` is re-exported from `crate::dialog` (Task 10 wires it; for this task add the `pub use` now if missing).

- [ ] **Step 5: Write + run a pump-level integration test**

Add to `program.rs` tests (the row-80 `MakeButtonDefault` precedent, ~line 2515): build a `Program` with a desktop, insert a `ColorPicker` on the Plane tab, dispatch a `MouseDown` in the SV box — this sets `active_drag` + pushes the capture — then push a `Deferred::ColorPickerDrag { picker, pos }` for a **known picker-local `pos`** and pump once; assert the picker's `color()` is the **exact** `hsv_to_rgb` value for the sat/val that `pos` maps to (this is the frame-locking assertion the coordinate contract promised). Also a capture-lifecycle test: `MouseDown` in the SV box → assert a capture was pushed; `MouseMove`, pump → assert `color()` moved; `MouseUp` → assert the capture popped (`captures` empty).

Run: `cargo test --lib colorpick 2>&1 | tail; cargo test --lib color_picker_drag 2>&1 | tail` → PASS.

- [ ] **Step 6: Verify gates + commit**

```bash
cargo test --workspace 2>&1 | tail -10
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5
cargo fmt --all --check
git add -A
git commit -m "feat(colorpick): mouse drag broker (Deferred::ColorPickerDrag)

The window.rs DragCapture pattern for the picker: MouseDown in a draggable region
pushes a ColorDragCapture, each MouseMove posts Deferred::ColorPickerDrag, the
pump downcasts to ColorPicker and apply_drag scrubs the active surface, MouseUp
pops. One new Deferred variant + its pump arm + Context::request_color_drag.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 10: `color_dialog` modal shell + result extraction

**Files:**
- Modify: `src/app/program.rs` (add `ModalCompletion::ColorPick` + its arm + `Program::color_dialog`)
- Modify: `src/dialog/mod.rs` (re-export `ColorPicker`, `Tab`, `color_dialog`-adjacent types)
- Modify: `src/lib.rs` (re-export `ColorPicker`)
- Test: integration tests in `program.rs`

`color_dialog(initial) -> Option<Color>` builds a `Dialog` titled "Select Color" embedding a `ColorPicker` + OK + Cancel, runs it modal, returns `Some(color)` on `cmOK` (via the downcast-`color()` completion), `None` on Cancel/Esc.

- [ ] **Step 1: Add the `ModalCompletion::ColorPick` variant + its apply arm**

In `program.rs`, extend the `ModalCompletion` enum (near line 271):
```rust
    /// `color_dialog` result extraction (the `HistoryPick`/`get_selection` shape):
    /// on `cmOK`, downcast the in-tree modal `ColorPicker` and write its `color()`
    /// into the caller's sink. NOT a `FieldValue` (the `color()` accessor is the
    /// contract; spec non-goal forbids `FieldValue::Color`).
    ColorPick {
        picker: ViewId,
        sink: std::rc::Rc<std::cell::Cell<Option<crate::color::Color>>>,
    },
```
Add an arm to `apply_modal_completion` (near line 1796):
```rust
        ModalCompletion::ColorPick { picker, sink } => {
            if result == Command::OK {
                let c = group
                    .find_mut(picker)
                    .and_then(|v| v.as_any_mut())
                    .and_then(|a| a.downcast_mut::<crate::dialog::ColorPicker>())
                    .map(|p| p.color());
                sink.set(c); // Some(color) on OK; stays None otherwise
            }
            None
        }
```

- [ ] **Step 2: Add `Program::color_dialog`**

Add near `input_box` (after line 768):
```rust
    /// Open the truecolor color-picker modal seeded with `initial`; return the
    /// chosen [`Color`](crate::color::Color) on OK, or `None` on Cancel/Esc.
    ///
    /// An rstv-original extension (not a faithful TV port) — see
    /// `docs/superpowers/specs/2026-06-09-color-picker-design.md`. The result is
    /// read by downcasting the in-tree modal `ColorPicker` to `color()` via a
    /// [`ModalCompletion::ColorPick`] sink (the `HistoryPick` precedent), NOT a
    /// `FieldValue` (the spec's non-goal).
    pub fn color_dialog(&mut self, initial: crate::color::Color) -> Option<crate::color::Color> {
        use crate::dialog::{ColorPicker, Dialog};
        use crate::widgets::{Button, ButtonFlags};

        // 60x23 dialog, centered on the desktop (mirrors input_box centering).
        let mut r = Rect::new(0, 0, 60, 23);
        let desk = self.desktop_size();
        r.r#move((desk.x - 60) / 2, (desk.y - 23) / 2);
        let mut d = Dialog::new(r, Some("Select Color".to_string()));

        let picker_id = d.insert_child(Box::new(ColorPicker::new(
            Rect::new(2, 2, 58, 20),
            initial,
        )));
        d.insert_child(Box::new(Button::new(
            Rect::new(20, 20, 30, 22),
            "O~K~",
            Command::OK,
            ButtonFlags { default: true, ..Default::default() },
        )));
        d.insert_child(Box::new(Button::new(
            Rect::new(31, 20, 41, 22),
            "~C~ancel",
            Command::CANCEL,
            ButtonFlags::default(),
        )));

        let sink = std::rc::Rc::new(std::cell::Cell::new(None));
        let completion = ModalCompletion::ColorPick { picker: picker_id, sink: sink.clone() };
        // initial focus = the picker (so arrows drive it immediately); the dialog's
        // Tab moves focus to OK/Cancel.
        self.exec_view_with_completion(Box::new(d), Some(completion), Some(picker_id), None);
        sink.get()
    }
```
Confirm: `Dialog::insert_child` is `pub(crate)` (it is, `dialog.rs:66`) and `Program::color_dialog` is in the same crate (yes). Confirm `desktop_size()` exists (used by `input_box`).

- [ ] **Step 3: Re-export `ColorPicker` from `dialog` + `lib`**

In `src/dialog/mod.rs` extend the `colorpick` block:
```rust
pub use colorpick::{color_picker_exports_here, ColorPicker, Tab};
```
(Adjust to the actual public items: `ColorPicker`, `Tab`; the `Surface` trait stays `pub(crate)`.) In `src/lib.rs` add `ColorPicker` to the `pub use dialog::{...}` line.

- [ ] **Step 4: Write the failing integration tests, then run**

Add to `program.rs` tests:
```rust
    #[test]
    fn color_dialog_ok_returns_edited_color() {
        // Build a program, drive: switch to RGB, edit, press OK -> Some(color).
        // (Use the existing test harness that builds a Program with a desktop;
        //  inject the keystrokes via the event queue, pump to completion.)
        // Assert the returned Option<Color> is Some and reflects the edit.
    }

    #[test]
    fn color_dialog_cancel_returns_none() {
        // Drive the same dialog, press Esc / Cancel -> None.
    }
```
Fill these following the nearest existing modal integration test (search `fn input_box_centered_ok_round_trip` / `message_box_direct_ok_returns_ok` for the harness shape — how they build the `Program`, push events, and pump). The OK path asserts `Some(_)`; the Cancel/Esc path asserts `None` (the sink is only written on `cmOK`).

Run: `cargo test --lib color_dialog 2>&1 | tail -15` → iterate to PASS.

- [ ] **Step 5: Verify all gates + commit**

```bash
cargo test --workspace 2>&1 | tail -10
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5
cargo fmt --all --check
git add -A
git commit -m "feat(colorpick): color_dialog modal shell + result extraction

Program::color_dialog(initial) -> Option<Color>: a 'Select Color' dialog
embedding the ColorPicker + OK/Cancel, run on the existing modal machinery. The
result is read by downcasting the in-tree modal ColorPicker to color() via a new
ModalCompletion::ColorPick { picker, sink } (the HistoryPick precedent) — no
FieldValue::Color (spec non-goal). Some(color) on OK, None on Cancel/Esc.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 11: Docs reconciliation (PORT-ORDER, HANDOVER, IMPLEMENTATION-LOG, PORTING-GUIDE)

**Files:**
- Modify: `docs/PORT-ORDER.md` (mark rows 81–87 dropped/superseded)
- Modify: `docs/IMPLEMENTATION-LOG.md` (add the color-picker session section)
- Modify: `docs/HANDOVER.md` (Current state → picker landed; Next → row 88)
- Modify: `docs/PORTING-GUIDE.md` (note the picker as an rstv-original, near the RegexValidator note)

- [ ] **Step 1: PORT-ORDER — mark 81–87 dropped**

Change rows 81–87 from their current `✅`/pending marks to a **dropped/superseded** mark (mirror the `TStreamable` disposition), each pointing at the spec. Add a one-line note that the faithful port resumes at row 88 (the outline family). Pattern after how any other dropped item is annotated in that file.

- [ ] **Step 2: IMPLEMENTATION-LOG — add the session section** (newest first, at the top)

Append a section describing: rows 81–82 reverted; the color-picker extension built (model + 4 surfaces + view + drag broker + `color_dialog`); the key seams reused (`window.rs` DragCapture pattern → `ColorDragCapture`, `Deferred` broker shape, `ModalCompletion` result extraction); the deviation from the spec phrasing (gather-by-downcast-`color()`, not `FieldValue::Color`); the geometry/preset/Hsv decisions. One subsection per commit, with the commit hash.

- [ ] **Step 3: HANDOVER — update Current state + Next**

Replace the "build the color-picker extension" Next block: the picker has **landed**; the faithful port now resumes at **row 88** (`TNode` / outline family 88–90, then terminal 91–92). Remove the "Step 0: revert 81–82" instruction (done). Note the picker entry point (`Program::color_dialog`) and that a future theme editor consumes it (needs the D7 Theme extension point first). Keep it slim.

- [ ] **Step 4: PORTING-GUIDE — note the rstv-original**

Near the existing RegexValidator note (the spec references "Appendix / the deviation reference"), add a short line: the truecolor color-picker (`src/dialog/colorpick/`, `Program::color_dialog`) is an rstv-original extension replacing the faithful `TColorDialog` cluster (rows 81–87), which was dropped as a D7 consequence (the flat `TPalette` it edited does not exist under `Theme`). Point at the spec + this plan.

- [ ] **Step 5: Commit**

```bash
cargo test --workspace 2>&1 | tail -5   # confirm still green
git add -A
git commit -m "docs: color-picker extension landed; rows 81-87 dropped, port resumes at 88

Reconciles PORT-ORDER (81-87 dropped/superseded, like TStreamable), adds the
IMPLEMENTATION-LOG session section, updates HANDOVER (picker landed; next = row
88 outline family), and notes the picker as an rstv-original in PORTING-GUIDE.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-review notes (for the executor)

- **Spec coverage:** model (§1) → Task 2; four surfaces (§2) → Tasks 4–7; `ColorPicker` view (§3) → Task 8; modal shell + drag (§4) → Tasks 9–10; testing (§5) → tests in every task; open items (§ deferred) → the **Decisions** section. Revert + PORT-ORDER reconciliation → Tasks 1 + 11.
- **Two spec-phrasing overrides are explicit, not silent:** Decision 1 (no `FieldValue::Color`) and Decision 2 (true 16×16 grid). If an implementer hits a wall where the downcast-`color()` path genuinely cannot return a value without new modal plumbing, **stop and surface it** — do not add `FieldValue::Color`.
- **Coordinate-frame contract (Task 9 Step 2):** ONE frame — **picker-local everywhere**. Keyboard/single-click/drag all feed picker-local positions; each surface subtracts `body.a` exactly once; nothing pre-subtracts `BODY_TOP`. The Task 9 Step 5 drag test asserts a known `pos` → exact `hsv_to_rgb` color, locking the frame. (An earlier draft mixed three frames — that was a real bug, now fixed.)
- **No widget type in `Deferred` (Task 9 Step 1):** `Deferred::ColorPickerDrag` carries only `{picker, pos}`; the region lives in the picker's `active_drag`. This keeps the FOUNDATION `Deferred` enum free of `dialog::colorpick` types (the `MakeButtonDefault`/scroller shape) — do not reintroduce a `region` field there.
- **`ModalCompletion` exhaustiveness (Task 10):** before integrating, `grep -n "match .*completion\|ModalCompletion::" src/app/program.rs` — confirm `apply_modal_completion` is the only non-wildcard `match`; the new `ColorPick` variant must not break a sibling match arm.
- **Each task lands green** (its own commit, gates passing) so the subagent-driven review cadence (spec reviewer → quality reviewer → integrate) applies per task. Task 3 is the one exception (folded into Task 4) because its trait has no consumer yet.
- **`cargo-insta` is not installed** — generate `.snap`s with `INSTA_UPDATE=always`, hand-verify, commit (handover gotcha).
- **4-core cap** on all `cargo` invocations (the machine is shared) — the commands above inherit the workspace default; do not add `-j`.
