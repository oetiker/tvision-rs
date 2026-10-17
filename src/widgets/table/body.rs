//! The two children of a [`Table`](super::Table): the one-row column header
//! and the row list. Both read the table's shared data; neither is public.

use crate::command::Command;
use crate::event::{Event, Key};
use crate::theme::Role;
use crate::view::{Context, DrawCtx, GrowMode, Point, Rect, StateFlag, View, ViewId, ViewState};
use crate::widgets::list_viewer::{self, ListViewer, ListViewerState};

use std::borrow::Cow;

use super::edit::cell_x;
use super::layout::{ColumnSpec, RowLayout, format_line, header_line, slice_cols};
use super::model::{RowKey, TableModel};
use super::{SelectionMode, Shared, TableData};
use crate::widgets::cluster::check_box;

/// The cell the row text (and the header text) starts at: the list viewer
/// leaves column 0 of every row blank, and the header lines up with it.
pub(super) const TEXT_X: i32 = 1;

/// The width available to the columns in a view `size_x` wide.
fn text_width(size_x: i32) -> i32 {
    (size_x - TEXT_X).max(0)
}

/// The row geometry for a view `size_x` wide: the layout widths (the frozen
/// check column first in multi mode) and the horizontal scroll extent.
pub(super) fn row_layout(d: &TableData, size_x: i32) -> RowLayout {
    RowLayout::new(&d.layout_columns(), d.check_cols(), text_width(size_x))
}

// ---------------------------------------------------------------------------
// TableHeader
// ---------------------------------------------------------------------------

/// The column-label row, in display order, with the drop indicator of a column
/// drag. Not selectable and draw-only: the table handles every header press
/// itself (it owns the drag tracking, the sort and the popup).
pub(super) struct TableHeader {
    state: ViewState,
    data: Shared,
}

impl TableHeader {
    /// A header one row high and `width` wide at the top of the table.
    pub(super) fn new(width: i32, data: Shared) -> Self {
        let mut state = ViewState::new(Rect::new(0, 0, width, 1));
        state.grow_mode = GrowMode {
            hi_x: true,
            ..Default::default()
        };
        TableHeader { state, data }
    }
}

impl View for TableHeader {
    fn state(&self) -> &ViewState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        let style = ctx.style(Role::TableHeader);
        ctx.fill(self.state.get_extent(), ' ', style);
        let d = self.data.borrow();
        let l = row_layout(&d, self.state.size.x);
        // The sort marker goes on the sorted column's display position.
        let sort = d
            .sort
            .and_then(|(c, dir)| d.col_order.iter().position(|&m| m == c).map(|p| (p, dir)));
        let sep = d.separators;
        let labels = header_line(
            &d.display_columns(),
            &l.widths[l.frozen..],
            sort,
            sep.header_char(),
        );
        // The labels scroll with the rows; the check column's header cell is
        // blank.
        ctx.put_str(
            TEXT_X,
            0,
            &l.compose("", &labels, d.h_offset, sep.header_char()),
            style,
        );
        if let Some(x) = d.drop_x {
            ctx.put_char(x, 0, DROP_MARK, ctx.style(Role::ListFocused));
        }
    }
}

/// The drop indicator a column drag draws on the target gap.
const DROP_MARK: char = '┃';

/// A model seen with its columns in display order: display column `i` is
/// model column `cols[i]`, so the layout helpers format a moved column's
/// cells under its label.
struct DisplayModel<'a> {
    model: &'a dyn TableModel,
    cols: &'a [usize],
}

impl TableModel for DisplayModel<'_> {
    fn row_count(&self) -> usize {
        self.model.row_count()
    }

    fn cell(&self, row: usize, col: usize) -> Cow<'_, str> {
        match self.cols.get(col) {
            Some(&c) => self.model.cell(row, c),
            None => Cow::Borrowed(""),
        }
    }

    fn row_key(&self, row: usize) -> RowKey {
        self.model.row_key(row)
    }
}

// ---------------------------------------------------------------------------
// TableBody
// ---------------------------------------------------------------------------

/// The row list: a single-column [`ListViewer`] whose item text is the
/// formatted row. Keys, mouse, focus colours, scroll-bar brokering and the
/// type-to-search all come from the shared list-viewer functions.
pub(super) struct TableBody {
    lv: ListViewerState,
    data: Shared,
    /// The row geometry for the current view width, refreshed by every
    /// `draw` before the shared draw calls back into `get_text`.
    layout: RowLayout,
    /// The column specs in display order, refreshed with `layout`.
    display: Vec<ColumnSpec>,
}

impl TableBody {
    /// The row list below the header, wired to the optional vertical bar.
    /// The horizontal bar, if any, is set later by the table; it moves the
    /// table's `h_offset`, never the list viewer's own `indent` (that would
    /// scroll the frozen check column too).
    pub(super) fn new(bounds: Rect, v_scroll_bar: Option<ViewId>, data: Shared) -> Self {
        let mut lv = ListViewerState::new(bounds, 1, None, v_scroll_bar);
        lv.state.grow_mode = GrowMode {
            hi_x: true,
            hi_y: true,
            ..Default::default()
        };
        TableBody {
            lv,
            data,
            layout: RowLayout::new(&[], 0, 0),
            display: Vec::new(),
        }
    }

