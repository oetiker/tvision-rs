//! The interactive menu layer — opening, navigating, and selecting menus — driven
//! by one [`MenuSession`] capture handler on the single event loop.
//!
//! ## The architecture
//!
//! Opening a submenu would normally nest one modal loop per open box. Here there
//! is a single event loop, so **one** [`MenuSession`] capture handler owns the
//! WHOLE open stack (bar + every open box) and runs the interaction as one flat
//! loop rather than recursion. While the session is on the capture stack it
//! **consumes every menu-directed event**: the bar and boxes only draw and report
//! item geometry — they are never focused and carry no event logic of their own.
//!
//! ## State
//!
//! The session holds a **stack of levels** ([`MenuLevel`]), one per open bar/box,
//! each carrying its `view_id`, a **clone** of its `menu`, its `current` highlight
//! index, its `bounds` (cached at open — a box never moves), and an `is_bar` flag
//! (a bar is one row, a box a vertical column). Cloning the menu at open freezes
//! its `disabled` flags for the menu's lifetime — the session swallows
//! command-set-changed broadcasts while active, so regray does not reach an open
//! menu. The top of the stack is the **active** level; a parent level is
//! suspended. Cross-level mouse hit-testing gates against the cached per-level
//! `bounds`.
//!
//! **Behavioral note (a consequence of cloning at open):** writing the chosen item
//! back as the level's default lands on the *level's clone*, which is discarded
//! when the session closes — so the bar's permanent [`Menu::default`] is NOT
//! updated, and a later re-activation restarts on the original default rather than
//! the last-selected item. This is inert within a session (a closed box is only
//! reopened by re-cloning through the bar), the same trade-off as the
//! `disabled`-freezing above. Persisting the default would write the chosen index
//! back to the bar's real menu on close, via a deferred request mirroring the
//! existing highlight write.
//!
//! ## Keyboard
//!
//! The keyboard arms cover Up/Down/Left/Right/Home/End/Enter/Esc plus a
//! character / hotkey arm. Choosing a submenu pushes a level; choosing a command
//! ends the session and posts the command; Esc or Left close levels.
//!
//! ## Mouse
//!
//! The mouse arms cover press/release/move plus item tracking and the parent-level
//! hit gates, with per-level transient flags for the "last opened title", "mouse
//! has landed on an item", and "first event of this level". Keyboard and mouse
//! share one re-apply [`run`](MenuSession::run) loop tail (set highlight → reset
//! the last-opened marker → open gate → command result → pop / re-apply). The open
//! gate re-applies the triggering mouse event into the freshly-opened child; the
//! child-pop records the parent's last-opened title and default (the "click an
//! open title to close it" mechanism). The bar's mouse-down activation lives in
//! [`menu_view::handle_event`] ([`activate_mouse`]). The per-session
//! [`put_click_event_on_exit`](MenuSession) flag (`true` for a bar/box, `false`
//! for a context popup) gates the bottom-level exit-click re-post.
//!
//! ## Context popups
//!
//! A context popup is a menu box with two observable differences: no default
//! highlight on open (the level starts with nothing highlighted and clears its
//! clone's [`Menu::default`]) and no exit-click re-post (the [`popup_menu`]
//! constructor sets [`put_click_event_on_exit`](MenuSession) to `false`). The
//! [`popup_menu`] free function builds and runs it (placement via
//! [`auto_place_popup`]). A popup needs no extra accelerator handling of its own:
//! it is run modally the instant it opens, so the session already owns the event
//! loop and the flat loop's hotkey handling covers it (see [`popup_menu`]).
//!
//! Mouse auto-repeat / press-and-hold does not apply to menus.
//!
//! # Turbo Vision heritage
//! Flattens `TMenuView::execute()` and `TMenuPopup` (`tmnuview.cpp`,
//! `tmenupop.cpp`, `popupmnu.cpp`/`menus.h`). The nested modal loops (one per open
//! box) become one capture handler on the single event loop (deviation D9).

use crate::capture::{CaptureFlow, CaptureHandler};
use crate::command::Command;
use crate::event::{Event, Key, KeyEvent, MouseEvent};
use crate::help::HelpCtx;
use crate::menu::menu_box::menu_box_rect;
use crate::menu::menu_view::hot_key;
use crate::menu::{Menu, MenuItem};
use crate::view::{Context, Point, Rect, ViewId};

/// One open bar/box level of the menu stack, made explicit so the single loop can
/// own every open level at once.
struct MenuLevel {
    /// The bar/box view's id in the root group (resolves to a
    /// [`MenuBar`](crate::menu::MenuBar)/[`MenuBox`](crate::menu::MenuBox) for the
    /// highlight-write and close brokers). The bar's id is real; each box id is
    /// **pre-minted** by the session before [`Deferred::OpenMenuBox`].
    view_id: ViewId,
    /// A clone of the level's menu. Cloning at open freezes its `disabled` flags
    /// for the menu's lifetime — the session swallows regray broadcasts while open.
    menu: Menu,
    /// The highlighted item index, or `None` for nothing highlighted.
    current: Option<usize>,
    /// The level's bounds in the root group's frame, cached at open (a box never
    /// moves). Used to compute a child box's geometry and to gate mouse
    /// hit-testing.
    bounds: Rect,
    /// Whether this level is the one-row horizontal bar (vs a vertical box).
    is_bar: bool,
    /// Whether a no-op step whose highlight names a submenu should open that
    /// submenu. **Per level**, reset at every level entry so it never leaks from
    /// the bar into a box's navigation. Set true on this level's Down / Enter /
    /// hotkey match. This is what makes a Left/Right walk along the bar **re-open**
    /// the adjacent title's box.
    auto_select: bool,
    /// The item whose submenu was most recently opened **from this level**, set
    /// when the child box pops back. **Per level**, reset at every level entry.
    /// Drives the "click an open title to close it" behaviour (the mouse-down and
    /// mouse-up arms read it to decide whether a click on an already-open title
    /// closes it). The keyboard arms never read it. Mouse-only.
    last_target_item: Option<usize>,
    /// Whether the mouse has landed on an item of this level. **Per level**, set by
    /// [`track_mouse`](MenuSession::track_mouse) and **monotonic** — never reset to
    /// false within a level's lifetime. Gates the "released outside after
    /// activating" mouse-up arm and the bar drag-open mouse-move arm. Mouse-only.
    mouse_active: bool,
    /// Whether this level has yet to finish processing its first event (the
    /// re-applied triggering event after an open counts). **Per level**, true at
    /// level entry, cleared after each loop iteration. Guards exactly one thing: a
    /// box just opened by a press must NOT be instantly closed by the re-applied
    /// press. Mouse-only.
    first_event: bool,
}

impl MenuLevel {
    /// The rect of item `index` for this level, in **view-local** coordinates —
    /// the same contract as
    /// [`MenuBar::get_item_rect`](crate::menu::MenuView::get_item_rect) /
    /// [`MenuBox::get_item_rect`](crate::menu::MenuView::get_item_rect), but
    /// computed from the cached `menu` + `bounds` (the session has no view
    /// reference). Must agree cell-for-cell with the draw layer.
    fn item_rect_local(&self, index: usize) -> Rect {
        if self.is_bar {
            // TMenuBar::getItemRect (tmenubar.cpp:94): horizontal accumulator.
            let mut r = Rect::new(1, 0, 1, 1);
            for (i, item) in self.menu.items.iter().enumerate() {
                r.a.x = r.b.x;
                if !matches!(item, MenuItem::Separator) {
                    r.b.x += cstrlen(item_name(item)) + 2;
                }
                if i == index {
                    return r;
                }
            }
            r
        } else {
            // TMenuBox::getItemRect (tmenubox.cpp:125): rows from y = 1.
            let y = 1 + index as i32;
            let size_x = self.bounds.b.x - self.bounds.a.x;
            Rect::new(2, y, size_x - 2, y + 1)
        }
    }
}

/// Display width of a `~`-marked label, ignoring the `~` markers (per-module
/// copy, as in `menu_bar.rs`/`menu_box.rs`).
fn cstrlen(s: &str) -> i32 {
    s.chars()
        .filter(|&c| c != '~')
        .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(1) as i32)
        .sum()
}

