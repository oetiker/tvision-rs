//! Truecolor color-picker — an tvision-rs-original extension (NOT a faithful port).
//!
//! [`ColorPicker`] is a [`Group`](crate::view::Group) container assembled from a
//! [`TabBar`](crate::widgets::TabBar) (tab strip), a
//! [`PageStack`](crate::widgets::PageStack) of four surface pages, and an
//! always-visible `InfoColumn`. The four surfaces and the info column share one
//! `ColorModel` through a [`SharedModel`]. Produces any
//! [`Color`](crate::color::Color) variant.

pub(crate) mod drag;
mod info;
pub mod model;
mod page;
pub(crate) mod plane;
pub(crate) mod presets;
pub(crate) mod rgb;
pub(crate) mod xterm256;

use std::cell::RefCell;
use std::rc::Rc;

use crate::color::Color;
use crate::data::FieldValue;
use crate::event::{Event, Key, hot_key};
use crate::view::{Context, DrawCtx, Group, Point, Rect, View, ViewId};
use model::ColorModel;

/// The picker's surfaces and info column share one model.
pub(crate) type SharedModel = Rc<RefCell<model::ColorModel>>;

/// A picker surface — draws + handles events against the shared [`ColorModel`].
pub(crate) trait Surface {
    fn draw(&self, ctx: &mut DrawCtx, body: Rect, m: &ColorModel);
    fn handle_event(&mut self, ev: &mut Event, body: Rect, m: &mut ColorModel, ctx: &mut Context);
    fn drag_region_at(&self, _p: Point, _body: Rect) -> Option<drag::ColorDragRegion> {
        None
    }
    fn apply_drag(
        &mut self,
        _region: drag::ColorDragRegion,
        _p: Point,
        _body: Rect,
        _m: &mut ColorModel,
    ) {
    }
}

// -- Tab enum -----------------------------------------------------------------

/// The active surface tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Presets,
    Rgb,
    Plane,
    Xterm256,
}

impl Tab {
    const ORDER: [Tab; 4] = [Tab::Presets, Tab::Rgb, Tab::Plane, Tab::Xterm256];

    /// Label for the tab bar, with `~X~` marking the (unique) hotkey letter.
    fn label(self) -> &'static str {
        match self {
            Tab::Presets => "~P~resets",
            Tab::Rgb => "~R~GB",
            Tab::Plane => "~H~ue/Sat",
            Tab::Xterm256 => "~X~term",
        }
    }

    /// The four labels in tab order (Presets, Rgb, Plane, Xterm256).
    fn labels() -> [&'static str; 4] {
        [
            Tab::Presets.label(),
            Tab::Rgb.label(),
            Tab::Plane.label(),
            Tab::Xterm256.label(),
        ]
    }

    fn idx(self) -> usize {
        // ORDER is exhaustive over all Tab variants, so position() always succeeds.
        Self::ORDER.iter().position(|&t| t == self).unwrap()
    }
}

// -- ColorPicker container ----------------------------------------------------

/// The reusable, embeddable truecolor color-picker view. A `Group` wrapping a
/// `TabBar` + `PageStack` (four surface pages) + an `InfoColumn`, all sharing one
/// `ColorModel`. Does NOT own OK/Cancel (dialog chrome).
pub struct ColorPicker {
    group: Group,
    tab_bar_id: ViewId,
    page_stack_id: ViewId,
    model: SharedModel,
    /// A tab preselected via [`select_tab`](Self::select_tab) before the modal
    /// loop, applied (page-switch sync queued) on the first event.
    pending_tab: Option<usize>,
}

/// Info-column width. Sized to fit the widest readout (`Rgb(255,255,255)` is 16
/// chars) plus a 1-cell left margin and a little slack → 18. With the real picker
/// sizes (color_dialog 56 wide, gallery/tvdemo 58–60 wide) this leaves the
/// surface body ≈ 38–42 cols, matching the old fixed `body=38` layout.
const INFO_COL_W: i32 = 18;

