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
//! # Turbo Vision heritage
//!
//! Replaces the single `TEvent` tagged union (`system.h`) with a Rust sum type
//! (deviation D4). C++ `MessageEvent` carried a `command` plus a `void* infoPtr`
//! union used three ways; here a command event carries only the [`Command`], the
//! "which view" case becomes [`Event::Broadcast`]'s resolvable [`ViewId`]
//! `source`, and the timer-id case becomes the typed [`Event::Timer`] variant.
//!
//! One C++ primitive is deliberately not reproduced: the synchronous,
//! return-valued `message()` round-trip (the `cmCanCloseForm` veto and
//! friends). A view here is borrowed downward by the event loop and cannot
//! synchronously call back into a sibling and read a return value, so there is
//! no event that carries a reply. Code that needs that pattern queries the tree
//! owner directly via [`find_mut`](crate::view::View::find_mut) instead.

mod key;

pub use key::{
    Key, KeyEvent, KeyModifiers, ctrl_to_arrow, hot_key, is_alt_hotkey, is_plain_hotkey,
};

use crate::command::Command;
use crate::timer::TimerId;
use crate::view::{Point, ViewId};

/// An event a [`View`](crate::view::View) handles, matched arm-by-arm.
///
/// `evNothing` and any *consumed* event are both [`Event::Nothing`] (see
/// [`Event::clear`], the `clearEvent` equivalent). The mouse wheel is its own
/// variant [`Event::MouseWheel`] (distinct from `MouseDown`); wheel direction
/// rides on the [`MouseEvent::wheel`] field.
///
/// # Turbo Vision heritage
///
/// Replaces the `TEvent` tagged union (`what` bitmask + `union { mouse; keyDown;
/// message; }`; `system.h`) with a real Rust sum type, matched arm-by-arm
/// instead of masked (deviation D4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// `evMouseDown` — a mouse button was pressed.
    MouseDown(MouseEvent),
    /// `evMouseUp` — a mouse button was released.
    MouseUp(MouseEvent),
    /// `evMouseMove` — the mouse moved (opt-in; see [`EventMask::mouse_move`]).
    MouseMove(MouseEvent),
    /// `evMouseAuto` — auto-repeat while a button is held (opt-in; see
    /// [`EventMask::mouse_auto`]).
    MouseAuto(MouseEvent),
    /// `evMouseWheel` — the mouse wheel was rotated. A **distinct event class**
    /// from `evMouseDown` (faithful to `views.h:199`,
    /// `positionalEvents = evMouse & ~evMouseWheel`): the wheel is **not**
    /// positional and **not** focused, so a [`crate::view::Group`] broadcasts it
    /// to every child (`forEach`/`TGroup::handleEvent` `else` branch) until one
    /// consumes it — the active window's scrollbar gets it regardless of cursor
    /// position. Carries a [`MouseEvent`] whose [`MouseEvent::wheel`] is
    /// `Up`/`Down`/`Left`/`Right`.
    MouseWheel(MouseEvent),
    /// `evKeyDown` — a key was pressed. Reuses [`key::KeyEvent`].
    KeyDown(KeyEvent),
    /// `evCommand` — a command targeted at a specific receiver.
    Command(Command),
    /// `evBroadcast` — a command broadcast to interested views. `source` names
    /// *which view this broadcast is about* (e.g. which scrollbar changed), as a
    /// resolvable [`ViewId`] (the successor to the C++ `message.infoPtr`). `None`
    /// for broadcasts about no particular view (pump-internal
    /// `cmCommandSetChanged`).
    Broadcast {
        command: Command,
        source: Option<ViewId>,
    },
    /// The successor to `evBroadcast cmTimerExpired`: a timer fired, carrying
    /// *which* [`TimerId`] expired.
    ///
    /// In C++ the timer-expiry broadcast was `evBroadcast` with
    /// `message.command == cmTimerExpired` and `message.infoPtr ==` the
    /// `TTimerId`. That `infoPtr` is an **integer** payload (the timer id), not a
    /// view subject, so it gets its own typed variant rather than reusing
    /// [`Event::Broadcast`]'s `source` field (which is for the view-subject case
    /// only). Routed **broadcast-class** (delivered to all views), faithful to
    /// `evBroadcast`.
    Timer(TimerId),
    /// `evNothing`, or an event that a handler has consumed via
    /// [`Event::clear`].
    Nothing,
    /// Terminal bracketed-paste — the whole pasted string as delivered by the
    /// terminal. Replaces the C++ `kbPaste`-flagged `evKeyDown` stream (tevent.cpp
    /// `setPasteText`/`getPasteEvent`). Routed identically to `evKeyDown` —
    /// delivered only to the focused view, not broadcast.
    Paste(String),
}

impl Event {
    /// Consume this event by setting it to [`Event::Nothing`]. This is the
    /// `clearEvent` equivalent; the porting recipe maps `clearEvent(event)`
    /// onto `event.clear()`.
    pub fn clear(&mut self) {
        *self = Event::Nothing;
    }

    /// Whether this is [`Event::Nothing`] (`evNothing` / a consumed event).
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
    /// Cursor position, in the relevant coordinate space (`where`).
    pub position: Point,
    /// Buttons currently down (`buttons`).
    pub buttons: MouseButtons,
    /// Wheel direction, if any (`wheel`).
    pub wheel: MouseWheel,
    /// Click/move flags (`eventFlags`).
    pub flags: MouseEventFlags,
    /// Modifiers held during the event (`controlKeyState`; reuses
    /// [`key::KeyModifiers`]).
    pub modifiers: KeyModifiers,
}

/// The mouse buttons currently down. Ports the `buttons` `mb*` bit-word
/// (`system.h`) as a struct-of-bools (deviation D5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseButtons {
    /// Left button (`mbLeftButton`).
    pub left: bool,
    /// Right button (`mbRightButton`).
    pub right: bool,
    /// Middle button (`mbMiddleButton`).
    pub middle: bool,
}

/// Mouse event flags: double/triple click and moved. Ports the `eventFlags`
/// `me*` bit-word (`system.h`) as a struct-of-bools (deviation D5).
///
/// There is no wheel flag here — wheel state lives on [`MouseEvent::wheel`],
/// because the wheel is an event *class*, not a flag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseEventFlags {
    /// The press completed a double click (`meDoubleClick`).
    pub double_click: bool,
    /// The press completed a triple click (`meTripleClick`).
    pub triple_click: bool,
    /// The mouse moved (`meMouseMoved`).
    pub mouse_moved: bool,
}

/// Mouse wheel direction; [`MouseWheel::None`] means no wheel motion. Ports the
/// `wheel` `mw*` closed set (`system.h`) as an enum.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MouseWheel {
    /// No wheel motion.
    #[default]
    None,
    /// Wheel scrolled up (`mwUp`).
    Up,
    /// Wheel scrolled down (`mwDown`).
    Down,
    /// Wheel scrolled left (`mwLeft`).
    Left,
    /// Wheel scrolled right (`mwRight`).
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
/// The surviving slice of the `ushort eventMask` bit-word (`TView::eventMask`),
/// trimmed to the two opt-ins worth keeping (deviation D4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventMask {
    /// Deliver [`Event::MouseMove`] (continuous tracking; `evMouseMove`).
    pub mouse_move: bool,
    /// Deliver [`Event::MouseAuto`] (auto-repeat while held; `evMouseAuto`).
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
