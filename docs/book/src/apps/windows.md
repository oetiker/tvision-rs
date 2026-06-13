# Windows & the desktop

[`Desktop`](../api/rstv/desktop/struct.Desktop.html) and
[`Window`](../api/rstv/window/struct.Window.html) are the two core structural
views of every rstv program: a full-screen container that holds a patterned
background and any number of overlapping windows. Both *embed a*
[`Group`](../api/rstv/view/struct.Group.html) and delegate the
[`View`](../api/rstv/view/trait.View.html) trait to it (the embed-and-delegate
pattern — see [Inheritance → trait + composition](../port/inheritance.md)), so a
desktop *is* a view and a window *is* a view: you insert windows into a desktop,
and child controls into a window *(these are the rstv equivalents of the C++
`TDeskTop`/`TWindow` pair)*.

## The desktop

You rarely build a [`Desktop`](../api/rstv/desktop/struct.Desktop.html) by hand
at runtime — the application skeleton's `init_desktop` factory does it for you. In
the `hello` example that factory insets the bounds one row below the menu bar and
one above the status line, then calls
[`Desktop::new`](../api/rstv/desktop/struct.Desktop.html#method.new) with a
*background factory*. Pass
[`Desktop::init_background`](../api/rstv/desktop/struct.Desktop.html#method.init_background)
for the classic light-shade (`░`) fill, which builds a
[`Background`](../api/rstv/desktop/struct.Background.html). The skeleton wires
all three factories into the program at construction:

```rust,ignore
{{#rustdoc_include ../../../../examples/hello.rs:setup}}
```

## Opening windows

A [`Window`](../api/rstv/window/struct.Window.html) is constructed with its
bounds, an optional title, and a *window number* (`1`–`9` become the
`Alt-1`…`Alt-9` selectors; `0` means "no number"). By default it is movable,
resizable, closable and zoomable — all four
[`WindowFlags`](../api/rstv/window/struct.WindowFlags.html) start true
*(corresponding to the C++ `wfMove | wfGrow | wfClose | wfZoom` flags)*.

To put a window on screen at construction time, insert it into the desktop. At
runtime — from inside the run loop — open one through the program, which inserts
it into the desktop *and* gives it focus in one step:

```rust
# use rstv as tv;
# use tv::Window;
# fn _demo(prog: &mut tv::Program) {
# let next_num: i16 = 1;
let r = prog.desktop_rect();
let win = Window::new(r, Some("Untitled".into()), next_num);
prog.desktop_insert(Box::new(win));
# }
```

To give a window scroll bars, call
[`Window::standard_scroll_bar`](../api/rstv/window/struct.Window.html#method.standard_scroll_bar)
with [`ScrollBarOptions`](../api/rstv/window/struct.ScrollBarOptions.html) — its
`vertical` flag selects the right edge (else the bottom), and `handle_keyboard`
opts the bar into post-processing of the focused chain's arrow keys. It inserts
the bar on the correct edge and returns its `ViewId`. For child controls, use
[`Window::insert_child`](../api/rstv/window/struct.Window.html#method.insert_child).

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
[`Desktop::tile`](../api/rstv/desktop/struct.Desktop.html#method.tile) packs
them into a most-equal grid;
[`Desktop::cascade`](../api/rstv/desktop/struct.Desktop.html#method.cascade)
stacks them stepped down and to the right. Both skip windows that are not visible
or not marked tileable, and both leave a window's bounds unchanged when it will
not fit — a safe no-op. Note that `Window` does **not** set the tileable option
for you; opt a window in explicitly:

```rust
# use rstv as tv;
# use tv::View;
# fn _demo(win: &mut tv::Window) {
win.state_mut().options.tileable = true;
# }
```

The `hello` example wires `Command::TILE` / `Command::CASCADE` menu items that
route to these calls, so its three demo windows rearrange on command.

## Splitter — resizable panes

A [`Splitter`](../api/rstv/widgets/splitter/struct.Splitter.html) divides a
rectangle into N panes along one axis, separated by 1-cell divider seams that the
user can drag to resize. It is the idiomatic way to build IDE-style or
file-manager-style layouts inside a window.

### Axes and panes

```rust,ignore
use rstv::{Splitter, Constraints};

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

[`Constraints`](../api/rstv/widgets/splitter/layout/struct.Constraints.html) control how
much space a pane claims along the axis:

| Constructor | Meaning |
| --- | --- |
| `Constraints::flex()` | Elastic — takes its share of remaining space (weight 1) |
| `Constraints::weight(w)` | Elastic with a custom weight |
| `Constraints::fixed(n)` | Pinned to exactly `n` cells |
| `.min(n)` builder | Minimum size in cells |

### Divider styles

[`DividerStyle`](../api/rstv/widgets/splitter/enum.DividerStyle.html) controls
how a seam looks and behaves:

| Variant | Look | Draggable? |
| --- | --- | --- |
| `Line` (default) | Always-visible `│` / `─` | Yes |
| `Handle` | Clean — only a grab nub at midpoint | Yes |
| `Hidden` | Invisible in normal use | Only in reconfig mode |
| `Locked` | Invisible and immovable | Never |

Set a per-seam style with `.divider(i, style)` or a blanket default with
`.default_divider(style)`.

### Live drag and `F6` reconfig

Dragging a `Line` or `Handle` seam with the mouse resizes immediately. Pressing
`F6` (or whatever key you bind to `Command::NEXT`) enters *reconfig mode*: the
selected seam is highlighted and arrow keys move it; `Tab` cycles between seams;
`Esc` restores the pre-reconfig weights; `Enter` confirms. `Locked` seams are
skipped in reconfig mode.

### Nested splitters

A `Splitter` can contain another `Splitter` as a pane, building grid layouts:

```rust,ignore
use rstv::{Splitter, Constraints, StaticText, Rect};

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
use rstv::{Splitter, Constraints, StaticText, Rect};

let split = Splitter::cols()
    .pane(Box::new(StaticText::new(Rect::new(0, 0, 1, 1), "left")), Constraints::flex())
    .pane(Box::new(StaticText::new(Rect::new(0, 0, 1, 1), "right")), Constraints::flex())
    .joined();   // connects divider lines to the window frame
```

A live example showing `.joined()` on a nested three-pane layout is in the
[Widget gallery](../gallery.md).

## See also

- [Dialogs & data](dialogs.md) — modal windows with gather/scatter
- [Controls](controls.md) — what goes *inside* a window
- [The view tree](../internals/view-tree.md) — how desktop, window and group nest
- [The event loop in depth](../internals/event-loop.md) — focus, capture and z-order at runtime
