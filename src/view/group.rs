//! [`Group`] — the view container and event router.
//!
//! A `Group` is the node type of the view tree: it owns a set of child views,
//! draws them, routes events to them, and tracks which one is current/focused.
//! The tree shape drives a few design choices:
//!
//! * **Ownership is a `Vec`, links are by id.** A [`Group`] owns
//!   `children: Vec<Child>`; children hold no up-pointer. Each child's [`ViewId`]
//!   is a process-global identity minted at [`Group::insert`] and stamped into the
//!   child's own `ViewState.id`. Cross-links (`current`) are a [`ViewId`],
//!   resolved by an internal index lookup ([`Group::index_of`]).
//!
//!   The `children` `Vec` is ordered **back-to-front for painting**:
//!   `children[0]` is the bottom (drawn first); `children.last()` is the
//!   top/frontmost (drawn last). [`Group::insert`] pushes, so a new child lands
//!   on top. Visits that go top-to-bottom (paint occlusion, hit-testing,
//!   broadcast delivery) iterate `children.iter().rev()`; tab order steps to the
//!   next-frontmost child by walking decreasing index with wrap.
//!
//! * **Mouse position is view-local at each level.** Because a child holds no
//!   pointer back to its owner, the group itself does the coordinate translation:
//!   on each positional delivery it subtracts the child's `origin`, handing the
//!   child a position already in the child's own coordinate frame.
//!
//! * **No event bubbling.** "Consumed" means the event was set to
//!   [`Event::Nothing`]; an unhandled event is simply left **not cleared**, and
//!   as the recursive [`handle_event`](View::handle_event) stack unwinds the
//!   parent/loop sees it still live. There is no owner pointer to bubble back to.
//!
//! * **Painter's-algorithm draw.** [`draw`](View::draw) paints back-to-front, so
//!   higher siblings overpaint lower ones. There is no per-cell occlusion
//!   bookkeeping: the whole tree is redrawn each frame and a diff sends only the
//!   changed cells to the terminal.
//!
//! * **Z-reorder lives in the owner.** A child cannot move itself in the
//!   Z-order; the group does it ([`Group::put_in_front_of`]/[`Group::make_first`]).
//!   Selecting a child that opts into raise-on-select
//!   ([`top_select`](crate::view::Options::top_select)) raises it to the top via
//!   [`Group::focus_child`] rather than merely making it current.
//!
//! Running a modal loop is not a group operation: the event loop owns the capture
//! stack, so modality runs through [`Program::exec_view`](crate::app::Program::exec_view)
//! and a [`ModalFrame`](crate::app::ModalFrame). Bulk data transfer uses the
//! typed [`View::value`]/[`View::set_value`] protocol, and the hardware cursor is
//! placed by the event loop.
//!
//! # Turbo Vision heritage
//! Ports `TGroup` (`tgroup.cpp`). The circular sibling ring plus owner
//! back-pointers become an owned `Vec` of children addressed by [`ViewId`]
//! (deviation D3); broadcasts carry a [`ViewId`] subject rather than a pointer
//! (deviation D4); the occlusion/buffering draw machinery is replaced by
//! whole-tree redraw + diff.

use crate::command::Command;
use crate::data::FieldValue;
use crate::event::{Event, Key};
use crate::view::context::{Context, DrawCtx};
use crate::view::geometry::{Point, Rect};
use crate::view::id::ViewId;
use crate::view::view::{Phase, StateFlag, View, ViewState};

/// Which select/deselect side effects [`Group::set_current`] applies when
/// changing the current view.
///
/// Pass this to [`Group::set_current`] to control focus transitions:
///
/// * [`Normal`](SelectMode::Normal) — the ordinary case: deselect the old current
///   view, then select the new one. Use this for all normal focus changes.
/// * [`Enter`](SelectMode::Enter) and [`Leave`](SelectMode::Leave) — used
///   internally by [`Program::exec_view`](crate::Program::exec_view) when starting
///   and ending a modal dialog. `Enter` avoids deselecting the view that was
///   selected before the modal opened; `Leave` avoids re-selecting it on close.
///   Together they ensure the view underneath a modal dialog keeps its
///   `selected` state visually intact across the modal session.
///
/// Widget implementors almost always pass `SelectMode::Normal`; the
/// `Enter`/`Leave` variants are the modal-loop plumbing in `Program`.
///
/// # Turbo Vision heritage
/// Ports the `selectMode` enum (`views.h`):
/// `normalSelect`/`enterSelect`/`leaveSelect`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectMode {
    /// Deselect the old current, select the new one — the ordinary focus change.
    Normal,
    /// Entering a modal: push focus onto the modal without deselecting the
    /// underlying current view.
    Enter,
    /// Leaving a modal: pop focus back without re-selecting the underlying view
    /// (its `selected` flag was preserved during the modal session).
    Leave,
}

/// One owned child view plus its global identity (the same value stored in the
/// child's own `ViewState.id`).
struct Child {
    id: ViewId,
    view: Box<dyn View>,
}

/// A view that owns and routes to a tree of child views.
///
/// See the [module docs](self) for the sibling-ring↔`Vec` Z-order mapping. Build
/// with [`Group::new`], add children with [`Group::insert`], and drive it as any
/// other [`View`] (`draw` / `handle_event` / …).
///
/// # Turbo Vision heritage
/// Ports `TGroup` (`tgroup.cpp`). Owns its children in a `Vec` addressed by
/// [`ViewId`] rather than a `next`/`prev`/`owner` pointer ring (deviation D3);
/// broadcasts carry a [`ViewId`] subject rather than a pointer (deviation D4).
pub struct Group {
    st: ViewState,
    /// Children in back-to-front paint order (`children[0]` is the bottom,
    /// `children.last()` is the top).
    children: Vec<Child>,
    /// The current (selected) child, as a [`ViewId`].
    current: Option<ViewId>,
    /// A visible+selectable child was inserted since the last
    /// [`set_current`](Group::set_current) — a pending request to re-establish
    /// currency. Settled by [`View::settle_currency`]; cleared by any explicit
    /// [`set_current`](Group::set_current).
    currency_dirty: bool,
}

impl Group {
    /// Create a [`Group`] covering `bounds`, ready to receive children via
    /// [`insert`](Self::insert).
    ///
    /// The constructed group is selectable (so it participates in focus routing)
    /// and opts into both mouse-tracking event classes (move and auto-repeat) so
    /// it receives them and can route them to its children. No children are
    /// added; add them with [`insert`](Self::insert) after construction.
    ///
    /// Most applications do not call `Group::new` directly — the standard
    /// container types ([`crate::desktop::Desktop`], [`crate::dialog::Dialog`])
    /// build on it internally. Call `Group::new` only when you need a bare,
    /// unstyled container that routes events and draws its children without any
    /// frame or background fill.
    ///
    /// # Turbo Vision heritage
    /// Ports `TGroup::TGroup` (`options |= ofSelectable`, `eventMask = 0xFFFF`).
    /// tvision-rs's event mask exposes only those two opt-in tracking classes; the
    /// dropped constructor bits were buffering/clipping flags that do not apply
    /// under whole-tree redraw + diff.
    pub fn new(bounds: Rect) -> Self {
        let mut st = ViewState::new(bounds);
        st.options.selectable = true;
        // Groups opt into all tracking classes so they can route them to children.
        st.event_mask.mouse_move = true;
        st.event_mask.mouse_auto = true;
        Group {
            st,
            children: Vec::new(),
            current: None,
            currency_dirty: false,
        }
    }

    /// The current (selected) child's id, if any.
    pub fn current(&self) -> Option<ViewId> {
        self.current
    }

    /// Number of children currently in the group.
    pub fn len(&self) -> usize {
        self.children.len()
    }

    /// Whether the group has no children.
    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    /// Resolve a [`ViewId`] to its index in `children` — the internal lookup
    /// that resolves an id handle to a child slot. `None` for a stale/foreign id.
    fn index_of(&self, id: ViewId) -> Option<usize> {
        self.children.iter().position(|c| c.id == id)
    }

    /// Test hook: resolve a [`ViewId`] to its child index (the private
    /// [`index_of`](Self::index_of) for other modules' tests).
    #[cfg(test)]
    pub fn index_of_pub(&self, id: ViewId) -> Option<usize> {
        self.index_of(id)
    }

    /// Test hook: mutably borrow the [`ViewState`] of child `idx`.
    #[cfg(test)]
    pub fn child_state_mut(&mut self, idx: usize) -> &mut ViewState {
        self.children[idx].view.state_mut()
    }

    /// The id of the topmost **visible** direct child whose bounds contain `pos`,
    /// or `None`. **Not recursive** — it scans only the direct children, mirroring
    /// the positional router in [`route_event`](Self::route_event) (same
    /// `visible && get_bounds().contains` predicate, same top-to-bottom scan).
    /// Used by the event loop to decide whether a click lands on the status line.
    pub(crate) fn topmost_child_at(&self, pos: Point) -> Option<ViewId> {
        // children are stored back-to-front; topmost is last → iterate rev.
        let n = self.children.len();
        (0..n).rev().find_map(|i| {
            let s = self.children[i].view.state();
            (s.state.visible && s.get_bounds().contains(pos)).then_some(self.children[i].id)
        })
    }

    /// Ids of tileable + visible direct children in **top-to-bottom** order
    /// (`children` reversed; see the module doc). Backs
    /// [`Desktop::tile`/`cascade`](crate::desktop::Desktop): the tile/cascade
    /// counter decrements across this visit, so the *first-visited* (topmost)
    /// child gets the highest position/offset.
    pub(crate) fn tileable_ids(&self) -> Vec<ViewId> {
        self.children
            .iter()
            .rev()
            .filter(|c| {
                let s = c.view.state();
                s.options.tileable && s.state.visible
            })
            .map(|c| c.id)
            .collect()
    }

    /// Child ids in **insertion order** (oldest first). `children` is stored
    /// back-to-front for painting and [`Group::insert`] *pushes*, so the storage
    /// order already equals the insertion order — a forward iteration suffices.
    /// Unlike [`tileable_ids`](Self::tileable_ids) this neither reverses nor
    /// filters: the splitter needs its panes parallel to its solver slots.
    pub(crate) fn child_ids_in_order(&self) -> Vec<ViewId> {
        self.children.iter().map(|c| c.id).collect()
    }

    /// Mutably borrow child `id`'s view — for an owner reaching into one of its
    /// own children by concrete type (downcast via [`View::as_any_mut`], e.g. a
    /// window reaching its frame). `None` for a stale/foreign id.
    pub fn child_mut(&mut self, id: ViewId) -> Option<&mut dyn View> {
        let i = self.index_of(id)?;
        Some(self.children[i].view.as_mut())
    }

    // -- gather / scatter ---------------------------------------------

    /// Collect the current typed value of every child into a snapshot vector.
    ///
    /// Returns one `Option<`[`FieldValue`]`>` per child in insertion
    /// (bottom-to-top) order — the same order [`scatter_data`](Self::scatter_data)
    /// expects. A `None` entry means that child carries no transferable value
    /// (e.g. a label or a decorative frame).
    ///
    /// Use this to read a dialog's current field values before closing it, or
    /// to checkpoint the dialog state for undo. Pair with
    /// [`scatter_data`](Self::scatter_data) to restore:
    ///
    /// ```rust,ignore
    /// let snapshot = dialog_group.gather_data();
    /// // … later …
    /// dialog_group.scatter_data(&snapshot, ctx);
    /// ```
    ///
    /// # Turbo Vision heritage
    /// Ports `TGroup::getData` (the data-walk that collects each control's
    /// value into a flat byte record). The typed [`FieldValue`] vector replaces
    /// the opaque byte record of the original `getData`/`setData` value protocol.
    pub fn gather_data(&self) -> Vec<Option<FieldValue>> {
        self.children.iter().map(|c| c.view.value()).collect()
    }

    /// Distribute typed values back to children in the same bottom-to-top
    /// order as [`gather_data`](Self::gather_data).
    ///
    /// Use this to initialize or reset a dialog's fields from application data
    /// before showing it, or to restore a snapshot previously taken with
    /// [`gather_data`](Self::gather_data). A typical dialog open sequence:
    ///
    /// ```rust,ignore
    /// let initial = vec![
    ///     Some(FieldValue::String("default".into())),
    ///     None, // label — no value
    ///     Some(FieldValue::Bool(true)),
    /// ];
    /// dialog_group.scatter_data(&initial, ctx);
    /// ```
    ///
    /// `values` shorter than `children`: remaining children are left untouched.
    /// Extra values beyond the children count are silently ignored. `None`
    /// entries skip the corresponding child without writing it.
    ///
    /// Uses [`set_value_ctx`](View::set_value_ctx) rather than
    /// [`set_value`](View::set_value) so widgets that need a `Context` for
    /// side effects (e.g. [`ListBox`](crate::widgets::ListBox) updating its
    /// scroll bar) receive it correctly.
    ///
    /// # Turbo Vision heritage
    /// Ports `TGroup::setData` (the data-walk that writes each control's value
    /// back from a flat byte record). The typed [`FieldValue`] slice replaces
    /// the opaque byte record of the original `getData`/`setData` value protocol.
    pub fn scatter_data(&mut self, values: &[Option<FieldValue>], ctx: &mut Context) {
        for (child, val) in self.children.iter_mut().zip(values.iter()) {
            if let Some(v) = val.clone() {
                child.view.set_value_ctx(v, ctx);
            }
        }
    }

    /// Gather the whole record as a single ordered [`FieldValue::List`] — the
    /// typed image of C++ `getData(void *rec)`'s offset-addressed walk. Only
    /// **data-bearing** children (those whose [`value`](View::value) is `Some`)
    /// contribute, in child order; a child with no value is the `dataSize == 0`
    /// case and is absent. Built on [`gather_data`](Self::gather_data).
    ///
    /// # Turbo Vision heritage
    /// `TGroup::getData` viewed as producing one record value.
    pub fn gather_list(&self) -> FieldValue {
        FieldValue::List(self.gather_data().into_iter().flatten().collect())
    }

    /// Scatter an ordered [`FieldValue::List`] record back to the data-bearing
    /// children, in child order (the inverse of [`gather_list`](Self::gather_list)).
    /// Children with no value are skipped (they consume no record slot — the
    /// `dataSize == 0` walk). A non-`List` argument is ignored.
    ///
    /// # Turbo Vision heritage
    /// `TGroup::setData` viewed as consuming one record value.
    pub fn scatter_list(&mut self, record: &FieldValue, ctx: &mut Context) {
        let FieldValue::List(items) = record else {
            return;
        };
        let mut next = items.iter();
        for child in self.children.iter_mut() {
            // Only children that carry a value take a slot (faithful to the
            // offset walk: a dataSize==0 control is skipped).
            if child.view.value().is_some()
                && let Some(v) = next.next()
            {
                child.view.set_value_ctx(v.clone(), ctx);
            }
        }
    }

    // -- insert / remove ----------------------------------------------------

    /// Insert `view` on **top** of the group (it becomes the frontmost child),
    /// mint a process-global [`ViewId`], stamp it into the view's own
    /// `ViewState.id`, and return it.
    ///
    /// Applies `center_x`/`center_y` centering (the only observable effect at
    /// insert time). `insert` alone never focuses — to make the new child current,
    /// callers use [`set_current`](Self::set_current)/[`reset_current`](Self::reset_current).
    ///
    /// **Insert-time currency:** inserting a visible+selectable child sets
    /// `currency_dirty`, a deferred request to re-establish the group's current
    /// view. It is settled by [`View::settle_currency`] (run by the event loop /
    /// at program start), which runs [`reset_current`](Self::reset_current); any
    /// explicit [`set_current`](Self::set_current) in between supersedes it and
    /// clears the flag.
    ///
    /// # Turbo Vision heritage
    /// Ports `TGroup::insert` (insert-on-top). The hide/show/active-restore dance
    /// around the original insert is a no-op here (no occlusion tracking); only
    /// the centering and the deferred currency cascade survive.
    pub fn insert(&mut self, mut view: Box<dyn View>) -> ViewId {
        // ofCenterX/ofCenterY centering (insertBefore).
        let opts = view.state().options;
        if opts.center_x || opts.center_y {
            let mut bounds = view.state().get_bounds();
            let size = view.state().size;
            if opts.center_x {
                let ox = (self.st.size.x - size.x) / 2;
                bounds.r#move(ox - bounds.a.x, 0);
            }
            if opts.center_y {
                let oy = (self.st.size.y - size.y) / 2;
                bounds.r#move(0, oy - bounds.a.y);
            }
            view.state_mut().set_bounds(bounds);
        }

        let id = ViewId::next();
        view.state_mut().id = Some(id); // stamp the view's own handle (self-id)
        // Pending insert-time show()->resetCurrent, gated exactly like the
        // C++ (sfVisible + ofSelectable); settled by settle_currency.
        let s = view.state();
        if s.options.selectable && s.state.visible {
            self.currency_dirty = true;
        }
        self.children.push(Child { id, view });
        id
    }

