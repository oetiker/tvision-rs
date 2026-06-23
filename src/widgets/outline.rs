//! A collapsible tree view: [`Node`], [`OutlineViewer`], and [`Outline`].
//!
//! # A trait, not a concrete struct
//!
//! An outline viewer's overridable methods
//! (`get_root`/`get_next`/`get_child`/`get_text`/`is_expanded`/`has_children`/`adjust`)
//! are called from inside the base's own `draw`/`handle_event`/`update` — the
//! same constraint that made
//! [`ListViewer`](crate::widgets::list_viewer::ListViewer) a trait rather than an
//! embedded struct. So it follows the same shape: [`OutlineViewer`] carries the
//! overridable methods, [`OutlineViewerState`] carries the data members, and the
//! shared draw / event / traversal logic lives as **free functions generic over
//! `<L: OutlineViewer + ?Sized>`** so a concrete widget's `View` impl reuses them
//! verbatim.
//!
//! # The cross-view scrollbar read-sync
//!
//! An outline viewer holds only `&mut Context` during dispatch and so can neither
//! read nor mutate its window-frame sibling scroll bars. The pump is the broker:
//! on a [`SCROLL_BAR_CHANGED`](crate::command::Command::SCROLL_BAR_CHANGED)
//! broadcast naming one of its bars as `source`, the viewer requests
//! [`Deferred::ScrollSync`](crate::view::Deferred::ScrollSync);
//! the pump reads both bars' `value`s and calls `apply_scroll_sync`, which writes
//! the resulting `(dx, dy)` into the viewer's [`delta`](OutlineViewerState::delta).
//! This read-sync is **read-only** (no focus write-back), so it terminates without
//! a change-guard.
//!
//! # Mouse press-and-hold
//!
//! A mouse-down arms the mouse-track capture; the subsequent move/auto/up events
//! route the hold loop, auto-scrolling when the mouse moves out of view. A
//! `dragged` gate distinguishes a click (which toggles the node's expand state)
//! from a drag.
//!
//! # Colors
//!
//! Each role is a [`Role`]: [`Role::OutlineNormal`] / [`Role::OutlineFocused`] /
//! [`Role::OutlineSelected`] / [`Role::OutlineNotExpanded`].
//!
//! # Construction
//!
//! Building an outline does not publish its scroll-bar parameters, because that
//! needs a [`Context`] that is not available at construction. The consumer calls
//! [`ov_update`] once after inserting the outline into a group (the same
//! constraint the scroller and list-viewer constructors hit — see
//! [`Outline::new`]).
//!
//! # Turbo Vision heritage
//!
//! Ports `TNode`, `TOutlineViewer`, and `TOutline` (`toutline.cpp`). The
//! viewer's abstract-class inheritance becomes a trait plus a state struct plus
//! generic free functions (D2), because the base's draw must call back into the
//! subclass's overrides. Owner back-pointers to the sibling scroll bars become
//! [`ViewId`] handles brokered by the event loop (D3), and the palette becomes
//! [`Role`]s. The node's recursive destructor becomes the automatic `Box<Node>`
//! drop.

use crate::capture::TrackMask;
use crate::command::Command;
use crate::event::{Event, Key, ctrl_to_arrow};
use crate::theme::Role;
use crate::view::{
    Context, DrawCtx, GrowMode, Options, Point, Rect, StateFlag, View, ViewId, ViewState,
};

/// Graph flag: the node is drawn as expanded (no children, or expanded).
const OV_EXPANDED: u16 = 0x01;
/// Graph flag: the node has children AND is expanded (draw the child-link).
const OV_CHILDREN: u16 = 0x02;
/// Graph flag: the node is the last child of its parent (└ vs ├).
const OV_LAST: u16 = 0x04;

/// Number of auto-repeat ticks to accumulate before stepping the focus by ±1
/// when the mouse is held outside the view.
const MOUSE_AUTO_TO_SKIP: i32 = 3;

/// Per-hold mouse-tracking state for the outline viewer.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OvTrack {
    /// Accumulated auto-repeat ticks since the last step/reset.
    count: i32,
    /// Iteration counter, capped at 2. After the hold, `dragged < 2`
    /// distinguishes a "click" from a "drag": only a click can toggle the graph
    /// expansion column.
    dragged: u8,
}

// ---------------------------------------------------------------------------
// Node — the tree node
// ---------------------------------------------------------------------------

