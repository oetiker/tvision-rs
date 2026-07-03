# Window Settable Minimum Size — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the window's interactive-resize floor a settable per-window
`min_size` (default 16×6, raisable only), restoring the C++ virtual-`sizeLimits`
behavior that the port's static dispatch lost, and convert the three in-repo
consumers (`FileDialog` 49×19, `ChDirDialog` 48×18, `EditWindow` 24×6).

**Architecture:** One chokepoint — every clamp path (mouse corner drag, keyboard
resize, ZOOM/restore, owner-driven `calc_bounds`) reads
`View::size_limits(self=Window)`, so changing `Window::size_limits` to return a
new `min_size` field fixes all of them with zero call-site changes. Wrappers
*push* their floor down via `set_min_size` at construction instead of the C++
model where clamp paths *pull* it up through virtual dispatch (impossible under
D2 embed-and-delegate). Spec:
`docs/superpowers/specs/2026-07-03-window-settable-min-size-design.md`.

**Tech Stack:** Rust, Cargo workspace (`tvision-rs` + `tvision-rs-macros`),
`#[delegate]` proc-macro, headless pump-level tests in `src/app/program.rs`.

## Global Constraints

- `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target` before any cargo
  command (artifacts must NOT land in `./target`).
- Max 4 cores: every cargo command gets `-j 4`; test runs get
  `-- --test-threads=4` only if you run the full suite.
- Gate for every task: `cargo fmt --all --check`, `cargo clippy --workspace
  --all-targets -j 4 -- -D warnings`, plus the task's tests.
- All work on branch `feat/window-settable-min-size` (main is protected;
  PR at the end).
- English for all code/comments/identifiers.
- Coordinates are `i32`; `Point::new` is `const fn` (verified), so `const`
  items may use it.
- Do NOT run `cargo fmt --all` and then commit files outside the task's scope;
  verify `git status` shows only your files before committing.
- Do not regenerate `docs/book/src/screens/*.html` (xtask noise); never commit
  changes there.

---

### Task 1: `Window` core — `MIN_WIN_SIZE`, `min_size` field, setter/builder, `size_limits`

**Files:**
- Modify: `src/window/window.rs` (struct ~`:162-183`, `new` ~`:237-284`,
  setters ~`:415-452`, `size_limits` ~`:1668-1681`, docs `:789-791`,
  tests ~`:1842-1875`)
- Modify: `src/app/program.rs` (tests, after `drag_move_clamps_to_limits`
  ~`:7151`)

**Interfaces:**
- Consumes: existing `Point` (`const fn new`), `View::size_limits`,
  `window_with_frame()` test helper (window bounds `0,0,40,15`), program.rs
  test helpers `program_with_desktop`, `mouse_down_at`, `mouse_move_at`,
  `mouse_up_at`, `win_state`.
- Produces: `Window::MIN_WIN_SIZE: Point` (assoc const, `16×6`),
  `pub fn set_min_size(&mut self, min: Point)`,
  `pub fn with_min_size(mut self, min: Point) -> Self`. Later tasks call these.

- [ ] **Step 0: Branch**

```bash
cd /home/oetiker/checkouts/rstv && git checkout -b feat/window-settable-min-size
```

- [ ] **Step 1: Write the failing unit tests** in the `tests` module of
  `src/window/window.rs`, directly after `calc_bounds_honours_min_win_size`
  (~`:1875`):

