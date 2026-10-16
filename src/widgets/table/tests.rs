//! Behaviour tests and `HeadlessBackend` snapshots for [`Table`].

use super::*;
use crate::backend::{HeadlessBackend, Renderer};
use crate::event::{Key, KeyEvent, KeyModifiers, MouseButtons, MouseEvent, MouseEventFlags};
use crate::screen::Buffer;
use crate::theme::{Role, Theme};
use crate::timer::TimerQueue;
use crate::view::{Deferred, DrawCtx, Point, StateFlag};
use crate::widgets::ListViewer;
use std::collections::VecDeque;

type Broadcasts = Vec<(Command, Option<ViewId>)>;

// -- fixtures -----------------------------------------------------------------

fn columns() -> Vec<ColumnSpec> {
    vec![
        ColumnSpec::new("ID", 4),
        ColumnSpec::new("NAME", 10),
        ColumnSpec::new("STATE", 8),
        ColumnSpec::new("CPU", 5).right(),
    ]
}

/// Rows keyed by their ID column.
fn model(rows: &[[&str; 4]]) -> Box<dyn TableModel> {
    let keys = rows.iter().map(|r| RowKey::from(r[0])).collect();
    let rows = rows
        .iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect();
    Box::new(VecModel::new(rows).with_keys(keys))
}

const ROWS: [[&str; 4]; 4] = [
    ["a1", "web", "running", "12.5"],
    ["b2", "db", "exited", "0"],
    ["c3", "cache", "running", "3.25"],
    ["d4", "worker-long-name", "paused", "100"],
];

/// Columns 4 + 10 + 8 + 5 plus three separators fill exactly the 30 text
/// cells of a 31-wide table (column 0 is the list viewer's blank gutter).
const W: i32 = 31;
const H: i32 = 6;

/// A table inserted into an owner group, so it (and its children) carry ids.
struct Fixture {
    owner: Group,
    id: ViewId,
}

impl Fixture {
    fn new(table: Table) -> Self {
        let mut owner = Group::new(Rect::new(0, 0, 60, 20));
        let id = owner.insert(Box::new(table));
        let mut fx = Fixture { owner, id };
        // Settle currency (owner → table, table → body) and activate the tree,
        // as the event loop does.
        fx.run(|owner, ctx| {
            owner.settle_currency(ctx);
            owner.set_state(StateFlag::Active, true, ctx);
        });
        fx
    }

    fn table() -> Self {
        Fixture::new(Table::new(Rect::new(0, 0, W, H), columns(), None).with_model(model(&ROWS)))
    }

    fn t(&mut self) -> &mut Table {
        self.owner
            .child_mut(self.id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<Table>())
            .unwrap()
    }

    /// Run `f` on the owner with a throwaway context; return the broadcasts.
    fn run(&mut self, f: impl FnOnce(&mut Group, &mut Context)) -> Broadcasts {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            f(&mut self.owner, &mut ctx);
        }
        out.into_iter()
            .filter_map(|e| match e {
                Event::Broadcast { command, source } => Some((command, source)),
                _ => None,
            })
            .collect()
    }

    /// Run `f` on the table; return the deferred effects it queued.
    fn with_deferred(&mut self, f: impl FnOnce(&mut Table, &mut Context)) -> Vec<Deferred> {
        let t = self.t();
        deferred_of(|ctx| f(t, ctx))
    }

    /// Run `f` on the table.
    fn with(&mut self, f: impl FnOnce(&mut Table, &mut Context)) -> Broadcasts {
        let id = self.id;
        self.run(|owner, ctx| {
            let t = owner
                .child_mut(id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<Table>())
                .unwrap();
            f(t, ctx);
        })
    }

    /// Deliver `ev` to the table; return the event (cleared or not) and the
    /// table's own broadcasts (focus chatter filtered out).
    fn send(&mut self, ev: Event) -> (Event, Broadcasts) {
        let mut ev = ev;
        let out = self.with(|t, ctx| t.handle_event(&mut ev, ctx));
        let out = out
            .into_iter()
            .filter(|(c, _)| *c != Command::RECEIVED_FOCUS && *c != Command::RELEASED_FOCUS)
            .collect();
        (ev, out)
    }

    fn key(&mut self, k: Key) -> Broadcasts {
        self.send(Event::KeyDown(KeyEvent::new(k, KeyModifiers::default())))
            .1
    }

    /// A left click: on a row a press; on the header (`y == 0`) a press
    /// and a release at the same cell, since a header press is tracked and
    /// acts on release.
    fn click(&mut self, x: i32, y: i32) -> Broadcasts {
        let mut out = self.send(mouse_down(x, y, false)).1;
        if y == 0 {
            out.extend(self.send(mouse_at(MouseKind::Up, x, 0)).1);
        }
        out
    }

    /// Press the header at `from`, move through `path`, release at the last
    /// point of `path` (or at `from`); return the broadcasts.
    fn drag(&mut self, from: i32, path: &[i32]) -> Broadcasts {
        let mut out = self.send(mouse_down(from, 0, false)).1;
        for &x in path {
            out.extend(self.send(mouse_at(MouseKind::Move, x, 0)).1);
        }
        let end = path.last().copied().unwrap_or(from);
        out.extend(self.send(mouse_at(MouseKind::Up, end, 0)).1);
        out
    }

    fn focus(&mut self, on: bool) {
        self.run(|owner, ctx| owner.set_state(StateFlag::Focused, on, ctx));
    }

    fn set_model(&mut self, rows: &[[&str; 4]]) {
        let m = model(rows);
        self.with(|t, ctx| t.set_model(m, ctx));
    }

    fn render(&mut self, w: u16, h: u16) -> String {
        render(self.t(), w, h)
    }
}

fn mouse_down(x: i32, y: i32, double: bool) -> Event {
    Event::MouseDown(MouseEvent {
        position: Point::new(x, y),
        buttons: MouseButtons {
            left: true,
            ..Default::default()
        },
        flags: MouseEventFlags {
            double_click: double,
            ..Default::default()
        },
        ..Default::default()
    })
}

#[derive(Clone, Copy)]
enum MouseKind {
    Move,
    Up,
}

/// A left-button move or release at table-local `(x, y)` — what the drag
/// tracking delivers to the table.
fn mouse_at(kind: MouseKind, x: i32, y: i32) -> Event {
    let me = MouseEvent {
        position: Point::new(x, y),
        buttons: MouseButtons {
            left: matches!(kind, MouseKind::Move),
            ..Default::default()
        },
        ..Default::default()
    };
    match kind {
        MouseKind::Move => Event::MouseMove(me),
        MouseKind::Up => Event::MouseUp(me),
    }
}

fn keys(v: Vec<RowKey>) -> Vec<String> {
    v.into_iter().map(|k| k.0).collect()
}

fn render(t: &mut Table, w: u16, h: u16) -> String {
    let theme = Theme::classic_blue();
    let (backend, screen) = HeadlessBackend::new(w, h);
    let mut r = Renderer::new(Box::new(backend));
    r.render(|buf: &mut Buffer| {
        let bounds = t.state().get_bounds();
        let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
        t.draw(&mut dc);
    });
    screen.snapshot()
}

// -- structure ------------------------------------------------------------------

#[test]
fn focus_reaches_the_body() {
    let mut fx = Fixture::table();
    fx.focus(true);
    let t = fx.t();
    assert!(t.state().state.focused, "the table itself is focused");
    assert!(
        t.body_mut().lv().state.state.focused,
        "the focused flag reaches the row body through the group"
    );
    fx.focus(false);
    assert!(!fx.t().body_mut().lv().state.state.focused);
}

/// The `max` and `page_step` the deferred queue leaves on scroll bar `bar`
/// (the last request wins, as when the pump applies them in order).
fn bar_params(deferred: &[Deferred], bar: ViewId) -> (Option<i32>, Option<i32>) {
    let max = deferred.iter().rev().find_map(|d| match d {
        Deferred::ScrollBarSetParams { id, max, .. } if *id == bar => *max,
        _ => None,
    });
    let page = deferred.iter().rev().find_map(|d| match d {
        Deferred::ScrollBarSetParams { id, page_step, .. } if *id == bar => *page_step,
        _ => None,
    });
    (max, page)
}

/// Run `f` with a throwaway context; return the deferred effects it queued.
fn deferred_of(f: impl FnOnce(&mut Context)) -> Vec<Deferred> {
    let mut out = VecDeque::new();
    let mut timers = TimerQueue::new();
    let mut deferred: Vec<Deferred> = vec![];
    {
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        f(&mut ctx);
    }
    deferred
}

#[test]
fn activation_publishes_range_and_steps_to_the_bar_without_any_event() {
    let mut mint = Group::new(Rect::new(0, 0, 4, 4));
    let bar = mint.insert(Box::new(Group::new(Rect::new(0, 0, 1, 1))));
    let mut owner = Group::new(Rect::new(0, 0, 60, 20));
    owner.insert(Box::new(
        Table::new(Rect::new(0, 0, W, H), columns(), Some(bar)).with_model(model(&ROWS)),
    ));
    // No event reaches the table: its owner only activates (as a window or a
    // dialog does when another control holds focus).
    let deferred = deferred_of(|ctx| owner.set_state(StateFlag::Active, true, ctx));
    assert_eq!(
        bar_params(&deferred, bar),
        (Some(3), Some(H - 1 - 1)),
        "range 0..=3 for four rows; page step = body height - 1"
    );
}

#[test]
fn set_model_on_a_live_table_republishes_the_range() {
    let mut mint = Group::new(Rect::new(0, 0, 4, 4));
    let bar = mint.insert(Box::new(Group::new(Rect::new(0, 0, 1, 1))));
    let mut fx = Fixture::new(
        Table::new(Rect::new(0, 0, W, H), columns(), Some(bar)).with_model(model(&ROWS)),
    );
    let m = model(&ROWS[..2]);
    let deferred = deferred_of(|ctx| fx.t().set_model(m, ctx));
    assert_eq!(
        bar_params(&deferred, bar).0,
        Some(1),
        "range 0..=1 for two rows"
    );
}

#[test]
fn as_any_mut_downcasts_to_table() {
    let mut t = Table::new(Rect::new(0, 0, W, H), columns(), None);
    let v: &mut dyn View = &mut t;
    assert!(
        v.as_any_mut()
            .is_some_and(|a| a.downcast_ref::<Table>().is_some())
    );
}

// -- navigation -----------------------------------------------------------------

#[test]
fn keys_move_the_focused_row() {
    let mut fx = Fixture::table();
    fx.focus(true);
    assert_eq!(fx.t().focused_key(), Some(RowKey::from("a1")));
    fx.key(Key::Down);
    assert_eq!(fx.t().focused_key(), Some(RowKey::from("b2")));
    fx.key(Key::End);
    assert_eq!(fx.t().focused_row(), Some(3));
    fx.key(Key::Home);
    assert_eq!(fx.t().focused_row(), Some(0));
}

#[test]
fn unhandled_keys_bubble() {
    let mut fx = Fixture::table();
    fx.focus(true);
    let (ev, _) = fx.send(Event::KeyDown(KeyEvent::new(
        Key::Char('x'),
        KeyModifiers::default(),
    )));
    assert!(!ev.is_nothing(), "'x' must bubble to the owner");
}

#[test]
fn row_click_focuses_that_row() {
    let mut fx = Fixture::table();
    fx.focus(true);
    // y = 3 → body row 2 (the header is row 0).
    fx.click(3, 3);
    assert_eq!(fx.t().focused_key(), Some(RowKey::from("c3")));
}

// -- sorting --------------------------------------------------------------------

#[test]
fn header_click_sorts_ascending_then_descending_and_broadcasts() {
    let mut fx = Fixture::table();
    fx.focus(true);
    let id = Some(fx.id);
    // NAME spans text cells 5..15 → view x 6..16.
    let out = fx.click(8, 0);
    assert_eq!(fx.t().sort_spec(), Some((1, SortDir::Asc)));
    assert_eq!(
        fx.t().row_order(),
        [2, 1, 0, 3],
        "cache < db < web < worker"
    );
    assert_eq!(out, [(Command::TABLE_SORT_CHANGED, id)]);
    let out = fx.click(8, 0);
    assert_eq!(fx.t().sort_spec(), Some((1, SortDir::Desc)));
    assert_eq!(fx.t().row_order(), [3, 0, 1, 2]);
    assert_eq!(out, [(Command::TABLE_SORT_CHANGED, id)]);
}

