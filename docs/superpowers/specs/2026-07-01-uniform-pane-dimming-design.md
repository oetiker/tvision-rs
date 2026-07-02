# Active-aware surfaces: two orthogonal axes (focus highlight + owner-active recede)

**Date:** 2026-07-01
**Status:** draft (design, ready for review)
**Replaces:** the deleted `2026-07-01-active-aware-content-surface.md` (the
`pane_active` state-flag draft) *and* the earlier revisions of this file (the
app-controlled `dim`-flag / colour-transform approach). Grounding the design in the
real edaptor form pane showed the answer is simpler and native: **two orthogonal
axes**, one of which the framework already provides and just needs to be applied
consistently.

---

## Problem

"Show which part of the UI is active" is being solved ad-hoc, and the recent commit
`52570f4` ("focus-aware surfaces for InputLine and Outline") **conflated two
different questions into one signal**, which is the confusion this design cleans up:

1. **Which single control/item is active?** (native TVision focus-highlight — one
   thing bright, rest muted.)
2. **Which whole *pane* is active?** (a sub-tree recedes when its pane isn't the
   focused one.)

Symptoms:

- **`ListViewer`** decides *both* its normal surface and its highlight from
  `st.selected && st.active` (`list_viewer.rs:995`, faithful to C++
  `sfSelected | sfActive`). In a nested layout `selected` is relative to the
  *immediate* owner and `active` fans window-wide, so a list that is current in an
  **unfocused** splitter/shuttle pane still reads "active" and stays bright.
- **`Outline`** (`outline.rs:771–780`) and **`InputLine`** key their *surface* on the
  widget's **own `focused`**. For a whole-pane widget that accidentally approximates
  "pane active"; for one control among several in a pane it wrongly recedes
  non-current controls even while the pane *is* focused.
- **`InputLine`** has **no** inactive surface at all, so the edaptor **form pane**
  can't recede its value wells with the pane. The form already knows it is inactive
  (`form.rs:624`, `focused = self.group.state().state.focused` — reliably correct),
  and pushes that bit down **three ad-hoc ways** (mirror onto `ScrollGroup.active`,
  `set_focused` on labels, a surface role) plus a **manual `dim_value_cells`
  repaint** (`form.rs:569–573`, `671–679`) purely because `InputLine` can't recede
  itself. That repaint is what "optically does not work."

