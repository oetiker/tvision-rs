# Group Focus-Aware Surface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `Group::set_surface(normal, inactive)` / `clear_surface()` — an opt-in
background fill of the group's own extent, keyed on the group's own `focused`
flag, painted before children so they overpaint it.

**Architecture:** One new field `surface: Option<(Role, Role)>` on `Group`
(default `None` = today's "paints nothing"). `Group::draw` already computes
`owner_active = self.st.state.focused` and fans it to children; the fill reuses
that same local, so the pane background and its children's content surfaces
recede off one signal. No new `Role`, no theme change, zero pixel movement under
`classic_blue`.

**Tech Stack:** Rust workspace (`tvision-rs` + `tvision-rs-macros`), `insta`
golden snapshots on `HeadlessBackend`.

**Spec:** `docs/superpowers/specs/2026-07-03-group-focus-aware-surface-design.md`

## Global Constraints

- `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target` before any cargo command.
- Max 4 cores: `CARGO_BUILD_JOBS=4`, test with `-- --test-threads=4`.
- Gates per task: `cargo test --workspace -j4 -- --test-threads=4`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`.
- **Zero pixel change under `classic_blue`:** no existing `.snap` file may change.
  Any modified (not added) file in `src/snapshots/` is a bug in the change, not a
  re-bless.
- English for all code/comments/identifiers.
- Work on branch `feat/group-surface` off up-to-date `main`
  (`git pull --ff-only` first; PRs #9/#10 are the precedent for this feature line).
- Commit trailer (every commit): `Co-Authored-By: Claude <your model name> <noreply@anthropic.com>`.

**Key existing code (read before Task 1):**
- `src/view/group.rs:111-157` — `Group` struct + `Group::new` (the struct literal to extend).
- `src/view/group.rs:983-1008` — `Group::draw` (the doc comment and method to change).
- `src/view/group.rs:1497-1560` — test harness: `with_ctx`, `Pass`; `Probe::boxed(rect, ch, log)`
  is a leaf that fills its extent with `ch` in `fg=Bios(0xF) bg=Bios(0x1)`.
- `src/view/group.rs:1783-1807` — `z_order_draw_topmost_wins_overlap_snapshot`, the
  render-to-`HeadlessBackend` snapshot pattern to copy.
- `src/view/context.rs:660` `DrawCtx::style(Role) -> Style`, `:827`
  `DrawCtx::fill(Rect, char, Style)`, `:670` `set_owner_active`.
- `src/theme.rs:1201/1206` — `Theme::style` / `Theme::set_style` (tests pin
  distinct styles because `classic_blue` may render a role pair identically).
- `src/screen/buffer.rs:108` — `Buffer::get(x, y) -> &Cell`; `src/screen/cell.rs:91`
  — `Cell::style() -> Style`.
- Snapshots live flat in `src/snapshots/` (e.g.
  `tvision_rs__view__group__tests__z_order_draw_topmost_wins_overlap_snapshot.snap`).

---

### Task 1: `surface` state, `set_surface`/`clear_surface`, draw fill

**Files:**
- Modify: `src/view/group.rs` (struct at :111, `Group::new` at :145, new methods
  in the `impl Group` block after `current()` ~:162, `draw` + its doc comment at
  :983-1008, imports at :54-60, unit test in `mod tests`)

**Interfaces:**
- Consumes: `DrawCtx::style(Role) -> Style`, `DrawCtx::fill(Rect, char, Style)`,
  `ViewState::get_extent() -> Rect` (all existing).
- Produces: `pub fn set_surface(&mut self, normal: Role, inactive: Role)` and
  `pub fn clear_surface(&mut self)` on `Group`; field
  `surface: Option<(Role, Role)>`. Task 2's fixture calls `set_surface`.

- [ ] **Step 1: Write the failing unit test**

Append to `mod tests` in `src/view/group.rs` (after the z-order snapshot test's
section). `use super::*` already brings in the Role import Step 3 adds; `Theme`,
`Style`, `Color`, `Buffer` are already imported by the test module.

```rust
    // -- opt-in focus-aware surface ------------------------------------------

    /// The surface fill keys on the group's OWN `focused` flag (`normal` when
    /// focused, `inactive` when not); a child overpaints the cells it covers;
    /// `clear_surface` reverts to "fills nothing". `classic_blue` may render a
    /// role pair identically, so the test pins two visibly distinct styles.
    #[test]
    fn surface_fill_keys_on_own_focus() {
        let mut theme = Theme::classic_blue();
        let normal = Style::new(Color::Bios(0x0), Color::Bios(0x3));
        let inactive = Style::new(Color::Bios(0x8), Color::Bios(0x0));
        theme.set_style(Role::ListNormal, normal);
        theme.set_style(Role::ListInactive, inactive);

        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        let mut group = Group::new(Rect::new(0, 0, 6, 3));
        group.set_surface(Role::ListNormal, Role::ListInactive);
        with_ctx(&mut out, &mut timers, |_ctx| {
            // one child covering only the top-left 2x1 corner
            group.insert(Probe::boxed(Rect::new(0, 0, 2, 1), 'C', log.clone()));
        });

        // Draw the group into a fresh buffer and report the cell at (x, y).
        let draw_cell = |group: &mut Group, focused: bool, x: u16, y: u16| {
            group.state_mut().state.focused = focused;
            let mut buf = Buffer::new(6, 3);
            let bounds = group.state().get_bounds();
            let mut dc = DrawCtx::new(&mut buf, &theme, bounds, bounds.a);
            group.draw(&mut dc);
            buf.get(x, y).clone()
        };

        // Uncovered cell: the surface tracks the group's own focus.
        assert_eq!(draw_cell(&mut group, true, 5, 2).style().bg, normal.bg);
        assert_eq!(draw_cell(&mut group, false, 5, 2).style().bg, inactive.bg);
        // Covered cell: the child overpaints the surface regardless of focus
        // (Probe fills with bg=Bios(0x1)).
        assert_eq!(
            draw_cell(&mut group, true, 0, 0).style().bg,
            Color::Bios(0x1)
        );
        // clear_surface: back to "fills nothing" — the uncovered cell is
        // identical to a never-touched buffer cell.
        group.clear_surface();
        let fresh = Buffer::new(6, 3);
        assert_eq!(
            &draw_cell(&mut group, true, 5, 2),
            fresh.get(5, 2),
            "cleared surface must fill nothing"
        );
    }
```

(If `Cell` does not implement `Clone`/`PartialEq`, compare
`draw_cell(...).style()` against `fresh.get(5, 2).style()` plus the glyph via the
snapshot string instead — but check `src/screen/cell.rs` first; `Buffer::diff`
compares cells, so equality almost certainly exists.)

- [ ] **Step 2: Run the test to verify it fails**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test -p tvision-rs -j4 surface_fill_keys_on_own_focus -- --test-threads=4
```

Expected: compile error — `no method named set_surface found for struct Group`.

- [ ] **Step 3: Implement — import, field, constructor, methods, draw**

3a. Imports (`src/view/group.rs:54-60`) — add:

```rust
use crate::theme::Role;
```

3b. Struct (`src/view/group.rs:111-123`) — add the field after `currency_dirty`:

```rust
    /// Opt-in background surface: `Some((normal, inactive))` fills the group's
    /// extent before children draw, keyed on the group's own `focused` flag.
    /// `None` (the default) — the group paints nothing; children cover it.
    surface: Option<(Role, Role)>,
```

3c. `Group::new` (`src/view/group.rs:151-156`) — extend the struct literal:

```rust
        Group {
            st,
            children: Vec::new(),
            current: None,
            currency_dirty: false,
            surface: None,
        }
```

3d. New methods — in the `impl Group` block, after `current()` (~line 162):

```rust
    /// Paint the group's own extent as a background surface before drawing
    /// children: `normal` when the group is focused (its pane is the active
    /// one), `inactive` when not.
    ///
    /// Children paint over the surface (painter's algorithm), so a fully-tiled
    /// group looks unchanged — only cells no child covers show it. Opt-in: a
    /// group with no surface set fills nothing (the default).
    ///
    /// The fill keys on the same signal the group fans to its children as
    /// [`DrawCtx::owner_active`], so the pane's background and its children's
    /// content surfaces recede together. Consequently a *nested* group with a
    /// surface recedes whenever **it** is off the focus chain, even while its
    /// enclosing pane is focused — intentional: its own children's
    /// `owner_active` recedes identically, so background and content always
    /// agree.
    ///
    /// Pick the role pair matching the pane's content, e.g.
    /// [`Role::ListNormal`] / [`Role::ListInactive`] for a pane built of lists:
    ///
    /// ```
    /// use tvision_rs::{Group, Rect, Role};
    ///
    /// let mut pane = Group::new(Rect::new(0, 0, 40, 10));
    /// pane.set_surface(Role::ListNormal, Role::ListInactive);
    /// ```
    ///
    /// # Turbo Vision heritage
    /// An tvision-rs convenience addition with no `TGroup` counterpart: in
    /// Turbo Vision a group never paints its own area (backgrounds are
    /// `TBackground` / frame territory). The faithful default is preserved —
    /// the surface is strictly opt-in.
    pub fn set_surface(&mut self, normal: Role, inactive: Role) {
        self.surface = Some((normal, inactive));
    }

    /// Remove a surface set by [`set_surface`](Self::set_surface): the group
    /// reverts to painting no background (the children cover it).
    pub fn clear_surface(&mut self) {
        self.surface = None;
    }
```

The doctest imports are verified: `Group`, `Rect` (`src/lib.rs:133-136`) and
`Role` (`src/lib.rs:126`) are all re-exported at the crate root. Doctests run
under `cargo test --workspace`.

3e. `draw` (`src/view/group.rs:995-1008`) — replace the body's opening and update
the doc comment. New doc comment (replacing the first paragraph only; keep the
shadow sentences and the heritage section as they are):

```rust
    /// Paint visible children **back-to-front** (`children[0]` →
    /// `children.last()`), each through a sub-context clipped to its bounds —
    /// painter's algorithm, so higher siblings overpaint lower ones. By default
    /// the group does not fill its own area — the children cover it; a group
    /// given a surface via [`Group::set_surface`] first fills its extent
    /// (`normal` role when focused, `inactive` when not), which the children
    /// then overpaint. After each child that
```

New method body:

```rust
    fn draw(&mut self, ctx: &mut DrawCtx) {
        // One signal for both the surface fill and the child fan-out: the
        // pane's background and its children's content surfaces recede
        // together (spec: 2026-07-03-group-focus-aware-surface-design.md).
        let owner_active = self.st.state.focused;
        if let Some((normal, inactive)) = self.surface {
            let role = if owner_active { normal } else { inactive };
            let style = ctx.style(role);
            ctx.fill(self.st.get_extent(), ' ', style);
        }
        for child in self.children.iter_mut() {
            if child.view.state().state.visible {
                let bounds = child.view.state().get_bounds();
                let mut sub = ctx.sub(bounds);
                sub.set_owner_active(owner_active);
                child.view.draw(&mut sub);
                if child.view.state().state.shadow {
                    ctx.cast_shadow(bounds);
                }
            }
        }
    }
```

(The child loop is byte-identical to the existing one; only the fill block and
the moved `owner_active` line are new. The fill must stay **before** the loop —
after it would erase the children.)

- [ ] **Step 4: Run the test to verify it passes**

```bash
cargo test -p tvision-rs -j4 surface_fill_keys_on_own_focus -- --test-threads=4
```

Expected: PASS.

- [ ] **Step 5: Full gates**

```bash
cargo test --workspace -j4 -- --test-threads=4
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo fmt --all --check
```

Expected: all green. **Also:** `git status --porcelain src/snapshots/` must be
empty — no existing golden moved.

- [ ] **Step 6: Commit**

```bash
git add src/view/group.rs
git commit -m "feat(view): Group::set_surface — opt-in focus-aware background surface

A group can name a (normal, inactive) role pair; draw fills its own extent
with it, keyed on the group's own focused flag — the same signal fanned to
children as owner_active — before children overpaint. Default unchanged:
no surface, no fill (faithful to TGroup).

Co-Authored-By: Claude <your model name> <noreply@anthropic.com>"
```

---

### Task 2: Golden snapshot fixture (inset pane, focused / unfocused)

**Files:**
- Modify: `src/view/group.rs` (one test in `mod tests`)
- Create (generated by insta): `src/snapshots/tvision_rs__view__group__tests__group_surface_pane_focused.snap`,
  `src/snapshots/tvision_rs__view__group__tests__group_surface_pane_unfocused.snap`

**Interfaces:**
- Consumes: `Group::set_surface(Role, Role)` from Task 1; existing test harness
  (`with_ctx`, `Probe::boxed`, `HeadlessBackend::new`, `Renderer::new`,
  `screen.snapshot()`).
- Produces: two golden files; no API.

- [ ] **Step 1: Write the snapshot test**

Append to `mod tests` in `src/view/group.rs`, after `surface_fill_keys_on_own_focus`:

```rust
    /// Golden: an inner pane with a surface, inset at a NON-ZERO origin inside
    /// a root group (project rule: never test layout only at (0,0) — the fill
    /// must land through a real sub-context). Uncovered pane cells render the
    /// surface role (normal when the pane is focused, inactive when not), the
    /// child overpaints its own cells, and cells outside the pane stay at the
    /// buffer default. Eyeball the whole snapshot, not just the asserted cells.
    #[test]
    fn surface_snapshot_inset_pane_focused_and_not() {
        let mut theme = Theme::classic_blue();
        theme.set_style(
            Role::ListNormal,
            Style::new(Color::Bios(0x0), Color::Bios(0x3)),
        );
        theme.set_style(
            Role::ListInactive,
            Style::new(Color::Bios(0x8), Color::Bios(0x0)),
        );

        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        // Pane at (2,1)–(10,5) inside a 12x6 root; one child covers only the
        // pane's top-left 4x2 corner.
        let mut pane = Group::new(Rect::new(2, 1, 10, 5));
        pane.set_surface(Role::ListNormal, Role::ListInactive);
        with_ctx(&mut out, &mut timers, |_ctx| {
            pane.insert(Probe::boxed(Rect::new(0, 0, 4, 2), 'C', log.clone()));
        });

        let mut root = Group::new(Rect::new(0, 0, 12, 6));
        let pane_id = with_ctx(&mut out, &mut timers, |_ctx| root.insert(Box::new(pane)));

        let mut shot = |root: &mut Group, focused: bool| -> String {
            root.find_mut(pane_id).unwrap().state_mut().state.focused = focused;
            let (backend, screen) = HeadlessBackend::new(12, 6);
            let mut r = Renderer::new(Box::new(backend));
            r.render(|buf: &mut Buffer| {
                let bounds = root.state().get_bounds();
                let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
                root.draw(&mut dc);
            });
            screen.snapshot()
        };

        insta::assert_snapshot!("group_surface_pane_focused", shot(&mut root, true));
        insta::assert_snapshot!("group_surface_pane_unfocused", shot(&mut root, false));
    }
```

- [ ] **Step 2: Run to generate the goldens, then review them**

```bash
INSTA_UPDATE=always cargo test -p tvision-rs -j4 surface_snapshot_inset_pane_focused_and_not -- --test-threads=4
```

Then **read both new `.snap` files** in `src/snapshots/` and verify by eye:
- `..._focused.snap`: rows 1–4, columns 2–9 carry the `bg=BIOS(3)` style
  (legend) **except** the 4x2 block at columns 2–5, rows 1–2, which is `C` on
  `bg=BIOS(1)`; rows 0 and 5 and columns 0–1, 10–11 are `.` (default).
- `..._unfocused.snap`: identical layout, but the uncovered pane cells carry
  `bg=BIOS(0)` with `fg=BIOS(8)`.
- The `C` block is identical in both (the child ignores the surface).

If the layout is wrong, fix the code — do not bless a wrong golden.

- [ ] **Step 3: Re-run without update mode to verify green**

```bash
cargo test -p tvision-rs -j4 surface_snapshot_inset_pane_focused_and_not -- --test-threads=4
```

Expected: PASS.

- [ ] **Step 4: Pixel-neutrality sweep + full gates**

```bash
cargo test --workspace -j4 -- --test-threads=4
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo fmt --all --check
git status --porcelain src/snapshots/
```

Expected: all green; `git status` shows **exactly two `??` (added) files** —
the two new goldens — and **zero modified** `.snap` files. A modified golden
means a group started painting when it should not: fix the bug, never re-bless.

- [ ] **Step 5: Commit**

```bash
git add src/view/group.rs src/snapshots/tvision_rs__view__group__tests__group_surface_pane_focused.snap src/snapshots/tvision_rs__view__group__tests__group_surface_pane_unfocused.snap
git commit -m "test(view): golden snapshots for Group surface (inset pane, focused/unfocused)

Co-Authored-By: Claude <your model name> <noreply@anthropic.com>"
```

---

### Task 3: CHANGELOG + final verification

**Files:**
- Modify: `CHANGELOG.md` (the `## Unreleased` → `### New` section, currently line ~13)

**Interfaces:**
- Consumes: nothing new.
- Produces: the changelog entry the release workflow will move into the next
  version section.

- [ ] **Step 1: Add the changelog entry**

Under `## Unreleased` → `### New` in `CHANGELOG.md`:

```markdown
- `Group::set_surface` / `Group::clear_surface` — opt-in focus-aware background
  surface for composite panes: the group fills its own extent with a
  consumer-supplied role pair (`normal` when the pane is focused, `inactive`
  when not) before children draw, keyed on the same signal fanned to children
  as `owner_active`. Default unchanged (no fill, faithful to `TGroup`); zero
  pixel change under `classic_blue`.
```

- [ ] **Step 2: Final whole-branch verification**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test --workspace -j4 -- --test-threads=4
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
git status --porcelain
```

Expected: all green; working tree clean except `CHANGELOG.md`. Confirm
`git diff main --stat` touches only `src/view/group.rs`, the two new `.snap`
files, `CHANGELOG.md`, and the plan/spec docs.

- [ ] **Step 3: Commit**

```bash
git add CHANGELOG.md
git commit -m "docs: CHANGELOG for Group::set_surface

Co-Authored-By: Claude <your model name> <noreply@anthropic.com>"
```

---

## Out of scope

- **edaptor migration** (deleting its hand-rolled pane fills) — downstream
  consumer work, explicitly not part of this plan (spec: "context, not part of
  this plan").
- Surface pass-through helpers on `Window`/`Dialog` (they embed `Group` via
  `#[delegate]`; a consumer pane holds its `Group` directly, as edaptor does).
  Add only when a real consumer needs it.
- No new `Role` variant, no `classic_blue` edit — by design.
