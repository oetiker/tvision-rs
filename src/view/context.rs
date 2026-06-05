//! Downward draw and event/update contexts — deviations **D3** / **D4**.
//!
//! D3 forbids up-pointers: a parent passes a context *down* carrying everything
//! a child would otherwise reach upward for. There are two:
//!
//! * [`DrawCtx`] — the clipped, themed writer a view paints through during
//!   `draw()`. It works in *view-local* coordinates; the ctx translates them to
//!   absolute screen coordinates and clips. It re-expresses the `DrawBuffer`
//!   write ops (D8 clip-for-correctness) on top of the row-18 [`Buffer`] and the
//!   row-8 [`text`](crate::text) primitives — never re-deriving wide-char logic.
//! * [`Context`] — the event/update context handlers and `handle_event` reach
//!   for. It is anchored to the decided `ctx.*` call surface (post / broadcast /
//!   timer scheduling / deferred capture push). It is built over loop-owned
//!   state as **distinct `&mut` fields** so Phase 1 can take disjoint-field
//!   borrows; the fields are deliberately not hidden behind one getter.

use crate::capture::CaptureHandler;
use crate::color::Style;
use crate::command::Command;
use crate::event::Event;
use crate::screen::Buffer;
use crate::theme::{Glyphs, Role, Theme};
use crate::timer::{TimerId, TimerQueue};
use crate::view::geometry::{Point, Rect};
use crate::view::id::ViewId;
use crate::view::view::StateFlag;
use std::collections::VecDeque;
use std::time::Duration;
use unicode_width::UnicodeWidthChar;

// ---------------------------------------------------------------------------
// Deferred — an effect on loop-owned state requested through Context (D3 / D9)
// ---------------------------------------------------------------------------

/// An effect on loop-owned state that a downward-borrowed view / capture handler
/// cannot perform inline (D3/D9). During dispatch the view tree is a live `&mut`
/// borrow stack (root → desktop → window → frame): a view cannot reach *up* or
/// *sideways* — every ancestor is already `&mut`-borrowed above it, and a fresh
/// `root.find_mut(id)` would alias that borrow. Nor does a view hold the program's
/// capture stack or command set. So any such effect is **requested** through
/// [`Context`] (which pushes a variant here) and **applied** by the loop after the
/// dispatch unwinds and the root is free again.
///
/// One queue, drained once per pump in **insertion order**. The variants fall into
/// four disjoint families by the loop-owned state they touch — capture stack,
/// command set, view tree, loop state (`end_state`) — so cross-family apply order
/// never affects the result;
/// same-family items keep their relative order.
pub enum Deferred {
    /// Push a capture handler onto the program's capture stack. Applied *after* the
    /// current dispatch, so the pushed handler sees the *next* event, never the
    /// current one (the `compose_full_protocol` invariant).
    PushCapture(Box<dyn CaptureHandler>),
    /// Enable a command in the program's command set (`enableCommand`).
    EnableCommand(Command),
    /// Disable a command in the program's command set (`disableCommand`).
    DisableCommand(Command),
    /// Apply new bounds to the view named by `ViewId` (drag move/grow). No ctx
    /// needed at apply time (`change_bounds` takes none).
    ChangeBounds(ViewId, Rect),
    /// Flip a propagating state flag on the view (drag end → `sfDragging` off).
    SetState(ViewId, StateFlag, bool),
    /// Remove the view from whichever group owns it (`cmClose`).
    Close(ViewId),
    /// Focus (select) the view named by `ViewId` within its owning group
    /// (`TLabel::focusLink` → `link->focus()`). The pump resolves it via
    /// [`View::focus_descendant`](crate::view::View::focus_descendant), which walks
    /// to the owning group and runs `focus_child` (the `ofSelectable` gate lives in
    /// that group walk, not at the request site). A view (the label) holds only the
    /// link's [`ViewId`] (D3), so it cannot select a sibling inline.
    FocusById(ViewId),
    /// Request the (modal) loop end with `command` (`TGroup::endModal`). The pump
    /// applies it by setting `Program::end_state`; the nested `exec_view` loop then
    /// observes it. The downward (D3) replacement for a view calling `endModal` up
    /// its owner chain.
    ///
    /// This touches **loop state** (`end_state`) — a fourth disjoint target
    /// alongside the capture stack / command set / view tree — so the `69897fe`
    /// insertion-order drain stays order-equivalent: no dispatch co-queues an
    /// `EndModal` with an effect on the *same* state, and cross-family order never
    /// affects the result.
    EndModal(Command),

    // -- row 27: the TScroller cross-view scrollbar broker (D3) --------------
    //
    // All three touch the **view tree** family (same as `ChangeBounds`/`SetState`/
    // `Close`/`FocusById`), so the `69897fe` insertion-order drain stays
    // order-equivalent: no single dispatch co-queues two ops on the *same*
    // scrollbar/scroller in a conflicting order. They exist because a leaf view
    // (the scroller) holds only `&mut Context` (D3) and so can neither **read** nor
    // **mutate** its window-frame sibling scrollbars; the pump — which owns the
    // whole tree — is the cross-view broker, performing every read/write at
    // deferred-apply time via `group.find_mut(id)`.
    /// **Read direction** (`TScroller::scrollDraw`): resolve the `h`/`v` scrollbars,
    /// read each `value` (via [`View::value`](crate::view::View::value) →
    /// [`FieldValue::Int`](crate::data::FieldValue::Int)), and push the resulting
    /// delta into `scroller` (the pump downcasts it to `Scroller` and calls
    /// `apply_delta`, which does the `setCursor` adjust + `delta = d`). The scroller
    /// requests this from `handle_event` when a `cmScrollBarChanged` broadcast names
    /// one of its bars as `source`.
    SyncScrollerDelta {
        /// The scroller whose `delta`/`cursor` to update.
        scroller: ViewId,
        /// The horizontal scrollbar to read `value` from (`None` = no h bar → 0).
        h: Option<ViewId>,
        /// The vertical scrollbar to read `value` from (`None` = no v bar → 0).
        v: Option<ViewId>,
    },
    /// **Write direction** (`TScrollBar::setParams`/`setValue`, driven by
    /// `TScroller::setLimit`/`scrollTo`). The pump resolves `id`, downcasts to
    /// `ScrollBar`, fills each `None` field from the bar's **live** value
    /// (preserve-where-`None`), then calls `set_params` — which clamps and may
    /// re-broadcast `cmScrollBarChanged`. One flexible variant serves row 27 and the
    /// future `TListViewer`/`TEditor` (`setRange`/`setStep`/`setValue` shapes).
    ScrollBarSetParams {
        /// The scrollbar to update.
        id: ViewId,
        /// New value, or `None` to preserve the bar's live `value`.
        value: Option<i32>,
        /// New range minimum, or `None` to preserve `min_value`.
        min: Option<i32>,
        /// New range maximum, or `None` to preserve `max_value`.
        max: Option<i32>,
        /// New page step, or `None` to preserve `page_step`.
        page_step: Option<i32>,
        /// New arrow step, or `None` to preserve `arrow_step`.
        arrow_step: Option<i32>,
    },
    /// **Visibility direction** (`TScroller::showSBar` → `TView::show`/`hide`). The
    /// pump resolves `id` and sets `state.state.visible` (no downcast —
    /// `state_mut` is on the trait; the painter skips `!visible` children). There is
    /// no propagating `StateFlag::Visible` (D8 dropped `sfVisible`'s side effects),
    /// so visibility is set directly on the [`ViewState`](crate::view::ViewState).
    SetVisible(ViewId, bool),