/// The display label of a named item (empty for a [`Separator`](MenuItem::Separator)).
fn item_name(item: &MenuItem) -> &str {
    match item {
        MenuItem::Command { name, .. } | MenuItem::SubMenu { name, .. } => name,
        MenuItem::Separator => "",
    }
}

/// The modal menu interaction, run as one capture handler on the single event
/// loop.
///
/// Pushed at activation (via
/// [`Deferred::PushCapture`](crate::view::Deferred::PushCapture)) alongside the
/// first [`OpenMenuBox`](crate::view::Deferred::OpenMenuBox) — or, for a bar
/// activation, with only the bar level on the stack and no box yet (the first key
/// opens a box). Consumes every event it is offered; pops itself
/// ([`CaptureFlow::ConsumedPop`]) when the last level closes, restoring the
/// pre-menu focus.
///
/// # Turbo Vision heritage
/// Flattens `TMenuView::execute()` (`tmnuview.cpp`/`menus.h`). The nested modal
/// loops (one per open box) become a single capture handler on the one event loop
/// (deviation D9).
pub struct MenuSession {
    /// The open levels, bottom (bar) → top (deepest box). The top is the active
    /// level.
    levels: Vec<MenuLevel>,
    /// The owner (root group) size, used as the bounds hint when sizing a submenu
    /// box. Captured at activation.
    owner_size: Point,
    /// Which arm (press/release/move) the in-flight mouse event selects, set by
    /// [`run`](Self::run) before each [`step_mouse`](Self::step_mouse) so the three
    /// arms can branch without re-threading the whole [`Event`] (the
    /// position/buttons ride on the [`MouseEvent`](crate::event::MouseEvent) passed
    /// to `step_mouse`). Unused for the keyboard path.
    mouse_kind: MouseKind,
    /// Whether an exit-click should be re-posted to the view tree. An exit-click is
    /// a mouse press outside the menu that ends the session; re-posting it lets the
    /// view under it recover focus. `true` for a bar/box, `false` for a context
    /// popup (a popup never re-posts its exit-click). Only the bottom level's value
    /// matters — an intermediate box's exit-click is carried up by the re-apply
    /// loop, not re-posted — so this is one session-wide flag, not a per-level
    /// field.
    put_click_event_on_exit: bool,
}

/// Which mouse arm the in-flight event selects.
#[derive(Clone, Copy)]
enum MouseKind {
    /// A button press.
    Down,
    /// A button release.
    Up,
    /// A move (drag).
    Move,
}

/// What a single step decided, used internally to drive the post-step logic.
#[derive(PartialEq)]
enum MenuAction {
    /// Stay open; redraw if the highlight changed.
    Nothing,
    /// Try to open the highlighted submenu, or select its command.
    Select,
    /// This level returns (close it; if it was the bar, end the session).
    Return,
}

impl MenuSession {
    /// Build a session over an initial level stack. `owner_size` is the root group
    /// size. Use [`activate`] rather than calling this directly — it assembles the
    /// level + the first deferred batch.
    ///
    /// There is no saved focus: the bar and boxes are **never focused** (the
    /// session consumes every event on the capture stack, before view-tree
    /// routing), so the pre-menu highlight is never disturbed and there is nothing
    /// to restore on close.
    fn new(levels: Vec<MenuLevel>, owner_size: Point) -> Self {
        MenuSession {
            levels,
            owner_size,
            mouse_kind: MouseKind::Down,
            // The bar/box default; `popup_menu` flips it false for a context popup.
            put_click_event_on_exit: true,
        }
    }

    /// The active (top) level. The session is never empty while on the stack
    /// (popping the last level returns `ConsumedPop`).
    fn top(&self) -> &MenuLevel {
        self.levels.last().expect("session has at least one level")
    }

    fn top_mut(&mut self) -> &mut MenuLevel {
        self.levels
            .last_mut()
            .expect("session has at least one level")
    }

    // -- mouse geometry + gates (tmnuview.cpp:97-166) -----------------------

    /// The rect of item `index` for `level` in the **root group frame** — the
    /// view-local [`item_rect_local`](MenuLevel::item_rect_local) offset by the
    /// level's origin (`level.bounds.a`). The mouse arms compare against
    /// root-frame positions, and the incoming event is already root-frame, so
    /// offsetting the rect by the origin is all that is needed.
    fn item_rect_global(level: &MenuLevel, index: usize) -> Rect {
        let r = level.item_rect_local(index);
        let o = level.bounds.a;
        Rect::new(r.a.x + o.x, r.a.y + o.y, r.b.x + o.x, r.b.y + o.y)
    }

    /// Whether `level`'s bounds contain the root-frame `pos`.
    fn mouse_in_view(level: &MenuLevel, pos: Point) -> bool {
        level.bounds.contains(pos)
    }

    /// Whether the **parent** level's highlighted-item rect contains `pos`. The
    /// parent is the level just below the top (a box always has the bar or another
    /// box above it); with no parent, or a parent with nothing highlighted, the
    /// result is false.
    fn mouse_in_owner(&self, pos: Point) -> bool {
        let n = self.levels.len();
        if n < 2 {
            return false; // no parent level
        }
        let parent = &self.levels[n - 2];
        match parent.current {
            Some(cur) => Self::item_rect_global(parent, cur).contains(pos),
            None => false,
        }
    }

    /// Whether ANY **parent** level (every level except the top) contains `pos` in
    /// its bounds.
    fn mouse_in_menus(&self, pos: Point) -> bool {
        let n = self.levels.len();
        self.levels[..n - 1]
            .iter()
            .any(|l| Self::mouse_in_view(l, pos))
    }

    /// Set the top level's highlight to the item whose rect contains `pos` (and
    /// mark the mouse active), or to nothing if no item is hit. All items are
    /// tested, separators included: in a **box** a separator has a full-width row
    /// rect and CAN be hit (the up/down arms then treat it as "not a real target"),
    /// while on the **bar** a separator's rect is zero-width and is never hit.
    fn track_mouse(&mut self, pos: Point) {
        let n = self.top().menu.items.len();
        for i in 0..n {
            if Self::item_rect_global(self.top(), i).contains(pos) {
                let top = self.top_mut();
                top.current = Some(i);
                top.mouse_active = true;
                return;
            }
        }
        self.top_mut().current = None; // nothing hit
    }

    // -- nav primitives -----------------------------------------------------

    /// Advance the active level's highlight, wrapping to the first item at the end.
    /// Nothing highlighted bootstraps to the first item; an empty menu stays at
    /// nothing.
    fn next_item(&mut self) {
        let n = self.top().menu.items.len();
        if n == 0 {
            return;
        }
        let cur = self.top().current;
        let next = match cur {
            None => 0,
            Some(i) if i + 1 >= n => 0, // wrap to first
            Some(i) => i + 1,
        };
        self.top_mut().current = Some(next);
    }

    /// Retreat the active level's highlight by one, wrapping the first item to the
    /// last. Nothing highlighted bootstraps to the last item.
    fn prev_item(&mut self) {
        let n = self.top().menu.items.len();
        if n == 0 {
            return;
        }
        let cur = self.top().current;
        let prev = match cur {
            None => n - 1,
            Some(0) => n - 1, // first → wrap to last
            Some(i) => i - 1,
        };
        self.top_mut().current = Some(prev);
    }

    /// Move the active level's highlight to the next or previous **non-separator**
    /// item.
    ///
    /// With nothing highlighted, it bootstraps to the first item (or the last, when
    /// going backward) and returns immediately if that item is not a separator;
    /// otherwise it steps in the chosen direction, skipping separators.
    fn track_key(&mut self, find_next: bool) {
        if self.top().current.is_none() {
            self.top_mut().current = Some(0);
            if !find_next {
                self.prev_item();
            }
            // A named first/last item needs no skip.
            if !self.current_is_separator() {
                return;
            }
        }
        // Step until the highlight lands on a non-separator.
        loop {
            if find_next {
                self.next_item();
            } else {
                self.prev_item();
            }
            if !self.current_is_separator() {
                break;
            }
        }
    }

