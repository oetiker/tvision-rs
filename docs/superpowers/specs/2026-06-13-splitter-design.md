# Splitter — design spec

> **Status:** design approved (brainstorm complete), pending implementation plan.
> **Date:** 2026-06-13. **Type:** rstv-original *extension* on a faithful base.
> Turbo Vision has no splitter/tiled-pane class; this is a modern extension built
> entirely on existing TV-native machinery (`Group`, `GrowMode`, `DragCapture`,
> `Deferred`, `Theme`, `Command`/keymap), in the spirit of the `RegexValidator`,
> the color picker, and the configurable keymap (extensions *alongside* the port).

## Why this exists

We want a **multi-pane view**: one window subdivided into several regions by
draggable divider lines — a tree in one pane, a list in another, a large form in
a third — that behaves as a *single* window, not a manually-arranged set of
windows. The framing the brainstorm settled on:

- The moment each pane is framed like a mini-window, you've just re-invented
  "arrange N windows" and lost the point. So panes are **frameless**; the divider
  line is the only separator. (A framed view may still be dropped into a slot, but
  that is the *content's* choice, never the Splitter's.)
- It must be a **generic, configurable component**. "Three panes side by side" is
  one trivial configuration; grids and arbitrary tiling are others. The hard part
  is a good API, not the rendering.
- It must be **TV-like**. In Turbo Vision you build every UI by constructing views
  and `insert()`ing them into a `TGroup` (a dialog inserts its buttons and input
  lines). A `Splitter` is therefore a thin specialization of our existing `Group`.

## Core model

A **`Splitter` is a `Group` specialization** (D2 embed-and-delegate: it embeds a
`Group` and forwards un-overridden `View` methods via `#[delegate(to = group)]`).

- One Splitter = **one axis** with **N children (N-ary)** and **N−1 dividers** in
  the 1-cell gaps between them. `cols` ⇒ vertical dividers / horizontal axis;
  `rows` ⇒ the transpose.
- A child may itself be a `Splitter`. That recursion expresses **every** layout.
  The nesting rule is exactly: **same direction → more panes in the same
  splitter; perpendicular direction → nest a sub-splitter.** You never nest just
  to add another pane along the same axis.
- A divider drag repartitions only its **two adjacent** neighbors — local and
  predictable regardless of how many panes share the splitter. No cascade.

```
3 vertical (ONE splitter, 2 dividers):   Splitter::cols([tree, list, form])

grid (a column that is itself split):    Splitter::cols([
                                              tree,
                                              Splitter::rows([form, log]),
                                          ])
```

## Configuration surface — three things, one API

There are exactly three things you configure. The **same setters work at build
time and at runtime** — runtime reconfiguration is the same surface, not a
bolted-on second API.

### ① Per-pane: `Constraints { weight, min, max }`

How a pane claims space.

```rust
Constraints::flex()            // weight 1, min 0, max ∞   (fully elastic)
Constraints::weight(2)         // 2× the free space of a weight-1 pane
Constraints::fixed(20)         // min == max == 20  → a pinned 20-cell sidebar
Constraints::flex().min(10)    // elastic, never narrower than 10
Constraints::flex().max(40)    // elastic, never wider than 40
```

- `weight: u16` — share of the *free* space (default 1).
- `min: i32`, `max: i32` — hard bounds in cells along the splitter's axis
  (default `min = 0`, `max = i32::MAX`).
- **A fixed pane is just `min == max`** — there is no separate "fixed" flag or
  code path; it falls out of the constraint model (the flexbox / Qt-size-policy
  idea). Coordinates are `i32`, faithful to the rest of the port.

### ② Per-divider: `DividerStyle`

How the seam *after* a given pane looks and behaves. **Set per divider**, with a
Splitter-level default — this is what unlocks design variants (a visible seam
between major regions, invisible seams subdividing one region).

```rust
DividerStyle::Line      // always drawn (║ / ─), grab-and-drag anytime
DividerStyle::Handle    // clean look; only a small grab nub hints it's draggable
DividerStyle::Hidden    // invisible & seamless in normal use, BUT resizable in reconfig mode
DividerStyle::Locked    // invisible AND immovable — a permanent boundary, even in reconfig mode
```

`Hidden` vs `Locked` is the load-bearing distinction: `Hidden` = clean-but-still-
adjustable; `Locked` = frozen structural seam.

### ③ The `Splitter` itself

Orientation (`cols`/`rows`), the panes, and a `default_divider` style.

## Public API

A view has **no id until it is inserted** (`View::id()` returns
`Option<ViewId>`, `None` beforehand). The id is **minted by `insert` and returned
to the caller** — exactly as `Group::insert(...) -> ViewId` works today. That
gives two construction forms over the identical underlying tree:

**Imperative — when you will reconfigure at runtime** (keep the returned ids):

```rust
let mut split = Splitter::cols().default_divider(DividerStyle::Hidden);
let tree_id = split.insert(tree, Constraints::fixed(20));      // ← id comes back here
let list_id = split.insert(list, Constraints::flex().min(10));
let form_id = split.insert(form, Constraints::weight(2));
split.set_divider_style(1, DividerStyle::Line);               // seam after pane 1
```

**Declarative / chained — for a static layout you won't touch again** (ids
discarded internally; `.pane()` just calls `insert` and drops the id):

```rust
let split = Splitter::cols()
    .default_divider(DividerStyle::Hidden)
    .pane(tree, Constraints::fixed(20))
    .pane(list, Constraints::flex().min(10))
    .pane(form, Constraints::weight(2))
    .divider(1, DividerStyle::Line);
```

**Runtime reconfiguration — the same setters, addressed by handle:**

```rust
split.set_constraints(list_id, Constraints::fixed(30));   // pin the list
split.set_divider_style(1, DividerStyle::Hidden);         // hide that seam
split.set_default_divider_style(DividerStyle::Line);      // reveal the rest
split.insert(notes, Constraints::flex());                 // add a pane live
split.remove(form_id);                                    // drop a pane live
split.relax(tree_id);                                     // fixed → flexible, no visual jump
```

**`relax(pane_id)` — the position-preserving constraint change.** It drops a
pane's `min`/`max` to `(0, ∞)` (making it flexible) **and** sets its weight so the
divider does not move: the pane keeps its current solved size. This is how a user
opts into "drag-resizable" behavior for a pane that started life `fixed` — without
the framework having to bake dual drag semantics in (see Resize semantics below).
The weight that preserves the layout has a closed form:

```text
weight_pane = Σ(current weights of the flexible panes) × pane_current_size / current_free_space
```

so relaxing never makes any divider jump. (A general
`set_constraints_keeping_size(pane_id, new)` is a natural extension, but `relax`
covers the stated use case for v1.)

- **Panes are addressed by `ViewId`** (the value `insert` returned).
- **Dividers are addressed by index `i`** — the seam between pane `i` and pane
  `i+1`. Indices shift when panes are inserted/removed; this is inherent and
  documented (callers reconfigure dividers relative to the current pane order).
- When reconfiguration is triggered from **inside** the event loop (a divider
  drag, or a child asking to reconfigure its parent), the splitter already holds
  its children's ids and resolves them itself via the existing `Deferred`/broker
  seam — the caller never threads ids in. The app-facing surface is identical.

## Layout algorithm

A flexbox-style fill along the splitter's axis, run on insert, on
parent/terminal resize, and after every drag:

1. Compute usable axis length = `total − (n_dividers × 1)` (each divider occupies
   one cell; `Hidden`/`Locked` still reserve their cell so the geometry is stable
   whether or not the seam is drawn).
