//! `TOutlineViewer` / `TOutline` — faithful Rust port of `toutline.cpp`
//! (rows 88–90: `TNode`, `TOutlineViewer`, `TOutline`).
//!
//! ## D-A: a TRAIT, not a concrete struct (the `ListViewer` shape, not `Scroller`)
//!
//! `TOutlineViewer` extends `TScroller`, but its abstract virtuals
//! (`getRoot`/`getNext`/`getChild`/`getText`/`isExpanded`/`hasChildren`/`adjust`)
//! are called from inside the base's own `draw`/`handleEvent`/`update` — exactly
//! the same constraint that made [`ListViewer`](crate::widgets::list_viewer::ListViewer)
//! a trait rather than a `Scroller`-style embedded struct. So this row follows the
//! **ListViewer trait pattern**: [`OutlineViewer`] carries the overridable
//! virtuals, [`OutlineViewerState`] carries the non-virtual data members, and the
//! shared draw / event / traversal logic lives as **free functions generic over
//! `<L: OutlineViewer + ?Sized>`** so a concrete widget's `View` impl reuses them
//! verbatim.
//!
//! ## The cross-view scrollbar read-sync (D3)
//!
//! Like the scroller and the list viewer, an outline viewer holds only `&mut
//! Context` during dispatch (D3) and so can neither read nor mutate its
//! window-frame sibling scrollbars. The pump is the broker: on a
//! `cmScrollBarChanged` broadcast naming one of its bars as `source`, the viewer
//! requests [`Deferred::SyncOutlineViewerDelta`](crate::view::Deferred::SyncOutlineViewerDelta);
//! the pump reads both bars' `value`s and writes the resulting `(dx, dy)` into the
//! viewer's [`delta`](OutlineViewerState::delta) (a downcast to [`Outline`], like
//! the scroller). This read-sync is **read-only** (no editor cursor / focus
//! write-back), so it terminates like the scroller's, no change-guard needed.
//!
//! ## Drops / deferrals (faithful, breadcrumbed)
//!
//! - **D12:** `TNode`'s and `TOutline`'s `write`/`read`/`build`/`readNode`/
//!   `writeNode`/`streamableName`/`name` (TStreamable) dropped. `TNode`'s
//!   destructor (`disposeNode`'s recursion) is Rust's automatic `Box<Node>` drop.
//! - **D8:** every `drawView()` call site dropped (the whole tree redraws + diffs
//!   each pass).
//! - **getPalette → Theme roles** (D7): `cpOutlineViewer "\x6\x7\x3\x8"` →
//!   [`Role::OutlineNormal`] / [`Role::OutlineFocused`] / [`Role::OutlineSelected`]
//!   / [`Role::OutlineNotExpanded`].
//! - **mouse press-and-hold / auto-scroll drag loop** — **landed** (row 31, D9 adoption):
//!   `MouseDown` arms the A3 `MouseTrackCapture`; `MouseMove`/`MouseAuto`/`MouseUp` route
//!   the hold loop faithfully (out-of-view auto-scroll `mouseAutoToSkip = 3`; `dragged`
//!   gate distinguishes click from drag for the graph-toggle post-loop).
//! - **ctor `update()`**: `TOutline`'s ctor calls `update()`, which needs a
//!   `Context` (to publish scrollbar params) we do not have at construction. The
//!   consumer must call [`ov_update`] once after inserting the outline into a group
//!   (the same constraint the scroller / list-viewer ctors hit — see
//!   [`Outline::new`]).

use crate::capture::TrackMask;
use crate::command::Command;
use crate::event::{Event, Key, ctrl_to_arrow};
use crate::theme::Role;
use crate::view::{
    Context, DrawCtx, GrowMode, Options, Point, Rect, StateFlag, View, ViewId, ViewState,
};

/// `ovExpanded` — the node is drawn as expanded (no children, or expanded).
const OV_EXPANDED: u16 = 0x01;
/// `ovChildren` — the node has children AND is expanded (draw the child-link).
const OV_CHILDREN: u16 = 0x02;
/// `ovLast` — the node is the last child of its parent (└ vs ├).
const OV_LAST: u16 = 0x04;

/// `mouseAutoToSkip = 3` — number of `evMouseAuto` ticks to accumulate before
/// stepping the focus by ±1 when the mouse is outside the view (toutline.cpp:421).
const MOUSE_AUTO_TO_SKIP: i32 = 3;

/// Per-hold mouse-tracking state for the outline viewer (the D9 successor of the
/// C++ locals `count` and `dragged`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct OvTrack {
    /// `count` — accumulated `evMouseAuto` ticks since the last step/reset.
    count: i32,
    /// `dragged` — iteration counter; capped at 2 (C++: `if (dragged < 2) dragged++`).
    /// After the loop, `dragged < 2` distinguishes a "click" from a "drag": only
    /// a click (dragged < 2) can toggle the graph expansion column.
    dragged: u8,
}

// ---------------------------------------------------------------------------
// TNode (row 88) — the tree node
// ---------------------------------------------------------------------------

/// `TNode` — one outline tree node (row 88).
///
/// A node owns its first child (`child_list`) and its next sibling (`next`), both
/// as `Option<Box<Node>>`; the recursive `Box` drop is the faithful successor to
/// C++ `disposeNode` (which recurses into `childList` then `next` then deletes the
/// node). `text` is the displayed label; `expanded` is the collapse state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The node's displayed text (`TNode::text`).
    pub text: String,
    /// The first child, or `None` (`TNode::childList`). Siblings chain via `next`.
    pub child_list: Option<Box<Node>>,
    /// The next sibling, or `None` (`TNode::next`).
    pub next: Option<Box<Node>>,
    /// Whether the node is expanded (`TNode::expanded`). New nodes default to
    /// expanded, faithful to the C++ one-arg ctor (`expanded(True)`).
    pub expanded: bool,
}

impl Node {
    /// `TNode(aText)` — a leaf node, expanded, with no children or siblings.
    pub fn new(text: impl Into<String>) -> Self {
        Node {
            text: text.into(),
            child_list: None,
            next: None,
            expanded: true,
        }
    }

    /// Builder: set the next sibling.
    pub fn with_next(mut self, next: Box<Node>) -> Self {
        self.next = Some(next);
        self
    }

    /// Builder: set the first child (siblings chain via `next` on the children).
    pub fn with_children(mut self, children: Box<Node>) -> Self {
        self.child_list = Some(children);
        self
    }

    /// Builder: set the initial expanded state (`TNode`'s three-arg ctor's
    /// `initialState`).
    pub fn with_expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }
}

// ---------------------------------------------------------------------------
// OutlineViewerState — the non-virtual data members (row 89)
// ---------------------------------------------------------------------------

/// The shared state of every outline viewer — `TOutlineViewer`'s (and its
/// `TScroller` base's) non-virtual data members. A concrete outline widget embeds
/// one and exposes it via [`OutlineViewer::ov`]/[`OutlineViewer::ov_mut`].
pub struct OutlineViewerState {
    /// View state (geometry, flags, …) — the D2 `View` composition target.
    pub state: ViewState,
    /// `TScroller::delta` — the scroll offset (x = horizontal char skip, y = the
    /// first visible DFS position). Refreshed from the bars by the read-sync.
    pub delta: Point,
    /// `TScroller::limit` — the content extent (x = max graph+text width, y = the
    /// visible node count). Set by [`set_limit`](Self::set_limit).
    pub limit: Point,
    /// The horizontal scrollbar, by id (`None` if absent). `TScroller::hScrollBar`.
    pub h_scroll_bar: Option<ViewId>,
    /// The vertical scrollbar, by id (`None` if absent). `TScroller::vScrollBar`.
    pub v_scroll_bar: Option<ViewId>,
    /// `TOutlineViewer::foc` — the focused item's DFS position (0-based).
    pub foc: i32,
    /// Absolute screen position of this view's `(0, 0)`, cached by the last
    /// `draw` call — feeds the [`MouseTrackCapture`] origin (D9/A3 seam).
    pub(crate) abs_origin: Point,
    /// Per-hold mouse-tracking state — `Some` while a track is in flight
    /// (between `MouseDown` and `MouseUp`), `None` otherwise. Guards the
    /// tracking arms against stray (untracked) events.
    pub(crate) track: Option<OvTrack>,
}

impl OutlineViewerState {
    /// Construct outline-viewer state — ports `TOutlineViewer::TOutlineViewer`
    /// (and the `TScroller` base ctor it chains).
    ///
    /// Faithful: `growMode = gfGrowHiX + gfGrowHiY`; `options |= ofSelectable`
    /// (from the scroller base); `foc = 0`; `delta = limit = (0, 0)`. The C++
    /// `eventMask |= evBroadcast` has no analogue under D4 (broadcasts always
    /// delivered).
    pub fn new(bounds: Rect, h: Option<ViewId>, v: Option<ViewId>) -> Self {
        let mut state = ViewState::new(bounds);
        state.options = Options {
            selectable: true,
            ..Default::default()
        };
        state.grow_mode = GrowMode {
            hi_x: true,
            hi_y: true,
            ..Default::default()
        };
        OutlineViewerState {
            state,
            delta: Point::new(0, 0),
            limit: Point::new(0, 0),
            h_scroll_bar: h,
            v_scroll_bar: v,
            foc: 0,
            abs_origin: Point::new(0, 0),
            track: None,
        }
    }