#[test]
fn header_click_off_a_label_does_not_sort() {
    let mut fx = Fixture::table();
    let out = fx.click(5, 0); // the separator after ID
    assert_eq!(fx.t().sort_spec(), None);
    assert!(out.is_empty());
}

#[test]
fn sorting_keeps_focus_on_its_row() {
    let mut fx = Fixture::table();
    fx.focus(true);
    assert_eq!(fx.t().focused_key(), Some(RowKey::from("a1")));
    fx.with(|t, ctx| t.sort_by(1, SortDir::Asc, ctx));
    assert_eq!(fx.t().focused_key(), Some(RowKey::from("a1")));
    assert_eq!(fx.t().body_mut().lv().focused, 2, "web is third by name");
    fx.with(|t, ctx| t.clear_sort(ctx));
    assert_eq!(fx.t().row_order(), [0, 1, 2, 3]);
    assert_eq!(fx.t().body_mut().lv().focused, 0);
}

#[test]
fn programmatic_sort_does_not_broadcast() {
    let mut fx = Fixture::table();
    let out = fx.with(|t, ctx| t.sort_by(3, SortDir::Asc, ctx));
    assert!(out.is_empty());
    assert_eq!(
        fx.t().row_order(),
        [1, 2, 0, 3],
        "numeric: 0 < 3.25 < 12.5 < 100"
    );
    fx.with(|t, ctx| t.sort_by(9, SortDir::Asc, ctx));
    assert_eq!(
        fx.t().sort_spec(),
        Some((3, SortDir::Asc)),
        "bad column ignored"
    );
}

#[test]
fn sort_commands_cycle_and_flip_skipping_hidden_columns() {
    let mut fx = Fixture::table();
    fx.focus(true);
    let id = Some(fx.id);
    let next = || Event::Command(Command::TABLE_SORT_NEXT);
    let flip = || Event::Command(Command::TABLE_SORT_FLIP);

    let (ev, out) = fx.send(flip());
    assert!(ev.is_nothing(), "consumed");
    assert_eq!(
        fx.t().sort_spec(),
        Some((0, SortDir::Asc)),
        "no sort yet: first column"
    );
    assert_eq!(out, [(Command::TABLE_SORT_CHANGED, id)]);
    fx.send(flip());
    assert_eq!(fx.t().sort_spec(), Some((0, SortDir::Desc)));

    fx.t().set_column_hidden(2, true);
    fx.send(next());
    assert_eq!(fx.t().sort_spec(), Some((1, SortDir::Asc)));
    fx.send(next());
    assert_eq!(
        fx.t().sort_spec(),
        Some((3, SortDir::Asc)),
        "hidden STATE skipped"
    );
    fx.send(next());
    assert_eq!(fx.t().sort_spec(), Some((0, SortDir::Asc)), "wraps");
}

#[test]
fn sort_commands_with_every_column_hidden_do_nothing() {
    let mut fx = Fixture::table();
    fx.focus(true);
    for c in 0..4 {
        fx.t().set_column_hidden(c, true);
    }
    let (_, out) = fx.send(Event::Command(Command::TABLE_SORT_NEXT));
    assert_eq!(fx.t().sort_spec(), None);
    assert!(out.is_empty());
}

// -- reload ---------------------------------------------------------------------

#[test]
fn reload_keeps_focus_on_the_same_key() {
    let mut fx = Fixture::table();
    fx.focus(true);
    fx.key(Key::Down);
    fx.key(Key::Down);
    assert_eq!(fx.t().focused_key(), Some(RowKey::from("c3")));
    let mut rev = ROWS;
    rev.reverse();
    fx.set_model(&rev);
    assert_eq!(fx.t().focused_key(), Some(RowKey::from("c3")));
    assert_eq!(fx.t().body_mut().lv().focused, 1);
}

#[test]
fn reload_reapplies_the_sort() {
    let mut fx = Fixture::table();
    fx.with(|t, ctx| t.sort_by(0, SortDir::Desc, ctx));
    fx.set_model(&[ROWS[2], ROWS[0], ROWS[1]]);
    // Model order c3, a1, b2; sorted by ID descending: c3, b2, a1.
    assert_eq!(fx.t().row_order(), [0, 2, 1]);
}

#[test]
fn reload_clamps_focus_when_the_key_vanishes() {
    let mut fx = Fixture::table();
    fx.focus(true);
    fx.key(Key::End);
    assert_eq!(fx.t().focused_key(), Some(RowKey::from("d4")));
    fx.set_model(&[ROWS[0], ROWS[1]]);
    assert_eq!(fx.t().focused_row(), Some(1), "clamped to the last row");
    fx.set_model(&[]);
    assert_eq!(fx.t().focused_key(), None);
    assert_eq!(fx.t().value(), Some(FieldValue::Text(String::new())));
}

// -- selection ------------------------------------------------------------------

fn multi() -> Fixture {
    let mut fx = Fixture::new(
        Table::new(Rect::new(0, 0, W, H), columns(), None)
            .with_model(model(&ROWS))
            .with_selection_mode(SelectionMode::Multi),
    );
    fx.focus(true);
    fx
}

#[test]
fn space_toggles_marks_and_broadcasts_with_the_table_id() {
    let mut fx = multi();
    let id = Some(fx.id);
    let out = fx.key(Key::Char(' '));
    assert_eq!(keys(fx.t().selected_keys()), ["a1"]);
    assert_eq!(out, [(Command::TABLE_SELECTION_CHANGED, id)]);
    fx.key(Key::Down);
    fx.key(Key::Char(' '));
    assert_eq!(keys(fx.t().selected_keys()), ["a1", "b2"]);
    fx.key(Key::Char(' '));
    assert_eq!(keys(fx.t().selected_keys()), ["a1"], "second Space unmarks");
}

#[test]
fn double_click_toggles_in_multi_mode() {
    let mut fx = multi();
    fx.send(mouse_down(3, 2, true));
    assert_eq!(keys(fx.t().selected_keys()), ["b2"]);
}

#[test]
fn single_mode_choice_broadcasts_list_item_selected_with_the_table_id() {
    let mut fx = Fixture::table();
    fx.focus(true);
    let id = Some(fx.id);
    let (_, out) = fx.send(mouse_down(3, 2, true));
    assert_eq!(out, [(Command::LIST_ITEM_SELECTED, id)]);
    assert_eq!(fx.t().focused_key(), Some(RowKey::from("b2")));
    assert_eq!(fx.key(Key::Char(' ')), [(Command::LIST_ITEM_SELECTED, id)]);
    assert!(
        fx.t().selected_keys().is_empty(),
        "single mode marks nothing"
    );
}

#[test]
fn click_on_the_check_cell_toggles_and_broadcasts() {
    let mut fx = multi();
    let id = Some(fx.id);
    // The check cell spans view x 1..4 (text starts at x 1); y 3 = row c3.
    let out = fx.click(2, 3);
    assert_eq!(
        fx.t().focused_key(),
        Some(RowKey::from("c3")),
        "click focuses"
    );
    assert_eq!(keys(fx.t().selected_keys()), ["c3"]);
    assert_eq!(out, [(Command::TABLE_SELECTION_CHANGED, id)]);
    fx.send(Event::MouseUp(MouseEvent::default()));
    fx.click(2, 3);
    assert!(fx.t().selected_keys().is_empty(), "a second click unmarks");
}

#[test]
fn click_on_a_text_cell_only_focuses() {
    let mut fx = multi();
    let out = fx.click(10, 2);
    assert_eq!(fx.t().focused_key(), Some(RowKey::from("b2")));
    assert!(fx.t().selected_keys().is_empty());
    assert!(out.is_empty());
}

#[test]
fn header_hit_test_accounts_for_the_check_column() {
    let mut fx = multi();
    let out = fx.click(2, 0); // the blank check header cell
    assert_eq!(fx.t().sort_spec(), None);
    assert!(out.is_empty());
    // Widths [check 3, ID 4, NAME 7, STATE 7, CPU 5]: ID spans view x 5..9.
    fx.click(6, 0);
    assert_eq!(fx.t().sort_spec(), Some((0, SortDir::Asc)));
}

#[test]
fn single_mode_has_no_check_column() {
    let mut fx = Fixture::table();
    let screen = fx.render(W as u16, H as u16);
    assert!(!screen.contains("[ ]"), "{screen}");
}

#[test]
fn marks_survive_reload_and_drop_vanished_keys() {
    let mut fx = multi();
    fx.key(Key::Char(' ')); // a1
    fx.key(Key::Down);
    fx.key(Key::Char(' ')); // b2
    let mut rev = ROWS;
    rev.reverse();
    fx.set_model(&rev);
    assert_eq!(keys(fx.t().selected_keys()), ["b2", "a1"], "model order");
    fx.set_model(&[ROWS[1], ROWS[2]]);
    assert_eq!(keys(fx.t().selected_keys()), ["b2"], "a1 vanished");
    fx.set_model(&ROWS);
    assert_eq!(
        keys(fx.t().selected_keys()),
        ["b2"],
        "a vanished mark stays gone"
    );
}

#[test]
fn set_selected_keys_ignores_unknown_keys_and_clear_selection_keeps_focus() {
    let mut fx = multi();
    fx.key(Key::Down);
    fx.t()
        .set_selected_keys(["c3".into(), "zz".into(), "a1".into()]);
    assert_eq!(keys(fx.t().selected_keys()), ["a1", "c3"]);
    fx.t().clear_selection();
    assert!(fx.t().selected_keys().is_empty());
    assert_eq!(fx.t().focused_key(), Some(RowKey::from("b2")));
}

// -- value (D10) ------------------------------------------------------------------

#[test]
fn single_value_is_the_focused_key_and_set_value_focuses_it() {
    let mut fx = Fixture::table();
    assert_eq!(fx.t().value(), Some(FieldValue::Text("a1".into())));
    fx.t().set_value(FieldValue::Text("c3".into()));
    assert_eq!(fx.t().focused_key(), Some(RowKey::from("c3")));
    assert_eq!(fx.t().value(), Some(FieldValue::Text("c3".into())));
    fx.t().set_value(FieldValue::Text("nope".into()));
    assert_eq!(
        fx.t().focused_key(),
        Some(RowKey::from("c3")),
        "unknown key ignored"
    );
    fx.t().set_value(FieldValue::Int(0));
    assert_eq!(
        fx.t().focused_key(),
        Some(RowKey::from("c3")),
        "other shape ignored"
    );
    fx.with(|t, ctx| t.set_value_ctx(FieldValue::Text("d4".into()), ctx));
    assert_eq!(fx.t().focused_key(), Some(RowKey::from("d4")));
}

#[test]
fn multi_value_is_the_marked_keys_in_model_order() {
    let mut fx = multi();
    fx.t().set_value(FieldValue::List(vec![
        FieldValue::Text("c3".into()),
        FieldValue::Text("a1".into()),
    ]));
    assert_eq!(
        fx.t().value(),
        Some(FieldValue::List(vec![
            FieldValue::Text("a1".into()),
            FieldValue::Text("c3".into())
        ]))
    );
    fx.with(|t, ctx| t.set_value_ctx(FieldValue::List(vec![]), ctx));
    assert_eq!(fx.t().value(), Some(FieldValue::List(vec![])));
}

// -- columns --------------------------------------------------------------------

#[test]
fn set_column_hidden_toggles() {
    let mut t = Table::new(Rect::new(0, 0, W, H), columns(), None);
    assert!(!t.is_column_hidden(1));
    t.set_column_hidden(1, true);
    assert!(t.is_column_hidden(1));
    t.set_column_hidden(1, false);
    assert!(!t.is_column_hidden(1));
    t.set_column_hidden(42, true);
    assert!(!t.is_column_hidden(42), "unknown column ignored");
}

