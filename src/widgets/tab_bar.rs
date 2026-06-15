//! A single-row tab selector: corner-cap active tab (`┌Label┐`), ←/→/hotkey/click
//! selection. An rstv-original widget — no Turbo Vision ancestor for the tab idiom
//! — but **cluster-shaped**: it follows the `TCluster`/`TRadioButtons` contract
//! (single selection, `~X~` hotkeys, `find_sel` hit-test, press-on-release,
//! `value`/`set_value` transfer), the way the C++ color dialog's
//! `TMonoSelector : public TCluster` was a custom selector built on the cluster.
//! On a change it broadcasts [`Command::TAB_BAR_CHANGED`] carrying its own
//! [`ViewId`](crate::view::ViewId) as `source`, so a sibling
//! [`PageStack`](crate::widgets::PageStack) can react (mirrors `ScrollBar`).
//!
//! # Turbo Vision heritage
//! None — the tabbed idiom is an rstv extension; the selection mechanics mirror
//! `TCluster`.

use crate::capture::TrackMask;
use crate::command::Command;
use crate::data::FieldValue;
use crate::event::{Event, Key, hot_key};
use crate::theme::Role;
use crate::view::{Context, DrawCtx, Point, Rect, View, ViewState};

/// A horizontal single-row tab selector. See the [module docs](self).
pub struct TabBar {
    /// View state (geometry, flags) — the composition target.
    pub state: ViewState,
    /// Tab labels, each optionally carrying a `~X~` hotkey marker.
    tabs: Vec<String>,
    /// Selected/active tab (cursor ≡ selection for a tab strip), clamped.
    value: usize,
    /// Absolute origin of bar-local (0,0), cached each `draw` for the track capture.
    abs_origin: Point,
    /// Whether a press-on-release mouse track is in flight.
    tracking: bool,
    /// The tab the mouse went down on (pressed only if MouseUp lands on the same one).
    pressed: Option<usize>,
}

impl TabBar {
    /// Construct at `bounds` with `labels` (each may carry `~X~`). Focusable; starts on tab 0.
    pub fn new(bounds: Rect, labels: &[&str]) -> Self {
        let mut state = ViewState::new(bounds);
        state.options.selectable = true;
        TabBar {
            state,
            tabs: labels.iter().map(|s| s.to_string()).collect(),
            value: 0,
            abs_origin: bounds.a,
            tracking: false,
            pressed: None,
        }
    }

    /// The selected tab index.
    pub fn selected(&self) -> usize {
        self.value
    }

    fn label_len(label: &str) -> i32 {
        label.chars().filter(|&c| c != '~').count() as i32
    }

    /// `(start_x, width)` per tab; the active tab is +2 wide (its caps); 1-cell gaps.
    fn tab_layout(&self) -> Vec<(i32, i32)> {
        let mut out = Vec::with_capacity(self.tabs.len());
        let mut x = 0i32;
        for (i, label) in self.tabs.iter().enumerate() {
            let w = Self::label_len(label) + if i == self.value { 2 } else { 0 };
            out.push((x, w));
            x += w + 1;
        }
        out
    }

    /// Natural width to fit all tabs (labels + caps for the one active + gaps). Stable.
    pub fn natural_width(&self) -> i32 {
        let n = self.tabs.len() as i32;
        if n == 0 {
            return 0;
        }
        self.tabs.iter().map(|l| Self::label_len(l)).sum::<i32>() + 2 + (n - 1)
    }

    /// The tab under view-local point `p`, or `None`.
    fn find_sel(&self, p: Point) -> Option<usize> {
        if p.y != 0 {
            return None;
        }
        for (i, (start, w)) in self.tab_layout().iter().enumerate() {
            if p.x >= *start && p.x < start + w {
                return Some(i);
            }
        }
        None
    }

    /// Select `item` (clamped); broadcast `TAB_BAR_CHANGED` (source = self) only on change.
    fn press(&mut self, item: usize, ctx: &mut Context) {
        let item = item.min(self.tabs.len().saturating_sub(1));
        if item != self.value {
            self.value = item;
            ctx.broadcast(Command::TAB_BAR_CHANGED, self.state.id());
        }
    }
}