/// One outline tree node.
///
/// A node owns its first child (`child_list`) and its next sibling (`next`), both
/// as `Option<Box<Node>>`; the recursive `Box` drop frees the whole subtree
/// automatically when the node is dropped or replaced.
/// `text` is the displayed label; `expanded` controls whether children are shown
/// in the outline viewer.
///
/// Build a tree with [`Node::new`] and the builder methods, then pass the root
/// to [`Outline::new`]:
///
/// ```rust
/// use tvision_rs::widgets::outline::Node;
/// let root = Box::new(
///     Node::new("Animals")
///         .with_children(Box::new(
///             Node::new("Cats")
///                 .with_next(Box::new(Node::new("Dogs")))
///         ))
/// );
/// ```
///
/// # Turbo Vision heritage
///
/// Ports `TNode` (`toutline.cpp`); the recursive node destructor becomes the
/// automatic `Box<Node>` drop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The text displayed in the outline row for this node.
    ///
    /// Displayed verbatim; assign a new value and call [`ov_update`] to refresh
    /// the column widths.
    pub text: String,
    /// The first child of this node, or `None` if the node is a leaf.
    ///
    /// Siblings of the first child chain through their [`Node::next`] fields.
    /// Set this (or use [`Node::with_children`]) before inserting the node into
    /// an [`Outline`]; then call [`ov_update`] to publish the new extents.
    pub child_list: Option<Box<Node>>,
    /// The next sibling of this node, or `None` if this is the last sibling.
    ///
    /// Use [`Node::with_next`] in the builder chain, or assign directly when
    /// building a sibling list manually.
    pub next: Option<Box<Node>>,
    /// Whether this node's children are currently shown (`true`) or hidden
    /// (`false`).
    ///
    /// New nodes default to `true` (expanded). Set to `false` via
    /// [`Node::with_expanded`] to start collapsed, or let the user collapse
    /// the node interactively via the `-` key or a click on the graph column.
    pub expanded: bool,
}

impl Node {
    /// Create a leaf node with the given display text.
    ///
    /// The new node has no children or next sibling and starts expanded.
    /// Chain [`with_children`](Self::with_children), [`with_next`](Self::with_next),
    /// and [`with_expanded`](Self::with_expanded) to build a full subtree before
    /// boxing and passing to [`Outline::new`].
    pub fn new(text: impl Into<String>) -> Self {
        Node {
            text: text.into(),
            child_list: None,
            next: None,
            expanded: true,
        }
    }

    /// Attach a next sibling to this node and return `self`.
    ///
    /// The next sibling (and its own `next` chain) is freed automatically when
    /// this node is dropped. To build a sibling list for use as children, chain
    /// `with_next` calls starting from the last sibling:
    ///
    /// ```rust
    /// use tvision_rs::widgets::outline::Node;
    /// // "Cats" → "Dogs" sibling pair.
    /// let siblings = Box::new(Node::new("Cats").with_next(Box::new(Node::new("Dogs"))));
    /// ```
    pub fn with_next(mut self, next: Box<Node>) -> Self {
        self.next = Some(next);
        self
    }

    /// Attach a first child to this node and return `self`.
    ///
    /// Additional children are siblings of `children`, chained via
    /// [`with_next`](Self::with_next). The entire subtree is freed automatically
    /// when this node is dropped.
    pub fn with_children(mut self, children: Box<Node>) -> Self {
        self.child_list = Some(children);
        self
    }

    /// Set whether this node starts expanded or collapsed and return `self`.
    ///
    /// Pass `false` to start collapsed; the user can then expand it with the
    /// `+` key or a click on the graph column. When `false`, [`Node::child_list`]
    /// is owned but not shown until expanded.
    pub fn with_expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }
}

// ---------------------------------------------------------------------------
// OutlineViewerState — the data members
// ---------------------------------------------------------------------------