    // -- row 28: the TListViewer cross-view scrollbar read-sync (D3) ----------
    /// **Read direction for `TListViewer`** (the `cmScrollBarChanged` handler).
    /// Resolve the `h`/`v` scrollbars, read each `value`
    /// (via [`View::value`](crate::view::View::value) →
    /// [`FieldValue::Int`](crate::data::FieldValue::Int)), then call
    /// [`View::apply_list_scroll`](crate::view::View::apply_list_scroll) on the
    /// `list` view (the trait method — NOT a downcast: `ListViewer` is a trait, so
    /// `dyn View → dyn ListViewer` cannot be downcast, unlike the row-27 scroller).
    ///
    /// **Termination (the centerpiece property):** unlike
    /// [`SyncScrollerDelta`](Self::SyncScrollerDelta), this read-sync **writes
    /// back** — `apply_list_scroll`'s `focus_item_num` calls `focusItem`, which
    /// requests a `setValue(focused)` on the v-bar (another
    /// [`ScrollBarSetParams`](Self::ScrollBarSetParams)). That terminates because
    /// [`ScrollBar::set_params`](crate::widgets::ScrollBar::set_params) is
    /// **change-guarded**: it re-broadcasts `cmScrollBarChanged` only on an actual
    /// value change, so writing back the already-current value is a silent no-op
    /// (steady state: quiescent; after a clamp: one extra round then quiescent).
    ///
    /// Touches the **view-tree** family (same as the scroller broker ops), so the
    /// insertion-order drain stays order-equivalent.
    SyncListViewer {
        /// The list view whose `focused`/`top_item`/`indent` to update.
        list: ViewId,
        /// The horizontal scrollbar to read `value` from (`None` = no h bar).
        h: Option<ViewId>,
        /// The vertical scrollbar to read `value` from (`None` = no v bar).
        v: Option<ViewId>,
    },

    // -- row 49: the TMenuView command-graying broker (D3) --------------------
    /// **Command-graying broker for `TMenuView`** (ports `updateMenu`, triggered
    /// by the `cmCommandSetChanged` broadcast). Resolve the menu view by `id` and
    /// call [`View::update_menu_commands`](crate::view::View::update_menu_commands)
    /// with the pump's **live** [`CommandSet`](crate::command::CommandSet), which
    /// regrays the menu tree (`disabled = !commandEnabled(command)` per command
    /// item, recursing submenus).
    ///
    /// A broker — **not** a `&CommandSet` read-accessor on [`Context`] — because
    /// the command set lives on `Program` and the apply-phase `Context` is alive
    /// across a loop whose `EnableCommand`/`DisableCommand` arms mutate
    /// `command_set` (`&mut`); a `&CommandSet` on `Context` would alias that
    /// borrow. The view (a child, D3) cannot read the command set inline, so it
    /// requests this by its own id and the pump calls back at apply time, exactly
    /// like [`SyncListViewer`](Self::SyncListViewer) + `apply_list_scroll`.
    ///
    /// Touches the **view-tree** family (same as the scroller/list broker ops), so
    /// the insertion-order drain stays order-equivalent.
    UpdateMenu(ViewId),

    // -- rows 50-52: the TMenuView modal layer (MenuSession, D3/D9) ------------
    /// **Open a menu box** — the deferred realization of `execute()`'s submenu
    /// open (`tmnuview.cpp:382`, `topMenu()->newSubView(r, current->subMenu)` →
    /// `owner->execView(target)`). The [`MenuSession`](crate::menu::MenuSession)
    /// capture handler **pre-mints** `id` from [`ViewId::next`](crate::view::ViewId)
    /// so it already knows the box id with no insert-time callback; the pump
    /// builds a [`MenuBox`](crate::menu::MenuBox) from `menu` over `bounds` and
    /// [`Group::insert_with_id`](crate::view::Group::insert_with_id)s it into the
    /// root group, stamping that id. **No focus move** — the box is never current
    /// (Clean Architecture A; the session owns every event). `menu` is a clone of
    /// the submenu subtree (clone-at-open is faithful — `execute()` has no
    /// evBroadcast case, so `disabled` is frozen for the box's lifetime).
    ///
    /// Touches the **view-tree** family (same as `Close`/the broker ops), so the
    /// insertion-order drain stays order-equivalent. The activation site queues
    /// the [`PushCapture`](Self::PushCapture) of the session AND the first
    /// `OpenMenuBox` in the same batch (no dead first event).
    OpenMenuBox {
        /// The pre-minted id the box will be stamped with.
        id: ViewId,
        /// The (cloned) submenu subtree the box presents.
        menu: crate::menu::Menu,
        /// The box bounds in the root group's frame.
        bounds: Rect,
    },
    /// **Set a menu view's highlight cache** (`TMenuView::current` ← index). The
    /// pump resolves `id` and calls
    /// [`View::set_menu_current`](crate::view::View::set_menu_current) (a trait
    /// method, mirroring the `update_menu_commands` broker — no downcast). This is
    /// the write-only display cache the bar/box `draw` reads to pick the selected
    /// colour; the [`MenuSession`](crate::menu::MenuSession) owns the authoritative
    /// `current` and pushes it here whenever navigation moves the highlight.
    ///
    /// Touches the **view-tree** family, so the insertion-order drain stays
    /// order-equivalent.
    SetMenuCurrent(ViewId, Option<usize>),

