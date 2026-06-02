# Implementer brief — Phase A: `Broadcast` source (D4 amendment, the buildable slice)

You are working on **rstv**, a faithful Rust port of magiblot/tvision (C++ Turbo
Vision) in the `tvision` crate (house alias `tv::`). This is a **FOUNDATION**
change to a core type (`Event`), done on the **main tree** (no worktree). Port
*faithfully* from the C++; the only intentional departures are the pre-decided
deviations (D-rules) named below. Do **not** invent extra features or widen the
scope.

C++ source of truth (read the relevant bits):
- `/home/oetiker/scratch/tvision-spec/magiblot-tvision/source/tvision/tview.cpp`
  (`TView::message`, `TView::setState`'s `sfFocused` → `message(owner,
  evBroadcast, cmReceivedFocus/cmReleasedFocus, this)`).
- `.../source/tvision/tscrlbar.cpp` (`message(owner, evBroadcast,
  cmScrollBarChanged/cmScrollBarClicked, this)`).

Read the **corrected D4 amendment** in `docs/PORTING-GUIDE.md` (search
"`message()` — corrected"). One of your tasks is to correct it further (see Task 5).

---

## Background — why this change, and what is *deliberately out of scope*

The C++ `TEvent` carries a `message.infoPtr` (`void*`) used three unrelated ways:

1. **Broadcast subject** (39 of 42 `message()` call sites): `message(owner,
   evBroadcast, cmX, this)` — `infoPtr == this` says *which view this broadcast is
   about*. A receiver matches `infoPtr == hScrollBar`. **This is the only case you
   build.**
2. **Command target hint** (`cmZoom`/`cmClose`, `infoPtr = owner`): a *target* on
   an `Event::Command`. **NOT built — proven vacuous; see Task 3.**
3. **Integer argument** (`cmSelectWindowNum`, `infoInt = window number`): a plain
   integer payload, not a view. **NOT built here — deferred to row 33d; see Task 4.**

When the project earlier "dropped `infoPtr` per D4", it dropped all three. The
recently-landed global-`ViewId` substrate (`7b15782`) makes case (1) reinstatable
as a **resolvable `ViewId`**, which is what this brief does.

**Scope discipline:** you are only reinstating case (1) — a `source: Option<ViewId>`
on `Event::Broadcast`, threaded from each emitter, with **no new receiver logic**
(no consumer reads `source` yet; the first is a two-scrollbar scroller in a later
batch). Cases (2) and (3) are doc/breadcrumb-only tasks here.

---

## Task 1 — add `source` to `Event::Broadcast`

In `src/event/mod.rs`, change the variant:

```rust
// was: Broadcast(Command),
/// `evBroadcast` — a command broadcast to interested views. `source` reinstates
/// the C++ `message.infoPtr` for the broadcast-subject case (D4 amendment): it
/// names *which view this broadcast is about* (e.g. which scrollbar changed),
/// as a resolvable `ViewId` rather than a `void*`. `None` for broadcasts that
/// are about no particular view (pump-internal `cmCommandSetChanged`,
/// `cmTimerExpired`).
Broadcast { command: Command, source: Option<ViewId> },
```

- Add `use crate::view::ViewId;` to `event/mod.rs` (it is exported at
  `crate::view::ViewId`). `ViewId` is `Copy` (a `NonZeroU64` newtype), so `Event`
  keeps its `#[derive(Clone, Copy, Debug, PartialEq, Eq)]` unchanged.
- Update the module-doc block at the top of `event/mod.rs` (lines ~9–14, the
  "`infoPtr` / `MessageEvent` dropped" paragraph): it currently says the round-trip
  is dropped entirely and "payloaded messages become typed `Context` queries … at a
  later row". Correct it to: `Event::Command` carries only the `Command`;
  `Event::Broadcast` additionally carries an optional **`source: ViewId`** (the
  broadcast-subject successor to `infoPtr`); the synchronous return-consuming
  `message()` primitive is deferred to row 34 (its first return-consumer, a dialog
  `cmCanCloseForm` veto). Keep it accurate and brief.
- Update the `each_variant_constructs` test (`event/mod.rs:161`) to
  `Event::Broadcast { command: Command::OK, source: None }`.

## Task 2 — thread `source` through every emitter and the fan-out

### 2a. `Context::broadcast` signature (`src/view/context.rs:298`)

```rust
/// Broadcast a command (`Event::Broadcast`) into the loop's queue. `source` names
/// the view the broadcast is about (the `infoPtr` successor; D4 amendment), or
/// `None` if it concerns no particular view.
pub fn broadcast(&mut self, command: Command, source: Option<ViewId>) {
    self.out_events.push_back(Event::Broadcast { command, source });
}
```
Add the `ViewId` import to `context.rs` if not already present. Update the
`context_post_and_broadcast_land_in_out_events` test (`context.rs:624,628`)
accordingly (`ctx.broadcast(Command::QUIT, None)` → assert
`Event::Broadcast { command: Command::QUIT, source: None }`).

