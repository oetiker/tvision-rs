//! The bottom [`StatusLine`] — a one-row [`View`] at the bottom of the screen
//! showing the items of the help-context-selected [`StatusDef`].
//!
//! It draws the items, hit-tests mouse clicks, runs the mouse press-and-hold
//! drag-highlight, handles the [`Command::COMMAND_SET_CHANGED`](crate::command::Command::COMMAND_SET_CHANGED)
//! broadcast (regraying its disabled items), and matches keyboard accelerators
//! against **all** items (including the hidden, text-less ones).
//!
//! ## Mouse press-and-hold drag-highlight
//!
//! Pressing the mouse over the status line highlights the item under the cursor
//! and follows it as the cursor moves, firing the item's command on release
//! only if the cursor is still over an enabled item:
//! * **`MouseDown`**: hit-test → set `pressed_item`, start tracking with
//!   [`TrackMask`]`{ mouse_move: true }`. Do NOT post yet.
//! * **`MouseMove`** (while tracking): re-derive the item under the cursor;
//!   update `pressed_item` (→ `None` when off all items). The next redraw shows
//!   the new highlight.
//! * **`MouseUp`** (while tracking): if `pressed_item` is `Some(idx)` and its
//!   command is enabled, post the command. Clear `pressed_item` / `tracking`.
//!
//! The `abs_origin` field caches the view-local `(0, 0)` in absolute screen
//! coords from the last `draw`, used by the capture to localize events.
//!
//! ## Help-context updates and keyboard accelerators
//!
//! [`Program`](crate::app::Program) pre-routes key and status-line mouse events
//! to the status line before normal dispatch, and on idle refreshes its help
//! context from the topmost view (via [`set_help_ctx`](StatusLine::set_help_ctx))
//! so the shown def follows the focused view. A key matching an item's
//! accelerator is transformed in place into a command event so it propagates to
//! the rest of the UI.
//!
//! ## Themes only — no palettes
//!
//! Colors resolve directly from the [`Theme`](crate::theme::Theme) via the
//! `Status*` [`Role`](crate::theme::Role)s ([`StatusColors`]), like
//! [`MenuColors`](crate::menu::MenuColors).
//!
//! # Turbo Vision heritage
//!
//! Ports `TStatusLine` (`tstatusl.cpp`). The palette-index color indirection
//! becomes direct theme-role lookups (deviation D7), and the streaming machinery
//! is dropped (deviation D12). Structurally it embeds a [`ViewState`] with
//! hand-written `View` methods rather than delegating, like
//! [`MenuBar`](crate::menu::MenuBar).

use crate::capture::TrackMask;
use crate::color::Style;
use crate::command::CommandSet;
use crate::event::Event;
use crate::help::HelpCtx;
use crate::status::StatusDef;
use crate::theme::Role;
use crate::view::{Context, DrawCtx, Point, Rect, View, ViewState};

/// Display width of a `~`-marked control string, **ignoring** the `~` markers
/// (they are hotkey delimiters, not printed columns). A per-module copy mirroring
/// [`menu_bar`](crate::menu::menu_bar)'s, using the same `UnicodeWidthChar`
/// primitive so widths match the rest of the renderer.
fn cstrlen(s: &str) -> i32 {
    s.chars()
        .filter(|&c| c != '~')
        .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(1) as i32)
        .sum()
}

/// The hint separator drawn before the hint text: a vertical bar `│` (U+2502)
/// followed by a space. Drawn as plain text, not a `~`-marked control string.
const HINT_SEPARATOR: &str = "\u{2502} ";

/// The four `(lo, hi)` style pairs a status item is drawn in — normal, selected,
/// normal-disabled, and selected-disabled — resolved once per draw from the
/// [`Theme`](crate::theme::Theme) via the `Status*` [`Role`]s. Each pair is
/// `(label style, shortcut-highlight style)`.
///
/// Analogous to [`MenuColors`](crate::menu::MenuColors) but reads the distinct
/// `Status*` roles — deliberately a separate type (different roles), not a reuse.
///
/// All four pairs are resolved as a unit. The `select` / `sel_disabled` pairs are
/// used by the hover/press highlight for the item under the cursor.
#[derive(Clone, Copy)]
pub struct StatusColors {
    /// Normal: `(StatusNormal, StatusShortcut)`.
    pub normal: (Style, Style),
    /// Selected/highlighted: `(StatusSelect, StatusShortcutSelect)`.
    pub select: (Style, Style),
    /// Normal but disabled: `StatusDisabled` for both lo and hi.
    pub norm_disabled: (Style, Style),
    /// Selected but disabled: `StatusSelDisabled` for both lo and hi.
    pub sel_disabled: (Style, Style),
}

impl StatusColors {
    /// Build a `StatusColors` by reading all four `Status*` role pairs from the
    /// draw context's theme in one pass.
    ///
    /// Call this once at the top of a `draw` implementation and pass the result to
    /// [`item`](Self::item) for each status item. This is how [`StatusLine::draw`]
    /// uses it — the C++ equivalent resolved palette indices for each item
    /// individually; this batches them all up front.
    pub fn resolve(ctx: &DrawCtx) -> Self {
        let d = ctx.style(Role::StatusDisabled);
        let sd = ctx.style(Role::StatusSelDisabled);
        StatusColors {
            normal: (
                ctx.style(Role::StatusNormal),
                ctx.style(Role::StatusShortcut),
            ),
            select: (
                ctx.style(Role::StatusSelect),
                ctx.style(Role::StatusShortcutSelect),
            ),
            // Disabled rows: a single style for both lo and hi (no shortcut
            // highlight when greyed).
            norm_disabled: (d, d),
            sel_disabled: (sd, sd),
        }
    }

    /// The `(lo, hi)` pair for an item given its `enabled`/`selected` state: a
    /// 2×2 matrix picking between the normal/select/disabled pairs.
    fn item(&self, enabled: bool, selected: bool) -> (Style, Style) {
        match (enabled, selected) {
            (true, true) => self.select,
            (true, false) => self.normal,
            (false, true) => self.sel_disabled,
            (false, false) => self.norm_disabled,
        }
    }
}

