# Event capture

Several Turbo Vision behaviours share **one mechanism** in tvision-rs, the
**capture stack**: a modal dialog that locks out the windows behind it, a window
dragged by its title bar or resized from a corner, and a scrollbar arrow that
keeps firing while the mouse button is held down.

A capture is a handler that sees every event *before* the normal view-tree
routing. It can read the event, act on it, swallow it, or let it through. A few
of these on a stack give modality, mouse tracking, drag/resize and
press-and-hold without any nested event loop. The code lives in
[`src/capture.rs`](../api/tvision_rs/capture/index.html) and is driven by the one
event loop in [`Program`](../api/tvision_rs/app/struct.Program.html).

## The stack

The event loop owns a single
[`CaptureStack`](../api/tvision_rs/capture/struct.CaptureStack.html), a LIFO stack
of handlers. When an event arrives, the loop offers it to the handlers **top-down**
(most recently pushed first) before any view sees it. Each handler implements
[`CaptureHandler`](../api/tvision_rs/capture/trait.CaptureHandler.html) and answers
with a [`CaptureFlow`](../api/tvision_rs/capture/enum.CaptureFlow.html):

| `CaptureFlow` | Meaning |
| ------------- | ------- |
| `Pass`        | I did not handle it. Offer it to the next lower handler, then to normal routing. |
| `Consumed`    | I handled it; stop routing. I stay on the stack. |
| `ConsumedPop` | I handled it **and** I am done. Remove me from the stack (for example a modal dialog closing, a mouse button released). |

A handler holds a [`ViewId`](../api/tvision_rs/view/struct.ViewId.html) for the view
it speaks for, never a view reference (see [Pointers & infoPtr →
handles](handles.md)). **A handler that consumes every otherwise-unhandled event
behaves like a blocking loop, so no code needs to write one.**

## Pushing is cooperative

A handler runs while the loop is holding the stack, so it cannot push another
handler inline. It requests the push via
[`Context::push_capture`](../api/tvision_rs/view/struct.Context.html#method.push_capture)
or the higher-level helpers, and the loop performs the push *after* dispatch,
through the [Deferred channel](deferred.md). As a result, a freshly pushed
capture first sees the **next** event, never the one that pushed it.

## What rides on it

The capture stack carries several features:

- **Modal dialogs.** A modal dialog pushes a `ModalFrame`. While it is on the
  stack, mouse events are gated by the dialog's bounds: inside, they pass;
  outside, the pump hands them to the dialog alone, so the windows beneath never
  see the click. That gate is what "modal" means here. Keys, commands,
  pastes and the mouse wheel pass the frame and then go to the dialog alone;
  broadcasts pass and reach the whole tree. See [Modal `execView`](modal.md).

- **Window drag and resize.** Grabbing a window's title bar or a resize corner in
  the [`Frame`](../api/tvision_rs/frame/struct.Frame.html) pushes a drag handler that
  owns the mouse until the button is released, translating each move into a new
  window position or size. The drag handler, the keyboard-resize handler and an
  open menu consume every other event while they hold the input, broadcasts
  included.

- **Press-and-hold across the widgets.** A press and hold on a control makes
  the widget push a mouse-tracking capture that forwards masked move/auto-repeat
  events to that one view and discards everything else until the button comes up.
  Roughly ten widgets use this: [`Button`](../api/tvision_rs/widgets/struct.Button.html),
  [`Cluster`](../api/tvision_rs/widgets/struct.Cluster.html),
  [`Editor`](../api/tvision_rs/widgets/struct.Editor.html),
  [`InputLine`](../api/tvision_rs/widgets/struct.InputLine.html),
  [`ScrollBar`](../api/tvision_rs/widgets/struct.ScrollBar.html),
  `ListViewer`, `Outline`, the status line, the menus, and the window
  [`Frame`](../api/tvision_rs/frame/struct.Frame.html). Each uses it for its own
  gesture (autoscroll, rubber-band selection, repeating arrows, drag-select), all
  over the same `MouseTrackCapture`.

Because a capture forwards events by `ViewId` rather than by reference, the loop
performs the actual delivery on the widget's behalf. The cross-view plumbing for
this (and for scrollbar↔scroller traffic) is the [cross-view brokering](../internals/brokering.md)
seam. For the step-by-step path of each event through the stack and then the
tree, see [the event loop in depth](../internals/event-loop.md).
