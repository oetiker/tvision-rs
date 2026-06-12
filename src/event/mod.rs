//! Events — the [`Event`] enum and the key/mouse records that fill it.
//!
//! [`Event`] is the sum type a [`View`](crate::view::View) matches arm-by-arm:
//! mouse presses/releases/moves/wheel, key presses, commands, broadcasts,
//! timer expiries, and paste. The keyboard side lives in the [`key`] submodule
//! ([`Key`], [`KeyEvent`], [`KeyModifiers`]); the mouse record ([`MouseEvent`])
//! and the [`EventMask`] opt-in for expensive event classes live here.
//!
//! Two events carry payloads worth naming up front. [`Event::Broadcast`] adds
//! an optional **`source: ViewId`** naming *which view the broadcast is about*
//! (e.g. which scrollbar changed) — a resolvable handle, `None` for broadcasts
//! about no particular view. [`Event::Timer`] carries *which* [`TimerId`]
//! expired.
//!
//! One primitive is deliberately not reproduced: a synchronous, return-valued
//! message round-trip (the kind used for "may I close?" vetoes). A view here is
//! borrowed downward by the event loop and cannot synchronously call back into a
//! sibling and read a return value, so there is no event that carries a reply.
//! Code that needs that pattern queries the tree owner directly via
//! [`find_mut`](crate::view::View::find_mut) instead.
//!
//! # Turbo Vision heritage
//!
//! Replaces the single `TEvent` tagged union (`system.h`) with a Rust sum type
//! (deviation D4). The original message event carried a command plus an untyped
//! pointer used three ways; here a command event carries only the [`Command`], the
//! "which view" case becomes [`Event::Broadcast`]'s resolvable [`ViewId`]
//! `source`, and the timer-id case becomes the typed [`Event::Timer`] variant.

mod key;

pub use key::{
    Key, KeyEvent, KeyModifiers, ctrl_to_arrow, hot_key, is_alt_hotkey, is_plain_hotkey,
};

use crate::command::Command;
use crate::timer::TimerId;
use crate::view::{Point, ViewId};

/// An event a [`View`](crate::view::View) handles, matched arm-by-arm.
///
/// A consumed event and "no event" are both [`Event::Nothing`] (see
/// [`Event::clear`]). The mouse wheel is its own variant [`Event::MouseWheel`]
/// (distinct from `MouseDown`); wheel direction rides on the
/// [`MouseEvent::wheel`] field.
///
/// # Turbo Vision heritage
///
/// Replaces the `TEvent` tagged union (a bitmask tag plus a `union` of
/// mouse/key/message records; `system.h`) with a real Rust sum type, matched
/// arm-by-arm instead of masked (deviation D4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A mouse button was pressed.
    MouseDown(MouseEvent),
    /// A mouse button was released.
    MouseUp(MouseEvent),
    /// The mouse moved (opt-in; see [`EventMask::mouse_move`]).
    MouseMove(MouseEvent),
    /// Auto-repeat while a button is held (opt-in; see [`EventMask::mouse_auto`]).
    MouseAuto(MouseEvent),
    /// The mouse wheel was rotated. A **distinct event class** from `MouseDown`:
    /// the wheel is **not** positional and **not** focused, so a
    /// [`crate::view::Group`] broadcasts it to every child until one consumes it —
    /// the active window's scrollbar gets it regardless of cursor position.
    /// Carries a [`MouseEvent`] whose [`MouseEvent::wheel`] is
    /// `Up`/`Down`/`Left`/`Right`.
    MouseWheel(MouseEvent),
    /// A key was pressed. Reuses [`key::KeyEvent`].
    KeyDown(KeyEvent),
    /// A command targeted at a specific receiver.
    Command(Command),
    /// A command broadcast to interested views. `source` names *which view this
    /// broadcast is about* (e.g. which scrollbar changed), as a resolvable
    /// [`ViewId`]. `None` for broadcasts about no particular view.
    Broadcast {
        command: Command,
        source: Option<ViewId>,
    },
    /// A timer fired, carrying *which* [`TimerId`] expired.
    ///
    /// The timer id is an integer payload, not a view subject, so it gets its own
    /// typed variant rather than reusing [`Event::Broadcast`]'s `source` field
    /// (which is for the view-subject case only). Delivered to all views.
    Timer(TimerId),
    /// No event, or an event that a handler has consumed via [`Event::clear`].
    Nothing,
    /// Terminal bracketed-paste — the whole pasted string as delivered by the
    /// terminal. Routed identically to a key press — delivered only to the focused
    /// view, not broadcast.
    Paste(String),
}

impl Event {
    /// Consume this event by setting it to [`Event::Nothing`].
    pub fn clear(&mut self) {
        *self = Event::Nothing;
    }

    /// Whether this is [`Event::Nothing`] (no event / a consumed event).
    pub fn is_nothing(&self) -> bool {
        matches!(self, Event::Nothing)
    }
}