    // -- row 57: the THistory view-triggered async-modal seam (D3/D9) ----------
    /// **View-triggered modal open** (`THistory`; msgbox 63 will add sibling
    /// completions). Built at apply time because the trigger view holds only the
    /// link's id (D3): the pump reads the link, records history, builds the
    /// `THistoryWindow`, and stashes it into `Program::pending_modal` — it does
    /// **not** call `exec_view` here (the apply phase is inside the `pump_once`
    /// destructure; a view cannot call `exec_view`, which is top-level only). The
    /// OUTER driver loop runs `exec_view` at top level after `pump_once` returns.
    ///
    /// Touches the **view-tree** family + **loop state** (`pending_modal`), like
    /// the other tree ops + `EndModal`, so the insertion-order drain stays
    /// order-equivalent (no dispatch co-queues a conflicting op on the same state).
    OpenHistory {
        /// The linked `TInputLine` whose text/bounds/focus drive the open + flowback.
        link: ViewId,
        /// The history channel id.
        history_id: u8,
        /// True for the keyboard trigger (gate on the link being focused, faithful
        /// to `(link->state & sfFocused)`); false for the mouse trigger.
        require_focus: bool,
    },
    /// **recordHistory(link->data)** for the broadcast arm (`cmReleasedFocus` on
    /// the link / `cmRecordHistory`): resolve the link, read its text,
    /// `history_add(id, text)`. Touches no loop-owned state beyond the read of the
    /// view tree (a pure side effect on the process-global history store), so it is
    /// order-equivalent with every other family.
    RecordHistory { link: ViewId, history_id: u8 },
}

// ---------------------------------------------------------------------------
// DrawCtx — the downward draw context (D3 / D8)
// ---------------------------------------------------------------------------

/// The clipped, themed writer every view paints through (D3).
///
/// All public write methods take **view-local** coordinates: `(0, 0)` is the
/// view's own top-left. The ctx adds [`origin`](Self::origin) to translate into
/// absolute screen columns/rows, and clips every write to [`clip`](Self::clip).
/// The clip is stored as an **absolute** rect already intersected with the
/// buffer bounds at construction, so a write can never index the buffer out of
/// range.
pub struct DrawCtx<'a> {
    buffer: &'a mut Buffer,
    /// Absolute clip rect, already intersected with the buffer's `(0,0,w,h)`.
    clip: Rect,
    /// View-local `(0, 0)` maps to this absolute screen position.
    origin: Point,
    theme: &'a Theme,
}

impl<'a> DrawCtx<'a> {
    /// Build a draw context.
    ///
    /// `clip` is intersected with the buffer's bounds (`(0, 0, width, height)`)
    /// at construction and stored absolute, so the write methods can never index
    /// out of bounds.
    pub fn new(buffer: &'a mut Buffer, theme: &'a Theme, clip: Rect, origin: Point) -> Self {
        let bounds = Rect::new(0, 0, buffer.width() as i32, buffer.height() as i32);
        let mut clip = clip;
        clip.intersect(&bounds);
        DrawCtx {
            buffer,
            clip,
            origin,
            theme,
        }
    }

    /// The [`Style`] for `role` from the active theme.
    pub fn style(&self, role: Role) -> Style {
        self.theme.style(role)
    }

    /// The theme's glyph holder (D7 stub for now).
    pub fn glyphs(&self) -> &Glyphs {
        self.theme.glyphs()
    }

    /// The absolute clip rect (already intersected with the buffer bounds).
    pub fn clip(&self) -> Rect {
        self.clip
    }

    /// The absolute screen position that view-local `(0, 0)` maps to.
    pub fn origin(&self) -> Point {
        self.origin
    }

    /// Write one cell at view-local `(x, y)` with `style`.
    ///
    /// A double-width `ch` sets the lead `wide` and the next cell `wide_trail`,
    /// but only if both fall inside the clip; if the trail would fall outside,
    /// a space is written instead. Anything fully outside the clip is dropped
    /// (never panics).
    pub fn put_char(&mut self, x: i32, y: i32, ch: char, style: Style) {
        if self.clip.is_empty() {
            return;
        }
        let ax = x + self.origin.x;
        let ay = y + self.origin.y;
        if ay < self.clip.a.y || ay >= self.clip.b.y {
            return;
        }
        if ax < self.clip.a.x || ax >= self.clip.b.x {
            return;
        }
        let wide = UnicodeWidthChar::width(ch).unwrap_or(1) > 1;
        let row = self.buffer.row_mut(ay as u16);
        let i = ax as usize;
        if wide && ax + 1 < self.clip.b.x {
            // Room for both halves inside the clip.
            let mut buf = [0u8; 4];
            row[i].set_str(ch.encode_utf8(&mut buf), true);
            row[i].set_style(style);
            row[i + 1].set_wide_trail();
            row[i + 1].set_style(style);
        } else if wide {
            // Trail would fall outside the clip — degrade to a space.
            row[i].set_char(' ');
            row[i].set_style(style);
        } else {
            row[i].set_char(ch);
            row[i].set_style(style);
        }
    }

