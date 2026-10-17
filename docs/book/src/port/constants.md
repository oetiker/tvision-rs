# Constant families → open newtypes

Turbo Vision carried flat families of small integers: `cmOK`, `cmCancel`,
`cmQuit` for commands; `hcNoContext`, `hcDragging` for help contexts; `kbEnter`,
`kbF1` for keys. Each prefix (`cm`, `hc`, `kb`) was hand-rolled namespacing in a
language without namespaces, and every value was a number that another library
could reuse.

The port keeps the *vocabulary* and changes the *representation*. Whether a
family becomes an open newtype or a closed `enum` is decided by **one question:
can application code legitimately invent a new value?**

## Commands: an open newtype

Application code defines its own commands all the time, so [`Command`](../api/tvision_rs/command/struct.Command.html)
is **open**: a one-field newtype around a `&'static str`, with the framework's
standard commands exposed as `SCREAMING_SNAKE` associated constants.

```rust
# use tvision_rs as tv;
let _ok     = tv::Command::OK;     // ports cmOK   — value "tv.ok"
let _cancel = tv::Command::CANCEL; // ports cmCancel
let _quit   = tv::Command::QUIT;   // ports cmQuit
```

There is no integer. In C++ those numbers existed only to serialize a
view (`TStreamable`, dropped) and to index a 256-bit `TCommandSet`. The port
needs neither, so a command's value is pure *identity*: a namespaced string. An
application defines its own with [`Command::custom`](../api/tvision_rs/command/struct.Command.html#method.custom),
using a dotted prefix unique to it:

```rust
# use tvision_rs as tv;
const REFRESH: tv::Command = tv::Command::custom("myapp.refresh");
# let _ = REFRESH;
```

The dotted prefix is the namespace the `cm` prefix imitated (`tv.*` for the
framework, `myapp.*` for the application), so application commands cannot
collide with the core's or another extension's. Equality and hashing compare the
string contents, so two `Command`s with the same name are equal wherever the
literals live.

Because identity is a string rather than a `0..=255` slot, the old 256-bit
`TCommandSet` becomes a hash-backed [`CommandSet`](../api/tvision_rs/command/struct.CommandSet.html).
Enable/disable, broadcasts and the command bus run on this open command space;
see [Commands & events](../apps/commands.md).

The framework keeps its shared commands in `Command`, but **view-specific
commands live with their view module** (the editor's `CHAR_LEFT`, the file
dialog's `FILE_OPEN`), and external views define theirs the same way. There is
no central registry to edit.

## Help contexts: the same shape

A context-sensitive help id is equally extensible, so
[`HelpCtx`](../api/tvision_rs/help/struct.HelpCtx.html) is built like
`Command`: an open newtype around a namespaced string, with
[`HelpCtx::NO_CONTEXT`](../api/tvision_rs/help/struct.HelpCtx.html) (TV's
`hcNoContext`) as the default and [`HelpCtx::custom`](../api/tvision_rs/help/struct.HelpCtx.html#method.custom)
for application ids. See [Menus, status line & help](../apps/menus.md).

## Keys: a closed enum

The set of physical keys is **fixed**, and no application invents a new one, so
[`Key`](../api/tvision_rs/event/enum.Key.html) is a closed `enum` (`Key::Enter`,
`Key::F(1)`). Closed sets become enums so that `match` is exhaustive; open,
app-extensible families become newtypes so the value space stays open. This
extensibility test decides every constant family in the port. Keys are covered
with the rest of input in [Events → enum + match](events.md).
