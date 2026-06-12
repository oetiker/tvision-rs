# The view tree

Everything you see on screen is a node in one tree. The desktop is a node; each
window on it is a node; the frame, scrollbars, buttons and input lines inside a
window are nodes. Turbo Vision called the base class `TView` and the container
`TGroup`; tvision keeps the model but renders it in idiomatic Rust.

## A trait, not a base class

C++ Turbo Vision built every widget by *inheriting* from `TView`. Rust has no
inheritance, so the hierarchy splits in two (this is deviation **D2**):

- **[`View`](../api/tvision/view/trait.View.html)** — the *behaviour*. It is the
  port of `TView`'s virtual methods: [`draw`](../api/tvision/view/trait.View.html#method.draw)
  paints the view, [`handle_event`](../api/tvision/view/trait.View.html#method.handle_event)
  reacts to input, and a handful of others (`set_state`, `valid`, `value` /
  `set_value`) cover focus, validation and data transfer.
- **[`ViewState`](../api/tvision/view/struct.ViewState.html)** — the *data*. It
  is the port of `TView`'s data members: `origin`, `size`, `cursor`, the state /
  option / grow / drag flags, the event mask and the help context.

Every widget **embeds** a `ViewState` (conventionally a field named `state`) and
**implements** `View`. Only three methods are required —
[`state`](../api/tvision/view/trait.View.html#method.state),
[`state_mut`](../api/tvision/view/trait.View.html#method.state_mut), and `draw`;
the rest have sensible defaults. That is the whole composition recipe, and the
[Writing your own View](custom-view.md) chapter walks it end to end.

## Flags became fields

Turbo Vision packed its `sfXxx` / `ofXxx` / `gfXxx` / `dmXxx` flag words into
single integers. tvision unpacks each family into a **struct of named booleans**
(deviation **D5**), so you read `self.state.state.visible` instead of masking
`sfVisible`:

| C++ flag word | tvision struct |
| ------------- | -------------- |
| `sfXxx` (state)     | [`State`](../api/tvision/view/struct.State.html)     |
| `ofXxx` (options)   | [`Options`](../api/tvision/view/struct.Options.html) |
| `gfXxx` (grow mode) | [`GrowMode`](../api/tvision/view/struct.GrowMode.html) |
| `dmXxx` (drag mode) | [`DragMode`](../api/tvision/view/struct.DragMode.html) |

The propagating subset of state flags a parent flips on a child during focus and
activation is the [`StateFlag`](../api/tvision/view/enum.StateFlag.html) enum
(`Active`, `Selected`, `Focused`, `Dragging`).

## Geometry

A view's rectangle is two corners: [`Rect`](../api/tvision/view/struct.Rect.html)
holds a top-left `a` (inclusive) and bottom-right `b` (exclusive), each a
[`Point`](../api/tvision/view/struct.Point.html). Coordinates are `i32` — signed,
because origins go negative when a view scrolls offscreen, and faithful to
magiblot's widening of the historical `short` to `int`. `Rect` carries the
familiar chained mutators (`grow`, `intersect`, plus `r#move` and `r#union`,
raw-named to dodge the Rust keywords).

A view's `origin` is **relative to its owner**, not the screen. The tree resolves
the absolute position as it descends — there is no up-pointer to walk.

## Groups are the branch nodes

A [`Group`](../api/tvision/view/struct.Group.html) is a `View` that owns child
views — the port of `TGroup`, and the node type of the tree. The desktop, every
window and every dialog are groups. A group:

- **owns its children in a `Vec`** in back-to-front paint order (deviation
  **D3**): `children[0]` is the bottom (drawn first), `children.last()` is the
  frontmost (drawn last). C++'s circular `next`/`prev` ring and per-child `owner`
  back-pointer are gone;
- **draws** its children back-to-front (painter's algorithm — there is no damage
  tracking; the whole tree redraws and a diff finds the changes);
- **routes events** to them and tracks which child is *current* (focused).

Because a child has no pointer back to its parent, cross-references use a
**[`ViewId`](../api/tvision/view/struct.ViewId.html) handle** instead of a raw
pointer (deviation **D3**). Each view's id is minted when it is inserted into a
group and stamped into its own `ViewState`; the framework resolves a handle back
to a view by walking down from the group. This is also how a window addresses one
of its controls, and how a leaf view reaches a sibling — see
[Cross-view brokering & ViewId](brokering.md).

## Where to go next

- **[The event loop in depth](event-loop.md)** — how an event travels down the
  tree and how the group's three-phase routing picks a receiver.
- **[Writing your own View](custom-view.md)** — the composition recipe in
  practice, plus the `#[delegate]` macro for embed-and-forward.