    /// Write `s` at view-local `(x, y)` with a fixed `style`, width-aware and
    /// clipped. Returns the number of columns actually written.
    ///
    /// Delegates the wide-char and edge-straddle logic to [`text::draw_str`],
    /// exactly as `DrawBuffer::move_str_part` does — the string is written into
    /// the clipped sub-slice of the target buffer row, with `indent` /
    /// `text_indent` chosen so a glyph straddling either clip edge degrades the
    /// same way `move_str_part` already handles it.
    pub fn put_str(&mut self, x: i32, y: i32, s: &str, style: Style) -> i32 {
        if self.clip.is_empty() {
            return 0;
        }
        let ay = y + self.origin.y;
        if ay < self.clip.a.y || ay >= self.clip.b.y {
            return 0;
        }
        let ax = x + self.origin.x;
        // The writable window for this row is the clip's column span.
        let lo = self.clip.a.x as usize;
        let hi = self.clip.b.x as usize; // > lo, since clip is non-empty
        let row = &mut self.buffer.row_mut(ay as u16)[lo..hi];

        let (indent, text_indent) = if ax >= self.clip.a.x {
            // String starts at or after the clip left edge: indent into the
            // sub-slice; right-edge truncation falls out of `draw_str` running
            // out of cells.
            ((ax - self.clip.a.x) as usize, 0)
        } else {
            // String starts left of the clip: skip the off-screen columns via
            // text_indent (this is move_str_part's left-edge straddle path).
            (0, self.clip.a.x - ax)
        };

        crate::text::draw_str(row, indent, s, text_indent, style) as i32
    }

    /// Write `s` at view-local `(x, y)` with a fixed `style`, starting from
    /// display column `text_indent` of `s` (skipping that many leading columns)
    /// — ports `TDrawBuffer::moveStr`'s `begin` parameter, used by
    /// `TInputLine::draw` to render a horizontally-scrolled field. Width-aware and
    /// clipped exactly like [`put_str`](Self::put_str). Returns columns written.
    ///
    /// A glyph straddling the `text_indent` boundary degrades to a space (the
    /// `move_str_part` left-edge straddle), via [`text::draw_str`].
    pub fn put_str_part(&mut self, x: i32, y: i32, s: &str, text_indent: i32, style: Style) -> i32 {
        if self.clip.is_empty() {
            return 0;
        }
        let ay = y + self.origin.y;
        if ay < self.clip.a.y || ay >= self.clip.b.y {
            return 0;
        }
        let ax = x + self.origin.x;
        let lo = self.clip.a.x as usize;
        let hi = self.clip.b.x as usize;
        let row = &mut self.buffer.row_mut(ay as u16)[lo..hi];

        // Combine the clip-left-edge skip (when the string starts left of the
        // clip) with the caller's text_indent — both are column skips into `s`.
        let (indent, clip_skip) = if ax >= self.clip.a.x {
            ((ax - self.clip.a.x) as usize, 0)
        } else {
            (0, self.clip.a.x - ax)
        };
        crate::text::draw_str(row, indent, s, text_indent + clip_skip, style) as i32
    }

    /// Write `s` at view-local `(x, y)`, toggling between `lo` and `hi` styles at
    /// each `~` (the `~` itself is not drawn) — ports `TDrawBuffer::moveCStr`'s
    /// attribute-pair toggle (used by frame icons; reused by buttons/labels/menus
    /// for hotkey highlighting). Starts in `lo`. Clipped exactly like
    /// [`put_char`](Self::put_char). Returns the number of columns advanced.
    ///
    /// Faithful to [`DrawBuffer::move_cstr_part`](crate::screen::DrawBuffer): the
    /// first `~` flips `lo` → `hi`, the next flips back, and so on; the `~`
    /// characters draw nothing and do not advance the column.
    pub fn put_cstr(&mut self, x: i32, y: i32, s: &str, lo: Style, hi: Style) -> i32 {
        let mut col = 0i32;
        let mut current = lo;
        let mut hi_active = false;
        for ch in s.chars() {
            if ch == '~' {
                hi_active = !hi_active;
                current = if hi_active { hi } else { lo };
                continue;
            }
            self.put_char(x + col, y, ch, current);
            col += UnicodeWidthChar::width(ch).unwrap_or(1) as i32;
        }
        col
    }

    /// Fill view-local rect `area_local` (clipped) with `ch` styled `style`.
    pub fn fill(&mut self, area_local: Rect, ch: char, style: Style) {
        if self.clip.is_empty() {
            return;
        }
        // Translate to absolute and clip.
        let mut abs = area_local;
        abs.r#move(self.origin.x, self.origin.y);
        abs.intersect(&self.clip);
        if abs.is_empty() {
            return;
        }
        for ay in abs.a.y..abs.b.y {
            let row = self.buffer.row_mut(ay as u16);
            for ax in abs.a.x..abs.b.x {
                let cell = &mut row[ax as usize];
                cell.set_char(ch);
                cell.set_style(style);
            }
        }
    }

    /// A child context for a sub-view at view-local rect `area_local`.
    ///
    /// The child's clip is `self.clip ∩ (area_local translated by origin)`, and
    /// its origin is `self.origin + area_local.a`. The buffer is reborrowed for
    /// the child's shorter lifetime. No re-intersection with the buffer bounds
    /// is needed — `self.clip` is already inside them.
    pub fn sub(&mut self, area_local: Rect) -> DrawCtx<'_> {
        let mut abs = area_local;
        abs.r#move(self.origin.x, self.origin.y);
        let mut clip = self.clip;
        clip.intersect(&abs);
        DrawCtx {
            buffer: &mut *self.buffer,
            clip,
            origin: self.origin + area_local.a,
            theme: self.theme,
        }
    }
}

// ---------------------------------------------------------------------------
// Context — the downward event/update context (D3 / D4)
// ---------------------------------------------------------------------------

