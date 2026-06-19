# Writing your own View

Everything on the screen is a [`View`](../api/tvision_rs/view/trait.View.html) — a
button, a window, the desktop background. This is the capstone of the *How It
Works* part: once you can write a `View`, the rest of the framework is just
ready-made views you can use or replace. This page walks the whole path twice —
first a trivial **leaf** built from scratch, then how to wrap an existing widget
and let a macro write the boilerplate.

## The shape of a view

Every widget in tvision-rs combines two parts: a `View` **trait** that the framework
calls, and a
[`ViewState`](../api/tvision_rs/view/struct.ViewState.html) **struct** you embed to
carry the per-view data — geometry, the state/option flags, the help context.
You *embed* a `ViewState` field in your struct and `impl View` for your type.
See [Inheritance → trait + composition](../port/inheritance.md) for the full
background.

The trait has exactly three methods you **must** supply —
[`state`](../api/tvision_rs/view/trait.View.html#tymethod.state),
[`state_mut`](../api/tvision_rs/view/trait.View.html#tymethod.state_mut), and
[`draw`](../api/tvision_rs/view/trait.View.html#tymethod.draw). Every other method
(`handle_event`, `set_state`, `value`, `calc_bounds`, …) has a sensible default,
so a static, non-interactive view needs only those three. The first two are pure
boilerplate — hand back the embedded state — so in practice the only code you
*write* is `draw` plus whatever behaviour you want to customise.

## A trivial leaf view

Here is a complete view that fills its rectangle and prints a centered label —
the same pattern the real
[`StaticText`](../api/tvision_rs/widgets/struct.StaticText.html) widget follows:

```rust
use tvision_rs::{DrawCtx, Rect, Role, View, ViewState};

# #[allow(dead_code)]
struct Banner {
    state: ViewState,
    text: String,
}

# #[allow(dead_code)]
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

- **Construct state with [`ViewState::new(bounds)`](../api/tvision_rs/view/struct.ViewState.html#method.new)**,
  never `ViewState::default()` for a real view — `new` applies the correct initial
  defaults (visible, the `dmLimitLoY` drag limit). An all-zero state would be
  invisible.
- **Draw in *view-local* coordinates.** `DrawCtx` clips and offsets for you; the
  view's own extent is always `0,0 .. size.x,size.y`
  ([`get_extent`](../api/tvision_rs/view/struct.ViewState.html#method.get_extent)).
- **Colors come from a [`Role`](../api/tvision_rs/theme/enum.Role.html), not a
  palette index.** Ask the theme for a role and get a
  [`Style`](../api/tvision_rs/color/struct.Style.html) back. See
  [Theming & colors](../apps/theming.md).

Insert it into a group (a window, the desktop) and the
[event loop](event-loop.md) draws it on the next pump. A leaf that overrides
nothing else is inert: it ignores events, carries no transferable value, and is
not selectable.

## Adding behaviour

To react to input, override
[`handle_event`](../api/tvision_rs/view/trait.View.html#method.handle_event) (the
base is a no-op — the event passes through). A leaf cannot mutate loop-owned
state directly; it asks for an effect through its `&mut Context`. Closing
yourself, enabling a command, focusing a sibling — all go through the
[Deferred channel](deferred.md), and cross-view reads/writes through
[brokering](brokering.md). Match on the
[`Event`](../api/tvision_rs/event/enum.Event.html) enum (see
[Events → enum + match](../port/events.md)) and clear the event once you have
consumed it so it does not route further.

Other commonly overridden hooks:
[`value`](../api/tvision_rs/view/trait.View.html#method.value) /
[`set_value`](../api/tvision_rs/view/trait.View.html#method.set_value) to make a
data control that participates in dialog gather/scatter,
[`size_limits`](../api/tvision_rs/view/trait.View.html#method.size_limits) to impose
a minimum size, and
[`set_state`](../api/tvision_rs/view/trait.View.html#method.set_state) to react when
you gain or lose focus. A view that launches a modal and needs its typed result
overrides
[`set_modal_data`](../api/tvision_rs/view/trait.View.html#method.set_modal_data)
to load the ordered `FieldValue::List` the pump read out of the modal's fields via
`View::value()` — virtual dispatch, no framework downcast (see [Dialogs & data §
Delivering a modal's result](../apps/dialogs.md#delivering-a-modals-result-back-to-its-launcher)).

## Wrapping an existing view: `#[delegate]`

Most "custom views" are not built from bare `ViewState` — they *specialise* an
existing widget. To do that in tvision-rs you *embed* the widget (e.g. a `Dialog`) and
`impl View` for your wrapper type. The catch: the `View` trait has roughly two
dozen methods, and you must hand-forward every one you did not override to the
inner field — tedious boilerplate that is also easy to get wrong.

That boilerplate is what the `#[delegate]` macro removes.

> **Turbo Vision heritage:** in C++ you would subclass (`class MyDialog : public
> TDialog`) and inherit every virtual method for free. Rust has no inheritance;
> embed-and-delegate via `#[delegate]` is the equivalent.

Re-exported as `tvision_rs::delegate`, it goes on the `impl View` block: write only
the methods that differ, and the macro injects a forwarder
(`self.<field>.method(args)`) for every method you did **not** write.

```rust
use tvision_rs::delegate;
# use tvision_rs::{DrawCtx, Scroller, View};

# #[allow(dead_code)]
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
(`tvision-rs-macros/src/specs.rs`) — otherwise delegating types silently fall back
to the default rather than forwarding. The required methods are caught at compile
time; defaulted ones are not. As a consumer writing your own views you will
rarely touch the trait, so this is mainly a note for the library's own
maintainers.

## Local and global coordinates

A view always draws in its own **local coordinate space**: the top-left corner
of its own extent is `(0, 0)`. When the router delivers a mouse event down the
tree, it subtracts each child's origin as it descends — so the position you
receive in `handle_event` is already local. There is no public `make_local` /
`make_global` helper; the subtraction lives inside `Group::route_event`
(`src/view/group.rs`):

```text
// Inside route_event, before delivering to child i:
ev.offset(-child.origin.x, -child.origin.y);
```

This means a `MouseDown` arriving at `(35, 7)` in the parent's space arrives as
`(3, 2)` in a child whose `origin` is `(32, 5)`. Converting a position you
received in `handle_event` back to the parent's space (e.g. to hit-test against a
sibling) is just adding your own origin:

```rust
# use tvision_rs as tv;
# use tv::{Event, View, ViewState};
# struct MyView { state: ViewState }
# impl View for MyView {
#   fn state(&self) -> &ViewState { &self.state }
#   fn state_mut(&mut self) -> &mut ViewState { &mut self.state }
#   fn draw(&mut self, ctx: &mut tv::DrawCtx) {}
fn handle_event(&mut self, ev: &mut Event, ctx: &mut tv::Context) {
    if let Event::MouseDown(m) = ev {
        // m.position is already in this view's local space.
        let local = m.position;
        // Convert to parent (owner) space, e.g. to compare with a sibling's bounds:
        let in_owner = local + self.state.origin;
        let _ = in_owner;
    }
}
# }
```

Because `DrawCtx` also clips to the view's own extent, you can safely write to
any position within `0 .. size` without worrying about neighboring views.

> **Turbo Vision heritage:** `TView::makeLocal` / `makeGlobal` walked the owner
> chain to convert between coordinate spaces. In tvision-rs the router handles the
> descent automatically; there is no equivalent pair of methods because views have
> no up-pointer to walk back up.

## Overriding `set_state`

The framework flips a small set of [`StateFlag`](../api/tvision_rs/view/enum.StateFlag.html)s
on a view during focus and activation — `Active`, `Selected`, `Focused`,
`Dragging`, `Visible`. To react when one of these changes, override
[`View::set_state`](../api/tvision_rs/view/trait.View.html#method.set_state). The
default implementation sets the flag on `ViewState` and, for `Focused`, broadcasts
`RECEIVED_FOCUS` / `RELEASED_FOCUS`. Always call through to the inner/default
behaviour **first** so the flag is set before your side effects run:

```rust
# use tvision_rs as tv;
# use tv::{Context, Event, Role, StateFlag, View, ViewState};
# struct MyView { state: ViewState }
# impl View for MyView {
#   fn state(&self) -> &ViewState { &self.state }
#   fn state_mut(&mut self) -> &mut ViewState { &mut self.state }
#   fn draw(&mut self, _ctx: &mut tv::DrawCtx) {}
fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
    // 1. Let the base update the flag and broadcast focus events.
    self.state.set_flag(flag, enable);
    if flag == StateFlag::Focused {
        let source = self.state.id();
        ctx.broadcast(
            if enable { tv::Command::RECEIVED_FOCUS } else { tv::Command::RELEASED_FOCUS },
            source,
        );
    }
    // 2. React to the new state — e.g. enable/disable a command when focus changes.
    //    The framework's whole-tree redraw picks up the changed state automatically;
    //    no explicit draw call is needed.
    if flag == StateFlag::Focused && enable {
        ctx.enable_command(tv::Command::OK);
    }
}
# }
```

The same pattern lets you disable a command while unfocused (call
`ctx.disable_command`) or show/hide sibling views (via
`ctx.request_set_visible(sibling_id, enable)`). Because you hold only `&mut
Context` — not a `&mut Group` — effects that reach beyond this view go through
the deferred channel. See [Deferred effects](deferred.md) for the full list.

Source: `src/view/view.rs` (`View::set_state` default), `src/view/group.rs`
(`Group::set_state` propagation to children).

## A custom view's colors

Colors come from a **[`Role`](../api/tvision_rs/theme/enum.Role.html)**, never from
a raw palette index. Ask `DrawCtx` for a role and get back a
[`Style`](../api/tvision_rs/color/struct.Style.html) — a foreground/background color
pair with optional modifiers:

```rust
use tvision_rs::{DrawCtx, Rect, Role, View, ViewState};

# #[allow(dead_code)]
struct Highlighted { state: ViewState }

impl View for Highlighted {
    fn state(&self) -> &ViewState { &self.state }
    fn state_mut(&mut self) -> &mut ViewState { &mut self.state }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        // Pick a role appropriate to the widget's semantic meaning.
        let normal = ctx.style(Role::Normal);
        let focused = ctx.style(Role::Focused);
        let style = if self.state.state.focused { focused } else { normal };
        ctx.fill(self.state.get_extent(), ' ', style);
    }
}
```

The [`Role`](../api/tvision_rs/theme/enum.Role.html) enum is closed — all roles are
first-party. The mapping from role to `Style` lives in the active
[`Theme`](../api/tvision_rs/theme/struct.Theme.html). Custom views pick the role
closest to their semantic meaning:

| Widget kind | Good starting role |
| --- | --- |
| Background / filler | `Role::Normal` |
| Focused editable field | `Role::Focused` |
| Inactive editable field | `Role::Normal` |
| Caption / label | `Role::StaticText` |
| Frame (active window) | `Role::FrameActive` |
| Frame (passive window) | `Role::FramePassive` |
| Disabled control | `Role::Disabled` |

For more on how roles map to colors and how to swap themes at runtime, see
[Theming & colors](../apps/theming.md). Source: `src/theme.rs`, `src/color.rs`.

> **Turbo Vision heritage:** C++ views looked up a palette-index entry
> (`mapColor`) and received a raw terminal attribute byte. tvision-rs replaces every
> palette lookup with a named `Role` (deviation D7), so swapping the whole theme
> repaints the UI without any per-widget change.

## Where to go next

- [The view tree](view-tree.md) — how groups own and lay out their children.
- [Deferred effects](deferred.md) — how a leaf requests changes to loop state.
- [Controls](../apps/controls.md) — the ready-made views you will reach for
  before writing your own.
- [Theming & colors](../apps/theming.md) — the full role catalog and how to build
  a custom theme.
