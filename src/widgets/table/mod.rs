//! A multi-column table: a column header with click-to-sort over a scrolling
//! row list, with single or multi selection that follows a stable row key
//! across sorting and reloads.
//!
//! # Structure
//!
//! [`Table`] is a [`Group`]-based composite with two children, each with its
//! own [`ViewState`](crate::view::ViewState):
//!
//! - a one-row header that draws the column labels and the sort marker
//!   (`▲`/`▼`); a click on a label sorts by that column, a second click flips
//!   the direction (see [Columns](#columns) for the drags and the popup);
//! - a row body that is a [`ListViewer`](crate::widgets::ListViewer) (one
//!   column, item text = the formatted row), so keys, mouse, focus colours,
//!   type-to-search and the vertical scroll bar behave exactly like a
//!   [`ListBox`](crate::widgets::ListBox).
//!
//! The body is the selectable child: focusing the table focuses the body, and
//! the `focused` state reaches it through the group. The header stretches
//! horizontally and the body in both directions when the table is resized. The
//! vertical scroll bar is an external sibling passed by id, the `ListBox`
//! convention.
//!
//! # Layout
//!
//! Column widths follow the view width on every draw. Columns that do not fit
//! shrink widest first, never below their
//! [`min_width`](ColumnSpec::min_width); spare width goes to the
//! [`flex`](ColumnSpec::flex) columns by weight (the cells the rounding leaves
//! over go one each to the leftmost flex columns), or stays empty without
//! one. When the columns still do not fit at their `min_width`s, they scroll
//! horizontally by display cells: the header scrolls with the rows, the
//! multi-select check column stays put, ←/→ scroll one cell (past the cell
//! cursor's stops, see [Editing](#editing)), and an optional
//! horizontal bar ([`Table::with_h_bar`]) shows and drives the offset
//! ([`Table::h_offset`] / [`Table::set_h_offset`]); while the columns fit,
//! the bar's range is 0. A resize changes the bars' page steps and the
//! horizontal range; the table re-publishes them at once (every framework
//! resize path calls [`on_bounds_changed`](View::on_bounds_changed)), or on
//! its next event after app code called `change_bounds` directly.
//!
//! # Columns
//!
//! The user arranges the columns from the header:
//!
//! - **Resize:** drag the separator cell right of a label (the cell after the
//!   last column too, or the last column's own last cell when it ends at the
//!   view's right edge). The width follows the pointer, clamped to the column's
//!   [`min_width`](ColumnSpec::min_width) and to
//!   [`MAX_COLUMN_WIDTH`](layout::MAX_COLUMN_WIDTH); the column turns
//!   [`fixed`](ColumnSpec::fixed) (a flex column stops stretching), so the
//!   layout keeps that width and scrolls instead of shrinking it.
//! - **Move:** press a label and drag off it; a marker shows the gap the
//!   column lands in, the release moves it there. A press and release without
//!   leaving the label sorts instead. Sort, width and hidden state belong to
//!   the column, so they move with it.
//! - **Show/hide:** a right-click on the header (or
//!   [`Command::TABLE_COLUMNS`]) opens a popup listing every column with a
//!   check mark; choosing one shows or hides it. The last visible column
//!   stays: the popup cannot grey it out, so choosing it does nothing. Each
//!   item posts [`Table::toggle_column_command`].
//!
//! [`ColumnSpec::resizable`] and [`ColumnSpec::movable`] turn the drags off
//! per column. [`Table::column_layout`] / [`Table::set_column_layout`] read
//! and restore the arrangement for persisting it.
//!
//! # Editing
//!
//! A column built with [`ColumnSpec::editable`] can be edited in place. While
//! at least one visible column is editable the focused row shows a **cell
//! cursor** ([`Role::TableCellCursor`](crate::theme::Role::TableCellCursor)):
//! ←/→ move it between the editable columns in display order, scrolling it
//! into view; at the first or last one the keys scroll the columns as usual.
//! A click on an editable cell puts the cursor there.
//!
//! Enter or F2 on the cell cursor, or a double-click on an editable cell,
//! opens an [`InputLine`](crate::widgets::InputLine) over the cell,
//! pre-filled with its text, all selected, with the column's
//! [`validator`](ColumnSpec::validator) if it has one. Enter stores the text
//! through [`TableModel::set_cell`]; a validator or a `set_cell` that refuses
//! it shows its message in an error box and the editor stays open. After a
//! store the cell cursor stays on the cell, so the next Enter edits it again;
//! the Enter that stores does nothing else (it does not press the dialog's
//! default button). Esc cancels; so does losing the focus or a click outside
//! the editor. Tab and Shift-Tab store and open the next/previous editable
//! cell of the row; past the last (first) one the edit ends and the Tab goes
//! on, moving the focus as usual. An unchanged text stores nothing. A
//! dialog's OK (any end command but Cancel, through [`View::valid`]) stores
//! an open edit first, and a refusal keeps the dialog open; Cancel cancels
//! it. Without a cell cursor (no visible editable column) or without rows,
//! the table does not use Enter, so it reaches the dialog's default button;
//! with a cursor and a row it is always the table's, like F2, even when the
//! cell has no room to show an editor. With modifiers (Ctrl, Alt, Shift)
//! Enter never edits. A double-click on a cell
//! that is not editable still chooses (single mode) or toggles (multi mode)
//! the row, and Space still marks it in multi mode.
//!
//! The status line sees every key first. An app that binds F2 app-wide (the
//! Turbo Vision `~F2~ Save` convention) turns F2 into its command before the
//! table gets it; Enter and a double-click still open the editor there. Bind
//! such keys only in a status def for the help context of the view that
//! owns them.
//!
//! After a store the sort is re-applied and the focus follows the row's key.
//! The editor stays over its cell through a re-sort, a horizontal scroll, a
//! column resize or move, a [`Table::set_model`] reload (with the typed text)
//! and a table resize (a resize scrolls the cell back into view). A cell
//! scrolled off screen hides the editor without ending the edit: the next
//! key scrolls it back and goes to the editor. When the cell is gone (its
//! row left the model in a [`Table::set_model`] reload, its column was
//! hidden) the edit ends without storing anything or showing a box — at once
//! for a reload, at the table's next event for a hidden column — and the next
//! key after that is used up, so an Enter meant for the editor cannot press
//! the dialog's default button. A press on the header
//! cancels the edit, and so does moving the focus to another row
//! ([`Table::set_focused_key`], [`View::set_value`]).
//!
//! # Data
//!
//! The rows come from a [`TableModel`] ([`VecModel`] for plain strings). The
//! table never reorders the model; it keeps a row permutation for the current
//! sort. Every row has a [`RowKey`]: focus and the multi-selection follow the
//! key through a re-sort and through [`Table::set_model`].
//!
//! # Wiring
//!
//! Build with [`Table::new`] and [`Table::with_model`] (plus
//! [`Table::with_h_bar`] for a horizontal bar), insert, done: the table
//! publishes its ranges and scroll steps to the bars on activation or on its
//! first event.
//! Reload later with [`Table::set_model`].
//!
//! ```rust,ignore
//! let vsb_id = dlg.insert_child(Box::new(ScrollBar::new(Rect::new(41, 2, 42, 12))));
//! let table = Table::new(
//!     Rect::new(2, 2, 41, 12),
//!     vec![ColumnSpec::new("Name", 16), ColumnSpec::new("Size", 8).right()],
//!     Some(vsb_id),
//! )
//! .with_model(Box::new(VecModel::new(rows).with_keys(keys)));
//! let table_id = dlg.insert_child(Box::new(table));
//! ```
//!
//! # Notifications
//!
//! User actions broadcast with the **table's** id as `source`:
//! [`Command::TABLE_SORT_CHANGED`] (header click, [`Command::TABLE_SORT_NEXT`],
//! [`Command::TABLE_SORT_FLIP`]), [`Command::TABLE_SELECTION_CHANGED`] (multi
//! mode: a click on a row's `[ ]` box, Space or a double-click toggled a row)
//! and
//! [`Command::LIST_ITEM_SELECTED`] (single mode: Space or a double-click chose
//! the focused row) and [`Command::TABLE_COLUMNS_CHANGED`] (a column resized,
//! moved, shown or hidden) and [`Command::TABLE_CELL_EDITED`] (a cell editor
//! stored a new text). Programmatic setters do not broadcast — the caller already
//! knows what it changed.
//!
//! # Turbo Vision heritage
//!
//! None — Turbo Vision has no table; this is a tvision-rs extension like
//! [`Splitter`](crate::widgets::Splitter) and
//! [`TabBar`](crate::widgets::TabBar). Its feature set and the pure helpers in
//! [`layout`] are adapted from the `Table` of tvdocker
//! (<https://github.com/supertorpe/tvdocker>, MIT, by supertorpe), rebuilt on
//! the list viewer.

