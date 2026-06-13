# Controls

Controls are the leaf views you put inside a dialog or window: buttons, check
boxes, radio buttons, input fields, captions, list boxes, and scroll bars.
You build them, insert them into a group (a [Dialog](dialogs.md) or
[Window](windows.md)), and let the [event loop](../internals/event-loop.md)
drive them. The names follow the `tv::` house style with no `T` prefix
*(each maps to the corresponding Turbo Vision `T*` class)*.

## Buttons

A [`Button`](../api/tvision/widgets/struct.Button.html) is a clickable command
button — a boxed, shadowed title with an optional `~`-marked hotkey. Pressing it
(mouse, `Alt`+hotkey, or focused `Space`) fires its command, either posted as a
normal command event or, when the button is flagged broadcast, sent as a
broadcast. The default button additionally responds to `Enter`. A keyboard press
does not fire instantly: the button flashes its pressed look for a moment (a
short one-shot timer) and then fires, so the click is visible.

```rust,ignore
use tv::widgets::{Button, ButtonFlags};
// bounds, title (~ marks the hotkey), command, flags — then insert into the dialog.
let ok = Button::new(bounds, "~O~K", Command::OK, ButtonFlags { default: true, ..Default::default() });
```

{{#include ../screens/button.html}}

A runnable, captured example of every control on this page — buttons, check
boxes, radio buttons, input lines, and more — is in the
[widget gallery](../gallery.md).

## Check boxes & radio buttons

These share one engine, the
[`Cluster`](../api/tvision/widgets/struct.Cluster.html): a single type that
branches on a
[`ClusterKind`](../api/tvision/widgets/enum.ClusterKind.html), wrapped by three
named types:

- [`CheckBoxes`](../api/tvision/widgets/struct.CheckBoxes.html) — independent
  on/off boxes (` [X] `); any combination may be set.
- [`RadioButtons`](../api/tvision/widgets/struct.RadioButtons.html) — mutually
  exclusive options (` (•) `); exactly one is selected.
- [`MultiCheckBoxes`](../api/tvision/widgets/struct.MultiCheckBoxes.html) —
  multi-state boxes that cycle through more than two values.

A cluster lays its items out top-to-bottom and wraps into a new column when the
height is exceeded — the bounds height is the column-break period. The selected
value is read and written through the [data protocol](dialogs.md) like every
other control, so a dialog gathers and scatters cluster state automatically.

## Input lines

An [`InputLine`](../api/tvision/widgets/struct.InputLine.html) is a single-line
text field with selection, horizontal scrolling, clipboard cut/copy/paste, and
an optional [`Validator`](../api/tvision/validate/trait.Validator.html). Its
cursor and selection track byte offsets into a real Rust `String`, so multi-byte
and wide text behave correctly.

## Labels & static text

- [`StaticText`](../api/tvision/widgets/struct.StaticText.html) — a read-only,
  word-wrapped block of text. Not selectable; it just paints.
- [`ParamText`](../api/tvision/widgets/struct.ParamText.html) — a `StaticText`
  variant whose content you set at runtime.
- [`Label`](../api/tvision/widgets/struct.Label.html) — a single-line caption
  **linked** to another control. Clicking the label, or pressing its `~`-marked
  hotkey, focuses the linked control, and the label highlights while that control
  holds focus. The link is a view handle, not a pointer.

## List boxes, scrollers & scroll bars

- [`ListBox`](../api/tvision/widgets/struct.ListBox.html) — a scrollable list of
  string items. Populate it after inserting it into a group (a `Context` is
  needed to publish the scroll range to its bars), then it handles selection and
  navigation for you. [`SortedListBox`](../api/tvision/widgets/struct.SortedListBox.html)
  keeps the items ordered.
- [`ScrollBar`](../api/tvision/widgets/struct.ScrollBar.html) — a vertical or
  horizontal bar (orientation inferred from its 1×N or N×1 bounds). It broadcasts
  a *changed* message when its value moves, naming itself as the source so a
  two-bar owner can tell which bar fired.
- [`Scroller`](../api/tvision/widgets/struct.Scroller.html) — the base for
  scrollable content. It references two sibling scroll bars on the window frame,
  mirrors their value into its own scroll offset, and pushes range changes back
  to them. The wiring between a scroller and its bars is brokered by the event
  loop — see [Cross-view brokering](../internals/brokering.md).

## Validators

A validator gates what an input line accepts.
[`Validator`](../api/tvision/validate/trait.Validator.html) is a trait — an
input line holds one as a boxed trait object *(the Rust-idiomatic successor to
C++ `TValidator`'s abstract base class)*. With no validator,
every keystroke is accepted. The trait has two checkpoints: `is_valid_input`
runs as each character is typed (and may auto-fill or modify the buffer), while
`is_valid` runs the final-form check when the field must be fully valid — on
focus-release or when a modal dialog's OK button is pressed.

The concrete validators port the Turbo Vision set, plus one Rust-native
addition:

| Validator | Accepts |
| --- | --- |
| [`FilterValidator`](../api/tvision/validate/struct.FilterValidator.html) | only characters from a given set |
| [`RangeValidator`](../api/tvision/validate/struct.RangeValidator.html) | an integer within `[min, max]` |
| [`PXPictureValidator`](../api/tvision/validate/struct.PXPictureValidator.html) | text matching a Paradox picture mask |
| [`LookupValidator`](../api/tvision/validate/struct.LookupValidator.html) / [`StringLookupValidator`](../api/tvision/validate/struct.StringLookupValidator.html) | a value from a fixed list |
| [`RegexValidator`](../api/tvision/validate/struct.RegexValidator.html) | text matching a regular expression (a modern extension) |

When a final-form check fails, the validator can pop an informational error
message box explaining what is wrong, then return focus to the field.

## Where to go next

- [Dialogs & data](dialogs.md) — put these controls in a modal dialog and gather
  their values.
- [Commands & events](commands.md) — how a button's command flows through the
  event model, and how to enable or disable it.
- [Writing your own View](../internals/custom-view.md) — build a control of your
  own.