/// The bottom status line. Owns its `defs`, caches the selected def index and a
/// command-set snapshot for graying.
///
/// # Turbo Vision heritage
///
/// Ports `TStatusLine` (`tstatusl.cpp`).
pub struct StatusLine {
    /// The embedded view state (geometry, flags, state bits).
    state: ViewState,
    /// The status-line definitions, owned.
    defs: Vec<StatusDef>,
    /// Index into [`defs`](Self::defs) of the currently-selected def, or `None` if
    /// none match the current help context. Resolved by
    /// [`find_items`](Self::find_items).
    items_def: Option<usize>,
    /// The view's current help context.
    help_ctx: HelpCtx,
    /// The hint provider; the default returns `None` (no hint). Overridable via
    /// [`set_hint`](Self::set_hint).
    hint: Box<dyn Fn(HelpCtx) -> Option<String>>,
    /// Cached **disabled**-command snapshot for graying (a denylist; refreshed
    /// by the [`update_menu_commands`](View::update_menu_commands) broker hook,
    /// whose contract passes the program's disabled set). `None` before the
    /// first refresh means **treat all as enabled** (the same startup gap menus
    /// have — and consistent with the denylist default, where an empty set also
    /// means all-enabled). A status item carries no per-item disabled flag, so
    /// enablement is resolved against this snapshot at draw time.
    disabled_cmds: Option<CommandSet>,
    /// The item index currently being highlighted during a press-and-hold drag.
    /// `None` when the cursor is over no item. Reset to `None` on `MouseUp`.
    pressed_item: Option<usize>,
    /// Whether a mouse hold-track is in flight (between the arming `MouseDown`
    /// and the terminating `MouseUp`). Guards the `MouseMove`/`MouseUp` tracking
    /// arms against stray (untracked) events — `MouseUp` is not mask-gated in
    /// `Group::wants`.
    tracking: bool,
    /// Absolute screen position of view-local `(0, 0)`, cached each `draw` for
    /// the mouse-tracking capture.
    abs_origin: Point,
}

impl StatusLine {
    /// Construct a status line over `bounds` presenting `defs`.
    ///
    /// The grow mode sticks the line to the bottom and stretches it with the
    /// screen, and the view pre-processes accelerators before the focused view
    /// sees the event. Broadcasts reach it because the group fans them to every
    /// child unconditionally (see the identical note in
    /// [`menu_view::handle_event`](crate::menu::menu_view)); no event-mask opt-in
    /// is needed.
    pub fn new(bounds: Rect, defs: Vec<StatusDef>) -> Self {
        let mut state = ViewState::new(bounds);
        state.grow_mode.lo_y = true; // stick to the bottom edge
        state.grow_mode.hi_x = true; // stretch with screen width
        state.grow_mode.hi_y = true; // stretch with screen height
        state.options.pre_process = true; // see events before the focused view
        let mut sl = StatusLine {
            state,
            defs,
            items_def: None,
            help_ctx: HelpCtx::NO_CONTEXT, // no context by default
            hint: Box::new(|_| None),      // no hint by default
            disabled_cmds: None,
            pressed_item: None,
            tracking: false,
            abs_origin: Point::new(0, 0),
        };
        sl.find_items();
        sl
    }

    /// Override the hint provider. The closure maps the current help context to an
    /// optional hint string.
    pub fn set_hint(&mut self, hint: impl Fn(HelpCtx) -> Option<String> + 'static) {
        self.hint = Box::new(hint);
    }

    /// Builder-style [`set_hint`](Self::set_hint).
    pub fn with_hint(mut self, hint: impl Fn(HelpCtx) -> Option<String> + 'static) -> Self {
        self.set_hint(hint);
        self
    }

    /// Scan `defs` and cache the index of the first def whose range matches the
    /// current help context, or `None` if no def matches.
    ///
    /// Called automatically by [`new`](Self::new) and by
    /// [`set_help_ctx`](Self::set_help_ctx) whenever the context changes.
    /// You only need to call it directly if you modify [`defs`](StatusLine) in
    /// place after construction.
    pub fn find_items(&mut self) {
        self.items_def = self
            .defs
            .iter()
            .position(|d| d.range.matches(self.help_ctx));
    }

    /// Update the active help context and refresh the selected def.
    ///
    /// [`Program`](crate::app::Program) calls this on idle with the topmost
    /// view's help context so the displayed items follow the focused view.
    /// Idempotent: if `ctx` equals the current context the rescan is skipped.
    ///
    /// # Turbo Vision heritage
    ///
    /// Replaces `TStatusLine::update()`, which walked the view tree itself
    /// (`TopView()`) to obtain the help context and then called `findItems` +
    /// `drawView`. Here the `Program` idle loop resolves the context and passes
    /// it in; the draw is handled by the framework's whole-tree redraw cycle.
    pub fn set_help_ctx(&mut self, ctx: HelpCtx) {
        if self.help_ctx == ctx {
            return;
        }
        self.help_ctx = ctx;
        self.find_items();
    }

    /// The items of the currently-selected def, or an empty slice if none is
    /// selected.
    fn items(&self) -> &[crate::status::StatusItem] {
        match self.items_def {
            Some(i) => &self.defs[i].items,
            None => &[],
        }
    }

    /// Whether `command` is enabled, per the cached disabled-set snapshot
    /// (denylist: enabled iff NOT in the set). `None` (before the first broker
    /// refresh) means **treat all as enabled** (the startup gap).
    fn command_enabled(&self, command: crate::command::Command) -> bool {
        !self
            .disabled_cmds
            .as_ref()
            .is_some_and(|cs| cs.has(command))
    }

    /// The index of the item whose drawn span `[i, k)` contains the
    /// **view-local** `mouse`, or `None`.
    ///
    /// A `mouse.y != 0` returns `None`; otherwise walk the selected def's items
    /// accumulating the start `i` and end `k = i + width + 2` over the visible
    /// items (a text-less item is **skipped** in the accumulator — it consumes no
    /// width), and return the item whose `[i, k)` contains `mouse.x`.
    fn item_mouse_is_in(&self, mouse: Point) -> Option<usize> {
        if mouse.y != 0 {
            return None;
        }
        let mut i = 0i32;
        for (idx, item) in self.items().iter().enumerate() {
            if let Some(text) = &item.text {
                let k = i + cstrlen(text) + 2;
                if mouse.x >= i && mouse.x < k {
                    return Some(idx);
                }
                i = k;
            }
        }
        None
    }
}