### 2b. Focus broadcasts — `source = the view whose focus changed` (= C++ `this`)

These are faithful: C++ `setState(sfFocused)` sends `message(owner, …, this)`, so
`source` is the view emitting.

- **`src/view/view.rs:604` base `set_state`** — after `self.state_mut().set_flag(…)`,
  read the id and pass it:
  ```rust
  if flag == StateFlag::Focused {
      let source = self.state().id();   // self == C++ `this`
      ctx.broadcast(
          if enable { Command::RECEIVED_FOCUS } else { Command::RELEASED_FOCUS },
          source,
      );
  }
  ```
  (The `state_mut` borrow has ended; `self.state().id()` is a fresh shared borrow.)
  Update the doc comment (view.rs ~599–603, the "`owner` receiver and the `this`
  `infoPtr` payload are dropped" sentence): now the `this` payload is **carried**
  as `source`; only the `owner` receiver is dropped (broadcast goes to the queue,
  not a receiver). See Task 5 note.

- **`src/view/group.rs:574` Group `set_state` override** — same shape, but the
  group's own focus flip is about the group itself: `let source = self.st.id();`
  then `ctx.broadcast(…, source)`. (The subsequent propagation to the `current`
  child triggers *that child's* `set_state`, which broadcasts with its own id —
  matching C++, where each view that receives `sfFocused` sends its own message.)

### 2c. Scrollbar broadcasts — `source = self.state().id()` (= C++ `this`)

`src/widgets/scrollbar.rs` — all four sites (`:269` `SCROLL_BAR_CHANGED`; `:474`,
`:484`, `:595` `SCROLL_BAR_CLICKED`): pass `self.state().id()` as `source`. Read it
into a local before the `ctx.broadcast` call where borrow-checking needs it.
Update the module-doc paragraphs (`scrollbar.rs:15–24`, "## D4 broadcast"): the
`this` `infoPtr` payload is **no longer dropped** — it is carried as `source` so a
scroller/editor with two bars can tell which bar fired (`infoPtr == hScrollBar`
becomes `source == self.h_scroll_bar`). Keep noting that *this widget adds no
receiver logic* — `source` is consumed by a future two-bar owner (Batch B).

### 2d. Sourceless broadcasts — `source: None`

- **`src/capture.rs:171`** — `ctx.broadcast(Command::COMMAND_SET_CHANGED, None)`
  (a capture handler has no subject view).
- **`src/app/program.rs:444`** — `Event::Broadcast { command:
  Command::COMMAND_SET_CHANGED, source: None }`.
- **`src/app/program.rs:453`** — `Event::Broadcast { command:
  Command::TIMER_EXPIRED, source: None }`. (Leave the existing
  `TODO(timer payload)` comment — the timer-id payload is a *separate* design,
  unrelated to `source`; do not conflate.)

### 2e. The fan-out match arms (route logic — unchanged behavior)

- **`src/view/group.rs:744`** `Event::Broadcast(_) =>` → `Event::Broadcast { .. } =>`.
- **`src/view/group.rs:865`** (the `Probe` test view)
  `!matches!(ev, Event::Broadcast(_))` → `!matches!(ev, Event::Broadcast { .. })`.

Broadcast routing is unchanged: still fans to **every** child (incl. disabled),
back-to-front. `source` is carried along untouched.

### 2f. All test construction / comparison sites

Every `Event::Broadcast(Command::X)` literal becomes
`Event::Broadcast { command: Command::X, source: <…> }`. Equality assertions that
only care about the command should match on the command and ignore the source:
```rust
// was: .any(|e| *e == Event::Broadcast(Command::SCROLL_BAR_CHANGED))
.any(|e| matches!(e, Event::Broadcast { command, .. } if *command == Command::SCROLL_BAR_CHANGED))
```
Sites to fix (grep `Event::Broadcast(` after you finish to confirm none remain):
`src/view/group.rs` (`:1245`, `:1798`, and the focus-broadcast assertions around
`:1074`, `:1085`, `:1089`, `:1113`), `src/widgets/scrollbar.rs` (`:725`, `:771`,
`:867`, `:871`, `:975`, `:1043`), `src/app/program.rs` (`:780`, `:793`, `:801`,
`:1021`, `:1032`). Use grep to find the complete set — line numbers will drift as
you edit.

**Add one focused new test** proving the new data flows: insert a `ScrollBar` into
a `Group`, drive a value change, and assert the queued
`Event::Broadcast { command: Command::SCROLL_BAR_CHANGED, source: Some(id) }` where
`id == the scrollbar's id()` (the id stamped at `Group::insert`). This is the
regression guard that `source` is actually the emitter, not `None`. (A standalone,
never-inserted scrollbar has `id() == None`; that is fine and need not be tested.)

## Task 3 — the cmZoom/cmClose guard: keep no-guard, fix the comment (NO code change)

`src/window/window.rs:~345-383` currently handles `cmZoom` with no
`infoPtr == 0 || == this` guard and a comment claiming the guard is gone "because
D4 dropped payloads". That reasoning is wrong; replace it with the *real* invariant.

**Do not add a target to `Event::Command`.** The guard is provably vacuous in this
architecture, and adding a target would be a Broadcast-sized ripple feeding a check
that rejects nothing. The reason (verified against the C++): the frame posts
`cmZoom`/`cmClose` with `infoPtr = owner` **only while `sfActive`** (`tframe.cpp`
lines 152/171), so its `owner` is always the *active* window; `Event::Command` is a
*focused* event, which the desktop routes to its `current` child = the active
window only; and the internal queue drains before `poll_event`, so the active
window cannot change between post and dispatch. Therefore a `cmZoom`/`cmClose`
command always reaches exactly the window it targets — `infoPtr == 0 || == this`
can never reject anything. Rewrite the `window.rs` doc comment to state this
invariant, and add a trip-wire: *revisit only if a future emitter targets a
non-active window via a command.* (The same reasoning will cover 33d's `cmClose`.)

## Task 4 — Alt-N (`cmSelectWindowNum`): correct the breadcrumbs, do NOT implement

Alt-N's payload is an **integer** (the window number), not a `ViewId`, so the
`source` substrate does not serve it — it belongs with `cmTimerExpired` under
"different payload type, own design". Its realization (a tree owner resolving the
numbered window) also depends on `select()` / `canMoveFocus`, which **do not exist
yet** and land with 33d's window-selection work (`cmNext`/`cmPrev`). So Alt-N
**rides with 33d**, not this phase.

Update these breadcrumbs to say so (replace the stale "D4 dropped the payload that
carried the window number → blocked" wording, which is no longer the obstacle):
- `src/app/program.rs:587-589` (the `TODO` in `program_handle_event`): Alt-1..9 is
  deferred to **33d**; realize it as a **direct walk** — the program (a tree owner)
  asks the desktop to select the child window whose `number` matches, gated by
  `canMoveFocus`. Needs `View::number() -> Option<u16>` (default `None`, `Window`
  overrides) + `select()`/`canMoveFocus` (33d). NOT a payload-carrying broadcast.
- `src/window/window.rs` (the `cmSelectWindowNum` deferral note ~`:363-365`) and
  `src/window/mod.rs:~22` (the "D4 events carry no payload → the
  `cmSelectWindowNum` window-number match defers" bullet): same correction — the
  blocker is the missing select machinery (33d), not a payload story.

(Leave `Command::SELECT_WINDOW_NUM` in `command.rs` as-is — it stays defined.)

## Task 5 — correct the D4 amendment in `docs/PORTING-GUIDE.md`

In the "`message()` — corrected" block (and cross-check D3's "Resolution
substrate"), the current text says `infoPtr` "ports **directly**" onto
`source: Option<ViewId>`. Refine it to reflect that **`infoPtr` is polymorphic**:
- `source: Option<ViewId>` covers the **broadcast-subject** case (39/42) — **built
  now** (`Event::Broadcast { command, source }`).
- The **command-target** case (`cmZoom`/`cmClose`, `infoPtr = owner`) is **not
  carried and not needed**: focused-command routing already delivers each such
  command only to the active window, which is the frame's `owner` — the guard is
  vacuous (cite the invariant from Task 3).
- The **integer-argument** case (`cmSelectWindowNum`) joins `cmTimerExpired` under
  "not carried by Broadcast source — different payload type, own design" (realized
  via a direct walk at 33d).
- The synchronous return-consuming `message()`/`query` primitive remains
  **designed but not built** — its first real consumer is row 34's dialog
  `cmCanCloseForm` veto; build it there. Keep the existing `message()` signature
  sketch as the design of record.

Also revisit the `src/view/context.rs:~240` comment (the "`query` deferred to row
26" note): correct it to say `query`/`message` is a **tree-owner** primitive
(Group/Program over `find_mut`), deferred to **row 34**, *not* a `Context` method
(a `Context` holds no tree).

---

## Verification (must all pass before you hand off)

- `cargo test` — all green (the count rises by your one new test; ~272+).
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean.
- `grep -rn "Event::Broadcast(" src/` returns **nothing** (every tuple-style
  construction/match is converted).
- The new test asserts `source == Some(<scrollbar id>)`, not `None`.

## What you are NOT doing (hard scope fence)

- **No** `message()`/`query` primitive (row 34).
- **No** target/source field on `Event::Command` (proven vacuous, Task 3).
- **No** Alt-N implementation, **no** `View::number()`, **no** `select`/
  `canMoveFocus` (all 33d).
- **No** receiver logic that reads `source` (no widget consumes it yet).
- **No** changes to broadcast *routing* (still fans to all children).
- **No** drag/close/cmNext/cmPrev work (that is 33d, the next brief).
