//! Process-global, byte-budget-bounded history store for input fields, plus the
//! [`HistoryViewer`] widget that shows the store in a modal recall list.
//!
//! Each "channel" is a small `u8` id that groups one input field's recall
//! list.  Entries are stored oldest-first globally; `history_str(id, 0)`
//! returns the oldest surviving entry for that id.
//!
//! # Deviation from C++ (`histlist.cpp`)
//!
//! The C++ implementation keeps a hidden **front sentinel** record (written by
//! `clearHistory` / `initHistory`) and `advanceStringPointer` always skips it
//! before matching.  A side-effect: once the budget is first exceeded and the
//! sentinel is evicted, the *actual* globally-oldest entry becomes the new
//! front and `advanceStringPointer` skips it — hiding it from
//! `historyCount`/`historyStr`.  This is a byte-block bookkeeping artifact,
//! not intentional designed behavior.
//!
//! **We model the clean contract: no sentinel, no front-skip — every
//! non-evicted entry is readable.**  Pre-overflow behavior is identical to
//! C++; the only divergence is a single hidden globally-oldest entry that the
//! C++ implementation would lose after the budget is first exceeded.  This
//! deviation is intentional and documented here so it is not mistaken for a
//! missing behavior.
//!
//! One precision note: because C++ carries its 3-byte front sentinel inside
//! its `used` accounting, C++'s real-entry budget is 3 bytes tighter, so its
//! first-eviction byte boundary differs from ours by 3 bytes.  This is a
//! direct consequence of the no-sentinel model above, not a separate
//! divergence.

use crate::command::Command;
use crate::event::{Event, Key};
use crate::view::{Context, DrawCtx, Point, Rect, StateFlag, View, ViewId, ViewState};
use crate::widgets::list_viewer::{self, ListViewer, ListViewerState};
use std::cell::RefCell;

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

struct HistRec {
    id: u8,
    str: String,
}

impl HistRec {
    /// Byte cost of one entry, matching the C++ `len = str.size() + 3` formula.
    fn cost(&self) -> usize {
        cost_of(&self.str)
    }
}

/// Byte cost of a candidate string — the single source of truth for the C++
/// `len = str.size() + 3` formula.
fn cost_of(s: &str) -> usize {
    s.len() + 3
}

// ---------------------------------------------------------------------------
// Thread-local store
//
// Thread-local is deliberate: `libtest` runs each `#[test]` on its own
// thread, giving each test a pristine store — no `Mutex` needed, and this
// faithfully models the single-threaded C++ design.
// ---------------------------------------------------------------------------

thread_local! {
    static HISTORY: RefCell<Vec<HistRec>> = const { RefCell::new(Vec::new()) };
}

