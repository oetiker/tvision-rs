# Session handover — resume at row 33d (drag + close) → TDialog 34

> Living handover for the **next** rstv session. Read this, then
> [CLAUDE.md](file:///home/oetiker/checkouts/rstv/CLAUDE.md) (orientation /
> Current state / Next step), then start the next stage. When it lands, update or
> replace this file for the session after.

## Where things stand (git `main`)

| commit | what |
|--------|------|
| `bff4885` | **Row 31 `TProgram`** — the live event loop (D9): capture stack + timer queue made live |
| `c80a20d` | **Row 30 `TDeskTop`** — `Desktop` = Group + Background (D2); embed-and-delegate-a-Group exemplar; faithful `defaultBkgrnd` ░ |
| `4da4f52` | **Row 33a** — Group/Context primitives for TWindow |
| `d44e39b` | **Row 33b** — TWindow core (static selectable window) |
| `432c01a` | **Row 33c** — TWindow zoom (owner-extent channel + downcast seam + zoom/locate + cmZoom enable) |

**Build state:** 269 unit + 3 integration tests green; `cargo clippy --all-targets
-- -D warnings` and `cargo fmt --check` clean. Working tree clean.

**Phase 2 progress:** row 30 done; **row 33 is staged** — 33a + 33b + 33c done,
**33d remains**; then **row 34 `TDialog`**. The goal is still "a window you can see
and drive": you can now **see**, **select/raise**, and **zoom** a window; 33d makes
it **draggable + closable** (move/resize/close + cmNext/cmPrev).

## The row-33 staging (decided + partly executed)

Row 33 (`TWindow`) is too big for one pass; it was decomposed (advisor-sharpened).
The gating realization: **`pump_once` drops disabled commands at the program
boundary**, and `cmZoom`/`cmClose`/`cmResize`/`cmNext`/`cmPrev` start **disabled**;
only `TWindow::setState(sfSelected)`→`enableCommands` turns them on. So a view must
reach the program's command set — a D3 view→owner problem. There are **three**
such view→owner effects in TWindow, each needs a downward channel:

1. **`select()`→`makeFirst`** (Z-reorder) — internal to `Group`. ✅ **done in 33a.**
2. **`setState`→enable/disable commands** — deferred command-enable channel on
   `Context`. ✅ **channel done in 33a; used by 33c/33d's setState.**
3. **`close()`→`destroy(this)`** (self-removal) — a close-removal channel. **33d.**

### ✅ 33a (commit `4da4f52`) — `docs/briefs/row33a-group-context-primitives.md`
- **Deferred command-enable channel** on `Context`: `command_changes:
  Vec<(Command,bool)>` + `Context::enable_command`/`disable_command`; `Program`
  has `pending_command_changes`, applied after dispatch (mirrors
  `pending_captures`), flips `command_set_changed`. Threaded the new 5th
  `Context::new` arg through all ~17 call sites.
- **Z-reorder on `Group`**: `put_in_front_of(id, target, ctx)` + `make_first(id,
  ctx)` (faithful `putInFrontOf`/`makeFirst`, D8 drawHide/drawShow dropped,
  `ofSelectable`→`resetCurrent` tail kept). **`None` is a deliberate to-top
  sentinel for `make_first`, NOT C++ `Target==0` (which is send-to-bottom — has no
  consumer, unimplemented).** Documented.
- **`ofTopSelect` rewire**: selecting a `top_select` child raises it via
  `make_first` (faithful `select()`); validate gate (`focus()`) preserved;
  `focusNext`→`focus()` raises `ofTopSelect` views (faithful). Raise-on-click test
  uses the realistic desktop layout (non-selectable Background at `children[0]`/
  bottom, so `firstMatch` skips it and returns the raised window as current).

### ✅ 33b (commit `d44e39b`) — `docs/briefs/row33b-twindow-core.md`
New module `src/window/{mod,window.rs}`. `Window` embeds a `Group` (D2), delegates
the `View` trait, overriding `draw`/`handle_event`/`set_state`/`size_limits`.
- ctor ports `TWindow::TWindow` verbatim (flags wfMove|wfGrow|wfClose|wfZoom,
  zoom_rect=bounds, palette=Blue, sfShadow + ofSelectable|ofTopSelect, growMode
  gfGrowAll|gfGrowRel). Builds the `Frame` directly + pushes title/flags/number
  down (D3) + inserts as a group child. **No frame factory** (under D3 a custom
  frame needs the downcast seam → reintroduce at 33c if needed; `TODO(33c)` in
  `window.rs`).
- **`WindowFlags` relocated** from `frame.rs` to the `window` module (crate-root
  re-export preserved). **`WindowPalette`** enum (Blue/Cyan/Gray; getPalette under
  D7 — blue scheme only, multi-scheme → row 34 gray dialogs).
