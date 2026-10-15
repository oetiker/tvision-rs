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

    fn click(&mut self, x: i32, y: i32) -> Broadcasts {
        self.send(mouse_down(x, y, false)).1
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
