# Focusable content surface generalization — implementation plan

**Spec (source of truth):**
`docs/superpowers/specs/2026-07-03-focusable-content-surface-generalization-design.md`
(decided: Option A, default-on). Read the spec's "The model", "The
non-selectable rule", and "Mechanism" sections before any task.

**Branch:** `docs/focus-surface-generalization` (PR #14). Ships in 0.9.0.

## Global constraints (bind every task)

- `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target` in every shell.
- Build/test on **≤ 4 cores**: `cargo build -j4`, `cargo test -j4`.
- Gate per task: `cargo test --workspace -j4`, `cargo clippy --workspace
  --all-targets -j4 -- -D warnings`, `cargo fmt --all --check`.
- **Zero pixel change under `classic_blue`: NO existing `.snap` file may be
  modified.** Any modified `.snap` is a bug in the change, never a re-bless.
- Known pre-existing flake (unrelated, do not chase): `input_line::tests::
  ins_toggles_cursor_ins` is order-dependent under parallel test runs; it
  passes when run isolated.
- The shared selection rule, verbatim from the spec (all three inputs):

  ```text
  if !ctx.owner_active()            -> roles.inactive
  else if self_focused || !selectable -> roles.normal
  else                              -> roles.surface
  ```

- English comments/identifiers; commit messages end with
  `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.
- Do not update CHANGELOG/PORTING-GUIDE/IMPLEMENTATION-LOG in Tasks 1–5
  (Task 6 owns all doc rolls — avoids conflicts).

## Task 1: Foundation — `SurfaceRoles` + `DrawCtx::content_surface` + two new theme roles

**Files:** `src/theme.rs`, `src/view/context.rs`.

1. `src/theme.rs` — add two roles, following `Role::InputSurface` as the exact
   pattern (enum variant, the roles array around line 506–555, the `name()`
   match around 603–652, the stable index match around 678–727 — take the next
   free indices; do NOT renumber existing entries):
   - `Role::ListSurface` — doc: the middle content surface of a list widget:
     active pane, a selectable sibling holds focus. Declared next to
     `ListInactive`.
   - `Role::OutlineSurface` — same for `Outline`, next to `OutlineInactive`.
   - `classic_blue()`: wire both to the SAME colour as their `*Normal` role
     (`ListSurface` = `ListNormal`'s `set(...)` values, `OutlineSurface` =
     `OutlineNormal`'s), each with a comment in the existing style:
     `// == ListNormal; middle content surface (three-surface rule), themes may restyle to the pane surface`.
   - Extend the existing classic_blue equality test (near line 1275, the one
     asserting `InputSurface == InputNormal`) with
     `ListSurface == ListNormal` and `OutlineSurface == OutlineNormal`.
2. `src/theme.rs` — add the role triple:

   ```rust
   /// The role triple a focusable content widget paints its surface from,
   /// consumed by [`DrawCtx::content_surface`] (three-surface rule).
   #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   pub struct SurfaceRoles {
       /// Active pane and this widget is focused (or can never be focused).
       pub normal: Role,
       /// Active pane, a selectable sibling holds focus.
       pub surface: Role,
       /// Owning pane receded.
       pub inactive: Role,
   }
   ```

   Re-export alongside `Role`/`Theme` wherever those are re-exported (check
   `src/lib.rs` / module `pub use`s so `tv::SurfaceRoles` works like
   `tv::Role`).
3. `src/view/context.rs` — on `DrawCtx` (next to `style()` at ~660 and
   `owner_active()` at ~665):

   ```rust
   /// Select a focusable content widget's surface per the three-surface
   /// rule: `owner_active` recede x own-focus well-vs-surface x
   /// selectability. `surface` means "a selectable sibling won the focus
   /// contest", so a widget that can never hold focus
   /// (`selectable == false`) paints `normal` in an active pane, never
   /// `surface`. Under `classic_blue` every triple collapses to `normal`'s
   /// colour, preserving classic per-window uniformity; a theme opts into
   /// sibling distinction by giving `surface` its own colour.
   pub fn content_surface(&self, roles: SurfaceRoles, self_focused: bool,
                          selectable: bool) -> Style
   ```

   Body = the shared rule from Global constraints, resolved through
   `self.style(...)`.
4. **Tests** (in `context.rs`'s test module, themed with three DISTINCT
   colours — build a `Theme` and `set` the three roles of one triple to
   different colours, mirroring how existing widget tests build themed
   fixtures):
   - `!owner_active` → `inactive` style, for `self_focused` = true AND false.
   - `owner_active` + `self_focused` → `normal`.
   - `owner_active` + `!self_focused` + `selectable` → `surface`.
   - `owner_active` + `!self_focused` + `!selectable` → `normal` (the
     non-selectable rule).
5. No widget changes in this task. Full gate; zero `.snap` diffs
   (`git status` must show no snapshot changes).

## Task 2: InputLine — remove the v0.8.0 opt-in, adopt the shared rule

**Files:** `src/widgets/input_line.rs`, possibly `src/widgets/file_input_line.rs`
(check), `src/theme.rs` (one comment).

1. Remove from `InputLine`: the `self_focus: bool` field (~line 144–148, and
   its `= false` init ~241), `set_self_focus_surface` (~291) and
   `with_self_focus_surface` (~296) including their rustdoc. This is a
   deliberate breaking removal (spec: a no-op flag would lie); do NOT leave
   deprecated shims. Grep the whole tree (`src/`, `examples/`, `tests/`) for
   `self_focus_surface` — no references may remain.
2. Add next to InputLine's other role usage:

   ```rust
   /// InputLine's surface triple for [`DrawCtx::content_surface`].
   const SURFACE_ROLES: SurfaceRoles = SurfaceRoles {
       normal: Role::InputNormal,
       surface: Role::InputSurface,
       inactive: Role::InputInactive,
   };
   ```

3. `draw()` (~755–767): replace the whole hand-rolled three-branch `let color =
   ctx.style(if ...)` with

   ```rust
   let color = ctx.content_surface(Self::SURFACE_ROLES,
                                   self.state.state.focused,
                                   self.state.options.selectable);
   ```

   and rewrite the preceding comment block: the three-surface rule is now the
   default (owner_active recede; own focus picks well-vs-surface within an
   active pane; a never-selectable field stays Normal); classic_blue collapses
   the triple so unthemed input is unchanged.
4. `src/theme.rs` line ~1160: the `InputSurface` classic_blue comment says
   "opt-in self-focus middle surface (InputLine::set_self_focus_surface)" —
   reword to match the new reality (default three-surface rule; themes may
   restyle to the pane surface), consistent with Task 1's new comments.
5. **Tests** in `input_line.rs`:
   - `snapshot_self_focus_classic_blue_identical` (~1429): the opt-in is gone.
     Repurpose it (rename: `snapshot_default_three_surface_classic_blue_identical`)
     to assert what the spec's test plan asks: a default field renders
     byte-identically to the existing golden — i.e. keep the SAME existing
     `.snap` golden, drop the `set_self_focus_surface` call, keep the
     unfocused+owner_active setup so it still exercises the discriminating
     path (which now selects `InputSurface` by DEFAULT). Do not create or
     modify any `.snap`.
   - `background_follows_owner_active` (~2530) and any themed test asserting
     "active pane + unfocused field → InputNormal": update to the new default
     — active pane + unfocused selectable field → `InputSurface`. Rename so
     names describe the rule (e.g. `surface_follows_three_surface_rule`).
   - Add the themed three-way + non-selectable cases if not already covered by
     the updated tests: focused → `InputNormal`; unfocused selectable →
     `InputSurface`; `!owner_active` → `InputInactive`; unfocused
     NON-selectable → `InputNormal`. (Use the existing `fill_bg` helper
     ~2512, extended as needed.)
   - Check `FileInputLine`: it previously had no opt-in forwarder; verify it
     compiles and inherits the new default via its embedded `InputLine`
     (no code needed unless a forwarder referenced the removed methods).
6. Full gate; `git status` must show zero `.snap` modifications.

## Task 3: ListViewer family — sextet roles, shared rule, non-selectable fixture

**Files:** `src/widgets/list_viewer.rs`; grep for other `ListRoles {` literal
constructions across `src/` (e.g. history viewer, list box) and update them.

1. `ListRoles` (~250): add field
   `/// The middle content surface: active pane, a selectable sibling holds focus.`
   `pub surface: Role,` — update the struct's rustdoc from "five roles /
   quintet" to six, and add `surface: Role::ListSurface` to
   `ListRoles::LIST_VIEWER` (~264). Fix EVERY other `ListRoles { ... }`
   literal in the tree (compiler will find them; keep each subclass's
   `surface` consistent with its family — for List* families that reuse the
   base quintet, `Role::ListSurface`).
2. `draw` (~999–1007): replace

   ```rust
   let normal = ctx.style(if owner_active { roles.normal } else { roles.inactive });
   ```

   with

   ```rust
   let normal = ctx.content_surface(
       SurfaceRoles { normal: roles.normal, surface: roles.surface, inactive: roles.inactive },
       st.focused,
       lv.state.options.selectable,
   );
   ```

   Keep the highlight axis (`focused_color` / `selected`) EXACTLY as is —
   the spec touches only the row surface. Update the surrounding axis
   comments (~2071 region and at the draw site) to the three-surface rule.
3. **Tests** in `list_viewer.rs`:
   - `surface_axis_tracks_owner_active_not_own_state` (~2080): the premise
     changed. Rewrite as the themed three-way selection (three distinct
     colours on `ListNormal`/`ListSurface`/`ListInactive`):
     `owner_active=false` → inactive; `owner_active=true` + list focused →
     normal; `owner_active=true` + list unfocused (selectable) → surface.
   - **Non-selectable fixture (spec test plan):** a list with
     `state.options.selectable = false` in an ACTIVE pane paints
     `ListNormal`, never `ListSurface`; in an inactive pane, `ListInactive`.
   - `highlight_axis_tracks_own_state_focused_not_owner_active` (~2118) must
     still pass unchanged (highlight axis untouched).
4. Full gate; zero `.snap` modifications (classic_blue collapses the triple).

## Task 4: Outline — adopt the shared rule

**Files:** `src/widgets/outline.rs`.

1. `draw` (~776–780): replace the two-way

   ```rust
   let nrm_color = ctx.style(if ctx.owner_active() { Role::OutlineNormal } else { Role::OutlineInactive });
   ```

   with `ctx.content_surface(...)` over a
   `const SURFACE_ROLES: SurfaceRoles = { OutlineNormal, OutlineSurface,
   OutlineInactive }` (same shape as InputLine's), passing the outline's own
   `state.focused` and `state.options.selectable`. Other role colours
   (`OutlineFocused`/`OutlineSelected`/`OutlineNotExpanded`) unchanged.
   Update the module-head role list rustdoc (~37) to mention
   `OutlineSurface`.
2. **Tests:** update `normal_row_surface_follows_owner_active_not_own_focus`
   (~2043) to the three-way rule (it currently asserts active-pane +
   unfocused → Normal, which is now Surface for a selectable outline);
   keep the fixture minimal per the spec (themed three-way, one test), reuse
   the existing `render_with`-style helper (~2026).
3. Full gate; zero `.snap` modifications.

## Task 5: Integration fixtures — shuttle, single-focusable invariant

**Files:** a new integration test file `tests/content_surface.rs` (or extend
an existing integration test module if one already covers group focus — check
`tests/` first and follow the existing pattern for building a `Group` with
children on the `HeadlessBackend`).

1. **Shuttle fixture (the motivating case, spec test plan):** one focused
   `Group` (themed: `ListNormal`/`ListSurface`/`ListInactive` three distinct
   colours) holding TWO selectable `ListViewer`s and a `Button`, each list
   with a few rows. Assert by sampling a background cell inside each list in
   three focus states:
   - list A focused → A's row surface = normal colour, B's = surface colour;
   - button focused → BOTH lists' row surfaces = surface colour;
   - owning pane unfocused → both = inactive colour.
   Drive focus through the group's real focus mechanism (select/focus the
   child as production code would — mirror how existing group-focus tests do
   it), not by poking private state, so the fixture exercises the focus
   chain end-to-end. **Test at a non-zero origin** (inset the group) per
   project test convention.
2. **Single-focusable-pane invariant (spec test plan):** a group whose ONLY
   selectable child is one list: pane focused → list surface = normal;
   pane unfocused → inactive. Assert the surface colour is NEVER the
   distinct `ListSurface` colour in either state.
3. Full gate; zero `.snap` modifications anywhere in the run
   (`git status` clean of `.snap` diffs) — this is the whole-feature
   classic_blue-frozen check.

## Task 6: Docs — PORTING-GUIDE, CHANGELOG, IMPLEMENTATION-LOG

**Files:** `docs/PORTING-GUIDE.md`, `CHANGELOG.md`,
`docs/IMPLEMENTATION-LOG.md`, `docs/HANDOVER.md` (only if it references the
removed opt-in — grep `self_focus`).

1. `docs/PORTING-GUIDE.md`: find the active-surfaces deviation entry (the one
   the v0.8.0 InputLine opt-in extended — grep `InputSurface` /
   `set_self_focus_surface`). Rewrite that extension per the spec's
   "Deviation note" section: default three-surface model via
   `DrawCtx::content_surface` (rule verbatim), the-theme-is-the-opt-in,
   sibling-includes-buttons consequence, non-selectable rule, and the
   standing rule that every future focusable content widget uses the triple +
   helper by construction. Remove/replace any text describing the opt-in as
   current API.
2. `CHANGELOG.md` under `## Unreleased`:
   - `### Changed` — breaking: `InputLine::set_self_focus_surface` /
     `with_self_focus_surface` removed; the three-surface selection
     (`Normal`/`Surface`/`Inactive`) is now the DEFAULT for focusable content
     widgets (`InputLine`, `ListViewer` family, `Outline`). Migration: delete
     the call — the behavior is the default. Zero visual change under
     `classic_blue` (all triples collapse). Also: `ListRoles` gained a
     `surface` field (breaking for literal constructions).
   - `### New` — `SurfaceRoles` + `DrawCtx::content_surface`;
     `Role::ListSurface` / `Role::OutlineSurface`; themes can now distinguish
     the focused widget among siblings.
3. `docs/IMPLEMENTATION-LOG.md`: prepend (newest-first) a session entry for
   this branch: what landed per commit, the Option A decision pointer to the
   spec, and the classic_blue-frozen verification.
4. `cargo fmt --all --check` still clean; docs-only task otherwise (no code).
   If `docs/HANDOVER.md` or rustdoc elsewhere still mentions the opt-in,
   fix it here.
