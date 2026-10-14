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
`Command::TABLE_SELECTION_CHANGED` (source = table id). Enter / double-click
keep the ListViewer meaning (`select_item` → the C++ `cmListItemSelected`
broadcast path the existing list viewers use).

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
