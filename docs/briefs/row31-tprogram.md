# Implementer brief — Row 31 `TProgram` (the live event loop), module `app`

You are porting **one class** of magiblot/tvision to idiomatic Rust in the `tvision`
crate (house alias `tv::`). This is a **FOUNDATION** row: the single event loop
(deviation **D9**) that finally makes the capture stack and timer queue *live*.
Port **faithfully** from the C++; the only intentional departures are the
pre-decided deviations named below. Do **not** invent extra features.

C++ source of truth (read it):
`/home/oetiker/scratch/tvision-spec/magiblot-tvision/source/tvision/tprogram.cpp`
and the modal machinery in `tgroup.cpp` (`execute`/`execView`/`endModal`).

## What you are building

A new module **`src/app/`** (`mod.rs` + `program.rs`) with a `Program` struct
that owns and runs TV's single event loop. Wire it into `src/lib.rs` (add
`pub mod app;` and re-export `Program` at the crate root, alphabetically near the
other re-exports). You are the only agent touching the tree — do all wiring
yourself.

### `Program` embeds a `Group` (D2 embed-and-delegate)

`Program` is **not** a `View` (it is the root; nothing contains it). It embeds a
`Group` as its view container and adds the loop machinery:

```rust
pub struct Program {
    group: Group,                 // the root container (holds desktop/statusline/menubar children)
    renderer: Renderer,           // owns back/front Buffer + Box<dyn Backend>
    captures: CaptureStack,       // row 21 — NOW LIVE
    timers: TimerQueue,           // row 20 — NOW LIVE
    clock: Box<dyn Clock>,        // injected (SystemClock prod / ManualClock test)
    theme: Theme,                 // for the paint pass (DrawCtx needs &Theme)
    out_events: VecDeque<Event>,  // posted commands/broadcasts + queued timer events
    pending_captures: Vec<Box<dyn CaptureHandler>>, // deferred pushes, applied after dispatch
    command_set: CommandSet,      // curCommandSet (enabled commands); see "command-enable" below
    desktop: Option<ViewId>,      // the inserted desktop child (canMoveFocus / Alt-N target)
    end_state: Option<Command>,   // TGroup::endState — Some(cmd) ends the (modal) loop
    command_set_changed: bool,    // TProgram::commandSetChanged
}
```

(Field names are a guide; adjust as the borrow checker and clippy prefer, but keep
`Context`-backing fields — `out_events`, `timers`, `pending_captures` — as
**distinct fields** so disjoint borrows work. See the borrow discipline section.)

### Construction — `Program::new(...)` ports `TProgram::TProgram`

Signature (factory-mixin deferral, PORT-ORDER row 31): take the backend and
**injected factory closures** for the three subviews, plus the clock:

```rust
pub fn new(
    backend: Box<dyn Backend>,
    clock: Box<dyn Clock>,
    theme: Theme,
    create_desktop:   impl FnOnce(Rect) -> Option<Box<dyn View>>,
    create_status_line: impl FnOnce(Rect) -> Option<Box<dyn View>>,
    create_menu_bar:  impl FnOnce(Rect) -> Option<Box<dyn View>>,
) -> Self
```

Faithful ctor behavior (`tprogram.cpp`):
- Bounds = `Rect::new(0, 0, w, h)` from `backend.size()`.
- The group's state: set `active`, `selected`, `focused`, `modal` true (C++
  `state = sfVisible | sfSelected | sfFocused | sfModal | sfExposed`; `sfExposed`
  is dropped under D8, `sfVisible` is the ctor default). Set these on
  `group.state_mut().state` **before** inserting children, then on insert the
  group's own `set_state` propagation is *not* what TV does here — TV just sets
  the bits. Set the bits directly.