/// The event/update context `handle_event` and capture handlers reach for (D3).
///
/// Built over loop-owned state as **distinct `&mut` fields** (not hidden behind
/// a single getter) so Phase 1 can borrow them disjointly. The live event loop
/// (row 31) owns the backing `VecDeque` / [`TimerQueue`] / pending-capture
/// `Vec` and constructs a fresh `Context` per dispatch.
///
/// `query(ViewId, …) -> Option<T>` / `message(ViewId, …)` are **tree-owner**
/// primitives (Group/Program over `find_mut`), *not* `Context` methods — a
/// `Context` deliberately holds no tree to route through. They are **deferred to
/// row 34** (their first return-consumer, a dialog `cmCanCloseForm` veto), so
/// they are intentionally not stubbed here.
pub struct Context<'a> {
    /// Posted commands / broadcasts, drained by the loop after dispatch.
    out_events: &'a mut VecDeque<Event>,
    /// The loop's timer queue.
    timers: &'a mut TimerQueue,
    /// The clock value sampled for this dispatch pass.
    now_ms: u64,
    /// Deferred effects on loop-owned state ([`Deferred`]) — capture pushes, command
    /// enable/disable, and tree mutations (bounds / state-flag / close). A
    /// downward-borrowed view / capture handler cannot touch the capture stack, the
    /// command set, or the tree inline (D3/D9; see [`Deferred`]); it requests the
    /// effect here and the loop applies the queue *after* the current dispatch. One
    /// channel — adding a capability adds a variant, not a field.
    deferred: &'a mut Vec<Deferred>,
    /// The size of the view's owner (the group currently routing to it), so a child
    /// can reach `owner->size` / `owner->getExtent()` without an up-pointer (D3).
    /// Used by `TWindow::zoom`/`sizeLimits` (33c) and the drag limits (33d).
    ///
    /// **Transient routing state**, NOT a loop-owned channel: each
    /// `Group::handle_event` sets it to its own size before delivering to children
    /// and restores it on exit (so nesting root→desktop→window works). It is valid
    /// **only during group-routed dispatch**; a capture handler runs *before* group
    /// routing and sees the default `(0,0)`. That is fine — 33d's drag handler must
    /// capture its limits at *push time* (inside the window's `handle_event`, where
    /// `owner_size` is correctly set), never read them at drag time.
    owner_size: Point,
}

impl<'a> Context<'a> {
    /// Build an event/update context over the loop-owned state.
    pub fn new(
        out_events: &'a mut VecDeque<Event>,
        timers: &'a mut TimerQueue,
        now_ms: u64,
        deferred: &'a mut Vec<Deferred>,
    ) -> Self {
        Context {
            out_events,
            timers,
            now_ms,
            deferred,
            owner_size: Point::default(),
        }
    }

    /// Post a targeted command (`Event::Command`) into the loop's queue.
    pub fn post(&mut self, cmd: Command) {
        self.out_events.push_back(Event::Command(cmd));
    }

    /// Broadcast a command (`Event::Broadcast`) into the loop's queue. `source`
    /// names the view the broadcast is about (the `infoPtr` successor; D4
    /// amendment), or `None` if it concerns no particular view.
    pub fn broadcast(&mut self, command: Command, source: Option<ViewId>) {
        self.out_events
            .push_back(Event::Broadcast { command, source });
    }

    /// Arm a timer, returning its handle. `now_ms` is supplied from this
    /// context's dispatch snapshot (D9: clock not stored in the queue).
    pub fn set_timer(&mut self, timeout: Duration, period: Option<Duration>) -> TimerId {
        self.timers.set_timer(self.now_ms, timeout, period)
    }

    /// Cancel a pending timer.
    pub fn kill_timer(&mut self, id: TimerId) {
        self.timers.kill_timer(id);
    }

    /// Push a capture handler — **deferred** ([`Deferred::PushCapture`]). The loop
    /// applies the queue after the current dispatch, so the pushed handler sees the
    /// *next* event, never the current one.
    ///
    /// There is intentionally **no `pop_capture`**: a handler pops itself by
    /// returning [`CaptureFlow::ConsumedPop`](crate::capture::CaptureFlow::ConsumedPop).
    pub fn push_capture(&mut self, handler: Box<dyn CaptureHandler>) {
        self.deferred.push(Deferred::PushCapture(handler));
    }

    /// Request `cmd` be enabled in the program's command set — **deferred**
    /// ([`Deferred::EnableCommand`]). Realizes `TView::enableCommand` from a view
    /// that has no up-pointer to the program (D3).
    pub fn enable_command(&mut self, cmd: Command) {
        self.deferred.push(Deferred::EnableCommand(cmd));
    }

    /// Request `cmd` be disabled — **deferred** ([`Deferred::DisableCommand`]; see
    /// [`enable_command`](Self::enable_command)).
    pub fn disable_command(&mut self, cmd: Command) {
        self.deferred.push(Deferred::DisableCommand(cmd));
    }

    /// Request the view named by `id` be moved/resized to `bounds` — **deferred**
    /// ([`Deferred::ChangeBounds`]). The loop resolves `id` via `find_mut` and calls
    /// `change_bounds`. A capture handler (the drag) holds only a [`ViewId`] (D3),
    /// so it cannot mutate the tree inline.
    pub fn request_bounds(&mut self, id: ViewId, bounds: Rect) {
        self.deferred.push(Deferred::ChangeBounds(id, bounds));
    }

    /// Request a propagating state flag be flipped on the view named by `id` —
    /// **deferred** ([`Deferred::SetState`]; see [`request_bounds`](Self::request_bounds)).
    /// The loop resolves `id` via `find_mut` and calls `set_state` (drag end →
    /// `sfDragging` off).
    pub fn request_set_state(&mut self, id: ViewId, flag: StateFlag, enable: bool) {
        self.deferred.push(Deferred::SetState(id, flag, enable));
    }

    /// Request the view named by `id` be removed from whichever group owns it —
    /// **deferred** ([`Deferred::Close`]; see [`request_bounds`](Self::request_bounds)).
    /// The loop resolves it via `remove_descendant` (`cmClose`).
    pub fn request_close(&mut self, id: ViewId) {
        self.deferred.push(Deferred::Close(id));
    }

    /// Request the view named by `id` be focused (selected) within its owning
    /// group — **deferred** ([`Deferred::FocusById`]; see
    /// [`request_close`](Self::request_close)). The loop resolves it via
    /// [`View::focus_descendant`](crate::view::View::focus_descendant)
    /// (`TLabel::focusLink`). The `ofSelectable` gate is applied during that group
    /// walk, so the caller (the label) need not — and cannot, holding only the id —
    /// check it.
    pub fn request_focus(&mut self, id: ViewId) {
        self.deferred.push(Deferred::FocusById(id));
    }

    /// Request the (modal) loop end with `cmd` — **deferred** ([`Deferred::EndModal`]).
    /// `TGroup::endModal` from a view with no up-pointer to the program (D3): the
    /// pump sets `Program::end_state` and the nested `exec_view` loop observes it.
    ///
    /// **View-side, deferred.** This is the path a [`View`](crate::view::View)
    /// takes (it holds only `&mut Context`, never `&mut Program`). The owner /
    /// top-level path is the immediate `Program::end_modal`. Rule of thumb:
    /// view → `ctx.end_modal`; owner / top-level → `Program::end_modal`.
    pub fn end_modal(&mut self, cmd: Command) {
        self.deferred.push(Deferred::EndModal(cmd));
    }

