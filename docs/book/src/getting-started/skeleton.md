# The application skeleton

Turbo Vision splits the running application into two layers, and so does
tvision. Knowing which is which makes the rest of the framework easier to read.

## `Program` and `Application`

[`Program`](../api/tvision/app/struct.Program.html) is the engine. It owns the
**view tree** (the desktop with its windows, the menu bar, the status line), the
**event loop**, the **capture stack** that drives modal dialogs, and the
**backend** that talks to the terminal. It is the port of C++ `TProgram`. The
`hello` example builds a `Program` directly and wraps it in its own
`HelloApp` struct.

[`Application`](../api/tvision/app/struct.Application.html) is a thin wrapper *over*
`Program` — the port of `TApplication`. It forwards everything to the embedded
`Program` and adds the application-level commands `tile` / `cascade` (desktop
window layout) and `dosShell` (suspend the terminal). You can build on either;
use `Application` when you want those extras for free, or `Program` when you
want to assemble the engine yourself.

Both are constructed the same way — from a theme and the three factory functions
that build the desktop, status line, and menu bar:

```rust,ignore
let program = Program::new(
    backend,
    clock,
    theme,
    init_desktop,      // FnOnce(Rect) -> Option<Box<dyn View>>
    init_status_line,
    init_menu_bar,
);
```

This mirrors the C++ `TProgInit(initStatusLine, initMenuBar, initDeskTop)`
factory-mixin: each factory receives the screen rectangle and returns the view
to install (insetting itself to the top row, bottom row, or the space between).

## The run loop

Calling [`run`](../api/tvision/app/struct.Program.html#method.run) spins the event
loop until a quit command ends it. For an app that needs to react to its *own*
commands, [`run_app`](../api/tvision/app/struct.Program.html#method.run_app) takes a
closure that is called whenever the program handles a command it does not
recognise — the equivalent of overriding `TApplication::handleEvent`:

```rust,ignore
{{#rustdoc_include ../../../../examples/hello.rs:run}}
```

Under the hood, each turn of the loop is one call to
[`pump_once`](../api/tvision/app/struct.Program.html#method.pump_once): read the next
event, route it through the capture stack and the view tree, apply any deferred
effects, then redraw the whole tree and diff it against the back buffer. The
[event loop in depth](../internals/event-loop.md) chapter walks through exactly
what `pump_once` does and why.

## Where to go next

You have now seen the three moving parts: the **factories** that build the view
tree, the **`Program`/`Application`** layers that own it, and the **run loop**
that drives it. From here:

- **[Building Apps](../apps/windows.md)** — task recipes for windows, dialogs,
  controls, menus, and more.
- **[How It Works](../internals/view-tree.md)** — the architecture, ending at
  writing your own `View`.