- Insert the three subviews **in C++ order: desktop, statusline, menubar**, each
  built from its factory over the **full program extent** (C++ passes
  `getExtent()`; each factory shrinks it — `initDeskTop` does `r.a.y++; r.b.y--`,
  `initStatusLine` does `r.a.y = r.b.y-1`, `initMenuBar` does `r.b.y = r.a.y+1`).
  **For this row the factories own that shrinking** (the real status-line/menu-bar
  are Phase 4); `Program::new` just passes the full extent and inserts whatever
  the factory returns. Remember the desktop's `ViewId` in `self.desktop`.
- A `None` from a factory inserts nothing (statusline/menubar are stubbed `None`
  in row-31 tests; desktop is real — a `Group` containing a `Background`).
- Seed `command_set` with the default-enabled vocabulary (see command-enable).

### Provide a convenience constructor for production

`Program::with_crossterm(theme)` or similar is **out of scope**; keep `new`
backend-injected so `HeadlessBackend` + `ManualClock` drive it in tests. (A
crossterm wrapper can be a one-liner later.)

## The event pump — `fn pump_once(&mut self)`

This is the heart. One iteration of TV's inner loop, restructured for D9. **Read
`src/capture.rs`'s `compose_full_protocol` test — it is the blueprint for the
deferred-capture handshake.** Sequence:

1. **Resize check.** `let (w,h) = self.renderer.backend().size();` compare to the
   group's current size; if changed: `self.renderer.resize(w,h)`,
   `View::change_bounds(&mut self.group, Rect::new(0,0,w,h))`. (No `Event::Resize`
   variant — `CrosstermBackend::size()` queries the terminal live; this avoids
   enum churn. Leave a one-line comment noting this is the D9 realization of
   `setScreenMode`/`cmScreenChanged`.)
2. **Sample the clock once:** `let now = self.clock.now_ms();`
3. **Pick the next event.** Prefer the internal queue, else poll the backend:
   ```text
   let ev = match out_events.pop_front() {
       Some(e) => Some(e),
       None => renderer.backend_mut().poll_event(Some(timeout)),  // timeout = event_wait_timeout(now)
   };
   ```
   Drain the whole `out_events` queue before polling (process queued
   commands/broadcasts/timer events first). One event per `pump_once` call.
4. **No event (`None`) → idle** (ports `TProgram::idle`):
   - `for id in timers.collect_expired(now)` push `Event::Broadcast(Command::TIMER_EXPIRED)`
     onto `out_events`. (C++ `message(this, evBroadcast, cmTimerExpired, id)`. The
     **`TimerId` payload is dropped** — D4 broadcasts carry only the `Command`.
     Leave a breadcrumb: when a widget needs to know *which* timer fired, revisit
     the payload story (it has multiple potential designs; do not invent one now).)
   - if `command_set_changed`: push `Event::Broadcast(Command::COMMAND_SET_CHANGED)`,
     set `command_set_changed = false`.
   - `statusLine->update()` is a **stub no-op** (statusline is Phase 4) — leave a
     `// TODO(TStatusLine row)` breadcrumb.
   - Then return (the redraw at the end of `pump_once` still runs — see step 7).
     Actually: do idle, then **fall through to redraw**; do not early-return before
     the redraw.
5. **Event present → dispatch.** Build a `Context` over the disjoint fields and:
   - Offer to the capture stack first: `captures.dispatch(&mut ev, &mut ctx)`. If
     it returns `true` (consumed), skip view routing.
   - Else run **program-level handle_event** (a free fn, see below).
   - Drop the `Context`. Then **apply deferred captures**:
     `for h in pending_captures.drain(..) { captures.push(h); }`
6. **`getEvent` status-line pre-handling** (`tprogram.cpp::getEvent`): keydown, or
   mousedown whose `firstThat(viewHasMouse)` is the statusline, is handed to the
   statusline *before* normal routing. With statusline stubbed this is a **no-op**;
   leave a `// TODO(TStatusLine row)` breadcrumb describing it so it is not lost.
   Likewise `cmScreenChanged` handling is realized by the resize check in step 1.
