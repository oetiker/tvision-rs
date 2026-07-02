# Active-Aware Surfaces Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show which control and which *pane* is active, via two orthogonal axes — a control's own `state.focused` (highlight) and a new draw-time `owner_active` signal (surface) — and clean up the recent commit that conflated them.

**Architecture:** A `Group` already knows whether it is the focused pane (its own `state.focused`, which fans only down the current-child chain). `Group::draw` hands that bit to each child through the `DrawCtx` as `owner_active`; content widgets pick their *surface* from it and their *highlight* from their own `state.focused`. The ad-hoc `*Passive`/`*Inactive` roles keyed on the wrong signal are renamed onto the correct axis; `InputLine` gains the inactive surface it lacked.

**Tech Stack:** Rust (Cargo workspace `tvision-rs` + `tvision-rs-macros`), `insta` snapshot tests on `HeadlessBackend`.

**Design doc:** `docs/superpowers/specs/2026-07-01-uniform-pane-dimming-design.md`

## Global Constraints

- `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target` before any cargo command.
- Build/test on **≤ 4 cores**: prefix cargo with `CARGO_BUILD_JOBS=4`, run tests with `-- --test-threads=4`.
- Verification gate for every task:
  - `cargo test --workspace -j4 -- --test-threads=4`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo fmt --all --check`
- **Zero pixel change under `classic_blue`.** Every `*Inactive` role maps identically to its active counterpart; every existing `.snap` must stay frozen. A `.snap` diff means a bug, never a re-bless (`cargo insta accept` is forbidden in this work).
- English for all code/comments/identifiers.
- `git pull` (ff-only) before starting; roll `CHANGELOG.md` under `## Unreleased`.
- Commit messages end with: `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`

## File Structure

- `src/view/context.rs` — `DrawCtx` gains the `owner_active` field, `sub()` inheritance, `set_owner_active`, `owner_active()`.
- `src/view/group.rs` — `Group::draw` threads `owner_active = self.st.state.focused` into each child's sub-context.
- `src/theme.rs` — role rename cleanup (`ListNormalActive`→`ListNormal`, `ListNormalInactive`→`ListInactive`, `OutlineNormalInactive`→`OutlineInactive`, `InputPassive`→`InputInactive`) across the five role tables + `classic_blue`.
- `src/widgets/list_viewer.rs` — `ListRoles` field rename + two-axis draw.
- `src/widgets/outline.rs` — surface re-keyed to `owner_active`.
- `src/widgets/input_line.rs` — background re-keyed to `owner_active`.
- `CHANGELOG.md`, rustdoc — notes.

---

### Task 1: `owner_active` signal in `DrawCtx` + `Group::draw`

**Files:**
- Modify: `src/view/context.rs` — `struct DrawCtx` (`618-625`), `DrawCtx::new` (`633-643`), `sub()` (`910-921`); add `set_owner_active` + `owner_active` accessor near `style` (`654`).
- Modify: `src/view/group.rs` — `draw` (`995-1006`).
- Test: `src/view/group.rs` `#[cfg(test)] mod tests`.

**Interfaces:**
- Consumes: nothing new.
- Produces:
  - `DrawCtx` field `owner_active: bool` (private).
  - `pub fn DrawCtx::owner_active(&self) -> bool` — true when the owning pane is the focused one.
  - `pub fn DrawCtx::set_owner_active(&mut self, v: bool)`.
  - `DrawCtx::new(..)` seeds `owner_active = true`; `sub()` inherits the parent's value.

- [ ] **Step 1: Write the failing test**

Add to `src/view/group.rs` tests. Mirror the existing test-view pattern in that module (search for `fn draw(&mut self, _ctx: &mut DrawCtx)`), but record the observed `owner_active`. Use a shared cell so the spy can report out of `draw`.