/// The shared state of every outline viewer. A concrete outline widget embeds
/// one and exposes it via [`OutlineViewer::ov`]/[`OutlineViewer::ov_mut`].
///
/// # Turbo Vision heritage
///
/// The data half of `TOutlineViewer` (`toutline.cpp`) plus the scroll
/// offset/limit it inherits from the scroller base.
pub struct OutlineViewerState {
    /// View state (geometry, flags, …) — the `View` composition target.
    pub state: ViewState,
    /// The scroll offset: `x` is the number of character columns to skip on
    /// the left; `y` is the DFS position of the first visible row.
    ///
    /// Read `delta` to find out which part of the tree is visible. Do not assign
    /// it directly — changes come from scrollbar broadcasts that the pump
    /// resolves and applies via [`apply_delta`](Self::apply_delta).
    pub delta: Point,
    /// The content extent: `x` is the maximum graph+text width (in character
    /// columns); `y` is the total number of currently-visible nodes.
    ///
    /// Updated by [`ov_update`] after every tree mutation. Read it to know the
    /// scrollable content size; do not assign directly.
    pub limit: Point,
    /// The horizontal scrollbar view id, or `None` if none was wired.
    ///
    /// Set at construction via [`OutlineViewerState::new`]; the outline viewer
    /// reads and writes its value through the pump broker.
    pub h_scroll_bar: Option<ViewId>,
    /// The vertical scrollbar view id, or `None` if none was wired.
    ///
    /// Set at construction via [`OutlineViewerState::new`]; the outline viewer
    /// reads and writes its value through the pump broker.
    pub v_scroll_bar: Option<ViewId>,
    /// The DFS position (0-based) of the focused node.
    ///
    /// Read this to find out which node is focused. To move the focus
    /// programmatically, call [`adjust_focus`] (it also scrolls the focused row
    /// into view and notifies the trait via
    /// [`OutlineViewer::focused_item`]). Do not assign directly.
    pub foc: i32,
    /// Absolute screen position of this view's `(0, 0)`, cached by the last
    /// `draw` call — feeds the [`MouseTrackCapture`] origin.
    pub(crate) abs_origin: Point,
    /// Per-hold mouse-tracking state — `Some` while a track is in flight
    /// (between `MouseDown` and `MouseUp`), `None` otherwise. Guards the
    /// tracking arms against stray (untracked) events.
    pub(crate) track: Option<OvTrack>,
}

impl OutlineViewerState {
    /// Create the shared state for an outline viewer.
    ///
    /// `bounds` sets the initial geometry; `h` and `v` are the optional
    /// horizontal and vertical scrollbar ids (pass `None` when scrollbars are
    /// absent). The view starts selectable, grows with its lower-right corner,
    /// and focuses at DFS position 0 with a zero scroll offset.
    ///
    /// After embedding this state and inserting the widget into a group, call
    /// [`ov_update`] with the resulting [`Context`] to publish the scrollbar
    /// range and page parameters — those require a live `Context` that is not
    /// available at construction time.
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

    /// Set the content extent and (re)publish each bar's range/page params.
    /// Same formula as [`Scroller::set_limit`](crate::widgets::Scroller::set_limit).
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

    /// Set each bar's value (preserving range and steps).
    /// Same as [`Scroller::scroll_to`](crate::widgets::Scroller::scroll_to).
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

    /// Show/hide one bar per this viewer's active/selected state: shown when
    /// either is set, hidden otherwise.
    fn show_sbar(&self, sbar: Option<ViewId>, ctx: &mut Context) {
        if let Some(id) = sbar {
            let visible = self.state.state.active || self.state.state.selected;
            ctx.request_set_visible(id, visible);
        }
    }
}

// ---------------------------------------------------------------------------
// OutlineViewer — the overridable methods (a trait)
// ---------------------------------------------------------------------------

/// The abstract outline-viewer base, as a trait of overridable methods.
/// Concrete outline widgets implement [`ov`](Self::ov)/[`ov_mut`](Self::ov_mut)
/// (the data accessors) and the tree-navigation methods; the shared draw / event
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
///
/// # Turbo Vision heritage
///
/// The trait half of `TOutlineViewer` (`toutline.cpp`): its overridable
/// tree-navigation virtuals, with the data members in [`OutlineViewerState`] and
/// the shared logic in this module's free functions.
pub trait OutlineViewer: View {
    /// Borrow the embedded [`OutlineViewerState`].
    fn ov(&self) -> &OutlineViewerState;
    /// Mutably borrow the embedded [`OutlineViewerState`].
    fn ov_mut(&mut self) -> &mut OutlineViewerState;

    // -- Abstract read-only virtuals (borrow `&Node`s out of `&self`) ---------

    /// Return the root node of the tree, or `None` if the tree is empty.
    ///
    /// Called at the start of every traversal (draw, update, expand-all). The
    /// concrete widget returns the root of its owned tree; the lifetime `'a`
    /// ties the returned reference to `&'a self` so that callers can keep it
    /// alive while iterating.
    fn get_root(&self) -> Option<&Node>;

    /// Return `node`'s next sibling, or `None` if `node` is the last sibling.
    ///
    /// The traversal walks siblings by repeatedly calling this method; `'a`
    /// ties the returned node to the same lifetime as `node` (which is itself
    /// already tied to `&'a self` at the call site).
    fn get_next<'a>(&'a self, node: &'a Node) -> Option<&'a Node>;

