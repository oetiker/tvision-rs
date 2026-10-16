# Table widget (tvision-rs extension)

Status: approved for implementation (2026-10-14).

## Origin

tvdocker (github.com/supertorpe/tvdocker, MIT, by supertorpe) built a `Table`
on tvision-rs 0.16 (`src/widgets/table.rs`). Its feature set and pure helpers
are good; its view structure is not: it keeps its own `ViewState` while
`#[delegate(to = scroller)]` forwards `set_state`, `calc_bounds`,
`size_limits`, `get_help_ctx`, … to the inner `Scroller`'s state, so `focused`
never reaches the table and grow-mode resize uses the wrong geometry. We adopt
the features and helpers and rebuild the view on `ListViewer`. Credit the origin
in the module docs.

C++ Turbo Vision has no table class; this is a tvision-rs-original extension
like `Splitter`, `TabBar`, `PageStack`.

## Structure

`Table` is a composite (a `Group`-based view) with two children, each with its
own single `ViewState`:

- `TableHeader` — one-row leaf that draws column labels with the sort marker
  (`▲`/`▼`). A click on a label sorts by that column (second click flips the
  direction).
- `TableBody` — implements the `ListViewer` trait (`src/widgets/list_viewer.rs`)
  with `num_cols = 1`; `get_text(i)` is the formatted row. Keys, mouse, focus
  colours, vertical scroll bar brokering and incremental find come from the
  shared `list_viewer::*` functions, exactly as `ListBox` uses them.

The body is the selectable child; the header is not selectable (clicks still
reach it). Grow modes make the header stretch horizontally and the body stretch
in both directions. The vertical scroll bar is an external sibling passed by id,
the `ListBox` convention.

## Data model

```rust
pub trait TableModel {
    fn row_count(&self) -> usize;
    fn cell(&self, row: usize, col: usize) -> Cow<'_, str>;
    /// Stable identity of a row; focus and selection follow the key across reloads.
    fn row_key(&self, row: usize) -> RowKey;          // RowKey = String newtype
    /// Sort key; default parses the cell as a number, else text.
    fn sort_key(&self, row: usize, col: usize) -> SortKey { … } // Num(f64) | Text(String)
    /// Optional theme role for the whole row (e.g. a dimmed or an error row).
    fn row_role(&self, row: usize) -> Option<Role> { None }
}
```

`VecModel` implements it for `Vec<Vec<String>>` plus keys (default key = row
index as text). The table owns a `Box<dyn TableModel>` and a row permutation for
the current sort; it never reorders the model.

## Columns

`ColumnSpec { label, width, min_width, align: Align::{Left, Right}, hidden }`.
Width layout: keep tvdocker's `layout_widths` (shrink the widest column one cell
at a time until it fits, never below `min_width`), `pad_cell` (truncate with
`…`), alignment. Hidden columns take zero width. `set_column_hidden(col, bool)`.

## Sorting

`sort_by(col, Dir)`, `sort_spec() -> Option<(usize, Dir)>`. Stable sort; numeric
keys before text, as tvdocker's `sort_rows`. Header click sorts. No default
keyboard binding for sorting (←/→ belong to ListViewer); apps bind keys to
`Command::TABLE_SORT_NEXT` / `TABLE_SORT_FLIP` if they want the tvdocker
behaviour. Broadcast `Command::TABLE_SORT_CHANGED` (source = table id).

## Selection

`SelectionMode::{Single, Multi}`. Single: the focused row is the value. Multi: a
set of `RowKey`, Space toggles (via `ListViewer::select_item`), marks drawn with
the list-selected role; the set survives `set_model` and drops vanished keys.
`selected_keys()`, `set_selected_keys(..)`, `clear_selection()`. Broadcast
`Command::TABLE_SELECTION_CHANGED` (source = table id). Double-click keeps the
ListViewer meaning (`select_item` → the C++ `cmListItemSelected` broadcast
path the existing list viewers use). The ListViewer does not handle Enter (nor
does C++ `TListViewer`); the table uses it only to edit (see Phase C), else it
reaches the dialog's default button.

## Value (D10)

`value()`: Single → `FieldValue::Text(key of focused row)`; Multi →
`FieldValue::List(Text(key) …)` in model order. `set_value` accepts the same
shapes.

## Reload

`set_model(Box<dyn TableModel>, ctx)` keeps focus on the same `RowKey` when it
still exists (else clamps), re-applies the sort, prunes the selection, updates
the scroll bar range.

## Theme

Header uses a dedicated role (add `Role::TableHeader` with sensible default
mappings in every built-in theme if no fitting role exists); rows use the list
roles; `row_role` overrides the normal (unfocused, unselected) colour only.

## Out of scope (v1)

Horizontal scrolling, column resizing by mouse, editable cells, tvdocker's
`FilterBar`. A follow-up may add a placeholder text and Ctrl+U/Ctrl+W to
`InputLine`, and a `LogView`, also seen in tvdocker.

