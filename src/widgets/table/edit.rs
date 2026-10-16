//! Cell editing: the cell cursor's columns and an [`InputLine`] editor laid
//! over one cell.
//!
//! The editor is a third child of the table's group, inserted on top of the
//! rows and made current while it is open, so the keys reach it the normal
//! way. The table takes the keys that end an edit before the group routes
//! them: Enter commits, Esc cancels, Tab/Shift-Tab commit and open the
//! next/previous editable cell of the row.
//!
//! The editor belongs to a row key and a model column, not to a screen
//! position: every draw lays it over that cell again, so it follows a
//! re-sort, a horizontal scroll, a column resize or move, a reload and a table
//! resize (a resize also scrolls the cell into view). A cell scrolled off
//! screen hides the editor but keeps the edit: the next key scrolls the cell
//! back into view and goes to the editor. A cell that is gone (its row left
//! the model, its column was hidden) ends the edit at the table's next event;
//! a key that finds it gone is used up, so it cannot turn into a dialog
//! action.

use super::body::{TEXT_X, row_layout};
use super::layout::RowLayout;
use super::model::RowKey;
use super::{Table, TableData};
use crate::command::Command;
use crate::data::FieldValue;
use crate::dialog::{MessageBoxButtons, MessageBoxKind};
use crate::event::{Event, Key, KeyEvent};
use crate::view::{Context, Point, Rect, SelectMode, View, ViewId};
use crate::widgets::input_line::{InputLine, LimitMode};
use crate::widgets::list_viewer;

/// The byte limit of a cell editor (one less is the longest text; a longer
/// cell text raises it for that editor).
const EDIT_LIMIT: i32 = 4096;

/// An open cell editor.
pub(super) struct CellEdit {
    /// The editor's id in the table's group.
    pub(super) editor: ViewId,
    /// The edited row.
    pub(super) key: RowKey,
    /// The edited model column.
    pub(super) col: usize,
    /// An error box about this edit is (or may be) on screen: the focus loss
    /// it causes does not cancel the edit. Cleared by the next focus gain,
    /// key or click.
    box_open: bool,
    /// Dropped without a [`Context`] (a programmatic focus move): hidden, and
    /// closed at the next event, which goes on as if there were no editor.
    dropped: bool,
}

impl TableData {
    /// The visible, editable model columns in display order: the stops of
    /// the cell cursor.
    pub(super) fn editable_cols(&self) -> Vec<usize> {
        self.col_order
            .iter()
            .copied()
            .filter(|&c| self.columns[c].editable && !self.columns[c].hidden)
            .collect()
    }

    /// The model column under the cell cursor: the remembered one while it is
    /// still visible and editable, else the first editable column. `None`
    /// when no visible column is editable (there is no cursor).
    pub(super) fn cursor_col(&self) -> Option<usize> {
        let cols = self.editable_cols();
        self.cursor
            .filter(|c| cols.contains(c))
            .or_else(|| cols.first().copied())
    }

    /// Model column `col`'s cells in `l`: `[start, end)` in content cells,
    /// `None` when it takes no cells.
    pub(super) fn col_span(&self, l: &RowLayout, col: usize) -> Option<(i32, i32)> {
        let lc = self.display_pos(col)? + l.frozen;
        l.spans()
            .into_iter()
            .find(|&(i, _, _)| i == lc)
            .map(|(_, s, e)| (s, e))
    }

    /// Scroll the columns of a view `size_x` wide so model column `col` is
    /// in view (its start, when it is wider than the view). Returns whether
    /// the offset changed.
    pub(super) fn reveal_col(&mut self, size_x: i32, col: usize) -> bool {
        let l = row_layout(self, size_x);
        let Some((s, e)) = self.col_span(&l, col) else {
            return false;
        };
        let old = l.clamp(self.h_offset);
        let off = if s < old {
            s
        } else if e > old.saturating_add(l.view) {
            (e - l.view).min(s)
        } else {
            old
        };
        self.h_offset = l.clamp(off);
        self.h_offset != old
    }
}

