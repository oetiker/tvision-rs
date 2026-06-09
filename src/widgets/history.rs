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
use crate::window::{ScrollBarOptions, Window, WindowFlags};
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

impl HistoryViewer {
    /// `THistoryWindow::getSelection` reads `viewer->getText(viewer->focused)`.
    ///
    /// This accessor is on `HistoryViewer` (not exposed to the crate root) so
    /// `HistoryWindow::get_selection` can reach `lv.focused` without making the
    /// field public. `lv` is private to this module, but `HistoryWindow` lives
    /// in the same file, so the private-field access is allowed directly there.
    /// This accessor provides the clean named path.
    ///
    /// First production consumer: row 57 (the code that calls `exec_view` on a
    /// `HistoryWindow` and reads the selection after the modal returns).
    pub(crate) fn selection(&self) -> String {
        <Self as ListViewer>::get_text(self, self.lv.focused)
    }
}

// ---------------------------------------------------------------------------
// HistoryWindow — THistoryWindow (thistwin.cpp, row 56)
// ---------------------------------------------------------------------------

/// `THistoryWindow` — the modal window hosting a [`HistoryViewer`] recall list.
///
/// A `TWindow` subtype (`wfClose` only — not movable) that assembles two scroll
/// bars and the viewer, then runs modally so the caller can read
/// [`get_selection`](HistoryWindow::get_selection) after `exec_view` returns.
///
/// # Deviations from C++
///
/// * `THistInit`/`TWindowInit` constructor-init indirection is moot (D12) —
///   `initViewer` is inlined.
/// * `createListViewer` hook (streamability, D12) — inlined with no substitution
///   path.
/// * `getPalette` returns `cpHistoryWindow "\x13\x13\x15\x18\x17\x13\x14"`.
///   We have no live palette mapping; the window uses the default `Window`/`Frame`
///   rendering.  `TODO(row 34): cpHistoryWindow palette remap`.
/// * `evMouseDown && !mouseInView → endModal(cmCancel)` is not ported —
///   see `handle_event` for the breadcrumb.
pub struct HistoryWindow {
    /// The embedded window (D2). `HistoryWindow` *is-a* `TWindow`.
    window: Window,
    /// The `HistoryViewer` child's id — resolved after construction for
    /// `setup` and `get_selection`.
    viewer_id: ViewId,
    /// Tracks whether the viewer's post-insert `setup` has been run.
    /// `setup` needs a live `Context`; it runs on the first `handle_event` call
    /// (the Context-free-ctor deviation established by row 55/ListBox).
    setup_done: bool,
}

impl HistoryWindow {
    /// `THistoryWindow::THistoryWindow(bounds, historyId)` + inlined
    /// `initViewer`.
    ///
    /// Faithful to the C++:
    /// 1. `TWindow(bounds, 0 /*title*/, wnNoNumber)`.
    /// 2. `flags = wfClose` — close box only; NOT move/grow/zoom.
    /// 3. `initViewer`: `r.grow(-1,-1)`, build h-bar and v-bar (in that order,
    ///    matching C++ evaluation order), build `HistoryViewer(r, hbar, vbar)`,
    ///    insert into the group.
    pub fn new(bounds: Rect, history_id: u8) -> Self {
        // (1) Window(bounds, NULL title, wnNoNumber).
        let mut window = Window::new(bounds, None, 0);
        // (2) flags = wfClose.
        window.set_flags(WindowFlags {
            close: true,
            ..WindowFlags::default()
        });
        // (3) initViewer inlined: r = getExtent(); r.grow(-1, -1).
        let mut r = View::state(&window).get_extent();
        r.grow(-1, -1);

        // Build the two bars (ORDER MATTERS — C++ evaluates h-bar arg first,
        // then v-bar; both are inserted into the window group).
        let h = window.standard_scroll_bar(ScrollBarOptions {
            vertical: false,
            handle_keyboard: true,
        });
        let v = window.standard_scroll_bar(ScrollBarOptions {
            vertical: true,
            handle_keyboard: true,
        });

        // Build and insert the viewer.
        let viewer = HistoryViewer::new(r, Some(h), Some(v), history_id);
        let viewer_id = window.insert_child(Box::new(viewer));

        HistoryWindow {
            window,
            viewer_id,
            setup_done: false,
        }
    }

