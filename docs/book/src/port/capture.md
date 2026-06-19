# Event capture

Most of what feels "special" about a Turbo Vision UI — a modal dialog that locks
out the windows behind it, a window you drag by its title bar or resize from a
corner, a scrollbar arrow that keeps firing while you hold the mouse down — is
**one mechanism** in tvision-rs: the **capture stack**.

A capture is a handler that gets first look at every event *before* the normal
view-tree routing. It can read the event, act on it, swallow it, or wave it
through. Stack a few of these and you have modality, mouse tracking, drag/resize,
and press-and-hold — without a single nested event loop. The whole thing lives in
[`src/capture.rs`](../api/tvision_rs/capture/index.html) and is driven by the one
event loop in [`Program`](../api/tvision_rs/app/struct.Program.html).

## The stack

The event loop owns a single
[`CaptureStack`](../api/tvision_rs/capture/struct.CaptureStack.html) — a LIFO stack
of handlers. When an event arrives, the loop offers it to the handlers **top-down**
(most recently pushed first) before any view sees it. Each handler implements
[`CaptureHandler`](../api/tvision_rs/capture/trait.CaptureHandler.html) and answers
with a [`CaptureFlow`](../api/tvision_rs/capture/enum.CaptureFlow.html):

| `CaptureFlow` | Meaning |
| ------------- | ------- |
| `Pass`        | I did not handle it — offer it to the next lower handler, then to normal routing. |
| `Consumed`    | I handled it; stop routing. I stay on the stack. |
| `ConsumedPop` | I handled it **and** I am done — remove me from the stack (e.g. a modal dialog closing, a mouse button released). |

A handler holds a [`ViewId`](../api/tvision_rs/view/struct.ViewId.html) for the view
it speaks for — never a view reference (see [Pointers & infoPtr →
handles](handles.md)). The whole idea fits in one sentence: **a handler that
consumes every otherwise-unhandled event behaves exactly like a blocking loop, so
you never need to write one.**

## Pushing is cooperative

A handler runs while the loop is holding the stack, so it cannot reach in and push
another handler inline. Instead it asks — via
[`Context::push_capture`](../api/tvision_rs/view/struct.Context.html#method.push_capture)
or the higher-level helpers — and the loop performs the push *after* dispatch,
through the [Deferred channel](deferred.md). One pleasant consequence: a
freshly-pushed capture first sees the **next** event, never the one that pushed
it.

## What rides on it

The capture stack is the single backbone under several features that look
unrelated on the surface:

- **Modal dialogs.** A modal dialog pushes a `ModalFrame`. While it is on the
  stack, keyboard, command, and broadcast events pass through (and reach the
  dialog because the group focuses it); mouse events are gated by the dialog's
  bounds — inside, they pass; outside, they are consumed and swallowed, so the
  windows beneath never see the click. That gate is exactly what "modal" means.
  See [Modal `execView`](modal.md).

- **Window drag and resize.** Grabbing a window's title bar or a resize corner in
  the [`Frame`](../api/tvision_rs/frame/struct.Frame.html) pushes a drag handler that
  owns the mouse until you let go, translating each move into a new window
  position or size.

- **Press-and-hold across the widgets.** When you press and hold on a control,
  the widget pushes a mouse-tracking capture that forwards masked move/auto-repeat
  events to that one view and discards everything else until the button comes up.
  Roughly ten widgets rely on this — [`Button`](../api/tvision_rs/widgets/struct.Button.html),
  [`Cluster`](../api/tvision_rs/widgets/struct.Cluster.html),
  [`Editor`](../api/tvision_rs/widgets/struct.Editor.html),
  [`InputLine`](../api/tvision_rs/widgets/struct.InputLine.html),
  [`ScrollBar`](../api/tvision_rs/widgets/struct.ScrollBar.html),
  `ListViewer`, `Outline`, the status line, the menus, and the window
  [`Frame`](../api/tvision_rs/frame/struct.Frame.html) — each for its own gesture
  (autoscroll, rubber-band selection, repeating arrows, drag-select), all over the
  same `MouseTrackCapture`.

Because a capture forwards events by `ViewId` rather than by reference, the loop
performs the actual delivery on the widget's behalf — the cross-view plumbing for
this (and for scrollbar↔scroller chatter) is the [cross-view brokering](../internals/brokering.md)
seam. For the step-by-step mechanics of how each event threads through the stack
and then the tree, see [the event loop in depth](../internals/event-loop.md).
