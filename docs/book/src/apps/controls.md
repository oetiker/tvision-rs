# Controls

Controls are the leaf views you put inside a dialog or window: buttons, check
boxes, radio buttons, input fields, captions, list boxes, and scroll bars.
You build them, insert them into a group (a [Dialog](dialogs.md) or
[Window](windows.md)), and let the [event loop](../internals/event-loop.md)
drive them. The names follow the `tv::` house style with no `T` prefix
*(each maps to the corresponding Turbo Vision `T*` class)*.

## Buttons

A [`Button`](../api/tvision_rs/widgets/struct.Button.html) is a clickable command
button — a boxed, shadowed title with an optional `~`-marked hotkey. Pressing it
(mouse, `Alt`+hotkey, or focused `Space`) fires its command, either posted as a
normal command event or, when the button is flagged broadcast, sent as a
broadcast. The default button additionally responds to `Enter`. A keyboard press
does not fire instantly: the button flashes its pressed look for a moment (a
short one-shot timer) and then fires, so the click is visible.

```rust
# use tvision_rs as tv;
# use tv::Command;
# let bounds = tv::Rect::new(0, 0, 10, 3);
use tv::widgets::{Button, ButtonFlags};
// bounds, title (~ marks the hotkey), command, flags — then insert into the dialog.
let ok = Button::new(bounds, "~O~K", Command::OK, ButtonFlags { default: true, ..Default::default() });
# let _ = ok;
```

