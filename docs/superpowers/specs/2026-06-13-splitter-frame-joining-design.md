# Splitter Frame-Joining — design

**Date:** 2026-06-13
**Status:** Proposed (for review)
**Builds on:** `Splitter` (rstv-original extension) — spec
[`2026-06-13-splitter-design.md`](2026-06-13-splitter-design.md), plan
[`2026-06-13-splitter.md`](../plans/2026-06-13-splitter.md). Implemented on branch
`feat/splitter`.

## Goal

Make a `Splitter` embedded in a window look like **one continuous piece of
linework**: its divider lines connect to the surrounding window frame and to each
other with proper box-drawing junctions (`┬ ┴ ├ ┤ ┼` and their double / mixed
variants), instead of floating as bare lines that stop one cell short of the
frame.

Target (a grid — a `tree` sidebar column, the right side split into stacked
`list` / `form` rows — embedded in a passive window):

```
┌────────┬────────────┐
│ tree   │ list       │
│        │            │
│        ├────────────┤
│        │ form       │
│        │            │
└────────┴────────────┘
```

- `┬`/`┴` — the **outer** vertical divider meets the top/bottom frame.
- `┤` — the **inner** horizontal divider meets the **right** frame.
- `├` — the inner divider meets the **outer** vertical divider.

Focused windows draw a **double** frame; a single-line divider still connects
cleanly through **mixed** junctions, so no divider ever needs to change weight:

```
╔════════╤════════════╗
║ tree   │ list       ║
║        ╞════════════╣
║        │ form       ║
╚════════╧════════════╝
```

## Background — this is an rstv-original extension, not a faithful port

Classic C++ Turbo Vision does **not** join linework between views. `TFrame::draw`
renders one window's complete box from a static mask (`initFrame` / `frameLine`)
plus the title break; it never inspects sibling or child views. Adjacent or
nested framed views simply overlap as separate boxes. The only `┬┴├┤` characters
TVision emits are a single window's own frame shape and static art (e.g. the file
dialog's `└─┬` directory tree, which rstv already mirrors).

So there is **no tvision behavior to be faithful to here** — this is a deliberate
rstv-original enhancement, consistent with `Splitter` itself being an
rstv-original extension. (The note in `src/frame.rs` calling this a dropped
"sibling tee-walk" overstates what C++ does; it is a future-cleanup, not a
porting debt.)

The design is therefore judged on a different axis: **does it stay inside the
rstv/TV *structural* mindset** — owner-data-down (D3), no child reaching sideways,
the parent brokering cross-view concerns, drawing via the established
`DrawCtx`/`Buffer` substrate — rather than bolting on a global screen-scraping
hack. That compatibility is the explicit review goal (see "Design rationale vs the
TV mindset").

## Scope

**In scope:** a window may *opt in* to joining the linework of the splitter(s) it
hosts — divider→frame and divider→divider (including nested grids).

**Explicitly out of scope / non-goals:**
- No change to any window that does **not** opt in. Plain windows, dialogs, the
  file dialog, message boxes — all render byte-for-byte as today.
- No global / whole-screen auto-join (that would merge unrelated overlapping
  window frames — rejected).
- No coupling of divider line-*weight* to window focus. Dividers keep their
  natural weight; mixed junction glyphs bridge single↔double. (A future option to
  make dividers track the frame weight is noted under "Future", not built.)
- No new public "split window" constructor — YAGNI; sizing a `Splitter` into the
  interior plus one window flag is the whole ergonomics.

## Design overview

```
Window::draw (only when joined_lines flag set):
  1. self.group.draw(ctx)            // frame child draws its box; splitter
                                     // child draws panes + dividers (as today)
  2. let cells = collect_join_cells(&self.group)   // walk descendants,
                                     // call View::line_join_cells() on each
  3. for cell in cells:              // each cell = an absolute coord to upgrade
        join_one(ctx, cell)          // read buffer arms+weights, write junction
```

The pass runs in the window's own `DrawCtx`, whose clip spans the **frame ring
and the entire interior**, so it can both read the splitter's divider cells and
overwrite the frame's edge cells (a child splitter cannot — its clip stops at the
interior). It is **endpoint-targeted** (only cells a splitter reports) and
**glyph-driven** (the junction is chosen from what is actually painted), so it
never touches pane *content* and needs no knowledge of focus state.