- `getTitle`; `sizeLimits` override (min `minWinSize {16,6}`); **`calc_bounds`
  deliberately NOT overridden** so the trait default routes through
  `Window::size_limits` (the 16×6 floor — delegating to `group.calc_bounds` would
  use its 0×0 min; load-bearing, commented — do NOT add a delegating override).
- `set_state`: **activation only** (sfSelected → sfActive self-recursion → frame
  goes active via `Group::set_state(Active)` propagation, no manual frame push).
  **Command-enable DEFERRED** (`TODO(33c)` names the exact set).
- `standard_scroll_bar` (vertical/horizontal edge rects + sbHandleKeyboard →
  ofPostProcess); `handle_event`: delegate group + kbTab/kbShiftTab → `focus_next`
  (Shift+Tab is `Key::Tab` + `shift`; no `BackTab`).
- **Deferred to 33c/33d** (breadcrumbs in `window.rs`, no dead stubs): zoom,
  cmResize/move/grow drag, close/destroy, the setState command-enable set,
  cmSelectWindowNum match (D4 dropped the payload).

### ✅ 33c (commit `432c01a`) — `docs/briefs/row33c-twindow-zoom.md`
- **Owner-extent-down channel on `Context`**: a transient `owner_size: Point` field
  + `owner_size()`/`set_owner_size()`. **Decided (advisor): a defaulted field +
  setter, NOT a `Context::new` param** — `Group::handle_event` mutates it during
  dispatch so a setter exists regardless; a param would be pure churn over ~17 call
  sites and conceptually muddies the disjoint-borrow model (the other four fields
  are loop-owned `&mut` channels; this is transient routing state). Each
  `Group::handle_event` sets it to its own size before routing and **restores
  unconditionally** — the three-phase body was extracted into `Group::route_event`
  so the positional arm's early `return` can't skip the restore. So when
  `Window`'s cmZoom arm runs after `self.group.handle_event`, `owner_size` is back
  to the *desktop's* size. **Caveat (33d):** `owner_size` is valid only during
  group-routed dispatch; a capture handler sees `(0,0)`, so 33d's drag handler must
  capture its limits at *push time* (inside `Window::handle_event`), not read them
  at drag time.
- **Downcast seam**: defaulted `View::as_any_mut() -> Option<&mut dyn Any>` (base
  `None` → no ripple; `Frame` overrides `Some(self)`) + `Group::child_mut(id) ->
  Option<&mut dyn View>`. `Window::zoom` pushes `set_zoomed` through it.
- **`zoom()` + `locate()`** (faithful `TWindow::zoom`/`TView::locate`): size!=max
  toggle, zoom_rect save/restore, `range`-clamp to sizeLimits; locate's
  owner!=0/drawUnderRect tail dropped (D8); local `range` (tview.cpp). cmZoom in
  `handle_event` (after the group delegate; infoPtr guard dropped, D4).
- **`setState` enables cmZoom only** (33a channel). **DIVERGENCE (documented):**
  C++ enables {cmNext,cmPrev,cmResize,cmClose,cmZoom} atomically; 33c enables only
  cmZoom — the one command whose handler exists. Rest → 33d/row 34.
- Milestone snapshots: window restored vs zoomed-to-fill (frame fills, scrollbar
  resizes, `[↑]`→`[↕]` icon). Two-stage reviewed (SPEC-PASS + QUALITY-PASS).

## NEXT: 33d (drag + close + cmNext/cmPrev) then TDialog 34

Run these the established way: own the design on the main thread, **advisor
consult before writing**, dispatch an Opus implementer against a self-contained
brief (`docs/briefs/` has the 33a/33b/33c templates), **two-stage review**
(spec-compliance → code-quality, fresh agents), fix via a fresh agent with a
precise change-list, integrate, commit at the stage boundary.

### 33d — make the window **draggable + closable**
1. **Drag capture handlers** (port `dragView`/`moveGrow` mouse branch as transient
   `CaptureHandler`s — the live loop + capture stack make this buildable). On a
   `wfMove` frame row-0 click (the frame currently leaves it unconsumed — see
   `frame.rs` `TODO(row 33, D9)`) the window pushes a **move** capture that tracks
   MouseMove and `moveGrow`s `origin` until MouseUp; on a `wfGrow` bottom-corner
   click, a **grow** capture. Use `ctx.owner_size()` (33c channel) for limits.
   Defer the **keyboard-resize** sub-mode (arrows-until-Enter/Esc, `cmResize` from a
   menu) unless cheap — separate capture mode.