    /// `TScroller::setLimit` — set the content extent and (re)publish each bar's
    /// range/page params. Identical formula to [`Scroller::set_limit`](crate::widgets::Scroller::set_limit).
    pub fn set_limit(&mut self, x: i32, y: i32, ctx: &mut Context) {
        self.limit = Point::new(x, y);
        let size = self.state.size;
        if let Some(h) = self.h_scroll_bar {
            ctx.request_scroll_bar_params(
                h,
                None,
                Some(0),
                Some(x - size.x),
                Some(size.x - 1),
                None,
            );
        }
        if let Some(v) = self.v_scroll_bar {
            ctx.request_scroll_bar_params(
                v,
                None,
                Some(0),
                Some(y - size.y),
                Some(size.y - 1),
                None,
            );
        }
    }

    /// `TScroller::scrollTo` — set each bar's value (preserving range and steps).
    /// Identical to [`Scroller::scroll_to`](crate::widgets::Scroller::scroll_to).
    pub fn scroll_to(&mut self, x: i32, y: i32, ctx: &mut Context) {
        if let Some(h) = self.h_scroll_bar {
            ctx.request_scroll_bar_params(h, Some(x), None, None, None, None);
        }
        if let Some(v) = self.v_scroll_bar {
            ctx.request_scroll_bar_params(v, Some(y), None, None, None, None);
        }
    }

    /// Apply a freshly-read scrollbar delta. Unlike the scroller's `apply_delta`
    /// there is no cursor adjust (the outline viewer has no editor cursor); just
    /// overwrite `delta`. Called by the pump (the read broker) after it resolves
    /// the bars and reads their `value`s.
    pub fn apply_delta(&mut self, d: Point) {
        self.delta = d;
    }

    /// `TScroller::showSBar` — show/hide one bar per this viewer's active/selected
    /// state. Faithful: `getState(sfActive | sfSelected) != 0` → show, else hide.
    fn show_sbar(&self, sbar: Option<ViewId>, ctx: &mut Context) {
        if let Some(id) = sbar {
            let visible = self.state.state.active || self.state.state.selected;
            ctx.request_set_visible(id, visible);
        }
    }
}

// ---------------------------------------------------------------------------
// OutlineViewer — the overridable virtuals (a trait, D-A)
// ---------------------------------------------------------------------------

/// The abstract outline-viewer base — `TOutlineViewer`'s overridable virtuals
/// (D-A). Concrete outline widgets implement [`ov`](Self::ov)/[`ov_mut`](Self::ov_mut)
/// (the data accessors) and the tree-navigation virtuals; the shared draw / event
/// / traversal logic (the free functions in this module) is generic over `L:
/// OutlineViewer` and calls back into these.
///
/// Intentionally **not object-safe** (the `'a`-bound getters) — that is fine,
/// concrete widgets are `Box<dyn View>` and `OutlineViewer` is only ever a generic
/// bound behind a concrete type (same as [`ListViewer`](crate::widgets::ListViewer)).
///
/// **Wiring caveat (no compile-time enforcement):** a concrete outline widget MUST
/// delegate the relevant `View` methods to this module's free functions: [`ov_draw`],
/// [`ov_handle_event`], [`ov_set_state`], and [`View::as_any_mut`](crate::view::View::as_any_mut)
/// (the cross-view broker downcasts through it).
pub trait OutlineViewer: View {
    /// Borrow the embedded [`OutlineViewerState`].
    fn ov(&self) -> &OutlineViewerState;
    /// Mutably borrow the embedded [`OutlineViewerState`].
    fn ov_mut(&mut self) -> &mut OutlineViewerState;

    // -- Abstract read-only virtuals (borrow `&Node`s out of `&self`) ---------

    /// `TOutlineViewer::getRoot` — the root node (or `None` if the tree is empty).
    fn get_root(&self) -> Option<&Node>;
    /// `TOutlineViewer::getNext` — `node`'s next sibling (or `None`).
    fn get_next<'a>(&'a self, node: &'a Node) -> Option<&'a Node>;
    /// `TOutlineViewer::getChild` — `node`'s `i`-th child (0-based, or `None`).
    fn get_child<'a>(&'a self, node: &'a Node, i: i32) -> Option<&'a Node>;
    /// `TOutlineViewer::getNumChildren` — `node`'s child count.
    fn get_num_children(&self, node: &Node) -> i32;
    /// `TOutlineViewer::getText` — `node`'s displayed text.
    fn get_text<'a>(&'a self, node: &'a Node) -> &'a str;
    /// `TOutlineViewer::isExpanded` — whether `node` is expanded.
    fn is_expanded(&self, node: &Node) -> bool;
    /// `TOutlineViewer::hasChildren` — whether `node` has any children.
    fn has_children(&self, node: &Node) -> bool;

    // -- Abstract mutation method ---------------------------------------------

    /// `TOutlineViewer::adjust` — set the expanded state of the node at DFS
    /// position `pos` in the **currently visible** tree (0-based, same numbering as
    /// [`foc`](OutlineViewerState::foc)).
    ///
    /// (C++ `adjust(TNode*, Boolean)` takes the node pointer; under D3 the shared
    /// free functions only hold `&self`, so the abstract contract is keyed by DFS
    /// position — the concrete widget resolves `pos` to the owned node mutably.)
    fn adjust(&mut self, pos: i32, expand: bool);

    // -- Overridable with defaults --------------------------------------------

    /// `TOutlineViewer::focused` — `node` at position `i` received focus. Base:
    /// `foc = i`.
    fn focused_item(&mut self, i: i32) {
        self.ov_mut().foc = i;
    }

    /// `TOutlineViewer::isSelected` — whether position `i` is "selected" (drawn in
    /// the selected color). Base: `i == foc` (single selection); multi-select
    /// subclasses override.
    fn is_selected(&self, i: i32) -> bool {
        self.ov().foc == i
    }

    /// `TOutlineViewer::selected` — the user committed to position `i`
    /// (double-click / Enter). The C++ base does nothing (empty body); override to
    /// act (e.g. broadcast [`Command::OUTLINE_ITEM_SELECTED`]).
    fn selected(&mut self, _i: i32) {}
}

// ---------------------------------------------------------------------------
// Traversal core — port of iterate + traverseTree (DFS visitor)
// ---------------------------------------------------------------------------

/// Port of `traverseTree`'s inner recursion: visit `node` and (if expanded) its
/// visible subtree, calling `action(this, node, level, position, lines, flags)`.
/// Returns `true` if the visitor stopped the traversal.
///
/// `flags`: `ovExpanded` if the node is a leaf or expanded; `ovChildren` if it has
/// children and is expanded; `ovLast` if it is its parent's last child. `lines`:
/// bit N set means level N has a continuation bar at/below this node. `position` is
/// pre-incremented before each visit (so 0-based after the first).
///
/// Root-level siblings are handled by the caller [`traverse`], NOT here (the C++
/// `if (cur == getRoot())` block) — we only ever enter this for a node already
/// chosen by `traverse`.
fn traverse_inner<L, F>(
    this: &L,
    action: &mut F,
    node: &Node,
    level: i32,
    lines: i64,
    position: &mut i32,
    last_child: bool,
) -> bool
where
    L: OutlineViewer + ?Sized,
    F: FnMut(&L, &Node, i32, i32, i64, u16) -> bool,
{
    let children = this.has_children(node);
    let expanded = this.is_expanded(node);

    let mut flags: u16 = 0;
    if last_child {
        flags |= OV_LAST;
    }
    if children && expanded {
        flags |= OV_CHILDREN;
    }
    if !children || expanded {
        flags |= OV_EXPANDED;
    }

    *position += 1;
    if action(this, node, level, *position, lines, flags) {
        return true;
    }

    if children && expanded {
        let child_lines = if !last_child {
            lines | (1i64 << level)
        } else {
            lines
        };
        let mut child = this.get_child(node, 0);
        while let Some(c) = child {
            let next = this.get_next(c);
            let is_last = next.is_none();
            if traverse_inner(this, action, c, level + 1, child_lines, position, is_last) {
                return true;
            }
            child = next;
        }
    }
    false
}

/// Port of `TOutlineViewer::iterate` + `traverseTree` — DFS-visit every currently
/// visible node, calling `action` for each. The visitor returns `true` to stop.
///
/// Visits the root, its visible subtree, then the root's next siblings at level 0
/// (the C++ `if (cur == getRoot())` block, lifted here so `traverse_inner` stays a
/// pure subtree walk).
pub fn traverse<L, F>(this: &L, action: &mut F)
where
    L: OutlineViewer + ?Sized,
    F: FnMut(&L, &Node, i32, i32, i64, u16) -> bool,
{
    let Some(root) = this.get_root() else {
        return;
    };
    let mut position = -1i32;

    let root_last = this.get_next(root).is_none();
    if traverse_inner(this, action, root, 0, 0, &mut position, root_last) {
        return;
    }

    // Root-level siblings (the C++ `if (cur == getRoot())` loop).
    let mut sibling = this.get_next(root);
    while let Some(s) = sibling {
        let next = this.get_next(s);
        let is_last = next.is_none();
        if traverse_inner(this, action, s, 0, 0, &mut position, is_last) {
            return;
        }
        sibling = next;
    }
}

/// Find the `(level, lines, flags)` of the node at DFS position `pos`, or `None`.
/// (`TOutlineViewer`'s `isFocused` helper made reusable — the draw/event code uses
/// it to recover graph-draw parameters for the focused node.)
pub fn ov_get_node_info<L: OutlineViewer + ?Sized>(this: &L, pos: i32) -> Option<(i32, i64, u16)> {
    let mut result = None;
    traverse(this, &mut |_ov: &L,
                         _node: &Node,
                         level,
                         position,
                         lines,
                         flags| {
        if position == pos {
            result = Some((level, lines, flags));
            true
        } else {
            false
        }
    });
    result
}

