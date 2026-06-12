# Events → enum + match

Turbo Vision packs every event into one `TEvent` record: a `what` bitmask that
says which kind, plus a `union { mouse; keyDown; message; }` that you read
according to `what`. You decode an event by masking (`if (what & evMouse)`) and
then reaching into the right union arm. It is compact, but nothing stops you from
reading the wrong arm, and the `void* infoPtr` inside the `message` arm is used
three unrelated ways.

In rstv the record becomes a real Rust sum type, [`Event`](../api/tvision/event/enum.Event.html),
which you **match arm-by-arm** instead of masking. Each `ev*` class maps onto one
variant, and the variant *carries the payload that class actually uses* — so the
compiler, not a convention, guarantees you only read fields that exist:

```rust,ignore
match event {
    Event::KeyDown(k)         => { /* k.key, k.modifiers */ }
    Event::MouseDown(m)       => { /* m.position, m.buttons */ }
    Event::Command(cmd)       => { /* a command for me */ }
    Event::Broadcast { .. }   => { /* a command for whoever cares */ }
    _ => {}
}
```

A handled event is consumed by setting it to [`Event::Nothing`](../api/tvision/event/enum.Event.html) —
the `clearEvent` equivalent, spelled [`event.clear()`](../api/tvision/event/enum.Event.html#method.clear).
`evNothing` and a consumed event are the same variant, exactly as in C++.

## The split that `infoPtr` forced

C++ `message(receiver, what, command, infoPtr)` did double duty: it both
*delivered* a command and *round-tripped a result* back through the event's
`void* infoPtr`. That one pointer field meant three different things across the
code base, so it splits into typed mechanisms instead of one untyped slot:

- **Targeted command** — [`Event::Command`](../api/tvision/event/enum.Event.html)
  carries **only** the [`Command`](../api/tvision/command/struct.Command.html). The
  C++ command-target hint on `infoPtr` is not carried: focused-command routing
  already delivers the command to the active window, so the hint checked nothing.
- **Broadcast subject** — [`Event::Broadcast { command, source }`](../api/tvision/event/enum.Event.html)
  reinstates the broadcast-subject use of `infoPtr` as `source: Option<ViewId>`.
  It names *which view this broadcast is about* — e.g. which scrollbar moved — as
  a resolvable [`ViewId`](../api/tvision/view/struct.ViewId.html) handle, not a
  raw pointer (see [Pointers & infoPtr → handles](handles.md)). A receiver's C++
  test `infoPtr == hScrollBar` becomes `source == self.h_scroll_bar`. Broadcasts
  about no particular view pass `None`.
- **Integer payload** — the timer id that C++ smuggled through `infoPtr` on a
  `cmTimerExpired` broadcast is an integer, not a view, so it gets its own typed
  variant [`Event::Timer`](../api/tvision/event/enum.Event.html) rather than being
  forced into `source`.

## The `eventMask` that survived

`TView::eventMask` was a bit-word gating which classes a view would receive.
Mouse-down/up, key-down, command and broadcast are always delivered, so the only
part worth keeping is the opt-in for the *expensive* classes: continuous
mouse-tracking ([`Event::MouseMove`](../api/tvision/event/enum.Event.html)) and
auto-repeat ([`Event::MouseAuto`](../api/tvision/event/enum.Event.html)). The
bit-word therefore collapses to a two-bool [`EventMask`](../api/tvision/event/struct.EventMask.html)
(a flag-word → struct-of-bools move; see [Flag words → struct-of-bools](flags.md)).

The *routing* of all this is ported faithfully — positional events to the
top-most child under the cursor, focused events through the pre-process / focused
/ post-process passes. For the mechanics, see
[The event loop in depth](../internals/event-loop.md). For the at-a-glance
summary see [deviation D4](../reference/deviations.md#d4).