    /// `THistoryWindow::getSelection` — the viewer's focused entry text.
    ///
    /// Uses `&mut self` because `child_mut` / `as_any_mut` require `&mut`.
    /// C++ `getSelection` is non-const for the same reason. The modal result
    /// read happens after the loop completes (row 57), so `&mut` is faithful.
    /// If the downcast somehow fails (unreachable in practice — the viewer_id
    /// always resolves to a `HistoryViewer`), returns an empty string.
    ///
    /// First production consumer: row 57 (the caller of `exec_view` reads the
    /// selection after the modal returns).
    pub(crate) fn get_selection(&mut self) -> String {
        self.window
            .child_mut(self.viewer_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<HistoryViewer>())
            .map(|hv| hv.selection())
            .unwrap_or_default()
    }
}

#[crate::delegate(
    to = window,
    skip(
        apply_list_scroll,
        calc_bounds,
        grabs_focus_on_click,
        select_window_num,
        set_value,
        value
    )
)]
impl View for HistoryWindow {
    /// Downcast hook so the row-57 modal completion can downcast the modal
    /// `dyn View` back to `HistoryWindow` and read [`get_selection`](Self::get_selection).
    /// Must be a real `Some(self)` — delegating to `window.as_any_mut()` would
    /// downcast to a `Window`, returning `None` for the `HistoryWindow` downcast
    /// (a silent pick-nothing). NOT in the `skip(...)` list for that reason.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    /// `THistoryWindow::handleEvent` — faithful order: setup guard → delegate
    /// to `TWindow::handleEvent`.
    ///
    /// (A) **One-time viewer setup BEFORE delegating** — the event then reaches
    ///     a ready viewer (range/focused initialized). This is the
    ///     Context-free-ctor deviation row 55/ListBox established: `setup()`
    ///     needs a live `Context`, so it lands post-insert, here, on the first
    ///     event.
    ///
    /// (B) `TWindow::handleEvent` (faithful order: base first).
    ///
    /// (C) **DEFERRED**: see the `TODO(row 57 modal-loop seam)` breadcrumb.
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        // (A) One-time viewer setup BEFORE delegating — ensures range/focused
        // are initialized before the first event reaches the viewer.
        if !self.setup_done {
            if let Some(v) = self.window.child_mut(self.viewer_id)
                && let Some(hv) = v
                    .as_any_mut()
                    .and_then(|a| a.downcast_mut::<HistoryViewer>())
            {
                hv.setup(ctx);
            }
            // Establish the popup's internal currency so the FIRST focused event
            // (even an immediate Esc/Enter, with no prior nav) routes to the viewer
            // → its endModal fires.
            //
            // Baseline → Deviation → Integration: C++ establishes a view's currency
            // at OPEN, via `insertView → show → resetCurrent` (the inserted view's
            // group selects its first selectable child). rstv's `Group::insert` takes
            // no `Context` and so cannot run `reset_current` at open (the foundational
            // initial-currency gap breadcrumbed at `Program::exec_view`). DEVIATION:
            // we establish currency at FIRST-EVENT instead of open — the SAME
            // already-accepted deviation class as the viewer's `setup()` running on
            // first event (row 55/ListBox Context-free-ctor constraint). INTEGRATION:
            // because this guard runs BEFORE delegating to `TWindow::handleEvent`, the
            // window's `current` is set in time for this very event; `route_event`'s
            // focused phase delivers to `current` by index with no focused-flag gate.
            self.window.select_child(self.viewer_id, ctx);
            self.setup_done = true;
        }
        // (B) TWindow::handleEvent (faithful order: base first).
        self.window.handle_event(ev, ctx);
        // (C) Outside-click cancel — C++ THistoryWindow::handleEvent:
        //   if (event.what == evMouseDown && !mouseInView(event.mouse.where))
        //       endModal(cmCancel);
        // The pump delivers outside clicks to us with the position already localized
        // (subtracted modal_bounds.a), so !mouseInView == !extent.contains(position).
        if let Event::MouseDown(m) = ev
            && !View::state(self).get_extent().contains(m.position)
        {
            ctx.end_modal(Command::CANCEL);
            ev.clear();
        }
    }
}