```rust
    /// `set_min_size` raises the interactive-resize floor; `size_limits`
    /// reports it (and every clamp path reads through `size_limits`).
    #[test]
    fn set_min_size_raises_floor() {
        let mut w = window_with_frame();
        w.set_min_size(Point::new(60, 20));
        let (min, max) = w.size_limits(Point::new(100, 40));
        assert_eq!(min, Point::new(60, 20), "raised floor");
        assert_eq!(max, Point::new(100, 40), "max is still the owner size");
    }

    /// The floor can only be raised: arguments below `MIN_WIN_SIZE` clamp up
    /// per axis (title/icons must stay legible).
    #[test]
    fn set_min_size_clamps_to_win_min() {
        let mut w = window_with_frame();
        w.set_min_size(Point::new(4, 2));
        let (min, _) = w.size_limits(Point::new(80, 25));
        assert_eq!(min, Window::MIN_WIN_SIZE, "cannot go below the 16×6 chrome floor");
        // Mixed: x above the floor, y below — per-axis clamp.
        w.set_min_size(Point::new(60, 2));
        let (min, _) = w.size_limits(Point::new(80, 25));
        assert_eq!(min, Point::new(60, 6), "per-axis clamp");
    }

    /// An owner-driven resize (`calc_bounds` trait default) honours a raised
    /// floor, same as `calc_bounds_honours_min_win_size` does for 16×6.
    #[test]
    fn calc_bounds_honours_raised_min() {
        let mut w = window_with_frame(); // bounds 0,0,40,15
        w.set_min_size(Point::new(60, 20));
        w.state_mut().grow_mode = GrowMode {
            hi_x: true,
            hi_y: true,
            ..Default::default()
        };
        // Owner shrinks by (5,5): raw new size (35,10) — below the raised floor.
        let b = View::calc_bounds(&mut w, Point::new(100, 40), Point::new(-5, -5));
        let size = b.b - b.a;
        assert_eq!(size, Point::new(60, 20), "clamped up to the raised floor");
    }
```

And the pump-level drag test in `src/app/program.rs`, after
`drag_move_clamps_to_limits` (~`:7151`):

```rust
    /// A grow-corner drag honours a raised `min_size`: with the floor lifted
    /// to 24×8 via `with_min_size`, dragging the corner toward the origin
    /// stops at 24×8, not the built-in 16×6. (Restores the C++ behavior where
    /// `TFrame::dragWindow` reads `sizeLimits` virtually.)
    #[test]
    fn drag_grow_clamps_to_raised_min_size() {
        let (mut program, _screen, _clock) = program_with_desktop(80, 25);
        let id = {
            let w = Window::new(Rect::new(2, 1, 32, 13), Some("Edit".into()), 1)
                .with_min_size(Point::new(24, 8));
            program.group_mut().insert(Box::new(w))
        };
        program.with_ctx(|g, ctx| g.set_current(Some(id), SelectMode::Normal, ctx));
        program.out_events.clear();

        // Grab the bottom-right grow corner: size (30,12), so window-local
        // (29,11) → absolute (31,12). (Corner rule: pos.y >= h-1 && pos.x >= w-2.)
        program.out_events.push_back(mouse_down_at(31, 12));
        program.pump_once();
        assert!(win_state(&mut program, id).state.dragging, "grow drag started");

        // Drag far past the minimum: raw size would be ~(2,2).
        program.out_events.push_back(mouse_move_at(3, 2));
        program.pump_once();
        let st = win_state(&mut program, id);
        assert_eq!(
            st.size,
            Point::new(24, 8),
            "size clamps at the raised floor, not 16×6"
        );

        // Clean finish.
        program.out_events.push_back(mouse_up_at(3, 2));
        program.pump_once();
        assert!(!win_state(&mut program, id).state.dragging);
    }
```

- [ ] **Step 2: Run the new tests, verify they fail to compile** (no
  `set_min_size`/`with_min_size`/`MIN_WIN_SIZE` yet):

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test -p tvision-rs -j 4 min_size 2>&1 | tail -20
```
Expected: compile errors `no method named 'set_min_size'` /
`no associated item named 'MIN_WIN_SIZE'`.

- [ ] **Step 3: Implement.** Four edits in `src/window/window.rs`:

(a) Field — add after `restore_rect: Option<Rect>` (`:174`), before `bordered`:

```rust
    /// The interactive-resize floor. Every size clamp (mouse corner drag,
    /// keyboard resize, ZOOM/restore, owner-driven `calc_bounds`) reads this
    /// through [`size_limits`](View::size_limits). Defaults to
    /// [`Self::MIN_WIN_SIZE`] (16×6) and can only be raised, never lowered
    /// below it ([`set_min_size`](Self::set_min_size) clamps).
    min_size: Point,
```

(b) Assoc const — first item inside `impl Window` (before `pub fn new`):

```rust
    /// The absolute minimum a framed window may shrink to and still render its
    /// title bar and corner icons legibly. Ports `minWinSize` (`twindow.cpp:30`).
    /// The default [`size_limits`](View::size_limits) minimum; a larger floor
    /// can be set with [`set_min_size`](Self::set_min_size).
    pub const MIN_WIN_SIZE: Point = Point::new(16, 6);