7. **Redraw** (D8 whole-tree + diff, every pass — the diff keeps I/O bounded):
   ```text
   renderer.render(|buf| {
       let bounds = group.state().get_bounds();
       let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
       group.draw(&mut dc);
   });
   ```
   Then **resetCursor** (see below): compute the absolute focused cursor and call
   `renderer.set_cursor(...)`. Note `Renderer::render` reads `self.cursor`, so call
   `renderer.set_cursor(...)` **before** `renderer.render(...)`, or restructure so
   the cursor set precedes the render in the same pass. Verify against
   `renderer.rs` (render uses `self.cursor`; set it first).

### `event_wait_timeout(now) -> Option<Duration>`

Ports `TProgram::eventWaitTimeout`: `min(eventTimeoutMs=20ms, timers.time_until_next(now))`.
If no timer, just the 20 ms frame tick. Return it for `poll_event`. (Headless
ignores the timeout and never blocks — that is the D11 determinism contract, so
tests stay synchronous.)

### program-level handle_event (free fn, NOT `&mut self`)

Ports `TProgram::handleEvent`. Take explicit field borrows so it composes with the
pump's borrows:

```rust
fn program_handle_event(
    group: &mut Group,
    ev: &mut Event,
    ctx: &mut Context,
    end_state: &mut Option<Command>,
)
```

Body:
- **Alt-1..9 window selection** (C++: `getAltChar` → broadcast `cmSelectWindowNum`
  to deskTop). **Stub this for row 31** with a breadcrumb: no numbered windows
  exist until TWindow (row 33), and the window *number* travelled via `infoPtr`
  which D4 dropped, so the payload mechanism is unresolved. Do **not** build a
  half path — just `// TODO(row 33+): Alt-1..9 window select; needs numbered
  windows + a payload story (D4 dropped infoPtr).`
- `group.handle_event(ev, ctx)` — delegate to the embedded group's three-phase
  router.
- After: if `*ev == Event::Command(Command::QUIT)` → `*end_state = Some(Command::QUIT)`
  and `ev.clear()`. (C++ `endModal(cmQuit); clearEvent`.)

### `end_modal` / `run` (ports `TGroup::endModal` + `execute`)

- `pub fn end_modal(&mut self, cmd: Command)` → `self.end_state = Some(cmd)`.
- `pub fn run(&mut self) -> Command` ports `TGroup::execute`:
  ```text
  loop {
      self.end_state = None;
      while self.end_state.is_none() {
          self.pump_once();
      }
      let es = self.end_state.unwrap();
      if self.valid_end(es) { return es; }   // TGroup::execute's outer while(!valid(endState))
  }
  ```
  `valid_end(cmd)` delegates to `self.group.valid(cmd)` (faithful: a modal only
  ends if the tree validates the end command). For row 31 the group's `valid` is
  already implemented; this just wires it.

`run()` is the production entry (SystemClock + crossterm, which blocks in
`poll_event`). Tests drive `pump_once` directly with `ManualClock` + headless so
they never spin.

### The modal capture frame (D9 modality — the mechanism, live)

This is the row's modality deliverable. **Do NOT build a nested loop and do NOT
build `exec_view`/`executeDialog`/`getData`/`setData` — those defer to row 34**
(TDialog is their concrete consumer; the sync-vs-event-driven return is decided
there). What you build now: a `CaptureHandler` that realizes modality, proving the
capture stack can gate events to a modal view.

Provide a `ModalFrame` capture handler (in `program.rs` or a small submodule):
- Holds the modal view's `ViewId` and a notion of "swallow otherwise-unhandled
  events" so a modal dialog blocks interaction with views beneath it.
