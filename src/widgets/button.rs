//! A clickable command button: a 2+-row box with a centered (or
//! left-justified) title, a drop shadow, and an `~`-marked hotkey. Pressing it
//! (mouse, Alt+hotkey, focused-Space, or — for the default button — the
//! [`Command::DEFAULT`] broadcast) fires its [`command`](Button::command), either
//! posted as an `Event::Command` or broadcast.
//!
//! # Model
//!
//! [`Button`] embeds [`ViewState`] and branches on its per-instance data; the
//! decoration/behavior flags are the [`ButtonFlags`] struct-of-bools. Drawing
//! goes through [`DrawCtx`], events through [`Context`]. Coordinates are
//! view-local (the group already translated the mouse position into this view's
//! frame).
//!
//! # The press animation
//!
//! A keyboard / [`Command::DEFAULT`] press does not fire immediately: it flips the button
//! to its pressed look (`down = true`), arms a one-shot 100 ms timer
//! ([`Context::set_timer`]), and stores the [`TimerId`]. When that timer's
//! [`Event::Timer`] arrives (it reaches every view), the button compares the id
//! to its stored [`animation_timer`](Button::animation_timer); on a match it
//! clears the flash and finally [`press`](Button::press)es. Matching the id
//! against the stored handle means a button only fires on *its own* timer, never
//! on an unrelated one.
//!
//! Broadcast and timer events reach every child regardless of its event mask
//! (the mask only gates `mouse_move`/`mouse_auto`), so opting into the broadcast
//! class is automatic — no per-button state is needed.
//!
//! # Mouse hold-tracking
//!
//! A mouse-down inside the click rectangle begins a modal hold: the button sets
//! `down = true` and calls [`Context::start_mouse_track`], caching the absolute
//! screen origin of button-local `(0, 0)` (recorded by the last `draw`) so the
//! capture can convert absolute mouse coordinates back to button-local.
//!
//! From the next pump on, the
//! [`MouseTrackCapture`](crate::capture::MouseTrackCapture) routes every
//! `MouseMove` (localized to button-local) back into this `handle_event`: each
//! move flips `down` on tracking-rectangle containment. On `MouseUp` the button
//! presses iff the last move was still inside the tracking rectangle (`self.down`)
//! — never the release position — then clears `down`. Both arms are guarded by a
//! `tracking` flag so a stray, untracked release falls through. Every other event
//! during the hold is swallowed by the capture (the hold is modal). A button
//! without an id (an uninserted, test-only button) keeps a single-shot fallback.
//!
//! # Command-enabled graying
//!
//! When a [`Command::COMMAND_SET_CHANGED`] broadcast arrives, the button grays
//! itself out if its command is currently disabled
//! (`state.disabled = !ctx.command_enabled(command)`). Initial disabled state is
//! established the same way, by the first command-set-changed broadcast on the
//! opening idle pass — no separate constructor step is needed. A disabled default
//! button does not flash on the default-command broadcast.
//!
//! # Plain-letter accelerator
//!
//! When the button has its post-process option set, an unmodified letter that
//! matches its hotkey fires it during the post-processing phase. A focused input
//! line that consumes the letter first still starves this path, so plain letters
//! are only stolen when the focused view leaves them live (matching Turbo Vision).
//!
//! Marker decoration (`showMarkers`/`specialChars`) is not modeled — the button
//! always draws the no-markers form.
//!
//! # Turbo Vision heritage
//!
//! Ports `TButton` (`tbutton.cpp`/`dialogs.h`). Inheritance becomes the `View`
//! trait plus `ViewState` composition (deviation D2); the C++ button flag word
//! becomes [`ButtonFlags`] (deviation D5); the view-local grab/release-default
//! commands become namespaced [`Command::custom`] consts
//! ([`Button::GRAB_DEFAULT`] / [`Button::RELEASE_DEFAULT`]). The owner up-pointer
//! is replaced by [`Context`], and a broadcast carries the firing button's
//! resolvable [`ViewId`] as its `source` (deviations D3, D4). The palette
//! AttrPairs become explicit (lo, hi) [`Role`] pairs chosen per state (see
//! [`Button::state_roles`]); the typed [`Event::Timer(id)`] payload replaces the
//! C++ timer-expired broadcast.
//!
//! [`ViewId`]: crate::view::ViewId

