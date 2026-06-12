//! Events — deviation **D4** (TV's event records as a Rust sum type).
//!
//! Turbo Vision's single `TEvent` (a tagged union over `evKeyDown`,
//! `evMouse*`, `evCommand`/`evBroadcast`, `evNothing`; `system.h`) becomes an
//! idiomatic [`Event`] enum matched arm-by-arm. The keyboard side lives in the
//! [`key`] submodule ([`Key`], [`KeyEvent`], [`KeyModifiers`]); the sum type,
//! the mouse record, and the trimmed [`EventMask`] opt-in live here.
//!
//! **`infoPtr` / `MessageEvent` (D4).** `TEvent`'s `MessageEvent` carried a
//! `command` plus a `void* infoPtr` union used three unrelated ways. We do not
//! reinstate the synchronous round-trip on the event itself: [`Event::Command`]
//! carries **only** the [`Command`]. [`Event::Broadcast`] additionally carries an
//! optional **`source: ViewId`** — the broadcast-subject successor to `infoPtr`,
//! naming *which view this broadcast is about* (e.g. which scrollbar changed) as a
//! resolvable [`ViewId`] rather than a `void*`. The timer-id integer payload (the
//! third `infoPtr` use-case) gets its own typed variant [`Event::Timer`] rather
//! than being forced into `Broadcast`'s `source` field, which carries a [`ViewId`],
//! not an integer. The synchronous return-consuming `message()` primitive (the
//! `cmCanCloseForm` veto and friends) is deferred to row 34, where it lives on the
//! tree owner over `find_mut`, not on the event.

mod key;

pub use key::{
    Key, KeyEvent, KeyModifiers, ctrl_to_arrow, hot_key, is_alt_hotkey, is_plain_hotkey,
};

use crate::command::Command;
use crate::timer::TimerId;
use crate::view::{Point, ViewId};

/// A Turbo Vision event — deviation **D4**. Replaces the `TEvent` tagged union
/// (`what` bitmask + `union { mouse; keyDown; message; }`; `system.h`) with a
/// real Rust sum type, matched arm-by-arm instead of masked.
///
/// The `ev*` event classes map onto the variants directly. `evNothing` and any
/// *consumed* event are both [`Event::Nothing`] (see [`Event::clear`], the
/// `clearEvent` equivalent). The `evMouseWheel` class is its own variant
/// [`Event::MouseWheel`] (distinct from `evMouseDown`, faithful to
/// `views.h`); wheel direction rides on the [`MouseEvent::wheel`] field,
/// faithful to the C++ `MouseEventType::wheel`.
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
    /// `evBroadcast` — a command broadcast to interested views. `source`
    /// reinstates the C++ `message.infoPtr` for the broadcast-subject case (D4
    /// amendment): it names *which view this broadcast is about* (e.g. which
    /// scrollbar changed), as a resolvable [`ViewId`] rather than a `void*`.
    /// `None` for broadcasts that are about no particular view (pump-internal
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
    /// view subject, so — per the project's Phase-A precedent — it gets its own
    /// typed variant rather than reusing [`Event::Broadcast`]'s `source` field
    /// (which is for the view-subject `infoPtr` case only). Routed
    /// **broadcast-class** (delivered to all views), faithful to `evBroadcast`.
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

/// A mouse event record — ports `MouseEventType` (`system.h`):
/// `{ TPoint where; ushort eventFlags; ushort controlKeyState; uchar buttons;
/// uchar wheel; }`. The `where` field (a Rust keyword) becomes `position`; the
/// bit-words become struct-of-bools / enums per deviation **D5**; and
/// `controlKeyState` reuses [`key::KeyModifiers`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseEvent {
    /// Cursor position, in the relevant coordinate space (`where`).
    pub position: Point,
    /// Buttons currently down (`buttons`, the `mb*` bit-word; D5).
    pub buttons: MouseButtons,
    /// Wheel direction, if any (`wheel`, the `mw*` set).
    pub wheel: MouseWheel,
    /// Click/move flags (`eventFlags`, the `me*` bit-word; D5).
    pub flags: MouseEventFlags,
    /// Modifiers held during the event (`controlKeyState`; reuses
    /// [`key::KeyModifiers`]).
    pub modifiers: KeyModifiers,
}

/// The mouse buttons currently down — deviation **D5**, replacing the `buttons`
/// `mb*` bit-word (`system.h`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseButtons {
    /// Left button (`mbLeftButton`).
    pub left: bool,
    /// Right button (`mbRightButton`).
    pub right: bool,
    /// Middle button (`mbMiddleButton`).
    pub middle: bool,
}

/// Mouse event flags — deviation **D5**, replacing the `eventFlags` `me*`
/// bit-word (`system.h`).
///
/// `system.h` defines exactly three `me*` flags; there is **no** `meMouseWheel`
/// (`evMouseWheel` is an event-*class* bit in `what`, not an `me*` flag). Wheel
/// state therefore lives on [`MouseEvent::wheel`], not here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseEventFlags {
    /// The press completed a double click (`meDoubleClick`).
    pub double_click: bool,
    /// The press completed a triple click (`meTripleClick`).
    pub triple_click: bool,
    /// The mouse moved (`meMouseMoved`).
    pub mouse_moved: bool,
}

/// Mouse wheel direction — deviation **D1**, replacing the `wheel` `mw*` closed
/// set (`system.h`). [`MouseWheel::None`] means no wheel motion.
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

/// The per-view opt-in for *expensive* event classes — deviation **D4**, the
/// surviving slice of the `ushort eventMask` bit-word (`TView::eventMask`).
///
/// The always-on classes — mouse-down/up, key-down, command, broadcast — are
/// **not** gated by this struct; they are delivered unconditionally. Per D4
/// only the costly opt-ins (continuous mouse tracking, auto-repeat) are worth
/// keeping, so the bit-word collapses to these two bools.
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
