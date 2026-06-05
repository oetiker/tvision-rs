# Row 56 — `THistoryWindow` (Phase 4, MECHANICAL + one small foundation touch)

Port `THistoryWindow` (`magiblot-tvision/source/tvision/thistwin.cpp`): the modal
window that hosts a `THistoryViewer` recall list. It is a `TWindow` subtype that
assembles two scroll bars + the viewer and exposes `getSelection`. You are an
implementer subagent with fresh context — everything you need is inline below.

## Environment / commands
- `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target` (artifacts land
  there, NOT `./target`).
- Cargo **workspace** — always use `--workspace`:
  - `cargo test --workspace`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo fmt --all --check`
- Work in `/home/oetiker/checkouts/rstv`. **Write one file, verify it compiles /
  tests pass, then move to the next** — do not generate everything in one shot.

## The C++ source (port faithfully)
```cpp
#define cpHistoryWindow "\x13\x13\x15\x18\x17\x13\x14"

THistoryWindow::THistoryWindow( const TRect& bounds, ushort historyId ) noexcept :
    TWindowInit( &THistoryWindow::initFrame ),
    THistInit( &THistoryWindow::initViewer ),
    TWindow( bounds, 0, wnNoNumber )
{
    flags = wfClose;
    if( createListViewer != 0 &&
        (viewer = createListViewer( getExtent(), this, historyId )) != 0 )
        insert( viewer );
}

TPalette& THistoryWindow::getPalette() const
{
    static TPalette palette( cpHistoryWindow, sizeof( cpHistoryWindow )-1 );
    return palette;
}

void THistoryWindow::getSelection( char *dest )
{
    viewer->getText( dest, viewer->focused, 255 );
}

void THistoryWindow::handleEvent( TEvent& event )
{
    TWindow::handleEvent( event );
    if( event.what == evMouseDown && !mouseInView( event.mouse.where ) )
        {
        endModal( cmCancel );
        clearEvent( event );
        }
}

TListViewer *THistoryWindow::initViewer( TRect r, TWindow * win, ushort historyId )
{
    r.grow( -1, -1 );
    return new THistoryViewer( r,
        win->standardScrollBar( sbHorizontal | sbHandleKeyboard ),
        win->standardScrollBar( sbVertical | sbHandleKeyboard ),
        historyId);
}
```
(The `createListViewer` indirection is a C++ streamability hook so a derived
viewer can be substituted; we have no streaming (D12) — inline `initViewer`
directly. The `THistInit`/`TWindowInit` constructor-init machinery is moot.)

## What already exists (build on these — do NOT reinvent)

### `HistoryViewer` (row 55, `src/widgets/history.rs`) — DONE
```rust
pub struct HistoryViewer { lv: ListViewerState, history_id: u8 }
impl HistoryViewer {
    // bounds, optional h-bar id, optional v-bar id, channel id
    pub fn new(bounds: Rect, h: Option<ViewId>, v: Option<ViewId>, history_id: u8) -> Self;
    // Context-needing ctor tail — call ONCE after insertion (needs a live Context
    // for the deferred scroll-bar param requests). Does setRange(historyCount) +
    // focusItem(1) when range>1 + h-bar setRange(0, historyWidth()-size.x+3).
    pub fn setup(&mut self, ctx: &mut Context);
}
// ListViewer::get_text(item) -> String  (the focused text, by store lookup)
// lv field is private; `focused` lives at self.lv.focused (pub field on ListViewerState)
```
You will need the viewer's **focused text** for `get_selection`. `get_text` is a
`ListViewer` trait method: `<HistoryViewer as ListViewer>::get_text(&self, item: i32)`.
The focused index is `self.lv.focused` — but `lv` is **private** to the module. Since
`HistoryViewer` lives in the SAME file (`src/widgets/history.rs`), add a small
pub(crate) accessor on `HistoryViewer`:
```rust
/// `THistoryWindow::getSelection` reads `viewer->getText(viewer->focused)`.
pub(crate) fn selection(&self) -> String {
    <Self as crate::widgets::ListViewer>::get_text(self, self.lv.focused)
}
```
(Put `THistoryWindow` in this same file so `self.lv.focused` is reachable if you
prefer — but the `selection()` accessor is cleaner; use it.)

### `Window` (`src/window/window.rs`)
- `Window::new(bounds: Rect, title: Option<String>, number: u16) -> Window`
  (`wnNoNumber == 0` → pass `0`; `title` NULL → `None`).
- `pub(crate) fn set_flags(&mut self, flags: WindowFlags)` — re-pushes to the frame.
- `WindowFlags { r#move, close, grow, zoom, .. }` (struct-of-bools, all `bool`,
  `Default`). For `flags = wfClose`: `WindowFlags { close: true, ..Default::default() }`
  — **NOT** `r#move` (a history window is not draggable; only `wfClose`).