impl ColorPicker {
    /// Build a `ColorPicker` occupying `bounds`, seeded with `initial` as the
    /// starting color.
    ///
    /// The picker is a [`Group`](crate::view::Group) containing a `TabBar`,
    /// a `PageStack` of four surface pages (Presets, RGB, Plane, Xterm-256),
    /// and an always-visible `InfoColumn`. All surfaces share one `ColorModel`.
    ///
    /// `bounds` sets the picker's absolute position within its owner (non-zero
    /// origin is supported — embed it inside a dialog with an inset, e.g.
    /// `Rect::new(2, 2, 58, 20)`). The picker handles OK/Cancel buttons
    /// externally; embed it inside a [`Dialog`](crate::dialog::Dialog) and add
    /// buttons separately, or use the convenience wrapper
    /// [`Program::color_dialog`](crate::Program::color_dialog).
    ///
    /// # Turbo Vision heritage
    /// tvision-rs-original extension. Supersedes `TColorDialog::Init` (guide
    /// p. 407–408), which built a fixed 62×19 dialog editing a 16-entry BIOS
    /// palette. This constructor builds only the picker body; the caller owns
    /// the dialog chrome.
    pub fn new(bounds: Rect, initial: Color) -> Self {
        let w = bounds.b.x - bounds.a.x;
        let h = bounds.b.y - bounds.a.y;
        let body_w = (w - INFO_COL_W).max(1);

        let model: SharedModel = Rc::new(RefCell::new(ColorModel::new(initial)));

        // The group carries the picker's real `bounds` (not a (0,0)-based extent):
        // its origin is what the parent's `ctx.sub(child_bounds)` offsets by, so a
        // non-zero placement (e.g. a dialog inserting the picker at (2,2)) lands
        // correctly instead of overdrawing the parent's frame. Children below stay
        // in (0,0)-based local coordinates.
        let mut group = Group::new(bounds);
        // Embeddable selectable child with first-click activation (port of the
        // old ctor's `Options { selectable: true, first_click: true, .. }`).
        {
            let opts = &mut group.state_mut().options;
            opts.selectable = true;
            opts.first_click = true;
        }

        // Tab strip (row 0, body-width).
        let labels = Tab::labels();
        let tab_bar = crate::widgets::TabBar::new(Rect::new(0, 0, body_w, 1), &labels);
        let tab_bar_id = group.insert(Box::new(tab_bar));

        // Page stack (rows 1..h), one page per surface.
        let mut page_stack = crate::widgets::PageStack::new(Rect::new(0, 1, body_w, h));
        let ext = Rect::new(0, 0, body_w, (h - 1).max(1));
        page_stack.insert_page(Box::new(page::SurfacePage::new(
            ext,
            presets::PresetsSurface::new(&model.borrow()),
            model.clone(),
        )));
        page_stack.insert_page(Box::new(page::SurfacePage::new(
            ext,
            rgb::RgbSurface::new(),
            model.clone(),
        )));
        page_stack.insert_page(Box::new(page::SurfacePage::new(
            ext,
            plane::PlaneSurface::new(),
            model.clone(),
        )));
        page_stack.insert_page(Box::new(page::SurfacePage::new(
            ext,
            xterm256::Xterm256Surface::new(&model.borrow()),
            model.clone(),
        )));
        page_stack.bind_tab_bar(tab_bar_id);
        let page_stack_id = group.insert(Box::new(page_stack));

        // Info column (right of the body, full height).
        let info =
            info::InfoColumn::new(Rect::new(w - INFO_COL_W, 0, w, h), model.clone(), initial);
        group.insert(Box::new(info));

        ColorPicker {
            group,
            tab_bar_id,
            page_stack_id,
            model,
            pending_tab: None,
        }
    }

    /// The current selection — the contract `color_dialog` reads.
    pub fn color(&self) -> Color {
        self.model.borrow().color
    }

    /// Open the picker on a specific [`Tab`]. By default the picker starts on the
    /// preset palette; call this to preselect another surface (e.g. the visual
    /// hue/saturation [`Tab::Plane`]) when embedding the picker yourself. Called
    /// BEFORE the modal loop (no `Context`): the TabBar value is set now so the
    /// strip shows the right active tab immediately, and the page switch is
    /// stashed to apply on the first event (the broker needs a `Context`).
    pub fn select_tab(&mut self, tab: Tab) {
        let idx = tab.idx();
        self.pending_tab = Some(idx);
        self.set_tab_bar_value(idx);
    }

    /// Set the embedded `TabBar`'s value (no ctx — see `TabBar::set_value`).
    fn set_tab_bar_value(&mut self, idx: usize) {
        if let Some(tb) = self
            .group
            .child_mut(self.tab_bar_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<crate::widgets::TabBar>())
        {
            View::set_value(tb, FieldValue::Int(idx as i32));
        }
    }

