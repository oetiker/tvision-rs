# Session handover — resume at the message()/payload cleanup → then row 33d

> Living handover for the **next** rstv session. Read this, then
> [CLAUDE.md](file:///home/oetiker/checkouts/rstv/CLAUDE.md) (orientation /
> Current state / Next step), then start. When it lands, update or replace this
> file for the session after.

## Where things stand (git `main`)

| commit | what |
|--------|------|
| `bff4885` | Row 31 `TProgram` — the live event loop (D9) |
| `c80a20d` | Row 30 `TDeskTop` |
| `432c01a` | Row 33c — TWindow zoom |
| `8f2d11d` | (handover doc for the 33c→33d session) |
| **`7b15782`** | **Substrate realignment — global `ViewId` + self-id + `find_mut`/`remove_descendant`** |

**Build state:** 271 tests green (267 unit + 3 integration + 1 doctest); `cargo
clippy --all-targets -- -D warnings` and `cargo fmt --check` clean. Working tree clean.

## What the last session did (read this — it changes 33d)

We stopped to fix a foundation rather than bandaid around it (see the new memory
`fix-foundations-not-bandaids`). Root cause: `ViewId`s were **group-local** (each
`Group` embedded its own generational `ViewArena`) — an unexamined default that
contradicted the guide's own D3 promise ("resolve a `ViewId` by a tree-walk via
`Context`") and whose `is_valid` validator was **dead code**. That group-locality
was the real obstacle behind 33d's drag/close.

**Landed (`7b15782`):** one process-global monotonic `ViewId` (`NonZeroU64`),
stamped into each view's own `ViewState.id` at `Group::insert`, resolved by
`View::find_mut(id)` / `remove_descendant(id, ctx)` (Group recurses; Window/Desktop
delegate; Frame leaf). The dead arena is gone. Guide corrected: **D3 "Resolution
substrate — corrected"** and **D4 "`message()` — corrected"**.

**The downstream realization (D4 amendment, designed, NOT yet built):** the C++
`message()` payload + targeted query were droppable *only because* the id substrate
was broken. The whole-tree audit (42 `message()` sites) shows it ports **directly**
onto the new substrate:
- **39/42 fire-and-forget** `message(owner, evBroadcast, cmX, this)` → a
  **`Broadcast { command, source: Option<ViewId> }`** (the `void* infoPtr`
  reinstated as a **resolvable `ViewId`**). Receiver: `source == self.h_scroll_bar`.
- **3/42 consume the return** (Alt-N, an app `cmCanCloseForm` veto in `valid()`, a
  test) — a synchronous "broadcast a question, was it claimed?" — and **all are
  owner-initiated and downward**. They port as one tree-owner primitive:
  `Group::message(target: ViewId, ev) -> Option<ViewId>` = `find_mut(target)` →
  deliver → return the source iff consumed. NOT a `Context` method (Context holds
  no tree). The aliasing rule bars only "a view querying across the tree mid-`handle_event`",
  which the audit shows **never happens**.

## NEXT — Phase A: build `message`/payload, then remove the kludges it was missing

