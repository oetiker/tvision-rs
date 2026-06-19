# Deferred effects

When a view handles an event, it can read and mutate *itself* freely — it holds
`&mut self`. What it cannot do is reach *up* or *sideways* through the view tree.
During dispatch the tree is a live `&mut` borrow stack (root → desktop → window →
the view being handled). Every ancestor above the current view is already
`&mut`-borrowed on that stack, and a fresh `find_mut(id)` from the root would
alias it. The borrow checker forbids it, and that is the *point*: it makes the
Turbo Vision "a view can call up its owner chain" pattern impossible to do
unsafely.

So any action a view wants that touches **loop-owned state it cannot borrow
inline** is *deferred*. The view does not perform the effect — it **requests** it
through its [`Context`](../api/tvision-rs/view/struct.Context.html), and the event
loop **applies** it after dispatch unwinds and the root is free again.

## What "loop-owned state" means

These are the things a downward-borrowed view literally cannot reach:

- the program's **capture stack** (modal frames, drag handlers, menu sessions),
- the **command set** (enable/disable, which the menus and status line mirror),
- **any other view** addressed by [`ViewId`](../api/tvision-rs/view/struct.ViewId.html)
  — a parent, a sibling scrollbar, the clipboard editor in another window,
- the **modal loop** itself (ending it with a result command).

A view that wants to move itself, close itself, focus a sibling, or end a modal
dialog asks the loop to do it.

## The `Deferred` queue

[`Context`](../api/tvision-rs/view/struct.Context.html) carries a single
`&mut Vec<Deferred>`. The request methods on `Context` (`request_close`,
`request_bounds`, `request_focus`, `end_modal`, `enable_command`, the scrollbar
brokers, …) each push one [`Deferred`](../api/tvision-rs/view/enum.Deferred.html)
variant onto it. The view never sees the apply step.

After the dispatch returns, [`pump_once`](../api/tvision-rs/app/struct.Program.html)
takes the queue (`std::mem::take`, so any effect that itself queues a follow-up
lands on the *next* pump, not this drain) and walks it **once, in insertion
order**. Each variant matches to an arm that now holds the loop-owned state the
view could not:

```rust,ignore
// Illustrative sketch — not a standalone program.
let effects: Vec<Deferred> = std::mem::take(deferred);
for effect in effects {
    match effect {
        Deferred::PushCapture(h)      => captures.push(h),
        Deferred::EnableCommand(cmd)  => { /* mutate the live command set */ }
        Deferred::ChangeBounds(id, r) => { if let Some(v) = group.find_mut(id) { v.change_bounds(r); } }
        Deferred::Close(id)           => { /* remove from the owning group */ }
        Deferred::EndModal(cmd)       => { /* set Program::end_state */ }
        Deferred::OpenModal { view, requester, then_command } => {
            // View-launched modal: stash into pending_modal with RouteModalAnswer
            // completion; pump_and_drive runs it via the existing exec_view machinery.
            // On close the pump delivers the close command to `requester` via
            // View::set_modal_answer and re-injects `then_command`.
            /* program.pending_modal = Some((view, RouteModalAnswer { answer_to: requester, then_command })) */
        }
        // … one arm per variant
    }
}
```

The variants fall into a few **disjoint families** by which state they touch —
capture stack, command set, view tree, loop `end_state`. Because no single
dispatch ever queues two conflicting effects on the *same* piece of state,
draining in insertion order is order-equivalent: same-family items keep their
relative order, and cross-family order never changes the result. One ordering
fact is load-bearing: `PushCapture` applies *after* dispatch, so a freshly pushed
handler sees the *next* event, never the one that pushed it.

This drain is what lets a button request `end_modal(Command::OK)` from deep
inside a dialog, a window request its own `Close`, a dragged frame request new
`ChangeBounds`, a scroller stay in sync with a sibling scrollbar it can never
touch directly, and a list view launch a custom modal via `OpenModal` (reusing
the existing `pending_modal` slot + `RouteModalAnswer` completion — no new
`ModalCompletion` variant needed). The cross-view (`ViewId`-addressed) cases are
the subject of [Cross-view brokering & `ViewId`](brokering.md); the drain's place
in the loop is in [The event loop in depth](event-loop.md).

## Adding a new deferred effect

The whole point of the single-enum design is that a new capability is **additive
and local** — it never churns a signature or every call site. To add one:

1. **Add a variant** to [`Deferred`](../api/tvision-rs/view/enum.Deferred.html),
   carrying whatever data the apply step needs (typically a `ViewId` plus
   parameters). Note which state family it touches.
2. **Add a request method** to `Context` that pushes that variant — the view-side
   entry point.
3. **Add the match arm** to the `pump_once` drain that performs the effect against
   the now-reachable loop-owned state.

That is the entire seam. Contrast it with the boundary the queue deliberately
keeps *out*: posted events and broadcasts are **not** deferred effects — they feed
back into the *input* stream and are routed as events at the top of the next pump,
not applied to state at the bottom of this one. `Deferred` means "mutate
loop/tree state after this dispatch"; posting means "produce an event to route
later." Keeping the two apart is what keeps each principled.

The conceptual rationale and the C++ patterns each variant replaces are covered
from the porting angle in [The Deferred channel](../port/deferred.md).