    /// Read the embedded `TabBar`'s selected index.
    fn tab_bar_selected(&mut self) -> usize {
        self.group
            .child_mut(self.tab_bar_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<crate::widgets::TabBar>())
            .map(|tb| tb.selected())
            .unwrap_or(0)
    }

    /// Drive a tab change exactly as a strip click would: set the TabBar value and
    /// queue the PageStack sync (the broker switches the visible page).
    fn switch_to_tab(&mut self, idx: usize, ctx: &mut Context) {
        self.set_tab_bar_value(idx);
        ctx.request_sync_page_stack(self.page_stack_id, self.tab_bar_id);
    }
}

#[crate::delegate(to = group, skip(as_any_mut, handle_event, value, set_value))]
impl View for ColorPicker {
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        // On the first event after a pre-modal `select_tab`, apply the stashed
        // page switch (set_value alone moved the strip; the broker moves the page).
        if let Some(idx) = self.pending_tab.take() {
            self.switch_to_tab(idx, ctx);
        }

        if let Event::KeyDown(ke) = *ev {
            // Ctrl+Left/Right cycle the tab.
            if ke.modifiers.ctrl && matches!(ke.key, Key::Left | Key::Right) {
                let n = Tab::ORDER.len();
                let cur = self.tab_bar_selected();
                let next = if matches!(ke.key, Key::Right) {
                    (cur + 1) % n
                } else {
                    (cur + n - 1) % n
                };
                self.switch_to_tab(next, ctx);
                ev.clear();
                return;
            }
            // Alt+hotkey jumps to a tab (P/R/H/X).
            if ke.modifiers.alt
                && let Key::Char(c) = ke.key
            {
                let up = c.to_ascii_uppercase();
                for t in Tab::ORDER {
                    if hot_key(t.label()) == Some(up) {
                        self.switch_to_tab(t.idx(), ctx);
                        ev.clear();
                        return;
                    }
                }
            }
            // Plain Tab / Shift+Tab belong to the dialog — leave uncleared.
            if ke.key == Key::Tab {
                return;
            }
        }

        // Everything else routes into the group: a strip click broadcasts
        // TAB_BAR_CHANGED → the PageStack broker switches the page; plain keys
        // route to the focused page; the info column ignores them.
        self.group.handle_event(ev, ctx);
    }
}

// -- Tests --------------------------------------------------------------------

#[cfg(test)]
mod view_tests {
    use super::*;
    use crate::color::Color;
    use crate::event::{Key, KeyEvent, KeyModifiers};
    use crate::timer::TimerQueue;
    use crate::view::Deferred;
    use std::collections::VecDeque;

    fn ctrl(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(
            k,
            KeyModifiers {
                ctrl: true,
                ..Default::default()
            },
        ))
    }

    fn plain(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(k, KeyModifiers::default()))
    }

    #[test]
    fn color_returns_seed() {
        let p = ColorPicker::new(Rect::new(0, 0, 56, 18), Color::Rgb(30, 144, 255));
        assert_eq!(p.color(), Color::Rgb(30, 144, 255));
    }

    #[test]
    fn ctrl_right_cycles_tab_and_queues_sync() {
        let mut p = ColorPicker::new(Rect::new(0, 0, 56, 18), Color::Default);
        assert_eq!(p.tab_bar_selected(), 0);

        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            let mut ev = ctrl(Key::Right);
            p.handle_event(&mut ev, &mut ctx);
            assert!(ev.is_nothing());
        }
        assert_eq!(p.tab_bar_selected(), 1, "Ctrl+Right advances the TabBar");
        assert!(
            deferred
                .iter()
                .any(|d| matches!(d, Deferred::PageStackSync { .. })),
            "a PageStackSync is queued so the page switches too"
        );
    }

    #[test]
    fn plain_tab_is_left_unhandled() {
        let mut p = ColorPicker::new(Rect::new(0, 0, 56, 18), Color::Default);
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        let mut ev = plain(Key::Tab);
        p.handle_event(&mut ev, &mut ctx);
        assert!(!ev.is_nothing(), "plain Tab must pass to the dialog");
    }

    #[test]
    fn switching_tab_does_not_change_color() {
        let mut p = ColorPicker::new(Rect::new(0, 0, 56, 18), Color::Rgb(10, 20, 30));
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        let mut ev = ctrl(Key::Right);
        p.handle_event(&mut ev, &mut ctx);
        assert_eq!(p.color(), Color::Rgb(10, 20, 30));
    }

    #[test]
    fn select_tab_sets_strip_immediately() {
        let mut p = ColorPicker::new(Rect::new(0, 0, 56, 18), Color::Default);
        p.select_tab(Tab::Plane);
        assert_eq!(
            p.tab_bar_selected(),
            Tab::Plane.idx(),
            "select_tab moves the strip before any event"
        );
        assert_eq!(p.pending_tab, Some(Tab::Plane.idx()));
    }

    #[test]
    fn picker_group_keeps_its_placement_origin() {
        // Regression: the picker's Group must carry the real `bounds` (origin 2,2),
        // not a (0,0)-based extent — otherwise the parent's `ctx.sub(child_bounds)`
        // applies no offset and the picker overdraws the dialog frame (tab bar on
        // the title row, left edge on the left border).
        let p = ColorPicker::new(Rect::new(2, 2, 60, 19), Color::Default);
        let b = View::state(&p).get_bounds();
        assert_eq!((b.a.x, b.a.y), (2, 2), "picker keeps its placement origin");
        assert_eq!(
            (b.b.x - b.a.x, b.b.y - b.a.y),
            (58, 17),
            "picker keeps its size"
        );
    }
}

