# InputLine self-focus surface: let a field be the "well" only when it is the focused one

**Date:** 2026-07-03
**Status:** shipped in v0.8.0 — **superseded by
[`2026-07-03-focusable-content-surface-generalization-design.md`](2026-07-03-focusable-content-surface-generalization-design.md)**,
which makes this behavior the framework default and removes the opt-in
(`set_self_focus_surface` / `with_self_focus_surface`) in 0.9.0.
Originally authored by the edaptor consumer as an upstream request;
implementation + release owned by tvision-rs.
**Builds on:** [`2026-07-01-active-aware-surfaces-design.md`](2026-07-01-active-aware-surfaces-design.md)
(the two-axis `owner_active` model, v0.6.0) and
[`2026-07-03-group-focus-aware-surface-design.md`](2026-07-03-group-focus-aware-surface-design.md)
(`Group::set_surface`, v0.7.0). This is the one remaining axis those left on the
table for `InputLine`: distinguishing *the focused field* from its siblings
**within an active pane**.

---

## Problem

v0.6.0 made `InputLine` key its surface purely on `ctx.owner_active()`
(`input_line.rs:724`):

```rust
let color = ctx.style(if ctx.owner_active() {
    Role::InputNormal      // active pane  → every field is the bright "well"
} else {
    Role::InputInactive    // inactive pane → every field recedes
});
```

That was the right fix for a classic Turbo Vision dialog, where all fields look
alike and the cursor marks the current one. But it collapses two questions the
active-aware spec had split apart back into one, for the field surface:

1. **Is this field the focused one?** (well vs plain)
2. **Is this field's pane active?** (bright vs receded)

A consumer that wants a **single-well** form — only the *focused* field is the
bright input well, the others sit flush on the pane surface, and the whole set
recedes when the pane is inactive — cannot express it. There are **three** target
surfaces, and `InputLine` offers a two-way choice keyed on the wrong axis.

