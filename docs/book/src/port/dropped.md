# Dropped & changed

Most of the differences in this part *transform* a Turbo Vision concept into an
idiomatic Rust shape. A few **drop** machinery outright, or keep something
*unchanged* where a rewrite might be expected. This page collects those.

## Streaming & persistence: dropped

Turbo Vision shipped its own object-serialization framework: `TStreamable`, the
`ipstream`/`opstream` byte streams, a `TStreamableClass` type registry with
factory functions, and `TResourceFile` resource files built on top. All of it
existed because 1991 C++ had no reflection and no standard serialization, so
Turbo Vision built both itself.

The port **does not carry the machinery forward**: there is no
`read`/`write`/`build` on views, no `streamableName`, no class registry. Nothing
in the core depends on it, so dropping it cost nothing.

If persistence is wanted later, the plan is
[`serde`](https://serde.rs/) (plus `typetag` for trait objects) on the
*specific* data worth saving, without a framework-wide streaming layer. Resource
files, if revived, become serde config or embedded assets, scoped to the actual
need.

This has one visible effect elsewhere in the port: because command
identities no longer need a stable integer for a stream format, they are
namespaced strings instead of small integers; see [Constant families → open
newtypes](constants.md).

> **Looking for `getData`/`setData`?** That is *not* dropped. It became the
> typed value protocol. See [Dialogs & data](../apps/dialogs.md).

## Damage tracking & buffered drawing: dropped

Turbo Vision's `drawHide`/`drawShow`/`drawUnder*` calls and its group-buffered,
occlusion-aware writes were an optimization for slow hardware: avoid touching
cells that need no change. The port replaces that scheme with **whole-tree
redraw plus a back-buffer diff**: draw everything into a buffer, compare it to
the last frame, and emit only the cells that changed. The optimization
moves from per-write bookkeeping to one diff at the end.

Views never call `lock`/`unlock` or reason about what is occluded; they draw.
The mechanics are in [Drawing & backends](../internals/drawing.md); the
rationale is [The draw model](draw.md).

## `TColorDialog`: replaced by `ColorPicker`

Turbo Vision's `TColorDialog`, with its `TColorSelector`, `TMonoSelector`,
`TColorDisplay`, `TColorGroupList` and `TColorItemList` helpers, edited the
16-entry BIOS palette in place. tvision-rs has no such palette: colors live in
a role-based [`Theme`](../api/tvision_rs/theme/struct.Theme.html) and can be
truecolor, so the cluster is not ported.

Its place is taken by
[`ColorPicker`](../api/tvision_rs/dialog/struct.ColorPicker.html), a truecolor
picker view, and
[`Program::color_dialog`](../api/tvision_rs/app/struct.Program.html#method.color_dialog),
which runs it as a modal and returns the chosen color. Editing a whole theme
goes through
[`Program::theme_editor`](../api/tvision_rs/app/struct.Program.html#method.theme_editor).
Both are listed on [Extensions beyond Turbo Vision](extensions.md).

## Coordinates stay `i32` (*unchanged*)

magiblot's coordinates are plain C++ `int`. The port keeps them as **`i32`**
instead of `usize` or `u16`, for faithfulness.

Turbo Vision routinely computes *negative* and off-screen coordinates: a view
scrolled partly above its owner, a delta that goes negative mid-calculation, a
rectangle intersected down to nothing. An unsigned type would underflow on
the arithmetic Turbo Vision does every frame. With the signed width, the
geometry math in [`Point`](../api/tvision_rs/view/struct.Point.html) and
[`Rect`](../api/tvision_rs/view/struct.Rect.html) ports line-for-line from the C++
and behaves identically at the edges.

An `i32` on a bounds calculation is intentional; changing it to `usize` would
break the negative cases above.

---

The list of every difference lives in
[Differences from C++ Turbo Vision](../reference/deviations.md), and the
C++→Rust name lookup is the [symbol map](../reference/symbol-map.md).
