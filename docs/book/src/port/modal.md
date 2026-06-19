# Modal `execView` → one loop

In C++ Turbo Vision, modality is recursion. `TGroup::execView` spins a **nested,
blocking `getEvent` loop** inside the already-running one; the outer loop is
suspended on the call stack while the inner one runs, and the modal view ends it
by calling `endModal`. The same trick drives `dragView` and a pressed button's
press-and-hold tracking.

Rust will not let you do that. A nested loop would have to re-borrow the view
tree that the outer loop already holds `&mut` to — the borrow checker refuses,
and there is no `&mut self`-reentrancy to lean on. So the nested loops collapse
into **one** non-recursive event loop, and modality becomes a handler on the
capture stack rather than a new loop. [Event capture](capture.md) is the general
mechanism; modality is one use of it.

## Modality as a handler

The modal handler is `ModalFrame`. While it sits on the capture stack it lets
keyboard, command, and broadcast events pass through to normal routing — which
reaches the modal view because the group focuses it — while positional (mouse)
events are gated by the modal view's bounds: inside, they pass; outside, they are
consumed and swallowed, so views beneath the dialog never see the click. That
gate is exactly what "modal" means.

## `exec_view` steers the one loop

[`Program::exec_view`](../api/tvision_rs/app/struct.Program.html#method.exec_view) is
the blocking wrapper that replaces `execView`. It inserts the view, makes it
current, pushes a `ModalFrame`, then runs the *same*
[`pump_once`](../api/tvision_rs/app/struct.Program.html#method.pump_once) loop until
the view calls [`end_modal`](../api/tvision_rs/view/struct.Context.html#method.end_modal),
setting the end state. Then it pops the frame, removes the view, restores the
previous focus and command set, and returns the chosen
[`Command`](../api/tvision_rs/command/struct.Command.html). No new loop is spun —
`exec_view` just steers the one loop that was already running.

See [Dialogs & data](../apps/dialogs.md) for the user-facing recipe, [Event
capture](capture.md) for the mechanism modality shares with drag/resize and
press-and-hold, and [the event loop in depth](../internals/event-loop.md) for
what each `pump_once` turn does.

## Choosing the right launch path

How you launch a modal depends on **what calls it**:

| Caller | Launch method | Result delivery |
|--------|---------------|-----------------|
| A `Program` / `Application` method | [`Program::exec_view_with`](../api/tvision_rs/app/struct.Program.html#method.exec_view_with) | returned by value from the `extract` closure |
| A `View` (inside `handle_event`) | `Context::request_exec_view` | close command routed to `requester` via `View::set_modal_answer`; optional `then_command` re-injected |

A view holds only `&mut Context`, never `&mut Program`, so it cannot call
`exec_view_with` inline. Calling `ctx.request_exec_view(view, requester,
then_command)` instead queues a `Deferred::OpenModal`
and returns immediately; the pump moves the boxed view into the existing
`pending_modal` slot, runs it via the same single-loop machinery, and on close
delivers the result to `requester` and re-injects `then_command`. No new loop
is spun, and no new `ModalCompletion` variant is needed.

## Getting a result back: `exec_view_with`

C++ `execView` returns a `ushort` end command; the caller then reads results out
of the still-live dialog with `getData` before it is destroyed. tvision-rs keeps
that shape with
[`Program::exec_view_with`](../api/tvision_rs/app/struct.Program.html#method.exec_view_with):
it runs the modal, then — at the **pre-drop window**, while the view is still in
the tree — hands your `extract` closure the modal's `&mut dyn View` and the end
command. Whatever the closure returns is handed straight back, **by value**:

```rust,ignore
let chosen: Option<Color> = program.exec_view_with(Box::new(dialog), |modal, cmd| {
    (cmd == Command::OK)
        .then(|| read_the_color_out_of(modal))
        .flatten()
});
```

There is no shared `Rc<Cell>` sink and no `dyn Any` in the framework: the result
type `R` is named by the caller, never by the framework. This is the by-value
successor to the old per-dialog `ModalCompletion` "sink" variants. A single field
crosses as a [`FieldValue`](../api/tvision_rs/data/enum.FieldValue.html) via
`View::value`; a richer native value (a `Color`, a whole `Theme`) is returned
directly from `extract` — `Color`/`Theme` are deliberately not `FieldValue`s.

## Launching a modal from a view: `Context::request_exec_view`

Use `request_exec_view` when the modal is triggered from inside a `handle_event`
implementation (i.e., from any `View`). The worked example is `tcv`'s Info box:
the `DirBox` list view builds a custom read-only `Dialog` and launches it when
the user presses Enter or double-clicks an entry.

### Building the dialog

{{#rustdoc_include ../../../../examples/tcv.rs:info_dialog}}

### Launching it

Inside `DirBox::handle_event` (which receives `&mut Context`, not `&mut Program`):

```rust,ignore
// Illustrative sketch — the real code lives in examples/tcv.rs (DirBox::open_info).
fn open_info(&mut self, ctx: &mut Context) {
    if let Some(e) = CATALOG.get(self.lv.focused as usize) {
        let dialog = build_info_dialog(e);
        if let Some(id) = self.state().id() {
            ctx.request_exec_view(Box::new(dialog), id, None);
        }
    }
}
```

`request_exec_view` queues `Deferred::OpenModal`. The pump picks it up at the
bottom of the same turn, stashes the boxed `Dialog` into `pending_modal` with a
`RouteModalAnswer { answer_to: id, then_command: None }` completion, and runs it
via the existing single-loop machinery. When the user presses OK (which posts
`Command::CANCEL` — the read-only-info convention: dismiss = `cmCancel`) the
pump delivers that command to `DirBox` via `set_modal_answer`. The base default
for `set_modal_answer` silently discards the command, which is correct here —
the Info box is read-only and `then_command` is `None`, so no follow-up action
is needed.

### Data-back path

The result is the close command only. A future input dialog that needs the
modal's typed `FieldValue` result back would override `set_modal_answer` on the
requester to cache the command, then read `modal_id.value()` (or call
`requester.set_modal_data(...)`) on the `then_command` re-injection. That path
is not built today because no current consumer needs it.

## Ending a modal (execView)

The full lifecycle of `exec_view` maps each C++ step to a tvision-rs equivalent:

| C++ `execView` step | tvision-rs |
| -------------------- | ---------- |
| Save and clear command set | Save `disabled_commands`; the modal starts with its own restrictions |
| `saveOwner`; insert into desktop | Root-insert into the program's group (not the desktop); own the view |
| `setState(sfModal, True)` | `state.state.modal = true` (direct field write) |
| `setCurrent(p, enterSelect)` | `group.set_current(id, SelectMode::Enter, &mut ctx)` |
| Push capture — none in C++ | Push a `ModalFrame` onto the capture stack |
| `p->execute()` — a nested loop | Steer the **same** pump loop; no new loop |
| `ClearEvent` on every unhandled event | The `ModalFrame` swallows outside-bounds mouse events |
| Pop capture — none in C++ | Pop the `ModalFrame` |
| `remove(p)`; restore focus | `group.remove(id, &mut ctx)`; restore previous current |
| Restore command set | Restore `disabled_commands` |
| Return `endState` | Return the `Command` that was passed to `end_modal` |

The capture-stack push and pop are the key addition over the C++ design: they
replace the nested loop's ability to intercept all events while the dialog runs.
Because the push is immediate (not deferred — `exec_view` holds `&mut self`
directly), the `ModalFrame` is live from the very first pump pass inside the
modal.

**Sources:** `exec_view_with_completion` in `src/app/program.rs`;
`ModalFrame` in `src/app/program.rs`; `CaptureStack` in `src/capture.rs`.

> **Turbo Vision heritage:** ports `TGroup::execView` (`tgroup.cpp`). The C++
> version spun a nested `getEvent` loop; in tvision-rs the nested loop is
> replaced by `ModalFrame` on the capture stack plus the shared `pump_once`
> loop (deviation D9).

## The modal loop (Execute)

tvision-rs has **one** event loop, period. The `Program::run` skeleton is:

```rust,ignore
// src/app/program.rs — Program::run (simplified)
loop {
    self.end_state = None;
    while self.end_state.is_none() {
        self.pump_and_drive();    // one pump pass: event → dispatch → redraw
    }
    let es = self.end_state.unwrap();
    if self.valid_end(es) {
        return es;
    }
}
```

`exec_view` runs the **same** `pump_and_drive` loop in a fresh `while` block
with its own `end_state`:

```rust,ignore
// src/app/program.rs — exec_view_with_completion (simplified inner loop)
loop {
    self.end_state = None;
    while self.end_state.is_none() {
        self.pump_and_drive();  // exactly the same pump
    }
    let es = self.end_state.unwrap();
    if self.validate_modal_close(id, es) {
        break es;
    }
}
```

There is no new thread, no async runtime, no re-borrow. The difference between
the outer `run` loop and the inner `exec_view` loop is only *which end-state
terminates them*: the outer loop ends when the whole application quits; the
inner loop ends when a view inside the dialog calls `end_modal`. When the
inner loop exits it restores `end_state` to its pre-modal value, so the outer
loop does not spuriously see the modal's end command.

A modal that opens another modal (e.g. a file dialog opening a history popup)
adds a second `exec_view` frame on top; this is safe because each frame owns
its own `end_state` snapshot and its own `ModalFrame` on the capture stack.

**Sources:** `Program::run` and `exec_view_with_completion` in
`src/app/program.rs`.

> **Turbo Vision heritage:** ports `TGroup::execute` (`tgroup.cpp`). C++ had
> one `execute` per active modal, each with its own `getEvent` loop. tvision-rs
> has one `pump_and_drive` loop shared by all levels; modality is a stack of
> `ModalFrame` capture handlers and `end_state` save/restore frames.

## endModal

A view signals "close this modal and return result `cmd`" by calling
[`ctx.end_modal(cmd)`](../api/tvision_rs/view/struct.Context.html#method.end_modal)
on its [`Context`](../api/tvision_rs/view/struct.Context.html). This **queues**
[`Deferred::EndModal(cmd)`](../api/tvision_rs/view/enum.Deferred.html) rather than
acting immediately, because a view's `handle_event` runs inside the dispatch
borrow and cannot reach the loop-owned `end_state` directly:

```rust
# use tvision_rs as tv;
# use tv::event::Event;
# use tv::view::{View, ViewState, Context, DrawCtx};
# struct OkButton { state: ViewState }
# impl View for OkButton {
#     fn state(&self) -> &ViewState { &self.state }
#     fn state_mut(&mut self) -> &mut ViewState { &mut self.state }
#     fn draw(&mut self, _ctx: &mut DrawCtx) {}
fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
    if let Event::Command(cmd) = ev {
        if *cmd == tv::command::Command::OK {
            ctx.end_modal(tv::command::Command::OK);
            ev.clear();
        }
    }
}
# }
```

The deferred drain — which runs every pump pass after dispatch — picks up the
`EndModal` effect and writes `end_state = Some(cmd)`. On the very next
iteration of `exec_view`'s `while end_state.is_none()` loop, the condition
becomes false and the loop exits.

From **top-level code** (outside a view, holding `&mut Program`) you can call
[`Program::end_modal`](../api/tvision_rs/app/struct.Program.html#method.end_modal)
directly — it sets `end_state` without the deferred queue, which is useful in
tests where you want to terminate a headless modal after pre-queuing events.

Rule of thumb: view code → `ctx.end_modal`; program-level code → `Program::end_modal`.

**Sources:** `Context::end_modal` (queues `Deferred::EndModal`) in
`src/view/context.rs`; `Deferred::EndModal` application in `src/app/program.rs`.

> **Turbo Vision heritage:** ports `TView::endModal` (`tview.cpp`). The C++
> version wrote `endState` directly from inside the nested loop's stack frame;
> in tvision-rs the write is deferred via `Deferred::EndModal` because the view
> cannot reach the loop-owned `end_state` during dispatch.
