# Theming & colors

Turbo Vision never let a widget hard-code its colors. Every `draw()` reached
through a chain of *palette* strings — the button asked its owner, the owner
asked *its* owner, all the way up to the application's master palette — and the
chain finally landed on a one-byte BIOS attribute. Re-theming the whole app was
a matter of swapping that master palette.

tvision keeps the *idea* — colors come from a central, swappable table — but
drops the index arithmetic. The palette chain collapses into a single typed map
from a semantic [`Role`](../api/tvision/theme/enum.Role.html) to a
[`Style`](../api/tvision/color/struct.Style.html). This is deviation **D7**; the
narrative behind it is in [Palettes & glyphs](../port/theme.md).

## The three types

| Type | What it is |
| ---- | ---------- |
| [`Color`](../api/tvision/color/enum.Color.html) | A desired color: terminal `Default`, 4-bit `Bios`, 256-color `Indexed`, or 24-bit `Rgb`. |
| [`Style`](../api/tvision/color/struct.Style.html) | A foreground `Color`, a background `Color`, and a [`Modifiers`](../api/tvision/color/struct.Modifiers.html) struct-of-bools (bold, italic, underline, blink, reverse, strike). |
| [`Theme`](../api/tvision/theme/struct.Theme.html) | The whole palette: a `Role → Style` map plus a [`Glyphs`](../api/tvision/theme/struct.Glyphs.html) holder for the box-drawing and marker characters. |

A `Role` is a *semantic* slot — `FrameActive`, `ButtonDefault`, `MenuSelected`,
`Error` — not a color. Widgets ask for roles, never for raw colors, so a single
theme drives every control consistently. The enum is closed and first-party: it
grows as new widgets are ported, but applications do not add their own roles.

## The default theme

The framework starts on
[`Theme::classic_blue`](../api/tvision/theme/struct.Theme.html#method.classic_blue) —
the canonical Turbo Vision blue look, and the value behind
[`Theme::default`](../api/tvision/theme/struct.Theme.html#method.default). Each
role is derived directly from the historic C++ palette chain (the source carries
the full derivation inline), but the colors are pinned to definite true-color
RGB via [`Color::bios_rgb`](../api/tvision/color/enum.Color.html#method.bios_rgb)
so contrast is correct no matter how the terminal has remapped its own 16-color
palette.

## Reading a theme color

If you are writing a custom `View`, you do not touch the `Theme` directly — the
draw context hands you the resolved style. Inside `draw()` you have a
[`DrawCtx`](../api/tvision/view/struct.DrawCtx.html); ask it for the style of a
role and paint with it:

```rust,ignore
fn draw(&self, ctx: &mut tv::DrawCtx) {
    let style = ctx.style(tv::Role::Normal);
    ctx.put_str(0, 0, "hello", style);
}
```

`ctx.style(role)` is a thin pass-through to `Theme::style`, so it is total and
never panics — every role always resolves. Box-drawing and marker characters
come from `ctx.glyphs()` the same way. See
[Writing your own View](../internals/custom-view.md) for the full draw path.

## Overriding colors

Re-theming is a whole-theme swap, faithful to how C++ replaced the master
palette. Take the default, override the roles you care about with
[`set_style`](../api/tvision/theme/struct.Theme.html#method.set_style), and
install it on the running program with
[`Program::set_theme`](../api/tvision/app/struct.Program.html#method.set_theme),
which forces a full repaint:

```rust,ignore
let mut theme = tv::Theme::classic_blue();
theme.set_style(
    tv::Role::Background,
    tv::Style::new(tv::Color::Rgb(20, 20, 30), tv::Color::Default),
);
program.set_theme(theme);
```

You can also build a `Style` with attributes via
[`Style::with_modifiers`](../api/tvision/color/struct.Style.html#method.with_modifiers),
or flip foreground and background with
[`reversed`](../api/tvision/color/struct.Style.html#method.reversed) — which
swaps concrete colors but toggles the `reverse` flag when one side is
`Default`, matching Turbo Vision's `reverseAttribute`.

For an interactive way to tweak roles at runtime, the program ships a built-in
theme-editor dialog
([`Program::theme_editor`](../api/tvision/app/struct.Program.html#method.theme_editor)):
it opens an editor seeded with the current theme and, on **OK**, installs the
result through `set_theme`.