#[test]
fn draw_degenerate_sizes_do_not_panic() {
    for (w, h) in [(0, 0), (1, 1), (4, 1), (1, 4), (3, 2)] {
        let mut t = Table::new(Rect::new(0, 0, w, h), columns(), None).with_model(model(&ROWS));
        let _ = render(&mut t, 8, 8);
    }
}

// -- snapshots ----------------------------------------------------------------

#[test]
fn snapshot_focused() {
    let mut fx = Fixture::table();
    fx.focus(true);
    fx.key(Key::Down);
    insta::assert_snapshot!(fx.render(W as u16, H as u16));
}

#[test]
fn snapshot_unfocused() {
    let mut fx = Fixture::table();
    fx.t().set_value(FieldValue::Text("b2".into()));
    insta::assert_snapshot!(fx.render(W as u16, H as u16));
}

#[test]
fn snapshot_sorted_descending() {
    let mut fx = Fixture::table();
    fx.focus(true);
    fx.with(|t, ctx| t.sort_by(3, SortDir::Desc, ctx));
    insta::assert_snapshot!(fx.render(W as u16, H as u16));
}

#[test]
fn snapshot_multi_selected() {
    let mut fx = multi();
    fx.key(Key::Char(' '));
    fx.key(Key::Down);
    fx.key(Key::Down);
    fx.key(Key::Char(' '));
    // Focus stays on the marked c3: its mark must show on the focused colour.
    insta::assert_snapshot!(fx.render(W as u16, H as u16));
}

#[test]
fn snapshot_narrow_truncation() {
    let mut fx =
        Fixture::new(Table::new(Rect::new(0, 0, 20, H), columns(), None).with_model(model(&ROWS)));
    fx.focus(true);
    fx.with(|t, ctx| t.sort_by(2, SortDir::Asc, ctx));
    insta::assert_snapshot!(fx.render(20, H as u16));
}

#[test]
fn snapshot_hidden_column() {
    let mut fx = Fixture::table();
    fx.focus(true);
    fx.t().set_column_hidden(1, true);
    insta::assert_snapshot!(fx.render(W as u16, H as u16));
}

#[test]
fn snapshot_row_role() {
    let rows: Vec<Vec<String>> = ROWS
        .iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect();
    let m = VecModel::new(rows).with_row_roles(vec![
        None,
        Some(Role::Disabled),
        None,
        Some(Role::Error),
    ]);
    let mut fx =
        Fixture::new(Table::new(Rect::new(0, 0, W, H), columns(), None).with_model(Box::new(m)));
    fx.focus(true);
    // Focus row c3: the dimmed b2 and the error d4 show their row roles; on
    // the focused row the focused colour would win.
    fx.key(Key::Down);
    fx.key(Key::Down);
    insta::assert_snapshot!(fx.render(W as u16, H as u16));
}

// -- layout: stretch and horizontal scroll -----------------------------------

/// A bar id minted by a throwaway group (the table only needs the id).
fn mint_bar() -> ViewId {
    let mut mint = Group::new(Rect::new(0, 0, 4, 4));
    mint.insert(Box::new(Group::new(Rect::new(0, 0, 1, 1))))
}

/// The fixture columns, none shrinkable: 4 + 10 + 8 + 5 plus three
/// separators = 30 content cells.
fn fixed_columns() -> Vec<ColumnSpec> {
    columns()
        .into_iter()
        .map(|c| {
            let w = c.width;
            c.min_width(w)
        })
        .collect()
}

/// A multi-mode table `w` wide over the fixed columns: the check cell and
/// its separator take 4 of the `w - 1` text cells, the rest scroll.
fn scrolling(w: i32, h_bar: Option<ViewId>) -> Fixture {
    let mut t = Table::new(Rect::new(0, 0, w, H), fixed_columns(), None)
        .with_model(model(&ROWS))
        .with_selection_mode(SelectionMode::Multi);
    if let Some(bar) = h_bar {
        t = t.with_h_bar(bar);
    }
    let mut fx = Fixture::new(t);
    fx.focus(true);
    fx
}

/// The `value` the deferred queue last sends to scroll bar `bar`.
fn bar_value(deferred: &[Deferred], bar: ViewId) -> Option<i32> {
    deferred.iter().rev().find_map(|d| match d {
        Deferred::ScrollBarSetParams { id, value, .. } if *id == bar => *value,
        _ => None,
    })
}

#[test]
fn flex_column_takes_the_spare_width() {
    let mut cols = columns();
    cols[1] = cols[1].clone().flex(1);
    let mut fx =
        Fixture::new(Table::new(Rect::new(0, 0, W + 6, H), cols, None).with_model(model(&ROWS)));
    let d = fx.t().data.clone();
    let l = body::row_layout(&d.borrow(), W + 6);
    assert_eq!(l.widths, vec![4, 16, 8, 5], "NAME took all six spare cells");
    assert_eq!(fx.t().h_offset(), 0);
}

#[test]
fn columns_that_fit_do_not_scroll_and_arrows_bubble() {
    let mut fx = Fixture::table();
    fx.focus(true);
    for k in [Key::Left, Key::Right] {
        let (ev, _) = fx.send(Event::KeyDown(KeyEvent::new(k, KeyModifiers::default())));
        assert!(!ev.is_nothing(), "{k:?} must bubble: nothing to scroll");
    }
    fx.with(|t, ctx| t.set_h_offset(5, ctx));
    assert_eq!(fx.t().h_offset(), 0, "clamped to no overflow");
}

#[test]
fn arrows_scroll_by_one_cell_within_the_overflow() {
    // 20 text cells − 4 frozen = 16 visible of 30 → overflow 14.
    let bar = mint_bar();
    let mut fx = scrolling(21, Some(bar));
    let (ev, _) = fx.send(Event::KeyDown(KeyEvent::new(
        Key::Right,
        KeyModifiers::default(),
    )));
    assert!(ev.is_nothing(), "Right consumed while the columns overflow");
    assert_eq!(fx.t().h_offset(), 1);
    fx.key(Key::Left);
    fx.key(Key::Left);
    assert_eq!(fx.t().h_offset(), 0, "clamped at the left edge");
    for _ in 0..20 {
        fx.key(Key::Right);
    }
    assert_eq!(fx.t().h_offset(), 14, "clamped at the overflow");
    assert_eq!(fx.t().focused_row(), Some(0), "rows did not move");
    // The bar follows a key scroll.
    let mut ev = Event::KeyDown(KeyEvent::new(Key::Left, KeyModifiers::default()));
    let id = fx.id;
    let mut deferred = vec![];
    {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        fx.owner
            .child_mut(id)
            .unwrap()
            .handle_event(&mut ev, &mut ctx);
    }
    assert_eq!(bar_value(&deferred, bar), Some(13));
}

#[test]
fn the_h_bar_gets_the_overflow_as_range_and_the_view_as_page() {
    let bar = mint_bar();
    let mut owner = Group::new(Rect::new(0, 0, 60, 20));
    owner.insert(Box::new(
        Table::new(Rect::new(0, 0, 21, H), fixed_columns(), None)
            .with_model(model(&ROWS))
            .with_selection_mode(SelectionMode::Multi)
            .with_h_bar(bar),
    ));
    let deferred = deferred_of(|ctx| owner.set_state(StateFlag::Active, true, ctx));
    assert_eq!(bar_params(&deferred, bar), (Some(14), Some(16)));
    assert_eq!(bar_value(&deferred, bar), Some(0));
}

#[test]
fn the_h_bar_moves_the_offset() {
    let bar = mint_bar();
    let mut fx = scrolling(21, Some(bar));
    fx.with(|t, ctx| t.body_mut().apply_scroll_sync(Some(6), None, ctx));
    assert_eq!(fx.t().h_offset(), 6);
    assert_eq!(fx.t().focused_row(), Some(0), "the v branch is untouched");
}

#[test]
fn resize_republishes_both_bars() {
    let (v, h) = (mint_bar(), mint_bar());
    let mut fx = Fixture::new(
        Table::new(Rect::new(0, 0, 21, H), fixed_columns(), Some(v))
            .with_model(model(&ROWS))
            .with_selection_mode(SelectionMode::Multi)
            .with_h_bar(h),
    );
    fx.with(|t, ctx| t.set_h_offset(12, ctx));
    // Grow 6 wider and 2 taller, as a window resize does.
    fx.t().change_bounds(Rect::new(0, 0, 27, H + 2));
    let deferred = fx.with_deferred(|t, ctx| t.set_state(StateFlag::Active, true, ctx));
    assert_eq!(
        bar_params(&deferred, h),
        (Some(8), Some(22)),
        "overflow 30 − 22"
    );
    assert_eq!(
        bar_value(&deferred, h),
        Some(8),
        "the offset clamps into it"
    );
    // The C++ resize formula (TListViewer::changeBounds): page step = the
    // new body height, arrow step left alone.
    assert_eq!(
        bar_params(&deferred, v).1,
        Some(H + 2 - 1),
        "page step = new body height"
    );
    let arrow = deferred.iter().rev().find_map(|d| match d {
        Deferred::ScrollBarSetParams {
            id,
            page_step: Some(_),
            arrow_step,
            ..
        } if *id == v => Some(*arrow_step),
        _ => None,
    });
    assert_eq!(arrow, Some(None), "the arrow step is kept");
    // The owner's on_bounds_changed publishes right away.
    fx.t().change_bounds(Rect::new(0, 0, 40, H));
    let deferred = fx.with_deferred(|t, ctx| t.on_bounds_changed(ctx));
    assert_eq!(bar_params(&deferred, h).0, Some(0), "everything fits");
}

#[test]
fn header_click_hits_the_scrolled_column() {
    let mut fx = scrolling(21, None);
    fx.with(|t, ctx| t.set_h_offset(5, ctx));
    // Text x 4 is the first scrolled cell: content cell 5 = NAME (ID is
    // 0..4, its separator 4). Screen x = text x + 1.
    fx.click(5, 0);
    assert_eq!(fx.t().sort_spec(), Some((1, SortDir::Asc)));
    // The frozen check cell still toggles the row.
    fx.click(2, 2);
    assert_eq!(keys(fx.t().selected_keys()), ["b2"]);
}

#[test]
fn snapshot_stretched() {
    let mut cols = columns();
    cols[1] = cols[1].clone().flex(2);
    cols[2] = cols[2].clone().flex(1);
    let mut fx =
        Fixture::new(Table::new(Rect::new(0, 0, W + 9, H), cols, None).with_model(model(&ROWS)));
    fx.focus(true);
    insta::assert_snapshot!(fx.render(W as u16 + 9, H as u16));
}

#[test]
fn snapshot_h_scrolled() {
    let mut fx = scrolling(21, None);
    fx.key(Key::Char(' '));
    fx.with(|t, ctx| t.set_h_offset(7, ctx));
    // Header and rows scroll together; the check column stays put.
    insta::assert_snapshot!(fx.render(21, H as u16));
}

#[test]
fn an_h_bar_change_requests_a_scroll_sync_for_the_body() {
    let bar = mint_bar();
    let mut fx = scrolling(21, Some(bar));
    let body = fx.t().body_id;
    let mut ev = Event::Broadcast {
        command: Command::SCROLL_BAR_CHANGED,
        source: Some(bar),
    };
    let deferred = fx.with_deferred(|t, ctx| t.handle_event(&mut ev, ctx));
    assert!(
        deferred.iter().any(|d| matches!(
            d,
            Deferred::ScrollSync { target, h: Some(h), .. } if *target == body && *h == bar
        )),
        "the pump is asked to read the h bar into the body"
    );
}

