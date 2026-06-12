# Dialogs & data

A **dialog** is a modal window: it appears on top of everything, captures input
until the user dismisses it, and returns a single answer — which button closed
it. [`Dialog`](../api/tvision/dialog/struct.Dialog.html) is the port of C++
`TDialog`. It *is* a [`Window`](../api/tvision/window/struct.Window.html) (it
embeds one and delegates to it) with dialog-specific behaviour layered on:
`Esc` cancels, `Enter` accepts the default button, and the frame carries only
the move and close affordances — no grow, no zoom.

## Building a dialog

Construct the dialog with a rectangle and an optional title, then populate it
with child views — buttons, input lines, checkboxes, labels — via
[`insert_child`](../api/tvision/dialog/struct.Dialog.html#method.insert_child):

```rust,ignore
let mut dialog = Dialog::new(Rect::new(0, 0, 40, 11), Some("Sign in".into()));

// insert_child returns the child's ViewId, so you can reach it later.
let name = dialog.insert_child(Box::new(InputLine::with_limit(
    Rect::new(3, 4, 34, 5),
    64, // byte limit (the convenience ctor uses LimitMode::MaxBytes)
)));
dialog.insert_child(Box::new(Button::new(
    Rect::new(15, 8, 25, 10),
    "~O~K",
    Command::OK,
    ButtonFlags { default: true, ..Default::default() },
)));
```

`InputLine::new` takes a validator and a [`LimitMode`](../api/tvision/widgets/enum.LimitMode.html)
too; [`with_limit`](../api/tvision/widgets/struct.InputLine.html#method.with_limit) is
the no-validator, byte-limit shortcut. `ButtonFlags` is a struct of named bools,
so the default button is `ButtonFlags { default: true, .. }`.

A button carries the [`Command`](commands.md) it broadcasts when pressed. The
conventional end commands are `Command::OK` and `Command::CANCEL`; a message box
also uses `Command::YES` / `Command::NO`.

## Running it modally

You do not insert a dialog into the view tree yourself. You hand it to
[`Program::exec_view`](../api/tvision/app/struct.Program.html#method.exec_view),
the port of C++ `TGroup::execView`. It inserts the dialog at the top of the
tree, marks it modal, gives it focus, and spins the **same** event loop until
the dialog ends itself — then removes it and hands back the closing command:

```rust,ignore
match program.exec_view(Box::new(dialog)) {
    Command::OK     => { /* read the fields back, act on them */ }
    Command::CANCEL => { /* user backed out */ }
    _ => {}
}
```

There is no separate "modal loop." In C++ each `execView` was a fresh nested
`getEvent` loop; rstv collapses them into one loop plus a **capture stack**
(see [Modal execView → one loop + capture](../port/modal.md) and
[the event loop in depth](../internals/event-loop.md)). `exec_view` is
**top-level only**: a view holds only a `&mut Context`, never the `Program`, so
it *cannot* re-enter the loop from inside `handle_event` — which is exactly what
keeps the single loop sound.

> A modal must have a path to closing itself, or it hangs. `Dialog` provides one
> out of the box: `Esc` becomes a `Cancel`, the default button becomes an `OK`.
> If you build a bare modal with neither, nothing will end it.

The framework ships ready-made modals built on this path —
[`message_box`](../api/tvision/app/struct.Program.html#method.message_box) for a
titled alert with Yes/No/OK/Cancel buttons, and
[`input_box`](../api/tvision/app/struct.Program.html#method.input_box) for a
single labelled text field. Both build a `Dialog`, run it through `exec_view`,
and return the user's answer.

## Moving data in and out

Turbo Vision moves dialog data with `getData`/`setData`: every control
`memcpy`s its value into an untyped record. rstv replaces that untyped blob
with a **typed value currency** —
[`FieldValue`](../api/tvision/data/enum.FieldValue.html) —
passed through the `value` / `set_value` pair on the
[`View`](../api/tvision/view/trait.View.html) trait. A text field reads and
writes `FieldValue::Text`; an integer control uses `FieldValue::Int`. The enum
**grows as controls need it**.

Two operations bracket a dialog:

| Turbo Vision | rstv | Direction |
| ------------ | ------- | --------- |
| `setData` | scatter — `set_value` on each field | seed the dialog before showing it |
| `getData` | gather — `value()` on each field | read results after `OK` |

For a single-field dialog you call `set_value`/`value` on that one field
directly — which is exactly what `input_box` does internally to seed and read
its lone input line. For a multi-field dialog, the
[`Group`](../api/tvision/view/struct.Group.html) behind the dialog walks its
children in order: `gather_data` collects a `Vec<Option<FieldValue>>` (one slot
per child, `None` where a child has no transferable value), and `scatter_data`
distributes a matching vector back in the same child order. Seed before
`exec_view`, gather after it returns `Command::OK`.

## See also

- [Windows & the desktop](windows.md) — the non-modal sibling of a dialog.
- [Controls](controls.md) — the buttons, input lines, and clusters you place
  inside a dialog.
- [Commands & events](commands.md) — how a button's command travels and how
  `OK`/`Cancel` end the modal.
