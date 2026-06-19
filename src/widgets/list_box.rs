//! A concrete list box over a `Vec<String>`: the first concrete
//! [`ListViewer`](crate::widgets::list_viewer::ListViewer). It holds the items
//! and delegates all draw/event/nav logic to the shared list-viewer functions via
//! the [`ListViewer`] trait. Only [`get_text`](ListViewer::get_text) is
//! overridden; `is_selected` and `select_item` inherit the base behavior
//! (`item == focused`, and the base broadcasts that an item was selected).
//!
//! # Population wiring
//!
//! The constructor sets fields only (empty items, range 0) — no [`Context`] is
//! available at construction. After insertion into a group:
//!
//! 1. Call [`new_list`](ListBox::new_list) to populate the items and publish the
//!    vertical-bar range + focus position.
//! 2. Call [`list_viewer::update_steps`](crate::widgets::list_viewer::update_steps)
//!    to publish the page/arrow step sizes to the bars.
//!
//! Missing step 1 leaves the list empty; missing step 2 leaves the scrollbar
//! thumb unsized. Both require a `Context`, so they cannot run in the constructor.
//!
//! # The focused item as a value
//!
//! The list box's [`value`](View::value) is its focused item index. Scattering a
//! value back (focusing an item) is done through
//! [`set_value_ctx`](ListBox::set_value_ctx) rather than the plain
//! [`set_value`](View::set_value), because focusing an item must republish the
//! scroll-bar position and so needs a [`Context`].
//!
//! # Turbo Vision heritage
//!
//! Ports `TListBox` (`tlistbox.cpp`). The list-record get/set hooks become the
//! typed value protocol (D10); mouse press-and-hold, auto-scroll, and resize
//! handling all live in the shared list-viewer base.

use crate::data::FieldValue;
use crate::event::Event;
use crate::view::{Context, DrawCtx, Point, StateFlag, View, ViewId, ViewState};
use crate::widgets::list_viewer::{self, ListViewer, ListViewerState};

/// A concrete list viewer over a `Vec<String>`.
///
/// Reuses all of the shared list-viewer draw/event/nav logic via the
/// [`ListViewer`] trait and overrides only [`get_text`](ListViewer::get_text).
/// See the module doc for population wiring notes.
pub struct ListBox {
    lv: ListViewerState,
    items: Vec<String>,
}

impl ListBox {
    /// Construct a new, empty list box.
    ///
    /// Builds the shared list-viewer state and an empty item list; the top item,
    /// focus, and range all start at 0. No `Context` here — publish the vertical
    /// bar's range + steps with [`new_list`](Self::new_list) +
    /// [`list_viewer::update_steps`](list_viewer::update_steps) after insertion.
    pub fn new(
        bounds: crate::view::Rect,
        num_cols: i32,
        h: Option<ViewId>,
        v: Option<ViewId>,
    ) -> Self {
        ListBox {
            lv: ListViewerState::new(bounds, num_cols, h, v),
            items: Vec::new(),
        }
    }

    /// Replace the item collection and (re)publish the vertical bar's range.
    ///
    /// Replaces `self.items`; calls `set_range(len)` (publishes the vertical-bar
    /// params); calls `focus_item(0)` iff `range > 0`. The old items drop on
    /// assignment.
    ///
    /// Call this **post-insert**, with a `Context`, so the v-bar `ViewId`s are
    /// resolvable and the deferred ops land correctly. Also call
    /// [`list_viewer::update_steps`](list_viewer::update_steps) after this to
    /// publish the page/arrow step sizes.
    pub fn new_list(&mut self, items: Vec<String>, ctx: &mut Context) {
        self.items = items;
        let len = self.items.len() as i32;
        list_viewer::set_range(self, len, ctx);
        if self.lv.range > 0 {
            list_viewer::focus_item(self, 0, ctx);
        }
    }

    /// The current item collection.
    pub fn list(&self) -> &[String] {
        &self.items
    }
}

impl ListViewer for ListBox {
    fn lv(&self) -> &ListViewerState {
        &self.lv
    }

