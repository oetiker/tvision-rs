# Events → enum + match

Turbo Vision packs every event into one `TEvent` record: a `what` bitmask that
says which kind, plus a `union { mouse; keyDown; message; }` that you read
according to `what`. You decode an event by masking (`if (what & evMouse)`) and
then reaching into the right union arm. It is compact, but nothing stops you from
reading the wrong arm, and the `void* infoPtr` inside the `message` arm is used
three unrelated ways.

In tvision-rs the record becomes a real Rust sum type, [`Event`](../api/tvision_rs/event/enum.Event.html),
which you **match arm-by-arm** instead of masking. Each `ev*` class maps onto one
variant, and the variant *carries the payload that class actually uses* — so the
compiler, not a convention, guarantees you only read fields that exist:

```rust
# use tvision_rs as tv;
# use tv::Event;
# #[allow(unused_variables)]
# fn _demo(event: Event) {
match event {
    Event::KeyDown(k)         => { /* k.key, k.modifiers */ }
    Event::MouseDown(m)       => { /* m.position, m.buttons */ }
    Event::Command(cmd)       => { /* a command for me */ }
    Event::Broadcast { .. }   => { /* a command for whoever cares */ }
    _ => {}
}
# }
```

A handled event is consumed by setting it to [`Event::Nothing`](../api/tvision_rs/event/enum.Event.html) —
the `clearEvent` equivalent, spelled [`event.clear()`](../api/tvision_rs/event/enum.Event.html#method.clear).
`evNothing` and a consumed event are the same variant, exactly as in C++.

## The split that `infoPtr` forced

C++ `message(receiver, what, command, infoPtr)` did double duty: it both
*delivered* a command and *round-tripped a result* back through the event's
`void* infoPtr`. That one pointer field meant three different things across the
code base, so it splits into typed mechanisms instead of one untyped slot:

- **Targeted command** — [`Event::Command`](../api/tvision_rs/event/enum.Event.html)
  carries **only** the [`Command`](../api/tvision_rs/command/struct.Command.html). The
  C++ command-target hint on `infoPtr` is not carried: focused-command routing
  already delivers the command to the active window, so the hint checked nothing.
- **Broadcast subject** — [`Event::Broadcast { command, source }`](../api/tvision_rs/event/enum.Event.html)
  reinstates the broadcast-subject use of `infoPtr` as `source: Option<ViewId>`.
  It names *which view this broadcast is about* — e.g. which scrollbar moved — as
  a resolvable [`ViewId`](../api/tvision_rs/view/struct.ViewId.html) handle, not a
  raw pointer (see [Pointers & infoPtr → handles](handles.md)). A receiver's C++
  test `infoPtr == hScrollBar` becomes `source == self.h_scroll_bar`. Broadcasts
  about no particular view pass `None`.
- **Integer payload** — the timer id that C++ smuggled through `infoPtr` on a
  `cmTimerExpired` broadcast is an integer, not a view, so it gets its own typed
  variant [`Event::Timer`](../api/tvision_rs/event/enum.Event.html) rather than being
  forced into `source`.

## The `eventMask` that survived

`TView::eventMask` was a bit-word gating which classes a view would receive.
Mouse-down/up, key-down, command and broadcast are always delivered, so the only
part worth keeping is the opt-in for the *expensive* classes: continuous
mouse-tracking ([`Event::MouseMove`](../api/tvision_rs/event/enum.Event.html)) and
auto-repeat ([`Event::MouseAuto`](../api/tvision_rs/event/enum.Event.html)). The
bit-word therefore collapses to a two-bool [`EventMask`](../api/tvision_rs/event/struct.EventMask.html)
(a flag-word → struct-of-bools move; see [Flag words → struct-of-bools](flags.md)).

The *routing* of all this is ported faithfully — positional events to the
top-most child under the cursor, focused events through the pre-process / focused
/ post-process passes. For the mechanics, see
[The event loop in depth](../internals/event-loop.md). For the at-a-glance
summary see [deviation D4](../reference/deviations.md#d4).

## When no one handles an event

If an event reaches the bottom of the routing chain without any view consuming
it, nothing special happens — the pump simply continues to the deferred drain
and then redraws. There is no abort, no error handler, and no "event not
handled" notification. An unhandled event silently falls out of the dispatch
step.

```rust,ignore
// src/app/program.rs — after dispatch (simplified sketch)
captures.dispatch(&mut ev, &mut ctx);   // capture stack first
if !ev.is_nothing() {
    program_handle_event(group, ..., &mut ev, &mut ctx, ...);
}
// ev may still be Some(something) here — that is fine.
// The pump drains deferred effects and redraws regardless.
```

The only pump-level filtering that *does* drop an event early is the disabled-
command gate: if an event is `Event::Command(c)` and `c` is in the disabled set,
the command is cleared before it reaches any view. All other events — including
`KeyDown`, mouse events, and broadcasts — always enter the routing chain.

This is a deliberate simplification over the C++ `TProgram::eventError`, which
was called when no view consumed an event and could abort the application by
default. In tvision-rs there is no `eventError`: an unhandled event is a
no-op, and the framework keeps running.

**Sources:** the dispatch step and disabled-command gate in `src/app/program.rs`.

> **Turbo Vision heritage:** `TProgram::eventError` (`tprogram.cpp`) fired on
> every unconsumed event and terminated the process by default. tvision-rs
> drops this behaviour entirely — an unhandled event is harmless.

## Event masks

By default a view receives every event class that reaches it through routing —
except two expensive ones that are off unless the view opts in:

| Event class | Default | To opt in |
| ----------- | ------- | --------- |
| `Event::MouseMove` | not delivered | set `state.event_mask.mouse_move = true` |
| `Event::MouseAuto` | not delivered | set `state.event_mask.mouse_auto = true` |

All other classes (`KeyDown`, `MouseDown`, `MouseUp`, `MouseWheel`, `Command`,
`Broadcast`, `Timer`) are delivered unconditionally and cannot be masked out.

The gate is applied per-child inside the group's delivery step. A child that
has not opted into `MouseMove` never receives it, even if the mouse is moving
over it:

```rust
# use tvision_rs as tv;
# use tv::view::{View, ViewState, Context, DrawCtx};
# use tv::event::{Event, EventMask};
# struct TrackingView { state: ViewState }
# impl View for TrackingView {
#     fn state(&self) -> &ViewState { &self.state }
#     fn state_mut(&mut self) -> &mut ViewState { &mut self.state }
#     fn draw(&mut self, _ctx: &mut DrawCtx) {}
# }
// Enable mouse-move and mouse-auto tracking for a view.
fn new() -> TrackingView {
    let mut state = ViewState::new(tv::Rect::new(0, 0, 20, 10));
    state.event_mask = EventMask {
        mouse_move: true,
        mouse_auto: true,
    };
    TrackingView { state }
}
```

The disabled-view gate (`State::disabled`) is separate: a disabled view ignores
positional and focused events (mouse clicks, key-down, commands) but still
receives broadcasts. This matches C++ behaviour where `sfDisabled` blocks
`evMouse | evKeyboard | evCommand` but not `evBroadcast`.

**Sources:** `Group::wants` / `Group::blocked` / `Group::deliver` in
`src/view/group.rs`; `EventMask` in `src/event/mod.rs`.

> **Turbo Vision heritage:** ports `TView::eventMask` (`views.h`). The C++
> bit-word gating collapses to the two-bool `EventMask` struct-of-bools because
> only the two opt-in classes are worth keeping (deviation D5).