    /// Whether the active level's highlight points at a separator. An
    /// out-of-range / absent highlight is treated as non-separator so the nav loops
    /// terminate (the menu is assumed non-empty when a nav key arrives).
    fn current_is_separator(&self) -> bool {
        match self.top().current {
            Some(i) => matches!(self.top().menu.items.get(i), Some(MenuItem::Separator)),
            None => false,
        }
    }

    /// The first **enabled, named** item on the active level whose hotkey letter
    /// matches a plain (no-alt) `ke`. Delegates to the shared
    /// [`menu_view::matching_item`] walk.
    fn find_item(&self, ke: &KeyEvent) -> Option<usize> {
        crate::menu::menu_view::matching_item(&self.top().menu, ke, false)
    }

    // -- the per-event step = one iteration of execute()'s do/while ---------

    /// One keyboard step on the active level. Returns an `(action, cleared)` pair:
    /// `cleared` is whether the event is consumed here — when it is `false` and the
    /// action is Return, the re-apply loop re-delivers the SAME event to the parent
    /// level (so one press can unwind the whole stack up to the bar). The step may
    /// mutate the level's highlight and `auto_select`; `pending_command` carries a
    /// hotkey accelerator result.
    fn step_keyboard(
        &mut self,
        k: KeyEvent,
        pending_command: &mut Option<Command>,
    ) -> (MenuAction, bool) {
        let is_bar = self.top().is_bar;
        match k.key {
            // kbUp / kbDown (tmnuview.cpp:280): box navigates; bar's kbDown sets
            // autoSelect = True (the open-gate then opens the current submenu —
            // action stays doNothing so the flag PERSISTS for a later Left/Right
            // walk, Blocker 3). A non-named key consumes (cleared) the event.
            Key::Up | Key::Down => {
                if !is_bar {
                    self.track_key(k.key == Key::Down);
                } else if k.key == Key::Down {
                    self.top_mut().auto_select = true;
                }
                (MenuAction::Nothing, true)
            }
            // kbLeft / kbRight (tmnuview.cpp:287): bar trackKeys to the adjacent
            // title; a box (parentMenu != 0) returns WITHOUT clearEvent → the
            // re-apply loop unwinds every open box to the bar, which then walks +
            // re-opens the neighbour (Blocker 3).
            Key::Left | Key::Right => {
                if is_bar {
                    self.track_key(k.key == Key::Right);
                    (MenuAction::Nothing, true)
                } else {
                    // parentMenu != 0 (always, a box has the bar/another box above)
                    // → doReturn, NOT cleared.
                    (MenuAction::Return, false)
                }
            }
            // kbHome / kbEnd (tmnuview.cpp:294): box only.
            Key::Home | Key::End => {
                if !is_bar {
                    self.top_mut().current = Some(0);
                    if k.key == Key::End {
                        self.track_key(false);
                    }
                }
                (MenuAction::Nothing, true)
            }
            // kbEnter (tmnuview.cpp:303): doSelect; the bar also sets autoSelect.
            Key::Enter => {
                if is_bar {
                    self.top_mut().auto_select = true;
                }
                (MenuAction::Select, true)
            }
            // kbEsc (tmnuview.cpp:308-312): doReturn. clearEvent runs iff
            // `parentMenu == 0 || parentMenu->size.y != 1` — i.e. cleared at the
            // bar OR at a 2nd+-level box (parent is a box), but NOT at a 1st-level
            // box (parent is the bar, size.y == 1). When not cleared the re-apply
            // loop carries the Esc up to the bar, closing the whole menu on one
            // press (Blocker 2). The asymmetry IS this guard, not a mouse concern.
            Key::Esc => {
                let cleared = self.esc_clear_event();
                (MenuAction::Return, cleared)
            }
            // default (tmnuview.cpp:313): alt-shortcut on the TOP menu, else a
            // plain char findItem, else a hotKey accelerator.
            _ => self.step_default_key(k, pending_command),
        }
    }

    /// Whether an Esc press at the active level should be consumed here rather than
    /// carried up. It is consumed at the bar (no level below) or at a second-or-
    /// deeper box (whose parent is another box), but NOT at a first-level box
    /// (whose parent is the bar) — there the Esc is carried up so one press closes
    /// the whole menu.
    fn esc_clear_event(&self) -> bool {
        let depth = self.levels.len();
        if depth <= 1 {
            // The bar: no parent → consume here.
            true
        } else {
            // A box: consume iff the parent is NOT the bar (a 2nd+-level box).
            !self.levels[depth - 2].is_bar
        }
    }

    /// The fallthrough key arm: a hotkey on the active level (or an Alt-shortcut on
    /// the bar), else a bar accelerator. Returns `(action, cleared)`.
    fn step_default_key(
        &mut self,
        k: KeyEvent,
        pending_command: &mut Option<Command>,
    ) -> (MenuAction, bool) {
        // C++: target = this; if Alt-char, target = topMenu(), p = findAltShortcut
        // on the bar; else p = findItem on THIS level.
        if k.modifiers.alt {
            // Alt-shortcut dispatches against the TOP menu (the bar, level 0).
            if let Some(idx) = self.find_alt_shortcut_bar(&k) {
                // C++ `tmnuview.cpp:331-340`: if target == this (the active level IS
                // the bar) → if size.y==1 autoSelect=True; doSelect; current=p.
                // Otherwise (a box is active) → doReturn (not cleared) so the
                // re-apply loop unwinds toward the bar, which re-resolves.
                if self.top().is_bar {
                    self.top_mut().current = Some(idx);
                    self.top_mut().auto_select = true;
                    return (MenuAction::Select, true);
                } else {
                    return (MenuAction::Return, false);
                }
            }
        } else if let Some(idx) = self.find_item(&k) {
            // findItem matched on THIS (active) level → target == this → select it.
            // (size.y==1 → autoSelect=True, harmless on a box where it is unused.)
            self.top_mut().current = Some(idx);
            if self.top().is_bar {
                self.top_mut().auto_select = true;
            }
            return (MenuAction::Select, true);
        }
        // No item match: try the bar's hotKey accelerator (topMenu()->hotKey).
        if let Some(cmd) = hot_key(&self.levels[0].menu, k) {
            // commandEnabled is backstopped by the pump's drop_disabled filter;
            // hot_key already skips cached-disabled items. The result
            // path ends the session (clearEvent runs, tmnuview.cpp:395).
            *pending_command = Some(cmd);
            return (MenuAction::Return, true);
        }
        // No match at all: consume (a stray key in a modal menu does nothing).
        (MenuAction::Nothing, true)
    }

    /// The matched top-level item index for an `Alt`-held `ke`, searched against
    /// the **bar** (the bottom level). Delegates to the shared
    /// [`menu_view::matching_item`] walk.
    fn find_alt_shortcut_bar(&self, ke: &KeyEvent) -> Option<usize> {
        crate::menu::menu_view::matching_item(&self.levels[0].menu, ke, true)
    }