/// A mouse event record: cursor position, the buttons down, wheel direction,
/// click/move flags, and the modifiers held.
///
/// # Turbo Vision heritage
///
/// Ports `MouseEventType` (`system.h`). The `where` field (a Rust keyword)
/// becomes `position`; the `mb*`/`me*` bit-words become struct-of-bools / enums
/// (deviation D5); and `controlKeyState` reuses [`key::KeyModifiers`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseEvent {
    /// Cursor position, in the relevant coordinate space.
    pub position: Point,
    /// Buttons currently down.
    pub buttons: MouseButtons,
    /// Wheel direction, if any.
    pub wheel: MouseWheel,
    /// Click/move flags.
    pub flags: MouseEventFlags,
    /// Modifiers held during the event (reuses [`key::KeyModifiers`]).
    pub modifiers: KeyModifiers,
}

/// The mouse buttons currently down — a struct of three named `bool` flags.
///
/// # Turbo Vision heritage
/// The `mb*` button bit-word becomes a struct-of-bools (deviation D5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseButtons {
    /// Left button.
    pub left: bool,
    /// Right button.
    pub right: bool,
    /// Middle button.
    pub middle: bool,
}

/// Mouse event flags: double/triple click and moved — a struct of named `bool`
/// flags.
///
/// There is no wheel flag here — wheel state lives on [`MouseEvent::wheel`],
/// because the wheel is an event *class*, not a flag.
///
/// # Turbo Vision heritage
/// The `me*` event-flag bit-word becomes a struct-of-bools (deviation D5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseEventFlags {
    /// The press completed a double click.
    pub double_click: bool,
    /// The press completed a triple click.
    pub triple_click: bool,
    /// The mouse moved.
    pub mouse_moved: bool,
}

/// Mouse wheel direction; [`MouseWheel::None`] means no wheel motion — a closed
/// enum.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MouseWheel {
    /// No wheel motion.
    #[default]
    None,
    /// Wheel scrolled up.
    Up,
    /// Wheel scrolled down.
    Down,
    /// Wheel scrolled left.
    Left,
    /// Wheel scrolled right.
    Right,
}

/// The per-view opt-in for *expensive* event classes.
///
/// The always-on classes — mouse-down/up, key-down, command, broadcast — are
/// **not** gated by this struct; they are delivered unconditionally. Only the
/// costly opt-ins (continuous mouse tracking, auto-repeat) need a flag.
///
/// # Turbo Vision heritage
///
/// The surviving slice of the `eventMask` bit-word (`TView::eventMask`), trimmed
/// to the two opt-ins worth keeping (deviation D4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventMask {
    /// Deliver [`Event::MouseMove`] (continuous tracking).
    pub mouse_move: bool,
    /// Deliver [`Event::MouseAuto`] (auto-repeat while held).
    pub mouse_auto: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_variant_constructs() {
        let m = MouseEvent::default();
        let _ = Event::MouseDown(m);
        let _ = Event::MouseUp(m);
        let _ = Event::MouseMove(m);
        let _ = Event::MouseAuto(m);
        let _ = Event::KeyDown(KeyEvent::from(Key::Enter));
        let _ = Event::Command(Command::OK);
        let _ = Event::Broadcast {
            command: Command::OK,
            source: None,
        };
        let _ = Event::Nothing;
        let _ = Event::Paste("hi".to_string());
    }

    #[test]
    fn clear_sets_nothing_and_is_nothing() {
        let mut ev = Event::KeyDown(KeyEvent::from(Key::Enter));
        assert!(!ev.is_nothing());
        ev.clear();
        assert_eq!(ev, Event::Nothing);
        assert!(ev.is_nothing());
    }

    #[test]
    fn key_down_round_trip() {
        let ev = Event::KeyDown(KeyEvent::from(Key::Enter));
        match ev {
            Event::KeyDown(k) => {
                assert_eq!(k.key, Key::Enter);
                assert_eq!(k.modifiers, KeyModifiers::default());
            }
            _ => panic!("expected KeyDown"),
        }
    }

    #[test]
    fn mouse_event_default_is_empty() {
        let m = MouseEvent::default();
        assert_eq!(m.position, Point::default());
        assert!(!m.buttons.left);
        assert!(!m.buttons.right);
        assert!(!m.buttons.middle);
        assert_eq!(m.wheel, MouseWheel::None);
        assert!(!m.flags.double_click);
        assert!(!m.flags.triple_click);
        assert!(!m.flags.mouse_moved);
        assert_eq!(m.modifiers, KeyModifiers::default());
    }

    #[test]
    fn struct_of_bools_defaults_all_false() {
        assert_eq!(
            MouseButtons::default(),
            MouseButtons {
                left: false,
                right: false,
                middle: false,
            }
        );
        assert_eq!(
            MouseEventFlags::default(),
            MouseEventFlags {
                double_click: false,
                triple_click: false,
                mouse_moved: false,
            }
        );
        assert_eq!(
            EventMask::default(),
            EventMask {
                mouse_move: false,
                mouse_auto: false,
            }
        );
    }

    #[test]
    fn wheel_up_mouse_event() {
        let m = MouseEvent {
            wheel: MouseWheel::Up,
            ..Default::default()
        };
        let ev = Event::MouseWheel(m);
        match ev {
            Event::MouseWheel(me) => assert_eq!(me.wheel, MouseWheel::Up),
            _ => panic!("expected MouseWheel"),
        }
    }
}
