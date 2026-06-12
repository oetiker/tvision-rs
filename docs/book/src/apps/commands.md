# Commands & events

In Turbo Vision, the things that *happen* in your app — clicking OK, choosing a
menu item, a scrollbar moving — travel through the view tree as **commands** and
**events**. A button does not call a function directly; it emits a command, and
some view up the tree decides what to do with it. This indirection is what lets
the same `Command::OK` come from a button, a menu, or a keystroke and be handled
in exactly one place.

## Commands

A [`Command`](../api/tvision/command/struct.Command.html) is an opaque token
naming an intent. The framework ships a standard vocabulary as associated
constants — `Command::OK`, `Command::CANCEL`, `Command::QUIT`, `Command::CLOSE`,
and so on — each ported from a C++ `cm*` constant. Unlike C++, a command is not
an integer: its identity is a namespaced `&'static str` (so `Command::OK` is
`"tv.ok"`). That makes app- and view-defined commands collision-safe by
construction. Mint your own with
[`Command::custom`](../api/tvision/command/struct.Command.html#method.custom),
under a dotted prefix of your own:

```rust,ignore
const REFRESH: tv::Command = tv::Command::custom("myapp.refresh");
```

Commands reach the tree as events. A view emits one through its
[`Context`](../api/tvision/view/struct.Context.html):

```rust,ignore
// a targeted command, like cmXxx — handled by one view up the tree
ctx.post(REFRESH);
```

The command then rides the event loop as an
[`Event::Command`](../api/tvision/event/enum.Event.html), is offered to views in
turn, and the first one to recognise it consumes it. (How the loop walks the
tree is the subject of [The event loop in depth](../internals/event-loop.md).)

## Enabling & graying out

Every command is **enabled by default**. To make a command unavailable —
graying out the menu items and buttons that emit it — disable it. When you hold
the top-level handle (an app `main`, startup, a test), call
[`Program::disable_command`](../api/tvision/app/struct.Program.html#method.disable_command)
/ `enable_command`; from inside a view, where you only have a downward-borrowed
`Context`, request it deferred via `ctx.disable_command(cmd)` /
`ctx.enable_command(cmd)`. A view can ask whether a command is currently live
with `ctx.command_enabled(cmd)`, which answers from a per-pump snapshot.

```rust,ignore
app.disable_command(tv::Command::SAVE);   // Save menu item / button grays out
// ...later, once there is something to save:
app.enable_command(tv::Command::SAVE);
```

Internally `Program` stores the *disabled* set (a denylist), so a brand-new
custom command is enabled the moment it exists — there is no registration step.
Five window commands (`ZOOM`, `CLOSE`, `RESIZE`, `NEXT`, `PREV`) start disabled
and are granted only while a window is selected, faithful to C++
`initCommands`. The disabled command set is a
[`CommandSet`](../api/tvision/command/struct.CommandSet.html) — a set of commands
with the familiar `+=` / `-=` / union / intersection operators ported from
`TCommandSet`.

When the enabled set changes, the framework broadcasts
`Command::COMMAND_SET_CHANGED` once on the next idle so menus, buttons and the
status line re-evaluate which of their commands are live and redraw themselves
grayed or active.

## Broadcasts

A targeted command goes to whoever handles it; a **broadcast** is offered to
*every* interested view. Broadcasts are how sibling views coordinate — a
scrollbar tells its scroller it moved, a list tells its dialog an item was
chosen. Emit one through the context:

```rust,ignore
ctx.broadcast(tv::Command::SCROLL_BAR_CHANGED, Some(my_id));
```

A broadcast is an
[`Event::Broadcast { command, source }`](../api/tvision/event/enum.Event.html).
The `source` is an optional [`ViewId`](../api/tvision/view/struct.ViewId.html)
naming *which view the broadcast is about* — the resolvable successor to C++'s
`infoPtr` void-pointer. It is a filter, not a payload: a receiver checks "is this
broadcast from the scrollbar I care about?" and ignores the rest. `None` means
the broadcast concerns no particular view. Because a leaf view cannot reach
across the tree to its sibling, the event loop itself brokers the read/write
between the two views when it applies the broadcast — see
[Cross-view brokering & ViewId](../internals/brokering.md).

## Where to go next

- [Menus, status line & help](menus.md) — the views that *emit* commands and
  gray themselves out.
- [Events → enum + match](../port/events.md) — the design behind `enum Event`
  and why `infoPtr` became `source`, for Turbo Vision veterans.
- [The event loop in depth](../internals/event-loop.md) — how a command is
  routed through the tree.