/// The table-local x range `[x0, x1)` model column `col`'s cells cover on
/// screen, clipped to the scrolling part of a view `size_x` wide; `None`
/// when no cell of it is on screen.
pub(super) fn cell_x(d: &TableData, l: &RowLayout, size_x: i32, col: usize) -> Option<(i32, i32)> {
    let (s, e) = d.col_span(l, col)?;
    let lo = TEXT_X + l.frozen_width;
    let x0 = (TEXT_X + l.text_x(s, d.h_offset)).max(lo);
    let x1 = (TEXT_X + l.text_x(e, d.h_offset)).min(size_x);
    (x0 < x1).then_some((x0, x1))
}

/// The editor bounds over model column `col` of display row `disp`, in a
/// table of `size` whose rows start at `top_item`: the cell plus the cell on
/// either side (the input line draws its text from its second cell and its
/// scroll arrows on the outer ones), clipped to the scrolling part. `None`
/// when the cell is not on screen.
fn editor_rect(d: &TableData, size: Point, top_item: i32, disp: usize, col: usize) -> Option<Rect> {
    let y = 1 + disp as i32 - top_item;
    if y < 1 || y >= size.y {
        return None;
    }
    let l = row_layout(d, size.x);
    let (x0, x1) = cell_x(d, &l, size.x, col)?;
    let lo = TEXT_X + l.frozen_width - 1;
    Some(Rect::new((x0 - 1).max(lo), y, (x1 + 1).min(size.x), y + 1))
}

impl Table {
    /// Whether a cell editor is open.
    pub fn is_editing(&self) -> bool {
        self.edit.is_some()
    }

    /// The model column under the cell cursor, `None` when no visible column
    /// is [`editable`](super::ColumnSpec::editable).
    pub fn cursor_column(&self) -> Option<usize> {
        self.data.borrow().cursor_col()
    }

    /// The open editor.
    pub(super) fn editor_mut(&mut self) -> Option<&mut InputLine> {
        let id = self.edit.as_ref()?.editor;
        self.group
            .child_mut(id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<InputLine>())
    }

    /// Lay the editor over its cell, scrolling the cell into view first when
    /// `reveal` (columns and rows; the bars catch up on the next publish).
    /// An editor whose cell is not on screen is hidden. Returns whether it
    /// is on screen.
    pub(super) fn place_editor(&mut self, reveal: bool) -> bool {
        let Some(e) = &self.edit else {
            return false;
        };
        let (editor, key, col) = (e.editor, e.key.clone(), e.col);
        let size = self.group.state().size;
        let disp = if e.dropped {
            None
        } else {
            self.data.borrow().display_of(&key)
        };
        if let Some(disp) = disp
            && reveal
        {
            self.data.borrow_mut().reveal_col(size.x, col);
            let lv = self.body_mut().lv_state();
            let (row, rows) = (disp as i32, lv.state.size.y);
            if row < lv.top_item {
                lv.top_item = row;
            } else if rows > 0 && row >= lv.top_item + rows {
                lv.top_item = row - rows + 1;
            }
        }
        let top = self.body_mut().lv_state().top_item;
        let rect = disp.and_then(|disp| editor_rect(&self.data.borrow(), size, top, disp, col));
        if let Some(v) = self.group.child_mut(editor) {
            if let Some(r) = rect {
                v.change_bounds(r);
            }
            v.state_mut().state.visible = rect.is_some();
        }
        rect.is_some()
    }