### D3 compatibility — the window is the broker

No child reaches sideways. Each splitter reports only **its own** divider
abutment cells via a defaulted `View` method; the **owning window** (the common
ancestor of its frame and its splitter) collects them and applies the joins. This
is the same shape as the established cross-view brokers in rstv (the
scroller↔scrollbar / listviewer↔scrollbar brokering the pump performs at
deferred-apply): the parent orchestrates, children stay ignorant of each other.

## Components

### 1. `Glyphs` — complete the junction set (`src/theme.rs`)

Single-line junctions already exist (`frame_tee_l ├`, `frame_tee_r ┤`,
`frame_tee_t ┬`, `frame_tee_b ┴`, `frame_cross ┼`). Add the missing ones:

- **Double:** `frame_tee_l_d ╠` (U+2560), `frame_tee_r_d ╣` (U+2563),
  `frame_tee_t_d ╦` (U+2566), `frame_tee_b_d ╩` (U+2569), `frame_cross_d ╬`
  (U+256C).
- **Mixed (single stem branching from a double bar):** `frame_tee_t_dh ╤`
  (U+2564, double horizontal, single up→down stem), `frame_tee_b_dh ╧` (U+2567),
  `frame_tee_l_dv ╞` (U+255E, double vertical, single right stem),
  `frame_tee_r_dv ╡` (U+2561), and the two mixed crosses `frame_cross_dh ╪`
  (U+256A), `frame_cross_dv ╫` (U+256B).

These are seeded in every theme's `Glyphs` exactly like the existing frame glyphs
(D7: glyphs live in the theme, not hardcoded). Naming follows the existing
`frame_*` convention; the `_d`/`_dh`/`_dv` suffixes mean "double", "double
horizontal bar", "double vertical bar".

### 2. `DrawCtx` cell read-back (`src/view/context.rs`)

`DrawCtx` today only writes. Add a clipped read:

```rust
/// Read the character currently in the buffer at view-local (x, y).
/// Returns `None` if outside the clip. (Read-back for the line-join pass.)
pub fn get_char(&self, x: i32, y: i32) -> Option<char>;
```

It mirrors `put_char`'s clip + origin translation, reading `Buffer::get(...)`'s
cell symbol. This is the one piece that inspects already-painted cells — justified
by rstv's whole-tree-redraw-into-a-back-buffer model (D8): the buffer is a
first-class intermediate the renderer already owns and diffs, so reading it back
within a clip before flush is consistent with the rendering substrate (it is *not*
the C++ damage-tracking model, which rstv deliberately replaced under D8).

### 3. `View::line_join_cells()` — the brokered report

Add to the `View` trait, defaulted to empty so it is purely additive:

```rust
/// Cells this view wants line-joined into its surroundings, in ABSOLUTE
/// screen coordinates. Default: none. A `Splitter` returns the cells just
/// past each divider end (where a divider abuts the frame or a parent
/// divider). The owning window collects these and upgrades each to the
/// matching box-drawing junction. Empty for every non-splitter view.
fn line_join_cells(&self) -> Vec<JoinCell> { Vec::new() }
```

`JoinCell` (a small public struct in the splitter or a shared `view` module):

```rust
pub struct JoinCell {
    pub at: Point,        // absolute screen cell to upgrade
    pub arms: ArmMask,    // which directions the *incoming* divider arm points
}
```

`ArmMask` is a 4-bool / bitflag set {up, down, left, right} naming the directions
in which the *reporting divider* extends from `at`. Division of labor in the pass:
the **declared** arm fixes which neighbor is the structural divider (its *weight*
is read from the buffer at that neighbor); the **bar** that `at` lies on (the
frame edge or the parent divider) has its arms + weight read from `at`'s own glyph
and its in-line neighbors. So the declared arm is the intent ("a divider lands
here"), and the buffer reads supply the weights — keeping the pass content-safe
(it only acts where a splitter declared a landing) without hardcoding weights.

**Collection is recursive via the trait, not a manual tree walk:**
- `View` default → empty.
- `Group` overrides to concatenate its children's `line_join_cells()` (children
  already report absolute coords, so no translation).
