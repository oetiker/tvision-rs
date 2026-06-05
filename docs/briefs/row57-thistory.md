# Row 57 — `THistory` (FOUNDATION): the view-triggered async-modal seam

**Tag:** FOUNDATION. **Model:** Opus. **C++:** `source/tvision/thistory.cpp`
(+ `tvtext1.cpp:86` for the icon). **Builds on:** rows 54 (store), 55
(`HistoryViewer`), 56 (`HistoryWindow`) — all DONE in `src/widgets/history.rs`.

`THistory` is the dropdown-arrow icon placed next to a `TInputLine` (row 39). On
its trigger (click, or Ctrl/↓ while the linked input is focused) it opens a
modal `THistoryWindow` over the channel's history, and on **OK** writes the
picked string back into the linked input line. This is the **first** consumer of
the **view-triggered async-modal path** (`Deferred::OpenModal`-class) the menu
sessions deliberately reserved — the FOUNDATION value of this row is that seam,
not the (small) `THistory` view.

---

## 0. Scope — what is IN, and what is explicitly OUT

**IN (this brief):**
1. The **async-modal seam**: a `Program::pending_modal` field, an outer-loop
   drive that runs `exec_view` at top level after each `pump_once` (in **both**
   the `run` loop and the `exec_view` inner loop), `exec_view` **end_state
   save/restore** for re-entrancy, and a **completion run before the modal is
   dropped** (`ModalCompletion::HistoryPick`).
2. The **OpenHistory / RecordHistory brokers** (cross-sibling reads of the link's
   text/bounds/focus, performed where `group` is reachable — the deferred-apply
   phase, exactly like `OpenMenuBox` builds a `MenuBox` there).
3. A **`View::descendant_global_bounds`** trait method (+ delegate forwarder) for
   the link-local → absolute coordinate conversion the root-insert path needs.
4. The **`THistory` view** (`src/widgets/history.rs`): draw the icon, trigger
   arms, the `recordHistory` broadcast arm.
5. **`HistoryWindow::as_any_mut`** returning `Some(self)` (the completion
   downcasts the modal `dyn View` to read `get_selection`).
6. Flowback via the existing **`InputLine::set_value(FieldValue::Text)`** (it
   already does `data = s; select_all(true,true)` — exactly C++ `strnzcpy` +
   `selectAll(True)`).
7. Wire `THistory` into `lib.rs` re-exports; tests.

**OUT (deferred, leave a breadcrumb — do NOT build):**
- **The `ModalFrame` outside-delivery seam** (un-deferring the row-56
  outside-click `endModal(cmCancel)`). Confirmed this session: `ModalFrame`
  cannot deliver-to-the-modal (it has no `group`), and `program_handle_event`
  routes outside positional events **positionally to the desktop** (not by
  `current`). So this is a **delivery-path change in `program_handle_event`**,
  not a `ModalFrame` return-value tweak — its own scoped change + review.
  Double-click confirm/cancel routes positionally and works without it; Esc /
  Enter need the popup's internal `current` established (see the
  **initial-modal-currency** correction below). Keep the existing
  `TODO(row 57 modal-loop seam)` breadcrumb in `HistoryWindow::handle_event`.

  **CORRECTION (SPEC review found this — the original §0 claim "Esc/Enter already
  work" was FALSE):** `Group::insert` has no `ctx` and never calls `reset_current`,
  so unlike C++ (`insertView→show→resetCurrent`) an opened modal's internal
  `current` stays `None` until a nav event — keyboard Esc/Enter were dead on open.
  Row 57 fixes this **locally** for the history popup by establishing the viewer
  as the window's `current` in the existing first-event setup guard (faithful,
  same first-event deviation class as the viewer `setup()`). The **general**
  gap (every dialog opened via `exec_view` lacks initial currency) is breadcrumbed
  at `exec_view`'s `set_current` site as a foundational follow-on.
- **msgbox 63** — the co-consumer. It will **ADD a `ModalCompletion` variant**
  later; do not build it now.
- The C++ `link->focus()` **success-abort** (C++ aborts if focus fails). Our
  focus is deferred (`focus_descendant`) with no inline success bool — request
  focus + proceed, and **document the deviation** (same class as the row-39/41
  deferred-focus TODOs).