// ---------------------------------------------------------------------------
// THistory — the dropdown-arrow icon next to a TInputLine (thistory.cpp, row 57)
// ---------------------------------------------------------------------------

/// `THistory` — the dropdown-arrow icon placed next to a [`InputLine`](crate::widgets::InputLine).
///
/// On its trigger (a click, or Ctrl/↓ while the linked input is focused) it opens
/// a modal [`HistoryWindow`] over the channel's history, and on **OK** writes the
/// picked string back into the linked input line. This is the first consumer of
/// the **view-triggered async-modal seam** ([`Deferred::OpenHistory`](crate::view::Deferred::OpenHistory)):
/// a `THistory` leaf holds only the link's [`ViewId`] (D3) and cannot call
/// `exec_view` (top-level only), so it **requests** the open and the pump builds +
/// drives the modal.
///
/// # Deviations from C++
///
/// * **focus-abort OUT.** C++ `THistory::handleEvent` aborts the open if
///   `link->focus()` fails (`if (!link->focus()) { clearEvent; return; }`). Our
///   focus is deferred ([`focus_descendant`](crate::view::View::focus_descendant))
///   with no inline success bool, so we request focus and proceed — the open path
///   (in the pump's `OpenHistory` arm) documents this (same class as the row-39/41
///   deferred-focus TODOs).
/// * **`shutDown` (`link = 0`)** is moot — the link is a [`ViewId`], not an owning
///   pointer, so there is nothing to null out (D3).
/// * **palette** — C++ `getPalette` returns `cpHistory "\x16\x17"`, a
///   dialog-context recolor; rstv dropped palettes, so the icon reuses the
///   provisional `Role::Input*` colors (it sits next to an input line).
///   `TODO(row 34): cpHistory palette remap` — same pattern as rows 55/56.
pub struct THistory {
    state: ViewState,
    /// The linked input line's id (`link`).
    link: ViewId,
    /// The history channel id (`historyId`).
    history_id: u8,
}

impl THistory {
    /// `THistory(bounds, aLink, aHistoryId)` — `options |= ofPostProcess`.
    ///
    /// `selectable` stays `false` (the [`ViewState`] default), so a click delivers
    /// to the icon without grabbing focus — faithful: `THistory` is never
    /// `current`. `eventMask |= evBroadcast` is **moot** ([`Group`](crate::view::Group)
    /// fans broadcasts to all children regardless — handover row 49).
    pub fn new(bounds: Rect, link: ViewId, history_id: u8) -> Self {
        let mut state = ViewState::new(bounds);
        // ofPostProcess — the icon gets keyDowns via the postProcess phase, AFTER
        // the focused input line (which leaves the ↓ arrow live + uncleared).
        state.options.post_process = true;
        THistory {
            state,
            link,
            history_id,
        }
    }
}

