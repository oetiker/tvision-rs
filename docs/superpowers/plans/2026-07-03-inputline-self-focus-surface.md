# InputLine Self-Focus Surface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an opt-in three-surface model to `InputLine` so a themed form can
render only the *focused* field as the bright input well, with non-focused
fields flush on the pane surface and every field receding when the pane is
inactive.

**Architecture:** One new theme role (`Role::InputSurface`, wired equal to
`InputNormal` in `classic_blue`) plus one new private `bool` on `InputLine`
(`self_focus`, default `false`) that switches `draw()`'s surface selection from
the current two-role `owner_active` branch to a three-way branch
(`owner_active` → recede axis, own `state.focused` → well-vs-surface axis).
Default off is byte-identical to today's behavior.

**Tech Stack:** Rust workspace (`tvision-rs` + `tvision-rs-macros`), `insta`
snapshots on `HeadlessBackend`.

**Spec:** `docs/superpowers/specs/2026-07-03-inputline-self-focus-surface-design.md`

## Global Constraints

- `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target` before any cargo command.
- Build/test on **≤ 4 cores**: pass `-j 4` to every cargo invocation.
- Gate: `cargo test --workspace -j 4`, `cargo clippy --workspace --all-targets -j 4 -- -D warnings`, `cargo fmt --all --check`.
- **Zero pixel change under `classic_blue`.** Any existing `.snap` movement is a bug — do not accept snapshot updates for pre-existing tests.
- `cargo-insta` is **not installed** — generate new snapshots with `INSTA_UPDATE=always`, hand-verify the `.snap` content, then commit it.
- English for all code/comments/identifiers.
- Commits end with the trailer: `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`

---

### Task 1: `Role::InputSurface` in the theme

**Files:**
- Modify: `src/theme.rs` (enum ~line 355, `ROLE_COUNT` line 477, `ALL` ~line 522, `name()` ~line 618, `index()` ~line 692, `classic_blue()` ~line 1143, tests module ~line 1229)

**Interfaces:**
- Consumes: existing `Role` enum machinery (`ROLE_COUNT`, `ALL`, `name()`, `index()`, the `set` closure in `classic_blue()`).
- Produces: `Role::InputSurface` variant — Task 2's draw branch and tests refer to it by exactly this name. In `classic_blue` its style **equals** `Role::InputNormal` (white on blue, `set(..., 0xF, 0x1)`).

Note: `index()` values are append-only slots into the style array — the next free
index is **77** (current max is `OutlineInactive => 76`), and `ROLE_COUNT` goes
77 → 78. The existing tests `index_is_total_and_distinct` and
`style_is_total_over_all_variants` (theme.rs ~line 1229) automatically cover the
new variant once it is in `ALL`; the only new test needed is the
classic_blue-equality one.

- [ ] **Step 1: Write the failing test**

In `src/theme.rs`, inside `mod tests` (after `style_is_total_over_all_variants`, ~line 1248), add:

```rust
    /// `InputSurface` is the opt-in middle surface for `InputLine`'s
    /// self-focus mode (active pane, non-focused field). In `classic_blue`
    /// it must equal `InputNormal` (as `InputInactive` already does) so the
    /// opt-in renders identically unthemed — the classic_blue-frozen
    /// guarantee of the self-focus-surface spec.
    #[test]
    fn input_surface_matches_input_normal_in_classic_blue() {
        let t = Theme::classic_blue();
        assert_eq!(t.style(Role::InputSurface), t.style(Role::InputNormal));
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test -p tvision-rs -j 4 --lib theme::tests::input_surface_matches_input_normal_in_classic_blue
```

Expected: FAIL to compile with `no variant or associated item named 'InputSurface' found for enum 'Role'`.

- [ ] **Step 3: Add the variant and wire it everywhere**

Five edits in `src/theme.rs`, all adjacent to the existing `InputInactive` lines:

**(a) Enum variant** — directly after the `InputInactive` variant (after ~line 355):

```rust
    /// An [`InputLine`](crate::widgets::InputLine)'s **non-focused** field
    /// surface within an **active** pane — used only when the field opts into
    /// the self-focus surface via
    /// `InputLine::set_self_focus_surface(true)`: the one focused field keeps
    /// [`InputNormal`](Role::InputNormal) (the bright "well"), its non-focused
    /// siblings take this role, and any field in an inactive pane recedes to
    /// [`InputInactive`](Role::InputInactive). Without the opt-in this role is
    /// never consulted. No C++ counterpart (`TInputLine::draw` has a single
    /// `getColor(focused ? 2 : 1)` and no pane-recede axis). In `classic_blue`
    /// it equals [`InputNormal`](Role::InputNormal), so an opt-in field renders
    /// identically unthemed; a theme wanting a single-well form points this at
    /// its pane-surface colour.
    InputSurface,
```