---

## 1. C++ source — `THistory` (faithful target)

```cpp
THistory::THistory( const TRect& bounds, TInputLine *aLink, ushort aHistoryId )
    : TView(bounds), link( aLink ), historyId( aHistoryId )
{
    options |= ofPostProcess;
    eventMask |= evBroadcast;
}

void THistory::draw() {            // icon = "\xDE~\x19~\xDD"  (▐ + hi ↓ + ▌)
    TDrawBuffer b;
    b.moveCStr( 0, icon, getColor(0x0102) );   // lo=palette[1], hi=palette[2]
    writeLine( 0, 0, size.x, size.y, b );
}

void THistory::handleEvent( TEvent& event ) {
    TView::handleEvent( event );
    if( event.what == evMouseDown ||
        ( event.what == evKeyDown &&
          ctrlToArrow( event.keyDown.keyCode ) == kbDown &&
          (link->state & sfFocused) != 0 ) )
    {
        if (!link->focus()) { clearEvent(event); return; }   // focus-abort (OUT — see §0)
        recordHistory(link->data);                           // record CURRENT text, at OPEN
        r = link->getBounds();                               // owner(dialog)-local
        r.a.x--; r.b.x++; r.b.y += 7; r.a.y--;               // grow: 1 L, 1 R, 1 up, 7 down
        p = owner->getExtent();                              // dialog extent, dialog-local
        r.intersect( p );
        r.b.y--;                                             // shrink bottom by 1
        historyWindow = initHistoryWindow( r );              // new THistoryWindow(r, historyId)
        if( historyWindow != 0 ) {
            c = owner->execView( historyWindow );            // MODAL
            if( c == cmOK ) {
                char rslt[256];
                historyWindow->getSelection( rslt );
                strnzcpy( link->data, rslt, link->maxLen+1 );
                link->selectAll( True );
                link->drawView();
            }
            destroy( historyWindow );
        }
        clearEvent( event );
    }
    else if( event.what == evBroadcast ) {
        if( (event.message.command == cmReleasedFocus &&
             event.message.infoPtr == link) ||
             event.message.command == cmRecordHistory )
            recordHistory(link->data);
    }
}

THistoryWindow *THistory::initHistoryWindow( const TRect& bounds ) {
    THistoryWindow *p = new THistoryWindow( bounds, historyId );
    p->helpCtx = link->helpCtx;       // OMIT — no help-ctx plumbing on views yet (note it)
    return p;
}
void THistory::recordHistory(const char* s) { historyAdd(historyId, s); }
```

**Three faithfulness pins (review will check against the C++, not this brief):**
- `recordHistory` records the link's **current** text at OPEN time; the **picked**
  value is **never re-recorded**.
- `getSelection` is read **after** `execView` returns but **before** `destroy` —
  i.e. while the modal still exists. (Our completion runs before the modal is
  dropped — §3.)
- The keyboard arm gates on `link` **already focused**; the mouse arm does not.
  Both then call `link->focus()`.

---

## 2. The seam — `Deferred` variants + `Context` methods

A `THistory` leaf holds only the link's `ViewId` (D3); it can read neither the
link's text, bounds, nor focus inline, and it cannot call `exec_view` (top-level
only — `program.rs:466`). So it **requests** the open; the pump (which owns
`group` + the loop) does everything at apply time. **A new deferred capability
ADDS A VARIANT** — do not add `Context::new` params or fields.

Add to `src/view/context.rs` `enum Deferred` (each with a doc comment in the
established style, noting the family it touches — **view-tree + loop-state**):

```rust
/// View-triggered modal open (THistory; msgbox 63 will add sibling completions).
/// Built at apply time because the trigger view holds only the link's id (D3):
/// the pump reads the link, records history, builds the THistoryWindow, and
/// stashes it into `Program::pending_modal` — it does NOT call exec_view here
/// (the apply phase is inside the pump destructure; see program.rs). The OUTER
/// driver loop runs exec_view at top level after pump_once returns.
OpenHistory {
    /// The linked TInputLine whose text/bounds/focus drive the open + flowback.
    link: ViewId,
    /// The history channel id.
    history_id: u8,
    /// True for the keyboard trigger (gate on the link being focused, faithful
    /// to `(link->state & sfFocused)`); false for the mouse trigger.
    require_focus: bool,
},
/// recordHistory(link->data) for the broadcast arm (cmReleasedFocus on the link
/// / cmRecordHistory): resolve the link, read its text, history_add(id, text).
RecordHistory { link: ViewId, history_id: u8 },
```

