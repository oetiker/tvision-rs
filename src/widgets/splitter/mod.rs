pub mod layout;

use crate::capture::TrackMask;
use crate::event::Event;
use crate::junction::{Edge, Junction, JunctionMark, Weight, divider_junction};
use crate::theme::Role;
use crate::view::{Context, DrawCtx, Group, GrowMode, Point, Rect, View, ViewId, ViewState};

pub use layout::{Constraints, Orientation};
use layout::{Slot, relax_weight, solve};

/// How the seam *after* a given pane looks and behaves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DividerStyle {
    /// Always drawn; grab-and-drag anytime.
    Line,
    /// Clean look; only a small grab nub at the midpoint.
    Handle,
    /// Invisible & seamless in normal use, but resizable in resize mode.
    Hidden,
    /// Invisible AND immovable — a permanent boundary, even in resize mode.
    Locked,
}

impl DividerStyle {
    /// Whether a *live* mouse drag may grab this divider in normal use.
    pub fn draggable_live(&self) -> bool {
        matches!(self, DividerStyle::Line | DividerStyle::Handle)
    }
    /// Whether this divider may be moved (true for everything except `Locked`).
    pub fn movable(&self) -> bool {
        !matches!(self, DividerStyle::Locked)
    }
}

/// Color role for a divider line: being-moved beats everything, then the line
/// matches the owning window frame (active vs passive).
fn divider_role(moving: bool, active: bool) -> Role {
    if moving {
        Role::FrameDragging
    } else if active {
        Role::FrameActive
    } else {
        Role::FramePassive
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
    divider_styles: Vec<DividerStyle>,
    default_style: DividerStyle,
    /// The divider that is the active keyboard resize target (highlighted), if any.
    active_divider: Option<usize>,
    /// Weight snapshot taken at [`Splitter::begin_resize_session`]; restored on Esc cancel.
    saved_weights: Vec<f64>,
    /// Absolute origin captured each `draw`, for the mouse-track capture (Task 6).
    abs_origin: Point,
    /// Active divider being mouse-dragged (Task 6).
    dragging: Option<usize>,
    /// Opt-in: join this splitter's linework — interior `├`/`┼` divider crossings
    /// and (when it is a window's body) the divider→frame junctions the window
    /// brokers. Default `false` — an un-joined splitter renders plain `│`/`─`
    /// dividers and emits no frame marks. Cascades to pane sub-splitters.
    joined: bool,
}

impl Splitter {
    fn new(bounds: Rect, orientation: Orientation) -> Self {
        let mut group = Group::new(bounds);
        // A splitter is a layout container: by default it grows with its owner
        // (bottom-right tracks the owner's resize, top-left stays — keeping any
        // inset), so its panes fill and resize without the caller wiring grow_mode.
        // Opt out with `with_grow_mode` (e.g. `GrowMode::default()` for fixed size).
        // (Harmless when nested: a parent splitter sets child bounds explicitly,
        // so a sub-splitter's grow_mode is never consulted.)
        group.state_mut().grow_mode = GrowMode {
            hi_x: true,
            hi_y: true,
            ..Default::default()
        };
        Splitter {
            group,
            orientation,
            slots: Vec::new(),
            divider_styles: Vec::new(),
            default_style: DividerStyle::Line,
            active_divider: None,
            saved_weights: Vec::new(),
            abs_origin: bounds.a,
            dragging: None,
            joined: false,
        }
    }

    /// Builder: opt this splitter's linework into joining — its divider lines
    /// connect to each other (interior `├`/`┼` crossings) and, when it is a
    /// window's body, to the surrounding frame (the window auto-brokers). Joining
    /// cascades to pane sub-splitters, so a nested grid needs `.joined()` only on
    /// the outermost splitter.
    pub fn joined(mut self) -> Self {
        self.set_joined(true);
        self
    }

    /// Override the grow mode (how the splitter resizes with its owner). A
    /// splitter grows to fill by default (`{ hi_x, hi_y }`); pass
    /// `GrowMode::default()` for a fixed-size splitter, or any custom mode.
    pub fn with_grow_mode(mut self, grow_mode: GrowMode) -> Self {
        self.group.state_mut().grow_mode = grow_mode;
        self
    }

    /// Set joining at runtime. Propagates to pane sub-splitters (ancestor joined
    /// ⇒ subtree joined), so the whole grid follows the outermost setting.
    pub fn set_joined(&mut self, on: bool) {
        self.joined = on;
        let ids = self.group.child_ids_in_order();
        for id in ids {
            if let Some(sub) = self
                .group
                .child_mut(id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<Splitter>())
            {
                sub.set_joined(on);
            }
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
        // If this splitter is already joined, a sub-splitter pane added afterwards
        // must inherit it (so `.joined()` set before adding panes still cascades).
        if self.joined
            && let Some(sub) = self
                .group
                .child_mut(id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<Splitter>())
        {
            sub.set_joined(true);
        }
        self.resolve_layout_local();
        id
    }

    /// The effective style of divider `i` (between pane `i` and `i+1`).
    fn style_of(&self, i: usize) -> DividerStyle {
        self.divider_styles
            .get(i)
            .copied()
            .unwrap_or(self.default_style)
    }

    /// True while divider `i` is being moved (mouse drag or the active keyboard
    /// resize target) — drives the FrameDragging highlight.
    fn is_moving(&self, i: usize) -> bool {
        self.dragging == Some(i) || self.active_divider == Some(i)
    }

    /// Paint the N−1 dividers into the 1-cell gaps. Called by `draw` AFTER the
    /// group paints its children. `ctx` is the splitter's own draw context
    /// (origin == splitter bounds origin), so coordinates are local (0-based).
    fn draw_dividers(&self, ctx: &mut DrawCtx) {
        let b = self.group.state().get_bounds();
        let sizes = solve(&self.slots, self.content_len());
        // Extract glyph chars before any mutable put_char borrow.
        let (frame_v, frame_h) = {
            let g = ctx.glyphs();
            (g.frame_v, g.frame_h)
        };
        // `run` = length of the divider line across the cross-axis (local).
        let run = match self.orientation {
            Orientation::Cols => b.b.y - b.a.y,
            Orientation::Rows => b.b.x - b.a.x,
        };
        let mut cursor = 0i32; // local position along the split axis
        for i in 0..self.slots.len().saturating_sub(1) {
            cursor += sizes.get(i).copied().unwrap_or(0);
            let style = self.style_of(i);
            let moving = self.is_moving(i);
            let role = divider_role(moving, self.state().state.active);
            let st = ctx.style(role);
            // Single-line always (match frame COLOR, not weight).
            let (line_glyph, nub_glyph) = match self.orientation {
                Orientation::Cols => (frame_v, frame_v),
                Orientation::Rows => (frame_h, frame_h),
            };
            let draw_full = matches!(style, DividerStyle::Line) || moving;
            let draw_handle = matches!(style, DividerStyle::Handle) && !moving;
            for k in 0..run {
                let (x, y) = match self.orientation {
                    Orientation::Cols => (cursor, k),
                    Orientation::Rows => (k, cursor),
                };
                if draw_full {
                    ctx.put_char(x, y, line_glyph, st);
                } else if draw_handle && k == run / 2 {
                    ctx.put_char(x, y, nub_glyph, st);
                }
                // Hidden / Locked in normal mode: draw nothing (pane background shows).
            }
            cursor += 1; // step over the divider cell
        }
    }

    /// Builder: add a pane (discards the returned id — for static layouts). Chains.
    pub fn pane(mut self, view: Box<dyn View>, c: Constraints) -> Self {
        self.insert(view, c);
        self
    }

    /// Builder: set the default divider style for gaps without an explicit style.
    pub fn default_divider(mut self, style: DividerStyle) -> Self {
        self.default_style = style;
        self.resolve_layout_local();
        self
    }

    /// Builder: set the style of divider `i` (between pane `i` and `i+1`).
    pub fn divider(mut self, i: usize, style: DividerStyle) -> Self {
        self.ensure_divider_len();
        if i < self.divider_styles.len() {
            self.divider_styles[i] = style;
        }
        self
    }

    /// Grow `divider_styles` to cover all current gaps, filling with `default_style`.
    fn ensure_divider_len(&mut self) {
        let want = self.slots.len().saturating_sub(1);
        while self.divider_styles.len() < want {
            self.divider_styles.push(self.default_style);
        }
    }

    /// Compute each child's `Rect` from the solver and apply via `change_bounds`.
    /// Local (no `Context`) — used at insert/build/resize time.
    fn resolve_layout_local(&mut self) {
        let sizes = solve(&self.slots, self.content_len());
        // Children are positioned in the splitter's OWN 0-based local space
        // (Group::draw translates each child by its bounds on top of the
        // splitter's already-translated context), so use the extent, not the
        // owner-space bounds — otherwise an inset splitter shifts every pane by
        // its own origin.
        let ext = self.group.state().get_extent();
        let mut cursor = match self.orientation {
            Orientation::Cols => ext.a.x,
            Orientation::Rows => ext.a.y,
        };
        let ids = self.group.child_ids_in_order();
        for (i, id) in ids.iter().enumerate() {
            let size = sizes.get(i).copied().unwrap_or(0);
            let rect = match self.orientation {
                Orientation::Cols => Rect::new(cursor, ext.a.y, cursor + size, ext.b.y),
                Orientation::Rows => Rect::new(ext.a.x, cursor, ext.b.x, cursor + size),
            };
            if let Some(child) = self.group.find_mut(*id) {
                child.change_bounds(rect);
            }
            cursor += size + 1; // +1 for the divider cell that follows
        }
    }

    /// Local axis coordinate of divider `i` (the gap cell after pane `i`).
    /// `None` if `i` is out of range.
    fn divider_axis_pos(&self, i: usize) -> Option<i32> {
        if i + 1 >= self.slots.len() {
            return None;
        }
        let sizes = solve(&self.slots, self.content_len());
        let mut pos = 0;
        for k in 0..=i {
            pos += sizes.get(k).copied().unwrap_or(0);
            if k < i {
                pos += 1; // earlier divider cells
            }
        }
        Some(pos)
    }

    /// Owner-data-down producer: for each divider whose drawn line abuts a frame
    /// edge, emit a [`JunctionMark`] in `frame_bounds`-local coordinates; recurses
    /// into pane sub-splitters. A pure function of layout (no drawing), but `&mut
    /// self` because reaching a pane child to recurse needs `Group::child_mut`
    /// (the only child accessor `Group` exposes is `&mut`). The owning window
    /// already holds the `&mut Splitter`, so this is free there.
    pub(crate) fn frame_junction_marks(&mut self, frame_bounds: Rect) -> Vec<JunctionMark> {
        if !self.joined {
            return Vec::new();
        }
        let mut out = Vec::new();
        let origin = self.group.state().get_bounds().a; // top splitter: frame-space origin
        self.collect_frame_marks(frame_bounds, origin, &mut out);
        out
    }

    /// Recursive worker for [`frame_junction_marks`]. `origin` is this splitter's
    /// top-left in the frame's coordinate space; children are 0-based local, so a
    /// sub-splitter's frame-space origin is `origin + child_local_bounds.a`.
    fn collect_frame_marks(&mut self, fb: Rect, origin: Point, out: &mut Vec<JunctionMark>) {
        let ext = self.group.state().get_extent();
        let w = ext.b.x - ext.a.x;
        let h = ext.b.y - ext.a.y;
        let sizes = solve(&self.slots, self.content_len());
        let stem = Weight::Single; // dividers are single-line; frame tee weight unchanged at rest
        let fw = fb.b.x - fb.a.x; // frame width
        let fh = fb.b.y - fb.a.y; // frame height

        let mut cursor = 0i32; // splitter-local 0-based axis position
        for i in 0..self.slots.len().saturating_sub(1) {
            cursor += sizes.get(i).copied().unwrap_or(0);
            let local = cursor; // this divider's local axis position
            let draws_full = matches!(self.style_of(i), DividerStyle::Line) || self.is_moving(i);
            if draws_full {
                match self.orientation {
                    Orientation::Cols => {
                        let off = (origin.x - fb.a.x) + local;
                        let interior = off > 0 && off < fw - 1;
                        if interior && origin.y == fb.a.y + 1 {
                            out.push(JunctionMark {
                                edge: Edge::Top,
                                offset: off,
                                stem,
                            });
                        }
                        if interior && origin.y + h == fb.b.y - 1 {
                            out.push(JunctionMark {
                                edge: Edge::Bottom,
                                offset: off,
                                stem,
                            });
                        }
                    }
                    Orientation::Rows => {
                        let off = (origin.y - fb.a.y) + local;
                        let interior = off > 0 && off < fh - 1;
                        if interior && origin.x == fb.a.x + 1 {
                            out.push(JunctionMark {
                                edge: Edge::Left,
                                offset: off,
                                stem,
                            });
                        }
                        if interior && origin.x + w == fb.b.x - 1 {
                            out.push(JunctionMark {
                                edge: Edge::Right,
                                offset: off,
                                stem,
                            });
                        }
                    }
                }
            }
            cursor += 1; // step over the divider cell
        }

        // Recurse: a child's frame-space origin = this splitter's origin + the
        // child's 0-based local position.
        let ids = self.group.child_ids_in_order();
        for id in ids {
            if let Some(sp) = self
                .group
                .child_mut(id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<Splitter>())
            {
                let ca = sp.group.state().get_bounds().a; // 0-based local within self
                let child_origin = Point::new(origin.x + ca.x, origin.y + ca.y);
                sp.collect_frame_marks(fb, child_origin, out);
            }
        }
    }

    /// Hit-test a **local** point to the divider index whose cell it lands on.
    fn divider_at(&self, local: Point) -> Option<usize> {
        let along = match self.orientation {
            Orientation::Cols => local.x,
            Orientation::Rows => local.y,
        };
        (0..self.slots.len().saturating_sub(1)).find(|&i| self.divider_axis_pos(i) == Some(along))
    }

    /// Move divider `i` so its boundary local axis position becomes `target`.
    /// Option A: rewrite only the two flexible neighbors' f64 weights, preserving
    /// their sum; a fixed (`min==max`) neighbor is a hard wall (clamped). Callers
    /// must gate `Locked` at the event layer.
    fn drag_divider_to(&mut self, i: usize, target: i32) {
        if i + 1 >= self.slots.len() {
            return;
        }
        let sizes = solve(&self.slots, self.content_len());
        let (a, b) = (i, i + 1);
        let cur_boundary = self.divider_axis_pos(i).unwrap_or(0);
        let mut delta = target - cur_boundary; // +delta grows pane a, shrinks b
        let (size_a, size_b) = (sizes[a], sizes[b]);
        let max_grow_a = (self.slots[a].max - size_a).max(0);
        let max_shrink_b = (size_b - self.slots[b].min).max(0);
        let max_pos = max_grow_a.min(max_shrink_b);
        let max_shrink_a = (size_a - self.slots[a].min).max(0);
        let max_grow_b = (self.slots[b].max - size_b).max(0);
        let max_neg = max_shrink_a.min(max_grow_b);
        delta = delta.clamp(-max_neg, max_pos);
        if delta == 0 {
            return;
        }
        let new_a = size_a + delta;
        let new_b = size_b - delta;
        let free_a = (new_a - self.slots[a].min).max(0) as f64;
        let free_b = (new_b - self.slots[b].min).max(0) as f64;
        let pair_w = self.slots[a].weight + self.slots[b].weight;
        let free_sum = free_a + free_b;
        if pair_w > 0.0 && free_sum > 0.0 {
            self.slots[a].weight = pair_w * free_a / free_sum;
            self.slots[b].weight = pair_w * free_b / free_sum;
        }
    }

    /// Re-flow children to current solved sizes via DEFERRED bounds (loop owns writes).
    fn request_relayout(&mut self, ctx: &mut Context) {
        let sizes = solve(&self.slots, self.content_len());
        let ext = self.group.state().get_extent();
        let mut cursor = match self.orientation {
            Orientation::Cols => ext.a.x,
            Orientation::Rows => ext.a.y,
        };
        let ids = self.group.child_ids_in_order();
        for (i, id) in ids.iter().enumerate() {
            let size = sizes.get(i).copied().unwrap_or(0);
            let rect = match self.orientation {
                Orientation::Cols => Rect::new(cursor, ext.a.y, cursor + size, ext.b.y),
                Orientation::Rows => Rect::new(ext.a.x, cursor, ext.b.x, cursor + size),
            };
            ctx.request_bounds(*id, rect);
            cursor += size + 1;
        }
    }

    // -- runtime mutators -------------------------------------------------------

    /// Index of the pane with this id, in slot/insertion order.
    fn pane_index(&self, id: ViewId) -> Option<usize> {
        self.group
            .child_ids_in_order()
            .iter()
            .position(|&c| c == id)
    }

    /// Replace a pane's constraints at runtime; re-solves.
    pub fn set_constraints(&mut self, id: ViewId, c: Constraints) {
        if let Some(i) = self.pane_index(id) {
            self.slots[i] = Slot::from_constraints(c);
            self.resolve_layout_local();
        }
    }

    /// Set divider `i`'s style at runtime.
    pub fn set_divider_style(&mut self, i: usize, style: DividerStyle) {
        self.ensure_divider_len();
        if i < self.divider_styles.len() {
            self.divider_styles[i] = style;
        }
    }

    /// Set the default divider style at runtime.
    pub fn set_default_divider_style(&mut self, style: DividerStyle) {
        self.default_style = style;
    }

    /// Remove a pane (and its slot); re-solves. Returns `true` if found.
    pub fn remove(&mut self, id: ViewId) -> bool {
        if let Some(i) = self.pane_index(id) {
            self.group.remove_child_by_id(id);
            self.slots.remove(i);
            if i < self.divider_styles.len() {
                self.divider_styles.remove(i);
            }
            self.resolve_layout_local();
            true
        } else {
            false
        }
    }

    /// Make a (possibly fixed) pane flexible WITHOUT moving any divider: drop its
    /// min/max to (0, `i32::MAX`) and set its weight to the position-preserving
    /// closed form (Σ flexible weights × current_size / current_free).
    pub fn relax(&mut self, id: ViewId) {
        let Some(i) = self.pane_index(id) else {
            return;
        };
        let sizes = solve(&self.slots, self.content_len());
        let cur = sizes.get(i).copied().unwrap_or(0);
        let others: f64 = self
            .slots
            .iter()
            .enumerate()
            .filter(|(k, s)| *k != i && s.weight > 0.0)
            .map(|(_, s)| s.weight)
            .sum();
        let free = self.content_len() - self.slots.iter().map(|s| s.min).sum::<i32>();
        let w = relax_weight(others, cur, free);
        self.slots[i].min = 0;
        self.slots[i].max = i32::MAX;
        self.slots[i].weight = w;
        self.resolve_layout_local();
    }

    // -- keyboard resize session (driven by the window's resize capture via the
    //    SplitterDivider broker; see docs/.../splitter-resize-unification-design.md)

    /// Begin a keyboard resize session: snapshot every slot weight (for Esc
    /// restore) and clear the active-target highlight (the capture sets it via
    /// [`set_active_divider`]). Recurses into pane children that are themselves
    /// splitters so a nested grid resizes too. Returns every movable divider as
    /// `(splitter_id, divider_index, orientation)` in depth-first axis order — this
    /// splitter's dividers first, then each sub-splitter's.
    ///
    /// Precondition: the splitter must already be inserted (have a [`ViewId`]) for
    /// its own dividers to appear in the list — `self.state().id()` is `None` before
    /// insertion, and its own dividers would then be silently omitted (recursion into
    /// inserted sub-splitters is unaffected).
    pub(crate) fn begin_resize_session(&mut self) -> Vec<(ViewId, usize, Orientation)> {
        // Only splitters that contribute a movable divider take a snapshot — they
        // are the only ones that appear in the capture's target list and thus the
        // only ones that receive a matching end_resize_session. A divider-less
        // splitter would otherwise leave a dangling snapshot that is never cleared.
        self.saved_weights = if self.has_movable_divider() {
            self.slots.iter().map(|s| s.weight).collect()
        } else {
            Vec::new()
        };
        self.active_divider = None;
        let mut out = Vec::new();
        if let Some(id) = self.state().id() {
            for i in 0..self.slots.len().saturating_sub(1) {
                if self.style_of(i).movable() {
                    out.push((id, i, self.orientation));
                }
            }
        }
        let ids = self.group.child_ids_in_order();
        for cid in ids {
            if let Some(sub) = self
                .group
                .child_mut(cid)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<Splitter>())
            {
                out.extend(sub.begin_resize_session());
            }
        }
        out
    }

    /// Set (or clear) which divider is the active resize target. Drives the
    /// `FrameDragging` highlight in `draw_dividers`. Per-splitter (not recursive):
    /// the broker addresses each splitter by id.
    pub(crate) fn set_active_divider(&mut self, sel: Option<usize>) {
        self.active_divider = sel;
    }

    /// Move divider `index` by `delta` cells along the split axis, then re-flow
    /// children synchronously (no `ctx` at broker-apply time — `resolve_layout_local`
    /// writes child bounds directly).
    pub(crate) fn nudge_divider(&mut self, index: usize, delta: i32) {
        if let Some(p) = self.divider_axis_pos(index) {
            self.drag_divider_to(index, p + delta);
        }
        self.resolve_layout_local();
    }

    /// End the resize session. On `!commit` restore the snapshotted weights (Esc).
    /// Clears the highlight and the snapshot. Per-splitter (not recursive).
    pub(crate) fn end_resize_session(&mut self, commit: bool) {
        if !commit && self.saved_weights.len() == self.slots.len() {
            for (s, w) in self.slots.iter_mut().zip(&self.saved_weights) {
                s.weight = *w;
            }
        }
        self.active_divider = None;
        self.saved_weights.clear();
        self.resolve_layout_local();
    }

    /// True if this splitter has ≥1 movable divider (used by the window to decide
    /// whether to enable Command::RESIZE / offer divider targets).
    pub(crate) fn has_movable_divider(&self) -> bool {
        (0..self.slots.len().saturating_sub(1)).any(|i| self.style_of(i).movable())
    }

    // -- interior crossings (Site 2) --------------------------------------------

    /// Site 2 (tvision-rs-original): overlay `├`/`┤`/`┴`/`┬` on this splitter's own
    /// divider cells where a perpendicular pane sub-splitter's divider meets them.
    /// `&mut self` because reaching a pane child to read its divider positions
    /// needs `Group::child_mut` (the `&self draw_dividers` cannot do this). Reads
    /// the child's positions into owned data (borrow released) before drawing on
    /// this splitter's own cells via `ctx`.
    ///
    /// Scope: each outer divider is joined from the pane(s) it borders. The common
    /// grid (one perpendicular sub-splitter per outer divider) renders correctly.
    /// Two ADJACENT perpendicular sub-splitters whose dividers coincide on the same
    /// shared outer-divider cell would need a `┼` (the [`Junction::Cross`] glyph
    /// exists for it); that topology is not composed here — the last-written tee
    /// wins — and is left for a future extension. Mixed-weight crossings are
    /// likewise out of scope (the glyph uses the outer divider's weight).
    fn draw_interior_crossings(&mut self, ctx: &mut DrawCtx) {
        if self.slots.len() < 2 {
            return; // no dividers of our own → nothing to cross
        }
        let weight = Weight::Single;
        let ids = self.group.child_ids_in_order();
        for (p, id) in ids.iter().enumerate() {
            // Owned (sub bounds, perpendicular divider local positions) or None.
            let info = self.group.child_mut(*id).and_then(|v| {
                v.as_any_mut()
                    .and_then(|a| a.downcast_mut::<Splitter>())
                    .filter(|sub| sub.orientation != self.orientation)
                    .map(|sub| {
                        let cb = sub.group.state().get_bounds();
                        let csizes = solve(&sub.slots, sub.content_len());
                        let mut pos = Vec::new();
                        let mut c = 0i32;
                        for i in 0..sub.slots.len().saturating_sub(1) {
                            c += csizes.get(i).copied().unwrap_or(0);
                            let full =
                                matches!(sub.style_of(i), DividerStyle::Line) || sub.is_moving(i);
                            if full {
                                pos.push(c);
                            }
                            c += 1;
                        }
                        (cb, pos)
                    })
            });
            let Some((cb, perp)) = info else { continue };

            let low = if p > 0 {
                self.divider_axis_pos(p - 1)
            } else {
                None
            };
            let high = if p < self.slots.len() - 1 {
                self.divider_axis_pos(p)
            } else {
                None
            };

            for d in &perp {
                let (cross_local, branch_low, branch_high) = match self.orientation {
                    // Cols outer: vertical dividers; sub is Rows (horizontal). The
                    // crossing's cross-axis is the ROW.
                    Orientation::Cols => {
                        let row = cb.a.y + d;
                        (row, Junction::TeeLeft, Junction::TeeRight)
                    }
                    // Rows outer: horizontal dividers; sub is Cols (vertical). The
                    // crossing's cross-axis is the COLUMN.
                    Orientation::Rows => {
                        let col = cb.a.x + d;
                        (col, Junction::TeeUp, Junction::TeeDown)
                    }
                };
                // Sub on the HIGH side of the low divider → branch toward high.
                if let Some(ld) = low {
                    let glyph = divider_junction(branch_high, weight, ctx.glyphs());
                    self.put_crossing(ctx, ld, cross_local, glyph);
                }
                // Sub on the LOW side of the high divider → branch toward low.
                if let Some(hd) = high {
                    let glyph = divider_junction(branch_low, weight, ctx.glyphs());
                    self.put_crossing(ctx, hd, cross_local, glyph);
                }
            }
        }
    }

    /// Overlay one crossing glyph at (outer-divider axis pos, cross-axis pos),
    /// mapped to (x, y) by orientation. Local 0-based coords (same as `draw_dividers`).
    fn put_crossing(&self, ctx: &mut DrawCtx, axis: i32, cross: i32, glyph: char) {
        // Crossings are never the moved target, so moving=false.
        let role = divider_role(false, self.state().state.active);
        let st = ctx.style(role);
        let (x, y) = match self.orientation {
            Orientation::Cols => (axis, cross), // vertical divider: column=axis, row=cross
            Orientation::Rows => (cross, axis), // horizontal divider: row=axis, column=cross
        };
        ctx.put_char(x, y, glyph, st);
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
        self.draw_dividers(ctx);
        if self.joined {
            self.draw_interior_crossings(ctx);
        }
    }

    fn change_bounds(&mut self, bounds: Rect) {
        self.group.state_mut().set_bounds(bounds);
        self.resolve_layout_local();
    }

    /// Downcast seam: a parent (the owning window, or an outer splitter reaching a
    /// pane sub-splitter) reaches this `Splitter` concretely via `child_mut` +
    /// `as_any_mut` + `downcast_mut::<Splitter>()` — the same mechanism a window
    /// uses to push data to its `Frame`. The `#[delegate(to = group)]` macro would
    /// otherwise forward this to the inner `Group` (which returns `None`), so the
    /// override body here is required; the macro auto-excludes it from forwarding.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        match ev {
            Event::KeyDown(_) => {
                // No splitter-owned keys. Tab focus traversal (including descending
                // into and cycling among the panes) is handled by the hierarchical
                // pass in `Group::handle_event`, so the splitter is transparent to
                // focus here. Divider resize is driven by the window's resize
                // capture (Command::RESIZE), not a raw KeyDown.
                self.group.handle_event(ev, ctx);
            }
            Event::MouseDown(me) => {
                let local = me.position; // already view-local; copy before ev.clear()
                if let Some(i) = self.divider_at(local) {
                    let style = self.style_of(i);
                    // Any non-Locked divider is mouse-draggable (incl. Hidden, which
                    // becomes visible in FrameDragging while dragged) — deliberate per spec.
                    let allowed = style.movable();
                    if let (true, Some(id)) = (allowed, self.state().id()) {
                        self.dragging = Some(i);
                        ctx.start_mouse_track(
                            id,
                            self.abs_origin,
                            TrackMask {
                                mouse_move: true,
                                ..Default::default()
                            },
                        );
                        ev.clear();
                        return;
                    }
                }
                self.group.handle_event(ev, ctx);
            }
            Event::MouseMove(me) if self.dragging.is_some() => {
                let i = self.dragging.unwrap();
                let target = match self.orientation {
                    Orientation::Cols => me.position.x,
                    Orientation::Rows => me.position.y,
                };
                self.drag_divider_to(i, target);
                self.request_relayout(ctx);
                ev.clear();
            }
            Event::MouseUp(_) if self.dragging.is_some() => {
                self.dragging = None;
                ev.clear();
            }
            _ => self.group.handle_event(ev, ctx),
        }
    }
}