**(b) `ROLE_COUNT`** (line 477): change `77` to `78`:

```rust
pub(crate) const ROLE_COUNT: usize = 78;
```

**(c) `ALL` array** — insert directly after `Role::InputInactive,` (~line 522):

```rust
    Role::InputSurface,
```

**(d) `name()`** — insert directly after the `InputInactive` arm (~line 618):

```rust
            Role::InputSurface => "InputSurface",
```

**(e) `index()`** — insert directly after the `InputInactive` arm (~line 692), taking the next free slot:

```rust
            Role::InputSurface => 77,
```

**(f) `classic_blue()`** — insert directly after the `InputInactive` set-line (~line 1143):

```rust
        set(&mut styles, Role::InputSurface, 0xF, 0x1); // == InputNormal; opt-in self-focus middle surface (InputLine::set_self_focus_surface), themes may restyle to the pane surface
```

- [ ] **Step 4: Run the theme tests to verify they pass**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test -p tvision-rs -j 4 --lib theme::
```

Expected: PASS, including `input_surface_matches_input_normal_in_classic_blue`,
`index_is_total_and_distinct`, and `style_is_total_over_all_variants`.

- [ ] **Step 5: Commit**

```bash
git add src/theme.rs
git commit -m "feat(theme): Role::InputSurface — opt-in middle surface for InputLine self-focus

New role for the active-pane/non-focused field state of the upcoming
InputLine self-focus surface opt-in. classic_blue wires it identically
to InputNormal, so nothing moves unthemed.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: `InputLine` opt-in + three-way draw branch

**Files:**
- Modify: `src/widgets/input_line.rs` (struct ~line 142, `new()` ~line 242, methods after `set_validator` ~line 269, `draw()` ~line 714, tests ~line 2459)

**Interfaces:**
- Consumes: `Role::InputSurface` from Task 1; existing test helpers `field(width, data) -> InputLine` (~line 1269) and `fill_bg(&mut InputLine, &Theme, owner_active: bool) -> Color` (~line 2449); `Theme::set_style(Role, Style)`.
- Produces: `pub fn set_self_focus_surface(&mut self, on: bool)` and `pub fn with_self_focus_surface(mut self, on: bool) -> Self` on `InputLine`; private field `self_focus: bool` (default `false`). Task 3's fixture calls `set_self_focus_surface(true)`.

- [ ] **Step 1: Write the failing tests**

In `src/widgets/input_line.rs`, in the `-- owner-active background --` test
section: first **extend the existing test** `background_follows_owner_active`
(~line 2489) — after the existing `focused = false` / `owner_active = true`
assertion, add the missing quadrant so the default-off contract fully matches
the spec's test plan ("paints `InputNormal` whether or not `state.focused`"):

```rust
        // owner_active=true with the field itself focused: still InputNormal —
        // with the self-focus opt-in OFF (the default), own focus never
        // changes the surface.
        il.state.state.focused = true;
        assert_eq!(fill_bg(&mut il, &theme, true), normal_bg);
```

Then add the new three-way test after `background_follows_owner_active`
(after ~line 2495):

