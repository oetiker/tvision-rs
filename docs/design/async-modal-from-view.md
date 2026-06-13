# Async modal-from-a-view (the `messageBox`-from-`valid()` seam)

> **Status: LANDED, and generalized into the project's interactive
> modal-from-view foundation.** This note was written for the first consumer (the
> `messageBox`-from-`valid()` case — validator errors + the `FileEditor`
> modified-save prompt, rows 68/79). The same four-part machinery
> (`Deferred::Open*` → `Program::pending_modal` → `exec_view_with_completion` →
> `ModalCompletion`) was then reused verbatim for **richer interactive dialogs that
> return typed data**: editor find/replace (C1, `b388492`), `FileEditor::saveAs`
> (C5), the color/theme pickers (C8). **There is no separate "interactive modal"
> seam — this is it.** The reuse recipe + the catalog of flow-back styles are in
> [Generalization](#generalization--arbitrary-interactive-modals-the-reuse-recipe)
> at the bottom; read that first if you are *adding a new* modal-from-view consumer.

> The forward-looking detour decided in `HANDOVER.md`: let a **downward-borrowed
> `&mut View`** (which owns no up-pointer and cannot run a nested modal inline)
> **request a modal `messageBox` from the pump** and later observe the user's
> choice. Retires three inert consumers at once: the five validator `error()`
> boxes, `FileEditor::valid()`'s modified-save prompt, and `FileEditor`'s
> save error popups.

## The problem

C++ `valid()` is *blocking* and does I/O: `TInputLine::valid` calls
`validator->valid()` → `error()` → a synchronous `messageBox`; `TFileEditor::valid`
calls `editorDialog(edSaveModify, …)` → a synchronous Yes/No/Cancel `messageBox`
and **uses its answer** to decide the bool it returns. rstv's single event loop
(D9) forbids a downward-borrowed `&mut View` from running a nested modal inline
(`exec_view` is top-level only — the `View` holds only `&mut Context`).

The `OpenHistory` async-modal precedent (row 57) is the template, but it triggers
from `handle_event`. `valid()` triggers from **two structurally different sites**,
and they are **not symmetric** (this was the subtle trap):

| Site | Where | ctx in flight? | Deferred drain runs? |
|---|---|---|---|
| **focus-leave** `valid(cmReleasedFocus)` | `group.focus_child` (`src/view/group.rs:351`), inside `handle_event` | yes | yes — normal `pump_once` drain |
| **window-close** `valid(cmClose)` | `Window::handle_event` (`src/window/window.rs:681`), inside `handle_event` | yes | yes — normal `pump_once` drain |
| **modal-close** `valid(endState)` | `exec_view_with_completion` (`src/app/program.rs:886`), **between** pump iterations | a ctx is built here | **NO** — the deferred drain is event-gated (`!ev.is_nothing()` inside `pump_once`); a modal queued at 886 would never fire headlessly |

**Consequence:** the `handle_event` sites use the normal deferred queue (pump
drains → `pending_modal` → `pump_and_drive`). The modal-close site at 886 must
**drive the modal inline**, holding `&mut self`, mirroring the explicit
`pending_modal` drive at `program.rs:501-503`.

## The seam

### 1. Signature change: `View::valid` carries `&mut Context`

`fn valid(&mut self, cmd: Command, ctx: &mut Context) -> bool` (was
`&self, cmd`). Pre-blessed in `HANDOVER.md` ("thread `&mut Context` through
`error()` **and** its `InputLine::valid` caller"). Ripples:

- Every `impl View::valid` (Group, Dialog, Window, InputLine, Editor, FileEditor,
  test stubs).
- `rstv-macros/src/specs.rs` `valid` forwarder (line ~36) → new signature.
- `Group::valid` (`group.rs:847`) `children.iter().all(|c| c.view.valid(cmd))` →
  manual `iter_mut` loop threading `&mut ctx`, **keeping the short-circuit**
  (C++ `firstThat` stops at the first invalid child).
- Call sites: `group.focus_child:351`, `window.rs:681`, `dialog.rs:143`,
  `program.rs:886` (+ `valid_end`/`group.valid` at `program.rs:508-509`).

### 2. `Deferred::OpenMessageBox` + `Context::request_message_box`

```rust
OpenMessageBox {
    text: String,
    kind: MessageBoxKind,
    buttons: MessageBoxButtons,
    /// Route the chosen Command to this view (via View::set_modal_answer) after
    /// the box closes. None = informational (OK-only) — no routing.
    answer_to: Option<ViewId>,
    /// After routing the answer, re-post this focused command so the original
    /// action (e.g. cmClose) re-runs valid() with the cached answer. None for
    /// informational boxes and for the inline-886 path (which re-validates inline).
    then_command: Option<Command>,
}
```

`Context::request_message_box(text, kind, buttons, answer_to, then_command)` pushes
it (mirror `request_open_history` at `context.rs:765`).

### 3. New `View` trait method: `set_modal_answer`

`fn set_modal_answer(&mut self, _cmd: Command) {}` — default no-op; add a
`specs.rs` forwarder. Overridden by `FileEditor` to stash the answer in a new
`pending_save_answer: Option<Command>` field.

### 4. `ModalCompletion::RouteModalAnswer { answer_to, then_command }`

`apply_modal_completion` returns `Option<Event>` (the event to re-post); the caller
at `program.rs:896-898` pushes it into `self.out_events` (the re-inject queue —
`pump_once` pops it before polling the backend). `RouteModalAnswer`:
`group.find_mut(answer_to).set_modal_answer(result)`; returns
`then_command.map(Event::Command)`. `HistoryPick` returns `None`.

### 5. The pump's deferred-drain arm (handle_event paths)

In `pump_once`'s deferred loop, add `Deferred::OpenMessageBox { … }`: build the
centered msgbox dialog (`build_message_box` + the `makeRect` centering already in
`Program::message_box`), and stash `pending_modal = Some((Box::new(dialog),
RouteModalAnswer { answer_to, then_command }))`. `pump_and_drive` runs it; the
completion routes the answer + re-posts `then_command`.

### 6. The inline-drive at `program.rs:886` (modal-close path)

Replace the bare `let valid = …valid(es)` with a helper that re-validates inline:

```rust
fn validate_modal_close(&mut self, id: ViewId, es: Command) -> bool {
    loop {
        let valid = { /* build ctx; self.group.find_mut(id).valid(es, &mut ctx) */ };
        // partition OpenMessageBox out of self.deferred (the rest is empty here)
        let requests = /* drain Deferred::OpenMessageBox from self.deferred */;
        if requests.is_empty() { return valid; }
        let mut revalidate = false;
        for req in requests {
            let r = self.centered_msgbox_rect(&req.text);
            let (d, first) = build_message_box(r, &req.text, req.kind, req.buttons);
            let (answer, _) = self.exec_view_with_completion(Box::new(d), None, first, None);
            if let Some(target) = req.answer_to {
                /* build ctx; self.group.find_mut(target).set_modal_answer(answer) */
                revalidate = true;
            }
        }
        if !revalidate { return valid; } // informational only: keep valid (false)
    }
}
```

`exec_view_with_completion` is re-entrant-safe (saves/restores `end_state` at
`program.rs:951`, exactly as `pump_and_drive` relies on).

## Consumer wiring

### Validators (informational, OK-only — `answer_to: None`, `then_command: None`)

`Validator::error(&self, ctx: &mut Context)` (NOT a `View` method — **no
`specs.rs` forwarder**). Each body emits the **exact C++ string** with
`MessageBoxKind::Error`, `MessageBoxButtons::ok()`:

| Validator | Message |
|---|---|
| Filter (`TFilterValidator`) | `Invalid character in input` |
| StringLookup (`TStringLookupValidator`) | `Input is not in list of valid strings` |
| Range (`TRangeValidator`) | `Value not in the range {min} to {max}` |
| PXPicture (`TPXPictureValidator`) | `Error in picture format.\n {pic}` |
| Regex (rstv extension) | `Input does not match pattern: {pattern}` |

`InputLine::valid`'s `!validate(&self.data)` branch calls `self.validator…error(ctx)`
before returning false (faithful to `TInputLine::valid`). Fires on **both** the
focus-leave (`cmReleasedFocus`) and modal-close (`cmOK`) paths.

### `FileEditor::valid` (Yes/No/Cancel prompt — `answer_to: Some(self)`,
`then_command: Some(cmClose)`)

`tfiledtr.cpp` `TFileEditor::valid`:
```
cmValid  → isValid
else if modified:
    edSaveUntitled ("Save untitled file?") if untitled, else
    edSaveModify   ("{fileName} has been modified. Save?")
    Yes    → save()
    No     → modified = False; return True
    Cancel → return False
else true
```
rstv: consume `pending_save_answer` if set (Yes→`save(ctx)`, No→`clear_modified()`
+true, Cancel→false). Else if `modified()` & untitled-or-named → queue
`OpenMessageBox` (`Information`, `yes_no_cancel()`, `answer_to = self id`,
`then_command = cmClose`) and return false. Else true.

`self id` = `View::id(self)` (delegates to `self.editor.id()`); set once the editor
is inserted (the close path always runs against an inserted tree).

### `FileEditor` save errors (informational — `answer_to: None`)

`save_file(&mut self, ctx)` / `save(&mut self, ctx)` gain `ctx`; on write/create
failure emit `OpenMessageBox(Error, ok())`:
- `Error creating file {fileName}.` / `Error writing file {fileName}.`
  (C++ distinguishes create vs write by whether the open failed; a single
  `Error writing file {fileName}.` is acceptable if create-vs-write isn't
  separable via `std::fs::write` — document the merge.)

`handle_event`'s `cmSave` arm already has ctx → pass it to `save`.

**Out of scope (still breadcrumbed):** `edReadError` on **load** — `load_file` is
only called from the ctor, which has **no ctx** and no inserted view. Keep
`is_valid = false` with no box; document. `saveAs`/`edSaveAs` still need
`TFileDialog`.

## Verification (TDD — write these first)

Headless tests that the queue-and-hope version fails:

1. **FileEditor close prompt** (the bug-surfacing test): a modified `FileEditor`
   in an `EditWindow` on a desktop; queue `cmClose`; drive the pump.
   - pre-queue `cmYes` → file written, window closed (`save_file` ran).
   - pre-queue `cmNo` → window closed, file NOT written, `modified` cleared.
   - pre-queue `cmCancel` → window stays open, still modified.
2. **Validator error box on OK** (the inline-886 path): a modal `Dialog` with a
   rejecting validator field; press `cmOK`; assert an Error messageBox appears and
   the dialog stays open.
3. **Validator error on focus-leave** (the deferred path): tab out of a bad
   `ofValidate` field; assert the box appears.

Snapshot tests only for anything that draws differently; these are behavioral.

## Generalization — arbitrary interactive modals (the reuse recipe)

The `messageBox` case above hard-codes the *dialog* (a centered text box) and the
*flow-back* (route a `Command` answer). The C1+ rows showed the **machinery is
general**: a view can launch *any* dialog, let the user fill it in, and read typed
data back. Nothing new in the loop was needed — only new variants.

### The invariant four parts (all keyed on `ViewId`, D3)

| Part | What | Where (find/replace example) |
|---|---|---|
| **1. Request** | View's `handle_event` pushes a `Deferred::Open*Dialog { requester_id, … }` (it can't call `exec_view` — top-level only). | `ctx.open_find_dialog(id)` → `Deferred::OpenFindDialog { editor_id }` |
| **2. Build + stash** | Pump deferred-drain arm assembles the modal `Box<dyn View>` from existing widgets, picks `initial_focus`, picks a `ModalCompletion`, stashes the triple into `Program::pending_modal`. **Does not run the modal** (apply phase is inside the `pump_once` split borrow). | `program.rs` `OpenFindDialog` arm → `pending_modal = Some((dialog, FindPick{…}, focus))` |
| **3. Drive** | `pump_and_drive` (used by *both* `run` and `exec_view`'s inner loop, so modal-from-modal works) takes `pending_modal` holding a whole `&mut self` and runs `exec_view_with_completion`. `end_state` save/restore keeps the inner modal transparent. | `program.rs:758` |
| **4. Flow-back** | The `ModalCompletion` runs **after the loop ends but before the modal is dropped** — the only window where the finished dialog's children are still resolvable by id. | `FindPick` reads the dialog's `InputLine`/`CheckBoxes` children, writes them onto the editor, re-injects `cmSearchAgain` |

### Why `ModalCompletion` is an enum, not a `Box<dyn FnOnce>`

A view-built closure cannot hold `&mut Program` (the completion needs it to walk
the tree and re-inject events), and the house pattern is **ADD A VARIANT** (same
as `Deferred`). Each consumer adds one `Deferred::Open*` variant + one
`ModalCompletion` variant; the loop itself is untouched.

### Catalog of flow-back styles (pick one)

1. **`set_modal_answer(cmd)` + re-post** (`RouteModalAnswer`) — cache a
   Yes/No/Cancel decision on the requester (a `View` trait method, default no-op,
   forwarded in `specs.rs`), then re-post `then_command` so the original
   `valid()`/action re-runs with the cached answer. *Use when the modal is a
   decision the requester's existing logic already branches on* (the save-on-close
   prompt; the editor's replace-prompt via `pending_replace_answer`).
2. **Direct read + re-inject** (`FindPick`/`ReplacePick`/`SaveAsPick`) — read the
   modal's child widgets by `ViewId`, write the typed values onto the requester's
   state, re-inject a command to continue. *Use when the modal collects multi-field
   typed input for the requester* (find string + options; a filename).
3. **`Rc<Cell>` / `Rc<RefCell>` sink** (`ColorPick`/`ThemeEdit`) — write the result
   into a shared cell the top-level (non-view) caller reads after the modal returns.
   *Use when a `Program` method — not a view — launched the modal and wants the
   value returned out* (`color_dialog`).

### Adding a new interactive modal-from-view — checklist

1. `Deferred::Open<Foo>Dialog { requester_id, … }` + a `Context::open_<foo>_dialog`
   helper (mirror `request_open_history` / `request_message_box`).
2. Request it from the view's `handle_event`.
3. Pump apply-arm: build the dialog (reuse existing widgets), choose `initial_focus`,
   pick a `ModalCompletion::<Foo>Pick { …child ids… }`, set `pending_modal`.
4. Completion arm in `apply_modal_completion`: resolve the modal's children by id,
   flow back via one of the three styles above; return any `Event` to re-inject.
5. If style 1, add a `set_modal_answer` override on the requester (+ `specs.rs`
   forwarder check). Layout snapshot test for the new dialog; a behavioral test that
   drives the pump and asserts the flow-back.

**Live consumers** (read these as worked examples): `FindPick`/`ReplacePick`
(`program.rs` ~2509/2596 build, ~2969/3006 flow-back), `SaveAsPick`,
`RouteModalAnswer`, `ColorPick`, `ThemeEdit`, `HistoryPick`.