    /// Return the `i`-th child of `node` (0-based), or `None` if out of range.
    ///
    /// The traversal calls this with `i = 0` to obtain the first child, then
    /// follows [`get_next`](Self::get_next) for subsequent siblings. A concrete
    /// widget that stores children in a `Vec` may use `children.get(i as usize)`.
    fn get_child<'a>(&'a self, node: &'a Node, i: i32) -> Option<&'a Node>;

    /// Return the number of direct children of `node`.
    ///
    /// Used by the traversal and by [`ov_update`] to decide whether to recurse.
    /// Must agree with [`has_children`](Self::has_children): return `0` iff
    /// `has_children` returns `false`.
    fn get_num_children(&self, node: &Node) -> i32;

    /// Return the display text for `node`, borrowed from `self`.
    ///
    /// Displayed verbatim in the outline row to the right of the graph prefix.
    /// The lifetime `'a` allows returning a `&str` into the node's own storage
    /// without copying.
    fn get_text<'a>(&'a self, node: &'a Node) -> &'a str;

    /// Return `true` if `node`'s children are currently shown.
    ///
    /// The traversal recurses into `node` only when both `has_children` and
    /// `is_expanded` return `true`. For [`Outline`], this reads
    /// [`Node::expanded`]. Override to store the expanded state externally (for
    /// example in a `HashMap<NodeId, bool>`).
    fn is_expanded(&self, node: &Node) -> bool;

    /// Return `true` if `node` has at least one child.
    ///
    /// Determines whether the graph prefix shows an expand/collapse indicator
    /// and whether the traversal may recurse. Must agree with
    /// [`get_num_children`](Self::get_num_children).
    fn has_children(&self, node: &Node) -> bool;

    // -- Abstract mutation method ---------------------------------------------

    /// Set the expanded state of the node at DFS position `pos` in the
    /// **currently visible** tree (0-based, same numbering as
    /// [`foc`](OutlineViewerState::foc)).
    ///
    /// Called by the keyboard handler (`+`/`-`/`*` keys) and by the mouse
    /// handler when the user clicks the graph prefix column. After `adjust`
    /// returns, the caller invokes [`ov_update`] to recount visible nodes and
    /// republish scrollbar limits.
    ///
    /// The contract is keyed by DFS position rather than a node reference,
    /// because the shared free functions only hold `&self`; the concrete widget
    /// resolves `pos` to the owned node mutably.
    fn adjust(&mut self, pos: i32, expand: bool);

    // -- Overridable with defaults --------------------------------------------

    /// Notify the concrete widget that node at DFS position `i` received focus.
    ///
    /// The default writes `i` to [`OutlineViewerState::foc`]. Override to
    /// maintain additional per-item focus state, such as a selection set.
    /// Always call `self.ov_mut().foc = i` (or `super`) to keep the default
    /// rendering consistent.
    fn focused_item(&mut self, i: i32) {
        self.ov_mut().foc = i;
    }

    /// Return `true` if DFS position `i` should be drawn in the selected color.
    ///
    /// The default returns `i == foc` (single-selection mode). Override in a
    /// multi-select widget to highlight a range or set of positions in addition
    /// to the focused one.
    fn is_selected(&self, i: i32) -> bool {
        self.ov().foc == i
    }

    /// The user committed to DFS position `i` (double-click or Enter key).
    ///
    /// The default does nothing. Override to react — for example, broadcast
    /// [`Command::OUTLINE_ITEM_SELECTED`] so that an enclosing dialog can read
    /// the focused node and act on it.
    fn selected(&mut self, _i: i32) {}
}

// ---------------------------------------------------------------------------
// Traversal core — the DFS visitor
// ---------------------------------------------------------------------------

/// The inner recursion of a tree traversal: visit `node` and (if expanded) its
/// visible subtree, calling `action(this, node, level, position, lines, flags)`.
/// Returns `true` if the visitor stopped the traversal.
///
/// `flags`: [`OV_EXPANDED`] if the node is a leaf or expanded; [`OV_CHILDREN`]
/// if it has children and is expanded; [`OV_LAST`] if it is its parent's last
/// child. `lines`: bit N set means level N has a continuation bar at/below this
/// node. `position` is pre-incremented before each visit (so 0-based after the
/// first).
///
/// Root-level siblings are handled by the caller [`traverse`], NOT here — we
/// only ever enter this for a node already chosen by `traverse`.
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

