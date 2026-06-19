# Drawing & backends

An tvision-rs app never writes to the terminal directly. Every view paints into an
in-memory grid; once per frame the framework diffs that grid against the previous
one and pushes *only the changed cells* to a pluggable backend. This is the
[whole-tree-redraw-plus-diff draw model](../port/draw.md) — there is no
damage-tracking, no per-view "dirty" bookkeeping. A view's only job is to fill in
its cells; keeping terminal I/O bounded is the diff's job.

## The cell and the draw buffer

The atom is a [`Cell`](../api/tvision_rs/screen/struct.Cell.html): one screen
position holding a grapheme cluster (its text), a colour
[`Style`](../api/tvision_rs/color/struct.Style.html), and two flags marking the lead
and trailing halves of a double-width glyph. It is the Rust form of magiblot's
`TScreenCell`.

A view does not poke cells one at a time. It fills a
[`DrawBuffer`](../api/tvision_rs/screen/struct.DrawBuffer.html) — a single fixed-width
*row* of cells under construction — then blits that row. Writes past the buffer's
width are clipped automatically. Text goes in through width-aware primitives, so
truncation and double-width handling are shared with the rest of the renderer.

```rust
# use tvision_rs as tv;
# use tv::{DrawBuffer, Style};
# fn _demo(style: Style) {
# let width = 10usize;
// In a view's draw(): build one line, then write it out.
let mut b = DrawBuffer::new(width);
b.move_str(0, "Hello", style);
// ... hand `b` to the framework to blit at row y ...
# }
```

> In the typed cell model, `move_char` always writes both char and style — there
> is no retain-sentinel. To update only the character or only the style, use
> `put_char` / `put_attribute`.

## The back buffer and the diff

The full view tree is painted into a
[`Buffer`](../api/tvision_rs/screen/struct.Buffer.html) — the screen-sized grid,
always rooted at `(0, 0)`. Each frame the renderer keeps **two** buffers: the
*back* buffer (painted this frame) and the *front* buffer (last frame, the diff
reference). [`Buffer::diff`](../api/tvision_rs/screen/struct.Buffer.html#method.diff)
walks the two grids and returns just the cells that changed, with double-width
lead/trail cells handled correctly. The algorithm is adapted from ratatui, minus
its `skip` opt-out — there is nothing to skip when you repaint everything.

## The Renderer cycle

The [`Renderer`](../api/tvision_rs/backend/struct.Renderer.html) owns the back/front
buffer pair and a boxed backend, and runs one frame per call to
[`render`](../api/tvision_rs/backend/struct.Renderer.html#method.render):

1. **Reset** the back buffer to blank.
2. **Paint** the whole view tree into it.
3. **Diff** it against the front buffer.
4. **Draw** the changed cells to the backend.
5. **Set cursor** to the focused position (or hide it).
6. **Flush**.
7. **Swap** back and front so this frame becomes next frame's reference.

This runs at the end of every [event-loop pump](event-loop.md).

## The Backend trait

The terminal seam is the [`Backend`](../api/tvision_rs/backend/trait.Backend.html)
trait ([deviation D11](../reference/deviations.md#d11)). The app holds a
`Box<dyn Backend>`, so the trait is **object-safe** and the view tree never
carries a backend type parameter. Its surface is small: report `size`, `draw` a
slice of changed cells, `flush`, `set_cursor`, `poll_event`, plus clipboard and
suspend/resume hooks.

Two implementations ship:

| Backend | Role |
| ------- | ---- |
| [`CrosstermBackend`](../api/tvision_rs/backend/struct.CrosstermBackend.html) | Production. Wraps crossterm; sets up raw mode, the alternate screen, and mouse capture, and restores the terminal on `Drop`. |
| [`HeadlessBackend`](../api/tvision_rs/backend/struct.HeadlessBackend.html) | Tests. An in-memory grid that never blocks: `poll_event` pops a queued event or returns immediately, so tests drive the loop deterministically. |

The headless backend is the verification backbone of the whole port: a widget is
rendered onto it and its grid is compared, via the frozen `screen::snapshot`
format, against a golden string.

## Clipping to owner bounds

Every view paints through a
[`DrawCtx`](../api/tvision_rs/view/struct.DrawCtx.html) — the clipped, themed
writer handed down from its parent group. The key property is the clip rect:
a `DrawCtx` carries an **absolute** clip rectangle already intersected with
its parent's clip rectangle, and every cell write is silently discarded if it
falls outside that rect. A view can never paint outside the bounds its owner
carved out for it, regardless of what coordinates it writes to.

The clip is set up automatically when a group descends into a child's `draw`:

```rust,ignore
// src/view/group.rs — Group::draw (simplified)
for child in self.children.iter_mut().filter(|c| c.view.state().state.visible) {
    let bounds = child.view.state().get_bounds();
    // `ctx.sub(bounds)` intersects the parent's clip with the child's bounds
    // and creates a DrawCtx whose origin is at the child's top-left corner.
    let mut sub = ctx.sub(bounds);
    child.view.draw(&mut sub);
}
```

`DrawCtx::sub` computes `child_clip = parent_clip ∩ child_bounds_absolute`.
A child that extends beyond its owner's edge (or is partially scrolled off
screen) receives a sub with a narrowed clip; it still calls its own draw code
unchanged, but the writes that land outside the clip are dropped in the buffer
writer.

Because clips are intersected at each level of the tree, a deeply nested view
automatically inherits the intersection of all its ancestors' clips. There is
no separate `getClipRect` call before drawing — the `DrawCtx` already carries
the correct clip.

```rust,ignore
// src/view/context.rs — DrawCtx::sub (verbatim logic)
pub fn sub(&mut self, area_local: Rect) -> DrawCtx<'_> {
    let mut abs = area_local;
    abs.r#move(self.origin.x, self.origin.y);  // child's absolute position
    let mut clip = self.clip;
    clip.intersect(&abs);                        // narrow to owner's clip
    DrawCtx {
        buffer: &mut *self.buffer,
        clip,                                    // already intersected
        origin: self.origin + area_local.a,
        theme: self.theme,
    }
}
```

This is the `getClipRect` successor: in C++ a view called `getClipRect` before
writing to avoid painting into occluded regions. In tvision-rs there is nothing
to call — the clip comes down with the `DrawCtx`, and the draw code never needs
to query or adjust it. It is used only for correctness (a view must not paint
outside its bounds), never to minimize writes (that is the diff's job).

**Sources:** `DrawCtx::sub` and `DrawCtx::clip` in `src/view/context.rs`;
`Group::draw` in `src/view/group.rs`.

> **Turbo Vision heritage:** `TView::getClipRect` (`tview.cpp`) returned the
> intersection of the view's bounds with the visible area; views checked it
> before writing. In tvision-rs the intersected clip is pre-computed and
> carried by `DrawCtx`; views write unconditionally and the clip silently
> discards out-of-bounds cells.

## Colour depth

Terminals vary in colour capability, so the backend maps each `Style`'s colours
down a quantization ladder selected by
[`ColorDepth`](../api/tvision_rs/backend/enum.ColorDepth.html): `TrueColor` passes
24-bit RGB through unchanged, `Xterm256` quantizes RGB to the nearest palette
entry, `Ansi16` reduces everything to the 16-colour set, and `NoColor` drops
colour entirely. The ladder itself (`RGB → xterm-256 → xterm-16 → BIOS`) is pure,
I/O-free maths; see the [Theme/Role port note](../port/theme.md) for how the typed
theme feeds it.
