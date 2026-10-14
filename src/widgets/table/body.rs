//! The two children of a [`Table`](super::Table): the one-row column header
//! and the row list. Both read the table's shared data; neither is public.

use crate::command::Command;
use crate::event::Event;
use crate::theme::Role;
use crate::view::{Context, DrawCtx, GrowMode, Point, Rect, StateFlag, View, ViewId, ViewState};
use crate::widgets::list_viewer::{self, ListViewer, ListViewerState};

use super::layout::{column_at, format_line, header_line, layout_widths};
use super::{SelectionMode, Shared};

/// The cell the row text (and the header text) starts at: the list viewer
/// leaves column 0 of every row blank, and the header lines up with it.
const TEXT_X: i32 = 1;

/// The width available to the columns in a view `size_x` wide.
fn text_width(size_x: i32) -> i32 {
    (size_x - TEXT_X).max(0)
}

// ---------------------------------------------------------------------------
// TableHeader
// ---------------------------------------------------------------------------

/// The column-label row. Not selectable; a click on a label records the column
/// for the table to sort by (the table drains it after routing the event).
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
        let widths = layout_widths(&d.columns, text_width(self.state.size.x));
        ctx.put_str(TEXT_X, 0, &header_line(&d.columns, &widths, d.sort), style);
    }

    fn handle_event(&mut self, ev: &mut Event, _ctx: &mut Context) {
        if let Event::MouseDown(me) = *ev {
            let mut d = self.data.borrow_mut();
            let widths = layout_widths(&d.columns, text_width(self.state.size.x));
            if let Some(col) = column_at(&widths, me.position.x - TEXT_X) {
                d.header_click = Some(col);
            }
            ev.clear();
        }
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
    /// Column widths for the current view width, refreshed by every `draw`
    /// before the shared draw calls back into `get_text`.
    widths: Vec<i32>,
}

impl TableBody {
    /// The row list below the header, wired to the optional vertical bar.
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
            widths: Vec::new(),
        }
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
        match d.order.get(item as usize) {
            Some(&row) => format_line(&*d.model, row, &self.widths, &d.columns),
            None => String::new(),
        }
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
        self.widths = layout_widths(
            &self.data.borrow().columns,
            text_width(self.lv.state.size.x),
        );
        list_viewer::draw(self, ctx);
    }

    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        list_viewer::handle_event(self, ev, ctx);
    }

    fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
        list_viewer::set_state(self, flag, enable, ctx);
    }

    fn on_bounds_changed(&mut self, ctx: &mut Context) {
        list_viewer::on_bounds_changed(self, ctx);
    }

    fn cursor_request(&self) -> Option<Point> {
        list_viewer::focused_cursor(self)
    }

    fn apply_scroll_sync(&mut self, h: Option<i32>, v: Option<i32>, ctx: &mut Context) {
        list_viewer::apply_scroll(self, h, v, ctx);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
}
