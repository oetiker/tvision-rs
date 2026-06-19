# Menus, status line & help

The top **menu bar**, the bottom **status line**, and the **context-sensitive
help** that ties them together are the chrome of every Turbo Vision program. In
tvision-rs all three are *data trees* you describe with fluent builders, then hand
to a view. The data lives in [`tv::menu`](../api/tvision_rs/menu/index.html) and
[`tv::status`](../api/tvision_rs/status/index.html); the help context is
[`HelpCtx`](../api/tvision_rs/help/struct.HelpCtx.html).

## The menu bar

A menu is a [`Menu`](../api/tvision_rs/menu/struct.Menu.html) — an ordered list of
entries plus a default selection. You rarely build one by hand; instead you chain
a [`MenuBuilder`](../api/tvision_rs/menu/struct.MenuBuilder.html) — a fluent
builder where each call appends one
[`MenuItem`](../api/tvision_rs/menu/enum.MenuItem.html) and returns `self`
*(the idiomatic successor to C++'s `operator+` chains)*:

```rust,ignore
let menu = Menu::builder()
    .submenu("~F~ile", alt('f'), |m| {
        m.command_key("~O~pen…", CMD_OPEN, KeyEvent::from(Key::F(3)), "F3")
            .command_key("~N~ew", CMD_NEW, KeyEvent::from(Key::F(4)), "F4")
            .separator()
            .command_key("E~x~it", Command::QUIT, alt('x'), "Alt-X")
    })
    .submenu("~W~indow", alt('w'), |m| {
        m.command("~T~ile", Command::TILE)
            .command("C~a~scade", Command::CASCADE)
    })
    .build();
```

Three things to notice:

- **`~`-marked labels.** The tildes bracket the hot-letter (`"~F~ile"` highlights
  the `F`). Submenus take an
  [`alt()`](../api/tvision_rs/menu/fn.alt.html) accelerator — a convenience that
  builds an `Alt`+`<char>` key.
- **Three entry kinds.** `command` / `command_key` append a
  [`MenuItem::Command`](../api/tvision_rs/menu/enum.MenuItem.html) (the latter adds
  an accelerator key plus the shortcut text shown at the right, like `"F3"`);
  `submenu` appends a nested menu; `separator` appends a divider.
- **It is just data.** Choosing an item emits its
  [`Command`](../api/tvision_rs/command/struct.Command.html) as an event — the menu
  never *does* anything itself. See [Commands & events](commands.md) for how that
  command reaches a handler, and for how a greyed-out (`disabled`) item is driven
  by command enable/disable state.

> **Turbo Vision heritage:** `command`/`command_key`, `submenu`, and `separator`
> map one-to-one onto C++ `TMenuItem`, `TSubMenu`, and `newLine()`; the
> `~`-marked hot-letters and `alt()` accelerators mirror the C++ `"~F~ile"`
> label convention and `kbAltF` literals.

Wrap the finished `Menu` in a
[`MenuBar`](../api/tvision_rs/menu/menu_bar/struct.MenuBar.html) and return it from
your `init_menu_bar` factory. `F10` enters the bar; the `Alt` accelerators open
submenus directly. With the `File` menu pulled down it looks like this:

{{#include ../screens/menubar.html}}

The complete factory is in
[`examples/hello.rs`](https://github.com/oetiker/tvision-rs/blob/main/examples/hello.rs)
— see [Your first app](../getting-started/first-app.md); the runnable menu/status
sources are the `menubar` and `statusline` entries in the
[widget gallery](../gallery.md).

## The status line

A status line is a `Vec<`[`StatusDef`](../api/tvision_rs/status/struct.StatusDef.html)`>`
— a list of *definitions*, each owning a list of
[`StatusItem`](../api/tvision_rs/status/struct.StatusItem.html)s, built with
[`StatusDef::list()`](../api/tvision_rs/status/struct.StatusDefListBuilder.html):

```rust,ignore
let defs = StatusDef::list()
    .def_all(|d| {
        d.item("~F3~ Open", KeyEvent::from(Key::F(3)), CMD_OPEN)
            .item("~F10~ Menu", KeyEvent::from(Key::F(10)), Command::MENU)
            .item("~Alt-X~ Exit", alt('x'), Command::QUIT)
    })
    .build();
```

Each item carries display text, an optional accelerator key, and a command —
clicking the label or pressing the key fires that command. Hand the `defs` to a
[`StatusLine`](../api/tvision_rs/status/status_line/struct.StatusLine.html) and
return it from your `init_status_line` factory.

A **hidden hotkey binding** is an item with no text — use the `key_item` builder
method. It draws nothing and consumes no width, but its accelerator still fires
globally — the standard trick for app-wide shortcuts like `Shift-Del` ⇒ Cut
*(the C++ equivalent was `TStatusItem(0, key, cmd)`).*

## Context-sensitive help

This is what a multiple-`StatusDef` list is *for*. Each def carries a
[`HelpCtxRange`](../api/tvision_rs/status/enum.HelpCtxRange.html); the status line
shows the items of the **first def whose range matches the current help
context**. Most apps need only one universal def — that is what `def_all`
([`HelpCtxRange::All`](../api/tvision_rs/status/enum.HelpCtxRange.html)) builds — but
you can register a `def_one_of(...)` whose items appear only while a particular
context is active, so the bottom line changes as focus moves between an editor, a
browser, and so on.

The current context is a [`HelpCtx`](../api/tvision_rs/help/struct.HelpCtx.html).
A help context is a namespaced `&'static str`
(`HelpCtx::custom("myapp.editor")`), so app- and view-defined contexts can never
collide. Because string identity carries no ordering, context ranges are expressed
as the two-variant `HelpCtxRange` above: `All`, or an explicit `OneOf` membership
set. Menu items also carry a `help_ctx`, so the same identity threads through the
whole UI.

> **Turbo Vision heritage:** the C++ `HelpCtx` was a hand-assigned `int`, and
> ranges were `[min, max]` numeric intervals. tvision-rs replaces the integer with a
> `&'static str` key and the numeric range with `HelpCtxRange`.

## Context-sensitive hints

Beyond switching which status-line items appear, the status line can show a
**free-form hint string** to the right of the items — a short message that
changes as the focused view changes context. This is the `hint` provider.

By default no hint is shown (`None`). Install one with
[`StatusLine::with_hint`](../api/tvision_rs/status/status_line/struct.StatusLine.html#method.with_hint)
(builder) or
[`StatusLine::set_hint`](../api/tvision_rs/status/status_line/struct.StatusLine.html#method.set_hint)
(post-construction). The provider is a closure `Fn(HelpCtx) -> Option<String>`;
it receives the *current* help context and returns the text to show (or `None`
to show nothing):

```rust
# use tvision_rs as tv;
# use tv::help::HelpCtx;
# use tv::status::{StatusDef, StatusLine};
# use tv::Rect;
# #[allow(unused_variables)]
# fn _demo() {
let defs = StatusDef::list()
    .def_all(|d| d)
    .build();

let line = StatusLine::new(Rect::new(0, 23, 80, 24), defs)
    .with_hint(|ctx| match ctx {
        c if c == HelpCtx::custom("myapp.editor") =>
            Some("F2 Save  F3 Open  Ctrl-F Find".to_string()),
        c if c == HelpCtx::custom("myapp.browser") =>
            Some("Enter Select  Esc Back  F5 Refresh".to_string()),
        _ => None,
    });
# let _ = line;
# }
```

The `Program` idle loop reads the focused view's `help_ctx` from its `ViewState`
and calls `StatusLine::set_help_ctx` at the start of each pump — so the hint
updates automatically as the user moves focus. The status line re-draws itself
when the context changes (it skips the redraw if the context has not changed,
making it idempotent).

**Combining defs with hints:** the two mechanisms are independent. Defs swap the
entire item list; the hint overlays a text string at the right end of the line.
Use `def_one_of` when you want *different command buttons* per context; use
`with_hint` for a short advisory string that supplements a shared button set.

Source: `src/status/status_line.rs` (`StatusLine::set_hint`, `StatusLine::with_hint`,
`StatusLine::set_help_ctx`), `src/status/mod.rs` (`HelpCtxRange`, `StatusDef`).

## See also

- [Your first app](../getting-started/first-app.md) — the menu bar and status
  line wired into a running program.
- [Commands & events](commands.md) — how a chosen item's command is dispatched,
  and how disabled items track command state.
- [Keyboard & key mapping](keyboard.md) — accelerators and the global key model.
