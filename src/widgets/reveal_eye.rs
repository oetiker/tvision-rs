//! A one-cell focusable "reveal" eye for masked [`InputLine`](super::InputLine)
//! fields.
//!
//! Drives a paired field's reveal: press-and-hold the mouse for a momentary
//! peek, or (while focused) press Space for a 1 s timed peek — or, when
//! configured `sticky`, toggle a latched reveal. It carries no secret text; it
//! only tracks its own intent and broadcasts [`Command::REVEAL_CHANGED`] so the
//! owning `MaskedInput` (added in a later task) can apply it.

use std::time::Duration;

use crate::command::Command;
use crate::event::Key;
use crate::theme::SurfaceRoles;
use crate::timer::TimerId;
use crate::view::{Context, DrawCtx, View, ViewState};
use crate::{Event, Options, Point, Rect, Role};

/// Look + behaviour of a [`RevealEye`].
#[derive(Debug, Clone, Copy)]
pub struct RevealEyeConfig {
    /// Glyph shown while the field is hidden. Default `⊝` (U+229D).
    pub hidden_glyph: char,
    /// Glyph shown while the field is revealed. Default `◉` (U+25C9).
    pub revealed_glyph: char,
    /// Timed-peek duration for a non-sticky Space press. Default 1 s.
    pub peek: Duration,
    /// When true, Space toggles a latched reveal instead of a timed peek.
    pub sticky: bool,
}

impl Default for RevealEyeConfig {
    fn default() -> Self {
        RevealEyeConfig {
            hidden_glyph: '⊝',
            revealed_glyph: '◉',
            peek: Duration::from_secs(1),
            sticky: false,
        }
    }
}

/// A focusable one-cell reveal toggle. See the module docs.
pub struct RevealEye {
    pub state: ViewState,
    cfg: RevealEyeConfig,
    /// Mouse button held on the eye (momentary peek).
    held: bool,
    /// Latched reveal (sticky mode).
    sticky_on: bool,
    /// Live one-shot peek timer (non-sticky Space), if any.
    pub(crate) peek_timer: Option<TimerId>,
    /// Absolute origin cached each `draw` for the mouse-track capture.
    abs_origin: Point,
}

impl RevealEye {
    /// Build a one-cell eye. `bounds` should be 1×1.
    pub fn new(bounds: Rect, cfg: RevealEyeConfig) -> Self {
        let mut state = ViewState::new(bounds);
        state.options = Options {
            selectable: true,
            first_click: true,
            ..Default::default()
        };
        RevealEye {
            state,
            cfg,
            held: false,
            sticky_on: false,
            peek_timer: None,
            abs_origin: Point::new(0, 0),
        }
    }

    /// Whether the paired field should currently show cleartext.
    pub fn is_revealing(&self) -> bool {
        self.held || self.sticky_on || self.peek_timer.is_some()
    }

    fn announce(&self, ctx: &mut Context) {
        ctx.broadcast(Command::REVEAL_CHANGED, self.state.id());
    }
}

/// [`RevealEye`]'s surface triple for [`DrawCtx::content_surface`] — mirrors
/// [`InputLine`](super::InputLine)'s private `SURFACE_ROLES` const, which is
/// not `pub` and so cannot be reused directly.
const SURFACE_ROLES: SurfaceRoles = SurfaceRoles {
    normal: Role::InputNormal,
    surface: Role::InputSurface,
    inactive: Role::InputInactive,
};

