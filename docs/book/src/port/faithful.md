# What "faithful" means

If you know C++ Turbo Vision, the fastest way to learn `tvision` is to lean on
what you already know — because almost all of it still holds. This part of the
guide is for you. It explains the *ideas* behind the handful of places where the
Rust port looks different from the C++, so that when you reach for a familiar
construct you know exactly what it became.

## Faithful by default

`tvision` is a **faithful** port of
[magiblot/tvision](https://github.com/magiblot/tvision). Class structure, method
names, control flow, algorithms, and observable behaviour are reproduced as-is.
The event loop dispatches the same way, modal dialogs nest the same way, the
draw model paints the same way, and the widgets behave the way Turbo Vision
widgets have always behaved. `handleEvent` became `handle_event` on the
[`View`](../api/tvision/view/trait.View.html) trait, but it is called at the same
moments, receives the same events, and is expected to consume them the same way.

The rule the port follows is simple: **if a behaviour isn't called out as a
deliberate deviation, it was translated straight from the C++.** When you wonder
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

## The only departures: D1–D13

The intentional differences are a **fixed, pre-decided set of thirteen
deviations**, D1 through D13. There is no open-ended "we redesigned things" list
— that is the whole point. Each one exists because Turbo Vision's prefixes,
bit-packing, raw pointers, and hand-rolled machinery were workarounds for
features the language of 1991 lacked, and Rust has those features built in:

> Recognize each 1991 workaround and replace it with the real feature — then keep
> everything around it faithful.

`TView` → [`View`](../api/tvision/view/trait.View.html) (a namespace C++ faked
with a `T` prefix). `state & sfFocused` → a boolean field. The `TEvent.what`
union → an `enum`. A raw `TView*` → a
[`ViewId`](../api/tvision/view/struct.ViewId.html) handle. Same idea every time:
the modern construct carries the same meaning the workaround was approximating,
so the surrounding code stays a direct translation.

Each deviation is written the same way — **Baseline** (what the C++ does),
**Deviation** (what we do instead), **Integration** (how the rest of the
faithful port plugs back in). The chapters that follow each take one and tell its
story:

- [Inheritance → trait + composition](inheritance.md) — the `TView` hierarchy
  becomes the [`View`](../api/tvision/view/trait.View.html) trait plus
  [`ViewState`](../api/tvision/view/struct.ViewState.html) composition (D2).
- [Pointers & infoPtr → handles](handles.md) — `TView*` becomes a
  [`ViewId`](../api/tvision/view/struct.ViewId.html) resolved through a
  downward-borrowed [`Context`](../api/tvision/view/struct.Context.html) (D3).
- [Events → enum + match](events.md) — the `TEvent` union becomes a sum type (D4).
- [Flag words → struct-of-bools](flags.md) — `ofXxx`/`sfXxx` bitmasks become
  named boolean fields (D5).
- [Constant families → open newtypes](constants.md) — `cmXxx`/`hcXxx` become
  namespaced consts like `Command::OK`.
- [Palettes & glyphs → Theme/Role](theme.md), the
  [draw model](draw.md), [modal execView → one loop + capture](modal.md), the
  [Deferred channel](deferred.md), and finally what was
  [dropped or changed](dropped.md).

The formal, line-level specification of all thirteen lives in the
[Deviations D1–D13 reference](../reference/deviations.md). This part is the
narrative; that is the spec. For the terse one-to-one name lookup, see the
[symbol map](../reference/symbol-map.md).