```

(c) Init — in the `Window { .. }` literal at the end of `new` (~`:270`), add
`min_size: Self::MIN_WIN_SIZE,` after `restore_rect: None,`.

(d) Setter + builder — after `set_drag_mode` (`:426-428`) add the setter;
after `with_drag_mode` (`:449-452`) add the builder:

```rust
    /// Raise the window's interactive-resize floor. The argument is clamped up
    /// to at least [`Self::MIN_WIN_SIZE`] per axis (title/icons must stay
    /// legible), so this can only grow the minimum. Consulted by every
    /// size-clamp path — mouse corner drag, keyboard resize, ZOOM/restore,
    /// owner-driven resize — via [`size_limits`](View::size_limits).
    ///
    /// # Turbo Vision heritage
    /// In C++ a window subclass raises its floor by overriding the virtual
    /// `sizeLimits` (e.g. `TFileDialog`, `TEditWindow`), which every clamp
    /// path calls virtually. Embed-and-delegate composition (D2) has no upward
    /// dispatch, so the wrapper pushes its floor down into the window instead.
    pub fn set_min_size(&mut self, min: Point) {
        self.min_size = Point::new(
            min.x.max(Self::MIN_WIN_SIZE.x),
            min.y.max(Self::MIN_WIN_SIZE.y),
        );
    }

    /// Builder form of [`set_min_size`](Self::set_min_size).
    pub fn with_min_size(mut self, min: Point) -> Self {
        self.set_min_size(min);
        self
    }
```

(e) `size_limits` (`:1678-1681`) — return the field:

```rust
    fn size_limits(&self, owner_size: Point) -> (Point, Point) {
        let (_min, max) = self.group.size_limits(owner_size);
        (self.min_size, max)
    }
```

- [ ] **Step 4: Doc-comment cleanup** (same file):
  - `size_limits` override doc (`:1668-1670`): change "the minimum forced to 16
    columns × 6 rows" to "the minimum forced to [`min_size`](Self::set_min_size)
    (defaults to [`Self::MIN_WIN_SIZE`], 16×6)". Keep the paragraph about the
    `#[delegate]` skip-list interaction unchanged.
  - `maximize` doc (`:790`): "min = 16×6" → "min = `min_size`, default 16×6".
  - Test comment `:1848`: "min forced to the window minimum {16, 6}" → "min
    defaults to `MIN_WIN_SIZE` {16, 6}".

- [ ] **Step 5: Run the tests, verify they pass:**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test -p tvision-rs -j 4 min_size 2>&1 | tail -10
cargo test -p tvision-rs -j 4 window:: 2>&1 | tail -5
cargo test -p tvision-rs -j 4 drag 2>&1 | tail -5
```
Expected: all PASS (new tests: `set_min_size_raises_floor`,
`set_min_size_clamps_to_win_min`, `calc_bounds_honours_raised_min`,
`drag_grow_clamps_to_raised_min_size`); no existing window/drag test regresses.

- [ ] **Step 6: Gate + commit**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo fmt --all --check && cargo clippy --workspace --all-targets -j 4 -- -D warnings
git add src/window/window.rs src/app/program.rs
git commit -m "feat(window): settable interactive-resize floor (min_size)

Window::size_limits now returns a per-window min_size field (default
MIN_WIN_SIZE 16x6, raisable only) instead of a hardcoded literal. Every
clamp path (corner drag, keyboard resize, zoom/restore, owner-driven
calc_bounds) reads through size_limits, so one field governs them all —
restoring the C++ behavior where TFrame::dragWindow reads sizeLimits
virtually and subclass floors govern interactive resizes.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: `Dialog` — forward `set_min_size`/`with_min_size`

**Files:**
- Modify: `src/dialog/dialog.rs` (setters `:145-195`, tests module `:271+`)

**Interfaces:**
- Consumes: `Window::set_min_size(Point)` from Task 1; `Dialog`'s embedded
  field is `window`; `Dialog::new(bounds: Rect, title: Option<String>)`.
- Produces: `Dialog::set_min_size(&mut self, min: Point)` and
  `Dialog::with_min_size(mut self, min: Point) -> Self` (public). Task 3 calls
  `dialog.set_min_size(...)`.

- [ ] **Step 1: Write the failing test** in `mod tests` of
  `src/dialog/dialog.rs`:

```rust
    /// `set_min_size` forwards to the embedded window: the raised floor is
    /// visible through the dialog's (delegated) `size_limits`.
    #[test]
    fn set_min_size_forwards_to_window() {
        let mut d = Dialog::new(Rect::new(0, 0, 40, 15), Some("T".into()));
        d.set_min_size(Point::new(60, 20));
        let (min, _) = View::size_limits(&d, Point::new(100, 40));
        assert_eq!(min, Point::new(60, 20), "setter forwards");

        let d2 = Dialog::new(Rect::new(0, 0, 40, 15), None).with_min_size(Point::new(50, 21));
        let (min2, _) = View::size_limits(&d2, Point::new(100, 40));
        assert_eq!(min2, Point::new(50, 21), "builder forwards");
    }
