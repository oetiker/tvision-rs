# Data-Movement Phase 3 — sync signals → trait methods Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Retire every cluster-B *sync-signal* downcast in the pump's deferred-drain by delivering each sync through a defaulted `View` trait method (virtual dispatch), and collapse the four sibling-scrollbar read-sync brokers into ONE shared `apply_scroll_sync` hook + ONE `Deferred::ScrollSync` variant.

**Architecture:** Cluster B (sync signals — "this sibling changed; recompute") stays a *separate mechanism* from cluster-A field data (it keeps its `Deferred` variants), but each variant's pump arm calls a defaulted `View` method on the resolved target instead of `as_any_mut().downcast_mut::<Concrete>()`. The three sibling-scrollbar *read* syncs (Scroller, Outline, Editor) plus the existing list-viewer read-sync share ONE hook `apply_scroll_sync(h, v, ctx)` (a generalization+rename of today's `apply_list_scroll`) and ONE variant `Deferred::ScrollSync { target, h, v }`. The two non-scroll syncs (Indicator `set_value`, PageStack page-switch) resist the shared `(h, v)` shape under the §2.1 test, so each keeps its own variant and gets its own defaulted hook — de-downcast in place, reason recorded.

**Tech Stack:** Rust (workspace `tvision-rs` + `tvision-rs-macros`); the `#[delegate(to = field)]` proc-macro; `insta` snapshot tests; the single-event-loop pump in `src/app/program.rs`.

## Global Constraints