`Context` methods (mirror the existing `request_*`):
```rust
pub fn request_open_history(&mut self, link: ViewId, history_id: u8, require_focus: bool) {
    self.deferred.push(Deferred::OpenHistory { link, history_id, require_focus });
}
pub fn request_record_history(&mut self, link: ViewId, history_id: u8) {
    self.deferred.push(Deferred::RecordHistory { link, history_id });
}
```

---

## 3. The async-modal mechanism (`Program`, `src/app/program.rs`)

### 3a. New field + completion type

```rust
/// A view-requested modal awaiting top-level execution. Set by the OpenHistory
/// apply arm (a view cannot call exec_view — top-level only); drained by the
/// outer driver loop after pump_once returns, where a whole `&mut self` is held.
pending_modal: Option<(Box<dyn View>, ModalCompletion)>,
```
(Initialize `None` in `Program::new`; bind it in the `pump_once` destructure.)

```rust
/// What to do with a view-triggered modal's result, run AFTER the modal loop
/// ends but BEFORE the modal view is removed/dropped (so it can read the modal's
/// final state, e.g. get_selection). An enum, not a boxed FnOnce: a view-made
/// closure cannot hold `&mut Program`, and the codebase's pattern is "ADD A
/// VARIANT". msgbox 63 adds its own variant here.
enum ModalCompletion {
    /// THistory: on cmOK, read the HistoryWindow's selection and set_value it
    /// into the linked input line (data + select_all). On cancel, nothing.
    HistoryPick { link: ViewId },
}
```

### 3b. `exec_view` refactor — completion + end_state save/restore

Rename the body to `exec_view_with_completion(&mut self, view, completion:
Option<ModalCompletion>) -> Command`; keep `pub fn exec_view(view)` as a
one-liner delegating with `None` (preserves every existing caller/test).

Two changes inside the unified body:

1. **end_state save/restore (REQUIRED for re-entrancy).** A `THistory` lives in
   a `Dialog` that is itself usually opened via `exec_view` — so this is a
   **modal-from-modal**. Without save/restore, when the inner `exec_view`
   returns, `self.end_state` still holds the inner end command and the **outer**
   `while self.end_state.is_none()` would spuriously exit. Fix: at entry (after
   the existing `save_current`/`save_commands`):
   ```rust
   let saved_end_state = self.end_state.take();
   ```
   and immediately before `retval` is returned (after step 9):
   ```rust
   self.end_state = saved_end_state;
   ```
   **Verify the cmQuit deviation still holds:** the modal still **returns** QUIT
   as `retval` when a cmQuit ends it (unchanged); only the leftover
   `self.end_state` is restored. **Check the existing exec_view tests** — if any
   assert `program.end_state()` *after* a top-level `exec_view`, reconcile it
   against the C++ (top-level callers should not see a leftover modal end_state).

2. **Run the completion before remove/drop.** After the `retval` loop breaks and
   **before step 8** (`captures.pop()` / `group.remove(id)`), while the modal is
   still in the tree by `id`:
   ```rust
   if let Some(c) = completion {
       apply_modal_completion(c, retval, &mut self.group, id);
   }
   ```
   This must NOT go through the deferred queue (that drain is gated on
   `!ev.is_nothing()` inside `pump_once` and would never fire from here in a
   headless test). It is a direct `group` mutation — no `Context` needed:
   ```rust
   fn apply_modal_completion(c: ModalCompletion, result: Command, group: &mut Group, modal_id: ViewId) {
       match c {
           ModalCompletion::HistoryPick { link } => {
               if result == Command::OK {
                   let s = group.find_mut(modal_id)
                       .and_then(|v| v.as_any_mut())
                       .and_then(|a| a.downcast_mut::<HistoryWindow>())
                       .map(|hw| hw.get_selection());
                   if let Some(s) = s {
                       if let Some(lv) = group.find_mut(link) {
                           lv.set_value(crate::data::FieldValue::Text(s));
                       }
                   }
               }
           }
       }
   }
   ```
   (Two sequential `find_mut` borrows — never simultaneous.)