    /// Open an editor on model column `col` of the focused row, pre-filled
    /// with the cell text, all selected. Returns whether it opened (there is
    /// a row and the cell can be shown).
    fn start_edit(&mut self, col: usize, ctx: &mut Context) -> bool {
        let Some(key) = self.focused_key() else {
            return false;
        };
        let (text, factory) = {
            let mut d = self.data.borrow_mut();
            d.cursor = Some(col);
            let row = d.order[d.focused];
            (
                d.model.cell(row, col).into_owned(),
                d.columns[col].validator.clone(),
            )
        };
        let limit = EDIT_LIMIT.max(i32::try_from(text.len()).unwrap_or(i32::MAX - 1) + 1);
        let editor = InputLine::new(
            Rect::new(0, 0, 0, 0),
            limit,
            factory.map(|f| f.make()),
            LimitMode::MaxBytes,
        );
        let id = self.group.insert(Box::new(editor));
        self.edit = Some(CellEdit {
            editor: id,
            key,
            col,
            box_open: false,
            dropped: false,
        });
        if !self.place_editor(true) {
            self.close_editor(ctx);
            return false;
        }
        // Loaded once placed: the input line scrolls the text to its width.
        if let Some(editor) = self.editor_mut() {
            editor.set_value(FieldValue::Text(text));
        }
        self.publish_h(ctx);
        self.group.set_current(Some(id), SelectMode::Normal, ctx);
        true
    }

    /// Remove the editor; the row body is current again.
    fn close_editor(&mut self, ctx: &mut Context) {
        if let Some(e) = self.edit.take() {
            self.group.remove(e.editor, ctx);
        }
    }

    /// Close the editor without storing its text.
    pub fn cancel_edit(&mut self, ctx: &mut Context) {
        self.close_editor(ctx);
    }

    /// Whether the edited cell still exists: the edit was not dropped, its
    /// row is in the model and its column is visible and editable.
    pub(super) fn edit_alive(&self) -> bool {
        let Some(e) = &self.edit else {
            return false;
        };
        let d = self.data.borrow();
        !e.dropped
            && d.display_of(&e.key).is_some()
            && d.columns
                .get(e.col)
                .is_some_and(|c| c.editable && !c.hidden)
    }

    /// Drop the edit without a [`Context`] when focus moves to another row
    /// than `key`'s: the editor hides and the next event closes it.
    pub(super) fn drop_edit_unless(&mut self, key: &RowKey) {
        if let Some(e) = &mut self.edit
            && e.key != *key
        {
            e.dropped = true;
        }
    }

    /// Cancel the edit when focus moves to another row than `key`'s, so the
    /// editor and the focused row never disagree.
    pub(super) fn cancel_edit_unless(&mut self, key: &RowKey, ctx: &mut Context) {
        if self.edit.as_ref().is_some_and(|e| e.key != *key) {
            self.cancel_edit(ctx);
        }
    }

    /// Scroll the edited cell back into view (columns and rows), publishing
    /// the bars, and lay the editor over it. Returns whether it is on screen.
    fn reveal_editor(&mut self, ctx: &mut Context) -> bool {
        let Some(e) = &self.edit else {
            return false;
        };
        let (key, col) = (e.key.clone(), e.col);
        let size_x = self.group.state().size.x;
        let disp = {
            let mut d = self.data.borrow_mut();
            d.reveal_col(size_x, col);
            d.display_of(&key)
        };
        if let Some(disp) = disp {
            // The edited row is the focused one: focusing it again scrolls it
            // into view and moves the vertical bar.
            list_viewer::focus_item_num(self.body_mut(), disp as i32, ctx);
        }
        self.publish_h(ctx);
        self.place_editor(false)
    }

    /// The dialog's [`valid`](View::valid) check with an editor open:
    /// [`Command::CANCEL`] cancels the edit; any other end command commits
    /// it, and a refusal (its box is up) vetoes the command, like a field
    /// whose validator refuses. The status check
    /// ([`Command::VALID`]) and a focus release leave the edit alone.
    pub(super) fn edit_valid(&mut self, cmd: Command, ctx: &mut Context) -> bool {
        if self.edit.is_none() || cmd == Command::VALID || cmd == Command::RELEASED_FOCUS {
            return true;
        }
        if cmd == Command::CANCEL {
            self.cancel_edit(ctx);
            return true;
        }
        let closed = self.commit_edit(ctx);
        self.drain(ctx);
        closed
    }

