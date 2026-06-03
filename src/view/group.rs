//! `TGroup` — the view container + event router (row 26, FOUNDATION).
//!
//! `TGroup` (`tgroup.cpp` / `grp.cpp`) is the node type of TV's view tree: it
//! owns a set of child views, draws them, routes events to them, and tracks
//! which one is current/focused. This port carries the deviations the tree
//! forces:
//!
//! * **Ownership is a `Vec`, links are gone (D3).** C++ threads children on a
//!   circular `next`/`prev` ring with `last`, and every child keeps an `owner`
//!   back-pointer. Here a [`Group`] owns `children: Vec<Child>`; children hold no
//!   up-pointer. Each child's [`ViewId`] is a process-global identity minted at
//!   [`Group::insert`] and stamped into the child's own `ViewState.id` (self-id,
//!   D3). Cross-links (`current`) are a [`ViewId`], resolved by an internal index
//!   lookup ([`Group::index_of`]).
//!
//!   The ring maps to the `Vec` in **back-to-front paint order**:
//!   `children[0]` == C++ `last` == bottom (drawn first); `children.last()` ==
//!   C++ `first()` == top/frontmost (drawn last). `insert` pushes (new child on
//!   top). `forEach`/`firstThat` (C++ visits `first()`→`last`) is therefore
//!   `children.iter().rev()`; tab order `next` walks decreasing index with wrap.
//!
//! * **Mouse position is view-local at each level (deviation).** C++ keeps the
//!   mouse in absolute screen coordinates and each view calls `makeLocal`. Under
//!   the downward model there is no owner to walk up to, so on each positional
//!   delivery the group subtracts the child's `origin`, handing the child a
//!   child-local position — the downward realization of `makeLocal`/
//!   `mouseInView`/`containsMouse`.
//!
//! * **No explicit `eventError`/bubble.** C++ `handleEvent` leaves an unhandled
//!   event in `event` and the program's `execute` loop calls `eventError`; a view
//!   that consumes calls `clearEvent`. Here "consumed" is the event being set to
//!   [`Event::Nothing`]; an unhandled event is simply left **not cleared**, and
//!   as the recursive `handle_event` stack unwinds the parent/loop sees it still
//!   live. There is no owner pointer to bubble to.
//!
//! * **Dropped under D8:** `buffer`/`getBuffer`/`freeBuffer`/`lock`/`unlock`/
//!   `clip`/`ofBuffered`/`sfExposed` and the occlusion-driven draw — replaced by
//!   whole-tree redraw + diff. `draw` paints back-to-front (painter's algorithm),
//!   **deliberately reversed** from C++ `drawSubViews` (which paints top-first
//!   and relies on occlusion). Shadow casting has no infra yet (`// TODO(row 33)`).
//!
//! * **Z-reorder (row 33a):** `ofTopSelect`/`makeFirst`/`putInFrontOf` are now
//!   realized in the owner ([`Group::put_in_front_of`]/[`Group::make_first`]); the
//!   select path ([`Group::focus_child`]) raises an `ofTopSelect` child to the top
//!   instead of just making it current, faithful to `TView::select`.
//! * **Deferred:** `execute`/`execView`/the blocking modal loop/`endModal` →
//!   row 31 (`TProgram`)/34 (the loop owns the capture stack, so a group cannot
//!   run a modal itself); `getData`/`setData`/`dataSize` → D10/row 39;
//!   `resetCursor` (hardware cursor) → row 31.

use crate::command::Command;
use crate::event::Event;
use crate::view::context::{Context, DrawCtx};
use crate::view::geometry::{Point, Rect};
use crate::view::id::ViewId;
use crate::view::view::{StateFlag, View, ViewState};

/// Which side effects `set_current` applies when changing the current view —
/// ports the `selectMode` enum (`views.h`: `normalSelect`/`enterSelect`/
/// `leaveSelect`). `Enter`/`Leave` are used by the deferred modal `execView`
/// path (row 31); `set_current` honours them faithfully so that path is a drop-in
/// when it lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectMode {
    /// `normalSelect` — deselect the old current, select the new one.
    Normal,
    /// `enterSelect` — entering a modal: do **not** deselect the old current.
    Enter,
    /// `leaveSelect` — leaving a modal: do **not** select the new current.
    Leave,
}