```rust
    /// The self-focus surface opt-in (`set_self_focus_surface(true)`) selects
    /// among THREE roles: an inactive pane recedes to `InputInactive`
    /// regardless of the field's own focus; within an active pane the focused
    /// field is the `InputNormal` well and a non-focused field sits on
    /// `InputSurface`. Uses a theme where all three roles differ (classic_blue
    /// makes them identical).
    #[test]
    fn self_focus_surface_three_way() {
        use crate::color::{Color, Style};

        let mut theme = Theme::classic_blue();
        theme.set_style(
            Role::InputSurface,
            Style::new(Color::Bios(0x0), Color::Bios(0x7)),
        );
        theme.set_style(
            Role::InputInactive,
            Style::new(Color::Bios(0x7), Color::Bios(0x4)),
        );
        let normal_bg = theme.style(Role::InputNormal).bg;
        let surface_bg = theme.style(Role::InputSurface).bg;
        let inactive_bg = theme.style(Role::InputInactive).bg;
        assert!(
            normal_bg != surface_bg && surface_bg != inactive_bg && normal_bg != inactive_bg,
            "test theme must distinguish all three roles"
        );

        let mut il = field(12, "hello");
        il.first_pos = 0;
        il.set_self_focus_surface(true);

        // Active pane, focused field → the well.
        il.state.state.focused = true;
        assert_eq!(fill_bg(&mut il, &theme, true), normal_bg);

        // Active pane, non-focused field → the pane surface (the state the
        // two-role model could not name).
        il.state.state.focused = false;
        assert_eq!(fill_bg(&mut il, &theme, true), surface_bg);

        // Inactive pane → receded, regardless of the field's own focus.
        il.state.state.focused = true;
        assert_eq!(fill_bg(&mut il, &theme, false), inactive_bg);
        il.state.state.focused = false;
        assert_eq!(fill_bg(&mut il, &theme, false), inactive_bg);
    }

    /// The builder form sets the same flag.
    #[test]
    fn with_self_focus_surface_builder() {
        use crate::color::{Color, Style};

        let mut theme = Theme::classic_blue();
        theme.set_style(
            Role::InputSurface,
            Style::new(Color::Bios(0x0), Color::Bios(0x7)),
        );
        let surface_bg = theme.style(Role::InputSurface).bg;

        let mut il = field(12, "hello").with_self_focus_surface(true);
        il.first_pos = 0;
        il.state.state.focused = false;
        assert_eq!(fill_bg(&mut il, &theme, true), surface_bg);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test -p tvision-rs -j 4 --lib widgets::input_line::tests::self_focus
```

Expected: FAIL to compile with `no method named 'set_self_focus_surface' found`.

- [ ] **Step 3: Implement the flag, the API, and the draw branch**

**(a) Struct field** — in `pub struct InputLine`, directly after the
`pub validator: ...` field (after ~line 142):

```rust
    /// Opt-in self-focus surface (see
    /// [`set_self_focus_surface`](InputLine::set_self_focus_surface)): when
    /// `true`, `draw` distinguishes the focused field (`Role::InputNormal`)
    /// from its active-pane siblings (`Role::InputSurface`); when `false`
    /// (default), the surface keys on `owner_active` alone.
    self_focus: bool,
```

**(b) Constructor** — in `InputLine::new` (~line 223), add to the struct literal
(after `validator,`):

```rust
            self_focus: false,
```

**(c) API methods** — directly after `set_validator` (after ~line 269):

```rust
    /// Opt into a **self-focus surface**: the field paints the input well
    /// ([`Role::InputNormal`]) only when it *itself* holds focus; a non-focused
    /// field in an active pane uses [`Role::InputSurface`], and any field in an
    /// inactive pane recedes to [`Role::InputInactive`]. Off by default, in
    /// which case the field keys its surface on `owner_active` alone (every
    /// field in an active pane is the well — the classic Turbo Vision look,
    /// unchanged).
    ///
    /// Use this for a "single-well" form where only the current field is the
    /// bright input target and the others sit flush on the pane surface. In
    /// `classic_blue` all three roles resolve to the same colour, so the
    /// opt-in only becomes visible on a theme that restyles
    /// [`Role::InputSurface`] / [`Role::InputInactive`].
    pub fn set_self_focus_surface(&mut self, on: bool) {
        self.self_focus = on;
    }

    /// Builder form of [`set_self_focus_surface`](InputLine::set_self_focus_surface).
    pub fn with_self_focus_surface(mut self, on: bool) -> Self {
        self.self_focus = on;
        self
    }
```

**(d) Draw branch** — in `draw()` (~line 720), replace the comment + two-role
selection:

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

with the three-way selection (default-off resolves identically to the old
branch — `self_focus == false` can never reach `InputSurface`):

```rust
        // Surface selection. Default: the background follows the owning pane,
        // not this field's own focus — within the focused pane every field uses
        // InputNormal (the cursor marks the current one); a field in an
        // inactive pane recedes to InputInactive. With the self-focus opt-in
        // (`set_self_focus_surface`), own focus additionally picks
        // well-vs-surface WITHIN an active pane: only the focused field is the
        // InputNormal well, its siblings sit on InputSurface. `owner_active`
        // alone owns "receded" on both paths. classic_blue maps all three roles
        // to white-on-blue, so unthemed input is unchanged either way.
        let color = ctx.style(if !ctx.owner_active() {
            Role::InputInactive
        } else if self.self_focus && !self.state.state.focused {
            Role::InputSurface
        } else {
            Role::InputNormal
        });
```

**(e) `draw()` rustdoc** — in the doc comment above `draw` (~line 702), the
sentence listing the roles currently reads:

```text
    /// Colors come from three theme roles: [`Role::InputNormal`] for the
    /// background and unselected text within an active pane
    /// ([`Role::InputInactive`] takes over when the owning pane recedes — see
    /// below), [`Role::InputArrow`] for the `◄`/`►` overflow indicators,
```

Replace those four lines with:

```text
    /// Colors come from the theme roles: [`Role::InputNormal`] for the
    /// background and unselected text within an active pane
    /// ([`Role::InputInactive`] takes over when the owning pane recedes, and,
    /// under the [`set_self_focus_surface`](InputLine::set_self_focus_surface)
    /// opt-in, [`Role::InputSurface`] for a non-focused field in an active
    /// pane), [`Role::InputArrow`] for the `◄`/`►` overflow indicators,
```

- [ ] **Step 4: Run the input_line tests to verify they pass**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test -p tvision-rs -j 4 --lib widgets::input_line::
```

Expected: PASS — the two new tests, the extended
`background_follows_owner_active`, and every pre-existing test (including the
four `snapshot_*` tests, unchanged).

- [ ] **Step 5: Commit**

```bash
git add src/widgets/input_line.rs
git commit -m "feat(input_line): opt-in self-focus surface (three-way role selection)

set_self_focus_surface / with_self_focus_surface: when enabled, only the
focused field paints the InputNormal well; a non-focused field in an
active pane uses the new Role::InputSurface, and an inactive pane still
recedes to InputInactive. Default off = the exact previous two-role
owner_active branch.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: classic_blue-frozen snapshot fixture

**Files:**
- Modify: `src/widgets/input_line.rs` (snapshot test section, after `snapshot_scrolled_selection_from_start`, ~line 1382)
- Create: `src/widgets/snapshots/tvision_rs__widgets__input_line__tests__snapshot_self_focus_classic_blue_identical.snap` (generated by insta)

**Interfaces:**
- Consumes: `field()` and `render()` test helpers (~lines 1214, 1269); `InputLine::set_self_focus_surface` from Task 2. `render()` draws with `Theme::classic_blue()`.
- Produces: the `.snap` fixture proving the classic_blue-frozen guarantee.

- [ ] **Step 1: Write the failing test**

After `snapshot_scrolled_selection_from_start` (~line 1382), add:

```rust
    /// classic_blue frozen: because InputSurface and InputInactive both equal
    /// InputNormal in classic_blue, a field with the self-focus opt-in ON
    /// renders byte-identically to a default field — the opt-in is invisible
    /// unthemed. Guards the zero-pixel-change guarantee of the
    /// self-focus-surface spec.
    #[test]
    fn snapshot_self_focus_classic_blue_identical() {
        let mut plain = field(12, "hello");
        plain.cur_pos = 0;
        plain.first_pos = 0;
        let mut opted_in = field(12, "hello");
        opted_in.cur_pos = 0;
        opted_in.first_pos = 0;
        opted_in.set_self_focus_surface(true);

        let plain_snap = render(&mut plain);
        let opted_snap = render(&mut opted_in);
        assert_eq!(
            plain_snap, opted_snap,
            "classic_blue must render an opt-in field byte-identically"
        );
        insta::assert_snapshot!(opted_snap);
    }
```

- [ ] **Step 2: Run it to verify only the missing snapshot fails**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test -p tvision-rs -j 4 --lib widgets::input_line::tests::snapshot_self_focus_classic_blue_identical
```

Expected: the `assert_eq!` passes but the test FAILS on the missing snapshot
(insta writes a `.snap.new`). If the `assert_eq!` itself fails, the draw branch
from Task 2 is wrong — stop and fix that first.

- [ ] **Step 3: Generate and hand-verify the snapshot**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
INSTA_UPDATE=always cargo test -p tvision-rs -j 4 --lib widgets::input_line::tests::snapshot_self_focus_classic_blue_identical
```

Then read
`src/widgets/snapshots/tvision_rs__widgets__input_line__tests__snapshot_self_focus_classic_blue_identical.snap`
and compare it against the committed
`tvision_rs__widgets__input_line__tests__snapshot_basic_field.snap` — the
rendered cells must be identical (same ` hello` text from column 1, same
white-on-blue styling across all 12 columns). **Do not blind-accept.**