- `Splitter` overrides to return its own divider abutment cells **plus**
  `self.group.line_join_cells()` (its panes — so a nested splitter pane
  contributes automatically).
- `Window` does **not** override; it inherits the `Group` aggregation via the
  delegate macro, so `window.line_join_cells()` yields every descendant
  splitter's cells (its frame child contributes nothing).

Per the CLAUDE.md delegation rule, adding this `View` method **requires a matching
forwarder in `tvision-macros/src/specs.rs`** (and the `delegate_view` spy test
will guard it thereafter).

### 4. `Splitter::line_join_cells()` (`src/widgets/splitter/mod.rs`)

For each divider, the splitter already knows its axis position and that it runs
the full cross-extent of the splitter bounds (it draws it that way). It reports
the **two abutment cells** just past the divider's ends, in absolute coords
(`self.abs_origin` + local), each with the arm pointing *back into* the divider:

- `Cols` divider at local x = `dx`, running local y `0..run`: report
  `(abs.x+dx, abs.y-1)` arm=Down (top abutment) and `(abs.x+dx, abs.y+run)`
  arm=Up (bottom abutment).
- `Rows` divider similarly reports left/right abutments with arm=Right / arm=Left.

A `Hidden` or `Locked` divider that drew no line still reports its cells, but the
glyph-driven pass finds no perpendicular line there and leaves the cell unchanged
— so invisibility is preserved with no special-casing. Nested splitters each
report their own dividers; an inner divider's outer-end abutment lands exactly on
the outer divider (→ `├`/`┤`) or the frame (→ `┤`/`┴`), so grids join correctly
with no inner/outer coordination beyond the window's collection walk.

### 5. The pure join function + the cell visitor

A small, standalone, unit-testable function with **no view/draw dependency**:

```rust
/// Given the line arms present at a cell — each classified as Off / Single /
/// Double — return the box-drawing char that joins them, or `None` to leave the
/// cell unchanged (fewer than 2 arms, or no usable glyph).
fn junction_glyph(up: Arm, down: Arm, left: Arm, right: Arm, g: &Glyphs)
    -> Option<char>;
```

`Arm = { Off, Single, Double }`. The function is a finite map over the arm
combinations to the `Glyphs` junction fields (single `┬┴├┤┼`, double `╦╩╠╣╬`,
mixed `╤╧╞╡╪╫`). Combinations with no exact Unicode box char (e.g. three
different weights meeting) fall back to the dominant-weight junction; in practice
only two weights ever meet (a divider and the frame), so the table is small.

The **visitor** (`join_one`) for a reported `JoinCell`:
1. Convert `at` (absolute) to window-local via the window's `DrawCtx` origin.
2. Classify each of the four neighbors by reading the buffer glyph there
   (`ctx.get_char`) against the known line glyphs (`frame_h/_v`, `frame_h_d/_v_d`)
   → `Arm::{Off,Single,Double}`. OR-in the reported `arms` (the incoming divider,
   which may be the cell's own painted line).
3. `junction_glyph(...)`; if `Some(ch)`, `ctx.put_char(localx, localy, ch, style)`
   using the existing cell's style (so the junction keeps the frame's color).

### 6. `Window` — the opt-in flag + draw override (`src/window/window.rs`)

- Add `joined_lines: bool` (default `false`) + a builder `with_joined_lines(self)
  -> Self` and/or a setter.
- Add a `draw` override (Window currently delegates `draw` to its group via
  `#[delegate]`): call the group draw as before, then, **iff `joined_lines`**,
  run the collect-and-join pass. When the flag is false the override is
  behaviorally identical to the delegated draw (verified by snapshot — existing
  window snapshots must not change).

```rust
fn draw(&mut self, ctx: &mut DrawCtx) {
    self.group.draw(ctx);
    if self.joined_lines {
        for c in self.group.line_join_cells() { // recursive aggregation (above)
            join_one(ctx, &c);
        }
    }
}
```

The cells come from the recursive `line_join_cells()` aggregation (component 3) —
no bespoke walk. (`self.group.line_join_cells()` folds the frame child → none and
the splitter child → its full nested set.)

