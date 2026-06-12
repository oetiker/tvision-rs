//! Process-global, byte-budget-bounded history store for input fields, plus the
//! [`HistoryViewer`] widget that shows the store in a modal recall list.
//!
//! Each "channel" is a small `u8` id that groups one input field's recall
//! list.  Entries are stored oldest-first globally; [`history_str(id, 0)`](history_str)
//! returns the oldest surviving entry for that id.
//!
//! # The clean read contract — every non-evicted entry is readable
//!
//! The store keeps a flat list of entries and a byte budget; once adding an
//! entry would exceed the budget, the globally-oldest entries are evicted until
//! it fits. **Every entry that has not been evicted is readable** — there is no
//! hidden front record and no skipped-oldest entry.
//!
//! This is a deliberate simplification of the original store, which carried a
//! hidden front sentinel record and skipped it (and, after the first eviction,
//! the real oldest entry) when reading. Pre-overflow behavior is identical; the
//! only difference is that we keep that one globally-oldest entry visible, and
//! our first-eviction byte boundary is 3 bytes looser because we do not spend
//! budget on a sentinel. The deviation is intentional and noted here so it is
//! not mistaken for a missing behavior.
//!
//! # Turbo Vision heritage
//!
//! Ports the history store (`histlist.cpp`) plus `THistoryViewer`
//! (`thstview.cpp`), `THistoryWindow` (`thistwin.cpp`), and the `THistory`
//! dropdown icon (`thistory.cpp`). The store drops the original front-sentinel
//! byte-block bookkeeping in favor of the clean read contract above.

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
    /// Byte cost of one entry: its byte length plus a 3-byte per-record overhead.
    fn cost(&self) -> usize {
        cost_of(&self.str)
    }
}

/// Byte cost of a candidate string — the single source of truth for the
/// per-entry budget accounting (string bytes plus a 3-byte per-record overhead).
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

/// Maximum byte budget shared across **all** ids.
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
/// Operation order:
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
// HistoryViewer
// ---------------------------------------------------------------------------

/// A read-only single-column list over the global history store, shown in a
/// modal recall popup when a user drops down an input field.
///
/// Enter / double-click confirms; Esc or a cancel command dismisses. All other
/// events fall through to the base [`ListViewer`] navigation.
///
/// # history_id type
///
/// The channel id is a `u8`. The store keys on a single byte, so using `u8`
/// throughout makes the width explicit and avoids a silent truncation bug at the
/// call boundary.
///
/// # Setup after insertion
///
/// Call [`setup`](HistoryViewer::setup) after inserting the viewer into a group:
/// it needs a `Context` to publish the list range, default focus, and horizontal
/// scrollbar range, none of which are available before insertion.
///
/// # Palette / theme
///
/// The history list recolors the gray-dialog list matrix into the blue
/// input-field look, surfaced through the [`ListViewer::list_roles`] override:
/// [`Role::HistoryViewerNormal`](crate::theme::Role::HistoryViewerNormal)
/// (white on blue) and
/// [`Role::HistoryViewerFocused`](crate::theme::Role::HistoryViewerFocused)
/// (white on green).
///
/// # Turbo Vision heritage
///
/// Ports `THistoryViewer` (`thstview.cpp`); its palette recolor surfaces as a
/// [`ListRoles`](crate::widgets::ListRoles) override.
pub struct HistoryViewer {
    lv: ListViewerState,
    history_id: u8,
}

impl HistoryViewer {
    /// The history viewer's color quintet: a normal row
    /// ([`Role::HistoryViewerNormal`](crate::theme::Role::HistoryViewerNormal),
    /// white on blue) and a focused row
    /// ([`Role::HistoryViewerFocused`](crate::theme::Role::HistoryViewerFocused),
    /// white on green).
    ///
    /// Lives here (not next to the base `LIST_VIEWER` quintet) because it is this
    /// class's own palette knowledge, sparing `list_viewer.rs` any reference to
    /// the history roles.
    pub const LIST_ROLES: crate::widgets::ListRoles = crate::widgets::ListRoles {
        normal_active: crate::theme::Role::HistoryViewerNormal,
        normal_inactive: crate::theme::Role::HistoryViewerNormal,
        focused: crate::theme::Role::HistoryViewerFocused,
        selected: crate::theme::Role::HistoryViewerNormal,
        divider: crate::theme::Role::HistoryViewerNormal,
    };

