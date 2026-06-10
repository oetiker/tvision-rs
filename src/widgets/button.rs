//! `TButton` — faithful Rust port of `tbutton.cpp` (row 37).
//!
//! A `TButton` is a clickable command button: a 2+-row box with a centered (or
//! left-justified) title, a drop shadow, and an `~`-marked hotkey. Pressing it
//! (mouse, Alt+hotkey, focused-Space, or — for the default button — `cmDefault`)
//! fires its [`command`](Button::command), either posted as an `Event::Command`
//! or broadcast (`bfBroadcast`).
//!
//! # Model (D2 / D5)
//!
//! Embeds [`ViewState`] (D2) + branches on per-instance data; the `bf*` flag word
//! becomes the [`ButtonFlags`] struct-of-bools (D5). Draw goes through
//! [`DrawCtx`], events through [`Context`]. Coordinates are view-local (the group
//! already translated the mouse position into this view's frame).
//!
//! # The press animation (D9 — timer-id payload, first consumer)
//!
//! A keyboard / `cmDefault` press does not fire immediately: it flips the button
//! to its pressed look (`down = true`), arms a one-shot timer
//! ([`Context::set_timer`], `animationDurationMs = 100`), and stores the
//! [`TimerId`]. When that timer's [`Event::Timer`] arrives (broadcast-class, so it
//! reaches every view), the button compares the id to its stored
//! [`animation_timer`](Button::animation_timer); on a match it clears the flash
//! and finally [`press`](Button::press)es. This is the **typed timer-id payload**
//! (D4): the C++ `evBroadcast cmTimerExpired` with `infoPtr == animationTimer`
//! becomes [`Event::Timer(id)`], with the id matched against the stored handle —
//! so a button only fires on *its own* timer, never on an unrelated one.
//!
//! # D-rules applied
//!
//! * **D1** the `T` prefix is dropped; the view-local commands `cmGrabDefault` /
//!   `cmReleaseDefault` become namespaced [`Command::custom`] consts
//!   ([`Button::GRAB_DEFAULT`] / [`Button::RELEASE_DEFAULT`]).
//! * **D3** `makeLocal` is gone — the group delivers view-local mouse positions;
//!   `getExtent()` → `self.state.get_extent()`. `makeDefault` / `press` reach the
//!   owner only to *broadcast / post*, which go through [`Context`], not an
//!   up-pointer. The C++ `message(owner, evBroadcast, …, this)` carries `this` as
//!   the broadcast `source` (the resolvable [`ViewId`] successor to `infoPtr`).
//! * **D4** `enum Event` match; `Event::Command` carries no `source` (so `press`'s
//!   non-broadcast path is `ctx.post(command)`); `Event::Broadcast` carries
//!   `source = self id`. The mouse-down auto-select (the relocated
//!   `TView::handleEvent` base body) is owned by the group + the
//!   [`grabs_focus_on_click`](View::grabs_focus_on_click) hook, which we override
//!   to return `bfGrabFocus` (a button only auto-selects when `bfGrabFocus`).
//! * **D7** colors via `ctx.style(Role::Button*)`; the C++ `getColor` AttrPairs
//!   (`0x0501` / `0x0602` / `0x0703` / `0x0404`) become explicit (lo, hi) role
//!   pairs chosen per state (see [`Button::state_roles`]).
//! * **D8** draw into the back buffer through `DrawCtx`; `drawView`/`writeLine`/
//!   occlusion dropped (whole-tree redraw + diff). `drawState`'s flash is realized
//!   by the [`down`](Button::down) field read each redraw.
//! * **D12** `TStreamable` (`read`/`write`/`build`) dropped.
//!
//! # `eventMask |= evBroadcast` is a no-op here
//!
//! The C++ ctor opts the button into the broadcast class. Under our [`Group`] the
//! broadcast (and timer) phase delivers to **every** child regardless of
//! `event_mask` (which only gates `mouse_move`/`mouse_auto`), so no field is
//! needed — the opt-in is automatic.
//!
//! # Mouse hold-tracking (D9 capture)
//!
//! A mouse-down inside `clickRect` arms tracking: the button sets `down = true`
//! (pressed look) and pushes a [`ButtonTrackCapture`] onto the D9 capture stack.
//! The capture runs before normal view routing and holds the button's id, the
//! tracking rect (button-local), and the absolute origin of button-local `(0,0)`
//! cached in `abs_origin` by the last `draw`.
//!
//! While the mouse is held the capture converts every `MouseMove`'s absolute
//! position to button-local, compares it against the tracking rect, and posts
//! `Deferred::ButtonTrackDown` whenever containment flips — the pump applies
//! `b.down = inside` so the whole-tree redraw shows the updated look. On
//! `MouseUp` the capture posts `Deferred::ButtonTrackRelease { pressed: last }` —
//! where `last` is the LAST MOVE's tracked state, not the mouse-up position (the
//! C++ `do{}while(mouseEvent(…,evMouseMove))` body never re-evaluates the up
//! event's position) — and pops itself (`CaptureFlow::ConsumedPop`). The pump
//! sets `b.down = false` and, if `pressed`, calls `b.press(ctx)`. Every other
//! event during tracking is swallowed (`CaptureFlow::Consumed`), making the hold
//! modal (the C++ `mouseEvent` discards non-move events while tracking).
//!
//! For a button without an id (an uninserted, test-only button) the old
//! single-shot path is kept as a degenerate fallback.
//!
//! # Deferrals (documented TODOs, not built)
//!
//! 1. **Command-enabled graying.** The ctor's `if(!commandEnabled) state |=
//!    sfDisabled` and the `cmCommandSetChanged → setState(sfDisabled,…)` handler
//!    are not implemented. The blocking API gap is closed —
//!    [`Context::command_enabled`](crate::view::Context::command_enabled) exists
//!    (added with the A1 denylist flip) — but the graying itself is deferred as
//!    **backlog row B1** (`docs/BACKLOG.md`). The button starts enabled;
//!    functional correctness is preserved because the program filters disabled
//!    `Event::Command` at its boundary. One cosmetic gap: a notionally-disabled
//!    default button could still flash on `cmDefault`.
//! 2. **Plain-letter (postProcess) accelerator** — landed with the A5 phase
//!    signal (`ctx.phase()`, `tbutton.cpp:219`): see `handle_event`'s
//!    `post_plain` leg. A focused input line that consumes the letter still
//!    starves the post-loop, so plain letters are only stolen when the
//!    focused view leaves them live (faithful TV behavior).
//! 3. **`showMarkers` / `specialChars` / `markers`** dropped (always the
//!    no-markers branch).

use crate::capture::{CaptureFlow, CaptureHandler};
use crate::command::Command;
use crate::event::{Event, Key, hot_key, is_alt_hotkey, is_plain_hotkey};
use crate::theme::Role;
use crate::timer::TimerId;
use crate::view::{
    Context, DrawCtx, Options, Phase, Point, Rect, StateFlag, View, ViewId, ViewState,
};
use std::time::Duration;

/// `animationDurationMs` — the press-flash duration before the command fires.
const ANIMATION_DURATION_MS: u64 = 100;

