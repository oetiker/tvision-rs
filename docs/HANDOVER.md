# Session handover — resume at row 33d-2 (selection: cmNext/cmPrev + Alt-N)

> Living handover for the **next** rstv session. Read this, then
> [CLAUDE.md](file:///home/oetiker/checkouts/rstv/CLAUDE.md) (orientation /
> Current state / Next step), then start. When 33d-2 lands, update or replace this
> file for the session after.

## Where things stand (git `main`)

| commit | what |
|--------|------|
| `7b15782` | Substrate realignment — global `ViewId` + self-id + `find_mut`/`remove_descendant` |
| `7efecb3` | Phase A — `Event::Broadcast { command, source }` (D4 amendment) |
| **`2887e95`** | **Row 33d-1 — TWindow drag + close + setState command set** |

**Build state:** 278 unit + 3 integration + 1 doctest green; `cargo clippy
--all-targets -- -D warnings` and `cargo fmt --check` clean. Working tree clean.

## What the last session did (33d-1 — read this; it shaped 33d-2's scope)

**33d was split** at its natural seam (advisor call). 33d-1 (the *interactive*
half — drag, close, setState) is **done + committed**; 33d-2 (the *selection*
half) is **NEXT**. The split kept 33c's "enable only commands whose handlers
exist" principle clean: 33d-1 enabled only `{cmClose, cmZoom}`; 33d-2 adds
`{cmNext, cmPrev}` **together with** the TDeskTop handler.

What 33d-1 built (brief:
[`docs/briefs/row33d-1-drag-close.md`](briefs/row33d-1-drag-close.md)):

- **Deferred tree-op channel on `Context`** — `TreeOp {ChangeBounds, SetState,
  Close}` + `request_bounds`/`request_set_state`/`request_close`, the **third
  member** of the deferred-channel family (`pending_captures` /
  `command_changes` / `pending_tree_ops`). The pump drains it after dispatch
  and applies each against the root `group` via `find_mut`/`change_bounds`,
  `find_mut`/`set_state`, `remove_descendant` — drain-to-local-then-rebuild-ctx
  (the row-31 destructure discipline). **Reuse this channel for 33d-2 if needed,
  but selection mostly uses `focus_next`/`put_in_front_of` directly.**
- **Drag = a `DragCapture` capture handler** (D9, replaces `dragView`'s nested
  `mouseEvent` loop). The **window** (not the frame — D3: a frame can't name the
  window it would move) starts the drag from a still-live `MouseDown` after group
  delegation, replicating `TFrame::handleEvent`'s geometry. `move_grow` ports
  `TView::moveGrow` verbatim. sfDragging on directly / off via the deferred
  channel. **(0,0)-desktop absolute-coords assumption documented** on
  `DragCapture` (matches `ModalFrame`'s caveat) — revisit when a menu/status bar
  shifts the desktop (Phase 4).
- **`cmClose`** → if `sfModal` post `cmCancel` (row 34 owns teardown) else
  `request_close` if `valid(cmClose)`. **No target guard** (Phase A vacuous).
- **`setState`** enable set = `{cmClose if wfClose, cmZoom if wfZoom}`.

Two-stage reviewed (SPEC-PASS after strengthening a vacuous `dmLimitLoY` clamp
test; QUALITY-PASS).

## NEXT — row 33d-2 (selection: cmNext/cmPrev + Alt-N + numbered windows)

Design on the main thread; **advisor consult before writing**; Opus implementer
against a written `docs/briefs/row33d-2-*.md`; fresh-agent two-stage review (spec
→ quality); integrate; commit at the boundary. Single main-thread FOUNDATION stage
→ **no worktree**. This stage touches `src/view/view.rs` (trait), `src/view/group.rs`,
`src/desktop/desktop.rs`, `src/window/window.rs`, `src/app/program.rs`.

Design of record (validated with the advisor in the 33d design session — these are
the decided mechanisms, not open questions):

1. **`View::number(&self) -> Option<i16>`** (default `None`). `Window` overrides:
   `Some(self.number)` if `self.number > 0` else `None`. **Resolve the name clash:
   make this the ONLY `number()` — drop `Window`'s inherent `number()->i16`
   getter** (a same-name/different-return inherent+trait pair will be flagged), and
   update the one test asserting `w.number()==3` → `View::number(&w)==Some(3)`. The
   field stays named `number`.

2. **`Group::focus_by_number(num, ctx) -> bool`** — iterate children, find the
   selectable one whose `view.number() == Some(num)`, `focus_child` it, return
   whether matched.

3. **`View::select_window_num(&mut self, num, ctx) -> bool`** (default `false`) —
   a trait-level tree op (consistent with `find_mut`/`remove_descendant`).
   `Desktop` overrides → `self.group.focus_by_number(num, ctx)`. **Use the trait
   method, NOT an `as_any_mut` downcast** (keeps the call site clean, avoids
   coupling Program to concrete Desktop).

4. **TDeskTop `cmNext`/`cmPrev`** (`src/desktop/desktop.rs`, the `TODO(row 33, D9)`
   breadcrumb — now buildable). After `group.handle_event`, on a command:
   `cmNext` → `if self.group.valid(RELEASED_FOCUS) { self.group.focus_next(false, ctx) }`;
   `cmPrev` → `if valid { if let Some(cur)=self.group.current() { self.group.put_in_front_of(cur, self.background, ctx) } }`;
   then `clearEvent` **for cmNext/cmPrev only** (C++: `default: return` without
   clearing; the `clearEvent` after the switch is reached only for the two cases).
   `put_in_front_of`/`focus_next`/`background` all exist.

5. **Alt-N (`cmSelectWindowNum`)** in `program_handle_event` — **BEFORE**
   `group.handle_event` (faithful C++ order). An Alt+digit keydown (`Key::Char('1'
   ..='9')` + `modifiers.alt`; the `getAltChar` equivalent) → `canMoveFocus` =
   `group.find_mut(desktop_id).map_or(false, |dt| dt.valid(RELEASED_FOCUS))`
   (desktop-specific, NOT root `.valid()`); if `can`,
   `group.find_mut(desktop_id).map_or(false, |dt| dt.select_window_num(num, ctx))`
   → `clearEvent` if it matched; **also `clearEvent` when `!can`** (faithful to
   `tprogram.cpp`). Add `desktop: Option<ViewId>` to `program_handle_event`'s
   params (it currently takes `group, ev, ctx, end_state`). **NOT** a number-
   carrying broadcast — the number is an integer, not a `ViewId`, so the
   `Broadcast` `source` substrate does not serve it.

6. **`setState`** — add `{cmNext, cmPrev}` to the window's enable set (alongside
   the 33d-1 `{cmClose, cmZoom}`), now that the desktop handler exists. (`cmResize`
   stays **un**enabled — keyboard resize sub-mode still deferred.)

