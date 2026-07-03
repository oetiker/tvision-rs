# Group focus-aware surface: let a composite pane paint (and recede) its own background

**Date:** 2026-07-03
**Status:** reviewed (design approved 2026-07-03, ready for planning)
**Builds on:** [`2026-07-01-active-aware-surfaces-design.md`](2026-07-01-active-aware-surfaces-design.md)
(the two-axis `owner_active` model, shipped in v0.6.0). This is the missing third
piece of that story: the *owning group's own* surface.

---

## Problem

The active-aware-surfaces work (v0.6.0) gave every **content** widget a clean way to
recede with its pane: a child reads `ctx.owner_active()` (the owning group's focus)
for its surface and its own `state.focused` for its highlight. That covers the
widgets a pane is *made of* — lists, outlines, input wells.

It does **not** cover the pane's **own background**. A `Group` deliberately
"does not fill its own area; the children cover it" (`group.rs:986`). But a composite
pane is rarely fully tiled by children: the area below the last list row, the gap
around a form's fields, the margin a splitter allots — those cells belong to no child.
A pane that wants a continuous surface that also **recedes when the pane is
unfocused** has no framework support, so the consumer fills it by hand.

Concretely, edaptor's three browser panes each carry the same boilerplate: in their
own `draw` they `ctx.fill(...)` the whole extent with a role chosen from
`self.group.state().state.focused`:

```rust
// edaptor TreePane::draw / FormPane::draw — repeated per pane
let role = if self.group.state().state.focused {
    Role::ListNormal
} else {
    Role::ListInactive
};
ctx.fill(Rect::new(0, 0, size.x, size.y), ' ', ctx.style(role));
```

This is correct but is hand-rolled per pane, and it is the one bit of focus-driven
drawing the v0.6.0 model still forces onto the consumer. There is no `Panel` /
`Background` surface widget and no group/pane surface role — a `grep` of
`src/view/`, `src/widgets/`, and `src/theme.rs` confirms the gap.

(A `Background`-like child widget inserted at index 0 was considered and rejected:
it would need its bounds kept in sync on every group resize, whereas a group-level
fill reads `self.st.size` at draw time for free.)

## The model — a group's own surface keys on its own `focused`

The two-axis model already contains the answer. `Group::draw` computes exactly the
bit we need and fans it to children:

```rust
let owner_active = self.st.state.focused;   // group.rs:996 — "is *this* pane focused?"
```

That same `self.st.state.focused` is the correct axis for the group's **own**
background: a pane's chrome should be bright when the pane is the active one and
receded otherwise. (`focused` fans only down the current-child chain, so a group is
`focused` iff its whole ancestor chain is on the focused path — it *is* "this pane is
active"; the v0.6.0 spec establishes this.)

So the feature is symmetric with `owner_active`, not a new concept:

| draws | axis | signal |
|-------|------|--------|
| a child's surface | owner-active | `ctx.owner_active()` (fanned from the owner's `focused`) |
| **the group's own surface** | **own focus** | **`self.st.state.focused`** (read directly) |

In fact the two rows read the **same variable**: the value the surface keys on is
literally the `owner_active` local that `Group::draw` already computes and hands its
children. So the pane's background and its children's content surfaces are guaranteed
to agree — they recede together off one signal. The implementation computes
`owner_active` once at the top of `draw` and uses it for both the fill and the fan-out.

A group never needs `ctx.owner_active()` for its *own* fill — at its own draw level
`ctx.owner_active()` is the *parent's* focus (the window, effectively always active),
which is why the consumer cannot express pane-recede through it and must read
`self.st.state.focused`. This feature moves that one read into the framework.

## The API — opt-in surface pair on `Group`

A group paints no background by default (unchanged). A consumer opts in by naming the
two roles the surface resolves to:

```rust
impl Group {
    /// Paint the group's own extent as a background surface before drawing
    /// children: `normal` when the group is focused (its pane is active),
    /// `inactive` when not. Children paint over it (painter's algorithm), so a
    /// fully-tiled group looks unchanged — only the cells no child covers show
    /// the surface. Opt-in: a group with no surface set fills nothing (the
    /// default, faithful to `TGroup`).
    pub fn set_surface(&mut self, normal: Role, inactive: Role);

    /// Remove a previously set surface (revert to "children cover it").
    pub fn clear_surface(&mut self);
}
```

Stored as `surface: Option<(Role, Role)>` on `Group` (default `None`). The roles are
**consumer-supplied** on purpose: a pane picks the roles that match its content
(edaptor's panes use `ListNormal` / `ListInactive` so the pane background is the same
surface as the list rows it contains). Consumer-supplied roles mean **no new `Role`
variant** and therefore **no `classic_blue` change**.

### Draw

`Group::draw` (`group.rs:995`) fills its own extent first, keyed on its own focus,
then draws children exactly as today:

```rust
fn draw(&mut self, ctx: &mut DrawCtx) {
    let owner_active = self.st.state.focused; // one signal for fill AND fan-out
    if let Some((normal, inactive)) = self.surface {
        let role = if owner_active { normal } else { inactive };
        let style = ctx.style(role);
        let ext = Rect::new(0, 0, self.st.size.x, self.st.size.y);
        ctx.fill(ext, ' ', style);
    }
    for child in self.children.iter_mut() {
        // ...unchanged: sub-context, set_owner_active(owner_active), draw, shadow...
    }
}
```

The group's draw context is already clipped to the group's bounds, so
`Rect::new(0, 0, size.x, size.y)` is the whole pane in local coordinates (the same
extent edaptor fills today). Fill happens *before* children so they overpaint it,
preserving the back-to-front painter's algorithm and the existing shadow handling.

## classic_blue stays frozen

No existing `Group` calls `set_surface`, so every `Group` still paints no background
and **every `.snap` is unchanged**. The feature is purely additive and opt-in; there
is no role rename and no colour-table edit. A new snapshot fixture (below) is the only
added coverage.

## Tech stack & global constraints

Rust workspace (`tvision-rs` + `tvision-rs-macros`), `insta` snapshots on
`HeadlessBackend`.

- `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target`; build/test on **≤ 4
  cores** (`CARGO_BUILD_JOBS=4`, `-- --test-threads=4`).
- Gate: `cargo test --workspace -j4 -- --test-threads=4`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`.
- **Zero pixel change under `classic_blue`.** The feature is opt-in; any `.snap`
  movement means a group started painting when it should not — a bug, not a re-bless.
- Faithful-by-default: `TGroup` paints no background (that is `TBackground` / a window
  frame's job). A self-painting group is a documented rstv convenience deviation —
  note it in the `Group` / `set_surface` rustdoc, alongside the existing `owner_active`
  deviation note.
- **Nested-group semantics — document in the `set_surface` rustdoc:** a sub-group
  with a surface recedes whenever *it* is off the focus chain, even while its
  enclosing pane is focused. This is coherent (its own children's `owner_active`
  recedes identically — background and content always agree), but a consumer might
  expect pane-level behavior, so state it explicitly as intentional.
- Roll `CHANGELOG.md` `### New`: `Group::set_surface` / `clear_surface` — opt-in
  focus-aware background surface for composite panes.
- Commit trailer: the project's `Co-Authored-By` trailer for the implementing model.

## Build order (fills out in writing-plans)

1. **State + API**: add `surface: Option<(Role, Role)>` to `Group` (default `None` in
   `Group::new`), `set_surface` / `clear_surface`. No draw change yet.
2. **Draw**: fill own extent keyed on `self.st.state.focused` before the child loop
   (code above). Update the `group.rs:986` doc comment ("does not fill its own area")
   to describe the opt-in surface.
3. **Test**: a snapshot fixture — a `Group` with `set_surface(A, B)` holding a couple
   of partial-cover children, **inset in a parent at a non-zero origin** (never only
   at `(0,0)` — the local-extent fill must be exercised through a real sub-context);
   assert the uncovered cells render `A` when the group is `focused` and `B` when
   not, and children still overpaint. Add a unit test that a group with no surface
   fills nothing (default path unchanged).
4. **Verify** every existing `.snap` is byte-identical (opt-in ⇒ pixel-neutral).
5. CHANGELOG + rustdoc (the deviation note; a one-line "composite pane surface"
   example).

## Downstream payoff (edaptor — context, not part of this plan)

This completes "the consumer stops compensating" for the pane background. edaptor's
`TreePane` and `FormPane` delete their manual `ctx.fill(... focused ? Normal :
Inactive ...)` blocks and instead call `group.set_surface(Role::ListNormal,
Role::ListInactive)` once at construction. Combined with v0.6.0 (`owner_active` for
content surfaces, `state.focused` for highlights), edaptor's panes end up with **no
hand-rolled focus-driven drawing at all** — every recede/brighten is framework-driven.

## Risks / watch-list

- **A group that fills when it should not**: any `.snap` movement under `classic_blue`
  means a `Group` gained a surface it should not have, or the default is not `None`.
  Bug, not a bless.
- **Fill vs. shadow ordering**: the surface must fill *before* children so shadows and
  higher siblings overpaint it correctly; putting it after would erase children.
- **Local extent**: fill `(0,0)–(size.x,size.y)` in the group's *own* (already-clipped)
  context, not owner coordinates.
- **Not `owner_active`**: the group's own surface keys on `self.st.state.focused`, not
  `ctx.owner_active()` (which at this level is the parent's focus). Using `owner_active`
  here would make a pane fail to recede — the exact bug this feature removes.
- **Diff/redraw**: the surface is pure draw-time colour; whole-tree redraw + diff is
  unaffected.
