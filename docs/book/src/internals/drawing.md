# Drawing & backends

A `tvision` app never writes to the terminal directly. Every view paints into an
in-memory grid; once per frame the framework diffs that grid against the previous
one and pushes *only the changed cells* to a pluggable backend. This is the
[whole-tree-redraw-plus-diff draw model](../port/draw.md) — there is no
damage-tracking, no per-view "dirty" bookkeeping. A view's only job is to fill in
its cells; keeping terminal I/O bounded is the diff's job.

## The cell and the draw buffer

The atom is a [`Cell`](../api/tvision/screen/struct.Cell.html): one screen
position holding a grapheme cluster (its text), a colour
[`Style`](../api/tvision/color/struct.Style.html), and two flags marking the lead
and trailing halves of a double-width glyph. It is the Rust form of magiblot's
`TScreenCell`.

A view does not poke cells one at a time. It fills a
[`DrawBuffer`](../api/tvision/screen/struct.DrawBuffer.html) — a single fixed-width
*row* of cells under construction, the port of `TDrawBuffer` — then blits that
row. Writes past the buffer's width are clipped, exactly as the C++ clamps against
its `capacity`. Text goes in through width-aware primitives, so truncation and
double-width handling are shared with the rest of the renderer.

```rust,ignore
// In a view's draw(): build one line, then write it out.
let mut b = DrawBuffer::new(width);
b.move_str(0, "Hello", style);
// ... hand `b` to the framework to blit at row y ...
```

> The C++ `0 = retain` sentinel in `moveChar` is gone: in the typed model
> `move_char` always writes both char and style. To touch only one, use
> `put_char` / `put_attribute`.

## The back buffer and the diff

The full view tree is painted into a
[`Buffer`](../api/tvision/screen/struct.Buffer.html) — the screen-sized grid,
always rooted at `(0, 0)`. Each frame the renderer keeps **two** buffers: the
*back* buffer (painted this frame) and the *front* buffer (last frame, the diff
reference). [`Buffer::diff`](../api/tvision/screen/struct.Buffer.html#method.diff)
walks the two grids and returns just the cells that changed, with double-width
lead/trail cells handled correctly. The algorithm is adapted from ratatui, minus
its `skip` opt-out — there is nothing to skip when you repaint everything.

## The Renderer cycle

The [`Renderer`](../api/tvision/backend/struct.Renderer.html) owns the back/front
buffer pair and a boxed backend, and runs one frame per call to
[`render`](../api/tvision/backend/struct.Renderer.html#method.render):

1. **Reset** the back buffer to blank.
2. **Paint** the whole view tree into it.
3. **Diff** it against the front buffer.
4. **Draw** the changed cells to the backend.
5. **Set cursor** to the focused position (or hide it).
6. **Flush**.
7. **Swap** back and front so this frame becomes next frame's reference.

This runs at the end of every [event-loop pump](event-loop.md).

## The Backend trait

The terminal seam is the [`Backend`](../api/tvision/backend/trait.Backend.html)
trait ([deviation D11](../reference/deviations.md)). The app holds a
`Box<dyn Backend>`, so the trait is **object-safe** and the view tree never
carries a backend type parameter. Its surface is small: report `size`, `draw` a
slice of changed cells, `flush`, `set_cursor`, `poll_event`, plus clipboard and
suspend/resume hooks.

Two implementations ship:

| Backend | Role |
| ------- | ---- |
| [`CrosstermBackend`](../api/tvision/backend/struct.CrosstermBackend.html) | Production. Wraps crossterm; sets up raw mode, the alternate screen, and mouse capture, and restores the terminal on `Drop`. |
| [`HeadlessBackend`](../api/tvision/backend/struct.HeadlessBackend.html) | Tests. An in-memory grid that never blocks: `poll_event` pops a queued event or returns immediately, so tests drive the loop deterministically. |

The headless backend is the verification backbone of the whole port: a widget is
rendered onto it and its grid is compared, via the frozen `screen::snapshot`
format, against a golden string.

## Colour depth

Terminals vary in colour capability, so the backend maps each `Style`'s colours
down a quantization ladder selected by
[`ColorDepth`](../api/tvision/backend/enum.ColorDepth.html): `TrueColor` passes
24-bit RGB through unchanged, `Xterm256` quantizes RGB to the nearest palette
entry, `Ansi16` reduces everything to the 16-colour set, and `NoColor` drops
colour entirely. The ladder itself (`RGB → xterm-256 → xterm-16 → BIOS`) is pure,
I/O-free maths; see the [Theme/Role port note](../port/theme.md) for how the typed
theme feeds it.
