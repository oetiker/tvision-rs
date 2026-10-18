# The draw model → whole-tree redraw + diff

Turbo Vision dates from 1991, when every repainted cell was also a cell written
to a slow terminal. The C++ draw model therefore spends a lot of machinery
*avoiding* repaints: per-cell visibility is computed at write time, and
`drawHide` / `drawShow` / `drawUnderView` / `putInFrontOf` plus the buffered
group (`lock` / `unlock`, `ofBuffered`) all exist to repaint as little of the
screen as possible. This damage tracking is the most intricate part of the
original framework.

tvision-rs drops all of it. In-memory work costs next to nothing by 1991
standards, and the one expensive operation left is writing escape sequences to
the terminal. Drawing is therefore split into two layers with very different
costs.

## Two layers

1. **In-memory redraw (cheap).** Every update cycle the whole view tree is
   painted back-to-front into a [`Buffer`](../api/tvision_rs/screen/struct.Buffer.html),
   the in-memory screen grid. This touches RAM only and takes microseconds, even
   for a full screen.
2. **Terminal flush (diff-bounded).** The freshly painted buffer is compared
   against the previous frame with
   [`Buffer::diff`](../api/tvision_rs/screen/struct.Buffer.html#method.diff), which
   returns only the cells that changed. Only those cells are turned into escape
   sequences and sent to the terminal.

Because the terminal pays only for cells that changed, a full redraw every frame
costs little, and the damage-tracking apparatus is unnecessary.

## What this changes for you

- **Occlusion is the painter's algorithm.** A higher (later-drawn) view
  overwrites a lower one. There is no write-time visibility computation: a view
  paints its content unconditionally, and a covered view is one that a later
  sibling paints over.
- **Z-order changes and window moves need no extra work.** `makeFirst` /
  `putInFrontOf` keep only their reorder role: they shuffle the child order. To
  bring a window forward, move it, or hide it, *mutate the tree and let the next
  frame redraw and diff*. There is nothing to invalidate.
- **No `sfExposed`, no buffered groups, no draw-under calls.** The whole family
  is gone. When a view's appearance depends on state, change the state; the
  redraw picks it up.
- **Clip bounds remain, for correctness only.** A view must not paint
  outside its own bounds. Clipping is never used to minimize writes; the diff
  does that.

## How a view paints

Views never touch the [`Buffer`](../api/tvision_rs/screen/struct.Buffer.html)
directly. They fill a scratch row, a
[`DrawBuffer`](../api/tvision_rs/screen/struct.DrawBuffer.html) (the faithful
successor to `TDrawBuffer`), one display line at a time using
[`Cell`](../api/tvision_rs/screen/struct.Cell.html) values, then blit it into the
draw context. Each `Cell` carries its grapheme and a typed
[`Style`](../api/tvision_rs/color/struct.Style.html) (see
[Palettes & glyphs → Theme/Role](theme.md)) instead of the packed attribute byte
of the original.

The redraw step lives in the single event loop: after handling an event, the
program sets the cursor and then repaints the whole tree into the buffer in
one pass, on every pump cycle, unconditionally. There is no per-view damage
rectangle to consult and no redraw-suppression flag; the diff against the
previous frame keeps a full repaint cheap.

The runtime mechanics of the buffer pair, the diff, and the `Backend` trait that
emits the escape sequences are covered in
[Drawing & backends](../internals/drawing.md). For the summary see
[deviation D8](../reference/deviations.md#d8).

## Draw-on-demand vs whole-tree redraw

In C++ Turbo Vision, `TView::drawView` checked the `sfExposed` flag before
calling `draw`. If the view was not exposed (covered by a higher view or
scrolled off screen), `drawView` returned immediately. The framework tracked
which views were exposed, maintained per-cell visibility information, and called
`drawView` only for views that contributed visible cells.

tvision-rs has no `draw_view`. The whole view tree is repainted unconditionally
on **every pump pass**, back-to-front into the in-memory
[`Buffer`](../api/tvision_rs/screen/struct.Buffer.html). The tree walk is cheap
(microseconds of RAM writes). The terminal pays only for cells that changed,
because `Renderer::render` diffs the newly painted buffer against the previous
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
`drawShow`, and `drawUnderView` have no equivalents. There is no notion of
"this view needs to be redrawn": everything redraws every time, and the diff
handles the rest.

**Practical consequences:**
- A view's `draw` method is called every pump pass, even when nothing visible
  changed. Keep `draw` cheap; it runs on the 20 ms frame cadence.
- There is no `invalidate` or `redraw` call to trigger a repaint. Change the
  state that `draw` reads and the next frame picks it up.
- Covered views still have `draw` called. Later-drawn siblings overwrite their
  cells before the diff sees them.

**Sources:** `Group::draw` (back-to-front tree walk) in `src/view/group.rs`;
`Renderer::render` (paint → diff → emit) in `src/backend/renderer.rs`;
`pump_once` (cursor + render step) in `src/app/program.rs`.

> **Turbo Vision heritage:** C++ `TView::drawView` short-circuited on
> `!exposed`; `sfExposed` / `ofBuffered` were maintained by the occlusion
> tracker. tvision-rs replaces this with whole-tree redraw + buffer diff
> (deviation D8). The `draw` method itself is ported faithfully; only the
> calling convention changes.