/// Maximum byte budget shared across **all** ids (faithful to C++ `historySize`).
const HISTORY_SIZE: usize = 1024;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Total bytes currently used by all entries.
fn used_bytes(history: &[HistRec]) -> usize {
    history.iter().map(HistRec::cost).sum()
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Add `str` to the history channel identified by `id`.
///
/// Operation order (faithful to C++):
/// 1. Ignore empty strings.
/// 2. Remove any existing duplicate for this `(id, str)` pair.
/// 3. Evict globally-oldest entries until the new entry fits within the budget.
/// 4. Append the new entry (newest position).
pub fn history_add(id: u8, str: &str) {
    if str.is_empty() {
        return;
    }

    HISTORY.with(|h| {
        let mut history = h.borrow_mut();

        // Step 2 — remove existing duplicate (at most one can exist).
        history.retain(|e| !(e.id == id && e.str == str));

        // Step 3 — evict globally-oldest entries until the new entry fits.
        let new_cost = cost_of(str);
        while used_bytes(&history) + new_cost > HISTORY_SIZE {
            if history.is_empty() {
                // The string alone exceeds the budget; bail out.
                return;
            }
            history.remove(0);
        }

        // Step 4 — append (newest).
        history.push(HistRec {
            id,
            str: str.to_string(),
        });
    });
}

/// Return the number of history entries for `id`.
///
/// `history_str(id, 0)` is the oldest; `history_str(id, count-1)` is the
/// newest.
#[must_use]
pub fn history_count(id: u8) -> usize {
    HISTORY.with(|h| h.borrow().iter().filter(|e| e.id == id).count())
}

/// Return the entry at `index` (oldest-first) for `id`, or `None` if out of
/// range.
#[must_use]
pub fn history_str(id: u8, index: usize) -> Option<String> {
    HISTORY.with(|h| {
        h.borrow()
            .iter()
            .filter(|e| e.id == id)
            .nth(index)
            .map(|e| e.str.clone())
    })
}

/// Remove all history entries for all ids.
pub fn clear_history() {
    HISTORY.with(|h| h.borrow_mut().clear());
}

// ---------------------------------------------------------------------------
// HistoryViewer — THistoryViewer (thstview.cpp, row 55)
// ---------------------------------------------------------------------------

/// `THistoryViewer` — a read-only single-column list over the global history
/// store, shown in a modal recall popup when a user drops down an input field.
///
/// Enter / double-click confirms (`endModal(cmOK)`); Esc / `cmCancel` dismisses
/// (`endModal(cmCancel)`). All other events fall through to the base
/// `TListViewer` nav.
///
/// # history_id type
///
/// C++ held `ushort historyId` but the store uses `uchar` (i.e. truncates at
/// the call boundary). Using `u8` throughout makes that truncation explicit and
/// avoids a silent aliasing bug.
///
/// # Setup after insertion
///
/// Call [`setup`](HistoryViewer::setup) after inserting the viewer into a group
/// (it needs a `Context` to publish the range and focus). This parallels how
/// `THistoryViewer::THistoryViewer` runs `setRange`/`focusItem`/hbar-range
/// inline in the C++ ctor where `Context` is always available.
///
/// # Palette / theme (provisional)
///
/// C++ `getPalette` returns `cpHistoryViewer "\x06\x06\x07\x06\x06"`, a
/// dialog-context recolor. rstv dropped palettes; `list_viewer::draw` uses
/// provisional `Role::List*` colors.
/// `TODO(row 34): cpHistoryViewer remap` — realign colors once row 34 gray
/// theming lands (same pattern as the menu/status breadcrumbs).
pub struct HistoryViewer {
    lv: ListViewerState,
    history_id: u8,
}

impl HistoryViewer {
    /// Construct a `HistoryViewer` — ports the data-init portion of
    /// `THistoryViewer::THistoryViewer`.
    ///
    /// `bounds`: the view rectangle; `h`/`v`: optional scrollbar ids;
    /// `history_id`: the store channel this viewer presents.  No `Context` is
    /// needed here (see [`setup`](Self::setup)).
    pub fn new(bounds: Rect, h: Option<ViewId>, v: Option<ViewId>, history_id: u8) -> Self {
        HistoryViewer {
            // 1 column: THistoryViewer always passes numCols=1.
            lv: ListViewerState::new(bounds, 1, h, v),
            history_id,
        }
    }

    /// Context-needing tail of the ctor — call once after insertion.
    ///
    /// Faithful to the C++ ctor body:
    /// 1. `setRange(historyCount(historyId))`
    /// 2. `if (range > 1) focusItem(1)` — the recall list shows the *most
    ///    recent* entry at item `count-1`, so item 1 (second-oldest) is the
    ///    default selection when more than one entry exists.
    /// 3. If an h-bar is wired, publish `setRange(0, historyWidth()-size.x+3)`.
    pub fn setup(&mut self, ctx: &mut Context) {
        let count = history_count(self.history_id) as i32;
        list_viewer::set_range(self, count, ctx);
        if self.lv.range > 1 {
            list_viewer::focus_item(self, 1, ctx);
        }
        if let Some(hbar) = self.lv.h_scroll_bar {
            let size_x = self.lv.state.size.x;
            let max = self.history_width() - size_x + 3;
            ctx.request_scroll_bar_params(hbar, None, Some(0), Some(max), None, None);
        }
    }

    /// Maximum display width over all entries for this channel.
    ///
    /// Faithful to `THistoryViewer::historyWidth()`: iterates the full channel
    /// and takes the max. Returns 0 for an empty channel.
    ///
    /// Note: this is O(n²)-ish — each `history_str(id, i)` re-filters the store
    /// from the front and clones a `String` just to measure it. That is fine
    /// for a recall list's tiny `n` (and matches the C++ `historyWidth` loop).
    /// The `.unwrap_or_default()` is defensive: `i` is always in `0..count`, so
    /// the `None` arm is effectively unreachable.
    fn history_width(&self) -> i32 {
        let id = self.history_id;
        (0..history_count(id))
            .map(|i| crate::text::width(&history_str(id, i).unwrap_or_default()) as i32)
            .max()
            .unwrap_or(0)
    }
}

impl ListViewer for HistoryViewer {
    fn lv(&self) -> &ListViewerState {
        &self.lv
    }

    fn lv_mut(&mut self) -> &mut ListViewerState {
        &mut self.lv
    }

    /// `THistoryViewer::getText` — return the store entry for `item`.
    ///
    /// Faithful: `historyStr(historyId, item)`. Negative items and out-of-range
    /// items return an empty string (C++ `*dest = EOS`).
    fn get_text(&self, item: i32) -> String {
        if item < 0 {
            return String::new();
        }
        history_str(self.history_id, item as usize).unwrap_or_default()
    }
    // is_selected / select_item: inherit the base (item == focused /
    // broadcast cmListItemSelected). THistoryViewer does NOT override these.
}

impl View for HistoryViewer {
    fn state(&self) -> &ViewState {
        &self.lv.state
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.lv.state
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        list_viewer::draw(self, ctx);
    }

    /// `THistoryViewer::handleEvent` — confirm or dismiss the modal recall list.
    ///
    /// Enter / double-click → `endModal(cmOK)`.
    /// Esc / `cmCancel`     → `endModal(cmCancel)`.
    /// Everything else      → `TListViewer::handleEvent` (nav, scrollbar sync…).
    ///
    /// **No `sfModal` gate**: the viewer only lives inside a `THistoryWindow`
    /// (always `execView`'d), so the endModal is unconditional. Faithful to the
    /// C++ `THistoryViewer::handleEvent` body.
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        match *ev {
            Event::MouseDown(me) if me.flags.double_click => {
                ctx.end_modal(Command::OK);
                ev.clear();
            }
            Event::KeyDown(k) if k.key == Key::Enter => {
                ctx.end_modal(Command::OK);
                ev.clear();
            }
            Event::KeyDown(k) if k.key == Key::Esc => {
                ctx.end_modal(Command::CANCEL);
                ev.clear();
            }
            Event::Command(c) if c == Command::CANCEL => {
                ctx.end_modal(Command::CANCEL);
                ev.clear();
            }
            _ => list_viewer::handle_event(self, ev, ctx),
        }
    }

    fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
        list_viewer::set_state(self, flag, enable, ctx);
    }

    fn cursor_request(&self) -> Option<Point> {
        list_viewer::focused_cursor(self)
    }

    fn apply_list_scroll(&mut self, h: Option<i32>, v: Option<i32>, ctx: &mut Context) {
        list_viewer::apply_scroll(self, h, v, ctx);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
    // value(): NOT overridden — HistoryViewer has no transferable value.
    // The selection is ephemeral (the caller uses the modal result command,
    // then reads the focused text from the store). No FieldValue variant added.
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Basic round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn add_count_str_round_trip() {
        clear_history();
        history_add(1, "first");
        history_add(1, "second");
        assert_eq!(history_count(1), 2);
        // oldest → newest
        assert_eq!(history_str(1, 0), Some("first".into()));
        assert_eq!(history_str(1, 1), Some("second".into()));
    }

    // -----------------------------------------------------------------------
    // Empty string is ignored
    // -----------------------------------------------------------------------

    #[test]
    fn empty_string_ignored() {
        clear_history();
        history_add(2, "");
        assert_eq!(history_count(2), 0);
        history_add(2, "real");
        history_add(2, "");
        assert_eq!(history_count(2), 1);
    }

    // -----------------------------------------------------------------------
    // Per-id isolation
    // -----------------------------------------------------------------------

    #[test]
    fn per_id_isolation() {
        clear_history();
        history_add(1, "alpha");
        history_add(2, "beta");
        assert_eq!(history_count(1), 1);
        assert_eq!(history_count(2), 1);
        assert_eq!(history_str(1, 0), Some("alpha".into()));
        assert_eq!(history_str(2, 0), Some("beta".into()));
        // id 1 cannot see id 2's entry
        assert_eq!(history_str(1, 1), None);
        assert_eq!(history_str(2, 1), None);
    }

    // -----------------------------------------------------------------------
    // Dedup moves entry to newest position
    //
    // Bite: a no-dedup implementation gives count==3 and order "a","b","a".
    // -----------------------------------------------------------------------

    #[test]
    fn dedup_moves_to_newest() {
        clear_history();
        history_add(3, "a");
        history_add(3, "b");
        history_add(3, "a"); // duplicate of first → moves to newest
        assert_eq!(history_count(3), 2, "duplicate must be collapsed");
        // "b" is now the older one, "a" is the newest
        assert_eq!(history_str(3, 0), Some("b".into()));
        assert_eq!(history_str(3, 1), Some("a".into()));
    }

    // -----------------------------------------------------------------------
    // Out-of-range index → None
    // -----------------------------------------------------------------------

    #[test]
    fn out_of_range_returns_none() {
        clear_history();
        history_add(4, "only");
        assert_eq!(history_str(4, 0), Some("only".into()));
        assert_eq!(history_str(4, 1), None);
        assert_eq!(history_str(4, 99), None);
        assert_eq!(history_str(4, 0), Some("only".into())); // unchanged after query
    }

    // -----------------------------------------------------------------------
    // Global byte-budget eviction across ids
    //
    // Design: fill with id=10 entries (each 50+3=53 bytes) until near-full,
    // then add an id=11 entry.  The oldest id=10 entry must be evicted first.
    //
    // We use `format!("{:050}", i)` to guarantee every string is exactly 50
    // bytes regardless of the number of decimal digits in `i`.
    //
    // Bite: a per-id budget model would evict from id=11's budget (empty) and
    // would refuse or evict from the wrong side.
    // -----------------------------------------------------------------------

    #[test]
    fn global_eviction_across_ids() {
        clear_history();
        // Each entry: format!("{:050}", i) → len=50, cost=53.
        // 19 × 53 = 1007 bytes — just under the 1024-byte limit.
        let make_entry = |i: u32| format!("{:050}", i);
        for i in 0..19u32 {
            history_add(10, &make_entry(i));
        }
        // Sanity: all 19 entries fit without eviction.
        assert_eq!(
            history_count(10),
            19,
            "19 × 53 = 1007 ≤ 1024, nothing evicted yet"
        );
        let oldest_id10 = make_entry(0);
        assert_eq!(
            history_str(10, 0),
            Some(oldest_id10.clone()),
            "oldest entry is index 0"
        );

        // Adding one id=11 entry (also 53 bytes) pushes total to 1007+53=1060 > 1024.
        // The globally-oldest entry (an id=10 entry) must be evicted to make room.
        let id11_entry = make_entry(999);
        history_add(11, &id11_entry);

        // id=11 entry must exist.
        assert_eq!(history_count(11), 1);
        assert_eq!(history_str(11, 0), Some(id11_entry));

        // The oldest id=10 entry was evicted (global FIFO, not per-id).
        assert_eq!(
            history_count(10),
            18,
            "one id=10 entry must have been evicted"
        );
        assert_ne!(
            history_str(10, 0),
            Some(oldest_id10),
            "oldest id=10 entry must have been evicted"
        );
    }

    // -----------------------------------------------------------------------
    // Dedup-before-evict: re-adding an existing string must not evict an
    // unrelated entry.
    //
    // Strategy:
    //   • Add 19 entries of cost 53 (len=50) under id=20: total 1007 bytes.
    //   • Add one "canary" entry of cost 17 (len=14): total 1024 bytes (full).
    //   • Re-add the newest of the 19 big entries (already in the store).
    //     – dedup removes it first: 1024-53 = 971 bytes.
    //     – new entry cost 53: 971+53 = 1024 ≤ 1024 → no eviction triggered.
    //   • Assert canary still present (not evicted as collateral).
    //
    // Bite: without dedup-before-evict the store would be at 1024 bytes before
    // the duplicate is removed, triggering an eviction of the oldest entry.
    // -----------------------------------------------------------------------

    #[test]
    fn dedup_before_evict_no_collateral_eviction() {
        clear_history();
        // 19 entries of len=50 (cost=53) — always exactly 50 bytes via {:050}.
        // 19 × 53 = 1007 bytes.
        let make_big = |i: u32| format!("{:050}", i);
        for i in 0..19u32 {
            history_add(20, &make_big(i));
        }

        // Canary: len=14, cost=17 → total 1007+17=1024 (exactly full).
        let canary: String = "c".repeat(14);
        history_add(20, &canary);
        assert_eq!(history_count(20), 20, "20 entries, 1024 bytes");

        // Re-add the newest big entry (make_big(18), already at back of store).
        // dedup removes it first: 1024-53 = 971 bytes.
        // Re-inserting costs 53: 971+53 = 1024 ≤ 1024 → no eviction triggered.
        let newest_big = make_big(18);
        history_add(20, &newest_big);

        // Count must remain 20: dedup freed one slot, re-insert fills it, no net eviction.
        assert_eq!(
            history_count(20),
            20,
            "count must remain 20 — no collateral eviction"
        );

        // Canary must still be present.
        let found_canary =
            (0..history_count(20)).any(|i| history_str(20, i) == Some(canary.clone()));
        assert!(
            found_canary,
            "canary must not have been evicted as collateral damage"
        );
    }

    // -----------------------------------------------------------------------
    // clear_history empties all ids
    // -----------------------------------------------------------------------

    #[test]
    fn clear_empties_all_ids() {
        clear_history();
        history_add(50, "foo");
        history_add(51, "bar");
        assert_eq!(history_count(50), 1);
        assert_eq!(history_count(51), 1);
        clear_history();
        assert_eq!(history_count(50), 0);
        assert_eq!(history_count(51), 0);
        assert_eq!(history_str(50, 0), None);
        assert_eq!(history_str(51, 0), None);
    }
}