mod body;
mod columns;
mod edit;
pub mod layout;
pub mod model;

use std::cell::{Ref, RefCell};
use std::collections::{BTreeSet, HashSet};
use std::rc::Rc;

use crate::command::Command;
use crate::data::FieldValue;
use crate::event::Event;
use crate::view::{Context, DrawCtx, Group, Point, Rect, StateFlag, View, ViewId};
use crate::widgets::list_viewer;

use body::{TableBody, TableHeader};
pub use columns::ColumnState;
pub use layout::{Align, ColumnSpec, Separators, SortDir, ValidatorFactory};
pub use model::{RowKey, SortKey, TableModel, VecModel};

/// How a [`Table`] selects rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionMode {
    /// The focused row is the selection (the [`value`](View::value) is its
    /// key); Space or a double-click broadcasts
    /// [`Command::LIST_ITEM_SELECTED`].
    #[default]
    Single,
    /// A set of marked rows. The rows get a leading, fixed check column drawn
    /// like [`CheckBoxes`](crate::widgets::CheckBoxes) (`[ ]` / `[X]`, blank
    /// header cell). A click on a row's box, Space, or a double-click toggles
    /// the row's mark and broadcasts [`Command::TABLE_SELECTION_CHANGED`]; a
    /// click elsewhere on a row only focuses it. Marked rows also draw in the
    /// list-selected colour.
    Multi,
}

