# Splitter Resize Unification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the splitter's bespoke F6 reconfig mode with the window's keyboard-resize mode (one modal capture where **Tab cycles targets** window↔dividers), let the mouse drag any movable divider with a "being-moved" color, and make divider color match the owning window frame.

**Architecture:** The splitter keeps its `reconfig`/`dragging`/`saved_weights` fields but exposes a `pub(crate)` *resize-session* API driven from outside. The window owns the resize entry (`Command::RESIZE`): it begins a session on its body splitter, enumerates divider targets, and pushes one extended `KeyboardResizeCapture` that cycles between the window and each divider. The capture reaches dividers (which it knows only by `ViewId`) through a new `Deferred::SplitterDivider` broker op applied by the pump — the established D3 sibling-broker pattern.

**Tech Stack:** Rust (cargo workspace `rstv` + `rstv-macros`), `insta` snapshot tests on `HeadlessBackend`.

**Spec:** `docs/superpowers/specs/2026-06-14-splitter-resize-unification-design.md`

**Standing commands** (run from repo root; max 4 cores per global policy):
```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test  -p rstv --jobs 4
cargo clippy --workspace --all-targets --jobs 4 -- -D warnings
cargo fmt --all --check
cargo insta accept   # after eyeballing new/changed snapshots
```

**Files touched**
- `src/widgets/splitter/mod.rs` — resize-session API; color rule; mouse gate; remove F6 (Tasks 1, 2)
- `src/view/context.rs` — `Deferred::SplitterDivider` + `DividerOp` + `Context::splitter_divider` (Task 3)
- `src/app/program.rs` — broker apply arm (Task 3)
- `src/window/window.rs` — extended `KeyboardResizeCapture`, target enumeration, RESIZE enablement (Task 4)
- `examples/splitter.rs`, `examples/gallery.rs`, `docs/book/src/apps/windows.md` — drop F6-resize wording (Task 5)

---

### Task 0: Feature branch

- [ ] **Step 1: Branch from main** (we are on the default branch; never commit the feature directly to main)

```bash
git checkout -b feat-splitter-resize-unification
```

---

### Task 1: Splitter resize-session API + remove F6

**Files:**
- Modify: `src/widgets/splitter/mod.rs` (add session API ~after line 527; remove `enter_reconfig`/`exit_reconfig`/`first_movable_divider`/`step_selection` at 529-578; gut the F6/reconfig `KeyDown` block at 723-772; update unit tests at 1039-1075)

The session API replaces the four private reconfig fns. `reconfig: Option<usize>` now means "the active resize-target divider" (set from outside); `saved_weights` is the Esc snapshot; both fields and `dragging` stay.

- [ ] **Step 1: Write the failing tests** (append to the `divider_tests` mod near line 1039; first delete the old `reconfig_arrow_moves_selected_divider`, `reconfig_esc_restores`, and `reconfig_snapshot_highlights_all_dividers` tests — they exercise the removed API)

```rust
#[test]
fn begin_resize_session_lists_movable_dividers_and_snapshots() {
    // Two panes → one divider (index 0), movable by default (Line style).
    let mut sp = three_pane_cols(); // helper below; 3 panes → dividers 0 and 1
    let targets = sp.begin_resize_session();
    let id = sp.state().id().unwrap();
    assert_eq!(
        targets,
        vec![(id, 0, Orientation::Cols), (id, 1, Orientation::Cols)],
        "movable dividers enumerated in axis order with this splitter's id"
    );
    assert_eq!(sp.reconfig, None, "begin does NOT auto-select a divider");
    assert_eq!(sp.saved_weights.len(), sp.slots.len(), "weights snapshotted");
}

#[test]
fn nudge_divider_moves_then_end_commit_keeps_position() {
    let mut sp = three_pane_cols();
    sp.begin_resize_session();
    let before = sp.divider_axis_pos(0).unwrap();
    sp.nudge_divider(0, 1);
    let after = sp.divider_axis_pos(0).unwrap();
    assert_eq!(after, before + 1, "nudge moves the divider one cell along the axis");
    sp.end_resize_session(true); // commit
    assert_eq!(sp.divider_axis_pos(0).unwrap(), after, "commit keeps the new position");
    assert!(sp.saved_weights.is_empty(), "session ended");
    assert_eq!(sp.reconfig, None);
}

#[test]
fn end_resize_session_cancel_restores_weights() {
    let mut sp = three_pane_cols();
    sp.begin_resize_session();
    let before = sp.divider_axis_pos(0).unwrap();
    sp.nudge_divider(0, 2);
    assert_ne!(sp.divider_axis_pos(0).unwrap(), before);
    sp.end_resize_session(false); // Esc / cancel
    assert_eq!(sp.divider_axis_pos(0).unwrap(), before, "cancel restores pre-session position");
}

#[test]
fn set_active_divider_drives_reconfig_field() {
    let mut sp = three_pane_cols();
    sp.begin_resize_session();
    sp.set_active_divider(Some(1));
    assert_eq!(sp.reconfig, Some(1));
    sp.set_active_divider(None);
    assert_eq!(sp.reconfig, None);
}
```

