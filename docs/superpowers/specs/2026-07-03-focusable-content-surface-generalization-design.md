# Focus-aware content surface: a focused widget among siblings should *look* focused

**Date:** 2026-07-03
**Status:** **decided — Option A (default-on), ready for implementation planning.**
Originally drafted from the edaptor consumer's shuttle; decision + refinements
recorded 2026-07-03 by tvision-rs.
**Builds on / relates to:**
- [`2026-07-01-active-aware-surfaces-design.md`](2026-07-01-active-aware-surfaces-design.md)
  — the two-axis model (`owner_active` recede × `state.focused` highlight), v0.6.0.
- [`2026-07-03-inputline-self-focus-surface-design.md`](2026-07-03-inputline-self-focus-surface-design.md)
  — the **one-off** `InputLine::with_self_focus_surface` opt-in + `Role::InputSurface`,
  v0.8.0. **Superseded by this spec:** the opt-in is removed and its behavior
  becomes the framework default.

This spec turns that one-off into a **general rule**, because the same need
appeared a second time (in `ListViewer`), and will appear again.

---

## Problem

v0.6.0 gave every content widget a pane-recede surface keyed on
`ctx.owner_active()`. But `owner_active` is a **per-group** signal: `Group::draw`
fans one value — the group's own `focused` — to *all* its children uniformly. It
answers "is my pane focused", not "am *I* the focused widget".

That is exactly right for a widget that is the **only focusable content in its
pane**: the tree's `Outline`, the leaf's list. For them `owner_active` and the
widget's own `state.focused` are the same value, because the sole focusable child
is focused iff its pane is. `owner_active` works there by coincidence of layout.

The two signals **diverge the moment a group holds two or more focusable
widgets**, and only `state.focused` can then say *which* is focused:

- **Form pane** — many `InputLine`s in one scroll group. Solved in v0.8.0 by
  `InputLine::with_self_focus_surface` + a third role `InputSurface` (focused
  field = `InputNormal` well, unfocused sibling = `InputSurface`, inactive pane =
  `InputInactive`).