impl View for RevealEye {
    fn state(&self) -> &ViewState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        self.abs_origin = ctx.origin();
        let color = ctx.content_surface(
            SURFACE_ROLES,
            self.state.state.focused,
            self.state.options.selectable,
        );
        let glyph = if self.is_revealing() {
            self.cfg.revealed_glyph
        } else {
            self.cfg.hidden_glyph
        };
        ctx.fill(Rect::new(0, 0, self.state.size.x, 1), ' ', color);
        ctx.put_char(0, 0, glyph, color);
    }

    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        match ev {
            Event::MouseDown(_) => {
                if let Some(id) = self.state.id() {
                    self.held = true;
                    ctx.start_mouse_track(
                        id,
                        self.abs_origin,
                        crate::capture::TrackMask::default(),
                    );
                    self.announce(ctx);
                    ev.clear();
                }
            }
            Event::MouseUp(_) if self.held => {
                self.held = false;
                self.announce(ctx);
                ev.clear();
            }
            Event::KeyDown(k) if k.key == Key::Char(' ') => {
                if self.cfg.sticky {
                    self.sticky_on = !self.sticky_on;
                } else {
                    if let Some(t) = self.peek_timer.take() {
                        ctx.kill_timer(t);
                    }
                    self.peek_timer = Some(ctx.set_timer(self.cfg.peek, None));
                }
                self.announce(ctx);
                ev.clear();
            }
            Event::Timer(id) if Some(*id) == self.peek_timer => {
                self.peek_timer = None;
                self.announce(ctx);
                ev.clear();
            }
            // Tab, arrows, and everything else pass through for field navigation.
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{
        Key, KeyEvent, KeyModifiers, MouseButtons, MouseEvent, MouseEventFlags, MouseWheel,
    };
    use crate::timer::TimerQueue;
    use crate::view::{Context, Deferred, ViewId};
    use crate::{Event, Point, Rect};
    use std::collections::VecDeque;

    fn eye(sticky: bool) -> RevealEye {
        let mut e = RevealEye::new(
            Rect::new(0, 0, 1, 1),
            RevealEyeConfig {
                sticky,
                ..Default::default()
            },
        );
        e.state_mut().id = Some(ViewId::next());
        e.state_mut().state.selected = true; // stand in for focus in a unit test
        e
    }

    /// Build a fresh `Context` over the given (persistent) `TimerQueue`, run
    /// `f`, and return the posted/broadcast events, the deferred queue, and
    /// `f`'s result. `timers` is owned by the caller so a test can inspect it
    /// (`len()`) after the dispatch — there is no `Deferred::SetTimer`;
    /// `Context::set_timer`/`kill_timer` mutate the injected `TimerQueue`
    /// directly (see `Context::set_timer`). Mirrors `button.rs`'s
    /// `with_ctx_d` so tests can assert on both the `REVEAL_CHANGED`
    /// broadcast and any `Deferred::PushCapture`.
    fn with_ctx<R>(
        timers: &mut TimerQueue,
        f: impl FnOnce(&mut Context) -> R,
    ) -> (Vec<Event>, Vec<Deferred>, R) {
        let mut out: VecDeque<Event> = VecDeque::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let r = {
            let mut ctx = Context::new(&mut out, timers, 0, &mut deferred);
            f(&mut ctx)
        };
        (out.into_iter().collect(), deferred, r)
    }

    /// Assert `out` contains exactly one `Command::REVEAL_CHANGED` broadcast,
    /// sourced from `id`.
    fn assert_reveal_changed(out: &[Event], id: Option<ViewId>) {
        let matches: Vec<_> = out
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Event::Broadcast { command, source }
                        if *command == Command::REVEAL_CHANGED && *source == id
                )
            })
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "expected exactly one REVEAL_CHANGED broadcast (source={:?}) in {:?}",
            id,
            out
        );
    }

    fn mouse_down() -> Event {
        Event::MouseDown(MouseEvent {
            position: Point::new(0, 0),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            wheel: MouseWheel::None,
            flags: MouseEventFlags::default(),
            modifiers: KeyModifiers::default(),
        })
    }
    fn mouse_up() -> Event {
        Event::MouseUp(MouseEvent {
            position: Point::new(0, 0),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            wheel: MouseWheel::None,
            flags: MouseEventFlags::default(),
            modifiers: KeyModifiers::default(),
        })
    }

    #[test]
    fn mouse_hold_reveals_until_release() {
        let mut e = eye(false);
        let id = e.state.id();
        let mut timers = TimerQueue::new();
        let (out, deferred, ()) = with_ctx(&mut timers, |ctx| {
            let mut d = mouse_down();
            e.handle_event(&mut d, ctx);
        });
        assert!(e.is_revealing(), "press reveals");
        assert_reveal_changed(&out, id);
        // A mouse-tracking capture must have been queued, targeting the
        // eye's own view id (mirrors button.rs's mouse_down_inside_arms_tracking).
        assert_eq!(deferred.len(), 1, "one capture deferred");
        assert!(
            matches!(deferred[0], Deferred::PushCapture(_)),
            "deferred[0] is PushCapture"
        );
        if let Deferred::PushCapture(ref h) = deferred[0] {
            assert_eq!(h.view(), id, "capture tracks the eye's own id");
        }

        let (out, _deferred, ()) = with_ctx(&mut timers, |ctx| {
            let mut u = mouse_up();
            e.handle_event(&mut u, ctx);
        });
        assert!(!e.is_revealing(), "release hides");
        assert_reveal_changed(&out, id);
    }

    #[test]
    fn space_non_sticky_arms_a_timed_peek() {
        let mut e = eye(false);
        let id = e.state.id();
        let mut timers = TimerQueue::new();
        let (out, _deferred, ()) = with_ctx(&mut timers, |ctx| {
            let mut sp = Event::KeyDown(KeyEvent::from(Key::Char(' ')));
            e.handle_event(&mut sp, ctx);
        });
        assert!(e.is_revealing(), "Space reveals");
        assert_eq!(timers.len(), 1, "a one-shot timer is armed");
        assert_reveal_changed(&out, id);

        // The matching Timer hides it again.
        let tid = e.peek_timer.expect("timer id stored");
        let (out, _deferred, ()) = with_ctx(&mut timers, |ctx| {
            let mut t = Event::Timer(tid);
            e.handle_event(&mut t, ctx);
        });
        assert!(!e.is_revealing(), "timer expiry hides");
        assert_reveal_changed(&out, id);
    }

    #[test]
    fn space_sticky_toggles() {
        let mut e = eye(true);
        let id = e.state.id();
        let mut timers = TimerQueue::new();
        let (out, _deferred, ()) = with_ctx(&mut timers, |ctx| {
            let mut sp = Event::KeyDown(KeyEvent::from(Key::Char(' ')));
            e.handle_event(&mut sp, ctx);
        });
        assert!(e.is_revealing(), "sticky Space latches on");
        assert_reveal_changed(&out, id);
        let (out, _deferred, ()) = with_ctx(&mut timers, |ctx| {
            let mut sp = Event::KeyDown(KeyEvent::from(Key::Char(' ')));
            e.handle_event(&mut sp, ctx);
        });
        assert!(!e.is_revealing(), "sticky Space latches off");
        assert_reveal_changed(&out, id);
    }
}