/// The state the table and its two children share.
struct TableData {
    model: Box<dyn TableModel>,
    columns: Vec<ColumnSpec>,
    /// Display order: `order[display_row] = model_row`.
    order: Vec<usize>,
    sort: Option<(usize, SortDir)>,
    mode: SelectionMode,
    selected: BTreeSet<RowKey>,
    /// Mirror of the body's focused display row (the body updates it on every
    /// focus move), for the table's `&self` accessors.
    focused: usize,
    /// User notifications queued by the children, broadcast by the table with
    /// its own id after it routed the event.
    pending: Vec<Command>,
    /// Display order of the columns: `col_order[display_col] = model_col`
    /// (hidden columns included). Sort, width and hidden state stay keyed by
    /// the model column, so they follow a moved column.
    col_order: Vec<usize>,
    /// Header-local x of a column drag's drop indicator, drawn by the header.
    drop_x: Option<i32>,
    /// Horizontal scroll offset of the non-frozen columns, in display cells.
    /// Clamped to the overflow wherever a layout is at hand (draw, publish).
    h_offset: i32,
    /// The model column the cell cursor was last put on; see
    /// [`cursor_col`](TableData::cursor_col) for the one in effect.
    cursor: Option<usize>,
    /// Which column separators the table draws (see [`Separators`]).
    separators: Separators,
}

type Shared = Rc<RefCell<TableData>>;

impl TableData {
    /// Recompute the display order from the model and the sort.
    fn reorder(&mut self) {
        self.order = match self.sort {
            Some((col, dir)) => layout::sort_order(&*self.model, col, dir),
            None => (0..self.model.row_count()).collect(),
        };
    }

    /// The key of the row at display position `display`.
    fn key_at(&self, display: usize) -> Option<RowKey> {
        self.order.get(display).map(|&row| self.model.row_key(row))
    }

    /// The display position of the row with `key`.
    fn display_of(&self, key: &RowKey) -> Option<usize> {
        self.order
            .iter()
            .position(|&row| self.model.row_key(row) == *key)
    }

    /// Every key of the model.
    fn all_keys(&self) -> HashSet<RowKey> {
        (0..self.model.row_count())
            .map(|row| self.model.row_key(row))
            .collect()
    }

    /// Drop marks whose row no longer exists.
    fn prune_selection(&mut self) {
        let keys = self.all_keys();
        self.selected.retain(|k| keys.contains(k));
    }

    /// The columns the layout runs over: in multi mode the fixed check column
    /// leads, so the layout index of user column `c` is `c + check_cols()`.
    fn layout_columns(&self) -> Vec<ColumnSpec> {
        let mut cols = Vec::with_capacity(self.columns.len() + 1);
        if self.mode == SelectionMode::Multi {
            cols.push(layout::check_column());
        }
        cols.extend(self.display_columns());
        cols
    }

    /// The column specs in display order.
    fn display_columns(&self) -> Vec<ColumnSpec> {
        self.col_order
            .iter()
            .map(|&c| self.columns[c].clone())
            .collect()
    }