#[test]
fn hiding_a_column_while_scrolled_clamps_and_republishes() {
    let bar = mint_bar();
    let mut fx = scrolling(21, Some(bar));
    fx.with(|t, ctx| t.set_h_offset(14, ctx));
    assert_eq!(fx.t().h_offset(), 14);
    // Without NAME (10 + separator) the content is 19 in 16: overflow 3.
    fx.t().set_column_hidden(1, true);
    assert_eq!(fx.t().h_offset(), 3, "clamped at once");
    let mut ev = Event::KeyDown(KeyEvent::new(Key::Char('x'), KeyModifiers::default()));
    let deferred = fx.with_deferred(|t, ctx| t.handle_event(&mut ev, ctx));
    assert_eq!(
        bar_params(&deferred, bar).0,
        Some(3),
        "the next event republishes"
    );
    assert_eq!(bar_value(&deferred, bar), Some(3));
}

#[test]
fn snapshot_wide_glyph_at_the_scroll_edge() {
    // 4 + 10 + 8 plus two separators = 24 content cells in 15: overflow 9.
    let cols = vec![
        ColumnSpec::new("ID", 4).min_width(4),
        ColumnSpec::new("NAME", 10).min_width(10),
        ColumnSpec::new("STATE", 8).min_width(8),
    ];
    let rows = vec![
        vec!["a1".to_string(), "日本語テキスト".into(), "running".into()],
        vec!["b2".to_string(), "ascii".into(), "exited".into()],
    ];
    let mut fx = Fixture::new(
        Table::new(Rect::new(0, 0, 16, 4), cols, None).with_model(Box::new(VecModel::new(rows))),
    );
    fx.focus(true);
    // Offset 6 cuts 日 (content cells 5–6) in half: its visible half is a
    // space, and 本 starts under the header's "M" of NAME.
    fx.with(|t, ctx| t.set_h_offset(6, ctx));
    insta::assert_snapshot!(fx.render(16, 4));
}

// -- header interaction: resize, move, show/hide, layout ----------------------

// The fixture header (W = 31): ID at view x 1..5, separator 5; NAME 6..16,
// separator 16; STATE 17..25, separator 25; CPU 26..31.

fn changed(out: &Broadcasts) -> usize {
    out.iter()
        .filter(|(c, _)| *c == Command::TABLE_COLUMNS_CHANGED)
        .count()
}

fn width_of(fx: &mut Fixture, col: usize) -> i32 {
    fx.t().column_layout()[col].width
}

/// The layout width of model column `col` at the table's current size.
fn laid_out(fx: &mut Fixture, col: usize) -> i32 {
    let t = fx.t();
    let size_x = t.state().size.x;
    let d = t.data.borrow();
    let l = body::row_layout(&d, size_x);
    l.widths[d.display_pos(col).unwrap() + l.frozen]
}

#[test]
fn a_header_press_starts_mouse_tracking_on_the_table() {
    let mut fx = Fixture::table();
    let deferred = fx.with_deferred(|t, ctx| t.handle_event(&mut mouse_down(8, 0, false), ctx));
    assert!(
        deferred
            .iter()
            .any(|d| matches!(d, Deferred::PushCapture(_))),
        "a label press is tracked until the release"
    );
    // Nothing happens before the release.
    assert_eq!(fx.t().sort_spec(), None);
}

#[test]
fn dragging_a_separator_resizes_and_broadcasts_on_release() {
    let mut fx = Fixture::table();
    let id = Some(fx.id);
    let out = fx.drag(16, &[18, 19]);
    assert_eq!(width_of(&mut fx, 1), 13, "NAME: 10 + 3");
    assert_eq!(
        laid_out(&mut fx, 1),
        13,
        "a dragged width holds in the layout"
    );
    assert!(fx.t().column_layout()[1].fixed);
    assert_eq!(out, [(Command::TABLE_COLUMNS_CHANGED, id)]);
    assert_eq!(fx.t().sort_spec(), None, "a resize does not sort");
}

#[test]
fn a_separator_press_without_a_move_changes_nothing() {
    let mut fx = Fixture::table();
    let before = fx.t().column_layout();
    let out = fx.drag(16, &[16]);
    assert_eq!(fx.t().column_layout(), before);
    assert!(out.is_empty());
}

#[test]
fn resize_clamps_to_min_width_and_to_the_cap() {
    let mut cols = columns();
    cols[1] = cols[1].clone().min_width(3);
    let mut fx =
        Fixture::new(Table::new(Rect::new(0, 0, W, H), cols, None).with_model(model(&ROWS)));
    // A column without a min_width stops at one cell.
    fx.drag(5, &[-40]);
    assert_eq!(width_of(&mut fx, 0), 1);
    // ID now ends at view x 2, NAME at 13.
    fx.drag(13, &[2]);
    assert_eq!(width_of(&mut fx, 1), 3, "never below min_width");
    // NAME now ends at view x 6.
    fx.drag(6, &[6 + 5000]);
    assert_eq!(width_of(&mut fx, 1), layout::MAX_COLUMN_WIDTH);
}

#[test]
fn resizing_a_flex_column_makes_it_fixed() {
    let mut cols = columns();
    cols[1] = cols[1].clone().flex(1);
    let mut fx =
        Fixture::new(Table::new(Rect::new(0, 0, W + 6, H), cols, None).with_model(model(&ROWS)));
    // NAME is stretched to 16 cells (view x 6..22), its separator at 22.
    assert_eq!(laid_out(&mut fx, 1), 16);
    fx.drag(22, &[19]);
    let s = &fx.t().column_layout()[1];
    assert_eq!((s.width, s.flex, s.fixed), (13, 0, true));
    assert_eq!(laid_out(&mut fx, 1), 13, "the spare width stays empty now");
}

#[test]
fn a_column_that_is_not_resizable_ignores_separator_drags() {
    let mut cols = columns();
    cols[1] = cols[1].clone().resizable(false);
    let mut fx =
        Fixture::new(Table::new(Rect::new(0, 0, W, H), cols, None).with_model(model(&ROWS)));
    let out = fx.drag(16, &[20]);
    assert_eq!(width_of(&mut fx, 1), 10);
    assert!(out.is_empty());
    // Its neighbours still resize.
    fx.drag(5, &[7]);
    assert_eq!(width_of(&mut fx, 0), 6);
}

#[test]
fn the_separator_hit_test_goes_through_the_h_offset() {
    let mut fx = scrolling(21, None);
    fx.with(|t, ctx| t.set_h_offset(5, ctx));
    // Check cell + separator = 4 frozen text cells; NAME's separator is at
    // content x 15 → text x 4 + 15 − 5 = 14 → view x 15.
    fx.drag(15, &[17]);
    assert_eq!(width_of(&mut fx, 1), 12);
    assert_eq!(width_of(&mut fx, 0), 4, "ID (scrolled off) is untouched");
}

#[test]
fn a_resize_republishes_the_h_bar_range() {
    let bar = mint_bar();
    let mut fx = scrolling(21, Some(bar));
    // Check cell + separator take 4 text cells: NAME's separator (content
    // x 15) is at view x 20.
    let mut ev = vec![mouse_down(20, 0, false)];
    ev.push(mouse_at(MouseKind::Move, 23, 0));
    // Content 30 in a view of 16: max offset 14; NAME + 3 → 17.
    let deferred = fx.with_deferred(|t, ctx| {
        for e in &mut ev {
            t.handle_event(e, ctx);
        }
    });
    assert_eq!(bar_params(&deferred, bar).0, Some(17));
}

#[test]
fn dragging_a_label_moves_the_column_on_release() {
    let mut fx = Fixture::table();
    let id = Some(fx.id);
    // Drop past STATE's middle: before CPU.
    let out = fx.drag(2, &[10, 24]);
    let order: Vec<usize> = fx.t().column_layout().iter().map(|s| s.order).collect();
    assert_eq!(order, [2, 0, 1, 3], "ID now sits third");
    assert_eq!(out, [(Command::TABLE_COLUMNS_CHANGED, id)]);
    assert_eq!(fx.t().sort_spec(), None, "a move does not sort");
    // The cells moved with the label.
    insta::assert_snapshot!(fx.render(W as u16, H as u16));
}

#[test]
fn a_move_keeps_sort_and_hidden_state_with_the_column() {
    let mut fx = Fixture::table();
    fx.with(|t, ctx| t.sort_by(0, SortDir::Asc, ctx));
    fx.t().set_column_hidden(2, true);
    // ID 1..5, NAME 6..16, CPU 17..22: drop NAME before ID.
    let out = fx.drag(8, &[1]);
    assert_eq!(changed(&out), 1);
    let order: Vec<usize> = fx.t().column_layout().iter().map(|s| s.order).collect();
    assert_eq!(order, [1, 0, 2, 3]);
    assert_eq!(fx.t().sort_spec(), Some((0, SortDir::Asc)), "still ID");
    assert!(fx.t().is_column_hidden(2), "STATE still hidden");
    insta::assert_snapshot!(fx.render(W as u16, H as u16));
}

#[test]
fn a_drag_shows_the_drop_indicator_until_release() {
    let mut fx = Fixture::table();
    fx.send(mouse_down(2, 0, false));
    fx.send(mouse_at(MouseKind::Move, 24, 0));
    // The gap before CPU: its separator cell, view x 25.
    assert_eq!(fx.t().data.borrow().drop_x, Some(25));
    insta::assert_snapshot!(fx.render(W as u16, H as u16));
    fx.send(mouse_at(MouseKind::Up, 24, 0));
    assert_eq!(fx.t().data.borrow().drop_x, None);
}

#[test]
fn a_small_move_inside_the_label_still_sorts() {
    let mut fx = Fixture::table();
    let out = fx.drag(8, &[10, 14]);
    assert_eq!(fx.t().sort_spec(), Some((1, SortDir::Asc)));
    assert_eq!(out, [(Command::TABLE_SORT_CHANGED, Some(fx.id))]);
    let order: Vec<usize> = fx.t().column_layout().iter().map(|s| s.order).collect();
    assert_eq!(order, [0, 1, 2, 3]);
}

#[test]
fn dropping_a_column_next_to_itself_changes_nothing() {
    let mut fx = Fixture::table();
    // Out of NAME and back between ID and NAME: the gap NAME already fills.
    let out = fx.drag(8, &[20, 6]);
    let order: Vec<usize> = fx.t().column_layout().iter().map(|s| s.order).collect();
    assert_eq!(order, [0, 1, 2, 3]);
    assert!(out.is_empty(), "no move, and a drag is no click: {out:?}");
}

#[test]
fn a_column_that_is_not_movable_only_sorts() {
    let mut cols = columns();
    cols[0] = cols[0].clone().movable(false);
    let mut fx =
        Fixture::new(Table::new(Rect::new(0, 0, W, H), cols, None).with_model(model(&ROWS)));
    // Dragged off the label and released there: neither a move nor a sort.
    let out = fx.drag(2, &[24]);
    let order: Vec<usize> = fx.t().column_layout().iter().map(|s| s.order).collect();
    assert_eq!(order, [0, 1, 2, 3]);
    assert_eq!(fx.t().data.borrow().drop_x, None);
    assert_eq!(fx.t().sort_spec(), None);
    assert!(out.is_empty());
    // Released below the header, even under the label: cancelled too.
    fx.send(mouse_down(2, 0, false));
    fx.send(mouse_at(MouseKind::Up, 2, 3));
    assert_eq!(fx.t().sort_spec(), None);
    // Back on the label before the release: a click, so it sorts.
    fx.drag(2, &[24, 3]);
    assert_eq!(fx.t().sort_spec(), Some((0, SortDir::Asc)));
    // Other columns still move past it.
    fx.drag(8, &[1]);
    let order: Vec<usize> = fx.t().column_layout().iter().map(|s| s.order).collect();
    assert_eq!(order, [1, 0, 2, 3]);
}

#[test]
fn an_untracked_mouse_up_is_ignored() {
    let mut fx = Fixture::table();
    let (ev, out) = fx.send(mouse_at(MouseKind::Up, 8, 0));
    assert!(!ev.is_nothing(), "not the table's release");
    assert!(out.is_empty());
    assert_eq!(fx.t().sort_spec(), None);
}

