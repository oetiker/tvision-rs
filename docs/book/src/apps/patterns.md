# Common app patterns

Short answers to questions that come up when a first real application is built
on tvision-rs. Each section links to the page or API entry with the full story.

## Is my view the one receiving keys?

A view receives keyboard input when it is **focused**:
[`State::focused`](../api/tvision_rs/view/struct.State.html#structfield.focused)
is `true` when the view is the current (selected) child of its owner *and* every
owner up to the root is focused too. `Group::set_state` sends the `Focused` flag
down the chain of current children only, so in a window with two list panes
exactly one of them has `focused == true`. While a modal dialog is open, the
dialog is the current child of the root, so every view on the desktop reads
`focused == false`.

To show which pane is active, read the flag in `draw`; there is nothing to track
yourself:

```rust,ignore
fn draw(&mut self, ctx: &mut DrawCtx) {
    let active = self.state().state.focused;
    let role = if active { Role::ListFocused } else { Role::ListNormal };
    // ... draw the title / cursor row with `role`
}
```

A wrapper built with `#[delegate(to = inner)]` has exactly one `ViewState`, the
inner one. Read the flag through `self.state()`, never from a second copy; see
[Wrapping an existing view](../internals/custom-view.md#wrapping-an-existing-view-delegate).

## Reacting to a terminal resize

There is no resize event and no resize command. On every pass of the loop the
pump compares the backend size with the root size; when they differ it calls
`change_bounds` on the root group. `Group::change_bounds` then calls
`calc_bounds` and `change_bounds` on each child, so the new size cascades down
the tree under each child's [grow mode](windows.md#grow-modes-anchoring-edges).

To lay out children by hand after any resize, override
[`View::change_bounds`](../api/tvision_rs/view/trait.View.html#method.change_bounds)
(the C++ `changeBounds` override). `Splitter` does this:

```rust,ignore
#[delegate(to = group)]
impl View for TwoPanes {
    fn change_bounds(&mut self, bounds: Rect) {
        self.group.change_bounds(bounds); // grow-mode cascade to the children
        self.place_panes();               // then move panes with group.child_mut(id)
    }
}
```

Do not use `View::on_bounds_changed` for this. The pump calls it only after it
applies a bounds request made with `Context::request_bounds` (for example a
window drag) and on a few window-chrome paths. It is not called for the views
that a resize cascade reaches through `change_bounds`.

## The cursor of an embedded terminal or custom view

The hardware cursor is shown for one view only: the focused view at the end of
the current chain, and only when its
[`State::cursor_vis`](../api/tvision_rs/view/struct.State.html#structfield.cursor_vis)
is set. `View::cursor_request` implements this test, and the pump places the
cursor from it on every pass before the redraw (the `TView::resetCursor` port).

A view that hosts a terminal emulator copies the emulator cursor into its own
`ViewState` each time the emulator state changes, in view-local coordinates:

```rust,ignore
let (x, y, visible) = self.emu.cursor();
let st = self.state_mut();
st.set_cursor(x, y);
if visible { st.show_cursor() } else { st.hide_cursor() }
```

When the view loses focus the cursor disappears, and it comes back when focus
returns. This is intended: one cursor exists on the screen, and it belongs to
the view that receives the keys. Keep the position inside the view's size.

## Modal dialogs, help context and key bindings

The status line follows the focused view. On each idle pass the pump reads the
help context and hands it to the status line, which then shows the first
[`StatusDef`](../api/tvision_rs/status/struct.StatusDef.html) whose range matches.
An open menu supplies the context first; else, with a modal open, the context
comes from the modal; otherwise from the root.
In both cases `Group::get_help_ctx` returns the focused child's context and falls
back to the group's own when the child has `HelpCtx::NO_CONTEXT`. Give a dialog a
context by setting `state_mut().help_ctx` on the dialog or on one of its controls.

Status-line keys are routed **before** any view sees them, also while a modal is
open. When a key matches an item of the current def (including hidden
`key_item` bindings) and its command is enabled, the status line turns the key
event into that command. The focused view never receives the key.

```rust,ignore
let defs = StatusDef::list()
    .def_one_of([HelpCtx::custom("app.containers")], |d| {
        d.item("~Del~ Remove", KeyEvent::from(Key::Delete), CMD_REMOVE)
    })
    .def_all(|d| d.item("~Alt-X~ Exit", alt('x'), Command::QUIT))
    .build();
```

An app-wide def must therefore not bind keys that dialogs and editors need, such
as Enter, Esc, Tab or Del: an input line would never see Del. Put such bindings
in a def that matches only the help context of the view that owns them, as above.
The [keymap](keyboard.md) is different: it is read only by the editor and the
input line to map keys to editing commands, and does not route keys for the app.
See [Menus, status line & help](menus.md#context-sensitive-help).

## Getting results out of a modal

From application code that holds `&mut Program`, use
[`Program::exec_view_with`](../api/tvision_rs/app/struct.Program.html#method.exec_view_with):
its closure reads the result from the modal before the modal is dropped.

From inside a view, which only has `&mut Context`, call
`ctx.request_exec_view(dialog, my_id, Some(CMD_DONE))`. When the modal closes,
the pump calls three things on the requesting view, in this order: it gives the
dialog's gathered record to `set_modal_data` (skipped when the close command is
`Command::CANCEL`), gives the close command to `set_modal_answer`, and posts
`CMD_DONE`. The two hooks have no `Context`, so store what they receive and act
when `CMD_DONE` arrives in `handle_event`:

```rust,ignore
fn set_modal_data(&mut self, data: FieldValue) {
    if let FieldValue::List(fields) = data
        && let Some(FieldValue::Text(name)) = fields.into_iter().next()
    {
        self.pending_name = Some(name); // used when CMD_DONE arrives
    }
}
```

Full details: [Modal execView → one loop](../port/modal.md#getting-the-data-back)
and [The data-movement model](data-movement.md#choosing-how-a-modal-returns-its-result).

## Feeding async results into the UI

Two hooks exist. Both run only on idle passes, which are passes with no input
event. With no input they come about every 20 ms, sooner when a timer is due.

- [`Program::set_on_idle`](../api/tvision_rs/app/struct.Program.html#method.set_on_idle)
  holds one callback with `&mut Program`. It also runs while a modal is open.
  `Program` has no method to post or broadcast an event, so the callback cannot
  notify views directly. Use it to update shared state (`Rc<RefCell<_>>`) that
  views read in `draw`; the next redraw shows the change.
- A view-owned periodic timer:
  [`Context::set_timer`](../api/tvision_rs/view/struct.Context.html#method.set_timer)
  with a period. Each expiry arrives as `Event::Timer(id)` and is delivered to
  every view in the tree like a broadcast, also while a modal is open. The view
  can drain a channel and call `ctx.broadcast(REFRESH, None)` so other views
  update themselves. The timer is not tied to its view: call
  `ctx.kill_timer(id)` when the work ends.

Use the timer when views must react (rebuild a list, change commands); use the
idle hook for state that is only drawn. The `external_state` example shows the
timer pattern end to end; see also
[Background work each idle pass](../internals/event-loop.md#background-work-each-idle-pass).

## Mouse double-click

The loop marks a mouse press as a double or triple click when the same buttons
are pressed on the same cell within the double-click interval. Read the flags in
the `MouseDown` handler:

```rust,ignore
Event::MouseDown(m) if m.flags.double_click => {
    self.open_selected(ctx);
    ev.clear();
}
```

[`MouseEventFlags`](../api/tvision_rs/event/struct.MouseEventFlags.html) also has
`triple_click`. The interval is 440 ms by default; change it with
[`Program::set_double_delay_ms`](../api/tvision_rs/app/struct.Program.html#method.set_double_delay_ms).

## The `Program::new` factories

`Program::new` takes three factories, each an
`impl FnOnce(Rect) -> Option<Box<dyn View>>`, for the desktop, the status line
and the menu bar. Each one runs once, inside `Program::new`, and receives the
full screen rectangle; it shrinks the rectangle to its own row or area. Return
`None` to leave out the status line or the menu bar. Because the factories are
`FnOnce`, they can move app handles in:

```rust,ignore
let program = Program::new(
    backend,
    Box::new(SystemClock::new()),
    Theme::classic_blue(),
    move |r| init_desktop(r, state.clone(), rx), // moves the Receiver in
    init_status_line,
    init_menu_bar,
);
```

After construction, `Program::desktop`, `status_line` and `menu_bar` return the
`ViewId` of each part. See [The application skeleton](../getting-started/skeleton.md)
and `examples/external_state.rs`.