impl View for THistory {
    fn state(&self) -> &ViewState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }

    /// `THistory::draw` — `b.moveCStr(0, icon, getColor(0x0102))`.
    ///
    /// The C++ icon is `"\xDE~\x19~\xDD"`: `▐` (U+2590) + a highlighted `↓`
    /// (U+2193, `\x19`) + `▌` (U+258C), where the `~…~` marks the hi region (the
    /// arrow). `getColor(0x0102)` → lo = palette[1], hi = palette[2]. We render the
    /// cstr `"▐~↓~▌"` with lo = `Role::InputNormal`, hi = `Role::InputArrow`.
    fn draw(&mut self, ctx: &mut DrawCtx) {
        let lo = ctx.style(crate::theme::Role::InputNormal);
        let hi = ctx.style(crate::theme::Role::InputArrow);
        ctx.put_cstr(0, 0, "\u{2590}~\u{2193}~\u{258C}", lo, hi);
    }

    /// `THistory::handleEvent` — open the modal on a trigger, or record history on
    /// the broadcast arm. Faithful to the C++ (base `TView::handleEvent` is a no-op
    /// here under D3, so we match the trigger directly):
    ///
    /// * **mouse-down**: open (mouse trigger never gates on focus).
    /// * **keyDown where `ctrlToArrow(keyCode) == kbDown`**: open, gated on the link
    ///   being focused (`(link->state & sfFocused)`). `ctrl_to_arrow` returns the
    ///   event UNCHANGED when not Ctrl, so `.key == Key::Down` matches BOTH the
    ///   literal ↓ AND Ctrl+X; modifiers are cleared on a mapped result, so we
    ///   compare `.key` only.
    /// * **broadcast `cmReleasedFocus`(source == link) / `cmRecordHistory`**:
    ///   `recordHistory(link->data)`; C++ does NOT clearEvent here — left live.
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        match ev {
            Event::MouseDown(_) => {
                ctx.request_open_history(self.link, self.history_id, false);
                ev.clear();
            }
            Event::KeyDown(k) if crate::event::ctrl_to_arrow(*k).key == crate::event::Key::Down => {
                ctx.request_open_history(self.link, self.history_id, true);
                // Baseline → Deviation: C++ keeps the ↓ live when the link is NOT
                // focused — its `clearEvent` sits INSIDE the `(link->state &
                // sfFocused)` guard. DEVIATION: we clear unconditionally. This is
                // D3-forced — the leaf cannot read the link's focus inline (it only
                // holds the link's id), so the focus gate is applied later in the
                // pump's `OpenHistory` arm; clear-always is the correct horn
                // (clear-never would let a focused-link ↓ be double-handled).
                ev.clear();
            }
            Event::Broadcast { command, source }
                if (*command == Command::RELEASED_FOCUS && *source == Some(self.link))
                    || *command == Command::RECORD_HISTORY =>
            {
                ctx.request_record_history(self.link, self.history_id);
                // C++ does not clearEvent in the broadcast arm — leave it live.
            }
            _ => {}
        }
    }
    // value/set_value: trait default (THistory has no transferable value).
}

