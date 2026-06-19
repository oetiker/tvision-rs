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
//! **Guide:** [Commands & events](../../../apps/commands.html).
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
/// Match on an `&Event` in [`View::handle`](crate::view::View::handle) to
/// selectively react to the cases you care about; return the event unchanged to
/// let it propagate, or call [`Event::clear`] (or assign `*ev = Event::Nothing`)
/// to consume it. [`Event::Nothing`] serves dual duty: it is both "no pending
/// event" and the consumed-event sentinel.
///
/// The five mouse variants (`MouseDown`, `MouseUp`, `MouseMove`, `MouseAuto`,
/// `MouseWheel`) correspond one-to-one to the magiblot C++ event-class constants
/// (`evMouseDown`, `evMouseUp`, etc.), making each class independently matchable.
/// To handle any mouse event, match all five; to handle clicks only, match
/// `MouseDown` and `MouseUp`.
///
/// # Turbo Vision heritage
///
/// Replaces the `TEvent` tagged union (a bitmask tag plus a `union` of
/// mouse/key/message records; `system.h`) with a real Rust sum type, matched
/// arm-by-arm instead of masked (deviation D4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A mouse button was pressed. Deliver to focused view; position is in
    /// screen coordinates. Check [`MouseEvent::buttons`] for which button.
    MouseDown(MouseEvent),
    /// A mouse button was released. Paired with `MouseDown`; arrives at the
    /// view that captured the pointer.
    MouseUp(MouseEvent),
    /// The mouse moved without a button held (opt-in; see [`EventMask::mouse_move`]).
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
    /// A key was pressed. Match `Event::KeyDown(ke)` and inspect `ke.key` and
    /// `ke.modifiers` (see [`KeyEvent`]).
    KeyDown(KeyEvent),
    /// A command targeted at a specific receiver. The event loop routes it
    /// through the capture stack; the first view that recognises the command
    /// should consume it via [`Event::clear`].
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
    /// No pending event, or the consumed-event sentinel after [`Event::clear`].
    ///
    /// Both "the queue is empty" and "a handler consumed this event" map to
    /// this variant. To consume an event inside a handler: call
    /// `ev.clear()` or assign `*ev = Event::Nothing`. To test whether an event
    /// was consumed after returning from a sub-handler: call
    /// [`Event::is_nothing`].
    Nothing,
    /// Terminal bracketed-paste — the whole pasted string as delivered by the
    /// terminal. Routed identically to a key press — delivered only to the focused
    /// view, not broadcast. Handle it alongside `KeyDown` in input widgets that
    /// accept text.
    Paste(String),
}

impl Event {
    /// Mark this event as handled by setting it to [`Event::Nothing`].
    ///
    /// Call this inside a [`View::handle`](crate::view::View::handle)
    /// implementation when your view has fully processed the event and no other
    /// view should see it. After `clear()` the caller can call
    /// [`is_nothing`](Self::is_nothing) to confirm the event was consumed.
    /// Prefer `clear()` over assigning `Event::Nothing` directly — it makes the
    /// intent explicit.
    pub fn clear(&mut self) {
        *self = Event::Nothing;
    }

    /// Returns `true` if this event is [`Event::Nothing`] — either no event was
    /// produced or a prior handler consumed it via [`clear`](Self::clear).
    ///
    /// Use this to check, after dispatching to a sub-handler, whether the event
    /// was consumed (so you can skip further routing).
    pub fn is_nothing(&self) -> bool {
        matches!(self, Event::Nothing)
    }
}

/// A mouse event record: cursor position, buttons down, wheel direction,
/// click/move flags, and keyboard modifiers held at event time.
///
/// Carried by every mouse [`Event`] variant. Construct one with struct-update
/// syntax; all fields `Default` to safe zeros:
///
/// ```
/// use tvision_rs::event::{MouseEvent, MouseButtons};
/// let ev = MouseEvent {
///     buttons: MouseButtons { left: true, ..Default::default() },
///     ..Default::default()
/// };
/// assert!(ev.buttons.left);
/// ```
///
/// # Turbo Vision heritage
///
/// Ports `MouseEventType` (`system.h`). The `where` field (a Rust keyword)
/// becomes `position`; the `mb*`/`me*` bit-words become struct-of-bools / enums
/// and `controlKeyState` reuses [`KeyModifiers`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseEvent {
    /// Cursor position in **absolute** screen coordinates at the time of the event.
    ///
    /// Coordinates are in the root (screen) space; subtract the view's global
    /// origin to convert to view-local coordinates before hit-testing. The
    /// position is always set, even for events that don't logically have one
    /// (e.g. `MouseAuto` repeats carry the last known position).
    ///
    /// The C++ field was named `where` (a Rust keyword), renamed to `position`.
    ///
    /// # Turbo Vision heritage
    ///
    /// Ports the C++ global `MouseWhere: TPoint` (`drivers.cpp`), which held the
    /// last known mouse position globally. In tvision-rs the position is carried
    /// per-event in `MouseEvent::position`; there is no mutable global.
    pub position: Point,
    /// Which buttons are currently down at the time of the event.
    pub buttons: MouseButtons,
    /// Wheel direction for [`Event::MouseWheel`]; [`MouseWheel::None`] for all
    /// other event classes.
    pub wheel: MouseWheel,
    /// Double/triple-click and mouse-moved flags for this event.
    pub flags: MouseEventFlags,
    /// Keyboard modifiers held during the event (shift, ctrl, alt). Reuses
    /// [`KeyModifiers`] — the same struct used for key-down events.
    pub modifiers: KeyModifiers,
}

