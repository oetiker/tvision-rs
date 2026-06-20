# The data-movement model

A tvision-rs application moves values across view boundaries constantly: a user
types text into an input line and the dialog gathers it; a scroller shifts its
position and two scrollbars have to follow; a color-picker modal closes and its
chosen color lands back in the caller. In C++ Turbo Vision all three of those
motions went through the same loose channel — `getData`/`setData` for records,
`message()`/`infoPtr` for signals, `execView` returning a bare `ushort` — and
the caller interpreted a raw pointer or cast an integer back to a useful type.

Rust will not let you stay vague. Every "move this value to there" needs a typed
answer, and the answer differs by what you are actually moving. tvision-rs gives
**one typed currency and one mechanism per kind of movement**.

## The three kinds

| Kind | Currency / mechanism | Deep-dive |
|------|---------------------|-----------|
| Field & record data (dialog ↔ controls) | [`FieldValue`] via `value()` / `set_value()`; groups gather/scatter an ordered `List` | [Dialogs & data](dialogs.md), [Third-party components](extensibility.md) |
| Sync signals (sibling ↔ sibling) | Defaulted `View` trait methods brokered by the pump (`apply_scroll_sync`, `set_indicator_value`, `apply_page_sync`) | [Cross-view brokering & ViewId](../internals/brokering.md) |
| By-value modal results | `Program::exec_view_with<R>` (result by value) or `Context::request_exec_view` (close command) | [Modal execView → one loop](../port/modal.md) |

[`FieldValue`]: ../api/tvision_rs/data/enum.FieldValue.html

### Field & record data

[`FieldValue`](../api/tvision_rs/data/enum.FieldValue.html) is the single typed
currency for data that moves between a dialog and its controls. The well-known
shapes — `Text`, `Int`, `Bool`, `Bits`, `List` — interoperate with framework
widgets and generic consumers. `Custom(Rc<dyn CustomValue>)` is the open seam
for a payload your own component invents; the framework moves it opaquely and the
consumer downcasts at the edge (runtime-checked, fail-loud).

A control exposes its value via `View::value()` and absorbs a new one via
`View::set_value()`. A group gathers its children in insertion order into a
`FieldValue::List` (`Group::gather_list`) and scatters a list back the same way
(`Group::scatter_list`). This is the typed successor to C++ `getData`/`setData`
— the same positional, anonymous walk over children, now statically typed.

See [Dialogs & data](dialogs.md) for the full recipe and [Third-party
components](extensibility.md) for the `Custom` seam and the three open
extensibility paths.

### Sync signals

A scroller cannot reach its scrollbars inline: during event dispatch it holds
only a downward `&mut Context`, and the borrow checker forbids a sideways reach
to a sibling. Instead the scroller queues a deferred effect
(`Deferred::ScrollSync`), and the **pump** — which owns the whole tree — resolves
the target by `ViewId` and calls the appropriate defaulted `View` method
(`apply_scroll_sync`, `set_indicator_value`, `apply_page_sync`) by virtual
dispatch. No downcast, no pointer-chasing: the pump is the broker.

This is the return-less, deferred successor to C++ `message(target, …, infoPtr)`
(deviation D3/D9). Sync signals are kept separate from field data because they
are behavioral pokes — "recompute from your scrollbar" — not values to be
marshalled and scattered. Folding them into `FieldValue` would be the
over-unification this design deliberately avoids.

See [Cross-view brokering & ViewId](../internals/brokering.md) for the broker
pattern in depth.

### By-value modal results

How a modal hands its result back depends on who opened it.

> **Turbo Vision heritage:** C++ `execView` returned a `ushort` end command;
> the caller read results from the still-live dialog with `getData`. tvision-rs
> keeps that shape — run the modal, read the result before it is torn down —
> but the return type is now caller-named Rust, not a bare integer.

### Choosing how a modal returns its result

The decision rule, stated once:

**Opened from a `Program` / `Application` method** → use
[`Program::exec_view_with<R>`](../api/tvision_rs/app/struct.Program.html#method.exec_view_with).
At the pre-drop window, while the view is still in the tree, the `extract`
closure receives the modal's `&mut dyn View` and the end command; whatever it
returns comes back by value. This is the right path when the result type is rich
and does not map to `FieldValue` (for example `Color` or a whole `Theme`).

```rust,ignore
let chosen: Option<Color> = program.exec_view_with(Box::new(dialog), |modal, cmd| {
    (cmd == Command::OK)
        .then(|| read_color_from(modal))
        .flatten()
});
```

**Opened from inside a view's `handle_event`** → a view holds only
`&mut Context`, never `&mut Program`, so it cannot call `exec_view_with` inline.
Call `ctx.request_exec_view(view, requester, then_command)` instead; this queues
`Deferred::OpenModal` and returns immediately. The pump picks it up on the same
turn, runs the modal through the existing single-loop machinery, and on close
delivers the **close command** to `requester` via `View::set_modal_answer(Command)`
(the requester overrides this to react). A typed data-back path — delivering the
modal's `value()` as a `FieldValue` via `View::set_modal_data` — is a documented
future extension; see [Data-back path](../port/modal.md#data-back-path).

## What stays separate, on purpose

Not everything is forced into `FieldValue` or routed through the pump:

- **`Event::Broadcast { source }`** carries a `ViewId` as a *subject filter*,
  not a value. A scrollbar sets itself as `source` so a scroller can tell which
  bar fired — addressing, not data. This is `infoPtr`-as-subject, unchanged from
  C++ (deviation [D4](../reference/deviations.md#d4)).
- **`Color` and `Theme`** cross by value via `exec_view_with<R>`, not as
  `FieldValue`. `Color` is a four-variant enum (`Default`/`Bios`/`Indexed`/`Rgb`)
  that does not pack into a scalar, and `Theme` is far larger than any field
  exchange warrants. Both are deliberate exceptions, documented at their sites.
- **Structural parent→child pushes** (a frame being told it is zoomed, a window
  pushing display state into its known child frame) still resolve a known child
  directly. This is a different category from data movement — a parent
  orchestrating its own known composition — and stays where it is.

## Where to go next

- [Dialogs & data](dialogs.md) — `FieldValue` in practice: building dialogs,
  gathering and scattering data, and reading results.
- [Third-party components & data interchange](extensibility.md) — the three open
  extensibility paths, `Custom`, `value_as`, and the TypeId caveat.
- [Modal execView → one loop](../port/modal.md) — the launch paths in depth:
  `exec_view_with`, `request_exec_view`, and the Info-box worked example.
- [Cross-view brokering & ViewId](../internals/brokering.md) — the pump as
  broker: sync signals, scrollbar/scroller coordination, and broadcasts.
- [Deviations D1–D13](../reference/deviations.md#d10) — D10 is the typed value
  protocol that replaced the `getData`/`setData` record transfer.