    /// Insert `view` on top of the group stamping a **caller-supplied** [`ViewId`]
    /// instead of minting a fresh one — the pre-minted-id sibling of
    /// [`insert`](Self::insert).
    ///
    /// The menu modal layer pre-mints each open box's [`ViewId`] from
    /// the global counter ([`ViewId::next`]) *before* requesting the box be opened,
    /// so the [`MenuSession`](crate::menu::MenuSession) capture handler already
    /// knows every box id with no insert-time callback or downcast. The event loop
    /// then builds the [`MenuBox`](crate::menu::MenuBox) and inserts it here,
    /// stamping the id the session already holds. Otherwise identical to
    /// [`insert`](Self::insert): centering applies; `current` is **not** touched —
    /// a menu box is never focused, the session owns every event.
    ///
    /// The caller is responsible for the id being globally unique (a `ViewId::next`
    /// value is, by construction); reusing a live id would make `find_mut` resolve
    /// the wrong child.
    pub fn insert_with_id(&mut self, mut view: Box<dyn View>, id: ViewId) {
        // ofCenterX/ofCenterY centering (insertBefore) — same as `insert`.
        let opts = view.state().options;
        if opts.center_x || opts.center_y {
            let mut bounds = view.state().get_bounds();
            let size = view.state().size;
            if opts.center_x {
                let ox = (self.st.size.x - size.x) / 2;
                bounds.r#move(ox - bounds.a.x, 0);
            }
            if opts.center_y {
                let oy = (self.st.size.y - size.y) / 2;
                bounds.r#move(0, oy - bounds.a.y);
            }
            view.state_mut().set_bounds(bounds);
        }

        view.state_mut().id = Some(id); // stamp the caller-supplied handle
        // Same pending insert-time show()->resetCurrent marker as `insert`
        // (a menu box is non-selectable, so this is a no-op on the menu path).
        let s = view.state();
        if s.options.selectable && s.state.visible {
            self.currency_dirty = true;
        }
        self.children.push(Child { id, view });
    }

    /// Build/runtime removal of a direct child by id (no `Context`). Drops the child
    /// from `children`; clears `current` if it pointed at the removed child. The
    /// in-event-loop close path uses `Deferred::Close`/[`remove`](Self::remove)
    /// (with `Context`) instead.
    ///
    /// Returns `true` if the child was found and removed, `false` if the id was
    /// not a direct child.
    pub(crate) fn remove_child_by_id(&mut self, id: ViewId) -> bool {
        if let Some(pos) = self.children.iter().position(|c| c.id == id) {
            self.children.remove(pos);
            if self.current == Some(id) {
                self.current = None;
            }
            true
        } else {
            false
        }
    }

    /// Remove the child named by `id` from this group, dropping it, and
    /// re-establish focus if needed (no-op if `id` is not a direct child).
    ///
    /// Use `remove` when you hold a `&mut Context` — typically inside a
    /// `handle_event` override or a deferred-effect callback. For closes
    /// triggered inside event dispatch (the common "close window" path) prefer
    /// [`Context::request_close`](crate::view::Context::request_close), which
    /// defers the removal to after the dispatch unwinds; `Group::remove` is
    /// available for the rare case where you already hold `&mut Group` directly
    /// outside the dispatch stack.
    ///
    /// Removing a visible+selectable child re-establishes currency
    /// ([`reset_current`](Self::reset_current)) whether or not that child was
    /// the current one. A removed current additionally vacates the `current`
    /// slot first. The removed child is **dropped** (Rust ownership): unlike
    /// the C++ `Delete` there is no returned detached pointer.
    ///
    /// # Turbo Vision heritage
    /// Ports `TGroup::Delete` and `TGroup::remove` (`tgroup.cpp`): hiding the
    /// view before unlinking it runs the visibility-change currency tail, which
    /// is why any visible+selectable removal resets currency, not just removal
    /// of the current view. The Rust port drops the child (owned `Box`) rather
    /// than handing it back as a raw pointer.
    pub fn remove(&mut self, id: ViewId, ctx: &mut Context) {
        let Some(i) = self.index_of(id) else {
            return;
        };
        let was_current = self.current == Some(id);
        // The remove→hide tail's gate, read BEFORE the child is dropped.
        let s = self.children[i].view.state();
        let needs_reset = was_current || (s.state.visible && s.options.selectable);
        self.children.remove(i);
        if was_current {
            self.current = None;
        }
        if needs_reset {
            self.reset_current(ctx);
        }
    }

    // -- focus machinery (faithful ports of setCurrent / focus / findNext) ---

    /// Select child `p` as the current view (or clear the current to `None`),
    /// applying focus/select side effects without validating the outgoing view.
    ///
    /// This is the **unvalidated** current-change path — it does not call
    /// `valid(RELEASED_FOCUS)` on the outgoing child. For the validated
    /// Tab/Shift-Tab navigation path use [`focus_next`](Self::focus_next) or
    /// [`focus_child`](Self::focus_child), both of which gate on the outgoing
    /// view's consent before switching.
    ///
    /// `mode` controls which selection side effects fire:
    /// - [`SelectMode::Normal`]: deselect the old current, select the new one.
    /// - [`SelectMode::Enter`]: entering a modal — do **not** deselect the old
    ///   current (it keeps its selected state underneath the modal).
    /// - [`SelectMode::Leave`]: leaving a modal — do **not** select the new
    ///   current (restores the pre-modal current without re-selecting it).
    ///
    /// When the group itself is focused the focused flag is moved between the
    /// outgoing and incoming children as well.
    ///
    /// **Unvalidated `SelectNext` equivalent:** to advance focus without
    /// validation (the C++ `SelectNext` behavior), compose
    /// [`find_next`](Self::find_next) + `set_current`:
    ///
    /// ```rust,ignore
    /// if let Some(next) = group.find_next(forwards) {
    ///     group.set_current(Some(next), SelectMode::Normal, ctx);
    /// }
    /// ```
    ///
    /// # Turbo Vision heritage
    /// Ports `TGroup::setCurrent` plus its focus/select helpers (`tgroup.cpp`),
    /// with the redraw-locking bracket dropped (whole-tree redraw + diff).
    /// `SelectNext` (guide p. 453, unvalidated sibling ring walk) is realized
    /// by composing `find_next` + `set_current` rather than a dedicated method.
    pub fn set_current(&mut self, p: Option<ViewId>, mode: SelectMode, ctx: &mut Context) {
        // CURRENCY KEYSTONE — DO NOT MOVE BELOW THE EARLY RETURN. Any explicit currency
        // op supersedes the pending insert-time reset: clearing `currency_dirty`
        // here (including on the `current == p` early-return leg) is the ENTIRE
        // defense against the settle pass clobbering explicit focus afterwards
        // (exec_view's initial_focus / set_current(Enter), any focus_child).
        // Without it, settle_currency would re-run reset_current after the caller
        // deliberately chose a different child, snapping focus back to firstMatch.
        self.currency_dirty = false;
        if self.current == p {
            return;
        }
        // Copy indices into locals before the &mut child calls (State is Copy).
        let cur_idx = self.current.and_then(|id| self.index_of(id));
        let p_idx = p.and_then(|id| self.index_of(id));
        let group_focused = self.st.state.focused;

        // focusView(current, false): only if the group itself is focused.
        if group_focused && let Some(i) = cur_idx {
            self.children[i]
                .view
                .set_state(StateFlag::Focused, false, ctx);
        }
        // Deselect the old current unless entering a modal.
        if mode != SelectMode::Enter
            && let Some(i) = cur_idx
        {
            self.children[i]
                .view
                .set_state(StateFlag::Selected, false, ctx);
        }
        // Select the new current unless leaving a modal.
        if mode != SelectMode::Leave
            && let Some(i) = p_idx
        {
            self.children[i]
                .view
                .set_state(StateFlag::Selected, true, ctx);
        }
        // focusView(p, true): only if the group itself is focused.
        if group_focused && let Some(i) = p_idx {
            self.children[i]
                .view
                .set_state(StateFlag::Focused, true, ctx);
        }
        self.current = p;
    }

    /// Make the child `id` the current/focused one (the group is assumed focused).
    ///
    /// First the **outgoing** current is validated: if it opts into validation
    /// ([`validate`](crate::view::Options::validate)) and reports it is not yet
    /// ready to release focus, the switch is refused and this returns `false`.
    /// Otherwise the child is selected: a child that opts into raise-on-select
    /// ([`top_select`](crate::view::Options::top_select)) is **raised to the top**
    /// ([`make_first`](Self::make_first)); any other selectable child is just made
    /// current via [`set_current`](Self::set_current). Returns `true` on success.
    ///
    /// **Self-heal:** after raising a `top_select` child, if it is somehow still
    /// not current we re-assert currency with an explicit
    /// [`set_current`](Self::set_current). Normally raising a child also
    /// re-establishes currency, but that step no-ops when the child is already on
    /// top — and a *just-inserted* topmost child has not yet had its deferred
    /// insert-time currency settled, so without this its `current` slot could
    /// still be empty. When currency is already correct this re-assert is itself a
    /// no-op ([`set_current`](Self::set_current) early-returns when nothing
    /// changes).
    ///
    /// **Selectability** is checked by the callers, not here: the mouse-down
    /// auto-select path checks `selectable && !selected && !disabled` first, and
    /// [`focus_next`](Self::focus_next) only iterates visible, enabled, selectable
    /// children — so `focus_child` itself does not re-check it.
    ///
    /// # Turbo Vision heritage
    /// Realizes `TView::focus()` → `TView::select()` (`tview.cpp`) in the owner:
    /// the validate-then-select sequence, where `ofTopSelect` views call
    /// `makeFirst` and others call `setCurrent`. The self-heal covers a case the
    /// C++ leaves to its synchronous insert-time `resetCurrent`, which tvision-rs defers
    /// (deviation D3).
    pub fn focus_child(&mut self, id: ViewId, ctx: &mut Context) -> bool {
        // focus(): validate the outgoing current before letting it lose focus.
        if let Some(ci) = self.current.and_then(|c| self.index_of(c)) {
            let validate = self.children[ci].view.state().options.validate;
            if validate && !self.children[ci].view.valid(Command::RELEASED_FOCUS, ctx) {
                return false; // focus refused
            }
        }
        // select(): ofTopSelect -> makeFirst (raise + resetCurrent); else
        // setCurrent(normalSelect).
        let top_select = match self.index_of(id) {
            Some(i) => self.children[i].view.state().options.top_select,
            None => return true, // unknown id: nothing to select (C++ owner!=0 guard)
        };
        if top_select {
            self.make_first(id, ctx);
            // Self-heal: C++ select() relies on putInFrontOf's resetCurrent tail,
            // which the already_in_place no-op skips. That is safe in C++ only
            // because insert-time show()->resetCurrent() keeps the topmost
            // ofTopSelect window current (so an already-top window is already
            // current). tvision-rs defers that cascade to the pump's settle_currency
            // pass, so between events the invariant holds — but it REMAINS
            // LOAD-BEARING for same-instant focus within one call sequence (the
            // e8d82f2 bite): insert_and_focus calls focus_child on a just-
            // inserted, not-yet-settled topmost window (current still None).
            // Re-assert currency explicitly; when the invariant holds this is a
            // no-op (set_current early-returns on current == p).
            if self.current != Some(id) {
                self.set_current(Some(id), SelectMode::Normal, ctx);
            }
        } else {
            self.set_current(Some(id), SelectMode::Normal, ctx);
        }
        true
    }

    /// Re-establish `current` as the first visible+selectable child.
    ///
    /// # Turbo Vision heritage
    /// Ports `TGroup::resetCurrent` (re-select the first matching child).
    pub fn reset_current(&mut self, ctx: &mut Context) {
        let p = self.first_match_visible_selectable();
        self.set_current(p, SelectMode::Normal, ctx);
    }

    /// The first visible+selectable child, in the search order used to establish
    /// currency: the **bottom** child (`children[0]`) is checked first, then the
    /// rest top-to-bottom (`children[len-1], len-2, …, 1`).
    fn first_match_visible_selectable(&self) -> Option<ViewId> {
        let n = self.children.len();
        if n == 0 {
            return None;
        }
        let matches = |c: &Child| {
            let s = c.view.state();
            s.state.visible && s.options.selectable
        };
        // last == children[0] first.
        if matches(&self.children[0]) {
            return Some(self.children[0].id);
        }
        // then first()→down: children[len-1], len-2, …, 1.
        for i in (1..n).rev() {
            if matches(&self.children[i]) {
                return Some(self.children[i].id);
            }
        }
        None
    }

    /// The next selectable child in tab order from `current`, or `None` if there
    /// is no other eligible child (the search wraps back to `current`).
    ///
    /// `forwards` walks toward the next-frontmost child (decreasing index, with
    /// wrap); the reverse direction walks increasing index. Eligible means
    /// visible, enabled, and selectable.
    ///
    /// # Turbo Vision heritage
    /// Ports `TGroup::findNext` (tab-order search over the sibling ring).
    pub fn find_next(&self, forwards: bool) -> Option<ViewId> {
        let cur = self.current?;
        let n = self.children.len();
        let start = self.index_of(cur)?;
        let mut i = start;
        loop {
            i = if forwards {
                // p->next == decreasing Vec index with wrap.
                if i == 0 { n - 1 } else { i - 1 }
            } else {
                // p->prev == increasing Vec index with wrap.
                if i == n - 1 { 0 } else { i + 1 }
            };
            let s = self.children[i].view.state();
            let eligible = s.state.visible && !s.state.disabled && s.options.selectable;
            if eligible || i == start {
                break;
            }
        }
        if i != start {
            Some(self.children[i].id)
        } else {
            None
        }
    }

    /// Move focus to the next selectable child in tab order: focuses the
    /// [`find_next`](Self::find_next) result, or returns `true` when there is no
    /// other eligible child (so a no-other-child group reports success rather than
    /// blocking the keystroke).
    ///
    /// # Turbo Vision heritage
    /// Ports `TGroup::focusNext`.
    pub fn focus_next(&mut self, forwards: bool, ctx: &mut Context) -> bool {
        match self.find_next(forwards) {
            Some(id) => self.focus_child(id, ctx),
            None => true,
        }
    }

    /// Advance focus to the next eligible sibling in tab order **without wrapping**,
    /// entering a child group at its edge (via [`View::focus_to_edge`]). `backward`
    /// is Shift-Tab; tab order is insertion index (forward = increasing index,
    /// matching [`find_next`](Self::find_next)'s plain-Tab direction). Returns
    /// `false` when there is no further sibling — the hierarchical Tab pass in
    /// [`handle_event`](View::handle_event) then leaves the key unconsumed so it
    /// bubbles to the parent group; the window wraps at the top. Skips
    /// selectable-but-empty subtrees (no focusable leaf).
    fn advance_no_wrap(&mut self, backward: bool, ctx: &mut Context) -> bool {
        let n = self.children.len() as i32;
        if n == 0 {
            return false;
        }
        // Start at the current child; with none, start just outside so the first
        // step lands on the leading edge.
        let start = self.current.and_then(|id| self.index_of(id));
        let mut i = match start {
            Some(s) => s as i32,
            None if backward => n,
            None => -1,
        };
        loop {
            i += if backward { -1 } else { 1 };
            if i < 0 || i >= n {
                return false;
            }
            let idx = i as usize;
            let s = self.children[idx].view.state();
            let eligible = s.state.visible && !s.state.disabled && s.options.selectable;
            if eligible && self.children[idx].view.has_focusable_leaf() {
                let id = self.children[idx].id;
                self.focus_child(id, ctx);
                self.children[idx].view.focus_to_edge(backward, ctx);
                return true;
            }
        }
    }

    /// Select (raise + focus) the selectable child whose [`number`](View::number)
    /// matches `num`. Returns whether a match was found. This backs the
    /// "select window N" shortcut (Alt-1 … Alt-9): it walks the children directly
    /// and uses [`focus_child`](Self::focus_child), so a matching window that opts
    /// into raise-on-select is raised to the top as well as focused.
    ///
    /// Unlike the tab-order path, selectability is filtered **here** (the match
    /// requires a selectable child); the validation re-check inside
    /// [`focus_child`](Self::focus_child) is harmless because the caller has
    /// already confirmed focus may move.
    ///
    /// # Turbo Vision heritage
    /// Realizes the "select window by number" broadcast arm of
    /// `TWindow::handleEvent` as a direct child walk (deviations D3/D4).
    pub fn focus_by_number(&mut self, num: i16, ctx: &mut Context) -> bool {
        let target = self.children.iter().find_map(|c| {
            let s = c.view.state();
            if s.options.selectable && c.view.number() == Some(num) {
                Some(c.id)
            } else {
                None
            }
        });
        match target {
            Some(id) => {
                self.focus_child(id, ctx);
                true
            }
            None => false,
        }
    }