- **Two-list shuttle** — two `ListViewer`s in one group. *Not* solved: both lists
  share `owner_active`, so both surfaces stay bright while either is focused; only
  the per-item **highlight** (which already keys on the list's own `state.focused`)
  distinguishes them. Focusing the dialog's OK/Cancel button dims *both* lists —
  proving the highlight axis is per-widget and correct, and the surface axis is
  per-group and too coarse.

The v0.8.0 fix was correct but **local to `InputLine`**. `ListViewer` now needs
the identical thing. Two instances of "the same fix, one tier down" is the signal
that this is not a per-widget feature but a **framework rule that a few widgets
had discovered piecemeal**. Nothing about `InputLine` or `ListViewer` is special —
the Outline would need the very same treatment the day it appears twice in one
focusable group. The distinguishing factor is **layout (one-of-many focusable),
not widget type.**

## Decision: the three-surface rule is the default (Option A)

The draft posed default-on (A) vs shared-mechanism-opt-in (B). **A is adopted.**
The decisive argument, sharper than the draft's "honest design" framing:

**The theme is already the opt-in.** All three roles of every triple collapse to
one colour in `classic_blue`, so classic per-window uniformity is preserved by the
*default theme*, pixel-frozen — not by a widget flag. A theme author only ever
sees sibling-dimming if they deliberately wire `*Surface` to a distinct colour,
which is precisely them asking for it. A per-widget flag (Option B) would be a
second switch on the same intent: `Role::ListSurface` would exist in the theme
vocabulary yet be silently dead unless the consumer *also* flipped a builder —
a theme role that sometimes does nothing is a trap for theme authors, and it is
the same "why does only this widget have it" smell the draft diagnosed, relocated
into the theme. With A, the widget-side rule is uniform and invisible; visibility
lives in exactly one place, the palette.

Supporting points:
- Single-focusable panes are provably unaffected (their sole selectable child is
  focused iff its pane is — the focus chain makes `state.focused` and
  `owner_active` coincide — so they only ever hit Normal or Inactive, never
  Surface). A fixture asserts this.
- The rule only changes rendering (under a *themed* palette) in exactly the
  multi-focusable layouts the Problem section identifies as wrong today.
- Option B's opt-in would keep accreting one widget at a time.

**Consequences of A:**
- `InputLine::set_self_focus_surface` / `with_self_focus_surface` and the
  `self_focus` field are **removed outright** — not kept as deprecated no-ops. A
  flag whose `false` arm no longer does anything lies to the caller; a compile
  error plus a CHANGELOG migration note ("delete the call — this is now the
  default") is honest. Breaking, ships in the next minor (0.9.0); the only known
  consumer (edaptor) wants to delete the call anyway.
- The default semantic deviates from C++ TV's per-window uniformity — recorded as
  a deliberate deviation (see the PORTING-GUIDE note below), invisible under
  `classic_blue`.

## The model — three surfaces from the two existing axes

The two axes already exist; the surface just needs all three of their meaningful
combinations. For any focusable content widget:

| pane active? | I'm focused? | surface | meaning |
|--------------|--------------|---------|---------|
| no (`!owner_active`) | — | **Inactive** | pane receded |
| yes | yes (`state.focused`) | **Normal** | I'm the focused widget |
| yes | no | **Surface** | active pane, a *sibling* is focused |

The middle row is the one `owner_active`-only cannot name. `InputLine` already
realises this exact table (`InputInactive` / `InputNormal` / `InputSurface`); this
table becomes the shared shape for the family
(`ListViewer`/`ListBox`/`SortedListBox`/`FileList`/`DirListBox`/`HistoryViewer`,
`Outline`, `InputLine`) via a role **triple** per widget:

```
ListNormal   / ListSurface   / ListInactive
OutlineNormal/ OutlineSurface/ OutlineInactive
InputNormal  / InputSurface  / InputInactive   (already exists)
```

(The per-item highlight axis — `state.focused` picking `Focused` vs `Selected`
for the current row — is unchanged; this is only about the row/background
*surface*.)

### The non-selectable rule

`Surface` means "**a sibling won the focus contest**", not "I'm not focused". A
content widget that can never hold focus (`selectable == false` — e.g. a
read-only `ListViewer` used as a display) never *loses* a focus contest, so in an
active pane it paints **Normal**, not Surface. Without this rule a non-selectable
widget would never be Normal under any theme that distinguishes the roles — a
trap discovered only by the first themed consumer with a read-only list.

The shared rule, with all three inputs:

```rust
let surface = if !ctx.owner_active() {
    roles.inactive            // pane receded — regardless of focus/selectability
} else if self_focused || !selectable {
    roles.normal              // the focused widget, or one that can't compete
} else {
    roles.surface             // active pane, a selectable sibling holds focus
};
```

### What "sibling" means — the headline behavioural consequence

The sibling that wins focus is **any focusable view in the group, content or
not**: another list, another field, *or a button/checkbox/cluster*. Under a theme
that distinguishes the roles, a plain dialog with one `InputLine` and two buttons
dims the field to `InputSurface` while a button is focused. This is deliberate —
it is exactly the shuttle behavior ("focusing OK dims both lists"), generalized:
*the focused control shows it, everything else sits on the pane surface*. It is
the one place Option A changes ordinary-dialog rendering (themed only), and the
deviation note documents it explicitly.

## Mechanism — one shared helper, so the rule cannot drift

Three widgets currently hand-roll their surface branch (`input_line.rs`,
`list_viewer.rs`, `outline.rs`) — and each drifted, which is how this spec came
to exist. The deliverable is **one** implementation of the rule that every
content widget calls; no per-widget branches:

```rust
/// The role triple a focusable content widget paints its surface from.
pub struct SurfaceRoles {
    pub normal: Role,     // active pane, I'm focused (or I can't be)
    pub surface: Role,    // active pane, a selectable sibling is focused
    pub inactive: Role,   // pane receded
}

impl DrawCtx<'_> {
    /// Select the content surface per the three-surface rule
    /// (owner_active recede × self-focus well-vs-surface × selectability).
    pub fn content_surface(&self, roles: SurfaceRoles, self_focused: bool,
                           selectable: bool) -> Style { … }
}
```

Each widget owns a `SurfaceRoles` const next to its other role bindings (e.g. in
`ListViewer`'s palette block alongside `focused`/`selected`/`divider`) and its
`draw` calls `ctx.content_surface(…)`. Exact signatures are the implementation
plan's call; the spec pins the shape: **triple + one shared function, zero
hand-rolled selection branches.**

### Standing rule for future widgets (PORTING-GUIDE)

The PORTING-GUIDE deviation entry states the rule prospectively: **every future
focusable content widget** gets a `*Normal / *Surface / *Inactive` role triple
and selects its surface via `content_surface` **by construction**. Two widgets
rediscovering the rule was the signal; the helper plus the written rule is what
prevents a third. [Correction: `Editor`/`Memo`/`Terminal`/`Scroller` are not
"future" — they already exist, predate both surface axes, and still paint
`Role::ScrollerNormal` unconditionally. They are a named, deliberate exception
pending a `Scroller*` triple, not covered by this generalization.]

## classic_blue stays frozen

Every role triple collapses to its `Normal` colour in `classic_blue` (as
`InputSurface` already does — `ListSurface` and `OutlineSurface` are wired the
same way), so **every existing `.snap` is unchanged**. Any `.snap` movement is a
bug. New coverage is themed fixtures only (below).

## Deviation note (PORTING-GUIDE)

Extends the existing active-surfaces deviation. C++ `TListViewer`/`TInputLine`
have neither `owner_active` nor a third "unfocused sibling in an active pane"
surface; their focus model is per-window and uniform within a window. rstv's
focusable content widgets select among **three** surfaces by default
(`owner_active` recede × `state.focused` well-vs-surface × selectability), via
`DrawCtx::content_surface`. Two consequences to state explicitly:

- **Deliberate deviation from per-window uniformity**, invisible under
  `classic_blue` (all triples collapse); a theme opts into sibling distinction by
  giving `*Surface` its own colour — the theme is the opt-in.
- Under such a theme, **any focusable sibling** (including buttons) pushes
  content widgets to `*Surface`; non-selectable content widgets stay `*Normal`
  in an active pane (they never compete for focus).
- Standing rule: future focusable content widgets use the triple + helper by
  construction. [Correction: the `Scroller`-family widgets (`Editor`/`Memo`/
  `Terminal`/`Scroller`) already exist and are NOT converted by this change —
  they still paint `Role::ScrollerNormal` unconditionally; that's a named,
  deliberate follow-up pending a `Scroller*` triple, not covered here.]

## Tech stack & global constraints

Rust workspace (`tvision-rs` + `tvision-rs-macros`), `insta` snapshots on
`HeadlessBackend`.

- `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target`; build/test on **≤ 4
  cores**.
- Gate: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo fmt --all --check`.
- **Zero pixel change under `classic_blue`** (all triples collapse). Any `.snap`
  movement is a bug.
- Ships in **0.9.0** (breaking: `InputLine` opt-in removed). CHANGELOG carries
  the migration note.

## Test plan

- **Shared rule (themed, three distinct colours), per widget family:**
  - `owner_active = false` → `*Inactive` regardless of `state.focused`.
  - `owner_active = true`, `state.focused = true` → `*Normal`.
  - `owner_active = true`, `state.focused = false`, selectable → `*Surface`.
  (Mirrors `input_line.rs`'s existing `fill_bg` helper, which already drives
  `owner_active` independently of `state.focused`.)
- **Non-selectable rule (themed):** a read-only (`selectable = false`) list in an
  *active* pane paints `*Normal`, never `*Surface`; in an inactive pane,
  `*Inactive`.
- **Single-focusable pane never hits Surface:** a lone list/outline in a focused
  group is its group's current child → `state.focused` true → `Normal`; when the
  pane loses focus, both `owner_active` false and `state.focused` false →
  `Inactive`. Assert `Surface` is never selected.
- **The motivating shuttle fixture (themed snapshot):** one group holding two
  `ListViewer`s and a `Button`, snapshotted in three focus states — list A
  focused (A `Normal`, B `Surface`), button focused (both lists `Surface`), pane
  inactive (both `Inactive`). This is the scenario that justified the spec; it
  gets its own snapshot, not just per-widget unit assertions.
- **classic_blue frozen:** a themed-default widget renders byte-identically to
  today — a `.snap` fixture per widget; plus the full existing `.snap` suite
  unchanged.
- **Opt-in removal:** `InputLine`'s `self_focus` field and both methods are gone;
  the previously opted-in themed fixture now passes with no opt-in call.

## Downstream payoff (edaptor — context, not part of this plan)

The two-list shuttle gets the behavior for free: `ListSurface` → the pane
surface, `ListInactive` → the receded tone. The focused list is then bright, its
sibling dim, and both dim when a dialog button takes focus — surface finally
agreeing with the per-item highlight that already worked. This unblocks the
edaptor shuttle dimming (issue tracked in edaptor's
`2026-07-03-tvision-0.7-highlighting-simplification-design.md` follow-up).
edaptor's form pane **deletes** its `with_self_focus_surface(true)` calls on
upgrade to 0.9.0 (the behavior is the default; the method no longer exists).

## Risks / watch-list

- **Breaking removal of the v0.8.0 opt-in.** Deliberate (see Decision): a no-op
  flag would lie. Pre-1.0, next-minor bump, CHANGELOG migration note; only known
  consumer deletes the call anyway.
- **Themed consumers see wider dimming.** Any downstream theme that already
  restyled `*Inactive`/`*Surface` sees unfocused siblings dim wherever
  multi-focusable groups exist, plus content dimming while buttons hold focus.
  Audit bundled themes before release (expected: only `classic_blue`, which
  collapses).
- **`Outline` inclusion.** Adding the triple to `Outline` is cheap and keeps the
  family uniform, but is only exercised once a consumer puts two outlines in one
  focusable group. Include the role + rule; keep its fixture minimal.
- **Naming: decided.** `*Surface` is kept as the middle role — slightly odd in
  isolation ("surface" sounds like the pane, which is in fact the point), but
  consistency with the shipped `InputSurface` beats a better word. Uniform across
  the family.

## Considered and rejected: Option B (shared mechanism, per-widget opt-in)

Keep C++-TV uniformity as the default; each widget exposes
`with_self_focus_surface`, sharing the helper internally. Rejected because the
flag is structurally redundant with the theme (two switches on one intent), it
leaves `*Surface` roles silently dead unless a builder is flipped, and the opt-in
accretes one widget at a time — the organised version of the drift this spec
exists to stop. Since `classic_blue` collapses the triples, Option B's only
advantage (defaults match C++ uniformity) is already provided by the default
theme under Option A.