    /// One mouse step (press/release/move) on the active level. Mirrors
    /// [`step_keyboard`](Self::step_keyboard)'s `(action, cleared)` contract,
    /// widened with a third `exit_click` flag: no mouse step ever consumes a Return
    /// itself, so a box's Return is always carried up to its parent by the re-apply
    /// loop; `exit_click` marks the press-outside branch so the loop tail re-posts
    /// the click to the view tree when the bottom level ends from it AND that
    /// level's [`put_click_event_on_exit`](MenuSession) is set. Mutates the top
    /// level's highlight, `auto_select`, `mouse_active`, and `last_target_item`.
    fn step_mouse(&mut self, m: MouseEvent) -> (MenuAction, bool, bool) {
        let pos = m.position;
        let is_bar = self.top().is_bar;
        let mut action = MenuAction::Nothing;
        let mut exit_click = false;

        match self.mouse_kind {
            MouseKind::Down => {
                // evMouseDown (tmnuview.cpp:201).
                if Self::mouse_in_view(self.top(), pos) || self.mouse_in_owner(pos) {
                    self.track_mouse(pos); // sets top.current (maybe None) + mouse_active
                    if is_bar {
                        // autoSelect makes a click OPEN the clicked title's box, yet
                        // CLOSE it on the second click of the same title (after a box
                        // closed it set last_target_item == that title).
                        let cur = self.top().current;
                        self.top_mut().auto_select =
                            cur.is_none() || self.top().last_target_item != cur;
                    } else if !self.top().first_event && self.mouse_in_owner(pos) {
                        // A box closes when the press lands on its parent's title,
                        // except when the box was just opened (firstEvent guard).
                        action = MenuAction::Return;
                    }
                    // (otherwise action stays doNothing; the open-gate may still fire
                    // via auto_select for a bar click.)
                } else {
                    // Click outside this level's bounds and outside the parent item:
                    // the menu closes. The exit click is flagged here; the loop tail
                    // re-posts it to the view tree only when the bottom level ends from
                    // it AND its `put_click_event_on_exit` is set (the bar/box default;
                    // a TMenuPopup clears it). An intermediate box just returns and the
                    // re-apply loop carries the click up to the bottom level.
                    action = MenuAction::Return;
                    exit_click = true;
                }
            }
            MouseKind::Up => {
                // evMouseUp (tmnuview.cpp:225) — always trackMouse first (no gate).
                self.track_mouse(pos);
                if self.mouse_in_owner(pos) {
                    // Released on the parent item → reset to the menu default.
                    self.top_mut().current = self.top().menu.default;
                } else if let Some(cur) = self.top().current {
                    // A named (non-separator) item: select / close / re-arm.
                    if !matches!(self.top().menu.items.get(cur), Some(MenuItem::Separator)) {
                        if Some(cur) != self.top().last_target_item {
                            action = MenuAction::Select;
                        } else if is_bar {
                            // A bar entry just closed → exit and stop listening.
                            action = MenuAction::Return;
                        } else {
                            // A box: MouseUp won't reopen a submenu just closed by a
                            // name-click; but the NEXT one will (clear last_target).
                            self.top_mut().last_target_item = None;
                        }
                    }
                    // A separator (name == 0): nothing — action stays doNothing.
                } else if self.top().mouse_active && !Self::mouse_in_view(self.top(), pos) {
                    // Released outside the view after activating → return.
                    action = MenuAction::Return;
                } else if !is_bar {
                    // Released inside the box but not on a highlightable entry (a
                    // margin / separator): highlight the default, else the first
                    // (TV 2.0). Nonsensical in a bar, so bar-only-excluded.
                    self.top_mut().current = self.top().menu.default.or(Some(0));
                }
            }
            MouseKind::Move => {
                // evMouseMove (tmnuview.cpp:263) — only while a button is held.
                if m.buttons.left || m.buttons.right || m.buttons.middle {
                    self.track_mouse(pos);
                    if !(Self::mouse_in_view(self.top(), pos) || self.mouse_in_owner(pos))
                        && self.mouse_in_menus(pos)
                    {
                        // Dragged off this box onto an ancestor menu → return.
                        action = MenuAction::Return;
                    } else if is_bar
                        && self.top().mouse_active
                        && self.top().current != self.top().last_target_item
                    {
                        // Drag to a new bar title → open it automatically.
                        self.top_mut().auto_select = true;
                    }
                }
                // buttons == 0 → no-op (action doNothing).
            }
        }
        (action, false, exit_click)
    }

    /// The menu-toggle command step on the active level: it resets the level's
    /// transient mouse flags, then a **box** returns (not consumed — the tail
    /// carries the command up, unwinding toward the bar) while the **bar** just
    /// resets and stays open. Mirrors the `(action, cleared)` step contract.
    fn step_cmd_menu(&mut self) -> (MenuAction, bool) {
        let is_bar = self.top().is_bar;
        let top = self.top_mut();
        top.auto_select = false;
        top.last_target_item = None;
        if is_bar {
            (MenuAction::Nothing, true)
        } else {
            // parentMenu != 0 → doReturn, not cleared (the tail re-posts cmMenu up).
            (MenuAction::Return, false)
        }
    }

    /// The flat event loop, **shared** by the keyboard and mouse paths. Steps the
    /// active level (by event kind), runs the post-step open gate, and on a
    /// not-consumed Return pops the level and **re-applies the SAME event** to the
    /// new top level, looping until a level produces a non-Return action (or a
    /// consumed Return), or the bar ends the whole session. This re-apply-upward is
    /// what replaces the recursion that would otherwise nest one loop per open box.
    fn run(&mut self, ev: Event, ctx: &mut Context) -> CaptureFlow {
        // Cache the event kind for the per-iteration step + the open-gate's
        // mouse-down/move `continue` divergence (tmnuview.cpp:374).
        let is_mouse_carry = matches!(ev, Event::MouseDown(_) | Event::MouseMove(_));
        loop {
            let mut pending_command = None;
            let (action, cleared, exit_click) = match ev {
                Event::KeyDown(k) => {
                    let (a, c) = self.step_keyboard(k, &mut pending_command);
                    (a, c, false)
                }
                Event::MouseDown(m) => {
                    self.mouse_kind = MouseKind::Down;
                    self.step_mouse(m)
                }
                Event::MouseUp(m) => {
                    self.mouse_kind = MouseKind::Up;
                    self.step_mouse(m)
                }
                Event::MouseMove(m) => {
                    self.mouse_kind = MouseKind::Move;
                    self.step_mouse(m)
                }
                // evCommand cmMenu (tmnuview.cpp:343-350): a box doReturns (re-applies
                // up, the tail's `putEvent(e)` for an evCommand), the bar resets +
                // stays. Routed through run() so it shares the doReturn pop/re-apply.
                Event::Command(Command::MENU) => {
                    let (a, c) = self.step_cmd_menu();
                    (a, c, false)
                }
                // run() is only entered for the step-bearing kinds.
                _ => unreachable!("run() dispatches only keyboard/mouse/cmMenu events"),
            };

            // Post the (possibly changed) highlight of the active level to its view
            // (execute()'s `if itemShown != current drawView`, tmnuview.cpp:362).
            let top_id = self.top().view_id;
            let top_current = self.top().current;
            ctx.request_set_menu_current(top_id, top_current);

            // Post-switch reset (tmnuview.cpp:359): if a submenu was closed by a
            // name-click and the mouse is dragged to another entry, the submenu
            // opens again the next time it is hovered. Runs every iteration, before
            // the open-gate, on the TOP level (inert for keyboard, which never sets
            // last_target_item).
            if self.top().last_target_item != self.top().current {
                self.top_mut().last_target_item = None;
            }

            // Post-switch open-gate (tmnuview.cpp:368-390):
            //   (doSelect || (doNothing && autoSelect)) && current names a NAMED
            //   item → open its submenu (any of the two), or select its command
            //   (doSelect only).
            let auto = self.top().auto_select;
            let gate = action == MenuAction::Select || (action == MenuAction::Nothing && auto);
            if gate && let Some(idx) = self.top().current {
                match self.top().menu.items.get(idx) {
                    // A submenu, not disabled → open a child box (recurse).
                    Some(MenuItem::SubMenu { menu, disabled, .. }) if !*disabled => {
                        let submenu = menu.clone();
                        self.open_submenu(idx, submenu, ctx);
                        // C++ putEvent(e) into the child's frame is gated on
                        // (evMouseDown | evMouseMove) (tmnuview.cpp:374): re-apply the
                        // SAME mouse-down/move to the freshly-opened child (its
                        // first_event == true guards the instant-close). Keyboard +
                        // mouseUp: the child opens and waits.
                        if is_mouse_carry {
                            continue;
                        }
                        return CaptureFlow::Consumed;
                    }
                    // A command item, not disabled → select it ONLY on doSelect
                    // (the autoSelect branch never selects a command,
                    // tmnuview.cpp:388). Post + end the whole session.
                    Some(MenuItem::Command {
                        command, disabled, ..
                    }) if !*disabled && action == MenuAction::Select => {
                        let cmd = *command;
                        return self.end_session_with(Some(cmd), ctx);
                    }
                    _ => {}
                }
            }

            // A hotKey accelerator (`topMenu()->hotKey`) is a COMMAND RESULT: it
            // propagates up through every nested execView and closes the WHOLE
            // menu, posting the command, regardless of depth (`tmnuview.cpp:392`).
            // Check it BEFORE the per-level Return-pop, else a deep hotKey would be
            // dropped (the box-level pop returns Consumed without posting).
            // Esc/Left/Right/mouse carry no pending_command, so they fall through.
            if let Some(cmd) = pending_command {
                return self.end_session_with(Some(cmd), ctx);
            }

            // doReturn — close the active level; re-apply upward unless cleared.
            if action == MenuAction::Return {
                if self.levels.len() > 1 {
                    // Pop + close the top box; the parent becomes active. C++
                    // `execView` returns here → set the parent's lastTargetItem /
                    // menu.default to its current (the "click an open title to close
                    // it" crux, tmnuview.cpp:385-386) and flip firstEvent
                    // (tmnuview.cpp:400, runs after execView returns).
                    let top = self.levels.pop().expect("len > 1");
                    ctx.request_close(top.view_id);
                    let parent = self.top_mut();
                    if let Some(cur) = parent.current {
                        parent.last_target_item = Some(cur);
                        parent.menu.default = Some(cur);
                    }
                    parent.first_event = false;
                    if cleared {
                        // clearEvent → stop; the parent stays open.
                        return CaptureFlow::Consumed;
                    }
                    // Not cleared → re-apply the SAME event to the new top level.
                    continue;
                } else {
                    // The bottom level returned → end the session. For an exit-click
                    // (a mouse-down outside the menu), re-post the click to the view
                    // tree so the view under it recovers focus — but ONLY when this
                    // bottom level's `putClickEventOnExit` is set (the bar/box default;
                    // a `TMenuPopup` clears it, `tmenupop.cpp:45`, so a popup's
                    // exit-click is swallowed, `tmnuview.cpp:220-222`). The final-tail
                    // putEvent does NOT fire (parentMenu == 0 && e.what != evCommand).
                    let r = self.end_session_with(None, ctx);
                    if exit_click && self.put_click_event_on_exit {
                        ctx.put_event(ev);
                    }
                    return r;
                }
            }

            // doNothing with no open → consume; the active level stays open. Flip
            // first_event (a level that processed an event without opening a child or
            // getting popped is no longer on its first event, tmnuview.cpp:400).
            self.top_mut().first_event = false;
            return CaptureFlow::Consumed;
        }
    }