use crate::capture::TrackMask;
use crate::command::Command;
use crate::event::{Event, Key, hot_key, is_alt_hotkey, is_plain_hotkey};
use crate::theme::Role;
use crate::timer::TimerId;
use crate::view::{Context, DrawCtx, Options, Phase, Point, Rect, StateFlag, View, ViewState};
use std::time::Duration;

/// The press-flash duration before the command fires.
const ANIMATION_DURATION_MS: u64 = 100;

// ---------------------------------------------------------------------------
// ButtonFlags — struct-of-bools for the button decoration/behavior flags
// ---------------------------------------------------------------------------

/// Button flags — the decoration/behavior flags as a struct-of-bools.
///
/// All-false is the default. Build with [`ButtonFlags::new`] or struct-update
/// syntax (`ButtonFlags { default: true, ..Default::default() }`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ButtonFlags {
    /// This is the default button (fires on the default command / Enter).
    pub default: bool,
    /// The title is left-justified rather than centered.
    pub left_just: bool,
    /// The command is broadcast instead of posted.
    pub broadcast: bool,
    /// A mouse-down selects (focuses) the button.
    pub grab_focus: bool,
}

impl ButtonFlags {
    /// All-false flags.
    pub fn new() -> Self {
        Self::default()
    }
}

// ---------------------------------------------------------------------------
// Button
// ---------------------------------------------------------------------------

/// A clickable command button.
pub struct Button {
    /// View state (geometry, flags, cursor) — the composition target.
    pub state: ViewState,
    /// The button label, with `~` marking the hotkey letter.
    pub title: String,
    /// The command fired when the button is pressed.
    pub command: Command,
    /// The decoration/behavior flags.
    pub flags: ButtonFlags,
    /// Whether the button currently acts as the default (toggled by the
    /// grab/release-default commands; initialized from the `default` flag).
    pub am_default: bool,
    /// The in-flight press-flash timer, if armed.
    pub animation_timer: Option<TimerId>,
    /// The pressed appearance, read each redraw. `true` during the press flash.
    pub down: bool,
    /// Absolute screen position of button-local `(0, 0)`, cached each `draw`
    /// so the mouse-tracking capture
    /// ([`MouseTrackCapture`](crate::capture::MouseTrackCapture)) can convert
    /// absolute mouse coordinates to button-local — the same approach as
    /// [`ColorPicker::body_origin`](crate::dialog::ColorPicker).
    /// Initialized to `(0, 0)`; updated on the first `draw` pass.
    abs_origin: Point,
    /// Whether a mouse hold-track is in flight (between the arming `MouseDown`
    /// and the terminating `MouseUp`). Guards the `MouseMove`/`MouseUp`
    /// tracking arms in `handle_event` against stray (untracked) events —
    /// `MouseUp` is not mask-gated in `Group::wants`.
    tracking: bool,
}

impl Button {
    /// Sent by a focused non-default button to ask the current default button to
    /// relinquish the default look. A view-local, namespaced command.
    pub const GRAB_DEFAULT: Command = Command::custom("tv.button.grab_default");
    /// The inverse of [`GRAB_DEFAULT`](Self::GRAB_DEFAULT): a non-default button
    /// losing focus asks the default button to take the default look back.
    pub const RELEASE_DEFAULT: Command = Command::custom("tv.button.release_default");