/// One owned child view plus its global identity (mirrors the child's own
/// `ViewState.id`).
struct Child {
    id: ViewId,
    view: Box<dyn View>,
}

/// `TGroup` — a view that owns and routes to a tree of child views (D3/D4).
///
/// See the [module docs](self) for the ring↔`Vec` Z-order mapping and the
/// deviations. Build with [`Group::new`], add children with [`Group::insert`],
/// and drive it as any other [`View`] (`draw` / `handle_event` / …).
pub struct Group {
    st: ViewState,
    /// Children in back-to-front paint order (`children[0]` == C++ `last`/bottom,
    /// `children.last()` == C++ `first()`/top).
    children: Vec<Child>,
    /// The current (selected) child — C++ `current`, as a [`ViewId`] (D3).
    current: Option<ViewId>,
}

impl Group {
    /// Construct a group covering `bounds`. Ports `TGroup::TGroup`:
    /// `options |= ofSelectable` and `eventMask = 0xFFFF`.
    ///
    /// Our [`crate::event::EventMask`] only has the two opt-ins (mouse-move /
    /// mouse-auto); a group must *receive* those to be able to route them to
    /// children, so it opts into both — the surviving slice of `0xFFFF`. The
    /// dropped ctor bits are `ofBuffered`/`clip` (D8).
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

    /// Resolve a [`ViewId`] to its index in `children` — the internal lookup that
    /// replaces the C++ pointer cross-links (D3). `None` for a stale/foreign id.
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

    /// Mutably borrow child `id`'s view (for an owner→child push that needs the
    /// concrete type via [`View::as_any_mut`], e.g. `TWindow::zoom` reaching its
    /// `TFrame`). `None` for a stale/foreign id.
    pub fn child_mut(&mut self, id: ViewId) -> Option<&mut dyn View> {
        let i = self.index_of(id)?;
        Some(self.children[i].view.as_mut())
    }

    // -- insert / remove ----------------------------------------------------