    /// Open the submenu at `index` of the active level as a new child box level.
    /// Pre-mints the box id, computes its geometry (just below the parent item),
    /// and queues [`OpenMenuBox`](crate::view::Deferred::OpenMenuBox). The new level
    /// starts highlighted on its own default with its transient flags freshly
    /// initialized.
    fn open_submenu(&mut self, index: usize, submenu: Menu, ctx: &mut Context) {
        // Geometry block (tmnuview.cpp:376-381):
        //   r = getItemRect(current);          // view-local
        //   r.a.x = r.a.x + origin.x;
        //   r.a.y = r.b.y + origin.y;          // BELOW the item
        //   r.b = owner->size;
        //   if (size.y == 1) r.a.x--;          // bar shift
        let parent = self.top();
        let origin = parent.bounds.a;
        let r = parent.item_rect_local(index);
        let mut hint = Rect::new(
            r.a.x + origin.x,
            r.b.y + origin.y,
            self.owner_size.x,
            self.owner_size.y,
        );
        if parent.is_bar {
            hint.a.x -= 1;
        }
        // The box sizes itself inside this hint (menu_box_rect clamps).
        let bounds = menu_box_rect(hint, &submenu);

        // Pre-mint the box id so the session knows it with no callback.
        let id = ViewId::next();
        ctx.request_open_menu_box(id, submenu.clone(), bounds);

        // The new box level starts with current = menu->deflt (execute()'s
        // prologue runs on the freshly entered frame: `current = menu->deflt`) and
        // its OWN autoSelect = False (per-level; C++ inits it False each frame).
        let current = submenu.default;
        self.levels.push(MenuLevel {
            view_id: id,
            menu: submenu,
            current,
            bounds,
            is_bar: false,
            auto_select: false,
            // The mouse loop-locals, re-init per level (C++ inits them at every
            // execute() frame entry, so they never leak across levels).
            last_target_item: None,
            mouse_active: false,
            first_event: true,
        });
        // Push the new level's initial highlight to its (about-to-exist) box.
        ctx.request_set_menu_current(id, current);
    }

    /// The help context of the currently highlighted menu item, as seen by the
    /// status line while the session is open.
    ///
    /// Walks the open levels from the **deepest (top of stack)** toward the bar.
    /// For the first level whose highlighted item is a named `Command` or `SubMenu`
    /// with `help_ctx != HelpCtx::NO_CONTEXT`, returns that `help_ctx`. Skips
    /// `Separator` items and levels with no current highlight. If no level
    /// qualifies, returns [`HelpCtx::NO_CONTEXT`].
    ///
    /// This mirrors `TMenuView::getHelpCtx` (`tmnuview.cpp:453-468`), which walks
    /// the `parentMenu` chain to find the first active level with a named, non-null
    /// `helpCtx`. Here the parentMenu chain is the level stack (deviation D9 /
    /// D3); the walk is equivalent.
    pub fn help_ctx(&self) -> HelpCtx {
        for level in self.levels.iter().rev() {
            let Some(idx) = level.current else {
                continue;
            };
            match level.menu.items.get(idx) {
                Some(MenuItem::Command { help_ctx, .. })
                | Some(MenuItem::SubMenu { help_ctx, .. })
                    if *help_ctx != HelpCtx::NO_CONTEXT =>
                {
                    return *help_ctx;
                }
                _ => {} // separator, or named item with NO_CONTEXT — keep walking
            }
        }
        HelpCtx::NO_CONTEXT
    }

    /// End the whole session: close every session-owned box, clear the bar's
    /// highlight, optionally post `cmd`, and pop the capture handler. Focus was
    /// never moved, so there is nothing to restore.
    fn end_session_with(&mut self, cmd: Option<Command>, ctx: &mut Context) -> CaptureFlow {
        // Tear down every level by kind: a **bar** (`is_bar`) is a permanent frame
        // child (C++ `TMenuBar`, reset + redrawn on close), so it is only
        // un-highlighted (execute()'s tail `current = 0; drawView()`), never removed;
        // every **box** (C++ `TMenuBox`/`TMenuPopup`, `execView`'d then destroyed) is
        // closed. A bar session has the bar at `levels[0]` + boxes above; a
        // [`popup_menu`] session has NO bar — its single level IS a box and must be
        // closed: the popup's level-0-is-a-box teardown needs this kind-keyed
        // loop, not a `skip(1)`/`first()` bar hard-coding.
        for level in &self.levels {
            if level.is_bar {
                ctx.request_set_menu_current(level.view_id, None);
            } else {
                ctx.request_close(level.view_id);
            }
        }
        // Post the selected command, if any (the pump's drop_disabled filter is
        // the backstop for a stale-enabled command).
        if let Some(cmd) = cmd {
            ctx.post(cmd);
        }
        // No focus restore: focus was never moved (Clean Architecture A — boxes
        // and the bar are never current), so the pre-menu current is intact.
        CaptureFlow::ConsumedPop
    }
}

