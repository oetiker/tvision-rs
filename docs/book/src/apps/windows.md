# Windows & the desktop

[`Desktop`](../api/tvision_rs/desktop/struct.Desktop.html) and
[`Window`](../api/tvision_rs/window/struct.Window.html) are the two core structural
views of every tvision-rs program: a full-screen container that holds a patterned
background and any number of overlapping windows. Both *embed a*
[`Group`](../api/tvision_rs/view/struct.Group.html) and delegate the
[`View`](../api/tvision_rs/view/trait.View.html) trait to it (the embed-and-delegate
pattern — see [Inheritance → trait + composition](../port/inheritance.md)), so a
desktop *is* a view and a window *is* a view: you insert windows into a desktop,
and child controls into a window *(these are the tvision-rs equivalents of the C++
`TDeskTop`/`TWindow` pair)*.

## The desktop

You rarely build a [`Desktop`](../api/tvision_rs/desktop/struct.Desktop.html) by hand
at runtime — the application skeleton's `init_desktop` factory does it for you. In
the `hello` example that factory insets the bounds one row below the menu bar and
one above the status line, then calls
[`Desktop::new`](../api/tvision_rs/desktop/struct.Desktop.html#method.new) with a
*background factory*. Pass
[`Desktop::init_background`](../api/tvision_rs/desktop/struct.Desktop.html#method.init_background)
for the classic light-shade (`░`) fill, which builds a
[`Background`](../api/tvision_rs/desktop/struct.Background.html). The skeleton wires
all three factories into the program at construction:

```rust,ignore
{{#rustdoc_include ../../../../examples/hello.rs:setup}}
```

## Opening windows

A [`Window`](../api/tvision_rs/window/struct.Window.html) is constructed with its
bounds, an optional title, and a *window number* (`1`–`9` become the
`Alt-1`…`Alt-9` selectors; `0` means "no number"). By default it is movable,
resizable, closable and zoomable — all four
[`WindowFlags`](../api/tvision_rs/window/struct.WindowFlags.html) start true
*(corresponding to the C++ `wfMove | wfGrow | wfClose | wfZoom` flags)*.

To put a window on screen at construction time, insert it into the desktop. At
runtime — from inside the run loop — open one through the program, which inserts
it into the desktop *and* gives it focus in one step:

```rust
# use tvision_rs as tv;
# use tv::Window;
# fn _demo(prog: &mut tv::Program) {
# let next_num: i16 = 1;
let r = prog.desktop_rect();
let win = Window::new(r, Some("Untitled".into()), next_num);
prog.desktop_insert(Box::new(win));
# }
```

To give a window scroll bars, call
[`Window::standard_scroll_bar`](../api/tvision_rs/window/struct.Window.html#method.standard_scroll_bar)
with [`ScrollBarOptions`](../api/tvision_rs/window/struct.ScrollBarOptions.html) — its
`vertical` flag selects the right edge (else the bottom), and `handle_keyboard`
opts the bar into post-processing of the focused chain's arrow keys. It inserts
the bar on the correct edge and returns its `ViewId`. For child controls, use
[`Window::insert_child`](../api/tvision_rs/window/struct.Window.html#method.insert_child).

## Z-order, focus and window commands

Windows overlap, and the **topmost** one is the active window — it draws its
frame in the active style and receives keyboard events. The active window enables
the five window-management commands while it is selected; they are disabled
otherwise (the framework starts them off — see [Commands & events](commands.md)):

| Command | `hello` key | Effect |
| ------- | ----------- | ------ |
| `Command::NEXT` | `F6` | Cycle focus to the next window |
| `Command::PREV` | — | Send the active window to the back |
| `Command::ZOOM` | `F5` | Toggle between restored size and filling the desktop |
| `Command::CLOSE` | `Alt-F3` | Close the active window |
| `Command::RESIZE` | — | Enter keyboard move/resize mode (arrows, `Enter`/`Esc`) |

The keys above are the bindings the `hello` example installs on its status line
and menu; the commands themselves carry no built-in key.

You can also drag a window by its title bar to move it, or by a bottom corner to
resize it — the desktop routes the click and the window starts the drag. The drag
runs as a capture handler on the single event loop, with no nested loop
(see [Modal execView → one loop + capture](../port/modal.md)) *(C++ used a
dedicated `dragView` nested-mouse-loop for this)*.

## Tiling and cascading

The desktop can auto-arrange its **tileable** windows.
[`Desktop::tile`](../api/tvision_rs/desktop/struct.Desktop.html#method.tile) packs
them into a most-equal grid;
[`Desktop::cascade`](../api/tvision_rs/desktop/struct.Desktop.html#method.cascade)
stacks them stepped down and to the right. Both skip windows that are not visible
or not marked tileable, and both leave a window's bounds unchanged when it will
not fit — a safe no-op. Note that `Window` does **not** set the tileable option
for you; opt a window in explicitly:

```rust
# use tvision_rs as tv;
# use tv::View;
# fn _demo(win: &mut tv::Window) {
win.state_mut().options.tileable = true;
# }
```

The `hello` example wires `Command::TILE` / `Command::CASCADE` menu items that
route to these calls, so its three demo windows rearrange on command.

## Splitter — resizable panes

A [`Splitter`](../api/tvision_rs/widgets/splitter/struct.Splitter.html) divides a
rectangle into N panes along one axis, separated by 1-cell divider seams that the
user can drag to resize. It is the idiomatic way to build IDE-style or
file-manager-style layouts inside a window.

### Axes and panes

```rust,ignore
use tvision_rs::{Splitter, Constraints};

// Side-by-side panes (vertical dividers):
let h = Splitter::cols()
    .pane(left_view, Constraints::fixed(20))   // 20-cell sidebar
    .pane(right_view, Constraints::flex());     // rest of the space

// Stacked panes (horizontal dividers):
let v = Splitter::rows()
    .pane(top_view, Constraints::flex())
    .pane(bottom_view, Constraints::flex());
```

`Splitter::cols()` splits along x (side-by-side); `Splitter::rows()` splits along
y (stacked). Each `.pane(view, constraints)` call appends a pane.

### Constraints

[`Constraints`](../api/tvision_rs/widgets/splitter/layout/struct.Constraints.html) control how
much space a pane claims along the axis:

| Constructor | Meaning |
| --- | --- |
| `Constraints::flex()` | Elastic — takes its share of remaining space (weight 1) |
| `Constraints::weight(w)` | Elastic with a custom weight |
| `Constraints::fixed(n)` | Pinned to exactly `n` cells |
| `.min(n)` builder | Minimum size in cells |

### Divider styles

[`DividerStyle`](../api/tvision_rs/widgets/splitter/enum.DividerStyle.html) controls
how a seam looks and behaves:

| Variant | Look | Draggable? |
| --- | --- | --- |
| `Line` (default) | Always-visible `│` / `─` | Yes |
| `Handle` | Clean — only a grab nub at midpoint | Yes |
| `Hidden` | Invisible in normal use | Only in resize mode |
| `Locked` | Invisible and immovable | Never |

Set a per-seam style with `.divider(i, style)` or a blanket default with
`.default_divider(style)`.

### Live drag and keyboard resize

Dragging a `Line` or `Handle` seam with the mouse resizes immediately. Pressing
`Ctrl-F5` (bound to `Command::RESIZE`) enters *resize mode*: `Tab` / `Shift-Tab`
cycle the resize target between the window itself and each splitter divider;
`Enter` commits; `Esc` cancels. `Locked` seams are skipped when cycling.

While the **window** is the active target, plain arrows **move** the window and
`Shift`+arrows **resize** (grow) it — matching the classic `TView::change`
contract. `Ctrl` scales the step to ±8 cells horizontally / ±4 vertically, so
`Ctrl`+arrow is a big move and `Ctrl`+`Shift`+arrow is a big resize.

While a **divider** is the active target, plain arrows nudge it ±1 cell along
its axis; `Shift` and `Ctrl` are ignored for dividers.

### Nested splitters

A `Splitter` can contain another `Splitter` as a pane, building grid layouts:

```rust,ignore
use tvision_rs::{Splitter, Constraints, StaticText, Rect};

// Right column: two stacked panes.
let right = Splitter::rows()
    .pane(Box::new(StaticText::new(Rect::new(0, 0, 1, 1), "top")), Constraints::flex())
    .pane(Box::new(StaticText::new(Rect::new(0, 0, 1, 1), "bottom")), Constraints::flex());

// Outer: fixed sidebar + the nested right column.
let split = Splitter::cols()
    .pane(Box::new(StaticText::new(Rect::new(0, 0, 1, 1), "sidebar")), Constraints::fixed(16))
    .pane(Box::new(right), Constraints::flex());
```

### Joined linework

By default a splitter draws plain `│`/`─` dividers. Call `.joined()` on the
**outermost** splitter to connect the divider lines:

- to the surrounding window frame: `┬` / `┴` at the top and bottom edges, `├` /
  `┤` at the left and right edges;
- to each other inside nested splitters: interior `├` / `┤` / `┬` / `┴` / `┼`
  crossings.

Joining cascades — you only need `.joined()` on the outermost splitter; all
nested pane-splitters inherit it automatically. Override the grow mode (a
splitter fills its owner by default) with `.with_grow_mode(GrowMode::default())`
for a fixed-size splitter.

```rust,ignore
use tvision_rs::{Splitter, Constraints, StaticText, Rect};

let split = Splitter::cols()
    .pane(Box::new(StaticText::new(Rect::new(0, 0, 1, 1), "left")), Constraints::flex())
    .pane(Box::new(StaticText::new(Rect::new(0, 0, 1, 1), "right")), Constraints::flex())
    .joined();   // connects divider lines to the window frame
```

A live example showing `.joined()` on a nested three-pane layout is in the
[Widget gallery](../gallery.md).

## Limiting move and resize

Every view carries a [`DragMode`](../api/tvision_rs/view/struct.DragMode.html) — a
struct of bools controlling whether a window can be moved or resized, and which
edges constrain the drag. `Window` sets `drag_move` and `drag_grow` by default
(the user can drag the title bar to move and the bottom corners to resize).

```rust
# use tvision_rs as tv;
# use tv::{DragMode, View};
# fn _demo(win: &mut tv::Window) {
// Make the window movable but not resizable:
win.state_mut().drag_mode = DragMode {
    drag_move: true,
    drag_grow: false,
    drag_grow_left: false,
    limit_lo_x: true,
    limit_lo_y: true,
    limit_hi_x: true,
    limit_hi_y: true,
};
# }
```

The `limit_*` bits clamp the drag to the owner's extent — without them the
user could drag the window partially off-screen. `DragMode::limit_all()` is a
convenience that sets all four limit bits at once; it is the standard setting
for a window that must remain fully on the desktop.

The `drag_grow_left` bit adds the left edge as a resize target. By default only
the **bottom-right corner** is draggable — `drag_grow_left: true` extends resize
to the left edge as well (useful for left-aligned panels).

Source: `src/view/view.rs` (`DragMode`), `src/window/window.rs` (window default drag setup).

> **Turbo Vision heritage:** `dmDragMove`/`dmDragGrow`/`dmLimitXxx` were bit-field
> constants combined with `|`. tvision-rs replaces them with a struct of bools (deviation
> D5) — the individual fields replace the bit-shift arithmetic.

## Grow modes: anchoring edges

[`GrowMode`](../api/tvision_rs/view/struct.GrowMode.html) controls how a child view
reacts when its **owner group is resized**. Each `gf*` bit anchors one edge of
the child to the *corresponding edge* of the owner. When the owner grows, the
anchored edge moves with it.

| Field | Meaning when `true` |
| --- | --- |
| `lo_x` (left edge) | tracks the owner's **right** edge — child slides right |
| `lo_y` (top edge) | tracks the owner's **bottom** edge — child slides down |
| `hi_x` (right edge) | tracks the owner's **right** edge — child widens |
| `hi_y` (bottom edge) | tracks the owner's **bottom** edge — child grows taller |
| `fixed` | override — child size is fixed; none of the above apply |
| `rel` | proportional scale instead of absolute anchoring |

A child whose *right* and *bottom* edges track the owner's — `hi_x: true, hi_y:
true` — fills the owner and grows with it. A child whose *left* and *right* edges
both track the owner's right — `lo_x: true, hi_x: true` — keeps a fixed width
but slides to maintain its right-edge distance.

```rust
# use tvision_rs as tv;
# use tv::{GrowMode, View};
# fn _demo(child: &mut tv::widgets::StaticText) {
// Child fills its owner (grows on both axes):
child.state_mut().grow_mode = GrowMode {
    hi_x: true,
    hi_y: true,
    ..Default::default()
};
# }
```

`GrowMode::grow_all()` sets `lo_x | lo_y | hi_x | hi_y` — every edge tracks
the owner, so the child maintains its margins on all four sides.

Grow mode is applied by [`View::change_bounds`](../api/tvision_rs/view/trait.View.html#method.change_bounds)
(`src/view/view.rs`): when a group is resized, it calls `calc_bounds` on each
child (which applies the grow formula) and then `change_bounds` to commit the new
rectangle.

Source: `src/view/view.rs` (`GrowMode`, `ViewState::calc_bounds`).

> **Turbo Vision heritage:** the `gf*` constants (`gfGrowLoX`, `gfGrowHiX`, …)
> were individual bits in a `ushort`. tvision-rs maps each to a named bool field
> in `GrowMode` (deviation D5).

## Bringing a window to the front

Z-order in a group is reverse insertion order: the most recently inserted child
sits on top. When the user clicks a window or presses `Alt`-*N*, the framework
calls [`Group::focus_child`](../api/tvision_rs/view/struct.Group.html#method.focus_child)
on the desktop. `focus_child` checks whether the outgoing current view wants to
keep focus (the `ofValidate` path), then, because windows opt into **raise-on-select**
(`Options::top_select = true`), calls
[`Group::make_first`](../api/tvision_rs/view/struct.Group.html#method.make_first):
the window is moved to the last slot of the `children` Vec (the top slot in
paint order).

There is no general "reorder to arbitrary Z position" primitive. Only two
operations exist:

- **`make_first(id, ctx)`** — raises one view to the very top.
- **`put_in_front_of(id, target, ctx)`** — places `id` immediately in front of
  `target` (one slot above it in paint order).

`make_first` is the primitive; `put_in_front_of(id, None, ctx)` is its
definition. Both are no-ops when the child is already in the target position.

The resulting effect is familiar: clicking any window brings it to the top of the
stack and makes it active. The previous front window becomes passive (its frame
changes color) and receives the `StateFlag::Active` clear cascade.

```rust
# use tvision_rs as tv;
# use tv::View;
# fn _demo(desktop: &mut tv::Group, win_id: tv::ViewId, ctx: &mut tv::Context) {
// Raise a window to the front programmatically:
desktop.make_first(win_id, ctx);
# }
```

Source: `src/view/group.rs` (`Group::focus_child`, `Group::make_first`,
`Group::put_in_front_of`, and the `top_select` option check).

> **Turbo Vision heritage:** `TGroup::makeFirst` / `putInFrontOf` performed the same
> move in the circular doubly-linked sibling ring. tvision-rs stores children in a
> `Vec` and implements raise-to-top as a `swap` to the last slot (deviation D3).

## See also

- [Dialogs & data](dialogs.md) — modal windows with gather/scatter
- [Controls](controls.md) — what goes *inside* a window
- [The view tree](../internals/view-tree.md) — how desktop, window and group nest
- [The event loop in depth](../internals/event-loop.md) — focus, capture and z-order at runtime
