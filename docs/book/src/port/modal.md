# Modal `execView` → one loop + capture

In C++ Turbo Vision, modality is recursion. `TGroup::execView` spins a **nested,
blocking `getEvent` loop** inside the already-running one; the outer loop is
suspended on the call stack while the inner one runs, and the modal view ends it
by calling `endModal`. The same trick drives `dragView` and a pressed button's
press-and-hold tracking.

Rust will not let you do that. A nested loop would have to re-borrow the view
tree that the outer loop already holds `&mut` to — the borrow checker refuses,
and there is no `&mut self`-reentrancy to lean on. This is deviation **D9**: the
nested loops collapse into **one** non-recursive event loop plus a **LIFO stack
of capture handlers**.

## The capture stack

The single loop owns a [`CaptureStack`](../api/tvision/capture/struct.CaptureStack.html).
Before an event reaches normal view-tree routing, it is offered to the handlers
on the stack, **top-down** (most recently pushed first). Each handler implements
[`CaptureHandler`](../api/tvision/capture/trait.CaptureHandler.html) and returns a
[`CaptureFlow`](../api/tvision/capture/enum.CaptureFlow.html):

| `CaptureFlow` | Meaning |
| ------------- | ------- |
| `Pass`        | Did not handle it — offer to the next lower handler, then to normal routing. |
| `Consumed`    | Handled it; stop routing. The handler stays on the stack. |
| `ConsumedPop` | Handled it **and** removes itself from the stack (e.g. a modal closing). |

Handlers hold a [`ViewId`](../api/tvision/view/struct.ViewId.html) for identity —
never a view reference (see [Pointers → handles](handles.md)). The key insight:
**a handler that consumes every otherwise-unhandled event *is* the modal loop.**
Modality, drag, and press-tracking all become handlers, not nested loops.

## Modality as a handler

The modal handler is `ModalFrame`. While it sits on the stack it lets keyboard,
command, and broadcast events `Pass` through to normal routing — which reaches
the modal view because the group focuses it — while positional (mouse) events
are gated by the modal view's bounds: inside, `Pass`; outside, `Consumed` and
swallowed, so views beneath the dialog never see the click. That gate is exactly
what "modal" means.

[`Program::exec_view`](../api/tvision/app/struct.Program.html#method.exec_view) is
the blocking wrapper that replaces `execView`. It inserts the view, makes it
current, pushes a `ModalFrame`, then runs the *same*
[`pump_once`](../api/tvision/app/struct.Program.html#method.pump_once) loop until
the view calls [`end_modal`](../api/tvision/view/struct.Context.html#method.end_modal),
setting the end state. Then it pops the frame, removes the view, restores the
previous focus and command set, and returns the chosen
[`Command`](../api/tvision/command/struct.Command.html). No new loop is spun —
`exec_view` just steers the one loop that was already running. See [Dialogs &
data](../apps/dialogs.md) for the user-facing recipe and [the event loop in
depth](../internals/event-loop.md) for what each `pump_once` turn does.

## Pushing is deferred

A handler runs while the loop holds the stack, so it cannot push onto the stack
inline without aliasing it. Instead it asks via
[`Context::push_capture`](../api/tvision/view/struct.Context.html#method.push_capture),
and the loop applies that push *after* dispatch (see [the Deferred
channel](deferred.md)). A consequence that happens to match C++: a freshly
pushed capture sees the **next** event, just as a C++ `do { … } while` runs its
body once before the first wait.
