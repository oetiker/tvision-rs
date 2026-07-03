//! Integration fixtures for the three-surface rule
//! ([`DrawCtx::content_surface`]) driven end-to-end through the **real**
//! `Group` focus chain — `Group::draw` fans its own `focused` flag to
//! children as `owner_active`, and `Group::set_current`/`set_state` move
//! `state.focused` between children — rather than through hand-set `DrawCtx`
//! flags.
//!
//! Two fixtures (spec test plan):
//! 1. The shuttle: one focused pane holding two selectable `ListBox`es and a
//!    `Button`. Moving focus between the three siblings must move each
//!    list's row surface between `ListNormal` (self-focused) and
//!    `ListSurface` ("a selectable sibling holds focus"); defocusing the
//!    whole pane must recede both to `ListInactive`.
//! 2. The single-focusable-pane invariant: a pane whose only selectable
//!    child is one list never shows `ListSurface` — there is no sibling that
//!    could ever "win" the focus contest.

use std::collections::VecDeque;

use tvision_rs::view::{Context, Deferred, DrawCtx, Group, Rect, SelectMode, StateFlag, View};
use tvision_rs::{Button, ButtonFlags, Command, Role, Style, Theme, TimerQueue};
use tvision_rs::{ListBox, screen::Buffer};

/// A `classic_blue` theme with `ListNormal`/`ListSurface`/`ListInactive`
/// overridden to three visibly distinct styles (`classic_blue` otherwise
/// collapses `ListSurface` onto `ListNormal` — see `theme.rs`). Returns the
/// theme plus the three styles so callers can assert against them and pin
/// the pairwise-distinct precondition.
fn three_way_theme() -> (Theme, Style, Style, Style) {
    let mut theme = Theme::classic_blue();
    let normal = Style::new(tvision_rs::Color::Bios(0xF), tvision_rs::Color::Bios(0x1)); // white on blue
    let surface = Style::new(tvision_rs::Color::Bios(0xE), tvision_rs::Color::Bios(0x3)); // yellow on cyan
    let inactive = Style::new(tvision_rs::Color::Bios(0x8), tvision_rs::Color::Bios(0x0)); // darkgray on black
    theme.set_style(Role::ListNormal, normal);
    theme.set_style(Role::ListSurface, surface);
    theme.set_style(Role::ListInactive, inactive);

    // No vacuous equality: the three roles must actually differ before we
    // lean on them to distinguish focus states.
    assert_ne!(normal, surface, "ListNormal and ListSurface must differ");
    assert_ne!(normal, inactive, "ListNormal and ListInactive must differ");
    assert_ne!(
        surface, inactive,
        "ListSurface and ListInactive must differ"
    );

    (theme, normal, surface, inactive)
}

/// A fresh `Context` for driving `Group`/`ListBox` mutators in tests. Each
/// call owns its backing queues so the returned `Context` can be dropped and
/// rebuilt between focus-mechanism calls without borrow conflicts.
struct CtxOwner {
    out: VecDeque<tvision_rs::event::Event>,
    timers: TimerQueue,
    deferred: Vec<Deferred>,
}
impl CtxOwner {
    fn new() -> Self {
        CtxOwner {
            out: VecDeque::new(),
            timers: TimerQueue::new(),
            deferred: Vec::new(),
        }
    }
    fn ctx(&mut self) -> Context<'_> {
        Context::new(&mut self.out, &mut self.timers, 0, &mut self.deferred)
    }
}

/// Draw `pane` into a fresh `w`x`h` buffer at its own (possibly non-zero)
/// bounds and return the buffer for cell sampling.
fn draw_pane(pane: &mut Group, theme: &Theme, w: u16, h: u16) -> Buffer {
    let mut buf = Buffer::new(w, h);
    let bounds = pane.state().get_bounds();
    let mut dc = DrawCtx::new(&mut buf, theme, bounds, bounds.a);
    pane.draw(&mut dc);
    buf
}

/// A few distinct row labels — enough to give each list a non-item-0 row to
/// sample (item 0 is always drawn as the focused-cursor or the
/// default-selected color, never the plain row surface: `ListViewer`'s
/// `is_selected` base is `item == focused`, and `focused` defaults to 0).
fn rows() -> Vec<String> {
    vec!["Row 0".into(), "Row 1".into(), "Row 2".into()]
}

// ---------------------------------------------------------------------------
// 1. Shuttle fixture
// ---------------------------------------------------------------------------