### 3c. The outer drive — in BOTH loops

Factor a helper and use it in place of the bare `self.pump_once()` in **both**
`run`'s inner `while self.end_state.is_none()` AND `exec_view`'s inner
`while self.end_state.is_none()`:
```rust
fn pump_and_drive(&mut self) {
    self.pump_once();
    if let Some((view, completion)) = self.pending_modal.take() {
        // Re-entrant exec_view at top level (faithful; end_state save/restore in
        // 3b keeps it transparent to the enclosing loop). Holds a whole &mut self.
        self.exec_view_with_completion(view, Some(completion));
    }
}
```
Keep `pub fn pump_once` unchanged (existing tests call it directly; they simply
won't drive a pending modal — fine, they don't create one).

### 3d. The apply arms (in the `pump_once` deferred drain match)

Bind `pending_modal` in the destructure. Add the two arms. They have `group`,
the per-drain `ctx`, and `pending_modal` available (none aliased — `ctx` borrows
`out_events`/`timers`/`deferred`, not `group`/`pending_modal`):

```rust
Deferred::RecordHistory { link, history_id } => {
    record_history_for(group, link, history_id);
}
Deferred::OpenHistory { link, history_id, require_focus } => {
    let focused = group.find_mut(link)
        .map(|v| v.state().state.focused).unwrap_or(false);
    // Keyboard trigger gate (faithful to `(link->state & sfFocused)`).
    if require_focus && !focused {
        // not focused — drop the request (no open).
    } else if let Some(bounds) = build_history_bounds(group, link) {
        // link->focus() — deferred-focus, fire-and-forget (success-abort is OUT, §0).
        group.focus_descendant(link, &mut ctx);
        // recordHistory(link->data) — CURRENT text, at OPEN (not the pick).
        record_history_for(group, link, history_id);
        // initHistoryWindow + stash for the outer drive.
        let hw = HistoryWindow::new(bounds, history_id);
        *pending_modal = Some((Box::new(hw), ModalCompletion::HistoryPick { link }));
    }
}
```

Free helpers (module-level in `program.rs`, near `field_int`):
```rust
fn field_text(v: crate::data::FieldValue) -> Option<String> {
    match v { crate::data::FieldValue::Text(s) => Some(s), _ => None }
}
fn record_history_for(group: &mut Group, link: ViewId, history_id: u8) {
    if let Some(t) = group.find_mut(link).and_then(|v| v.value()).and_then(field_text) {
        crate::widgets::history::history_add(history_id, &t);
    }
}
```

### 3e. Geometry — `build_history_bounds` + `descendant_global_bounds`

The C++ formula runs in the link's **owner (dialog)** frame; our `exec_view`
inserts into the **root** group and `ModalFrame` hit-tests in **root/absolute**
coords (the documented ROOT-INSERT + (0,0) caveat). So convert link-local →
absolute. Add a `View` trait method (default `None`), overridden by `Group`,
mirroring `find_mut`'s recursion but accumulating origins — **add the matching
forwarder to `tvision-macros/src/specs.rs`** (and the spy-test count bumps in
`tests/delegate_view.rs`; CLAUDE.md convention):

```rust
// View trait (view.rs): default None.
fn descendant_global_bounds(&self, _id: ViewId, _acc: Point) -> Option<Rect> { None }

// Group override (group.rs): `acc` is THIS group's absolute origin.
// `Child` is `{ id, view }` (no bounds field — confirmed); a child's owner-local
// bounds come from `child.view.state().get_bounds()`.
fn descendant_global_bounds(&self, id: ViewId, acc: Point) -> Option<Rect> {
    for child in &self.children {
        let b = child.view.state().get_bounds();      // owner-local
        let child_origin = Point::new(acc.x + b.a.x, acc.y + b.a.y);
        if child.id == id {
            let w = b.b.x - b.a.x; let h = b.b.y - b.a.y;
            return Some(Rect::new(child_origin.x, child_origin.y,
                                  child_origin.x + w, child_origin.y + h));
        }
        if let Some(r) = child.view.descendant_global_bounds(id, child_origin) {
            return Some(r);
        }
    }
    None
}
```

