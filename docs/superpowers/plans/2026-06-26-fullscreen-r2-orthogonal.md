# Fullscreen Revision 2 — Orthogonal Primitives (rework plan)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Reworks the shipped `feat/fullscreen-window` branch. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Decouple the fullscreen feature into independent primitives so the bugs found in live testing (un-zoom → small-but-frameless; frameless content keeps a 1-cell margin) disappear by construction.

**Design source of truth:** `docs/design/fullscreen-window.md` → the **"Revision 2 — orthogonal primitives"** section. Read it first.

**Architecture:** Three independent primitives — `Window::set_bordered` (border, decoupled from fullscreen), one unified `maximize`/`restore` with a single `restore_rect` (shared by zoom + fullscreen-Desktop), and `MenuBar::set_collapsed`. `Window::set_fullscreen`/`Command::FULLSCREEN` *compose* them. Border toggle reflows content via the **existing** `grow_mode` resize plus a (∓1,∓1) origin shift; scrollbars (owned by the window) are re-derived by the `client_rect` formula.

## Global Constraints

- `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target`; `CARGO_BUILD_JOBS=4`; tests `-- --test-threads=4`; **≤4 cores**.
- Gate each task: `cargo test --workspace -j4 -- --test-threads=4`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, and `cargo xtask docs` (no NEW broken intra-doc links).
- English everywhere. Commit trailer: `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.
- This rework **changes** existing R1 tests (e.g. the collapsed-menu-bar `⋮`-cell bounds become a 3-cell `[⋮]`; the un-zoom behavior). Updating those tests to the R2 behavior is in scope — but each change must be a deliberate behavior update, not a deletion to make a test pass. Keep the four loop-owned-state tests (round-trip, removal-restore, resize re-fit, popup activation) working against the new behavior.
- Do not regress non-fullscreen windows: a bordered window at rest must render byte-for-byte as before (existing frame/window/scroller snapshots unchanged).

---

### Task A: Orthogonal Window + pump rework (border, content reflow, unified maximize, recomposed fullscreen)

**Model:** strongest available (FOUNDATION — subtle, multi-method, interdependent).

**Files:**
- Modify: `src/window/window.rs` (struct + `new` + `set_fullscreen` + `zoom`/maximize + `client_rect` + drag guard + `standard_scroll_bar` + accessors/tests)
- Modify: `src/app/program.rs` (`FullscreenSlot`, `apply_fullscreen`, the drain arm, the resize/vanish block, tests)
- Modify: `src/frame.rs` only if a doc/comment references the old coupling (no behavior change there — `set_border_visible` stays)

**Interfaces produced (consumed by Task B + demos):**
- `pub fn Window::set_bordered(&mut self, bordered: bool, ctx: &mut Context)` — toggles the frame border AND reflows content; idempotent if unchanged.
- `pub fn Window::bordered(&self) -> bool`.
- `pub fn Window::maximize(&mut self, ctx: &mut Context)` / `pub fn Window::restore(&mut self, ctx: &mut Context)` / `pub fn Window::is_maximized(&self) -> bool` — one restore slot (`restore_rect: Option<Rect>`).
- `Window::set_fullscreen(&mut self, mode: Fullscreen, ctx)` — recomposed (below); `Window::fullscreen()` unchanged.
- `Deferred::SetFullscreen { window, mode }` unchanged in shape; `FullscreenSlot` trimmed to `{ window: ViewId, mode: Fullscreen }` (no `restore`/`shadow` — the window owns the restore now).

**Approach (do these as cohesive TDD slices; the pieces interlock, so they land in one task):**

1. **`Window` struct:** replace `zoom_rect: Rect` with `restore_rect: Option<Rect>` (None = not maximized); add `bordered: bool` (default `true`); add `scrollbar_ids: Vec<ViewId>` (the window's own scroll bars). Update `Window::new` initializers. Update the existing `zoom_rect()` accessor + any tests: expose `restore_rect()`/`is_maximized()` instead and migrate references.

2. **`set_bordered`:** 
```rust
pub fn set_bordered(&mut self, bordered: bool, ctx: &mut Context) {
    if self.bordered == bordered { return; }
    let old_client = self.client_rect();
    self.bordered = bordered;
    if let Some(frame) = self.frame_mut() { frame.set_border_visible(bordered); }
    let new_client = self.client_rect();
    self.reflow_client(old_client, new_client, ctx);
}
```
Repoint `client_rect()` to test `self.bordered` (NOT `self.fullscreen`). Repoint the `handle_event` drag-start guard from `self.fullscreen == Off` to `self.bordered` (a borderless window starts no title/edge drag).

3. **`reflow_client(old, new, ctx)`** — the key new mechanism. The window size is unchanged; only the client inset changed. For each child:
   - **frame** (`frame_id`): skip (it always fills the extent; `change_bounds` of the window already keeps it full).
   - **a scroll bar** (`scrollbar_ids`): re-derive its rect from the `standard_scroll_bar` formula for the NEW `bordered` state and `change_bounds` it (it anchors to an edge, not the client origin).
   - **content** (everything else): apply the existing grow-mode resize for the client **size delta** then translate by the client **origin delta**:
     ```
     size_delta   = (new.b - new.a) - (old.b - old.a)
     origin_delta = new.a - old.a
     let resized = child.calc_bounds(self.group.state().size, size_delta /*, min,max */); // grow_mode reaction
     child.change_bounds(resized.offset(origin_delta));
     child.on_bounds_changed(ctx);
     ```
   (Verify `calc_bounds`'s exact signature/visibility — it's the same helper the resize cascade uses, `view.rs` ~697. If a content child has no edge-tracking `grow_mode`, `calc_bounds` returns it unresized and it simply shifts by `origin_delta` — correct: "reacts as on a resize.")
   Refactor `standard_scroll_bar`'s rect computation into a private helper `scroll_bar_rect(vertical) -> Rect` so both `standard_scroll_bar` (insert time) and `reflow_client` (border toggle) use one formula; have `standard_scroll_bar` push the returned id into `self.scrollbar_ids`.

4. **Unified maximize:**
```rust
pub fn maximize(&mut self, ctx: &mut Context) {
    if self.restore_rect.is_none() {
        self.restore_rect = Some(self.group.state().get_bounds());
    }
    let owner = ctx.owner_size();
    let (_min, max) = View::size_limits(self, owner);
    self.locate(Rect::new(0, 0, max.x, max.y), owner);
    self.push_zoomed(true); // existing frame set_zoomed seam
}
pub fn restore(&mut self, ctx: &mut Context) {
    if let Some(r) = self.restore_rect.take() {
        let owner = ctx.owner_size();
        self.locate(r, owner);
    }
    self.push_zoomed(false);
}
```
(Extract the existing `zoom`'s frame `set_zoomed` downcast into a `push_zoomed(bool)` helper.) The **`ZOOM` command** handler becomes: `if self.is_maximized() { self.restore(ctx) } else { self.maximize(ctx) }`. Remove the old `zoom()`/`zoom_rect` logic.

5. **Recompose `set_fullscreen`:**
```rust
pub fn set_fullscreen(&mut self, mode: Fullscreen, ctx: &mut Context) {
    self.fullscreen = mode;
    match mode {
        Fullscreen::Off => { self.restore(ctx); self.set_bordered(true, ctx); }
        _ /* Desktop | Screen */ => { self.maximize(ctx); self.set_bordered(false, ctx); }
    }
    if let Some(id) = self.group.state().id() { ctx.set_fullscreen(id, mode); }
}
```
The window-local work (maximize/restore + border + content reflow) is INLINE; the deferred op tells the pump to do the cross-tree work (Screen only) and to track the window for resize/removal. (For Screen, the inline `maximize` fills the *current* desktop; the pump re-fills into the expanded desktop — see below. That double-set is harmless.)

6. **Slim the pump (`apply_fullscreen` + `FullscreenSlot`):** `FullscreenSlot = { window, mode }`. `apply_fullscreen(group, desktop, menu_bar, status_line, slot, window, mode, ctx)` now ONLY does cross-tree + tracking (NO border, NO restore/shadow — the window did those inline):
   - menu bar: `set_collapsed(mode == Screen)` + bounds (full row vs the collapsed cell — Task B changes the cell to `[⋮]`; for Task A keep the existing single-cell logic, Task B widens it).
   - desktop: top row 0 when Screen else 1 (`change_bounds`).
   - window: for `mode != Off`, re-fill to the (now-sized) desktop via `change_bounds(Rect::new(0,0,w,dh))` (Screen needs this post-expansion; Desktop is already filled inline but re-filling is harmless and keeps resize uniform). For `Off`, do NOT touch the window bounds (the window restored itself inline).
   - slot: `(mode != Off).then(|| FullscreenSlot { window, mode })`.
   - The drain arm and the resize/vanish block stay, minus the removed `restore`/`shadow` capture. Vanish path: window gone → uncollapse menu + desktop row 1 + clear slot (no window bounds to restore).

7. **Tests (update R1 + add R2):**
   - **Update** the existing `set_fullscreen_screen_collapses_menu_and_covers_top`, `fullscreen_command_cycles_to_screen`, and the four loop-owned-state tests to the recomposed behavior (they should still pass; the collapsed-bounds assertions stay single-cell until Task B).
   - **Add — un-zoom coherence (the reported bug):** drive a window to `Desktop` fullscreen, then send `Command::ZOOM`; assert the result is a COHERENT state — the window is bordered iff `bordered()` says so and zoom/fullscreen don't leave a small-frameless-with-stale-slot. Concretely: after `set_fullscreen(Desktop)` then a `ZOOM` (restore), the window is back at `restore_rect` AND still `!bordered()` (border is independent — zoom doesn't re-border), i.e. an intentional small borderless window, not the old accidental one; `program.fullscreen` reflects the tracked state. (Pin the exact post-conditions to the implemented semantics and assert them.)
   - **Add — content reflow:** a window with one content child filling the interior `(1,1,w-1,h-1)`; call `set_bordered(false, ctx)` (through a pump or `with_ctx`); assert the content child is now `(0,0,w,h)`. Then `set_bordered(true)`; assert it returns to `(1,1,w-1,h-1)`. This is the margin-bug regression guard — it would fail if the reflow or origin shift were dropped.

- [ ] **Step 1:** Write the failing content-reflow test + the un-zoom-coherence test first (TDD RED); run, confirm they fail to compile / fail.
- [ ] **Step 2:** Implement struct + `set_bordered`/`reflow_client` + `scroll_bar_rect`/`scrollbar_ids` + `client_rect`/drag repoint.
- [ ] **Step 3:** Implement `maximize`/`restore`/`push_zoomed` + reroute `ZOOM`; remove `zoom_rect`/`zoom()`.
- [ ] **Step 4:** Recompose `set_fullscreen`; slim `FullscreenSlot` + `apply_fullscreen` + drain + resize/vanish.
- [ ] **Step 5:** Update the R1 tests to the new behavior; run the FULL gate (test + clippy + fmt + docs). Iterate to green.
- [ ] **Step 6:** Commit.

```bash
git add src/window/window.rs src/app/program.rs src/frame.rs
git commit -m "refactor(fullscreen): orthogonal primitives — bordered, unified maximize, content reflow

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task B: `[⋮]` bracketed kebab