{{#include ../screens/button.html}}

A runnable, captured example of every control on this page — buttons, check
boxes, radio buttons, input lines, and more — is in the
[widget gallery](../gallery.md).

## Check boxes & radio buttons

These share one engine, the
[`Cluster`](../api/tvision_rs/widgets/struct.Cluster.html): a single type that
branches on a
[`ClusterKind`](../api/tvision_rs/widgets/enum.ClusterKind.html), wrapped by three
named types:

- [`CheckBoxes`](../api/tvision_rs/widgets/struct.CheckBoxes.html) — independent
  on/off boxes (` [X] `); any combination may be set.
- [`RadioButtons`](../api/tvision_rs/widgets/struct.RadioButtons.html) — mutually
  exclusive options (` (•) `); exactly one is selected.
- [`MultiCheckBoxes`](../api/tvision_rs/widgets/struct.MultiCheckBoxes.html) —
  multi-state boxes that cycle through more than two values.

A cluster lays its items out top-to-bottom and wraps into a new column when the
height is exceeded — the bounds height is the column-break period. The selected
value is read and written through the [data protocol](dialogs.md) like every
other control, so a dialog gathers and scatters cluster state automatically.

## Input lines

An [`InputLine`](../api/tvision_rs/widgets/struct.InputLine.html) is a single-line
text field with selection, horizontal scrolling, clipboard cut/copy/paste, and
an optional [`Validator`](../api/tvision_rs/validate/trait.Validator.html). Its
cursor and selection track byte offsets into a real Rust `String`, so multi-byte
and wide text behave correctly.

### Read-only fields (rstv extension)

`set_read_only(true)` makes a field refuse every change to its text while
leaving it fully navigable. This is **not** the same as disabling it: a disabled
view is skipped by focus entirely, so a value too wide for its cell can never be
scrolled into view, selected, or copied. A read-only field still takes focus,
moves its caret, scrolls, select-alls and copies — only typing, deleting,
cutting and pasting are refused.

Each refusal consumes the event and broadcasts `InputLine::READ_ONLY_REJECTED`
with the field's own `ViewId` as the source, so the owner can tell the user
*which* field would not budge and why — the widget deliberately says nothing on
its own, because only the owner knows the reason. Cut and paste are also grayed
in the command set, so the menu route cannot do what the keyboard route refuses.

```rust
# use tvision_rs as tv;
# fn _demo() {
use tv::widgets::{InputLine, LimitMode};

let mut line = InputLine::new(tv::Rect::new(3, 3, 30, 4), 64, None, LimitMode::MaxBytes);
line.set_read_only(true); // reachable and copyable, but not editable
# }
```

A read-only field is drawn exactly like an editable one. Signalling the
distinction visually is left to the owner, which knows whether "read-only" means
server-maintained, computed, or merely not-yet-unlocked.

## Labels & static text

- [`StaticText`](../api/tvision_rs/widgets/struct.StaticText.html) — a read-only,
  word-wrapped block of text. Not selectable; it just paints.
- [`ParamText`](../api/tvision_rs/widgets/struct.ParamText.html) — a `StaticText`
  variant whose content you set at runtime.
- [`Label`](../api/tvision_rs/widgets/struct.Label.html) — a single-line caption
  **linked** to another control. Clicking the label, or pressing its `~`-marked
  hotkey, focuses the linked control, and the label highlights while that control
  holds focus. The link is a view handle, not a pointer.

## List boxes, scrollers & scroll bars

- [`ListBox`](../api/tvision_rs/widgets/struct.ListBox.html) — a scrollable list of
  string items. Populate it after inserting it into a group (a `Context` is
  needed to publish the scroll range to its bars), then it handles selection and
  navigation for you. [`SortedListBox`](../api/tvision_rs/widgets/struct.SortedListBox.html)
  keeps the items ordered.

  Both support an opt-in incremental find via
  [`with_find(FindMode)`](../api/tvision_rs/widgets/list_viewer/enum.FindMode.html):
  `FindMode::Highlight` owns the query and highlights the matched substring in
  every visible row; `FindMode::Filter` additionally self-filters the list to
  rows that contain the query (showing `No match: <query>` when nothing survives).
  `FindMode::Off` is the default — the classic type-to-jump lookup is unchanged
  for existing consumers. While find is active, printable keys (including Space)
  append to the query, Backspace deletes, and Esc clears (an empty-query Esc still
  propagates so a host dialog can close). The current query is readable via
  `find_query()` and clearable externally via `clear_find()`. Every query change
  broadcasts `Command::LIST_FIND_CHANGED` with the list's own `ViewId` as `source`
  — the same notify-by-broadcast pattern
  [`ScrollBar`](../api/tvision_rs/widgets/struct.ScrollBar.html) uses — so a host
  can mirror the query or drive an async search.
- [`ScrollBar`](../api/tvision_rs/widgets/struct.ScrollBar.html) — a vertical or
  horizontal bar (orientation inferred from its 1×N or N×1 bounds). It broadcasts
  a *changed* message when its value moves, naming itself as the source so a
  two-bar owner can tell which bar fired.
- [`Scroller`](../api/tvision_rs/widgets/struct.Scroller.html) — the base for
  scrollable content. It references two sibling scroll bars on the window frame,
  mirrors their value into its own scroll offset, and pushes range changes back
  to them. The wiring between a scroller and its bars is brokered by the event
  loop — see [Cross-view brokering](../internals/brokering.md).

## Validators

A validator gates what an input line accepts.
[`Validator`](../api/tvision_rs/validate/trait.Validator.html) is a trait — an
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
| [`FilterValidator`](../api/tvision_rs/validate/struct.FilterValidator.html) | only characters from a given set |
| [`RangeValidator`](../api/tvision_rs/validate/struct.RangeValidator.html) | an integer within `[min, max]` |
| [`PXPictureValidator`](../api/tvision_rs/validate/struct.PXPictureValidator.html) | text matching a Paradox picture mask |
| [`LookupValidator`](../api/tvision_rs/validate/struct.LookupValidator.html) / [`StringLookupValidator`](../api/tvision_rs/validate/struct.StringLookupValidator.html) | a value from a fixed list |
| [`RegexValidator`](../api/tvision_rs/validate/struct.RegexValidator.html) | text matching a regular expression (a modern extension) |

When a final-form check fails, the validator can pop an informational error
message box explaining what is wrong, then return focus to the field.

## Validating a field

Setting [`Options::validate`](../api/tvision_rs/view/struct.Options.html#structfield.validate)
(`ofValidate`) on a control tells the owning group to ask the control whether it
is ready to give up focus *before* moving focus away. The check runs through
[`View::valid`](../api/tvision_rs/view/trait.View.html#method.valid) with the
command `Command::RELEASED_FOCUS`.

For an `InputLine` with a validator, this is wired automatically: the input line
sets `ofValidate` for you, and its `valid` override calls the validator's
`validate` method. If the field is not yet valid, `valid` returns `false` —
which causes `Group::focus_child` to refuse the focus transfer and keep the
cursor in the current field.

```rust
# use tvision_rs as tv;
# fn _demo() {
use tv::widgets::{InputLine, LimitMode};
use tv::RangeValidator;

// An input line that only accepts integers in [1, 100].
// ofValidate is set automatically because a validator is attached.
let line = InputLine::new(
    tv::Rect::new(3, 3, 20, 4),
    64,
    Some(Box::new(RangeValidator::new(1, 100))),
    LimitMode::MaxBytes,
);
# let _ = line;
# }
```

The user cannot Tab away or dismiss the dialog with OK until every validated
field reports `is_valid`. Source: `src/view/group.rs` (`Group::focus_child`
validate gate).

## Validating without closing

Validation runs at two moments:

1. **Focus release** — `valid(Command::RELEASED_FOCUS)`: only the *current*
   (focused) control is asked. The group's `valid` override for this command
   delegates to the current child exclusively (`src/view/group.rs`).
2. **Modal close** — `valid(cmd)` for any other command (e.g. `Command::OK`):
   the group walks **all** children, calling `valid` on each and stopping at the
   first that returns `false` (a `firstThat` walk).

This means you can also call `group.valid(Command::OK, ctx)` programmatically —
from a button handler for instance — to force a full validation pass without
closing the dialog. The dialog's own `handle_event` for `Command::OK` runs
exactly this check before it calls `ctx.end_modal(Command::OK)`.

Source: `src/view/group.rs` (`Group::valid`), `src/dialog/dialog.rs`
(`Dialog::handle_event` OK branch).

> **Turbo Vision heritage:** `TGroup::valid` had the same two arms — one for
> `cmReleasedFocus` (current only) and one for all other commands (all children).
> tvision-rs ports both behaviors verbatim.

## When validation fails {#validator-error-dialogs}

When [`Validator::is_valid`](../api/tvision_rs/validate/trait.Validator.html#method.is_valid)
returns `false`, the validator's
[`error`](../api/tvision_rs/validate/trait.Validator.html#method.error) method is
called. This is where the user sees an informational message box explaining what
is wrong.

A leaf view cannot run a modal dialog inline — it holds only `&mut Context`, not
the `Program`. Instead, `error` requests the message box through the async-modal
seam:
[`Context::request_message_box`](../api/tvision_rs/view/struct.Context.html#method.request_message_box)
queues a `Deferred::RequestMessageBox` entry; the event loop builds and runs the
dialog at the end of the current pump tick. The call parameters include an
optional `answer_to: Option<ViewId>` and `then_command: Option<Command>` — when
these are `None` (the validator error case), the message box is **informational
only**: the user clicks OK, the box closes, and focus returns to the invalid
field. No round-trip answer is needed.

Here is how a concrete validator implements `error` (from `src/validate.rs`,
`FilterValidator::error`):

```rust,ignore
fn error(&self, ctx: &mut Context) {
    ctx.request_message_box(
        "Invalid character in input".to_string(),
        tvision_rs::dialog::MessageBoxKind::Error,
        tvision_rs::dialog::MessageBoxButtons::ok(),
        None,  // answer_to — no round-trip needed
        None,  // then_command
    );
}
```

When you write a custom validator, override both `is_valid` and `error`:

```rust
# use tvision_rs as tv;
use tv::validate::Validator;
use tv::view::Context;

struct NoSpaceValidator;

impl Validator for NoSpaceValidator {
    fn is_valid(&self, s: &str) -> bool {
        !s.contains(' ')
    }

    fn error(&self, ctx: &mut Context) {
        ctx.request_message_box(
            "Spaces are not allowed.".to_string(),
            tv::dialog::MessageBoxKind::Error,
            tv::dialog::MessageBoxButtons::ok(),
            None,
            None,
        );
    }
}
```

The provided `validate` method calls `is_valid` and then `error` for you —
override `is_valid` and `error` rather than `validate` itself.

Source: `src/validate.rs` (`Validator::error`, `FilterValidator::error`),
`src/view/context.rs` (`Context::request_message_box`).

## History lists

A [`THistory`](../api/tvision_rs/widgets/struct.THistory.html) dropdown icon lets
users recall previous entries for an input field. It pairs with a `u8`
**channel id** (the `history_id`) that identifies which history list the icon
reads and writes.

Place the icon immediately to the right of the input line (it is 3 cells wide:
`▐↓▌`) and pass the input's `ViewId` and the channel id:

```rust
# use tvision_rs as tv;
# fn _demo(dialog: &mut tv::Dialog) {
use tv::widgets::{InputLine, THistory, LimitMode};

// Insert the input line first to get its ViewId.
let input_id = dialog.insert_child(Box::new(
    InputLine::new(tv::Rect::new(3, 3, 30, 4), 64, None, LimitMode::MaxBytes)
));

// Place the recall icon immediately to the right.
let _hist_id = dialog.insert_child(Box::new(
    THistory::new(tv::Rect::new(30, 3, 33, 4), input_id, 42)
));
# }
```

The channel id `42` (any `u8`) is the key for the global history store. When
the user confirms an entry in the field (the input's value is saved on dialog
OK), call
[`history_add(id, text)`](../api/tvision_rs/widgets/fn.history_add.html) to push
the string into the store. On the next open, the `↓` button pops a scrollable
recall list sorted newest-first; picking an entry writes it back into the linked
input.

The store is **process-global** and **byte-budgeted** (default 1024 bytes across
all channels). The oldest entries across all channels are evicted when the budget
is exceeded. Access the store directly with
[`history_str(id, index)`](../api/tvision_rs/widgets/fn.history_str.html) and
[`history_count(id)`](../api/tvision_rs/widgets/fn.history_count.html) when you
need to pre-populate or audit entries.

The icon is **not selectable** — clicking it does not steal focus from the linked
input — and it opts into **post-processing** so it sees key events after the
focused input, leaving the `↓` arrow key available as a keyboard trigger.

Source: `src/widgets/history.rs` (`THistory`, `HistoryViewer`, `HistoryWindow`,
`history_add`, `history_str`, `history_count`).

> **Turbo Vision heritage:** `THistory` / `THistoryViewer` / `THistoryWindow`
> port one-to-one. The global byte store (`histlist.cpp`) is reimplemented as a
> thread-local `Vec<HistRec>`, dropping the original front-sentinel bookkeeping
> in favor of a cleaner read contract.

## Where to go next

- [Dialogs & data](dialogs.md) — put these controls in a modal dialog and gather
  their values.
- [Commands & events](commands.md) — how a button's command flows through the
  event model, and how to enable or disable it.
- [Writing your own View](../internals/custom-view.md) — build a control of your
  own.