#[test]
fn sort_next_follows_the_display_order() {
    let mut fx = Fixture::table();
    fx.drag(26, &[1]); // CPU to the front: CPU ID NAME STATE
    fx.send(Event::Command(Command::TABLE_SORT_NEXT));
    assert_eq!(fx.t().sort_spec(), Some((3, SortDir::Asc)), "CPU first");
    fx.send(Event::Command(Command::TABLE_SORT_NEXT));
    assert_eq!(fx.t().sort_spec(), Some((0, SortDir::Asc)), "then ID");
}

/// The menu the last popup the deferred queue opens shows.
fn popup_labels(deferred: &[Deferred]) -> Option<(Vec<String>, Rect)> {
    deferred.iter().rev().find_map(|d| match d {
        Deferred::OpenMenuBox { menu, bounds, .. } => Some((
            menu.items
                .iter()
                .filter_map(|i| match i {
                    crate::menu::MenuItem::Command { name, .. } => Some(name.clone()),
                    _ => None,
                })
                .collect(),
            *bounds,
        )),
        _ => None,
    })
}

fn right_down(x: i32, y: i32) -> Event {
    Event::MouseDown(MouseEvent {
        position: Point::new(x, y),
        buttons: MouseButtons {
            right: true,
            ..Default::default()
        },
        ..Default::default()
    })
}

#[test]
fn right_click_on_the_header_opens_the_column_popup() {
    let mut fx = Fixture::table();
    fx.t().set_column_hidden(2, true);
    fx.drag(18, &[1]); // CPU (view x 17..22 with STATE hidden) first
    fx.render(60, 20); // places the table on a 60 × 20 screen
    let mut ev = right_down(8, 0);
    let deferred = fx.with_deferred(|t, ctx| t.handle_event(&mut ev, ctx));
    assert!(ev.is_nothing());
    let (labels, bounds) = popup_labels(&deferred).expect("a popup opens");
    assert_eq!(labels, ["[X] CPU", "[X] ID", "[X] NAME", "[ ] STATE"]);
    assert_eq!((bounds.a.x, bounds.a.y), (8, 1), "below-right of the click");
    assert_eq!(fx.t().sort_spec(), None, "a right-click does not sort");
}

#[test]
fn the_columns_command_opens_the_popup_at_the_header() {
    let mut fx = Fixture::table();
    fx.render(60, 20);
    let mut ev = Event::Command(Command::TABLE_COLUMNS);
    let deferred = fx.with_deferred(|t, ctx| t.handle_event(&mut ev, ctx));
    assert!(ev.is_nothing());
    let (labels, bounds) = popup_labels(&deferred).expect("a popup opens");
    assert_eq!(labels.len(), 4);
    assert_eq!((bounds.a.x, bounds.a.y), (1, 1));
}

#[test]
fn a_toggle_command_hides_and_shows_and_broadcasts() {
    let mut fx = Fixture::table();
    let id = Some(fx.id);
    let toggle = Table::toggle_column_command(2);
    assert_eq!(toggle, Table::toggle_column_command(2), "stable per column");
    let (ev, out) = fx.send(Event::Command(toggle));
    assert!(ev.is_nothing());
    assert!(fx.t().is_column_hidden(2));
    assert_eq!(out, [(Command::TABLE_COLUMNS_CHANGED, id)]);
    fx.send(Event::Command(toggle));
    assert!(!fx.t().is_column_hidden(2));
    // A toggle for a column the table does not have bubbles.
    let (ev, _) = fx.send(Event::Command(Table::toggle_column_command(9)));
    assert!(!ev.is_nothing());
}

#[test]
fn the_last_visible_column_cannot_be_hidden() {
    let mut fx = Fixture::table();
    for c in [0, 1, 2] {
        fx.t().set_column_hidden(c, true);
    }
    let (ev, out) = fx.send(Event::Command(Table::toggle_column_command(3)));
    assert!(ev.is_nothing(), "the command is the table's");
    assert!(!fx.t().is_column_hidden(3));
    assert!(out.is_empty());
    // Showing one again works, and then CPU may go.
    fx.send(Event::Command(Table::toggle_column_command(1)));
    fx.send(Event::Command(Table::toggle_column_command(3)));
    assert!(fx.t().is_column_hidden(3));
    assert!(!fx.t().is_column_hidden(1));
}

#[test]
fn hiding_from_the_popup_republishes_the_h_bar() {
    let bar = mint_bar();
    let mut fx = scrolling(21, Some(bar));
    // Hiding NAME (10 + separator) shrinks content 30 → 19: max offset 3.
    let mut ev = Event::Command(Table::toggle_column_command(1));
    let deferred = fx.with_deferred(|t, ctx| t.handle_event(&mut ev, ctx));
    assert_eq!(bar_params(&deferred, bar).0, Some(3));
}

#[test]
fn column_layout_round_trips() {
    let mut fx = Fixture::table();
    fx.drag(2, &[24]); // move ID before CPU
    fx.drag(25, &[27]); // resize ID (now third, its separator at 25)
    fx.send(Event::Command(Table::toggle_column_command(3)));
    let saved = fx.t().column_layout();
    let screen = fx.render(W as u16, H as u16);

    let mut other = Fixture::table();
    other.t().set_column_layout(&saved);
    assert_eq!(other.t().column_layout(), saved);
    assert_eq!(other.render(W as u16, H as u16), screen);
}

#[test]
fn set_column_layout_clamps_widths_and_keeps_unnamed_columns() {
    let mut fx = Fixture::table();
    fx.t().set_column_layout(&[
        ColumnState {
            column: 2,
            order: 0,
            width: 5000,
            flex: 0,
            fixed: false,
            hidden: false,
        },
        ColumnState {
            column: 7, // unknown: ignored
            order: 1,
            width: 3,
            flex: 0,
            fixed: false,
            hidden: true,
        },
    ]);
    let l = fx.t().column_layout();
    assert_eq!(l[2].width, layout::MAX_COLUMN_WIDTH);
    let order: Vec<usize> = l.iter().map(|s| s.order).collect();
    assert_eq!(order, [1, 2, 0, 3], "STATE first, the rest in their order");
    assert_eq!(l.len(), 4);
}

#[test]
fn programmatic_layout_changes_do_not_broadcast() {
    let mut fx = Fixture::table();
    let saved = fx.t().column_layout();
    let out = fx.with(|t, _| t.set_column_layout(&saved));
    assert!(out.is_empty());
}

// -- fix round 1 ----------------------------------------------------------------

#[test]
fn a_min_width_past_the_cap_does_not_panic_a_resize() {
    let cols = vec![ColumnSpec::new("Log", 2000).min_width(i32::MAX)];
    let mut fx =
        Fixture::new(Table::new(Rect::new(0, 0, W, H), cols, None).with_model(model(&ROWS)));
    // The column (capped at 1024 cells) overflows; scrolled to its end, its
    // last cell (view x 30) is the resize handle.
    fx.with(|t, ctx| t.set_h_offset(i32::MAX, ctx));
    let out = fx.drag(30, &[25, 20]);
    // The floor is capped too, so the 1024 cells on screen cannot shrink:
    // nothing changes.
    assert_eq!(width_of(&mut fx, 0), 2000);
    assert!(out.is_empty());
}

#[test]
fn a_last_column_that_fills_the_view_is_resizable_on_its_last_cell() {
    let mut cols = columns();
    cols[3] = cols[3].clone().flex(1);
    let mut fx =
        Fixture::new(Table::new(Rect::new(0, 0, W + 6, H), cols, None).with_model(model(&ROWS)));
    // CPU is stretched to 11 cells and ends on the view's last cell, x 36.
    assert_eq!(laid_out(&mut fx, 3), 11);
    let out = fx.drag(W + 5, &[W + 3]);
    let s = &fx.t().column_layout()[3];
    assert_eq!((s.width, s.flex, s.fixed), (9, 0, true));
    assert_eq!(changed(&out), 1);
    assert_eq!(fx.t().sort_spec(), None, "the handle cell does not sort");
}

#[test]
fn set_column_layout_raises_widths_to_min_width() {
    let mut cols = columns();
    cols[1] = cols[1].clone().min_width(3);
    cols[2] = cols[2].clone().min_width(i32::MAX);
    let mut fx =
        Fixture::new(Table::new(Rect::new(0, 0, W, H), cols, None).with_model(model(&ROWS)));
    let mut l = fx.t().column_layout();
    for s in &mut l {
        s.width = 0;
    }
    fx.t().set_column_layout(&l);
    let widths: Vec<i32> = fx.t().column_layout().iter().map(|s| s.width).collect();
    assert_eq!(widths, [1, 3, layout::MAX_COLUMN_WIDTH, 1]);
}

#[test]
fn a_tilde_in_a_label_does_not_become_a_popup_hotkey() {
    let mut cols = columns();
    cols[0].label = "~5~ min".to_string();
    let mut fx =
        Fixture::new(Table::new(Rect::new(0, 0, W, H), cols, None).with_model(model(&ROWS)));
    fx.render(60, 20);
    let mut ev = Event::Command(Command::TABLE_COLUMNS);
    let deferred = fx.with_deferred(|t, ctx| t.handle_event(&mut ev, ctx));
    let (labels, _) = popup_labels(&deferred).expect("a popup opens");
    assert_eq!(labels[0], "[X] 5 min");
}

#[test]
fn the_fixed_builder_sets_the_flag() {
    assert!(ColumnSpec::new("A", 4).fixed(true).fixed);
    assert!(!ColumnSpec::new("A", 4).fixed);
}

// -- cell editing ----------------------------------------------------------------

/// The fixture columns with NAME and CPU editable; CPU takes digits and dots.
fn edit_columns() -> Vec<ColumnSpec> {
    let mut cols = columns();
    cols[1] = cols[1].clone().editable(true);
    cols[3] = cols[3].clone().editable(true).validator(Box::new(|| {
        Box::new(crate::validate::FilterValidator::new("0123456789."))
    }));
    cols
}

/// A focused single-mode table over [`edit_columns`]. Text x: ID 1..5,
/// NAME 6..16, STATE 17..25, CPU 26..31; row `i` at y `i + 1`.
fn editing() -> Fixture {
    let mut fx = Fixture::new(
        Table::new(Rect::new(0, 0, W, H), edit_columns(), None).with_model(model(&ROWS)),
    );
    fx.focus(true);
    fx
}

fn shift_tab() -> Event {
    Event::KeyDown(KeyEvent::new(
        Key::Tab,
        KeyModifiers {
            shift: true,
            ..Default::default()
        },
    ))
}

impl Fixture {
    /// The open editor's text, `None` without one.
    fn editor_text(&mut self) -> Option<String> {
        self.t().editor_mut().map(|e| e.data.clone())
    }

    /// The open editor's (row key, model column, bounds).
    fn edit_at(&mut self) -> Option<(String, usize, Rect)> {
        let t = self.t();
        let (key, col) = t.edit.as_ref().map(|e| (e.key.0.clone(), e.col))?;
        let bounds = t.editor_mut()?.state().get_bounds();
        Some((key, col, bounds))
    }

    fn cell(&mut self, row: usize, col: usize) -> String {
        self.t().model().cell(row, col).into_owned()
    }

    fn type_text(&mut self, s: &str) {
        for c in s.chars() {
            self.key(Key::Char(c));
        }
    }

    /// Deliver `ev`; return whether it was used and the deferred effects.
    fn send_deferred(&mut self, ev: Event) -> (bool, Vec<Deferred>) {
        let mut ev = ev;
        let deferred = self.with_deferred(|t, ctx| t.handle_event(&mut ev, ctx));
        (ev.is_nothing(), deferred)
    }
}

fn edited(out: &Broadcasts, id: ViewId) -> usize {
    out.iter()
        .filter(|&&(c, s)| c == Command::TABLE_CELL_EDITED && s == Some(id))
        .count()
}

fn message_box_text(deferred: &[Deferred]) -> Option<String> {
    deferred.iter().find_map(|d| match d {
        Deferred::OpenMessageBox { text, .. } => Some(text.clone()),
        _ => None,
    })
}

/// A model that keeps the default, read-only `set_cell`.
struct ReadOnly(VecModel);