    // -- Z-reorder (putInFrontOf / makeFirst, realized in the owner) ------

    /// Reposition child `id` to sit immediately **in front of** (above in
    /// Z-order) `target`, or at the very top if `target` is `None`.
    ///
    /// Use this to control the stacking order of child views — for example,
    /// to ensure a newly opened panel appears behind an existing top-level
    /// dialog, or to implement a "send to back of peers" layout. A child
    /// cannot reorder itself (it holds no owner pointer); the owning group is
    /// always the actor.
    ///
    /// Concretely, "in front of `target`" means `id` lands one slot above
    /// `target` in the back-to-front `children` Vec (painted later, therefore
    /// on top). `target == None` is a sentinel meaning "the top" (used
    /// internally by [`make_first`](Self::make_first)).
    ///
    /// **Guards:** this is a no-op if `id == target`, if `id` is already
    /// immediately in front of `target`, or (for `target == None`) if `id`
    /// is already the topmost child. Unknown or foreign ids are silently
    /// ignored. If the moved view is selectable, currency is re-established
    /// ([`reset_current`](Self::reset_current)) afterwards.
    ///
    /// **No send-to-bottom:** there is intentionally no send-to-bottom path —
    /// no current consumer needs it.
    ///
    /// # Turbo Vision heritage
    /// Realizes `TView::putInFrontOf` in the owner. The original send-to-bottom
    /// overload (`putInFrontOf(0)`) is unimplemented; the visibility hide/show
    /// redraw dance is dropped (whole-tree redraw + diff).
    pub fn put_in_front_of(&mut self, id: ViewId, target: Option<ViewId>, ctx: &mut Context) {
        let Some(from) = self.index_of(id) else {
            return;
        };
        // Resolve the target's index (None = to-top sentinel for make_first).
        // An unknown target id is ignored (C++ requires Target->owner == owner;
        // the Target==0 / send-to-bottom path is unimplemented — no consumer).
        let target_idx = match target {
            None => None,
            Some(t) => {
                if t == id {
                    return; // Target == this -> no-op.
                }
                match self.index_of(t) {
                    Some(ti) => Some(ti),
                    None => return, // foreign/stale target -> ignore.
                }
            }
        };

        // No-op guard (C++ `Target == nextView()` / already-top): `id` is already
        // immediately in front of `target`. With `target == None` that means `id`
        // is already the top (`from == len - 1`); with a target it means `id` sits
        // one slot above `target` (`from == target_idx + 1`).
        let n = self.children.len();
        let already_in_place = match target_idx {
            None => from == n - 1,
            Some(ti) => from == ti + 1,
        };
        if already_in_place {
            return;
        }

        // Reorder: pull `id` out, then re-insert. The insertion index is computed
        // against the POST-removal Vec.
        //  - target == None  -> push to the very end (top): index == new len.
        //  - target == Some  -> immediately above `target`: `target`'s post-removal
        //    index + 1. Removing `from` shifts indices above `from` down by one.
        let child = self.children.remove(from);
        let insert_at = match target_idx {
            None => self.children.len(),
            Some(ti) => {
                let ti = if ti > from { ti - 1 } else { ti };
                ti + 1
            }
        };
        self.children.insert(insert_at, child);

        // Faithful tail: if the moved view is selectable, resetCurrent().
        if self.children[insert_at].view.state().options.selectable {
            self.reset_current(ctx);
        }
    }

    /// Move child `id` to the top (frontmost) of the Z-order. Equivalent to
    /// [`put_in_front_of`](Self::put_in_front_of) with `target == None`. Realized
    /// in the owner.
    ///
    /// # Turbo Vision heritage
    /// Ports `TView::makeFirst` (`putInFrontOf(first())`).
    pub fn make_first(&mut self, id: ViewId, ctx: &mut Context) {
        self.put_in_front_of(id, None, ctx);
    }

    // -- event routing helpers ----------------------------------------------

    /// Per-child event-mask gate: does this child accept events of `ev`'s class?
    /// Only the two opt-in tracking classes (mouse-move / mouse-auto) are gated;
    /// every other class falls through to `true`.
    fn wants(s: &ViewState, ev: &Event) -> bool {
        match ev {
            Event::MouseMove(_) => s.event_mask.mouse_move,
            Event::MouseAuto(_) => s.event_mask.mouse_auto,
            // `MouseWheel` falls through to `true`: delivered unconditionally,
            // with no eventMask gate (like `MouseDown`). Behavior-neutral vs the
            // C++ eventMask gate — in tvision-rs only the two opt-in classes are gated.
            _ => true,
        }
    }

    /// Disabled gate — a disabled view ignores positional (mouse) and focused
    /// (key/command) events but still receives broadcasts.
    fn blocked(s: &ViewState, ev: &Event) -> bool {
        s.state.disabled
            && matches!(
                ev,
                Event::MouseDown(_)
                    | Event::MouseUp(_)
                    | Event::MouseMove(_)
                    | Event::MouseAuto(_)
                    | Event::KeyDown(_)
                    | Event::Command(_)
            )
    }

    /// Deliver `ev` to child `idx` (the phase gating is applied by the caller).
    /// No-op if the event is already consumed, the child is disabled for this
    /// class, or the child has not opted into it.
    ///
    /// For positional (mouse) events the position is translated into the child's
    /// local coordinate frame first (subtract the child's `origin`); if the child
    /// consumes the event we propagate the *consumed* state (`Nothing`) back up —
    /// never the translated position.
    fn deliver(&mut self, idx: usize, ev: &mut Event, ctx: &mut Context) {
        if ev.is_nothing() {
            return;
        }
        let s = self.children[idx].view.state();
        if Self::blocked(s, ev) || !Self::wants(s, ev) {
            return;
        }
        let origin = s.origin;
        let mut local = ev.clone();
        if let Some(p) = mouse_pos_mut(&mut local) {
            *p -= origin;
        }
        self.children[idx].view.handle_event(&mut local, ctx);
        // Propagate "consumed" back up, but not the translated position.
        if local.is_nothing() {
            ev.clear();
        }
    }
}

/// A mutable handle on the position of a mouse event (`None` for non-mouse
/// events), so the group can rewrite the position into the child's coordinate
/// frame before delivery.
fn mouse_pos_mut(ev: &mut Event) -> Option<&mut Point> {
    match ev {
        Event::MouseDown(m)
        | Event::MouseUp(m)
        | Event::MouseMove(m)
        | Event::MouseAuto(m)
        | Event::MouseWheel(m) => Some(&mut m.position),
        _ => None,
    }
}

/// `Some(Point)` for the four mouse variants — the (group-local) hit-test
/// position.
fn mouse_pos(ev: &Event) -> Option<Point> {
    match ev {
        Event::MouseDown(m) | Event::MouseUp(m) | Event::MouseMove(m) | Event::MouseAuto(m) => {
            Some(m.position)
        }
        _ => None,
    }
}

impl View for Group {
    fn state(&self) -> &ViewState {
        &self.st
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.st
    }

    /// Paint visible children **back-to-front** (`children[0]` →
    /// `children.last()`), each through a sub-context clipped to its bounds —
    /// painter's algorithm, so higher siblings overpaint lower ones. The group
    /// does not fill its own area; the children cover it. After each child that
    /// casts a drop shadow the group draws the shadow ([`DrawCtx::cast_shadow`]):
    /// back-to-front order means later (higher) siblings overwrite the shadow
    /// cells they occlude.
    ///
    /// # Turbo Vision heritage
    /// Ports `TGroup::draw`/`drawSubViews`, with the paint order reversed: the
    /// original paints top-first and tracks occlusion, which tvision-rs drops in favor
    /// of whole-tree redraw + diff.
    fn draw(&mut self, ctx: &mut DrawCtx) {
        for child in self.children.iter_mut() {
            if child.view.state().state.visible {
                let bounds = child.view.state().get_bounds();
                let mut sub = ctx.sub(bounds);
                child.view.draw(&mut sub);
                if child.view.state().state.shadow {
                    ctx.cast_shadow(bounds);
                }
            }
        }
    }

    /// Flip the group's own state flag (emitting the focus-gained/lost broadcast
    /// for the focused flag) then propagate: `Active`/`Dragging` to **all**
    /// children; `Focused` to the **current** child only.
    ///
    /// # Turbo Vision heritage
    /// Ports `TGroup::setState`. The dropped flag cases (visible/exposed) were
    /// occlusion side effects, unneeded under whole-tree redraw + diff.
    fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
        // Base behaviour: flip the group's own flag + (for Focused) broadcast.
        self.st.set_flag(flag, enable);
        if flag == StateFlag::Focused {
            let source = self.st.id(); // the group itself == C++ `this`
            ctx.broadcast(
                if enable {
                    Command::RECEIVED_FOCUS
                } else {
                    Command::RELEASED_FOCUS
                },
                source,
            );
        }
        match flag {
            StateFlag::Active | StateFlag::Dragging => {
                for i in 0..self.children.len() {
                    self.children[i].view.set_state(flag, enable, ctx);
                }
            }
            StateFlag::Focused => {
                if let Some(i) = self.current.and_then(|id| self.index_of(id)) {
                    self.children[i]
                        .view
                        .set_state(StateFlag::Focused, enable, ctx);
                }
            }
            // Selected and Visible do not fan out to children; Visible is targeted per-child via set_visible_descendant.
            StateFlag::Selected | StateFlag::Visible => {}
        }
    }

    /// Apply `bounds`; if the size changed, propagate the size delta to every
    /// child via [`calc_bounds`](View::calc_bounds)/[`change_bounds`](View::change_bounds)
    /// (the grow-mode resize math), so children grow/move with their owner.
    ///
    /// # Turbo Vision heritage
    /// Ports `TGroup::changeBounds`, with the buffer-reallocation/redraw-locking
    /// bracket dropped (whole-tree redraw + diff).
    fn change_bounds(&mut self, bounds: Rect) {
        let delta = (bounds.b - bounds.a) - self.st.size;
        self.st.set_bounds(bounds);
        if delta.x != 0 || delta.y != 0 {
            let owner_size = self.st.size;
            for i in 0..self.children.len() {
                let r = self.children[i].view.calc_bounds(owner_size, delta);
                self.children[i].view.change_bounds(r);
            }
        }
    }

    /// The focused child's help context (recursively), falling back to the group's
    /// own when there is no current child or the chain yields no context.
    ///
    /// # Turbo Vision heritage
    /// Ports `TGroup::getHelpCtx`: returns `current->getHelpCtx()`, and only when
    /// that is `hcNoContext` (or there is no current) falls back to
    /// `TView::getHelpCtx` (own `help_ctx`, or `DRAGGING` while dragging). The
    /// dragging case is therefore preserved by the fallback leg.
    fn get_help_ctx(&self) -> crate::help::HelpCtx {
        let from_current = self
            .current
            .and_then(|id| self.index_of(id))
            .map(|i| self.children[i].view.get_help_ctx());
        match from_current {
            Some(h) if h != crate::help::HelpCtx::NO_CONTEXT => h,
            _ => self.state().get_help_ctx(),
        }
    }

    /// Descend into the `current` child to find where the hardware cursor should
    /// sit, accumulating the child's `origin` so the result is in this group's
    /// coordinate frame. `None` if there is no current child or it wants no cursor
    /// shown. The event loop walks this chain from the root to place the cursor.
    ///
    /// # Turbo Vision heritage
    /// The group case of the focused-chain cursor walk (`TView::resetCursor`).
    fn cursor_request(&self) -> Option<Point> {
        let i = self.current.and_then(|id| self.index_of(id))?;
        let child = &self.children[i];
        child
            .view
            .cursor_request()
            .map(|p| p + child.view.state().origin)
    }

    /// The group's [`View::find_mut`] override — the recursive tree-walk.
    /// One pass per child: match the child's own id, else recurse into it. A
    /// two-pass split (all direct children, then all recursions) fails the borrow
    /// checker (E0499) — the early `return` of `&mut` in the first loop escapes to
    /// the function's return lifetime, blocking the second `iter_mut`. The single
    /// pass is semantically identical here: ids are process-global and unique, so
    /// at most one node in the subtree matches and visit order is irrelevant.
    fn find_mut(&mut self, id: ViewId) -> Option<&mut dyn View> {
        for child in self.children.iter_mut() {
            if child.id == id {
                return Some(child.view.as_mut());
            }
            if let Some(v) = child.view.find_mut(id) {
                return Some(v);
            }
        }
        None
    }

    /// The group's [`View::descendant_global_bounds`] override — resolve `id` to its
    /// absolute bounds. `acc` is THIS group's absolute origin; a child's
    /// owner-local bounds come from `child.view.state().get_bounds()` (the `Child`
    /// struct carries no bounds field). Mirrors [`find_mut`](Self::find_mut)'s
    /// recursion but accumulates origins: the child's absolute origin is `acc +
    /// child.origin`, and the returned rect is the child's size placed at that
    /// origin. Recurses into each child with the child's accumulated origin, so it
    /// is faithful for any nesting depth (root → dialog → link).
    fn descendant_global_bounds(&self, id: ViewId, acc: Point) -> Option<Rect> {
        for child in &self.children {
            let b = child.view.state().get_bounds(); // owner-local
            let child_origin = Point::new(acc.x + b.a.x, acc.y + b.a.y);
            if child.id == id {
                let w = b.b.x - b.a.x;
                let h = b.b.y - b.a.y;
                return Some(Rect::new(
                    child_origin.x,
                    child_origin.y,
                    child_origin.x + w,
                    child_origin.y + h,
                ));
            }
            if let Some(r) = child.view.descendant_global_bounds(id, child_origin) {
                return Some(r);
            }
        }
        None
    }

    /// The group's [`View::remove_descendant`] override — route to the owning
    /// group's [`remove`](Self::remove) (which also re-establishes currency). If
    /// `id` is a direct child, remove it here; otherwise recurse so the group that
    /// actually owns it does the removal.
    fn remove_descendant(&mut self, id: ViewId, ctx: &mut Context) -> bool {
        if self.index_of(id).is_some() {
            self.remove(id, ctx); // direct child: removal + reset_current
            return true;
        }
        for child in self.children.iter_mut() {
            if child.view.remove_descendant(id, ctx) {
                return true;
            }
        }
        false
    }

    /// The group's [`View::focus_descendant`] override — route to the owning
    /// group's [`focus_child`](Self::focus_child). If `id` is a **direct child**,
    /// gate on selectability and select it; either way `id` was found, so **return
    /// `true` to stop the walk** (a non-selectable match is still a match — it just
    /// doesn't get focused). Otherwise recurse so the group that actually owns `id`
    /// does the selection.
    fn focus_descendant(&mut self, id: ViewId, ctx: &mut Context) -> bool {
        if let Some(i) = self.index_of(id) {
            // Direct child: gate on selectability, then select.
            if self.children[i].view.state().options.selectable {
                self.focus_child(id, ctx);
            }
            return true; // found (selectable or not) — stop walking.
        }
        for child in self.children.iter_mut() {
            if child.view.focus_descendant(id, ctx) {
                return true;
            }
        }
        false
    }

    /// A group has a focusable leaf iff any visible, enabled child does (a leaf
    /// child reports itself; a group child recurses).
    fn has_focusable_leaf(&self) -> bool {
        self.children.iter().any(|c| {
            let s = c.view.state();
            s.state.visible && !s.state.disabled && c.view.has_focusable_leaf()
        })
    }

    /// Enter this group at its edge child for hierarchical Tab: the first child
    /// (Tab) or last (Shift-Tab) that has a focusable leaf, made current and then
    /// recursively entered. Returns whether a leaf was focused.
    fn focus_to_edge(&mut self, backward: bool, ctx: &mut Context) -> bool {
        let n = self.children.len();
        let order: Vec<usize> = if backward {
            (0..n).rev().collect()
        } else {
            (0..n).collect()
        };
        for idx in order {
            let s = self.children[idx].view.state();
            let eligible = s.state.visible && !s.state.disabled && s.options.selectable;
            if eligible && self.children[idx].view.has_focusable_leaf() {
                let id = self.children[idx].id;
                self.focus_child(id, ctx);
                self.children[idx].view.focus_to_edge(backward, ctx);
                return true;
            }
        }
        false
    }

    /// The group's [`View::set_visible_descendant`] — write the visible flag in
    /// the OWNING group and run the visibility-change currency tail: if the flag
    /// actually changed and the child is selectable, re-establish currency
    /// ([`reset_current`](Self::reset_current)) — in **both** directions (show and
    /// hide). A no-change write runs no tail (an idempotent toggle has no cascade).
    /// Recurses like [`find_mut`](Self::find_mut) when `id` is not a direct child.
    ///
    /// Delivers [`StateFlag::Visible`](crate::view::StateFlag::Visible) via
    /// `child.set_state` so that widgets that own sibling scroll bars (e.g.
    /// `ListViewer`) can show/hide them in sync — mirroring C++ `setState(sfVisible)`.
    fn set_visible_descendant(&mut self, id: ViewId, visible: bool, ctx: &mut Context) -> bool {
        if let Some(i) = self.index_of(id) {
            let was_visible = self.children[i].view.state().state.visible;
            if was_visible != visible {
                // Deliver via set_state so overriding widgets (e.g. list viewers
                // with scroll bars) can react.  set_state flips the flag through
                // set_flag and runs any widget-specific side effects.
                self.children[i]
                    .view
                    .set_state(StateFlag::Visible, visible, ctx);
                if self.children[i].view.state().options.selectable {
                    // The visibility-change tail: re-establish the owning
                    // group's own currency.
                    Group::reset_current(self, ctx);
                }
            }
            return true; // found (changed or not) — stop walking.
        }
        for child in self.children.iter_mut() {
            if child.view.set_visible_descendant(id, visible, ctx) {
                return true;
            }
        }
        false
    }

    /// The [`View::reset_current`] override — re-establish the group's internal
    /// currency (first visible+selectable child). Delegates to the inherent
    /// [`Group::reset_current`](Group::reset_current). Inherent methods take
    /// resolution priority over same-named trait methods, so the fully-qualified
    /// call below is NOT recursion — it dispatches to the inherent fn, not back
    /// here.
    fn reset_current(&mut self, ctx: &mut Context) {
        Group::reset_current(self, ctx)
    }

    /// The group's [`View::settle_currency`] — run pending insert-time currency
    /// cascades, POST-ORDER (children first, so a child group's currency exists
    /// before this group's focus cascade descends into it). Runs the INHERENT
    /// [`Group::reset_current`] (not the trait override) — see the trait method's
    /// doc for why (a file dialog's one-time init must stay on the
    /// [`exec_view`](crate::app::Program::exec_view) path).
    fn settle_currency(&mut self, ctx: &mut Context) {
        for i in 0..self.children.len() {
            self.children[i].view.settle_currency(ctx);
        }
        if self.currency_dirty {
            self.currency_dirty = false;
            Group::reset_current(self, ctx);
        }
    }

    /// Validity check. For the "may I release focus?" command
    /// ([`Command::RELEASED_FOCUS`]) it defers to the current child only when that
    /// child opts into validation (else `true`); for any other command **every**
    /// child must be valid.
    ///
    /// Threads `&mut Context` because a child's validity check may itself want to
    /// pop up a modal message box. The all-children branch stops at the **first**
    /// invalid child, which also ensures only that child enqueues an error box (a
    /// single box, not one per invalid field).
    ///
    /// # Turbo Vision heritage
    /// Ports `TGroup::valid`.
    fn valid(&mut self, cmd: Command, ctx: &mut Context) -> bool {
        if cmd == Command::RELEASED_FOCUS {
            match self.current.and_then(|id| self.index_of(id)) {
                Some(i) if self.children[i].view.state().options.validate => {
                    self.children[i].view.valid(cmd, ctx)
                }
                _ => true,
            }
        } else {
            for child in self.children.iter_mut() {
                if !child.view.valid(cmd, ctx) {
                    return false; // firstThat: stop at the first invalid child
                }
            }
            true
        }
    }

    /// Finish initialization for every child after the group (or the dialog
    /// that owns it) is fully wired into the view tree.
    ///
    /// `awaken` is called once by the framework — after all siblings and the
    /// group itself have been inserted — so each child can safely inspect the
    /// tree topology or its siblings at this point. The call order across
    /// children is intentionally unspecified; do not rely on one child's
    /// `awaken` completing before another's.
    ///
    /// **When to override** (in a type that embeds a `Group`): if your
    /// container widget needs a one-time startup action that requires a live
    /// `Context` (e.g. triggering a data scatter), override
    /// [`View::awaken`] and call `self.group.awaken()` first so children
    /// are ready before your own logic runs.
    ///
    /// # Turbo Vision heritage
    /// Ports `TGroup::awaken`, which iterates each subview calling
    /// `subview->awaken()` before the group's own `TView::awaken`. The
    /// children-first ordering is preserved.
    fn awaken(&mut self) {
        for child in self.children.iter_mut() {
            child.view.awaken();
        }
    }

    /// The three-phase event router.
    ///
    /// * **focused events** ([`KeyDown`](Event::KeyDown)/[`Command`](Event::Command)):
    ///   a pre-process pass (top→bottom, only children that opt into pre-processing)
    ///   → the focused pass (the current child) → a post-process pass (top→bottom,
    ///   only children that opt into post-processing).
    /// * **broadcasts**: delivered to every child (top→bottom).
    /// * **positional** (mouse): the topmost **visible** child whose bounds
    ///   contain the (group-local) position — with mouse-down auto-select applied
    ///   before delivery, so clicking a selectable child focuses it.
    ///
    /// A group does not run a self-focus step on itself here: selecting a child is
    /// the *parent's* job, and the base view's event handler is a no-op.
    ///
    /// **Owner size:** the routing body is bracketed by a
    /// `ctx.set_owner_size(self.size)` / restore so a child can read its owner's
    /// size (e.g. a window computing its zoomed bounds). The restore is
    /// **unconditional** because the actual routing lives in
    /// [`route_event`](Self::route_event), which may `return` early — the bracket
    /// here guarantees a parent group's later sibling deliveries never see this
    /// group's size.
    ///
    /// # Turbo Vision heritage
    /// Ports `TGroup::handleEvent` (the pre/focused/post phase router and
    /// positional hit-test).
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        let saved_owner_size = ctx.owner_size();
        ctx.set_owner_size(self.st.size); // children's owner is THIS group (Copy read first)
        self.route_event(ev, ctx);
        // Hierarchical Tab traversal. `route_event` delivered the key to the
        // focused child first (recursing into nested groups, which advance and
        // consume it there). A still-live Tab/Shift-Tab means the focused subtree
        // was at its edge, so advance to the next focusable leaf at THIS level
        // (entering a child group at its edge). Leaving it unconsumed lets it
        // bubble to the parent group; the window wraps at the top. A widget that
        // owns Tab (e.g. a multi-line editor) consumes it in `route_event`, so it
        // never reaches here.
        let is_tab = matches!(ev, Event::KeyDown(k) if k.key == Key::Tab);
        if is_tab {
            let backward = matches!(ev, Event::KeyDown(k) if k.modifiers.shift);
            if self.advance_no_wrap(backward, ctx) {
                ev.clear();
            }
        }
        ctx.set_owner_size(saved_owner_size); // unconditional restore (see doc above)
    }
}

