# The draw model → whole-tree redraw + diff

Turbo Vision was born in 1991, when every cell you repainted was also a cell you
wrote to a slow terminal. So the C++ draw model spends a great deal of machinery
*avoiding* repaints: per-cell visibility is computed at write time, and
`drawHide` / `drawShow` / `drawUnderView` / `putInFrontOf` plus the buffered
group (`lock` / `unlock`, `ofBuffered`) all exist to repaint as little of the
screen as possible. That is damage tracking, and it is the single most intricate
part of the original framework.

tvision-rs drops all of it. The reasoning: in-memory work is
free by 1991 standards, and the *only* expensive operation left is writing escape
sequences to the terminal. So we split drawing into two layers with very
different costs.

## Two layers

1. **In-memory redraw (cheap).** Every update cycle the whole view tree is
   painted back-to-front into a [`Buffer`](../api/tvision_rs/screen/struct.Buffer.html),
   the in-memory screen grid. This is RAM only — microseconds, even for a full
   screen.
2. **Terminal flush (diff-bounded).** The freshly-painted buffer is compared
   against the previous frame with
   [`Buffer::diff`](../api/tvision_rs/screen/struct.Buffer.html#method.diff), which
   returns only the cells that changed. Just those cells are turned into escape
   sequences and sent to the terminal.

Because the terminal only ever pays for *real* change, a full redraw every frame
is effectively free — and the entire damage-tracking apparatus becomes
unnecessary.

## What this changes for you

- **Occlusion is just the painter's algorithm.** A higher (later-drawn) view
  overwrites a lower one. There is no write-time visibility computation: a view
  paints its content unconditionally, and being covered simply means a later
  sibling paints over it.
- **Z-order changes and window moves are trivial.** `makeFirst` / `putInFrontOf`
  keep only their reorder role — they shuffle the child order. To bring a window
  forward, move it, or hide it, you *mutate the tree and let the next frame
  redraw and diff*. There is nothing to invalidate.
- **No `sfExposed`, no buffered groups, no draw-under calls.** The whole family
  is gone. If a view's appearance depends on state, change the state; the redraw
  picks it up.
- **Clip bounds remain — but only for correctness.** A view must not paint
  outside its own bounds. Clipping is never used to minimize writes; that job
  belongs entirely to the diff.

## How a view paints

Views never touch the [`Buffer`](../api/tvision_rs/screen/struct.Buffer.html)
directly. They fill a scratch row — a
[`DrawBuffer`](../api/tvision_rs/screen/struct.DrawBuffer.html), the faithful
successor to `TDrawBuffer` — one display line at a time using
[`Cell`](../api/tvision_rs/screen/struct.Cell.html) values, then blit it into the
draw context. Each `Cell` carries its grapheme and a typed
[`Style`](../api/tvision_rs/color/struct.Style.html) (see
[Palettes & glyphs → Theme/Role](theme.md)) instead of the packed attribute byte
of the original.

The redraw step itself lives in the single event loop: after handling an event,
the program sets the cursor and then repaints the whole tree into the buffer in
one pass — every pump cycle, unconditionally. There is no per-view damage
rectangle to consult and no redraw-suppression flag; the diff against the
previous frame is what keeps a full repaint cheap.

The runtime mechanics of the buffer pair, the diff, and the `Backend` trait that
emits the escape sequences are covered in
[Drawing & backends](../internals/drawing.md). For the at-a-glance summary see
[deviation D8](../reference/deviations.md#d8).

## Draw-on-demand vs whole-tree redraw

In C++ Turbo Vision, `TView::drawView` consulted the `sfExposed` flag before
calling `draw`. If the view was not exposed — covered by a higher view or
scrolled off screen — `drawView` returned immediately. The framework worked hard
to know which views were exposed, maintained per-cell visibility information,
and called `drawView` only for views that actually contributed pixels.

tvision-rs has no `draw_view`. The whole view tree is repainted unconditionally
on **every pump pass**, back-to-front into the in-memory
[`Buffer`](../api/tvision_rs/screen/struct.Buffer.html). The tree walk is cheap
(microseconds of RAM writes). The terminal only pays for cells that changed,
because `Renderer::render` diffs the newly-painted buffer against the previous
frame and emits escape sequences only for the diffed cells:

```rust,ignore
// src/app/program.rs — end of pump_once (simplified)
renderer.set_cursor(cursor);
renderer.render(|buf| {
    // Paint the ENTIRE tree into buf, unconditionally.
    let bounds = group.state().get_bounds();
    let mut dc = DrawCtx::new(buf, theme, bounds, bounds.a);
    group.draw(&mut dc);
    // Renderer::render then diffs buf against the previous frame
    // and emits only the changed cells.
});
```

The `sfExposed` flag, `ofBuffered`, `lock`/`unlock`, `drawHide`,
`drawShow`, and `drawUnderView` have no equivalents. There is no concept of
"this view needs to be redrawn" — everything always redraws, and the diff
handles the rest.

**Practical consequences:**
- A view's `draw` method is called every pump pass, even when nothing visible
  changed. Keep `draw` cheap; it runs on the 20 ms frame cadence.
- There is no `invalidate` or `redraw` call to trigger a repaint. Change the
  state that `draw` reads and the next frame picks it up automatically.
- Covered views still have `draw` called. Their cells are simply overwritten by
  later-drawn siblings before the diff sees them.

**Sources:** `Group::draw` (back-to-front tree walk) in `src/view/group.rs`;
`Renderer::render` (paint → diff → emit) in `src/backend/renderer.rs`;
`pump_once` (cursor + render step) in `src/app/program.rs`.

> **Turbo Vision heritage:** C++ `TView::drawView` short-circuited on
> `!exposed`; `sfExposed` / `ofBuffered` were maintained by the occlusion
> tracker. tvision-rs replaces this with whole-tree redraw + buffer diff
> (deviation D8). The `draw` method itself is ported faithfully; only the
> calling convention changes.