/// DFS-visit every currently visible node, calling `action` for each.
///
/// The visitor signature is `(this, node, level, position, lines, flags) -> bool`,
/// where `level` is the nesting depth (0 = root), `position` is the 0-based DFS
/// counter, `lines` is a bitmask of levels that have a continuation bar, and
/// `flags` is a combination of `OV_EXPANDED`, `OV_CHILDREN`, and `OV_LAST`.
/// Returning `true` from `action` stops the traversal early (like C++ `firstThat`);
/// returning `false` visits all nodes (like C++ `forEach`).
///
/// Collapsed subtrees are skipped — only the nodes that are visually present are
/// visited. Use this when you need to count visible nodes, build a row list, or
/// search the visible tree.
///
/// # Turbo Vision heritage
///
/// Collapses the two C++ methods `TOutlineViewer::firstThat` (stop on `true`)
/// and `forEach` (visit all) into one generic function.
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

    // Root-level siblings.
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

/// Return the `(level, lines, flags)` of the node at DFS position `pos`,
/// or `None` if `pos` is out of range.
///
/// `level` is the nesting depth (0 = root); `lines` is the continuation-bar
/// bitmask used by `create_graph`; `flags` combines `OV_EXPANDED`,
/// `OV_CHILDREN`, and `OV_LAST`.
///
/// The draw and event code uses this to retrieve graph-draw parameters for the
/// focused node so it can determine whether a mouse click landed in the graph
/// column. You can call it directly when you need the structural metadata of a
/// specific DFS position.
///
/// # Turbo Vision heritage
///
/// Replaces `TOutlineViewer::getNode`, which returned a raw `TNode*`. Returning
/// `(level, lines, flags)` instead avoids exposing a mutable pointer into the
/// owned tree.
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
// create_graph / get_graph — the indent/box-drawing prefix
// ---------------------------------------------------------------------------

/// Build the box-drawing prefix string for one outline row.
///
/// Returns the indent and connector graphic that appears to the left of each
/// node's text label. `level` is the nesting depth (0 = root);
/// `lines` is the continuation-bar bitmask (bit `k` set means level `k` draws a
/// vertical bar through this row); `flags` combines `OV_EXPANDED`, `OV_CHILDREN`,
/// and `OV_LAST`; `lev_width` and `end_width` are the column widths of the
/// per-level indent and the connector end-graphic (both 3 in the default style).
///
/// `chars` maps symbol roles to characters:
/// `[0]` = level filler (space), `[1]` = level continuation bar (│),
/// `[2]` = end connector for a non-last child (├), `[3]` = end connector for
/// the last child (└), `[4]` = end filler / straight run (─),
/// `[5]` = end child-link indicator (─ or a custom char), `[6]` = retracted
/// (collapsed, `+`), `[7]` = expanded (─ or space).
///
/// Use [`ov_get_graph`] for the default style. Call `create_graph` directly only
/// when you need a custom character set or different column widths.
///
/// # Turbo Vision heritage
///
/// Ports `TOutlineViewer::createGraph` (`toutline.cpp`). The C++ `const char*`
/// byte string becomes a `&[char; 8]` typed array.
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

    // End graphic (the decrementing end-width cascade).
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

/// Return the default box-drawing prefix for one outline row.
///
/// Uses a level width and end width of 3, and the classic box-drawing character
/// set (space, │, ├, └, ─, ─, +, ─). Called from [`ov_draw`] for every visible
/// row. To use a different character set or column widths, build an outline
/// viewer that does not call `ov_draw` and instead calls [`create_graph`]
/// directly with the desired parameters.
///
/// # Turbo Vision heritage
///
/// Ports `TOutlineViewer::getGraph` (`toutline.cpp`). The C++ virtual method
/// becomes a free function; a concrete widget that wants a different style
/// replaces the call rather than overriding the method.
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
// ov_draw — render the visible tree
// ---------------------------------------------------------------------------