    /// Request the `TScroller` `scroller` re-read its scrollbars' values and update
    /// its `delta`/`cursor` — **deferred** ([`Deferred::SyncScrollerDelta`]). The
    /// scroller (a leaf, D3) cannot read its window-frame sibling bars itself; the
    /// pump brokers the read. `h`/`v` are the bar [`ViewId`]s (`None` = no bar).
    pub fn request_sync_scroller_delta(
        &mut self,
        scroller: ViewId,
        h: Option<ViewId>,
        v: Option<ViewId>,
    ) {
        self.deferred
            .push(Deferred::SyncScrollerDelta { scroller, h, v });
    }

    /// Request the scrollbar `id` have its parameters set — **deferred**
    /// ([`Deferred::ScrollBarSetParams`]). Each `None` field is preserved from the
    /// bar's live value at apply time (`TScrollBar::setParams`/`setValue` driven by
    /// `TScroller::setLimit`/`scrollTo`). The scroller (a leaf, D3) cannot mutate its
    /// sibling bar inline.
    #[allow(clippy::too_many_arguments)]
    pub fn request_scroll_bar_params(
        &mut self,
        id: ViewId,
        value: Option<i32>,
        min: Option<i32>,
        max: Option<i32>,
        page_step: Option<i32>,
        arrow_step: Option<i32>,
    ) {
        self.deferred.push(Deferred::ScrollBarSetParams {
            id,
            value,
            min,
            max,
            page_step,
            arrow_step,
        });
    }

    /// Request the view `id` be shown/hidden — **deferred**
    /// ([`Deferred::SetVisible`]). `TScroller::showSBar` → `TView::show`/`hide` on a
    /// sibling scrollbar (which the scroller, a leaf, cannot reach inline, D3).
    pub fn request_set_visible(&mut self, id: ViewId, visible: bool) {
        self.deferred.push(Deferred::SetVisible(id, visible));
    }

    /// Request the `TListViewer` `list` re-read its scrollbars' values and update
    /// its `focused`/`top_item`/`indent` — **deferred**
    /// ([`Deferred::SyncListViewer`]). The list (a leaf, D3) cannot read its
    /// window-frame sibling bars itself; the pump brokers the read and calls back
    /// through [`View::apply_list_scroll`](crate::view::View::apply_list_scroll).
    /// `h`/`v` are the bar [`ViewId`]s (`None` = no bar).
    pub fn request_sync_list_viewer(&mut self, list: ViewId, h: Option<ViewId>, v: Option<ViewId>) {
        self.deferred.push(Deferred::SyncListViewer { list, h, v });
    }

    /// Request the menu view `id` regray its menu tree against the program's live
    /// command set — **deferred** ([`Deferred::UpdateMenu`]). The menu view (a
    /// child, D3) cannot read the command set itself; the pump brokers it and
    /// calls back through
    /// [`View::update_menu_commands`](crate::view::View::update_menu_commands).
    /// `TMenuView`'s `cmCommandSetChanged` handler requests this by its own id.
    pub fn request_update_menu(&mut self, id: ViewId) {
        self.deferred.push(Deferred::UpdateMenu(id));
    }

    /// Request a [`MenuBox`](crate::menu::MenuBox) be opened over `bounds`
    /// presenting `menu`, stamped with the pre-minted `id` — **deferred**
    /// ([`Deferred::OpenMenuBox`]). The [`MenuSession`](crate::menu::MenuSession)
    /// mints `id` itself (so it knows the box id with no callback) and the pump
    /// builds + inserts the box (no focus move). The submenu-open arm of the
    /// flattened `execute()`.
    pub fn request_open_menu_box(&mut self, id: ViewId, menu: crate::menu::Menu, bounds: Rect) {
        self.deferred
            .push(Deferred::OpenMenuBox { id, menu, bounds });
    }

    /// Request the menu view `id` set its highlight cache (`current`) to `current`
    /// — **deferred** ([`Deferred::SetMenuCurrent`]). The pump calls back through
    /// [`View::set_menu_current`](crate::view::View::set_menu_current). The
    /// session owns the authoritative `current` and pushes it to the view for
    /// `draw`.
    pub fn request_set_menu_current(&mut self, id: ViewId, current: Option<usize>) {
        self.deferred.push(Deferred::SetMenuCurrent(id, current));
    }

    /// Request a view-triggered history modal be opened over the link `link` —
    /// **deferred** ([`Deferred::OpenHistory`]). The `THistory` icon (a leaf, D3)
    /// holds only the link's id and cannot call `exec_view` (top-level only), so it
    /// requests the open; the pump reads the link, records history, builds the
    /// `THistoryWindow`, and stashes it into `Program::pending_modal` for the outer
    /// driver to `exec_view` at top level. `require_focus` gates the keyboard
    /// trigger on the link being focused (faithful to `(link->state & sfFocused)`).
    pub fn request_open_history(&mut self, link: ViewId, history_id: u8, require_focus: bool) {
        self.deferred.push(Deferred::OpenHistory {
            link,
            history_id,
            require_focus,
        });
    }

    /// Request `recordHistory(link->data)` for the `THistory` broadcast arm —
    /// **deferred** ([`Deferred::RecordHistory`]). The pump resolves the link, reads
    /// its current text, and `history_add`s it to the channel.
    pub fn request_record_history(&mut self, link: ViewId, history_id: u8) {
        self.deferred
            .push(Deferred::RecordHistory { link, history_id });
    }

    /// Re-queue a **raw event** into the loop's event queue — the raw-event
    /// sibling of [`post`](Self::post) (which only ever queues an
    /// `Event::Command`). Ports `execute()`'s `putEvent(e)`
    /// (`tmnuview.cpp:375/405`): the menu session re-posts the triggering event so
    /// the next pump re-delivers it (e.g. an outside click that should reach the
    /// view recovering focus, or — stage 2 — a mouse event on submenu-open). Lands
    /// in `out_events`, drained before the backend is polled.
    pub fn put_event(&mut self, ev: Event) {
        self.out_events.push_back(ev);
    }

