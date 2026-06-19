# Constant families → open newtypes

Turbo Vision carried flat families of small integers — `cmOK`, `cmCancel`,
`cmQuit` for commands; `hcNoContext`, `hcDragging` for help contexts; `kbEnter`,
`kbF1` for keys. Each prefix (`cm`, `hc`, `kb`) was hand-rolled namespacing in a
language that had no real namespaces, and every value was a magic number you
hoped nobody else had reused.

The port keeps the *vocabulary* but changes the *representation*. Whether a
family becomes an open newtype or a closed `enum` is decided by **one question:
can application code legitimately invent a new value?**

## Commands: an open newtype

App code constantly mints its own commands, so [`Command`](../api/tvision_rs/command/struct.Command.html)
is **open**: a one-field newtype around a `&'static str`, with the framework's
standard commands exposed as `SCREAMING_SNAKE` associated constants.

```rust
# use tvision_rs as tv;
let _ok     = tv::Command::OK;     // ports cmOK   — value "tv.ok"
let _cancel = tv::Command::CANCEL; // ports cmCancel
let _quit   = tv::Command::QUIT;   // ports cmQuit
```

The integer never comes back. In C++ those numbers existed only to serialize a
view (`TStreamable`, dropped) and to index a 256-bit `TCommandSet`. Neither is
needed, so a command's value is now pure *identity*: a namespaced string. Your
app defines its own with [`Command::custom`](../api/tvision_rs/command/struct.Command.html#method.custom),
picking a dotted prefix unique to you:

```rust
# use tvision_rs as tv;
const REFRESH: tv::Command = tv::Command::custom("myapp.refresh");
# let _ = REFRESH;
```

The dotted prefix is the namespace the `cm` prefix was faking — `tv.*` for the
framework, `myapp.*` for you — so your commands cannot collide with the core's
or another extension's *by construction*. Equality and hashing compare the
string contents, so two `Command`s with the same name are equal no matter where
the literals live.

Because identity is a string rather than a `0..=255` slot, the old 256-bit
`TCommandSet` becomes a hash-backed [`CommandSet`](../api/tvision_rs/command/struct.CommandSet.html).
That open command space is what enable/disable, broadcasts, and the command bus
run on — see [Commands & events](../apps/commands.md).

The framework keeps its shared commands in `Command`, but **view-specific
commands live with their view module** (the editor's `CHAR_LEFT`, the file
dialog's `FILE_OPEN`), and external views mint theirs the same way. There is no
central registry to edit.

## Help contexts: the same shape

A context-sensitive help id is just as extensible, so
[`HelpCtx`](../api/tvision_rs/help/struct.HelpCtx.html) is built exactly like
`Command` — an open newtype around a namespaced string, with
[`HelpCtx::NO_CONTEXT`](../api/tvision_rs/help/struct.HelpCtx.html) (TV's
`hcNoContext`) as the default and [`HelpCtx::custom`](../api/tvision_rs/help/struct.HelpCtx.html#method.custom)
for your own. See [Menus, status line & help](../apps/menus.md).

## Keys: a closed enum

Keys are the opposite case. The set of physical keys is **fixed** — no app
invents a new one — so [`Key`](../api/tvision_rs/event/enum.Key.html) is a closed
`enum` (`Key::Enter`, `Key::F(1)`), not a newtype. Closed sets become enums so you
get exhaustive `match`; open, app-extensible families become newtypes so the
value space stays open. That single extensibility test decides every constant
family in the port. Keys are covered with the rest of input in
[Events → enum + match](events.md).
