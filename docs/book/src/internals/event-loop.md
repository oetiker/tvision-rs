# The event loop in depth

tvision-rs runs the entire application on **one** non-recursive event loop in
[`Program`](../api/tvision_rs/app/struct.Program.html). Every event — keystrokes,
mouse motion, modal dialogs, window drags, mouse hold-tracking — routes through a
single pass called `pump_once`. Modality and press-and-hold are not separate
blocking loops; they are *capture handlers* stacked on a LIFO capture stack (see
[Cross-view brokering & ViewId](./brokering.md) and the
[capture section](#the-capture-stack) below). Because the whole tree is owned
behind a single `&mut`, exactly one thing at a time may borrow it: the single
loop enforces this structurally.

> **Turbo Vision heritage:** the C++ library had *many* loops. `execView` spun a
> nested blocking `getEvent` loop for every modal dialog; `dragView` spun another
> while you dragged a window; a pressed button spun its own while you held the
> mouse. Each re-entered the framework and re-borrowed the view tree — which Rust
> forbids. Every one of those nested loop bodies is now a capture handler.

## `run` is the only outer loop

[`Program::run`](../api/tvision_rs/app/struct.Program.html#method.run) is the whole
application loop. It pumps until something sets an end command, then asks the
*tree* to validate that command; if it validates, return, otherwise clear it and
keep pumping:

```rust,ignore
// Illustrative sketch — not a standalone program.
loop {
    self.end_state = None;
    while self.end_state.is_none() {
        self.pump_and_drive();        // one event, fully processed
    }
    let es = self.end_state.unwrap();
    if self.valid_end(es) {           // tree-wide valid() walk
        return es;
    }
}
```

> **Turbo Vision heritage:** this mirrors `TGroup::execute`'s
> `while (!valid(endState))` pattern.

[`run_app`](../api/tvision_rs/app/struct.Program.html#method.run_app) is the same
loop with one addition: any [`Command`](../api/tvision_rs/command/struct.Command.html)
that survives all view routing is handed to your callback. That is where menu
commands like "open the color picker" get serviced. You almost always call one of
these two and never touch the machinery below.

## One pass: `pump_once`

`pump_once` is the heart of the single loop. Each call does exactly one trip through these
phases, in order:

| Phase | What happens |
| ----- | ------------ |
| **Resize** | Query the terminal size; if it changed, relayout the whole tree. There is no `Event::Resize` — the backend is polled live. |
| **Settle currency** | Apply any pending insert-time focus cascades so the event about to be dispatched sees a fully settled focus state. |
| **Pick an event** | Drain the internal queue first, else poll the backend with the frame-tick timeout; an idle pick may synthesize a mouse auto-repeat. |
| **Idle** | No event: fire expired timers as [`Event::Timer`](../api/tvision_rs/event/enum.Event.html), refresh the status line's help context. |
| **Pre-route** | A `KeyDown` (always) or a `MouseDown` on the status line is offered to the status line first, so accelerators like F10/Alt-X fire even under a modal. |
| **The dispatch gate** | Drop the event if it is a disabled command; otherwise offer it to the capture stack, then to normal view routing. |
| **Deferred drain** | Apply every queued effect once, in insertion order. |
| **Cursor + redraw** | Set the hardware cursor, then redraw the whole tree and diff it to the screen. |

### The dispatch gate

Before an event reaches a view it passes a small gate. A command that is
currently **disabled** is dropped here — tvision-rs uses a denylist, so unknown custom
commands flow through untouched (see [Commands & events](../apps/commands.md)).
What survives is offered to the [capture stack](#the-capture-stack) first; only if
no handler consumes it does it go to the normal view-tree walk
(`program_handle_event`). A modal handler that consumes every otherwise-unhandled
event *is* the modal loop.

### The deferred drain

A view is borrowed *downward* during dispatch as `&mut dyn View` plus a
[`Context`](../api/tvision_rs/view/struct.Context.html); it cannot reach back up to
the loop-owned capture stack, command set, or sibling views. So instead of acting
inline it **queues** the effect, and the pump applies the whole queue in one pass
*after* dispatch — capture pushes, command enable/disable, bounds changes, modal
close, focus moves, and the cross-view broker syncs. This is the
[`Deferred`](../api/tvision_rs/view/enum.Deferred.html) channel; it has its own page,
[Deferred effects](./deferred.md). Two rules matter here: the drain runs even when
the pre-route consumed the event, and it runs **once** — anything an effect
re-queues waits for the next pump (a loop-until-empty would risk spinning).

Because capture pushes are deferred, a freshly pushed handler sees the *next*
event, not the one that pushed it — the push and the first handled event are
always separated by at least one pump boundary.

## The capture stack

The [`CaptureStack`](../api/tvision_rs/capture/struct.CaptureStack.html) is the LIFO
list of [`CaptureHandler`](../api/tvision_rs/capture/trait.CaptureHandler.html)s that
implements modality, dragging, press-and-hold, and menu sessions — anything that
needs to intercept events globally before normal routing. Each handler is offered
every event and returns a
[`CaptureFlow`](../api/tvision_rs/capture/enum.CaptureFlow.html):

- `Pass` — not mine; offer it to the next lower handler, then to the view tree.
- `Consumed` — handled; stop routing, stay on the stack.
- `ConsumedPop` — handled, and remove *myself* (e.g. a modal closing).

The return value is authoritative — handlers do **not** signal "consumed" by
clearing the event. A handler holds a [`ViewId`](./brokering.md), never a view
reference. Concrete handlers include a bounds-gating *modal frame*, window
dragging and keyboard resize, mouse hold-tracking, and the menu session. Before
every dispatch the pump re-syncs each bounds-gating handler from the live tree
(`sync_gate_bounds`), so a dialog you have just dragged stays clickable in its new
position.

## The Phase field

A focused-event dispatch (a `KeyDown` or `Command`) visits three legs of the
view tree in order: the **pre-process** children, the **focused** child, then
the **post-process** children. A view that participates in more than one leg —
or that simply needs to know which leg it is on — reads `ctx.phase()`.

The three values of [`Phase`](../api/tvision_rs/view/enum.Phase.html):

| Phase | Which views | Typical use |
| ----- | ----------- | ----------- |
| `PreProcess` | Children with `options.pre_process = true` | Alt-letter accelerators (menu bar), global hot-keys |
| `Focused` | The current child only (no option gate) | Ordinary key handling, text input |
| `PostProcess` | Children with `options.post_process = true` | Plain-letter hot-keys on buttons, check-boxes, labels |

The group sets the phase on the shared [`Context`](../api/tvision_rs/view/struct.Context.html)
before each leg and restores the previous value after it, so a nested group
that re-enters the three-phase router always sees its *own* legs, not the
outer group's:

```rust,ignore
// Sketch of what Group::handle_event does for focused events
// (src/view/group.rs — the three-phase router body).
ctx.set_phase(Phase::PreProcess);
for child in pre_process_children { child.handle_event(ev, ctx); }

ctx.set_phase(Phase::Focused);
current_child.handle_event(ev, ctx);

ctx.set_phase(Phase::PostProcess);
for child in post_process_children { child.handle_event(ev, ctx); }

ctx.set_phase(saved);   // restore for the outer group
```

A leaf handler reads the phase to decide whether to act:

```rust
# use tvision_rs as tv;
# use tv::event::Event;
# use tv::view::{Phase, View, ViewState, Context, DrawCtx};
# struct MyButton { state: ViewState }
# impl View for MyButton {
#     fn state(&self) -> &ViewState { &self.state }
#     fn state_mut(&mut self) -> &mut ViewState { &mut self.state }
#     fn draw(&mut self, _ctx: &mut DrawCtx) {}
fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
    if let Event::KeyDown(k) = ev {
        match ctx.phase() {
            Phase::PreProcess => {
                // Alt-letter accelerator: fires before the focused view sees it.
                if tv::is_alt_hotkey(k, 'O') { ev.clear(); /* open */ }
            }
            Phase::Focused => {
                // Focused key: Enter / Escape handled here.
                if k.key == tv::Key::Enter { ev.clear(); }
            }
            Phase::PostProcess => {
                // Plain-letter hot-key: fires after the focused view.
                if tv::is_plain_hotkey(k, 'O') { ev.clear(); }
            }
        }
    }
}
# }
```

**Sources:** [`Phase`](../api/tvision_rs/view/enum.Phase.html) in `src/view/view.rs`;
[`Context::phase`](../api/tvision_rs/view/struct.Context.html#method.phase) /
`set_phase` in `src/view/context.rs`; the router body in `src/view/group.rs`.

> **Turbo Vision heritage:** ports `phaseType` (`views.h`). In the C++ code the
> phase was an owner field (`owner->phase`); because tvision-rs views have no
> up-pointer the phase rides the `Context` instead (deviation D4).

## Cursor shape: insert vs overwrite

When a view wants to show a hardware cursor it sets
[`State::cursor_vis`](../api/tvision_rs/view/struct.State.html) (via
[`ViewState::show_cursor`](../api/tvision_rs/view/struct.ViewState.html#method.show_cursor))
and also sets the cursor *shape*: underline for the normal insertion point
(`normal_cursor`, the default) or block for overwrite mode (`block_cursor`),
controlled by [`State::cursor_ins`](../api/tvision_rs/view/struct.State.html).

```rust
# use tvision_rs as tv;
# use tv::view::{View, ViewState, Context, DrawCtx};
# use tv::event::Event;
# struct MyEditor { state: ViewState, insert_mode: bool }
# impl View for MyEditor {
#     fn state(&self) -> &ViewState { &self.state }
#     fn state_mut(&mut self) -> &mut ViewState { &mut self.state }
#     fn draw(&mut self, _ctx: &mut DrawCtx) {}
fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
    if let Event::KeyDown(k) = ev {
        if k.key == tv::Key::Insert {
            self.insert_mode = !self.insert_mode;
            if self.insert_mode {
                self.state.normal_cursor();   // underline = insert point
            } else {
                self.state.block_cursor();    // block = overwrite
            }
            ev.clear();
        }
    }
}
# }
```

After each `pump_once` the event loop walks the current-child chain from the
root group downward via `Group::cursor_request` to find the deepest focused
view that has `cursor_vis = true`. The result — or `None` if no view wants a
cursor — is placed on the renderer before the redraw step:

```rust,ignore
// src/app/program.rs — end of pump_once
let cursor = group.cursor_request()   // descends through current children
    .map(|p| p + group_origin)
    .map(|p| (p.x.max(0) as u16, p.y.max(0) as u16));
renderer.set_cursor(cursor);
renderer.render(|buf| { /* whole-tree redraw */ });
```

A view that wants no cursor simply leaves `cursor_vis = false`; it never calls
`show_cursor`. A view that loses focus keeps whatever `cursor_vis` value it
had, but the walk only descends through *focused* children, so the cursor is
hidden automatically until the view is focused again.

**Sources:** `State::cursor_ins` / `block_cursor` / `normal_cursor` in
`src/view/view.rs`; `Group::cursor_request` in `src/view/group.rs`;
`resetCursor` walk in `src/app/program.rs`.

> **Turbo Vision heritage:** ports `sfCursorVis` and `sfCursorIns` from
> `views.h`. The `TView::resetCursor` tree-walk is realized by
> `Group::cursor_request`.

## Marking an event handled

A handler signals "I consumed this event — stop routing" by calling
[`ev.clear()`](../api/tvision_rs/event/enum.Event.html#method.clear) on the
`&mut Event` it received. `clear()` sets the event to
[`Event::Nothing`](../api/tvision_rs/event/enum.Event.html), the consumed-event
sentinel. Every subsequent routing step first tests `ev.is_nothing()` and skips
the delivery if true, so no further handler sees it.

```rust
# use tvision_rs as tv;
# use tv::event::Event;
# use tv::view::{View, ViewState, Context, DrawCtx};
# struct MyView { state: ViewState }
# impl View for MyView {
#     fn state(&self) -> &ViewState { &self.state }
#     fn state_mut(&mut self) -> &mut ViewState { &mut self.state }
#     fn draw(&mut self, _ctx: &mut DrawCtx) {}
fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
    match ev {
        Event::Command(cmd) if *cmd == tv::command::Command::OK => {
            // Handle it, then consume so no other handler sees it.
            ev.clear();
        }
        _ => {}   // unhandled — leave ev alone; it will propagate
    }
}
# }
```

"Who handled it" is recorded in-place on the event, not through a shared
flag. There is no out-of-band consumed signal. After routing returns to the
caller, a simple `ev.is_nothing()` test tells the caller whether anything
consumed the event.

**Sources:** `Event::clear` / `Event::is_nothing` / `Event::Nothing` in
`src/event/mod.rs`.

> **Turbo Vision heritage:** ports `clearEvent` (`views.h`). The C++ convention
> was to assign `what = evNothing`; `ev.clear()` is the idiomatic Rust spelling
> of the same thing.

## Background work each idle pass

An **idle pass** is a `pump_once` call where the backend returned no event and
no mouse auto-repeat was due — the `None =>` arm in the event-pick step. Idle
passes occur at the frame-tick cadence (roughly 20 ms) whenever the user is
not typing or moving the mouse.

On an idle pass the pump does three things before falling through to the normal
redraw:

1. **Fires the command-set-changed broadcast.** If the enabled/disabled command
   set changed since the last broadcast, a `Command::COMMAND_SET_CHANGED`
   broadcast is queued so buttons and menu items can re-gray themselves.
2. **Drains expired timers.** Each timer whose deadline has passed is collected
   and queued as a typed
   [`Event::Timer(id)`](../api/tvision_rs/event/enum.Event.html) — a view matches
   the id against its own timer handles to know which timer fired.
3. **Refreshes the status line.** The help context of the topmost modal view
   (or `NO_CONTEXT` if none) is handed to the status line so it can display
   context-sensitive help text.

After `pump_once` returns, the outer `pump_and_drive` fires the optional
**idle hook** registered with
[`Program::set_on_idle`](../api/tvision_rs/app/struct.Program.html#method.set_on_idle):

```rust
# use tvision_rs as tv;
# fn _demo(program: &mut tv::app::Program) {
program.set_on_idle(|p| {
    // Called on every idle pass — roughly every 20 ms with no user input.
    // `p` is &mut Program, so you can post events, access the view tree, etc.
    // Keep this callback cheap: it runs on the loop's frame cadence.
});
# }
```

Only one idle hook is held at a time; a second `set_on_idle` call replaces the
first. The hook fires during any loop level, including while a modal dialog is
open, because `pump_and_drive` drives both the outer and inner (exec_view)
loops.

For **exact timing** — actions that must fire at a specific wall-clock instant —
register a timer with
[`Context::set_timer`](../api/tvision_rs/view/struct.Context.html#method.set_timer)
instead. The timer expires at the exact deadline and arrives as `Event::Timer`
on an idle pass.

**Sources:** the `None =>` arm of `pump_once` and `Program::set_on_idle` in
`src/app/program.rs`.

> **Turbo Vision heritage:** idle-pass logic ports `TProgram::idle`
> (`tprogram.cpp`). The idiomatic successor to overriding `TProgram::idle` is
> `Program::set_on_idle`.

## Where events come from

Every event in tvision-rs enters the loop through a **single acquisition path**
inside `pump_once`:

1. **Internal queue first.** `out_events` (a `VecDeque`) is drained before
   polling the backend. Timer expirations, command re-posts, and events injected
   by capture handlers or deferred effects all land here and are processed in
   first-in-first-out order on the next pump passes.
2. **Backend poll if the queue is empty.** `renderer.backend_mut().poll_event(timeout)`
   blocks for up to `timeout` (the frame-tick, typically 20 ms) and returns the
   next terminal event, or `None` on timeout.
3. **Mouse-auto synthesizer.** If both the queue and the backend returned `None`,
   and a mouse button is held, `MouseAutoState::synthesize` may produce a
   synthetic `Event::MouseAuto` at the configured repeat cadence.

```rust,ignore
// src/app/program.rs — the event-pick step (pump_once, step 3)
let ev = match out_events.pop_front() {
    Some(e) => Some(e),
    None    => renderer.backend_mut().poll_event(timeout),
};
let ev = match ev {
    Some(e) => { mouse_auto.observe(&e, now); Some(e) }
    None    => mouse_auto.synthesize(now),
};
```

There is no app-level override of this path. Instead:

- **Periodic work** → `set_on_idle` or a timer (`Context::set_timer`).
- **Inject an event programmatically** → push to the internal queue
  (`out_events.push_back(ev)`) from a deferred effect or from top-level code.
- **Deterministic test events** → push events to the
  [`HeadlessBackend`](../api/tvision_rs/backend/struct.HeadlessBackend.html)'s
  queue; `poll_event` pops them without blocking.

**Sources:** `pump_once` (the event-pick and mouse-auto steps) in
`src/app/program.rs`.

> **Turbo Vision heritage:** ports the role of `TProgram::getEvent`
> (`tprogram.cpp`). There is no virtual `getEvent` override point; the
> idiomatic substitutes are the timer queue, `set_on_idle`, and the headless
> event queue for tests.

## Where to go next

- [Deferred effects](./deferred.md) — the full effect catalogue and why each one
  is queued rather than applied inline.
- [Cross-view brokering & ViewId](./brokering.md) — how the pump brokers reads and
  writes between sibling views during the drain.
- [Modal execView → one loop + capture](../port/modal.md) — the veteran's view of
  how `execView` became a capture handler.