impl TableModel for ReadOnly {
    fn row_count(&self) -> usize {
        self.0.row_count()
    }
    fn cell(&self, row: usize, col: usize) -> std::borrow::Cow<'_, str> {
        self.0.cell(row, col)
    }
    fn row_key(&self, row: usize) -> RowKey {
        self.0.row_key(row)
    }
}

#[test]
fn no_cell_cursor_without_editable_columns() {
    let mut fx = Fixture::table();
    fx.focus(true);
    assert_eq!(fx.t().cursor_column(), None);
    let (ev, _) = fx.send(Event::KeyDown(KeyEvent::new(
        Key::F(2),
        KeyModifiers::default(),
    )));
    assert!(!ev.is_nothing(), "F2 bubbles");
    assert!(!fx.t().is_editing());
    fx.send(mouse_down(7, 1, true));
    assert!(!fx.t().is_editing(), "a double-click does not edit");
}

#[test]
fn arrows_move_the_cell_cursor_between_editable_columns() {
    let mut fx = editing();
    assert_eq!(fx.t().cursor_column(), Some(1), "starts on the first");
    let right = || Event::KeyDown(KeyEvent::new(Key::Right, KeyModifiers::default()));
    let left = || Event::KeyDown(KeyEvent::new(Key::Left, KeyModifiers::default()));
    assert!(fx.send(right()).0.is_nothing());
    assert_eq!(fx.t().cursor_column(), Some(3), "STATE is skipped");
    assert!(
        !fx.send(right()).0.is_nothing(),
        "past the last stop the key bubbles (nothing to scroll)"
    );
    assert_eq!(fx.t().cursor_column(), Some(3));
    assert!(fx.send(left()).0.is_nothing());
    assert_eq!(fx.t().cursor_column(), Some(1));
    assert!(!fx.send(left()).0.is_nothing());
}

#[test]
fn the_cell_cursor_skips_hidden_columns_and_follows_the_display_order() {
    let mut fx = editing();
    fx.t().set_column_hidden(3, true);
    fx.key(Key::Right);
    assert_eq!(fx.t().cursor_column(), Some(1), "CPU is hidden");
    fx.t().set_column_hidden(3, false);
    fx.key(Key::Right);
    assert_eq!(fx.t().cursor_column(), Some(3));
    fx.t().set_column_hidden(3, true);
    assert_eq!(
        fx.t().cursor_column(),
        Some(1),
        "a hidden cursor column hands the cursor to the first stop"
    );
    fx.t().set_column_hidden(3, false);
    // CPU first, then the rest.
    let mut layout = fx.t().column_layout();
    for s in &mut layout {
        s.order = if s.column == 3 { 0 } else { s.column + 1 };
    }
    fx.t().set_column_layout(&layout);
    fx.key(Key::Left);
    assert_eq!(fx.t().cursor_column(), Some(3), "CPU leads now");
}

#[test]
fn moving_the_cell_cursor_scrolls_it_into_view() {
    let bar = mint_bar();
    let mut cols = fixed_columns();
    cols[1] = cols[1].clone().editable(true);
    cols[3] = cols[3].clone().editable(true);
    let mut fx = Fixture::new(
        Table::new(Rect::new(0, 0, 21, H), cols, None)
            .with_model(model(&ROWS))
            .with_h_bar(bar),
    );
    fx.focus(true);
    // 20 text cells over 30 content cells: CPU (25..30) is off screen.
    let mut ev = Event::KeyDown(KeyEvent::new(Key::Right, KeyModifiers::default()));
    let deferred = fx.with_deferred(|t, ctx| t.handle_event(&mut ev, ctx));
    assert_eq!(fx.t().cursor_column(), Some(3));
    assert_eq!(fx.t().h_offset(), 10, "CPU's end lines up with the view's");
    assert_eq!(bar_value(&deferred, bar), Some(10), "the bar follows");
    fx.key(Key::Left);
    assert_eq!(fx.t().cursor_column(), Some(1));
    assert_eq!(fx.t().h_offset(), 5, "NAME (5..15) scrolled back in");
}

#[test]
fn f2_opens_the_editor_over_the_cell_all_selected() {
    let mut fx = editing();
    let (ev, _) = fx.send(Event::KeyDown(KeyEvent::new(
        Key::F(2),
        KeyModifiers::default(),
    )));
    assert!(ev.is_nothing());
    assert_eq!(
        fx.edit_at(),
        Some(("a1".into(), 1, Rect::new(5, 1, 17, 2))),
        "over NAME plus the separator cell either side"
    );
    let t = fx.t();
    let editor = t.edit.as_ref().unwrap().editor;
    assert_eq!(t.group.current(), Some(editor), "the editor is current");
    let e = t.editor_mut().unwrap();
    assert_eq!(e.data, "web");
    assert_eq!((e.sel_start, e.sel_end), (0, 3), "all selected");
    assert_eq!(e.first_pos, 0, "the text is not scrolled");
    assert!(e.state().state.focused);
}

#[test]
fn typing_and_enter_commit_through_set_cell_and_broadcast() {
    let mut fx = editing();
    let id = fx.id;
    fx.key(Key::F(2));
    fx.type_text("api");
    assert_eq!(fx.editor_text().as_deref(), Some("api"), "typing replaced");
    let (ev, out) = fx.send(Event::KeyDown(KeyEvent::new(
        Key::Enter,
        KeyModifiers::default(),
    )));
    assert!(ev.is_nothing(), "Enter is used, not the dialog's default");
    assert!(!fx.t().is_editing());
    assert_eq!(fx.cell(0, 1), "api");
    assert_eq!(edited(&out, id), 1);
    let t = fx.t();
    assert_eq!(t.group.current(), Some(t.body_id), "the rows are current");
    assert!(t.body_mut().lv().state.state.focused);
}

fn enter() -> Event {
    Event::KeyDown(KeyEvent::new(Key::Enter, KeyModifiers::default()))
}

#[test]
fn enter_opens_the_editor_on_the_cursor_cell() {
    let mut fx = editing();
    let (ev, out) = fx.send(enter());
    assert!(ev.is_nothing(), "Enter is used");
    assert_eq!(fx.edit_at().map(|(k, c, _)| (k, c)), Some(("a1".into(), 1)));
    assert_eq!(fx.editor_text().as_deref(), Some("web"));
    assert_eq!(edited(&out, fx.id), 0);
    fx.key(Key::Esc);
    fx.key(Key::Right);
    fx.send(enter());
    assert_eq!(
        fx.edit_at().map(|(k, c, _)| (k, c)),
        Some(("a1".into(), 3)),
        "on the cursor's column"
    );
}

#[test]
fn enter_with_a_modifier_does_not_edit() {
    let mut fx = editing();
    for m in [
        KeyModifiers {
            ctrl: true,
            ..Default::default()
        },
        KeyModifiers {
            alt: true,
            ..Default::default()
        },
        KeyModifiers {
            shift: true,
            ..Default::default()
        },
    ] {
        fx.send(Event::KeyDown(KeyEvent::new(Key::Enter, m)));
        assert!(!fx.t().is_editing(), "{m:?}");
    }
}

#[test]
fn enter_keeps_its_list_meaning_without_a_cell_cursor() {
    // No editable column: Enter is not the table's, it reaches the dialog.
    let mut fx = Fixture::table();
    fx.focus(true);
    let (ev, out) = fx.send(enter());
    assert!(!ev.is_nothing(), "Enter bubbles");
    assert!(!fx.t().is_editing());
    assert!(out.is_empty());
    // Editable columns, all hidden: no cursor either.
    let mut fx = editing();
    fx.t().set_column_hidden(1, true);
    fx.t().set_column_hidden(3, true);
    assert_eq!(fx.t().cursor_column(), None);
    let (ev, _) = fx.send(enter());
    assert!(!ev.is_nothing(), "Enter bubbles");
    assert!(!fx.t().is_editing());
}

#[test]
fn enter_without_rows_keeps_its_list_meaning() {
    let mut fx = editing();
    fx.set_model(&[]);
    let (ev, _) = fx.send(enter());
    assert!(!ev.is_nothing(), "no row to edit: Enter bubbles");
    assert!(!fx.t().is_editing());
}

#[test]
fn an_enter_commit_stays_on_the_cell_and_the_next_enter_edits_it_again() {
    let mut fx = editing();
    let id = fx.id;
    fx.key(Key::Right);
    fx.send(enter());
    fx.type_text("7");
    let (ev, out) = fx.send(enter());
    assert!(ev.is_nothing(), "the commit's Enter is used, once");
    assert!(!fx.t().is_editing(), "and does not open the editor again");
    assert_eq!(fx.cell(0, 3), "7");
    assert_eq!(edited(&out, id), 1);
    assert_eq!(
        fx.t().focused_key(),
        Some(RowKey::from("a1")),
        "no move down"
    );
    assert_eq!(fx.t().cursor_column(), Some(3), "the cursor stays");
    let (ev, _) = fx.send(enter());
    assert!(ev.is_nothing());
    assert_eq!(
        fx.edit_at().map(|(k, c, _)| (k, c)),
        Some(("a1".into(), 3)),
        "the same cell again"
    );
    assert_eq!(fx.editor_text().as_deref(), Some("7"));
}

#[test]
fn enter_edits_in_multi_mode_and_space_still_toggles() {
    let mut fx = Fixture::new(
        Table::new(Rect::new(0, 0, W + 4, H), edit_columns(), None)
            .with_model(model(&ROWS))
            .with_selection_mode(SelectionMode::Multi),
    );
    fx.focus(true);
    fx.key(Key::Char(' '));
    assert_eq!(keys(fx.t().selected_keys()), ["a1"], "Space marks");
    let (ev, _) = fx.send(enter());
    assert!(ev.is_nothing());
    assert_eq!(fx.edit_at().map(|(k, c, _)| (k, c)), Some(("a1".into(), 1)));
    fx.key(Key::Esc);
    assert_eq!(keys(fx.t().selected_keys()), ["a1"], "Enter marks nothing");
    // A double-click on ID (not editable; text x 5..9 behind the check
    // column) still toggles the row.
    fx.send(mouse_down(6, 2, true));
    assert_eq!(keys(fx.t().selected_keys()), ["a1", "b2"]);
    assert!(!fx.t().is_editing());
}

#[test]
fn enter_after_a_dropped_edit_opens_the_new_rows_editor() {
    let mut fx = editing();
    fx.key(Key::F(2));
    fx.type_text("zz");
    fx.t().set_value(FieldValue::Text("c3".into()));
    let (ev, _) = fx.send(enter());
    assert!(ev.is_nothing());
    assert_eq!(
        fx.edit_at().map(|(k, c, _)| (k, c)),
        Some(("c3".into(), 1)),
        "the dropped edit closed, the Enter went on"
    );
    assert_eq!(fx.editor_text().as_deref(), Some("cache"));
    assert_eq!(fx.cell(0, 1), "web", "the dropped text is not stored");
}

#[test]
fn esc_cancels_and_is_used() {
    let mut fx = editing();
    fx.key(Key::F(2));
    fx.type_text("zzz");
    let (ev, out) = fx.send(Event::KeyDown(KeyEvent::new(
        Key::Esc,
        KeyModifiers::default(),
    )));
    assert!(ev.is_nothing(), "Esc does not reach the dialog");
    assert!(!fx.t().is_editing());
    assert_eq!(fx.cell(0, 1), "web");
    assert_eq!(edited(&out, fx.id), 0);
}

#[test]
fn losing_focus_cancels() {
    let mut fx = editing();
    fx.key(Key::F(2));
    fx.type_text("zzz");
    fx.focus(false);
    assert!(!fx.t().is_editing());
    assert_eq!(fx.cell(0, 1), "web");
}

#[test]
fn a_click_outside_the_editor_cancels_and_acts() {
    let mut fx = editing();
    fx.key(Key::F(2));
    fx.click(2, 3);
    assert!(!fx.t().is_editing());
    assert_eq!(
        fx.t().focused_key(),
        Some(RowKey::from("c3")),
        "the click focused"
    );
    fx.key(Key::F(2));
    let (_, _, r) = fx.edit_at().unwrap();
    fx.click(r.a.x + 2, r.a.y);
    assert!(fx.t().is_editing(), "a click inside stays in the editor");
}