/// Render every visible node.
///
/// Per visible node: compute the color (focused / selected / normal), fill the
/// row, draw the graph then the text (the text uses the dim color when the node
/// is not expanded), shifted left by `delta.x`. After the traversal the remaining
/// rows are blank-filled.
///
/// NOTE: `this: &mut L` (not `&L`) — the `abs_origin` cache write requires
/// mutability. The drawing is logically read-only, but the origin is stored here
/// to feed [`Context::start_mouse_track`] (the `Button::abs_origin` pattern).
pub fn ov_draw<L: OutlineViewer + ?Sized>(this: &mut L, ctx: &mut DrawCtx) {
    // Cache the absolute origin for the mouse-tracking capture, which converts
    // absolute mouse coords to view-local via this value.
    this.ov_mut().abs_origin = ctx.origin();
    let size = this.ov().state.size;
    let delta = this.ov().delta;
    let foc = this.ov().foc;
    let focused_state = this.ov().state.state.focused;

    let nrm_color = ctx.style(Role::OutlineNormal);
    let focused_color = ctx.style(Role::OutlineFocused);
    let selected_color = ctx.style(Role::OutlineSelected);
    let not_expanded_color = ctx.style(Role::OutlineNotExpanded);

    // Last drawn position (-1 if nothing drawn).
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
        // Color selection: each row has a base color and a "dim" color used for
        // the text of a not-expanded node. For focused/selected rows the dim
        // color equals the base color; only a normal row dims to NotExpanded.
        let (color, dim_color) = if row.position == foc && focused_state {
            (focused_color, focused_color)
        } else if this.is_selected(row.position) {
            (selected_color, selected_color)
        } else {
            (nrm_color, not_expanded_color)
        };

        let y = row.position - delta.y;
        // Fill the whole row first.
        ctx.fill(Rect::new(0, y, size.x, y + 1), ' ', color);

        // Graph: drawn from column 0, shifted left by delta.x.
        let graph = ov_get_graph(this, row.level, row.lines, row.flags, ctx);
        let graph_w = graph.chars().count() as i32;
        // The text starts at max(0, graph_width - delta.x).
        let x = graph_w - delta.x;
        if x > 0 {
            // Skip delta.x leading columns of the graph.
            ctx.put_str_part(0, y, &graph, delta.x, color);
        }

        // Text: dim color when not expanded, else the row color.
        let text_color = if row.flags & OV_EXPANDED != 0 {
            color
        } else {
            dim_color
        };
        let text_x = x.max(0);
        let text_skip = (-x).max(0);
        ctx.put_str_part(text_x, y, &row.text, text_skip, text_color);
    }

    // Blank the remaining rows below the last drawn node. The last drawn DFS
    // position (`aux_pos`) is converted to a view-local row by subtracting
    // `delta.y`, so the fill never draws off the top when scrolled.
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

/// Clamp `new_focus`, focus it, and scroll it into view.
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