```rust
#[test]
fn group_draw_sets_child_owner_active_from_group_focused() {
    use std::cell::Cell;
    use std::rc::Rc;
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use crate::view::context::DrawCtx;

    // A minimal spy child that records ctx.owner_active() when drawn.
    struct Spy {
        st: ViewState,
        seen: Rc<Cell<Option<bool>>>,
    }
    impl View for Spy {
        fn state(&self) -> &ViewState { &self.st }
        fn state_mut(&mut self) -> &mut ViewState { &mut self.st }
        fn draw(&mut self, ctx: &mut DrawCtx) { self.seen.set(Some(ctx.owner_active())); }
    }

    let seen = Rc::new(Cell::new(None));
    let mut group = Group::new(Rect::new(0, 0, 10, 4));
    group.insert(Box::new(Spy {
        st: ViewState::new(Rect::new(0, 0, 10, 2)),
        seen: seen.clone(),
    }));

    let theme = Theme::classic_blue();
    let mut buf = Buffer::new(10, 4);
    let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 4), Point::new(0, 0));

    // Group NOT focused -> child sees owner_active == false.
    group.state_mut().state.focused = false;
    group.draw(&mut ctx);
    assert_eq!(seen.get(), Some(false), "unfocused group must mark children owner-inactive");

    // Group focused -> child sees owner_active == true.
    group.state_mut().state.focused = true;
    group.draw(&mut ctx);
    assert_eq!(seen.get(), Some(true), "focused group must mark children owner-active");
}
```