    /// Construct a `HistoryViewer` (the data-init portion of the widget).
    ///
    /// `bounds`: the view rectangle; `h`/`v`: optional scrollbar ids;
    /// `history_id`: the store channel this viewer presents.  No `Context` is
    /// needed here (see [`setup`](Self::setup)).
    pub fn new(bounds: Rect, h: Option<ViewId>, v: Option<ViewId>, history_id: u8) -> Self {
        HistoryViewer {
            // Always a single column.
            lv: ListViewerState::new(bounds, 1, h, v),
            history_id,
        }
    }

    /// Context-needing tail of construction — call once after insertion.
    ///
    /// 1. Set the list length to the channel's entry count.
    /// 2. If more than one entry exists, focus item 1 (the second-oldest) as the
    ///    default selection.
    /// 3. If a horizontal bar is wired, publish its range as
    ///    `0 ..= history_width() - size.x + 3`.
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
    /// Iterates the full channel and takes the max; returns 0 for an empty channel.
    ///
    /// Note: this is O(n²)-ish — each [`history_str(id, i)`](history_str)
    /// re-filters the store from the front and clones a `String` just to measure
    /// it. That is fine for a recall list's tiny `n`. The `.unwrap_or_default()`
    /// is defensive: `i` is always in `0..count`, so the `None` arm is
    /// effectively unreachable.
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

    /// Return the store entry for `item`.
    ///
    /// Negative or out-of-range items return an empty string.
    fn get_text(&self, item: i32) -> String {
        if item < 0 {
            return String::new();
        }
        history_str(self.history_id, item as usize).unwrap_or_default()
    }

    /// The history viewer's recolored quintet ([`HistoryViewer::LIST_ROLES`]).
    fn list_roles(&self) -> crate::widgets::ListRoles {
        Self::LIST_ROLES
    }
    // is_selected / select_item: inherit the base (item == focused, and the
    // base broadcasts list-item-selected). Not overridden here.
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

    /// Confirm or dismiss the modal recall list.
    ///
    /// Enter / double-click → end the modal with [`Command::OK`].
    /// Esc / cancel command → end the modal with [`Command::CANCEL`].
    /// Everything else      → the base list-viewer handler (nav, scrollbar sync…).
    ///
    /// There is no modal-state gate: the viewer only ever lives inside a
    /// [`HistoryWindow`] that is always run modally, so ending the modal is
    /// unconditional.
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