impl CaptureHandler for MenuSession {
    /// One pass per offered event. Consumes every menu-directed event; keyboard,
    /// mouse, and the menu-toggle command share the flat re-apply loop, a non-menu
    /// command ends the session and is re-posted to the view tree, and everything
    /// else is swallowed to keep the session modal.
    fn handle(&mut self, ev: &mut Event, ctx: &mut Context) -> CaptureFlow {
        match *ev {
            // Keyboard + mouse + cmMenu share the flattened re-apply loop (run); the
            // per-kind switch arm runs inside it (step_keyboard / step_mouse /
            // step_cmd_menu). cmMenu is an `execute()` evCommand arm
            // (tmnuview.cpp:343-350): at a BOX it doReturns (closes + re-applies up,
            // unwinding to the bar); at the BAR it resets autoSelect/lastTargetItem
            // and stays open. It MUST go through the same post-switch/doReturn tail as
            // the other arms (not a top-only reset), else a box stays open on cmMenu.
            Event::KeyDown(_)
            | Event::MouseDown(_)
            | Event::MouseUp(_)
            | Event::MouseMove(_)
            | Event::Command(Command::MENU) => self.run(ev.clone(), ctx),
            // A non-cmMenu command → doReturn (close the whole menu). C++
            // execute()'s tail re-posts the command (`putEvent(e)` when
            // `e.what == evCommand`, tmnuview.cpp:403-405) so it still reaches the
            // view after the menu closes — port that with put_event.
            Event::Command(cmd) => {
                let r = self.end_session_with(None, ctx);
                ctx.put_event(Event::Command(cmd));
                r
            }
            // evBroadcast: SWALLOWED while active (clone-at-open is faithful —
            // execute() has no evBroadcast case; a cmCommandSetChanged is fetched
            // and ignored, so disabled stays frozen and boxes never regray
            // mid-menu). Consume so it does not reach the (idle) menu broker.
            Event::Broadcast { .. } => CaptureFlow::Consumed,
            // evMouseAuto: execute() has NO evMouseAuto arm (no auto-repeat /
            // press-hold in a menu). Consume to keep the session modal.
            Event::MouseAuto(_) => CaptureFlow::Consumed,
            // Anything else (Timer, Nothing): consume to keep the session modal.
            _ => CaptureFlow::Consumed,
        }
    }

    fn view(&self) -> Option<ViewId> {
        // The session is associated with the bar (level 0). Bounds gating uses the
        // per-level cache, not set_gate_bounds (boxes never move), so this is
        // informational only.
        self.levels.first().map(|l| l.view_id)
    }

    fn menu_help_ctx(&self) -> Option<HelpCtx> {
        // An open menu preempts the help context of any view behind it —
        // including when the highlighted item has NO_CONTEXT — matching C++
        // `TStatusLine::update` calling `TopView()->getHelpCtx()` where
        // `TopView()` is the active `TMenuView` while a menu is open.
        // Always returns `Some` so `CaptureStack::active_menu_help_ctx` stops
        // at this (topmost) handler and does not fall through to views below.
        Some(self.help_ctx())
    }
}

// ---------------------------------------------------------------------------
// Activation — assemble a MenuSession + its first deferred batch
// ---------------------------------------------------------------------------

/// Open a menu session from the **bar**.
///
/// Two activation kinds, distinguished by `open_index`:
///
/// * **Menu key (F10)** (`open_index == None`): the bar is highlighted on its
///   default title with `auto_select` off, and **no box opens** — the key only
///   highlights the default title and waits for the next key.
/// * **Hotkey** (`open_index == Some(idx)`): the bar is highlighted on the matched
///   title with `auto_select` on, and the matched title's box is opened in the
///   SAME deferred batch (no dead first event); `auto_select` persists so a later
///   Left/Right re-opens neighbouring titles.
///
/// A hotkey that names a top-level **command** (not a submenu) posts that command
/// and opens no session at all.
///
/// `bar_menu` is a clone of the bar's `menu`; `bar_bounds` its bounds in the root
/// frame; `owner_size` the root group size.
pub fn activate(
    bar_id: ViewId,
    bar_menu: Menu,
    bar_bounds: Rect,
    owner_size: Point,
    open_index: Option<usize>,
    ctx: &mut Context,
) {
    // The bar's initial highlight: the matched item (Alt-shortcut) or the menu
    // default (cmMenu / kbF10).
    let initial = open_index.or(bar_menu.default);
    // autoSelect is True only for the Alt-shortcut path (it doSelects); cmMenu
    // resets it to False (Blocker 1 / 3).
    let auto_select = open_index.is_some();

    // Alt-shortcut to a top-level COMMAND item (`tmnuview.cpp:388`: doSelect on a
    // command → result = command): post it and open NO session (the menu never
    // appears, faithful to execView returning the command immediately).
    if let Some(idx) = open_index
        && let Some(MenuItem::Command {
            command, disabled, ..
        }) = bar_menu.items.get(idx)
        && !*disabled
    {
        ctx.post(*command);
        return;
    }

    let bar_level = bar_level(bar_id, bar_menu.clone(), initial, bar_bounds, auto_select);
    let mut session = MenuSession::new(vec![bar_level], owner_size);

    // Push the bar's initial highlight for draw.
    ctx.request_set_menu_current(bar_id, initial);

    // Open the first box ONLY for the Alt-shortcut path (open_index is Some) — NOT
    // for cmMenu, which only highlights the default (Blocker 1). The matched item
    // must name a non-disabled submenu.
    if let Some(idx) = open_index
        && let Some(MenuItem::SubMenu { menu, disabled, .. }) = bar_menu.items.get(idx)
        && !*disabled
    {
        let submenu = menu.clone();
        session.open_submenu(idx, submenu, ctx);
    }

    ctx.push_capture(Box::new(session));
}

/// Build the bar [`MenuLevel`] with the transient mouse flags freshly initialized
/// (`last_target_item: None`, `mouse_active: false`, `first_event: true`). Shared
/// by [`activate`] (keyboard) and [`activate_mouse`].
fn bar_level(
    bar_id: ViewId,
    menu: Menu,
    current: Option<usize>,
    bounds: Rect,
    auto_select: bool,
) -> MenuLevel {
    MenuLevel {
        view_id: bar_id,
        menu,
        current,
        bounds,
        is_bar: true,
        auto_select,
        last_target_item: None,
        mouse_active: false,
        first_event: true,
    }
}

/// Open a menu session from the **bar** on a mouse press: push the session, then
/// re-post the click so the session's press arm processes it.
///
/// Unlike the hotkey [`activate`], this opens **no box up front**: the re-posted
/// click drives the session's press arm (track to the clicked title, set
/// `auto_select`) and the open gate, which yields the correct
/// `auto_select`/`last_target_item` for the second-click-closes behaviour.
///
/// `bar_menu` is a clone of the bar's `menu`; `bar_bounds` its bounds in the root
/// frame (the bar is at `(0,0)`, so the click delivered to `handle_event` is
/// already root-frame); `owner_size` the root group size; `mouse` the click to
/// re-post.
pub fn activate_mouse(
    bar_id: ViewId,
    bar_menu: Menu,
    bar_bounds: Rect,
    owner_size: Point,
    mouse: MouseEvent,
    ctx: &mut Context,
) {
    // execute()'s prologue sets current = menu->deflt; the re-posted click's
    // evMouseDown arm trackMouses to the clicked title and sets auto_select.
    let initial = bar_menu.default;
    let bar_level = bar_level(bar_id, bar_menu, initial, bar_bounds, false);
    let session = MenuSession::new(vec![bar_level], owner_size);

    // Initial highlight (the menu default) for draw.
    ctx.request_set_menu_current(bar_id, initial);
    // Push the session, then re-post the click so the session (now on the stack)
    // processes it through its evMouseDown arm on the next pump and opens the
    // clicked title's box.
    ctx.push_capture(Box::new(session));
    ctx.put_event(Event::MouseDown(mouse));
}

// ---------------------------------------------------------------------------
// TMenuPopup — a standalone popup menu (popupMenu, popupmnu.cpp)
// ---------------------------------------------------------------------------