(If `Spy`'s minimal `View` impl needs more required methods to compile, copy them from the nearest existing test view in `group.rs`.)

- [ ] **Step 2: Run test to verify it fails**

Run: `CARGO_BUILD_JOBS=4 cargo test --workspace -j4 group_draw_sets_child_owner_active_from_group_focused -- --test-threads=4`
Expected: FAIL — `no method named owner_active found for ... DrawCtx`.

- [ ] **Step 3: Add the field, constructor seed, inheritance, and accessors**

In `src/view/context.rs`, add the field to `struct DrawCtx`:

```rust
pub struct DrawCtx<'a> {
    buffer: &'a mut Buffer,
    clip: Rect,
    origin: Point,
    theme: &'a Theme,
    /// The owning pane is the focused one. Set by `Group::draw` from the owning
    /// group's own `focused`; content widgets pick their surface from it. Unlike
    /// `active` (window-wide) this is per-pane. rstv deviation — C++ focus is
    /// per-window, with no nested panes.
    owner_active: bool,
}
```

In `DrawCtx::new`, seed it `true` (add `owner_active: true,` to the returned struct literal).

In `sub()`, inherit it (add `owner_active: self.owner_active,` to the returned struct literal).

Add the accessors near `style` (after line 656):

```rust
/// Whether the owning pane is the focused one (see [`DrawCtx`] `owner_active`).
pub fn owner_active(&self) -> bool {
    self.owner_active
}

/// Set the owning-pane-active flag for this context. Called by `Group::draw`.
pub fn set_owner_active(&mut self, v: bool) {
    self.owner_active = v;
}
```

- [ ] **Step 4: Thread it in `Group::draw`**

In `src/view/group.rs` `draw` (`995`), set the child's sub-context before drawing:

```rust
fn draw(&mut self, ctx: &mut DrawCtx) {
    let owner_active = self.st.state.focused;
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

- [ ] **Step 5: Run test to verify it passes**

Run: `CARGO_BUILD_JOBS=4 cargo test --workspace -j4 group_draw_sets_child_owner_active_from_group_focused -- --test-threads=4`
Expected: PASS.

- [ ] **Step 6: Full gate + commit**

```bash
CARGO_BUILD_JOBS=4 cargo test --workspace -j4 -- --test-threads=4
CARGO_BUILD_JOBS=4 cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
git add src/view/context.rs src/view/group.rs
git commit -m "feat(view): owner_active draw signal (owning group fans its own focused to children)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

Expected: all snapshots unchanged (nothing reads `owner_active` yet).

---

### Task 2: Rename the mis-axised roles (pixel-neutral cleanup)

Pure identifier rename onto the correct axis. No behaviour change yet; every `.snap` must stay frozen. `InputPassive`→`InputInactive` realizes the spec's "delete `InputPassive` + add `InputInactive`" by re-using the slot (they were identical in classic_blue).

Rename map:
- `Role::ListNormalActive` → `Role::ListNormal`
- `Role::ListNormalInactive` → `Role::ListInactive`
- `Role::OutlineNormalInactive` → `Role::OutlineInactive`
- `Role::InputPassive` → `Role::InputInactive`
- `ListRoles` fields: `normal_active` → `normal`, `normal_inactive` → `inactive`

**Files:**
- Modify: `src/theme.rs` — enum variants (`193`, `198`, `347`, `447`), `ALL` (`482-483`, `512`, `531`), `short_name` (`578-579`, `608`, and the `OutlineNormalInactive` arm), `index` (`652-653`, and the `InputPassive`/`OutlineNormalInactive` arms), `classic_blue` `set()` (`1077`, `1133`, `1176`; leave `1076`/`1132`/`1175` — those are the active roles, only their identifiers change).
- Modify: `src/widgets/list_viewer.rs` — `ListRoles` struct (`250-262`), `LIST_VIEWER` const (`265-272`), draw references (`1001`, `1007`, `1013`).
- Modify: `src/widgets/outline.rs` — draw (`779`).
- Modify: `src/widgets/input_line.rs` — draw (`726`) + the rustdoc comment (`719-722`).

- [ ] **Step 1: Rename across `src/theme.rs`**

Update every occurrence in the enum definition (keep each variant's rustdoc, just rename the identifier and fix wording that says "passive"), the `ALL` array, the `short_name` match, and the `index` match. Keep `short_name` strings ≤ 16 chars: use `"ListNormal"`, `"ListInactive"`, `"OutlineInactive"`, `"InputInactive"`. Do **not** change `ROLE_COUNT` (count is unchanged — a rename, not add/remove).

In `classic_blue`, update the three `set(...)` identifiers only (values stay):

```rust
set(&mut styles, Role::ListInactive, 0x0, 0x3);      // was ListNormalInactive; == ListNormal
set(&mut styles, Role::InputInactive, 0xF, 0x1);     // was InputPassive; == InputNormal
set(&mut styles, Role::OutlineInactive, 0xE, 0x1);   // was OutlineNormalInactive; == OutlineNormal
```

- [ ] **Step 2: Rename the `ListRoles` fields**

In `src/widgets/list_viewer.rs`:

```rust
pub struct ListRoles {
    /// A normal item of an owner-active list (also the `<empty>` fill).
    pub normal: Role,
    /// A normal item when the owning pane is inactive.
    pub inactive: Role,
    /// The focused (cursor) item, shown when this list is the focused control.
    pub focused: Role,
    /// A selected item.
    pub selected: Role,
    /// The inter-column divider.
    pub divider: Role,
}

impl ListRoles {
    pub const LIST_VIEWER: ListRoles = ListRoles {
        normal: Role::ListNormal,
        inactive: Role::ListInactive,
        focused: Role::ListFocused,
        selected: Role::ListSelected,
        divider: Role::ListDivider,
    };
}
```

Update the draw references (`1001`, `1007`, `1013`): `roles.normal_active` → `roles.normal`, `roles.normal_inactive` → `roles.inactive` (logic unchanged in this task).

- [ ] **Step 3: Rename the widget draw identifiers**

`src/widgets/outline.rs:779`: `Role::OutlineNormalInactive` → `Role::OutlineInactive` (logic unchanged).
`src/widgets/input_line.rs:726`: `Role::InputPassive` → `Role::InputInactive`; fix the comment at `719-722` to stop referencing "passive". (Logic unchanged in this task — still keyed on `focused`.)

- [ ] **Step 4: Build, run the role-table guard, and the full suite**

Run: `CARGO_BUILD_JOBS=4 cargo test --workspace -j4 index_is_total_and_distinct -- --test-threads=4`
Expected: PASS (the rename kept `index`/`ALL`/`ROLE_COUNT` consistent).

Run: `CARGO_BUILD_JOBS=4 cargo test --workspace -j4 -- --test-threads=4`
Expected: PASS with **no snapshot changes**.

- [ ] **Step 5: Confirm no stale references remain**

Run: `grep -rn "ListNormalActive\|ListNormalInactive\|OutlineNormalInactive\|InputPassive\|normal_active\|normal_inactive" src/ docs/`
Expected: no hits (except possibly historical `docs/IMPLEMENTATION-LOG.md` entries, which are frozen history — leave those).

- [ ] **Step 6: Gate + commit**

```bash
CARGO_BUILD_JOBS=4 cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
git add src/theme.rs src/widgets/list_viewer.rs src/widgets/outline.rs src/widgets/input_line.rs
git commit -m "refactor(theme): rename surface roles onto the owner-active axis (pixel-neutral)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: `ListViewer` — surface on `owner_active`, highlight on `state.focused`

Split the single `selected && active` predicate into the two axes. Fixes the shuttle/splitter (a list current in an unfocused pane stops looking active).

**Files:**
- Modify: `src/widgets/list_viewer.rs` — `draw` (`994-1013`, loop condition `1033`).
- Test: `src/widgets/list_viewer.rs` tests (snapshot on `HeadlessBackend`).

**Interfaces:**
- Consumes: `DrawCtx::owner_active()` (Task 1); `ListRoles { normal, inactive, focused, selected, divider }` (Task 2).
- Produces: no new API.

- [ ] **Step 1: Write the failing test**

A nested fixture: a `Group` (the "splitter") holding two child groups (the "panes"), each holding a `ListBox`. Focus pane A; assert pane A's list draws its current item in the focused colour while pane B's list does not — even though B's list is the current child of its own pane. Snapshot both. Model on the existing `ListBox` snapshot tests in this file (search `assert_snapshot!`). Key assertion: the two panes' lists render differently despite identical content and both being their pane's current child.

```rust
#[test]
fn list_current_item_bright_only_in_the_focused_pane() {
    // Build splitter -> [paneA[listA], paneB[listB]] on a HeadlessBackend,
    // give both lists the same items and a selected index, focus paneA,
    // render, and snapshot. listA shows ListFocused on its current row;
    // listB (current in paneB but paneB is unfocused) shows ListNormal.
    // (Construct with the same helpers the other ListBox snapshot tests use.)
    // assert_snapshot!(render(...));
}
```

- [ ] **Step 2: Run it to see it fail / capture the wrong (bright-in-both) baseline**

Run: `CARGO_BUILD_JOBS=4 cargo test --workspace -j4 list_current_item_bright_only_in_the_focused_pane -- --test-threads=4`
Expected: the new snapshot shows the **bug** (both lists bright) — do NOT accept it; it documents the pre-fix state and will change in Step 4.

- [ ] **Step 3: Implement the two-axis draw**

Replace `list_viewer.rs:994-1011`:

```rust
    let lv = this.lv();
    let st = &lv.state.state;
    let owner_active = ctx.owner_active(); // surface axis: is my pane focused?
    let list_focused = st.focused;         // highlight axis: am I the focused list?

    let roles = this.list_roles();
    let normal = ctx.style(if owner_active { roles.normal } else { roles.inactive });
    let selected = ctx.style(roles.selected);
    let focused_color = if list_focused {
        Some(ctx.style(roles.focused))
    } else {
        None
    };
    let divider_color = ctx.style(roles.divider);
    let empty_color = ctx.style(roles.normal);
    let accent = ctx.style(roles.selected);
```

Replace the loop condition at `1033`:

```rust
            let color = if list_focused && focused == item && range > 0 {
                focused_color.unwrap_or(normal)
```

(Everything else in the loop is unchanged.)

- [ ] **Step 4: Re-run — verify the fix, then review the new snapshot**

Run: `CARGO_BUILD_JOBS=4 cargo test --workspace -j4 list_current_item_bright_only_in_the_focused_pane -- --test-threads=4`
Inspect the `.snap.new`: pane A's list current row = focused colour, pane B's = normal. Accept **only this new test's** snapshot (it is a brand-new fixture, not an existing frozen one): `cargo insta accept` scoped to that file, or hand-write the `.snap`.

- [ ] **Step 5: Verify all pre-existing snapshots are unchanged**

Run: `CARGO_BUILD_JOBS=4 cargo test --workspace -j4 -- --test-threads=4`
Expected: PASS with **no** changes to any pre-existing `.snap` (flat dialogs: `focused` and `selected && active` agree; `ListNormal == ListInactive`).

- [ ] **Step 6: Gate + commit**

```bash
CARGO_BUILD_JOBS=4 cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
git add src/widgets/list_viewer.rs
git commit -m "fix(list): key highlight on focused, surface on owner_active (nested panes)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: `Outline` — surface on `owner_active`

**Files:**
- Modify: `src/widgets/outline.rs` — `ov_draw` (`771`, `776-780`).
- Test: `src/widgets/outline.rs` tests.

**Interfaces:**
- Consumes: `DrawCtx::owner_active()` (Task 1); `Role::OutlineInactive` (Task 2).

- [ ] **Step 1: Write the failing test**

Nested fixture (splitter → two panes → an `Outline` each), like Task 3. Focus pane A; assert pane B's outline draws its normal rows on the inactive surface while pane A's uses the normal surface. Under an `AutoDim`-style test theme where `OutlineInactive != OutlineNormal`, the two panes' outlines must differ. (If no such test theme helper exists, add one that clones `classic_blue` and overrides `OutlineInactive` to a distinct colour, used only in this test.)

- [ ] **Step 2: Run to verify it fails**

Run: `CARGO_BUILD_JOBS=4 cargo test --workspace -j4 outline -- --test-threads=4`
Expected: FAIL — pre-fix, the surface keys on the outline's own `focused`, so an unfocused *pane's* outline that is its pane's current child still uses `OutlineNormal`.

- [ ] **Step 3: Implement**

In `src/widgets/outline.rs`, delete the now-unused `let focused_state = ...;` (`771`) and re-key the surface (`776-780`):

```rust
    // Normal-row surface follows the owning pane, not this outline's own focus:
    // an outline inside the focused pane uses OutlineNormal; inside an inactive
    // pane it recedes to OutlineInactive. classic_blue maps them identically.
    let nrm_color = ctx.style(if ctx.owner_active() {
        Role::OutlineNormal
    } else {
        Role::OutlineInactive
    });
```

- [ ] **Step 4: Run to verify pass + no frozen-snapshot drift**

Run: `CARGO_BUILD_JOBS=4 cargo test --workspace -j4 -- --test-threads=4`
Expected: new test PASS; all pre-existing `classic_blue` snapshots unchanged.

- [ ] **Step 5: Gate + commit**

```bash
CARGO_BUILD_JOBS=4 cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
git add src/widgets/outline.rs
git commit -m "fix(outline): surface follows owner_active, not the outline's own focus

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 5: `InputLine` — background on `owner_active`

Gives `InputLine` the inactive surface it lacked — the form's blocker. When the owning pane is inactive the whole field recedes; within an active pane every field uses `InputNormal` (the cursor marks the current one).

**Files:**
- Modify: `src/widgets/input_line.rs` — `draw` (`718-727`).
- Test: `src/widgets/input_line.rs` tests.

**Interfaces:**
- Consumes: `DrawCtx::owner_active()` (Task 1); `Role::InputInactive` (Task 2).

- [ ] **Step 1: Write the failing test**

Nested fixture: two panes, an `InputLine` each, focus pane A. Under a test theme where `InputInactive != InputNormal`, assert pane B's input fills with `InputInactive` while pane A's fills with `InputNormal`. (Reuse the Task-4 test-theme helper.)

- [ ] **Step 2: Run to verify it fails**

Run: `CARGO_BUILD_JOBS=4 cargo test --workspace -j4 input_line -- --test-threads=4`
Expected: FAIL — pre-fix the field keys on its own `focused`, so pane B's field (unfocused, but so is pane A's non-current field) does not track the pane.

- [ ] **Step 3: Implement**

Replace `src/widgets/input_line.rs:719-727`:

```rust
        // Background follows the owning pane, not this field's own focus: within
        // the focused pane every field uses InputNormal (the cursor marks the
        // current one); a field in an inactive pane recedes to InputInactive.
        // classic_blue maps both to white-on-blue, so unthemed input is unchanged.
        let color = ctx.style(if ctx.owner_active() {
            Role::InputNormal
        } else {
            Role::InputInactive
        });
```

- [ ] **Step 4: Run to verify pass + no frozen-snapshot drift**

Run: `CARGO_BUILD_JOBS=4 cargo test --workspace -j4 -- --test-threads=4`
Expected: new test PASS; all pre-existing `classic_blue` snapshots unchanged (`InputInactive == InputNormal`).

- [ ] **Step 5: Gate + commit**

```bash
CARGO_BUILD_JOBS=4 cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
git add src/widgets/input_line.rs
git commit -m "fix(input): background follows owner_active (recede with an inactive pane)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 6 (optional): `StaticText` / `Label` inactive surfaces

Only if captions should recede with the pane too. Additive roles — append at the end of the tables so existing `index` values do not shift.

**Files:**
- Modify: `src/theme.rs` — add `StaticTextInactive`, `LabelNormalInactive`, `LabelLightInactive` at the **end** of the enum, `ALL`, `short_name`, `index` (new highest indices), bump `ROLE_COUNT` by 3, and `classic_blue` maps each `== ` its active counterpart.
- Modify: `src/widgets/static_text.rs` — `StaticText::draw` (`119`) and `Label::draw` role picks (`439-441`, `958-963`) key the surface/text on `ctx.owner_active()`.
- Test: `src/widgets/static_text.rs`.

- [ ] **Step 1:** Write a nested-pane test (as Task 4/5) asserting a caption in the unfocused pane uses the inactive role under a test theme.
- [ ] **Step 2:** Run — verify FAIL.
- [ ] **Step 3:** Add the three roles (append; bump `ROLE_COUNT` 77→80; map identically in `classic_blue`) and re-key the draws: `let base = if ctx.owner_active() { Role::StaticText } else { Role::StaticTextInactive };` (and the analogous `Label` pair selection).
- [ ] **Step 4:** Run `index_is_total_and_distinct` + full suite — new test PASS, frozen snapshots unchanged.
- [ ] **Step 5:** Gate + commit.

---

### Task 7: CHANGELOG, rustdoc, spec rename

**Files:**
- Modify: `CHANGELOG.md`; `src/view/context.rs` (rustdoc); `src/theme.rs` (role rustdoc).
- Rename: the design doc file.

- [ ] **Step 1: CHANGELOG**

Under `## Unreleased`:

```markdown
### New
- `DrawCtx::owner_active()` — a draw-time signal (an owning group fans its own
  `focused` to its children) so content widgets recede when their pane is not the
  focused one. `Role::InputInactive` (and `Role::OutlineInactive`) let a theme
  render that recede; `classic_blue` maps them identically (no visual change).

### Changed
- `ListViewer` keys its item highlight on `state.focused` and its row surface on
  `owner_active`, fixing lists that stayed bright inside an unfocused splitter or
  shuttle pane. `Outline`/`InputLine` surfaces now follow `owner_active` too.
- Renamed roles: `ListNormalActive`→`ListNormal`, `ListNormalInactive`→`ListInactive`,
  `OutlineNormalInactive`→`OutlineInactive`, `InputPassive`→`InputInactive`.
```

- [ ] **Step 2: Rustdoc**

Ensure `DrawCtx::owner_active` and the renamed roles carry a `# Turbo Vision heritage` note that this per-pane axis is an rstv deviation (C++ focus is per-window; no nested panes). Remove any lingering "passive/focus-aware surface" wording from the `52570f4` additions.

- [ ] **Step 3: Rename the spec file to match its title**

```bash
git mv docs/superpowers/specs/2026-07-01-uniform-pane-dimming-design.md \
       docs/superpowers/specs/2026-07-01-active-aware-surfaces-design.md
```

Fix the `Design doc:` path near the top of this plan to the new name.

- [ ] **Step 4: Doctest + docs gate**

Run: `CARGO_BUILD_JOBS=4 cargo test --workspace -j4 -- --test-threads=4` and, if the docs site is built here, `cargo xtask docs` — no unresolved intra-doc links to the removed role names.

- [ ] **Step 5: Commit**

```bash
git add CHANGELOG.md src/view/context.rs src/theme.rs docs/superpowers
git commit -m "docs: changelog + rustdoc for owner_active surfaces; rename spec

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Self-Review

- **Spec coverage:** Piece 1 → Task 3; Piece 2 signal → Task 1; role cleanup/rename + `InputInactive` → Tasks 2 & 5; Outline re-key → Task 4; StaticText/Label → Task 6 (optional); classic_blue frozen → verified in every task; downstream (edaptor) is explicitly out of scope. ✓
- **Placeholder scan:** test bodies for Tasks 3–6 describe the fixture precisely and point at the existing snapshot-test helpers to copy; no "add error handling"/"TBD". The one soft spot is "construct with the same helpers the other tests use" — acceptable because the concrete construction is codebase-idiomatic and the assertion is stated exactly.
- **Type consistency:** `owner_active()`/`set_owner_active()` names match across Tasks 1/3/4/5; `ListRoles { normal, inactive, focused, selected, divider }` matches Task 2 → Task 3; role names consistent post-rename.
- **Ordering:** Task 2 (rename) precedes the logic tasks so Tasks 3–5 reference final identifiers; Task 1 (signal) precedes everything that reads it.