// ---------------------------------------------------------------------------
// THistory tests (row 57)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod thistory_tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::event::{KeyEvent, KeyModifiers, MouseButtons, MouseEvent};
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

    fn key_ev(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(k, KeyModifiers::default()))
    }

    fn mouse_down() -> Event {
        Event::MouseDown(MouseEvent {
            position: Point::new(0, 0),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        })
    }

    /// Mint a real link id (so the THistory has a resolvable link).
    fn mint_link() -> ViewId {
        let mut g = Group::new(Rect::new(0, 0, 4, 4));
        g.insert(Box::new(HistoryViewer::new(
            Rect::new(0, 0, 1, 1),
            None,
            None,
            0,
        )))
    }

    // -- mouse trigger queues OpenHistory(require_focus = false) -------------
    #[test]
    fn mouse_down_queues_open_history_no_focus_gate() {
        let link = mint_link();
        let mut h = THistory::new(Rect::new(0, 0, 3, 1), link, 5);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let mut ev = mouse_down();
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            h.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "mouse-down consumed");
        assert!(
            deferred.iter().any(|x| matches!(
                x,
                Deferred::OpenHistory { link: l, history_id: 5, require_focus: false } if *l == link
            )),
            "mouse-down queues OpenHistory(require_focus=false)"
        );
    }

    // -- ▼ queues OpenHistory(require_focus = true) --------------------------
    //
    // ctrl_to_arrow returns the literal Down unchanged, so `.key == Down` matches.
    #[test]
    fn down_arrow_queues_open_history_with_focus_gate() {
        let link = mint_link();
        let mut h = THistory::new(Rect::new(0, 0, 3, 1), link, 6);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let mut ev = key_ev(Key::Down);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            h.handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "▼ consumed");
        assert!(
            deferred.iter().any(|x| matches!(
                x,
                Deferred::OpenHistory { link: l, history_id: 6, require_focus: true } if *l == link
            )),
            "▼ queues OpenHistory(require_focus=true)"
        );
    }

    // -- Ctrl+X maps to Down → also triggers (ctrl_to_arrow) -----------------
    #[test]
    fn ctrl_x_maps_to_down_and_triggers() {
        let link = mint_link();
        let mut h = THistory::new(Rect::new(0, 0, 3, 1), link, 6);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let mut ev = Event::KeyDown(KeyEvent::new(
            Key::Char('x'),
            KeyModifiers {
                ctrl: true,
                ..Default::default()
            },
        ));
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            h.handle_event(&mut ev, &mut ctx);
        }
        assert!(
            deferred.iter().any(|x| matches!(
                x,
                Deferred::OpenHistory {
                    require_focus: true,
                    ..
                }
            )),
            "Ctrl+X (→ Down via ctrl_to_arrow) triggers the open"
        );
    }

    // -- a non-trigger key is ignored (left live, no deferred) ---------------
    #[test]
    fn unrelated_key_ignored() {
        let link = mint_link();
        let mut h = THistory::new(Rect::new(0, 0, 3, 1), link, 6);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let mut ev = key_ev(Key::Up);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            h.handle_event(&mut ev, &mut ctx);
        }
        assert!(
            !ev.is_nothing(),
            "a non-trigger key is left live (not consumed)"
        );
        assert!(
            deferred.is_empty(),
            "no deferred request for a non-trigger key"
        );
    }

    // -- broadcast arm: cmReleasedFocus(source==link) / cmRecordHistory ------
    #[test]
    fn broadcast_record_history_arm() {
        let link = mint_link();
        let other = mint_link();
        let mut h = THistory::new(Rect::new(0, 0, 3, 1), link, 9);
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        // cmReleasedFocus on the link → record.
        let mut ev = Event::Broadcast {
            command: Command::RELEASED_FOCUS,
            source: Some(link),
        };
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            h.handle_event(&mut ev, &mut ctx);
        }
        assert!(
            deferred.iter().any(|x| matches!(
                x,
                Deferred::RecordHistory { link: l, history_id: 9 } if *l == link
            )),
            "cmReleasedFocus(source==link) queues RecordHistory"
        );
        // C++ does not clearEvent in the broadcast arm — left live.
        assert!(!ev.is_nothing(), "broadcast arm does not clear the event");

        // cmReleasedFocus on ANOTHER view → no record (source filter).
        deferred.clear();
        let mut ev2 = Event::Broadcast {
            command: Command::RELEASED_FOCUS,
            source: Some(other),
        };
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            h.handle_event(&mut ev2, &mut ctx);
        }
        assert!(
            deferred.is_empty(),
            "cmReleasedFocus on another view is filtered out (source mismatch)"
        );

        // cmRecordHistory (source ignored) → record.
        let mut ev3 = Event::Broadcast {
            command: Command::RECORD_HISTORY,
            source: None,
        };
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            h.handle_event(&mut ev3, &mut ctx);
        }
        assert!(
            deferred
                .iter()
                .any(|x| matches!(x, Deferred::RecordHistory { history_id: 9, .. })),
            "cmRecordHistory queues RecordHistory regardless of source"
        );
    }

    // -- draw snapshot: the ▐↓▌ icon -----------------------------------------
    fn render_history(h: &mut THistory, w: u16, ht: u16) -> String {
        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(w, ht);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = h.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            h.draw(&mut dc);
        });
        screen.snapshot()
    }

    #[test]
    fn snapshot_history_icon() {
        let link = mint_link();
        let mut h = THistory::new(Rect::new(0, 0, 3, 1), link, 1);
        insta::assert_snapshot!(render_history(&mut h, 3, 1));
    }
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