impl Group {
    /// The three-phase router body of [`handle_event`](View::handle_event),
    /// extracted so the `owner_size` save/restore bracket in `handle_event` is
    /// unconditional even though this fn may `return` early.
    fn route_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        let n = self.children.len();
        match ev {
            // -- focusedEvents: pre-process → focused → post-process ----------
            //
            // The phase bracket ports the C++ `phase = ph…` writes around each
            // leg (`tgroup.cpp:362-371`); a child reads it via `ctx.phase()`
            // (the `owner->phase` successor). Save/restore makes one shared
            // Context field behave like the per-group C++ field under nesting:
            // a nested Group's own bracket restores to ITS saved value — which
            // is exactly the phase the OUTER group set for the section this
            // nested group is being delivered in — so "a child reads its
            // immediate owner's phase" holds. Leaf views never write the field.
            Event::KeyDown(_) | Event::Command(_) | Event::Paste(_) => {
                let saved_phase = ctx.phase();
                // phPreProcess: forEach top→bottom, ofPreProcess children only.
                ctx.set_phase(Phase::PreProcess);
                for i in (0..n).rev() {
                    if self.children[i].view.state().options.pre_process {
                        self.deliver(i, ev, ctx);
                    }
                }
                // phFocused: the current child only (no phase-option gate).
                ctx.set_phase(Phase::Focused);
                if let Some(i) = self.current.and_then(|id| self.index_of(id)) {
                    self.deliver(i, ev, ctx);
                }
                // phPostProcess: forEach top→bottom, ofPostProcess children only.
                ctx.set_phase(Phase::PostProcess);
                for i in (0..n).rev() {
                    if self.children[i].view.state().options.post_process {
                        self.deliver(i, ev, ctx);
                    }
                }
                ctx.set_phase(saved_phase);
            }
            // -- broadcast: phFocused, every child (incl. disabled) -----------
            // Also carries timer-expiry (`Event::Timer`), which is broadcast-class
            // (the `evBroadcast cmTimerExpired` successor) and so delivers to every
            // child identically.
            //
            // No `set_phase` here (nor in the positional arm below): the C++
            // sets `phase = phFocused` for both (`tgroup.cpp:373-376`), but tvision-rs
            // broadcasts are re-dispatched by the pump on a fresh `Context`
            // already defaulting to `Focused`, and the focused-events bracket
            // above restores on exit — so the field is always `Focused` here.
            Event::Broadcast { .. } | Event::Timer(_) => {
                for i in (0..n).rev() {
                    self.deliver(i, ev, ctx);
                }
            }
            // -- evMouseWheel: non-positional, non-focused → forEach every child
            // until consumed. Faithful to `views.h:199`
            // (`positionalEvents = evMouse & ~evMouseWheel`) and
            // `TGroup::handleEvent`'s `else`/`forEach(doHandleEvent)` branch:
            // the wheel broadcasts to every child top→bottom, and `deliver`
            // early-returns once a child has consumed it (`clearEvent`), so the
            // active window's scrollbar gets it regardless of cursor position.
            Event::MouseWheel(_) => {
                for i in (0..n).rev() {
                    self.deliver(i, ev, ctx);
                }
            }
            // -- positionalEvents: the topmost visible child under the cursor --
            Event::MouseDown(_) | Event::MouseUp(_) | Event::MouseMove(_) | Event::MouseAuto(_) => {
                let Some(pos) = mouse_pos(ev) else {
                    return;
                };
                // firstThat(hasMouse) — topmost (rev) visible child containing pos.
                let target = (0..n).rev().find(|&i| {
                    let s = self.children[i].view.state();
                    s.state.visible && s.get_bounds().contains(pos)
                });
                if let Some(ti) = target {
                    // carryover #1: relocated TView::handleEvent mouse-down auto-select.
                    // Gated on `grabs_focus_on_click()`: in C++ each view chooses
                    // whether to invoke the base auto-select; the default is to
                    // invoke it (true), and TButton is the canonical opt-OUT
                    // (it auto-selects only when `bfGrabFocus` is set). A view
                    // returning false is not focused by the click but still
                    // receives it below (it can press without becoming current).
                    if matches!(ev, Event::MouseDown(_)) {
                        let s = self.children[ti].view.state();
                        let (selectable, selected, disabled) =
                            (s.options.selectable, s.state.selected, s.state.disabled);
                        let first_click = s.options.first_click;
                        let grabs = self.children[ti].view.grabs_focus_on_click();
                        let id = self.children[ti].id;
                        if grabs && selectable && !selected && !disabled {
                            let ok = self.focus_child(id, ctx);
                            if !ok || !first_click {
                                ev.clear();
                            }
                        }
                    }
                    if !ev.is_nothing() {
                        self.deliver(ti, ev, ctx);
                    }
                }
            }
            Event::Nothing => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::color::{Color, Style};
    use crate::command::Command;
    use crate::event::{Key, KeyEvent, KeyModifiers, MouseButtons, MouseEvent};
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use crate::timer::TimerQueue;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    // -- test harness --------------------------------------------------------

    /// Build a throwaway `Context` over loop-owned locals, run `f`, return its
    /// value. Drained `out_events` is left in `out` for inspection.
    fn with_ctx<R>(
        out: &mut VecDeque<Event>,
        timers: &mut TimerQueue,
        f: impl FnOnce(&mut Context) -> R,
    ) -> R {
        let mut deferred: Vec<crate::view::Deferred> = Vec::new();
        let mut ctx = Context::new(out, timers, 0, &mut deferred);
        f(&mut ctx)
    }

    fn key(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(k, KeyModifiers::default()))
    }

