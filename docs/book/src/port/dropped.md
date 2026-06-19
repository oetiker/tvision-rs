# Dropped & changed

Most of the differences in this part *transform* a Turbo Vision concept into an
idiomatic Rust shape. A few instead **drop** machinery outright, or keep a thing
deliberately *unchanged* where you might expect a rewrite. This page collects
those.

## Streaming & persistence — dropped

Turbo Vision shipped its own object-serialization framework: `TStreamable`, the
`ipstream`/`opstream` byte streams, a `TStreamableClass` type registry with
factory functions, and `TResourceFile` resource files built on top. All of it
existed because 1991 C++ had no reflection and no standard serialization — so
Turbo Vision hand-rolled both.

Rust does not need any of that. The port **does not carry the machinery
forward**: there is no `read`/`write`/`build` on views, no `streamableName`, no
class registry. Nothing in the core depends on it, so dropping it cost nothing.

If persistence is ever wanted, the answer is
[`serde`](https://serde.rs/) (plus `typetag` for trait objects) on the
*specific* data worth saving — not a framework-wide streaming layer. Resource
files, if revived, become serde config or embedded assets, scoped to the real
need.

This has one knock-on effect you can see elsewhere in the port: because command
identities no longer need a stable integer for a stream format, they became
namespaced strings instead of small integers — see [Constant families → open
newtypes](constants.md).

> **Looking for `getData`/`setData`?** That is *not* dropped — it became the
> typed value protocol. See [Dialogs & data](../apps/dialogs.md).

## Damage tracking & buffered drawing — dropped

Turbo Vision's `drawHide`/`drawShow`/`drawUnder*` dance and its group-buffered,
occlusion-aware writes were an optimization for slow hardware: avoid touching
cells you do not have to. The port replaces that whole scheme with **whole-tree
redraw plus a back-buffer diff** — draw everything into a buffer, compare it to
the last frame, and emit only the cells that actually changed. The optimization
moves from per-write bookkeeping to one diff at the end.

You never call `lock`/`unlock` or reason about what is occluded; you just draw.
The mechanics are in [Drawing & backends](../internals/drawing.md); the
rationale is [The draw model](draw.md).

## Coordinates stay `i32` — deliberately *unchanged*

magiblot's coordinates are plain C++ `int`. The port keeps them as **`i32`**,
not `usize` or `u16`. This is a faithfulness decision, not an oversight.

Turbo Vision routinely computes *negative* and off-screen coordinates — a view
scrolled partly above its owner, a delta that goes negative mid-calculation, a
rectangle intersected down to nothing. An unsigned type would underflow on
exactly the arithmetic Turbo Vision does every frame. Keeping the signed width
means the geometry math in [`Point`](../api/tvision_rs/view/struct.Point.html) and
[`Rect`](../api/tvision_rs/view/struct.Rect.html) ports line-for-line from the C++
and behaves identically at the edges.

So when you see `i32` on a bounds calculation and reach for "shouldn't that be
`usize`?" — no. The signedness is load-bearing.

---

The at-a-glance list of every difference lives in
[Differences from C++ Turbo Vision](../reference/deviations.md), and the terse
C++→Rust name lookup is the [symbol map](../reference/symbol-map.md).