/// Open a standalone context popup [`MenuBox`] near `where_`, run as a single-box
/// [`MenuSession`] on the capture stack.
///
/// This is a single box level (no bar above it) consuming events on the capture
/// stack — the same handler as a bar dropdown, minus the bar. Two popup-specific
/// behaviours:
///
/// * **No default highlight on open**: the level starts with nothing highlighted
///   AND its cloned [`Menu::default`] is cleared, so a mouse release on a margin
///   highlights the first item.
/// * **No exit-click re-post**: a click outside the popup closes it without
///   re-posting the click, so the session's
///   [`put_click_event_on_exit`](MenuSession) is set `false`.
///
/// `where_` is in root-group coords (the root group is at `(0,0)`). `owner_size` is
/// the root group size.
///
/// This function returns immediately; the chosen command arrives later via
/// [`Context::post`] (in [`end_session_with`](MenuSession::end_session_with)) when
/// the user selects an item, reaching the active routing. A popup needs no separate
/// accelerator handling: it is run modally and the session owns the event loop, so
/// the flat loop's hotkey handling already covers it.
///
/// # Turbo Vision heritage
/// Ports `popupMenu`/`TMenuPopup` (`popupmnu.cpp`/`tmenupop.cpp`/`menus.h`). The
/// blocking modal call becomes an async capture-stack session that posts the
/// chosen command (deviation D9); the `receiver` group argument is dropped because
/// there is no per-receiver routing seam.
pub fn popup_menu(where_: Point, menu: Menu, owner_size: Point, ctx: &mut Context) {
    let bounds = auto_place_popup(where_, &menu, owner_size);
    let id = ViewId::next();
    // The popup box clears its default (`menu->deflt = 0`, tmenupop.cpp:51) — no
    // highlight on open, and the margin-release arm picks the first item, not a
    // default.
    let mut menu = menu;
    menu.default = None;
    ctx.request_open_menu_box(id, menu.clone(), bounds);
    let level = MenuLevel {
        view_id: id,
        menu,
        current: None, // menu->deflt = 0
        bounds,
        is_bar: false,
        auto_select: false,
        last_target_item: None,
        mouse_active: false,
        first_event: true,
    };
    let mut session = MenuSession::new(vec![level], owner_size);
    // (A) TMenuPopup: putClickEventOnExit = False (tmenupop.cpp:45).
    session.put_click_event_on_exit = false;
    // No initial highlight (menu->deflt == 0).
    ctx.request_set_menu_current(id, None);
    ctx.push_capture(Box::new(session));
}

