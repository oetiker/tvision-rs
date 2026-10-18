# Palettes & glyphs → Theme/Role

In C++ Turbo Vision, a view's colors are not stored in one place. Each class
exposes a `getPalette()` that returns a *length-prefixed string of byte indices*,
and `getColor(idx)` walks the owner chain: a control's palette indexes into
its window's palette, which indexes into the application palette, whose final
byte is a BIOS attribute (`bg << 4 | fg`). Drawing glyphs (frame corners,
scrollbar arrows, check marks, shadows) are character literals scattered through
the widget source. Finding the color of a focused button's shortcut letter
means tracing four palette hops by hand.

tvision-rs replaces that machinery with one typed object.

## One `Theme`, two halves

A [`Theme`](../api/tvision_rs/theme/struct.Theme.html) owns both ends of the old
scheme:

- a flat map from a semantic [`Role`](../api/tvision_rs/theme/enum.Role.html) to a
  [`Style`](../api/tvision_rs/color/struct.Style.html) (a foreground/background
  [`Color`](../api/tvision_rs/color/enum.Color.html) pair), and
- a [`Glyphs`](../api/tvision_rs/theme/struct.Glyphs.html) holder for every drawing
  character the framework uses.

A view never walks a palette chain. It asks the theme for the role it wants:

```rust
# use tvision_rs as tv;
# use tv::Role;
# fn _demo(ctx: &tv::DrawCtx) {
let _style = ctx.style(Role::FrameActive);
let _corner = ctx.glyphs().frame_tl; // ┌
# }
```

## `Role` is the palette index, named

Where the C++ said *"the high nibble of `cpButton` slot 6, two owner hops down"*,
tvision-rs says [`Role::ButtonDefaultShortcut`](../api/tvision_rs/theme/enum.Role.html).
Each `getColor` call site in the original maps to one named `Role` here,
and the *state → role* decision is made once, in the code that draws the widget,
instead of being implicit in a byte string. The enum covers the state matrices
the widgets need: active / passive / dragging frames (plus the gray- and
cyan-scheme variants), the normal / focused / disabled / pressed quartet, the
list-item matrix, the cluster, button, label, input-line, menu and status-line
families, and an error / warning / info / success feedback set.

`Role` is a **closed, first-party enum**, unlike the open newtype used
for [`Command`](constants.md). The reason is extensibility: a `Command`
crosses the app↔framework boundary and is dispatched by code that never saw it,
so it needs open runtime identity. A `Role` is always resolved at draw time by
the code that owns it, so even a custom widget knows its roles at compile time.
The closed enum allows a fixed `[Style; N]` array with a total, panic-free
lookup, and the compiler checks exhaustiveness while roles change during the
port.

## The default theme *is* the classic blue look

[`Theme::classic_blue()`](../api/tvision_rs/theme/struct.Theme.html#method.classic_blue)
(also the `Default`) reproduces the canonical Turbo Vision palette. Every entry
is derived from the literal C++ palette chain, and each line in the source
carries an inline comment tracing the original hops, for example
`cpFrame[3] → cpBlueWindow[2] → cpAppColor[9]`. The result is stored as one
flat `Role → Style` table. To stay legible on any terminal, the stored colors are
pinned to canonical true-color RGB (via `Color::bios_rgb`) instead of the
terminal's own BIOS palette.

Because a theme is data, recoloring the whole app is one call:
[`set_style(role, style)`](../api/tvision_rs/theme/struct.Theme.html#method.set_style)
replaces any role's color, and
[`style(role)`](../api/tvision_rs/theme/struct.Theme.html#method.style) reads it
back. See the [Theming & colors](../apps/theming.md) recipe for doing this in a
running app, and the [drawing chapter](../internals/drawing.md) for how a `Style`
reaches the screen.

## Glyphs, not literals

The CP437 box-drawing and marker characters that magiblot's `tvtext1.cpp` seeds
live as named fields on [`Glyphs`](../api/tvision_rs/theme/struct.Glyphs.html):
single- and double-line frame pieces (`frame_tl`, `frame_h_d`, …), scrollbar
arrows and thumb, button-shadow blocks, input-line scroll arrows, and the
composite frame-icon strings such as the close box `"[~■~]"`. Defaults match the
classic character set; a theme can swap them without touching any widget code.