```

(If `Point`/`View`/`Rect` are not already in the test module's imports, add
them to the existing `use` lines.)

- [ ] **Step 2: Run it, verify compile failure:**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test -p tvision-rs -j 4 set_min_size_forwards 2>&1 | tail -10
```
Expected: `no method named 'set_min_size' found for struct 'Dialog'`.

- [ ] **Step 3: Implement** — after `set_drag_mode` (`:168-170`) add:

```rust
    /// Raise the dialog's interactive-resize floor (forwards to the embedded
    /// [`Window`]; see [`Window::set_min_size`]).
    pub fn set_min_size(&mut self, min: Point) {
        self.window.set_min_size(min);
    }
```

and after `with_drag_mode` (`:191-195`) add:

```rust
    /// Builder form of [`set_min_size`](Self::set_min_size).
    pub fn with_min_size(mut self, min: Point) -> Self {
        self.set_min_size(min);
        self
    }
```

(`Point` is already imported in dialog.rs via `crate::view`; verify, add if not.)

- [ ] **Step 4: Run tests, verify pass:**

```bash
cargo test -p tvision-rs -j 4 set_min_size_forwards 2>&1 | tail -5
```
Expected: PASS.

- [ ] **Step 5: Gate + commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -j 4 -- -D warnings
git add src/dialog/dialog.rs
git commit -m "feat(dialog): forward set_min_size/with_min_size to the embedded window

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: Convert `FileDialog` + `ChDirDialog`

**Files:**
- Modify: `src/dialog/filedlg.rs`:
  - `FileDialog` struct doc `:1910-1918`
  - `FileDialog::new` `:1993+` (after the growable-flags block ending `:2019`)
  - `FileDialog` `#[delegate]` skip list `:2226-2238`
  - screen-relative resize clamp `:2284-2288`
  - `FileDialog::size_limits` override `:2337-2343` (delete)
  - `ChDirDialog::new` `:2581+` (after the growable-flags block ending `:2599`)
  - `ChDirDialog` `#[delegate]` skip list `:2792` region
  - `ChDirDialog::size_limits` override `:2850-2856` (delete)
  - tests module (append two tests)