2. Give every pane its `min`.
3. Distribute the remaining free cells across panes in proportion to `weight`.
4. When a pane would exceed its `max`, clamp it and **redistribute the remainder**
   to the still-growing siblings (repeat until stable — standard "fill until
   saturated" pass).
5. Assign each child its `Rect` and issue the bounds via the existing
   `change_bounds`/`Deferred` flow.

The **cross-axis** extent of every child is the full splitter extent (a `cols`
splitter makes every pane full height). Parent/terminal resize reuses the
existing `GrowMode.rel` / `calc_bounds` reflow — panes keep their proportions for
free; only the constraint re-solve is splitter-specific.

## Resize semantics — what a drag/keyboard-move mutates (Option A)

Moving divider *i* by Δ cells transfers Δ between the two adjacent panes *i* and
*i+1*. The recording rule:

- **Both neighbors flexible:** rewrite the **two neighbors' weights** to match
  their new sizes, so the new split is a *ratio* that survives the next terminal
  resize (rather than snapping back). **Invariant: the drag preserves
  `weight_i + weight_{i+1}`** — it only redistributes weight *within* the pair. So
  the drag is strictly local not just on screen now but on every future resize:
  the other panes' shares are mathematically untouched.
- **A `fixed` (`min == max`) neighbor is a hard wall.** A fixed pane has no
  free-space share and a pinned size, so the divider **cannot move into it** —
  it pins its adjacent divider. The drag simply clamps against it (a divider
  between two fixed panes is therefore immovable, Δ = 0). Fixed means *the user
  cannot drag-resize it*. To make a constant-width pane drag-resizable, `relax()`
  it first (above), then drag it as a flexible pane.
- A `Locked` *divider* never responds regardless of its neighbors' constraints —
  that is the explicit "this seam never moves" marker, kept distinct from the
  pane-level constraint question.

Clamping always respects every pane's `[min, max]`. Weight is rewritten in a
precision sufficient to reproduce integer cell sizes (implementation: either
widen `weight` to an `i32` free-cell allocation, or keep `u16` as the authoring
unit and solve in higher internal precision — decided in the plan).

## Interaction

Both input paths ship in v1 (per the brainstorm).

**Mouse drag (live).** Any *visible* divider (`Line` or `Handle`, or any divider
while reconfig mode is active) is grab-and-drag at any time via the existing
`DragCapture` seam the scrollbars use. A drag moves the boundary between the two
adjacent panes per the **Resize semantics** above (Option A: rewrite the two
flexible neighbors' weights, sum-preserving; a `fixed` neighbor is a hard wall;
a `Locked` divider never responds).

**Keyboard reconfig mode.** A `Command` (default a function key, wired through the
`Command`/keymap system so it is rebindable) toggles reconfig mode. While active:

| Key | Action |
|---|---|
| `Tab` / `Shift-Tab` | select previous/next divider (skipping `Locked`) |
| arrows | move the selected divider along the axis (clamped to min/max) |
| `Enter` | commit and exit |
| `Esc` | cancel (restore the pre-mode layout) and exit |

In reconfig mode **all** dividers light up (highlighted line + handles) and become
resize targets regardless of their configured style — this is the only way to move
a `Hidden` divider. On exit they revert to their per-divider style. This mirrors
Turbo Vision's window move/resize mode (Ctrl-F5), applied to internal dividers.

Normal-use `Tab` between panes is unchanged — the Splitter is a `Group`, so focus
cycles through focusable content views for free; reconfig-mode `Tab` (divider
selection) is active only inside the mode.

## Rendering & theming

- Divider glyphs and colors are `Theme` entries: a **normal** pair (per style) and
  a **highlighted** pair for reconfig mode / the divider under an active drag.
- A `cols` divider draws a vertical run (`║`-class glyph) in its reserved column;
  `rows` draws a horizontal run. `Handle` draws a small nub mid-span over an
  otherwise blank reserved cell. `Hidden`/`Locked` draw nothing in normal mode
  (the reserved cell renders as pane background) and the highlighted line only
  while reconfig mode is active.

## Reused seams vs. net-new

- **Reused:** `Group` (children, focus, event routing), `GrowMode.rel` /
  `calc_bounds` (resize reflow), `DragCapture` + `Deferred` bounds (drag),
  `Theme` (glyphs/colors), `Command`/keymap (reconfig toggle), `#[delegate]`
  (forwarding).
- **Net-new:** the `Constraints` solver, the per-divider `DividerStyle` state +
  draw + hit-test in the inter-child gaps, the drag→weight repartition, and the
  reconfig-mode state machine.
- Per the delegation rule, this adds **no new `View` trait method**, so no
  `tvision-macros/src/specs.rs` forwarder is required. If a divider drag needs a
  capability not already in `Deferred`, that is a **new `Deferred` variant** (no
  forwarder needed) — not a `Context::new` parameter.

## Verification (D11 snapshot tests, `HeadlessBackend`)

- Equal 3-column split (`weight 1,1,1`) renders three equal panes + 2 dividers.
- Fixed sidebar (`fixed(20)`) keeps its width across a terminal resize while the
  others reflow.
- A synthesized mouse drag repartitions two flexible neighbors and clamps at
  `min`; the non-adjacent panes' sizes are unchanged after a later resize
  (sum-preserving invariant).
- A drag against a `fixed` neighbor does not move (hard wall); `relax()` then
  makes the same pane drag-resizable with **no visual jump** at the moment of
  relaxing (position-preserving weight).
- A `max`-saturated pane redistributes remainder to siblings.
- Nested `rows`-in-`cols` grid renders correctly.
- Per-divider styles render distinctly (`Line` vs `Handle` vs `Hidden`).
- Reconfig-mode highlight render (all dividers lit, selected one emphasized).

## Demo example

`cargo run --example splitter` (name TBD): one `Window` whose body is
`Splitter::cols([Outline (tree), ListBox (list), Dialog-style form])` — the
original request, as the showcase: live mouse-drag dividers + a key to enter
reconfig mode.

## Non-goals (v1)

- Per-pane frames/titlebars (panes are frameless by design; drop a framed view in
  a slot if you need it).
- Collapse-to-zero / accordion toggles (a pane's `min` floors it; collapsing is a
  later extension if wanted).
- Saving/restoring layouts to disk (the constraint values are plain data and
  serialize cleanly, but persistence is out of scope here).
- "Mouse only inside reconfig mode" was explicitly *not* chosen — live mouse drag
  is always available on visible dividers.
