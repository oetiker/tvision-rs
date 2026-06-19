# Pointers & `infoPtr` → handles

C++ Turbo Vision is held together by raw `TView*` pointers pointing in every
direction at once: `owner` points *up* at the enclosing group, a circular
`next`/`prev`/`last` ring threads the *siblings*, and `current`/`selected`
cross-link a group to its focused and default child. Capture handlers and
`message()` receivers stash bare `TView*` too. Rust forbids that aliased
mutable web. Replacing it with handles is the one structural change every other
part of the port leans on.

## The web splits in two

The handle model cuts the pointer graph along a single clean line: **down is
ownership, up-and-sideways is identity.**

- **Downward is a tree.** A `Group` *owns* its children as
  `Vec<Box<dyn View>>` in Z-order. Recursive dispatch (`for c in &mut
  self.children { … }`) walks them without ever aliasing.
- **Up and sideways are handles.** Every link that used to be a `TView*`
  pointing up or across — `owner`, `current`, `selected`, a captured view, a
  broadcast subject — becomes a [`ViewId`](../api/tvision_rs/view/struct.ViewId.html):
  a `Copy`, lightweight identity that carries *no* reference into the tree, so
  you can store it freely. Identity is just `ViewId` equality.

A `ViewId` is **not** an index into any arena. It is a single,
process-global, monotonic id, minted once when a view is inserted and stamped
into the view's own [`ViewState`](../api/tvision_rs/view/struct.ViewState.html). A
view therefore knows its own handle; ask for it with
[`id()`](../api/tvision_rs/view/struct.ViewState.html#method.id), which returns
`Option<ViewId>` — `None` before the view has been inserted into a group. A
stale handle (its
view long since removed) simply resolves to nothing — there is no slot to
dangle on, so no generational bookkeeping is needed.

## Resolving a handle: the downward `Context`

A child holds no pointer back at its parent and no `&Program`. Everything it
would once have reached *upward* for is instead handed *downward* through a
borrowed [`Context`](../api/tvision_rs/view/struct.Context.html): the owner's size,
the focused-dispatch phase, the disabled-command set, and the queue for
[deferred effects](deferred.md). When the framework needs to act on a view it
holds only by id — move a window's bounds, flip a state flag, deliver a query —
it resolves the id with the
[`find_mut`](../api/tvision_rs/view/trait.View.html#method.find_mut) tree-walk: a
`Group` searches its children and recurses, a `Group`-embedding view delegates
inward, a leaf returns `None`. The same walk powers `remove_descendant`
(self-removal happens in the *owner's* child vector, since a view cannot remove
itself) and focus-by-id.

## `infoPtr` becomes a resolvable subject

Turbo Vision's `message(receiver, evBroadcast, cmXxx, infoPtr)` round-trips a
`void* infoPtr` — used, in practice, three unrelated ways. The dominant one (39
of 42 call sites) is fire-and-forget: the only thing the `infoPtr` carries is
*which view the message is about*. That becomes a posted
[`Event::Broadcast`](../api/tvision_rs/event/enum.Event.html) carrying a `command`
and a `source: Option<ViewId>`, where `source` is a resolvable handle rather
than a pointer. A receiver's C++ test `infoPtr == hScrollBar` becomes the
idiomatic `source == self.h_scroll_bar`. See
[Events → enum + match](events.md) for the broadcast model, and
[Cross-view brokering & `ViewId`](../internals/brokering.md) for how the event
loop brokers a read or write between two sibling views that each hold only the
other's handle.

```rust,ignore
// C++:  if (event.message.infoPtr == hScrollBar) …
// Rust: a scrollbar broadcasts about itself; the scroller filters by id.
if source == Some(self.h_scroll_bar) {
    // react to *this* scrollbar's broadcast
}
```

## What did *not* change

Only two substitutions happen: `TView*` → `ViewId`, and "reach upward" → "read
from `Context`." The focus and traversal *logic* ports verbatim — tab order, the
per-group `current` versus the global focused view, validate-on-focus-leave, and
`makeFirst` raising a window all behave exactly as Turbo Vision always has. And
the genuinely *downward* channels the C++ also had — owner extent, deferred
command-enable, deferred capture pushes — survive untouched, because global
identity resolution addresses sideways links, not the parent→program state a
child still cannot reach upward.

This pairs with [Inheritance → trait + composition](inheritance.md): together
they replace the `TView` class hierarchy and its pointer web with a `View` trait
over an owned tree. For the runtime mechanics, see
[The view tree](../internals/view-tree.md) and
[The event loop in depth](../internals/event-loop.md).
