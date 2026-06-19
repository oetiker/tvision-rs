# Inheritance → trait + composition

C++ Turbo Vision is built on inheritance. `TView` is the base class; `TGroup`,
`TWindow`, `TButton` and every other widget derive from it, override a handful of
virtuals (`draw`, `handleEvent`, `setState`, …), and inherit the rest for free.
Rust has no inheritance, so tvision-rs uses the idiom that replaces it: a **trait**
for behaviour and **composition** for data.

## The `View` trait + `ViewState`

The `TView` class splits into two pieces:

- [`View`](../api/tvision_rs/view/trait.View.html) — the **trait** that carries the
  virtual methods. `draw`, `handle_event`, `set_state`, `calc_bounds` and the
  rest live here. Most have a faithful default body, so a widget overrides only
  what differs.
- [`ViewState`](../api/tvision_rs/view/struct.ViewState.html) — a plain **struct**
  holding what were `TView`'s data members: `origin`, `size`, `cursor`, the
  state/option/grow/drag flag sets, the event mask, the help context, and the
  view's own id. The flag words become named-boolean structs —
  [`State`](../api/tvision_rs/view/struct.State.html) (`sf*`),
  [`Options`](../api/tvision_rs/view/struct.Options.html) (`of*`),
  [`GrowMode`](../api/tvision_rs/view/struct.GrowMode.html) (`gf*`),
  [`DragMode`](../api/tvision_rs/view/struct.DragMode.html) (`dm*`) — see
  [Flag words → struct-of-bools](flags.md).

Every widget *embeds* a `ViewState` and exposes it through the trait's two
required accessors, `state()` and `state_mut()`. Those two — plus `draw`, which
has no sensible base default — are the only methods you *must* write; everything
else defaults.

```rust,ignore
// Illustrative sketch — not a standalone program.
fn state(&self) -> &ViewState { &self.state }
fn state_mut(&mut self) -> &mut ViewState { &mut self.state }
```

So where C++ writes `class TButton : public TView`, tvision-rs writes `struct
Button { state: ViewState, /* … */ }` and `impl View for Button`.

## Building on a *concrete* widget: embed-and-delegate

Subclassing a leaf `TView` is easy — embed a `ViewState`. But Turbo Vision code
also derives from *concrete* widgets: an "About" box is `class AboutDialog :
public TDialog`, reusing all of `TDialog`'s behaviour and overriding only
`draw`. With no inheritance, you can't extend `Dialog`; you **embed** one:

```rust
# use tvision_rs as tv;
# use tv::Dialog;
# #[allow(dead_code)]
struct AboutDialog { dialog: Dialog }
```

But now Rust gives you nothing for free. To behave like its inner `Dialog`, the
embedder must forward *every* un-overridden `View` method to the inner field by
hand — a dozen-plus one-line forwarders like `fn handle_event(&mut self, ev,
ctx) { self.dialog.handle_event(ev, ctx) }`. That boilerplate is the cost of
composition.

## `#[delegate]` removes the boilerplate

The `#[delegate]` attribute macro (from the `tvision-rs-macros` crate, re-exported
as `tv::delegate`) fills the gap automatically. You write only the methods that
differ; the macro injects a forwarder for the rest:

```rust
# use tvision_rs as tv;
# use tv::{delegate, Dialog, View, DrawCtx};
# #[allow(dead_code)]
# struct AboutDialog { dialog: Dialog }
#[delegate(to = dialog)]
impl View for AboutDialog {
    fn draw(&mut self, ctx: &mut DrawCtx) { /* custom paint */ }
    // handle_event, set_state, calc_bounds, … all auto-forward to self.dialog
}
```

It reads the trait name from the `impl` header, collects the methods you wrote,
looks up the trait's full method set from a hand-maintained table, and emits `fn
m(args) { self.dialog.m(args) }` for each method you neither wrote nor listed in
`skip(...)`. A `skip(name)` leaves that method at the trait's *default* body
instead of forwarding — used when forwarding would change behaviour (for
example, `Window` skips `calc_bounds` so its size floor still applies, instead of
deferring to the group's). `skip`-ping a name that isn't a trait method is a hard
error, so a typo can't silently turn into a forward. Real embedders such as
`Terminal` (`#[delegate(to = scroller)]`) and `Desktop` (`#[delegate(to =
group)]`) reuse their inner view this way.

> **When you add a `View` method, add its forwarder too.** The macro keeps a
> hand-maintained mirror of `trait View`. A missing *required* method fails to
> compile, but a new *defaulted* one would silently not forward; a behavioural
> spy test guards every currently-known method. The design note
> `docs/design/delegation-macros.md` has the full rationale.

For the inverse direction — how a view tree is assembled and walked at runtime —
see [The view tree](../internals/view-tree.md).