/// Place a freshly-built popup box near the click point `p`.
///
/// The box is first sized by [`menu_box_rect`] from a zero-size hint at `p`, which
/// lands it **above-left** of `p`. It is then moved to sit **below-right** of `p`
/// (top-left at `(p.x, p.y + 1)`), each axis clamped so the box stays inside the
/// desktop; if it would then cover `p` and there is room above, it is shifted up so
/// its bottom edge sits at `p.y`, keeping the click row visible.
///
/// # Turbo Vision heritage
/// Ports `autoPlacePopup` (`popupmnu.cpp`).
fn auto_place_popup(p: Point, menu: &Menu, owner_size: Point) -> Rect {
    // r = getRect(TRect(p, p), menu) → (p - size)..p (above-left of p).
    let mut r = menu_box_rect(Rect::new(p.x, p.y, p.x, p.y), menu);
    let size_x = r.b.x - r.a.x; // m->size.x
    let size_y = r.b.y - r.a.y; // m->size.y
    // d = app->size - p.
    let dx = owner_size.x - p.x;
    let dy = owner_size.y - p.y;
    // r.move(min(size.x, d.x), min(size.y + 1, d.y)) → top-left toward (p.x, p.y+1),
    // clamped so the box stays inside the desktop.
    r.r#move(size_x.min(dx), (size_y + 1).min(dy));
    // If the box still covers p and there is room above it (its height fits above
    // p), shift it up so its bottom edge is exactly at p.y.
    if r.contains(p) && (r.b.y - r.a.y) < p.y {
        r.r#move(0, -(r.b.y - p.y));
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::help::HelpCtx;

    // ---------------------------------------------------------------------------
    // MenuSession::help_ctx — mirrors TMenuView::getHelpCtx
    // ---------------------------------------------------------------------------

    /// Build a bare `MenuLevel` for test purposes (all transient flags at their
    /// level-entry defaults).
    fn make_level(menu: Menu, current: Option<usize>, is_bar: bool) -> MenuLevel {
        let id = ViewId::next();
        MenuLevel {
            view_id: id,
            menu,
            current,
            bounds: Rect::new(0, 0, 40, 1),
            is_bar,
            auto_select: false,
            last_target_item: None,
            mouse_active: false,
            first_event: false,
        }
    }

    /// `help_ctx` returns the highlighted item's context when it is non-NO_CONTEXT.
    ///
    /// Session has a bar level (no highlight) and a box level whose second item
    /// (a Command with a distinct HelpCtx) is highlighted. The walk must find the
    /// box item and return its context.
    #[test]
    fn help_ctx_returns_highlighted_item_context() {
        const SAVE_CTX: HelpCtx = HelpCtx::custom("test.save");

        // Bar level — nothing highlighted.
        let bar_menu = Menu::builder()
            .submenu("~F~ile", None, |m| {
                m.item(MenuItem::Command {
                    name: "~S~ave".to_string(),
                    command: Command::SAVE,
                    key_code: None,
                    param: None,
                    help_ctx: SAVE_CTX,
                    disabled: false,
                })
            })
            .build();
        let bar = make_level(bar_menu, None, true);

        // Box level — item 0 (Open) has NO_CONTEXT; item 1 (Save) has SAVE_CTX.
        let box_menu = Menu::builder()
            .item(MenuItem::Command {
                name: "~O~pen".to_string(),
                command: Command::OPEN,
                key_code: None,
                param: None,
                help_ctx: HelpCtx::NO_CONTEXT,
                disabled: false,
            })
            .item(MenuItem::Command {
                name: "~S~ave".to_string(),
                command: Command::SAVE,
                key_code: None,
                param: None,
                help_ctx: SAVE_CTX,
                disabled: false,
            })
            .build();
        // Highlight item 1 (Save).
        let box_level = make_level(box_menu, Some(1), false);

        let session = MenuSession::new(vec![bar, box_level], Point::new(80, 25));

        assert_eq!(
            session.help_ctx(),
            SAVE_CTX,
            "highlighted Save item's context must be returned"
        );
    }

    /// `help_ctx` falls back to `NO_CONTEXT` when no level has a qualifying item.
    ///
    /// Cases covered:
    /// - `current` is `None` (nothing highlighted)
    /// - `current` points at a `NO_CONTEXT` command
    /// - `current` points at a `Separator`
    #[test]
    fn help_ctx_fallback_cases_return_no_context() {
        // No current on any level.
        let session = MenuSession::new(vec![make_bar()], Point::new(80, 25));
        assert_eq!(
            session.help_ctx(),
            HelpCtx::NO_CONTEXT,
            "nothing highlighted → NO_CONTEXT"
        );

        // Current points at a command with NO_CONTEXT.
        let box_menu = Menu::builder()
            .command("~O~pen", Command::OPEN) // NO_CONTEXT by default
            .build();
        let box_level = make_level(box_menu, Some(0), false);
        let session2 = MenuSession::new(vec![make_bar(), box_level], Point::new(80, 25));
        assert_eq!(
            session2.help_ctx(),
            HelpCtx::NO_CONTEXT,
            "item with NO_CONTEXT → NO_CONTEXT"
        );

        // Current points at a Separator.
        let sep_menu = Menu {
            items: vec![MenuItem::Separator],
            default: None,
        };
        let sep_level = make_level(sep_menu, Some(0), false);
        let session3 = MenuSession::new(vec![make_bar(), sep_level], Point::new(80, 25));
        assert_eq!(
            session3.help_ctx(),
            HelpCtx::NO_CONTEXT,
            "separator highlight → NO_CONTEXT"
        );
    }

    /// `help_ctx` walks from the deepest (top) level toward the bar, stopping at
    /// the first qualifying item. Even when the bar has a highlighted title with a
    /// context, the box's item wins if it also has a context.
    #[test]
    fn help_ctx_deepest_level_wins() {
        const BAR_CTX: HelpCtx = HelpCtx::custom("test.bar_title");
        const BOX_CTX: HelpCtx = HelpCtx::custom("test.box_item");

        // Bar level — item 0 is a SubMenu with BAR_CTX, highlighted.
        let bar_menu = Menu {
            items: vec![MenuItem::SubMenu {
                name: "~F~ile".to_string(),
                key_code: None,
                help_ctx: BAR_CTX,
                disabled: false,
                menu: Menu::default(),
            }],
            default: Some(0),
        };
        let bar = make_level(bar_menu, Some(0), true);

        // Box level — a Command with BOX_CTX, highlighted.
        let box_menu = Menu {
            items: vec![MenuItem::Command {
                name: "~S~ave".to_string(),
                command: Command::SAVE,
                key_code: None,
                param: None,
                help_ctx: BOX_CTX,
                disabled: false,
            }],
            default: Some(0),
        };
        let box_level = make_level(box_menu, Some(0), false);

        let session = MenuSession::new(vec![bar, box_level], Point::new(80, 25));

        // The box (deeper / top of stack) must win.
        assert_eq!(
            session.help_ctx(),
            BOX_CTX,
            "deepest level's item context must win over the bar's"
        );
    }

    // ---------------------------------------------------------------------------
    // FIX D tests — disabled items, menu_help_ctx / active_menu_help_ctx semantics
    // ---------------------------------------------------------------------------

    /// Build the "File" bar level used by the fallback test cases (DRYs the three
    /// inline repetitions that each needed the same empty bar).
    fn make_bar() -> MenuLevel {
        make_level(
            Menu::builder()
                .submenu("~F~ile", None, |m| m.command("Exit", Command::QUIT))
                .build(),
            None,
            true,
        )
    }

    /// `help_ctx` does NOT skip a **disabled** item that has a non-NO_CONTEXT
    /// `help_ctx`. The C++ `getHelpCtx` walk (`tmnuview.cpp:453-468`) checks
    /// `helpCtx != hcNoContext` — it does not filter by disabled.
    #[test]
    fn help_ctx_disabled_item_with_context_is_returned() {
        const OPEN_CTX: HelpCtx = HelpCtx::custom("test.open_disabled");

        let box_menu = Menu {
            items: vec![MenuItem::Command {
                name: "~O~pen".to_string(),
                command: Command::OPEN,
                key_code: None,
                param: None,
                help_ctx: OPEN_CTX,
                disabled: true, // disabled, but context still surfaced
            }],
            default: None,
        };
        let box_level = make_level(box_menu, Some(0), false);
        let session = MenuSession::new(vec![make_bar(), box_level], Point::new(80, 25));

        assert_eq!(
            session.help_ctx(),
            OPEN_CTX,
            "disabled item with non-NO_CONTEXT help_ctx must still be returned"
        );
    }

    /// `menu_help_ctx` returns `Some(NO_CONTEXT)` — not `None` — when the
    /// highlighted item has `NO_CONTEXT`. This makes an open menu preempt any view
    /// behind it, matching C++ `TopView()->getHelpCtx()` semantics.
    #[test]
    fn menu_help_ctx_yields_some_no_context_when_item_is_no_context() {
        let box_menu = Menu::builder()
            .command("~O~pen", Command::OPEN) // NO_CONTEXT by default
            .build();
        let box_level = make_level(box_menu, Some(0), false);
        let session = MenuSession::new(vec![make_bar(), box_level], Point::new(80, 25));

        // help_ctx() must be NO_CONTEXT …
        assert_eq!(session.help_ctx(), HelpCtx::NO_CONTEXT);
        // … but menu_help_ctx() must return Some, not None, so it preempts.
        assert_eq!(
            session.menu_help_ctx(),
            Some(HelpCtx::NO_CONTEXT),
            "open menu yields Some(NO_CONTEXT) to preempt views below it"
        );
    }

    /// `CaptureStack::active_menu_help_ctx` returns the topmost handler's context
    /// (not a lower handler's), and returns `None` when the stack is empty.
    #[test]
    fn active_menu_help_ctx_uses_topmost_handler_only() {
        use crate::capture::CaptureStack;

        const ITEM_CTX: HelpCtx = HelpCtx::custom("test.item_ctx");

        // A session with a NO_CONTEXT highlighted item on the stack.
        let no_ctx_session = {
            let box_menu = Menu::builder()
                .command("~O~pen", Command::OPEN) // NO_CONTEXT
                .build();
            let box_level = make_level(box_menu, Some(0), false);
            MenuSession::new(vec![make_bar(), box_level], Point::new(80, 25))
        };

        let mut stack = CaptureStack::new();
        // Empty stack → None.
        assert_eq!(stack.active_menu_help_ctx(), None, "no handlers → None");

        stack.push(Box::new(no_ctx_session));
        // Topmost handler has NO_CONTEXT → Some(NO_CONTEXT), not None.
        assert_eq!(
            stack.active_menu_help_ctx(),
            Some(HelpCtx::NO_CONTEXT),
            "open menu with NO_CONTEXT item yields Some(NO_CONTEXT)"
        );

        // Now build a session whose top level has a real context.
        let ctx_session = {
            let box_menu = Menu {
                items: vec![MenuItem::Command {
                    name: "~S~ave".to_string(),
                    command: Command::SAVE,
                    key_code: None,
                    param: None,
                    help_ctx: ITEM_CTX,
                    disabled: false,
                }],
                default: None,
            };
            let box_level = make_level(box_menu, Some(0), false);
            MenuSession::new(vec![make_bar(), box_level], Point::new(80, 25))
        };

        // Replace the stack with just this session.
        let mut stack2 = CaptureStack::new();
        stack2.push(Box::new(ctx_session));
        assert_eq!(
            stack2.active_menu_help_ctx(),
            Some(ITEM_CTX),
            "topmost session with real context yields Some(ctx)"
        );
    }

    // ---------------------------------------------------------------------------
    // Legacy geometry tests follow
    // ---------------------------------------------------------------------------

    /// A flat two-command menu sized exactly like the program-test `popup_data`:
    /// every label fits the `menu_box_rect` minimum, so the box is 10 wide and
    /// `2 + 2 = 4` rows tall (`size_x == 10`, `size_y == 4`).
    fn popup_data() -> Menu {
        Menu::builder()
            .command("~C~ut", Command::CUT)
            .command("~C~opy", Command::custom("test.copy"))
            .build()
    }

    /// `auto_place_popup` with room everywhere puts the box below-right of `p`:
    /// top-left at `(p.x, p.y + 1)`, and the right edge stays inside the desktop.
    #[test]
    fn auto_place_popup_below_right_with_room() {
        let menu = popup_data();
        let p = Point::new(5, 2);
        let owner = Point::new(40, 12);
        let r = auto_place_popup(p, &menu, owner);

        // size_x = 10, size_y = 4 → box = Rect(5,3,15,7).
        assert_eq!(r.a, Point::new(p.x, p.y + 1), "top-left at (p.x, p.y+1)");
        assert_eq!(r, Rect::new(5, 3, 15, 7), "the full below-right box");
        assert!(r.b.x <= owner.x, "right edge clamped inside the desktop");
        assert!(
            !r.contains(p),
            "with room everywhere the box does NOT cover the click point",
        );
    }

    /// `auto_place_popup` near the desktop bottom: the vertical clamp would leave the
    /// box covering `p`, so (room above) it is shifted up until its bottom edge sits
    /// at `p.y` — the click row stays visible.
    ///
    /// BITE: drop the `if r.contains(p) ...` shift in [`auto_place_popup`] → the box
    /// stays at `Rect(5,8,15,12)`, covering the click row (`r.b.y == 12 != p.y`),
    /// failing the `r.b.y == p.y` assert.
    #[test]
    fn auto_place_popup_bottom_edge_shifts_up() {
        let menu = popup_data();
        let p = Point::new(5, 10);
        let owner = Point::new(40, 12);
        let r = auto_place_popup(p, &menu, owner);

        // d = (35, 2); move(min(10,35)=10, min(5,2)=2) → Rect(5,8,15,12) covers p
        // (y=10 ∈ [8,12)) and height 4 < p.y=10 → shift move(0, -(12-10)) → Rect(5,6,15,10).
        assert_eq!(
            r,
            Rect::new(5, 6, 15, 10),
            "shifted up to clear the click row"
        );
        assert_eq!(
            r.b.y, p.y,
            "the box's bottom edge sits exactly at the click row"
        );
        assert!(
            !r.contains(p),
            "after the shift the box no longer covers the click point",
        );
        assert!(
            r.b.x <= owner.x,
            "right edge still clamped inside the desktop"
        );
    }
}