    /// Insert `view` on **top** of the group (becomes the frontmost child),
    /// mint a process-global [`ViewId`], stamp it into the view's own
    /// `ViewState.id` (self-id, D3), and return it. Ports `TGroup::insert` →
    /// `insertBefore(p, first())`.
    ///
    /// Applies `ofCenterX`/`ofCenterY` centering. Under D8 the `insertBefore`
    /// hide/show dance and the `sfActive`-restore are a **no-op** (no occlusion
    /// tracking, so show/hide is absent; the child's own saved state is preserved
    /// as-is). Centering is therefore the only observable effect here. It does
    /// **not** auto-set `current` — `insert` alone never focuses; callers use
    /// [`set_current`](Self::set_current)/[`reset_current`](Self::reset_current).
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
        self.children.push(Child { id, view });
        id
    }

    /// Remove the child named by `id` (no-op if it is not a child). Ports
    /// `TGroup::remove` → `removeView`: if the removed child was `current`, the
    /// group resets `current` to another selectable child afterward.
    pub fn remove(&mut self, id: ViewId, ctx: &mut Context) {
        let Some(i) = self.index_of(id) else {
            return;
        };
        let was_current = self.current == Some(id);
        self.children.remove(i);
        if was_current {
            self.current = None;
            self.reset_current(ctx);
        }
    }

    // -- focus machinery (faithful ports of setCurrent / focus / findNext) ---

    /// Change the current view to `p` (or `None`), applying the focus/select
    /// side effects per `mode`. Faithful port of `TGroup::setCurrent` +
    /// `focusView`/`selectView` (`tgroup.cpp`), with the D8 `lock`/`unlock`
    /// redraw bracket dropped.
    pub fn set_current(&mut self, p: Option<ViewId>, mode: SelectMode, ctx: &mut Context) {
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

    /// Make the child `id` the current/focused one. The downward realization of
    /// `TView::focus()` → `TView::select()` for a child in this (assumed-focused)
    /// group.
    ///
    /// Faithful to the C++ ordering: `focus()` validates the **outgoing** current
    /// (the `ofValidate` / `cmReleasedFocus` gate) and, if that passes, calls
    /// `select()`. `select()` is:
    /// ```cpp
    /// if( options & ofTopSelect ) makeFirst();
    /// else owner->setCurrent( this, normalSelect );
    /// ```
    /// so a **selectable + `ofTopSelect`** child is **raised to the top**
    /// ([`make_first`](Self::make_first), which reorders + `resetCurrent`s); any
    /// other selectable child is just made current via
    /// [`set_current`](Self::set_current). Returns `false` if the validate gate
    /// refused the switch.
    ///
    /// **`ofSelectable` gate:** the real C++ `TView::select()` has an outer guard
    /// `if( (options & ofSelectable) != 0 && owner != 0 )`. That gate is enforced
    /// at the **call sites** — the mouse-down auto-select path checks
    /// `selectable && !selected && !disabled` before calling, and `focus_next`
    /// only iterates `eligible = visible && !disabled && selectable` children —
    /// so `focus_child` itself does not re-check `ofSelectable`.
    pub fn focus_child(&mut self, id: ViewId, ctx: &mut Context) -> bool {
        // focus(): validate the outgoing current before letting it lose focus.
        if let Some(ci) = self.current.and_then(|c| self.index_of(c)) {
            let validate = self.children[ci].view.state().options.validate;
            if validate && !self.children[ci].view.valid(Command::RELEASED_FOCUS) {
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
        } else {
            self.set_current(Some(id), SelectMode::Normal, ctx);
        }
        true
    }

    /// Reset `current` to the first visible+selectable child. Ports
    /// `TGroup::resetCurrent` → `setCurrent(firstMatch(sfVisible, ofSelectable),
    /// normalSelect)`.
    pub fn reset_current(&mut self, ctx: &mut Context) {
        let p = self.first_match_visible_selectable();
        self.set_current(p, SelectMode::Normal, ctx);
    }

    /// `TGroup::firstMatch(sfVisible, ofSelectable)` — the **only** caller in
    /// row 26. C++ checks `last` (bottom, `children[0]`) **first**, then walks
    /// `first()`→down, i.e. `children[0]`, then `children[len-1], len-2, …, 1`.
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
    /// is no other eligible child (wrapping back to `current`). Ports
    /// `TGroup::findNext`.
    ///
    /// `forwards` steps C++ `p = p->next`, which (Vec mapping) walks **decreasing
    /// index with wrap** (top→…→bottom→top); `backwards` walks increasing index.
    /// Eligible = `visible && !disabled && selectable`.
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

    /// Move focus to the next selectable child in tab order. Ports
    /// `TGroup::focusNext`: focuses the [`find_next`](Self::find_next) result, or
    /// returns `true` when there is no other eligible child (faithful to C++,
    /// where `focusNext` returns `True` if `findNext` yields nothing).
    pub fn focus_next(&mut self, forwards: bool, ctx: &mut Context) -> bool {
        match self.find_next(forwards) {
            Some(id) => self.focus_child(id, ctx),
            None => true,
        }
    }

    /// Select (raise + focus) the selectable child whose [`number`](View::number)
    /// matches `num`. Returns whether a match was found. Realizes the C++
    /// `cmSelectWindowNum` broadcast arm as a **direct walk** (`TWindow::handleEvent`:
    /// `infoInt == number && (options & ofSelectable)` → `select()`).
    ///
    /// **`focus_child` is the faithful realization of C++ `select()` here** (not a
    /// raw `set_current`): `focus_child` is C++ `select()` *plus* an outgoing
    /// `valid(cmReleasedFocus)` re-check. The Alt-N call site is already gated on
    /// `canMoveFocus()` (== `deskTop->valid(cmReleasedFocus)`) upstream, so that
    /// re-check is **redundant and always passes** — it cannot refuse focus the
    /// upstream gate permitted. Windows carry `ofTopSelect`, so `focus_child` →
    /// `make_first` **raises** the window, exactly matching C++ `select()` →
    /// `makeFirst`.
    ///
    /// **The `ofSelectable` filter is explicit here** — unlike `cmNext` (whose
    /// `find_next` already filters selectable), the by-number path must check it
    /// itself (faithful to the C++ arm's `(options & ofSelectable) != 0`).
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

    // -- Z-reorder (putInFrontOf / makeFirst, realized in the owner, D3) ------

    /// `TView::putInFrontOf(target)` realized in the owner (D3 — a child cannot
    /// reorder itself; the group does it). Move child `id` so it sits immediately
    /// **in front of** `target` in Z-order. `target == None` moves `id` to the
    /// very front (top). Unknown ids are ignored.
    ///
    /// **NOTE:** `target == None` is a **to-top** sentinel used exclusively by
    /// [`make_first`](Self::make_first). Do NOT equate it with C++
    /// `Target == 0` — C++ `putInFrontOf(0)` / `insertView(p, 0)` sets
    /// `last = p`, sending the view to the **BOTTOM**, which is the opposite of
    /// this API's `None`-to-top behavior. The C++ send-to-bottom path has no
    /// consumer here and is intentionally unimplemented.
    ///
    /// **Ring → Vec mapping.** C++ `putInFrontOf(Target)` re-splices `this` so that
    /// `this->next == Target`; in next-walk order (`first()`→`last`) that places
    /// `this` immediately *ahead of* `Target` (one step closer to the top). Our Vec
    /// is back-to-front (`children[0]` == `last`/bottom, `children.last()` ==
    /// `first()`/top) and next-walk == **decreasing** index, so "`this->next ==
    /// Target`" means `id` lands at index `index_of(target) + 1` (one slot above
    /// `target`). `makeFirst` (`Target == first()`) therefore lands `id` at
    /// `children.last()` (the top).
    ///
    /// **C++ guards (faithful):** no-op if `id == target`, or if `id` is already
    /// immediately in front of `target` (C++ `Target == nextView()`), or — for
    /// `target == None` — if `id` is already the top. The `sfVisible` hide/show +
    /// `drawHide`/`drawShow` dance is **dropped (D8)**: whole-tree redraw makes it
    /// unnecessary. The trailing `if (options & ofSelectable) owner->resetCurrent()`
    /// is kept.
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

    /// `TView::makeFirst` == `putInFrontOf(first())` — move child `id` to the top
    /// (frontmost) of the Z-order. Realized in the owner (D3).
    pub fn make_first(&mut self, id: ViewId, ctx: &mut Context) {
        self.put_in_front_of(id, None, ctx);
    }

    // -- event routing helpers ----------------------------------------------

    /// Per-child eventMask gate (our mask has only the two opt-ins). Ports the
    /// `event.what & p->eventMask` test in `doHandleEvent`.
    fn wants(s: &ViewState, ev: &Event) -> bool {
        match ev {
            Event::MouseMove(_) => s.event_mask.mouse_move,
            Event::MouseAuto(_) => s.event_mask.mouse_auto,
            _ => true,
        }
    }

    /// `sfDisabled` gate — a disabled view ignores positional + focused events
    /// (`positionalEvents | focusedEvents`) but still receives broadcasts. Ports
    /// the `(p->state & sfDisabled) && (event.what & (positionalEvents |
    /// focusedEvents))` test in `doHandleEvent`.
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

    /// Deliver `ev` to child `idx` — the `doHandleEvent` core (the phase gating
    /// is applied by the caller). No-op if the event is already consumed, the
    /// child is disabled for this class, or the child has not opted into it.
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
        let mut local = *ev;
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

/// `Some(&mut Point)` for the four mouse variants, `None` otherwise — the
/// downward realization of `makeLocal` (the group rewrites the position into the
/// child's frame before delivery).
fn mouse_pos_mut(ev: &mut Event) -> Option<&mut Point> {
    match ev {
        Event::MouseDown(m) | Event::MouseUp(m) | Event::MouseMove(m) | Event::MouseAuto(m) => {
            Some(&mut m.position)
        }
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

    /// `TGroup::draw` → `drawSubViews`: paint visible children **back-to-front**
    /// (`children[0]`→`children.last()`), each through a sub-context clipped to
    /// its bounds. Painter's algorithm — deliberately reversed from C++ (which
    /// paints top-first and relies on occlusion, dropped under D8). No own-area
    /// fill: children cover it. `// TODO(row 33)`: shadow casting (no infra yet).
    fn draw(&mut self, ctx: &mut DrawCtx) {
        for child in self.children.iter_mut() {
            if child.view.state().state.visible {
                let bounds = child.view.state().get_bounds();
                let mut sub = ctx.sub(bounds);
                child.view.draw(&mut sub);
            }
        }
    }

    /// `TGroup::setState` — flip the group's own flag (+ focus broadcast via the
    /// base behaviour) then propagate: `sfActive`/`sfDragging` to **all**
    /// children; `sfFocused` to the **current** child only. Faithful port; the
    /// dropped C++ cases (`sfVisible`/`sfExposed`) are D8.
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
            StateFlag::Selected => {}
        }
    }

    /// `TGroup::changeBounds` — apply `bounds`; if the size changed, propagate the
    /// delta to every child via `calc_bounds`/`change_bounds` (the resize grow
    /// math). The D8 `getBuffer`/`lock`/`unlock` redraw bracket is dropped.
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

    /// `TView::resetCursor` (group case) — descend into the `current` child for
    /// the absolute cursor position, accumulating the child's `origin` at this
    /// level. `None` if there is no current child or it wants no cursor shown.
    /// The top-down realization of the C++ focused-chain cursor walk.
    fn cursor_request(&self) -> Option<Point> {
        let i = self.current.and_then(|id| self.index_of(id))?;
        let child = &self.children[i];
        child
            .view
            .cursor_request()
            .map(|p| p + child.view.state().origin)
    }

    /// `Group`'s [`View::find_mut`] override — the recursive tree-walk (D3).
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

    /// `Group`'s [`View::remove_descendant`] override — route to the owning
    /// group's [`remove`](Self::remove) (faithful removal + `reset_current`). If
    /// `id` is a direct child, remove it here; otherwise recurse so the group that
    /// actually owns it does the removal.
    fn remove_descendant(&mut self, id: ViewId, ctx: &mut Context) -> bool {
        if self.index_of(id).is_some() {
            self.remove(id, ctx); // direct child: faithful removal + reset_current
            return true;
        }
        for child in self.children.iter_mut() {
            if child.view.remove_descendant(id, ctx) {
                return true;
            }
        }
        false
    }

    /// `TGroup::valid` — for `cmReleasedFocus`, defer to the current child iff it
    /// has `ofValidate` (else `true`); otherwise every child must be `valid`.
    fn valid(&self, cmd: Command) -> bool {
        if cmd == Command::RELEASED_FOCUS {
            match self.current.and_then(|id| self.index_of(id)) {
                Some(i) if self.children[i].view.state().options.validate => {
                    self.children[i].view.valid(cmd)
                }
                _ => true,
            }
        } else {
            self.children.iter().all(|c| c.view.valid(cmd))
        }
    }

    /// `TGroup::awaken` — `forEach(doAwaken)`: awaken every child (order
    /// irrelevant).
    fn awaken(&mut self) {
        for child in self.children.iter_mut() {
            child.view.awaken();
        }
    }

    /// `TGroup::handleEvent` — the three-phase router (D4).
    ///
    /// * **focused events** (`KeyDown`/`Command`): `phPreProcess` (top→bottom,
    ///   `ofPreProcess` children) → `phFocused` (the current child) →
    ///   `phPostProcess` (top→bottom, `ofPostProcess` children).
    /// * **broadcast**: `phFocused`, delivered to every child (top→bottom).
    /// * **positional** (mouse): the topmost **visible** child whose bounds
    ///   contain the (group-local) position — with the relocated mouse-down
    ///   auto-select (carryover #1) applied before delivery.
    ///
    /// The C++ leading `TView::handleEvent(event)` (its own mouse-down→focus
    /// body) is **not** restored here: under D3 a view does not select *itself
    /// within itself* — that selection is the parent's job (the base
    /// `handle_event` is a no-op).
    ///
    /// **owner-extent-down (33c, D3):** the routing body is bracketed by a
    /// `ctx.set_owner_size(self.size)` / restore so a child can read its owner's
    /// size (`TWindow::zoom`). The restore is **unconditional**: the actual
    /// routing lives in [`route_event`](Self::route_event), which may `return`
    /// early (the positional `mouse_pos` guard), so the bracket here guarantees a
    /// parent group's later sibling deliveries never see this group's size.
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        let saved_owner_size = ctx.owner_size();
        ctx.set_owner_size(self.st.size); // children's owner is THIS group (Copy read first)
        self.route_event(ev, ctx);
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
            Event::KeyDown(_) | Event::Command(_) => {
                // phPreProcess: forEach top→bottom, ofPreProcess children only.
                for i in (0..n).rev() {
                    if self.children[i].view.state().options.pre_process {
                        self.deliver(i, ev, ctx);
                    }
                }
                // phFocused: the current child only (no phase-option gate).
                if let Some(i) = self.current.and_then(|id| self.index_of(id)) {
                    self.deliver(i, ev, ctx);
                }
                // phPostProcess: forEach top→bottom, ofPostProcess children only.
                for i in (0..n).rev() {
                    if self.children[i].view.state().options.post_process {
                        self.deliver(i, ev, ctx);
                    }
                }
            }
            // -- broadcast: phFocused, every child (incl. disabled) -----------
            // Also carries timer-expiry (`Event::Timer`), which is broadcast-class
            // (the `evBroadcast cmTimerExpired` successor) and so delivers to every
            // child identically.
            Event::Broadcast { .. } | Event::Timer(_) => {
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

    /// A probe view: fills its extent with `ch` and records every event it is
    /// handed (post-translation), so tests can assert routing/order/coords.
    struct Probe {
        st: ViewState,
        ch: char,
        log: Rc<RefCell<Vec<Event>>>,
        /// Mirrors `View::grabs_focus_on_click` (default true). Set false to
        /// model a TButton-without-`bfGrabFocus` (the carryover-#1 opt-out).
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
            self.log.borrow_mut().push(*ev);
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
        fn valid(&self, _cmd: Command) -> bool {
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
        let events: Vec<Event> = out.iter().copied().collect();
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Broadcast { command, .. } if *command == Command::RELEASED_FOCUS
            )),
            "A releases focus"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Broadcast { command, .. } if *command == Command::RECEIVED_FOCUS
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

    #[test]
    fn focused_dispatch_visits_pre_then_current_then_post() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        // Shared order log: each probe pushes a tagged event so we read order.
        let order = Rc::new(RefCell::new(Vec::new()));

        // distinct logs to identify which child saw it via the recorded char-key
        struct Tagged {
            st: ViewState,
            tag: char,
            order: Rc<RefCell<Vec<char>>>,
        }
        impl View for Tagged {
            fn state(&self) -> &ViewState {
                &self.st
            }
            fn state_mut(&mut self) -> &mut ViewState {
                &mut self.st
            }
            fn draw(&mut self, _ctx: &mut DrawCtx) {}
            fn handle_event(&mut self, _ev: &mut Event, _ctx: &mut Context) {
                self.order.borrow_mut().push(self.tag);
                // does NOT consume — so all phases get a chance to run
            }
        }

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
            vec!['P', 'C', 'O'],
            "pre-process, then current, then post-process; plain child skipped"
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
            !group.valid(Command::OK),
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
            !group.valid(Command::RELEASED_FOCUS),
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
            group2.valid(Command::RELEASED_FOCUS),
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

    // -- 15. owner_size set during routing + unconditional restore (33c) ------

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

    // -- focus_by_number (33d-2) ---------------------------------------------

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
}