    /// Build a button from `bounds`, `title`, `command`, `flags`.
    ///
    /// `am_default` follows the `default` flag; the press-flash timer starts
    /// unarmed; the view is selectable and takes part in pre- and
    /// post-processing.
    ///
    /// Opting into the broadcast class is automatic (broadcasts reach every
    /// child regardless of its event mask), so no per-button state is needed.
    /// Initial command-enabled graying is established lazily by the first
    /// command-set-changed broadcast rather than in the constructor — see the
    /// module docs.
    pub fn new(bounds: Rect, title: &str, command: Command, flags: ButtonFlags) -> Self {
        let mut state = ViewState::new(bounds);
        state.options = Options {
            selectable: true,
            first_click: true,
            pre_process: true,
            post_process: true,
            ..Default::default()
        };
        // Initial command-enabled graying is established lazily: the first
        // COMMAND_SET_CHANGED broadcast fires on the opening idle pass when
        // initial_disabled_commands is non-empty. The constructor has no ctx,
        // so handling it in the broadcast arm is correct.
        Button {
            state,
            title: title.to_string(),
            command,
            flags,
            am_default: flags.default,
            animation_timer: None,
            down: false,
            abs_origin: Point::new(0, 0),
            tracking: false,
        }
    }

    /// The (lo, hi) [`Role`] pair selected for the current state. `lo` is the
    /// body/label color, `hi` the hotkey-shortcut color (the `~`-toggled half).
    ///
    /// * disabled → `(ButtonDisabled, ButtonDisabled)`
    /// * active + selected → `(ButtonSelected, ButtonSelectedShortcut)`
    /// * active + acting-as-default → `(ButtonDefault, ButtonDefaultShortcut)`
    /// * else → `(ButtonNormal, ButtonNormalShortcut)`
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

    /// Fire the button. Broadcasts [`Command::RECORD_HISTORY`] first (history with
    /// no subject → `source = None`), then either broadcasts the command (when the
    /// `broadcast` flag is set, with `source = self id`) or posts it as an
    /// `Event::Command` (which carries no source).
    ///
    /// Private: the mouse-up press fires from this view's own `MouseUp` arm,
    /// routed back by the [`MouseTrackCapture`](crate::capture::MouseTrackCapture),
    /// so no pump-side broker needs to reach in.
    fn press(&mut self, ctx: &mut Context) {
        let id = self.state.id();
        ctx.broadcast(Command::RECORD_HISTORY, None);
        if self.flags.broadcast {
            ctx.broadcast(self.command, id);
        } else {
            ctx.post(self.command);
        }
    }