    /// Republish the scroll-bar steps after a bounds change.
    fn on_bounds_changed(&mut self, ctx: &mut Context) {
        list_viewer::on_bounds_changed(self, ctx);
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
    /// The text of the currently focused entry (the window's selection).
    ///
    /// This accessor is on `HistoryViewer` (not exposed to the crate root) so
    /// `HistoryWindow::get_selection` can reach `lv.focused` without making the
    /// field public. `lv` is private to this module, but `HistoryWindow` lives
    /// in the same file, so the private-field access is allowed directly there.
    /// This accessor provides the clean named path.
    ///
    /// Read by the code that runs a `HistoryWindow` modally and reads the
    /// selection after `exec_view` returns.
    pub(crate) fn selection(&self) -> String {
        <Self as ListViewer>::get_text(self, self.lv.focused)
    }
}

// ---------------------------------------------------------------------------
// HistoryWindow
// ---------------------------------------------------------------------------

/// The modal window hosting a [`HistoryViewer`] recall list.
///
/// A window with a close box only (not movable) that assembles two scroll bars
/// and the viewer, then runs modally so the caller can read
/// [`get_selection`](HistoryWindow::get_selection) after `exec_view` returns.
///
/// # Color
///
/// The window keeps the default blue [`Window`]/[`Frame`](crate::frame::Frame)
/// role family. Because the popup is always the modal top, the passive-frame
/// color is never seen; every cell the popup actually shows (active frame, icon,
/// scroll-bar page) renders as expected. The viewer's item colors do remap —
/// see [`HistoryViewer`]'s `list_roles`.
///
/// An outside-bounds click cancels the modal ([`Command::CANCEL`]).
///
/// # Turbo Vision heritage
///
/// Ports `THistoryWindow` (`thistwin.cpp`); the viewer-construction indirection
/// is inlined into the constructor.
pub struct HistoryWindow {
    /// The embedded window. `HistoryWindow` *is-a* window.
    window: Window,
    /// The `HistoryViewer` child's id — resolved after construction for
    /// `setup` and `get_selection`.
    viewer_id: ViewId,
    /// Tracks whether the viewer's post-insert `setup` has been run.
    /// `setup` needs a live `Context`; it runs on the first `handle_event` call
    /// (the same constraint the list box's setup hits).
    setup_done: bool,
}

impl HistoryWindow {
    /// Build a `HistoryWindow` over `bounds` for the given channel.
    ///
    /// 1. A numberless, titleless window.
    /// 2. Close box only — not movable, growable, or zoomable.
    /// 3. Shrink the extent by one cell on each side, build a horizontal then a
    ///    vertical scroll bar, build a [`HistoryViewer`] over that inner rect, and
    ///    insert it into the window group.
    pub fn new(bounds: Rect, history_id: u8) -> Self {
        // (1) Numberless, titleless window.
        let mut window = Window::new(bounds, None, 0);
        // (2) Close box only.
        window.set_flags(WindowFlags {
            close: true,
            ..WindowFlags::default()
        });
        // (3) Inner rect: the extent shrunk by one cell on every side.
        let mut r = View::state(&window).get_extent();
        r.grow(-1, -1);

        // Build the two bars (order matters: horizontal then vertical; both are
        // inserted into the window group).
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

    /// The viewer's focused entry text.
    ///
    /// Uses `&mut self` because `child_mut` / `as_any_mut` require `&mut`. The
    /// modal result is read after the loop completes. If the downcast somehow
    /// fails (unreachable in practice — the viewer_id always resolves to a
    /// `HistoryViewer`), returns an empty string.
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
    /// Downcast hook so the modal completion can downcast the modal
    /// `dyn View` back to `HistoryWindow` and read [`get_selection`](Self::get_selection).
    /// Must be a real `Some(self)` — delegating to `window.as_any_mut()` would
    /// downcast to a `Window`, returning `None` for the `HistoryWindow` downcast
    /// (a silent pick-nothing). NOT in the `skip(...)` list for that reason.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    /// Setup guard → delegate to the window's `handle_event` → outside-click
    /// cancel.
    ///
    /// (A) **One-time viewer setup BEFORE delegating** — the event then reaches
    ///     a ready viewer (range/focused initialized). `setup()` needs a live
    ///     `Context`, so it lands post-insert, here, on the first event (the same
    ///     constraint the list box's setup hits).
    ///
    /// (B) The window's base `handle_event` (base first).
    ///
    /// (C) **Outside-click cancel** — an outside-bounds mouse-down cancels the
    ///     modal. Such clicks reach the modal top window via the loop's modal
    ///     redirect; the test is `!extent.contains(position)`.
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
            // NOTE: the popup's internal currency (viewer == current) is NOT
            // established here anymore. `exec_view`'s kept post-insert
            // `reset_current` (the faithful open hook) makes the viewer current AT
            // OPEN — the viewer is the popup's first visible+selectable child
            // (frame and scroll bars are non-selectable) — so even an immediate
            // Esc/Enter with no prior nav routes to the viewer and its endModal
            // fires. The first-event `select_child` workaround that used to live
            // here was redundant once the exec_view open hook made the viewer
            // current (proven by the no_nav_first_event bite test).
            self.setup_done = true;
        }
        // (B) The window's base handler runs first.
        self.window.handle_event(ev, ctx);
        // (C) Outside-click cancel: a mouse-down outside the window's extent ends
        // the modal. The pump delivers outside clicks with the position already
        // localized, so the test is just !extent.contains(position).
        if let Event::MouseDown(m) = ev
            && !View::state(self).get_extent().contains(m.position)
        {
            ctx.end_modal(Command::CANCEL);
            ev.clear();
        }
    }
}

// ---------------------------------------------------------------------------
// THistory — the dropdown-arrow icon next to an input line
// ---------------------------------------------------------------------------