    /// Whether view-local `x` falls on the check cell (multi mode only).
    fn on_check_cell(&self, x: i32) -> bool {
        let d = self.data.borrow();
        if d.mode != SelectionMode::Multi {
            return false;
        }
        let l = row_layout(&d, self.lv.state.size.x);
        l.column_at(x - TEXT_X, d.h_offset) == Some(0)
    }

    /// ←/→ (no Ctrl/Alt/Shift) move the cell cursor to the previous/next editable
    /// column, scrolling it into view; returns whether the key was used. With
    /// no cell cursor, or at the first/last editable column, the key is left
    /// to [`scroll_key`](Self::scroll_key), so the columns past the cursor's
    /// stops can still be scrolled to.
    fn cursor_key(&mut self, ev: &Event, ctx: &mut Context) -> bool {
        let Event::KeyDown(ke) = *ev else {
            return false;
        };
        // Shift+←/→ stay free (they bubble).
        if ke.modifiers.ctrl || ke.modifiers.alt || ke.modifiers.shift {
            return false;
        }
        let forwards = match ke.key {
            Key::Left => false,
            Key::Right => true,
            _ => return false,
        };
        let off = {
            let mut d = self.data.borrow_mut();
            let cols = d.editable_cols();
            let Some(i) = d
                .cursor_col()
                .and_then(|c| cols.iter().position(|&x| x == c))
            else {
                return false;
            };
            let next = if forwards {
                cols.get(i + 1).copied()
            } else {
                i.checked_sub(1).map(|j| cols[j])
            };
            let Some(next) = next else {
                return false;
            };
            d.cursor = Some(next);
            d.reveal_col(self.lv.state.size.x, next)
                .then_some(d.h_offset)
        };
        if let (Some(off), Some(h)) = (off, self.lv.h_scroll_bar) {
            ctx.request_scroll_bar_params(h, Some(off), None, None, None, None);
        }
        true
    }

    /// A left press at view-local `at` on a row's editable cell puts the
    /// cell cursor there.
    fn cursor_to(&mut self, at: Point, left: bool) {
        if !left || at.y + self.lv.top_item >= self.lv.range {
            return;
        }
        let x = at.x;
        let mut d = self.data.borrow_mut();
        let l = row_layout(&d, self.lv.state.size.x);
        let col = l
            .column_at(x - TEXT_X, d.h_offset)
            .and_then(|lc| d.model_col(lc))
            .filter(|&c| d.columns[c].editable);
        if col.is_some() {
            d.cursor = col;
        }
    }

    /// Paint the cell cursor over the focused row's cell in the cursor
    /// column, while the rows have the focus.
    fn draw_cursor(&self, ctx: &mut DrawCtx) {
        let lv = &self.lv;
        let y = lv.focused - lv.top_item;
        if !lv.state.state.focused || lv.range <= 0 || !(0..lv.state.size.y).contains(&y) {
            return;
        }
        let d = self.data.borrow();
        let Some((x0, x1)) = d
            .cursor_col()
            .and_then(|c| cell_x(&d, &self.layout, lv.state.size.x, c))
        else {
            return;
        };
        drop(d);
        let text = slice_cols(&self.get_text(lv.focused), x0 - TEXT_X, x1 - x0);
        let style = ctx.style(Role::TableCellCursor);
        ctx.fill(Rect::new(x0, y, x1, y + 1), ' ', style);
        ctx.put_str(x0, y, &text, style);
    }

    /// ←/→ (no Ctrl/Alt/Shift) scroll the columns one cell while they overflow the
    /// view; returns whether the key was used. A table without overflow lets
    /// the keys bubble.
    fn scroll_key(&mut self, ev: &Event, ctx: &mut Context) -> bool {
        let Event::KeyDown(ke) = *ev else {
            return false;
        };
        // Shift+←/→ stay free (they bubble).
        if ke.modifiers.ctrl || ke.modifiers.alt || ke.modifiers.shift {
            return false;
        }
        let step = match ke.key {
            Key::Left => -1,
            Key::Right => 1,
            _ => return false,
        };
        let off = {
            let mut d = self.data.borrow_mut();
            let l = row_layout(&d, self.lv.state.size.x);
            if l.max_offset() == 0 {
                return false;
            }
            d.h_offset = l.clamp(d.h_offset.saturating_add(step));
            d.h_offset
        };
        if let Some(h) = self.lv.h_scroll_bar {
            ctx.request_scroll_bar_params(h, Some(off), None, None, None, None);
        }
        true
    }