// ---------------------------------------------------------------------------
// createGraph / getGraph — the indent/box-drawing prefix
// ---------------------------------------------------------------------------

/// Faithful port of `TOutlineViewer::createGraph` — build the indent + node graph
/// string. Returns an owned `String` (C++ returns a heap `char*`).
///
/// `chars` layout (C++ comment): [0] level filler, [1] level mark, [2] end-first
/// (not last), [3] end-first (last), [4] end filler, [5] end-child, [6] retracted,
/// [7] expanded.
pub fn create_graph(
    level: i32,
    mut lines: i64,
    flags: u16,
    lev_width: i32,
    mut end_width: i32,
    chars: &[char; 8],
) -> String {
    const FILLER_OR_BAR: usize = 0;
    const Y_OR_L: usize = 2;
    const STRAIGHT_OR_TEE: usize = 4;
    const RETRACTED: usize = 6;

    let expanded = (flags & OV_EXPANDED) != 0;
    let children = (flags & OV_CHILDREN) != 0;
    let last = (flags & OV_LAST) != 0;

    let mut graph = String::new();

    // Level marks: per level, the mark-or-filler, then `lev_width - 1` fillers.
    let mut lev = level;
    while lev > 0 {
        graph.push(if lines & 1 != 0 {
            chars[FILLER_OR_BAR + 1]
        } else {
            chars[FILLER_OR_BAR]
        });
        for _ in 0..(lev_width - 1) {
            graph.push(chars[FILLER_OR_BAR]);
        }
        lev -= 1;
        lines >>= 1;
    }

    // End graphic (the `--endWidth` cascade, verbatim).
    end_width -= 1;
    if end_width > 0 {
        graph.push(if last {
            chars[Y_OR_L + 1]
        } else {
            chars[Y_OR_L]
        });
        end_width -= 1;
        if end_width > 0 {
            end_width -= 1;
            if end_width > 0 {
                for _ in 0..end_width {
                    graph.push(chars[STRAIGHT_OR_TEE]);
                }
            }
            graph.push(if children {
                chars[STRAIGHT_OR_TEE + 1]
            } else {
                chars[STRAIGHT_OR_TEE]
            });
        }
        graph.push(if expanded {
            chars[RETRACTED + 1]
        } else {
            chars[RETRACTED]
        });
    }

    graph
}

/// Port of `TOutlineViewer::getGraph` — the default graph string (levelWidth =
/// endWidth = 3) with the classic box-drawing chars.
pub fn ov_get_graph<L: OutlineViewer + ?Sized>(
    _this: &L,
    level: i32,
    lines: i64,
    flags: u16,
    ctx: &DrawCtx,
) -> String {
    let g = ctx.glyphs();
    // "\x20\xB3\xC3\xC0\xC4\xC4+\xC4": space, │, ├, └, ─, ─, +, ─.
    let chars: [char; 8] = [
        ' ',
        g.frame_v,
        g.frame_tee_l,
        g.frame_bl,
        g.frame_h,
        g.frame_h,
        '+',
        g.frame_h,
    ];
    create_graph(level, lines, flags, 3, 3, &chars)
}

// ---------------------------------------------------------------------------
// ov_draw — port of TOutlineViewer::draw / drawTree
// ---------------------------------------------------------------------------

/// `TOutlineViewer::draw` / the `drawTree` callback — render every visible node.
///
/// Ports the C++ draw loop: per visible node compute the color (focused / selected
/// / normal), fill the row, draw the graph then the text (the text uses the dim
/// `color >> 8` when the node is not expanded), shifted left by `delta.x`. After
/// the traversal the remaining rows are blank-filled (the C++ trailing
/// `writeLine`).
///
/// NOTE: `this: &mut L` (not `&L`) — the `abs_origin` cache write requires
/// mutability. Do NOT revert to `&L`: the C++ draw is logically const, but the
/// port stores the origin here to feed [`Context::start_mouse_track`]
/// (the Button::abs_origin pattern, recipe step 1 in docs/design/mouse-track.md).
pub fn ov_draw<L: OutlineViewer + ?Sized>(this: &mut L, ctx: &mut DrawCtx) {
    // Cache the absolute origin for the mouse-tracking capture (D3/D9 — the
    // MouseTrackCapture converts abs mouse coords to view-local via this value,
    // mirroring the Button::abs_origin pattern).
    this.ov_mut().abs_origin = ctx.origin();
    let size = this.ov().state.size;
    let delta = this.ov().delta;
    let foc = this.ov().foc;
    let focused_state = this.ov().state.state.focused;

    let nrm_color = ctx.style(Role::OutlineNormal);
    let focused_color = ctx.style(Role::OutlineFocused);
    let selected_color = ctx.style(Role::OutlineSelected);
    let not_expanded_color = ctx.style(Role::OutlineNotExpanded);

    // Last drawn position (the C++ `auxPos`, -1 if nothing drawn).
    let mut aux_pos = -1i32;

    // The closure cannot borrow `ctx` (it would need a second &mut alongside the
    // generic `this`), so we gather the draw instructions and apply them after.
    struct Row {
        position: i32,
        level: i32,
        lines: i64,
        flags: u16,
        text: String,
    }
    let mut rows: Vec<Row> = Vec::new();

    traverse(this, &mut |this: &L,
                         node: &Node,
                         level,
                         position,
                         lines,
                         flags| {
        if position >= delta.y {
            if position >= delta.y + size.y {
                return true; // past the bottom — stop
            }
            rows.push(Row {
                position,
                level,
                lines,
                flags,
                text: this.get_text(node).to_string(),
            });
            aux_pos = position;
        }
        false
    });

    for row in &rows {
        // Color selection (C++ drawTree). The C++ picks an AttrPair per row and
        // draws the text with `(flags & ovExpanded) ? color : (color >> 8)` — the
        // HIGH byte of THAT row's pair, not a fixed not-expanded color. The pairs:
        //   normal   getColor(0x0401): lo = Normal,   hi = NotExpanded
        //   focused  getColor(0x0202): lo = hi = Focused
        //   selected getColor(0x0303): lo = hi = Selected
        // so the dim (not-expanded) text color equals the row color for the
        // focused/selected branches, and only differs (→ NotExpanded) for normal.
        let (color, dim_color) = if row.position == foc && focused_state {
            (focused_color, focused_color)
        } else if this.is_selected(row.position) {
            (selected_color, selected_color)
        } else {
            (nrm_color, not_expanded_color)
        };

        let y = row.position - delta.y;
        // moveChar(0, ' ', color, size.x) — fill the whole row first.
        ctx.fill(Rect::new(0, y, size.x, y + 1), ' ', color);

        // Graph: drawn from column 0, shifted left by delta.x.
        let graph = ov_get_graph(this, row.level, row.lines, row.flags, ctx);
        let graph_w = graph.chars().count() as i32;
        // x = strwidth(graph) - delta.x; the text starts at max(0, x).
        let x = graph_w - delta.x;
        if x > 0 {
            // moveStr(0, graph, color, -1, delta.x) — skip delta.x leading cols.
            ctx.put_str_part(0, y, &graph, delta.x, color);
        }

        // Text: dim color (`color >> 8`) when not expanded, else the row color.
        let text_color = if row.flags & OV_EXPANDED != 0 {
            color
        } else {
            dim_color
        };
        // moveStr(max(0, x), text, c, -1, max(0, -x)).
        let text_x = x.max(0);
        let text_skip = (-x).max(0);
        ctx.put_str_part(text_x, y, &row.text, text_skip, text_color);
    }

    // Blank the remaining rows below the last drawn node (the C++ trailing fill:
    // writeLine(0, auxPos+1, size.x, size.y - (auxPos - delta.y), ...)). DEVIATION:
    // C++ passes `auxPos + 1` (an absolute DFS position) as a view-local start row;
    // we subtract `delta.y` to convert it to a view-local row. Equivalent in every
    // reachable state (delta.y == 0, or the view is full and the loop clipped at
    // the bottom), and avoids drawing off the top when delta.y > 0.
    let first_blank = aux_pos + 1 - delta.y;
    if first_blank < size.y {
        ctx.fill(
            Rect::new(0, first_blank.max(0), size.x, size.y),
            ' ',
            nrm_color,
        );
    }
}

// ---------------------------------------------------------------------------
// Focus / navigation / event handling
// ---------------------------------------------------------------------------

/// `TOutlineViewer::adjustFocus` — clamp `new_focus`, focus it, and scroll it into
/// view.
pub fn adjust_focus<L: OutlineViewer + ?Sized>(
    this: &mut L,
    mut new_focus: i32,
    ctx: &mut Context,
) {
    let limit_y = this.ov().limit.y;
    if new_focus < 0 {
        new_focus = 0;
    } else if new_focus >= limit_y {
        new_focus = limit_y - 1;
    }
    if this.ov().foc != new_focus {
        this.focused_item(new_focus);
    }
    let delta = this.ov().delta;
    let size_y = this.ov().state.size.y;
    if new_focus < delta.y {
        this.ov_mut().scroll_to(delta.x, new_focus, ctx);
    } else if (new_focus - size_y) >= delta.y {
        this.ov_mut()
            .scroll_to(delta.x, new_focus - size_y + 1, ctx);
    }
}