// ---------------------------------------------------------------------------
// ButtonFlags — D5 struct-of-bools for the `bf*` word (dialogs.h)
// ---------------------------------------------------------------------------

/// Button flags — ports the `bf*` family (`dialogs.h`), D5.
///
/// `bfNormal == 0` is the all-false default. Build with [`ButtonFlags::new`] or
/// struct-update syntax (`ButtonFlags { default: true, ..Default::default() }`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ButtonFlags {
    /// `bfDefault` — this is the default button (fires on `cmDefault` / Enter).
    pub default: bool,
    /// `bfLeftJust` — the title is left-justified rather than centered.
    pub left_just: bool,
    /// `bfBroadcast` — the command is broadcast instead of posted.
    pub broadcast: bool,
    /// `bfGrabFocus` — a mouse-down selects (focuses) the button.
    pub grab_focus: bool,
}

impl ButtonFlags {
    /// All-false flags (`bfNormal`).
    pub fn new() -> Self {
        Self::default()
    }
}

// ---------------------------------------------------------------------------
// Button
// ---------------------------------------------------------------------------

/// `TButton` — a clickable command button.
pub struct Button {
    /// View state (geometry, flags, cursor) — the D2 composition target.
    pub state: ViewState,
    /// `title` — the button label, with `~` marking the hotkey letter.
    pub title: String,
    /// `command` — the command fired when the button is pressed.
    pub command: Command,
    /// `flags` — the `bf*` decoration/behavior flags (D5).
    pub flags: ButtonFlags,
    /// `amDefault` — whether the button currently acts as the default (toggled by
    /// `cmGrabDefault`/`cmReleaseDefault`; initialized from `bfDefault`).
    pub am_default: bool,
    /// `animationTimer` — the in-flight press-flash timer, if armed.
    pub animation_timer: Option<TimerId>,
    /// The pressed appearance (C++ `drawState`'s `down` argument), read each
    /// redraw (D8). `true` during the press flash.
    pub down: bool,
    /// Absolute screen position of button-local `(0, 0)`, cached each `draw`
    /// so the mouse-tracking capture (`ButtonTrackCapture`) can convert absolute
    /// mouse coordinates to button-local — the
    /// [`ColorPicker::body_origin`](crate::dialog::ColorPicker) pattern (D3/D9).
    /// Initialized to `(0, 0)`; updated on the first `draw` pass.
    abs_origin: Point,
}

impl Button {
    /// `cmGrabDefault` (61) — sent by a focused non-default button to ask the
    /// current default button to relinquish the default look. D1: a view-local
    /// command, namespaced.
    pub const GRAB_DEFAULT: Command = Command::custom("tv.button.grab_default");
    /// `cmReleaseDefault` (62) — the inverse of [`GRAB_DEFAULT`](Self::GRAB_DEFAULT):
    /// a non-default button losing focus asks the default button to take the
    /// default look back.
    pub const RELEASE_DEFAULT: Command = Command::custom("tv.button.release_default");

    /// `TButton::TButton` — build a button from `bounds`, `title`, `command`,
    /// `flags`.
    ///
    /// Faithful to the C++ ctor: `amDefault = (flags & bfDefault) != 0`,
    /// `animationTimer = 0` (→ `None`), `down = false`,
    /// `options |= ofSelectable | ofFirstClick | ofPreProcess | ofPostProcess`.
    ///
    /// The `eventMask |= evBroadcast` is a no-op under our group (broadcasts reach
    /// every child regardless of `event_mask`); see the module docs. The
    /// `if(!commandEnabled) state |= sfDisabled` is deferred as backlog row B1
    /// (`Context::command_enabled` now exists for it) — see deferral 1.
    pub fn new(bounds: Rect, title: &str, command: Command, flags: ButtonFlags) -> Self {
        let mut state = ViewState::new(bounds);
        state.options = Options {
            selectable: true,
            first_click: true,
            pre_process: true,
            post_process: true,
            ..Default::default()
        };
        // TODO(B1 command graying): ctx.command_enabled is available (A1 denylist
        // flip); port the C++ `if(!commandEnabled) state |= sfDisabled` here.
        Button {
            state,
            title: title.to_string(),
            command,
            flags,
            am_default: flags.default,
            animation_timer: None,
            down: false,
            abs_origin: Point::new(0, 0),
        }
    }

    /// The (lo, hi) [`Role`] pair `drawState` selects for the current state — the
    /// D7 form of the C++ `getColor` AttrPairs. `lo` is the body/label color, `hi`
    /// the hotkey-shortcut color (the `~`-toggled half).
    ///
    /// * disabled → `(ButtonDisabled, ButtonDisabled)` (`getColor(0x0404)`)
    /// * active + selected → `(ButtonSelected, ButtonSelectedShortcut)` (`0x0703`)
    /// * active + amDefault → `(ButtonDefault, ButtonDefaultShortcut)` (`0x0602`)
    /// * else → `(ButtonNormal, ButtonNormalShortcut)` (`0x0501`)
    fn state_roles(&self) -> (Role, Role) {
        let s = &self.state.state;
        if s.disabled {
            (Role::ButtonDisabled, Role::ButtonDisabled)
        } else if s.active && s.selected {
            (Role::ButtonSelected, Role::ButtonSelectedShortcut)
        } else if s.active && self.am_default {
            (Role::ButtonDefault, Role::ButtonDefaultShortcut)
        } else {
            (Role::ButtonNormal, Role::ButtonNormalShortcut)
        }
    }

    /// `TButton::press` — fire the button. Broadcasts `cmRecordHistory` first
    /// (history with no subject → `source = None`), then either broadcasts the
    /// command (`bfBroadcast`, `source = self id`) or posts it as an
    /// `Event::Command` (which carries no source under D4).
    ///
    /// `pub(crate)` so the pump's
    /// [`ButtonTrackRelease`](crate::view::Deferred::ButtonTrackRelease) broker
    /// can call it on the button after the mouse-hold tracking resolves (the
    /// `ButtonTrackCapture` D9 round-trip — analogous to `make_default`).
    pub(crate) fn press(&mut self, ctx: &mut Context) {
        let id = self.state.id();
        ctx.broadcast(Command::RECORD_HISTORY, None);
        if self.flags.broadcast {
            ctx.broadcast(self.command, id);
        } else {
            ctx.post(self.command);
        }
    }

    /// `TButton::makeDefault` — grab/release the default look. Only a **non**-default
    /// button does anything (the `if((flags & bfDefault) == 0)` guard): it
    /// broadcasts `cmGrabDefault`/`cmReleaseDefault` (so the real default button
    /// relinquishes/retakes the look) and toggles its own `am_default`. The C++
    /// `drawView()` is a D8 no-op.
    ///
    /// `pub(crate)` so the pump's
    /// [`MakeButtonDefault`](crate::view::Deferred::MakeButtonDefault) broker
    /// (row 80: `TDirListBox` focus → `chDirButton`) can drive it on a sibling.
    pub(crate) fn make_default(&mut self, enable: bool, ctx: &mut Context) {
        if !self.flags.default {
            let id = self.state.id();
            ctx.broadcast(
                if enable {
                    Self::GRAB_DEFAULT
                } else {
                    Self::RELEASE_DEFAULT
                },
                id,
            );
            self.am_default = enable;
        }
    }