// ---------------------------------------------------------------------------
// HistoryViewer tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod viewer_tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::event::{
        KeyEvent, KeyModifiers, MouseButtons, MouseEvent, MouseEventFlags, MouseWheel,
    };
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

    // -----------------------------------------------------------------------
    // get_text — valid, oob, negative
    // -----------------------------------------------------------------------

    #[test]
    fn get_text_valid_oob_negative() {
        clear_history();
        history_add(60, "first");
        history_add(60, "second");
        let hv = HistoryViewer::new(Rect::new(0, 0, 20, 8), None, None, 60);
        // In-range: must return store strings (bite: distinguishes from empty).
        let t0 = hv.get_text(0);
        assert_eq!(t0, "first");
        assert_ne!(t0, "", "in-range item is not empty (bite)");
        assert_eq!(hv.get_text(1), "second");
        // Out-of-range returns empty.
        assert_eq!(hv.get_text(2), "");
        assert_eq!(hv.get_text(99), "");
        // Negative items return empty.
        assert_eq!(hv.get_text(-1), "");
        assert_eq!(hv.get_text(-100), "");
    }

    // -----------------------------------------------------------------------
    // history_width — max, not min/first (bite)
    // -----------------------------------------------------------------------

    #[test]
    fn history_width_is_max_not_min_or_first() {
        clear_history();
        history_add(61, "hi"); // width 2
        history_add(61, "medium"); // width 6
        history_add(61, "longest"); // width 7 — must be the result
        let hv = HistoryViewer::new(Rect::new(0, 0, 20, 8), None, None, 61);
        let w = hv.history_width();
        assert_eq!(w, 7, "history_width = max (7), not first (2) or min (2)");
    }

    // -----------------------------------------------------------------------
    // setup: range > 1 focuses item 1; range <= 1 leaves focus 0
    // -----------------------------------------------------------------------

    #[test]
    fn setup_range_gt_1_focuses_item_1() {
        clear_history();
        history_add(62, "a");
        history_add(62, "b");
        history_add(62, "c");
        let mut hv = HistoryViewer::new(Rect::new(0, 0, 20, 8), None, None, 62);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            hv.setup(&mut ctx);
        }
        assert_eq!(hv.lv.range, 3, "range set to history count");
        assert_eq!(hv.lv.focused, 1, "focused == 1 when range > 1");
    }

    #[test]
    fn setup_range_le_1_leaves_focus_0() {
        clear_history();
        history_add(63, "only");
        let mut hv = HistoryViewer::new(Rect::new(0, 0, 20, 8), None, None, 63);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            hv.setup(&mut ctx);
        }
        assert_eq!(hv.lv.range, 1, "range == 1");
        assert_eq!(hv.lv.focused, 0, "focused stays 0 when range == 1");
    }

    #[test]
    fn setup_empty_history_leaves_focus_0() {
        clear_history();
        // No entries added for id 64.
        let mut hv = HistoryViewer::new(Rect::new(0, 0, 20, 8), None, None, 64);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            hv.setup(&mut ctx);
        }
        assert_eq!(hv.lv.range, 0, "range == 0 for empty history");
        assert_eq!(hv.lv.focused, 0, "focused stays 0 for empty history");
    }

    // -----------------------------------------------------------------------
    // setup: with an h-bar, publishes setRange(0, historyWidth - size_x + 3)
    //
    // This exercises the only non-trivial arithmetic in `setup` (the hbar
    // branch), which all the None-bar tests skip.
    // -----------------------------------------------------------------------

    #[test]
    fn setup_with_hbar_publishes_history_width_range() {
        clear_history();
        // Known widths: "abcde" → 5, "ab" → 2. historyWidth = max = 5.
        history_add(80, "abcde");
        history_add(80, "ab");

        // Mint a real ViewId for the h-bar (mirror list_box's vbar-minting test).
        let mut mint_group = Group::new(Rect::new(0, 0, 4, 4));
        let hbar = mint_group.insert(Box::new(HistoryViewer::new(
            Rect::new(0, 0, 1, 1),
            None,
            None,
            80,
        )));

        // size.x = 20 from the bounds. EXPECTED = historyWidth - size.x + 3
        //                                       = 5 - 20 + 3 = -12.
        let mut hv = HistoryViewer::new(Rect::new(0, 0, 20, 8), Some(hbar), None, 80);
        assert_eq!(hv.lv.state.size.x, 20, "size.x derived from bounds width");
        let expected_max = 5 - 20 + 3; // -12

        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            hv.setup(&mut ctx);
        }

        // The hbar range publication carries the exact computed max (bite: a
        // wrong formula yields a different value).
        assert!(
            deferred.iter().any(|x| matches!(
                x,
                Deferred::ScrollBarSetParams {
                    id,
                    value: None,
                    min: Some(0),
                    max: Some(m),
                    page_step: None,
                    arrow_step: None,
                } if *id == hbar && *m == expected_max
            )),
            "setup must queue hbar setRange(0, {expected_max})"
        );

        // Sanity: set_range also published v-bar-less range work? With no v-bar
        // wired, set_range queues nothing for the v-bar, but it still ran
        // (range was set). focus_item(1) requires range > 1 (range == 2 here).
        assert_eq!(hv.lv.range, 2, "range set to history count");
        assert_eq!(hv.lv.focused, 1, "range > 1 → focus item 1");
    }

    // -----------------------------------------------------------------------
    // handle_event: Enter and double-click → EndModal(OK)
    // -----------------------------------------------------------------------

    #[test]
    fn enter_queues_end_modal_ok() {
        clear_history();
        history_add(65, "item");
        let mut hv = HistoryViewer::new(Rect::new(0, 0, 20, 8), None, None, 65);
        hv.lv.range = 1;
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let mut ev = key_ev(Key::Enter);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            hv.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "Enter consumed");
        assert!(
            deferred
                .iter()
                .any(|x| matches!(x, Deferred::EndModal(Command::OK))),
            "Enter queues EndModal(OK)"
        );
    }

    #[test]
    fn double_click_queues_end_modal_ok() {
        clear_history();
        history_add(66, "item");
        let mut hv = HistoryViewer::new(Rect::new(0, 0, 20, 8), None, None, 66);
        hv.lv.range = 1;
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let flags = MouseEventFlags {
            double_click: true,
            ..Default::default()
        };
        let me = MouseEvent {
            position: crate::view::Point { x: 0, y: 0 },
            flags,
            buttons: MouseButtons::default(),
            wheel: MouseWheel::None,
            modifiers: KeyModifiers::default(),
        };
        let mut ev = Event::MouseDown(me);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            hv.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "double-click consumed");
        assert!(
            deferred
                .iter()
                .any(|x| matches!(x, Deferred::EndModal(Command::OK))),
            "double-click queues EndModal(OK)"
        );
    }

    // -----------------------------------------------------------------------
    // handle_event: Esc → EndModal(CANCEL)
    // -----------------------------------------------------------------------

    #[test]
    fn esc_queues_end_modal_cancel() {
        clear_history();
        history_add(67, "item");
        let mut hv = HistoryViewer::new(Rect::new(0, 0, 20, 8), None, None, 67);
        hv.lv.range = 1;
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let mut ev = key_ev(Key::Esc);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            hv.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "Esc consumed");
        assert!(
            deferred
                .iter()
                .any(|x| matches!(x, Deferred::EndModal(Command::CANCEL))),
            "Esc queues EndModal(CANCEL)"
        );
    }

    // -----------------------------------------------------------------------
    // handle_event: Command(CANCEL) → EndModal(CANCEL)
    // -----------------------------------------------------------------------

    #[test]
    fn command_cancel_queues_end_modal_cancel() {
        clear_history();
        history_add(68, "item");
        let mut hv = HistoryViewer::new(Rect::new(0, 0, 20, 8), None, None, 68);
        hv.lv.range = 1;
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let mut ev = Event::Command(Command::CANCEL);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            hv.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "Command(CANCEL) consumed");
        assert!(
            deferred
                .iter()
                .any(|x| matches!(x, Deferred::EndModal(Command::CANCEL))),
            "Command(CANCEL) queues EndModal(CANCEL)"
        );
    }

    // -----------------------------------------------------------------------
    // handle_event: Down-arrow does NOT queue EndModal (falls through to base)
    // -----------------------------------------------------------------------

    #[test]
    fn down_arrow_no_end_modal_falls_through() {
        clear_history();
        history_add(69, "a");
        history_add(69, "b");
        history_add(69, "c");
        let mut hv = HistoryViewer::new(Rect::new(0, 0, 20, 8), None, None, 69);
        hv.lv.range = 3;
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let mut ev = key_ev(Key::Down);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            hv.handle_event(&mut ev, &mut ctx);
        }
        // The Down arrow must NOT produce an EndModal (bite: a broken dispatch
        // that catches all events would do so).
        assert!(
            !deferred.iter().any(|x| matches!(x, Deferred::EndModal(_))),
            "Down-arrow must not queue EndModal (falls through to base nav)"
        );
        // The base nav should have moved focus.
        assert_eq!(hv.lv.focused, 1, "Down-arrow wired: focused moves to 1");
    }

    // -----------------------------------------------------------------------
    // Snapshot: 3 entries, setup, item 1 focused
    // -----------------------------------------------------------------------

    fn render_viewer(hv: &mut HistoryViewer, w: u16, h: u16) -> String {
        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(w, h);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = hv.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            hv.draw(&mut dc);
        });
        screen.snapshot()
    }

    #[test]
    fn snapshot_history_viewer_item1_focused() {
        clear_history();
        history_add(70, "oldest");
        history_add(70, "middle");
        history_add(70, "newest");

        let mut hv = HistoryViewer::new(Rect::new(0, 0, 14, 5), None, None, 70);
        // Activate so the focused row renders in the distinct focused color.
        hv.lv.state.state.selected = true;
        hv.lv.state.state.active = true;

        // Call setup — None bars means the hbar block is skipped and no ViewId
        // resolution is needed, so the deferred queue stays empty.
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            hv.setup(&mut ctx);
        }
        // range == 3 > 1, so setup focused item 1.
        assert_eq!(hv.lv.focused, 1, "setup focused item 1 (range > 1 path)");

        insta::assert_snapshot!(render_viewer(&mut hv, 14, 5));
    }
}