C++ source (read it): `tdesktop.cpp` `TDeskTop::handleEvent`; `tprogram.cpp`
`TProgram::handleEvent` (the Alt-N block) + `canMoveFocus`; `twindow.cpp`
`TWindow::handleEvent` (the `cmSelectWindowNum` broadcast arm — we realize it as
the direct walk above, not a broadcast) + `setState`; `tgroup.cpp` `selectNext`.

Tests: integration through `pump_once` — inject Alt+digit, assert the numbered
window became `current`; cmNext/cmPrev cycle the desktop's windows (assert
`current` / Z-order changed). Plus unit tests for `focus_by_number` and
`View::number`.

## Still deferred after 33d-2
- **`cmResize` keyboard resize sub-mode** (`dragView`'s `else` arrows-until-Enter/
  Esc branch) — no menu can trigger `cmResize` yet; revisit when menus land (and
  enable `cmResize` in `setState` only then). `TODO(33d-2/later, D9)` breadcrumb in
  `window.rs`.
- **Scrollbar auto-repeat + thumb-drag** (`scrollbar.rs` `TODO(row 31, D9)`) —
  capture handlers, independent of the window → **Batch B widget pass**.
- **Close press-and-hold release-confirm loop** (`frame.rs` `TODO(row 33, D9)`) —
  we post `cmClose` on mouse-down.
- **Modal teardown** (`exec_view`/`executeDialog` + the `ModalFrame` pop, the
  `message()`/`query` return-consuming primitive) → **row 34 (`TDialog`)**.
- Sibling tee-walk, multi-scheme theming, shadow casting, row-9 glyphs — as before.

## Row 34 — `TDialog` (the modality payoff)

Consumes the row-31 `ModalFrame` seam. Design `exec_view`/`executeDialog` + the
push→run-until-`valid(end_state)`→**pop (conditional on `valid(end_state)`)**
lifecycle on `Program` (the crux: a view can't reach the loop, so `exec_view` owns
it). `cmOK`/`cmCancel`; gather/scatter typed values (D10). Gray window scheme drives
the deferred multi-scheme theming. **Also build the return-consuming
`message()`/`query` tree-owner primitive here** — first consumer is the dialog
`cmCanCloseForm` veto (design of record: guide D4 "message() — corrected").

## Process reminders
- Subagent-driven worked well (main-thread design + **advisor consult before
  writing** + Opus implementer against a written `docs/briefs/` brief + fresh-agent
  two-stage review — keep reviewers adversarial against the **C++ + corrected
  guide**, not just the brief).
- Single main-thread FOUNDATION stages → **no worktree**. Commit at clean reviewed
  stage boundaries.
- **Split a too-large stage at its natural seam** (memory `fix-foundations-not-
  bandaids` is the cousin): 33d → 33d-1/33d-2 made the review tractable AND made
  the command-enable staging fall out cleanly. The advisor flagged the split.
- **The verification that matters here is the `pump_once` round-trip**, not a
  capture/handler unit test in isolation — a unit test of the handler proves
  nothing about the deferred-channel drain or the borrow discipline.

## Outstanding TODOs seeded in code (grep)
- `TODO(33d-2/later, D9)` in `src/window/window.rs` — cmResize keyboard sub-mode.
- `TODO(row 33, D9)` in `src/desktop/desktop.rs` — cmNext/cmPrev (33d-2, now
  buildable: `focus_next` + `put_in_front_of` exist).
- Alt-N breadcrumb in `src/app/program.rs` `program_handle_event` (33d-2).
- `row 34` in `src/app/program.rs` — `exec_view`/`executeDialog` + the `ModalFrame`
  pop lifecycle + the `message()` primitive.
- `TODO(row 33, D9)` in `src/frame.rs` — close press-and-hold confirm, plus the
  frame's own drag cases (now handled window-side via DragCapture; the frame still
  just leaves them unconsumed — that breadcrumb can be trimmed/clarified in 33d-2).
- `TODO(row 31, D9)` in `src/widgets/scrollbar.rs` — auto-repeat + thumb-drag.
- `TODO(row 33)` in `src/view/group.rs` — shadow casting in `Group::draw`.
- Sibling tee-walk + full `framelin.cpp` machinery — deferred (`src/frame.rs`).
- Row 9 `Glyphs` continues to fill in per-widget.
