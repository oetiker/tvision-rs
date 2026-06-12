# Menus, status line & help

The top **menu bar**, the bottom **status line**, and the **context-sensitive
help** that ties them together are the chrome of every Turbo Vision program. In
`tvision` all three are *data trees* you describe with fluent builders, then hand
to a view. The data lives in [`tv::menu`](../api/tvision/menu/index.html) and
[`tv::status`](../api/tvision/status/index.html); the help context is
[`HelpCtx`](../api/tvision/help/struct.HelpCtx.html).

## The menu bar

A menu is a [`Menu`](../api/tvision/menu/struct.Menu.html) — an ordered list of
entries plus a default selection. You rarely build one by hand; instead you chain
a [`MenuBuilder`](../api/tvision/menu/struct.MenuBuilder.html), the idiomatic
replacement for the C++ `operator+` chains. Each call appends one
[`MenuItem`](../api/tvision/menu/enum.MenuItem.html) and returns `self`:

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

- **`~`-marked labels.** The tildes bracket the hot-letter, exactly as in C++
  (`"~F~ile"` highlights the `F`). Submenus take an
  [`alt()`](../api/tvision/menu/fn.alt.html) accelerator — a convenience that
  builds an `Alt`+`<char>` key, mirroring the C++ `kbAltF` literals.
- **Three entry kinds.** `command` / `command_key` append a
  [`MenuItem::Command`](../api/tvision/menu/enum.MenuItem.html) (the latter adds
  an accelerator key plus the shortcut text shown at the right, like `"F3"`);
  `submenu` appends a nested menu; `separator` appends a divider. These map
  one-to-one onto C++ `TMenuItem`, `TSubMenu`, and `newLine()`.
- **It is just data.** Choosing an item emits its
  [`Command`](../api/tvision/command/struct.Command.html) as an event — the menu
  never *does* anything itself. See [Commands & events](commands.md) for how that
  command reaches a handler, and for how a greyed-out (`disabled`) item is driven
  by command enable/disable state.

Wrap the finished `Menu` in a
[`MenuBar`](../api/tvision/menu/menu_bar/struct.MenuBar.html) and return it from
your `init_menu_bar` factory. `F10` enters the bar; the `Alt` accelerators open
submenus directly. The complete factory is in
[`examples/hello.rs`](https://github.com/oetiker/rstv/blob/main/examples/hello.rs)
— see [Your first app](../getting-started/first-app.md).

## The status line

A status line is a `Vec<`[`StatusDef`](../api/tvision/status/struct.StatusDef.html)`>`
— a list of *definitions*, each owning a list of
[`StatusItem`](../api/tvision/status/struct.StatusItem.html)s, built with
[`StatusDef::list()`](../api/tvision/status/struct.StatusDefListBuilder.html):

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
[`StatusLine`](../api/tvision/status/status_line/struct.StatusLine.html) and
return it from your `init_status_line` factory.

A **hidden hotkey binding** is an item with no text (the `key_item` builder
method, the C++ `TStatusItem(0, key, cmd)`): it draws nothing and consumes no
width, but its accelerator still fires globally — the standard trick for app-wide
shortcuts like `Shift-Del` ⇒ Cut.

## Context-sensitive help

This is what a multiple-`StatusDef` list is *for*. Each def carries a
[`HelpCtxRange`](../api/tvision/status/enum.HelpCtxRange.html); the status line
shows the items of the **first def whose range matches the current help
context**. Most apps need only one universal def — that is what `def_all`
([`HelpCtxRange::All`](../api/tvision/status/enum.HelpCtxRange.html)) builds — but
you can register a `def_one_of(...)` whose items appear only while a particular
context is active, so the bottom line changes as focus moves between an editor, a
browser, and so on.

The current context is a [`HelpCtx`](../api/tvision/help/struct.HelpCtx.html).
Under deviation **D1** a help context is not the C++ hand-assigned `int` but a
namespaced `&'static str` (`HelpCtx::custom("myapp.editor")`), so app- and
view-defined contexts cannot collide. Because string identity has no ordering,
the C++ `[min, max]` numeric range becomes the two-variant `HelpCtxRange` above:
`All`, or an explicit `OneOf` membership set. Menu items also carry a `help_ctx`,
so the same identity threads through the whole UI.

## See also

- [Your first app](../getting-started/first-app.md) — the menu bar and status
  line wired into a running program.
- [Commands & events](commands.md) — how a chosen item's command is dispatched,
  and how disabled items track command state.
- [Keyboard & key mapping](keyboard.md) — accelerators and the global key model.
