# Focus-aware content surface: a focused widget among siblings should *look* focused

**Date:** 2026-07-03
**Status:** draft (design, for discussion — authored from the edaptor consumer's
shuttle; framework direction + implementation owned by tvision-rs)
**Builds on / relates to:**
- [`2026-07-01-active-aware-surfaces-design.md`](2026-07-01-active-aware-surfaces-design.md)
  — the two-axis model (`owner_active` recede × `state.focused` highlight), v0.6.0.
- [`2026-07-03-inputline-self-focus-surface-design.md`](2026-07-03-inputline-self-focus-surface-design.md)
  — the **one-off** `InputLine::with_self_focus_surface` opt-in + `Role::InputSurface`, v0.8.0.

This spec asks whether that one-off should become a **general rule**, because the
same need has now appeared a second time (in `ListViewer`), and will appear again.

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

The two signals **diverge the moment a group holds two or more focusable content
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
the identical thing. Adding a second bespoke opt-in works, but two instances of
"the same fix, one tier down" is the signal to ask whether this is a per-widget
feature or a **framework rule that a few widgets happen to have discovered
piecemeal**.

The observation that motivates the general framing: nothing about `InputLine` or
`ListViewer` is special. The Outline would need the very same treatment the day it
appears twice in one focusable group. The distinguishing factor is **layout
(one-of-many focusable), not widget type.**

## The model — three surfaces from the two existing axes

The two axes already exist; the surface just needs all three of their meaningful
combinations. For any focusable content widget:

| pane active? | I'm focused? | surface | meaning |
|--------------|--------------|---------|---------|
| no (`!owner_active`) | — | **Inactive** | pane receded |
| yes | yes (`state.focused`) | **Normal** | I'm the focused widget |
| yes | no | **Surface** | active pane, a *sibling* is focused |

The middle row is the one `owner_active`-only cannot name. `InputLine` already
realises this exact table (`InputInactive` / `InputNormal` / `InputSurface`); the
proposal is to make the table the shared shape for the family
(`ListViewer`/`ListBox`/`SortedListBox`/`FileList`/`DirListBox`/`HistoryViewer`,
`Outline`, `InputLine`) via a role **triple** per widget:

```
ListNormal   / ListSurface   / ListInactive
OutlineNormal/ OutlineSurface/ OutlineInactive
InputNormal  / InputSurface  / InputInactive   (already exists)
```

Each widget's surface selection collapses to one shared rule:

```rust
let surface = if !ctx.owner_active() {
    roles.inactive
} else if self.state.state.focused {
    roles.normal
} else {
    roles.surface
};
```

(The per-item highlight axis — `state.focused` picking `Focused` vs `Selected`
for the current row — is unchanged; this is only about the row/background
*surface*.)

## The decision this spec exists to make: default-on vs opt-in

Both are pixel-frozen under `classic_blue` (all three roles in a triple resolve to
the same colour, exactly as `InputSurface == InputNormal` does today). The
difference is **semantics and ergonomics**, and it is the framework author's call.

**Option A — make the three-surface rule the default for focusable content
widgets.** No per-widget flag. `InputLine::self_focus` and its opt-in method are
retired (or kept as a deprecated no-op). "A focused widget among siblings looks
focused" becomes a framework property.
- *Pro:* the capability stops being a special case; no opt-in accretes; `ListViewer`
  and the shuttle get it for free; single-focusable panes are provably unaffected
  (their sole widget is focused iff its pane is, so they only ever hit Normal or
  Inactive — never Surface).
- *Con:* it flips the *default* semantic from "uniform pane" (classic TV: every
  field/row in a focused pane bright, cursor marks the current) to
  "focused-sibling-distinguished". Zero visual change under `classic_blue`, but a
  theme that distinguishes the roles now dims unfocused siblings **everywhere** by
  default, not only where a consumer opted in. This is a real deviation from C++
  TV's per-window uniformity — it should be a deliberate, documented choice.