impl View for TabBar {
    fn state(&self) -> &ViewState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }

    /// Selected index as the typed transfer currency (getData successor).
    fn value(&self) -> Option<FieldValue> {
        Some(FieldValue::Int(self.value as i32))
    }

    /// Load the selected index (clamped). The trait setter takes NO ctx
    /// (there is a separate `set_value_ctx`) — match that signature.
    fn set_value(&mut self, v: FieldValue) {
        if let FieldValue::Int(i) = v {
            self.value = (i.max(0) as usize).min(self.tabs.len().saturating_sub(1));
        }
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        self.abs_origin = ctx.origin();
        let g = *ctx.glyphs();
        let (norm, norm_hi) = (
            ctx.style(Role::LabelNormal),
            ctx.style(Role::LabelNormalShortcut),
        );
        let (act, act_hi) = (
            ctx.style(Role::LabelLight),
            ctx.style(Role::LabelLightShortcut),
        );
        let layout = self.tab_layout();
        for (i, (start, _w)) in layout.iter().enumerate() {
            let label = self.tabs[i].clone();
            if i == self.value {
                ctx.put_char(*start, 0, g.frame_tl, act);
                let lw = ctx.put_cstr(start + 1, 0, &label, act, act_hi);
                ctx.put_char(start + 1 + lw, 0, g.frame_tr, act);
            } else {
                ctx.put_cstr(*start, 0, &label, norm, norm_hi);
            }
        }
    }

    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        let n = self.tabs.len();
        if n == 0 {
            return;
        }
        match *ev {
            Event::KeyDown(ke) => match ke.key {
                Key::Left => {
                    self.press((self.value + n - 1) % n, ctx);
                    ev.clear();
                }
                Key::Right => {
                    self.press((self.value + 1) % n, ctx);
                    ev.clear();
                }
                Key::Char(c) => {
                    let up = c.to_ascii_uppercase();
                    // hot_key returns the char already uppercased
                    if let Some(i) = self.tabs.iter().position(|l| hot_key(l) == Some(up)) {
                        self.press(i, ctx);
                        ev.clear();
                    }
                }
                _ => {}
            },
            // Press-on-release (mirrors cluster.rs): arm a track on down, commit on up.
            Event::MouseDown(me) => {
                if let Some(i) = self.find_sel(me.position) {
                    self.pressed = Some(i);
                    if let Some(id) = self.state.id() {
                        self.tracking = true;
                        ctx.start_mouse_track(
                            id,
                            self.abs_origin,
                            TrackMask {
                                mouse_move: true,
                                ..Default::default()
                            },
                        );
                    } else {
                        // Degenerate fallback (no ViewId): single-shot press on down.
                        self.press(i, ctx);
                    }
                    ev.clear();
                }
            }
            Event::MouseUp(me) if self.tracking => {
                self.tracking = false;
                // pressed.take() clears self.pressed regardless of the release position.
                let pressed = self.pressed.take();
                if let (Some(p), Some(i)) = (pressed, self.find_sel(me.position))
                    && p == i
                {
                    self.press(i, ctx);
                }
                ev.clear();
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::event::{
        KeyEvent, KeyModifiers, MouseButtons, MouseEvent, MouseEventFlags, MouseWheel,
    };
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use crate::view::{Deferred, Group};
    use std::collections::VecDeque;

    // -- Helper to build a Context with local backing storage ----------------

    fn drive(tb: &mut TabBar, ev: &mut Event, out: &mut VecDeque<Event>) {
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let mut ctx = Context::new(out, &mut timers, 0, &mut deferred);
        tb.handle_event(ev, &mut ctx);
    }

    fn key(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(k, KeyModifiers::default()))
    }

    fn mouse(kind: fn(MouseEvent) -> Event, x: i32) -> Event {
        kind(MouseEvent {
            position: Point::new(x, 0),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            flags: MouseEventFlags::default(),
            wheel: MouseWheel::None,
            modifiers: KeyModifiers::default(),
        })
    }

    // -----------------------------------------------------------------------
    // Task 5 unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn natural_width_sums_labels_caps_gaps() {
        // "AB"(2) + "CDE"(3) + 2(caps) + 1(gap) = 8
        assert_eq!(
            TabBar::new(Rect::new(0, 0, 20, 1), &["AB", "CDE"]).natural_width(),
            8
        );
        // "~P~resets" → visible chars = "resets" + "P" = 7 chars, + 2 caps = 9
        assert_eq!(
            TabBar::new(Rect::new(0, 0, 20, 1), &["~P~resets"]).natural_width(),
            9
        );
    }

    #[test]
    fn value_protocol_reports_and_sets_clamped() {
        let mut tb = TabBar::new(Rect::new(0, 0, 20, 1), &["A", "B", "C"]);
        assert_eq!(View::value(&tb), Some(FieldValue::Int(0)));
        View::set_value(&mut tb, FieldValue::Int(5)); // clamped to 2 (len-1)
        assert_eq!(tb.selected(), 2);
    }

    #[test]
    fn find_sel_locates_tab_under_point() {
        // Active tab 0 ("A"): width = 1 + 2 = 3 → [0..3)  gap@3
        // Tab 1 ("B"): width = 1           → [4..5)  gap@5
        // Tab 2 ("C"): width = 1           → [6..7)
        let tb = TabBar::new(Rect::new(0, 0, 20, 1), &["A", "B", "C"]);
        assert_eq!(tb.find_sel(Point::new(1, 0)), Some(0));
        assert_eq!(tb.find_sel(Point::new(4, 0)), Some(1));
        assert_eq!(tb.find_sel(Point::new(6, 0)), Some(2));
        assert_eq!(tb.find_sel(Point::new(3, 0)), None, "the gap");
        assert_eq!(tb.find_sel(Point::new(1, 1)), None, "wrong row");
    }

    // -----------------------------------------------------------------------
    // Task 7 event tests
    // -----------------------------------------------------------------------

    #[test]
    fn right_left_arrows_cycle_with_wrap() {
        let mut tb = TabBar::new(Rect::new(0, 0, 30, 1), &["A", "B", "C"]);
        let mut out = VecDeque::new();
        let mut e = key(Key::Right);
        drive(&mut tb, &mut e, &mut out);
        assert_eq!(tb.selected(), 1);
        tb.value = 2;
        let mut e = key(Key::Right);
        drive(&mut tb, &mut e, &mut out);
        assert_eq!(tb.selected(), 0, "wrap");
        let mut e = key(Key::Left);
        drive(&mut tb, &mut e, &mut out);
        assert_eq!(tb.selected(), 2, "wrap back");
    }

    #[test]
    fn hotkey_selects() {
        let mut tb = TabBar::new(Rect::new(0, 0, 30, 1), &["~P~resets", "~R~GB", "~X~term"]);
        let mut out = VecDeque::new();
        let mut e = key(Key::Char('x'));
        drive(&mut tb, &mut e, &mut out);
        assert_eq!(tb.selected(), 2);
        assert!(e.is_nothing());
    }

    #[test]
    fn mouse_presses_on_release_over_same_tab() {
        let mut tb = TabBar::new(Rect::new(0, 0, 30, 1), &["A", "B", "C"]);
        tb.state.id = Some(crate::view::ViewId::next());
        let mut out = VecDeque::new();
        // Tab 2 ("C"): active is tab 0 (w=3), gap@3, tab1 [4..5), gap@5, tab2 [6..7)
        let mut down = mouse(Event::MouseDown, 6);
        drive(&mut tb, &mut down, &mut out);
        assert_eq!(tb.selected(), 0, "no commit on down");
        let mut up = mouse(Event::MouseUp, 6);
        drive(&mut tb, &mut up, &mut out);
        assert_eq!(tb.selected(), 2, "commit on release over same tab");
    }

    #[test]
    fn mouse_release_on_different_tab_does_not_commit() {
        let mut tb = TabBar::new(Rect::new(0, 0, 30, 1), &["A", "B", "C"]);
        tb.state.id = Some(crate::view::ViewId::next());
        let mut out = VecDeque::new();
        // down on tab 2 (x=6)
        let mut down = mouse(Event::MouseDown, 6);
        drive(&mut tb, &mut down, &mut out);
        // up on tab 0 (x=1) — different tab
        let mut up = mouse(Event::MouseUp, 1);
        drive(&mut tb, &mut up, &mut out);
        assert_eq!(tb.selected(), 0, "no commit when release lands elsewhere");
    }

    #[test]
    fn change_broadcasts_with_source_id() {
        let tb = TabBar::new(Rect::new(0, 0, 30, 1), &["A", "B", "C"]);
        let mut group = Group::new(Rect::new(0, 0, 30, 1));
        let id = group.insert(Box::new(tb));
        let mut out = VecDeque::new();
        {
            let mut timers = crate::timer::TimerQueue::new();
            let mut deferred: Vec<Deferred> = vec![];
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            let mut e = key(Key::Right);
            group.find_mut(id).unwrap().handle_event(&mut e, &mut ctx);
        }
        assert!(out.iter().any(|e| matches!(e,
            Event::Broadcast { command, source } if *command == Command::TAB_BAR_CHANGED && *source == Some(id)
        )));
    }

    // -----------------------------------------------------------------------
    // Task 6 snapshot tests
    // -----------------------------------------------------------------------

    fn render(active: usize) -> String {
        let theme = Theme::classic_blue();
        let mut tb = TabBar::new(Rect::new(0, 0, 30, 1), &["~P~resets", "~R~GB", "~H~ue/Sat"]);
        tb.value = active;
        let (backend, screen) = HeadlessBackend::new(30, 1);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let b = Rect::new(0, 0, 30, 1);
            tb.draw(&mut DrawCtx::new(buf, &theme, b, b.a));
        });
        screen.snapshot()
    }

    #[test]
    fn snapshot_tabbar_first_active() {
        insta::assert_snapshot!(render(0));
    }

    #[test]
    fn snapshot_tabbar_middle_active() {
        insta::assert_snapshot!(render(1));
    }
}
