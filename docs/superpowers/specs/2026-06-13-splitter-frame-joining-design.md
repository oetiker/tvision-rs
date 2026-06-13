# Splitter Frame-Joining — design

**Date:** 2026-06-13
**Status:** Proposed (for review) — v3, after two Turbo-Vision-mindset reviews
(v2 fixed the read-back/trait-method issues; v3 fixes the `as_any_mut` downcast
keystone the second review found)
**Builds on:** `Splitter` (rstv-original extension) — spec
[`2026-06-13-splitter-design.md`](2026-06-13-splitter-design.md), plan
[`2026-06-13-splitter.md`](../plans/2026-06-13-splitter.md). Implemented on branch
`feat/splitter`.

## Goal

Make a `Splitter` embedded in a window look like **one continuous piece of
linework**: its divider lines connect to the surrounding window frame and to each
other with proper box-drawing junctions, instead of floating as bare lines that
stop one cell short of the frame.

Target — a grid (`tree` sidebar column; the right side split into stacked
`list` / `form` rows) embedded in a passive window:

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

A focused window draws a **double** frame; a single-line divider still connects
cleanly through **mixed** junctions (`╤ ╧ ╞ ╡`), so a divider never has to change
weight:

```
╔════════╤════════════╗
║ tree   │ list       ║
║        ╞════════════╣
║        │ form       ║
╚════════╧════════════╝
```

## Background — what Turbo Vision actually does

Classic Turbo Vision **does** join framed linework, in `TFrame`. `TFrame::frameLine`
(`magiblot-tvision/source/tvision/framelin.cpp`) builds each frame row as a
**per-cell arm bitmask** (`FrameMask`), seeded from the base frame bits, then
**walks the owner's subviews** (`owner->last->next`) and, for every `ofFramed`
visible sibling abutting the line, ORs junction bits into the mask; finally it
maps `frameChars[mask]` → the box glyph (`┬ ┴ ├ ┤ ┼`). So TV composes connected
linework **from a bitmask the frame owns**; it never reads the screen back.

rstv **dropped this** behavior (documented at `src/frame.rs:50-55`) for a specific
reason: `frameLine` has the *frame reach sideways to read its siblings'
`origin`/`size`, which is exactly what deviation **D3 (owner-data-down — no
sideways pointers)** forbids. So today rstv draws plain corners and edges.

**Implications for this feature:**
- The **divider→frame** join is a **faithful revival** of TV's `frameLine`
  tee-walk — re-expressed in a D3-legal way (below). Not a new idea.
- The **divider→divider** interior cross (`├`/`┼`) is **rstv-original**: TV has no
  splitter, so there is no interior cross to be faithful to.

(An earlier v1 of this spec wrongly claimed TV does no joining and proposed a
whole-buffer read-back pass. The TV-mindset review corrected the premise and
rejected the read-back as re-introducing exactly the screen-inspection that D8
deleted. This v2 is the corrected design.)

## The D3 inversion (the key idea)

TV's `frameLine` is the frame **pulling** sibling geometry sideways. D3 forbids a
child reaching sideways, so we **invert the data flow**: the **owning window**
(the common parent of its frame and its splitter) reads the divider positions from
its splitter child (parent→child, allowed) and **pushes them down to the frame**
as data (owner-data-down, allowed). The frame then composes its line exactly like
`frameLine` — same algorithm, same visual result — but fed by pushed data instead
of a sideways walk. No child reaches sideways; nothing reads the screen back.

The one net-new piece, the interior `├`/`┼`, is composed by the **outer splitter
during its own draw**, because the outer splitter *owns* the inner sub-splitter as
a pane child and can read its divider positions as owner-data — again parent→child,
local, no read-back.

## Scope

**In scope:** a window may opt in to joining the linework of the splitter(s) it
hosts — divider→frame and divider→divider (including nested grids).

**Non-goals:**
- No change to any window that does not opt in. Plain windows, dialogs, the file
  dialog, message boxes — byte-for-byte unchanged.
- **No buffer read-back / screen inspection** (rejected by review as re-adding
  what D8 deleted).