**Interfaces:**
- Consumes: `Dialog::set_min_size(Point)` from Task 2. Both types embed
  `dialog: Dialog`; the delegate macro forwards un-skipped `View` methods to
  it, and `Dialog` forwards `size_limits` to its `Window` (verified: `size_limits`
  is NOT in Dialog's skip list at `dialog.rs:199-207`).
- Produces: no new API. Observable contract preserved:
  `View::size_limits(&FileDialog, o).0 == (49,19)`,
  `View::size_limits(&ChDirDialog, o).0 == (48,18)` — now via the forwarded
  path, honored by interactive drags too.

- [ ] **Step 1: Write the failing tests** in filedlg.rs's `mod tests`
  (note: after this task's implementation these pass via forwarding; right now
  they PASS via the overrides — so write them, watch them pass, then keep them
  as the regression guard through the conversion. This is the one place the
  cycle is refactor-shaped rather than red/green):

```rust
    /// The TFileDialog 49×19 floor, reported through the delegated
    /// `size_limits` (the floor lives on the embedded window via
    /// `set_min_size`, not in an override). Catches a forgotten
    /// `set_min_size` in the constructor.
    #[test]
    fn file_dialog_size_limits_floor() {
        let fd = FileDialog::new("*.*", "Open a File", "~N~ame", 0, 100);
        let (min, max) =
            crate::view::View::size_limits(&fd, crate::view::Point::new(100, 40));
        assert_eq!(min, crate::view::Point::new(49, 19), "TFileDialog floor");
        assert_eq!(max, crate::view::Point::new(100, 40), "max is the owner size");
    }

    /// The TChDirDialog 48×18 floor, same mechanism.
    #[test]
    fn chdir_dialog_size_limits_floor() {
        let cd = ChDirDialog::new(0, 100);
        let (min, _) =
            crate::view::View::size_limits(&cd, crate::view::Point::new(100, 40));
        assert_eq!(min, crate::view::Point::new(48, 18), "TChDirDialog floor");
    }
```

- [ ] **Step 2: Run them (they pass pre-conversion — baseline):**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test -p tvision-rs -j 4 _size_limits_floor 2>&1 | tail -5
```
Expected: 2 PASS.

- [ ] **Step 3: Convert `FileDialog`:**

(a) In `FileDialog::new`, directly after the growable-flags block (`:2015-2019`):

```rust
        // The TFileDialog::sizeLimits floor (49×19), pushed down into the
        // window (D16): every clamp path incl. the interactive drag reads it.
        dialog.set_min_size(crate::view::Point::new(49, 19));
```

(b) Delete the `size_limits` override (`:2337-2343`, the whole method) and
remove `size_limits,` from the `#[delegate]` skip list (`:2235`). The macro
then forwards `size_limits` → `Dialog` → `Window`, which carries the floor.
`calc_bounds` STAYS in the skip list (trait default routes through the
forwarded `size_limits` — same floor).

(c) Rewrite the manual clamp (`:2284-2288`). It stays — C++ applies this
resize via `locate(bounds)` (`tfildlg.cpp:166`) which clamps through
`sizeLimits`, while our `ctx.request_bounds` deferred applies a raw
`change_bounds` with no clamp — but it now reads the floor from the single
source of truth:

```rust
                // Apply the size floor. C++ applies this resize via locate(),
                // which clamps through sizeLimits; the ChangeBounds deferred
                // applies raw change_bounds, so clamp here by hand — reading
                // the floor from size_limits (single source: the window's
                // min_size, 49×19).
                let (floor, _) = crate::view::View::size_limits(self, screen_size);
                let w = (bounds.b.x - bounds.a.x).max(floor.x);
                let h = (bounds.b.y - bounds.a.y).max(floor.y);
```

(the two lines `bounds.b.x = bounds.a.x + w;` / `bounds.b.y = bounds.a.y + h;`
that follow stay unchanged).

(d) Update the struct doc (`:1912-1918`): the override list "overrides only
`handle_event`, `size_limits`, `reset_current`, and `as_any_mut`" drops
`size_limits`; replace the `calc_bounds`/49×19 sentence with: "The 49×19
minimum (`TFileDialog::sizeLimits`) is pushed into the embedded window via
`set_min_size` at construction (D16), so every clamp path — including the
interactive corner drag — honors it through the delegated `size_limits`."

- [ ] **Step 4: Convert `ChDirDialog`** (same three moves):

(a) In `ChDirDialog::new`, directly after its growable-flags block
(`:2595-2599`):

```rust
        // The TChDirDialog::sizeLimits floor (48×18), pushed down into the
        // window (D16).
        dialog.set_min_size(crate::view::Point::new(48, 18));
```

(b) Delete the `size_limits` override (`:2850-2856`) and remove `size_limits,`
from ChDirDialog's `#[delegate]` skip list (`:2792` region). `calc_bounds`
stays skipped.

(c) Update ChDirDialog's struct/impl doc if it mentions a `size_limits`
override (mirror the FileDialog wording).

- [ ] **Step 5: Run the floor tests + the filedlg suite, verify pass:**

```bash
cargo test -p tvision-rs -j 4 _size_limits_floor 2>&1 | tail -5
cargo test -p tvision-rs -j 4 filedlg 2>&1 | tail -5
```
Expected: all PASS (the floor tests now exercise the forwarded path; deleting
`set_min_size` from either constructor would fail them with min == 16×6).

- [ ] **Step 6: Gate + commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -j 4 -- -D warnings
git add src/dialog/filedlg.rs
git commit -m "fix(filedlg): FileDialog/ChDirDialog minimums honored by interactive drag

Replace the size_limits overrides (invisible to the drag path under
static dispatch) with set_min_size pushed into the embedded window at
construction. The screen-relative resize clamp stays (it emulates the
C++ locate() clamp the raw ChangeBounds deferred lacks) but reads the
floor from size_limits instead of duplicating 49/19 literals.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: Convert `EditWindow`

**Files:**
- Modify: `src/widgets/editor.rs` (`EditWindow::new` `:2879+`, `#[delegate]`
  skip list `:2929-2941`, `size_limits` override `:2994-3008` (delete),
  module const near `EditWindow`, existing test `edit_window_size_limits_min`
  `:4369-4375` stays untouched)
- Modify: `src/app/program.rs` (one pump-level drag test, after
  `drag_grow_clamps_to_raised_min_size` from Task 1)

**Interfaces:**
- Consumes: `Window::set_min_size(Point)` from Task 1 (EditWindow embeds
  `window: Window` directly). `EditWindow::new(bounds: Rect, file_name:
  Option<std::path::PathBuf>, number: i16)`.
- Produces: `pub(crate) const MIN_EDIT_WIN_SIZE: Point = Point::new(24, 6);`
  (module-level in `editor.rs`, mirroring C++ `minEditWinSize`,
  `teditwnd.cpp:29`). Do NOT intra-doc-link it from public rustdoc (pub(crate)
  target = rustdoc warning that fails `xtask docs`); use a code span.

- [ ] **Step 1: Write the failing pump-level drag test** in
  `src/app/program.rs`, after `drag_grow_clamps_to_raised_min_size`:

```rust
    /// A converted consumer end-to-end: `EditWindow`'s 24×6 floor
    /// (`minEditWinSize`) now lives on the embedded window via `set_min_size`,
    /// so a grow-corner drag stops at 24×6 — previously it fell through to the
    /// plain-window 16×6 because the drag read `Window::size_limits`
    /// statically.
    #[test]
    fn editwindow_drag_grow_honours_min_edit_win_size() {
        use crate::widgets::EditWindow;
        let (mut program, _screen, _clock) = program_with_desktop(80, 25);
        let id = {
            let w = EditWindow::new(Rect::new(2, 1, 42, 16), None, 1);
            program.group_mut().insert(Box::new(w))
        };
        program.with_ctx(|g, ctx| g.set_current(Some(id), SelectMode::Normal, ctx));
        program.out_events.clear();

        // Bottom-right grow corner: size (40,15) → window-local (39,14) →
        // absolute (41,15).
        program.out_events.push_back(mouse_down_at(41, 15));
        program.pump_once();
        assert!(win_state(&mut program, id).state.dragging, "grow drag started");

        // Drag far past the minimum.
        program.out_events.push_back(mouse_move_at(3, 2));
        program.pump_once();
        let st = win_state(&mut program, id);
        assert_eq!(
            st.size,
            Point::new(24, 6),
            "EditWindow floor is minEditWinSize 24×6, not 16×6"
        );

        program.out_events.push_back(mouse_up_at(3, 2));
        program.pump_once();
    }
```

- [ ] **Step 2: Run it, verify it FAILS** (drag still clamps to 16×6 because
  the override is invisible to the drag path — this is the bug):

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test -p tvision-rs -j 4 editwindow_drag_grow 2>&1 | tail -10
```
Expected: FAIL with `left: Point { x: 16, y: 6 }` vs `right: Point { x: 24, y: 6 }`.

- [ ] **Step 3: Implement in `src/widgets/editor.rs`:**

(a) Module-level const, directly above `impl EditWindow` (`:2864`):

```rust
/// The `EditWindow` interactive-resize floor: frame + indicator + scroll bars
/// + a few text columns. Ports `minEditWinSize` (`teditwnd.cpp:29`).
pub(crate) const MIN_EDIT_WIN_SIZE: Point = Point::new(24, 6);
```

(b) In `EditWindow::new` (`:2879`), directly after
`let mut window = crate::window::Window::new(bounds, Some(title), number);`:

```rust
        // The TEditWindow::sizeLimits floor, pushed down into the window
        // (D16): every clamp path incl. the interactive drag reads it.
        window.set_min_size(MIN_EDIT_WIN_SIZE);
```

(c) Delete the `size_limits` override (`:2994-3008`, the whole method incl.
its doc comment) and remove `size_limits,` from the `#[delegate]` skip list
(`:2938`). `calc_bounds` stays in the skip list (trait default routes through
the now-forwarded `size_limits` → the window's floor).

(d) If the `EditWindow` struct doc mentions overriding `size_limits`, reword
to the push-down phrasing (mirror Task 3's FileDialog wording, with
`MIN_EDIT_WIN_SIZE` in a plain code span, not an intra-doc link).

- [ ] **Step 4: Run the tests, verify pass:**

```bash
cargo test -p tvision-rs -j 4 editwindow_drag_grow 2>&1 | tail -5
cargo test -p tvision-rs -j 4 edit_window_size_limits_min 2>&1 | tail -5
cargo test -p tvision-rs -j 4 editor 2>&1 | tail -5
```
Expected: all PASS — `edit_window_size_limits_min` (`editor.rs:4371`) must
pass UNCHANGED (same observable contract through the forwarded path).

- [ ] **Step 5: Gate + commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -j 4 -- -D warnings
git add src/widgets/editor.rs src/app/program.rs
git commit -m "fix(editor): EditWindow 24x6 minimum honored by interactive drag

Push minEditWinSize into the embedded window via set_min_size and drop
the size_limits override the drag path could not see.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 5: Bookkeeping — PORTING-GUIDE D16, Appendix A, CHANGELOG

**Files:**
- Modify: `docs/PORTING-GUIDE.md` (new `## D16` section after D15 `:757-780`;
  one new row in Appendix A table `:906+`)
- Modify: `CHANGELOG.md` (`## Unreleased` → `### New` + `### Fixed`)

**Interfaces:**
- Consumes: the landed API names from Tasks 1–4 (`Window::MIN_WIN_SIZE`,
  `set_min_size`/`with_min_size`, `MIN_EDIT_WIN_SIZE`).
- Produces: deviation D16 (referenced by the code comments added in Tasks 3–4).

- [ ] **Step 1: Add D16** after the D15 section (before the `---` that
  precedes Appendix A), following the Baseline → Deviation → Integration
  format:

```markdown
## D16 — virtual `sizeLimits` floor → settable `min_size` (push-down) · *minor*

**Baseline.** Every interactive clamp path reads the window minimum through a
**virtual** `sizeLimits` call — `TFrame::dragWindow` calls
`owner->sizeLimits(min, max)` (`tframe.cpp:138`); `TWindow::zoom`,
`TView::locate`/`dragView` and group resizing do the same — so a subclass
override (`TFileDialog` 49×19, `TChDirDialog` 48×18, `TEditWindow`
`minEditWinSize` 24×6) governs the mouse drag, keyboard resize, and zoom.
`TWindow::sizeLimits` itself forces `min = minWinSize` ({16,6}, a file-level
const, `twindow.cpp:30`).

**Deviation.** D2 embed-and-delegate has no upward virtual dispatch: the inner
`Window` cannot see a composing wrapper's `size_limits`, so wrapper overrides
were invisible to the drag path (a lost-fidelity bug). Instead the floor is a
**settable field**: `Window::min_size`, default `Window::MIN_WIN_SIZE` (16×6),
raised via `set_min_size`/`with_min_size` (surfaced on `Dialog` too). A wrapper
*pushes its floor down* into the window once at construction instead of the
clamp paths *pulling* it up through virtual dispatch. `set_min_size` clamps its
argument up to `MIN_WIN_SIZE` per axis (chrome legibility) — a C++ override
could in principle go lower; no upstream subclass does. Converted consumers
keep **no** `size_limits` override; the delegate macro forwards it to the
embedded window.

**Integration.** `Window::size_limits` returns `(self.min_size, owner max)`;
`start_drag`, the keyboard-resize capture, `locate` (ZOOM/restore/fullscreen),
and the `calc_bounds` trait default all read through it, so one field governs
every clamp. Converted: `FileDialog` (49×19), `ChDirDialog` (48×18),
`EditWindow` (24×6, `MIN_EDIT_WIN_SIZE`). One seam stays explicit:
`FileDialog`'s screen-relative resize applies bounds via the raw `ChangeBounds`
deferred (no clamp), so it clamps to `View::size_limits(self, …).0` inline —
the C++ `locate()` clamp, performed by hand.

---
```

- [ ] **Step 2: Add the Appendix A row** (keep table order/format; append near
  the other view-layer rows):

```markdown
| `minWinSize` / virtual `sizeLimits` override | `Window::MIN_WIN_SIZE` + `set_min_size`/`with_min_size` (push-down floor) | D16 |
```

- [ ] **Step 3: Roll `CHANGELOG.md`** under `## Unreleased`:

Under `### New`:

```markdown
- `Window::set_min_size`/`with_min_size` (also on `Dialog`) — a raisable
  interactive-resize floor (default `Window::MIN_WIN_SIZE`, 16×6). Every clamp
  path (mouse corner drag, keyboard resize, zoom/restore, owner-driven resize)
  reads it through `size_limits` (D16).
```

Under `### Fixed`:

```markdown
- `FileDialog` (49×19), `ChDirDialog` (48×18) and `EditWindow` (24×6) minimum
  sizes are now enforced during interactive drag-resize; previously the drag
  read the plain-window 16×6 floor and ignored their `size_limits` overrides.
```

- [ ] **Step 4: Verify the docs build** (guide doctest gate — D16 has no Rust
  blocks, but run the gate to be safe):

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo xtask test 2>&1 | tail -5
```
Expected: PASS (if `cargo xtask test` is unavailable in this checkout state,
run `cargo test --workspace -j 4 -- --test-threads=4` instead and note it).

- [ ] **Step 5: Full gate + commit**

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -j 4 -- -D warnings
cargo test --workspace -j 4 -- --test-threads=4 2>&1 | tail -5
git add docs/PORTING-GUIDE.md CHANGELOG.md
git commit -m "docs: D16 — settable min_size replaces virtual sizeLimits floor; roll CHANGELOG

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Final: PR

- [ ] Push the branch and open a PR against `main` (protected; `test` check
  required). Include the spec + plan files in the PR
  (`git add docs/superpowers/specs/2026-07-03-window-settable-min-size-design.md docs/superpowers/plans/2026-07-03-window-settable-min-size.md`
  in whichever commit is convenient, e.g. amend Task 5's or a separate
  `docs:` commit).

```bash
git push -u origin feat/window-settable-min-size
gh pr create --title "Window settable minimum size: restore virtual-sizeLimits floor behavior (D16)" --body "$(cat <<'EOF'
## Summary
- `Window::set_min_size`/`with_min_size` + `Window::MIN_WIN_SIZE`: the interactive-resize floor becomes a settable per-window field read by every clamp path through `size_limits` (mouse corner drag, keyboard resize, zoom/restore, owner-driven resize).
- Restores C++ fidelity: upstream reads `sizeLimits` **virtually** (`TFrame::dragWindow`), so subclass floors govern drags; the port's static dispatch had silently lost that. Recorded as deviation **D16** (push-down floor instead of pull-up virtual dispatch, forced by D2).
- Converts the three in-repo victims: `FileDialog` (49×19), `ChDirDialog` (48×18), `EditWindow` (24×6) — their minimums are now enforced during interactive drag-resize.
- Surfaced on `Dialog` (forwarder), mirroring `set_flags`.

Spec: `docs/superpowers/specs/2026-07-03-window-settable-min-size-design.md`

## Test plan
- New: `set_min_size_raises_floor`, `set_min_size_clamps_to_win_min`, `calc_bounds_honours_raised_min`, pump-level `drag_grow_clamps_to_raised_min_size`, `editwindow_drag_grow_honours_min_edit_win_size`, `set_min_size_forwards_to_window`, `file_dialog_size_limits_floor`, `chdir_dialog_size_limits_floor`
- Existing `title_and_size_limits`, `calc_bounds_honours_min_win_size`, `edit_window_size_limits_min` pass unchanged
- `cargo fmt --check` + `clippy -D warnings` + full workspace tests green

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```