**Option B — keep it opt-in, but share the mechanism.** One helper + role-triple
convention; each widget exposes `with_self_focus_surface`. `ListViewer` gains it
now (fixes the shuttle); `Outline` gains it if/when needed.
- *Pro:* conservative; defaults match C++ TV uniformity; no behavior flip.
- *Con:* the opt-in keeps accreting one widget at a time; the "why does only this
  widget have it" smell persists (just better-organised).

**Recommendation for discussion:** if the framework is willing to own "a focused
control among siblings shows it" as a *rule*, **A** is the honest design and
removes the drift. If C++-TV uniformity-by-default is a hard constraint, **B** with
a shared helper is the disciplined version of the status quo. Either way the
`InputLine`-specific mechanism should fold into the shared one rather than standing
alone.

## classic_blue stays frozen

Every role triple collapses to its `Normal` colour in `classic_blue` (as
`InputSurface` already does), so **every `.snap` is unchanged** under either
option. New coverage is a themed fixture per widget asserting the three-way
selection, plus (Option A) a fixture proving a single-focusable pane never selects
`Surface`.

## Deviation note (PORTING-GUIDE)

Extends the existing active-surfaces deviation. C++ `TListViewer`/`TInputLine` have
neither `owner_active` nor a third "unfocused sibling in an active pane" surface;
their focus model is per-window and uniform within a window. rstv's focusable
content widgets support a three-surface model (`owner_active` recede ×
`state.focused` well-vs-surface). Under Option A this is the default (documented as
a deliberate deviation from per-window uniformity); under Option B it is opt-in.

## Tech stack & global constraints

Rust workspace (`tvision-rs` + `tvision-rs-macros`), `insta` snapshots on
`HeadlessBackend`.

- `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target`; build/test on **≤ 4
  cores**.
- Gate: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo fmt --all --check`.
- **Zero pixel change under `classic_blue`** (all triples collapse). Any `.snap`
  movement is a bug.

## Test plan

- **Shared rule (themed, three distinct colours), per widget family:**
  - `owner_active = false` → `*Inactive` regardless of `state.focused`.
  - `owner_active = true`, `state.focused = true` → `*Normal`.
  - `owner_active = true`, `state.focused = false` → `*Surface`.
  (Mirrors `input_line.rs`'s existing `fill_bg` helper, which already drives
  `owner_active` independently of `state.focused`.)
- **Single-focusable pane never hits Surface** (Option A): a lone list/outline in a
  focused group is its group's current child → `state.focused` true → `Normal`;
  when the pane loses focus, both `owner_active` false and `state.focused` false →
  `Inactive`. Assert `Surface` is never selected.
- **classic_blue frozen:** a themed opt-in/default widget renders byte-identically
  to today — a `.snap` fixture per widget.

## Downstream payoff (edaptor — context, not part of this plan)

The two-list shuttle enables the behavior on both lists (Option A: for free;
Option B: `with_self_focus_surface(true)` per list) and maps `ListSurface` → the
pane surface, `ListInactive` → the receded tone. The focused list is then bright,
its sibling dim, and both dim when a dialog button takes focus — surface finally
agreeing with the per-item highlight that already worked. This unblocks the edaptor
shuttle dimming (issue tracked in edaptor's
`2026-07-03-tvision-0.7-highlighting-simplification-design.md` follow-up). If
Option A lands, edaptor's form pane also drops the `InputLine` opt-in call (it
becomes the default).

## Risks / watch-list

- **Option A flips a default.** Visual-frozen under `classic_blue`, but any
  downstream custom theme that already restyled `*Inactive`/`*Surface` would see
  unfocused siblings dim more widely. Audit bundled themes before flipping.
- **`Outline` inclusion.** Adding the triple to `Outline` is cheap and keeps the
  family uniform, but is only exercised once a consumer puts two outlines in one
  focusable group. Include the role + rule; leave the fixture minimal.
- **Naming.** `*Surface` as the middle role reads well next to `*Normal`/`*Inactive`
  and matches the existing `InputSurface`; keep it consistent across the family.