### 7. Example (`examples/splitter.rs`)

Rework the existing demo into the grid: `Splitter::cols([tree, Splitter::rows(
[list, form])])` sized into the interior of a `Window::…with_joined_lines()`. It
must build and run; it demonstrates `┬ ┴ ├ ┤` against the frame and the inner
`├` crossing. (Reuses the real widget constructors already wired in the current
example.)

## Data flow

```
draw frame child ─┐
draw splitter ────┤→ back buffer has frame box + bare divider lines
                  │
window.draw tail ─┘
   └ self.group.line_join_cells() → [JoinCell{abs, arms}, …]
        (recursive: Group folds children; nested splitters contribute their own)
   └ for each: read 4 neighbor glyphs from buffer (DrawCtx::get_char),
        classify arms, junction_glyph(...), put_char the junction
   └ renderer diffs back vs front buffer and flushes (unchanged)
```

## Coordinates & ordering

- Splitters report **absolute** screen coords (using `abs_origin`, captured each
  draw — fresh, because children draw before the window's join tail).
- The window converts absolute→window-local for `DrawCtx` get/put.
- The join runs **after** all children have painted (frame and splitter both
  done), **before** the renderer diff — exactly the right window in the cycle.

## Testing

- **Pure unit tests** for `junction_glyph`: every arm combination → expected
  `Glyphs` field (single ┬┴├┤┼, double ╦╩╠╣╬, mixed ╤╧╞╡, no-op when <2 arms).
- **`Splitter::line_join_cells`** unit tests: a 2-pane cols splitter reports the
  two correct abutment cells with the right arms; a nested grid reports the
  expected set.
- **Snapshot tests** (HeadlessBackend) on a small `Window::with_joined_lines`
  hosting:
  - a single cols splitter in a **passive** frame → `┬…┴`;
  - the same in a **focused/double** frame → `╤…╧`;
  - the **grid** → the `├` crossing + `┤` to the right frame.
- **Regression:** an existing window snapshot WITHOUT the flag is unchanged
  (gating proof). The `delegate_view` spy test passes (new `View` method
  forwarder added to `specs.rs`).

## Design alternatives considered

**A. Frame draws the tees via window-brokering (no buffer read-back).** The
window brokers divider positions from the splitter to the `Frame`, and the frame
draws `┬┴` at those columns as part of its own line (closer to how C++
`frameLine` embeds junctions; no screen read-back). *Rejected as primary because*
the **interior** nested crossing (`├` where an inner divider meets the outer
divider) is drawn by two different splitters and cannot be resolved by the frame
at all — it would need a second, different mechanism, and the outer splitter's
`draw` would have to ingest externally-supplied crossing rows. The buffer-driven
join handles frame edges and interior crossings with **one** uniform rule. The
read-back is cheap and sits naturally on rstv's D8 back-buffer. (If review judges
screen read-back as too far from the TV mindset, this alternative is the fallback
for the frame edges, paired with a splitter-internal join for crossings.)

**B. Global post-paint pass in the renderer over registered regions.** More
general (works with no host window), but a splitter is never used outside a
window, so the generality is unused weight; it also needs a new render-time region
channel threaded through `DrawCtx`. Rejected for YAGNI; the window-scoped pass is
strictly simpler for the only real use case.

**C. Whole-screen auto-join.** Rejected — merges unrelated overlapping window
frames into spurious junctions.

## Future (not in this spec)

- Optionally make dividers **track the frame weight** (single when passive, double
  when focused) for an even more unified look — needs the window's active state to
  reach the splitter; deferred because mixed junctions already read seamlessly.
- A `Role::Splitter*` theme entry set if dividers should be themable
  independently of the frame roles (today they reuse `FramePassive` /
  `FrameDragging`).
- Auto-enabling `joined_lines` when a `Splitter` is detected as a window's body
  (vs the explicit flag) — only if the explicit flag proves annoying in practice.

## Methodology note

Faithful-by-default is not in tension here: there is no C++ behavior to port, so
this is squarely an rstv-original extension (precedent: `RegexValidator`,
`Splitter`). It is gated and additive — it changes nothing unless a window opts
in — so it carries no risk to the faithful baseline.