    /// Grab/release the default look. Only a **non**-default button does anything:
    /// it broadcasts [`GRAB_DEFAULT`](Self::GRAB_DEFAULT) /
    /// [`RELEASE_DEFAULT`](Self::RELEASE_DEFAULT) (so the real default button
    /// relinquishes/retakes the look) and toggles its own `am_default`.
    ///
    /// `pub(crate)` so the pump's
    /// [`MakeButtonDefault`](crate::view::Deferred::MakeButtonDefault) broker can
    /// drive it on a sibling (a directory list box's focus moving to its directory
    /// button).
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
    /// default-command paths.
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
    /// downcast a sibling button and call [`make_default`](Button::make_default).
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    /// Paint the button, building each row explicitly so the shadow column and
    /// row land exactly.
    ///
    /// Geometry: `s = size.x - 1` (last column), `t = size.y / 2 - 1` (the title
    /// row). Body rows are `y in 0..=size.y-2`; the bottom row (`size.y-1`) is the
    /// all-shadow stripe.
    ///
    /// Normal (`!down`): each body row is filled with spaces in the button color,
    /// col 0 gets the shadow style, and the right column `s` gets the shadow style
    /// plus a shadow glyph (`▄` on the top row, `█` below). The title is drawn at
    /// `i+l` with `i = 1`. The bottom row is 2 shadow spaces then `▀` across in the
    /// shadow color.
    ///
    /// Pressed (`down`): the body shifts right by one — cols 0..=1 take the shadow
    /// style, the right-column shadow glyph vanishes (`ch = ' '`), the title is
    /// drawn with `i = 2`. The bottom row is all shadow spaces.
    fn draw(&mut self, ctx: &mut DrawCtx) {
        // Cache the absolute origin for the mouse-tracking capture: the
        // MouseTrackCapture converts absolute mouse coords to button-local via
        // this value, mirroring the ColorPicker `body_origin` pattern.
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

    /// Handle the button's events — mouse hold-tracking, keyboard/hotkey presses,
    /// default-look grab/release, and command-set graying. See the module docs for
    /// the per-branch behavior.
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
                        // Normal path (inserted button with a ViewId): this is
                        // the first iteration of the hold loop — the down
                        // position is inside clickRect ⊂ trackRect, so
                        // `down = true` (pressed look) — then enter the loop by
                        // starting a mouse track. The capture routes subsequent
                        // MouseMove/MouseUp back into the arms below,
                        // localized via `abs_origin` (cached by the last draw).
                        self.down = true;
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
                        // Degenerate fallback: an uninserted (test-only) button has no id,
                        // so the capture broker cannot resolve it. Press immediately.
                        self.press(ctx);
                    }
                }
                ev.clear();
            }

            // The hold-loop body, re-entered per tracked MouseMove via
            // Deferred::MouseTrack (position already button-local): flip `down`
            // on tracking-rect containment. Tracking rect: clickRect widened by
            // one on b.x (`clickRect.b.x++`), recomputed per event so it stays
            // resize-fresh.
            Event::MouseMove(m) if self.tracking => {
                let track_rect =
                    Rect::from_points(click_rect.a, Point::new(click_rect.b.x + 1, click_rect.b.y));
                let inside = track_rect.contains(m.position);
                if inside != self.down {
                    self.down = inside;
                }
                ev.clear();
            }

            // The post-loop code: press iff the LAST MOVE's tracked
            // containment (`self.down`) — the loop body never
            // re-evaluates the up-event's position (`mouseEvent` returns false
            // on mouse-up before the body runs again), so the up position is
            // deliberately not read. Guarded by `tracking` against stray ups
            // (MouseUp is not mask-gated in Group::wants).
            Event::MouseUp(_) if self.tracking => {
                self.tracking = false;
                let pressed = self.down;
                self.down = false;
                if pressed {
                    self.press(ctx);
                }
                ev.clear();
            }

            Event::KeyDown(ke) => {
                // Alt+hotkey, OR postProcess + plain hotkey letter, OR focused +
                // Space. The Space branch is independent of whether the title
                // has a hotkey (a hotkey-less button still acts on Space when
                // focused). The plain-letter leg fires only on the post-process
                // walk (`ctx.phase() == Phase::PostProcess`): an unfocused
                // button picks up letters the focused view left unconsumed.
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
                // Command graying: disable the button iff its command is disabled.
                // The COMMAND_SET_CHANGED broadcast fires during the idle phase
                // whenever the command set changes (including the initial pass via
                // Program::initial_disabled_commands), so this arm also covers the
                // initial disabled state — no separate constructor init needed.
                // Whole-tree redraw replaces the C++ drawView().
                Command::COMMAND_SET_CHANGED => {
                    self.state.state.disabled = !ctx.command_enabled(self.command);
                }
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

    /// Flip the propagating flag (replicating the trait-default body, since Rust
    /// has no `super` for a default method) then, for [`StateFlag::Focused`], run
    /// [`make_default`](Button::make_default). No explicit redraw is needed on
    /// select/activate (the whole tree is redrawn each pass).
    ///
    /// Replicate-then-extend (the `Group::set_state` shape): flip the flag and, on
    /// `Focused`, emit the base focus broadcast ([`Command::RECEIVED_FOCUS`] /
    /// [`Command::RELEASED_FOCUS`], `source = self`) — *then* `make_default`. So
    /// focusing a non-default button queues `RECEIVED_FOCUS` **then** `GRAB_DEFAULT`.
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

    /// Opt-in for the mouse-down auto-select: only a button with the `grab_focus`
    /// flag is selected (focused) by a click; the group consults this hook to
    /// decide whether a click should focus the button.
    fn grabs_focus_on_click(&self) -> bool {
        self.flags.grab_focus
    }
}