    /// Store the editor's text: the column's validator must accept it, then
    /// the model's [`set_cell`](super::TableModel::set_cell). A refusal shows
    /// its message box and keeps the editor open. On success the sort is
    /// re-applied, focus follows the row's key and
    /// [`Command::TABLE_CELL_EDITED`] is queued; an unchanged text just
    /// closes the editor. Returns whether the editor closed.
    pub(super) fn commit_edit(&mut self, ctx: &mut Context) -> bool {
        let Some(e) = &self.edit else {
            return false;
        };
        let (key, col) = (e.key.clone(), e.col);
        let Some(editor) = self.editor_mut() else {
            self.close_editor(ctx);
            return true;
        };
        let boxes = ctx.message_boxes_requested();
        if !editor.valid(Command::OK, ctx) {
            // The validator re-focused the editor; it may have popped an
            // error box (the base validator shows none).
            if ctx.message_boxes_requested() > boxes {
                self.box_opened();
            }
            return false;
        }
        let text = editor.data.clone();
        let stored = {
            let mut d = self.data.borrow_mut();
            match d.display_of(&key).map(|disp| d.order[disp]) {
                Some(row) if d.model.cell(row, col) != text => Some(
                    d.model
                        .set_cell(row, col, &text)
                        .map(|()| d.model.row_key(row)),
                ),
                // Unchanged, or the row is gone: nothing to store.
                _ => None,
            }
        };
        match stored {
            Some(Err(message)) => {
                ctx.request_message_box(
                    message,
                    MessageBoxKind::Error,
                    MessageBoxButtons::ok(),
                    None,
                    None,
                );
                self.box_opened();
                false
            }
            Some(Ok(new_key)) => {
                self.close_editor(ctx);
                // The edit may move the row (sort) or even re-key it (a
                // model keyed by a cell).
                let idx = {
                    let mut d = self.data.borrow_mut();
                    // A re-keyed row keeps its mark.
                    if new_key != key && d.selected.remove(&key) {
                        d.selected.insert(new_key.clone());
                    }
                    d.reorder();
                    d.prune_selection();
                    d.pending.push(Command::TABLE_CELL_EDITED);
                    d.display_of(&new_key).unwrap_or(d.focused)
                };
                list_viewer::focus_item_num(self.body_mut(), idx as i32, ctx);
                true
            }
            None => {
                self.close_editor(ctx);
                true
            }
        }
    }

    /// An error box about the edit is coming up.
    fn box_opened(&mut self) {
        if let Some(e) = &mut self.edit {
            e.box_open = true;
        }
    }

    /// The table's focus changed: losing it cancels an open edit, unless an
    /// error box about the edit took it.
    pub(super) fn edit_focus(&mut self, focused: bool, ctx: &mut Context) {
        match &mut self.edit {
            Some(e) if focused => e.box_open = false,
            Some(e) if !e.box_open => self.cancel_edit(ctx),
            _ => {}
        }
    }

