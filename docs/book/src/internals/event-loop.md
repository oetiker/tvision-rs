# The event loop in depth

Turbo Vision in C++ has *many* loops. `execView` spins a nested blocking
`getEvent` loop for every modal dialog; `dragView` spins another while you drag a
window; a pressed button spins its own while you hold the mouse. Each one
re-enters the framework and re-borrows the view tree.

Rust will not let you nest a blocking loop that re-borrows the same `&mut` tree,
so rstv collapses all of those into **one** non-recursive loop in
[`Program`](../api/tvision/app/struct.Program.html). What used to be a nested
loop becomes a *capture handler* on a LIFO stack (see
[Cross-view brokering & ViewId](./brokering.md) and the
[capture section](#the-capture-stack) below). Everything routes through a single
pass: `pump_once`.

## `run` is the only outer loop

[`Program::run`](../api/tvision/app/struct.Program.html#method.run) is the whole
application loop. It mirrors C++ `TGroup::execute`'s
`while (!valid(endState))` — pump until something sets an end command, then ask
the *tree* to validate that command; if it validates, return, otherwise clear it
and keep pumping:

```rust,ignore
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

[`run_app`](../api/tvision/app/struct.Program.html#method.run_app) is the same
loop with one addition: any [`Command`](../api/tvision/command/struct.Command.html)
that survives all view routing is handed to your callback — the rstv stand-in for
`TApplication::handleEvent`. That is where menu commands like "open the color
picker" get serviced. You almost always call one of these two and never touch the
machinery below.

## One pass: `pump_once`

`pump_once` is the heart of the single loop. Each call does exactly one trip through these
phases, in order:

| Phase | What happens |
| ----- | ------------ |
| **Resize** | Query the terminal size; if it changed, relayout the whole tree. There is no `Event::Resize` — the backend is polled live. |
| **Settle currency** | Apply any pending insert-time focus cascades so the event about to be dispatched sees C++-equivalent currency. |
| **Pick an event** | Drain the internal queue first, else poll the backend with the frame-tick timeout; an idle pick may synthesize a mouse auto-repeat. |
| **Idle** | No event: fire expired timers as [`Event::Timer`](../api/tvision/event/enum.Event.html), refresh the status line's help context. |
| **Pre-route** | A `KeyDown` (always) or a `MouseDown` on the status line is offered to the status line first, so accelerators like F10/Alt-X fire even under a modal. |
| **The dispatch gate** | Drop the event if it is a disabled command; otherwise offer it to the capture stack, then to normal view routing. |
| **Deferred drain** | Apply every queued effect once, in insertion order. |
| **Cursor + redraw** | Set the hardware cursor, then redraw the whole tree and diff it to the screen. |

### The dispatch gate

Before an event reaches a view it passes a small gate. A command that is
currently **disabled** is dropped here — rstv uses a denylist, so unknown custom
commands flow through untouched (see [Commands & events](../apps/commands.md)).
What survives is offered to the [capture stack](#the-capture-stack) first; only if
no handler consumes it does it go to the normal view-tree walk
(`program_handle_event`, the successor to `TProgram::handleEvent`). A modal
handler that consumes every otherwise-unhandled event *is* the modal loop.

### The deferred drain

A view is borrowed *downward* during dispatch as `&mut dyn View` plus a
[`Context`](../api/tvision/view/struct.Context.html); it cannot reach back up to
the loop-owned capture stack, command set, or sibling views. So instead of acting
inline it **queues** the effect, and the pump applies the whole queue in one pass
*after* dispatch — capture pushes, command enable/disable, bounds changes, modal
close, focus moves, and the cross-view broker syncs. This is the
[`Deferred`](../api/tvision/view/enum.Deferred.html) channel; it has its own page,
[Deferred effects](./deferred.md). Two rules matter here: the drain runs even when
the pre-route consumed the event, and it runs **once** — anything an effect
re-queues waits for the next pump (a loop-until-empty would risk spinning).

Because capture pushes are deferred, a freshly pushed handler sees the *next*
event, not the one that pushed it — exactly matching a C++ `do { } while` that
runs its body once before its first wait.

## The capture stack

The [`CaptureStack`](../api/tvision/capture/struct.CaptureStack.html) is the LIFO
list of [`CaptureHandler`](../api/tvision/capture/trait.CaptureHandler.html)s that
replaces all those nested C++ loops. Each handler is offered every event before
normal routing and returns a
[`CaptureFlow`](../api/tvision/capture/enum.CaptureFlow.html):

- `Pass` — not mine; offer it to the next lower handler, then to the view tree.
- `Consumed` — handled; stop routing, stay on the stack.
- `ConsumedPop` — handled, and remove *myself* (e.g. a modal closing).

The return value is authoritative — handlers do **not** signal "consumed" by
clearing the event. A handler holds a [`ViewId`](./brokering.md), never a view
reference. Concrete handlers include a bounds-gating *modal frame*, window
dragging and keyboard resize, mouse hold-tracking, and the menu session — each
the single-loop form of a C++ loop body that used to block. Before every dispatch the pump
re-syncs each bounds-gating handler from the live tree (`sync_gate_bounds`), so a
dialog you have just dragged stays clickable in its new position.

## Where to go next

- [Deferred effects](./deferred.md) — the full effect catalogue and why each one
  is queued rather than applied inline.
- [Cross-view brokering & ViewId](./brokering.md) — how the pump brokers reads and
  writes between sibling views during the drain.
- [Modal execView → one loop + capture](../port/modal.md) — the veteran's view of
  how `execView` became a capture handler.
