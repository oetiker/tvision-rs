//! The framed, selectable, movable [`Window`].
//!
//! A [`Window`] is the thing a user actually sees and drives: a [`Group`] that
//! builds a [`Frame`](crate::frame::Frame) around itself, owns a window number /
//! title / decoration flags ([`WindowFlags`]), can hold standard scroll bars,
//! and zooms, moves, resizes, and closes. It sits on the
//! [`Desktop`](crate::desktop::Desktop) and is the base type that
//! [`Dialog`](crate::dialog::Dialog) and editor windows build on.
//!
//! Its colour scheme is recorded as a [`WindowPalette`] (`Blue` for plain
//! windows, `Cyan` and `Gray` for the alternate schemes dialogs use), which the
//! frame turns into the matching [`Role`](crate::theme::Role) styles.
//!
//! **Guide:** [Windows & the desktop](../../../apps/windows.html).
//!
//! # Turbo Vision heritage
//!
//! Ports `TWindow` (`twindow.cpp`). The base-class container becomes an embedded
//! [`Group`] that the [`View`](crate::view::View) trait forwards to (deviation
//! D2); frame title/flags/number are pushed down to the child instead of reached
//! up for (deviation D3); the decoration flag word becomes [`WindowFlags`]
//! (deviation D5); and the palette indirection becomes a [`WindowPalette`] →
//! [`Role`](crate::theme::Role) mapping (deviation D7).

#[allow(clippy::module_inception)]
mod window;

pub use window::{Fullscreen, ScrollBarOptions, Window, WindowFlags, WindowPalette};
