# Your first app

The shortest path to something on screen is the `hello` example that ships with
the crate. It builds the three things every Turbo Vision program has — a
**desktop** with a patterned background, a **menu bar** along the top, and a
**status line** along the bottom — and then spins the real event loop. Run it:

```console
$ cargo run --example hello
```

You get a classic Turbo Vision screen. Overlapping demo windows sit on the
desktop; `F10` enters the menu, `Alt-F`/`Alt-W`/`Alt-C` open the menus by
hot-key, and `Alt-X` quits:

{{#include ../screens/hello.html}}

> The screenshot above is the *actual* terminal output, captured from the
> running program and rendered as colored, selectable HTML — not a picture. See
> [The screenshot tooling](../reference/screenshots.md) for how it is made.

## Building the program

A Turbo Vision app is assembled from three factory functions — one each for the
desktop, the status line, and the menu bar — handed to the program at
construction. In C++ these are the `initDeskTop` / `initStatusLine` /
`initMenuBar` overrides of `TApplication`; here they are plain functions passed
to [`Program::new`](../api/tvision/app/struct.Program.html):

```rust,ignore
{{#rustdoc_include ../../../../examples/hello.rs:setup}}
```

Every command is **enabled by default** (the framework starts from a denylist,
not an allowlist), so the app-minted commands need no registration. Only the
five window-management commands begin disabled, until a window grants them when
it is selected.

## Entering the event loop

`main` owns the terminal and runs the loop until a quit command ends it. The
[`CrosstermBackend`](../api/tvision/backend/struct.CrosstermBackend.html) constructor
takes over raw mode, the alternate screen, and mouse capture, and restores all
of it on drop — even on a panic or a signal — just like the C++ `TApplication`
constructor chain:

```rust,ignore
{{#rustdoc_include ../../../../examples/hello.rs:main}}
```

That is the whole entry point. The complete, runnable program — including the
desktop/menu/status factories above and the command handler that opens editor
windows — is
[`examples/hello.rs`](https://github.com/oetiker/rstv/blob/main/examples/hello.rs).

## What's next

- The pieces you just wired up — [`Program`](../api/tvision/app/struct.Program.html),
  [`Desktop`](../api/tvision/desktop/struct.Desktop.html),
  [`MenuBar`](../api/tvision/menu/menu_bar/struct.MenuBar.html),
  [`StatusLine`](../api/tvision/status/status_line/struct.StatusLine.html) — are explained at a
  higher level in [The application skeleton](skeleton.md).
- To see how the loop, the views, and drawing actually fit together, read [How
  It Works](../internals/view-tree.md).