2. **Close-removal channel** (the "genuinely hard" one). A window can't remove
   itself (no owner pointer; doesn't know its own `ViewId`). Recipe: `Context`
   gains a close-request signal (`request_close()` setting a flag/Option); the
   **owning `Group`**, right after the `deliver` that ran a child's `handle_event`,
   checks-and-clears the flag and records that child's idx; removes it **after the
   group's full dispatch completes** (not mid-phase — index shifts). Check-and-clear
   at the *innermost* deliver makes nesting (root→desktop→window) unambiguous.
   Window `cmClose`: `valid(cmClose)`; if `sfModal` → post `cmCancel` (do NOT
   remove — row 34 owns modal teardown); else request close.
3. **setState**: extend the enabled set to cmResize/cmClose; land TDeskTop's
   deferred cmNext/cmPrev (`src/desktop/desktop.rs` `TODO(row 33, D9)` — now
   buildable: `focus_next` + `put_in_front_of` exist).
4. Also land the row-25 scrollbar **auto-repeat + thumb-drag** (capture handlers)
   if not split into a Batch-B widget pass — it's independent of the window.

### Row 34 — `TDialog` (the modality payoff)
Consumes the `ModalFrame` seam shipped in row 31. Design `exec_view`/
`executeDialog` + the push→run-until-`valid(end_state)`→pop lifecycle on
`Program` — **the pop is conditional on `valid(end_state)`** (the crux; a view
can't reach the loop, so `exec_view` owns it). `cmOK`/`cmCancel`; gather/scatter
typed values (D10). Gray window scheme (`WindowPalette::Gray`) drives the
multi-scheme theming deferred in 33b (introduce cyan/gray `Role`s or a scheme
mechanism in the `Theme`). See the row-31 "modality seam" notes (below) + the
`row 34` breadcrumbs in `src/app/program.rs`.

## The row-31 modality seam — still the plan for row 34
Row 31 shipped the modality **mechanism only**: a `ModalFrame` capture handler
gating positional events to the modal view. **Row 34 adds:** the frame-pop is
row 34's job (`CaptureStack` has no external pop; a `ModalFrame` only gets `&mut
Context` and can't observe `end_state`), and it must be **conditional on
`valid(end_state)`** — so the push→run→pop lifecycle belongs to `exec_view` (a
`Program` method that owns `end_state`/`valid_end`). Zero test coverage of the pop
path until row 34; breadcrumb `row 34` in `program.rs`.

## Process reminders
- Subagent-driven worked well again (30/33a/33b): main-thread design + **advisor
  consult before writing** + Opus implementer against a written `docs/briefs/`
  brief + fresh-agent two-stage review (spec then quality) + fresh-agent fixes.
- Keep reviewers **adversarial against the C++**, not just the brief — the 33a
  spec reviewer re-derived the `firstMatch`/raise-on-click ring math; the 33b
  quality reviewer caught the hollow `create_frame` factory.
- These are single main-thread FOUNDATION stages (no parallel fan-out) → **no
  worktree**. The later widget batches (B–E) are the parallel ones.
- **Commit at clean reviewed stage boundaries** (the project workflow).

## Outstanding TODOs seeded in code (grep for them)
- `TODO(33d)` in `src/window/window.rs` — cmResize (drag), cmClose (close-removal),
  cmNext/cmPrev in the setState command-enable set, and re-pushing `set_zoomed` on
  owner resize (the pushed-bool staleness vs C++'s per-draw recompute). The
  cmSelectWindowNum match stays deferred (D4 dropped the payload). The downcast
  seam + owner-extent channel that 33d's drag reuses are **already in** (33c).
- `row 34` in `src/app/program.rs` — `exec_view`/`executeDialog`/getData/setData +
  the `ModalFrame` pop lifecycle (conditional on `valid(end_state)`).
- `TODO(row 33, D9)` in `src/frame.rs` — close press-and-hold confirm, `wfMove`
  drag, grow drags, middle-button move (now buildable: capture is live).
- `TODO(row 31, D9)` in `src/widgets/scrollbar.rs` — scrollbar auto-repeat +
  thumb-drag (capture handlers).
- `TODO(row 33, D9)` in `src/desktop/desktop.rs` — TDeskTop cmNext/cmPrev (now
  unblocked by 33a's Z-reorder + select).
- `TODO(row 33)` in `src/view/group.rs` — shadow casting in `Group::draw` (still
  deferred; needs a shadow-dim mechanism in DrawCtx/Theme).
- Sibling tee-walk + full `framelin.cpp` machinery — deferred (see `src/frame.rs`).
- Row 9 `Glyphs` continues to fill in per-widget.

## `cargo doc` cleanup (opportunistic)
`cargo doc -D warnings` is pre-existing-broken project-wide on
`private_intra_doc_links`. Not in the normal gate (test/clippy/fmt). A small
separate cleanup pass would make `cargo doc` clean.