    /// Arm the press flash: flip to the pressed look and start the one-shot
    /// animation timer if one is not already running. Shared by the keyboard and
    /// `cmDefault` paths (C++ `drawState(True); if(animationTimer==0)
    /// animationTimer = setTimer(animationDurationMs);`).
    fn start_animation(&mut self, ctx: &mut Context) {
        self.down = true;
        if self.animation_timer.is_none() {
            self.animation_timer =
                Some(ctx.set_timer(Duration::from_millis(ANIMATION_DURATION_MS), None));
        }
    }
}

impl View for Button {
    fn state(&self) -> &ViewState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }

    /// Exposes the concrete `Button` so the pump's
    /// [`MakeButtonDefault`](crate::view::Deferred::MakeButtonDefault) broker can
    /// downcast a sibling button and call [`make_default`](Button::make_default)
    /// (row 80: `TDirListBox` focus → `chDirButton`).
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    /// `TButton::draw` → `drawState(down)`. Builds each row explicitly so the
    /// shadow column / row land exactly as the C++ `TDrawBuffer` ops produced.
    ///
    /// Geometry (C++): `s = size.x - 1` (last column), `T = size.y/2 - 1` (the
    /// title row). Body rows are `y in 0..=size.y-2`; the bottom row (`size.y-1`)
    /// is the all-shadow stripe.
    ///
    /// Normal (`!down`): each body row is filled with spaces in `cButton`, col 0
    /// gets the shadow attr, the right column `s` gets the shadow attr + a shadow
    /// glyph (`▄` on the top row, `█` below). The title is drawn at `i+l` with
    /// `i = 1`. The bottom row is 2 shadow spaces then `▀` across in `cShadow`.
    ///
    /// Pressed (`down`): the body shifts right by one — cols 0..=1 take the shadow
    /// attr, the right-column shadow glyph vanishes (`ch = ' '`), the title is
    /// drawn with `i = 2`. The bottom row is all shadow spaces.
    fn draw(&mut self, ctx: &mut DrawCtx) {
        // Cache the absolute origin for the mouse-tracking capture (D3/D9 —
        // the ButtonTrackCapture converts abs mouse coords to button-local via
        // this value, mirroring the ColorPicker `body_origin` pattern).
        self.abs_origin = ctx.origin();
        let down = self.down;
        let (lo_role, hi_role) = self.state_roles();
        let c_button = ctx.style(lo_role);
        let c_button_hi = ctx.style(hi_role);
        let c_shadow = ctx.style(Role::ButtonShadow);
        let glyphs = *ctx.glyphs();

        let size = self.state.size;
        let s = size.x - 1; // last column
        let t = size.y / 2 - 1; // title row
        // The fill char for the bottom-row stripe (`▀` when not down, else space).
        let bottom_ch = if down {
            ' '
        } else {
            glyphs.button_shadow_bottom
        };

        // -- Body rows: 0 ..= size.y - 2 -------------------------------------
        for y in 0..=(size.y - 2) {
            // Fill the whole row with spaces in the button color.
            // NB: `Rect::new` takes corner coords `(ax, ay, bx, by)`, not w/h.
            ctx.fill(Rect::new(0, y, size.x, y + 1), ' ', c_button);
            // Column 0 always takes the shadow attribute.
            ctx.put_char(0, y, ' ', c_shadow);

            let i = if down {
                // Pressed: cols 0..=1 are shadow; body shifts right (i = 2); no
                // right-edge shadow glyph.
                ctx.put_char(1, y, ' ', c_shadow);
                2
            } else {
                // Normal: the right column takes the shadow attr + a shadow glyph
                // (`▄` on top row, `█` below).
                let glyph = if y == 0 {
                    glyphs.button_shadow_top
                } else {
                    glyphs.button_shadow_side
                };
                ctx.put_char(s, y, glyph, c_shadow);
                1
            };

            // Title on the vertically-centered row.
            if y == t && !self.title.is_empty() {
                // C++ drawTitle: l = bfLeftJust ? 1 : max(1, (s - cstrlen - 1)/2).
                let l = if self.flags.left_just {
                    1
                } else {
                    let centered = (s - cstrlen(&self.title) - 1) / 2;
                    centered.max(1)
                };
                // Centering uses cstrlen (strips `~`); the raw `~`-bearing title is
                // drawn through put_cstr's lo/hi toggle.
                ctx.put_cstr(i + l, y, &self.title, c_button, c_button_hi);
            }
        }

        // -- Bottom row: size.y - 1 ------------------------------------------
        // 2 leading shadow spaces, then `bottom_ch` (▀ when !down) across in the
        // shadow color (C++ `moveChar(0,' ',cShadow,2); moveChar(2,ch,cShadow,s-1)`).
        let last = size.y - 1;
        // C++ moveChar(0, ' ', cShadow, 2): 2 cells at col 0 → cols 0..2.
        ctx.fill(Rect::new(0, last, 2, last + 1), ' ', c_shadow);
        // C++ moveChar(2, ch, cShadow, s - 1): `s - 1` cells at col 2 → cols 2..s+1.
        ctx.fill(Rect::new(2, last, s + 1, last + 1), bottom_ch, c_shadow);
    }

    /// `TButton::handleEvent` — see the per-branch mapping in the module docs.
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        // C++ clickRect = getExtent() shrunk by (a.x+1, b.x-1, b.y-1).
        let ext = self.state.get_extent();
        let click_rect = Rect::from_points(
            Point::new(ext.a.x + 1, ext.a.y),
            Point::new(ext.b.x - 1, ext.b.y - 1),
        );

        // Note: the C++ `if(flags & bfGrabFocus) TView::handleEvent(event)`
        // mouse-down auto-select is relocated to the group via the
        // `grabs_focus_on_click` hook below — NOT done here.

        let c = hot_key(&self.title);

        match ev {
            Event::MouseDown(m) => {
                // Mouse-down outside clickRect is consumed without action (C++ first
                // clears it, then the disabled/contains gate makes the press a no-op);
                // either way the result is: press iff !disabled && contains, then clear.
                if !self.state.state.disabled && click_rect.contains(m.position) {
                    if let Some(id) = self.state.id() {
                        // Normal path (inserted button with a ViewId): arm tracking.
                        // Set the pressed look immediately, then push a ButtonTrackCapture
                        // so subsequent MouseMove/MouseUp events track containment and
                        // eventually fire press() via Deferred::ButtonTrackRelease.
                        //
                        // Tracking rect: clickRect widened by one on b.x (C++: `clickRect.b.x++`).
                        let track_rect = Rect::from_points(
                            click_rect.a,
                            Point::new(click_rect.b.x + 1, click_rect.b.y),
                        );
                        self.down = true;
                        ctx.push_capture(Box::new(ButtonTrackCapture {
                            button: id,
                            track_rect,
                            origin: self.abs_origin,
                            down: true,
                        }));
                    } else {
                        // Degenerate fallback: an uninserted (test-only) button has no id,
                        // so the capture broker cannot resolve it. Press immediately,
                        // mimicking the old pre-D9 behavior.
                        self.press(ctx);
                    }
                }
                ev.clear();
            }

            Event::KeyDown(ke) => {
                // Alt+hotkey, OR postProcess + plain hotkey letter, OR focused +
                // Space (`tbutton.cpp:216-228`). The Space branch is independent
                // of whether the title has a hotkey (a hotkey-less button still
                // acts on Space when focused). The plain-letter leg fires only
                // on the post-process walk (`owner->phase == phPostProcess` →
                // `ctx.phase()`, the A5 phase signal): an unfocused button picks
                // up letters the focused view left unconsumed.
                let alt_hot = c.map(|c| is_alt_hotkey(ke, c)).unwrap_or(false);
                let post_plain = ctx.phase() == Phase::PostProcess
                    && c.map(|c| is_plain_hotkey(ke, c)).unwrap_or(false);
                let focused_space = self.state.state.focused && ke.key == Key::Char(' ');
                if alt_hot || post_plain || focused_space {
                    self.start_animation(ctx);
                    ev.clear();
                }
            }

            Event::Broadcast { command, .. } => match *command {
                Command::DEFAULT if self.am_default && !self.state.state.disabled => {
                    self.start_animation(ctx);
                    ev.clear();
                }
                // Only the bfDefault button reacts to grab/release; NO clearEvent
                // (C++ — every default button must see it).
                cmd @ (Self::GRAB_DEFAULT | Self::RELEASE_DEFAULT) if self.flags.default => {
                    self.am_default = cmd == Self::RELEASE_DEFAULT;
                }
                // TODO(button: cmCommandSetChanged graying — see deferral 1)
                _ => {}
            },

            // The press-flash fired: fire iff it is *our* timer.
            Event::Timer(id) if Some(*id) == self.animation_timer => {
                self.animation_timer = None;
                self.down = false;
                self.press(ctx);
                ev.clear();
            }

            _ => {}
        }
    }

    /// `TButton::setState` — flip the propagating flag (replicating the trait-default
    /// body, since Rust has no `super` for a default method) then, for
    /// [`StateFlag::Focused`], run `makeDefault`. The C++ `if(aState &
    /// (sfSelected|sfActive)) drawView()` is a D8 no-op.
    ///
    /// Replicate-then-extend (the `Group::set_state` shape): flip the flag and, on
    /// `Focused`, emit the base focus broadcast (`cmReceivedFocus`/
    /// `cmReleasedFocus`, `source = self`) — *then* `make_default`. So focusing a
    /// non-default button queues `RECEIVED_FOCUS` **then** `GRAB_DEFAULT`.
    fn set_state(&mut self, flag: StateFlag, enable: bool, ctx: &mut Context) {
        // Base behaviour (replicated from the View::set_state default).
        self.state.set_flag(flag, enable);
        if flag == StateFlag::Focused {
            let source = self.state.id(); // self == C++ `this`
            ctx.broadcast(
                if enable {
                    Command::RECEIVED_FOCUS
                } else {
                    Command::RELEASED_FOCUS
                },
                source,
            );
            // TButton extension.
            self.make_default(enable, ctx);
        }
    }

    /// `TButton`'s opt-out for the relocated mouse-down auto-select: only a
    /// `bfGrabFocus` button is selected by a click (C++ calls
    /// `TView::handleEvent`'s base body only when `bfGrabFocus`).
    fn grabs_focus_on_click(&self) -> bool {
        self.flags.grab_focus
    }
}