## Verification

Unit tests for the pure helpers (port tvdocker's as behaviour specs), behaviour
tests (sort, reload keeps focus key, multi-select survives reload, header click,
value/set_value, broadcasts), `insta` snapshots on the `HeadlessBackend`
(focused/unfocused, sorted, multi-selected, narrow width with truncation,
hidden column, `row_role`). Add the table to the gallery example and a book page
next to the other extensions.

---

# v2 additions (2026-10-15, after the user tried the demo)

The user asked for all of the former "out of scope" items. Built in three
phases on `feat/table`, each two-stage reviewed. Building blocks that already
exist: `Context::start_mouse_track` (drag tracking), `menu::popup_menu`,
`validate::Validator` + `InputLine::set_validator`, `request_exec_view` +
`set_modal_data` (dialog data-back).

## Check column (done first)

Multi mode draws a fixed leading `[ ]`/`[X]` column (CheckBoxes glyphs); a
click on it toggles, Space and double-click toggle too. No column-0 `X`.

## Phase A — layout

- **Stretch.** `ColumnSpec::flex(weight: u16)` (default 0). When the visible
  columns are narrower than the table, the spare cells go to the flex columns
  in proportion to their weights (remainder to the leftmost); with no flex
  column the spare stays empty, as today.
- **Horizontal scroll.** When the columns still do not fit after every column
  is at `min_width`, the table scrolls horizontally by display cells. Optional
  h scroll bar passed by id (`Table::new(..., v_bar, h_bar)` or a
  `with_h_bar(id)` builder). The header scrolls with the body. ←/→ scroll
  when no cell cursor exists (see Phase C). The check column is frozen (does
  not scroll). `h_offset()`/`set_h_offset()`.
- Resize: widths are derived on every draw from the current size, so window
  resizes re-lay out. The table re-publishes its v/h bar ranges and page
  steps from `change_bounds` itself, so it does not depend on the framework's
  missing `on_bounds_changed` cascade.

## Phase B — header interaction

- **Resize by mouse.** Pressing on the separator cell right of a column label
  and dragging changes that column's width (clamped to `min_width`, ≥ 1).
  Uses `start_mouse_track`. A resize turns a flex column into a fixed width.
- **Reorder by drag.** Pressing on a label and dragging horizontally by more
  than its own cell moves the column; a drop indicator shows the target gap;
  release commits. A press-release without drag still sorts. Sort and hidden
  state follow the column (they are keyed by the column's model index, not
  its display position).
- **Show/hide.** Right-click on the header opens a `popup_menu` listing every
  column with a check mark; choosing one toggles it. The last visible column
  cannot be hidden. The same toggle is reachable as `Command::TABLE_COLUMNS`
  (opens the popup at the header) so apps can bind a key or menu item.
- **Persisting layout.** `Table::column_layout() -> Vec<ColumnState>` (model
  index, width, hidden, order) and `set_column_layout(..)`; every user change
  broadcasts `Command::TABLE_COLUMNS_CHANGED` (source = table id).
- Column specs gain `resizable` (default true) and `movable` (default true).

## Phase C — cell editing

- `ColumnSpec::editable(bool)` (default false), optional
  `ColumnSpec::validator(Box<dyn Fn() -> Box<dyn Validator>>)` factory.
- `TableModel::set_cell(&mut self, row, col, text: &str) -> Result<(), String>`
  defaulted to `Err("read-only")`; `VecModel` implements it.
- **Cell cursor.** Exists only when at least one visible column is editable:
  ←/→ move it between editable columns (auto-scrolling horizontally); it is
  drawn as a distinct colour on the focused row.
- **Start editing:** Enter or F2 on the cell cursor, or double-click on an
  editable cell (in Multi mode a double-click on a non-editable cell still
  toggles, and Space still marks). *User decision 2026-10-16
  (spreadsheet style):* while there is a cell cursor and a focused row,
  plain Enter opens the editor on the cursor cell, exactly like F2, and is
  used up like F2 even when the cell has no room to show the editor. Without
  a cell cursor (no visible editable column) or without rows the table does
  not use Enter: it reaches the dialog's default button (no item-selected
  broadcast). Ctrl/Alt/Shift+Enter never edit. Enter still works where the app binds F2
  in its status line.
- **Editor:** an `InputLine` (with the column's validator) placed over the
  cell inside the table group, focused, pre-filled with the cell text, all
  selected. Enter commits → `set_cell`; on `Err(msg)` show `message_box` and
  stay in the editor. The committing Enter is used up (it neither re-opens
  the editor nor reaches the dialog's default button) and the cell cursor
  stays on the cell, so a second Enter edits it again. Esc cancels. Losing
  focus cancels. Tab/Shift-Tab commit and move to the next/previous
  editable cell on the same row.
- After a commit the sort is re-applied; focus follows the row key.
  Broadcast `Command::TABLE_CELL_EDITED` (source = table id); the app reads
  `table.model()` or its own copy.