/// One focused pane (NON-ZERO origin, per project test convention) holding
/// two selectable `ListBox`es and a `Button`. Shuttling focus between the
/// three siblings via the group's real focus mechanism
/// (`set_state(Focused)` + `set_current`) must move each list's row surface
/// through the three-surface rule end-to-end.
#[test]
fn shuttle_moves_row_surface_through_group_focus_chain() {
    let (theme, normal, surface, inactive) = three_way_theme();
    let mut owner = CtxOwner::new();

    // Pane inset at (2, 1) inside a nominal 44x13 screen — never test only at
    // (0, 0): the sub-context translation that carries `owner_active` down
    // to the children must survive a real, non-zero origin.
    let mut pane = Group::new(Rect::new(2, 1, 44, 13));

    let mut list_a = ListBox::new(Rect::new(1, 1, 21, 6), 1, None, None);
    list_a.new_list(rows(), &mut owner.ctx());
    let mut list_b = ListBox::new(Rect::new(22, 1, 42, 6), 1, None, None);
    list_b.new_list(rows(), &mut owner.ctx());
    let button = Button::new(
        Rect::new(1, 7, 21, 9),
        "~O~K",
        Command::OK,
        ButtonFlags::new(),
    );

    let id_a = pane.insert(Box::new(list_a));
    let _id_b = pane.insert(Box::new(list_b));
    let id_button = pane.insert(Box::new(button));

    // Cell inside list A's / list B's row 1 (item index 1, never the
    // focused-cursor or default-selected item-0 cell): pane origin (2, 1) +
    // list-local origin (x=0) + row offset (y=1).
    let cell_a: (u16, u16) = (2 + 1, 1 + 1 + 1); // (3, 3)
    let cell_b: (u16, u16) = (2 + 22, 1 + 1 + 1); // (24, 3)

    let sample = |pane: &mut Group, x: u16, y: u16| -> Style {
        draw_pane(pane, &theme, 44, 13).get(x, y).style()
    };

    // Focus the pane itself (real mechanism: the Focused state flag, exactly
    // what a window activation would flip); no current child yet, so the
    // cascade to a child is a no-op here.
    pane.set_state(StateFlag::Focused, true, &mut owner.ctx());

    // -- List A focused: A -> normal, B -> surface --------------------------
    pane.set_current(Some(id_a), SelectMode::Normal, &mut owner.ctx());
    assert_eq!(
        sample(&mut pane, cell_a.0, cell_a.1),
        normal,
        "list A is the focused control -> its own row surface is ListNormal"
    );
    assert_eq!(
        sample(&mut pane, cell_b.0, cell_b.1),
        surface,
        "list B is a selectable sibling that lost the focus contest -> ListSurface"
    );

    // -- Button focused: BOTH lists -> surface -------------------------------
    pane.set_current(Some(id_button), SelectMode::Normal, &mut owner.ctx());
    assert_eq!(
        sample(&mut pane, cell_a.0, cell_a.1),
        surface,
        "neither list holds focus once the button does -> ListSurface"
    );
    assert_eq!(
        sample(&mut pane, cell_b.0, cell_b.1),
        surface,
        "neither list holds focus once the button does -> ListSurface"
    );

    // -- Owning pane unfocused: BOTH -> inactive -----------------------------
    pane.set_state(StateFlag::Focused, false, &mut owner.ctx());
    assert_eq!(
        sample(&mut pane, cell_a.0, cell_a.1),
        inactive,
        "an unfocused owning pane recedes every child's surface to ListInactive"
    );
    assert_eq!(
        sample(&mut pane, cell_b.0, cell_b.1),
        inactive,
        "an unfocused owning pane recedes every child's surface to ListInactive"
    );
}

// ---------------------------------------------------------------------------
// 2. Single-focusable-pane invariant
// ---------------------------------------------------------------------------

/// A pane whose only selectable child is one list: there is no sibling that
/// could ever "win" the focus contest, so the list's row surface must never
/// show `ListSurface` — only `ListNormal` (pane focused) or `ListInactive`
/// (pane unfocused).
#[test]
fn single_focusable_pane_never_shows_surface_colour() {
    let (theme, normal, surface, inactive) = three_way_theme();
    let mut owner = CtxOwner::new();

    // Non-zero origin again, independently of the shuttle fixture's pane.
    let mut pane = Group::new(Rect::new(3, 2, 30, 10));

    let mut list = ListBox::new(Rect::new(1, 1, 20, 5), 1, None, None);
    list.new_list(rows(), &mut owner.ctx());
    let id = pane.insert(Box::new(list));

    // Same non-item-0 row-1 sampling rule as the shuttle fixture.
    let cell: (u16, u16) = (3 + 1, 2 + 1 + 1); // (4, 4)
    let sample =
        |pane: &mut Group| -> Style { draw_pane(pane, &theme, 30, 10).get(cell.0, cell.1).style() };

    // Pane focused -> its only selectable child becomes current+focused.
    pane.set_state(StateFlag::Focused, true, &mut owner.ctx());
    pane.set_current(Some(id), SelectMode::Normal, &mut owner.ctx());
    let focused_style = sample(&mut pane);
    assert_eq!(
        focused_style, normal,
        "the sole selectable child is always self-focused when the pane is -> ListNormal"
    );
    assert_ne!(
        focused_style, surface,
        "with no sibling to lose a focus contest to, ListSurface must never appear"
    );

    // Pane unfocused -> recedes to inactive.
    pane.set_state(StateFlag::Focused, false, &mut owner.ctx());
    let unfocused_style = sample(&mut pane);
    assert_eq!(
        unfocused_style, inactive,
        "an unfocused pane recedes its only child to ListInactive"
    );
    assert_ne!(
        unfocused_style, surface,
        "with no sibling to lose a focus contest to, ListSurface must never appear"
    );
}
