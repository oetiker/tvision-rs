# Writing your own View

Everything on the screen is a [`View`](../api/tvision/view/trait.View.html) — a
button, a window, the desktop background. This is the capstone of the *How It
Works* part: once you can write a `View`, the rest of the framework is just
ready-made views you can use or replace. This page walks the whole path twice —
first a trivial **leaf** built from scratch, then how to wrap an existing widget
and let a macro write the boilerplate.

## The shape of a view

A view is the port of C++ `TView`: the
inheritance hierarchy became a `View` **trait** plus a
[`ViewState`](../api/tvision/view/struct.ViewState.html) **composition target**.
Where C++ would write `class MyView : public TView`, you instead *embed* a
`ViewState` field (TV's data members — geometry, the `sf*`/`of*` flags, the help
context) and `impl View` (TV's virtual methods). See
[Inheritance → trait + composition](../port/inheritance.md) for the why.

The trait has exactly three methods you **must** supply —
[`state`](../api/tvision/view/trait.View.html#tymethod.state),
[`state_mut`](../api/tvision/view/trait.View.html#tymethod.state_mut), and
[`draw`](../api/tvision/view/trait.View.html#tymethod.draw). Every other method
(`handle_event`, `set_state`, `value`, `calc_bounds`, …) has a sensible default,
so a static, non-interactive view needs only those three. The first two are pure
boilerplate — hand back the embedded state — so in practice the only code you
*write* is `draw` plus whatever behaviour you want to customise.

## A trivial leaf view

Here is a complete view that fills its rectangle and prints a centered label —
the same pattern the real
[`StaticText`](../api/tvision/widgets/struct.StaticText.html) widget follows:

```rust,ignore
use tvision::{DrawCtx, Rect, Role, View, ViewState};

struct Banner {
    state: ViewState,
    text: String,
}

impl Banner {
    fn new(bounds: Rect, text: impl Into<String>) -> Self {
        Banner { state: ViewState::new(bounds), text: text.into() }
    }
}

impl View for Banner {
    fn state(&self) -> &ViewState { &self.state }
    fn state_mut(&mut self) -> &mut ViewState { &mut self.state }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        let style = ctx.style(Role::StaticText);
        // Paint the whole view-local extent, then write the label.
        ctx.fill(self.state.get_extent(), ' ', style);
        let x = (self.state.size.x - self.text.chars().count() as i32) / 2;
        ctx.put_str(x, 0, &self.text, style);
    }
}
```

Three things worth noting:

- **Construct state with [`ViewState::new(bounds)`](../api/tvision/view/struct.ViewState.html#method.new)**,
  never `ViewState::default()` for a real view — `new` applies `TView`'s exact
  constructor defaults (visible, the `dmLimitLoY` drag limit). An all-zero state
  would be invisible.
- **Draw in *view-local* coordinates.** `DrawCtx` clips and offsets for you; the
  view's own extent is always `0,0 .. size.x,size.y`
  ([`get_extent`](../api/tvision/view/struct.ViewState.html#method.get_extent)).
- **Colors come from a [`Role`](../api/tvision/theme/enum.Role.html), not a
  palette.** There is no `getColor(n)` indirection — you ask
  the theme for a role and get a [`Style`](../api/tvision/color/struct.Style.html)
  back. See [Theming & colors](../apps/theming.md).

Insert it into a group (a window, the desktop) and the
[event loop](event-loop.md) draws it on the next pump. A leaf that overrides
nothing else is inert: it ignores events, carries no transferable value, and is
not selectable.

## Adding behaviour

To react to input, override
[`handle_event`](../api/tvision/view/trait.View.html#method.handle_event) (the
base is a no-op — the event passes through). A leaf cannot mutate loop-owned
state directly; it asks for an effect through its `&mut Context`. Closing
yourself, enabling a command, focusing a sibling — all go through the
[Deferred channel](deferred.md), and cross-view reads/writes through
[brokering](brokering.md). Match on the
[`Event`](../api/tvision/event/enum.Event.html) enum (see
[Events → enum + match](../port/events.md)) and clear the event once you have
consumed it so it does not route further.

Other commonly overridden hooks:
[`value`](../api/tvision/view/trait.View.html#method.value) /
[`set_value`](../api/tvision/view/trait.View.html#method.set_value) to make a
data control that participates in dialog gather/scatter,
[`size_limits`](../api/tvision/view/trait.View.html#method.size_limits) to impose
a minimum size, and
[`set_state`](../api/tvision/view/trait.View.html#method.set_state) to react when
you gain or lose focus.

## Wrapping an existing view: `#[delegate]`

Most "custom views" are not built from bare `ViewState` — they *specialise* an
existing widget. In C++ you would subclass (`class MyDialog : public TDialog`)
and inherit every virtual for free. In Rust, composition gives nothing for free:
embed a `Dialog` and you must hand-forward every `View` method you did not
override — and the trait has roughly two dozen — to the inner field.

That boilerplate is what the `#[delegate]` macro removes. Re-exported as
`tvision::delegate`, it goes on the `impl View` block: write only the methods
that differ, and the macro injects a forwarder (`self.<field>.method(args)`) for
every method you did **not** write.

```rust,ignore
use tvision::delegate;

struct MyTerminal {
    scroller: Scroller,
}

#[delegate(to = scroller)]
impl View for MyTerminal {
    // Only the methods that differ are written by hand…
    fn draw(&mut self, ctx: &mut DrawCtx) { /* custom rendering */ }
    // …everything else (state, state_mut, handle_event, calc_bounds, …)
    // is forwarded to `self.scroller` automatically.
}
```

The attribute reads the trait name from the `impl Trait for Type` line and the
field from `to = <field>`; it never needs the method list spelled out. A
`skip(method, …)` clause leaves a named method at its **trait default** instead
of forwarding it — used when forwarding would be wrong (for example a wrapper
whose own `size_limits` must win over the inner group's). The full rationale,
including the path-resolution trick that makes generated forwarders compile under
any consumer alias, is in the design note
`docs/design/delegation-macros.md`.

One caveat worth internalising: if you add a brand-new *defaulted* method to the
`View` trait itself, you must also teach the macro's spec table about it
(`tvision-macros/src/specs.rs`) — otherwise delegating types silently fall back
to the default rather than forwarding. The required methods are caught at compile
time; defaulted ones are not. As a consumer writing your own views you will
rarely touch the trait, so this is mainly a note for the library's own
maintainers.

## Where to go next

- [The view tree](view-tree.md) — how groups own and lay out their children.
- [Deferred effects](deferred.md) — how a leaf requests changes to loop state.
- [Controls](../apps/controls.md) — the ready-made views you will reach for
  before writing your own.