```rust
fn build_history_bounds(group: &mut Group, link: ViewId) -> Option<Rect> {
    let mut r = group.descendant_global_bounds(link, Point::new(0, 0))?;
    // C++ grow: 1 left, 1 right, 1 up, 7 down.
    r.a.x -= 1; r.b.x += 1; r.a.y -= 1; r.b.y += 7;
    // Clamp to the SCREEN extent (deviation from C++'s owner-extent intersect —
    // we root-insert, so the screen is the outer frame; documented).
    let screen = Rect::new(0, 0, group.state().size.x, group.state().size.y);
    r.intersect(&screen);
    r.b.y -= 1; // shrink bottom by 1.
    Some(r)
}
```
**Document the two geometry deviations** (clamp-to-screen instead of dialog
extent; absolute via `descendant_global_bounds`) inline — same family as the
existing ModalFrame coordinate caveat. `descendant_global_bounds` is faithful
for any nesting depth; the clamp difference only matters when the dialog is
inset from the screen.

---

## 4. The `THistory` view (`src/widgets/history.rs`)

Add a plain `View` (not a D2 wrapper — it embeds no inner view). `ViewState`
defaults `selectable = false` (so a click delivers without grabbing focus —
faithful; `THistory` is never `current`).

```rust
pub struct THistory {
    state: ViewState,
    link: ViewId,
    history_id: u8,
}
impl THistory {
    /// THistory(bounds, link, historyId): options |= ofPostProcess.
    pub fn new(bounds: Rect, link: ViewId, history_id: u8) -> Self {
        let mut state = ViewState::new(bounds);
        state.options.post_process = true; // ofPostProcess — gets keyDowns via the postProcess phase
        // selectable stays false (default); eventMask|=evBroadcast is MOOT (Group
        // fans broadcasts to all children — handover row 49).
        THistory { state, link, history_id }
    }
}
```

`impl View for THistory` — override:
- **`state` / `state_mut`** (the usual accessors).
- **`draw`**: `put_cstr(0, 0, "▐↓▌", lo, hi)` then nothing else (single row).
  Use the icon `"\xDE~\x19~\xDD"` → render literal **`▐`** (U+2590), highlighted
  **`↓`** (U+2193), **`▌`** (U+258C). The `~...~` in the C++ cstr marks the hi
  region (the arrow). Provisional theme: add `Role::HistoryArrow` (+ maybe
  `HistoryNormal`) with `TODO(row 34 gray theming)`, OR reuse the provisional
  `Input*` roles if a clean fit — match how row 55/56 handled `cpHistory*`
  (they reused `List*` + a `TODO(row 34) cpHistory remap`). Keep it minimal; no
  new palette machinery.
- **`handle_event`**:
  ```rust
  fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
      match ev {
          Event::MouseDown(_) => {
              ctx.request_open_history(self.link, self.history_id, false);
              ev.clear();
          }
          // ctrl_to_arrow returns the event UNCHANGED when not ctrl, so `== Key::Down`
          // matches BOTH the literal ↓ arrow AND Ctrl+X — faithful to
          // `ctrlToArrow(keyCode) == kbDown`. Modifiers are cleared on a mapped
          // result, so compare `.key` only.
          Event::KeyDown(k)
              if crate::event::key::ctrl_to_arrow(*k).key == crate::event::Key::Down =>
          {
              ctx.request_open_history(self.link, self.history_id, true);
              ev.clear();
          }
          Event::Broadcast { command, source }
              if (*command == Command::RELEASED_FOCUS && *source == Some(self.link))
                  || *command == Command::RECORD_HISTORY =>
          {
              ctx.request_record_history(self.link, self.history_id);
              // C++ does not clearEvent in the broadcast arm — leave it live.
          }
          _ => {}
      }
  }
  ```
  Confirm the "is ↓" check: `ctrl_to_arrow` returns a `KeyEvent`; compare its
  `key` to the Down arrow `Key` variant (see `event/key.rs` — `kbDown`). Modifier
  fields are cleared by `ctrl_to_arrow`, so compare the `key` only.