/// `THistory` — the dropdown-arrow icon placed next to a [`InputLine`](crate::widgets::InputLine).
///
/// On its trigger (a click, or Ctrl/↓ while the linked input is focused) it opens
/// a modal [`HistoryWindow`] over the channel's history, and on **OK** writes the
/// picked string back into the linked input line. A leaf view cannot itself run a
/// top-level modal loop, so it **requests** the open through
/// [`Deferred::OpenHistory`](crate::view::Deferred::OpenHistory) and the event
/// loop builds and drives the modal.
///
/// # Color
///
/// The classic green dropdown button: a black-on-green arrow
/// ([`Role::HistoryArrow`](crate::theme::Role::HistoryArrow)) and green-on-gray
/// sides ([`Role::HistorySides`](crate::theme::Role::HistorySides)).
///
/// # Focus on open
///
/// Focusing the linked input is a separate request
/// ([`focus_descendant`](crate::view::View::focus_descendant)) applied by the
/// loop; there is no inline success flag to test, so the open always proceeds —
/// focusing the link and opening the popup are independent requests.
///
/// # Turbo Vision heritage
///
/// Ports `THistory` (`thistory.cpp`). The owning back-pointer to the linked
/// input becomes a [`ViewId`] (D3), so there is nothing to null out on teardown,
/// and the palette becomes [`Role`](crate::theme::Role)s.
pub struct THistory {
    state: ViewState,
    /// The linked input line's id.
    link: ViewId,
    /// The history channel id.
    history_id: u8,
}

impl THistory {
    /// Build the icon over `bounds`, linked to input `link`, for channel
    /// `history_id`.
    ///
    /// The icon is *not* selectable, so a click delivers to it without grabbing
    /// focus — it is never the current view. It opts into post-processing so it
    /// sees key events only after the focused input line has had its turn.
    pub fn new(bounds: Rect, link: ViewId, history_id: u8) -> Self {
        let mut state = ViewState::new(bounds);
        // Post-process: the icon gets key-downs after the focused input line,
        // which leaves the ↓ arrow live and uncleared.
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

    /// Draw the dropdown icon `"▐~↓~▌"`: a left half-block `▐` (U+2590), a
    /// highlighted down-arrow `↓` (U+2193), and a right half-block `▌` (U+258C),
    /// where the `~…~` marks the highlighted region (the arrow). The sides render
    /// in [`Role::HistorySides`](crate::theme::Role::HistorySides) and the arrow
    /// in [`Role::HistoryArrow`](crate::theme::Role::HistoryArrow).
    fn draw(&mut self, ctx: &mut DrawCtx) {
        let lo = ctx.style(crate::theme::Role::HistorySides);
        let hi = ctx.style(crate::theme::Role::HistoryArrow);
        ctx.put_cstr(0, 0, "\u{2590}~\u{2193}~\u{258C}", lo, hi);
    }

    /// Open the modal on a trigger, or record history on the broadcast arm:
    ///
    /// * **mouse-down**: open (a mouse trigger never gates on focus).
    /// * **key-down mapping to the down-arrow**: open, gated (downstream, in the
    ///   loop) on the linked input being focused.
    ///   [`ctrl_to_arrow`](crate::event::ctrl_to_arrow) returns the
    ///   event UNCHANGED when it is not a Ctrl combo, so matching on `.key ==
    ///   Key::Down` covers both the literal ↓ and the Ctrl mapping; modifiers are
    ///   cleared on a mapped result, so we compare `.key` only.
    /// * **a released-focus broadcast from the link, or a record-history
    ///   broadcast**: record the linked input's current text. The event is left
    ///   live (not consumed) here.
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        match ev {
            Event::MouseDown(_) => {
                ctx.request_open_history(self.link, self.history_id, false);
                ev.clear();
            }
            Event::KeyDown(k) if crate::event::ctrl_to_arrow(*k).key == crate::event::Key::Down => {
                ctx.request_open_history(self.link, self.history_id, true);
                // We consume the ↓ unconditionally: the leaf cannot read the
                // link's focus inline (it only holds the link's id), so the focus
                // gate is applied later in the loop's `OpenHistory` arm.
                // Clear-always is correct — leaving it live would let a
                // focused-link ↓ be double-handled.
                ev.clear();
            }
            Event::Broadcast { command, source }
                if (*command == Command::RELEASED_FOCUS && *source == Some(self.link))
                    || *command == Command::RECORD_HISTORY =>
            {
                ctx.request_record_history(self.link, self.history_id);
                // Leave the broadcast live (not consumed).
            }
            _ => {}
        }
    }
    // value/set_value: trait default (the icon has no transferable value).
}

// ---------------------------------------------------------------------------
// THistory tests
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
        // The broadcast arm does not consume the event.
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
// HistoryWindow tests
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
    //         doesn't panic
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