- **Spec authority:** `docs/superpowers/specs/2026-06-18-unified-data-movement-design.md` — read §3.2 (sync → defaulted per-capability `View` methods; the five-into-one collapse; the §2.1 "stays separate only if folding it in makes the hook murkier" test, *reason recorded*) and §5 Phase 3.
- **Behavior-preserving.** Every task in this plan is a refactor: no user-visible behavior changes. The safety net is the **existing test suite staying green** plus the per-task grep-proof that the downcast/variant is gone. Snapshots must be **byte-identical** (do NOT accept `insta` changes — a changed snapshot means a real regression).
- **No *framework-internal* `dyn Any`.** The pump must never `downcast_mut::<ConcreteWidget>()` to deliver a sync. (The deliberate typed-at-the-edges `FieldValue::Custom` escape is unrelated and out of scope.)
- **Each new `View` trait method needs BOTH:** (1) a forwarder entry in `tvision-rs-macros/src/specs.rs`, and (2) an entry in the `tests/delegate_view.rs` spy test (impl that `mark`s + a call + the name in the asserted method-name list). A renamed method renames its existing forwarder + spy entry; a brand-new method adds one. The spy test does NOT auto-catch a forgotten forwarder for a *brand-new* defaulted method, so adding it is a required step, not optional.
- **Out of scope (deliberately NOT migrated this phase; record as such):** `Deferred::ScrollBarSetParams` (the *write* direction scroller→scrollbar) and `Deferred::SplitterDivider` downcast to `ScrollBar`/`Splitter` — they are not in the spec's "five scroll-family brokers" list. Non-sync structural downcasts (FileDialog readback, modal-completion routing, menu, button-press) are a different category (§6) and stay.
- **Coordinates are `i32`.** `Point` is `crate::geometry::Point` (already imported in the pump as `Point`).
- **Commands:** workspace build. `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target`. Use `cargo test --workspace -j2 -- --test-threads=2`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`. Commit messages end with the project Co-Authored-By trailer.

---

## File Structure

The work centers on three files plus per-widget overrides:

- `src/view/view.rs` — the `View` trait: rename `apply_list_scroll` → `apply_scroll_sync` (generalized doc); add `set_indicator_value` + `apply_page_sync` defaulted methods.
- `src/view/context.rs` — the `Deferred` enum + the `request_*` push methods: add `Deferred::ScrollSync`; remove `SyncScrollerDelta`/`SyncListViewer`/`SyncOutlineViewerDelta`/`SyncEditorDelta`; replace the four named `request_sync_*` methods with one `request_scroll_sync`; keep `IndicatorSetValue` + `PageStackSync` variants (their request methods unchanged).
- `src/app/program.rs` — the pump deferred-drain: one unified `ScrollSync` arm (replaces four arms, no downcast); de-downcast the `IndicatorSetValue` + `PageStackSync` arms to trait calls.
- Per-widget overrides: `src/widgets/scroller.rs`, `src/widgets/outline.rs`, `src/widgets/editor.rs`, `src/widgets/indicator.rs`, `src/widgets/page_stack.rs`, and the rename ripple through `src/widgets/list_box.rs`, `src/widgets/list_viewer.rs`, `src/widgets/history.rs`, `src/dialog/filedlg.rs`.
- Macro + spy: `tvision-rs-macros/src/specs.rs`, `tests/delegate_view.rs`.
- Docs: `docs/book/src/internals/brokering.md`, `docs/IMPLEMENTATION-LOG.md`. (Everything under `docs/book/book/` is **generated** — do not hand-edit.)

---

## Task 1: Rename `apply_list_scroll` → `apply_scroll_sync` (pure rename, generalized doc)

**Files:**
- Modify: `src/view/view.rs:924` (trait default + doc)
- Modify: `tvision-rs-macros/src/specs.rs:80-81` (forwarder)
- Modify: `tests/delegate_view.rs:125-126, ~279-285, ~345` (spy impl, call, name list)
- Modify override sites: `src/widgets/list_box.rs:143, 344`, `src/widgets/list_viewer.rs:1086`, `src/widgets/history.rs:343`, `src/dialog/filedlg.rs:555, 990`, `src/app/program.rs:7033` (test impl)
- Modify call site: `src/app/program.rs:2210` (pump arm `view.apply_list_scroll(...)`)

**Interfaces:**
- Produces: `View::apply_scroll_sync(&mut self, h: Option<i32>, v: Option<i32>, ctx: &mut Context)` (defaulted no-op) — the shared scroll-sync hook later overridden by Scroller/Outline/Editor (Tasks 2–4). Same signature as the old `apply_list_scroll`.

- [ ] **Step 1: Find every reference**

Run:
```bash
cd /scratch/oetiker/claude-worktrees/tvision-rs-consumer-api-coverage
grep -rn "apply_list_scroll" src/ tests/ tvision-rs-macros/
```
Expected: the override sites, the trait default, the macro forwarder, the spy test (impl + call + name string), and the pump call site listed above. (Generated `docs/book/book/**` and historical `docs/briefs/**` are NOT edited.)

- [ ] **Step 2: Rename the trait default + generalize its doc**

In `src/view/view.rs`, replace the `apply_list_scroll` method (around line 912–924) with:
```rust
    /// The shared scrollbar read-sync broker hook. Defaulted no-op; scroll-aware
    /// widgets override it to apply a freshly-read scrollbar delta to themselves.
    /// The pump passes the horizontal/vertical scrollbar values (`None` if that bar
    /// is absent or unresolved), each read via [`View::value`]. Overridden by the
    /// list viewers (delegate to
    /// [`list_viewer::apply_scroll`](crate::widgets::list_viewer::apply_scroll)),
    /// [`Scroller`](crate::widgets::Scroller), the outline viewer, and the editor —
    /// every sibling-scrollbar *read* sync routes through this one method instead of
    /// a pump downcast.
    ///
    /// Each widget interprets `None` per its own semantics (a read-only scroller
    /// treats a missing bar as delta `0`; the editor preserves `None` to skip that
    /// axis). Driven by [`Deferred::ScrollSync`](crate::view::Deferred::ScrollSync).
    fn apply_scroll_sync(&mut self, _h: Option<i32>, _v: Option<i32>, _ctx: &mut Context) {}
```

- [ ] **Step 3: Rename the macro forwarder**

In `tvision-rs-macros/src/specs.rs` replace the `apply_list_scroll` entry (lines 80–81) with:
```rust
        ("apply_scroll_sync",
         quote! { fn apply_scroll_sync(&mut self, h: ::core::option::Option<i32>, v: ::core::option::Option<i32>, ctx: &mut #k::Context) { self.#f.apply_scroll_sync(h, v, ctx) } }),
```

- [ ] **Step 4: Rename the spy-test impl, call, and name-list entry**

In `tests/delegate_view.rs`: rename the impl (`fn apply_list_scroll` → `fn apply_scroll_sync`, `self.mark("apply_list_scroll")` → `self.mark("apply_scroll_sync")`), the call `d.apply_list_scroll(Some(0), Some(0), &mut ctx)` → `d.apply_scroll_sync(...)`, and the `"apply_list_scroll"` string in the asserted name list → `"apply_scroll_sync"`.

- [ ] **Step 5: Rename every override site + the pump call site**

Mechanically rename `fn apply_list_scroll` → `fn apply_scroll_sync` in `src/widgets/list_box.rs` (both), `src/widgets/list_viewer.rs`, `src/widgets/history.rs`, `src/dialog/filedlg.rs` (both), `src/app/program.rs:7033` (test impl); and the call `view.apply_list_scroll(hv, vv, &mut ctx)` → `view.apply_scroll_sync(hv, vv, &mut ctx)` at `src/app/program.rs:2210`. The `SyncListViewer` variant doc that names `apply_list_scroll` (in `src/view/context.rs`, ~line 170) is rewritten in Task 2 — leave it for now or fix the name in passing.

- [ ] **Step 6: Verify nothing named `apply_list_scroll` remains in code**

Run:
```bash
grep -rn "apply_list_scroll" src/ tests/ tvision-rs-macros/
```
Expected: no matches (generated HTML under `docs/book/book/` is regenerated separately and is not searched here).

- [ ] **Step 7: Build, test, lint**

Run:
```bash
cargo test --workspace -j2 -- --test-threads=2
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```
Expected: all green; the `delegate_view` spy test passes with the renamed method; no snapshot changes.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit
# message: "refactor(view): rename apply_list_scroll -> apply_scroll_sync (the shared scroll-sync hook)"
```

---

## Task 2: `Deferred::ScrollSync` + unified pump arm; migrate Scroller; collapse SyncListViewer

**Files:**
- Modify: `src/view/context.rs` — add `Deferred::ScrollSync` (near the old scroller variant, ~line 117); remove `SyncScrollerDelta` (117–124) and `SyncListViewer` (186–193); replace `request_sync_scroller_delta` (1191) and `request_sync_list_viewer` (1246) with one `request_scroll_sync`.
- Modify: `src/app/program.rs` — replace the `SyncScrollerDelta` arm (2070–2089) and the `SyncListViewer` arm (2200–2212) with ONE `ScrollSync` arm.
- Modify: `src/widgets/scroller.rs:219` (`ctx.request_sync_scroller_delta(...)` → `ctx.request_scroll_sync(...)`), `:563` (inline `Deferred::SyncScrollerDelta {..}` → `Deferred::ScrollSync {..}` + its `matches!` dedup guard, ~556), and add the `apply_scroll_sync` override.
- Modify: `src/widgets/list_box.rs:646` (inline `Deferred::SyncListViewer {..}` + its `matches!` guard) and `src/widgets/list_viewer.rs:724` (`ctx.request_sync_list_viewer(...)`) and `:1606` (inline construction + guard).

**Interfaces:**
- Consumes: `View::apply_scroll_sync` (Task 1).
- Produces:
  - `Deferred::ScrollSync { target: ViewId, h: Option<ViewId>, v: Option<ViewId> }` — the unified sibling-scrollbar read-sync.
  - `Context::request_scroll_sync(&mut self, target: ViewId, h: Option<ViewId>, v: Option<ViewId>)` — pushes `Deferred::ScrollSync`. Replaces `request_sync_scroller_delta` and `request_sync_list_viewer`.
  - `Scroller::apply_scroll_sync` override.

- [ ] **Step 1: Add the unified `Deferred::ScrollSync` variant**

In `src/view/context.rs`, add (replacing the `SyncScrollerDelta` block at 117–124; keep its rich doc, generalized):
```rust
    /// **Unified sibling-scrollbar read-sync.** On a scrollbar-changed broadcast a
    /// scroll-aware view (a leaf that can neither read nor mutate its window-frame
    /// scrollbar siblings) requests this. The pump resolves the `h`/`v` scrollbars,
    /// reads each `value` (via [`View::value`] → [`FieldValue::Int`]) into an
    /// `Option<i32>` (`None` = bar absent/unresolved), then calls
    /// [`View::apply_scroll_sync`] on `target` — **virtual dispatch to the concrete
    /// widget, never a downcast**. Serves the scroller, the list viewers, the
    /// outline viewer, and the editor; each interprets `None` per its own semantics.
    ///
    /// **Termination:** read-only consumers (scroller/outline/editor) write nothing
    /// back. The list viewers write back (item-focus → v-bar value), which
    /// terminates because [`ScrollBar::set_params`] is change-guarded (re-broadcasts
    /// only on an actual change, so writing the already-current value is a silent
    /// no-op). Touches the view-tree deferred family, so the insertion-order drain
    /// stays order-equivalent.
    ScrollSync {
        /// The scroll-aware view to apply the delta to (scroller / list / outline / editor).
        target: ViewId,
        /// The horizontal scrollbar to read `value` from (`None` = no h bar).
        h: Option<ViewId>,
        /// The vertical scrollbar to read `value` from (`None` = no v bar).
        v: Option<ViewId>,
    },
```
Delete the old `SyncListViewer` block (186–193). Fix any now-broken intra-doc links (`[`FieldValue::Int`]` etc. — copy the fully-qualified forms from the old docs).

- [ ] **Step 2: Replace the two `request_*` methods with one**

In `src/view/context.rs`, remove `request_sync_scroller_delta` (1191–1199) and `request_sync_list_viewer` (1246–1248), and add:
```rust
    /// Request the unified sibling-scrollbar read-sync ([`Deferred::ScrollSync`]):
    /// the pump reads `h`/`v` bar values and calls [`View::apply_scroll_sync`] on
    /// `target`. Used by the scroller, list viewers, outline viewer, and editor.
    pub fn request_scroll_sync(&mut self, target: ViewId, h: Option<ViewId>, v: Option<ViewId>) {
        self.deferred.push(Deferred::ScrollSync { target, h, v });
    }
```

- [ ] **Step 3: Replace the two pump arms with one unified arm**

In `src/app/program.rs`, delete the `SyncScrollerDelta` arm (2070–2089) and the `SyncListViewer` arm (2200–2212), and add (placed where `SyncScrollerDelta` was):
```rust
                            // Unified sibling-scrollbar read-sync (replaces the
                            // four SyncScrollerDelta/SyncListViewer/SyncOutline/
                            // SyncEditor downcasts): read each bar's `value` (each
                            // in its own find_mut so only one &mut is live) and call
                            // back through the defaulted View::apply_scroll_sync —
                            // virtual dispatch, never a downcast. The list-viewer
                            // override writes back (v-bar setValue); it terminates
                            // because ScrollBar::set_params is change-guarded.
                            Deferred::ScrollSync { target, h, v } => {
                                let hv = h
                                    .and_then(|id| group.find_mut(id))
                                    .and_then(|view| view.value())
                                    .and_then(field_int);
                                let vv = v
                                    .and_then(|id| group.find_mut(id))
                                    .and_then(|view| view.value())
                                    .and_then(field_int);
                                if let Some(view) = group.find_mut(target) {
                                    view.apply_scroll_sync(hv, vv, &mut ctx);
                                }
                            }
```

- [ ] **Step 4: Add the `Scroller::apply_scroll_sync` override**

In `src/widgets/scroller.rs`, in the `impl View for Scroller` block, add:
```rust
    fn apply_scroll_sync(&mut self, h: Option<i32>, v: Option<i32>, _ctx: &mut Context) {
        // The read-only scroller treats a missing bar as delta 0 (faithful to the
        // old SyncScrollerDelta `.unwrap_or(0)` read).
        self.apply_delta(Point::new(h.unwrap_or(0), v.unwrap_or(0)));
    }
```
Ensure `Point` is imported in `scroller.rs` (it is used by `apply_delta`).

- [ ] **Step 5: Repoint the Scroller + list-viewer push sites**

- `src/widgets/scroller.rs:219`: `ctx.request_sync_scroller_delta(scroller, self.h_scroll_bar, self.v_scroll_bar)` → `ctx.request_scroll_sync(scroller, self.h_scroll_bar, self.v_scroll_bar)`.
- `src/widgets/scroller.rs:~556-563`: the dedup guard `matches!(d, Deferred::SyncScrollerDelta { .. })` → `matches!(d, Deferred::ScrollSync { .. })` and the constructed `Deferred::SyncScrollerDelta { scroller, h: rh, v: rv }` → `Deferred::ScrollSync { target: scroller, h: rh, v: rv }`. (Read the surrounding 10 lines to keep the guard's `if`-condition, if any, intact.)
- `src/widgets/list_viewer.rs:724`: `ctx.request_sync_list_viewer(id, h, v)` → `ctx.request_scroll_sync(id, h, v)`.
- `src/widgets/list_viewer.rs:~1600-1606`: dedup guard + `Deferred::SyncListViewer { list, h: rh, v: rv }` → `Deferred::ScrollSync { target: list, h: rh, v: rv }`.
- `src/widgets/list_box.rs:~640-646`: dedup guard + `Deferred::SyncListViewer { .. }` → `Deferred::ScrollSync { target: ..., h, v }`.

- [ ] **Step 6: Verify the old variants/methods are gone**

Run:
```bash
grep -rn "SyncScrollerDelta\|SyncListViewer\|request_sync_scroller_delta\|request_sync_list_viewer" src/
```
Expected: no matches.

- [ ] **Step 7: Build, test, lint**

Run:
```bash
cargo test --workspace -j2 -- --test-threads=2
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```
Expected: all green; scroller + list-box + list-viewer snapshot tests **byte-identical** (behavior preserved). If any snapshot changed, STOP — it is a real regression.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit
# message: "refactor(pump): collapse scroller + list read-sync into Deferred::ScrollSync (no downcast)"
```

---

## Task 3: Migrate the outline viewer into `ScrollSync`

**Files:**
- Modify: `src/view/context.rs` — remove `SyncOutlineViewerDelta` variant (207–214) + `request_sync_outline_viewer_delta` (1259–1267).
- Modify: `src/app/program.rs` — remove the `SyncOutlineViewerDelta` arm (2120–2139).
- Modify: `src/widgets/outline.rs:845` (`ctx.request_sync_outline_viewer_delta(...)` → `ctx.request_scroll_sync(...)`), `:~1556-1562` (inline construction + dedup guard), add `apply_scroll_sync` override.

**Interfaces:**
- Consumes: `Deferred::ScrollSync`, `Context::request_scroll_sync`, `View::apply_scroll_sync` (Task 2).
- Produces: `Outline::apply_scroll_sync` override (read-only, `None` → 0).

- [ ] **Step 1: Add the `Outline::apply_scroll_sync` override**

In `src/widgets/outline.rs`, in the `impl View for Outline` block, add:
```rust
    fn apply_scroll_sync(&mut self, h: Option<i32>, v: Option<i32>, _ctx: &mut Context) {
        // Read-only, like the scroller: a missing bar is delta 0 (faithful to the
        // old SyncOutlineViewerDelta `.unwrap_or(0)` read).
        self.ov_mut().apply_delta(Point::new(h.unwrap_or(0), v.unwrap_or(0)));
    }
```
Ensure `Point` is in scope in `outline.rs`.

- [ ] **Step 2: Repoint the outline push sites**

- `src/widgets/outline.rs:845`: `ctx.request_sync_outline_viewer_delta(id, this.ov().h_scroll_bar, this.ov().v_scroll_bar)` → `ctx.request_scroll_sync(id, this.ov().h_scroll_bar, this.ov().v_scroll_bar)`.
- `src/widgets/outline.rs:~1556-1562`: dedup guard `matches!(d, Deferred::SyncOutlineViewerDelta { .. })` → `matches!(d, Deferred::ScrollSync { .. })` and `Deferred::SyncOutlineViewerDelta { viewer, h: rh, v: rv }` → `Deferred::ScrollSync { target: viewer, h: rh, v: rv }`.

- [ ] **Step 3: Remove the variant, request method, and pump arm**

Delete `Deferred::SyncOutlineViewerDelta` (context.rs 207–214), `request_sync_outline_viewer_delta` (context.rs 1259–1267), and the pump arm (program.rs 2120–2139).

- [ ] **Step 4: Verify removed**

Run:
```bash
grep -rn "SyncOutlineViewerDelta\|request_sync_outline_viewer_delta" src/
```
Expected: no matches.

- [ ] **Step 5: Build, test, lint**

Run:
```bash
cargo test --workspace -j2 -- --test-threads=2
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```
Expected: all green; outline snapshots byte-identical.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit
# message: "refactor(outline): route scrollbar read-sync through apply_scroll_sync (no downcast)"
```

---

## Task 4: Migrate the editor into `ScrollSync` (delegation reaches the inner Editor)

**Files:**
- Modify: `src/view/context.rs` — remove `SyncEditorDelta` variant (354–361) + `request_sync_editor_delta` (1466–1474).
- Modify: `src/app/program.rs` — remove the `SyncEditorDelta` arm (2314–2331).
- Modify: `src/widgets/editor.rs:1838, 2128` (`ctx.request_sync_editor_delta(...)` → `ctx.request_scroll_sync(...)`), `:~4550-4556` (inline construction + dedup guard), add `apply_scroll_sync` override on `Editor`.

**Interfaces:**
- Consumes: `Deferred::ScrollSync`, `Context::request_scroll_sync`, `View::apply_scroll_sync` (Task 2).
- Produces: `Editor::apply_scroll_sync` override that **preserves `None`** (`self.apply_scroll_delta(h, v, ctx)`). Reaches a `FileEditor`/`Memo` target via the `#[delegate(to = editor)]` forwarder for `apply_scroll_sync` (added in Task 1) — no `editor_mut` needed in this path.

**Key correctness note:** the editor target id, in an `EditWindow`, resolves to a `FileEditor` (not a plain `Editor`). The old `SyncEditorDelta` arm peeled it with `editor_mut`. Now `group.find_mut(target)` returns the `FileEditor` as `&mut dyn View`, and `.apply_scroll_sync(...)` is **forwarded by the `#[delegate(to = editor)]` macro** to the inner `Editor` (FileEditor and Memo both delegate to their `editor` field, and neither `skip`s this method — verify in Step 3). The `editor_mut` helper stays (still used by `EditorPaste`).

- [ ] **Step 1: Add the `Editor::apply_scroll_sync` override**

In `src/widgets/editor.rs`, in the `impl View for Editor` block, add:
```rust
    fn apply_scroll_sync(&mut self, h: Option<i32>, v: Option<i32>, ctx: &mut Context) {
        // Unlike the scroller/outline, the editor PRESERVES `None` (a missing bar
        // skips that axis rather than scrolling to 0) — apply_scroll_delta is its
        // TEditor::checkScrollBar body. ctx is live (the editor may queue scrollbar
        // param write-backs for the next pump).
        self.apply_scroll_delta(h, v, ctx);
    }
```

- [ ] **Step 2: Repoint the editor push sites**

- `src/widgets/editor.rs:1838` and `:2128`: `ctx.request_sync_editor_delta(id, self.h_scroll_bar, self.v_scroll_bar)` → `ctx.request_scroll_sync(id, self.h_scroll_bar, self.v_scroll_bar)`.
- `src/widgets/editor.rs:~4550-4556`: the dedup guard (`matches!(d, Deferred::SyncEditorDelta { editor, .. } if *editor == id)`, see also editor.rs:3543) → `matches!(d, Deferred::ScrollSync { target, .. } if *target == id)` and the constructed `Deferred::SyncEditorDelta { editor, h, v }` → `Deferred::ScrollSync { target: editor, h, v }`. **Also check editor.rs:3543** — there is a second `matches!(d, Deferred::SyncEditorDelta { editor, .. } if *editor == id)` guard that must become `Deferred::ScrollSync { target, .. } if *target == id`.

- [ ] **Step 3: Confirm FileEditor + Memo forward `apply_scroll_sync` (do not skip it)**

Run:
```bash
grep -n "skip(" src/widgets/editor.rs
```
Expected: neither the `#[crate::delegate(to = editor)]` on `Memo` (editor.rs:2345) nor on `FileEditor` (editor.rs:2562) lists `apply_scroll_sync` in a `skip(...)`. (They delegate it — confirming the trait call reaches the inner `Editor`.) If either skips it, REMOVE `apply_scroll_sync` from that `skip` list so it forwards.

- [ ] **Step 4: Remove the variant, request method, and pump arm**

Delete `Deferred::SyncEditorDelta` (context.rs 354–361), `request_sync_editor_delta` (context.rs 1466–1474), and the pump arm (program.rs 2314–2331).

- [ ] **Step 5: Verify removed**

Run:
```bash
grep -rn "SyncEditorDelta\|request_sync_editor_delta" src/
```
Expected: no matches.

- [ ] **Step 6: Build, test, lint**

Run:
```bash
cargo test --workspace -j2 -- --test-threads=2
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```
Expected: all green; editor scroll snapshots byte-identical. The editor scroll-sync now flows: scrollbar broadcast → `request_scroll_sync` → `ScrollSync` arm → `FileEditor::apply_scroll_sync` (delegated) → `Editor::apply_scroll_delta`.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit
# message: "refactor(editor): route scrollbar read-sync through apply_scroll_sync (delegation peels to inner Editor)"
```

---

## Task 5: De-downcast the Indicator (keep its variant; §2.1 reason recorded)

**Files:**
- Modify: `src/view/view.rs` — add `set_indicator_value` defaulted method.
- Modify: `tvision-rs-macros/src/specs.rs` — add a forwarder.
- Modify: `tests/delegate_view.rs` — add a spy entry (impl + call + name).
- Modify: `src/app/program.rs` — de-downcast the `IndicatorSetValue` arm (2335–2348).
- Modify: `src/widgets/indicator.rs` — add the `set_indicator_value` override; record the §2.1 reason on the variant doc in `src/view/context.rs` (~366).

**Rationale (record in the variant doc):** `IndicatorSetValue` carries `{ location: Point, modified: bool }` — an editor→indicator *push*, not a sibling-scrollbar `(h, v)` read. Folding it into `apply_scroll_sync` would force `ScrollSync` to carry unrelated fields and make the hook murkier than the downcast it removes (§2.1 test fails). It keeps its own variant and gets its own defaulted hook — de-downcast in place.

**Interfaces:**
- Produces: `View::set_indicator_value(&mut self, _location: Point, _modified: bool)` (defaulted no-op); `Indicator::set_indicator_value` override delegating to its existing inherent `set_value(location, modified)`.

- [ ] **Step 1: Add the defaulted trait method**

In `src/view/view.rs`, near the other broker hooks, add:
```rust
    /// The editor→indicator status-push broker hook. Defaulted no-op; the editor's
    /// status [`Indicator`](crate::widgets::Indicator) overrides it to store the new
    /// cursor `location` + `modified` flag. Driven by
    /// [`Deferred::IndicatorSetValue`](crate::view::Deferred::IndicatorSetValue):
    /// the editor (a leaf) cannot reach its indicator sibling inline, so it requests
    /// this and the pump calls the method by id — virtual dispatch, not a downcast.
    fn set_indicator_value(&mut self, _location: Point, _modified: bool) {}
```
Confirm `Point` is in scope in `view.rs` (it is — used by other methods).

- [ ] **Step 2: Add the macro forwarder**

In `tvision-rs-macros/src/specs.rs`, add an entry (mirror the `apply_scroll_sync` entry's shape):
```rust
        ("set_indicator_value",
         quote! { fn set_indicator_value(&mut self, location: #k::geometry::Point, modified: bool) { self.#f.set_indicator_value(location, modified) } }),
```
(Use the correct fully-qualified path for `Point` — match how `Point` is referenced in the existing `specs.rs` entries; if none, use `#k::Point` per the crate's re-export. Verify by building.)

- [ ] **Step 3: Add the spy-test entry**

In `tests/delegate_view.rs`: add an impl method on the spy that marks:
```rust
    fn set_indicator_value(&mut self, _location: Point, _modified: bool) {
        self.mark("set_indicator_value");
    }
```
add a call in the call-list section:
```rust
    d.set_indicator_value(Point::new(0, 0), false);
```
and add `"set_indicator_value"` to the asserted method-name list. (Match the exact patterns of the existing entries — `Point` is already imported in this test if other methods use it; otherwise import it.)

- [ ] **Step 4: Add the Indicator override**

In `src/widgets/indicator.rs`, in the `impl View for Indicator` block, add:
```rust
    fn set_indicator_value(&mut self, location: Point, modified: bool) {
        self.set_value(location, modified);
    }
```
(`set_value` is the existing inherent method at indicator.rs:85.)

- [ ] **Step 5: De-downcast the pump arm**

In `src/app/program.rs`, replace the `IndicatorSetValue` arm (2335–2348) body with a trait call:
```rust
                            Deferred::IndicatorSetValue {
                                indicator,
                                location,
                                modified,
                            } => {
                                if let Some(ind) = group.find_mut(indicator) {
                                    ind.set_indicator_value(location, modified);
                                }
                            }
```
Remove the now-unused `use crate::widgets::Indicator;` from this arm.

- [ ] **Step 6: Record the §2.1 reason on the variant doc**

In `src/view/context.rs`, on the `IndicatorSetValue` variant doc (~366), append a sentence: that it stays a separate variant + its own `set_indicator_value` hook (not folded into `ScrollSync`/`apply_scroll_sync`) because its payload is a `(location, modified)` push, not a scrollbar `(h, v)` read — §2.1.

- [ ] **Step 7: Verify the downcast is gone**

Run:
```bash
grep -n "downcast_mut::<Indicator>" src/app/program.rs
```
Expected: the `IndicatorSetValue` arm no longer matches (one remaining hit at program.rs:6725 is a *different*, non-Phase-3 site — confirm it is not the `IndicatorSetValue` arm; leave it).

- [ ] **Step 8: Build, test, lint**

Run:
```bash
cargo test --workspace -j2 -- --test-threads=2
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```
Expected: all green; the new `delegate_view` spy assertion passes; editor/indicator snapshots byte-identical.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit
# message: "refactor(indicator): deliver IndicatorSetValue via set_indicator_value trait method (no downcast)"
```

---

## Task 6: De-downcast the PageStack (keep its variant; §2.1 reason recorded)

**Files:**
- Modify: `src/view/view.rs` — add `apply_page_sync` defaulted method.
- Modify: `tvision-rs-macros/src/specs.rs` — add a forwarder.
- Modify: `tests/delegate_view.rs` — add a spy entry.
- Modify: `src/app/program.rs` — de-downcast the `PageStackSync` arm (2534–2554).
- Modify: `src/widgets/page_stack.rs` — add the `apply_page_sync` override; record the §2.1 reason on the variant doc in `src/view/context.rs` (~567).

**Rationale (record in the variant doc):** `PageStackSync` reads a *single* source (the tab bar's `value`), not an `(h, v)` scrollbar pair, and its effect is "switch the active page", not "apply a scroll delta". Folding it into `apply_scroll_sync` would overload that hook with an unrelated single-index semantic (§2.1 test fails). It keeps its own variant and gets its own defaulted hook.

**Interfaces:**
- Produces: `View::apply_page_sync(&mut self, _idx: usize, _ctx: &mut Context)` (defaulted no-op); `PageStack::apply_page_sync` override delegating to its existing inherent `set_active(idx, ctx)`.

- [ ] **Step 1: Add the defaulted trait method**

In `src/view/view.rs`, near the other broker hooks, add:
```rust
    /// The tab-bar→page-stack switch broker hook. Defaulted no-op;
    /// [`PageStack`](crate::widgets::PageStack) overrides it to make page `idx`
    /// active. Driven by
    /// [`Deferred::PageStackSync`](crate::view::Deferred::PageStackSync): the pump
    /// reads the bound tab bar's `value` and calls this method by id — virtual
    /// dispatch, not a downcast.
    fn apply_page_sync(&mut self, _idx: usize, _ctx: &mut Context) {}
```

- [ ] **Step 2: Add the macro forwarder**

In `tvision-rs-macros/src/specs.rs`, add:
```rust
        ("apply_page_sync",
         quote! { fn apply_page_sync(&mut self, idx: usize, ctx: &mut #k::Context) { self.#f.apply_page_sync(idx, ctx) } }),
```

- [ ] **Step 3: Add the spy-test entry**

In `tests/delegate_view.rs`: add the spy impl
```rust
    fn apply_page_sync(&mut self, _idx: usize, _ctx: &mut Context) {
        self.mark("apply_page_sync");
    }
```
a call `d.apply_page_sync(0, &mut ctx);`, and `"apply_page_sync"` in the asserted name list.

- [ ] **Step 4: Add the PageStack override**

In `src/widgets/page_stack.rs`, in the `impl View for PageStack` block, add:
```rust
    fn apply_page_sync(&mut self, idx: usize, ctx: &mut Context) {
        self.set_active(idx, ctx);
    }
```
(`set_active` is the existing inherent method at page_stack.rs:71.)

- [ ] **Step 5: De-downcast the pump arm**

In `src/app/program.rs`, replace the `PageStackSync` arm (2534–2554) with:
```rust
                            Deferred::PageStackSync {
                                page_stack,
                                tab_bar,
                            } => {
                                // TabBar::value() is always non-negative, so a plain
                                // `as usize` is safe (no defensive max(0)).
                                let idx = group
                                    .find_mut(tab_bar)
                                    .and_then(|v| v.value())
                                    .and_then(field_int)
                                    .unwrap_or(0);
                                if let Some(ps) = group.find_mut(page_stack) {
                                    ps.apply_page_sync(idx as usize, &mut ctx);
                                }
                            }
```
Remove the now-unused `use crate::widgets::PageStack;` from this arm.

- [ ] **Step 6: Record the §2.1 reason on the variant doc**

In `src/view/context.rs`, on the `PageStackSync` variant doc (~567), append: stays a separate variant + its own `apply_page_sync` hook because it reads a single tab-bar `value` and switches the active page (not an `(h, v)` scroll delta) — §2.1.

- [ ] **Step 7: Verify the downcast is gone**

Run:
```bash
grep -n "downcast_mut::<PageStack>" src/app/program.rs
```
Expected: the `PageStackSync` arm no longer matches (the remaining hits at program.rs:6599/6609 are *different*, non-Phase-3 sites — leave them).

- [ ] **Step 8: Build, test, lint**

Run:
```bash
cargo test --workspace -j2 -- --test-threads=2
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```
Expected: all green; the new spy assertion passes; ColorPicker tab-switch snapshots byte-identical.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit
# message: "refactor(page_stack): deliver PageStackSync via apply_page_sync trait method (no downcast)"
```

---

## Task 7: Docs + final whole-phase verification

**Files:**
- Modify: `docs/book/src/internals/brokering.md` (the only hand-edited doc that names these brokers).
- Modify: `docs/IMPLEMENTATION-LOG.md` (prepend a Phase 3 section).
- Verify only (no edit): the generated `docs/book/book/**` (regenerated by `cargo xtask docs`, separate concern).

**Interfaces:** none (docs + verification).

- [ ] **Step 1: Update the brokering internals guide**

In `docs/book/src/internals/brokering.md`, find the prose describing the scroller/list/outline/editor read-sync brokers and the `apply_list_scroll` hook. Rewrite it to describe the unified `Deferred::ScrollSync` + `View::apply_scroll_sync` (one hook for scroller/list/outline/editor), and note that the Indicator (`set_indicator_value`) and PageStack (`apply_page_sync`) syncs stay separate hooks (the §2.1 reason). Keep the doctest convention (hidden `# use tvision_rs as tv;` for any ```` ```rust ```` block; see HANDOVER "Phase-3 doctest convention").

Run after editing:
```bash
grep -rn "apply_list_scroll\|SyncScrollerDelta\|SyncListViewer\|SyncOutlineViewerDelta\|SyncEditorDelta" docs/book/src/
```
Expected: no matches in the hand-edited `docs/book/src/` tree.

- [ ] **Step 2: Prepend the IMPLEMENTATION-LOG section**

Add a newest-first section to `docs/IMPLEMENTATION-LOG.md` summarizing Phase 3: the four-into-one `ScrollSync` collapse (Scroller/Outline/Editor/list viewers through `apply_scroll_sync`), the two de-downcasted-in-place syncs (Indicator/PageStack, §2.1 reason), the macro forwarders + spy entries added, and that `ScrollBarSetParams`/`SplitterDivider` were deliberately left (out of the spec's "five scroll-family brokers").

- [ ] **Step 3: Final grep-proof — no sync downcast remains in the pump**

Run:
```bash
grep -n "downcast_mut::<Scroller>\|downcast_mut::<Outline>\|downcast_mut::<Indicator>\|downcast_mut::<PageStack>" src/app/program.rs
```
Confirm: none of the matches are inside the deferred-drain sync arms (`ScrollSync`/`IndicatorSetValue`/`PageStackSync`). Remaining hits are non-Phase-3 sites (e.g. the scrollbar-param write helpers around 6482/6725/6841/6903, the page_stack helpers 6599/6609) — those are out of scope and stay.

- [ ] **Step 4: Full integrated-tree gate**

Run:
```bash
cargo test --workspace -j2 -- --test-threads=2
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo build --examples
```
Expected: all green; **no snapshot changes across the whole phase** (every task was behavior-preserving).

- [ ] **Step 5: Update the SDD ledger + HANDOVER**

Append a "PHASE 3 COMPLETE" block to the worktree's `sdd/progress.md` (commits + review verdicts). Update `docs/HANDOVER.md`'s 2026-06-19 section: mark Phase 3 done, point "Next" at Phase 4. Commit:
```bash
git add -A
git commit
# message: "docs(handover,log): Phase 3 sync-trait-methods complete; next is Phase 4"
```

---

## Self-Review

**Spec coverage (§3.2 / §5 Phase 3):**
- "add the defaulted `View` method, move the pump's downcast call to the method, verify, repeat" → Tasks 2–6 each do exactly this per widget.
- "collapse the five scroll-family brokers into ONE shared `apply_scroll_sync` hook" → Tasks 1–4 collapse the *three* true sibling-scrollbar read-syncs (Scroller/Outline/Editor) + the existing list-viewer sync into `apply_scroll_sync`/`Deferred::ScrollSync`. The remaining two of the spec's nominal "five" (Indicator, PageStack) are settled by the §2.1 test as resisting the shared `(h, v)` hook (their payloads are not scroll deltas) and are de-downcast in place with their own hooks + recorded reasons (Tasks 5, 6) — this is the spec's explicit "which (if any) sync genuinely resists the shared hook is settled per-widget in Phase 3 ... reason recorded".
- "each new `View` method needs a `specs.rs` forwarder AND a `tests/delegate_view.rs` entry" → Tasks 1 (rename), 5, 6 each include both steps.
- "no sync site downcasts in the pump" → Task 7 Step 3 grep-proves it.

**Placeholder scan:** every code step shows the actual code; every command shows expected output. No TBD/TODO.

**Type consistency:** `apply_scroll_sync(&mut self, h: Option<i32>, v: Option<i32>, ctx: &mut Context)` is used identically in the trait default (Task 1), forwarder (Task 1), and all overrides (Tasks 2–4). `Deferred::ScrollSync { target, h, v }` with `target: ViewId, h/v: Option<ViewId>` is consistent across the variant def, `request_scroll_sync`, the pump arm, and every push site (Tasks 2–4). `set_indicator_value(location: Point, modified: bool)` and `apply_page_sync(idx: usize, ctx: &mut Context)` are each consistent across trait/forwarder/spy/override/pump (Tasks 5, 6).

**Out-of-scope guard:** `ScrollBarSetParams` and `SplitterDivider` are explicitly excluded (Global Constraints) and confirmed left by Task 7's scoped grep.