    /// The model column at layout index `layout_col` (`None` for the check
    /// column or past the end).
    fn model_col(&self, layout_col: usize) -> Option<usize> {
        layout_col
            .checked_sub(self.check_cols())
            .and_then(|i| self.col_order.get(i).copied())
    }

    /// How many layout columns precede the user columns (1 in multi mode).
    fn check_cols(&self) -> usize {
        usize::from(self.mode == SelectionMode::Multi)
    }

    /// The model column of the first visible column at or after display
    /// position `from` (wrapping), if any.
    fn visible_from(&self, from: usize) -> Option<usize> {
        let n = self.col_order.len();
        (0..n)
            .map(|step| self.col_order[(from + step) % n])
            .find(|&c| !self.columns[c].hidden)
    }

    /// The display position of model column `col`.
    fn display_pos(&self, col: usize) -> Option<usize> {
        self.col_order.iter().position(|&c| c == col)
    }
}

/// A multi-column table view. See the [module docs](self).
pub struct Table {
    group: Group,
    data: Shared,
    body_id: ViewId,
    /// Set until the range and scroll steps have been published to the bars —
    /// that needs a [`Context`], which construction and
    /// [`change_bounds`](View::change_bounds) do not have.
    needs_publish: bool,
    /// The bars have been published once: a later resize re-publishes the
    /// vertical page step with the resize formula, not the construction one.
    published: bool,
    /// The size changed since the last publish.
    resized: bool,
    /// A header press being tracked (resize or move), if any.
    drag: Option<columns::HeaderDrag>,
    /// Absolute screen position of the table's `(0, 0)` and the screen size,
    /// cached by `draw`: the drag tracking localizes against the first, the
    /// column popup is placed with both.
    abs_origin: Point,
    screen: Point,
    /// The open cell editor, if any.
    edit: Option<edit::CellEdit>,
    /// An edit ended because its cell vanished: the next key is used up
    /// (cleared by a click or a focus loss).
    eat_key: bool,
}

impl Table {
    /// A table at `bounds` (header on the first row, rows below) with
    /// `columns`, wired to the optional vertical scroll bar `v_scroll_bar`.
    /// It starts with an empty model; add rows with
    /// [`with_model`](Self::with_model) or, after insertion,
    /// [`set_model`](Self::set_model).
    pub fn new(bounds: Rect, columns: Vec<ColumnSpec>, v_scroll_bar: Option<ViewId>) -> Self {
        let mut group = Group::new(bounds);
        // A click on an unfocused table both focuses it and acts (selects the
        // row, sorts the column), like a list box.
        group.state_mut().options.first_click = true;
        let size = group.state().size;
        let col_order = (0..columns.len()).collect();
        let data: Shared = Rc::new(RefCell::new(TableData {
            model: Box::new(VecModel::default()),
            columns,
            order: Vec::new(),
            sort: None,
            mode: SelectionMode::Single,
            selected: BTreeSet::new(),
            focused: 0,
            pending: Vec::new(),
            col_order,
            drop_x: None,
            h_offset: 0,
            cursor: None,
            separators: Separators::default(),
        }));
        group.insert(Box::new(TableHeader::new(size.x, data.clone())));
        let body_bounds = Rect::new(0, 1, size.x, size.y.max(1));
        let body_id = group.insert(Box::new(TableBody::new(
            body_bounds,
            v_scroll_bar,
            data.clone(),
        )));
        Table {
            group,
            data,
            body_id,
            needs_publish: true,
            published: false,
            resized: false,
            drag: None,
            abs_origin: Point::new(0, 0),
            screen: Point::new(0, 0),
            edit: None,
            eat_key: false,
        }
    }

    /// Builder: wire the horizontal scroll bar `h_bar` (an external sibling,
    /// like the vertical one). Its range is the columns' overflow past the
    /// view: 0 while they fit. The table publishes range and steps itself.
    pub fn with_h_bar(mut self, h_bar: ViewId) -> Self {
        // The list viewer's bar wiring gives the bar its visibility and the
        // changed/clicked broadcasts; the body turns its value into the
        // table's offset.
        self.body_mut().lv_state().h_scroll_bar = Some(h_bar);
        self.needs_publish = true;
        self
    }

    /// How far the columns are scrolled to the left, in display cells (0 when
    /// they fit).
    pub fn h_offset(&self) -> i32 {
        let d = self.data.borrow();
        body::row_layout(&d, self.group.state().size.x).clamp(d.h_offset)
    }