- Its `handle` returns `CaptureFlow::Pass` for events destined to the modal view's
  subtree (let normal routing reach it) and `CaptureFlow::Consumed` for events that
  would otherwise escape to non-modal views (the D9 "a handler that consumes every
  otherwise-unhandled event *is* the modal loop").
- Pops itself (`CaptureFlow::ConsumedPop`) — or is popped by the loop — when the
  modal ends.

Keep this **minimal and faithful to the capture.rs contract**; the goal is to
prove the mechanism with a synthetic modal view in tests (see test 5), not to ship
the dialog API. If a clean `pub fn begin_modal(&mut self, id: ViewId)` that pushes
the frame falls out, add it; otherwise a tested internal mechanism is enough. Add
a module-doc breadcrumb that `exec_view`/`executeDialog` (the blocking wrapper +
data marshalling) land at row 34 on top of this frame.

### resetCursor — a new `View` trait method + `Group` override

Ports `TView::resetCursor` / the focused-chain cursor walk. Add to the `View`
trait (`src/view/view.rs`) a **defaulted** method:

```rust
/// `TView::resetCursor` support — the view-local hardware-cursor position this
/// view wants shown, or `None` to hide. Base: `Some(cursor)` iff the view is
/// focused with a visible cursor (`sfFocused && sfCursorVis`), else `None`.
/// `Group` overrides to descend into its `current` child, accumulating origin.
fn cursor_request(&self) -> Option<Point> {
    let s = self.state();
    if s.state.focused && s.state.cursor_vis { Some(s.cursor) } else { None }
}
```

`Group` overrides: if `current` resolves to a child, return
`child.cursor_request().map(|p| p + child.origin)`, else `None`. (This is the
top-down realization of the focused-chain walk; each level adds its child's
origin.) `Program`'s resetCursor: `self.group.cursor_request().map(|p| p +
group.origin)` → convert to `(u16,u16)` (clamp/guard negatives) →
`renderer.set_cursor(Some(...))`, or `None` → `renderer.set_cursor(None)`.

Add a focused-cursor unit test in `group.rs` (a synthetic focused child with
`cursor_vis` + a set cursor → group returns the origin-shifted position).

### command-enable policy (curCommandSet)

`Program` owns `command_set: CommandSet` (the `curCommandSet`). Seed it in `new`
with a reasonable default-enabled set (mirror C++ `TView::TView`'s initial
`curCommandSet` which enables everything except a small disabled set —
`cmZoom`/`cmClose`/`cmResize`/`cmNext`/`cmPrev` start **disabled**; see
`tview.cpp` static init). The **">255 always enabled" rule is DROPPED** (D1).
Expose `enable_command`/`disable_command`/`command_enabled` methods that set
`command_set_changed = true` on change (so idle broadcasts
`cmCommandSetChanged`). **Routing-time command filtering** (a disabled `cmXxx`
command event is dropped before view routing) — wire the check in `pump_once`
before `program_handle_event` for `Event::Command`: if the command is not enabled,
drop it (`ev.clear()` / skip). Keep this faithful but minimal; if the exact C++
filtering point is ambiguous, prefer "filter `Event::Command` at the program
boundary" and leave a breadcrumb.

If seeding the exact disabled-set proves fiddly, a defensible minimal seed +
breadcrumb is acceptable — but DO implement the enable/disable/changed plumbing and
the `cmCommandSetChanged` idle broadcast, because the scrollbar/widgets already
broadcast `COMMAND_SET_CHANGED` and tests will check it.

## Borrow discipline (your #1 risk — heed this)

`Context` was deliberately built over **distinct `&mut` fields** so the pump can
take disjoint borrows. In `pump_once`, **destructure `self` into field bindings at
the top**:

```rust
let Program { group, renderer, captures, timers, clock, theme,
              out_events, pending_captures, command_set, end_state,
              command_set_changed, desktop } = self;