/// Which mouse buttons are currently held at the time of a [`MouseEvent`].
///
/// Check individual fields in a mouse handler to distinguish single-button
/// from multi-button presses:
///
/// ```
/// use tvision_rs::event::{Event, MouseEvent};
/// fn handle(ev: &Event) {
///     if let Event::MouseDown(me) = ev {
///         if me.buttons.left { /* left click */ }
///         if me.buttons.right { /* right click / context menu */ }
///     }
/// }
/// ```
///
/// # Turbo Vision heritage
///
/// The `mb*` button bitmask (`mbLeftButton = 0x01`, `mbRightButton = 0x02`)
/// becomes a struct-of-bools.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseButtons {
    /// Left mouse button.
    pub left: bool,
    /// Right mouse button.
    pub right: bool,
    /// Middle mouse button (scroll-wheel click).
    pub middle: bool,
}

/// Per-event flags for click-repeat detection and movement — a struct of named
/// `bool` flags carried in every [`MouseEvent`].
///
/// Check `flags.double_click` in a `MouseDown` handler to react to a
/// double-click without manually tracking timing. Note that there is no wheel
/// flag here: wheel state is the [`MouseEvent::wheel`] field, because a wheel
/// event is an independent event *class* (`Event::MouseWheel`), not a flag on
/// another class.
///
/// # Turbo Vision heritage
///
/// The magiblot `eventFlags: ushort` bitmask (`meMouseMoved = 0x01`,
/// `meDoubleClick = 0x02`, `meTripleClick = 0x04`) becomes a struct-of-bools
/// extending the 1992 guide's single `Double: Boolean` field.
/// The 1992 guide's `DoubleDelay: Word` global (double-click interval in
/// 1/18.2 s DOS timer ticks) has no equivalent: the double-click interval is
/// determined by the OS/terminal and delivered by crossterm; it cannot be
/// overridden at application level.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseEventFlags {
    /// This press completed a double click (two clicks within the platform
    /// double-click interval).
    ///
    /// Check this flag in a [`Event::MouseDown`] handler to distinguish a
    /// double-click from a single click. The timing interval is OS-defined and
    /// delivered pre-computed by the crossterm backend.
    pub double_click: bool,
    /// This press completed a triple click.
    pub triple_click: bool,
    /// The mouse moved since the last event.
    pub mouse_moved: bool,
}

/// The direction of mouse wheel motion carried by a [`MouseEvent`].
///
/// Only meaningful when the enclosing event is [`Event::MouseWheel`]; for all
/// other event classes the field is [`MouseWheel::None`]. Match it to scroll
/// content:
///
/// ```
/// use tvision_rs::event::{Event, MouseWheel};
/// fn handle(ev: &Event) {
///     if let Event::MouseWheel(me) = ev {
///         match me.wheel {
///             MouseWheel::Up   => { /* scroll up   */ }
///             MouseWheel::Down => { /* scroll down */ }
///             _ => {}
///         }
///     }
/// }
/// ```
///
/// # Turbo Vision heritage
///
/// Replaces the magiblot `uchar wheel` field and `mw*` constants with a
/// closed enum; the `None` variant replaces the `0` value for "no wheel".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MouseWheel {
    /// No wheel motion (the default; present on non-wheel mouse events).
    #[default]
    None,
    /// Wheel scrolled up (towards the user).
    Up,
    /// Wheel scrolled down (away from the user).
    Down,
    /// Wheel scrolled left (horizontal scroll).
    Left,
    /// Wheel scrolled right (horizontal scroll).
    Right,
}

/// Per-view opt-in mask for the *expensive* continuous event classes.
///
/// **Most event classes are always on** — `MouseDown`, `MouseUp`, `KeyDown`,
/// `Command`, `Broadcast`, `Timer`, and `Paste` are delivered unconditionally to
/// every eligible view. Only the high-frequency classes that are costly to route
/// through the whole tree need an explicit opt-in:
///
/// - [`mouse_move`](Self::mouse_move) — fires on every cursor movement; opt in
///   only if your view needs real-time tracking (e.g. a custom drag target).
/// - [`mouse_auto`](Self::mouse_auto) — fires repeatedly while a button is held;
///   opt in for views like scrollbars that need auto-repeat.
///
/// Set these fields on a view's [`EventMask`] field to enable the classes you
/// need. Views that do not set them never receive the corresponding events, so
/// there is no performance cost for the common case.
///
/// # Turbo Vision heritage
///
/// The surviving slice of the `TView::eventMask` bit-word, trimmed to the two
/// opt-ins that are non-trivial to route selectively. The
/// always-on classes needed no flag in C++ either; explicitly tracking them here
/// would be noise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventMask {
    /// Opt in to [`Event::MouseMove`] (fires on every cursor movement).
    pub mouse_move: bool,
    /// Opt in to [`Event::MouseAuto`] (fires repeatedly while a button is held).
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