/// `TOutlineViewer::update` — recount the visible nodes + max width, (re)publish
/// the scrollbar limits, and re-clamp the focus.
///
/// The C++ `firstThat(countNode)` walks the SAME visible traversal as draw, so
/// `update` counts only currently-visible nodes (collapsed subtrees excluded).
pub fn ov_update<L: OutlineViewer + ?Sized>(this: &mut L, ctx: &mut Context) {
    // Count visible nodes and the max graph+text width. getGraph needs a DrawCtx
    // for glyphs, but here we only need the width — and the default graph width is
    // deterministic: level*3 + 3 (levWidth = endWidth = 3). So compute the width
    // analytically (faithful: strwidth(graph) == level*levWidth + endWidth for the
    // default 3/3 graph, whose end portion is always exactly endWidth chars).
    let mut count = 0i32;
    let mut max_x = 0i32;
    traverse(this, &mut |this: &L,
                         node: &Node,
                         level,
                         _position,
                         _lines,
                         _flags| {
        count += 1;
        let graph_w = level * 3 + 3;
        let text_w = this.get_text(node).chars().count() as i32;
        let len = graph_w + text_w;
        if max_x < len {
            max_x = len;
        }
        false
    });
    this.ov_mut().set_limit(max_x, count, ctx);
    let foc = this.ov().foc;
    adjust_focus(this, foc, ctx);
}

/// `TOutlineViewer::expandAll` — expand the node at position `pos` and all of its
/// descendants (NOT its siblings).
///
/// C++ `expandAll(TNode*)` recurses over a fixed node pointer; under D3 the shared
/// code is keyed by DFS position, and positions shift as nodes expand. We restart
/// the traversal each round: find the depth (`start_level`) of the node at `pos`
/// once, then repeatedly expand the first unexpanded node-with-children that is
/// inside the `pos` subtree (position `>= pos`, and either `position == pos` or
/// `level > start_level`); a node at `level <= start_level && position > pos` is
/// either a sibling or an ancestor's sibling → stop. Converges because each round
/// expands at least one node.
pub fn ov_expand_all<L: OutlineViewer + ?Sized>(this: &mut L, pos: i32) {
    // 1. Find the level of the node at `pos`.
    let mut start_level = 0i32;
    let mut found_start = false;
    traverse(this, &mut |_ov: &L,
                         _node: &Node,
                         level,
                         position,
                         _lines,
                         _flags| {
        if position == pos {
            start_level = level;
            found_start = true;
            true
        } else {
            false
        }
    });
    if !found_start {
        return;
    }

    // 2. Loop: expand the first eligible unexpanded node-with-children.
    loop {
        let mut to_expand: Option<i32> = None;
        traverse(this, &mut |ov: &L,
                             node: &Node,
                             level,
                             position,
                             _lines,
                             _flags| {
            if position < pos {
                return false;
            }
            // A node at the start level or shallower past `pos` is a sibling (or
            // ancestor's sibling) — stop the scan.
            if level <= start_level && position > pos {
                return true;
            }
            if ov.has_children(node) && !ov.is_expanded(node) {
                to_expand = Some(position);
                return true;
            }
            false
        });
        match to_expand {
            Some(p) => this.adjust(p, true),
            None => break,
        }
    }
}

/// `TOutlineViewer::setState` — flip the flag (+ the Focused broadcast), then on
/// `Active`/`Selected` show/hide both bars. Mirrors the scroller's `set_state`
/// (the outline viewer's `setState` chains `TScroller::setState`). The C++
/// `sfFocused → drawView()` is dropped (D8; the whole tree redraws each pass).
pub fn ov_set_state<L: OutlineViewer + ?Sized>(
    this: &mut L,
    flag: StateFlag,
    enable: bool,
    ctx: &mut Context,
) {
    this.ov_mut().state.set_flag(flag, enable);
    if flag == StateFlag::Focused {
        let source = this.ov().state.id();
        ctx.broadcast(
            if enable {
                Command::RECEIVED_FOCUS
            } else {
                Command::RELEASED_FOCUS
            },
            source,
        );
    }
    if flag == StateFlag::Active || flag == StateFlag::Selected {
        let h = this.ov().h_scroll_bar;
        let v = this.ov().v_scroll_bar;
        this.ov().show_sbar(h, ctx);
        this.ov().show_sbar(v, ctx);
    }
}