**Model:** standard.

**Files:** `src/menu/menu_bar.rs` (draw + collapsed bounds expectation), `src/app/program.rs` (collapsed menu-bar bounds in `apply_fullscreen` + the bounds assertions in the fullscreen tests), and any test asserting the `⋮`-cell bounds.

**Approach:**
1. Collapsed `MenuBar::draw` paints `[⋮]` (3 cells) at the top-right: `put_str(size.x - 3, 0, "[⋮]", ...)`. (Keep the transparent-elsewhere behavior.)
2. `apply_fullscreen` sets the collapsed menu-bar bounds to the 3-cell rect `Rect::new(w - 3, 0, w, 1)` (was `w-1`). Hit-routing still works because it's a real bounds change; the popup anchor stays at the top-right (`owner.x - 1, 0` is fine, or `owner.x - 2`).
3. Update the affected tests: the collapsed-bounds assertions become `(w-3, w, 0, 1)` (e.g. `(37, 40, 0, 1)` on a 40-wide screen), and the `collapsed_bar_draws_only_kebab` assertion checks for `[⋮]`/`⋮` present and items absent. The popup-activation test still asserts a popup opens on a click in the `[⋮]` cells.

- [ ] **Step 1:** Update the collapsed-draw test to expect `[⋮]` (RED).
- [ ] **Step 2:** Implement the 3-cell draw + the `apply_fullscreen` bounds + update assertions.
- [ ] **Step 3:** Full gate; commit.