- `pub fn standard_scroll_bar(&mut self, opts: ScrollBarOptions) -> ViewId`
  — inserts a `ScrollBar` on the right (vertical) or bottom (horizontal) edge,
  returns its `ViewId`. `ScrollBarOptions { vertical: bool, handle_keyboard: bool }`.
  - h-bar: `ScrollBarOptions { vertical: false, handle_keyboard: true }`
  - v-bar: `ScrollBarOptions { vertical: true,  handle_keyboard: true }`
- `insert_child` is **currently `#[cfg(test)]`** — see the seam task below.
- Window's inner `group` is **private**; to reach a child you need an accessor —
  see the seam task below (`child_mut`).
- `get_extent()` is reachable via the view state: `self.window.<...>`. Window's
  state is the group state; you can get the extent through the `View` trait
  (`View::state(&self.window).get_extent()` — or add a thin helper). The window's
  extent is `(0, 0, size.x, size.y)`.

### The D2 embed-and-delegate pattern (copy `Dialog`, `src/dialog/dialog.rs`)
`THistoryWindow` *is-a* `TWindow`: embed a `Window` field and forward the
un-overridden `View` methods with the `#[delegate(to = window)]` proc-macro.
`Dialog` is your exact template:
```rust
pub struct Dialog { window: Window }
impl Dialog {
    pub fn new(bounds: Rect, title: Option<String>) -> Self {
        let mut window = Window::new(bounds, title, 0);
        window.set_flags(WindowFlags { r#move: true, close: true, ..Default::default() });
        window.set_grow_mode(GrowMode::default());
        window.set_palette(WindowPalette::Gray);
        Dialog { window }
    }
}
#[crate::delegate(to = window, skip( apply_list_scroll, as_any_mut, calc_bounds,
        grabs_focus_on_click, select_window_num, set_value, value ))]
impl View for Dialog {
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        self.window.handle_event(ev, ctx);
        /* ...dialog's own arms... */
    }
}
```
Use the **same `skip(...)` list** as `Dialog` unless an arm below requires you to
override a method (you will override `handle_event`; you do NOT need `as_any_mut`
unless a test downcasts the window itself — it does not, so keep `as_any_mut`
skipped like Dialog).

---

## Seam task (do this FIRST — small foundation touch)

**Promote `Window::insert_child` from `#[cfg(test)]` to a real `pub(crate)`
method, and add a `pub(crate)` child accessor.** THistoryWindow is the first
production consumer of window child-insertion; this also unblocks msgbox 63 +
Batch E.

In `src/window/window.rs`:
1. Remove the `#[cfg(test)]` attribute from `insert_child` (line ~247). The body
   is already `self.group.insert(view)`. `ViewId` is already imported at the top
   level (line 7), so no import change.
2. Add a child accessor (the group already has `child_mut`):
```rust
/// Reach a direct child of the embedded group by id (used by `THistoryWindow`
/// to run its viewer's post-insert `setup` + read `getSelection`).
pub(crate) fn child_mut(&mut self, id: ViewId) -> Option<&mut dyn View> {
    self.group.child_mut(id)
}
```

In `src/dialog/dialog.rs` (symmetry + unblocks msgbox 63):
3. Remove the `#[cfg(test)]` from `Dialog::insert_child` (line ~62). Its signature
   uses `ViewId`, which is currently in the **`#[cfg(test)]` import group** (lines
   7–8). Move `ViewId` into the non-test `use crate::view::{...}` at line 5 so the
   now-always-compiled method resolves. Leave the other formerly-test-only imports
   (`DrawCtx`, `StateFlag`, `ViewState`) under `#[cfg(test)]` if they are only used
   in tests — adjust the cfg group so it still compiles (run `cargo build
   --workspace` to confirm no unused-import warning under `-D warnings`).

Verify after the seam: `cargo build --workspace` + `cargo clippy --workspace
--all-targets -- -D warnings` clean. (A `pub(crate)` method with only test callers
must not trip `dead_code` — `insert_child`/`child_mut` both have a production
caller now via THistoryWindow, so fine.)

---