This is a real, current need. edaptor's entry-form pane deliberately renders a
single well (the WIP that shipped its form rework describes "the selected field's
value is the one bright 'type here' well; non-selected fields carry no special
background"). On v0.5 it got this for free because `InputLine` keyed on its own
`state.focused`; v0.6 removed that, so every field in a focused form is now a well.
edaptor has no way back without either abandoning the design or hand-painting every
non-focused cell.

## The three surfaces

| field state | meaning | edaptor tone |
|-------------|---------|--------------|
| focused, pane active | the input well | `InputNormal` (near-white) |
| **not focused, pane active** | **plain pane surface** | **base3 (bright)** |
| any field, pane inactive | receded | desktop (dim) |

The middle row is the one the current two-role model cannot name: an active pane's
non-focused field. It is neither the well nor "receded".

## The model — opt-in `self_focus` surface, three roles

`InputLine` gains an opt-in. **Default off ⇒ today's v0.6/v0.7 behavior, unchanged.**
When on, the field distinguishes its own focus *and* still recedes with its pane,
choosing among three roles:

```rust
// input_line.rs draw(), when self_focus is enabled:
let color = ctx.style(if !ctx.owner_active() {
    Role::InputInactive          // pane receded → dim, regardless of field focus
} else if self.state.state.focused {
    Role::InputNormal            // the one focused field → the well
} else {
    Role::InputSurface           // active pane, non-focused field → pane surface
});
```

`Role::InputSurface` is a **new role** for the middle state. In `classic_blue` it
resolves identically to `InputNormal` (see below), so nothing moves unthemed; a
theme that wants a single-well look points `InputSurface` at its pane-surface colour
(edaptor: base3) and `InputInactive` at its receded colour (edaptor: desktop).

Default (opt-in off) keeps the exact current two-role branch — no `InputSurface`,
no behavior change.

### Why a third role, not "just key on own focus"

A two-role opt-in (`owner_active ? …` → `state.focused ? …`) would make a
non-focused field in an *active* pane resolve to `InputInactive` — i.e. read as
"receded" while its pane is focused. That re-creates exactly the axis-conflation
the v0.6 active-aware spec set out to remove (a control that recedes for the wrong
reason). The third role keeps the two axes orthogonal: `owner_active` still owns
"receded", `state.focused` only picks well-vs-surface *within* an active pane.

A consumer that does not want the third distinction never enables the opt-in and is
unaffected.

## The API — opt-in on `InputLine`

```rust
impl InputLine {
    /// Opt into a **self-focus surface**: the field paints the input well
    /// (`Role::InputNormal`) only when it *itself* holds focus; a non-focused
    /// field in an active pane uses `Role::InputSurface`, and any field in an
    /// inactive pane recedes to `Role::InputInactive`. Off by default, in which
    /// case the field keys its surface on `owner_active` alone (every field in
    /// an active pane is the well — the classic Turbo Vision look, unchanged).
    ///
    /// Use this for a "single-well" form where only the current field is the
    /// bright input target and the others sit flush on the pane surface.
    pub fn set_self_focus_surface(&mut self, on: bool);

    /// Builder form of [`set_self_focus_surface`].
    pub fn with_self_focus_surface(mut self, on: bool) -> Self;
}
```

Stored as `self_focus: bool` on `InputLine` (default `false`).

## classic_blue stays frozen

`InputSurface` is wired in `classic_blue` to the **same** colour as `InputNormal`
(as `InputInactive` already is), so:

- Every existing `InputLine` (opt-in off) uses the unchanged two-role branch → no
  change.
- Even an opt-in field under `classic_blue` renders identically (all three roles
  resolve to the same white-on-blue), so **every existing `.snap` is unchanged**.

The only new coverage is a fixture on a **themed** palette (three distinct colours)
asserting the three-way selection.

## Deviation note (PORTING-GUIDE)

This extends the existing active-surfaces deviation (C++ `TInputLine::draw` has a
single `getColor(focused ? 2 : 1)` and no pane-recede). Document it alongside that
deviation: rstv's `InputLine` supports an opt-in three-surface model
(`owner_active` for recede × own `focused` for well-vs-surface); the C++ original
has neither `owner_active` nor a third surface role.

## Tech stack & global constraints

Rust workspace (`tvision-rs` + `tvision-rs-macros`), `insta` snapshots on
`HeadlessBackend`.

- `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target`; build/test on **≤ 4
  cores**.
- Gate: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo fmt --all --check`.
- **Zero pixel change under `classic_blue`.** The opt-in is additive and the new
  role resolves to `InputNormal` in `classic_blue`; any `.snap` movement is a bug.

## Test plan

- **Default unchanged (themed):** an `InputLine` with the opt-in *off*, drawn with
  `owner_active = true`, paints `InputNormal` whether or not `state.focused`; with
  `owner_active = false`, `InputInactive`. (Mirrors the existing `fill_bg` helper
  at `input_line.rs:2449`, which already drives `owner_active` independently of
  `state.focused`.)
- **Opt-in three-way (themed palette, three distinct colours):**
  - `owner_active = true`, `focused = true` → `InputNormal`
  - `owner_active = true`, `focused = false` → `InputSurface`
  - `owner_active = false`, `focused = {true,false}` → `InputInactive`
- **classic_blue frozen:** an opt-in field renders byte-identically to a default
  field (all three roles equal) — a `.snap` fixture.

## Downstream payoff (edaptor — context, not part of this plan)

edaptor's form pane enables `with_self_focus_surface(true)` on its value cells and
maps `InputSurface`→base3, `InputInactive`→desktop. It then gets the single-well
look back for free **and** can delete its manual `dim_value_cells` repaint pass (the
receded state is now the field's own `InputInactive`). Its `form_focus_visualization`
test (currently `#[ignore]`d against the two-role model) re-enables and passes. This
unblocks the edaptor `feat/shuttle-widget` 0.7 highlighting simplification (see the
edaptor spec `2026-07-03-tvision-0.7-highlighting-simplification-design.md`), which
is waiting on the published release carrying this opt-in.

## Risks / watch-list

- **A consumer that wanted two-role recede but flips the opt-in** would see
  non-focused active-pane fields jump to `InputSurface`. Mitigated by default-off +
  the explicit builder name.
- **Theme authors** must wire `InputSurface` (a new role) or it defaults to
  `InputNormal`; document it in the role list next to `InputNormal`/`InputInactive`.