#[cfg(test)]
mod divider_tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::screen::Buffer;
    use crate::theme::{Role, Theme};

    // Minimal fill view for snapshot tests.
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
            let b = self.1.get_bounds();
            let (w, h) = (b.b.x - b.a.x, b.b.y - b.a.y);
            ctx.fill(Rect::new(0, 0, w, h), self.0, ctx.style(Role::Normal));
        }
    }

    fn render_splitter(sp: &mut Splitter, w: u16, h: u16) -> String {
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
    fn splitter_grows_with_owner_by_default() {
        let sp = Splitter::cols();
        let gm = sp.state().grow_mode;
        assert!(gm.hi_x && gm.hi_y, "splitter fills/grows by default");
        assert!(!gm.lo_x && !gm.lo_y, "top-left stays (keeps any inset)");
        // rows() too
        let sp2 = Splitter::rows();
        assert!(sp2.state().grow_mode.hi_x && sp2.state().grow_mode.hi_y);
    }

    #[test]
    fn with_grow_mode_can_disable_growth() {
        let sp = Splitter::cols().with_grow_mode(GrowMode::default());
        let gm = sp.state().grow_mode;
        assert!(
            !gm.hi_x && !gm.hi_y && !gm.lo_x && !gm.lo_y,
            "fixed-size opt-out"
        );
    }

    #[test]
    fn draggability_matrix() {
        assert!(DividerStyle::Line.draggable_live());
        assert!(DividerStyle::Handle.draggable_live());
        assert!(!DividerStyle::Hidden.draggable_live());
        assert!(!DividerStyle::Locked.draggable_live());

        assert!(DividerStyle::Hidden.movable());
        assert!(!DividerStyle::Locked.movable());
    }

    // Minimal view used by resize-session tests (no rendering needed here).
    struct Leaf(ViewState);
    impl Leaf {
        fn boxed() -> Box<dyn View> {
            Box::new(Leaf(ViewState::new(Rect::new(0, 0, 1, 1))))
        }
    }
    impl View for Leaf {
        fn state(&self) -> &ViewState {
            &self.0
        }
        fn state_mut(&mut self) -> &mut ViewState {
            &mut self.0
        }
        fn draw(&mut self, _ctx: &mut DrawCtx) {}
    }

    /// Three-pane column splitter laid out in a 30×5 box (2 dividers → 28 content
    /// cols → the largest-remainder solver gives the extra cell to the first pane,
    /// e.g. [10, 9, 9]). `divider_axis_pos` is well-defined after this.
    fn three_pane_cols() -> Splitter {
        let mut sp = Splitter::cols();
        sp.change_bounds(Rect::new(0, 0, 30, 5));
        sp.insert(Fill::boxed('A'), Constraints::flex());
        sp.insert(Fill::boxed('B'), Constraints::flex());
        sp.insert(Fill::boxed('C'), Constraints::flex());
        // Give the splitter a stable ViewId so begin_resize_session can include it.
        sp.state_mut().id = Some(ViewId::next());
        sp
    }

    #[test]
    fn begin_resize_session_lists_movable_dividers_and_snapshots() {
        // Three panes → two dividers (indices 0 and 1), both movable (default Line style).
        let mut sp = three_pane_cols(); // helper below; 3 panes → dividers 0 and 1
        let targets = sp.begin_resize_session();
        let id = sp.state().id().unwrap();
        assert_eq!(
            targets,
            vec![(id, 0, Orientation::Cols), (id, 1, Orientation::Cols)],
            "movable dividers enumerated in axis order with this splitter's id"
        );
        assert_eq!(
            sp.active_divider, None,
            "begin does NOT auto-select a divider"
        );
        assert_eq!(
            sp.saved_weights.len(),
            sp.slots.len(),
            "weights snapshotted"
        );
    }

    #[test]
    fn nudge_divider_moves_then_end_commit_keeps_position() {
        let mut sp = three_pane_cols();
        sp.begin_resize_session();
        let before = sp.divider_axis_pos(0).unwrap();
        sp.nudge_divider(0, 1);
        let after = sp.divider_axis_pos(0).unwrap();
        assert_eq!(
            after,
            before + 1,
            "nudge moves the divider one cell along the axis"
        );
        sp.end_resize_session(true); // commit
        assert_eq!(
            sp.divider_axis_pos(0).unwrap(),
            after,
            "commit keeps the new position"
        );
        assert!(sp.saved_weights.is_empty(), "session ended");
        assert_eq!(sp.active_divider, None);
    }

    #[test]
    fn end_resize_session_cancel_restores_weights() {
        let mut sp = three_pane_cols();
        sp.begin_resize_session();
        let before = sp.divider_axis_pos(0).unwrap();
        sp.nudge_divider(0, 2);
        assert_ne!(sp.divider_axis_pos(0).unwrap(), before);
        sp.end_resize_session(false); // Esc / cancel
        assert_eq!(
            sp.divider_axis_pos(0).unwrap(),
            before,
            "cancel restores pre-session position"
        );
    }

    #[test]
    fn set_active_divider_drives_active_field() {
        let mut sp = three_pane_cols();
        sp.begin_resize_session();
        sp.set_active_divider(Some(1));
        assert_eq!(sp.active_divider, Some(1));
        sp.set_active_divider(None);
        assert_eq!(sp.active_divider, None);
    }

    #[test]
    fn divider_role_rule() {
        use crate::theme::Role;
        // moving wins over everything.
        assert_eq!(divider_role(true, true), Role::FrameDragging);
        assert_eq!(divider_role(true, false), Role::FrameDragging);
        // not moving: active window -> FrameActive, else FramePassive.
        assert_eq!(divider_role(false, true), Role::FrameActive);
        assert_eq!(divider_role(false, false), Role::FramePassive);
    }

    #[test]
    fn divider_inactive_is_single_line_passive() {
        let mut sp = three_pane_cols(); // default state: not active
        let snap = render_splitter(&mut sp, 30, 5);
        insta::assert_snapshot!("divider_inactive", snap);
    }

    #[test]
    fn divider_dragging_highlight_on_mouse() {
        let mut sp = three_pane_cols();
        sp.dragging = Some(0); // simulate mid-drag (plain field, no ctx needed)
        let snap = render_splitter(&mut sp, 30, 5);
        insta::assert_snapshot!("divider_dragging_highlight_on_mouse", snap);
    }

    #[test]
    fn divider_active_is_single_line_active() {
        let mut sp = three_pane_cols();
        sp.state_mut().state.active = true; // owning window is active
        let snap = render_splitter(&mut sp, 30, 5);
        insta::assert_snapshot!("divider_active", snap);
    }

    #[test]
    fn begin_resize_session_recurses_into_sub_splitter() {
        // OUTER cols splitter (1 divider) with a SUB rows splitter (1 divider) as
        // its second pane. begin_resize_session enumerates the outer's dividers
        // first, then recurses depth-first into the sub.
        let sub = Splitter::rows()
            .pane(Leaf::boxed(), Constraints::flex())
            .pane(Leaf::boxed(), Constraints::flex());
        let mut outer = Splitter::cols();
        outer.change_bounds(Rect::new(0, 0, 30, 10));
        outer.insert(Leaf::boxed(), Constraints::flex());
        // insert() stamps the child's own ViewId, so the sub-splitter is addressable.
        let sub_id = outer.insert(Box::new(sub), Constraints::flex());
        // The outer splitter needs its own id for its dividers to be listed.
        outer.state_mut().id = Some(ViewId::next());
        let outer_id = outer.state().id().unwrap();

        let targets = outer.begin_resize_session();
        assert_eq!(
            targets,
            vec![
                (outer_id, 0, Orientation::Cols), // outer's divider first
                (sub_id, 0, Orientation::Rows),   // then the sub's, depth-first
            ],
            "depth-first: outer's dividers (outer id/orientation) then the sub's (sub id/orientation)"
        );

        // The recursion must have begun the sub's session too (its weights snapshotted).
        let sub = outer
            .group
            .child_mut(sub_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<Splitter>())
            .expect("sub pane is a Splitter");
        assert_eq!(
            sub.saved_weights.len(),
            sub.slots.len(),
            "sub-splitter's weights snapshotted by the recursive begin"
        );
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

    fn render_buf(sp: &mut Splitter, w: u16, h: u16) -> crate::screen::Buffer {
        let theme = crate::theme::Theme::classic_blue();
        let mut buf = crate::screen::Buffer::new(w, h);
        let bounds = sp.state().get_bounds();
        let mut dc = DrawCtx::new(&mut buf, &theme, bounds, bounds.a);
        sp.draw(&mut dc);
        buf
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

    #[test]
    fn dividers_line_style() {
        let mut sp = Splitter::cols(); // default_style == Line
        sp.change_bounds(Rect::new(0, 0, 13, 3)); // 2 panes => 1 divider, 12 content => 6/6
        sp.insert(Fill::boxed('A'), Constraints::flex());
        sp.insert(Fill::boxed('B'), Constraints::flex());
        insta::assert_snapshot!(render(&mut sp, 13, 3)); // AAAAAA│BBBBBB ×3 rows
    }

    #[test]
    fn dividers_hidden_style_is_seamless() {
        let mut sp = Splitter::cols();
        sp.default_style = DividerStyle::Hidden;
        sp.change_bounds(Rect::new(0, 0, 13, 3));
        sp.insert(Fill::boxed('A'), Constraints::flex());
        sp.insert(Fill::boxed('B'), Constraints::flex());
        insta::assert_snapshot!(render(&mut sp, 13, 3)); // AAAAAA BBBBBB (blank gap)
    }

    #[test]
    fn dividers_rows_orientation() {
        let mut sp = Splitter::rows();
        sp.change_bounds(Rect::new(0, 0, 6, 7)); // 2 panes => 1 divider, 6 content rows => 3/3
        sp.insert(Fill::boxed('A'), Constraints::flex());
        sp.insert(Fill::boxed('B'), Constraints::flex());
        insta::assert_snapshot!(render(&mut sp, 6, 7)); // AAA rows, ────── divider row, BBB rows
    }

    #[test]
    fn builder_builds_same_layout_as_imperative() {
        let mut imperative = Splitter::cols();
        imperative.change_bounds(Rect::new(0, 0, 13, 2));
        imperative.insert(Fill::boxed('A'), Constraints::flex());
        imperative.insert(Fill::boxed('B'), Constraints::flex());

        let mut built = Splitter::cols()
            .pane(Fill::boxed('A'), Constraints::flex())
            .pane(Fill::boxed('B'), Constraints::flex());
        built.change_bounds(Rect::new(0, 0, 13, 2));

        assert_eq!(render(&mut imperative, 13, 2), render(&mut built, 13, 2));
    }

    #[test]
    fn per_divider_override_renders() {
        let mut sp = Splitter::cols()
            .default_divider(DividerStyle::Hidden)
            .pane(Fill::boxed('A'), Constraints::flex())
            .pane(Fill::boxed('B'), Constraints::flex())
            .pane(Fill::boxed('C'), Constraints::flex())
            .divider(1, DividerStyle::Line); // seam after pane 1 (B|C) visible; others hidden
        sp.change_bounds(Rect::new(0, 0, 14, 1)); // 3 panes, 2 dividers, 12 content => 4/4/4
        insta::assert_snapshot!(render(&mut sp, 14, 1)); // AAAA BBBB│CCCC
    }

    #[test]
    fn drag_repartitions_two_neighbors_sum_preserved() {
        let mut sp = Splitter::cols();
        sp.change_bounds(Rect::new(0, 0, 31, 1)); // 3 panes, 2 dividers, 29 content
        sp.insert(Fill::boxed('A'), Constraints::flex());
        sp.insert(Fill::boxed('B'), Constraints::flex());
        sp.insert(Fill::boxed('C'), Constraints::flex());
        let w_before_c = sp.slots[2].weight;
        let pair_before = sp.slots[0].weight + sp.slots[1].weight;
        let d0 = sp.divider_axis_pos(0).unwrap();
        sp.drag_divider_to(0, d0 + 3);
        assert!(
            (sp.slots[2].weight - w_before_c).abs() < 1e-9,
            "pane C weight untouched (locality)"
        );
        assert!(
            (sp.slots[0].weight + sp.slots[1].weight - pair_before).abs() < 1e-9,
            "pair sum preserved"
        );
        let sizes = super::layout::solve(&sp.slots, sp.content_len());
        assert!(sizes[0] > sizes[1], "pane 0 grew, pane 1 shrank");
    }

    #[test]
    fn drag_against_fixed_neighbor_is_a_wall() {
        let mut sp = Splitter::cols();
        sp.change_bounds(Rect::new(0, 0, 31, 1));
        sp.insert(Fill::boxed('A'), Constraints::fixed(10)); // pinned
        sp.insert(Fill::boxed('B'), Constraints::flex());
        let before = super::layout::solve(&sp.slots, sp.content_len());
        let d0 = sp.divider_axis_pos(0).unwrap();
        sp.drag_divider_to(0, d0 + 5); // try to grow the fixed pane
        let after = super::layout::solve(&sp.slots, sp.content_len());
        assert_eq!(
            before, after,
            "fixed pane is immovable — divider does not move"
        );
    }

    #[test]
    fn relax_does_not_move_dividers() {
        let mut sp = Splitter::cols();
        sp.change_bounds(Rect::new(0, 0, 41, 1)); // 3 panes, 2 dividers, 39 content
        let a = sp.insert(Fill::boxed('A'), Constraints::fixed(12));
        sp.insert(Fill::boxed('B'), Constraints::flex());
        sp.insert(Fill::boxed('C'), Constraints::flex());
        let before = solve(&sp.slots, sp.content_len());
        sp.relax(a);
        let after = solve(&sp.slots, sp.content_len());
        assert_eq!(before, after, "relax keeps every pane the same size");
        assert!(
            sp.slots[0].min != sp.slots[0].max,
            "pane A is now draggable (not fixed)"
        );
    }

    #[test]
    fn remove_pane_resolves_remaining() {
        let mut sp = Splitter::cols();
        sp.change_bounds(Rect::new(0, 0, 21, 1)); // after remove: 2 panes, 1 divider, 20 content
        let a = sp.insert(Fill::boxed('A'), Constraints::flex());
        sp.insert(Fill::boxed('B'), Constraints::flex());
        sp.insert(Fill::boxed('C'), Constraints::flex());
        assert!(sp.remove(a));
        let sizes = solve(&sp.slots, sp.content_len());
        assert_eq!(sizes.len(), 2);
        assert_eq!(sizes.iter().sum::<i32>(), sp.content_len());
    }

    #[test]
    fn frame_marks_two_pane_cols_abut_top_and_bottom() {
        use crate::junction::{Edge, Weight};
        let frame_bounds = Rect::new(0, 0, 13, 5);
        let mut sp = Splitter::cols();
        sp.set_joined(true);
        sp.change_bounds(Rect::new(1, 1, 12, 4));
        sp.insert(Fill::boxed('A'), Constraints::flex());
        sp.insert(Fill::boxed('B'), Constraints::flex());
        let marks = sp.frame_junction_marks(frame_bounds);
        assert_eq!(marks.len(), 2, "one top + one bottom mark");
        assert!(marks.contains(&crate::junction::JunctionMark {
            edge: Edge::Top,
            offset: 6,
            stem: Weight::Single
        }));
        assert!(marks.contains(&crate::junction::JunctionMark {
            edge: Edge::Bottom,
            offset: 6,
            stem: Weight::Single
        }));
    }

    #[test]
    fn frame_marks_handle_divider_emits_nothing() {
        let frame_bounds = Rect::new(0, 0, 13, 5);
        let mut sp = Splitter::cols()
            .default_divider(DividerStyle::Handle)
            .joined();
        sp.change_bounds(Rect::new(1, 1, 12, 4));
        sp.insert(Fill::boxed('A'), Constraints::flex());
        sp.insert(Fill::boxed('B'), Constraints::flex());
        // Joined, but a Handle divider draws no full line, so it abuts nothing.
        assert!(sp.frame_junction_marks(frame_bounds).is_empty());
    }

    #[test]
    fn frame_marks_inset_splitter_emits_nothing() {
        let frame_bounds = Rect::new(0, 0, 13, 7);
        let mut sp = Splitter::cols().joined();
        sp.change_bounds(Rect::new(2, 2, 11, 5)); // not adjacent to any frame edge
        sp.insert(Fill::boxed('A'), Constraints::flex());
        sp.insert(Fill::boxed('B'), Constraints::flex());
        // Joined, but the splitter is inset from every frame edge, so no abutment.
        assert!(sp.frame_junction_marks(frame_bounds).is_empty());
    }

    #[test]
    fn frame_marks_empty_when_not_joined() {
        let frame_bounds = Rect::new(0, 0, 13, 5);
        let mut sp = Splitter::cols(); // NOT joined
        sp.change_bounds(Rect::new(1, 1, 12, 4));
        sp.insert(Fill::boxed('A'), Constraints::flex());
        sp.insert(Fill::boxed('B'), Constraints::flex());
        assert!(
            sp.frame_junction_marks(frame_bounds).is_empty(),
            "no marks unless joined"
        );
    }

    #[test]
    fn frame_marks_nested_grid_inner_divider_hits_right_frame() {
        use crate::junction::Edge;
        let frame_bounds = Rect::new(0, 0, 22, 7);
        let inner = Splitter::rows()
            .pane(Fill::boxed('L'), Constraints::flex())
            .pane(Fill::boxed('F'), Constraints::flex());
        let mut outer = Splitter::cols();
        outer.set_joined(true); // OUTER only — propagation joins the inner sub-splitter
        outer.change_bounds(Rect::new(1, 1, 21, 6));
        outer.insert(Fill::boxed('T'), Constraints::fixed(8));
        outer.insert(Box::new(inner), Constraints::flex());
        let marks = outer.frame_junction_marks(frame_bounds);
        assert_eq!(
            marks.len(),
            3,
            "outer top+bottom + inner right, got {marks:?}"
        );
        assert!(
            marks.iter().any(|m| m.edge == Edge::Right && m.offset == 3),
            "inner horizontal divider abuts the right frame edge at offset 3, got {marks:?}"
        );
        assert_eq!(
            marks.iter().filter(|m| m.edge == Edge::Top).count(),
            1,
            "outer vertical divider abuts the top edge once"
        );
        assert!(
            !marks.iter().any(|m| m.edge == Edge::Left),
            "no spurious left mark, got {marks:?}"
        );
    }

    #[test]
    fn interior_crossing_grid_renders_left_tee() {
        // `├` is Junction::TeeRight (vertical bar, branch pointing RIGHT into the
        // inner pane); "left tee" in the name refers to the glyph's open-left shape.
        // Outer cols: [tree(fixed 6) | inner-rows(flex)]. The inner rows splitter
        // has a horizontal divider; where it meets the outer vertical divider, the
        // outer divider cell must show ├ (a vertical line branching right into the
        // inner pane), not a plain │.
        let inner = Splitter::rows()
            .pane(Fill::boxed('L'), Constraints::flex())
            .pane(Fill::boxed('F'), Constraints::flex());
        let mut outer = Splitter::cols();
        outer.set_joined(true); // join the linework so the crossing tee renders
        outer.change_bounds(Rect::new(0, 0, 20, 7));
        outer.insert(Fill::boxed('T'), Constraints::fixed(6));
        outer.insert(Box::new(inner), Constraints::flex());
        insta::assert_snapshot!(render(&mut outer, 20, 7));
    }

    #[test]
    fn interior_crossings_absent_when_not_joined() {
        // Same grid as interior_crossing_grid_renders_left_tee but NOT joined:
        // the outer divider column stays plain │ (no ├).
        let inner = Splitter::rows()
            .pane(Fill::boxed('L'), Constraints::flex())
            .pane(Fill::boxed('F'), Constraints::flex());
        let mut outer = Splitter::cols(); // NOT joined
        outer.change_bounds(Rect::new(0, 0, 20, 7));
        outer.insert(Fill::boxed('T'), Constraints::fixed(6));
        outer.insert(Box::new(inner), Constraints::flex());
        let out = render(&mut outer, 20, 7);
        assert!(!out.contains('├'), "no interior tee unless joined:\n{out}");
    }

    #[test]
    fn inset_splitter_panes_fill_from_origin() {
        // Regression: an inset splitter (non-zero origin) must lay its panes out in
        // 0-based local space, so pane content begins exactly at the splitter's
        // origin column — NOT shifted right by the origin. Before the fix, pane A
        // started at column 4 (origin.x + 1), leaving column 3 blank.
        let mut sp = Splitter::cols();
        sp.change_bounds(Rect::new(3, 2, 16, 5)); // origin (3,2), width 13, height 3
        sp.insert(Fill::boxed('A'), Constraints::flex());
        sp.insert(Fill::boxed('B'), Constraints::flex());
        let buf = render_buf(&mut sp, 16, 5);
        // First pane content fills from the splitter's own origin column (3),
        // on a middle content row (y=3, inside [2,5)).
        assert_eq!(
            buf.get(3, 3).symbol(),
            "A",
            "pane A must start at the splitter origin column, not be shifted by it"
        );
        // And there is no blank gap immediately inside the origin.
        assert_ne!(
            buf.get(3, 3).symbol(),
            " ",
            "no blank column at the splitter origin"
        );
    }

    #[test]
    fn splitter_downcasts_through_as_any_mut() {
        let mut sp = Splitter::cols();
        sp.change_bounds(Rect::new(0, 0, 13, 3));
        sp.insert(Fill::boxed('A'), Constraints::flex());
        sp.insert(Fill::boxed('B'), Constraints::flex());
        let resolved = (&mut sp as &mut dyn View)
            .as_any_mut()
            .and_then(|a| a.downcast_mut::<Splitter>())
            .is_some();
        assert!(resolved, "Splitter must override as_any_mut → Some(self)");
    }
}