#[test]
fn double_click_edits_an_editable_cell() {
    let mut fx = editing();
    let (ev, out) = fx.send(mouse_down(27, 2, true));
    assert!(ev.is_nothing());
    assert_eq!(fx.edit_at().map(|(k, c, _)| (k, c)), Some(("b2".into(), 3)));
    assert_eq!(fx.t().cursor_column(), Some(3));
    assert!(
        !out.iter().any(|(c, _)| *c == Command::LIST_ITEM_SELECTED),
        "not a list choice"
    );
}

#[test]
fn double_click_on_a_read_only_cell_keeps_the_list_meaning() {
    let mut fx = editing();
    let (_, out) = fx.send(mouse_down(19, 2, true));
    assert!(!fx.t().is_editing());
    assert!(out.iter().any(|(c, _)| *c == Command::LIST_ITEM_SELECTED));
}

#[test]
fn multi_mode_double_click_toggles_on_read_only_and_edits_editable_cells() {
    // 39 text cells: check 1..4, ID 5..9, NAME 10..20, STATE 21..29, CPU 30..35.
    let mut fx = Fixture::new(
        Table::new(Rect::new(0, 0, 40, H), edit_columns(), None)
            .with_model(model(&ROWS))
            .with_selection_mode(SelectionMode::Multi),
    );
    fx.focus(true);
    let out = fx.send(mouse_down(23, 1, true)).1;
    assert!(!fx.t().is_editing());
    assert_eq!(keys(fx.t().selected_keys()), ["a1"]);
    assert!(
        out.iter()
            .any(|(c, _)| *c == Command::TABLE_SELECTION_CHANGED)
    );
    let out = fx.send(mouse_down(12, 2, true)).1;
    assert_eq!(fx.edit_at(), Some(("b2".into(), 1, Rect::new(9, 2, 21, 3))));
    assert!(
        out.iter()
            .all(|(c, _)| *c != Command::TABLE_SELECTION_CHANGED)
    );
    assert_eq!(keys(fx.t().selected_keys()), ["a1"], "no toggle");
}

#[test]
fn the_validator_filters_typing_and_rejects_on_commit() {
    let mut fx = editing();
    fx.key(Key::Right);
    fx.key(Key::F(2));
    fx.type_text("4x2");
    assert_eq!(fx.editor_text().as_deref(), Some("42"), "x was filtered");
    fx.t().editor_mut().unwrap().data = "4x2".into();
    let (used, deferred) = fx.send_deferred(Event::KeyDown(KeyEvent::new(
        Key::Enter,
        KeyModifiers::default(),
    )));
    assert!(used);
    assert!(fx.t().is_editing(), "the editor stays");
    assert!(message_box_text(&deferred).is_some(), "the validator's box");
    assert_eq!(fx.cell(0, 3), "12.5");
}

#[test]
fn a_refused_set_cell_shows_its_message_and_keeps_the_editor() {
    let rows = ROWS
        .iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect();
    let mut fx = Fixture::new(
        Table::new(Rect::new(0, 0, W, H), edit_columns(), None)
            .with_model(Box::new(ReadOnly(VecModel::new(rows)))),
    );
    fx.focus(true);
    fx.key(Key::F(2));
    fx.type_text("new");
    let (used, deferred) = fx.send_deferred(Event::KeyDown(KeyEvent::new(
        Key::Enter,
        KeyModifiers::default(),
    )));
    assert!(used);
    assert_eq!(message_box_text(&deferred).as_deref(), Some("read-only"));
    assert!(fx.t().is_editing());
    assert_eq!(fx.editor_text().as_deref(), Some("new"), "the text is kept");
    // The box takes the focus: that does not cancel.
    fx.focus(false);
    assert!(
        fx.t().is_editing(),
        "the error box's focus grab keeps the edit"
    );
    fx.focus(true);
    fx.focus(false);
    assert!(!fx.t().is_editing(), "a later focus loss cancels again");
}

#[test]
fn an_unchanged_text_closes_without_storing() {
    let rows = ROWS
        .iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect();
    let mut fx = Fixture::new(
        Table::new(Rect::new(0, 0, W, H), edit_columns(), None)
            .with_model(Box::new(ReadOnly(VecModel::new(rows)))),
    );
    fx.focus(true);
    fx.key(Key::F(2));
    let (used, deferred) = fx.send_deferred(Event::KeyDown(KeyEvent::new(
        Key::Enter,
        KeyModifiers::default(),
    )));
    assert!(used);
    assert!(!fx.t().is_editing());
    assert_eq!(message_box_text(&deferred), None, "set_cell was not asked");
}

#[test]
fn tab_and_shift_tab_commit_and_move_along_the_row() {
    let mut fx = editing();
    let id = fx.id;
    fx.key(Key::F(2));
    fx.type_text("api");
    let (ev, out) = fx.send(Event::KeyDown(KeyEvent::new(
        Key::Tab,
        KeyModifiers::default(),
    )));
    assert!(ev.is_nothing(), "Tab does not leave the table");
    assert_eq!(fx.cell(0, 1), "api");
    assert_eq!(edited(&out, id), 1);
    assert_eq!(fx.edit_at().map(|(k, c, _)| (k, c)), Some(("a1".into(), 3)));
    assert_eq!(fx.editor_text().as_deref(), Some("12.5"));
    fx.type_text("7");
    let (ev, _) = fx.send(shift_tab());
    assert!(ev.is_nothing());
    assert_eq!(fx.cell(0, 3), "7");
    assert_eq!(fx.edit_at().map(|(k, c, _)| (k, c)), Some(("a1".into(), 1)));
    fx.type_text("app");
    let (ev, _) = fx.send(shift_tab());
    assert!(
        !ev.is_nothing(),
        "nothing before the first stop: the Tab goes on and moves the focus"
    );
    assert_eq!(fx.cell(0, 1), "app", "stored first");
    assert!(!fx.t().is_editing());
    assert_eq!(fx.t().cursor_column(), Some(1));
    fx.key(Key::Right);
    fx.key(Key::F(2));
    let (ev, _) = fx.send(Event::KeyDown(KeyEvent::new(
        Key::Tab,
        KeyModifiers::default(),
    )));
    assert!(
        !ev.is_nothing(),
        "nothing after the last stop: the Tab goes on"
    );
    assert!(!fx.t().is_editing());
    assert_eq!(fx.t().cursor_column(), Some(3));
}

#[test]
fn a_commit_resorts_and_focus_follows_the_row() {
    let mut fx = editing();
    fx.with(|t, ctx| t.sort_by(1, SortDir::Asc, ctx));
    // cache, db, web, worker-long-name.
    assert_eq!(fx.t().row_order(), [2, 1, 0, 3]);
    fx.with(|t, ctx| {
        t.set_focused_key(&RowKey::from("a1"), ctx);
    });
    fx.key(Key::F(2));
    fx.type_text("aaa");
    fx.key(Key::Enter);
    assert_eq!(fx.t().row_order(), [0, 2, 1, 3], "a1 sorts first now");
    assert_eq!(fx.t().focused_key(), Some(RowKey::from("a1")));
    assert_eq!(fx.t().focused_row(), Some(0));
}

#[test]
fn the_editor_follows_its_cell_through_a_resort() {
    let mut fx = editing();
    fx.key(Key::F(2));
    fx.with(|t, ctx| t.sort_by(1, SortDir::Desc, ctx));
    // worker-long-name, web, db, cache: a1 is on display row 1.
    fx.render(W as u16, H as u16);
    assert_eq!(fx.edit_at(), Some(("a1".into(), 1, Rect::new(5, 2, 17, 3))));
}

#[test]
fn the_editor_follows_an_h_scroll_and_a_frozen_check_column() {
    let mut cols = fixed_columns();
    cols[3] = cols[3].clone().editable(true);
    let mut fx = Fixture::new(
        Table::new(Rect::new(0, 0, 24, H), cols, None)
            .with_model(model(&ROWS))
            .with_selection_mode(SelectionMode::Multi),
    );
    fx.focus(true);
    // 23 text cells: check 1..4 + separator, 19 scrolling cells from x 5.
    fx.key(Key::F(2));
    let off = fx.t().h_offset();
    assert_eq!(off, 11, "CPU (25..30) revealed");
    assert_eq!(fx.edit_at().map(|e| e.2), Some(Rect::new(18, 1, 24, 2)));
    fx.with(|t, ctx| t.set_h_offset(9, ctx));
    fx.render(24, H as u16);
    assert_eq!(
        fx.edit_at().map(|e| e.2),
        Some(Rect::new(20, 1, 24, 2)),
        "moved right with the scroll, clipped at the view"
    );
    fx.with(|t, ctx| t.set_h_offset(0, ctx));
    fx.render(24, H as u16);
    assert!(
        !fx.t().editor_mut().unwrap().state().state.visible,
        "scrolled out: hidden"
    );
    let (ev, _) = fx.send(Event::KeyDown(KeyEvent::new(
        Key::Char('7'),
        KeyModifiers::default(),
    )));
    assert!(ev.is_nothing(), "the key went to the editor");
    assert!(fx.t().is_editing(), "the edit outlives the scroll");
    assert_eq!(fx.t().h_offset(), 11, "the key scrolled the cell back");
    assert!(fx.t().editor_mut().unwrap().state().state.visible);
    assert_eq!(fx.editor_text().as_deref(), Some("7"));
}

#[test]
fn the_editor_never_covers_the_check_column() {
    let mut cols = fixed_columns();
    cols[1] = cols[1].clone().editable(true);
    let mut fx = Fixture::new(
        Table::new(Rect::new(0, 0, 24, H), cols, None)
            .with_model(model(&ROWS))
            .with_selection_mode(SelectionMode::Multi),
    );
    fx.focus(true);
    fx.key(Key::F(2));
    fx.with(|t, ctx| t.set_h_offset(7, ctx));
    fx.render(24, H as u16);
    // NAME (5..15) from offset 7: its cells 7..15 at x 5..13.
    assert_eq!(fx.edit_at().map(|e| e.2), Some(Rect::new(4, 1, 14, 2)));
}

#[test]
fn a_header_press_cancels_the_edit() {
    let mut fx = editing();
    fx.key(Key::F(2));
    fx.type_text("zzz");
    fx.drag(16, &[18]);
    assert!(!fx.t().is_editing());
    assert_eq!(fx.cell(0, 1), "web");
}

#[test]
fn hiding_the_edited_column_cancels_on_the_next_event() {
    let mut fx = editing();
    fx.key(Key::F(2));
    let toggle = Table::toggle_column_command(1);
    fx.send(Event::Command(toggle));
    fx.render(W as u16, H as u16);
    assert!(!fx.t().editor_mut().unwrap().state().state.visible);
    fx.key(Key::Char('q'));
    assert!(!fx.t().is_editing());
}

#[test]
fn a_resize_keeps_the_editor_on_its_cell() {
    let mut cols = edit_columns();
    cols[1] = cols[1].clone().flex(1);
    let mut fx =
        Fixture::new(Table::new(Rect::new(0, 0, W, H), cols, None).with_model(model(&ROWS)));
    fx.focus(true);
    fx.key(Key::F(2));
    fx.t().change_bounds(Rect::new(0, 0, W + 6, H));
    assert_eq!(
        fx.edit_at().map(|e| e.2),
        Some(Rect::new(5, 1, 23, 2)),
        "NAME stretched to 16"
    );
    // Shrink so far the row is off screen: it scrolls back in.
    fx.with(|t, ctx| {
        t.set_focused_key(&RowKey::from("d4"), ctx);
    });
    fx.with(|t, ctx| t.cancel_edit(ctx));
    fx.key(Key::F(2));
    fx.t().change_bounds(Rect::new(0, 0, W, 2));
    let (_, _, r) = fx.edit_at().unwrap();
    assert_eq!(r.a.y, 1, "the only row on screen");
    assert!(fx.t().editor_mut().unwrap().state().state.visible);
}