// ---------------------------------------------------------------------------
// ButtonTrackCapture — D9 mouse press-and-hold capture (row 31)
// ---------------------------------------------------------------------------

/// D9 capture handler for button mouse press-and-hold tracking.
///
/// Pushed onto the capture stack on `MouseDown` inside the button's click rect.
/// Tracks mouse movement, posting [`Deferred::ButtonTrackDown`] whenever
/// containment in `track_rect` flips, and [`Deferred::ButtonTrackRelease`] on
/// `MouseUp` — both applied by the pump to the button identified by `button`.
///
/// Uses the button's `abs_origin` (cached each `draw`) to convert the capture's
/// absolute mouse coordinates to button-local (the `ColorPicker::body_origin`
/// pattern, D3/D9).
struct ButtonTrackCapture {
    /// The button this capture is tracking.
    button: ViewId,
    /// Button-local tracking rect (the C++ `clickRect` after `b.x++`).
    track_rect: Rect,
    /// Absolute screen position of button-local `(0, 0)` at push time —
    /// the button's `abs_origin` captured once so the drag math is stable
    /// (the `DragCapture` 3a coordinate-frame assumption).
    origin: Point,
    /// Last tracked containment state. Starts `true` (mouse-down was inside
    /// click_rect, which is a subset of track_rect). Used to send only on
    /// flips (no spam) and as the `pressed` payload on `MouseUp` — the C++
    /// decision uses the LAST MOVE's state, not the mouse-up position.
    down: bool,
}

impl CaptureHandler for ButtonTrackCapture {
    fn handle(&mut self, ev: &mut Event, ctx: &mut Context) -> CaptureFlow {
        match ev {
            Event::MouseMove(m) => {
                // Absolute → button-local (capture runs before group routing).
                let local = m.position - self.origin;
                let inside = self.track_rect.contains(local);
                if inside != self.down {
                    self.down = inside;
                    ctx.request_button_track_down(self.button, inside);
                }
                CaptureFlow::Consumed
            }
            Event::MouseUp(_) => {
                // The C++ `do{}while(mouseEvent(…,evMouseMove))` loop body
                // never re-evaluates the up-event's position — `mouseEvent`
                // returns `false` on mouse-up before the body runs again.
                // So the decision (press or not) uses the LAST MOVE's tracked
                // state (`self.down`), not the mouse-up position.
                ctx.request_button_track_release(self.button, self.down);
                CaptureFlow::ConsumedPop
            }
            // C++ `mouseEvent(event, evMouseMove)` discards everything that is
            // not a mouse-move/up while tracking — the hold is modal. Swallow
            // the rest (MouseAuto, keys, commands, broadcasts) without effect.
            _ => CaptureFlow::Consumed,
        }
    }

    fn view(&self) -> Option<ViewId> {
        Some(self.button)
    }
}

