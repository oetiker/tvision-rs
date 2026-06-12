# The Deferred channel

In C++ Turbo Vision a view does whatever it likes, whenever it likes. Handling
an event, it can call `endModal` up its owner chain, `enableCommand` on the
application, `close()` on itself, or reach across to a sibling scrollbar — all
inline, because every object holds raw pointers to everything around it.

A Rust port cannot work that way, and that single constraint shapes one of the
framework's defining seams.

## Why a leaf can't act directly

While rstv dispatches an event, the view tree is a live `&mut` borrow
**stack**: root → desktop → window → frame → the focused control. Each ancestor
is already borrowed mutably *above* the view on the stack. So the view being
handled cannot reach **up** (its parents are borrowed) or **sideways** (a fresh
`root.find_mut(id)` would alias a borrow already held higher up). It also does
not hold the loop-owned things a C++ view took for granted — the capture stack
that drives modal dialogs, or the program's command set.

The escape hatch is to **not act now**. Instead the view *records a request*,
and the event loop carries it out **after** dispatch unwinds and the tree is
free again. That request is one variant of
[`Deferred`](../api/tvision/view/enum.Deferred.html).

## The shape of it

A view never constructs a `Deferred` itself. It calls a method on the
[`Context`](../api/tvision/view/struct.Context.html) it was handed (the downward
context from [pointers → handles](handles.md), where there are no up-pointers),
and that method pushes the
variant onto a single queue. The loop drains the queue once per pump, in
insertion order, and applies each effect against the state it owns.

```rust,ignore
// inside a view's handle_event: "close me" — recorded, not performed.
ctx.request_close(self.id());
```

Each variant names an effect on loop-owned state that the view could not do
inline. Conceptually they fall into four disjoint families by *what they touch*:

| Family        | Effects                                           | C++ equivalent                     |
| ------------- | ------------------------------------------------- | ---------------------------------- |
| Capture stack | push a capture handler (drives modal input)       | `TGroup` mouse capture             |
| Command set   | enable / disable a command                        | `enableCommand` / `disableCommand` |
| View tree     | change bounds, set a state flag, close, focus, sync scrollbars | `setState`, `close`, sibling pointers |
| Loop state    | end the (modal) loop with a command               | `endModal`                         |

Because the families are disjoint, the order in which the loop applies effects
*across* families never changes the result; effects within one family keep the
order in which they were requested.

## Why this is the right shape

The alternative — handing every view a back-reference to the program — is
exactly the raw-pointer aliasing the port set out to remove. Deferring instead
keeps the borrow checker happy *and* keeps the rule simple: a view describes
**what** it wants; the one place that owns the tree decides **when** and
**how**. A new capability costs only a new variant, never a new wire threaded
through every view.

One consequence is worth internalising: a deferred effect lands *after* the
current event finishes. A pushed capture handler, for instance, sees the
**next** event, never the one being handled. That timing is deliberate and
load-bearing.

## Going deeper

This page is the *why*. For the mechanics — how the loop drains the queue, how
the [cross-view broker](../internals/brokering.md) resolves sibling scrollbars,
and how you add a new effect — see [Deferred effects](../internals/deferred.md)
and [The event loop in depth](../internals/event-loop.md).
