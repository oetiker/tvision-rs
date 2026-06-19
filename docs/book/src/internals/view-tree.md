# The view tree

Everything you see on screen is a node in one tree. The desktop is a node; each
window on it is a node; the frame, scrollbars, buttons and input lines inside a
window are nodes. tvision-rs calls the core abstraction the `View` trait and the
container type `Group`, keeping the tree model but rendering it in idiomatic Rust.

## A trait, not a base class

Every widget in tvision-rs is defined by two things working together:

- **[`View`](../api/tvision_rs/view/trait.View.html)** — the *behaviour*. It is a
  trait whose required methods cover everything the framework calls:
  [`draw`](../api/tvision_rs/view/trait.View.html#method.draw)
  paints the view, [`handle_event`](../api/tvision_rs/view/trait.View.html#method.handle_event)
  reacts to input, and a handful of others (`set_state`, `valid`, `value` /
  `set_value`) cover focus, validation and data transfer.
- **[`ViewState`](../api/tvision_rs/view/struct.ViewState.html)** — the *data*. A
  struct that every widget embeds (conventionally a field named `state`),
  holding `origin`, `size`, `cursor`, the state / option / grow / drag flags,
  the event mask and the help context.

Every widget **embeds** a `ViewState` and **implements** `View`. Only three
methods are required —
[`state`](../api/tvision_rs/view/trait.View.html#method.state),
[`state_mut`](../api/tvision_rs/view/trait.View.html#method.state_mut), and `draw`;
the rest have sensible defaults. That is the whole composition recipe, and the
[Writing your own View](custom-view.md) chapter walks it end to end.

> **Turbo Vision heritage:** the C++ library built every widget by *inheriting*
> from `TView`. The `View` trait covers `TView`'s virtual methods; `ViewState`
> covers its data members.

## Flags became fields

The boolean properties of a view — visibility, focus, whether it responds to
keyboard input — are organized into four **structs of named booleans**, one per
logical family. You read `self.state.state.visible` directly; no bitmask
arithmetic is needed.

Coming from C++ Turbo Vision, the flag word families map like this:

| C++ flag word | tvision-rs struct |
| ------------- | -------------- |
| `sfXxx` (state)     | [`State`](../api/tvision_rs/view/struct.State.html)     |
| `ofXxx` (options)   | [`Options`](../api/tvision_rs/view/struct.Options.html) |
| `gfXxx` (grow mode) | [`GrowMode`](../api/tvision_rs/view/struct.GrowMode.html) |
| `dmXxx` (drag mode) | [`DragMode`](../api/tvision_rs/view/struct.DragMode.html) |

The propagating subset of state flags a parent flips on a child during focus and
activation is the [`StateFlag`](../api/tvision_rs/view/enum.StateFlag.html) enum
(`Active`, `Selected`, `Focused`, `Dragging`).

## Geometry

A view's rectangle is two corners: [`Rect`](../api/tvision_rs/view/struct.Rect.html)
holds a top-left `a` (inclusive) and bottom-right `b` (exclusive), each a
[`Point`](../api/tvision_rs/view/struct.Point.html). Coordinates are `i32` — signed,
because origins go negative when a view scrolls offscreen, and faithful to
magiblot's widening of the historical `short` to `int`. `Rect` carries the
familiar chained mutators (`grow`, `intersect`, plus `r#move` and `r#union`,
raw-named to dodge the Rust keywords).

A view's `origin` is **relative to its owner**, not the screen. The tree resolves
the absolute position as it descends — there is no up-pointer to walk.

## Groups are the branch nodes

A [`Group`](../api/tvision_rs/view/struct.Group.html) is a `View` that owns child
views — the port of `TGroup`, and the node type of the tree. The desktop, every
window and every dialog are groups. A group:

- **owns its children in a `Vec`** in back-to-front paint order:
  `children[0]` is the bottom (drawn first), `children.last()` is the
  frontmost (drawn last);
- **draws** its children back-to-front (painter's algorithm — there is no damage
  tracking; the whole tree redraws and a diff finds the changes);
- **routes events** to them and tracks which child is *current* (focused).

> **Turbo Vision heritage:** `TGroup` stored children in a circular `next`/`prev`
> ring with a per-child `owner` back-pointer. tvision-rs replaces this with a plain
> `Vec` and handle-based addressing — children have no up-pointer.

Because a child has no pointer back to its parent, cross-references use a
**[`ViewId`](../api/tvision_rs/view/struct.ViewId.html) handle** instead of a raw
pointer. Each view's id is minted when it is inserted into a
group and stamped into its own `ViewState`; the framework resolves a handle back
to a view by walking down from the group. This is also how a window addresses one
of its controls, and how a leaf view reaches a sibling — see
[Cross-view brokering & ViewId](brokering.md).

## Where to go next

- **[The event loop in depth](event-loop.md)** — how an event travels down the
  tree and how the group's three-phase routing picks a receiver.
- **[Writing your own View](custom-view.md)** — the composition recipe in
  practice, plus the `#[delegate]` macro for embed-and-forward, and how to choose
  a `Role` for a custom view's colors (see
  [A custom view's colors](custom-view.md#a-custom-views-colors)).