```bash
git add src/menu/menu_bar.rs src/app/program.rs
git commit -m "feat(menu): render the collapsed menu bar as a bracketed [⋮] kebab

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task C: tvdemo content fills frameless (verification)

**Model:** cheap. After A+B, the demo windows should fill correctly with NO `insert_client` (content reflows automatically). Verify by reading: the tvdemo windows (Puzzle/Calendar/ASCII/FileWindow/splitter) insert content as ordinary children with edge-tracking grow modes, so `reflow_client` handles them. If any demo content has an empty/zero grow mode that prevents fill, note it — but do NOT add per-window hooks; the framework reflow is the mechanism. (Likely no code change; this is a confirmation task. If a demo content child has `grow_mode == default` (no tracking) and thus doesn't grow on reflow, give it `hi_x|hi_y` so it fills — that's the faithful "reacts to resize" fix.)

- [ ] Confirm via reading + a build; adjust demo content `grow_mode` only if needed; commit only if changed.

## Self-review (coverage)
- Un-zoom bug → Task A unified maximize + independent border + the coherence test. ✓
- Content margin bug → Task A `reflow_client` + the reflow test + Task C demo confirm. ✓
- `[⋮]` → Task B. ✓
- Independent API (`set_bordered`/`maximize`/`set_collapsed`) → Task A (public) + B. ✓
- Fullscreen still works (cycle + Screen cross-tree + resize/removal) → Task A recompose + updated tests. ✓