- **`value`/`set_value`**: leave at trait default (no transferable value).

`Command::RECORD_HISTORY` already exists (`command.rs:154`). `RELEASED_FOCUS`
exists. Re-export `THistory` from `lib.rs` next to `HistoryWindow`.

`historyWindow->helpCtx = link->helpCtx` is **OMITTED** (no help-ctx-on-view
plumbing yet — `get_help_ctx` is a standing deferral); note it as a one-line
`TODO(help-ctx propagation)`.

---

## 5. `HistoryWindow::as_any_mut` (the completion downcast)

`HistoryWindow`'s `#[delegate(... skip(as_any_mut ...))]` currently leaves
`as_any_mut` at the trait default (`None`), so the completion cannot downcast the
modal to `HistoryWindow`. **Remove `as_any_mut` from the `skip(...)` list and add
a real impl** in the manual `impl View for HistoryWindow` block:
```rust
fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> { Some(self) }
```
Drop the `#[allow(dead_code)]` on `get_selection` once the completion calls it.

---

## 6. Verification (discriminating + bite-checked; run on the integrated tree)

Add to `src/app/program.rs` tests (the `pump_once`/`exec_view` harness lives
there) and `src/widgets/history.rs`:

1. **End-to-end pick → flowback (the headline).** Build a `Dialog` containing a
   `TInputLine` (the link) + a `THistory` pointed at it, with a populated history
   channel. `exec_view` the dialog with pre-queued events that: click/▼-trigger
   the THistory → (inner HistoryWindow opens) → Enter to pick the focused entry →
   then `cmCancel` to close the dialog. Assert the **input line's `data` == the
   picked entry** afterward. **Bite:** make `set_value` a no-op (or skip the
   completion) → data unchanged → test fails.
2. **Cancel writes nothing.** Same setup, Esc the HistoryWindow → assert the
   input line's data is unchanged.
3. **recordHistory at OPEN.** Type something into the link, trigger the open →
   assert the link's *current* text is now in the channel (`history_count`/`str`),
   and that the *picked* value is not double-recorded.
4. **Keyboard gate.** ▼ with the link **not** focused → no modal opens (assert
   `pending_modal` stayed `None` / no flowback). With it focused → opens. Mouse
   trigger opens regardless of focus.
5. **Re-entrancy / end_state.** Assert the inner modal's end command does NOT
   leak out to end the outer dialog modal (the save/restore property) — e.g. the
   dialog stays open after the HistoryWindow closes with cmOK, until its own
   cmCancel. **Bite:** remove the save/restore → the outer loop exits early.
6. **`descendant_global_bounds`** unit test: a nested group (root → dialog →
   link) returns the link's absolute bounds = dialog-origin + link-local
   (discriminating: place the dialog at a non-zero origin so identity-conversion
   would fail).
7. **THistory draw snapshot** (`HeadlessBackend`): the `▐↓▌` icon. (cargo-insta
   not installed → `INSTA_UPDATE=always`, hand-verify, commit the `.snap`.)
8. **`descendant_global_bounds` delegate forwarder**: bump the `delegate_view`
   spy count in `tests/delegate_view.rs` (the new trait method must forward).

Run `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
warnings` (force a fresh re-lint), `cargo fmt --all --check`.
`export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target`.

---

## 7. Process notes for the implementer

- **Write one file, verify, next** (subagent incremental-write rule). Suggested
  order: (a) `Deferred` variants + `Context` methods; (b) `descendant_global_bounds`
  trait method + Group override + specs forwarder; (c) `Program` field +
  `ModalCompletion` + exec_view refactor + pump_and_drive + apply arms + helpers;
  (d) `THistory` view + `HistoryWindow::as_any_mut` + lib.rs; (e) tests.
- **Faithful by default**; the only deviations are the four enumerated in §0 +
  the two geometry notes in §3e — each gets an inline comment in the
  Baseline → Deviation → Integration style.
- Do **not** touch `ModalFrame` / `program_handle_event` routing (that is the
  separate OUT seam). Do **not** build msgbox.
- After it builds + tests green, it goes to **two-stage review** (fresh SPEC vs
  the C++ + guide, then QUALITY) — do not self-certify.