/// `TOutlineViewer::handleEvent` — the scrollbar broadcast filter (inherited from
/// `TScroller`), mouse hold-tracking (D9/A3 seam, row 31), and the keyboard nav switch.
///
/// The press-and-hold / edge auto-scroll loop (toutline.cpp:433-463) is ported
/// via the A3 `MouseTrackCapture` seam: `MouseDown` arms capture; tracked
/// `MouseMove`/`MouseAuto` route the loop body; `MouseUp` runs the post-loop
/// graph-toggle logic (`dragged < 2` distinguishes click from drag).
pub fn ov_handle_event<L: OutlineViewer + View + ?Sized>(
    this: &mut L,
    ev: &mut Event,
    ctx: &mut Context,
) {
    // TScroller::handleEvent super-call — the cmScrollBarChanged read-sync filter.
    // Do NOT clear the event — same as the scroller (it stays live for the
    // scrollbar's own handling). The super-call does not consume it.
    if let Event::Broadcast { command, source } = *ev
        && command == Command::SCROLL_BAR_CHANGED
        && source.is_some()
        && (source == this.ov().h_scroll_bar || source == this.ov().v_scroll_bar)
        && let Some(id) = this.ov().state.id()
    {
        ctx.request_sync_outline_viewer_delta(id, this.ov().h_scroll_bar, this.ov().v_scroll_bar);
    }

    match *ev {
        // -------------------------------------------------------------------
        // evMouseDown — first loop iteration: position, then arm the
        // mouse-track capture (D9/A3 seam, toutline.cpp:433-463).
        //
        // C++ `do { … } while(mouseEvent(event, evMouseMove + evMouseAuto))`:
        // the loop body runs once per DOWN, MOVE, or AUTO event; the post-loop
        // block (double-click / graph-toggle) runs after the hold ends.
        //
        // Unlike ListViewer, the post-loop logic is COMPLEX — it depends on
        // `dragged` (how many iterations ran) and `mouse.x` at exit. We store
        // `dragged` in `OvTrack` so the MouseUp arm can perform the post-loop
        // checks faithfully.
        // -------------------------------------------------------------------
        Event::MouseDown(me) => {
            let delta = this.ov().delta;
            let limit_y = this.ov().limit.y;
            let foc = this.ov().foc;
            // mouse is view-local already (D3 — makeLocal/mouseInView are gone).
            let mouse = me.position;
            // mouseInView: the click landed inside this view's extent.
            let in_view = mouse.x >= 0
                && mouse.y >= 0
                && mouse.x < this.ov().state.size.x
                && mouse.y < this.ov().state.size.y;
            let new_focus = if in_view {
                let i = delta.y + mouse.y;
                if i < limit_y { i } else { foc }
            } else {
                foc
            };

            // First loop iteration: position (C++ body line 436-462).
            // `dragged` starts at 0, then incremented to 1 on the first
            // iteration (C++ `if (dragged < 2) dragged++`).
            let dragged: u8 = 1; // after first iteration
            if foc != new_focus {
                adjust_focus(this, new_focus, ctx);
            }

            if me.flags.double_click {
                // Double-click: break immediately (no tracking), then
                // post-loop: `selected(foc)`. `foc` is the focused item at
                // break time (which is `new_focus` after the first iteration).
                this.selected(this.ov().foc);
                ev.clear();
            } else if let Some(id) = this.ov().state.id() {
                // Non-double-click: arm the mouse-track capture (D9/A3 seam).
                // The post-loop graph-toggle logic runs in the MouseUp arm.
                let abs_origin = this.ov().abs_origin;
                this.ov_mut().track = Some(OvTrack { count: 0, dragged });
                ctx.start_mouse_track(
                    id,
                    abs_origin,
                    TrackMask {
                        mouse_move: true,
                        mouse_auto: true,
                        ..Default::default()
                    },
                );
                ev.clear();
            } else {
                // Uninserted (test/degenerate) widget: single-shot behavior,
                // mirroring the existing pre-D9 path.
                if let Some((level, _lines, flags)) = ov_get_node_info(this, this.ov().foc) {
                    let graph_w = level * 3 + 3;
                    if mouse.x < graph_w {
                        let cur_pos = this.ov().foc;
                        let expanded = flags & OV_EXPANDED != 0;
                        this.adjust(cur_pos, !expanded);
                        ov_update(this, ctx);
                    }
                }
                ev.clear();
            }
        }

        // -------------------------------------------------------------------
        // evMouseMove (tracked) — the loop body's in-view move case.
        //
        // C++ toutline.cpp:437-462: `if (dragged < 2) dragged++` fires first
        // (every iteration), then `if (mouseInView)` → compute newFocus.
        // Out-of-view moves do nothing (only evMouseAuto steps the focus).
        // Guarded by `track.is_some()`.
        // -------------------------------------------------------------------
        Event::MouseMove(me) if this.ov().track.is_some() => {
            let delta = this.ov().delta;
            let limit_y = this.ov().limit.y;
            let foc = this.ov().foc;
            let mouse = me.position;
            let size = this.ov().state.size;
            let in_view = mouse.x >= 0 && mouse.y >= 0 && mouse.x < size.x && mouse.y < size.y;

            // Increment dragged (capped at 2) — faithful: first in the loop body.
            if let Some(t) = this.ov_mut().track.as_mut()
                && t.dragged < 2
            {
                t.dragged += 1;
            }

            if in_view {
                let new_focus = {
                    let i = delta.y + mouse.y;
                    if i < limit_y { i } else { foc }
                };
                if foc != new_focus {
                    adjust_focus(this, new_focus, ctx);
                    // drawView() dropped (D8).
                }
            }
            // Out-of-view moves: no-op (only evMouseAuto steps for out-of-view).
            ev.clear();
        }

        // -------------------------------------------------------------------
        // evMouseAuto (tracked) — the loop body's auto-scroll case.
        //
        // C++ toutline.cpp:437-462: `if (dragged < 2) dragged++`; then
        // `if (mouseInView)` → compute newFocus; else: `count++`, if
        // `count == mouseAutoToSkip` (3): reset, step by ±1 based on y.
        // Guarded by `track.is_some()`.
        // -------------------------------------------------------------------
        Event::MouseAuto(me) if this.ov().track.is_some() => {
            let delta = this.ov().delta;
            let limit_y = this.ov().limit.y;
            let foc = this.ov().foc;
            let mouse = me.position;
            let size = this.ov().state.size;
            let in_view = mouse.x >= 0 && mouse.y >= 0 && mouse.x < size.x && mouse.y < size.y;

            // Increment dragged (capped at 2) — faithful: first in the loop body.
            if let Some(t) = this.ov_mut().track.as_mut()
                && t.dragged < 2
            {
                t.dragged += 1;
            }

            let new_focus = if in_view {
                let i = delta.y + mouse.y;
                if i < limit_y { i } else { foc }
            } else {
                // Out-of-view AUTO: increment count, step every MOUSE_AUTO_TO_SKIP ticks.
                let stepped = if let Some(t) = this.ov_mut().track.as_mut() {
                    t.count += 1;
                    if t.count == MOUSE_AUTO_TO_SKIP {
                        t.count = 0;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if stepped {
                    if mouse.y < 0 {
                        foc - 1
                    } else if mouse.y >= size.y {
                        foc + 1
                    } else {
                        foc
                    }
                } else {
                    foc
                }
            };

            if foc != new_focus {
                adjust_focus(this, new_focus, ctx);
                // drawView() dropped (D8).
            }
            ev.clear();
        }

        // -------------------------------------------------------------------
        // evMouseUp (tracked) — post-loop logic (toutline.cpp:465-480).
        //
        // C++ post-loop:
        //   if (meDoubleClick) selected(foc);
        //   else if (dragged < 2 && firstThat(isFocused) != 0) {
        //       graph = getGraph(focLevel,focLines,focFlags);
        //       if (mouse.x < strwidth(graph)) { adjust; update; }
        //   }
        //
        // `meDoubleClick` cannot arrive via a MouseUp (it's only on a down event),
        // so in the capture flow the double-click branch is unreachable here.
        // `dragged < 2` gates the graph-toggle: only a short press (≤1 iteration
        // after the first) toggles expansion; a drag must NOT toggle.
        // `mouse` at exit is the last localized position, which for MouseUp is
        // the release position (the capture forwards the localized up event).
        // Guarded by `track.is_some()`.
        // -------------------------------------------------------------------
        Event::MouseUp(me) if this.ov().track.is_some() => {
            let mouse = me.position;
            let dragged = this.ov().track.map(|t| t.dragged).unwrap_or(2);
            this.ov_mut().track = None;

            // Post-loop graph-toggle: only if dragged < 2 (it was a click, not
            // a drag) and there is a focused node with a graph column.
            if dragged < 2
                && let Some((level, _lines, flags)) = ov_get_node_info(this, this.ov().foc)
            {
                let graph_w = level * 3 + 3;
                if mouse.x < graph_w {
                    let cur_pos = this.ov().foc;
                    let expanded = flags & OV_EXPANDED != 0;
                    this.adjust(cur_pos, !expanded);
                    ov_update(this, ctx);
                }
            }
            // drawView() dropped (D8).
            ev.clear();
        }

        Event::KeyDown(ke) => {
            let foc = this.ov().foc;
            let size_y = this.ov().state.size.y;
            let delta = this.ov().delta;
            let limit_y = this.ov().limit.y;

            let mut new_focus = foc;
            let mapped = ctrl_to_arrow(ke);

            // kbCtrlPgUp / kbCtrlPgDn first (matched on the decomposed key, like the
            // list viewer — ctrl_to_arrow leaves PageUp/PageDown unchanged).
            if matches!(ke.key, Key::PageUp) && ke.modifiers.ctrl {
                new_focus = 0;
                clear_and_adjust(this, new_focus, ctx, ev);
                return;
            }
            if matches!(ke.key, Key::PageDown) && ke.modifiers.ctrl {
                new_focus = limit_y - 1;
                clear_and_adjust(this, new_focus, ctx, ev);
                return;
            }

            match mapped.key {
                Key::Up | Key::Left => {
                    new_focus -= 1;
                    clear_and_adjust(this, new_focus, ctx, ev);
                }
                Key::Down | Key::Right => {
                    new_focus += 1;
                    clear_and_adjust(this, new_focus, ctx, ev);
                }
                Key::PageDown => {
                    new_focus += size_y - 1;
                    clear_and_adjust(this, new_focus, ctx, ev);
                }
                Key::PageUp => {
                    new_focus -= size_y - 1;
                    clear_and_adjust(this, new_focus, ctx, ev);
                }
                Key::Home => {
                    new_focus = delta.y;
                    clear_and_adjust(this, new_focus, ctx, ev);
                }
                Key::End => {
                    new_focus = delta.y + size_y - 1;
                    clear_and_adjust(this, new_focus, ctx, ev);
                }
                Key::Enter => {
                    // kbEnter / kbCtrlEnter → selected(newFocus).
                    this.selected(new_focus);
                    clear_and_adjust(this, new_focus, ctx, ev);
                }
                Key::Char(c) => match c {
                    '+' => {
                        let cur = this.ov().foc;
                        this.adjust(cur, true);
                        ov_update(this, ctx);
                        clear_and_adjust(this, new_focus, ctx, ev);
                    }
                    '-' => {
                        let cur = this.ov().foc;
                        this.adjust(cur, false);
                        ov_update(this, ctx);
                        clear_and_adjust(this, new_focus, ctx, ev);
                    }
                    '*' => {
                        let cur = this.ov().foc;
                        ov_expand_all(this, cur);
                        ov_update(this, ctx);
                        clear_and_adjust(this, new_focus, ctx, ev);
                    }
                    _ => {} // C++ default: return (event left live).
                },
                _ => {} // unhandled nav key — return (event left live).
            }
        }

        _ => {}
    }
}

/// Helper for the keyboard handler: clear the event, then `adjustFocus` (the C++
/// tail `clearEvent(event); adjustFocus(newFocus); drawView();` — `drawView`
/// dropped, D8).
fn clear_and_adjust<L: OutlineViewer + ?Sized>(
    this: &mut L,
    new_focus: i32,
    ctx: &mut Context,
    ev: &mut Event,
) {
    ev.clear();
    adjust_focus(this, new_focus, ctx);
}

// ---------------------------------------------------------------------------
// TOutline (row 90) — the concrete outline over an owned tree
// ---------------------------------------------------------------------------

/// `TOutline` — the concrete outline viewer over an owned [`Node`] tree (row 90).
pub struct Outline {
    ov: OutlineViewerState,
    /// `TOutline::root` — the owned tree root (`None` = empty).
    pub root: Option<Box<Node>>,
}

impl Outline {
    /// `TOutline::TOutline` — build over `bounds`, the two scrollbars, and `root`.
    ///
    /// **NOTE:** the C++ ctor calls `update()`, which needs a `Context` we do not
    /// have at construction. The consumer must call [`ov_update`] once after
    /// inserting this outline into a group (the same constraint the scroller /
    /// list-viewer ctors hit).
    pub fn new(
        bounds: Rect,
        h: Option<ViewId>,
        v: Option<ViewId>,
        root: Option<Box<Node>>,
    ) -> Self {
        Outline {
            ov: OutlineViewerState::new(bounds, h, v),
            root,
        }
    }
}

/// Collect, in DFS pre-order, raw pointers to every currently-visible node
/// (a node and — if expanded — its visible subtree). Mirrors `traverseTree`'s
/// visible walk without the flags/lines bookkeeping; the caller iterates root
/// siblings separately (matching `traverse`).
/// Recursively find the visible node at DFS position `target` (pre-order, 0-based)
/// and set its `expanded` flag. `counter` starts at the position of `node`.
///
/// Visits `node`, then its visible children (via `child_list`), then its next
/// sibling (via `next`) — matching the DFS order of `traverse`. Returns `true`
/// when the target is found so callers can short-circuit.
fn set_expanded_at_pos(node: &mut Node, target: i32, counter: &mut i32, expand: bool) -> bool {
    if *counter == target {
        node.expanded = expand;
        return true;
    }
    // Recurse into visible children first.
    if node.expanded
        && let Some(child) = node.child_list.as_deref_mut()
    {
        *counter += 1;
        if set_expanded_at_pos(child, target, counter, expand) {
            return true;
        }
    }
    // Then continue to the next sibling (handles both child-level siblings and,
    // when called on root, root-level siblings — same DFS order as `traverse`).
    if let Some(next) = node.next.as_deref_mut() {
        *counter += 1;
        set_expanded_at_pos(next, target, counter, expand)
    } else {
        false
    }
}

impl OutlineViewer for Outline {
    fn ov(&self) -> &OutlineViewerState {
        &self.ov
    }
    fn ov_mut(&mut self) -> &mut OutlineViewerState {
        &mut self.ov
    }

    fn get_root(&self) -> Option<&Node> {
        self.root.as_deref()
    }
    fn get_next<'a>(&'a self, node: &'a Node) -> Option<&'a Node> {
        node.next.as_deref()
    }
    fn get_child<'a>(&'a self, node: &'a Node, i: i32) -> Option<&'a Node> {
        // C++ getChild walks childList->next i times.
        let mut p = node.child_list.as_deref();
        let mut idx = 0;
        while idx < i {
            match p {
                Some(n) => p = n.next.as_deref(),
                None => break,
            }
            idx += 1;
        }
        p
    }
    fn get_num_children(&self, node: &Node) -> i32 {
        let mut i = 0;
        let mut p = node.child_list.as_deref();
        while let Some(n) = p {
            i += 1;
            p = n.next.as_deref();
        }
        i
    }
    fn get_text<'a>(&'a self, node: &'a Node) -> &'a str {
        &node.text
    }
    fn is_expanded(&self, node: &Node) -> bool {
        node.expanded
    }
    fn has_children(&self, node: &Node) -> bool {
        node.child_list.is_some()
    }

    fn adjust(&mut self, pos: i32, expand: bool) {
        if pos < 0 {
            return;
        }
        if let Some(root) = self.root.as_deref_mut() {
            set_expanded_at_pos(root, pos, &mut 0i32, expand);
        }
    }
}

impl View for Outline {
    fn state(&self) -> &ViewState {
        &self.ov.state
    }
    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.ov.state
    }
    fn draw(&mut self, ctx: &mut DrawCtx) {
        ov_draw(self, ctx);
    }
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        ov_handle_event(self, ev, ctx);
    }
    fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
        ov_set_state(self, flag, enable, ctx);
    }

    /// `TScroller::changeBounds` — re-publish scrollbar range/page params with
    /// the stored `limit` and the new `size` after the pump applies new bounds
    /// (B5, identical to the Scroller override — Outline inherits from TScroller).
    fn on_bounds_changed(&mut self, ctx: &mut Context) {
        let (x, y) = (self.ov().limit.x, self.ov().limit.y);
        self.ov_mut().set_limit(x, y, ctx);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use crate::view::{Deferred, Group};
    use std::collections::VecDeque;

    fn make_ctx<'a>(
        out: &'a mut VecDeque<Event>,
        timers: &'a mut crate::timer::TimerQueue,
        deferred: &'a mut Vec<Deferred>,
    ) -> Context<'a> {
        Context::new(out, timers, 0, deferred)
    }

    /// Mint a real `ViewId` by inserting a throwaway view into a group.
    fn mint_id() -> (Group, ViewId) {
        let mut g = Group::new(Rect::new(0, 0, 4, 4));
        let id = g.insert(Box::new(Outline::new(
            Rect::new(0, 0, 1, 1),
            None,
            None,
            None,
        )));
        (g, id)
    }

    /// Build: root "Animals" with children "Cats" → "Dogs".
    fn animals_tree() -> Box<Node> {
        let children = Box::new(Node::new("Cats").with_next(Box::new(Node::new("Dogs"))));
        Box::new(Node::new("Animals").with_children(children))
    }

    // -- TNode ----------------------------------------------------------------

    #[test]
    fn node_new_defaults() {
        let n = Node::new("hi");
        assert_eq!(n.text, "hi");
        assert!(n.child_list.is_none());
        assert!(n.next.is_none());
        assert!(n.expanded, "new node is expanded");
    }

    #[test]
    fn node_builders() {
        let n = Node::new("a")
            .with_next(Box::new(Node::new("b")))
            .with_children(Box::new(Node::new("c")))
            .with_expanded(false);
        assert_eq!(n.next.as_ref().unwrap().text, "b");
        assert_eq!(n.child_list.as_ref().unwrap().text, "c");
        assert!(!n.expanded);
    }

    // -- traversal / getChild / getNumChildren --------------------------------

    #[test]
    fn traverse_dfs_positions_and_flags() {
        let o = Outline::new(Rect::new(0, 0, 20, 10), None, None, Some(animals_tree()));
        let mut visited: Vec<(String, i32, i32, u16)> = Vec::new();
        traverse(&o, &mut |this: &Outline,
                           node,
                           level,
                           position,
                           _lines,
                           flags| {
            visited.push((this.get_text(node).to_string(), level, position, flags));
            false
        });
        // Animals (level 0, pos 0, has children + expanded), Cats (level 1, pos 1),
        // Dogs (level 1, pos 2, last child).
        assert_eq!(visited.len(), 3);
        assert_eq!(visited[0].0, "Animals");
        assert_eq!(visited[0].1, 0);
        assert_eq!(visited[0].2, 0);
        assert_ne!(visited[0].3 & OV_CHILDREN, 0, "Animals has children");
        assert_ne!(visited[0].3 & OV_LAST, 0, "Animals is the only root → last");

        assert_eq!(visited[1].0, "Cats");
        assert_eq!(visited[1].1, 1);
        assert_eq!(visited[1].2, 1);
        assert_eq!(visited[1].3 & OV_LAST, 0, "Cats is not the last child");

        assert_eq!(visited[2].0, "Dogs");
        assert_eq!(visited[2].2, 2);
        assert_ne!(visited[2].3 & OV_LAST, 0, "Dogs is the last child");
    }

    #[test]
    fn collapsed_node_hides_children() {
        // Collapse the root: only "Animals" is visible.
        let mut root = animals_tree();
        root.expanded = false;
        let o = Outline::new(Rect::new(0, 0, 20, 10), None, None, Some(root));
        let mut count = 0;
        traverse(&o, &mut |_o: &Outline, _n, _l, _p, _li, _f| {
            count += 1;
            false
        });
        assert_eq!(count, 1, "collapsed root hides its children");
    }

    #[test]
    fn get_child_and_num_children() {
        let o = Outline::new(Rect::new(0, 0, 20, 10), None, None, Some(animals_tree()));
        let root = o.get_root().unwrap();
        assert_eq!(o.get_num_children(root), 2);
        assert_eq!(o.get_child(root, 0).unwrap().text, "Cats");
        assert_eq!(o.get_child(root, 1).unwrap().text, "Dogs");
        assert!(o.get_child(root, 2).is_none());
    }

    // -- createGraph ----------------------------------------------------------

    #[test]
    fn create_graph_root_shapes() {
        // levWidth = endWidth = 3, level 0.
        let chars: [char; 8] = [' ', '│', '├', '└', '─', '─', '+', '─'];
        // Last child, no children → "└──".
        let g = create_graph(0, 0, OV_EXPANDED | OV_LAST, 3, 3, &chars);
        assert_eq!(g, "└──");
        // Not last, has children, expanded → "├──".
        let g = create_graph(0, 0, OV_EXPANDED | OV_CHILDREN, 3, 3, &chars);
        assert_eq!(g, "├──");
        // Last, has children, collapsed (not expanded) → "└─+".
        let g = create_graph(
            0,
            0,
            OV_CHILDREN /* but not EXPANDED */ | OV_LAST,
            3,
            3,
            &chars,
        );
        // Note OV_CHILDREN without OV_EXPANDED is not a real traversal state, but
        // exercises the retract char path.
        assert_eq!(g, "└─+");
    }

    #[test]
    fn create_graph_level_indent() {
        let chars: [char; 8] = [' ', '│', '├', '└', '─', '─', '+', '─'];
        // Level 1, continuation bar at level 0 (lines bit 0 set), last child.
        let g = create_graph(1, 0b1, OV_EXPANDED | OV_LAST, 3, 3, &chars);
        // 3 chars indent ("│  ") + 3 end chars ("└──").
        assert_eq!(g, "│  └──");
        // Level 1, no continuation bar.
        let g = create_graph(1, 0, OV_EXPANDED | OV_LAST, 3, 3, &chars);
        assert_eq!(g, "   └──");
    }

    // -- ov_update / set_limit ------------------------------------------------

    #[test]
    fn ov_update_counts_and_sets_limit() {
        let (_gh, h) = mint_id();
        let (_gv, v) = mint_id();
        let mut o = Outline::new(
            Rect::new(0, 0, 20, 5),
            Some(h),
            Some(v),
            Some(animals_tree()),
        );
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            ov_update(&mut o, &mut ctx);
        }
        // 3 visible nodes → limit.y = 3. max_x: "Animals" graph 3 + text 7 = 10;
        // "Cats"/"Dogs" graph 6 + text 4 = 10. → 10.
        assert_eq!(o.ov().limit, Point::new(10, 3));
    }

    // -- adjust (expand/collapse via DFS position) ----------------------------

    #[test]
    fn adjust_collapses_and_expands_by_position() {
        let mut o = Outline::new(Rect::new(0, 0, 20, 5), None, None, Some(animals_tree()));
        // Collapse position 0 (root).
        o.adjust(0, false);
        assert!(!o.root.as_ref().unwrap().expanded);
        // Re-expand.
        o.adjust(0, true);
        assert!(o.root.as_ref().unwrap().expanded);
        // Collapse a child (position 1 = "Cats").
        o.adjust(1, false);
        assert!(
            !o.root
                .as_ref()
                .unwrap()
                .child_list
                .as_ref()
                .unwrap()
                .expanded
        );
    }

    // -- ov_expand_all --------------------------------------------------------

    #[test]
    fn expand_all_expands_subtree_not_siblings() {
        // root "R" with children A (collapsed, with child A1) and B (collapsed,
        // with child B1). Start at A; expanding all from A must expand A (and A1's
        // parent chain) but NOT B.
        let a1 = Box::new(Node::new("A1"));
        let a = Box::new(Node::new("A").with_children(a1).with_expanded(false));
        let b1 = Box::new(Node::new("B1"));
        let b = Box::new(Node::new("B").with_children(b1).with_expanded(false));
        let children = Box::new(*a).with_next(b);
        let root = Box::new(Node::new("R").with_children(Box::new(children)));
        let mut o = Outline::new(Rect::new(0, 0, 20, 10), None, None, Some(root));

        // Visible: R(0), A(1), B(2). expandAll from A (position 1).
        ov_expand_all(&mut o, 1);

        // A must now be expanded; B must remain collapsed.
        let root = o.root.as_ref().unwrap();
        let a = root.child_list.as_ref().unwrap();
        assert_eq!(a.text, "A");
        assert!(a.expanded, "A expanded by expandAll");
        let b = a.next.as_ref().unwrap();
        assert_eq!(b.text, "B");
        assert!(!b.expanded, "B (sibling of A) NOT expanded");
    }

    // -- handle_event: scrollbar filter --------------------------------------

    #[test]
    fn scrollbar_changed_filter_requests_sync_only_for_own_bars() {
        let mut group = Group::new(Rect::new(0, 0, 30, 20));
        let (_gh, h) = mint_id();
        let (_gv, v) = mint_id();
        let id = group.insert(Box::new(Outline::new(
            Rect::new(0, 0, 10, 5),
            Some(h),
            Some(v),
            Some(animals_tree()),
        )));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        // CHANGED from own h-bar → SyncOutlineViewerDelta queued.
        let mut ev = Event::Broadcast {
            command: Command::SCROLL_BAR_CHANGED,
            source: Some(h),
        };
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            group.find_mut(id).unwrap().handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(deferred.len(), 1);
        assert!(matches!(
            deferred[0],
            Deferred::SyncOutlineViewerDelta { viewer, h: rh, v: rv }
                if viewer == id && rh == Some(h) && rv == Some(v)
        ));

        // CHANGED from a foreign source → nothing.
        deferred.clear();
        let mut ev = Event::Broadcast {
            command: Command::SCROLL_BAR_CHANGED,
            source: Some(id),
        };
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            group.find_mut(id).unwrap().handle_event(&mut ev, &mut ctx);
        }
        assert!(deferred.is_empty(), "foreign source ignored");
    }

    // -- handle_event: keyboard nav ------------------------------------------

    #[test]
    fn key_down_moves_focus() {
        let mut o = Outline::new(Rect::new(0, 0, 20, 5), None, None, Some(animals_tree()));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];
        // limit.y must be set for adjust_focus clamping.
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            ov_update(&mut o, &mut ctx);
        }
        let mut ev = Event::KeyDown(crate::event::KeyEvent::new(
            Key::Down,
            crate::event::KeyModifiers::default(),
        ));
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            o.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(o.ov().foc, 1, "Down → focus position 1");
        assert!(ev.is_nothing(), "Down consumed");
    }

    #[test]
    fn key_minus_collapses_focused() {
        let mut o = Outline::new(Rect::new(0, 0, 20, 5), None, None, Some(animals_tree()));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            ov_update(&mut o, &mut ctx);
        }
        // foc = 0 (root). '-' collapses it.
        let mut ev = Event::KeyDown(crate::event::KeyEvent::new(
            Key::Char('-'),
            crate::event::KeyModifiers::default(),
        ));
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            o.handle_event(&mut ev, &mut ctx);
        }
        assert!(!o.root.as_ref().unwrap().expanded, "'-' collapsed the root");
        assert!(ev.is_nothing(), "'-' consumed");
    }

    // -- draw snapshot --------------------------------------------------------

    fn render_outline(outline: &mut Outline, w: u16, h: u16) -> String {
        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(w, h);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = outline.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            outline.draw(&mut dc);
        });
        screen.snapshot()
    }

    #[test]
    fn draw_simple_tree() {
        let mut outline = Outline::new(Rect::new(0, 0, 20, 5), None, None, Some(animals_tree()));
        // Mark focused so the focused-row color branch applies.
        outline.ov_mut().state.state.focused = true;
        outline.ov_mut().foc = 0;
        // Set the limit by hand (ov_update needs a Context); the draw only reads
        // delta/size/foc, so limit is not strictly required for this snapshot.
        outline.ov_mut().limit = Point::new(10, 3);
        insta::assert_snapshot!(render_outline(&mut outline, 20, 5));
    }

    #[test]
    fn draw_focused_collapsed_parent_uses_focused_text_color() {
        // A focused, COLLAPSED parent must draw its text in the Focused color
        // (C++ `color >> 8` of the 0x0202 pair = Focused), NOT the NotExpanded
        // dim color — the case the expanded `draw_simple_tree` snapshot can't see.
        let mut root = animals_tree();
        root.expanded = false; // collapse the root → only "Animals" visible
        let mut outline = Outline::new(Rect::new(0, 0, 20, 5), None, None, Some(root));
        outline.ov_mut().state.state.focused = true;
        outline.ov_mut().foc = 0; // the collapsed parent is focused
        outline.ov_mut().limit = Point::new(10, 1);
        // The "Animals" row's text attr must be the Focused style (white-on-blue),
        // not NotExpanded (darkgray-on-blue) — the snapshot legend makes this
        // visible and a regression to `not_expanded_color` would change it.
        insta::assert_snapshot!(render_outline(&mut outline, 20, 5));
    }

    // -- A3 mouse-track seam: Outline (D9 adoption) ---------------------------
    //
    // These tests verify that MouseDown arms tracking with the view-id payload,
    // MouseMove/MouseAuto route focus, MouseAuto out-of-view scrolls after the
    // skip count, and MouseUp performs the post-loop graph-toggle logic:
    // drag (dragged >= 2) must NOT toggle; a click (dragged < 2) DOES toggle.

    fn ov_mouse_down(x: i32, y: i32) -> Event {
        Event::MouseDown(crate::event::MouseEvent {
            position: Point::new(x, y),
            buttons: crate::event::MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        })
    }

    fn ov_mouse_move(x: i32, y: i32) -> Event {
        Event::MouseMove(crate::event::MouseEvent {
            position: Point::new(x, y),
            buttons: crate::event::MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        })
    }

    fn ov_mouse_auto(x: i32, y: i32) -> Event {
        Event::MouseAuto(crate::event::MouseEvent {
            position: Point::new(x, y),
            ..Default::default()
        })
    }

    fn ov_mouse_up(x: i32, y: i32) -> Event {
        Event::MouseUp(crate::event::MouseEvent {
            position: Point::new(x, y),
            ..Default::default()
        })
    }

    fn ov_double_click(x: i32, y: i32) -> Event {
        Event::MouseDown(crate::event::MouseEvent {
            position: Point::new(x, y),
            flags: crate::event::MouseEventFlags {
                double_click: true,
                ..Default::default()
            },
            buttons: crate::event::MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        })
    }

    /// Stamp an outline with a ViewId (as Group::insert would do).
    fn give_ov_id(o: &mut Outline) -> ViewId {
        let id = ViewId::next();
        o.ov.state.id = Some(id);
        id
    }

    /// `MouseDown` (non-double-click) on an inserted outline: arms tracking
    /// with the correct view-id payload in the PushCapture deferred.
    #[test]
    fn ov_mouse_down_arms_tracking_and_pushes_capture() {
        // 20×5 outline, 3-node "Animals" tree (limit.y = 3 after ov_update).
        let mut o = Outline::new(Rect::new(0, 0, 20, 5), None, None, Some(animals_tree()));
        let id = give_ov_id(&mut o);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            ov_update(&mut o, &mut ctx);
        }
        deferred.clear();

        // Click at (5, 1): delta.y=0, i = 0+1=1 < limit_y=3 → new_focus = 1.
        let mut ev = ov_mouse_down(5, 1);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            o.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "MouseDown consumed");
        assert_eq!(o.ov().foc, 1, "focus positioned to item 1");
        assert!(o.ov().track.is_some(), "track state armed");
        // The PushCapture deferred must name this outline's id.
        assert_eq!(deferred.len(), 1, "one PushCapture deferred");
        assert!(
            matches!(deferred[0], Deferred::PushCapture(_)),
            "deferred[0] is PushCapture"
        );
        if let Deferred::PushCapture(ref h) = deferred[0] {
            assert_eq!(h.view(), Some(id), "capture tracks the outline's id");
        }
    }

    /// `MouseDown` without an id: single-shot behavior, no tracking.
    #[test]
    fn ov_mouse_down_without_id_is_single_shot() {
        let mut o = Outline::new(Rect::new(0, 0, 20, 5), None, None, Some(animals_tree()));
        // No id.
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            ov_update(&mut o, &mut ctx);
        }
        deferred.clear();

        let mut ev = ov_mouse_down(5, 1);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            o.handle_event(&mut ev, &mut ctx);
        }
        assert!(o.ov().track.is_none(), "no track without an id");
        assert!(
            deferred
                .iter()
                .all(|d| !matches!(d, Deferred::PushCapture(_))),
            "no capture pushed for id-less outline"
        );
    }

    /// `MouseMove` in-view while tracking: repositions focus.
    #[test]
    fn ov_mouse_move_in_view_recomputes_focus() {
        let mut o = Outline::new(Rect::new(0, 0, 20, 5), None, None, Some(animals_tree()));
        give_ov_id(&mut o);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            ov_update(&mut o, &mut ctx);
        }
        deferred.clear();

        // Arm tracking with a click at row 0.
        let mut ev = ov_mouse_down(5, 0);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            o.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(o.ov().foc, 0);
        deferred.clear();

        // MouseMove to row 2: i = delta.y(0) + 2 = 2 < limit_y(3) → new_focus = 2.
        let mut ev = ov_mouse_move(5, 2);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            o.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "MouseMove consumed");
        assert_eq!(o.ov().foc, 2, "focus moves to row 2");
        assert!(o.ov().track.is_some(), "still tracking after move");
        // dragged increments: was 1 after down, now 2.
        assert_eq!(o.ov().track.unwrap().dragged, 2, "dragged incremented");
    }

    /// `MouseAuto` out-of-view (tracking): skips 2 ticks, then steps on the 3rd.
    #[test]
    fn ov_mouse_auto_out_of_view_skips_then_steps() {
        let mut o = Outline::new(Rect::new(0, 0, 20, 5), None, None, Some(animals_tree()));
        give_ov_id(&mut o);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            ov_update(&mut o, &mut ctx);
        }
        // Focus item 1 (Cats), arm tracking manually.
        o.ov_mut().foc = 1;
        o.ov_mut().track = Some(OvTrack {
            count: 0,
            dragged: 1,
        });
        deferred.clear();

        // 2 ticks below (y >= size.y = 5): count reaches 1, 2 — no step.
        for tick in 1..=2 {
            let mut ev = ov_mouse_auto(5, 6); // y=6 >= size.y=5
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            o.handle_event(&mut ev, &mut ctx);
            assert_eq!(o.ov().foc, 1, "tick {tick}: no step yet");
        }
        // 3rd tick: count == MOUSE_AUTO_TO_SKIP → step forward.
        let mut ev = ov_mouse_auto(5, 6);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            o.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(o.ov().foc, 2, "3rd tick steps focus by +1 (below view)");
    }

    /// `MouseUp` while tracking + `dragged < 2` (click): graph-toggle fires if
    /// the release x is within the graph column.
    #[test]
    fn ov_mouse_up_click_toggles_graph_expansion() {
        // Build a tree with Animals (expanded, has children) at position 0.
        // Animals at level 0 → graph_w = 0*3 + 3 = 3. Click at x < 3 → toggle.
        let mut o = Outline::new(Rect::new(0, 0, 20, 5), None, None, Some(animals_tree()));
        give_ov_id(&mut o);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            ov_update(&mut o, &mut ctx);
        }
        o.ov_mut().foc = 0;
        // Arm tracking with dragged = 1 (click — fewer than 2 iterations).
        o.ov_mut().track = Some(OvTrack {
            count: 0,
            dragged: 1,
        });
        deferred.clear();

        // MouseUp at (1, 0): x=1 < graph_w=3 → should toggle Animals (expanded → collapsed).
        assert!(o.root.as_ref().unwrap().expanded, "Animals starts expanded");
        let mut ev = ov_mouse_up(1, 0);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            o.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "MouseUp consumed");
        assert!(o.ov().track.is_none(), "track cleared on MouseUp");
        assert!(
            !o.root.as_ref().unwrap().expanded,
            "Animals collapsed after click on graph"
        );
    }

    /// `MouseUp` while tracking + `dragged >= 2` (drag): graph-toggle does NOT
    /// fire — the drag discriminator (toutline.cpp:469 `if (dragged < 2)`) gates it.
    #[test]
    fn ov_mouse_up_drag_does_not_toggle_graph() {
        let mut o = Outline::new(Rect::new(0, 0, 20, 5), None, None, Some(animals_tree()));
        give_ov_id(&mut o);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            ov_update(&mut o, &mut ctx);
        }
        o.ov_mut().foc = 0;
        // Arm tracking with dragged = 2 (drag — 2+ iterations).
        o.ov_mut().track = Some(OvTrack {
            count: 0,
            dragged: 2,
        });
        deferred.clear();

        assert!(o.root.as_ref().unwrap().expanded, "Animals starts expanded");
        let mut ev = ov_mouse_up(1, 0); // x < graph_w — would toggle without the guard
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            o.handle_event(&mut ev, &mut ctx);
        }
        assert!(o.ov().track.is_none(), "track cleared");
        assert!(
            o.root.as_ref().unwrap().expanded,
            "Animals still expanded (drag must NOT toggle graph)"
        );
    }

    /// `MouseUp` outside the graph column (click): does NOT toggle.
    #[test]
    fn ov_mouse_up_click_outside_graph_no_toggle() {
        let mut o = Outline::new(Rect::new(0, 0, 20, 5), None, None, Some(animals_tree()));
        give_ov_id(&mut o);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            ov_update(&mut o, &mut ctx);
        }
        o.ov_mut().foc = 0;
        o.ov_mut().track = Some(OvTrack {
            count: 0,
            dragged: 1,
        });
        deferred.clear();

        // graph_w = 3; release at x = 5 (outside graph) → no toggle.
        let mut ev = ov_mouse_up(5, 0);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            o.handle_event(&mut ev, &mut ctx);
        }
        assert!(o.root.as_ref().unwrap().expanded, "Animals still expanded");
    }

    /// Stray `MouseUp` (not tracking) falls through unconsumed.
    #[test]
    fn ov_stray_mouse_up_falls_through() {
        let mut o = Outline::new(Rect::new(0, 0, 20, 5), None, None, Some(animals_tree()));
        give_ov_id(&mut o);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        let mut ev = ov_mouse_up(5, 2);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            o.handle_event(&mut ev, &mut ctx);
        }
        assert!(
            !ev.is_nothing(),
            "stray MouseUp falls through (not consumed)"
        );
    }

    /// Stray `MouseMove` (not tracking) falls through unconsumed.
    #[test]
    fn ov_stray_mouse_move_falls_through() {
        let mut o = Outline::new(Rect::new(0, 0, 20, 5), None, None, Some(animals_tree()));
        give_ov_id(&mut o);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        let mut ev = ov_mouse_move(5, 2);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            o.handle_event(&mut ev, &mut ctx);
        }
        assert!(
            !ev.is_nothing(),
            "stray MouseMove falls through (not consumed)"
        );
    }

    /// Double-click: calls `selected(foc)`, no tracking armed.
    #[test]
    fn ov_double_click_calls_selected_no_tracking() {
        // Override selected() to record the call by changing foc to a sentinel.
        struct CountingOutline {
            ov: OutlineViewerState,
            root: Option<Box<Node>>,
            selected_called: bool,
        }
        impl OutlineViewer for CountingOutline {
            fn ov(&self) -> &OutlineViewerState {
                &self.ov
            }
            fn ov_mut(&mut self) -> &mut OutlineViewerState {
                &mut self.ov
            }
            fn get_root(&self) -> Option<&Node> {
                self.root.as_deref()
            }
            fn get_next<'a>(&'a self, n: &'a Node) -> Option<&'a Node> {
                n.next.as_deref()
            }
            fn get_child<'a>(&'a self, n: &'a Node, i: i32) -> Option<&'a Node> {
                let mut p = n.child_list.as_deref();
                let mut idx = 0;
                while idx < i {
                    p = p.and_then(|x| x.next.as_deref());
                    idx += 1;
                }
                p
            }
            fn get_num_children(&self, n: &Node) -> i32 {
                let mut i = 0;
                let mut p = n.child_list.as_deref();
                while let Some(x) = p {
                    i += 1;
                    p = x.next.as_deref();
                }
                i
            }
            fn get_text<'a>(&'a self, n: &'a Node) -> &'a str {
                &n.text
            }
            fn is_expanded(&self, n: &Node) -> bool {
                n.expanded
            }
            fn has_children(&self, n: &Node) -> bool {
                n.child_list.is_some()
            }
            fn adjust(&mut self, pos: i32, expand: bool) {
                if let Some(root) = self.root.as_deref_mut() {
                    set_expanded_at_pos(root, pos, &mut 0i32, expand);
                }
            }
            fn selected(&mut self, _i: i32) {
                self.selected_called = true;
            }
        }
        impl View for CountingOutline {
            fn state(&self) -> &ViewState {
                &self.ov.state
            }
            fn state_mut(&mut self) -> &mut ViewState {
                &mut self.ov.state
            }
            fn draw(&mut self, ctx: &mut DrawCtx) {
                ov_draw(self, ctx);
            }
            fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
                ov_handle_event(self, ev, ctx);
            }
            fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
                ov_set_state(self, flag, enable, ctx);
            }
            fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
                Some(self)
            }
        }

        let mut o = CountingOutline {
            ov: OutlineViewerState::new(Rect::new(0, 0, 20, 5), None, None),
            root: Some(animals_tree()),
            selected_called: false,
        };
        let id = ViewId::next();
        o.ov.state.id = Some(id);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            ov_update(&mut o, &mut ctx);
        }
        deferred.clear();

        // Double-click at row 1.
        let mut ev = ov_double_click(5, 1);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            o.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "double-click consumed");
        assert!(o.selected_called, "selected() called on double-click");
        assert!(o.ov.track.is_none(), "no tracking armed on double-click");
        assert!(
            deferred
                .iter()
                .all(|d| !matches!(d, Deferred::PushCapture(_))),
            "double-click does NOT arm tracking"
        );
    }
}