- [ ] **Step 4: Full-workspace verification (zero .snap movement)**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test --workspace -j 4
git status --porcelain
```

Expected: all tests green; `git status` shows ONLY the new test code, the ONE
new `.snap` file, and (from later tasks) doc files — **no modified pre-existing
`.snap` files and no stray `.snap.new` files anywhere**. A modified existing
`.snap` means the classic_blue-frozen guarantee is broken: revert it and fix
the draw branch instead of accepting it.

- [ ] **Step 5: Commit**

```bash
git add src/widgets/input_line.rs src/widgets/snapshots/tvision_rs__widgets__input_line__tests__snapshot_self_focus_classic_blue_identical.snap
git commit -m "test(input_line): classic_blue-frozen fixture for the self-focus opt-in

An opt-in field renders byte-identically to a default field under
classic_blue (all three surface roles resolve to the same colour).

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: Deviation note, CHANGELOG, final gate

**Files:**
- Modify: `docs/PORTING-GUIDE.md` (the `## tvision-rs-original extensions` section, after the `### Truecolor color-picker` subsection, ~line 818)
- Modify: `CHANGELOG.md` (`## Unreleased` → `### New`, ~line 13)

**Interfaces:**
- Consumes: the landed API names from Tasks 1–2 (`Role::InputSurface`, `InputLine::set_self_focus_surface` / `with_self_focus_surface`).
- Produces: nothing downstream — documentation only.

Background for the PORTING-GUIDE edit: the spec says to document this
"alongside the active-aware surfaces deviation", but PORTING-GUIDE currently has
**no** entry for the v0.6/v0.7 active-aware surface work (it lives only in the
role rustdoc and the spec/plan files). So this task adds one extensions
subsection covering the whole surface family, with this opt-in as its newest
member.

- [ ] **Step 1: Add the PORTING-GUIDE extensions subsection**

In `docs/PORTING-GUIDE.md`, after the `### Truecolor color-picker` subsection
(insert before the `---` at ~line 820), add:

```markdown
### Active-aware surfaces (`owner_active` × self-focus)
C++ Turbo Vision has no notion of a control surface that follows its owning
pane's activity: `TInputLine::draw` picks between exactly two palette entries
via `getColor(sfFocused ? 2 : 1)`, keyed on the field's own focus only.
tvision-rs adds an orthogonal two-axis model on top of the faithful roles:

- **`DrawCtx::owner_active`** (v0.6.0) — "is this control's pane active?"
  `InputLine` keys its background on it (`Role::InputNormal` vs
  `Role::InputInactive`); in `classic_blue` both resolve to the same colour, so
  the classic look is unchanged.
- **`Group::set_surface` / `clear_surface`** (v0.7.0) — an opt-in pane
  background that follows the group's focus.
- **`InputLine::set_self_focus_surface` / `with_self_focus_surface`** — an
  opt-in third surface: only the *focused* field paints the `Role::InputNormal`
  well; a non-focused field in an active pane uses `Role::InputSurface`, and
  any field in an inactive pane recedes to `Role::InputInactive`. Off by
  default (the two-role `owner_active` branch above). `classic_blue` wires all
  three roles identically, so the opt-in is invisible unthemed. The two axes
  stay orthogonal: `owner_active` alone owns "receded"; own focus only picks
  well-vs-surface *within* an active pane.

See `docs/superpowers/specs/2026-07-01-active-aware-surfaces-design.md`,
`…/2026-07-03-group-focus-aware-surface-design.md`, and
`…/2026-07-03-inputline-self-focus-surface-design.md`.
```

- [ ] **Step 2: Roll the CHANGELOG**

In `CHANGELOG.md`, under `## Unreleased` → `### New` (after the existing
`redeploy-docs.yml` bullet), add:

```markdown
- `InputLine::set_self_focus_surface` / `with_self_focus_surface` — opt-in
  three-surface model for single-well forms: only the focused field paints the
  `Role::InputNormal` well, a non-focused field in an active pane uses the new
  `Role::InputSurface`, and an inactive pane still recedes to
  `Role::InputInactive`. Default off = previous behavior; `classic_blue` wires
  all three roles identically, so nothing changes unthemed.
```

- [ ] **Step 3: Run the full gate**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test --workspace -j 4
cargo clippy --workspace --all-targets -j 4 -- -D warnings
cargo fmt --all --check
```

Expected: all three green. If `xtask docs` is part of your verification habit,
note the new rustdoc links (`Role::InputSurface`, `set_self_focus_surface`) are
all to public items — no `pub(crate)` intra-doc links were introduced.

- [ ] **Step 4: Commit**

```bash
git add docs/PORTING-GUIDE.md CHANGELOG.md
git commit -m "docs: active-aware surfaces extension note + CHANGELOG for self-focus opt-in

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```
