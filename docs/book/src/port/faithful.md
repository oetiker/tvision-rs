# What "faithful" means

If you know C++ Turbo Vision, the fastest way to learn tvision-rs is to lean on what
you already know — because almost all of it still holds. This part of the guide
is written for you. It explains the *ideas* behind the handful of places where
the Rust looks different from the C++, so that when you reach for a familiar
construct you know exactly what it became.

## Faithful by default

tvision-rs is a **faithful** port of
[magiblot/tvision](https://github.com/magiblot/tvision). Class structure, method
names, control flow, algorithms, and observable behaviour are reproduced as-is.
The event loop dispatches the same way, modal dialogs nest the same way, the
draw model paints the same way, and the widgets behave the way Turbo Vision
widgets have always behaved. `handleEvent` became `handle_event` on the
[`View`](../api/tvision_rs/view/trait.View.html) trait, but it is called at the same
moments, receives the same events, and is expected to consume them the same way.

The rule the port follows is simple: **if a behaviour isn't called out as a
deliberate departure, it was translated straight from the C++.** When you wonder
"does it still work like Turbo Vision here?", the default answer is *yes*.

## Why port faithfully

A faithful port is not nostalgia. Turbo Vision is a mature, battle-tested design:
its command bus, its modal `execView`, its desktop/window/dialog layering, and
its view-owns-its-children tree are decades-refined and fit together. Re-deriving
that design from scratch would throw away that maturity for no gain. Faithfulness
also means your existing Turbo Vision knowledge — and the
[magiblot documentation](https://github.com/magiblot/tvision) — transfers
directly, and that the port's behaviour can be checked against the C++ source
rather than guessed at.

## Where it departs — and why

The places tvision-rs looks different from the C++ are a small, deliberate set, and
they share one root cause. Turbo Vision's `T` prefixes, bit-packed flag words,
raw pointers, and hand-rolled machinery were workarounds for features the
language of 1991 lacked — features Rust has built in. So the guiding move is the
same every time:

> Recognize each 1991 workaround and replace it with the real feature — then keep
> everything around it faithful.

`TView` becomes the [`View`](../api/tvision_rs/view/trait.View.html) trait (a
namespace C++ faked with a `T` prefix). `state & sfFocused` becomes a boolean
field. The `TEvent.what` union becomes an `enum`. A raw `TView*` becomes a
[`ViewId`](../api/tvision_rs/view/struct.ViewId.html) handle. Each time, the modern
construct carries the same meaning the workaround was approximating, so the
surrounding code stays a direct translation — not a redesign.

## The chapters that follow

Each of the next chapters takes one of these departures and tells its story —
what the C++ does, what tvision-rs does instead, and how the rest of the faithful port
plugs back in:

- [Inheritance → trait + composition](inheritance.md) — the `TView` hierarchy
  becomes the [`View`](../api/tvision_rs/view/trait.View.html) trait plus
  [`ViewState`](../api/tvision_rs/view/struct.ViewState.html) composition.
- [Pointers & infoPtr → handles](handles.md) — `TView*` becomes a
  [`ViewId`](../api/tvision_rs/view/struct.ViewId.html) resolved through a
  downward-borrowed [`Context`](../api/tvision_rs/view/struct.Context.html).
- [Events → enum + match](events.md) — the `TEvent` union becomes a sum type.
- [Flag words → struct-of-bools](flags.md) — `ofXxx`/`sfXxx` bitmasks become
  named boolean fields.
- [Constant families → open newtypes](constants.md) — `cmXxx`/`hcXxx` become
  namespaced consts like `Command::OK`.
- [Palettes & glyphs → Theme/Role](theme.md) — the palette chain becomes a
  [`Theme`](../api/tvision_rs/theme/struct.Theme.html) keyed by semantic roles.
- [The draw model](draw.md) — `drawView`/`drawSubViews` becomes whole-tree
  redraw plus a cell-buffer diff.
- [Modal execView → one loop + capture](modal.md) — nested modal loops collapse
  into a single event loop driven by a capture stack.
- [The Deferred channel](deferred.md) — effects a downward-borrowed view can't
  perform inline are routed back to the loop owner.
- [Dropped & changed](dropped.md) — the few pieces that were removed or replaced
  outright, and what stands in for them.

For the at-a-glance mapping see
[Differences from C++ Turbo Vision](../reference/deviations.md); the chapters
that follow tell each story. For the terse one-to-one name lookup, see the
[symbol map](../reference/symbol-map.md).