    /// Scroll the columns to `offset` display cells (clamped to the
    /// overflow) and move the horizontal bar with them.
    pub fn set_h_offset(&mut self, offset: i32, ctx: &mut Context) {
        self.ensure_published(ctx);
        self.data.borrow_mut().h_offset = offset;
        self.publish_h(ctx);
    }

    /// Builder: the initial rows (no [`Context`] needed; the table publishes
    /// them to its scroll bar on its first event).
    pub fn with_model(mut self, model: Box<dyn TableModel>) -> Self {
        let idx = self.load_model(model);
        // The rows draw right away; the bar learns the range on publish.
        let len = self.data.borrow().order.len();
        self.body_mut().lv_state().range = len as i32;
        self.place_focus(idx.min(len.saturating_sub(1)));
        self.needs_publish = true;
        self
    }

    /// Builder: the selection mode (default [`SelectionMode::Single`]).
    pub fn with_selection_mode(self, mode: SelectionMode) -> Self {
        self.data.borrow_mut().mode = mode;
        self
    }

    /// The selection mode.
    pub fn selection_mode(&self) -> SelectionMode {
        self.data.borrow().mode
    }

    /// Builder: which column separators to draw (see [`Separators`]).
    /// The default, [`Separators::Off`], leaves the one-cell separators
    /// blank; [`Separators::Header`] draws the theme's vertical frame glyph
    /// (`│` by default) in the header row only, making the resize handle
    /// visible; [`Separators::All`] draws the header rule and the same rule
    /// between every row's cells as well. When the columns leave spare width,
    /// the header also rules the cell after the last column (its resize
    /// handle); the rows leave that cell blank.
    pub fn with_separators(self, separators: Separators) -> Self {
        self.data.borrow_mut().separators = separators;
        self
    }

    /// The column separators drawn between the table's columns.
    pub fn separators(&self) -> Separators {
        self.data.borrow().separators
    }

