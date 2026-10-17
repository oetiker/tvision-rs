# What "faithful" means

Almost everything a C++ Turbo Vision programmer knows still holds in
tvision-rs. This part of the guide is written for that reader. It explains the
*ideas* behind the few places where the Rust looks different from the C++, so
that each familiar construct can be traced to what it became.

## Faithful by default

tvision-rs is a **faithful** port of
[magiblot/tvision](https://github.com/magiblot/tvision). Class structure, method
names, control flow, algorithms, and observable behaviour are reproduced as-is.
The event loop dispatches the same way, modal dialogs nest the same way, the
draw model paints the same way, and the widgets behave as Turbo Vision
widgets do. `handleEvent` became `handle_event` on the
[`View`](../api/tvision_rs/view/trait.View.html) trait, but it is called at the same
moments, receives the same events, and is expected to consume them the same way.

The rule the port follows: **if a behaviour isn't called out as a
deliberate departure, it was translated straight from the C++.** The answer to
"does it still work like Turbo Vision here?" is *yes* unless a chapter in this
part says otherwise.

## Why port faithfully

Turbo Vision's command bus, its modal `execView`, its desktop/window/dialog
layering, and its view-owns-its-children tree were refined over decades and fit
together. Re-deriving that design from scratch would gain nothing. Faithfulness
also means existing Turbo Vision knowledge, and the
[magiblot documentation](https://github.com/magiblot/tvision), transfers
directly, and the port's behaviour can be checked against the C++ source
instead of guessed at.

## Where it departs, and why

The places where tvision-rs looks different from the C++ are a small,
deliberate set with one root cause. Turbo Vision's `T` prefixes, bit-packed flag
words, raw pointers, and hand-rolled machinery were workarounds for features the
language of 1991 lacked and Rust has built in. The approach is the same every
time:

> Recognize each 1991 workaround, replace it with the language feature, and keep
> everything around it faithful.

`TView` becomes the [`View`](../api/tvision_rs/view/trait.View.html) trait (a
namespace C++ approximated with a `T` prefix). `state & sfFocused` becomes a boolean
field. The `TEvent.what` union becomes an `enum`. A raw `TView*` becomes a
[`ViewId`](../api/tvision_rs/view/struct.ViewId.html) handle. Each time, the
Rust construct carries the meaning the workaround approximated, so the
surrounding code stays a direct translation.

## The chapters that follow

Each of the next chapters covers one of these departures: what the C++ does,
what tvision-rs does instead, and how the rest of the faithful port connects to
it:

- [Inheritance → trait + composition](inheritance.md): the `TView` hierarchy
  becomes the [`View`](../api/tvision_rs/view/trait.View.html) trait plus
  [`ViewState`](../api/tvision_rs/view/struct.ViewState.html) composition.
- [Pointers & infoPtr → handles](handles.md): `TView*` becomes a
  [`ViewId`](../api/tvision_rs/view/struct.ViewId.html) resolved through a
  downward-borrowed [`Context`](../api/tvision_rs/view/struct.Context.html).
- [Events → enum + match](events.md): the `TEvent` union becomes a sum type.
- [Flag words → struct-of-bools](flags.md): `ofXxx`/`sfXxx` bitmasks become
  named boolean fields.
- [Constant families → open newtypes](constants.md): `cmXxx`/`hcXxx` become
  namespaced consts like `Command::OK`.
- [Palettes & glyphs → Theme/Role](theme.md): the palette chain becomes a
  [`Theme`](../api/tvision_rs/theme/struct.Theme.html) keyed by semantic roles.
- [The draw model](draw.md): `drawView`/`drawSubViews` becomes whole-tree
  redraw plus a cell-buffer diff.
- [Modal execView → one loop + capture](modal.md): nested modal loops collapse
  into a single event loop driven by a capture stack.
- [The Deferred channel](deferred.md): effects a downward-borrowed view can't
  perform inline are routed back to the loop owner.
- [Dropped & changed](dropped.md): the few pieces that were removed or replaced
  outright, and what stands in for them.

For the mapping in one table see
[Differences from C++ Turbo Vision](../reference/deviations.md). For the
one-to-one name lookup, see the [symbol map](../reference/symbol-map.md).
