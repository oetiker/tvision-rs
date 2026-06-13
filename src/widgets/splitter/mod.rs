pub mod layout;

use crate::event::Event;
use crate::view::{Context, DrawCtx, Group, Point, Rect, View, ViewId, ViewState};

pub use layout::{Constraints, Orientation};
use layout::{Slot, solve};

/// How the seam *after* a given pane looks and behaves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DividerStyle {
    /// Always drawn; grab-and-drag anytime.
    Line,
    /// Clean look; only a small grab nub at the midpoint.
    Handle,
    /// Invisible & seamless in normal use, but resizable in reconfig mode.
    Hidden,
    /// Invisible AND immovable — a permanent boundary, even in reconfig mode.
    Locked,
}

impl DividerStyle {
    /// Whether a *live* mouse drag may grab this divider in normal use.
    pub fn draggable_live(&self) -> bool {
        matches!(self, DividerStyle::Line | DividerStyle::Handle)
    }
    /// Whether reconfig mode may move this divider.
    pub fn movable_in_reconfig(&self) -> bool {
        !matches!(self, DividerStyle::Locked)
    }
}

/// A generic, N-ary, resizable multi-pane view. One axis, N child panes, N−1
/// dividers in 1-cell gaps. Embeds a `Group` and delegates the un-overridden
/// `View` methods to it. See `docs/superpowers/specs/2026-06-13-splitter-design.md`.
pub struct Splitter {
    group: Group,
    orientation: Orientation,
    /// Per-pane solver slots, parallel to the group's children in INSERTION order.
    slots: Vec<Slot>,
    /// Per-divider styles (len ≤ panes−1); `default_style` fills any gap.
    #[allow(dead_code)]
    divider_styles: Vec<DividerStyle>,
    #[allow(dead_code)]
    default_style: DividerStyle,
    /// Reconfig-mode selected divider (Task 8). `None` = normal use.
    #[allow(dead_code)]
    reconfig: Option<usize>,
    /// Absolute origin captured each `draw`, for the mouse-track capture (Task 6).
    #[allow(dead_code)]
    abs_origin: Point,
}

impl Splitter {
    fn new(bounds: Rect, orientation: Orientation) -> Self {
        Splitter {
            group: Group::new(bounds),
            orientation,
            slots: Vec::new(),
            divider_styles: Vec::new(),
            default_style: DividerStyle::Line,
            reconfig: None,
            abs_origin: bounds.a,
        }
    }

    /// Empty horizontal-axis splitter (side-by-side panes, vertical dividers).
    pub fn cols() -> Self {
        Splitter::new(Rect::new(0, 0, 0, 0), Orientation::Cols)
    }

    /// Empty vertical-axis splitter (stacked panes, horizontal dividers).
    pub fn rows() -> Self {
        Splitter::new(Rect::new(0, 0, 0, 0), Orientation::Rows)
    }

    /// Axis length available to content = bounds extent minus the N−1 divider cells.
    fn content_len(&self) -> i32 {
        let b = self.group.state().get_bounds();
        let extent = match self.orientation {
            Orientation::Cols => b.b.x - b.a.x,
            Orientation::Rows => b.b.y - b.a.y,
        };
        let dividers = self.slots.len().saturating_sub(1) as i32;
        (extent - dividers).max(0)
    }

    /// Insert a pane with its constraints; returns the pane's `ViewId`. Re-solves.
    pub fn insert(&mut self, view: Box<dyn View>, c: Constraints) -> ViewId {
        let id = self.group.insert(view);
        self.slots.push(Slot::from_constraints(c));
        self.resolve_layout_local();
        id
    }

    /// Compute each child's `Rect` from the solver and apply via `change_bounds`.
    /// Local (no `Context`) — used at insert/build/resize time.
    fn resolve_layout_local(&mut self) {
        let sizes = solve(&self.slots, self.content_len());
        let b = self.group.state().get_bounds();
        let mut cursor = match self.orientation {
            Orientation::Cols => b.a.x,
            Orientation::Rows => b.a.y,
        };
        let ids = self.group.child_ids_in_order();
        for (i, id) in ids.iter().enumerate() {
            let size = sizes.get(i).copied().unwrap_or(0);
            let rect = match self.orientation {
                Orientation::Cols => Rect::new(cursor, b.a.y, cursor + size, b.b.y),
                Orientation::Rows => Rect::new(b.a.x, cursor, b.b.x, cursor + size),
            };
            if let Some(child) = self.group.find_mut(*id) {
                child.change_bounds(rect);
            }
            cursor += size + 1; // +1 for the divider cell that follows
        }
    }
}

#[crate::delegate(to = group)]
impl View for Splitter {
    fn state(&self) -> &ViewState {
        self.group.state()
    }
    fn state_mut(&mut self) -> &mut ViewState {
        self.group.state_mut()
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        self.abs_origin = ctx.origin();
        self.group.draw(ctx);
        // Task 4 adds divider drawing here.
    }

    fn change_bounds(&mut self, bounds: Rect) {
        self.group.state_mut().set_bounds(bounds);
        self.resolve_layout_local();
    }

    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        // Task 6 adds mouse drag; Task 8 adds reconfig. For now, pass through.
        self.group.handle_event(ev, ctx);
    }
}

#[cfg(test)]
mod divider_tests {
    use super::*;

    #[test]
    fn draggability_matrix() {
        assert!(DividerStyle::Line.draggable_live());
        assert!(DividerStyle::Handle.draggable_live());
        assert!(!DividerStyle::Hidden.draggable_live());
        assert!(!DividerStyle::Locked.draggable_live());

        assert!(DividerStyle::Hidden.movable_in_reconfig());
        assert!(!DividerStyle::Locked.movable_in_reconfig());
    }
}

#[cfg(test)]
mod view_tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::screen::Buffer;
    use crate::theme::{Role, Theme};

    struct Fill(char, ViewState);
    impl Fill {
        fn boxed(ch: char) -> Box<dyn View> {
            Box::new(Fill(ch, ViewState::new(Rect::new(0, 0, 1, 1))))
        }
    }
    impl View for Fill {
        fn state(&self) -> &ViewState {
            &self.1
        }
        fn state_mut(&mut self) -> &mut ViewState {
            &mut self.1
        }
        fn draw(&mut self, ctx: &mut DrawCtx) {
            // Group hands each child a sub-context translated to the child's
            // origin, so the child fills in LOCAL coords (0,0)-(w,h).
            let b = self.1.get_bounds();
            let (w, h) = (b.b.x - b.a.x, b.b.y - b.a.y);
            ctx.fill(Rect::new(0, 0, w, h), self.0, ctx.style(Role::Normal));
        }
    }

    fn render(sp: &mut Splitter, w: u16, h: u16) -> String {
        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(w, h);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = sp.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            sp.draw(&mut dc);
        });
        screen.snapshot()
    }

    #[test]
    fn three_equal_columns_render() {
        let mut sp = Splitter::cols();
        sp.change_bounds(Rect::new(0, 0, 32, 4)); // 32 wide; 2 dividers => 30 content => 10/10/10
        sp.insert(Fill::boxed('A'), Constraints::flex());
        sp.insert(Fill::boxed('B'), Constraints::flex());
        sp.insert(Fill::boxed('C'), Constraints::flex());
        insta::assert_snapshot!(render(&mut sp, 32, 4));
    }
}