/// Recount the visible nodes, republish the scrollbar limits, and re-clamp the
/// focus.
///
/// Call this after every mutation to the tree (node inserted, removed, or
/// expanded/collapsed) **and** after inserting the widget into a group for the
/// first time. Without this call the scrollbar ranges and the focus clamp are
/// stale: the initial call after insertion is mandatory because a [`Context`]
/// is unavailable at construction time.
///
/// Internally this counts only the currently-visible nodes (collapsed subtrees
/// are excluded), computes the maximum row width, calls
/// [`OutlineViewerState::set_limit`] to publish those extents to the scrollbars,
/// and then calls [`adjust_focus`] to re-clamp `foc` within the new count.
pub fn ov_update<L: OutlineViewer + ?Sized>(this: &mut L, ctx: &mut Context) {
    // Count visible nodes and the max graph+text width. The default graph width
    // is deterministic — `level * 3 + 3` (level width = end width = 3) — so we
    // compute it analytically rather than building the graph string per node.
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

/// Expand the node at DFS position `pos` and all of its descendants.
///
/// Sibling nodes of the subtree at `pos` are left unchanged. This is the
/// operation bound to the `*` key: press `*` to recursively expand everything
/// under the focused node.
///
/// Because DFS positions shift each time a node expands, the algorithm restarts
/// the traversal each round: it finds the nesting depth of `pos` once, then
/// repeatedly locates the first unexpanded node-with-children inside that
/// subtree and calls [`OutlineViewer::adjust`] to expand it. The loop
/// terminates when no unexpanded node-with-children remains in the subtree.
///
/// After the loop, call [`ov_update`] to recount visible nodes and publish the
/// new scrollbar limits.
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

/// Apply a view-state flag change to an outline viewer.
///
/// Delegates to the concrete widget's [`View`] method. In addition to flipping
/// the flag, this function:
/// - On [`StateFlag::Focused`]: broadcasts [`Command::RECEIVED_FOCUS`] or
///   [`Command::RELEASED_FOCUS`] so the rest of the application can track which
///   outline has keyboard focus.
/// - On [`StateFlag::Active`] or [`StateFlag::Selected`]: shows or hides both
///   scrollbars to match the visibility convention (bars appear when the viewer
///   is active or selected, hide otherwise).
///
/// Concrete widgets that delegate `View::set_state` to this function
/// (as [`Outline`] does) get the scroller-compatible show/hide behavior for
/// free without duplicating it.
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

/// The scrollbar broadcast filter (inherited from the scroller), mouse
/// hold-tracking, and the keyboard nav switch.
///
/// The press-and-hold / edge auto-scroll loop runs through the
/// `MouseTrackCapture`: `MouseDown` arms the capture; tracked
/// `MouseMove`/`MouseAuto` route the loop body; `MouseUp` runs the post-loop
/// graph-toggle logic (`dragged < 2` distinguishes click from drag).
pub fn ov_handle_event<L: OutlineViewer + View + ?Sized>(
    this: &mut L,
    ev: &mut Event,
    ctx: &mut Context,
) {
    // The scroll-bar-changed read-sync filter (same as the scroller). Do NOT
    // clear the event — it stays live for the scrollbar's own handling.
    if let Event::Broadcast { command, source } = *ev
        && command == Command::SCROLL_BAR_CHANGED
        && source.is_some()
        && (source == this.ov().h_scroll_bar || source == this.ov().v_scroll_bar)
        && let Some(id) = this.ov().state.id()
    {
        ctx.request_scroll_sync(id, this.ov().h_scroll_bar, this.ov().v_scroll_bar);
    }

    match *ev {
        // -------------------------------------------------------------------
        // evMouseDown — first loop iteration: position, then arm the
        // mouse-track capture.
        //
        // The loop body runs once per DOWN, MOVE, or AUTO event; the post-loop
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
            // mouse is view-local already (the group delivers view-local coords).
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
                // Non-double-click: arm the mouse-track capture. The post-loop
                // graph-toggle logic runs in the MouseUp arm.
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
                // Uninserted (test/degenerate) widget: single-shot behavior, no
                // hold tracking.
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

/// Helper for the keyboard handler: clear the event, then adjust the focus.
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
// Outline — the concrete outline over an owned tree
// ---------------------------------------------------------------------------

/// A collapsible tree widget backed by an owned [`Node`] tree.
///
/// `Outline` is the ready-to-use concrete outline viewer. Build a [`Node`]
/// tree with [`Node::new`] and the builder methods, pass it to [`Outline::new`],
/// insert the widget into a group, and call [`ov_update`] once to publish the
/// scrollbar parameters. After that, the widget handles keyboard and mouse
/// navigation on its own.
///
/// To replace the displayed tree at runtime, assign a new value to
/// [`Outline::root`] and call [`ov_update`] again so the scrollbar limits
/// reflect the new content.
///
/// # Turbo Vision heritage
///
/// Ports `TOutline` (`toutline.cpp`). The C++ pointer-based tree becomes owned
/// `Box<Node>` links whose recursive drop replaces the explicit `disposeNode`
/// destructor.
pub struct Outline {
    ov: OutlineViewerState,
    /// The owned root node of the displayed tree, or `None` for an empty outline.
    ///
    /// Siblings chain through [`Node::next`] and children through
    /// [`Node::child_list`]; the recursive `Box<Node>` drop frees the entire
    /// subtree when this field is replaced or the `Outline` is dropped.
    ///
    /// To swap the tree at runtime, replace this field and call [`ov_update`]
    /// so the scrollbar limits stay consistent with the new content.
    pub root: Option<Box<Node>>,
}

impl Outline {
    /// Create an `Outline` over `bounds`, with optional horizontal (`h`) and
    /// vertical (`v`) scrollbar ids and an initial tree `root`.
    ///
    /// After calling `new`, insert the widget into a group and then call
    /// [`ov_update`] with the resulting [`Context`]. That second step is
    /// mandatory: publishing the scrollbar range and page parameters requires a
    /// `Context`, which is unavailable at construction time (the same constraint
    /// that [`crate::widgets::Scroller`] and [`crate::widgets::ListViewer`] face).
    /// Skipping `ov_update` leaves the scrollbars at their zero defaults and the
    /// focus unclamped.
    ///
    /// # Warning: call `ov_update` before first use
    ///
    /// Without [`ov_update`], `limit.y` stays 0 and `adjust_focus` clamps the
    /// focus index to -1. Even on a non-empty tree, pressing the down-arrow will
    /// appear to have no effect and the selection will be invisible. Call
    /// [`ov_update`] once after the widget is inserted into a group, the first
    /// time a [`Context`] is available — typically at the start of your first
    /// `handle_event` call with a `seeded` guard:
    ///
    /// ```rust,ignore
    /// if !self.seeded {
    ///     tv::ov_update(&mut self.outline, ctx);
    ///     self.seeded = true;
    /// }
    /// ```
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
/// (a node and — if expanded — its visible subtree). The same visible walk as
/// [`traverse`] without the flags/lines bookkeeping; the caller iterates root
/// siblings separately.
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

    /// Returns `self.root.as_deref()` — the root of the owned node tree.
    fn get_root(&self) -> Option<&Node> {
        self.root.as_deref()
    }

    /// Returns the next sibling of `node` by following [`Node::next`].
    fn get_next<'a>(&'a self, node: &'a Node) -> Option<&'a Node> {
        node.next.as_deref()
    }

    /// Returns the `i`-th child of `node` (0-based) by walking the
    /// [`Node::child_list`] → [`Node::next`] sibling chain `i` steps.
    /// Returns `None` if `i` is out of range.
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

    /// Counts `node`'s direct children by walking the sibling chain starting
    /// at [`Node::child_list`].
    fn get_num_children(&self, node: &Node) -> i32 {
        let mut i = 0;
        let mut p = node.child_list.as_deref();
        while let Some(n) = p {
            i += 1;
            p = n.next.as_deref();
        }
        i
    }

    /// Returns `node.text` as a `&str`.
    fn get_text<'a>(&'a self, node: &'a Node) -> &'a str {
        &node.text
    }

    /// Returns `node.expanded`.
    fn is_expanded(&self, node: &Node) -> bool {
        node.expanded
    }

    /// Returns `true` when [`Node::child_list`] is `Some`.
    fn has_children(&self, node: &Node) -> bool {
        node.child_list.is_some()
    }

    /// Set the expanded state of the node at DFS position `pos` in the
    /// currently visible tree.
    ///
    /// Unlike the C++ `TOutline::adjust`, which takes a raw node pointer,
    /// this implementation takes a DFS position index. It walks the owned tree
    /// with [`set_expanded_at_pos`] to find and mutate the node — making the
    /// shared traversal code position-keyed throughout rather than mixing
    /// positions and pointers.
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

    /// Return the index of the currently focused node as
    /// `Some(FieldValue::Int(foc))`, matching the [`crate::widgets::ListBox`]
    /// contract. Returns `Some(FieldValue::Int(-1))` when no node is focused
    /// (e.g. before [`ov_update`] is called on an empty tree).
    fn value(&self) -> Option<crate::data::FieldValue> {
        Some(crate::data::FieldValue::Int(self.ov().foc))
    }

    /// Re-publish scrollbar range/page params with the stored `limit` and the new
    /// `size` after the loop applies new bounds (identical to the scroller, which
    /// the outline viewer derives from).
    fn on_bounds_changed(&mut self, ctx: &mut Context) {
        let (x, y) = (self.ov().limit.x, self.ov().limit.y);
        self.ov_mut().set_limit(x, y, ctx);
    }

    fn apply_scroll_sync(&mut self, h: Option<i32>, v: Option<i32>, _ctx: &mut Context) {
        // Read-only, like the scroller: a missing bar is delta 0 (faithful to the
        // old dedicated pump arm's `.unwrap_or(0)` read).
        self.ov_mut()
            .apply_delta(Point::new(h.unwrap_or(0), v.unwrap_or(0)));
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

    // -- Node -----------------------------------------------------------------

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

        // CHANGED from own h-bar → ScrollSync queued.
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
            Deferred::ScrollSync { target: viewer, h: rh, v: rv }
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
    fn value_returns_focused_index() {
        // Before ov_update, foc = 0 and limit.y = 0. value() still returns Some.
        let mut o = Outline::new(Rect::new(0, 0, 20, 5), None, None, Some(animals_tree()));
        assert_eq!(
            o.value(),
            Some(crate::data::FieldValue::Int(0)),
            "value() returns Some(Int(foc)) before ov_update"
        );

        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            ov_update(&mut o, &mut ctx);
        }
        // foc is still 0 after ov_update (no navigation yet).
        assert_eq!(
            o.value(),
            Some(crate::data::FieldValue::Int(0)),
            "value() returns Some(Int(0)) at start"
        );

        // Navigate down once → foc = 1.
        let mut ev = Event::KeyDown(crate::event::KeyEvent::new(
            Key::Down,
            crate::event::KeyModifiers::default(),
        ));
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            o.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(
            o.value(),
            Some(crate::data::FieldValue::Int(1)),
            "value() tracks focused index after Down"
        );
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

    // -- mouse-track: Outline -------------------------------------------------
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