    fn lv_mut(&mut self) -> &mut ListViewerState {
        &mut self.lv
    }

    /// Return the text for `item` from the owned Vec.
    ///
    /// An out-of-bounds index (including an empty list) returns an empty string.
    fn get_text(&self, item: i32) -> String {
        self.items.get(item as usize).cloned().unwrap_or_default()
    }
    // is_selected / select_item: inherit the base (item == focused, and the base
    // broadcasts that an item was selected). Not overridden here.
}

impl View for ListBox {
    fn state(&self) -> &ViewState {
        &self.lv.state
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.lv.state
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        list_viewer::draw(self, ctx);
    }

    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        list_viewer::handle_event(self, ev, ctx);
    }

    fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
        list_viewer::set_state(self, flag, enable, ctx);
    }

    /// Republish the scroll-bar steps after a bounds change.
    fn on_bounds_changed(&mut self, ctx: &mut Context) {
        list_viewer::on_bounds_changed(self, ctx);
    }

    fn cursor_request(&self) -> Option<Point> {
        list_viewer::focused_cursor(self)
    }

    fn apply_scroll_sync(&mut self, h: Option<i32>, v: Option<i32>, ctx: &mut Context) {
        list_viewer::apply_scroll(self, h, v, ctx);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    /// The focused item index as a typed `FieldValue::Int`. The collection is
    /// configuration ([`new_list`](Self::new_list) manages it), not part of the
    /// transferable value.
    fn value(&self) -> Option<FieldValue> {
        Some(FieldValue::Int(self.lv.focused))
    }

    /// Set the focused item and republish the vertical bar. The item list is
    /// **not** transferred (it is configuration, managed via
    /// [`new_list`](Self::new_list), not dialog data).
    ///
    /// Uses `focus_item_num` (the range-clamped variant) in case the range
    /// changed between gather and scatter.
    fn set_value_ctx(&mut self, v: FieldValue, ctx: &mut Context) {
        if let FieldValue::Int(idx) = v {
            list_viewer::focus_item_num(self, idx, ctx);
        }
    }
}

// ---------------------------------------------------------------------------
// SortedListBox
// ---------------------------------------------------------------------------

/// Case-insensitive ordering (ASCII-fold; list items are filenames or labels).
fn ci_cmp(a: &str, b: &str) -> core::cmp::Ordering {
    a.chars()
        .map(|c| c.to_ascii_lowercase())
        .cmp(b.chars().map(|c| c.to_ascii_lowercase()))
}

/// A list viewer with incremental type-to-search over a case-insensitively
/// sorted string list. A **direct `ListViewer` impl** (like [`ListBox`]); the
/// search state machine lives as the
/// [`sorted_handle_event`](crate::widgets::list_viewer::sorted_handle_event) /
/// [`sorted_cursor`](crate::widgets::list_viewer::sorted_cursor) free functions
/// over the [`SortedSearch`](crate::widgets::list_viewer::SortedSearch) sub-trait,
/// which this widget implements.
///
/// ## Design notes
/// * The list is an owned `Vec<String>`; `new_list` keeps it CASE-INSENSITIVELY
///   SORTED so the binary search and the case-insensitive prefix-confirm cohere.
///   File/dir subclasses can impose their own ordering.
/// * The search key here is the typed prefix itself; a file/dir subclass
///   overrides `search` to supply its own key derivation and ordering.
/// * The shift bits are captured but UNUSED in this base — a file list reads them.
///
/// # Turbo Vision heritage
///
/// Ports `TSortedListBox` (`stddlg.cpp`); the injected sorted collection becomes
/// the owned, case-insensitively sorted `Vec<String>`.
pub struct SortedListBox {
    lv: ListViewerState,
    items: Vec<String>,
    /// The index of the last matched char in the focused item's text; -1 = no
    /// active search.
    search_pos: i32,
    /// Captured shift bits; UNUSED in this base (a file list reads them).
    shift_state: u8,
}

