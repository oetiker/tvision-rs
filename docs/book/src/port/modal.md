# Modal `execView` → one loop

In C++ Turbo Vision, modality is recursion. `TGroup::execView` spins a **nested,
blocking `getEvent` loop** inside the already-running one; the outer loop is
suspended on the call stack while the inner one runs, and the modal view ends it
by calling `endModal`. The same trick drives `dragView` and a pressed button's
press-and-hold tracking.

Rust will not let you do that. A nested loop would have to re-borrow the view
tree that the outer loop already holds `&mut` to — the borrow checker refuses,
and there is no `&mut self`-reentrancy to lean on. So the nested loops collapse
into **one** non-recursive event loop, and modality becomes a handler on the
capture stack rather than a new loop. [Event capture](capture.md) is the general
mechanism; modality is one use of it.

## Modality as a handler

The modal handler is `ModalFrame`. While it sits on the capture stack it lets
keyboard, command, and broadcast events pass through to normal routing — which
reaches the modal view because the group focuses it — while positional (mouse)
events are gated by the modal view's bounds: inside, they pass; outside, they are
consumed and swallowed, so views beneath the dialog never see the click. That
gate is exactly what "modal" means.

## `exec_view` steers the one loop

[`Program::exec_view`](../api/tvision/app/struct.Program.html#method.exec_view) is
the blocking wrapper that replaces `execView`. It inserts the view, makes it
current, pushes a `ModalFrame`, then runs the *same*
[`pump_once`](../api/tvision/app/struct.Program.html#method.pump_once) loop until
the view calls [`end_modal`](../api/tvision/view/struct.Context.html#method.end_modal),
setting the end state. Then it pops the frame, removes the view, restores the
previous focus and command set, and returns the chosen
[`Command`](../api/tvision/command/struct.Command.html). No new loop is spun —
`exec_view` just steers the one loop that was already running.

See [Dialogs & data](../apps/dialogs.md) for the user-facing recipe, [Event
capture](capture.md) for the mechanism modality shares with drag/resize and
press-and-hold, and [the event loop in depth](../internals/event-loop.md) for
what each `pump_once` turn does.