- **No new `View` trait method.** Producers are concrete (`Splitter`, `Frame`);
  the window/parent reaches them by the existing `as_any_mut` downcast (the same
  mechanism Window already uses to push the zoom flag to its Frame —
  `window.rs:357-365` → `child_mut → as_any_mut → downcast_mut::<Frame>()`). This
  works for `Frame` because `Frame` overrides `as_any_mut` to return `Some(self)`
  (`frame.rs:450-452`). **Keystone fix (required):** `Splitter` does **not** yet
  do this — its `#[delegate(to = group)]` forwards `as_any_mut` to the inner
  `Group`, which returns `None`, so a parent cannot currently downcast a pane (or
  the window's interior child) to `Splitter`. This design therefore requires
  `Splitter` to override `as_any_mut` itself (see Component 5). That is **not** a
  new trait method (`as_any_mut` already exists and is in
  `tvision-macros/src/specs.rs`), so the `delegate_view` spy test stays green — it
  is only a `skip(as_any_mut)` + a one-line override, exactly the `Frame`
  precedent.
- No coupling of divider line-*weight* to window focus. Dividers keep their
  natural weight; mixed junction glyphs bridge single↔double.
- No new "split window" constructor (YAGNI).
- No global / whole-screen auto-join (would merge unrelated overlapping frames).

## Design overview — two composition sites, both owner-data-down

```
Window::draw (only when joined_lines):
  1. marks = interior_splitter.frame_junction_marks(frame_bounds)   // parent→child
  2. frame_child.set_junction_marks(marks)                          // owner-data-down
  3. self.group.draw(ctx)        // Frame composes tees from marks (faithful
                                 // frameLine); Splitter draws panes + dividers;
                                 // outer Splitter overlays its own ├/┼ crossings
```

- **Site 1 — Frame (divider→frame):** the Frame gains the `frameLine`-style
  composition: when emitting an edge cell that carries a junction mark, it
  substitutes the matching tee glyph (chosen from the frame's own weight × the
  mark's stem weight). Marks are pushed by the window each draw, computed from the
  splitter's current layout (so they track drags/resizes).
- **Site 2 — Splitter (divider→divider):** while drawing its own dividers, the
  outer splitter inspects each adjacent pane; if a pane is itself a `Splitter`
  with perpendicular dividers, it overlays `├`/`┤`/`┼` on its own divider cell at
  those positions (weight-correct), instead of the plain `│`/`─`.

Nothing reads the buffer; nothing reaches sideways; no universal trait grows.

## Components

### 1. `Glyphs` — complete the junction set (`src/theme.rs`, D7)

Single-line junctions already exist (`frame_tee_l ├`, `frame_tee_r ┤`,
`frame_tee_t ┬`, `frame_tee_b ┴`, `frame_cross ┼`). Add, seeded into every theme's
`Glyphs` like the other frame glyphs:

- **Double:** `frame_tee_t_d ╦` (U+2566), `frame_tee_b_d ╩` (U+2569),
  `frame_tee_l_d ╠` (U+2560), `frame_tee_r_d ╣` (U+2563), `frame_cross_d ╬`
  (U+256C).
- **Mixed — double bar / single stem:** `frame_tee_t_dh ╤` (U+2564),
  `frame_tee_b_dh ╧` (U+2567), `frame_tee_l_dv ╞` (U+255E), `frame_tee_r_dv ╡`
  (U+2561), and crosses `frame_cross_dh ╪` (U+256A), `frame_cross_dv ╫` (U+256B).
- **Mixed — single bar / double stem** (only when a reconfig-double divider meets
  a passive single frame — an edge case; include for completeness):
  `frame_tee_t_sh ╥` (U+2565), `frame_tee_b_sh ╨` (U+2568), `frame_tee_l_sv ╟`
  (U+255F), `frame_tee_r_sv ╢` (U+2562).

Naming extends the existing `frame_*` convention (`_d` = both double; `_dh`/`_dv`
= double bar with single perpendicular stem; `_sh`/`_sv` = single bar with double
stem).

### 2. The pure junction-glyph selector (unit-testable, no view deps)

```rust
/// Pick the box-drawing junction for an edge cell. `edge` = which frame edge
/// (Top/Bottom/Left/Right); `bar` = the frame line's weight (Single/Double);
/// `stem` = the abutting divider's weight. Returns the matching `Glyphs` field.
fn frame_junction(edge: Edge, bar: Weight, stem: Weight, g: &Glyphs) -> char;

/// Pick the interior junction where two dividers meet. `through` = the weight of
/// the divider being drawn; `branch` = directions+weights of meeting dividers.
fn divider_junction(...) -> char;
```

`Weight = { Single, Double }`, `Edge = { Top, Bottom, Left, Right }`. These are
small finite maps to the `Glyphs` fields above — the rstv-local equivalent of TV's
`frameChars[mask]` table. They are the only "logic" and get exhaustive unit tests.

### 3. `Frame` junction marks + mark-aware draw (`src/frame.rs`)

```rust
pub struct JunctionMark {
    pub edge: Edge,    // which frame edge this lands on
    pub offset: i32,   // frame-local position along that edge
    pub stem: Weight,  // the abutting divider's weight
}

impl Frame {
    /// Owner-data-down: the owning window pushes the divider abutment marks the
    /// frame should join into its border. Replaced each draw; empty = today's
    /// plain frame (so non-joined windows are unchanged).
    pub(crate) fn set_junction_marks(&mut self, marks: Vec<JunctionMark>);
}
```

`Frame::draw` is extended: as it emits each border cell, if a mark matches that
edge+offset it writes `frame_junction(edge, self_weight, mark.stem, glyphs)`
instead of the plain edge/corner glyph (`self_weight` = Double when active, Single
otherwise — the frame already branches on this, `frame.rs:246-252`). The per-cell
top/middle/bottom edge loops (`frame.rs:277-305`) make this a clean drop-in. Two
ordering/guard requirements:
- **Corner guard:** ignore any mark at a corner offset (0 or `size.x-1` /
  `size.y-1`) — corners keep their corner glyph. (A divider abutment can only land
  on an interior edge cell for an interior-filling splitter, but guard explicitly.)
- **Apply marks before the title/number/icon overlays** (`frame.rs:307-362`) so an
  icon never lands on a junction. (Junctions are interior-edge cells, away from the
  top-center title, so this is non-conflicting in practice — but ordering it this
  way is unambiguous.)

With no marks, the output is identical to today. This is the faithful `frameLine`
composition, minus the forbidden sideways walk (the data arrives pre-computed).

### 4. `Window` — opt-in flag + brokering draw override (`src/window/window.rs`)

- Add `joined_lines: bool` (default `false`) + builder `with_joined_lines(self)
  -> Self` / setter.
- Override `draw` (Window currently delegates it to its group): when
  `joined_lines`, (a) find the interior splitter child by trying an `as_any_mut` →
  `downcast_mut::<Splitter>()` on each non-frame child (the one that succeeds is
  the splitter; needs the Component-5 `Splitter` override), (b) collect its
  `frame_junction_marks(frame_bounds)` into an owned `Vec`, (c) **drop that borrow**
  and then downcast the Frame child (`downcast_mut::<Frame>()`, the existing zoom
  precedent) and `set_junction_marks`, then (d) draw the group as usual. When the
  flag is off (or there is no splitter child), it is behaviorally identical to the
  delegated draw — existing window snapshots must not change.

```rust
fn draw(&mut self, ctx: &mut DrawCtx) {
    if self.joined_lines {
        let fb = self.frame_bounds();
        // Borrow the splitter child mutably only to READ its layout; the marks
        // Vec is owned, so the borrow ends before we touch the frame child.
        let marks = self.interior_splitter_mut()           // child_mut + downcast_mut::<Splitter>
                        .map(|s| s.frame_junction_marks(fb));
        if let (Some(marks), Some(frame)) = (marks, self.frame_mut()) { // downcast_mut::<Frame>
            frame.set_junction_marks(marks);
        }
    }
    self.group.draw(ctx);
}
```

`interior_splitter_mut()` iterates the group's non-frame children and returns the
first that `downcast_mut::<Splitter>()` succeeds on (no stored id needed). The two
child borrows (splitter, then frame) are **sequential** — the marks are cloned out
between them, so only one `&mut` child is live at a time (D3-safe, no aliasing).
`frame_junction_marks` itself is `&self` on `Splitter`; the window simply holds the
`&mut Splitter` transiently to call it. Marks come from layout state (divider
positions), valid before drawing, so computing them at the top of `draw` is always
current. (This reuses the exact parent→child channel Window already uses to push
the zoom flag to its Frame — `window.rs:357-365`.)

### 5. `Splitter` — downcast keystone + frame marks + interior crossings (`src/widgets/splitter/mod.rs`)

- **Keystone — make `Splitter` downcastable** (required by Components 4 and by the
  interior crossings below). Add `as_any_mut` to the delegate skip list and
  override it, exactly like `Frame` (`frame.rs:450-452`):
  ```rust
  #[crate::delegate(to = group, skip(as_any_mut))]
  impl View for Splitter { /* existing overrides … */
      fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> { Some(self) }
  }
  ```
  This is **not** a new `View` method (`as_any_mut` already exists / is in
  `specs.rs`), so the `delegate_view` spy test stays green. Without it,
  `splitter.as_any_mut()` forwards to the inner `Group` and returns `None`, and
  every downcast in this design fails.
- `pub(crate) fn frame_junction_marks(&self, frame_bounds: Rect) -> Vec<JunctionMark>`:
  for each of this splitter's dividers, if its end abuts the given frame edge,
  emit a mark (edge, frame-local offset, this divider's weight). **Recurses into
  pane sub-splitters** (reached by `child_mut → as_any_mut → downcast_mut::<Splitter>`),
  translating coordinates into frame-local space (see "Coordinates" — the frame
  interior starts at `(1,1)`, so a child's parent-local position is offset by the
  splitter's own origin and then by the frame inset). A `Hidden`/`Locked` divider
  that drew nothing emits no mark. Pure function of layout — no drawing,
  unit-testable. (It is `&self`; recursion borrows each pane child `&mut`
  transiently only to read — the read returns owned data, so borrows don't overlap.)
- **Interior crossings — composed in `draw(&mut self)`, not the `&self`
  `draw_dividers`.** After `self.group.draw(ctx)` + `self.draw_dividers(ctx)`, run
  `self.draw_interior_crossings(ctx)` (which is `&mut self`): for each adjacent pane
  that `downcast_mut::<Splitter>()` succeeds on, read its perpendicular divider
  positions (owned `Vec`, borrow released), then overlay the correct tee/cross
  (`divider_junction`) on *this* splitter's own divider cells at the meeting
  positions (e.g. an inner `rows` splitter on the right of an outer vertical divider
  → `├` at the inner divider's row). It draws only on this splitter's own cells, via
  `ctx`. (The `&self draw_dividers` cannot do this — reading a child needs `&mut`
  access, and `Group` exposes only `&mut` child accessors; hence the move to
  `draw`.) Children are laid out (`resolve_layout_local`) before the parent draws,
  so the child divider positions are valid — the only hazard is the access path,
  resolved by using `draw(&mut self)` + `child_mut`.

### 6. Example (`examples/splitter.rs`)

Rework the demo into the grid — `Splitter::cols([tree, Splitter::rows([list,
form])])` sized into the interior of a `Window::…with_joined_lines()`. Must build
and run; shows `┬ ┴ ┤` against the frame and the interior `├`.

## Data flow

```
Window::draw (joined_lines):
  interior_splitter.frame_junction_marks(frame_bounds)   // recursive, layout-only
      → [JunctionMark{edge, offset, stem}, …]
  frame_child.set_junction_marks(marks)                  // owner-data-down
  group.draw:
     Frame::draw   → composes border, substituting frame_junction(...) at marks
     Splitter::draw→ panes + dividers; outer splitter overlays divider_junction(...)
  renderer diff/flush (unchanged)
```

All coordinates are **owner-local** (frame-local marks; splitter-local crossings),
consistent with the downward `DrawCtx` convention — no absolute/screen coords, no
read-back. Concretely, a divider at splitter-local axis position `dx` maps to a
frame-edge offset of `splitter.origin - frame.origin + dx`; for a top-level
splitter filling a window interior that is `1 + dx` (the frame inset is one cell).
A nested sub-splitter adds its own parent-local origin first. `frame_bounds` is
passed in so the splitter never assumes the inset — it computes the offset from the
actual frame rect.

## Weight handling

The junction glyph is a function of (frame weight, divider weight): passive single
frame + single divider → `┬`; active double frame + single divider → `╤`; double
frame + double (reconfig) divider → `╦`; the rare passive+double → `╥`. The window
passes each divider's current weight in the mark, and the frame knows its own
weight — so the correct (possibly mixed) glyph is chosen with no view needing the
other's focus state.

## Edge cases

- **Hidden/Locked divider:** draws no line, emits no mark → frame edge unchanged.
- **Reconfig mode** (divider drawn double): the mark carries `Double`, so the
  frame joins with `╦/╩` (active) or `╥/╨` (passive) — still correct.
- **Splitter inset from the frame** (a margin): the divider end does not abut the
  frame edge, so no mark is emitted → nothing joins (correct).
- **Window not containing a splitter:** `interior_splitter_mut()` is `None` (no
  child downcasts to `Splitter`) → no marks → unchanged.

## Testing (D11)

- **Pure unit tests** for `frame_junction` / `divider_junction`: every (edge, bar,
  stem) and crossing combination → expected `Glyphs` field.
- **`Splitter::frame_junction_marks`** unit tests: a 2-pane cols splitter abutting
  a frame yields the two correct top/bottom marks; a nested grid yields the
  expected set including the inner divider's right-edge mark.
- **Snapshot tests** (HeadlessBackend) on a small `Window::with_joined_lines`:
  passive single frame → `┬…┴`; active double frame → `╤…╧`; the grid → interior
  `├` + `┤` to the right frame.
- **Regression:** an existing window snapshot WITHOUT the flag is unchanged.
  **No new `View` trait method** is added — the only delegate change is
  `Splitter` adding `skip(as_any_mut)` + a one-line `as_any_mut` override (the
  `Frame` precedent), and since `as_any_mut` is already declared in
  `tvision-macros/src/specs.rs`, the `delegate_view` spy test stays green with no
  `specs.rs` edit. A focused test should assert `(&mut splitter as &mut dyn
  View).as_any_mut().and_then(|a| a.downcast_mut::<Splitter>()).is_some()` so the
  keystone override can't silently regress.

## Alternatives considered

- **Whole-buffer read-back pass (v1 of this spec).** A renderer/window post-pass
  that reads painted cells (`DrawCtx::get_char`) and upgrades line stubs. Rejected
  by the TV-mindset review: it re-introduces the screen inspection D8 deliberately
  removed (`drawUnderView`/per-view back buffers), widens `DrawCtx`'s write-only
  contract, and does cross-view work in `draw` with ad-hoc rules. The mask
  composition above is how `frameLine` already thinks and needs no read-back.
- **`View::line_join_cells()` on the universal trait.** Rejected: a presentation
  concern leaking into a structural/lifecycle trait; the producers are concrete
  types reachable by the existing `as_any` downcast (the window→frame precedent).
- **Global whole-screen auto-join.** Rejected: merges unrelated overlapping window
  frames.

## Future (not in this spec)

- Make dividers **track the frame weight** (single passive / double active) for a
  fully unified look — needs the window's active state to reach the splitter;
  deferred because mixed glyphs already read seamlessly.
- **Auto-enable** joining when a `Splitter` is detected as a window's body (drop
  the explicit flag) — only if the flag proves annoying.
- A `Role::Splitter*` theme entry set if dividers should be themable independently
  of the frame roles (today they reuse `FramePassive`/`FrameDragging`).
- Reviving the **full** `frameLine` `ofFramed`-sibling tee-walk generally (any
  framed sub-view, not just splitters) — a larger, separate effort; this spec
  deliberately scopes to splitter dividers.

## Methodology note

The divider→frame join **restores a genuine Turbo Vision behavior** (`frameLine`'s
tee-walk) that rstv shelved under D3, re-expressed D3-legally as owner-data-down —
so a tvision veteran recognizes it on sight. The divider→divider interior cross is
an rstv-original extension (TV has no splitter). The whole feature is **gated and
additive** — nothing changes unless a window opts in — so it carries no risk to the
faithful baseline.
