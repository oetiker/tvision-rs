# Flag words → struct-of-bools

In C++ Turbo Vision, a view's runtime condition is packed into `ushort` bit
words: `state` holds the `sf*` flags, `options` the `of*` flags, plus
`growMode` (`gf*`), `dragMode` (`dm*`) and `eventMask`. Code sets a bit with
`state |= sfFocused` and tests one with `if (state & sfFocused)`. This is
compact but untyped: the compiler cannot tell that `sfFocused` belongs to
`state` and not to `options`.

Memory is no longer that scarce, so tvision-rs turns each flag word into a plain
struct of named `bool` fields. Each bit becomes a field:

```rust
# use tvision_rs as tv;
# use tv::View;
# fn _demo(view: &dyn tv::View) {
if view.state().state.focused { /* ... */ }   // was: state & sfFocused
# }
```

## The four families

Each `sf*`/`of*`/`gf*`/`dm*` constant maps to one field, and each word maps to
one `#[derive(Default)]` struct. The field's *name* documents the bit that the
old `0x0001`-style constant left to a comment:

| C++ word   | tvision-rs type                                                | Example                        |
| ---------- | -------------------------------------------------------- | ------------------------------ |
| `state`    | [`State`](../api/tvision_rs/view/struct.State.html)         | `sfFocused` → `focused`        |
| `options`  | [`Options`](../api/tvision_rs/view/struct.Options.html)     | `ofSelectable` → `selectable`  |
| `growMode` | [`GrowMode`](../api/tvision_rs/view/struct.GrowMode.html)   | `gfGrowAll` → `grow_all()`     |
| `dragMode` | [`DragMode`](../api/tvision_rs/view/struct.DragMode.html)   | `dmLimitLoY` → `limit_lo_y`    |

A combined constant such as `gfGrowAll` (four `gf*` bits OR'd together) becomes
a constructor, `GrowMode::grow_all()`, instead of a single field, since it
was never a single bit.

A few flags fell away with their reason for existing: `sfExposed` and
`ofBuffered` were caches for partial-repaint occlusion, and tvision-rs redraws
the whole tree and diffs it (see [Drawing & backends](../internals/drawing.md)),
so there is nothing to cache.

## Reading vs. flipping

A read is field access on the snapshot returned by `state()` /
`options()`. *Flipping* a flag is where Turbo Vision's `setState(flag, on)`
mattered: besides toggling a bit, it fired side effects such as redrawing,
broadcasting a focus change, and cascading into children. tvision-rs keeps that verb
where the side effects live, as
[`View::set_state`](../api/tvision_rs/view/trait.View.html#method.set_state) over a
small [`StateFlag`](../api/tvision_rs/view/enum.StateFlag.html) enum: the named
subset of `sf*` (`Active`, `Selected`, `Focused`, `Dragging`) that the focus and
activation machinery propagates. Flags with no propagation (visibility, cursor
shape) are set directly on the struct, never through this hook.

Most application code touches neither directly. It uses the verbs that
wrap them: `view.show()` / `view.hide()` for visibility, and command
enable/disable through the command set (`Context::disable_command`,
which the menus and status line mirror) instead of setting a bit.

## Beyond the view: `WindowFlags`

The `wf*` decoration word gets the same treatment. A window's
[`WindowFlags`](../api/tvision_rs/window/struct.WindowFlags.html) carries `r#move`
(can be dragged, `wfMove`, spelled with a raw identifier because `move` is a
Rust keyword), `grow`, `close` and `zoom`. A default window sets all four, as
the C++ ctor does with `flags = wfMove | wfGrow | wfClose | wfZoom`. Those bools
gate behaviour as the bits did: a `cmClose` acts only when `close`
is set, and dragging is enabled when `r#move` or `grow` is.

For the runtime mechanics of how these flags steer dispatch and focus, see
[The event loop in depth](../internals/event-loop.md) and
[The view tree](../internals/view-tree.md).