    /// Mutable access to the list-viewer state, for the table's focus moves.
    pub(super) fn lv_state(&mut self) -> &mut ListViewerState {
        &mut self.lv
    }
}

impl ListViewer for TableBody {
    fn lv(&self) -> &ListViewerState {
        &self.lv
    }

    fn lv_mut(&mut self) -> &mut ListViewerState {
        &mut self.lv
    }

    fn get_text(&self, item: i32) -> String {
        let d = self.data.borrow();
        let Some(&row) = d.order.get(item as usize) else {
            return String::new();
        };
        let l = &self.layout;
        let Some(rest_widths) = l.widths.get(l.frozen..) else {
            return String::new();
        };
        let model = DisplayModel {
            model: &*d.model,
            cols: &d.col_order,
        };
        let sep = d.separators;
        let line = format_line(&model, row, rest_widths, &self.display, sep.row_char());
        let check = if l.frozen > 0 {
            let marked = d
                .key_at(item as usize)
                .is_some_and(|k| d.selected.contains(&k));
            check_box(marked)
        } else {
            String::new()
        };
        l.compose(&check, &line, d.h_offset, sep.row_char())
    }

    fn is_selected(&self, item: i32) -> bool {
        let d = self.data.borrow();
        match d.mode {
            SelectionMode::Single => item == self.lv.focused,
            SelectionMode::Multi => d
                .key_at(item as usize)
                .is_some_and(|k| d.selected.contains(&k)),
        }
    }

    fn item_role(&self, item: i32) -> Option<Role> {
        let d = self.data.borrow();
        d.order
            .get(item as usize)
            .and_then(|&row| d.model.row_role(row))
    }

    /// Single mode: the row was chosen (Space or a double-click) — the table
    /// broadcasts [`Command::LIST_ITEM_SELECTED`]. Multi mode: toggle the row's
    /// mark — the table broadcasts [`Command::TABLE_SELECTION_CHANGED`]. Either
    /// way the broadcast carries the table's id, so it is queued here and sent
    /// by the table.
    fn select_item(&mut self, item: i32, _ctx: &mut Context) {
        let mut d = self.data.borrow_mut();
        match d.mode {
            SelectionMode::Single => d.pending.push(Command::LIST_ITEM_SELECTED),
            SelectionMode::Multi => {
                if let Some(key) = d.key_at(item as usize) {
                    if !d.selected.remove(&key) {
                        d.selected.insert(key);
                    }
                    d.pending.push(Command::TABLE_SELECTION_CHANGED);
                }
            }
        }
    }

    /// Mirror the focused display row into the shared data, so the table's
    /// `&self` accessors (`value`, `focused_key`) can read it.
    fn on_focus_changed(&mut self, _ctx: &mut Context) {
        self.data.borrow_mut().focused = self.lv.focused.max(0) as usize;
    }
}

impl View for TableBody {
    fn state(&self) -> &ViewState {
        &self.lv.state
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.lv.state
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        {
            let mut d = self.data.borrow_mut();
            self.layout = row_layout(&d, self.lv.state.size.x);
            self.display = d.display_columns();
            // A wider view may have shrunk the overflow under the offset.
            d.h_offset = self.layout.clamp(d.h_offset);
        }
        // The table scrolls by `h_offset` (frozen check column); the list
        // viewer's own indent stays 0.
        self.lv.indent = 0;
        list_viewer::draw(self, ctx);
        self.draw_cursor(ctx);
    }

    /// The shared list-viewer handling, plus: ←/→ move the cell cursor, or
    /// scroll the columns while they overflow; a click on an editable cell
    /// puts the cell cursor there; in multi mode a single click on a row's
    /// check cell also toggles that row's mark (after the click focused it).
    /// A double-click already toggles through `select_item`.
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        if self.cursor_key(ev, ctx) || self.scroll_key(ev, ctx) {
            ev.clear();
            return;
        }
        if let Event::MouseDown(me) = *ev {
            self.cursor_to(me.position, me.buttons.left);
        }
        let check_click = match *ev {
            Event::MouseDown(me) if !me.flags.double_click && self.on_check_cell(me.position.x) => {
                Some(me.position.y + self.lv.top_item)
            }
            _ => None,
        };
        list_viewer::handle_event(self, ev, ctx);
        if let Some(item) = check_click
            && item < self.lv.range
            && item == self.lv.focused
        {
            self.select_item(item, ctx);
        }
    }

    fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
        list_viewer::set_state(self, flag, enable, ctx);
    }

    fn cursor_request(&self) -> Option<Point> {
        list_viewer::focused_cursor(self)
    }

    /// The horizontal bar moves the table's `h_offset` (the check column
    /// stays frozen); the vertical bar goes the list-viewer way.
    fn apply_scroll_sync(&mut self, h: Option<i32>, v: Option<i32>, ctx: &mut Context) {
        if let Some(hv) = h {
            self.data.borrow_mut().h_offset = hv.max(0);
        }
        list_viewer::apply_scroll(self, None, v, ctx);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
}