    /// The clock value sampled for this dispatch pass.
    pub fn now_ms(&self) -> u64 {
        self.now_ms
    }

    /// The owner's size for the view currently being routed to — the downward
    /// realization of `owner->size` / `owner->getExtent()` (D3). See the
    /// [`owner_size`](Self::owner_size) field docs: it is **transient routing
    /// state** set/restored by each [`Group::handle_event`](crate::view::Group)
    /// around delivery, valid only during group-routed dispatch. Defaults to
    /// `(0, 0)`.
    pub fn owner_size(&self) -> Point {
        self.owner_size
    }

    /// Set the owner size for the routed view — called by
    /// [`Group::handle_event`](crate::view::Group) before delivering to children
    /// (set to the group's own size) and to restore it on exit.
    pub fn set_owner_size(&mut self, size: Point) {
        self.owner_size = size;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    fn style(fg: u8, bg: u8) -> Style {
        Style::new(Color::Bios(fg), Color::Bios(bg))
    }

    // -- DrawCtx ------------------------------------------------------------

    #[test]
    fn put_char_writes_at_origin_offset() {
        let mut buf = Buffer::new(10, 5);
        let theme = Theme::classic_blue();
        let s = style(0xF, 0x1);
        {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 5), Point::new(2, 1));
            // local (0,0) -> absolute (2,1)
            ctx.put_char(0, 0, 'X', s);
        }
        assert_eq!(buf.get(2, 1).symbol(), "X");
        assert_eq!(buf.get(2, 1).style(), s);
        // origin cell (0,0) untouched
        assert_eq!(buf.get(0, 0).symbol(), " ");
    }

    #[test]
    fn put_char_outside_clip_is_dropped() {
        let mut buf = Buffer::new(10, 5);
        let theme = Theme::classic_blue();
        {
            // clip only covers columns 2..5, rows 1..3
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(2, 1, 5, 3), Point::new(0, 0));
            ctx.put_char(0, 0, 'A', style(0xF, 0x1)); // outside clip
            ctx.put_char(3, 2, 'B', style(0xF, 0x1)); // inside clip
        }
        assert_eq!(
            buf.get(0, 0).symbol(),
            " ",
            "outside clip must not be written"
        );
        assert_eq!(buf.get(3, 2).symbol(), "B");
    }

    #[test]
    fn put_char_never_writes_out_of_buffer_with_huge_clip() {
        let mut buf = Buffer::new(4, 2);
        let theme = Theme::classic_blue();
        {
            // clip far larger than the buffer; construction intersects it down.
            let mut ctx = DrawCtx::new(
                &mut buf,
                &theme,
                Rect::new(0, 0, 1000, 1000),
                Point::new(0, 0),
            );
            // off the buffer edge -> dropped, no panic
            ctx.put_char(100, 100, 'Z', style(0xF, 0x1));
            ctx.put_char(3, 1, 'Q', style(0xF, 0x1));
        }
        assert_eq!(buf.get(3, 1).symbol(), "Q");
    }

    #[test]
    fn put_char_wide_at_clip_right_edge_degrades_to_space() {
        let mut buf = Buffer::new(10, 1);
        let theme = Theme::classic_blue();
        {
            // clip columns 0..3; place a wide glyph whose lead is at col 2,
            // so its trail (col 3) is outside the clip.
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 3, 1), Point::new(0, 0));
            ctx.put_char(2, 0, '中', style(0xF, 0x1));
        }
        assert_eq!(
            buf.get(2, 0).symbol(),
            " ",
            "wide lead with no room degrades to space"
        );
        assert!(!buf.get(2, 0).is_wide());
        assert_eq!(buf.get(3, 0).symbol(), " ", "outside clip untouched");
    }

    #[test]
    fn put_char_wide_with_room_sets_lead_and_trail() {
        let mut buf = Buffer::new(10, 1);
        let theme = Theme::classic_blue();
        {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 1), Point::new(0, 0));
            ctx.put_char(1, 0, '中', style(0xF, 0x1));
        }
        assert!(buf.get(1, 0).is_wide());
        assert_eq!(buf.get(1, 0).symbol(), "中");
        assert!(buf.get(2, 0).is_wide_trail());
    }

    #[test]
    fn put_str_writes_and_returns_columns() {
        let mut buf = Buffer::new(10, 2);
        let theme = Theme::classic_blue();
        let s = style(0xF, 0x1);
        let n = {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 2), Point::new(1, 0));
            ctx.put_str(0, 0, "hi", s)
        };
        assert_eq!(n, 2);
        assert_eq!(buf.get(1, 0).symbol(), "h");
        assert_eq!(buf.get(2, 0).symbol(), "i");
        assert_eq!(buf.get(1, 0).style(), s);
    }

    #[test]
    fn put_str_truncates_at_clip_right_edge() {
        let mut buf = Buffer::new(10, 1);
        let theme = Theme::classic_blue();
        let n = {
            // clip columns 0..4
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 4, 1), Point::new(0, 0));
            ctx.put_str(0, 0, "abcdefgh", style(0xF, 0x1))
        };
        assert_eq!(n, 4, "only the clip width is written");
        assert_eq!(buf.get(3, 0).symbol(), "d");
        // beyond the clip stays blank
        assert_eq!(buf.get(4, 0).symbol(), " ");
    }

    #[test]
    fn put_str_starting_left_of_clip_skips_offscreen_columns() {
        let mut buf = Buffer::new(10, 1);
        let theme = Theme::classic_blue();
        {
            // clip columns 2..10. Draw "abcdef" starting at absolute col 0:
            // columns 0,1 ('a','b') are off the clip left edge and skipped.
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(2, 0, 10, 1), Point::new(0, 0));
            ctx.put_str(0, 0, "abcdef", style(0xF, 0x1));
        }
        assert_eq!(buf.get(0, 0).symbol(), " ");
        assert_eq!(buf.get(1, 0).symbol(), " ");
        assert_eq!(buf.get(2, 0).symbol(), "c");
        assert_eq!(buf.get(3, 0).symbol(), "d");
    }

    #[test]
    fn put_cstr_toggles_style_on_tilde() {
        let mut buf = Buffer::new(10, 1);
        let theme = Theme::classic_blue();
        let lo = style(0xF, 0x1);
        let hi = style(0xA, 0x1);
        let n = {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 1), Point::new(0, 0));
            // "[~X~]" -> '[' and ']' in lo, 'X' in hi; tildes draw nothing.
            ctx.put_cstr(0, 0, "[~X~]", lo, hi)
        };
        assert_eq!(n, 3, "three visible columns advanced (the ~ draw nothing)");
        assert_eq!(buf.get(0, 0).symbol(), "[");
        assert_eq!(buf.get(0, 0).style(), lo);
        assert_eq!(buf.get(1, 0).symbol(), "X");
        assert_eq!(buf.get(1, 0).style(), hi, "between the ~ the style is hi");
        assert_eq!(buf.get(2, 0).symbol(), "]");
        assert_eq!(
            buf.get(2, 0).style(),
            lo,
            "after the closing ~ the style is lo"
        );
    }

    #[test]
    fn put_cstr_clips_like_put_char() {
        let mut buf = Buffer::new(10, 1);
        let theme = Theme::classic_blue();
        let lo = style(0xF, 0x1);
        let hi = style(0xA, 0x1);
        {
            // clip columns 0..2; "[~X~]" draws '[' at 0, 'X' at 1, ']' at 2 (clipped).
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 2, 1), Point::new(0, 0));
            ctx.put_cstr(0, 0, "[~X~]", lo, hi);
        }
        assert_eq!(buf.get(0, 0).symbol(), "[");
        assert_eq!(buf.get(1, 0).symbol(), "X");
        assert_eq!(buf.get(2, 0).symbol(), " ", "beyond the clip stays blank");
    }

    #[test]
    fn fill_clips_to_clip_rect() {
        let mut buf = Buffer::new(6, 4);
        let theme = Theme::classic_blue();
        let s = style(0x0, 0x3);
        {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(1, 1, 4, 3), Point::new(0, 0));
            // fill a local rect bigger than the clip
            ctx.fill(Rect::new(0, 0, 6, 4), '.', s);
        }
        // inside the clip
        assert_eq!(buf.get(1, 1).symbol(), ".");
        assert_eq!(buf.get(3, 2).symbol(), ".");
        // outside the clip untouched
        assert_eq!(buf.get(0, 0).symbol(), " ");
        assert_eq!(buf.get(4, 2).symbol(), " ");
        assert_eq!(buf.get(1, 1).style(), s);
    }

    #[test]
    fn sub_narrows_clip_and_shifts_origin() {
        let mut buf = Buffer::new(10, 10);
        let theme = Theme::classic_blue();
        {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 10), Point::new(0, 0));
            let mut child = ctx.sub(Rect::new(3, 2, 6, 5));
            assert_eq!(child.origin(), Point::new(3, 2));
            assert_eq!(child.clip(), Rect::new(3, 2, 6, 5));
            // child-local (0,0) -> absolute (3,2)
            child.put_char(0, 0, 'C', style(0xF, 0x1));
            // child-local write outside the child clip is dropped
            child.put_char(100, 100, 'X', style(0xF, 0x1));
        }
        assert_eq!(buf.get(3, 2).symbol(), "C");
    }

    #[test]
    fn sub_clip_intersects_parent() {
        let mut buf = Buffer::new(10, 10);
        let theme = Theme::classic_blue();
        {
            // parent clip 2..6 x 2..6
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(2, 2, 6, 6), Point::new(0, 0));
            // child local rect spans 0..10 -> intersect with parent clip
            let child = ctx.sub(Rect::new(0, 0, 10, 10));
            assert_eq!(child.clip(), Rect::new(2, 2, 6, 6));
        }
    }

    #[test]
    fn empty_clip_writes_nothing() {
        let mut buf = Buffer::new(5, 5);
        let theme = Theme::classic_blue();
        {
            // a clip that does not overlap the buffer at all
            let mut ctx = DrawCtx::new(
                &mut buf,
                &theme,
                Rect::new(100, 100, 200, 200),
                Point::new(0, 0),
            );
            assert!(ctx.clip().is_empty());
            ctx.put_char(0, 0, 'X', style(0xF, 0x1));
            ctx.put_str(0, 0, "hello", style(0xF, 0x1));
            ctx.fill(Rect::new(0, 0, 5, 5), '#', style(0xF, 0x1));
        }
        for cell in buf.cells() {
            assert_eq!(cell.symbol(), " ");
        }
    }

    // -- Context ------------------------------------------------------------

    #[test]
    fn context_post_and_broadcast_land_in_out_events() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            ctx.post(Command::OK);
            ctx.broadcast(Command::QUIT, None);
        }
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], Event::Command(Command::OK));
        assert_eq!(
            out[1],
            Event::Broadcast {
                command: Command::QUIT,
                source: None
            }
        );
    }

    #[test]
    fn context_set_and_kill_timer() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let id = {
            let mut ctx = Context::new(&mut out, &mut timers, 100, &mut deferred);
            assert_eq!(ctx.now_ms(), 100);
            ctx.set_timer(Duration::from_millis(50), None)
        };
        assert_eq!(timers.len(), 1);
        {
            let mut ctx = Context::new(&mut out, &mut timers, 100, &mut deferred);
            ctx.kill_timer(id);
        }
        assert_eq!(timers.len(), 0);
    }

    #[test]
    fn context_command_changes_queue_enable_and_disable() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            ctx.enable_command(Command::OK);
            ctx.disable_command(Command::CANCEL);
        }
        assert_eq!(deferred.len(), 2);
        assert!(matches!(deferred[0], Deferred::EnableCommand(Command::OK)));
        assert!(matches!(
            deferred[1],
            Deferred::DisableCommand(Command::CANCEL)
        ));
    }

    #[test]
    fn context_end_modal_queues_deferred() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            ctx.end_modal(Command::CANCEL);
        }
        assert_eq!(deferred.len(), 1);
        assert!(matches!(deferred[0], Deferred::EndModal(Command::CANCEL)));
    }

    #[test]
    fn context_owner_size_defaults_zero_and_round_trips() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        // Context::new defaults owner_size to (0, 0).
        assert_eq!(ctx.owner_size(), Point::new(0, 0));
        // The setter round-trips.
        ctx.set_owner_size(Point::new(80, 25));
        assert_eq!(ctx.owner_size(), Point::new(80, 25));
    }
}
