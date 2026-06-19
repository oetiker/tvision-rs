# Dialogs & data

A **dialog** is a modal window: it appears on top of everything, captures input
until the user dismisses it, and returns a single answer — which button closed
it. [`Dialog`](../api/tvision-rs/dialog/struct.Dialog.html) embeds a
[`Window`](../api/tvision-rs/window/struct.Window.html) and delegates to it, with
dialog-specific behaviour layered on: `Esc` cancels, `Enter` accepts the default
button, and the frame carries only the move and close affordances — no grow, no
zoom *(the tvision-rs equivalent of C++ `TDialog`)*.

## Building a dialog

Construct the dialog with a rectangle and an optional title, then populate it
with child views — buttons, input lines, checkboxes, labels — via
[`insert_child`](../api/tvision-rs/dialog/struct.Dialog.html#method.insert_child):

```rust
# use tvision_rs as tv;
# use tv::{Command, Dialog, Rect};
# use tv::widgets::{Button, ButtonFlags, InputLine};
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
# let _ = name;
```

`InputLine::new` takes a validator and a [`LimitMode`](../api/tvision-rs/widgets/enum.LimitMode.html)
too; [`with_limit`](../api/tvision-rs/widgets/struct.InputLine.html#method.with_limit) is
the no-validator, byte-limit shortcut. `ButtonFlags` is a struct of named bools,
so the default button is `ButtonFlags { default: true, .. }`.

A button carries the [`Command`](commands.md) it broadcasts when pressed. The
conventional end commands are `Command::OK` and `Command::CANCEL`; a message box
also uses `Command::YES` / `Command::NO`.

A dialog assembled this way — a labelled input, check boxes, and the two
buttons — looks like this:

{{#include ../screens/dialog.html}}

The runnable source is the `dialog` entry in the [widget gallery](../gallery.md).

## Running it modally

You do not insert a dialog into the view tree yourself. You hand it to
[`Program::exec_view`](../api/tvision-rs/app/struct.Program.html#method.exec_view).
It inserts the dialog at the top of the tree, marks it modal, gives it focus,
and spins the **same** event loop until the dialog ends itself — then removes it
and hands back the closing command:

```rust
# use tvision_rs as tv;
# use tv::{Command, Dialog};
# fn _demo(program: &mut tv::Program, dialog: Dialog) {
match program.exec_view(Box::new(dialog)) {
    Command::OK     => { /* read the fields back, act on them */ }
    Command::CANCEL => { /* user backed out */ }
    _ => {}
}
# }
```

There is no separate "modal loop." tvision-rs runs a single event loop plus a
**capture stack**: `exec_view` pushes the dialog as the capture target, and the
loop drives it until the dialog closes itself (see
[Modal execView → one loop + capture](../port/modal.md) and
[the event loop in depth](../internals/event-loop.md)). `exec_view` is
**top-level only**: a view holds only a `&mut Context`, never the `Program`, so
it *cannot* re-enter the loop from inside `handle_event` — which is exactly what
keeps the single loop sound.

> **Turbo Vision heritage:** in C++ each `TGroup::execView` spun a fresh nested
> `getEvent` loop. tvision-rs collapses all modal nesting into one loop + capture
> stack, eliminating the reentrancy entirely.

> A modal must have a path to closing itself, or it hangs. `Dialog` provides one
> out of the box: `Esc` becomes a `Cancel`, the default button becomes an `OK`.
> If you build a bare modal with neither, nothing will end it.

The framework ships ready-made modals built on this path —
[`message_box`](../api/tvision-rs/app/struct.Program.html#method.message_box) for a
titled alert with Yes/No/OK/Cancel buttons, and
[`input_box`](../api/tvision-rs/app/struct.Program.html#method.input_box) for a
single labelled text field. Both build a `Dialog`, run it through `exec_view`,
and return the user's answer.

## Moving data in and out

Dialog data flows through a **typed value currency** —
[`FieldValue`](../api/tvision-rs/data/enum.FieldValue.html) — passed through the
`value` / `set_value` pair on the
[`View`](../api/tvision-rs/view/trait.View.html) trait. A text field reads and
writes `FieldValue::Text`; an integer control uses `FieldValue::Int`. The enum
**grows as controls need it** *(this replaces the C++ `getData`/`setData` pair,
which moved data through an untyped `memcpy` record)*.

Two operations bracket a dialog:

| Turbo Vision | tvision-rs | Direction |
| ------------ | ------- | --------- |
| `setData` | scatter — `set_value` on each field | seed the dialog before showing it |
| `getData` | gather — `value()` on each field | read results after `OK` |

For a single-field dialog you call `set_value`/`value` on that one field
directly — which is exactly what `input_box` does internally to seed and read
its lone input line. For a multi-field dialog, the
[`Group`](../api/tvision-rs/view/struct.Group.html) behind the dialog walks its
children in order: `gather_data` collects a `Vec<Option<FieldValue>>` (one slot
per child, `None` where a child has no transferable value), and `scatter_data`
distributes a matching vector back in the same child order. Seed before
`exec_view`, gather after it returns `Command::OK`.

## The full `FieldValue` currency

`FieldValue` is the **single typed currency** for all data exchange in tvision-rs.
The well-known shapes are:

| Variant | Produced by |
| ------- | ----------- |
| `Text(String)` | `InputLine`, `Editor` |
| `Int(i64)` | numeric inputs |
| `Bool(bool)` | single-item toggle controls |
| `Bits(u32)` | `CheckBoxes` / `RadioButtons` cluster |
| `List(Vec<FieldValue>)` | `Group::gather_list` / `scatter_list` |
| `Custom(Rc<dyn CustomValue>)` | your own controls |

`Bool` carries a simple toggle; `Bits` carries a bitmask from a cluster (each
`CheckBoxes` or `RadioButtons` bit maps to one check or radio item in order).

### Ordered-record gather and scatter

When you want the whole dialog's data as **one value** rather than one field at a
time, use the ordered-record pair on
[`Group`](../api/tvision-rs/view/struct.Group.html):

```rust,ignore
// Seed before exec_view — positional, same child order as insert_child.
// scatter_list takes a &mut Context (available inside a handle_event / the pump):
let initial = FieldValue::List(vec![
    FieldValue::Text("Alice".into()),
    FieldValue::Bits(0b01),  // first checkbox ticked
]);
dialog.scatter_list(&initial, ctx);

// Read after exec_view returns Command::OK:
let record = dialog.gather_list();
// record == FieldValue::List([FieldValue::Text(...), FieldValue::Bits(...)])
```

`gather_list` walks the group's children in insertion order and collects each
child's `value()` into a `FieldValue::List` — the positional equivalent of C++
`getData`'s `memcpy` record. `scatter_list` distributes the list back in the same
order via `set_value`. Children with no transferable value (labels, decorative
views) are skipped in both directions; the list contains only the slots that
produce a value, so the index order matches the sequence of *data-bearing* children.

### The `Custom` seam and third-party controls

For richer payloads — a date range, a colour swatch, a structured selection — a
control can return `FieldValue::Custom(...)`. See
[Third-party components & data interchange](extensibility.md) for the full open
story: the three exchange paths, `value_as::<T>()` / `as_custom::<T>()`, the
TypeId caveat, and when to skip `FieldValue` entirely.

## See also

- [Windows & the desktop](windows.md) — the non-modal sibling of a dialog.
- [Controls](controls.md) — the buttons, input lines, and clusters you place
  inside a dialog.
- [Commands & events](commands.md) — how a button's command travels and how
  `OK`/`Cancel` end the modal.
- [Third-party components](extensibility.md) — the `Custom` seam and the three
  open exchange paths for your own controls.