impl View for StatusLine {
    fn state(&self) -> &ViewState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }

    /// Fill the row, then lay the selected def's visible items out left-to-right,
    /// each with a leading + trailing space in the per-item style, then the hint
    /// tail.
    ///
    /// The highlighted item is [`pressed_item`](StatusLine::pressed_item) — `None`
    /// when no press-and-hold is in flight, or `Some(idx)` when the item at `idx`
    /// is currently under the held button. The `(enabled, selected)` pair feeds
    /// [`StatusColors::item`], which selects the highlight styles for that item and
    /// the normal styles for all others.
    ///
    /// `abs_origin` is cached here for the mouse-tracking capture.
    fn draw(&mut self, ctx: &mut DrawCtx) {
        // Cache the absolute origin for the mouse-tracking capture: the
        // MouseTrackCapture converts abs mouse coords to view-local via this
        // value.
        self.abs_origin = ctx.origin();

        let colors = StatusColors::resolve(ctx);
        let size = self.state.size;

        // Fill the whole row with the normal label style.
        ctx.fill(Rect::new(0, 0, size.x, 1), ' ', colors.normal.0);

        let mut i = 0i32;
        for (idx, item) in self.items().iter().enumerate() {
            // The `i += l + 2` advance is INSIDE the `Some(text)` arm, so a
            // text-less item draws nothing AND consumes no width.
            if let Some(text) = &item.text {
                let l = cstrlen(text);
                if i + l < size.x {
                    let enabled = self.command_enabled(item.command);
                    // selected = true iff this item is the one being held.
                    let selected = self.pressed_item == Some(idx);
                    let (lo, hi) = colors.item(enabled, selected);
                    ctx.put_char(i, 0, ' ', lo);
                    ctx.put_cstr(i + 1, 0, text, lo, hi);
                    ctx.put_char(i + l + 1, 0, ' ', lo);
                }
                i += l + 2;
            }
        }

        // Hint tail: if there is room (at least 2 columns left) and the hint
        // provider returns a non-empty string, draw the separator then the
        // clipped hint.
        if i < size.x - 2
            && let Some(text) = (self.hint)(self.help_ctx)
            && !text.is_empty()
        {
            // Plain separator, in the normal label style.
            ctx.put_str(i, 0, HINT_SEPARATOR, colors.normal.0);
            i += 2;
            // The hint text, clipped to the row: put_str already truncates at the
            // clip right edge, so no explicit width arg is needed.
            ctx.put_str(i, 0, &text, colors.normal.0);
        }
    }

    /// Handle the status line's events.
    ///
    /// Branches:
    ///
    /// - **command-set-changed broadcast** → request the regray broker by the
    ///   view's own id ([`Context::request_update_menu`] — the same menu pattern;
    ///   reuses [`Deferred::UpdateMenu`](crate::view::Deferred::UpdateMenu) + the
    ///   [`update_menu_commands`](View::update_menu_commands) hook). The whole-tree
    ///   redraw makes any explicit repaint redundant.
    /// - **mouse down** — the first step of the press-and-hold drag: hit-test the
    ///   item, store as [`pressed_item`](StatusLine::pressed_item), set
    ///   `tracking = true`, arm [`TrackMask`]`{ mouse_move: true }`. Do NOT post
    ///   yet.
    /// - **mouse move** (guarded by `tracking`) — re-derive the item; update
    ///   `pressed_item`. The next whole-tree redraw renders the new highlight.
    /// - **mouse up** (guarded by `tracking`) — if `pressed_item` is Some and the
    ///   command is enabled, post it. Clear `pressed_item` / `tracking`.
    /// - **key down** — match the key against every item's accelerator (incl. the
    ///   hidden text-less ones) and, if its command is enabled, transform the event
    ///   into a command event in place so it propagates (see the module docs).
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        match ev {
            // Command-set-changed broadcast: the regray runs through the broker — a
            // child cannot read the command set inline, so request UpdateMenu by
            // our own id; the pump calls back through View::update_menu_commands
            // at apply time. No explicit repaint is needed (whole-tree redraw).
            //
            // The group fans broadcasts to EVERY child unconditionally, so no
            // event-mask opt-in is needed (same as menu_view).
            Event::Broadcast {
                command: crate::command::Command::COMMAND_SET_CHANGED,
                ..
            } => {
                if let Some(id) = self.state.id() {
                    ctx.request_update_menu(id);
                }
            }

            // Mouse down: the first step of the press-and-hold drag. Hit-test the
            // item under the view-local mouse position, record it as
            // `pressed_item`, and arm mouse-move tracking via the A3 seam. The
            // command is NOT posted here — post waits until MouseUp.
            Event::MouseDown(m) => {
                self.pressed_item = self.item_mouse_is_in(m.position);
                if let Some(id) = self.state.id() {
                    self.tracking = true;
                    ctx.start_mouse_track(
                        id,
                        self.abs_origin,
                        TrackMask {
                            mouse_move: true,
                            ..Default::default()
                        },
                    );
                } else {
                    // Degenerate fallback: no ViewId — ids are stamped at
                    // Group::insert, so this is test-only (an uninserted status
                    // line). Posts ON DOWN, diverging from the release-confirm
                    // semantics (the act-on-down fallback shared by
                    // button/cluster/frame in B2 wave 1).
                    if let Some(idx) = self.pressed_item {
                        let cmd = self.items()[idx].command;
                        if self.command_enabled(cmd) {
                            ctx.post(cmd);
                        }
                    }
                    self.pressed_item = None;
                }
                // The clear is load-bearing in BOTH branches: an uncleared
                // MouseDown would fall through to normal positional routing and
                // reach the status line a SECOND time via the root group,
                // double-arming the capture.
                ev.clear();
            }

            // Mouse move: re-derive the item under the view-local mouse (already
            // localized by the capture) and update `pressed_item`. The next
            // whole-tree redraw renders the highlight. Guarded by `tracking` —
            // MouseMove is not mask-gated in Group::wants for untracked events.
            Event::MouseMove(m) if self.tracking => {
                self.pressed_item = self.item_mouse_is_in(m.position);
                ev.clear();
            }

            // Mouse up: if the final item is present and its command is enabled,
            // post the command; then clear. Guarded by `tracking` — MouseUp is not
            // mask-gated in Group::wants.
            Event::MouseUp(_) if self.tracking => {
                self.tracking = false;
                if let Some(idx) = self.pressed_item.take() {
                    let cmd = self.items()[idx].command;
                    if self.command_enabled(cmd) {
                        ctx.post(cmd);
                    }
                }
                ev.clear();
            }

            // Key down: match the key against EVERY item (incl. text-less hidden
            // global hotkeys) and, if its command is enabled, TRANSFORM the event
            // into a command event IN PLACE — no clear, no post. The program's
            // pre-routing then lets the transformed command propagate to normal
            // dispatch (posting + clearing here would double-handle). These
            // transform-in-place-and-propagate semantics only make sense inside the
            // pre-routing stage, which is why this arm landed with the program
            // wiring step.
            Event::KeyDown(k) => {
                // Copy the key out so the `&mut ev` write below does not alias the
                // `k: &mut KeyEvent` borrow.
                let key = *k;
                for item in self.items() {
                    if item.key_code == Some(key) && self.command_enabled(item.command) {
                        *ev = Event::Command(item.command);
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    /// The command-graying broker hook (the same mechanism the menus use). The
    /// pump calls this at apply time with the live **disabled-command set** (a
    /// denylist) in hand; we snapshot it into
    /// [`disabled_cmds`](StatusLine::disabled_cmds) so `draw` can gray disabled
    /// items. (A status item has no per-item disabled flag, so enablement is
    /// resolved against this cached snapshot rather than stored on the item.)
    fn update_menu_commands(&mut self, disabled_cmds: &CommandSet) {
        self.disabled_cmds = Some(disabled_cmds.clone());
    }

    /// Expose the concrete line so the pump / tests can introspect it (the cached
    /// command-set snapshot the broker drives).
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
}

impl StatusLine {
    /// Read the cached **disabled**-command snapshot (test/inspection hook for
    /// the broker-driven graying cache; a denylist).
    pub fn disabled_cmds(&self) -> Option<&CommandSet> {
        self.disabled_cmds.as_ref()
    }

    /// Read the currently-selected def index (test hook for `find_items`).
    pub fn selected_def(&self) -> Option<usize> {
        self.items_def
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::command::{Command, CommandSet};
    use crate::event::{Event, Key, KeyEvent, MouseButtons, MouseEvent};
    use crate::menu::alt;
    use crate::screen::Buffer;
    use crate::status::StatusDef;
    use crate::theme::Theme;

    fn f1() -> KeyEvent {
        KeyEvent::from(Key::F(1))
    }

    /// A canonical default status line: Help, Exit, and a hidden Cut binding.
    fn sample_defs() -> Vec<StatusDef> {
        StatusDef::list()
            .def_all(|d| {
                d.item("~F1~ Help", f1(), Command::HELP)
                    .item("~Alt-X~ Exit", alt('x'), Command::QUIT)
                    .key_item(KeyEvent::from(Key::F(10)), Command::MENU)
            })
            .build()
    }

    fn render(line: &mut StatusLine, w: u16, h: u16) -> String {
        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(w, h);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = line.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            line.draw(&mut dc);
        });
        screen.snapshot()
    }

    // -- ctor ---------------------------------------------------------------

    #[test]
    fn ctor_sets_grow_and_preprocess() {
        let line = StatusLine::new(Rect::new(0, 24, 40, 25), sample_defs());
        assert!(line.state.grow_mode.lo_y, "gfGrowLoY set");
        assert!(line.state.grow_mode.hi_x, "gfGrowHiX set");
        assert!(line.state.grow_mode.hi_y, "gfGrowHiY set");
        assert!(line.state.options.pre_process, "ofPreProcess set");
        // find_items ran in the ctor: the All def is selected.
        assert_eq!(line.selected_def(), Some(0));
        assert_eq!(line.help_ctx, HelpCtx::NO_CONTEXT);
        assert!(line.disabled_cmds.is_none(), "no command set snapshot yet");
    }

    // -- find_items ---------------------------------------------------------

    #[test]
    fn find_items_all_matches_any_ctx() {
        let mut line = StatusLine::new(Rect::new(0, 0, 40, 1), sample_defs());
        assert_eq!(line.selected_def(), Some(0));
        line.set_help_ctx(HelpCtx::custom("whatever.context"));
        assert_eq!(line.selected_def(), Some(0), "All matches any context");
    }

    #[test]
    fn find_items_first_match_wins_with_one_of_then_all() {
        // [OneOf([a]), All]: ctx a -> def 0; ctx b -> def 1 (the All fallback).
        let a = HelpCtx::custom("app.editor");
        let b = HelpCtx::custom("app.browser");
        let defs = StatusDef::list()
            .def_one_of([a], |d| d.item("~F2~ Save", None, Command::SAVE))
            .def_all(|d| d.item("~F1~ Help", f1(), Command::HELP))
            .build();
        let mut line = StatusLine::new(Rect::new(0, 0, 40, 1), defs);

        line.set_help_ctx(a);
        assert_eq!(line.selected_def(), Some(0), "ctx a selects the OneOf def");
        line.set_help_ctx(b);
        assert_eq!(
            line.selected_def(),
            Some(1),
            "ctx b falls through to the All def"
        );
    }

    #[test]
    fn find_items_bite_all_first_captures_everything() {
        // BITE: reorder so All is FIRST -> everything selects def 0 (the OneOf def
        // is never reached). Proves first_match_wins is order-sensitive.
        let a = HelpCtx::custom("app.editor");
        let defs = StatusDef::list()
            .def_all(|d| d.item("~F1~ Help", f1(), Command::HELP))
            .def_one_of([a], |d| d.item("~F2~ Save", None, Command::SAVE))
            .build();
        let mut line = StatusLine::new(Rect::new(0, 0, 40, 1), defs);
        line.set_help_ctx(a);
        assert_eq!(
            line.selected_def(),
            Some(0),
            "All first captures even ctx a"
        );
    }

    #[test]
    fn find_items_no_match_leaves_none() {
        // A line with only a OneOf def and a non-member ctx -> no selection.
        let a = HelpCtx::custom("app.editor");
        let b = HelpCtx::custom("app.browser");
        let defs = StatusDef::list()
            .def_one_of([a], |d| d.item("~F2~ Save", None, Command::SAVE))
            .build();
        let mut line = StatusLine::new(Rect::new(0, 0, 40, 1), defs);
        line.set_help_ctx(b);
        assert_eq!(line.selected_def(), None, "no def matches -> items == 0");
        // items() is then empty.
        assert!(line.items().is_empty());
    }

    // -- item_mouse_is_in ---------------------------------------------------

    #[test]
    fn item_mouse_is_in_off_row_is_none() {
        let line = StatusLine::new(Rect::new(0, 0, 40, 1), sample_defs());
        // mouse.y != 0 -> None (off-row guard).
        assert_eq!(line.item_mouse_is_in(Point::new(2, 1)), None);
    }

    #[test]
    fn item_mouse_is_in_hits_correct_item_and_skips_textless() {
        // Layout (cstrlen ignores ~):
        //   "F1 Help"  cstrlen 7 -> [0, 9)   (idx 0)
        //   "Alt-X Exit" cstrlen 10 -> [9, 21) (idx 1)
        //   hidden Cut (text None) -> consumes NOTHING, span untouched (idx 2)
        let line = StatusLine::new(Rect::new(0, 0, 40, 1), sample_defs());

        assert_eq!(line.item_mouse_is_in(Point::new(0, 0)), Some(0));
        assert_eq!(
            line.item_mouse_is_in(Point::new(8, 0)),
            Some(0),
            "last col of item 0"
        );
        assert_eq!(
            line.item_mouse_is_in(Point::new(9, 0)),
            Some(1),
            "first col of item 1"
        );
        assert_eq!(
            line.item_mouse_is_in(Point::new(20, 0)),
            Some(1),
            "last col of item 1"
        );
        // BITE: the trailing space col of item 1 (col 20) maps to item 1, and a
        // click past it (col 21) is in no item's span -> None (the hidden Cut
        // binding does NOT occupy columns).
        assert_eq!(
            line.item_mouse_is_in(Point::new(21, 0)),
            None,
            "past last visible item"
        );
    }

    #[test]
    fn item_mouse_is_in_textless_neighbour_unaffected() {
        // A textless item BETWEEN two visible items must not shift the second's
        // span (its columns are unaffected — it consumes no width).
        let defs = StatusDef::list()
            .def_all(|d| {
                d.item("AB", None, Command::HELP) // cstrlen 2 -> [0, 4)
                    .key_item(f1(), Command::CUT) // hidden -> no width
                    .item("CD", None, Command::QUIT) // cstrlen 2 -> [4, 8)
            })
            .build();
        let line = StatusLine::new(Rect::new(0, 0, 40, 1), defs);
        assert_eq!(line.item_mouse_is_in(Point::new(0, 0)), Some(0));
        // Index 1 is the hidden item; the click at col 4 lands on the visible
        // "CD" item (index 2), NOT the hidden index 1.
        assert_eq!(line.item_mouse_is_in(Point::new(4, 0)), Some(2));
        assert_eq!(line.item_mouse_is_in(Point::new(7, 0)), Some(2));
    }

    // -- command graying (broker hook) --------------------------------------

    #[test]
    fn update_menu_commands_snapshots_set() {
        // The broker hook caches the live DISABLED set; command_enabled reads it.
        let mut line = StatusLine::new(Rect::new(0, 0, 40, 1), sample_defs());
        // Before any refresh, disabled_cmds is None -> everything is enabled.
        assert!(line.disabled_cmds().is_none());
        assert!(
            line.command_enabled(Command::QUIT),
            "None set -> all enabled"
        );

        // Snapshot a disabled set holding QUIT (HELP stays enabled).
        let mut disabled = CommandSet::new();
        disabled.insert(Command::QUIT);
        line.update_menu_commands(&disabled);

        assert!(line.disabled_cmds().is_some(), "broker hook cached the set");
        assert!(
            line.command_enabled(Command::HELP),
            "HELP not in the disabled set -> enabled"
        );
        assert!(
            !line.command_enabled(Command::QUIT),
            "QUIT in the disabled set -> grayed"
        );
    }

    #[test]
    fn command_enabled_bite_without_refresh_stays_all_enabled() {
        // BITE for the broker: without update_menu_commands the snapshot stays
        // None and every command reads enabled (the startup gap menus share).
        let line = StatusLine::new(Rect::new(0, 0, 40, 1), sample_defs());
        assert!(line.disabled_cmds().is_none());
        assert!(line.command_enabled(Command::QUIT));
        assert!(line.command_enabled(Command::HELP));
    }

    // -- handle_event: broadcast arm ----------------------------------------

    #[test]
    fn broadcast_command_set_changed_requests_update_menu() {
        use crate::view::{Context, Deferred};
        use std::collections::VecDeque;

        let mut line = StatusLine::new(Rect::new(0, 0, 40, 1), sample_defs());
        // The view needs an id for request_update_menu to fire; stamp one as the
        // group would.
        let id = crate::view::ViewId::next();
        line.state.id = Some(id);

        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = Vec::new();
        let mut ev = Event::Broadcast {
            command: Command::COMMAND_SET_CHANGED,
            source: None,
        };
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            line.handle_event(&mut ev, &mut ctx);
        }

        assert!(
            deferred
                .iter()
                .any(|d| matches!(d, Deferred::UpdateMenu(uid) if *uid == id)),
            "cmCommandSetChanged requests UpdateMenu by the view's own id"
        );
    }

    // -- handle_event: mouse arm --------------------------------------------
    //
    // The tracking arms (MouseMove, MouseUp) are driven directly with
    // view-local positions — exactly as the pump's Deferred::MouseTrack apply
    // arm does after the capture localizes them. The PushCapture path (the
    // Deferred::PushCapture that start_mouse_track queues) is tested via the
    // with_ctx_d helper and the integration test in program.rs wiring.

    use crate::timer::TimerQueue;
    use crate::view::{Context, Deferred};
    use std::collections::VecDeque;

    /// Run f with a fresh Context; return (out_events, deferred, return_value).
    fn with_ctx_d<R>(
        line: &mut StatusLine,
        ev: &mut Event,
        timers: &mut TimerQueue,
        f: impl FnOnce(&mut StatusLine, &mut Event, &mut Context) -> R,
    ) -> (Vec<Event>, Vec<Deferred>, R) {
        let mut out = VecDeque::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let r = {
            let mut ctx = Context::new(&mut out, timers, 0, &mut deferred);
            f(line, ev, &mut ctx)
        };
        (out.into_iter().collect(), deferred, r)
    }

    fn mouse_down(x: i32, y: i32) -> Event {
        Event::MouseDown(MouseEvent {
            position: Point::new(x, y),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        })
    }

    fn mouse_move(x: i32, y: i32) -> Event {
        Event::MouseMove(MouseEvent {
            position: Point::new(x, y),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        })
    }

    fn mouse_up(x: i32, y: i32) -> Event {
        Event::MouseUp(MouseEvent {
            position: Point::new(x, y),
            ..Default::default()
        })
    }

    /// Stamp the view with a fresh id (like Group::insert would).
    fn stamp_id(line: &mut StatusLine) -> crate::view::ViewId {
        let id = crate::view::ViewId::next();
        line.state.id = Some(id);
        id
    }

    // Existing single-shot tests remain as coverage for the no-id degenerate
    // fallback path (an uninserted status line, no ViewId -> immediate post).
    // This path exists to preserve backward compatibility for unit tests that
    // create a StatusLine without inserting it into a Group.

    #[test]
    fn mouse_down_on_enabled_item_posts_command_and_clears() {
        // No id -> degenerate single-shot fallback: post immediately on MouseDown.
        // Adapted from the old single-shot behavior; kept as coverage for the
        // no-id path.  The real tracking path is tested in the `_with_id` tests
        // below (post-on-release, sanctioned correction — same as cluster/button
        // in wave 1).
        let mut line = StatusLine::new(Rect::new(0, 0, 40, 1), sample_defs());
        let mut timers = TimerQueue::new();
        // Click inside "Alt-X Exit" (item 1, span [9, 21)) -> post QUIT immediately.
        let mut ev = mouse_down(10, 0);
        let (out, _deferred, ()) = with_ctx_d(&mut line, &mut ev, &mut timers, |l, e, ctx| {
            l.handle_event(e, ctx)
        });

        assert!(ev.is_nothing(), "mouse-down is always cleared");
        assert!(
            out.iter()
                .any(|e| matches!(e, Event::Command(Command::QUIT))),
            "no-id path: enabled item posts immediately on MouseDown"
        );
    }

    #[test]
    fn mouse_down_on_disabled_item_clears_but_posts_nothing() {
        let mut line = StatusLine::new(Rect::new(0, 0, 40, 1), sample_defs());
        // Disable QUIT via the broker snapshot (the DISABLED set).
        let mut disabled = CommandSet::new();
        disabled.insert(Command::QUIT);
        line.update_menu_commands(&disabled);

        let mut timers = TimerQueue::new();
        let mut ev = mouse_down(10, 0); // on the (disabled) Exit item
        let (out, _deferred, ()) = with_ctx_d(&mut line, &mut ev, &mut timers, |l, e, ctx| {
            l.handle_event(e, ctx)
        });

        assert!(ev.is_nothing(), "mouse-down is cleared even when disabled");
        assert!(
            !out.iter().any(|e| matches!(e, Event::Command(_))),
            "a disabled item posts nothing (C++ commandEnabled guard)"
        );
    }

    #[test]
    fn mouse_down_off_row_clears_but_posts_nothing() {
        let mut line = StatusLine::new(Rect::new(0, 0, 40, 1), sample_defs());
        let mut timers = TimerQueue::new();
        let mut ev = mouse_down(2, 1); // y != 0 -> no item hit
        let (out, _deferred, ()) = with_ctx_d(&mut line, &mut ev, &mut timers, |l, e, ctx| {
            l.handle_event(e, ctx)
        });

        assert!(ev.is_nothing());
        assert!(!out.iter().any(|e| matches!(e, Event::Command(_))));
    }

    // -- mouse hold-tracking arm tests (the A3 seam — the real path) ----------
    //
    // These tests use a stamped id (as Group::insert would supply) so the
    // MouseDown arm takes the real tracking path instead of the no-id fallback.
    // Positions are view-local (as the capture localizes them).

    /// Helper: a 40×1 status line with a stamped id, armed by a MouseDown on
    /// "Alt-X Exit" (local col 10, span [9, 21)).  Returns the line + the
    /// PushCapture deferred entry.
    fn tracked_status_line() -> (StatusLine, crate::view::ViewId) {
        let mut line = StatusLine::new(Rect::new(0, 0, 40, 1), sample_defs());
        let id = stamp_id(&mut line);
        let mut timers = TimerQueue::new();
        let mut ev = mouse_down(10, 0); // inside "Alt-X Exit"
        with_ctx_d(&mut line, &mut ev, &mut timers, |l, e, ctx| {
            l.handle_event(e, ctx)
        });
        assert!(line.tracking, "MouseDown on an item arms tracking");
        assert_eq!(
            line.pressed_item,
            Some(1),
            "pressed_item = Alt-X Exit (idx 1)"
        );
        (line, id)
    }

    /// MouseDown on an item WITH an id: arms tracking (PushCapture deferred),
    /// records pressed_item, does NOT post the command yet.
    #[test]
    fn mouse_down_with_id_arms_tracking_no_command() {
        let mut line = StatusLine::new(Rect::new(0, 0, 40, 1), sample_defs());
        let id = stamp_id(&mut line);
        let mut timers = TimerQueue::new();
        // Click inside "Alt-X Exit" (item 1, span [9, 21)).
        let mut ev = mouse_down(10, 0);
        let (out, deferred, ()) = with_ctx_d(&mut line, &mut ev, &mut timers, |l, e, ctx| {
            l.handle_event(e, ctx)
        });

        assert!(ev.is_nothing(), "MouseDown is always cleared");
        assert!(line.tracking, "tracking armed");
        assert_eq!(
            line.pressed_item,
            Some(1),
            "pressed_item set to the hit item index"
        );
        assert!(
            out.is_empty(),
            "command NOT posted at down time (post-on-release)"
        );
        // A PushCapture must have been deferred.
        assert_eq!(deferred.len(), 1, "one PushCapture deferred");
        assert!(
            matches!(deferred[0], Deferred::PushCapture(_)),
            "deferred[0] is PushCapture"
        );
        // The pushed capture's view() must return the status line's id.
        if let Deferred::PushCapture(ref h) = deferred[0] {
            assert_eq!(h.view(), Some(id), "capture tracks the status line's id");
        }
    }

    /// MouseDown on NO item: tracking is armed (capture deferred), pressed_item
    /// = None, no command posted.
    #[test]
    fn mouse_down_with_id_off_item_arms_tracking_no_pressed_item() {
        let mut line = StatusLine::new(Rect::new(0, 0, 40, 1), sample_defs());
        let _id = stamp_id(&mut line);
        let mut timers = TimerQueue::new();
        // Click past the last visible item (col 30) -> no hit.
        let mut ev = mouse_down(30, 0);
        let (out, deferred, ()) = with_ctx_d(&mut line, &mut ev, &mut timers, |l, e, ctx| {
            l.handle_event(e, ctx)
        });

        assert!(ev.is_nothing());
        assert!(
            line.tracking,
            "tracking armed even off items (mirrors C++ loop start)"
        );
        assert_eq!(line.pressed_item, None, "no item under the cursor");
        assert!(out.is_empty(), "no command");
        assert_eq!(deferred.len(), 1, "PushCapture still deferred");
    }

    /// MouseMove while tracking re-derives the item: move off item → pressed_item
    /// becomes None; move back on → Some again.
    #[test]
    fn track_move_updates_pressed_item() {
        let (mut line, _id) = tracked_status_line();
        let mut timers = TimerQueue::new();

        // Move off all items (col 30, past "Alt-X Exit" span [9, 21)).
        let mut ev = mouse_move(30, 0);
        let (out, deferred, ()) = with_ctx_d(&mut line, &mut ev, &mut timers, |l, e, ctx| {
            l.handle_event(e, ctx)
        });
        assert!(ev.is_nothing(), "tracked move is consumed");
        assert!(deferred.is_empty());
        assert!(out.is_empty());
        assert_eq!(
            line.pressed_item, None,
            "off items -> pressed_item = None (C++ drawSelect(0))"
        );

        // Move back onto "F1 Help" (item 0, span [0, 9)).
        let mut ev = mouse_move(4, 0);
        let (_, _, ()) = with_ctx_d(&mut line, &mut ev, &mut timers, |l, e, ctx| {
            l.handle_event(e, ctx)
        });
        assert_eq!(
            line.pressed_item,
            Some(0),
            "moved to item 0 -> pressed_item updated"
        );
        assert!(line.tracking, "still tracking");
    }

    /// Untracked MouseMove (tracking == false) falls through without updating
    /// pressed_item.
    #[test]
    fn untracked_move_falls_through() {
        let mut line = StatusLine::new(Rect::new(0, 0, 40, 1), sample_defs());
        let mut timers = TimerQueue::new();
        let mut ev = mouse_move(4, 0);
        let (out, _, ()) = with_ctx_d(&mut line, &mut ev, &mut timers, |l, e, ctx| {
            l.handle_event(e, ctx)
        });
        // Event is not consumed (falls through to the _ arm).
        assert!(!ev.is_nothing(), "untracked move is not consumed");
        assert!(out.is_empty());
    }

    /// MouseUp while tracking with a valid pressed_item fires the command.
    #[test]
    fn track_release_on_item_fires_command() {
        let (mut line, _id) = tracked_status_line();
        let mut timers = TimerQueue::new();

        let mut ev = mouse_up(10, 0); // still on "Alt-X Exit"
        let (out, deferred, ()) = with_ctx_d(&mut line, &mut ev, &mut timers, |l, e, ctx| {
            l.handle_event(e, ctx)
        });

        assert!(ev.is_nothing(), "tracked MouseUp is consumed");
        assert!(!line.tracking, "tracking cleared on MouseUp");
        assert_eq!(line.pressed_item, None, "pressed_item cleared on MouseUp");
        assert!(deferred.is_empty());
        assert!(
            out.iter()
                .any(|e| matches!(e, Event::Command(Command::QUIT))),
            "enabled item fires its command on release"
        );
    }

    /// MouseUp while tracking with pressed_item = None (cursor was off all items
    /// when released) does NOT fire any command.
    #[test]
    fn track_release_off_item_fires_nothing() {
        let (mut line, _id) = tracked_status_line();
        let mut timers = TimerQueue::new();

        // First move off items so pressed_item = None.
        let mut ev = mouse_move(30, 0);
        with_ctx_d(&mut line, &mut ev, &mut timers, |l, e, ctx| {
            l.handle_event(e, ctx)
        });
        assert_eq!(line.pressed_item, None);

        // Release while off items.
        let mut ev = mouse_up(30, 0);
        let (out, _, ()) = with_ctx_d(&mut line, &mut ev, &mut timers, |l, e, ctx| {
            l.handle_event(e, ctx)
        });

        assert!(ev.is_nothing(), "tracked up is consumed even with no item");
        assert!(!line.tracking);
        assert!(out.is_empty(), "no command when off items");
    }

    /// MouseUp while tracking with a DISABLED pressed_item does NOT fire the
    /// command (the release path checks command enablement first).
    #[test]
    fn track_release_on_disabled_item_fires_nothing() {
        let (mut line, _id) = tracked_status_line();
        // Disable QUIT (the pressed item's command).
        let mut disabled = CommandSet::new();
        disabled.insert(Command::QUIT);
        line.update_menu_commands(&disabled);

        let mut timers = TimerQueue::new();
        let mut ev = mouse_up(10, 0);
        let (out, _, ()) = with_ctx_d(&mut line, &mut ev, &mut timers, |l, e, ctx| {
            l.handle_event(e, ctx)
        });

        assert!(ev.is_nothing(), "tracked up is consumed");
        assert!(!line.tracking);
        assert!(
            out.is_empty(),
            "disabled item fires nothing on release (commandEnabled guard)"
        );
    }

    /// A stray MouseUp with no tracking in flight falls through untouched.
    #[test]
    fn stray_mouse_up_without_tracking_is_ignored() {
        let mut line = StatusLine::new(Rect::new(0, 0, 40, 1), sample_defs());
        let mut timers = TimerQueue::new();
        let mut ev = mouse_up(10, 0);
        let (out, _, ()) = with_ctx_d(&mut line, &mut ev, &mut timers, |l, e, ctx| {
            l.handle_event(e, ctx)
        });

        assert!(!ev.is_nothing(), "stray up is not consumed");
        assert!(out.is_empty());
        assert!(!line.tracking);
    }

    // -- draw snapshots -----------------------------------------------------

    #[test]
    fn snapshot_normal_with_disabled_item() {
        // Two visible items + a hidden Cut binding; QUIT disabled (grayed) via the
        // broker snapshot. Proves the color matrix and the `i += l + 2` layout
        // (the hidden item adds nothing).
        let mut line = StatusLine::new(Rect::new(0, 0, 40, 1), sample_defs());
        let mut disabled = CommandSet::new();
        disabled.insert(Command::QUIT);
        line.update_menu_commands(&disabled);
        insta::assert_snapshot!(render(&mut line, 40, 1));
    }

    #[test]
    fn snapshot_with_hint_tail() {
        // A hint closure returning text -> proves the hint tail (separator +
        // clipped hint), with `i < size.x - 2` true.
        let defs = StatusDef::list()
            .def_all(|d| d.item("~F1~ Help", f1(), Command::HELP))
            .build();
        let mut line = StatusLine::new(Rect::new(0, 0, 40, 1), defs)
            .with_hint(|_| Some("Press F1 for help".to_string()));
        insta::assert_snapshot!(render(&mut line, 40, 1));
    }

    #[test]
    fn empty_hint_string_draws_no_separator() {
        // A hint closure returning Some("") must render NOTHING extra — an empty
        // hint is skipped, so no separator is drawn.
        // BITE: deleting the `!text.is_empty()` guard in `draw` would draw the
        // `│ ` separator for the empty hint, making this render differ from the
        // None-hint render. We assert they are byte-identical.
        let defs = StatusDef::list()
            .def_all(|d| d.item("~F1~ Help", f1(), Command::HELP))
            .build();
        let mut empty_hint = StatusLine::new(Rect::new(0, 0, 40, 1), defs.clone())
            .with_hint(|_| Some(String::new()));
        let mut none_hint = StatusLine::new(Rect::new(0, 0, 40, 1), defs).with_hint(|_| None);
        assert_eq!(
            render(&mut empty_hint, 40, 1),
            render(&mut none_hint, 40, 1),
            "an empty hint string draws no separator (C++ if(hintText.size()))"
        );
    }

    #[test]
    fn snapshot_narrow_drops_overflowing_item() {
        // Width 8: "F1 Help" cstrlen 7, i=0 -> 0+7 < 8 true (drawn). i becomes 9;
        // "Alt-X Exit" -> 9+10 < 8 FALSE, not drawn (the clip-skip branch), and
        // there is no room for a hint (i=9, size.x-2=6). Exercises the clipped
        // path the wide line never hits.
        let mut line = StatusLine::new(Rect::new(0, 0, 8, 1), sample_defs());
        insta::assert_snapshot!(render(&mut line, 8, 1));
    }

    #[test]
    fn snapshot_textless_item_draws_nothing() {
        // A line whose only items are a hidden binding then a visible one: the
        // hidden item must add no width, so the visible item starts at column 0.
        let defs = StatusDef::list()
            .def_all(|d| {
                d.key_item(f1(), Command::CUT) // hidden
                    .item("Visible", None, Command::HELP)
            })
            .build();
        let mut line = StatusLine::new(Rect::new(0, 0, 40, 1), defs);
        insta::assert_snapshot!(render(&mut line, 40, 1));
    }

    /// Drag-highlight snapshot: with `pressed_item = Some(1)` (the "Alt-X Exit"
    /// item held), `draw` renders that item with the select/sel-disabled pair and
    /// all others with the normal/norm-disabled pair.
    ///
    /// Item 1 is drawn with `colors.select` (the "held" look), item 0 with
    /// `colors.normal`. The snapshot freezes the selected style for regression
    /// protection; hand-verify that item 1 appears visually distinct from item 0.
    #[test]
    fn snapshot_drag_highlight_held_item() {
        let mut line = StatusLine::new(Rect::new(0, 0, 40, 1), sample_defs());
        // Simulate a hold on "Alt-X Exit" (item 1) as if MouseDown was processed
        // and pressed_item was set. No actual event dispatch needed — draw reads
        // pressed_item directly.
        line.pressed_item = Some(1);
        insta::assert_snapshot!(render(&mut line, 40, 1));
    }

    /// Drag-highlight with a DISABLED held item: when the pressed item's command
    /// is disabled, `draw` uses the selected-disabled pair for that item, not the
    /// plain select pair.
    #[test]
    fn snapshot_drag_highlight_held_disabled_item() {
        let mut line = StatusLine::new(Rect::new(0, 0, 40, 1), sample_defs());
        // Disable QUIT (item 1's command) so colors.item(false, true) = sel_disabled.
        let mut disabled = CommandSet::new();
        disabled.insert(Command::QUIT);
        line.update_menu_commands(&disabled);
        line.pressed_item = Some(1);
        insta::assert_snapshot!(render(&mut line, 40, 1));
    }
}