```

Then build `Context::new(out_events, timers, now, pending_captures)` and pass
`&mut *captures` / `&mut *group` alongside it — these are disjoint fields, so it
compiles. **Do NOT** decompose the pump into `&mut self` helper methods
(`self.idle()`, `self.dispatch()`, `self.redraw()`) that each need overlapping
field sets — that thrashes the borrow checker. Free functions taking explicit
field borrows (like `program_handle_event` above) are the pattern. `clock.now_ms()`
needs only `&` so it's fine to call through the binding.

## Tests (this is the verification — D11)

Add unit/integration tests in `program.rs` (and the `group.rs` cursor test). Use
`HeadlessBackend` + `ManualClock`. A small synthetic `Probe` view (copy the shape
from `group.rs` tests — fills its extent, records/acts on events) is your tool.
Cover at least:

1. **End-to-end loop snapshot.** Build `Program` with a desktop factory returning
   a `Group` containing a `Background` (the minimal real desktop); `pump_once`;
   `assert_snapshot!` the headless screen. (This is the mandatory snapshot gate.)
2. **Quit.** A probe posts `Command::QUIT` (or inject a key the probe turns into
   it); after the command re-enters as an event and routes, `end_state ==
   Some(QUIT)` and `run()` would exit. Test via stepping `pump_once` until
   `end_state` is set (don't call the blocking `run()` with a spinning ManualClock
   unless you bound it).
3. **Timer dispatch.** Arm a timer (via a probe's `ctx.set_timer`); advance
   `ManualClock` past expiry; `pump_once` with no input → idle collects it → a
   `Broadcast(TIMER_EXPIRED)` is queued → next `pump_once` routes it → a probe
   records receiving `cmTimerExpired`.
4. **Capture stack live.** A probe pushes a capture handler via
   `ctx.push_capture` during one pump; assert it is applied **after** that dispatch
   (not seen by the current event) and sees the **next** event first — exactly the
   `compose_full_protocol` invariant, now through the real pump.
5. **Modal frame.** Insert a modal `Probe` + push the `ModalFrame`; feed an event
   aimed outside the modal view → it is swallowed (a non-modal probe beneath does
   NOT see it); feed one aimed at the modal view → it reaches it; `end_modal`
   surfaces `end_state` and the frame pops.
6. **resetCursor.** A focused probe with `cursor_vis` and a set cursor → after a
   pump, `HeadlessHandle`'s cursor is the absolute (origin-shifted) position;
   without `cursor_vis` → cursor hidden (`None`).
7. **Posted command re-entry.** A probe `ctx.post`s a command during dispatch;
   assert it lands in `out_events` and is routed back as an `Event::Command` on the
   next pump.
8. **commandSetChanged idle broadcast.** `disable_command`/`enable_command` sets
   the flag; an idle pump broadcasts `Command::COMMAND_SET_CHANGED` once, then
   clears the flag.

Each snapshot uses the frozen format (`src/screen/snapshot.rs` via
`HeadlessHandle::snapshot()` + `insta::assert_snapshot!`), the same as existing
widget tests.

## Definition of done (run these; all must pass)

- `cargo test` — all green (existing 229 unit + 3 integration + your new tests).
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean.
- New snapshots reviewed and accepted (`cargo insta accept` or commit the `.snap`
  files under the module's `snapshots/` dir; inspect them — they must show a
  sensible desktop fill).
- English-only comments/identifiers; module docs explain the D9 realization, every
  deferral (exec_view→34, Alt-N→33+, timer payload, statusline pre-handling,
  status-line/menu-bar factories) with a grep-able breadcrumb.

## Deviations in play (apply mechanically; do not re-decide)
- **D9** single loop + capture stack (no nested modal loop); timer queue live.
- **D4** events carry no payload (`infoPtr` dropped) — timer id + window number
  payloads are gone; broadcast carries only the `Command`.
- **D8** whole-tree redraw + diff (no damage tracking); `sfExposed`/buffer dropped.
- **D3** no up-pointers — a view reaches the loop only through `Context`.
- **D2/D5** embed `Group`; struct-of-bools state (already built).
- **D11** injected `Clock` + `Backend`; headless never blocks.
- **D1** commands are string-newtypes; ">255 always enabled" rule dropped.

Do not reference the Go port. Match the existing house style (see `group.rs`,
`frame.rs`, `context.rs` for the idiom: module doc with the C++ mapping +
deviations, doc-commented methods citing the C++ symbol, tests at the bottom).