    /// The current model.
    pub fn model(&self) -> Ref<'_, dyn TableModel> {
        Ref::map(self.data.borrow(), |d| &*d.model)
    }

    /// Replace the rows. Focus stays on the same [`RowKey`] when it still
    /// exists (else the focused position is clamped into the new rows), the
    /// sort is re-applied, marks of vanished rows are dropped, and the scroll
    /// bar's range is updated. Does not broadcast.
    pub fn set_model(&mut self, model: Box<dyn TableModel>, ctx: &mut Context) {
        let idx = self.load_model(model);
        self.publish(idx, ctx);
        // An open edit survives the reload, typed text and all, while its row
        // is still there.
        if self.edit.is_some() && !self.edit_alive() {
            self.end_dead_edit(ctx);
        }
    }

    /// Swap in `model`, re-apply the sort and prune the marks; return the
    /// display row focus should land on: the focused key's new position, else
    /// the old position (the caller clamps it).
    fn load_model(&mut self, model: Box<dyn TableModel>) -> usize {
        let key = self.focused_key();
        let mut d = self.data.borrow_mut();
        let old = d.focused;
        d.model = model;
        d.reorder();
        d.prune_selection();
        key.and_then(|k| d.display_of(&k)).unwrap_or(old)
    }

    /// Publish the range and the scroll steps to the bar, then focus display
    /// row `idx` (clamped). The steps use the list viewer's construction
    /// formula the first time and its resize formula (page = height, arrow
    /// kept) after a resize, as `TListViewer::changeBounds` does.
    fn publish(&mut self, idx: usize, ctx: &mut Context) {
        let len = self.data.borrow().order.len() as i32;
        let resize = self.published && self.resized;
        let body = self.body_mut();
        if resize {
            list_viewer::on_bounds_changed(body, ctx);
        } else {
            list_viewer::update_steps(body, ctx);
        }
        list_viewer::set_range(body, len, ctx);
        // Keep the viewport inside the new rows so a shrunken model does not
        // leave the visible window scrolled past its end.
        let lv = body.lv_state();
        lv.top_item = lv.top_item.min((len - lv.state.size.y).max(0));
        list_viewer::focus_item_num(body, idx as i32, ctx);
        if len == 0 {
            self.data.borrow_mut().focused = 0;
        }
        self.publish_h(ctx);
        self.needs_publish = false;
        self.published = true;
        self.resized = false;
    }

    /// Clamp the horizontal offset to the current overflow and publish it,
    /// the range (`0..=overflow`) and the steps (page = the visible width of
    /// the scrolling columns, arrow = 1) to the horizontal bar.
    fn publish_h(&mut self, ctx: &mut Context) {
        let size_x = self.group.state().size.x;
        let (off, max, page) = {
            let mut d = self.data.borrow_mut();
            let l = body::row_layout(&d, size_x);
            d.h_offset = l.clamp(d.h_offset);
            (d.h_offset, l.max_offset(), l.view)
        };
        // The h bar id is stored once, in the body's list-viewer wiring.
        if let Some(h) = self.body_mut().lv_state().h_scroll_bar {
            ctx.request_scroll_bar_params(
                h,
                Some(off),
                Some(0),
                Some(max),
                Some(page.max(1)),
                Some(1),
            );
        }
    }

    /// Publish to the bar if construction left that pending.
    fn ensure_published(&mut self, ctx: &mut Context) {
        if self.needs_publish {
            let idx = self.data.borrow().focused;
            self.publish(idx, ctx);
        }
    }

    /// The row body, by downcast through the group.
    fn body_mut(&mut self) -> &mut TableBody {
        self.group
            .child_mut(self.body_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<TableBody>())
            .expect("the table body is a fixed child of the table")
    }

    /// The display order: model row indices, top to bottom.
    pub fn row_order(&self) -> Vec<usize> {
        self.data.borrow().order.clone()
    }

    /// The model index of the focused row, `None` when there are no rows.
    pub fn focused_row(&self) -> Option<usize> {
        let d = self.data.borrow();
        d.order.get(d.focused).copied()
    }

    /// The key of the focused row, `None` when there are no rows.
    pub fn focused_key(&self) -> Option<RowKey> {
        let d = self.data.borrow();
        d.key_at(d.focused)
    }

    /// Move focus to the row with `key` (scrolling it into view). Returns
    /// `false` if no row has that key.
    pub fn set_focused_key(&mut self, key: &RowKey, ctx: &mut Context) -> bool {
        self.ensure_published(ctx);
        let idx = self.data.borrow().display_of(key);
        if idx.is_some() {
            self.cancel_edit_unless(key, ctx);
        }
        match idx {
            Some(i) => {
                list_viewer::focus_item_num(self.body_mut(), i as i32, ctx);
                true
            }
            None => false,
        }
    }

    /// Focus display row `idx` without a [`Context`] (no scroll-bar update):
    /// the D10 context-free scatter path.
    fn place_focus(&mut self, idx: usize) {
        let lv = self.body_mut().lv_state();
        let idx = idx as i32;
        let rows = lv.state.size.y;
        lv.focused = idx;
        if idx < lv.top_item {
            lv.top_item = idx;
        } else if rows > 0 && idx >= lv.top_item + rows {
            lv.top_item = idx - rows + 1;
        }
        self.data.borrow_mut().focused = idx as usize;
    }

    /// The current sort: column and direction, `None` for model order.
    pub fn sort_spec(&self) -> Option<(usize, SortDir)> {
        self.data.borrow().sort
    }

    /// Sort by column `col` (an index into the columns) in `dir`, keeping
    /// focus on its row. An out-of-range column is ignored. Does not
    /// broadcast.
    pub fn sort_by(&mut self, col: usize, dir: SortDir, ctx: &mut Context) {
        if col >= self.data.borrow().columns.len() {
            return;
        }
        self.resort(Some((col, dir)), ctx);
    }

    /// Back to model order, keeping focus on its row. Does not broadcast.
    pub fn clear_sort(&mut self, ctx: &mut Context) {
        self.resort(None, ctx);
    }

    fn resort(&mut self, sort: Option<(usize, SortDir)>, ctx: &mut Context) {
        self.ensure_published(ctx);
        let key = self.focused_key();
        let idx = {
            let mut d = self.data.borrow_mut();
            d.sort = sort;
            d.reorder();
            key.and_then(|k| d.display_of(&k)).unwrap_or(0)
        };
        list_viewer::focus_item_num(self.body_mut(), idx as i32, ctx);
    }

    /// User sort: apply and queue the broadcast.
    fn user_sort(&mut self, sort: Option<(usize, SortDir)>, ctx: &mut Context) {
        if sort.is_some() && sort != self.sort_spec() {
            self.resort(sort, ctx);
            self.data
                .borrow_mut()
                .pending
                .push(Command::TABLE_SORT_CHANGED);
        }
    }

    /// The sort a click on column `col` asks for: the same column flips,
    /// another starts ascending.
    fn click_sort(&self, col: usize) -> Option<(usize, SortDir)> {
        match self.sort_spec() {
            Some((c, dir)) if c == col => Some((c, dir.flip())),
            _ => Some((col, SortDir::Asc)),
        }
    }

    /// [`Command::TABLE_SORT_NEXT`]: the next visible column (in display
    /// order), ascending.
    fn next_sort(&self) -> Option<(usize, SortDir)> {
        let d = self.data.borrow();
        let from = d
            .sort
            .and_then(|(c, _)| d.display_pos(c))
            .map_or(0, |p| p + 1);
        d.visible_from(from).map(|c| (c, SortDir::Asc))
    }

    /// [`Command::TABLE_SORT_FLIP`]: flip the current column; with no sort,
    /// the first visible column ascending.
    fn flip_sort(&self) -> Option<(usize, SortDir)> {
        match self.sort_spec() {
            Some((c, dir)) => Some((c, dir.flip())),
            None => self
                .data
                .borrow()
                .visible_from(0)
                .map(|c| (c, SortDir::Asc)),
        }
    }

    /// Hide or show column `col`. A hidden column takes no space; a hidden
    /// sort column keeps ordering the rows (its marker is just not shown).
    pub fn set_column_hidden(&mut self, col: usize, hidden: bool) {
        if let Some(c) = self.data.borrow_mut().columns.get_mut(col) {
            c.hidden = hidden;
        }
        // The horizontal range changes with the visible columns.
        self.needs_publish = true;
    }

    /// Whether column `col` is hidden (`false` for an unknown column).
    pub fn is_column_hidden(&self, col: usize) -> bool {
        self.data
            .borrow()
            .columns
            .get(col)
            .is_some_and(|c| c.hidden)
    }

    /// The keys of the marked rows, in model order (multi mode; empty in
    /// single mode unless set programmatically).
    pub fn selected_keys(&self) -> Vec<RowKey> {
        let d = self.data.borrow();
        (0..d.model.row_count())
            .map(|row| d.model.row_key(row))
            .filter(|k| d.selected.contains(k))
            .collect()
    }

    /// Replace the marks with `keys`; keys without a row are ignored. Does not
    /// broadcast.
    pub fn set_selected_keys(&mut self, keys: impl IntoIterator<Item = RowKey>) {
        let mut d = self.data.borrow_mut();
        let all = d.all_keys();
        d.selected = keys.into_iter().filter(|k| all.contains(k)).collect();
    }

    /// Clear every mark, leaving focus where it is. Does not broadcast.
    pub fn clear_selection(&mut self) {
        self.data.borrow_mut().selected.clear();
    }

    /// Broadcast the queued user notifications with this table's id as
    /// `source`.
    fn drain(&mut self, ctx: &mut Context) {
        let pending = std::mem::take(&mut self.data.borrow_mut().pending);
        let source = self.group.state().id();
        for command in pending {
            ctx.broadcast(command, source);
        }
    }
}