impl SortedListBox {
    /// Construct a sorted list box, showing the cursor at column 1.
    pub fn new(
        bounds: crate::view::Rect,
        num_cols: i32,
        h: Option<ViewId>,
        v: Option<ViewId>,
    ) -> Self {
        let mut lv = ListViewerState::new(bounds, num_cols, h, v);
        lv.state.show_cursor();
        lv.state.set_cursor(1, 0);
        SortedListBox {
            lv,
            items: Vec::new(),
            search_pos: -1,
            shift_state: 0,
        }
    }

    /// Sort the items CASE-INSENSITIVELY, (re)publish the vertical bar's range +
    /// focus, and reset the search. Mirrors [`ListBox::new_list`].
    pub fn new_list(&mut self, mut items: Vec<String>, ctx: &mut Context) {
        items.sort_by(|a, b| ci_cmp(a, b));
        self.items = items;
        let len = self.items.len() as i32;
        list_viewer::set_range(self, len, ctx);
        if self.lv.range > 0 {
            list_viewer::focus_item(self, 0, ctx);
        }
        self.search_pos = -1;
    }

    /// The current item collection.
    pub fn list(&self) -> &[String] {
        &self.items
    }

    /// Test accessor for `search_pos` (used in tests only).
    #[cfg(test)]
    pub(crate) fn search_pos(&self) -> i32 {
        self.search_pos
    }
}

impl ListViewer for SortedListBox {
    fn lv(&self) -> &ListViewerState {
        &self.lv
    }

    fn lv_mut(&mut self) -> &mut ListViewerState {
        &mut self.lv
    }

    /// Return the text for `item` from the owned Vec.
    fn get_text(&self, item: i32) -> String {
        self.items.get(item as usize).cloned().unwrap_or_default()
    }
    // is_selected / select_item: inherit the base (item == focused, and the base
    // broadcasts that an item was selected). Not overridden here.
}

impl list_viewer::SortedSearch for SortedListBox {
    fn search_pos(&self) -> i32 {
        self.search_pos
    }

    fn set_search_pos(&mut self, pos: i32) {
        self.search_pos = pos;
    }

    fn shift_state(&self) -> u8 {
        self.shift_state
    }

    fn set_shift_state(&mut self, s: u8) {
        self.shift_state = s;
    }

    /// The search key IS the typed prefix `cur`. Returns the first index `i` in
    /// `0..range` whose item is `>= key` case-insensitively (the insertion
    /// point), or `range` if none. Binary search over `get_text(i)`.
    fn search(&self, cur: &[char]) -> i32 {
        let key: String = cur.iter().collect();
        let range = self.lv.range;
        let (mut lo, mut hi) = (0i32, range);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if ci_cmp(&self.get_text(mid), &key) == core::cmp::Ordering::Less {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }
}

impl View for SortedListBox {
    fn state(&self) -> &ViewState {
        &self.lv.state
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.lv.state
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        list_viewer::draw(self, ctx);
    }

    /// The incremental type-to-search state machine, shared verbatim via the
    /// [`sorted_handle_event`](list_viewer::sorted_handle_event) free function
    /// over the `SortedSearch` sub-trait.
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        list_viewer::sorted_handle_event(self, ev, ctx);
    }

    fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
        list_viewer::set_state(self, flag, enable, ctx);
    }

    /// Republish the scroll-bar steps after a bounds change.
    fn on_bounds_changed(&mut self, ctx: &mut Context) {
        list_viewer::on_bounds_changed(self, ctx);
    }

    /// Cursor advanced past the matched prefix — shared via
    /// [`sorted_cursor`](list_viewer::sorted_cursor).
    fn cursor_request(&self) -> Option<Point> {
        list_viewer::sorted_cursor(self)
    }

    fn apply_scroll_sync(&mut self, h: Option<i32>, v: Option<i32>, ctx: &mut Context) {
        list_viewer::apply_scroll(self, h, v, ctx);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn value(&self) -> Option<FieldValue> {
        Some(FieldValue::Int(self.lv.focused))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::event::{Key, KeyEvent, KeyModifiers};
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use crate::view::{Deferred, Group, Rect};
    use std::collections::VecDeque;

    fn make_ctx<'a>(
        out: &'a mut VecDeque<Event>,
        timers: &'a mut crate::timer::TimerQueue,
        deferred: &'a mut Vec<Deferred>,
    ) -> Context<'a> {
        Context::new(out, timers, 0, deferred)
    }

    fn key_ev(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(k, KeyModifiers::default()))
    }

    /// Render a ListBox into a snapshot string.
    fn render(lb: &mut ListBox, w: u16, h: u16) -> String {
        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(w, h);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = lb.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            lb.draw(&mut dc);
        });
        screen.snapshot()
    }

    // -- 1. ctor ----------------------------------------------------------------

    #[test]
    fn ctor_empty_items_and_zeroed_fields() {
        let lb = ListBox::new(Rect::new(0, 0, 20, 8), 1, None, None);
        assert!(lb.lv.state.options.first_click, "ofFirstClick set");
        assert!(lb.lv.state.options.selectable, "ofSelectable set");
        assert_eq!(lb.lv.range, 0, "range starts at 0");
        assert_eq!(lb.lv.focused, 0, "focused starts at 0");
        assert_eq!(lb.lv.top_item, 0, "top_item starts at 0");
        assert_eq!(lb.lv.indent, 0, "indent starts at 0");
        assert_eq!(lb.lv.num_cols, 1, "num_cols == 1");
        assert!(lb.items.is_empty(), "items starts empty");
    }

    // -- 2. new_list --------------------------------------------------------

    #[test]
    fn new_list_sets_range_and_queues_vbar_params() {
        // Need a real ViewId for the v-bar.
        let mut mint_group = Group::new(Rect::new(0, 0, 4, 4));
        let sentinel =
            mint_group.insert(Box::new(ListBox::new(Rect::new(0, 0, 1, 1), 1, None, None)));

        let mut lb = ListBox::new(Rect::new(0, 0, 20, 8), 1, None, Some(sentinel));
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            lb.new_list(
                vec!["alpha".into(), "beta".into(), "gamma".into()],
                &mut ctx,
            );
        }
        assert_eq!(lb.lv.range, 3, "range == N after new_list");
        assert_eq!(lb.lv.focused, 0, "focus_item(0) called");
        // set_range queues ScrollBarSetParams{value:0, min:0, max:2, pg:None, ar:None}
        // focus_item queues ScrollBarSetParams{value:0, min:None, max:None, …}
        assert_eq!(
            deferred.len(),
            2,
            "set_range + focus_item each queue one op"
        );
        assert!(matches!(
            deferred[0],
            Deferred::ScrollBarSetParams {
                id,
                value: Some(0),
                min: Some(0),
                max: Some(2),
                page_step: None,
                arrow_step: None,
            } if id == sentinel
        ));
        assert!(matches!(
            deferred[1],
            Deferred::ScrollBarSetParams {
                id,
                value: Some(0),
                min: None,
                max: None,
                page_step: None,
                arrow_step: None,
            } if id == sentinel
        ));
    }

    #[test]
    fn new_list_empty_skips_focus_item() {
        let mut lb = ListBox::new(Rect::new(0, 0, 20, 8), 1, None, None);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            lb.new_list(vec![], &mut ctx);
        }
        assert_eq!(lb.lv.range, 0, "range == 0 for empty list");
        // No v-bar, so set_range queues nothing; focus_item not called.
        assert!(deferred.is_empty(), "empty list queues nothing");
    }

    #[test]
    fn new_list_replaces_previous_items() {
        let mut lb = ListBox::new(Rect::new(0, 0, 20, 8), 1, None, None);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            lb.new_list(vec!["first".into()], &mut ctx);
        }
        deferred.clear();
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            lb.new_list(vec!["second".into(), "third".into()], &mut ctx);
        }
        assert_eq!(lb.items.len(), 2, "old items replaced");
        assert_eq!(lb.items[0], "second");
        assert_eq!(lb.items[1], "third");
        assert!(
            lb.items.iter().all(|s| s != "first"),
            "old item 'first' is gone"
        );
    }

    // -- 3. get_text --------------------------------------------------------

    #[test]
    fn get_text_returns_item_or_empty_for_oob() {
        let mut lb = ListBox::new(Rect::new(0, 0, 20, 8), 1, None, None);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            lb.new_list(vec!["alpha".into(), "beta".into()], &mut ctx);
        }
        // In-range: a real item differs from empty (bite check).
        let text0 = lb.get_text(0);
        assert_eq!(text0, "alpha");
        assert_ne!(
            text0, "",
            "in-range item is not empty (bite: distinguishes from OOB)"
        );

        let text1 = lb.get_text(1);
        assert_eq!(text1, "beta");

        // Out-of-range returns empty string (faithful: C++ `*dest = EOS`).
        assert_eq!(lb.get_text(2), "");
        assert_eq!(lb.get_text(99), "");
        assert_eq!(lb.get_text(-1_i32), "");
    }

    // -- 4. value() ---------------------------------------------------------

    #[test]
    fn value_reflects_focused_item() {
        let mut lb = ListBox::new(Rect::new(0, 0, 20, 8), 1, None, None);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            lb.new_list(
                vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()],
                &mut ctx,
            );
        }
        // Initial focused == 0.
        assert_eq!(lb.value(), Some(FieldValue::Int(0)), "initial focused == 0");

        // Drive focus to item 2 via KeyDown(Down) twice.
        deferred.clear();
        let mut ev = key_ev(Key::Down);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            lb.handle_event(&mut ev, &mut ctx);
        }
        let mut ev = key_ev(Key::Down);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            lb.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(lb.lv.focused, 2, "focus moved to 2");
        // value() must reflect the new focus (bite: 0 vs 2).
        assert_eq!(
            lb.value(),
            Some(FieldValue::Int(2)),
            "value() reflects focused == 2 (not the initial 0)"
        );
    }

    // -- 5. draw snapshot ---------------------------------------------------

    #[test]
    fn snapshot_active_focused_list_box() {
        let mut lb = ListBox::new(Rect::new(0, 0, 14, 5), 1, None, None);
        lb.lv.state.state.selected = true;
        lb.lv.state.state.active = true;
        // Set items directly (no Context needed for draw test; range set manually).
        lb.items = vec![
            "apple".into(),
            "banana".into(),
            "cherry".into(),
            "date".into(),
        ];
        lb.lv.range = 4;
        lb.lv.focused = 1;
        insta::assert_snapshot!(render(&mut lb, 14, 5));
    }

    // -- 6. delegation smoke ------------------------------------------------

    #[test]
    fn handle_event_wired_down_moves_focused() {
        let mut lb = ListBox::new(Rect::new(0, 0, 20, 8), 1, None, None);
        lb.items = vec!["x".into(), "y".into(), "z".into()];
        lb.lv.range = 3;
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        let mut ev = key_ev(Key::Down);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            lb.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(lb.lv.focused, 1, "KeyDown(Down) wired: focused moves to 1");
        assert!(ev.is_nothing(), "Down consumed");
    }

    #[test]
    fn broadcast_from_own_vbar_queues_sync_list_viewer() {
        // Insert the list box into a group so it has a ViewId.
        let mut group = Group::new(Rect::new(0, 0, 30, 20));

        // Mint a v-bar id.
        let mut vbar_group = Group::new(Rect::new(0, 0, 4, 4));
        let v_id = vbar_group.insert(Box::new(ListBox::new(Rect::new(0, 0, 1, 1), 1, None, None)));

        let lb_id = group.insert(Box::new(ListBox::new(
            Rect::new(0, 0, 20, 8),
            1,
            None,
            Some(v_id),
        )));

        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        // cmScrollBarChanged from own v-bar → SyncListViewer queued.
        let mut ev = Event::Broadcast {
            command: crate::command::Command::SCROLL_BAR_CHANGED,
            source: Some(v_id),
        };
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            group
                .find_mut(lb_id)
                .unwrap()
                .handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(deferred.len(), 1, "one SyncListViewer op queued");
        assert!(
            matches!(
                deferred[0],
                Deferred::SyncListViewer {
                    list,
                    h: None,
                    v: Some(vid),
                } if list == lb_id && vid == v_id
            ),
            "SyncListViewer carries correct list and v-bar ids"
        );
    }

    // =========================================================================
    // SortedListBox tests
    // =========================================================================

    // Helper: build a SortedListBox populated with the given items (pre-sorted
    // externally for readability, but new_list will sort them anyway).
    fn make_sorted_lb(
        items: Vec<&str>,
    ) -> (
        SortedListBox,
        VecDeque<Event>,
        crate::timer::TimerQueue,
        Vec<Deferred>,
    ) {
        let mut slb = SortedListBox::new(Rect::new(0, 0, 20, 8), 1, None, None);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            slb.new_list(items.into_iter().map(|s| s.into()).collect(), &mut ctx);
        }
        deferred.clear();
        (slb, out, timers, deferred)
    }

    // -- SLB 1. type-to-jump ---------------------------------------------------

    #[test]
    fn sorted_lb_type_to_jump_b_then_br() {
        use crate::widgets::list_viewer::ListViewer;
        let (mut slb, mut out, mut timers, mut deferred) =
            make_sorted_lb(vec!["alpha", "beta", "bravo", "charlie"]);
        // After new_list sorts: ["alpha", "beta", "bravo", "charlie"].
        // Focused starts at 0 ("alpha").

        // Type 'b' → jump to first item starting with 'b' ("beta" at index 1).
        let mut ev = key_ev(Key::Char('b'));
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            slb.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(slb.lv().focused, 1, "'b' -> focused == 1 (\"beta\")");
        assert_eq!(slb.search_pos(), 0, "search_pos == 0 after first char");
        assert!(ev.is_nothing(), "'b' consumed (alpha match found)");

        deferred.clear();

        // Type 'r' → advance to "bravo" (index 2).
        let mut ev = key_ev(Key::Char('r'));
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            slb.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(slb.lv().focused, 2, "'br' -> focused == 2 (\"bravo\")");
        assert_eq!(slb.search_pos(), 1, "search_pos == 1 after second char");
        assert!(ev.is_nothing(), "'r' consumed");
    }

    // -- SLB 2. backspace shortens ---------------------------------------------

    #[test]
    fn sorted_lb_backspace_shortens_search() {
        use crate::widgets::list_viewer::ListViewer;
        let (mut slb, mut out, mut timers, mut deferred) =
            make_sorted_lb(vec!["alpha", "beta", "bravo", "charlie"]);

        // Type "br" to focus "bravo".
        for ch in ['b', 'r'] {
            let mut ev = key_ev(Key::Char(ch));
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            slb.handle_event(&mut ev, &mut ctx);
            deferred.clear();
        }
        assert_eq!(slb.lv().focused, 2, "pre: focused == 2 (\"bravo\")");
        assert_eq!(slb.search_pos(), 1, "pre: search_pos == 1");

        // Backspace → search shortens to "b" and re-resolves.
        // cur is re-seeded from "bravo" (the focused item), truncated to 1 char ("b").
        // search("b") finds "beta" (index 1) or "bravo" (index 2) — the first
        // item >= "b" case-insensitively.  "beta" < "bravo" alphabetically, so
        // search returns index 1 ("beta").
        let mut ev = key_ev(Key::Backspace);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            slb.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(slb.search_pos(), 0, "search_pos decremented to 0");
        // Focus should be on the first item matching "b" prefix ("beta").
        assert_eq!(slb.lv().focused, 1, "backspace re-focuses to \"beta\"");
    }

    // -- SLB 3. dot jumps to the extension separator ---------------------------

    #[test]
    fn sorted_lb_dot_jumps_to_extension() {
        use crate::widgets::list_viewer::ListViewer;
        // Same-basename sibling: after case-insensitive sort the order is
        // ["file.bak", "file.txt", "zebra"]. The dot branch must search for the
        // FULL focused text ("file.txt"), NOT the truncated prefix ("file."):
        // searching "file." would binary-resolve to "file.bak" (index 0) and,
        // since ci_prefix_eq("file.txt","file.bak",5) is true, wrongly jump there.
        // Searching the full "file.txt" lands on index 1 (the same item) → no jump.
        let (mut slb, mut out, mut timers, mut deferred) =
            make_sorted_lb(vec!["file.txt", "file.bak", "zebra"]);
        // After sort: index 0 = "file.bak", 1 = "file.txt", 2 = "zebra".
        assert_eq!(slb.get_text(0), "file.bak");
        assert_eq!(slb.get_text(1), "file.txt");
        assert_eq!(slb.lv().focused, 0, "starts at 0 (\"file.bak\")");

        // Type "file.t" up to (but not including) the dot, then the dot, then a
        // final char, navigating focus onto "file.txt" first.
        // Type 'f' → first item >= "f" is "file.bak" (index 0).
        for (ch, want_focus) in [('f', 0), ('i', 0), ('l', 0), ('e', 0)] {
            let mut ev = key_ev(Key::Char(ch));
            {
                let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
                slb.handle_event(&mut ev, &mut ctx);
            }
            assert_eq!(
                slb.lv().focused,
                want_focus,
                "typing '{ch}' keeps focus on \"file.bak\" (shared prefix)"
            );
            deferred.clear();
        }
        assert_eq!(slb.search_pos(), 3, "search_pos == 3 after \"file\"");

        // Now move focus onto "file.txt" via Down (arrow nav resets search_pos).
        let mut ev = key_ev(Key::Down);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            slb.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(slb.lv().focused, 1, "Down -> focus \"file.txt\"");
        assert_eq!(slb.search_pos(), -1, "arrow nav reset search_pos");
        deferred.clear();

        // Press '.' → cur is re-seeded from the FOCUSED item "file.txt"; the dot
        // branch finds '.' at index 4, so search_pos = 4. The search key MUST be
        // the full "file.txt", landing on index 1 (same item) → focus must NOT
        // mis-jump to "file.bak".
        let mut ev = key_ev(Key::Char('.'));
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            slb.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(
            slb.search_pos(),
            4,
            "dot sets search_pos to the '.' position"
        );
        assert_eq!(
            slb.lv().focused,
            1,
            "dot must NOT mis-jump to \"file.bak\"; stays on \"file.txt\""
        );
    }

    // -- SLB 4. no-match reverts but alpha still consumes ----------------------

    #[test]
    fn sorted_lb_no_match_alpha_consumes_but_reverts() {
        use crate::widgets::list_viewer::ListViewer;
        let (mut slb, mut out, mut timers, mut deferred) =
            make_sorted_lb(vec!["alpha", "beta", "bravo", "charlie"]);

        // Type 'z' — no item starts with 'z'. Focused stays at 0.
        // search_pos reverts to -1 (the old_pos before the attempt).
        // Event is CONSUMED because 'z' is alpha.
        let mut ev = key_ev(Key::Char('z'));
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            slb.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(slb.lv().focused, 0, "no match: focus unchanged");
        assert_eq!(slb.search_pos(), -1, "no match: search_pos reverted to -1");
        assert!(ev.is_nothing(), "alpha key consumed even on no-match");
    }

    // -- SLB 5. no-match punctuation passes through ----------------------------

    #[test]
    fn sorted_lb_no_match_punctuation_passes_through() {
        use crate::widgets::list_viewer::ListViewer;
        // Items have no '.' so the dot branch sets search_pos = -1 (no dot found).
        // After revert the event should NOT be cleared (not alpha, search_pos unchanged).
        let (mut slb, mut out, mut timers, mut deferred) =
            make_sorted_lb(vec!["alpha", "beta", "bravo", "charlie"]);

        // search_pos is -1. Type '.' — dot branch finds no '.' in "alpha"
        // (the focused item), so search_pos = -1 (same as old_pos).
        // is_alpha is false for '.'; search_pos == old_pos (-1 == -1) → NOT consumed.
        let mut ev = key_ev(Key::Char('.'));
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            slb.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(slb.lv().focused, 0, "focus unchanged");
        assert_eq!(slb.search_pos(), -1, "search_pos stays -1");
        assert!(!ev.is_nothing(), "non-alpha no-match: event NOT consumed");
    }

    // -- SLB 6. arrow nav resets search ----------------------------------------

    #[test]
    fn sorted_lb_arrow_nav_resets_search_pos() {
        let (mut slb, mut out, mut timers, mut deferred) =
            make_sorted_lb(vec!["alpha", "beta", "bravo", "charlie"]);

        // Type 'b' to start a search.
        let mut ev = key_ev(Key::Char('b'));
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            slb.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(slb.search_pos(), 0, "pre: search_pos == 0");
        deferred.clear();

        // Send Down — the base handle_event moves focused (1→2 or further),
        // which triggers the `old_value != focused` reset.
        let mut ev = key_ev(Key::Down);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            slb.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(slb.search_pos(), -1, "arrow nav resets search_pos to -1");
    }

    // -- SLB 7. cmReleasedFocus resets search ----------------------------------

    #[test]
    fn sorted_lb_released_focus_resets_search_pos() {
        let (mut slb, mut out, mut timers, mut deferred) =
            make_sorted_lb(vec!["alpha", "beta", "bravo", "charlie"]);

        // Type 'b' to start a search.
        let mut ev = key_ev(Key::Char('b'));
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            slb.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(slb.search_pos(), 0, "pre: search_pos == 0");
        deferred.clear();

        // Send cmReleasedFocus broadcast.
        let mut ev = Event::Broadcast {
            command: crate::command::Command::RELEASED_FOCUS,
            source: None,
        };
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            slb.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(slb.search_pos(), -1, "cmReleasedFocus resets search_pos");
    }

    // -- SLB 8. new_list sorts case-insensitively and resets search ------------

    #[test]
    fn sorted_lb_new_list_sorts_and_resets() {
        use crate::widgets::list_viewer::ListViewer;
        let mut slb = SortedListBox::new(Rect::new(0, 0, 20, 8), 1, None, None);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        // Provide items deliberately out of order; new_list should sort them.
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            slb.new_list(
                vec!["Zebra".into(), "apple".into(), "Banana".into()],
                &mut ctx,
            );
        }
        // Case-insensitive sort: "apple" < "Banana" < "Zebra".
        assert_eq!(slb.get_text(0), "apple", "sorted: apple first");
        assert_eq!(slb.get_text(1), "Banana", "sorted: Banana second");
        assert_eq!(slb.get_text(2), "Zebra", "sorted: Zebra third");
        assert_eq!(slb.search_pos(), -1, "new_list resets search_pos");
    }

    // -- set_value_ctx -------------------------------------------------------

    #[test]
    fn set_value_ctx_focuses_the_item() {
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = Vec::new();
        let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);

        let mut lb = ListBox::new(Rect::new(0, 0, 10, 5), 1, None, None);
        lb.new_list(
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
            &mut ctx,
        );

        // Focus item 2 via set_value_ctx.
        lb.set_value_ctx(FieldValue::Int(2), &mut ctx);

        // value() returns the focused index.
        assert_eq!(lb.value(), Some(FieldValue::Int(2)));
    }

    #[test]
    fn set_value_ctx_ignores_non_int_variant() {
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = Vec::new();
        let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);

        let mut lb = ListBox::new(Rect::new(0, 0, 10, 5), 1, None, None);
        lb.new_list(vec!["a".to_string(), "b".to_string()], &mut ctx);
        // Focus item 1 first.
        lb.set_value_ctx(FieldValue::Int(1), &mut ctx);
        assert_eq!(lb.value(), Some(FieldValue::Int(1)));

        // Passing a Text variant should be ignored (no panic, focus unchanged).
        lb.set_value_ctx(FieldValue::Text("ignored".to_string()), &mut ctx);
        assert_eq!(
            lb.value(),
            Some(FieldValue::Int(1)),
            "focus unchanged after Text variant"
        );
    }
}
