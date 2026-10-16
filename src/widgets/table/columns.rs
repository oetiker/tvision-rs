//! Header interaction: resize a column by dragging the separator right of its
//! label, move it by dragging the label, show or hide columns from a popup,
//! and read or restore the whole column layout.
//!
//! The table, not the header child, handles every header press: the drags
//! track the mouse with the table as the tracked view (so the release reaches
//! [`Table::handle_event`] and its notifications go out with the table's id),
//! and a press-release without a drag sorts, which needs the row body.

use std::sync::Mutex;

use super::body::{TEXT_X, row_layout};
use super::layout::MAX_COLUMN_WIDTH;
use super::{Table, TableData};
use crate::capture::TrackMask;
use crate::command::Command;
use crate::event::{Event, MouseEvent};
use crate::menu::{Menu, popup_menu};
use crate::view::{Context, Point, View};
use crate::widgets::cluster::check_box;

/// The persistent state of one column, as [`Table::column_layout`] reports it
/// and [`Table::set_column_layout`] restores it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnState {
    /// The column's index in the model (and in the `columns` the table was
    /// built with).
    pub column: usize,
    /// Its display position, 0 = leftmost.
    pub order: usize,
    /// Its [`width`](super::ColumnSpec::width).
    pub width: i32,
    /// Its [`flex`](super::ColumnSpec::flex) weight (a user resize sets 0).
    pub flex: u16,
    /// Whether the layout keeps it at `width` (a user resize sets it; see
    /// [`ColumnSpec::fixed`](super::ColumnSpec::fixed)).
    pub fixed: bool,
    /// Whether it is hidden.
    pub hidden: bool,
}

/// A header press being tracked.
pub(super) enum HeaderDrag {
    /// Dragging the separator right of model column `col`, which was `start`
    /// cells wide on screen when pressed at header x `press_x`.
    Resize {
        col: usize,
        press_x: i32,
        start: i32,
        changed: bool,
    },
    /// Pressed on the label of model column `col`; `moving` once the pointer
    /// left the label's cell (a movable column only). A release without a
    /// move sorts.
    Move { col: usize, moving: bool },
}

/// The prefix of the per-column toggle commands.
const TOGGLE_PREFIX: &str = "tv.table_toggle_column.";

/// The command a column popup item posts for model column `col` (see
/// [`Table::toggle_column_command`]). Each name is built once and kept for
/// the life of the process, since a [`Command`] holds a `&'static str`.
fn toggle_command(col: usize) -> Command {
    static NAMES: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let mut names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    while names.len() <= col {
        let name = format!("{TOGGLE_PREFIX}{}", names.len());
        names.push(Box::leak(name.into_boxed_str()));
    }
    Command::custom(names[col])
}

/// The model column a toggle command names.
fn toggle_index(command: Command) -> Option<usize> {
    command.name().strip_prefix(TOGGLE_PREFIX)?.parse().ok()
}

impl TableData {
    /// How many columns are not hidden.
    fn visible_count(&self) -> usize {
        self.columns.iter().filter(|c| !c.hidden).count()
    }
}

impl Table {
    /// The command that shows or hides model column `col` when it reaches
    /// the focused table — what the column popup's items post. Bind it to a
    /// key or a menu item to toggle a column directly; like the popup, it
    /// never hides the last visible column, and it broadcasts
    /// [`Command::TABLE_COLUMNS_CHANGED`].
    pub fn toggle_column_command(col: usize) -> Command {
        toggle_command(col)
    }

    /// Every column's state, in model order: the display position, width,
    /// flex weight, `fixed` flag and hidden flag. Store it and hand it to
    /// [`set_column_layout`](Self::set_column_layout) to restore what the
    /// user arranged.
    pub fn column_layout(&self) -> Vec<ColumnState> {
        let d = self.data.borrow();
        d.columns
            .iter()
            .enumerate()
            .map(|(column, c)| ColumnState {
                column,
                order: d.display_pos(column).unwrap_or(column),
                width: c.width,
                flex: c.flex,
                fixed: c.fixed,
                hidden: c.hidden,
            })
            .collect()
    }

    /// Restore a [`column_layout`](Self::column_layout). Entries for unknown
    /// columns are ignored; the columns `layout` does not name keep their
    /// state and follow the named ones in their current order. Widths are
    /// clamped to `0..=`[`MAX_COLUMN_WIDTH`].
    /// Does not broadcast; the bars catch up on the table's next event.
    pub fn set_column_layout(&mut self, layout: &[ColumnState]) {
        let mut d = self.data.borrow_mut();
        let n = d.columns.len();
        let mut named: Vec<&ColumnState> = layout.iter().filter(|s| s.column < n).collect();
        named.sort_by_key(|s| s.order);
        let mut order: Vec<usize> = Vec::with_capacity(n);
        for s in &named {
            if !order.contains(&s.column) {
                order.push(s.column);
            }
            let c = &mut d.columns[s.column];
            c.width = s.width.clamp(0, MAX_COLUMN_WIDTH);
            c.flex = s.flex;
            c.fixed = s.fixed;
            c.hidden = s.hidden;
        }
        let rest: Vec<usize> = d
            .col_order
            .iter()
            .copied()
            .filter(|c| !order.contains(c))
            .collect();
        order.extend(rest);
        d.col_order = order;
        drop(d);
        self.needs_publish = true;
    }

