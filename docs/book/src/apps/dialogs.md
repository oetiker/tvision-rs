# Dialogs & data

A **dialog** is a modal window: it appears on top of everything, captures input
until the user dismisses it, and returns a single answer — which button closed
it. [`Dialog`](../api/tvision_rs/dialog/struct.Dialog.html) embeds a
[`Window`](../api/tvision_rs/window/struct.Window.html) and delegates to it, with
dialog-specific behaviour layered on: `Esc` cancels, `Enter` accepts the default
button, and the frame carries only the move and close affordances — no grow, no
zoom *(the tvision-rs equivalent of C++ `TDialog`)*.

## Building a dialog

Construct the dialog with a rectangle and an optional title, then populate it
with child views — buttons, input lines, checkboxes, labels — via
[`insert_child`](../api/tvision_rs/dialog/struct.Dialog.html#method.insert_child):

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

`InputLine::new` takes a validator and a [`LimitMode`](../api/tvision_rs/widgets/enum.LimitMode.html)
too; [`with_limit`](../api/tvision_rs/widgets/struct.InputLine.html#method.with_limit) is
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
[`Program::exec_view`](../api/tvision_rs/app/struct.Program.html#method.exec_view).
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
[`message_box`](../api/tvision_rs/app/struct.Program.html#method.message_box) for a
titled alert with Yes/No/OK/Cancel buttons, and
[`input_box`](../api/tvision_rs/app/struct.Program.html#method.input_box) for a
single labelled text field. Both build a `Dialog`, run it through `exec_view`,
and return the user's answer.

## Moving data in and out

Dialog data flows through a **typed value currency** —
[`FieldValue`](../api/tvision_rs/data/enum.FieldValue.html) — passed through the
`value` / `set_value` pair on the
[`View`](../api/tvision_rs/view/trait.View.html) trait. A text field reads and
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
[`Group`](../api/tvision_rs/view/struct.Group.html) behind the dialog walks its
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
[`Group`](../api/tvision_rs/view/struct.Group.html):

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

## Delivering a modal's result back to its launcher

How a modal's result reaches the view that opened it depends on *who* opened it.

**`Program`-launched modals** — opened from outside the view tree (for example
the color picker or theme editor) — return their result directly: the pump runs
`exec_view_with<R>`, the closure reads the modal's state while it is still in
the tree, and the result comes back as a plain Rust value. This is the right path
when the result type is rich and does not map naturally to `FieldValue` (such as
`Color` or `Theme`).

**View-launched modals** — opened from inside `handle_event` via the
[Deferred channel](../internals/deferred.md) — cannot return inline (a view holds
only `&mut Context`, not the `Program`). Instead, after the modal closes, the pump
reads each dialog field by id via `View::value()` and delivers the assembled
result to the launcher by id via `View::set_modal_data(FieldValue)` (virtual
dispatch, no downcast). The launcher overrides `set_modal_data` to interpret the
ordered `FieldValue::List` and update its own state. The editor's Find/Replace
modals follow this pattern: the pump reads the text field as `FieldValue::Text`
and the options cluster as `FieldValue::Bits`, then calls `editor.set_modal_data(List([Text, Bits]))`.

Note that `set_modal_data` is distinct from `set_value`: `set_value` carries a
view's *own* document or field data (the D10 scatter path); `set_modal_data`
carries a *modal's result* collected by the pump on the launcher's behalf.

## Tab order and data transfer

Both **Tab navigation** and **gather/scatter data order** follow the single
`children` `Vec` in the group — specifically, the **insertion order** (oldest
first, as stored in `children[0]`, `children[1]`, …). The children are stored
back-to-front for painting, and forward for data transfer: `gather_data` walks
them oldest-first and returns one `Option<FieldValue>` per child.

```text
inserted 1st → children[0]  (Tab: first stop; gather: slot 0)
inserted 2nd → children[1]  (Tab: second stop; gather: slot 1)
…
inserted last → children[n] (Tab: last stop; gather: slot n)
```

`Tab` calls `Group::focus_next(forwards: true, ctx)`, which iterates
`children` in forward order and wraps around. `Shift-Tab` iterates in
reverse. **Disabled or invisible children are skipped** — they remain in the
`children` slice but `focus_next` steps over them.

When you assemble a dialog, insert controls in the order you want the Tab key to
visit them — typically top-to-bottom, then left-to-right. Buttons are usually
inserted last so they receive focus after all the data fields.

Gather (the dialog walking its children to collect values) follows the same
forward order. Scatter distributes values in the **same** order. The index in
the `Vec<Option<FieldValue>>` that `gather_data` returns corresponds 1:1 to the
insertion slot: index 0 is the first-inserted child, index 1 is the second, and
so on.

```rust
# use tvision_rs as tv;
# use tv::{Command, Dialog, Rect};
# use tv::widgets::{Button, ButtonFlags, InputLine, LimitMode};
# #[allow(unused_variables)]
# fn _demo() {
let mut dialog = Dialog::new(Rect::new(0, 0, 40, 10), Some("Login".into()));

// Insert in Tab order: name first, password second, button last.
let name_id   = dialog.insert_child(Box::new(InputLine::new(Rect::new(3, 3, 34, 4), 64, None, LimitMode::MaxBytes)));
let pass_id   = dialog.insert_child(Box::new(InputLine::new(Rect::new(3, 5, 34, 6), 64, None, LimitMode::MaxBytes)));
let _btn_id   = dialog.insert_child(Box::new(Button::new(Rect::new(15, 7, 25, 9), "~O~K", Command::OK, ButtonFlags { default: true, ..Default::default() })));

// After exec_view returns Command::OK, gather in the same order:
// gathered[0] = name, gathered[1] = password, gathered[2] = None (button has no value).
# }
```

Source: `src/view/group.rs` (`Group::gather_data`, `Group::scatter_data`,
`Group::focus_next`).

> **Turbo Vision heritage:** in C++ `TGroup::getData`/`setData` walked the same
> circular sibling ring in forward order; the ring order was insertion order
> (newest at the front, so a full walk from `last->next` gives oldest first). The
> tvision-rs `Vec` stores oldest at index 0, preserving the same forward walk.

## The change-directory dialog

[`ChDirDialog`](../api/tvision_rs/dialog/struct.ChDirDialog.html) is a ready-made
directory chooser: a path input line with a history recall icon, a collapsible
directory tree (`DirListBox`), and action buttons. Build it with
[`ChDirDialog::new`](../api/tvision_rs/dialog/struct.ChDirDialog.html#method.new):

```rust
# use tvision_rs as tv;
# fn _demo(program: &mut tv::Program) {
use tv::dialog::{ChDirDialog, CD_NORMAL};

let mut cd = ChDirDialog::new(CD_NORMAL, 0);
let result = program.exec_view(Box::new(cd));
# let _ = result;
# }
```

The `opts` parameter is a bitmask:

| Constant | Effect |
| --- | --- |
| `CD_NORMAL` | Standard dialog, loads the current directory tree on open |
| `CD_NO_LOAD_DIR` | Skip the initial directory scan (faster first open) |
| `CD_HELP_BUTTON` | Add a Help button at the right |

The second argument is the `history_id` (`u8`) for the path input's recall
dropdown — use a non-zero id to enable history recall, or `0` to skip it (the
gallery example uses `0`).

When the user navigates the tree and double-clicks a directory (or types a path
and presses Chdir), the dialog writes the chosen path into its input field and
dismisses with `Command::OK`. Read the result from the dialog's input:

```rust,ignore
use tv::dialog::{ChDirDialog, CD_NORMAL};
use tv::data::FieldValue;

let mut cd = ChDirDialog::new(CD_NORMAL, 0);
if program.exec_view(Box::new(cd)) == tv::Command::OK {
    // The path is available after exec_view returns — gather from the child
    // or std::env::current_dir() if ChDirDialog has already changed directories.
    if let Ok(path) = std::env::current_dir() {
        println!("Changed to {}", path.display());
    }
}
```

The dialog is growable (the `WindowFlags::grow` flag is set internally) so the
user can resize it, and the tree grows with the dialog bounds via `GrowMode`.

Source: `src/dialog/filedlg.rs` (`ChDirDialog::new`, `DirListBox`, `DirCollection`).

## See also

- [Windows & the desktop](windows.md) — the non-modal sibling of a dialog.
- [Controls](controls.md) — the buttons, input lines, and clusters you place
  inside a dialog.
- [Commands & events](commands.md) — how a button's command travels and how
  `OK`/`Cancel` end the modal.
- [Third-party components](extensibility.md) — the `Custom` seam and the three
  open exchange paths for your own controls.