Do this **before** 33d (33d's `cmClose` self-target guard is its first consumer).
Run it the established way: own the design on the main thread, **advisor consult
before writing**, Opus implementer against a written `docs/briefs/` brief,
fresh-agent two-stage review (spec → quality), integrate, commit at the boundary.

### A1 — the infrastructure (D4 amendment)
1. **`Broadcast` carries a source.** Change `Event::Broadcast(Command)` →
   `Event::Broadcast { command: Command, source: Option<ViewId> }` (or add an
   `Event::Message{..}` variant if you want to keep a one-word sourceless broadcast
   — decide in the brief). Thread the `source` through `ctx.broadcast(cmd, source)`
   and the pump's broadcast fan-out. Receivers match on `source`.
2. **`message`/`query` tree-owner primitive.** Add `Group::message(id, ev) ->
   Option<ViewId>` and a read-only `Group::query(id, …) -> Option<T>` over
   `find_mut`; `Program` exposes the same via its root group. Faithful to C++
   `message()` (deliver synchronously, return the payload iff the receiver
   consumed). Document: only a tree owner may call it (the only place C++ ever
   calls a return-consuming `message()` from).
3. Keep the `cmTimerExpired` timer-id payload **separate** — it carries *which
   timer*, not a `ViewId`; its own design, when a widget needs it.

### A2 — remove the kludges introduced because the payload/query was missing
Each is a `grep`-able breadcrumb today:
- **`src/widgets/scrollbar.rs`** (`:20,:24,:267`) — `cmScrollBarChanged` /
  `cmScrollBarClicked` broadcast with the `this` payload dropped. Now include the
  scrollbar's `source: ViewId` so a scroller/editor with two bars can tell which
  changed (`infoPtr == hScrollBar` → `source == self.h_scroll_bar`).
- **`src/window/window.rs`** (`:350`) — the `cmZoom` `infoPtr == 0 || == this`
  self-target guard was dropped. Restore it now (and `cmClose` gets the same guard
  in 33d) so a zoom/close command targets the right window when several exist.
- **Alt-N window selection** — `src/app/program.rs` (`:38-39,:587-588`),
  `src/window/window.rs` (`:363-364`), `src/window/mod.rs` (`:22`),
  `src/command.rs` (`:147` `cmSelectWindowNum`). Realize it: program (a tree owner)
  resolves the numbered window — either `message`/broadcast carrying the number, or
  a direct walk of the desktop's children by `number` + `select()`. Pick the
  simpler in the brief.
- **`src/view/context.rs`** (`:240`) — the "`query` deferred" note: update it.
  `query` is now a **tree-owner** primitive (Group/Program over `find_mut`), NOT a
  `Context` method — Context holds no tree. Correct the comment.
- **`src/view/view.rs`** (`:600`) — the relocated-`handleEvent` broadcast note that
  mentions the dropped `owner`/`this` payload: revisit once `source` exists.

(Scope note: don't over-reach. `owner_size`, the deferred `command_changes`
channel, `pending_captures`, owner-data-down to the frame, and the `Frame`
`as_any_mut` downcast seam are **legitimate D3 downward needs**, not message
kludges — leave them.)

## THEN — Phase B: row 33d (drag + close + cmNext/cmPrev), now simplified

The substrate dissolves the hard parts the old handover agonized over (the
close-removal channel + drag path-building are **gone**). Design on the main
thread; advisor consult; Opus implementer; two-stage review.

1. **Drag = a capture handler** (the D9 replacement for `dragView`'s nested
   `mouseEvent` loop — the capture stack is the centerpiece, do not route around
   it). Flow:
   - The **frame** leaves a row-0 / bottom-corner mouse-down unconsumed (it already
     consumes close/zoom; see `frame.rs` `TODO(row 33, D9)`). The **window**, after
     delegating to its group, detects the unconsumed `MouseDown` and starts the
     drag — it knows its own id via **`self.state().id`** and its limits via
     **`ctx.owner_size()`** (valid at that point; capture the limits at push time).
   - Push a transient `DragCapture { window_id, kind, anchor, min, max, limits }`.
     Each `MouseMove`: compute new bounds via faithful **`moveGrow`/`locate`**
     (ported in `window.rs` already for zoom) and request the apply via a small
     deferred channel on `Context` (`ctx.request_bounds(id, rect)` — mirrors
     `command_changes`). The **loop** applies it after dispatch via
     `root.find_mut(id).change_bounds(rect)`. `MouseUp` → `ConsumedPop`; the loop
     flips `sfDragging` off via `find_mut(id).set_state(Dragging, false, ctx)`
     (the capture can't call `set_state`; `find_mut` is the uniform apply primitive).
   - `cmResize` keyboard sub-mode (arrows-until-Enter/Esc) — defer unless cheap.
2. **Close** = `cmClose` → `if valid(cmClose)`: if `sfModal` post `cmCancel` (row 34
   owns modal teardown), else `ctx.request_close(self.state().id)`; the loop drains
   it after dispatch via `root.remove_descendant(id, ctx)`. Add the restored
   `infoPtr == this` guard (A2) so the command targets the right window.
3. **`setState`** — extend the enable set to the full C++ `{cmNext, cmPrev,
   cmResize if (grow|move), cmClose if close, cmZoom if zoom}` (33c shipped cmZoom
   only). Land **TDeskTop cmNext/cmPrev** (`src/desktop/desktop.rs` `TODO(row 33,
   D9)` — now buildable: `focus_next` + `put_in_front_of` exist).
4. **Scrollbar auto-repeat + thumb-drag** (`src/widgets/scrollbar.rs` `TODO(row 31,
   D9)`) — capture handlers; independent of the window. Land here or split into a
   Batch-B widget pass.

## Row 34 — `TDialog` (the modality payoff) — unchanged plan
Consumes the row-31 `ModalFrame` seam. Design `exec_view`/`executeDialog` + the
push→run-until-`valid(end_state)`→**pop (conditional on `valid(end_state)`)**
lifecycle on `Program` (the crux: a view can't reach the loop, so `exec_view` owns
it). `cmOK`/`cmCancel`; gather/scatter typed values (D10). Gray window scheme
(`WindowPalette::Gray`) drives the deferred multi-scheme theming. Breadcrumbs:
`row 34` in `src/app/program.rs`.

## Process reminders
- Subagent-driven worked well (substrate stage: main-thread design + **advisor
  consult before writing** + Opus implementer against a written `docs/briefs/`
  brief + fresh-agent two-stage review + fresh-agent fixes).
- Keep reviewers **adversarial against the C++ + the corrected guide**, not just
  the brief.
- Single main-thread FOUNDATION stages → **no worktree**. Commit at clean reviewed
  stage boundaries.
- **When a design forces non-obvious machinery, investigate WHY before bandaiding**
  (memory `fix-foundations-not-bandaids`). The substrate + `message()` fixes both
  came from that.

## Outstanding TODOs seeded in code (grep)
- `TODO(33d)` in `src/window/window.rs` — cmResize (drag), cmClose, cmNext/cmPrev
  in the setState set, re-pushing `set_zoomed` on owner resize.
- `row 34` in `src/app/program.rs` — `exec_view`/`executeDialog` + the `ModalFrame`
  pop lifecycle (conditional on `valid(end_state)`).
- `TODO(row 33, D9)` in `src/frame.rs` — close press-and-hold confirm, `wfMove`
  drag, grow drags, middle-button move (now buildable).
- `TODO(row 31, D9)` in `src/widgets/scrollbar.rs` — auto-repeat + thumb-drag.
- `TODO(row 33)` in `src/view/group.rs` — shadow casting in `Group::draw`.
- Sibling tee-walk + full `framelin.cpp` machinery — deferred (`src/frame.rs`).
- Row 9 `Glyphs` continues to fill in per-widget.
- `cargo doc -D warnings` pre-existing-broken on `private_intra_doc_links`
  (not in the gate; opportunistic cleanup).