#[test]
fn a_reload_keeps_the_edit_while_its_row_exists() {
    let mut fx = editing();
    fx.key(Key::F(2));
    fx.type_text("api");
    let mut rows = ROWS;
    rows[0][1] = "nginx";
    fx.set_model(&rows);
    assert!(fx.t().is_editing(), "the row is still there");
    assert_eq!(fx.editor_text().as_deref(), Some("api"), "typed text kept");
    fx.key(Key::Enter);
    assert_eq!(fx.cell(0, 1), "api", "last writer wins");
    fx.key(Key::F(2));
    fx.set_model(&ROWS[1..]);
    assert!(!fx.t().is_editing(), "a1 left the model");
}

#[test]
fn snapshot_cell_cursor() {
    let mut fx = editing();
    fx.key(Key::Down);
    fx.key(Key::Right);
    insta::assert_snapshot!(fx.render(W as u16, H as u16));
}

#[test]
fn snapshot_cell_editor() {
    let mut fx = editing();
    fx.key(Key::Down);
    fx.key(Key::F(2));
    fx.type_text("postgres");
    insta::assert_snapshot!(fx.render(W as u16, H as u16));
}

#[test]
fn enter_and_esc_on_an_off_screen_editor_stay_with_the_editor() {
    // Vertical: the wheel (here: top_item) scrolls the edited row away.
    let mut fx = editing();
    fx.key(Key::F(2));
    fx.type_text("zz");
    fx.t().body_mut().lv_state().top_item = 2;
    fx.render(W as u16, H as u16);
    assert!(!fx.t().editor_mut().unwrap().state().state.visible);
    let (ev, out) = fx.send(Event::KeyDown(KeyEvent::new(
        Key::Enter,
        KeyModifiers::default(),
    )));
    assert!(ev.is_nothing(), "Enter does not reach the dialog");
    assert_eq!(fx.cell(0, 1), "zz", "and it stored the text");
    assert_eq!(edited(&out, fx.id), 1);
    assert_eq!(fx.t().body_mut().lv().top_item, 0, "the row is back");
    fx.key(Key::F(2));
    fx.t().body_mut().lv_state().top_item = 2;
    fx.render(W as u16, H as u16);
    let (ev, _) = fx.send(Event::KeyDown(KeyEvent::new(
        Key::Esc,
        KeyModifiers::default(),
    )));
    assert!(ev.is_nothing(), "Esc does not close the dialog");
    assert!(!fx.t().is_editing());
}

#[test]
fn a_key_that_finds_the_cell_gone_is_used_up() {
    let mut fx = editing();
    fx.key(Key::F(2));
    fx.t().set_column_hidden(1, true);
    let (ev, _) = fx.send(Event::KeyDown(KeyEvent::new(
        Key::Enter,
        KeyModifiers::default(),
    )));
    assert!(ev.is_nothing(), "not the dialog's default button");
    assert!(!fx.t().is_editing());
}

#[test]
fn valid_ok_commits_and_cancel_cancels() {
    let mut fx = editing();
    let id = fx.id;
    fx.key(Key::F(2));
    fx.type_text("api");
    let mut ok = false;
    let out = fx.with(|t, ctx| ok = t.valid(Command::OK, ctx));
    assert!(ok);
    assert_eq!(fx.cell(0, 1), "api", "OK stored the open edit");
    assert_eq!(edited(&out, id), 1);
    assert!(!fx.t().is_editing());
    fx.key(Key::F(2));
    fx.type_text("zzz");
    let mut valid = false;
    fx.with(|t, ctx| valid = t.valid(Command::VALID, ctx));
    assert!(valid && fx.t().is_editing(), "the status check leaves it");
    fx.with(|t, ctx| ok = t.valid(Command::CANCEL, ctx));
    assert!(ok);
    assert!(!fx.t().is_editing());
    assert_eq!(fx.cell(0, 1), "api");
}

#[test]
fn valid_ok_with_a_refused_edit_vetoes_and_keeps_the_editor() {
    let rows = ROWS
        .iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect();
    let mut fx = Fixture::new(
        Table::new(Rect::new(0, 0, W, H), edit_columns(), None)
            .with_model(Box::new(ReadOnly(VecModel::new(rows)))),
    );
    fx.focus(true);
    fx.key(Key::F(2));
    fx.type_text("new");
    let mut ok = true;
    let deferred = fx.with_deferred(|t, ctx| ok = t.valid(Command::OK, ctx));
    assert!(!ok, "the dialog stays open");
    assert_eq!(message_box_text(&deferred).as_deref(), Some("read-only"));
    assert!(fx.t().is_editing());
}

/// Refuses everything and, like the base validator, shows no box.
struct Silent;
impl crate::validate::Validator for Silent {
    fn is_valid(&self, _s: &str) -> bool {
        false
    }
}

#[test]
fn a_refusal_without_a_box_does_not_block_the_focus_loss_cancel() {
    let mut cols = edit_columns();
    cols[1] = cols[1].clone().validator(Box::new(|| Box::new(Silent)));
    let mut fx =
        Fixture::new(Table::new(Rect::new(0, 0, W, H), cols, None).with_model(model(&ROWS)));
    fx.focus(true);
    fx.key(Key::F(2));
    fx.key(Key::Enter);
    assert!(fx.t().is_editing(), "refused");
    fx.focus(false);
    assert!(!fx.t().is_editing(), "no box took the focus: cancel");
}

/// Rows keyed by their NAME cell, so editing NAME re-keys the row.
struct KeyedByName(VecModel);
impl TableModel for KeyedByName {
    fn row_count(&self) -> usize {
        self.0.row_count()
    }
    fn cell(&self, row: usize, col: usize) -> std::borrow::Cow<'_, str> {
        self.0.cell(row, col)
    }
    fn row_key(&self, row: usize) -> RowKey {
        RowKey(self.0.cell(row, 1).into_owned())
    }
    fn set_cell(&mut self, row: usize, col: usize, text: &str) -> Result<(), String> {
        self.0.set_cell(row, col, text)
    }
}

#[test]
fn a_re_keyed_row_keeps_its_mark() {
    let rows = ROWS
        .iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect();
    let mut fx = Fixture::new(
        Table::new(Rect::new(0, 0, 40, H), edit_columns(), None)
            .with_model(Box::new(KeyedByName(VecModel::new(rows))))
            .with_selection_mode(SelectionMode::Multi),
    );
    fx.focus(true);
    fx.key(Key::Char(' '));
    assert_eq!(keys(fx.t().selected_keys()), ["web"]);
    fx.key(Key::F(2));
    fx.type_text("api");
    fx.key(Key::Enter);
    assert_eq!(
        keys(fx.t().selected_keys()),
        ["api"],
        "the mark moved along"
    );
    assert_eq!(fx.t().focused_key(), Some(RowKey::from("api")));
}

#[test]
fn moving_the_focus_to_another_row_ends_the_edit() {
    let mut fx = editing();
    fx.key(Key::F(2));
    fx.with(|t, ctx| {
        t.set_focused_key(&RowKey::from("a1"), ctx);
    });
    assert!(fx.t().is_editing(), "the same row keeps it");
    fx.with(|t, ctx| {
        t.set_focused_key(&RowKey::from("c3"), ctx);
    });
    assert!(!fx.t().is_editing());
    // Without a context: hidden at once, closed by the next event, which goes on.
    fx.key(Key::F(2));
    fx.t().set_value(FieldValue::Text("a1".into()));
    fx.render(W as u16, H as u16);
    assert!(!fx.t().editor_mut().unwrap().state().state.visible);
    let (ev, _) = fx.send(Event::KeyDown(KeyEvent::new(
        Key::Down,
        KeyModifiers::default(),
    )));
    assert!(ev.is_nothing(), "Down moved the rows");
    assert!(!fx.t().is_editing());
    assert_eq!(fx.t().focused_key(), Some(RowKey::from("b2")));
}

#[test]
fn a_left_click_on_an_editable_cell_moves_the_cell_cursor() {
    let mut fx = editing();
    fx.click(27, 2);
    assert_eq!(fx.t().cursor_column(), Some(3));
    fx.click(19, 2);
    assert_eq!(
        fx.t().cursor_column(),
        Some(3),
        "a read-only cell leaves it"
    );
    let right = Event::MouseDown(MouseEvent {
        position: Point::new(8, 2),
        buttons: MouseButtons {
            right: true,
            ..Default::default()
        },
        ..Default::default()
    });
    fx.send(right);
    assert_eq!(fx.t().cursor_column(), Some(3), "a right press leaves it");
    fx.click(8, 5);
    assert_eq!(fx.t().cursor_column(), Some(3), "below the last row");
}

#[test]
fn shift_arrows_leave_the_cell_cursor_and_bubble() {
    let mut fx = editing();
    let (ev, _) = fx.send(Event::KeyDown(KeyEvent::new(
        Key::Right,
        KeyModifiers {
            shift: true,
            ..Default::default()
        },
    )));
    assert!(!ev.is_nothing());
    assert_eq!(fx.t().cursor_column(), Some(1));
}

#[test]
fn validator_factory_is_exported_from_the_crate_root() {
    let f = crate::ValidatorFactory::new(Box::new(|| Box::new(Silent)));
    assert!(!f.make().is_valid("x"));
}

#[test]
fn snapshot_cell_editor_opened() {
    let mut fx = editing();
    fx.key(Key::Down);
    fx.key(Key::F(2));
    insta::assert_snapshot!(fx.render(W as u16, H as u16));
}

#[test]
fn a_click_on_a_hidden_editors_old_place_cancels_and_acts() {
    let mut fx = editing();
    fx.key(Key::F(2));
    fx.type_text("zz");
    fx.t().body_mut().lv_state().top_item = 1;
    fx.render(W as u16, H as u16);
    // The hidden editor's stale bounds are (5, 1)..(17, 2): click there.
    fx.click(8, 1);
    assert!(
        !fx.t().is_editing(),
        "the click is outside the (hidden) editor"
    );
    assert_eq!(fx.t().focused_key(), Some(RowKey::from("b2")), "and acts");
    assert_eq!(fx.cell(0, 1), "web", "nothing stored");
}

#[test]
fn valid_discards_a_dead_edit() {
    let mut fx = editing();
    fx.key(Key::F(2));
    fx.type_text("zz");
    fx.t().set_column_hidden(1, true);
    let mut ok = false;
    fx.with(|t, ctx| ok = t.valid(Command::OK, ctx));
    assert!(ok);
    assert!(!fx.t().is_editing());
    assert_eq!(fx.cell(0, 1), "web", "a hidden column is not written");
    fx.t().set_column_hidden(1, false);
    fx.key(Key::F(2));
    fx.type_text("zz");
    fx.t().set_value(FieldValue::Text("c3".into()));
    fx.with(|t, ctx| ok = t.valid(Command::OK, ctx));
    assert!(ok);
    assert_eq!(fx.cell(0, 1), "web", "a dropped edit is not written");
}

#[test]
fn a_reload_that_removes_the_row_eats_the_next_key() {
    let mut fx = editing();
    fx.key(Key::F(2));
    fx.type_text("zz");
    let deferred = fx.with_deferred(|t, ctx| {
        t.set_model(model(&ROWS[1..]), ctx);
    });
    assert!(!fx.t().is_editing(), "ended at once");
    assert_eq!(message_box_text(&deferred), None, "no box for a reload");
    // A timer broadcast in between does not use up the key.
    fx.send(Event::Broadcast {
        command: Command::custom("test.tick"),
        source: None,
    });
    let (ev, _) = fx.send(Event::KeyDown(KeyEvent::new(
        Key::Enter,
        KeyModifiers::default(),
    )));
    assert!(ev.is_nothing(), "Enter does not reach the dialog");
    assert!(!fx.t().is_editing(), "nor open an editor");
    let (ev, _) = fx.send(Event::KeyDown(KeyEvent::new(
        Key::Enter,
        KeyModifiers::default(),
    )));
    assert!(ev.is_nothing());
    assert!(fx.t().is_editing(), "only that one key: the next edits");
}