    /// The column popup, the column drags and the toggle commands. Returns
    /// whether `ev` was used.
    pub(super) fn header_event(&mut self, ev: &Event, ctx: &mut Context) -> bool {
        match *ev {
            Event::MouseDown(me) if me.position.y == 0 && self.drag.is_none() => {
                self.header_press(me, ctx);
                true
            }
            Event::MouseMove(me) if self.drag.is_some() => {
                self.header_drag(me.position.x, ctx);
                true
            }
            // Only a tracked release ends a drag: an untracked up reaching
            // the table through normal routing is left alone.
            Event::MouseUp(me) if self.drag.is_some() => {
                self.header_release(me.position.x, ctx);
                true
            }
            Event::Command(c) if c == Command::TABLE_COLUMNS => {
                self.open_column_popup(Point::new(TEXT_X, 0), ctx);
                true
            }
            Event::Command(c) => match toggle_index(c) {
                Some(col) if col < self.data.borrow().columns.len() => {
                    self.user_toggle(col, ctx);
                    true
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// A press on the header row: right button → the column popup; left
    /// button on a resize handle → start resizing, on a label → start a
    /// move-or-sort.
    fn header_press(&mut self, me: MouseEvent, ctx: &mut Context) {
        if me.buttons.right {
            self.open_column_popup(me.position, ctx);
            return;
        }
        if !me.buttons.left {
            return;
        }
        let x = me.position.x;
        let drag = {
            let d = self.data.borrow();
            let l = row_layout(&d, self.group.state().size.x);
            let tx = x - TEXT_X;
            if let Some(lc) = l.separator_at(tx, d.h_offset)
                && let Some(col) = d.model_col(lc)
            {
                d.columns[col].resizable.then_some(HeaderDrag::Resize {
                    col,
                    press_x: x,
                    start: l.widths[lc],
                    changed: false,
                })
            } else {
                // Layout columns before `frozen` are the check column: its
                // blank header cell does nothing.
                l.column_at(tx, d.h_offset)
                    .filter(|&lc| lc >= l.frozen)
                    .and_then(|lc| d.model_col(lc))
                    .map(|col| HeaderDrag::Move { col, moving: false })
            }
        };
        let Some(drag) = drag else { return };
        match self.group.state().id() {
            Some(id) => {
                ctx.start_mouse_track(
                    id,
                    self.abs_origin,
                    TrackMask {
                        mouse_move: true,
                        ..Default::default()
                    },
                );
                self.drag = Some(drag);
            }
            // Not inserted, so nothing can track the drag: a label press
            // sorts at once.
            None => {
                if let HeaderDrag::Move { col, .. } = drag {
                    let sort = self.click_sort(col);
                    self.user_sort(sort, ctx);
                }
            }
        }
    }

    /// The pointer moved to header x `x` while a press is held.
    fn header_drag(&mut self, x: i32, ctx: &mut Context) {
        match self.drag {
            Some(HeaderDrag::Resize {
                col,
                press_x,
                start,
                ref mut changed,
            }) => {
                let mut d = self.data.borrow_mut();
                let c = &mut d.columns[col];
                let width = start
                    .saturating_add(x - press_x)
                    .clamp(c.min_width.max(1), MAX_COLUMN_WIDTH);
                if !*changed && width == start {
                    return;
                }
                // A dragged width is a fixed width.
                *changed = true;
                c.width = width;
                c.flex = 0;
                c.fixed = true;
                drop(d);
                self.publish_h(ctx);
            }
            Some(HeaderDrag::Move {
                col,
                ref mut moving,
            }) => {
                let mut d = self.data.borrow_mut();
                if !*moving {
                    let off_label = d.columns[col].movable
                        && label_span(&d, self.group.state().size.x, col)
                            .is_none_or(|(s, e)| x < s || x >= e);
                    if !off_label {
                        return;
                    }
                    *moving = true;
                }
                let size_x = self.group.state().size.x;
                d.drop_x = drop_gap(&d, size_x, x).and_then(|(_, gx)| gx);
            }
            None => {}
        }
    }

    /// The press ends at header x `x`: commit a resize or a move, or sort on
    /// a click.
    fn header_release(&mut self, x: i32, ctx: &mut Context) {
        let Some(drag) = self.drag.take() else { return };
        self.data.borrow_mut().drop_x = None;
        match drag {
            HeaderDrag::Resize { changed, .. } => {
                if changed {
                    self.columns_changed();
                }
            }
            HeaderDrag::Move { col, moving: false } => {
                let sort = self.click_sort(col);
                self.user_sort(sort, ctx);
            }
            HeaderDrag::Move { col, moving: true } => {
                let size_x = self.group.state().size.x;
                let moved = {
                    let mut d = self.data.borrow_mut();
                    match drop_gap(&d, size_x, x) {
                        Some((gap, _)) => move_column(&mut d, size_x, col, gap),
                        None => false,
                    }
                };
                if moved {
                    self.publish_h(ctx);
                    self.columns_changed();
                }
            }
        }
    }

    /// Open the column popup at table-local `at`: one item per column in
    /// display order, `[X]` when shown, posting its toggle command.
    fn open_column_popup(&mut self, at: Point, ctx: &mut Context) {
        let menu = {
            let d = self.data.borrow();
            d.col_order
                .iter()
                .fold(Menu::builder(), |b, &c| {
                    let spec = &d.columns[c];
                    let label = format!("{} {}", check_box(!spec.hidden), spec.label);
                    b.command(label, toggle_command(c))
                })
                .build()
        };
        // Before the first draw the screen size is unknown; the owner's size
        // is the best bound there is.
        let screen = if self.screen.x > 0 && self.screen.y > 0 {
            self.screen
        } else {
            ctx.owner_size()
        };
        popup_menu(self.abs_origin + at, menu, screen, ctx);
    }

    /// Show or hide model column `col` for the user; the last visible column
    /// stays.
    fn user_toggle(&mut self, col: usize, ctx: &mut Context) {
        {
            let mut d = self.data.borrow_mut();
            let hide = !d.columns[col].hidden;
            if hide && d.visible_count() <= 1 {
                return;
            }
            d.columns[col].hidden = hide;
        }
        self.publish_h(ctx);
        self.columns_changed();
    }

    /// Queue [`Command::TABLE_COLUMNS_CHANGED`] for the drain.
    fn columns_changed(&mut self) {
        self.data
            .borrow_mut()
            .pending
            .push(Command::TABLE_COLUMNS_CHANGED);
    }
}

/// The header x range `[start, end)` model column `col`'s label covers on
/// screen (it may reach outside the view), `None` when it takes no cells.
fn label_span(d: &TableData, size_x: i32, col: usize) -> Option<(i32, i32)> {
    let l = row_layout(d, size_x);
    let lc = d.display_pos(col)? + l.frozen;
    let (_, s, e) = l.spans().into_iter().find(|&(i, _, _)| i == lc)?;
    let x = |cx| TEXT_X + l.text_x(cx, d.h_offset);
    Some((x(s), x(e)))
}

/// The gap a column dropped at header x `x` lands in: its index among the
/// visible scrolling columns (0 = before the first, `n` = after the last),
/// and the header x of the indicator when that gap is on screen. The gap is
/// the one before the first column whose middle lies right of `x`.
fn drop_gap(d: &TableData, size_x: i32, x: i32) -> Option<(usize, Option<i32>)> {
    let l = row_layout(d, size_x);
    let spans = l.spans();
    let last = spans.last()?.2;
    // The pointer in content cells (unclamped: it may be off either edge).
    let cx = (x - TEXT_X - l.frozen_width).saturating_add(l.clamp(d.h_offset));
    let gap = spans
        .iter()
        .position(|&(_, s, e)| cx < s + (e - s) / 2)
        .unwrap_or(spans.len());
    // The separator cell before the gap's column, or the cell after the last.
    let mark = spans.get(gap).map_or(last, |&(_, s, _)| s - 1);
    let gx = TEXT_X + l.text_x(mark, d.h_offset);
    // On screen: from the cell before the scrolling part to its last cell.
    let lo = TEXT_X + l.frozen_width - 1;
    let hi = (TEXT_X + l.frozen_width + l.view).min(size_x - 1);
    Some((gap, (lo..=hi).contains(&gx).then_some(gx.max(0))))
}

/// Move model column `col` into visible gap `gap` (see [`drop_gap`]);
/// returns whether the order changed. Hidden columns keep their places
/// relative to their neighbours.
fn move_column(d: &mut TableData, size_x: i32, col: usize, gap: usize) -> bool {
    let l = row_layout(d, size_x);
    let visible: Vec<usize> = l
        .spans()
        .into_iter()
        .filter_map(|(lc, _, _)| d.model_col(lc))
        .collect();
    let before = visible.get(gap).copied();
    let after = gap.checked_sub(1).and_then(|g| visible.get(g)).copied();
    if before == Some(col) || after == Some(col) {
        return false;
    }
    let Some(from) = d.display_pos(col) else {
        return false;
    };
    d.col_order.remove(from);
    let to = match (before, after) {
        (Some(b), _) => d.display_pos(b),
        (None, Some(a)) => d.display_pos(a).map(|p| p + 1),
        (None, None) => None,
    };
    match to {
        Some(to) => d.col_order.insert(to, col),
        None => {
            d.col_order.insert(from, col);
            return false;
        }
    }
    true
}