#[cfg(test)]
mod snap_tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::color::Color;
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use crate::view::DrawCtx;

    /// Render the whole picker Group at a realistic 56×18 (the color_dialog size).
    /// A 40×12 canvas would shrink the body to 40−18=22 cols — too narrow for the
    /// preset list to read — so we use the real picker size here.
    fn render_picker(initial: Color, tab: Tab) -> String {
        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(56, 18);
        let mut r = Renderer::new(Box::new(backend));
        let mut p = ColorPicker::new(Rect::new(0, 0, 56, 18), initial);
        p.select_tab(tab);
        r.render(|buf: &mut Buffer| {
            let bounds = Rect::new(0, 0, 56, 18);
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            p.draw(&mut dc);
        });
        screen.snapshot()
    }

    #[test]
    fn snapshot_picker_presets() {
        insta::assert_snapshot!(render_picker(Color::Bios(4), Tab::Presets));
    }

    /// Extract the `text:` rows (the `|...|` lines) from a snapshot string.
    fn text_rows(snap: &str) -> Vec<String> {
        let mut rows = Vec::new();
        let mut in_text = false;
        for line in snap.lines() {
            match line {
                "text:" => in_text = true,
                "attr:" | "legend:" => in_text = false,
                _ if in_text && line.starts_with('|') && line.ends_with('|') => {
                    rows.push(line[1..line.len() - 1].to_string());
                }
                _ => {}
            }
        }
        rows
    }

    #[test]
    fn picker_nested_at_offset_renders_below_and_right_of_origin() {
        // Regression for the (0,0)-origin bug: place the picker at (2,2) inside a
        // parent Group and render the parent. The tab bar must land on row 2 / col 2,
        // never on the parent's (0,0) corner.
        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(60, 20);
        let mut r = Renderer::new(Box::new(backend));
        let mut parent = crate::view::Group::new(Rect::new(0, 0, 60, 20));
        let mut picker = ColorPicker::new(Rect::new(2, 2, 60, 19), Color::Default);
        picker.select_tab(Tab::Presets);
        parent.insert(Box::new(picker));
        r.render(|buf: &mut Buffer| {
            let bounds = Rect::new(0, 0, 60, 20);
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            parent.draw(&mut dc);
        });
        let rows = text_rows(&screen.snapshot());
        assert!(
            !rows[0].contains("Presets") && !rows[1].contains("Presets"),
            "tab bar must not overdraw the parent's top rows; row0={:?} row1={:?}",
            rows[0],
            rows[1]
        );
        // Row 2 (the picker's origin) carries the tab strip, offset by 2 columns:
        // 2 leading blanks, then the active corner-cap `┌Presets┐`.
        assert!(
            rows[2].starts_with("  ┌Presets┐"),
            "tab strip starts 2 cols in with the active corner-cap; row2={:?}",
            rows[2]
        );
    }
}
