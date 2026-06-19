# Theming & colors

In tvision-rs, colors come from a single central, swappable table: a typed map from a
semantic [`Role`](../api/tvision_rs/theme/enum.Role.html) to a
[`Style`](../api/tvision_rs/color/struct.Style.html). No widget ever hard-codes a
color; every `draw()` asks for a role, and the active theme resolves it. Swapping
the whole palette is a one-call operation. The narrative behind the design is in
[Palettes & glyphs](../port/theme.md).

> **Turbo Vision heritage:** the C++ framework used a chain of per-widget palette
> index strings that bottomed out in one-byte BIOS attributes. tvision-rs keeps the
> central-swappable-table idea but replaces the index arithmetic with a typed
> `Role → Style` map, eliminating the palette-chain traversal entirely.

## The three types

| Type | What it is |
| ---- | ---------- |
| [`Color`](../api/tvision_rs/color/enum.Color.html) | A desired color: terminal `Default`, 4-bit `Bios`, 256-color `Indexed`, or 24-bit `Rgb`. |
| [`Style`](../api/tvision_rs/color/struct.Style.html) | A foreground `Color`, a background `Color`, and a [`Modifiers`](../api/tvision_rs/color/struct.Modifiers.html) struct-of-bools (bold, italic, underline, blink, reverse, strike). |
| [`Theme`](../api/tvision_rs/theme/struct.Theme.html) | The whole palette: a `Role → Style` map plus a [`Glyphs`](../api/tvision_rs/theme/struct.Glyphs.html) holder for the box-drawing and marker characters. |

A `Role` is a *semantic* slot — `FrameActive`, `ButtonDefault`, `MenuSelected`,
`Error` — not a color. Widgets ask for roles, never for raw colors, so a single
theme drives every control consistently. The enum is closed and first-party: it
grows as new widgets are ported, but applications do not add their own roles.

## The default theme

The framework starts on
[`Theme::classic_blue`](../api/tvision_rs/theme/struct.Theme.html#method.classic_blue) —
the canonical Turbo Vision blue look, and the value behind
[`Theme::default`](../api/tvision_rs/theme/struct.Theme.html#method.default). Each
role resolves to a definite true-color RGB via
[`Color::bios_rgb`](../api/tvision_rs/color/enum.Color.html#method.bios_rgb),
so contrast is correct regardless of how the terminal has remapped its own
16-color palette. The source carries the full per-role derivation inline
*(each value is anchored to the corresponding entry in the classic C++ palette
chain)*.

## Reading a theme color

If you are writing a custom `View`, you do not touch the `Theme` directly — the
draw context hands you the resolved style. Inside `draw()` you have a
[`DrawCtx`](../api/tvision_rs/view/struct.DrawCtx.html); ask it for the style of a
role and paint with it:

```rust
# use tvision_rs as tv;
# struct MyWidget;
# impl MyWidget {
fn draw(&self, ctx: &mut tv::DrawCtx) {
    let style = ctx.style(tv::Role::Normal);
    ctx.put_str(0, 0, "hello", style);
}
# }
```

`ctx.style(role)` is a thin pass-through to `Theme::style`, so it is total and
never panics — every role always resolves. Box-drawing and marker characters
come from `ctx.glyphs()` the same way. See
[Writing your own View](../internals/custom-view.md) for the full draw path.

## Overriding colors

Re-theming is a whole-theme swap. Take the default, override the roles you care
about with
[`set_style`](../api/tvision_rs/theme/struct.Theme.html#method.set_style), and
install it on the running program with
[`Program::set_theme`](../api/tvision_rs/app/struct.Program.html#method.set_theme),
which forces a full repaint:

```rust
# use tvision_rs as tv;
# fn _demo(program: &mut tv::Program) {
let mut theme = tv::Theme::classic_blue();
theme.set_style(
    tv::Role::Background,
    tv::Style::new(tv::Color::Rgb(20, 20, 30), tv::Color::Default),
);
program.set_theme(theme);
# }
```

You can also build a `Style` with attributes via
[`Style::with_modifiers`](../api/tvision_rs/color/struct.Style.html#method.with_modifiers),
or flip foreground and background with
[`reversed`](../api/tvision_rs/color/struct.Style.html#method.reversed) — which
swaps concrete colors but toggles the `reverse` flag when one side is
`Default`, matching Turbo Vision's `reverseAttribute`.

For an interactive way to tweak roles at runtime, the program ships a built-in
theme-editor dialog
([`Program::theme_editor`](../api/tvision_rs/app/struct.Program.html#method.theme_editor)):
it opens an editor seeded with the current theme and, on **OK**, installs the
result through `set_theme`.

## See also

- [Writing your own View — colors](../internals/custom-view.md#a-custom-views-colors) —
  how to pick a `Role` for a custom view and what the role map looks like.