Add this helper inside `divider_tests` if one does not already exist (reuse an existing builder if the test module already has an equivalent 3-pane factory — check first to stay DRY):

```rust
fn three_pane_cols() -> Splitter {
    let mut sp = Splitter::cols()
        .pane(Box::new(crate::StaticText::new("a")), Constraints::default())
        .pane(Box::new(crate::StaticText::new("b")), Constraints::default())
        .pane(Box::new(crate::StaticText::new("c")), Constraints::default());
    // Lay out in a known box so divider_axis_pos is well-defined.
    sp.change_bounds(Rect::new(0, 0, 30, 5));
    sp
}
```

- [ ] **Step 2: Run the tests — expect compile failure** (methods don't exist yet)

```bash
cargo test -p rstv --jobs 4 splitter::divider_tests 2>&1 | tail -20
```
Expected: FAIL — `no method named begin_resize_session` etc.

- [ ] **Step 3: Add the session API** (insert where `enter_reconfig` was, ~line 529, after deleting the four old fns)

```rust
// -- keyboard resize session (driven by the window's resize capture via the
//    SplitterDivider broker; see docs/.../splitter-resize-unification-design.md)

/// Begin a keyboard resize session: snapshot every slot weight (for Esc
/// restore) and clear the active-target highlight (the capture sets it via
/// [`set_active_divider`]). Recurses into pane children that are themselves
/// splitters so a nested grid resizes too. Returns every movable divider as
/// `(splitter_id, divider_index, orientation)` in depth-first axis order — this
/// splitter's dividers first, then each sub-splitter's.
pub(crate) fn begin_resize_session(&mut self) -> Vec<(ViewId, usize, Orientation)> {
    self.saved_weights = self.slots.iter().map(|s| s.weight).collect();
    self.reconfig = None;
    let mut out = Vec::new();
    if let Some(id) = self.state().id() {
        for i in 0..self.slots.len().saturating_sub(1) {
            if self.style_of(i).movable_in_reconfig() {
                out.push((id, i, self.orientation));
            }
        }
    }
    let ids = self.group.child_ids_in_order();
    for cid in ids {
        if let Some(sub) = self
            .group
            .child_mut(cid)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<Splitter>())
        {
            out.extend(sub.begin_resize_session());
        }
    }
    out
}

/// Set (or clear) which divider is the active resize target. Drives the
/// `FrameDragging` highlight in `draw_dividers`. Per-splitter (not recursive):
/// the broker addresses each splitter by id.
pub(crate) fn set_active_divider(&mut self, sel: Option<usize>) {
    self.reconfig = sel;
}

/// Move divider `index` by `delta` cells along the split axis, then re-flow
/// children synchronously (no `ctx` at broker-apply time — `resolve_layout_local`
/// writes child bounds directly).
pub(crate) fn nudge_divider(&mut self, index: usize, delta: i32) {
    if let Some(p) = self.divider_axis_pos(index) {
        self.drag_divider_to(index, p + delta);
    }
    self.resolve_layout_local();
}

/// End the resize session. On `!commit` restore the snapshotted weights (Esc).
/// Clears the highlight and the snapshot. Per-splitter (not recursive).
pub(crate) fn end_resize_session(&mut self, commit: bool) {
    if !commit && self.saved_weights.len() == self.slots.len() {
        for (s, w) in self.slots.iter_mut().zip(&self.saved_weights) {
            s.weight = *w;
        }
    }
    self.reconfig = None;
    self.saved_weights.clear();
    self.resolve_layout_local();
}
```

- [ ] **Step 4: Remove the F6 / inline reconfig key handling** in `handle_event` (the `KeyDown` arm, lines 723-772). Replace the whole `Event::KeyDown(k) => { ... }` arm with a plain delegation — keyboard resize is now owned by the window capture, and F6 must fall through to `cmNext`:

```rust
Event::KeyDown(_) => {
    // No splitter-owned keys: keyboard divider resize is driven by the
    // window's resize capture (Command::RESIZE → Tab cycles dividers). F6 etc.
    // fall through to the normal group/program handling (e.g. cmNext).
    self.group.handle_event(ev, ctx);
}
```

Remove the now-unused imports if `Key` is no longer referenced in this file (let clippy/fmt tell you).

- [ ] **Step 5: Run the new tests + full splitter tests — expect PASS**

```bash
cargo test -p rstv --jobs 4 splitter 2>&1 | tail -20
```
Expected: PASS (the three deleted tests are gone; four new ones pass).

- [ ] **Step 6: Clippy + fmt**

```bash
cargo clippy --workspace --all-targets --jobs 4 -- -D warnings 2>&1 | tail -20
cargo fmt --all --check
```
Expected: clean (fix any dead-code warning from removed fns — they should all be deleted).

- [ ] **Step 7: Commit**

```bash
git add src/widgets/splitter/mod.rs
git commit -m "feat(splitter): resize-session API; drop F6 reconfig

Replace the bespoke F6 keyboard reconfig with a pub(crate) session API
(begin/set_active/nudge/end_resize_session) driven externally by the
window resize capture. F6 now falls through to cmNext.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: Divider color matches frame state + mouse drags any movable divider

**Files:**
- Modify: `src/widgets/splitter/mod.rs` — `draw_dividers` (176-220), `collect_frame_marks` (321-334), `draw_interior_crossings` (600-620), `put_crossing` (674-679), `MouseDown` gate (776-795)
- Test: same file's `divider_tests` (snapshot tests via `assert_snapshot!`)

Rule (from spec §1): a divider is *moving* iff `self.dragging == Some(i) || self.reconfig == Some(i)`. Color: moving → `FrameDragging`; else window-active → `FrameActive`; else `FramePassive`. Glyph weight: **always single-line** (drop every `Weight::Double`/`frame_v_d`/`frame_h_d` branch keyed on `reconfig`). A divider draws its full line iff `Line` style **or** it is moving (so a Hidden divider appears only while moved).

To make the color rule directly testable without fabricating an `Active` `Context` in a unit test, extract the decision into a tiny pure helper and unit-test *that*; cover the visual side with snapshots of the default (inactive) and mid-drag cases (both reachable without a ctx — `dragging` is a plain field).

- [ ] **Step 1: Write failing tests** (append to `divider_tests`)

```rust
#[test]
fn divider_role_rule() {
    use crate::theme::Role;
    // moving wins over everything.
    assert_eq!(divider_role(true, true), Role::FrameDragging);
    assert_eq!(divider_role(true, false), Role::FrameDragging);
    // not moving: active window -> FrameActive, else FramePassive.
    assert_eq!(divider_role(false, true), Role::FrameActive);
    assert_eq!(divider_role(false, false), Role::FramePassive);
}

#[test]
fn divider_inactive_is_single_line_passive() {
    let mut sp = three_pane_cols(); // default state: not active
    let snap = render_splitter(&mut sp, 30, 5);
    assert_snapshot!("divider_inactive", snap);
}

#[test]
fn divider_dragging_highlight_on_mouse() {
    let mut sp = three_pane_cols();
    sp.dragging = Some(0); // simulate mid-drag (plain field, no ctx needed)
    let snap = render_splitter(&mut sp, 30, 5);
    assert_snapshot!("divider_dragging_highlight_on_mouse", snap);
}
```

Reuse the existing snapshot helper if `divider_tests` already has one (a `render_*` building a `HeadlessBackend` — check the top of the test module and `src/screen/snapshot.rs`); only add `render_splitter` if no equivalent exists, and reuse the same pane view type existing splitter tests use for `three_pane_cols`. The active-window *visual* is covered by the Task 4 integration (a selected window renders its body splitter Active); here the pure `divider_role` unit test covers the active branch.

- [ ] **Step 2: Run — expect FAIL** (snapshots not yet created / colors wrong)

```bash
cargo test -p rstv --jobs 4 splitter::divider_tests::divider_color 2>&1 | tail -20
```
Expected: FAIL (new snapshots / pending).

- [ ] **Step 3: Add the pure role helper + rewrite `draw_dividers`**. First add a free fn near the top of the splitter module (module scope, so the test can call it):

```rust
/// Color role for a divider line: being-moved beats everything, then the line
/// matches the owning window frame (active vs passive).
fn divider_role(moving: bool, active: bool) -> Role {
    if moving {
        Role::FrameDragging
    } else if active {
        Role::FrameActive
    } else {
        Role::FramePassive
    }
}
```
Then replace `draw_dividers` lines 193-205:

```rust
let moving = self.dragging == Some(i) || self.reconfig == Some(i);
let role = divider_role(moving, self.state().state.active);
let st = ctx.style(role);
// Single-line always (match frame COLOR, not weight).
let (line_glyph, nub_glyph) = match self.orientation {
    Orientation::Cols => (frame_v, frame_v),
    Orientation::Rows => (frame_h, frame_h),
};
let draw_full = matches!(style, DividerStyle::Line) || moving;
let draw_handle = matches!(style, DividerStyle::Handle) && !moving;
```
`frame_v_d`/`frame_h_d` are no longer used in this fn — drop them from the destructure at line 180-183 (keep only `frame_v`, `frame_h`) **unless** still needed by the crossing fns below (they won't be after Step 4/5 — remove there too).

- [ ] **Step 4: Single-line the frame-junction marks** (`collect_frame_marks`, lines 321-334): replace the `stem` weight and the `draws_full` test.

```rust
let stem = Weight::Single; // dividers are single-line; matching tee weight unchanged at rest
```
```rust
let moving_i = self.dragging == Some(i) || self.reconfig == Some(i);
let draws_full = matches!(self.style_of(i), DividerStyle::Line) || moving_i;
```

- [ ] **Step 5: Single-line the interior crossings** (`draw_interior_crossings`, lines 600-620 and `put_crossing`, 674-679).

In `draw_interior_crossings` replace the weight + the sub `full` test:
```rust
let weight = Weight::Single;
```
```rust
let full = matches!(sub.style_of(i), DividerStyle::Line)
    || sub.dragging == Some(i)
    || sub.reconfig == Some(i);
```
In `put_crossing`, match the divider color rule (crossings belong to a resting outer divider; they are never the moved target in practice — use active/passive):
```rust
let role = if self.state().state.active {
    Role::FrameActive
} else {
    Role::FramePassive
};
```

- [ ] **Step 6: Mouse drags any movable divider** (`MouseDown` gate, lines 778-781): replace

```rust
let allowed = (style.draggable_live() || self.reconfig.is_some())
    && style.movable_in_reconfig();
```
with
```rust
// Mouse may move ANY divider that is not Locked (including Hidden, which
// becomes visible in FrameDragging while dragged).
let allowed = style.movable_in_reconfig();
```

- [ ] **Step 7: Run tests + accept snapshots after eyeballing**

```bash
cargo test -p rstv --jobs 4 splitter 2>&1 | tail -20
cargo insta accept   # ONLY after visually checking the .snap.new diffs
```
Expected: PASS. Verify the active snapshot shows dividers in the bright frame color and the dragging snapshot shows the divider in `FrameDragging`, single-line throughout. Per memory "snapshot-test-nonzero-origin": eyeball the WHOLE snapshot, not just asserted glyphs.

- [ ] **Step 8: Clippy + fmt + commit**

```bash
cargo clippy --workspace --all-targets --jobs 4 -- -D warnings 2>&1 | tail -20
cargo fmt --all --check
git add src/widgets/splitter/mod.rs src/widgets/splitter/snapshots/ 2>/dev/null; git add -A
git commit -m "feat(splitter): dividers match frame color; mouse drags any movable divider

Divider color follows the owning window frame state (active->FrameActive,
inactive->FramePassive, being-moved->FrameDragging), single-line always.
Mouse can now drag any non-Locked divider (incl. Hidden) and it glows
while dragged.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: `Deferred::SplitterDivider` broker

**Files:**
- Modify: `src/view/context.rs` — add `DividerOp` enum + `Deferred::SplitterDivider` variant (in `enum Deferred`, ~after line 134) + `Context::splitter_divider` helper (alongside the other `request_*` helpers, ~line 1210)
- Modify: `src/app/program.rs` — broker apply arm next to `Deferred::SyncScrollerDelta` (~line 1992)
- Test: `src/app/program.rs` tests module (integration) OR `src/view/context.rs` — see Step 1

- [ ] **Step 1: Write the failing broker test** (add to the `program.rs` tests module; mirror an existing scroller-broker test for setup — build a desktop with a window whose body is a `Splitter`, begin a session, then drive the broker)

```rust
#[test]
fn splitter_divider_broker_nudges_and_ends() {
    use crate::view::context::DividerOp;
    let (mut program, _screen, _clock) = program_with_desktop(40, 12);
    // Insert a window whose body is a 3-pane cols splitter; capture the splitter id.
    let split_id = insert_splitter_window(&mut program); // helper: see note
    // Begin a session via the broker is synchronous in the window; here drive the
    // ops directly through the deferred channel and pump.
    let before = splitter_divider_pos(&mut program, split_id, 0);
    program.push_deferred(Deferred::SplitterDivider {
        splitter: split_id,
        op: DividerOp::Nudge { index: 0, delta: 1 },
    });
    program.apply_deferred(); // drain
    assert_eq!(
        splitter_divider_pos(&mut program, split_id, 0),
        before + 1,
        "broker Nudge moved the divider"
    );
}
```

Implementation note for the agent: reuse whatever test plumbing the scroller-broker tests use to (a) insert a window+body and (b) drain deferred (`program.apply_deferred`/the pump helper — find the exact name in the test module). If a direct `push_deferred` test hook does not exist, drive the op through the window resize path added in Task 4 instead and move this assertion into Task 4. Keep the broker arm itself covered by at least one test.

- [ ] **Step 2: Run — expect compile failure** (`DividerOp`/variant missing)

```bash
cargo test -p rstv --jobs 4 splitter_divider_broker 2>&1 | tail -20
```
Expected: FAIL — unresolved `DividerOp` / `Deferred::SplitterDivider`.

- [ ] **Step 3: Add the enum + variant** in `src/view/context.rs`. Above `pub enum Deferred` add:

```rust
/// One operation on a splitter's keyboard-resize session, brokered by id from
/// the window's resize capture (which knows the splitter only by `ViewId`).
#[derive(Debug, Clone)]
pub enum DividerOp {
    /// Set (or clear) the active-target divider highlight.
    SetActive(Option<usize>),
    /// Move divider `index` by `delta` cells along the split axis.
    Nudge { index: usize, delta: i32 },
    /// End the session; `commit=false` restores the snapshotted weights.
    EndSession { commit: bool },
}
```
Inside `enum Deferred`, after `SetVisible` (line 134), add:
```rust
// -- the splitter keyboard-resize broker (D3 sibling-broker) -------
/// Apply a [`DividerOp`] to the splitter named by `splitter`. The pump
/// resolves it via `group.find_mut(splitter).as_any_mut()` → `Splitter`.
/// Touches the **view-tree** deferred family (same as the scroller ops), so
/// the insertion-order drain stays order-equivalent: a single dispatch never
/// co-queues conflicting ops on the same splitter.
SplitterDivider { splitter: ViewId, op: DividerOp },
```

Add the `Context` helper near the other `request_*` methods (~line 1210):
```rust
/// Broker a [`DividerOp`] to a splitter by id (used by the window resize
/// capture, which cannot touch the splitter inline).
pub fn splitter_divider(&mut self, splitter: ViewId, op: DividerOp) {
    self.deferred.push(Deferred::SplitterDivider { splitter, op });
}
```

- [ ] **Step 4: Add the broker apply arm** in `src/app/program.rs`, right after the `Deferred::SyncScrollerDelta` arm (~line 1992):

```rust
Deferred::SplitterDivider { splitter, op } => {
    use crate::view::context::DividerOp;
    use crate::widgets::Splitter;
    if let Some(sp) = group
        .find_mut(splitter)
        .and_then(|view| view.as_any_mut())
        .and_then(|a| a.downcast_mut::<Splitter>())
    {
        match op {
            DividerOp::SetActive(sel) => sp.set_active_divider(sel),
            DividerOp::Nudge { index, delta } => sp.nudge_divider(index, delta),
            DividerOp::EndSession { commit } => sp.end_resize_session(commit),
        }
    }
}
```
If the deferred drain is a non-exhaustive `match` it will now compile; if exhaustive, this arm satisfies it. Confirm `Splitter` is re-exported from `crate::widgets` (it is — examples use `rstv::Splitter`).

- [ ] **Step 5: Run the test — expect PASS**

```bash
cargo test -p rstv --jobs 4 splitter_divider_broker 2>&1 | tail -20
```
Expected: PASS.

- [ ] **Step 6: Clippy + fmt + commit**

```bash
cargo clippy --workspace --all-targets --jobs 4 -- -D warnings 2>&1 | tail -20
cargo fmt --all --check
git add src/view/context.rs src/app/program.rs
git commit -m "feat(splitter): SplitterDivider deferred broker

Add Deferred::SplitterDivider{splitter, op: DividerOp} + Context helper +
pump apply arm so the window resize capture can drive divider sessions by
id (D3 sibling-broker pattern).

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: Window unified resize capture (Tab cycles window↔dividers)

**Files:**
- Modify: `src/window/window.rs` — `ResizeTarget` enum + extend `KeyboardResizeCapture` (struct 640-656, impl 658-683, `CaptureHandler` 685-819); the `Command::RESIZE` handler (959-990); RESIZE enablement (1085)
- Test: `src/window/window.rs` tests (or `src/app/program.rs` integration tests)

The capture gains a target list. `targets[current]` is the live target; Tab cycles; arrows act on it; Enter/Esc end the whole session.

- [ ] **Step 1: Write the failing integration test** (in the test module that already builds programs; mirror the existing window-resize test if there is one)

```rust
#[test]
fn resize_mode_tab_cycles_to_divider_and_arrow_moves_it() {
    let (mut program, _screen, _clock) = program_with_desktop(48, 16);
    let split_id = insert_splitter_window(&mut program); // body = 3-pane cols splitter
    select_top_window(&mut program); // enables Command::RESIZE
    let before = splitter_divider_pos(&mut program, split_id, 0);

    post_command(&mut program, Command::RESIZE); // enter resize mode (target = Window)
    feed_key(&mut program, Key::Tab);            // → divider 0
    feed_key(&mut program, Key::Right);          // move divider 0 right by 1
    feed_key(&mut program, Key::Enter);          // commit

    assert_eq!(
        splitter_divider_pos(&mut program, split_id, 0),
        before + 1,
        "Tab selected divider 0 and Right moved it one cell"
    );
}

#[test]
fn resize_mode_esc_restores_divider() {
    let (mut program, _screen, _clock) = program_with_desktop(48, 16);
    let split_id = insert_splitter_window(&mut program);
    select_top_window(&mut program);
    let before = splitter_divider_pos(&mut program, split_id, 0);

    post_command(&mut program, Command::RESIZE);
    feed_key(&mut program, Key::Tab);
    feed_key(&mut program, Key::Right);
    feed_key(&mut program, Key::Right);
    feed_key(&mut program, Key::Esc); // cancel

    assert_eq!(splitter_divider_pos(&mut program, split_id, 0), before,
        "Esc restored the pre-mode divider position");
}
```

Agent note: `insert_splitter_window`, `splitter_divider_pos`, `select_top_window`, `post_command`, `feed_key` — reuse existing test helpers where they exist (grep the test module for the pump/feed helper used by window-resize and cmNext tests); add thin wrappers only as needed. `splitter_divider_pos` resolves the splitter via `program.group.find_mut(id).as_any_mut().downcast_mut::<Splitter>()` and calls `divider_axis_pos`.

- [ ] **Step 2: Run — expect FAIL** (Tab/divider path not wired; capture still window-only)

```bash
cargo test -p rstv --jobs 4 resize_mode_ 2>&1 | tail -20
```
Expected: FAIL (divider unchanged / Tab does nothing useful in capture).

- [ ] **Step 3: Add the target type + extend the capture struct** (`src/window/window.rs`, near line 640). Import `Orientation` and `DividerOp`:

```rust
use crate::view::context::DividerOp;
use crate::widgets::splitter::Orientation;

/// A keyboard-resize target: the window itself, or one divider of a splitter
/// in the window body. Dividers are addressed only by id (the capture never
/// touches the `Splitter` inline — it brokers via `DividerOp`).
enum ResizeTarget {
    Window,
    Divider {
        splitter: ViewId,
        index: usize,
        orientation: Orientation,
    },
}
```
Add fields to `KeyboardResizeCapture`:
```rust
    /// Cycle targets: `targets[current]` is live. Tab/Shift+Tab move `current`.
    targets: Vec<ResizeTarget>,
    current: usize,
```

- [ ] **Step 4: Add capture helpers** (`impl KeyboardResizeCapture`, after `apply_delta`)

```rust
fn current_is_window(&self) -> bool {
    matches!(self.targets.get(self.current), Some(ResizeTarget::Window))
}

/// Turn the current target's highlight on/off.
fn set_highlight(&self, on: bool, ctx: &mut Context) {
    match self.targets.get(self.current) {
        Some(ResizeTarget::Window) => {
            ctx.request_set_state(self.window_id, StateFlag::Dragging, on);
        }
        Some(ResizeTarget::Divider { splitter, index, .. }) => {
            ctx.splitter_divider(
                *splitter,
                DividerOp::SetActive(on.then_some(*index)),
            );
        }
        None => {}
    }
}

/// Tab/Shift+Tab: hand the highlight from the old target to the next.
fn cycle(&mut self, forward: bool, ctx: &mut Context) {
    let n = self.targets.len();
    if n < 2 {
        return;
    }
    self.set_highlight(false, ctx);
    self.current = if forward {
        (self.current + 1) % n
    } else {
        (self.current + n - 1) % n
    };
    self.set_highlight(true, ctx);
}

/// An arrow on the current target: window resize, or a ±1 divider nudge along
/// the divider's axis (cross-axis arrows are ignored for dividers).
fn arrow(&mut self, key: Key, ctx: &mut Context) {
    match self.targets.get(self.current) {
        Some(ResizeTarget::Window) | None => {
            let d = match key {
                Key::Left => Point::new(-1, 0),
                Key::Right => Point::new(1, 0),
                Key::Up => Point::new(0, -1),
                Key::Down => Point::new(0, 1),
                _ => return,
            };
            self.apply_delta(d, ctx);
        }
        Some(ResizeTarget::Divider { splitter, index, orientation }) => {
            let delta = match (orientation, key) {
                (Orientation::Cols, Key::Left) => -1,
                (Orientation::Cols, Key::Right) => 1,
                (Orientation::Rows, Key::Up) => -1,
                (Orientation::Rows, Key::Down) => 1,
                _ => 0,
            };
            if delta != 0 {
                ctx.splitter_divider(*splitter, DividerOp::Nudge { index: *index, delta });
            }
        }
    }
}

/// Enter (commit) / Esc (cancel): clear every highlight and end every session.
fn finish(&self, commit: bool, ctx: &mut Context) {
    let mut seen: Vec<ViewId> = Vec::new();
    let mut window_in_targets = false;
    for t in &self.targets {
        match t {
            ResizeTarget::Window => window_in_targets = true,
            ResizeTarget::Divider { splitter, .. } => {
                if !seen.contains(splitter) {
                    seen.push(*splitter);
                    ctx.splitter_divider(*splitter, DividerOp::EndSession { commit });
                }
            }
        }
    }
    if window_in_targets {
        if !commit {
            ctx.request_bounds(self.window_id, self.save_bounds);
        }
        ctx.request_set_state(self.window_id, StateFlag::Dragging, false);
    }
}
```

- [ ] **Step 5: Rewrite `CaptureHandler::handle`** (lines 688-816) to dispatch through the helpers. Keep Home/End/PageUp/PageDown working for the **window** target only:

```rust
fn handle(&mut self, ev: &mut Event, ctx: &mut Context) -> CaptureFlow {
    let Event::KeyDown(k) = ev else {
        return CaptureFlow::Consumed; // modal: swallow non-keys
    };
    match k.key {
        Key::Tab => {
            self.cycle(!k.modifiers.shift, ctx);
            CaptureFlow::Consumed
        }
        Key::Left | Key::Right | Key::Up | Key::Down => {
            // Ctrl = larger window step; dividers ignore Ctrl (always ±1).
            if k.modifiers.ctrl && self.current_is_window() {
                let d = match k.key {
                    Key::Left => Point::new(-8, 0),
                    Key::Right => Point::new(8, 0),
                    Key::Up => Point::new(0, -4),
                    Key::Down => Point::new(0, 4),
                    _ => Point::new(0, 0),
                };
                self.apply_delta(d, ctx);
            } else {
                self.arrow(k.key, ctx);
            }
            CaptureFlow::Consumed
        }
        Key::Home | Key::End | Key::PageUp | Key::PageDown if self.current_is_window() => {
            // (keep the existing edge-snap bodies for these four, unchanged)
            // ... existing Home/End/PageUp/PageDown logic ...
            CaptureFlow::Consumed
        }
        Key::Enter => {
            self.finish(true, ctx);
            CaptureFlow::ConsumedPop
        }
        Key::Esc => {
            self.finish(false, ctx);
            CaptureFlow::ConsumedPop
        }
        _ => CaptureFlow::Pass,
    }
}
```
Preserve the four edge-snap key bodies verbatim from the current code (lines 731-797) inside the guarded `Home|End|PageUp|PageDown` arm.

- [ ] **Step 6: Build the target list at RESIZE entry** (the `Command::RESIZE` handler, lines 959-990). Replace the body so it (a) begins the body splitter session + enumerates dividers, (b) builds `targets` with `Window` first iff move/grow, (c) bails if no targets, (d) sets the initial highlight, (e) pushes the extended capture.

First add a window helper (in `impl Window`):
```rust
/// Begin a resize session on the body splitter (if any) and return its divider
/// targets. The body is the first child that downcasts to `Splitter`.
fn begin_body_splitter_session(&mut self) -> Vec<ResizeTarget> {
    let ids = self.group.child_ids_in_order();
    for id in ids {
        if let Some(sp) = self
            .group
            .child_mut(id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<crate::widgets::Splitter>())
        {
            return sp
                .begin_resize_session()
                .into_iter()
                .map(|(splitter, index, orientation)| ResizeTarget::Divider {
                    splitter,
                    index,
                    orientation,
                })
                .collect();
        }
    }
    Vec::new()
}
```
Then the handler:
```rust
if let Event::Command(c) = *ev
    && c == Command::RESIZE
    && let Some(id) = self.group.state().id()
{
    let can_window = self.flags.r#move || self.flags.grow;
    let div_targets = self.begin_body_splitter_session();
    if !can_window && div_targets.is_empty() {
        // nothing to resize
    } else {
        let mut targets = Vec::new();
        if can_window {
            targets.push(ResizeTarget::Window);
        }
        targets.extend(div_targets);

        let owner_size = ctx.owner_size();
        let limits = Rect::new(0, 0, owner_size.x, owner_size.y);
        let (min, max) = View::size_limits(self, owner_size);
        let save_bounds = self.group.state().get_bounds();
        let origin = save_bounds.a;
        let size = save_bounds.b - save_bounds.a;
        let mut mode = self.group.state().drag_mode;
        mode.drag_move = self.flags.r#move;
        mode.drag_grow = self.flags.grow;

        // Initial highlight: window target glows via Dragging; a divider-first
        // (fixed window) start highlights divider 0.
        match targets.first() {
            Some(ResizeTarget::Window) => {
                View::set_state(self, StateFlag::Dragging, true, ctx);
            }
            Some(ResizeTarget::Divider { splitter, index, .. }) => {
                ctx.splitter_divider(*splitter, DividerOp::SetActive(Some(*index)));
            }
            None => {}
        }

        ctx.push_capture(Box::new(KeyboardResizeCapture {
            window_id: id,
            save_bounds,
            limits,
            min,
            max,
            mode,
            origin,
            size,
            targets,
            current: 0,
        }));
        ev.clear();
    }
}
```
Note the guard dropped the `(self.flags.r#move || self.flags.grow)` precondition (so a fixed window with a splitter can still resize dividers); the `can_window` flag now gates only the `Window` target.

- [ ] **Step 7: Extend RESIZE enablement** so a fixed window with movable dividers still receives the command (line 1085 in `set_state`). Add a body-scan helper and use it:
```rust
/// True if the body splitter (first Splitter child) has ≥1 movable divider.
fn body_has_movable_divider(&self) -> bool {
    self.group
        .child_ids_in_order()
        .iter()
        .filter_map(|id| self.group.find(*id)) // read-only accessor
        .any(|v| {
            v.as_any()
                .and_then(|a| a.downcast_ref::<crate::widgets::Splitter>())
                .map(|sp| sp.has_movable_divider())
                .unwrap_or(false)
        })
}
```
Change line 1085 to:
```rust
toggle(
    Command::RESIZE,
    self.flags.r#move || self.flags.grow || self.body_has_movable_divider(),
);
```
This needs a read-only `Group::find` + `View::as_any` (check they exist; `find_mut`/`as_any_mut` do — add `find`/`as_any` only if missing, or use a `&mut self`-free downcast already available). Add `Splitter::has_movable_divider(&self) -> bool` in Task 1's file if not already:
```rust
pub(crate) fn has_movable_divider(&self) -> bool {
    (0..self.slots.len().saturating_sub(1)).any(|i| self.style_of(i).movable_in_reconfig())
}
```
If a read-only `find`/`as_any` path does not exist and adding it is non-trivial, **skip this step** (leave enablement at `move || grow`) and note it: divider resize then requires a movable/growable window, which all current demos are. This is the spec's noted edge case, not core.

- [ ] **Step 8: Run the integration tests — expect PASS**

```bash
cargo test -p rstv --jobs 4 resize_mode_ 2>&1 | tail -30
```
Expected: PASS (Tab→divider→arrow moves it; Esc restores).

- [ ] **Step 9: Full suite + clippy + fmt**

```bash
cargo test  --workspace --jobs 4 2>&1 | tail -20
cargo clippy --workspace --all-targets --jobs 4 -- -D warnings 2>&1 | tail -20
cargo fmt --all --check
```
Expected: all pass. Per memory "trust cargo, not diagnostics" — read the real cargo output.

- [ ] **Step 10: Commit**

```bash
git add src/window/window.rs src/widgets/splitter/mod.rs
git commit -m "feat(window): unified keyboard resize — Tab cycles window<->dividers

Command::RESIZE now begins a session on the body splitter and pushes one
capture whose Tab/Shift+Tab cycle the resize target (window then each
divider); arrows move the active target, Enter commits, Esc cancels all.
Divider ops are brokered by id via DividerOp.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 5: Examples + docs cleanup

**Files:**
- Modify: `examples/splitter.rs` (doc comment lines 18-24; the status-line `~F6~ Resize panes` item at 237-238)
- Modify: `examples/gallery.rs` (line 454 doc comment)
- Modify: `docs/book/src/apps/windows.md` (any "F6 reconfig" splitter wording)

- [ ] **Step 1: Update `examples/splitter.rs`** — replace the F6 status-line item and the module doc. The status line should advertise the real path (Ctrl-F5 / the app's resize binding, then Tab to pick a divider). If the example has no resize key bound, bind `Command::RESIZE` to a key in its status line (mirror how tvdemo binds resize) so the example is runnable end-to-end. Replace the `~F6~ Resize panes` status item:

```rust
// before: ("~F6~ Resize panes", KeyEvent::from(Key::F(6)), <custom cmd>)
StatusDef::key("~Ctrl-F5~ Resize", KeyEvent::from(Key::ctrl(Key::F(5))), Command::RESIZE),
```
(Match the exact `StatusDef`/`KeyEvent` constructor the file already uses; the point is RESIZE, not F6.) Update the module doc block (18-24) to describe: "Resize: enter resize mode (Ctrl-F5), **Tab** cycles window↔dividers, arrows move, Enter/Esc."

- [ ] **Step 2: Update `examples/gallery.rs` line 454 doc** — change "`F6` enters keyboard reconfig" to "enter resize mode then **Tab** to a divider; arrows move it."

- [ ] **Step 3: Update `docs/book/src/apps/windows.md`** — find the splitter F6 wording and replace with the unified-mode description (Tab cycles window↔dividers). Do not edit generated `docs/book/book/*` HTML by hand.

- [ ] **Step 4: Build examples + docs gate**

```bash
cargo build --examples --jobs 4 2>&1 | tail -10
cargo xtask test 2>&1 | tail -20   # guide rust-block doctests (per memory: this gates)
```
Expected: examples compile; doc gate passes. If `cargo xtask docs` is needed to regenerate the book, run it.

- [ ] **Step 5: Commit**

```bash
git add examples/splitter.rs examples/gallery.rs docs/book/src/apps/windows.md
git commit -m "docs: splitter resize via Tab in window resize mode (drop F6 wording)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 6: Final verification + manual smoke + log

- [ ] **Step 1: Whole-tree diff review** (per memory "diff-full-tree-after-subagent")

```bash
git diff main...HEAD --stat
git diff main...HEAD
```
Confirm: no stray refactors, no `Weight::Double` left in splitter divider/crossing paths, no `enter_reconfig`/F6 remnants, `Splitter` still re-exported.

- [ ] **Step 2: Full gate on the integrated tree**

```bash
cargo test  --workspace --jobs 4 2>&1 | tail -20
cargo clippy --workspace --all-targets --jobs 4 -- -D warnings 2>&1 | tail -20
cargo fmt --all --check
```
Expected: all clean.

- [ ] **Step 3: Manual smoke in tvdemo** (per memory "tmux-sandbox-gotcha": launch+interact+capture in ONE Bash call). Verify: (a) F6 now cycles windows (cmNext) instead of entering splitter reconfig; (b) Ctrl-F5 on the splitter window enters resize, Tab moves highlight to a divider (it glows), arrows move it; (c) dragging a divider with the mouse glows it; (d) dividers at rest are the frame color. Capture a screenshot or two for the record.

- [ ] **Step 4: Append to `docs/IMPLEMENTATION-LOG.md`** (newest first) a short entry: what landed (unified splitter/window resize, divider color, mouse-any-movable, F6 dropped), the new `Deferred::SplitterDivider` seam, and the Cyan/Gray-palette + Ctrl+arrow-step follow-ups. Commit.

```bash
git add docs/IMPLEMENTATION-LOG.md
git commit -m "docs: log splitter resize unification

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

- [ ] **Step 5: Hand back** — report status and let the user decide on merge to main (per memory "rstv-commit-workflow", completed+reviewed work lands on main; this was built on a branch for safety). Use superpowers:finishing-a-development-branch.
