# Row 55 — `THistoryViewer` (`thstview.cpp`)

**Tag:** MECHANICAL. **Base:** `TListViewer` (row 28 — the trait seam + free fns
are ready; `TListBox` row 48 is the template). **Module:** `src/widgets/history.rs`
(extend the row-54 file — same module) or a new `src/widgets/history_viewer.rs`;
prefer extending `history.rs` so the store + its viewer live together.

## What it is

A read-only `TListViewer` over the global history store (row 54), shown inside a
modal `THistoryWindow` (row 56) when the user drops down an input field's recall
list. Single column. Enter / double-click confirms (endModal cmOK); Esc / cmCancel
dismisses (endModal cmCancel).

## C++ to port (`thstview.cpp`)

```cpp
THistoryViewer(bounds, hSB, vSB, historyId)
  : TListViewer(bounds, 1, hSB, vSB), historyId(historyId)   // 1 column
{ setRange(historyCount(historyId));
  if (range > 1) focusItem(1);
  hScrollBar->setRange(0, historyWidth() - size.x + 3); }
getText(dest, item, maxChars) { str = historyStr(historyId, item); copy or EOS; }
handleEvent:
  if ((mouseDown && doubleClick) || (keyDown && kbEnter)) { endModal(cmOK);  clear; }
  else if ((keyDown && kbEsc) || (command && cmCancel))   { endModal(cmCancel); clear; }
  else TListViewer::handleEvent(event);
historyWidth() { max over i in 0..historyCount(id) of strwidth(historyStr(id,i)); }
getPalette() -> cpHistoryViewer "\x06\x06\x07\x06\x06"
```

## Rust shape (mirror `ListBox`)

```rust
pub struct HistoryViewer { lv: ListViewerState, history_id: u8 }
```

- **`history_id: u8`** — faithful to the row-54 store's `u8` API. (C++ holds
  `ushort historyId` but the store is `uchar`, truncating at the call boundary;
  using `u8` throughout the history widgets makes that explicit and avoids a
  silent aliasing bug. Document this one line.)

- **`pub fn new(bounds, h: Option<ViewId>, v: Option<ViewId>, history_id: u8) -> Self`**
  — `lv: ListViewerState::new(bounds, 1, h, v)` (num_cols hard-coded to 1),
  `history_id`. **No `Context` in the ctor** (same constraint as `ListBox::new`),
  so the `setRange`/`focusItem`/h-bar-range work moves to a setup method:

- **`pub fn setup(&mut self, ctx: &mut Context)`** — the ctor's Context-needing
  tail, run after insertion (row 56's window calls it; standalone tests call it):
  1. `list_viewer::set_range(self, history_count(self.history_id) as i32, ctx);`
  2. `if self.lv.range > 1 { list_viewer::focus_item(self, 1, ctx); }`
  3. publish the h-bar range — faithful target `setRange(0, historyWidth() -
     size.x + 3)`. The h-bar id is `self.lv.h_scroll_bar: Option<ViewId>`. If
     `Some(hbar)`, call (signature confirmed):
     ```rust
     let size_x = self.lv.state.size.x;
     let max = self.history_width() - size_x + 3;
     ctx.request_scroll_bar_params(hbar, None, Some(0), Some(max), None, None);
     ```
     (value=None preserves the bar's value; min=0, max as above; page/arrow steps
     unchanged.) Skip if the h-bar is `None`.

- **`impl ListViewer for HistoryViewer`** — `lv()`/`lv_mut()` return `&self.lv`/
  `&mut self.lv`; override **only `get_text`**:
  ```rust
  fn get_text(&self, item: i32) -> String {
      if item < 0 { return String::new(); }
      history_str(self.history_id, item as usize).unwrap_or_default()
  }
  ```
  `is_selected`/`select_item` inherit the base (like `ListBox`).

- **`fn history_width(&self) -> i32`** — `(0..history_count(id)).map(|i|
  crate::text::width(&history_str(id, i).unwrap_or_default()) as i32).max().unwrap_or(0)`.
  (`crate::text::width` is the D13 display-width helper = faithful `strwidth`.)

- **`impl View for HistoryViewer`** — `state`/`state_mut` from `self.lv.state`;
  `draw` delegates to `list_viewer::draw(self, ctx)` (the base list draw);
  **`handle_event`** ports the C++ switch:
  ```rust
  fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
      match *ev {
          Event::MouseDown(me) if me.flags.double_click => { ctx.end_modal(Command::OK); ev.clear(); }
          Event::KeyDown(k) if k.key == Key::Enter      => { ctx.end_modal(Command::OK); ev.clear(); }
          Event::KeyDown(k) if k.key == Key::Esc        => { ctx.end_modal(Command::CANCEL); ev.clear(); }
          Event::Command(c) if c == Command::CANCEL     => { ctx.end_modal(Command::CANCEL); ev.clear(); }
          _ => list_viewer::handle_event(self, ev, ctx),
      }
  }
  ```
  **NOTE — no `sfModal` gate** (unlike `Dialog::handle_event`): the C++
  `THistoryViewer` endModals unconditionally because it only ever lives inside a
  modal `THistoryWindow` (always `execView`'d). Faithful = unconditional.
  Mirror `Dialog`'s pattern for the `end_modal` + deferred test inspection
  (`src/dialog/dialog.rs:99-131` and its `Deferred::EndModal(Command::OK)` test).

  Match the `View` method set exactly as `ListBox` does (look at
  `src/widgets/list_box.rs` — implement the same set, delegating draw/event,
  leaving the rest at trait defaults or `self.lv.state`). If `ListBox` is a D2
  embed using `#[delegate]`, follow that; if it hand-writes `state`/`state_mut`
  + draw + handle_event, do the same.

## Palette — provisional, reuse the List* roles (documented)

C++ `getPalette` remaps to `cpHistoryViewer "\x06\x06\x07\x06\x06"` (a
dialog-context recolor). rstv dropped palettes for `Theme`/`Role`, and
`list_viewer::draw` is hard-keyed to `Role::ListNormalActive/…Inactive/Focused/
Selected/Divider` with **provisional colors (`TODO(row 34 gray theming)`)**.
**Reuse those roles** (delegate to `list_viewer::draw`) and breadcrumb the
distinct history recolor as `TODO(row 34): cpHistoryViewer remap`. This matches
the menu/status precedent (provisional theme colors, realigned in row 34). Do
**not** add new `Role` variants for this row.

## Omit / breadcrumb (no dead stubs)

- `getText`'s `maxChars` truncation — the base `draw` clips to column width, and
  `get_text -> String` returns the full string (exactly as `ListBox` does). No
  truncation param needed.
- streaming (`write`/`read`/`build`/`name`, D12) — dropped.
- `drawView()` calls — D8 whole-tree redraw.

## Verification

- **1 snapshot** (`insta`, via `INSTA_UPDATE=always` then hand-verify + commit —
  `cargo-insta` is not installed; see how `list_box.rs`/`status` snapshots are
  generated): build a `HistoryViewer` on the `HeadlessBackend`, push a few
  entries into the store first (`clear_history()`; `history_add(id, …)` ×3),
  insert it, call `setup`, `render`, assert the list draws with item 1 focused
  (range>1 path). Reset the store with `clear_history()` at the top.
- **Unit tests (bite-checked):**
  - `get_text` returns the store string for a valid item, empty for out-of-range
    / negative (mirror `list_box.rs:316`).
  - `history_width` = max display width over entries (add entries of differing
    widths; bite: a `min`/first-only impl fails).
  - `setup` with range>1 focuses item 1 (not 0); with range≤1 leaves focus 0.
  - `handle_event`: Enter and double-click each queue `Deferred::EndModal(
    Command::OK)`; Esc and `Event::Command(Command::CANCEL)` each queue
    `Deferred::EndModal(Command::CANCEL)`; a plain Down-arrow does NOT queue any
    `EndModal` (falls through to base nav — bite). Inspect `ctx`'s deferred queue
    exactly as `dialog.rs:314`/`337` do.
  - Each store-touching test starts with `clear_history()`.

## Definition of done

`CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target cargo test --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`
all clean. English-only. Export `HistoryViewer` from `src/widgets/mod.rs` +
`src/lib.rs` (mirror the `ListBox` re-export).