/// Display width of a `~`-marked control string, **ignoring** the `~` markers
/// (which are not printed columns).
///
/// Zero-alloc: iterates chars, skips `~`, and sums each char's display width.
/// Uses `UnicodeWidthChar` directly — the same primitive that `crate::text` uses
/// — so behavior is identical for all inputs, including consecutive or trailing
/// `~`.
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
    /// when the phase is `Phase::PostProcess` and the letter matches the hotkey.
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

    // -- mouse hold-tracking arm tests (the loop body / post-loop) -------------
    //
    // The MouseTrackCapture routes localized MouseMove/MouseUp events back into
    // the button's own handle_event (Deferred::MouseTrack); these tests drive
    // those arms directly with button-local positions, exactly as the pump's
    // apply arm does. The router itself is unit-tested in `capture::tests`.

    /// Helper: a 10×2 button with an id, armed by a MouseDown inside clickRect
    /// (`tracking == true`, `down == true`, one `PushCapture` deferred).
    fn tracked_button() -> Button {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags::new(),
        );
        button_with_id(&mut b);
        let mut timers = TimerQueue::new();
        // clickRect = (1,0,9,1): (3,0) is inside; trackRect = (1,0,10,1).
        let mut ev = mouse_down(3, 0);
        with_ctx_d(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(b.tracking, "MouseDown inside clickRect arms tracking");
        assert!(b.down);
        b
    }

    /// Tracked `MouseMove` outside the track rect flips `down` to false;
    /// a move back inside flips it back (the C++ loop body).
    #[test]
    fn track_move_outside_then_inside_flips_down() {
        let mut b = tracked_button();
        let mut timers = TimerQueue::new();

        // Button-local (0, 0): outside trackRect (x < 1).
        let mut ev = mouse_move(0, 0);
        with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "tracked move is consumed");
        assert!(!b.down, "down flipped to false");

        // Back inside: (3, 0).
        let mut ev = mouse_move(3, 0);
        with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(b.down, "down flipped back to true");
        assert!(b.tracking, "still tracking until MouseUp");
    }

    /// Consecutive moves inside keep `down` (containment unchanged — the loop
    /// body's `if (down != contains)` no-flip path).
    #[test]
    fn track_no_flip_when_containment_unchanged() {
        let mut b = tracked_button();
        let mut timers = TimerQueue::new();

        let mut ev = mouse_move(3, 0);
        with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(b.down);
        let mut ev = mouse_move(4, 0);
        with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(b.down, "still down on second inside move");
    }

    /// `MouseUp` while the last tracked state is inside → press fires
    /// (RECORD_HISTORY + the posted command), `down`/`tracking` cleared.
    #[test]
    fn track_release_inside_fires_press() {
        let mut b = tracked_button();
        let mut timers = TimerQueue::new();

        let mut ev = mouse_up(3, 0);
        let (out, ()) = with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "tracked up is consumed");
        assert!(!b.tracking, "tracking ends on MouseUp");
        assert!(!b.down, "pressed look cleared");
        assert_eq!(
            out[0],
            Event::Broadcast {
                command: Command::RECORD_HISTORY,
                source: None
            }
        );
        assert_eq!(out[1], Event::Command(Command::OK));
    }

    /// `MouseUp` at an INSIDE position after the last move tracked OUTSIDE →
    /// no press: the decision uses the LAST MOVE's containment, never the
    /// up-event's position (the hold loop never re-reads it).
    #[test]
    fn track_release_uses_last_move_not_up_position() {
        let mut b = tracked_button();
        let mut timers = TimerQueue::new();

        // Move outside: down = false.
        let mut ev = mouse_move(0, 0);
        with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(!b.down);

        // Up at (3, 0) — inside clickRect, but the up position must be ignored.
        let mut ev = mouse_up(3, 0);
        let (out, ()) = with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(ev.is_nothing(), "tracked up is consumed even without press");
        assert!(!b.tracking);
        assert!(out.is_empty(), "no press: last tracked state was outside");
    }

    /// A stray `MouseUp` with no tracking in flight falls through untouched
    /// (the `tracking` guard — MouseUp is not mask-gated in `Group::wants`).
    #[test]
    fn stray_mouse_up_without_tracking_is_ignored() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags::new(),
        );
        button_with_id(&mut b);
        let mut timers = TimerQueue::new();
        let mut ev = mouse_up(3, 0);
        let (out, ()) = with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(!ev.is_nothing(), "stray up is not consumed");
        assert!(out.is_empty(), "stray up fires nothing");
        assert!(!b.down);
    }

    /// A `bfBroadcast` button pressed via the mouse-track path fires its command
    /// as a broadcast carrying its own id — not as an `Event::Command`. The
    /// existing `broadcast_button_fires_broadcast_with_source` covers the
    /// keyboard/timer path; this covers the mouse path through the real arms
    /// (MouseDown inside, then tracked MouseUp inside).
    #[test]
    fn track_release_fires_broadcast_for_bf_broadcast_button() {
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
        let mut timers = TimerQueue::new();

        let mut ev = mouse_down(3, 0);
        with_ctx_d(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(b.tracking);

        let mut ev = mouse_up(3, 0);
        let (out, ()) = with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
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

    // -- behavior: set_state / make_default ---------------------------------

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

    // -- command graying via cmCommandSetChanged broadcast ---------------------

    /// `cmCommandSetChanged` with the button's command disabled: the button
    /// transitions to `state.disabled = true`.
    #[test]
    fn command_set_changed_grays_button_when_command_disabled() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags::new(),
        );
        assert!(!b.state.state.disabled, "starts enabled");
        let mut timers = TimerQueue::new();

        // Deliver a COMMAND_SET_CHANGED broadcast with OK in the disabled set.
        let mut ev = Event::Broadcast {
            command: Command::COMMAND_SET_CHANGED,
            source: None,
        };
        let (_, _, ()) = with_ctx_d(&mut timers, 0, |ctx| {
            // Seed the snapshot: OK is disabled.
            let mut ds = crate::command::CommandSet::new();
            ds.insert(Command::OK);
            ctx.set_disabled_commands(ds);
            b.handle_event(&mut ev, ctx)
        });
        assert!(
            b.state.state.disabled,
            "button must be disabled when its command is in the disabled set"
        );
        // The broadcast is NOT consumed (no clearEvent in the C++ arm either).
        assert!(
            !ev.is_nothing(),
            "cmCommandSetChanged is not consumed by the button"
        );
    }

    /// `cmCommandSetChanged` with the button's command re-enabled: the button
    /// transitions back to `state.disabled = false` (the un-gray path).
    #[test]
    fn command_set_changed_ungrays_button_when_command_enabled() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags::new(),
        );
        // Start disabled.
        b.state.state.disabled = true;
        let mut timers = TimerQueue::new();

        // COMMAND_SET_CHANGED with an empty disabled set: OK is enabled.
        let mut ev = Event::Broadcast {
            command: Command::COMMAND_SET_CHANGED,
            source: None,
        };
        with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(
            !b.state.state.disabled,
            "button must be re-enabled when its command is not in the disabled set"
        );
    }

    /// A disabled button does not arm on `cmDefault` (the `!self.state.state.disabled`
    /// guard is pre-existing; this test confirms graying + cmDefault interaction).
    #[test]
    fn grayed_default_button_ignores_cm_default() {
        let mut b = Button::new(
            Rect::new(0, 0, 10, 2),
            "OK",
            Command::OK,
            ButtonFlags {
                default: true,
                ..Default::default()
            },
        );
        let mut timers = TimerQueue::new();

        // Gray the button via COMMAND_SET_CHANGED.
        let mut ev = Event::Broadcast {
            command: Command::COMMAND_SET_CHANGED,
            source: None,
        };
        with_ctx_d(&mut timers, 0, |ctx| {
            let mut ds = crate::command::CommandSet::new();
            ds.insert(Command::OK);
            ctx.set_disabled_commands(ds);
            b.handle_event(&mut ev, ctx)
        });
        assert!(b.state.state.disabled);

        // cmDefault must not arm the animation.
        let mut ev = Event::Broadcast {
            command: Command::DEFAULT,
            source: None,
        };
        with_ctx(&mut timers, 0, |ctx| b.handle_event(&mut ev, ctx));
        assert!(
            !ev.is_nothing(),
            "disabled default button leaves cmDefault live"
        );
        assert!(
            b.animation_timer.is_none(),
            "no animation on a disabled button"
        );
    }
}