// ---------------------------------------------------------------------------
// HistoryWindow tests (row 56)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod window_tests {
    use super::*;
    use crate::app::Program;
    use crate::backend::{HeadlessBackend, HeadlessHandle};
    use crate::command::Command;
    use crate::desktop::Desktop;
    use crate::event::{Event, Key, KeyEvent, KeyModifiers, MouseButtons, MouseEvent};
    use crate::theme::Theme;
    use crate::timer::ManualClock;
    use crate::view::{Deferred, Point, Rect, View};
    use std::rc::Rc;

    fn key_ev(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(k, KeyModifiers::default()))
    }

    /// Build a `Program` with a real desktop over a headless 80×25 backend.
    /// Returns the program and the headless handle (for injecting events via
    /// `HeadlessHandle::push_event`).
    fn make_program() -> (Program, HeadlessHandle) {
        let (backend, handle) = HeadlessBackend::new(80, 25);
        let theme = Theme::classic_blue();
        let clock = Rc::new(ManualClock::new(0));
        let program = Program::new(
            Box::new(backend),
            Box::new(clock),
            theme,
            |r| {
                Some(Box::new(Desktop::new(r, |r2| {
                    Some(Desktop::init_background(r2))
                })))
            },
            |_r| None,
            |_r| None,
        );
        (program, handle)
    }

    // -----------------------------------------------------------------------
    // Test 1: Construction — viewer_id resolves to a HistoryViewer
    // -----------------------------------------------------------------------

    #[test]
    fn construction_viewer_id_resolves() {
        clear_history();
        history_add(100, "first");
        history_add(100, "second");

        let mut hw = HistoryWindow::new(Rect::new(0, 0, 40, 15), 100);

        // viewer_id must resolve to a HistoryViewer via child_mut + downcast.
        let found = hw
            .window
            .child_mut(hw.viewer_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<HistoryViewer>())
            .is_some();
        assert!(
            found,
            "viewer_id resolves to a HistoryViewer via child_mut + downcast"
        );
    }

    // -----------------------------------------------------------------------
    // Test 2: Keyboard routes to the viewer after first-event setup
    //
    // Strategy: pre-queue a Down-arrow then Enter via the headless handle, run
    // exec_view. The viewer starts at focused=1 (3 entries → range=3, setup
    // focuses item 1). A Down moves to focused=2, Enter ends the modal OK.
    // -----------------------------------------------------------------------

    #[test]
    fn keyboard_routes_to_viewer_after_setup() {
        clear_history();
        history_add(101, "oldest");
        history_add(101, "middle");
        history_add(101, "newest");

        let (mut program, handle) = make_program();
        // Pre-queue via the headless handle: Down moves focused 1→2, Enter ends modal.
        handle.push_event(key_ev(Key::Down));
        handle.push_event(key_ev(Key::Enter));

        let hw = HistoryWindow::new(Rect::new(5, 3, 45, 18), 101);
        let result = program.exec_view(Box::new(hw));

        assert_eq!(result, Command::OK, "Enter ends modal with OK");
    }

    // -----------------------------------------------------------------------
    // Test 3: get_selection returns the focused entry text
    //
    // Seed 3 entries; setup focuses item 1. Assert get_selection == get_text(1).
    // We run setup directly via handle_event so we can read the field before
    // dismissing the modal (avoids the exec_view post-remove problem).
    // -----------------------------------------------------------------------

    #[test]
    fn get_selection_returns_focused_text() {
        clear_history();
        history_add(102, "alpha");
        history_add(102, "beta");
        history_add(102, "gamma");

        let mut hw = HistoryWindow::new(Rect::new(0, 0, 40, 15), 102);

        // Run setup by calling handle_event once directly with a throwaway Context.
        let mut out = std::collections::VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = crate::view::Context::new(&mut out, &mut timers, 0, &mut deferred);
            // A harmless broadcast triggers setup without consuming any real nav.
            let mut ev = Event::Broadcast {
                command: Command::SCROLL_BAR_CHANGED,
                source: None,
            };
            hw.handle_event(&mut ev, &mut ctx);
        }

        // After setup with 3 entries: focused = 1 (range > 1 → focusItem(1)).
        // get_selection must return get_text(focused=1).
        let expected = history_str(102, 1).unwrap_or_default();
        let actual = hw.get_selection();
        assert_eq!(
            actual, expected,
            "get_selection returns get_text(focused=1): expected {expected:?}, got {actual:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Test 4: Setup-guard ordering BITE (TRUE discriminator)
    //
    // The BITE mechanism:
    //   Seed 3 entries → setup will set range=3 and focused=1.
    //   Make the viewer the CURRENT child of the window so a Down key routes
    //   to it (not to the v-bar). Then call handle_event with a Down key.
    //
    //   With guard BEFORE window.handle_event (CORRECT ORDER):
    //     setup runs first → range=3, focused=1 → Down reaches viewer with
    //     initialized range → focus_item_num(1+1=2) succeeds → focused=2.
    //
    //   With guard AFTER window.handle_event (MISORDERED — the failing case):
    //     Down reaches viewer BEFORE setup → range=0 → focus_item_num(0+1=1)
    //     clamps to range-1=0 (no-op, range=0 means no items) → focused stays 0
    //     → then setup sets focused=1 → final value is 1, not 2.
    //
    //   assert_eq!(focused, 2) passes iff the guard is BEFORE delegation.
    //   If the guard is misordered, the assertion fails with focused==1.
    //
    // VERIFIED: moving the guard to AFTER window.handle_event causes this test
    // to fail with focused==1 (not 2), confirming the bite is real.
    // -----------------------------------------------------------------------

    #[test]
    fn setup_guard_before_delegation_bite() {
        clear_history();
        history_add(103, "a");
        history_add(103, "b");
        history_add(103, "c"); // 3 entries → setup will set range=3, focused=1

        let mut hw = HistoryWindow::new(Rect::new(0, 0, 40, 15), 103);

        let mut out = std::collections::VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];

        // Make the viewer the current child so the Down key routes to it
        // (not the v-bar). Without this, the Down goes to the v-bar via
        // ofPostProcess and the test would not discriminate guard ordering.
        {
            let mut ctx = crate::view::Context::new(&mut out, &mut timers, 0, &mut deferred);
            hw.window.select_child(hw.viewer_id, &mut ctx);
        }
        deferred.clear(); // discard the set_current side-effects

        // Deliver a Down key via handle_event. The setup guard runs BEFORE
        // window.handle_event, so the sequence is:
        //   (A) setup: range=3, focused=1          (guard before delegation)
        //   (B) window.handle_event: Down → viewer (current) → focused 1→2
        {
            let mut ctx = crate::view::Context::new(&mut out, &mut timers, 0, &mut deferred);
            let mut ev = key_ev(Key::Down);
            hw.handle_event(&mut ev, &mut ctx);
        }

        // Read the viewer's focused value.
        let focused = hw
            .window
            .child_mut(hw.viewer_id)
            .and_then(|v| v.as_any_mut())
            .and_then(|a| a.downcast_mut::<HistoryViewer>())
            .map(|hv| hv.lv.focused)
            .expect("viewer resolves");

        // BITE: if the guard were moved AFTER window.handle_event, Down would
        // reach the viewer with range=0 (un-initialized) → focus_item_num clamps
        // → no-op → focused stays 0 → then setup sets focused=1 → assertion sees
        // 1, not 2. This test FAILS with focused==1 when the guard is misordered.
        assert_eq!(
            focused, 2,
            "focused must be 2: setup (range=3, focused=1) ran BEFORE Down (1→2). \
             If the guard is misordered (after delegation), focused would be 1."
        );
    }

    // -----------------------------------------------------------------------
    // Test 5: Negative h-bar max — value genuinely produced AND live drain
    //         doesn't panic (the HANDOVER watch-item, REQUIRED)
    //
    // Two halves:
    //
    // (A) STANDALONE: prove the negative max value is actually generated.
    //     Build a HistoryViewer with size.x=38 (matching the real interior of
    //     a 40-wide HistoryWindow after grow(-1,-1)) and history "hi!" (width 3).
    //     historyWidth() - size.x + 3 = 3 - 38 + 3 = -32 (negative).
    //     Assert the queued ScrollBarSetParams has max==-32 (the exact negative
    //     value). This half fails if setup ever skips the negative path or the
    //     arithmetic changes.
    //
    // (B) LIVE PUMP via exec_view: prove that draining the negative max through
    //     ScrollBar::set_params does not panic and the modal exits cleanly.
    //     ScrollBar::set_params floors aMax to aMin (= max(aMax, aMin)), so
    //     negative max=-32 becomes 0 — safe (no i32::clamp panic).
    // -----------------------------------------------------------------------

    #[test]
    fn negative_hbar_max_live_pump_no_panic() {
        clear_history();
        // Narrow entry: "hi!" → display width 3.
        history_add(104, "hi!");

        // -------- (A) Standalone: confirm the negative value is produced --------
        //
        // The real exec_view path uses Rect::new(5,3,45,13) → 40×10 window →
        // grow(-1,-1) → viewer size.x=38. Replicate that geometry directly.
        // historyWidth("hi!") = 3; size.x = 38 → max = 3 - 38 + 3 = -32.
        let expected_max: i32 = 3 - 38 + 3; // -32
        assert!(expected_max < 0, "expected_max must be negative");

        // Mint a real ViewId for the h-bar (mirror setup_with_hbar test pattern).
        let hbar = {
            let mut mint_group = crate::view::Group::new(Rect::new(0, 0, 4, 4));
            mint_group.insert(Box::new(HistoryViewer::new(
                Rect::new(0, 0, 1, 1),
                None,
                None,
                104,
            )))
        };

        // size.x = 38, matching the real interior width after grow(-1,-1).
        let mut hv = HistoryViewer::new(Rect::new(0, 0, 38, 8), Some(hbar), None, 104);
        assert_eq!(
            hv.lv.state.size.x, 38,
            "size.x == 38 (interior of 40-wide window)"
        );

        let mut out = std::collections::VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred_a: Vec<Deferred> = vec![];
        {
            let mut ctx = crate::view::Context::new(&mut out, &mut timers, 0, &mut deferred_a);
            hv.setup(&mut ctx);
        }

        // The queued h-bar SetParams must carry max == -32.
        assert!(
            deferred_a.iter().any(|x| matches!(
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
            "setup must queue hbar setRange(0, {expected_max}) — negative max genuinely produced"
        );

        // -------- (B) Live pump: drain the negative max without panic ----------
        //
        // Rect::new(5,3,45,13) → 40×10 window → grow(-1,-1) → viewer 38×8.
        // historyWidth() - size.x + 3 = 3 - 38 + 3 = -32.
        let (mut program, handle) = make_program();
        // Pre-queue event sequence to drive the modal to completion:
        //   Pump 1: Down → setup (queues h-bar SetParams max=-32); v-bar postProcess
        //           handles Down → broadcasts SCROLL_BAR_CLICKED. Deferred drain:
        //           h-bar SetParams max=-32 → set_params floors to 0 (no panic).
        //   Pump 2: SCROLL_BAR_CLICKED → viewer → request_focus → viewer becomes current.
        //   Pump 3: Enter → EndModal(OK).
        //   Pump 4: deferred drain → loop exits.
        handle.push_event(key_ev(Key::Down));
        handle.push_event(key_ev(Key::Enter));

        let hw = HistoryWindow::new(Rect::new(5, 3, 45, 13), 104);

        // exec_view drives the full pump loop. No panic = set_params handles
        // negative max safely (floors to min=0 via max(aMax, aMin)).
        let result = program.exec_view(Box::new(hw));
        assert_eq!(
            result,
            Command::OK,
            "Enter dismisses the modal cleanly after setup with negative h-bar max"
        );
        // Reaching here without panic confirms the negative-max path (-32 → 0) is safe.
    }

    // -----------------------------------------------------------------------
    // Test 6: history_window_cancels_on_outside_click
    //
    // Simulate what the pump redirect does: deliver a MouseDown with a position
    // outside the window's extent (localized to the window's frame) and verify
    // that Deferred::EndModal(Command::CANCEL) is queued.
    //
    // The pump subtracts modal_bounds.a before calling handle_event. A
    // HistoryWindow at Rect::new(10, 5, 50, 20) has size (40, 15), so
    // get_extent() = (0, 0)...(40, 15). Deliver with position=(-1, 0)
    // (outside the extent) to simulate an outside click.
    // -----------------------------------------------------------------------

    #[test]
    fn history_window_cancels_on_outside_click() {
        clear_history();
        history_add(110, "entry");

        let mut hw = HistoryWindow::new(Rect::new(10, 5, 50, 20), 110);

        // Run setup first (first-event guard) via a harmless broadcast.
        let mut out = std::collections::VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        {
            let mut ctx = crate::view::Context::new(&mut out, &mut timers, 0, &mut deferred);
            let mut ev = Event::Broadcast {
                command: Command::SCROLL_BAR_CHANGED,
                source: None,
            };
            hw.handle_event(&mut ev, &mut ctx);
        }
        deferred.clear();

        // Deliver a MouseDown with position outside the extent — simulates the
        // pump redirect with the position already localized (modal_bounds.a
        // subtracted). Position (-1, 0) is outside extent (0,0)...(40,15).
        let outside_click = Event::MouseDown(MouseEvent {
            position: Point::new(-1, 0),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        });
        let mut ev = outside_click;
        {
            let mut ctx = crate::view::Context::new(&mut out, &mut timers, 0, &mut deferred);
            hw.handle_event(&mut ev, &mut ctx);
        }

        assert!(
            ev.is_nothing(),
            "outside-click MouseDown consumed by HistoryWindow"
        );
        assert!(
            deferred
                .iter()
                .any(|x| matches!(x, Deferred::EndModal(Command::CANCEL))),
            "outside click must queue EndModal(CANCEL)"
        );
    }
}