## `THistoryWindow` — the new type (put it in `src/widgets/history.rs`)

```rust
pub struct HistoryWindow {
    window: Window,
    viewer_id: ViewId,
    setup_done: bool,
}
```

### Constructor — `HistoryWindow::new(bounds: Rect, history_id: u8) -> Self`
Faithful to the C++ ctor + `initViewer`:
1. `let mut window = Window::new(bounds, None, 0);` (NULL title, `wnNoNumber`).
2. `window.set_flags(WindowFlags { close: true, ..Default::default() });`
   (`flags = wfClose` — close box only; NOT move/grow/zoom).
3. **`initViewer` inlined:** compute `let mut r = <window extent>; r.grow(-1, -1);`
   (the extent is `(0,0,size.x,size.y)`; `Rect::grow(-1,-1)` shrinks by 1 on each
   side — confirm the `Rect` API: there is a `grow`/inset helper; if the method is
   named differently, use the equivalent, e.g. construct
   `Rect::new(r.a.x+1, r.a.y+1, r.b.x-1, r.b.y-1)`).
4. Build the two bars (ORDER MATTERS — C++ evaluates the h-bar arg first, then the
   v-bar arg; both are inserted into the window group):
   ```rust
   let h = window.standard_scroll_bar(ScrollBarOptions { vertical: false, handle_keyboard: true });
   let v = window.standard_scroll_bar(ScrollBarOptions { vertical: true,  handle_keyboard: true });
   ```
5. `let viewer = HistoryViewer::new(r, Some(h), Some(v), history_id);`
6. `let viewer_id = window.insert_child(Box::new(viewer));`
7. `HistoryWindow { window, viewer_id, setup_done: false }`

Do NOT set a palette/grow_mode (C++ `THistoryWindow` overrides neither; its
`getPalette` returns `cpHistoryWindow`, which we map to the provisional default
`Window`/`Frame` roles — add a `// TODO(row 34): cpHistoryWindow palette remap`
breadcrumb, no new `WindowPalette` variant).

### `get_selection(&mut self) -> String`
```rust
/// `THistoryWindow::getSelection` — the viewer's focused entry text.
pub(crate) fn get_selection(&mut self) -> String { /* reach the viewer + selection() */ }
```
Use **`&mut self`**: the only child accessors are `Window::child_mut` (which you
add in the seam task) and `View::as_any_mut` — there is **no immutable `Group::child`
/ `View::as_any`**, so don't try the `&self` path. C++ `getSelection` is non-const
anyway, and the modal result read happens after the loop (see row 57), so `&mut`
is faithful. Body: resolve `self.viewer_id` via `self.window.child_mut(...)` →
`as_any_mut().and_then(|a| a.downcast_mut::<HistoryViewer>())` → `.selection()`. If
the downcast fails (never, in practice) return `String::new()`. Note the choice in
a doc comment.

### `handle_event` — the override
```rust
fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
    // (A) One-time viewer setup BEFORE delegating — the event then reaches a
    //     ready viewer (range/focused initialized). This is the Context-free-ctor
    //     deviation row 55/ListBox established: setup() needs a live Context, so it
    //     lands post-insert, here, on the first event.
    if !self.setup_done {
        if let Some(v) = self.window.child_mut(self.viewer_id) {
            if let Some(hv) = v.as_any_mut().and_then(|a| a.downcast_mut::<HistoryViewer>()) {
                hv.setup(ctx);
            }
        }
        self.setup_done = true;
    }
    // (B) TWindow::handleEvent (faithful order: base first).
    self.window.handle_event(ev, ctx);
    // (C) DEFERRED: the C++ `evMouseDown && !mouseInView -> endModal(cmCancel)`
    //     outside-click cancel is NOT ported in row 56 — see the note below.
}
```

**WHY the setup guard is BEFORE delegation (the discriminating bite):**
`Window::handle_event` routes the event down to the focused viewer child. If setup
ran *after* delegation, the **first** event would hit a viewer with `range=0,
focused=0` and nav would misbehave. Add a test that moving the guard after the
delegation breaks first-event behavior (e.g. the first Down-arrow on a multi-entry
history lands on the wrong item / no focus). Keep the guard at the top.