None of this is a deep conceptual gap: the framework *does* provide the pane-active
bit (a group's own `focused`). It just isn't exposed to leaves (D3: a leaf can't see
its owner), and the two axes were merged onto one mis-chosen signal.

## The model — two orthogonal axes

| axis | question | driven by | shows |
|------|----------|-----------|-------|
| **focus** | "am I the active control/item?" | the view's own `state.focused` | bright highlight vs muted |
| **owner-active** | "is my owning pane the active pane?" | a draw-time signal = the owning group's `focused` | normal surface vs receded |

They compose. A shuttle whose Available list is focused, inside the focused pane:
Available = bright highlight + normal surface; Selected = muted highlight + normal
surface. Move focus to another splitter pane: **both** lists = muted highlight +
**receded** surface. A form field: focused field = bright well; siblings = normal
well; whole pane unfocused = every well receded.

`focused == true` implies owner-active (focus requires the whole owner chain to be
current), so the two axes never contradict — a leaf is never "focused" *and*
"owner-inactive."

## Piece 1 — focus highlight on `state.focused` (fixes the shuttle/splitter)

Make **`ListViewer`** choose its **highlight** from `state.focused`, joining
`Outline`/`InputLine` which already do:

- `list_viewer.rs:995` `let active = st.selected && st.active;` → `let active = st.focused;`

The focused-item colour (`ListFocused`) is used when the list is the focused control;
otherwise the current item shows in `ListSelected` (muted). Flat dialogs are
unchanged (there `focused` and `selected && active` agree); nested panes are fixed.
Documented deviation from C++ `sfSelected | sfActive` — justified by nested panes
(splitters/embedded multi-list widgets), which C++ has no concept of.

## Piece 2 — an `owner_active` draw signal + inactive surface roles

### The signal

Add `owner_active: bool` to `DrawCtx` (`context.rs:618`), default `true`, inherited
by `sub()` (`context.rs:910`). `Group::draw` (`group.rs:995`) sets it for each child
from **its own** focus:

```rust
let mut sub = ctx.sub(bounds);
sub.set_owner_active(self.st.state.focused);   // "is *this* group the focused pane?"
child.view.draw(&mut sub);
```

Because `focused` already fans only down the current-child chain, a group is
`focused` iff its whole ancestor chain is on the focused path — so
`self.st.state.focused` *is* "this pane is the active pane," and one line hands it to
the children. No new persistent state, no stickiness param, no invariant to
maintain. Expose `DrawCtx::owner_active() -> bool` so widgets (and edaptor's
`FieldLabel`) can branch surface/text on it.

### The surface roles (this is the cleanup)

Every content widget picks its **surface** from `ctx.owner_active()`, and its
**highlight** from its own `state.focused`. The role set is rationalized to exactly
one inactive-surface role per widget, keyed on the correct (owner-active) axis:

| widget | active-pane surface | inactive-pane surface | highlight (on own `focused`) |
|--------|--------------------|-----------------------|------------------------------|
| `ListViewer` | `ListNormal` (was `ListNormalActive`) | `ListInactive` (was `ListNormalInactive`) | `ListFocused` / `ListSelected` |
| `Outline` | `OutlineNormal` | `OutlineInactive` (was `OutlineNormalInactive`) | `OutlineFocused` / `OutlineSelected` |
| `InputLine` | `InputNormal` | `InputInactive` (**new**) | bright well + cursor + select-all |
| `StaticText` / `Label` | `StaticText` / `Label` | `StaticTextInactive` / `LabelInactive` (**new**) | — |

- **Delete `InputPassive`** — it was a *focus*-axis surface, but focus is shown by
  the well/cursor, not a background swap (it equalled `InputNormal` anyway). Its job
  is taken over by `InputInactive` on the correct axis.
- **Re-key `Outline`** (`outline.rs:776`) and `ListViewer` surfaces from the widget's
  own `focused`/`selected && active` to `ctx.owner_active()`.
- **Rename** `*NormalActive`/`*NormalInactive` → `*Normal`/`*Inactive` (the `Active`
  suffix is meaningless once the axis is explicit).

### classic_blue stays frozen

`classic_blue` maps every `*Inactive` role **==** its active counterpart (the DOS
palette never receded pane content), so with the axes re-keyed the rendered pixels
are identical: `list_viewer`/`outline`/`input` all resolve to today's colours.
**Every `.snap` stays frozen.** A modern theme (edaptor's) sets the `*Inactive` roles
to receded colours and gets whole-pane recede for free.

## Tech stack & global constraints

Rust workspace (`tvision-rs` + `tvision-rs-macros`), `insta` snapshots on
`HeadlessBackend`.

- `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target`; build/test on **≤ 4
  cores** (`CARGO_BUILD_JOBS=4`, `-- --test-threads=4`).
- Gate: `cargo test --workspace -j4 -- --test-threads=4`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`.
- **Zero pixel change under `classic_blue`.** Any `.snap` movement means an
  `*Inactive` role wasn't mapped identically — a bug, not a re-bless.
- Faithful-by-default: the `owner_active` axis + `ListViewer`'s `focused` keying are
  documented rstv deviations (C++ focus is per-window; no nested panes). Note them in
  the rustdoc.
- Roll `CHANGELOG.md` (`### New`: `owner_active` + `*Inactive` roles; `### Changed`:
  `ListViewer` highlight axis, Outline/Input re-keyed; `### Removed`: `InputPassive`).
- Commit trailer: `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.

## Build order (fills out in writing-plans)

1. **Piece 1**: `ListViewer` highlight → `state.focused`. Snapshot: a two-pane
   fixture where a list current in the *unfocused* pane is muted, bright in the
   focused pane. classic_blue flat-dialog snapshots unchanged.
2. **`owner_active` signal**: `DrawCtx.owner_active` + `sub()` inheritance +
   `Group::draw` threading + `DrawCtx::owner_active()`. Test the sub-context carries
   the owning group's `focused`.
3. **Role rationalization**: rename `*NormalActive`/`*NormalInactive` → `*Normal`/
   `*Inactive`; delete `InputPassive`; add `InputInactive`,
   `StaticTextInactive`, `LabelInactive`; map all `*Inactive == *Normal` in
   `classic_blue`.
4. **Re-key surfaces on `owner_active()`**: `ListViewer`, `Outline`, `InputLine`,
   `StaticText`, `Label`. Remove the misleading "focus-aware surface" logic/docs from
   `52570f4`.
5. **Verify** every existing `.snap` unchanged under `classic_blue` (all pixel-neutral).
6. CHANGELOG + rustdoc (two-axis model, the two deviation notes).

## Downstream payoff (edaptor — context, not part of this plan)

The form pane deletes: the `ScrollGroup.active = focused` mirror (`form.rs:631`),
`set_focused`/`set_active` plumbing on labels, the surface-role branch, and
`dim_value_cells` (`569–573`). Widgets read `ctx.owner_active()`; `InputInactive`
recedes the wells; `FieldLabel` branches its text on `ctx.owner_active()`. The tree
and leaf panes drop their equivalent mirrors. "The consumer stops compensating" is
the acceptance test that both pieces landed in the right place.

## Risks / watch-list

- **Snapshot drift under classic_blue** = an `*Inactive` role not mapped identically,
  or a surface still keyed on the old signal. Bug, not a bless.
- **Dangling references to removed/renamed roles**: grep `src/` + `docs/` for
  `InputPassive` / `OutlineNormalInactive` / `ListNormalActive` / `ListNormalInactive`
  so no intra-doc link or comment survives the rename.
- **`ListViewer` highlight change**: verify no flat-dialog snapshot moves (the axes
  agree there) — only nested-pane behaviour should change.
- **Seed of the root `owner_active`** (top-level draw) = `true`; an inactive
  application window recedes via its own `focused` at the next level, so the seed only
  matters if we later want the whole desktop to recede.
- **Diff/redraw**: surfaces are pure draw-time colour; whole-tree redraw + diff is
  unaffected.
```