    fn shift_key(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(
            k,
            KeyModifiers {
                shift: true,
                ..Default::default()
            },
        ))
    }

    /// A selectable leaf that does NOT consume keys (unlike `Probe`), so Tab passes
    /// through to the group's hierarchical traversal — models an InputLine/ListBox.
    struct Pass(ViewState);
    impl Pass {
        fn boxed() -> Box<dyn View> {
            let mut st = ViewState::new(Rect::new(0, 0, 1, 1));
            st.options.selectable = true;
            Box::new(Pass(st))
        }
    }
    impl View for Pass {
        fn state(&self) -> &ViewState {
            &self.0
        }
        fn state_mut(&mut self) -> &mut ViewState {
            &mut self.0
        }
        fn draw(&mut self, _ctx: &mut DrawCtx) {}
        fn handle_event(&mut self, _ev: &mut Event, _ctx: &mut Context) {}
    }

    #[test]
    fn tab_traverses_nested_groups_hierarchically() {
        // root = [ A, G[ B, C ], D ]. Tab must visit A → B → C → D (descending into
        // G at its first leaf, ascending out of G to D), and Shift-Tab reverses.
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();

        let mut sub = Group::new(Rect::new(0, 0, 4, 2));
        let b = sub.insert(Pass::boxed());
        let c = sub.insert(Pass::boxed());

        let mut root = Group::new(Rect::new(0, 0, 12, 2));
        // Mark the root focused so the `focused` flag propagates down the current
        // chain (a real window/desktop does this); then it is a correct global
        // focus-tip indicator for the assertions below.
        root.state_mut().state.focused = true;
        let a = root.insert(Pass::boxed());
        let _g = root.insert(Box::new(sub));
        let d = root.insert(Pass::boxed());

        let focused = |root: &mut Group, id| {
            root.find_mut(id)
                .map(|v| v.state().state.focused)
                .unwrap_or(false)
        };

        with_ctx(&mut out, &mut timers, |ctx| {
            root.focus_child(a, ctx);
            assert!(focused(&mut root, a), "seed: A focused");

            // Tab: A → B (descend into G at its first leaf)
            let mut ev = key(Key::Tab);
            root.handle_event(&mut ev, ctx);
            assert!(ev.is_nothing(), "Tab consumed");
            assert!(focused(&mut root, b) && !focused(&mut root, a), "A → B");

            // Tab: B → C (within G)
            let mut ev = key(Key::Tab);
            root.handle_event(&mut ev, ctx);
            assert!(focused(&mut root, c) && !focused(&mut root, b), "B → C");

            // Tab: C → D (ascend out of G)
            let mut ev = key(Key::Tab);
            root.handle_event(&mut ev, ctx);
            assert!(focused(&mut root, d) && !focused(&mut root, c), "C → D");

            // Tab at the last leaf: nothing left at the root → unconsumed (a window
            // would wrap). focus_to_edge models the wrap back to the first leaf A.
            let mut ev = key(Key::Tab);
            root.handle_event(&mut ev, ctx);
            assert!(
                !ev.is_nothing(),
                "Tab past the last leaf bubbles (unconsumed)"
            );
            assert!(focused(&mut root, d), "still on D until the window wraps");
            root.focus_to_edge(false, ctx);
            assert!(focused(&mut root, a) && !focused(&mut root, d), "wrap → A");

            // Shift-Tab reverses: A → D (wrap backward), then D → C (into G at last leaf)
            let mut ev = shift_key(Key::Tab);
            root.handle_event(&mut ev, ctx);
            assert!(!ev.is_nothing(), "Shift-Tab before the first leaf bubbles");
            root.focus_to_edge(true, ctx);
            assert!(focused(&mut root, d), "Shift-Tab wrap → D (last leaf)");

            let mut ev = shift_key(Key::Tab);
            root.handle_event(&mut ev, ctx);
            assert!(
                focused(&mut root, c) && !focused(&mut root, d),
                "Shift-Tab D → C"
            );
        });
    }

    fn mouse_down_at(x: i32, y: i32) -> Event {
        Event::MouseDown(MouseEvent {
            position: Point::new(x, y),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        })
    }

    /// Regression: the mouse wheel is a non-positional event class that a group
    /// broadcasts to every child until consumed — NOT a positional click
    /// hit-tested under the cursor. A wheel over empty content must still reach
    /// the active window's scrollbar, while a real click there must not.
    #[test]
    fn wheel_broadcasts_to_scrollbar_off_the_bar() {
        use crate::event::MouseWheel;
        use crate::widgets::ScrollBar;

        // A wide group with a 1-col vertical scrollbar pinned to the right edge.
        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        let bar_id = {
            let mut bar = ScrollBar::new(Rect::new(19, 0, 20, 10)); // vertical (width 1)
            let mut out = VecDeque::new();
            let mut timers = TimerQueue::new();
            with_ctx(&mut out, &mut timers, |ctx| {
                bar.set_params(50, 0, 100, 10, 1, ctx);
            });
            group.insert(Box::new(bar))
        };

        let bar_value = |g: &mut Group| {
            g.find_mut(bar_id)
                .and_then(|v| v.as_any_mut())
                .and_then(|a| a.downcast_mut::<ScrollBar>())
                .map(|b| b.value)
                .unwrap()
        };
        assert_eq!(bar_value(&mut group), 50, "initial value");

        // A wheel-down at (2, 5) — NOT over the bar (which is column 19).
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let off_bar = Point::new(2, 5);
        with_ctx(&mut out, &mut timers, |ctx| {
            let mut ev = Event::MouseWheel(MouseEvent {
                position: off_bar,
                wheel: MouseWheel::Down,
                ..Default::default()
            });
            group.handle_event(&mut ev, ctx);
            assert!(
                ev.is_nothing(),
                "wheel consumed by the scrollbar via broadcast"
            );
        });
        // Down wheel: set_value(value + 3 * arrow_step) = 50 + 3 = 53.
        assert_eq!(
            bar_value(&mut group),
            53,
            "wheel reached the bar via broadcast, off the bar's column"
        );

        // A real left-click at the SAME off-bar position must NOT reach the bar
        // (positional routing still hit-tests; the bar is in column 19).
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        with_ctx(&mut out, &mut timers, |ctx| {
            let mut ev = mouse_down_at(off_bar.x, off_bar.y);
            group.handle_event(&mut ev, ctx);
        });
        assert_eq!(
            bar_value(&mut group),
            53,
            "a positional click off the bar must not reach it"
        );
    }

    /// A probe view: fills its extent with `ch` and records every event it is
    /// handed (post-translation), so tests can assert routing/order/coords.
    struct Probe {
        st: ViewState,
        ch: char,
        log: Rc<RefCell<Vec<Event>>>,
        /// Mirrors [`View::grabs_focus_on_click`] (default true). Set false to
        /// model a control that takes a click without grabbing focus (e.g. a
        /// button that fires but does not become current).
        grabs: bool,
    }

    impl Probe {
        fn new(bounds: Rect, ch: char, log: Rc<RefCell<Vec<Event>>>) -> Self {
            Probe {
                st: ViewState::new(bounds),
                ch,
                log,
                grabs: true,
            }
        }
        fn boxed(bounds: Rect, ch: char, log: Rc<RefCell<Vec<Event>>>) -> Box<dyn View> {
            Box::new(Probe::new(bounds, ch, log))
        }
    }

    impl View for Probe {
        fn state(&self) -> &ViewState {
            &self.st
        }
        fn state_mut(&mut self) -> &mut ViewState {
            &mut self.st
        }
        fn draw(&mut self, ctx: &mut DrawCtx) {
            let ext = self.st.get_extent();
            ctx.fill(ext, self.ch, Style::new(Color::Bios(0xF), Color::Bios(0x1)));
        }
        fn handle_event(&mut self, ev: &mut Event, _ctx: &mut Context) {
            self.log.borrow_mut().push(ev.clone());
            // Consume key/command/mouse so we can observe "reached me". Broadcasts
            // are passed through (TV convention: multiple views react to one), so
            // they reach every child.
            if !matches!(ev, Event::Broadcast { .. } | Event::Timer(_)) {
                ev.clear();
            }
        }
        fn grabs_focus_on_click(&self) -> bool {
            self.grabs
        }
    }

    /// A child that reports a fixed `valid()` result (and records nothing).
    struct ValidProbe {
        st: ViewState,
        valid: bool,
    }
    impl View for ValidProbe {
        fn state(&self) -> &ViewState {
            &self.st
        }
        fn state_mut(&mut self) -> &mut ViewState {
            &mut self.st
        }
        fn draw(&mut self, _ctx: &mut DrawCtx) {}
        fn valid(&mut self, _cmd: Command, _ctx: &mut Context) -> bool {
            self.valid
        }
    }

    // -- 1. Z-order draw (mandatory snapshot) --------------------------------

    #[test]
    fn z_order_draw_topmost_wins_overlap_snapshot() {
        let theme = Theme::classic_blue();
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        let mut group = Group::new(Rect::new(0, 0, 6, 3));
        with_ctx(&mut out, &mut timers, |_ctx| {
            // A (bottom) fills the whole area with 'A'; B (top) overlaps the
            // right half with 'B'. B is inserted last -> drawn last -> wins.
            group.insert(Probe::boxed(Rect::new(0, 0, 6, 3), 'A', log.clone()));
            group.insert(Probe::boxed(Rect::new(3, 0, 6, 3), 'B', log.clone()));
        });

        let mut view: Box<dyn View> = Box::new(group);
        let (backend, screen) = HeadlessBackend::new(6, 3);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = view.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            view.draw(&mut dc);
        });
        insta::assert_snapshot!(screen.snapshot());
    }

    // -- 2. positional routing + local coords --------------------------------

    #[test]
    fn positional_routing_translates_to_child_local() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log_a = Rc::new(RefCell::new(Vec::new()));
        let log_b = Rc::new(RefCell::new(Vec::new()));

        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        with_ctx(&mut out, &mut timers, |_ctx| {
            group.insert(Probe::boxed(Rect::new(0, 0, 5, 5), 'A', log_a.clone()));
            group.insert(Probe::boxed(Rect::new(10, 4, 16, 9), 'B', log_b.clone()));
        });

        // Group-local (12, 5) is inside B (origin 10,4) -> B-local (2, 1).
        let mut ev = mouse_down_at(12, 5);
        with_ctx(&mut out, &mut timers, |ctx| {
            group.handle_event(&mut ev, ctx)
        });
        assert_eq!(log_a.borrow().len(), 0, "A must not see a click inside B");
        assert_eq!(log_b.borrow().len(), 1, "B must see the click");
        match log_b.borrow()[0] {
            Event::MouseDown(m) => assert_eq!(m.position, Point::new(2, 1), "B-local coords"),
            _ => panic!("expected MouseDown"),
        }

        // A click outside every child reaches nobody.
        log_b.borrow_mut().clear();
        let mut ev2 = mouse_down_at(8, 8);
        with_ctx(&mut out, &mut timers, |ctx| {
            group.handle_event(&mut ev2, ctx)
        });
        assert!(log_a.borrow().is_empty() && log_b.borrow().is_empty());
        assert!(!ev2.is_nothing(), "an unhit click is left live (no bubble)");
    }

    // -- 3. carryover #1: mouse-down auto-select -----------------------------

    #[test]
    fn mouse_down_auto_selects_and_consumes_without_first_click() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        let id = with_ctx(&mut out, &mut timers, |_ctx| {
            let mut p = Probe::new(Rect::new(0, 0, 5, 5), 'A', log.clone());
            p.st.options.selectable = true; // selectable, first_click = false
            group.insert(Box::new(p))
        });

        let mut ev = mouse_down_at(2, 2);
        with_ctx(&mut out, &mut timers, |ctx| {
            group.handle_event(&mut ev, ctx)
        });

        assert_eq!(group.current(), Some(id), "child became current");
        assert!(
            group.children[0].view.state().state.selected,
            "child became selected"
        );
        assert!(
            log.borrow().is_empty(),
            "first_click=false consumes the selecting click"
        );
        assert!(ev.is_nothing(), "event consumed");
    }

    #[test]
    fn mouse_down_auto_select_with_first_click_passes_event_through() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        with_ctx(&mut out, &mut timers, |_ctx| {
            let mut p = Probe::new(Rect::new(0, 0, 5, 5), 'A', log.clone());
            p.st.options.selectable = true;
            p.st.options.first_click = true;
            group.insert(Box::new(p));
        });

        let mut ev = mouse_down_at(1, 1);
        with_ctx(&mut out, &mut timers, |ctx| {
            group.handle_event(&mut ev, ctx)
        });
        assert_eq!(
            log.borrow().len(),
            1,
            "first_click=true: child also receives the click"
        );
    }

    #[test]
    fn mouse_down_does_not_select_disabled_or_nonselectable() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        with_ctx(&mut out, &mut timers, |_ctx| {
            // non-selectable child
            group.insert(Probe::boxed(Rect::new(0, 0, 5, 5), 'A', log.clone()));
        });
        let mut ev = mouse_down_at(2, 2);
        with_ctx(&mut out, &mut timers, |ctx| {
            group.handle_event(&mut ev, ctx)
        });
        assert_eq!(group.current(), None, "non-selectable child not selected");
        // It still receives the (non-consumed) click since auto-select did nothing.
        assert_eq!(log.borrow().len(), 1);

        // disabled selectable child
        let log2 = Rc::new(RefCell::new(Vec::new()));
        let mut group2 = Group::new(Rect::new(0, 0, 20, 10));
        with_ctx(&mut out, &mut timers, |_ctx| {
            let mut p = Probe::new(Rect::new(0, 0, 5, 5), 'B', log2.clone());
            p.st.options.selectable = true;
            p.st.state.disabled = true;
            group2.insert(Box::new(p));
        });
        let mut ev2 = mouse_down_at(1, 1);
        with_ctx(&mut out, &mut timers, |ctx| {
            group2.handle_event(&mut ev2, ctx)
        });
        assert_eq!(group2.current(), None, "disabled child not selected");
        assert!(log2.borrow().is_empty(), "disabled child receives no event");
    }

    #[test]
    fn mouse_down_does_not_select_when_grabs_focus_on_click_is_false() {
        // A selectable, not-selected, not-disabled child whose
        // grabs_focus_on_click() == false (a TButton without bfGrabFocus) must
        // NOT become current on click, but MUST still receive the click so it
        // can act (press) without stealing focus.
        //
        // This test BITES against the old unconditional code: there, the
        // `selectable && !selected && !disabled` block would call focus_child
        // and make this child current (first_click defaults to false, so the
        // click would also be consumed and never reach the child). The
        // grabs_focus_on_click() gate is the only thing keeping current == None
        // and the event live — remove it and both assertions below fail.
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        with_ctx(&mut out, &mut timers, |_ctx| {
            let mut p = Probe::new(Rect::new(0, 0, 5, 5), 'A', log.clone());
            p.st.options.selectable = true; // selectable, first_click = false
            p.grabs = false; // opts OUT of auto-select (no bfGrabFocus)
            group.insert(Box::new(p));
        });

        let mut ev = mouse_down_at(2, 2);
        with_ctx(&mut out, &mut timers, |ctx| {
            group.handle_event(&mut ev, ctx)
        });

        assert_eq!(
            group.current(),
            None,
            "grabs_focus_on_click()==false: child not made current by the click"
        );
        assert!(
            !group.children[0].view.state().state.selected,
            "child not selected"
        );
        assert_eq!(
            log.borrow().len(),
            1,
            "child still receives the mouse-down (event left live by the group)"
        );
    }

    // -- 4. carryover #2: focus broadcast ------------------------------------

    #[test]
    fn focused_group_select_drives_focus_broadcasts() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        group.st.state.focused = true; // the group itself is focused
        let (id_a, id_b) = with_ctx(&mut out, &mut timers, |_ctx| {
            let mut a = Probe::new(Rect::new(0, 0, 5, 5), 'A', log.clone());
            a.st.options.selectable = true;
            let ida = group.insert(Box::new(a));
            let mut b = Probe::new(Rect::new(6, 0, 11, 5), 'B', log.clone());
            b.st.options.selectable = true;
            let idb = group.insert(Box::new(b));
            (ida, idb)
        });

        // Select A: RECEIVED_FOCUS for A.
        out.clear();
        with_ctx(&mut out, &mut timers, |ctx| {
            group.set_current(Some(id_a), SelectMode::Normal, ctx)
        });
        assert!(
            out.iter().any(|e| matches!(
                e,
                Event::Broadcast { command, .. } if *command == Command::RECEIVED_FOCUS
            )),
            "selecting A while focused broadcasts RECEIVED_FOCUS"
        );

        // Switch to B: RELEASED_FOCUS for A then RECEIVED_FOCUS for B.
        out.clear();
        with_ctx(&mut out, &mut timers, |ctx| {
            group.set_current(Some(id_b), SelectMode::Normal, ctx)
        });
        let events: Vec<Event> = out.iter().cloned().collect();
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Broadcast { command, .. } if command == &Command::RELEASED_FOCUS
            )),
            "A releases focus"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Broadcast { command, .. } if command == &Command::RECEIVED_FOCUS
            )),
            "B receives focus"
        );
        assert_eq!(group.current(), Some(id_b));
    }

    #[test]
    fn unfocused_group_select_does_not_broadcast_focus() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        let mut group = Group::new(Rect::new(0, 0, 20, 10)); // not focused
        let id = with_ctx(&mut out, &mut timers, |_ctx| {
            let mut a = Probe::new(Rect::new(0, 0, 5, 5), 'A', log.clone());
            a.st.options.selectable = true;
            group.insert(Box::new(a))
        });
        out.clear();
        with_ctx(&mut out, &mut timers, |ctx| {
            group.set_current(Some(id), SelectMode::Normal, ctx)
        });
        assert!(
            !out.iter().any(|e| matches!(
                e,
                Event::Broadcast { command, .. } if *command == Command::RECEIVED_FOCUS
            )),
            "unfocused group must not broadcast focus on select"
        );
        // But the child is still selected.
        assert!(group.children[0].view.state().state.selected);
    }

    // -- 5. three-phase focused dispatch -------------------------------------

    /// Order/phase probe for the three-phase dispatch tests: records its tag +
    /// the [`Phase`] the routing group set for the delivery (read via
    /// `ctx.phase()`). Does NOT consume, so all legs run.
    struct Tagged {
        st: ViewState,
        tag: char,
        order: Rc<RefCell<Vec<(char, Phase)>>>,
    }
    impl View for Tagged {
        fn state(&self) -> &ViewState {
            &self.st
        }
        fn state_mut(&mut self) -> &mut ViewState {
            &mut self.st
        }
        fn draw(&mut self, _ctx: &mut DrawCtx) {}
        fn handle_event(&mut self, _ev: &mut Event, ctx: &mut Context) {
            self.order.borrow_mut().push((self.tag, ctx.phase()));
        }
    }

    #[test]
    fn focused_dispatch_visits_pre_then_current_then_post() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        // Shared order log: each probe pushes a tagged event so we read order.
        let order = Rc::new(RefCell::new(Vec::new()));

        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        let cur_id = with_ctx(&mut out, &mut timers, |_ctx| {
            // pre-process child
            let mut pre = Tagged {
                st: ViewState::new(Rect::new(0, 0, 5, 5)),
                tag: 'P',
                order: order.clone(),
            };
            pre.st.options.pre_process = true;
            group.insert(Box::new(pre));

            // current child (plain)
            let mut cur = Tagged {
                st: ViewState::new(Rect::new(6, 0, 11, 5)),
                tag: 'C',
                order: order.clone(),
            };
            cur.st.options.selectable = true;
            let id = group.insert(Box::new(cur));

            // post-process child
            let mut post = Tagged {
                st: ViewState::new(Rect::new(12, 0, 17, 5)),
                tag: 'O',
                order: order.clone(),
            };
            post.st.options.post_process = true;
            group.insert(Box::new(post));

            // a plain non-pre/post non-current child must be skipped entirely
            let mut plain = Tagged {
                st: ViewState::new(Rect::new(0, 6, 5, 9)),
                tag: 'X',
                order: order.clone(),
            };
            plain.st.options.selectable = true;
            group.insert(Box::new(plain));

            id
        });
        with_ctx(&mut out, &mut timers, |ctx| {
            group.set_current(Some(cur_id), SelectMode::Normal, ctx)
        });

        let mut ev = key(Key::Char('z'));
        with_ctx(&mut out, &mut timers, |ctx| {
            group.handle_event(&mut ev, ctx)
        });
        assert_eq!(
            *order.borrow(),
            vec![
                ('P', Phase::PreProcess),
                ('C', Phase::Focused),
                ('O', Phase::PostProcess),
            ],
            "pre-process, then current, then post-process; plain child skipped; \
             each delivery sees its leg's phase (tgroup.cpp:362-371)"
        );
    }

    /// Nesting: a nested group runs its OWN three-phase bracket and restores
    /// the phase the outer group set for the section it was delivered in.
    /// The nested group is an `ofPreProcess` child of the outer, so its inner
    /// post-process probe sees `PostProcess` (the inner group's own leg) while
    /// the outer pre-loop sibling delivered AFTER it sees `PreProcess` again
    /// (the restore).
    #[test]
    fn nested_group_restores_outer_sections_phase() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let order = Rc::new(RefCell::new(Vec::new()));

        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        with_ctx(&mut out, &mut timers, |_ctx| {
            // Outer pre-process probe, inserted FIRST (index 0) so the rev
            // pre-loop visits it AFTER the nested group (index 1).
            let mut p2 = Tagged {
                st: ViewState::new(Rect::new(0, 6, 5, 9)),
                tag: 'Q',
                order: order.clone(),
            };
            p2.st.options.pre_process = true;
            group.insert(Box::new(p2));

            // The nested group, an ofPreProcess child of the outer; it holds
            // one ofPostProcess probe of its own.
            let mut inner = Group::new(Rect::new(0, 0, 10, 5));
            let mut inner_post = Tagged {
                st: ViewState::new(Rect::new(0, 0, 5, 2)),
                tag: 'I',
                order: order.clone(),
            };
            inner_post.st.options.post_process = true;
            inner.insert(Box::new(inner_post));
            inner.state_mut().options.pre_process = true;
            group.insert(Box::new(inner));
        });

        let mut ev = key(Key::Char('z'));
        with_ctx(&mut out, &mut timers, |ctx| {
            group.handle_event(&mut ev, ctx);
            // The outer bracket restored to the resting default on exit.
            assert_eq!(ctx.phase(), Phase::Focused, "outer bracket restores");
        });
        assert_eq!(
            *order.borrow(),
            vec![('I', Phase::PostProcess), ('Q', Phase::PreProcess)],
            "the inner probe sees the inner group's own post-process leg; the \
             outer sibling delivered after the nested group sees the OUTER \
             section's phase again (the save/restore nesting argument)"
        );
    }

    #[test]
    fn focused_dispatch_respects_disabled_gate() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        let id = with_ctx(&mut out, &mut timers, |_ctx| {
            let mut a = Probe::new(Rect::new(0, 0, 5, 5), 'A', log.clone());
            a.st.options.selectable = true;
            a.st.state.disabled = true;
            group.insert(Box::new(a))
        });
        with_ctx(&mut out, &mut timers, |ctx| {
            group.set_current(Some(id), SelectMode::Normal, ctx)
        });
        let mut ev = key(Key::Char('q'));
        with_ctx(&mut out, &mut timers, |ctx| {
            group.handle_event(&mut ev, ctx)
        });
        assert!(
            log.borrow().is_empty(),
            "disabled current child must not receive a KeyDown"
        );
    }

    // -- 6. broadcast + tab order --------------------------------------------

    #[test]
    fn broadcast_reaches_all_children_including_disabled() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        with_ctx(&mut out, &mut timers, |_ctx| {
            group.insert(Probe::boxed(Rect::new(0, 0, 5, 5), 'A', log.clone()));
            let mut b = Probe::new(Rect::new(6, 0, 11, 5), 'B', log.clone());
            b.st.state.disabled = true;
            group.insert(Box::new(b));
        });
        let mut ev = Event::Broadcast {
            command: Command::SCROLL_BAR_CHANGED,
            source: None,
        };
        with_ctx(&mut out, &mut timers, |ctx| {
            group.handle_event(&mut ev, ctx)
        });
        assert_eq!(
            log.borrow().len(),
            2,
            "broadcast reaches all children incl. disabled"
        );
    }

    #[test]
    fn find_next_and_focus_next_tab_order_skips_and_wraps() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        // Insert A, B, C -> children = [A, B, C], top = C.
        let mut group = Group::new(Rect::new(0, 0, 30, 10));
        let (ida, _idb, idc) = with_ctx(&mut out, &mut timers, |_ctx| {
            let mut a = Probe::new(Rect::new(0, 0, 5, 5), 'A', log.clone());
            a.st.options.selectable = true;
            let ida = group.insert(Box::new(a));
            // B is NOT selectable -> must be skipped in tab order.
            let idb = group.insert(Probe::boxed(Rect::new(6, 0, 11, 5), 'B', log.clone()));
            let mut c = Probe::new(Rect::new(12, 0, 17, 5), 'C', log.clone());
            c.st.options.selectable = true;
            let idc = group.insert(Box::new(c));
            (ida, idb, idc)
        });

        // current = C. forwards (p->next = decreasing index, wrap): C -> skip B
        // (non-selectable) -> A.
        with_ctx(&mut out, &mut timers, |ctx| {
            group.set_current(Some(idc), SelectMode::Normal, ctx)
        });
        assert_eq!(group.find_next(true), Some(ida), "C -> (skip B) -> A");

        // focus_next moves current to A.
        with_ctx(&mut out, &mut timers, |ctx| {
            assert!(group.focus_next(true, ctx))
        });
        assert_eq!(group.current(), Some(ida));

        // backwards from A (p->prev = increasing index, wrap): A -> skip B -> C.
        assert_eq!(group.find_next(false), Some(idc), "A -> (skip B) -> C");
    }

    // -- 7. change_bounds + valid --------------------------------------------

    #[test]
    fn change_bounds_propagates_resize_delta_to_children() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        with_ctx(&mut out, &mut timers, |_ctx| {
            // child with gfGrowHiX|HiY -> its hi edges track the owner.
            let mut a = Probe::new(Rect::new(0, 0, 10, 5), 'A', log.clone());
            a.st.grow_mode.hi_x = true;
            a.st.grow_mode.hi_y = true;
            group.insert(Box::new(a));
        });

        // Grow the group from (20,10) to (25,13): delta (5,3).
        View::change_bounds(&mut group, Rect::new(0, 0, 25, 13));
        let child_bounds = group.children[0].view.state().get_bounds();
        assert_eq!(
            child_bounds,
            Rect::new(0, 0, 15, 8),
            "child hi edges grew by the delta"
        );
    }

    #[test]
    fn valid_is_false_iff_any_child_invalid() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();

        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        with_ctx(&mut out, &mut timers, |_ctx| {
            group.insert(Box::new(ValidProbe {
                st: ViewState::new(Rect::new(0, 0, 5, 5)),
                valid: true,
            }));
            group.insert(Box::new(ValidProbe {
                st: ViewState::new(Rect::new(6, 0, 11, 5)),
                valid: false,
            }));
        });
        // Generic command: every child must be valid.
        assert!(
            !with_ctx(&mut out, &mut timers, |ctx| group.valid(Command::OK, ctx)),
            "an invalid child fails group valid"
        );
    }

    #[test]
    fn valid_released_focus_defers_to_validating_current_only() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();

        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        // current child is invalid AND has ofValidate -> RELEASED_FOCUS fails.
        let id = with_ctx(&mut out, &mut timers, |_ctx| {
            let mut v = ValidProbe {
                st: ViewState::new(Rect::new(0, 0, 5, 5)),
                valid: false,
            };
            v.st.options.selectable = true;
            v.st.options.validate = true;
            group.insert(Box::new(v))
        });
        with_ctx(&mut out, &mut timers, |ctx| {
            group.set_current(Some(id), SelectMode::Normal, ctx)
        });
        assert!(
            !with_ctx(&mut out, &mut timers, |ctx| group
                .valid(Command::RELEASED_FOCUS, ctx)),
            "validating invalid current blocks focus release"
        );

        // Without ofValidate on the current, RELEASED_FOCUS is always true even
        // though the child is invalid.
        let mut out2 = VecDeque::new();
        let mut group2 = Group::new(Rect::new(0, 0, 20, 10));
        let id2 = with_ctx(&mut out2, &mut timers, |_ctx| {
            let mut v = ValidProbe {
                st: ViewState::new(Rect::new(0, 0, 5, 5)),
                valid: false,
            };
            v.st.options.selectable = true; // no ofValidate
            group2.insert(Box::new(v))
        });
        with_ctx(&mut out2, &mut timers, |ctx| {
            group2.set_current(Some(id2), SelectMode::Normal, ctx)
        });
        assert!(
            with_ctx(&mut out2, &mut timers, |ctx| group2
                .valid(Command::RELEASED_FOCUS, ctx)),
            "non-validating current: focus release always allowed"
        );
    }

    // -- 8. remove resets current --------------------------------------------

    #[test]
    fn remove_current_resets_to_another_selectable_child() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        let (ida, idc) = with_ctx(&mut out, &mut timers, |_ctx| {
            // children = [A, B, C]; A and C selectable, B not.
            let mut a = Probe::new(Rect::new(0, 0, 5, 5), 'A', log.clone());
            a.st.options.selectable = true;
            let ida = group.insert(Box::new(a));
            group.insert(Probe::boxed(Rect::new(6, 0, 11, 5), 'B', log.clone()));
            let mut c = Probe::new(Rect::new(12, 0, 17, 5), 'C', log.clone());
            c.st.options.selectable = true;
            let idc = group.insert(Box::new(c));
            (ida, idc)
        });

        with_ctx(&mut out, &mut timers, |ctx| {
            group.set_current(Some(idc), SelectMode::Normal, ctx)
        });
        assert_eq!(group.current(), Some(idc));

        // Remove the current (C) -> reset_current picks a visible+selectable
        // child via firstMatch order (children[0] == A is checked first).
        with_ctx(&mut out, &mut timers, |ctx| group.remove(idc, ctx));
        assert_eq!(
            group.current(),
            Some(ida),
            "removing current resets to the remaining selectable child"
        );
        assert!(group.index_of(idc).is_none(), "C is gone");
    }

    // -- 9. find_next with single eligible child -----------------------------

    #[test]
    fn find_next_returns_none_when_only_eligible_child_is_current() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        // One selectable child (A); a second child (B) that is NOT selectable.
        // With A as current, find_next must return None (no other eligible child
        // to move to — wrapping back to start yields the same id).
        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        let ida = with_ctx(&mut out, &mut timers, |ctx| {
            let mut a = Probe::new(Rect::new(0, 0, 5, 5), 'A', log.clone());
            a.st.options.selectable = true;
            let id = group.insert(Box::new(a));
            // B is non-selectable — ineligible for tab order.
            group.insert(Probe::boxed(Rect::new(6, 0, 11, 5), 'B', log.clone()));
            group.set_current(Some(id), SelectMode::Normal, ctx);
            id
        });
        assert_eq!(group.current(), Some(ida));
        assert_eq!(
            group.find_next(true),
            None,
            "no other eligible child -> find_next returns None"
        );
        assert_eq!(group.find_next(false), None, "backwards also returns None");
    }

    // -- 10. remove non-current leaves current unchanged ---------------------

    #[test]
    fn remove_non_current_child_leaves_current_unchanged() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        // Insert two selectable children A and B; make A the current one;
        // then remove B (the non-current one) and assert current is still A.
        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        let (ida, idb) = with_ctx(&mut out, &mut timers, |ctx| {
            let mut a = Probe::new(Rect::new(0, 0, 5, 5), 'A', log.clone());
            a.st.options.selectable = true;
            let ida = group.insert(Box::new(a));
            let mut b = Probe::new(Rect::new(6, 0, 11, 5), 'B', log.clone());
            b.st.options.selectable = true;
            let idb = group.insert(Box::new(b));
            group.set_current(Some(ida), SelectMode::Normal, ctx);
            (ida, idb)
        });
        assert_eq!(group.current(), Some(ida), "A is current before remove");

        // Remove the non-current child B.
        with_ctx(&mut out, &mut timers, |ctx| group.remove(idb, ctx));

        assert_eq!(
            group.current(),
            Some(ida),
            "current (A) is preserved after removing non-current child (B)"
        );
        assert!(group.index_of(idb).is_none(), "B is gone");
        assert!(group.index_of(ida).is_some(), "A is still present");
    }

    // -- 11. cursor_request descends into the current child ------------------

    #[test]
    fn cursor_request_descends_into_current_child_with_origin() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        // A focused child at origin (6, 4) that wants a visible cursor at its
        // view-local (2, 1). The group must return the origin-shifted (8, 5).
        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        let id = with_ctx(&mut out, &mut timers, |_ctx| {
            let mut p = Probe::new(Rect::new(6, 4, 11, 9), 'A', log.clone());
            p.st.options.selectable = true;
            p.st.state.focused = true;
            p.st.state.cursor_vis = true;
            p.st.cursor = Point::new(2, 1);
            group.insert(Box::new(p))
        });
        with_ctx(&mut out, &mut timers, |ctx| {
            group.set_current(Some(id), SelectMode::Normal, ctx)
        });

        assert_eq!(
            group.cursor_request(),
            Some(Point::new(8, 5)),
            "current child's view-local cursor is shifted by its origin"
        );

        // With the cursor hidden the group returns None.
        with_ctx(&mut out, &mut timers, |_ctx| {
            group.children[0].view.state_mut().state.cursor_vis = false;
        });
        assert_eq!(
            group.cursor_request(),
            None,
            "no current cursor when the child hides it"
        );

        // No current child -> None.
        let empty = Group::new(Rect::new(0, 0, 20, 10));
        assert_eq!(empty.cursor_request(), None);
    }

    // -- 12. Z-reorder: put_in_front_of (the primitive) ----------------------

    #[test]
    fn put_in_front_of_moves_child_in_z_order() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        // children = [A, B, C]; A == bottom (children[0]), C == top (last).
        // None of them selectable here, so reset_current's tail is a no-op and we
        // observe the raw reorder.
        let mut group = Group::new(Rect::new(0, 0, 30, 10));
        let (ida, idb, idc) = with_ctx(&mut out, &mut timers, |_ctx| {
            let ida = group.insert(Probe::boxed(Rect::new(0, 0, 5, 5), 'A', log.clone()));
            let idb = group.insert(Probe::boxed(Rect::new(6, 0, 11, 5), 'B', log.clone()));
            let idc = group.insert(Probe::boxed(Rect::new(12, 0, 17, 5), 'C', log.clone()));
            (ida, idb, idc)
        });
        assert_eq!(group.index_of(ida), Some(0));
        assert_eq!(group.index_of(idc), Some(2));

        // Move A (bottom) to the very top: A -> children.last().
        with_ctx(&mut out, &mut timers, |ctx| group.make_first(ida, ctx));
        assert_eq!(group.index_of(ida), Some(2), "A is now the top (last slot)");
        assert_eq!(group.index_of(idb), Some(0), "B drops to the bottom");
        assert_eq!(group.index_of(idc), Some(1));

        // Move B in front of C: B lands immediately above C (one slot up).
        // Order is now [B, C, A]; put B in front of C -> [C, B, A].
        with_ctx(&mut out, &mut timers, |ctx| {
            group.put_in_front_of(idb, Some(idc), ctx)
        });
        let order: Vec<_> = (0..group.len()).map(|i| group.children[i].id).collect();
        assert_eq!(order, vec![idc, idb, ida], "B sits just in front of C");
    }

    #[test]
    fn put_in_front_of_is_a_noop_when_already_in_place() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        // children = [A, B, C]; A == bottom, C == top.
        let mut group = Group::new(Rect::new(0, 0, 30, 10));
        let (ida, idb, idc) = with_ctx(&mut out, &mut timers, |_ctx| {
            let ida = group.insert(Probe::boxed(Rect::new(0, 0, 5, 5), 'A', log.clone()));
            let idb = group.insert(Probe::boxed(Rect::new(6, 0, 11, 5), 'B', log.clone()));
            let idc = group.insert(Probe::boxed(Rect::new(12, 0, 17, 5), 'C', log.clone()));
            (ida, idb, idc)
        });

        // make_first on the already-top child (C) is a no-op (C++ Target == this/
        // already-top guard).
        with_ctx(&mut out, &mut timers, |ctx| group.make_first(idc, ctx));
        let order: Vec<_> = (0..group.len()).map(|i| group.children[i].id).collect();
        assert_eq!(
            order,
            vec![ida, idb, idc],
            "already-top make_first changes nothing"
        );

        // put_in_front_of where the child is ALREADY immediately in front of the
        // target is a no-op (C++ Target == nextView()). B is immediately in front
        // of A (B at index 1 == A's index 0 + 1).
        with_ctx(&mut out, &mut timers, |ctx| {
            group.put_in_front_of(idb, Some(ida), ctx)
        });
        let order2: Vec<_> = (0..group.len()).map(|i| group.children[i].id).collect();
        assert_eq!(
            order2,
            vec![ida, idb, idc],
            "already-in-front put_in_front_of is a no-op"
        );

        // put_in_front_of(self) is a no-op.
        with_ctx(&mut out, &mut timers, |ctx| {
            group.put_in_front_of(idb, Some(idb), ctx)
        });
        let order3: Vec<_> = (0..group.len()).map(|i| group.children[i].id).collect();
        assert_eq!(order3, vec![ida, idb, idc]);
    }

    // -- 13. unknown/stale target id in put_in_front_of ----------------------

    #[test]
    fn put_in_front_of_unknown_id_is_noop() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        // children = [A, B, C].
        let mut group = Group::new(Rect::new(0, 0, 30, 10));
        let (ida, idb, idc) = with_ctx(&mut out, &mut timers, |_ctx| {
            let ida = group.insert(Probe::boxed(Rect::new(0, 0, 5, 5), 'A', log.clone()));
            let idb = group.insert(Probe::boxed(Rect::new(6, 0, 11, 5), 'B', log.clone()));
            let idc = group.insert(Probe::boxed(Rect::new(12, 0, 17, 5), 'C', log.clone()));
            (ida, idb, idc)
        });
        let order_before: Vec<_> = (0..group.len()).map(|i| group.children[i].id).collect();

        // Obtain a guaranteed-stale id by removing C, then passing its id as the
        // target to put_in_front_of — the group no longer contains it.
        with_ctx(&mut out, &mut timers, |ctx| group.remove(idc, ctx));
        with_ctx(&mut out, &mut timers, |ctx| {
            group.put_in_front_of(ida, Some(idc), ctx)
        });

        let order_after: Vec<_> = (0..group.len()).map(|i| group.children[i].id).collect();
        assert_eq!(
            order_after,
            vec![ida, idb],
            "stale target id: order of remaining children is unchanged"
        );
        // Also verify: passing the child's own id as the target is a no-op (the
        // t == id early-return guard), even without prior remove.
        let mut group2 = Group::new(Rect::new(0, 0, 30, 10));
        let (id2a, id2b) = with_ctx(&mut out, &mut timers, |_ctx| {
            let id2a = group2.insert(Probe::boxed(Rect::new(0, 0, 5, 5), 'A', log.clone()));
            let id2b = group2.insert(Probe::boxed(Rect::new(6, 0, 11, 5), 'B', log.clone()));
            (id2a, id2b)
        });
        with_ctx(&mut out, &mut timers, |ctx| {
            group2.put_in_front_of(id2a, Some(id2a), ctx)
        });
        let order2: Vec<_> = (0..group2.len()).map(|i| group2.children[i].id).collect();
        assert_eq!(order2, vec![id2a, id2b], "put_in_front_of(self) is a no-op");
        let _ = order_before; // captured before the remove; no assertion needed
    }

    // -- 14. raise-on-click (ofTopSelect select-path rewire) -----------------

    #[test]
    fn click_raises_top_select_child_to_top_and_makes_it_current() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        // Mirror a real desktop: a non-selectable background at the very bottom
        // (children[0] == C++ `last`), then two overlapping selectable+top_select
        // windows A (bottom of the two) and B (top). The non-selectable bottom is
        // what makes firstMatch (which checks `last` first, then `first()`→down)
        // return the *raised* (top) window — faithful to C++ resetCurrent on a
        // desktop. Without it, firstMatch returns the bottom-most SELECTABLE view.
        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        group.st.state.focused = true; // a focused group, so focus propagates
        let (ida, idb) = with_ctx(&mut out, &mut timers, |_ctx| {
            // background (non-selectable), full extent
            group.insert(Probe::boxed(Rect::new(0, 0, 20, 10), '.', log.clone()));
            // window A, overlapping window B
            let mut a = Probe::new(Rect::new(0, 0, 12, 8), 'A', log.clone());
            a.st.options.selectable = true;
            a.st.options.top_select = true;
            let ida = group.insert(Box::new(a));
            let mut b = Probe::new(Rect::new(6, 2, 18, 10), 'B', log.clone());
            b.st.options.selectable = true;
            b.st.options.top_select = true;
            let idb = group.insert(Box::new(b));
            (ida, idb)
        });
        // B is on top initially.
        assert!(group.index_of(idb).unwrap() > group.index_of(ida).unwrap());

        // Click a point inside A but NOT inside B (A is at x 0..12, B at x 6..18;
        // pick (2, 1): inside A, outside B). Even though B is topmost, the hit-test
        // falls to A.
        let mut ev = mouse_down_at(2, 1);
        with_ctx(&mut out, &mut timers, |ctx| {
            group.handle_event(&mut ev, ctx)
        });

        // A is now the topmost child (raised via make_first).
        assert_eq!(
            group.index_of(ida),
            Some(group.len() - 1),
            "clicked top_select child raised to the top of Z-order"
        );
        // A is current (firstMatch returns the top window since the bottom is the
        // non-selectable background).
        assert_eq!(group.current(), Some(ida), "raised window becomes current");
    }

    #[test]
    fn non_top_select_click_does_not_reorder() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        // Two selectable children WITHOUT top_select. Clicking the bottom one must
        // make it current but must NOT change Z-order (regression guard so the
        // rewire does not over-fire).
        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        group.st.state.focused = true;
        let (ida, idb) = with_ctx(&mut out, &mut timers, |_ctx| {
            let mut a = Probe::new(Rect::new(0, 0, 5, 5), 'A', log.clone());
            a.st.options.selectable = true; // no top_select
            let ida = group.insert(Box::new(a));
            let mut b = Probe::new(Rect::new(6, 0, 11, 5), 'B', log.clone());
            b.st.options.selectable = true;
            let idb = group.insert(Box::new(b));
            (ida, idb)
        });
        let order_before: Vec<_> = (0..group.len()).map(|i| group.children[i].id).collect();

        // Click A (the bottom child).
        let mut ev = mouse_down_at(1, 1);
        with_ctx(&mut out, &mut timers, |ctx| {
            group.handle_event(&mut ev, ctx)
        });

        let order_after: Vec<_> = (0..group.len()).map(|i| group.children[i].id).collect();
        assert_eq!(
            order_after, order_before,
            "non-top_select select must not reorder"
        );
        assert_eq!(order_after, vec![ida, idb]);
        assert_eq!(group.current(), Some(ida), "A is current via set_current");
    }

    // -- 14b. focus_child self-heal (ctx-less insert broke the invariant) ----

    /// `focus_child` on a `top_select` child that is ALREADY topmost but NOT
    /// current must still make it current+selected. This is the post-insert
    /// state tvision-rs's ctx-less `Group::insert` produces (C++ never sees it:
    /// insert-time show()->resetCurrent keeps the topmost ofTopSelect window
    /// current). Without the self-heal, make_first hits put_in_front_of's
    /// already-in-place no-op, its resetCurrent tail never runs, and the click
    /// that called focus_child is a complete no-op.
    #[test]
    fn focus_child_self_heals_topmost_non_current_top_select_child() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        // Two selectable + top_select "windows"; B inserted last == topmost.
        // The ctx-less insert leaves current == None (the broken invariant).
        // B gets its OWN log so the key-routing assertion below proves the key
        // reached B specifically (not just "some probe").
        let log_b = Rc::new(RefCell::new(Vec::new()));
        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        group.st.state.focused = true; // focused group: currency also focuses
        let (ida, idb) = with_ctx(&mut out, &mut timers, |_ctx| {
            let mut a = Probe::new(Rect::new(0, 0, 12, 8), 'A', log.clone());
            a.st.options.selectable = true;
            a.st.options.top_select = true;
            let ida = group.insert(Box::new(a));
            let mut b = Probe::new(Rect::new(6, 2, 18, 10), 'B', log_b.clone());
            b.st.options.selectable = true;
            b.st.options.top_select = true;
            let idb = group.insert(Box::new(b));
            (ida, idb)
        });
        assert_eq!(group.current(), None, "ctx-less insert never sets current");

        // focus_child on the TOPMOST child: make_first no-ops (already top), so
        // only the self-heal can establish currency.
        let ok = with_ctx(&mut out, &mut timers, |ctx| group.focus_child(idb, ctx));
        assert!(ok, "focus accepted");
        assert_eq!(group.current(), Some(idb), "topmost child became current");
        let st = group.children[group.index_of(idb).unwrap()].view.state();
        assert!(st.state.selected, "self-heal selected the child");
        assert!(
            st.state.focused,
            "focused group cascades focus to the child"
        );
        // Z-order untouched (A bottom, B top — make_first's no-op stays a no-op).
        let order: Vec<_> = (0..group.len()).map(|i| group.children[i].id).collect();
        assert_eq!(order, vec![ida, idb], "no reorder for an already-top child");

        // End-to-end dispatch after the self-heal: a focused-class event (key)
        // routes through the three-phase router to the healed `current` — and
        // is consumed there. Without the self-heal current would still be None
        // and the key would fall through unrouted (the click-then-type no-op).
        let mut ev = key(Key::Char('z'));
        with_ctx(&mut out, &mut timers, |ctx| {
            group.handle_event(&mut ev, ctx)
        });
        assert!(
            log_b
                .borrow()
                .iter()
                .any(|e| matches!(e, Event::KeyDown(k) if k.key == Key::Char('z'))),
            "a key typed after the self-heal reaches the healed current (B)"
        );
        assert!(log.borrow().is_empty(), "A (not current) saw nothing");
        assert!(ev.is_nothing(), "the key was consumed by the focused child");
    }

    /// Companion: `focus_child` on an already-current topmost child stays a
    /// no-op — the self-heal's `current != id` guard skips `set_current`, so no
    /// duplicate focus broadcasts are queued.
    #[test]
    fn focus_child_on_current_topmost_child_is_a_noop() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        group.st.state.focused = true;
        let idb = with_ctx(&mut out, &mut timers, |_ctx| {
            let mut a = Probe::new(Rect::new(0, 0, 12, 8), 'A', log.clone());
            a.st.options.selectable = true;
            a.st.options.top_select = true;
            group.insert(Box::new(a));
            let mut b = Probe::new(Rect::new(6, 2, 18, 10), 'B', log.clone());
            b.st.options.selectable = true;
            b.st.options.top_select = true;
            group.insert(Box::new(b))
        });
        // Establish the C++ invariant first (topmost is current), then drain the
        // focus broadcasts that establishing it queued.
        with_ctx(&mut out, &mut timers, |ctx| group.focus_child(idb, ctx));
        assert_eq!(group.current(), Some(idb));
        out.clear();

        // Re-focusing the already-current topmost child must queue nothing.
        let ok = with_ctx(&mut out, &mut timers, |ctx| group.focus_child(idb, ctx));
        assert!(ok);
        assert_eq!(group.current(), Some(idb), "still current");
        assert!(
            out.is_empty(),
            "no duplicate focus broadcasts for an already-current child: {out:?}"
        );
    }

    // -- 15. owner_size set during routing + unconditional restore ------

    /// A probe that records `ctx.owner_size()` at the moment it handles an event,
    /// so a test can observe the value a child sees during group routing.
    struct OwnerSizeProbe {
        st: ViewState,
        seen: Rc<RefCell<Vec<Point>>>,
    }
    impl View for OwnerSizeProbe {
        fn state(&self) -> &ViewState {
            &self.st
        }
        fn state_mut(&mut self) -> &mut ViewState {
            &mut self.st
        }
        fn draw(&mut self, _ctx: &mut DrawCtx) {}
        fn handle_event(&mut self, _ev: &mut Event, ctx: &mut Context) {
            self.seen.borrow_mut().push(ctx.owner_size());
        }
    }

    #[test]
    fn handle_event_sets_owner_size_to_group_and_restores_on_exit() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let seen = Rc::new(RefCell::new(Vec::new()));

        // Nesting: an OUTER group (size 80x25) containing an INNER group (size
        // 30x12) which contains the probe (the current child). When the broadcast
        // is routed outer→inner→probe, the probe must see the INNER group's size.
        let mut inner = Group::new(Rect::new(5, 3, 35, 15)); // size 30x12
        let probe_id = inner.insert(Box::new(OwnerSizeProbe {
            st: ViewState::new(Rect::new(0, 0, 10, 5)),
            seen: seen.clone(),
        }));
        // Make the probe current so the focused/broadcast path reaches it.
        with_ctx(&mut out, &mut timers, |ctx| {
            inner.set_current(Some(probe_id), SelectMode::Normal, ctx)
        });
        assert_eq!(inner.state().size, Point::new(30, 12));

        let mut outer = Group::new(Rect::new(0, 0, 80, 25)); // size 80x25
        outer.insert(Box::new(inner));

        // Broadcast reaches every child (probe sees owner_size). Set a sentinel
        // owner_size on the ctx first to prove the OUTER group restores it.
        let mut ev = Event::Broadcast {
            command: Command::SCROLL_BAR_CHANGED,
            source: None,
        };
        with_ctx(&mut out, &mut timers, |ctx| {
            ctx.set_owner_size(Point::new(111, 222)); // sentinel (a "parent" value)
            outer.handle_event(&mut ev, ctx);
            // Unconditional restore: after routing, owner_size is back to the
            // sentinel the parent had set, NOT the outer group's 80x25.
            assert_eq!(
                ctx.owner_size(),
                Point::new(111, 222),
                "owner_size restored to the parent's value after handle_event"
            );
        });

        // During routing the probe saw the INNER group's size (its actual owner).
        assert_eq!(*seen.borrow(), vec![Point::new(30, 12)]);
    }

    #[test]
    fn handle_event_restores_owner_size_even_on_early_return_path() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();

        // A positional event with no children to hit still runs the positional arm
        // (which can `return` early inside route_event). The bracket must still
        // restore owner_size. (Regression guard for the extracted-fn restore.)
        let mut group = Group::new(Rect::new(0, 0, 40, 20));
        let mut ev = mouse_down_at(5, 5); // hits nobody (empty group)
        with_ctx(&mut out, &mut timers, |ctx| {
            ctx.set_owner_size(Point::new(7, 9));
            group.handle_event(&mut ev, ctx);
            assert_eq!(
                ctx.owner_size(),
                Point::new(7, 9),
                "owner_size restored even when routing returns early"
            );
        });
    }

    // -- 16. find_mut / remove_descendant on a plain Group (direct-child path) --

    #[test]
    fn find_mut_resolves_direct_child_and_misses_unknown() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        let (ida, idb) = with_ctx(&mut out, &mut timers, |_ctx| {
            let ida = group.insert(Probe::boxed(Rect::new(0, 0, 5, 5), 'A', log.clone()));
            let idb = group.insert(Probe::boxed(Rect::new(6, 0, 11, 5), 'B', log.clone()));
            (ida, idb)
        });

        // Resolve A and mutate a field through the returned reference; observe it.
        {
            let v = group.find_mut(ida).expect("A resolves");
            v.state_mut().set_cursor(3, 4);
        }
        assert_eq!(
            group.find_mut(idb).expect("B resolves").state().origin,
            Point::new(6, 0)
        );
        // The mutation through find_mut(ida) is visible on the child.
        assert_eq!(
            group.find_mut(ida).expect("A resolves").state().cursor,
            Point::new(3, 4),
            "mutation through find_mut is observed on the child"
        );

        // A never-inserted id resolves to None.
        let bogus = ViewId::next();
        assert!(group.find_mut(bogus).is_none(), "unknown id -> None");
    }

    /// `View::reset_current` (the trait override) establishes a group's internal
    /// currency through the **trait** dispatch path — the seam `exec_view` uses on a
    /// freshly-inserted modal (it holds the modal only as `&mut dyn View`). This is
    /// the discriminating guard: a ctx-less `Group::insert`
    /// leaves `current == None` (insert never focuses); calling `reset_current`
    /// via `&mut dyn View` must flip it to the first visible+selectable child. It
    /// also proves the override dispatches to the inherent `Group::reset_current`
    /// (UFCS) rather than recursing into itself — infinite recursion would
    /// stack-overflow this test instead of passing.
    #[test]
    fn reset_current_via_trait_sets_current_to_first_selectable() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        let ida = with_ctx(&mut out, &mut timers, |_ctx| {
            let mut a = Probe::new(Rect::new(0, 0, 5, 5), 'A', log.clone());
            a.st.options.selectable = true;
            // insert (ctx-less) does NOT focus — current stays None. It
            // instead marks `currency_dirty` (the pending insert-time
            // show()->resetCurrent), settled later by settle_currency — insert
            // itself still never touches `current`.
            group.insert(Box::new(a))
        });
        assert_eq!(
            group.current(),
            None,
            "ctx-less insert leaves current == None (insert sets a flag, never current)"
        );

        // Establish internal currency via the TRAIT method on &mut dyn View — the
        // exact path exec_view takes (find_mut -> reset_current).
        with_ctx(&mut out, &mut timers, |ctx| {
            let v: &mut dyn View = &mut group;
            v.reset_current(ctx);
        });

        assert_eq!(
            group.current(),
            Some(ida),
            "reset_current via the trait sets current to the first selectable child"
        );
    }

    /// Currency keystone, EARLY-RETURN leg: `set_current` with `current == p`
    /// early-returns, but MUST still clear `currency_dirty` — an explicit
    /// currency op supersedes the pending insert-time reset even when it is a
    /// no-op for `current` itself. A regression (clearing below the early
    /// return) would leave the flag set and the next `settle_currency` would
    /// snap currency to firstMatch, clobbering the explicit choice.
    #[test]
    fn set_current_early_return_still_clears_pending_insert_reset() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        let mut group = Group::new(Rect::new(0, 0, 40, 10));
        // Non-selectable backstop at the bottom so firstMatch never lands on
        // children[0] (it must discriminate against the explicit choice below).
        let mut backstop = Probe::new(Rect::new(0, 0, 5, 5), 'N', log.clone());
        backstop.st.options.selectable = false;
        group.insert(Box::new(backstop));
        let mut b = Probe::new(Rect::new(6, 0, 11, 5), 'B', log.clone());
        b.st.options.selectable = true;
        let idb = group.insert(Box::new(b));

        // Explicitly choose B (clears the insert-time flags so far).
        with_ctx(&mut out, &mut timers, |ctx| {
            group.set_current(Some(idb), SelectMode::Normal, ctx)
        });

        // A new selectable topmost child marks the group dirty; firstMatch is
        // now C (topmost selectable), NOT the current B.
        let mut c = Probe::new(Rect::new(12, 0, 17, 5), 'C', log.clone());
        c.st.options.selectable = true;
        let idc = group.insert(Box::new(c));

        // Re-assert the SAME current — the early-return leg. The flag must
        // clear even though `current` does not change.
        with_ctx(&mut out, &mut timers, |ctx| {
            group.set_current(Some(idb), SelectMode::Normal, ctx)
        });

        // Settle: with the flag cleared this is a no-op; a regression would
        // reset_current to firstMatch == C.
        with_ctx(&mut out, &mut timers, |ctx| {
            let v: &mut dyn View = &mut group;
            v.settle_currency(ctx);
        });
        assert_eq!(
            group.current(),
            Some(idb),
            "early-return set_current cleared the pending flag — settle stayed a no-op"
        );
        let _ = idc;
    }

    #[test]
    fn remove_descendant_removes_direct_child_and_misses_unknown() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        // Two selectable children; A current. Remove the current (A) and the
        // non-current (B) via remove_descendant; verify removal + reset_current.
        let mut group = Group::new(Rect::new(0, 0, 20, 10));
        let (ida, idb) = with_ctx(&mut out, &mut timers, |ctx| {
            let mut a = Probe::new(Rect::new(0, 0, 5, 5), 'A', log.clone());
            a.st.options.selectable = true;
            let ida = group.insert(Box::new(a));
            let mut b = Probe::new(Rect::new(6, 0, 11, 5), 'B', log.clone());
            b.st.options.selectable = true;
            let idb = group.insert(Box::new(b));
            group.set_current(Some(ida), SelectMode::Normal, ctx);
            (ida, idb)
        });
        assert_eq!(group.current(), Some(ida));

        // Removing a bogus id changes nothing and returns false.
        let bogus = ViewId::next();
        let removed_bogus = with_ctx(&mut out, &mut timers, |ctx| {
            group.remove_descendant(bogus, ctx)
        });
        assert!(!removed_bogus, "unknown id -> false");
        assert_eq!(group.len(), 2, "no child removed");

        // Remove the current child A -> true; reset_current picks B.
        let removed = with_ctx(&mut out, &mut timers, |ctx| {
            group.remove_descendant(ida, ctx)
        });
        assert!(removed, "direct current child removed -> true");
        assert!(group.find_mut(ida).is_none(), "A gone");
        assert_eq!(
            group.current(),
            Some(idb),
            "reset_current selected the remaining child"
        );
    }

    /// Removing a view runs the visibility-change currency tail for ANY
    /// visible+selectable child. So removing a NON-current visible+selectable
    /// child must still snap `current` to the first matching child — not leave it
    /// where it was.
    #[test]
    fn remove_of_noncurrent_visible_selectable_child_resets_current() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let log = Rc::new(RefCell::new(Vec::new()));

        // A (bottom), B, C (top) — all visible+selectable; current = C.
        let mut group = Group::new(Rect::new(0, 0, 40, 10));
        let mk = |ch: char, x: i32, log: &Rc<RefCell<Vec<Event>>>| {
            let mut p = Probe::new(Rect::new(x, 0, x + 5, 5), ch, log.clone());
            p.st.options.selectable = true;
            Box::new(p)
        };
        let ida = group.insert(mk('A', 0, &log));
        let idb = group.insert(mk('B', 6, &log));
        let idc = group.insert(mk('C', 12, &log));
        with_ctx(&mut out, &mut timers, |ctx| {
            group.set_current(Some(idc), SelectMode::Normal, ctx)
        });
        assert_eq!(group.current(), Some(idc), "C explicitly current");

        // Remove B — NOT current, but visible+selectable: the remove→hide tail
        // resets currency to firstMatch (A == children[0], checked first,
        // faithful to C++ starting at `last`). Pre-stage-4, current stayed C.
        with_ctx(&mut out, &mut timers, |ctx| group.remove(idb, ctx));
        assert!(group.find_mut(idb).is_none(), "B gone");
        assert_eq!(
            group.current(),
            Some(ida),
            "removal of a non-current visible+selectable child re-ran resetCurrent \
             (current snapped to firstMatch)"
        );

        // Counter-case: removing a HIDDEN child runs no tail (the C++ hide()
        // gate `if (state & sfVisible)` means an already-hidden child's removal
        // never reaches the setState tail).
        let mut hidden = Probe::new(Rect::new(18, 0, 23, 5), 'H', log.clone());
        hidden.st.options.selectable = true;
        hidden.st.state.visible = false;
        let idh = group.insert(Box::new(hidden));
        with_ctx(&mut out, &mut timers, |ctx| {
            group.set_current(Some(idc), SelectMode::Normal, ctx)
        });
        with_ctx(&mut out, &mut timers, |ctx| group.remove(idh, ctx));
        assert_eq!(
            group.current(),
            Some(idc),
            "removing a hidden child runs no reset (current unchanged)"
        );
    }

    // -- focus_by_number ---------------------------------------------

    /// A numbered, selectable probe (the `cmSelectWindowNum` target shape).
    struct NumberedProbe {
        st: ViewState,
        number: i16,
    }
    impl NumberedProbe {
        fn boxed(number: i16, selectable: bool) -> Box<dyn View> {
            let mut st = ViewState::new(Rect::new(0, 0, 5, 3));
            st.options.selectable = selectable;
            st.options.top_select = true; // like a window: focus_child raises it
            Box::new(NumberedProbe { st, number })
        }
    }
    impl View for NumberedProbe {
        fn state(&self) -> &ViewState {
            &self.st
        }
        fn state_mut(&mut self) -> &mut ViewState {
            &mut self.st
        }
        fn draw(&mut self, _ctx: &mut DrawCtx) {}
        fn number(&self) -> Option<i16> {
            Some(self.number)
        }
    }

    #[test]
    fn focus_by_number_matches_selectable_skips_nonselectable_misses_absent() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut group = Group::new(Rect::new(0, 0, 40, 20));

        // Insert a non-selectable backstop at the BOTTOM (children[0]) — mirrors a
        // real desktop's background, so `reset_current`/`firstMatch` (which checks
        // children[0] first, faithful to C++ starting at `last`) skips it and lands
        // on the raised window. Then: selectable #1, selectable #2, NON-selectable #3.
        let (id1, id2, _id3) = with_ctx(&mut out, &mut timers, |_ctx| {
            group.insert(NumberedProbe::boxed(0, false)); // background backstop
            let id1 = group.insert(NumberedProbe::boxed(1, true));
            let id2 = group.insert(NumberedProbe::boxed(2, true));
            let id3 = group.insert(NumberedProbe::boxed(3, false));
            (id1, id2, id3)
        });
        // Establish a current so focus_child's outgoing-validate path is exercised.
        with_ctx(&mut out, &mut timers, |ctx| {
            group.set_current(Some(id1), SelectMode::Normal, ctx)
        });

        // Match #2 (selectable) -> true; raised + becomes current (the backstop
        // means firstMatch skips children[0] and selects the raised window).
        let matched = with_ctx(&mut out, &mut timers, |ctx| group.focus_by_number(2, ctx));
        assert!(matched, "selectable #2 matched");
        assert_eq!(group.current(), Some(id2), "#2 is now current");

        // Absent number #9 -> false, current unchanged.
        let absent = with_ctx(&mut out, &mut timers, |ctx| group.focus_by_number(9, ctx));
        assert!(!absent, "no window 9 -> false");
        assert_eq!(group.current(), Some(id2), "current unchanged on no match");

        // Non-selectable #3 is skipped by the explicit ofSelectable filter -> false.
        let non_sel = with_ctx(&mut out, &mut timers, |ctx| group.focus_by_number(3, ctx));
        assert!(!non_sel, "non-selectable #3 is filtered out -> false");
        assert_eq!(group.current(), Some(id2), "current unchanged");

        let _ = id1;
    }

    // -- gather_data / scatter_data ------------------------------------------

    #[test]
    fn gather_data_returns_values_in_forward_child_order() {
        use crate::data::FieldValue;
        use crate::widgets::InputLine;

        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut group = Group::new(Rect::new(0, 0, 40, 10));

        group.insert(Box::new(InputLine::new(
            Rect::new(0, 0, 10, 1),
            20,
            None,
            crate::widgets::LimitMode::MaxBytes,
        )));
        group.insert(Box::new(InputLine::new(
            Rect::new(0, 2, 10, 3),
            20,
            None,
            crate::widgets::LimitMode::MaxBytes,
        )));

        // Set text via scatter_data: children[0] gets "alpha", children[1] "beta".
        let initial = vec![
            Some(FieldValue::Text("alpha".to_string())),
            Some(FieldValue::Text("beta".to_string())),
        ];
        with_ctx(&mut out, &mut timers, |ctx| {
            group.scatter_data(&initial, ctx);
        });

        let vals = group.gather_data();
        assert_eq!(vals.len(), 2);
        assert_eq!(vals[0], Some(FieldValue::Text("alpha".to_string())));
        assert_eq!(vals[1], Some(FieldValue::Text("beta".to_string())));
    }

    #[test]
    fn scatter_data_round_trips_with_gather() {
        use crate::data::FieldValue;
        use crate::widgets::InputLine;

        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut group = Group::new(Rect::new(0, 0, 40, 10));

        group.insert(Box::new(InputLine::new(
            Rect::new(0, 0, 10, 1),
            20,
            None,
            crate::widgets::LimitMode::MaxBytes,
        )));
        group.insert(Box::new(InputLine::new(
            Rect::new(0, 2, 10, 3),
            20,
            None,
            crate::widgets::LimitMode::MaxBytes,
        )));

        // Scatter initial values.
        let initial = vec![
            Some(FieldValue::Text("foo".to_string())),
            Some(FieldValue::Text("bar".to_string())),
        ];
        with_ctx(&mut out, &mut timers, |ctx| {
            group.scatter_data(&initial, ctx);
        });

        // Gather and verify round-trip.
        let gathered = group.gather_data();
        assert_eq!(gathered[0], Some(FieldValue::Text("foo".to_string())));
        assert_eq!(gathered[1], Some(FieldValue::Text("bar".to_string())));
    }

    #[test]
    fn scatter_data_skips_none_entries() {
        use crate::data::FieldValue;
        use crate::widgets::InputLine;

        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut group = Group::new(Rect::new(0, 0, 40, 10));

        group.insert(Box::new(InputLine::new(
            Rect::new(0, 0, 10, 1),
            20,
            None,
            crate::widgets::LimitMode::MaxBytes,
        )));
        group.insert(Box::new(InputLine::new(
            Rect::new(0, 2, 10, 3),
            20,
            None,
            crate::widgets::LimitMode::MaxBytes,
        )));

        // Set both, then scatter with None for the first child.
        let initial = vec![
            Some(FieldValue::Text("first".to_string())),
            Some(FieldValue::Text("second".to_string())),
        ];
        with_ctx(&mut out, &mut timers, |ctx| {
            group.scatter_data(&initial, ctx);
        });

        // Scatter again with None for child[0] — it should be unchanged.
        let partial = vec![None, Some(FieldValue::Text("updated".to_string()))];
        with_ctx(&mut out, &mut timers, |ctx| {
            group.scatter_data(&partial, ctx);
        });

        let gathered = group.gather_data();
        assert_eq!(gathered[0], Some(FieldValue::Text("first".to_string())));
        assert_eq!(gathered[1], Some(FieldValue::Text("updated".to_string())));
    }

    // -- gather_list / scatter_list ------------------------------------------

    #[test]
    fn gather_list_packs_data_bearing_children_in_order() {
        use crate::data::FieldValue;
        use crate::widgets::InputLine;

        let mut group = Group::new(Rect::new(0, 0, 40, 10));
        group.insert(Box::new(InputLine::new(
            Rect::new(0, 0, 10, 1),
            20,
            None,
            crate::widgets::LimitMode::MaxBytes,
        )));
        // A non-data child (a bare Group) contributes nothing to the record.
        group.insert(Box::new(Group::new(Rect::new(0, 5, 5, 6))));
        group.insert(Box::new(InputLine::new(
            Rect::new(0, 2, 10, 3),
            20,
            None,
            crate::widgets::LimitMode::MaxBytes,
        )));

        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        with_ctx(&mut out, &mut timers, |ctx| {
            group.scatter_list(
                &FieldValue::List(vec![
                    FieldValue::Text("alpha".into()),
                    FieldValue::Text("beta".into()),
                ]),
                ctx,
            );
        });

        // Two data-bearing children, in order; the bare Group is skipped.
        assert_eq!(
            group.gather_list(),
            FieldValue::List(vec![
                FieldValue::Text("alpha".into()),
                FieldValue::Text("beta".into()),
            ]),
        );
    }

    #[test]
    fn scatter_list_ignores_non_list() {
        use crate::data::FieldValue;
        use crate::widgets::InputLine;

        let mut group = Group::new(Rect::new(0, 0, 40, 10));
        group.insert(Box::new(InputLine::new(
            Rect::new(0, 0, 10, 1),
            20,
            None,
            crate::widgets::LimitMode::MaxBytes,
        )));
        let before = group.gather_list();

        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        with_ctx(&mut out, &mut timers, |ctx| {
            group.scatter_list(&FieldValue::Int(7), ctx); // not a List → no-op
        });
        assert_eq!(
            group.gather_list(),
            before,
            "a non-List record changes nothing"
        );
    }

    #[test]
    fn get_help_ctx_bubbles_to_current_child() {
        use crate::help::HelpCtx;
        use crate::view::SelectMode;
        const LEAF: HelpCtx = HelpCtx::custom("test.leaf");
        // Minimal selectable leaf carrying a help context.
        struct Leaf {
            st: ViewState,
        }
        impl View for Leaf {
            fn state(&self) -> &ViewState {
                &self.st
            }
            fn state_mut(&mut self) -> &mut ViewState {
                &mut self.st
            }
            fn draw(&mut self, _ctx: &mut DrawCtx) {}
        }

        let mut g = Group::new(Rect::new(0, 0, 20, 10));
        let mut st = ViewState::new(Rect::new(1, 1, 10, 3));
        st.options.selectable = true;
        st.help_ctx = LEAF;
        let leaf = Box::new(Leaf { st });
        let id = g.insert(leaf);

        // No current child yet -> group's own context (NO_CONTEXT by default).
        assert_eq!(g.get_help_ctx(), HelpCtx::NO_CONTEXT);

        // Make the leaf current -> its context bubbles up.
        let mut out = std::collections::VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<crate::view::Deferred> = Vec::new();
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            g.set_current(Some(id), SelectMode::Normal, &mut ctx);
        }
        assert_eq!(
            g.get_help_ctx(),
            LEAF,
            "TGroup::getHelpCtx returns current child's context"
        );
    }
}