**DEFERRED — the outside-click cancel arm (`mouseDown && !mouseInView →
endModal(cmCancel)`).** Do NOT port it in row 56. Reason (verified): our
`ModalFrame` capture handler (`src/app/program.rs`, `ModalFrame::handle`)
**Consumes (swallows) positional mouse events that fall outside the modal view's
bounds** before they ever reach the modal view's `handle_event`. So an outside
`MouseDown` is unreachable at `HistoryWindow::handle_event` under the current modal
loop. Delivering outside positional events to the modal view is a **modal-loop
foundation change** reserved for the row-57 / msgbox-63 async-modal design session.
Leave a precise breadcrumb comment in `handle_event`:
```rust
// TODO(row 57 modal-loop seam): the C++ `evMouseDown && !mouseInView ->
// endModal(cmCancel)` outside-click cancel is omitted here: ModalFrame
// (program.rs) swallows outside positional events before they reach this view.
// Porting it needs ModalFrame to DELIVER (not Consume) outside clicks to the
// modal view — designed alongside the Deferred::OpenModal async-modal path.
```
Consequence for row 56: Esc/Enter/double-click (handled by the **viewer**, row 55)
still confirm/cancel the modal. The window's only un-ported behavior is the
click-outside cancel.

### Delegate forwarder check
`HistoryWindow` overrides only `handle_event` (+ `state`/`state_mut` come from the
delegate to `window`). Use the **same `#[delegate(to = window, skip(...))]` list as
`Dialog`**. Run the `tests/delegate_view.rs` spy test (`cargo test --workspace`) —
it verifies forwarders; if it complains, mirror Dialog exactly. You do NOT add a
new `View` trait method, so no `tvision-macros/src/specs.rs` change is needed.

### Wiring
Re-export `HistoryWindow` from `src/lib.rs` next to the existing history exports
(grep for `HistoryViewer` / `history_add` in `lib.rs` and add `HistoryWindow`
alongside, plus any `widgets` mod re-export).

---

## Tests (Appendix-B step 4 — discriminating, bite-checked)
Add to the `#[cfg(test)]` module in `src/widgets/history.rs`. Use the existing
test helpers there (they seed the thread-local store via `history_add` and build a
`Context` — copy the setup from the row-55 `setup_*` tests). REQUIRED:

1. **Construction:** `HistoryWindow::new` inserts exactly the frame + 2 scroll bars
   + the viewer (assert the group child count / that `viewer_id` resolves to a
   `HistoryViewer` via `child_mut` + downcast).
2. **Viewer is the focused/current selectable child** after construction +
   first-event setup: drive it through a real `pump_once`/`exec_view` (see the
   row-34 `Dialog` modal tests + row-55 viewer tests for the harness) and confirm
   keyboard routes to the viewer (a Down-arrow moves `focused`). The viewer is the
   only `ofSelectable` child (scroll bars are not) — verify, don't assume.
3. **`get_selection`** returns the focused entry's text. Seed 3 entries, run setup
   (range>1 → focuses item 1), assert `get_selection() == get_text(1)`.
4. **Setup-guard ordering bite:** a variant/assertion proving setup runs before the
   first event reaches the viewer (e.g. first Down-arrow lands correctly *because*
   range was initialized). The bite: moving the guard after `window.handle_event`
   makes this fail.
5. **Negative h-bar `max` end-to-end (the HANDOVER watch-item — REQUIRED):** build a
   history with a **narrow** longest entry and a **wide** viewer so
   `historyWidth() - size.x + 3` is **negative**, then drive setup through a **live
   pump** so the `request_scroll_bar_params(hbar, .., Some(negative_max), ..)`
   actually **drains through `ScrollBar::set_params`**. Assert no panic and the bar
   ends in a sane clamped state. This is the FIRST time a negative scroll-bar max
   drains into a live bar — pin it.
6. **(optional) snapshot** of the assembled window (frame + viewer rows + bars) on
   the `HeadlessBackend` if it adds signal beyond the row-55 viewer snapshot.

For the modal-driven tests, mirror the row-34 `Dialog` / row-55 patterns: pre-queue
events so the modal reaches `end_modal` (HEADLESS HANG WARNING — a modal with no
path to `end_modal` spins forever). Enter/Esc reach the viewer and end the modal.

## Done = all green
- `cargo test --workspace` (report the before→after count)
- `cargo clippy --workspace --all-targets -- -D warnings` (run a **forced re-lint**
  — `touch src/lib.rs` first — a cached run can mask a fresh warning)
- `cargo fmt --all --check`
Report: files changed, the test count delta, the `get_selection`/setup/negative-max
results, and confirm the outside-cancel deferral breadcrumb is in place. Do NOT
commit — the orchestrator integrates and commits.