/// C++ `cstrlen` — display width of a `~`-marked control string, **ignoring**
/// the `~` markers (which are not printed columns).
///
/// Zero-alloc: iterates chars, skips `~`, and sums each char's display width
/// (matching C++'s `cstrlen` loop). Uses `UnicodeWidthChar` directly — the same
/// primitive that `crate::text` uses — so behavior is identical for all inputs,
/// including consecutive or trailing `~`.
fn cstrlen(s: &str) -> i32 {
    s.chars()
        .filter(|&c| c != '~')
        .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(1) as i32)
        .sum()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::event::{KeyEvent, KeyModifiers, MouseButtons, MouseEvent};
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use crate::timer::TimerQueue;
    use crate::view::Deferred;
    use std::collections::VecDeque;

    // -- helpers ------------------------------------------------------------

    /// Render the button to a snapshot string.
    fn render(button: &mut Button) -> String {
        let theme = Theme::classic_blue();
        let size = button.state.size;
        let (backend, screen) = HeadlessBackend::new(size.x as u16, size.y as u16);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = button.state.get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            button.draw(&mut dc);
        });
        screen.snapshot()
    }

    /// Run a closure with a fresh `Context` over loop-owned locals, returning the
    /// drained out-events plus the closure's value.
    fn with_ctx<R>(
        timers: &mut TimerQueue,
        now_ms: u64,
        f: impl FnOnce(&mut Context) -> R,
    ) -> (Vec<Event>, R) {
        let mut out = VecDeque::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let r = {
            let mut ctx = Context::new(&mut out, timers, now_ms, &mut deferred);
            f(&mut ctx)
        };
        (out.into_iter().collect(), r)
    }

    /// Like [`with_ctx`] but also returns the deferred vec so tests can assert
    /// on `Deferred::PushCapture` and the button-tracking variants.
    fn with_ctx_d<R>(
        timers: &mut TimerQueue,
        now_ms: u64,
        f: impl FnOnce(&mut Context) -> R,
    ) -> (Vec<Event>, Vec<Deferred>, R) {
        let mut out = VecDeque::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let r = {
            let mut ctx = Context::new(&mut out, timers, now_ms, &mut deferred);
            f(&mut ctx)
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

    fn key(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(k, KeyModifiers::default()))
    }

    fn alt_key(c: char) -> Event {
        Event::KeyDown(KeyEvent::new(
            Key::Char(c),
            KeyModifiers {
                alt: true,
                ..Default::default()
            },
        ))
    }

    /// A button stamped with an id (so broadcasts carry `source = Some(id)`), as
    /// `Group::insert` would do at runtime.
    fn button_with_id(b: &mut Button) -> crate::view::ViewId {
        let id = crate::view::ViewId::next();
        b.state.id = Some(id);
        id
    }

    // -- snapshot tests -----------------------------------------------------

    /// Normal button: centered "OK", drop shadow (`▄` top-right, `█` below, `▀`
    /// bottom stripe offset by 2).
    #[test]
    fn snapshot_normal() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags::new(),
        );
        insta::assert_snapshot!(render(&mut b));
    }

    /// Pressed look (`down = true`): body shifts right by one, shadow glyphs
    /// vanish, bottom row is all shadow spaces.
    #[test]
    fn snapshot_pressed() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags::new(),
        );
        b.down = true;
        insta::assert_snapshot!(render(&mut b));
    }

    /// Default button: `bfDefault` + active → ButtonDefault colors (the special
    /// branch is gated on sfActive, so `state.active` must be set).
    #[test]
    fn snapshot_default_active() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags {
                default: true,
                ..Default::default()
            },
        );
        b.state.state.active = true;
        assert!(b.am_default, "bfDefault initializes am_default");
        insta::assert_snapshot!(render(&mut b));
    }

    /// Selected button: active + selected → ButtonSelected colors.
    #[test]
    fn snapshot_selected_active() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags::new(),
        );
        b.state.state.active = true;
        b.state.state.selected = true;
        insta::assert_snapshot!(render(&mut b));
    }

    /// Disabled button: ButtonDisabled colors (overrides every other branch).
    #[test]
    fn snapshot_disabled() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags::new(),
        );
        b.state.state.disabled = true;
        insta::assert_snapshot!(render(&mut b));
    }

    /// Left-justified title (`bfLeftJust`): the title hugs the left (l = 1).
    #[test]
    fn snapshot_left_justified() {
        let mut b = Button::new(
            Rect::new(0, 0, 16, 2),
            "Save",
            Command::SAVE,
            ButtonFlags {
                left_just: true,
                ..Default::default()
            },
        );
        insta::assert_snapshot!(render(&mut b));
    }

    /// Centered title (default) in a wide button — contrast with the left-just one.
    #[test]
    fn snapshot_centered() {
        let mut b = Button::new(
            Rect::new(0, 0, 16, 2),
            "Save",
            Command::SAVE,
            ButtonFlags::new(),
        );
        insta::assert_snapshot!(render(&mut b));
    }

    /// `~`-hotkey title: the hotkey letter is drawn in the shortcut (hi) role.
    #[test]
    fn snapshot_hotkey_title() {
        let mut b = Button::new(
            Rect::new(0, 0, 12, 2),
            "~C~ancel",
            Command::CANCEL,
            ButtonFlags::new(),
        );
        insta::assert_snapshot!(render(&mut b));
    }

    // -- behavior: keyboard / timer animation -------------------------------

    /// Alt+hotkey arms the timer (no command yet); the matching Event::Timer then
    /// fires the command and resets down / animation_timer. An *unrelated* timer
    /// id must NOT fire it.
    #[test]
    fn alt_hotkey_arms_then_fires_on_own_timer() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "~O~k",
            Command::OK,
            ButtonFlags::new(),
        );
        let mut timers = TimerQueue::new();

        // Alt+O arms the timer; no command posted yet.
        let mut ev = alt_key('o');
        let (out, ()) = with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "Alt+hotkey consumes the key");
        assert!(b.down, "armed → pressed look");
        let armed = b.animation_timer.expect("timer armed");
        assert!(
            !out.iter().any(|e| matches!(e, Event::Command(_))),
            "no command posted at arm time"
        );

        // BITE: a foreign timer id must not fire the button. Arm a second, real
        // timer in the queue (a distinct id) and feed *its* expiry.
        let other = timers.set_timer(50, Duration::from_millis(100), None);
        assert_ne!(other, armed, "a fresh timer has a distinct id");
        let mut ev = Event::Timer(other);
        let (out, ()) = with_ctx(&mut timers, 50, |ctx| b.handle_event(&mut ev, ctx));
        assert!(!ev.is_nothing(), "foreign timer is not consumed");
        assert!(b.down, "still armed after a foreign timer");
        assert!(b.animation_timer.is_some());
        assert!(out.is_empty(), "foreign timer fires nothing");

        // Our own timer fires the command and resets.
        let mut ev = Event::Timer(armed);
        let (out, ()) = with_ctx(&mut timers, 100, |ctx| b.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "own timer is consumed");
        assert!(!b.down, "flash cleared");
        assert!(b.animation_timer.is_none(), "timer handle cleared");
        // press(): RECORD_HISTORY then the posted command.
        assert_eq!(
            out[0],
            Event::Broadcast {
                command: Command::RECORD_HISTORY,
                source: None
            }
        );
        assert_eq!(out[1], Event::Command(Command::OK));
    }

    /// The plain hotkey letter arms the press on the post-process walk —
    /// `owner->phase == phPostProcess && c == toupper(charCode)`
    /// (`tbutton.cpp:219-221`), via the A5 phase signal.
    #[test]
    fn plain_hotkey_arms_at_post_process() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "~O~k",
            Command::OK,
            ButtonFlags::new(),
        );
        let mut timers = TimerQueue::new();
        // Unfocused, plain 'o', delivered on the post-process leg.
        let mut ev = key(Key::Char('o'));
        with_ctx(&mut timers, 0, |ctx| {
            ctx.set_phase(Phase::PostProcess);
            b.handle_event(&mut ev, ctx)
        });
        assert!(ev.is_nothing(), "the postProcess plain letter is consumed");
        assert!(b.down, "pressed look armed");
        assert!(b.animation_timer.is_some(), "animation timer armed");
    }

    /// The same plain letter at the default (Focused) phase, unfocused, is
    /// ignored — the plain-letter leg is gated on phPostProcess.
    #[test]
    fn plain_hotkey_ignored_outside_post_process() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "~O~k",
            Command::OK,
            ButtonFlags::new(),
        );
        let mut timers = TimerQueue::new();
        let mut ev = key(Key::Char('o'));
        with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(
            !ev.is_nothing(),
            "a plain letter outside phPostProcess is left live"
        );
        assert!(!b.down, "not pressed");
        assert!(b.animation_timer.is_none(), "no timer armed");
    }

    /// A `bfBroadcast` button fires its command as a broadcast carrying its own id.
    #[test]
    fn broadcast_button_fires_broadcast_with_source() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "Go",
            Command::custom("app.go"),
            ButtonFlags {
                broadcast: true,
                ..Default::default()
            },
        );
        let id = button_with_id(&mut b);
        b.state.state.focused = true;
        let mut timers = TimerQueue::new();

        // Focused + Space arms.
        let mut ev = key(Key::Char(' '));
        with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        let armed = b.animation_timer.expect("armed by Space");

        // Own timer fires → broadcast (not post) with source = our id.
        let mut ev = Event::Timer(armed);
        let (out, ()) = with_ctx(&mut timers, 100, |ctx| b.handle_event(&mut ev, ctx));
        assert_eq!(
            out[0],
            Event::Broadcast {
                command: Command::RECORD_HISTORY,
                source: None
            }
        );
        assert_eq!(
            out[1],
            Event::Broadcast {
                command: Command::custom("app.go"),
                source: Some(id)
            }
        );
        assert!(
            !out.iter().any(|e| matches!(e, Event::Command(_))),
            "broadcast, not post"
        );
    }

    /// Focused + Space arms the timer (same animation path); a non-focused Space
    /// is ignored.
    #[test]
    fn focused_space_arms_unfocused_ignored() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags::new(),
        );
        let mut timers = TimerQueue::new();

        // Not focused → Space ignored.
        let mut ev = key(Key::Char(' '));
        with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(!ev.is_nothing(), "unfocused Space is not consumed");
        assert!(b.animation_timer.is_none());

        // Focused → Space arms.
        b.state.state.focused = true;
        let mut ev = key(Key::Char(' '));
        with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing());
        assert!(b.down);
        assert!(b.animation_timer.is_some());
    }

    // -- behavior: mouse ----------------------------------------------------

    /// Mouse-down inside clickRect on a button WITH an id: arms tracking.
    /// The event is consumed, `down` is set, a `PushCapture` is deferred, and
    /// NO command is posted immediately (the command fires after `MouseUp`).
    #[test]
    fn mouse_down_inside_arms_tracking() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags::new(),
        );
        let id = button_with_id(&mut b);
        let mut timers = TimerQueue::new();
        // clickRect = (1, 0, 9, 1): (3, 0) is inside.
        let mut ev = mouse_down(3, 0);
        let (out, deferred, ()) = with_ctx_d(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "mouse-down on the button is consumed");
        assert!(b.down, "button transitions to pressed look");
        assert!(
            b.animation_timer.is_none(),
            "no animation timer on mouse path"
        );
        assert!(out.is_empty(), "no command posted at mouse-down time");
        // A PushCapture must have been deferred.
        assert_eq!(deferred.len(), 1, "one PushCapture deferred");
        assert!(
            matches!(deferred[0], Deferred::PushCapture(_)),
            "deferred[0] is PushCapture"
        );
        // The pushed capture's view() must return the button's id.
        if let Deferred::PushCapture(ref h) = deferred[0] {
            assert_eq!(h.view(), Some(id), "capture tracks the button's id");
        }
    }

    /// Mouse-down inside clickRect on a button WITHOUT an id (test-only /
    /// uninserted): falls back to the old single-shot press (backwards compat).
    #[test]
    fn mouse_down_without_id_presses_immediately() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags::new(),
        );
        // No id stamped — the button has never been inserted in a group.
        let mut timers = TimerQueue::new();
        // clickRect = (1, 0, 9, 1): (3, 0) is inside.
        let mut ev = mouse_down(3, 0);
        let (out, deferred, ()) = with_ctx_d(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "mouse-down on the button is consumed");
        assert!(deferred.is_empty(), "no capture pushed for id-less button");
        assert_eq!(
            out[0],
            Event::Broadcast {
                command: Command::RECORD_HISTORY,
                source: None
            }
        );
        assert_eq!(out[1], Event::Command(Command::OK));
    }

    /// Mouse-down outside clickRect: consumed, but no press and no capture.
    #[test]
    fn mouse_down_outside_does_not_press() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags::new(),
        );
        let _id = button_with_id(&mut b);
        let mut timers = TimerQueue::new();
        // clickRect = (1, 0, 9, 1): (0, 0) is the shadow column, outside.
        let mut ev = mouse_down(0, 0);
        let (out, deferred, ()) = with_ctx_d(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "mouse-down is still consumed");
        assert!(out.is_empty(), "no press outside clickRect");
        assert!(deferred.is_empty(), "no capture for an outside click");
        assert!(!b.down, "down remains false");
    }

    /// Disabled button: a mouse-down inside is consumed but does not press and
    /// does not push a capture.
    #[test]
    fn disabled_mouse_down_does_not_press() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags::new(),
        );
        let _id = button_with_id(&mut b);
        b.state.state.disabled = true;
        let mut timers = TimerQueue::new();
        let mut ev = mouse_down(3, 0);
        let (out, deferred, ()) = with_ctx_d(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing());
        assert!(out.is_empty(), "disabled button does not press");
        assert!(deferred.is_empty(), "no capture for disabled button");
    }

    // -- ButtonTrackCapture unit tests ---------------------------------------

    /// Helper: construct a `ButtonTrackCapture` at origin (5, 3), tracking rect
    /// (1, 0, 10, 1) button-local (track_rect = clickRect widened by 1 on b.x
    /// for a 10×2 button). `down` starts `true`.
    ///
    /// Returns `(capture, button_id)`.
    fn make_capture() -> (ButtonTrackCapture, ViewId) {
        let id = ViewId::next();
        // 10×2 button: clickRect = (1,0,9,1); trackRect = (1,0,10,1).
        let cap = ButtonTrackCapture {
            button: id,
            track_rect: Rect::new(1, 0, 10, 1),
            origin: Point::new(5, 3), // abs origin of button-local (0,0)
            down: true,
        };
        (cap, id)
    }

    /// `MouseMove` outside the track rect → `ButtonTrackDown { down: false }`,
    /// `Consumed`; `MouseMove` back inside → `ButtonTrackDown { down: true }`.
    #[test]
    fn capture_move_outside_then_inside_emits_flips() {
        let (mut cap, id) = make_capture();
        let mut timers = TimerQueue::new();

        // Move to abs (5, 3) = button-local (0, 0): outside trackRect (x < 1).
        let mut ev = mouse_move(5, 3);
        let (_, deferred, flow) = with_ctx_d(&mut timers, 0, |ctx| cap.handle(&mut ev, ctx));
        assert!(matches!(flow, CaptureFlow::Consumed));
        assert!(!cap.down, "down flipped to false");
        assert_eq!(deferred.len(), 1);
        assert!(
            matches!(deferred[0], Deferred::ButtonTrackDown { button, down: false } if button == id)
        );

        // Move back inside: abs (8, 3) = button-local (3, 0): inside trackRect.
        let mut ev = mouse_move(8, 3);
        let (_, deferred, flow) = with_ctx_d(&mut timers, 0, |ctx| cap.handle(&mut ev, ctx));
        assert!(matches!(flow, CaptureFlow::Consumed));
        assert!(cap.down, "down flipped back to true");
        assert_eq!(deferred.len(), 1);
        assert!(
            matches!(deferred[0], Deferred::ButtonTrackDown { button, down: true } if button == id)
        );
    }

    /// Two consecutive moves inside → no flip, no deferred item (no spam).
    #[test]
    fn capture_no_spam_when_containment_unchanged() {
        let (mut cap, _id) = make_capture();
        let mut timers = TimerQueue::new();

        // First move inside: abs (8, 3) = button-local (3, 0) — inside, and
        // `down` already `true`, so NO flip and no deferred emit.
        let mut ev = mouse_move(8, 3);
        let (_, deferred, _) = with_ctx_d(&mut timers, 0, |ctx| cap.handle(&mut ev, ctx));
        assert!(deferred.is_empty(), "no emit when containment unchanged");

        // Second move inside: still no emit.
        let mut ev = mouse_move(9, 3);
        let (_, deferred, _) = with_ctx_d(&mut timers, 0, |ctx| cap.handle(&mut ev, ctx));
        assert!(deferred.is_empty(), "still no emit on second inside move");
    }

    /// `MouseUp` after last inside move → `ButtonTrackRelease { pressed: true }`,
    /// `ConsumedPop`.
    #[test]
    fn capture_mouse_up_after_inside_fires_press() {
        let (mut cap, id) = make_capture();
        let mut timers = TimerQueue::new();

        let mut ev = mouse_up(8, 3);
        let (_, deferred, flow) = with_ctx_d(&mut timers, 0, |ctx| cap.handle(&mut ev, ctx));
        assert!(
            matches!(flow, CaptureFlow::ConsumedPop),
            "MouseUp pops the capture"
        );
        assert_eq!(deferred.len(), 1);
        assert!(
            matches!(deferred[0], Deferred::ButtonTrackRelease { button, pressed: true } if button == id),
            "ButtonTrackRelease with pressed=true when last tracked state was inside"
        );
    }

    /// `MouseUp` after last outside move → `pressed: false` (no fire).
    #[test]
    fn capture_mouse_up_after_outside_no_press() {
        let (mut cap, id) = make_capture();
        let mut timers = TimerQueue::new();

        // Move outside first.
        let mut ev = mouse_move(5, 3); // button-local (0, 0), outside
        with_ctx_d(&mut timers, 0, |ctx| cap.handle(&mut ev, ctx));
        assert!(!cap.down);

        // Then MouseUp.
        let mut ev = mouse_up(5, 3);
        let (_, deferred, flow) = with_ctx_d(&mut timers, 0, |ctx| cap.handle(&mut ev, ctx));
        assert!(matches!(flow, CaptureFlow::ConsumedPop));
        assert_eq!(
            deferred.len(),
            1,
            "MouseUp must post exactly one ButtonTrackRelease"
        );
        assert!(
            matches!(deferred[0], Deferred::ButtonTrackRelease { button, pressed: false } if button == id),
            "ButtonTrackRelease with pressed=false when last tracked state was outside"
        );
    }

    /// A `bfBroadcast` button pressed via the mouse-track path fires its command
    /// as a broadcast carrying its own id — not as an `Event::Command`. The
    /// existing `broadcast_button_fires_broadcast_with_source` covers the
    /// keyboard/timer path; this covers the mouse path by applying the
    /// `Deferred::ButtonTrackRelease { pressed: true }` effect exactly as the
    /// pump arm does (set `down = false`, then `press(ctx)`).
    #[test]
    fn capture_release_fires_broadcast_for_bf_broadcast_button() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "Go",
            Command::custom("app.go"),
            ButtonFlags {
                broadcast: true,
                ..Default::default()
            },
        );
        let id = button_with_id(&mut b);
        b.down = true; // armed by the MouseDown that pushed the capture
        let mut timers = TimerQueue::new();

        // Apply ButtonTrackRelease { pressed: true } as the pump arm does.
        let (out, ()) = with_ctx(&mut timers, 0, |ctx| {
            b.down = false;
            b.press(ctx);
        });

        assert!(!b.down, "pressed look cleared on release");
        // press(): RECORD_HISTORY then the BROADCAST (source = our id), no post.
        assert_eq!(
            out[0],
            Event::Broadcast {
                command: Command::RECORD_HISTORY,
                source: None
            }
        );
        assert_eq!(
            out[1],
            Event::Broadcast {
                command: Command::custom("app.go"),
                source: Some(id)
            },
            "bfBroadcast fires a broadcast carrying the button's id"
        );
        assert!(
            !out.iter().any(|e| matches!(e, Event::Command(_))),
            "broadcast, not post"
        );
    }

    /// A `KeyDown` during tracking is swallowed (`Consumed`, nothing deferred).
    #[test]
    fn capture_key_during_tracking_is_swallowed() {
        let (mut cap, _id) = make_capture();
        let mut timers = TimerQueue::new();

        let mut ev = key(Key::Char('x'));
        let (out, deferred, flow) = with_ctx_d(&mut timers, 0, |ctx| cap.handle(&mut ev, ctx));
        assert!(
            matches!(flow, CaptureFlow::Consumed),
            "key is swallowed (hold is modal)"
        );
        assert!(out.is_empty());
        assert!(
            deferred.is_empty(),
            "no deferred effect for a key during tracking"
        );
    }

    // -- behavior: cmDefault ------------------------------------------------

    /// cmDefault arms the default button; a non-default button ignores it.
    #[test]
    fn cm_default_arms_default_button_only() {
        let mut default = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags {
                default: true,
                ..Default::default()
            },
        );
        let mut plain = Button::new(
            Rect::new(0, 0, 10, 2),
            "No",
            Command::NO,
            ButtonFlags::new(),
        );
        let mut timers = TimerQueue::new();

        let mut ev = Event::Broadcast {
            command: Command::DEFAULT,
            source: None,
        };
        with_ctx(&mut timers, 0, |ctx| default.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "default button consumes cmDefault");
        assert!(default.down);
        assert!(default.animation_timer.is_some());

        let mut ev = Event::Broadcast {
            command: Command::DEFAULT,
            source: None,
        };
        with_ctx(&mut timers, 0, |ctx| plain.handle_event(&mut ev, ctx));
        assert!(!ev.is_nothing(), "non-default button leaves cmDefault live");
        assert!(plain.animation_timer.is_none());
    }

    /// A disabled default button does not arm on cmDefault.
    #[test]
    fn cm_default_ignored_when_disabled() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags {
                default: true,
                ..Default::default()
            },
        );
        b.state.state.disabled = true;
        let mut timers = TimerQueue::new();
        let mut ev = Event::Broadcast {
            command: Command::DEFAULT,
            source: None,
        };
        with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(!ev.is_nothing());
        assert!(b.animation_timer.is_none());
    }

    // -- behavior: grab/release default -------------------------------------

    /// The bfDefault button toggles am_default on GRAB/RELEASE_DEFAULT; the event
    /// is NOT consumed (faithful — every default button must see it).
    #[test]
    fn bf_default_button_reacts_to_grab_release() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags {
                default: true,
                ..Default::default()
            },
        );
        assert!(b.am_default);
        let mut timers = TimerQueue::new();

        // GRAB_DEFAULT → another button is taking the default → am_default = false.
        let mut ev = Event::Broadcast {
            command: Button::GRAB_DEFAULT,
            source: None,
        };
        with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(!b.am_default);
        assert!(!ev.is_nothing(), "grab/release is not consumed");

        // RELEASE_DEFAULT → take the default back.
        let mut ev = Event::Broadcast {
            command: Button::RELEASE_DEFAULT,
            source: None,
        };
        with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(b.am_default);
    }

    /// A non-default button ignores GRAB/RELEASE_DEFAULT entirely.
    #[test]
    fn non_default_button_ignores_grab_release() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags::new(),
        );
        assert!(!b.am_default);
        let mut timers = TimerQueue::new();
        let mut ev = Event::Broadcast {
            command: Button::RELEASE_DEFAULT,
            source: None,
        };
        with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(!b.am_default, "non-default button stays non-default");
    }

    // -- behavior: set_state / makeDefault ----------------------------------

    /// Focusing a NON-default button: base focus broadcast (RECEIVED_FOCUS,
    /// source=self) THEN GRAB_DEFAULT (source=self), in that order; am_default
    /// becomes true; the focus bit is flipped.
    #[test]
    fn set_state_focused_non_default_grabs_default() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags::new(),
        );
        let id = button_with_id(&mut b);
        let mut timers = TimerQueue::new();

        let (out, ()) = with_ctx(&mut timers, 0, |ctx| {
            b.set_state(StateFlag::Focused, true, ctx)
        });
        assert!(b.state.state.focused, "focus bit flipped (base preserved)");
        assert!(
            b.am_default,
            "non-default button grabs the default on focus"
        );
        // Order: RECEIVED_FOCUS first, then GRAB_DEFAULT — both source = self.
        assert_eq!(
            out[0],
            Event::Broadcast {
                command: Command::RECEIVED_FOCUS,
                source: Some(id)
            }
        );
        assert_eq!(
            out[1],
            Event::Broadcast {
                command: Button::GRAB_DEFAULT,
                source: Some(id)
            }
        );
        assert_eq!(out.len(), 2);
    }

    /// Focusing a bfDefault button: the base focus broadcast fires, but
    /// makeDefault's guard means NO grab/release broadcast.
    #[test]
    fn set_state_focused_default_broadcasts_nothing_extra() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags {
                default: true,
                ..Default::default()
            },
        );
        let id = button_with_id(&mut b);
        let mut timers = TimerQueue::new();
        let (out, ()) = with_ctx(&mut timers, 0, |ctx| {
            b.set_state(StateFlag::Focused, true, ctx)
        });
        assert!(b.state.state.focused);
        assert_eq!(
            out,
            vec![Event::Broadcast {
                command: Command::RECEIVED_FOCUS,
                source: Some(id)
            }],
            "only the base focus broadcast; makeDefault guard blocks grab/release"
        );
    }

    /// A non-Focused set_state (e.g. Active) flips the bit with no broadcast.
    #[test]
    fn set_state_active_flips_bit_no_broadcast() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags::new(),
        );
        let mut timers = TimerQueue::new();
        let (out, ()) = with_ctx(&mut timers, 0, |ctx| {
            b.set_state(StateFlag::Active, true, ctx)
        });
        assert!(b.state.state.active);
        assert!(out.is_empty(), "Active flip emits nothing");
    }

    // -- grabs_focus_on_click ----------------------------------------------

    #[test]
    fn grabs_focus_on_click_reflects_flag() {
        let plain = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags::new(),
        );
        assert!(!plain.grabs_focus_on_click());
        let grab = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags {
                grab_focus: true,
                ..Default::default()
            },
        );
        assert!(grab.grabs_focus_on_click());
    }

    // -- boundary tests: mouse outside the click rect ----------------------

    /// Mouse-down on the bottom shadow row (y=1 for a 10x2 button, which is
    /// excluded by `click_rect.b.y = ext.b.y - 1 = 1`) must not press.
    #[test]
    fn mouse_down_bottom_shadow_row_does_not_press() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags::new(),
        );
        // click_rect = (1, 0, 9, 1): the half-open y range is 0..1, so y=1 is outside.
        let mut timers = TimerQueue::new();
        let mut ev = mouse_down(3, 1);
        let (out, ()) = with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "mouse-down is still consumed");
        assert!(out.is_empty(), "bottom shadow row must not press");
    }

    /// Mouse-down on the right shadow column (x=9 for a 10x2 button, excluded by
    /// `click_rect.b.x = ext.b.x - 1 = 9`) must not press.
    #[test]
    fn mouse_down_right_shadow_col_does_not_press() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags::new(),
        );
        // click_rect = (1, 0, 9, 1): the half-open x range is 1..9, so x=9 is outside.
        let mut timers = TimerQueue::new();
        let mut ev = mouse_down(9, 0);
        let (out, ()) = with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "mouse-down is still consumed");
        assert!(out.is_empty(), "right shadow column must not press");
    }

    // -- tiny-size smoke tests: draw must not panic ------------------------

    /// A 1-row button (size.y=1): the body loop `for y in 0..=size.y-2` is
    /// `0..=(-1)` in signed terms — with i32 that is an empty range, so it must
    /// produce no iterations and complete without panic.
    #[test]
    fn draw_one_row_does_not_panic() {
        let mut b = Button::new(Rect::new(0, 0, 3, 1), "X", Command::OK, ButtonFlags::new());
        // Wrapping size: size.y - 2 == -1, loop range 0..=-1 iterates zero times.
        // Only the bottom row is drawn; must not panic.
        let snap = render(&mut b);
        // Just assert we got something (no panic is the real check).
        assert!(!snap.is_empty(), "render completed without panic");
    }

    /// A 1-column button (size.x=1): `s = size.x - 1 = 0`, `t = size.y/2 - 1 = 0`.
    /// The `s - cstrlen - 1` centering formula can go negative; `centered.max(1)`
    /// keeps `l` ≥ 1, so `put_cstr` is called at x ≥ i+1 which may exceed the
    /// buffer width — must complete without panic.
    #[test]
    fn draw_one_col_does_not_panic() {
        let mut b = Button::new(Rect::new(0, 0, 1, 2), "X", Command::OK, ButtonFlags::new());
        let snap = render(&mut b);
        assert!(!snap.is_empty(), "render completed without panic");
    }
}
