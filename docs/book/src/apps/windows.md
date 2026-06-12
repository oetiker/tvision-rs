# Windows & the desktop

Every Turbo Vision program draws on a **desktop** — a full-screen container that
holds a patterned background and any number of overlapping **windows**. This is
the same `TDeskTop`/`TWindow` pair you know from C++, ported as
[`Desktop`](../api/tvision/desktop/struct.Desktop.html) and
[`Window`](../api/tvision/window/struct.Window.html). Both *embed a*
[`Group`](../api/tvision/view/struct.Group.html) and delegate the
[`View`](../api/tvision/view/trait.View.html) trait to it (the embed-and-delegate
pattern — see [Inheritance → trait + composition](../port/inheritance.md)), so a
desktop *is* a view and a window *is* a view: you insert windows into a desktop,
and child controls into a window.

## The desktop

You rarely build a [`Desktop`](../api/tvision/desktop/struct.Desktop.html) by hand
at runtime — the application skeleton's `init_desktop` factory does it for you. In
the `hello` example that factory insets the bounds one row below the menu bar and
one above the status line, then calls
[`Desktop::new`](../api/tvision/desktop/struct.Desktop.html#method.new) with a
*background factory*. Pass
[`Desktop::init_background`](../api/tvision/desktop/struct.Desktop.html#method.init_background)
for the classic light-shade (`░`) fill, which builds a
[`Background`](../api/tvision/desktop/struct.Background.html). The skeleton wires
all three factories into the program at construction:

```rust,ignore
{{#rustdoc_include ../../../../examples/hello.rs:setup}}
```

## Opening windows

A [`Window`](../api/tvision/window/struct.Window.html) is constructed with its
bounds, an optional title, and a *window number* (`1`–`9` become the
`Alt-1`…`Alt-9` selectors; `0` means "no number"). By default it is movable,
resizable, closable and zoomable — all four
[`WindowFlags`](../api/tvision/window/struct.WindowFlags.html) start true,
exactly as the C++ ctor sets `wfMove | wfGrow | wfClose | wfZoom`.

To put a window on screen at construction time, insert it into the desktop. At
runtime — from inside the run loop — open one through the program, which inserts
it into the desktop *and* gives it focus in one step:

```rust,ignore
let r = prog.desktop_rect();
let win = Window::new(r, Some("Untitled".into()), next_num);
prog.desktop_insert(Box::new(win));
```

To give a window scroll bars, call
[`Window::standard_scroll_bar`](../api/tvision/window/struct.Window.html#method.standard_scroll_bar)
with [`ScrollBarOptions`](../api/tvision/window/struct.ScrollBarOptions.html) — its
`vertical` flag selects the right edge (else the bottom), and `handle_keyboard`
opts the bar into post-processing of the focused chain's arrow keys. It inserts
the bar on the correct edge and returns its `ViewId`. For child controls, use
[`Window::insert_child`](../api/tvision/window/struct.Window.html#method.insert_child).

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
resize it — the desktop routes the click and the window starts the drag. The
underlying nested mouse loop of C++ `dragView` becomes a capture handler on the
single event loop (see [Modal execView → one loop + capture](../port/modal.md)).

## Tiling and cascading

The desktop can auto-arrange its **tileable** windows.
[`Desktop::tile`](../api/tvision/desktop/struct.Desktop.html#method.tile) packs
them into a most-equal grid;
[`Desktop::cascade`](../api/tvision/desktop/struct.Desktop.html#method.cascade)
stacks them stepped down and to the right. Both skip windows that are not visible
or not marked tileable, and both leave bounds unchanged when a window will not fit
— faithful to the C++ `tileError` no-op. Note that `Window` does **not** set the
tileable option for you; opt a window in explicitly:

```rust,ignore
win.state_mut().options.tileable = true;
```

The `hello` example wires `Command::TILE` / `Command::CASCADE` menu items that
route to these calls, so its three demo windows rearrange on command.

## See also

- [Dialogs & data](dialogs.md) — modal windows with gather/scatter
- [Controls](controls.md) — what goes *inside* a window
- [The view tree](../internals/view-tree.md) — how desktop, window and group nest
- [The event loop in depth](../internals/event-loop.md) — focus, capture and z-order at runtime