#[crate::delegate(to = group)]
impl View for Table {
    /// Publish the rows to the scroll bar if that is still pending, then let
    /// the group fan the flag out. The owner sends `Active` to every child
    /// when its window or dialog activates, so a table publishes even when no
    /// event ever reaches it (another control holds focus).
    fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
        self.ensure_published(ctx);
        self.group.set_state(flag, enable, ctx);
        if flag == StateFlag::Focused {
            if !enable {
                self.eat_key = false;
            }
            self.edit_focus(enable, ctx);
        }
    }

    /// Remember where the table sits on screen (for the drag tracking and
    /// the column popup), then draw the header and the rows.
    fn draw(&mut self, ctx: &mut DrawCtx) {
        self.abs_origin = ctx.origin();
        self.screen = ctx.screen_size();
        self.place_editor(false);
        self.group.draw(ctx);
    }

    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        self.ensure_published(ctx);
        match ev {
            Event::KeyDown(_) if self.eat_key => {
                self.eat_key = false;
                ev.clear();
                return;
            }
            Event::MouseDown(_) => self.eat_key = false,
            _ => {}
        }
        self.close_dropped_edit(ctx);
        let used = if self.edit.is_some() {
            self.edit_event(ev, ctx)
        } else {
            self.start_event(ev, ctx)
        };
        if used || self.header_event(ev, ctx) {
            ev.clear();
        }
        if let Event::Command(command) = *ev {
            let sort = if command == Command::TABLE_SORT_NEXT {
                Some(self.next_sort())
            } else if command == Command::TABLE_SORT_FLIP {
                Some(self.flip_sort())
            } else {
                None
            };
            if let Some(sort) = sort {
                self.user_sort(sort, ctx);
                ev.clear();
            }
        }
        if !ev.is_nothing() {
            self.group.handle_event(ev, ctx);
        }
        self.drain(ctx);
    }

    /// With a cell editor open, [`Command::CANCEL`] cancels the edit and any
    /// other command commits it first — a dialog's OK, but also a window's
    /// [`Command::CLOSE`] and the program's [`Command::QUIT`], as C++
    /// `TInputLine::valid` validates for every command but `cmCancel`. A
    /// refused text shows its box and vetoes the command, so closing never
    /// drops what the field shows. [`Command::VALID`] and a focus release
    /// leave the edit alone, and an edit whose cell is gone is discarded.
    /// Then the children are checked as usual.
    ///
    /// The [`Command::TABLE_CELL_EDITED`] broadcast of a commit made here is
    /// queued; when the command closes a modal dialog, it is delivered after
    /// the dialog is gone. Views outside the dialog receive it, but its
    /// `source` names a table that no longer exists and views inside the
    /// dialog never see it.
    fn valid(&mut self, cmd: Command, ctx: &mut Context) -> bool {
        self.edit_valid(cmd, ctx) && self.group.valid(cmd, ctx)
    }

    /// Resize the header and the body (grow modes), and mark the bars for a
    /// re-publish: the new size changes the vertical page step and the
    /// horizontal range. Every framework resize path then calls
    /// [`on_bounds_changed`](View::on_bounds_changed), which publishes at
    /// once. The mark is a fallback for app code that calls `change_bounds`
    /// directly: the publish then runs on the table's next event.
    fn change_bounds(&mut self, bounds: Rect) {
        let old = self.group.state().size;
        self.group.change_bounds(bounds);
        if self.group.state().size != old {
            self.needs_publish = true;
            self.resized = true;
            // Keep the edited cell on screen under the editor.
            self.place_editor(true);
        }
    }

    /// Re-publish the bars for the new size, and hand the hook on to the
    /// children the group re-bounded (the header and body publish nothing
    /// themselves; this keeps the group's cascade intact for any child).
    fn on_bounds_changed(&mut self, ctx: &mut Context) {
        self.group.on_bounds_changed(ctx);
        self.needs_publish = true;
        self.resized = true;
        self.ensure_published(ctx);
    }

    /// Single mode: the focused row's key as [`FieldValue::Text`] (`""` with
    /// no rows). Multi mode: the marked keys as a [`FieldValue::List`] of
    /// `Text`, in model order.
    fn value(&self) -> Option<FieldValue> {
        Some(match self.selection_mode() {
            SelectionMode::Single => {
                FieldValue::Text(self.focused_key().map(|k| k.0).unwrap_or_default())
            }
            SelectionMode::Multi => FieldValue::List(
                self.selected_keys()
                    .into_iter()
                    .map(|k| FieldValue::Text(k.0))
                    .collect(),
            ),
        })
    }

    /// Accepts the shapes [`value`](View::value) produces: single mode focuses
    /// the row with the `Text` key (unknown keys are ignored), multi mode
    /// replaces the marks with the `List` of `Text` keys. Other shapes are
    /// ignored. Without a [`Context`] the scroll bar catches up on its next
    /// sync; prefer [`set_value_ctx`](View::set_value_ctx).
    fn set_value(&mut self, v: FieldValue) {
        match (self.selection_mode(), v) {
            (SelectionMode::Single, FieldValue::Text(k)) => {
                let key = RowKey(k);
                let idx = self.data.borrow().display_of(&key);
                if let Some(i) = idx {
                    self.drop_edit_unless(&key);
                    self.place_focus(i);
                }
            }
            (SelectionMode::Multi, FieldValue::List(items)) => {
                self.set_selected_keys(items.into_iter().filter_map(|i| match i {
                    FieldValue::Text(k) => Some(RowKey(k)),
                    _ => None,
                }));
            }
            _ => {}
        }
    }

    /// Like [`set_value`](View::set_value), but a single-mode focus move also
    /// updates the scroll bar.
    fn set_value_ctx(&mut self, v: FieldValue, ctx: &mut Context) {
        match (self.selection_mode(), v) {
            (SelectionMode::Single, FieldValue::Text(k)) => {
                self.set_focused_key(&RowKey(k), ctx);
            }
            (_, v) => self.set_value(v),
        }
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
}

#[cfg(test)]
mod tests;