    /// An event reaching the table while an editor is open: Enter commits,
    /// Esc cancels, Tab/Shift-Tab commit and move to the next/previous
    /// editable cell of the row (past the last/first one the edit ends and
    /// Tab goes on, so the focus moves); a press outside the editor cancels
    /// (and goes on). A key that finds the editor scrolled off screen scrolls
    /// it back first; one that finds the cell gone ends the edit and is used
    /// up (unless the app dropped the edit). Returns whether `ev` was used.
    pub(super) fn edit_event(&mut self, ev: &Event, ctx: &mut Context) -> bool {
        let is_key = matches!(ev, Event::KeyDown(_));
        if (is_key || matches!(ev, Event::MouseDown(_)))
            && let Some(e) = &mut self.edit
        {
            e.box_open = false;
        }
        if !self.edit_alive() {
            let dropped = self.edit.as_ref().is_some_and(|e| e.dropped);
            self.cancel_edit(ctx);
            return is_key && !dropped;
        }
        if !self.place_editor(false) && is_key && !self.reveal_editor(ctx) {
            // No room to show the cell at all.
            self.cancel_edit(ctx);
            return true;
        }
        match *ev {
            Event::KeyDown(ke) if plain(ke) && ke.key == Key::Enter => {
                self.commit_edit(ctx);
                true
            }
            Event::KeyDown(ke) if plain(ke) && ke.key == Key::Esc => {
                self.cancel_edit(ctx);
                true
            }
            Event::KeyDown(ke) if !ke.modifiers.ctrl && !ke.modifiers.alt && ke.key == Key::Tab => {
                let col = self.edit.as_ref().map(|e| e.col);
                if !self.commit_edit(ctx) {
                    return true;
                }
                // Past the last stop the Tab goes on and moves the focus.
                col.is_some_and(|col| self.edit_neighbour(col, !ke.modifiers.shift, ctx))
            }
            Event::MouseDown(me) => {
                let editor = self.edit.as_ref().map(|e| e.editor);
                let inside = editor
                    .and_then(|id| self.group.child_mut(id))
                    .is_some_and(|v| v.state().get_bounds().contains(me.position));
                if !inside {
                    self.cancel_edit(ctx);
                }
                false
            }
            _ => false,
        }
    }

    /// Open the editor on the editable column after (`forwards`) or before
    /// model column `col` on the focused row; past either end the cursor
    /// stays on `col` and no editor opens. Returns whether there was a
    /// neighbour.
    fn edit_neighbour(&mut self, col: usize, forwards: bool, ctx: &mut Context) -> bool {
        let next = {
            let d = self.data.borrow();
            let cols = d.editable_cols();
            cols.iter().position(|&c| c == col).and_then(|i| {
                if forwards {
                    cols.get(i + 1).copied()
                } else {
                    i.checked_sub(1).map(|j| cols[j])
                }
            })
        };
        match next {
            Some(next) => {
                self.start_edit(next, ctx);
                true
            }
            None => false,
        }
    }

    /// An event reaching the table while no editor is open: F2 edits the
    /// cell under the cursor, a double-click on an editable cell edits that
    /// cell. Returns whether `ev` was used.
    pub(super) fn start_event(&mut self, ev: &Event, ctx: &mut Context) -> bool {
        match *ev {
            Event::KeyDown(ke) if plain(ke) && !ke.modifiers.shift && ke.key == Key::F(2) => {
                match self.cursor_column() {
                    Some(col) => {
                        self.start_edit(col, ctx);
                        true
                    }
                    None => false,
                }
            }
            Event::MouseDown(me)
                if me.flags.double_click && me.buttons.left && me.position.y >= 1 =>
            {
                let Some((item, col)) = self.editable_cell_at(me.position) else {
                    return false;
                };
                list_viewer::focus_item_num(self.body_mut(), item, ctx);
                self.start_edit(col, ctx)
            }
            _ => false,
        }
    }

    /// The display row and model column of the editable cell under
    /// table-local `at`, if there is one.
    fn editable_cell_at(&mut self, at: Point) -> Option<(i32, usize)> {
        let size_x = self.group.state().size.x;
        let lv = self.body_mut().lv_state();
        let item = at.y - 1 + lv.top_item;
        if item >= lv.range {
            return None;
        }
        let d = self.data.borrow();
        let l = row_layout(&d, size_x);
        let col = l
            .column_at(at.x - TEXT_X, d.h_offset)
            .and_then(|lc| d.model_col(lc))?;
        (d.columns[col].editable).then_some((item, col))
    }
}

/// Whether `ke` has neither Ctrl nor Alt (Shift is checked per key).
fn plain(ke: KeyEvent) -> bool {
    !ke.modifiers.ctrl && !ke.modifiers.alt
}
